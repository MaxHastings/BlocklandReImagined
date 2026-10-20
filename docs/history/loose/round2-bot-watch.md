> **Historical note. Written 2026-10-06 (the file's date in the cloud project folder, `round2-bot-watch.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Bot watch round 2: v0.2.4 (main 43c7b178) vs bot lane tip af324cc

Each run is a headless host on one of Max's saves with his Add-On choices,
8 bots filled by Slayer team bot fill (4 v 4, or 5 v 3 where the saved game
asked for 5), the host standing at the map's spawn on a team, and a 2 s warm-up.
The other saves run 180 s, and Close Quarters runs 120 s per loadout. Saves
without their own mini-game borrow Soccer Field Goo's Slayer game (Team
Deathmatch; CTF where marked). Content: a release-style folder with the bundled
originals re-imported and the branch's own repo packages. Full per-run table:
table.md. The "lane" column is af324cc (push broom WIP).

## Headline numbers

| save (loadout) | kills main / lane | no-killer deaths main / lane | stuck % main / lane | idle with enemies alive % main / lane |
|---|---|---|---|---|
| Soccer Field Goo (sword) | 82 / 79 | 0 / 0 | 0.8 / 1.4 | 13 / 9 |
| A TDM, Slate | 8 / 12 | 0 / 0 | 0.6 / 0.9 | 89 / 94 |
| Afghanistan DM, Slate | 7 / 4 | 0 / 0 | 6.1 / 6.4 | 79 / 84 |
| TDM Red Vs. Blue, Slopes | 105 / 106 | 0 / 0 | 1.1 / 1.0 | 15 / 3 |
| race, Slopes | 36 / **0** | 0 / 0 | 0.6 / 0.1 | 4 / 0 |
| My Vehicles, Slate | 16 / 10 | 0 / 0 | 0.0 / 0.5 | 79 / 86 |
| C.T.F. Battle Grounds (CTF) | 14 / 14 | 0 / 0 | 0.1 / 0.3 | 80 / 85 |
| Maxwell's DogFight, Slopes | 14 / 31 | 0 / 0 | 0.0 / 0.6 | 0 / 0 |
| Close Quarters, rocket | 4 / 9 | 8 / 16 | 5.7 / 9.4 | 75 / 35 |
| Close Quarters, gun | 15 / 20 | 5 / 7 | 7.3 / 11.4 | 59 / 37 |
| Close Quarters, gravity gun | 0 / 0 | 7 / 5 | 12.4 / 16.5 | 52 / 41 |
| Close Quarters, spear | 12 / 4 | 15 / 9 | 9.7 / 18.6 | 23 / 35 |
| Close Quarters, bow | 16 / 16 | 1 / 7 | 14.6 / 15.6 | 28 / 24 |
| Close Quarters, HE grenade | 3 / 1 | 5 / 9 | 22.4 / 16.3 | 2 / 3 |
| Close Quarters, sword | 17 / 17 | 5 / 5 | 7.4 / 13.7 | 47 / 46 |
| Close Quarters, butterfly knife | 10 / 15 | 2 / 5 | 19.5 / 25.4 | 29 / 16 |
| Close Quarters, mixed five | 13 / 13 | 9 / 11 | 14.5 / 11.1 | 41 / 45 |

## Dumbest / weirdest moments, worst first

1. **Lane regression: frozen in front of an enemy they can't reach (race,
   Slopes).** On af324cc all 8 bots sit in "fight" for the whole 180 s,
   standing still, 0 kills, nobody loses a single hit point. The four on
   Team 1 lock onto the host, who stands at the map spawn, out of their reach.
   On main the same save gives 36 kills. Frame: r2-lane-race-frozen.png.
2. **Close Quarters: they walk off the floating build.** 8 to 16 deaths with
   no killer in 120 s with rockets, 15 with spears on main, and some in
   every loadout. Each one falls about 45 units from the build's lowest floor
   (y about 331-336) down to the bedroom floor (y 286). The edges are all
   around that floor, not one spot. When they stepped off they were in
   search, fight or chase about equally, so it isn't only knockback: gun
   and sword bots fall too. Clip: r2-cq-fall.gif.
3. **Close Quarters: stuck against walls in "search".** Stuck time is 6-25%
   (knife 25% on lane). 74 of 78 stuck episodes on main are "search" with
   **no next waypoint**: the bot has a search point but no route, so it
   presses into a wall or corner. The worst spots are the bottom storey's
   corridors around (-20, 338, 88), (-20, 348, 80) and (-32, 347, 88). On
   Max's question: none of the 78 started at the spawn point. Every stuck bot
   had walked at least a few units from where it spawned, so this is
   bot routing, not a spawn bug. Frame: r2-cq-stuck-corner.png (Ashley
   wedged in a corner with a teammate standing inside her). Spawn points
   under the low ceilings are not caught yet. My runner reads positions, not
   head clearance.
4. **Big maps: a deathmatch where nobody looks for anybody.** On A TDM,
   Afghanistan DM, My Vehicles and the CTF, bots spend 80-94% of the time in
   "wander" while enemies are alive, about half of that goofing (look,
   emote, stare, hop). The objective readout says "no grounded objective
   plan" or "unsupported rule semantics", so they have no reason to cross
   the map. 7-16 kills in 3 minutes. Frame: r2-tdm-idle.png.
5. **CTF: the flags are never touched.** C.T.F. Battle Grounds in Slayer CTF
   mode: 0 captures, 80%+ wander, "unsupported rule semantics" most of the time.
6. **No vehicle is ever used.** My Vehicles, race and DogFight: mounted 0%
   on both builds.
7. **Gravity gun: 0 kills on both builds**, despite some "carry" time.
8. **HE grenade in tight corridors:** self kills (2 main, 1 lane), circling
   16-18%, 25 flip-flop windows.
9. **Soccer Field Goo: still a sword brawl.** "objective action budget
   exceeded" on about 70% of samples on both builds, so the push broom
   never gets a ball to clear. Chase/wander flip-flopping got worse on lane
   (23 windows vs 17).

## Tooling notes
- `soccer_probe` at af324cc still stops with "Stale/replayed command" right
  after configuring the game.
- The Mini-Game window's Save & Reset settings did not take in my runner:
  the AddOnSettings request is Accepted, but neither the mode nor the
  team bot fill changes on a game made with Create. I worked around it with
  the save's own Slayer game. It could be my harness. If not, a slot loaded
  onto a fresh game wouldn't take either. The lane's UI-path probe would
  settle it once it runs.
- The shared dev content's imported Add-Ons are stale against main (Slayer's
  bot fill fails in `bot_name`).
