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

## Finite correction handoff

At `9105d1ca2bb931bcbbb1741ddd1e2e8eef8bed6f`, the complete unfamiliar-game
acceptance passes with all original free-play runs, seeds, layouts and replays
intact. A new geometry-only diagnostic confirms that all three Variant(1)
free-play runs present zero glass sight opportunities after legitimate bot
behavior corrections. The earlier selected-target aggregate therefore could
not prove a visibility defect.

The test now separately establishes that opportunity using the same authored
world, glass, bot kinds, rules/provider and seeds. Native F7/Fetch admin commands
place an existing opposing pair, then ordinary brains run without injected bot
inputs, goals or sight answers. Every variant/seed must have actual geometric
opportunities and positive ordinary-brain sight through the authored glass;
changing only that glass's raycast flag to opaque must produce zero sightings
with positive opportunities. All six positive and six opaque sessions pass
(240 positive observations for Variant(0), 208 for Variant(1), zero for opaque
controls). The original match's natural sight count remains diagnostic. No
threshold, seed, layout, damage assertion or replay assertion was relaxed.

An intermediate bounded runtime sight observer did not supply the missing
opportunity and is excluded entirely from the candidate and its ancestry.
Its source and failed evidence remain on
`codex/v042-validation-perception-observer`; no new observer API, runtime
bookkeeping or seam ships. The final probe is test-only, based on the already
verified timing corrections. The original owner handoff
`40bffa31615dfa2a083de450eaed853cb5e814c9` is an ancestor of this candidate.

Final warm focused checks pass: the complete acceptance case, all four sight
budget units, all 12 harm units and the strengthened actual-host versus
predictor lifecycle case (18 tests total), plus warning-free all-target Clippy
for `bri-sim` and `bri-chaos`. The earlier 40 deck/objective/carrier/physical
checks and four audio/native-room corrections remain recorded. All four deck
layouts now report zero falls. The final full release gate and native package
checks remain required; scoped evidence is not a full pass.

Source availability was rechecked for all 204 seam rows, 552 named items and
13 manually reviewed source paths. The serialized `height_black` spelling is
verified through `OverlayFit::HeightBlack` and its actual serde snake-case
attribute, rather than misreported as a missing literal Rust identifier.
This source audit does not establish consumer completeness or human alpha
acceptance. Maxwell's approved mix and media are unchanged.
