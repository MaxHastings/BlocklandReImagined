# Vanilla administration audit and native foundation

This is an isolated, tested foundation for root integration. It does **not** complete the alpha administration requirement: no App screen, transport handler, camera, world mutation or durable host identity service is wired here. No separate Super Admin menu is invented.

## Evidence and provenance

`evidence.json` records full-file SHA-256 values and exact function/GUI locations. `source-index.txt` indexes every recovered role-reference function; a reference does not always mean a permission guard. Regenerate with `python docs/research/admin/audit_sources.py`. This offline tool neither executes Torque nor copies original scripts into the repository.

Citation abbreviations below identify these read-only files:

| Key | File |
| --- | --- |
| S | `.research/v20-dso/server/mainServer.cs` |
| G | `.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs` |
| C | `.research/v20-dso/client/scripts/allClientScripts-Vanilla.cs` |
| U | `.research/v20-dso/client/ui/allClientGuis-Vanilla.gui` |
| D | `.research/bl-decompiled/v20/server/defaults.cs` |

The four DSO inputs for S/G/C/U match the designated E: reference byte for byte. These are decompiled scripts, not the closed v20 engine source; successful recovery does not prove every engine behavior. Loose `base/server/defaults.cs` in that installation is explicitly B4v21-modified: server name, filtering, falling damage, rates, capacity and quotas differ. Native settings use recovered v20 D: name `Blockland Server`, 8 players, 10 bricks/sec, filtering and falling damage enabled, distance 50, the recorded per-player/LAN quotas. Both default-file hashes are retained in evidence; the altered B4v21 file is not silently treated as vanilla.

The earlier UI audit, `docs/research/ui-ux/01-screens-and-flows.md:510`, identified admin/minigame/server configuration as future work. Its general GUI skin/layout evidence remains applicable; this audit supplies authority and workflow details.

## Actual dialogs and controls

| Dialog | Verified layout/actions | Evidence |
| --- | --- | --- |
| Administrator Menu | 311×480 window at (164,0) in 640×480 canvas; player Name/BL_ID list; Kick, Ban, Un-Ban, Spy, Destructo Wand, Change Map, Clear Bricks | U:11325–11598 |
| Administrator Login | Password, Submit, close; field cleared on wake/sleep; SAD sends nonempty password | U:12375–12478; C:1834–1847,9392–9405 |
| Add Ban | Selected victim, reason, days 0–15/hours 0–23/minutes 0–59, Forever, Ban/Cancel; zero duration does not submit | U:11599–11771; C:8947–9026 |
| Un-Ban | Admin, Name, BL_ID, IP, Reason, remaining time; column sorting; confirmation then unban | U:11772–11982; C:9028–9124 |
| Brick Management | BL_ID/Name/# Bricks, Clear, Hilight, Ban, CLEAR ALL, Cancel; confirmation for removal | U:11983–12203; C:9125–9201 |
| Change Map | Stock map-list/selection dialog; server verifies mission running, actual mission file/extension and admin | U:10254–10425; G:4261–4349 |
| Advanced Server Configuration | Local host preferences: port, limits, quotas, falling damage, filter, random color, distance/public-domain timeout, Defaults/Done | U:10426–11324; C:21133–21160 |
| Start Game | Local server name, join/admin/super-admin passwords, player capacity, Add-Ons/Music/configuration workflows | U:5040–5460 |

Opening Admin Menu routes ordinary players to login and opens the menu for admin/local clients (C:9277–9288). LAN hides Ban/Un-Ban with a blocker (C:8827–8837). Clear Bricks in LAN confirms global deletion; otherwise it opens Brick Management (C:8866–8878). The foundation preserves these branches, despite supporting validated native identities independently of the obsolete LAN BL_ID system.

No separate Super Admin dialog, Give Admin/Remove Admin button, remote auto-list editor or remote general-preference command was found in these stock GUI/script files. `Script_AdminGuiEdit` appears in the bad-add-on list, not as a shipped feature to reproduce (C:21195; S:2033). This is evidence about the checked sources, not proof that the closed engine exposes no further console command. Native host-only role/list configuration is labeled an adaptation below. Grant/removal acceptance remains open until integrated and compared with Maxwell's intended stock workflow.

## Roles and permissions

Only `Player`, `Admin`, `SuperAdmin` are represented. Original SA sets both `isAdmin` and `isSuperAdmin`; the native enum guarantees that relationship (S:255–269,355–390). Auto-admin return value 3 means **host acquisition reason**, not a fourth role. C:9407–9456 renders levels 0/1/2 as `-`/`A`/`S`; the fallback numeric display is not evidence of another stock role.

