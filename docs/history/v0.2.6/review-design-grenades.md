> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-design-grenades.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: design note, timed throws / grenades (plan row 5)
`docs/plans/v0.2.6-design-grenades.md` @ 035beb21 on claude/project-thread-uoh3ap. Read against the plan, my capabilities design review (children = zero worth plus a wider danger zone; fuse budget from the projectile's own data) and `weapons/src/runtime.rs` `projectile_step` at 035beb21.

## Verdict: approve with four conditions. None changes the design; they make the walk match the runtime and finish the one-mechanism job.

What's right:
- the delivery is picked from data alone (cook, or ballistic plus arm ticks plus not `explode_player`);
- the fuse is read in the runtime's own order;
- the burst point drives worth, safety and the hit check;
- it reuses the existing aims and arcs, with no new search;
- rays come from the shared budget, with `pending` continuation;
- children widen the danger zone from their own data;
- auras and cooking are out;
- it deletes the two refusals and the runtime's inline copies.

Nothing names an item. The absolutes-only chaos test and the vanilla listing before merge are the right proofs.

## Condition 1: the client's ghost bounce is a third copy. Fold it in too.

- `client/src/ghosts.rs:528-536` reflects a predicted projectile with the same formula: `(reflected − tangent·friction)·elasticity`, which is algebraically the runtime's `((v − vₙ)(1 − f) − vₙ)·e`.
- Once `bounce()` is public in `bri_weapons::runtime`, the ghost should call it for the velocity (the client already calls `bri_weapons::runtime::fall_per_tick` beside it, `app/frame.rs:890`). Otherwise there are still two bounce rules.
- The ghost's own `rest_speed` stop and contact cap are prediction policy and can stay.
- That's one small client edit. If the lane would rather not touch the client, say why in the PR. I'd accept "ghosts are corrected by the host" as the reason only if the formula copy is deleted anyway.

## Condition 2: the walk must handle sticking and coming to rest, as the runtime does

- **Sticking.** The runtime sticks a projectile (`min_stick_speed`, incidence under `bounce_angle / 2`: `runtime.rs:3773-3783`), and it then sits until the fuse or end of life.
  - The note's `bounce()` returns "whether it sticks", but §3's walk doesn't say what happens next. It should be: burst point = stick point, at the fuse or lifetime tick.
- **Rolling.** A low-elasticity grenade on the floor re-hits every tick. That's a ray per tick until the fuse, which can eat the shared budget for one candidate.
  - When the velocity after a bounce can't lift it off the surface (its normal component is under one tick's `fall_per_tick`), treat it as resting there.
  - That threshold comes from the projectile's own data, not a new number. The burst point is then that spot at the fuse tick, within the same plate tolerance.
  - The runtime itself keeps micro-bouncing in place, so this matches its outcome, not its steps. Say so in the doc.

## Condition 3: `bursts()` must carry every burst branch of the hit path, and the agreement test must cover them

Per hit, the runtime (`runtime.rs:3700-3800`, in order):
1. contact responses from the query (portal redirect, delete, explode);
2. sport handling;
3. `age ≥ arm_ticks`, or `explode_player` on an actor, or `!ballistic` → burst;
4. a shield stop deletes it unexploded;
5. stick;
6. bounce;
7. `max_bounces` reached → burst.

Before any hit: fuse (`self.fuses`), then end of life with `explode_death`.

- `bursts(def, age, bounces, hit, fuse)` covers 3, 7, the fuse and end of life. Keep that order, or the arm-tick case and the max-bounce case will disagree on the bounce that reaches the limit.
- 1, 2 and 4 are query- or rules-dependent. The planner can leave them out: portal hits already go through `clear_path`'s `passage`. List them in the doc as not modelled.
- The tick-for-tick agreement test should include:
  - a burst on the `max_bounces`-th bounce;
  - an armed hit;
  - a stick then fuse;
  - a fuse in mid-air.

## Condition 4: name the chord tolerance's source, and cover nested children

- **Plate tolerance: yes, use it.** Projectiles are swept as rays (size 0), so "the projectile's size" adds nothing.
  - The runtime itself flies a polyline of one straight segment per tick. A chord of k ticks sags `fall·k²/8` off that polyline.
  - Bounding that by the thinnest thing the world can hold is the honest reading of my condition.
  - Take the plate height from the brick grid's existing constant (wherever `bri_world` defines it), not a new literal, and keep k ≥ 1.
- **Children's reach.** `speed · lifetime / HZ + explosion radius` per child set is fine and conservative, since it ignores their fall.
  - If a child itself has children, either add their reach recursively or refuse that weapon.
  - The note says "a child set with no lifetime is still refused". Say the same for depth.

## Think-time

- At most 3 aims × 2 arcs = 6 walks per timed weapon per turn.
- Each walk is bounded by the fuse in ticks and chorded by the plate rule. With condition 2 a rolling grenade costs one or two rays, not one per tick.
- All of it comes from the shared budget, and `pending` carries an unfinished walk to the next turn.
- That's bounded by construction. `bot_think_time_16` under its bar is the check, as the note says.
- One caution: `pending` should continue the same candidate. Re-starting all six on a later turn would mean they never finish.

## Small notes (fold into the doc, not blocking)

- **Target at the burst tick.** Carrying its current velocity forward over a long fuse (2-3 s) is a rough guess. That's fine because the shot judge corrects for it, but say that a long fuse against a moving target will often miss, and that the judge learns from it.
- **Ballistic, arm-ticked, *and* `explode_player`** (bursts on a player, bounces off walls) stays on the impact delivery. Its first-impact check is wrong when a wall comes first. That's existing behaviour, so it's out of scope, but list it as a known gap.
- **The vanilla listing before merge** should also say whether any stock weapon moves from "refused" to usable through the children rule. That's a behaviour change Max would notice in stock games.
