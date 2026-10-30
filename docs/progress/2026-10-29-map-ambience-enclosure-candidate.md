# 2026-10-29 Ambience enclosure and profile candidate

Root approved the content-profile, existing-handle and live-query dataflow.
Candidate source now adds optional `SoundDef.spatial` and `environment`
(exterior/interior); omitted fields preserve legacy playback. Explicit profile
distances win over ambient `range`; old bank/default distances do not opt in.
Validation requires finite0.01 <= reference <= max <=20000. Mechanical existing
SoundDef constructors supplyNone. Profile variants require matching spatial and
environmental metadata. No Cargo manifest/dependency/lockfile change.

Client preparation resolves explicit distances once; ambient handles keep their
placement and environmental intent. Existing streaming/cursor/fade ownership is
retained. A per-handle mixer command changes gain and a1200Hz low-pass blend,
smoothed on the sample clock. Clear voices retain exact dry samples; coefficients
are cached by sample rate, with no per-sample exponential work. No filter touches
music/UI/voice chat merely because it is audio. Unaffected ambient intent does
not bypass ordinary positional obstruction.

The listener probe samples17 physical rays, at most once per250ms, caching by
listener movement and `Building::query_generation`. It calls existing
`solid_segment`, which owns current map/brick collision shapes and gaps. Roof-only
shelter, surrounding walls and closed space blend continuously. Region-authored
reverb remains authoritative; automatic fallback reverb is deferred. Positional
ambient handles now participate in hearing updates, and blocked paths can be
judged even when a map provides no RegionMap. Long source paths query at most
the existing2000-unit segment limit near the listener. This is a bounded
approximation, not material absorption or sound-path simulation. Tiny openings
between sample directions can be missed; no claim of exact acoustic propagation.

Added proposed checks for decoded/mixed profile attenuation, gain/muffling and
streaming recovery without handle restart; client profile/queue retry/lifecycle;
and actual Building queries over a roof, closed brick walls, a window shape,
open rafters and a removed wall. The removal test is **not** an actual weapon
destruction acceptance test; that remains required with the integrated lane.

Only the isolated filter test has run so far:
`rustc --edition2024 --test crates/audio/src/acoustics.rs -o artifacts/map-ambience/acoustics-tests.exe`,
then that executable:1passed. `rustfmt` on changed sources and `git diff --check`
pass. No Cargo/full client/converter build, mixer integration test, GPU job,
package startup or human acceptance is claimed. Root will serialize warm scoped
compilation rather than creating another14GB target. The actual game process
was observed active, then root verified no game processes at22:33:03Z; no work
here terminated it. Asset/source work continued during that hold.

Serialization review: the new authored fields belong to the JSON weapons pack;
the runtime acoustics command is private mixer state without Serialize/Deserialize.
`AmbientPlay`, notices, binary protocol structures and save structures are
unchanged. Exact content identity naturally changes with new opted-in package
data. Root's history/protocol checks are still required before merge.

This is a compilation/validation candidate, **not ready for main**. Original
map-emitter conversion, high-quality master selection, sparse living details,
generated shipping recipe/policy, all required scoped checks, actual destruction
integration, packaged startup and Maxwell's listening remain open. See the
single current dependency handoff in `docs/audits/map-ambience.md`.
