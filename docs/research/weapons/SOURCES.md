# Source evidence and implementation decisions

The designated installation `E:/Downloads/B4v21Launcher/versions/Blockland v20` was opened read-only. Selected inputs are its shipped `Weapon_*`, `Item_*`, `Projectile_*`, `Vehicle_Tank` and `Vehicle_Pirate_Cannon` archives. Core dependencies come from `.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs`, recovered earlier from matching verified vanilla bytes. No community packages from the older installation define this scope.

`inventory.json` is a compact authored coverage/evidence index. The generated pack contains complete declaration fields, original source paths/lines and SHA-256 values, plus resource hashes. `artifacts/native-weapons/source-compact.txt` is an ignored local review aid; its line numbers are not canonical source citations.

| Family | Primary source and callback evidence | Native behavior |
| --- | --- | --- |
| Gun | `Weapon_Gun.zip/server.cs`, gunImage/gunProjectile | 90 speed, 30 damage, semi-auto release gate, smoke/reload, recoil, flash/shell/audio references |
| Akimbo | `Weapon_Guns_Akimbo.zip/Weapon_AkimboGun.cs`, AkimboGunImage/LeftHandedGunImage, onFireAkimbo | Right fires on press; left on release; independent image timers/mount/recoil |
| Bow | `Weapon_Bow.zip/Weapon_Bow.cs`, bowImage/arrowProjectile | 0.5 s activation, 0.05 fire + 0.5 reload, held repeat, 65 speed, gravity .25, 4 s life, stick/bounce parameters |
| Rocket Launcher | `Weapon_Rocket_Launcher.zip/Weapon_Rocket Launcher.cs` | 65 speed, direct30, explosion100/radius3, cooldown .7 s and release, ammo state |
| Spear | `Weapon_Spear.zip/server.cs`, onCharge/onAbortCharge/onFire | .7 s charge, short-release abort, armed-release throw, speed50/gravity.5/life20s, direct50/explosion40 |
| Sword | `Weapon_Sword.zip/server.cs` | Original draw/pre-fire/repeat/stop graph; short-lived speed50/35-damage projectile; armattack |
| Push Broom | `Weapon_Push_Broom.zip/server.cs` | Held .2 s strikes; zero damage,1300 impact and vertical impulse; rotCW |
| Horse Ray | `Weapon_Horse_Ray.zip/Weapon_HorseRay.cs`, HorseRayProjectile::Damage | HorseArmor transformation/recolor/dismount instead of nominal damage; original printer model |
| Keys | `Item_Key.zip/server.cs`, blueKey/greenKey/yellowKey | Red/blue/green/yellow models/colors; ten-unit ray; source HSV hue wrap, .1 threshold and greyscale exclusion |
| Skis | `Item_Skis.zip/Item_Skis.cs`, SkiWeaponImage::onFire, Player::startSkiing/stopSkiing | Authored image delay, blocked other mounts, +.3 native Y spawn,250ms delayed mount, velocity/node state; actual ski/death vehicle lifecycle delegated to vehicles |
| Sports | `Item_Sports.zip/basketball.cs`, dodgeball.cs, football.cs, soccer.cs, horse.cs, support.cs | Dribble/shoot/horse image variants, authored state graphs, lob/pass/lateral/pop/fumble/catch/tackle/steal rules, damage, bounce/rest/brick inputs and host movement commands |
| Special projectiles | `Projectile_GravityRocket`, `Projectile_Pinball`, `Projectile_Pong`, `Projectile_Radio_Wave` | Source lifetime/gravity/bounce/light/trail/sound; Radio Wave excludes player collision |
| Vehicle dependencies | `Vehicle_Tank.zip/Vehicle_Tank.cs`, `Vehicle_Pirate_Cannon.zip/Vehicle_Pirate_Cannon.cs` (pack records exact member names) | Tank shell/cannon ball speed120, direct100, tankShellExplosion170/radius8, gravity/TTL; additional death explosion projectiles |
| Clock | recovered core clockProjectile/ClockExplosion near lines19278–19322 | Native projectile/effect/impulse lifetime; root owns clock event timing |

Core recovered `WeaponImage::onFire` begins at line7827; native lowering includes melee eye origin/velocity ratio, first-person close-wall eye-origin correction, source-scale velocity and minShotTime. Core `ProjectileData::onCollision`, `onExplode`, `Damage`, `radiusDamage`, `radiusImpulse`, `impactImpulse` begin near 8014,8113,8275,8298,8339,8394. They ground minigame authorization separation, direct clamp, damage types, brick flags and force/impulse intents. Root remains responsible for exact source protection policies and passenger/ownership state.

Core `Projectile::Bounce` and `Redirect` begin near18392 and18420. Native synchronous contact responses preserve velocity math and 200-unit speed cap. Keeping the native projectile identity on redirect is an intentional internal representation choice; source Torque recreated the object. It does not confer authority or preserve original network IDs.

For engine-family behavior, the primary open-source [GarageGames Projectile implementation](https://github.com/GarageGames/Torque3D/blob/development/Engine/source/T3D/projectile.cpp) documents/implements 9.81 gravity and reflected velocity with tangential friction followed by elasticity. This supports the native integration choice, but it is **not proof of the proprietary v20 engine's exact implementation**. Stick-angle/blood-effect selection, source-collision grace, partial exposure, lifetime quantization and source-specific visual feel still need final source/behavior verification. Positive image timeouts are explicitly quantized upward to the native120Hz clock; original engine tick granularity differs.

The first conversion had no model failures, but a dependency audit found original `printGun.dts` material `blank` and its printer icon were outside the selected add-on archives. Pack003 now includes the original base texture and icon; all117 native resource records resolve. Colors inherited through item fields are resolved, including Push Broom's explicit source fractions. No white substitute texture was introduced. Shell physics and hidden non-Euler effect mount rotations remain presentation integration work, with original fields retained.

During implementation, tests exposed/guarded the important distinction between source image transitions and weapon callbacks: basketball swaps images while a trigger remains held, and sports/skis must emit an unmount even when the active image is temporarily taken out of the actor during tick processing. The final runtime handles both. Source review also corrected touchdown eligibility to **any sports ball**, and the vehicle agent corrected tumble timing: the requested3s duration is not an active automatic dismount timer in the shipped script.

This subsystem is an integration handoff, not alpha acceptance. Shared root manifests, shared engine code, docs/progress.md, original installations and audio implementation were not edited by this agent. The root should append the handoff evidence to docs/progress.md when integrating, while keeping the full alpha goal incomplete.