| Operation | Player | Admin | Super Admin | Local host/console | Source |
| --- | --- | --- | --- | --- | --- |
| Password login | Yes | Yes | Yes | Local connection may submit | S:951–1021 |
| Kick | No | Yes | Yes | Yes, target protections still apply | G:3053–3133 |
| Ban/list/unban | No | Yes | Yes | Yes; native persistent identity required | G:3135–3603 |
| Spy, Destructo Wand, map list/change, group highlight/clear, global clear | No | Yes | Yes | Yes | G:3605–3614,4261–4349,4613–4654,20479–20707 |
| Set Admin password | No | No | Yes | Yes | S:1029–1035 |
| Set join/SA password, general settings, direct role/list edits | No verified remote command | No | No verified remote command | Explicit native host-only API | U local prefs; S:355–390 |
| Remove/invite minigame member, end/configure/reset minigame | Owner only | Owner only | Owner only | No implicit admin override | G:21464–21503,21699–21732,21922–21938 |

Kick protects server owner, local connection and SA for human clients; the AI branch bypasses those latter role/local checks after original legacy-ID checks. Ban protects owner and every connected local/SA sharing the victim identity. These protections also apply to the source console route. Admin can kick another Admin; no extra Admin immunity is added. Ban by offline BL_ID is source-supported, including a Brick Management entry, but **offline native subject resolution remains a host integration gap**. The foundation's ban action targets connected, host-registered identities only. It does not treat a client-supplied historical number as authentication.

Passwords: source SA password comparison comes first, then Admin. Entering the Admin password can lower an SA's role. Empty submissions do nothing. `$Game::MaxAdminTries=3` (G:2851), with disconnect when attempts are **greater than** 3 (fourth failed attempt); successes do not reset that counter in the recovered function. The host callback must verify configured credentials and prefer SA on an equal-password match. Actual credential storage/change effects and connection teardown remain host responsibilities. Demo/purchase-key exclusions are not reproduced; obsolete authentication is excluded by contract.

Auto lists: owner preference first, then auto-SA, then auto-Admin (S:355–390). Native lists collapse a principal to one role, preserving SA precedence when imported. Editing an auto list affects later connections; it does not silently demote current sessions. Removing a current role is a separate host action. This foundation keeps owner/local authority as an explicit trusted connection attribute, not as role 3 or a claim sent over the network. Owner/local authority remains available after a password-induced role change; that is the contract's modern host override, documented separately from original script flags.

## Other stock commands and cross-system effects

The typed gameplay effects cover Fetch/Find/Ret (G:4511–4611), Spy/Warp/TimeScale (4613–4700), RealBrickCount (4701–4729), DropPlayerAtCamera/DropCameraAtPlayer (4801–4890), CancelAllEvents (4939–4966), ResetVehicles/ClearVehicles/ClearBots (5033–5134), plus the menu commands above. Source TimeScale clamps to 0.2–2.0; native validation rejects nonfinite values before applying that clamp. Host target existence is checked for connection actions. Map IDs must be bounded native catalog identifiers, not source filesystem paths. Root must additionally resolve the map, mission state, body/camera existence, collision/placement, brick-group identity and deletion lifecycle before performing these effects.

Additional authority interactions remain with their owning systems:

- Loading/reloading bricks and save upload require admin (G:1710–2795). No file reader or upload protocol is added here.
- `/clearBricks` clears one's own group, has a five-second gate and no Admin requirement; `/cancelEvents` has minigame-owner, LAN-admin and five-second conditions (G:4910–4997). They must not be mapped to unrestricted global clear/cancel.
- Minigame actions check owner; admin status alone does not bypass it. `owns_minigame` deliberately encodes only equality. Full minigame policy belongs to that subsystem.
- Chat spam protection exempts admins (S:872–894); trust-invite spam removal exempts admins (G:20883–20923). Player-list badge updates are C:517–555. These policies are audited, not wired by this crate.
- Wrench event restrictions/admin override (G:740–746,1195–1214,10853–10969), brick planting rate exemption (G:5811–5825), admin wand world destruction (G:12921–13095), observer respawn bypass (G:6949–7004) require integration with gameplay, not just UI hiding. The distance check at G:5845–5854 is not bypassed by that rate exemption: zero means 50, then the preference clamps to 20–99,999 for planting. Public-domain timeout uses minutes (S:71–100); native scheduling must preserve it without depending on obsolete server-list posting.
- SA-only DFG/GetPZ/RayPZ (G:5135–5199) and admin diagnostics/icon-generation/preview commands are indexed. Diagnostic/editor tooling is not reproduced here; their existence does not justify inventing a gameplay privilege tier. Any gameplay acceptance implication must be decided by root under the contract, not silently omitted.

## Native API and schemas

Files: `crates/admin/src/lib.rs`, `src/view.rs`, `tests/authority.rs`. The crate is independently buildable, with its own workspace/lock; root adds workspace membership after ownership transfer. No existing UI/network/sim file was edited.

