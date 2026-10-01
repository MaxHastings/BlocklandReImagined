//! Offline typing of imported wrench events and spawn-brick attachments.
//!
//! `+-EVENT` records become `bri_events` rows checked against the native event
//! catalog; `+-VEHICLE` and `+-AUDIOEMITTER` become the brick's vehicle and
//! music. Datablock names are bound to native content IDs through [`Aliases`].
//! Rows that cannot be typed stay as preserved (never executed) rows, and every
//! original source record is kept unchanged.
use anyhow::{Context, Result, bail, ensure};
use bri_events::{Catalog, Param, PreservedRow, Row, Slot, Target, Value};
use bri_world::{ContentRef, VehicleSpawn, World};
use serde::Serialize;
use std::collections::BTreeMap;

/// Datablock class -> lowercase source name (datablock or UI name) -> native ID.
#[derive(Debug, Default, Clone)]
pub struct Aliases(BTreeMap<String, BTreeMap<String, String>>);
impl Aliases {
    /// Register a native ID under each of its source names. A name claimed by
    /// two different IDs of one class is dropped rather than guessed.
    pub fn insert(&mut self, class: &str, names: &[&str], id: &str) {
        let entries = self.0.entry(class.to_ascii_lowercase()).or_default();
        for name in names {
            let name = name.trim().to_ascii_lowercase();
            if name.is_empty() {
                continue;
            }
            match entries.get(&name) {
                Some(existing) if existing != id => {
                    entries.insert(name, String::new());
                }
                _ => {
                    entries.insert(name, id.into());
                }
            }
        }
    }
    pub fn resolve(&self, class: &str, name: &str) -> Option<&str> {
        self.0
            .get(&class.to_ascii_lowercase())?
            .get(&name.trim().to_ascii_lowercase())
            .map(String::as_str)
            .filter(|id| !id.is_empty())
    }
    /// Build aliases from the converted native content packs.
    pub fn from_packs(
        audio_manifest: &serde_json::Value,
        weapons: &serde_json::Value,
        vehicles: &serde_json::Value,
        effects: &bri_content::effects::Library,
    ) -> Result<Self> {
        let mut aliases = Self::default();
        let text = |v: &serde_json::Value, key: &str| v[key].as_str().unwrap_or("").to_string();
        for sound in audio_manifest["sounds"]
            .as_array()
            .context("Audio manifest has no sounds")?
        {
            let id = text(sound, "id");
            let names = [text(sound, "name"), text(sound, "ui_name")];
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            let lists = sound["lists"].as_array().cloned().unwrap_or_default();
            let listed = |l: &str| lists.iter().any(|x| x.as_str() == Some(l));
            if listed("event-param:Sound") {
                aliases.insert("Sound", &names, &id);
            }
            if listed("event-param:Music") {
                aliases.insert("Music", &names, &id);
            }
        }
        for (class, key) in [("ItemData", "items"), ("ProjectileData", "projectiles")] {
            for (id, entry) in weapons[key]
                .as_object()
                .with_context(|| format!("Weapons pack has no {key}"))?
            {
                let names = [text(entry, "name"), text(entry, "ui_name")];
                aliases.insert(class, &[&names[0], &names[1]], id);
            }
        }
        for vehicle in vehicles["definitions"]
            .as_array()
            .context("Vehicle pack has no definitions")?
        {
            let names = [text(vehicle, "datablock"), text(vehicle, "name")];
            aliases.insert("Vehicle", &[&names[0], &names[1]], &text(vehicle, "id"));
        }
        for light in &effects.lights {
            let datablock = light.id.rsplit('/').next().unwrap_or_default();
            aliases.insert("FxLightData", &[datablock, &light.name], &light.id);
        }
        for emitter in &effects.emitters {
            let datablock = emitter.id.rsplit('/').next().unwrap_or_default();
            aliases.insert(
                "ParticleEmitterData",
                &[datablock, &emitter.name],
                &emitter.id,
            );
        }
        aliases.insert(
            "PlayerData",
            &["PlayerStandardArmor"],
            "PlayerStandardArmor",
        );
        Ok(aliases)
    }
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub rows: usize,
    pub runnable: usize,
    pub preserved: BTreeMap<String, usize>,
    pub vehicles: usize,
    pub music: usize,
    pub unresolved_attachments: BTreeMap<String, usize>,
}

fn truncate(text: &str, bytes: usize) -> String {
    let mut end = text.len().min(bytes);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].into()
}
fn preserved(original: &str, diagnostic: &str) -> Row {
    Row {
        preserved: Some(PreservedRow {
            original: truncate(original, 2048),
            diagnostic: truncate(diagnostic, 1024),
        }),
        enabled: false,
        input: String::new(),
        delay_ms: 0,
        target: Target::Slot(Slot::SelfBrick),
        output: String::new(),
        params: vec![],
    }
}

