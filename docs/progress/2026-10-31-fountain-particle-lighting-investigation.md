# Fountain particle lighting investigation

Maxwell's night screenshot shows blue fountain-like particles remaining bright
against dark surroundings. The screenshot does not identify the save or the
emitter. A plausible match is the supplied Beta16 city tower save: its fountain
has three `v20/emitter/wateremittera` sources and eight
`v20/emitter/playerfoamemitter` sources. The former emits
`v20/particle/waterparticlea`, a blue-to-white alpha-blended water/cloud sprite;
the latter emits `v20/particle/playerfoamparticle`, a bubble. In the preview.3
effect pack, both particles have `lit: false` and are consequently self-lit.
This is a likely match, not proof that the screenshot uses that exact build.

More broadly, the shipped stock effect pack contains 119 particles and none is
marked lit; the stock converter currently assigns `lit: false` unconditionally.
The existing engine supports an explicit lit particle and modder-authored effects
can use it, but lit sprites currently receive scene ambient, unshadowed sunlight
and point lights, not geometry shadows or map irradiance. The recovered source
declarations for Water A and Player Foam specify appearance and blending but no
lighting field, so they do not alone establish original-game lighting semantics.

Maxwell subsequently chose to leave particle lighting unchanged for v0.4.3.
A future investigation can consider a separately recorded, data-driven policy
for ordinary water and foam, preserve luminous effects, and test dark, sun and
local-light cases. Do not silently change every particle or claim indoor
shadowing from the existing diffuse path. No GPU or interactive test was run for
this inspection; converted content and the original install were not changed.
