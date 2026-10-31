# 2026-10-31 Suspend destroyed brick pickups without resetting their clocks

The existing static-item owner retained a destroyed brick in its contact index,
published its pickup and could grant it once its ordinary cooldown elapsed.
The real Session fake-kill/restore regression reproduces this: after 60 ticks
the hidden source is still in `weapon_view().static_items`. Its first fixture
was rejected for an off-grid one-stud plate; correcting the authored stud-centre
position exposes the actual failure. Private `destroyed-item-red*.txt` logs keep
both failures distinct.

Keep the existing item/absolute due-time once, with a bounded suspension set
owned by `ItemSpawners`. Destruction removes pickup contact and presentation,
without deleting the item clock. `present` supplies public views and bot arming;
`available` governs both ordinary pickups and package objective pickup points.
Restock/hide operations retain their explicit author intent while suspended.
Restoration reuses the original due-time, so elapsed delays stay elapsed and
remaining delays still apply. Permanent source removal clears suspension too.
Authored invisible/noncolliding sources remain valid: `damage_hidden`, rather
than rendering or collision flags, owns this decision. No wire/save field,
runtime migration, timer boost or original-content edit is introduced.

Recovered read-only v20 evidence is `.research/v20-dso/server/scripts/
allGameScripts-Vanilla.cs`: `setItem` at 11334 deletes/replaces its Item and
refuses a dead brick; `fakeKillBrick` at 17471 delegates fake death to the native
engine; `spawnItem` at 17524 refuses fake-dead sources. The latter guard already
exists in our event adapter and is preserved. The script alone does not prove
the native engine's complete static-item fake-death behavior. The retained-clock
policy here implements Maxwell's explicit destruction/respawn expectation and
the approved plan, without claiming an unverified native-engine match.

Checks:

- `cargo test --locked --profile playtest -p bri-sim --lib
  destroyed_source_suspends_pickup`: passes after the correction. Actual timed
  fake kill, nearby visitor, no grant/public item, unmodified independent drop,
  real brick restoration and subsequent pickup cover due-times on either side
  of restoration. The original failure passes without weakening its assertions.
- `cargo test --locked --profile playtest -p bri-sim --test items --test
  item_lifecycle --test item_hooks --test item_hook_removal`: 10/3/14/2 pass.
  A source starting destroyed, invisible restored source, explicit hide/restock,
  direct pickup refusal, removal and restored-source cleanup are covered too.
- `cargo clippy --locked --profile playtest -p bri-sim --all-targets --
  -D warnings`: passes. Private logs are `destroyed-item-{green,integration,
  clippy}.txt` in the release evidence diagnostics directory.

Remaining destruction acceptance: moving light/emitter transforms, correct
fragment/section parent, expiry/culling/respawn and observer rendering still
need implementation and verification. Adjacent reads identify native bot/vehicle
spawner reconciliation without a destruction guard and brick music selected
from a static position. Existing spawned actors must not be deleted merely to
suspend production; these consumers need their own source/lifecycle evidence
and correction or explicit accounting before the overall destruction item is
complete. This item-source increment does not check off that broader requirement.
