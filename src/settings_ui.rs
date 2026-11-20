//! 设置界面（独立顶层窗口，托盘菜单打开）：交互开关 → 写 config.json → 通知主窗口重载。
//! 纯 Win32 控件（BUTTON/STATIC），不引入任何 UI 框架。
use crate::config::{CategoryDisplayMode, Config, IconStyle, Quadrant};
use crate::open;
use crate::sys::wide;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, POINT, RECT};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::{EM_SETLIMITTEXT, EM_SETMARGINS};
use windows::Win32::UI::Controls::{
    SetWindowTheme, COMBOBOXINFO, DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_SELECTED, ODT_COMBOBOX,
    GetComboBoxInfo,
};
use windows::Win32::UI::WindowsAndMessaging::*;

/// 发给主窗口：设置已保存，请重新加载 config.json
pub const WM_CONFIG_SAVED: u32 = WM_APP + 2;
pub const WM_REQUEST_ENVIRONMENT: u32 = WM_APP + 44;

static TITLE_FONT: AtomicIsize = AtomicIsize::new(0);
static UI_FONT: AtomicIsize = AtomicIsize::new(0);
static BG_BRUSH: AtomicIsize = AtomicIsize::new(0);
static GLASS: AtomicBool = AtomicBool::new(false);
static GLASS_BMP: AtomicIsize = AtomicIsize::new(0);
const BACKGROUND: COLORREF = COLORREF(0x001c130e);
const FOREGROUND: COLORREF = COLORREF(0x00fff7f0);
const GLASS_RIM: COLORREF = COLORREF(0x00e2d0bc);
const GLASS_SPEC: COLORREF = COLORREF(0x00fff6ea);
#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(
        hwnd: *mut std::ffi::c_void,
        attribute: u32,
        value: *const std::ffi::c_void,
        size: u32,
    ) -> i32;
    fn DwmFlush() -> i32;
}
const CLASS_NAME: &str = "RingDockSettings";

// 控件 ID
const ID_SWITCH: i32 = 1001;
const ID_AUTOCLOSE: i32 = 1002;
const ID_AUTOSTART: i32 = 1005;
const ID_ICON_SEG: i32 = 1500;
const ID_MODE_SEG: i32 = 1510;
const ID_COUNT_SEG: i32 = 1520;
const LABEL_X: i32 = 28;
const CONTROL_X: i32 = 156;
const ROW_STEP: i32 = 34;
const CLIENT_W: i32 = 640;
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
const ID_WEATHER_LABEL: i32 = 2006;
const ID_CLOSE: i32 = 2007;

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
        let (_, _, client_h) = category_layout(row_count);
        let (ww, hh) = outer_size(CLIENT_W, client_h);
        let wx = x + ((aw - ww) / 2).max(0);
        let wy = y + ((ah - hh) / 2).max(0);

        let cls = wide(CLASS_NAME);
        let title = wide("ring-dock 设置");
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            PCWSTR(cls.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_POPUP,
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
        enable_liquid_glass(hwnd);
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
    FIELD_BRUSH.store(CreateSolidBrush(FIELD).0 as isize, Ordering::SeqCst);
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
        hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        lpszClassName: PCWSTR(cname.as_ptr()),
        ..Default::default()
    };
    let _ = RegisterClassW(&wc);
}

const ACCENT: COLORREF = COLORREF(0x0062B08F);
const ACCENT_INK: COLORREF = COLORREF(0x00140f0c);
const PILL: COLORREF = COLORREF(0x00403024);
const FIELD: COLORREF = COLORREF(0x001e140e);

unsafe fn enable_liquid_glass(hwnd: HWND) {
    let round: i32 = 2;
    let border: u32 = 0x00e8d6ba;
    let _ = DwmSetWindowAttribute(hwnd.0, 33, &round as *const _ as *const _, 4);
    let _ = DwmSetWindowAttribute(hwnd.0, 34, &border as *const _ as *const _, 4);
    capture_glass(hwnd);
}

fn glass_on() -> bool {
    GLASS.load(Ordering::SeqCst)
}

