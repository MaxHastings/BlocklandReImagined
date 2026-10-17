# v0.3.3 publication preparation

Maxwell explicitly authorized automatic v0.3.3 publication after required
checks, followed by the approved obsolete local build-cache cleanup. No
additional interactive playtest or approval is required. Other consumer work
stays parked; private assets and existing previews remain unchanged.

Reviewed source/documentation integration b138622a completed its actual full
gate before these Markdown-only release notes were added. The old remote
candidate a4b55e6d also completed all six required CI jobs before the branch
was updated, preserving its running checks. Final matching CI, the normal
gate and all three package/archive checks still govern publication.

No importer/content schema, default Add-On selection or bundle-input tooling
changes require reimport since published e4d9bb05. Fresh downloads of the
canonical private draft inputs were checked: addon-bundle.zip SHA256
87869bfbf8d3f30e56abcbde60dfcc386394f28ed0944be9bc864e5645617a6a,
ci-content.zip SHA256
822a70966616372806dde25f012c7a0c5ef18df09105f9385895c14e5ad58d5f.
There are 100 original Add-Ons, with AvP/private game packages excluded and
the authored Creek/Emerald preview bytes retained. Original installs and
local AvP copies are preserved; no content upload or cleanup was performed.

Exact source, CI/build IDs, archive verification and publication outcome are
recorded outside frozen source under artifacts/v033-*. This entry describes
preparation, not a claim that unfinished checks or publication already passed.
