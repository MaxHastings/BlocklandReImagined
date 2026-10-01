//! An Add-On's own models, beside its `weapons.json`: a hand-written
//! weapons pack draws original art without Import Add-On.
//!
//! An item, image or projectile `model` named without an extension
//! (`models/rifle`) is the file `models/rifle.shape.json` in the folder
//! holding `weapons.json`, in the game's native shape format. As Torque
//! binds a shape's materials, each material `m` is the texture `m.png`
//! beside the model. The client draws it (`bri_client::items`) and
//! `bri-addon-check` checks it, both through [`read`].
use anyhow::{Context, Result, ensure};
use bri_content::{brick_materials::safe_relative, shape::Shape};
use std::path::Path;

/// Largest model file, its materials, and each texture's file and side.
pub const MAX_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_MATERIALS: usize = 16;
pub const MAX_TEXTURE_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_TEXTURE_SIDE: u32 = 1024;

/// A model read and checked, with what it needs.
pub struct OwnModel {
    /// The model's file, relative to the folder holding `weapons.json`.
    pub file: String,
    pub bytes: Vec<u8>,
    pub shape: Shape,
    /// Each material's texture file, in material order, relative as `file`.
    pub textures: Vec<String>,
    /// The box its geometry fills, in the game's axes.
    pub bounds: ([f32; 3], [f32; 3]),
}

/// The file a model `name` names: `<name>.shape.json`, or `None` for a
/// name that is not a safe relative path (a stock model's `.dts` path is
/// one, and simply has no such file).
pub fn file_of(name: &str) -> Option<String> {
    let file = format!("{}.shape.json", name.replace('\\', "/"));
    (!name.is_empty() && safe_relative(&file)).then_some(file)
}

/// `name`'s own model in `assets`: `Ok(None)` when there is no such file,
/// an error saying what is wrong with one that is there.
pub fn read(assets: &Path, name: &str) -> Result<Option<OwnModel>> {
    let Some(file) = file_of(name) else {
        return Ok(None);
    };
    let path = assets.join(&file);
    if !path.is_file() {
        return Ok(None);
    }
    let size = std::fs::metadata(&path)?.len();
    ensure!(size <= MAX_BYTES, "{file} is {size} bytes; at most {MAX_BYTES}");
    let bytes = std::fs::read(&path).with_context(|| format!("reading {file}"))?;
    let shape: Shape = serde_json::from_slice(&bytes).with_context(|| format!("{file} is not a shape"))?;
    shape.validate().with_context(|| format!("{file} is not a valid shape"))?;
    ensure!(
        shape.materials.len() <= MAX_MATERIALS,
        "{file} has {} materials; at most {MAX_MATERIALS}",
        shape.materials.len()
    );
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in shape.meshes.iter().flatten().flat_map(|m| &m.positions) {
        for axis in 0..3 {
            lo[axis] = lo[axis].min(p[axis]);
            hi[axis] = hi[axis].max(p[axis]);
        }
    }
    ensure!(lo[0] <= hi[0], "{file} has no geometry");
    let folder = file.rsplit_once('/').map_or("", |(folder, _)| folder);
    let mut textures = Vec::new();
    for material in &shape.materials {
        let texture = if folder.is_empty() {
            format!("{}.png", material.name)
        } else {
            format!("{folder}/{}.png", material.name)
        };
        ensure!(
            safe_relative(&texture) && assets.join(&texture).is_file(),
            "material {} of {file} needs its texture {texture} beside it",
            material.name
        );
        let size = std::fs::metadata(assets.join(&texture))?.len();
        ensure!(size <= MAX_TEXTURE_BYTES, "{texture} is {size} bytes; at most {MAX_TEXTURE_BYTES}");
        textures.push(texture);
    }
    Ok(Some(OwnModel { file, bytes, shape, textures, bounds: (lo, hi) }))
}
