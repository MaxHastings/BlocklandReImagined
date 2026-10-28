# Destruction preview

This separate build enables the full destruction experience. Use the existing
Physics Quality setting; there is no extra destruction/shard switch. The server
and clients must use this same preview version.

Start with **Afghanistan DM**, equip the Rocket Launcher, and use a minigame
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
or leave permanent rubble. Intact sections have a gentle shared fall/tumble and
approximate floor probes; constituent bricks use normal local physics after
breakup. Physics Quality may omit cosmetic members. Unsupported printed/special
shapes retain their previous weapon size rules. At a split resource/catalog
limit, an oversized brick stays intact instead of vanishing wholesale.

This is a playtest candidate. Automated checks establish technical behavior;
Maxwell decides whether the experience is enjoyable and ready to merge.
