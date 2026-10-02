# Weapon state scripts and stock images

Status: on main. Code: `crates/weapons/src/runtime.rs`
(`WeaponsWorld::callback`) and `crates/weapons/src/runtime/stock.rs`.

## The callback

A held image's state machine runs a script when it enters a state
(`onCharge`, `onPreFire`, `onFire`, `onFireAkimbo`, ...). In v20 each was a
TorqueScript function on the image's datablock (`spearImage::onCharge`).
`WeaponsWorld::callback` is the engine's one handler for all of them, in
this order:

1. The image's Add-On commands for that script (`Image::commands`).
2. A cooked grenade's fuse (`Image::cook`).
3. A ported script (`Image::scripts`): its arm animation, and whether it
   fires.
4. The built-in handling of the script name, which for `onFire` launches the
   image's projectiles from its data (`shot`, `volleys`, `last_shot`,
   `state_shots`, `magazine`, hitscan).

## Stock images

Some v20 stock images did something no data field describes: the key tries
a brick, skis mount, a basketball swaps to its shooting image, a spear
raises the arm while charging, a dodgeball cannot be thrown just after
spawning. Those cases live in one table, `runtime/stock.rs`:
`Stock::of(image)` returns what that image's scripts did (its arm animations,
its `StockFire` kind, its throw), by its datablock name. The callback reads
the `Stock` and never the name.

The rest of the runtime follows the same rule. The akimbo gun's left
image (`Stock::left_image`), the sports balls' movement keys
(`SportKeys`), their catches and drops (`Ball`), and the stock
projectiles whose `onCollision` did more than their data says (the
dodgeball, football and horse ray, `StockProjectile`) are all read from
this file.

A unit test (`the_runtime_never_matches_datablock_names`) fails if
`runtime.rs`, `runtime/sports.rs` or `runtime/persistence.rs` compares an
image's or projectile's name again.

## Adding a compatibility case

- A v20 **stock** image whose script did something new: add it to
  `Stock` and `Stock::named` (a projectile: `StockProjectile`), and make
  the runtime act on the field.
- An **Add-On** image: describe it in data (`Image::scripts`,
  `commands`, `shot`, a new `Image` field when two images would share it).
  Never by name: Add-On datablock names are the author's, and a name check
  would also catch another Add-On's image that shares a word.

