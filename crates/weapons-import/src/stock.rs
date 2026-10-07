//! What v20's stock images and projectiles did in their TorqueScript
//! callbacks, written into pack data so the runtime reads fields and never a
//! name.
//!
//! Two kinds of reading:
//! - From the script body where it says it plainly: the arm move a state's
//!   script plays (`playThread(2, X)`) becomes that state's `arm`, and the
//!   image `onMount` puts in the left hand (`mountImage(X, 1)`) becomes
//!   `left_image`. The Add-On importer reads its scripts with the same
//!   functions ([`script_arm`], [`left_hand_image`]).
//! - From a table by datablock name, where the script calls engine
//!   functions no reader can turn into data (`hammerImage::onFire`'s brick
//!   hit, Item_Sports' ball physics, the horse ray's transform). This table
//!   is the only name list left; it applies to the vanilla import only, so
//!   an Add-On image never picks up stock behaviour by sharing part of a
//!   name.
use bri_convert::tscript::Script;
use bri_weapons::*;
use regex::Regex;
use std::{collections::BTreeMap, sync::LazyLock};

/// The holder's arm move (animation thread 2) a script body plays, lower
/// case: `%obj.playThread(2, spearReady)` gives `spearready`.
pub fn script_arm(body: &str) -> Option<String> {
    static ARM: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)playthread\s*\(\s*2\s*,\s*([A-Za-z_]\w*)\s*\)").expect("pattern")
    });
    ARM.captures(&uncommented(body))
        .map(|c| c[1].to_ascii_lowercase())
}

/// What a script body does to the holder's raised arms (animation thread
/// 1): `Some(Some((right, left)))` for `playThread(1, armReadyRight)`,
/// `armReadyLeft` or `armReadyBoth`, `Some(None)` for `root` (lowered),
/// None when it leaves them be.
pub fn script_raised_arms(body: &str) -> Option<Option<(bool, bool)>> {
    static RAISE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)playthread\s*\(\s*1\s*,\s*([A-Za-z_]\w*)\s*\)").expect("pattern")
    });
    let uncommented = uncommented(body);
    let sequence = RAISE.captures_iter(&uncommented).last()?[1].to_ascii_lowercase();
    Some(match sequence.as_str() {
        "armreadyright" => Some((true, false)),
        "armreadyleft" => Some((false, true)),
        "armreadyboth" => Some((true, true)),
        "root" => None,
        _ => return None,
    })
}

/// Raises `image`'s arms where its scripts do on thread 1: `onMount` for
/// the whole time it is held (`both_arms`, or `arm_ready` for the right
/// arm), a state's script for that state and those it leads to that all
/// come from states with the same arms up, until a script lowers them.
fn raise_arms(image: &mut Image, bodies: &BTreeMap<String, &str>) {
    let name = image.name.to_ascii_lowercase();
    let script = |function: &str| {
        bodies
            .get(&format!("{name}::{}", function.to_ascii_lowercase()))
            .and_then(|body| script_raised_arms(body))
    };
    match script("onmount") {
        Some(Some((true, true))) => image.both_arms = true,
        Some(Some((true, false))) => image.arm_ready = true,
        _ => {}
    }
    let own: Vec<_> = image
        .states
        .iter()
        .map(|s| (!s.script.is_empty()).then(|| script(&s.script)).flatten())
        .collect();
    let mut from: Vec<Vec<usize>> = vec![Vec::new(); image.states.len()];
    for (i, s) in image.states.iter().enumerate() {
        for next in [
            s.timeout,
            s.down,
            s.up,
            s.ammo,
            s.no_ammo,
            s.loaded,
            s.not_loaded,
        ]
        .into_iter()
        .flatten()
        {
            if next != i && next < from.len() && !from[next].contains(&i) {
                from[next].push(i);
            }
        }
    }
    let mut raised: Vec<Option<(bool, bool)>> = vec![None; image.states.len()];
    // Settles within one pass a state; the bound only guards odd graphs.
    for _ in 0..=raised.len() {
        let next: Vec<_> = (0..raised.len())
            .map(|i| match own[i] {
                Some(arms) => arms,
                None => {
                    let first = from[i].first().and_then(|f| raised[*f]);
                    first.filter(|arms| from[i].iter().all(|f| raised[*f] == Some(*arms)))
                }
            })
            .collect();
        if next == raised {
            break;
        }
        raised = next;
    }
    for (state, arms) in image.states.iter_mut().zip(raised) {
        state.raised_arms = arms;
    }
}

