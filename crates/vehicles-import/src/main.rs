use anyhow::{Context, Result, ensure};
use bri_content::shape::Shape;
use bri_vehicles::schema::*;
use bri_vehicles_import::*;
use regex::Regex;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 4,
        "usage: bri-vehicles-import VANILLA_ROOT MAPS_PASS OUTPUT_NEW_DIRECTORY"
    );
    let root = Path::new(&args[1]);
    let maps = Path::new(&args[2]);
    let out = Path::new(&args[3]);
    ensure!(!out.exists(), "output already exists; use fresh version");
    let packages = [
        "Vehicle_Ball",
        "Vehicle_Flying_Wheeled_Jeep",
        "Vehicle_Horse",
        "Vehicle_Jeep",
        "Vehicle_Magic_Carpet",
        "Vehicle_Pirate_Cannon",
        "Vehicle_Rowboat",
        "Vehicle_Tank",
        "Item_Skis",
    ];
    let db = Regex::new(r"(?is)datablock\s+\w+\s*\(\s*(\w+)(?:\s*:\s*\w+)?\s*\)\s*\{(.*?)\};")?;
    let fields = Regex::new(r"(?m)(\w+(?:\[\d+\])?)\s*=\s*([^;]+);")?;
    let comments = Regex::new(r"//[^\n]*")?;
    let mut blocks = BTreeMap::new();
    let mut files = BTreeMap::new();
    for package in packages {
        let archive_path = root.join("Add-Ons").join(format!("{package}.zip"));
        let mut zip = zip::ZipArchive::new(fs::File::open(&archive_path)?)?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            if entry.is_dir() {
                continue;
            }
            ensure!(entry.size() < 64 * 1024 * 1024, "oversized archive member");
            let name = entry.name().to_string();
            let mut bytes = vec![];
            entry.read_to_end(&mut bytes)?;
            let virtual_name = format!("Add-Ons/{package}/{name}");
            if name.to_lowercase().ends_with(".cs") {
                let text = String::from_utf8_lossy(&bytes).replace('\r', "");
                let clean = comments.replace_all(&text, "");
                for cap in db.captures_iter(&clean) {
                    let name = cap[1].to_string();
                    let mut values = BTreeMap::new();
                    for f in fields.captures_iter(&cap[2]) {
                        values.insert(
                            f[1].to_lowercase(),
                            f[2].trim().trim_matches('"').to_string(),
                        );
                    }
                    let line = clean[..cap.get(0).unwrap().start()]
                        .bytes()
                        .filter(|x| *x == b'\n')
                        .count()
                        + 1;
                    blocks.insert(
                        name.to_lowercase(),
                        Block {
                            package: package.into(),
                            fields: values,
                            evidence: Evidence {
                                source: virtual_name.clone(),
                                sha256: hash(&bytes),
                                line,
                                subject: name,
                            },
                        },
                    );
                }
            }
            files.insert(virtual_name.to_lowercase(), (virtual_name, bytes));
        }
    }
    let manifest: Value = serde_json::from_slice(&fs::read(maps.join("manifest.json"))?)?;
    let records = manifest["records"].as_array().context("records")?;
    let mut assets = vec![];
    let mut models = BTreeMap::<String, (String, Shape)>::new();
    let mut pending = Vec::<(PathBuf, Vec<u8>)>::new();
    for r in records {
        let Some(v) = r["virtual_path"].as_str() else {
            continue;
        };
        if !packages.iter().any(|p| {
            v.to_lowercase()
                .starts_with(&format!("add-ons/{}/", p.to_lowercase()))
        }) {
            continue;
        }
        let Some(file) = r["output"].as_str() else {
            continue;
        };
        if file.ends_with(".shape.json") || file.ends_with(".clips.json") {
            let bytes = fs::read(maps.join(file))?;
            let path = format!("models/{file}");
            if file.ends_with(".shape.json") {
                let shape: Shape = serde_json::from_slice(&bytes)?;
                shape.validate()?;
                models.insert(v.to_lowercase(), (path.clone(), shape));
            }
            assets.push(Asset {
                source_sha256: r["source_sha256"].as_str().context("source hash")?.into(),
                virtual_path: v.into(),
                path: path.clone(),
                sha256: hash(&bytes),
                kind: if file.ends_with(".clips.json") {
                    "animation"
                } else {
                    "model"
                }
                .into(),
            });
            pending.push((path.into(), bytes));
        }
    }
    for (v, bytes) in files.values() {
        if [".png", ".jpg"]
            .iter()
            .any(|x| v.to_lowercase().ends_with(x))
        {
            let ext = Path::new(v).extension().unwrap().to_string_lossy();
            let path = format!("textures/{}.{}", hash(bytes), ext);
            assets.push(Asset {
                source_sha256: hash(bytes),
                virtual_path: v.clone(),
                path: path.clone(),
                sha256: hash(bytes),
                kind: "texture".into(),
            });
            pending.push((path.into(), bytes.clone()));
        }
    }
    let entries = [
        ("BallVehicle", Family::Ball),
        ("FlyingWheeledJeepVehicle", Family::FlyingWheeled),
        ("HorseArmor", Family::Horse),
        ("JeepVehicle", Family::Wheeled),
        ("MagicCarpetVehicle", Family::Flying),
        ("CannonTurret", Family::Cannon),
        ("RowBoatArmor", Family::Rowboat),
        ("TankVehicle", Family::Wheeled),
        ("TankTurretPlayer", Family::Turret),
        ("skiVehicle", Family::Skis),
        ("deathVehicle", Family::Tumble),
    ];
    let mut definitions = vec![];
    let vanilla_id = |kind: &str, name: &str| format!("v20.{kind}.{}", name.to_lowercase());
    for (name, family) in entries {
        definitions.push(lower(name, family, &blocks, &models, &files, &vanilla_id)?);
    }
    let mut animation_aliases = BTreeMap::new();
    if let Some(horse) = blocks.get("horsedts") {
        for (key, value) in &horse.fields {
            if key.starts_with("sequence") {
                let words: Vec<_> = value.split_whitespace().collect();
                if words.len() == 2 {
                    let vp = virtual_path(&horse.package, words[0]);
                    let asset = assets
                        .iter()
                        .find(|a| a.virtual_path.eq_ignore_ascii_case(&vp))
                        .with_context(|| format!("missing horse animation {vp}"))?;
                    animation_aliases.insert(
                        format!("v20.vehicle.horsearmor::{}", words[1]),
                        asset.path.clone(),
                    );
                }
            }
        }
    }
    let pack=Pack{schema_version:SCHEMA_VERSION,animation_aliases,definitions,assets,evidence:blocks.values().map(|b|b.evidence.clone()).collect(),unresolved:vec!["Exact original physics solver parity requires Maxwell playtest; native Rapier adaptation preserves authored inputs, not engine numerical equivalence".into(),"Native material asset index preserves all package textures; renderer must resolve material names against virtual paths, colorShift sentinels and default base textures".into(),"Horse external DSQ clips are included; host animation adapter must bind authored sequence aliases".into()]};
    pack.validate()?;
    for (path, bytes) in pending {
        let dst = out.join(path);
        fs::create_dir_all(dst.parent().unwrap())?;
        fs::write(dst, bytes)?;
    }
    fs::write(out.join("vehicles.json"), serde_json::to_vec_pretty(&pack)?)?;
    pack.verify_assets(out)?;
    println!(
        "Converted {} definitions, {} assets, {} source datablocks",
        pack.definitions.len(),
        pack.assets.len(),
        pack.evidence.len()
    );
    Ok(())
}
