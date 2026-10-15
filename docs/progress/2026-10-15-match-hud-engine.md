# 2026-10-15 Match HUD and game-flow engine capabilities (v0.3.1)

For the v0.3.1 vertical slice (a rifle, a Spartan body, a Warthog, the Blood
Gulch map and its HUD), the engine gained the match capabilities that slice
needs, all general and Add-On driven; nothing in the engine names a game.

- **Kill feed.** A death inside a mini-game reaches its members as
  `Notice::Kill` (victim, killer, damage type, and the old chat line). A HUD
  layout's `kill_feed` element draws the newest lines with per-damage-type
  icons and team colours; while one is drawn, deaths stay out of chat.
  Deaths outside mini-games are chat lines as before.
- **Chat region.** A `chat` HUD region; claiming it hides chat's passing
  lines (typing or paging up shows them).
- **Stock prompt texts.** A layout's `texts` replaces or hides (`""`)
  `click_to_respawn`, `respawn_countdown`, `respawn_countdown_one` and
  `building_disabled`; a `respawn` value lets it draw its own countdown.
  `when.minigame` limits a layout to playing in a game (or not).
- **Motion tracker.** A `tracker` element: blips by bearing and range,
  per-side textures and tints, above/below blips, hidden while still or
  crouch-walking, shown for a while after firing (`Vitals::firing`,
  `Vitals::fired_tick`). Built each frame in `bri_client::match_hud`.
- **Spawn protection** is a mini-game setting (`spawn_protection_ms`, mode
  `spawn_protection_seconds`, 0 = off), replacing the fixed 2.5 s.
- **Abandoned vehicles** from spawn bricks return as fresh ones after
  sitting empty away from the brick: vehicle `return_seconds`, overridden by
  the game's `vehicle_return_ms` (mode `vehicle_return_seconds`).
- **Death clips and death view.** The host records the side a killing hit
  came from and a roll (`Vitals::death`); an archetype's `deaths` maps sides
  to its own clips so every client plays the same one. A game's
  `death_view_ms` holds the dead player's camera on their body.
- **Ambient beds.** `play_ambient[_all]` / `stop_ambient[_all]` loop keyed
  sounds on the effects bus beside rule music, placed or at the ears, for
  everyone including joiners, up to 16.

Wire changes: five files in `crates/net/protocol-changes/`
(`match-rules-settings`, `vitals-death-pose`, `kill-feed-notice`,
`rule-ambient`, `archetype-deaths-vehicle-return`).

Evidence (Linux container, no GPU): `cargo test` for bri-content,
bri-minigames, bri-vehicles, bri-package-runtime, bri-audio, bri-motor and
bri-sim libs pass; bri-client and bri-ui libs pass apart from tests that need
a wgpu adapter ("no wgpu adapter"). New tests pass:
`hud::tests::{a_kill_feed_…, a_tracker_…, stock_texts_…, when_can_ask_for_a_minigame}`,
`match_hud::tests`, `combat::tests::a_hit_side_…`,
`tests/combat.rs::{spawn_protection_follows_the_game_and_can_be_off, a_game_death_is_a_kill_notice_with_its_side}`,
`tests/script_api.rs::rules_play_ambient_beds_for_everyone_until_stopped`,
`tests/vehicles.rs::an_abandoned_vehicle_returns_to_its_spawn::synthetic`.
`cargo clippy --workspace --all-targets -- -D warnings` is clean.

Not done (out of the slice): a wasm `contacts` call for client-code HUDs and
a grenade count of a non-held item. No interactive playtest was run; Maxwell
plays the slice.
