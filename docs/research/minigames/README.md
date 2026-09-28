# Vanilla minigame evidence and native implementation

Completed isolated rules module on 2026-09-26. Original installation remained
read-only; no originals executed, game windows opened, OS input sent or audio played.
Runtime: `crates/minigames`. Read-only converter: `import_catalog.py`. Latest local
native pack: `content/minigames-pack-002`. Native schema 1, rule ID
`v20.minigame.rules.1`. Stable player/item IDs share existing content conventions.

## Source map

Line numbers below refer to recovered
`.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs` unless stated otherwise.
The recovered core evidence is tied to the designated primary reference's unchanged
DSO/resources by `docs/vanilla-reference.md`; the converter hashes original server,
client and game DSOs and recovered input separately. No legacy source is included
in the runtime or this tracked audit. Hashes and catalog declarations are in
`source-inventory.json`; complete derived fields remain in the ignored native pack.

| Evidence | Source location | Implemented consequence |
| --- | --- | --- |
| Timer constants | 2840–2850 | Respawn 1–30 s; vehicles 0–300 s; bricks 2–300 s; items4 s; public join throttle5 s. |
| Manual observer trigger | around 6980 | Main trigger after strict elapsed `>` death timer; outside admin exception. |
| Default create UI | `.research/bl-decompiled/v20/client/scripts/allClientScripts.cs:1407–1488` | Actual player-facing defaults: public, all damage flags enabled; building/paint enabled; wand off; owner bricks/spawn bricks; kill+1/self−1; timers1/5/30 s; Hammer/Wrench/Printer/Gun/Rocket. |
| UI data block selection | same client file1490–1530 | Every ItemData/PlayerData with nonempty uiName selectable, including vehicle PlayerData. |
| Constructor vs settings send | core21942–21985; client1646,1727 | Core temporary timers1 ms, scores0, invite-only and damage off are overwritten by GUI send. Native create atomically uses supplied final settings. |
| List/join/leave/remove | 21399–21482 | Color/title/owner list, loading-phase gate, public join cooldown, owner kick, owner leave ends game. |
| Invite/accept/reject/ignore | 21483–21600 | Owner only, loaded outside target, one pending invite, ignore stable owner; accept bypasses public join cooldown; no source expiry. |
| Ten reserved colors | 21600–21659 | Unique colors; native uses authored integer RGB, avoiding inconsistent source hex strings (128 vs0x88). |
| Create/end/configure/reset commands | 21660–21941 | Owner-only, every stock setting represented; invalid IDs/ranges rejected. |
| Add/remove/end | 21987–22258 | Zero score, membership/name/HUD state, cleanup/respawn, owner alive end heals in place; other members respawn. |
| Reset and RespawnAll | 22259–22386 | Owner or owner-brick event context; reset throttle5 s; owner/all-member brick vehicle+item reset; clear events/objects; score reset only for Reset. |
| Equipment/player/build/paint updates | 22387–22500 | Explicit equipment/remount, type/avatar/energy/dismount, ghost/brick inventory and spray unmount contracts. |
| MiniGame event outputs | registrations18383–18387; implementations22501–22608 | ChatMsgAll, CenterPrintAll, BottomPrintAll, Reset, RespawnAll with owner-brick/member authorization. |
| Spawn selection | 22639–22710 | Owner group, each member's own group or uniform total member spawn bricks; fallback map. |
| CanUse | 22733–22814 | Tri-state outside; loose-item exception; owner and own-brick conditions; explicit LAN behavior. |
| CanDamage | 22815–22959 | Weapon/self/vehicle/brick flags; same-game controlled players; ownership gate for objects; use-own-bricks does not restrict damage. |
| Object membership | 22961 onward | Distinct controlled player/client, projectile source client, explicit object membership and spawn-brick owner. |
| Radius self check | around8236 | Additional selfDamage check independent of relaxed LAN helper. |
| Falling/impact damage | around9174 | Fall flag distinct from weaponDamage; host retains admin-wand immunity/minimum impact threshold. |
| Scoring/death/spawn | `.research/bl-decompiled/v20/server/scripts/game.cs:589–624`, around780 | Suicide only kill-self points; environmental die points; killer/victim points; five starting tools. Dead code after unconditional return624 is not executed. |
| Brick plant/break score | core16597,16683,17103 | Commit-only score callbacks. |
| Wheeled/AIPlayer/flying respawn | core18887–18904,9283–9294,18972–18983 | Outside delay0; wheeled source-game delay max burn+100 ms, AIPlayer source-game base, flying destroyed-object game base. |
| Sports package | primary `Item_Sports.zip!support.cs`, `sportBallsPackage` | Sports tools cleared; slot0 StartBall image, on-spawn mount, 50 ms config update if hands empty; native sports behavior remains weapons responsibility. |