fn box_pass(src: &[u8], dst: &mut [u8], w: i32, h: i32, radius: i32, horizontal: bool) {
    let r = radius.max(1);
    let span = (r * 2 + 1) as u32;
    if horizontal {
        for y in 0..h {
            let mut sum = [0u32; 3];
            let row = (y * w) as isize;
            for k in -r..=r {
                let x = k.clamp(0, w - 1) as isize;
                let i = ((row + x) * 4) as usize;
                sum[0] += src[i] as u32;
                sum[1] += src[i + 1] as u32;
                sum[2] += src[i + 2] as u32;
            }
            for x in 0..w {
                let o = ((row + x as isize) * 4) as usize;
                dst[o] = (sum[0] / span) as u8;
                dst[o + 1] = (sum[1] / span) as u8;
                dst[o + 2] = (sum[2] / span) as u8;
                dst[o + 3] = 255;
                let remove = (x - r).clamp(0, w - 1) as isize;
                let add = (x + r + 1).clamp(0, w - 1) as isize;
                let ri = ((row + remove) * 4) as usize;
                let ai = ((row + add) * 4) as usize;
                sum[0] = sum[0] + src[ai] as u32 - src[ri] as u32;
                sum[1] = sum[1] + src[ai + 1] as u32 - src[ri + 1] as u32;
                sum[2] = sum[2] + src[ai + 2] as u32 - src[ri + 2] as u32;
            }
        }
    } else {
        for x in 0..w {
            let mut sum = [0u32; 3];
            for k in -r..=r {
                let y = k.clamp(0, h - 1);
                let i = ((y * w + x) * 4) as usize;
                sum[0] += src[i] as u32;
                sum[1] += src[i + 1] as u32;
                sum[2] += src[i + 2] as u32;
            }
            for y in 0..h {
                let o = ((y * w + x) * 4) as usize;
                dst[o] = (sum[0] / span) as u8;
                dst[o + 1] = (sum[1] / span) as u8;
                dst[o + 2] = (sum[2] / span) as u8;
                dst[o + 3] = 255;
                let remove = (y - r).clamp(0, h - 1);
                let add = (y + r + 1).clamp(0, h - 1);
                let ri = ((remove * w + x) * 4) as usize;
                let ai = ((add * w + x) * 4) as usize;
                sum[0] = sum[0] + src[ai] as u32 - src[ri] as u32;
                sum[1] = sum[1] + src[ai + 1] as u32 - src[ri + 1] as u32;
                sum[2] = sum[2] + src[ai + 2] as u32 - src[ri + 2] as u32;
            }
        }
    }
}

fn frost_pixels(pixels: &mut [u8], w: i32, h: i32) {
    if w <= 1 || h <= 1 {
        return;
    }
    let n = (w as usize) * (h as usize) * 4;
    if pixels.len() < n {
        return;
    }
    let mut tmp = vec![0u8; n];
    for _ in 0..2 {
        box_pass(pixels, &mut tmp, w, h, 16, true);
        box_pass(&tmp, pixels, w, h, 16, false);
    }
    for y in 0..h {
        let lift = if y < 36 {
            ((36 - y) * 48 / 36) as i16
        } else {
            0
        };
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            for (channel, ink) in [(0usize, 30u16), (1, 22u16), (2, 16u16)] {
                let mixed = pixels[i + channel] as u16 * 36 / 100 + ink * 64 / 100;
                pixels[i + channel] = (mixed as i16 + lift).clamp(0, 255) as u8;
            }
            pixels[i + 3] = 255;
        }
    }
}

unsafe fn capture_glass(hwnd: HWND) {
    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);
    let w = client.right;
    let h = client.bottom;
    if w < 8 || h < 8 {
        return;
    }
    let mut origin = POINT { x: 0, y: 0 };
    let _ = ClientToScreen(hwnd, &mut origin);
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(Some(screen));
    let bmp = CreateCompatibleBitmap(screen, w, h);
    if bmp.0.is_null() {
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        return;
    }
    let raw = bmp.0 as isize;
    let old = SelectObject(mem, bmp.into());
    let _ = BitBlt(mem, 0, 0, w, h, Some(screen), origin.x, origin.y, SRCCOPY);
    SelectObject(mem, old);
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        bmiColors: [RGBQUAD::default()],
    };
    let mut pixels = vec![0u8; (w as usize) * (h as usize) * 4];
    if GetDIBits(
        mem,
        HBITMAP(raw as *mut _),
        0,
        h as u32,
        Some(pixels.as_mut_ptr() as *mut _),
        &mut info,
        DIB_RGB_COLORS,
    ) > 0
    {
        frost_pixels(&mut pixels, w, h);
        let _ = SetDIBits(
            Some(mem),
            HBITMAP(raw as *mut _),
            0,
            h as u32,
            pixels.as_ptr() as *const _,
            &info,
            DIB_RGB_COLORS,
        );
    }
    let _ = DeleteDC(mem);
    ReleaseDC(None, screen);
    let previous = GLASS_BMP.swap(raw, Ordering::SeqCst);
    if previous != 0 {
        let _ = DeleteObject(HGDIOBJ(previous as *mut _));
    }
    GLASS.store(true, Ordering::SeqCst);
}

