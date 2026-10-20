> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-minigame-set-favs.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: Create Mini-Game Set Favs state (97b5fc1)

Branch claude/fix-minigame-set-favs, one commit 97b5fc13ddf6c21dc660568291f499e59e55fd46 on 1ba65f1c (ancestor of origin/main). UI only.

Verdict: PASS. Merge 97b5fc13ddf6c21dc660568291f499e59e55fd46.

- Set Favs state now lives on the screen (`setting_favs`), as AvatarGui does (avatar.rs ~340); the helper text only follows it, and starts hidden. This is the right owner for the state whatever the layout, not a patch.
- ClickFav saves on the flag and turns it off after saving; a failed form read keeps it on (status shows the error), like before.
- The test runs with and without the helper control; minigame_screens here: 33 passed, 1 ignored (34 counting the ignored one).

Caveat (cause not confirmed): the old code only failed if the layout has no CMG_FavsHelper or it starts visible. Views take visibility from the imported layout (view.rs:397). If the PC check shows CMG_FavsHelper exists in content/ui_pack's CreateMiniGameGui and starts hidden, this change does not explain Max's missing minigame_favorites, and the cause is still elsewhere (for example, the settings save path). Merge this regardless; it is correct on its own.
