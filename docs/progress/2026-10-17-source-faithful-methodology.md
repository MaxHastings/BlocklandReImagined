# 2026-10-17 Source-faithful development methodology saved

Maxwell asked to save the named methodology and have the AA and Halo threads
follow it going forward. Added the concise canonical guide
[Source-Faithful, Consumer-Driven Vertical Slice Development](../source-faithful-development.md)
and linked it from the existing source-game section of `AGENTS.md`.

The guide preserves the source/evidence, dependency tracing, mechanism/policy,
ordinary-consumer and test boundaries already in force. It emphasizes one visible
outcome, independent expectations and useful negative controls, integrated state
combinations, immutable handoffs and a verified playable build. It retains
research and regressions while avoiding repeated discovery and parallel process
documents. Explicit preview adaptations remain distinct from fidelity completion.

Root will route the committed guide to the existing AA and Halo owners. This is
documentation only: no production changes, new research, paused-lane restart,
release action or interruption of the running projection validation. The current
projection preview remains pending its engine and actual AA consumer checks.

Validation: `git diff --check` and `python tools/check_docs.py`, including explicit
link checking of the new guide and `AGENTS.md`; results recorded with this commit.
No Rust code changed, so no duplicate Cargo checks are needed.
