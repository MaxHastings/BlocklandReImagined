//! What an Add-On's weapons draw and play: its own particle emitters (image
//! states, projectile trails), explosions (their emitters, burst and light)
//! and sounds, converted into its weapons pack. Vehicles share the emitter
//! conversion.
use super::*;
use bri_content::effects::{Curve, Emitter, Light, Particle};
use bri_convert::{effect_script::Declaration, effects};

/// A datablock this import can read: the Add-On's own, or else the
/// reference install's (a base game particle an Add-On emitter uses).
fn declaration(cx: &Ctx, name: &str) -> Option<(Declaration, bool)> {
    let fields = |f: &BTreeMap<String, String>| {
        f.iter()
            .map(|(k, v)| (k.clone(), literal(v).trim().to_owned()))
            .collect()
    };
    if let Some(o) = cx.owned.get(&name.to_ascii_lowercase()) {
        return Some((
            Declaration {
                class: o.d.class.clone(),
                name: o.d.name.clone(),
                source: o.path.clone(),
                fields: fields(&o.fields),
            },
            true,
        ));
    }
    let r = cx.reference.datablocks.get(&name.to_ascii_lowercase())?;
    Some((
        Declaration {
            class: r.datablock.class.clone(),
            name: r.datablock.name.clone(),
            source: r.path.clone(),
            fields: fields(&r.datablock.fields),
        },
        false,
    ))
}

/// The key of the Add-On's own image `reference` (without or with its
/// extension), as its item presentation lists textures.
pub(crate) fn own_texture(cx: &Ctx, reference: &str) -> Option<String> {
    ["", ".png", ".jpg", ".jpeg"].iter().find_map(|ext| {
        let key = format!("{reference}{ext}").to_ascii_lowercase();
        (cx.src.get(&key).is_some() && cx.outputs.contains_key(&key)).then_some(key)
    })
}

/// A datablock v20 itself refused to load, so the import leaving it out
/// is faithful.
#[derive(Debug)]
pub(crate) struct NeverLoaded(pub String);

impl std::fmt::Display for NeverLoaded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NeverLoaded {}

