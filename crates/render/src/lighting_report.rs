//! Which lighting each draw of a loaded scene takes, set against the original engine's path for that kind of surface
//! (`native-research/specs/render-path-per-kind.md`): a one-time account for a map that has just loaded, in triangles per kind,
//! so a surface that falls to the renderer's own live lighting instead of the ported path is named, not guessed at.
use crate::scene::{AlphaMode, Material, MaterialKind, SceneData};
use std::collections::BTreeMap;

/// One kind of surface and how it is lit.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub kind: &'static str,
    /// What feeds the shader, and the multiplier it takes.
    pub source: &'static str,
    /// Follows the original engine's path for this kind.
    pub matches: bool,
    /// Why not, when it does not.
    pub reason: &'static str,
    pub batches: usize,
    pub triangles: usize,
    pub materials: usize,
    /// Up to three material names, for a row that does not match.
    pub examples: Vec<String>,
}

/// The original engine's path a material follows: (kind, source and multiplier, matches, reason it does not).
fn classify(m: &Material) -> (&'static str, &'static str, bool, &'static str) {
    if matches!(m.kind, MaterialKind::Sky | MaterialKind::Cloud) {
        return ("sky", "texture, fog backdrop; no scene light", true, "");
    }
    if m.kind == MaterialKind::LayerBlend {
        return (
            "terrain",
            "layers x 2 x baked vertex colour (UTerrainSector::StaticLight)",
            true,
            "",
        );
    }
    if m.kind == MaterialKind::VertexLight {
        return (
            "static mesh",
            "picture x 2 x runtime vertex light (FStaticMeshBatchVertexStream)",
            true,
            "",
        );
    }
    if m.baked_exact {
        return (
            "BSP surface",
            "picture x lightmap x strength (MODULATE2X of the half-value lightmap)",
            true,
            "",
        );
    }
    if matches!(m.alpha, AlphaMode::Multiply | AlphaMode::Frame(_)) {
        return (
            "unlit frame blend",
            "picture, frame-buffer blend; no scene light",
            true,
            "",
        );
    }
    if matches!(
        m.kind,
        MaterialKind::Unlit | MaterialKind::UnlitOverlay | MaterialKind::BrickOverlay
    ) {
        return ("unlit", "picture only", true, "");
    }
    (
        "other",
        "the renderer's own live lighting (sun, ambient, point lights)",
        false,
        "no lightmap, vertex light or unlit flag reaches this material",
    )
}

/// Rows for a scene, most triangles first.
pub fn report(scene: &SceneData) -> Vec<Row> {
    let mut rows: BTreeMap<(&'static str, &'static str), Row> = BTreeMap::new();
    let mut counted: BTreeMap<(&'static str, &'static str), std::collections::BTreeSet<usize>> =
        BTreeMap::new();
    for batch in &scene.batches {
        let Some(material) = scene.materials.get(batch.material) else {
            continue;
        };
        let (kind, source, matches, reason) = classify(material);
        let row = rows.entry((kind, source)).or_insert_with(|| Row {
            kind,
            source,
            matches,
            reason,
            batches: 0,
            triangles: 0,
            materials: 0,
            examples: Vec::new(),
        });
        row.batches += 1;
        row.triangles += (batch.indices.end - batch.indices.start) as usize / 3;
        if counted
            .entry((kind, source))
            .or_default()
            .insert(batch.material)
        {
            row.materials += 1;
            if !matches && row.examples.len() < 3 {
                row.examples.push(material.name.clone());
            }
        }
    }
    let mut rows: Vec<_> = rows.into_values().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.triangles));
    rows
}

/// The report as console lines.
pub fn lines(rows: &[Row]) -> Vec<String> {
    let mut out = vec!["Lighting by surface kind (triangles):".to_string()];
    for r in rows {
        let verdict = if r.matches {
            "follows the original path".to_string()
        } else {
            format!("DIFFERS: {}", r.reason)
        };
        let examples = if r.examples.is_empty() {
            String::new()
        } else {
            format!(" [{}]", r.examples.join(", "))
        };
        out.push(format!(
            "  {}: {} triangles in {} draws, {} materials; {}; {verdict}{examples}",
            r.kind, r.triangles, r.batches, r.materials, r.source
        ));
    }
    out
}
