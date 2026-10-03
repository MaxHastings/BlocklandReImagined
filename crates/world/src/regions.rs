//! Detection-region geometry shared by observation and creator presentation.
use crate::Brick;
use glam::Vec3;

pub const MAX_OBSERVED_REGIONS: usize = 256;
pub const REGION_INPUTS: [&str; 6] = [
    "onRegionEnter",
    "onRegionLeave",
    "onRegionStay",
    "onObjectEnter",
    "onObjectLeave",
    "onObjectStay",
];

/// The observer indexes supported rows even when disabled, so other rules can
/// still query occupancy. Preserved unsupported rows do not create a region.
pub fn has_region_input(brick: &Brick) -> bool {
    brick.events.iter().any(|row| {
        row.preserved.is_none()
            && REGION_INPUTS
                .iter()
                .any(|input| row.input.eq_ignore_ascii_case(input))
    })
}

/// World-axis-aligned dimensions, centered on the logical brick box. An
/// explicit size does not rotate with the brick; default footprint does.
pub fn bounds(size: Option<[f32; 3]>, brick_box: (Vec3, Vec3)) -> (Vec3, Vec3) {
    let (min, max) = brick_box;
    let center = (min + max) * 0.5;
    let size = size.map(Vec3::from).unwrap_or(Vec3::new(
        (max.x - min.x).max(1.0),
        4.0,
        (max.z - min.z).max(1.0),
    ));
    (center - size * 0.5, center + size * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn centered_explicit_size_and_default_footprint() {
        let center = Vec3::new(12.0, 6.0, -3.0);
        let half = Vec3::new(0.25, 0.1, 2.0);
        let box_ = (center - half, center + half);
        let (lo, hi) = bounds(Some([8.0, 5.0, 8.0]), box_);
        assert_eq!(
            (lo, hi),
            (
                center - Vec3::new(4.0, 2.5, 4.0),
                center + Vec3::new(4.0, 2.5, 4.0)
            )
        );
        let (lo, hi) = bounds(None, box_);
        assert_eq!(hi - lo, Vec3::new(1.0, 4.0, 4.0));
    }
}