```rust
use bri_admin::*;
let mut admin = Administration::default();
// Host-supplied registration, not deserialized from the client handshake:
admin.connect(TrustedConnection {
    id: ConnectionId(1), display_name: "Host".into(), principal: None,
    is_owner: true, is_local: true, is_bot: false,
}, unix_seconds)?;

// The authenticated transport selects origin; request contains only schema/action.
let request = Request::decode(received_bytes)?;
let effects = admin.handle(
    Origin::Connection(connection_from_transport), request, unix_seconds,
    |password| host_credentials.verify_admin_password(password),
)?;
// Root executes effects, persists BansChanged/AutoRolesChanged, broadcasts roles,
// and calls admin.disconnect(id) during authoritative connection teardown.
```

`Origin` and `TrustedConnection` deliberately lack deserialization. `Request` schema 1 denies unknown fields; there is no acting connection, host flag, account ID or claimed role in it. Target connection IDs increase for the service lifetime, and cannot be reused after disconnect. A stale packet cannot select a replacement client. The caller must never fabricate `Origin::HostConsole` from a packet. Host-owned catalog and world resolution must occur before applying `Gameplay` effects; these effects are not standalone proof that a world action succeeded.

`Principal([u8;32])` is a typed host-verified native identity key, not a cryptographic implementation. If the current host has no secure persistent identity binding, register `None`: kick and password roles work, while persistent ban returns `IdentityUnavailable`, and auto-role grants cannot attach to that connection. Public identity services remain out of scope. Root still needs a local reconnect identity design and verified offline-subject lookup. Do not hash an unverified name/BL_ID and call it verified.

`DurableState` schema 1 contains `next_ban_id`, a ban array and an auto-role array. Records store native principal, stable BanId, victim/admin display names, reason, creation time and optional UTC-seconds expiry (`None` = Forever). This replaces original year/minute arithmetic and index-based unban with monotonic stable IDs. Expiry uses `[created,expires)`; source listing/check functions disagree at exact minute equality, so the native boundary is an explicit numerical adaptation. Original IP-display/BL_ID columns have no fabricated replacement; host UI should label its verified native identity and leave unavailable legacy fields explicit.

One active record per principal; replacing a ban assigns a new ID, so a stale unban cannot remove its replacement. Expired entries are filtered from listings and compacted on the next ban. Restore validates a full candidate before publishing; no connection handles, roles, secrets or failed-login counters are serialized. Restore is intended for startup/quiescent host loading, with request/UI queues refreshed; it does not reevaluate live roles or disconnect newly banned sessions automatically. Root owns file location, atomic staging/rename, durability failure handling and clock source. `write` validates and emits bounded bytes; it does not promise an atomic filesystem commit.

Budgets: 1,024 connected clients; 4,096 bans and auto-role entries each; 4 MiB save input/output; 4 KiB request; 128-byte names/map IDs; 512-byte reasons; 256-byte passwords. Control characters reject; render names/reasons as plain text, never markup/commands. Positive finite duration is capped at 525,600,000 minutes and checked for overflow; this is a native safety bound, not a recovered UI limit. Settings resource caps are native validation bounds, not claimed original limits. Unknown/duplicate IDs, invalid schema/times and oversized input reject. Validation failure leaves durable state unchanged. `Secret` redacts Debug; secrets are not in snapshots/save files, though request serialization necessarily contains submitted credentials and must use root's protected transport/storage path.

`view::ViewModel` derives the seven original Admin buttons, login/menu routing, selected stable IDs, sorting, LAN ban blocker, typed one-shot confirmations, ban duration validation and password field consumption. `BanDraft` preserves the actual dialog ranges. It does not render widgets, invent a fourth role or add a Super Admin menu. Root may present `can_configure_host` through local configuration UI; it is not a remote SA grant. UI properties are not authority: every submitted request is rechecked against current host sessions.

## Verification and open integration

```powershell
python docs/research/admin/audit_sources.py
cargo test --manifest-path crates/admin/Cargo.toml --locked
cargo clippy --manifest-path crates/admin/Cargo.toml --all-targets --locked -- -D warnings
```

All ten tests and all-target Clippy with warnings denied pass. Tests cover all modeled permission families, Admin peer kicks, owner/SA protection including shared principals, forged actor/role fields, disconnected/reused connection IDs, SA-to-Admin password transition, fourth failure, secret redaction, durable expiry/replacement/unban, corrupt-state and capacity/overflow rejection, auto-role separation, original UI actions/confirmation selection and source time-scale limits. Source inventory regeneration verifies four reference DSO identities and indexes 66 role-reference functions. No visible window, gameplay input, audio or original-file write occurred.

Remaining before acceptance: root's authenticated network routing, local persistent identity/offline bans, secret storage, actual UI layout and skins, effect execution, chat/trust/minigame/build authority propagation, disk-save policy, late-join/admin-state replication, permission-change/disconnect cancellation, original map-list throttle and normal workflow tests. Integration must verify denied remote requests as well as allowed actions. The native foundation and source audit do not check off the alpha Admin/Super Admin requirement.
