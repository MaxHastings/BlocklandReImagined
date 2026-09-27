# Root progress entry for integration

2026-09-26: Added isolated native WeaponEffects adapter and approved focused
shared cue/FX lifetime support. Root has not yet integrated it into App. All
eight CPU adapter tests pass with private native resources explicitly enabled;
25 projectile definitions/35 transient resources resolve against new FX pack002.
The additive offline importer verified original source hashes and added12
particles/12emitters/6composites with no new conversion diagnostics. Independent
Python verification checked27source hashes/all18original texture hashes and
baseline preservation. Offscreen GPU gallery rendered and was inspected; report
and images are ignored in artifacts/native-weapon-effects. No visible window,
input automation or audio.

Reliable cues now carry actual image/hand/aim or collision normal plus scale.
Malformed metadata rejects atomically; map ID0 stays valid. Source removal drains,
duplicates and pre-checkpoint cues do not replay, and finite sources cap emission
inside long frames. Separate shell/animation host requests are bounded.

Commands and exact limits/open requirements: README.md in this directory. Gates:
8adapter tests,2presentation tests,1new codec/replica atomicity test,24weapon tests,
native Gun Session test and9FX runtime tests passed; relevant all-target Clippy
and standalone importer Clippy clean. Initial importer attempt correctly failed
because recovered core is not plaintext in the original installation; added an
explicit recovered-core argument verified against its stored hash. Initial new
net fixture used nonexistent Message::Delta; fixed to Message::Update without
changing production behavior. Initial Clippy single-element clone warnings fixed.
Trail axis audit corrected the provisional positive-velocity orientation to the
negative-velocity engine-family convention, with regression and source citation.

Remaining: root App/mount/renderer integration, shells and image/avatar playback,
mid-state late-join emitter reconstruction, explosion model/debris, scale and
legacy emission/distribution/light fidelity. Goal/alpha remain incomplete.
