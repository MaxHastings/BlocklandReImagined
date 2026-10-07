//! A pack written for another version of the game: a player reads one
//! plain line; the rebuild command is only for developers.

use bri_weapons::{OtherSchema, Pack, SCHEMA};

fn load(schema: u32) -> anyhow::Error {
    let mut pack = serde_json::to_value(bri_weapons::testing::pack()).unwrap();
    pack["schema_version"] = schema.into();
    Pack::from_json(&serde_json::to_vec(&pack).unwrap()).unwrap_err()
}

#[test]
fn another_versions_pack_reads_plainly_and_tells_developers_how_to_rebuild() {
    let older = load(SCHEMA - 1).context("Add-On Old Guns: weapons.json");
    let shown = format!("{older:#}");
    assert_eq!(
        shown,
        "Add-On Old Guns: weapons.json: made for an older version of the game; reinstall it or get an updated copy"
    );
    let developer = OtherSchema::developer_of(&older).unwrap();
    assert!(
        developer.contains("tools/bootstrap.py --rebuild weapons"),
        "{developer}"
    );
    assert!(!shown.contains("bootstrap"), "{shown}");
    assert!(format!("{:#}", load(SCHEMA + 1)).contains("newer version"));
    let other = anyhow::anyhow!("Weapon pack exceeds byte limit");
    assert_eq!(OtherSchema::developer_of(&other), None);
}
