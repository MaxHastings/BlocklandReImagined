//! A masked tint paints the vertex colour only where the mask's red channel
//! says; the unmasked texels keep the texture as it is.
#[path = "unified_lighting/common.rs"]
mod common;
use anyhow::Result;
use bri_render::scene::*;

fn plane(mask: u8, tint: [f32; 4], masked: bool, kind: MaterialKind) -> SceneData {
    let mut material = Material::surface("armor", 0, 0);
    material.kind = kind;
    material.images = [0; 13];
    let mut stand_ins = vec![];
    if kind == MaterialKind::Physical {
        let mut maps = [1; 7];
        for (k, map) in SurfaceMap::ALL.into_iter().enumerate().skip(1) {
            stand_ins.push(map.stand_in());
            maps[k] = 1 + k;
        }
        material = Material::physical(
            "armor",
            0,
            maps,
            &bri_content::shape::Surface {
                normal: None,
                normal_scale: 1.0,
                metallic_roughness: None,
                metallic: 0.0,
                roughness: 1.0,
                occlusion: None,
                occlusion_strength: 1.0,
                emissive_map: None,
                detail: None,
                detail_scale: 1.0,
                emissive: [0.0; 3],
                reflection: None,
                lightmap: None,
                lightmap_strength: 1.0,
            },
        );
    }
    material.images[10] = 1;
    material.tint_mask = masked;
    let mut data = SceneData {
        images: vec![
            SceneImage {
                label: format!("plating {mask}"),
                width: 1,
                height: 1,
                rgba: vec![200, 200, 200, 255],
                srgb: true,
            },
            SceneImage {
                label: format!("change-colour mask {mask}"),
                width: 1,
                height: 1,
                rgba: vec![mask, 0, 0, 255],
                srgb: false,
            },
        ],
        materials: vec![material],
        vertices: [
            [-10., 0., -10.],
            [-10., 0., 10.],
            [10., 0., 10.],
            [10., 0., -10.],
        ]
        .map(|position| SceneVertex {
            position,
            normal: [0., 1., 0.],
            uv: [0.5; 2],
            lightmap_uv: [0.; 2],
            color: tint,
            fx: [0.; 4],
        })
        .to_vec(),
        indices: vec![0, 1, 2, 0, 2, 3],
        batches: vec![MeshBatch {
            indices: 0..6,
            material: 0,
            center: [0.; 3],
        }],
        ..Default::default()
    };
    data.images.extend(stand_ins);
    data
}

#[test]
fn the_colour_lands_only_where_the_mask_is_set() -> Result<()> {
    let (device, queue) = common::gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = SceneRenderer::new(&device, format);
    let target = common::color_target(&device, format, 64, 64);
    let red = [1., 0., 0., 1.];
    for kind in [MaterialKind::VertexLit, MaterialKind::Physical] {
        for modern in [false, true] {
            let mut camera = Camera::perspective([0., 3., 0.01], [0.; 3], 1., 1., 0.05, 100.);
            if modern {
                camera.shading[0] = 1.;
                camera.ambient[3] = 1.;
            }
            renderer.update_camera(&queue, &camera);
            let mut pixel = |mask: u8, tint: [f32; 4], masked: bool| -> Result<Vec<u8>> {
                let scene = renderer.upload(&device, &queue, &plane(mask, tint, masked, kind))?;
                let pixels =
                    common::render(&device, &queue, &mut renderer, &target, &[&scene], &[], &[])?;
                Ok(pixels[(32 * 64 + 32) * 4..][..4].to_vec())
            };
            let plain = pixel(0, [1.; 4], false)?;
            let full = pixel(255, red, true)?;
            let none = pixel(0, red, true)?;
            let half = pixel(128, red, true)?;
            let label = format!("{kind:?} modern={modern}");
            assert!(plain[1] > 40, "{label}: plain {plain:?}");
            assert!(
                full[1] < plain[1] / 4 && full[2] < plain[1] / 4 && full[0] > 3 * full[1],
                "{label}: full {full:?}"
            );
            for c in 0..3 {
                assert!(
                    (i32::from(none[c]) - i32::from(plain[c])).abs() <= 3,
                    "{label}: an unset mask leaves the texel: {none:?} vs {plain:?}"
                );
            }
            assert!(
                half[1] > full[1] + 10 && half[1] < plain[1] - 10,
                "{label}: half {half:?}"
            );
        }
    }
    Ok(())
}