/// Convert the Add-On's emitter `name` and the particles it uses into its
/// namespace, for the pack written to `file`. A base game particle it uses
/// is copied in. Particles must draw base game textures.
pub(crate) fn convert_emitter(
    cx: &mut Ctx,
    name: &str,
    file: &str,
) -> Result<(Emitter, Vec<Particle>)> {
    let (d, own) = declaration(cx, name).with_context(|| format!("no emitter {name}"))?;
    ensure!(
        own && d.class.eq_ignore_ascii_case("ParticleEmitterData"),
        "{} is a {}, not an emitter of this Add-On",
        d.name,
        d.class
    );
    // The emitter nodes it names (`GenericEmitterNode`, the base game's)
    // only time it when it is placed on a brick.
    let mut nodes = BTreeMap::new();
    for (key, node) in &d.fields {
        if !["emitternode", "pointemitternode"].contains(&key.to_ascii_lowercase().as_str())
            || node.is_empty()
        {
            continue;
        }
        let multiple = match declaration(cx, node) {
            Some((n, _)) if n.class.eq_ignore_ascii_case("ParticleEmitterNodeData") => {
                effects::Fields::new(&n).ratio("timemultiple", 1.0)?
            }
            _ => {
                cx.report.diagnostics.push(format!(
                    "emitter {}: emitter node {node} is not declared, so on a brick it runs at its own pace",
                    d.name
                ));
                1.0
            }
        };
        nodes.insert(node.to_lowercase(), multiple);
    }
    let (mut emitter, notes) =
        effects::emitter(&d, &nodes).with_context(|| format!("emitter {}", d.name))?;
    let emitter_name = d.name.clone();
    // Torque's `ParticleEmitterData::onAdd` skips a particle name nothing
    // declares, and refuses an emitter left with none: it never existed.
    let named = std::mem::take(&mut emitter.particles);
    for p in named {
        let particle = p.strip_prefix("v20/particle/").unwrap_or(&p);
        if declaration(cx, particle).is_some() {
            emitter.particles.push(p);
        } else {
            cx.report.diagnostics.push(format!(
                "emitter {emitter_name}: particle {particle} is declared nowhere; Torque skipped it too"
            ));
        }
    }
    if emitter.particles.is_empty() {
        return Err(anyhow::Error::new(NeverLoaded(format!(
            "emitter {emitter_name} names no particle anything declares, so v20 refused it when it loaded and nothing ever drew it"
        ))));
    }
    emitter.id = cx.id("emitter", name, &emitter_name, file);
    let mut particles = vec![];
    for p in &emitter.particles {
        let particle = p.strip_prefix("v20/particle/").unwrap_or(p).to_owned();
        let (pd, own) = declaration(cx, &particle).with_context(|| {
            format!("emitter {emitter_name} uses {particle}, which neither the Add-On nor the base game declares")
        })?;
        let (mut converted, more) =
            effects::particle(&pd).with_context(|| format!("particle {}", pd.name))?;
        // A base game texture comes from the effects library; the Add-On's
        // own is named by its converted file's key, which its item
        // presentation lists and the client loads.
        if !converted.texture.starts_with("base/") {
            let Some(texture) = own_texture(cx, &converted.texture).filter(|_| own) else {
                // A texture missing from the Add-On itself was missing in
                // v20 too: the particle never drew.
                ensure!(
                    own,
                    "particle {} draws {}, which this Add-On does not have",
                    pd.name,
                    converted.texture
                );
                cx.mark(
                    &particle,
                    "particle",
                    "consumed",
                    vec![],
                    Some(format!(
                        "it draws {}, which the Add-On does not have, so v20 could not draw it either",
                        converted.texture
                    )),
                );
                continue;
            };
            converted.texture = texture;
        }
        converted.id = if own {
            cx.id("particle", &particle, &pd.name, file)
        } else {
            content_id(&cx.ns, "particle", &particle)
        };
        for note in more {
            cx.report
                .diagnostics
                .push(format!("particle {}: {note}", pd.name));
        }
        if own {
            cx.mark(
                &particle,
                "particle",
                "converted",
                vec![converted.id.clone()],
                None,
            );
        }
        particles.push(converted);
    }
    if particles.is_empty() {
        return Err(anyhow::Error::new(NeverLoaded(format!(
            "emitter {emitter_name} draws only particles whose textures the Add-On does not have, so v20 drew nothing"
        ))));
    }
    for note in notes {
        cx.report
            .diagnostics
            .push(format!("emitter {emitter_name}: {note}"));
    }
    emitter.particles = particles.iter().map(|p| p.id.clone()).collect();
    cx.mark(name, "emitter", "converted", vec![emitter.id.clone()], None);
    Ok((emitter, particles))
}

