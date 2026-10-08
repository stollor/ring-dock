//! Safe, on-demand cleanup helpers for the center action.
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows::core::{GUID, PCWSTR};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

const TEMP_FILE_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;

#[derive(Default)]
pub struct TempCleanupSummary {
    pub files_removed: u64,
    pub bytes_removed: u64,
}

/// Locate the official WinMemoryCleaner executable without requiring a fixed install method.
pub fn find_win_memory_cleaner(configured_path: Option<&str>) -> Result<PathBuf, String> {
    if let Some(configured_path) = configured_path.filter(|path| !path.trim().is_empty()) {
        return configured_executable(configured_path);
    }
    if let Some(path) = std::env::var_os("RING_DOCK_WINMEMORYCLEANER") {
        return configured_executable(&path.to_string_lossy());
    }

    let executable_name = "WinMemoryCleaner.exe";
    if let Ok(executable) = std::env::current_exe() {
        if let Some(directory) = executable.parent() {
            let candidate = directory.join(executable_name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let candidate = directory.join(executable_name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    let mut candidates = Vec::new();
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        let local_app_data = PathBuf::from(local_app_data);
        candidates.extend([
            local_app_data.join("RingDock").join(executable_name),
            local_app_data
                .join(r"Microsoft\WinGet\Links")
                .join(executable_name),
            local_app_data
                .join(r"Programs\Windows Memory Cleaner")
                .join(executable_name),
            local_app_data
                .join(r"scoop\apps\winmemorycleaner\current")
                .join(executable_name),
        ]);
        candidates.extend(winget_package_candidates(&local_app_data));
    }
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        candidates.push(
            PathBuf::from(program_files)
                .join("Windows Memory Cleaner")
                .join(executable_name),
        );
    }
    if let Some(program_files_x86) = std::env::var_os("ProgramFiles(x86)") {
        candidates.push(
            PathBuf::from(program_files_x86)
                .join("Windows Memory Cleaner")
                .join(executable_name),
        );
    }

    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            "未找到 WinMemoryCleaner.exe；请安装 WinMemoryCleaner，或在 config.json 设置 win_memory_cleaner_path".into()
        })
}

fn configured_executable(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path.trim());
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join(path)))
            .unwrap_or_else(|| path.to_path_buf())
    };
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!("WinMemoryCleaner 路径无效：{}", path.display()))
    }
}

fn winget_package_candidates(local_app_data: &Path) -> Vec<PathBuf> {
    let packages = local_app_data.join(r"Microsoft\WinGet\Packages");
    let Ok(entries) = fs::read_dir(packages) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for entry in entries.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with("IgorMundstein.WinMemoryCleaner_")
        {
            continue;
        }
        let root = entry.path();
        candidates.push(root.join("WinMemoryCleaner.exe"));
        if let Ok(children) = fs::read_dir(root) {
            candidates.extend(
                children
                    .flatten()
                    .map(|child| child.path().join("WinMemoryCleaner.exe")),
            );
        }
    }
    candidates
}

/// Start a narrowly scoped elevated broker once for this ring-dock session.
/// The dock remains unelevated and talks to the broker over an authenticated loopback socket.
pub fn start_memory_broker(executable: &Path) -> Result<Arc<Mutex<TcpStream>>, String> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or("无法定位当前用户的 AppData 目录")?;
    let broker_directory = local_app_data.join("RingDock");
    fs::create_dir_all(&broker_directory).map_err(|error| format!("无法创建助手目录：{error}"))?;
    let broker_executable = broker_directory.join("ring-dock-broker.exe");
    let current_executable = std::env::current_exe().map_err(|error| error.to_string())?;
    fs::copy(&current_executable, &broker_executable)
        .map_err(|error| format!("无法准备内存清理助手：{error}"))?;

    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .map_err(|error| format!("无法创建本机清理通道：{error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("无法配置本机清理通道：{error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    let token = format!("{:?}", GUID::new().map_err(|error| error.to_string())?);

    let windows_directory = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let system_directory = windows_directory.join("System32");
    let command_interpreter = system_directory.join("cmd.exe");
    let verb = crate::sys::wide("runas");
    let exe = crate::sys::wide(&command_interpreter.to_string_lossy());
    let command = format!(
        "/d /c \"\"{}\" --memory-cleanup-broker {port} {token}\"",
        broker_executable.display()
    );
    let args = crate::sys::wide(&command);
    let working_directory = crate::sys::wide(&system_directory.to_string_lossy());
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(exe.as_ptr()),
            PCWSTR(args.as_ptr()),
            PCWSTR(working_directory.as_ptr()),
            SW_HIDE,
        )
    };
    // ShellExecute may return ERROR_CANCELLED (1223) when the user declines UAC;
    // unlike SE_ERR_* values, this is greater than the usual success threshold.
    if result.0 as isize == 1223 {
        return Err("管理员授权已取消，内存整理助手未启动".into());
    }
    if result.0 as isize <= 32 {
        let code = result.0 as isize;
        if code == 5 {
            return Err("Windows 拒绝启动内存整理助手".into());
        }
        return Err(format!("无法启动内存整理助手（ShellExecute 错误 {code}）"));
    }

    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .map_err(|error| error.to_string())?;
                let mut reader =
                    BufReader::new(stream.try_clone().map_err(|error| error.to_string())?);
                let mut hello = String::new();
                reader
                    .read_line(&mut hello)
                    .map_err(|error| format!("管理员助手握手失败：{error}"))?;
                if hello.trim() != format!("HELLO {token}") {
                    return Err("管理员助手身份校验失败".into());
                }
                writeln!(stream, "READY").map_err(|error| error.to_string())?;
                writeln!(stream, "WMC {}", executable.display())
                    .map_err(|error| error.to_string())?;
                stream.flush().map_err(|error| error.to_string())?;
                return Ok(Arc::new(Mutex::new(stream)));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err("等待管理员助手超时".into());
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(format!("等待管理员助手失败：{error}")),
        }
    }
}

