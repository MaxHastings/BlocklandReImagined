# Status

One page so everyone starts from the same place: Max in the morning, or any
new thread. Revised 2026-09-28 around 09:50Z from project memory, the
coordinator's morning notes and the docs linked below. It summarises; the
linked docs are the evidence. Where sources disagreed, a note says which one
this page followed.

Main is `f95d273` (protocol 36). Max played a14, built from `84f9ac9`
(`dist\BlocklandReImagined-alpha-2026-09-28-a14` on his PC). a15 is being
built from `f95d273`.

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

Current phase: feature freeze. Max's playtest of a15 matters most; work until
then is bugs, first impressions, robustness, and modder docs.

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

| # | Item | State this morning |
|---|---|---|
| 1 | A new player can download, host or join, and play; all 20 first-impressions items closed | Partly. See the table below and the night-QA findings. |
| 2 | A modder gets from zero to an Add-On with the guide; v20 imports work | Partly. The rewritten guide is on main. Night QA imported a v20 weapon with the button and a brick pack with `bri-import-addon.exe`, and played both in a hosted game. Not covered: a guest joining a host that runs imported Add-Ons. |
| 3 | Reported bugs fixed; the gate has zero known failures | Partly. `tools/gate-known-failures.toml` lists no failures, only tests the gate can't run. Night-QA findings A to E below are still open. |
| 4 | A 1 h multi-player soak with save and reload | **Done.** Four players for 3608 s on Slate over LAN: no disconnects, every save and reload exact, 120 Hz held, 694 to 726 MB, zero warnings. The night-QA doc counts 4 of 4 reloads; the coordinator's note says 5. |
| 5 | A home test, then a small group playtest | Max started on a14; a15 is next. |

### First-impressions items

From [audits/first-impressions.md](audits/first-impressions.md), updated
with the night-QA check of the new-player screens on a13 code. Night QA ran
headless, so anything that needs a real window was not checked.

