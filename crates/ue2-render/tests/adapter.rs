use bri_render::dynamic_sprites::{Combine, Factor, Frames, ImageId, Topology};
use bri_ue2_emitters::props::DrawStyle;
use bri_ue2_emitters::sprite::{SpriteDraw, SpriteVertex};
use bri_ue2_render::{SceneFog, state, vertices};
use glam::{Mat4, Vec3};

fn draw(style: DrawStyle, blend_subdivisions: bool) -> SpriteDraw {
    SpriteDraw {
        state: style.draw_state(),
        blend_subdivisions,
        alpha_test: true,
        alpha_ref: 51,
        z_test: true,
        z_write: false,
        two_sided: true,
        accepts_projectors: true,
        local_to_world: None,
    }
}

#[test]
fn draw_styles_become_generic_batch_states() {
    let fog = Some(SceneFog {
        start: 10.0,
        end: 100.0,
        color: [0.5, 0.6, 0.7],
    });
    let s = state(
        &draw(DrawStyle::Translucent, false),
        ImageId(3),
        Topology::Triangles,
        fog,
    );
    assert_eq!(s.blend, Some((Factor::One, Factor::One)));
    assert_eq!(s.color, Combine::Modulate);
    assert_eq!(s.frames, Frames::Single);
    assert!(s.depth_test && !s.depth_write);
    assert!((s.alpha_test.unwrap() - 0.2).abs() < 1e-6);
    // Additive draws fade to black fog, not the scene's colour; the range is the scene's.
    let f = s.fog.unwrap();
    assert_eq!((f.start, f.end, f.color), (10.0, 100.0, [0.0; 3]));

    let m = state(
        &draw(DrawStyle::Modulated, true),
        ImageId(0),
        Topology::Lines,
        None,
    );
    assert_eq!(m.blend, Some((Factor::DstColor, Factor::SrcColor)));
    assert_eq!(m.color, Combine::BlendByVertexAlpha);
    assert_eq!(m.alpha, Combine::BlendByVertexAlpha);
    assert_eq!(m.frames, Frames::ByVertexAlpha);
    assert_eq!(m.topology, Topology::Lines);
    assert!(m.fog.is_none());
}

#[test]
fn vertices_unpack_colour_transform_and_swap_frames() {
    let v = SpriteVertex {
        position: Vec3::new(1.0, 0.0, 0.0),
        color: 0x80ff4000,
        uv: [0.0, 0.0],
        uv2: [1.0, 1.0],
    };
    let m = Mat4::from_translation(Vec3::new(0.0, 5.0, 0.0));
    let out = vertices(&[v], Some(m), true);
    assert_eq!(out[0].position, [1.0, 5.0, 0.0]);
    let c = out[0].color;
    assert!((c[0] - 1.0).abs() < 1e-6 && (c[1] - 64.0 / 255.0).abs() < 1e-6);
    assert!((c[3] - 128.0 / 255.0).abs() < 1e-6);
    // Frame blending: the second frame's coordinates lead, the first mixes behind it.
    assert_eq!((out[0].uv, out[0].uv2), ([1.0, 1.0], [0.0, 0.0]));
    let plain = vertices(&[v], None, false);
    assert_eq!((plain[0].uv, plain[0].uv2), ([0.0, 0.0], [1.0, 1.0]));
}

#[test]
fn a_projector_volume_becomes_a_depth_projection() {
    use bri_render::projected_textures::{ProjectionBlend, ProjectorImage};
    use bri_ue2_emitters::projector::{FrameBufferBlend, ProjectorProps, calc_matrix};
    use bri_ue2_emitters::rotation::Rotator;
    use bri_ue2_render::projection;
    let volume = calc_matrix(&ProjectorProps {
        location: Vec3::new(0.0, 0.0, 0.0),
        rotation: Rotator::new(0, 0, 0),
        draw_scale: 1.0,
        texture_size: (64, 64),
        fov: 0,
        max_trace_distance: 100,
        rotating: false,
        gradient_height: Some(32),
    })
    .unwrap();
    let p = projection(
        &volume,
        ProjectorImage(0),
        Some(ProjectorImage(1)),
        FrameBufferBlend::Add,
        false,
    );
    assert_eq!(p.blend, ProjectionBlend::Add);
    assert!(!p.divide);
    assert_eq!(p.planes.len(), 6);
    assert!(p.gradient.is_some());
    assert_eq!(p.normal_fade, Some(Vec3::X));
    let off = projection(
        &volume,
        ProjectorImage(0),
        None,
        FrameBufferBlend::Alpha,
        true,
    );
    assert!(off.normal_fade.is_none() && off.gradient.is_none());
}
