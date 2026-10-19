//! Rendering the scene each frame.
use super::*;
use bri_console::Clamp;

impl App {
    pub(super) fn render_frame(&mut self, frame: &mut RenderContext<'_>) -> Result<bool> {
        self.fx.weapon_frame_observation = None;
        let span = crate::frame_trace::span("prepare");
        self.prepare_render(frame)?;
        drop(span);
        let Some(a) = self.net.attempt.as_ref().filter(|a| a.entered) else {
            return Ok(false);
        };
        let Some(scene) = &self.scene.cpu_scene else {
            return Ok(false);
        };
        let Some(view) = &a.view else {
            return Ok(false);
        };
        let Some(local) = self.motion.presented().get(&view.owner) else {
            return Ok(false);
        };
        // Draw what this frame's tick posed, not input that arrived since.
        let controls = self.view.drawn_controls.as_ref().unwrap_or(&self.controls);
        let stock_third_person = draws_third_person(
            controls,
            view.vitals.get(&view.owner).is_none_or(|v| v.alive),
        );
        let third_person = stock_third_person || self.view.addon_camera;
        let mut hidden = self.combat.hidden_bodies(&view.vitals);
        // Players whose archetype looks like a package model draw as it, in
        // place of the Blockhead; the local player's in first person only
        // in other views (mirrors, portals), as the Blockhead does.
        let package_catalog = packages_for(&self.addons.package_catalog, view);
        let mut package_placements: Vec<_> =
            crate::packages::entity_placements(self.ghosts.entities_at(view.tick, &view.entities))
                .collect();
        let mut own_package_body = None;
        if let Some(catalog) = package_catalog {
            for (owner, mut placement) in
                crate::packages::body_placements(catalog, &view.archetypes, self.motion.presented())
            {
                hidden.insert(owner);
                // Seated, tilted with the seat and sitting, as its
                // stand-in mesh times it.
                if let Some(p) = &mut placement
                    && let Some(mesh) = self.avatar.avatars.get(&owner)
                    && mesh.body_kind().stand_in
                {
                    p.pose = Some(mesh.box_pose());
                }
                match placement {
                    Some(p) if owner == view.owner && !third_person => own_package_body = Some(p),
                    Some(p) => package_placements.push(p),
                    None => {}
                }
            }
        }
        // The world's pipelines compile on a worker (seconds with FXC).
        // Entering a game and finishing a map change wait for them on the
        // loading screen (`scene_pipelines_ready`), so a frame of a game
        // that is in reaches here with them compiled; only a GPU opened
        // after entering (a lost device, an offscreen capture) waits here.
        let building = self
            .gpu
            .renderer
            .as_mut()
            .context("Scene GPU not initialized")?;
        if building.ready().is_none() {
            crate::frame_trace::note("waited for the world's pipelines to compile");
        }
        let span = crate::frame_trace::span("pipelines wait");
        let renderer = building.wait();
        drop(span);
        if let Some(mut owner) = self.gpu.retired_renderer.take()
            && let Err(error) = renderer.adopt_scenes(&mut owner)
        {
            bri_console::warn(format!("Uploading the world again: {error:#}"));
            crate::frame_trace::note(format!("kept no world: {error:#}"));
            self.gpu.gpu_scene = None;
            self.gpu.gpu_terrain.clear();
            self.gpu.gpu_water.clear();
            self.gpu.gpu_palette = None;
            self.gpu.gpu_chunks.clear();
        }
        renderer.set_filtering(frame.device, self.graphics.filtering);
        // Player sessions time the passes all the time, for the session
        // log's GPU line (`crate::quality::FrameLog::gpu`).
        let timing =
            self.gpu.time_passes || self.ui.core.perf.wants_net() || self.perf.frame_log.is_some();
        renderer.time_passes(frame.device, frame.queue, timing);
        let readings = renderer.pass_readings();
        match renderer.pass_times(frame.device) {
            Some((whole, passes)) => {
                if renderer.pass_readings() != readings
                    && let Some(log) = &mut self.perf.frame_log
                {
                    log.gpu(whole, &passes);
                }
                self.gpu.gpu_passes = passes
                    .iter()
                    .map(|(pass, time)| (*pass, time.as_secs_f32() * 1000.0))
                    .collect();
            }
            None => self.gpu.gpu_passes.clear(),
        }
        if self.gpu.gpu_scene.is_none() {
            let _span = crate::frame_trace::span("world upload");
            crate::frame_trace::note("uploaded the map");
            self.gpu.gpu_broken.clear();
            self.gpu.gpu_scene = Some(renderer.upload(frame.device, frame.queue, scene)?);
            self.gpu.decal_layer = self.scene.decals.as_ref().map(|loaded| {
                let layer = bri_render::decals::DecalLayer::new(
                    frame.device,
                    frame.queue,
                    frame.format,
                    bri_render::scene::DEPTH_FORMAT,
                    renderer.samples(),
                    loaded,
                );
                crate::frame_trace::note(format!(
                    "uploaded {} decals ({} triangles)",
                    loaded.set.decals.len(),
                    layer.triangle_counts().iter().sum::<usize>()
                ));
                layer
            });
            self.gpu.gpu_backdrop = match &self.scene.backdrop {
                Some(backdrop) => {
                    Some(renderer.upload(frame.device, frame.queue, &backdrop.scene)?)
                }
                None => None,
            };
            self.lighting.light_volume.uploaded = false;
            self.gpu.gpu_terrain = self
                .scene
                .cpu_terrain
                .iter()
                .map(|terrain| {
                    bri_render::terrain_scene::GpuTerrain::upload(
                        renderer,
                        frame.device,
                        frame.queue,
                        terrain.clone(),
                        FAR_PLANE,
                    )
                })
                .collect::<Result<_>>()?;
            self.gpu.gpu_water = self
                .scene
                .cpu_water
                .iter()
                .map(|water| bri_render::water_scene::GpuWater::new(water.clone(), FAR_PLANE))
                .collect::<Result<_>>()?;
        }
        let previous_lighting = self.lighting.light_volume.bound_mode;
        let span = crate::frame_trace::span("light volume");
        self.lighting.light_volume.upload(
            renderer,
            frame.device,
            frame.queue,
            self.graphics.lighting,
        )?;
        drop(span);
        if previous_lighting != self.lighting.light_volume.bound_mode {
            // A cached reflection/probe must never carry illumination from
            // another mode into modern pixels (or back into compatibility).
            self.lighting.reflections = None;
            self.lighting.environment_probe = None;
        }
        let effective = self
            .graphics
            .with_lighting(self.lighting.light_volume.mode(self.graphics.lighting));
        if let Some(view) = self.net.attempt.as_ref().and_then(|a| a.view.as_ref()) {
            self.lighting.light_volume.tint(
                renderer,
                frame.queue,
                &view.broken_shapes,
                &view.map_lights,
                // Wall-clock time, so every player's lights blink together.
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0.0, |d| d.as_secs_f64()),
            );
        }
        let span = crate::frame_trace::span("chunk uploads");
        if self.gpu.gpu_palette.is_none()
            && let Some(palette) = &self.scene.palette
        {
            self.gpu.gpu_palette =
                Some(renderer.upload(frame.device, frame.queue, &palette.scene)?);
            self.gpu
                .chunk_uploads
                .extend(self.scene.cpu_chunks.keys().copied());
        }
        if let Some(palette) = &self.gpu.gpu_palette {
            let pending: Vec<&SceneData> = self
                .gpu
                .chunk_uploads
                .iter()
                .filter_map(|key| self.scene.cpu_chunks.get(key))
                .collect();
            let vertices: usize = pending.iter().map(|c| c.vertices.len()).sum();
            if !pending.is_empty() {
                note_chunk_uploads(pending.len(), vertices);
            }
            if vertices > 1 << 16 {
                renderer.reserve_chunks(&pending)?;
            }
            for key in std::mem::take(&mut self.gpu.chunk_uploads) {
                if let Some(chunk) = self.scene.cpu_chunks.get(&key) {
                    self.gpu.gpu_chunks.insert(
                        key,
                        renderer.upload_chunk(frame.device, frame.queue, chunk, palette)?,
                    );
                    if let Some(bricks) = self.scene.cpu_chunk_bricks.get(&key) {
                        self.gpu.gpu_chunk_bricks.insert(key, bricks.clone());
                    }
                    // A chunk built before a brick died still draws it.
                    for (hidden_key, applied) in self.scene.chunk_hides.values_mut() {
                        if *hidden_key == key {
                            *applied = false;
                        }
                    }
                }
            }
        }
        drop(span);
        // Dead bricks leave their drawn chunks now, not when the rebuilt
        // chunks land. A hide ends once the brick is back (respawned) or
        // the uploaded chunk no longer holds it.
        let (debris, uploads, drawn) = (
            &self.fx.brick_debris,
            &self.gpu.chunk_uploads,
            &self.gpu.gpu_chunk_bricks,
        );
        self.scene.chunk_hides.retain(|brick, (key, _)| {
            let back =
                !debris.is_dead(*brick) && view.world.bricks.get(brick).is_some_and(|b| b.visible);
            !back
                && (uploads.contains(key)
                    || drawn.get(key).is_some_and(|b| b.vertices(*brick).is_some()))
        });
        for (brick, (key, applied)) in &mut self.scene.chunk_hides {
            if *applied {
                continue;
            }
            *applied = true;
            if let (Some(gpu), Some(bricks)) = (
                self.gpu.gpu_chunks.get(key),
                self.gpu.gpu_chunk_bricks.get(key),
            ) && let Some(vertices) = bricks.vertices(*brick)
            {
                gpu.hide_vertices(frame.queue, vertices);
            }
        }
        // Options > Advanced's temp brick colours and flash.
        let ghost_look = crate::world_scene::TempBrickLook::from_prefs(&self.ui.core.prefs);
        if let Some(building) = &self.build.building
            && self.gpu.ghost_uploaded != ghost_key(building)
        {
            // A copied build in hand shows instead of the single ghost.
            let ghosts: Option<Vec<bri_world::Brick>> = match building.copy_ghost() {
                Some(copy) => Some(copy.to_vec()),
                None => building.ghost().map(|g| vec![g.clone()]),
            };
            // Built around the first brick, so a moved ghost (or a world
            // change that leaves it as it was) only moves its transform;
            // only a new look rebuilds it, textures and all.
            let placed = ghosts.map(|mut bricks| {
                let anchor = Vec3::from(bricks[0].position);
                for brick in &mut bricks {
                    brick.position = (Vec3::from(brick.position) - anchor).to_array();
                    // Placement previews show every copied brick, including invisible
                    // triggers. Only these local render copies change; planting retains
                    // the blueprint's authored rendering and collision flags.
                    crate::world_scene::show_placement_ghost(brick);
                }
                let look = GhostLook {
                    bricks,
                    blocked: building.ghost_blocked(),
                    temp: ghost_look,
                    palette: view.world.palette.clone(),
                };
                (anchor, look)
            });
            match &placed {
                Some((_, look)) if self.gpu.ghost_look.as_ref() == Some(look) => {}
                _ => {
                    self.gpu.ghost_gpu = None;
                    self.gpu.ghost_look = None;
                }
            }
            let anchor = placed.as_ref().map(|(anchor, _)| *anchor);
            if let Some((anchor, look)) = placed
                && self.gpu.ghost_look.is_none()
            {
                let world = bri_net::protocol::PublicWorld {
                    name: "Local unplanted ghost".into(),
                    map_id: view.world.map_id.clone(),
                    palette: look.palette.clone(),
                    bricks: look
                        .bricks
                        .iter()
                        .cloned()
                        .enumerate()
                        .map(|(i, b)| (i as u64, b))
                        .collect(),
                };
                let preview = crate::world_scene::build_placement_preview(
                    &world,
                    self.scene
                        .meshes
                        .as_ref()
                        .context("Ghost mesh catalog missing")?,
                    Some(
                        self.scene
                            .materials
                            .as_ref()
                            .context("Ghost material catalog missing")?,
                    ),
                )?;
                if let Some(text) = preview.notice {
                    eprintln!("{text}");
                    self.ui.apply(UiUpdate::Chat { text });
                }
                let mut data = preview.scene;
                // Warn before a plant the server would refuse: the ghost
                // turns red (not in v20, which only showed the error icon).
                if look.blocked {
                    for vertex in &mut data.vertices {
                        vertex.color = BLOCKED_GHOST;
                    }
                }
                translucent_ghost(&mut data, &ghost_look);
                if !data.indices.is_empty() {
                    self.gpu.ghost_gpu = Some((
                        renderer.upload(frame.device, frame.queue, &data)?,
                        bri_render::scene::GpuInstances::new(frame.device, 1)?,
                    ));
                }
                self.gpu.ghost_look = Some(look);
                if let Some((_, instances)) = &mut self.gpu.ghost_gpu {
                    instances.update(
                        frame.queue,
                        &[bri_render::scene::SceneTransform {
                            transform: glam::Mat4::from_translation(anchor),
                            tint: [1.0; 4],
                        }],
                    )?;
                }
            } else if let (Some(anchor), Some((_, instances))) = (anchor, &mut self.gpu.ghost_gpu) {
                instances.update(
                    frame.queue,
                    &[bri_render::scene::SceneTransform {
                        transform: glam::Mat4::from_translation(anchor),
                        tint: [1.0; 4],
                    }],
                )?;
            }
            self.gpu.ghost_uploaded = ghost_key(building);
        }
        // Other players' ghost bricks, translucent in their colour and shape.
        self.build.remote_ghosts.retain(|owner, (ghost, _)| {
            *owner != view.owner
                && view
                    .vitals
                    .get(owner)
                    .and_then(|v| v.ghost.as_ref())
                    .is_some_and(|now| now == ghost)
        });
        for (owner, vitals) in &view.vitals {
            let Some(ghost) = vitals.ghost.as_ref().filter(|_| *owner != view.owner) else {
                continue;
            };
            if self.build.remote_ghosts.contains_key(owner) {
                continue;
            }
            let mut brick = bri_world::Brick::new(
                bri_world::ContentRef::Resolved(ghost.definition.clone()),
                ghost.position,
                *owner,
            );
            brick.quarter_turns = ghost.quarter_turns;
            brick.color = ghost.color;
            brick.print = ghost.print.clone().map(bri_world::ContentRef::Resolved);
            let world = bri_net::protocol::PublicWorld {
                name: "Remote unplanted ghost".into(),
                map_id: view.world.map_id.clone(),
                palette: view.world.palette.clone(),
                bricks: bri_world::Bricks::unit(0, brick),
            };
            // A brick this client cannot draw shows nothing.
            let gpu = match (&self.scene.meshes, &self.scene.materials) {
                (Some(meshes), Some(materials)) => crate::world_scene::build_world_scene_materials(
                    &world,
                    meshes,
                    100_000,
                    Some(materials),
                )
                .ok()
                .filter(|data| !data.indices.is_empty())
                .map(|mut data| {
                    translucent_ghost(&mut data, &ghost_look);
                    renderer.upload(frame.device, frame.queue, &data)
                })
                .transpose()?,
                _ => None,
            };
            self.build
                .remote_ghosts
                .insert(*owner, (ghost.clone(), gpu));
        }
        let shapes_changed = self.shapes_uploaded.as_ref().is_none_or(|sent| {
            sent.len() != view.world_shapes.len()
                || sent
                    .iter()
                    .zip(&view.world_shapes)
                    .any(|((a, x), (b, y))| a != b || !std::sync::Arc::ptr_eq(x, y))
        });
        if shapes_changed && let Some(renderer) = &mut self.world_shapes {
            let mut vertices = vec![];
            for shape in view.world_shapes.values().flat_map(|s| s.iter()) {
                let rgba = |c: [u8; 4]| c.map(|v| f32::from(v) / 255.0);
                bri_render::world_shapes::box_faces(
                    Vec3::from(shape.min),
                    Vec3::from(shape.max),
                    shape.outside().map(rgba),
                    rgba(shape.inside),
                    &mut vertices,
                );
            }
            renderer.set_faces(frame.device, &vertices)?;
            self.shapes_uploaded = Some(view.world_shapes.clone());
        }
        if let Some(building) = &self.build.building
            && let (Some(meshes), Some(materials)) = (&self.scene.meshes, &self.scene.materials)
        {
            // v20 `showBricks` images (hammer, wrench, printer, wands, bricks)
            // reveal non-rendering bricks as box outlines in their paint
            // colour (`fxDTSBrick::renderObject`), not as ghost bricks.
            let show = matches!(
                building.equipment(),
                crate::building::Equipment::Brick(_) | crate::building::Equipment::Tool(_)
            );
            let fading = if show {
                self.fx.brick_fades.outlined()
            } else {
                Vec::new()
            };
            let preview = self
                .ui
                .core
                .wrench
                .open
                .as_ref()
                .filter(|open| open.fill.is_none())
                .map(|open| {
                    (
                        open.brick,
                        self.ui.core.wrench.values(open.variant).rule_region,
                    )
                });
            if let Some(vertices) =
                self.gpu
                    .region_outlines
                    .update(&view.world.bricks, show, preview, |brick| {
                        crate::brick_cover::mesh(brick, meshes)
                    })
                && let Some(lines) = &mut self.gpu.region_lines
            {
                lines.set_lines(frame.device, &vertices)?;
            }
            let debris = &self.fx.brick_debris;
            if let Some(lines) = &mut self.gpu.hidden_lines
                && let Some(vertices) = self.gpu.hidden_outlines.update(
                    &view.world.bricks,
                    &view.world.palette,
                    show,
                    &fading,
                    |id| debris.is_dead(id),
                    |brick| {
                        crate::brick_cover::mesh(brick, meshes)
                            .map(|mesh| hidden_brick_box(brick, mesh))
                    },
                    self.gpu.hidden_uploaded != Some(show),
                )
            {
                lines.set_lines(frame.device, &vertices)?;
                self.gpu.hidden_uploaded = Some(show);
            }
            let selection = self.build.building.as_ref().and_then(|b| b.outline());
            if self.gpu.selection_uploaded != Some(selection)
                && let Some(lines) = &mut self.gpu.selection_lines
            {
                let mut vertices = vec![];
                if let Some((low, high)) = selection {
                    // Just outside the box, so its edges do not fight the
                    // faces of the bricks they frame.
                    let margin = Vec3::splat(0.02);
                    bri_render::lines::box_edges(
                        Vec3::from(low) - margin,
                        Vec3::from(high) + margin,
                        SELECTION_COLOR,
                        &mut vertices,
                    );
                }
                lines.set_lines(frame.device, &vertices)?;
                self.gpu.selection_uploaded = Some(selection);
            }
            if let (Some(palette), Some(gpu_palette)) = (&self.scene.palette, &self.gpu.gpu_palette)
            {
                self.fx.debris_models.upload(
                    &self.fx.brick_debris,
                    renderer,
                    frame.device,
                    frame.queue,
                    meshes,
                    palette,
                    gpu_palette,
                    materials,
                    &view.world.palette,
                )?;
            }
            if let (Some(world), Some(palette), Some(gpu_palette)) = (
                &self.scene.world_source,
                &self.scene.palette,
                &self.gpu.gpu_palette,
            ) {
                self.fx.fade_models.upload(
                    &self.fx.brick_fades,
                    &self.scene.chunks_left_out,
                    world,
                    renderer,
                    frame.device,
                    frame.queue,
                    meshes,
                    palette,
                    gpu_palette,
                    materials,
                )?;
            }
            self.addons.package_models.upload(
                package_catalog,
                package_placements,
                own_package_body,
                renderer,
                frame.device,
                frame.queue,
                meshes,
                materials,
            )?;
        }
        if self
            .gpu
            .depth
            .as_ref()
            .is_none_or(|(_, _, size)| *size != frame.size)
        {
            let samples = renderer.samples();
            let color = (samples > 1).then(|| {
                frame.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("multisampled world color"),
                    size: wgpu::Extent3d {
                        width: frame.size.0,
                        height: frame.size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format: frame.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
            });
            self.gpu.depth = Some((
                create_depth_samples(frame.device, frame.size.0, frame.size.1, samples),
                color,
                frame.size,
            ));
        }
        // With shadows the first-person body is posed too: it casts a
        // shadow without being drawn.
        let casts = renderer.shadow_settings().is_some();
        self.world_items
            .upload(renderer, frame.device, frame.queue)?;
        crate::vehicles::ClientVehicles::upload(
            &mut self.vehicle_assets,
            renderer,
            frame.device,
            frame.queue,
        )?;
        self.fx
            .explosion_shapes
            .upload(renderer, frame.device, frame.queue)?;
        self.fx.beams.upload(renderer, frame.device, frame.queue)?;
        self.fx
            .weapon_effects
            .upload_contrails(renderer, frame.device, frame.queue)?;
        self.fx.decals.upload(renderer, frame.device, frame.queue)?;
        self.fx
            .tutorial_targets
            .upload(renderer, frame.device, frame.queue)?;
        self.fx
            .map_movers
            .upload(renderer, frame.device, frame.queue)?;
        let shells: Vec<_> = self
            .fx
            .weapon_shells
            .instances()
            .map(|i| bri_render::scene::SceneTransform {
                transform: i.transform,
                tint: i.tint,
            })
            .collect();
        if !shells.is_empty() || self.gpu.shell_gpu.is_some() {
            if self.gpu.shell_gpu.is_none() {
                let scene = renderer.upload(
                    frame.device,
                    frame.queue,
                    &self.fx.weapon_shells.assets().shell_scene,
                )?;
                self.gpu.shell_gpu = Some((
                    scene,
                    bri_render::scene::GpuInstances::new(frame.device, 64)?,
                ));
            }
            let (_, instances) = self.gpu.shell_gpu.as_mut().unwrap();
            if instances.capacity() < shells.len() {
                *instances = bri_render::scene::GpuInstances::new(
                    frame.device,
                    shells.len().next_power_of_two(),
                )?;
            }
            instances.update(frame.queue, &shells)?;
        }
        let first_person_eye = self
            .mounts
            .posed_eye
            .or(self.motion.local_eye())
            .unwrap_or_else(|| view.archetypes.eye(local));
        let (eye, yaw, pitch, roll) = Self::view_camera(
            controls,
            self.motion.presented(),
            self.build
                .building
                .as_ref()
                .context("Camera collision mirror missing")?,
            self.motion.collision(),
            &self.vehicle_assets,
            &self.vehicles,
            view,
            local,
            first_person_eye,
            &self.motion.passages(),
            orbit_drawn_offset(controls, &self.avatar.avatars),
        )?;
        // Resolve environment once for both presentation phases. Camera reads a
        // coherent current stock world; ordinary frame reads the resolved world.
        let live = (!view.environment.is_empty()).then(|| {
            bri_content::atmosphere::resolve(
                &authored_environment(scene),
                &view.environment,
                self.motion.server_tick().unwrap_or(view.tick as f64),
            )
        });
        let apply_environment = |camera: &mut Camera| {
            camera.apply_environment(scene);
            if let Some(live) = &live {
                camera.apply_atmosphere(live);
            }
        };
        let player_view = bri_client_sandbox::View {
            fov: controls.fov(),
            normal_fov: controls.normal_fov(),
            size: [frame.size.0, frame.size.1],
            first_person: !stock_third_person,
            aiming: controls.aiming(),
            alive: view.vitals.get(&view.owner).is_none_or(|v| v.alive),
            view_model: if stock_third_person {
                [0.0; 5]
            } else {
                controls.view_model_record()
            },
            hud_owned: 0,
        };
        let mut addon_world = if self.addons.client_code.is_started() {
            let (stock_forward, _, stock_up) = rolled_view_basis(yaw, pitch, roll);
            let aspect = frame.size.0 as f32 / frame.size.1 as f32;
            let mut stock_camera = Camera::oriented(
                eye.to_array(),
                stock_forward.to_array(),
                stock_up.to_array(),
                aspect,
                vertical_fov(controls.fov().to_radians(), aspect),
                0.05,
                FAR_PLANE,
            );
            apply_environment(&mut stock_camera);
            let world = if self.addons.client_code.reads_world() {
                let image_meshes = self.world_items.held_image_meshes();
                // A corpse past its timeout has no body left to pose: a
                // ragdoll goes with it instead of lingering unseen.
                let gone = self.combat.hidden_bodies(&view.vitals);
                let skeletons = if self.addons.client_code.poses_bodies() {
                    self.avatar
                        .avatars
                        .iter_mut()
                        .filter(|(owner, _)| !gone.contains(owner))
                        .map(|(owner, avatar)| {
                            (*owner, avatar.skeleton(&self.avatar.avatar_assets))
                        })
                        .collect()
                } else {
                    Default::default()
                };
                // Death and respawn as drawn, not the newest vitals.
                let lives = self
                    .avatar
                    .avatars
                    .iter()
                    .filter_map(|(owner, avatar)| Some((*owner, avatar.life()?)))
                    .collect();
                std::sync::Arc::new(crate::client_code::world_view(
                    view,
                    self.ghosts.entities_at(view.tick, &view.entities),
                    self.motion.presented(),
                    &self.vehicles,
                    &self.vehicle_assets,
                    &stock_camera,
                    &self.world_items,
                    image_meshes,
                    crate::client_code::DrawnBodies { skeletons, lives },
                    std::sync::Arc::new(self.motion.passages()),
                ))
            } else {
                Default::default()
            };
            self.addons.client_code.begin_camera_frame(
                self.avatar.animation_time,
                world.clone(),
                player_view,
            );
            world
        } else {
            Default::default()
        };
        // An Add-On may place the camera (`camera.control`): what the
        // player sees, never where they aim or move.
        let shot = if self.addons.client_code.is_started() {
            let look = Vec3::new(
                yaw.sin() * pitch.cos(),
                pitch.sin(),
                -yaw.cos() * pitch.cos(),
            );
            self.addons.client_code.camera_shot(eye, look)
        } else {
            None
        };
        self.view.addon_camera = shot.is_some();
        let (eye, yaw, pitch, roll) = match shot {
            Some(shot) => {
                let (yaw, pitch) = crate::controls::angles(Vec3::from(shot.forward), Vec3::Y);
                (Vec3::from(shot.eye), yaw, pitch, shot.roll)
            }
            None => (eye, yaw, pitch, roll),
        };
        let shown_fov = shot.and_then(|s| s.fov).unwrap_or_else(|| controls.fov());
        self.view.rendered_camera = Some((eye, yaw, pitch));
        self.view.rendered_roll = roll;
        // Explosion `CameraShake`: 10 degrees of view rotation per unit of offset.
        let shake = self.fx.actor_effects.camera_shake(eye) * 10f32.to_radians();
        let (forward, right, up) = rolled_view_basis(
            yaw + shake.z.clamped(-0.3, 0.3),
            pitch + shake.x.clamped(-0.3, 0.3),
            roll,
        );
        let aspect = frame.size.0 as f32 / frame.size.1 as f32;
        let fov = vertical_fov(shown_fov.to_radians(), aspect);
        self.view.pixels_per_unit = Some(frame.size.1 as f32 / (2.0 * (fov * 0.5).tan()));
        let mut camera = Camera::oriented(
            eye.to_array(),
            forward.to_array(),
            up.to_array(),
            aspect,
            fov,
            0.05,
            FAR_PLANE,
        );
        apply_environment(&mut camera);
        // The region the eye is in sets the fog (linear, the colour and distances the region names).
        let mut region_clear = None;
        if let Some(regions) = &self.scene.regions {
            let now = self.avatar.animation_time;
            let dt = (now - self.scene.fog_time).clamped(0.0, 1.0) as f32;
            self.scene.fog_time = now;
            let zone = regions.locate(eye.to_array());
            // The zone's ambient is what lit models take; lightmaps carry it already.
            if let Some(ambient) = regions.ambient(zone) {
                camera.ambient[..3].copy_from_slice(&ambient);
            }
            let fog = self.scene.fog_blend.step(regions, zone, dt);
            if fog.enabled {
                let [r, g, b] = fog.color.map(|c| f32::from(c) / 255.0);
                camera.fog_color = [r, g, b, 0.0];
                camera.atmosphere[0] = fog.start;
                camera.atmosphere[1] = fog.end;
                camera.atmosphere[3] = 1.0;
                camera.haze = [bri_content::environment::FOG_HEIGHT, 1.0, 0.0, 1.0];
            } else {
                camera.atmosphere[3] = 0.0;
            }
            region_clear = fog.clear;
        }
        if let Some(vignette) = &mut self.gpu.vignette {
            vignette.update(
                frame.queue,
                live.and_then(|l| l.vignette).map(|v| (v.color, v.multiply)),
                aspect,
            );
        }
        if let (Some(flare), Some(scene)) = (&mut self.gpu.lens_flare, &self.scene.cpu_scene) {
            // The sun's lens flare, where the map has one: shown while the
            // sun is in view and nothing drawn stands in front of it.
            flare.set_scene(frame.device, frame.queue, scene.lens_flare.as_ref());
            let toward = -Vec3::from_slice(&camera.sun_direction[..3]).normalize_or_zero();
            if toward != Vec3::ZERO {
                let bricks = self.build.building.as_ref().is_some_and(|b| {
                    b.solid_segment(eye, eye + toward * 1500.0)
                        .is_ok_and(|h| h.is_some())
                });
                let (at, strength) = self.gpu.sun_flare.update(
                    scene,
                    eye,
                    forward,
                    toward,
                    &glam::Mat4::from_cols_array(&camera.view_projection),
                    bricks,
                );
                flare.update(frame.queue, at, strength, aspect);
            } else {
                flare.update(frame.queue, glam::Vec2::ZERO, 0.0, aspect);
            }
        }
        camera.ambient[3] = f32::from(self.lighting.light_volume.mode(self.graphics.lighting));
        // Classic stays exactly v20's flat shade.
        let modern = camera.ambient[3] >= 0.5;
        camera.set_sky_ambient(self.graphics.soft_shading && modern);
        let occlusion_on = self.graphics.ambient_occlusion && modern;
        camera.atmosphere[2] = (self.avatar.animation_time % 86400.0) as f32;
        // `$pref::visibleDistanceMax` caps the map's visible distance; the
        // fog start scales with it so the fade keeps its shape.
        let cap = bri_ui::screens::options::visible_distance_max(&self.ui.core.prefs);
        if camera.atmosphere[3] > 0. && camera.atmosphere[1] > cap {
            let scale = cap / camera.atmosphere[1];
            camera.atmosphere[0] *= scale;
            camera.atmosphere[1] = cap;
        }
        renderer.update_camera(frame.queue, &camera);
        // Mirrors an Add-On's bricks carry: the planes that reflect live
        // this frame, each with its own view of the world.
        if self
            .lighting
            .reflections
            .as_ref()
            .is_none_or(|r| !r.matches(frame.format, renderer.samples()))
        {
            self.lighting.reflections = Some(bri_render::reflection::Reflections::new(
                frame.device,
                frame.format,
                renderer.samples(),
                self.graphics.reflections,
            ));
        }
        let reflections = self.lighting.reflections.as_mut().unwrap();
        reflections.set_settings(self.graphics.reflections);
        // A knocked-out mirror brick's mirrors leave its place and ride
        // its debris instead.
        let debris = &self.fx.brick_debris;
        let eye = glam::Vec4::from(camera.eye).truncate();
        let mut mirrors = self
            .scene
            .mirror_index
            .mirrors(|id| debris.is_dead(id), eye);
        crate::mirrors::debris(debris, &self.scene.mirror_shapes, eye, &mut mirrors);
        reflections.prepare(
            frame.device,
            frame.queue,
            renderer,
            &camera,
            frame.size,
            &mirrors,
        )?;
        let reflecting = reflections.live() > 0;
        // Metal reflects the world around the nearest metal surface within
        // mirror distance, with mirrors on; otherwise only the sky.
        if self
            .lighting
            .environment_probe
            .as_ref()
            .is_none_or(|p| !p.matches(renderer, frame.format))
        {
            self.lighting.environment_probe =
                Some(bri_render::environment_probe::EnvironmentProbe::new(
                    frame.device,
                    renderer,
                    frame.format,
                    renderer.samples(),
                ));
        }
        let settings = self.graphics.reflections;
        let metal = (settings.planes > 0)
            .then(|| {
                self.vehicle_assets
                    .metal_centres()
                    .into_iter()
                    .filter(|c| c.distance(eye) <= settings.distance)
                    .min_by(|a, b| a.distance(eye).total_cmp(&b.distance(eye)))
            })
            .flatten();
        self.lighting.environment_probe.as_mut().unwrap().prepare(
            frame.device,
            frame.queue,
            renderer,
            &camera,
            metal,
            settings.distance,
        );
        // Bodies build their mesh here, once the view is known, and only
        // where they can show: in the player's view, another view (a
        // mirror plane, a probe face) or a shadow one of them reads. A
        // moving body is skinned and uploaded every frame it is built, so
        // bodies far behind the camera cost nothing. A mirror shows the
        // player's own body in first person too.
        let span = crate::frame_trace::span("avatar meshes");
        let in_view =
            crate::culling::Frustum::new(glam::Mat4::from_cols_array(&camera.view_projection));
        let reflecting_planes: Vec<glam::Mat4> = self
            .lighting
            .reflections
            .as_ref()
            .filter(|_| reflecting)
            .map(|r| r.plan().planes.iter().map(|p| p.view_projection).collect())
            .unwrap_or_default();
        let probe_faces = self
            .lighting
            .environment_probe
            .as_ref()
            .map(|p| p.face_views())
            .unwrap_or_default();
        let elsewhere = crate::culling::Elsewhere::new(
            reflecting_planes
                .into_iter()
                .chain(probe_faces.iter().map(|f| f.view_projection)),
            renderer.shadow_reach(),
        );
        let others_see = casts || reflecting || !probe_faces.is_empty();
        let mut bodies_drawn = BTreeSet::new();
        let passages = self.motion.passages();
        // A new body or outfit uploads whole: materials, and textures the
        // shared cache lacks. A round that respawns and dresses every
        // player at once would do them all in one frame, so whole uploads
        // stop once they have taken BODY_UPLOAD_BUDGET; the rest draw from
        // the next frames. The player's own body goes first.
        let mut whole = WholeUploads::default();
        let mut own_first: Vec<_> = self.avatar.avatars.keys().copied().collect();
        own_first.sort_by_key(|owner| *owner != view.owner);
        for owner in &own_first {
            let avatar = self.avatar.avatars.get_mut(owner).expect("listed above");
            if hidden.contains(owner) {
                continue;
            }
            let own_hidden = *owner == view.owner && !third_person;
            if own_hidden && !others_see {
                continue;
            }
            let (center, radius) = avatar.bounding_sphere();
            // A body part way through an opening draws on both sides.
            avatar.straddle = body_straddle(&self.vehicles, view, &passages, *owner, avatar);
            let through = avatar.straddle.map(|s| s.carry.transform_point3(center));
            let seen = |c: Vec3| {
                (!own_hidden && in_view.sees_sphere(c, radius))
                    || (others_see && elsewhere.sees_sphere(c, radius))
            };
            if !seen(center) && through.is_none_or(|c| !seen(c)) {
                continue;
            }
            let full = avatar.needs_full_upload();
            if full && !whole.may_start() {
                continue;
            }
            let started = full.then(std::time::Instant::now);
            avatar.build_pending(&self.avatar.avatar_assets)?;
            let textures = self
                .avatar
                .body_textures
                .entry(avatar.model.clone())
                .or_default();
            avatar.upload_sharing(renderer, frame.device, frame.queue, Some(textures))?;
            if let Some(started) = started {
                whole.done(started.elapsed());
            }
            bodies_drawn.insert(*owner);
        }
        whole.note();
        self.fx.body_flashes.upload(
            renderer,
            frame.device,
            frame.queue,
            &self.avatar.avatars,
            |owner| bodies_drawn.contains(&owner),
        )?;
        self.fx.body_shells.upload(
            renderer,
            frame.device,
            frame.queue,
            &self.avatar.avatars,
            |owner| bodies_drawn.contains(&owner),
        )?;
        // Horses likewise: posed every frame (riders sit on their nodes),
        // their mesh built and drawn only where it can show.
        let mut mounts_drawn = BTreeSet::new();
        for (id, mesh) in &mut self.avatar.mount_meshes {
            let (center, radius) = mesh.bounding_sphere();
            let through = mesh.straddle.map(|s| s.carry.transform_point3(center));
            let seen = |c: Vec3| {
                in_view.sees_sphere(c, radius) || (others_see && elsewhere.sees_sphere(c, radius))
            };
            if !seen(center) && through.is_none_or(|c| !seen(c)) {
                continue;
            }
            mesh.build_pending(&self.avatar.avatar_assets)?;
            mesh.upload(renderer, frame.device, frame.queue)?;
            mounts_drawn.insert(*id);
        }
        self.avatar.meshes_built = (bodies_drawn.len(), mounts_drawn.len());
        drop(span);
        let effects_camera = bri_fx_runtime::Camera {
            view_projection: glam::Mat4::from_cols_array(&camera.view_projection),
            position: eye,
            right,
            up,
        };
        if self.addons.client_code.is_started() {
            if self.addons.client_code.reads_world() {
                let rgb = |v: [f32; 4]| [v[0], v[1], v[2]];
                std::sync::Arc::make_mut(&mut addon_world).environment =
                    bri_client_sandbox::world::Environment {
                        sun_direction: rgb(camera.sun_direction),
                        sun_color: rgb(camera.sun_color),
                        ambient: rgb(camera.ambient),
                        sky: rgb(camera.fog_color),
                    };
            }
            let span = crate::frame_trace::span("Add-On code");
            self.addons.client_code.run_frame(
                self.avatar.animation_time,
                eye,
                forward,
                addon_world,
                player_view,
            );
            drop(span);
            for (asset, at, volume) in self.addons.client_code.take_sounds() {
                let placement = match at {
                    Some(at) => bri_audio::Placement::World(bri_audio::Vec3::from(at)),
                    None => bri_audio::Placement::Listener,
                };
                self.audio.play_asset(asset, placement, volume);
            }
            let _span = crate::frame_trace::span("Add-On draw setup");
            self.addons.client_code.prepare(
                frame.device,
                frame.queue,
                frame.format,
                bri_render::scene::DEPTH_FORMAT,
                renderer.samples(),
                effects_camera.view_projection,
                eye,
                [frame.size.0, frame.size.1],
            );
        }
        // Every other view this frame (mirror and portal planes, the
        // environment probe's faces) and the eyes they all see from: the
        // terrain tiles and effect lights every view shares cover them all.
        let planes = self
            .lighting
            .reflections
            .as_ref()
            .map(|r| r.plan().planes.clone())
            .unwrap_or_default();
        let probe_views = self
            .lighting
            .environment_probe
            .as_ref()
            .map(|p| p.face_views())
            .unwrap_or_default();
        let weather_camera = self.weather.world.camera();
        let other_views = crate::views::other_views(
            &planes,
            &probe_views,
            &crate::views::PlayerCamera {
                forward: weather_camera.forward,
                right,
                up,
                velocity: weather_camera.velocity,
            },
        );
        let eyes = crate::views::eyes(eye, &other_views);
        // Slots are disjoint from every mirror and the six probe faces.
        let local_bindings = if !stock_third_person && shot.is_none() {
            self.world_items.local_views(view.owner)
        } else {
            Vec::new()
        };
        let mut local_views = Vec::with_capacity(local_bindings.len());
        for (affinity, policy) in local_bindings {
            let vertical = policy.vertical_fov(aspect);
            let mut local_camera = camera;
            local_camera.view_projection = Camera::oriented(
                eye.to_array(),
                forward.to_array(),
                up.to_array(),
                aspect,
                vertical,
                policy.near,
                policy.far,
            )
            .view_projection;
            let effects_camera = bri_fx_runtime::Camera {
                view_projection: glam::Mat4::from_cols_array(&local_camera.view_projection),
                position: eye,
                right,
                up,
            };
            ensure!(
                effects_camera.view_projection.is_finite(),
                "Invalid local image projection"
            );
            let effects = self
                .fx
                .weapon_effects
                .world()
                .snapshot_in_local_view(&effects_camera, &affinity);
            let prebudget_particles = effects.particles.len();
            local_views.push(LocalImagePass {
                affinity,
                policy,
                camera: local_camera,
                effects_camera,
                effects,
                prebudget_particles,
                cut_sprites: 0,
            });
        }
        self.fx.weapon_effects.check_local_views(
            &local_views
                .iter()
                .map(|v| v.affinity.clone())
                .collect::<Vec<_>>(),
        );
        if !local_views.is_empty() {
            renderer.set_view_count(frame.device, LOCAL_IMAGE_FIRST_VIEW + local_views.len());
            for (index, local) in local_views.iter().enumerate() {
                renderer.update_view(frame.queue, LOCAL_IMAGE_FIRST_VIEW + index, &local.camera);
            }
        }

        // The map's backdrop looks from its own eye with the player's rotation and projection.
        let backdrop_view = LOCAL_IMAGE_FIRST_VIEW + local_views.len();
        if let (Some(backdrop), Some(_)) = (&self.scene.backdrop, &self.gpu.gpu_backdrop) {
            renderer.set_view_count(frame.device, backdrop_view + 1);
            renderer.update_view(
                frame.queue,
                backdrop_view,
                &backdrop_camera(&camera, backdrop.eye),
            );
        }

        let rgb = |v: [f32; 4]| [v[0], v[1], v[2]];
        self.addons.item_skins.prepare(
            frame.device,
            frame.queue,
            frame.format,
            bri_render::scene::DEPTH_FORMAT,
            renderer.samples(),
            &mut self.world_items,
            self.addons.client_code.trusts_server(),
            crate::item_skins::Light {
                sun_direction: rgb(camera.sun_direction),
                sun_color: rgb(camera.sun_color),
                ambient: rgb(camera.ambient),
            },
            effects_camera.view_projection,
            eye,
            [frame.size.0, frame.size.1],
            self.avatar.animation_time as f32,
        );
        // Brick emitters no view this frame can see sleep; ones a view can
        // see again wake with their cloud refilled before any snapshot.
        let culled_views: Vec<_> = std::iter::once(effects_camera.view_projection)
            .chain(other_views.iter().map(|v| v.view_projection))
            .collect();
        let cull_span = crate::frame_trace::span("effects cull");
        self.fx.effects.world.cull_sources(&culled_views);
        drop(cull_span);
        let world_frame = self.fx.effects.world.snapshot_in_view(&effects_camera);
        let weapon_frame = self
            .fx
            .weapon_effects
            .world()
            .snapshot_in_view(&effects_camera);
        // Probe the already computed inputs only when explicitly requested.
        // Keep local until the entire render attempt succeeds, so failures do
        // not expose a partial preparation or a previous frame.
        let mut weapon_observation =
            self.fx
                .weapon_frame_probe
                .then(|| crate::weapon_effects::WeaponFrameObservation {
                    session: self.fx.weapon_effect_session,
                    replica_tick: view.tick,
                    owner: view.owner,
                    cue_cursor: self.fx.weapon_effects.cue_cursor(),
                    camera: effects_camera,
                    attachments: self.fx.weapon_effects.timed_attachments().collect(),
                    mounted_images: self.world_items.mounted_image_observations().collect(),
                    local_views: Vec::new(),
                    weapon_frame: weapon_frame.clone(),
                    combined_particles: 0,
                    combined_lights: 0,
                    cut_sprites: 0,
                    cut_lights: 0,
                });
        let actor_frame = self
            .fx
            .actor_effects
            .world()
            .snapshot_in_view(&effects_camera);
        let sprite_budget = self
            .gpu
            .effects_renderer
            .as_ref()
            .context("Effects GPU not initialized")?
            .max_instances();
        let (mut effects_frame, mut cuts) = combine_effect_frames(
            world_frame,
            [weapon_frame, actor_frame],
            &eyes,
            if local_views.is_empty() {
                sprite_budget
            } else {
                usize::MAX
            },
        );
        if !local_views.is_empty() {
            let mut partitions: Vec<_> = std::iter::once(&mut effects_frame)
                .chain(local_views.iter_mut().map(|v| &mut v.effects))
                .collect();
            let removed = trim_effect_partitions(&mut partitions, eye, sprite_budget);
            cuts.sprites += removed.iter().sum::<usize>();
            for (local, cut) in local_views.iter_mut().zip(removed.into_iter().skip(1)) {
                local.cut_sprites = cut;
            }
        }
        let (fog_start, fog_end) = if camera.atmosphere[3] > 0. {
            (camera.atmosphere[0], camera.atmosphere[1])
        } else {
            (cap, cap + 1.)
        };
        for terrain in &mut self.gpu.gpu_terrain {
            terrain.update(frame.device, frame.queue, &eyes, fog_end.max(1.))?;
        }
        let water_eyes = if self.scene.scenery.native_water_visible() {
            &eyes[..]
        } else {
            &[]
        };
        bri_render::water_scene::GpuWater::update_all(
            &mut self.gpu.gpu_water,
            renderer,
            frame.device,
            frame.queue,
            water_eyes,
            fog_end.clamped(1., FAR_PLANE),
        )?;
        self.ui.core.name_tags = name_tags(
            view,
            self.motion.presented(),
            &self.vehicle_assets,
            &self.content.weapons.pack,
            self.build.building.as_ref(),
            glam::Mat4::from_cols_array(&camera.view_projection),
            eye,
            (fog_start.max(0.), fog_end.max(1.)),
            // The interface's window, not the world's smaller Render Scale.
            (
                self.gpu.display_size.0 as f32,
                self.gpu.display_size.1 as f32,
            ),
            self.ui.scale(),
            controls.observer().is_none(),
            &passages,
            |drop| self.world_items.drop_center(drop),
            self.ui
                .core
                .hud_regions
                .iter()
                .find(|r| r.region == bri_content::hud::Region::NameTags),
        );
        let slowdown = assist_slowdown(
            view,
            self.motion.presented(),
            &self.content.weapons.pack,
            glam::Mat4::from_cols_array(&camera.view_projection),
            eye,
        );
        self.controls.set_assist_slowdown(slowdown);
        // Plants keep their authored tint, scaled by the live outdoor light
        // relative to the mission's own daylight. All other views share it.
        let live_up = (-glam::Vec3::from_slice(&camera.sun_direction)
            .normalize_or_zero()
            .y)
            .max(0.0);
        let baked_up = (-glam::Vec3::from_array(scene.sun_direction)
            .normalize_or_zero()
            .y)
            .max(0.0);
        let plant_light = std::array::from_fn(|i| {
            ((camera.ambient[i] + camera.sun_color[i] * live_up)
                / (scene.ambient[i] + scene.sun_color[i] * baked_up).max(0.001))
            .clamped(0.0, 4.0)
        });
        self.foliage.set_illumination(plant_light)?;
        self.foliage.prepare(
            frame,
            &bri_foliage::Camera {
                position: eye,
                right,
                view_projection: effects_camera.view_projection,
                visible_distance: fog_end.max(1.),
            },
            fog_start,
            fog_end.max(fog_start + 0.001),
        )?;
        self.fx.weapon_light_deferred = cuts.lights;
        self.fx.effect_sprites_cut = cuts.sprites;
        // Player lights are effect lights too; the nearest to the camera win.
        let lights: Vec<_> = effects_frame
            .lights
            .iter()
            .map(|light| bri_render::scene::PointLight {
                position_radius: light.position.extend(light.radius).to_array(),
                color: light.color.extend(0.).to_array(),
                ..Default::default()
            })
            .collect();
        // Vehicle lamps their drivers turned on light the world too.
        let mut lights = lights;
        if let Some(view) = self.net.attempt.as_ref().and_then(|a| a.view.as_ref()) {
            let vehicles = &self.vehicles;
            let assets = &self.vehicle_assets;
            let infos: Vec<_> = view
                .vehicles
                .values()
                .filter_map(|info| {
                    let f = vehicles.frame(info.id)?;
                    let body = glam::Mat4::from_scale_rotation_translation(
                        glam::Vec3::splat(info.scale),
                        f.rotation,
                        f.position,
                    );
                    Some((
                        info.id,
                        body,
                        info.lights,
                        assets.driver_present(info),
                        f.driver_jump_held,
                        info.definition.clone(),
                    ))
                })
                .collect();
            lights.extend(self.vehicle_assets.lamp_lights(infos));
        }
        lights.truncate(bri_render::scene::MAX_POINT_LIGHTS);
        // Map lights past the shaded budget fill what the effects leave.
        let overflow = super::lighting::overflow_point_lights(
            &self.lighting.light_volume.overflow,
            eye,
            lights.len(),
        );
        lights.extend(overflow);
        renderer.update_lights(frame.queue, &lights)?;
        let effects_renderer = self
            .gpu
            .effects_renderer
            .as_mut()
            .context("Effects GPU not initialized")?;
        effects_renderer.set_fog(camera.atmosphere, camera.fog_color, camera.haze);
        effects_renderer.set_lighting(
            glam::Vec3::from_slice(&camera.ambient),
            glam::Vec3::from_slice(&camera.sun_direction),
            glam::Vec3::from_slice(&camera.sun_color),
            &lights
                .iter()
                .map(|l| bri_fx_runtime::GpuLight {
                    position_radius: l.position_radius,
                    color: l.color,
                })
                .collect::<Vec<_>>(),
        )?;
        effects_renderer.prepare(frame.queue, &effects_camera, &effects_frame)?;
        for (index, local) in local_views.iter().enumerate() {
            effects_renderer.prepare_view(
                frame.device,
                frame.queue,
                LOCAL_IMAGE_FIRST_VIEW + index,
                &local.effects_camera,
                &local.effects,
            )?;
            self.addons.item_skins.prepare_local_view(
                frame.device,
                frame.queue,
                frame.format,
                bri_render::scene::DEPTH_FORMAT,
                renderer.samples(),
                &mut self.world_items,
                self.addons.client_code.trusts_server(),
                crate::item_skins::Light {
                    sun_direction: rgb(camera.sun_direction),
                    sun_color: rgb(camera.sun_color),
                    ambient: rgb(camera.ambient),
                },
                index,
                &local.affinity,
                local.effects_camera.view_projection,
                eye,
                [frame.size.0, frame.size.1],
                self.avatar.animation_time as f32,
                local.policy.horizontal_fov,
            );
        }
        if let Some(o) = &mut weapon_observation {
            o.combined_particles = effects_frame.particles.len()
                + local_views
                    .iter()
                    .map(|v| v.effects.particles.len())
                    .sum::<usize>();
            o.local_views = local_views
                .iter()
                .map(|v| crate::weapon_effects::LocalViewObservation {
                    affinity: v.affinity.clone(),
                    policy: v.policy,
                    viewport: [frame.size.0, frame.size.1],
                    world_horizontal_fov: shown_fov,
                    camera: v.effects_camera,
                    weapon_frame: v.effects.clone(),
                    prebudget_particles: v.prebudget_particles,
                    cut_sprites: v.cut_sprites,
                })
                .collect();
            o.combined_lights = effects_frame.lights.len();
            o.cut_sprites = cuts.sprites;
            o.cut_lights = cuts.lights;
        }
        // A sky's rain or snow changed the weather pack: its pictures go up.
        if self.gpu.weather_revision != self.weather.revision {
            let limits = bri_weather::WeatherLimits::default();
            self.gpu.weather_renderer = Some(bri_weather::gpu::WeatherRenderer::new(
                frame.device,
                frame.queue,
                self.weather.world.pack(),
                frame.format,
                bri_render::scene::DEPTH_FORMAT,
                self.graphics.samples,
                limits.drops + limits.splashes,
            )?);
            self.gpu.weather_revision = self.weather.revision;
        }
        let weather_renderer = self
            .gpu
            .weather_renderer
            .as_mut()
            .context("Weather GPU not initialized")?;
        if let Some(lines) = &self.gpu.hidden_lines {
            lines.prepare(frame.queue, effects_camera.view_projection);
        }
        if let Some(lines) = &self.gpu.region_lines {
            lines.prepare(frame.queue, effects_camera.view_projection);
        }
        if let Some(lines) = &self.gpu.selection_lines {
            lines.prepare(frame.queue, effects_camera.view_projection);
        }
        if let Some(shapes) = &self.world_shapes {
            shapes.prepare(
                frame.queue,
                effects_camera.view_projection,
                effects_camera.position,
            );
        }
        weather_renderer.prepare(
            frame.queue,
            effects_camera.view_projection,
            &self.weather.world.snapshot(),
        )?;
        // Each other view sees the sprites, plants, weather and Add-On
        // layers from its own eye: its own culling and far-to-near order,
        // and billboards turned to face it. The probe's faces also see the
        // mirrors in them, so metal reflects the world the player sees.
        let mut layers = crate::views::Layers {
            effects: [
                &self.fx.effects.world,
                self.fx.weapon_effects.world(),
                self.fx.actor_effects.world(),
            ],
            sprites: &mut *effects_renderer,
            foliage: &mut self.foliage,
            weather: &self.weather.world,
            drops: &mut *weather_renderer,
            client_code: &mut self.addons.client_code,
            item_skins: &mut self.addons.item_skins,
            fog: (fog_start, fog_end),
        };
        for v in &other_views {
            layers.prepare(frame, v)?;
        }
        if let Some(reflections) = &mut self.lighting.reflections {
            let size = bri_render::environment_probe::PROBE_SIZE;
            for face in &probe_views {
                reflections.prepare_view(
                    frame.device,
                    frame.queue,
                    face.view,
                    face.view_projection,
                    face.eye,
                    (size, size),
                );
            }
        }
        let (depth, multisampled, _) = self.gpu.depth.as_ref().unwrap();
        let depth = depth.create_view(&Default::default());
        let multisampled = multisampled
            .as_ref()
            .map(|color| color.create_view(&Default::default()));
        let world_target = multisampled.as_ref().unwrap_or(frame.target);
        // A changed fog colour clears the frame with it too.
        let clear_color = match &live {
            Some(l) if l.fog_color != scene.fog.color => {
                [l.fog_color[0], l.fog_color[1], l.fog_color[2], 1.0]
            }
            _ => scene.clear_color,
        };
        let clear_color = match region_clear {
            Some(c) => [
                f32::from(c[0]) / 255.0,
                f32::from(c[1]) / 255.0,
                f32::from(c[2]) / 255.0,
                1.0,
            ],
            None => clear_color,
        };
        let [r, g, b, a] = clear_color.map(f64::from);
        if let (Some(gpu), Some(view)) = (
            self.gpu.gpu_scene.as_mut(),
            self.net.attempt.as_ref().and_then(|a| a.view.as_ref()),
        ) && self.gpu.gpu_broken != view.broken_shapes
        {
            // Smashed shapes stop drawing (`renderWhenDestroyed = 0`); only a
            // new mission restores them, with a fresh upload.
            let ranges: Vec<_> = view
                .broken_shapes
                .difference(&self.gpu.gpu_broken)
                .filter_map(|node| self.scene.shape_indices.get(node).cloned())
                .collect();
            gpu.hide_indices(&ranges);
            self.gpu
                .gpu_broken
                .extend(view.broken_shapes.iter().copied());
        }
        let mut scenes = vec![self.gpu.gpu_scene.as_ref().unwrap()];
        scenes.extend(self.gpu.gpu_chunks.values());
        scenes.extend(self.gpu.gpu_water.iter().flat_map(|water| water.draws()));

        scenes.extend(
            self.build
                .remote_ghosts
                .values()
                .filter_map(|(_, gpu)| gpu.as_ref()),
        );
        // Bodies draw through their one-instance body transform.
        let avatar_draws: Vec<_> = self
            .avatar
            .avatars
            .iter()
            .filter(|(owner, _)| bodies_drawn.contains(*owner))
            .filter_map(|(owner, avatar)| {
                Some((*owner, (avatar.gpu.as_ref()?, avatar.instance.as_ref()?)))
            })
            .collect();
        let mount_draws: Vec<_> = self
            .avatar
            .mount_meshes
            .iter()
            .filter(|(id, _)| mounts_drawn.contains(*id))
            .filter_map(|(_, mesh)| Some((mesh.gpu.as_ref()?, mesh.instance.as_ref()?)))
            .collect();
        scenes.extend(self.fx.fade_models.scenes());
        scenes.extend(self.fx.weapon_effects.contrail_scenes());
        self.fx
            .map_scatter
            .upload(renderer, frame.device, frame.queue, eye)?;
        // Models every view draws; the player's own body and held items
        // differ between the player's view and a mirror's.
        let mut shared_draws = Vec::new();
        if let Some((ghost, placed)) = &self.gpu.ghost_gpu {
            shared_draws.push((ghost, placed));
        }
        shared_draws.extend(crate::vehicles::ClientVehicles::draws(&self.vehicle_assets));
        shared_draws.extend(mount_draws.iter().copied());
        shared_draws.extend(self.gpu.gpu_terrain.iter().flat_map(|t| t.draws()));
        shared_draws.extend(self.fx.explosion_shapes.draws());
        shared_draws.extend(self.fx.beams.draws());
        shared_draws.extend(self.fx.decals.draws());
        shared_draws.extend(self.fx.tutorial_targets.draws());
        shared_draws.extend(self.fx.map_movers.draws());
        shared_draws.extend(self.fx.map_scatter.draws());
        if let Some((scene, instances)) = &self.gpu.shell_gpu
            && self.fx.weapon_shells.active_count() > 0
        {
            shared_draws.push((scene, instances));
        }
        shared_draws.extend(self.fx.debris_models.draws());
        shared_draws.extend(self.addons.package_models.draws());
        let mut item_draws = self.world_items.draws();
        item_draws.extend(
            avatar_draws
                .iter()
                .filter(|(owner, _)| *owner != view.owner || third_person)
                .map(|(_, draw)| *draw),
        );
        item_draws.extend(shared_draws.iter().copied());
        item_draws.extend(
            self.fx.body_flashes.draws(|owner| {
                bodies_drawn.contains(&owner) && (owner != view.owner || third_person)
            }),
        );
        item_draws.extend(
            self.fx.body_shells.draws(|owner| {
                bodies_drawn.contains(&owner) && (owner != view.owner || third_person)
            }),
        );
        use bri_render::scene::ShadowCasters;
        // Every view's shadow casters: mirror and window planes draw their
        // own sun shadows from them.
        let (bodies, cast_models, blockers, blocking, map, terrain_map) = {
            // Players, vehicles and items (dropped and held) cast, like v20's
            // projected shape shadows; bricks only with the BrickShadows pref,
            // and bricks that do not cast still stop shadows passing through
            // them. Dynamic includes current bricks and terrain independently
            // of compatibility preferences, even while a mode switch is pending.
            let chunks: Vec<&GpuScene> = self
                .gpu
                .gpu_chunks
                .values()
                .chain(self.fx.fade_models.scenes())
                .collect();
            let (bodies, blockers) = if self.graphics.brick_shadows || effective.lighting == 3 {
                (chunks, Vec::new())
            } else {
                (Vec::new(), chunks)
            };
            let mut blocking = Vec::new();

            // Rigged mounts cast through the same clipped body instances
            // as their main, portal, mirror and probe views.
            // The player's own items cast from their hands, as others see
            // them, not from the first-person copy at the eye.
            let mut models = self.world_items.reflection_draws();
            models.extend(avatar_draws.iter().map(|(_, draw)| *draw));
            models.extend(crate::vehicles::ClientVehicles::draws(&self.vehicle_assets));
            models.extend(mount_draws.iter().copied());
            if let Some((scene, instances)) = &self.gpu.shell_gpu
                && self.fx.weapon_shells.active_count() > 0
            {
                models.push((scene, instances));
            }
            // Debris is bricks, so it follows the same setting as the bricks
            // it broke from; Add-On models cast like items.
            if self.graphics.brick_shadows || effective.lighting == 3 {
                models.extend(self.fx.debris_models.draws());
            } else {
                blocking.extend(self.fx.debris_models.draws());
            }
            models.extend(self.addons.package_models.draws());
            models.extend(self.addons.package_models.own_draws());
            // In the Unified modes the map's own walls shade objects from
            // the sun too (the map layer), so they are sunlit exactly where
            // the walls beside them are.
            let map: Vec<&GpuScene> = if effective.lighting != 0 {
                self.gpu.gpu_scene.iter().collect()
            } else {
                Vec::new()
            };
            let terrain_map: Vec<_> = if effective.lighting == 3 {
                self.gpu
                    .gpu_terrain
                    .iter()
                    .flat_map(|t| t.draws())
                    .collect()
            } else {
                Vec::new()
            };
            (bodies, models, blockers, blocking, map, terrain_map)
        };
        let sky_exposure = |encoder: &mut wgpu::CommandEncoder, view: usize| {
            let sky_scenes: Vec<_> = self
                .gpu
                .gpu_scene
                .iter()
                .chain(self.gpu.gpu_chunks.values())
                .collect();
            let mut sky_moving = cast_models.clone();
            sky_moving.extend(blocking.iter().copied());
            let sky_fading: Vec<_> = self.fx.fade_models.scenes().collect();
            renderer.render_view_sky_exposure_with_moving(
                encoder,
                view,
                ShadowCasters {
                    scenes: &sky_scenes,
                    instances: &terrain_map,
                },
                ShadowCasters {
                    scenes: &sky_fading,
                    instances: &sky_moving,
                },
            );
        };
        let shadows = |encoder: &mut wgpu::CommandEncoder, view: usize| {
            sky_exposure(encoder, view);
            renderer.render_view_shadows(
                encoder,
                view,
                ShadowCasters {
                    scenes: &bodies,
                    instances: &cast_models,
                },
                ShadowCasters {
                    scenes: &blockers,
                    instances: &blocking,
                },
                ShadowCasters {
                    scenes: &map,
                    instances: &terrain_map,
                },
            );
        };
        let _recording = crate::frame_trace::span("record passes");
        // Prepare environmental occlusion from the world, never from only the
        // rifle supplied to the later model-only pass. Sun shadow inputs stay main.
        for index in 0..local_views.len() {
            sky_exposure(frame.encoder, LOCAL_IMAGE_FIRST_VIEW + index);
        }
        renderer.begin_timing(frame.encoder);
        // Live mirror and window planes record the player's shadows
        // themselves, after those with their own.
        if !reflecting {
            shadows(frame.encoder, 0);
        }
        let clear = wgpu::Color { r, g, b, a };
        let reflections = self.lighting.reflections.as_ref().unwrap();
        if reflecting {
            let mut mirrored = self.world_items.reflection_draws_without_self();
            mirrored.extend(
                avatar_draws
                    .iter()
                    .filter(|(owner, _)| *owner != view.owner || third_person)
                    .map(|(_, draw)| *draw),
            );
            mirrored.extend(shared_draws.iter().copied());
            let mut own_body: Vec<_> = avatar_draws
                .iter()
                .filter(|(owner, _)| *owner == view.owner && !third_person)
                .map(|(_, draw)| *draw)
                .collect();
            own_body.extend(self.addons.package_models.own_draws());
            own_body.extend(self.world_items.reflected_self_draws());
            let straddle = self.avatar.avatars.get(&view.owner).and_then(|avatar| {
                body_straddle(&self.vehicles, view, &passages, view.owner, avatar)
            });
            let body_eye = Self::first_person_eye_here(controls, local, first_person_eye);
            let own_visible: Vec<_> = reflections
                .plan()
                .planes
                .iter()
                .map(|plane| {
                    third_person
                        || crate::portal_view::first_person_body_visible(
                            body_eye, plane.eye, straddle,
                        )
                })
                .collect();
            let mut mirrored_with_body = mirrored.clone();
            mirrored_with_body.extend(own_body);
            let models = |view: usize| {
                if own_visible[view - 1] {
                    mirrored_with_body.as_slice()
                } else {
                    mirrored.as_slice()
                }
            };
            let (foliage, sprites, drops) = (&self.foliage, &*effects_renderer, &*weather_renderer);
            let (layers, skins) = (&self.addons.client_code, &self.addons.item_skins);
            // As the player's view draws them after the world.
            let late = |pass: &mut wgpu::RenderPass<'_>, view: usize| {
                foliage.render_view(pass, view);
                sprites.render_view(pass, view);
                drops.render_view(pass, view);
                skins.render_view(pass, view);
                layers.render_view(pass, view);
            };
            reflections.render_views(
                renderer,
                frame.encoder,
                &scenes,
                &models,
                clear,
                &late,
                &shadows,
            );
            renderer.mark(frame.encoder, "mirrors");
        }
        let probe = self.lighting.environment_probe.as_ref().unwrap();
        if !probe.faces().is_empty() {
            let mut around = self.world_items.reflection_draws();
            around.extend(avatar_draws.iter().map(|(_, draw)| *draw));
            around.extend(shared_draws.iter().copied());
            around.extend(self.addons.package_models.own_draws());
            let (foliage, sprites, drops) = (&self.foliage, &*effects_renderer, &*weather_renderer);
            let (layers, skins) = (&self.addons.client_code, &self.addons.item_skins);
            let late = |pass: &mut wgpu::RenderPass<'_>, view: usize| {
                foliage.render_view(pass, view);
                sprites.render_view(pass, view);
                drops.render_view(pass, view);
                skins.render_view(pass, view);
                layers.render_view(pass, view);
            };
            let surfaces = |pass: &mut wgpu::RenderPass<'_>, view: usize| {
                reflections.draw_surfaces(pass, view)
            };
            probe.render(
                renderer,
                frame.encoder,
                &scenes,
                &around,
                clear,
                &surfaces,
                &late,
            );
        }
        // The backdrop first, then depth starts over under the map; where the map draws nothing the backdrop shows.
        let backdrop_drawn = match &self.gpu.gpu_backdrop {
            Some(gpu) if self.scene.backdrop.is_some() => {
                renderer.render_world(
                    frame.encoder,
                    bri_render::scene::WorldPass {
                        view: backdrop_view,
                        color: world_target,
                        resolve: None,
                        depth: &depth,
                        viewport: None,
                        clear: Some(clear),
                        after_opaque: None,
                        after_all: None,
                    },
                    &[gpu],
                    &[],
                );
                drop(
                    frame
                        .encoder
                        .begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("backdrop fresh depth"),
                            color_attachments: &[],
                            depth_stencil_attachment: Some(
                                wgpu::RenderPassDepthStencilAttachment {
                                    view: &depth,
                                    depth_ops: Some(wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(bri_render::scene::DEPTH_CLEAR),
                                        store: wgpu::StoreOp::Store,
                                    }),
                                    stencil_ops: None,
                                },
                            ),
                            timestamp_writes: None,
                            occlusion_query_set: None,
                            multiview_mask: None,
                        }),
                );
                true
            }
            _ => false,
        };
        let surfaces = |pass: &mut wgpu::RenderPass<'_>| reflections.draw_surfaces(pass, 0);
        let world_pass = bri_render::scene::WorldPass {
            view: 0,
            color: world_target,
            resolve: None,
            depth: &depth,
            viewport: None,
            clear: (!backdrop_drawn).then_some(clear),
            after_opaque: (!mirrors.is_empty()).then_some(&surfaces as _),
            after_all: None,
        };
        match self.gpu.ambient_occlusion.as_ref().filter(|_| occlusion_on) {
            // Ambient occlusion darkens the opaque world only: water, glass,
            // see-through bricks and everything drawn later stay as they are.
            Some(occlusion) => {
                let (device, queue, size) = (frame.device, frame.queue, frame.size);
                let mut between = |encoder: &mut wgpu::CommandEncoder| {
                    renderer.mark(encoder, "world");
                    occlusion.render(
                        device,
                        queue,
                        encoder,
                        world_target,
                        &depth,
                        size,
                        camera.view_projection,
                        [camera.eye[0], camera.eye[1], camera.eye[2]],
                        (camera.atmosphere, camera.fog_color[3], camera.haze),
                        (renderer, &scenes, &item_draws),
                    );
                    renderer.mark(encoder, "occlusion");
                };
                renderer.render_world_split(
                    frame.encoder,
                    world_pass,
                    &scenes,
                    &item_draws,
                    &mut between,
                );
                renderer.mark(frame.encoder, "world blended");
            }
            None => {
                renderer.render_world(frame.encoder, world_pass, &scenes, &item_draws);
                renderer.mark(frame.encoder, "world");
            }
        }
        if let Some(layer) = &mut self.gpu.decal_layer {
            layer.prepare(
                frame.device,
                frame.queue,
                glam::Mat4::from_cols_array(&camera.view_projection),
                glam::Vec3::new(camera.eye[0], camera.eye[1], camera.eye[2]),
                (self.avatar.animation_time % 86400.0) as f32,
            );
        }
        let mut pass = frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("replicated world particles and flares"),
                // The last world pass resolves MSAA into the frame for the UI.
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: world_target,
                    depth_slice: None,
                    resolve_target: (local_views.is_empty())
                        .then(|| multisampled.as_ref().map(|_| frame.target))
                        .flatten(),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: if multisampled.is_some() && local_views.is_empty() {
                            wgpu::StoreOp::Discard
                        } else {
                            wgpu::StoreOp::Store
                        },
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        if let Some(layer) = &self.gpu.decal_layer {
            layer.render(&mut pass);
        }
        self.foliage.render(&mut pass);
        effects_renderer.render(&mut pass);
        weather_renderer.render(&mut pass);
        self.addons.item_skins.render(&mut pass);
        self.addons.client_code.render(&mut pass);
        if let Some(lines) = &self.gpu.hidden_lines {
            lines.render(&mut pass);
        }
        if let Some(lines) = &self.gpu.region_lines {
            lines.render(&mut pass);
        }
        if let Some(lines) = &self.gpu.selection_lines {
            lines.render(&mut pass);
        }
        if let Some(shapes) = &self.world_shapes {
            shapes.render(&mut pass);
        }
        if local_views.is_empty() {
            if let Some(flare) = &self.gpu.lens_flare {
                flare.render(&mut pass);
            }
            if let Some(vignette) = &self.gpu.vignette {
                vignette.render(&mut pass);
            }
        }
        drop(pass);
        for (index, local) in local_views.iter().enumerate() {
            // Every world-depth consumer has finished. Load the existing color,
            // clear only depth; no world/observer camera or geometry is replayed.
            drop(
                frame
                    .encoder
                    .begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("local held image fresh depth"),
                        color_attachments: &[],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &depth,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(bri_render::scene::DEPTH_CLEAR),
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    }),
            );
            let draws = self
                .world_items
                .local_view_draws(local.affinity.owner, local.affinity.slot);
            let late = |pass: &mut wgpu::RenderPass<'_>| {
                effects_renderer.render_view(pass, LOCAL_IMAGE_FIRST_VIEW + index);
                self.addons.item_skins.render_local_view(pass, index);
                // Screen tint remains above every image; HUD still follows render_scene.
                if index + 1 == local_views.len() {
                    if let Some(flare) = &self.gpu.lens_flare {
                        flare.render(pass);
                    }
                    if let Some(vignette) = &self.gpu.vignette {
                        vignette.render(pass);
                    }
                }
            };
            renderer.render_world(
                frame.encoder,
                bri_render::scene::WorldPass {
                    view: LOCAL_IMAGE_FIRST_VIEW + index,
                    color: world_target,
                    resolve: (index + 1 == local_views.len())
                        .then(|| multisampled.as_ref().map(|_| frame.target))
                        .flatten(),
                    depth: &depth,
                    viewport: None,
                    clear: None,
                    after_opaque: None,
                    after_all: Some(&late),
                },
                &[],
                &draws,
            );
        }
        self.addons.item_skins.resolve(frame.encoder);
        self.addons.client_code.resolve(frame.encoder);
        renderer.end_timing(frame.encoder, "effects");
        self.fx.weapon_frame_observation = weapon_observation;
        Ok(true)
    }

    /// Before the game is drawn: save pictures, renderer rebuilds, icons,
    /// the avatar preview, the splash and the map bake's lightmap patches.
    fn prepare_render(&mut self, frame: &mut RenderContext<'_>) -> Result<()> {
        if let Some(current) = &self.scene.cpu_scene
            && let Ok(root) = self.content.paths.maps.root(&current.id)
            && let Some((visual, compatibility_source)) = self.lighting.light_volume.poll_source(
                root,
                &current.id,
                self.graphics.lighting == 3,
            )
        {
            self.lighting.light_volume.change_source(
                compatibility_source,
                &self.state_dir.join("light-volumes"),
                visual.modern_lights.as_deref(),
            );
            self.lighting.light_volume.animations = visual.light_animations;
            self.lighting.light_volume.spots = visual.light_spots;
            self.lighting.light_volume.overflow = visual.overflow_lights;
            self.scene.cpu_scene = Some(visual.scene);
            self.scene.backdrop = visual.backdrop;
            self.scene.regions = visual.regions;
            self.scene.decals = visual.decals;
            self.scene.cpu_terrain = visual.terrain.into_iter().map(Arc::new).collect();
            self.scene.cpu_water = visual.repeated_water.into_iter().map(Arc::new).collect();
            self.scene.scenery.reset();
            self.scene.shape_indices = visual.shape_indices;
            self.fx.map_movers.replace_scenes(visual.movers);
            self.fx.map_scatter.replace_scenes(visual.scatter);
            crate::frame_trace::note("the map's lighting source changed");
            self.gpu.gpu_scene = None;
            self.lighting.reflections = None;
            self.lighting.environment_probe = None;
        }
        self.update_scenery(frame);
        if self.graphics.lighting != 3 {
            self.lighting
                .light_volume
                .ensure_compatibility(&self.state_dir.join("light-volumes"));
        }
        let effective = self
            .graphics
            .with_lighting(self.lighting.light_volume.mode(self.graphics.lighting));
        // Failures are logged by the writer; success is not news.
        self.files.save_shots.poll(frame.device);
        if let Some(path) = self.files.save_picture.take() {
            self.take_save_picture(frame, path)?;
        }
        // Anti-aliasing and shadow quality rebuild world pipelines and maps;
        // a map change needs renderers built for the new map.
        // Colour-vision assistance is a pipeline constant, too.
        let vision = bri_ui::screens::options::color_vision(&self.ui.core.prefs);
        let restart = std::mem::take(&mut self.gpu.gpu_restart);
        if restart
            || bri_render::color::color_vision() != vision
            || self
                .gpu
                .renderer
                .as_mut()
                .and_then(|r| r.ready())
                .is_some_and(|r| {
                    r.samples() != self.graphics.samples || r.shadow_settings() != effective.shadows
                })
        {
            crate::frame_trace::note("renderers rebuilt for a graphics option or a new map");
            let _span = crate::frame_trace::span("graphics rebuild");
            // A graphics option on the same device and map keeps the world
            // uploaded: sending every chunk again took 0.5-1 s on a
            // 190,000-brick build. The new renderer draws them once it takes
            // them from the one that uploaded them (`adopt_scenes`).
            let same_device = self.gpu.device.as_ref().is_some_and(|(device, _, format)| {
                device == frame.device && *format == frame.format
            });
            let kept = if restart || !same_device {
                crate::frame_trace::note(if restart {
                    "kept no world: the map changed with no device open"
                } else {
                    "kept no world: another device or format"
                });
                None
            } else {
                // Until the newest renderer has taken the scenes, the one it
                // would take them from still owns them.
                let owner = match self.gpu.retired_renderer.take() {
                    Some(owner) => Some(owner),
                    None => self
                        .gpu
                        .renderer
                        .take()
                        .and_then(crate::gpu_build::Building::into_ready),
                };
                if owner.is_none() {
                    crate::frame_trace::note("kept no world: no finished renderer owned it");
                } else if self.gpu.gpu_scene.is_none() {
                    crate::frame_trace::note("kept no world: the map was not uploaded");
                }
                owner.map(|owner| {
                    (
                        owner,
                        self.gpu.gpu_scene.take(),
                        std::mem::take(&mut self.gpu.gpu_terrain),
                        std::mem::take(&mut self.gpu.gpu_water),
                        self.gpu.gpu_palette.take(),
                        std::mem::take(&mut self.gpu.gpu_chunks),
                    )
                })
            };
            self.gpu_ready(frame.device, frame.queue, frame.format)?;
            if let Some((owner, scene, terrain, water, palette, chunks)) = kept {
                self.gpu.retired_renderer = Some(owner);
                self.gpu.gpu_scene = scene;
                self.gpu.gpu_terrain = terrain;
                self.gpu.gpu_water = water;
                self.gpu.gpu_palette = palette;
                self.gpu.gpu_chunks = chunks;
                // The new renderer binds no light volume or map lights yet,
                // and its environment probe faces are drawn again as before.
                self.lighting.light_volume.uploaded = false;
                self.lighting.environment_probe = None;
            }
        }
        // A successful Add-On reload replaces the CPU effects worlds without
        // restarting the device or unrelated scene pipelines. Rebuild their
        // matching atlas before any main, mirror or portal view prepares it.
        if self.gpu.effects_renderer.is_none() {
            self.rebuild_effects_renderer(frame.device, frame.queue, frame.format)?;
        }
        self.item_ui.register_icons(frame);
        self.addons.hud_regions.register_textures(frame);
        // Until its pipelines finish compiling, the preview stays due.
        if (self.avatar.preview_dirty
            || self.ui.stack().contains(&bri_ui::screens::ScreenId::Avatar))
            && let Some((appearance, rotation, distance)) = &self.avatar.preview_request
            && let Some(preview) = self
                .avatar
                .avatar_preview
                .as_mut()
                .context("Avatar preview GPU not initialized")?
                .ready()
        {
            preview.render(
                &self.avatar.avatar_assets,
                appearance,
                *rotation,
                *distance,
                self.avatar.preview_time,
                frame,
            )?;
            self.ui.apply(UiUpdate::AvatarPreview(IconRef::External(
                crate::avatar::Preview::ID,
            )));
            self.avatar.preview_dirty = false;
        }
        // An enabled Add-On's splash over the main menu, once a run.
        if !self.splash_checked
            && self.net.attempt.is_none()
            && self.ui.stack().first() == Some(&bri_ui::screens::ScreenId::MainMenu)
        {
            self.splash_checked = true;
            let prefs = &self.ui.core.prefs;
            let due = self.addons.package_catalog.as_deref().and_then(|catalog| {
                crate::splash::due(catalog, crate::splash::today(), |key| {
                    prefs.get(key).and_then(|v| v.parse().ok())
                })
            });
            if let Some((key, view, pictures)) = due {
                for (id, picture) in &pictures {
                    crate::save_picture::upload_as(frame, *id, picture);
                }
                let year = crate::splash::today().0;
                self.ui.core.prefs.set(&key, year.to_string());
                self.ui.core.save_settings();
                self.ui.apply(UiUpdate::Splash(view));
            }
        }
        if let Some(((map, name), picture)) = self.files.save_previews.ready.take() {
            crate::save_picture::upload(frame, &picture);
            self.ui.apply(UiUpdate::SavePreview {
                map,
                name,
                preview: IconRef::External(crate::save_picture::ID),
            });
        }

        // The map bake's leak cleanup patches the map's lightmaps once: the
        // scene kept for uploads, and the uploaded textures.
        if effective.lighting != 3
            && !self.lighting.light_volume.leaks.is_empty()
            && let Some(scene) = self.scene.cpu_scene.as_mut()
        {
            let fixes = std::mem::take(&mut self.lighting.light_volume.leaks);
            let changed = bri_render::map_lighting::TexelFix::apply(&fixes, &mut scene.images);
            if let Some(gpu) = &self.gpu.gpu_scene {
                gpu.patch_images(frame.queue, &scene.images, &changed)?;
            }
        }
        // Unified's switchable fixtures retain their exact legacy per-texel
        // light shares. Dynamic never equips or patches these images.
        if self.switchable_sheets_due()
            && let Some(scene) = self.scene.cpu_scene.as_mut()
        {
            bri_render::map_lighting::DynamicSheet::equip(
                &self.lighting.light_volume.switchable_sheets,
                scene,
            );
            self.lighting.light_volume.switchable_equipped = true;
            crate::frame_trace::note("the map took its switchable light sheets");
            self.gpu.gpu_scene = None;
        }
        Ok(())
    }
    /// The map bake's switchable sheets wait for the map's images: taking
    /// them uploads the whole scene again (`prepare_render`).
    pub(super) fn switchable_sheets_due(&self) -> bool {
        let effective = self
            .graphics
            .with_lighting(self.lighting.light_volume.mode(self.graphics.lighting));
        let rules = self
            .net
            .attempt
            .as_ref()
            .and_then(|a| a.view.as_ref())
            .is_some_and(|v| !v.map_lights.is_empty());
        let switchable = !self.lighting.light_volume.light_shapes.is_empty() || rules;
        effective.lighting == 2
            && switchable
            && !self.lighting.light_volume.switchable_equipped
            && self.lighting.light_volume.map.is_some()
            && self.scene.cpu_scene.is_some()
    }
}

