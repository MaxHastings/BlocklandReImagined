> **Historical note. Written 2026-10-05 (the file's date in the cloud project folder, `round1-soccer-main.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Bot watch round 1: Soccer Field Goo, slot 1, v0.2.4 (main 43c7b178)

Headless host on the save's map (Skylands, 52,187 bricks, all placed), Max's
Add-On choices, slot-1 preset applied as Load slot then Save & Reset: Slayer
Team Deathmatch, Team Bot Fill 4 per team, loadout sword. Host stays in the
game on Team 2 standing at spawn (as Max would be). 180 s after a 2 s warm-up.

## Numbers
- 8 bots (4 v 4). 74 kills, 0 team kills, 0 self kills, 0 falls/void deaths.
- Goals: 0. Team scores 0 to 0 the whole match (see "nobody can win").
- Stuck 0.6%, circling 1.9%, idle with enemies alive 11.0%, goof 8.4%.
- Behaviour share: chase 56%, fight 15%, interact 15%, wander 13%.
- Items: sword held 99.9% (264 sword swings), spray can once, one alarm emote.
- Bot think time 0.37 ms per tick for 8 bots; 180 s ran in 21 s wall.

## Dumbest / weirdest moments
1. **They never play soccer.** On 70% of samples every bot's objective
   readout says "objective action budget exceeded": the planner gives up on
   this save, so the field's goals are never an objective. It is a sword
   brawl on a soccer pitch. (BotThought.objective_diagnostic)
2. **Chase/wander flip-flopping.** 28 five-second windows where a bot switched
   behaviour 8+ times, mostly chase > wander > chase > wander every half
   second (frames 00-02, clip0). Looks like dithering at a perception edge:
   31% of all wander time is spent with an enemy *visible*.
3. **Idle in a deathmatch.** 11% of bot time is "wander" while enemies are
   alive in the same game.
4. **Interact with nothing.** 15% of time in "interact" with no objective, a
   third of it with no item in hand.
5. **Nobody can win.** All players are bots and the preset has Slayer's Kill
   Bot points at 0 (points_kill_player 0 too), so 74 kills score nothing and
   the round never ends. That is Max's preset, not a bug, but in a bots-only
   game it reads as "the game is broken". Worth a decision.

## Tooling notes for the bot lane
- The repo's shared `content/addons` imports are stale against main: with
  them Slayer's bot fill dies in `bot_name` ("Function not found:
  data_lines"), so no bots appear at all. Blockhead Bot, steel ball, gravity
  gun, portal and ragdoll are not installed in the dev content either. I run
  from a scratch content root laid out like a release: base packs, a fresh
  import of the bundled originals with this branch's importer, and the repo
  packages from the branch under test.
- `soccer_probe` at e41b040 stops right after configuring the game with
  "Error: Stale/replayed command" (a command sequence number reused after
  ConfigureMiniGame), so it produced no match. Full stderr attached.
- Max's own add-on-health.json from his last v0.2.4 game: "bot 8
  (bot.blockhead) could not think this tick: Empty slot".
