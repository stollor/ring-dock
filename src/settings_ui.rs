//! 设置界面（独立顶层窗口，托盘菜单打开）：交互开关 → 写 config.json → 通知主窗口重载。
//! 纯 Win32 控件（BUTTON/STATIC），不引入任何 UI 框架。
use crate::config::{CategoryDisplayMode, Config, IconStyle, Quadrant};
use crate::open;
use crate::sys::wide;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::{
    CheckDlgButton, IsDlgButtonChecked, BST_CHECKED, BST_UNCHECKED, EM_SETLIMITTEXT,
};
use windows::Win32::UI::Controls::{SetWindowTheme, DRAWITEMSTRUCT, ODS_FOCUS, ODS_SELECTED};
use windows::Win32::UI::WindowsAndMessaging::*;

/// 发给主窗口：设置已保存，请重新加载 config.json
pub const WM_CONFIG_SAVED: u32 = WM_APP + 2;
pub const WM_REQUEST_ENVIRONMENT: u32 = WM_APP + 44;

static TITLE_FONT: AtomicIsize = AtomicIsize::new(0);
static UI_FONT: AtomicIsize = AtomicIsize::new(0);
static BG_BRUSH: AtomicIsize = AtomicIsize::new(0);
const BACKGROUND: COLORREF = COLORREF(0x00251c16);
const FOREGROUND: COLORREF = COLORREF(0x00f4e8df);
#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(
        hwnd: *mut std::ffi::c_void,
        attribute: u32,
        value: *const std::ffi::c_void,
        size: u32,
    ) -> i32;
}
const CLASS_NAME: &str = "RingDockSettings";

// 控件 ID
const ID_SWITCH: i32 = 1001;
const ID_AUTOCLOSE: i32 = 1002;
const ID_AUTOSTART: i32 = 1005;
const ID_ICON_STYLE: i32 = 1003;
const ID_CATEGORY_MODE: i32 = 1004;
const ID_CATEGORY_COUNT: i32 = 1006;
const ID_CATEGORY_ICON_BASE: i32 = 1100;
const ID_CATEGORY_NAME_BASE: i32 = 1200;
const ID_CATEGORY_FILE_BASE: i32 = 1300;
const CATEGORY_SLOTS: i32 = 8;
const ICON_PRESETS: [(&str, &str); 9] = [
    ("auto", "自动（按分类名称）"),
    ("collaboration", "对话气泡"),
    ("development", "代码符号"),
    ("ai", "AI 星芒"),
    ("entertainment", "播放按钮"),
    ("program", "程序窗口"),
    ("file", "文件"),
    ("folder", "文件夹"),
    ("url", "网址"),
];
const ID_SAVE: i32 = 2001;
const ID_CANCEL: i32 = 2002;
const ID_OPENCFG: i32 = 2003;
const ID_REQUEST_ENVIRONMENT: i32 = 2004;
const ID_WEATHER_NOTE: i32 = 2005;

static CATEGORY_ICON_PATHS: Mutex<[String; 8]> = Mutex::new([
    String::new(),
    String::new(),
    String::new(),
    String::new(),
    String::new(),
    String::new(),
    String::new(),
    String::new(),
]);
static SETTINGS_HWND: AtomicIsize = AtomicIsize::new(0);
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static CLASS_REGISTERED: AtomicBool = AtomicBool::new(false);

/// 打开设置窗口（单实例：已打开则置前）
pub fn open_settings(
    main_hwnd: HWND,
    switch_on_click: bool,
    auto_collapse: bool,
    auto_start: bool,
    icon_style: IconStyle,
    category_display_mode: CategoryDisplayMode,
    quadrants: &[Quadrant],
) {
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
        let row_count = quadrants.len().clamp(2, 8) as i32;
        let (_, _, hh) = category_layout(row_count);
        let ww = 640;
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
        let dark: i32 = 1;
        DwmSetWindowAttribute(hwnd.0, 20, &dark as *const _ as *const std::ffi::c_void, 4);
        create_controls(
            hwnd,
            switch_on_click,
            auto_collapse,
            auto_start,
            icon_style,
            category_display_mode,
            quadrants,
            hh,
        );
        SETTINGS_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
        // The first ShowWindow can obey STARTUPINFO's hidden launch flag.
        // SetWindowPos explicitly shows this user-requested window regardless.
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_SHOWWINDOW,
        );
        let _ = SetForegroundWindow(hwnd);
    }
}

