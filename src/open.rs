//! 打开收纳条目：程序 / 文件 / 文件夹 / 网址统一走 ShellExecute
use windows::core::PCWSTR;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

pub fn launch(target: &str, args: &str, workdir: &str) -> Result<(), String> {
    if target.is_empty() {
        return Err("目标为空，请检查配置".into());
    }
    unsafe {
        let verb = crate::sys::wide("open");
        let tgt = crate::sys::wide(target);
        let arg = crate::sys::wide(args);
        let dir = crate::sys::wide(workdir);
        let result = ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(tgt.as_ptr()),
            if args.is_empty() {
                PCWSTR::null()
            } else {
                PCWSTR(arg.as_ptr())
            },
            if workdir.is_empty() {
                PCWSTR::null()
            } else {
                PCWSTR(dir.as_ptr())
            },
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
    let _ = launch(&p.to_string_lossy(), "", "");
}