/// The Add-On's emitters its images, projectiles and explosions name, and
/// its explosions' effects, into `pack.effects`; its sounds into
/// `pack.sounds`. References to its own emitters and sounds are rewritten to
/// their ids; the base game's keep their names.
pub(crate) fn weapon_effects(cx: &mut Ctx, pack: &mut bri_weapons::Pack) {
    let file = "assets/weapons.json";
    let own = |cx: &Ctx, name: &str, class: &str| {
        cx.owned
            .get(&name.to_ascii_lowercase())
            .is_some_and(|o| o.d.class.eq_ignore_ascii_case(class))
    };
    // Emitters by lower-case Torque name to id, converted once each.
    let mut emitters: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut emitter = |cx: &mut Ctx, pack: &mut bri_weapons::Pack, name: &str| -> Option<String> {
        let key = name.to_ascii_lowercase();
        if let Some(id) = emitters.get(&key) {
            return id.clone();
        }
        let id = match convert_emitter(cx, name, file) {
            Ok((e, particles)) => {
                for p in particles {
                    if !pack.effects.particles.iter().any(|q| q.id == p.id) {
                        pack.effects.particles.push(p);
                    }
                }
                let id = e.id.clone();
                pack.effects.emitters.push(e);
                Some(id)
            }
            Err(error) if error.downcast_ref::<NeverLoaded>().is_some() => {
                cx.mark(name, "emitter", "consumed", vec![], Some(format!("{error}")));
                None
            }
            Err(error) => {
                cx.unsupported(
                    format!("emitter {name}"),
                    None,
                    format!("{error:#}; the effect is left out"),
                );
                None
            }
        };
        emitters.insert(key, id.clone());
        id
    };
    let names: Vec<(String, usize, String)> = pack
        .images
        .iter()
        .flat_map(|(id, im)| {
            im.states
                .iter()
                .enumerate()
                .filter(|(_, s)| !s.emitter.is_empty())
                .map(|(i, s)| (id.clone(), i, s.emitter.clone()))
        })
        .collect();
    for (image, state, name) in names {
        if own(cx, &name, "ParticleEmitterData")
            && let Some(id) = emitter(cx, pack, &name)
        {
            pack.images.get_mut(&image).expect("listed").states[state].emitter = id;
        }
    }
    let trails: Vec<(String, String)> = pack
        .projectiles
        .iter()
        .filter(|(_, p)| !p.trail.is_empty())
        .map(|(id, p)| (id.clone(), p.trail.clone()))
        .collect();
    for (projectile, name) in trails {
        if own(cx, &name, "ParticleEmitterData")
            && let Some(id) = emitter(cx, pack, &name)
        {
            pack.projectiles.get_mut(&projectile).expect("listed").trail = id;
        }
    }
    let explosions: Vec<String> = pack.explosions.values().map(|e| e.name.clone()).collect();
    for name in explosions {
        let Some((d, true)) = declaration(cx, &name) else {
            continue;
        };
        let number = |key: &str, default: f32| {
            d.fields
                .get(key)
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| v.is_finite())
                .unwrap_or(default)
        };
        // A missing emitter is the base game's, if it has one by that name.
        let mut named = |cx: &mut Ctx, pack: &mut bri_weapons::Pack, v: &str| {
            if own(cx, v, "ParticleEmitterData") {
                emitter(cx, pack, v)
            } else {
                cx.reference
                    .datablocks
                    .contains_key(&v.to_ascii_lowercase())
                    .then(|| format!("v20/emitter/{}", v.to_ascii_lowercase()))
            }
        };
        let lifetime = (number("lifetimems", 1000.0) / 1000.0).clamp(0.001, 3600.0);
        let mut effect = bri_weapons::ExplosionEffect {
            id: content_id(&cx.ns, "explosion", &d.name),
            lifetime,
            emitters: vec![],
            light: None,
            burst: None,
        };
        // Whether every emitter it names was drawn, or was one v20 itself
        // never loaded.
        let mut complete = true;
        let mut settle = |cx: &mut Ctx, v: &str, id: &Option<String>| {
            complete &= id.is_some()
                || cx
                    .report
                    .datablocks
                    .iter()
                    .any(|e| e.name.eq_ignore_ascii_case(v) && e.status == "consumed");
        };
        for i in 0..4 {
            if let Some(v) = d
                .fields
                .get(&format!("emitter[{i}]"))
                .filter(|v| !v.is_empty())
                .cloned()
            {
                let id = named(cx, pack, &v);
                settle(cx, &v, &id);
                effect.emitters.extend(id);
            }
        }
        if let Some(v) = d
            .fields
            .get("particleemitter")
            .filter(|v| !v.is_empty())
            .cloned()
        {
            let id = named(cx, pack, &v);
            settle(cx, &v, &id);
            if let Some(id) = id {
                let count = number("particledensity", 10.0).clamp(0.0, 32768.0) as u32;
                effect.burst = Some((id, count, number("particleradius", 1.0).max(0.0)));
            }
        }
        if complete && let Some(e) = cx.entry(&d.name) {
            // Its emitters, burst, light, sound and debris all have their
            // native parts.
            e.status = "converted".into();
            e.notes.retain(|n| !n.starts_with("debris and particle parts"));
        }
        let (start, end) = (
            number("lightstartradius", 0.0).max(0.0),
            number("lightendradius", 0.0).max(0.0),
        );
        if start > 0.0 || end > 0.0 {
            let color = |key: &str| -> [f32; 3] {
                let v: Vec<f32> = d
                    .fields
                    .get(key)
                    .map(|s| {
                        s.split_whitespace()
                            .filter_map(|x| x.parse().ok())
                            .collect()
                    })
                    .unwrap_or_default();
                if v.len() >= 3 && v.iter().all(|c| c.is_finite() && *c >= 0.0) {
                    [v[0], v[1], v[2]]
                } else {
                    [1.0; 3]
                }
            };
            let (from, to) = (color("lightstartcolor"), color("lightendcolor"));
            let id = content_id(&cx.ns, "explosion-light", &d.name);
            pack.effects.lights.push(Light {
                id: id.clone(),
                name: String::new(),
                enabled: true,
                color: from,
                brightness: 1.0,
                radius: start,
                color_curves: Some(std::array::from_fn(|i| Curve {
                    period: lifetime,
                    linear: true,
                    values: vec![from[i], to[i]],
                })),
                brightness_curve: None,
                radius_curve: Some(Curve {
                    period: lifetime,
                    linear: true,
                    values: vec![start, end],
                }),
                flare: None,
            });
            effect.light = Some(id);
        }
        pack.effects.explosions.push(effect);
    }
    // The trails of the debris its explosions and casings throw: its own
    // emitters convert, and the pieces find them by name
    // (`bri_weapons::debris`); others are the base game's.
    let trails: BTreeSet<String> = bri_weapons::debris::explosion_debris(pack)
        .into_values()
        .chain(
            bri_weapons::debris::casings(pack)
                .into_values()
                .map(|c| c.debris),
        )
        .flat_map(|d| d.emitters)
        .map(|id| bri_weapons::effect_symbol(&id).to_owned())
        .collect();
    for name in trails {
        if own(cx, &name, "ParticleEmitterData") {
            emitter(cx, pack, &name);
        }
    }
    // An emitter with a uiName is one players put on bricks (the wrench's
    // emitter list), whether or not anything else uses it.
    let mut offered: Vec<String> = cx
        .owned
        .values()
        .filter(|o| o.d.class.eq_ignore_ascii_case("ParticleEmitterData"))
        .filter(|o| {
            o.fields
                .get("uiname")
                .is_some_and(|n| !literal(n).trim().is_empty())
        })
        .map(|o| o.d.name.clone())
        .collect();
    offered.sort();
    for name in offered {
        if emitter(cx, pack, &name).is_some()
            && let Some(e) = cx.entry(&name)
        {
            e.notes.push("offered for bricks by its uiName".into());
        }
    }
    sounds(cx, pack);
}

