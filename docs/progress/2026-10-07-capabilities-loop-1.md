# 2026-10-07 Gameplay capabilities: plan and loop 1

Branch `claude/v0.2.6-capabilities-8gived`, off main 2cf0b3a4. Max approved the
plan in `docs/audits/gameplay-capabilities.md` (catalogue of every place that
recognises content by name, Max's five buckets, the capability / attempt /
outcome vocabulary, four loops and the finish line) and loop 1.

## Loop 1: weapon reading, bots only

- The bots' two spray-can id checks (`surprise.rs` goof trigger, `team.rs`
  "that player is spraying") ask `tools::image_paints` instead: does the held
  image's shot paint, read from the existing paint rule. The FX cans now
  count as spraying too; the old id check only matched the colour can.
- Trigger style is not derived from image state machines: that would make
  bots hold the trigger on automatic weapons and the hammer instead of
  clicking, which a player would see.
- `onFireAkimbo` stays: it is a native engine state script, not a content
  name (bucket 4).
- The bot `Weapon` view, `tactics::Capability` and `BotUse` are not copies of
  one another. The ranged standoff rule that was written twice is now one
  function, `Weapon::standoff`.
- The unnamed numbers in the weapon band and reach rules are named constants,
  and tick conversions use `bri_weapons::TICK_HZ`. Values are unchanged.

## Evidence

- `cargo clippy -p bri-sim --all-targets -- -D warnings`: clean.
- `cargo test -p bri-sim --lib`: 294 passed, 2 ignored (plus the new
  `an_image_paints_by_what_its_shot_does_not_by_its_name`).
- `cargo test -p bri-chaos --test bot_watch --test bot_extras --test bot_brain`:
  all pass.
- Not run in the cloud: the full workspace gate (PC gate at merge time).

## Next

Loop 2: declare the building tools (`host_tool`) on the stock images through
the importer, then delete `NativeTool`'s id match, `HOST_TOOL_IMAGES`,
`native_hammer`'s name check, the wrench-dialog item check and the client
`Equipment` mapping. New fields are marked not stable until the modding API
freeze.
