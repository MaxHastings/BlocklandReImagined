# 2026-10-12 Default builds no longer report their stock prints as missing

Max loaded default builds in v0.2.10 and got "Content this server does not
have" listing stock prints (`print/2x2f/vent x6`, `print/Letters/A`, ...),
with the list running straight into "Install or enable them".

## Cause

Nothing was missing. The P2-04 notice (2026-10-11) counted every print a
placed brick holds as `ContentRef::Unresolved`. Saves keep their print names
exactly as written (`2x2f/vent`, `Letters/A`, older texture paths) and the
client binds them to the stock bundle through its aliases, so every print in
an imported v20 save is unresolved by design and was reported. The message box
also lost its line breaks: the client ran the server's text through
`plain_chat`, which drops every control character, `\n` included.

## Evidence

Checked on Max's PC against his installed v0.2.10 content
(`brick-materials-002`, `stock-catalog-004`, `worlds-pass-006`): all 35
default builds, every print name (resolved by ID, alias or legacy alias, as
the client does) and every brick definition resolve. 0 missing in all 35.
`Print_2x2f_Default.zip` in the v20 reference holds `prints/vent.png`, and the
bundle carries it as `print/print_2x2f_default/vent` with alias `2x2f/vent`.
No content from the Steam install is needed.

## What changed

- `ToolCatalog` gains `print_aliases` (lowercase alias -> stable print ID,
  from the materials bundle; validated to name only catalog prints) and
  `has_print`, which resolves a save's print name by ID, alias or
  `legacy_print_alias`. Not on the wire or in saves.
- `unresolved_prints` takes an `installed` check; the build loader passes
  `has_print`, and `NOPRINT` is never missing. Add-On prints not installed
  here are still listed.
- The client keeps `\n` in server message boxes and questions
  (`plain_lines`); markup and other control characters are still removed.

Tests: `tool_catalog::tests::saves_print_names_resolve_through_aliases`,
`missing_content_groups_bricks_by_addon_and_lists_prints_apart` (installed
and `NOPRINT` prints are not counted),
`app::tests::message_box_text_keeps_line_breaks_and_drops_markup`.
