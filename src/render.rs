//! Orbit Glass: premultiplied BGRA -> Direct2D/DirectWrite -> UpdateLayeredWindow.
//! No desktop capture, color keys, GDI text, or window regions in this pipeline.
use crate::{
    config::{CategoryDisplayMode, Config, IconStyle},
    hit::Hit,
    icons::draw_icon,
};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, SyncSender};
use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED, STGM_READ,
};
use windows::Win32::UI::Shell::{
    ExtractIconExW, IShellLinkW, SHGetFileInfoW, ShellLink, SHFILEINFOW, SHGFI_ICON,
    SHGFI_LARGEICON,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DrawIconEx, UpdateLayeredWindow, DI_NORMAL, ULW_ALPHA,
};
use windows_numerics::{Matrix3x2, Vector2};

pub struct RingGeom {
    pub cx: f32,
    pub cy: f32,
    pub r_mid: f32,
    pub stroke: f32,
    pub n: usize,
}
impl RingGeom {
    pub fn center_angle(&self, qi: usize) -> f32 {
        -45.0 + qi as f32 * 360.0 / self.n.max(1) as f32
    }
    pub fn r_disc(&self) -> f32 {
        self.r_mid + self.stroke / 2.0
    }
    pub fn r_chip(&self) -> f32 {
        (self.r_mid - self.stroke / 2.0 - 24.0).max(36.0)
    }
}
pub const PANEL_HEADER: f32 = 58.0;
pub const PANEL_FOOTER: f32 = 32.0;
pub const ARC_GAP_DEG: f32 = 4.0;

fn resolve_system_icon_path(source: &str) -> String {
    let mut expanded = source.to_owned();
    for (key, value) in std::env::vars() {
        expanded = expanded.replace(&format!("%{key}%"), &value);
    }
    if expanded.eq_ignore_ascii_case("ms-screenclip:") {
        if let Some(root) = std::env::var_os("SystemRoot") {
            let tool = std::path::PathBuf::from(&root)
                .join("System32")
                .join("SnippingTool.exe");
            if tool.is_file() {
                return tool.to_string_lossy().into_owned();
            }
            // Windows 11 can host screen clipping in Client.Core instead of System32.
            let host = std::path::PathBuf::from(&root)
                .join("SystemApps")
                .join("MicrosoftWindows.Client.Core_cw5n1h2txyewy")
                .join("ScreenClippingHost.exe");
            if host.is_file() {
                return host.to_string_lossy().into_owned();
            }
        }
    }
    let candidate = std::path::Path::new(&expanded);
    if !candidate.is_file() && candidate.components().count() == 1 {
        let mut dirs = Vec::new();
        if let Some(root) = std::env::var_os("SystemRoot") {
            let root = std::path::PathBuf::from(root);
            dirs.push(root.join("System32"));
            dirs.push(root);
        }
        if let Some(path) = std::env::var_os("PATH") {
            dirs.extend(std::env::split_paths(&path));
        }
        for dir in dirs {
            let file = dir.join(&expanded);
            if file.is_file() {
                return file.to_string_lossy().into_owned();
            }
        }
    }
    expanded
}

/// Shortcuts return a Shell overlay arrow with their displayed icon in some
/// Windows configurations. Load the shortcut's own icon resource or target
/// directly so the panel displays only the icon itself.
fn resolve_shortcut_icon_source(path: &str) -> Option<String> {
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let persist: IPersistFile = link.cast().ok()?;
        let path_w = crate::sys::wide(path);
        persist.Load(PCWSTR(path_w.as_ptr()), STGM_READ).ok()?;

        let mut icon_path = [0u16; 260];
        let mut icon_index = 0;
        if link
            .GetIconLocation(&mut icon_path, &mut icon_index)
            .is_ok()
        {
            let end = icon_path
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(icon_path.len());
            if end > 0 {
                return Some(String::from_utf16_lossy(&icon_path[..end]));
            }
        }

        let mut target_path = [0u16; 260];
        let mut find_data = windows::Win32::Storage::FileSystem::WIN32_FIND_DATAW::default();
        link.GetPath(&mut target_path, &mut find_data, 0).ok()?;
        let end = target_path
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(target_path.len());
        (end > 0).then(|| String::from_utf16_lossy(&target_path[..end]))
    }
}

