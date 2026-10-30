# 2026-10-30 Microphone packaging on main and v0.4.2 correction

Maxwell reported the microphone failure again in v0.4.2. PR #402 had been
pushed but never merged; both origin/main and the v0.4.2 recipe lacked
NSMicrophoneUsageDescription. The previous v0.4.1 asset correction did not
change the shared recipe. Maxwell explicitly authorized merging the fix,
correcting the v0.4.2 Mac download and ensuring future releases include it.

Merged origin/main 74c7e62f into codex/macos-microphone-permission, resolving
only the STATUS.md conflict by preserving current main's v0.4.2 publication
receipt. Preserved the unrelated untracked October 2 local setup note.
The recipe adds the microphone purpose string before signing and its verifier
requires a nonempty string. Added two content-free tests of the actual recipe:
generated plist includes the key before signing, and the actual verification
preflight rejects missing/blank/non-string values. Both detect the original
v0.4.2 omission. The normal CI tool-test discovery includes these tests, and
the official mac-release-asset workflow uses this recipe and verifies its ZIP.

The scoped gate first exposed a pre-existing test_port_addon assertion that
compared arbitrary iterdir order to sorted names on macOS. Sorted actual
folder names while preserving exact expected membership. The final local
gate at 9390c39cf63874ac22b8470541666b1f208ef891 passed history/protocol/blob
checks, all 154 tool tests, documentation and focused changed-area checks.
No Rust sources changed; no full gate or GPU/gameplay validation was run.
Final platform CI is run 41070826367; the preceding superseded run was
cancelled, rather than used as evidence for the new head.

Prepared the corrected package from the actual published Mac archive, retaining
the original and evidence under ignored artifacts/v042-mac-microphone and
dist/v042-mac-microphone. Original SHA256:
1240edeaec6065def6203f678382b19006887e73ebe94e9e4b77756e839b650e.
Original archive/signature/default-Add-On/menu checks passed 13,452 files;
its actual Info.plist lacked the permission description. Added only that
field, renewed ad-hoc signatures, regenerated the manifest and repacked using
ditto. Every bundled content file is unchanged. All four executables match
after removing embedded signatures and normalizing only the retained
__LINKEDIT signature-allocation virtual size; every other byte matches.

Corrected folder and ZIP pass all 13,452 manifest hashes, exact-tag default
Add-On/import/media identities, curated menu art and strict deep signature
verification. Packaged --version reports v0.4.2 (93c1439da). Headless --check
with fresh isolated state passes: 21 maps, 1,372 bricks, 35/35 pictures, 14
modern light-parameter maps. It retains 226 fallback diagnostics and the
existing Duplicator preference compatibility warnings. No window or audio
device opened. Corrected ZIP is 234,177,011 bytes, SHA256:
8e4d9177f41a07fafa3c6637c7beceb710f0558020df35f05d0084ae05959794.

While CI was running, Maxwell explicitly authorized publishing immediately,
then merging when ready. Uploaded the corrected Mac ZIP and SHA256SUMS to
v0.4.2 with gh release upload --clobber, and added a dated correction to its
release note. The existing signed-in gh CLI performed these writes because
the GitHub connector does not upload release assets. Retained the source
tag and original Windows/Linux asset IDs, sizes and digests. GitHub's uploaded
digests match the verified files. Downloaded the actual public ZIP and sums
again: both hashes match local, and the public ZIP includes the purpose string.
SHA256SUMS digest:
960c8e92d506568baaa3c10a163244ec75b808a6b3446a496ca9b132a0315cef.
Remote annotated tag 0166353a6b043926ec7e166cbd34f5a03c09e3ea still peels to
the original source 93c1439dacea2c6bce19ca735195bb0f6b944daa.

[CI 41070826367](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/41070826367)
passed all seven jobs on exact head 9390c39cf63874ac22b8470541666b1f208ef891:
Windows check/binaries, Linux wheel and four Linux test shards. The existing
event-specific Windows test aggregate is skipped for PRs, as before. Used
python3 tools/gate.py --push; the scoped checks passed again, then the enabled
pre-push hook verified the five-commit history range and reused the locked
parent's validation before pushing that exact head to main. No force push,
hook bypass, ignored failure or extended-gate claim.

GitHub confirms [PR #402](https://github.com/MaxHastings/BlocklandReImagined/pull/402)
merged at 09:41:18 AM America/Chicago. Fetched remote main and independently
checked it equals 9390c39cf63874ac22b8470541666b1f208ef891 and includes the
permission key, verifier guard, regression tests and official release workflow
consumers. Updated the public correction note from pending to merged. This
completion receipt is recorded after that guarded source update; the source
fix is on main, rather than merely on a pushed PR branch.

Live permission prompting, microphone input and gameplay acceptance remain
Maxwell's. No visible game, microphone capture or interactive input was used.
