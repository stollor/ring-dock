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
    D2D_RECT_F {
        left: l,
        top: t,
        right: r,
        bottom: b,
    }
}

/// 在 (cx, cy) 画 kind 图标，size 为图标内图形边长
#[allow(clippy::too_many_arguments)] // Explicit vector geometry parameters.
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
            "collaboration" => {
                // Two overlapping chat bubbles: everyday communication and teamwork.
                let back = D2D1_ROUNDED_RECT {
                    rect: rect(cx - s * 0.05, cy - s * 0.78, cx + s * 0.95, cy + s * 0.30),
                    radiusX: s * 0.20,
                    radiusY: s * 0.20,
                };
                rt.DrawRoundedRectangle(&back, brush, stroke * 0.9, style);
                rt.DrawLine(
                    pt(cx + s * 0.46, cy + s * 0.30),
                    pt(cx + s * 0.30, cy + s * 0.55),
                    brush,
                    stroke * 0.9,
                    style,
                );
                let front = D2D1_ROUNDED_RECT {
                    rect: rect(cx - s * 1.00, cy - s * 0.56, cx + s * 0.45, cy + s * 0.48),
                    radiusX: s * 0.20,
                    radiusY: s * 0.20,
                };
                rt.DrawRoundedRectangle(&front, brush, stroke, style);
                rt.DrawLine(
                    pt(cx - s * 0.54, cy + s * 0.48),
                    pt(cx - s * 0.72, cy + s * 0.73),
                    brush,
                    stroke,
                    style,
                );
                rt.DrawLine(
                    pt(cx - s * 0.72, cy + s * 0.73),
                    pt(cx - s * 0.18, cy + s * 0.48),
                    brush,
                    stroke,
                    style,
                );
            }
            "development" => {
                // Code brackets with a clear center slash.
                rt.DrawLine(
                    pt(cx - s * 0.48, cy - s * 0.58),
                    pt(cx - s * 1.02, cy),
                    brush,
                    stroke,
                    style,
                );
                rt.DrawLine(
                    pt(cx - s * 1.02, cy),
                    pt(cx - s * 0.48, cy + s * 0.58),
                    brush,
                    stroke,
                    style,
                );
                rt.DrawLine(
                    pt(cx + s * 0.48, cy - s * 0.58),
                    pt(cx + s * 1.02, cy),
                    brush,
                    stroke,
                    style,
                );
                rt.DrawLine(
                    pt(cx + s * 1.02, cy),
                    pt(cx + s * 0.48, cy + s * 0.58),
                    brush,
                    stroke,
                    style,
                );
                rt.DrawLine(
                    pt(cx + s * 0.25, cy - s * 0.86),
                    pt(cx - s * 0.25, cy + s * 0.86),
                    brush,
                    stroke * 0.9,
                    style,
                );
            }
            "ai" => {
                // A bright four-point spark with two smaller orbiting glints.
                let rays = [
                    (0.0, -1.05),
                    (0.24, -0.24),
                    (1.0, 0.0),
                    (0.24, 0.24),
                    (0.0, 1.05),
                    (-0.24, 0.24),
                    (-1.0, 0.0),
                    (-0.24, -0.24),
                ];
                for index in 0..rays.len() {
                    let (x1, y1) = rays[index];
                    let (x2, y2) = rays[(index + 1) % rays.len()];
                    rt.DrawLine(
                        pt(cx + x1 * s * 0.78, cy + y1 * s * 0.78),
                        pt(cx + x2 * s * 0.78, cy + y2 * s * 0.78),
                        brush,
                        stroke,
                        style,
                    );
                }
                let small = D2D1_ELLIPSE {
                    point: pt(cx + s * 0.78, cy - s * 0.72),
                    radiusX: stroke * 0.72,
                    radiusY: stroke * 0.72,
                };
                rt.FillEllipse(&small, brush);
                rt.DrawLine(
                    pt(cx - s * 0.95, cy - s * 0.76),
                    pt(cx - s * 0.95, cy - s * 0.32),
                    brush,
                    stroke * 0.85,
                    style,
                );
                rt.DrawLine(
                    pt(cx - s * 1.17, cy - s * 0.54),
                    pt(cx - s * 0.73, cy - s * 0.54),
                    brush,
                    stroke * 0.85,
                    style,
                );
            }
            "entertainment" => {
                // Media play control: a compact circle and forward-facing play mark.
                let disc = D2D1_ELLIPSE {
                    point: pt(cx, cy),
                    radiusX: s * 0.88,
                    radiusY: s * 0.88,
                };
                rt.DrawEllipse(&disc, brush, stroke, style);
                rt.DrawLine(
                    pt(cx - s * 0.20, cy - s * 0.38),
                    pt(cx + s * 0.40, cy),
                    brush,
                    stroke,
                    style,
                );
                rt.DrawLine(
                    pt(cx + s * 0.40, cy),
                    pt(cx - s * 0.20, cy + s * 0.38),
                    brush,
                    stroke,
                    style,
                );
                rt.DrawLine(
                    pt(cx - s * 0.20, cy + s * 0.38),
                    pt(cx - s * 0.20, cy - s * 0.38),
                    brush,
                    stroke,
                    style,
                );
            }
            "program" => {
                // 窗口：圆角外框 + 标题栏线 + 两颗窗钮
                let w = s * 1.86;
                let h = s * 1.42;
                let rr = D2D1_ROUNDED_RECT {
                    rect: rect(cx - w, cy - h, cx + w, cy + h),
                    radiusX: s * 0.30,
                    radiusY: s * 0.30,
                };
                rt.DrawRoundedRectangle(&rr, brush, stroke, style);
                let bar = cy - h * 0.42;
                rt.DrawLine(
                    pt(cx - w, bar),
                    pt(cx + w, bar),
                    brush,
                    stroke * 0.82,
                    style,
                );
                for dx in [-w * 0.72, -w * 0.52] {
                    let dot = D2D1_ELLIPSE {
                        point: pt(cx + dx, cy - h * 0.71),
                        radiusX: stroke * 0.52,
                        radiusY: stroke * 0.52,
                    };
                    rt.FillEllipse(&dot, brush);
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
                rt.DrawLine(pt(l, t), pt(r - fold, t), brush, stroke, style);
                rt.DrawLine(pt(r - fold, t), pt(r, t + fold), brush, stroke, style);
                rt.DrawLine(pt(r, t + fold), pt(r, b), brush, stroke, style);
                rt.DrawLine(pt(r, b), pt(l, b), brush, stroke, style);
                rt.DrawLine(pt(l, b), pt(l, t), brush, stroke, style);
                // 折角
                rt.DrawLine(
                    pt(r - fold, t),
                    pt(r - fold, t + fold),
                    brush,
                    stroke * 0.82,
                    style,
                );
                rt.DrawLine(
                    pt(r - fold, t + fold),
                    pt(r, t + fold),
                    brush,
                    stroke * 0.82,
                    style,
                );
                // 内容线
                rt.DrawLine(
                    pt(l + s * 0.30, cy + s * 0.08),
                    pt(r - s * 0.30, cy + s * 0.08),
                    brush,
                    stroke * 0.82,
                    style,
                );
                rt.DrawLine(
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
                rt.DrawRoundedRectangle(&tab, brush, stroke, style);
                // 身体
                let body = D2D1_ROUNDED_RECT {
                    rect: rect(l, body_t, r, b),
                    radiusX: s * 0.22,
                    radiusY: s * 0.22,
                };
                rt.DrawRoundedRectangle(&body, brush, stroke, style);
                // 身体顶边压在标签上（去掉重叠线）
                rt.DrawLine(
                    pt(l + stroke, body_t),
                    pt(r - stroke, body_t),
                    brush,
                    stroke,
                    style,
                );
            }
            "url" => {
                // 地球：外圆 + 经线椭圆 + 赤道 + 斜经线
                let rr = s * 0.94;
                let e = D2D1_ELLIPSE {
                    point: pt(cx, cy),
                    radiusX: rr,
                    radiusY: rr,
                };
                rt.DrawEllipse(&e, brush, stroke, style);
                let e2 = D2D1_ELLIPSE {
                    point: pt(cx, cy),
                    radiusX: rr * 0.42,
                    radiusY: rr,
                };
                rt.DrawEllipse(&e2, brush, stroke * 0.82, style);
                rt.DrawLine(
                    pt(cx - rr, cy),
                    pt(cx + rr, cy),
                    brush,
                    stroke * 0.82,
                    style,
                );
                rt.DrawLine(
                    pt(cx - rr * 0.86, cy - rr * 0.50),
                    pt(cx + rr * 0.86, cy - rr * 0.50),
                    brush,
                    stroke * 0.72,
                    style,
                );
                rt.DrawLine(
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
                rt.DrawRoundedRectangle(&rr, brush, stroke, style);
            }
        }
    }
}

pub fn category_icon(label: &str) -> Option<&'static str> {
    match label {
        "日常协作" => Some("collaboration"),
        "开发创作" => Some("development"),
        "AI助手" | "AI 助手" => Some("ai"),
        "娱乐影音" => Some("entertainment"),
        _ => None,
    }
}
