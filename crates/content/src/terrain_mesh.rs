//! Native periodic heightfield geometry, shared by rendering and physics.
use crate::Terrain;
/// Terrain triangles in world space; built by `TerrainField::mesh`.
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub grid_uv: Vec<[f32; 2]>,
    pub triangles: Vec<[u32; 3]>,
}
fn elevation(terrain: &Terrain, x: i32, y: i32) -> f32 {
    let side = terrain.side as i32;
    terrain.elevations[(y.rem_euclid(side) * side + x.rem_euclid(side)) as usize]
}
/// Triangle interpolation, deliberately not bilinear height interpolation.
/// Requires validated terrain, positive finite spacing and finite bounded coordinates.
pub fn height(terrain: &Terrain, spacing: f32, x: f32, z: f32) -> f32 {
    let gx = x / spacing;
    let gy = -z / spacing;
    let ix = gx.floor() as i32;
    let iy = gy.floor() as i32;
    let u = gx - ix as f32;
    let v = gy - iy as f32;
    let a = elevation(terrain, ix, iy);
    let b = elevation(terrain, ix + 1, iy);
    let c = elevation(terrain, ix, iy + 1);
    let d = elevation(terrain, ix + 1, iy + 1);
    if (ix ^ iy) & 1 == 0 {
        if u > v {
            a + u * (b - a) + v * (d - b)
        } else {
            a + u * (d - c) + v * (c - a)
        }
    } else if 1.0 - u > v {
        b + (1.0 - u) * (a - b) + v * (c - a)
    } else {
        b + (1.0 - u) * (c - d) + v * (d - b)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::TerrainLayer;
    #[test]
    fn periodic_border_and_triangle_heights() {
        let t = Terrain {
            schema_version: 1,
            id: "fixture".into(),
            side: 2,
            elevations: vec![0.0, 0.0, 0.0, 4.0],
            primary_layers: vec![0; 4],
            layers: vec![TerrainLayer {
                slot: 0,
                material: "test".into(),
                weights: vec![255; 4],
            }],
        };
        assert_eq!(height(&t, 1.0, 0.5, -0.5), 2.0); // Bilinear would wrongly return 1.
        assert_eq!(height(&t, 1.0, -1.5, 1.5), 2.0);
    }
}
