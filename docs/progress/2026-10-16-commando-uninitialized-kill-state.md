# 2026-10-16 Commando first-kill state correction

Root follows the warning observed in the integrated mixed-body network soak:
sample-commando:on_death attempted +((),i64) when killer streak was absent.
Its spawn callback initializes streak, but on_death can hear credited actors
without that package's initialized player state. The sample now treats an absent
streak as the first kill; an existing streak increments as before. No engine,
hook dispatch, other Add-On state, scoring authority or declared policy changed.

New package-runtime/commando_kills regression loads and compiles the actual
sample packages and invokes their actual on_death through Runtime with empty
package state and valid player snapshots. Before the fix it reproduces exactly
the script.error at commando.rhai:59, Function not found +((),i64). Afterward
first/second credited kills yield score/streak1/2; suicide and uncredited deaths
produce no state or operations. This is callback evidence, not a claim of a full
mixed-body network soak rerun or original-game fidelity acceptance.

Validation: new regression passes; check.py quick --crates bri-package-runtime
37/0/0 in15s; focused --crates bri-package-runtime --depth0 formatting,
all-target warnings-denied Clippy and all package-runtime ordinary tests pass
in27s. Existing actual Session commando integration target also passes, covering
joining, loadout, dummy kills, minigame player kills and sentry interaction.
Logs: artifacts/commando-uninitialized-killer-{before,after}.log,
commando-state-{quick,focused}.log and commando-session-regression.log.

Root branch codex/v031-integration-followup. This scoped correction awaits next
combined candidate Windows/full gate; main is still exact1be98fd7. No release
publication or new player build is claimed. Portal vehicle collision remains
root-owned separate work; shared layer/pose and original fidelity continue.
