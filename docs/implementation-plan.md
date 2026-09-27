# Implementation plan

Current scope is the complete vanilla v20 contract expanded on 2026-09-26.
Earlier one-vehicle/one-weapon/event-subset assignments are superseded. Track
every verified stock entry through conversion, behavior, UI, networking and tests;
shared pipelines reduce repeated work but do not implement gameplay callbacks.

## 1. Technical preflight (foundation implemented; coverage work continues)
- Establish Cargo workspace, reproducible toolchain/dependencies and verification commands.
- Verify wgpu through a bounded offscreen render/readback without desktop input.
- Verify Rapier's Rust integration, collision, character and vehicle controllers in headless scenes.
- Establish native content identities, source provenance, schema versions and diagnostics.
- Implement bounded legacy readers/resolution and convert representative assets.

## 2. Shared authoritative simulation (active; state/save/event foundation implemented)
- Authored brick state, grid conventions, object ownership and command validation.
- Native persistence preserving legacy extension metadata.
- Server tick, event scheduler and stable-ID targets independent of GPU/physics.
- Two-client protocol proof with initial snapshot, ordered edits and late join.

## 3. Render and content integration
- General mission/interior/terrain conversion for Bedroom, Kitchen and Slopes.
- Brick batching/material overlays/prints and source-faithful lighting baseline.
- Animated customizable character, tool mounts, particles and audio.
- Physics world, measured movement/camera rules and every stock player/vehicle type.
- All stock weapons, items, projectiles, prints, music/sounds and effect bindings.
- Native adapters for stock brick interactions and original destruction/respawn.

## 4. Familiar complete workflows
- Cross-platform window/input and UI with original action names/layout references.
- Menus/settings/loading, build catalog/favorites, tools/wrench/events and chat.
- Full vanilla event catalog and dependent targets/parameters, minigames, stock
  host administration/trust, Add-Ons/Music selection and remaining vanilla flows.
- Event modernization: large editable event lists, ordered zero-delay relays,
  explicit budgets/cancellation/loop diagnostics and measured eight-player
  event/bot workloads. See `event-modernization.md`; later RPG modes motivate
  the foundation without expanding the alpha into an RPG implementation.
- Save/load and hosting/joining through the same authority as solo play.

## 5. Integration and handoff
- Representative original saves, automated network/persistence regression checks.
- Complete vanilla coverage audit; exercise every required content/behavior family
  in solo and multiplayer through the packaged application.
- Performance/load measurements, bounded rendering checks, crash/error logging.
- Packaging, content discovery/import UX, platform status and playtest guide.

Internal proofs are not user handoff milestones. Keep code and evidence durable
between turns and update progress before stopping. Revisit a decision when an
actual failure or measured constraint warrants it; avoid speculative framework work.
