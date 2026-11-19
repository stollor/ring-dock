//! 把桌面快捷方式解析成它真正指向的目标。
//! 圆环里保存这个目标；桌面上的 `.lnk` / `.url` 之后被移动或删除，也不影响已收纳的条目。
use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::Storage::FileSystem::{GetFullPathNameW, GetLongPathNameW};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED, STGM_READ,
};
use windows::Win32::UI::Shell::{
    IShellLinkW, ShellLink, SLR_NOSEARCH, SLR_NOTRACK, SLR_NOUPDATE, SLR_NO_UI,
};

use crate::config::{Config, Item};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkTarget {
    pub target: String,
    pub args: String,
    pub workdir: String,
}

struct ComInit {
    owned: bool,
}

impl ComInit {
    fn enter() -> Self {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        // S_OK 才是本次初始化。S_FALSE 或线程上已有 OLE 时不能再 CoUninitialize。
        Self { owned: hr.0 == 0 }
    }
}

impl Drop for ComInit {
    fn drop(&mut self) {
        if self.owned {
            unsafe { CoUninitialize() };
        }
    }
}

/// 仅解析快捷方式文件。普通程序、文件和文件夹原样保留，返回 None。
pub fn resolve(path: &str) -> Option<LinkTarget> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".url") || lower.ends_with(".website") {
        return resolve_url_file(path);
    }
    if !lower.ends_with(".lnk") {
        return None;
    }
    let _com = ComInit::enter();
    resolve_lnk(path, 0)
}

/// 把仍指向快捷方式文件的条目改写成真实目标。显示名保持不变。
pub fn materialize_item(item: &mut Item) -> bool {
    let Some(link) = resolve(&item.target) else {
        return false;
    };
    let kind = classify(&link.target);
    if link.target.eq_ignore_ascii_case(&item.target)
        && link.args == item.args
        && link.workdir.eq_ignore_ascii_case(&item.workdir)
        && item.kind == kind
    {
        return false;
    }
    item.kind = kind.to_string();
    item.target = link.target;
    item.args = link.args;
    item.workdir = link.workdir;
    true
}

/// 改写配置中已有的快捷方式条目。返回是否发生了变化。
pub fn retarget_config(cfg: &mut Config) -> bool {
    let mut changed = false;
    for quadrant in &mut cfg.quadrants {
        for item in &mut quadrant.items {
            changed |= materialize_item(item);
        }
    }
    changed
}

pub(crate) fn classify(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if is_url(&lower) || lower.ends_with(".url") || lower.ends_with(".website") {
        return "url";
    }
    if lower.ends_with(".lnk")
        || lower.ends_with(".exe")
        || lower.ends_with(".bat")
        || lower.ends_with(".cmd")
    {
        return "program";
    }
    if std::path::Path::new(path).is_dir() {
        return "folder";
    }
    "file"
}

fn is_url(lower: &str) -> bool {
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return true;
    }
    match lower.find(':') {
        Some(index) if index > 1 => {
            let rest = &lower[index + 1..];
            !rest.starts_with('\\') && !rest.starts_with('/')
        }
        _ => false,
    }
}

fn resolve_lnk(path: &str, depth: u8) -> Option<LinkTarget> {
    if depth > 4 {
        return None;
    }
    let link = read_shell_link(path)?;
    if link.target.is_empty() {
        return None;
    }
    if link.target.to_ascii_lowercase().ends_with(".lnk") && !link.target.eq_ignore_ascii_case(path)
    {
        if let Some(next) = resolve_lnk(&link.target, depth + 1) {
            return Some(LinkTarget {
                target: next.target,
                args: if next.args.is_empty() {
                    link.args
                } else {
                    next.args
                },
                workdir: if next.workdir.is_empty() {
                    link.workdir
                } else {
                    next.workdir
                },
            });
        }
    }
    Some(link)
}

