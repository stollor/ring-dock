//! 桌面底图抓取 + 形状边界带「向外采样重建」—— 给抗锯齿边缘提供正确的混色底
//!
//! 背景：普通窗口 + SetWindowRgn 的形状边界是**硬切**的（GDI 区域无 alpha），
//! 要让 D2D 画出的形状边缘真正平滑，边缘像素必须与「真实桌面」混色。
//!
//! 两个关键点（缺一不可）：
//! 1. **抓屏内容含自己上一帧**：直接当底图会把上一帧的混色再混一次 → 边缘逐帧变硬（反馈）。
//!    解法：对每个形状的边界带（±BAND 像素）做「向外采样」重建 —— 用形状外 `SAMPLE` 像素处的
//!    抓屏内容覆盖。那里位于 SetWindowRgn 之外，永远是真实桌面（不被自身绘制污染）。
//! 2. **采样距离 > Rgn 外边距**：边界带内的采样点必须落在 Rgn 之外（真实桌面），
//!    否则会采到自己上一帧的内容（缓慢漂移）。`SAMPLE > BAND` 即可保证。
use std::ffi::c_void;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC, HGDIOBJ, SRCCOPY,
};

/// 边界带半宽（含 Rgn 外边距，须覆盖抗锯齿过渡区）
pub const BAND: f32 = 2.5;
/// 向外采样距离（须 > BAND，保证采样点落在 Rgn 外 = 真实桌面）
pub const SAMPLE: f32 = 4.0;

/// 一段屏幕像素（BGRA，alpha 统一 255）
pub struct Grab {
    pub x: i32,
    pub y: i32,
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

/// 需要重建边界带的形状（窗口客户区坐标）
pub enum Shape {
    Disc { cx: f32, cy: f32, r: f32 },
    Rect { x0: f32, y0: f32, x1: f32, y1: f32 },
}

impl Shape {
    /// 到边界的有符号距离（外部为正）
    fn signed_dist(&self, px: f32, py: f32) -> f32 {
        match *self {
            Shape::Disc { cx, cy, r } => ((px - cx).powi(2) + (py - cy).powi(2)).sqrt() - r,
            Shape::Rect { x0, y0, x1, y1 } => {
                let dx = (x0 - px).max(px - x1);
                let dy = (y0 - py).max(py - y1);
                if dx > 0.0 || dy > 0.0 {
                    dx.max(dy) // 外部
                } else {
                    dx.min(dy) // 内部（负值）
                }
            }
        }
    }

    /// 指向形状外部的单位向量
    fn outward(&self, px: f32, py: f32) -> (f32, f32) {
        match *self {
            Shape::Disc { cx, cy, .. } => {
                let (dx, dy) = (px - cx, py - cy);
                let d = (dx * dx + dy * dy).sqrt().max(1e-3);
                (dx / d, dy / d)
            }
            Shape::Rect { x0, y0, x1, y1 } => {
                let dx = (x0 - px).max(px - x1);
                let dy = (y0 - py).max(py - y1);
                if dx.abs() >= dy.abs() {
                    ((x0 - px).signum(), 0.0)
                } else {
                    (0.0, (y0 - py).signum())
                }
            }
        }
    }
}

/// 抓取屏幕区域（客户区坐标 + 窗口原点 → 屏幕坐标）
pub fn grab_screen(ox: i32, oy: i32, x: f32, y: f32, w: usize, h: usize) -> Option<Grab> {
    unsafe {
        let hdc_screen: HDC = GetDC(None::<windows::Win32::Foundation::HWND>);
        if hdc_screen.is_invalid() {
            return None;
        }
        let mem = CreateCompatibleDC(Some(hdc_screen));
        let mut bmi = BITMAPINFO::default();
        bmi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let dib = match CreateDIBSection(
            Some(hdc_screen),
            &bmi,
            DIB_RGB_COLORS,
            &mut bits,
            None::<HANDLE>,
            0,
        ) {
            Ok(d) if !d.is_invalid() && !bits.is_null() => d,
            _ => {
                let _ = DeleteDC(mem);
                let _ = ReleaseDC(None::<windows::Win32::Foundation::HWND>, hdc_screen);
                return None;
            }
        };
        let old = SelectObject(mem, dib.into());
        let _ = BitBlt(mem, 0, 0, w as i32, h as i32, Some(hdc_screen), ox + x as i32, oy + y as i32, SRCCOPY);
        let mut px = vec![0u8; w * h * 4];
        std::ptr::copy_nonoverlapping(bits as *const u8, px.as_mut_ptr(), w * h * 4);
        // GDI BitBlt 不写 alpha 通道 → 统一置 255（此时 premultiplied == straight）
        for i in (3..px.len()).step_by(4) {
            px[i] = 255;
        }
        SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(dib.0));
        let _ = DeleteDC(mem);
        let _ = ReleaseDC(None::<windows::Win32::Foundation::HWND>, hdc_screen);
        Some(Grab { x: x as i32, y: y as i32, w, h, px })
    }
}

