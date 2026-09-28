# Status

One page so everyone starts from the same place: Max, or any new thread.
Revised 2026-09-28 around 13:20Z from project memory, the coordinator's notes
and the docs linked below. It summarises; the linked docs are the evidence.
Where sources disagreed, a note says which one this page followed.

Main is `64b1487` (protocol 40). The current build is a16, from `64b1487e9`:
`C:\Users\Maxwell\Desktop\Games\BlocklandReImagined\dist\BlocklandReImagined-alpha-2026-09-28-a16-stress-lab`.
The folder kept "-stress-lab" in its name because Max launched it before it
could be renamed. a15 (`f95d273`) and a14 (`84f9ac9`) are beside it in
`dist`.

## What this is

Blockland ReImagined rebuilds Blockland v20 in Rust. The original v20
experience is the fidelity reference: original art, music and sounds keep
their identity, and the modernisation is mostly underneath. The v20 install
is only ever read. See [alpha-contract.md](alpha-contract.md) and
[playtest-contract.md](playtest-contract.md).

North star: the game is easy to modify at the core and at the seams. The
engine owns mechanisms, Add-Ons own policy, and there is no game-specific code
in engine crates. One rule per concept, with no opt-in lists for properties
that should be universal. See
[architecture/platform-principles.md](architecture/platform-principles.md).

There is no feature freeze. One was suggested overnight, but Max never agreed
to it. Max's playtest of a16 matters most.

## Decisions already made

Don't reopen these without Max.

- **Hosting is direct IP only.** No join codes, relay, Steam, EOS, STUN or
  any other hosted service. The public address comes from the router only
  (UPnP, then NAT-PMP). The Windows firewall rule is named
  "Blockland ReImagined". Hosts without a public address get a home-network
  invite. The shelved join-codes patch lives in the project's shared files,
  not the repo. See [architecture/hosting.md](architecture/hosting.md).
- **Windows only.** Other platforms are not a goal.
- **Players see "Add-Ons".** "Package" is an internal word.
- **Add-On client code is sandboxed, with trust tiers.** Data needs no
  prompt. Sandboxed wasm and WGSL asks once per server ("Trust and join"),
  and again when the code's hash changes. Elevated capabilities (network,
  the Add-On's own folder) get their own per-Add-On prompt. Native plugins
  are a later tier: deferred, not forbidden, not built. See
  [architecture/client-sandbox.md](architecture/client-sandbox.md).
- **Pre-release scope:** quality of life, first impressions, robustness,
  stability, modder docs and samples. Out of scope: legal questions and a
  server list.
- **No compatibility or migration work.** Pure game development until the
  first beta; schemas freeze then. Max pre-approved all breaking and protocol
  changes.
- **Headless evidence is enough** for agents. Max does all interactive
  playtests (see AGENTS.md, "User's testing boundary").
- Not bugs, by Max's call: the dismount sound, floating hips.
- The v20 screenshot comparison is cancelled.

## Release definition of done

Agreed with Max on 2026-09-28. Not yet in the roadmap docs; this page is its
home until then.

| # | Item | State for a16 |
|---|---|---|
| 1 | A new player can download, host or join, and play; all 20 first-impressions items closed | Partly. See the table below. |
| 2 | A modder gets from zero to an Add-On with the guide; v20 imports work | Partly. The guide is on main, and night QA imported and played a v20 weapon and brick pack. Hosts now share their Add-Ons with joiners, downloading what's missing (`6074aad`, `169549a`); a guest joining a host with imported Add-Ons was not covered by night QA. |
| 3 | Reported bugs fixed; the gate has zero known failures | Partly. `tools/gate-known-failures.toml` lists no failures. Max's two-machine map mismatch is open (fix coming in a17). |
| 4 | A 1 h multi-player soak with save and reload | **Done** on a13 code: four players for 3608 s on Slate over LAN, no disconnects, every reload exact, 120 Hz held, zero warnings ([audits/night-qa.md](audits/night-qa.md)). |
| 5 | A home test, then a small group playtest | Max is testing a16. The group session is for later (see "For testers later"). |

### First-impressions items

From [audits/first-impressions.md](audits/first-impressions.md), updated
from night QA and the commits since. Night QA ran headless, so nothing that
needs a real window was checked.

