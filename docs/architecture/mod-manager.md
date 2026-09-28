# Add-Ons: in-game mod management

Status: first slice built 2026-09-28 (branch `claude/mod-manager-n90clz`).
Player research: [`docs/research/mod-manager-expectations.md`](../research/mod-manager-expectations.md).
Package format: [`packages.md`](packages.md).

## One word for players: Add-Ons

There is one concept. Players see **Add-Ons** everywhere: menus, the join
screen, errors and player-facing docs. **Package** is the internal word in
code, formats and developer docs. An old Blockland v20 zip is not a second
kind of thing: "Import Add-On" converts it into an Add-On
(`bri-import-addon`). No player-facing text says "package".

## What players expect, mapped onto our package system

| Expectation | Mechanism (engine) | Player sees | State |
|---|---|---|---|
| One screen listing what is installed, on/off | `bri_package::library`: `packages.json` = on, `packages-disabled.json` = off, unlisted manifests = discovered (off) | Main menu → Add-Ons: grouped list, Enabled box, Defaults | **built** |
| Know what each one is | Manifest `name`, `description`, `authors`, `license`, `provenance`, `provides` | Details: what it adds, credits, source | **built** |
| Know where it runs | `packages.json` `side` | "Only on the server you host", "Everyone in the game", "Just you" | **built** |
| Know what it may do | Manifest `capabilities` (checked by the package runtime) | "Allowed to: change the world's bricks, send chat messages" | **built** |
| Dependencies handled | `Library::plan`: enabling pulls in dependencies first; disabling takes dependents | Notice "Also turned on: …"; a confirm box before turning off what others need | **built** |
| Plain-word errors; one broken add-on does not break the rest | `library.*` diagnostics per package (missing folder, unreadable manifest, newer API, missing or wrong-version dependency, role conflict) | `!` in the list, "Won't load:" in details, refusals in a message box | **built** (the loaders' partial load is the Add-On import thread's multi-pack work) |
| Joining a modded server just works | PR #1: `bri_net::packages::fetch_missing` into the download cache | Join screen: what the server needs, progress, Cancel; never changes your own Add-Ons | **screen built**; wiring waits for PR #1 and door-closers' join |
| Know why a join was refused | Main's join check (protocol 31) refuses differing `shared` packages with `environment::refusal`; `parse_refusal` reads it back package by package | Can't Join dialog: each add-on with the server's version beside yours, and an Add-Ons button | **built** |
| Pick a game mode when starting a game | Packages that provide `world` or a mode kind | Start Game offers them beside maps | next |
| Import an old add-on zip | `bri_addon_import::import` | "Import Add-On…" button, report shown in details | next |
| Presets / profiles | Named copies of the enabled list | Preset picker on the Add-Ons screen | later |
| Per-add-on settings | Package `slots` and settings schema | Settings tab in details | later |
| Browse and update in game | Needs a hosted index | Browse tab | later |

## Mechanism: the library (`crates/package/src/library.rs`)

The engine owns the mechanism; the screen only shows it.

- **Enabled** is exactly `packages.json`, the list the host and client load.
  Nothing else needs to change for a loader: it reads `packages.json` as it
  always has.
- **Disabled** packages keep their exact entry (side, role, directory) in
  `packages-disabled.json` beside it, same schema, so turning one back on
  restores it unchanged.
- **Discovered** packages are directories under the content root, up to
  three levels deep, holding a `package.json` manifest that neither list
  names. They start off. Their side defaults to `server` when everything they
  provide is a server-only kind (`behaviour`, `script`, `world`), otherwise
  `shared` (the strict choice: a mismatch refuses the join rather than
  desyncing).
- **Base game** packages (reserved `v20-*` ids) are always on and cannot be
  turned off from the library.
- `Library::plan(id, enable)` returns what else changes and why a change is
  refused, without touching disk. `Library::apply` rewrites both lists
  atomically (temp file and rename; the disabled list first, so a failure
  between the two leaves a package listed twice and treated as on, never
  lost), then rescans.
- The manifest is read leniently for display only (`PackageInfo`). Checking a
  manifest is the package runtime's job; one the library cannot read at all
  is `library.manifest`.

Diagnostic codes: `library.missing_dir`, `library.manifest`,
`library.manifest_id`, `library.version`, `library.api`,
`library.dependency` (enabled package whose dependency is not on or not
satisfied; a warning on disabled ones), `library.dependency_missing`,
`library.dependency_version`, `library.role_conflict`, `library.required`,
`library.unknown`, `library.listed_twice`, `library.duplicate`,
`library.too_many`.

## Presentation

- **Host adapter** `crates/client/src/add_ons.rs` turns the library into
  `bri_ui::api::AddOnRow`s: groups by provided kind (Game Modes & Worlds,
  Weapons & Items, Bricks, Vehicles, Gameplay, Looks Sounds & HUD, Other),
  words for sides and capabilities, and one locked "Blockland v20" row for
  all base packages. Unknown kinds and capabilities still show, by their
  raw names.
- **UI** `crates/ui/src/screens/addons.rs`: native dialogs in v20 profiles
  (no v20 layout had them). `UiAction::RequestAddOns`,
  `SetAddOnEnabled { id, enabled }`, `DefaultAddOns`; answered with
  `UiUpdate::AddOns(AddOnsView)`. The join screen is
  `ConnectionState::DownloadingPackages(PackageDownload)`.
- Changes apply the next time a game starts; the screen says so.

## What is not wired yet

- On main nothing reads `packages.json` yet, so toggles take effect when the
  Add-On import thread's multi-pack loading and door-closers' join wiring land
  (both load through `packages.json`).
- The client does not yet call PR #1's `fetch_missing` on a join mismatch; that
  call should drive `ConnectionState::DownloadingPackages` with one row per
  missing package, `Cached` for ones already in the cache, and byte progress
  from `Stage::DownloadingPackages`.
- **Trust prompt (planned, with sandboxed client code).** When a server's
  Add-Ons include client code (WebAssembly or shaders), the join stops
  before any download with a prompt that names the server, lists what the
  code may do in plain words, and offers Trust or Leave, remembered per
  server. The hook is a connection state between the package comparison and
  `DownloadingPackages`: `ConnectionState::TrustServer { server, add_ons,
  permissions }`, answered by a `UiAction` carrying the choice (Leave is the
  existing `CancelConnect`). Servers whose Add-Ons are data only never see
  it. The Add-Ons screen's details would also say when an Add-On carries
  client code. The sandbox design thread owns the permission list and the
  wire signal; this screen only presents it.
- No wire format changed in this slice. The refusal text is still the wire
  carrier for differing packages; `refusal` and `parse_refusal` sit side by
  side in `bri_package::environment` so they cannot drift.

## Tests (no v20 content needed)

- `cargo test -p bri-package library`: discovery and derived side, dependency
  order on enable, cascading disable, refusals (missing, wrong version, base,
  role conflict), broken and newer packages reported, disk unchanged on
  refusal.
- `cargo test -p bri-ui --lib addons`: grouping and marks, toggle requests,
  confirm-before-cascade, locked base, search, details text, main menu
  button placement and routing, join progress and Cancel.
- `cargo test -p bri-client --lib add_ons`: rows, words, toggling through
  files, Defaults.
