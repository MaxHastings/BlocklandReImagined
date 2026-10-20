> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/whole-picture-notes-bots.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Whole-picture working notes: bot module sweep (combined tree d068f06e + ec7db637 + ed0680c0 + 92501ea + e59b354 + 97b5fc1)
Paths are relative to crates/sim/src/session/. Spot-verified by me (✔), the rest are from the sweep.

## Duplicate mechanisms (structural)
- ✔ Threat/armed readers split. contest.rs:80 and team.rs:471 use `hand_combat::item_attacks` (capability-only), while the planner uses `attack_of` (combat.rs:318, which also covers script-fired mechanisms). An enemy holding a scripted weapon reads as unarmed. extras.rs:672 and interactions.rs:1372 are more variants.
- Push valued 4 ways: `tactics::rate` (none, tactics.rs:650), `worth` (simulated push_harm), the spots bound (whole health), `item_worth` (none, UNKNOWN_WORTH fallback).
- ✔ Dodge projectile prediction: extras.rs:300 integrates with `9.81 * d.gravity` and raw damage. It is a third flight model next to tactics::flight_position and runtime::fall_per_tick.
- Ally/blast clearance: worth UnsafeBlast, safe_blast, bot_fire_clear, bot_miss_spares_allies and bots.rs:818 BLAST_CLEARANCE use margins that disagree. Ally-harm build deletes clear_cone/safe_blast/clearances/UnsafeBlast/bot_fire_clear judge, so recheck after it lands; bot_miss_spares_allies + BLAST_CLEARANCE aren't on its list.
- ✔ Second fire decision: bots.rs:2878-2892, with `reach*1.1+0.5` and aim tolerances 0.1/0.12 rad, runs alongside hand_combat::Decision.
- item_worth (pickups) vs tactics::suitability (fight) score the same weapons on two scales.
- Walk speed constants (contest WALK 4.0, surprise WALK_UP_SPEED 3.0) vs tuning().forward elsewhere; contest's "moving" threshold uses two values.
- Give-up/cooldown memories: Cooldowns, interactions GIVE_UP_TICKS/VEHICLE_GIVE_UP, extras rests.
- ✔ Idle look drift: perception.rs:593 has its own two-sine signal next to cadence::drift.

## Outside chooser/hold/boredom
- ✔ extras crouch_until re-extends while hit without re-asking the chooser (extras.rs ~427).
- Rests next_dodge/DODGE_REST, CLICK_REST, HAND_REST; next_wander with bare 240+random*480 (bots.rs:758); strafe leg timer.

## Numbers
- 35 bare 120.0 tick-rate literals in bots (✔ count) instead of the tick constant; blast margin 1.0 duplicated; behaviour base scores vs contest weights coupled only by comments.

## Clean
- No item/datablock/map/mode name gates found. Event-name strings are protocol. Hammer is a documented host-tool descriptor.
