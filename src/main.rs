//! Desktop Orbit Glass. True per-pixel alpha; no captured wallpaper or text regions.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod cleanup;
mod config;
mod drop;
mod environment;
mod hit;
mod icons;
mod open;
mod placement;
mod render;
mod settings_ui;
mod sys;
mod tray;

use config::Config;
use render::{Renderer, RingGeom, SceneState};
use std::ffi::c_void;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, HBRUSH, PAINTSTRUCT};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetMessageW, GetWindowLongPtrW, KillTimer, LoadCursorW,
    PostMessageW, PostQuitMessage, RegisterClassW, SetTimer, SetWindowLongPtrW, ShowWindow,
    TrackPopupMenu, TranslateMessage, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, HMENU, IDC_ARROW,
    IDC_HAND, IDC_SIZEALL, MF_STRING, MSG, SW_SHOWNOACTIVATE, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WM_APP, WM_CANCELMODE, WM_CAPTURECHANGED, WM_COMMAND, WM_DESTROY,
    WM_DISPLAYCHANGE, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE, WM_PAINT, WM_RBUTTONUP,
    WM_SETCURSOR, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};

const WM_MOUSELEAVE: u32 = 0x02A3;
const CLASS_VISUAL: &str = "RingDockVisual";
const TIMER_TICK: usize = 1;
const TIMER_FRAME: usize = 4;
const FRAME_INTERVAL_MS: u32 = 66;
const TIMER_ANIMATE: usize = 3;
const WM_RECREATE: u32 = WM_APP + 42;
const WM_CLEANUP_DONE: u32 = WM_APP + 43;
const WM_ENVIRONMENT_READY: u32 = WM_APP + 45;
const WM_LOCATION_READY: u32 = WM_APP + 46;
/// 长按定时器：按住超过 HOLD_MS 进入编辑态
const TIMER_HOLD: usize = 2;
const HOLD_MS: u32 = 550;
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
    environment: Option<environment::EnvironmentSnapshot>,
    environment_status: String,
    environment_refreshing: bool,
    animation_origin: Instant,
    resources: crate::sys::ResourceUsage,
    resource_sampler: crate::sys::ResourceSampler,
    resources_paused: bool,
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
    /// 编辑态（长按进入：可删除条目 / 拖动换位）
    edit_mode: bool,
    /// 按下点（长按判定以它为准：按住时轻微移动不取消）
    press_pt: Option<(f32, f32)>,
    /// 本次按下是否已被长按消费（消费后抬起不再当作点击）
    press_consumed: bool,
    /// 编辑态拖动中的条目序号
    drag_item: Option<usize>,
    /// 中键移动整个挂件，与左键排序独立；只在结束时持久化。
    dock_drag: Option<placement::DockDrag>,
    hover: hit::Hit,
    panel_alpha: f32,
    animation: Option<Instant>,
    drop_hot: bool,
    drag_hover: Option<(usize, Instant)>,
    status: Option<(String, Instant)>,
    cleanup_running: bool,
    cleanup_result: Arc<Mutex<Option<String>>>,
    memory_broker: Option<Arc<Mutex<TcpStream>>>,
    preview: bool,
}

