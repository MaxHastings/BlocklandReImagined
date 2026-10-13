# 2026-10-13 v0.3.0 W6: ammo counters for HUD regions

Follow-up slice to W6 (HUD regions and per-body cameras), agreed with the
integration thread: a weapon's magazine reached clients only as the
bottom-print text, so no HUD could draw its own ammo counter.

## What changed

- **Wire** (`protocol-changes/held-ammo-notice.md`): `Notice::Ammo(Option<HeldAmmo>)`
  to the holder only, sent from `show_ammo` on every ammo change (shot,
  reload, switch, put away) just before the ammo display's bottom print:
  rounds ready (the reserve for a gun counted from it), magazine size,
  reserve (`None` when endless), reloading, and whether the print that
  follows is the ammo display.
- **HUD values** `ammo`, `ammo_max`, `ammo_reserve`, `reloading`
  (`bri_content::hud::Source`); `ammo`'s natural maximum is `ammo_max`.
- **`ammo` region** (the stock display's place, bottom right): claiming it
  hides the bottom print only while that print is the ammo display
  (`Core::bottom_print_ammo`), so rules' other bottom prints still show.
  `bottom_print` still hides every bottom print.
- **Playground**: `playground-runner-hud/ammo.json`, a sprite-font round
  count, a magazine bar, the reserve and a RELOAD flag.

## Decisions

- A notice rather than a snapshot field: the magazine is the holder's
  alone, notices are already per player and ordered with the bottom print,
  and `show_ammo` already runs on every change.
- Sent even when the gun has no ammo display (Display Ammo off), so a HUD
  can still count; `printed` is false then and nothing is hidden.

## Evidence

- `cargo test -p bri-sim --test script_api ammo`: the holder gets
  (5, 5, 10, not reloading, printed) on drawing the probe rifle with the
  print right after, (2, reloading) on the light key, an endless reserve as
  `None`, `None` for a gun without a magazine, and nobody else hears it.
- `cargo test -p bri-client --lib hud_regions`: counter text, bar share,
  missing reserve as `-`, RELOAD by `show`, gone when the gun is put away.
- `cargo test -p bri-client --test hud_regions_render`: the Runner draws
  the Carbine; the `ammo` region draws "24" and "x96" in sprite digits and
  the stock ammo print is hidden (`runner-ammo.png`).
