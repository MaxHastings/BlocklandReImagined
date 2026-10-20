> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-capabilities-rebase.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Capabilities rebase: d233e7fc (claude/project-thread-uoh3ap-rebased) on fighting 608cf674

**Verdict: PASS. Merge d233e7fc845741ae8ca5baddbc92d6e9b4586982 after 608cf674.**

## (2) Nothing beyond the reviewed commits
I compared the old stack (5fd6cfaf, without its merge of fighting b6280a24)
with the new one (608cf674..d233e7fc), pair by pair, using
`git patch-id --stable`.
- 11 commits each, in the same order, with the same subjects.
- 10 of 11 have identical patches.
- The one that differs is 70bf9274, the "WIP timed throws" commit (was
  3e5ae24c). Apart from hunk offsets, its only difference is in spots.rs:
  `&self.weapons.pack.projectiles` added to the two
  `hand_combat::capability` calls. Its old version patched the one call that
  existed at b6280a24.
- The merge commit is gone, as stated.

## (1) spots.rs resolution
`git diff 608cf674 d233e7fc -- spots.rs` is exactly two lines, the projectiles
argument on:
- `bot_best_attack` (the new spot bound), and
- `bot_capability`'s `of`.

Everything else is fighting's file unchanged.

The bound stays admissible on the merged code:
- `bot_best_attack` reads each slot through the same `capability()` the
  chooser uses, now including timed deliveries and the stock-tools rule.
- `worth` for a timed throw uses the same damage formula, and its seconds
  only grow (fuse and flight on top of the cadence).

## Not done by me
No build here; the PC gate covers that. The two call sites match the four-argument
signature that the stack's other callers already use.
