# Destruction motion follow-up after preview.1

Maxwell accepted the removal of the canned base bend and delayed rubble
updates in preview.1, but found the resulting fall too stiff and vertical.
PR #437 remains held for the follow-up and gameplay acceptance.

Two lost physical inputs were found. The asynchronous support proof discarded
its causal blast metadata, and first contact retained a distance/point but
discarded the normal. Every landing clump then received gravity velocity and
zero spin, regardless of the contact's location. The saved rocket replays
also establish that the detached material is outside those blasts' radii:
retaining the blast alone does not fix these two saves.

Section death cues now carry an optional validated radial impulse. The wire
change has its own protocol-change note; peers with the older layout are
rejected by the existing checkpoint. There is no save migration. Source
search restarts and interrupted delivery preserve the exact admitted cause.
Clients accumulate uniform-density member volume, first moment, inertia and
impulse during bounded admission. Density is the same common plastic-density
assumption as existing debris, and cancels from the derived response. Original
geometry, gaps and mass centers determine motion, not section aspect ratios.

First contact keeps its actual normal. The effective-mass normal reaction,
with the existing chunk restitution, supplies a rigid velocity field around
the true section center of mass. Clumps inherit velocity at their own material
centers and the resulting spin; the rendering/proxy origin stays independent
of that mass center. This is one representative cosmetic contact, not a full
support-patch solver or an authoritative structural simulation. Overflow keeps
its existing bounded translation-only collision path; this work does not add
unchecked angular movement to it.

An early-partition experiment broke the retained lifecycle/audio checks and
was reverted. Delivery, Ready/model readiness, shared fall until contact,
bounded partition, retirement and audio contracts remain. No assertions were
weakened to accept that experiment. A broad debris unit filter inadvertently
included the existing CPU measurements (14 seconds); subsequent local runs
use the section filter. No GPU suite, visible window or interactive playtest
was run.

Validation: the 33 focused section cases pass, including new
centered/eccentric contact, separating contact and non-increasing impact-energy
checks. Initial causal-retention fixture expected an unnormalized zero radius;
it now compares against the actual admitted blast, including the existing
direct-hit minimum radius. The 24 server destruction tests, untrusted impulse
validation and the real-codec section/backlog replication case pass. Both
external two-stock-rocket replays pass, with zero dropped cues and fallback
debt still zero for Towers and at most two frames for Pirate World. Observed
landing angular velocities are nonzero for the actual 193- and 1334-member
sections; this is a real contact correction rather than only a synthetic
impulse fixture. These CPU traces do not establish visual acceptance.

`cargo clippy --locked -p bri-client -p bri-net -p bri-sim --all-targets --
-D warnings` passes after removing an unnecessary normal conversion.
`python tools/check_format.py` and `python tools/check_docs.py` pass (77
maintained documents and six generated guide layouts). This is scoped Windows
validation, not a full workspace pass. The wire addition is transient cosmetic
metadata, with no authority/save changes; platform CI and protocol/history
checks remain required for merge. Preview.2 packaging and Maxwell's acceptance
of motion and the Bedroom zombie follow-up remain pending.

Private inputs and traces remain outside Git under
`E:\Downloads\BlocklandReImagined-Release-Evidence\destruction-2026-11-04`.
The earlier input hashes and repro commands are in
[the first motion receipt](2026-11-04-destruction-motion-repro.md).
