# 2026-10-11 Small UX items from the v0.2.9 brief (P2-01, P2-02, P2-04, P2-05)

The v0.2.9 audit (`release/v0.2.9-ask-audit.md`) found these four low-risk
items from Max's brief were never started. This entry covers them; no wire
or save format changed.

## What changed

- **P2-01 Ctrl+wheel paging.** With Ctrl alone held (Shift, Alt and Cmd
  leave the plain wheel), the wheel over any scrolling list in a dialog
  moves one page per notch: the list's height less one row. The plain wheel
  still moves one row, Page Up/Down are untouched, and the existing
  whole-notch accumulation in `Ui::handle_input` runs first, so precision
  wheels do not page on a fraction. The Brick Selector shows "Ctrl + mouse
  wheel scrolls a page" beside its search box. In play with no dialog open
  Ctrl+wheel is unchanged. Test:
  `host_colorsets::tests::ctrl_wheel_pages_a_long_list_and_plain_wheel_steps_a_row`.
- **P2-02 Chat while overlays are open.** The bound chat keys (`globalChat`,
  `teamChat`) now open the existing chat box over F1 Help, F2 Player List
  and the Esc menu. The key is consumed before accelerators, so it cannot
  press a button, and its character is not typed into the box. Escape
  closes only the chat, Enter sends and returns to the overlay. No other
  dialog (confirmations, Options, Remap) changed. Tests in
  `crates/ui/tests/runtime_input.rs`: `chat_opens_over_the_escape_menu_and_player_list_and_returns_to_them`,
  `chat_key_stays_out_of_confirmations`.
- **P2-04 Missing content.** Loading a build used to post one long chat line
  naming up to eight brick kinds. It now posts one short line (counts, and
  that everything is kept and saved) and sends the loader a dismissible
  message box with the grouped details: bricks by Add-On, prints listed
  apart (placed bricks whose print is not installed), the Add-On IDs to
  install or enable, and where to do it. `v20` is not offered as an Add-On
  to install. Tests: `missing_content_groups_bricks_by_addon_and_lists_prints_apart`
  and the updated `build_load_keeps_unknown_bricks_aside_and_preserves_existing_players`
  in `crates/sim/tests/session.rs`. A map load still logs the short line.
- **P2-05 Hosting diagnostics.** The "router did not open the port" verdict
  no longer says friends cannot join: it says the game could not open the
  port over UPnP and NAT-PMP, that friends may still join if the port was
  forwarded by hand (the game cannot test that), and lists the manual steps
  (UPnP on, or forward UDP port to this PC, and Windows Firewall).
  `Report`'s `Debug` output, which the client echoes to the console and so
  to logs and bug reports, now shows only whether an address or invite
  exists, and scrubs IPv4 addresses from the router error. Tests in
  `crates/net/src/reach.rs`: blocked and normal router wording, and no
  leaked address, invite key or gateway address in the logged report.

## Evidence

`cargo test -p bri-ui --no-fail-fast`, `-p bri-net --lib`,
`-p bri-sim --test session`, `cargo clippy -p bri-ui -p bri-sim -p bri-net
--all-targets -- -D warnings` and `cargo fmt --check` pass in the cloud
container, except six offscreen render tests that need a GPU adapter
(`no wgpu adapter`) and `bri-client` cannot be built there (no ALSA). The
PC gate is the authority.

## Not done

- No new screenshot of the Brick Selector hint or the message box; the
  offscreen render tests did not run here.
- A real blocked router is untested here: only the report wording
  and verdict logic are covered, not an actual blocked router.