| # | Item | State |
|---|---|---|
| 1 | Double-click opens nothing; startup errors invisible | Fixed on main (`87321c0`); needs a window to check. |
| 2 | Crashes are silent | Fixed on main (`87321c0`); the dialog names the .dmp (PR #15). Needs a window to check. |
| 3 | Unsaved work is lost | Fixed on main: autosave and an unsaved-changes prompt (PR #5). |
| 4 | Damaged settings stop the game | Fixed on main (PR #5). |
| 5 | Joining by IP | Pass in night QA; the "no answer" message is plain. |
| 6 | Rejoin loses your bricks; no reconnect | Fixed on main (`3ee8ab3`); not exercised overnight. |
| 7 | One bad save hides every save | Fixed on main (PR #5). |
| 8 | No warning before a bad plant; undo | **Open.** Still no warning before planting. Night QA found and fixed the blocker where clicks never placed a ghost (`012c760`). Undo not exercised. |
| 9 | Raw internal disconnect text | Pass, except the Tutorial join refusal (finding B). |
| 10 | Loading feedback | Partly. The host's loading screen is fine; the guest's has no map name or preview. The startup splash is parked. |
| 11 | Tutorial end to end | Not checked; hosting it for two fails (finding B). |
| 12 | Frame cap, presets, render distance | Partly. Presets and Max FPS work; no render distance. First Options visit no longer saves 800x600 and VSync off (`70f293b`). |
| 13 | Text size, colourblind, subtitles | **Open.** None in Options. |
| 14 | Join passwords and brick limits not applied | Open: passwords are one of the choices for Max. |
| 15 | Name prompt; duplicate names | **Open.** No prompt; guests join as "Blockhead". |
| 16 | Dedicated server persistence | `bri-server` autosaves (PR #5); resume not checked. |
| 17 | Version, updates, debug symbols | On main via PR #15; a14's `--version` shows it; the menu corner not checked in a window. |
| 18 | Brick search, duplicator | **Open.** Neither exists. |
| 19 | Input options | **Open.** No hold or toggle choice for crouch, walk or jet; no gamepad. |
| 20 | Music slider, live preview | Pass. |

## What's on main

a14 carries all of this (main `84f9ac9`):

- **a13's contents:** PR #1, the stress campaign (games download a server's
  Add-Ons on join), the shadow fix, the sandbox fixes, Add-On weapon ids,
  and rejoin after a drop.
- **Gate and build:** test binaries run in parallel pools, docs-only pushes
  skip builds, stderr reaches the log before exit, the lean build profile
  (`060ae1d`), and a `port_bound` rule so tests that host on fixed ports run
  one at a time.
- **PR #15, ready for friends:** version line and `--version`, a GitHub
  Releases check, crash files named, optional signing, first-run quality
  from the GPU. Package builds must set `$env:BRI_VERSION` to match
  `-Version`.
- **Guide walkthrough:** the guide ([modding/README.md](modding/README.md))
  rewritten cold, `bri-addon-check` checks weapons, and `bri-addon-run`
  runs an Add-On with a host and a guest headless.
- **Red-team fixes:** idle or spoofed connections can't fill a host, Add-On
  recursion can't crash the game, a hostile host can't hang a join, trust
  covers only what the prompt showed, and a LAN listing can't override a
  saved pin.
- **Consistency audit fixes, two batches:** 25 findings fixed in a14
  (26 on main with #32 below), each listed with its commit in
  [audits/orthogonality.md](audits/orthogonality.md).
  Among them: one save snapshot, one build gate, one water query, one game
  clock for visuals, the tick survives a failing system, Add-On bricks
  plantable after hosting, joining or Change Map (`8f9f418`), firing from the
  crouch-blended eye (`e065e16`), one targeting ray (`e3376fc`), one range
  per setting (`003c3aa`), sitting replicated as state (protocol 36).
- **Night QA fixes:** clicks place the ghost after a brick is in hand
  (the release blocker), the Options defaults above, and message boxes grow
  to fit their text (the firewall question was cut off).
- **Revived water feel** (`5721b96`): v20's splash, exit-sound, zone and
  swimmer rules, an underwater tint, froth and bubbles.

## Landed after a14, in a15

- Audit #32: joining picks the spawn the way a respawn does
  (`56f7964`).
- Camera revive: the first-person eye at v20's Eye node, v20's third-person
  camera and zoom ramp, keyboard turn speed, hammer and wand reach from the
  Eye node (`8118df4` to `3faffcd`).
- Night QA: the harness, its findings doc, and a refused click stays out of
  the bottom print instead of showing "No weapon image equipped"
  (`fb45617`).
- Draining the stderr tee can no longer hang the exit (`7a3d604`); it held
  the gate lock this morning until Max stopped it.

Not revived yet: `revive/chat-emotes` and `revive/admin-orb` (not on main).

## Open findings

From Max's a14 play:

- **Turning the Stress Lab Add-Ons off in the Add-Ons menu leaves the
  Stress Lab HUD in game.** A PC thread is on it.

From night QA, full list in
[audits/night-qa.md](audits/night-qa.md). The map and mode matrix passed
10 of 15.

- **A. The Slopes:** the first brick near spawn is refused as Buried.
- **B. Tutorial hosted as LAN** refuses every joiner: "Player spawn is
  obstructed". Keep it single player, or word the refusal for players.
- **C. Stress Lab Strata:** a guest can't build on the generated ground, and
  sees no plant-error icon. Needs a decision (below).
- **D. The Stress Lab HUD shows on every map** with the shipped Add-Ons.
- **E. Stress Lab Strata saves include the generated ground,** so loading
  one stacks about 15 000 bricks on the regenerated world.
- **Slate Sea and Slate Storm** (from the matrix): bricks land on the
  seabed, out of hammer reach.

The doc also lists smaller ones:
Slate's ground missing in offscreen captures, the Import button has no v20
reference for shared sounds, the guest loading screen, and no hint on
Connect to IP.

## Audits

- [audits/orthogonality.md](audits/orthogonality.md): 40 ranked findings,
  what's fixed, and what's left. Not started: 10, 13, 25 (large refactors)
  and 37.
- [audits/red-team.md](audits/red-team.md): hostile Add-Ons, hosts and
  clients; what's fixed and what isn't.
- [audits/night-qa.md](audits/night-qa.md): overnight headless QA of a13,
  with the map and mode matrix, the soak and the new-player screens.
- [audits/first-impressions.md](audits/first-impressions.md),
  [audits/platform-door-closers.md](audits/platform-door-closers.md),
  [audits/engine-foundations.md](audits/engine-foundations.md),
  [audits/bug-sweep.md](audits/bug-sweep.md).

## Parked, and why

- **Startup splash.** Content loads before the window opens; left for later
  in [audits/engine-foundations.md](audits/engine-foundations.md).
- **Stress campaign next steps.** Block faces in the renderer, a saturation
  round, predicted driven entities, animated box models (see
  [stress-lab/HANDOFF.md](stress-lab/HANDOFF.md)). Held by the freeze.
- **`revive/visual-compare`.** It drives v20 itself, so only with Max's OK.
- **`revive/docs-drift`, `revive/creature-notes`.** Not revived.
- **An easy script tier (for example Rhai) for modders.** Suggested as an
  after-playtest idea. Not started.
- **Terrain.** Opus's data, conversion and collision pieces are kept; the
  playtest ships the finite map-bundle-014 path.

## Open decisions for Max

From the consistency audit's "Choices for Max", which now has ten:

1. Join passwords: wire them up, or hide the three fields.
2. Player cap: 32 or 64, and whether bots take a slot.
3. Slash commands: which v20 commands to add (`/magicwand`, `/spy`, `/ret`).
4. "Fast 1st/3rd person switch": add v20's smooth transition, or hide it.
5. Harmful event outputs: whether SetVelocity, AddVelocity and Dismount from
   someone else's brick need a mini-game, as damage does.
6. Particle and rain fog: v20's behaviour not yet checked.
7. Trust rules: one table for every tool, or each tool keeps its v20 rule.
8. Add-Ons on Change Map: re-run them so a game mode survives, or end it.
9. Add-On blasts on bricks: permanent, or temporary like weapon blasts.
10. Vehicle respawn: keep the owner's mini-game with a 1 s floor, or v20's
    damage source and burn time.

From the red-team audit ([audits/red-team.md](audits/red-team.md)):

- **Server identity changed:** today a failed pin is forgotten silently.
  Proposed: an SSH-style Continue or Cancel screen. Max's call on wording.
- **Download size prompt:** a server can make a join download up to 4 GiB
  with no question. Proposed: ask above about 200 MB.
- **Per-player request limit:** one player can stall everyone's commands
  for 10 s at a time. The fix needs the size of the largest legitimate
  request.

From night QA:

- **Tutorial** (B): single player only, or keep LAN with a clear refusal.
- **Generated ground** (C): world blocks public, or an icon and a message.
- **Stress Lab Add-Ons** (D): ship them off, or show the HUD only when its
  server Add-On runs.

## Things only Max can do

- Play a15, then a small group playtest.
- Try a build on a weaker PC.
- Publish GitHub Releases, so the update check has something to find.
- Buy a code-signing certificate, if wanted. Optional; signing is already
  behind a parameter.
- Look once in a real window at Slate's ground, the double-click start, the
  crash dialog and the firewall prompt. Night QA couldn't.

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
