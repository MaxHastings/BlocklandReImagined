use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use zip::ZipArchive;

#[derive(Serialize)]
struct Model {
    id: String,
    file: String,
    source: String,
    source_sha256: String,
    native_sha256: String,
    warnings: Vec<String>,
}
#[derive(Serialize)]
struct Texture {
    id: String,
    file: String,
    source: String,
    source_sha256: String,
    width: u32,
    height: u32,
}
#[derive(Serialize)]
struct SourceScript {
    archive: String,
    entry: String,
    sha256: String,
}
#[derive(Serialize)]
struct Pack {
    schema_version: u32,
    id: String,
    weapons_sha256: String,
    models: Vec<Model>,
    textures: Vec<Texture>,
    source_scripts: Vec<SourceScript>,
    shell: Shell,
}
#[derive(Serialize)]
struct Shell {
    model: String,
    lifetime_seconds: f32,
    min_spin_degrees_per_second: f32,
    max_spin_degrees_per_second: f32,
    elasticity: f32,
    friction: f32,
    bounces: u32,
    static_on_max_bounce: bool,
    snap_on_max_bounce: bool,
    fade: bool,
    gravity_multiplier: f32,
    exit_direction: [f32; 3],
    exit_offset: [f32; 3],
    exit_variance_degrees: f32,
    velocity: f32,
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn entry(zip: &mut ZipArchive<fs::File>, path: &str) -> Result<Vec<u8>> {
    let mut file = zip
        .by_name(path)
        .with_context(|| format!("missing archive member {path}"))?;
    ensure!(
        file.size() <= 16 * 1024 * 1024,
        "source model exceeds size bound"
    );
    let mut data = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut data)?;
    Ok(data)
}
fn convert(
    archive_path: &Path,
    member: &str,
    source: String,
    id: &str,
    out: &Path,
) -> Result<Model> {
    let mut archive = ZipArchive::new(fs::File::open(archive_path)?)?;
    let bytes = entry(&mut archive, member)?;
    let source_sha256 = sha(&bytes);
    let (shape, provenance) = bri_convert::shape::read_dts(&bytes, id.to_owned())?;
    shape.validate()?;
    let file = format!("models/{id}.json");
    let native = serde_json::to_vec(&shape)?;
    let native_sha256 = sha(&native);
    fs::write(out.join(&file), native)?;
    fs::write(
        out.join(format!("models/{id}.source.json")),
        serde_json::to_vec(&provenance)?,
    )?;
    Ok(Model {
        id: id.into(),
        file,
        source,
        source_sha256,
        native_sha256,
        warnings: provenance.warnings,
    })
}
fn script(archive_path: &Path, member: &str) -> Result<String> {
    let mut archive = ZipArchive::new(fs::File::open(archive_path)?)?;
    let bytes = entry(&mut archive, member)?;
    Ok(String::from_utf8(bytes)?)
}
fn source_script(archive_path: &Path, archive: &str, member: &str) -> Result<SourceScript> {
    let mut archive_file = ZipArchive::new(fs::File::open(archive_path)?)?;
    let bytes = entry(&mut archive_file, member)?;
    Ok(SourceScript {
        archive: archive.into(),
        entry: member.into(),
        sha256: sha(&bytes),
    })
}
fn copy_texture(
    archive_path: &Path,
    member: &str,
    source: String,
    id: &str,
    out: &Path,
) -> Result<Texture> {
    let mut archive = ZipArchive::new(fs::File::open(archive_path)?)?;
    let bytes = entry(&mut archive, member)?;
    let (width, height) = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()?
        .into_dimensions()?;
    ensure!(
        width > 0 && height > 0 && width <= 4096 && height <= 4096,
        "invalid source texture dimensions"
    );
    let file = format!("textures/{id}.png");
    fs::write(out.join(&file), &bytes)?;
    Ok(Texture {
        id: id.into(),
        file,
        source,
        source_sha256: sha(&bytes),
        width,
        height,
    })
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 3,
        "Usage: bri-weapon-debris-import <v20-root> <weapons.json> <new-output>"
    );
    let root = PathBuf::from(&args[0]).canonicalize()?;
    let weapons = PathBuf::from(&args[1]).canonicalize()?;
    let out = PathBuf::from(&args[2]);
    ensure!(
        !out.exists(),
        "output must be fresh; never overwrite a previous pack"
    );
    let weapon_sha = sha(&fs::read(&weapons)?);
    let gun = root.join("Add-Ons/Weapon_Gun.zip");
    let akimbo = root.join("Add-Ons/Weapon_Guns_Akimbo.zip");
    let rocket = root.join("Add-Ons/Weapon_Rocket_Launcher.zip");
    let gun_script = script(&gun, "server.cs")?;
    for exact in [
        "datablock DebrisData(gunShellDebris)",
        "lifetime = 2.0;",
        "minSpinSpeed = -400.0;",
        "maxSpinSpeed = 200.0;",
        "elasticity = 0.5;",
        "friction = 0.2;",
        "numBounces = 3;",
        "staticOnMaxBounce = true;",
        "snapOnMaxBounce = false;",
        "gravModifier = 2;",
        "shellExitDir        = \"1.0 -1.3 1.0\";",
        "shellExitVariance   = 15.0;",
        "shellVelocity       = 7.0;",
        "stateEjectShell[2]",
    ] {
        ensure!(
            gun_script.contains(exact),
            "primary source changed/missing expected field: {exact}"
        );
    }
    let gun_compact = gun_script
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    ensure!(
        gun_compact.contains("casing=gunShellDebris;")
            && gun_compact.matches("stateEjectShell[2]=true;").count() == 1,
        "Gun primary source must bind the casing and eject once on state 2"
    );
    let akimbo_script = script(&akimbo, "Weapon_AkimboGun.cs")?;
    let compact = |value: &str| {
        value
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    };
    let akimbo_compact = compact(&akimbo_script);
    for field in [
        "casing=gunShellDebris;",
        "shellExitDir=\"1.0-1.31.0\";",
        "shellExitOffset=\"000\";",
        "shellExitVariance=15.0;",
        "shellVelocity=7.0;",
    ] {
        ensure!(
            akimbo_compact.matches(field).count() == 2,
            "Akimbo primary source changed/missing expected field: {field}"
        );
    }
    ensure!(
        akimbo_compact.matches("stateEjectShell[2]=true;").count() == 2,
        "both Akimbo image states must author shell ejection"
    );
    let rocket_script = script(&rocket, "Weapon_Rocket Launcher.cs")?;
    ensure!(
        rocket_script
            .to_ascii_lowercase()
            .contains("explosionshape = \"./explosionsphere1.dts\""),
        "rocket explosion shape source reference missing"
    );
    fs::create_dir(&out)?;
    fs::create_dir(out.join("models"))?;
    fs::create_dir(out.join("textures"))?;
    let models = vec![
        convert(
            &gun,
            "gunshell.dts",
            "Add-Ons/Weapon_Gun.zip::gunshell.dts".into(),
            "v20.weapon_debris.gun_shell",
            &out,
        )?,
        convert(
            &rocket,
            "explosionsphere1.dts",
            "Add-Ons/Weapon_Rocket_Launcher.zip::explosionsphere1.dts".into(),
            "v20.weapon_debris.rocket_explosion_sphere",
            &out,
        )?,
    ];
    let shell = Shell {
        model: "v20.weapon_debris.gun_shell".into(),
        lifetime_seconds: 2.,
        min_spin_degrees_per_second: -400.,
        max_spin_degrees_per_second: 200.,
        elasticity: 0.5,
        friction: 0.2,
        bounces: 3,
        static_on_max_bounce: true,
        snap_on_max_bounce: false,
        fade: true,
        gravity_multiplier: 2.,
        // Torque source vector (Z-up) lowered into the project's X-right,
        // Y-up, -Z-forward native basis as [x,z,-y].
        exit_direction: [1., 1., 1.3],
        exit_offset: [0.; 3],
        exit_variance_degrees: 15.,
        velocity: 7.,
    };
    let textures = vec![
        copy_texture(
            &gun,
            "black50.png",
            "Add-Ons/Weapon_Gun.zip::black50.png".into(),
            "gun_black50",
            &out,
        )?,
        copy_texture(
            &gun,
            "yellow.png",
            "Add-Ons/Weapon_Gun.zip::yellow.png".into(),
            "gun_yellow",
            &out,
        )?,
    ];
    let source_scripts = vec![
        source_script(&gun, "Add-Ons/Weapon_Gun.zip", "server.cs")?,
        source_script(
            &akimbo,
            "Add-Ons/Weapon_Guns_Akimbo.zip",
            "Weapon_AkimboGun.cs",
        )?,
        source_script(
            &rocket,
            "Add-Ons/Weapon_Rocket_Launcher.zip",
            "Weapon_Rocket Launcher.cs",
        )?,
    ];
    let pack = Pack {
        schema_version: 1,
        id: "v20.weapon-debris.001".into(),
        weapons_sha256: weapon_sha,
        models,
        textures,
        source_scripts,
        shell,
    };
    fs::write(out.join("pack.json"), serde_json::to_vec_pretty(&pack)?)?;
    Ok(())
}
