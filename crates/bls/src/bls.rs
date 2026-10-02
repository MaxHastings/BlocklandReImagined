//! BLS import. Original extension text is preserved, never executed.
use anyhow::{Context, Result, bail, ensure};
use bri_content::brick::Catalog;
use bri_world::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

fn reference(namespace: &str, name: &str) -> ContentRef {
    ContentRef::unresolved(namespace, name.trim())
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
            brick.light = Some(Box::new(Light {
                asset: reference("light_ui", name),
                enabled,
            }));
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
            brick.emitter = Some(Box::new(Emitter {
                asset: (!name.trim().eq_ignore_ascii_case("NONE"))
                    .then(|| reference("emitter_ui", name)),
                direction,
            }));
            Ok(Some(
                "Emitter state preserved; content reference requires resolution".into(),
            ))
        }
        _ => bail!("Unsupported extension {tag}; original record retained"),
    }
}
pub fn read(bytes: &[u8], catalog: &Catalog, name: &str, map_id: &str) -> Result<World> {
    Ok(read_counting(bytes, catalog, name, map_id)?.0)
}
/// Brick lines a read left out, by reason.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub reasons: BTreeMap<String, usize>,
}
impl Skipped {
    pub fn lines(&self) -> usize {
        self.reasons.values().sum()
    }
    fn add(&mut self, reason: impl Into<String>) {
        *self.reasons.entry(reason.into()).or_default() += 1;
    }
}
impl std::fmt::Display for Skipped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, (reason, count)) in self.reasons.iter().enumerate() {
            write!(f, "{}{count} {reason}", if i > 0 { ", " } else { "" })?;
        }
        Ok(())
    }
}
/// Windows-1252, the code page v20 wrote its text in, for 0x80..0xA0.
/// The five bytes it leaves undefined keep their Latin-1 control code.
const CP1252_HIGH: [char; 32] = [
    '\u{20ac}', '\u{0081}', '\u{201a}', '\u{0192}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02c6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008d}', '\u{017d}', '\u{008f}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02dc}', '\u{2122}', '\u{0161}', '\u{203a}', '\u{0153}', '\u{009d}', '\u{017e}', '\u{0178}',
];
fn windows_1252(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            0x80..0xa0 => CP1252_HIGH[usize::from(b - 0x80)],
            _ => char::from(b),
        })
        .collect()
}
/// Torque's `getWord` of a number: an empty or unreadable word is 0.
fn integer(word: Option<&&str>) -> i64 {
    word.and_then(|w| w.trim().parse::<f64>().ok())
        .filter(|n| n.is_finite())
        .map_or(0, |n| n.trunc() as i64)
}
/// Torque's `dAtob`: "true" or a nonzero number. A word older saves did
/// not write yet takes the brick's default.
fn flag(word: Option<&&str>, default: bool) -> bool {
    match word.map(|w| w.trim()) {
        None | Some("") => default,
        Some(w) if w.eq_ignore_ascii_case("true") => true,
        Some(w) => w.parse::<f64>().is_ok_and(|n| n != 0.0),
    }
}
/// One colorset line. Unreadable components are 0 (alpha 1); a line
/// written in 0..255 is scaled into 0..1.
fn palette_color(line: &str) -> [f32; 4] {
    let mut color = [0.0, 0.0, 0.0, 1.0];
    for (slot, word) in color.iter_mut().zip(line.split_whitespace()) {
        *slot = word
            .parse::<f32>()
            .ok()
            .filter(|n| n.is_finite())
            .unwrap_or(0.0);
    }
    if color.iter().any(|c| *c > 1.0) {
        color = color.map(|c| c / 255.0);
    }
    color.map(|c| c.clamp(0.0, 1.0))
}
/// [`read`], and which brick lines were skipped. As v20's
/// `ServerLoadSaveFile_Tick` does, a brick line that cannot be read is
/// skipped with the extension lines under it, and the rest of the save
/// still loads; its `Linecount` is only a progress hint. Fields are read
/// the way its `getWord` reads them: words older saves did not write yet
/// take their defaults, and words after the twelfth are ignored.
pub fn read_counting(
    bytes: &[u8],
    catalog: &Catalog,
    name: &str,
    map_id: &str,
) -> Result<(World, Skipped)> {
    read_with_header(bytes, catalog, name, map_id, |l| {
        l.starts_with("This is a Blockland save file.")
    })
}
/// The first lines v20 duplicators wrote their saved selections under,
/// in the save file layout: Plornt's Duplorcator (positions relative to
/// the first brick) and Zeblote's New Duplicator (where they stood).
const DUPLICATION_HEADERS: [&str; 3] = [
    "This is a Blockland save file.",
    "Duplorcation save file",
    "Do not modify this file at all.",
];
/// A v20 duplication file (`saves/Duplications/*.bls`,
/// `config/NewDuplicator/Saves/*.bls`) or save, read as [`read_counting`]
/// reads a save. Its bricks may stand off the grid.
pub fn read_duplication(bytes: &[u8], catalog: &Catalog, name: &str) -> Result<(World, Skipped)> {
    read_with_header(bytes, catalog, name, "duplication", |l| {
        DUPLICATION_HEADERS.iter().any(|h| l.starts_with(h))
    })
}
fn read_with_header(
    bytes: &[u8],
    catalog: &Catalog,
    name: &str,
    map_id: &str,
    header: impl Fn(&str) -> bool,
) -> Result<(World, Skipped)> {
    ensure!(bytes.len() <= 128 * 1024 * 1024, "Oversized BLS input");
    let (text, encoding) = match std::str::from_utf8(bytes) {
        Ok(text) => (std::borrow::Cow::Borrowed(text), "utf8"),
        // Without 0x80..0xA0 the two code pages agree; the stock saves'
        // degree signs keep the name Latin-1 their conversions carry.
        Err(_) => (
            std::borrow::Cow::Owned(windows_1252(bytes)),
            if bytes.iter().any(|b| (0x80..0xa0).contains(b)) {
                "windows-1252"
            } else {
                "latin1"
            },
        ),
    };
    let lines: Vec<_> = text.lines().collect();
    ensure!(lines.iter().all(|l| l.len() <= 65536), "Oversized BLS line");
    ensure!(
        lines.first().is_some_and(|l| header(l)),
        "Unrecognized BLS header"
    );
    let description_count: usize = lines
        .get(1)
        .context("Missing description count")?
        .trim()
        .parse()
        .context("Unreadable description count")?;
    ensure!(
        description_count <= 4096 && lines.len() >= description_count + 66,
        "Truncated BLS header/palette"
    );
    let mut at = 2 + description_count;
    let palette = lines[at..at + 64]
        .iter()
        .map(|l| palette_color(l))
        .collect();
    at += 64;
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
    let palette_len = world.palette.len();
    let mut last = None;
    let mut skipped = Skipped::default();
    for (index, line) in lines.iter().enumerate().skip(at) {
        // `Linecount` usually follows the colorset, but is only a hint.
        if line.is_empty() || line.starts_with("Linecount") {
            continue;
        }
        if line.starts_with("+-") {
            // Under a skipped brick (or before the first), as in v20.
            let Some(brick) = last.and_then(|id| world.bricks.get_mut(&id)) else {
                continue;
            };
            if brick.source_records.len() >= 4096 {
                continue;
            }
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
        let brick = (|| -> Result<Brick, &'static str> {
            let (display, fields) = line.split_once('"').ok_or("without a brick name")?;
            ensure_or(!display.trim().is_empty(), "without a brick name")?;
            // Words are single-space separated: an empty print is an
            // empty word between the colour and the effects.
            let words: Vec<_> = fields.trim_start_matches(' ').split(' ').collect();
            let mut position = [0.0; 3];
            for (axis, word) in position.iter_mut().zip(&words) {
                *axis = number(word).map_err(|_| "with an unreadable position")?;
            }
            ensure_or(words.len() >= 3, "with an unreadable position")?;
            let [x, y, z] = position;
            let definition = names
                .get(&display.to_lowercase())
                .map(|id| ContentRef::Resolved(id.clone()))
                .unwrap_or_else(|| reference("brick_ui", display));
            let mut diagnostics = vec![];
            if matches!(definition, ContentRef::Unresolved(_)) {
                diagnostics.push(format!(
                    "Brick definition missing from supplied catalog: {display}"
                ));
            }
            if words.len() < 12 {
                diagnostics.push(format!(
                    "Older save format: {} of 12 brick fields; the rest take their defaults",
                    words.len()
                ));
            }
            let mut brick = Brick::new(definition, [x, z, -y], 0);
            brick.quarter_turns = integer(words.get(3)).rem_euclid(4) as u8;
            brick.base_plate = flag(words.get(4), false);
            // A colour or effect this save's version did not have is the
            // default one.
            brick.color = u8::try_from(integer(words.get(5)))
                .ok()
                .filter(|c| usize::from(*c) < palette_len)
                .unwrap_or(0);
            brick.print = words
                .get(6)
                .filter(|p| !p.is_empty() && **p != "/")
                .map(|p| reference("print", p));
            brick.color_effect = u8::try_from(integer(words.get(7)))
                .ok()
                .filter(|c| *c <= 6)
                .unwrap_or(0);
            brick.shape_effect = u8::try_from(integer(words.get(8)))
                .ok()
                .filter(|c| *c <= 2)
                .unwrap_or(0);
            brick.raycast = flag(words.get(9), true);
            brick.colliding = flag(words.get(10), true);
            brick.visible = flag(words.get(11), true);
            if brick.print.is_some() {
                diagnostics.push("Print reference retained; texture binding pending".into());
            }
            brick.source_records.push(SourceRecord {
                line: (index + 1) as u32,
                text: (*line).into(),
                diagnostic: (!diagnostics.is_empty()).then(|| diagnostics.join("; ")),
            });
            brick
                .validate(palette_len)
                .map_err(|_| "that are not valid bricks")?;
            Ok(brick)
        })();
        let brick = match brick {
            Ok(brick) => brick,
            Err(reason) => {
                skipped.add(format!("lines {reason}"));
                last = None;
                continue;
            }
        };
        if world.bricks.len() >= MAX_BRICKS {
            skipped.add(format!("bricks over the {MAX_BRICKS}-brick limit"));
            last = None;
            continue;
        }
        let id = world.next_brick_id;
        world.bricks.insert(id, brick);
        world.next_brick_id += 1;
        last = Some(id);
    }
    world.validate()?;
    Ok((world, skipped))
}
fn ensure_or(condition: bool, reason: &'static str) -> Result<(), &'static str> {
    if condition { Ok(()) } else { Err(reason) }
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
        // Linecount is only v20's progress hint.
        let (hinted, skipped) = read_counting(
            source.replace("Linecount 1", "Linecount 2").as_bytes(),
            &catalog,
            "test",
            "map/test",
        )
        .unwrap();
        assert_eq!((hinted.bricks.len(), skipped.lines()), (1, 0));
        // A line v20 cannot plant is skipped with its extensions; the next
        // brick still loads.
        let broken = source.replace("1 2 3 1 0 2", "NaN 2 3 1 0 2")
            + "Missing Brick\" 5 2 3 1 0 2  0 0 1 0 1
+-CUSTOM kept
";
        let (world, skipped) =
            read_counting(broken.as_bytes(), &catalog, "test", "map/test").unwrap();
        assert_eq!((world.bricks.len(), skipped.lines()), (1, 1));
        let brick = world.bricks.values().next().unwrap();
        assert_eq!(brick.position[0], 5.0);
        assert_eq!(
            brick.source_records.len(),
            2,
            "Its own line and extension only"
        );
    }
    fn header(description: &str, linecount: bool) -> String {
        format!(
            "This is a Blockland save file.  You probably shouldn't modify it cause you'll screw it up.\n1\n{description}\n{}{}",
            "0.5 0.25 0 1\n".repeat(64),
            if linecount { "Linecount 3\n" } else { "" }
        )
    }
    fn stock() -> Catalog {
        let entry = |name: &str| bri_content::brick::CatalogEntry {
            id: format!("stock/{}", name.to_lowercase()),
            display_name: name.into(),
            category: "Bricks".into(),
            subcategory: "Basic".into(),
            mesh_id: String::new(),
            collision_source: None,
            icon_source: String::new(),
            print_aspect_ratio: None,
            orientation_fix: 0,
            can_cover: false,
            indestructible: false,
            special_kind: None,
            other_properties: Default::default(),
            reflection: None,
            link: None,
            stretch: None,
            bot: None,
        };
        Catalog {
            schema_version: 1,
            bricks: vec![entry("2x2 Brick"), entry("1x2 Plate")],
        }
    }
    #[test]
    fn custom_bricks_are_kept_aside_and_every_stock_brick_still_loads() {
        let source = header("Mixed", true)
            + "2x2 Brick\" 0 0 0.3 0 1 5  0 0 1 1 1\n+-OWNER 12345\n"
            // An Add-On datablock without isBasePlate writes an empty word.
            + "Custom Wedge 4x\" 1 1 0.5 1  3 Letters/A 0 0 1 1 1\n+-NTOBJECTNAME _door\n"
            + "1x2 Plate\" 0.25 0.5 0.7 1 0 2  0 0 1 1 1\n";
        let (world, skipped) = read_counting(source.as_bytes(), &stock(), "t", "map/t").unwrap();
        assert_eq!(skipped, Skipped::default());
        let definitions: Vec<_> = world
            .bricks
            .values()
            .map(|b| b.definition.clone())
            .collect();
        assert_eq!(
            definitions,
            vec![
                ContentRef::Resolved("stock/2x2 brick".into()),
                reference("brick_ui", "Custom Wedge 4x"),
                ContentRef::Resolved("stock/1x2 plate".into()),
            ]
        );
        assert_eq!(world.bricks[&2].name.as_deref(), Some("_door"));
    }
    #[test]
    fn older_saves_with_fewer_fields_load_with_defaults() {
        // Saves from before events wrote no raycast/collision/rendering
        // flags; some wrote no effects either.
        let source = header("Old", true)
            + "2x2 Brick\" 0 0 0.3 2 0 5  1 0\n"
            + "2x2 Brick\" 5 0 0.3 0 0 6 \n"
            + "1x2 Plate\" 0.25 0.5 0.7 1 0 2  0 0 1 1 1 extra words\n";
        let (world, skipped) = read_counting(source.as_bytes(), &stock(), "t", "map/t").unwrap();
        assert_eq!(skipped.lines(), 0);
        assert_eq!(world.bricks.len(), 3);
        let old = &world.bricks[&1];
        assert_eq!((old.quarter_turns, old.color, old.color_effect), (2, 5, 1));
        assert!(old.raycast && old.colliding && old.visible);
        assert!(
            old.source_records[0]
                .diagnostic
                .as_deref()
                .unwrap()
                .contains("9 of 12 brick fields")
        );
        assert_eq!(world.bricks[&2].color, 6);
        assert!(world.bricks[&3].source_records[0].diagnostic.is_none());
    }
    #[test]
    fn a_missing_linecount_or_odd_values_do_not_empty_the_save() {
        let source = header("No count", false)
            // Colour, angle and effects this version did not have.
            + "2x2 Brick\" 0 0 0.3 7 true 200  9 5 1 0 1\n"
            + "Linecount 2\n"
            + "1x2 Plate\" 0.25 0.5 0.7 1 0 2  0 0 1 1 1\n";
        let (world, skipped) = read_counting(source.as_bytes(), &stock(), "t", "map/t").unwrap();
        assert_eq!((world.bricks.len(), skipped.lines()), (2, 0));
        let b = &world.bricks[&1];
        assert_eq!(
            (
                b.quarter_turns,
                b.base_plate,
                b.color,
                b.color_effect,
                b.shape_effect
            ),
            (3, true, 0, 0, 0)
        );
        assert!(b.raycast && !b.colliding && b.visible);
    }
    #[test]
    fn windows_1252_text_and_0_to_255_colorsets_are_read() {
        let mut source = header("Bob\u{2019}s house", true).replace(
            &"0.5 0.25 0 1\n".repeat(64),
            &format!("255 128 0 255\n{}", "0.5 0.25 0 1\n".repeat(63)),
        );
        source += "2x2 Brick\" 0 0 0.3 0 1 0  0 0 1 1 1\n";
        let bytes: Vec<u8> = source
            .chars()
            .map(|c| if c == '\u{2019}' { 0x92 } else { c as u8 })
            .collect();
        let world = read(&bytes, &stock(), "t", "map/t").unwrap();
        assert_eq!(world.source_encoding.as_deref(), Some("windows-1252"));
        assert_eq!(world.description, vec!["Bob\u{2019}s house"]);
        assert_eq!(world.palette[0], [1.0, 128.0 / 255.0, 0.0, 1.0]);
        assert_eq!(world.palette[1], [0.5, 0.25, 0.0, 1.0]);
        assert_eq!(world.bricks.len(), 1);
    }
    #[test]
    fn unreadable_lines_are_skipped_and_counted_by_reason() {
        let source = header("Broken", true)
            + "2x2 Brick\" 0 0 0.3 0 1 0  0 0 1 1 1\n"
            + "no delimiter here\n+-OWNER 1\n"
            + "2x2 Brick\" x 0 0.3 0 1 0  0 0 1 1 1\n"
            + "2x2 Brick\" 1 2\n"
            + "\" 1 2 3 0 1 0  0 0 1 1 1\n";
        let (world, skipped) = read_counting(source.as_bytes(), &stock(), "t", "map/t").unwrap();
        assert_eq!(world.bricks.len(), 1);
        assert_eq!(
            world.bricks[&1].source_records.len(),
            1,
            "The skipped line's owner is not moved"
        );
        assert_eq!(
            skipped.to_string(),
            "2 lines with an unreadable position, 2 lines without a brick name"
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

    #[test]
    fn duplication_files_of_both_v20_duplicators_read_but_are_not_saves() {
        let body = "0.5 0.25 0 1\n".repeat(64)
            + "Linecount 2\n"
            + "2x2 Brick\" 0 0 0 0 0 5  0 0 1 1 1\n"
            + "1x2 Plate\" 0.25 0.5 0.4 1 0 2  0 0 1 1 1\n";
        for first in [
            "Duplorcation save file\t2\n1\nDuplication saved by Plornt\n",
            "Do not modify this file at all. You will break it.\n1\nSaved by Zeblote (4928)\n",
        ] {
            let source = format!("{first}{body}");
            let (world, skipped) = read_duplication(source.as_bytes(), &stock(), "dup").unwrap();
            assert_eq!(skipped, Skipped::default());
            assert_eq!(world.bricks.len(), 2);
            assert!(read_counting(source.as_bytes(), &stock(), "dup", "map/t").is_err());
        }
    }
}
