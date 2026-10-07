//! The weapons runtime reads declared data ([`bri_weapons::Image::on_fire`],
//! [`bri_weapons::Image::sport`], [`bri_weapons::ProjectileDef::sport_hit`],
//! ...), never an image's, item's or projectile's name. A v20
//! compatibility case is a field the importer fills.

#[test]
fn the_runtime_never_matches_datablock_names() {
    let sources = [
        ("runtime.rs", include_str!("../src/runtime.rs")),
        ("runtime/sports.rs", include_str!("../src/runtime/sports.rs")),
        (
            "runtime/persistence.rs",
            include_str!("../src/runtime/persistence.rs"),
        ),
    ];
    // Compared without whitespace, so a call split over lines still counts.
    let banned = [
        "name.contains(",
        "name==\"",
        "name.as_str()",
        "image.contains(",
        "image.to_ascii_lowercase()",
        "image.rsplit(",
        "projectile.contains(",
        "name.eq_ignore_ascii_case(\"",
        "native_id(",
        "format!(\"horse",
        "Stock::",
    ];
    for (file, source) in sources {
        let source: String = source.chars().filter(|c| !c.is_whitespace()).collect();
        for pattern in banned {
            assert!(
                !source.contains(pattern),
                "{file} matches a datablock name (`{pattern}`); \
                 declare the behaviour in pack data and fill it in the importer"
            );
        }
    }
}