/// A frame trace note for chunk uploads: how many and how big.
fn note_chunk_uploads(chunks: usize, vertices: usize) {
    if chunks >= 8 || vertices >= 1 << 16 {
        crate::frame_trace::note(format!(
            "uploaded {chunks} brick chunks ({:.1}k vertices)",
            vertices as f64 / 1000.0
        ));
    }
}

/// How long one frame may spend on whole body uploads before the rest wait
/// for the next frame. One always goes, so a body is never held back
/// forever.
const BODY_UPLOAD_BUDGET: Duration = Duration::from_millis(4);

/// Whole body uploads this frame (`render_frame`).
#[derive(Default)]
struct WholeUploads {
    done: usize,
    spent: Duration,
    deferred: usize,
}

impl WholeUploads {
    fn may_start(&mut self) -> bool {
        let may = self.done == 0 || self.spent < BODY_UPLOAD_BUDGET;
        self.deferred += usize::from(!may);
        may
    }
    fn done(&mut self, took: Duration) {
        self.done += 1;
        self.spent += took;
    }
    /// Say in the frame's trace what a slow "avatar meshes" did.
    fn note(&self) {
        if (self.done > 0 && self.spent >= Duration::from_millis(8)) || self.deferred > 0 {
            crate::frame_trace::note(format!(
                "uploaded {} new bodies in {:.1} ms, {} wait",
                self.done,
                self.spent.as_secs_f64() * 1000.0,
                self.deferred
            ));
        }
    }
}

