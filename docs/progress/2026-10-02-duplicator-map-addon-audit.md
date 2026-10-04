# 2026-10-02 Duplicator map and Add-On integration audit

The New Duplicator's focused short-click lifecycle regression now exercises
the optional converted local Add-On content only when that test is selected.
Other New Duplicator port tests remain fixture-only when
`BRI_TEST_DUPLICATOR_CONTENT` is set. The focused test passes with both the
stand-in fixture and the read-only converted content in
`/Users/maxhastings/Documents/BlockReImagined/content`.

Added a classic Duplorcator regression that plants, selects through the tool's
short click path, adopts the player into a fresh map session, then repeats the
selection. This covers the classic Add-On's package player defaults on map
adoption as well as the New Duplicator case. The classic regression passes.

Commands run:

- `rustfmt --edition 2024 crates/addon-import/tests/ports.rs`
- `cargo test -p bri-addon-import --test ports duplorcator_selects_after_map_adoption -- --exact --nocapture` — passed (1 test).
- `cargo test -p bri-addon-import --test ports new_duplicator_selects_after_a_short_click_and_after_map_adoption -- --exact --nocapture` — passed (1 test).
- `BRI_TEST_DUPLICATOR_CONTENT=/Users/maxhastings/Documents/BlockReImagined/content cargo test -p bri-addon-import --test ports new_duplicator_selects_after_a_short_click_and_after_map_adoption -- --exact --nocapture` — passed (1 test).

No game window or interactive input was used. The primary content path was only
read. The bot-fire ally-corridor helper currently treats non-finite aim vectors
as clear because its distance comparison fails open; this is a hardening lead,
not evidence of the reported crash, and should be considered alongside the
active projectile/collision crash investigation. No change was made there.

Next: root should run the broader Add-On ports regression set and integrate
these tests with the session map-adoption and client body/ghost changes.
