# Blend modes of a map shape's materials

A static map shape's material (`Material::blend` in a shape's JSON, as read by the map loader) is one of:

| `blend` | Draws |
| --- | --- |
| `opaque`, `alpha`, `additive` | as before: no blending, source-alpha blending (cut-outs become a mask), source-alpha added to the surface |
| `multiply` | the texture times the vertex colour multiplies the surface under it, fading to the unchanged surface with alpha |
| `modulate` | 2 x texture x surface (blend factors destination colour, source colour); fades to a neutral output of 0.5 in linear light, which leaves the surface unchanged |
| `darken` | the surface times (1 - texture x alpha) (factors zero, 1 - source colour) |
| `screen` | texture x alpha + surface x (1 - texture x alpha) (factors one, 1 - source colour) |
| `premultiplied` | colour x alpha over the surface (factors one, 1 - source alpha) |
| `invisible` | nothing (factors zero, one) |
| `true_additive` | surface + texture x alpha (factors one, one) |

These are Unreal Engine 2's `FinalBlend.FrameBufferBlending` modes. Their factors were read from the original D3DDrv.dll: the switch on
the blend byte at RVA 0x9e6f of the pinned build (sha256 58327df42932678736d1df277ece62f6dc0b4b96461da8178f63a412bf6bc144) writes the
source and destination blend factors of the draw's render state (D3D numbering: 1 zero, 2 one, 3 source colour, 4 1 - source colour,
5 source alpha, 6 1 - source alpha, 9 destination colour), and the mode names come from `EFrameBufferBlending` in `Engine.u`:

| mode (index) | source | destination |
| --- | --- | --- |
| Overwrite (0) | one | zero, blending off |
| Modulate (1) | destination colour | source colour |
| AlphaBlend (2) | source alpha | 1 - source alpha |
| AlphaModulate_MightNotFogCorrectly (3) | one | 1 - source alpha |
| Translucent (4), BrightenSmooth (9) | one | 1 - source colour |
| Darken (5) | zero | 1 - source colour |
| Brighten (6) | source alpha | one |
| Invisible (7) | zero | one |
| TrueModulated (8) | zero | source colour |
| TrueAdditive (10) | one | one |

Overwrite, AlphaBlend, Brighten (`additive`) and TrueModulated (`multiply`) use the older modes; Translucent and BrightenSmooth share
`screen`. Not established: the grey 127 constant D3DDrv sets for Modulate and
TrueModulated (read here as Modulate's neutral colour), the exact fog behaviour of AlphaModulate, and whether the originals light
these surfaces (they are drawn unlit here).