No finite-lives counter or setting exists in the scanned core minigame, GUI,
death/respawn or stock add-on paths. Original global `EndGameScore=0` and unused
game-duration scaffolding are not minigame life/round limits. Root explicitly
confirmed `Lives::Unlimited` as faithful baseline. No extension was invented.

## Catalog and importer

```powershell
python docs/research/minigames/import_catalog.py --reference 'E:/Downloads/B4v21Launcher/versions/Blockland v20' --core .research/v20-dso/server/scripts/allGameScripts-Vanilla.cs --client .research/bl-decompiled/v20/client/scripts/allClientScripts.cs --output content/minigames-pack-003
```

Choose a fresh output directory; the converter refuses to overwrite. It reads
bounded core and stock Player/Item/Weapon/Vehicle archive scripts, strips comments,
extracts literal PlayerData/ItemData declarations, resolves inheritance, and exports
nonempty uiName entries. It does not execute functions or interpret a VM. Duplicate
declarations and unresolved parents are reported. Pack002: **11 player types,
21 items, zero diagnostics**, 83 provenance source entries. Catalog SHA-256:
`0ef2807a0390a9f63410704816e3ceba76d18d4a1976939fe97c3bd491642090`.
Pack001 has byte-identical catalog; pack002 adds original DSO provenance.

Selectable player types are Standard, Sport, Horse, Fuel/Jump/Leap/No-Jet,
Quake-Like, CannonTurret, RowBoatArmor and TankTurretPlayer. Hidden sports
BallShootPlayer and PlayerSportTurboArmor have empty uiName and are correctly
excluded from minigame selection. Their existence remains relevant to their owning
player/weapons subsystem. Catalog membership is evidence of source selection,
not proof those host controllers/models are already integrated.

## Deliberate native decisions and limits

* Commands atomically apply full typed settings; source used sequential string
  tokens and sometimes respawned mid-update. Native effects apply final settings.
* Invalid settings/IDs fail explicitly instead of source's silent fallback or
  integer-index coercion. Five slots are bounded; host UI clamps original ranges.
* First join/reset is permitted immediately. Original zero-initialized timestamps
  accidentally gated first use until server uptime5 s. Subsequent cooldown is5 s.
* Stale generations/rounds/lives fail closed; same source account after reconnect
  cannot receive an old projectile's authority. Delayed old-round effects cannot
  score. Membership changes clean source objects through explicit host effects.
* End emits cleanup/ejection consistently, including owner end, where original end
  callback had fewer direct cleanup calls than leave. This prevents retained event
  objects from carrying ended-game authority.
* All settings are active as rules or explicit host effects; no inert toggle.
  Rules return `OutsideMinigames` rather than interpreting Torque's −1 as boolean.
* Original LAN behavior is an explicit host option, never automatic authority from
  a transport type. Radius self damage's separate callback check is preserved.
* Integer RGB values take precedence over inconsistent original name-color hex;
  counters use saturating i64; bound exhaustion returns an error.
* Stable sorted spawn IDs replace source object enumeration. Uniform random words
  select equally weighted bricks; host supplies deterministic PRNG and collision.
* Manual respawn quantization preserves strict `>` at 120 Hz. Source may be polled
  on a different engine timestep; feel must be checked after host binding.
* Outside brick respawn helper uses default30 s only; server preference overrides
  and vehicle authored destruction/burn scheduling remain host responsibilities.
* Full save restores active IDs only with matching trusted host session bindings.
  Reconnects are always freshly allocated; a serialized BL_ID is not authentication.

The source recovery remains an evidence limitation for engine-side behavior and
dynamic package order, not a claim of executing original semantics. Literal
catalog scan has no unresolved references for current source. Runtime/menu/player/
weapons/event/vehicle/network adapters and Maxwell's interactive feel validation
remain required before alpha acceptance. No acceptance checkbox was marked.

## Verification record

20 headless integration tests pass and Clippy all-targets with `-D warnings` passes.
The release `catalog_smoke` processes every selectable type/item, eight sessions in
two minigames, 12,000 ticks (100 simulated seconds), 671,200 damage checks, 400 effects
and a validated save/restore. Initial measured rules loop33.8352 ms on AMD Ryzen7
7800X3D / Windows / rustc1.93.1; this excludes build, IO, physics, rendering and
network work and is not the full eight-client acceptance benchmark.

Reproduction commands are in the crate README. Logs, CPU/toolchain metadata and
smoke results are under ignored `artifacts/native-minigames`. The source
installation was only read.
