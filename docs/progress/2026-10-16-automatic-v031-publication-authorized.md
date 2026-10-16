# Automatic v0.3.1 publication authorization

Maxwell explicitly authorized publishing v0.3.1 automatically when ready because
he may not be home for a final playtest. This supersedes the earlier requirement
to wait for his final interactive test and a separate publication approval. No
additional confirmation is required once the scoped release meets its checks.

The scope is unchanged: finish the current approved animation seam and release
bug fixes, freeze new mechanisms for v0.3.2, keep AA/GMod parked, and let Halo
continue private fidelity independently. Keep original/source-specific assets
private and do not include them in public release artifacts. Agents still do not
perform interactive gameplay. Both delivered Halo previews remain unchanged.

Root must verify the integrated candidate, required Windows/platform checks,
content/package integrity and release documentation before publishing. Failed
checks or unresolved release blockers still hold publication. Use the established
repository release tooling and inspect tag/release/workflow state first to avoid
duplicate publication. Verify the resulting Windows, macOS Apple silicon and Linux
artifacts, version and update metadata, then notify Maxwell with the release link
and precise automated-validation limits. Do not claim interactive acceptance.

Root updated the existing heartbeat through automation_update to retain this
authorization and remove the stale manual approval hold. The scope/readiness and
STATUS documents are updated in root's isolated bug worktree; primary sampler
f554 remains frozen while its own gate and Windows checks run. This entry does
not claim v0.3.1 is ready or that publication has happened.
