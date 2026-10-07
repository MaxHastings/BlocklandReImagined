//! Loop 3's proof that moving the runtime's stock name table into pack data
//! changed nothing for vanilla: the importer's declared behaviour, compared
//! image by image with what the old table (`weapons/src/runtime/stock.rs`
//! before 2026-10-07) made the runtime do. The table is kept here, in a
//! test, as the reference.
//!
//! The only differences are arm moves, read now from v20's own scripts
//! where the table guessed by name. They are presentation cues, outside the
//! match state a replay digests.
use bri_weapons::*;
use std::path::PathBuf;

/// What the old table gave an image, in the runtime's terms.
#[derive(Debug, PartialEq)]
struct Old {
    charge_arm: Option<&'static str>,
    prefire_arm: Option<&'static str>,
    /// The arm played after a right-hand shot: the throw arm, else the
    /// recoil arm.
    fire_arm: Option<&'static str>,
    on_fire: Option<OnFire>,
    sport: Sport,
    left_image: Option<String>,
}

fn old(name: &str) -> Old {
    let has = |part: &str| name.contains(part);
    let tool = match name {
        "hammerimage" => Some(HostTool::Break),
        "wrenchimage" => Some(HostTool::Inspect),
        "printgunimage" => Some(HostTool::Print),
        "wandimage" => Some(HostTool::Destroy),
        "adminwandimage" => Some(HostTool::AdminDestroy),
        _ => None,
    };
    Old {
        charge_arm: (has("spear") || has("football")).then_some("spearready"),
        prefire_arm: if has("key") {
            Some("shiftleft")
        } else if name == "wrenchimage" {
            Some("wrench")
        } else if has("sword") || matches!(name, "hammerimage" | "wandimage" | "adminwandimage")
        {
            Some("armattack")
        } else {
            None
        },
        fire_arm: if has("spear") || has("football") {
            Some("spearthrow")
        } else if has("pushbroom") {
            Some("rotcw")
        } else if has("gun") || has("horseray") {
            Some("shiftaway")
        } else {
            None
        },
        on_fire: if let Some(tool) = tool {
            Some(OnFire::Tool(tool))
        } else if name == "skiweaponimage" {
            Some(OnFire::Skis)
        } else if has("keyimage") {
            Some(OnFire::Key)
        } else if name == "basketballimage" {
            Some(OnFire::Mount(native_id("image", "basketballShootImage")))
        } else {
            None
        },
        sport: Sport {
            throw: if has("dodgeball") {
                [30.0, 4.0]
            } else if has("football") {
                [40.0, 0.0]
            } else if has("soccer") {
                [20.0, 3.0]
            } else {
                [7.0, 7.5]
            },
            aimed_throw: has("basketball"),
            spawn_grace_ticks: if has("dodgeball") { 120 } else { 0 },
            thrown: has("football"),
            keys: if has("football") {
                Some(SportKeys::Lateral)
            } else if has("soccer") {
                Some(SportKeys::Pop)
            } else if has("basketballshoot") {
                Some(SportKeys::Pass)
            } else {
                None
            },
            ball: if has("basketball") {
                Some(Ball::Basketball)
            } else if has("football") {
                Some(Ball::Football)
            } else {
                None
            },
        },
        left_image: (name == "akimbogunimage").then(|| native_id("image", "LeftHandedGunImage")),
    }
}

