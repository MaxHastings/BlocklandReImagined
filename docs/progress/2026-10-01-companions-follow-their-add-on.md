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

## End-to-end guard

`crates/addon-import/tests/bundled_in_game.rs` runs in the gate. It needs no
content and uses the made-up base game from `bri_net::testing` plus the CC0
stand-ins.

1. Every bundled original that has a stand-in goes through the release path.
   `addon_bundle.py build` and `install` put it into a game folder. A new
   original whose port has host rules but no stand-in fails the test.
2. The originals get turned on two ways: by a `packages.json` written before
   their rules existed (Max's case), and in the Add-Ons screen.
3. `load_root` must list each original's rules right after it.
   `bri_net::dedicated::load` must load every rules package with no package
   diagnostics.
4. A real client connects over loopback QUIC to a host started like
   `bri-server`, from the old list:
   - The Hookshot pulls the player to the floor it strikes.
   - The Grapple Rope holds the player: turning and walking away moves them
     under 3/4 as far as the same walk off the rope.
   - `/fillcan` (a rules command) puts the Fill Can in hand. A picked colour
     keeps it there, and firing at a planted brick fills it with that colour.

Both tests fail without the `load_root` fix: the list check fails, and the
Hookshot never pulls. They pass 3/3 with it, at about 34 s.