| # | Item | State |
|---|---|---|
| 1 | Double-click opens nothing; startup errors invisible | Fixed (`87321c0`). Max to say whether a console window or error shows on a16. |
| 2 | Crashes are silent | Fixed; the dialog names the .dmp (PR #15). Not seen in a window. |
| 3 | Unsaved work is lost | Fixed: autosave and an unsaved-changes prompt (PR #5). |
| 4 | Damaged settings stop the game | Fixed (PR #5). |
| 5 | Joining by IP | Fixed (PR #13); passed night QA. |
| 6 | Rejoin loses your bricks; no reconnect | Fixed (`3ee8ab3`). |
| 7 | One bad save hides every save | Fixed (PR #5). |
| 8 | No warning before a bad plant; undo | The ghost turns red before a plant that would fail (`c4c993b`). Undo not exercised. |
| 9 | Raw internal disconnect text | Fixed (PR #5); the Tutorial refusal went away with item 11. |
| 10 | Loading feedback | Partly. The guest's loading screen has no map name or preview; the startup splash is parked. |
| 11 | Tutorial | Kept single player (`3fc7e4f`). The first-run welcome and Tutorial offer come in a17. |
| 12 | Frame cap, presets, render distance | Fixed: presets, Max FPS and v20's Max Draw Distance slider (`1932015`). |
| 13 | Text size, colourblind, subtitles | Partly: a UI Size setting (`64b1487`). Colourblind modes come in a17. |
| 14 | Join passwords and brick limits | Password fields hidden (`ac00991`); Server Settings now apply the brick limit, plant rate and chat length (`d22e5d6`). |
| 15 | Name prompt; duplicate names | Duplicate names are numbered (`1b1c0fc`). No name prompt yet. |
| 16 | Dedicated server persistence | `bri-server` autosaves and resumes its newest save (`1b1c0fc`). |
| 17 | Version, updates, debug symbols | Fixed (PR #15); builds ship the .pdb. |
| 18 | Brick search, duplicator | Brick search comes in a17; no duplicator. |
| 19 | Input options | Toggle Crouch option and bindable Mouse 4 and 5 (`bb41fb3`); no gamepad. |
| 20 | Music slider, live preview | Fixed (PR #10). |

## What's new in a16

Since a15 (`f95d273`), 33 commits. The full list is in the gate thread.

- **Add-Ons:** the host's Add-Ons follow joiners, downloading what's
  missing. Toggling an Add-On applies at the next game, and a HUD shows only
  on servers that run its Add-On, which fixes the Stress Lab HUD Max saw in
  a14 (`49c7165`). A changed server identity asks Continue or Cancel, and
  Add-On downloads over 200 MB ask first (`364a427`).
- **v20 fidelity:** brick damage (below), the admin camera and orb, chat
  colours and emotes (/hug, /zombie, /bsd, /wtf), skis, Demo Pong playable in
  Bedroom, tire emitters, per-datablock impact sounds, horse fall damage,
  no crosshair outside first person, and bricks resting flush on map floors.
- **Building:** a red ghost before a failing plant; planting into sloped
  terrain works (The Slopes); Server Settings apply.
- **Options:** Max Draw Distance, Toggle Crouch, Mouse 4 and 5, UI Size.
- **Tests** host on free ports, never 28000 or 28050.

### Brick damage, as in v20

See [audits/brick-damage.md](audits/brick-damage.md). Outside mini-games,
rockets knock bricks out for 30 s in single player and on LAN, and knock
out only your own bricks on internet servers. Only the hammer, the wand and
undo remove bricks for good.

### Defaults picked

Max asked for everything merged without waiting on open choices, so each got
a safe default. Any of them can be revisited after the playtest.

- Toggle Crouch off, as in v20.
- No vehicle crash damage; v20 never applied any.
- Server Settings at v20's defaults: a 256,000-brick limit and 10 bricks per
  second for non-admins.
- The consistency audit's ten choices: join passwords hidden, everything
  else kept as it was. The list and reasons are in
  [audits/orthogonality.md](audits/orthogonality.md), "Release defaults".
- Red-team: the identity and download questions are built (above), and each
  player now has their own request budget, so one player can't stall the
  rest ([audits/red-team.md](audits/red-team.md)).

## Known issues in a16

- **Two-machine map mismatch** Max hit: fix coming in a17.
- The Kitchen's main floor still sits slightly off the grid.
- From night QA, not yet re-checked: a Strata guest can't build on the
  generated ground; Strata saves include the generated ground; on Slate Sea
  and Slate Storm bricks land on the seabed out of hammer reach; the guest
  loading screen; no hint on Connect to IP; the Import button has no v20
  reference for shared sounds.

## Coming in a17

The fix for the two-machine map mismatch, the first-run welcome and Tutorial
offer, brick search, colourblind modes, the Ctrl+N net graph and an F3
performance overlay, explosion debris and more v20 effects, Advanced Config,
a sortable Join Server list, and the server list font.

## Audits

- [audits/orthogonality.md](audits/orthogonality.md): 40 ranked findings,
  what's fixed, the release defaults.
- [audits/red-team.md](audits/red-team.md): hostile Add-Ons, hosts and
  clients.
- [audits/night-qa.md](audits/night-qa.md): headless QA of a13, with the map
  and mode matrix, the soak and the new-player screens.
- [audits/brick-damage.md](audits/brick-damage.md),
  [audits/skis-v20.md](audits/skis-v20.md),
  [audits/pong-events.md](audits/pong-events.md),
  [audits/water.md](audits/water.md): v20 comparisons behind a16's changes.
- [audits/first-impressions.md](audits/first-impressions.md),
  [audits/platform-door-closers.md](audits/platform-door-closers.md),
  [audits/engine-foundations.md](audits/engine-foundations.md),
  [audits/bug-sweep.md](audits/bug-sweep.md).

The v20-parity, v20-fidelity and net-graph audits aren't on main yet.

## Parked, and why

- **Startup splash.** Content loads before the window opens; left for later
  in [audits/engine-foundations.md](audits/engine-foundations.md).
- **Stress campaign next steps.** Block faces in the renderer, a saturation
  round, predicted driven entities, animated box models (see
  [stress-lab/HANDOFF.md](stress-lab/HANDOFF.md)). Not started.
- **`revive/visual-compare`.** It drives v20 itself, so only with Max's OK.
- **`revive/docs-drift`, `revive/creature-notes`.** Not revived.
- **An easy script tier (for example Rhai) for modders.** Suggested as an
  after-playtest idea. Not started.
- **Terrain.** Opus's data, conversion and collision pieces are kept; the
  playtest ships the finite map-bundle-014 path.

## Things only Max can do

- Say whether a console window or error shows when you double-click a16.
- Try the skis, and Demo Pong in Bedroom: load it, click the ramp, and use
  the + and - buttons.
- Publish GitHub Releases, so the update check has something to find.
- Buy a code-signing certificate, if wanted. Optional; signing is already
  behind a parameter.

## For testers later

Other PCs (including a weaker one), a clean Windows install, playing over
the internet with port 28000 forwarded, sending crash logs, and a small
group session.

## How to work here

- Read AGENTS.md first. It has setup, the push gate and the testing
  boundary.
- **Push to main only through the gate:** `python tools/gate.py --push`.
  Never `--no-verify`. Never force push main. Branch protection is on, and
  PRs need the Windows GitHub Actions check.
- **Batch landings.** Gate runs are serialised on one lock. The gate
  rebases, so squash big branches before handing them in.
- **Cloud threads push only their own branch** and send the head to the
  coordinator; the gate or cleanup thread lands it.
- **PC worktrees** go under
  `C:\Users\Maxwell\Desktop\Games\BlocklandReImagined-worktrees\<name>` with
  `CARGO_PROFILE_DEV_DEBUG=0`. One worktree and branch per thread. Delete a
  finished worktree's `target/` with `python tools/clean_targets.py`.
- **Never `git stash`**, never run `cargo fmt` workspace-wide, never kill
  processes by image name.
- Commit work in progress before pausing a thread.
- Record decisions, evidence and commands in [progress.md](progress.md).
- Cloud containers have about 30 GB of disk: build only the crates you need,
  with `CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`.
