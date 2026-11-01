# Prior Add-On bundle provenance

Downloaded the existing private GitHub `addon-bundle` draft asset, uploaded
2026-10-30, into private diagnostics. Its ZIP SHA-256 is
`60db1e6addd09fdce194a76c250556655b94be986dd43c3bbe526e4fa7aaf99e`;
its `bundle.json` records build commit `60fcbe7f4`. This is a draft asset,
not a public immutable release, so the recorded digest is essential if it is
used as an input. The asset contains original converted content and was not
committed.

The current private reconstructed bundle has 100 original generation receipts.
Ninety-nine match the draft asset's receipt bytes. For those 99, independently
hashed every inventoried local file and corresponding ZIP member: 9,866 file
comparisons matched their receipt hashes. Slayer is the one different receipt:
the private reconstruction intentionally substituted the newer imported rules.
This establishes the relationship to the recorded draft bytes; it does not
prove compatibility with the final runtime or that all current importer changes
are present.

Since the recorded bundle commit, relevant importer changes include source
`hSuperStacker` conversion, bot teamwork defaults and Bot_Town's installed
brick-geometry dependency resolution. Slayer's default reset policy changed
afterward too. In particular the draft Shark conversion lacks its new explicit
false stacking setting and Slayer requires the new rules. A release path must
choose and verify current outputs for affected packages, and may reuse older
outputs only with an explicit per-package compatibility decision, pinned
receipt/inventory, and truthful source and importer identities. Changing the
entire importer-recipe hash to match older content is not evidence of a rebuild.
Platform package, source-dependency and final content checks remain open.
