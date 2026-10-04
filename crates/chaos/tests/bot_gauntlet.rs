//! The bot gauntlet: fixed scenarios bots play on invented content, scored
//! on what looks bad on camera (`gauntlet::Report`): stuck, idle or
//! circling bots, flip-flopping, void and own-side deaths, shots away from
//! the target or into an ally, and whether the scenario's goal moved at all.
//! Thresholds sit just past today's measured play, so they pass now and
//! fail when play gets worse; tighten them as the brain improves (each
//! threshold's comment gives the measured value). Every run is the same
//! run: bots draw from seeded generators. `BRI_GAUNTLET_ROUNDS=n` plays
//! every scenario n times as long, for soak runs, against the same
//! thresholds: problems that pile up over a long game fail there first.
//! `BRI_GAUNTLET_TRACE=<scenario>` prints every bot's state twice a second.
mod gauntlet;

use bri_chaos::fixture;
use bri_minigames::Settings;
use bri_sim::bot_kind::BotKind;
use bri_world::{Brick, OwnerId};
use gauntlet::*;
use glam::Vec3;
use std::collections::BTreeMap;

const FAR_A: Vec3 = Vec3::new(92.0, 0.05, 92.0);
const FAR_B: Vec3 = Vec3::new(92.0, 0.05, 86.0);
const GUN: &str = bri_weapons::testing::GUN_ITEM;
const NO_JETS: &str = "v20.player.playernojet";

fn loadout(items: &[&str]) -> [Option<String>; 5] {
    let mut out: [Option<String>; 5] = Default::default();
    for (slot, item) in items.iter().enumerate() {
        out[slot] = Some(item.to_string());
    }
    out
}

/// A Blockhead-like kind named `id`, changed by `change`.
fn kind(id: &str, change: impl FnOnce(&mut BotKind)) -> BotKind {
    let mut k = blockhead_kinds().remove(0);
    k.id = id.into();
    k.name = id.into();
    change(&mut k);
    k
}
const BRAWLER: &str = "gauntlet:bot/brawler";
fn brawler() -> BotKind {
    kind(BRAWLER, |k| k.melee = Some(melee(20.0, 2.5)))
}

