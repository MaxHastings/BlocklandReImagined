# Add-Ons: in-game mod management

Status: built 2026-09-28.
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
| One screen listing what is installed, on/off | `bri_package::library`: `packages.json` = on, `packages-disabled.json` = off, unlisted manifests = discovered (off) | Start Game → Add-Ons (as in v20, not the main menu): grouped list, Enabled box, Defaults | **built** |
| Know what each one is | Manifest `name`, `description`, `authors`, `license`, `provenance`, `provides` | Details: what it adds, credits, source | **built** |
| Know where it runs | `packages.json` `side` | "Only on the server you host", "Everyone in the game", "Just you" | **built** |
| Know what it may do | Manifest `capabilities` (checked by the package runtime) | "Allowed to: change the world's bricks, send chat messages" | **built** |
| Dependencies handled | `Library::plan`: enabling pulls in dependencies first; disabling takes dependents | Notice "Also turned on: …"; a confirm box before turning off what others need | **built** |
| Plain-word errors; one broken add-on does not break the rest | `library.*` diagnostics per package (missing folder, unreadable manifest, newer API, missing or wrong-version dependency, role conflict) | `!` in the list, "Won't load:" in details, refusals in a message box | **built** (the loaders' partial load is the Add-On import thread's multi-pack work) |
| Joining a modded server just works | PR #1: `bri_net::packages::fetch_missing` into the download cache | Join screen: what the server needs, progress, Cancel; never changes your own Add-Ons | **partial**: the client fetches missing Add-Ons on join without asking, and the loading screen shows a Downloading Packages stage; the join screen with Cancel exists in `bri-ui` but the client does not drive it yet |
| Joins never fail over Add-Ons | The server refuses a first join whose `shared` packages differ, naming them (`environment::refusal`); the client fetches what the server offers and joins again with `accept_differences`, and the server lets it in and says what it still lacks | Nothing to do; the server tells the player anything it could not provide | **built** |
| Pick a game mode when starting a game | Packages that provide a `mode` (and `world`) | Start Game → Game Mode (`crates/ui/src/screens/modes.rs`) | **built** |
| Import an old add-on zip | Drop folder `content/Add-Ons/` (v20's name), the only place the game looks: it never searches for a Blockland install. Opening Add-Ons converts what is new or changed there (`bri_package::classic::plan`, records in `content/classic-imports.json`) by running `bri-import-addon` as a separate program into `content/addons/<name>`, with the folder as `--reference` so a required Add-On is found beside it; one taken out is removed with its companion rules | "Converting" group with a progress mark, "Could Not Convert" with the reason and Retry, an Add-Ons Folder button and a line saying what the folder is for | **built** |
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
  provide is a server-only kind (`behaviour`, `script`, `world`, `entity`),
  to `client` when everything is a client-only kind (`model`, `hud`), and
  otherwise to `shared` (the strict choice: a mismatch makes the joiner
  fetch the host's copy rather than desync). A package mixing server and client kinds cannot
  load on either side, so it is marked broken (`library.mixed_sides`) with a
  hint to split it.
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

## Importing old add-ons

Players drop old Blockland zips or folders into `content/Add-Ons/`, as they
did in v20. The library lists each one (`LegacyAddOn`) and matches it to an
installed package whose `provenance.source` starts with
`Blockland Add-On <name> (`, which is what `bri-import-addon` writes. The
Add-Ons screen shows unmatched ones under "Not Imported Yet" with an Import
button. The client runs `bri-import-addon <zip> content/addons/<name> --json`
(`Library::import_dir` picks a fresh folder) as a child process on a worker
thread, so conversion tooling stays out of the game's dependency graph and the
game keeps running. A failed import removes its half-written folder. The
imported package is then discovered, starts off, and turns on like any other.

The packaged build ships `bri-import-addon.exe` next to `bri-client.exe`
(`tools/package_playtest.ps1`); without it Import says the importer is missing. Imports run without a v20
reference install, so references to base datablocks are reported rather than
resolved. The importer writes `provides`, so imported packages group by what
they add.

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
- One click on a row's check box (the list's `checkColumn`,
  `EventKind::Toggle`) turns it on or off; the box ticks at once and goes
  back if the host refuses. `SetAddOnEnabled` and `DefaultAddOns` only
  write the lists; the screen sends `ApplyAddOns` as it closes, which loads
  the new list once (bricks, weapons, maps, modes). Loading on every click
  was the lag players saw between a click and its tick.

## Join wiring

- The host and client load `packages.json`. A join fetches missing Add-Ons
  through `bri_net::packages::fetch_missing_pinned`
  (`crates/net/src/client.rs`). The client drives
  `ConnectionState::DownloadingPackages` with byte progress and Cancel, then
  resumes the same pinned join after verified package preparation.
- **Trust prompt.** When a server's Add-Ons include sandboxed client code the
  player has not trusted, the client asks before that code runs. Accepting
  remembers exactly what the question showed (`crates/client/src/client_code.rs`);
  the Add-Ons screen can forget every server's trust. The question comes
  after the join, not as a separate connection state before the download.
- No wire format changed in this slice. The refusal text is still the wire
  carrier for differing packages; `refusal` and `parse_refusal` sit side by
  side in `bri_package::environment` so they cannot drift.

## Tests (no v20 content needed)

- `cargo test -p bri-package library`: discovery and derived side, dependency
  order on enable, cascading disable, refusals (missing, wrong version, base,
  role conflict), broken and newer packages reported, disk unchanged on
  refusal.
- `cargo test -p bri-ui --lib addons`: grouping and marks, toggle requests,
  confirm-before-cascade, locked base, search, details text, the Add-Ons
  button under Start Game (not the main menu) and routing, join progress and Cancel, Import instead of
  Enabled for old add-ons.
- `cargo test -p bri-client --lib add_ons`: rows, words, toggling through
  files, Defaults, old add-ons offered for import, missing importer named.
- End to end (manual, 2026-09-28): `Weapon_Synthetic_Blaster` from the
  importer's CC0 fixture dropped in `Add-Ons/`, imported through
  `add_ons::start_import` with the built `bri-import-addon`, listed as
  "Synthetic Blaster", then turned on; `packages.json` gained
  `{"id":"weapon_synthetic_blaster",...,"dir":"addons/weapon_synthetic_blaster"}`.
