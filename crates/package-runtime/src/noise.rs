//! Deterministic hashing and value noise for package generators. Pure
//! functions of their inputs, so a world regenerates identically anywhere.

fn mix(mut h: u64) -> u64 {
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    h ^ (h >> 33)
}
/// Uniform in `[0, 1)` for integer lattice coordinates.
pub fn hash3(seed: i64, x: i64, y: i64, z: i64) -> f64 {
    let h = mix((seed as u64)
        ^ mix(x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ mix((y as u64).wrapping_add(0x632B_E59B_D9B4_E019)).rotate_left(21)
        ^ mix((z as u64).wrapping_add(0x8515_7AF5)).rotate_left(42));
    (h >> 11) as f64 / (1u64 << 53) as f64
}
/// Smooth 2D value noise in `[-1, 1]`, one lattice cell per unit.
pub fn value2(seed: i64, x: f64, z: f64) -> f64 {
    if !x.is_finite() || !z.is_finite() {
        return 0.0;
    }
    let (x0, z0) = (x.floor(), z.floor());
    let (fx, fz) = (x - x0, z - z0);
    let (ix, iz) = (x0 as i64, z0 as i64);
    let s = |t: f64| t * t * (3.0 - 2.0 * t);
    // Wrapping: a lattice at the i64 edge (huge finite inputs) must not panic.
    let corner =
        |dx: i64, dz: i64| hash3(seed, ix.wrapping_add(dx), 0, iz.wrapping_add(dz)) * 2.0 - 1.0;
    let top = corner(0, 0) + (corner(1, 0) - corner(0, 0)) * s(fx);
    let bottom = corner(0, 1) + (corner(1, 1) - corner(0, 1)) * s(fx);
    top + (bottom - top) * s(fz)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn huge_finite_coordinates_do_not_panic() {
        for x in [1e19, -1e19, f64::MAX, f64::MIN, 9.2e18] {
            assert!(value2(1, x, x).abs() <= 1.0);
        }
    }
    #[test]
    fn noise_is_deterministic_and_bounded() {
        for i in 0..1000 {
            let (x, z) = (i as f64 * 0.37, i as f64 * -0.91);
            let v = value2(7, x, z);
            assert!((-1.0..=1.0).contains(&v));
            assert_eq!(v, value2(7, x, z));
        }
        assert_ne!(hash3(1, 2, 3, 4), hash3(2, 2, 3, 4));
    }
}
