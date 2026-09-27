# Pickup semantics: recovered v20 scripts and qualified engine evidence

Audited 2026-09-26, read-only. **Do not substitute a 0.625 center-distance sphere
or add wall ray occlusion as if either were established v20 behavior.** The exact
v20 scripts establish collision-driven pickup and permissions; pinned engine-family
code corroborates world-box contact and a 480 ms thrower exclusion. The closed v20
engine implementation and final faded-item appearance remain unverified.

## Pickup geometry

Recovered `.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs`:

- Lines 8783–8785 author standing/crouch bounding boxes via scale-four values and
  `pickupRadius = 0.625`. The field alone does not specify a spherical distance
  test, its origin, native-unit conversion or an occlusion test.
- Lines 9010–9033, `Armor::OnCollision`, reject dead players and destroyed objects.
  For an Item, the player must not already have that item's datablock in any tool
  slot. The callback then calls `pickup`; lines 7101–7105 dispatch to its datablock.
- Lines 7342–7432 require `canPickup`, a client, permission and a free tool slot.
  A minigame with weapon damage enabled blocks pickups for five seconds after F8.
  Successful static pickup respawns the existing object; dynamic pickup deletes
  it. No free slot leaves the item unchanged. `Weapon::onPickup` at 7712 immediately
  delegates to ItemData and returns; the older code below that return is inactive.

Pinned [OpenMBG player code](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/game/player.cc#L324)
is **family evidence only**. Local `mbg-player.cc:324–331` clamps pickupRadius
against the larger XY bounding-box size and twice that size, then truncates the
difference to integer pickupDelta. Lines 2699–2705 expand a candidate box; the
actual Item branch at 2737–2743 checks overlap between the player and item's world
AABBs and excludes the item's collision-timeout object. There is no line-of-sight
ray in that branch. A wall can constrain movement but does not independently veto
an overlapping item in this family path. Do not transplant its numeric box/radius
conversion into Blockland without accounting for v20's customized player scaling.

Local `mbg-shapeBase.cc:754` takes the shape's authored bounds as the object box.
Item contact therefore depends on model bounds/pivot, not just item origin.
The [family item implementation](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/game/item.cc#L785)
builds box collision from object bounds and scale with an axis-aligned transform;
player pickup separately uses world boxes. These two tests should not be assumed
identical for rotated items. Authored DTS bounds should remain distinct from visual
triangle bounds or a newly fitted physics hull. Exact v20 broadphase and rotated
world-box rules are still unknown.

## Thrower exclusion

`ServerCmdDropTool`, core 5319–5348, assigns the drop's minigame and account ID and
calls `setCollisionTimeout(player)`. It supplies no duration. The generic
`ShapeBase::throwObject` does the same at 7201.

Pinned family `mbg-item.cc:26,317–336` uses 15 simulation ticks and stores one
excluded ShapeBase. `mbg-gameBase.h:325–327` defines a 32 ms tick, giving **480 ms**;
a 120 Hz native deadline rounded up is **58 ticks**. The exclusion suppresses that
thrower's item contact/pickup and physical collision, not every player's access.
The item clears it on countdown or excluded-object deletion. This duration is a
source-backed family compatibility choice, **not proof of the exact v20 value**.
[Item source](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/game/item.cc#L317),
[tick definition](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/game/gameBase.h#L325).

## Static respawn visibility

Exact script, core 7237–7288: fadeOut requests a zero-duration fade-out, writes node
alpha 0.25 using the datablock color if applicable, and disables pickup. Respawn
schedules fadeIn after the brick's configured milliseconds or the global 4000 ms
default. fadeIn cancels an existing pending schedule, requests a zero-duration
fade-in, restores image colorShiftColor or white alpha 1 and enables pickup.
The object persists throughout; the script does not delete/recreate or setHidden.

The [family ShapeBase fade path](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/game/shapeBase.cc#L3467)
sets fade factor to zero after an outward fade completes; render passes that factor
to the mesh override (`mbg-shapeBase.cc:978–990,2311,3467–3479`). Thus the node-alpha
write alone **does not prove that the cooling-down item remains visibly 25% opaque**.
Blockland's custom per-node color/fade interaction is not in this family source.
Retain separate existence, pickup availability, fade and node-color concepts; mark
the final ghost-versus-hidden presentation as awaiting stronger v20 evidence or
Maxwell's feel/visual test. A cooldown timer itself is established.

## Ordinary pickup permissions

Core `miniGameCanUse` at 22733–22813 executes this order:

| Case, in precedence order | Result |
| --- | --- |
| LAN | Allow immediately |
| Different minigames but same owning account | Treat item as player's minigame; controlled-player exception does not apply to items |
| Both outside minigames | Return -1 (no minigame decision) |
| Remaining different minigames | Deny with MiniGameDifferent |
| Same minigame, Item without spawnBrick | Allow before brick ownership settings |
| Same minigame, useAllPlayersBricks and PlayersUseOwnBricks | Allow only same owning account |
| Same minigame, useAllPlayersBricks without own-only | Allow |
| Same minigame, useAllPlayersBricks false, ordinary brick Item | Allow only minigame owner's item; otherwise NotInMiniGame |

`ItemData::onPickup` starts allowed and overrides only explicit 0/1, so outside
`-1` means **allow in this caller**. It does not call getTrustLevel. The trust-themed
denial string is not an actual outside-minigame trust check. Do not copy vehicle
mounting's separate trust fallback into item pickup. Other item conditions above
still apply, including LAN's duplicate/free-slot/death checks.

Core 22961 onward resolves an explicit valid object miniGame first. Brick items
otherwise follow the brick group's currently connected owner's minigame; thrown
items use their stored miniGame. Core 21307–21399 resolves ownership through the
spawn brick group or `BrickGroup_<drop.bl_id>`. Drop creation stamps miniGame/bl_id;
thus a thrower's later minigame change does not simply rewrite every drop, while
the same-account exception can still allow their pickup. Missing owner groups and
deleted minigames can produce invalid legacy values; native code should preserve
explicit absent identities rather than equating unrelated missing accounts.

## Evidence identity and limits

Recovered vanilla core SHA-256:
`a720f7a188a615276befa34409eda68585ec9abbc629b7a8107f37dcf7b33a9b`.
Family commit: `MBU-Team/OpenMBG@9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7`.
Ignored local evidence under `.research/openmbu-reference/`:

| File | SHA-256 |
| --- | --- |
| mbg-player.cc | d5ae8d944b706e586010d8cfd3d5e80f1c182f56291b6bf70323319f7936e334 |
| mbg-item.cc | 3cd4179b107d81197d011dd7b4089eb38c75c1714b1b3f1665cd1d5c97c5dd6f |
| mbg-shapeBase.cc | 2ed39a98294f4f43fab73193c334c45435f4343e9e35807cfd1f1c8832db4aa0 |
| mbg-gameBase.h | 6e30f0c4f0ae2f320c3bb24a0992caf7247aea5eec562a7ff9db732b33bf5180 |

No v20 engine binary was reverse-engineered for this audit. No runtime code was
changed, original installation written, window launched, input sent or audio played.
