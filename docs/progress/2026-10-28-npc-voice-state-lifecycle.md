# 2026-10-28 NPC voice state lifecycle

The v0.4.1 coordinator's extended release gate found repeated
`npc_popculture_rules: on_tick -> voice: Function not found: len (())`
errors in all_features_soak. This lane changes only the NPC reaction policy
and its focused regression fixture; the coordinator owns integration,
regeneration, release validation and publication.

`get_player(id, "voice")` returns unit when no value exists. New live bots
can be observed without the default map or a spawn hook having populated it;
`voice(p)` unconditionally called `.len()`. The same line exists in the
v0.4.0 tag, establishing a pre-existing policy defect, not that every old
session encountered it. The old focused fixture manually populated all 40
bots' maps, hiding this ordinary initial state.

The fixture now starts without per-bot state, including one existing player
namespace with no voice key. A regression performs the first tick, actual
health loss, death and respawn; it requires initialized observations for every
bot, an ordinary heavy-damage reaction, reset cooldown/combat/life history,
and no false healing cue for respawn. The call helper still fails on every
script error. No all_features_soak assertion or warning filter is weakened.

The policy now checks for absent state before inspecting its map length, then
uses its existing initialization path. No runtime mechanism, schema, protocol,
recording, reaction probability or cooldown changes are needed. The script is
a generated-NPC recipe input: regenerate the NPC companion and bundle receipt
before packaging; do not manually copy an old receipt or publish the current
build-only run.

Verification results are recorded below after the focused run. Original
all-features failure evidence: `.bri-gate/logs/0eff36e30c99.log`, beginning at
line 593. Local logs: `artifacts/npc-lifecycle-before.log` and
`artifacts/npc-lifecycle-after.log`. No full gate or release workflow is run
from this lane.

- Before fix: `cargo test --profile playtest --locked -p bri-package-runtime
  --test npc_reactions npc_reactions_initialize_new_bots_and_reset_observations_on_respawn`
  failed with the exact `len (())` diagnostic (0.02s test, 1m54s cold compile).
- After fix: `cargo test --profile playtest --locked -p bri-package-runtime
  --test npc_reactions`: all 8 pass, 0.33s test, 4.55s recompile.
- `cargo fmt -p bri-package-runtime`, `python -m unittest discover -s tools
  -p test_generated_addons.py` (2 pass), `python tools/check_docs.py` and
  `git diff --check` pass.
- Focused clippy: `cargo clippy --profile playtest --locked -p bri-package-runtime
  --test npc_reactions -- -D warnings` is running at PR creation; its actual
  outcome will be handed to the coordinator before integration approval.
