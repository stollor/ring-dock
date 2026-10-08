//! 文字 → 窗口区域（GDI 路径）：让时钟文字“悬浮”在透明中心
//!
//! 桌面子窗口没有任何透明机制（α/色键都不合成，见 HANDOFF §四-G），真透明只能靠
//! **窗口形状挖洞**：Rgn 之外没有窗口像素 → 壁纸自然透出。于是文字要显示在透明中心，
//! 就必须把「文字的形状」并进 Rgn —— 本模块用 GDI 文字路径构造：
//! - `fill`：字形填充区域（PathToRegion）；
//! - `halo`：把字形轮廓按笔宽加宽后的区域（WidenPath）→ 作为深色描边/光晕，
//!   让白字在亮壁纸上也清晰，同时给硬切的形状边缘一个自然的深色过渡。
use windows::Win32::Graphics::Gdi::{
    BeginPath, CombineRgn, CreateFontW, DeleteObject, DrawTextW, EndPath, ExtCreatePen, PathToRegion,
    SelectObject, WidenPath, BS_SOLID, DT_CENTER, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, FW_LIGHT,
    FW_NORMAL, HDC, HGDIOBJ, HRGN, LOGBRUSH, PS_ENDCAP_ROUND, PS_GEOMETRIC, PS_JOIN_ROUND, PS_SOLID,
    RGN_OR,
};

/// 在 (x, y, w, h) 矩形内按 GDI 排版构造文字区域（与 render.rs 的绘制同字体/同矩形）；
/// halo = 字形外扩的光晕像素（深色描边，保证亮壁纸上可读）
#[allow(clippy::too_many_arguments)]
pub fn text_region(
        hdc: HDC,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        family: &str,
        px: f32,
        weight: i32,
        halo: u32,
    ) -> Option<HRGN> {
    unsafe {
            // 字体（与 render.rs 的绘制同字体/同字号 → 窗口形状与可见文字精确对齐）
            let face = crate::sys::wide(family);
            let font = match CreateFontW(
                -(px as i32), 0, 0, 0, weight,
                0, 0, 0,
                Default::default(), Default::default(), Default::default(), Default::default(),
                0,
                windows::core::PCWSTR(face.as_ptr()),
            ) {
                f if !f.is_invalid() => f,
                _ => return None,
            };

            let mut wt = crate::sys::wide(text);
            wt.pop(); // 去掉结尾 \0（DrawTextW 用长度）
            let mut rc = windows::Win32::Foundation::RECT {
                left: x as i32,
                top: y as i32,
                right: (x + w) as i32,
                bottom: (y + h) as i32,
            };
            let flags = DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX;
            let old_font = SelectObject(hdc, HGDIOBJ(font.0));

            // 1) 字形填充区
            let _ = BeginPath(hdc);
            let _ = DrawTextW(hdc, &mut wt, &mut rc, flags);
            let _ = EndPath(hdc);
            let fill = PathToRegion(hdc);
            if fill.is_invalid() {
                SelectObject(hdc, old_font);
                let _ = DeleteObject(HGDIOBJ(font.0));
                return None;
            }

            // 2) 光晕：选宽画笔后把轮廓加宽（字形外扩 halo 像素）
            let pen = ExtCreatePen(
                PS_GEOMETRIC | PS_SOLID | PS_JOIN_ROUND | PS_ENDCAP_ROUND,
                halo,
                &LOGBRUSH {
                    lbStyle: BS_SOLID,
                    lbColor: windows::Win32::Foundation::COLORREF(0),
                    lbHatch: 0,
                },
                None,
            );
            if pen.is_invalid() {
                SelectObject(hdc, old_font);
                let _ = DeleteObject(HGDIOBJ(font.0));
                return Some(fill);
            }
            let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));
            let _ = BeginPath(hdc);
            let _ = DrawTextW(hdc, &mut wt, &mut rc, flags);
            let _ = EndPath(hdc);
            let _ = WidenPath(hdc);
            let halo_rgn = PathToRegion(hdc);
            SelectObject(hdc, old_pen);
            SelectObject(hdc, old_font);
            let _ = DeleteObject(HGDIOBJ(pen.0));
            let _ = DeleteObject(HGDIOBJ(font.0));

            if halo_rgn.is_invalid() {
                return Some(fill);
            }
            // 并集：光晕 + 字形
            let _ = CombineRgn(Some(fill), Some(fill), Some(halo_rgn), RGN_OR);
            let _ = DeleteObject(HGDIOBJ(halo_rgn.0));
            Some(fill)
        }
    }

/// 时钟两行文字（时间 + 日期）的合成区域；返回 None 表示构造失败（回退整圆盘）
pub fn clock_region(hdc: HDC, cx: f32, cy: f32, time: &str, date: &str) -> Option<HRGN> {
    let t = text_region(
        hdc,
        time,
        cx - 110.0,
        cy - 30.0,
        220.0,
        42.0,
        "Segoe UI Light",
        34.0,
        FW_LIGHT.0 as i32,
        5,
    )?;
    let d = text_region(
        hdc,
        date,
        cx - 110.0,
        cy + 12.0,
        220.0,
        22.0,
        "Segoe UI",
        11.0,
        FW_NORMAL.0 as i32,
        4,
    )?;
    unsafe {
        let _ = CombineRgn(Some(t), Some(t), Some(d), RGN_OR);
        let _ = windows::Win32::Graphics::Gdi::DeleteObject(HGDIOBJ(d.0));
        Some(t)
    }
}
