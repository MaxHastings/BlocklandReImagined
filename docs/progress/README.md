# Progress entries

From 2026-10-01 each progress entry is its own file in this folder, so two
threads never edit the same file and progress notes stop causing merge
conflicts. [../progress.md](../progress.md) keeps the earlier dated history.
Read [../STATUS.md](../STATUS.md) for current release state and open work.

- Name a new entry `YYYY-MM-DD-short-topic.md` (the date it lands, a few
  lowercase words joined by `-`), for example
  `2026-10-01-portal-big-size.md`. Add a branch or lane word if two
  entries could share a name.
- Start it with a `# YYYY-MM-DD Title` heading, then the same content a
  progress.md section had: what changed and why, decisions, the commands
  run and their evidence, failures, and what is next.
- Never edit another thread's entry to add your own news; write a new one.
  Fixing a wrong fact in an old entry is fine.
- An entry written into progress.md on a branch that started before this
  folder existed still merges: progress.md is only appended to. New work
  goes here.

`ls docs/progress` sorted by name is the history in date order.
