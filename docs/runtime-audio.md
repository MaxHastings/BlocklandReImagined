# Native client audio integration

The actual client loads the native audio pack and preserves saved preferences
case-insensitively while seeding missing original defaults. Menu note intents,
ghost move/rotate/change, committed channel volumes and audio preference toggles
are connected. Audio failures are bounded diagnostics, never gameplay failures.
The requested missing title track remains unbound; no substitute is selected.

App::load and all integration tests select Null or Offline output. Only the
explicit executable --run path requests the cpal device; device failure falls
back to silent output with diagnostics. No audible playback was performed.

Session emits ordered presentation cues after successful jump/plant/remove/tool
operations. Protocol 5 carries them on the reliable update stream independently
of coalesced pose snapshots. Checkpoints carry a cursor: late join does not replay
old sounds. Replica validation rejects malformed/out-of-order/unreported gaps,
and bounded queues expose overflow. A saturated client transport fails explicitly
rather than silently dropping reliable state. Cues are transient, not saved world
state; clients cannot submit arbitrary sound-play requests. Runtime identity v5
adds the full manifest and checksum-validated clip bytes; current v6 also includes
weather. Dedicated host hashes these resources without decoding or playback.

The listener is updated before pending sounds begin, preventing distant initial
positions from incorrectly culling a nearby cue. Offline evidence covers this,
original defaults, preference gating, finite samples and teardown. Real QUIC
tests cover two listeners, rejected actions, exactly-once delivery and late join.
The actual App/UI/QUIC/offscreen flow exercises initial cues with silent mixing.

Remaining requirements: music-brick state and streaming attachment; player
jet/pain/water/death/spawn sounds; weapon/projectile/vehicle bindings; plant-error
reply mapping and undo/event-driven sound completeness; audible timing/loudness
acceptance by Maxwell. The original 16-voice runtime policy needs gameplay-load
evaluation. A complete converted trigger table does not establish full dispatch.
