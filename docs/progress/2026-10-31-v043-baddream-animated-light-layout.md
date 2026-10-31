# BadDream: animated interior light layout

The local read-only `torque-reference/Engine/source/interior/interior.h` and
`interiorIO.cpp` identify the arrays that the DIF importer currently preserves
as raw sections: an animated light stores a name-buffer index, first state,
state count, flags and duration in milliseconds; each state stores RGB,
activation time and a range of state-data records; each state-data record names
a surface, a state-buffer map offset (or the no-map sentinel), and an instance
light-state index. The serialized widths match this converter's 16-, 13- and
10-byte strides. Struct padding in memory is not serialized.

This source evidence distinguishes the interior's per-surface animated
lightmap data from `content::light_animation`, which interprets map2dif light
node properties. It does not establish how this specific TV is triggered, how
the state-buffer maps are decoded or how nearby characters receive light. The
existing BadDream observation of one light, four states and 116 state-data
entries remains a count, not proof of any visual outcome. A related engine
reference is not itself evidence of Blockland's exact runtime behavior.

No renderer or importer behavior was changed. Next, use the existing preserved
section offsets to decode this fixture's names, timing and surface references,
validate bounds and source version, then establish state-buffer and actor-light
semantics before selecting a native representation. Keep the case incomplete;
do not add a guessed lamp or texture-specific override.

## Fixture state and surface associations

Extracted `bd2.dif` from the supplied nested archive into ignored worktree
artifacts (source SHA-256
`394db625c228939a5761ee3f543deabd1a6378b2a6ba41bdd94dc770fdd80912`). A
temporary offline example called the existing `interior::read` to obtain its
preserved section offsets and native source-index/material mapping; Python
decoded those sections using the reference's little-endian serialized fields.
The temporary example was removed; the original archive was not changed.

- One unnamed light references four states, flag value 3 and duration 250 ms.
  The related Torque reference identifies bits 1 and 2 as ambient and loop;
  applying that meaning to this Blockland fixture remains a source-version
  inference, not a captured original-game observation.
- State `(RGB, active time ms, first data, count)` values are white at 0,
  white at 125, black at 125 and black at 250; each has 29 records. Equal
  activation times require the original interpolation/boundary semantics to be
  verified rather than assumed.
- All four state ranges reference the same 29 source surface indices 88–116:
  28 `white` surfaces and one `gsfstatic` surface, source index 116. All have
  surface flags 16. Thus the data does associate the animation with a TV-texture
  surface and surrounding surfaces; it does not alone establish their final
  appearance or illuminate an actor.
- The state-data maps are present, with four 828-byte regions in the 3,312-byte
  buffer. Those four regions are byte-identical (SHA-256
  `e8ca9844f48d95a78c2331bbff59a08e11a211d2057cd2aa322e79e761347c7d`).
  Their encoded meaning, interpolation and any actor lighting remain unresolved.

This is an offline source-data inspection, not an implementation or visual
verification. The 250 ms cycle is an input record, not a promise that a guessed
on/off lamp reproduces the source.