/// The image an `onMount` body mounts in the left hand
/// (`%obj.mountImage(LeftHandedGunImage, 1)`), by datablock name.
pub fn left_hand_image(body: &str) -> Option<String> {
    static LEFT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)mountimage\s*\(\s*([A-Za-z_]\w*)\s*,\s*1\s*\)").expect("pattern")
    });
    LEFT.captures(&uncommented(body)).map(|c| c[1].to_owned())
}

/// `body` without its `//` comments.
fn uncommented(body: &str) -> String {
    body.lines()
        .map(|l| l.find("//").map_or(l, |i| &l[..i]))
        .collect::<Vec<_>>()
        .join(
            "
",
        )
}

/// Fills the stock images' and projectiles' script behaviour: arms and the
/// left hand from `scripts`, the rest from the vanilla table.
pub fn declare(scripts: &[Script], pack: &mut Pack) {
    let bodies: BTreeMap<String, &str> = scripts
        .iter()
        .flat_map(|s| &s.functions)
        .map(|f| (f.qualified().to_ascii_lowercase(), f.body.as_str()))
        .collect();
    let ids: BTreeMap<String, String> = pack
        .images
        .iter()
        .map(|(id, image)| (image.name.to_ascii_lowercase(), id.clone()))
        .collect();
    for image in pack.images.values_mut() {
        let name = image.name.to_ascii_lowercase();
        for state in &mut image.states {
            if state.arm.is_empty()
                && !state.script.is_empty()
                && let Some(arm) = bodies
                    .get(&format!("{name}::{}", state.script.to_ascii_lowercase()))
                    .and_then(|body| script_arm(body))
            {
                state.arm = arm;
            }
        }
        if image.left_image.is_none() {
            image.left_image = bodies
                .get(&format!("{name}::onmount"))
                .and_then(|body| left_hand_image(body))
                .and_then(|left| ids.get(&left.to_ascii_lowercase()).cloned());
        }
        raise_arms(image, &bodies);
        image.on_fire = on_fire(&name);
        image.sport = sport(&name);
        // Item_Sports' `passBallCheck` mounts `"horse" @ %image` on a horse.
        if image.sport.is_some() {
            image.riding_image = ids.get(&format!("horse{name}")).cloned();
        }
    }
    for projectile in pack.projectiles.values_mut() {
        let name = projectile.name.to_ascii_lowercase();
        projectile.sport_hit = match name.as_str() {
            "dodgeballprojectile" => Some(SportHit::KnockOut),
            "footballprojectile" => Some(SportHit::Catch),
            _ => None,
        };
        projectile.turns_into =
            (name == "horserayprojectile").then(|| "v20.player.horsearmor".to_owned());
    }
}

/// What a stock image's `onFire` does instead of launching its projectile.
fn on_fire(name: &str) -> Option<OnFire> {
    Some(match name {
        "hammerimage" => OnFire::Tool(HostTool::Break),
        "wrenchimage" => OnFire::Tool(HostTool::Inspect),
        "printgunimage" => OnFire::Tool(HostTool::Print),
        "wandimage" => OnFire::Tool(HostTool::Destroy),
        "adminwandimage" => OnFire::Tool(HostTool::AdminDestroy),
        "skiweaponimage" => OnFire::Skis,
        "basketballimage" => OnFire::Mount(native_id("image", "basketballShootImage")),
        _ if name.contains("keyimage") => OnFire::Key,
        _ => return None,
    })
}

