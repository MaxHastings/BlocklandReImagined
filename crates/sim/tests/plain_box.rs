//! Which bricks are plain boxes of plastic (`Definition::is_plain_box`):
//! the ones a hard blow cracks and a big blast cuts along its studs. The
//! synthetic set here; the v20 catalog, ignored, where it is generated.
use bri_sim::definitions::Definitions;
use std::path::Path;

#[test]
fn plain_bricks_are_boxes_and_shaped_or_special_ones_are_not() {
    let definitions = bri_sim::testing::definitions();
    let plain = |id: &str| definitions.entries[id].is_plain_box();
    assert!(plain(bri_sim::testing::BRICK));
    assert!(plain(bri_sim::testing::PLATE));
    assert!(!plain(bri_sim::testing::STEEP_RAMP), "a ramp is not a box");
    assert!(!plain(bri_sim::testing::WATER), "water is special");
    assert!(
        !plain(bri_sim::testing::PORTAL),
        "a portal opens onto its partner"
    );
}

#[test]
#[ignore = "requires generated v20 content"]
fn the_v20_catalog_s_plain_bricks_are_its_boxes() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let definitions = Definitions::load(
        &bri_package::testing::pack_dir(&root, "brick_catalog"),
        &bri_package::testing::pack_dir(&root, "geometry"),
    )?;
    let mut ids: Vec<&String> = definitions.entries.keys().collect();
    ids.sort();
    let (plain, shaped): (Vec<&String>, Vec<&String>) = ids
        .into_iter()
        .partition(|id| definitions.entries[*id].is_plain_box());
    println!("{} plain: {plain:?}", plain.len());
    println!("{} shaped: {shaped:?}", shaped.len());
    for id in [
        "v20/brick/brick1x1data",
        "v20/brick/brick2x4data",
        "v20/brick/brick1x4fdata",
        "v20/brick/brick4xcubedata",
        "v20/brick/brick64xcubedata",
    ] {
        if let Some(d) = definitions.entries.get(id) {
            anyhow::ensure!(d.is_plain_box(), "{id} is a plain brick");
        }
    }
    for id in &plain {
        anyhow::ensure!(
            !["ramp", "round", "wedge", "window", "door", "arch", "cone"]
                .iter()
                .any(|shape| id.contains(shape)),
            "{id} is shaped, not a box"
        );
    }
    anyhow::ensure!(plain.len() > 20, "too few plain bricks: {}", plain.len());
    Ok(())
}
