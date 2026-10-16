# v0.3.2 bug-fix integration started

Root-owned v031-release-regressions checkout now uses
`codex/v032-bugfix-integration`, starting from reviewed release-direction docs
`9579eda4` (runtime base published `5956e3a3`). Merge head before this note:
`8dad87093442965f497206f14f9bf76eefc3fa80`.

Merged immutable completed slices without conflicts:

- Map validator and actual SP confirmation controls: `4d197331` includes `761b695f`.
- Pixel wheel density correction: `1283d9ae`.
- Q selection memory: `a6933e2c`.
- Supported-rest drop apex correction: `d1b0d492`.
- Completed fitting/socket/retained-input/phase/hold/weapon patch and host
  composition branch through `6d6e3aa1`. Partial package bridge is excluded.

Root inspected the merged runtime persistence path: it has zero diff from the
published allocator correction; the equivalent allocator input did not undo it.
Tracked tree is clean and diff whitespace check passes. This is branch assembly,
not a combined test pass, main push or release candidate acceptance. Full combined
diff review, final fixes, Windows checks, integrated gate and packages remain.
Do not build or gate this provisional head while incoming fixes are incomplete.

Root's separate loading-typing branch is `codex/v032-loading-typing` from 5956 in
v031-linux-wheel-fixes. New actual UI draw test failed before the fix and passes
after sharing typing-name drawing between play and accepted loading. In-progress
transport uses accepted lobby identity rather than fabricated local echo:
LobbyRequest::Talking, authoritative LobbyTalking snapshots, Arrival event/client
App routing and a protocol change marker. New real network controls and a held-
loading App/UI-to-authority test are compiling, not yet passed. No immutable
loading fix or final tiers claimed. Preserve sessions50714 and17046; the initial
App test command's bare-name `--exact` filter must be replaced by the full
`app::tests::accepted_loading_chat_routes_real_ui_typing_to_authority_and_back`
name before counting any result (otherwise zero tests).

GSF sign omission needs a new map-item admission mechanism and remains an
explicit community-map limitation outside this completed-seams-only release.
Missing sand7 source texture and fog facts do not qualify an invented tan texture
or shader change. Import-report honesty correction is separately granted to the
core owner; it does not restore the sign or original colours.
