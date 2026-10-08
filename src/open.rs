//! 打开收纳条目：程序 / 文件 / 文件夹 / 网址统一走 ShellExecute
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

pub fn launch(kind: &str, target: &str) -> Result<(), String> {
    if target.is_empty() {
        return Err("目标为空，请检查配置".into());
    }
    let _ = kind; // url / program / file / folder 统一 open（系统按协议分发）
    unsafe {
        let verb = crate::sys::wide("open");
        let tgt = crate::sys::wide(target);
        let result = ShellExecuteW(
            None,
            windows::core::PCWSTR(verb.as_ptr()),
            windows::core::PCWSTR(tgt.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        );
        if result.0 as isize <= 32 {
            return Err(format!("无法打开目标（错误 {}）", result.0 as isize));
        }
    }
    Ok(())
}

pub fn open_config_file() {
    let p = crate::config::Config::path();
    let _ = launch("file", &p.to_string_lossy());
}