unsafe fn refresh_glass(hwnd: HWND) {
    let hidden: i32 = 1;
    let _ = DwmSetWindowAttribute(hwnd.0, 13, &hidden as *const _ as *const _, 4);
    let _ = DwmFlush();
    capture_glass(hwnd);
    let shown: i32 = 0;
    let _ = DwmSetWindowAttribute(hwnd.0, 13, &shown as *const _ as *const _, 4);
    let _ = InvalidateRect(Some(hwnd), None, false);
}

unsafe fn blit_glass(dc: HDC, hwnd: HWND, width: i32, height: i32) {
    let bmp = GLASS_BMP.load(Ordering::SeqCst);
    if bmp == 0 {
        let brush = CreateSolidBrush(BACKGROUND);
        FillRect(
            dc,
            &RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            },
            brush,
        );
        let _ = DeleteObject(brush.into());
        return;
    }
    let parent = GetParent(hwnd).unwrap_or_default();
    let mut origin = POINT { x: 0, y: 0 };
    let _ = ClientToScreen(hwnd, &mut origin);
    let mut parent_origin = POINT { x: 0, y: 0 };
    if parent.0.is_null() {
        let _ = ClientToScreen(hwnd, &mut parent_origin);
        origin = parent_origin;
    } else {
        let _ = ClientToScreen(parent, &mut parent_origin);
    }
    let mem = CreateCompatibleDC(Some(dc));
    let old = SelectObject(mem, HGDIOBJ(bmp as *mut _));
    let _ = BitBlt(
        dc,
        0,
        0,
        width,
        height,
        Some(mem),
        origin.x - parent_origin.x,
        origin.y - parent_origin.y,
        SRCCOPY,
    );
    SelectObject(mem, old);
    let _ = DeleteDC(mem);
}
static FIELD_BRUSH: AtomicIsize = AtomicIsize::new(0);
static COMBO_PROC: AtomicIsize = AtomicIsize::new(0);
static LIST_PROC: AtomicIsize = AtomicIsize::new(0);

unsafe fn set_on(hwnd: HWND, on: bool) {
    if hwnd.0.is_null() {
        return;
    }
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, if on { 1 } else { 0 });
    let _ = InvalidateRect(Some(hwnd), None, false);
}

unsafe fn is_on(hwnd: HWND) -> bool {
    !hwnd.0.is_null() && GetWindowLongPtrW(hwnd, GWLP_USERDATA) != 0
}

fn place_segments(
    make: &impl Fn(&str, &str, u32, i32, i32, i32, i32, i32) -> HWND,
    labels: &[&str],
    id_base: i32,
    selected: usize,
    x: i32,
    y: i32,
    width: i32,
) {
    let style = WS_CHILD.0 | WS_VISIBLE.0;
    for (index, label) in labels.iter().enumerate() {
        let button = make(
            "BUTTON",
            label,
            style,
            x + index as i32 * (width + 6),
            y,
            width,
            30,
            id_base + index as i32,
        );
        unsafe { set_on(button, index == selected) };
    }
}

unsafe fn select_segment(parent: HWND, id_base: i32, count: i32, chosen: i32) {
    for index in 0..count {
        let id = id_base + index;
        set_on(
            GetDlgItem(Some(parent), id).unwrap_or_default(),
            id == chosen,
        );
    }
}

unsafe fn segment_index(parent: HWND, id_base: i32, count: i32) -> usize {
    for index in 0..count {
        if is_on(GetDlgItem(Some(parent), id_base + index).unwrap_or_default()) {
            return index as usize;
        }
    }
    0
}

fn category_rows_y() -> i32 {
    340
}

unsafe extern "system" fn combo_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_CTLCOLORLISTBOX || msg == WM_CTLCOLOREDIT || msg == WM_CTLCOLORSTATIC {
        let dc = HDC(w.0 as *mut _);
        SetTextColor(dc, FOREGROUND);
        SetBkColor(dc, FIELD);
        return LRESULT(FIELD_BRUSH.load(Ordering::SeqCst));
    }
    if msg == WM_NCPAINT {
        paint_combo_frame(hwnd);
        return LRESULT(0);
    }
    if msg == WM_ERASEBKGND {
        return LRESULT(1);
    }
    if msg == WM_PAINT {
        paint_combo_face(hwnd);
        return LRESULT(0);
    }
    let prev = COMBO_PROC.load(Ordering::SeqCst);
    if prev == 0 {
        return DefWindowProcW(hwnd, msg, w, l);
    }
    CallWindowProcW(
        std::mem::transmute::<isize, WNDPROC>(prev),
        hwnd,
        msg,
        w,
        l,
    )
}

