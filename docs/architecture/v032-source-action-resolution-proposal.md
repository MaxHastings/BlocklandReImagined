# Source action admission and resolution: bounded future proposal

DOCS ONLY. Engine stays at `31f2fbfdb86b3bb5ab35b9f86103c206eb2d37e6`;
`722bca2132300e5b03744d89c924f561ce91ad4a` only clarifies its documentation.
Root owns any implementation grant. This future lane is outside frozen v0.3.2
candidate e4 and does not hold its gate, packaging or publication.

Read-only private evidence: completion contract
`133dbe707ec895b335156d0230b3b8860fac2c5c`,
`aa25/notes/2026-10-16-m16-completion-policy-contract.md` and its evidence.json;
selected-fire qualification `10feaf5309198bc2f9e34f947705d7669d8f2593`,
`aa25/notes/2026-10-16-m16-fire-completion-source.md` and
`m16-fire-completion/{source-contract,controls}.json`. The latter supersedes the
former's missing *selected static fire-policy* prerequisite; runtime completion,
controller re-arming and broader source fidelity remain open. Consumer844c39a
on exact31f is still strict red. AA is separately qualifying an inactive Fire
provider; this document neither assumes that provider passed nor edits it.

## Smallest phases and owning paths

| Phase | Proposed scope, after a separate root grant | Evidence required before proceeding |
|---|---|---|
| 1a: finite rate | Extend existing package/native admission options to finite positive bounded rate, preserving one required operation and original provenance. Reuse AnimationStart, native phase_at and clock/patch composition; no second clock or set-rate-after-admission workaround. | Source-free variable-rate controls, then verified Fire/FireB/FireC provider and actual selected clip/rate2.25 producer. No fixed Fire substitution or source RNG-equivalence claim. |
| 1b: tween | Extend the same admission only after defining authoritative previous-sequence/effective-tween context and native timing semantics. Record requested versus effective tween; finite bounded nonnegative input, first previous None suppresses it. | Known None versus Still cases, actual prior-target context and binding, unchanged source keys. Absence of an active Weapon clock is not evidence that the source previous sequence was None. Unknown context refuses nonzero tween. |
| 2: callback policy resolution | One separately opted-in, trusted native-result decision context may return one required exact-weapon state resolution plus bounded own namespace edits. Existing on_weapon_animation remains state-only. Prove fresh reload first, then qualified ordinary F -> R -> F. | Package establishes selected source notify/probe/state/authority and Finish/controller branch; engine rechecks live actual identity, current action and selected original mount. NativeNatural alone grants no Finish decision. |
| 3: distinct latent source wait | Only if separately granted: a bounded source-owned wait attached to the same admitted action, competing with callback resolution. | Faithful state-code entry/exit and f32 Sleep polling evidence; no request-time deadline, guessed on_tick unlock, new counter or general script VM. Its completion origin is SourceWait, never NativeNatural/Finish. |

Likely existing owners for phases1-2: package-runtime ops/player, script and
behaviour declaration; sim session/animation and restricted package dispatch/
perform/player. Prefer reusing weapons prepare_weapon_patch unchanged for state
writes. Any required producer work is explicitly scoped in weapons/runtime and
Session forwarding after root review. These are candidate paths, not a grant;
root records seam ownership before implementation. No wire, renderer, motor,
manifest, save schema or original asset changes are proposed. Tween timing
support here would not prove blended pose/render equivalence.

31f native prepare already admits bounded rates including zero but requires
tween0; host composition and package operation fix rate1/tween0. Thus rate is
mainly a narrow admission extension; tween is an actual native-context/timing gap,
not merely an argument to pass through. For selected source fire, keep the six
original keys at30FPS, rate2.25, requested f32 .01; last-key span5/(30*2.25) is
separate from tween. Do not stretch clips or claim exact original tick parity.

## Trust, retention and one atomic resolution

Prefer a restricted result-decision hook over a freely callable token operation.
Its unassigned API has an implicit host context: actual PlaybackId, original
binding, package/source identity, provider digest, result and authority tick.
A script-supplied JSON token/nonce/outcome or typed/image command cannot create
that context. Ordinary action admission still starts at original ToolFire or
trusted ToggleLight; unmount/emote/unqualified routes remain unsupported.
Started remains asynchronous and observational. No existing hook is widened.

The resolution capability must survive both host and observer drains. Current
31f dedup receipts (MAX_RECEIPTS256) store request/id/clip but no outcome or
package permission and evict oldest entries; they are **not** completion authority.
Minimal retention candidate: extend that existing bounded receipt envelope with
owning package/source identity, native terminal evidence, action phase and
unresolved/consumed status. Reserve space before admission and pin unresolved
entries; evict only entries without a live permission. Explicitly refuse at
capacity before clock/patch/package writes. This introduces bounded admission
backpressure, not a second event queue, identity serial or retry mechanism; root
must review its effect on ordinary host admissions and dedup semantics. If this
cannot preserve existing guarantees, stop for a retention decision rather than
silently using an evictable receipt or adding an unbounded store.

