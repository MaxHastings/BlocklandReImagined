# 2026-11-23 Automatic release record

Both publishing workflows now queue a release record sync. Releases published
directly on GitHub trigger the same workflow. It reads the latest public release
from GitHub, resolves the tag to its commit, and updates the marked block in
[STATUS.md](../STATUS.md). The detailed v0.4.6 validation remains in the
[publication receipt](2026-11-23-v046-public-release.md).

The workflow stages a single generated documentation change, dispatches the
repository's required `check` job on that commit, and fast-forwards `main`
after the check succeeds. The dispatch path validates the change scope and
history. The sync retries when `main` or the latest release moves during CI.
The standard Actions token and existing branch protection cover the update.

Local checks used `python -m unittest discover -s tools -p test_publish_release.py`
and `python tools/check_docs.py`. The release tests cover exact tag resolution,
checksums links, an unchanged second sync and preservation of editorial text.
The [release process](../release-builds.md) documents the workflow and its
manual rerun entry point.
