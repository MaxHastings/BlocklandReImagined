# Blockland ReImagined

Blockland v20 rebuilt from scratch in Rust. It keeps v20's maps, bricks,
tools, weapons, vehicles, mini-games, events, art, music and sounds, and
adds modern safety nets, easy direct-IP hosting and Add-Ons that can go
much further than v20's. Windows 10 and 11 only.

## What's in it

- **v20 as you remember it.** All 14 maps, the brick selector, paint and
  prints, the wrench and events, all 21 stock items, the stock vehicles,
  mini-games, bots, the avatar editor, admin tools and v20's own screens,
  keys and numbers.
- **Built for huge builds.** Bricks are drawn in batched chunks, joining
  a big build is quick, and saves are compact binary files. The aim is a
  million bricks at 60 fps.
- **Modern quality of life.** An unsaved-work prompt, crash
  reports, brick search, a red ghost before a plant that would fail,
  graphics presets, frame cap, field of view, UI size, colour-vision
  modes, sound captions, a gamepad while playing, and old v20 `.bls`
  saves that load as they are.
- **Add-Ons.** The Stunt Plane and the Duplicator come with the game.
  Add-Ons add rules, HUDs, weapons, vehicles, game modes, creatures and
  bodies through generic engine hooks, with no game-specific code in the
  engine. Players joining a server download the Add-Ons it runs, and
  Add-On code on players' PCs runs in a sandbox they choose to trust.
  Old v20 Add-Ons import their bricks, weapons, vehicles and sounds.
- **Direct hosting.** No accounts, relays or server list (see Hosting
  below).

The full list, with what is still missing, is in
[FEATURES.md](docs/FEATURES.md).

## Play

Download the zip for your system from the Releases page
([Windows](https://github.com/MaxHastings/BlocklandReImagined/releases/latest/download/BlocklandReImagined-windows.zip),
[Mac](https://github.com/MaxHastings/BlocklandReImagined/releases/latest/download/BlocklandReImagined-macos.zip),
[Linux](https://github.com/MaxHastings/BlocklandReImagined/releases/latest/download/BlocklandReImagined-linux.zip)),
extract it somewhere you can write to (not Program Files) and run `Launch.cmd`
(the app on Mac, `launch.sh` on Linux). Nothing else needs installing, and you
don't need the original Blockland. Settings and saves live in your user folder
(`%LOCALAPPDATA%\BlocklandReImagined` on Windows), so a newer release's folder
picks them up.

- [Tester guide](docs/TESTER-GUIDE.md): install, playing together, what to
  send when something breaks.
- [Features](docs/FEATURES.md): what is done, partly done and missing.
- [Playtest notes](docs/PLAYTEST.md): things to try, default keys, what
  to do if it runs slowly.
- [Known issues](docs/KNOWN-ISSUES.md).

All four ship in every release folder.

**Hosting** is a direct connection: pick LAN or Internet in Start Game.
Friends join by IP address or with the invite the game copies for you.
Over the internet only **UDP port 28000** needs forwarding, and the game
asks your router to open it for you. UDP 28050 only finds LAN games.

## Make Add-Ons

Start with [Making Add-Ons](docs/modding/README.md). It walks through
making an Add-On from a sample, checking and trying it without the game,
importing old v20 Add-Ons, porting what their scripts did, and what players
are asked to trust. You need a checkout of this repository and Rust, but
not the v20 game or its content. The Commando sample shows a small total
conversion built only from those hooks, and
[total-conversion.md](docs/audits/total-conversion.md) lists the seams
still to come, such as custom movement, a side camera, animated models and
Add-On magazines and recoil.

## Develop

You need Windows, git, Python 3.9+ and a Blockland v20 install (the folder
with `base/`, `Add-Ons/` and `saves/`). The v20 install is only read; its
content is converted locally and never committed.

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

- [Docs index](docs/README.md): every doc, grouped by what you're after.
- [Status](docs/STATUS.md): decisions already made and what is open.
- [Platform principles](docs/architecture/platform-principles.md): read
  before changing content identity, saves, the wire protocol or Add-Ons.
- [Progress log](docs/progress.md): decisions, evidence and commands, by
  date; entries from 2026-10-01 are one file each in
  [docs/progress/](docs/progress/README.md).

Useful commands:

```powershell
cargo test --workspace --locked
cargo run -p bri-client --release -- --check content user-state
cargo run -p bri-client --release -- --run content user-state
```

A dedicated server is `cargo run -p bri-net --release --bin bri-server`;
run it without arguments for its usage.

Release folders are built with `tools/package_playtest.ps1`; see
[package layout](docs/playtest-package-layout.md).

Generated content, diagnostics and packages live under the ignored
`content/`, `artifacts/` and `dist/` folders. Research copies of original
files live under the ignored `.research/`.
