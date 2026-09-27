# Native UI handoff integration — 2026-09-26

Claude's transferred UI code, content and renders are present locally. Both
crates now build in the root workspace with its shared lockfile. The originally
reported 25 UI + 9 converter tests passed after transfer. Astra and explicitly
user-authorized Astra High workers completed the native screen layer and added
integration/behavior tests; see crates/ui/README.md for the active host contract.
The old wip drafts remain uncompiled reference files.

Native screens now cover menu/host/join/loading/pause, default controls/options/
remap, HUD/chat, bricks/cart/favorites, prints, wrench variants/events,
avatar/palette/favorites, save/load and player list. The integration handles
modal releases, repeat cancellation, request results, stale connection tokens,
inventory bounds and visual-only authoritative slot updates. Tests use data
input and offscreen rendering, never the desktop or a running game.

Full-install conversion was independently checked in ui-pack-002. Canonical
ui-pack-003 adds 30 original stock brick icons from the native catalog: 558 images,
39 fonts on 41 sheets, seven skins, 114 styles and 68 layouts. It inventories
26 installed maps; vanilla classification remains separate. All 166 selectable
stock brick icons resolve. Provenance reports verify 660 asset inputs, four
recovered scripts plus the native catalog, 600 outputs and the manifest checksum.
Original installation and previous packs remain unchanged. The 23 warnings are
documented in ui-conversion.md.

The 48-frame native runtime gallery under artifacts/native-ui-runtime exercises
three viewport/scale combinations, actual menu dispatch/cancel flows and a
nonsquare external-texture UV check. Agent-specific authored dialog/selector/
avatar tests supplement it. These are UI-layer checks, not gameplay acceptance.
The UI milestone workspace gate passed 145 tests, with four asset-dependent GPU tests
ignored by default and also verified explicitly. Formatting and all-target
Clippy with warnings denied pass. The 48-frame probe was rerun successfully
after the final integration fixes.

The subsequent `bri-client` milestone adds the native window/compositor, map
loading, movement/chat network adapters and settings persistence. Its offscreen
integration catches and fixes initial HUD placement and late catalog resizing.
See `native-client.md` and current `progress.md` for evidence and limits.

Building/ghost and basic tool adapters now connect through real authority;
all 77 converted default prints have original icons, compatible selection and
render bindings. Wrench/event integration currently implements only the documented
native subset, with opaque source rows retained read-only.

Remaining required work includes the other gameplay adapters, real avatar previews,
full vanilla event/minigame targets, trust/admin/add-on/music/server-config flows,
remaining graphics/audio controls and presentation polish. Current host capability
flags do not reduce the full vanilla contract. Missing behavior is kept explicit.

Working UX choices use Maxwell's existing pivot authorization: remapping replaces
old bindings, Linux uses Windows defaults, and dialogs/typing block gameplay while
releases still work. UI native save names use .world.json; actual storage/permissions
are host policy and remain unimplemented here.

Tutorial exists in both transferred 001 and full 002/003 packs as a mission entry;
earlier audit wording claiming absence is stale against these inputs. Its stock
provenance, dependencies and playable behavior remain open. Crosshair/camera feel
and detailed visual fidelity still require Maxwell's eventual playtest.
