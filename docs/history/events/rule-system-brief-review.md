> **Historical note. Written 2026-10-02 (the file's date in the cloud project folder, `events/rule-system-brief-review.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: the "Rule/Event System Overhaul" brief

Checked against main at e05af0df (v0.1.15-alpha). Opinion only, nothing built.

## 1. Overall take

The direction is right and it agrees with what the repo already says
(platform-principles.md: "semantic operations are the mutation boundary",
"the event system's typed intents are the seed", Torque is an input format).

The timing is wrong for most of it. The brief is a beta-freeze plan: it
designs the permanent save format, a new editor, provenance, variables,
revision-aware editing and an eleven-phase roadmap. The repo's own roadmap
puts that work in "Phase 2: vanilla as packages; first beta", after vanilla
is done and after the mod spikes. And on 10-01 you agreed to run a friends
playtest before starting big new work, so the playtest picks what comes next.

Recommendation: keep the brief as the north-star doc for beta, run the
playtest first, then build one small slice (section 5).

## 2. What the repo already does

| Brief asks for | Already on main |
|---|---|
| One catalog of facts/actions | `crates/events/src/catalog.rs:102`, `content/events-pack-002/catalog.json` (16 inputs, 65 outputs), used by editor, importer and runtime |
| Deterministic scheduler, origins, budgets, cancellation, delays, stale generations | `crates/events/src/runtime.rs` (Limits at `:8`), documented in `crates/events/README.md` |
| Semantic actions, not string dispatch | `Intent` enum, `crates/events/src/model.rs:304`, applied in one place `crates/sim/src/session/events.rs:1723` |
| Canonical damage path | `damage_player` `crates/sim/src/session/combat.rs:1011`, already called from weapons/combat, bots, vehicles, tools, events and packages |
| Score / end round / reset as ops | `SetScore`, `ResetMinigame`, `EndRound` in `crates/package-runtime/src/ops/minigame.rs:62,79,186` |
| Add-Ons add event vocabulary | `FireBrickInput` / `FireGameInput` `crates/package-runtime/src/ops/brick_events.rs:28,48` (Slayer's onMinigameDeath etc.) |
| Instigator | `instigator()` `crates/sim/src/session/events.rs:1012` (client provenance kept separate from target slots) |
| Unresolved legacy rows kept, never run, round-trip | `Row.preserved` `crates/events/src/model.rs:115`; `bind_world_events` in `crates/convert` |
| v20 is an import format, not runtime | events crate has no Torque reader; importer is separate (`crates/events-import`, `bind_world_events`) |
| Separate checkpoint schema for pending work | `save()/restore()` exact-version checkpoint, events README "Persistence" |
| Durable namespaced ids | `bri_package::id` `namespace:kind/name` (door-closers P0, done) |
| Add-On hooks for policy | `on_death`, `on_damage`, `on_spawn`, ... (`docs/architecture/package-runtime.md:90`) |
| Each script op is one typed impl | `ops/list.rs` + `session/packages/perform/` (`docs/architecture/script-operations.md`) |

So roughly half the brief describes the existing system in new words.

## 3. What is strong

1. "Add-Ons extend vocabulary, not grammar." Exactly right, and a natural
   next step from FireGameInput (Add-Ons already add inputs; outputs and
   conditions are the missing half).
2. Command / Fact / Action / Query / Cue as separate ideas. Cheap to adopt
   in docs now, and it stops "event" meaning five things.
3. The "could this fact survive a rewrite?" test for public facts.
4. Three schemas (authored, compiled, checkpoint). The runtime already
   splits compiled vs checkpoint; naming the authored one is the gap.
5. Instigator with a defined lifetime, not "last touched forever". Matches
   the 10-01 proposal's Last Toucher rule (contract item 6).
6. Steel-Ball Soccer as the architecture test. It touches physics,
   attribution, teams, delays and reset in one game. Same as the 10-01
   "first test build".
7. "Do not rewrite" list. Good guard rails.
8. Explicit failure rule (actions atomic, run in order, earlier ones stay).
   The runtime already behaves like this (`Applied` visible before next row,
   `Rejected` counted); it just needs writing down in one place.

## 4. Overreach or premature for an alpha

1. Freezing a RuleProgram format and a compatibility promise. Alpha rule is
   "no migrations, formats are disposable" (platform-principles.md
   principle 1). Designing the beta contract now means designing it twice.
2. Replacing the Wrench Events editor. v20 players know it; the product
   direction is v20-faithful. Add an "only if" field to it first instead of a
   new WHEN/IF/DO/AFTER editor.
3. Revision-aware multi-builder editing. Nobody has hit this yet. Wait for a
   playtest report.
4. Cause/provenance chain for every action, plus per-origin budgets for
   projectiles, entities, network and storage. Origins already exist for
   events; the door-closers audit has the rest as P1 "when the system is
   touched". A global provenance system is the "generic operation bus" the
   audit says not to build yet.
5. Five variable scopes with full lifecycle tables. Start with the two the
   10-01 proposal found real games need: per-player Progress and a per-brick
   Counter.
6. Object rules, Region rules, MiniGame rules as owners. Brick is the only
   owner today; regions are just bricks with On Enter/Leave in the earlier
   proposal. One owner is fine until a second one is forced.
7. The 12-game acceptance list (Payload, Infection, Trench hybrid, ...)
   before any freeze. Fine as a beta exit check, too big as a starting plan.
8. Formal simulation phase contract. The audit already rates this P2 and
   says to name phases in comments and tests first.
9. Converting Slayer/CTF policy into rules. Slayer works today as an Add-On
   using ops and hooks. "Rules for one build, Add-On for any build" (the 10-01
   answer) is simpler than moving it.

## 5. Smallest first slice (after the playtest)

This is the 10-01 proposal's "first test build", with three ideas from the
new brief folded in. No new editor, no new format, no freeze.

1. Write a one-page note in `crates/events/README.md`: Fact vs Action vs
   Cue, the failure rule, and how Instigator is chosen. No code.
2. Add On Enter / On Leave to bricks (swept, with an occupancy list), as
   catalog inputs.
3. Add a Last Toucher record on physical objects (pushed, shot, driven,
   thrown, grabbed; cleared on respawn) and expose it as the Instigator.
4. Add one "only if" check per event line in the existing wrench editor
   (Object kind, team).
5. Add Win Round / Add Team Score outputs that call the existing
   `EndRound` / `SetScore` code, so the event and the Add-On op share one
   path.
6. Prove it with Steel-Ball Soccer as an automated test, then reuse the
   same code for a Jeep checkpoint and King of the Hill with no special
   cases.
7. Then let an Add-On register one output (not just inputs), so Add-Ons
   extend the vocabulary in both directions.

If those seven steps go cleanly, the brief's RuleProgram, editor and freeze
can be designed from what actually got built, at beta.