fn value(param: &Param, text: &str, aliases: &Aliases) -> Result<Value> {
    let text = text.trim();
    Ok(match param {
        Param::Int { min, max, default } => {
            let n = if text.is_empty() {
                *default
            } else {
                text.parse()?
            };
            ensure!(
                (*min..=*max).contains(&n),
                "Integer {n} outside {min}..{max}"
            );
            Value::Int(n)
        }
        Param::List { items } => {
            let n = if text.is_empty() {
                items.first().map_or(0, |i| i.1)
            } else {
                text.parse()?
            };
            ensure!(items.iter().any(|i| i.1 == n), "List value {n} not offered");
            Value::Int(n)
        }
        Param::Float {
            min,
            max,
            step,
            default,
        } => {
            let n: f32 = if text.is_empty() {
                *default
            } else {
                text.parse()?
            };
            ensure!(
                n.is_finite() && (*min..=*max).contains(&n),
                "Float {n} out of range"
            );
            Value::Float(min + ((n - min) / step + 1e-5).floor() * step)
        }
        Param::Bool => match text {
            "" | "0" => Value::Bool(false),
            "1" => Value::Bool(true),
            _ => bail!("Expected 0 or 1, got {text:?}"),
        },
        Param::String { max_length, .. } => {
            ensure!(
                text.chars().count() <= *max_length as usize && !text.contains('\0'),
                "Text too long"
            );
            Value::Text(text.into())
        }
        Param::PaintColor { default } => Value::Color(if text.is_empty() {
            *default
        } else {
            text.parse()?
        }),
        Param::IntList { .. } => Value::Rows(bri_events::migration::row_selection(text)?),
        Param::Vector { max_length } => {
            let v: Vec<f32> = if text.is_empty() {
                vec![0.0; 3]
            } else {
                text.split_whitespace()
                    .map(str::parse)
                    .collect::<std::result::Result<_, _>>()?
            };
            ensure!(v.len() == 3, "Vector needs three components");
            // Torque Z-up to native Y-up.
            let v = glam::Vec3::new(v[0], v[2], -v[1]);
            ensure!(
                v.is_finite() && v.length() <= max_length + 1e-4,
                "Vector too long"
            );
            Value::Vector(v)
        }
        Param::Datablock { class_name } => {
            if matches!(text, "" | "-1" | "0") {
                Value::Datablock(None)
            } else {
                let id = aliases
                    .resolve(class_name, text)
                    .with_context(|| format!("Unresolved {class_name} {text}"))?;
                Value::Datablock(Some(id.into()))
            }
        }
    })
}

/// Type one `+-EVENT` source line.
pub fn row(line: &str, catalog: &Catalog, aliases: &Aliases) -> Result<(u16, Row)> {
    let f: Vec<&str> = line.trim_end_matches(['\r', '\n']).split('\t').collect();
    ensure!(
        f.len() == 12 && f[0] == "+-EVENT",
        "Expected 12 event fields"
    );
    let index: u16 = f[1].parse().context("Invalid event row index")?;
    ensure!(index < 4096, "Event row index too large");
    let enabled = match f[2] {
        "0" => false,
        "1" => true,
        _ => bail!("Invalid enabled flag"),
    };
    let input = catalog
        .input(f[3])
        .with_context(|| format!("Unknown input {}", f[3]))?;
    let delay_ms: u32 = f[4].parse().context("Invalid delay")?;
    ensure!(delay_ms <= 300_000, "Delay exceeds five minutes");
    let (target, class) = if f[5] == "-1" || f[5].eq_ignore_ascii_case("<NAMED BRICK>") {
        ensure!(!f[6].trim().is_empty(), "Named target without a name");
        (Target::Named(f[6].trim().into()), "fxDTSBrick".to_string())
    } else {
        let (slot, class) = input
            .targets
            .iter()
            .find(|(slot, _)| slot.eq_ignore_ascii_case(f[5]))
            .with_context(|| format!("Target {} unavailable for {}", f[5], input.name))?;
        (
            Target::Slot(Slot::parse(slot).context("Unknown target slot")?),
            class.clone(),
        )
    };
    let output = catalog
        .output(
            bri_events::Class::parse(&class).context("Unknown target class")?,
            f[7],
        )
        .with_context(|| format!("Unknown output {} on {class}", f[7]))?;
    ensure!(
        f[8 + output.params.len()..].iter().all(|s| s.is_empty()),
        "Unexpected extra parameters"
    );
    let params = output
        .params
        .iter()
        .zip(&f[8..])
        .map(|(param, text)| value(param, text, aliases))
        .collect::<Result<Vec<_>>>()?;
    Ok((
        index,
        Row {
            preserved: None,
            enabled,
            input: input.name.clone(),
            delay_ms,
            target,
            output: output.name.clone(),
            params,
        },
    ))
}