A resolution does not start/finish/cancel another animation and cannot edit ammo,
reserve or spares. Read-only preflight checks retained permission, original
owner/life/body/item generation/slot/right-hand mount, provider digest, package
source, current namespace nonce and exact expected native image state. The
selected branch additionally requires authoritative controlled-actor equivalence;
peers/alive or custody flags alone do not prove a source controller gate.
Package private policy supplies source UState/notify/probe/Finish predicates;
unknown/unsupported branch refuses rather than defaulting to Ready.

Compose existing capabilities under one exclusive Session borrow:

1. Validate the one-part/one-resolution shape, authorization, namespace budgets,
   checked package revision, expected native state and retained permission.
2. Split the existing fields and prepare_weapon_patch with rounds=None,
   expected_rounds=None, desired state only. No callback or tick interleaves.
3. Commit the prepared patch; then infallibly consume the already located
   permission and move the owned namespace. No late lookup, Result or callback.

Only successful commit consumes permission; refusal changes no native state,
package state, ammo/notice, permission consumption, serial or watermark. A
separate decision-dispatch attempted flag is marked before hook execution, just
like observation delivery, and is not a consumed resolution permission. Retain
that flag on the existing bounded payload if a separate decision delivery is
approved; never steal either 31f consumer flag or introduce retry dispatch.
An error/no-op does not auto-resolve or refund. Source changes abandon old
permissions; life/body/custody/actual mount and newer-action checks defeat stale
application. Native Weapon lifetime still follows custody, even when resolution
requires current selection. Do not cancel a holstered clock to simplify policy.

## Callback Finish and latent direct Idle are different decisions

| Origin | Selected qualified meaning | Required isolation |
|---|---|---|
| Permitted native-end callback | Package's applicable source state permits AnimEnd; authority invokes Finish. Instigator/controller present selects Idle; missing selects empty, not Ready. | Match actual action/result and weapon target; a Pawn15 endpoint or old F cannot release R. Notify disabled means no callback Finish. |
| Listen single-shot state wait | BusyFiring's own Begin sleeps source SingleFiringRate2, then goes directly Idle. | State processing starts the wait; original strict half-delta/f32 polls and GotoState cancellation apply. Do not impose Finish's controller test on this different branch or call2seconds a mandatory cadence. |
| Dedicated single-shot wait | Own Begin sleeps min(source count6/(30*2.25)+requested tween, SingleFiringRate), then directly Idle. | Not reload's inherited Sleep -> AnimEnd -> Finish, not visual last-key span. |

A future wait, if needed, can use one fixed wait slot in the retained action
rather than another ticket/counter: source-owned parameters, start at actual
state-code processing, bounded f32 remaining state, and a distinct trusted wake
origin. Callback resolution clears the old wait in the same state transition;
a later wake is stale. Do not build this phase without proving state exit and
wake arbitration. Existing state-only on_tick is not substitute authorization.

## Acceptance and refusal tests, in order

- Rate/tween: exact selected clip and provider bytes; zero/negative/NaN/infinite/
  excessive options refuse; None/Still effective tween; native span versus source
  count/wait; zero-duration Started/Natural order; no fixed source tick deadline.
- Authority: original queued binding across remount/duplicate selection/life;
  wrong package/provider/token/channel/Pawn15, invented token JSON, old F versus
  new R, unqualified/unmount routes and changed source all refuse before writes.
- Atomicity: wrong expected state, custody loss, holster selection mismatch,
  dead/missing controlled actor, namespace/state/revision/permission capacity and
  duplicate consumption preserve all resolution state. No ammo or notice at resolve.
- Delivery: both host/package drain orders, Started before callback delivery,
  observer throw/world-op refusal, exhausted shares, decision-hook throw/no-op,
  and both drains before resolution cannot forge, steal or retry permission.
- Arbitration: native end with notify off stays busy; controller-present Finish
  versus missing-controller empty; source direct Idle remains distinct; callback
  wins/clears wait, late callback/wake cannot release replacement action; no
  Interrupted/Cancelled/FinishAnim-stop-to-Ready/refund shortcut.
- Ordinary consumer: retain current red traces, then fresh12->31/spare11->shot30;
  combined shot12->11, source F resolution, reload11->31/spare index1=10 once,
  source R resolution, final shot31->30. Total82->81->81->80. Single pending0
  does not weapon-auto-refire; GuardTimer is not a fabricated mandatory lock.

Open review choices: authoritative controlled-actor/previous-sequence inputs;
non-evictable256 receipt reservation/backpressure; separate restricted decision
hook name/dispatch order; and whether callback-only phase2 suffices for a clearly
bounded consumer before latent phase3. No production work or A1/N1 closure is
requested by this proposal. Root owns combined validation and later grants.
