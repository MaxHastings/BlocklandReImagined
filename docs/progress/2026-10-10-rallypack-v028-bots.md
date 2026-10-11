# 2026-10-10 Rallypack basic bot weapon support on v0.2.8

Branch: `codex/rallypack-p90-v028`, based on the exact v0.2.8 release.
Maxwell requested that bots use supported Add-On weapons. This is a package
conversion change; no runtime, protocol, executable or original-content edit.

The port had cleared image projectile references because Rhai commands own
shots. Runtime inspection showed the top-level image command intercepts native
onFire and returns before launching the image projectile. Restoring the actual
projectile reference therefore supplies flight/range/blast information to the
existing scripted-weapon bot executor without duplicate firing. Added a shared
conversion helper that reads held-trigger/ammo/timeout transitions to choose
hold versus tap, rather than using weapon names to guess bot mechanics.

P90 and 43 full-pack items now have these descriptions. Variable-charge bow
and cooked M67 grenade remain unsupported by this change. Package ammo and
automatic empty-magazine reloads remain ordinary shared rules. No promise of
ammo-aware switching, tactical reloads, aiming modes, advanced recoil handling
or full native attack planning for package commands: v0.2.8 needs richer hooks
for those. Stock native weapons can take priority in a mixed loadout; first
playtest should give the bot only the intended weapon.

Evidence:

- A private ignored integration test creates a rules bot in a minigame, gives
  it only the P90, and leaves all equip/trigger/reload decisions to the bot.
  Target absorbs hits through a test-only damage hook so the bot can empty a
  magazine. Assertions prove a projectile fired, a hit reached the target and
  a spare magazine was consumed by completed reload. Previous package fails
  with `trigger none`; updated package passes. Initial test fixture needed a
  named kind, a single catalog installation and removal of stock weapons from
  its default minigame loadout; those were fixture corrections.
- `cargo test -p bri-net --test rallypack_p90 --test rallypack -- --ignored`
  passes all six checks against final private outputs, with
  `BRI_P90_TEST_ROOT=E:\Downloads\BRI-P90-native-v028-bot-support`,
  `BRI_RALLYPACK_TEST_ROOT=E:\Downloads\Rallypack-native-v028-bot-support`,
  `BRI_P90_CONTENT_ROOT=E:\Downloads\BlocklandReImagined-v0.2.8-windows\content`
  and `CARGO_PROFILE_DEV_DEBUG=0`. Includes clean-cache download and late-join
  checks, P90 ammo/minigame checks and full-pack family smoke check. Only P90
  has the autonomous bot acceptance test; other weapons' bot handling awaits
  Maxwell's playtest.
- Released `bri-client.exe --check E:\Downloads\BRI-Rallypack-startup-check
  E:\Downloads\BRI-Rallypack-startup-state` exits 0. Final enabled-content
  snapshot has no Rallypack/P90 faults; only the two previously known
  Butterfly Knife warnings. stdout/stderr saved under full-pack final output.
- Both builders complete, retaining all original models/audio/effects and
  reporting no unresolved full-pack materials. First invocation named the
  importer incorrectly; corrected to released `bri-import-addon.exe`.

Installed four updated package folders into the requested release's
`content/addons`, preserving enabled selections and executable. Refreshed
`E:\Downloads\Rallypack-Weapons-native-v028.zip` and standalone P90 ZIP.
Original ZIPs and vanilla reference remain read-only. Maxwell restarts the
game, enables the weapons, assigns one gun to a bot in a damage-enabled
minigame and performs interactive acceptance. No visible game or gameplay
automation, push, main integration or alpha completion claimed.
