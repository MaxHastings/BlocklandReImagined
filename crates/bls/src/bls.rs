//! BLS import. Original extension text is preserved, never executed.
use anyhow::{Context, Result, bail, ensure};
use bri_content::brick::Catalog;
use bri_world::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

fn reference(namespace: &str, name: &str) -> ContentRef {
    ContentRef::Unresolved {
        namespace: namespace.into(),
        name: name.trim().into(),
    }
}
fn boolean(s: &str) -> Result<bool> {
    match s.trim() {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => bail!("Expected 0 or 1, got {s:?}"),
    }
}
fn number(s: &str) -> Result<f32> {
    let n: f32 = s.parse()?;
    ensure!(n.is_finite(), "Nonfinite number");
    Ok(n)
}
/// Diagnostic on `+-EVENT` records until `events::bind` types them.
pub const EVENT_PENDING: &str = "Wrench event row awaiting catalog binding";
fn attachment<'a>(line: &'a str, tag: &str) -> Result<(&'a str, &'a str)> {
    line.strip_prefix(tag)
        .context("Missing attachment tag")?
        .trim_start()
        .split_once('"')
        .context("Attachment is missing its name delimiter")
}
fn extension(brick: &mut Brick, line: &str) -> Result<Option<String>> {
    let tag = line.split_whitespace().next().context("Empty extension")?;
    match tag {
        // Rows are typed against the event catalog by `events::bind`.
        "+-EVENT" => Ok(Some(EVENT_PENDING.into())),
        "+-NTOBJECTNAME" => {
            let name = line.strip_prefix(tag).unwrap().trim();
            ensure!(!name.is_empty() && name.len() <= 128, "Invalid brick name");
            brick.name = Some(name.into());
            Ok(None)
        }
        "+-OWNER" => {
            let _: u64 = line.strip_prefix(tag).unwrap().trim().parse()?;
            Ok(Some(
                "Original owner retained as metadata; native ownership remains world-owned".into(),
            ))
        }
        "+-ITEM" => {
            let (name, tail) = attachment(line, tag)?;
            ensure!(!name.trim().is_empty(), "Empty item name");
            let fields: Vec<_> = tail.split_whitespace().collect();
            ensure!(
                fields.len() == 3,
                "Item requires position, direction and respawn milliseconds"
            );
            let position: i64 = fields[0].parse().context("Invalid item position")?;
            let direction: i64 = fields[1].parse().context("Invalid item direction")?;
            let respawn: i64 = fields[2]
                .parse()
                .context("Invalid item respawn milliseconds")?;
            // The original direction/time setters clamp; an invalid position
            // leaves the brick's preceding position unchanged. Preserve raw text.
            let normalized = !(0..=5).contains(&position)
                || !(2..=5).contains(&direction)
                || !(1000..=300000).contains(&respawn);
            let item_spawn = ItemSpawn {
                item: (!name.trim().eq_ignore_ascii_case("NONE"))
                    .then(|| reference("item_ui", name)),
                position: if (0..=5).contains(&position) {
                    position as u8
                } else {
                    brick.item_spawn.position
                },
                direction: direction.clamp(2, 5) as u8,
                respawn_ms: respawn.clamp(1000, 300000) as u32,
            };
            item_spawn.validate()?;
            let unresolved = item_spawn.item.is_some();
            brick.item_spawn = item_spawn;
            Ok(match (unresolved, normalized) {
                (true,true) => Some("Item state adapted with original setter clamps; content reference requires resolution".into()),
                (true,false) => Some("Item state preserved; content reference requires resolution".into()),
                (false,true) => Some("Item NONE selectors adapted with original setter clamps".into()),
                (false,false) => None,
            })
        }
        "+-LIGHT" => {
            let (name, tail) = attachment(line, tag)?;
            ensure!(!name.trim().is_empty(), "Empty light name");
            let enabled = if tail.trim().is_empty() {
                true
            } else {
                boolean(tail)?
            };
            brick.light = Some(Light {
                asset: reference("light_ui", name),
                enabled,
            });
            Ok(Some(
                "Light state preserved; content reference requires resolution".into(),
            ))
        }
        "+-EMITTER" => {
            let (name, tail) = attachment(line, tag)?;
            let direction: u8 = tail.trim().parse()?;
            ensure!(
                direction <= 5 && !name.trim().is_empty(),
                "Invalid emitter attachment"
            );
            brick.emitter = Some(Emitter {
                asset: (!name.trim().eq_ignore_ascii_case("NONE"))
                    .then(|| reference("emitter_ui", name)),
                direction,
            });
            Ok(Some(
                "Emitter state preserved; content reference requires resolution".into(),
            ))
        }
        _ => bail!("Unsupported extension {tag}; original record retained"),
    }
}
pub fn read(bytes: &[u8], catalog: &Catalog, name: &str, map_id: &str) -> Result<World> {
    ensure!(bytes.len() <= 128 * 1024 * 1024, "Oversized BLS input");
    let (text, encoding) = match std::str::from_utf8(bytes) {
        Ok(text) => (std::borrow::Cow::Borrowed(text), "utf8"),
        Err(_) => {
            // Stock v20 saves use an eight-bit degree sign in ramp UI names.
            // Do not guess a code page for ambiguous C1 bytes.
            ensure!(
                !bytes.iter().any(|b| (0x80..0xa0).contains(b)),
                "BLS has ambiguous eight-bit encoding; explicit code-page adaptation needed"
            );
            (
                std::borrow::Cow::Owned(bytes.iter().map(|b| char::from(*b)).collect::<String>()),
                "latin1",
            )
        }
    };
    let lines: Vec<_> = text.lines().collect();
    ensure!(lines.iter().all(|l| l.len() <= 65536), "Oversized BLS line");
    ensure!(
        lines
            .first()
            .is_some_and(|l| l.starts_with("This is a Blockland save file.")),
        "Unrecognized BLS header"
    );
    let description_count: usize = lines.get(1).context("Missing description count")?.parse()?;
    ensure!(
        description_count <= 4096 && lines.len() >= description_count + 67,
        "Truncated BLS header/palette"
    );
    let mut at = 2 + description_count;
    let mut palette = Vec::new();
    for line in &lines[at..at + 64] {
        let values = line
            .split_whitespace()
            .map(number)
            .collect::<Result<Vec<_>>>()?;
        ensure!(values.len() == 4, "Palette entry needs four components");
        palette.push(values.try_into().unwrap());
    }
    at += 64;
    let count_line: Vec<_> = lines[at].split_whitespace().collect();
    ensure!(
        count_line.len() == 2 && count_line[0] == "Linecount",
        "Missing BLS brick count"
    );
    let expected: usize = count_line[1].parse()?;
    ensure!(expected <= MAX_BRICKS, "BLS brick count too large");
    at += 1;
    let mut names = BTreeMap::new();
    for b in &catalog.bricks {
        // v20 builds this lookup in declaration order (allGameScripts:2608).
        // Treasure Chest deliberately declares its closed state last.
        if !b.display_name.is_empty() {
            names.insert(b.display_name.to_lowercase(), b.id.clone());
        }
    }
    let mut world = World::new(name.into(), map_id.into(), palette);
    world.description = lines[2..2 + description_count]
        .iter()
        .map(|s| (*s).into())
        .collect();
    world.source_sha256 = Some(format!("{:x}", Sha256::digest(bytes)));
    world.source_encoding = Some(encoding.into());
    let mut last = None;
    for (index, line) in lines.iter().enumerate().skip(at) {
        if line.is_empty() {
            continue;
        }
        if line.starts_with("+-") {
            let brick = world
                .bricks
                .get_mut(&last.context("Extension before first brick")?)
                .unwrap();
            ensure!(
                brick.source_records.len() < 4096,
                "Too many extensions on a brick"
            );
            let diagnostic = match extension(brick, line) {
                Ok(d) => d,
                Err(e) => Some(e.to_string()),
            };
            brick.source_records.push(SourceRecord {
                line: (index + 1) as u32,
                text: (*line).into(),
                diagnostic,
            });
            continue;
        }
        let (display, fields) = line
            .split_once('"')
            .with_context(|| format!("Brick delimiter missing on line {}", index + 1))?;
        // Empty print is a meaningful empty word between the color and effects.
        let fields: Vec<_> = fields.trim_start_matches(' ').split(' ').collect();
        ensure!(
            fields.len() == 12,
            "Expected 12 brick fields on line {}, found {}",
            index + 1,
            fields.len()
        );
        let x = number(fields[0])?;
        let y = number(fields[1])?;
        let z = number(fields[2])?;
        let definition = names
            .get(&display.to_lowercase())
            .map(|id| ContentRef::Resolved(id.clone()))
            .unwrap_or_else(|| reference("brick_ui", display));
        let diagnostic = matches!(definition, ContentRef::Unresolved { .. })
            .then(|| format!("Brick definition missing from supplied catalog: {display}"));
        let mut brick = Brick::new(definition, [x, z, -y], 0);
        brick.quarter_turns = fields[3].parse()?;
        brick.base_plate = boolean(fields[4])?;
        brick.color = fields[5].parse()?;
        brick.print =
            (!fields[6].is_empty() && fields[6] != "/").then(|| reference("print", fields[6]));
        brick.color_effect = fields[7].parse()?;
        brick.shape_effect = fields[8].parse()?;
        brick.raycast = boolean(fields[9])?;
        brick.colliding = boolean(fields[10])?;
        brick.visible = boolean(fields[11])?;
        brick.source_records.push(SourceRecord {
            line: (index + 1) as u32,
            text: (*line).into(),
            diagnostic,
        });
        if brick.print.is_some() {
            brick.source_records[0].diagnostic = Some(format!(
                "{}Print reference retained; texture binding pending",
                brick.source_records[0]
                    .diagnostic
                    .as_ref()
                    .map_or(String::new(), |s| format!("{s}; "))
            ));
        }
        brick
            .validate(world.palette.len())
            .with_context(|| format!("Invalid brick on line {}", index + 1))?;
        let id = world.next_brick_id;
        world.bricks.insert(id, brick);
        world.next_brick_id += 1;
        last = Some(id);
        ensure!(world.bricks.len() <= expected, "More bricks than declared");
    }
    ensure!(
        world.bricks.len() == expected,
        "BLS brick count mismatch: declared {expected}, read {}",
        world.bricks.len()
    );
    world.validate()?;
    Ok(world)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> String {
        format!(
            "This is a Blockland save file.\n1\nDescription\n{}Linecount 1\nMissing Brick\" 1 2 3 1 0 2  0 0 1 0 1\n+-OWNER 1234\n+-EVENT\t0\t1\tonActivate\t15\tSelf\t\tsetRendering\t0\t\t\t\n+-EVENT\t1\t1\tonRelay\t0\tSelf\t\tfireRelay\t\t\t\t\n+-CUSTOM keep exactly\n",
            "1 0.5 0 1\n".repeat(64)
        )
    }
    #[test]
    fn preserves_missing_content_extensions_and_ownership_without_executing() {
        let source = fixture();
        let catalog = Catalog {
            schema_version: 1,
            bricks: vec![],
        };
        let world = read(source.as_bytes(), &catalog, "test", "map/test").unwrap();
        let b = &world.bricks[&1];
        assert_eq!(b.position, [1.0, 3.0, -2.0]);
        assert_eq!(b.owner, 0);
        assert_eq!(b.quarter_turns, 1);
        assert!(!b.colliding);
        assert!(b.events.is_empty());
        assert_eq!(
            b.source_records
                .iter()
                .filter(|r| r.diagnostic.as_deref() == Some(EVENT_PENDING))
                .count(),
            2
        );
        assert_eq!(b.source_records.len(), 5);
        assert_eq!(b.source_records[4].text, "+-CUSTOM keep exactly");
        assert!(
            b.transform()
                .transform_vector3(glam::Vec3::X)
                .distance(glam::Vec3::Z)
                < 1e-6
        );
        let saved = serde_json::to_vec(&world).unwrap();
        assert_eq!(bri_world::persistence::decode(&saved).unwrap(), world);
        assert!(
            read(
                source.replace("Linecount 1", "Linecount 2").as_bytes(),
                &catalog,
                "test",
                "map/test"
            )
            .is_err()
        );
        assert!(
            read(
                source.replace("1 2 3 1 0 2", "NaN 2 3 1 0 2").as_bytes(),
                &catalog,
                "test",
                "map/test"
            )
            .is_err()
        );
    }
    #[test]
    fn single_byte_degree_names_are_not_replaced_or_lost() {
        let source = fixture().replace("Missing Brick", "25° Ramp 4x");
        let bytes: Vec<_> = source.chars().map(|c| c as u8).collect();
        let world = read(
            &bytes,
            &Catalog {
                schema_version: 1,
                bricks: vec![],
            },
            "test",
            "map/test",
        )
        .unwrap();
        assert_eq!(world.source_encoding.as_deref(), Some("latin1"));
        assert!(
            world.bricks[&1].source_records[0]
                .text
                .starts_with("25° Ramp 4x\"")
        );
    }
    #[test]
    fn item_attachments_preserve_selection_and_none_state_through_native_save() {
        let catalog = Catalog {
            schema_version: 1,
            bricks: vec![],
        };
        for (line, expected) in [
            (
                "+-ITEM Gun\" 4 3 12000",
                ItemSpawn {
                    item: Some(reference("item_ui", "Gun")),
                    position: 4,
                    direction: 3,
                    respawn_ms: 12000,
                },
            ),
            (
                "+-ITEM NONE\" 1 5 300000",
                ItemSpawn {
                    item: None,
                    position: 1,
                    direction: 5,
                    respawn_ms: 300000,
                },
            ),
            (
                "+-ITEM Hammer \" 0 2 4000",
                ItemSpawn {
                    item: Some(reference("item_ui", "Hammer")),
                    ..ItemSpawn::default()
                },
            ),
        ] {
            let source = format!("{}{line}\n", fixture());
            let world = read(source.as_bytes(), &catalog, "test", "map/test").unwrap();
            assert_eq!(world.bricks[&1].item_spawn, expected);
            assert_eq!(world.bricks[&1].source_records.last().unwrap().text, line);
            assert_eq!(
                bri_world::persistence::decode(&serde_json::to_vec(&world).unwrap()).unwrap(),
                world
            );
        }
    }
    #[test]
    fn item_source_clamps_and_malformed_records_do_not_destroy_previous_state() {
        let source = format!(
            "{}+-ITEM Gun\" 5 4 10000\n+-ITEM NONE\" 99 0 -20\n+-ITEM Broken\" 0 nonsense 4000\n+-ITEM Extra\" 0 2 4000 arbitrary\n",
            fixture()
        );
        let world = read(
            source.as_bytes(),
            &Catalog {
                schema_version: 1,
                bricks: vec![],
            },
            "test",
            "map/test",
        )
        .unwrap();
        assert_eq!(
            world.bricks[&1].item_spawn,
            ItemSpawn {
                item: None,
                position: 5,
                direction: 2,
                respawn_ms: 1000
            }
        );
        let records = &world.bricks[&1].source_records;
        assert!(
            records[records.len() - 3]
                .diagnostic
                .as_ref()
                .unwrap()
                .contains("clamps")
        );
        assert!(
            records[records.len() - 2]
                .diagnostic
                .as_ref()
                .unwrap()
                .contains("direction")
        );
        assert_eq!(
            records.last().unwrap().text,
            "+-ITEM Extra\" 0 2 4000 arbitrary"
        );
        assert!(records.last().unwrap().diagnostic.is_some());
    }
}