/// How a stock ball image throws (Item_Sports' `onFire` scripts); None
/// for an image that throws as [`Sport::default`] or throws nothing.
fn sport(name: &str) -> Option<Sport> {
    let has = |part: &str| name.contains(part);
    let throw = if has("dodgeball") {
        [30.0, 4.0]
    } else if has("football") {
        [40.0, 0.0]
    } else if has("soccer") {
        [20.0, 3.0]
    } else {
        Sport::default().throw
    };
    let sport = Sport {
        throw,
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
    };
    (sport != Sport::default()).then_some(sport)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_reads_find_the_arm_and_the_left_hand() {
        assert_eq!(
            script_arm("%obj.playthread(2, spearReady);").as_deref(),
            Some("spearready")
        );
        assert_eq!(script_arm("%obj.playThread(1, armReadyRight);"), None);
        assert_eq!(script_arm("// %obj.playThread(2, root);"), None);
        assert_eq!(
            script_raised_arms("%obj.playThread(1,armReadyRight);"),
            Some(Some((true, false)))
        );
        assert_eq!(script_raised_arms("%obj.playThread(1, root);"), Some(None));
        assert_eq!(
            script_raised_arms("//%obj.playThread(1, armReadyBoth);"),
            None
        );
        assert_eq!(
            script_raised_arms("%obj.playThread(2, armReadyBoth);"),
            None
        );
        assert_eq!(
            left_hand_image(
                "Parent::onMount(%this,%obj,%slot); %obj.mountImage(LeftHandedGunImage, 1);"
            )
            .as_deref(),
            Some("LeftHandedGunImage")
        );
        assert_eq!(
            left_hand_image("%obj.mountImage(basketballShootImage,0);"),
            None
        );
    }

    /// The football and Akimbo shapes: `onMount` raises the arms for as long
    /// as it is held; `onCharge` raises the right arm through the states
    /// after it until `onFire` lowers it.
    #[test]
    fn scripts_raise_arms_for_their_states() {
        let state = |name: &str, script: &str, timeout, down, up| State {
            name: name.into(),
            script: script.into(),
            timeout,
            down,
            up,
            ..State::authored()
        };
        let mut football = Image {
            name: "footballImage".into(),
            states: vec![
                state("Activate", "", Some(1), None, None),
                state("Ready", "", None, Some(2), None),
                state("Charge", "onCharge", Some(3), None, Some(4)),
                state("Armed", "", None, None, Some(5)),
                state("AbortCharge", "onAbortCharge", Some(1), None, None),
                state("Fire", "onFire", Some(1), None, None),
            ],
            ..Default::default()
        };
        let mut akimbo = Image {
            name: "LeftHandedGunImage".into(),
            states: vec![state("Ready", "", None, None, None)],
            ..Default::default()
        };
        let bodies: BTreeMap<String, &str> = [
            (
                "footballimage::oncharge",
                "%obj.playThread(1,armReadyRight);",
            ),
            ("footballimage::onabortcharge", "%obj.playThread(1,root);"),
            ("footballimage::onfire", "%obj.playThread(1,root); throw();"),
            (
                "lefthandedgunimage::onmount",
                "%obj.playThread(1, armreadyboth);",
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        raise_arms(&mut football, &bodies);
        raise_arms(&mut akimbo, &bodies);
        let arms: Vec<_> = football.states.iter().map(|s| s.raised_arms).collect();
        let right = Some((true, false));
        assert_eq!(arms, [None, None, right, right, None, None]);
        assert!(!football.both_arms && !football.arm_ready);
        assert!(akimbo.both_arms);
        assert_eq!(akimbo.states[0].raised_arms, None);
    }

    #[test]
    fn the_table_names_only_vanilla_behaviour() {
        assert_eq!(
            on_fire("wrenchimage"),
            Some(OnFire::Tool(HostTool::Inspect))
        );
        assert_eq!(on_fire("rocketlauncherimage"), None);
        assert_eq!(on_fire("redkeyimage"), Some(OnFire::Key));
        assert_eq!(sport("rocketlauncherimage"), None);
        let football = sport("footballimage").unwrap();
        assert!(football.thrown);
        assert_eq!(football.throw, [40.0, 0.0]);
        assert_eq!(football.keys, Some(SportKeys::Lateral));
        assert_eq!(sport("dodgeballimage").unwrap().spawn_grace_ticks, 120);
        assert_eq!(
            sport("basketballshootimage").unwrap().keys,
            Some(SportKeys::Pass)
        );
    }
}
