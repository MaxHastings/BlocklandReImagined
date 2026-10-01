# Update check follows release version numbers

Max, on the published v0.1.11-alpha, still saw "New Version Available:
v0.1.11-alpha" on the main menu.

Cause: the build is stamped `v0.1.11` (`BRI_VERSION`) but the GitHub tag is
`v0.1.11-alpha`. `crates/client/src/updates.rs` treated the two names as
different releases (it only knew an `alpha-` prefix), and the commit-date
fallback did not help because the release was published after its commit.

Fix: `newer()` now compares release version numbers only, through one
`version_number()` (the numbers in the name, in order). Every spelling of a
release is equal (`v0.1.11`, `v0.1.11-alpha`, `0.1.11`) and only a higher
number is announced. Commit dates and hashes no longer play any part (Max:
players download releases, not commits), so `BRI_BUILD_COMMITTED` and the
date parser are gone.

Evidence: `cargo test -p bri-client --lib updates` passes;
`only_a_higher_release_version_is_newer` asserts `v0.1.11-alpha` against
`v0.1.11` is not newer, which the old code reported as newer.
