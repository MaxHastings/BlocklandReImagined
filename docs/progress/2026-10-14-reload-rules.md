# 2026-10-14 Reload rules without hosting again

Modders iterating on a rule had to host the game again after every edit,
losing the game's state. Maxwell approved a reload command.

## What changed

- **`/reloadrules`** (host only) reads every hosted Add-On's rules script
  from disk again. Changed scripts are compiled together and swapped in;
  if any has a mistake, nothing is swapped, the host is told the problems
  and the old scripts keep running.
- **State carries on.** The rule's saved state, host data, settings,
  creatures, cooldowns and players are untouched; `on_join` is not run
  again. Only the `.rhai` script reloads: `behaviour.json` (commands,
  state keys, settings) and other files still need hosting again, since
  they shape state a running game already holds.
- `Package` keeps the folder it was read from (`Package::dir`,
  `reread_script`, `set_script`), so file reading stays in
  `bri-package-runtime`; `Session::reload_rules` swaps catalog and runtime.
- `bri-addon-run` reaches it as `--send :reloadrules` (the runner is the
  host).
- Documented in `docs/modding/README.md` ("Edit while you play") and
  `docs/modding/rhai.md`.

## Checks

- `crates/sim/tests/rules_reload.rs`: a counter rule counts 1, a broken
  edit is refused and the old rule counts 2, the fixed edit reloaded by
  typing `/reloadrules` adds 10 to reach 12, and a second reload reports
  nothing changed.
- `cargo fmt --check`, clippy `-D warnings` and tests of
  `bri-package-runtime` and `bri-sim`'s new test.

## Caveat

A recorded replay re-run later reads whatever script is on disk then, not
the one swapped in during the recording.
