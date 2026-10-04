//! A script-only Add-On can supply authoritative death messages and icons.
use bri_addon_import::{Options, import};
#[test]
fn standalone_damage_types_survive_without_weapon_datablocks() {
    for (name, damage) in [
        ("Event_Pebble", "PebbleMark"),
        ("Event_Cobalt", "CobaltStamp"),
    ] {
        let root =
            std::env::temp_dir().join(format!("bri-type-only-{}-{name}", std::process::id()));
        let input = root.join(name);
        let out = root.join("imported");
        std::fs::create_dir_all(&input).unwrap();
        std::fs::write(
            input.join("description.txt"),
            "Title: Invented type-only policy\nAuthor: Test\n",
        )
        .unwrap();
        std::fs::write(input.join("server.cs"), format!("AddDamageType(\"{damage}\", '<bitmap:Add-Ons/{name}/stamp> %1', '%2 <bitmap:Add-Ons/{name}/stamp> %1', 0.5, 1);\n")).unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([120, 35, 210, 255]))
            .save(input.join("stamp.png"))
            .unwrap();
        import(&Options {
            input,
            out: out.clone(),
            ..Default::default()
        })
        .unwrap();
        let pack =
            bri_weapons::Pack::from_json(&std::fs::read(out.join("assets/weapons.json")).unwrap())
                .unwrap();
        assert!(pack.items.is_empty() && pack.images.is_empty() && pack.projectiles.is_empty());
        let ty = &pack.damage_types[&damage.to_ascii_lowercase()];
        assert_eq!(ty.name, damage);
        let icons: Vec<_> = ty
            .icons()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        assert_eq!(
            icons,
            vec![format!("add-ons/{}/stamp", name.to_ascii_lowercase())]
        );
        let resource = pack
            .resources
            .iter()
            .find(|r| r.path.eq_ignore_ascii_case(&format!("{}.png", icons[0])))
            .unwrap();
        assert!(
            out.join("assets")
                .join(resource.native_file.as_ref().unwrap())
                .is_file()
        );
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(out.join("package.json")).unwrap()).unwrap();
        assert!(
            manifest["provides"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["kind"] == "weapons")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
