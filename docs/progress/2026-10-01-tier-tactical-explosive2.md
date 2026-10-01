# 2026-10-01 Tier+Tactical: Explosive 2

Kai's Explosive 2 (RPG, grenade launcher, Calibre Cannon, and the hidden
mortar) gets its port, `weapon_package_explosive2`, pinned to the Gate's
real copy (sha256 12213beb…9d869). Its covers are checked against the stand-in.

Engine, each a generic seam:

- `Children::on_hit`: children thrown at each thing the projectile hits,
  bouncing or bursting, never as it dies in the air (the flak round's
  `onCollision`, which runs before `Parent::onCollision`).
- `Children::angles`: each child along `(cos a, cos b, sin a)` (x, up,
  back) at `speed`, `a` and `b` whole degrees, as `PrjLoop_emitPrj` and
  the flak burst build `%vec`: not one length, more of them up and down.
- `Children::redraw`: `for(%i = 0; %i < getRandom(3, 5); %i++)` draws the
  limit again each pass: 3 a third of the time, 4 four ninths, 5 two
  ninths.
- `Children::max_times`: `every_ticks` children stop after
  `PrjLoop_maxTicks`.
- `Shot::lob`: the mortar's `onFire`: muzzle × speed, plus upward the
  distance from the holder's feet to where their look lands (`range` ×
  scale, `otherwise` when nothing) over a divisor, plus a whole-step
  jitter along the world's x and -z. The holder's scale and velocity are
  not added, as the script spawned the shell itself.
- `Aura::max_targets`: a pulse hurts only the first so many. This is for
  Explosive 1's "Molotov Targeting Bugfix" setting when it is off. Kai's
  default turns it on, so the port leaves it unset until the settings
  seam can switch it.

Importer:

- Script rules filling `children` on one projectile from several methods
  add their sets together instead of one replacing the other.
- `Code::sounds` covers the profiles of the Add-Ons an import builds on:
  Explosive 2's reloads name Tier 1's `block_MoveBrick_Sound`, which plays
  the base game's `clickMoveSound`.
- Shared Tier+Tactical rule: `if(vectorLen(...) > n) { %spread = moving; }
  else { %spread = still; }` (the RPG) reads as the moving spread.

Checks: `cargo test -p bri-weapons --test thrown_grenades` (8/8),
`cargo test -p bri-addon-import --test tier_port` (all), and the wider run
and clippy below before the push.
