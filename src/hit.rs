//! 点击命中测试（纯函数）——**与绘制几何严格一一对应**，杜绝「点 A 处触发 B 逻辑」。
//!
//! 判定规则（与 `render.rs` 的绘制完全同源）：
//! - 弧段 i 覆盖角度 `[center_angle(i) - half + GAP, center_angle(i) + half - GAP]`
//!   （`half = 180/n`，`GAP = 4°` 与绘制缩进一致）→ 弧缝内是**空白**，不属任何象限；
//! - 中心圆（时钟区）为 `Center`；
//! - 面板内按图标格精确判定 `Item`，面板其余为 `PanelBlank`；
//! - 其余位置为 `None`（圆外 / 弧缝）。
use crate::config::Config;
use crate::render::{layout_panel, RingGeom};

/// 绘制弧段的角向缩进（度），必须与 render.rs 的 `+4.0 / -4.0` 保持一致
const ARC_GAP_DEG: f32 = 4.0;

/// 点击命中结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// 环带弧段（某象限）
    Quadrant(usize),
    /// 中心圆（时钟区）
    Center,
    /// 面板内图标（展开象限、条目序号）
    Item { qi: usize, index: usize },
    /// 面板内空白（图标间隙 / 滚出区域）
    PanelBlank,
    /// 空白处：圆外 / 弧缝
    None,
}

/// 两角之差，归一到 (-180, 180]
fn ang_diff(a: f32, b: f32) -> f32 {
    (a - b + 180.0).rem_euclid(360.0) - 180.0
}

