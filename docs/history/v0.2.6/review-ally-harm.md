> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-ally-harm.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: teammate-harm check (fighting lane)

Branch claude/v0.2.6-fighting-allyharm-ki8k6d = **54095358** (6edd574e, 024c8619, 54095358 on rooftop 1400e814). Design: design-ally-harm.md.

## Verdict: PASS with 3 small must-fixes (tests and leftovers, no design change). Merge the sha that carries them; it then stacks with 8c0c7b34 for the gate.

Ran in the cloud on 54095358:
- `bri-sim --lib harm`: 10/10
- `bri-chaos --test bot_tactics`: 16/16
- `bot_interactions a_gunner_never_fires_a_blast_that_hurts_its_crew_or_an_ally`: pass
- `bot_perception a_relaxed_bot_returns_fire_after_its_longer_reaction` **with goofs left on**: pass at 024c8619 and at 54095358 (see (a))

The gauntlet was not run here; the full suite runs in the PC gate.

The design is followed. `harm::shot_harm` is the one prediction. It walks the same chords as `clear_path`, a timed throw's `timed_aim` path, or the swing's struck body. The first body on the way stops the shot. Pellets take a share of v20's spread box, with nearer bodies shading farther ones. The blast uses `runtime::blast_falloff`, the host's own. Each body counts only where `can_damage_player` lets the shot hurt it, and never for more than its health left. `assess` takes the worse of the aimed shot and the shot its aim error sends off. `worth` = enemy − ally − own, one for one.

## (c) The veto: a shot expected to kill a teammate never fires ✅
- **Planning.** `tactics::worth` returns `Unsuited::Kills` when `harm_allows` fails, meaning `kills_ally`, or own harm reaching the shooter's health. `kills_ally` is set when one ally's capped harm reaches its health. The error-turned shot is OR'd in.
- **Hand weapons at the trigger.** `validate_fire` re-runs `assess` with fresh `Bodies` and the real direction (aim error included) on the release tick. It requires `harm_allows` and `ally + own < planned enemy`. Timed throws use the real throw's burst path.
- **Charged wind-ups.** They are kept only while `harm_allows` holds, and the release still goes through `validate_fire`.
- **Mounted guns.** `choose_mounted` prices the shell from the muzzle with `shot_harm`. Crew don't stop the path, but the blast counts against them. bots.rs ~1376 fires only on a fresh `Ready` (`fire &= matches!(native, Ready)`), and the intent only lives for its own tick. So a mount can't fire on a stale plan, and an `Unsafe` (Kills) tick never fires.
- **Splash.** Inside `danger` (fragments) a body takes the full splash. Past that it takes the host's falloff. Allies count at the nearer of where they are and where they're heading.
- **Trigger shape.** Each tick, any uncounted body stepping into the planned way (widened by the aim wander) or burst holds fire.

Not blocking. Both of these are on the safe side, and both are noted for v0.2.7:
- Pellets veto on the expected share, which matches Max's "expected to kill" wording.
- Scripted weapons the chooser can't plan (`Unsupported`) fall back to a line and blast shape. Any other body in it holds fire, but own harm isn't counted. This class is the same as before.
- Own harm uses the rules' self-damage answer, not the projectile's own self-damage flag. That over-counts, which only refuses more.

## (d) Nothing on the deleted list survives ✅
`clear_cone`, `SPREAD_CLEAR`, `bot_fire_clear`, `bot_miss_spares_allies`, `MISS_REACH`, `MISS_CLEARANCE`, `safe_blast`, `clearances`, `self_clearance`/`ally_clearance`/`blast_margin`, `UnsafeBlast` and `BLAST_CLEARANCE` each have 0 hits in crates/. The only hit is the low-roof test's name ("delivers_a_safe_blast"), which is fine. `clear_path`'s ally loop is gone. Timed throws now walk their path. `standoff` = `splash.max(MIN_STANDOFF)`. The mount `Unsupported` is gone. Two leftovers are in must-fix 3.

## (b) Changed asserts
- **Gauntlet.** `team_kills + at_ally == 0` becomes `team_kills == 0` plus own-side < enemy damage, which is Max's "Trade it" spec. The assert itself is fair, but see **must-fix 1** (it's summed over all sides) and **must-fix 2** (`sane` leaves out own harm). The new over-seeds test is good.
- **`nearby_blast_risk_…` → `a_close_blast_weapon_is_fired_only_as_a_trade_its_holder_wins`.** Fair. The old test asserted the deleted 7-unit clearance and "picks the second slot". Under the trade rule, the rocket is allowed when it wins the trade. The new asserts (hurts its enemy, hurts itself less, lives) are the spec's absolutes.
- **Low-roof test.** It asserts the bot is never hurt by its own blast, in place of "launched more than 7 units away". That is the real outcome instead of a proxy, so it's at least as strict. It still asserts it escaped and delivered.
- **`replacing_the_charged_equipment…`.** Recovery now means the replacement fired, not that it hit. Fair: a hit depends on aim chance, and the test is about cancelling without releasing the old attack (that assert is unchanged).

## (a) `first_wound` with goofs off
This isn't a loosening. The test's claim is the reaction delay; goofs add a separate, variable delay on top, and isolating them is what bot_extras.rs:435 already does. The claim's asserts are unchanged.
But it **passes here with goofs on**, at both 024c8619 and 54095358. So:
- **Ask the lane** on which build and platform it saw the 230-tick hold. If it only shows on Windows, the sim took a different path across platforms. That's a v0.2.7 note, not this branch's.
- **Player-facing question, v0.2.7 bot mood unless it's new here.** If a goof can *start* after an armed enemy comes into sight and hold fire for about 2 s, that's a surprise-gate issue. Goofs should only start with no enemy in sight, not just with "nobody fighting it".

## 🔴 Must-fixes (small)
1. **Gauntlet own-side damage is summed over all sides** (gauntlet/mod.rs ~954-958), while the message says "a side hurts its own less than its enemies". With two sides, one side can hurt its own more than its enemies and still pass. Keep `team_damage`/`enemy_damage` per side and assert per side; this is Max's spec.
2. **`sane()`'s `bad_plans` leaves out own harm** (mod.rs ~641). It should match the rule the fire gate uses (combat.rs:1925, `ally + own < enemy`): `p.kills_ally || (p.ally + p.own > 0.0 && p.ally + p.own >= p.enemy)`.
3. **Leftovers.**
   - fire.rs `bot_hand_fire_gate`: `let actual = direction; … let _ = actual;` is dead code; delete it.
   - combat.rs:1681: the doc comment on `clear_path` still says "Allies get a swept motion envelope"; that loop is gone.
   - Nit: the GAUNTLET eprintln format string has a run of literal spaces before `team_damage=` (mod.rs ~493).
