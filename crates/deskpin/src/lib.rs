//! # deskpin —— Windows 桌面挂件「固定显示在桌面上」工具箱（无业务）
//!
//! 解决桌面挂件（贴图 / 时钟 / 圆环收纳 …）的两个刚需：
//! 1. **不遮挡别的窗口**：挂在所有应用窗口之下；
//! 2. **不受最小化影响**：Win+D「显示桌面」/「最小化所有」/ 任务栏右键最小化都不带走它。
//!
//! ## 原理（调研结论）
//!
//! Windows 没有任何样式位能让**顶层窗口**同时满足上述两点：
//! `WS_EX_TOPMOST` 置顶会盖住所有应用、且 Win+D 照样把它最小化；
//! Rainmeter 一类方案靠 `EVENT_SYSTEM_FOREGROUND` 钩子 + 定时器持续压 Z 序（会偶发失效）。
//!
//! 动态壁纸 / 桌面贴图类工具（Lively Wallpaper、AutoIt 贴图脚本、Electron 桌面挂件）
//! 的通行做法是 **`SetParent` 挂进桌面窗口树**，让挂件成为桌面子窗口：
//! - 天然位于所有应用之下；
//! - 不属于顶层窗口，Win+D / 最小化所有只作用于 Progman 之外的普通应用窗口。
//!
//! ## 用法（业务侧只需三步）
//!
//! ```ignore
//! // 1) 进程启动时启用 DPI 感知（物理像素坐标）
//! deskpin::enable_dpi_awareness();
//!
//! // 2) 创建普通 WS_POPUP 窗口后挂载（幂等；失败自动退化为顶层压底并可稍后重试）
//! let mut pinner = deskpin::Pinner::new();
//! pinner.attach(hwnd);
//!
//! // 3) 在自己的定时器里低频维护（建议 5~10 秒一次：挂载自愈 + Z 序拉回兄弟栈顶）
//! pinner.maintain(hwnd);
//! ```
//!
//! ## 业务侧需要处理的一件事
//!
//! Explorer 重启 / 切换壁纸会**连带销毁**桌面子窗口。业务方应在 `WM_DESTROY`
//! （非用户主动退出时）**重建窗口**再 `attach`，而不是退出进程：
//!
//! ```ignore
//! WM_DESTROY => {
//!     if !quitting {
//!         hwnd = create_window();     // 业务重建
//!         pinner.attach(hwnd);        // 重新挂载
//!     } else {
//!         PostQuitMessage(0);
//!     }
//! }
//! ```
//!
//! ## 挂载点降级链（`find_desktop_host`）
//!
//! ```text
//! SysListView32（桌面图标列表，圆环显示在图标之上）
//!   → SHELLDLL_DefView（图标层）
//!   → Progman 子级 WorkerW（图标之下、壁纸之上，Win11 24H2 起的层级）
//!   → 顶层 WorkerW（Win10 壁纸层，0x052C 触发创建）
//!   → Progman（兜底）
//! ```
//!
//! 全部失败时退化为**顶层窗口压底 `HWND_BOTTOM`**（仍不遮挡应用；此档位才可能被
//! Win+D 带走），并在下次 `maintain` 时自动重试嵌入。
//!
//! ## 已知边界
//!
//! - Win11 若在「系统属性 → 性能选项」关闭了「动画控件和元素」，Explorer 不创建
//!   WorkerW（仅影响「图标之下」档位；图标层档位不受影响）。
//! - `work_area` 取主屏工作区；多屏需求可自行扩展。
//! - 挂件若需要鼠标滚轮：桌面子窗口无焦点，依赖系统「悬停时滚动非活动窗口」
//!   （Win10/11 默认开启）。

use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, FindWindowExW, GetClassNameW, GetParent, GetShellWindow,
    GetWindowLongPtrW, IsWindow, SendMessageTimeoutW, SetParent, SetWindowLongPtrW, SetWindowPos,
    SystemParametersInfoW, GWL_STYLE, HWND_BOTTOM, SMTO_NORMAL, SPI_GETWORKAREA, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    WS_CHILD, WS_CLIPCHILDREN, WS_POPUP,
};
use windows::core::{BOOL, PCWSTR};

/* ============================ 通用小工具 ============================ */

