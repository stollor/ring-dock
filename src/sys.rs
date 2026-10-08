//! 业务用 Win32 小工具：本地时间 / 日期。
//! 「固定显示在桌面上」相关能力在 `deskpin` crate（零业务、可复用）。
use windows::Win32::Foundation::FILETIME;
use windows::Win32::Foundation::SYSTEMTIME;
use windows::Win32::System::SystemInformation::{
    GetLocalTime, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};
use windows::Win32::System::Threading::GetSystemTimes;

#[derive(Clone, Copy, Debug, Default)]
pub struct ResourceUsage {
    pub cpu: f32,
    pub memory: f32,
}

#[derive(Default)]
pub struct ResourceSampler {
    previous: Option<(u64, u64, u64)>,
}

impl ResourceSampler {
    pub fn reset_cpu_baseline(&mut self) {
        self.previous = None;
    }

    /// CPU is calculated from system time deltas; memory is the system-wide physical-memory load.
    pub fn sample(&mut self) -> ResourceUsage {
        let cpu = unsafe {
            let mut idle = FILETIME::default();
            let mut kernel = FILETIME::default();
            let mut user = FILETIME::default();
            if GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).is_ok() {
                let idle = filetime_value(idle);
                let kernel = filetime_value(kernel);
                let user = filetime_value(user);
                let current = (idle, kernel, user);
                let value = self
                    .previous
                    .map_or(0.0, |(old_idle, old_kernel, old_user)| {
                        let idle_delta = idle.saturating_sub(old_idle);
                        let total_delta = kernel
                            .saturating_sub(old_kernel)
                            .saturating_add(user.saturating_sub(old_user));
                        if total_delta == 0 {
                            0.0
                        } else {
                            (100.0 * (total_delta.saturating_sub(idle_delta) as f64)
                                / total_delta as f64) as f32
                        }
                    });
                self.previous = Some(current);
                value.clamp(0.0, 100.0)
            } else {
                0.0
            }
        };

        let memory = unsafe {
            let mut status = MEMORYSTATUSEX {
                dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
                ..Default::default()
            };
            if GlobalMemoryStatusEx(&mut status).is_ok() {
                status.dwMemoryLoad as f32
            } else {
                0.0
            }
        };
        ResourceUsage { cpu, memory }
    }
}

fn filetime_value(value: FILETIME) -> u64 {
    ((value.dwHighDateTime as u64) << 32) | value.dwLowDateTime as u64
}

/// &str → Vec<u16>（Win32 宽字符，含结尾 NUL）——re-export 自 deskpin
pub use deskpin::wide;

/// 本地时间（时 / 分 / 秒）
pub fn local_hms(_now: i64) -> (i32, i32, i32) {
    let st: SYSTEMTIME = unsafe { GetLocalTime() };
    (st.wHour as i32, st.wMinute as i32, st.wSecond as i32)
}

/// 本地秒数的小数进度（0.0 到 60.0），供平滑秒针动画使用。
pub fn local_second_phase() -> f32 {
    let st: SYSTEMTIME = unsafe { GetLocalTime() };
    st.wSecond as f32 + st.wMilliseconds as f32 / 1000.0
}
