//! 系统托盘图标（业务侧）：常驻托盘、点击弹菜单、Explorer 重启后自动重建
use crate::sys::wide;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    LoadIconW, RegisterWindowMessageW, HICON, IDI_APPLICATION, WM_APP,
};
use windows::core::PCWSTR;

/// 托盘回调消息（发给宿主窗口；lparam 低字 = 鼠标消息）
pub const WM_TRAY: u32 = WM_APP + 1;

/// 托盘图标生命周期管理
pub struct Tray {
    data: NOTIFYICONDATAW,
    added: bool,
}

impl Tray {
    /// 添加托盘图标：tip = 悬浮提示，icon_res = exe 内嵌图标资源 ID（winres 默认 1）
    pub fn add(hwnd: HWND, tip: &str, icon_res: u16) -> Self {
        let mut tip_buf = [0u16; 128];
        for (dst, src) in tip_buf.iter_mut().zip(wide(tip).iter()) {
            *dst = *src;
        }
        let data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: load_icon(icon_res),
            szTip: tip_buf,
            ..Default::default()
        };
        let added = unsafe { Shell_NotifyIconW(NIM_ADD, &data).as_bool() };
        if !added {
            eprintln!("[ring-dock] 托盘图标添加失败");
        }
        Self { data, added }
    }

    /// Explorer 重启（TaskbarCreated 广播）后图标会被清掉：换绑新窗口并重建
    pub fn readd(&mut self, hwnd: HWND) {
        self.remove();
        self.data.hWnd = hwnd;
        self.added = unsafe { Shell_NotifyIconW(NIM_ADD, &self.data).as_bool() };
    }

    /// 移除托盘图标（进程退出前调用）
    pub fn remove(&mut self) {
        if self.added {
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &self.data);
            }
            self.added = false;
        }
    }
}

/// 注册 "TaskbarCreated" 广播消息 ID（Explorer 重启后用它触发托盘重建）
pub fn taskbar_created_msg() -> u32 {
    unsafe { RegisterWindowMessageW(PCWSTR(wide("TaskbarCreated").as_ptr())) }
}

/// 从 exe 资源加载图标；失败回退系统默认应用图标
fn load_icon(res_id: u16) -> HICON {
    unsafe {
        // MAKEINTRESOURCE：资源 ID 直接当指针值用
        let as_res = PCWSTR(res_id as usize as *const u16);
        LoadIconW(Some(deskpin::module_handle()), as_res)
            .or_else(|_| LoadIconW(None, IDI_APPLICATION))
            .unwrap_or_default()
    }
}
