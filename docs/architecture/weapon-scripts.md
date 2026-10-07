# Weapon state scripts and stock images

Status: on main. Code: `crates/weapons/src/runtime.rs`
(`WeaponsWorld::callback`), the declared fields in `crates/weapons/src/lib.rs`
and the importer that fills them, `crates/weapons-import/src/stock.rs`.

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

Some v20 stock images did something no ordinary datablock field describes:
the key tries a brick, skis mount, a basketball swaps to its shooting image,
a spear raises the arm while charging, a dodgeball cannot be thrown just
after spawning. Each is a declared field the runtime reads, never a name:

- a state's `arm`: the arm move its script played (`playThread(2, X)`);
- `Image::left_image`: the image `onMount` put in the left hand
  (`mountImage(X, 1)`);
- `Image::on_fire` ([`OnFire`]): what `onFire` does instead of launching
  (a host building tool, skis, a key, or swapping to another image);
- `Image::sport` ([`Sport`]): a sports ball's throw, keys and kind;
- `ProjectileDef::sport_hit` ([`SportHit`]): a dodgeball's knock-out or a
  football's catch;
- `ProjectileDef::turns_into`: the horse ray's player type.

The v20 importer fills them (`weapons-import/src/stock.rs`). The arm move
and the left hand are read from the scripts themselves, by the same two
functions the Add-On importer uses (`script_arm`, `left_hand_image`). The
rest come from a small table by datablock name, the only one left, applied
to the vanilla import only, so an Add-On image never picks up stock
behaviour by sharing part of a name. These fields are not stable until the
modding API freeze.

A test (`weapons/tests/no_datablock_names.rs`) fails if the runtime
compares an image's, item's or projectile's name again, and
`weapons-import/tests/stock_parity.rs` checks the vanilla import against the
name table the runtime used before (2026-10-07).

## Adding a compatibility case

- A v20 **stock** image whose script did something new: read it from the
  script if the script says it plainly; otherwise add a field and fill it
  in `weapons-import/src/stock.rs`. The runtime acts on the field.
- An **Add-On** image: describe it in data (`Image::scripts`,
  `commands`, `shot`, `on_fire`, `sport`, a new `Image` field when two
  images would share it). Never by name: Add-On datablock names are the
  author's, and a name check would also catch another Add-On's image that
  shares a word.
