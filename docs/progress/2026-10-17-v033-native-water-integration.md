# Native-water consumer correction and replacement validation

The first integrated v0.3.3 candidate, `0b27fe669d22ea3b2f3b58a72d0a690c95f2a470`,
passed all six required CI jobs in run39050221305 but failed its full local gate
(actual exit1, 2746 seconds). The native catalog consumer still counted every
authored water as static and rendered only the static scene plus terrain. Its
standalone retry failed too; this was not waived or entered as a known failure.

Root reviewed and merged test-only `e96ec3f2f571f9a22cea90f4dba203e42a6ddfdb`
from parent `bfcba5dc993688f15d64d3203aaf9e4ffff405ee`. Its only paths are
`crates/render/tests/persistent_scene.rs` and the existing water progress note.
Renderer/client production source is unchanged. The consumer checks the complete
finite/static and repeated source inventory, actually draws repeated groups,
checks multiview ownership and stationary upload reuse, and preserves prior
terrain/material/fog/camera assertions. Five real maps have additional paired
pixel controls beyond the old fixed water patch. Existing ignore annotations,
resource bounds and camera distances are retained.

Root independently verified all 22 durable evidence file hashes, the remote
branch pin, final tier logs and native test result, and inspected the restored
Slopes render. Worker final quick: 70 passed/0 failed, 5 seconds. Full normal
focused render/dependents: 1259 passed/0 failed/123 existing ignored, 13 steps,
638 seconds, including warnings-denied all-target Clippy. The separately enabled
native case passes all 14 maps in13.03 seconds. These scoped results do not replace
the integrated gate or CI for the replacement candidate.

Replacement validation uses one full gate with Cargo2jobs, RUST_TEST_THREADS8,
CARGO_TARGET_DIR unset, eight test binaries and a20-minute binary timeout. Its
actual result and matching CI must pass before authorized old-cache cleanup.
Until then all diagnostics and build output are preserved. No main push,
publication, interactive acceptance, private preview change or cleanup is part
of this integration step. The exact new source pin and live validation IDs are
recorded outside frozen source in artifacts/v033-aa-coordination-checkpoint.json.
