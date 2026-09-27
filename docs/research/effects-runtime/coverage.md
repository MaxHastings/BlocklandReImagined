# Effects runtime evidence and integration coverage

2026-09-26. Implemented only new isolated fx-runtime/fx-import crates and their
owned docs/content/artifacts. Original install stayed read-only. No visible game,
desktop input, audio playback, source-install write, shared manifest edit or
interactive playtest occurred. Maxwell retains all interactive testing.

## Evidence

- Recovered v20 `allGameScripts-Vanilla.cs`: brick setEmitter/setEmitterDirection
  around lines 11180–11362; player jet/splash around 8549–8799; death and spawn
  effects around 10044–10270; original tool/projectile/add-on literal definitions.
- `effects-pass-004/provenance/effective-declarations.json`: 119 particles,
  120 emitters, 13 fxLight definitions and original node scale fields.
- Primary `E:/Downloads/B4v21Launcher/versions/Blockland v20`: all 18 texture
  bytes and 75 add-on script inputs match prior source hashes. The recovered core
  script is the 76th hashed input; it is not misrepresented as original plaintext.
  The designated-reference comparison already establishes corresponding base-file
  identity. `source-proof.json` retains all paths/hashes.
- Inspected OpenMBU engine-family files at local pinned commit
  `3d6516e1c9cb43e61aead3369d1f7210d08b83ef`: particle.cpp initializeParticle
  (lines 301–310), particleEmitter.cpp addParticle/update/setupOriented
  (772–806, 919–925, 1037 onward), fxLight.cpp renderObject
  (1337–1352, 1396–1483). These are corroborating engine-family evidence,
  **not** a claim that Blockland's closed engine is identical. No engine source
  implementation was copied into the native crate.

## Implemented and exercised

| Area | Native behavior | Evidence/check |
| --- | --- | --- |
| Source lifecycle | Stable nonrecycled handles, start/update/pause/drain/immediate/teardown | Moving-path, repeated-source, stale-handle and stop tests |
| Particle keys | Authored time/color/size interpolation and source override keys | Synthetic numeric assertions; actual 119 definitions loaded |
| Spawn distribution | Authored theta/phi, rotating phi reference, velocity/offset variance, random particle selection | Engine-family addParticle; all 120 emitters simulated/rendered |
| Motion | Inherited velocity, wind sign, gravity, acceleration, drag, spin + random spin | Numeric half-second motion/curve test, frame-partition stability |
| Attachment movement | Transform interpolation at each emission instant; particles detach into world space | Synthetic path test; moving-player-jet nine-frame GPU montage |
| Burst/finite sources | Immediate bursts, finite emitter lifetimes, composed explosion sources/lights | All 41 composites execute; original-composites gallery |
| Brick setup | Six exact script directions, small-dimension thresholds, point-node choice, paint keys, fake-death pause | Recovered v20 script; brick threshold/direction test |
| Billboard drawing | Instanced original PNG/JPEG texture binding, spinning camera-facing and oriented modes | All-emitter offscreen gallery; source texture verifier |
| Compositing | SrcAlpha/InvSrcAlpha; SrcAlpha/One; One/One; depth-read/no-write particles | Exact synthetic GPU readback assertions |
| Animated lights | Authored RGB/brightness/radius curves, 21 transient explosion lights | All lights sampled; light-flares gallery and numeric snapshots |
| Flares | Authored radius/distance interpolation, luminance link, size fade, third-person gating, host LOS | Numeric tests plus GPU depth-bypass assertion |
| Bounds | Source/particle/light limits, bounded overdue emission work, explicit diagnostics | 1,000-source crowd benchmark and 3,600-second stall test |
| Content safety | Schema/references/checksum/header/dimension/containment/read limits | Tampered native fixture; independent 76-source/18-image verification |
| Host shading contract | 32-byte typed light record plus concrete WGSL storage-buffer helper | GPU module validation; root owns actual scene shader binding |

