# 2026-10-28 v0.4.1 extended release corrections

The first combined source was `0eff36e30`; its platform CI passed, but the
extended local release run did not. Main and the public tag remained unchanged.
The 525-binary run took 1,205 seconds and its automatic retries added 666
seconds. Do not call that run a pass.

The coordinator found that main's ignored generated Add-Ons were older than
the refreshed release bundle. It backed up 138 existing generated folders,
installed the verified bundle, and compared all 10,148 shipping files including
generation receipts byte-for-byte. The shipping-content run used those fresh
inputs. Original installations and main's tracked local changes were preserved.
The bundle has 100 imported originals plus the generated NPC Add-On and its
companion; the builder's printed count includes the generated entry.

Failures and supported corrections:

- NPC voice policy called `len()` on an absent per-bot state value. The same
  guard exists in v0.4.0. PR #398 removes the masking fixture initialization,
  reproduces the error before the fix and passes eight strict reaction tests
  afterward, including new bot, damage, death and respawn. Regenerate the NPC
  recipe/companion before shipping; the 118 recording inputs are unchanged.
- Joining a host failed when downloaded client bindings referred to a declared
  server-only companion. The shared bounded manifest helper now supplies
  non-executed metadata for declared dependencies and companions. The real New
  Duplicator's 13 bindings pass, and an undeclared-target negative case still
  fails. It adds no server script download/execution or host authority bypass.
- Resume cursor equality raced a first client that had not consumed a newer
  update. The corrected test deliberately leaves that client behind with a
  chat update, verifies resumed chat and cursor coverage, and retains exact
  worlds plus the two World-frame transfer assertions.
- Seven bundle/in-game/grapple cases selected local `python3` 3.9 rather than
  supported `python` 3.11. PR #399 shares a version-aware selector. All seven
  affected cases and two bundle controls pass with hashing/assertions intact.
- The actual-content pickup-ghost fixture had four starting tools, so dropping
  one Gun did not reserve its assumed two empty slots. Commit `4cf209bbc`
  reserves actual capacity through ordinary controls. All four synthetic and
  native solo/LAN cases pass, preserving ghost, pixel, respawn and immediate
  pickup assertions. Runtime pickup behavior did not change.
- The catalog startup fixture counted script-only hidden items as player
  choices. The existing typed `hidden` contract excludes them from selectors.
  It now checks exact selectable IDs in both UI catalogs and exact physics
  coverage for every item. This replaces a wrong count assertion with the
  authored visibility contract, rather than changing a count to get green.
  Final integrated verification is pending.

The first run also reported the ragdoll soft-cone case and the old resume
assertion as flaky: each failed, then passed alone. Preserve that history.
The resume test is corrected above; the unrelated ragdoll flake is not skipped
or weakened. All final results belong in the publication receipt.

PR #397's standard Cargo command wrapper, cache retention and build diagnostics
were reviewed after its CI passed and are now included. Release optimization,
Thin LTO, executable sets and scheduling remain unchanged. No measured speedup
is claimed. Its Python contracts and the final three native builds verify the
wrapper; future performance comparisons remain separate.

Evidence is under ignored `artifacts/v041-release/` in the coordinator checkout
and each lane's focused logs/PR handoff. Earlier runs stopped for the tank
fixture or stale generated inputs are incomplete, not passing evidence. The
coordinator's cold catalog-only compile was stopped to use the warm integrated
gate after these corrections; it is not a successful test result.

Next: validate the corrected combined commit, refresh only generated inputs
whose verified recipes changed, rerun the release gate including real content,
fixed save corpus and performance checks, verify matching CI and all three
packages, then merge/tag/publish within Maxwell's explicit authorization.