/// 命中测试：x/y 为窗口客户区坐标（与绘制同一坐标系）。
/// 判定顺序 = 绘制层级（越晚绘制越优先）：面板图标 > 弧段 > 面板背板 > 中心圆 > 空白。
pub fn hit_test(
    cfg: &Config,
    geom: &RingGeom,
    screen_w: f32,
    screen_h: f32,
    expanded: Option<usize>,
    scroll: f32,
    x: f32,
    y: f32,
) -> Hit {
    // 面板布局（有展开时复用）
    let panel = expanded.and_then(|qi| {
        cfg.quadrants
            .get(qi)
            .map(|q| (qi, q, layout_panel(cfg, q.items.len(), geom, screen_w, screen_h, qi)))
    });

    // ---- 1. 面板图标格（绘制最上层）：与 draw_panel_items 共用 item_cell ----
    if let Some((qi, q, ref lay)) = panel {
        for (i, _) in q.items.iter().enumerate() {
            let (ix, iy, iw, ih) = lay.item_cell(cfg, i, scroll);
            if x >= ix && x <= ix + iw && y >= iy && y <= iy + ih {
                return Hit::Item { qi, index: i };
            }
        }
    }

    // ---- 2. 环带弧段（绘制在面板背板之上，重叠处以弧为准）----
    let (dx, dy) = (x - geom.cx, y - geom.cy);
    let dist = (dx * dx + dy * dy).sqrt();
    let r_out = geom.r_mid + geom.stroke / 2.0 + 1.0;
    let r_in = geom.r_mid - geom.stroke / 2.0 - 1.0;

    if dist <= r_out && dist > r_in {
        // 按「与弧段中心的角距离」归象限 —— 与绘制弧段一一对应；
        // 弧缝（|角距离| > half - GAP）是视觉空白，不触发任何象限逻辑
        let deg = dy.atan2(dx).to_degrees().rem_euclid(360.0);
        let step_half = 180.0 / geom.n as f32;
        for qi in 0..geom.n {
            let c = geom.center_angle(qi).rem_euclid(360.0);
            if ang_diff(deg, c).abs() <= step_half - ARC_GAP_DEG {
                return Hit::Quadrant(qi);
            }
        }
    }

    // ---- 3. 面板背板（图标间隙 / 超出弧带的面板区域）----
    if let Some((_, _, ref lay)) = panel {
        if x >= lay.x && x <= lay.x + lay.w && y >= lay.y && y <= lay.y + lay.h {
            return Hit::PanelBlank;
        }
    }

    // ---- 4. 中心圆 / 空白 ----
    if dist <= r_in {
        return Hit::Center;
    }
    Hit::None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用几何：屏幕中心 (500, 400)，r_mid=117 stroke=26 n=4
    fn geom() -> RingGeom {
        RingGeom { cx: 500.0, cy: 400.0, r_mid: 117.0, stroke: 26.0, n: 4 }
    }

    /// 弧段中线上的一点（距圆心 r，角度 deg）
    fn on_ring(g: &RingGeom, deg: f32, r: f32) -> (f32, f32) {
        let a = deg.to_radians();
        (g.cx + r * a.cos(), g.cy + r * a.sin())
    }

    #[test]
    fn arc_center_maps_to_own_quadrant() {
        let g = geom();
        let cfg = Config::default();
        // center_angle(i) = -45 + 90i：象限中心依次为 315°/45°/135°/225°
        for qi in 0..4usize {
            let deg = g.center_angle(qi);
            let (x, y) = on_ring(&g, deg, g.r_mid);
            assert_eq!(
                hit_test(&cfg, &g, 1000.0, 800.0, None, 0.0, x, y),
                Hit::Quadrant(qi),
                "弧 {qi} 中心应命中象限 {qi}"
            );
        }
    }

    #[test]
    fn regression_no_cross_trigger() {
        // 回归：弧 1 覆盖 [4°, 86°]，其中 [4°, 45°] 段旧公式会误判成象限 0
        let g = geom();
        let cfg = Config::default();
        let (x, y) = on_ring(&g, 10.0, g.r_mid);
        assert_eq!(hit_test(&cfg, &g, 1000.0, 800.0, None, 0.0, x, y), Hit::Quadrant(1));
        let (x, y) = on_ring(&g, 350.0, g.r_mid); // 弧 0 覆盖 [274°, 356°]
        assert_eq!(hit_test(&cfg, &g, 1000.0, 800.0, None, 0.0, x, y), Hit::Quadrant(0));
    }

    #[test]
    fn arc_gap_is_blank() {
        // 弧缝（0°/90°/180°/270° 附近 ±4°）是视觉空白：不触发任何象限
        let g = geom();
        let cfg = Config::default();
        for deg in [0.0f32, 90.0, 180.0, 270.0] {
            let (x, y) = on_ring(&g, deg, g.r_mid);
            assert_eq!(
                hit_test(&cfg, &g, 1000.0, 800.0, None, 0.0, x, y),
                Hit::None,
                "{deg}° 弧缝应为空白"
            );
        }
    }

    #[test]
    fn center_and_outside() {
        let g = geom();
        let cfg = Config::default();
        assert_eq!(hit_test(&cfg, &g, 1000.0, 800.0, None, 0.0, g.cx, g.cy), Hit::Center);
        assert_eq!(hit_test(&cfg, &g, 1000.0, 800.0, None, 0.0, g.cx + 50.0, g.cy), Hit::Center);
        // 环带之外
        assert_eq!(hit_test(&cfg, &g, 1000.0, 800.0, None, 0.0, g.cx + 200.0, g.cy), Hit::None);
    }

    #[test]
    fn panel_items_and_blank() {
        let g = geom();
        let cfg = Config::default();
        // 象限 0（右上）面板：含 6 个条目
        let items = cfg.quadrants[0].items.len();
        let lay = layout_panel(&cfg, items, &g, 1000.0, 800.0, 0);
        // 第 0 格中心（item_cell 与绘制同源）
        let (ix, iy, iw, ih) = lay.item_cell(&cfg, 0, 0.0);
        let (x0, y0) = (ix + iw / 2.0, iy + ih / 2.0);
        assert_eq!(
            hit_test(&cfg, &g, 1000.0, 800.0, Some(0), 0.0, x0, y0),
            Hit::Item { qi: 0, index: 0 }
        );
        // 面板右下角空白
        assert_eq!(
            hit_test(&cfg, &g, 1000.0, 800.0, Some(0), 0.0, lay.x + lay.w - 2.0, lay.y + lay.h - 2.0),
            Hit::PanelBlank
        );
        // 面板外的圆环外空白
        assert_eq!(hit_test(&cfg, &g, 1000.0, 800.0, Some(0), 0.0, 2.0, 2.0), Hit::None);
    }
}
