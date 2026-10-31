# Loading input and the reported friend-server exit

Maxwell reported that chat Page Up/Page Down and F2 do not work during add-on
downloads/map loading in preview v0.4.3. Added this to the loading experience:
accepted loading connections must retain chat history controls and a real live
player list, without enabling movement, building or fire. Both loading UI roots
must draw the chat HUD. Existing code permits only `globalChat` over Loading,
not paging/F2, and omits the PackageDownload root. The actual arrival protocol
also has no player roster, so merely opening an empty F2 dialog is insufficient.
A new device-free UI regression reproduces the missing paging behavior (red).

Maxwell then reported an application exit joining his friend's add-on server
using `E:/Downloads/BlocklandReImagined-v0.4.2-windows`.
Preserved the stable 57,900-byte `session-20261031-045911.log` privately in
`diagnostics/2026-10-31-friend-server-load/` under the planning evidence root,
SHA256 `84a78426132f19b138a3e9a78c86eee8cc64fa16d47aa97de4ad9c997af36e62`.
The next session log was empty at inspection and was not treated as evidence
of the preceding failure.

The log identifies `bri-client v0.4.2 (93c1439da)`, reaches Mikes House world/map
loading and ends with `The chat queue is full or the connection ended: channel
closed`. It records no panic/backtrace. Earlier missing dynamic-light data and
unsupported map-object messages are explicit fallbacks; they are not the final
reported cause. Do not blame add-on geometry or delete the user's cache based
on those warnings.

Confirmed that exact v0.4.2 source contains the same fatal path as this branch:
`App::dispatch_action` returns `held.arrival.talking(talking)` directly, bypassing
normal UI action-result handling. `dispatch` and the platform/main loop propagate
that error and exit. A closed loading writer can occur during disconnect or the
normal loading-to-game transport replacement before visual entry completes.
The log proves the final closed-channel error; it does not by itself establish
which transport-close reason occurred first.

Extended the existing real-QUIC loading-chat App regression to close the real
transport, wait for its bounded writer to close and dispatch late typing edges.
Require the application to remain alive. This is a targeted lifecycle correction,
not a blanket catch of map loading failures, periodic reconnect or ignored panic.
Red/green and final integrated handoff evidence are still pending.

The first extension failed before the intended boundary: the synthetic App's
imported global Escape binding canceled loading instead of closing its chat
overlay, so no arrival remained. This uncovered a separate real input-order bug,
not the closed-writer red result. The UI now lets dialogs/dropdowns own global
Escape before the underlying join cancellation. The crash-specific driver uses
Return to submit chat, then closes the server, so its writer-close failure is
independent of Escape. Await the actual new regression result before claiming red.

Maxwell also requested Ctrl+O during loading. The loading utility whitelist now
allows the bound Options command even before host acceptance; conversation keys
still require acceptance. Actual roster delivery uses bounded public
`ConnectedPlayer` rows, with no administrative settings/addresses/control state.
The new protocol change is registered; loading trust editing remains disabled.

Further explicit feedback requests a horizontal speaking-name HUD. The default
speaker region now spans the available width and flows measured plain-text
names/icons horizontally, wrapping within the declared region and deriving
chat clearance from the actual occupied rows. Custom HUD rectangles, speaker
count/overflow bounds, alpha and plain-name semantics remain in force. Updated
the existing bounded/plain-text/HUD-claim regression for the horizontal row;
no new raster artwork or schema is involved.

The corrected App regression now fails against the old typing dispatch with
the exact `chat queue ... channel closed` error and passes with the narrow
best-effort presence correction. The ordinary chat-message path still reports
its own send errors; this does not catch unrelated load failures. The test uses
an actual accepted QUIC connection, real UI typing/chat submission, authoritative
typing/roster updates, a real transport close and late Start/StopTyping actions.
`cargo test --locked --profile playtest -p bri-client --lib
accepted_loading_chat_routes_ui_typing_and_survives_transport_close` passes (1).

UI input integration passes all 39 cases, including both loading roots, paging
back to live chat, F2 toggle, Ctrl+O before acceptance, global-Escape overlay
ownership and no movement/jump leakage. The initial Options expectation wrongly
allowed only device refresh: its authored Escape/Done action legitimately applies
local volumes and saves settings. The test now requires that save and allows
only the specific local utility actions, excluding cancellation/gameplay. No
production assertion was weakened. Loading typing/draw integration passes (1),
and the existing bounded/plain-text/speaker-HUD-claim test passes with horizontal
placement (1). All checks are device-free; no interactive playtest was requested.
Native QUIC live-roster integration and final changed-area Clippy are pending.
The first roster integration run passed three existing cases and failed the new
identity assertion: the fixture incorrectly expected the raw public key. The
host's established authenticated principal is SHA256(public key), confirmed in
`server::verify_identity`. Corrected the expected principal; production identity
derivation and the exact equality assertion remain unchanged. Rerun is pending.

The corrected real-QUIC suite now passes all four cases, including live roster
updates as a second loading player joins/leaves, exact authenticated principal,
chat during actual package download, identity isolation and normal loading-to-game
replacement. Command: `cargo test --locked --profile playtest -p bri-net --test
chat_while_joining`. All-target Clippy passes for client/net/sim/ui with
`--locked --profile playtest -- -D warnings`; the final net test-only edit receives
its own follow-up Clippy check. These are scoped checks, not the release gate.

The private screenshot feedback is preserved as
`diagnostics/voice-speaker-layout-feedback.png`, 12,068 bytes, SHA256
`ec97e346bc5ba694f5225e60cb25f583163fa75e854a1d2a6fac0ff0ba464941`.
No visible game, GPU suite or requested extra playtest was used for these checks.
Broader loading voice/audience handoff, final source/package/CI gates and the
remaining release scope stay open. The separate navigation-clearance increment
still has a retained ledge-descent failure and is not represented as green here.
