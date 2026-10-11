# 2026-10-11 Wrench: reset-only respawn joins the timer row

The "Respawn only on mini-game reset" box sat below the Ray Casting /
Collision / Rendering rows, far from Item Respawn Time, so nothing said it
overrides the timer. It is now an "On reset" box on the Item Respawn
Time row itself: the seconds field shrinks and the box sits to its right;
ticking it greys the field out. The row's Copy box covers both (the lock for
`ItemRestore` resolves to `ItemRespawn`). No wire or save change. Tests:
`item_reset_box_*` in `crates/ui/src/screens/wrench.rs`. The GPU offscreen
wrench test needs a GPU and was not run in the cloud container.
