# 2026-10-27 Mast video landing correction

Maxwell's preview.2 recording, F:/NVIDIA/Blocklandreimagined-v0.3.7-destruction-preview.2-windows/Blocklandreimagined-v0.3.7-destruction-preview.2-windows 2026.10.27 - 23.19.29.01.mp4,
is 18.96s, 3440x1440 at ~30fps. Read it locally with OpenCV and inspected the
extracted frames (ignored artifacts/collapse-assemblies/video-review). At 8-10s
parts clip into the hull and landing assemblies kick apart/settle awkwardly.
At the end a rocket passes through settled visible rubble without reaction.
This invalidates preview.2's gameplay acceptance; passing member-count tests
did not establish a good landing. No interactive game was automated.

Code-backed causes: a box around a sparse group invents solid space around
sails/crossbeams, permitting false deep penetration on dynamic-body creation;
eight extreme corners can miss the middle of a broad sail over a narrow deck;
the old upward-floor-only hit filter ignores hull-side impacts during toppling;
and deleting sleeping bodies also deletes local shot targets. Maxwell explicitly
selected client-only rubble with a clear visual reaction, retaining authoritative
rockets continuing through it. No server rubble physics or fake gameplay blast.

Correction: merge actual original-member boxes only across matching full faces,
then use their compound union. Bound it to 64 primitive boxes per body and 2048
primitive admission slots, retaining geometry if physics cannot be admitted.
Now groups can collide with each other without their old empty box proxies.
Cache distributed real material samples, including large face interiors, across
64 spatial cells in addition to the original eight extremes. Prefer each cell's
lowest sample. Solid impacts include hull sides; shared candidate budget 4096.
Sleeping bodies retain targets and wake on shots. A minimum 1.5-unit/s chunk hit
jolt keeps grouping from making a projectile's visible response negligible;
ordinary small debris, blast radius/force and authoritative projectiles retain
existing behavior. Budget-evicted geometry still has bounded cosmetic limits.

Targeted tests: six section tests passed. New cases check an open collision
union keeps its gap, a sleeping chunk survives advancement then wakes/moves on
a shot, and a broad sail over a raised narrow deck. The latter's old eight-corner
negative control misses the deck; the new probes limit penetration to 0.25 units.
The first improved-contact fixture still penetrated 1.32 units, exposing both
cell selection and the floor-only filter; corrected rather than loosening the
threshold. Existing 1500-member/eight-body, late delivery and toppling cases pass.

The scoped quick tier passed all 710 client unit tests (78 ignored); focused lint/units and an incremental playtest-profile build follow.
Use a new preview.3 folder; leave the running preview.2 and accepted separate
jet-audio installation untouched. PR #379 remains draft; full gate and human
acceptance remain required before merge. Do not rerun the broad gate while
Maxwell is testing. Original installations remain read-only.
