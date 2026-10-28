# 2026-10-28 Content loading evidence and installation cache isolation

Maxwell targets <=5 seconds for the first map/server load in a fresh process,
<=3 seconds for repeats, measured from selection through playable control.
Those targets are not established for a truly cold content read.

The exact screenshot log is `E:/Downloads/BlocklandReImagined-v0.3.7-windows/logs/session-20261028-163108.log`,
engine b927779dc. Bedroom preparation took 41.453 s: animations 3.230 s,
brick catalogs 10.053 s, world collision 6.346 s, verification 21.399 s reading
540.5 MB.
An earlier session on the same engine (`session-20261028-042037.log`) took
35.266 s; `session-20261028-043111.log` then prepared Bedroom in 2.642 s,
including reading the same 538.4 MB in 1.241 s. Source changes cannot explain
that same-build difference. Cold filesystem reads/scanning/content residency
remain suspects; the logs alone do not establish Defender or a specific I/O
cause. No new scenario or disruptive cache flush was run while Maxwell plays.

Preview 9's actual `session-20261028-163547.log` prepared Desert Slate in
2.381 s (hashing 545.7 MB in 1.205 s). Summing its loading transitions through
IN GAME gives about 2.962 s. It is a different map and does not prove the cold
Bedroom target. Initial GPU calibration/uploads produce a separate first-frame
stall; the phase labels alone do not account for selection-to-responsive-control.

One source-backed bug is corrected: all installations used one per-user hash
cache, but saving that cache prunes files outside the content root seen by the
current process. Switching preview/release roots discards the previous root's
verified entries and forces a full read next time. The actual current cache
contained only preview 9's 12,044 file entries.

Each canonical content root now selects its own hash-cache filename in the
existing per-user state directory. Relative/absolute spellings agree, different
installations cannot evict each other, and all existing size/mtime invalidation,
settling checks, package directory traversal and full download verification stay
in place. No new content schema, wire data, broad cache or trusted digest source.
A cold/new/moved root still requires verification: this fix does not claim a
41-second first load became five seconds.

Validation: all 65 bri-package unit tests passed, including switching roots,
process-style cache reload, equality with full package hashing, canonical aliases
and a same-length edit forcing a reread. Package clippy and package/client format
checks passed. Only the small package crate was rebuilt locally (8.19 s build,
0.96 s tests, two build jobs); no game/GPU/full gate was started. Client entrypoint
build/platform CI accompany the PR. Preview 9 remains the delivered playtest.

Next supported work: capture cold loading I/O/CPU without concurrent builds or
playtesting, retain correct verification, then reduce the dominant cold reads.
The existing per-content map/definition/material caches already cover repeats;
adding another unkeyed cache or weakening content verification is not supported.
