//! Pure geometry for middle-button dock movement. No screen capture or window relocation.
use crate::config::DockPosition;

#[derive(Clone, Copy)]
pub struct DockDrag {
    start_pointer: (f32, f32),
    pub start_center: (f32, f32),
}
impl DockDrag {
    pub fn new(start_pointer: (f32, f32), start_center: (f32, f32)) -> Self {
        Self {
            start_pointer,
            start_center,
        }
    }
    pub fn center_at(self, pointer: (f32, f32), w: f32, h: f32, radius: f32) -> (f32, f32) {
        clamp_center(
            (
                self.start_center.0 + pointer.0 - self.start_pointer.0,
                self.start_center.1 + pointer.1 - self.start_pointer.1,
            ),
            w,
            h,
            radius,
        )
    }
}
fn clamp_axis(v: f32, size: f32, margin: f32) -> f32 {
    let size = size.max(1.0);
    let margin = margin.min(size / 2.0);
    v.clamp(margin, size - margin)
}
pub fn clamp_center(center: (f32, f32), w: f32, h: f32, radius: f32) -> (f32, f32) {
    // Include the 7px soft shadow and leave a small desktop gutter.
    let margin = radius + 12.0;
    (
        clamp_axis(center.0, w, margin),
        clamp_axis(center.1, h, margin),
    )
}
pub fn restored_center(position: Option<DockPosition>, w: f32, h: f32, radius: f32) -> (f32, f32) {
    let position = position.unwrap_or(DockPosition { x: 0.5, y: 0.4 });
    clamp_center((position.x * w, position.y * h), w, h, radius)
}
pub fn normalized_center(center: (f32, f32), w: f32, h: f32) -> DockPosition {
    DockPosition {
        x: (center.0 / w.max(1.0)).clamp(0.0, 1.0),
        y: (center.1 / h.max(1.0)).clamp(0.0, 1.0),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_pointer_offset_without_jump() {
        let drag = DockDrag::new((550.0, 310.0), (500.0, 320.0));
        assert_eq!(
            drag.center_at((550.0, 310.0), 1000.0, 800.0, 158.0),
            (500.0, 320.0)
        );
        assert_eq!(
            drag.center_at((670.0, 360.0), 1000.0, 800.0, 158.0),
            (620.0, 370.0)
        );
    }
    #[test]
    fn clamps_all_edges_and_recovers_without_sticky_offsets() {
        let drag = DockDrag::new((500.0, 320.0), (500.0, 320.0));
        assert_eq!(
            drag.center_at((-1000.0, -1000.0), 1000.0, 800.0, 158.0),
            (170.0, 170.0)
        );
        assert_eq!(
            drag.center_at((9000.0, 9000.0), 1000.0, 800.0, 158.0),
            (830.0, 630.0)
        );
        assert_eq!(
            drag.center_at((500.0, 320.0), 1000.0, 800.0, 158.0),
            (500.0, 320.0)
        );
    }
    #[test]
    fn persists_relative_position_and_rescales() {
        let p = normalized_center((600.0, 400.0), 1000.0, 800.0);
        assert_eq!(
            restored_center(Some(p), 2000.0, 1600.0, 158.0),
            (1200.0, 800.0)
        );
        assert_eq!(restored_center(None, 1000.0, 800.0, 158.0), (500.0, 320.0));
    }
    #[test]
    fn tiny_workarea_does_not_panic() {
        assert_eq!(
            clamp_center((-999.0, 999.0), 120.0, 100.0, 158.0),
            (60.0, 50.0)
        );
    }
}
