# Status

One page so everyone starts from the same place: Max in the morning, or any
new thread. Written 2026-09-28 around 06:00Z from project memory and the docs
linked below. It summarises; the linked docs are the evidence. Where memory
and a doc disagreed, the note says which one this page followed.

Main was `3ee8ab3` when this was written. The a13 gate run was in progress.

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

Current phase: feature freeze after a13. Max's playtest of a13 matters most;
work until then is bugs, first impressions, robustness, and modder docs.

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

| # | Item | State tonight |
|---|---|---|
| 1 | A new player can download, host or join, and play; all 20 first-impressions items closed | Partly. See the table below. |
| 2 | A modder gets from zero to an Add-On with the guide; v20 imports work | Guide on main; the cold walkthrough rewrite lands after a13. v20 brick and weapon imports work. |
| 3 | Reported bugs fixed; the gate has zero known failures | Shadow bug fixed in a13. `tools/gate-known-failures.toml` lists no failures today, only two tests the gate cannot run (a real UPnP router, a benchmark world). Memory still names `app_flow`; this page follows the file. |
| 4 | A 1 h multi-player soak with save and reload | Not done. Planned for the overnight PC QA thread. |
| 5 | A home test, then a small group playtest | Waiting on a13. |

### First-impressions items

From [audits/first-impressions.md](audits/first-impressions.md). "Main" means
the commit is on main now; "after a13" means it is in the overnight batch.
Items marked "not checked" were not verified for this page.

