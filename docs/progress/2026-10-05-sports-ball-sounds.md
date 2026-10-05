# 2026-10-05 Sports ball sounds

Max asked whether the throwable balls had more sounds in v20 than ours. They
did. The balls are the stock Item_Sports Add-On (basketball, dodgeball,
football, soccer ball). Read from its scripts in the vanilla reference
(`support.cs`, `basketball.cs`, `dodgeball.cs`, `football.cs`, `soccer.cs`):

| When | v20 | Before this change |
|---|---|---|
| Throw, pass, pop, lateral, fumble, steal (`Player::spawnBall`) | `weaponSwitchSound` half a unit below the eye | silent |
| Bounce / any hit (`onCollision` → `Projectile::playSportBallSound`) | the ball's bounce sound, only above 3 u/s and not within 50 ms | silent |
| Catch or pickup (`passBallCheck`) | `weaponSwitchSound` | played |
| Basketball steal | `impact1ASound` | played |
| New football record (`CatchFootballMessage`) | `rewardSound` at receiver and passer | silent |

The basketball and football share `basketballBounceSound` (the football calls
`playSportBallSound()` with no sound and takes the helper's default).
`dropBall` and the scripted re-spawns (basketball spin, soccer header) pass
`%noSound` or skip `spawnBall`, so they stay silent.

## What changed

- `ProjectileDef.collision_sound` (`bri_weapons::CollisionSound`: profile,
  `min_speed`, `gap_ticks`): a projectile plays it where it hits something,
  faster than `min_speed`, at most once per `gap_ticks`. Any pack can use
  it; validation bounds it.
- `bri-weapons-import` reads it from scripts with `tscript::read`: a
  `serverPlay3D` at the top level of `<Projectile>::onCollision`, or a
  `Projectile::` method that body calls on the projectile, with the method's
  argument or default and its `vectorLen(getVelocity()) > N` and
  `getSimTime() + ms` guards. On the vanilla reference it finds exactly the
  four balls (basketball/football `basketballBounceSound`, dodgeball
  `dodgeballBounceSound`, soccer `soccerBounceSound`, each 3 u/s, 6 ticks)
  and nothing else.
- The runtime's shared ball release (`ball_released`) plays
  `weaponSwitchSound` for every thrown ball: the fire path, sport actions,
  tackle fumbles and steals.
- The session plays `rewardSound` at both players when a pass sets the
  football record.

Not changed: third-party Add-Ons through `bri-addon-import`. Its ports already
turn an `onCollision` sound into a bounce effect per Add-On (HE Grenade,
Explosive 1), so reading `onCollision` there too would play those twice. The
football record's star emote on the passer is visual and still missing.

## Evidence

- `cargo test -p bri-weapons -p bri-weapons-import`: all pass, including
  `a_ball_plays_its_collision_sound_on_fast_hits_only`,
  `collision_sounds_follow_on_collision_into_its_helper` (made-up scripts,
  no v20 content) and the throw-sound checks added to
  `sports_charge_throw_consume_catch_and_dodgeball_damage` and
  `sports_actions_tackle_steal_and_touchdown`.
- `bri-weapons-import` run on the vanilla reference into a scratch folder:
  the four balls above get their sounds.

## Next

The weapons pack must be regenerated (`python tools/bootstrap.py`) for the
bounce sounds to reach a build; the importer change marks it stale, which
also rebuilds the packs made from it.
