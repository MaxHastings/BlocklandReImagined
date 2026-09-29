# Status

One page so everyone starts from the same place: Max, or any new thread.
Revised 2026-09-29. It summarises; the linked docs are the evidence, and
[progress.md](progress.md) has the dated history of each build.

## Release state

Test builds are named by date and letter (for example `2026-09-28-a23`)
and show that name and their commit in the main menu. Each is recorded in
[progress.md](progress.md) with its commit and protocol version. The first
public build on GitHub Releases is
[2026-09-28-a20](https://github.com/MaxHastings/BlocklandReImagined/releases/tag/2026-09-28-a20);
it is unsigned. Later releases go up only on Max's word.

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

## Decisions already made

Don't reopen these without Max.

- **Joining never fails over Add-Ons.** Joining downloads every Add-On the
  host runs that the player lacks or has a different copy of. The only
  question asked is trust for Add-On code; missing art or sounds fall back
  to stock ones instead of blocking.
- **Cosmetics stay local.** Anything purely visual or audible (debris,
  particles, knocked-out bricks tumbling) runs on each player's PC and sends
  nothing over the network. Anything that could change what a player can do
  is synced.
- **Hosting is direct IP only.** No join codes, relay, Steam, EOS, STUN or
  any other hosted service. The public address comes from the router only
  (UPnP, then NAT-PMP). The Windows firewall rule is named
  "Blockland ReImagined". Hosts without a public address get a home-network
  invite. Only UDP 28000 needs forwarding; 28050 is LAN discovery only.
  The shelved join-codes patch lives in the project's shared files, not
  the repo. See [architecture/hosting.md](architecture/hosting.md).
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
- **Default Add-Ons.** The Stunt Plane and the Duplicator ship turned on
  as Add-Ons, not base content.
- **Add-Ons can change the game completely**, through generic hooks only;
  no genre code in the engine. Seams still to build are in
  [audits/total-conversion.md](audits/total-conversion.md), "Future work".
- **Performance is the headline:** a million bricks at 60 fps.
- **Event limits** follow the alpha contract (1024 rows per brick, 300 s
  delays), not v20's 100 rows and 30 s.
- **Budgets, not pacing.** Work is limited by budgets; anything that
  affects the simulation is budgeted deterministically, and wall clock is
  only a watchdog.
- **Deterministic tests only.** A test that failed and then passed alone
  never blocks a landing. Heavy fuzz and soak runs stay small in the gate,
  and wall-time checks run only as benchmarks (`BRI_BENCH`).
- Not bugs, by Max's call: the dismount sound, floating hips.
- The v20 screenshot comparison is cancelled.

## Release definition of done

Agreed with Max on 2026-09-28.

| # | Item | State |
|---|---|---|
| 1 | A new player can download, host or join, and play; all 20 first-impressions items closed | Nearly. See the table below. |
| 2 | A modder gets from zero to an Add-On with the guide; v20 imports work | **Done.** The guide is on main, and v20 weapon and brick packs import and play. |
| 3 | Reported bugs fixed; the gate has zero known failures | Ongoing. `tools/gate-known-failures.toml` lists no failures. |
| 4 | A 1 h multi-player soak with save and reload | **Done** on a13 code: four players for 3608 s on Slate over LAN, no disconnects, every reload exact ([audits/night-qa.md](audits/night-qa.md)). |
| 5 | A home test, then a small group playtest | Max has played test builds in multiplayer. The group session is for later (see "For testers later"). |

### First-impressions items

From [audits/first-impressions.md](audits/first-impressions.md). Items that
need a real window are for Max to confirm.

| # | Item | State |
|---|---|---|
| 1 | Double-click opens nothing; startup errors invisible | Fixed (`87321c0`). Max to confirm a console window or error shows. |
| 2 | Crashes are silent | Fixed; the dialog names the .dmp (PR #15). |
| 3 | Unsaved work is lost | Fixed: autosave and an unsaved-changes prompt. |
| 4 | Damaged settings stop the game | Fixed. |
| 5 | Joining by IP | Fixed. |
| 6 | Rejoin loses your bricks; no reconnect | Fixed (`3ee8ab3`). |
| 7 | One bad save hides every save | Fixed. |
| 8 | No warning before a bad plant; undo | The ghost turns red before a plant that would fail (`c4c993b`). |
| 9 | Raw internal disconnect text | Fixed. |
| 10 | Loading feedback | Partly: the guest's loading screen has no map name or preview; the startup splash is parked. |
| 11 | Tutorial | First run offers the Tutorial (`78df917`); it stays single player. |
| 12 | Frame cap, presets, render distance | Fixed (`1932015`). |
| 13 | Text size, colourblind, subtitles | Fixed: UI Size, colour-vision modes (`b7566e9`), sound captions (`a3cdc7b`). |
| 14 | Join passwords and brick limits | Password fields hidden; Server Settings apply. |
| 15 | Name prompt; duplicate names | First run asks your name (`78df917`); duplicates are numbered. |
| 16 | Dedicated server persistence | `bri-server` autosaves and resumes its newest save. |
| 17 | Version, updates, debug symbols | Fixed; release builds keep the .pdb as a CI artifact, not shipped to players. |
| 18 | Brick search, duplicator | Fixed: brick search (`cc96712`) and the Duplicator Add-On (`d8484f0`). |
| 19 | Input options | Toggle Crouch, Mouse 4 and 5, and a gamepad while playing (`c2e7941`). |
| 20 | Music slider, live preview | Fixed. |

### Defaults picked

Max asked for everything merged without waiting on open choices, so each got
a safe default. Any of them can be revisited after the playtest. The list
of release defaults and their reasons is in
[audits/orthogonality.md](audits/orthogonality.md), "Release defaults".
Among them: Toggle Crouch off, no vehicle crash damage and Server Settings
at v20's defaults (256,000 bricks, 10 per second for non-admins), all as in
v20; join passwords hidden; rockets outside mini-games knock bricks out for
30 s, like v20 ([audits/brick-damage.md](audits/brick-damage.md)).

## Known issues

[KNOWN-ISSUES.md](KNOWN-ISSUES.md) is the player-facing list, and
[FEATURES.md](FEATURES.md) lists every v20 feature still missing. From
night QA, not yet re-checked: a Strata guest can't build on the generated
ground; Strata saves include the generated ground; on Slate Sea and Slate
Storm bricks land on the seabed out of hammer reach; no hint on Connect to
IP.

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
- [audits/v20-parity.md](audits/v20-parity.md),
  [audits/v20-fidelity.md](audits/v20-fidelity.md): v20 feature and
  item-by-item comparisons, with what is still open.
- [audits/net-graph.md](audits/net-graph.md): the net graph and
  performance overlay.
- [audits/bug-patterns.md](audits/bug-patterns.md): the bug patterns seen
  so far and how to avoid each; read before fixing a bug.
- [audits/v20-behaviour.md](audits/v20-behaviour.md): v20 behaviour checked
  against the decompiled scripts.
- [audits/total-conversion.md](audits/total-conversion.md): the Add-On
  seams a total conversion uses, and the ones still to come.
- [audits/first-impressions.md](audits/first-impressions.md),
  [audits/platform-door-closers.md](audits/platform-door-closers.md),
  [audits/engine-foundations.md](audits/engine-foundations.md),
  [audits/bug-sweep.md](audits/bug-sweep.md).

## Parked, and why

- **Startup splash.** Content loads before the window opens; left for later
  in [audits/engine-foundations.md](audits/engine-foundations.md).
- **Stress campaign next steps.** Block faces in the renderer, a saturation
  round, predicted driven entities, animated box models (see
  [stress-lab/HANDOFF.md](stress-lab/HANDOFF.md)). Not started.
- **`revive/visual-compare`.** It drives v20 itself, so only with Max's OK.
- **`revive/docs-drift`, `revive/creature-notes`.** Not revived.
- **Terrain.** Opus's data, conversion and collision pieces are kept; the
  playtest ships the finite map-bundle path.

## Things only Max can do

- Say whether a console window or error shows when you double-click the
  game.
- Try the skis, and Demo Pong in Bedroom: load it, click the ramp, and use
  the + and - buttons.
- Upload the content for release builds once (`python tools/ci_content.py
  upload`, see [release-builds.md](release-builds.md)), then push a version
  tag. GitHub Actions builds and publishes the release the update check
  reads.
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
