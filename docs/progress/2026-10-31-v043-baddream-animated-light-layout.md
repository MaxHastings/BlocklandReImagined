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
