# UI assignment scope update — 2026-09-26

Maxwell now requires the complete vanilla v20 content and gameplay set before
alpha handoff. Read `alpha-contract.md` and `vanilla-coverage.md`; these supersede
the earlier event-subset/minigame exclusions in the UI audit and assignment.

For the UI work, include full minigame creation/settings/join/leave/invites/reset/
end, player/host administration and trust, stock Add-Ons/Music selection, all
printer packs, special wrench dialogs and the full typed event catalog. Resource
and event lists must be supplied by engine view models rather than hardcoded to
the previously supported subset. Expose actions and pending/error results for
these workflows. Astra implements their authoritative gameplay systems.

Keep unavailable legacy purchase/authentication/master-server/update services
outside the alpha. Keep the remaining vanilla workflows represented in the
coverage ledger until implemented; a disabled button is not acceptance. Preserve
the agreed crate ownership and Maxwell-only interactive playtesting boundary.
