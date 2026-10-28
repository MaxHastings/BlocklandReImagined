# Brick destruction audio

A weapons/effects pack may own the exclusive `effects.brick_debris_audio` slot.
The engine reports bounded local section starts, checked landings and real
Rapier impacts. The package chooses ordinary positional one-shot sound profiles,
including existing weighted `SoundDef.variants`. This changes presentation only;
no sound events, dynamic-body poses, damage or rubble cover are replicated.

The bundled example is [brick-destruction-audio](../../packages/brick-destruction-audio/README.md).
Its full policy and clips are in `assets/weapons.json`. Fields are:

| Field | Meaning |
| --- | --- |
| `id` | Stable namespaced identity of this response |
| `snap`, `impact`, `landing` | A `{profile,radius}` response, or null for silence |
| `min_speed`, `full_speed` | Impulse/effective-mass contact threshold and volume reference, in units/second |
| `cooldown` | Minimum time between accepted reports of one source/kind |
| `landing_min_members` | Smaller groups use the single-contact response on landing |
| `pitch` | Inclusive pitch multiplier range; supported envelope0.8 to1.25 |

Radii are world units, not studs. The normal mixer supplies distance falloff,
volume controls, sound carry and room response. A profile must be positional,
nonlooping, nonmusic and without sustain. References are included in Add-On
health. Multiple providers disable this slot and report the conflict; enable
one provider rather than relying on load order. With no policy, the old break
sound rules remain. No special destruction-audio setting is added to the UI.

Reports stop at64/frame across fixed steps. One shared8/sec token budget with a
burst of3 prioritizes landings/snaps and nearby strong contacts, with a2-unit
spatial-cell cooldown and bounded source history. These feed the existing local
16-start queue and global mixer voice budget; overflow does not replay later.
Choose short clips: the bundled taps are30 to220ms, scatter clips850 to1000ms.
Resting gravity, presentation staging, proxy preparation and refresh-only physics
steps do not create sound. Spawned landing proxies briefly mute their own
insertion, preventing a duplicate of the analytic landing. Audio does not alter
visual randomness or physics motion.

This is a native stylization, not a claim of original v20 contact-sound behavior.
Record licensing and processing with assets. Use original recordings or sounds
you can redistribute; the bundled CC0 inputs retain exact source/author/hash
records and clearly identify use of public MP3 previews rather than WAV masters.
