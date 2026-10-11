//! Bot hardening scenarios: places and company that made a bot do something a
//! player would not, each found by a probe, fixed, and kept here.
mod gauntlet;

use bri_minigames::Settings;
use bri_sim::bot_kind::BotKind;
use gauntlet::*;
use glam::Vec3;

const BRAWLER: &str = "hardening:bot/brawler";
const NO_JETS: &str = "v20.player.playernojet";
const FAR_A: Vec3 = Vec3::new(92.0, 0.05, 92.0);
const FAR_B: Vec3 = Vec3::new(92.0, 0.05, 86.0);

fn brawler() -> BotKind {
    let mut k = blockhead_kinds().remove(0);
    k.id = BRAWLER.into();
    k.name = BRAWLER.into();
    k.melee = Some(melee(20.0, 2.5));
    k
}

/// Allies in a file along a deck's edge: each steps round the one ahead,
/// and the side it picks is the edge's. None of them goes over. Without
/// the fix (a step round a body judged as the route's own, so a drop that
/// does not hurt is walked off) the four layouts lose five bots between them.
#[test]
fn allies_stepping_round_each_other_stay_on_a_deck() {
    let mut falls = 0;
    for (spacing, z) in [(1.0, 54.5), (1.0, 55.0), (1.25, 55.0), (2.0, 55.0)] {
        let mut arena = Arena::custom(&[], &[], &[]);
        arena.kinds(vec![brawler()], &[]);
        let red = arena.join("Red builder", FAR_A);
        let blue = arena.join("Blue builder", FAR_B);
        // A 24 x 16 deck six up, no rails; its south edge is z = 56.
        let mut bricks = floor(Vec3::new(-79.0, 0.0, 40.0), [3, 2], 6.0);
        let file: Vec<Vec3> = (0..4)
            .map(|i| Vec3::new(-76.0 + i as f32 * spacing, 6.0, z))
            .collect();
        for at in &file {
            bricks.push(spawner(BRAWLER, *at));
        }
        arena.load(red, bricks);
        let enemy = Vec3::new(-58.0, 6.0, 47.0);
        arena.load(blue, vec![spawner(BRAWLER, enemy)]);
        let settings = Settings {
            loadout: Default::default(),
            use_all_players_bricks: true,
            player_type: NO_JETS.into(),
            falling_damage: true,
            ..Default::default()
        };
        let game = arena.minigame(red, settings);
        arena.join_game(blue, game);
        arena.step(TICKS_PER_SECOND);
        let mut spawns: Vec<(Vec3, u32)> = file.iter().map(|at| (*at, 0)).collect();
        spawns.push((enemy, 1));
        let sides = arena.sides_by_spawn(&spawns);
        let mut scorer = Scorer::new("deck_file", 6.0);
        for _ in 0..40 * TICKS_PER_SECOND {
            arena.step(1);
            let vitals = arena.s.vitals();
            scorer.sample(&arena.s, &sides, |bot| {
                sides
                    .iter()
                    .any(|(o, side)| Some(side) != sides.get(&bot) && vitals[o].alive)
            });
        }
        let r = scorer.report;
        falls += r.fell;
        eprintln!("deck_file spacing {spacing} z {z}: fell {}", r.fell);
        assert!(r.kills > 0, "a real fight: {}", r.kills);
    }
    // A bot stepping off an ally's head can still carry its speed past the
    // edge, and where that happens differs between machines (physics floats):
    // one such fall in the four layouts is allowed. The bug cost 5 on the
    // machine it was found on.
    assert!(falls <= 1, "{falls} bots walked off the deck");
}
