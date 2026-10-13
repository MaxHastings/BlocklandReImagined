# 2026-10-12 LAN and Internet hosts list Add-Ons from the kept hashes

Finding 3 of the load-path waste audit (`perf/waste-audit-2-load-paths.md`
in the project files), on top of PR #95. Every Host listed its Add-Ons for
joiners' downloads (`PackageShelf::new` -> `Listing::of`) by reading and
SHA-256ing every file of every Add-On, the same bytes #95's
`Environment::load` now answers from its length+mtime cache. `Listing::of`
now hashes through that cache (`environment::kept_file_hashes`); the host's
content check has just hashed those files, so the listing reads nothing
that did not change. A joiner checking a download still reads every byte.
Single Player builds no shelf at all (PR #97).

Checks: new test `kept_file_hashes_answer_unchanged_files_without_reading_them`;
`cargo test -p bri-package`, `bri-net` `package_sync` and
`crash_rejoin_journey`, clippy `-D warnings` on bri-package and bri-net.
