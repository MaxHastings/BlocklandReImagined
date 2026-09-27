# Mini-game UI integration checkpoint (work in progress)

Maxwell reprioritized the first handoff to core building and explicitly deferred
mini-games on 2026-09-26. This file records the partial UI boundary only; it is
not a completion claim or acceptance evidence for mini-game gameplay.

`crates/ui/src/api.rs` now has session-scoped game/player selector IDs, stock
rule fields, list/member/invitation DTOs, host capability flags, typed actions,
and updates. Action payloads contain no acting player, role, admin status, BL_ID
ownership claim, or credential. The host must derive caller authority from the
authenticated session. `MiniGameUiState::default()` is not ready and advertises
no capabilities; `Core::minigame_request` refuses the operation unless the
host-supplied state explicitly enables it. Admin status is not interpreted as
mini-game ownership.

The new screens map original `joinMiniGameGui`, `CreateMiniGameGui`,
`MiniGameInviteGui`, and player-list controls from `content/ui-pack-003` through
typed handlers. Rule DTO defaults match recovered Create Mini-Game defaults;
time values are UI seconds and need conversion at the host boundary. Lives are
not a setting: v20 has unlimited lives. Join/create/action requests retain
stable target IDs and request IDs; reset/end and Ignore confirmations re-check
the active game or invitation before queuing the request.

The screens are **not connected to App/transport/minigame authority**. No real
host publishes `UiUpdate::MiniGames` or `MiniGameInvite`, supplies catalogs and
capabilities, or handles these actions in the current first-playtest scope. All
operations therefore stay disabled in a normal session. This checkpoint also
does not implement stock favorites persistence or mini-game list sorting, and it
has no offscreen render evidence using the original UI pack. Do not expose these
controls as working minigame gameplay in the first-playtest handoff.

Synthetic UI tests cover default-off gates, public join target correlation,
stock create defaults, owner-vs-admin capability separation, and invite identity.
`cargo test -p bri-ui` passed with 65 library tests, 6 admin-screen tests, 3
minigame-screen tests and 11 runtime-input tests; 4 existing library probes and
1 admin offscreen probe remain ignored. `cargo clippy -p bri-ui --all-targets --
-D warnings` passed at this checkpoint. Full minigame adapter integration is
deferred with the rest of combat/vehicles/minigames.