unsafe fn register_class_once() {
    if CLASS_REGISTERED.swap(true, Ordering::SeqCst) {
        return;
    }
    BG_BRUSH.store(CreateSolidBrush(BACKGROUND).0 as isize, Ordering::SeqCst);
    let face = wide("Noto Sans SC");
    UI_FONT.store(
        CreateFontW(
            -17,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            DEFAULT_PITCH.0 as u32,
            PCWSTR(face.as_ptr()),
        )
        .0 as isize,
        Ordering::SeqCst,
    );
    TITLE_FONT.store(
        CreateFontW(
            -24,
            0,
            0,
            0,
            600,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            DEFAULT_PITCH.0 as u32,
            PCWSTR(face.as_ptr()),
        )
        .0 as isize,
        Ordering::SeqCst,
    );
    let cname = wide(CLASS_NAME);
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(settings_proc),
        hInstance: deskpin::module_handle(),
        hbrBackground: HBRUSH(BG_BRUSH.load(Ordering::SeqCst) as *mut _),
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        lpszClassName: PCWSTR(cname.as_ptr()),
        ..Default::default()
    };
    let _ = RegisterClassW(&wc);
}

/// 创建子控件并套用系统 UI 字体 / 勾选初值
unsafe fn create_controls(
    parent: HWND,
    switch_on_click: bool,
    auto_collapse: bool,
    auto_start: bool,
    icon_style: IconStyle,
    category_display_mode: CategoryDisplayMode,
    quadrants: &[Quadrant],
    window_height: i32,
) {
    let font = HFONT(UI_FONT.load(Ordering::SeqCst) as *mut _);
    let make = |class: &str, text: &str, style: u32, x: i32, y: i32, w: i32, h: i32, id: i32| {
        let cls = wide(class);
        let tx = wide(text);
        let c = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(cls.as_ptr()),
            PCWSTR(tx.as_ptr()),
            WINDOW_STYLE(
                if class == "BUTTON" && style & 0xf != BS_AUTOCHECKBOX as u32 {
                    (style & !(BS_DEFPUSHBUTTON as u32)) | BS_OWNERDRAW as u32 | WS_TABSTOP.0
                } else {
                    style | if class == "BUTTON" { WS_TABSTOP.0 } else { 0 }
                },
            ),
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
        if class == "BUTTON" {
            let empty = wide("");
            let _ = SetWindowTheme(c, PCWSTR(empty.as_ptr()), PCWSTR(empty.as_ptr()));
        }
        let _ = SendMessageW(
            c,
            WM_SETFONT,
            Some(WPARAM(if text.starts_with("O R B I T") {
                TITLE_FONT.load(Ordering::SeqCst) as usize
            } else {
                font.0 as usize
            })),
            Some(LPARAM(1)),
        );
        c
    };

    let base = WS_CHILD.0 | WS_VISIBLE.0;
    make(
        "STATIC",
        "O R B I T   /   偏好设置",
        base,
        32,
        24,
        560,
        34,
        0,
    );
    make("STATIC", "让桌面按照你的习惯运转", base, 32, 60, 560, 24, 0);
    make("STATIC", "01   交互与行为", base, 32, 112, 560, 24, 0);
    make(
        "BUTTON",
        "点击其他分类时直接切换面板",
        base | BS_AUTOCHECKBOX as u32,
        36,
        150,
        550,
        28,
        ID_SWITCH,
    );
    make(
        "BUTTON",
        "打开程序、文件或网址后自动收起",
        base | BS_AUTOCHECKBOX as u32,
        36,
        188,
        550,
        28,
        ID_AUTOCLOSE,
    );
    make(
        "BUTTON",
        "登录 Windows 时自动启动 Ring Dock",
        base | BS_AUTOCHECKBOX as u32,
        36,
        226,
        550,
        28,
        ID_AUTOSTART,
    );
    make("STATIC", "02   外观与分类", base, 32, 282, 560, 24, 0);
    make("STATIC", "条目图标", base, 36, 326, 140, 24, 0);
    let combo_cls = wide("COMBOBOX");
    let combo = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        PCWSTR(combo_cls.as_ptr()),
        PCWSTR::null(),
        WINDOW_STYLE(base | CBS_DROPDOWNLIST as u32),
        190,
        320,
        398,
        120,
        Some(parent),
        Some(HMENU(ID_ICON_STYLE as isize as *mut _)),
        Some(deskpin::module_handle()),
        None,
    )
    .unwrap_or_default();
    let theme = wide("DarkMode_CFD");
    let _ = SetWindowTheme(combo, PCWSTR(theme.as_ptr()), PCWSTR::null());
    let choices = ["统一默认样式", "原图标样式", "原图标冷色调处理"];
    for choice in choices {
        let text = wide(choice);
        let _ = SendMessageW(
            combo,
            CB_ADDSTRING,
            Some(WPARAM(0)),
            Some(LPARAM(text.as_ptr() as isize)),
        );
    }
    let selected = match icon_style {
        IconStyle::Unified => 0,
        IconStyle::Original => 1,
        IconStyle::Tinted => 2,
    };
    let _ = SendMessageW(combo, CB_SETCURSEL, Some(WPARAM(selected)), Some(LPARAM(0)));
    let _ = SendMessageW(
        combo,
        WM_SETFONT,
        Some(WPARAM(font.0 as usize)),
        Some(LPARAM(1)),
    );
    make("STATIC", "圆环显示", base, 36, 368, 140, 24, 0);
    let mode_cls = wide("COMBOBOX");
    let mode_combo = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        PCWSTR(mode_cls.as_ptr()),
        PCWSTR::null(),
        WINDOW_STYLE(base | CBS_DROPDOWNLIST as u32),
        190,
        362,
        398,
        120,
        Some(parent),
        Some(HMENU(ID_CATEGORY_MODE as isize as *mut _)),
        Some(deskpin::module_handle()),
        None,
    )
    .unwrap_or_default();
    let _ = SetWindowTheme(mode_combo, PCWSTR(theme.as_ptr()), PCWSTR::null());
    let mode_choices = ["只显示图标", "只显示弧形文字", "图标和文字"];
    for choice in mode_choices {
        let text = wide(choice);
        let _ = SendMessageW(
            mode_combo,
            CB_ADDSTRING,
            Some(WPARAM(0)),
            Some(LPARAM(text.as_ptr() as isize)),
        );
    }
    let mode_selected = match category_display_mode {
        CategoryDisplayMode::Icon => 0,
        CategoryDisplayMode::Text => 1,
        CategoryDisplayMode::Both => 2,
    };
    let _ = SendMessageW(
        mode_combo,
        CB_SETCURSEL,
        Some(WPARAM(mode_selected)),
        Some(LPARAM(0)),
    );
    let _ = SendMessageW(
        mode_combo,
        WM_SETFONT,
        Some(WPARAM(font.0 as usize)),
        Some(LPARAM(1)),
    );

    make("STATIC", "分类数量", base, 36, 410, 140, 24, 0);
    let count_cls = wide("COMBOBOX");
    let count_combo = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        PCWSTR(count_cls.as_ptr()),
        PCWSTR::null(),
        WINDOW_STYLE(base | CBS_DROPDOWNLIST as u32),
        190,
        404,
        160,
        180,
        Some(parent),
        Some(HMENU(ID_CATEGORY_COUNT as isize as *mut _)),
        Some(deskpin::module_handle()),
        None,
    )
    .unwrap_or_default();
    let _ = SetWindowTheme(count_combo, PCWSTR(theme.as_ptr()), PCWSTR::null());
    for count in 2..=CATEGORY_SLOTS {
        let text = wide(&format!("{count} 个"));
        let _ = SendMessageW(
            count_combo,
            CB_ADDSTRING,
            Some(WPARAM(0)),
            Some(LPARAM(text.as_ptr() as isize)),
        );
    }
    let _ = SendMessageW(
        count_combo,
        WM_SETFONT,
        Some(WPARAM(font.0 as usize)),
        Some(LPARAM(1)),
    );
    make("STATIC", "名称", base, 36, 446, 140, 20, 0);
    make("STATIC", "图标", base, 190, 446, 140, 20, 0);
    let shown = quadrants.len().clamp(2, CATEGORY_SLOTS as usize) as i32;
    for index in 0..CATEGORY_SLOTS {
        let y = 468 + index * 34;
        let quadrant = quadrants.get(index as usize);
        let visible = if index < shown { WS_VISIBLE.0 } else { 0 };
        let name = quadrant
            .map(|q| q.label.clone())
            .unwrap_or_else(|| format!("分类 {}", index + 1));
        let edit_cls = wide("EDIT");
        let edit_text = wide(&name);
        let name_edit = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(edit_cls.as_ptr()),
            PCWSTR(edit_text.as_ptr()),
            WINDOW_STYLE(WS_CHILD.0 | visible | WS_BORDER.0 | ES_AUTOHSCROLL as u32 | WS_TABSTOP.0),
            36,
            y,
            146,
            28,
            Some(parent),
            Some(HMENU((ID_CATEGORY_NAME_BASE + index) as isize as *mut _)),
            Some(deskpin::module_handle()),
            None,
        )
        .unwrap_or_default();
        let _ = SendMessageW(
            name_edit,
            EM_SETLIMITTEXT,
            Some(WPARAM(24)),
            Some(LPARAM(0)),
        );
        let _ = SendMessageW(
            name_edit,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
        let icon_cls = wide("COMBOBOX");
        let icon_combo = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(icon_cls.as_ptr()),
            PCWSTR::null(),
            WINDOW_STYLE(WS_CHILD.0 | visible | CBS_DROPDOWNLIST as u32),
            190,
            y,
            292,
            160,
            Some(parent),
            Some(HMENU((ID_CATEGORY_ICON_BASE + index) as isize as *mut _)),
            Some(deskpin::module_handle()),
            None,
        )
        .unwrap_or_default();
        let _ = SetWindowTheme(icon_combo, PCWSTR(theme.as_ptr()), PCWSTR::null());
        let _ = SendMessageW(
            icon_combo,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
        let icon = quadrant.map(|q| q.category_icon.as_str()).unwrap_or("auto");
        fill_icon_combo(icon_combo, index as usize, icon);
        make(
            "BUTTON",
            "自定义",
            WS_CHILD.0 | visible,
            490,
            y,
            98,
            28,
            ID_CATEGORY_FILE_BASE + index,
        );
    }
    let _ = SendMessageW(
        count_combo,
        CB_SETCURSEL,
        Some(WPARAM((shown - 2) as usize)),
        Some(LPARAM(0)),
    );

    let (weather_y, _, _) = category_layout(shown);
    make(
        "BUTTON",
        "定位并更新天气",
        base,
        36,
        weather_y,
        190,
        32,
        ID_REQUEST_ENVIRONMENT,
    );
    make(
        "STATIC",
        "使用系统定位，坐标发送至 Open-Meteo",
        base,
        36,
        weather_y + 40,
        552,
        24,
        ID_WEATHER_NOTE,
    );
    let button_y = window_height - 84;
    make(
        "BUTTON",
        "保存更改",
        base | BS_DEFPUSHBUTTON as u32,
        462,
        button_y,
        126,
        36,
        ID_SAVE,
    );
    make("BUTTON", "取消", base, 352, button_y, 96, 36, ID_CANCEL);
    make(
        "BUTTON",
        "打开配置文件…",
        base,
        36,
        button_y,
        210,
        36,
        ID_OPENCFG,
    );

    CheckDlgButton(
        parent,
        ID_SWITCH,
        if switch_on_click {
            BST_CHECKED
        } else {
            BST_UNCHECKED
        },
    )
    .ok();
    CheckDlgButton(
        parent,
        ID_AUTOCLOSE,
        if auto_collapse {
            BST_CHECKED
        } else {
            BST_UNCHECKED
        },
    )
    .ok();
    CheckDlgButton(
        parent,
        ID_AUTOSTART,
        if auto_start {
            BST_CHECKED
        } else {
            BST_UNCHECKED
        },
    )
    .ok();
}

