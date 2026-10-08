//! Per-user Windows logon startup registration.
use crate::sys::wide;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, WIN32_ERROR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, REG_SZ,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "Ring Dock";

/// Add or remove Ring Dock from the current user's logon startup entries.
pub fn apply(enabled: bool) -> Result<(), String> {
    let key_path = wide(RUN_KEY);
    let mut key = HKEY::default();
    let open = unsafe { RegCreateKeyW(HKEY_CURRENT_USER, PCWSTR(key_path.as_ptr()), &mut key) };
    check(open, "打开当前用户的开机启动注册表项")?;

    let result = if enabled {
        let executable =
            std::env::current_exe().map_err(|error| format!("读取程序路径失败：{error}"))?;
        let command = wide(&format!("\"{}\"", executable.display()));
        let bytes =
            unsafe { std::slice::from_raw_parts(command.as_ptr().cast::<u8>(), command.len() * 2) };
        unsafe {
            RegSetValueExW(
                key,
                PCWSTR(wide(VALUE_NAME).as_ptr()),
                None,
                REG_SZ,
                Some(bytes),
            )
        }
    } else {
        unsafe { RegDeleteValueW(key, PCWSTR(wide(VALUE_NAME).as_ptr())) }
    };

    let _ = unsafe { RegCloseKey(key) };
    if !enabled && result == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    check(
        result,
        if enabled {
            "写入开机启动项"
        } else {
            "移除开机启动项"
        },
    )
}

fn check(result: WIN32_ERROR, operation: &str) -> Result<(), String> {
    if result.0 == 0 {
        Ok(())
    } else {
        Err(format!("{operation}失败（Windows 错误 {}）", result.0))
    }
}
