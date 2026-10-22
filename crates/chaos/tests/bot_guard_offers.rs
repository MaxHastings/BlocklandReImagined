//! A changing package objective must release a guard without a timeout or death.
mod gauntlet;

use bri_chaos::fixture;
use bri_sim::session::{Command, PackageCommand};
use gauntlet::{Arena, spawner};
use glam::Vec3;
use serde_json::json;

#[test]
fn guards_share_live_posts_and_leave_a_withdrawn_post() {
    let mut arena = Arena::new(&[]);
    arena.package(
        "changing-posts",
        &["minigame", "player"],
        json!({"bot_objectives": true, "commands": [{"name": "close"}], "state": {
            "global": {"closed": {"default": false, "visible": "everyone", "persist": false}}
        }}),
        r#"
fn cmd_close(p) { set("closed", true); }
fn bot_objectives(p) {
    if !player(p).bot || player(p).minigame == () { return []; }
    let out = [];
    if !get("closed") {
        out.push(#{kind: "guard", id: "near", at: [0.0, 0.0, 0.0], radius: 2.0, claimants: 1});
    }
    out.push(#{kind: "guard", id: "far", at: [14.0, 0.0, 0.0], radius: 2.0, claimants: 1});
    out
}
"#,
    );
    let owner = arena.join("Author", Vec3::new(80.0, 0.05, 80.0));
    arena.load(
        owner,
        vec![
            spawner(fixture::BOT, Vec3::new(-4.0, 0.1, 0.0)),
            spawner(fixture::BOT, Vec3::new(-4.0, 0.1, 4.0)),
        ],
    );
    arena.minigame(
        owner,
        bri_minigames::Settings {
            loadout: Default::default(),
            ..Default::default()
        },
    );
    arena.step(1200);
    let posts: Vec<_> = arena
        .s
        .bot_thoughts()
        .into_iter()
        .filter_map(|t| t.objective_detail.map(|d| d.desired))
        .collect();
    assert_eq!(
        posts.iter().filter(|d| d.ends_with(":near")).count(),
        1,
        "{posts:?}"
    );
    assert_eq!(
        posts.iter().filter(|d| d.ends_with(":far")).count(),
        1,
        "{posts:?}"
    );
    arena.command(
        owner,
        Command::Package(PackageCommand {
            package: "changing-posts".into(),
            command: "close".into(),
            args: vec![],
        }),
    );
    // Simulation ticks, not a wall-clock allowance. A normal discovery turn
    // must see the withdrawal while both soldiers are still alive.
    arena.step(360);
    let thoughts = arena.s.bot_thoughts();
    assert!(
        thoughts.iter().all(|t| t
            .objective_detail
            .as_ref()
            .is_none_or(|d| !d.desired.ends_with(":near"))),
        "{thoughts:?}"
    );
    assert_eq!(
        thoughts
            .iter()
            .filter(|t| t
                .objective_detail
                .as_ref()
                .is_some_and(|d| d.desired.ends_with(":far")))
            .count(),
        1
    );
    assert!(arena.s.take_package_problems().is_empty());
    assert_eq!(arena.s.bot_step_failures().0, 0);
}