fn icon_paths() -> std::sync::MutexGuard<'static, [String; 8]> {
    CATEGORY_ICON_PATHS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn icon_file_caption(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_string())
}

unsafe fn fill_icon_combo(combo: HWND, index: usize, icon: &str) {
    let _ = SendMessageW(combo, CB_RESETCONTENT, Some(WPARAM(0)), Some(LPARAM(0)));
    for (_, label) in ICON_PRESETS {
        let text = wide(label);
        let _ = SendMessageW(
            combo,
            CB_ADDSTRING,
            Some(WPARAM(0)),
            Some(LPARAM(text.as_ptr() as isize)),
        );
    }
    let custom = crate::icons::custom_icon_path(icon);
    if let Some(path) = custom {
        if index < 8 {
            icon_paths()[index] = path.to_string();
        }
        let caption = wide(&icon_file_caption(path));
        let _ = SendMessageW(
            combo,
            CB_ADDSTRING,
            Some(WPARAM(0)),
            Some(LPARAM(caption.as_ptr() as isize)),
        );
    }
    let selected = custom
        .map(|_| ICON_PRESETS.len())
        .or_else(|| ICON_PRESETS.iter().position(|(value, _)| *value == icon))
        .unwrap_or(0);
    let _ = SendMessageW(combo, CB_SETCURSEL, Some(WPARAM(selected)), Some(LPARAM(0)));
}

