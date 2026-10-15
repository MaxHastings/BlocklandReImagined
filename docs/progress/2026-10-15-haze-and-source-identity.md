# 2026-10-15 Independent haze and source identity audit

Maxwell explicitly requested all eight execution phases and a Halo Add-On
playtest only after completing them. The named later weapon/vehicle expansion
is now part of that delivery; it must not be silently treated as optional.
Private halo/EXECUTION-PLAN.md and COVERAGE.md record ordering and acceptance.
The full work and playtest delivery remain incomplete.

## Shared engine correction

Native Fog now has optional density_end, separate from end (geometry visibility).
Old content omits it and retains its existing fog. The shared CPU/WGSL atmosphere
uses it for optical depth on both geometry and sky, without changing visibility.
glTF scene extras expose bri_fog_density_end, bri_fog_height and bri_fog_max.
This is an exponential haze capability, not proof of original CE falloff fidelity.

Checks: all four content environment tests pass. The bounded offscreen
persistent_scene::a_haze_caps_the_fog_and_keeps_far_geometry passes with an earlier
density distance, checking GPU color against CPU fog and retaining geometry past
that distance. Importer unit checks passed before extending its existing sky/map
test to cover the new extras; the current focused run covers the extension.

## Checkpoint failure traced, not waived

The earlier focused run completed 39 steps in 1004 seconds, including all selected
clippy checks, then failed bri-net crossover_soak. Host timing samples were empty,
although clients received hundreds of poses. The sampler only ran when a catch-up
batch ended on an exact multiple of 120 ticks; batches could miss every boundary.
It now samples a crossed boundary once, in crossover and all-features soaks. Added
an explicit empty-sample failure to crossover; no performance threshold changed.
The crossover retry passed: mean 120.0 ticks/s, slowest 119.2, mean tick .95 ms.
It also logged an existing sample-commando on_death undefined streak for a killer;
that package warning remains a separate open issue, not fixed or waived here.

The fresh focused run stopped after 41 steps in 1253 seconds. All selected clippy
checks passed, along with client unit tests and the crossover retry within the
full net suite. Net sized_join then failed: 1,788,212 B/s through a 12,500,000 B/s
throttled link, below its unchanged 30% floor. A standalone retry is pending.
This is unresolved; no assertion was lowered and no failure was waived. Remaining
focused steps still need running. Logs: artifacts/v031-checkpoint-focused-2.log.

## Private source audit and binding

Private read-only inventory recovered 2408 Blood Gulch and 3724 campaign tags,
with 6996/9607 dependency references and no unreadable metadata records. This is
an indexed dependency inventory, not a completed semantic or fidelity audit.

The presentation extractor previously keyed only by path. CE lights/lens flares,
and several other classes, share paths; entries could overwrite each other.
Private extraction and its consumers now use class-qualified identities. A staged
rifle/Warthog conversion produced byte-identical packages to the current candidates,
so their current effects did not change from this identity correction. Expanded
recovery now retains 869 descriptions and 376 original bitmap tags including
meter, layered transparent, glass, environment, plasma and sky materials. Original
decoded input and raw reader logs remain in ignored research, outside public Git.

Original locomotion root metadata was missing from the converted body aliases.
Private importer now validates each alias against all original native skeletal
channels and frame duration before applying recovered cumulative stride metadata.
17 aliases across Chief/Elite/Grunt now supply the existing velocity-based playback
mechanism. No skeletal tracks changed. This does not close slope or transition
visual review. Original-map hashes and per-alias evidence are private provenance.

Private map now binds original warm haze color, cap and scaled start/density range
while keeping its existing visible distance. Native height/exponential behavior,
indoor transitions and sun/lens composition remain unverified; explicit provenance
retains those gaps. This content requires the new engine; it is not installed.

The old private tool fixture expected a borrowed body action. It now checks that
stock tool socket motion moves the item while every foreign body node retains its
own pose. The actual Chief/Elite/Grunt fixture passes. This is regression evidence,
not human acceptance or original-game fidelity certification.

No latest executable rebuild/install, public commit/push/merge/tag/publication,
visible gameplay or interactive input occurred. Original installations unchanged.