const LOCAL_IMAGE_FIRST_VIEW: usize = bri_render::environment_probe::FIRST_VIEW + 6;
/// The player's camera as the map's backdrop sees it: the same rotation and projection from `eye` (no fog).
fn backdrop_camera(camera: &Camera, eye: [f32; 3]) -> Camera {
    let from = glam::Vec3::new(camera.eye[0], camera.eye[1], camera.eye[2]);
    let shift = glam::Mat4::from_translation(from - glam::Vec3::from(eye));
    let mut out = *camera;
    out.view_projection =
        (glam::Mat4::from_cols_array(&camera.view_projection) * shift).to_cols_array();
    out.eye = [eye[0], eye[1], eye[2], camera.eye[3]];
    out.atmosphere[3] = 0.0;
    out.shading = [0.0; 4];
    out
}
struct LocalImagePass {
    affinity: std::sync::Arc<bri_fx_runtime::LocalViewAffinity>,
    policy: bri_weapons::FirstPersonView,
    camera: Camera,
    effects_camera: bri_fx_runtime::Camera,
    effects: bri_fx_runtime::FrameEffects,
    prebudget_particles: usize,
    cut_sprites: usize,
}
/// Snapshots are far-first from the same eye. Remove globally farthest prefixes
/// before preparing any view, retaining the existing single renderer budget.
fn trim_effect_partitions(
    frames: &mut [&mut bri_fx_runtime::FrameEffects],
    eye: Vec3,
    budget: usize,
) -> Vec<usize> {
    let mut heads = vec![0; frames.len()];
    let excess = frames
        .iter()
        .map(|f| f.particles.len())
        .sum::<usize>()
        .saturating_sub(budget);
    for _ in 0..excess {
        let mut best: Option<(usize, f32)> = None;
        for (index, frame) in frames.iter().enumerate() {
            if let Some(p) = frame.particles.get(heads[index]) {
                let distance = eye.distance_squared(p.position);
                if best.is_none_or(|(_, previous)| distance.total_cmp(&previous).is_gt()) {
                    best = Some((index, distance));
                }
            }
        }
        if let Some((index, _)) = best {
            heads[index] += 1;
        }
    }
    for (frame, &count) in frames.iter_mut().zip(&heads) {
        frame.particles.drain(..count);
    }
    heads
}

