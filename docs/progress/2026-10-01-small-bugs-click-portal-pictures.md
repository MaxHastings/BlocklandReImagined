# 2026-10-01 Three small bugs for v0.1.12 (branch `claude/project-thread-9cbbpg`)

- **Empty-handed click, then a tool.** v20's empty-hand click is also the
  held move trigger, so a tool taken out before letting go fires once it is
  ready. The host kept a held trigger with nothing in hand, but the client
  sends that click as `Activate`, which never pressed it. Now `Activate`
  also queues the press and `ActivateRelease` the release (no wire change).
  Guard: `bri-sim` `an_empty_handed_click_held_fires_the_tool_taken_out`
  (fails on main: no shot). `docs/audits/tools-v20-audit.md` row marked fixed.
- **Held items mid-portal.** A body part way through a portal drew on both
  sides (`portal_view::Straddle`), but what it held drew only on the near
  side. `MountPose` now carries the holder's straddle and `WorldItems` draws
  a held image cut at the opening plus a carried copy cut the other way,
  with per-instance clip planes. `app::body_straddle` is shared by the body
  and its items (small `app.rs` change). Guard: `bri-client` world_items
  `a_held_item_part_way_through_a_portal_draws_on_both_sides`. Skins drawn
  over a held item are not clipped yet (rare: only skinned items mid-portal).
- **Stock builds without a save picture.** Load Bricks shows `<name>.jpg`
  beside a save (`saveBricks`' screenshot). Converted stock worlds had none
  because `import_saves` left the `.jpg` behind. It now copies the picture
  beside each `.bls` (any case) to `<sha>.jpg` in the worlds pack
  (`bind_world_events` carries it), and `saves::Entry::picture` looks beside
  a converted original too. The pictures come from the player's own v20
  folder at import; nothing enters the repo. The worlds pack rebuilds on
  the next bootstrap because the convert sources changed. Guard:
  `saves::tests::converted_originals_show_the_picture_beside_them_in_the_worlds_pack`.
  Not verified against real v20 saves here (no v20 folder in this session).

Commands: `cargo test -p bri-sim`, `cargo test -p bri-client --lib --test
world_items --test app_flow` (GPU tests on lavapipe), `cargo clippy -p
bri-client -p bri-sim -p bri-convert --tests -- -D warnings`.
