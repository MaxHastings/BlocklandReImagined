# v0.4.2 release validation blockers

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
`BRI_AMBIENCE_CONTEXT_CONTENT` input is supplied. The release wrapper must
provide that same actual content root, in addition to `BRI_CONTENT`.

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
