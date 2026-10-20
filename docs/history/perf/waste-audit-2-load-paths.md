> **Historical note. Written 2026-10-12 (the file's date in the cloud project folder, `perf/waste-audit-2-load-paths.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Wasted-work audit 2: loading, hosting, menus and map changes

Date 2026-10-12, main at `f4ce0ae` (v0.2.10). Thread "Wasted-work audit" (round 2), started after PR #95 (every Host and Join re-read and SHA-256'd all ~563 MB of content).

## Scope

The v0.2.10 audit (`waste-audit-v0.2.10.md`) covered per-frame and per-tick hot loops. This round covers the other class, the one PR #95 belonged to: whole-dataset work redone on a common **event** (launch, Host, Join, Change Map, opening a menu, changing a setting, saving or loading a build) when its inputs did not change. Code was read at `f4ce0ae`. Costs come from Max's v0.2.10 session logs (`E:\Downloads\BlocklandReImagined-v0.2.10-windows\logs`, 9 sessions on 2026-10-12, RTX 4070 SUPER, DX12 + DXC) where a log line measures them. Otherwise they are estimates and are marked that way.

## What Max's logs show (v0.2.10, every session)

| Moment | Log evidence | Cost |
|---|---|---|
| Launch | `Startup: content loaded at` 2.6-2.7 s warm, 10.2 s and 24.0 s cold | per launch |
| Launch, in the background | `Compiled scene pipelines in` 1.45-1.9 s | per launch, off the frame thread |
| First Host of a run | not logged: the "Loading Add-Ons..." step before LOADING MAP (finding 2). The Add-On health warnings print a second time just before `LOADING MAP`. | about one more startup content load |
| First Host of a run | `LOADING MAP` 1.36-1.48 s warm (23.9 s and 27.0 s cold, PR #95) | per Host |
| **First in-game frame of the first Host of a run** | `Long frame: 3085-3612 ms ... draw (record passes 1006-1136, Add-On draw setup 219-300, ...)` in **all 7** sessions that hosted | **3.1-3.6 s freeze, every launch** |
| Second Host in the same run | worst frame 162 ms | none |
| Change Map | `LOADING GRAPHICS` 516-568 ms, then a 212-218 ms frame (`chunk uploads 63, pipelines wait 39`) | per map change |

## Ranked findings

Ranked by time lost multiplied by how often a player hits it. "Fixed in PR" means the draft PR from this thread.

### 1. Every launch's first game freezes for 3 s on its first frame, compiling shaders on the frame thread (measured, fixed in PR for the main part)

- **What happens:** the startup worker (`app/mod.rs:1594`) compiles only the Classic world pipelines. Unified and Dynamic lighting draw with 12 more world pipelines from `scene.wgsl`. They were created lazily in `SceneRenderer::update_view` (`render/src/scene.rs:3627-3657`, comment: "pipelines cost nothing until first use"), on the first in-game frame, in no named span. That is the ~1.6 s of the frame that no span accounts for.
- **More on the same frame:**
  - The AO exclusion mask's 2 pipelines (`scene.rs:4866`), inside "record passes".
  - `EnvironmentProbe::new` (2 pipelines, `render.rs:704`).
  - `Reflections::new` again, because the first light-volume upload resets it (`render.rs:126-130`).
  - The Add-On GPU calibration (`client_code.rs:533`): one pipeline plus a blocking `device.poll(Wait)`. That is the 219-300 ms "Add-On draw setup".
  - The first kept-shadow layer draw.
- **Why it is per launch:** wgpu has no pipeline cache on DX12. After the first compile, the process-wide DXC cache makes the same pipelines take ~51 ms, which is why the second Host is fine.
- **Fix in PR:** `SceneRenderer::prepare_lighting_pipelines` builds the soft-shading and AO-mask pipelines on the startup worker. The loading screen already waits for that worker (`scene_pipelines_ready`), so the compile is absorbed by the menu. The startup worker grows from ~1.45 s to about 3 s, still behind the main menu.
- **Left for later:**
  - Move the Add-On calibration and the probe onto a worker, worth ~0.3 s more.
  - Stop the reflections reset on the first upload.
- **Measure:** the `Long frame` line after `IN GAME` on the first Host of a run, and `Compiled scene pipelines in N ms` at startup.

### 2. The first Host (or Tutorial) after launch reloads every Add-On and all client content that startup just loaded (fixed in PR)

- **Cause:** `App::load` left `packages_from_tools: false` (`app/load.rs:305`) after loading `PackageSet::load_root`. So the first `HostGame` (`app/actions.rs:213-240`) runs `queue_package_reload` → `PreparedPackages::load` (`app/addons.rs:672-747`).
- **What that repeats:**
  - `ClientContent::load_leaving_out_broken`
  - `ContentParts::build` (item, vehicle, debris and explosion assets, read and hashed)
  - `AvatarAssets`
  - both package catalogs
  - client code and sounds
  - the Add-On health check
  - the effects renderer rebuild
  - restarting old-saves conversion
- **When:** the same thing happens the first time the Add-Ons screen is closed with nothing changed (`screens/addons.rs` `on_sleep` → `ApplyAddOns`).
- **Cost:** about one startup content load (2.6 s warm per the `Startup: content loaded` line). It is unlogged because it runs before the loading screen.
- **Fix:** set the flag at the end of startup when no Add-On was left out (the left-out path already sets it through `apply_packages`). A test is in `app/addons.rs`.
- Found independently by two of the sweeps.

### 3. Every Host reads and SHA-256s every Add-On file again for the joiners' download shelf (partly fixed in PR)

- **Where:** `app/session.rs:648` `PackageShelf::new` → `net/src/packages.rs:76` `Listing::of` → `package/src/sync.rs:139` `hash_file`.
- **What it touches:** every file of every enabled client-side Add-On, about 563 MB minus the base packs. These are the same bytes `Environment::load` hashes. PR #95's cache does not cover this path.
- **When:** every Host, including Single Player, which nobody can join. It sits in the STARTING SERVER stage (`CONNECTING (after N ms)`, ~300 ms warm).
- **In PR:** Single Player skips the shelf.
- **Follow-up after #95 merges:** give `Listing::of` the same length+mtime cache, or list lazily on the first download request, for LAN and Internet hosts.

### 4. Weapon and item-physics content identity is re-read and SHA-256'd per Host (twice), per Join and per Change Map

- **Where:** `net/src/content_identity.rs:167-236` and `:297` (`hash_files_bounded`), `:649`.
- **How often:** Host runs it at `session.rs:454` and again through `paths.map_content()` (`session.rs:477` → `content.rs:358`). Join runs it at `session.rs:896-898`. Change Map runs it at `content.rs:415`.
- **Why it is waste:** its only use is `ensure_same` against the snapshot startup already holds.
- **Fix:** small. Pass the startup snapshot in, and reuse PR #95's stat cache in `hash_files_bounded`.
- **Measure:** inside LOADING MAP; there is no own log line.

### 5. Map-independent content is rebuilt per Host, Join and Change Map, and the map itself is parsed 2-3 times

All of this is inside the 1.4 s warm LOADING MAP and the Join map preparation.

- **Brick definitions:** `Definitions::load_with_geometry` (`net/src/map_content.rs:104-108`, `client/src/app/mod.rs:405`) reads every brick mesh file and builds a collider for each of ~963 definitions. On a local host this happens twice per Change Map: once on the host, once on the client. `definitions.rs:283-288` also does a linear `.find` per catalog entry, which is quadratic.
- **Brick materials:** `BrickMaterials::load` (`session.rs:493`, `app/mod.rs:420`, `materials.rs:149-170`) re-reads, SHA-256s and PNG-decodes every surface and print image.
- **Map parsing:** `bundle.json` is parsed 3 times, the scene JSON 3 times and each interior JSON twice per Host or Join (`content.rs:440/829`, `sim/map.rs:156-185`, `render/scene_loader.rs:202-234`).
- **Re-validation:** content that startup already validated is validated again: `load_bundle`, `validate_catalog` and `load_ui_schema` (`content.rs:440-465`, ~1000 `canonicalize` calls).
- **Fix:** small for each. Cache `Arc<Definitions>` and `Arc<BrickMaterials>` per content set, and share the host's copy with the local client. Skip what was validated at startup. Sharing the map parse is structural.
- **Estimate:** a few hundred ms to over 1 s of the 1.4 s.

### 6. Change Map restarts the whole GPU side (measured)

- **Where:** `take_prepared_scene` (`app/net_events.rs:840-877`) takes the device and calls `gpu_ready`. That rebuilds:
  - every renderer and pipeline set, including the avatar preview;
  - reflections, weather, AO and line renderers;
  - the effects atlas.

  It also drops every item, vehicle and avatar mesh and every chunk, and uploads them again.
- **Why it is waste:** only the per-map sky, terrain and probe state needs to change. The device, format and samples are unchanged.
- **Log:** LOADING GRAPHICS 516-568 ms per map change, plus a ~215 ms frame.
- **Fix:** structural-light. Keep the renderers and add a per-map reset.
- **Decision (2026-10-12): deliberately skipped.** A closer read showed most of the 0.5 s is needed work: the new map's chunks, terrain and sky have to upload anyway. Only about 0.1 s comes from rebuilding renderers that could be kept, and a per-map reset risks drawing the old map's state after a teardown. Revisit only if a timing breakdown shows more avoidable time.

### 7. Every save list reads every native save file in full to show its header

- **Where:** `client/src/saves.rs:179-197`. `read_header` reads up to `MAX_BUILD_BYTES` of each file; only the head is decoded (`world/src/build.rs:100-118`).
- **When it runs:**
  - every Load or Save Bricks dialog open;
  - after every save (`saves.rs:536`);
  - inside every load (`Store::load` lists again just to find the entry, `saves.rs:352-360`);
  - after each `.bls` conversion while the dialog is open.
- **On Max's PC:** about 43 MB of native saves, so the full 43 MB is read per open, plus the chosen file again on load.
- **Fix:** small. Read only the header prefix, or cache entries by (path, length, mtime).
- **Related:** `old_saves.rs:398-403` rewrites `index.json` on every list open even when nothing changed.

### 8. A manual save validates the build 4 times and encodes it twice, then re-lists every save

- **Where:**
  - `sim/src/session.rs:957-963`: capture validates, then validate runs again.
  - `net/src/server.rs:486-495`: the reply is MessagePack + zstd, with bricks unpacked.
  - `client/src/saves.rs:385-386`: validates again, then `build::encode` validates a fourth time and re-packs and recompresses.
  - Then finding 7's listing.
- **Cost:** off the tick since PR #81, but at 200k bricks it is plausibly several hundred ms of latency per save.
- **Fix:** small. The host replies with the native file bytes and the client writes them. Drop the duplicate validates.

### 9. Loading a build through a local host unpacks and re-packs the whole build

- **Path:** file (packed) → `decode` (unpack) → `wire::Upload::take` (pack) → zstd → host decompress → `Upload::restore` (unpack). See `build.rs:127-139`, `protocol.rs:345-351`, `wire.rs:57-70` and `server.rs:992-995`.
- **Fix:** medium. Forward the packed body.
- **Measure:** `brick_load_bench` part 2.

### 10. Every joiner gets a fresh whole-world encode

- **Where:** `server.rs:1599-1606` → `protocol.rs:746-820` sorts and clones every brick by distance from the joiner, then MessagePack + zstd. A Change Map transfer is correctly encoded once for everyone.
- **Estimate:** 100-300 ms CPU per join at 200k bricks.
- **Fix:** structural. Cache encoded neighbourhood frames.

### 11. Client chunks are rebuilt several times while a build load or a join streams in

- **Where:** `app/net_events.rs:309-390` starts a chunk job whenever the world changed. Each streamed slice touches chunks that were already built, and cover neighbours rebuild as the front advances.
- **Estimate:** plausibly 2-5x the meshing work.
- **Fix:** small. Coalesce chunk jobs while a load or join is streaming.
- **Measure:** `chunk_jobs` and `chunks_rebuilt` (`app/probes.rs:22-23`) against the brick count.

### Smaller (each confirmed in code unless marked)

- **Zoom wheel:** each notch saves `settings.json` with two `fsync`s and rebuilds graphics, audio and weather from settings on the frame thread (`ui/src/ui.rs:1365-1376` → `app/actions.rs:331-341`). Debounce it.
- **Add-Ons screen:** opening it scans the whole library 3 times (`actions.rs:939-944`, `add_ons.rs:411-412`, `frame.rs:730`), and each toggle scans 3 times (`add_on_choices.rs:140-143`). Plausibly 50-150 ms with ~300 Add-Ons.
- **One broken enabled Add-On:** makes every launch run the whole content load twice (`app/load.rs:370-376`, `apply_packages(&requested)` repeats the leave-out loop).
- **MSAA, shadow quality or colour-vision change:** `gpu_ready` drops every mesh, texture and effects atlas, although only pipelines and shadow maps depend on these settings (`render.rs:1524-1583`). Plausible cost.
- **Lighting mode switch:** reloads the map bundle from disk (`lighting.rs:276-300`).
- **Compatibility lighting:** every compatibility-lighting map load deep-copies the scene (`lighting.rs:187`) and builds a BVH and SHA-256s all triangles and textures before the disk cache lookup (`light_volume.rs:382-466`). This is off the frame thread.
- **Mini-game reset:** scans the whole world once per member for owned vehicles and respawns vehicles twice (`minigames/src/lib.rs:418-460`, `combat.rs:2135-2176`, `vehicles.rs:1592-1605`). Estimated 30-60 ms on the tick with 32 members at 200k bricks.
- **Item icons:** every icon is re-uploaded each time one more icon finishes drawing (`item_ui.rs:137-160`). This happens only on an icon-cache miss.
- **Recovery snapshot:** validates 3 times per write (`net/src/recovery.rs:43-46`), off-thread.
- **Build-load start:** makes 5 linear passes on the tick (`session/build_load.rs:142-215`).
- **Clear Brick Group:** makes a second whole-world scan (`admin.rs:599-602`).
- **Clear All Bricks:** sends one removal per brick (`admin.rs:566-581`). This is rare.
- **Add-On vehicle assets:** re-hashed per Host (`vehicles/schema.rs:935`).
- **Join that downloads Add-Ons:** does the content work twice. This is expected; only the duplicate hashing is waste.

## Checked and already cached (do not re-audit)

- **Downloads:**
  - A joiner's downloaded packages are trusted by a seal file.
  - Package downloads are served by byte range with no per-request re-zip or re-hash.
  - The download shelf is built once per Host, not per joiner.
- **Rendering and lighting caches:**
  - Item icons are kept on disk by content hash.
  - Light-volume and map-lighting bakes are cached on disk; only the key computation repeats.
  - A graphics option change keeps the world uploaded (`adopt_scenes`).
- **Conversion and imports:**
  - Old `.bls` conversion is keyed by length, mtime and converter fingerprint.
  - Add-On ZIP import is stamped, with no re-conversion per launch.
- **Startup and menus:**
  - UI images decode lazily and system fonts load once.
  - The update check runs once per launch.
  - The Options screen saves only on Done; volume changes do not save.
  - No UI screen touches the filesystem per frame.
- **Network and saves:**
  - Resume sends only the backlog.
  - The Change Map transfer is encoded once for all peers.
  - Copy, paste and undo are budgeted per tick.
  - Paint fill is bounded.
  - `Covers::hidden` on a 64x64 baseplate is under a millisecond.
  - The 128 MB `Reply::Saved` decode is on reader threads.
  - Recovery snapshots are revision-gated and off-thread.

## Method

- **Code sweeps:** three read-only sweeps in parallel (Host/Join path; startup, menus and settings; in-session load, save and join events), plus a focused trace of the first-frame freeze.
- **Verification:** every line reference in the top findings was re-read on `f4ce0ae`.
- **Measurements:** none were new. The numbers come from Max's own session logs, read in place.
