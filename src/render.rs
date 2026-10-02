//! Direct2D 渲染：圆环弧 / 象限图标 / 中心时钟 / 玻璃面板（全矢量，无图片素材）
//!
//! 视觉语言：**极简玻璃块**。窗口为 WS_EX_LAYERED 统一透明度（~82% 不透明）：
//! 整体是一块半透明玻璃，Rgn 内全域可命中（好点）；透明度由窗口提供，
//! 绘制只管内容（深色玻璃底 + 细弧 + 线性图标 + 排版），无需抓屏/快照。
//! - 收起态：玻璃圆盘（细弧 + 断口图标 + 时钟排版）；
//! - 展开态：仅「展开象限」弧段让位，其余圆环照常 + 圆环外的玻璃面板。
use crate::config::Config;
use crate::icons::draw_icon;
use std::mem::ManuallyDrop;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F,
    D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_OPEN, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    ID2D1Brush, ID2D1DCRenderTarget, ID2D1Geometry, ID2D1SolidColorBrush,
    ID2D1StrokeStyle, D2D1_ANTIALIAS_MODE, D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_SMALL,
    D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_SOLID,
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_LAYER_OPTIONS,
    D2D1_LAYER_PARAMETERS, D2D1_LINE_JOIN_ROUND, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT,
    D2D1_STROKE_STYLE_PROPERTIES, D2D1_SWEEP_DIRECTION_CLOCKWISE,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL,
    DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, IDWriteFactory, IDWriteTextFormat,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::HDC;
use windows::core::Interface;
use windows_numerics::{Matrix3x2, Vector2};

/* ============================ 色板（深夜玻璃 + 冰蓝） ============================ */

/// 深色墨（阴影 / 图标 / 正文深色）
const INK: (f32, f32, f32) = (0.09, 0.10, 0.13);
/// 强调色：冰蓝
const ACCENT: (f32, f32, f32) = (0.45, 0.72, 1.0);

/* ============================ 几何与布局 ============================ */

pub struct RingGeom {
    pub cx: f32,
    pub cy: f32,
    pub r_mid: f32,
    pub stroke: f32,
    pub n: usize,
}

impl RingGeom {
    pub fn center_angle(&self, qi: usize) -> f32 {
        -45.0 + qi as f32 * 360.0 / self.n as f32
    }
}

/// 图标名称行高（布局与命中共用）
const ITEM_NAME_H: f32 = 18.0;

pub struct PanelLayout {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub content_h: f32,
    pub cols: usize,
    #[allow(dead_code)]
    pub rows: usize,
    pub scroll_max: f32,
}

impl PanelLayout {
    /// 图标格矩形 (x, y, w, h) —— **绘制与命中测试唯一来源**（含滚动偏移）
    pub fn item_cell(&self, cfg: &Config, i: usize, scroll: f32) -> (f32, f32, f32, f32) {
        let icon = cfg.icon_size;
        let gap = cfg.item_gap;
        let row_gap = cfg.row_gap;
        let pad = cfg.panel_padding;
        let col = i % self.cols;
        let row = i / self.cols;
        let x = self.x + pad + col as f32 * (icon + gap);
        let y = self.y + pad + row as f32 * (icon + ITEM_NAME_H + row_gap) - scroll;
        (x, y, icon, icon + ITEM_NAME_H)
    }
}

