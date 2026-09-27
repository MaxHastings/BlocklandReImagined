# Core playtest UI path check

Scope: the first-playtest contract in `docs/playtest-contract.md`, not the full
vanilla alpha. This is a source and headless-test audit; it does not establish
interactive feel or replace Maxwell's playtest.

## Normal path

| Flow | Current path/evidence | Result |
| --- | --- | --- |
| Main menu → map list/preview → host | `crates/ui/src/screens/menus.rs` maps the stock start-mission commands to the native map preview and `HostGame`; the selected map is checked against the loaded catalog before dispatch. `crates/client/tests/app_flow.rs` exercises host/session setup in its ignored native flow. | No UI blocker found. |
| Join an unpassworded server by address | Manual Join accepts an address and sends `JoinServer`; `crates/ui/tests/runtime_input.rs::direct_join_accepts_text_and_blocks_duplicate_request` verifies it. | Usable when the user has a trusted direct IP. |
| Move/build/select/paint/tools | `crates/client/src/building.rs` maps input to placement, rotation, paint, undo and core equipment; `crates/ui/src/screens/selector.rs` implements the brick and print selectors. Selector regressions cover pending/rejection/retry, print selection and stock symbol shortcuts. | No menu/UI blocker found in the required building path. Hammer and Printer route through the tool adapter; Wrench properties/events route through `crates/client/src/tool_ui.rs`. |
| Pause → save/load → return to play | `crates/ui/src/screens/menus.rs` routes Escape-menu Save/Load to `SaveLoad`; `crates/ui/src/screens/saveload.rs` handles file selection, overwrite confirmation, map selection and pending results. Its tests cover overwrite/rejection and load selection/permissions. The ignored native `app_flow` test saves a planted brick, reloads it, and checks authoritative/rendered state (`crates/client/tests/app_flow.rs`, around lines 867–993). | No UI blocker found; the native end-to-end proof is opt-in/ignored by default. |
| Pause → disconnect confirmation; main menu → quit | Escape menu requests a confirmation before `Disconnect`; `runtime_input::confirmation_is_modal_and_escape_declines_without_underlying_action` checks the flow. Main-menu quit uses the UI `Quit` action. | No UI blocker found. |

## Actionable limits for the first playtest

- **LAN browser discovery is unavailable.** The Join screen disables the web-master query; LAN query returns an explicit unsupported message at `crates/client/src/app.rs:2130–2137`. Start a local host or use a known direct IP for this playtest.
- **Joining passworded servers is unavailable.** A password is collected by Manual Join, but the client currently returns “Password authentication is not connected yet” (`crates/client/src/app.rs:847`); hosting with a join password is also rejected (`app.rs:628–642`). Use an unpassworded host. This is a connection/setup blocker only for password-protected sessions.
- **The fully integrated app flow test is ignored by default.** `crates/client/tests/app_flow.rs` contains host/build/tool/save/load/disconnect checks, but these require local converted content, QUIC and GPU setup and are gated with `#[ignore]`. The UI unit tests establish action routing, not a normal interactive session. Do not call the ignored test an interactive playtest.
- **Wand destruction is not connected.** `crates/client/src/building.rs:939` reports this directly. Combat is deferred by the playtest contract; do not include Wand destruction in the first-playtest checklist.
- **Special Wrench behavior is partial.** `crates/client/src/tool_ui.rs:246` and `:399` reject source sound/vehicle properties without behavior adapters. Basic wrench properties/events are in scope; sound/vehicle-backed special behavior is not ready.

The inspected normal UI routes contain no additional disabled or unbound control that blocks the contract's core menu → map → building/tools/print/wrench → save/load → exit path. The UI regression checkpoint most recently verified here was `cargo test -p bri-ui` (65 library tests passed, 4 ignored; 6 admin tests passed, 1 ignored; 3 minigame tests, 11 runtime-input tests and 3 view-input tests passed) and `cargo clippy -p bri-ui --all-targets -- -D warnings`. These are UI checks, not proof of the App/GPU/QUIC flow.
