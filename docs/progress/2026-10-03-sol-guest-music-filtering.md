# 2026-10-03 Guest Wrench music follows the host

The host already restricts `ToolCatalog.sounds` from Start Game's Music Files and sends `Notice::MusicTracks` on the existing protocol. The guest handler previously changed only `ToolUi`'s private menu and skipped the UI update, leaving the actual Wrench with all locally installed tracks. Its previous unit test inspected that private menu and missed the integration defect.

The client now applies the changed datablock menu through the existing session-scoped notice path. `ToolUi` retains the current host's allowed IDs separately from installed content, reapplies them when special catalogs refresh, rejects a disallowed typed Wrench action, and resets the connection-specific offer on disconnect. The full installed catalog stays available for the next local host. An identical offer avoids rebuilding the UI. An offered ID absent from local content does not fabricate a playable track.

An open music Wrench retains its selected unavailable ID and ordinary draft when the host list changes. Send gives a short recovery message until the creator chooses an offered track or NONE; it never silently clears or substitutes the selection. The existing Music datablock menu also feeds music event parameter choices. An open `setMusic` event similarly retains the old selected ID and row timing, reports the unavailable selection locally, and waits for an explicit offered track or NONE before Send.

Root owns authoritative notice broadcasts for successful live catalog replacement, resume and map adoption. There is no new wire representation or separate UI policy. Music Files remains a next-host setup screen; this patch does not invent an in-game Music Files editor.

Focused regressions cover the actual adapter update applied to an in-memory UI/OpenWrench, unknown offered IDs, an empty offer, installed catalog refresh, a different subsequent host, unchanged offers, retained inspection plus typed-action validation, and a live open Wrench's selected unavailable music/name draft and explicit recovery.

Validation:

- `cargo test -p bri-ui --lib music` — 3 passed, including the property/event live-selection regressions; `/tmp/bri-v022-sol-guest-music-ui.log`.
- `cargo test -p bri-client --lib music` — the adapter-to-actual-UI test passed; the second new fixture used invalid default item-facing selector 0. It now reuses ordinary inspected WrenchData, retaining the music positive/negative checks. Rerun: 2 passed; `/tmp/bri-v022-sol-guest-music-client-retry.log`.
- `cargo test -p bri-sim --test tools a_joining_player_learns_the_music_the_host_offers -- --include-ignored` — 2 passed (synthetic and generated content), checking exact allowed sets on join, resume, changed/empty catalogs and map adoption; `/tmp/bri-v022-sol-music-host-lifecycle.log`. Root implemented the lifecycle broadcasts through the existing notice.
- `cargo clippy -p bri-ui -p bri-client --all-targets -- -D warnings` initially stopped at the new dependency `sim/session/events.rs:47` ledger type-complexity warning. Root factored key/value aliases without changing semantics. Cached rerun passed (26.68 seconds); `/tmp/bri-v022-sol-music-jpeg-clippy-final.log`. Initial failure log: `/tmp/bri-v022-sol-music-jpeg-clippy.log`.
- Touched files formatted; `git diff --check` passes. No interactive game or original-content mutation.