The runtime library compiles without any Torque import/converter dependency.
GPU formats, multisampling, camera and depth target come from the host. The probe
creates an offscreen device only in its example binary.

## Artifacts and measurements

`artifacts/native-effects-runtime/` contains original-emitters.png,
moving-player-jet.png, original-composites.png, original-light-flares.png,
report.json, independent-verification.json and release versions. JSON indexes
map every tile to its stable ID and sample time. Visually inspected emitter and
attachment galleries show original clouds, star/chunk/ring sprites, directed
streams and moving tails, rather than placeholder quads.

On NVIDIA GeForce RTX 4070 SUPER, release probe with 1,000 jet sources,
20,000-particle cap and 240 advances at 120 Hz: median 0.784 ms, p95 0.942 ms,
total 0.191 seconds for simulation; 303,877 accepted emissions and 29,123
capacity drops, no emission-work skips. One-texture 20,000-instance upload
took 0.302 ms, 960,112 bytes, one draw call. These final figures include updating
live source wind, visibility and override keys on existing particles. This is a CPU/update/upload probe,
not a GPU timestamp measurement or mixed-scene gameplay FPS claim. Mixed
transparent textures can require many order-preserving draw calls.

Nine synthetic runtime tests, one explicit actual-pack test, one explicit
headless GPU blend/depth contract test and one importer parser test pass.
Both crates pass all-target Clippy with `-D warnings`. An initial Clippy run
caught an oversized brick helper signature and needless reference; both were
fixed. The GPU example was updated to the current glam 0.33 camera constructors.

## Required remaining work; no acceptance waiver

1. Root must bind lifecycle/gameplay dispatch, brick/actor attachments, effect
   menus, visibility/LOS and dynamic light buffers in the actual client. Literal
   relationships cover player/tool/weapon/vehicle data, but cannot implement
   those host state transitions by themselves. Native precipitation spawning is
   root-owned. Weapon/vehicle teams must consume native IDs and stop handles on
   entity despawn/session cancellation.
2. Exact v20 scaled-volume placement and usePlacementForVelocity behavior is
   closed-engine behavior; current native uniform-box/radial interpretation is
   explicit. Explosion density currently uses uniform-box placement and needs
   original distribution comparison. Source color overrides default to particle
   keys when the host supplies none; exact owner paint/water colors must be
   supplied by adapters. TimeMultiple controls emission timing; exact engine
   node pre-advance interaction remains to compare.
3. `pongBounceExplosion` has no authored lifetimeMS and currently uses a clearly
   reported native 1-second default. Jeep tire/body debris and rocket explosion
   DTS shape require their separate model/debris adapters. The pack lists these
   unresolved resources; they have not been replaced by fake geometry.
4. Exact trajectories differ from the engine-family explicit/semi-implicit Euler
   update because the native implementation solves linear drag analytically.
   Seeded replay is deterministic for the same update/input sequence; it is not
   the proprietary RNG sequence or a claim of bit-identical particle timing.
   Budget degradation skips cosmetic emissions and is explicitly counted.
5. Root's actual material shader must implement the dynamic light buffer.
   Example quadratic attenuation is unshadowed, and original fixed-function
   falloff/occlusion must be verified. Flares need host line-of-sight queries;
   they deliberately bypass fragment depth like the inspected engine-family code.
   sRGB texture/linear blending needs Maxwell's original-vs-native comparison.
6. Prior source diagnostics remain visible: PlayerBubbleEmitter alpha override
   is now adapted; malformed spawn color/spark first-key times were normalized
   by the previous converter. `dragcoeffiecient` / `overrideadvances` are source
   misspellings, not silently renamed. `doDetail`/`doFalloff` are not mapped to a
   native distance-LOD policy; the complete authored density currently renders
   within configured budgets. Exact v20 interpretation remains required.

These deliverables are an integration-ready subsystem and reproducible evidence,
not completion of the full alpha contract or a substitute for Maxwell's playtest.
