//! Native periodic heightfield geometry, shared by rendering and physics.
use crate::Terrain;
use anyhow::{Result, ensure};
use glam::Vec3;
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
/// Region is [column,row,width,height] in cells. Negative cells wrap exactly.
pub fn mesh(terrain: &Terrain, spacing: f32, region: [i32; 4]) -> Result<Mesh> {
    terrain.validate()?;
    let [x, y, w, h] = region;
    ensure!(
        spacing.is_finite()
            && spacing > 0.0
            && w > 0
            && h > 0
            && w <= 1024
            && h <= 1024
            && x.unsigned_abs() < 1_000_000
            && y.unsigned_abs() < 1_000_000,
        "Invalid terrain region"
    );
    let mut mesh = Mesh {
        positions: vec![],
        normals: vec![],
        grid_uv: vec![],
        triangles: vec![],
    };
    for row in y..=y + h {
        for column in x..=x + w {
            mesh.positions.push([
                column as f32 * spacing,
                elevation(terrain, column, row),
                -(row as f32) * spacing,
            ]);
            let dx = (elevation(terrain, column + 1, row) - elevation(terrain, column - 1, row))
                / (2.0 * spacing);
            let dy = (elevation(terrain, column, row + 1) - elevation(terrain, column, row - 1))
                / (2.0 * spacing);
            mesh.normals
                .push(Vec3::new(-dx, 1.0, dy).normalize().to_array());
            mesh.grid_uv.push([
                column as f32 / terrain.side as f32,
                row as f32 / terrain.side as f32,
            ]);
        }
    }
    for row in 0..h {
        for column in 0..w {
            let a = (row * (w + 1) + column) as u32;
            let b = a + 1;
            let c = a + (w + 1) as u32;
            let d = c + 1;
            // Checkerboard diagonals are part of the native mesh definition.
            if ((column + x) ^ (row + y)) & 1 == 0 {
                mesh.triangles.extend([[a, b, d], [a, d, c]]);
            } else {
                mesh.triangles.extend([[a, b, c], [b, d, c]]);
            }
        }
    }
    Ok(mesh)
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
        let m = mesh(&t, 1.0, [0, 0, 2, 2]).unwrap();
        assert_eq!(m.positions[0][1], m.positions[8][1]);
        for triangle in m.triangles {
            let [a, b, c] = triangle.map(|i| Vec3::from(m.positions[i as usize]));
            assert!((b - a).cross(c - a).y > 0.0);
        }
    }
}