unsafe fn choose_category_icon(hwnd: HWND, index: usize) {
    let Some(path) = pick_icon_file(hwnd) else {
        return;
    };
    if index < 8 {
        icon_paths()[index] = path.clone();
    }
    fill_icon_combo(
        GetDlgItem(Some(hwnd), ID_CATEGORY_ICON_BASE + index as i32).unwrap_or_default(),
        index,
        &path,
    );
}

unsafe fn pick_icon_file(owner: HWND) -> Option<String> {
    use windows::core::Interface;
    use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FileOpenDialog, IFileDialog, IFileOpenDialog, FILEOPENDIALOGOPTIONS, FOS_FILEMUSTEXIST,
        FOS_FORCEFILESYSTEM, SIGDN_FILESYSPATH,
    };
    let open: IFileOpenDialog =
        CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
    let dialog: IFileDialog = open.cast().ok()?;
    dialog
        .SetOptions(FILEOPENDIALOGOPTIONS(
            FOS_FILEMUSTEXIST.0 | FOS_FORCEFILESYSTEM.0,
        ))
        .ok()?;
    let title = wide("选择分类图标");
    dialog.SetTitle(PCWSTR(title.as_ptr())).ok()?;
    let filter_name = wide("程序、图标或图片");
    let filter_spec = wide("*.exe;*.ico;*.dll;*.png;*.jpg;*.jpeg;*.bmp;*.webp");
    let filters = [COMDLG_FILTERSPEC {
        pszName: PCWSTR(filter_name.as_ptr()),
        pszSpec: PCWSTR(filter_spec.as_ptr()),
    }];
    dialog.SetFileTypes(&filters).ok()?;
    dialog.Show(Some(owner)).ok()?;
    let item = dialog.GetResult().ok()?;
    let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
    if name.0.is_null() {
        return None;
    }
    let mut len = 0usize;
    while *name.0.add(len) != 0 {
        len += 1;
    }
    let path = String::from_utf16_lossy(std::slice::from_raw_parts(name.0, len));
    CoTaskMemFree(Some(name.0 as *const _));
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