/// 边界带重建：把每个形状 ±BAND 像素内的抓屏内容换成「向外 SAMPLE 像素」处的内容。
/// 只动边界带（含 Rgn 外边距），形状内部反正会被不透明填充覆盖。
pub fn reconstruct(g: &mut Grab, shapes: &[Shape]) {
    if shapes.is_empty() {
        return;
    }
    let (w, h) = (g.w as i32, g.h as i32);
    // 逐形状处理：边界带按“离边界距离”筛选；后处理的形状在重叠处优先（重叠区最终被
    // 不透明形状填充覆盖，此处近似即可）
    for shape in shapes {
        // 该形状的边界带包围盒（像素扫描范围）
        let (bx0, by0, bx1, by1) = band_bbox(shape, g.x as f32, g.y as f32, w as f32, h as f32);
        // 先把带内的源像素收集出来（避免读写同一缓冲互相污染）
        let mut patch: Vec<(usize, [u8; 4])> = Vec::new();
        for py in by0..by1 {
            for px in bx0..bx1 {
                let (cx, cy) = (px as f32 + g.x as f32 + 0.5, py as f32 + g.y as f32 + 0.5);
                let sd = shape.signed_dist(cx, cy);
                if sd.abs() > BAND {
                    continue;
                }
                let (ux, uy) = shape.outward(cx, cy);
                let sx = cx + ux * SAMPLE;
                let sy = cy + uy * SAMPLE;
                let (ix, iy) = ((sx - g.x as f32) as i32, (sy - g.y as f32) as i32);
                if ix < 0 || iy < 0 || ix >= w || iy >= h {
                    continue;
                }
                let s = (iy as usize * g.w + ix as usize) * 4;
                let d = (py as usize * g.w + px as usize) * 4;
                patch.push((d, [g.px[s], g.px[s + 1], g.px[s + 2], g.px[s + 3]]));
            }
        }
        for (d, v) in patch {
            g.px[d..d + 4].copy_from_slice(&v);
        }
    }
}

/// 边界带在抓缓冲内的扫描范围（与抓取范围求交）
fn band_bbox(shape: &Shape, gx: f32, gy: f32, w: f32, h: f32) -> (i32, i32, i32, i32) {
    let (x0, y0, x1, y1) = match *shape {
        Shape::Disc { cx, cy, r } => (cx - r - BAND, cy - r - BAND, cx + r + BAND, cy + r + BAND),
        Shape::Rect { x0, y0, x1, y1 } => (x0 - BAND, y0 - BAND, x1 + BAND, y1 + BAND),
    };
    let bx0 = ((x0 - gx).floor().max(0.0)) as i32;
    let by0 = ((y0 - gy).floor().max(0.0)) as i32;
    let bx1 = ((x1 - gx).ceil().min(w)) as i32;
    let by1 = ((y1 - gy).ceil().min(h)) as i32;
    (bx0, by0, bx1, by1)
}
