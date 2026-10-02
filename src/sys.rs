//! 业务用 Win32 小工具：本地时间 / 日期。
//! 「固定显示在桌面上」相关能力在 `deskpin` crate（零业务、可复用）；
//! 渲染改为 WS_EX_LAYERED 真透明后，屏幕快照/模糊管线已退役。
use windows::Win32::Foundation::SYSTEMTIME;
use windows::Win32::System::SystemInformation::GetLocalTime;

/// &str → Vec<u16>（Win32 宽字符，含结尾 NUL）——re-export 自 deskpin
pub use deskpin::wide;

/// 本地时间（时 / 分 / 秒）
pub fn local_hms(_now: i64) -> (i32, i32, i32) {
    let st: SYSTEMTIME = unsafe { GetLocalTime() };
    (st.wHour as i32, st.wMinute as i32, st.wSecond as i32)
}

/// 本地日期（月 / 日 / 星期，星期 0 = 周日）——中心时钟区的日期行
pub fn local_date() -> (u32, u32, u32) {
    let st: SYSTEMTIME = unsafe { GetLocalTime() };
    (st.wMonth as u32, st.wDay as u32, st.wDayOfWeek as u32)
}