/// In the elevated helper process, wait on the parent's authenticated loopback connection
/// and run only the requested low-priority standby-list cleanup.
pub fn run_memory_broker_mode() -> Option<Result<(), String>> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("--memory-cleanup-broker") {
        return None;
    }
    let port = args.next().and_then(|value| value.parse::<u16>().ok());
    let token = args.next();
    Some(match (port, token) {
        (Some(port), Some(token)) => run_memory_broker(port, &token),
        _ => Err("管理员助手启动参数无效".into()),
    })
}

fn run_memory_broker(port: u16, token: &str) -> Result<(), String> {
    let address = (Ipv4Addr::LOCALHOST, port);
    let mut stream = TcpStream::connect_timeout(
        &std::net::SocketAddr::from(address),
        Duration::from_secs(30),
    )
    .map_err(|error| format!("无法连接 ring-dock：{error}"))?;
    writeln!(stream, "HELLO {token}").map_err(|error| error.to_string())?;
    stream.flush().map_err(|error| error.to_string())?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|error| error.to_string())?);
    let mut ready = String::new();
    reader
        .read_line(&mut ready)
        .map_err(|error| format!("ring-dock 握手失败：{error}"))?;
    if ready.trim() != "READY" {
        return Err("ring-dock 未确认管理员助手".into());
    }
    let mut executable_line = String::new();
    reader
        .read_line(&mut executable_line)
        .map_err(|error| format!("无法接收 WinMemoryCleaner 路径：{error}"))?;
    let executable = executable_line
        .strip_prefix("WMC ")
        .map(str::trim_end)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or("WinMemoryCleaner 路径无效")?;

    loop {
        let mut request = String::new();
        match reader.read_line(&mut request) {
            Ok(0) => return Ok(()),
            Ok(_) if request.trim() == "CLEAN" => {
                let response = match Command::new(&executable)
                    .arg("/StandbyListLowPriority")
                    .status()
                {
                    Ok(status) if status.success() => "OK".to_string(),
                    Ok(status) => format!(
                        "ERR WinMemoryCleaner 退出码 {}",
                        status.code().unwrap_or(-1)
                    ),
                    Err(error) => format!("ERR 无法启动 WinMemoryCleaner：{error}"),
                };
                writeln!(stream, "{response}").map_err(|error| error.to_string())?;
                stream.flush().map_err(|error| error.to_string())?;
            }
            Ok(_) => {
                writeln!(stream, "ERR 未知清理请求").map_err(|error| error.to_string())?;
                stream.flush().map_err(|error| error.to_string())?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                return Err("ring-dock 已关闭连接".into());
            }
            Err(error) => return Err(format!("管理员助手通信失败：{error}")),
        }
    }
}

pub fn request_memory_cleanup(broker: &Arc<Mutex<TcpStream>>) -> Result<String, String> {
    let mut stream = broker
        .lock()
        .map_err(|_| "内存整理助手连接已损坏".to_string())?;
    writeln!(&mut *stream, "CLEAN").map_err(|error| format!("无法联系内存整理助手：{error}"))?;
    stream.flush().map_err(|error| error.to_string())?;
    let mut response = String::new();
    BufReader::new(
        stream
            .try_clone()
            .map_err(|error| format!("无法读取内存整理结果：{error}"))?,
    )
    .read_line(&mut response)
    .map_err(|error| format!("读取内存整理结果失败：{error}"))?;
    match response.trim() {
        "OK" => Ok("低优先级待机内存整理完成".into()),
        response => Err(response
            .strip_prefix("ERR ")
            .unwrap_or(response)
            .to_string()),
    }
}

/// Remove only old files under the current user's TEMP directory. Reparse points and files in
/// active use are skipped; empty child directories are removed after their old files are gone.
pub fn clean_old_user_temp_files() -> Result<TempCleanupSummary, String> {
    let root = std::env::temp_dir();
    let root_metadata = fs::symlink_metadata(&root)
        .map_err(|error| format!("无法访问当前用户临时目录：{error}"))?;
    if !root_metadata.is_dir() || is_reparse_point(&root_metadata) {
        return Err("当前用户临时目录不是普通目录，已停止清理".into());
    }

    let cutoff = SystemTime::now()
        .checked_sub(TEMP_FILE_MAX_AGE)
        .unwrap_or(UNIX_EPOCH);
    let mut summary = TempCleanupSummary::default();
    let mut pending = vec![root];
    let mut directories = Vec::new();

    while let Some(directory) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) if !is_reparse_point(&metadata) => metadata,
                _ => continue,
            };
            if metadata.is_dir() {
                directories.push(path.clone());
                pending.push(path);
            } else if metadata.is_file()
                && metadata.modified().is_ok_and(|modified| modified <= cutoff)
            {
                let length = metadata.len();
                if fs::remove_file(&path).is_ok() {
                    summary.files_removed += 1;
                    summary.bytes_removed = summary.bytes_removed.saturating_add(length);
                }
            }
        }
    }

    for directory in directories.into_iter().rev() {
        let _ = fs::remove_dir(directory);
    }
    Ok(summary)
}

fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}
