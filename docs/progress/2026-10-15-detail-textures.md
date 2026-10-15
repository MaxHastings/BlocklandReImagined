# Detail textures on imported materials

Maxwell asked why Blood Gulch's ground looked so low resolution. Halo
stretches one colour map over the whole valley and repeats small detail
maps over it up to 100 times; imported models took one texture, so the
detail had been squeezed into the stretched one.

A model's full material (`bri_content::shape::Surface`) now takes a
`detail` texture and `detail_scale`, read from glTF material extras
(`bri_detail`, a texture reference plus `scale`). The Physical shader
samples it at the UV times the scale in linear and multiplies the colour by
twice it, so mid-grey leaves the colour unchanged and mipmapping fades it
to neutral with distance. It sits in material slot 6 with its scale and
strength in `parameters[2]`. Documented in docs/modding/maps.md.

Checked with an offscreen render of Blood Gulch: the cliffs and base metal
show their grain. The ground stays soft up close, because Halo's own sand
detail is 256 pixels over about 6 units.
