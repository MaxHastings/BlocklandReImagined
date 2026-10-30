# 2026-10-30 v0.4.2 release validation blockers

The first full release run at `60fcbe7f4597da5d88c26d93843b0605b06cc742`
built all targets and passed packaged content startup, then ran 528 test
binaries: 5538 tests passed and seven failed. Clippy independently stopped
with an sccache socket error (`10048`), rather than a Rust lint. The gate
returned failure before the explicit 23-save corpus and performance rounds.
This is not a full release pass. Publication remains blocked.

Unchanged, serial reruns against the same generated release content reproduce
all three bot failures: unfamiliar-package grenade friendly damage, allies
leaving a deck, and an inaccessible objective failing to yield to a reachable
alternative. The bot owner is investigating these finite release blockers;
assertions and the accepted feature scope remain intact.

The synthetic Slopes mirror case passes alone after failing under full-run
load; retain it as a load-sensitive failure, not an unqualified pass. The
native Bedroom/Kitchen collision case passes when its documented
`BRI_AMBIENCE_CONTEXT_CONTENT` input is supplied. That test now also accepts
the full gate's standard `BRI_CONTENT` input as the same actual content root;
an explicit ambience-only override still takes precedence.

Two older tool checks did not account for the approved package-owned swishes.
The hammer/contact test now expects exactly one native Fire-state swing before
the unchanged explosion, hit sound and brick death, with the synthetic fixture
still requiring only its original cues. The client fidelity check now loads
package sounds through `ClientAudio`, the production decoder/resolver, and
checks every weighted alternative as well as each referenced cue. It no longer
mistakes absence from the legacy audio bank for absence from the game. No
playback, weapon policy, Gun damage or user-selected media changes are made.

Preserved local evidence is under
`artifacts/v042-release/full-gate-60fcbe7f4597/attempt-1-cache-socket-failure/`
and `artifacts/v042-release/isolated-failures-60fc/`. The first receipt records
all seven named failures. No retry erases this evidence. The build-only
platform run `41008144532` was deliberately cancelled after these blockers
were established; its earlier Linux/macOS content-download ZIP failures are
separate preserved transport failures. A corrected exact-source candidate
still needs scoped checks, platform CI, the deliberate full release gate,
both performance budgets, fixed-save coverage and actual package verification.

Maxwell's final combined-playtest and mix approval remains valid and recorded.
Full alpha remains incomplete.

## Focused correction evidence

At `487ff84560ef825c22873b557465b997c15b1bdd`, the existing warm broker
passed both native/synthetic hammer-contact cases, the actual stock-cue audio
resolution case, and the native-room collision case using only `BRI_CONTENT`.
Commands were `cargo test -p bri-sim --test tools` filtered to
`destructo_wand_breaks_a_brick_like_the_hammer_with_its_own_hit_sound`,
`cargo test -p bri-client --test v20_fidelity` filtered to
`every_stock_cue_resolves_to_a_sound_or_effect`, and
`cargo test -p bri-client --lib` filtered to the named native-room case;
each used `--locked`, the shared warm target, `--include-ignored` and one
test thread. The isolated full
`cargo clippy --workspace --all-targets --locked -- -D warnings` step also
passed after safely restarting the cache service with idle timeout disabled.
Neither disk cache nor build targets were cleared.

At combined `4e19a2f7e92c22bec6826ed2d150fc0b6e28de75`, the bot owner's
objective correction passes the original 43-point reachable-alternative case.
It replans on the ordinary cadence while the failed approach is still excluded,
instead of allowing the exclusion to expire before choosing another action.
The other two original cases still fail and their opt-in traces are retained;
no successful release gate is claimed. The official content planner reports all
19 base recipes current, and the actual imported-generation recipe equals the
existing bundle receipt, so these changes do not justify restamping content.