/// 面板排布：紧贴圆心朝象限展开、同类同一横排、超长换排、超高内滚
pub fn layout_panel(
    cfg: &Config,
    item_count: usize,
    g: &RingGeom,
    screen_w: f32,
    screen_h: f32,
    qi: usize,
) -> PanelLayout {
    let icon = cfg.icon_size;
    let gap = cfg.item_gap;
    let row_gap = cfg.row_gap;
    let pad = cfg.panel_padding;

    let cols = item_count.clamp(1, cfg.max_columns);
    let rows = item_count.div_ceil(cols).max(1);

    let w = pad * 2.0 + cols as f32 * icon + (cols as f32 - 1.0) * gap;
    let content_h = pad * 2.0
        + rows as f32 * (icon + ITEM_NAME_H)
        + (rows as f32 - 1.0) * row_gap;
    let h_max = screen_h * cfg.max_height_ratio;
    let h = content_h.min(h_max);
    let scroll_max = (content_h - h).max(0.0);

    let ang = g.center_angle(qi).to_radians();
    let (sx, sy) = (ang.cos(), ang.sin());
    // 面板从圆环外缘展开（不与时钟/圆环重叠 → 面板更清晰）
    let gap_out = g.r_mid + g.stroke / 2.0 + 16.0;
    let x = if sx >= 0.0 { g.cx + gap_out } else { g.cx - gap_out - w };
    let y = if sy >= 0.0 { g.cy + gap_out } else { g.cy - gap_out - h };

    let x = x.clamp(8.0, (screen_w - w - 8.0).max(8.0));
    let y = y.clamp(8.0, (screen_h - h - 8.0).max(8.0));

    PanelLayout { x, y, w, h, content_h, cols, rows, scroll_max }
}

/// 布局小工具（draw 内复用）
pub fn lay_of(st: &SceneState, qi: usize) -> PanelLayout {
    let items = st.cfg.quadrants.get(qi).map(|q| q.items.len()).unwrap_or(0);
    layout_panel(st.cfg, items, st.geom, st.screen_w, st.screen_h, qi)
}

/* ============================ 渲染器 ============================ */

pub struct SceneState<'a> {
    pub cfg: &'a Config,
    pub geom: &'a RingGeom,
    pub screen_w: f32,
    pub screen_h: f32,
    pub expanded: Option<usize>,
    pub scroll: f32,
}

pub struct Renderer {
    d2d: windows::Win32::Graphics::Direct2D::ID2D1Factory,
    dw: IDWriteFactory,
    rt: Option<ID2D1DCRenderTarget>,
    brush_white: Option<ID2D1SolidColorBrush>,
    brush_ink: Option<ID2D1SolidColorBrush>,
    brush_accent: Option<ID2D1SolidColorBrush>,
    stroke_round: Option<ID2D1StrokeStyle>,
    fmt_clock: Option<IDWriteTextFormat>,
    fmt_date: Option<IDWriteTextFormat>,
    fmt_name: Option<IDWriteTextFormat>,
}

#[inline]
fn color(r: f32, g: f32, b: f32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r, g, b, a }
}

#[inline]
fn pt(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}

#[inline]
fn rect(l: f32, t: f32, r: f32, b: f32) -> D2D_RECT_F {
    D2D_RECT_F { left: l, top: t, right: r, bottom: b }
}

#[inline]
fn identity_matrix() -> Matrix3x2 {
    Matrix3x2 { M11: 1.0, M22: 1.0, ..Default::default() }
}

