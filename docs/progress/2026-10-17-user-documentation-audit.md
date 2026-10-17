# 2026-10-17 User documentation audit

Maxwell asked for a thorough review of everything a reader encounters from the
README, with practical long-term maintenance. He subsequently authorized
collaboration with the project coordinator and integration onto main after the
required checks. Work is isolated on `codex/user-docs-audit`, based on published
v0.3.2/main `e4d9bb05df3c26c95a410cb9872990d99b37b54d`. The historical primary
checkout and its unrelated dirty/local material were preserved.

## Definition of done

- A newcomer can understand the alpha, download the right archive, launch,
  host/join, find controls and report a problem without reading engineering
  coordination or installing developer tools.
- Players can find settings/saves/custom content, update into a fresh folder,
  back up important state and understand alpha compatibility limits.
- Creator paths distinguish brick events, native Add-Ons, legacy data import,
  supported native ports and source-engine development. Document reachable
  public functionality, with experimental/partial/unsupported behavior explicit.
- Release state agrees with the published source/assets; private previews and
  unintegrated mechanisms are not advertised as finished public games.
- Current guides remain navigable in source and extracted platform layouts.
  Historical evidence stays reachable, with obvious obsolete release plans
  labeled rather than rewritten or removed.
- The repository has a small repeatable check and clear expectations for
  updating docs with behavior changes.

## Findings and corrections

The audit inventoried tracked Markdown outside vendored material, progress
entries and source research, and reviewed the reader paths through player,
creator, contributor, packaging and subsystem entry documents. The automated
scope is explicit: 71 maintained guide/entry pages, not a claim to re-prove every
historical audit or every API field. Source references include the packagers,
bootstrap, release tooling, current defaults, importer/glTF implementation,
weapon/drop runtime, menu labels and computed protocol/save constants.

- README placed large videos before downloads, implied complete parity and
  required Rust for ordinary Add-On authors despite the bundled tools. Play
  now comes first; capability wording is qualified, with separate player,
  creator and engine-developer routes.
- STATUS still described v0.3.1 as current and v0.3.2 as pending. `gh release
  list` and `gh release view v0.3.2 --json ...` confirmed public v0.3.2,
  source `e4d9bb05d`, published October 16 at 10:17 PM America/Chicago
  (October 17 03:17:49 UTC). Current state now reflects publication without
  closing the full alpha contract or player acceptance.
- Update instructions encouraged copying entire installed Add-On trees, and
  Mac cleanup implied custom additions could be reconstructed. Guides now
  explain state backups, fresh release folders, selective custom-content
  transfer, bundled-file preservation and per-build Mac content.
- FEATURES said new bricks could only use legacy files despite working glTF
  brick import. Old-Add-On guidance claimed a 64-texture limit; source admits
  1024 new effect textures per merge under shared budgets. Corrected both,
  exposed map/model routes and retained import/port limitations.
- Rule Workshop named the obsolete Teams & Add-Ons route; current UI uses
  Teams. Dedicated hosting lacked a Windows command, called all 64-bit Linux
  supported and guaranteed at most one minute lost on crash. Corrected the
  architecture, command and bounded recovery wording.
- Packaging docs referenced nonexistent `tools/shipped-addons.json` and omitted
  disabled bundled Add-Ons, newer helpers/notices and Linux layout. Source
  setup still called Linux tooling-only. Updated against actual code.
- Runtime notes mixed old schema compatibility, unconditional eye-origin shots
  and provisional point-sweep drops with current claims. Corrected demonstrated
  current behavior and labeled early protocol/probe evidence as checkpoints.
- Pre-publication readiness plans now identify themselves as historical.
  Existing acceptance requirements, dated test results and original release
  notes were retained. No private assets were added.

## Repeatable validation

`tools/check_docs.py` validates local guide paths/headings and runs the actual
`package_guides.copy_guides` in temporary Windows/Linux/macOS guide layouts,
each with and without Stress Lab. The common copier now rewrites links in all
player guides, distinguishes creator/player PLAYTEST paths, preserves anchors
and sends absent platform/history pages to their source online. Online fallback
paths/headings are also validated against repository source, so HTTPS rewriting
cannot conceal a missing page. Vendored notices remain byte-identical.

The first generated-layout run found a real escaping `../progress/...` link in
the shipped v0.2.3 notes. It now resolves to the existing online receipt.
Regression tests cover that failure, missing platform guides, document-name
collisions, anchors, invalid links, fenced examples, encoded/reference links
and agreement with the actual packagers' player-guide inventories. Windows CI
now runs the offline documentation check; its existing tool-test discovery
includes these regressions.

Validation:

- `python tools/check_docs.py`: 71 documents and six generated layouts pass.
- `python -m unittest discover -s tools -p test_docs.py`: ten tests pass.
- `python -m unittest discover -s tools -p 'test_*.py'`: complete tool suite
  passes, 109 tests.
- `python tools/check.py quick` and `focused`: both report no changed crates.
- `git diff --check`: passes.

No Cargo build, content regeneration, new game package, original-install write
or interactive gameplay occurred. The coordinator already owns the active
integrated gate; this audit does not start a competing full gate. The source
docs improvements will ship in future packages, not replace immutable v0.3.2
assets. External URL availability and every historical/API claim are outside
the link check; code-backed editorial review remains necessary.

Next: coordinator reviews the immutable branch/PR, reconciles any unpublished
candidate STATUS overlap, and integrates through required CI and gate discipline.
