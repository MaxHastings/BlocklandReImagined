//! Offline extension from source-hashed weapon declarations. Never a runtime dep.
use anyhow::{Context, Result, ensure};
use bri_content::effects::{Curve, Light};
use bri_convert::{effect_script::Declaration, effects};
use bri_fx_runtime::{
    EffectsPack,
    pack::{Composite, TextureRecord, digest},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const LIMIT: u64 = 32 << 20;
fn read(path: &Path) -> Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    ensure!(file.metadata()?.len() <= LIMIT, "Oversized input");
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= LIMIT, "Input grew past bound");
    Ok(bytes)
}
fn unique_child(parent: &Path, name: &str) -> Result<PathBuf> {
    let matches: Vec<_> = fs::read_dir(parent)?
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .filter(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .collect();
    ensure!(matches.len() == 1, "Missing/ambiguous source {name}");
    Ok(matches[0].path())
}
fn resource(root: &Path, path: &str) -> Result<Vec<u8>> {
    ensure!(
        !path.contains(['\\', ':'])
            && path
                .split('/')
                .all(|p| !p.is_empty() && p != "." && p != ".."),
        "Unsafe source path"
    );
    let parts: Vec<_> = path.split('/').collect();
    if parts[0].eq_ignore_ascii_case("Add-Ons") {
        ensure!(parts.len() >= 3, "Missing archive member");
        let archive =
            unique_child(&root.join("Add-Ons"), &format!("{}.zip", parts[1]))?.canonicalize()?;
        ensure!(
            archive.starts_with(root),
            "Archive alias escapes original root"
        );
        let mut zip = zip::ZipArchive::new(fs::File::open(archive)?)?;
        ensure!(zip.len() <= 65536, "Archive entry bound");
        let wanted = parts[2..].join("/");
        let names: Vec<_> = zip
            .file_names()
            .filter(|n| n.eq_ignore_ascii_case(&wanted))
            .map(str::to_owned)
            .collect();
        ensure!(names.len() == 1, "Missing/ambiguous archive member {path}");
        let file = zip.by_name(&names[0])?;
        ensure!(file.size() <= LIMIT, "Oversized archive member");
        let mut bytes = Vec::new();
        file.take(LIMIT + 1).read_to_end(&mut bytes)?;
        ensure!(bytes.len() as u64 <= LIMIT, "Oversized archive bytes");
        Ok(bytes)
    } else {
        let mut file = root.to_owned();
        for part in parts {
            file = unique_child(&file, part)?;
        }
        let file = file.canonicalize()?;
        ensure!(file.starts_with(root), "File alias escapes original root");
        read(&file)
    }
}
fn native(d: &bri_weapons::Definition) -> Declaration {
    Declaration {
        name: d.name.clone(),
        class: d.class.to_ascii_lowercase(),
        source: d.source.path.clone(),
        fields: d
            .fields
            .iter()
            .map(|(k, v)| (k.clone(), v.trim().trim_matches('"').to_owned()))
            .collect(),
    }
}
/// `SplashData` as a finite emitter of ring particles: every `1/ejectionFreq`
/// seconds for `lifetimeMS`, `numSegments` bubbles leave `startRadius` at
/// `ejectionAngle` with `velocity` and `acceleration`, living `ringLifetime`
/// with the splash's colors. The engine draws each ring as a textured band;
/// the band width here is the spacing between rings, velocity / ejectionFreq.
fn splash(d: &Declaration) -> Result<[Declaration; 2]> {
    let segments = number(d, "numsegments", 10.)?;
    let frequency = number(d, "ejectionfreq", 5.)?;
    let velocity = number(d, "velocity", 5.)?;
    ensure!(
        segments >= 1. && frequency > 0. && velocity >= 0.,
        "Invalid splash rings"
    );
    let band = (velocity / frequency).to_string();
    let ring = format!("{}Ring", d.name);
    let text = |key: &str, default: &str| {
        d.fields
            .get(key)
            .cloned()
            .unwrap_or_else(|| default.to_owned())
    };
    let mut particle = BTreeMap::from([
        ("texturename".into(), text("texture", "")),
        (
            "lifetimems".into(),
            (number(d, "ringlifetime", 1.)? * 1000.).to_string(),
        ),
        ("constantacceleration".into(), text("acceleration", "0")),
        ("dragcoefficient".into(), "0".into()),
        ("gravitycoefficient".into(), "0".into()),
        ("windcoefficient".into(), "0".into()),
        ("inheritedvelfactor".into(), "0".into()),
    ]);
    for i in 0..4 {
        for key in ["colors", "times"] {
            if let Some(v) = d.fields.get(&format!("{key}[{i}]")) {
                particle.insert(format!("{key}[{i}]"), v.clone());
            }
        }
        particle.insert(format!("sizes[{i}]"), band.clone());
    }
    let emitter = BTreeMap::from([
        (
            "ejectionperiodms".into(),
            (1000. / (frequency * segments)).to_string(),
        ),
        ("periodvariancems".into(), "0".into()),
        ("ejectionvelocity".into(), velocity.to_string()),
        ("velocityvariance".into(), "0".into()),
        ("ejectionoffset".into(), text("startradius", "0")),
        ("thetamin".into(), text("ejectionangle", "45")),
        ("thetamax".into(), text("ejectionangle", "45")),
        ("phireferencevel".into(), "0".into()),
        ("phivariance".into(), "360".into()),
        ("lifetimems".into(), text("lifetimems", "1000")),
        ("particles".into(), ring.clone()),
    ]);
    Ok([
        Declaration {
            name: ring,
            class: "particledata".into(),
            source: d.source.clone(),
            fields: particle,
        },
        Declaration {
            name: d.name.clone(),
            class: "particleemitterdata".into(),
            source: d.source.clone(),
            fields: emitter,
        },
    ])
}
fn number(d: &Declaration, key: &str, default: f32) -> Result<f32> {
    let v = d
        .fields
        .get(key)
        .map(|s| s.parse::<f32>())
        .transpose()?
        .unwrap_or(default);
    ensure!(v.is_finite(), "Nonfinite {key}");
    Ok(v)
}
fn color(d: &Declaration, key: &str) -> Result<[f32; 3]> {
    let Some(s) = d.fields.get(key) else {
        return Ok([1.; 3]);
    };
    let v = s
        .split_whitespace()
        .map(str::parse)
        .collect::<std::result::Result<Vec<f32>, _>>()?;
    ensure!(
        v.len() >= 3 && v.iter().all(|v| v.is_finite()),
        "Invalid color"
    );
    Ok([v[0], v[1], v[2]])
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    ensure!(
        args.len() == 5,
        "Usage: importer <original-root> <base-fx-pack> <weapons.json> <fresh-output> <recovered-core.cs>"
    );
    let original = args[0].canonicalize()?;
    let output = args[3]
        .parent()
        .context("Output parent")?
        .canonicalize()?
        .join(args[3].file_name().context("Output name")?);
    ensure!(
        !output.exists() && !output.starts_with(&original) && !original.starts_with(&output),
        "Output must be fresh and outside original installation"
    );
    let pack = EffectsPack::load(&args[1])?;
    let weapon_bytes = read(&args[2])?;
    let weapons = bri_weapons::Pack::from_json(&weapon_bytes)?;
    let mut sources = BTreeMap::new();
    for d in &weapons.definitions {
        ensure!(
            sources
                .get(&d.source.path)
                .is_none_or(|hash| hash == &d.source.sha256),
            "Inconsistent evidence hashes for one source"
        );
        if !sources.contains_key(&d.source.path) {
            let bytes = if d.source.path == "base/server/scripts/allGameScripts.cs (recovered)" {
                read(&args[4])?
            } else {
                resource(&original, &d.source.path)?
            };
            ensure!(
                digest(&bytes) == d.source.sha256,
                "Source hash changed: {}",
                d.source.path
            );
            sources.insert(d.source.path.clone(), d.source.sha256.clone());
        }
    }
    let mut library = pack.library.clone();
    let mut manifest = pack.manifest.clone();
    let mut files = BTreeMap::new();
    for t in manifest.textures.values() {
        ensure!(
            !t.file.contains(['/', '\\', ':']),
            "Unsafe base texture filename"
        );
        files.insert(t.file.clone(), read(&args[1].join(&t.file))?);
    }
    let mut declarations: Vec<_> = weapons.definitions.iter().map(native).collect();
    let mut added_splashes = Vec::new();
    for d in declarations.clone().iter().filter(|d| d.class == "splashdata") {
        added_splashes.push(effects::id("emitter", &d.name));
        declarations.extend(splash(d)?);
    }
    let nodes: BTreeMap<_, _> = declarations
        .iter()
        .filter(|d| d.class == "particleemitternodedata")
        .map(|d| Ok((d.name.to_ascii_lowercase(), number(d, "timescale", 1.)?)))
        .collect::<Result<_>>()?;
    let mut notes = Vec::new();
    let mut added_particles = Vec::new();
    let mut added_emitters = Vec::new();
    let mut added_composites = Vec::new();
    for d in declarations.iter().filter(|d| d.class == "particledata") {
        let id = effects::id("particle", &d.name);
        if library.particles.iter().any(|p| p.id == id) {
            continue;
        }
        let (particle, diagnostics) = effects::particle(d)?;
        notes.extend(diagnostics.into_iter().map(|n| format!("{}: {n}", d.name)));
        if !library.textures.contains_key(&particle.texture) {
            let mut matches = Vec::new();
            for suffix in ["", ".png", ".jpg", ".jpeg"] {
                let path = format!("{}{suffix}", particle.texture);
                if let Ok(bytes) = resource(&original, &path) {
                    matches.push((path, bytes));
                }
            }
            ensure!(
                matches.len() == 1,
                "Missing/ambiguous texture {}",
                particle.texture
            );
            let (source, bytes) = matches.pop().unwrap();
            let hash = digest(&bytes);
            let reader =
                image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format()?;
            let (width, height) = reader.into_dimensions()?;
            let file = format!("{hash}.{}", source.rsplit('.').next().unwrap());
            library
                .textures
                .insert(particle.texture.clone(), file.clone());
            manifest.textures.insert(
                particle.texture.clone(),
                TextureRecord {
                    file: file.clone(),
                    sha256: hash,
                    width,
                    height,
                },
            );
            files.insert(file, bytes);
            notes.push(format!(
                "Original texture {} from {source}",
                particle.texture
            ));
        }
        added_particles.push(particle.id.clone());
        library.particles.push(particle);
    }
    for d in declarations
        .iter()
        .filter(|d| d.class == "particleemitterdata")
    {
        let id = effects::id("emitter", &d.name);
        if library.emitters.iter().any(|e| e.id == id) {
            continue;
        }
        let (emitter, diagnostics) = effects::emitter(d, &nodes)?;
        notes.extend(diagnostics.into_iter().map(|n| format!("{}: {n}", d.name)));
        if let Some(alpha) = d.fields.get("useinvalpha") {
            ensure!(
                matches!(alpha.as_str(), "true" | "false" | "1" | "0"),
                "Nonliteral emitter alpha"
            );
            manifest
                .emitter_alpha
                .insert(id, matches!(alpha.as_str(), "true" | "1"));
        }
        added_emitters.push(emitter.id.clone());
        library.emitters.push(emitter);
    }
    let emitters: BTreeSet<_> = library.emitters.iter().map(|e| e.id.clone()).collect();
    for d in declarations.iter().filter(|d| d.class == "explosiondata") {
        let id = effects::id("explosion", &d.name);
        if manifest.composites.iter().any(|c| c.id == id) {
            continue;
        }
        // Keep the baseline importer's explicit engine-default uncertainty.
        let lifetime = number(d, "lifetimems", 1000.)? / 1000.;
        if !d.fields.contains_key("lifetimems") {
            manifest.unresolved.push(format!("{}: absent lifetimeMS uses baseline one-second fallback; original engine default verification pending", d.name));
        }
        let mut c = Composite {
            id,
            lifetime,
            emitters: vec![],
            light: None,
            burst: None,
        };
        for (key, value) in &d.fields {
            if key.starts_with("emitter[") && !value.is_empty() {
                let id = effects::id("emitter", value);
                ensure!(emitters.contains(&id), "Missing composite emitter {id}");
                c.emitters.push(id);
            }
        }
        if let Some(value) = d.fields.get("particleemitter").filter(|s| !s.is_empty()) {
            let id = effects::id("emitter", value);
            ensure!(emitters.contains(&id), "Missing burst emitter {id}");
            let count = number(d, "particledensity", 10.)?;
            ensure!(
                (0.0..=32768.).contains(&count) && count.fract() == 0.,
                "Invalid burst count"
            );
            c.burst = Some((id, count as u32, number(d, "particleradius", 1.)?));
        }
        let start = number(d, "lightstartradius", 0.)?;
        let end = number(d, "lightendradius", 0.)?;
        if start > 0. || end > 0. {
            let a = color(d, "lightstartcolor")?;
            let b = color(d, "lightendcolor")?;
            let id = effects::id("explosion-light", &d.name);
            library.lights.push(Light {
                id: id.clone(),
                name: String::new(),
                enabled: true,
                color: a,
                brightness: 1.,
                radius: start,
                color_curves: Some(std::array::from_fn(|i| Curve {
                    period: lifetime,
                    linear: true,
                    values: vec![a[i], b[i]],
                })),
                brightness_curve: None,
                radius_curve: Some(Curve {
                    period: lifetime,
                    linear: true,
                    values: vec![start, end],
                }),
                flare: None,
            });
            c.light = Some(id);
        }
        for key in [
            "debris",
            "explosionshape",
            "subexplosion[0]",
            "subexplosion[1]",
            "subexplosion[2]",
        ] {
            if let Some(v) = d.fields.get(key).filter(|v| !v.is_empty()) {
                manifest.unresolved.push(format!(
                    "{}.{key}={v}: model/debris/subexplosion host adapter pending",
                    d.name
                ));
            }
        }
        added_composites.push(c.id.clone());
        manifest.composites.push(c);
    }
    let mut resources: BTreeMap<_, _> = library
        .emitters
        .iter()
        .map(|e| (e.id.rsplit('/').next().unwrap().to_owned(), e.id.clone()))
        .collect();
    resources.extend(
        manifest
            .composites
            .iter()
            .map(|c| (c.id.rsplit('/').next().unwrap().to_owned(), c.id.clone())),
    );
    for d in &declarations {
        for (field, value) in &d.fields {
            if let Some(resource) = resources.get(&value.to_ascii_lowercase()) {
                let owner = format!("v20/{}/{}", d.class, d.name.to_ascii_lowercase());
                if !manifest
                    .bindings
                    .iter()
                    .any(|b| b.owner == owner && &b.field == field)
                {
                    manifest.bindings.push(bri_fx_runtime::Binding {
                        owner,
                        field: field.clone(),
                        resource: resource.clone(),
                        source: d.source.clone(),
                    });
                }
            }
        }
    }
    library.validate()?;
    let bytes = serde_json::to_vec_pretty(&library)?;
    manifest.library_sha256 = digest(&bytes);
    fs::create_dir(&output)?;
    for (file, bytes) in files {
        fs::write(output.join(file), bytes)?;
    }
    fs::write(output.join("effects.json"), bytes)?;
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    let proof = serde_json::json!({"original":original,"weapons_sha256":digest(&weapon_bytes),"sources":sources,
        "added_particles":added_particles,"added_emitters":added_emitters,"added_composites":added_composites,"added_splashes":added_splashes,
        "conversion_diagnostics":notes,"base_pack":args[1],"base_library_sha256":pack.manifest.library_sha256});
    fs::write(
        output.join("weapon-source-proof.json"),
        serde_json::to_vec_pretty(&proof)?,
    )?;
    EffectsPack::load(&output)?;
    println!("{}", serde_json::to_string_pretty(&proof)?);
    Ok(())
}
