# 2026-10-16 Session clock/weapon composition proposal

Root accepted immutable prepared-patch db12842aacdb9bebec72ae5db6b267f6273875e0
and requested docs-only next-boundary review. Added
architecture/v032-session-weapon-patch-composition.md. No production/test edits.

Proposed one trusted-host Session method in existing animation.rs, plus one new
Session integration target after a grant. Existing private access suffices;
no Session exports, widened visibility, reusable ticket or generic transaction.
Whole-Session binding/clip validation precedes disjoint field borrowing; weapons
guard precedes real clock preparation/commit; only infallible patch commit
remains after clock success. Real full-outbox refusal exercises guard abort.

Specified actual native Session baseline red/green with512 genuine records,
unchanged ammo/image on refusal, same-nonce retry, real Started/Natural and no
source busy release. Package transaction, original ToolFire provenance, observer
delivery and current-selection versus custody lifetime remain dependencies.
AA review e4402c420d0ead7a63ef31fd7916aa2135bad7df remains source proposal review,
not an executed bridge or A1/N1 closure.

Validation: read existing clock prepare/commit/refusal and native Session fixture,
accepted weapons guard and lifecycle controls. Documentation whitespace and empty
production diff checked; no Cargo rerun for docs. db12842a source stays immutable.
