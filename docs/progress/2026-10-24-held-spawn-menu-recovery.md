# 2026-10-24 Recover server menus across loading and held spawns

A server panel received before map preparation finishes opened on Loading.
SetContent(Play) then removed its dialog, leaving the player without the menu.
Retain the latest panel while loading and open it after Play replaces that
content. Disconnect clears it so another session cannot inherit the old menu.
The retained UI regression fails before and passes after; all five panel cases
pass without synthesizing a user close during loading.

The assembled private consumer exposed a second failure: Escape closed the
team panel, but F3 could not reopen it before deployment. Authoritative state
held the first spawn and assigned the game's player type, while the replica
retained a default archetype-0 pose. Controls incorrectly followed that pose.
Use the living body, or the declared game/team player type while waiting/dead.

Camera control has no held item/body for HUD claims; it still belongs to its
minigame. Resolve those claims normally instead of clearing every package HUD
and restoring unwanted stock name tags/building prompts. Admin camera commands
retain their ordinary permission checks.

An ordinary offscreen App/host regression fails on the original control path,
then passes in 75.23s: host/menu, Escape/F3 before deployment, Join/class/Deploy,
F8, F3 while flying, F7, suicide/held next life, F3 while dead, disconnect cleanup.
No OS input, visible game window, position fixture or rule bypass is used.
Private test and exact before/after logs stay in the consumer repository and
interdiction-ui-lighting-20261024 evidence archive. Quick UI/client checks pass
(72s). Focused checks pass (96s), including strict all-target clippy and UI
integration tests. Full gate/Windows PR checks and main integration follow;
full UI appearance is separate.
