# Status

Current release and engineering decisions. Reviewed 2026-10-30. Players should
start with the [tester guide](TESTER-GUIDE.md), [features](FEATURES.md) and
[known issues](KNOWN-ISSUES.md). Dated development receipts are in
[progress/](progress/README.md); older history is in [progress.md](progress.md).

## Release state

Published release: [v0.4.2 Alpha](https://github.com/MaxHastings/BlocklandReImagined/releases/tag/v0.4.2),
published October 30, 2026 at 04:22:56 AM America/Chicago, from
`93c1439dacea2c6bce19ca735195bb0f6b944daa`. Windows x86-64,
Apple-silicon macOS and Linux x86-64 ZIPs and SHA256SUMS are public. Every
public download matches its tested artifact, the tag names the tested source,
and the latest-release endpoint returns v0.4.2. Published assets stay immutable.

This release integrates bot/navigation recovery, brick destruction contact,
approved indoor/outdoor map ambience and tool swings, the accepted small
original-texture water glare change, and bounded Add-On/map import compatibility
fixes. Maxwell approved the combined warm playtest and final ambience mix.
PRs #403-#410 are merged. The exact-source full release gate passed all 528
test binaries without retries, the actual 23-save corpus and both performance
budgets; all three native platform package/startup jobs passed. See the
[release notes](releases/v0.4.2.md) and
[publication receipt](progress/2026-10-30-v042-public-release.md) for actual
commands, source/content provenance, retained failures, archive hashes and
cleanup evidence. Full alpha remains incomplete; compatibility warnings still
describe unsupported legacy scripts and map objects.

Use the [latest published release](https://github.com/MaxHastings/BlocklandReImagined/releases/latest)
for downloads, its exact source tag and release notes. Repository source
can be ahead of that build; source checks alone are not a new publication.

Previous [v0.4.1 Alpha](https://github.com/MaxHastings/BlocklandReImagined/releases/tag/v0.4.1)
remains unchanged at `4db7796916d8b023aa512374334973a5574ef040`.
Its [release notes](releases/v0.4.1.md) and
[publication receipt](progress/2026-10-29-v041-public-release.md) retain that
release's feature, validation and cleanup history.

Earlier [v0.3.6](progress/2026-10-23-v036-public-release.md) and
[v0.3.5](progress/2026-10-23-v035-public-release.md) receipts remain historical.
Private AA/Halo/GMod playtests retain their separate acceptance boundaries.

An independent [actual v0.3.4 package audit](progress/2026-10-23-v034-release-package-resource-audit.md)
confirmed doubled animation-resource paths and omitted menu background photos.
The [rendering portability audit](audits/rendering-portability-2026-10-23.md)
tracks corrections and final-pixel checks. One RX550 Windows reporter sees a
black gameplay world while the interface works; the cause is unconfirmed.
It remains documented without blocking the next release indefinitely.

Recent main also contains generic imported-map submission, player headroom,
cosmetic casing contact, narrow-passage yielding, map emitters, breakable props,
volumes and package lifetime improvements used by private AA2 consumers.
The [imported-map performance evidence](progress/2026-10-23-static-world-visibility.md)
records the compared rendering paths. Its bounded frame measurements do not
establish 100 FPS gameplay or completed mission fidelity.

The v0.3.6 release adds pursuit beyond idle
homes, intrinsic-hole reset fixes, native builder spawner/lifecycle controls,
and a shuffled main-menu slideshow including saved screenshots.
See the [release notes](releases/v0.3.6.md) and
[supplied-source evidence](progress/2026-10-23-supplied-hole-bots.md), plus
[menu pixel comparisons](progress/2026-10-23-menu-background-slideshow.md).

The Rule Workshop, bot planning and alpha formats remain experimental.
Bots compose supported declared mechanics, not arbitrary scripts. Shark/Zombie
ports remain partial. Private AA/Halo/GMod source assets stay excluded from
public releases. Separate private playtests retain their own mission plans,
source evidence and outstanding gameplay/visual acceptance. Pipeline and
Interdiction are not complete merely because shared mechanisms land on main.

Maxwell performs all interactive playtests. Headless checks, offscreen pictures
and independently verified packages are distinct evidence. A separately
authorized release can publish after its own requirements pass while the full
[alpha contract](alpha-contract.md) and AA roadmap remain open.

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
- **Release platforms:** Maxwell authorized Windows x86-64, macOS Apple silicon
  and Linux x86-64 releases on 2026-10-02. All interactive acceptance remains
  Maxwell's; the full alpha roadmap is still open. Current publication is
  recorded above rather than duplicated here.
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
  no genre code in the engine. What exists is in
  [modding/ENGINE-CAPABILITIES.md](modding/ENGINE-CAPABILITIES.md); each
  seam's status is in [architecture/seams.md](architecture/seams.md).
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
| 3 | Reported bugs fixed; the gate has zero known failures | Ongoing across the alpha roadmap. The published v0.3.2 candidate passed its gate; this does not close unconfirmed player reports. See `tools/gate-known-failures.toml` and dated integration evidence for subsequent candidates. |
| 4 | A 1 h multi-player soak with save and reload | **Done** on a13 code: four players for 3608 s on Slate over LAN, no disconnects, every reload exact ([audits/night-qa.md](audits/night-qa.md)). |
| 5 | A home test, then a small group playtest | Max has played test builds in multiplayer. The group session is for later (see "For testers later"). |

### First-impressions items

This table preserves the original release checklist and its acceptance evidence.
It is not a fresh test run of every item on today's main. Use Known issues
below and the linked player guides for current reports and limitations.

From [audits/first-impressions.md](audits/first-impressions.md). Items that
need a real window are for Max to confirm.

| # | Item | State |
|---|---|---|
| 1 | Double-click opens nothing; startup errors invisible | Fixed (`87321c0`). Max to confirm a console window or error shows. |
| 2 | Crashes are silent | Fixed; the dialog names the .dmp (PR #15). |
| 3 | Unsaved work is lost | Fixed: an unsaved-changes prompt. Autosave was removed 2026-09-29 (Max: v20 never auto-saved; it filled disks with saves). |
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
| 16 | Dedicated server persistence | `bri-server` saves on shutdown, keeps one rolling crash-recovery checkpoint, and supports `resume`. Local play uses manual Save Bricks; it has no autosave. |
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
- [audits/total-conversion.md](audits/total-conversion.md): the 2026-09-29
  audit of the seams a total conversion uses (historical; see
  [modding/ENGINE-CAPABILITIES.md](modding/ENGINE-CAPABILITIES.md)).
- [audits/first-impressions.md](audits/first-impressions.md),
  [audits/platform-door-closers.md](audits/platform-door-closers.md),
  [audits/engine-foundations.md](audits/engine-foundations.md),
  [audits/bug-sweep.md](audits/bug-sweep.md).

## Parked, and why

- **Startup splash.** A dedicated splash remains deferred. The earlier
  [engine-foundations audit](audits/engine-foundations.md) predates background
  content preparation and should not be read as a current startup trace.
- **Stress campaign next steps.** Block faces in the renderer, a saturation
  round, predicted driven entities, animated box models (see
  [stress-lab/HANDOFF.md](stress-lab/HANDOFF.md)). Not started.
- **Visual comparison with v20.** Cancelled by Maxwell; retained evidence is
  historical, not a pending agent task.
- **Terrain.** Opus's data, conversion and collision pieces are kept; the
  playtest ships the finite map-bundle path.

## Things only Max can do

- Say whether a console window or error shows when you double-click the
  game.
- Try the skis, and Demo Pong in Bedroom: load it, click the ramp, and use
  the + and - buttons.
- Original content access authorizes private CI bundle refreshes. The release
  coordinator performs upload, checks and publication within Max's authorization;
  see [release-builds.md](release-builds.md).
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
- Record decisions, evidence and commands as a new file in
  [progress/](progress/README.md), one file per entry.
- Cloud containers have about 30 GB of disk: build only the crates you need,
  with `CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`.
