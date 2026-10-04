//! Rendering the scene each frame.
use super::*;

impl App {
    pub(super) fn render_frame(&mut self, frame: &mut RenderContext<'_>) -> Result<bool> {
        self.prepare_render(frame)?;
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
        let third_person = draws_third_person(
            controls,
            view.vitals.get(&view.owner).is_none_or(|v| v.alive),
        );
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
            for (owner, placement) in
                crate::packages::body_placements(catalog, &view.archetypes, self.motion.presented())
            {
                hidden.insert(owner);
                match placement {
                    Some(p) if owner == view.owner && !third_person => own_package_body = Some(p),
                    Some(p) => package_placements.push(p),
                    None => {}
                }
            }
        }
        let renderer = self
            .gpu
            .renderer
            .as_mut()
            .context("Scene GPU not initialized")?
            .wait();
        renderer.set_filtering(frame.device, self.graphics.filtering);
        let timing = self.gpu.time_passes || self.ui.core.perf.wants_net();
        renderer.time_passes(frame.device, frame.queue, timing);
        match renderer.pass_times(frame.device) {
            Some((_, passes)) => {
                self.gpu.gpu_passes = passes
                    .iter()
                    .map(|(pass, time)| (*pass, time.as_secs_f32() * 1000.0))
                    .collect();
            }
            None => self.gpu.gpu_passes.clear(),
        }
        if self.gpu.gpu_scene.is_none() {
            self.gpu.gpu_broken.clear();
            self.gpu.gpu_scene = Some(renderer.upload(frame.device, frame.queue, scene)?);
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
        }
        let previous_lighting = self.lighting.light_volume.bound_mode;
        self.lighting.light_volume.upload(
            renderer,
            frame.device,
            frame.queue,
            self.graphics.lighting,
        )?;
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
            );
        }
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
            if pending.iter().map(|c| c.vertices.len()).sum::<usize>() > 1 << 16 {
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
                crate::building::Equipment::Brick(_)
                    | crate::building::Equipment::Hammer
                    | crate::building::Equipment::Wrench
                    | crate::building::Equipment::Printer
                    | crate::building::Equipment::Wand
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
            if (self.gpu.hidden_uploaded != Some(show) || self.gpu.hidden_fading != fading)
                && let Some(lines) = &mut self.gpu.hidden_lines
            {
                let mut vertices = vec![];
                if show {
                    // Hidden bricks, and any fading in or out drawn under
                    // alpha 0.1 (`brick_fade::OUTLINE_ALPHA`).
                    let faint: BTreeSet<u64> = fading
                        .iter()
                        .filter(|(_, faint)| *faint)
                        .map(|(id, _)| *id)
                        .collect();
                    let easing: BTreeSet<u64> = fading.iter().map(|(id, _)| *id).collect();
                    let bricks = view
                        .world
                        .bricks
                        .iter()
                        .filter(|(id, b)| !b.visible && !easing.contains(*id))
                        .chain(
                            faint
                                .iter()
                                .filter_map(|id| Some((id, view.world.bricks.get(id)?))),
                        );
                    for (id, brick) in bricks {
                        if self.fx.brick_debris.is_dead(*id) {
                            continue;
                        }
                        let Some(mesh) = crate::brick_cover::mesh(brick, meshes) else {
                            continue;
                        };
                        let Some(color) = view.world.palette.get(usize::from(brick.color)) else {
                            continue;
                        };
                        let (low, high) = hidden_brick_box(brick, mesh);
                        bri_render::lines::box_edges(
                            low,
                            high,
                            [color[0], color[1], color[2]],
                            &mut vertices,
                        );
                    }
                }
                lines.set_lines(frame.device, &vertices)?;
                self.gpu.hidden_uploaded = Some(show);
                self.gpu.hidden_fading = fading;
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
        for mesh in self.avatar.mount_meshes.values_mut() {
            mesh.upload(renderer, frame.device, frame.queue)?;
        }
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
            .tutorial_targets
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
            .rider_eye
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
        self.view.rendered_camera = Some((eye, yaw, pitch));
        self.view.rendered_roll = roll;
        // Explosion `CameraShake`: 10 degrees of view rotation per unit of offset.
        let shake = self.fx.actor_effects.camera_shake(eye) * 10f32.to_radians();
        let (forward, right, up) = rolled_view_basis(
            yaw + shake.z.clamp(-0.3, 0.3),
            pitch + shake.x.clamp(-0.3, 0.3),
            roll,
        );
        let aspect = frame.size.0 as f32 / frame.size.1 as f32;
        let mut camera = Camera::oriented(
            eye.to_array(),
            forward.to_array(),
            up.to_array(),
            aspect,
            vertical_fov(controls.fov().to_radians(), aspect),
            0.05,
            FAR_PLANE,
        );
        camera.apply_environment(scene);
        // The host's environment (Admin Menu, Add-Ons) over the map's own;
        // an untouched map skips it and draws exactly as authored.
        let live = (!view.environment.is_empty()).then(|| {
            bri_content::atmosphere::resolve(
                &authored_environment(scene),
                &view.environment,
                self.motion.server_tick().unwrap_or(view.tick as f64),
            )
        });
        if let Some(live) = &live {
            camera.apply_atmosphere(live);
        }
        if let Some(vignette) = &mut self.gpu.vignette {
            vignette.update(
                frame.queue,
                live.and_then(|l| l.vignette).map(|v| (v.color, v.multiply)),
                aspect,
            );
        }
        camera.ambient[3] = f32::from(self.lighting.light_volume.mode(self.graphics.lighting));
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
        // Bodies build their mesh here, once the view is known. Without
        // shadows one out of view draws nothing, so it is not built; with
        // shadows or a live mirror every body may show. A mirror shows the
        // player's own body in first person too.
        let in_view =
            crate::culling::Frustum::new(glam::Mat4::from_cols_array(&camera.view_projection));
        let probing = self
            .lighting
            .environment_probe
            .as_ref()
            .is_some_and(|p| !p.faces().is_empty());
        let anywhere = casts || reflecting || probing;
        let mut bodies_drawn = BTreeSet::new();
        let passages = self.motion.passages();
        for (owner, avatar) in &mut self.avatar.avatars {
            if (*owner != view.owner || third_person || anywhere) && !hidden.contains(owner) {
                let (center, radius) = avatar.bounding_sphere();
                // A body part way through an opening draws on both sides.
                avatar.straddle = body_straddle(&self.vehicles, view, &passages, *owner, avatar);
                let seen = |c: Vec3| in_view.sees_sphere(c, radius);
                if !anywhere
                    && !seen(center)
                    && avatar
                        .straddle
                        .is_none_or(|s| !seen(s.carry.transform_point3(center)))
                {
                    continue;
                }
                avatar.build_pending(&self.avatar.avatar_assets)?;
                avatar.upload(renderer, frame.device, frame.queue)?;
                bodies_drawn.insert(*owner);
            }
        }
        let effects_camera = bri_fx_runtime::Camera {
            view_projection: glam::Mat4::from_cols_array(&camera.view_projection),
            position: eye,
            right,
            up,
        };
        if self.addons.client_code.is_started() {
            let world = if self.addons.client_code.reads_world() {
                let image_meshes = self.world_items.held_image_meshes();
                let skeletons = if self.addons.client_code.poses_bodies() {
                    self.avatar
                        .avatars
                        .iter_mut()
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
                    &camera,
                    &self.world_items,
                    image_meshes,
                    crate::client_code::DrawnBodies { skeletons, lives },
                    std::sync::Arc::new(passages.clone()),
                ))
            } else {
                Default::default()
            };
            let player_view = bri_client_sandbox::View {
                fov: self.controls.fov(),
                normal_fov: self.controls.normal_fov(),
                size: [frame.size.0, frame.size.1],
                first_person: !third_person,
                aiming: self.controls.aiming(),
                alive: view.vitals.get(&view.owner).is_none_or(|v| v.alive),
            };
            self.addons.client_code.run_frame(
                self.avatar.animation_time,
                eye,
                forward,
                world,
                player_view,
            );
            for (asset, at, volume) in self.addons.client_code.take_sounds() {
                let placement = match at {
                    Some(at) => bri_audio::Placement::World(bri_audio::Vec3::from(at)),
                    None => bri_audio::Placement::Listener,
                };
                self.audio.play_asset(asset, placement, volume);
            }
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
        let world_frame = self.fx.effects.world.snapshot_in_view(&effects_camera);
        let weapon_frame = self
            .fx
            .weapon_effects
            .world()
            .snapshot_in_view(&effects_camera);
        let actor_frame = self
            .fx
            .actor_effects
            .world()
            .snapshot_in_view(&effects_camera);
        let (effects_frame, deferred_lights) =
            combine_effect_frames(world_frame, [weapon_frame, actor_frame], &eyes);
        let (fog_start, fog_end) = if camera.atmosphere[3] > 0. {
            (camera.atmosphere[0], camera.atmosphere[1])
        } else {
            (cap, cap + 1.)
        };
        for terrain in &mut self.gpu.gpu_terrain {
            terrain.update(frame.device, frame.queue, &eyes, fog_end.max(1.))?;
        }
        self.ui.core.name_tags = name_tags(
            view,
            self.motion.presented(),
            &self.content.weapons.pack,
            self.build.building.as_ref(),
            glam::Mat4::from_cols_array(&camera.view_projection),
            eye,
            (fog_start.max(0.), fog_end.max(1.)),
            (frame.size.0 as f32, frame.size.1 as f32),
            self.ui.scale(),
            controls.observer().is_none(),
            &passages,
            |drop| self.world_items.drop_center(drop),
        );
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
            .clamp(0.0, 4.0)
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
        self.fx.weapon_light_deferred = deferred_lights;
        // Player lights are effect lights too; the nearest to the camera win.
        let lights: Vec<_> = effects_frame
            .lights
            .iter()
            .map(|light| bri_render::scene::PointLight {
                position_radius: light.position.extend(light.radius).to_array(),
                color: light.color.extend(0.).to_array(),
            })
            .collect();
        renderer.update_lights(frame.queue, &lights)?;
        let effects_renderer = self
            .gpu
            .effects_renderer
            .as_mut()
            .context("Effects GPU not initialized")?;
        effects_renderer.set_fog(camera.atmosphere, camera.fog_color);
        effects_renderer.prepare(frame.queue, &effects_camera, &effects_frame)?;
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
            .values()
            .filter_map(|mesh| Some((mesh.gpu.as_ref()?, mesh.instance.as_ref()?)))
            .collect();
        scenes.extend(self.fx.fade_models.scenes());
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
        shared_draws.extend(self.fx.tutorial_targets.draws());
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
        {
            use bri_render::scene::ShadowCasters;
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
            renderer.begin_timing(frame.encoder);
            let terrain_map: Vec<_> = if effective.lighting == 3 {
                self.gpu
                    .gpu_terrain
                    .iter()
                    .flat_map(|t| t.draws())
                    .collect()
            } else {
                Vec::new()
            };
            renderer.render_shadows_with_geometry(
                frame.encoder,
                ShadowCasters {
                    scenes: &bodies,
                    instances: &models,
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
            reflections.render_views(renderer, frame.encoder, &scenes, &models, clear, &late);
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
        let surfaces = |pass: &mut wgpu::RenderPass<'_>| reflections.draw_surfaces(pass, 0);
        renderer.render_world(
            frame.encoder,
            bri_render::scene::WorldPass {
                view: 0,
                color: world_target,
                resolve: None,
                depth: &depth,
                viewport: None,
                clear: Some(clear),
                after_opaque: (!mirrors.is_empty()).then_some(&surfaces as _),
                after_all: None,
            },
            &scenes,
            &item_draws,
        );
        renderer.mark(frame.encoder, "world");
        let mut pass = frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("replicated world particles and flares"),
                // The last world pass resolves MSAA into the frame for the UI.
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: world_target,
                    depth_slice: None,
                    resolve_target: multisampled.as_ref().map(|_| frame.target),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: if multisampled.is_some() {
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
        if let Some(vignette) = &self.gpu.vignette {
            vignette.render(&mut pass);
        }
        drop(pass);
        self.addons.item_skins.resolve(frame.encoder);
        self.addons.client_code.resolve(frame.encoder);
        renderer.end_timing(frame.encoder, "effects");
        Ok(true)
    }

    /// Before the game is drawn: save pictures, renderer rebuilds, icons,
    /// the avatar preview, the splash and the map bake's lightmap patches.
    fn prepare_render(&mut self, frame: &mut RenderContext<'_>) -> Result<()> {
        if let Some(current) = &self.scene.cpu_scene
            && let Some((visual, compatibility_source)) = self.lighting.light_volume.poll_source(
                &self.content.paths.map_bundle,
                &current.id,
                self.graphics.lighting == 3,
            )
        {
            self.lighting.light_volume.change_source(
                compatibility_source,
                &self.state_dir.join("light-volumes"),
                visual.modern_lights.as_deref(),
            );
            self.scene.cpu_scene = Some(visual.scene);
            self.scene.cpu_terrain = visual.terrain.into_iter().map(Arc::new).collect();
            self.scene.shape_indices = visual.shape_indices;
            self.gpu.gpu_scene = None;
            self.lighting.reflections = None;
            self.lighting.environment_probe = None;
        }
        if self.graphics.lighting != 3 {
            self.lighting
                .light_volume
                .ensure_compatibility(&self.state_dir.join("light-volumes"));
        }
        let effective = self
            .graphics
            .with_lighting(self.lighting.light_volume.mode(self.graphics.lighting));
        // The last frame, holding any picture copied then, was submitted.
        // Failures are logged by the writer; success is not news.
        self.files.save_shots.submitted();
        self.files.save_shots.poll(frame.device);
        if let Some(path) = self.files.save_picture.take() {
            self.take_save_picture(frame, path)?;
        }
        // Anti-aliasing and shadow quality rebuild world pipelines and maps;
        // a map change needs renderers built for the new map.
        // Colour-vision assistance is a pipeline constant, too.
        let vision = bri_ui::screens::options::color_vision(&self.ui.core.prefs);
        if std::mem::take(&mut self.gpu.gpu_restart)
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
            self.gpu_ready(frame.device, frame.queue, frame.format)?;
        }
        // A successful Add-On reload replaces the CPU effects worlds without
        // restarting the device or unrelated scene pipelines. Rebuild their
        // matching atlas before any main, mirror or portal view prepares it.
        if self.gpu.effects_renderer.is_none() {
            self.rebuild_effects_renderer(frame.device, frame.queue, frame.format)?;
        }
        self.item_ui.register_icons(frame);
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
        let rules = self
            .net
            .attempt
            .as_ref()
            .and_then(|a| a.view.as_ref())
            .is_some_and(|v| !v.map_lights.is_empty());
        let switchable = !self.lighting.light_volume.light_shapes.is_empty() || rules;
        if (effective.lighting == 2 && switchable)
            && !self.lighting.light_volume.switchable_equipped
            && self.lighting.light_volume.map.is_some()
            && let Some(scene) = self.scene.cpu_scene.as_mut()
        {
            bri_render::map_lighting::DynamicSheet::equip(
                &self.lighting.light_volume.switchable_sheets,
                scene,
            );
            self.lighting.light_volume.switchable_equipped = true;
            self.gpu.gpu_scene = None;
        }
        Ok(())
    }
}