fn attachment_name(line: &str, tag: &str) -> Option<String> {
    let rest = line.strip_prefix(tag)?.trim_start();
    let (name, _) = rest.split_once('"')?;
    Some(name.trim().to_string()).filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case("NONE"))
}

/// Replace every brick's event rows with the typed rows of its `+-EVENT`
/// records, and bind `+-VEHICLE` / `+-AUDIOEMITTER` attachments.
pub fn bind(world: &mut World, catalog: &Catalog, aliases: &Aliases) -> Result<Report> {
    let palette_len = world.palette.len();
    let bindings = bri_events::Bindings {
        palette_len,
        datablocks: aliases
            .0
            .iter()
            .map(|(class, names)| {
                (
                    class.clone(),
                    names
                        .values()
                        .filter(|id| !id.is_empty())
                        .cloned()
                        .collect(),
                )
            })
            .collect(),
    };
    let mut report = Report::default();
    for id in world.bricks.keys().copied().collect::<Vec<_>>() {
        let Some(brick) = world.bricks.get_mut(&id) else {
            continue;
        };
        let mut rows = BTreeMap::new();
        for record in &brick.source_records {
            let text = record.text.as_str();
            if text.starts_with("+-EVENT\t") {
                let index = text.split('\t').nth(1).and_then(|i| i.parse::<u16>().ok());
                let typed = row(text, catalog, aliases).and_then(|(index, row)| {
                    catalog.validate_row(&row, &bindings)?;
                    Ok((index, row))
                });
                // Rows past the native per-brick limit are left out; the
                // original text stays in the brick's source records.
                let limit = bri_world::MAX_EVENTS_PER_BRICK;
                match (typed, index) {
                    (Ok((index, _)), _) | (Err(_), Some(index)) if usize::from(index) >= limit => {
                        *report
                            .preserved
                            .entry(format!("Row past the {limit}-row limit"))
                            .or_default() += 1;
                    }
                    (Ok((index, row)), _) => {
                        rows.insert(index, row);
                    }
                    (Err(error), Some(index)) => {
                        let reason = format!("{error:#}");
                        *report.preserved.entry(reason.clone()).or_default() += 1;
                        rows.insert(index, preserved(text, &reason));
                    }
                    (Err(error), _) => {
                        *report.preserved.entry(format!("{error:#}")).or_default() += 1;
                    }
                }
            } else if let Some(name) = attachment_name(text, "+-VEHICLE") {
                let recolor = text.trim_end().ends_with('1');
                match aliases.resolve("Vehicle", &name) {
                    Some(id) => {
                        brick.vehicle = Some(VehicleSpawn {
                            vehicle: ContentRef::Resolved(id.into()),
                            recolor,
                        });
                        report.vehicles += 1;
                    }
                    None => {
                        *report
                            .unresolved_attachments
                            .entry(format!("Vehicle:{name}"))
                            .or_default() += 1
                    }
                }
            } else if let Some(name) = attachment_name(text, "+-AUDIOEMITTER") {
                match aliases.resolve("Music", &name) {
                    Some(id) => {
                        brick.sound = Some(ContentRef::Resolved(id.into()));
                        report.music += 1;
                    }
                    None => {
                        *report
                            .unresolved_attachments
                            .entry(format!("Music:{name}"))
                            .or_default() += 1
                    }
                }
            }
        }
        // Row indices address rows (setEventEnabled 0 2...): keep gaps.
        let count = rows
            .keys()
            .next_back()
            .map_or(0, |last| usize::from(*last) + 1);
        brick.events = (0..count)
            .map(|i| {
                rows.remove(&(i as u16))
                    .unwrap_or_else(|| preserved("", "Missing original row index"))
            })
            .collect();
        report.rows += brick.events.len();
        report.runnable += brick
            .events
            .iter()
            .filter(|r| r.preserved.is_none())
            .count();
    }
    world.validate()?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_events::{InputDef, OutputDef};
    use bri_world::{Brick, SourceRecord};

    fn catalog() -> Catalog {
        Catalog {
            schema_version: 1,
            inputs: vec![InputDef {
                id: "in/activate".into(),
                class_name: "fxDTSBrick".into(),
                name: "onActivate".into(),
                targets: vec![
                    ("Self".into(), "fxDTSBrick".into()),
                    ("Player".into(), "Player".into()),
                ],
                source: "fixture".into(),
                source_line: 1,
            }],
            outputs: vec![
                OutputDef {
                    id: "out/light".into(),
                    class_name: "fxDTSBrick".into(),
                    name: "setLight".into(),
                    params: vec![Param::Datablock {
                        class_name: "FxLightData".into(),
                    }],
                    append_client: false,
                    source: "fixture".into(),
                    source_line: 1,
                    package: None,
                },
                OutputDef {
                    id: "out/velocity".into(),
                    class_name: "Player".into(),
                    name: "addVelocity".into(),
                    params: vec![Param::Vector { max_length: 200.0 }],
                    append_client: false,
                    source: "fixture".into(),
                    source_line: 1,
                    package: None,
                },
            ],
            targets: vec![],
            sources: vec![],
            scope: serde_json::Value::Null,
        }
    }
    #[test]
    fn rows_past_the_native_limit_are_left_out_not_the_save() {
        let mut world = World::new("t".into(), "m".into(), vec![[1.0; 4]]);
        let mut brick = Brick::new(ContentRef::Resolved("b".into()), [0.0; 3], 0);
        for (line, text) in [
            (
                1,
                "+-EVENT\t0\t1\tonActivate\t0\tPlayer\t\taddVelocity\t0 0 5\t\t\t",
            ),
            (
                2,
                "+-EVENT\t4000\t1\tonActivate\t0\tPlayer\t\taddVelocity\t0 0 5\t\t\t",
            ),
            (3, "+-EVENT\t2000\t1\tonMissing\t0\tSelf\t\tnothing\t\t\t\t"),
        ] {
            brick.source_records.push(SourceRecord {
                line,
                text: text.into(),
                diagnostic: None,
            });
        }
        world.bricks.insert(1, brick);
        world.next_brick_id = 2;
        let report = bind(&mut world, &catalog(), &Aliases::default()).unwrap();
        assert_eq!(world.bricks[&1].events.len(), 1);
        assert_eq!(report.runnable, 1);
        assert_eq!(
            report.preserved.get(&format!(
                "Row past the {}-row limit",
                bri_world::MAX_EVENTS_PER_BRICK
            )),
            Some(&2)
        );
        assert_eq!(world.bricks[&1].source_records.len(), 3);
    }
    #[test]
    fn types_rows_keeps_indices_and_preserves_what_it_cannot_bind() {
        let mut aliases = Aliases::default();
        aliases.insert(
            "FxLightData",
            &["RedLight", "Red Light"],
            "v20/light/redlight",
        );
        aliases.insert(
            "Vehicle",
            &["JeepVehicle", "Jeep"],
            "v20.vehicle.jeepvehicle",
        );
        let mut world = World::new("t".into(), "m".into(), vec![[1.0; 4]]);
        let mut brick = Brick::new(ContentRef::Resolved("b".into()), [0.0; 3], 0);
        for (line, text) in [
            (
                1,
                "+-EVENT\t0\t1\tonActivate\t10\tSelf\t\tsetLight\tRedLight\t\t\t",
            ),
            (
                2,
                "+-EVENT\t2\t0\tonActivate\t0\tPlayer\t\taddVelocity\t0 0 5\t\t\t",
            ),
            (
                3,
                "+-EVENT\t3\t1\tonActivate\t0\tSelf\t\tsetLight\tCommunityLight\t\t\t",
            ),
            (4, "+-VEHICLE Jeep\" 1"),
        ] {
            brick.source_records.push(SourceRecord {
                line,
                text: text.into(),
                diagnostic: None,
            });
        }
        world.bricks.insert(1, brick);
        world.next_brick_id = 2;
        let records = world.bricks[&1].source_records.clone();
        let report = bind(&mut world, &catalog(), &aliases).unwrap();
        let b = &world.bricks[&1];
        assert_eq!(b.events.len(), 4);
        assert_eq!(
            b.events[0].params,
            vec![Value::Datablock(Some("v20/light/redlight".into()))]
        );
        assert!(b.events[1].preserved.is_some(), "missing index 1 is a gap");
        assert!(!b.events[2].enabled);
        assert_eq!(
            b.events[2].params,
            vec![Value::Vector(glam::Vec3::new(0.0, 5.0, 0.0))]
        );
        assert!(b.events[3].preserved.is_some());
        assert_eq!(report.runnable, 2);
        assert_eq!(report.vehicles, 1);
        assert!(b.vehicle.as_ref().unwrap().recolor);
        assert_eq!(b.source_records, records);
        assert_eq!(
            bri_world::persistence::decode(&serde_json::to_vec(&world).unwrap()).unwrap(),
            world
        );
    }
}
