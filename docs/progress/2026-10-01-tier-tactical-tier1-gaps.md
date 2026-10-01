# 2026-10-01 Tier+Tactical: recoil shake nearby, ammo box counts

Two Tier 1 gaps the coordinator listed, each a generic seam:

- `Kick::radius`: Kai's recoil projectiles explode with a camera shake of
  radius 10 that every player near the shooter felt. The kick reader now
  reads `camShakeRadius`, and clients shake their view for anyone else's
  shot within it, with the explosion falloff (`ActorEffects::kick_near`).
  Clients already see every shot (tracers, new projectiles), so it sends
  nothing. The two kick readers (shots and script rules) are one now; it
  takes the mean of the three frequencies.
- `Item::label`: v20's `setShapeName` on an item, drawn by the name HUD
  above where it lies. Script rules read item methods (`on: "item"`) and
  gain `field`, `word<N>` and `text` filters, so Kai's
  `setShapeName(getWord(%obj.TT_ammoPickup[0], 1))` becomes the box's
  label.

A port that fails while an Add-On it requires is missing now says which.
The Gate's real-copy runs of Tier 1A, 2 and 2A failed that way: their
reference held no Tier 1. The import needs Tier 1 beside it, as the game's
Add-Ons folder has it.

Bag life (drop_item lifetime) and the EasterEgg setting wait for the
Slayer branch's seams to reach main.

Checks: `cargo test -p bri-addon-import -p bri-weapons -p bri-package`,
`cargo test -p bri-client --test actor_effects`, `cargo test -p bri-sim
--test fill_can`, workspace clippy `-D warnings`.
