use crate::Reader;
use anyhow::{Context, Result, ensure};
use bri_content::{TERRAIN_SCHEMA, Terrain, TerrainLayer};
use serde::{Deserialize, Serialize};

const SAMPLES: usize = 256 * 256;

/// Preserved separately from runtime content. Scripts are data, never executed.
#[derive(Debug, Serialize, Deserialize)]
pub struct TerrainProvenance {
    pub source_version: u8,
    pub original_material_flags: Vec<u8>,
    pub texture_authoring_script: Vec<u8>,
    pub height_authoring_script: Vec<u8>,
    pub warnings: Vec<String>,
}

pub fn read_v3(data: &[u8], id: String) -> Result<(Terrain, TerrainProvenance)> {
    let mut reader = Reader::new(data);
    let version = reader.u8()?;
    ensure!(
        version == 3,
        "Unsupported TER version {version}; expected 3"
    );
    let elevations = (0..SAMPLES)
        .map(|_| reader.u16().map(|v| v as f32 / 32.0))
        .collect::<Result<Vec<_>>>()?;
    let original_material_flags = reader.bytes(SAMPLES)?.to_vec();
    let primary_layers = original_material_flags.iter().map(|v| v & 7).collect();
    let mut names = Vec::with_capacity(8);
    for _ in 0..8 {
        names.push(reader.string8()?);
    }
    let mut layers = Vec::new();
    for (slot, name) in names.into_iter().enumerate() {
        if !name.is_empty() {
            layers.push(TerrainLayer {
                slot: slot as u8,
                material: name,
                weights: reader.bytes(SAMPLES)?.to_vec(),
            });
        }
    }
    let texture_authoring_script = reader.blob32(4 * 1024 * 1024)?;
    let height_authoring_script = reader.blob32(4 * 1024 * 1024)?;
    reader.finish()?;
    let mut warnings = vec!["Material references require resolution; map spacing/placement/repetition are supplied by MIS, not TER".into()];
    if original_material_flags.iter().any(|v| v & !7 != 0) {
        warnings.push("Non-layer material flag bits retained in provenance; their gameplay meaning has not been resolved".into());
    }
    let terrain = Terrain {
        schema_version: TERRAIN_SCHEMA,
        id,
        side: 256,
        elevations,
        layers,
        primary_layers,
    };
    terrain.validate()?;
    Ok((
        terrain,
        TerrainProvenance {
            source_version: version,
            original_material_flags,
            texture_authoring_script,
            height_authoring_script,
            warnings,
        },
    ))
}

/// Legacy `TerrainBlock` defaults (classic engine field table).
const DEFAULT_BUMP_SCALE: f32 = 1.0;
const DEFAULT_BUMP_OFFSET: f32 = 0.01;
const DEFAULT_ZERO_BUMP_SCALE: i32 = 8;

fn number<T: std::str::FromStr>(
    node: &bri_content::scene::Node,
    key: &str,
    default: T,
    diagnostics: &mut Vec<String>,
) -> T {
    match node.properties.get(key).map(|v| v.trim().parse::<T>()) {
        None => default,
        Some(Ok(value)) => value,
        Some(Err(_)) => {
            diagnostics.push(format!(
                "Unparseable terrain field {key}; classic default used"
            ));
            default
        }
    }
}

/// Decode the legacy `emptySquares` run list: each signed 32-bit entry packs
/// `(count << 16) | first`, with `first = x + y * 256` over the primary block.
/// Invalid entries fail conversion instead of being silently dropped.
pub fn empty_runs(value: &str, side: u32) -> Result<Vec<[u32; 2]>> {
    let cells = side * side;
    let mut runs = Vec::new();
    for token in value
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
    {
        let packed = token
            .parse::<i64>()
            .map_err(|_| anyhow::anyhow!("Invalid emptySquares entry {token}"))?;
        ensure!(
            (i64::from(i32::MIN)..=i64::from(u32::MAX)).contains(&packed),
            "emptySquares entry outside 32-bit range"
        );
        let bits = packed as u32;
        let first = bits & 0xffff;
        let count = bits >> 16;
        ensure!(
            first + count <= cells,
            "emptySquares run {first}+{count} exceeds the terrain block"
        );
        if count > 0 {
            runs.push([first, count]);
        }
    }
    ensure!(runs.len() <= 65_536, "Too many emptySquares runs");
    Ok(runs)
}

