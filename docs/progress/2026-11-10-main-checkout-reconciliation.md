# 2026-11-10 Main checkout reconciliation

Maxwell added reconciliation of the primary checkout's local changes to the
autonomous PR 442 / issue 428 / release goal: incorporate useful material and
remove obsolete or accidental files using long-term judgment.

The primary folder was on `claude/checkpoint-race-jeeps` at `979793613`.
Its local `main` was 1,024 commits behind origin; that branch has exactly one
commit absent from current main, covering jeeps in a race test. It remains
preserved on its existing local/remote branch. It was not silently merged into
this unrelated engine review.

`AGENTS.md` already matched current origin/main. Two October 16 progress files
had no content diff despite Git's modified status (line-ending/index metadata).
The two untracked October 30 release-plan drafts were older than their committed
counterparts: the committed plans contain subsequent approval, implementation
and acceptance instructions absent from those drafts. Three other untracked
progress entries were byte-equivalent to current main after newline normalization.
Those duplicates were superseded by the committed copies.

Historical local research, private consumer checkpoints and design auditions are
preserved verbatim outside the checkout at
`C:/Users/Maxwell/.codex/archives/BlocklandReImagined/2026-11-10-main-checkout`.
`MANIFEST.json` records source branch/head, relative path, size and SHA-256 for
all 84 preserved files; every copy was read back before its original untracked
location was removed. This keeps private, stale working notes from presenting
themselves as current public acceptance. The empty `=` file and `du.exe.stackdump`
are accidental artifacts, also retained in that archive rather than committed.

Durable decisions recovered from those notes remain explicit:

- The October 9 character illustration accepted a visual direction only. It
  did not approve final geometry, customization, a rig or a runtime replacement.
  Source-frame/rig round-trip and later motion acceptance were still required.
- Footstep F was selected as the D/E midpoint, retaining the two-semitone pitch
  reduction and softened attack; avoid reopening that isolated choice. The
  installed source and later public sound receipts remain authoritative for
  gameplay, cadence and contact acceptance.
- The jet audition selected a 50 ms attack / 150 ms release, then Maxwell's own
  authored Desktop mix superseded the earlier generated textures. Sustain-only
  variants did not authorize replacing his attack, EQ or source identity.
  Later committed source/receipts decide what actually shipped.
- The original-reference workflow, separate technical/reference/human acceptance,
  and read-only original installations are already incorporated in current
  `AGENTS.md`. Old private game checkpoints do not establish completed fidelity.
- November 4 reports and pre-release handoffs predate the verified v0.4.4 receipt;
  their historical open/closed statements do not override current known issues
  or imply a new regression.

A zero-byte `.git/index.lock` dated November 4 prevented checkout changes.
There was no active Git process and its timestamp was over a day old. Removed
that verified stale lock, restored the three tracked files from their index,
switched to `main` and fast-forwarded to `e68f1d345`. `git status --short --branch`
then reported only `## main`: the primary checkout is clean and current. Its
build output, generated content, saved games and original installations were
not deleted or rewritten.
