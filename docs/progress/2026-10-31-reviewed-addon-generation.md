# Reviewed earlier Add-On conversions

The release verifier can now accept an earlier conversion only when the
repository supplies `packages/imported-addon-compatibility.json` with schema 1,
the exact current importer recipe hash, and that package's exact receipt SHA-256.
The prior receipt still verifies every imported and companion file, and the
existing original-source pin, version, author, map and companion checks remain
in force. A different importer recipe invalidates the review. The receipt and
its original recipe are not rewritten.

There is deliberately no compatibility lock in this commit: no prior package
is automatically approved, and normal release validation remains strict until
the per-package compatibility assessment is made and an exact lock is added.
This supplies a bounded mechanism for a truthful hybrid bundle, not evidence
that such a bundle is ready or that unavailable sources were rebuilt. It also
does not assemble or upload that bundle. New fresh outputs for affected content,
source provenance, final health checks, platform packaging and release validation
remain necessary.

`python -m unittest discover -s tools -p test_imported_addons.py` passes eight
tests. The new case accepts an explicitly reviewed intact conversion and
rejects an unlisted package, modified companion rules, altered receipt, later
recipe and wrong original-source pin. The first run exposed a missing source
name in the new test fixture; supplying the real required fixture field resolved
the error. `test_preview_packaging_guard.py` passes, Python compilation and
`git diff --check` pass. No source asset or generated content is committed.