impl App {
    fn set_expanded(&mut self, qi: Option<usize>) {
        self.cancel_press();
        let next = qi.filter(|&i| i < self.cfg.quadrants.len());
        let switching_category =
            matches!((self.expanded, next), (Some(current), Some(next)) if current != next);
        self.expanded = next;
        self.scroll = 0.0;
        self.edit_mode = false;
        self.hover = hit::Hit::None;
        self.drop_hot = false;
        self.status = None;
        self.panel_alpha = if self.expanded.is_some() && !switching_category {
            0.20
        } else {
            1.0
        };
        self.animation = self
            .expanded
            .filter(|_| !switching_category)
            .map(|_| Instant::now());
        unsafe {
            if self.animation.is_some() {
                SetTimer(Some(self.hwnd), TIMER_ANIMATE, 16, None);
            } else {
                let _ = KillTimer(Some(self.hwnd), TIMER_ANIMATE);
            }
        }
        self.sync_title();
        self.redraw();
    }
    fn refresh_panel(&mut self) {
        if let Some(q) = self.expanded {
            let n = self.cfg.quadrants.get(q).map_or(0, |q| q.items.len());
            let l = render::layout_panel(&self.cfg, n, &self.geom, self.screen_w, self.screen_h, q);
            self.scroll = self.scroll.clamp(0.0, l.scroll_max);
        }
        self.sync_title();
        self.redraw();
    }
    fn hit_at(&self, x: f32, y: f32) -> hit::Hit {
        hit::hit_test(
            &self.cfg,
            &self.geom,
            self.screen_w,
            self.screen_h,
            self.expanded,
            self.scroll,
            self.edit_mode,
            x,
            y,
        )
    }
    fn cancel_press(&mut self) {
        self.press_pt = None;
        self.drag_item = None;
        self.press_consumed = false;
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_HOLD);
            let _ = ReleaseCapture();
        }
    }
    fn restore_position(&mut self) {
        (self.geom.cx, self.geom.cy) = placement::restored_center(
            self.cfg.dock_position,
            self.screen_w,
            self.screen_h,
            self.geom.r_disc(),
        );
    }
    fn on_middle_down(&mut self, x: f32, y: f32) {
        if self.dock_drag.is_some() || self.hit_at(x, y) == hit::Hit::None {
            return;
        }
        // 先结束左键手势，避免移动挂件时触发长按或意外启动条目。
        if self.drag_item.is_some() {
            self.save_items();
        }
        self.cancel_press();
        self.dock_drag = Some(placement::DockDrag::new(
            (x, y),
            (self.geom.cx, self.geom.cy),
        ));
        self.hover = hit::Hit::None;
        unsafe {
            let _ = SetCapture(self.hwnd);
        }
        self.redraw();
    }
    fn move_dock(&mut self, x: f32, y: f32) {
        if let Some(drag) = self.dock_drag {
            let center = drag.center_at((x, y), self.screen_w, self.screen_h, self.geom.r_disc());
            if center != (self.geom.cx, self.geom.cy) {
                (self.geom.cx, self.geom.cy) = center;
                self.refresh_panel();
            }
        }
    }
    fn finish_dock_drag(&mut self) {
        let Some(drag) = self.dock_drag.take() else {
            return;
        };
        if drag.start_center != (self.geom.cx, self.geom.cy) {
            self.cfg.dock_position = Some(placement::normalized_center(
                (self.geom.cx, self.geom.cy),
                self.screen_w,
                self.screen_h,
            ));
            self.save_items();
        }
        self.hover = hit::Hit::None;
        self.redraw();
    }
    fn on_middle_up(&mut self, x: f32, y: f32) {
        if self.dock_drag.is_none() {
            return;
        }
        self.move_dock(x, y);
        self.finish_dock_drag();
        unsafe {
            let _ = ReleaseCapture();
        }
    }
    /// 窗口标题写入展开/编辑状态（测试观测点：窗口无标题栏，用户不可见）
    fn sync_title(&self) {
        let t = match self.expanded {
            Some(qi) if self.edit_mode => format!("ring-dock#expanded={qi}#edit"),
            Some(qi) => format!("ring-dock#expanded={qi}"),
            None => "ring-dock".to_string(),
        };
        let wt = crate::sys::wide(&t);
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(
                self.hwnd,
                windows::core::PCWSTR(wt.as_ptr()),
            );
        }
    }

    fn redraw(&mut self) {
        if self.hwnd.is_invalid() {
            return;
        }
        let state = SceneState {
            cfg: &self.cfg,
            geom: &self.geom,
            screen_w: self.screen_w,
            screen_h: self.screen_h,
            expanded: self.expanded,
            scroll: self.scroll,
            edit_mode: self.edit_mode,
            hover: self.hover,
            panel_alpha: self.panel_alpha,
            drop_hot: self.drop_hot,
            status: self.status.as_ref().map(|s| s.0.as_str()),
            resources: self.resources,
            second_phase: crate::sys::local_second_phase(),
            local_time_seconds: {
                let (hour, minute, second) = crate::sys::local_hms(0);
                hour as f32 * 3600.0
                    + minute as f32 * 60.0
                    + second as f32
                    + crate::sys::local_second_phase().fract()
            },
            environment: self.environment.as_ref(),
            animation_time: self.animation_origin.elapsed().as_secs_f32(),
        };
        if let Err(e) = self.renderer.draw(&state, self.hwnd) {
            eprintln!("[ring-dock] {e}");
        }
    }
    /// The ring is embedded beneath ordinary windows, so pause expensive updates when
    /// the center chip is fully covered by the active window.
    fn resource_region_visible(&self) -> bool {
        let foreground = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
        if foreground.is_invalid() || foreground == self.hwnd {
            return true;
        }

        let mut class_name = [0u16; 64];
        let class_len = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetClassNameW(foreground, &mut class_name)
        };
        if class_len > 0 {
            let class_name = String::from_utf16_lossy(&class_name[..class_len as usize]);
            if matches!(
                class_name.as_str(),
                "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
            ) {
                return true;
            }
        }

        let mut foreground_rect = RECT::default();
        if unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowRect(foreground, &mut foreground_rect)
        }
        .is_err()
        {
            return true;
        }

        let radius = self.geom.r_chip();
        let left = (self.origin.0 as f32 + self.geom.cx - radius).floor() as i32;
        let top = (self.origin.1 as f32 + self.geom.cy - radius).floor() as i32;
        let right = (self.origin.0 as f32 + self.geom.cx + radius).ceil() as i32;
        let bottom = (self.origin.1 as f32 + self.geom.cy + radius).ceil() as i32;
        let covered = foreground_rect.left <= left
            && foreground_rect.top <= top
            && foreground_rect.right >= right
            && foreground_rect.bottom >= bottom;
        !covered
    }
    fn start_weather_fetch(&mut self, location: environment::SystemLocation) {
        if self.environment_refreshing {
            return;
        }
        self.environment_refreshing = true;
        self.environment_status = "正在获取天气".into();
        self.redraw();
        let hwnd = self.hwnd.0 as isize;
        std::thread::spawn(move || {
            let result = environment::fetch_weather(location);
            post_environment_result(HWND(hwnd as *mut _), result);
        });
    }
    fn request_system_location(&mut self) {
        if self.environment_refreshing {
            return;
        }
        self.environment_refreshing = true;
        self.environment_status = "正在等待 Windows 定位授权".into();
        self.redraw();
        match environment::request_system_location_async(self.hwnd, WM_LOCATION_READY) {
            Ok(()) => {}
            Err(error) => {
                self.environment_refreshing = false;
                self.environment_status = error;
                self.redraw();
            }
        }
    }
    fn start_initial_environment_lookup(&mut self) {
        if self.environment_refreshing {
            return;
        }
        self.environment_refreshing = true;
        self.environment_status = "正在读取系统位置".into();
        let hwnd = self.hwnd.0 as isize;
        std::thread::spawn(move || {
            let result = environment::read_system_location().and_then(environment::fetch_weather);
            post_environment_result(HWND(hwnd as *mut _), result);
        });
    }
    fn start_resource_cleanup(&mut self) {
        if self.cleanup_running {
            return;
        }

        self.cleanup_running = true;
        self.status = Some(("正在清理旧临时文件…".into(), Instant::now()));
        self.redraw();

        let memory_broker = self.memory_broker.clone();
        let result_slot = Arc::clone(&self.cleanup_result);
        let hwnd = self.hwnd.0 as isize;
        let worker = std::thread::Builder::new()
            .name("ring-dock-temp-cleanup".into())
            .spawn(move || {
                let memory_status = match memory_broker {
                    Some(broker) => cleanup::request_memory_cleanup(&broker)
                        .unwrap_or_else(|error| format!("内存整理未执行：{error}")),
                    None => "内存整理未执行：管理员助手未就绪".into(),
                };
                let temp_result = cleanup::clean_old_user_temp_files();
                let message = match temp_result {
                    Ok(summary) => {
                        let temp_status = if summary.files_removed == 0 {
                            "没有可删除的旧临时文件".to_string()
                        } else {
                            format!(
                                "删除 {} 个旧临时文件，释放 {:.1} MB 磁盘空间",
                                summary.files_removed,
                                summary.bytes_removed as f64 / (1024.0 * 1024.0)
                            )
                        };
                        format!("{temp_status}；{memory_status}")
                    }
                    Err(error) => format!("临时文件清理失败：{error}；{memory_status}"),
                };
                if let Ok(mut result) = result_slot.lock() {
                    *result = Some(message);
                }
                unsafe {
                    let _ = PostMessageW(
                        Some(HWND(hwnd as *mut _)),
                        WM_CLEANUP_DONE,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            });
        if let Err(error) = worker {
            self.cleanup_running = false;
            self.status = Some((format!("无法启动清理任务：{error}"), Instant::now()));
            self.redraw();
        }
    }
    /// 面板内某个条目的格中心（命中/拖动共用布局）
    fn cell_at(&self, x: f32, y: f32) -> Option<usize> {
        match self.hit_at(x, y) {
            hit::Hit::Item { index, .. } => Some(index),
            _ => None,
        }
    }

    /// 面板矩形内？（长按判定用）
    fn point_in_panel_client(&self, x: f32, y: f32) -> bool {
        let qi = match self.expanded {
            Some(q) => q,
            None => return false,
        };
        let items = self
            .cfg
            .quadrants
            .get(qi)
            .map(|q| q.items.len())
            .unwrap_or(0);
        let lay = render::layout_panel(
            &self.cfg,
            items,
            &self.geom,
            self.screen_w,
            self.screen_h,
            qi,
        );
        lay.contains(x, y)
    }

    /// 写回配置（删除/排序后）
    fn save_items(&mut self) {
        if let Err(e) = self.cfg.save() {
            eprintln!("[ring-dock] 配置写回失败：{e}");
            self.status = Some(("保存失败 · 本次改动尚未写入磁盘".into(), Instant::now()));
            self.redraw();
        }
    }

    /// 按下：起长按定时器 + 捕获鼠标；编辑态下按住条目=开始拖动换位
    fn on_mouse_down(&mut self, x: f32, y: f32) {
        crate::drop::dlog(&format!(
            "MouseDown ({x},{y}) offset={:?}",
            self.renderer.offset
        ));
        if self.dock_drag.is_some() {
            return;
        }
        self.press_consumed = false;
        self.press_pt = Some((x, y));
        unsafe {
            SetTimer(Some(self.hwnd), TIMER_HOLD, HOLD_MS, None);
            let _ = SetCapture(self.hwnd);
        }
        if self.edit_mode {
            // 只有按在条目本体上才开始拖动；删除角标由抬起时的点击处理
            if let hit::Hit::Item { index, .. } = hit::hit_test(
                &self.cfg,
                &self.geom,
                self.screen_w,
                self.screen_h,
                self.expanded,
                self.scroll,
                self.edit_mode,
                x,
                y,
            ) {
                self.drag_item = Some(index);
            }
        }
    }

    /// 长按超时：仍按住且按下点在面板上 → 进入编辑态（若停在条目上，顺带开始拖动）
    fn on_hold(&mut self) {
        let Some((x, y)) = self.press_pt else { return };
        crate::drop::dlog(&format!(
            "长按触发：按下点({x:.0},{y:.0}) expanded={:?} 在面板内={}",
            self.expanded,
            self.expanded.is_some() && self.point_in_panel_client(x, y)
        ));
        if self.expanded.is_some() && self.point_in_panel_client(x, y) {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), TIMER_HOLD); // 已消费，停止重复触发
            }
            self.edit_mode = true;
            self.press_consumed = true;
            self.drag_item = self.cell_at(x, y);
            self.sync_title();
            self.redraw();
        }
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        if self.dock_drag.is_some() {
            self.move_dock(x, y);
            return;
        }
        unsafe {
            let mut t = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: self.hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut t);
        }
        if let Some((px, py)) = self.press_pt {
            if (x - px).hypot(y - py) > 8.0 && !self.edit_mode {
                self.press_consumed = true;
                unsafe {
                    let _ = KillTimer(Some(self.hwnd), TIMER_HOLD);
                }
            }
        }
        let h = self.hit_at(x, y);
        if self.hover != h {
            self.hover = h;
            self.redraw();
        }
        if let (Some(index), Some(qi)) = (self.drag_item, self.expanded) {
            if let Some(to) = self.cell_at(x, y) {
                if to != index && self.cfg.move_item(qi, index, to) {
                    self.drag_item = Some(to);
                    self.refresh_panel();
                }
            }
        }
    }
    fn on_mouse_up(&mut self, x: f32, y: f32) {
        crate::drop::dlog(&format!(
            "MouseUp ({x},{y}) offset={:?}",
            self.renderer.offset
        ));
        if self.dock_drag.is_some() {
            return;
        }
        let pressed = self.press_pt.take();
        let consumed = self.press_consumed || self.drag_item.is_some();
        let drag = self.drag_item.take().is_some();
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_HOLD);
            let _ = ReleaseCapture();
        }
        if drag {
            self.save_items();
        }
        let mut clicked = false;
        if let Some((px, py)) = pressed {
            if !consumed
                && (x - px).hypot(y - py) <= 8.0
                && self.hit_at(px, py) == self.hit_at(x, y)
            {
                clicked = true;
                self.on_click(x, y);
            }
        }
        self.press_consumed = false;
        // State-changing click handlers already redraw. Avoid rendering the same
        // full-screen layered frame a second time for one mouse-up event.
        if drag || !clicked {
            self.redraw();
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
            self.edit_mode,
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
                    if self.edit_mode {
                        return;
                    }
                    if let Err(e) = open::launch(&item.kind, &item.target) {
                        self.status = Some((e, Instant::now()));
                        self.redraw();
                        return;
                    }
                    if self.cfg.auto_collapse_after_open {
                        self.set_expanded(None);
                    }
                }
            }
            // 弧段：展开 / 收起 / 切换（切换行为走设置）
            hit::Hit::Quadrant(qi) => match self.expanded {
                Some(cur) if cur == qi => self.set_expanded(None), // 再点同一象限 = 收起
                Some(_) if self.cfg.switch_panel_on_click => self.set_expanded(Some(qi)), // 直接切换
                Some(_) => self.set_expanded(None), // 先收起（再点才展开）
                None => self.set_expanded(Some(qi)),
            },
            // 编辑态：点删除角标 → 删条目并写回配置
            hit::Hit::ItemDelete { qi, index } => {
                if self.cfg.remove_item(qi, index) {
                    self.save_items();
                    self.refresh_panel();
                }
            }
            // 面板空白保留状态；独立关闭/整理按钮，中心退出整理或收起
            hit::Hit::PanelClose => self.set_expanded(None),
            hit::Hit::PanelEdit => {
                self.edit_mode = !self.edit_mode;
                self.sync_title();
                self.redraw();
            }
            hit::Hit::PanelBlank => {}
            hit::Hit::Center => {
                if self.edit_mode {
                    self.edit_mode = false;
                    self.sync_title();
                    self.redraw();
                } else if self.expanded.is_some() {
                    self.set_expanded(None);
                } else {
                    self.start_resource_cleanup();
                }
            }
            hit::Hit::None => {
                if self.edit_mode {
                    self.edit_mode = false;
                    self.sync_title();
                    self.redraw();
                } else if self.expanded.is_some() {
                    self.set_expanded(None);
                }
            }
        }
    }

    fn on_wheel(&mut self, delta: f32) {
        if let Some(qi) = self.expanded {
            let items = self
                .cfg
                .quadrants
                .get(qi)
                .map(|q| q.items.len())
                .unwrap_or(0);
            let lay = render::layout_panel(
                &self.cfg,
                items,
                &self.geom,
                self.screen_w,
                self.screen_h,
                qi,
            );
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
            // A desktop-attached child cannot reliably activate a popup menu.
            let owner = CreateWindowExW(
                windows::Win32::UI::WindowsAndMessaging::WS_EX_TOOLWINDOW,
                PCWSTR(crate::sys::wide("STATIC").as_ptr()),
                PCWSTR::null(),
                windows::Win32::UI::WindowsAndMessaging::WS_POPUP,
                p.x,
                p.y,
                1,
                1,
                None,
                None,
                Some(deskpin::module_handle()),
                None,
            )
            .unwrap_or(self.hwnd);
            if owner != self.hwnd {
                let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                    owner,
                    windows::Win32::UI::WindowsAndMessaging::SW_SHOW,
                );
                let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(owner);
            }
            let command = TrackPopupMenu(
                menu,
                TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
                p.x,
                p.y,
                None::<i32>,
                owner,
                None::<*const RECT>,
            );
            let _ = DestroyMenu(menu);
            if owner != self.hwnd {
                let _ = DestroyWindow(owner);
            }
            // Open windows only after the menu releases capture and activation.
            if command.0 != 0 {
                let _ = PostMessageW(
                    Some(self.hwnd),
                    WM_COMMAND,
                    WPARAM(command.0 as usize),
                    LPARAM(0),
                );
            }
        }
    }

    fn reload_config(&mut self) {
        // 显式加载磁盘配置时不把旧内存位置写回覆盖外部编辑。
        self.dock_drag = None;
        self.cancel_press();
        let (cfg, note) = Config::load();
        if !note.is_empty() {
            eprintln!("[ring-dock] {note}");
        }
        self.cfg = cfg;
        if let Err(error) = crate::autostart::apply(self.cfg.auto_start) {
            self.status = Some((format!("开机启动设置失败：{error}"), Instant::now()));
        }
        self.geom.n = self.cfg.quadrant_count;
        self.restore_position();
        self.set_expanded(None);
    }

    /// 打开条目后自动收起面板
    /// 拖放落点是否可接受：面板展开且落在面板内
    pub(crate) fn drop_target_ok(&self, screen_x: f32, screen_y: f32) -> bool {
        drop::point_in_panel(
            &self.cfg,
            &self.geom,
            self.screen_w,
            self.screen_h,
            self.expanded,
            self.origin,
            screen_x,
            screen_y,
        )
    }

    /// 拖放收纳：加入当前展开象限 → 写回 config.json → 刷新面板
    pub(crate) fn on_drop_files(&mut self, paths: &[String]) {
        let qi = match self.expanded {
            Some(q) => q,
            None => return,
        };
        let added = drop::add_items(&mut self.cfg, qi, paths);
        if added > 0 {
            self.save_items();
            self.refresh_panel(); // 刷新布局/命中/重绘（保留编辑态）
        }
    }

    /// 打开设置窗口（托盘 / 右键菜单入口）
    fn open_settings(&self) {
        settings_ui::open_settings(
            self.hwnd,
            self.cfg.switch_panel_on_click,
            self.cfg.auto_collapse_after_open,
            self.cfg.auto_start,
            self.cfg.icon_style,
            self.cfg.category_display_mode,
            &self.cfg.quadrants,
        );
    }

    /// 工作区几何同步（分辨率/任务栏变化）。桌面子窗口收不到 WM_DISPLAYCHANGE，
    /// 靠定时器低频对比几何兜底。
    fn sync_geometry(&mut self) {
        let (x, y, w, h) = deskpin::work_area();
        if (x, y) == self.origin && w as f32 == self.screen_w && h as f32 == self.screen_h {
            return;
        }
        self.finish_dock_drag();
        self.cancel_press();
        self.origin = (x, y);
        self.screen_w = w as f32;
        self.screen_h = h as f32;
        self.restore_position();
        self.set_expanded(None);
    }

    /// 挂载自愈（deskpin）：挂进桌面窗口树（固定显示在桌面上）；
    /// 失败退化为顶层压底（不遮挡应用窗口），定时器持续重试
    fn ensure_embedded(&mut self) {
        if self.preview {
            return;
        }
        let before = self.pinner.mode();
        let after = self.pinner.maintain(self.hwnd);
        if before != Some(after) {
            self.redraw();
        }
    }
    fn register_drop(&mut self) {
        let target: windows::Win32::System::Ole::IDropTarget = drop::DropTarget {
            app: self as *mut App,
            accepts_files: std::cell::Cell::new(false),
        }
        .into();
        if let Err(e) = unsafe { windows::Win32::System::Ole::RegisterDragDrop(self.hwnd, &target) }
        {
            eprintln!("[ring-dock] RegisterDragDrop: {e}");
        }
    }
    pub(crate) fn drag_over(&mut self, sx: f32, sy: f32) -> bool {
        let x = sx - self.origin.0 as f32;
        let y = sy - self.origin.1 as f32;
        let h = self.hit_at(x, y);
        if let hit::Hit::Quadrant(q) = h {
            match self.drag_hover {
                Some((old, start)) if old == q => {
                    if start.elapsed().as_millis() >= 400 && self.expanded != Some(q) {
                        self.set_expanded(Some(q));
                    }
                }
                _ => self.drag_hover = Some((q, Instant::now())),
            }
        } else {
            self.drag_hover = None;
        }
        let ok = self.drop_target_ok(sx, sy);
        if ok != self.drop_hot || self.hover != h {
            self.drop_hot = ok;
            self.hover = h;
            self.redraw();
        }
        ok
    }
    pub(crate) fn drag_leave(&mut self) {
        self.drop_hot = false;
        self.drag_hover = None;
        self.hover = hit::Hit::None;
        self.redraw();
    }

    /// 桌面宿主被销毁（Explorer 重启/换壁纸）会连带销毁桌面子窗口：换新窗口并重挂，不退出
    fn recreate_window(&mut self) {
        match create_visual_window(self.preview) {
            Ok(hwnd) => {
                self.hwnd = hwnd;
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, self as *mut App as isize);
                    SetTimer(Some(hwnd), TIMER_TICK, 500, None);
                    SetTimer(Some(hwnd), TIMER_FRAME, FRAME_INTERVAL_MS, None);
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                }
                self.cancel_press();
                self.animation = None;
                self.panel_alpha = 1.0;
                self.register_drop();
                self.sync_title();
                self.tray.readd(hwnd);
                self.ensure_embedded();
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
            DefWindowProcW(hwnd, msg, w, l)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _hdc = BeginPaint(hwnd, &mut ps);
            if !ptr.is_null() {
                (*ptr).redraw();
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        // 不擦背景（类背景刷为空）：避免擦除色帧闪烁，内容由整帧绘制覆盖
        WM_ERASEBKGND => LRESULT(1),
        WM_LBUTTONDOWN => {
            if !ptr.is_null() {
                let x = (l.0 & 0xFFFF) as i16 as f32 + (*ptr).renderer.offset.0 as f32;
                let y = ((l.0 >> 16) & 0xFFFF) as i16 as f32 + (*ptr).renderer.offset.1 as f32;
                (*ptr).on_mouse_down(x, y);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if !ptr.is_null() {
                let x = (l.0 & 0xFFFF) as i16 as f32 + (*ptr).renderer.offset.0 as f32;
                let y = ((l.0 >> 16) & 0xFFFF) as i16 as f32 + (*ptr).renderer.offset.1 as f32;
                (*ptr).on_mouse_up(x, y);
            }
            LRESULT(0)
        }
        WM_MBUTTONDOWN | WM_MBUTTONUP => {
            if !ptr.is_null() {
                let x = (l.0 & 0xFFFF) as i16 as f32 + (*ptr).renderer.offset.0 as f32;
                let y = ((l.0 >> 16) & 0xFFFF) as i16 as f32 + (*ptr).renderer.offset.1 as f32;
                if msg == WM_MBUTTONDOWN {
                    (*ptr).on_middle_down(x, y);
                } else {
                    (*ptr).on_middle_up(x, y);
                }
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if !ptr.is_null() {
                let x = (l.0 & 0xFFFF) as i16 as f32 + (*ptr).renderer.offset.0 as f32;
                let y = ((l.0 >> 16) & 0xFFFF) as i16 as f32 + (*ptr).renderer.offset.1 as f32;
                (*ptr).on_mouse_move(x, y);
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            if !ptr.is_null() {
                (*ptr).hover = hit::Hit::None;
                (*ptr).redraw();
            }
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            if !ptr.is_null() {
                let app = &mut *ptr;
                app.finish_dock_drag();
                app.press_pt = None;
                app.press_consumed = false;
                if app.drag_item.take().is_some() {
                    app.save_items();
                }
                let _ = KillTimer(Some(hwnd), TIMER_HOLD);
            }
            LRESULT(0)
        }
        WM_CANCELMODE => {
            if !ptr.is_null() {
                (*ptr).finish_dock_drag();
                (*ptr).cancel_press();
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            if !ptr.is_null() {
                let interactive = !matches!(
                    (*ptr).hover,
                    hit::Hit::None | hit::Hit::PanelBlank | hit::Hit::Center
                );
                let id = if (*ptr).dock_drag.is_some() {
                    IDC_SIZEALL
                } else if interactive {
                    IDC_HAND
                } else {
                    IDC_ARROW
                };
                windows::Win32::UI::WindowsAndMessaging::SetCursor(LoadCursorW(None, id).ok());
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, msg, w, l)
        }
        WM_KEYDOWN if w.0 == 27 => {
            if !ptr.is_null() {
                (*ptr).set_expanded(None);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            if !ptr.is_null() && (*ptr).dock_drag.is_none() {
                let delta = ((w.0 >> 16) & 0xFFFF) as i16 as f32;
                (*ptr).on_wheel(delta);
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            if !ptr.is_null() && (*ptr).dock_drag.is_none() {
                (*ptr).popup_menu();
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if !ptr.is_null() && w.0 == TIMER_FRAME {
                let app = &mut *ptr;
                if app.resource_region_visible() {
                    app.redraw();
                }
                return LRESULT(0);
            }
            if !ptr.is_null() && w.0 == TIMER_ANIMATE {
                let app = &mut *ptr;
                if let Some(start) = app.animation {
                    let t = (start.elapsed().as_secs_f32() / 0.16).min(1.0);
                    app.panel_alpha = 1.0 - (1.0 - t).powi(3);
                    if t >= 1.0 {
                        app.animation = None;
                        let _ = KillTimer(Some(hwnd), TIMER_ANIMATE);
                    }
                    app.redraw();
                }
                return LRESULT(0);
            }
            if !ptr.is_null() && w.0 == TIMER_HOLD {
                (*ptr).on_hold();
                return LRESULT(0);
            }
            if !ptr.is_null() && w.0 == TIMER_TICK {
                let app = &mut *ptr;
                app.tick = app.tick.wrapping_add(1);
                if app.tick.is_multiple_of(20) {
                    // 每 ~10s：工作区几何同步 + 挂载自愈（开销可忽略）
                    app.sync_geometry();
                    app.ensure_embedded();
                }
                let stale_location = app.environment.as_ref().and_then(|weather| {
                    (weather.updated_at.elapsed().as_secs() >= 1800).then_some(weather.location)
                });
                if let Some(location) = stale_location {
                    app.start_weather_fetch(location);
                }
                let region_visible = app.resource_region_visible();
                if region_visible {
                    let resumed = app.resources_paused;
                    if resumed {
                        app.resource_sampler.reset_cpu_baseline();
                        SetTimer(Some(hwnd), TIMER_FRAME, FRAME_INTERVAL_MS, None);
                    }
                    let mut usage = app.resource_sampler.sample();
                    if resumed {
                        // Start a fresh CPU interval after the hidden period; keep the last
                        // reading until the next sample instead of showing a misleading 0%.
                        usage.cpu = app.resources.cpu;
                    }
                    let t = app.cfg.clock_text();
                    let usage_changed = usage.cpu.round() != app.resources.cpu.round()
                        || usage.memory.round() != app.resources.memory.round();
                    if resumed || t != app.last_clock || usage_changed {
                        app.last_clock = t;
                        app.resources = usage;
                        app.redraw();
                    }
                    app.resources_paused = false;
                } else if !app.resources_paused {
                    let _ = KillTimer(Some(hwnd), TIMER_FRAME);
                    app.resource_sampler.reset_cpu_baseline();
                    app.resources_paused = true;
                }
                if app
                    .status
                    .as_ref()
                    .is_some_and(|s| s.1.elapsed().as_secs() > 4)
                {
                    app.status = None;
                    if region_visible {
                        app.redraw();
                    }
                }
            }
            LRESULT(0)
        }
        WM_CLEANUP_DONE => {
            if !ptr.is_null() {
                let app = &mut *ptr;
                let message = app
                    .cleanup_result
                    .lock()
                    .ok()
                    .and_then(|mut result| result.take());
                if let Some(message) = message {
                    app.cleanup_running = false;
                    app.status = Some((message, Instant::now()));
                    if app.resource_region_visible() {
                        app.redraw();
                    }
                }
            }
            LRESULT(0)
        }
        settings_ui::WM_REQUEST_ENVIRONMENT => {
            if !ptr.is_null() {
                (*ptr).request_system_location();
            }
            LRESULT(0)
        }
        WM_LOCATION_READY => {
            if !ptr.is_null() && l.0 != 0 {
                let result = Box::from_raw(l.0 as *mut Result<environment::SystemLocation, String>);
                let app = &mut *ptr;
                app.environment_refreshing = false;
                match *result {
                    Ok(location) => app.start_weather_fetch(location),
                    Err(error) => {
                        app.environment_status = error;
                        app.redraw();
                    }
                }
            }
            LRESULT(0)
        }
        WM_ENVIRONMENT_READY => {
            if !ptr.is_null() && l.0 != 0 {
                let result =
                    Box::from_raw(l.0 as *mut Result<environment::EnvironmentSnapshot, String>);
                let app = &mut *ptr;
                app.environment_refreshing = false;
                match *result {
                    Ok(weather) => {
                        app.environment = Some(weather);
                        app.environment_status = "天气已更新".into();
                    }
                    Err(error) => {
                        app.environment_status = error;
                    }
                }
                app.redraw();
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
                        (*ptr).finish_dock_drag();
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
                let _ = windows::Win32::System::Ole::RevokeDragDrop(hwnd);
                (*ptr).hwnd = HWND::default();
                let _ = PostMessageW(None, WM_RECREATE, WPARAM(0), LPARAM(0));
            } else {
                let _ = windows::Win32::System::Ole::RevokeDragDrop(hwnd);
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

fn post_environment_result(hwnd: HWND, result: Result<environment::EnvironmentSnapshot, String>) {
    let payload = Box::into_raw(Box::new(result));
    let posted = unsafe {
        PostMessageW(
            Some(hwnd),
            WM_ENVIRONMENT_READY,
            WPARAM(0),
            LPARAM(payload as isize),
        )
    };
    if posted.is_err() {
        unsafe { drop(Box::from_raw(payload)) };
    }
}

/// 逐像素 alpha 窗口；生产模式挂桌面，显式预览模式才置顶用于隔离测试。
fn create_visual_window(preview: bool) -> Result<HWND, String> {
    let class_visual = crate::sys::wide(CLASS_VISUAL);
    let title_v = crate::sys::wide("ring-dock");
    let hinst = windows::Win32::Foundation::HINSTANCE(deskpin::module_handle().0);
    unsafe {
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED
                | WS_EX_TOOLWINDOW
                | WS_EX_NOACTIVATE
                | if preview {
                    WS_EX_TOPMOST
                } else {
                    Default::default()
                },
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
        Ok(hwnd)
    }
}
fn register_class(
    name: &str,
    brush: Option<HBRUSH>,
    proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
) -> Result<(), String> {
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
            return Err(format!(
                "RegisterClassW({name}) 失败，GetLastError={:?}",
                err
            ));
        }
    }
    Ok(())
}

fn main() {
    if let Some(result) = cleanup::run_memory_broker_mode() {
        if let Err(error) = result {
            eprintln!("[ring-dock broker] {error}");
        }
        return;
    }

    deskpin::enable_dpi_awareness();
    let (ox, oy, w, h) = deskpin::work_area();

    let (cfg, note) = Config::load();
    if !note.is_empty() {
        eprintln!("[ring-dock] {note}");
    }
    let preview = std::env::args().any(|arg| arg == "--preview");
    if !preview {
        if let Err(error) = crate::autostart::apply(cfg.auto_start) {
            eprintln!("[ring-dock] 开机启动设置失败：{error}");
        }
    }
    let broker_result = if preview {
        Ok(None)
    } else {
        cleanup::find_win_memory_cleaner(cfg.win_memory_cleaner_path.as_deref())
            .and_then(|executable| cleanup::start_memory_broker(&executable))
            .map(Some)
    };
    let (memory_broker, startup_status) = match broker_result {
        Ok(broker) => (
            broker,
            Some(("内存清理助手已就绪".to_string(), Instant::now())),
        ),
        Err(error) => (
            None,
            Some((format!("内存清理助手未就绪：{error}"), Instant::now())),
        ),
    };

    let renderer = Renderer::new().expect("Direct2D 初始化失败");

    unsafe {
        register_class(CLASS_VISUAL, None, wnd_proc).expect("窗口类注册失败");
        let hwnd = create_visual_window(preview).unwrap_or_else(|e| {
            eprintln!("[ring-dock] {e}");
            std::process::exit(1);
        });
        // Transparent pixels are both visually empty and input-pass-through.

        let mut app_box = Box::new(App {
            cfg,
            geom: RingGeom {
                cx: w as f32 / 2.0,
                cy: h as f32 * 0.40,
                r_mid: 136.0,
                stroke: 44.0,
                n: 4,
            },
            origin: (ox, oy),
            screen_w: w as f32,
            screen_h: h as f32,
            expanded: None,
            scroll: 0.0,
            renderer,
            hwnd,
            last_clock: String::new(),
            environment: None,
            environment_status: "正在尝试读取系统位置".into(),
            environment_refreshing: false,
            animation_origin: Instant::now(),
            resources: crate::sys::ResourceUsage::default(),
            resource_sampler: crate::sys::ResourceSampler::default(),
            resources_paused: false,
            quitting: false,
            edit_mode: false,
            press_pt: None,
            press_consumed: false,
            drag_item: None,
            dock_drag: None,
            tick: 0,
            pinner: deskpin::Pinner::new(),
            tray: tray::Tray::add(hwnd, "ring-dock", 1),
            msg_taskbar: tray::taskbar_created_msg(),
            hover: hit::Hit::None,
            panel_alpha: 1.0,
            animation: None,
            drop_hot: false,
            drag_hover: None,
            status: startup_status,
            cleanup_running: false,
            cleanup_result: Arc::new(Mutex::new(None)),
            memory_broker,
            preview,
        });
        app_box.geom.n = app_box.cfg.quadrant_count;
        app_box.restore_position();
        let app_ptr: *mut App = &mut *app_box;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, app_ptr as isize);

        (*app_ptr).last_clock = (*app_ptr).cfg.clock_text();
        (*app_ptr).resources = (*app_ptr).resource_sampler.sample();
        let winrt = windows::Win32::System::WinRT::RoInitialize(
            windows::Win32::System::WinRT::RO_INIT_SINGLETHREADED,
        );
        if let Err(ref error) = winrt {
            eprintln!("[ring-dock] Windows Runtime 初始化失败：{error}");
        }
        let ole = windows::Win32::System::Ole::OleInitialize(None);
        if ole.is_ok() {
            (*app_ptr).register_drop();
        } else {
            eprintln!("[ring-dock] OLE 初始化失败：{ole:?}");
        }
        // 挂进桌面窗口树（固定显示在桌面上）；失败退回顶层压底 Z 序，定时器里持续重试
        (*app_ptr).ensure_embedded();
        (*app_ptr).sync_title();
        (*app_ptr).redraw();

        // Display without stealing focus. Alpha zero passes through to the desktop.
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        SetTimer(Some(hwnd), TIMER_TICK, 500, None);
        SetTimer(Some(hwnd), TIMER_FRAME, FRAME_INTERVAL_MS, None);
        (*app_ptr).start_initial_environment_lookup();

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.hwnd.is_invalid() && msg.message == WM_RECREATE {
                (*app_ptr).recreate_window();
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if ole.is_ok() {
            windows::Win32::System::Ole::OleUninitialize();
        }
        if winrt.is_ok() {
            windows::Win32::System::WinRT::RoUninitialize();
        }
        // app_box owns App; reconstructing another Box here would double-free it.
    }
}