unsafe fn paint_combo_frame(hwnd: HWND) {
    let dc = GetWindowDC(Some(hwnd));
    let mut window = RECT::default();
    let _ = GetWindowRect(hwnd, &mut window);
    let mut origin = POINT { x: 0, y: 0 };
    let _ = ClientToScreen(hwnd, &mut origin);
    let left = origin.x - window.left;
    let top = origin.y - window.top;
    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);
    let width = window.right - window.left;
    let height = window.bottom - window.top;
    let brush = CreateSolidBrush(FIELD);
    for strip in [
        RECT { left: 0, top: 0, right: width, bottom: top.max(0) },
        RECT { left: 0, top: 0, right: left.max(0), bottom: height },
        RECT {
            left: left + client.right,
            top: 0,
            right: width,
            bottom: height,
        },
        RECT {
            left: 0,
            top: top + client.bottom,
            right: width,
            bottom: height,
        },
    ] {
        if strip.right > strip.left && strip.bottom > strip.top {
            FillRect(dc, &strip, brush);
        }
    }
    let _ = DeleteObject(brush.into());
    ReleaseDC(Some(hwnd), dc);
}

unsafe fn paint_combo_face(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let dc = BeginPaint(hwnd, &mut ps);
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    let brush = CreateSolidBrush(FIELD);
    FillRect(dc, &rect, brush);
    let _ = DeleteObject(brush.into());
    glass_rim(dc, rect, 10, false, false);
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, FOREGROUND);
    let font = SendMessageW(hwnd, WM_GETFONT, Some(WPARAM(0)), Some(LPARAM(0))).0;
    let font = if font == 0 {
        UI_FONT.load(Ordering::SeqCst)
    } else {
        font
    };
    let old_font = SelectObject(dc, HGDIOBJ(font as *mut _));
    let selected = SendMessageW(hwnd, CB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0;
    if selected >= 0 {
        let mut text = [0u16; 128];
        let count = SendMessageW(
            hwnd,
            CB_GETLBTEXT,
            Some(WPARAM(selected as usize)),
            Some(LPARAM(text.as_mut_ptr() as isize)),
        )
        .0;
        if count > 0 {
            let mut text_rect = RECT {
                left: rect.left + 10,
                top: rect.top,
                right: rect.right - 28,
                bottom: rect.bottom,
            };
            DrawTextW(
                dc,
                &mut text[..count as usize],
                &mut text_rect,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
        }
    }
    let pen = CreatePen(PS_SOLID, 1, COLORREF(0x00b5a79a));
    let old_pen = SelectObject(dc, pen.into());
    let cx = rect.right - 14;
    let cy = (rect.top + rect.bottom) / 2;
    let _ = MoveToEx(dc, cx - 4, cy - 2, None);
    let _ = LineTo(dc, cx, cy + 2);
    let _ = LineTo(dc, cx + 4, cy - 2);
    SelectObject(dc, old_pen);
    SelectObject(dc, old_font);
    let _ = DeleteObject(pen.into());
    let _ = EndPaint(hwnd, &ps);
}

unsafe fn subclass_combo(combo: HWND) {
    if combo.0.is_null() {
        return;
    }
    let previous = SetWindowLongPtrW(
        combo,
        GWLP_WNDPROC,
        std::mem::transmute::<WNDPROC, isize>(Some(combo_proc)),
    );
    let _ = COMBO_PROC.compare_exchange(0, previous, Ordering::SeqCst, Ordering::SeqCst);
    subclass_combo_list(combo);
}

unsafe extern "system" fn list_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_ERASEBKGND {
        let dc = HDC(w.0 as *mut _);
        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let brush = CreateSolidBrush(FIELD);
        FillRect(dc, &rect, brush);
        let _ = DeleteObject(brush.into());
        return LRESULT(1);
    }
    if msg == WM_NCPAINT {
        paint_combo_frame(hwnd);
        return LRESULT(0);
    }
    let prev = LIST_PROC.load(Ordering::SeqCst);
    if prev == 0 {
        return DefWindowProcW(hwnd, msg, w, l);
    }
    CallWindowProcW(std::mem::transmute::<isize, WNDPROC>(prev), hwnd, msg, w, l)
}

