//! The one place an imported shape material's blend, cutout and sidedness become
//! a render material. Every loader of shapes (map static models, held items,
//! avatars, debris) calls [`apply`], so a property honoured for one cannot be
//! lost on another, and a blend nobody can honour is refused, never drawn opaque.
use crate::scene::{AlphaMode, Material};
use anyhow::{Result, bail};
use bri_content::shape::Material as ShapeMaterial;

/// Sets `material`'s alpha mode, texture-alpha use and sidedness from `source`.
///
/// `subject` names the model in refusals. A cutout (`alpha_cutoff`) is always
/// [`AlphaMode::Mask`] with the texture's alpha kept: the cutoff is the
/// material's own test, whatever the loader's habit for its blend. Without a
/// cutoff `plain` decides, given the authored blend (`"opaque"`, `"alpha"` or
/// `"additive"`; anything else is refused before `plain` runs).
pub fn apply(
    material: &mut Material,
    source: &ShapeMaterial,
    subject: &str,
    plain: impl FnOnce(&str) -> Result<AlphaMode>,
) -> Result<()> {
    let blend = source.blend.as_str();
    if !matches!(blend, "opaque" | "alpha" | "additive") {
        bail!("Unsupported {subject} blend {blend}");
    }
    material.double_sided = source.double_sided;
    match source.alpha_cutoff {
        Some(_) if blend == "additive" => {
            bail!(
                "{subject}/{}: an alpha cutoff on an additive material has no meaning here",
                source.name
            )
        }
        Some(cutoff) => {
            material.alpha = AlphaMode::Mask(cutoff);
            material.ignore_texture_alpha = false;
        }
        None => material.alpha = plain(blend)?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_content::testing::material as shape_material_with;
    fn shape_material(name: &str) -> ShapeMaterial {
        shape_material_with(name, "opaque")
    }

    #[test]
    fn a_cutout_keeps_its_cutoff_and_its_texture_alpha() {
        let mut source = shape_material("leaf");
        source.blend = "opaque".into();
        source.alpha_cutoff = Some(0.47);
        source.double_sided = true;
        let mut material = Material::vertex_lit("leaf", 0);
        material.ignore_texture_alpha = true;
        apply(&mut material, &source, "tree", |_| Ok(AlphaMode::Opaque)).unwrap();
        assert_eq!(material.alpha, AlphaMode::Mask(0.47));
        assert!(!material.ignore_texture_alpha);
        assert!(material.double_sided);
    }

    #[test]
    fn without_a_cutoff_the_loader_decides() {
        let mut source = shape_material("glass");
        source.blend = "alpha".into();
        let mut material = Material::vertex_lit("glass", 0);
        apply(&mut material, &source, "x", |b| {
            assert_eq!(b, "alpha");
            Ok(AlphaMode::Blend)
        })
        .unwrap();
        assert_eq!(material.alpha, AlphaMode::Blend);
        assert!(!material.double_sided);
    }

    #[test]
    fn a_blend_nobody_can_honour_is_refused() {
        let mut source = shape_material("odd");
        source.blend = "multiply".into();
        let mut material = Material::vertex_lit("odd", 0);
        let error = apply(&mut material, &source, "tree", |_| Ok(AlphaMode::Opaque)).unwrap_err();
        assert!(error.to_string().contains("multiply"));
        source.blend = "additive".into();
        source.alpha_cutoff = Some(0.5);
        assert!(apply(&mut material, &source, "tree", |_| Ok(AlphaMode::Opaque)).is_err());
    }
}
