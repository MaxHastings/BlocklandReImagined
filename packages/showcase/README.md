# Showcase Add-Ons

Add-Ons built only from what any Add-On maker gets: data, Rhai rules,
WebAssembly and WGSL shaders. The Gravity Gun, the Steel Ball, the
Grapple Rope and the Grappling Hook are each three Add-Ons, split the way
the platform splits sides:

| Add-On | Side | What it is |
|---|---|---|
| `gravity-gun` | host | the rule: grab what you point at, drag and swing it, reel it with the wheel, drop or fling it (`physics` operations; the engine's `hold` does the moving) |
| `gravity-gun-tool` | everyone | the tool in your hand, the stock Printer by reference; its image runs the rule's commands on pressing and letting go of the trigger and on the mouse wheel |
| `gravity-gun-fx` | each player | the Printer's alien skin, the beam from its muzzle, the grip glow, bubble, sparks, shockwave and sounds, drawn from the rule's public `beam` state; with the Ragdoll Add-On, a held corpse dangles from the limb the beam grabbed |
| `steel-ball` | host | the rule: roll or hurl a ball, three per player |
| `steel-ball-kit` | everyone | the ball (a seatless `Ball` vehicle: in minigames only it punches through bricks, kills the players it hits and wrecks vehicles), its bare-metal model and textures, and the hand-held ball |
| `steel-ball-fx` | each player | the clank and thud sounds |
| `grapple-rope` | host | the rule: throw the hook where you aim, hang and swing from the brick or map it bites, climb and pay out with the wheel, let go to drop (`physics` operations; the engine's `tether` does the swinging) |
| `grapple-rope-tool` | everyone | the launcher in your hand, the stock Printer by reference; its image runs the rule's commands on pressing and letting go of the trigger and on the mouse wheel |
| `grapple-rope-fx` | each player | the Printer carved from jungle hardwood with bamboo bands and a vine, the braided hemp-and-vine rope sagging and snapping taut, the brass three-pronged hook and its sounds, drawn from the rule's public `rope` state |
| `grappling-hook` | host | the rule: one click fires the grapnel and the winch pulls you straight to the brick, map, player or vehicle it bites; hang there (switching items too) until the next click; jump reels in, crouch lets out; an admin's /hookobjects limits it to bricks and the map (`tether` with `straight`, `keys` and `object`) |
| `grappling-hook-tool` | everyone | the launcher in your hand, the stock Printer by reference; its image runs the rule's command on each click and on the mouse wheel |
| `grappling-hook-fx` | each player | the Printer rebuilt as a riveted gunmetal winch gun with brass drum bands and a glowing gauge, the taut steel cable that buzzes as it takes the load, the forged four-claw grapnel springing open and riding along on players and vehicles, and its sounds, drawn from the rule's public `hook` state |
| `hookshot` | host | the rule: point and click, the spearhead bites a brick, the map, a player or a vehicle and its chain hauls you straight there and lets go, landing you on the spot; click again (or put it away) to let go mid-flight with a third of your speed (`tether` with `straight` and `object`, `untether` with `keep`) |
| `hookshot-tool` | everyone | the launcher in your hand, the stock Printer by reference; its image runs the rule's command on each click |
| `hookshot-fx` | each player | the Printer recast as a temple relic of weathered bronze with carved glyph rings, gold filigree and a teal eye-stone, the chain of interlocking bronze links running back into the barrel as it hauls, the gold-bronze spearhead whose barbs spring out to bite, riding along on players and vehicles, and its sounds, drawn from the rule's public `hook` state |
| `ragdoll` | each player | a dead Blockhead goes floppy instead of the death animation (`physics.local`, `avatar.pose`); other Add-Ons can grab and throw the body |

Every copy of the game carries them turned off (`"enabled": false` in
`packages/default-addons.json`), so a host turns them on in Add-Ons. The
Ragdoll is only on that player's screen and changes nothing in the game.
The Grapple Rope is after the v20 Grapple Rope by Demian, SolarFlare and
Uristqwerty (original code by Qwertyuiopas), rebuilt from scratch: none of
its files are used. The Grappling Hook is after the Grappling Hook by
Conan, and the HookShot after the Hookshot by Loz, both rebuilt the same
way.

Read them alongside [the modding guide](../../docs/modding/README.md).
Generated files come from `tools/make_steel_ball_assets.py` (the ball's
model, steel and detail textures and vehicle entry) and `tools/make_showcase_sounds.py` (the
sounds); each `client/main.wasm` is built from the `main.wat` beside it
(`BRI_BLESS=1 cargo test -p bri-client-sandbox --test showcase`, and
`--test ragdoll` for the Ragdoll).

Tests: `cargo test -p bri-sim --test showcase`, `--test grapple_rope` and
`--test grappling_hook`, `--test hookshot`
(gameplay; `--test tether` for the engine's rope),
`cargo test -p bri-net --test showcase` (a second player sees a lift and drop),
`cargo test -p bri-client-sandbox --test showcase -- --include-ignored`
(the effects, rendered offscreen with a GPU).
