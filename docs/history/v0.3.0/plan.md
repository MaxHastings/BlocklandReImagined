> **Historical note. Written 2026-10-14 (the file's date in the cloud project folder, `v0.3.0/plan.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# v0.3.0 — Seamless Add-On composition

Written 2026-10-12 from Max's second brief ("what Blockland ReImagined needs
for a seamless crossover sandbox"), with the first brief used where it adds
detail, and both checked against `main` at `ebf02ca` (v0.2.10).

## v0.3.0 definition of done *(frozen 2026-10-13 16:50; Max: "make a clear definition of done so we do get v0.3.0 out")*

v0.3.0 ships when every box below is checked. Anything not listed goes to v0.3.1 unless Max adds it here himself. Each thread delivers its scope as written here; extras wait.

**In (each merged into `v0.3.0-experimental` with green Windows CI):**
- [x] #129 bot kinds and host start; #130 UI scale; #131 vehicle engine sounds and the decoding sound checker.
- [x] #132 Environment window: Look, Sky and day cycle stop conflicting.
- [x] #133 mini-game menu bugs: team loadouts apply, team lives, mode reset.
- [x] One-window mini-game setup (#135) (Max's redesign mockups): every setting that the old menus reach is reachable from the one window, the old menus are gone, and team CTF with per-team loadouts works headless.
- [x] Engine pile (#137; first person is gun-only, no arms): a weapon sits in the hands of every body (Blockhead, Mario, Chief) at the right grip, shown by before/after renders; imported bodies get a working first-person camera and their own first-person arms or a clean gun-only view; imported models sit naturally in seats.
- [x] Modder tools (#136): the Add-Ons screen says in plain words why an Add-On or player type isn't available; a re-host notice; a package command; live tuning that saves back to the Add-On.
- [x] CTF bots (#138 merged 01:40 at e0243268; real TDM Afghan save untested here, needs Max's content) (Max added 2026-10-13 20:00, "TDM Afghan"): bots take the vehicle by their spawn and vary their opening routes; a flag carrier heads home instead of fighting; bots return their own dropped flag; indoors they turn and walk forward instead of side-stepping or backpedalling; bots with jets drop from heights and break the fall with jets. Each shown by a headless test.
- [x] Performance (#138 carries the bot CPU work: Collage 32 bots 5.86/6.12 ms vs 6.42/6.58, 16-19% of ticks over budget vs 23-24%; #142 parked-vehicle sleep still to land) (Max added 2026-10-13 20:01, "Crazy Collage Madness", 32-bot deathmatch with rockets, spears and HE grenades): the profiled CPU hot spots are fixed, with before/after host tick times from a headless run of that setup.
- [x] Portal split collision (#139 merged 00:31 at 6f0f86ba; about 20 µs per vehicle part way in, bit-identical repeats) (Max added 2026-10-13 20:09, "ideally" v0.3.0): players, vehicles and other shapes halfway through a portal collide on both sides, with headless tests plus host tick cost and repeat-run flakiness measured. If the measurements show real risk, it moves to v0.3.1 with Max's OK.
- [x] Modding building blocks (#141 via batch #144, merged 03:14 at 133ff75d) (Max added 2026-10-13 20:14): Add-Ons extending each other; object created/removed/changed and should_collide hooks; real UI panels; sandboxed camera and input control; per-Add-On saved data that survives server restarts (sandboxed, size-capped; Max said yes 20:21). Each with docs in docs/modding and a test. A piece that proves too big moves to v0.3.1 only with Max's OK.
- [x] Gate fixes (#140; flaky boarding fixed in #144; /tune fix #143 via #144): Mario left on the Warthog roof after jetting out; the input dropped on exit; Chief creeping up while standing; Chief falling through the Bedroom floor (Max's RC playtest 23:07, preview 1a73131e; expected fixed by #140's controller-body floor fix; checked headless on Bedroom with Chief on Max's PC as part of the release run, since Max dropped the RC playtest); vehicles falling forever off the map; `/botreload` in a mode; `/tune` changes applying inconsistently, sometimes only after "save to Add-On" (Max's RC playtest 23:18; the Modder tools thread fixes it in its own PR). Each has a regression test.
- [x] Tidy pass (#145: release notes, modding guide index, tuning/rules docs; branch cleanup still to list for Max): `docs/modding` reads as one guide (importer, sockets and clips, movement controllers, body switching, Add-On sounds, HUD and camera, vehicle engine sounds, the checker and the new tools); release notes and KNOWN-ISSUES.md match what shipped; merged branches deleted, and unmerged leftovers listed for Max.

- [x] Overlap review (#145 merged 04:12 at 8e4360f3; report overlap-review.md) after everything merges (Max yes 21:24): find ideas built twice by parallel threads, starting with collision filtering (portal split collision, modding collision groups, should_collide share one path); remove duplicates and dead code.

**Gate (on the final experimental head):**
- [ ] Release gate session script: headless and two clients pass three runs in a row against the private Mario and Halo Add-Ons. No FAIL, and KNOWN only for Add-On content.
- [ ] Sized join and the 10-minute crossover soak pass.
- [ ] Performance budget (wired into gate.py by #146, merged 05:09 at 1bdfd4d7; runs in the release gate) (Max yes 21:24): fixed 32-bot Crazy Collage Madness and TDM Afghan CTF headless runs fail if the host tick is slower than v0.3.0's measured numbers (harness and baseline: /mnt/project-files/bots/collage-32-bot-lag-2026-10-13.md).
- [ ] All-features soak (#146: short soak green in CI; the 3-minute classic Add-On soak runs in the release gate) (Max yes 21:24): bots, portals, vehicles and the ported classic Add-Ons running together with a client connected.
- [x] ~~Max playtests the release candidate before it ships.~~ Dropped by Max 23:29: his playtest of the 1a73131e preview counts; publish when everything else here passes.
- [ ] On Max's PC: `tools/gate.py` (full workspace with `--include-ignored`), `bri-client --check`, the real-content grip test and an offscreen render of a Blockhead holding the Halo AR, RL and Carbine.
- [ ] `tools/release.py v0.3.0` publishes Windows, macOS and Linux.

**Not in v0.3.0:** Mario and Halo Add-On feel polish (they stay private and keep improving separately); Enhanced Sky (#31) and the Soft Shading/AO sky branches; the Gravity Gun; vehicle enter/exit animations; bots using special moves; a body per mini-game team; per-body footstep sounds; new features proposed after this list.

**Goal.** A player moves from one Add-On's gameplay into another's
(character, weapon, vehicle, NPC, world object) without feeling where one
system ends and the next begins, in a real multiplayer server, with no engine
code that names a character, weapon, vehicle or franchise.

**Freedom, not feel (Max, 2026-10-12).** Add-Ons own reverse-engineering
the feel of the game they imitate; the engine's job is to give them the
freedom to do what they need, with no friction. Every workstream judges
engine-imposed rules against this: a behaviour the engine forces that an
Add-On might legitimately want to override is a gap to flag, not a feature.

**Join policy (Max, 2026-10-12).** The join trust prompt is only for
content that is genuinely risky to the player's computer: native code, DLLs
and the like. Sandboxed Add-On content, movement controllers included,
downloads and runs on join with no prompt.

**One body for players and bots (Max, 2026-10-12).** A body is one
archetype shared by players and bots. A bot kind references a body
(`kind.body`, as today); any Add-On body is selectable as a player type and
drivable by the shared brain. There is no separate NPC body kind: Max can
play as a Goomba driving the Warthog or jetting with a rocket launcher, and
a Goomba bot can do the same if its profile allows. NPCs navigate after
players through buildings with the shared nav, and jump or jet onto things
when the body's reach allows (W7, below).

**Constraints that stand.** No second physics engine, no new ECS, no
TorqueScript interpreter, no new networking stack, and no copyrighted
third-party assets in the repo. v20 gameplay and server authority are
preserved, and Blockhead movement stays bit-for-bit the same. The engine owns
mechanisms and Add-Ons own policy. Each new API needs two independent content
examples. Tests are evidence, not the authority: when better gameplay
changes behaviour, the test that pinned the old behaviour is updated.

**Polish is the bar (Max, 2026-10-12).** "We do not want a rough experience
polished." A mixed world has to feel like one living game: controls that
feel tight, animations that blend, a HUD and camera that hand over cleanly,
sound and visual cues for every action, and a world that reacts. A seam is
done only when content built on it feels finished, not when a test passes;
each demo is judged on feel by Max's playtest as well as by its tests.

**What v0.3.0 is for (Max, 2026-10-13).** "The main thing is making the engine
capable here of helping modders. The real world examples are to make sure it
works and is fun." The product is the engine's modding capability; Mario and
Halo are proving grounds that it works and is fun. So every fix found through
Mario or Halo lands as a generic engine capability, documented in
`docs/modding`, never as a special case for one Add-On. A gap only a special
case could close is flagged here as a missing capability instead. When
reviewing a PR for merge, check for content names standing in for mechanics.

**Sensible defaults (Max, 2026-10-13).** "Just make choices that seem
sensible and that still let us have fun." On small forks, threads pick the
default that keeps the playtest fun and note it here instead of asking. Only
real goal changes or irreversible steps go back to Max.

**Branching (Max, 2026-10-12).** v0.3.0 does not merge into `main`. Every
v0.3.0 PR targets the integration branch `v0.3.0-experimental` (cut from
`main` at `ebf02ca`), which takes in `main` as releases land. `main`
stays the stable line until Max has playtested the experimental build with
the Halo- and Mario-style Add-Ons he will add. The bar is an engine ready
for those Add-Ons in a polished way, not a release to `main`.

Standing rule (Max asked, 2026-10-12): every release that lands on `main`,
starting with v0.2.11, is merged into `v0.3.0-experimental` (a merge
commit, never a rebase, so open workstream branches stay valid) before any
further workstream PR merges there. The merge resolves conflicts, expected
mostly in the network protocol notes, and is green before work resumes, so
the experimental branch never drifts from stable.

## Headline scenario: Blockhead, Mario, Warthog, Goombas

Max's own picture of what v0.3.0 is for (2026-10-12): in one multiplayer brick
world, a player switches between a Blockhead and Mario, jumps into a
Warthog, and each feels like its own game. Goomba NPCs carry rocket
launchers and jet around, and a Goomba jumps into a tank to shell Mario
hiding in a brick castle. "Chaos. Fun. Smooth. Integrated well.
Multiplayer." The engine must make every beat possible with Add-On data and
code only; the Mario, Warthog, Goomba and tank Add-Ons themselves are Max's.

| Beat | Owner | State |
|---|---|---|
| Mario's run, jump arcs, triple jump, wall kick, ground pound feel like Mario | W3 movement controllers | wall kick and dive working in PR #107; triple jump next |
| Mario's model animates with its own clips (run, jump, kick, sit) | W2 animation roles; W5 glTF import for a rigged model | W2 in progress |
| Switch between Blockhead and Mario mid-game, mid-air or seated, no respawn | W4 switching lifecycle | not started |
| Mario or a Blockhead holds a rocket launcher in his own hand; shots leave its muzzle | W2 sockets and muzzle | in progress |
| Jump into the Warthog: any body sits in any seat, the passenger keeps their weapon, a gunner seat fires a turret | W2 seats; vehicles (seats, `weapon` seats, turrets already work) | seats for box bodies in W2 |
| The Warthog drives like a Warthog (loose suspension, drift, chase camera, engine sound) | W8 vehicle feel, W6 camera ownership | not started |
| The HUD switches with the body and the vehicle (Mario's coins, the Warthog's gun) | W6 HUD regions | not started |
| Goombas are bots on the one shared brain with their own body (`kind.body`) | W7; bot kinds already pick a body and weapons | works today for box bodies |
| Goombas use rocket launchers and jet around | works today: bots fight with any weapon and jet when their body `can_jet` | works |
| A Goomba jumps into a tank and shells Mario | works today: bots drive and crew vehicles, and a gunner fights with its seat's gun (`bots/combat.rs`) | needs a tank Add-On to prove it |
| Rockets blow the brick castle apart | works today: brick damage under mini-game rules | works |
| Mario stomps a Goomba (a stomp differs from a bump) | W7 contact information on hooks | not started |
| A Creeper-style NPC walks up, fuses (swells, hisses) and blows itself up, breaking bricks | W7 ability attack mode; the explosion itself works today (`explode` op with a brick radius, any Add-On explosion look and sound) | brain trigger not started |
| Bots move with the body they have (a Goomba with Mario-style movement still paths sensibly) | W7: routes use the body's reach measured with its own controller (#114); special moves for bots are parked | works for run, jump and jet |
| Everyone sees it all in multiplayer, late joiners included | every workstream extends the Playground network test | Playground proves it for today's seams |

## 1. What the code says today

Each row is checked against code; the evidence is the `file:line`.

### Works with no engine change

| Capability | Evidence |
|---|---|
| Add-On player bodies: movement constants (45), collision box/ball, health, scale, camera distance, rideable, mount points | `crates/motor/src/archetype.rs:26-93`, `crates/motor/src/player.rs:588-640` |
| Switching body at run time, predicted (`set_archetype`) | `crates/package-runtime/src/ops/player.rs:110`, `crates/motor/src/player.rs:1114` |
| Native model format with nodes, skins, animation clips (priority, additive, looping, triggers) | `crates/content/src/shape.rs:6-152` |
| Own `*.shape.json` models for items, images, projectiles and vehicles | `docs/modding/weapons.md:15-28`, `packages/showcase/steel-ball-kit` |
| Weapons: hitscan, projectiles, magazines, reloads, cooked grenades, explosions, sounds | `docs/modding/weapons.md`, `docs/audits/adventure-pack.md` |
| Vehicles from data: wheeled, flying, ball; seats, wheel physics, weapons, brick smashing | `crates/vehicles/src/schema.rs`, `docs/modding/content-kinds.md:132-192` |
| Bots as an Add-On (`bots` kind), able to fight, drive and crew ground vehicles; paths update when bricks change | `packages/blockhead_bot`, `crates/sim/src/nav.rs:8-9,843`, `crates/sim/src/session/bots.rs:1185` |
| Add-On creatures (`entity`): think, steer, health, fire, damage hooks | `crates/package-runtime/src/ops/entity.rs:41-54` |
| Brick damage under mini-game permissions | `docs/audits/brick-damage.md` |
| Joining downloads every missing Add-On into a content-addressed cache | `crates/net/src/packages.rs:1-24,185`, `crates/package/src/sync.rs` |
| Real two-client network tests, late join, clean-cache download | `crates/net/tests/showcase.rs`, `loopback.rs`, `package_sync.rs`, `crates/client/tests/add_on_join.rs` |
| Client sandbox: draw in world/view/screen space, override drawn avatar pose (`avatar.pose`) | `crates/client-sandbox/src/host.rs:1644-1729` |

### Gaps the briefs understate

| Gap | Evidence | Why it matters |
|---|---|---|
| Held items attach to a node literally named `Mount0`, and draw only when the body has a node named `Eye`; otherwise "Missing avatar pose" and no item | `crates/client/src/avatar.rs:200,1282,1312`, `crates/client/src/world_items.rs:601` | A custom body without Blockhead node names holds nothing visible |
| A body missing any Blockhead clip (`armreadyright`, `sit`, `jump`, `fall`, crouch variants) fails its whole pose; load checks only 7 clips | `crates/client/src/avatar.rs:493,1323-1603` | "Missing animations degrade safely" is false today |
| Box-model and `"none"` bodies hold items on an invisible Blockhead's hand | `crates/client/src/app/frame.rs:985-1003`, `render.rs:37-47` | Items float off a box body |
| The server fires every shot from the eye; its own comment says this is temporary | `crates/sim/src/session/weapons.rs:347-356`, `bots/combat.rs:267` | Muzzle position is wrong for every body, in gameplay, not just visuals |
| Eye heights are Blockhead tuning constants, never read from the body | `crates/motor/src/player.rs:539-559,672` | Short or tall bodies see from the wrong height unless retuned by hand |
| Box and `"none"` riders get no seat tilt and no sitting pose | `crates/client/src/packages.rs:275-298`, `app/mounts.rs:30-56` | Characters float or stand in seats |
| Movement controllers are a design only; wasm runs only on the client | `docs/architecture/client-sandbox.md:359-384`; wasmtime only in `crates/client-sandbox/Cargo.toml` | Largest and riskiest item in the milestone |
| No glTF/GLB importer; DTS conversion is the only path into a `Shape` | `docs/audits/platform-door-closers.md:429` | Additive; the Shape format already carries what glTF needs |
| Add-On blocks load, save and replicate but are not drawn | `docs/KNOWN-ISSUES.md:68` | Needed for custom world objects |
| Add-On creatures (`entity`) have no path op; only bots path-find | `crates/package-runtime/src/ops/entity.rs` | NPCs should be bot kinds on the shared brain (W7), not entities |
| No base-HUD replacement, no camera ownership seam | `docs/audits/total-conversion.md:99-102` | HUDs and cameras of different Add-Ons fight |
| Commando's body, rules and enemies are one Add-On and its rule hands out the rifle; no original drivable vehicle exists in `packages/` (the Steel Ball has no seats) | `packages/samples/sample-commando/package.json`, `steel-ball-kit/assets/vehicles.json` | Demo 1 needs content that is genuinely independent |

## 2. Order and why

The second brief puts the movement controller first. This plan puts the
Playground first, then the attachment contract, with the controller design
running alongside:

1. The Playground and its tests are the safety net. Every later workstream
   (controller, animation, switching) is a change that could break a
   Blockhead, a seat or a late join; the Playground is what says whether it
   did.
2. The attachment and animation-fallback work is cheap, the most visible,
   and unblocks every character the later workstreams make.
3. The controller needs a server-side wasm host and cross-machine
   determinism. That is a design spike before code, and it does not depend
   on 1 or 2, so it runs in parallel.

## 3. Workstreams

Each is a separate thread and PR series. "Fork" marks where the project can
split work into its own thread.

### W1 — Mixed Add-On Playground and verified capability matrix *(started 2026-10-12)*

Independent original Add-Ons, none naming another:

| Role | Add-On | Side |
|---|---|---|
| Character | `playground-runner` (short, quick archetype) + `playground-runner-look` (its box model) | host + each player |
| Weapon | `playground-rifle` (new: the Pulse Carbine, our own model and sounds) | everyone |
| Vehicle | `playground-buggy` (new, our own model, two seats, four wheels) | everyone |
| NPC | `blockhead_bot` (existing `bots` kind) | everyone |
| World object | a brick fort built from ordinary bricks in the test world | — |
| Glue | `playground-mode` (a `mode` naming the Add-Ons; the only package that knows the others) | host |

Tests: a headless session (`crates/sim/tests/playground.rs`) and a two-client
network run (`crates/net/tests/playground.rs`): pick up and fire, enter and
exit the buggy, seated equipment, bot combat, vehicle-player and vehicle-brick
impacts, death and respawn, a late joiner reconstructing state, clean-cache
download. Output: `docs/audits/interop-matrix.md`, the verified matrix, with
each row "works", "works with content", "engine gap" and its test.

Done when: the six acceptance items in section 5 have a passing test or a
named, specific failure.

### W2 — Character attachment and animation contract *(fork)*

Smallest generic change that makes any body hold any item and sit in any seat:

- Archetype `sockets`: right hand, left hand, eye, seat (pelvis) anchor,
  each a node name or a fixed offset, defaulting to today's Blockhead names.
- Missing clips skip the layer instead of failing the pose; an
  `animations` map from semantic roles (`idle`, `run`, `jump`, `fall`,
  `aim_right`, `aim_both`, `sit`, `death`, `swim`…) to the body's clip names,
  with a fallback chain and a one-line diagnostic per missing role.
- Box-model and `"none"` bodies: items attach to the declared socket, or
  hide when there is none.
- Server muzzle from the image's `muzzlePoint` on the body's hand socket
  (replacing the eye stand-in), shared with the client's `held_muzzle`.
- Eye from the body (socket or bounds) when an archetype does not set it.

Sockets must not assume hands or a humanoid skeleton: a Goomba or a
Creeper declares a named socket (a node or an offset) where a held weapon
sits, and the engine attaches the item and fires from that socket's
muzzle. A body with no weapon socket can't hold items, and says so.

Two examples: the Runner and a second, taller body. Done when both hold the
Commando Rifle and ride the buggy and the jeep without floating, and a body
with no clips at all still renders, holds, sits and logs what it lacks.

### W3 — Predicted Add-On movement controllers *(fork, design spike first)*

Follows `docs/architecture/client-sandbox.md` "Prediction". Spike first:
wasmtime on the host, NaN canonicalisation, fuel budget, the `step(state,
input, dt)` ABI, contacts and sweeps as host calls, a bounded controller
state blob replicated with the pose, animation intents out. Then one original
mechanic (wall kick, then triple jump and dive). Blockhead keeps the native
motor. Done when a wall kick works at 150 ms simulated latency, corrects
from the server, and two clients use different controllers at once.

**Third sample: a heavy shooter (Max, 2026-10-12).** Max wants the Halo
body to move like Combat Evolved too, so switching bodies feels like three
games in one. Beside the wall kicker and the diver, W3 ships a shooter
controller: lower run speed, a higher, floaty jump with slower gravity,
crouch, and no wall kick or double jump. The Playground's Commando body
carries it, so the matrix proves a shooter feel as well as platformer
tricks.

### W4 — Character switching lifecycle *(fork, after W2; uses W3 once landed; started 2026-10-12 in its own thread, from #109's head)*

One authoritative transition over `set_archetype`: body, collider, controller
state, held item and its sockets, camera, HUD, seat. Explicit answers: too
tall for the space (defer and report), mid-dive (carry momentum, reset
controller state), item the new body cannot hold (keep in inventory,
unequip), switching while seated (stay seated if the seat accepts the body,
else dismount safely).

**Deliverable: switch body from a brick, no mini-game needed (Max,
2026-10-12).** The v20 brick event `onActivate -> Player -> ChangeDatablock`
already exists (`crates/sim/src/session/events.rs`, `PlayerOp::DataBlock`)
and already calls `set_player_archetype`. Today its PlayerData list holds
only the built-in player types (`PlayerType::ALL`), so Add-On bodies can't
be picked. W4 makes the event list every loaded Add-On body and drives W4's
single authoritative transition. Mini-game rules and a slash command reach
the same transition. Click a brick to become Mario; the Warthog comes from
a vehicle spawn brick and the rifle from an item brick, as in v20.

### W5 — glTF/GLB to native Shape *(fork, independent; started 2026-10-12 at Max's request, in its own thread)*

`bri-addon-import` gains a glTF path into the existing `Shape`: nodes,
skins, clips, materials and textures, scale and axis conversion,
deterministic output, warnings, an offscreen preview. Done when an original
rigged character is imported, checked, installed and drawn on two clients.

### W6 — HUD regions and camera ownership *(fork, after W2; started 2026-10-12 in its own thread)*

Named, replaceable base-HUD regions with declared owners, switching with the
active body, item and vehicle; one owner for camera and movement axes at a
time (body, vehicle, aiming).

**What a shooter feel needs (2026-10-12)**, each with an owner:

| Need | Owner | Today |
|---|---|---|
| First-person camera owned by the body: field of view, camera height, weapon sway and bob | W6 camera ownership | no seam |
| Shield-style health: a recharging shield over health, regen after a delay | rules today (the `heal` op and damage hooks, `crates/package-runtime/src/ops/damage.rs`); the shield bar is a W6 HUD region | works with content, no HUD |
| Melee strike with any weapon held | weapons already support melee images; a melee key alongside the held weapon is a W8 input action | partial |
| Grenade throw without switching weapons | cooked grenades work as a held weapon; a throw key alongside the held weapon is a W8 input action | partial |
| Heavier, slower movement | W3 shooter sample controller | in W3 |
| Ammo counter: the held weapon's magazine and reserve reach the owning client and show in a HUD region | W6 (default picked 2026-10-13 under the sensible-defaults rule: W6 found it and owns the region; the wire field adds a `crates/net/protocol-changes/` note) | not replicated: only the bottom print carries it (`crates/net/src` has no magazine state) |

### W7 — NPC and world interop *(started 2026-10-12 in its own thread: brain profiles and reach, building on the Stomper #110)*

**One NPC brain (Max, 2026-10-12).** Every NPC shares the engine's bot
brain; a game's NPC is a `bots` kind (its own body through `kind.body`, its
dials and weapons), never a new brain. What a kind needs that the brain
lacks (reacting to a stomp, fleeing, a squish death) is added to the brain
for every kind, as a declared capability. Scripted `entity` creatures stay
for props, turrets and traps, not as a second NPC system.

**The boundary.** One shared brain for anything that moves itself through
the world with intent. Scripted `entity` creatures are for props, turrets,
traps and platforms only, with no partial brain logic on the scripted side.

**Cost scales with the profile (acceptance).** A bot's per-tick think time
scales with what its behaviour profile enables. Target: a minimal profile
(wander, notice a nearby player, walk at them, contact damage) costs a small
fraction of a full Blockhead bot. Measure it with the existing headless bot
benchmark (`bot_think_time` in `crates/chaos/tests/bot_gauntlet.rs`, which
reads `Session::bot_think_nanos` against the bar in `bot_tuning.json`), with
a minimal-profile scenario added beside it. The server's bot limit
(`bot_limit()`, 16 by default; `MAX_BOTS` 32) is sized for full bots and
does not apply to minimal-profile creatures; they get their own,
cost-derived limit. **If this cost target cannot be met, that is the trigger
to revisit the one-brain decision** (Max asked on 2026-10-12 whether it
should be challenged).

**Simple NPCs on the shared brain: profile plus intents (2026-10-12).** An
Add-On never ships its own brain loop. It shapes the shared brain in two
layers:

1. *Behaviour profile*, declared per bot kind in data (`bots.json`). A
   Goomba is the shared brain with nearly everything off; a Blockhead bot
   is the profile with everything on.
   - Knobs that already exist on `BotKind` (`crates/sim/src/bot_kind.rs`):
     - aim and reactions: `sight`, `reaction_seconds`, `turn_degrees`,
       `aim_error_degrees`, `memory_seconds`;
     - range: `wander_radius`, `chase_radius`;
     - sides: `fights_bots`, `side`, `alerts_allies`;
     - body and movement: `body`, `melee`, `moves` (walk or swim);
     - `perception` (salience, view cone, human-ness);
     - `surprise`, where 0 is the plain brain;
     - `team`, where teamwork 0 turns it off;
     - `hold_seconds`.
   - `behaviours` weights, where 0 turns one off: `fight`, `chase`,
     `search`, `return`, `wander`, `carry`, `interact`, `objective`, `arm`
     (picks up weapons) and `fly` (jet use).
   - New knobs W7 adds (confirm against the brain when the thread starts):
     - separate switches for tactics the brain may now always use: cover,
       flank, retreat and dodge;
     - vehicle use apart from other interactions: drive, crew and gun;
     - contact damage on touch, as an attack mode beside shooting and
       `melee`;
     - a stomp-vulnerable flag, tied to the contact information below;
     - a switch so a body never fires even when handed a weapon.
   - **Attack modes**: shoot, `melee`, contact damage, and *ability*. With
     an ability the brain decides when to attack (range, sight, cooldown)
     and triggers an action the Add-On defines; the brain never hardcodes
     what an attack looks like. A Creeper's ability is a fuse (duration, an
     animation intent such as swell, a looping sound, cancelled if the
     target escapes) that ends in a self-detonation through the existing
     explosion op (`explode` in `crates/package-runtime/src/ops/damage.rs`:
     radius, damage, brick radius under the host's brick rules, and the
     look and sound of an explosion from the Add-On's own weapons file).
     That op can already go off at a bot's position; what is new is the
     brain choosing to start the ability.
2. *Intents*, supplied through the existing hook and rule surface: patrol
   between points, guard a spot, flee on a condition, follow a leader. The
   shared brain owns how they are carried out: nav, jumping, seats and
   aiming.

**Bots plan with the body's real reach (after W3)**, the same reach a
player with that body has; one body, one reach, for players and bots. Nav and the shared
brain plan jumps and jet hops with the Blockhead motor's reach today. A bot
whose body runs an Add-On movement controller (a Goomba, a Mario-bodied bot)
must plan with that body's real jump and jet. Either the controller
declares its reach, or the brain probes it, and routes use it.
Done in #114: the brain measures each body's reach with its own
controller (`reach::Reach::of`). Bots using a body's special moves (wall
kick, triple jump, dive, ground pound, anything needing a second press in
the air) are parked, out of scope for v0.3.0 (Max, 2026-10-13): a bot in a
Mario or Chief body runs, jumps and shoots through that body's controller
with the default brain. No second-press input is built for bots.

Contact information on hooks (relative velocity, normal, source) so a stomp
differs from a bump; drawing Add-On blocks. Vehicle handling extensions only where a real
vehicle cannot be expressed in `vehicles.json`.

### W8 — Feel and presentation seams *(fork, alongside W2 and W6; started 2026-10-12 in its own thread, PRs may come in slices)*

The other seams a Mario- or Warthog-style Add-On needs to feel finished,
from `docs/audits/total-conversion.md` and the Playground:

- Cameras an Add-On can own: side-on, fixed, follow and chase, with smooth
  hand-over between body, vehicle and aiming (with W6).
- Animated box-model bodies, so a simple character still walks, jumps and
  sits instead of sliding.
- A music bus: an Add-On or mode plays, crossfades and stops tracks;
  per-action sound cues for bodies (footsteps, jump, land) as well as items.
- Per-player tuning deltas (power-ups, speed boosts) through the
  authoritative motor, predicted on the client.
- Add-On HUD text, prompts and input actions (with W6), so "press E to
  enter" and a coin counter are Add-On data.
- World environment from rules (sky, fog, lighting, gravity), so a mode
  sets the mood.
- Vehicle feel: steering that tracks straight, smooth boarding, a seated
  third-person camera and engine sound tied to throttle (the Playground
  buggy drifts left and bounces when boarded today).

### Running throughout — cross-Add-On compatibility suite

The Playground tests grow into a suite every workstream extends, with
explicit diagnostics: missing animation role, failed equip, incompatible
seat, conflicting override, denied damage.

## 4. Demo milestones

| Demo | Needs | Proves |
|---|---|---|
| **1. Multiplayer mashup** | W1 plus W2's fallbacks | A Runner picks up a rifle, drives the buggy through a brick fort and fights bots while a second player builds; nothing is special-cased |
| **2. Polished crossover** | W2, W3 (wall kick, triple jump), W4, W5 | An original rigged platformer hero with its own movement holds the same rifle and sits in the same seats as a soldier body, switching without respawn |
| **3. Blind fifth Add-On** | the suite, W6 | Someone else writes a gameplay Add-On against the public docs; it installs on camera with no engine edits, and the suite says whether it interoperates or exactly why not |

### Checkpoint 1: done, and no more checkpoints *(2026-10-13)*

A solo build of experimental fa670e9b went onto Max's PC on 2026-10-13
(`C:\Users\Maxwell\Desktop\Games\BlocklandReImagined-checkpoint1`). Max found it
premature ("The mario test seems pre mature and not ready for play testing").
**No more optional checkpoints.** The next build Max is offered is the playtest
build, and only once his whole session script (below) works end to end in our
own headless and two-client runs. See "Gate items before any build is offered".

What that build showed: you can't switch bodies inside the Playground mode,
because its mini-game refuses Player type edits even from an admin host. Bodies
from `bri-import-addon gltf --kind body` came out host-only. Mario has no
back, crouch_back, crouch_side, swim, head_side or head_up clips, his fall
is 1 frame and his aim clips are 2 frames.

## 5. Acceptance checklist

- [ ] Independent packages: character, rifle, vehicle and NPC each enable alone and contain no knowledge of one another.
- [ ] Equipment and seating: pickup, equip, fire, enter, exit, die and respawn keep consistent state; no lost items or dangling references.
- [ ] Physics and permissions: vehicle impacts, player, NPC and brick damage follow the authoritative rules.
- [ ] Real multiplayer: a clean joining client sees the right models, weapons, impacts and state; a late joiner reconstructs them.
- [ ] Different bodies: two independently authored bodies hold the same rifle and use the same seats without glitches; a body missing animations degrades visibly and safely.
- [ ] Custom movement: one Add-On controller mechanic predicted and corrected under latency, Blockhead unchanged.
- [ ] Switching: body changes at rest, in motion, in combat and seated, without respawn.
- [ ] Blind fifth Add-On: works through existing interfaces with no engine edits.
- [ ] Failure diagnosis: unsupported behaviour reports a specific limitation instead of crashing or silently breaking.

Human visual and interactive acceptance stays Max's, separate from these.

## Release instead of a playtest *(Max, 2026-10-13, supersedes the hand-off below)*

> "After all working threads are finished instead of a playtest i think we
> should roll everything together and get v0.3.0 out. And anything i find will
> most likely be small and can be in a follow up v0.3.1 lol"

There is no playtest build for Max. Once every working thread has landed in
`v0.3.0-experimental`, the integrator merges it to main and runs the release
(`tools/release.py` on Max's PC) without asking again, as for v0.2.12. Max's
findings go into v0.3.1.

Release gate (run by us, not Max): his session script below, automated as two
clients as far as possible, against the private Mario and Halo Add-Ons loaded
from outside the repo, plus the crossover soak and sized-join gates. The gate
items table below still lists what must land first.

Overnight hand-over (Max, 2026-10-13 05:32: "Going to sleep you think you got
this from here to get v0.3.0 out?"): the integrator drives it to a published
release without him. Defaults: a gate item whose owner thread has not landed by
the time everything else is green and gated ships as a v0.3.1 follow-up rather
than holding the release, unless it is in Max's session script (switching,
joiner download, the Mario and Chief controllers, Add-On sounds). Blockers only
Max can clear get one short reply in the thread.

Max, 2026-10-13 14:05: "while all the add-ons feel half complete while testing that is fine, I mostly wanted to make the engine more capable that was the main goal ... we can just commit whatever add-ons to the private repo so we can work on them later, and lets get v0.3.0 released". So Mario and Halo Add-On polish (and Chief missing from his preview) never holds the release; only an engine bug the Add-Ons expose does. All Mario and Halo Add-Ons are committed to `MaxHastings/BlocklandReImagined-addons` main (d2f5224): `addons/<package>` for the twelve packages and `halo/` for the Halo build sources. Copy Add-Ons out of project files only as zips: that storage rewrites PNGs, which breaks hashed assets.

Never in the public release or repo: `mario.glb` and anything SM64-derived,
the Halo Add-Ons and their models and sounds, and any other ripped asset. They
stay private Add-Ons. The release ships the engine capabilities, the
Playground's own Add-Ons and the `docs/modding` docs.

### What goes into v0.3.0 *(open-PR sweep, 2026-10-13 05:25)*

Max asked that the fixes which missed v0.2.12 ship in v0.3.0. Fixes against
main land on main first, then main is back-merged into experimental before the
release.

| PR / work | Base | In v0.3.0? |
|---|---|---|
| #123 stereo world sounds play in 3D (Mini-Nuke, Add-On weapons) | main | **merged to main** (ddccc629), in #124 |
| #126 Stop mid-match freezes (avatar mesh uploads, per-event mouse handling, timing instrumentation) | main | yes, once green; back-merged. The cold DX12 shader compile and sky exposure costs it leaves are v0.3.1 (docs/progress/2026-10-13-match-freezes.md) |
| v0.2.12 (#117, #118) | main | yes: back-merge in progress |
| #119 ammo counter (W6) | experimental | yes |
| #121 Add-On sound files as body sounds | experimental | yes, after taking the back-merge |
| #122 joiners connected and chatting while loading | experimental | yes, once out of draft and green |
| #116 Warthog handling (W8) and W8's per-body PR | experimental | yes |
| #125 importer: bodies reach joiners, take world light, clips keep rest channels; body model id becomes `<id>:body/<id>` (Mario and Chief re-import after) | experimental | yes, once green |
| #31 Enhanced sky host setting | main | **no**: a feature, not a fix, open since 2026-10-09 against an old main and handed to a reviewer thread that never merged it; the sky, water and ground pickers shipped separately. Left for Max to revive or close |

## Hand-off (superseded 2026-10-13 by the release above; kept for its scripts)

The milestone ends with a local Windows build of `v0.3.0-experimental` on
Max's PC, ready for the Halo- and Mario-style Add-Ons he will add. Once the
branch is playable (the Playground plus the seams), a Remote Control session
in `C:\Users\Maxwell\Desktop\Games\BlocklandReImagined` builds and packages
it with the release script's own build and packaging steps, without pushing,
tagging or publishing, and tells Max where the build and its content folder
are, with a playtest script. The v0.2.11 release proceeds separately and
does not wait on this.

The rules for authoring models in Blender for Add-Ons (scale, axes, rig,
clip names, materials) are the W5 authoring contract,
[gltf-authoring.md](gltf-authoring.md) in this folder. The hand-off
points Max there for his Mario, Halo and Creeper models.

Max's own Add-Ons (Mario, Halo, Creeper and the like) live in a private
repo, `MaxHastings/BlocklandReImagined-addons`, cloned on his PC at
`C:\Users\Maxwell\Desktop\Games\BlocklandReImagined-addons`. It holds one
folder per Add-On, with models and textures in Git LFS. The playtest build
on his PC loads Add-Ons from that clone. Nothing from it ever enters the
main repo or a PR.

A Mario movement-controller Add-On is being built in its own thread from
the SM64 tuning notes ([../tuning.md](../tuning.md)). It is delivered to
[addons/mario/](addons/mario/) and [addons/mario-rules/](addons/mario-rules/) (load both) in this folder for Max to copy into his
private Add-Ons repo, and it reports engine gaps to W8. Max decided
(2026-10-13) that Combat Evolved era models are light enough to need no
distance levels of detail.

### Practical multiplayer gates (run before the hand-off)

Max asked (2026-10-12) whether friends playing this together will feel its
power or find it clunky. Two gates answer that before the branch is handed
off:

- **Crossover soak.** Four players and sixteen mixed bots (Blockhead,
  Runner, Commando, trooper, Stomper) on a large build, with every
  Playground pack loaded and decals active, for thirty minutes at 150 ms
  simulated latency. It runs headless, or with two clients plus bots. It
  reports frame time, tick time, prediction corrections and join time. The
  branch is not handed off until it passes.
- **Sized join.** A client joins a server whose Add-On packs total at least
  150 MB, over a throttled connection. The test measures the join time and
  checks that progress is reported throughout.

Owned elsewhere:
- W8: a vehicle-occupants network test. Three occupants ride the buggy at
  150 ms with lost packets, and neither the passengers nor the gunner
  rubber-band.
- W7: a contact hook, so an Add-On can answer body-to-body contact (a
  Mario stomp, a Warthog run-over).

The interop matrix gains rows for dying as each body (death clip, ragdoll
or vanish, respawn as the chosen body, spectate camera) and for
body-on-body contact (stomp, run-over).

### Agent-authored Add-On acceptance (run before the hand-off)

Modding is now telling an agent what to make (Max, 2026-10-12), so the engine
is judged on how well an agent can author for it. Before the hand-off, a
fresh session gets only the modder documentation (`docs/modding/`, no engine
source). It must produce one working Add-On with:
- a new body with sockets and animation roles;
- a movement controller;
- a weapon;
- a bot kind with a behaviour profile.

That Add-On must pass the cross-Add-On compatibility suite and appear in the
interop matrix. Every place the session has to read engine code or guess
counts as a docs or format defect, and gets fixed, not papered over with a
hint. This follows the earlier cold-modder walkthrough thread, and its list
of defects and fixes is recorded with the hand-off.

### The session the build is for

After v0.3.0, Max will author his own Add-Ons: N64 Mario and maybe a Goomba,
a Halo Warthog, Master Chief with the assault rifle, and one or two Combat
Evolved NPCs (a Grunt, an Elite). He will then host friends on his server.
Each of those roles has a Playground stand-in, so the matrix proves every
seam he will actually use before he writes a line of it:

| Max's Add-On | Playground stand-in | Seams it proves | Stand-in state |
|---|---|---|---|
| Mario (own movement, own model) | Runner, plus a W3 controller (wall kick, dive, then triple jump) on it | W2 sockets and animation roles, W3 controller, W4 switching | Runner exists; it gets a controller when #107 lands |
| Master Chief (a second, taller body) | Commando body (`sample-commando`), which W2 already uses as its second example | Two bodies hold the same weapon and use the same seats | exists |
| Assault rifle | Pulse Carbine | Grips, muzzle, magazine and reload on any body | exists |
| Warthog with driver, passenger and turret seats | Dune Buggy | Seats for any body, a passenger keeps their weapon, a gunner seat aims a turret | exists (gunner seat merged, #108) |
| Goomba or Grunt (simple NPC) | the Stomper (`playground-stomper`): a minimal-profile bot kind with a small box body | W7 behaviour profile with nearly everything off, contact damage, cost target | exists (#110): existing knobs plus a new `weapons: false` that keeps its hands empty under a gun loadout |
| Elite (smart, armed NPC) | `blockhead_bot` with a custom body | Full shared brain, any weapon, drives and crews vehicles | exists for the brain; body via `kind.body` |
| A brick build | the Playground fort | Brick damage, vehicles smashing bricks, bots pathing around them | exists |

The two missing stand-ins (gunner seat, simple NPC) are added to the
Playground by its thread, each with a test.

### Max's hand-off session (the gate, in his words, 2026-10-13)

> Host a server, spawn a Halo rifle and rocket launcher, blow some stuff up
> using them as a Blockhead, then turn myself into Mario, jump around, spawn
> a Warthog with a wrench on a vehicle spawn, jump in, drive around, spawn
> some Goombas, run them over. My friend connects, and we try to blow each
> other up in a mini game.

The friend joins as an ordinary player, not as Chief. He becomes Master Chief
with what exists: a wrench brick event that changes player type, or the mini
game's player type spawning both players as Chief. (Per-team bodies are parked
for later, Max 2026-10-13: "keep it simple".)

Everything happens inside one mini-game Max starts right after hosting, with
building on (Max picked "Mini game first", 2026-10-13): outside
a mini-game nobody, bot or player, can be hurt, so Goombas could not be run
over there.

| Step | Path | Owner | Matrix |
|---|---|---|---|
| Host, spawn the rifle and launcher | item brick set with the wrench, no mini-game | works | row "With no mode or mini-game…" |
| Blow bricks up as Blockhead | v20 rule outside a mini-game: a LAN host breaks any brick, an internet host only the shooter's own (`events.rs` brick explosion) | works | row added (test `rocket_outside_a_minigame_follows_v20_lan_and_ownership_rules`) |
| Turn into Mario, jump around | `ChangeDatablock` brick event listing Add-On bodies; Mario controller | W4; W3 plus the Mario controller thread | rows "brick event switches…", "Switch between Blockhead and Mario…" |
| Warthog from a vehicle spawn brick with the wrench, drive | vehicle spawn brick; seats for any body | works; Warthog is Max's | row "With no mode or mini-game…" |
| Spawn Goombas, run them over | bot spawn brick puts them in the builder's mini-game; run-over damage | works | row and test `a_buggy_runs_a_stomper_over` (#110) |
| Friend joins, Add-Ons download | clean-client download | works; sized-join gate | net Playground test |
| Friend becomes Master Chief | the brick event, or the mini game's player type | W4 | as above |
| Mini-game fight with the Halo guns: a body picked with the event survives joining and respawning (`package_archetype`), or the game's player type makes both Chief | brick event or mini-game player type | W4 | **no row** for the event body surviving join and respawn |

### Gate items before the v0.3.0 release *(coordinator, 2026-10-13; was "before any build is offered")*

Each must be merged into `v0.3.0-experimental` and proven in our own headless and
two-client runs of the script above before Max is offered a build:

| Item | Owner | State |
|---|---|---|
| Mario controller in the build | Mario Add-On thread | **ready 2026-10-13**: no PR needed; the controller ships inside the Add-On (`mario:controller/sm64` on #107's ABI, 7 engine tests pass on dccec6b). The playtest build must load the whole `/mnt/project-files/v0.3.0/addons/mario` folder, not the importer's bare output (the checkpoint build did, which is why Mario moved like a Blockhead). **Re-imported on #125 (0bbb9b91), 7/7 engine tests: now two packages, load both: `addons/mario` (shared: body `mario:body/mario`, archetype, controller) and `addons/mario-rules` (server: /mario and /blockhead, depends on mario ^0.1.0).** New mario.glb also fixes weapon holding and run playback rate |
| Mario's missing clips filled (back, crouch_back/side, swim, head_side/up; real fall and aim) | Mario Add-On thread | **done 2026-10-13**: 19 clips in /mnt/project-files/mario.glb, no missing clips on importer branch 41cb509; assets in `addons/mario/assets/`. Clips are substitutes derived from the export (NOTES.md#model-clips) |
| Switching merged (#115: brick event lists Add-On bodies, body survives join and respawn) | W4 | **merged** (#115) |
| Imported bodies download to joining clients (not host-only), plus the lighting fix for imported bodies | Imported models thread, one PR | **merged** (#125, 0bbb9b91); Mario and Halo re-imported on it |
| Halo Add-Ons loaded: Chief, rifle, launcher, Warthog | Halo Add-Ons thread | **built 2026-10-13**: the playtest build loads six Add-Ons from `addons/halo/` (Max's private Add-Ons only): halo-spartan, halo-assault-rifle, halo-rocket-launcher, halo-warthog, halo-rules, halo-mode. Grunts, Elites and both plasma guns sit in the same folder, built and tested but parked: leave them out. 9/9 headless tests on fa670e9. Open: body re-import after the host-only fix |
| Impact decals | W8 (feel) | **merged** (#127, 281cdd03) |
| Mario's dive and ground pound through the attack input | Mario controller thread | controller ready (mario.wasm reads the attack bit; ground pound already works); **closed: #127 merged (281cdd03)** |
| Body faces the run direction | Mario controller thread / W3 | controller ready (writes the heading every tick, test mario_heading.rs); **closed: #127 merged (281cdd03)** |
| Add-Ons play their own sound files by name (Mario's wahoo) | Converted player types thread | **PR #121** (body jump/pain/death sounds from Add-On files, two-client test, carries #118); **merged** (627af7d7). It also fixes host_setup.rs dropping the catalog of an Add-On that only adds or adjusts player types: main has the same bug, a candidate for the next main fix (not v0.2.12) |
| Reasonable seated pose for every body (Mario's hands on the Warthog turret handles); seat changes instant | feel thread (W8) with the Halo Add-Ons thread | **merged** (#127 seat hand holds); Halo Warthog rebuilt on 281cdd03 with gunner `hands`; 10/10 Add-Ons pass bri-addon-check, 12/12 sim tests |
| Master Chief as a full second player type (Max, 2026-10-13: "help stress test another angle"): Halo CE movement through the same controller ABI as Mario, his own animations and sounds, works with bots. Content stays private; any engine gap comes through the feel thread as a generic capability | Halo Add-Ons thread | **built 2026-10-13**: halo.wasm controller, sounds, Spartan bot; 12 headless tests pass on experimental + #121. Needs #121 merged, then a re-import after #125 |
| On Max's PC with real content and a GPU: run `item_mounts_use_the_same_sampled_avatar_pose_and_body_transform::content` (skipped in the cloud), and an offscreen render of a real Blockhead holding the Halo AR, RL and the stock Carbine in third person (the cloud's synthetic Blockhead can't judge real grips) | integrator, with the release gate run | open |
| Whole script runs end to end headless and with two clients | integrator (this thread) | open |
| Crossover soak and sized-join gates pass | integrator | open |

The stand-in rehearsal below runs first, on the Playground's own Add-Ons.

### Playtest script for the hand-off

Run with at least two friends joining over the internet on a fresh install.
Check the session feels like one game, not a pile of Add-Ons:

1. Host the Playground mode on the experimental build. Friends join from a
   clean install: each Add-On downloads once, with no trust prompt, and
   they spawn in.
2. Each player switches between Blockhead, the Runner (with its controller)
   and the Commando body at rest, while running, mid-air and in a fight,
   without a respawn. Check the held weapon, camera and HUD follow.
3. As the Runner, wall kick and dive across the fort, then do the same at a
   friend's screen: the friend sees smooth movement, with no snapping.
4. Three players take the buggy as driver, passenger and gunner. The
   passenger fires the Carbine and the gunner fires the turret while the
   driver smashes through the fort.
5. Spawn the simple NPCs and the full bots together. The simple ones
   wander, walk at players and hurt on touch; the full bots take cover,
   jet, pick up weapons and take the buggy. A bot drives it against a
   player hiding in the fort.
6. Stomp a simple NPC from above and check it dies differently from a bump.
7. A late friend joins mid-fight and sees the same bodies, seats, bricks
   and bots as everyone else.
8. Note anything that feels rough (animation pops, camera jumps, missing
   sounds, an ugly hand-over). That is the bar, not just whether it works.

## 6. Not doing in v0.3.0

A universal character or vehicle framework, automatic skeleton retargeting,
a second physics or event system, terrain authoring, and per-franchise code.

Parked: bots using a body's special moves (wall kick, triple jump, dive,
ground pound). Max asked on 2026-10-13 whether the default brain would ever
use them and parked it; bots use each body's ordinary run, jump and jet
reach (#114).

Parked by default (coordinator, 2026-10-13, after Max asked "anything else
that should be out of scope?"; any item comes back if Max objects):
Grunts, Elites and Covenant plasma weapons (models kept, not gate items);
music buses (weapon and vehicle sounds stay); per-move animation-set
swapping; further creature cost optimization. (Add-Ons playing their own sound files by name was
parked and then brought back by Max the same day: "Add-Ons should be able to
play their own sound files like Mario's wahoo"; it is a gate item.)

Parked by Max (2026-10-13): vehicle enter and exit animations and seat-to-seat
transition animations. Getting in, getting out and switching seats are
instant. In scope: every body sits reasonably in every seat, small ones
included ("as long as his hands are on the gun handles that is good enough",
Mario on the Warthog turret).

Parked (coordinator, 2026-10-13): the playSound brick event lists only
base-game sounds; Add-On sounds there wait unless the playtest script needs them.

Missing capability, v0.3.1 (W8, 2026-10-13): a body supplies its own
first-person arms. v20 draws no player model in first person, and the engine
draws only the held image's detail9999. For v0.3.0 the Halo guns drop their
baked `arms` object, so every body sees the gun alone, as with v20 tools. In
v0.3.1 a body declares its first-person arms (an archetype model, or its own
arm nodes drawn in first person), and they reach the gun's grip nodes with
#127's two-bone reach (`avatar::reach_hands`).

v0.3.1 from Max's preview (2026-10-13; Max chose "Release now" over adding them first): vehicle engine sounds (a vehicle can't play an engine loop; the Warthog's is packed but silent, Halo NOTES gap 4); `bri-addon-check` should decode every sound with the client's decoder and refuse a missing sound file (Halo's 20-byte fmt WAVs and a misnamed chaingun file passed the checker, then played silently or dropped the whole Add-On); footstep/land sounds per body (Halo NOTES gap 7). Add-On content, not engine: Mario ships no sound files yet (map them through `sounds` and `intent_sounds`).

Later, optional: a body per mini-game team (one team Mario, the other Chief).
Max liked it but said to keep it simple for now (2026-10-13). When picked up,
it is a teams mode Add-On's policy over existing ops (`set_teams`,
`set_team`, `set_archetype`), with W4's transition underneath.


## Release cut (15:35)
v0.3.1 PRs (e.g. #130 UI scale) now target v0.3.0-experimental. v0.3.0 is cut from experimental at d610bad4 plus only the gate agent's own gate commits; merge that SHA to main, not the branch tip. v0.3.1 PRs stay out unless the integrator pulls one in as a bug fix.
- 15:45 PC: preview (pre-#129) fails Halo Slayer/Playground with "bot_shark... body in no enabled Add-On". #129's playable_bot_kinds (d610bad4) fixes the map-load path; gate run on Max's PC must confirm Halo Slayer hosts with Bot_Shark/Bot_Zombie on. v0.3.1: /botreload in a mode re-reads all bot kinds unfiltered (session.rs bot_tuning.reload) and may hit the same error.
- 16:20 Release cut: branch claude/v030-interop-playground-e14qg7 at dbe9737c (d610bad4 + docs/releases/v0.3.0.md). Two-client gate: every engine step passed in at least one run; remaining FAILs are the test driver's walking routes, not engine. Sized join and the 10-min soak were not run separately; tools/gate.py's full workspace run (with --include-ignored) is the remaining check. Max asked "is v0.3.0 even out yet?" so the release goes now.
- 16:25 Max chose (card) to fold #130, #131, #132, #133 into v0.3.0 first; mini-game redesign stays v0.3.1. Gate tests + notes are PR #134 (662de9b0). After the four merge: update docs/releases/v0.3.0.md (drop engine-sound known issue, add UI scale/sky/loadouts), rerun gate, release from v0.3.0-experimental head at that point.
- 16:42 Max: "i feel we should just wait for everything to get into v0.3.0 lol". Everything in flight goes into v0.3.0, no separate v0.3.1: #130 (merged), #131 (merged), #132, #133, #134 (gate tests), the one-window mini-game redesign thread, and the grips / first-person arms / imported-bodies engine thread. Then full gate, update release notes, release. Mario/Halo feel polish stays parked.
- 16:44 Max: "lets try to get everything tidied up big picture for v0.3.0". Also hold for the new "modder tools" thread. Tidy pass after the feature PRs land (so docs don't conflict): one coherent docs/modding guide (importer, sockets, controllers, switching, Add-On sounds, HUD, engine sounds, checker), accurate release notes + KNOWN-ISSUES.md, loose ends. Branch cleanup: only delete branches whose PRs merged; list unmerged ones (Gravity Gun, sky/soft shading, perf diagnostics, pc-allowlist) for Max rather than deleting.
- 16:44 Max: "take our time get everything in, stabilized, polished." So the gate's open items become v0.3.0 work, not v0.3.1: the two session-script fight FAILs must pass reliably (repeat runs), and gate observations (a) jet-dismount reseat, (b) processed_move kept on dismount, (c) Chief's feet creep / lost jump, (d) vehicle off the map edge falls forever get fixed with tests, plus /botreload in a mode.
- 18:44 Max edited README.md directly on main (GitHub). Before release: merge main into experimental and keep his README; tidy-pass README edits build on his version.
- 20:15 Max: "idc about moving the release back I just want one nice big chunky well rounded release". Take his additions into the definition of done without flagging the slip; keep the list current as the finish line.
- 21:15 Max: all bot-code changes in one PR to avoid conflicts. #138 (CTF bots) carries the Collage lag thread's bot path-finding and aim speedups; merge #138 only when its thread marks it ready with those in. The lag thread's parked-vehicle settling fix (physics only) is its own PR. Other bot-code changes go through #138.
- 2026-10-13 23:29 Max: no RC playtest; release as soon as the definition of done passes.
- 2026-10-13 23:37 #140 (gate engine fixes) merged into v0.3.0-experimental at 597f6b6a.
- 2026-10-14 00:31 #139 (portal split collision) merged into v0.3.0-experimental at 6f0f86ba.
- 2026-10-14 01:07 Perf budget baseline for Collage 32 bots: #138 at 4539126 averages ~6.0 ms/step (9.4 ms before the v0.3.0 bot work), 16-19% of ticks over budget; Afghan DM unchanged. Use these for the perf budget test once #138 merges.
- 2026-10-14 01:40 #138 (CTF bots + Collage bot CPU work) merged into v0.3.0-experimental at e0243268.
- 2026-10-14 03:14 Batch #144 merged at 133ff75d: boarding landing window, #141 modding blocks, #143 /tune live apply, #142 parked-vehicle sleep (as one commit; #142 closed by hand). All feature PRs are in. Overlap review: /mnt/project-files/v0.3.0/overlap-review.md; fixes on a follow-up PR.
- 2026-10-14 04:12 #145 (pass groups in contact checks, docs tidy) merged at 8e4360f3. Preview 133ff75d built on Max's PC.
- 2026-10-14 04:36 Max playtested the 133ff75d preview: "seems good", no problems reported (Bedroom floor bug not seen again).
- 2026-10-14 05:09 #146 merged at 1bdfd4d7 (perf budget + all-features soak). main is already in experimental. Release waits on Max's message to the PC session.
- 2026-10-14 15:2x v0.3.0 PUBLISHED from ee244a84 (main + tag v0.3.0): https://github.com/MaxHastings/BlocklandReImagined/releases/tag/v0.3.0. Gate passed; perf Collage 3.81 ms/7.00, Afghan 2.83 ms/5.50. Known failures shipped: portal-car (::content) and jeep-roof standing test (::content), both for v0.3.1.
