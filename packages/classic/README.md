# Classic Add-Ons

Our own remakes of well-loved v20-era Add-Ons. Nothing of the originals is
here: the models, animations, textures and sounds are made by
`tools/make_classic_weapons.py` (Python standard library only, the same
output every run), and each `weapons.json` says how it plays. The original
authors' designs are credited in each `package.json`.

| Add-On | What it is | Design by |
|---|---|---|
| `butterfly-knife` | A balisong that flips open as you draw it. Click to jab (30), hold 0.7 s and let go to stab (100). The hit is the Sword's. | Stratofortress |
| `he-grenade` | Click to pull the pin (it flies off), hold and let go to throw. It bounces and goes off 2.5 s later: 250 damage within 17, a push within 20, and bricks blown loose within 10 as a rocket's do. Thrown grenades are used up; carry several. | TheGeek, Pload, Rotondo and Ephialtes |

Both ship turned off (`"enabled": false` in `packages/default-addons.json`);
a host turns them on in Add-Ons. They are weapons packs only, so everyone
in the game gets them from the host.

They use only data seams any Add-On has: image state `holder_sequence`,
`projectile` and `use_up`, casings, their own models and sounds, and an icon
drawn from the model (`look.materials`). See
[the modding guide](../../docs/modding/README.md), section 5, and
[torque-equivalents.md](../../docs/modding/torque-equivalents.md).

Tests: `cargo test -p bri-weapons --test state_attacks` (how they play),
`cargo test -p bri-client --lib classic_tests` (their art loads and
animates; writes `target/classic-weapons.png`).
