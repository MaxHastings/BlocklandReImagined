# 2026-10-02 Guard: the worlds pack must bring its save pictures (branch `claude/project-thread-9cbbpg`)

The save-picture fix (437d90ce) did not reach v0.1.12 players: the shipped
worlds pack (`content/worlds-pass-006`) was converted before the fix and
holds no `.jpg`. Nothing failed, so the stale pack shipped silently.

- `world_index` now reads each save's `picture` flag from the pack's
  `report.json`; `None` means the pack predates pictures.
- `content::check_world_pictures` fails when a pack predates pictures, when
  a recorded picture is missing, or when a pack with builds has no picture at
  all, and names the fix (`python tools/bootstrap.py --rebuild worlds`).
- `bri-client --check` runs it and prints `Save pictures: N of M converted
  builds.` The Gate, the release workflows (Windows, Mac, Linux) and
  packaging all run `--check` on the real content, so a stale pack stops the
  release. Content-free CI has no worlds, so it is unaffected.
- Tests: `content::tests::a_worlds_pack_without_its_save_pictures_fails_the_check`
  (synthetic) and, ignored, `every_converted_stock_build_brings_its_save_picture`
  (every stock build in the real pack has its `{sha}.jpg`).

Until the worlds pack is rebuilt, `--check` on an old checkout fails with
that hint; bootstrap rebuilds it, since the converter sources changed.