impl Renderer {
    pub fn new() -> Result<Self, String> {
        use windows::Win32::Graphics::Direct2D::{D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1CreateFactory};
        let d2d = unsafe {
            D2D1CreateFactory::<windows::Win32::Graphics::Direct2D::ID2D1Factory>(
                D2D1_FACTORY_TYPE_SINGLE_THREADED,
                None,
            )
            .map_err(|e| format!("D2D 工厂创建失败：{e}"))?
        };
        let dw = unsafe {
            DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED)
                .map_err(|e| format!("DirectWrite 工厂创建失败：{e}"))?
        };
        Ok(Self {
            d2d,
            dw,
            rt: None,
            brush_white: None,
            brush_ink: None,
            brush_accent: None,
            stroke_round: None,
            fmt_clock: None,
            fmt_date: None,
            fmt_name: None,
        })
    }

    /// 确保 DC 渲染目标 + 笔刷/描边/文字格式 就绪
    fn ensure(&mut self) -> Result<(), String> {
        if self.rt.is_some() {
            return Ok(());
        }
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let rt = unsafe {
            self.d2d
                .CreateDCRenderTarget(&props)
                .map_err(|e| format!("DC 渲染目标创建失败：{e}"))?
        };
        // 圆头描边（矢量线条的质感基线）
        self.stroke_round = unsafe {
            self.d2d
                .CreateStrokeStyle(&D2D1_STROKE_STYLE_PROPERTIES {
                    startCap: D2D1_CAP_STYLE_ROUND,
                    endCap: D2D1_CAP_STYLE_ROUND,
                    dashCap: D2D1_CAP_STYLE_ROUND,
                    lineJoin: D2D1_LINE_JOIN_ROUND,
                    miterLimit: 1.0,
                    dashStyle: D2D1_DASH_STYLE_SOLID,
                    dashOffset: 0.0,
                }, None)
                .ok()
        };
        unsafe {
            self.brush_white = Some(
                rt.CreateSolidColorBrush(&color(1.0, 1.0, 1.0, 1.0), None)
                    .map_err(|e| e.to_string())?,
            );
            self.brush_ink = Some(
                rt.CreateSolidColorBrush(&color(INK.0, INK.1, INK.2, 1.0), None)
                    .map_err(|e| e.to_string())?,
            );
            self.brush_accent = Some(
                rt.CreateSolidColorBrush(&color(ACCENT.0, ACCENT.1, ACCENT.2, 1.0), None)
                    .map_err(|e| e.to_string())?,
            );
        }
        if self.fmt_clock.is_none() {
            use windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_LIGHT;
            self.fmt_clock = Some(self.make_format(34.0, DWRITE_FONT_WEIGHT_LIGHT, true)?);
            self.fmt_date = Some(self.make_format(11.0, DWRITE_FONT_WEIGHT_NORMAL, true)?);
            self.fmt_name = Some(self.make_format(11.0, DWRITE_FONT_WEIGHT_NORMAL, true)?);
        }
        self.rt = Some(rt);
        Ok(())
    }

    fn make_format(
        &self,
        size: f32,
        weight: windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT,
        center: bool,
    ) -> Result<IDWriteTextFormat, String> {
        let family = crate::sys::wide("Segoe UI");
        let locale = crate::sys::wide("zh-cn");
        let f = unsafe {
            self.dw
                .CreateTextFormat(
                    windows::core::PCWSTR(family.as_ptr()),
                    None,
                    weight,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    windows::core::PCWSTR(locale.as_ptr()),
                )
                .map_err(|e| format!("文本格式创建失败：{e}"))?
        };
        unsafe {
            let _ = f.SetTextAlignment(if center {
                DWRITE_TEXT_ALIGNMENT_CENTER
            } else {
                DWRITE_TEXT_ALIGNMENT_LEADING
            });
            let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
        }
        Ok(f)
    }

    /// 画一整帧到窗口 DC
    pub fn draw(&mut self, st: &SceneState, hwnd: HWND, hdc: HDC) -> Result<(), String> {
        self.ensure()?;
        let rt = self.rt.as_ref().unwrap().clone();
        let d2d = self.d2d.clone();

        unsafe {
            let rc = RECT {
                left: 0,
                top: 0,
                right: st.screen_w as i32,
                bottom: st.screen_h as i32,
            };
            rt.BindDC(hdc, &rc).map_err(|e| format!("BindDC: {e}"))?;
            rt.BeginDraw();

            // 0) 清为深色玻璃底（统一 alpha≈82% → 整体是"一块半透明玻璃"，
            //    透明度由窗口提供，Rgn 内全域可命中）
            rt.Clear(Some(&color(INK.0, INK.1, INK.2, 1.0) as *const D2D1_COLOR_F));

            // 1) 圆环场景：展开态仅让位「展开象限」的弧段与图标，其余照常
            self.draw_ring_scene(&rt, &d2d, st, st.expanded);

            // 2) 展开面板（绘制在圆环之上：与中心时钟重叠处由面板遮挡 → 面板更清晰）
            if let Some(qi) = st.expanded {
                let lay = lay_of(st, qi);
                self.paint_panel_glass(&rt, &d2d, st, &lay);
                self.draw_panel_items(&rt, st, qi, &lay);
            }

            rt.EndDraw(None, None).map_err(|e| format!("EndDraw 失败：{e}"))?;
        }
        let _ = hwnd;
        Ok(())
    }

    /* ---------------------- 圆环场景（极简：细弧 + 线性图标 + 时钟） ---------------------- */

    /// skip = 展开象限（它的弧段与图标让位给面板，其余圆环内容不变）
    fn draw_ring_scene(
        &self,
        rt: &windows::Win32::Graphics::Direct2D::ID2D1DCRenderTarget,
        d2d: &windows::Win32::Graphics::Direct2D::ID2D1Factory,
        st: &SceneState,
        skip: Option<usize>,
    ) {
        self.draw_ring_segments(rt, d2d, st, skip);
        self.draw_quadrant_icons(rt, st, skip);
        self.paint_clock(rt, st);
    }

    /// 细弧段：每象限两段细弧，中点断口留给象限图标
    fn draw_ring_segments(
        &self,
        rt: &windows::Win32::Graphics::Direct2D::ID2D1DCRenderTarget,
        d2d: &windows::Win32::Graphics::Direct2D::ID2D1Factory,
        st: &SceneState,
        skip: Option<usize>,
    ) {
        let g = st.geom;
        let op = st.cfg.opacity;
        let half = 180.0 / g.n as f32;
        let notch = 6.5f32; // 断口半角（图标位）
        let brush = self.ab(self.brush_accent(), 0.85 * op);
        for qi in 0..g.n {
            if skip == Some(qi) {
                continue;
            }
            let c = g.center_angle(qi);
            self.draw_arc(
                rt,
                d2d,
                &brush,
                g.cx,
                g.cy,
                g.r_mid,
                (c - half + 5.0).to_radians(),
                (c - notch).to_radians(),
                2.6,
            );
            self.draw_arc(
                rt,
                d2d,
                &brush,
                g.cx,
                g.cy,
                g.r_mid,
                (c + notch).to_radians(),
                (c + half - 5.0).to_radians(),
                2.6,
            );
        }
    }

    /// 象限图标（线性、无底座）：置于弧段断口
    fn draw_quadrant_icons(&self, rt: &windows::Win32::Graphics::Direct2D::ID2D1DCRenderTarget, st: &SceneState, skip: Option<usize>) {
        let g = st.geom;
        let op = st.cfg.opacity;
        for qi in 0..g.n {
            if skip == Some(qi) {
                continue;
            }
            let q = match st.cfg.quadrants.get(qi) {
                Some(q) => q,
                None => continue,
            };
            let ang = g.center_angle(qi).to_radians();
            let (ix, iy) = (g.cx + g.r_mid * ang.cos(), g.cy + g.r_mid * ang.sin());
            let ink = self.ab(self.brush_white(), 0.92 * op);
            draw_icon(rt, &ink, &q.kind, ix, iy, 21.0, 1.7, self.stroke_round.as_ref());
        }
    }

    /// 中心时钟：时间 + 日期，纯排版（轻阴影保证亮背景下可读）
    fn paint_clock(&self, rt: &windows::Win32::Graphics::Direct2D::ID2D1DCRenderTarget, st: &SceneState) {
        let g = st.geom;
        let op = st.cfg.opacity;
        unsafe {
            let clock = crate::sys::wide(&st.cfg.clock_text());
            let cr = rect(g.cx - 110.0, g.cy - 30.0, g.cx + 110.0, g.cy + 12.0);
            let cr2 = rect(g.cx - 109.4, g.cy - 29.0, g.cx + 110.6, g.cy + 13.0);
            let _ = rt.DrawText(
                &clock[..clock.len() - 1],
                self.fmt_clock.as_ref().unwrap(),
                &cr2 as *const _,
                &self.ab(self.brush_ink(), 0.45 * op),
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            let _ = rt.DrawText(
                &clock[..clock.len() - 1],
                self.fmt_clock.as_ref().unwrap(),
                &cr as *const _,
                &self.ab(self.brush_white(), 0.96 * op),
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );

            let (m, d, wd) = crate::sys::local_date();
            let weeks = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];
            let date = format!("{m}月{d}日 · {}", weeks[(wd as usize) % 7]);
            let dr = rect(g.cx - 110.0, g.cy + 12.0, g.cx + 110.0, g.cy + 34.0);
            let dtxt = crate::sys::wide(&date);
            let _ = rt.DrawText(
                &dtxt[..dtxt.len() - 1],
                self.fmt_date.as_ref().unwrap(),
                &dr as *const _,
                &self.ab(self.brush_white(), 0.45 * op),
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }

    /* ---------------------- 展开态：轻玻璃面板（极简） ---------------------- */

    /// 轻玻璃：深色半透明底（整窗 alpha 提供透明度）+ 1px 边界线
    fn paint_panel_glass(
        &self,
        rt: &windows::Win32::Graphics::Direct2D::ID2D1DCRenderTarget,
        d2d: &windows::Win32::Graphics::Direct2D::ID2D1Factory,
        st: &SceneState,
        lay: &PanelLayout,
    ) {
        let radius = 20.0;
        let frame = rect(lay.x, lay.y, lay.x + lay.w, lay.y + lay.h);
        unsafe {
            let mask = match d2d.CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT {
                rect: frame,
                radiusX: radius,
                radiusY: radius,
            }) {
                Ok(g) => g,
                Err(_) => return,
            };
            let mask_geo: ID2D1Geometry = match mask.cast() {
                Ok(g) => g,
                Err(_) => return,
            };
            push_geo_layer(rt, st, mask_geo);
            // 面板层：比圆盘底稍亮的玻璃（层次区分，透明度仍由窗口统一提供）
            let _ = rt.FillRectangle(&frame, &self.ab(self.brush_white(), 0.07));
            rt.PopLayer();
            let rr = D2D1_ROUNDED_RECT { rect: frame, radiusX: radius, radiusY: radius };
            let _ = rt.DrawRoundedRectangle(&rr, &self.ab(self.brush_white(), 0.14), 1.0, None);
        }
    }

    /// 面板内容：线性图标 + 名称（无卡片底、无投影）；滚动条仅内容超多时浮现
    fn draw_panel_items(&self, rt: &windows::Win32::Graphics::Direct2D::ID2D1DCRenderTarget, st: &SceneState, qi: usize, lay: &PanelLayout) {
        let cfg = st.cfg;
        let q = match cfg.quadrants.get(qi) {
            Some(q) => q,
            None => return,
        };
        let icon = cfg.icon_size;
        let gap = cfg.item_gap;

        unsafe {
            let clip = match self
                .d2d
                .CreateRectangleGeometry(&rect(lay.x, lay.y, lay.x + lay.w, lay.y + lay.h))
            {
                Ok(g) => g,
                Err(_) => return,
            };
            let clip_geo: ID2D1Geometry = match clip.cast() {
                Ok(g) => g,
                Err(_) => return,
            };
            push_geo_layer(rt, st, clip_geo);

            for (i, item) in q.items.iter().enumerate() {
                let (x, y, cw, chh) = lay.item_cell(cfg, i, st.scroll);
                if y > lay.y + lay.h + 8.0 || y + chh < lay.y - 8.0 {
                    continue;
                }
                let (cx, cy) = (x + cw / 2.0, y + icon * 0.5);

                // 线性图标
                let ink = self.ab(self.brush_white(), 0.92);
                draw_icon(rt, &ink, &item.kind, cx, cy, icon * 0.44, 1.8, self.stroke_round.as_ref());

                // 名称
                let name: String = {
                    let mut s = item.name.clone();
                    if s.chars().count() > 6 {
                        s = s.chars().take(6).collect::<String>() + "…";
                    }
                    s
                };
                let nr = rect(x - gap / 2.0, y + icon + 1.0, x + cw + gap / 2.0, y + chh);
                let nw = crate::sys::wide(&name);
                let _ = rt.DrawText(
                    &nw[..nw.len() - 1],
                    self.fmt_name.as_ref().unwrap(),
                    &nr as *const _,
                    &self.ab(self.brush_white(), 0.65),
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
            rt.PopLayer();

            if lay.scroll_max > 1.0 {
                let track_w = 3.0;
                let tx = lay.x + lay.w - track_w - 8.0;
                let th = lay.h - 20.0;
                let ratio = (lay.h / lay.content_h).clamp(0.12, 1.0);
                let bar_h = th * ratio;
                let bar_y = lay.y + 10.0 + (th - bar_h) * (st.scroll / lay.scroll_max.max(1.0));
                let track = D2D1_ROUNDED_RECT {
                    rect: rect(tx, lay.y + 10.0, tx + track_w, lay.y + 10.0 + th),
                    radiusX: 1.5,
                    radiusY: 1.5,
                };
                let _ = rt.FillRoundedRectangle(&track, &self.ab(self.brush_white(), 0.12));
                let bar = D2D1_ROUNDED_RECT {
                    rect: rect(tx, bar_y, tx + track_w, bar_y + bar_h),
                    radiusX: 1.5,
                    radiusY: 1.5,
                };
                let _ = rt.FillRoundedRectangle(&bar, &self.ab(self.brush_white(), 0.55));
            }
        }
    }
    /// 单段圆弧（圆头描边）
    fn draw_arc(
        &self,
        rt: &windows::Win32::Graphics::Direct2D::ID2D1DCRenderTarget,
        d2d: &windows::Win32::Graphics::Direct2D::ID2D1Factory,
        brush: &ID2D1Brush,
        cx: f32,
        cy: f32,
        r: f32,
        a0: f32,
        a1: f32,
        stroke: f32,
    ) {
        unsafe {
            let geo = match d2d.CreatePathGeometry() {
                Ok(g) => g,
                Err(_) => return,
            };
            if let Ok(sink) = geo.Open() {
                let p0 = pt(cx + r * a0.cos(), cy + r * a0.sin());
                let p1 = pt(cx + r * a1.cos(), cy + r * a1.sin());
                sink.BeginFigure(p0, D2D1_FIGURE_BEGIN_HOLLOW);
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: p1,
                    size: D2D_SIZE_F { width: r, height: r },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                    arcSize: D2D1_ARC_SIZE_SMALL,
                });
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
                let _ = sink.Close();
            }
            let _ = rt.DrawGeometry(&geo, brush, stroke, self.stroke_round.as_ref());
        }
    }

    /* 笔刷快捷方式（字段一定已初始化） */
    fn brush_white(&self) -> &ID2D1SolidColorBrush {
        self.brush_white.as_ref().unwrap()
    }
    fn brush_ink(&self) -> &ID2D1SolidColorBrush {
        self.brush_ink.as_ref().unwrap()
    }
    fn brush_accent(&self) -> &ID2D1SolidColorBrush {
        self.brush_accent.as_ref().unwrap()
    }

    fn ab(&self, src: &ID2D1SolidColorBrush, alpha: f32) -> ID2D1SolidColorBrush {
        self.alpha_brush(src, alpha).unwrap_or_else(|_| src.clone())
    }

    fn alpha_brush(
        &self,
        src: &ID2D1SolidColorBrush,
        alpha: f32,
    ) -> Result<ID2D1SolidColorBrush, String> {
        let rt = self.rt.as_ref().ok_or("渲染目标未就绪")?;
        unsafe {
            let c = src.GetColor();
            rt.CreateSolidColorBrush(&color(c.r, c.g, c.b, (c.a * alpha).clamp(0.0, 1.0)), None)
                .map_err(|e| e.to_string())
        }
    }
}

/* ============================ 绘制小工具 ============================ */

/// 几何遮罩图层（作用域内绘制被裁剪到几何形状）
fn push_geo_layer(rt: &windows::Win32::Graphics::Direct2D::ID2D1DCRenderTarget, st: &SceneState, geo: ID2D1Geometry) {
    let lp = D2D1_LAYER_PARAMETERS {
        contentBounds: rect(0.0, 0.0, st.screen_w, st.screen_h),
        geometricMask: ManuallyDrop::new(Some(geo)),
        maskAntialiasMode: D2D1_ANTIALIAS_MODE(1),
        maskTransform: identity_matrix(),
        opacity: 1.0,
        opacityBrush: ManuallyDrop::new(None),
        layerOptions: D2D1_LAYER_OPTIONS(0),
    };
    unsafe {
        rt.PushLayer(&lp, None);
    }
}
