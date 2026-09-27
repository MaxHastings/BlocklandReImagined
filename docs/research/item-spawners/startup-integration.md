# Catalog startup and content identity v8

Historical v8 checkpoint. Current startup also requires presentation pack003 and
content identity9; see [checked physics startup](item-physics-startup.md) for the
current CLI and additional integrity checks.

2026-09-26: native item catalogs are connected to normal client host/join and the
dedicated host. This extends the property slice in [README.md](README.md). It does
not establish completed weapon rendering, spawner placement or pickup/firing feel.
Root owns the live Session weapon lifecycle; muzzle positions remain provisional
eye origins until the presentation mounts are integrated.

## Connected paths

`ContentConfig.weapons` defaults to `weapons-pack-003`; old client-content config
JSON defaults this new field. `ContentPaths.weapons` follows existing package
containment validation. `ClientContent` loads a `WeaponContent` snapshot and
advertises `ItemData` entries. App installs its 17 native pack choices plus
Hammer/Wrench/Printer/Wand in ToolUi before server catalog or menu updates.

Host startup reloads and compares the weapon snapshot, installs both ToolCatalog
and `Session::set_weapon_pack` before opening the server, then advertises v8
identity. Join performs the same snapshot comparison and v8 calculation. Catalog
replacement after App construction requires restart, avoiding mismatched cached
UI and authority definitions. Dedicated startup uses the same loader and installs
the pack before accepting peers. Session rejects replacing definitions after
players have joined; it remains the authority for live inventory rules.

`WeaponContent::resolve_world_items` maps only native `item_ui` unresolved names
through validated, trimmed ASCII-lowercase catalog aliases. It retains missing
names and every source record, returning an unresolved count. ContentPaths applies
it before constructing map/reference-world simulation; dedicated startup applies
it after `load_startup` and reports the count in host metadata. Later build-append
jobs are root-owned and must use the same trusted resolution rather than parsing
original source records. Unknown resolved IDs are not silently replaced.

## Shared bounded loader and identity

`bri_net::content_identity::WeaponContent::load(root)` validates `bri_weapons::Pack`
and catalog IDs/names, canonicalizes every declared native resource under the pack,
and builds a deterministic digest over ordered filenames, sizes and actual bytes.
It limits the manifest to 32 MiB, resource entries to 4096, each native resource to
64 MiB and total manifest/resource bytes to 512 MiB. Shared native filenames are
hashed once; `weapons.json` cannot be reused as a resource. The parsed manifest's
bytes must match those hashed, and files changing size during hashing fail.

The source `Resource.sha256` is an original DTS/image checksum. It is retained in
the manifest and digest as provenance, **not** treated as the expected checksum
of converted shape JSON. Converted native bytes change peer identity even if the
original checksum does not change. This is content agreement between peers, not a
claim that the current pack contains independently certified native hashes. The
separate presentation pack will need its own checksum validation and identity
extension when integrated.

`extend_identity(base)` appends `BRI_WEAPONS_V1` under `BRI_NATIVE_RUNTIME_V8` after
the existing avatar/effects/audio/weather/foliage chain. Host, join and dedicated
all call that path; `CONTENT_IDENTITY_VERSION` is 8. Wire protocol version 7 is
independent. `with_weapons(base, root)` is a convenience loader/extension entry.

## Commands and evidence

All checks were headless, with no original writes, visible window, GPU rendering,
OS input or audio device/playback:

```powershell
cargo test -p bri-net --lib content_identity
cargo test -p bri-net --lib content_identity::tests::native_weapons_pack_identity_and_all_21_choices -- --ignored
cargo test -p bri-client --lib content::tests -- --skip local_native_content_index_and_lazy_maps
cargo test -p bri-client --lib app::tests::native_weapon_catalog_startup_and_headless_host -- --ignored --nocapture
cargo check -p bri-client -p bri-net --all-targets
cargo clippy -p bri-net -p bri-client --all-targets -- -D warnings
cargo run -p bri-net --bin bri-server -- content/stock-catalog-004 content/maps-pass-003 content/worlds-pass-004/1d1679fca49fa09325f55b8ac77c35ddc7c4131d6ff2e4ae96096c1af54dfca8.world.json content/map-bundle-014 content/brick-materials-001 content/effects-pass-004 content/avatar-pack-001 content/effects-runtime-pack-001 content/audio-pack-001 content/weather-pack-001 content/foliage-pack-001 content/weapons-pack-003 target/native-weapons-startup-server 127.0.0.1:0 2
```

Results: identity tests 10 passed/2 ignored; explicit actual pack test 1 passed;
content tests 6 passed; actual App startup/host test 1 passed; all-target check
and strict Clippy passed. The App test uses null audio and no GPU, exercises normal
host setup through UiAction, checks all 21 choices and receives the replicated
starter Hammer/Wrench/Printer inventory. Identity regressions cover converted-byte
changes, definition metadata, immutable snapshots, missing/escaping resources,
count/file/total budgets and alias/source-record retention. The actual native pack
contains 17 items and 117 declared native files (27 JSON, 90 PNG).

The two-second dedicated smoke completed 240 ticks, zero dropped ticks and zero
dropped cues. `host.json` records content identity 8, 21 item choices and zero
unresolved item references for that world. It saved a new native world on shutdown.
The smoke did not connect a remote player; the separate App test covered a local
loopback connection. Output summaries are retained in startup-evidence.json.
An initial check encountered a concurrent root-owned physics API mismatch; root
fixed it and the subsequent all-target check/Clippy passed.
