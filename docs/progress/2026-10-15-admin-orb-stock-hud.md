# 2026-10-15 Admin orb uses the stock HUD

Maxwell requires F8's admin ball to always show the Blockland HUD, even
when the parked body has a custom HUD. `update_package_hud` now checks the
authoritative local `Vitals.control == ControlObject::Camera` before
selecting package panels or region claims. It clears panel/key hints,
region draws, stock-text replacements, tracker/kill-feed state and client
HUD ownership. Package bindings and the cached catalog are retained so the
body's HUD returns when control returns to the player. This rule applies
to every body and package; spectator, death and package cameras retain
their existing HUD behavior.

Extended the existing offscreen `hud_regions_render` probe: a Runner with
its custom reticle and live ammo display enters the actual server admin
camera, retains its archetype, releases HUD claims and stock text overrides,
then returns via F7 and restores the body's reticle and ammo claim.

Validation: `cargo test -p bri-client --test hud_regions_render -- --nocapture`
passed its synthetic loopback/offscreen case (32.85 seconds); the generated
v20 content variant remains ignored by the existing test configuration.
`cargo fmt -p bri-client` passed. The quick client tier first exposed a
vehicle-test Wheel literal missing the new steering default from the
parallel fidelity work in this branch; added `linear_steering: false`.
The corrected client quick tier passed: 558 unit tests passed, 76 existing
ignored tests, formatting passed (88 seconds total). `git diff --check`
passed. The client focused tier also passed: clippy with warnings denied,
formatting and 558 unit tests (125 seconds total), with its log in the main
clone's ignored `artifacts/halo-ce-client-focused.log`.

Not installed into the running playtest executable. No interactive inputs
or visible game launches were performed.
