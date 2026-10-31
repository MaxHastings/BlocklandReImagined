# 2026-10-31 Brick music follows the same visual carrier

Adjacent destruction audit found brick music indexed at its static authored point.
Recovered v20 `fxDTSBrick::setSound` (allGameScripts-Vanilla.cs:11500) creates an
AudioEmitter at the brick transform; the script alone does not prove its native
fake-death lifecycle. Follow actual visible debris here, using the same source
restoration/partition rules as lights and emitters, rather than copying that gap.

`brick_debris::attachment_delta` is the shared read-only resolver. The existing
music owner keeps its playback handle/phase while moving, retains removed-source
metadata only in its existing bounded 64-voice set, and stops on source retirement.
One authored original follows its declared surviving partition carrier or actual
moving debris; a newer original death supersedes it. Missing historical debris
stays silent. Restoration wins over old frames; an explicit sound removal stops
the old voice. No sound settings are copied onto generated parts.

The authored index still diffs persistent brick maps. It additionally indexes
carrier references and destruction eligibility, so restoration can activate a
previously silent source even when the restored carrier has no sound itself.
Stable playback updates at most 64 voices; candidate refill follows authored,
cue or eligibility changes. App frame code observes deaths before source sync
and resolves audio after debris advancement. Its old duplicate world revision
cache is removed. No wire/save/content/asset representation changes.

Checks and failures, private `brick-music-*` diagnostics:

- Initial shared-helper edit accidentally changed a test-local frame reference;
  restored that independent expectation. Test setup then needed an explicit
  `u64` key and OrdMap collection instead of array conversion. These compiler
  corrections do not change acceptance assertions.
- The first real-debris test incorrectly expected Physics Quality zero to retire
  a tool-kill animation. Existing ownership keeps that animation visible until
  its fade; test that its same audio handle stays with it until natural expiry.
  Blast culling and tool expiry both then require silence. No runtime lifetime,
  threshold or ignore is changed to pass the fixture.
- Both music lifecycle cases pass: actual blast/tool bodies through temporary
  and permanent removal, changing source positions, expiry/culling and restore;
  plus offline PCM attenuation away from the original point, unchanged playback
  handle, cue/removal delivery order, partition death, late observation, restored
  carrier eligibility and explicit sound removal. The incremental index case
  retains its original 5,000-brick changed-work assertions.
- The existing audio module passes 22 cases (one established ignored converted
  case) and FX passes five (two established ignored content/timing cases).
- A new full-command-queue case reproduces committing a refused position to the
  cache. Source updates now commit only after acceptance, refused stops retain
  their owned handle for retry, and a queue-refused start retries next frame
  without needing another world edit. The unchanged queue-pressure assertions
  now pass for refused moves, stops and starts.

Completed-source commands:

- `cargo test --locked --profile playtest -p bri-client --lib audio::tests`:
  23 pass, one established converted-content case ignored by its default harness.
- `cargo test --locked --profile playtest -p bri-client --lib effects::tests`:
  five pass, two established content/timing cases ignored by default.
- Explicit `effects::tests::replicated_effect_lifecycle_late_join_and_capacity::content
  -- --ignored --exact`: passes using generated native content.
- `cargo clippy --locked --profile playtest -p bri-client --all-targets --
  -D warnings`, formatting and `git diff --check`: pass.

No failing test was accepted by an unchanged retry. No ignores or production
lifetime/physics changes were added for these checks.

These are device-free PCM/native lifecycle checks, not listening acceptance or
final rendered pixels. Broader movement/session/bot/lighting work and final
combined/platform/release gates remain open. Do not mark the release complete.
