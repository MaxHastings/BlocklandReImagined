//! Bakes the Projector actors a UE2 map places (the asset lane's `<map>-projectors.json`, package units, left-handed) into the engine's
//! decals ([`bri_content::decals`], native frame). This is the fallback for a game that does not attach its projectors itself: the
//! original attaches them from script at run time (`bri_effects_render::projector_runtime`), and both make the decal the same way.
//!
//! A turning projector (`Physics` rotating) bakes with a spin in degrees a second, since a baked decal cannot be moved by the game.
use crate::projector_props::{Ue2Projector, placement};
use bri_content::decals::DecalSet;
use bri_effects_render::projector_runtime::{MakeOptions, make_decal};
use serde_json::Value;

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        _ => None,
    }
}

/// A property as the actor sets it, else the class default.
fn prop(p: &Value, name: &str) -> Option<Value> {
    p["properties"]
        .get(name)
        .or_else(|| p["defaults"].get(name))
        .cloned()
}

fn triple(v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    Some([number(a.first()?)?, number(a.get(1)?)?, number(a.get(2)?)?])
}

/// The settings of one projector of the file.
pub fn settings_from_json(p: &Value) -> Ue2Projector {
    let mut s = Ue2Projector::default();
    let flag = |name: &str, slot: &mut bool| {
        if let Some(n) = prop(p, name).and_then(|v| number(&v)) {
            *slot = n != 0.0;
        }
    };
    s.texture = p["texture"].as_str().unwrap_or_default().into();
    let int = |name: &str, fallback: i64| {
        prop(p, name)
            .and_then(|v| number(&v))
            .map_or(fallback, |n| n as i64)
    };
    s.fov = int("FOV", 0) as i32;
    s.max_trace_distance = int("MaxTraceDistance", 0) as i32;
    s.draw_scale = p["draw_scale"].as_f64().unwrap_or(1.0) as f32;
    s.physics = int("Physics", 0) as i32;
    s.rotation_rate = prop(p, "RotationRate")
        .and_then(|v| triple(&v))
        .unwrap_or([0.0; 3])
        .map(|v| v as f32);
    flag("bProjectBSP", &mut s.project_bsp);
    flag("bProjectStaticMesh", &mut s.project_static_mesh);
    flag("bProjectTerrain", &mut s.project_terrain);
    flag("bProjectOnBackfaces", &mut s.project_on_backfaces);
    flag("bProjectOnParallelBSP", &mut s.project_on_parallel_bsp);
    s.blending_op = int("FrameBufferBlendingOp", 1).clamp(0, 255) as u8;
    s
}

/// What a picture is: its file name (relative to the decals file) and size.
pub struct Picture {
    pub file: String,
    pub width: u32,
    pub height: u32,
}

/// Bake every projector of `input` (the projectors file). `picture` finds a `ProjTexture` by its package path; a projector whose picture it
/// cannot find is left out and noted. `unit` is the native length of one package unit.
pub fn bake(input: &Value, unit: f32, picture: &dyn Fn(&str) -> Option<Picture>) -> DecalSet {
    let mut set = DecalSet::default();
    let projectors = input["projectors"].as_array().cloned().unwrap_or_default();
    let (mut mixed, mut other_spin) = (0, 0);
    for p in &projectors {
        let name = p["name"].as_str().unwrap_or("Projector");
        let settings = settings_from_json(p);
        if settings.texture.is_empty() {
            set.notes.push(format!("{name}: no ProjTexture; left out"));
            continue;
        }
        let Some(picture) = picture(&settings.texture) else {
            set.notes.push(format!(
                "{name}: picture {} is not available; left out",
                settings.texture
            ));
            continue;
        };
        let (Some(location), Some(rotation)) = (triple(&p["location"]), triple(&p["rotation"]))
        else {
            set.notes
                .push(format!("{name}: no location or rotation; left out"));
            continue;
        };
        let size = (picture.width, picture.height);
        let Some(decal) = make_decal(
            &settings.spec(size, unit),
            &placement(location.map(|v| v as f32), rotation.map(|v| v as i32), unit),
            size,
            &picture.file,
            MakeOptions {
                bake_spin: true,
                widen_for_turn: true,
            },
        ) else {
            set.notes.push(format!("{name}: no picture size; left out"));
            continue;
        };
        mixed += usize::from(settings.project_bsp != settings.project_static_mesh);
        other_spin += usize::from(
            settings.rotating()
                && (settings.rotation_rate[0] != 0.0 || settings.rotation_rate[1] != 0.0),
        );
        set.decals.push(decal);
    }
    if mixed > 0 {
        set.notes.push(format!(
            "{mixed} projectors take BSP and static meshes differently; the map's walls and meshes are one receiver class here, so both are taken or neither"
        ));
    }
    if other_spin > 0 {
        set.notes.push(format!(
            "{other_spin} projectors turn about pitch or yaw; only the roll turn is applied"
        ));
    }
    set.notes.push(
        "projectors with bProjectActor/bProjectParticles/bDynamicAttach also draw on actors and particles in the original; only the map's surfaces take them here".into(),
    );
    set
}
