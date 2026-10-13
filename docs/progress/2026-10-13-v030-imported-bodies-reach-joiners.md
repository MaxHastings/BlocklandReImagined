# 2026-10-13 v0.3.0 W5: imported bodies reach joining players

Branch `claude/gltf-importer-boc77k`, into `v0.3.0-experimental`. Follows
`2026-10-13-v030-gltf-lod-and-body-box.md`.

## Problem

A body from `bri-import-addon gltf --kind body` provided only its
`archetype`, a host-only kind, so the Add-On was `server` side
(`bri-addon-check`: "players never download it"). A friend joining a host
who plays Mario would never get the body. Even with the files downloaded, a
join reloaded content only for new bricks, weapons, vehicles or bots, so the
joiner would still have drawn a Blockhead.

## What changed

- **New content kind `body`** (`crates/package-runtime/src/content.rs`): a
  native shape an archetype names as its `model`, its PNGs in
  `assets/content.json`. Client side, so an Add-On with a player type and
  its body is `shared` (`side_for_package`'s rule is unchanged: the kind is
  neither host-only nor personal). The package runtime parses and validates
  the shape, so `bri-addon-check` names a broken one. Limit 32 MiB.
- **The importer** writes the body as `<id>:body/<id>` (was
  `<id>:asset/<id>.glb`) provided as `body` beside the archetype.
- **The client** loads every `body` an Add-On provides
  (`avatar::body_models`, used by `AvatarAssets::load_bodies`), besides the
  v20 imports' archetype-named assets as before.
- **Joining** reloads content when a downloaded Add-On brings a body
  (`app/session.rs`), so the joiner draws it.
- Weapons and vehicles were not affected: imported items and props go into
  a weapon or vehicle Add-On, which is shared and already downloaded and
  reloaded on join (`a_guest_without_a_vehicle_add_on_downloads_it_and_can_spawn_it`).
- Docs: `docs/modding/content-kinds.md`, `docs/architecture/package-runtime.md`,
  `docs/modding/gltf-models.md`, and the authoring guide in project files.

## Evidence

- `cargo test -p bri-client --test add_on_join -- imported_body`:
  `a_guest_downloads_an_imported_body_and_sees_the_host_as_it` (synthetic
  content). A host runs an imported figure through a mode that makes
  everyone the figure; a guest without it joins, downloads `test_figure`
  (42 KB), and draws the host as `test_figure:body/test_figure`. Before the
  reload change the guest downloaded it and still drew a Blockhead.
- `cargo test -p bri-addon-import --lib gltf`: the package test asserts the
  body Add-On is `shared`. `cargo test -p bri-package-runtime`:
  `archetype` with `body` is shared.

## Next

- A v20 import providing only player types (no weapons or bots) is still
  host-only for the same reason; none is in alpha scope yet. Giving the v20
  importer the `body` kind would fix it the same way.
