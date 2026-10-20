> **Historical note. Written 2026-10-12 (the file's date in the cloud project folder, `v0.3.0/w2-assessment.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# W2 assessment: grips, sockets and animation roles

Checked on `v0.3.0-experimental` at `a29b446` (after the Playground, PR #106).

## Three kinds of body, three different failures

| Body kind | Example | Today |
|---|---|---|
| The Blockhead (v20 rig) | everyone by default | Works: items on `Mount0`, eye on `Eye`, sit clip in seats |
| An Add-On rig (`*.shape.json` with nodes and clips) | ported Shark | Refused at load unless it has all 7 Blockhead clips (`avatar.rs:493`); then any missing `armreadyright`, `sit`, `jump`, `fall` or crouch clip fails the *whole* pose (`avatar.rs:1388-1520`), so holding a gun or sitting makes the body vanish. Items need a node literally named `Mount0`, the camera a node named `Eye` |
| A box model (`model` kind, static boxes) | Runner (1.9 tall), Commando (2.5 tall) | Draws, but an invisible Blockhead is posed underneath (`frame.rs:985-1003`), so the held gun hangs off a Blockhead's hand at Blockhead height; seated, the boxes stand upright through the seat with no tilt (`packages.rs:275-298` uses yaw only) |

The server fires every shot from the eye for every body (`weapons.rs:347-356`, marked temporary), and bots plan with the same assumption (`bots/combat.rs:267`).

## What I will build

1. **Sockets on the archetype** (`sockets`: `right_hand`, `left_hand`, `eye`, `seat`). Each is a node name (rigged bodies) or an offset from the feet (box bodies), or a node plus an offset. Defaults are today's Blockhead names (`Mount0`, `Mount1`, `Eye`), so the Blockhead is unchanged. Image mount points 0 and 1 resolve through the hand sockets; others keep `Mount<n>` as now.
2. **Box bodies stop borrowing a Blockhead.** Held items attach to the body's own hand socket and pitch with the aim; a body with no hand socket hides the item and says so once. The camera uses the eye socket.
3. **Animation roles.** An `animations` map from roles (`idle`, `run`, `back`, `side`, `jump`, `fall`, `crouch`, `crouch_run`, `crouch_back`, `crouch_side`, `aim_right`, `aim_left`, `aim_both`, `sit`, `death`, `swim`, `look`, `head_side`, `head_up`) to the body's clip names, defaulting to the Blockhead's. Each role has a fallback chain (`back` to `run` reversed, `crouch_run` to `crouch`, `fall` to `jump` to `idle`, `aim_both` to `aim_right`, anything to `idle`); a role with nothing to fall back on skips its layer instead of failing the pose. Load accepts any body with at least one clip (or none: it stands in its bind pose), and logs one line per missing role.
4. **Seats for every body.** Box riders take the seat's full tilt. Seated, a box body lowers so its eye lands on the archetype's sitting eye (the same eye the server already gives seated players, so view and drawn head agree), and boxes below its `seat` socket (the hip) fold forward like legs. A body with no seat socket lowers without folding and says so. Climbing out eases back over one transition, as the Blockhead does.
5. **Shots from the muzzle.** The host learns each body's resting hand (for rigged bodies from the rig, as it already learns the Blockhead's mount points at host setup) and each image's barrel (`mountPoint` to `muzzlePoint` from its model). The server fires from that muzzle, converging on what the crosshair points at, and falls back to the eye when the gun is pressed into a wall. Bots use the same origin.
6. **Eye from the body.** An archetype that sets an eye socket but no eye heights gets its standing, crouching and sitting eyes from it.
7. **Diagnostics.** One line per body naming exactly what it lacks (socket or role, and what that costs), shown as an Add-On fault on clients and in the host log.

Both proving bodies already exist and are independent: the Runner (Playground) and the Commando (sample). I add `sockets` to each. A third, synthetic rigged body in tests has its own node names, no `Eye` and only two clips.

## Evidence I will deliver

- Client tests: the Runner and the Commando each hold the Pulse Carbine at their own hand (the Commando Rifle has no third-person model, so it proves the model-less case: it fires from the hand); both seated in the buggy's two seats tilt with it, sit with folded legs, and climb out upright; the synthetic rig with two clips poses, holds, sits and dies without failing, and its diagnostics list the missing roles.
- Sim tests: shots leave the Runner's and Commando's muzzles (not the eye), land on the crosshair, and fall back to the eye against a wall; a Blockhead's shot is unchanged where the decision below says so.
- Offscreen render of the two bodies holding the same gun in the buggy, if this container can open a software GPU; otherwise the render test is written for the gate on your PC.
- Tests that pinned "box bodies hold on a Blockhead hand" or "missing clip fails the pose" are updated.

## Not in W2

Animating box bodies' arms and legs while walking (W8), retargeting skeletons, a new physics or ECS, and switching bodies mid-game (W4).
