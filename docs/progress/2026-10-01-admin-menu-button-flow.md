# Admin menu: native buttons no longer cover the Destructo Wand

v0.1.11 put Add-On Settings at a fixed y=318 in adminGui's right column,
right on top of v20's own Destructo Wand button (Max's screenshot).

Now the native buttons (Environment, Host Options, Passwords, Add-On
Settings) flow down the right column from y=210 with one rule
(`flow_buttons` in `crates/ui/src/screens/admin.rs`): each takes the first
spot clear of every button the layout already has, so v20's Kick, Ban,
Un-Ban, Spy, Destructo Wand, Change Map and Clear Bricks keep their places.
A button that would pass the window's foot starts a new column to the right
and the window widens to hold it. With the stock layout Add-On Settings lands
just under the wand, above Change Map.

Guard: `screens::admin::tests::native_admin_buttons_never_cover_another_button`
(4, 6 and 12 native buttons against v20's right column; no two rects
intersect, none past the foot). Wand rect in the fixture is estimated from
the screenshot; check the real layout on the PC.

Commands: `cargo test -p bri-ui --lib native_admin`, `cargo test -p bri-ui
--test admin_screens` (render test needs a GPU), clippy -D warnings clean.