/// Each of the Add-On's `AudioProfile`s whose file converted, keyed by its
/// id; image states, projectiles and explosions naming one play it. Volume
/// and looping come from its `AudioDescription`; a sound that is not 3D is
/// heard by its holder alone.
fn sounds(cx: &mut Ctx, pack: &mut bri_weapons::Pack) {
    let profiles: Vec<(String, String, String, String)> = cx
        .owned
        .values()
        .filter(|o| o.d.class.eq_ignore_ascii_case("AudioProfile"))
        .filter_map(|o| {
            let f = o
                .fields
                .get("filename")
                .map(|f| crate::file_named(cx.src, &cx.outputs, &o.path, f))?;
            let rel = cx.outputs.get(&f.to_ascii_lowercase())?.clone();
            let description = o
                .fields
                .get("description")
                .map(|d| literal(d).trim().to_owned())
                .unwrap_or_default();
            Some((o.d.name.clone(), rel, description, o.path.clone()))
        })
        .collect();
    let mut keys = BTreeMap::new();
    for (name, rel, description, _) in profiles {
        let fields = cx
            .owned
            .get(&description.to_ascii_lowercase())
            .map(|o| o.fields.clone())
            .or_else(|| {
                cx.reference
                    .datablocks
                    .get(&description.to_ascii_lowercase())
                    .map(|r| r.datablock.fields.clone())
            })
            .unwrap_or_default();
        let get = |k: &str| {
            fields
                .get(k)
                .map(|v| literal(v).trim().to_ascii_lowercase())
        };
        let flag = |k: &str, default: bool| get(k).map_or(default, |v| v == "1" || v == "true");
        let volume = get("volume")
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .map_or(1.0, |v| v.clamp(0.0, 1.0));
        // Converted files sit under `assets/`, beside `weapons.json`.
        let lower = rel.to_ascii_lowercase();
        if !(lower.ends_with(".wav") || lower.ends_with(".ogg")) {
            continue;
        }
        let id = cx.id("sound", &name, &name, &format!("assets/{rel}"));
        pack.sounds.insert(
            id.clone(),
            bri_weapons::SoundDef {
                file: rel,
                volume,
                looping: flag("islooping", false),
                local: !flag("is3d", true),
                package: None,
            },
        );
        cx.mark(&name, "sound", "converted", vec![id.clone()], None);
        keys.insert(name.to_ascii_lowercase(), id);
    }
    let own = |s: &mut String| {
        if let Some(id) = keys.get(&s.to_ascii_lowercase()) {
            s.clone_from(id);
        }
    };
    for image in pack.images.values_mut() {
        for state in &mut image.states {
            own(&mut state.sound);
        }
    }
    for p in pack.projectiles.values_mut() {
        own(&mut p.sound);
    }
    for e in pack.explosions.values_mut() {
        own(&mut e.sound);
    }
}