/// How a battle is laid out.
struct Spec {
    name: &'static str,
    /// Spawn bricks of red (side 0) and blue (side 1).
    red: Vec<Vec3>,
    blue: Vec<Vec3>,
    kinds: [&'static str; 2],
    extra_kinds: Vec<BotKind>,
    vehicle_kinds: Vec<&'static str>,
    /// Arena bricks (red's), and blue's own.
    bricks: Vec<Brick>,
    blue_bricks: Vec<Brick>,
    settings: Settings,
    /// Arena brick and item definitions for a package.
    brick_defs: Vec<(&'static str, &'static str)>,
    item_defs: Vec<(&'static str, &'static str)>,
}
impl Spec {
    fn new(name: &'static str, red: Vec<Vec3>, blue: Vec<Vec3>, items: &[&str]) -> Self {
        Self {
            name,
            red,
            blue,
            kinds: [fixture::BOT, fixture::BOT],
            extra_kinds: Vec::new(),
            vehicle_kinds: Vec::new(),
            bricks: Vec::new(),
            blue_bricks: Vec::new(),
            settings: Settings {
                loadout: loadout(items),
                use_all_players_bricks: true,
                ..Default::default()
            },
            brick_defs: Vec::new(),
            item_defs: Vec::new(),
        }
    }
}

/// Spawn points in a line across z at `x`, `count` of them 4 apart.
fn line(x: f32, y: f32, z: f32, count: usize) -> Vec<Vec3> {
    (0..count)
        .map(|i| Vec3::new(x, y, z + (i as f32 - (count as f32 - 1.0) / 2.0) * 4.0))
        .collect()
}

struct Battle {
    name: &'static str,
    arena: Arena,
    sides: BTreeMap<OwnerId, u32>,
}

/// Two builders' bots in one mini-game; `package` runs before anyone
/// joins.
fn battle(spec: Spec, package: impl FnOnce(&mut Arena)) -> Battle {
    let mut arena = Arena::custom(&spec.brick_defs, &spec.item_defs, &spec.vehicle_kinds);
    arena.kinds(spec.extra_kinds, &spec.vehicle_kinds);
    package(&mut arena);
    let red = arena.join("Red builder", FAR_A);
    let blue = arena.join("Blue builder", FAR_B);
    let mut spawns = Vec::new();
    let mut red_bricks = spec.bricks;
    for at in &spec.red {
        red_bricks.push(spawner(spec.kinds[0], *at));
        spawns.push((*at, 0));
    }
    let mut blue_bricks = spec.blue_bricks;
    for at in &spec.blue {
        spawns.push((*at, 1));
        blue_bricks.push(spawner(spec.kinds[1], *at));
    }
    arena.load(red, red_bricks);
    arena.load(blue, blue_bricks);
    let game = arena.minigame(red, spec.settings);
    arena.join_game(blue, game);
    arena.step(TICKS_PER_SECOND);
    let sides = arena.sides_by_spawn(&spawns);
    assert_eq!(
        sides.len(),
        spec.red.len() + spec.blue.len(),
        "every bot spawned"
    );
    Battle {
        name: spec.name,
        arena,
        sides,
    }
}

impl Battle {
    /// Play `seconds` (times the soak rounds), scoring every tick;
    /// `each` adds scenario progress.
    fn play(
        &mut self,
        floor: f32,
        seconds: usize,
        mut each: impl FnMut(&bri_sim::session::Session, &mut Report),
    ) -> Report {
        let mut scorer = Scorer::new(self.name, floor);
        for _ in 0..seconds * TICKS_PER_SECOND * rounds() {
            self.arena.step(1);
            let s = &self.arena.s;
            let vitals = s.vitals();
            let sides = &self.sides;
            scorer.sample(s, sides, |bot| {
                sides
                    .iter()
                    .any(|(o, side)| Some(side) != sides.get(&bot) && vitals[o].alive)
            });
            each(s, &mut scorer.report);
        }
        scorer.report.print();
        scorer.report
    }
}

/// Shares and rates no worse than today's (`max` per item). Everywhere
/// today no bot stands inside an ally and no shot leaves 25 degrees off its
/// visible target; those hold for every scenario.
fn within(r: &Report, stuck: f32, idle: f32, circling: f32, switches_per_min: f32) {
    assert!(
        r.share(r.clumped) <= 0.01,
        "{}: clumped {:.3}",
        r.name,
        r.share(r.clumped)
    );
    assert!(
        r.off_target * 20 <= r.shots,
        "{}: {} of {} shots off target",
        r.name,
        r.off_target,
        r.shots
    );
    assert!(
        r.share(r.stuck) <= stuck,
        "{}: stuck {:.3} > {stuck}",
        r.name,
        r.share(r.stuck)
    );
    assert!(
        r.share(r.idle) <= idle,
        "{}: idle {:.3} > {idle}",
        r.name,
        r.share(r.idle)
    );
    assert!(
        r.share(r.circling) <= circling,
        "{}: circling {:.3} > {circling}",
        r.name,
        r.share(r.circling)
    );
    assert!(
        r.per_bot_minute(r.switches) <= switches_per_min,
        "{}: behaviour switches {:.1}/min > {switches_per_min}",
        r.name,
        r.per_bot_minute(r.switches)
    );
}

#[test]
fn deathmatch_open_field() {
    let spec = Spec::new(
        "deathmatch_open_field",
        line(-73.0, 0.0, 50.0, 3),
        line(-37.0, 0.0, 50.0, 3),
        &[GUN],
    );
    let mut b = battle(spec, |_| {});
    let r = b.play(0.0, 60, |_, _| {});
    assert!(r.kills >= 50 * rounds() as u64, "a real fight: {}", r.kills);
    assert_eq!(r.team_kills + r.at_ally, 0, "no fire on its own side");
    // Measured at the merge: circling 18% (strafing in its band), 55
    // changes a bot-minute (a respawned bot chased for a tick before it
    // took out its gun), 66 kills. With long strafe legs and the gun out on
    // respawn: circling 8.8%, 20 changes (a side wiped out, wandering until
    // it respawns), reversals 62 -> 14 a bot-minute, 65 kills.
    within(&r, 0.01, 0.01, 0.12, 26.0);
}

#[test]
fn deathmatch_mixed_arsenal() {
    use bri_weapons::testing::*;
    let spec = Spec::new(
        "deathmatch_mixed_arsenal",
        line(-73.0, 0.0, 50.0, 3),
        line(-37.0, 0.0, 50.0, 3),
        &[ROCKET_ITEM, SHOTGUN_ITEM, BOW_ITEM, BOUNCER_ITEM, GUN],
    );
    let mut b = battle(spec, |_| {});
    let r = b.play(0.0, 60, |_, _| {});
    assert!(r.kills >= 50 * rounds() as u64, "a real fight: {}", r.kills);
    // Measured at the merge: circling 24% (strafing), 44 changes a
    // bot-minute, 69 kills, no suicide by splash. Now: circling 4.5%, 21
    // changes, reversals 66 -> 11, 67 kills.
    within(&r, 0.01, 0.01, 0.10, 28.0);
    assert_eq!(r.self_kills, 0, "no bot blew itself up");
    assert_eq!(r.team_kills + r.at_ally, 0, "no fire on its own side");
}

#[test]
fn rooftop_brawl_without_rails() {
    // A 24 x 16 deck six units up, no rails; a fall hurts.
    let mut spec = Spec::new(
        "rooftop_brawl_without_rails",
        line(-76.0, 6.0, 48.0, 3),
        line(-58.0, 6.0, 48.0, 3),
        &[GUN],
    );
    spec.bricks = floor(Vec3::new(-79.0, 0.0, 40.0), [3, 2], 6.0);
    spec.settings.falling_damage = true;
    spec.settings.player_type = NO_JETS.into();
    let mut b = battle(spec, |_| {});
    let r = b.play(6.0, 60, |_, _| {});
    assert!(r.kills >= 40 * rounds() as u64, "a real fight: {}", r.kills);
    // Measured at the merge: idle 21% (bots that strafed off the deck
    // wander below it), 8 falls, 58 changes a bot-minute, 53 kills. Now the
    // strafe stops at the edge: no falls, idle 0.2%, circling 7%, 46
    // changes (nearly all a side wiped out and respawning), 80 kills.
    within(&r, 0.01, 0.01, 0.08, 58.0);
    assert_eq!(r.fell, 0, "no bot strafed off the deck");
    assert_eq!(r.team_kills + r.at_ally, 0, "no fire on its own side");
}

/// A staircase of `steps` bricks rising `rise` each toward +x from `at`.
fn stairs(at: Vec3, steps: usize, rise: f32, width: usize) -> Vec<Brick> {
    let mut out = Vec::new();
    for i in 0..steps {
        let top = rise * (i as f32 + 1.0);
        let mut y = 0.0;
        while y < top - 0.01 {
            let (def, h) = if top - y >= 0.6 - 0.01 {
                (fixture::BRICK, 0.6)
            } else {
                (fixture::PLATE, 0.2)
            };
            for w in 0..width {
                if def == fixture::BRICK {
                    // 2 x 1 units: lay it along z.
                    let mut b = brick(
                        def,
                        Vec3::new(
                            at.x + i as f32 + 0.5,
                            y + h / 2.0,
                            at.z + w as f32 * 2.0 + 1.0,
                        ),
                    );
                    b.quarter_turns = 1;
                    out.push(b);
                } else {
                    for dz in 0..4 {
                        for dx in 0..2 {
                            out.push(brick(
                                def,
                                Vec3::new(
                                    at.x + i as f32 + 0.25 + dx as f32 * 0.5,
                                    y + h / 2.0,
                                    at.z + w as f32 * 2.0 + 0.25 + dz as f32 * 0.5,
                                ),
                            ));
                        }
                    }
                }
            }
            y += h;
        }
    }
    out
}

#[test]
fn stairs_to_a_deck() {
    // Red brawlers start on the ground; blue brawlers on a deck 3 up,
    // reached by a staircase of brick-high steps. No jets: walking only.
    let deck_top = 3.0;
    let mut spec = Spec::new(
        "stairs_to_a_deck",
        line(-78.0, 0.0, 50.0, 2),
        line(-50.0, deck_top, 50.0, 2),
        &[],
    );
    spec.kinds = [BRAWLER, BRAWLER];
    spec.extra_kinds = vec![brawler()];
    spec.settings.player_type = NO_JETS.into();
    let mut bricks = floor(Vec3::new(-58.0, 0.0, 42.0), [2, 2], deck_top);
    bricks.extend(stairs(Vec3::new(-63.0, 0.0, 47.0), 5, 0.6, 3));
    spec.bricks = bricks;
    let mut b = battle(spec, |_| {});
    let r = b.play(0.0, 60, |_, _| {});
    // Measured at the merge: stuck 3.6%, circling 12.7%, 40 changes a
    // bot-minute (melee chase/fight flip-flop), 21 kills. Now melee closes
    // instead of strafing round its target: circling 1.5-2.7%, 18-26
    // changes, 17-22 kills over the runs while these fixes landed. Stuck
    // moved between 3.5% and 7.2% from run to run (a chase that settles
    // short of an enemy on the deck, or a bot stood on another's head), so
    // its bound has that headroom.
    within(&r, 0.08, 0.01, 0.04, 30.0);
    assert!(
        r.kills >= 15 * rounds() as u64,
        "the deck was taken: {}",
        r.kills
    );
}

#[test]
fn water_between_the_sides() {
    // A strip of deep water, eight across, between two sides of brawlers.
    let mut spec = Spec::new(
        "water_between_the_sides",
        line(-74.0, 0.0, 50.0, 2),
        line(-46.0, 0.0, 50.0, 2),
        &[],
    );
    spec.kinds = [BRAWLER, BRAWLER];
    spec.extra_kinds = vec![brawler()];
    spec.settings.player_type = NO_JETS.into();
    let mut bricks = Vec::new();
    for x in 0..2 {
        for z in 0..8 {
            bricks.push(brick(
                bri_sim::testing::DEEP_WATER,
                Vec3::new(-64.0 + x as f32 * 4.0, 3.0, 34.0 + z as f32 * 4.0),
            ));
        }
    }
    spec.bricks = bricks;
    let mut b = battle(spec, |_| {});
    let r = b.play(0.0, 60, |_, _| {});
    // Measured at the merge: stuck 94% (walkers float in deep water with
    // no route), no kills, 2 changes a bot-minute. Now a brawler in its
    // band walks at its enemy rather than strafing, and some cross: stuck
    // 13-30%, 6-15 kills, 7-15 changes (the chase/fight of a real fight)
    // over the runs while these fixes landed. TARGET stuck < 5%.
    within(&r, 0.35, 0.01, 0.02, 20.0);
    assert!(r.kills > 0, "the water was crossed");
}

#[test]
fn a_jeep_on_each_side() {
    let car = bri_vehicles::testing::CAR;
    let mut spec = Spec::new(
        "a_jeep_on_each_side",
        line(-76.0, 0.0, 50.0, 2),
        line(-36.0, 0.0, 50.0, 2),
        &[],
    );
    spec.kinds = [BRAWLER, BRAWLER];
    spec.extra_kinds = vec![brawler()];
    spec.vehicle_kinds = vec![car];
    spec.bricks = vec![
        spawner(car, Vec3::new(-70.0, 0.0, 58.0)),
        spawner(car, Vec3::new(-42.0, 0.0, 42.0)),
    ];
    let mut b = battle(spec, |_| {});
    let mut driving = 0i64;
    let r = b.play(0.0, 60, |s, report| {
        driving += s
            .vitals()
            .iter()
            .filter(|(o, v)| s.is_bot(**o) && v.mounted.is_some())
            .count() as i64;
        report.progress.insert("mounted_ticks".into(), driving);
    });
    // Measured: on foot stuck 59% (stood on a jeep roof with no route),
    // 78% of bot time driving, 1 kill in 60 s (drivers circle each other):
    // TARGET kills > 10, stuck < 5%.
    within(&r, 0.65, 0.01, 0.02, 14.0);
    assert!(
        r.progress["mounted_ticks"] >= 10_000 * rounds() as i64,
        "the jeeps were used: {:?}",
        r.progress
    );
}

#[test]
fn weapons_lying_on_the_ground() {
    // Unarmed Blockhead Bots, no body attack, guns and launchers on item
    // bricks around the field.
    use bri_weapons::testing::*;
    let mut spec = Spec::new(
        "weapons_lying_on_the_ground",
        line(-74.0, 0.0, 50.0, 2),
        line(-46.0, 0.0, 50.0, 2),
        &[],
    );
    let mut bricks = Vec::new();
    for (i, item) in [GUN, ROCKET_ITEM, BOW_ITEM, SHOTGUN_ITEM, GUN, BOUNCER_ITEM]
        .iter()
        .enumerate()
    {
        let mut b = brick(
            fixture::BRICK,
            Vec3::new(-68.0 + i as f32 * 3.5, 0.3, 44.0 + (i % 2) as f32 * 12.0),
        );
        b.item_spawn.item = Some(bri_world::ContentRef::Resolved(item.to_string()));
        bricks.push(b);
    }
    spec.bricks = bricks;
    let mut b = battle(spec, |_| {});
    let r = b.play(0.0, 60, |s, report| {
        let armed = s
            .tool_inventories()
            .iter()
            .filter(|(o, inv)| s.is_bot(**o) && inv.slots.iter().any(|x| x.is_some()))
            .count() as i64;
        let best = report.progress.get("armed_bots").copied().unwrap_or(0);
        report.progress.insert("armed_bots".into(), best.max(armed));
    });
    // Measured at the merge: circling 91% (unarmed bots strafe round each
    // other doing nothing), no bot ever armed, no kills. Now a bot with
    // nothing to attack with does not fight; it arms itself from a weapon
    // in sight: all 4 armed, 31 kills, circling 4.4%, 36 changes a
    // bot-minute (arm, fight, and arm again after each respawn).
    within(&r, 0.01, 0.01, 0.08, 45.0);
    assert!(
        r.progress["armed_bots"] == 4 * rounds() as i64,
        "every bot armed itself: {:?}",
        r.progress
    );
    assert!(r.kills > 0, "armed, they fought: {}", r.kills);
}

#[test]
fn zombie_survival() {
    const ZOMBIE: &str = "gauntlet:bot/zombie";
    const ARMORY: &str = "gauntlet-survival:brick/armory";
    let mut spec = Spec::new(
        "zombie_survival",
        line(-76.0, 0.0, 50.0, 4),
        line(-44.0, 0.0, 50.0, 2),
        &[],
    );
    spec.kinds = [ZOMBIE, fixture::BOT];
    spec.extra_kinds = vec![kind(ZOMBIE, |k| {
        k.melee = Some(melee(25.0, 2.5));
        k.side = Some("zombies".into());
        k.behaviours.insert("interact".into(), 0.0);
    })];
    // Bots whose builder owns the armory brick are armed as they spawn.
    spec.brick_defs = vec![(ARMORY, fixture::PLATE)];
    spec.blue_bricks = vec![brick(ARMORY, Vec3::new(-40.25, 0.1, 40.25))];
    let mut b = battle(spec, |arena| {
        let script = format!(
            "fn on_spawn(p) {{ let me = player(p); if !me.bot {{ return; }} \
             for a in bricks(\"{ARMORY}\") {{ \
             if a.owner == me.bot_owner {{ give_item(p, \"{GUN}\", true); }} }} }}"
        );
        arena.package(
            "gauntlet-survival",
            &["player"],
            serde_json::json!({"on_spawn": true}),
            &script,
        );
    });
    let r = b.play(0.0, 60, |_, _| {});
    assert!(
        r.kills >= 35 * rounds() as u64,
        "the horde and the survivors fought: {}",
        r.kills
    );
    // Measured at the merge: circling 22% (melee circles its target), 33
    // changes a bot-minute (chase/fly flip-flop), 47 kills. Now melee
    // closes: circling 5%, 25 changes, 48 kills.
    within(&r, 0.01, 0.01, 0.08, 32.0);
    assert_eq!(r.team_kills + r.at_ally, 0, "no fire on its own side");
}

#[test]
fn capture_the_flag() {
    const FLAG: &str = "gauntlet-ctf:brick/flag";
    const BASE: &str = "gauntlet-ctf:brick/base";
    const FLAG_ITEM: &str = "gauntlet-ctf:weapon/flag";
    const FLAG_IMAGE: &str = "gauntlet-ctf:image/flag";
    let mut spec = Spec::new(
        "capture_the_flag",
        line(-76.0, 0.0, 50.0, 2),
        line(-36.0, 0.0, 50.0, 2),
        &[GUN],
    );
    spec.brick_defs = vec![(FLAG, fixture::PLATE), (BASE, fixture::PLATE)];
    spec.item_defs = vec![(FLAG_ITEM, FLAG_IMAGE)];
    spec.bricks = vec![
        brick(FLAG, Vec3::new(-79.75, 0.1, 50.25)),
        brick(BASE, Vec3::new(-79.75, 0.1, 44.25)),
    ];
    spec.blue_bricks = vec![
        brick(FLAG, Vec3::new(-32.25, 0.1, 50.25)),
        brick(BASE, Vec3::new(-32.25, 0.1, 56.25)),
    ];
    let script = r#"
fn flag() { "gauntlet-ctf:weapon/flag" }
fn image() { "gauntlet-ctf:image/flag" }
fn bump(id) {
    let e = get("epoch"); let k = `${id}`;
    let n = e[k]; if n == () { n = 0; }
    e[k] = n + 1; set("epoch", e);
}
fn on_tick() {
    if get("ready") != 0 { return; }
    let all = bricks("gauntlet-ctf:brick/flag");
    if all.len() < 2 { return; }
    for b in all { if b.game == () { return; } }
    for b in all { set_brick_item(b.id, flag()); bump(b.id); }
    set("ready", 1);
}
fn on_spawn(p) {
    let held = get_player(p, "burden");
    if held == () { return; }
    set_player(p, "burden", ()); mount_image(p, (), 2);
    set_brick_item(held, flag()); bump(held);
}
fn on_pickup(p, it, info) {
    if it != flag() { return; }
    let me = player(p);
    if !me.bot || me.minigame == () || info.spawner == () || get_player(p, "burden") != () { return false; }
    let b = brick(info.spawner);
    if b.owner == me.bot_owner || b.game != me.minigame { return false; }
    set_player(p, "burden", b.id);
    mount_image(p, image(), 2);
    set_brick_item(b.id, ());
    false
}
fn on_zone(p, b, event) {
    let me = player(p);
    if event != "enter" || !me.bot || me.minigame == () { return; }
    let held = get_player(p, "burden");
    if held == () || brick(b).owner != me.bot_owner { return; }
    add_player(p, "done", 1);
    add_score(p, 1);
    set_player(p, "burden", ()); mount_image(p, (), 2);
    set_brick_item(held, flag()); bump(held);
}
fn bot_objectives(p) {
    let me = player(p);
    if !me.bot || me.minigame == () || get("ready") == 0 { return []; }
    let homes = [];
    for d in bricks("gauntlet-ctf:brick/base") { if d.owner == me.bot_owner { homes.push(d.id); } }
    let out = [];
    for src in bricks("gauntlet-ctf:brick/flag") {
        if src.owner == me.bot_owner || src.game != me.minigame { continue; }
        out.push(#{kind: "carry_return", id: `flag-${src.id}`,
            source: #{kind: "brick", brick: src.id}, item: flag(),
            epoch: #{scope: "global", key: "epoch", path: [`${src.id}`]},
            destinations: homes,
            carriage: #{key: "burden", worn: #{slot: 2, image: image()}},
            completion: #{scope: "player", key: "done", path: []}});
    }
    out
}
"#;
    let mut b = battle(spec, |arena| {
        arena.package(
            "gauntlet-ctf",
            &["minigame", "player", "world.edit"],
            serde_json::json!({
                "bot_objectives": true, "on_pickup": true, "on_spawn": true,
                "zones": [{"bricks": [BASE], "above": 0.4, "period_ms": 50}],
                "tick_interval": 30,
                "state": {
                    "global": {
                        "epoch": {"default": {}, "visible": "everyone", "persist": false},
                        "ready": {"default": 0, "visible": "everyone", "persist": false}
                    },
                    "player": {
                        "burden": {"default": null, "visible": "everyone", "persist": false},
                        "done": {"default": 0, "visible": "everyone", "persist": false}
                    }
                }
            }),
            script,
        );
    });
    let sides = b.sides.clone();
    let r = b.play(0.0, 90, |s, report| {
        let state = s.package_state();
        let Some(ns) = state.packages.get("gauntlet-ctf") else {
            return;
        };
        for side in 0..2u32 {
            let caps: i64 = sides
                .iter()
                .filter(|(_, x)| **x == side)
                .filter_map(|(o, _)| ns.players.get(o)?.get("done")?.as_i64())
                .sum();
            report.progress.insert(format!("captures_side{side}"), caps);
        }
        let carrying = sides
            .keys()
            .filter(|o| {
                ns.players
                    .get(o)
                    .and_then(|p| p.get("burden"))
                    .is_some_and(|v| !v.is_null())
            })
            .count() as i64;
        *report.progress.entry("carry_ticks".into()).or_default() += carrying;
    });
    // Measured at the merge: stuck 34.5% (opposing runners deadlock
    // head-on), circling 1%, 39 changes a bot-minute, 34 kills, 1656 ticks
    // carrying, and no capture in 90 s. Now, with fights held through a
    // jump: stuck 0.4-20%, 43-53 kills. The changes a bot-minute rose with
    // the fighting, to 44-60 (each fight is an objective/fight change and
    // back). TARGET captures > 0.
    within(&r, 0.25, 0.01, 0.03, 65.0);
    assert!(
        r.progress["carry_ticks"] >= 1000 * rounds() as i64,
        "flags were taken: {:?}",
        r.progress
    );
    assert_eq!(r.team_kills + r.at_ally, 0, "no fire on its own side");
}

#[test]
fn checkpoint_race() {
    // The Workshop's own race recipe, built by the map's spawn point; four
    // bots of one builder start beside it.
    let mut arena = Arena::new(&[]);
    arena
        .s
        .set_spawn_points(vec![Vec3::new(-60.0, 0.05, 40.0)])
        .unwrap();
    // The builder plays too: it stands far away, out of the race.
    let owner = arena.join("Race builder", FAR_A);
    let starts = line(-66.0, 0.0, 40.0, 4);
    arena.load(
        owner,
        starts.iter().map(|at| spawner(fixture::BOT, *at)).collect(),
    );
    arena.command(
        owner,
        bri_sim::session::Command::Package(bri_sim::session::PackageCommand {
            package: String::new(),
            command: "rulelab".into(),
            args: vec![bri_sim::session::PackageArg::String("race".into())],
        }),
    );
    // The recipe made its mini-game; the builder then respawns far from
    // the track as the round restarts.
    arena.step(60);
    arena.s.set_spawn_points(vec![FAR_A]).unwrap();
    arena.command(
        owner,
        bri_sim::session::Command::MiniGame(bri_sim::session::MiniGameRequest::Reset),
    );
    arena.step(120);
    let bots = arena.bots();
    assert_eq!(bots.len(), 4, "every racer spawned");
    let sides: BTreeMap<OwnerId, u32> = bots.iter().map(|b| (*b, 0)).collect();
    let mut scorer = Scorer::new("checkpoint_race", 0.0);
    let mut won_at = None;
    for tick in 0..90 * TICKS_PER_SECOND * rounds() {
        arena.step(1);
        // Racing is the work: any strolling is idling.
        scorer.sample(&arena.s, &sides, |_| true);
        if won_at.is_none() && arena.s.round_results().next().is_some() {
            // Three laps won the round: the race is over.
            won_at = Some(tick as i64 / TICKS_PER_SECOND as i64);
            break;
        }
    }
    scorer
        .report
        .progress
        .insert("won_after_seconds".into(), won_at.unwrap_or(-1));
    let vitals = arena.s.vitals();
    let laps: Vec<i64> = bots.iter().map(|b| vitals[b].score).collect();
    scorer
        .report
        .progress
        .insert("laps_total".into(), laps.iter().sum());
    scorer.report.progress.insert(
        "racers_with_a_lap".into(),
        laps.iter().filter(|l| **l > 0).count() as i64,
    );
    scorer.report.print();
    let r = scorer.report;

    // Measured: stuck 0.5%, idle 0.2%, circling 14% (out-and-back track),
    // 54 behaviour changes a bot-minute (objective blinks off for a tick at
    // each checkpoint), won after 15 s with 9 laps run.
    within(&r, 0.01, 0.01, 0.17, 62.0);
    let won = r.progress["won_after_seconds"];
    assert!((0..=20).contains(&won), "the race was won in time: {won}");
    assert!(r.progress["laps_total"] >= 9, "laps run: {:?}", r.progress);
}
