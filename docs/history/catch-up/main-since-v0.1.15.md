> **Historical note. Written 2026-10-04 (the file's date in the cloud project folder, `catch-up/main-since-v0.1.15.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# What changed on main since v0.1.15 (e05af0df) — read-only catch-up

Main is now 0eebedb8 (43 commits, ~430 files). It is one docs commit past the
v0.2.2 release source (e3c9010f). Nothing was changed, pushed or branched.

## Big picture
- Three releases: v0.2.0, v0.2.1, v0.2.2. All now ship for Windows, Mac (Apple
  silicon) and Linux. Main is the only branch the other tool keeps.
- Rule Workshop: a menu for building game rules (conditions, variables, scoring,
  regions) with nine ready-made recipes. Detection regions are sized directly in
  the Wrench and show outlines while you hold building tools.
- Bots: they now read a game's goals (from its rules) and try to complete them
  with normal controls. They crew vehicles together, push objects, and pick
  weapons and tactics more sensibly. Unknown Add-On mechanics stay unsupported
  unless declared (docs/modding/bot-objectives.md).
- Creator menus: Wrench and MiniGame menus reorganised around common tasks; the
  colorset is chosen before hosting.
- Lighting: a new "Dynamic" lighting mode with live lamps; Classic and Unified
  stay as before.
- Shark: now properly ported (random colours, Cool Shark, swimming, mouth grab
  with the 5-second kill). Zombie still partial.
- Smaller things: JPEG screenshots, guest music filtering, longer debris on
  blasts, Add-Ons reload in the background, effects refresh after reload.
- Still open per docs/STATUS.md: Windows firefight crash (unreproduced),
  performance coverage, full alpha contract.

## The five earlier reports
(a) Shark join error — FIXED for the normal case (commit a87daa8e,
    docs/progress/2026-10-02-five-reported-bugs.md). Shark now borrows Bot_Hole's
    hole shape at load time. Gap: it only finds that shape when Bot_Hole is ALSO
    enabled (crates/net/src/content_identity.rs brick_geometry_assets reads
    enabled packages only), and the test enables all three
    (crates/client/tests/default_add_ons.rs:626). Shark on + Bot_Hole off is
    still untested and probably still fails.
(b) Doors — FIXED (a87daa8e). Cause was what we suspected: open doors are wider
    (4x7/4x9 vs 4x1) and a same-size guard silently refused every click. Swaps now
    resize, and play the original brick-change click on open and close.
(c) Bedroom foliage / Slopes distant water at night — FIXED (a87daa8e). Both now
    dim with the time of day; covered by render tests.
(d) Spray can "Color is outside palette" after Change Map — FIXED (a87daa8e plus
    e0dedc7 colorset fixes). The new map's palette wasn't sent after a map change;
    the HUD also clamps the chosen colour if the palette shrinks.
(e) Add-Ons folder next to the exe + conversion status screen — NOT DONE. The
    "Open Add-Ons folder" button still opens content/Add-Ons
    (crates/client/src/app/actions.rs:948). Only a clearer error if opening fails
    was added.

## Conflict with our leftover branch
claude/gravity-gun-rework-ainb6j (c9adcfab) is still on GitHub, the only branch
besides main. It already has main merged in, so it applies cleanly, but it fixes
(a) a different way: the importer copies the borrowed hole shape into the Shark
package itself. Main's fix borrows it at load time instead. Having both would be
two mechanisms for one job. The branch's approach is the one that would cover
the "Bot_Hole off" gap. Options: drop the branch, or rework it on top of main
so Shark carries its own copy and main's load-time path stays for other cases.