pub struct PanelLayout {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub content_h: f32,
    pub cols: usize,
    pub scroll_max: f32,
}
impl PanelLayout {
    pub fn item_cell(&self, cfg: &Config, i: usize, scroll: f32) -> (f32, f32, f32, f32) {
        let cw = cfg.icon_size + 40.0;
        let ch = cfg.icon_size + 46.0;
        (
            self.x + cfg.panel_padding + (i % self.cols) as f32 * (cw + cfg.item_gap),
            self.y + PANEL_HEADER + (i / self.cols) as f32 * (ch + cfg.row_gap) - scroll,
            cw,
            ch,
        )
    }
    pub fn viewport(&self) -> (f32, f32, f32, f32) {
        (
            self.x + 8.0,
            self.y + PANEL_HEADER,
            self.w - 16.0,
            (self.h - PANEL_HEADER - PANEL_FOOTER).max(1.0),
        )
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        rounded_contains(x, y, self.x, self.y, self.w, self.h, 20.0)
    }
}
pub fn rounded_contains(x: f32, y: f32, l: f32, t: f32, w: f32, h: f32, r: f32) -> bool {
    if x < l || y < t || x > l + w || y > t + h {
        return false;
    }
    let r = r.min(w / 2.0).min(h / 2.0);
    let cx = x.clamp(l + r, l + w - r);
    let cy = y.clamp(t + r, t + h - r);
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}
pub fn layout_panel(
    cfg: &Config,
    count: usize,
    g: &RingGeom,
    sw: f32,
    sh: f32,
    _qi: usize,
) -> PanelLayout {
    let cw = cfg.icon_size + 40.0;
    let ch = cfg.icon_size + 46.0;
    let fit = ((sw - 32.0 - 2.0 * cfg.panel_padding + cfg.item_gap) / (cw + cfg.item_gap))
        .floor()
        .max(1.0) as usize;
    let cols = count.clamp(1, cfg.max_columns.max(1)).min(fit);
    let rows = count.div_ceil(cols).max(1);
    let w = (2.0 * cfg.panel_padding + cols as f32 * cw + (cols - 1) as f32 * cfg.item_gap)
        .max(328.0)
        .min((sw - 24.0).max(64.0));
    let content_h = rows as f32 * ch + (rows - 1) as f32 * cfg.row_gap + 8.0;
    let below = sh - (g.cy + g.r_disc() + 20.0) - 16.0;
    let above = g.cy - g.r_disc() - 20.0 - 16.0;
    // 靠近底部时把面板放到圆环上方，避免面板覆盖整个时钟。
    let on_top = below < 156.0 && above > below;
    let available = (if on_top { above } else { below }).max(130.0);
    let h = (PANEL_HEADER + content_h + PANEL_FOOTER)
        .min((sh * cfg.max_height_ratio).max(156.0))
        .min(available)
        .min((sh - 24.0).max(64.0));
    let x = (g.cx - w / 2.0).clamp(12.0, (sw - w - 12.0).max(12.0));
    let preferred_y = if on_top {
        g.cy - g.r_disc() - 20.0 - h
    } else {
        g.cy + g.r_disc() + 20.0
    };
    let y = preferred_y.clamp(12.0, (sh - h - 12.0).max(12.0));
    PanelLayout {
        x,
        y,
        w,
        h,
        content_h,
        cols,
        scroll_max: (content_h - (h - PANEL_HEADER - PANEL_FOOTER)).max(0.0),
    }
}
pub struct SceneState<'a> {
    pub cfg: &'a Config,
    pub geom: &'a RingGeom,
    pub screen_w: f32,
    pub screen_h: f32,
    pub expanded: Option<usize>,
    pub scroll: f32,
    pub edit_mode: bool,
    pub hover: Hit,
    pub panel_alpha: f32,
    pub drop_hot: bool,
    pub status: Option<&'a str>,
    pub resources: crate::sys::ResourceUsage,
    pub second_phase: f32,
    pub local_time_seconds: f32,
    pub environment: Option<&'a crate::environment::EnvironmentSnapshot>,
    pub animation_time: f32,
}
/// Work-area coordinates; includes antialiasing and shadow margins.
pub fn scene_bounds(st: &SceneState) -> RECT {
    let radius = st.geom.r_disc() + 12.0;
    let mut bounds = RECT {
        left: (st.geom.cx - radius).floor() as i32,
        top: (st.geom.cy - radius).floor() as i32,
        right: (st.geom.cx + radius).ceil() as i32,
        bottom: (st.geom.cy + radius).ceil() as i32,
    };
    if let Some(q) = st.expanded.and_then(|qi| st.cfg.quadrants.get(qi)) {
        let panel = layout_panel(st.cfg, q.items.len(), st.geom, st.screen_w, st.screen_h, 0);
        bounds.left = bounds.left.min((panel.x - 12.0).floor() as i32);
        bounds.top = bounds.top.min((panel.y - 12.0).floor() as i32);
        bounds.right = bounds.right.max((panel.x + panel.w + 12.0).ceil() as i32);
        bounds.bottom = bounds.bottom.max((panel.y + panel.h + 12.0).ceil() as i32);
    }
    bounds.left = bounds.left.max(0);
    bounds.top = bounds.top.max(0);
    bounds.right = bounds.right.min(st.screen_w.ceil() as i32);
    bounds.bottom = bounds.bottom.min(st.screen_h.ceil() as i32);
    bounds
}
struct BackBuffer {
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    w: i32,
    h: i32,
    bits: *mut std::ffi::c_void,
}
impl Drop for BackBuffer {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(HGDIOBJ(self.bmp.0));
            let _ = DeleteDC(self.dc);
        }
    }
}
type IconKey = (String, IconStyle, usize);
type IconResult = (IconKey, Option<Vec<u8>>);
pub struct Renderer {
    pub offset: (i32, i32),
    d2d: ID2D1Factory,
    dw: IDWriteFactory,
    rt: Option<ID2D1DCRenderTarget>,
    brush: Option<ID2D1SolidColorBrush>,
    stroke: Option<ID2D1StrokeStyle>,
    back: Option<BackBuffer>,
    clock: Option<IDWriteTextFormat>,
    clock_compact: Option<IDWriteTextFormat>,
    small: Option<IDWriteTextFormat>,
    name: Option<IDWriteTextFormat>,
    heading: Option<IDWriteTextFormat>,
    shell_icons: HashMap<(String, IconStyle, usize), Option<ID2D1Bitmap>>,
    pending_shell_icons: HashSet<(String, IconStyle, usize)>,
    shell_icon_tx: SyncSender<IconResult>,
    shell_icon_rx: Receiver<IconResult>,
}
fn p(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}
fn rc(x: f32, y: f32, w: f32, h: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}
fn c(rgb: (f32, f32, f32), a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: rgb.0,
        g: rgb.1,
        b: rgb.2,
        a,
    }
}
const WHITE: (f32, f32, f32) = (0.94, 0.97, 1.0);
const INK: (f32, f32, f32) = (0.055, 0.075, 0.11);
const MUTED: (f32, f32, f32) = (0.65, 0.73, 0.82);
pub fn accent(qi: usize) -> (f32, f32, f32) {
    [
        (0.43, 0.82, 1.0),
        (0.72, 0.64, 1.0),
        (0.45, 0.9, 0.76),
        (1.0, 0.76, 0.49),
    ][qi % 4]
}
impl Renderer {
    pub fn new() -> Result<Self, String> {
        let d2d =
            unsafe { D2D1CreateFactory::<ID2D1Factory>(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }
                .map_err(|e| e.to_string())?;
        let dw = unsafe { DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED) }
            .map_err(|e| e.to_string())?;
        let (shell_icon_tx, shell_icon_rx) = mpsc::sync_channel(32);
        Ok(Self {
            offset: (0, 0),
            d2d,
            dw,
            rt: None,
            brush: None,
            stroke: None,
            back: None,
            clock: None,
            clock_compact: None,
            small: None,
            name: None,
            heading: None,
            shell_icons: HashMap::new(),
            pending_shell_icons: HashSet::new(),
            shell_icon_tx,
            shell_icon_rx,
        })
    }
    fn format(
        &self,
        size: f32,
        weight: DWRITE_FONT_WEIGHT,
        align: DWRITE_TEXT_ALIGNMENT,
    ) -> Result<IDWriteTextFormat, String> {
        let font = crate::sys::wide("Segoe UI");
        let locale = crate::sys::wide("zh-CN");
        let f = unsafe {
            self.dw.CreateTextFormat(
                PCWSTR(font.as_ptr()),
                None,
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                PCWSTR(locale.as_ptr()),
            )
        }
        .map_err(|e| e.to_string())?;
        unsafe {
            let _ = f.SetTextAlignment(align);
            let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
            let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
        }
        Ok(f)
    }
    fn ensure(&mut self, w: i32, h: i32) -> Result<(), String> {
        if self.back.as_ref().is_some_and(|b| b.w != w || b.h != h) {
            // Release software target storage on shrink instead of retaining its high-water size.
            self.rt = None;
            self.brush = None;
            self.back = None;
        }
        if self.rt.is_none() {
            self.shell_icons.clear();
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let rt = unsafe { self.d2d.CreateDCRenderTarget(&props) }.map_err(|e| e.to_string())?;
            unsafe {
                rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            }
            self.brush = Some(
                unsafe { rt.CreateSolidColorBrush(&c(WHITE, 1.0), None) }
                    .map_err(|e| e.to_string())?,
            );
            self.stroke = Some(
                unsafe {
                    self.d2d.CreateStrokeStyle(
                        &D2D1_STROKE_STYLE_PROPERTIES {
                            startCap: D2D1_CAP_STYLE_ROUND,
                            endCap: D2D1_CAP_STYLE_ROUND,
                            dashCap: D2D1_CAP_STYLE_ROUND,
                            lineJoin: D2D1_LINE_JOIN_ROUND,
                            miterLimit: 10.0,
                            dashStyle: D2D1_DASH_STYLE_SOLID,
                            dashOffset: 0.0,
                        },
                        None,
                    )
                }
                .map_err(|e| e.to_string())?,
            );
            if self.clock.is_none() {
                self.clock = Some(self.format(
                    36.0,
                    DWRITE_FONT_WEIGHT_SEMI_BOLD,
                    DWRITE_TEXT_ALIGNMENT_CENTER,
                )?);
                self.clock_compact = Some(self.format(
                    28.0,
                    DWRITE_FONT_WEIGHT_SEMI_BOLD,
                    DWRITE_TEXT_ALIGNMENT_CENTER,
                )?);
                self.small = Some(self.format(
                    11.0,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_TEXT_ALIGNMENT_CENTER,
                )?);
                self.name = Some(self.format(
                    12.0,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_TEXT_ALIGNMENT_CENTER,
                )?);
                self.heading = Some(self.format(
                    15.0,
                    DWRITE_FONT_WEIGHT_SEMI_BOLD,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                )?);
            }
            self.rt = Some(rt);
        }
        if self.back.as_ref().is_none_or(|b| b.w != w || b.h != h) {
            let dc = unsafe { CreateCompatibleDC(None) };
            if dc.is_invalid() {
                return Err("CreateCompatibleDC failed".into());
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bmp = match unsafe {
                CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            } {
                Ok(b) => b,
                Err(e) => {
                    unsafe {
                        let _ = DeleteDC(dc);
                    }
                    return Err(e.to_string());
                }
            };
            let old = unsafe { SelectObject(dc, HGDIOBJ(bmp.0)) };
            self.back = Some(BackBuffer {
                dc,
                bmp,
                old,
                w,
                h,
                bits,
            });
        }
        Ok(())
    }
    fn b(&self, rgb: (f32, f32, f32), alpha: f32) -> &ID2D1SolidColorBrush {
        let b = self.brush.as_ref().unwrap();
        unsafe {
            b.SetColor(&c(rgb, alpha.clamp(0.0, 1.0)));
        }
        b
    }
    fn ellipse(
        &self,
        rt: &ID2D1DCRenderTarget,
        x: f32,
        y: f32,
        r: f32,
        rgb: (f32, f32, f32),
        a: f32,
    ) {
        unsafe {
            rt.FillEllipse(
                &D2D1_ELLIPSE {
                    point: p(x, y),
                    radiusX: r,
                    radiusY: r,
                },
                self.b(rgb, a),
            );
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn ellipse_xy(
        &self,
        rt: &ID2D1DCRenderTarget,
        x: f32,
        y: f32,
        rx: f32,
        ry: f32,
        rgb: (f32, f32, f32),
        alpha: f32,
    ) {
        unsafe {
            rt.FillEllipse(
                &D2D1_ELLIPSE {
                    point: p(x, y),
                    radiusX: rx,
                    radiusY: ry,
                },
                self.b(rgb, alpha),
            );
        }
    }
    fn round(
        &self,
        rt: &ID2D1DCRenderTarget,
        r: D2D_RECT_F,
        radius: f32,
        rgb: (f32, f32, f32),
        a: f32,
    ) {
        unsafe {
            rt.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: r,
                    radiusX: radius,
                    radiusY: radius,
                },
                self.b(rgb, a),
            );
        }
    }
    fn outline(
        &self,
        rt: &ID2D1DCRenderTarget,
        r: D2D_RECT_F,
        radius: f32,
        rgb: (f32, f32, f32),
        a: f32,
    ) {
        unsafe {
            rt.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: r,
                    radiusX: radius,
                    radiusY: radius,
                },
                self.b(rgb, a),
                1.0,
                None,
            );
        }
    }
    fn text(
        &self,
        rt: &ID2D1DCRenderTarget,
        s: &str,
        r: D2D_RECT_F,
        f: &IDWriteTextFormat,
        rgb: (f32, f32, f32),
        a: f32,
    ) {
        let w = crate::sys::wide(s);
        unsafe {
            rt.DrawText(
                &w[..w.len() - 1],
                f,
                &r,
                self.b(rgb, a),
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }
    fn curved_category_text(
        &self,
        rt: &ID2D1DCRenderTarget,
        label: &str,
        g: &RingGeom,
        center_deg: f32,
        rgb: (f32, f32, f32),
    ) {
        let chars: Vec<char> = label.chars().collect();
        if chars.is_empty() {
            return;
        }
        let widths: Vec<f32> = chars
            .iter()
            .map(|ch| if ch.is_ascii() { 9.0 } else { 14.0 })
            .collect();
        let total: f32 = widths.iter().sum();
        let radius = g.r_mid;
        let direction = if center_deg.rem_euclid(360.0) < 180.0 {
            -1.0
        } else {
            1.0
        };
        let mut cursor = -total / 2.0;
        let mut old_transform = Matrix3x2::identity();
        unsafe { rt.GetTransform(&mut old_transform) };
        for (ch, advance) in chars.iter().zip(widths) {
            let offset = cursor + advance / 2.0;
            let angle = center_deg.to_radians() + direction * offset / radius;
            let x = g.cx + radius * angle.cos();
            let y = g.cy + radius * angle.sin();
            let tangent = center_deg
                + direction * 90.0
                + direction * offset / radius * 180.0 / std::f32::consts::PI;
            let center = Vector2 { X: x, Y: y };
            let transform = Matrix3x2::rotation_around(tangent, center) * old_transform;
            let glyph = ch.to_string();
            let wide = crate::sys::wide(&glyph);
            let rect = rc(x - 8.0, y - 9.0, 16.0, 18.0);
            unsafe {
                rt.SetTransform(&transform);
                rt.DrawText(
                    &wide[..wide.len() - 1],
                    self.small.as_ref().unwrap(),
                    &rect,
                    self.b(rgb, 0.96),
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                rt.SetTransform(&old_transform);
            }
            cursor += advance;
        }
    }
    #[allow(clippy::too_many_arguments)] // Shared layout/geometry inputs, kept explicit.
    fn arc(
        &self,
        rt: &ID2D1DCRenderTarget,
        g: &RingGeom,
        r: f32,
        a0: f32,
        a1: f32,
        width: f32,
        rgb: (f32, f32, f32),
        alpha: f32,
    ) {
        unsafe {
            if let Ok(geo) = self.d2d.CreatePathGeometry() {
                if let Ok(s) = geo.Open() {
                    s.BeginFigure(
                        p(g.cx + r * a0.cos(), g.cy + r * a0.sin()),
                        D2D1_FIGURE_BEGIN_HOLLOW,
                    );
                    s.AddArc(&D2D1_ARC_SEGMENT {
                        point: p(g.cx + r * a1.cos(), g.cy + r * a1.sin()),
                        size: D2D_SIZE_F {
                            width: r,
                            height: r,
                        },
                        rotationAngle: 0.0,
                        sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                        arcSize: if a1 - a0 > std::f32::consts::PI {
                            D2D1_ARC_SIZE_LARGE
                        } else {
                            D2D1_ARC_SIZE_SMALL
                        },
                    });
                    s.EndFigure(D2D1_FIGURE_END_OPEN);
                    let _ = s.Close();
                }
                rt.DrawGeometry(
                    &geo,
                    self.b(rgb, alpha),
                    width,
                    if width > 5.0 {
                        None
                    } else {
                        self.stroke.as_ref()
                    },
                );
            }
        }
    }

    fn day_sector(
        &self,
        rt: &ID2D1DCRenderTarget,
        g: &RingGeom,
        radius: f32,
        start: f32,
        end: f32,
    ) {
        let span = (end - start).rem_euclid(std::f32::consts::TAU);
        if span <= 0.001 || span >= std::f32::consts::TAU - 0.001 {
            return;
        }
        unsafe {
            if let Ok(geometry) = self.d2d.CreatePathGeometry() {
                if let Ok(sink) = geometry.Open() {
                    sink.BeginFigure(p(g.cx, g.cy), D2D1_FIGURE_BEGIN_FILLED);
                    let steps = ((span * radius / 3.0).ceil() as usize).clamp(24, 240);
                    for step in 0..=steps {
                        let angle = start + span * step as f32 / steps as f32;
                        sink.AddLine(p(g.cx + radius * angle.cos(), g.cy + radius * angle.sin()));
                    }
                    sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                    if sink.Close().is_ok() {
                        rt.FillGeometry(&geometry, self.b((0.25, 0.53, 0.76), 0.72), None);
                    }
                }
            }
        }
    }
    pub fn draw(&mut self, st: &SceneState, hwnd: HWND) -> Result<(), String> {
        let bounds = scene_bounds(st);
        self.offset = (bounds.left, bounds.top);
        self.ensure(bounds.right - bounds.left, bounds.bottom - bounds.top)?;
        // Drop resources for removed items, other categories and previous styles.
        let active: HashSet<_> = st
            .expanded
            .and_then(|qi| st.cfg.quadrants.get(qi).map(|q| (qi, q)))
            .into_iter()
            .flat_map(|(qi, q)| {
                q.items
                    .iter()
                    .map(move |item| (item.target.clone(), st.cfg.icon_style, qi))
            })
            .collect();
        self.shell_icons.retain(|key, _| active.contains(key));
        let rt = self.rt.as_ref().unwrap().clone();
        self.collect_shell_icons(&rt, &active);
        {
            let back = self.back.as_ref().unwrap();
            unsafe {
                rt.BindDC(
                    back.dc,
                    &RECT {
                        left: 0,
                        top: 0,
                        right: back.w,
                        bottom: back.h,
                    },
                )
                .map_err(|e| e.to_string())?;
                rt.BeginDraw();
                rt.SetTransform(&Matrix3x2 {
                    M11: 1.0,
                    M12: 0.0,
                    M21: 0.0,
                    M22: 1.0,
                    M31: -(bounds.left as f32),
                    M32: -(bounds.top as f32),
                });
                rt.Clear(Some(&c((0.0, 0.0, 0.0), 0.0)));
            }
        }
        self.ring(&rt, st);
        if let Some(q) = st.expanded {
            self.panel(&rt, st, q);
        }
        if st.cfg.icon_style != IconStyle::Unified {
            self.draw_shell_icons(st, &rt);
        }
        if let Err(e) = unsafe { rt.EndDraw(None, None) } {
            self.rt = None;
            self.brush = None;
            return Err(format!("D2D EndDraw: {e}"));
        }
        let back = self.back.as_ref().unwrap();
        if let Some(path) = std::env::var_os("RING_DOCK_FRAME") {
            let pixels = unsafe {
                std::slice::from_raw_parts(back.bits as *const u8, (back.w * back.h * 4) as usize)
            };
            // Preserve the diagnostic work-area coordinate contract only when requested.
            let width = st.screen_w.ceil() as i32;
            let height = st.screen_h.ceil() as i32;
            let mut bytes = vec![0; (width * height * 4) as usize + 8];
            bytes[..4].copy_from_slice(&width.to_le_bytes());
            bytes[4..8].copy_from_slice(&height.to_le_bytes());
            for row in 0..back.h as usize {
                let target =
                    8 + ((row + bounds.top as usize) * width as usize + bounds.left as usize) * 4;
                let source = row * back.w as usize * 4;
                bytes[target..target + back.w as usize * 4]
                    .copy_from_slice(&pixels[source..source + back.w as usize * 4]);
            }
            let panel=st.expanded.and_then(|qi|st.cfg.quadrants.get(qi).map(|q|{
                let l=layout_panel(st.cfg,q.items.len(),st.geom,st.screen_w,st.screen_h,qi);
                serde_json::json!({"x":l.x,"y":l.y,"w":l.w,"h":l.h,"scrollMax":l.scroll_max,"viewport":l.viewport(),"itemNames":q.items.iter().map(|item|&item.name).collect::<Vec<_>>(),"cells":(0..q.items.len()).map(|i|l.item_cell(st.cfg,i,st.scroll)).collect::<Vec<_>>()})}));
            let meta = serde_json::json!({"width":width,"height":height,"bufferWidth":back.w,"bufferHeight":back.h,"offset":[bounds.left,bounds.top],"center":[st.geom.cx,st.geom.cy],"radius":st.geom.r_mid,"outer":st.geom.r_disc(),"chip":st.geom.r_chip(),"expanded":st.expanded,"edit":st.edit_mode,"scroll":st.scroll,"panel":panel});
            let _ = std::fs::write(format!("{}.json", path.to_string_lossy()), meta.to_string());
            let _ = std::fs::write(path, bytes);
        }
        deskpin::place_rect(hwnd, bounds.left, bounds.top, back.w, back.h);
        let size = SIZE {
            cx: back.w,
            cy: back.h,
        };
        let src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        unsafe {
            UpdateLayeredWindow(
                hwnd,
                None,
                None,
                Some(&size),
                Some(back.dc),
                Some(&src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        }
        .map_err(|e| format!("UpdateLayeredWindow: {e}"))
    }

    fn draw_shell_icons(&mut self, st: &SceneState, rt: &ID2D1DCRenderTarget) {
        let Some(qi) = st.expanded else { return };
        let Some(q) = st.cfg.quadrants.get(qi) else {
            return;
        };
        let layout = layout_panel(st.cfg, q.items.len(), st.geom, st.screen_w, st.screen_h, qi);
        let (vx, vy, vw, vh) = layout.viewport();
        let visible: Vec<_> = q
            .items
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                let (_, y, _, h) = layout.item_cell(st.cfg, *i, st.scroll);
                y + h >= vy && y <= vy + vh
            })
            .map(|(_, item)| item.clone())
            .collect();
        self.schedule_shell_icons(&visible, st.cfg.icon_style, qi);
        unsafe {
            rt.PushAxisAlignedClip(&rc(vx, vy, vw, vh), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        }
        for (i, item) in q.items.iter().enumerate() {
            let key = (item.target.clone(), st.cfg.icon_style, qi);
            let bitmap = self.shell_icons.get(&key).cloned().flatten();
            let (x, y, w, h) = layout.item_cell(st.cfg, i, st.scroll);
            if y + h < vy || y > vy + vh {
                continue;
            }
            let Some(bitmap) = bitmap else {
                draw_icon(
                    rt,
                    self.b(WHITE, 0.94),
                    &item.kind,
                    x + w / 2.0,
                    y + 29.0,
                    15.0,
                    1.4,
                    self.stroke.as_ref(),
                );
                continue;
            };
            let rect = rc(x + w / 2.0 - 18.0, y + 11.0, 36.0, 36.0);
            unsafe {
                rt.DrawBitmap(
                    &bitmap,
                    Some(&rect),
                    1.0,
                    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                    None,
                );
            }
        }
        unsafe { rt.PopAxisAlignedClip() };
    }

    fn schedule_shell_icons(&mut self, items: &[crate::config::Item], style: IconStyle, qi: usize) {
        // One extraction batch at a time; rapid scrolling must not spawn many COM threads.
        if !self.pending_shell_icons.is_empty() {
            return;
        }
        let mut jobs = Vec::new();
        for item in items {
            let key = (item.target.clone(), style, qi);
            if !self.shell_icons.contains_key(&key) && self.pending_shell_icons.insert(key.clone())
            {
                jobs.push((key, item.target.clone()));
                if jobs.len() == 32 {
                    break;
                }
            }
        }
        if jobs.is_empty() {
            return;
        }

        let tx = self.shell_icon_tx.clone();
        std::thread::spawn(move || {
            let com_initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
            for (key, path) in jobs {
                let pixels = Renderer::load_shell_pixels(&path, style, qi);
                if tx.send((key, pixels)).is_err() {
                    break;
                }
            }
            if com_initialized {
                unsafe { CoUninitialize() };
            }
        });
    }

    fn collect_shell_icons(
        &mut self,
        rt: &ID2D1DCRenderTarget,
        active: &HashSet<(String, IconStyle, usize)>,
    ) {
        let ready: Vec<_> = self.shell_icon_rx.try_iter().collect();
        for (key, pixels) in ready {
            self.pending_shell_icons.remove(&key);
            if active.contains(&key) {
                let bitmap = pixels.and_then(|pixels| Self::create_shell_bitmap(rt, &pixels));
                // Bound cache even for very large collections; visible rows load on demand.
                if self.shell_icons.len() >= 128 {
                    self.shell_icons.clear();
                }
                self.shell_icons.insert(key, bitmap);
            }
        }
    }

    fn load_shell_pixels(path: &str, style: IconStyle, qi: usize) -> Option<Vec<u8>> {
        const SIZE: i32 = 36;
        let source = if path.to_ascii_lowercase().ends_with(".lnk") {
            resolve_shortcut_icon_source(path).unwrap_or_else(|| path.to_owned())
        } else {
            path.to_owned()
        };
        let source = resolve_system_icon_path(&source);
        let target = crate::sys::wide(&source);
        let mut info = SHFILEINFOW::default();
        // Read executable resources directly: avoid third-party Shell extensions for
        // the common .exe/.dll case. Other file types retain their Windows association.
        let direct = std::path::Path::new(&source)
            .extension()
            .is_some_and(|ext| {
                ext.eq_ignore_ascii_case("exe")
                    || ext.eq_ignore_ascii_case("dll")
                    || ext.eq_ignore_ascii_case("ico")
            });
        let extracted = direct
            && unsafe {
                ExtractIconExW(PCWSTR(target.as_ptr()), 0, Some(&mut info.hIcon), None, 1)
            } > 0
            && !info.hIcon.is_invalid();
        let result = if extracted {
            1
        } else {
            unsafe {
                SHGetFileInfoW(
                    PCWSTR(target.as_ptr()),
                    FILE_FLAGS_AND_ATTRIBUTES(0),
                    Some(&mut info),
                    std::mem::size_of::<SHFILEINFOW>() as u32,
                    SHGFI_ICON | SHGFI_LARGEICON,
                )
            }
        };
        if result == 0 || info.hIcon.is_invalid() {
            return None;
        }

        let dc = unsafe { CreateCompatibleDC(None) };
        if dc.is_invalid() {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyIcon(info.hIcon);
            }
            return None;
        }
        let bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: SIZE,
                biHeight: -SIZE,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let dib =
            unsafe { CreateDIBSection(Some(dc), &bitmap_info, DIB_RGB_COLORS, &mut bits, None, 0) };
        let Ok(dib) = dib else {
            unsafe {
                let _ = DeleteDC(dc);
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyIcon(info.hIcon);
            }
            return None;
        };
        let old = unsafe { SelectObject(dc, HGDIOBJ(dib.0)) };
        unsafe {
            std::ptr::write_bytes(bits as *mut u8, 0, (SIZE * SIZE * 4) as usize);
        }
        let _ = unsafe { DrawIconEx(dc, 0, 0, info.hIcon, SIZE, SIZE, 0, None, DI_NORMAL) };

        let pixels =
            unsafe { std::slice::from_raw_parts_mut(bits as *mut u8, (SIZE * SIZE * 4) as usize) };
        if style == IconStyle::Tinted {
            let tint = [
                (0.58_f32, 0.82_f32, 1.0_f32),
                (0.78, 0.68, 1.0),
                (0.54, 0.92, 0.82),
                (1.0, 0.82, 0.62),
            ][qi % 4];
            for pixel in pixels.chunks_exact_mut(4) {
                let alpha = pixel[3] as f32;
                if alpha == 0.0 {
                    continue;
                }
                let b = pixel[0] as f32;
                let g = pixel[1] as f32;
                let r = pixel[2] as f32;
                let luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                pixel[0] = (luminance * 0.72 + tint.2 * alpha * 0.28).min(alpha) as u8;
                pixel[1] = (luminance * 0.72 + tint.1 * alpha * 0.28).min(alpha) as u8;
                pixel[2] = (luminance * 0.72 + tint.0 * alpha * 0.28).min(alpha) as u8;
            }
        }
        let pixels = pixels.to_vec();
        unsafe {
            SelectObject(dc, old);
            let _ = DeleteObject(HGDIOBJ(dib.0));
            let _ = DeleteDC(dc);
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyIcon(info.hIcon);
        }
        pixels
            .chunks_exact(4)
            .any(|pixel| pixel[3] > 0)
            .then_some(pixels)
    }

    fn create_shell_bitmap(rt: &ID2D1DCRenderTarget, pixels: &[u8]) -> Option<ID2D1Bitmap> {
        const SIZE: i32 = 36;
        let properties = D2D1_BITMAP_PROPERTIES {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
        };
        let size = D2D_SIZE_U {
            width: SIZE as u32,
            height: SIZE as u32,
        };
        unsafe {
            rt.CreateBitmap(
                size,
                Some(pixels.as_ptr() as *const _),
                (SIZE * 4) as u32,
                &properties,
            )
            .ok()
        }
    }

    fn ring(&self, rt: &ID2D1DCRenderTarget, st: &SceneState) {
        let g = st.geom;
        let opacity = st.cfg.opacity.clamp(0.3, 1.0);
        self.environment_background(rt, st);
        // Soft shadows remain transparent, with no framebuffer/wallpaper sampling.
        for (r, a) in [(g.r_disc() + 7.0, 0.035), (g.r_disc() + 4.0, 0.055)] {
            for q in 0..g.n {
                let center = g.center_angle(q);
                let half = 180.0 / g.n as f32;
                self.arc(
                    rt,
                    g,
                    r - g.stroke / 2.0,
                    (center - half + ARC_GAP_DEG).to_radians(),
                    (center + half - ARC_GAP_DEG).to_radians(),
                    g.stroke,
                    (0.0, 0.0, 0.0),
                    a,
                );
            }
        }
        for q in 0..g.n {
            let center = g.center_angle(q);
            let half = 180.0 / g.n as f32;
            let a0 = (center - half + ARC_GAP_DEG).to_radians();
            let a1 = (center + half - ARC_GAP_DEG).to_radians();
            let hot = st.hover == Hit::Quadrant(q);
            let active = st.expanded == Some(q);
            let col = accent(q);
            self.arc(
                rt,
                g,
                g.r_mid,
                a0,
                a1,
                g.stroke,
                INK,
                (0.76 + if hot { 0.08 } else { 0.0 }) * opacity,
            );
            if hot || active {
                self.arc(
                    rt,
                    g,
                    g.r_mid,
                    a0,
                    a1,
                    g.stroke - 2.0,
                    col,
                    if active { 0.14 } else { 0.08 },
                );
            }
            self.arc(rt, g, g.r_disc() - 0.6, a0, a1, 0.8, WHITE, 0.16);
            self.arc(
                rt,
                g,
                g.r_mid - g.stroke / 2.0 + 0.6,
                a0,
                a1,
                0.7,
                WHITE,
                0.09,
            );
            self.arc(
                rt,
                g,
                g.r_disc() - 5.0,
                (center - half + 9.0).to_radians(),
                (center + half - 9.0).to_radians(),
                if active || hot { 2.0 } else { 1.2 },
                col,
                if active || hot { 0.95 } else { 0.38 },
            );
            if let Some(quad) = st.cfg.quadrants.get(q) {
                let a = center.to_radians();
                let x = g.cx + g.r_mid * a.cos();
                let y = g.cy + g.r_mid * a.sin();
                let icon_kind = match quad.category_icon.as_str() {
                    "" | "auto" => crate::icons::category_icon(&quad.label).unwrap_or(&quad.kind),
                    "collaboration" | "development" | "ai" | "entertainment" | "program"
                    | "file" | "folder" | "url" => quad.category_icon.as_str(),
                    _ => crate::icons::category_icon(&quad.label).unwrap_or(&quad.kind),
                };
                let color = if hot || active { col } else { WHITE };
                match st.cfg.category_display_mode {
                    CategoryDisplayMode::Icon => draw_icon(
                        rt,
                        self.b(color, 0.96),
                        icon_kind,
                        x,
                        y,
                        18.0,
                        1.6,
                        self.stroke.as_ref(),
                    ),
                    CategoryDisplayMode::Text => self.curved_category_text(
                        rt,
                        &quad.label,
                        g,
                        center,
                        if hot || active { col } else { MUTED },
                    ),
                    CategoryDisplayMode::Both => {
                        draw_icon(
                            rt,
                            self.b(color, 0.94),
                            icon_kind,
                            x,
                            y - 6.0,
                            16.0,
                            1.5,
                            self.stroke.as_ref(),
                        );
                        self.text(
                            rt,
                            &quad.label,
                            rc(x - 35.0, y + 9.0, 70.0, 16.0),
                            self.small.as_ref().unwrap(),
                            if hot || active { col } else { MUTED },
                            1.0,
                        );
                    }
                }
            }
        }
        let r = g.r_chip();
        self.ellipse(rt, g.cx, g.cy + 4.0, r + 3.0, (0.0, 0.0, 0.0), 0.075);
        self.ellipse(rt, g.cx, g.cy, r, INK, 0.68 * opacity);
        unsafe {
            rt.DrawEllipse(
                &D2D1_ELLIPSE {
                    point: p(g.cx, g.cy),
                    radiusX: r - 0.5,
                    radiusY: r - 0.5,
                },
                self.b(WHITE, 0.15),
                0.8,
                None,
            );
        }
        let cpu_color = (0.28, 0.86, 0.91);
        let memory_color = (0.72, 0.58, 1.0);
        self.progress_ring(rt, g, r - 6.0, st.resources.cpu, cpu_color);
        self.progress_ring(rt, g, r - 12.0, st.resources.memory, memory_color);
        // Seconds hand dot remains on the inner side of the memory ring.
        let second_angle = st.second_phase.rem_euclid(60.0) * std::f32::consts::TAU / 60.0
            - std::f32::consts::FRAC_PI_2;
        let second_radius = r - 18.0;
        let second_x = g.cx + second_radius * second_angle.cos();
        let second_y = g.cy + second_radius * second_angle.sin();
        self.ellipse(rt, second_x, second_y, 4.0, (0.01, 0.025, 0.06), 0.90);
        self.ellipse(rt, second_x, second_y, 2.8, WHITE, 0.98);
        self.ellipse(rt, second_x, second_y, 1.4, cpu_color, 1.0);
        self.text(
            rt,
            &st.cfg.clock_text(),
            rc(g.cx - r + 3.0, g.cy - 27.0, 2.0 * r - 6.0, 48.0),
            if st.cfg.clock_text().len() > 5 {
                self.clock_compact.as_ref().unwrap()
            } else {
                self.clock.as_ref().unwrap()
            },
            WHITE,
            1.0,
        );
    }
    fn environment_background(&self, rt: &ID2D1DCRenderTarget, st: &SceneState) {
        let g = st.geom;
        // The light sector follows the 24-hour dial: midnight at 6 o'clock,
        // noon at 12 o'clock, and time advances clockwise.
        let radius = (g.r_chip() - 2.0).max(1.0);
        let minute = st.local_time_seconds / 60.0;
        let daylight = st
            .environment
            .map_or((360.0..1080.0).contains(&minute), |weather| {
                weather
                    .sunrise_minute
                    .zip(weather.sunset_minute)
                    .map(|(sunrise, sunset)| minute >= sunrise as f32 && minute < sunset as f32)
                    .unwrap_or(weather.is_day)
            });
        self.ellipse(rt, g.cx, g.cy, radius, (0.055, 0.10, 0.23), 0.74);
        let (sunrise, sunset) = st
            .environment
            .and_then(|weather| weather.sunrise_minute.zip(weather.sunset_minute))
            .unwrap_or((360, 1080));
        let angle_for_minute =
            |minute: f32| minute / 1440.0 * std::f32::consts::TAU + std::f32::consts::FRAC_PI_2;
        self.day_sector(
            rt,
            g,
            radius,
            angle_for_minute(sunrise as f32),
            angle_for_minute(sunset as f32),
        );

        let phase = st.animation_time;
        if let Some(weather) = st.environment {
            self.weather_illustration(
                rt,
                g,
                radius,
                weather.weather_code,
                weather.cloud_cover,
                weather.wind_speed_kmh,
                weather.wind_direction,
                phase,
                daylight,
            );
        } else {
            self.weather_illustration(rt, g, radius, 0, 0.0, 0.0, 0.0, phase, daylight);
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn weather_illustration(
        &self,
        rt: &ID2D1DCRenderTarget,
        g: &RingGeom,
        radius: f32,
        code: i32,
        cloud_cover: f32,
        wind_speed_kmh: f32,
        wind_direction: f32,
        phase: f32,
        daylight: bool,
    ) {
        let partly_cloudy = code == 2 || (cloud_cover > 60.0 && code <= 1);
        let overcast = matches!(code, 3 | 45 | 48);
        let celestial = matches!(code, 0..=2);
        let rain = matches!(code, 51..=67 | 80..=82 | 95..=99);
        let snow = matches!(code, 71..=77 | 85 | 86);
        let hail = matches!(code, 96 | 99);
        let thunder = matches!(code, 95..=99);
        let fog = matches!(code, 45 | 48);
        let tau = std::f32::consts::TAU;
        let drift = (phase * 0.16).sin() * radius * 0.055;
        let icon_x = g.cx - radius * 0.12;
        let icon_y = g.cy - radius * 0.31;
        let sun = (1.0, 0.84, 0.42);
        let moon = (0.79, 0.87, 1.0);
        let night_sky = (0.055, 0.10, 0.23);

        // Clear weather gets a sun by day and a crescent moon at night. Partly cloudy
        // conditions keep the same sky object and let the animated cloud pass in front.
        if celestial && !overcast {
            if daylight {
                let pulse = (phase * 1.7).sin() * radius * 0.006;
                for ray in 0..8 {
                    let angle = ray as f32 * tau / 8.0 + phase * 0.12;
                    let inner = radius * 0.105;
                    let outer = radius * 0.155 + pulse;
                    unsafe {
                        rt.DrawLine(
                            p(icon_x + inner * angle.cos(), icon_y + inner * angle.sin()),
                            p(icon_x + outer * angle.cos(), icon_y + outer * angle.sin()),
                            self.b(sun, 0.66),
                            1.5,
                            self.stroke.as_ref(),
                        );
                    }
                }
                self.ellipse(rt, icon_x, icon_y, radius * 0.105 + pulse * 0.2, sun, 0.95);
            } else {
                self.ellipse(rt, icon_x, icon_y, radius * 0.115, moon, 0.95);
                self.ellipse(
                    rt,
                    icon_x + radius * 0.055,
                    icon_y - radius * 0.035,
                    radius * 0.108,
                    night_sky,
                    0.97,
                );
            }
        }

        let cloud_present =
            partly_cloudy || overcast || rain || snow || hail || fog || (code != 0 && code != 1);
        let cloud_x = g.cx + radius * 0.13 + drift;
        let cloud_y = g.cy - radius * 0.27;
        let cloud_color = if daylight {
            (0.83, 0.91, 0.98)
        } else {
            (0.57, 0.68, 0.84)
        };
        if cloud_present {
            let cloud_alpha = if partly_cloudy {
                0.73
            } else if overcast {
                0.86
            } else {
                0.82
            };
            self.ellipse_xy(
                rt,
                cloud_x - radius * 0.15,
                cloud_y + radius * 0.025,
                radius * 0.145,
                radius * 0.09,
                cloud_color,
                cloud_alpha,
            );
            self.ellipse_xy(
                rt,
                cloud_x,
                cloud_y - radius * 0.035,
                radius * 0.17,
                radius * 0.13,
                cloud_color,
                cloud_alpha,
            );
            self.ellipse_xy(
                rt,
                cloud_x + radius * 0.15,
                cloud_y + radius * 0.02,
                radius * 0.13,
                radius * 0.085,
                cloud_color,
                cloud_alpha,
            );
            self.ellipse_xy(
                rt,
                cloud_x,
                cloud_y + radius * 0.075,
                radius * 0.26,
                radius * 0.075,
                cloud_color,
                cloud_alpha,
            );
        }

        // Wind direction is where the wind comes from; particles travel the opposite way.
        if wind_speed_kmh >= 20.0 {
            let flow = (wind_direction + 180.0).to_radians();
            let speed = ((wind_speed_kmh / 55.0).sqrt()).clamp(0.65, 2.2);
            let vx = flow.cos();
            let vy = flow.sin();
            for i in 0..5 {
                let lane = i as f32 - 2.0;
                let along = (phase * (0.42 + speed * 0.24) + i as f32 * 0.23).rem_euclid(1.0);
                let x = cloud_x + (along - 0.5) * radius * 0.58 + lane * radius * 0.025;
                let y = g.cy - radius * (0.06 + i as f32 * 0.045);
                let length = radius * (0.06 + speed * 0.025);
                unsafe {
                    rt.DrawLine(
                        p(x, y),
                        p(x + vx * length, y + vy * length),
                        self.b((0.77, 0.87, 0.96), (0.22 + speed * 0.12).min(0.55)),
                        1.1,
                        self.stroke.as_ref(),
                    );
                }
            }
        }

        if rain || snow || hail {
            for i in 0..7 {
                let x = cloud_x + (i as f32 - 3.0) * radius * 0.075;
                let rate = if snow {
                    0.48
                } else if hail {
                    1.15
                } else {
                    1.35
                };
                let fall = (phase * rate + i as f32 * 0.37).rem_euclid(1.0);
                let y = cloud_y + radius * (0.15 + fall * 0.38);
                if hail {
                    let wobble = (phase * 3.0 + i as f32).sin() * 2.3;
                    self.ellipse(rt, x + wobble, y, 2.2, (0.82, 0.94, 1.0), 0.95);
                    self.ellipse(rt, x + wobble - 0.5, y - 0.5, 0.7, WHITE, 0.8);
                } else if snow {
                    let wobble = (phase * 1.6 + i as f32).sin() * 3.0;
                    self.ellipse(rt, x + wobble, y, 1.8, WHITE, 0.9);
                } else {
                    let slant = if wind_speed_kmh >= 20.0 {
                        -2.0 - wind_speed_kmh.min(90.0) * 0.045
                    } else {
                        -2.0
                    };
                    unsafe {
                        rt.DrawLine(
                            p(x, y),
                            p(x + slant, y + radius * 0.075),
                            self.b((0.62, 0.83, 1.0), 0.92),
                            1.6,
                            self.stroke.as_ref(),
                        );
                    }
                }
            }
        }

        if fog {
            for line in 0..3 {
                let y = cloud_y + radius * (0.14 + line as f32 * 0.07);
                let dx = (phase * 0.22 + line as f32).sin() * radius * 0.05;
                unsafe {
                    rt.DrawLine(
                        p(cloud_x - radius * 0.26 + dx, y),
                        p(cloud_x + radius * 0.26 + dx, y),
                        self.b(WHITE, 0.72),
                        2.0,
                        self.stroke.as_ref(),
                    );
                }
            }
        }

        if thunder {
            // Brief periodic flashes keep lightning visibly animated without a random source.
            let flash = (phase * 2.6).sin() > 0.94;
            if flash {
                self.ellipse_xy(
                    rt,
                    g.cx,
                    g.cy - radius * 0.24,
                    radius * 0.43,
                    radius * 0.31,
                    (0.62, 0.76, 1.0),
                    0.17,
                );
            }
            let bolt_x = cloud_x + radius * 0.07;
            let bolt_y = cloud_y + radius * 0.11;
            let bolt = [
                p(bolt_x + 3.0, bolt_y),
                p(bolt_x - 4.0, bolt_y + radius * 0.14),
                p(bolt_x + 1.0, bolt_y + radius * 0.14),
                p(bolt_x - 5.0, bolt_y + radius * 0.29),
            ];
            let alpha = if flash { 1.0 } else { 0.84 };
            for pair in bolt.windows(2) {
                unsafe {
                    rt.DrawLine(
                        pair[0],
                        pair[1],
                        self.b((0.95, 0.88, 0.50), alpha),
                        2.4,
                        None,
                    );
                }
            }
        }

        if wind_speed_kmh >= 118.0 {
            // Hurricane-force winds (118 km/h and above) get a rotating typhoon spiral.
            let cx = g.cx + radius * 0.27;
            let cy = g.cy - radius * 0.24;
            let turns = 1.45;
            let steps = 36;
            let mut previous = None;
            for step in 0..=steps {
                let t = step as f32 / steps as f32;
                let angle = phase * 1.8 + t * turns * tau;
                let spiral_radius = radius * (0.018 + t * 0.19);
                let point = p(
                    cx + spiral_radius * angle.cos(),
                    cy + spiral_radius * angle.sin(),
                );
                if let Some(from) = previous {
                    unsafe {
                        rt.DrawLine(
                            from,
                            point,
                            self.b((0.70, 0.88, 1.0), 0.94),
                            1.8,
                            self.stroke.as_ref(),
                        );
                    }
                }
                previous = Some(point);
            }
            self.ellipse(rt, cx, cy, radius * 0.025, WHITE, 0.95);
        }

        // Keep an animated cloud for any non-clear WMO condition not explicitly handled above.
        if !celestial && !cloud_present {
            self.ellipse_xy(
                rt,
                g.cx + drift,
                g.cy - radius * 0.23,
                radius * 0.22,
                radius * 0.10,
                WHITE,
                0.52,
            );
        }
    }
    fn progress_ring(
        &self,
        rt: &ID2D1DCRenderTarget,
        g: &RingGeom,
        radius: f32,
        value: f32,
        color: (f32, f32, f32),
    ) {
        let start = -std::f32::consts::FRAC_PI_2;
        let middle = start + std::f32::consts::PI;
        let end = start + std::f32::consts::TAU;
        self.arc(rt, g, radius, start, middle, 2.2, MUTED, 0.16);
        self.arc(rt, g, radius, middle, end, 2.2, MUTED, 0.16);

        let progress_end = start + std::f32::consts::TAU * value.clamp(0.0, 99.5) / 100.0;
        if progress_end > start {
            let first_end = progress_end.min(middle);
            self.arc(rt, g, radius, start, first_end, 2.2, color, 0.95);
            if progress_end > middle {
                self.arc(rt, g, radius, middle, progress_end, 2.2, color, 0.95);
            }
        }
    }
    fn panel(&self, rt: &ID2D1DCRenderTarget, st: &SceneState, qi: usize) {
        let Some(q) = st.cfg.quadrants.get(qi) else {
            return;
        };
        let l = layout_panel(st.cfg, q.items.len(), st.geom, st.screen_w, st.screen_h, qi);
        let a = st.panel_alpha;
        let col = accent(qi);
        self.round(
            rt,
            rc(l.x - 3.0, l.y + 5.0, l.w + 6.0, l.h + 3.0),
            22.0,
            (0.0, 0.0, 0.0),
            0.12 * a,
        );
        self.round(
            rt,
            rc(l.x, l.y, l.w, l.h),
            20.0,
            INK,
            (0.86 + if st.drop_hot { 0.06 } else { 0.0 }) * a,
        );
        self.outline(
            rt,
            rc(l.x + 0.5, l.y + 0.5, l.w - 1.0, l.h - 1.0),
            19.5,
            if st.drop_hot { col } else { WHITE },
            if st.drop_hot { 0.85 * a } else { 0.16 * a },
        );
        self.round(rt, rc(l.x + 20.0, l.y + 18.0, 3.0, 20.0), 1.5, col, a);
        self.text(
            rt,
            &q.label,
            rc(l.x + 33.0, l.y + 13.0, l.w - 140.0, 28.0),
            self.heading.as_ref().unwrap(),
            WHITE,
            a,
        );
        // Dedicated edit / close buttons; names communicate the long-press shortcut.
        let editing = st.hover == Hit::PanelEdit || st.edit_mode;
        self.round(
            rt,
            rc(l.x + l.w - 98.0, l.y + 13.0, 54.0, 29.0),
            9.0,
            if editing { col } else { WHITE },
            if editing { 0.16 * a } else { 0.05 * a },
        );
        self.text(
            rt,
            if st.edit_mode { "完成" } else { "整理" },
            rc(l.x + l.w - 98.0, l.y + 13.0, 54.0, 29.0),
            self.small.as_ref().unwrap(),
            if editing { col } else { MUTED },
            a,
        );
        let close = st.hover == Hit::PanelClose;
        self.round(
            rt,
            rc(l.x + l.w - 39.0, l.y + 13.0, 27.0, 29.0),
            9.0,
            WHITE,
            if close { 0.13 * a } else { 0.04 * a },
        );
        unsafe {
            let x = l.x + l.w - 25.5;
            let y = l.y + 27.5;
            for (s, t) in [(-4.0, 4.0), (4.0, -4.0)] {
                rt.DrawLine(
                    p(x - 4.0, y + s),
                    p(x + 4.0, y + t),
                    self.b(MUTED, a),
                    1.2,
                    self.stroke.as_ref(),
                );
            }
        }
        let (vx, vy, vw, vh) = l.viewport();
        unsafe {
            rt.PushAxisAlignedClip(&rc(vx, vy, vw, vh), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        }
        if q.items.is_empty() {
            self.text(
                rt,
                "拖入文件，开启你的收藏",
                rc(vx, vy, vw, vh),
                self.name.as_ref().unwrap(),
                MUTED,
                a,
            );
        }
        for (i, it) in q.items.iter().enumerate() {
            let (x, y, w, h) = l.item_cell(st.cfg, i, st.scroll);
            if y + h < vy || y > vy + vh {
                continue;
            }
            let hot = st.hover == Hit::Item { qi, index: i };
            let del = st.hover == Hit::ItemDelete { qi, index: i };
            self.round(
                rt,
                rc(x, y, w, h - 4.0),
                12.0,
                if hot { col } else { WHITE },
                if hot { 0.12 * a } else { 0.035 * a },
            );
            if hot || st.edit_mode {
                self.outline(
                    rt,
                    rc(x + 0.5, y + 0.5, w - 1.0, h - 5.0),
                    11.5,
                    col,
                    if hot { 0.28 * a } else { 0.13 * a },
                );
            }
            self.round(
                rt,
                rc(x + w / 2.0 - 23.0, y + 7.0, 46.0, 43.0),
                12.0,
                col,
                0.075 * a,
            );
            if st.cfg.icon_style == IconStyle::Unified {
                draw_icon(
                    rt,
                    self.b(col, 0.94 * a),
                    &it.kind,
                    x + w / 2.0,
                    y + 29.0,
                    15.0,
                    1.4,
                    self.stroke.as_ref(),
                );
            }
            let mut name = it.name.clone();
            if name.chars().count() > 10 {
                name = name.chars().take(9).collect::<String>() + "…";
            }
            self.text(
                rt,
                &name,
                rc(x + 4.0, y + st.cfg.icon_size + 8.0, w - 8.0, 22.0),
                self.name.as_ref().unwrap(),
                WHITE,
                0.94 * a,
            );
            if st.edit_mode {
                let bx = x + w - 6.0;
                let by = y + 7.0;
                self.ellipse(
                    rt,
                    bx,
                    by,
                    9.0,
                    if del {
                        (1.0, 0.35, 0.38)
                    } else {
                        (0.8, 0.28, 0.34)
                    },
                    a,
                );
                unsafe {
                    rt.DrawLine(
                        p(bx - 3.0, by),
                        p(bx + 3.0, by),
                        self.b(WHITE, a),
                        1.3,
                        self.stroke.as_ref(),
                    );
                }
            }
        }
        unsafe {
            rt.PopAxisAlignedClip();
        }
        if l.scroll_max > 0.0 {
            let track = vh - 12.0;
            let thumb = (track * vh / l.content_h).max(20.0);
            let y = vy + 6.0 + (track - thumb) * st.scroll / l.scroll_max;
            self.round(rt, rc(l.x + l.w - 7.0, y, 2.0, thumb), 1.0, MUTED, 0.5 * a);
        }
        let footer = if let Some(status) = st.status {
            status.to_string()
        } else if st.drop_hot {
            "松开鼠标 · 收纳到此分类".into()
        } else if st.edit_mode {
            "拖动排序 · 点 − 删除".into()
        } else {
            format!("{} 个收藏 · 中键拖动位置", q.items.len())
        };
        self.text(
            rt,
            &footer,
            rc(
                l.x + 12.0,
                l.y + l.h - PANEL_FOOTER,
                l.w - 24.0,
                PANEL_FOOTER,
            ),
            self.small.as_ref().unwrap(),
            if st.drop_hot { col } else { MUTED },
            a,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_shortcuts_have_visible_shell_icons() {
        for target in ["notepad.exe", "calc.exe", "explorer.exe", "ms-screenclip:"] {
            let pixels = Renderer::load_shell_pixels(target, IconStyle::Original, 0)
                .unwrap_or_else(|| panic!("No visible system icon for {target}"));
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] > 0));
        }
    }
    fn geom() -> RingGeom {
        RingGeom {
            cx: 500.0,
            cy: 320.0,
            r_mid: 136.0,
            stroke: 44.0,
            n: 4,
        }
    }
    #[test]
    fn moved_dock_panels_fit_and_bottom_uses_above() {
        let cfg = Config::default();
        let mut g = geom();
        for (x, y) in [
            (170.0, 170.0),
            (830.0, 170.0),
            (170.0, 630.0),
            (830.0, 630.0),
        ] {
            g.cx = x;
            g.cy = y;
            let l = layout_panel(&cfg, 61, &g, 1000.0, 800.0, 0);
            assert!(l.x >= 12.0 && l.y >= 12.0 && l.x + l.w <= 988.0 && l.y + l.h <= 788.0);
            if y == 630.0 {
                assert!(l.y + l.h < g.cy - g.r_disc());
            }
        }
    }

    #[test]
    fn compact_screens_and_large_icons_have_valid_layout() {
        let cfg = Config {
            icon_size: 96.0,
            panel_padding: 64.0,
            item_gap: 64.0,
            ..Config::default()
        };
        for (w, h) in [(320.0, 480.0), (640.0, 480.0), (1280.0, 720.0)] {
            let mut g = geom();
            g.cx = w / 2.0;
            g.cy = h * 0.4;
            let l = layout_panel(&cfg, 80, &g, w, h, 0);
            assert!(l.x >= 0.0 && l.y >= 0.0 && l.x + l.w <= w && l.y + l.h <= h);
            assert!(l.scroll_max.is_finite() && l.cols >= 1);
            let (_, y, _, ch) = l.item_cell(&cfg, 79, l.scroll_max);
            let (_, vy, _, vh) = l.viewport();
            assert!(y + ch <= vy + vh + 0.1);
        }
    }

    #[test]
    fn panel_stays_on_screen() {
        for n in [0, 1, 8, 100] {
            for q in 0..8 {
                let l = layout_panel(&Config::default(), n, &geom(), 1000.0, 800.0, q);
                assert!(l.x >= 0.0 && l.y >= 0.0 && l.x + l.w <= 1000.0 && l.y + l.h <= 800.0);
                assert!(l.cols > 0);
            }
        }
    }
    #[test]
    fn scroll_last_row_reachable() {
        let cfg = Config::default();
        let l = layout_panel(&cfg, 70, &geom(), 1000.0, 800.0, 0);
        let (_, y, _, h) = l.item_cell(&cfg, 69, l.scroll_max);
        let (_, vy, _, vh) = l.viewport();
        assert!(y + h <= vy + vh + 0.1);
    }
    #[test]
    fn corner_is_not_interactive() {
        assert!(!rounded_contains(1.0, 1.0, 0.0, 0.0, 100.0, 100.0, 20.0));
        assert!(rounded_contains(20.0, 20.0, 0.0, 0.0, 100.0, 100.0, 20.0));
    }
}