unsafe fn subclass_combo_list(combo: HWND) {
    let mut info = COMBOBOXINFO {
        cbSize: std::mem::size_of::<COMBOBOXINFO>() as u32,
        ..Default::default()
    };
    if GetComboBoxInfo(combo, &mut info).is_err() || info.hwndList.0.is_null() {
        return;
    }
    let ours = std::mem::transmute::<WNDPROC, isize>(Some(list_proc));
    if GetWindowLongPtrW(info.hwndList, GWLP_WNDPROC) == ours {
        return;
    }
    let previous = SetWindowLongPtrW(info.hwndList, GWLP_WNDPROC, ours);
    let _ = LIST_PROC.compare_exchange(0, previous, Ordering::SeqCst, Ordering::SeqCst);
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
    _window_height: i32,
) {
    let font = HFONT(UI_FONT.load(Ordering::SeqCst) as *mut _);
    let make = |class: &str, text: &str, style: u32, x: i32, y: i32, w: i32, h: i32, id: i32| {
        let cls = wide(class);
        let tx = wide(text);
        let c = CreateWindowExW(
            WINDOW_EX_STYLE(if class == "STATIC" {
                WS_EX_TRANSPARENT.0
            } else {
                0
            }),
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
            Some(WPARAM(if text == "偏好设置" {
                TITLE_FONT.load(Ordering::SeqCst) as usize
            } else {
                font.0 as usize
            })),
            Some(LPARAM(1)),
        );
        c
    };

    let base = WS_CHILD.0 | WS_VISIBLE.0;
    make("STATIC", "偏好设置", base, LABEL_X, 18, 400, 32, 0);
    make("BUTTON", "×", base, CLIENT_W - 48, 16, 32, 28, ID_CLOSE);
    let mut y = 62;
    for (label, id, on) in [
        ("直接切换", ID_SWITCH, switch_on_click),
        ("打开后收起", ID_AUTOCLOSE, auto_collapse),
        ("登录时启动", ID_AUTOSTART, auto_start),
    ] {
        make("STATIC", label, base, LABEL_X, y + 2, 120, 24, 0);
        let toggle = make("BUTTON", "", base, CONTROL_X, y, 48, 26, id);
        set_on(toggle, on);
        y += 40;
    }
    y += 8;
    let icon_selected = match icon_style {
        IconStyle::Unified => 0,
        IconStyle::Original => 1,
        IconStyle::Tinted => 2,
    };
    make("STATIC", "条目图标", base, LABEL_X, y + 4, 120, 24, 0);
    place_segments(
        &make,
        &["统一", "原图标", "冷色"],
        ID_ICON_SEG,
        icon_selected,
        CONTROL_X,
        y,
        88,
    );
    y += 40;
    let mode_selected = match category_display_mode {
        CategoryDisplayMode::Icon => 0,
        CategoryDisplayMode::Text => 1,
        CategoryDisplayMode::Both => 2,
    };
    make("STATIC", "圆环显示", base, LABEL_X, y + 4, 120, 24, 0);
    place_segments(
        &make,
        &["图标", "文字", "图文"],
        ID_MODE_SEG,
        mode_selected,
        CONTROL_X,
        y,
        88,
    );
    y += 40;
    make("STATIC", "分类数量", base, LABEL_X, y + 4, 120, 24, 0);
    let count_labels = ["2", "3", "4", "5", "6", "7", "8"];
    let shown = quadrants.len().clamp(2, CATEGORY_SLOTS as usize) as i32;
    place_segments(
        &make,
        &count_labels,
        ID_COUNT_SEG,
        (shown - 2) as usize,
        CONTROL_X,
        y,
        40,
    );
    y += 46;
    make("STATIC", "名称", base, LABEL_X, y, 140, 20, 0);
    make("STATIC", "图标", base, 176, y, 140, 20, 0);
    let rows_y = y + 24;
    for index in 0..CATEGORY_SLOTS {
        let y = rows_y + index * ROW_STEP;
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
            WINDOW_STYLE(WS_CHILD.0 | visible | ES_AUTOHSCROLL as u32 | WS_TABSTOP.0),
            LABEL_X,
            y,
            140,
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
            EM_SETMARGINS,
            Some(WPARAM(3)),
            Some(LPARAM(10 | (8 << 16))),
        );
        let empty_theme = wide("");
        let _ = SetWindowTheme(
            name_edit,
            PCWSTR(empty_theme.as_ptr()),
            PCWSTR(empty_theme.as_ptr()),
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
            WINDOW_STYLE(
                WS_CHILD.0 | visible | CBS_DROPDOWNLIST as u32 | CBS_HASSTRINGS as u32
                    | CBS_OWNERDRAWFIXED as u32,
            ),
            176,
            y,
            320,
            160,
            Some(parent),
            Some(HMENU((ID_CATEGORY_ICON_BASE + index) as isize as *mut _)),
            Some(deskpin::module_handle()),
            None,
        )
        .unwrap_or_default();
        let empty_theme = wide("");
        let _ = SetWindowTheme(
            icon_combo,
            PCWSTR(empty_theme.as_ptr()),
            PCWSTR(empty_theme.as_ptr()),
        );
        subclass_combo(icon_combo);
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
            504,
            y,
            108,
            28,
            ID_CATEGORY_FILE_BASE + index,
        );
    }
    let (weather_y, button_y, _) = category_layout(shown);
    make(
        "STATIC",
        "天气",
        base,
        LABEL_X,
        weather_y + 4,
        120,
        24,
        ID_WEATHER_LABEL,
    );
    make(
        "BUTTON",
        "定位并更新",
        base,
        CONTROL_X,
        weather_y,
        132,
        30,
        ID_REQUEST_ENVIRONMENT,
    );
    make(
        "STATIC",
        "使用系统定位，坐标发送至 Open-Meteo",
        base,
        LABEL_X,
        weather_y + 36,
        CLIENT_W - LABEL_X * 2,
        22,
        ID_WEATHER_NOTE,
    );
    let save_x = CLIENT_W - 28 - 112;
    make(
        "BUTTON",
        "保存更改",
        base | BS_DEFPUSHBUTTON as u32,
        save_x,
        button_y,
        112,
        34,
        ID_SAVE,
    );
    make("BUTTON", "取消", base, save_x - 96, button_y, 88, 34, ID_CANCEL);
    make(
        "BUTTON",
        "打开配置文件…",
        base,
        LABEL_X,
        button_y,
        168,
        34,
        ID_OPENCFG,
    );
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

