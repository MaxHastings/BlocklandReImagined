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

A unit test (`the_state_script_callback_never_matches_image_names`) fails
if the callback reads an image's name again.

## Adding a compatibility case

- A v20 **stock** image whose script did something new: add it to
  `Stock` and `Stock::named`, and make the callback act on the field.
- An **Add-On** image: describe it in data (`Image::scripts`,
  `commands`, `shot`, a new `Image` field when two images would share it).
  Never by name: Add-On datablock names are the author's, and a name check
  would also catch another Add-On's image that shares a word.

## Still to do

`sport_trigger` (a sports ball's jet-key pass, lateral and pop) still names
its balls; it is the next case to move into `Stock`.
