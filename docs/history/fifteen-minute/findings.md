> **Historical note. Written 2026-10-20 (the file's date in the cloud project folder, `fifteen-minute/findings.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# 15-minute mini-game path: partial findings (parked 2026-10-20)

Method: desk walk of release v0.3.3 docs and code. Not a timed run; no game was launched, so no minutes are measured.

## What already exists
- Rule Workshop (Escape menu, host/admin only) places 9 editable recipes: switch, team door, puzzle, race, king of the hill, kill scoring, soccer, sandbox, add-on switch. Race, hill and soccer cover two of the three ideas (tag has no recipe).
- Recipes place only sensor bricks and rules. The stranger still builds the arena, walls and spawns.
- Friend join: pick Internet, wait for UPnP, paste the bri:// invite (/invite). If the router won't open UDP 28000, the stranger needs port forwarding. That is the likeliest 10+ minute stall or dead end.

## Where a stranger gets stuck (from docs/code)
1. The entry is called "Rule Workshop", is hidden in the Escape menu, and is described as experimental.
2. No "Tag" recipe. No arena included with any recipe.
3. Inviting a friend is a chat command; no button.
4. Internet join depends on router behaviour. Unmeasured.

## Ranked fixes (by expected minutes saved, unverified)
1. Make friend join not need router knowledge (relay or clearer fallback).
2. Recipes that come with a ready arena and spawns.
3. Rename Rule Workshop to "Make a Game" and add an Invite a friend button (done on branch, compiles).
4. Add a Tag recipe.

## Code on branch claude/fifteen-minute-minigame-ki5ju2
Rename to "Make a Game", plainer header text, "Invite a friend" button (runs /invite), Settings button shortened. `cargo check -p bri-ui` passes. Not run through the quick/focused tiers.