fn category_layout(count: i32) -> (i32, i32, i32) {
    let count = count.clamp(2, CATEGORY_SLOTS);
    let weather = 490 + count * 34;
    let height = weather + 176;
    (weather, height - 84, height)
}

unsafe fn category_count_from_ui(hwnd: HWND) -> i32 {
    let selected = SendMessageW(
        GetDlgItem(Some(hwnd), ID_CATEGORY_COUNT).unwrap_or_default(),
        CB_GETCURSEL,
        Some(WPARAM(0)),
        Some(LPARAM(0)),
    )
    .0;
    if selected < 0 {
        return 4;
    }
    (selected as i32 + 2).clamp(2, CATEGORY_SLOTS)
}

unsafe fn window_text(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    let count = GetWindowTextW(hwnd, &mut buf);
    String::from_utf16_lossy(&buf[..count.max(0) as usize])
}

unsafe fn move_control(parent: HWND, id: i32, x: i32, y: i32, w: i32, h: i32) {
    let child = GetDlgItem(Some(parent), id).unwrap_or_default();
    if child.0.is_null() {
        return;
    }
    let _ = SetWindowPos(child, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
}

unsafe fn sync_category_rows(hwnd: HWND) {
    let count = category_count_from_ui(hwnd);
    for index in 0..CATEGORY_SLOTS {
        let show = if index < count { SW_SHOW } else { SW_HIDE };
        for id in [
            ID_CATEGORY_NAME_BASE + index,
            ID_CATEGORY_ICON_BASE + index,
            ID_CATEGORY_FILE_BASE + index,
        ] {
            let child = GetDlgItem(Some(hwnd), id).unwrap_or_default();
            if !child.0.is_null() {
                let _ = ShowWindow(child, show);
            }
        }
    }
    let (weather, buttons, height) = category_layout(count);
    move_control(hwnd, ID_REQUEST_ENVIRONMENT, 36, weather, 190, 32);
    move_control(hwnd, ID_WEATHER_NOTE, 36, weather + 40, 552, 24);
    move_control(hwnd, ID_OPENCFG, 36, buttons, 210, 36);
    move_control(hwnd, ID_CANCEL, 352, buttons, 96, 36);
    move_control(hwnd, ID_SAVE, 462, buttons, 126, 36);
    let _ = SetWindowPos(
        hwnd,
        None,
        0,
        0,
        640,
        height,
        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
    let _ = InvalidateRect(Some(hwnd), None, true);
}

/// 读勾选 → 落盘 config.json → 通知主窗口重载
unsafe fn save(hwnd: HWND) {
    let switch_on_click = IsDlgButtonChecked(hwnd, ID_SWITCH) != 0;
    let auto_collapse_after_open = IsDlgButtonChecked(hwnd, ID_AUTOCLOSE) != 0;
    let auto_start = IsDlgButtonChecked(hwnd, ID_AUTOSTART) != 0;
    let (mut cfg, note) = Config::load();
    if !note.is_empty() {
        eprintln!("[ring-dock] {note}");
    }
    crate::shortcut::retarget_config(&mut cfg);
    cfg.switch_panel_on_click = switch_on_click;
    cfg.auto_collapse_after_open = auto_collapse_after_open;
    cfg.auto_start = auto_start;
    let display_mode = SendMessageW(
        GetDlgItem(Some(hwnd), ID_CATEGORY_MODE).unwrap_or_default(),
        CB_GETCURSEL,
        Some(WPARAM(0)),
        Some(LPARAM(0)),
    )
    .0 as usize;
    cfg.category_display_mode = match display_mode {
        0 => CategoryDisplayMode::Icon,
        1 => CategoryDisplayMode::Text,
        _ => CategoryDisplayMode::Both,
    };
    let selected = SendMessageW(
        GetDlgItem(Some(hwnd), ID_ICON_STYLE).unwrap_or_default(),
        CB_GETCURSEL,
        Some(WPARAM(0)),
        Some(LPARAM(0)),
    )
    .0 as usize;
    cfg.icon_style = match selected {
        1 => IconStyle::Original,
        2 => IconStyle::Tinted,
        _ => IconStyle::Unified,
    };
    let requested = category_count_from_ui(hwnd) as usize;
    let mut edits = Vec::with_capacity(requested);
    let paths = icon_paths();
    for index in 0..requested {
        let name = window_text(
            GetDlgItem(Some(hwnd), ID_CATEGORY_NAME_BASE + index as i32).unwrap_or_default(),
        );
        let selected = SendMessageW(
            GetDlgItem(Some(hwnd), ID_CATEGORY_ICON_BASE + index as i32).unwrap_or_default(),
            CB_GETCURSEL,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        )
        .0;
        let icon = if selected >= 0 && (selected as usize) < ICON_PRESETS.len() {
            ICON_PRESETS[selected as usize].0.to_string()
        } else {
            paths.get(index).cloned().unwrap_or_default()
        };
        edits.push((name, icon));
    }
    drop(paths);
    let applied = cfg.apply_categories(requested, &edits);
    if applied > requested {
        let text = wide("末尾分类里还有收藏，这些分类已保留。先把条目移到其他分类，再减少数量。");
        let title = wide("分类数量");
        let _ = MessageBoxW(
            Some(hwnd),
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONINFORMATION,
        );
    }
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
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            FillRect(dc, &rect, HBRUSH(BG_BRUSH.load(Ordering::SeqCst) as *mut _));
            let brush = CreateSolidBrush(COLORREF(0x0041362b));
            for y in [96, 266, rect.bottom - 68] {
                FillRect(
                    dc,
                    &RECT {
                        left: 32,
                        top: y,
                        right: rect.right - 32,
                        bottom: y + 1,
                    },
                    brush,
                );
            }
            let _ = DeleteObject(brush.into());
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }

        WM_DRAWITEM => {
            let item = &*(l.0 as *const DRAWITEMSTRUCT);
            let primary = item.CtlID == ID_SAVE as u32;
            let pressed = item.itemState.0 & ODS_SELECTED.0 != 0;
            let color = if primary {
                if pressed {
                    COLORREF(0x00957a3a)
                } else {
                    COLORREF(0x00cbb85a)
                }
            } else {
                COLORREF(0x0041362b)
            };
            let brush = CreateSolidBrush(color);
            let old_brush = SelectObject(item.hDC, brush.into());
            let old_pen = SelectObject(item.hDC, GetStockObject(NULL_PEN));
            let r = item.rcItem;
            let _ = RoundRect(item.hDC, r.left, r.top, r.right, r.bottom, 12, 12);
            SelectObject(item.hDC, old_brush);
            SelectObject(item.hDC, old_pen);
            let _ = DeleteObject(brush.into());
            SetBkMode(item.hDC, TRANSPARENT);
            SetTextColor(item.hDC, if primary { BACKGROUND } else { FOREGROUND });
            let old_font =
                SelectObject(item.hDC, HGDIOBJ(UI_FONT.load(Ordering::SeqCst) as *mut _));
            let mut text = [0u16; 128];
            let count = GetWindowTextW(item.hwndItem, &mut text);
            let mut rect = r;
            DrawTextW(
                item.hDC,
                &mut text[..count as usize],
                &mut rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            SelectObject(item.hDC, old_font);
            if item.itemState.0 & ODS_FOCUS.0 != 0 {
                let focus = RECT {
                    left: r.left + 4,
                    top: r.top + 4,
                    right: r.right - 4,
                    bottom: r.bottom - 4,
                };
                let _ = DrawFocusRect(item.hDC, &focus);
            }
            LRESULT(1)
        }

        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLORLISTBOX | WM_CTLCOLOREDIT => {
            let dc = HDC(w.0 as *mut _);
            let mut label = [0u16; 128];
            let count = GetWindowTextW(HWND(l.0 as *mut _), &mut label);
            let label = String::from_utf16_lossy(&label[..count as usize]);
            SetTextColor(
                dc,
                if label.starts_with("让桌面") || label.starts_with("使用系统") {
                    COLORREF(0x00b5a79a)
                } else if label.starts_with("01") || label.starts_with("02") {
                    COLORREF(0x00d9c271)
                } else {
                    FOREGROUND
                },
            );
            SetBkColor(dc, BACKGROUND);
            LRESULT(BG_BRUSH.load(Ordering::SeqCst))
        }

        WM_COMMAND => {
            let id = (w.0 & 0xFFFF) as i32;
            let code = (w.0 >> 16) as u16;
            if id == ID_CATEGORY_COUNT && code == CBN_SELCHANGE as u16 {
                sync_category_rows(hwnd);
            }
            if (ID_CATEGORY_FILE_BASE..ID_CATEGORY_FILE_BASE + CATEGORY_SLOTS).contains(&id) {
                choose_category_icon(hwnd, (id - ID_CATEGORY_FILE_BASE) as usize);
            }
            match id {
                ID_SAVE => {
                    save(hwnd);
                    let _ = DestroyWindow(hwnd);
                }
                ID_CANCEL => {
                    let _ = DestroyWindow(hwnd);
                }
                ID_OPENCFG => open::open_config_file(),
                ID_REQUEST_ENVIRONMENT => {
                    let main = MAIN_HWND.load(Ordering::SeqCst);
                    if main != 0 {
                        let _ = PostMessageW(
                            Some(HWND(main as *mut _)),
                            WM_REQUEST_ENVIRONMENT,
                            WPARAM(0),
                            LPARAM(hwnd.0 as isize),
                        );
                    }
                }
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
