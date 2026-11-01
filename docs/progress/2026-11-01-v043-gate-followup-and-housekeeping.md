# Gate follow-up and housekeeping

All four ignored `default_add_ons` tests passed in the isolated gate worktree
after preparing the current package bytes at the stock paths; its `content`
junction was used as the fallback after an attempted relative `BRI_CONTENT`
resolution from that worktree failed. The imported-weapon hosted test and a
native debris-content test also passed against that same prepared content.
The source installation and release package selection were not changed.

Filed #419 for the opened-door stall, #420 for the TDM scenario's very low
fight activity and below-world actor, #421 for the reproducible deck-edge
falls, and #422 for the synthetic mirror/free-camera failures. These remain
open evidence, not claims of diagnosed causes. Commented on #400, #412–#416
with candidate findings and remaining limits; #417 was closed as not planned
under Maxwell's culling-first decision.

Removed 40 remote branches whose exact current tips were verified merged into
origin/main and which had no open PR or checked-out worktree. A first atomic
push caught two stale remote references and deleted nothing; after fetch/prune,
the refreshed and leased deletion succeeded. `python tools/clean_targets.py
--apply --keep v043-final-gate --keep v043-integration` freed 0.2 GB of old
worktree target directories; main and active outputs were retained.

Old v0.4.3 preview folders 1 and 2 were inventoried and their logs and modified
settings preserved under the private `v043-preview-logs` evidence directory.
An automatic policy check rejected their permanent removal, so they remain on
disk (about 0.82 GiB each); no alternate destructive removal was attempted.
Preview 3 remains available while release validation is unfinished.
