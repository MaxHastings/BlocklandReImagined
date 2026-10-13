# 2026-10-13 v0.3.0 W7: NPC profiles, intents, abilities, contact and reach

Max's rule: one shared brain for anything that moves with intent; an
Add-On never ships its own brain loop. This makes that brain configurable
enough for a Goomba, a Creeper and a Blockhead to be data on it, and
measures what a cut-down one costs.

## What changed

- **Behaviour profile per kind** (`crates/sim/src/bot_kind.rs`, all in
  `bots.json`): `reaction_seconds` and `memory_seconds` are now a kind's
  own; new `tactics` (`cover`, `retreat`, `flank`, `dodge`), `vehicles`
  (`drive`, `gun`, `ride`), `contact` (harm on touch), `ability` and
  `flee_below`. Each tactic switched off skips its work: spots need
  `cover`, the back-step and far spots need `retreat`, flanks need `flank`,
  shot dodges and stepping out of an ally's fire need `dodge`; allies'
  intents and gap-following need `teamwork` above 0. Defaults keep the
  Blockhead exactly as it was.
- **Intents** (`bot_intent(bot, #{ guard | patrol | follow | flee })`, op
  `bot_intent`, capability `bots`; `crates/sim/src/session/bots/intents.rs`):
  the rules move the bot's anchor and the brain walks, routes and fights
  as usual. A new `flee` behaviour runs from the named player or the
  enemy it knows of, switched on by a flee intent or `flee_below`.
- **Ability attack** (`crates/sim/src/session/bots/ability.rs`): the brain
  walks into range, waits out its reaction, winds the ability up (action,
  image, looping sound), calls it off if the target escapes, then calls
  the Add-On's `on_bot_ability(bot, kind, ability, target)`. The brain
  never decides what the attack is.
- **Body contact** (`on_contact(info)`,
  `crates/sim/src/session/packages/contacts.rs`; added at Max's request on
  2026-10-12): as two bodies begin to touch, or a vehicle runs into a
  player, the rules hear `kind`, `by`, `target`, `vehicle`, `from`
  (above, below, side), `velocity` and `speed`.
- **Cost-derived limit**: `BotKind::cost` is 0.25 for a minimal profile
  (no hand weapons, tactics, seats, teamwork, surprise or perception
  strength) and 1 otherwise. The bot limit adds costs, so the default 16
  runs 64 creatures. Brains now share their kind (`Arc<BotKind>`), so a
  tick no longer copies it.
- **Playground**: the Fizzer (`playground-fizzer`, `-look`, `-rules`), a
  Creeper-style creature. It walks up, hisses while its wick burns and
  pops by firing its own explosive projectile, so brick damage is the
  mini-game's. `playground-stomper-rules` flattens a Stomper landed on
  from above or run over, with its own kill-feed line ("Ann flattened
  Stomper"). The Stomper is now a minimal profile (tactics and seats off).
  Rules live in server-side packages, because shared packages may not
  carry scripts.

## Evidence

- `cargo test -p bri-sim --test npc_profiles`: 5 passed (guard, patrol and
  follow; flee by intent and when hurt; ability hook and contact damage;
  each body's reach with the Wall Kicker, Diver and Trooper: the body that
  cannot jump goes round a 1.1-high wall, out to z = 8.2, while the others
  go straight over; 20 creatures under a limit of 16).
- `cargo test -p bri-sim --test playground`: 12 passed, including
  `a_fizzer_walks_up_hisses_and_pops_breaking_bricks`,
  `a_fizzer_calls_off_its_pop_when_the_runner_runs`,
  `a_runner_landing_on_a_stomper_flattens_it` and
  `the_buggy_runs_a_stomper_over`. The last two check the kill feed says
  "flattened", so the rules killed it, not the engine's own run-over damage.
  With the rules' hook emptied, the stomp test fails.
- `cargo test --release -p bri-chaos --test bot_gauntlet creature_think_time_16 -- --ignored --nocapture`:
  16 minimal creatures think in 21 to 24 percent of 16 full Blockhead
  bots' time over several runs (latest: 227 us against 1060 us a tick).
  It was 27 percent before gating team intents, ally-fire dodges and
  gap-following by profile and sharing the kind. Callgrind showed what is
  left is sight, route search and walking, which every mover needs.
- `python tools/check.py quick`: format and unit tests clean.

## Decisions

- No declared reach override: a body's reach is measured with its own
  motor and controller (`reach::Reach::of`), and a declared reach the leg
  controller cannot fly would strand the bot. Known limit: moves needing a
  second press in the air (wall kick, double jump) are not used by bots.
- Retreat means giving ground in a fight; running away is the separate
  `flee` behaviour.
- The cost bar is a quarter, not a fifth: the measured share is 21 to 24
  percent. The one-brain decision holds: a creature is a small fraction of
  a full bot, and the limit counts it as what it costs.

## Next

- A stomp bounce (the stomper springing up) is the Add-On's, using `push`.
- Bots using a body's special moves (wall kick, triple jump, dive, ground
  pound) is parked, out of scope for v0.3.0 (Max, 2026-10-13): bots run,
  jump and jet through the body's controller with the default brain.
