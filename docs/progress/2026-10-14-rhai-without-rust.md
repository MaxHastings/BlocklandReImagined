# 2026-10-14 Rhai scripting without Rust

Community modders kept asking for a scripting language other than Rust.
Rules were already Rhai, not Rust; what needed Rust was the tooling
around them. Maxwell decided to stay with Rhai and asked to make it easy
for modders.

## What changed

- **The modder tools ship with the game.** `bri-addon-check` (check and
  `--package`) and `bri-addon-run` (run rules headless) are built by the
  Windows, Linux and Mac release workflows and packaged beside the client
  (inside `Contents/MacOS/` on a Mac, so the app's signature covers them).
  `docs/modding/README.md` now runs them from the game folder; `cargo run`
  stays as the way from a checkout. Cost to the download, measured on
  Linux: stripped, `bri-addon-check` is 10 MB (3.6 MB gzipped) and
  `bri-addon-run` 38 MB (13 MB gzipped), since each links the rules engine
  and `bri-addon-run` the whole simulation.
- **`print` works.** A script's `print` lines were collected into a host
  buffer nobody read. They now go to the host's console and log
  (`bri_console::echo`, after the Add-On's id), so they show in a hosted
  game's `~` console and in `bri-addon-run`.
- **Every script function is in the guide.** 15 functions had no line in
  `docs/modding`: `game_version`, `remove_body`, `scroll_mode`,
  `shift_copy`, `rotate_copy`, `take_paint`, `bot_intent`,
  `setting_text`, `team_setting_text`, `setting_info`, `data_line_count`,
  `data_line`, `minigame_snapshot`, `restore_minigame`, `revive_bricks`,
  documented in `rules.md` from their code. A new test,
  `crates/package-runtime/tests/script_docs.rs`, fails when a function
  registered with `register_fn("name", ...)` appears nowhere in
  `docs/modding`.
- **`docs/modding/rhai.md`, "Rhai in ten minutes":** the language as rules
  use it (values, lists and maps, loops, text), what common errors mean,
  and a TorqueScript-to-Rhai table. Every example ran through
  `bri-addon-run` in a scratch Add-On and printed what the page says.

## Checks

- `bri-addon-check` and `bri-addon-run` release builds; the primer's
  scratch Add-On checked and ran with no script problems.
- `cargo fmt --check` on the changed crates.
- `python tools/check.py focused --base origin/v0.3.1-experimental`
  (see the PR for its result; GPU tests cannot run in the container).

## Next

- Release packaging itself runs only on the release workflows; the next
  tagged release confirms the tools land in all three downloads.
- A guest crate or AssemblyScript bindings for sandboxed client code,
  which is still hand-written WebAssembly text.
