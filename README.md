# Blockland ReImagined

Blockland v20 rebuilt in Rust: build with bricks, make mini-games, and play
with friends using the original maps, art, music and sounds. Add-Ons extend
the game with tools, weapons, vehicles, rules and new worlds.

**This is an alpha.** Familiar v20 gameplay is the reference; fidelity,
performance and compatibility still have limits. See [features](docs/FEATURES.md)
and [known issues](docs/KNOWN-ISSUES.md) before planning a larger project.

## Play

Download from the [latest release](https://github.com/MaxHastings/BlocklandReImagined/releases/latest),
extract the whole ZIP, then launch:

| System | Download | Start |
|---|---|---|
| Windows 10/11 x86-64 | [Windows ZIP](https://github.com/MaxHastings/BlocklandReImagined/releases/latest/download/BlocklandReImagined-windows.zip) | `Launch.cmd` |
| macOS Apple silicon (M1 or newer) | [Mac ZIP](https://github.com/MaxHastings/BlocklandReImagined/releases/latest/download/BlocklandReImagined-macos.zip) | `BlocklandReImagined.app` |
| Linux x86-64, glibc 2.35+ and a Vulkan driver | [Linux ZIP](https://github.com/MaxHastings/BlocklandReImagined/releases/latest/download/BlocklandReImagined-linux.zip) | `./launch.sh` |

You need no original Blockland install, account, compiler or Python to play.
The [tester guide](docs/TESTER-GUIDE.md) covers first launch, security prompts,
updates, old saves and file locations. Start with the Tutorial, or host a
Custom game on Slate. [Playtest notes](docs/PLAYTEST.md) cover controls and
things to try.

**Playing together:** use the same release. Pick LAN or Internet in Start
Game; friends join by IP or paste the `bri://` invite into Connect to IP.
Internet hosts need **UDP 28000** reachable; the game asks the router to open
it automatically. UDP 28050 is only for LAN discovery. There is no public
server list or relay. See [playing together](docs/TESTER-GUIDE.md#playing-together)
or [dedicated servers](docs/DEDICATED-SERVER.md).

Settings and saves live outside the release folder. Extract updates into a
fresh folder, back up your state, and carry over custom Add-Ons as described
in [updating and backups](docs/TESTER-GUIDE.md#updating-and-backups).
Alpha save and Add-On formats may change between releases.

## Video

[![Blockland ReImagined - Showcase](https://img.youtube.com/vi/QeILUBHx8Vw/maxresdefault.jpg)](https://www.youtube.com/watch?v=QeILUBHx8Vw)
[![Blockland ReImagined - Multiplayer](https://img.youtube.com/vi/W9tYEOUL650/maxresdefault.jpg)](https://www.youtube.com/watch?v=W9tYEOUL650)

## Community

Join the [Discord](https://discord.com/invite/J8e4kvndyu).
For bugs, include your version, reproduction steps and logs from
[the reporting guide](docs/TESTER-GUIDE.md#when-something-breaks).

## What's in it

- **Familiar v20 content.** The 14 reference maps, the brick selector, paint and
  prints, the wrench and events, all 21 stock items, the stock vehicles,
  mini-games, bots, the avatar editor and admin tools. Fidelity gaps are tracked
  in the feature guide and known issues.
- **Built for huge builds.** Bricks are drawn in batched chunks, joining
  a big build is quick, and saves are compact binary files. The aim is a
  million bricks at 60 fps.
- **Modern quality of life.** An unsaved-work prompt, crash
  reports, brick search, a red ghost before a plant that would fail,
  graphics presets, frame cap, field of view, UI size, colour-vision
  modes, sound captions, a gamepad while playing, and import for old v20 `.bls`
  saves, with conversion gaps reported.
- **Add-Ons.** The Stunt Plane and the Duplicator come with the game.
  Add-Ons add rules, HUDs, weapons, vehicles, game modes, creatures and
  bodies through generic engine hooks, with no game-specific code in the
  engine. Players joining a server download the Add-Ons it runs, and
  Add-On code on players' PCs runs in a sandbox they choose to trust.
  Old v20 Add-Ons import their bricks, weapons, vehicles and sounds.
- **Direct hosting.** No accounts, relays or server list; see Play above.

The full list, with what is still missing, is in
[FEATURES.md](docs/FEATURES.md).

## Build games

Use the Wrench's Events screen for brick actions and checks, and MiniGames
for teams, equipment and game settings. The experimental
[Rule Workshop guide](docs/rule-workshop/PLAYTEST.md) starts with editable
examples, including team doors, checkpoints, puzzles and ball goals. Its
[creator test card](docs/rule-workshop/CREATOR-TEST-CARD.md) gives focused
things to try. Both ship in the release folder; no code or checkout is needed.

## Make Add-Ons

Start with [Making Add-Ons](docs/modding/README.md). It walks through
making an Add-On from a sample, checking and trying it without the game,
importing old v20 Add-Ons, porting what their scripts did, and what players
are asked to trust. The release includes `bri-import-addon`, `bri-addon-check`
and `bri-addon-run`; editing and checking native Add-Ons needs no Rust.
Get samples from this repository (GitHub's **Code → Download ZIP** works).
Building the engine or the tools from source needs Rust; importing some old
maps also needs a v20 install for borrowed resources. The Commando sample shows a small total
conversion built only from those hooks.
[ENGINE-CAPABILITIES.md](docs/modding/ENGINE-CAPABILITIES.md) lists what the
engine offers by what you want to build, and
[seams.md](docs/architecture/seams.md) holds the status of every seam. The
older [total-conversion.md](docs/audits/total-conversion.md) is a dated audit
(2026-09-29), not a current list of gaps.

## Develop

You need git, Python 3.11+ and a Blockland v20 install (the folder with
`base/`, `Add-Ons/` and `saves/`). Development works on Windows, macOS and
Linux; bootstrap checks the platform's toolchain. The v20 install is only
read; its content is converted locally and never committed. Bootstrap alone
supports Python 3.9+, but the push gate requires 3.11+ for `tomllib`. On macOS
or Linux, use `python3` if that is your Python 3.11+ command.

```sh
git clone https://github.com/MaxHastings/BlocklandReImagined.git
cd BlocklandReImagined
python tools/bootstrap.py --v20 "/path/to/Blockland v20"
python tools/gate.py --install-hook
```

`bootstrap.py` checks the toolchain and prints the exact install command
for anything missing (Rust 1.93 or later, Pillow and so on), recovers the
v20 scripts with a pinned tool, generates every content pack into
`content/`, builds the client and validates it with `bri-client --check`.
When it finishes it prints the command that starts the game. Run the same
command after pulling; it rebuilds only what changed. See
[content regeneration](docs/content-regeneration.md).

Read [AGENTS.md](AGENTS.md) before working here. It covers disk use, the
push gate (`python tools/gate.py --push`), the testing boundary and the
engineering rules. Then:

- [Docs index](docs/README.md): starting points grouped by what you're after.
- [Status](docs/STATUS.md): decisions already made and what is open.
- [Platform principles](docs/architecture/platform-principles.md): read
  before changing content identity, saves, the wire protocol or Add-Ons.
- [Progress log](docs/progress.md): decisions, evidence and commands, by
  date; entries from 2026-10-01 are one file each in
  [docs/progress/](docs/progress/README.md).

Use the [contributor guide](CONTRIBUTING.md) for scoped checks and changes.
After bootstrap, these commands validate content or start a development game
using the normal user profile:

```powershell
cargo run -p bri-client --release --locked -- --check content
cargo run -p bri-client --release --locked -- --run content
```

For isolated settings and saves, append a state directory after `content`.
A dedicated server is documented in [DEDICATED-SERVER.md](docs/DEDICATED-SERVER.md).

Platform packagers and verification are in [package layout](docs/playtest-package-layout.md);
publication is in [release builds](docs/release-builds.md).

Generated content, diagnostics and packages live under the ignored
`content/`, `artifacts/` and `dist/` folders. Research copies of original
files live under the ignored `.research/`.
