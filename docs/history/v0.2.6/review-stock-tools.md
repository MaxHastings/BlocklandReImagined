> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-stock-tools.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Stock tools read by their data: 5fd6cfaf (on 1bfaba28)

**Verdict: PASS. Merge 5fd6cfaf02e3d9396518ba345397400ab4bee613, which includes 1bfaba28.**

## What it does
- `combat::capability` returns None for any image with `on_fire` set.
  The hammer is caught earlier by `native_hammer`.
- `attack_of` reads the rest:
  - Tool(Destroy | AdminDestroy) is Some(None).
  - Any other on_fire is None.
  - A painting image is None.
- The rule matches the runtime (weapons/src/runtime.rs ~3222). When `on_fire`
  is set, the image runs it in place of launching, so a projectile it carries
  never flies. The stock wrench was the false weapon.
- No names. The test `stock_tools_read_as_their_data_says` checks hammer,
  wands, wrench, printer and spray can, plus capability on the wrench.

## Can it knock out a real weapon?
Not with any content today.
- `on_fire` is set only by the stock importer, by name (weapons-import/src/stock.rs:103):
  - hammer, wrench, printer and both wands;
  - skis;
  - basketball (Mount to basketballShootImage);
  - keys.
- Otherwise only an Add-On's own native JSON can set it. The Add-On importer
  never does (it ports `onFire` into `image.scripts`).
- No stock gun or mounted weapon has it. A dodgeball has no on_fire.
- The basketball's swap image reads as nothing. That is right: the ball hurts
  nobody.

Three edges where the bot's reader and the runtime part ways. None is reachable
with current content, so these are v0.2.7, one line each:
1. **Script precedence.** The runtime ignores `on_fire` when the image has a
   ported onfire script (`image.scripts["onfire"]`) or a `command`; that wins
   and fires. capability/attack_of should check the same way. Best as one shared
   helper in bri_weapons ("on_fire replaces the launch") that both use.
2. **Swap.** Mount(next) swaps in `next`, which then fires. attack_of should
   read `next` (one hop) rather than None. A modded swap-to-gun would otherwise
   read harmless.
3. **Non-native Break.** A Tool(Break) image that fails `native_hammer`
   (overridden callbacks) now reads None, "hurts nobody". A swing still lands,
   so it should be Some(None), cautious, like the wands.

The lane's fight probe found no change; a bot with hammer, wrench and printer
uses the hammer. That matches the code, since the hammer path is untouched.