#[cfg(test)]
mod local_budget_tests {
    use super::*;
    #[test]
    fn local_and_world_particles_share_one_budget_and_ties_are_stable() {
        let particle = |z| bri_fx_runtime::ParticleInstance {
            lit: false,
            layout: Default::default(),
            position: Vec3::new(0., 0., z),
            size: 1.,
            color: glam::Vec4::ONE,
            spin: 0.,
            axis: Vec3::ZERO,
            plane_right: Vec3::ZERO,
            texture: 0,
            blend: bri_fx_runtime::BlendMode::Alpha,
            depth_test: true,
        };
        let mut world = bri_fx_runtime::FrameEffects {
            particles: vec![particle(9.), particle(3.)],
            lights: vec![],
        };
        let mut first = bri_fx_runtime::FrameEffects {
            particles: vec![particle(9.), particle(2.)],
            lights: vec![],
        };
        let mut second = bri_fx_runtime::FrameEffects {
            particles: vec![particle(8.), particle(1.)],
            lights: vec![],
        };
        let cuts =
            trim_effect_partitions(&mut [&mut world, &mut first, &mut second], Vec3::ZERO, 3);
        assert_eq!(cuts, vec![1, 1, 1]);
        assert_eq!(world.particles[0].position.z, 3.);
        let cuts =
            trim_effect_partitions(&mut [&mut world, &mut first, &mut second], Vec3::ZERO, 2);
        assert_eq!(cuts, vec![1, 0, 0]);
        assert_eq!(first.particles.len() + second.particles.len(), 2);
    }
}

