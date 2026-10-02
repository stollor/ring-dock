//! ring-dock —— Windows 桌面圆环收纳工具（初版）
//!
//! 模块地图：
//!   deskpin（独立 crate，零业务）——「固定显示在桌面上」全部能力：桌面树嵌入 / 挂载自愈 / 工作区摆放
//!   hit.rs（命中测试纯函数 + 单测）、tray.rs（托盘）、settings_ui.rs（设置窗口）、render/blur/icons（绘制）
//!
//! 桌面挂件式常驻（动态壁纸/桌面贴图类软件的通行做法，调研结论见 README）：
//!   - 窗口 SetParent 挂进桌面窗口树（挂载点降级链见 deskpin）→ 作为桌面子窗口：
//!     天然在所有应用之下（不遮挡别的窗口），且不参与顶层窗口的最小化/
//!     「显示桌面」（Win+D 只动 Progman 之外的普通应用窗口），结构性常驻
//!   - 不用 WS_EX_TOPMOST：置顶反而盖住应用窗口，且 Win+D 照样把它最小化
//!   - 纯预合成玻璃绘制（每帧先铺桌面快照，不再用色键色——品红中间帧会"闪紫"；
//!     桌面子窗口不支持 UpdateLayeredWindow 逐像素 alpha，预合成出同样的半透明观感）
//!   - SetWindowRgn 只保留「环带 ∪ 中心圆 ∪ 面板」：区域内可点，区域外不显示不拦截
//!   - 挂载自愈（deskpin::Pinner::maintain）：Explorer 重启/换壁纸连带销毁子窗口 →
//!     WM_DESTROY 重建并重挂；定时器低频校验挂载点与工作区几何（子窗口收不到 WM_DISPLAYCHANGE）
//! 空闲近零 CPU：消息驱动；时钟文本无变化不重绘；桌面快照仅在展开/收起时抓取。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod hit;
mod icons;
mod open;
mod render;
mod settings_ui;
mod sys;
mod tray;

use config::Config;
use render::{Renderer, RingGeom, SceneState};
use std::ffi::c_void;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CombineRgn, CreateEllipticRgn, CreateRoundRectRgn, CreateSolidBrush,
    DeleteObject, EndPaint, HBRUSH, HGDIOBJ, HRGN, PAINTSTRUCT, RGN_DIFF, RGN_OR,
    SetWindowRgn,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetMessageW, GetWindowLongPtrW, LoadCursorW, PostQuitMessage,
    RegisterClassW, SetTimer, SetWindowLongPtrW, ShowWindow, TrackPopupMenu, TranslateMessage,
    CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, HMENU, IDC_ARROW, MF_STRING, MSG,
    SW_SHOWNOACTIVATE,
    TPM_RIGHTBUTTON, WM_COMMAND, WM_DESTROY, WM_DISPLAYCHANGE, WM_ERASEBKGND, WM_LBUTTONDBLCLK,
    WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEWHEEL, WM_NCCREATE, WM_PAINT, WM_RBUTTONUP, WM_TIMER, WNDCLASSW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::PCWSTR;

const CLASS_VISUAL: &str = "RingDockVisual";
const TIMER_TICK: usize = 1;
const MENU_SETTINGS: usize = 104;
const MENU_RELOAD: usize = 101;
const MENU_OPENCFG: usize = 102;
const MENU_QUIT: usize = 103;

struct App {
    cfg: Config,
    geom: RingGeom,
    origin: (i32, i32),
    screen_w: f32,
    screen_h: f32,
    expanded: Option<usize>,
    scroll: f32,
    renderer: Renderer,
    hwnd: HWND,
    last_clock: String,
    /// 退出标志：区分「用户退出」与「桌面宿主被销毁后重建窗口」两种 WM_DESTROY
    quitting: bool,
    /// 500ms 心跳计数（低频挂载自愈用）
    tick: u32,
    /// 桌面固定显示管理（deskpin，零业务）
    pinner: deskpin::Pinner,
    /// 托盘图标
    tray: tray::Tray,
    /// "TaskbarCreated" 广播消息 ID（Explorer 重启后重建托盘）
    msg_taskbar: u32,
    /// 待交还桌面重绘的区域（点击路径不跨进程，由定时器低频处理）
    pending_repaint: Option<RECT>,
}

