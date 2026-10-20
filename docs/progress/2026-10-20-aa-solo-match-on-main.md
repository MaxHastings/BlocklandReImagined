# 2026-10-20 AA solo match on main

Branch `claude/project-thread-t3kkxz` (engine) rebased onto main a8395b2e (map look, materials, collision/audio,
scatter, lighting, decals, terrain lights). It holds three commits only: the capture harness steps
(`--panel`, `--report`), bots climbing ladders and opening map doors the rules open (`Mode::Climb`, `bots_open`),
and the nav fall-law drop plus the `nav_route` example. No copy of any seam work from the cleanup PRs.

Live offscreen check on the rebuilt client with the regenerated AA2 content (addons branch, same name):
- both teams of eight spawn at their starts and play; bots wound and kill each other (one body dead, one at 30 health in 150 s);
- iron sights: right mouse shows the original peep sight, release returns to the hip view with the arms and rifle;
- the human walks from the assault start to the compound fence and stops at it (map collision), as the bots' routes assume;
- team/position window shows and Deploy starts the round countdown.

Open: AA-art team/role menu (panels have no images; needs a HUD-layout menu with decoded AA menu textures), objective
letters on the compass (needs a HUD compass marker element), run-time emitters/projectors (cleanup thread landing).
Death is silent in the source (AGP_Pawn.Died, PlayDying), so no death sound is a fact, not a gap.
Scratch content roots built from older packs lack the v20 packs, so stock add-ons list as problems there; a bootstrapped
content folder has them.
