//! Hit testing shares the renderer's layout and viewport, never GDI window regions.
use crate::{
    config::Config,
    render::{layout_panel, RingGeom, ARC_GAP_DEG},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Quadrant(usize),
    Center,
    Item { qi: usize, index: usize },
    ItemDelete { qi: usize, index: usize },
    PanelClose,
    PanelEdit,
    PanelBlank,
    None,
}
fn ang_diff(a: f32, b: f32) -> f32 {
    (a - b + 180.0).rem_euclid(360.0) - 180.0
}
#[allow(clippy::too_many_arguments)] // Shared layout/geometry inputs, kept explicit.
pub fn hit_test(
    cfg: &Config,
    g: &RingGeom,
    sw: f32,
    sh: f32,
    expanded: Option<usize>,
    scroll: f32,
    edit: bool,
    x: f32,
    y: f32,
) -> Hit {
    if let Some((qi, q)) = expanded.and_then(|i| cfg.quadrants.get(i).map(|q| (i, q))) {
        let l = layout_panel(cfg, q.items.len(), g, sw, sh, qi);
        if l.contains(x, y) {
            if y >= l.y + 13.0 && y <= l.y + 42.0 {
                if x >= l.x + l.w - 39.0 && x <= l.x + l.w - 12.0 {
                    return Hit::PanelClose;
                }
                if x >= l.x + l.w - 98.0 && x <= l.x + l.w - 44.0 {
                    return Hit::PanelEdit;
                }
            }
            let (vx, vy, vw, vh) = l.viewport();
            if x >= vx && x <= vx + vw && y >= vy && y <= vy + vh {
                if edit {
                    for i in 0..q.items.len() {
                        let (ix, iy, iw, _) = l.item_cell(cfg, i, scroll);
                        if (x - (ix + iw - 6.0)).hypot(y - (iy + 7.0)) <= 9.0 {
                            return Hit::ItemDelete { qi, index: i };
                        }
                    }
                }
                for i in 0..q.items.len() {
                    let (ix, iy, iw, ih) = l.item_cell(cfg, i, scroll);
                    if x >= ix && x <= ix + iw && y >= iy && y <= iy + ih - 4.0 {
                        return Hit::Item { qi, index: i };
                    }
                }
            }
            return Hit::PanelBlank;
        }
    }
    let (dx, dy) = (x - g.cx, y - g.cy);
    let d = dx.hypot(dy);
    if d <= g.r_disc() + 1.0 && d >= g.r_mid - g.stroke / 2.0 - 1.0 {
        let deg = dy.atan2(dx).to_degrees();
        for q in 0..g.n {
            if ang_diff(deg, g.center_angle(q)).abs() <= 180.0 / g.n.max(1) as f32 - ARC_GAP_DEG {
                return Hit::Quadrant(q);
            }
        }
    }
    if d <= g.r_chip() {
        Hit::Center
    } else {
        Hit::None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn g() -> RingGeom {
        RingGeom {
            cx: 500.0,
            cy: 320.0,
            r_mid: 136.0,
            stroke: 44.0,
            n: 4,
        }
    }
    fn ring(g: &RingGeom, deg: f32) -> (f32, f32) {
        (
            g.cx + g.r_mid * deg.to_radians().cos(),
            g.cy + g.r_mid * deg.to_radians().sin(),
        )
    }
    #[test]
    fn every_supported_sector_count_matches_render_centers() {
        for n in 2..=8 {
            let mut c = Config {
                quadrant_count: n,
                ..Config::default()
            };
            c.quadrants.resize(n, c.quadrants[0].clone());
            let mut g = g();
            g.n = n;
            for q in 0..n {
                let a = g.center_angle(q).to_radians();
                assert_eq!(
                    hit_test(
                        &c,
                        &g,
                        1000.0,
                        800.0,
                        None,
                        0.0,
                        false,
                        g.cx + g.r_mid * a.cos(),
                        g.cy + g.r_mid * a.sin()
                    ),
                    Hit::Quadrant(q)
                );
            }
        }
    }

    #[test]
    fn quadrant_centers() {
        let g = g();
        let c = Config::default();
        for i in 0..4 {
            let (x, y) = ring(&g, g.center_angle(i));
            assert_eq!(
                hit_test(&c, &g, 1000.0, 800.0, None, 0.0, false, x, y),
                Hit::Quadrant(i)
            );
        }
    }
    #[test]
    fn no_cross_trigger() {
        let g = g();
        let c = Config::default();
        for (a, q) in [(10.0, 1), (350.0, 0)] {
            let (x, y) = ring(&g, a);
            assert_eq!(
                hit_test(&c, &g, 1000.0, 800.0, None, 0.0, false, x, y),
                Hit::Quadrant(q)
            );
        }
    }
    #[test]
    fn gaps_pass_through() {
        let g = g();
        let c = Config::default();
        for a in [0.0, 90.0, 180.0, 270.0] {
            let (x, y) = ring(&g, a);
            assert_eq!(
                hit_test(&c, &g, 1000.0, 800.0, None, 0.0, false, x, y),
                Hit::None
            );
        }
    }
    #[test]
    fn panel_header_and_footer_not_items() {
        let g = g();
        let c = Config::default();
        let l = layout_panel(&c, 6, &g, 1000.0, 800.0, 0);
        assert_eq!(
            hit_test(
                &c,
                &g,
                1000.0,
                800.0,
                Some(0),
                0.0,
                false,
                l.x + 30.0,
                l.y + 20.0
            ),
            Hit::PanelBlank
        );
        assert_eq!(
            hit_test(
                &c,
                &g,
                1000.0,
                800.0,
                Some(0),
                0.0,
                false,
                l.x + l.w - 25.0,
                l.y + 25.0
            ),
            Hit::PanelClose
        );
    }
    #[test]
    fn clipped_item_not_clickable() {
        let g = g();
        let c = Config::default();
        let l = layout_panel(&c, 6, &g, 1000.0, 800.0, 0);
        let (x, y, w, _) = l.item_cell(&c, 0, 100.0);
        assert!(!matches!(
            hit_test(
                &c,
                &g,
                1000.0,
                800.0,
                Some(0),
                100.0,
                false,
                x + w / 2.0,
                y + 10.0
            ),
            Hit::Item { .. }
        ));
    }
    #[test]
    fn edit_delete_matches_icon() {
        let g = g();
        let c = Config::default();
        let l = layout_panel(&c, 6, &g, 1000.0, 800.0, 0);
        let (x, y, w, _) = l.item_cell(&c, 0, 0.0);
        assert_eq!(
            hit_test(
                &c,
                &g,
                1000.0,
                800.0,
                Some(0),
                0.0,
                true,
                x + w - 6.0,
                y + 7.0
            ),
            Hit::ItemDelete { qi: 0, index: 0 }
        );
    }
    #[test]
    fn transparent_center_gap() {
        let g = g();
        let c = Config::default();
        assert_eq!(
            hit_test(
                &c,
                &g,
                1000.0,
                800.0,
                None,
                0.0,
                false,
                g.cx + g.r_chip() + 10.0,
                g.cy
            ),
            Hit::None
        );
        assert_eq!(
            hit_test(&c, &g, 1000.0, 800.0, None, 0.0, false, g.cx, g.cy),
            Hit::Center
        );
    }
}