fn read_shell_link(path: &str) -> Option<LinkTarget> {
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let persist: IPersistFile = link.cast().ok()?;
        let path_w = crate::sys::wide(path);
        persist.Load(PCWSTR(path_w.as_ptr()), STGM_READ).ok()?;
        let flags = (SLR_NO_UI.0 | SLR_NOUPDATE.0 | SLR_NOSEARCH.0 | SLR_NOTRACK.0) as u32;
        let _ = link.Resolve(HWND::default(), flags);

        let mut target_buf = vec![0u16; 32768];
        let mut find_data = windows::Win32::Storage::FileSystem::WIN32_FIND_DATAW::default();
        link.GetPath(&mut target_buf, &mut find_data, 0).ok()?;
        let target = utf16_string(&target_buf);
        if target.is_empty() {
            return None;
        }

        let mut args_buf = vec![0u16; 32768];
        let args = if link.GetArguments(&mut args_buf).is_ok() {
            utf16_string(&args_buf)
        } else {
            String::new()
        };
        let mut dir_buf = vec![0u16; 32768];
        let workdir = if link.GetWorkingDirectory(&mut dir_buf).is_ok() {
            utf16_string(&dir_buf)
        } else {
            String::new()
        };
        Some(LinkTarget {
            target: qualify(&target, &workdir, path),
            args,
            workdir: qualify_directory(&workdir),
        })
    }
}

fn resolve_url_file(path: &str) -> Option<LinkTarget> {
    let text = read_shortcut_text(path)?;
    for line in text.lines() {
        let line = line.trim();
        let Some(url) = line
            .get(4..)
            .filter(|_| line.len() >= 4 && line[..4].eq_ignore_ascii_case("URL="))
        else {
            continue;
        };
        let url = url.trim();
        if !url.is_empty() {
            return Some(LinkTarget {
                target: url.to_string(),
                args: String::new(),
                workdir: String::new(),
            });
        }
    }
    None
}

fn read_shortcut_text(path: &str) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        return Some(String::from_utf16_lossy(&units));
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
            .collect();
        return Some(String::from_utf16_lossy(&units));
    }
    let start = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    };
    Some(String::from_utf8_lossy(&bytes[start..]).into_owned())
}

fn qualify(path: &str, workdir: &str, link_file: &str) -> String {
    if path.is_empty() || is_url(&path.to_lowercase()) {
        return path.to_string();
    }
    let raw = std::path::Path::new(path);
    let joined = if raw.is_absolute() {
        path.to_string()
    } else {
        let base = if !workdir.is_empty() {
            std::path::PathBuf::from(workdir)
        } else {
            std::path::Path::new(link_file)
                .parent()
                .map(|parent| parent.to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("."))
        };
        base.join(raw).to_string_lossy().into_owned()
    };
    full_path(&joined).unwrap_or(joined)
}

fn qualify_directory(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    full_path(path).unwrap_or_else(|| path.to_string())
}

fn full_path(path: &str) -> Option<String> {
    let wide = crate::sys::wide(path);
    let mut buf = vec![0u16; 32768];
    let length = unsafe { GetFullPathNameW(PCWSTR(wide.as_ptr()), Some(&mut buf), None) };
    if length == 0 || length as usize >= buf.len() {
        return None;
    }
    let full = String::from_utf16_lossy(&buf[..length as usize]);
    Some(long_path(&full).unwrap_or(full))
}

fn long_path(path: &str) -> Option<String> {
    let wide = crate::sys::wide(path);
    let mut buf = vec![0u16; 32768];
    let length = unsafe { GetLongPathNameW(PCWSTR(wide.as_ptr()), Some(&mut buf)) };
    if length == 0 || length as usize >= buf.len() {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..length as usize]))
}

