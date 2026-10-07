# Create Mini-Game Set Favs keeps its own state

Max saved his ACM City CTF mini-game to Create Mini-Game favourite slot 2;
his settings.json had no `minigame_favorites`.

What the code does: `MiniGameScreen::favorite` saved a slot only while the
`CMG_FavsHelper` control was visible, and Set Favs only toggled that
control. If the real v20 layout has no control by that name (or it starts
shown), Set Favs never turns on, and the number buttons only load. The
synthetic test layout has the control, so tests passed either way. Not
verified against the real `CreateMiniGameGui` layout from the cloud
container (it needs the generated v20 UI pack).

Fix: Set Favs keeps a `setting_favs` flag on the screen, as AvatarGui does,
and shows the helper text where the layout has one. It starts off.

Also worth knowing: these favourites hold only the vanilla rules (v20's
`MiniGameFavorites/<slot>.cs`). Slayer and CTF settings and teams are saved by
the Favourites row in the Add-On Settings window (`addon_favorites`), which
also keeps the vanilla rules.

Evidence: `set_favs_saves_the_form_to_a_slot_and_the_slot_fills_it_again` now
runs with and without the helper control; without it, it fails on origin/main
and passes with the fix. 34 mini-game screen tests pass; `clippy -p bri-ui
--tests -D warnings` clean.
