# Installed-content gate expectations

The 591ff3 gate reproduced three content-test failures. Its content refresh
installed the current default original Add-On bundle, including eight additional
maps and previously unresolved brick definitions. No brick conversion, planting
or overlap implementation changed between3117 and5ec54.

The native content test now counts the14 required integrated maps separately
from installed Add-On maps, retaining their stock preview checks and native load
checks for every map. Start Game compares its complete sorted names with the
actual installed map inventory, instead of assuming no Add-On maps. Baseline
failed22vs14; corrected native test1/0/0 and Start Game synthetic/content2/0/0
pass on the same content. No runtime map-selection change.

The save-corpus comment claimed placed+set_aside was independent of Add-Ons.
That is false: a newly resolved definition participates in ordinary on-grid and
overlap checks, while an unresolved brick is simply retained unloaded. An
isolated diagnostic copied the real SaveHost and recorded input source-line
identities absent from the completed host, then asked the unchanged native
Simulation::on_grid and drop_overlapping to classify every rejected brick
against the retained world. All rejected rows were on-grid overlaps; none was
unexplained. Same source bytes with base-only package selection changed totals,
independently demonstrating the faulty content-independence assumption.

| Save | Converted rows | Current default kept | All overlap rejections | Base-only kept |
|---|---:|---:|---:|---:|
| 2023 XMas |309781|309418|363|309734|
| Sumz City |171909|171898|11|171902|
| Halloween Block Party 2026 |67511|67422|89|67503|

Both default runs returned the same kept totals. Base-only rejected-source lines
were a subset of default rejections: new resolved content exposed316,4,81 more
overlaps respectively, including fences/glass/vertical plates and plant geometry.
The historical default totals309630/171902/67423 were not valid for this content.
Update those three exact kept expectations and additionally assert converted
row totals independently, so missing import rows still fail. Correct the public
Hosted field documentation; no planting behavior or equality assertion changes.

Original BLS SHA256, read-only:

- XMas:8102cd45a68b23bd6c5e25ca1f54b5caea935793b5e2802a51034bbc1097dd71
- Sumz:10c7c0aeac5ddf055433d96df45dd9031a24a926f878fc2bbb40431808a9a055
- Halloween:e6fc4686c32a6431ed138a459ee286a2f8d9865e3f8e243d5f02a40d2e760215

Local diagnostic source, full rejected rows and logs are under primary
artifacts/v031-save-rejections and v031-save-rejections-base. Diagnostic-only
example was moved out of the crate before validation; no original content or
debug instrumentation ships. The initial overlap-diagnostic link encountered
Windows LNK1104 because the base-only diagnostic still held the same EXE; waited
for that process to finish, then rebuilt/reran successfully without interruption.

Validation continues with exact corrected corpus, quick and focused. Full gate
and all platform checks are still required on the combined successor candidate.

Corrected corpus test passes1/0/0 in81.01s (session3538 exit0). Quick passes
format and584unit tests/0failures/76existing ignored in110s (session4878 exit0).
Scoped focused session46925 exits0: format, client all-target Clippy with
warnings denied and584unit tests/0failures/76existing ignored,357s total.
Logs: primary artifacts/v031-corpus-corrected.log,
v031-content-quick.log and v031-content-focused.log. These scoped results do not
establish main integration or a full candidate pass. The cargo fmt --all shell invocation hit
Windows command-length206; exact changed-file rustfmt succeeded and the required
quick per-package format check subsequently passed.
