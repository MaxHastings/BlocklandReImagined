# Showcase Add-Ons

Two Add-Ons that ship turned on with the game, built only from what any
Add-On maker gets: data, Rhai rules, WebAssembly and WGSL shaders. Each is
three Add-Ons, split the way the platform splits sides:

| Add-On | Side | What it is |
|---|---|---|
| `gravity-gun` | host | the rule: grab, hold, charge, throw and punt (`physics` operations) |
| `gravity-gun-tool` | everyone | the tool in your hand; its image runs the rule's commands on charge, fire and right click |
| `gravity-gun-fx` | each player | beam, force bubble, sparks, shockwave and sounds, drawn from the rule's public `beam` state |
| `steel-ball` | host | the rule: roll or hurl a ball, three per player |
| `steel-ball-kit` | everyone | the ball (a seatless `Ball` vehicle that smashes bricks and bowls players over), its model and texture, and the hand-held ball |
| `steel-ball-fx` | each player | the mirror-steel shader and the clank and thud sounds |
| `ragdoll` | each player | a dead Blockhead goes floppy instead of the death animation (`physics.local`, `avatar.pose`); other Add-Ons can grab and throw the body |

The Ragdoll is not held back like the others: every copy of the game
carries it turned off (`"enabled": false` in `packages/default-addons.json`),
so a player turns it on in Add-Ons. It is only on that player's screen and
changes nothing in the game.

Read them alongside [the modding guide](../../docs/modding/README.md).
Generated files come from `tools/make_steel_ball_assets.py` (the ball's
model, texture and vehicle entry) and `tools/make_showcase_sounds.py` (the
sounds); each `client/main.wasm` is built from the `main.wat` beside it
(`BRI_BLESS=1 cargo test -p bri-client-sandbox --test showcase`, and
`--test ragdoll` for the Ragdoll).

Tests: `cargo test -p bri-sim --test showcase` (gameplay),
`cargo test -p bri-net --test showcase` (a second player sees a throw),
`cargo test -p bri-client-sandbox --test showcase -- --include-ignored`
(the effects, rendered offscreen with a GPU).
