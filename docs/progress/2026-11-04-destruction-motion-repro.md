# 2026-11-04 Rocket collapse motion repro

Maxwell supplied two v0.4.3 repros: two stock rockets from fixed feet/camera
poses after loading Jetpuff's Towers on Bedroom and Pirate World on Kitchen.
Private originals and screenshots remain outside Git under
E:/Downloads/BlocklandReImagined-Release-Evidence/destruction-2026-11-04.

The new explicit external-save test uses the ordinary old-save converter,
map and package loader, stock rocket image transitions, authoritative section
cues, public-world changes and client Building/BrickDebris paths. No window or
interactive gameplay input is used. It launches exactly two projectiles, with
no dropped cues. Both computed camera eyes match the supplied diagnostics to
floating-point precision. Runtime tick numbers are fresh repro ticks, not a
replay of the original session history. The initial trace omits GPU upload to
isolate client motion/collision scheduling; GPU validation remains required.

Baseline results (default High quality, 512 resident-body limit):

- Tower: 6,577 source bricks, one 193-member detached section, 19 chunks.
  No flight-clock freeze and no fallback debt observed.
- Pirate: 12,363 source bricks, a one-member section and a 1,334-member section;
  the latter partitions into 147 chunks. Many fallback chunks remain at exactly
  the same pose with nonzero velocity for 140â€“146 client frames (about 2.4 s).
  The measured debris advance peaks at 4.89 ms in this debug repro; this is a
  scoped observation, not a release performance budget pass.
- Both larger sections select the aspect-ratio `Topple` path, whose pose
  explicitly rotates around the supplied base pivot and delays free fall.
  The hinge is invented presentation policy, not an actual surviving support.

The fallback separately re-queries world geometry per chunk and retains a
snapshot of moving rubble across paused queries. Its bounded debt is capped
at 0.1 s, so seconds of waiting cannot catch up. Resident collision preparation
already has shared tile caching and a prepared spatial query world. The correction reuses that mechanism for fallback too, keeps current-frame
moving-contact evidence, and spends the existing collision work budget on
completed coherent motion steps. Body and collision-work caps are unchanged.
The duplicate per-chunk Building sweep implementation is retired. The static
query world refresh uses the existing island-safe physics adapter. Nearby
geometry invalidation and collision-window coverage remain in force.

Unsupported assemblies now translate under gravity, without aspect-ratio
classification, a random lean or an invented hinge. Base witnesses are retained;
real first contact begins bounded local breakup. Velocity transfers continuously
from gravity to chunks, using the common cosmetic speed bound. Overflow still
holds angular orientation: full angular constraints require resident admission.
This remains a cosmetic mechanism, without structural stress/crushing or a wire
change. Original support provenance remains available for release audio.

A first moving-obstacle prototype incorrectly predicted gravity for supported
neighbors and failed the real stacked-rubble contact regression. It was
corrected to use observed velocity; the assertion was retained. The large
airborne-start fixture was raised above the test floor: a supported assembly
cannot honestly be required to continue falling. The former hinge-specific
sail case now checks distributed contact on a broad freely falling assembly,
with a corner-only negative control.

The combined CPU replay passes both supplied saves. Pirate World still has
147 chunks and at most 64 resident bodies; maximum fallback debt is two client
frames (33.3 ms), versus the old saturated 100-ms window and seconds of frozen
poses. Tower debt is zero. Debug advance p99/max was 6.42/8.38 ms for Pirate and
1.73/4.01 ms for Tower. These are diagnostic timings, not optimized performance
acceptance. The extra coherent collision work costs more than the broken path.

The GPU replay also passes both sequences using ordinary bounded debris-model
uploads/readiness. Fourteen bounded captures show native brick geometry from
the supplied camera views; captures omit the map renderer, lighting/effects
and weapon view. Tower and Pirate captures were inspected. GPU replay again
records zero/two-frame peak fallback debt, exactly two stock projectiles each,
and no dropped cues. A pre-impact capture initially assumed debris already
existed; that capture now serves as the ordinary intact reference. The final
probe separately requires real debris uploads.

Checks: 62 debris unit cases pass, including real openings, actor crossings,
stacking, zero/one body slot, lifetime, source retirement and large-scale
preparation. `cargo clippy --locked -p bri-client --all-targets -- -D warnings`
passes. Full client unit tests, format/docs, final-head CI/history and private
v0.4.4-preview.1 packaging remain pending. Maxwell’s interactive acceptance
of both rocket repros remains required.

Input SHA256:

- Jetpuff's Towers.bls:
  `70433443239efffab027427a38bda60135852374dd59cf1d93700d2a37260c9b`
- Pirate World.bls:
  `f74be6db2e28cb2fbd1fcb938ab9833596e51b7116b3107efcda53df452ff6ac`

`cargo test --locked -p bri-client --test destruction_repro
two_stock_rockets_replay_authority_and_client_section_motion -- --ignored
--exact --nocapture` passes its setup assertions and records baseline evidence.
Set BRI_CONTENT, BRI_DESTRUCTION_SAVES and BRI_DESTRUCTION_OUT explicitly. This
diagnostic pass is not an assertion that the reported motion bug is fixed.