| # | Item | State |
|---|---|---|
| 1 | Double-click opens nothing; startup errors invisible | Fixed on main (`87321c0`). |
| 2 | Crashes are silent | Fixed on main (`87321c0`); PR #15 adds the .dmp name to the dialog after a13. |
| 3 | Unsaved work is lost | Fixed on main: autosave and an unsaved-changes prompt (PR #5). |
| 4 | Damaged settings stop the game | Fixed on main (PR #5). |
| 5 | Joining by IP | Fixed on main: one port, invites, reachability check, firewall fix (PR #13). |
| 6 | Rejoin loses your bricks; no reconnect | Fixed on main (`3ee8ab3`). |
| 7 | One bad save hides every save | Fixed on main (PR #5). |
| 8 | No warning before a bad plant; undo render test | Not checked. |
| 9 | Raw internal disconnect text | Fixed on main (PR #5). |
| 10 | Loading feedback | Real loading progress on main (`9996bf7`); the startup splash is parked. |
| 11 | Tutorial untested end to end | Not checked. |
| 12 | Frame cap, presets, render distance | Presets and frame cap on main (PR #10); render distance not checked. |
| 13 | Text size, colourblind, subtitles | Not checked. |
| 14 | Join passwords and brick limits not applied | Open: passwords are one of the audit's choices for Max. |
| 15 | Name prompt; duplicate names | Not checked. |
| 16 | Dedicated server persistence | `bri-server` autosaves on main (PR #5); resume not checked. |
| 17 | Version, updates, debug symbols | PR #15 after a13 (version, update check); a13 ships the .pdb. |
| 18 | Brick search, duplicator | Not checked. |
| 19 | Input options | Not checked. |
| 20 | Music slider, live preview | Music volume and live volume preview on main (PR #10). |

## What's on main tonight

- `47dcf2a`: the batch of PRs #14, #8, #12, #7, #5, #9, #10, #13 (protocol
  34), #11 (the client sandbox), the v20 slides port, and Add-Ons reached from
  Start Game only, like v20.
- `c80782c`: slide acceptance tests run on a spread sample.
- `3ee8ab3`: rejoin keeps your owner number, and you can rejoin after a drop.

In the gate run for a13 (squashed onto `3ee8ab3` as `7b4c6f5`):

- PR #1, the stress campaign (protocol 35). Games download and load a
  server's Add-Ons on join. The campaign's own state is in
  [stress-lab/HANDOFF.md](stress-lab/HANDOFF.md).
- The shadow fix `6568310`: the occluder map keeps the first surface below
  the caster, and mounts cast. By design: bricks cast only with Brick Shadows
  on (v20 had none), map geometry never casts, point lights don't cast.
- Sandbox fixes `696d3d3`: base game packages no longer load as Add-On
  client code (they spammed 16 "client.missing" lines per game); a manifest
  may carry a `client` section; one broken Add-On no longer switches off the
  rest; `bri-client` builds on Linux again.
- Add-On weapon ids `37ba325`: imported weapons no longer disconnect every
  player (audit finding 1).

a13 is a release build with `bri-import-addon.exe` and the `.pdb`, smoke
tested, in `BlocklandReImagined\dist` on Max's PC.

## Landing overnight, after a13

The gate thread lands these as one batch, so the gate runs once:

- **Gate speed-up.** Client and render integration tests run in their own
  pool; the full suite takes about 92 s. `bri_crash::finish()` now waits for
  the stderr reader, so startup-failure text isn't lost.
- **Lean build profile** `060ae1d` (branch `claude/project-thread-4nxjsd`):
  a leaner dev profile, `CARGO_INCREMENTAL=0`, target trimming and the
  worktree pool rule. One build went from 42 GB to 8.9 GB.
- **PR #15, ready for friends** (branch `claude/friends-ready-cvjyvf`):
  version in the menu corner and `--version`; a GitHub Releases check once
  per start, with a toggle in Options > Advanced; the crash dialog names the
  .dmp; a SmartScreen note in the README; signing behind a parameter; a
  first-run quality preset from the GPU; a frame-time line each minute.
  After it lands, package builds must set `$env:BRI_VERSION` to match
  `-Version`.
- **Guide walkthrough** `97099bb` (branch `claude/guide-walkthrough-s8sxrn`):
  the Add-On guide ([modding/README.md](modding/README.md)) rewritten from a
  cold walkthrough (make, check, try, play); `bri-addon-check` validates
  weapons and shares the Add-Ons screen's side rule; a new `bri-addon-run`
  runs an Add-On with a host and a guest headless; four new samples.
- **Audit fixes** (branch `claude/orthogonality-audit-67nx3z`, head
  `c4a7162` at writing): effects, grass and rain get the world's colour
  correction; every save goes through one snapshot, with an autosave before
  Change Map; planting, painting and the wand share one build gate; Add-On
  commands can be typed in chat; Ray Casting decides what projectiles hit;
  tools check a brick may be destroyed before breaking it.
- **Red-team fixes** (branch `claude/red-team-o8nvo2`): idle or spoofed
  connections can no longer fill a host's slots; Add-On recursion can't
  crash the game; a hostile host can't hang a join; and more in the doc.
- **Revived v20-feel branches**, rebased one at a time on the PC:
  `revive/v20-parity` (eye height, camera tilt, turn speed),
  `revive/chat-emotes`, `revive/water-player`, `revive/admin-orb` (has
  conflicts).

Memory lists the red-team and friends-ready branches without their suffixes;
this page uses the names on origin.

The morning build, a14, is meant to carry all of it.

## Audits

Read these rather than a summary here.

- [audits/orthogonality.md](https://github.com/MaxHastings/BlocklandReImagined/blob/claude/orthogonality-audit-67nx3z/docs/audits/orthogonality.md):
  40 ranked findings and about 50 smaller ones, grouped under eleven general
  rules. On its branch until the batch lands.
- [audits/red-team.md](https://github.com/MaxHastings/BlocklandReImagined/blob/claude/red-team-o8nvo2/docs/audits/red-team.md):
  hostile Add-Ons against the sandbox, hostile hosts and clients against
  hosting. On its branch until the batch lands.
- [audits/first-impressions.md](audits/first-impressions.md),
  [audits/platform-door-closers.md](audits/platform-door-closers.md),
  [audits/engine-foundations.md](audits/engine-foundations.md),
  [audits/bug-sweep.md](audits/bug-sweep.md).

## Parked, and why

- **Startup splash.** Content loads before the window opens; a splash was
  left for later in [audits/engine-foundations.md](audits/engine-foundations.md).
- **Stress campaign next steps.** Draw block faces in the renderer, a final
  saturation round, predict driven entities, animate box models (see
  [stress-lab/HANDOFF.md](stress-lab/HANDOFF.md)). Held by the feature
  freeze.
- **`revive/visual-compare`.** It drives v20 itself, so only with Max's OK.
- **`revive/docs-drift`, `revive/creature-notes`.** Not revived tonight.
- **An easy script tier (for example Rhai) for modders.** Suggested to Max
  as an after-playtest idea: the foundation is ahead, ergonomics behind. Not
  started.
- **Terrain.** Opus's data, conversion and collision pieces are kept; the
  playtest ships the finite map-bundle-014 path. Integration needs its own
  verification.

## Open decisions for Max

From the consistency audit's "Choices for Max". Memory says five; the doc
lists six, so all six are here:

1. Join passwords: wire them up, or hide the three fields.
2. Player cap: 32 or 64, and whether bots take a slot.
3. Slash commands: which v20 commands to add (`/magicwand`, `/spy`, `/ret`).
4. "Fast 1st/3rd person switch": add v20's smooth camera transition, or hide
   the checkbox.
5. Harmful event outputs: whether SetVelocity, AddVelocity and Dismount from
   someone else's brick need a minigame, as damage does.
6. Particle and rain fog: v20's behaviour not yet checked.

Also waiting on Max:

- **Add-Ons on Change Map:** reinstall them and carry their state over, or
  end them with the old map.
- **Server identity changed** (red-team, not fixed #2): today a failed pin
  is forgotten silently. Proposed: an SSH-style screen, "This server's
  identity changed. Only continue if its host told you they reinstalled",
  with Continue and Cancel. Max's call on the wording.
- **Download size prompt** (red-team #3): a server can make each join
  download up to 4 GiB with no question. Proposed: ask above about 200 MB
  ("This server's Add-Ons need 1.3 GB. Download and join?").
- **Per-player request limit** (red-team #1): one player can stall
  everyone's commands for 10 s at a time. The fix needs the size of the
  largest legitimate request (converted stock builds).

## Things only Max can do

- Try a build on a weaker PC.
- Publish GitHub Releases, so PR #15's update check has something to find.
- Buy a code-signing certificate, if wanted. Optional; signing is already
  behind a parameter.
- Play a13, then a small group playtest.

## How to work here

- Read AGENTS.md first. It has setup, the push gate and the testing
  boundary.
- **Push to main only through the gate:** `python tools/gate.py --push`.
  Never `--no-verify`. Never force push main. Branch protection is on, and
  PRs need the Windows GitHub Actions check.
- **Batch landings.** Gate runs are serialised on one lock; a run takes
  minutes, and more for a big batch. The gate rebases, so squash big
  branches before handing them in.
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
