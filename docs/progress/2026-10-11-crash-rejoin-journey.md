# Crash and rejoin journey with Add-On download

Date: 2026-10-11. Lane: engine hardening and test tiers (v0.2.10).

## What was added

`crates/net/tests/crash_rejoin_journey.rs` runs the whole journey headless
over loopback QUIC, with no game window:

1. A host with two shared Add-Ons and one server-only Add-On runs in a child
   process (the same test binary, re-run as `journey_host_child`) and keeps
   its recovery snapshot every 100 ms.
2. Two players with empty caches join through `Client::connect_fetching`,
   download and verify the shared Add-Ons, and build 4 and 2 bricks.
3. Once the snapshot holds all six bricks, the host process is killed
   outright (SIGKILL / TerminateProcess), not stopped.
4. One file of Ben's cached castle is damaged in both places the cache
   keeps it: the installed copy and the stored object.
5. A new host starts from the dead host's snapshot. Both rejoin: Ann
   downloads nothing; Ben downloads exactly the damaged texture again; both
   run exactly the server's Add-Ons; owner numbers, bricks and ownership are
   unchanged.

## Bug found and fixed

The first run failed: Ben rejoined running only `banners`, with no error
shown. `Cache::install` correctly found the damaged object, discarded it and
failed, but `fetch_missing_pinned` then left the castle out ("Left out: ...")
instead of downloading the discarded file. A player with a damaged cache
would join a modded server silently without one of its Add-Ons.

`fetch_missing_pinned` (crates/net/src/packages.rs) now makes one more round
when an install fails and the cache is missing files the listing needs: it
downloads them and installs again. A second failure still leaves the package
out, as before.

## Evidence

- Before the fix: `Error: Ben runs [PackageRef { id: "banners", ... }]`.
- After: `cargo test -p bri-net --test crash_rejoin_journey` passes (about
  0.6 s); `cargo test -p bri-net -p bri-package` passes;
  `cargo clippy -p bri-net -p bri-package --all-targets -- -D warnings` is
  clean.
- The first version of the test hung: libtest prints
  `test journey_host_child ... ` ahead of the child's own output, so the host
  line is now marked `journey-host` and found anywhere in the line.
