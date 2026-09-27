//! Offline literal-only effect relationship extension. Never linked by the game.
use anyhow::{Context, Result, ensure};
use bri_content::effects::{Curve, Library, Light};
use bri_fx_runtime::pack::{Binding, Composite, Manifest, TextureRecord, digest};
use regex::Regex;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
const LIMIT: u64 = 32 << 20;
fn read_file(path: &Path) -> Result<Vec<u8>> {
    let f = fs::File::open(path)?;
    ensure!(
        f.metadata()?.is_file() && f.metadata()?.len() <= LIMIT,
        "Oversized input"
    );
    let mut b = Vec::new();
    f.take(LIMIT + 1).read_to_end(&mut b)?;
    ensure!(b.len() as u64 <= LIMIT, "Growing input exceeds bound");
    Ok(b)
}
fn resource(root: &Path, path: &str) -> Result<Vec<u8>> {
    ensure!(
        !path.contains(['\\', ':'])
            && path
                .split('/')
                .all(|s| !s.is_empty() && s != "." && s != ".."),
        "Unsafe source path"
    );
    let (base, rest) = path.split_once('/').context("Invalid resource")?;
    if base.eq_ignore_ascii_case("Add-Ons") {
        let (package, member) = rest.split_once('/').context("Invalid archive source")?;
        let wanted = format!("{package}.zip");
        let matches: Vec<_> = fs::read_dir(root.join("Add-Ons"))?
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&wanted)
            })
            .collect();
        ensure!(
            matches.len() == 1,
            "Missing/ambiguous stock archive {package}"
        );
        let path = matches[0].path().canonicalize()?;
        ensure!(path.starts_with(root), "Archive escapes source root");
        let mut zip = zip::ZipArchive::new(fs::File::open(path)?)?;
        ensure!(zip.len() <= 65536, "Archive entry budget exceeded");
        let names: Vec<_> = zip
            .file_names()
            .filter(|p| p.eq_ignore_ascii_case(member))
            .map(str::to_owned)
            .collect();
        ensure!(names.len() == 1, "Missing/ambiguous member {member}");
        let file = zip.by_name(&names[0])?;
        ensure!(file.size() <= LIMIT, "Oversized archive member");
        let mut bytes = Vec::new();
        file.take(LIMIT + 1).read_to_end(&mut bytes)?;
        ensure!(bytes.len() as u64 <= LIMIT, "Archive member exceeds bound");
        Ok(bytes)
    } else {
        let mut p = root.to_path_buf();
        for part in path.split('/') {
            let matches: Vec<_> = fs::read_dir(&p)?
                .collect::<std::io::Result<Vec<_>>>()?
                .into_iter()
                .filter(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))
                .collect();
            ensure!(matches.len() == 1, "Missing/ambiguous source {path}");
            p = matches[0].path();
        }
        let p = p.canonicalize()?;
        ensure!(p.starts_with(root), "Source alias escapes root");
        read_file(&p)
    }
}
#[derive(Clone)]
struct Decl {
    class: String,
    name: String,
    source: String,
    fields: BTreeMap<String, String>,
}
fn strip_comments(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = b.to_vec();
    let mut i = 0;
    let mut quote = 0;
    while i < b.len() {
        if quote != 0 {
            if b[i] == b'\\' {
                i += 2;
                continue;
            }
            if b[i] == quote {
                quote = 0;
            }
            i += 1;
            continue;
        }
        if b[i] == b'"' || b[i] == b'\'' {
            quote = b[i];
            i += 1;
            continue;
        }
        if b.get(i..i + 2) == Some(b"//") {
            while i < b.len() && b[i] != b'\n' {
                out[i] = b' ';
                i += 1;
            }
            continue;
        }
        if b.get(i..i + 2) == Some(b"/*") {
            out[i] = b' ';
            out[i + 1] = b' ';
            i += 2;
            while i + 1 < b.len() && &b[i..i + 2] != b"*/" {
                if b[i] != b'\n' {
                    out[i] = b' ';
                }
                i += 1;
            }
            if i + 1 < b.len() {
                out[i] = b' ';
                out[i + 1] = b' ';
                i += 2;
            }
            continue;
        }
        i += 1;
    }
    String::from_utf8(out).expect("comment replacement preserves UTF8")
}
fn declarations(
    s: &str,
    source: &str,
    entries: &mut BTreeMap<String, Decl>,
    unresolved: &mut Vec<String>,
) -> Result<()> {
    let declaration = Regex::new(
        r"(?is)\bdatablock\s+(\w+)\s*\(\s*(\w+)\s*(?::\s*(\w+)\s*)?\)\s*\{([^{}]*)\}\s*;",
    )?;
    let field = Regex::new(r"(?m)(\w+(?:\s*\[\s*\d+\s*\])?)\s*=\s*([^;]*);")?;
    for c in declaration.captures_iter(&strip_comments(s)) {
        let name = c[2].to_lowercase();
        let mut fields = if let Some(parent) = c.get(3) {
            entries
                .get(&parent.as_str().to_lowercase())
                .map(|d| d.fields.clone())
                .unwrap_or_else(|| {
                    unresolved.push(format!(
                        "{source}:{name}: parent {} not available to literal relationship reader",
                        parent.as_str()
                    ));
                    BTreeMap::new()
                })
        } else {
            BTreeMap::new()
        };
        for f in field.captures_iter(&c[4]) {
            fields.insert(
                f[1].chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
                    .to_lowercase(),
                f[2].trim().trim_matches('"').to_owned(),
            );
        }
        entries.insert(
            name.clone(),
            Decl {
                class: c[1].to_lowercase(),
                name,
                source: source.into(),
                fields,
            },
        );
    }
    Ok(())
}
fn number(d: &Decl, key: &str, default: f32) -> Result<f32> {
    let v = d
        .fields
        .get(key)
        .map(|v| v.parse::<f32>())
        .transpose()
        .with_context(|| format!("{}.{key} is nonliteral", d.name))?
        .unwrap_or(default);
    ensure!(v.is_finite(), "Nonfinite effect field");
    Ok(v)
}
fn color(d: &Decl, key: &str, default: [f32; 3]) -> Result<[f32; 3]> {
    let Some(v) = d.fields.get(key) else {
        return Ok(default);
    };
    let v: Vec<f32> = v
        .split_whitespace()
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()?;
    ensure!(
        v.len() >= 3 && v.iter().all(|v| v.is_finite() && *v >= 0.),
        "Invalid light color"
    );
    Ok([v[0], v[1], v[2]])
}
fn main() -> Result<()> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    ensure!(
        args.len() == 4,
        "Usage: bri-fx-import <original-root> <effects-pass-004> <recovered-core.cs> <new-output-dir>"
    );
    let original = args[0].canonicalize()?;
    let previous = args[1].canonicalize()?;
    ensure!(!args[3].exists(), "Output must be fresh");
    let parent = args[3]
        .parent()
        .context("Missing output parent")?
        .canonicalize()?;
    ensure!(
        !parent.starts_with(&original)
            && !original
                .starts_with(parent.join(args[3].file_name().context("Output filename missing")?)),
        "Refusing original-install output"
    );
    let output = parent.join(args[3].file_name().unwrap());
    let report: Value = serde_json::from_slice(&read_file(&previous.join("report.json"))?)?;
    ensure!(
        report["errors"].as_array().is_some_and(Vec::is_empty),
        "Prior conversion has unresolved errors"
    );
    let mut library: Library = serde_json::from_slice(&read_file(&previous.join("effects.json"))?)?;
    library.validate()?;
    let mut manifest = Manifest {
        schema_version: 1,
        library_sha256: String::new(),
        textures: BTreeMap::new(),
        emitter_alpha: BTreeMap::new(),
        bindings: Vec::new(),
        composites: Vec::new(),
        unresolved: Vec::new(),
    };
    let mut images = Vec::new();
    for t in report["texture_bindings"]
        .as_array()
        .context("No texture provenance")?
    {
        let id = t["id"].as_str().context("Texture ID missing")?;
        let source = t["source"].as_str().context("Texture source missing")?;
        let bytes = resource(&original, source)?;
        let hash = digest(&bytes);
        ensure!(
            Some(hash.as_str()) == t["sha256"].as_str(),
            "Primary original texture differs: {source}"
        );
        let file = library
            .textures
            .get(id)
            .context("Unknown texture binding")?;
        ensure!(!file.contains(['/', '\\', ':']), "Unsafe texture output");
        ensure!(
            bytes == read_file(&previous.join(file))?,
            "Previous native texture differs from primary source"
        );
        manifest.textures.insert(
            id.into(),
            TextureRecord {
                file: file.clone(),
                sha256: hash,
                width: t["width"].as_u64().context("Missing width")?.try_into()?,
                height: t["height"].as_u64().context("Missing height")?.try_into()?,
            },
        );
        images.push((file.clone(), bytes));
    }
    let mut entries = BTreeMap::new();
    let mut source_proof = Vec::new();
    for s in report["sources"]
        .as_array()
        .context("No source provenance")?
    {
        let path = s["path"].as_str().context("Source path missing")?;
        let bytes = if path == "core/allGameScripts-Vanilla.cs" {
            read_file(&args[2])?
        } else {
            resource(&original, path)?
        };
        ensure!(
            Some(digest(&bytes).as_str()) == s["sha256"].as_str(),
            "Source hash differs: {path}"
        );
        declarations(
            std::str::from_utf8(&bytes)?,
            path,
            &mut entries,
            &mut manifest.unresolved,
        )?;
        source_proof.push(s.clone());
    }
    let recovered: Value = serde_json::from_slice(&read_file(
        &previous.join("provenance/effective-declarations.json"),
    )?)?;
    for d in recovered
        .as_array()
        .context("Missing recovered effect declarations")?
    {
        if d["class"] == "particleemitterdata"
            && let Some(alpha) = d["fields"]["useinvalpha"].as_str()
        {
            ensure!(matches!(alpha, "0" | "1"), "Nonliteral alpha override");
            manifest.emitter_alpha.insert(
                format!(
                    "v20/emitter/{}",
                    d["name"].as_str().context("Emitter name")?.to_lowercase()
                ),
                alpha == "1",
            );
        }
    }
    let emitter_names: BTreeMap<_, _> = library
        .emitters
        .iter()
        .map(|e| (e.id.rsplit('/').next().unwrap().to_owned(), e.id.clone()))
        .collect();
    let mut resources = emitter_names.clone();
    resources.extend(
        library
            .lights
            .iter()
            .map(|l| (l.id.rsplit('/').next().unwrap().to_owned(), l.id.clone())),
    );
    for d in entries.values().filter(|d| d.class == "explosiondata") {
        let lifetime = number(d, "lifetimems", 1000.)? / 1000.;
        if !d.fields.contains_key("lifetimems") {
            manifest.unresolved.push(format!("{}: absent lifetimeMS uses native 1-second fallback; engine-default verification pending",d.name));
        }
        let mut c = Composite {
            id: format!("v20/explosion/{}", d.name),
            lifetime,
            emitters: Vec::new(),
            light: None,
            burst: None,
        };
        for (k, v) in &d.fields {
            if k.starts_with("emitter[") && !v.is_empty() {
                if let Some(id) = emitter_names.get(&v.to_lowercase()) {
                    c.emitters.push(id.clone());
                } else {
                    manifest
                        .unresolved
                        .push(format!("{}.{k}: missing emitter {v}", d.name));
                }
            }
        }
        if let Some(v) = d.fields.get("particleemitter").filter(|v| !v.is_empty()) {
            if let Some(id) = emitter_names.get(&v.to_lowercase()) {
                let count = number(d, "particledensity", 10.)?;
                ensure!((0.0..=32768.).contains(&count), "Invalid explosion density");
                c.burst = Some((id.clone(), count as u32, number(d, "particleradius", 1.)?));
            } else {
                manifest
                    .unresolved
                    .push(format!("{}: missing burst emitter {v}", d.name));
            }
        }
        let start_radius = number(d, "lightstartradius", 0.)?;
        let end_radius = number(d, "lightendradius", 0.)?;
        if start_radius > 0. || end_radius > 0. {
            let start = color(d, "lightstartcolor", [1.; 3])?;
            let end = color(d, "lightendcolor", [1.; 3])?;
            let id = format!("v20/explosion-light/{}", d.name);
            library.lights.push(Light {
                id: id.clone(),
                name: String::new(),
                enabled: true,
                color: start,
                brightness: 1.,
                radius: start_radius,
                color_curves: Some(std::array::from_fn(|i| Curve {
                    period: lifetime,
                    linear: true,
                    values: vec![start[i], end[i]],
                })),
                brightness_curve: None,
                radius_curve: Some(Curve {
                    period: lifetime,
                    linear: true,
                    values: vec![start_radius, end_radius],
                }),
                flare: None,
            });
            c.light = Some(id);
        }
        for k in [
            "debris",
            "explosionshape",
            "subexplosion[0]",
            "subexplosion[1]",
            "subexplosion[2]",
        ] {
            if let Some(v) = d.fields.get(k).filter(|v| !v.is_empty()) {
                manifest.unresolved.push(format!(
                    "{}.{k}={v}: separate mesh/debris/subexplosion adapter pending",
                    d.name
                ));
            }
        }
        resources.insert(d.name.clone(), c.id.clone());
        manifest.composites.push(c);
    }
    for d in entries.values() {
        for (field, value) in &d.fields {
            if let Some(resource) = resources.get(&value.to_lowercase()) {
                manifest.bindings.push(Binding {
                    owner: format!("v20/{}/{}", d.class, d.name),
                    field: field.clone(),
                    resource: resource.clone(),
                    source: d.source.clone(),
                });
            }
        }
    }
    manifest.unresolved.extend(["Brick scaled-volume and explosion radius placement distributions need exact v20 engine/playtest verification.".into(),"Relationship records are literal data; host adapters must dispatch vehicle/player/tool/weapon state transitions, audio, damage and camera shake.".into(),"Core source is recovered script, not an original plaintext file; texture/add-on hashes verified against the primary install.".into()]);
    manifest.unresolved.sort();
    manifest.unresolved.dedup();
    library.validate()?;
    let bytes = serde_json::to_vec_pretty(&library)?;
    manifest.library_sha256 = digest(&bytes);
    fs::create_dir(&output)?;
    for (file, bytes) in images {
        fs::write(output.join(file), bytes)?;
    }
    fs::write(output.join("effects.json"), bytes)?;
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    fs::write(
        output.join("source-proof.json"),
        serde_json::to_vec_pretty(
            &json!({"primary_root":original,"previous_pack":previous,"sources":source_proof,"texture_bindings":report["texture_bindings"],"prior_conversion_diagnostics":report["diagnostics"],"prior_script_diagnostics":report["script_diagnostics"]}),
        )?,
    )?;
    bri_fx_runtime::EffectsPack::load(&output)?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"lights":library.lights.len(),"particles":library.particles.len(),"emitters":library.emitters.len(),"textures":library.textures.len(),"composites":manifest.composites.len(),"bindings":manifest.bindings.len(),"unresolved":manifest.unresolved.len(),"output":output})
        )?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_relationships_keep_inheritance_and_ignore_comment_fakes() {
        let mut entries = BTreeMap::new();
        let mut unresolved = Vec::new();
        declarations("// datablock X(fake) { emitter = Evil; };\ndatablock X(base) { emitter = Good; url=\"https://example\"; }; datablock X(child:base) { emitter[1] = Other; };","fixture",&mut entries,&mut unresolved).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries["child"].fields["emitter"], "Good");
        assert_eq!(entries["base"].fields["url"], "https://example");
        assert!(unresolved.is_empty());
    }
}
