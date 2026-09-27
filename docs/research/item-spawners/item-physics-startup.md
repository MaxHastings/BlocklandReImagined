# Checked item physics startup and identity 9

2026-09-26: client host/join and dedicated startup now load authored native bounds
for all 21 catalog items. `ContentConfig.item_presentation` defaults to
`item-presentation-pack-003`, including old config JSON that omits the new field.
`ContentPaths.item_presentation` uses the existing contained package resolver.

`bri_net::content_identity::ItemPhysicsContent::load(root, &weapons)` returns
`bounds: BTreeMap<String, bri_weapons::ItemBounds>` plus an identity snapshot.
It needs no renderer, original DTS reader or original installation. App stores the
snapshot, reloads and compares it before host/join, and rejects resource replacement
until restart. Host and dedicated call `Session::set_item_bounds(bounds)` immediately
after `set_weapon_pack`, before any connections. Root's Session setter initializes
the static-item state and rejects replacement after players have joined.

## Integrity chain

The delivered format deliberately avoids duplicating provenance in every bound:

1. `presentation.json` schema 2 pins the actual weapons.json SHA-256 and
   `item-physics.json` SHA-256.
2. `item-physics.json` schema 1 contains `{items:{stable_id:{min,max}}}`. Its IDs
   must exactly equal the weapon catalog plus the four core tools, as must the
   presentation item IDs.
3. Each item's presentation model must exist. Physics bounds must be finite,
   ordered, within native ItemBounds limits and exactly match that model's
   `bounds_min`/`bounds_max`. For pack weapons, the model key must also match the
   normalized model reference in the weapon definition.
4. Every declared presentation model and texture resolves canonically within the
   package and is checked against its native byte checksum. Model `source_sha256`
   is validated as provenance syntax; it is never compared to native JSON bytes.

Manifest budget is 32 MiB; physics metadata 2 MiB; model count 1024; texture count 4096;
each resource 64 MiB; the complete manifest/physics/resource digest is bounded to
512 MiB. Reserved paths, escapes, missing resources and conflicting duplicate
filename hashes fail. Native models are hashed without parsing/rendering them.
The presentation importer owns source-to-native bounds correctness; matching pins
establish package consistency, not an independent re-reading of original geometry.

`ItemPhysicsContent::extend_identity` follows the weapons v8 digest with
`BRI_ITEM_PRESENTATION_PHYSICS_V1` resources under `BRI_NATIVE_RUNTIME_V9`.
All startup paths use content identity 9. The independently versioned wire protocol
is 8 at this checkpoint. Dedicated CLI adds the presentation directory immediately
after weapons directory; the current complete command is in docs/networking.md.

## Verification and reproduction

```powershell
cargo test -p bri-net --lib content_identity
cargo test -p bri-net --lib content_identity::tests::native_item_physics_covers_all_21_and_pins_authored_bounds -- --ignored
cargo test -p bri-client --lib content::tests -- --skip local_native_content_index_and_lazy_maps
cargo test -p bri-client --lib app::tests::native_weapon_catalog_startup_and_headless_host -- --ignored --nocapture
cargo clippy -p bri-net -p bri-client --all-targets -- -D warnings
```

Results: 13 identity tests passed/3 ignored; explicit actual pack physics test 1
passed; content 7 passed; actual App startup/loopback host 1 passed; all-target strict
Clippy passed. Regressions cover stale pins, checksum corruption, missing/extra IDs,
invalid/mismatched bounds, schema mismatch, changed snapshot, unsafe paths and byte
limits. An initial test insertion landed inside a fixture impl; it was moved to
module scope and all subsequent tests passed. Concurrent root Session work initially
prevented compilation until its setter/contact index landed.

The dedicated smoke used a copied Demo House native world, assigning all 21 items
to its first 21 bricks. Half used unresolved original display-name references; the
rest used stable native IDs. Different position/facing selectors exercised bounds
placement initialization. Recreate that fixture from repository root in PowerShell 7:

```powershell
$fixtureDir = 'target/native-item-physics-startup'
New-Item -ItemType Directory -Force $fixtureDir | Out-Null
$fixtureWorld = Get-Content content/worlds-pass-004/1d1679fca49fa09325f55b8ac77c35ddc7c4131d6ff2e4ae96096c1af54dfca8.world.json -Raw | ConvertFrom-Json -AsHashtable
$fixtureItems = (Get-Content docs/research/item-spawners/source-inventory.json -Raw | ConvertFrom-Json).items
for ($index = 0; $index -lt $fixtureItems.Count; $index++) {
    $choice = $fixtureItems[$index]
    $reference = if ($index % 2 -eq 0) {
        @{kind='resolved';value=$choice.id}
    } else {
        @{kind='unresolved';value=@{namespace='item_ui';name=$choice.name}}
    }
    $fixtureWorld.bricks[[string]($index + 1)].item_spawn = @{
        item=$reference;position=$index % 6;direction=2+($index % 4);respawn_ms=4000
    }
}
$fixtureWorld.name = 'Native item bounds startup fixture'
$fixtureWorld | ConvertTo-Json -Depth 100 -Compress | Set-Content "$fixtureDir/world-input.json"
cargo run -p bri-net --bin bri-server -- content/stock-catalog-004 content/maps-pass-003 target/native-item-physics-startup/world-input.json content/map-bundle-014 content/brick-materials-001 content/effects-pass-004 content/avatar-pack-001 content/effects-runtime-pack-001 content/audio-pack-001 content/weather-pack-001 content/foliage-pack-001 content/weapons-pack-003 content/item-presentation-pack-003 target/native-item-physics-startup/server 127.0.0.1:0 2
```

The smoke initialized 21 static items, resolved every alias, advertised identity 9,
ran 240 ticks with zero dropped ticks/cues, and saved all 21 resolved IDs. A comparison
against the copied input verified each brick's original source records unchanged.
See [item-physics-startup-evidence.json](item-physics-startup-evidence.json) for the
captured metadata and assertions. No remote player joined this dedicated smoke;
the separate App test covered the normal loopback startup and starter inventory.

These checks establish native catalog/bounds initialization and persistence. They
do not establish player-contact pickup feel, exact v20 fade visibility, mounted
muzzle fidelity or full gameplay acceptance. No visible window, GPU render, audio
device/playback, OS input, original-file write or child agent was used.
