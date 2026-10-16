# Q tool selection memory

Root-owned `codex/v032-q-tool-memory`, based on released `5956e3a3`, reuses the
idle v031-linux-wheel-fixes checkout after committing wheel branch `1283d9ae`.
That branch and all existing logs/previews are preserved.

Reproduced pharzedia's Q-menu report through actual UI key binding: authoritative
tool slot 1, Q close emitting UnUseTool, authoritative SetActiveTool(None), then
Q reopen. Baseline requests slot 0 instead of 1 (runtime assertion failure,
`target-q-memory-red.log`). Initial diagnostic used a nonexistent Key::Q variant;
corrected to existing Key::Letter('q') before this executed behavioral red.

The authoritative deselection cleared cur_tool, which was also the UI's sole
reopen preference. Keep a separate private last valid slot/authored-ID preference.
Authoritative None still clears held state, label and cur_tool without emitting
a command. Only a new Q input requests the remembered tool, subject to normal
server authority. Inventory removal, empty inventory and changed authored IDs
discard the preference; session reset recreates the HUD. Recovered original
v20 useTools retains CurrScrollToolSlot when leaving tool mode, consistent with
remembering a UI choice separately from held state.

Targeted UI key/echo/reopen regression now passes, including disconnect/rejoin
with identical inventory returning to initial slot 0. New unit controls cover
removed/replaced/empty/refilled inventory. No App/network playtest or physical
reporter confirmation is inferred from these UI controls.

Client/server authority, schemas, gameplay simulation and published v0.3.1 assets
are unchanged. Production is limited to ui/models/hud.rs; retained tests are in
that module and ui/tests/runtime_input.rs. No interactive gameplay occurred.

Validation: targeted tests green; quick format and UI unit tests pass 227/0/11
in 5 seconds. Full standard focused UI/dependents exits 0 as session41092 in
131 seconds: format, UI all-target Clippy with warnings denied, client unit,
all UI tests and UI-import tests pass. Client units are 584/0/76; the retained
runtime-input integration includes the ordinary Q/echo/reopen/lifecycle control.
Logs
target-q-memory-{red,green,inventory,quick,focused}.log remain outside the commit.
Required Windows and integrated candidate validation remain pending.