impl App {
    /// 展开/收起：刷新桌面快照（混色底 + 面板模糊底）+ 命中区域 + 重绘
    fn set_expanded(&mut self, qi: Option<usize>) {
        // 面板消失/切换后，旧面板区交还桌面树重绘（layered 窗口由 DWM 合成，一般无残影，保险）
        let old_area = self.expanded.map(|q| {
            let rc = self.panel_rect_win(q);
            RECT {
                left: self.origin.0 + rc.left,
                top: self.origin.1 + rc.top,
                right: self.origin.0 + rc.right,
                bottom: self.origin.1 + rc.bottom,
            }
        });
        self.expanded = qi;
        self.scroll = 0.0;
        self.update_hit_rgn();
        self.redraw();
        // 旧面板区交还桌面重绘（保险）：不在此处跨进程调用（会拖慢点击），
        // 记账后由定时器低频处理
        self.pending_repaint = old_area.or(self.pending_repaint);
    }

    /// 面板矩形（窗口客户区坐标；与 layout_panel 同源）
    fn panel_rect_win(&self, qi: usize) -> RECT {
        let items = self.cfg.quadrants.get(qi).map(|q| q.items.len()).unwrap_or(0);
        let lay = render::layout_panel(&self.cfg, items, &self.geom, self.screen_w, self.screen_h, qi);
        RECT {
            left: lay.x as i32,
            top: lay.y as i32,
            right: (lay.x + lay.w).ceil() as i32 + 1,
            bottom: (lay.y + lay.h).ceil() as i32 + 1,
        }
    }

    /// 命中区域 = 环带（外圆-内圆） ∪ 中心圆 ∪ 面板（展开时）
    fn update_hit_rgn(&self) {
        unsafe {
            let r_out = self.geom.r_mid + self.geom.stroke / 2.0 + 1.0;
            let r_in = self.geom.r_mid - self.geom.stroke / 2.0 - 1.0;
            let outer: HRGN = CreateEllipticRgn(
                (self.geom.cx - r_out) as i32,
                (self.geom.cy - r_out) as i32,
                (self.geom.cx + r_out) as i32,
                (self.geom.cy + r_out) as i32,
            );
            let inner = CreateEllipticRgn(
                (self.geom.cx - r_in) as i32,
                (self.geom.cy - r_in) as i32,
                (self.geom.cx + r_in) as i32,
                (self.geom.cy + r_in) as i32,
            );
            let center = CreateEllipticRgn(
                (self.geom.cx - r_in) as i32,
                (self.geom.cy - r_in) as i32,
                (self.geom.cx + r_in) as i32,
                (self.geom.cy + r_in) as i32,
            );
            // 环带 = 外圆 - 内圆；再并上中心圆
            let _ = CombineRgn(Some(outer), Some(outer), Some(inner), RGN_DIFF);
            let _ = CombineRgn(Some(outer), Some(outer), Some(center), RGN_OR);
            let _ = DeleteObject(HGDIOBJ(inner.0));
            let _ = DeleteObject(HGDIOBJ(center.0));
            if let Some(qi) = self.expanded {
                let items = self.cfg.quadrants.get(qi).map(|q| q.items.len()).unwrap_or(0);
                let lay = render::layout_panel(&self.cfg, items, &self.geom, self.screen_w, self.screen_h, qi);
                let panel = CreateRoundRectRgn(
                    lay.x as i32,
                    lay.y as i32,
                    (lay.x + lay.w) as i32 + 1,
                    (lay.y + lay.h) as i32 + 1,
                    24,
                    24,
                );
                let _ = CombineRgn(Some(outer), Some(outer), Some(panel), RGN_OR);
                let _ = DeleteObject(HGDIOBJ(panel.0));
            }
            let _ = SetWindowRgn(self.hwnd, Some(outer), true);
        }
    }

    /// 重绘：直接拿窗口 DC 立即绘制（不赌 WM_PAINT 消息链）
    fn redraw(&mut self) {
        unsafe {
            let hdc = windows::Win32::Graphics::Gdi::GetDC(Some(self.hwnd));
            if !hdc.is_invalid() {
                self.paint(hdc);
                let _ = windows::Win32::Graphics::Gdi::ReleaseDC(Some(self.hwnd), hdc);
            }
        }
    }

    /// WM_PAINT：把整帧画到窗口 DC
    fn paint(&mut self, hdc: windows::Win32::Graphics::Gdi::HDC) {
        let state = SceneState {
            cfg: &self.cfg,
            geom: &self.geom,
            screen_w: self.screen_w,
            screen_h: self.screen_h,
            expanded: self.expanded,
            scroll: self.scroll,
        };
        if let Err(e) = self.renderer.draw(&state, self.hwnd, hdc) {
            eprintln!("[ring-dock] 渲染失败：{e}");
        }
    }

