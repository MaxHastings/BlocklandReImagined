# Showcase Add-Ons

Two Add-Ons that ship turned on with the game, built only from what any
Add-On maker gets: data, Rhai rules, WebAssembly and WGSL shaders. Each is
three Add-Ons, split the way the platform splits sides:

| Add-On | Side | What it is |
|---|---|---|
| `gravity-gun` | host | the rule: grab what you point at, drag and swing it, reel it with the wheel, drop or fling it (`physics` operations; the engine's `hold` does the moving) |
| `gravity-gun-tool` | everyone | the tool in your hand, the stock Printer by reference; its image runs the rule's commands on pressing and letting go of the trigger and on the mouse wheel |
| `gravity-gun-fx` | each player | the Printer's alien skin, the beam from its muzzle, the grip glow, bubble, sparks, shockwave and sounds, drawn from the rule's public `beam` state; with the Ragdoll Add-On, a held corpse dangles from the limb the beam grabbed |
| `steel-ball` | host | the rule: roll or hurl a ball, three per player |
| `steel-ball-kit` | everyone | the ball (a seatless `Ball` vehicle that smashes bricks and bowls players over), its model and texture, and the hand-held ball |
| `steel-ball-fx` | each player | the mirror-steel shader and the clank and thud sounds |
| `grapple-rope` | host | the rule: throw the hook where you aim, hang and swing from the brick or map it bites, climb and pay out with the wheel, let go to drop (`physics` operations; the engine's `tether` does the swinging) |
| `grapple-rope-tool` | everyone | the launcher in your hand, the stock Printer by reference; its image runs the rule's commands on pressing and letting go of the trigger and on the mouse wheel |
| `grapple-rope-fx` | each player | the Printer carved from jungle hardwood with bamboo bands and a vine, the braided hemp-and-vine rope sagging and snapping taut, the brass three-pronged hook and its sounds, drawn from the rule's public `rope` state |
| `ragdoll` | each player | a dead Blockhead goes floppy instead of the death animation (`physics.local`, `avatar.pose`); other Add-Ons can grab and throw the body |

The Ragdoll and the Grapple Rope are not held back like the others:
every copy of the game carries them turned off (`"enabled": false` in `packages/default-addons.json`),
so a player turns them on in Add-Ons. The Ragdoll is only on that
player's screen and changes nothing in the game. The Grapple Rope is
after the v20 Grapple Rope by Demian, SolarFlare and Uristqwerty (original
code by Qwertyuiopas), rebuilt from scratch: none of its files are used.

Read them alongside [the modding guide](../../docs/modding/README.md).
Generated files come from `tools/make_steel_ball_assets.py` (the ball's
model, texture and vehicle entry) and `tools/make_showcase_sounds.py` (the
sounds); each `client/main.wasm` is built from the `main.wat` beside it
(`BRI_BLESS=1 cargo test -p bri-client-sandbox --test showcase`, and
`--test ragdoll` for the Ragdoll).

Tests: `cargo test -p bri-sim --test showcase` and `--test grapple_rope`
(gameplay; `--test tether` for the engine's rope),
`cargo test -p bri-net --test showcase` (a second player sees a lift and drop),
`cargo test -p bri-client-sandbox --test showcase -- --include-ignored`
(the effects, rendered offscreen with a GPU).
