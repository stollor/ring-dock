//! 设置界面（独立顶层窗口，托盘菜单打开）：交互开关 → 写 config.json → 通知主窗口重载。
//! 纯 Win32 控件（BUTTON/STATIC），不引入任何 UI 框架。
use crate::config::Config;
use crate::open;
use crate::sys::wide;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetStockObject, DEFAULT_GUI_FONT};
use windows::Win32::UI::Controls::{CheckDlgButton, IsDlgButtonChecked, BST_CHECKED, BST_UNCHECKED};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, IsWindow, LoadCursorW, PostMessageW,
    RegisterClassW, SendMessageW, SetForegroundWindow, ShowWindow, BS_AUTOCHECKBOX,
    BS_DEFPUSHBUTTON, CS_HREDRAW, CS_VREDRAW, HMENU, IDC_ARROW, SW_SHOW, SW_SHOWNORMAL,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_COMMAND, WM_DESTROY, WM_SETFONT, WNDCLASSW,
    WS_CAPTION, WS_CHILD, WS_EX_TOOLWINDOW, WS_OVERLAPPED, WS_SYSMENU, WS_VISIBLE,
};
use windows::core::PCWSTR;

/// 发给主窗口：设置已保存，请重新加载 config.json
pub const WM_CONFIG_SAVED: u32 = WM_APP + 2;

const CLASS_NAME: &str = "RingDockSettings";

// 控件 ID
const ID_SWITCH: i32 = 1001;
const ID_AUTOCLOSE: i32 = 1002;
const ID_SAVE: i32 = 2001;
const ID_CANCEL: i32 = 2002;
const ID_OPENCFG: i32 = 2003;

static SETTINGS_HWND: AtomicIsize = AtomicIsize::new(0);
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static CLASS_REGISTERED: AtomicBool = AtomicBool::new(false);

/// 打开设置窗口（单实例：已打开则置前）
pub fn open_settings(main_hwnd: HWND, switch_on_click: bool, auto_collapse: bool) {
    unsafe {
        // 单实例：已有窗口则激活置前
        let existing = SETTINGS_HWND.load(Ordering::SeqCst);
        if existing != 0 {
            let h = HWND(existing as *mut _);
            if IsWindow(Some(h)).as_bool() {
                let _ = ShowWindow(h, SW_SHOWNORMAL);
                let _ = SetForegroundWindow(h);
                return;
            }
        }
        MAIN_HWND.store(main_hwnd.0 as isize, Ordering::SeqCst);
        register_class_once();

        // 居中于主屏工作区
        let (x, y, aw, ah) = deskpin::work_area();
        let (ww, hh) = (540, 240);
        let wx = x + ((aw - ww) / 2).max(0);
        let wy = y + ((ah - hh) / 2).max(0);

        let cls = wide(CLASS_NAME);
        let title = wide("ring-dock 设置");
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            PCWSTR(cls.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
            wx,
            wy,
            ww,
            hh,
            None,
            None,
            Some(deskpin::module_handle()),
            None,
        );
        let hwnd = match hwnd {
            Ok(h) => h,
            Err(e) => {
                eprintln!("[ring-dock] 设置窗口创建失败：{e:?}");
                return;
            }
        };
        create_controls(hwnd, switch_on_click, auto_collapse);
        SETTINGS_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

unsafe fn register_class_once() {
    if CLASS_REGISTERED.swap(true, Ordering::SeqCst) {
        return;
    }
    let cname = wide(CLASS_NAME);
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(settings_proc),
        hInstance: deskpin::module_handle(),
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        lpszClassName: PCWSTR(cname.as_ptr()),
        ..Default::default()
    };
    let _ = RegisterClassW(&wc);
}

/// 创建子控件并套用系统 UI 字体 / 勾选初值
unsafe fn create_controls(parent: HWND, switch_on_click: bool, auto_collapse: bool) {
    let font = GetStockObject(DEFAULT_GUI_FONT);
    let make = |class: &str, text: &str, style: u32, x: i32, y: i32, w: i32, h: i32, id: i32| {
        let cls = wide(class);
        let tx = wide(text);
        let c = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(cls.as_ptr()),
            PCWSTR(tx.as_ptr()),
            WINDOW_STYLE(style),
            x,
            y,
            w,
            h,
            Some(parent),
            Some(HMENU(id as isize as *mut _)),
            Some(deskpin::module_handle()),
            None,
        )
        .unwrap_or_default();
        let _ = SendMessageW(
            c,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
        c
    };

    let base = WS_CHILD.0 | WS_VISIBLE.0;
    make("STATIC", "面板交互", base, 24, 18, 480, 20, 0);
    make(
        "BUTTON",
        "已展开面板时，点击其他象限直接切换面板（不勾选 = 先收起，再点才展开）",
        base | BS_AUTOCHECKBOX as u32,
        24,
        48,
        490,
        24,
        ID_SWITCH,
    );
    make(
        "BUTTON",
        "打开条目（程序 / 文件 / 网址）后自动收起面板",
        base | BS_AUTOCHECKBOX as u32,
        24,
        82,
        490,
        24,
        ID_AUTOCLOSE,
    );
    make("BUTTON", "保存", base | BS_DEFPUSHBUTTON as u32, 232, 140, 90, 28, ID_SAVE);
    make("BUTTON", "取消", base, 332, 140, 90, 28, ID_CANCEL);
    make("BUTTON", "打开配置文件…", base, 24, 140, 150, 28, ID_OPENCFG);

    CheckDlgButton(
        parent,
        ID_SWITCH,
        if switch_on_click { BST_CHECKED } else { BST_UNCHECKED },
    )
    .ok();
    CheckDlgButton(
        parent,
        ID_AUTOCLOSE,
        if auto_collapse { BST_CHECKED } else { BST_UNCHECKED },
    )
    .ok();
}

/// 读勾选 → 落盘 config.json → 通知主窗口重载
unsafe fn save(hwnd: HWND) {
    let switch_on_click = IsDlgButtonChecked(hwnd, ID_SWITCH) != 0;
    let auto_collapse_after_open = IsDlgButtonChecked(hwnd, ID_AUTOCLOSE) != 0;
    let (mut cfg, note) = Config::load();
    if !note.is_empty() {
        eprintln!("[ring-dock] {note}");
    }
    cfg.switch_panel_on_click = switch_on_click;
    cfg.auto_collapse_after_open = auto_collapse_after_open;
    match cfg.save() {
        Ok(()) => {
            let main = MAIN_HWND.load(Ordering::SeqCst);
            if main != 0 {
                let _ = PostMessageW(
                    Some(HWND(main as *mut _)),
                    WM_CONFIG_SAVED,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
        Err(e) => eprintln!("[ring-dock] 设置保存失败：{e}"),
    }
}

unsafe extern "system" fn settings_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            match (w.0 & 0xFFFF) as i32 {
                ID_SAVE => {
                    save(hwnd);
                    let _ = DestroyWindow(hwnd);
                }
                ID_CANCEL => {
                    let _ = DestroyWindow(hwnd);
                }
                ID_OPENCFG => open::open_config_file(),
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            SETTINGS_HWND.store(0, Ordering::SeqCst);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}
