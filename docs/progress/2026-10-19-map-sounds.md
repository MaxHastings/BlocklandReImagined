# Placed map sounds (2026-10-19)

A map scene can place looping sounds: `Kind::Sound` nodes (`sound`, `volume`, `pitch`, `inner`, `range`), made by the glTF map importer from
Empties named `sound...` with `bri_sound` (docs/modding/maps.md). `NativeMap::sounds` carries them to the client, which starts one looping
voice per source (`ClientAudio::start_map_sounds`) through `SoundAsset::placed`: the add-on's sound (a long one streams, as a `music`
sound does) heard in full within `inner`, silent at `range`, at `pitch`. The mixer keeps far loops virtual until the listener comes near.

Tests: `an_empty_named_sound_with_a_sound_becomes_a_looping_source_in_the_scene` (importer and scene), `a_map_sound_loops_in_place_and_is_heard_only_within_its_range`
(client audio, offline mixer). Played: nothing, headless only.
