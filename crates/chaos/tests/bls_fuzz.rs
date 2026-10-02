//! Old `.bls` saves as players bring them: bricks this game has and
//! Add-On bricks it lacks, lines older versions wrote shorter, odd values
//! and eight-bit names, among lines that cannot be read. A save never
//! comes out empty because of some of its lines: every readable brick
//! loads, and only the unreadable lines are skipped and counted.
use bri_content::brick::{Catalog, CatalogEntry};
use proptest::prelude::*;

fn catalog() -> Catalog {
    let entry = |name: &str| CatalogEntry {
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
        swap: None,
    };
    Catalog {
        schema_version: 1,
        bricks: ["2x2 Brick", "1x2 Plate", "1x1 Cone"]
            .into_iter()
            .map(entry)
            .collect(),
    }
}

#[derive(Debug, Clone)]
enum Line {
    /// A brick line, cut to this many fields (3..=12) or given extras.
    Brick { name: u8, fields: usize, odd: bool },
    /// An unreadable line: no delimiter, or an unreadable position.
    Broken(bool),
    /// An extension line under the brick before it.
    Extension(u8),
}

fn line() -> impl Strategy<Value = Line> {
    prop_oneof![
        6 => (0..6u8, 3..=14usize, any::<bool>())
            .prop_map(|(name, fields, odd)| Line::Brick { name, fields, odd }),
        1 => any::<bool>().prop_map(Line::Broken),
        2 => (0..5u8).prop_map(Line::Extension),
    ]
}

fn render(lines: &[Line], linecount: bool) -> (Vec<u8>, usize, usize, usize) {
    let mut out = b"This is a Blockland save file.  You probably shouldn't modify it cause you'll screw it up.\n1\nA save\n".to_vec();
    for i in 0..64 {
        out.extend(format!("{} 0.5 0.25 1\n", i as f32 / 64.0).bytes());
    }
    if linecount {
        out.extend(format!("Linecount {}\n", lines.len()).bytes());
    }
    let (mut bricks, mut stock, mut broken) = (0, 0, 0);
    for (i, line) in lines.iter().enumerate() {
        match line {
            Line::Brick { name, fields, odd } => {
                let name: &[u8] = match name {
                    0 => b"2x2 Brick",
                    1 => b"1x2 Plate",
                    2 => b"1x1 Cone",
                    3 => b"Custom Arch 1x6",
                    // Eight-bit Add-On names, as v20 wrote them.
                    4 => b"Caf\xe9 Table",
                    _ => b"\x93Fancy\x94 Brick",
                };
                let values = if *odd {
                    ["7", "true", "200", "", "9", "5", "", "2", "1"]
                } else {
                    ["1", "0", "5", "", "0", "0", "1", "1", "1"]
                };
                let mut words = vec![format!("{}", i as f32 * 0.5), "0".into(), "0.3".into()];
                words.extend(values.iter().map(|v| v.to_string()));
                words.extend(["extra".to_string(), "words".to_string()]);
                words.truncate(*fields);
                out.extend(name);
                out.extend(b"\" ");
                out.extend(words.join(" ").bytes());
                out.push(b'\n');
                bricks += 1;
                stock += usize::from(name.is_ascii() && !name.starts_with(b"Custom"));
            }
            Line::Broken(delimiter) => {
                out.extend(if *delimiter {
                    b"2x2 Brick\" x 0 0.3 0 1 5  0 0 1 1 1\n".as_slice()
                } else {
                    b"garbage without a delimiter\n".as_slice()
                });
                broken += 1;
            }
            Line::Extension(kind) => out.extend(match kind {
                0 => b"+-OWNER 12345\n".as_slice(),
                1 => b"+-NTOBJECTNAME _door\n",
                2 => b"+-EVENT\t0\t1\tonActivate\t0\tSelf\t\tfireRelay\t\t\t\t\n",
                3 => b"+-AUDIOEMITTER Music Loop\"\n",
                _ => b"+-LIGHT Custom Light\" 1\n",
            }),
        }
    }
    (out, bricks, stock, broken)
}

proptest! {
    #![proptest_config(bri_chaos::proptest_config(256, 0xb15))]

    #[test]
    fn old_saves_keep_every_readable_brick(
        lines in proptest::collection::vec(line(), 0..48),
        linecount in any::<bool>(),
    ) {
        let (bytes, bricks, stock, broken) = render(&lines, linecount);
        let (world, skipped) = bri_bls::bls::read_counting(&bytes, &catalog(), "fuzz", "map/fuzz")
            .map_err(|e| TestCaseError::fail(format!("{e:#}")))?;
        prop_assert_eq!(world.bricks.len(), bricks);
        prop_assert_eq!(skipped.lines(), broken);
        let resolved = world
            .bricks
            .values()
            .filter(|b| matches!(b.definition, bri_world::ContentRef::Resolved(_)))
            .count();
        prop_assert_eq!(resolved, stock);
        world.validate().map_err(|e| TestCaseError::fail(format!("{e:#}")))?;
        let saved = serde_json::to_vec(&world).unwrap();
        prop_assert_eq!(bri_world::persistence::decode(&saved).unwrap(), world);
    }

    #[test]
    fn garbage_never_panics_the_bls_reader(
        lines in proptest::collection::vec(line(), 0..16),
        noise in proptest::collection::vec(any::<u8>(), 0..256),
        at in any::<usize>(),
    ) {
        let (mut bytes, ..) = render(&lines, true);
        let at = at % (bytes.len() + 1);
        bytes.splice(at..at, noise);
        let _ = bri_bls::bls::read_counting(&bytes, &catalog(), "fuzz", "map/fuzz");
    }
}
