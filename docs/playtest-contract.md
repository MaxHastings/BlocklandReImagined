# First core-building playtest

> Superseded by [STATUS.md](STATUS.md), which holds the current priorities
> and release definition of done. Kept as history.

Maxwell explicitly changed the immediate handoff on 2026-09-26: wrap up, focus on
necessities, and deliver a **core building playtest first**, with unfinished
features clearly listed. This is the current packaging gate. The complete vanilla
alpha in `alpha-contract.md` remains the longer-term target, not the prerequisite
for this first playtest and not a claim this build makes.

## Required before this handoff

- Windows package launches without Rust, a compiler or the original installation.
- Menus, options, map selection, loading, pause and HUD work through normal flows.
- Original maps load; player movement, jump/crouch/jets and first/third-person
  cameras are usable. Clearly disclose remaining terrain and visual limitations.
- Building: brick selection/favorites, ghost/numpad placement/rotation, plant,
  cancel, paint, hammer, printer and implemented wrench properties/events.
- Save/load: create a build, save it, and reload it; original converted reference
  builds are selectable. Clearly disclose unsupported legacy event behavior.
- Basic host/direct-IP join, two-player building/chat and late join have headless
  evidence and documented connection setup. Do not hide setup dependencies.
- Fresh-state startup, map/session exit/re-entry, core input/menu transitions,
  content availability, and bounded offscreen rendering have been checked.
- Include simple launch instructions, focused Maxwell playtest checklist, known
  limitations, build/content identity and useful logs. Interactive feel is for
  Maxwell to assess; no desktop input, visible test game or audible playback.

## Deferred from this first playtest

Complete combat/weapon damage, vehicles, minigames, full event coverage, complete
administration, large-world streaming/performance and remaining visual fidelity.
These remain tracked work, not silently completed or removed from the full alpha.
Do not integrate unfinished large subsystems just to increase the feature count.

Finish current coherent changes and fix blockers on the required path. Maxwell
subsequently explicitly asked to **wait for Opus's terrain handoff** before the
final playtest build. Integrate it and verify on Windows before packaging; cloud
software-Vulkan evidence alone is insufficient. Record the implementation shipped.
While waiting, Maxwell authorizes small fixes and nice-to-haves by the Luna
agents. Keep those bounded to menu/input polish, useful errors and packaging/log
helpers; do not reopen large feature or subsystem expansion.

After Opus delivered the partial terrain handoff, Maxwell explicitly requested
wrapping up this playtest release and pushing to GitHub. A new **private** source
repository is authorized for this first playtest. Original/generated content
remains excluded from Git. This does not claim full-alpha completion.

Opus's delivered terrain data/conversion/collision components are preserved, but
its renderer and all runtime wiring were not implemented. Ship the previously
tested map-bundle-014 finite terrain path for this early building playtest, with
that limitation disclosed, rather than mark unimplemented streaming complete.
