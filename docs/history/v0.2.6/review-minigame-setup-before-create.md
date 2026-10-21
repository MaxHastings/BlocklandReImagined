> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-minigame-setup-before-create.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: mini-game Setup/Teams before Create (dd0a4b3)

Branch claude/fix-minigame-setup-before-create, one commit dd0a4b3 on 1ba65f1c
(an ancestor of origin/main d068f06e; the request said ab8f7f02, also an ancestor; either way it merges clean as UI-only).

Verdict: FAIL, one must-fix. Otherwise sound.

## Must-fix: the draft lets a non-host owner set host/admin-only settings, then the whole draft is rejected
- `AddOnSettings::locked` (minigame_addons.rs:615) only looks at `addon_locked` for `self.game`; in draft mode `game` is None, so nothing is locked and every setting is editable.
- The host's `edit_settings` (sim session/packages/settings.rs ~560) refuses any setting above the editor's level ("You may not change ...") and is all-or-nothing.
- Slayer declares settings with editor "host" (8) and "admin" (3) (crates/addon-import/ports). So a normal player making a game on someone's server can pick, say, a host-only value in Setup; after Create, the send fails and ALL their Setup and Teams choices are silently lost (inferred: the error lands in `minigames.status` after the Create window has closed; not run end to end).
- Fix (small, same mechanism the window already uses): client publishes the keys the local player could not change in a game of their own (minigame_ui.rs:388 already computes `addon_locked` with `creator = owner`; compute the same list with creator = local player) and `locked()` uses it when `draft`. Add a test: non-host draft greys a host-only setting; Create sends no locked key.

## Checked and fine
- Pending for Create is removed before `on_result` (ui.rs ~1852), so the draft goes out at once if the listing already shows the game, else on the next MiniGames update.
- `draft` is only true from the Create window path; Admin menu sets `server_addon_settings`, bind screen sets a game.
- Changed-from-default filtering matches the host's keys; team edits with id None replace the (empty) new game's team list; reset+quiet = Save & Reset.
- Reset/End/Save & Reset/Tell players/Players hidden in draft.
- Closing Create without creating drops the draft.
- `cargo test -p bri-ui --test minigame_screens`: 35 passed, 1 ignored (cloud).

## Not required (v0.2.7 or never)
- `minigame_draft_created` stays true if the game vanishes before the listing names it (end/disconnect in that gap); the next owned game would then receive the stale draft. Rare; clearing both on disconnect / when the listing shows no owned game would close it.
- Real GUI not checked visually (Max's rule: check it on the PC before merge).

## Re-review e59b354 (delta from dd0a4b3): PASS. Merge e59b354604438c51c24d57499356bb8d505d725c (claude/fix-minigame-setup-before-create head).
- `addon_locked_new` (client minigame_ui.rs ~403) uses the same `rank.allows(editor, creator)` rule as `addon_locked`, with creator = local player; keys via `AddOnSetting::key`, same as `addon_locked`.
- `locked()` uses it in draft; `send_minigame_draft` also drops locked keys for game and team settings, so a loaded favourite can't carry one.
- Draft and created flag cleared on session reset (closes the stale-draft note).
- New test a_guests_setup_draft_greys_and_never_sends_host_only_settings; minigame_screens 36 passed, 1 ignored here.
- Still owed before merge: a visual check of the real window on the PC.
