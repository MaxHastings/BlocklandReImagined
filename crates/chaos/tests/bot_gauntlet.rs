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
            timed_step(&mut self.arena, &mut scorer.report);
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
        finish(&scorer.report);
        scorer.report
    }
}

/// The report's shares, printed for reading against main; only the
/// bizarre fails: bots inside allies, stuck, idle or circling most of the
/// match, dithering between behaviours twice a second, or most shots off
/// their target.
fn sane(r: &Report) {
    eprintln!(
        "{}: stuck {:.3}, idle {:.3}, circling {:.3}, clumped {:.3}, {:.1} switches/min, {} of {} shots off target, {} kills",
        r.name,
        r.share(r.stuck),
        r.share(r.idle),
        r.share(r.circling),
        r.share(r.clumped),
        r.per_bot_minute(r.switches),
        r.off_target,
        r.shots,
        r.kills
    );
    for (what, share) in [
        ("clumped", r.share(r.clumped)),
        ("stuck", r.share(r.stuck)),
        ("idle", r.share(r.idle)),
        ("circling", r.share(r.circling)),
    ] {
        assert!(share < 0.5, "{}: {what} {share:.3}", r.name);
    }
    assert!(
        r.off_target * 2 <= r.shots,
        "{}: {} of {} shots off target",
        r.name,
        r.off_target,
        r.shots
    );
    assert!(
        r.per_bot_minute(r.switches) < 120.0,
        "{}: behaviour switches {:.1}/min",
        r.name,
        r.per_bot_minute(r.switches)
    );
    assert_eq!(
        r.bad_plans, 0,
        "{}: no planned shot hurts its side as much as its enemies or kills a teammate",
        r.name
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
    // With human aim (`perception`) and dodge hops (`extras`) 41 kills
    // where nearly every shot landing gave 69; 40 is still a fight.
    assert!(r.kills > 0, "a real fight: {}", r.kills);
    sane(&r);
    own_side_report(&r);
}

/// Max's soccer save with its slot-1 kit (2026-10-05 watch loop): four a
/// side with swords on open ground. Bots close and swing, not dither
/// between chasing and wandering, and never wander with an enemy about.
#[test]
fn swords_four_a_side() {
    let spec = Spec::new(
        "swords_four_a_side",
        line(-70.0, 0.0, 50.0, 4),
        line(-40.0, 0.0, 50.0, 4),
        &[bri_weapons::testing::SWORD_ITEM],
    );
    let mut b = battle(spec, |_| {});
    let r = b.play(0.0, 60, |_, _| {});
    eprintln!("transitions {:?}", r.transitions);
    assert!(r.kills > 0, "a real fight: {}", r.kills);
    sane(&r);
    own_side_report(&r);
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
    // With human aim (`perception`: the steady hit rate in the fair band,
    // not every shot landing) 48 kills where 67 were; 40 is still a fight.
    assert!(r.kills > 0, "a real fight: {}", r.kills);
    sane(&r);
    own_side_report(&r);
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
    assert!(r.kills > 0, "a real fight: {}", r.kills);
    sane(&r);
    // Its own doing only: an enemy's shot that knocks it off is a shove
    // that worked (`knocked_off`, printed above).
    assert_eq!(r.fell, 0, "no bot strafed off the deck");
    own_side_report(&r);
}

/// Push brooms only, on a deck high enough that a fall from it kills, one
/// side with its back to the edge: a push is the one attack, worth the fall
/// it sends its target into. Over three seeds, pushes the bots plan with
/// worth (read from their readout) are fired (the acceptance run once saw
/// no push fired at all). How many knock an enemy off is printed: the edge
/// side steps in off its edge within a second, and from then on a push
/// lands its target on the deck, worth nothing, so few are planned; that a
/// planned push knocks its target off is `shove`'s
/// `a_push_only_weapon_at_an_enemy_by_a_drop_is_worth_firing_and_fires`.
/// Lining a push up is v0.2.7.
#[test]
fn push_brooms_on_a_high_deck() {
    let (mut planned, mut fired, mut lapsed, mut off) = (0, 0, 0, 0);
    let mut refused: BTreeMap<&str, u64> = BTreeMap::new();
    let mut unplanned: BTreeMap<&str, u64> = BTreeMap::new();
    for seed in 0..3 {
        let r = gauntlet::tuning::with_seed(seed, broom_deck);
        own_side_report(&r);
        planned += r.push_planned;
        fired += r.planned_push_fired;
        lapsed += r.push_lapsed;
        off += r.knocked_off;
        for (why, n) in &r.push_refused {
            *refused.entry(why).or_default() += n;
        }
        for (why, n) in &r.push_unplanned {
            *unplanned.entry(why).or_default() += n;
        }
    }
    let refusals: u64 = refused.values().sum();
    eprintln!(
        "push brooms: {planned} planned with worth: {fired} fired, refused {refused:?},          {lapsed} lapsed; unplanned pushes {unplanned:?}; {off} knocked off"
    );
    assert!(planned > 0, "pushes planned with worth: {planned}");
    // Every plan ends one way; a plan still open when the match ends is
    // the only one not counted.
    assert!(
        planned >= fired + refusals + lapsed && planned <= fired + refusals + lapsed + 6,
        "every plan accounted for"
    );
    assert!(
        unplanned.is_empty(),
        "every push fired was planned with worth: {unplanned:?}"
    );
}

fn broom_deck() -> Report {
    const TOP: f32 = 23.0;
    let mut spec = Spec::new(
        "push_brooms_on_a_high_deck",
        // One side with its back to the deck's -x edge (at x = -79), the
        // other a few steps in front of it.
        line(-78.3, TOP, 48.0, 3),
        line(-74.0, TOP, 48.0, 3),
        &[bri_weapons::testing::BROOM_ITEM],
    );
    spec.bricks = floor(Vec3::new(-79.0, 0.0, 40.0), [3, 2], TOP);
    spec.settings.falling_damage = true;
    spec.settings.player_type = NO_JETS.into();
    battle(spec, |_| {}).play(TOP, 60, |_, _| {})
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
    sane(&r);
    assert!(r.kills > 0, "the deck was taken: {}", r.kills);
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
    sane(&r);
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
    sane(&r);
    assert!(
        r.progress["mounted_ticks"] > 0,
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
    sane(&r);
    assert!(
        // The bots that ever armed: the same four however long it runs.
        r.progress["armed_bots"] == 4,
        "every bot armed itself: {:?}",
        r.progress
    );
    assert!(r.kills > 0, "armed, they fought: {}", r.kills);
}

/// A gun on a block a step high under a roof low enough that only a
/// crouched body gets on: the unarmed bots step up crouched to it and arm.
/// Near the gun across, a bot still on the way up is not yet at it, so if
/// it gets nowhere it is stuck (hops, plans again), never left standing.
#[test]
fn a_weapon_in_a_cubby_a_step_up_is_fetched() {
    let mut spec = Spec::new(
        "a_weapon_in_a_cubby_a_step_up_is_fetched",
        line(-74.0, 0.0, 50.0, 1),
        line(-46.0, 0.0, 50.0, 1),
        &[],
    );
    let (x, z) = (-60.0, 50.0);
    let mut bricks = Vec::new();
    // Five baseplates make the block a step high; one more is the roof,
    // 2.0 over the block (a crawlspace) and 3.0 over the floor round it.
    for k in 0..5 {
        let mut b = brick(fixture::BASEPLATE, Vec3::new(x, 0.1 + k as f32 * 0.2, z));
        if k == 4 {
            b.item_spawn.item = Some(bri_world::ContentRef::Resolved(GUN.to_string()));
        }
        bricks.push(b);
    }
    bricks.push(brick(fixture::BASEPLATE, Vec3::new(x, 3.1, z)));
    spec.bricks = bricks;
    let mut b = battle(spec, |_| {});
    let r = b.play(0.0, 30, |s, report| {
        let armed = s
            .tool_inventories()
            .iter()
            .filter(|(o, inv)| s.is_bot(**o) && inv.slots.iter().any(|x| x.is_some()))
            .count() as i64;
        let best = report.progress.get("armed_bots").copied().unwrap_or(0);
        report.progress.insert("armed_bots".into(), best.max(armed));
    });
    assert!(
        r.progress["armed_bots"] >= 1,
        "a bot armed: {:?}",
        r.progress
    );
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
        r.kills > 0,
        "the horde and the survivors fought: {}",
        r.kills
    );
    sane(&r);
    own_side_report(&r);
}

#[test]
fn capture_the_flag() {
    let spec = Spec::new(
        "capture_the_flag",
        line(-76.0, 0.0, 50.0, 2),
        line(-36.0, 0.0, 50.0, 2),
        &[GUN],
    );
    let r = flags(spec, NEAR_POSTS, 90);
    sane(&r);
    let caps = r.progress["captures_side0"] + r.progress["captures_side1"];
    assert!(caps > 0, "flags were run home: {:?}", r.progress);
    own_side_report(&r);
}

/// Harm to a bot's own side and itself over a whole match. Team and self
/// kills are printed for reading against main, not asserted: who steps
/// into whose line of fire is how a seeded match happens to go. The
/// mechanisms they rest on are held where they are deterministic: no
/// planned shot trades its side for less (`sane`'s `bad_plans`, every tick
/// here), what a shot does to each body (`bots/harm.rs`), the fire gate
/// sparing its side (`combat.rs`'s
/// `an_unplanned_press_fires_only_when_it_spares_its_own_side`), and a
/// blast fired only as a trade its holder wins (`bot_tactics.rs`). Still
/// asserted, with a wide margin today: each side hurts its enemies more
/// than its own, which a mass friendly-fire regression `bad_plans` cannot
/// see (melee into allies, unplanned presses, aim error) would break.
fn own_side_report(r: &Report) {
    eprintln!(
        "{}: {} team kills, {} self kills, team damage {:.0}, {} shots at an ally",
        r.name, r.team_kills, r.self_kills, r.team_damage, r.at_ally,
    );
    r.each_side_hurts_its_own_less().unwrap();
}

/// Runners and nothing else: no weapon and no fighting, each side's flag
/// on the lane the other side runs down, so they meet head-on.
#[test]
fn runners_cross_head_on() {
    const RUNNER: &str = "gauntlet:bot/runner";
    let mut spec = Spec::new(
        "runners_cross_head_on",
        line(-76.0, 0.0, 50.0, 1),
        line(-36.0, 0.0, 50.0, 1),
        &[],
    );
    spec.kinds = [RUNNER, RUNNER];
    spec.extra_kinds = vec![kind(RUNNER, |k| {
        for b in ["fight", "chase", "search", "arm", "fly", "return"] {
            k.behaviours.insert(b.into(), 0.0);
        }
    })];
    let r = flags(spec, NEAR_POSTS, 60);
    sane(&r);
    let caps = r.progress["captures_side0"] + r.progress["captures_side1"];
    assert!(caps > 0, "both ran it home: {:?}", r.progress);
}

/// Where each side's flag and base stand: red flag, red base, blue flag,
/// blue base.
type Posts = [Vec3; 4];
/// Each flag beside the other side's spawns, each base behind its own.
const NEAR_POSTS: Posts = [
    Vec3::new(-79.75, 0.1, 50.25),
    Vec3::new(-79.75, 0.1, 44.25),
    Vec3::new(-32.25, 0.1, 50.25),
    Vec3::new(-32.25, 0.1, 56.25),
];

/// A run longer than a fixed approach timeout (30 s): runners spawn by
/// their base in one corner of the floor, and the flag they take stands in
/// the opposite corner, about 245 units off, 35 s at a run.
#[test]
fn a_run_longer_than_the_approach_timeout() {
    const RUNNER: &str = "gauntlet:bot/runner";
    let mut spec = Spec::new(
        "a_run_longer_than_the_approach_timeout",
        vec![Vec3::new(-88.0, 0.0, -84.0)],
        vec![Vec3::new(88.0, 0.0, 84.0)],
        &[],
    );
    spec.kinds = [RUNNER, RUNNER];
    spec.extra_kinds = vec![kind(RUNNER, |k| {
        for b in ["fight", "chase", "search", "arm", "fly", "return"] {
            k.behaviours.insert(b.into(), 0.0);
        }
    })];
    // Each side's flag and base by its own spawn: a runner crosses the
    // floor for the other side's flag and back.
    let posts = [
        Vec3::new(-91.75, 0.1, -80.25),
        Vec3::new(-91.75, 0.1, -88.25),
        Vec3::new(91.75, 0.1, 80.25),
        Vec3::new(91.75, 0.1, 88.25),
    ];
    let r = flags(spec, posts, 120);
    sane(&r);
    let caps = r.progress["captures_side0"] + r.progress["captures_side1"];
    assert!(caps > 0, "both ran the far flag home: {:?}", r.progress);
}

/// Capture the flag in `spec`'s layout and arsenal, for `seconds`: each
/// side takes the other's flag from `posts` back to its base.
fn flags(mut spec: Spec, posts: Posts, seconds: usize) -> Report {
    const FLAG: &str = "gauntlet-ctf:brick/flag";
    const BASE: &str = "gauntlet-ctf:brick/base";
    const FLAG_ITEM: &str = "gauntlet-ctf:weapon/flag";
    const FLAG_IMAGE: &str = "gauntlet-ctf:image/flag";
    spec.brick_defs = vec![(FLAG, fixture::PLATE), (BASE, fixture::PLATE)];
    spec.item_defs = vec![(FLAG_ITEM, FLAG_IMAGE)];
    spec.bricks = vec![brick(FLAG, posts[0]), brick(BASE, posts[1])];
    spec.blue_bricks = vec![brick(FLAG, posts[2]), brick(BASE, posts[3])];
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
    b.play(0.0, seconds, |s, report| {
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
    })
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
        timed_step(&mut arena, &mut scorer.report);
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
    finish(&scorer.report);
    let r = scorer.report;

    sane(&r);
    let won = r.progress["won_after_seconds"];
    assert!(won >= 0, "the race was won: {won}");
    assert!(r.progress["laps_total"] > 0, "laps run: {:?}", r.progress);
}

/// Bot surprise, measured but not yet held to bands
/// (`docs/architecture/bots.md`, "Surprise"): variety (distinct choices in
/// effect a bot-minute), goof share and the longest goof, at strengths 0,
/// 0.5 and 1, in a mixed-arsenal fight and at an idle pause. Strength 0 is
/// the plain brain: no goofing and no pick away from the plain one.
#[test]
fn surprise_by_strength() {
    use bri_weapons::testing::*;
    const RUNS: [(&str, &str, &str, f32); 3] = [
        (
            "gauntlet:bot/surprise0",
            "surprise_fight_0",
            "surprise_idle_0",
            0.0,
        ),
        (
            "gauntlet:bot/surprise5",
            "surprise_fight_0.5",
            "surprise_idle_0.5",
            0.5,
        ),
        (
            "gauntlet:bot/surprise10",
            "surprise_fight_1",
            "surprise_idle_1",
            1.0,
        ),
    ];
    for (id, fight, idle, strength) in RUNS {
        let surprising = || kind(id, |k| k.surprise.strength = strength);
        let mut spec = Spec::new(
            fight,
            line(-73.0, 0.0, 50.0, 3),
            line(-37.0, 0.0, 50.0, 3),
            &[ROCKET_ITEM, SHOTGUN_ITEM, BOW_ITEM, BOUNCER_ITEM, GUN],
        );
        spec.kinds = [id, id];
        spec.extra_kinds = vec![surprising()];
        let r = battle(spec, |_| {}).play(0.0, 60, |_, _| {});
        assert!(r.kills > 0, "{fight}: a real fight");
        // Three bots of one side and nobody to fight: a long pause.
        let mut spec = Spec::new(
            idle,
            line(-73.0, 0.0, 50.0, 3),
            Vec::new(),
            &[ROCKET_ITEM, GUN],
        );
        spec.kinds = [id, id];
        spec.extra_kinds = vec![surprising()];
        // The other builder has only a plate far off.
        spec.blue_bricks = floor(Vec3::new(60.0, 0.0, 60.0), [1, 1], 1.0);
        let quiet = battle(spec, |_| {}).play(0.0, 60, |_, _| {});
        if strength == 0.0 {
            assert_eq!(r.goof + quiet.goof, 0, "the plain brain does not goof");
            assert_eq!(r.surprised + quiet.surprised, 0, "nor varies its picks");
        } else {
            assert!(
                quiet.goof > 0,
                "{idle}: idle bots do something now and then"
            );
        }
    }
}

/// The scenarios the tuning tools replay (`gauntlet::tuning`): every one
/// above that plays the shipped kinds (`surprise_by_strength` sets its own
/// strengths).
const SCENARIOS: &[gauntlet::tuning::Scenario] = &[
    ("deathmatch_open_field", deathmatch_open_field),
    ("deathmatch_mixed_arsenal", deathmatch_mixed_arsenal),
    ("rooftop_brawl_without_rails", rooftop_brawl_without_rails),
    ("stairs_to_a_deck", stairs_to_a_deck),
    ("water_between_the_sides", water_between_the_sides),
    ("a_jeep_on_each_side", a_jeep_on_each_side),
    ("weapons_lying_on_the_ground", weapons_lying_on_the_ground),
    ("zombie_survival", zombie_survival),
    ("capture_the_flag", capture_the_flag),
    ("runners_cross_head_on", runners_cross_head_on),
    (
        "a_run_longer_than_the_approach_timeout",
        a_run_longer_than_the_approach_timeout,
    ),
    ("checkpoint_race", checkpoint_race),
];

/// The off-switch check: each top-level dial of `bots.json` at 0, one at a
/// time (a dial shipped at 0 is turned on instead), over the scenarios;
/// prints and writes `target/bot-tuning/ablation.{csv,txt}`, flagging a
/// dial whose change moves nothing as a cut candidate.
/// `cargo test --release -p bri-chaos --test bot_gauntlet off_switches -- --ignored --nocapture`
#[test]
#[ignore = "tuning tool: slow"]
fn off_switches() {
    eprintln!("{}", gauntlet::tuning::ablation(SCENARIOS));
}

/// The sweep: dial values over the scenarios, `BRI_TUNING_SEEDS` seeds a
/// point, scored against the bands and ranked; writes
/// `target/bot-tuning/sweep.csv` and `sweep_summary.txt`.
/// `cargo test --release -p bri-chaos --test bot_gauntlet dial_sweep -- --ignored --nocapture`
#[test]
#[ignore = "tuning tool: slow"]
fn dial_sweep() {
    eprintln!("{}", gauntlet::tuning::sweep(SCENARIOS));
}

/// Over seeds, no bot plans a shot that hurts its side as much as its
/// enemies, or that would kill a teammate (`sane`), with every kind of
/// weapon in hand.
#[test]
fn no_planned_shot_trades_its_side_for_less_over_seeds() {
    use gauntlet::tuning::{Setting, run_all};
    let plain = [Setting {
        label: "plain".into(),
        dials: Vec::new(),
    }];
    let runs = run_all(&[("mixed", deathmatch_mixed_arsenal)], &plain, 3);
    assert_eq!(runs.len(), 3);
    for (_, _, seed, m) in &runs {
        assert!(m.broken.is_none(), "seed {seed}: {:?}", m.broken);
    }
}

/// The tools' plumbing: a dial set for a run reaches the kinds its
/// scenario builds, a scenario's report is handed over, and a seed other
/// than 0 plays other random streams.
#[test]
fn tuning_dials_and_seeds_reach_the_scenario() {
    use gauntlet::tuning::{Setting, run_all};
    let settings = [
        Setting {
            label: "plain".into(),
            dials: vec![("surprise.strength".into(), 0.0)],
        },
        Setting {
            label: "on".into(),
            dials: vec![("surprise.strength".into(), 1.0)],
        },
    ];
    let runs = run_all(&[("probe", tuning_probe)], &settings, 2);
    assert_eq!(runs.len(), 4);
    for (_, _, _, m) in &runs {
        assert!(m.broken.is_none(), "{:?}", m.broken);
    }
    let goof = |setting: usize, seed: u64| {
        runs.iter()
            .find(|r| r.0 == setting && r.2 == seed)
            .unwrap()
            .3
            .goof
    };
    assert_eq!(
        goof(0, 0) + goof(0, 1),
        0.0,
        "the plain brain does not goof"
    );
    assert!(goof(1, 0) > 0.0, "strength 1 reached the kinds");
    assert_ne!(goof(1, 0), goof(1, 1), "another seed, other streams");
}

/// A seed other than 0 still lets a scenario install its package before
/// anyone joins: the seed players join just before the first builder, and
/// take the ids and ticks before it (`Arena::seed_players_join`).
#[test]
fn a_seeded_run_installs_its_package_before_anyone_joins() {
    let builder = gauntlet::tuning::with_seed(3, || {
        let mut arena = Arena::new(&[]);
        assert!(arena.humans.is_empty(), "nobody has joined yet");
        arena.package(
            "gauntlet-seeded",
            &["player"],
            serde_json::json!({"on_spawn": true}),
            "fn on_spawn(p) { }",
        );
        let builder = arena.join("Builder", FAR_A);
        assert_eq!(
            arena.humans.len(),
            4,
            "three seed players, then the builder"
        );
        assert_eq!(arena.humans.last(), Some(&builder));
        builder
    });
    let mut plain = Arena::new(&[]);
    assert_ne!(
        plain.join("Builder", FAR_A),
        builder,
        "the seed players took the first ids"
    );
}

/// The all-on run: every scenario with every dial at its ON value
/// together (`gauntlet::tuning::all_on`), with the share report. Every
/// scenario's own checks and every enforced band must hold: this is the
/// configuration that gates merges. Slow, so ignored by default; the push
/// gate runs ignored tests.
/// `cargo test -p bri-chaos --test bot_gauntlet all_dials_on -- --ignored --nocapture`
#[test]
#[ignore = "all-on gauntlet: slow; the push gate runs it"]
fn all_dials_on() {
    use gauntlet::tuning::{self, Config, Setting};
    let config = Config::shipped();
    let on = tuning::all_on(&config);
    eprintln!("ALL ON: {on:?}");
    let settings = [Setting {
        label: "all on".into(),
        dials: on,
    }];
    let runs = tuning::run_all(SCENARIOS, &settings, 1);
    let bands = gauntlet::shares::Bands::shipped();
    let mut broken = Vec::new();
    for (_, name, _, m) in &runs {
        eprintln!(
            "ALL ON {name}: variety {:.2} bits, goof {:.1}%, waves {:.3}, stuck {:.1}%, flags [{}]",
            m.variety,
            100.0 * m.goof,
            m.goof_waves,
            100.0 * m.stuck,
            m.flagged.join(", ")
        );
        for row in &m.rows {
            if row.share > 0.0 || row.flag.bad() {
                eprintln!(
                    "    {:<14} {:>5.1}%  {}",
                    row.kind,
                    100.0 * row.share,
                    row.flag.word()
                );
            }
        }
        if let Some(b) = &m.broken {
            broken.push(format!("{name}: {b}"));
        }
        for row in &m.rows {
            if let Some(band) = bands.band(name, &row.kind)
                && band.enforced.fails(row.flag)
            {
                broken.push(format!("{name}: {} {}", row.kind, row.flag.word()));
            }
        }
    }
    assert!(broken.is_empty(), "all on: {broken:#?}");
}

/// Bot think time (`Session::bot_think_nanos`, the ticking thread's CPU
/// time, so a loaded machine does not stretch it) per tick with the server's
/// most bots (16, `MAX_BOTS`) in the busiest scenario (a mixed-arsenal
/// deathmatch, eight a side) with every dial on, against the bar in
/// `bot_tuning.json` (`perf`: one for debug builds, one for release).
/// `cargo test --release -p bri-chaos --test bot_gauntlet bot_think_time_16 -- --ignored --nocapture`
#[test]
#[ignore = "timing; run in release"]
fn bot_think_time_16() {
    use bri_weapons::testing::*;
    use gauntlet::tuning::{self, Config};
    let config = Config::shipped();
    let us = tuning::with_dials(tuning::all_on(&config), || {
        let spec = Spec::new(
            "think_time_16",
            line(-73.0, 0.0, 50.0, 8),
            line(-37.0, 0.0, 50.0, 8),
            &[ROCKET_ITEM, SHOTGUN_ITEM, BOW_ITEM, BOUNCER_ITEM, GUN],
        );
        let mut b = battle(spec, |_| {});
        assert_eq!(b.sides.len(), bri_package_runtime::ops::MAX_BOTS);
        // Warm up (spawns, first plans), then time 30 s of fighting.
        b.arena.step(5 * TICKS_PER_SECOND);
        let ticks = 30 * TICKS_PER_SECOND;
        let before = b.arena.s.bot_think_nanos();
        let started = std::time::Instant::now();
        b.arena.step(ticks);
        let step_us = started.elapsed().as_secs_f64() * 1e6 / ticks as f64;
        let think = (b.arena.s.bot_think_nanos() - before) as f64 / 1000.0 / ticks as f64;
        eprintln!(
            "PERF 16 bots, all on: bot think {think:.0} us/tick ({:.1} us a bot), whole step {step_us:.0} us/tick",
            think / 16.0
        );
        think
    });
    let bar = if cfg!(debug_assertions) {
        config.perf.debug_us
    } else {
        config.perf.release_us
    };
    assert!(
        us <= bar,
        "bot think time {us:.0} us/tick over the bar of {bar:.0} us"
    );
}

/// Hits and shots (trigger ticks) in one phase of an engagement.
#[derive(Clone, Copy, Debug, Default)]
struct Rate {
    shots: u64,
    hits: u64,
}
impl Rate {
    fn share(self) -> Option<f32> {
        (self.shots > 0).then(|| self.hits as f32 / self.shots as f32)
    }
}
/// The fair metric's result for one weapon at one range.
#[derive(Clone, Debug, Default)]
struct Fair {
    /// The first seconds after the bot first sees its target, the middle,
    /// and steady state.
    first: Rate,
    middle: Rate,
    steady: Rate,
    engagements: u64,
    range_sum: f32,
}
impl Fair {
    fn add(&mut self, o: &Fair) {
        for (a, b) in [
            (&mut self.first, o.first),
            (&mut self.middle, o.middle),
            (&mut self.steady, o.steady),
        ] {
            a.shots += b.shots;
            a.hits += b.hits;
        }
        self.engagements += o.engagements;
        self.range_sum += o.range_sum;
    }
    fn shots(&self) -> u64 {
        self.first.shots + self.middle.shots + self.steady.shots
    }
}

/// One bot with `weapon` against a scripted player who strafes and jumps
/// like an average player, from about `range` away, for `seconds`. A
/// package takes the target's damage away and counts it (`on_damage`), so
/// the fight runs long enough for a steady state; an engagement starts
/// when the bot sees the target. A shot is a tick the bot fired on; it
/// hits when damage lands within 2.5 s, at most once, in the phase it was
/// fired in.
fn fair_run(weapon: &str, range: f32, seconds: usize) -> Fair {
    let config = gauntlet::tuning::Config::shipped().fair;
    let spawn = Vec3::new(10.0, 0.05, 40.0);
    let post = Vec3::new(40.0, 0.05, 10.0);
    // Bots fight every other player of a free-for-all, their builder too:
    // the builder walks out of sight before its bot is placed.
    let away = Vec3::new(-50.0, 0.05, 90.0);
    let mut arena = Arena::new(&[]);
    arena.s.set_spawn_points(vec![spawn]).unwrap();
    arena.package(
        "gauntlet-fair",
        &["player", "damage"],
        serde_json::json!({
            "on_damage": true,
            "state": {"global": {
                "hits": {"default": 0, "visible": "everyone", "persist": false}
            }}
        }),
        "fn on_damage(victim, attacker, amount, info) { \
         if player(victim).bot { return (); } \
         set(\"hits\", get(\"hits\") + 1); 0 }",
    );
    let builder = arena.join("Bot builder", FAR_A);
    let target = arena.join("Target", FAR_B);
    let game = arena.minigame(
        builder,
        Settings {
            loadout: loadout(&[weapon]),
            use_all_players_bricks: true,
            ..Default::default()
        },
    );
    arena.join_game(target, game);
    for _ in 0..30 * TICKS_PER_SECOND {
        let Some(at) = arena.feet(builder) else { break };
        let d = away - at;
        if Vec3::new(d.x, 0.0, d.z).length() < 2.0 {
            break;
        }
        arena.moves.insert(
            builder,
            bri_sim::player::MoveInput {
                forward: 1.0,
                yaw: d.x.atan2(-d.z),
                ..Default::default()
            },
        );
        arena.step(1);
    }
    arena.moves.remove(&builder);
    arena.load(builder, vec![spawner(fixture::BOT, post - Vec3::Z * range)]);
    arena.step(TICKS_PER_SECOND);
    let bots = arena.bots();
    assert_eq!(bots.len(), 1, "the bot spawned");
    let bot = bots[0];
    let mut b = Battle {
        name: "fair",
        sides: [(bot, 0)].into(),
        arena,
    };
    let ticks = TICKS_PER_SECOND as u64;
    let first_end = (config.first_seconds * ticks as f32) as u64;
    let steady_from = (config.steady_after * ticks as f32) as u64;
    let mut out = Fair::default();
    // A seeded generator for the strafes: legs of 0.4 to 1.2 s, a hop
    // every 1.5 to 3 s.
    let mut rng = 0x9E37_79B9_7F4A_7C15u64 ^ gauntlet::tuning::seed();
    let mut random = move || {
        rng = rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (rng >> 40) as f32 / (1u64 << 24) as f32
    };
    let (mut leg_until, mut side, mut hop_at, mut lean) = (0u64, 1.0f32, 0u64, 0.0f32);
    let mut seen_since: Option<u64> = None;
    let mut life = 0u64;
    let mut seen_shots = std::collections::BTreeSet::new();
    let mut last_fire = 0u64;
    // Shots in flight: when fired, and in which phase.
    let mut pending: std::collections::VecDeque<(u64, usize)> = Default::default();
    let mut hits_seen = 0i64;
    let trace = std::env::var("BRI_FAIR_TRACE").is_ok();
    for _ in 0..seconds * TICKS_PER_SECOND {
        let s = &b.arena.s;
        let tick = s.simulation().state().tick;
        let vitals = s.vitals();
        let v = &vitals[&target];
        if !v.alive {
            if tick >= v.respawn_tick && tick.is_multiple_of(30) {
                b.arena.command(target, bri_sim::session::Command::Respawn);
            }
            b.arena.moves.remove(&target);
            timed_step(&mut b.arena, &mut Default::default());
            continue;
        }
        if v.spawn_tick != life {
            life = v.spawn_tick;
            seen_since = None;
        }
        let feet = b.arena.feet(target).unwrap_or(post);
        let to_post = Vec3::new(post.x - feet.x, 0.0, post.z - feet.z);
        if tick >= leg_until {
            side = -side;
            leg_until = tick + ((0.4 + 0.8 * random()) * ticks as f32) as u64;
            lean = random() * 0.6 - 0.3;
        }
        let jump = tick >= hop_at;
        if jump {
            hop_at = tick + ((1.5 + 1.5 * random()) * ticks as f32) as u64;
        }
        // Facing -z (toward the bot): forward is -z, right is +x.
        let input = if to_post.length() > 2.0 {
            bri_sim::player::MoveInput {
                forward: unit(-to_post.z / 3.0),
                right: unit(to_post.x / 3.0 + 0.5 * side),
                jump,
                ..Default::default()
            }
        } else {
            bri_sim::player::MoveInput {
                forward: unit(lean - to_post.z * 0.3),
                right: unit(side + to_post.x * 0.3),
                jump,
                ..Default::default()
            }
        };
        b.arena.moves.insert(target, input);
        let thought = s.bot_thoughts().into_iter().find(|t| t.bot == bot);
        if trace && tick.is_multiple_of(120) {
            eprintln!(
                "FAIR T{tick} {weapon} bot {:?} target {feet:.1} {:?} {:?}",
                b.arena.feet(bot),
                thought.as_ref().map(|t| t.behaviour),
                thought.as_ref().map(|t| t.visible)
            );
        }
        if seen_since.is_none() && thought.is_some_and(|t| t.visible == Some(target)) {
            seen_since = Some(tick);
            out.engagements += 1;
        }
        timed_step(&mut b.arena, &mut Default::default());
        let s = &b.arena.s;
        let now = s.simulation().state().tick;
        let hits = s
            .package_state()
            .packages
            .get("gauntlet-fair")
            .and_then(|ns| ns.global.get("hits")?.as_i64())
            .unwrap_or(0);
        let landed = hits > hits_seen;
        hits_seen = hits;
        pending.retain(|(at, _)| now - at <= 300);
        if landed && let Some((_, phase)) = pending.pop_front() {
            [&mut out.first, &mut out.middle, &mut out.steady][phase].hits += 1;
        }
        let Some(since) = seen_since else {
            continue;
        };
        let age = now - since;
        let index = if age < first_end {
            0
        } else if age < steady_from {
            1
        } else {
            2
        };
        let fired = s
            .weapon_view()
            .fired()
            .filter(|p| p.source.0 == bot && seen_shots.insert(p.id))
            .count();
        if fired > 0 && now != last_fire {
            last_fire = now;
            [&mut out.first, &mut out.middle, &mut out.steady][index].shots += 1;
            pending.push_back((now, index));
            if let (Some(a), Some(c)) = (b.arena.feet(bot), b.arena.feet(target)) {
                out.range_sum += a.distance(c);
            }
        }
    }
    out
}

/// `v` within -1..=1 (a control's range).
#[allow(clippy::manual_clamp)]
fn unit(v: f32) -> f32 {
    v.max(-1.0).min(1.0)
}

/// The weapon classes the fair metric shoots with.
fn fair_weapons() -> [(&'static str, &'static str); 5] {
    use bri_weapons::testing::*;
    [
        ("gun", GUN),
        ("rocket", ROCKET_ITEM),
        ("shotgun", SHOTGUN_ITEM),
        ("bow", BOW_ITEM),
        ("bouncer", BOUNCER_ITEM),
    ]
}

/// Every weapon class at every configured range, with a table; returns
/// the total and whether its steady rate is out of band.
fn fair_table() -> (Fair, Vec<(&'static str, Fair)>, String) {
    let config = gauntlet::tuning::Config::shipped().fair;
    let pct = |r: Rate| {
        r.share()
            .map_or("   -  ".to_string(), |s| format!("{:>5.1}%", 100.0 * s))
    };
    let flag = |r: Rate| match r.share() {
        Some(s) if s > config.max => "TOO GOOD",
        Some(s) if s < config.min => "HOPELESS",
        Some(_) => "ok",
        None => "no shots",
    };
    let mut text = format!(
        "FAIR hit rate vs a strafing, hopping player (band {:.0}-{:.0}% steady):\n  {:<8} {:>5}  {:>6} {:>6} {:>6}  {:>5} {:>4}\n",
        100.0 * config.min,
        100.0 * config.max,
        "weapon",
        "range",
        "first",
        "middle",
        "steady",
        "shots",
        "eng"
    );
    let mut total = Fair::default();
    let mut classes = Vec::new();
    for (class, item) in fair_weapons() {
        let mut each = Fair::default();
        for range in &config.ranges {
            let f = fair_run(item, *range, config.seconds);
            text.push_str(&format!(
                "  {:<8} {:>5.1}  {} {} {}  {:>5} {:>4}  {}\n",
                class,
                if f.shots() > 0 {
                    f.range_sum / f.shots() as f32
                } else {
                    *range
                },
                pct(f.first),
                pct(f.middle),
                pct(f.steady),
                f.shots(),
                f.engagements,
                flag(f.steady)
            ));
            total.add(&f);
            each.add(&f);
        }
        classes.push((class, each));
    }
    text.push_str(&format!(
        "  {:<8} {:>5}  {} {} {}  {:>5} {:>4}  {}\n",
        "all",
        "",
        pct(total.first),
        pct(total.middle),
        pct(total.steady),
        total.shots(),
        total.engagements,
        flag(total.steady)
    ));
    (total, classes, text)
}

/// The fair metric: a bot's hit rate against a player who strafes and
/// hops, per weapon class and range, in the first seconds of a fight and
/// in steady state, against a band (`bot_tuning.json` `fair`): never near
/// perfect, never hopeless. Enforced for every class together and for the
/// gun and the bow each: the steady aim error (`perception`'s tracking
/// lag) keeps a strafing target from being hit every time.
#[test]
fn fair_hit_rate() {
    let config = gauntlet::tuning::Config::shipped().fair;
    let (total, classes, text) = fair_table();
    eprint!("{text}");
    assert!(total.shots() > 0, "the bots shot at the target");
    assert!(total.engagements > 0, "the bots saw the target");
    let band = config.min..=config.max;
    let steady = |f: &Fair| f.steady.share().unwrap_or(0.0);
    assert!(
        band.contains(&steady(&total)),
        "steady hit rate {:.1}% outside the band\n{text}",
        100.0 * steady(&total)
    );
    for (class, f) in classes.iter().filter(|(c, _)| matches!(*c, "gun" | "bow")) {
        assert!(
            band.contains(&steady(f)),
            "{class}: steady hit rate {:.1}% outside the band\n{text}",
            100.0 * steady(f)
        );
    }
}

/// The fair metric against the alertness dial (the first of
/// `bot_tuning.json`'s `fair.dials` that bots.json has): its value low,
/// shipped and high should move the steady hit rate the dial's way.
/// `cargo test -p bri-chaos --test bot_gauntlet fair_by_dial -- --ignored --nocapture`
#[test]
#[ignore = "tuning tool: slow"]
fn fair_by_dial() {
    use bri_sim::bot_kind::tuning as dials;
    let config = gauntlet::tuning::Config::shipped().fair;
    let kind = gauntlet::tuning::shipped_kind();
    let Some((dial, sign, base)) = config
        .dials
        .iter()
        .find_map(|d| Some((d.path.clone(), d.sign, dials::dial(&kind, &d.path)?)))
    else {
        panic!("no fair dial in bots.json: {:?}", config.dials);
    };
    let mut rates = Vec::new();
    for value in [base * 0.5, base, base * 2.0] {
        let total = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    gauntlet::tuning::with_dials(vec![(dial.clone(), value)], || fair_table().0)
                })
                .join()
                .unwrap()
        });
        let rate = total.steady.share().unwrap_or(0.0);
        eprintln!("FAIR {dial} = {value}: steady {:.1}%", 100.0 * rate);
        rates.push(rate);
    }
    let rising = rates.windows(2).all(|w| (w[1] - w[0]) * sign >= 0.0);
    eprintln!(
        "FAIR {dial}: {} (expects the hit rate to {} with it)",
        if rising {
            "predictable"
        } else {
            "NOT MONOTONE"
        },
        if sign > 0.0 { "rise" } else { "fall" }
    );
    assert!(
        rising,
        "{dial} moves the steady hit rate its way: {rates:?}"
    );
}

/// Three idle bots for 30 s (surprise's idle pause), for the test above.
fn tuning_probe() {
    use bri_weapons::testing::*;
    let mut spec = Spec::new(
        "tuning_probe",
        line(-73.0, 0.0, 50.0, 3),
        Vec::new(),
        &[ROCKET_ITEM, GUN],
    );
    spec.blue_bricks = floor(Vec3::new(60.0, 0.0, 60.0), [1, 1], 1.0);
    battle(spec, |_| {}).play(0.0, 30, |_, _| {});
}
