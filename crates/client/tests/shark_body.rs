//! Real converted Shark body evidence; no window, GPU, audio or gameplay input.
use anyhow::{Context, Result, ensure};
use bri_client::avatar::{AvatarAnimationInput, AvatarAssets, HeldToolPose};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use std::path::PathBuf;

const MODEL: &str = "bot_shark:asset/shark.dts";

#[test]
#[ignore = "requires generated v20 content including Bot_Shark; set BRI_CONTENT"]
fn converted_shark_body_loads_its_textures_and_finite_scaled_geometry() -> Result<()> {
    let root = PathBuf::from(std::env::var_os("BRI_CONTENT").context("set BRI_CONTENT")?);
    let mut packages = PackageSet::load_root(&root)?;
    packages.packages.retain(|p| p.id != "bot_shark");
    let avatar = packages.role_dir(&root, "avatar")?;
    let mut assets = AvatarAssets::load(&avatar)?;
    ensure!(
        !assets.has_body(MODEL),
        "Shark is absent before enabling it"
    );
    packages.packages.push(PackageEntry {
        id: "bot_shark".into(),
        version: "1.0.0".into(),
        side: Side::Shared,
        dir: "addons/bot_shark".into(),
        role: None,
    });
    assets.load_bodies(&root, &packages);
    ensure!(assets.has_body(MODEL), "enabled real Shark model loaded");

    let dir = root.join("addons/bot_shark");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("package.json"))?)?;
    ensure!(manifest["id"] == "bot_shark");
    let content: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("assets/content.json"))?)?;
    let entries = content["content"].as_array().context("content entries")?;
    let file = |id: &str| -> Result<PathBuf> {
        let entry = entries
            .iter()
            .find(|entry| entry["id"] == id)
            .with_context(|| format!("missing {id}"))?;
        Ok(dir.join(entry["file"].as_str().context("asset file")?))
    };
    let shape: bri_content::shape::Shape = serde_json::from_slice(&std::fs::read(file(MODEL)?)?)?;
    shape.validate()?;
    ensure!(
        shape.nodes.len() == 12 && shape.objects.len() == 6,
        "converted Shark rig identity changed"
    );
    ensure!(
        shape
            .animations
            .iter()
            .any(|a| a.name == "swim" && a.looping),
        "authored swim loop retained"
    );
    ensure!(
        shape.animations.iter().any(|a| a.name == "biteFix"),
        "authored bite sequence retained"
    );

    let mut full_extent = None;
    for scale in [1.0, 0.25, 2.0] {
        let mut mesh = assets.body_mesh(MODEL, assets.package.defaults.clone())?;
        ensure!(
            mesh.model.as_deref() == Some(MODEL),
            "no Blockhead fallback"
        );
        let player: bri_sim::player::PlayerState = serde_json::from_value(serde_json::json!({
            "owner": 1, "feet": [0.0,0.0,0.0], "velocity": [0.0,0.0,0.0],
            "yaw": 0.0, "pitch": 0.0, "grounded": true, "crouched": false,
            "jetting": false, "scale": scale
        }))?;
        mesh.pose(&assets, &player, 0.0)?;
        ensure!(
            !mesh.data.vertices.is_empty() && !mesh.data.indices.is_empty(),
            "real Shark has visible posed geometry"
        );
        let mut min = glam::Vec3::splat(f32::INFINITY);
        let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
        for vertex in &mesh.data.vertices {
            ensure!(
                vertex
                    .position
                    .iter()
                    .chain(&vertex.normal)
                    .all(|v| v.is_finite()),
                "finite body geometry at scale {scale}"
            );
            let p = glam::Vec3::from_array(vertex.position);
            min = min.min(p);
            max = max.max(p);
        }
        let extent = max - min;
        ensure!(
            extent.min_element() > 0.0,
            "body has three-dimensional extent"
        );
        if let Some(unit) = full_extent {
            ensure!(
                extent.abs_diff_eq(unit * scale, 0.001),
                "rendered scale tracks player scale"
            );
        } else {
            full_extent = Some(extent);
        }
        for material in &shape.materials {
            let name = material.name.to_ascii_lowercase();
            let png = if name.ends_with(".png") {
                name.clone()
            } else {
                format!("{name}.png")
            };
            let original =
                image::load_from_memory(&std::fs::read(file(&format!("bot_shark:asset/{png}"))?)?)?
                    .to_rgba8();
            let loaded = mesh
                .data
                .images
                .iter()
                .find(|image| image.label == format!("{MODEL}/{name}"))
                .context("original Shark material loaded")?;
            ensure!(
                loaded.width == original.width()
                    && loaded.height == original.height()
                    && loaded.rgba == original.as_raw().as_slice(),
                "material {name} retains original pixels"
            );
        }
        eprintln!(
            "{MODEL}: scale {scale}, {} vertices, {} indices, extent {extent}, {} total scene images",
            mesh.data.vertices.len(),
            mesh.data.indices.len(),
            mesh.data.images.len()
        );
    }
    let player: bri_sim::player::PlayerState = serde_json::from_value(serde_json::json!({
        "owner": 1, "feet": [0.0,0.0,0.0], "velocity": [0.0,0.0,0.0],
        "yaw": 0.0, "pitch": 0.0, "grounded": false, "crouched": false,
        "jetting": false, "scale": 1.0
    }))?;
    let wet = AvatarAnimationInput {
        water_coverage: 1.0,
        ..Default::default()
    };
    let mut swimming = assets.body_mesh(MODEL, assets.package.defaults.clone())?;
    swimming.pose_with_animation(&assets, &player, 0.0, &wet)?;
    let initial: Vec<_> = swimming.data.vertices.iter().map(|v| v.position).collect();
    swimming.pose_with_animation(&assets, &player, 0.1, &wet)?;
    swimming.pose_with_animation(&assets, &player, 0.2, &wet)?;
    ensure!(
        swimming
            .data
            .vertices
            .iter()
            .zip(initial)
            .any(|(v, before)| {
                glam::Vec3::from_array(v.position).distance(glam::Vec3::from_array(before)) > 0.001
            }),
        "real authored swim loop moves geometry over time, without script cues"
    );
    for (i, held_tool_pose) in [HeldToolPose::Right, HeldToolPose::Left, HeldToolPose::Both]
        .into_iter()
        .enumerate()
    {
        swimming.pose_with_animation(
            &assets,
            &player,
            0.3 + i as f64 * 0.1,
            &AvatarAnimationInput {
                held_tool_pose,
                ..wet.clone()
            },
        )?;
        ensure!(
            swimming
                .data
                .vertices
                .iter()
                .all(|v| v.position.iter().all(|x| x.is_finite())),
            "held tool preserves finite authored body pose"
        );
    }
    Ok(())
}
