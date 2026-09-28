//! Literal mission object reader; no evaluation, script execution or regex nesting.
use crate::catalog::{Token, lex};
use anyhow::{Context, Result, ensure};
use bri_content::scene::{Kind, Node, PendingScript, Scene};
use glam::{Mat4, Quat, Vec3};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

fn numbers<const N: usize>(value: Option<&String>, default: [f32; N]) -> Result<[f32; N]> {
    let Some(s) = value else {
        return Ok(default);
    };
    let v = s
        .split_whitespace()
        .map(str::parse::<f32>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(
        v.len() == N && v.iter().all(|x| x.is_finite()),
        "Invalid mission transform"
    );
    Ok(v.try_into().unwrap())
}
fn transform(p: &BTreeMap<String, String>) -> Result<[f32; 16]> {
    let t = numbers(p.get("position"), [0.0; 3])?;
    let s = numbers(p.get("scale"), [1.0; 3])?;
    let a = numbers(p.get("rotation"), [1.0, 0.0, 0.0, 0.0])?;
    let axis = Vec3::new(a[0], a[2], -a[1]);
    ensure!(
        axis.length() > 0.00001 || a[3] == 0.0,
        "Zero mission rotation axis"
    );
    let rotation = if a[3] == 0.0 {
        Quat::IDENTITY
    } else {
        // `Quat::from_axis_angle` with the portable `libm` sine: the platform
        // C runtime's can differ in the last bit between machines.
        let half = -a[3].to_radians() * 0.5;
        let v = axis.normalize() * libm::sinf(half);
        Quat::from_xyzw(v.x, v.y, v.z, libm::cosf(half))
    };
    Ok(Mat4::from_scale_rotation_translation(
        Vec3::new(s[0], s[2], s[1]),
        rotation,
        Vec3::new(t[0], t[2], -t[1]),
    )
    .to_cols_array())
}
pub(crate) fn reference(source: &str, path: &str) -> Result<String> {
    let path = path.replace('\\', "/");
    let resolved = if let Some(local) = path.strip_prefix("./") {
        format!(
            "{}/{}",
            source
                .rsplit_once('/')
                .context("Mission path lacks parent")?
                .0,
            local
        )
    } else if let Some(local) = path.strip_prefix("~/") {
        // Torque's tilde is the script's mod root, e.g. Add-Ons for a map.
        format!(
            "{}/{}",
            source
                .split_once('/')
                .context("Mission path lacks mod root")?
                .0,
            local
        )
    } else {
        path
    };
    ensure!(
        !resolved.starts_with('/')
            && !resolved.contains(':')
            && resolved
                .split('/')
                .all(|s| !s.is_empty() && s != ".." && s != "."),
        "Unsafe mission asset reference"
    );
    Ok(format!("v20/{}", resolved.to_lowercase()))
}
struct Parser {
    tokens: Vec<Token>,
    at: usize,
    nodes: Vec<Node>,
}
impl Parser {
    fn take(&mut self) -> Result<Token> {
        let t = self
            .tokens
            .get(self.at)
            .context("Truncated mission")?
            .clone();
        self.at += 1;
        Ok(t)
    }
    fn expect(&mut self, c: char) -> Result<()> {
        ensure!(
            self.take()? == Token::Symbol(c),
            "Expected mission punctuation {c}"
        );
        Ok(())
    }
    fn object(&mut self, parent: Option<usize>, source: &str, depth: usize) -> Result<()> {
        ensure!(
            depth <= 64 && self.nodes.len() < 100_000,
            "Mission hierarchy too large"
        );
        ensure!(
            self.take()?.atom("new"),
            "Expected literal object declaration"
        );
        let class = self.take()?.literal()?.to_lowercase();
        self.expect('(')?;
        let name = if self.tokens.get(self.at) == Some(&Token::Symbol(')')) {
            String::new()
        } else {
            self.take()?.literal()?
        };
        self.expect(')')?;
        self.expect('{')?;
        let index = self.nodes.len();
        self.nodes.push(Node {
            name,
            parent,
            kind: Kind::Unadapted,
            transform: Mat4::IDENTITY.to_cols_array(),
            asset: None,
            properties: BTreeMap::new(),
        });
        let mut properties = BTreeMap::new();
        while self.tokens.get(self.at) != Some(&Token::Symbol('}')) {
            if self.tokens.get(self.at).is_some_and(|t| t.atom("new")) {
                self.object(Some(index), source, depth + 1)?;
                continue;
            }
            let mut field = self.take()?.literal()?.to_lowercase();
            if self.tokens.get(self.at) == Some(&Token::Symbol('[')) {
                self.at += 1;
                field.push('[');
                field.push_str(&self.take()?.literal()?);
                field.push(']');
                self.expect(']')?;
            }
            self.expect('=')?;
            let value = self.take()?.literal()?;
            self.expect(';')?;
            properties.insert(field, value);
        }
        self.expect('}')?;
        self.expect(';')?;
        let kind = match class.as_str() {
            "simgroup" => Kind::Group,
            "scriptobject" => Kind::Metadata,
            "interiorinstance" => Kind::Interior,
            "terrainblock" => Kind::Terrain,
            "tsstatic" => Kind::StaticModel,
            "staticshape" => Kind::DatablockModel,
            "spawnsphere" => Kind::Spawn,
            "sky" => Kind::Sky,
            "sun" => Kind::Sun,
            "waterblock" => Kind::Water,
            "precipitation" => Kind::Precipitation,
            "fxgrassreplicator" | "fxfoliagereplicator" => Kind::Foliage,
            "missionarea" => Kind::Bounds,
            _ => Kind::Unadapted,
        };
        let asset = match kind {
            Kind::Interior => Some("interiorfile"),
            Kind::Terrain => Some("terrainfile"),
            Kind::StaticModel => Some("shapename"),
            _ => None,
        }
        .map(|key| {
            reference(
                source,
                properties
                    .get(key)
                    .with_context(|| format!("Missing {key}"))?,
            )
        })
        .transpose()?;
        let matrix = if matches!(kind, Kind::Terrain) {
            let spacing = numbers(properties.get("squaresize"), [8.0])?[0];
            ensure!(spacing > 0.0, "Invalid terrain square size");
            // Legacy TerrainBlock centers its 256-cell period on add; the old
            // 'position' field is removed from its native field table.
            properties.insert(
                "native_origin_policy".into(),
                "centered_256_cell_period".into(),
            );
            Mat4::from_translation(Vec3::new(-spacing * 128.0, 0.0, spacing * 128.0))
                .to_cols_array()
        } else {
            transform(&properties)?
        };
        properties.insert("source_class".into(), class);
        let node = &mut self.nodes[index];
        node.kind = kind;
        node.asset = asset;
        node.transform = matrix;
        node.properties = properties;
        Ok(())
    }
}
pub fn read(text: &str, source: &str) -> Result<Scene> {
    let (objects, pending_scripts) = exported_objects(text)?;
    let mut parser = Parser {
        tokens: lex(objects)?,
        at: 0,
        nodes: vec![],
    };
    while parser.at < parser.tokens.len() {
        parser.object(None, source, 0)?;
    }
    let name = parser
        .nodes
        .iter()
        .find(|n| n.name.eq_ignore_ascii_case("MissionInfo"))
        .and_then(|n| n.properties.get("name"))
        .cloned()
        .unwrap_or_else(|| source.into());
    Ok(Scene {
        schema_version: 1,
        id: format!("v20/{}", source.to_lowercase()),
        name,
        nodes: parser.nodes,
        pending_scripts,
    })
}

/// Torque's editor delimits literal object exports. Keep surrounding behavior
/// as an explicit native-adaptation requirement, never execute it or pretend it
/// was part of the object model. Without delimiters, parsing remains strict.
fn exported_objects(text: &str) -> Result<(&str, Vec<PendingScript>)> {
    ensure!(text.len() <= 16 * 1024 * 1024, "Mission too large");
    let mut markers = Vec::new();
    let mut offset = 0;
    for (line, text_line) in text.split_inclusive('\n').enumerate() {
        match text_line.trim() {
            "//--- OBJECT WRITE BEGIN ---" => {
                markers.push((true, offset, offset + text_line.len(), line + 1))
            }
            "//--- OBJECT WRITE END ---" => {
                markers.push((false, offset, offset + text_line.len(), line + 1))
            }
            _ => {}
        }
        offset += text_line.len();
    }
    if markers.is_empty() {
        return Ok((text, vec![]));
    }
    ensure!(
        markers.len() == 2 && markers[0].0 && !markers[1].0,
        "Missing, reversed or duplicate mission export delimiters"
    );
    let (_, begin, objects_begin, begin_line) = markers[0];
    let (_, objects_end, end, end_line) = markers[1];
    let mut pending = Vec::new();
    for (section, body, first_line, last_line) in [
        ("before", &text[..begin], 1, begin_line - 1),
        ("after", &text[end..], end_line + 1, text.lines().count()),
    ] {
        if !lex(body)?.is_empty() {
            pending.push(PendingScript {
                section: section.into(),
                first_line,
                last_line,
                sha256: format!("{:x}", Sha256::digest(body.as_bytes())),
            });
        }
    }
    Ok((&text[objects_begin..objects_end], pending))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exported_objects_preserve_script_requirements_without_evaluation() {
        let scene=read("exec(\"./setup.cs\");\r\n//--- OBJECT WRITE BEGIN ---\r\nnew SimGroup(root) { new Trigger(t) { dataBlock=\"Tip\"; }; };\r\n//--- OBJECT WRITE END ---\r\nstartScenario();", "test.mis").unwrap();
        assert_eq!(scene.nodes.len(), 2);
        assert_eq!(scene.nodes[1].properties["datablock"], "Tip");
        assert_eq!(scene.pending_scripts.len(), 2);
        assert_eq!(
            (
                scene.pending_scripts[0].first_line,
                scene.pending_scripts[0].last_line
            ),
            (1, 1)
        );
        assert_eq!(
            (
                scene.pending_scripts[1].first_line,
                scene.pending_scripts[1].last_line
            ),
            (5, 5)
        );
        assert_eq!(
            scene.pending_scripts[0].sha256,
            format!("{:x}", Sha256::digest(b"exec(\"./setup.cs\");\r\n"))
        );
        assert!(read("exec(\"./setup.cs\"); new SimGroup(root) {};", "test.mis").is_err());
        assert!(
            read(
                "//--- OBJECT WRITE BEGIN ---\nexec(\"./setup.cs\");\n//--- OBJECT WRITE END ---",
                "test.mis"
            )
            .is_err()
        );
        for text in [
            "//--- OBJECT WRITE BEGIN ---",
            "//--- OBJECT WRITE END ---\n//--- OBJECT WRITE BEGIN ---",
            "//--- OBJECT WRITE BEGIN ---\n//--- OBJECT WRITE BEGIN ---\n//--- OBJECT WRITE END ---",
        ] {
            assert!(read(text, "test.mis").is_err());
        }
        let scene=read("// comment\n//--- OBJECT WRITE BEGIN ---\nnew SimGroup(root) {};\n//--- OBJECT WRITE END ---\n// comment", "test.mis").unwrap();
        assert!(scene.pending_scripts.is_empty());
    }
    #[test]
    fn terrain_origin_uses_cell_size_and_retains_serialized_position() {
        for (field, spacing) in [("", 8.0), ("squareSize=\"16\";", 16.0)] {
            let scene=read(&format!("new TerrainBlock(t) {{ terrainFile=\"./t.ter\"; position=\"9 10 11\"; {field} }};"),"Add-Ons/Map_Test/t.mis").unwrap();
            let node = &scene.nodes[0];
            let origin = Mat4::from_cols_array(&node.transform).transform_point3(Vec3::ZERO);
            assert_eq!(origin, Vec3::new(-128.0 * spacing, 0.0, 128.0 * spacing));
            assert_eq!(node.properties["position"], "9 10 11");
        }
    }
    #[test]
    fn hierarchy_and_torque_rotation() {
        let scene=read("new SimGroup(root) { new InteriorInstance(room) { interiorFile=\"./room.dif\"; rotation=\"0 0 -1 90\"; position=\"1 2 3\"; }; };","Add-Ons/Map_Test/test.mis").unwrap();
        assert_eq!(scene.nodes[1].parent, Some(0));
        assert_eq!(
            scene.nodes[1].asset.as_deref(),
            Some("v20/add-ons/map_test/room.dif")
        );
        let m = Mat4::from_cols_array(&scene.nodes[1].transform);
        assert!(
            m.transform_point3(Vec3::X)
                .distance(Vec3::new(1.0, 3.0, -3.0))
                < 1e-5
        );
        assert!(read("exec(\"bad.cs\");", "test.mis").is_err());
        assert_eq!(
            reference(
                "Add-Ons/Map_Construct/construct.mis",
                "~/Map_Slate/slate.dif"
            )
            .unwrap(),
            "v20/add-ons/map_slate/slate.dif"
        );
        assert!(reference("Add-Ons/Map_Test/test.mis", "~/../escape.dif").is_err());
    }
}