/// Strip a recognised image extension (case-insensitive) from a reference.
pub fn image_stem(path: &str) -> &str {
    for ext in [".png", ".jpg", ".jpeg"] {
        if path.len() > ext.len() && path[path.len() - ext.len()..].eq_ignore_ascii_case(ext) {
            return &path[..path.len() - ext.len()];
        }
    }
    path
}

/// Convert one authored terrain placement. `resolve` receives a lowercase
/// virtual texture path without the `v20/` prefix (possibly extensionless)
/// and returns the package-local image, or `None` if it is not packaged.
pub fn instance(
    scene: &bri_content::scene::Scene,
    node_index: usize,
    side: u32,
    resolve: &mut dyn FnMut(&str) -> Result<Option<bri_content::terrain_field::TerrainTexture>>,
) -> Result<bri_content::terrain_field::TerrainInstance> {
    use bri_content::terrain_field::*;
    let node = scene.nodes.get(node_index).context("Terrain node index")?;
    ensure!(
        matches!(node.kind, bri_content::scene::Kind::Terrain),
        "Scene node is not a terrain placement"
    );
    let mission = scene
        .id
        .strip_prefix("v20/")
        .context("Scene ID lacks the v20 namespace")?;
    let mut diagnostics = Vec::new();
    let square_size = number(node, "squaresize", 8_i32, &mut diagnostics);
    ensure!(square_size > 0, "Invalid terrain squareSize");
    let (repeat, repeat_source) = match node.properties.get("repeatterrain") {
        Some(value) => (legacy_bool(value), RepeatSource::Authored),
        None => {
            diagnostics.push("RepeatTerrain is not authored; classic always-repeat default used (Blockland default unverified)".into());
            (true, RepeatSource::LegacyDefaultUnverified)
        }
    };
    let empty = match node.properties.get("emptysquares") {
        Some(value) => empty_runs(value, side)?,
        None => vec![],
    };
    let mut texture =
        |key: &str, diagnostics: &mut Vec<String>| -> Result<Option<TerrainTexture>> {
            let Some(value) = node
                .properties
                .get(key)
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
            else {
                return Ok(None);
            };
            let virtual_path = crate::mission::reference(mission, value)?;
            let path = virtual_path
                .strip_prefix("v20/")
                .context("Unexpected texture namespace")?;
            let found = resolve(path)?;
            if found.is_none() {
                diagnostics.push(format!(
                    "Terrain {key} {value} is not packaged; it is not rendered"
                ));
            }
            Ok(found)
        };
    let detail = texture("detailtexture", &mut diagnostics)?;
    let bump_texture = texture("bumptexture", &mut diagnostics)?;
    let mut scale = number(node, "bumpscale", DEFAULT_BUMP_SCALE, &mut diagnostics);
    if scale.is_nan() || scale <= 0.0 || !scale.is_finite() {
        // The classic network pack clamps non-positive scale to 0.0001.
        diagnostics
            .push("Non-positive bumpScale clamped to 0.0001 as in the classic engine".into());
        scale = 0.0001;
    }
    let zero_scale = number(
        node,
        "zerobumpscale",
        DEFAULT_ZERO_BUMP_SCALE,
        &mut diagnostics,
    )
    .clamp(0, 31);
    let instance = TerrainInstance {
        schema_version: TERRAIN_INSTANCE_SCHEMA,
        node: node_index,
        terrain: node
            .asset
            .clone()
            .context("Terrain placement has no asset")?,
        square_size: square_size as f32,
        origin: TerrainField::origin_from_transform(&node.transform)?,
        repeat,
        repeat_source,
        empty_runs: empty,
        detail,
        bump: TerrainBump {
            texture: bump_texture,
            scale,
            offset: number(node, "bumpoffset", DEFAULT_BUMP_OFFSET, &mut diagnostics),
            zero_scale,
        },
        diagnostics,
    };
    instance.validate(side)?;
    Ok(instance)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut out = vec![3];
        for _ in 0..SAMPLES {
            out.extend(320_u16.to_le_bytes());
        }
        out.extend(vec![1_u8; SAMPLES]);
        for name in ["grass", "rock", "", "", "", "", "", ""] {
            out.push(name.len() as u8);
            out.extend(name.as_bytes());
        }
        out.extend(vec![90; SAMPLES]);
        out.extend(vec![165; SAMPLES]);
        out.extend(3_u32.to_le_bytes());
        out.extend(b"abc");
        out.extend(0_u32.to_le_bytes());
        out
    }
    #[test]
    fn preserves_blends_and_authoring_data() {
        let (terrain, source) = read_v3(&fixture(), "test/terrain".into()).unwrap();
        assert_eq!(terrain.elevations[123], 10.0);
        assert_eq!(terrain.layers[0].weights[123], 90);
        assert_eq!(terrain.layers[1].weights[123], 165);
        assert_eq!(terrain.primary_layers[123], 1);
        assert_eq!(source.texture_authoring_script, b"abc");
    }
    #[test]
    fn rejects_truncated_oversized_and_unknown_data() {
        let good = fixture();
        for length in [0, 1, 100, 131073, 196609, good.len() - 1] {
            assert!(read_v3(&good[..length], "test".into()).is_err());
        }
        let mut data = good.clone();
        data[0] = 7;
        assert!(read_v3(&data, "test".into()).is_err());
        let mut data = good.clone();
        data.push(0);
        assert!(read_v3(&data, "test".into()).is_err());
        let mut data = good;
        let end = data.len();
        data[end - 4..].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_v3(&data, "test".into()).is_err());
    }

    #[test]
    fn converts_empty_runs_repeat_and_detail_bump() {
        use bri_content::terrain_field::*;
        assert_eq!(empty_runs("", 256).unwrap(), Vec::<[u32; 2]>::new());
        // Two squares from (3, 2) and one at the last cell; zero runs vanish.
        let packed = (2 << 16) | (2 * 256 + 3);
        let last = (1 << 16) | 65535;
        assert_eq!(
            empty_runs(&format!("{packed} {last}, 17"), 256).unwrap(),
            vec![[2 * 256 + 3, 2], [65535, 1]]
        );
        assert!(empty_runs(&format!("{}", (2 << 16) | 65535), 256).is_err());
        assert!(empty_runs("x", 256).is_err());
        // Signed legacy storage of long runs.
        let long = ((40000_u32 << 16) | 10) as i32;
        assert_eq!(
            empty_runs(&long.to_string(), 256).unwrap(),
            vec![[10, 40000]]
        );
        let scene = read_scene();
        let mut asked = vec![];
        let i = instance(&scene, 0, 256, &mut |path| {
            asked.push(path.to_string());
            Ok(
                (path == "add-ons/map_test/paper05.jpg").then(|| TerrainTexture {
                    file: "abc.jpg".into(),
                    source: "Add-Ons/Map_Test/PAPER05.jpg".into(),
                }),
            )
        })
        .unwrap();
        assert_eq!(
            asked,
            ["add-ons/map_test/paper05.jpg", "add-ons/map_test/ttgrass01"]
        );
        assert!(!i.repeat && i.repeat_source == RepeatSource::Authored);
        assert_eq!(i.square_size, 16.0);
        assert_eq!(i.origin, [-2048.0, 0.0, 2048.0]);
        assert_eq!(i.empty_runs, vec![[258, 3]]);
        assert_eq!(i.detail.as_ref().unwrap().file, "abc.jpg");
        assert!(i.bump.texture.is_none());
        assert_eq!(
            (i.bump.scale, i.bump.offset, i.bump.zero_scale),
            (14.0, 0.1, 5)
        );
        assert!(i.diagnostics.iter().any(|d| d.contains("bumptexture")));
    }
    fn read_scene() -> bri_content::scene::Scene {
        crate::mission::read(
            "new TerrainBlock(t) { terrainFile=\"./t.ter\"; squareSize=\"16\"; RepeatTerrain=\"0\"; emptySquares=\"196866\"; detailTexture=\"./PAPER05.jpg\"; bumpTexture=\"./TTgrass01\"; bumpScale=\"14\"; bumpOffset=\"0.1\"; zeroBumpScale=\"5\"; };",
            "Add-Ons/Map_Test/test.mis",
        )
        .unwrap()
    }
}
