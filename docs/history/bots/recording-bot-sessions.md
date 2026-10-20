> **Historical note. Written 2026-10-12 (the file's date in the cloud project folder, `bots/recording-bot-sessions.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Recording a game with bots so it can be debugged later

Written 2026-10-12 for the v0.2.10 playtest (local build f4ce0ae).

## Tonight's steps

1. Before you host, open the console (`~`) and type `recordmatches 1`. The setting is kept, so you only need to do this once. Type `recordmatches 0` to turn it off.
2. Host as usual and add bots. Every game you host from then on is recorded to
   `%LOCALAPPDATA%\BlocklandReImagined\recordings\match-<n>.brimatch`
   (that is `C:\Users\Maxwell\AppData\Local\BlocklandReImagined\recordings`). Each new game gets the next number.
3. When a bot does something odd, write down roughly how far into the match it happened, which bot it was (its name, or what it looked like) and what it did. The F3 overlay shows what the bot you are looking at is thinking, and a screenshot of it helps.
4. To hand one over, tell a thread which match number, the time and what you saw. Claude can read that folder on your PC directly, so you don't need to copy anything.

Keep in mind:

- Only the newest ten recordings are kept, so a game you care about is deleted after ten more hosted games. Copy it somewhere else, or tell us about it soon.
- A replay needs the same build and your content folder, so it runs on your PC. If you pull or rebuild before we look, tell us the recording came from f4ce0ae and we'll replay it on a worktree at that commit.
- Recording makes each tick about a third more expensive (about 165 µs to about 220 µs per tick in the 50k-brick test). In that heavy test the file grew by a few MB a minute; a normal game with a few bots is smaller.
- The file holds names, chat as typed and identity keys. Admin and host passwords are replaced by stand-ins.
- Only the host records. A game you join is not recorded on your side.

## What a recording holds

Everything the host was given, in order: the starting world, map, mode, settings, content, Add-On state, admin state and bot dial overrides. Then every join, leave, command (including chat, `/addbot` and other bot commands), movement, seat and camera report, and map change, plus the outside reads the host made (the clock and file reads). After every tick it keeps a digest of the match state. Once a second it also keeps each bot's five-line F3 readout.

Bots run inside the simulation, so replaying those inputs re-runs every bot decision exactly: their targets, routes, vehicle seat and push claims, objectives, shots and chat calls. Nothing about the bots has to be stored separately for a replay to reproduce them.

It does not hold video or what your own client showed you, and a replay only works with the build that made the recording.

## Reading the bots out of a recording (draft PR #89)

Before the PR, `bri-replay` could only say whether a recording replayed the same. It could not show you what a bot was doing at a given moment. PR #89 (https://github.com/MaxHastings/BlocklandReImagined/pull/89) adds that:

```
bri-replay <content-root> match-12.brimatch --bots 4:20-4:40
```

This prints every bot's name, position, velocity, facing and health ten times a second in that window, along with its F3 readout and its full thinking: behaviour, route leg, who it sees or remembers, seat or push claim, goal, objective plan, drives and decisions with each candidate's score, the controls it pressed, the shot it planned and the fire gate. Each chat line said in the window is printed too. `--every 1` prints every tick.

The PR doesn't change the recording format, so recordings made tonight with v0.2.10 can be read this way once it lands, or with a build of the PR branch.
