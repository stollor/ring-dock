//! 打开收纳条目：程序 / 文件 / 文件夹 / 网址统一走 ShellExecute
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

pub fn launch(kind: &str, target: &str) {
    if target.is_empty() {
        return;
    }
    let _ = kind; // url / program / file / folder 统一 open（系统按协议分发）
    unsafe {
        let verb = crate::sys::wide("open");
        let tgt = crate::sys::wide(target);
        ShellExecuteW(
            None,
            windows::core::PCWSTR(verb.as_ptr()),
            windows::core::PCWSTR(tgt.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}

pub fn open_config_file() {
    let p = crate::config::Config::path();
    launch("file", &p.to_string_lossy());
}
