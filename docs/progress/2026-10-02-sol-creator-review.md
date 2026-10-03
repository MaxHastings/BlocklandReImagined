# 2026-10-02 Sol High creator and authority review

Reviewed existing native creator controls and every Workshop recipe, preserving
all shared dirty GUI patches. Added active SuperAdmin/selected-player and
Workshop captures with real native UI assets, and expanded the Wrench fixture
to 1024×768/1920×1080 requested 1×/2×. Both fixtures use the real UI preference
clamp: 1024 requested 2× becomes 1.6× (640×480 logical). A forced undersized
canvas initially clipped Admin, and a fixture omitted the options needed to
enable Host Options; both were corrected as fixture inputs, not runtime bugs.

Final representative captures show familiar compact ordinary event rows,
WHEN/Delay → IF/AND → DO for guarded rows, visible Send/Cancel, and usable
active Admin controls. No superficial restyle or prose tutorial was added.
The existing async typed-team-name regression already increments the listing
revision; it passes for both valid and temporarily empty text. All nine recipes
have practical purposes. Soccer distinguishes two identical Steel Balls via
their authored spawner identity. Real generated catalogs confirm Shark/Zombie
hole bot identities and ordinary panel geometry; existing ordinary-control
filtering excludes them. Source review found classic Door panel skips the tall
selection/lift branch, unlike Gate panel; sent that defect to root for recipe
repair within its ownership.

Commands: real-pack `cargo test -p bri-ui --lib offscreen -- --include-ignored
--nocapture` passes 6; `cargo test -p bri-ui --lib --test minigame_screens
--test admin_screens` passes 209, ignores 8; `cargo clippy -p bri-ui
--all-targets -- -D warnings` passes; touched-path diff whitespace passes.
Full capture scope, recipe limits and artifact locations are in
[the GUI audit](../audits/gui-style-review.md#populated-creator-and-authority-review--v021).
No visible game/input. Maxwell retains interactive acceptance. This UI source
is frozen for integration; the overall alpha/release remains incomplete.