#[cfg(test)]
mod repeated_water_tests {
    use super::*;
    use crate::testing::content_root::ContentRoot;
    use bri_content::{
        scene::{Kind, Node, Scene},
        water::Water,
    };
    use bri_ui::models::admin::AdminAction;

    struct Frames {
        gpu: bri_ui::gpu::Headless,
        ui: bri_ui::gpu::UiRenderer,
        target: wgpu::Texture,
    }
    impl Frames {
        fn new() -> Result<Self> {
            let gpu = bri_ui::gpu::Headless::new()?;
            let ui = bri_ui::gpu::UiRenderer::new(&gpu.device, &gpu.queue);
            let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("repeated water App fixture"),
                size: wgpu::Extent3d {
                    width: 160,
                    height: 120,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            Ok(Self { gpu, ui, target })
        }
        fn open(&self, app: &mut App) -> Result<()> {
            app.gpu_ready(
                &self.gpu.device,
                &self.gpu.queue,
                wgpu::TextureFormat::Rgba8Unorm,
            )
        }
        fn draw(&mut self, app: &mut App) -> Result<bool> {
            let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
            let target = self.target.create_view(&Default::default());
            let drew = app.render_scene(&mut RenderContext {
                device: &self.gpu.device,
                queue: &self.gpu.queue,
                encoder: &mut encoder,
                target: &target,
                format: wgpu::TextureFormat::Rgba8Unorm,
                size: (160, 120),
                ui_renderer: &mut self.ui,
            })?;
            self.gpu.queue.submit([encoder.finish()]);
            Ok(drew)
        }
        fn until(
            &mut self,
            app: &mut App,
            label: &str,
            predicate: impl Fn(&App) -> bool,
        ) -> Result<()> {
            eprintln!("water lifecycle: {label}");
            let start = std::time::Instant::now();
            loop {
                app.tick(Duration::from_millis(16))?;
                app.ui.update(16);
                ensure!(app.pump()?.is_empty(), "unexpected native window command");
                let drew = self.draw(app)?;
                if drew && predicate(app) {
                    return Ok(());
                }
                ensure!(
                    start.elapsed() < Duration::from_secs(300),
                    "{label}: {:?}; admin {}; stack {:?}",
                    app.ui.core.conn,
                    app.ui.core.admin.status,
                    app.ui.stack()
                );
                if let ConnectionState::Failed { reason } = &app.ui.core.conn {
                    anyhow::bail!("{label}: {reason}");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
    fn install_water(app: &App, map: &str, flat: bool) -> Result<()> {
        let root = app.content.paths.maps.root(map)?.to_owned();
        let bundle_path = root.join("bundle.json");
        let mut bundle: serde_json::Value = serde_json::from_slice(&std::fs::read(&bundle_path)?)?;
        let record = bundle["maps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == map)
            .unwrap();
        let scene_path = root.join(record["file"].as_str().unwrap());
        let mut scene: Scene = serde_json::from_slice(&std::fs::read(&scene_path)?)?;
        if flat {
            for node in &mut scene.nodes {
                if matches!(node.kind, Kind::StaticModel | Kind::DatablockModel) {
                    node.kind = Kind::Metadata;
                    node.asset = None;
                }
                if matches!(node.kind, Kind::Interior) {
                    let file = root.join(
                        bundle["assets"][node.asset.as_ref().unwrap()]
                            .as_str()
                            .unwrap(),
                    );
                    let mut interior: serde_json::Value =
                        serde_json::from_slice(&std::fs::read(&file)?)?;
                    for detail in interior["details"].as_array_mut().unwrap() {
                        detail["surfaces"] = serde_json::json!([]);
                    }
                    std::fs::write(file, serde_json::to_vec(&interior)?)?;
                }
            }
        }
        let mut water = Water::volume([-1024., -10., -1024.], [1024., -2., 1024.]);
        water.node = scene.nodes.len();
        water.repeat_period = Some(2048.);
        scene.nodes.push(Node {
            name: "invented repeating water".into(),
            parent: None,
            kind: Kind::Water,
            transform: glam::Mat4::IDENTITY.to_cols_array(),
            asset: None,
            properties: BTreeMap::new(),
        });
        let bytes = crate::testing::png(1, 1, |_, _| [10, 80, 180, 255])?;
        water.surface.sha256 = crate::testing::sha256(&bytes);
        water.shore = water.surface.clone();
        std::fs::write(root.join(&water.surface.file), bytes)?;
        if !bundle["waters"].is_object() {
            bundle["waters"] = serde_json::json!({});
        }
        bundle["waters"][&scene.id] = serde_json::json!([water]);
        std::fs::write(scene_path, serde_json::to_vec(&scene)?)?;
        std::fs::write(bundle_path, serde_json::to_vec(&bundle)?)?;
        Ok(())
    }
    /// Real loader/async map adoption and ordinary frame preparation, all offscreen.
    #[test]
    fn repeated_water_survives_app_graphics_lighting_and_device_then_leaves_with_map() -> Result<()>
    {
        let fixture = ContentRoot::synthetic()?;
        let state = fixture.state()?;
        let mut app = App::load(&fixture.root, state.path(), (160, 120))?;
        install_water(&app, &fixture.map.0, false)?;
        app.graphics.samples = 1;
        app.graphics.bloom = false;
        app.graphics.lighting = 0;
        let mut frames = Frames::new()?;
        frames.open(&mut app)?;
        app.ui.core.request(UiAction::HostGame {
            map: fixture.map.0.clone(),
            mode: ServerMode::SinglePlayer,
            game_mode: None,
            max_players: 1,
            server_name: "Water lifecycle".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        });
        frames.until(&mut app, "initial water", |a| {
            matches!(a.ui.core.conn, ConnectionState::InGame { .. }) && !a.gpu.gpu_water.is_empty()
        })?;
        assert_eq!(app.scene.cpu_water.len(), 1);
        let builds = app.gpu.gpu_water[0].mask_builds();
        assert!(builds > 0 && app.gpu.gpu_water[0].copy_count() > 0);
        for _ in 0..3 {
            frames.draw(&mut app)?;
        }
        assert_eq!(app.gpu.gpu_water[0].mask_builds(), builds);
        let cpu = app.scene.cpu_water[0].clone();
        // Same device, new pipelines: existing geometry/masks must be adopted.
        app.graphics.samples = 4;
        frames.until(&mut app, "same-device graphics", |_| true)?;
        assert_eq!(
            app.gpu
                .renderer
                .as_mut()
                .unwrap()
                .ready()
                .unwrap()
                .samples(),
            4
        );
        assert_eq!(app.gpu.gpu_water[0].mask_builds(), builds);
        assert!(Arc::ptr_eq(&cpu, &app.scene.cpu_water[0]));
        // Actual async loader source replacement, not a manually injected scene.
        app.graphics.lighting = 3;
        frames.until(&mut app, "Dynamic source", |a| {
            a.lighting.light_volume.source_modern && !a.gpu.gpu_water.is_empty()
        })?;
        assert!(!Arc::ptr_eq(&cpu, &app.scene.cpu_water[0]));
        let dynamic = app.scene.cpu_water[0].clone();
        app.graphics.lighting = 0;
        frames.until(&mut app, "compatibility source", |a| {
            !a.lighting.light_volume.source_modern && !a.gpu.gpu_water.is_empty()
        })?;
        assert!(!Arc::ptr_eq(&dynamic, &app.scene.cpu_water[0]));
        // A genuinely different headless device must drop and rebuild GPU ownership.
        app.gpu_stopped();
        assert!(app.gpu.gpu_water.is_empty());
        assert_eq!(app.scene.cpu_water.len(), 1);
        let mut other = Frames::new()?;
        other.open(&mut app)?;
        other.until(&mut app, "new device", |a| !a.gpu.gpu_water.is_empty())?;
        assert!(app.gpu.gpu_water[0].copy_count() > 0);
        app.ui.core.request(UiAction::OpenAdmin);
        other.until(&mut app, "admin", |a| {
            !a.ui.core.admin.busy() && a.ui.stack().contains(&ScreenId::Admin)
        })?;
        app.ui
            .core
            .admin_request(AdminAction::RequestMaps)
            .context("map list request unavailable")?;
        other.until(&mut app, "map list", |a| {
            !a.ui.core.admin.busy()
                && a.ui
                    .core
                    .admin
                    .maps
                    .iter()
                    .any(|m| m.id == fixture.open_map.0)
        })?;
        app.ui
            .core
            .admin_request(AdminAction::ChangeMap {
                map: fixture.open_map.0.clone(),
            })
            .context("map change unavailable")?;
        other.until(&mut app, "map without repeated water", |a| {
            a.scene.scene_map.as_deref() == Some(fixture.open_map.0.as_str())
                && a.scene.cpu_water.is_empty()
                && a.gpu.gpu_water.is_empty()
        })?;
        app.disconnect();
        assert!(app.scene.cpu_water.is_empty() && app.gpu.gpu_water.is_empty());
        Ok(())
    }
    #[test]
    fn repeated_native_water_obeys_flat_scenery_overrides_and_nonflat_maps_keep_it() -> Result<()> {
        use bri_content::{
            atmosphere::{NO_WATER, Settings},
            environment::Image,
            environment_pack::{FILE, Pack, SCHEMA, Water as PlaneWater},
        };
        let fixture = ContentRoot::synthetic()?;
        let env_dir = bri_net::testing::role_dir(&fixture.root, "environments")?;
        let bytes = crate::testing::png(2, 2, |_, _| [20, 140, 210, 255])?;
        let picture = Image {
            file: "fixture.png".into(),
            source: "invented".into(),
            sha256: crate::testing::sha256(&bytes),
            width: 2,
            height: 2,
        };
        let pack = Pack {
            schema_version: SCHEMA,
            source: Some("invented".into()),
            waters: vec![PlaneWater {
                id: "fixture-water".into(),
                label: "Fixture water".into(),
                thumbnail: picture.clone(),
                top: picture.clone(),
                bottom: picture,
                loops_per_unit: 0.2,
                color: [1.; 4],
                overlay_color: [0.1, 0.3, 0.5, 0.2],
                color_multiply: true,
                height: 1.,
                kill: false,
                scroll: [0.; 2],
            }],
            ..Default::default()
        };
        pack.validate()?;
        std::fs::write(env_dir.join("fixture.png"), bytes)?;
        std::fs::write(env_dir.join(FILE), serde_json::to_vec(&pack)?)?;
        let mut packages = bri_package::packages::PackageSet::load_root(&fixture.root)?;
        packages
            .packages
            .push(bri_net::testing::package("environments")?);
        std::fs::write(
            fixture.root.join("packages.json"),
            serde_json::to_vec(&packages)?,
        )?;
        let state = fixture.state()?;
        let mut app = App::load(&fixture.root, state.path(), (160, 120))?;
        install_water(&app, &fixture.map.0, true)?;
        install_water(&app, &fixture.open_map.0, false)?;
        app.graphics.samples = 1;
        app.graphics.bloom = false;
        app.graphics.lighting = 0;
        let mut frames = Frames::new()?;
        frames.open(&mut app)?;
        app.ui.core.request(UiAction::HostGame {
            map: fixture.map.0.clone(),
            mode: ServerMode::SinglePlayer,
            game_mode: None,
            max_players: 1,
            server_name: "Native water scenery".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        });
        frames.until(&mut app, "native flat water", |a| {
            matches!(a.ui.core.conn, ConnectionState::InGame { .. })
                && a.gpu.gpu_water.iter().any(|w| w.copy_count() > 0)
        })?;
        assert!(
            bri_sim::map::NativeMap::load(
                app.content.paths.maps.root(&fixture.map.0)?,
                &fixture.map.0
            )?
            .plate
            .is_some()
        );
        app.ui.core.request(UiAction::OpenAdmin);
        frames.until(&mut app, "scenery admin", |a| {
            !a.ui.core.admin.busy() && a.ui.stack().contains(&ScreenId::Admin)
        })?;
        let set_water = |frames: &mut Frames,
                         app: &mut App,
                         choice: Option<&str>,
                         native: bool|
         -> Result<()> {
            let settings = Settings {
                water: choice.map(str::to_string),
                ..Default::default()
            };
            app.ui
                .core
                .admin_request(AdminAction::SetEnvironment {
                    settings: Box::new(settings),
                })
                .context("environment request unavailable")?;
            frames.until(app, "accepted water scenery", |a| {
                !a.ui.core.admin.busy()
                    && a.network_view()
                        .is_some_and(|v| v.environment.water.as_deref() == choice)
            })?;
            assert_eq!(
                app.gpu.gpu_water.iter().any(|w| w.copy_count() > 0),
                native,
                "native repeated draw ownership must follow accepted scenery {choice:?}"
            );
            assert_eq!(app.scene.scenery.native_water_visible(), native);
            Ok(())
        };
        for choice in [
            Some("fixture-water"),
            Some(NO_WATER),
            Some("missing-water"),
            None,
        ] {
            let native = choice.is_none() || choice == Some("missing-water");
            set_water(&mut frames, &mut app, choice, native)?;
            let custom = app
                .scene
                .cpu_scene
                .as_ref()
                .unwrap()
                .batches
                .iter()
                .any(|b| {
                    app.scene.cpu_scene.as_ref().unwrap().materials[b.material].name
                        == "environment/water-top"
                });
            assert_eq!(custom, choice == Some("fixture-water"));
            if !native {
                assert!(app.gpu.gpu_water.iter().all(|w| w.copy_count() == 0));
            }
        }
        // Restore from an explicit hide, so source geometry must be readmitted.
        set_water(&mut frames, &mut app, Some(NO_WATER), false)?;
        set_water(&mut frames, &mut app, None, true)?;
        app.ui
            .core
            .admin_request(AdminAction::RequestMaps)
            .context("map list unavailable")?;
        frames.until(&mut app, "scenery map list", |a| {
            !a.ui.core.admin.busy()
                && a.ui
                    .core
                    .admin
                    .maps
                    .iter()
                    .any(|m| m.id == fixture.open_map.0)
        })?;
        app.ui
            .core
            .admin_request(AdminAction::ChangeMap {
                map: fixture.open_map.0.clone(),
            })
            .context("nonflat map unavailable")?;
        frames.until(&mut app, "nonflat native water", |a| {
            a.scene.scene_map.as_deref() == Some(fixture.open_map.0.as_str())
                && a.gpu.gpu_water.iter().any(|w| w.copy_count() > 0)
        })?;
        assert!(
            bri_sim::map::NativeMap::load(
                app.content.paths.maps.root(&fixture.open_map.0)?,
                &fixture.open_map.0
            )?
            .plate
            .is_none()
        );
        for choice in [
            Some("fixture-water"),
            Some(NO_WATER),
            Some("missing-water"),
            None,
        ] {
            set_water(&mut frames, &mut app, choice, true)?;
        }
        app.disconnect();
        assert!(app.gpu.gpu_water.is_empty() && app.scene.cpu_water.is_empty());
        Ok(())
    }
}
