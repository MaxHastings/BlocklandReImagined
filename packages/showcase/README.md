# Showcase Add-Ons

Add-Ons built only from what any Add-On maker gets: data, Rhai rules,
WebAssembly and WGSL shaders. The Gravity Gun and the Steel Ball are each
three Add-Ons, split the way the platform splits sides:

| Add-On | Side | What it is |
|---|---|---|
| `gravity-gun` | host | the rule: grab what you point at, drag and swing it, reel it with the wheel, drop or fling it (`physics` operations; the engine's `hold` does the moving) |
| `gravity-gun-tool` | everyone | the tool in your hand, the stock Printer by reference; its image runs the rule's commands on pressing and letting go of the trigger and on the mouse wheel |
| `gravity-gun-fx` | each player | the Printer's alien skin, the beam from its muzzle, the grip glow, bubble, sparks, shockwave and sounds, drawn from the rule's public `beam` state; with the Ragdoll Add-On, a held corpse dangles from the limb the beam grabbed |
| `steel-ball` | host | the rule: roll or hurl a ball, three per player |
| `steel-ball-kit` | everyone | the ball (a seatless `Ball` vehicle: in minigames only it punches through bricks, kills the players it hits and wrecks vehicles), its bare-metal model and textures, and the hand-held ball |
| `steel-ball-fx` | each player | the clank and thud sounds |
| `throwmod` | host | with empty hands, click a player in a minigame (or the bot from your own bot brick) to lift them limp into your arms, click again to throw them; our own rule after Nobot's Script_Nobotthrowmod (`on_activate`, `mount_object`, `set_scale`, `set_look_limits`) |
| `ragdoll` | each player | a dead Blockhead goes floppy instead of the death animation (`physics.local`, `avatar.pose`); other Add-Ons can grab and throw the body |

Every copy of the game carries them turned off (`"enabled": false` in
`packages/default-addons.json`), so a host turns them on in Add-Ons. The
Ragdoll is only on that player's screen and changes nothing in the game.

Read them alongside [the modding guide](../../docs/modding/README.md).
Generated files come from `tools/make_steel_ball_assets.py` (the ball's
model, steel and detail textures and vehicle entry) and `tools/make_showcase_sounds.py` (the
sounds); each `client/main.wasm` is built from the `main.wat` beside it
(`BRI_BLESS=1 cargo test -p bri-client-sandbox --test showcase`, and
`--test ragdoll` for the Ragdoll).

Tests: `cargo test -p bri-sim --test showcase` (gameplay),
`cargo test -p bri-sim --test throwmod` (Throwmod and the seams under it),
`cargo test -p bri-net --test showcase` (a second player sees a lift and drop),
`cargo test -p bri-client-sandbox --test showcase -- --include-ignored`
(the effects, rendered offscreen with a GPU).
