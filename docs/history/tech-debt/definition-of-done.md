> **Historical note. Written 2026-10-01 (the file's date in the cloud project folder, `tech-debt/definition-of-done.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# v0.1.11 definition of done

The combined list posted to Max in the project chat (2026-10-01 03:12Z).
It replaces v0.1.11-definition-of-done.md.

1. **All lanes landed.** Every lane is merged through the Gate, or Max dropped it.
2. **Originals are real.** They're imported and ported from Max's copies and
   credited in CREDITS.md, with nothing third-party in the repo. Duplicator,
   Stunt Plane and Mirror are on; the rest are installed but off. Every port
   is complete; an unfinished one holds the release.
3. **Each bundled Add-On has a test** proving its main job works: a gun fires,
   a tool works, a mode scores.
4. **Cleanup lands last, together with the app.rs split.** The tech debt items:
   - Progress notes are one file each in `docs/progress/`.
   - The protocol version counts `crates/net/protocol-changes/`.
   - `docs/architecture/seams.md` and the Rust 1.93.1 pin. These three are on
     branch claude/project-thread-9ttn9s and land after batch153.
   - Script ops live one per module, with one union-merged list line each.
   - New content goes through `HostSetup`.
   - One default Add-On list.
   - The modding guide is split by topic, with its op reference generated.
   - The session.rs re-exports have one item per line.
   - One rustfmt pass, then a CI fmt check.
   - Shared test constructors, and `Default` on public option structs.
   - Content-free fixtures for the cloud-skipped test targets, where feasible.
   - Importer ports as one folder each (`ports/<name>/port.json`, discovered),
     replacing the shared ports.json and tests/ports.rs every port lane edits.
   - README protocol and hook tables replaced: protocol change files and a
     generated op reference.
5. **Known duplicates have one source each.** Item looks, portal links, Add-On
   explosions and vehicle cameras.
6. **Green with no excuses.** The full check on Max's PC passes, with no flaky
   or skipped tests and no test reading his real folders. The 700 saves are
   never run.
7. **Speed holds.** Kitchen, Slate, the 1M city and join time are no worse
   than v0.1.10 (46 ms, 85 ms, 9 ms, 0.9 s).
8. **Windows, Mac and Linux all build.**
9. **Short release notes**, checked by the Gate against git log.

When it's done it publishes straight away (Max, 03:14Z).

## Scope: everything (Max, 03:17Z: "wait for everything for this release")

No cuts and no "ships if ready". The release waits for all of it:
- Duplicator, Trench Digging, Player Throwing, Fill Can, and bundling the
  originals into the download.
- Everything in batch 153.
- Tier Tactical's 26 Add-Ons, the whole Adventure Pack, New Duplicator, and
  Slayer with Capture the Flag. Every bundled original's port is complete and
  applied on the real copy, including any gaps against the original.
- The Bedroom light bulb probe and the parked performance round
  (claude/project-thread-cw15he d5c44dc85, which needs its protocol renumbered
  as a change file).
- Cleanup and the app.rs split, last.
