# Private previews with previous converted Add-Ons

Added an explicit `-PreviousImportsForPrivatePreview` path to the Windows
packager, restricted to a named preview and `-SkipArchive`. The manifest is
labeled and packaging warns that the old conversions are not current-importer
output. The default release path still requires the current recipe. A preview
can reconstruct a private bundle from a previously packaged folder with
`addon_bundle.py from-release --previous-imports --preview-content <current content>`;
generated Add-Ons come from current content. Sources and verification then
accept the explicit preview flag.

Previous imports must still carry a well-formed recipe identity, match every
file in their own inventory (including companions), match the pinned original
source, version and author, and pass the existing companion and map checks.
The old recipe is not rewritten or called current. Python tests demonstrate
that normal packaging rejects an older recipe while preview validation accepts
its intact inventory and rejects modified rules or malformed identity.

`python -m unittest discover -s tools -p test_imported_addons.py` passed six;
Python compilation, PowerShell parser, `git diff --check`, and explicit
non-preview/archived-package guard checks passed. An attempted
`test_addon_bundle.py` discovery found zero tests and is not counted. The
full actual bundle reconstruction and package verification remain to be run
after Maxwell's play session; this change does not retroactively turn
preview.2 into a release-verified package.

Follow-up: verification now requires both the prior-imports marker and a
matching preview version when that preview mode is requested; ordinary release
verification rejects the marker outright. A small PowerShell integration test
checks missing markers, mismatched versions and rejection by normal release
verification. `python -m unittest discover -s tools -p
test_preview_packaging_guard.py` passed. Its first assertion assumed PowerShell
would not line-wrap an error; the test was corrected to match the stable error
text and passed without changing the guard.

The actual private workflow subsequently reconstructed 102 prior conversions,
replaced Slayer and its rules with a fresh pinned-source import, and used the
current generated ambience. The Windows preview packager produced a private
folder and its explicit preview verification passed 13,458 manifest files,
default Add-Ons and ten menu images. This was a packaging smoke check with the
older preview.2 executable, not a delivered new build or release validation.
It exposed stale generated-recipe hashes in the copied credits; reconstruction
now regenerates credits when current generated content is substituted. A second
actual reconstruction confirmed both generated package hashes appear in its
credits. Normal release verification remains strict.
