# Destruction preview

This separate build enables the full destruction experience. Use the existing
Physics Quality setting; there is no extra destruction/shard switch. The server
and clients must use this same preview version.

First retry the **pirate ship mast**: shoot its base support and watch the
mast, sails and flag stay recognizable together as they tip. On landing they
should separate into connected assemblies, keeping their original little
bricks. Repeat at a lower Physics Quality: breakup should be coarser, without
losing half the sail. Also try two supported structures losing their supports
in one blast, and restoration after a large collapse.

Then use **Afghanistan DM**, equip the Rocket Launcher, and use a minigame
with brick damage enabled (and all players' bricks enabled for the bundled map).
Try the center of an upper floor, a floor edge, and the ceiling corner by a wall.
Ordinary small bricks should still pop off whole. Large plain floors/blocks
should lose stud-aligned chunks. Remaining support should hold; removing the
final connection should show a section falling and breaking on landing.

Then try a small wall beside an ordinary large block, a ground baseplate, a
roof baseplate, and a deliberately floating build. Foundations/floating roots
stay anchored; ordinary ground blocks can split. Try a second shot in a hole,
wait for restoration, and save/reload after a blast. Transient damage saves the
original build. Deliberately editing/cutting a surviving part commits the
remaining parts as authored geometry and cancels that original's restoration.

Watch for wrong-sized/colored debris, duplicate material, floating fragments,
restored bricks remaining invisible, ugly long delays and noticeable hitches.
Also judge repeated blasts: the uncommon cracked plastic should add variety
without becoming noisy or exhausting. Radius and force have not been enlarged.

Falling sections are local visuals: they cannot crush, carry players, block shots
or leave permanent rubble. Tall sections tip about a base near the hit;
wide slabs drop and tilt. Landing contact and assembly box colliders are
cosmetic approximations. Physics Quality controls assembly breakup and bodies,
not member visibility. Sleeping or budget-limited assemblies keep their art
at the last pose through the normal fade. There is a separate 65536-member
render ceiling for extreme simultaneous collapses; new groups beyond it can
be omitted whole. Missing/interrupted cue batches remain one visible assembly
instead of breaking apart prematurely. Unsupported printed/special
shapes retain their previous weapon size rules. At a split resource/catalog
limit, an oversized brick stays intact instead of vanishing wholesale.

This is a playtest candidate. Automated checks establish technical behavior;
Maxwell decides whether the experience is enjoyable and ready to merge.
