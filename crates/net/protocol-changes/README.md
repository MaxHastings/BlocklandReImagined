# Protocol changes

Every change to what host and client send each other (a new message, a new
field, a changed meaning) adds **one file here**. Nothing else is edited:
the protocol version is the last hand-numbered version (69) plus the number
of files in this folder, counted when the game is built. Two branches that
each add a file land as two versions, with no shared line to conflict on and
no renumbering.

- Name the file after the change, lowercase words joined by `-`, for example
  `map-light-rules.md`. Add your thread or branch word if the name could be
  taken.
- Write one or two lines: which messages or fields changed and what they
  carry, naming types the way the old list in `src/protocol.rs` does (for
  example `Checkpoint::environment` and `Delta::environment`: the live
  environment).
- Never delete, rename or merge files here: the count would go down and the
  gate refuses that. A change that is undone before release still keeps its
  file; write a new file for the undo.
- Changes up to version 69 are listed above `VERSION` in `src/protocol.rs`.

Host and client must have the same version to play together, so any change
to the wire needs a file, even a small one.
