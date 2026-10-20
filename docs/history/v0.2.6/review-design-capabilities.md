> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-design-capabilities.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: design note, capabilities lane (loop 3, timed throws, harm reader, acceptance half)

Read against the plan's contract and main ac2bdc2.

## Verdict: approved, with the three decisions below and four conditions

## The three decisions

**1. Item_Sports: move it to data, and delete `stock.rs`.**
- The contract says every content-name read leaves the runtime. Keeping one "documented port" would be the special case the contract exists to stop.
- The cost is small:
  - `stock.rs` is 260 lines, with about 40 sports references outside it;
  - the name table moves to the importer, which is already outside the runtime graph.
- The fields aren't vanilla-only in practice. Ball and sports Add-Ons are common, and `Sport`, `SportHit` and `turns_into` are what they'd declare.
- Mark them "not stable" as proposed.
- Condition: the importer test compares every vanilla image's sport values against the old table, and the PC replay of a sports match plays back with the same checksums.

**2. Children: accept them, count them as zero worth, but not zero danger.**
- Counting zero worth is an honest underestimate, and it's what makes Add-On grenades usable. Flak and cluster weapons becoming usable, valued on their main burst only, is fine.
- But `safe_blast` must not treat children as harmless:
  - Shrapnel is exactly what kills a thrower.
  - Max's rule is that bots blowing themselves up with grenades is a bug.
- So, for the bot's own safety and its allies':
  - widen the danger radius by how far the children can carry. That's their own data: speed × lifetime, plus their splash radius. It needs no new constant;
  - if a child has no lifetime, refuse the projectile as before.
  - That's conservative. A bot may hold a grenade it could have thrown, but it never throws one into its own shrapnel.
- Auras stay refused, as proposed.

**3. OnFire: wrap `HostTool`, don't replace it.**
- `OnFire::Tool(HostTool)` keeps the host's dispatch type that `session/tools.rs` and PR #29's callers already use.
- Condition: `on_fire` is the only stored field. The `PR #29` review found `StockFire::HostTool` and `Stock::host_tool` holding the same fact twice. Loop 3 should leave one field, with `host_tool()` reading it.

## The two checks I was asked for

**The harm reader: acceptable, with two limits.**
- The fall part is right.
  - Extract `fall_damage(owner, impact)` from `session/combat.rs:2440-2489`, the same code real impacts use.
  - That covers the minimum speed with height scale, falling-damage permission, the admin-tool exemption, `SPEED_DAMAGE_SCALE` and guards.
  - So there's one rule, not a copy.
  - The extraction must change no behaviour. Tell the coordinator first, since `combat.rs` is shared.
- Kill bricks: only if it's a query the events code answers, not a bot-side scan.
  - Add one function to the events module that answers: "what touching this brick does to the toucher right away".
  - It counts only a touch input with no condition, no delay, the toucher as target, and an output the events runtime itself resolves as kill or damage. It reads that from the runtime's own output table, never a name list in bots.
  - Anything else counts 0, including Add-On outputs, delays, toggled-off rows and variable conditions.
  - It must not run or simulate rows. If that function can't be written without a second interpreter, drop kill bricks from v0.2.6 and ship fall damage only.
- "Out of the world": I found no lower-bound rule for players.
  - `KILL_Y` at `packages.rs:61` is for package entities only. Grep on main found no player rule (inferred, so confirm it).
  - If players have none, a bot must not invent one, which would be simulation built for bots.
  - A landing that never lands then has no harm from game rules. The fighting lane can weigh "out of play" in consequence worth, but that's its call, written down in its note.

**The fuse budget: no new constant. That's right.**
- The chord length comes from the projectile's own gravity and radius: the longest run of ticks whose arc bulges less than its radius.
- Two things to state in the code:
  - **Chord rays can graze past.** A ray along a chord can miss a corner the real arc would graze, but by less than the radius. Positions stay exact per tick.
  - **Projectiles without gravity.** With no gravity the arc never bulges, so a chord could run the whole fuse. That's correct: the flight is straight, so one ray is exact.
- Rays still come from the shared budget, and `bot_think_time_16` gates it.

## Smaller points

- **The Timed rule's second trigger.** Ballistic, `arm_ticks > 0` and not `explode_player` also matches any bouncing projectile with a delay before it arms. The promised vanilla list must show exactly which stock projectiles it picks up. If any changes a stock weapon's use, take it out of this PR or explain it.
- **The hammer's bare 5.0 and 10.0** (`combat.rs:170-181`): good catch. Read them from data, or name them with their v20 source.
- **Client `Equipment`:** one `Tool(HostTool)` variant from `on_fire` closes the item I left for loop 3 in the PR #29 review. Tell the coordinator before the edit, as planned.
- **Acceptance half:** agreed. The spec for the whole run is mine, and I'll write it once rows 5 to 7 have their notes settled. The grenade there should have children, so the safety rule above is tested: an absolute "no bot is ever killed by its own throw's children".