fn outer_size(client_w: i32, client_h: i32) -> (i32, i32) {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: client_w,
        bottom: client_h,
    };
    unsafe {
        let _ = AdjustWindowRectEx(
            &mut rect,
            WS_POPUP,
            false,
            WS_EX_TOOLWINDOW,
        );
    }
    (rect.right - rect.left, rect.bottom - rect.top)
}

fn category_layout(count: i32) -> (i32, i32, i32) {
    let count = count.clamp(2, CATEGORY_SLOTS);
    let weather = category_rows_y() + count * ROW_STEP + 16;
    let buttons = weather + 72;
    let height = buttons + 54;
    (weather, buttons, height)
}

unsafe fn category_count_from_ui(hwnd: HWND) -> i32 {
    (segment_index(hwnd, ID_COUNT_SEG, 7) as i32 + 2).clamp(2, CATEGORY_SLOTS)
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
    move_control(hwnd, ID_WEATHER_LABEL, LABEL_X, weather + 4, 120, 24);
    move_control(hwnd, ID_REQUEST_ENVIRONMENT, CONTROL_X, weather, 132, 30);
    move_control(
        hwnd,
        ID_WEATHER_NOTE,
        LABEL_X,
        weather + 36,
        CLIENT_W - LABEL_X * 2,
        22,
    );
    let save_x = CLIENT_W - 28 - 112;
    move_control(hwnd, ID_OPENCFG, LABEL_X, buttons, 168, 34);
    move_control(hwnd, ID_CANCEL, save_x - 96, buttons, 88, 34);
    move_control(hwnd, ID_SAVE, save_x, buttons, 112, 34);
    let (outer_w, outer_h) = outer_size(CLIENT_W, height);
    let _ = SetWindowPos(
        hwnd,
        None,
        0,
        0,
        outer_w,
        outer_h,
        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
    refresh_glass(hwnd);
}

/// 读勾选 → 落盘 config.json → 通知主窗口重载
unsafe fn save(hwnd: HWND) {
    let switch_on_click = is_on(GetDlgItem(Some(hwnd), ID_SWITCH).unwrap_or_default());
    let auto_collapse_after_open = is_on(GetDlgItem(Some(hwnd), ID_AUTOCLOSE).unwrap_or_default());
    let auto_start = is_on(GetDlgItem(Some(hwnd), ID_AUTOSTART).unwrap_or_default());
    let (mut cfg, note) = Config::load();
    if !note.is_empty() {
        eprintln!("[ring-dock] {note}");
    }
    crate::shortcut::retarget_config(&mut cfg);
    cfg.switch_panel_on_click = switch_on_click;
    cfg.auto_collapse_after_open = auto_collapse_after_open;
    cfg.auto_start = auto_start;
    let display_mode = segment_index(hwnd, ID_MODE_SEG, 3);
    cfg.category_display_mode = match display_mode {
        0 => CategoryDisplayMode::Icon,
        1 => CategoryDisplayMode::Text,
        _ => CategoryDisplayMode::Both,
    };
    let selected = segment_index(hwnd, ID_ICON_SEG, 3);
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

fn is_segment(id: i32) -> bool {
    (ID_ICON_SEG..ID_ICON_SEG + 3).contains(&id)
        || (ID_MODE_SEG..ID_MODE_SEG + 3).contains(&id)
        || (ID_COUNT_SEG..ID_COUNT_SEG + 7).contains(&id)
}

unsafe fn fill_round(dc: HDC, rect: RECT, color: COLORREF, radius: i32) {
    let brush = CreateSolidBrush(color);
    let old_brush = SelectObject(dc, brush.into());
    let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
    let _ = RoundRect(dc, rect.left, rect.top, rect.right, rect.bottom, radius, radius);
    SelectObject(dc, old_brush);
    SelectObject(dc, old_pen);
    let _ = DeleteObject(brush.into());
}

unsafe fn glass_rim(dc: HDC, rect: RECT, radius: i32, bright: bool, specular: bool) {
    let pen = CreatePen(
        PS_SOLID,
        1,
        if bright {
            COLORREF(0x00f2f8e4)
        } else {
            GLASS_RIM
        },
    );
    let old_pen = SelectObject(dc, pen.into());
    let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
    let _ = RoundRect(
        dc,
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        radius,
        radius,
    );
    if specular && rect.right - rect.left > 28 {
        let spec = CreatePen(PS_SOLID, 1, GLASS_SPEC);
        SelectObject(dc, spec.into());
        let inset = (radius / 3).max(10);
        let y = rect.top + 2;
        let _ = MoveToEx(dc, rect.left + inset, y, None);
        let _ = LineTo(dc, rect.right - inset, y);
        SelectObject(dc, old_pen);
        let _ = DeleteObject(spec.into());
    } else {
        SelectObject(dc, old_pen);
    }
    SelectObject(dc, old_brush);
    let _ = DeleteObject(pen.into());
}

unsafe fn paint_combo_item(item: &DRAWITEMSTRUCT) {
    let selected = item.itemState.0 & ODS_SELECTED.0 != 0;
    let brush = CreateSolidBrush(if selected { ACCENT } else { FIELD });
    FillRect(item.hDC, &item.rcItem, brush);
    let _ = DeleteObject(brush.into());
    SetBkMode(item.hDC, TRANSPARENT);
    SetTextColor(item.hDC, if selected { ACCENT_INK } else { FOREGROUND });
    let old_font = SelectObject(item.hDC, HGDIOBJ(UI_FONT.load(Ordering::SeqCst) as *mut _));
    let mut text = [0u16; 128];
    let count = SendMessageW(
        item.hwndItem,
        CB_GETLBTEXT,
        Some(WPARAM(item.itemID as usize)),
        Some(LPARAM(text.as_mut_ptr() as isize)),
    )
    .0;
    if count > 0 {
        let mut rect = item.rcItem;
        rect.left += 10;
        DrawTextW(
            item.hDC,
            &mut text[..count as usize],
            &mut rect,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
    }
    SelectObject(item.hDC, old_font);
}

unsafe fn paint_owner_button(item: &DRAWITEMSTRUCT) {
    blit_glass(
        item.hDC,
        item.hwndItem,
        item.rcItem.right,
        item.rcItem.bottom,
    );
    let id = item.CtlID as i32;
    let on = is_on(item.hwndItem);
    let pressed = item.itemState.0 & ODS_SELECTED.0 != 0;
    let r = item.rcItem;
    if matches!(id, ID_SWITCH | ID_AUTOCLOSE | ID_AUTOSTART) {
        if on {
            fill_round(item.hDC, r, ACCENT, r.bottom - r.top);
        } else if !glass_on() {
            fill_round(item.hDC, r, PILL, r.bottom - r.top);
        }
        glass_rim(item.hDC, r, r.bottom - r.top, on, false);
        let pad = 3;
        let d = (r.bottom - r.top - pad * 2).max(8);
        let knob_x = if on { r.right - pad - d } else { r.left + pad };
        let knob = CreateSolidBrush(COLORREF(0x00fff8f2));
        let old_brush = SelectObject(item.hDC, knob.into());
        let old_pen = SelectObject(item.hDC, GetStockObject(NULL_PEN));
        let _ = Ellipse(
            item.hDC,
            knob_x,
            r.top + pad,
            knob_x + d,
            r.top + pad + d,
        );
        let bead = CreateSolidBrush(COLORREF(0x00ffffff));
        SelectObject(item.hDC, bead.into());
        let bead_d = (d / 3).max(4);
        let _ = Ellipse(
            item.hDC,
            knob_x + d / 5,
            r.top + pad + d / 7,
            knob_x + d / 5 + bead_d,
            r.top + pad + d / 7 + bead_d,
        );
        SelectObject(item.hDC, old_brush);
        SelectObject(item.hDC, old_pen);
        let _ = DeleteObject(knob.into());
        let _ = DeleteObject(bead.into());
        return;
    }
    let selected = id == ID_SAVE || (is_segment(id) && on);
    let radius = r.bottom - r.top;
    if selected {
        fill_round(
            item.hDC,
            r,
            if pressed {
                COLORREF(0x00548A4C)
            } else {
                ACCENT
            },
            radius,
        );
    } else if pressed || !glass_on() {
        fill_round(item.hDC, r, PILL, radius);
    }
    glass_rim(item.hDC, r, radius, selected, true);
    SetBkMode(item.hDC, TRANSPARENT);
    SetTextColor(item.hDC, if selected { ACCENT_INK } else { FOREGROUND });
    let old_font = SelectObject(item.hDC, HGDIOBJ(UI_FONT.load(Ordering::SeqCst) as *mut _));
    let mut text = [0u16; 128];
    let count = GetWindowTextW(item.hwndItem, &mut text);
    let mut rect = r;
    if count > 0 {
        DrawTextW(
            item.hDC,
            &mut text[..count as usize],
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    }
    SelectObject(item.hDC, old_font);
}

unsafe extern "system" fn settings_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            blit_glass(dc, hwnd, rect.right, rect.bottom);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),

        WM_MEASUREITEM => {
            let item = &mut *(l.0 as *mut MEASUREITEMSTRUCT);
            if item.CtlType == ODT_COMBOBOX {
                item.itemHeight = 28;
            }
            LRESULT(1)
        }
        WM_DRAWITEM => {
            let item = &*(l.0 as *const DRAWITEMSTRUCT);
            if item.CtlType == ODT_COMBOBOX {
                paint_combo_item(item);
            } else {
                paint_owner_button(item);
            }
            LRESULT(1)
        }

        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLORLISTBOX | WM_CTLCOLOREDIT => {
            let dc = HDC(w.0 as *mut _);
            let child = HWND(l.0 as *mut _);
            let mut class_name = [0u16; 32];
            let class_len = GetClassNameW(child, &mut class_name).max(0) as usize;
            let class = String::from_utf16_lossy(&class_name[..class_len]);
            if class.eq_ignore_ascii_case("Edit")
                || class.eq_ignore_ascii_case("ComboBox")
                || class.eq_ignore_ascii_case("ComboLBox")
            {
                SetTextColor(dc, FOREGROUND);
                SetBkColor(dc, FIELD);
                return LRESULT(FIELD_BRUSH.load(Ordering::SeqCst));
            }
            let mut label = [0u16; 128];
            let count = GetWindowTextW(child, &mut label);
            let label = String::from_utf16_lossy(&label[..count as usize]);
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(
                dc,
                if label.starts_with("使用系统") {
                    COLORREF(0x00c6b096)
                } else {
                    FOREGROUND
                },
            );
            LRESULT(GetStockObject(NULL_BRUSH).0 as isize)
        }

        WM_COMMAND => {
            let id = (w.0 & 0xFFFF) as i32;
            let code = (w.0 >> 16) as u16;
            if code == 0 {
                if matches!(id, ID_SWITCH | ID_AUTOCLOSE | ID_AUTOSTART) {
                    let button = GetDlgItem(Some(hwnd), id).unwrap_or_default();
                    set_on(button, !is_on(button));
                } else if (ID_ICON_SEG..ID_ICON_SEG + 3).contains(&id) {
                    select_segment(hwnd, ID_ICON_SEG, 3, id);
                } else if (ID_MODE_SEG..ID_MODE_SEG + 3).contains(&id) {
                    select_segment(hwnd, ID_MODE_SEG, 3, id);
                } else if (ID_COUNT_SEG..ID_COUNT_SEG + 7).contains(&id) {
                    select_segment(hwnd, ID_COUNT_SEG, 7, id);
                    sync_category_rows(hwnd);
                }
            }
            if (ID_CATEGORY_FILE_BASE..ID_CATEGORY_FILE_BASE + CATEGORY_SLOTS).contains(&id) {
                choose_category_icon(hwnd, (id - ID_CATEGORY_FILE_BASE) as usize);
            }
            match id {
                ID_SAVE => {
                    save(hwnd);
                    let _ = DestroyWindow(hwnd);
                }
                ID_CANCEL | ID_CLOSE => {
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
        WM_EXITSIZEMOVE => {
            refresh_glass(hwnd);
            LRESULT(0)
        }
        WM_NCHITTEST => {
            let x = (l.0 & 0xFFFF) as i16 as i32;
            let y = ((l.0 >> 16) & 0xFFFF) as i16 as i32;
            let mut pt = POINT { x, y };
            let _ = ScreenToClient(hwnd, &mut pt);
            if pt.y >= 0 && pt.y < 52 && pt.x < CLIENT_W - 56 {
                LRESULT(HTCAPTION as isize)
            } else {
                DefWindowProcW(hwnd, msg, w, l)
            }
        }
        WM_DESTROY => {
            SETTINGS_HWND.store(0, Ordering::SeqCst);
            let previous = GLASS_BMP.swap(0, Ordering::SeqCst);
            if previous != 0 {
                let _ = DeleteObject(HGDIOBJ(previous as *mut _));
            }
            GLASS.store(false, Ordering::SeqCst);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}
