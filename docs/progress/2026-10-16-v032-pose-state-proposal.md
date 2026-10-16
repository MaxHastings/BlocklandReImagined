# 2026-10-16 v0.3.2 pose-state proposal after D1

Root accepted immutable D1 `63edd2300eaec1a9f8a63ad9073ab6263d45eb96` as
bounded retained native inputs/read-only diagnostics, preserving supported-
provider and no-live-authority limits and keeping it outside v0.3.1. Its separate
implementation/evidence receipt remains in `2026-10-16-v032-retained-body-inputs.md`.

Added `docs/architecture/v032-authoritative-pose-state.md` for the separately
authorized documentation review. No new production paths are granted or edited.
It proposes evaluated active phases at the existing Session clock as the first
independent bounded step, then separately reviews final-key pose retention and
shared mutable locomotion/transition state before host composition.

Audited current clock removal/outbox semantics, actual Avatar mode/phase/history,
existing motor CrouchThread and motor-to-content dependency, runtime custody and
image identity queries, and the sync_seated/advance_animations/step_weapons order.
The proposal distinguishes Natural completion, visual hold and source policy;
names whole-mount applicability and retained-state budgets as decisions required
before retention implementation; and gives paths, dependency edges and native/
Session acceptance controls. It reuses pinned AA/Halo evidence and preserves
their unclosed authored-prone, dynamic-eye, aim and completion requirements.

Validation: `git diff --check` passed; `python tools/check.py quick --base HEAD`
and `python tools/check.py focused --base HEAD` both reported no crate changed,
with nothing to run. No builds, game windows, original asset writes, source
endpoint activation or runtime behavior changes. Future Cargo commands retain
the process-local two-job cap and unset CARGO_TARGET_DIR. Root review and an
explicit bounded production reservation are next.