/// &str → Vec<u16>（Win32 宽字符，含结尾 NUL）
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 进程级 DPI 感知（Per-Monitor V2，物理像素坐标）
pub fn enable_dpi_awareness() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

/// 当前模块句柄（窗口类注册用）
pub fn module_handle() -> windows::Win32::Foundation::HINSTANCE {
    unsafe {
        let m = GetModuleHandleW(None).unwrap_or_default();
        windows::Win32::Foundation::HINSTANCE(m.0)
    }
}

/// 主屏工作区矩形 (x, y, w, h)；物理像素。挂件窗口通常铺满此区域。
pub fn work_area() -> (i32, i32, i32, i32) {
    let mut rc = RECT::default();
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut rc as *mut _ as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    (rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top)
}

/// 把窗口摆到主屏工作区（已挂进桌面树时自动换算为父客户区坐标）
pub fn place_workarea(hwnd: HWND) {
    let (x, y, w, h) = work_area();
    let (mut px, mut py) = (x, y);
    unsafe {
        if let Ok(p) = GetParent(hwnd) {
            if !p.is_invalid() {
                let mut pt = POINT { x, y };
                let _ = ScreenToClient(p, &mut pt);
                px = pt.x;
                py = pt.y;
            }
        }
        let _ = SetWindowPos(hwnd, None, px, py, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

/* ============================ 桌面树嵌入 ============================ */

/// 触发 Explorer 创建 WorkerW 的私有消息（Win10 壁纸层；Win11 24H2 起层级收进 Progman）
const WM_SPAWN_WORKERW: u32 = 0x052C;

/// 把一块屏幕区域「交还」给桌面重绘（挂件自擦保险）。
///
/// 桌面子窗口的通病：`SetWindowRgn` 缩小（如收起面板）后，暴露出来的像素**没有人自动擦**——
/// Explorer 的桌面平时静态不重绘，残影会一直留在屏幕上。调用本函数让桌面树
///（壁纸 + 图标层 + 其子窗口）重绘该矩形，露出真实桌面。参数为屏幕坐标矩形（物理像素）。
pub fn repaint_desktop_area(rect_screen: RECT) {
    use windows::Win32::Graphics::Gdi::{RedrawWindow, RDW_ALLCHILDREN, RDW_INVALIDATE};
    unsafe {
        let shell = GetShellWindow(); // Progman（桌面树根）
        if shell.is_invalid() {
            return;
        }
        // RedrawWindow 的矩形是目标窗口客户区坐标
        let mut pt = POINT {
            x: rect_screen.left,
            y: rect_screen.top,
        };
        let _ = ScreenToClient(shell, &mut pt);
        let rc = RECT {
            left: pt.x,
            top: pt.y,
            right: pt.x + (rect_screen.right - rect_screen.left),
            bottom: pt.y + (rect_screen.bottom - rect_screen.top),
        };
        // 只重绘该矩形内的桌面树（壁纸 + 图标 + 挂件自身）。注意：
        // **不加 RDW_UPDATENOW**——同步重绘是跨进程等待 Explorer，而 Explorer 重绘
        // 桌面树时又可能等待本窗口的 WM_PAINT，存在阻塞/死锁风险；异步请求即可，
        // 视觉正确性由 blit_snapshot 保证，这里只是「请桌面换回真身」的尽力而为。
        let _ = RedrawWindow(Some(shell), Some(&rc), None, RDW_INVALIDATE | RDW_ALLCHILDREN);
    }
}

/// 挂载方式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinMode {
    /// 已挂进桌面窗口树（推荐档）：不遮挡应用、不受 Win+D / 最小化影响
    Embedded,
    /// 降级档：顶层窗口压底 Z 序（仍不遮挡应用，但可能被 Win+D 带走）
    BottomFallback,
}

/// 桌面固定显示管理器：挂载 + 周期自愈（无业务、无窗口创建逻辑）
#[derive(Default)]
pub struct Pinner {
    host: Option<HWND>,
    mode: Option<PinMode>,
}

impl Pinner {
    pub fn new() -> Self {
        Self::default()
    }

    /// 当前挂载方式（尚未 attach 时为 None）
    pub fn mode(&self) -> Option<PinMode> {
        self.mode
    }

    /// 当前宿主窗口（仅 Embedded 档有意义）
    pub fn host(&self) -> Option<HWND> {
        self.host
    }

    /// 幂等挂载：已挂进桌面树则不动；否则尝试嵌入，失败退化为顶层压底
    pub fn attach(&mut self, hwnd: HWND) -> PinMode {
        if is_embedded(hwnd) {
            self.mode = Some(PinMode::Embedded);
            return PinMode::Embedded;
        }
        if let Some(host) = embed_to_desktop(hwnd) {
            self.host = Some(host);
            self.mode = Some(PinMode::Embedded);
            return PinMode::Embedded;
        }
        // 顶层兜底档：摆好位置并压到 Z 序底部（应用窗口之下）
        place_workarea(hwnd);
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_BOTTOM),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        self.host = None;
        self.mode = Some(PinMode::BottomFallback);
        PinMode::BottomFallback
    }

    /// 周期维护（建议 5~10 秒一次）：挂载自愈 + Z 序拉回同父兄弟栈顶。
    /// 典型场景：挂载曾经失败、或宿主被重建后自动重挂。
    pub fn maintain(&mut self, hwnd: HWND) -> PinMode {
        if is_embedded(hwnd) {
            // 偶发被父窗口重排沉底 → 拉回兄弟栈顶（仅桌面层内部，不会盖到应用窗口）
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
            self.mode = Some(PinMode::Embedded);
            return PinMode::Embedded;
        }
        self.attach(hwnd)
    }
}

/// 是否已挂进桌面窗口树（宿主仍存活）
pub fn is_embedded(hwnd: HWND) -> bool {
    unsafe {
        match GetParent(hwnd) {
            Ok(p) => !p.is_invalid() && IsWindow(Some(p)).as_bool(),
            Err(_) => false,
        }
    }
}

/// 按降级链找桌面挂载点（见文件头注释）
pub fn find_desktop_host() -> Option<HWND> {
    let cls_lv = wide("SysListView32");
    let cls_workerw = wide("WorkerW");
    unsafe {
        let shell = GetShellWindow(); // Progman
        if shell.is_invalid() {
            return None;
        }
        // 触发 Explorer 创建 WorkerW（个别环境关掉「窗口动画」时不会创建；失败不影响图标层档位）
        let _ = SendMessageTimeoutW(
            shell,
            WM_SPAWN_WORKERW,
            WPARAM(0),
            LPARAM(0),
            SMTO_NORMAL,
            1000,
            None,
        );
        // 1/2. 图标层（挂进图标列表 → 挂件显示在图标之上）
        if let Some(defview) = find_defview(shell) {
            let lv = FindWindowExW(
                Some(defview),
                None::<HWND>,
                PCWSTR(cls_lv.as_ptr()),
                PCWSTR::null(),
            )
            .unwrap_or_default();
            return Some(if lv.is_invalid() { defview } else { lv });
        }
        // 3. Progman 子级 WorkerW（Win11 24H2：图标之下、壁纸之上）
        let w = FindWindowExW(
            Some(shell),
            None::<HWND>,
            PCWSTR(cls_workerw.as_ptr()),
            PCWSTR::null(),
        )
        .unwrap_or_default();
        if !w.is_invalid() {
            return Some(w);
        }
        // 4. 顶层壁纸层 WorkerW（Win10 经典布局）；5. 兜底 Progman
        find_wallpaper_workerw().or(Some(shell))
    }
}

/// 挂进桌面窗口树（幂等）：成功返回宿主窗口；失败返回 None（保持顶层，稍后重试）
fn embed_to_desktop(hwnd: HWND) -> Option<HWND> {
    let host = find_desktop_host()?;
    unsafe {
        // WS_POPUP → WS_CHILD：挂进桌面树必须是子窗口（经典约束），失败要还原
        let revert = |keep_child: bool| {
            let s = GetWindowLongPtrW(hwnd, GWL_STYLE);
            let s = if keep_child {
                (s & !(WS_POPUP.0 as isize)) | (WS_CHILD.0 as isize)
            } else {
                (s & !(WS_CHILD.0 as isize)) | (WS_POPUP.0 as isize)
            };
            SetWindowLongPtrW(hwnd, GWL_STYLE, s);
        };
        revert(true);
        // 父链 WS_CLIPCHILDREN：祖先自绘时不覆盖/擦除子窗口区域（否则挂件被图标层重绘吃掉）
        let mut p = Some(host);
        while let Some(h) = p {
            if h.is_invalid() {
                break;
            }
            let s = GetWindowLongPtrW(h, GWL_STYLE);
            SetWindowLongPtrW(h, GWL_STYLE, s | (WS_CLIPCHILDREN.0 as isize));
            p = GetParent(h).ok();
        }
        let _ = SetParent(hwnd, Some(host));
        // SetParent 返回值在「此前无父」时与失败难以区分，统一回读父窗口验证
        let ok = matches!(GetParent(hwnd), Ok(p) if p == host);
        if !ok {
            revert(false);
            return None;
        }
        // SetParent 不换算坐标：摆到主屏工作区（父客户区坐标）
        place_workarea(hwnd);
        // 子窗口 Z 序置顶 → 在图标内容之上（仅限同父兄弟组，不会盖到应用窗口）
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
    Some(host)
}

/// 取窗口类名
fn class_of(hwnd: HWND) -> String {
    let mut name = [0u16; 64];
    unsafe {
        GetClassNameW(hwnd, &mut name);
    }
    let len = name.iter().position(|&c| c == 0).unwrap_or(64);
    String::from_utf16_lossy(&name[..len])
}

/// 找桌面图标层 SHELLDLL_DefView（限定 Progman/WorkerW 子树，绝不误命中文件夹窗口）
fn find_defview(shell: HWND) -> Option<HWND> {
    unsafe extern "system" fn find_cb(child: HWND, lparam: LPARAM) -> BOOL {
        let out = lparam.0 as *mut HWND;
        if class_of(child) == "SHELLDLL_DefView" {
            *out = child;
            return BOOL(0);
        }
        BOOL(1)
    }

    unsafe {
        let mut defview = HWND::default();
        let _ = EnumChildWindows(
            Some(shell),
            Some(find_cb),
            LPARAM(&mut defview as *mut HWND as isize),
        );
        if !defview.is_invalid() {
            return Some(defview);
        }
        // shell 子树没有 → 遍历顶层窗口，只在 Progman/WorkerW 里找
        struct Ctx {
            defview: HWND,
        }
        unsafe extern "system" fn scan_top(top: HWND, lparam: LPARAM) -> BOOL {
            let ctx = &mut *(lparam.0 as *mut Ctx);
            let name = class_of(top);
            if name == "Progman" || name == "WorkerW" {
                let mut found = HWND::default();
                let _ = EnumChildWindows(
                    Some(top),
                    Some(find_cb),
                    LPARAM(&mut found as *mut HWND as isize),
                );
                if !found.is_invalid() {
                    ctx.defview = found;
                    return BOOL(0);
                }
            }
            BOOL(1)
        }
        let mut ctx = Ctx {
            defview: HWND::default(),
        };
        let _ = EnumWindows(Some(scan_top), LPARAM(&mut ctx as *mut Ctx as isize));
        (!ctx.defview.is_invalid()).then_some(ctx.defview)
    }
}

/// 顶层壁纸层 WorkerW（Win10 经典布局：图标层所在 WorkerW 的下一个兄弟）
fn find_wallpaper_workerw() -> Option<HWND> {
    let cls_workerw = wide("WorkerW");
    let cls_defview = wide("SHELLDLL_DefView");
    unsafe {
        let null = PCWSTR::null();
        let workerw = PCWSTR(cls_workerw.as_ptr());
        let defview = PCWSTR(cls_defview.as_ptr());
        let mut prev = HWND::default();
        loop {
            let w = FindWindowExW(None::<HWND>, Some(prev), workerw, null).unwrap_or_default();
            if w.is_invalid() {
                return None;
            }
            if FindWindowExW(Some(w), None::<HWND>, defview, null).is_ok() {
                // 图标层所在 WorkerW 的下一个 WorkerW = 壁纸层
                let next = FindWindowExW(None::<HWND>, Some(w), workerw, null).unwrap_or_default();
                return (!next.is_invalid()).then_some(next);
            }
            prev = w;
        }
    }
}
