use anyhow::{Context, Result, bail, ensure};
use bri_content::brick::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Serialize)]
pub struct Provenance {
    pub geometry: String,
    pub original_text: String,
    pub warnings: Vec<String>,
}

struct Lines<'a> {
    lines: Vec<(usize, &'a str)>,
    cursor: usize,
}
impl<'a> Lines<'a> {
    fn new(text: &'a str) -> Self {
        let lines = text
            .lines()
            .enumerate()
            .filter_map(|(i, s)| {
                let s = s.split("//").next().unwrap().trim();
                if s.is_empty()
                    || (s.starts_with("---") && s.to_ascii_lowercase().contains("quads"))
                {
                    None
                } else {
                    Some((i + 1, s))
                }
            })
            .collect();
        Self { lines, cursor: 0 }
    }
    fn peek(&self) -> Option<&'a str> {
        self.lines.get(self.cursor).map(|v| v.1)
    }
    fn next(&mut self) -> Result<&'a str> {
        let (_, line) = self
            .lines
            .get(self.cursor)
            .context("Unexpected end of BLB")?;
        self.cursor += 1;
        Ok(line)
    }
    fn expect(&mut self, expected: &str) -> Result<()> {
        let line = self.next()?;
        ensure!(
            line.eq_ignore_ascii_case(expected),
            "Expected {expected}, got {line}"
        );
        Ok(())
    }
    fn number(&mut self, max: usize) -> Result<usize> {
        let line_no = self.lines.get(self.cursor).map(|v| v.0).unwrap_or(0);
        let text = self.next()?;
        let n: usize = text
            .parse()
            .with_context(|| format!("Line {line_no}: expected integer count, got {text}"))?;
        ensure!(n <= max, "Count {n} exceeds limit {max}");
        Ok(n)
    }
    fn vector<const N: usize>(&mut self) -> Result<[f32; N]> {
        let line = self.next()?;
        let values = line
            .split_whitespace()
            .map(str::parse::<f32>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ensure!(
            values.len() == N && values.iter().all(|v| v.is_finite()),
            "Invalid vector: {line}"
        );
        Ok(values.try_into().unwrap())
    }
}

pub fn position([x, y, z]: [f32; 3]) -> [f32; 3] {
    [x * STUD, z * PLATE, -y * STUD]
}
pub fn normal([x, y, z]: [f32; 3]) -> Result<[f32; 3]> {
    // BLB positions use stud/plate coordinates, but authored normals already
    // describe world proportions. Applying inverse scale again distorts slopes.
    let n = [x, z, -y];
    let length = n.iter().map(|v| v * v).sum::<f32>().sqrt();
    ensure!(
        length > 1e-10 && length.is_finite(),
        "Invalid zero/overflowing normal"
    );
    Ok(n.map(|v| v / length))
}