    /// 点击：命中测试与绘制严格一致（hit.rs，带单测），杜绝跨区域误触发
    fn on_click(&mut self, x: f32, y: f32) {
        let hit = hit::hit_test(
            &self.cfg,
            &self.geom,
            self.screen_w,
            self.screen_h,
            self.expanded,
            self.scroll,
            x,
            y,
        );
        match hit {
            // 面板图标：打开条目（打开后是否自动收起走设置）
            hit::Hit::Item { qi, index } => {
                let item = self
                    .cfg
                    .quadrants
                    .get(qi)
                    .and_then(|q| q.items.get(index))
                    .cloned();
                if let Some(item) = item {
                    open::launch(&item.kind, &item.target);
                    if self.cfg.auto_collapse_after_open {
                        self.set_expanded(None);
                    }
                }
            }
            // 弧段：展开 / 收起 / 切换（切换行为走设置）
            hit::Hit::Quadrant(qi) => match self.expanded {
                Some(cur) if cur == qi => self.set_expanded(None), // 再点同一象限 = 收起
                Some(_) if self.cfg.switch_panel_on_click => self.set_expanded(Some(qi)), // 直接切换
                Some(_) => self.set_expanded(None),               // 先收起（再点才展开）
                None => self.set_expanded(Some(qi)),
            },
            // 中心 / 面板空白 / 弧缝空白：展开时点击 = 收起（保证"回得去"）
            hit::Hit::Center | hit::Hit::PanelBlank | hit::Hit::None => {
                if self.expanded.is_some() {
                    self.set_expanded(None);
                }
            }
        }
    }

    fn on_wheel(&mut self, delta: f32) {
        if let Some(qi) = self.expanded {
            let items = self.cfg.quadrants.get(qi).map(|q| q.items.len()).unwrap_or(0);
            let lay = render::layout_panel(&self.cfg, items, &self.geom, self.screen_w, self.screen_h, qi);
            if lay.scroll_max > 0.0 {
                self.scroll = (self.scroll - delta * 0.35).clamp(0.0, lay.scroll_max);
                self.redraw();
            }
        }
    }

    fn popup_menu(&self) {
        unsafe {
            let menu: HMENU = match CreatePopupMenu() {
                Ok(m) => m,
                Err(_) => return,
            };
            let t0 = crate::sys::wide("设置…");
            let t1 = crate::sys::wide("重新加载配置");
            let t2 = crate::sys::wide("打开配置文件");
            let t3 = crate::sys::wide("退出");
            let _ = AppendMenuW(menu, MF_STRING, MENU_SETTINGS, PCWSTR(t0.as_ptr()));
            let _ = AppendMenuW(menu, MF_STRING, MENU_RELOAD, PCWSTR(t1.as_ptr()));
            let _ = AppendMenuW(menu, MF_STRING, MENU_OPENCFG, PCWSTR(t2.as_ptr()));
            let _ = AppendMenuW(menu, MF_STRING, MENU_QUIT, PCWSTR(t3.as_ptr()));
            let mut p = POINT::default();
            let _ = GetCursorPos(&mut p);
            let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, p.x, p.y, None::<i32>, self.hwnd, None::<*const RECT>);
            let _ = DestroyMenu(menu);
        }
    }

    fn reload_config(&mut self) {
        let (cfg, note) = Config::load();
        if !note.is_empty() {
            eprintln!("[ring-dock] {note}");
        }
        self.cfg = cfg;
        self.geom.n = self.cfg.quadrant_count;
        self.set_expanded(None);
    }

    /// 打开设置窗口（托盘 / 右键菜单入口）
    fn open_settings(&self) {
        settings_ui::open_settings(
            self.hwnd,
            self.cfg.switch_panel_on_click,
            self.cfg.auto_collapse_after_open,
        );
    }

    /// 工作区几何同步（分辨率/任务栏变化）。桌面子窗口收不到 WM_DISPLAYCHANGE，
    /// 靠定时器低频对比几何兜底。
    fn sync_geometry(&mut self) {
        let (x, y, w, h) = deskpin::work_area();
        if (x, y) == self.origin && w as f32 == self.screen_w && h as f32 == self.screen_h {
            return;
        }
        // 窗口整体挪动：旧位置交还桌面重绘（layered 合成，保险）
        let old_win = RECT {
            left: self.origin.0,
            top: self.origin.1,
            right: self.origin.0 + self.screen_w as i32,
            bottom: self.origin.1 + self.screen_h as i32,
        };
        self.set_expanded(None);
        self.origin = (x, y);
        self.screen_w = w as f32;
        self.screen_h = h as f32;
        self.geom.cx = w as f32 / 2.0;
        self.geom.cy = h as f32 / 2.0;
        deskpin::place_workarea(self.hwnd);
        deskpin::repaint_desktop_area(old_win);
    }

    /// 挂载自愈（deskpin）：挂进桌面窗口树（固定显示在桌面上）；
    /// 失败退化为顶层压底（不遮挡应用窗口），定时器持续重试
    fn ensure_embedded(&mut self) {
        let before = self.pinner.mode();
        let after = self.pinner.maintain(self.hwnd);
        if before != Some(after) {
            self.update_hit_rgn();
            self.redraw();
        }
    }

    /// 桌面宿主被销毁（Explorer 重启/换壁纸）会连带销毁桌面子窗口：换新窗口并重挂，不退出
    fn recreate_window(&mut self) {
        match create_visual_window() {
            Ok(hwnd) => {
                self.hwnd = hwnd;
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, self as *mut App as isize);
                    SetTimer(Some(hwnd), TIMER_TICK, 500, None);
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                }
                self.tray.readd(hwnd);
                self.ensure_embedded();
                self.update_hit_rgn();
                self.redraw();
            }
            Err(e) => eprintln!("[ring-dock] 窗口重建失败：{e}"),
        }
    }
}

