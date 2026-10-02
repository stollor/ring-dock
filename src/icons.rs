//! 矢量图标 —— 全代码绘制，无图片素材（圆头描边，风格与圆环/面板统一）
//! 四类：program（窗口）/ file（折角文档）/ folder（文件夹）/ url（地球）
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{
    ID2D1RenderTarget, ID2D1SolidColorBrush, ID2D1StrokeStyle, D2D1_ELLIPSE, D2D1_ROUNDED_RECT,
};
use windows_numerics::Vector2;

#[inline]
fn pt(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}

#[inline]
fn rect(l: f32, t: f32, r: f32, b: f32) -> D2D_RECT_F {
    D2D_RECT_F { left: l, top: t, right: r, bottom: b }
}

/// 在 (cx, cy) 画 kind 图标，size 为图标内图形边长
pub fn draw_icon(
    rt: &ID2D1RenderTarget,
    brush: &ID2D1SolidColorBrush,
    kind: &str,
    cx: f32,
    cy: f32,
    size: f32,
    stroke: f32,
    style: Option<&ID2D1StrokeStyle>,
) {
    let s = size * 0.5;
    unsafe {
        match kind {
            "program" => {
                // 窗口：圆角外框 + 标题栏线 + 两颗窗钮
                let w = s * 1.86;
                let h = s * 1.42;
                let rr = D2D1_ROUNDED_RECT {
                    rect: rect(cx - w, cy - h, cx + w, cy + h),
                    radiusX: s * 0.30,
                    radiusY: s * 0.30,
                };
                let _ = rt.DrawRoundedRectangle(&rr, brush, stroke, style);
                let bar = cy - h * 0.42;
                let _ = rt.DrawLine(pt(cx - w, bar), pt(cx + w, bar), brush, stroke * 0.82, style);
                for dx in [-w * 0.72, -w * 0.52] {
                    let dot = D2D1_ELLIPSE {
                        point: pt(cx + dx, cy - h * 0.71),
                        radiusX: stroke * 0.52,
                        radiusY: stroke * 0.52,
                    };
                    let _ = rt.FillEllipse(&dot, brush);
                }
            }
            "file" => {
                // 折角文档：轮廓 + 折角 + 两条内容线
                let w = s * 0.78;
                let h = s * 1.02;
                let fold = s * 0.46;
                let l = cx - w;
                let t = cy - h;
                let r = cx + w;
                let b = cy + h;
                let _ = rt.DrawLine(pt(l, t), pt(r - fold, t), brush, stroke, style);
                let _ = rt.DrawLine(pt(r - fold, t), pt(r, t + fold), brush, stroke, style);
                let _ = rt.DrawLine(pt(r, t + fold), pt(r, b), brush, stroke, style);
                let _ = rt.DrawLine(pt(r, b), pt(l, b), brush, stroke, style);
                let _ = rt.DrawLine(pt(l, b), pt(l, t), brush, stroke, style);
                // 折角
                let _ = rt.DrawLine(pt(r - fold, t), pt(r - fold, t + fold), brush, stroke * 0.82, style);
                let _ = rt.DrawLine(pt(r - fold, t + fold), pt(r, t + fold), brush, stroke * 0.82, style);
                // 内容线
                let _ = rt.DrawLine(
                    pt(l + s * 0.30, cy + s * 0.08),
                    pt(r - s * 0.30, cy + s * 0.08),
                    brush,
                    stroke * 0.82,
                    style,
                );
                let _ = rt.DrawLine(
                    pt(l + s * 0.30, cy + s * 0.50),
                    pt(r - s * 0.52, cy + s * 0.50),
                    brush,
                    stroke * 0.82,
                    style,
                );
            }
            "folder" => {
                // 文件夹：圆角标签页 + 圆角身体
                let l = cx - s * 1.02;
                let r = cx + s * 1.02;
                let tab_h = s * 0.30;
                let body_t = cy - s * 0.38;
                let b = cy + s * 0.82;
                // 标签页（左上小圆角梯形简化为圆角矩形上缘）
                let tab = D2D1_ROUNDED_RECT {
                    rect: rect(l, body_t - tab_h, l + s * 0.72, body_t + s * 0.10),
                    radiusX: s * 0.16,
                    radiusY: s * 0.16,
                };
                let _ = rt.DrawRoundedRectangle(&tab, brush, stroke, style);
                // 身体
                let body = D2D1_ROUNDED_RECT {
                    rect: rect(l, body_t, r, b),
                    radiusX: s * 0.22,
                    radiusY: s * 0.22,
                };
                let _ = rt.DrawRoundedRectangle(&body, brush, stroke, style);
                // 身体顶边压在标签上（去掉重叠线）
                let _ = rt.DrawLine(pt(l + stroke, body_t), pt(r - stroke, body_t), brush, stroke, style);
            }
            "url" => {
                // 地球：外圆 + 经线椭圆 + 赤道 + 斜经线
                let rr = s * 0.94;
                let e = D2D1_ELLIPSE { point: pt(cx, cy), radiusX: rr, radiusY: rr };
                let _ = rt.DrawEllipse(&e, brush, stroke, style);
                let e2 = D2D1_ELLIPSE {
                    point: pt(cx, cy),
                    radiusX: rr * 0.42,
                    radiusY: rr,
                };
                let _ = rt.DrawEllipse(&e2, brush, stroke * 0.82, style);
                let _ = rt.DrawLine(pt(cx - rr, cy), pt(cx + rr, cy), brush, stroke * 0.82, style);
                let _ = rt.DrawLine(
                    pt(cx - rr * 0.86, cy - rr * 0.50),
                    pt(cx + rr * 0.86, cy - rr * 0.50),
                    brush,
                    stroke * 0.72,
                    style,
                );
                let _ = rt.DrawLine(
                    pt(cx - rr * 0.86, cy + rr * 0.50),
                    pt(cx + rr * 0.86, cy + rr * 0.50),
                    brush,
                    stroke * 0.72,
                    style,
                );
            }
            _ => {
                let rr = D2D1_ROUNDED_RECT {
                    rect: rect(cx - s * 0.8, cy - s * 0.8, cx + s * 0.8, cy + s * 0.8),
                    radiusX: s * 0.24,
                    radiusY: s * 0.24,
                };
                let _ = rt.DrawRoundedRectangle(&rr, brush, stroke, style);
            }
        }
    }
}
