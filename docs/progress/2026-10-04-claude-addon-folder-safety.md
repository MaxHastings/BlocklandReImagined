# 2026-10-04 Add-Ons folder safety: bundled originals, re-conversion, running games

Branch `fix/addon-folder-safety` from origin/main 53f05cf (v0.2.3). Fixes
three review findings in the classic Add-Ons folder sync
(`bri_package::classic`, `bri_package::library`, `bri-client` `add_ons.rs`)
plus one latent bug found while writing the guards.

## Findings and changes

1. **[S1] A dropped copy of a bundled original could delete it.**
   `library::legacy` matched a dropped zip to any installed package whose
   `provenance.source` starts `Blockland Add-On <name> (`, which the 88
   bundled originals at `addons/<id>` also carry. The sync then adopted the
   bundled package as the zip's conversion, replaced it when the zip changed
   and uninstalled it (and its `-rules`) when the zip was removed.
   - Source of truth: the manifest's `provenance.bundled`, which only
     `tools/addon_bundle.py` writes (`PackageInfo::bundled`). It travels with
     the package, so it holds for every root a release or bootstrap
     installs, whatever the list or release manifest says.
   - `LegacyAddOn` gains `included` (the shipped package it copies);
     `imported_as` now only ever names a player's conversion.
   - `classic::plan` never emits Import/Adopt for an included copy; it emits
     a new `Step::Included` once (remembered in `Record.included`), and the
     game says in plain words: "Bot_Shark is already included: Shark Bot
     comes with the game, so the copy in your Add-Ons folder is not needed."
     The shipped Add-On's own row says the same; no "Converting" row.
   - Removal requires `!library.shipped(id)`, so a record an earlier build
     wrote (adopting a bundled package) only drops the record.
   - `Library::uninstall` refuses shipped packages and their host rules
     ("... comes with the game, so it is not removed").

2. **[S2] Re-converting an Add-On others need left them off, and a failed
   re-conversion lost the working copy.** `convert` uninstalled the old copy
   (cascading its dependents off) before running the importer and re-enabled
   only the Add-On itself.
   - The importer now writes into `content/.addon-staging/<name>` (never
     discovered, never looked in for host rules). `Library::install_staged`
     moves it in: for the same id, the old folder and its rules are moved
     aside, the new ones moved in (every move undone on failure), the lists'
     entries refreshed from the new manifest (version/side; rules only the
     new copy has follow it via `follow_companions`), and only then the old
     copy deleted. Enabled state does not change, so dependents stay on.
   - A failed import or an unusable staged copy leaves the old package and
     the lists untouched; its record keeps pointing at the old package so
     taking the zip out still removes it.
   - A new copy that became a different id is handled as before (old one
     removed with what needed it, new one on if the old was).

3. **[S3] A background sync could change enabled packages under a running
   host.** Chosen to match the existing reload gating:
   - Hosting/joining (`HostGame`, `JoinServer`, `TrustNewServerIdentity`,
     `StartTutorial`) asked for while the sync runs is parked
     (`AddOns.after_sync`, "Converting Add-Ons…" on the connection screen)
     and resumed through `queue_package_reload` when the sync finishes, as
     it already waits for loading. A cancel while parked starts nothing.
   - A sync started while a game runs (`net.attempt`) holds back steps that
     touch Add-Ons that are on (new copy of one on, or one on taken out;
     `add_ons::outside_a_game`) and says which wait "until you leave the game
     and open Add-Ons again". `add_ons_changed` no longer tries (and fails)
     to reload during a game; it says "Changes apply the next time you start
     a game."

4. **Latent: removing or re-converting a conversion whose host rules are on
   always failed.** `uninstall` turned the `-rules` companion off on its own,
   which `Library::plan` refuses ("is part of ... and turns on and off with
   it"), so the sync only logged a warning and kept the package forever, and
   every re-conversion of a port with rules failed. Now the rules go off with
   their Add-On (`turn_off_with_rules`).

## Tests (each fails on origin/main, passes on the branch)

`crates/package/tests/addon_folder_safety.rs` (on main with only the test
file added, using only main's API, all four failed):
- `removing_a_dropped_copy_of_a_bundled_add_on_keeps_the_bundled_one`
  (reviewer's scenario, with and without rules): main panicked at
  `addons/bot_shark/package.json` missing.
- `a_changed_copy_of_a_bundled_add_on_never_replaces_it`: main planned
  `Bot_Shark replacing Some("bot_shark")`.
- `an_earlier_builds_record_of_a_bundled_add_on_never_removes_it`: main
  deleted it.
- `removing_a_conversion_with_host_rules_that_are_on_removes_both`: main
  logged "weapon_gun-rules is part of weapon_gun and turns on and off with
  it" and kept the folder.
- New API, passing: `a_new_copy_of_an_add_on_others_need_keeps_them_on`,
  `a_new_copy_that_cannot_go_in_leaves_the_old_one`.

`bri-client` unit tests (main's sources plus the tests, then the branch):
- `add_ons::reconvert_tests::a_new_copy_of_an_add_on_without_host_rules_keeps_what_needs_it_on`:
  main panicked "bot_zombie is still on".
- `add_ons::reconvert_tests::a_new_copy_of_an_add_on_others_need_keeps_them_on_and_a_failed_one_keeps_it`:
  main kept the first copy (re-conversion with rules on failed, finding 4).
  Both also cover a failing import keeping the old package, and removal
  afterwards.
- `app::addons::tests::hosting_waits_for_the_add_ons_folder_to_finish_converting`:
  main started loading for the host while the sync ran.
- New API, passing: `add_ons::in_game_tests::during_a_game_only_changes_to_add_ons_that_are_off_are_made`,
  `add_ons::in_game_tests::a_dropped_copy_of_a_bundled_add_on_is_already_included`.
- `frame.rs` `poll_background_jobs` became `pub(super)` so the app test can
  drive it (also applied for the main run).

Commands (shared target dir, debug info off):

```sh
cargo test -p bri-package                       # 54 + 6 pass
cargo test -p bri-client --lib -- add_ons app::addons   # 20 pass
cargo clippy -p bri-package -p bri-client --all-targets -- -D warnings   # clean
```

`crates/client/tests/night_qa.rs` was updated for `start_sync`'s new
`in_game` argument. The full workspace and the gate were not run here (disk);
the gate runs on push.

## Next

- Maxwell's playtest: drop a bundled original's zip (e.g. Bot_Shark.zip)
  into `Add-Ons/`, open Add-Ons: one "already included" notice, nothing
  converted; remove it: the bundled Add-On stays on. Replace a dropped zip
  that another Add-On needs: both stay on.