/// Every difference between the declared data and the old table, as
/// `image | what | old | new`.
fn differences(pack: &Pack) -> Vec<String> {
    let mut out = vec![];
    for image in pack.images.values() {
        let name = image.name.to_ascii_lowercase();
        let o = old(&name);
        let arm = |script: &str| {
            image
                .states
                .iter()
                .find(|s| s.script.eq_ignore_ascii_case(script))
                .map(|s| (!s.arm.is_empty()).then(|| s.arm.to_ascii_lowercase()))
        };
        let mut differs = |what: &str, old: Option<&str>, new: Option<Option<String>>| {
            if let Some(new) = new
                && old != new.as_deref()
            {
                out.push(format!("{name} | {what} | {old:?} | {new:?}"));
            }
        };
        differs("onCharge arm", o.charge_arm, arm("onCharge"));
        differs("onPreFire arm", o.prefire_arm, arm("onPreFire"));
        // The old table played no throw or recoil arm for an image whose
        // `onFire` ran something else (a building tool, the skis), and
        // the left hand's `leftrecoil` came before any recoil arm.
        let fire_arm = if o.on_fire.is_some() {
            None
        } else if name == "lefthandedgunimage" {
            Some("leftrecoil")
        } else {
            o.fire_arm
        };
        let new_fire_arm = if image.on_fire.is_some() {
            Some(None)
        } else {
            arm("onFire")
        };
        differs("onFire arm", fire_arm, new_fire_arm);
        for script in ["onAbortCharge", "onStopFire"] {
            differs(&format!("{script} arm"), Some("root"), arm(script).map(|a| a.or(Some("root".into()))));
        }
        if o.on_fire != image.on_fire {
            out.push(format!("{name} | on_fire | {:?} | {:?}", o.on_fire, image.on_fire));
        }
        if o.sport != image.sport.unwrap_or_default() {
            out.push(format!("{name} | sport | {:?} | {:?}", o.sport, image.sport));
        }
        if o.left_image != image.left_image {
            out.push(format!("{name} | left_image | {:?} | {:?}", o.left_image, image.left_image));
        }
    }
    for p in pack.projectiles.values() {
        let name = p.name.to_ascii_lowercase();
        let hit = match name.as_str() {
            "dodgeballprojectile" => Some(SportHit::KnockOut),
            "footballprojectile" => Some(SportHit::Catch),
            _ => None,
        };
        if hit != p.sport_hit {
            out.push(format!("{name} | sport_hit | {hit:?} | {:?}", p.sport_hit));
        }
        let turns = (name == "horserayprojectile").then(|| "v20.player.horsearmor".to_owned());
        if turns != p.turns_into {
            out.push(format!("{name} | turns_into | {turns:?} | {:?}", p.turns_into));
        }
        // A resting ball became the football's item or else the soccer
        // ball's; now the item that holds its ball's image.
        if let Some(image) = p.sport_image.as_ref().filter(|_| p.rest_speed > 0.0) {
            let was = native_id(
                "weapon",
                if name == "footballprojectile" {
                    "footballItem"
                } else {
                    "soccerBallItem"
                },
            );
            let now = pack.items.iter().find(|(_, i)| &i.image == image).map(|(id, _)| id.clone());
            if Some(&was) != now.as_ref() {
                out.push(format!("{name} | rest item | {was:?} | {now:?}"));
            }
        }
    }
    out.sort();
    out
}

#[test]
#[ignore = "requires the v20 install and the recovered core scripts in .research"]
fn vanilla_behaviour_matches_the_old_stock_table() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    // `BRI_V20`, else the install `tools/bootstrap.py` remembered.
    let v20 = std::env::var_os("BRI_V20").map(PathBuf::from).or_else(|| {
        std::fs::read_to_string(repo.join("content/_regeneration/v20-path.txt"))
            .ok()
            .map(|p| PathBuf::from(p.trim()))
    });
    let scripts = repo.join(".research/v20-dso/server/scripts");
    let Some(v20) = v20.filter(|v| v.join("Add-Ons").is_dir() && scripts.is_dir()) else {
        eprintln!("skipped: no v20 install (BRI_V20) or no recovered scripts in .research");
        return;
    };
    let out = std::env::temp_dir().join(format!("bri-stock-parity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let pack = bri_weapons_import::convert(
        &v20,
        &scripts.join("allGameScripts-Vanilla.cs"),
        &scripts.join("DamageTypes.cs"),
        &out,
    )
    .unwrap();
    let _ = std::fs::remove_dir_all(&out);
    // v20's scripts, read where the table guessed: the ball images' `onFire`
    // plays `playThread(2, root)`; only `redKeyImage` has an `onPreFire`
    // (the other keys copy its fields, not its namespace); the horse's
    // football has no charge or throw script of its own.
    let expected = [
        "basketballshootimage | onFire arm | None | Some(\"root\")",
        "bluekeyimage | onPreFire arm | Some(\"shiftleft\") | None",
        "dodgeballimage | onFire arm | None | Some(\"root\")",
        "greenkeyimage | onPreFire arm | Some(\"shiftleft\") | None",
        "horsefootballimage | onCharge arm | Some(\"spearready\") | None",
        "horsefootballimage | onFire arm | Some(\"spearthrow\") | None",
        "soccerballimage | onFire arm | None | Some(\"root\")",
        "yellowkeyimage | onPreFire arm | Some(\"shiftleft\") | None",
    ];
    assert_eq!(differences(&pack), expected);
}
