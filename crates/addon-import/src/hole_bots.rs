//! Bot_Hole's bots. A `PlayerData` with `isHoleBot` is two things here: a
//! body (its archetype, converted as any player type) and a bot kind
//! (`assets/bots.json`) read from the `h` settings Bot_Hole's brain reads.
//! A hole brick (`isBotHole`, `holeBot`) keeps one of that kind.
use bri_console::Clamp;
use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::player_types::number;

/// Bot_Hole's `brickToRadius`: a search radius in brick units as world
/// units (`support.cs`).
fn brick_to_radius(n: f32) -> f32 {
    ((n - 5.0) / 2.0).max(0.25)
}
/// Bot_Hole's `brickToMetric`: brick units as world units.
fn brick_to_metric(n: f32) -> f32 {
    n / 2.0
}

/// The bot kind `id` a hole bot's merged `fields` (lower-case keys, raw
/// source) describe, playing in `body`.
pub(crate) fn kind(
    id: &str,
    datablock: &str,
    body: &str,
    fields: &BTreeMap<String, String>,
) -> Value {
    let num = |k: &str| fields.get(k).and_then(|v| number(v));
    let on = |k: &str| num(k).is_some_and(|n| n != 0.0);
    let text = |k: &str| {
        fields
            .get(k)
            .map(|v| crate::literal(v).trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    let name: String = text("hname")
        .unwrap_or_else(|| datablock.to_owned())
        .chars()
        .filter(|c| !c.is_control())
        .take(32)
        .collect();
    // Without `hSearch` it never goes after anyone.
    let sight = if fields.contains_key("hsearch") && !on("hsearch") {
        1.0
    } else {
        num("hsearchradius").map_or(80.0, brick_to_radius)
    }
    .clamped(1.0, 400.0);
    let wander = if fields.contains_key("hwander") && !on("hwander") {
        0.0
    } else {
        num("hspawndist").map_or(12.0, brick_to_metric)
    }
    .clamped(0.0, 64.0);
    let mut kind = json!({
        "id": id,
        "name": name,
        "body": body,
        "sight": sight,
        "wander_radius": wander,
        "chase_radius": wander.max(48.0),
    });
    if on("hmelee") {
        // `AIPlayer::hMeleeAttack`: the touched player takes
        // `hAttackDamage`, at most once a second, and the bot swings
        // (`playThread(2, activate2)`).
        kind["melee"] = json!({
            "damage": num("hattackdamage").unwrap_or(0.0).clamped(0.0, 1000.0),
            "seconds": 1.0,
            "action": "activate2",
            "name": "Melee",
        });
    }
    if on("halertotherbots") {
        kind["alerts_allies"] = json!(true);
    }
    if let Some(side) = text("htype") {
        kind["side"] = json!(
            side.to_ascii_lowercase()
                .chars()
                .take(64)
                .collect::<String>()
        );
    }
    kind
}

/// `bots.json` holding `kinds`.
pub(crate) fn pack(kinds: &[Value]) -> Value {
    json!({ "schema_version": 1, "bots": kinds })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hole_bots_settings_become_its_kind() {
        let fields: BTreeMap<String, String> = [
            ("hname", "\"Walker\""),
            ("htype", "Walkers"),
            ("hsearch", "1"),
            ("hsearchradius", "64"),
            ("hwander", "1"),
            ("hspawndist", "20"),
            ("hmelee", "1"),
            ("hattackdamage", "12"),
            ("halertotherbots", "1"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
        let k = kind(
            "x:bot/walkerbot",
            "WalkerBot",
            "x:archetype/walkerbot",
            &fields,
        );
        assert_eq!(k["name"], "Walker");
        assert_eq!(k["side"], "walkers");
        assert_eq!(k["sight"], 29.5);
        assert_eq!(k["wander_radius"], 10.0);
        assert_eq!(k["chase_radius"], 48.0);
        assert_eq!(k["melee"]["damage"], 12.0);
        assert_eq!(k["melee"]["action"], "activate2");
        assert_eq!(k["body"], "x:archetype/walkerbot");
        assert_eq!(k["alerts_allies"], true);
    }
}