pub fn read(data: &[u8], id: String) -> Result<(Brick, Provenance)> {
    let text = std::str::from_utf8(data)?.trim_start_matches('\u{feff}');
    let (adapted, mut warnings) = adapt_known_source(data, text)?;
    let mut lines = Lines::new(&adapted);
    let dimensions = lines
        .vector::<3>()
        .context("Missing BLB dimensions (possibly a non-standalone fragment)")?;
    ensure!(
        dimensions
            .iter()
            .all(|v| *v >= 1.0 && *v <= 4096.0 && v.fract() == 0.0),
        "Invalid BLB dimensions"
    );
    let [width, depth, height] = dimensions.map(|n| n as u32);
    ensure!(
        u64::from(width) * u64::from(depth) * u64::from(height) <= 2_000_000,
        "Brick grid too large"
    );
    let geometry = lines.next()?.to_owned();
    let mut brick = Brick {
        schema_version: BRICK_SCHEMA,
        id,
        footprint_studs: [width, depth],
        height_plates: height,
        attachment_rows: vec![],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    match geometry.as_str() {
        "BRICK" => {
            standard(&mut brick);
            ensure!(lines.peek().is_none(), "Unexpected data after BRICK");
        }
        "SPECIAL" | "SPECIALBRICK" => {
            if geometry == "SPECIAL" {
                for _ in 0..depth * height {
                    brick
                        .attachment_rows
                        .push(lines.next()?.to_ascii_lowercase());
                }
            } else {
                brick.attachment_rows = solid_grid(width, depth, height);
                warnings.push("Legacy SPECIALBRICK has no grid; solid attachment grid synthesized, behavior requires review".into());
            }
            let count = lines.number(1024)?;
            for _ in 0..count {
                let center = position(lines.vector()?);
                let [x, y, z] = lines.vector::<3>()?;
                brick.collision_boxes.push(CollisionBox {
                    center,
                    size: [x * STUD, z * PLATE, y * STUD],
                });
            }
            brick.needs_external_collision = count == 0;
            if count == 0 {
                warnings.push("No collision boxes: resolve datablock collisionShapeName; do not substitute the visual mesh".into());
            }
            if lines
                .peek()
                .is_some_and(|l| l.eq_ignore_ascii_case("COVERAGE:"))
            {
                lines.next()?;
                let mut coverage = [Coverage {
                    hides_adjacent: false,
                    required_area: 0.0,
                }; 6];
                for c in &mut coverage {
                    let value = lines.next()?;
                    let (flag, area) = value.split_once(':').context("Invalid coverage record")?;
                    let flag: u8 = flag.trim().parse()?;
                    ensure!(flag <= 1, "Invalid coverage flag");
                    c.hides_adjacent = flag != 0;
                    c.required_area = area.trim().parse()?;
                    ensure!(
                        c.required_area.is_finite() && c.required_area >= 0.0,
                        "Invalid coverage area"
                    );
                }
                brick.coverage = Some(coverage);
            }
            for face in FACES {
                let count = lines.number(100_000)?;
                ensure!(
                    brick.quads.len() + count <= 100_000,
                    "Total quad limit exceeded"
                );
                for _ in 0..count {
                    let surface = match lines.next()?.to_ascii_uppercase().as_str() {
                        "TEX:TOP" => Surface::Top,
                        "TEX:SIDE" => Surface::Side,
                        "TEX:BOTTOMEDGE" => Surface::BottomEdge,
                        "TEX:BOTTOMLOOP" => Surface::BottomLoop,
                        "TEX:RAMP" => Surface::Ramp,
                        "TEX:PRINT" => Surface::Print,
                        other => bail!("Unknown surface {other}"),
                    };
                    lines.expect("POSITION:")?;
                    let mut positions = [[0.0; 3]; 4];
                    for p in &mut positions {
                        *p = position(lines.vector()?);
                    }
                    lines.expect("UV COORDS:")?;
                    let mut uvs = [[0.0; 2]; 4];
                    for uv in &mut uvs {
                        *uv = lines.vector()?;
                    }
                    let colors = if lines
                        .peek()
                        .is_some_and(|s| s.eq_ignore_ascii_case("COLORS:"))
                    {
                        lines.next()?;
                        let mut colors = [[0.0; 4]; 4];
                        for c in &mut colors {
                            *c = lines.vector()?;
                        }
                        Some([colors[0], colors[3], colors[2], colors[1]])
                    } else {
                        None
                    };
                    lines.expect("NORMALS:")?;
                    let mut normals = [[0.0; 3]; 4];
                    for n in &mut normals {
                        *n = normal(lines.vector()?)?;
                    }
                    let vertices = [0, 3, 2, 1].map(|i| Vertex {
                        position: positions[i],
                        normal: normals[i],
                        uv: uvs[i],
                    });
                    brick.quads.push(Quad {
                        face,
                        surface,
                        vertices,
                        colors,
                    });
                }
            }
            ensure!(
                lines.peek().is_none(),
                "Unparsed BLB data: {:?}",
                lines.peek()
            );
        }
        _ => bail!("Unknown BLB geometry {geometry}"),
    }
    brick.validate()?;
    Ok((
        brick,
        Provenance {
            geometry,
            original_text: text.into(),
            warnings,
        },
    ))
}

#[derive(Deserialize)]
struct Repair {
    path: String,
    sha256: String,
    edits: Vec<Edit>,
}
#[derive(Deserialize)]
struct Edit {
    line: usize,
    before: String,
    after: String,
    reason: String,
}

fn adapt_known_source(data: &[u8], text: &str) -> Result<(String, Vec<String>)> {
    let digest = format!("{:x}", Sha256::digest(data));
    let repairs: Vec<Repair> = serde_json::from_str(include_str!("../data/blb-repairs.json"))?;
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let mut warnings = Vec::new();
    for repair in repairs.into_iter().filter(|r| r.sha256 == digest) {
        for edit in repair.edits {
            let line = lines
                .get_mut(edit.line.checked_sub(1).context("Invalid repair line")?)
                .context("Repair line exceeds input")?;
            ensure!(
                *line == edit.before,
                "Known-source repair precondition failed"
            );
            *line = edit.after;
            warnings.push(format!(
                "Source adaptation {}:{}: {}",
                repair.path, edit.line, edit.reason
            ));
        }
    }
    Ok((lines.join("\n"), warnings))
}

fn solid_grid(width: u32, depth: u32, height: u32) -> Vec<String> {
    (0..depth)
        .flat_map(|_| {
            (0..height).map(move |y| {
                let cell = if height == 1 {
                    "b"
                } else if y == 0 {
                    "u"
                } else if y == height - 1 {
                    "d"
                } else {
                    "x"
                };
                cell.repeat(width as usize)
            })
        })
        .collect()
}

fn standard(brick: &mut Brick) {
    let [w, d] = brick.footprint_studs.map(|n| n as f32);
    let h = brick.height_plates as f32;
    let [x, y, z] = [w * STUD * 0.5, h * PLATE * 0.5, d * STUD * 0.5];
    brick.attachment_rows = solid_grid(w as u32, d as u32, h as u32);
    brick.collision_boxes.push(CollisionBox {
        center: [0.0; 3],
        size: [x * 2.0, y * 2.0, z * 2.0],
    });
    brick.coverage = Some(
        [w * d, w * d, w * h, d * h, w * h, d * h].map(|area| Coverage {
            hides_adjacent: true,
            required_area: area,
        }),
    );
    let mut add = |face, surface, positions: [[f32; 3]; 4], n, uv: [[f32; 2]; 4]| {
        brick.quads.push(Quad {
            face,
            surface,
            vertices: std::array::from_fn(|i| Vertex {
                position: positions[i],
                normal: n,
                uv: uv[i],
            }),
            colors: None,
        });
    };
    let uv = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    add(
        Face::North,
        Surface::Side,
        [[-x, -y, -z], [-x, y, -z], [x, y, -z], [x, -y, -z]],
        [0.0, 0.0, -1.0],
        [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
    );
    add(
        Face::South,
        Surface::Side,
        [[-x, -y, z], [x, -y, z], [x, y, z], [-x, y, z]],
        [0.0, 0.0, 1.0],
        uv,
    );
    add(
        Face::West,
        Surface::Side,
        [[-x, -y, -z], [-x, -y, z], [-x, y, z], [-x, y, -z]],
        [-1.0, 0.0, 0.0],
        uv,
    );
    add(
        Face::East,
        Surface::Side,
        [[x, -y, z], [x, -y, -z], [x, y, -z], [x, y, z]],
        [1.0, 0.0, 0.0],
        uv,
    );
    add(
        Face::Top,
        Surface::Top,
        [[-x, y, -z], [-x, y, z], [x, y, z], [x, y, -z]],
        [0.0, 1.0, 0.0],
        [[0.0, 0.0], [0.0, d], [w, d], [w, 0.0]],
    );
    let ix = x - STUD * 0.5;
    let iz = z - STUD * 0.5;
    let n = [0.0, -1.0, 0.0];
    if ix > 0.0 && iz > 0.0 {
        add(
            Face::Bottom,
            Surface::BottomLoop,
            [[-ix, -y, -iz], [ix, -y, -iz], [ix, -y, iz], [-ix, -y, iz]],
            n,
            [
                [d - 1.0, 0.0],
                [d - 1.0, w - 1.0],
                [0.0, w - 1.0],
                [0.0, 0.0],
            ],
        );
    }
    add(
        Face::Bottom,
        Surface::BottomEdge,
        [[-x, -y, -z], [x, -y, -z], [ix, -y, -iz], [-ix, -y, -iz]],
        n,
        [[-0.5, 0.0], [w - 0.5, 0.0], [w - 1.0, 0.5], [0.0, 0.5]],
    );
    add(
        Face::Bottom,
        Surface::BottomEdge,
        [[-ix, -y, iz], [ix, -y, iz], [x, -y, z], [-x, -y, z]],
        n,
        [[0.0, 0.5], [w - 1.0, 0.5], [w - 0.5, 0.0], [-0.5, 0.0]],
    );
    add(
        Face::Bottom,
        Surface::BottomEdge,
        [[-x, -y, -z], [-ix, -y, -iz], [-ix, -y, iz], [-x, -y, z]],
        n,
        [[d - 0.5, 0.0], [d - 1.0, 0.5], [0.0, 0.5], [-0.5, 0.0]],
    );
    add(
        Face::Bottom,
        Surface::BottomEdge,
        [[ix, -y, -iz], [x, -y, -z], [x, -y, z], [ix, -y, iz]],
        n,
        [[d - 1.0, 0.5], [d - 0.5, 0.0], [-0.5, 0.0], [0.0, 0.5]],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_keeps_grid_collision_and_bottom_regions() {
        let (b, _) = read(b"2 4 3\nBRICK", "test".into()).unwrap();
        assert_eq!(b.collision_boxes[0].size, [1.0, 0.6, 2.0]);
        assert_eq!(b.attachment_rows.len(), 12);
        assert_eq!(b.attachment_rows[..3], ["uu", "xx", "dd"]);
        assert_eq!(
            b.quads
                .iter()
                .filter(|q| q.surface == Surface::BottomEdge)
                .count(),
            4
        );
        let (plate, _) = read(b"1 1 1\nBRICK", "plate".into()).unwrap();
        assert_eq!(plate.attachment_rows, ["b"]);
        assert!(!plate.quads.iter().any(|q| q.surface == Surface::BottomLoop));
    }
    #[test]
    fn authored_world_normal_rotates_without_rescaling() {
        let n = normal([1.0, 0.0, 1.0]).unwrap();
        assert!((n[1] / n[0] - 1.0).abs() < 0.00001);
        assert_eq!(position([2.0, 4.0, 3.0]), [1.0, 0.6, -2.0]);
        let slope = normal([0.0, 0.985127, 0.171828]).unwrap();
        // Original tall-ramp face rises 14.333 plates over one stud.
        let tangent = [0.0, -14.333 * PLATE, -STUD];
        let dot: f32 = slope.iter().zip(tangent).map(|(a, b)| a * b).sum();
        assert!(
            dot.abs() < 0.00001,
            "Normal no longer perpendicular to converted slope"
        );
    }
    #[test]
    fn rejects_bad_or_unbounded_input() {
        for data in [
            "0 1 1\nBRICK",
            "4096 4096 4096\nBRICK",
            "1 1 1\nINVALID",
            "1 1 1\nBRICK\nextra",
            "nan 2 3\nBRICK",
        ] {
            assert!(read(data.as_bytes(), "test".into()).is_err());
        }
    }
    #[test]
    fn special_keeps_empty_grid_print_coverage_and_authored_alpha() {
        let data = "3 1 1\nSPECIAL\n---\n1\n0 0 0\n3 1 1\nCOVERAGE:\n0 : 3\n1 : 3\n0 : 99\n0 : 99\n0 : 99\n0 : 99\n1\nTEX:PRINT\nPOSITION:\n-1.5 -0.5 0.5\n-1.5 0.5 0.5\n1.5 0.5 0.5\n1.5 -0.5 0.5\nUV COORDS:\n0 0\n0 1\n1 1\n1 0\nCOLORS:\n1 0 0 0.3\n0 1 0 0.4\n0 0 1 0.5\n1 1 1 0.6\nNORMALS:\n0 0 1\n0 0 1\n0 0 1\n0 0 1\n0\n0\n0\n0\n0\n0";
        let (brick, _) = read(data.as_bytes(), "special".into()).unwrap();
        assert_eq!(brick.attachment_rows, ["---"]);
        assert_eq!(brick.quads[0].surface, Surface::Print);
        assert_eq!(brick.quads[0].colors.unwrap()[1], [1.0, 1.0, 1.0, 0.6]);
        assert!(brick.coverage.unwrap()[1].hides_adjacent);
        assert!(!brick.needs_external_collision);
        assert_eq!(brick.quads[0].vertices[1].position, [0.75, 0.1, 0.25]);
    }
    #[test]
    fn generated_faces_have_consistent_outward_winding() {
        let (brick, _) = read(b"2 4 3\nBRICK", "box".into()).unwrap();
        for quad in brick.quads {
            for triangle in [[0, 1, 2], [0, 2, 3]] {
                let p = triangle.map(|i| quad.vertices[i].position);
                let a: [f32; 3] = std::array::from_fn(|i| p[1][i] - p[0][i]);
                let b: [f32; 3] = std::array::from_fn(|i| p[2][i] - p[0][i]);
                let cross = [
                    a[1] * b[2] - a[2] * b[1],
                    a[2] * b[0] - a[0] * b[2],
                    a[0] * b[1] - a[1] * b[0],
                ];
                let dot: f32 = cross
                    .iter()
                    .zip(quad.vertices[0].normal)
                    .map(|(a, b)| a * b)
                    .sum();
                assert!(dot > 0.0, "Backwards generated face {:?}", quad.face);
            }
        }
    }
}
