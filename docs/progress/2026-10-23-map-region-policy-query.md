# 2026-10-23 Authored map regions available to package rules

The host previously loaded collision and surfaces but did not retain the map's
existing presentation partition for package rules. Packages therefore could
not select behavior using the same converted region tree. This change retains
the validated optional partition in Simulation and exposes a read-only
`map_region_at(x,y,z)` query with a region number and bounded text properties.
The engine assigns no game meaning to those properties. No wire change,
collision replacement or new spatial volume system is involved.

The demonstrated private consumer selects a rifle recording from original
map-zone metadata in its existing admitted-shot callback. Independent original
function decoding, execution of the original actor/point-region lookup, and
original recording PCM qualify that relationship. The corrected assembled
Pipeline output matches the independent recording through the production
client mixer outdoors, in two indoor locations, and on return outdoors. A
reload refusal remains silent; aim, crouch and weapon-switch recovery are
exercised. The known-bad fixed recording fails both indoor comparisons.
This does not certify original final mixing, full boundary timing, traversal,
enemy-specific hearing, network animation or human acceptance.

The original investigation's positioned audio oracle used an incorrect inverse
coordinate transform. The corrected fixture checks original and converted BSP
agreement at actual body/cue positions before giving an audio verdict. That
test-harness correction is separate from the production fixed-recording defect.
The earlier receipt now explicitly withdraws its invalid outdoor control.

Verification: quick tier and the package-query integration tests pass. Focused
checks caught a new compatibility regression: an explicitly null optional
partition was rejected. The loader now accepts missing/null partitions while
still rejecting malformed non-null values; all four affected existing map
import tests pass. The final focused tier passes in 1,068 seconds: strict clippy,
all non-GPU integration tests of 28 affected/dependent crates, and client/render
unit tests. Full gate and integration are pending.

Raw private evidence stays outside this public repository. The working-tree
receipt `audio-correction-final-variations/run.json` records the tested file
hashes and unchanged inputs; its two tests pass, four audio outputs have zero
sample difference from the source-recording controls. Secondary placement still
reports collision recovery failure, so it is audio evidence only. An ordinary
cellar descent also changes the audible recording from outdoor to indoor using
the source map partition. The actual two-client QUIC consumer passes twelve
rifle output comparisons with identical per-shot cue IDs and zero sample error.
An initial AK receipt-field mismatch was a harness failure and is preserved
separately. These consumers do not establish rendered animation agreement.