fn utf16_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&unit| unit == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "ring-dock-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write_lnk(path: &Path, target: &str, args: &str, workdir: &str) {
        let _com = ComInit::enter();
        unsafe {
            let link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
            let target_w = crate::sys::wide(target);
            let args_w = crate::sys::wide(args);
            let dir_w = crate::sys::wide(workdir);
            link.SetPath(PCWSTR(target_w.as_ptr())).unwrap();
            link.SetArguments(PCWSTR(args_w.as_ptr())).unwrap();
            link.SetWorkingDirectory(PCWSTR(dir_w.as_ptr())).unwrap();
            let persist: IPersistFile = link.cast().unwrap();
            let path_w = crate::sys::wide(&path.to_string_lossy());
            persist.Save(PCWSTR(path_w.as_ptr()), true).unwrap();
        }
    }

    #[test]
    fn shortcut_resolves_to_real_target_without_keeping_the_lnk_path() {
        let dir = TempDir::new("lnk");
        let notepad = PathBuf::from(std::env::var("WINDIR").unwrap()).join(r"System32\notepad.exe");
        let lnk = dir.0.join("记事本.lnk");
        write_lnk(
            &lnk,
            &notepad.to_string_lossy(),
            "/fixture",
            &dir.0.to_string_lossy(),
        );

        let resolved = resolve(&lnk.to_string_lossy()).expect("lnk target");
        assert!(resolved
            .target
            .eq_ignore_ascii_case(&notepad.to_string_lossy()));
        assert!(!resolved.target.to_ascii_lowercase().ends_with(".lnk"));
        assert_eq!(resolved.args, "/fixture");
        let workdir = full_path(&dir.0.to_string_lossy())
            .unwrap_or_else(|| dir.0.to_string_lossy().into_owned());
        assert!(
            resolved.workdir.eq_ignore_ascii_case(&workdir),
            "got {:?} expected {:?}",
            resolved.workdir,
            workdir
        );

        let mut cfg = Config::default();
        let before = cfg.quadrants[0].items.len();
        assert_eq!(
            crate::drop::add_items(&mut cfg, 0, &[lnk.to_string_lossy().into_owned()]),
            1
        );
        let item = cfg.quadrants[0].items.last().unwrap();
        assert_eq!(item.name, "记事本");
        assert!(item.target.eq_ignore_ascii_case(&resolved.target));
        assert_eq!(item.args, "/fixture");
        assert_eq!(item.kind, "program");
        assert_eq!(cfg.quadrants[0].items.len(), before + 1);
        assert_eq!(
            crate::drop::add_items(&mut cfg, 0, &[lnk.to_string_lossy().into_owned()]),
            0
        );

        let other = dir.0.join("另一个.lnk");
        write_lnk(
            &other,
            &notepad.to_string_lossy(),
            "/other",
            &dir.0.to_string_lossy(),
        );
        assert_eq!(
            crate::drop::add_items(&mut cfg, 0, &[other.to_string_lossy().into_owned()]),
            1
        );
    }

    #[test]
    fn existing_shortcut_entries_are_rewritten_once() {
        let dir = TempDir::new("retarget");
        let folder = dir.0.join("目录");
        std::fs::create_dir_all(&folder).unwrap();
        let lnk = dir.0.join("目录.lnk");
        write_lnk(&lnk, &folder.to_string_lossy(), "", "");

        let mut cfg = Config::default();
        cfg.quadrants[2].items.push(Item {
            name: "我的目录".into(),
            kind: "program".into(),
            target: lnk.to_string_lossy().into_owned(),
            ..Default::default()
        });
        assert!(retarget_config(&mut cfg));
        let item = cfg.quadrants[2].items.last().unwrap();
        assert_eq!(item.name, "我的目录");
        let expected = full_path(&folder.to_string_lossy())
            .unwrap_or_else(|| folder.to_string_lossy().into_owned());
        assert!(
            item.target.eq_ignore_ascii_case(&expected),
            "got {:?} expected {:?}",
            item.target,
            expected
        );
        assert_eq!(item.kind, "folder");
        assert!(!retarget_config(&mut cfg));
    }

    #[test]
    fn missing_shortcut_and_plain_files_stay_unchanged() {
        assert!(resolve(r"C:\missing\no-such.lnk").is_none());
        assert!(resolve(r"C:\Windows\System32\notepad.exe").is_none());
        let mut cfg = Config::default();
        let original = cfg.quadrants[0].items[0].target.clone();
        assert!(!retarget_config(&mut cfg));
        assert_eq!(cfg.quadrants[0].items[0].target, original);
    }

    #[test]
    fn internet_shortcut_resolves_to_its_url() {
        let dir = TempDir::new("url");
        let path = dir.0.join("站点.url");
        std::fs::write(
            &path,
            "[InternetShortcut]\r\nURL=https://example.invalid/never-opened\r\n",
        )
        .unwrap();
        let resolved = resolve(&path.to_string_lossy()).unwrap();
        assert_eq!(resolved.target, "https://example.invalid/never-opened");
        assert_eq!(classify(&resolved.target), "url");
    }
}
