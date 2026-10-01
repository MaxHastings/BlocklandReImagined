# Host rules follow their Add-On in every list (2026-10-01)

Max's build b5d99c948 had all nine `-rules` folders installed, but its
`packages.json` turned on only `tool_duplicator-rules`. The release packager
had listed that one because the Duplicator starts on. The other originals
were turned on in the Add-Ons screen before their rules existed, and nothing
re-read their `companions` afterwards. The repair in `defaults::update_lists`
only runs in a source checkout, never in a release. So Fill Can, Grapple Rope,
HookShot, Throwing, Trench, the New Duplicator and Slayer/CTF loaded without
their rules.

Fix: `library::follow_companions`, run by `PackageSet::load_root`. Every
loader goes through it: the client, the dedicated server, the Add-Ons screen
and the tools.
- Each listed Add-On's installed companions are on right after it, moved
  after it if they were listed before it.
- A companion whose Add-On is installed but not on is dropped.
- Companions are found beside the Add-On by manifest id, and each folder is
  read once.
- `Library::scan` drops a companion from `packages-disabled.json` once its
  Add-On's list turned it on, so nothing is listed twice.
- Turning an Add-On off in the game already takes its rules with it (they
  depend on it).

Max's current install heals on its next start, with no file edits.

Guard: `library::tests::every_on_add_on_loads_its_companions_whatever_wrote_the_list`
uses four made-up imports and an existing list with no rules on three of them.
It also covers one rules entry listed before its Add-On, one rules entry on
without its Add-On, and one rules entry in the disabled list while its Add-On
is on. It checks the load order, the Add-Ons screen, and off/on round trips.
It fails without the `load_root` call.
