# Native presentation bindings

The engine supplies attachment frames, native model sampling, conditional
appearance selection and cone lighting. Add-Ons choose what those mean.
No renderer recognizes headlights, brake lights, a particular vehicle or a
particular item.

## Shared cone geometry

Vehicle lamps, native effect lights and mounted image `light` definitions accept
the same optional `cone`:

```json
{"inner":15,"outer":35,"axis":[0,0,-1]}
```

Angles are degrees from the source-local axis: full within `inner`, fading
out toward `outer`. The attachment's full rotation carries the axis into the
world. Omit the cone for an all-round light. The common native light path
supports a vehicle spotlight, a held flashlight and an independently placed
effect source; particle illumination uses the same angular attenuation.
Flares retain their separate authored appearance and visibility rules.

## Conditions and model appearances

`bri_content::presentation::Condition` combines named source facts: every
`all` entry, at least one `any` entry when present, and no `none` entry.
Adapters validate supported names before admission. Unknown inputs are errors,
including under `none`; they are not interpreted as absent or false.
There are at most 32 inputs per condition. This is a bounded data expression,
not a script evaluator or a new authoritative state store.

The vehicle adapter exposes `switch`, `occupied`, `jump`, `brake`, `reverse`
and `destroyed`. `reverse` means negative throttle input, not measured backward
velocity. `jump` and `brake` remain independent. A lamp can retain its existing
activation name or supply a condition:

```json
{"activation":{"when":{"all":["occupied"],"any":["brake","reverse"]}}}
```

The driver's light key toggles `switch` when the live vehicle has lamps or
an appearance condition consuming that signal. A visual-only indicator does
not need a dummy lamp. Passengers do not control this switch.

Destroyed vehicles stop emitting lamps. A vehicle definition's independent
`visuals` list selects authored model-object appearances:

```json
{
  "visuals":[{
    "objects":["indicator"],
    "when":{"all":["occupied"],"any":["brake","reverse"],"none":["destroyed"]},
    "on":{},
    "off":{"color":[0,0,0,255]}
  }]
}
```

An empty appearance preserves authored textures. `color` replaces the base
texture of only the named objects; it does not repaint other objects sharing
their material. Authored shading and alpha rules still apply. `visible:false`
hides that appearance, allowing alternate authored geometry. The same binding
can select an indicator, screen, cover, lens or another model part without a
corresponding light. Conditions and appearance types are shared native data;
the vehicle facts above are the currently integrated adapter, not a promise
that every source already exposes every binding.

Each definition admits at most 64 bindings, each referencing at most 64 object
names. Duplicate object bindings are rejected. A loaded model admits at most
256 prebuilt appearance states across its definitions; exceeding the budget
reports an asset fault. Missing object and sequence names are diagnosed.
Definitions sharing a model choose their own appearances; a definition without
a binding retains the object's authored appearance.

## Animation and grips

`steering_sequence` names a body clip scrubbed from full right at phase 0 to
full left at phase 1. Endpoints hold even if the source clip loops. Ordinary
threads, look, steering and additive acceleration overlays compose in that
order through the existing native sampler. Geometry, grip nodes and visual
mount-node deltas use that pose, including model scale. Static seat offsets
remain authored relative to the mount's rest frame; host seat physics is
unchanged by cosmetic animation.

Seat hand nodes can resolve on the eligible weapon attachment or the body.
Only the weapon seat resolves attachment grips. Body grips carry their turn
since rest in world space; the arm reach uses the actual drawn body's inverse
frame. A raised tool hand releases its grip; a supporting two-handed tool
releases both. Stowing resumes the authored grips.

`on_vehicle_lights` remains a driver-authorized control-operation notification,
following the established declared rule-hook mechanism. It does not choose
materials, play a particular sound, implement a horn, or own a rendering system.