/// 窗口过程
unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
    match msg {
        WM_NCCREATE => {
            let cs = l.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW;
            if !cs.is_null() {
                let app = (*cs).lpCreateParams as *mut App;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, app as isize);
            }
            LRESULT(1)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            if !ptr.is_null() {
                (*ptr).paint(hdc);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        // 不擦背景（类背景刷为空）：避免擦除色帧闪烁，内容由整帧绘制覆盖
        WM_ERASEBKGND => LRESULT(1),
        WM_LBUTTONDOWN => {
            if !ptr.is_null() {
                let x = (l.0 & 0xFFFF) as i16 as f32;
                let y = ((l.0 >> 16) & 0xFFFF) as i16 as f32;
                (*ptr).on_click(x, y);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            if !ptr.is_null() {
                let delta = ((w.0 >> 16) & 0xFFFF) as i16 as f32;
                (*ptr).on_wheel(delta);
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            if !ptr.is_null() {
                (*ptr).popup_menu();
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if !ptr.is_null() && w.0 == TIMER_TICK {
                let app = &mut *ptr;
                app.tick = app.tick.wrapping_add(1);
                if app.tick % 20 == 0 {
                    // 每 ~10s：工作区几何同步 + 挂载自愈（开销可忽略）
                    app.sync_geometry();
                    app.ensure_embedded();
                }
                if let Some(rc) = app.pending_repaint.take() {
                    // 低频处理「交还桌面重绘」（点击路径零跨进程调用）
                    deskpin::repaint_desktop_area(rc);
                }
                let t = app.cfg.clock_text();
                if t != app.last_clock {
                    app.last_clock = t;
                    app.redraw();
                }
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            if !ptr.is_null() {
                match w.0 & 0xFFFF {
                    MENU_SETTINGS => (*ptr).open_settings(),
                    MENU_RELOAD => (*ptr).reload_config(),
                    MENU_OPENCFG => open::open_config_file(),
                    MENU_QUIT => {
                        (*ptr).tray.remove();
                        (*ptr).quitting = true;
                        let _ = DestroyWindow((*ptr).hwnd);
                    }
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            if !ptr.is_null() {
                (*ptr).sync_geometry();
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            if !ptr.is_null() && !(*ptr).quitting {
                // 非用户退出（Explorer 重启/换壁纸连带销毁子窗口）→ 重建并重挂
                (*ptr).recreate_window();
            } else {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        // Explorer 重启广播（动态消息 ID）：重建托盘图标
        m if !ptr.is_null() && m == (*ptr).msg_taskbar => {
            (*ptr).tray.readd((*ptr).hwnd);
            LRESULT(0)
        }
        // 托盘点击（左/右键）→ 弹同一套菜单
        tray::WM_TRAY => {
            if !ptr.is_null() {
                let mouse = (l.0 & 0xFFFF) as u32;
                if mouse == WM_LBUTTONUP || mouse == WM_RBUTTONUP || mouse == WM_LBUTTONDBLCLK {
                    (*ptr).popup_menu();
                }
            }
            LRESULT(0)
        }
        // 设置界面保存完毕 → 重新加载配置
        settings_ui::WM_CONFIG_SAVED => {
            if !ptr.is_null() {
                (*ptr).reload_config();
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

/// 创建可视窗口：WS_EX_LAYERED 统一透明度（一块半透明玻璃，命中按 Rgn 全域生效），
/// 随后由 deskpin 挂进桌面窗口树；嵌入失败时保持顶层压底并定时重试
fn create_visual_window() -> Result<HWND, String> {
    let class_visual = crate::sys::wide(CLASS_VISUAL);
    let title_v = crate::sys::wide("ring-dock");
    let hinst = windows::Win32::Foundation::HINSTANCE(deskpin::module_handle().0);
    unsafe {
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            PCWSTR(class_visual.as_ptr()),
            PCWSTR(title_v.as_ptr()),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinst),
            Some(std::ptr::null::<c_void>()),
        )
        .map_err(|e| format!("窗口创建失败：{e:?}"))?;
        // 不用 WS_EX_LAYERED：layered 窗口的鼠标命中按"像素透明度"算，实测极难点中。
        // 普通窗口 + SetWindowRgn：Rgn 内全域可命中，视觉=不透明深色玻璃块。
        Ok(hwnd)
    }
}
fn register_class(name: &str, brush: Option<HBRUSH>, proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT) -> Result<(), String> {
    let cname = crate::sys::wide(name);
    unsafe {
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(proc),
            hInstance: deskpin::module_handle(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: brush.unwrap_or_default(),
            lpszClassName: PCWSTR(cname.as_ptr()),
            ..Default::default()
        };
        let atom = RegisterClassW(&wc);
        if atom == 0 {
            let err = windows::Win32::Foundation::GetLastError();
            return Err(format!("RegisterClassW({name}) 失败，GetLastError={:?}", err));
        }
    }
    Ok(())
}

fn main() {
    deskpin::enable_dpi_awareness();
    let (ox, oy, w, h) = deskpin::work_area();

    let (cfg, note) = Config::load();
    if !note.is_empty() {
        eprintln!("[ring-dock] {note}");
    }

    let renderer = Renderer::new().expect("Direct2D 初始化失败");

    unsafe {
        // 类背景 = 色键黑（WS_EX_LAYERED 色键透明：黑像素=完全透明，其余像素×alpha 半透明）
        let key_brush = CreateSolidBrush(COLORREF(0x000000));
        register_class(CLASS_VISUAL, Some(key_brush), wnd_proc).expect("窗口类注册失败");

        let hwnd = create_visual_window().unwrap_or_else(|e| {
            eprintln!("[ring-dock] {e}");
            std::process::exit(1);
        });
        // 形状即透明：SetWindowRgn 之外的区域不存在（无需 layered/色键）

        let mut app_box = Box::new(App {
            cfg,
            geom: RingGeom { cx: w as f32 / 2.0, cy: h as f32 / 2.0, r_mid: 117.0, stroke: 26.0, n: 4 },
            origin: (ox, oy),
            screen_w: w as f32,
            screen_h: h as f32,
            expanded: None,
            scroll: 0.0,
            renderer,
            hwnd,
            last_clock: String::new(),
            quitting: false,
            tick: 0,
            pinner: deskpin::Pinner::new(),
            tray: tray::Tray::add(hwnd, "ring-dock", 1),
            msg_taskbar: tray::taskbar_created_msg(),
            pending_repaint: None,
        });
        app_box.geom.n = app_box.cfg.quadrant_count;
        let app_ptr: *mut App = &mut *app_box;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, app_ptr as isize);

        (*app_ptr).update_hit_rgn();
        (*app_ptr).last_clock = (*app_ptr).cfg.clock_text();
        // 挂进桌面窗口树（固定显示在桌面上）；失败退回顶层压底 Z 序，定时器里持续重试
        (*app_ptr).ensure_embedded();
        (*app_ptr).redraw();

        // 显示但不抢焦点；SetWindowRgn 形状外完全不存在 → 不拦截任何鼠标/滚动
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        SetTimer(Some(hwnd), TIMER_TICK, 500, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        drop(Box::from_raw(app_ptr));
    }
}
