# Creator direction and current scope

Maxwell shared a longer architectural discussion on2026-09-26. Its useful
direction is incorporated here without treating its external product claims as
verified facts or replacing the agreed alpha contract.

The long-term product should let creators combine familiar bricks, tools, events,
vehicles, maps and sounds into different games. Classic Blockland is the first
reference experience. Authoritative state, rendering, physics and content readers
remain separate; stable authored IDs and versioned schemas are already in use.
Future data packages and gameplay APIs should expose the systems we use ourselves.

Easy modding should not require Rust or recompilation. Luau is a candidate to
evaluate after the native gameplay surface is concrete, not a selected dependency
or current compatibility claim. Sandboxed advanced extensions, package registries,
automatic content downloads and extensive creator tools are longer-term work.
Do not build a general TorqueScript VM or let a speculative platform delay the
playable alpha. Keep converter recipes rerunnable even though runtime content is
native: fixing conversion defects must not require manually remaking every asset.

Original toy proportions, expressive simple characters, readable paint colors,
sounds and first-class environments are requirements. Physics supports gameplay;
it does not define movement feel. The gameplay-specific motor remains tunable
around Rapier, and equivalent custom behavior is appropriate for jets, skiing,
projectiles and vehicles. No structural fragmentation/collapse experiments.

Offline and LAN operation should remain possible without a public account service.
Network authority and prediction serve familiar play and fast joining. Preserve
future portability, report actual measurements and keep mod/package compatibility
versioned rather than implicitly tying it to Rust memory layouts.

Separate Classic/Remastered modes, an expanded launch feature list and public
distribution services were suggestions in the shared text, not new alpha scope.
The required handoff remains every item in `alpha-contract.md`, including all
three maps and the Jeep. User playtesting remains Maxwell's responsibility.

Subsequent explicit scope expansion on 2026-09-26 now requires the complete vanilla
v20 set, including all stock vehicles/weapons/items, minigames, prints and events.
That user instruction supersedes the former minimum content slice. The platform
proposals above still do not add a public registry, scripting VM or new game modes
to the alpha. See the updated contract and `vanilla-coverage.md`.
