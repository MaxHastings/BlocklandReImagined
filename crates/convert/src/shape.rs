//! Bounded DTS/DSQ v24 readers and lowering into native meshes/animation tracks.
use crate::Reader;
use anyhow::{Context, Result, bail, ensure};
use bri_content::shape::*;
use glam::{Mat4, Vec3};
use serde::Serialize;

const MAX: usize = 1_000_000;
fn axis([x, y, z]: [f32; 3]) -> [f32; 3] {
    [x, z, -y]
}
fn scale_axis([x, y, z]: [f32; 3]) -> [f32; 3] {
    [x, z, y]
}
fn vec3(r: &mut Reader<'_>) -> Result<[f32; 3]> {
    Ok([r.f32()?, r.f32()?, r.f32()?])
}
fn rotation(r: &mut Reader<'_>) -> Result<[f32; 4]> {
    let q = [
        r.u16()? as i16,
        r.u16()? as i16,
        r.u16()? as i16,
        r.u16()? as i16,
    ]
    .map(|x| x as f32 / 32767.0);
    let length = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    ensure!(length > 0.001, "Zero quaternion");
    // Torque serializes a conjugated rotation. Conjugate, then change basis.
    Ok([-q[0] / length, -q[2] / length, q[1] / length, q[3] / length])
}
fn collect<T>(count: usize, mut read: impl FnMut() -> Result<T>) -> Result<Vec<T>> {
    ensure!(count <= MAX, "Array exceeds limit");
    (0..count).map(|_| read()).collect()
}
fn subset<T: Clone>(data: &[T], start: usize, count: usize) -> Result<Vec<T>> {
    // An empty range's start is not checked: exporters leave stale starts on
    // empty lists (the Stunt Plane's `propfast` has no triggers at index 4).
    if count == 0 {
        return Ok(vec![]);
    }
    let end = start.checked_add(count).context("Range overflow")?;
    Ok(data
        .get(start..end)
        .context("Array range outside source")?
        .to_vec())
}
fn index(value: i32) -> Result<usize> {
    usize::try_from(value).context("Negative array index")
}
fn optional(value: i32) -> Result<Option<usize>> {
    if value == -1 {
        Ok(None)
    } else {
        Ok(Some(index(value)?))
    }
}
fn name(names: &[String], value: i32) -> Result<String> {
    Ok(names
        .get(index(value)?)
        .context("Invalid name index")?
        .clone())
}

#[derive(Serialize)]
pub struct Provenance {
    pub version: u32,
    pub exporter: u32,
    pub warnings: Vec<String>,
    pub material_flags: Vec<u32>,
    pub mesh_flags: Vec<u32>,
    pub opaque_sections: Vec<Vec<u32>>,
}
impl Provenance {
    fn new(version: u32, exporter: u32) -> Self {
        Self {
            version,
            exporter,
            warnings: vec![],
            material_flags: vec![],
            mesh_flags: vec![],
            opaque_sections: vec![],
        }
    }
}

struct Tri<'a> {
    wide: Reader<'a>,
    short: Reader<'a>,
    byte: Reader<'a>,
    guard: u32,
}
impl<'a> Tri<'a> {
    fn new(file: &mut Reader<'a>) -> Result<Self> {
        let size = file.count(16_000_000)?;
        let first16 = file.count(size)?;
        let first8 = file.count(size)?;
        ensure!(first16 <= first8, "Invalid DTS split-buffer offsets");
        let buffer = file.bytes(size.checked_mul(4).context("Buffer size overflow")?)?;
        Ok(Self {
            wide: Reader::new(&buffer[..first16 * 4]),
            short: Reader::new(&buffer[first16 * 4..first8 * 4]),
            byte: Reader::new(&buffer[first8 * 4..]),
            guard: 0,
        })
    }
    fn guard(&mut self, label: &str) -> Result<()> {
        let values = (self.wide.u32()?, self.short.u16()?, self.byte.u8()?);
        ensure!(
            values == (self.guard, self.guard as u16, self.guard as u8),
            "DTS guard mismatch at {label}: {values:?} expected {}",
            self.guard
        );
        self.guard += 1;
        Ok(())
    }
    fn string(&mut self) -> Result<String> {
        let mut bytes = Vec::new();
        loop {
            let b = self.byte.u8()?;
            if b == 0 {
                break;
            }
            ensure!(bytes.len() < 4096, "Name too long");
            bytes.push(b);
        }
        Ok(String::from_utf8(bytes)?)
    }
    fn finish(&self) -> Result<()> {
        ensure!(
            self.wide.remaining() == 0 && self.short.remaining() < 4 && self.byte.remaining() < 4,
            "Unconsumed DTS split buffers: {}/{}/{}",
            self.wide.remaining(),
            self.short.remaining(),
            self.byte.remaining()
        );
        Ok(())
    }
}

#[derive(Default)]
struct Pools {
    rotations: Vec<[f32; 4]>,
    translations: Vec<[f32; 3]>,
    uniform: Vec<f32>,
    aligned: Vec<[f32; 3]>,
    arbitrary_rotations: Vec<[f32; 4]>,
    arbitrary_factors: Vec<[f32; 3]>,
    ground_translations: Vec<[f32; 3]>,
    ground_rotations: Vec<[f32; 4]>,
    objects: Vec<State>,
    triggers: Vec<Trigger>,
}
#[derive(Clone)]
struct State {
    visibility: f32,
    frame: u32,
    material: u32,
}
fn state(r: &mut Reader<'_>) -> Result<State> {
    Ok(State {
        visibility: r.f32()?,
        frame: r.u32()?,
        material: r.u32()?,
    })
}
struct Sequence {
    name: String,
    flags: u32,
    frames: usize,
    duration: f32,
    priority: i32,
    ground: usize,
    ground_count: usize,
    rotation: usize,
    translation: usize,
    scale: usize,
    object: usize,
    trigger: usize,
    trigger_count: usize,
    members: [Vec<usize>; 8],
}
fn bitset(r: &mut Reader<'_>) -> Result<Vec<usize>> {
    let _obsolete = r.u32()?;
    let count = r.count(8192)?;
    let mut indices = Vec::new();
    for word in 0..count {
        let bits = r.u32()?;
        for bit in 0..32 {
            if bits & (1 << bit) != 0 {
                indices.push(word * 32 + bit);
            }
        }
    }
    Ok(indices)
}
fn sequence(r: &mut Reader<'_>, name: String) -> Result<Sequence> {
    let flags = r.u32()?;
    let frames = r.count(100_000)?;
    let duration = r.f32()?;
    let priority = r.i32()?;
    let ground = r.count(MAX)?;
    let ground_count = r.count(MAX)?;
    let rotation = r.count(MAX)?;
    let translation = r.count(MAX)?;
    let scale = r.count(MAX)?;
    let object = r.count(MAX)?;
    let _decal = r.i32()?;
    let trigger = r.count(MAX)?;
    let trigger_count = r.count(MAX)?;
    let _tool_begin = r.f32()?;
    let mut members: [Vec<usize>; 8] = Default::default();
    for set in &mut members {
        *set = bitset(r)?;
    }
    Ok(Sequence {
        name,
        flags,
        frames,
        duration,
        priority,
        ground,
        ground_count,
        rotation,
        translation,
        scale,
        object,
        trigger,
        trigger_count,
        members,
    })
}
fn lower_sequence(
    s: Sequence,
    names: &[String],
    p: &Pools,
    provenance: &mut Provenance,
) -> Result<Animation> {
    let mut tracks = std::collections::BTreeMap::new();
    for slot in 0..3 {
        for (rank, &node) in s.members[slot].iter().enumerate() {
            let track = tracks.entry(node).or_insert_with(|| NodeTrack {
                node: String::new(),
                rotations: vec![],
                translations: vec![],
                scales: vec![],
                scale_rotations: vec![],
            });
            track.node = names
                .get(node)
                .context("Animation references absent node")?
                .clone();
            let offset = rank
                .checked_mul(s.frames)
                .context("Animation range overflow")?;
            match slot {
                0 => track.rotations = subset(&p.rotations, s.rotation + offset, s.frames)?,
                1 => {
                    track.translations = subset(&p.translations, s.translation + offset, s.frames)?
                }
                _ => {
                    if s.flags & 1 != 0 {
                        track.scales = subset(&p.uniform, s.scale + offset, s.frames)?
                            .into_iter()
                            .map(|v| [v; 3])
                            .collect();
                    } else if s.flags & 2 != 0 {
                        track.scales = subset(&p.aligned, s.scale + offset, s.frames)?;
                    } else if s.flags & 4 != 0 {
                        track.scales = subset(&p.arbitrary_factors, s.scale + offset, s.frames)?;
                        track.scale_rotations =
                            subset(&p.arbitrary_rotations, s.scale + offset, s.frames)?;
                    } else {
                        bail!("Scale membership without scale mode");
                    }
                }
            }
        }
    }
    if !s.members[3].is_empty() || !s.members[4].is_empty() {
        provenance.warnings.push(format!(
            "{} has legacy decal/IFL animation membership requiring material adaptation",
            s.name
        ));
    }
    let union: std::collections::BTreeSet<_> = s.members[5..].iter().flatten().copied().collect();
    let mut objects = Vec::new();
    for (rank, object) in union.into_iter().enumerate() {
        let states = subset(&p.objects, s.object + rank * s.frames, s.frames)?;
        objects.push(ObjectTrack {
            object,
            visibility: if s.members[5].contains(&object) {
                states.iter().map(|s| s.visibility).collect()
            } else {
                vec![]
            },
            frames: if s.members[6].contains(&object) {
                states.iter().map(|s| s.frame).collect()
            } else {
                vec![]
            },
            material_frames: if s.members[7].contains(&object) {
                states.iter().map(|s| s.material).collect()
            } else {
                vec![]
            },
        });
    }
    let animation = Animation {
        name: s.name,
        frames: s.frames,
        duration: s.duration,
        looping: s.flags & 16 != 0,
        additive: s.flags & 8 != 0,
        priority: s.priority,
        nodes: tracks.into_values().collect(),
        objects,
        ground_translations: subset(&p.ground_translations, s.ground, s.ground_count)?,
        ground_rotations: subset(&p.ground_rotations, s.ground, s.ground_count)?,
        triggers: subset(&p.triggers, s.trigger, s.trigger_count)?,
    };
    animation.validate()?;
    Ok(animation)
}

fn shared<T: Clone>(
    count: usize,
    parent: Option<usize>,
    previous: &[Option<Mesh>],
    get: impl Fn(&Mesh) -> &Vec<T>,
    read: impl FnMut() -> Result<T>,
) -> Result<Vec<T>> {
    if let Some(parent) = parent {
        subset(
            get(previous
                .get(parent)
                .and_then(Option::as_ref)
                .context("Invalid shared mesh parent")?),
            0,
            count,
        )
    } else {
        collect(count, read)
    }
}

fn mesh(
    t: &mut Tri<'_>,
    previous: &[Option<Mesh>],
    provenance: &mut Provenance,
) -> Result<Option<Mesh>> {
    let kind = t.wide.u32()?;
    let kind = kind & 7;
    if kind == 4 {
        return Ok(None);
    }
    ensure!(kind == 0 || kind == 1, "Unsupported DTS mesh kind {kind}");
    t.guard("mesh header")?;
    let _frames = t.wide.count(MAX)?;
    let _material_frames = t.wide.count(MAX)?;
    let parent = optional(t.wide.i32()?)?;
    for _ in 0..10 {
        t.wide.f32()?;
    }
    let count = t.wide.count(MAX)?;
    let mut positions = shared(
        count,
        parent,
        previous,
        |m| &m.positions,
        || Ok(axis(vec3(&mut t.wide)?)),
    )?;
    let uv_count = t.wide.count(MAX)?;
    let uv = shared(
        uv_count,
        parent,
        previous,
        |m| &m.uv,
        || Ok([t.wide.f32()?, t.wide.f32()?]),
    )?;
    let mut normals = shared(
        count,
        parent,
        previous,
        |m| &m.normals,
        || Ok(axis(vec3(&mut t.wide)?)),
    )?;
    if parent.is_none() {
        t.byte.bytes(count)?;
    }
    let primitive_count = t.wide.count(MAX)?;
    let spans = collect(primitive_count, || {
        Ok((t.short.u16()? as usize, t.short.u16()? as usize))
    })?;
    let flags = collect(primitive_count, || t.wide.u32())?;
    let count = t.wide.count(MAX)?;
    let indices = collect(count, || Ok(t.short.u16()? as u32))?;
    let count = t.wide.count(MAX)?;
    provenance
        .opaque_sections
        .push(collect(count, || Ok(t.short.u16()? as u32))?);
    let frame_vertices = t.wide.count(MAX)?;
    let mesh_flags = t.wide.u32()?;
    provenance.mesh_flags.push(mesh_flags);
    t.guard("mesh data")?;
    let skin = if kind == 1 {
        let count = t.wide.count(MAX)?;
        positions = shared(
            count,
            parent,
            previous,
            |m| &m.positions,
            || Ok(axis(vec3(&mut t.wide)?)),
        )?;
        normals = shared(
            count,
            parent,
            previous,
            |m| &m.normals,
            || Ok(axis(vec3(&mut t.wide)?)),
        )?;
        if parent.is_none() {
            t.byte.bytes(count)?;
        }
        let count = t.wide.count(65536)?;
        let mut inverse_bind = Vec::new();
        if let Some(parent) = parent {
            inverse_bind = subset(
                &previous
                    .get(parent)
                    .and_then(Option::as_ref)
                    .and_then(|m| m.skin.as_ref())
                    .context("Missing parent skin")?
                    .inverse_bind,
                0,
                count,
            )?;
        }
        for _ in 0..if parent.is_none() { count } else { 0 } {
            let values: Vec<f32> = collect(16, || t.wide.f32())?;
            let m = Mat4::from_cols_array(&values.try_into().unwrap()).transpose();
            let basis = Mat4::from_cols(
                Vec3::X.extend(0.0),
                (-Vec3::Z).extend(0.0),
                Vec3::Y.extend(0.0),
                glam::Vec4::W,
            );
            inverse_bind.push((basis * m * basis.transpose()).to_cols_array());
        }
        let count = t.wide.count(MAX)?;
        let influences = if let Some(parent) = parent {
            subset(
                &previous
                    .get(parent)
                    .and_then(Option::as_ref)
                    .and_then(|m| m.skin.as_ref())
                    .context("Missing parent skin")?
                    .influences,
                0,
                count,
            )?
        } else {
            let vertices = collect(count, || t.wide.count(MAX))?;
            let bones = collect(count, || t.wide.count(65536))?;
            let weights = collect(count, || t.wide.f32())?;
            (0..count)
                .map(|i| Influence {
                    vertex: vertices[i],
                    bone: bones[i],
                    weight: weights[i],
                })
                .collect()
        };
        let count = t.wide.count(65536)?;
        let nodes = if let Some(parent) = parent {
            subset(
                &previous
                    .get(parent)
                    .and_then(Option::as_ref)
                    .and_then(|m| m.skin.as_ref())
                    .context("Missing parent skin")?
                    .nodes,
                0,
                count,
            )?
        } else {
            collect(count, || t.wide.count(65536))?
        };
        t.guard("skin data")?;
        Some(Skin {
            inverse_bind,
            nodes,
            influences,
        })
    } else {
        None
    };
    let mut primitives = Vec::new();
    for ((start, count), flags) in spans.into_iter().zip(flags) {
        let elements = if flags & 0x20000000 != 0 {
            subset(&indices, start, count)?
        } else {
            (start..start + count).map(|i| i as u32).collect()
        };
        let mut triangles = Vec::new();
        match flags >> 30 {
            0 => {
                ensure!(elements.len() % 3 == 0, "Incomplete triangle list");
                for tri in elements.chunks_exact(3) {
                    triangles.push([tri[0], tri[2], tri[1]]);
                }
            }
            1 => {
                for i in 2..elements.len() {
                    let a = elements[i - 2];
                    let b = elements[i - 1];
                    let c = elements[i];
                    if a != b && b != c && a != c {
                        triangles.push(if i % 2 == 0 { [a, c, b] } else { [b, c, a] });
                    }
                }
            }
            2 => {
                for i in 2..elements.len() {
                    triangles.push([elements[0], elements[i], elements[i - 1]]);
                }
            }
            _ => bail!("Unknown primitive topology"),
        }
        primitives.push(Primitive {
            material: if flags & 0x10000000 != 0 {
                None
            } else {
                Some((flags & 0x0fffffff) as usize)
            },
            triangles,
        });
    }
    if frame_vertices == 0
        && positions.is_empty()
        && normals.is_empty()
        && primitives.iter().all(|p| p.triangles.is_empty())
    {
        provenance
            .warnings
            .push("Explicit empty mesh slot normalized to null".into());
        return Ok(None);
    }
    Ok(Some(Mesh {
        frame_vertices,
        positions,
        normals,
        uv,
        primitives,
        skin,
        billboard: mesh_flags & 0x80000000 != 0,
        billboard_y: mesh_flags & 0x20000000 != 0,
    }))
}

pub fn read_dts(data: &[u8], id: String) -> Result<(Shape, Provenance)> {
    let mut file = Reader::new(data);
    let version = file.u32()?;
    ensure!(
        version & 255 == 24,
        "Unsupported DTS version {}",
        version & 255
    );
    let mut provenance = Provenance::new(24, version >> 16);
    let mut t = Tri::new(&mut file)?;
    let counts = collect(17, || t.wide.count(MAX))?;
    let [
        nodes,
        objects,
        decals,
        subshapes,
        ifls,
        rotations,
        translations,
        uniform,
        aligned,
        arbitrary,
        ground,
        states,
        decal_states,
        triggers,
        details,
        meshes,
        names,
    ]: [usize; 17] = counts.try_into().unwrap();
    t.wide.u32()?;
    t.wide.i32()?;
    t.guard("shape counts")?;
    for _ in 0..11 {
        t.wide.f32()?;
    }
    t.guard("shape bounds")?;
    let node_records = collect(nodes, || {
        let a = t.wide.i32()?;
        let b = t.wide.i32()?;
        for _ in 0..3 {
            t.wide.i32()?;
        }
        Ok((a, b))
    })?;
    t.guard("nodes")?;
    let object_records = collect(objects, || {
        let row = collect(6, || t.wide.i32())?;
        Ok(row)
    })?;
    t.guard("objects")?;
    provenance
        .opaque_sections
        .push(collect(decals * 5, || t.wide.u32())?);
    t.guard("decals")?;
    provenance
        .opaque_sections
        .push(collect(ifls * 5, || t.wide.u32())?);
    t.guard("IFL")?;
    if decals > 0 || ifls > 0 {
        provenance.warnings.push(format!(
            "{decals} decal and {ifls} IFL records retained for material adaptation"
        ));
    }
    let starts = collect(subshapes * 3, || t.wide.count(MAX))?;
    t.guard("subshape starts")?;
    let sizes = collect(subshapes * 3, || t.wide.count(MAX))?;
    t.guard("subshape counts")?;
    let bind_rotations = collect(nodes, || rotation(&mut t.short))?;
    let bind_translations = collect(nodes, || Ok(axis(vec3(&mut t.wide)?)))?;
    let mut pools = Pools {
        translations: collect(translations, || Ok(axis(vec3(&mut t.wide)?)))?,
        rotations: collect(rotations, || rotation(&mut t.short))?,
        ..Default::default()
    };
    t.guard("node animation")?;
    pools.uniform = collect(uniform, || t.wide.f32())?;
    pools.aligned = collect(aligned, || Ok(scale_axis(vec3(&mut t.wide)?)))?;
    pools.arbitrary_factors = collect(arbitrary, || Ok(scale_axis(vec3(&mut t.wide)?)))?;
    pools.arbitrary_rotations = collect(arbitrary, || rotation(&mut t.short))?;
    t.guard("scale animation")?;
    pools.ground_translations = collect(ground, || Ok(axis(vec3(&mut t.wide)?)))?;
    pools.ground_rotations = collect(ground, || rotation(&mut t.short))?;
    t.guard("ground animation")?;
    pools.objects = collect(states, || state(&mut t.wide))?;
    t.guard("object states")?;
    provenance
        .opaque_sections
        .push(collect(decal_states, || t.wide.u32())?);
    t.guard("decal states")?;
    pools.triggers = collect(triggers, || {
        Ok(Trigger {
            state: t.wide.u32()?,
            position: t.wide.f32()?,
        })
    })?;
    t.guard("triggers")?;
    let detail_records = collect(details, || {
        Ok((
            t.wide.i32()?,
            t.wide.i32()?,
            t.wide.i32()?,
            t.wide.f32()?,
            t.wide.u32()?,
            t.wide.u32()?,
            t.wide.u32()?,
        ))
    })?;
    t.guard("details")?;
    let mut mesh_data = Vec::new();
    for i in 0..meshes {
        mesh_data
            .push(mesh(&mut t, &mesh_data, &mut provenance).with_context(|| format!("Mesh {i}"))?);
    }
    t.guard("mesh list")?;
    let names = collect(names, || t.string())?;
    t.guard("names")?;
    t.finish()?;
    let mut shape_nodes = Vec::new();
    for (i, (node, parent)) in node_records.into_iter().enumerate() {
        shape_nodes.push(Node {
            name: name(&names, node)?,
            parent: optional(parent)?,
            translation: bind_translations[i],
            rotation: bind_rotations[i],
        });
    }
    let mut shape_objects = Vec::new();
    for (i, record) in object_records.into_iter().enumerate() {
        let state = pools
            .objects
            .get(i)
            .context("Missing default object state")?;
        let start = index(record[2])?;
        let count = index(record[1])?;
        ensure!(
            start
                .checked_add(count)
                .is_some_and(|n| n <= mesh_data.len()),
            "Object mesh span out of bounds"
        );
        shape_objects.push(Object {
            name: name(&names, record[0])?,
            node: optional(record[3])?,
            meshes: (start..start + count).collect(),
            visibility: state.visibility,
            frame: state.frame,
            material_frame: state.material,
        });
    }
    let mut shape_details = Vec::new();
    for (label, subshape, offset, size, _, _, _) in detail_records {
        if subshape < 0 {
            provenance.warnings.push(format!(
                "Auto billboard detail {} requires renderer adaptation",
                name(&names, label)?
            ));
            continue;
        }
        let sub = index(subshape)?;
        ensure!(sub < subshapes, "Invalid detail subshape");
        shape_details.push(Detail {
            name: name(&names, label)?,
            pixel_threshold: size,
            object_start: starts[subshapes + sub],
            object_count: sizes[subshapes + sub],
            mesh_offset: index(offset)?,
            collision: size < 0.0,
        });
    }
    let count = file.count(100_000)?;
    let mut sequences = Vec::new();
    for _ in 0..count {
        let n = name(&names, file.i32()?)?;
        sequences.push(sequence(&mut file, n)?);
    }
    ensure!(file.u8()? == 1, "Unsupported material list");
    let count = file.count(100_000)?;
    let material_names = collect(count, || file.string8())?;
    let flags = collect(count, || file.u32())?;
    let reflectance = collect(count, || file.i32())?;
    let bump = collect(count, || file.i32())?;
    let detail = collect(count, || file.i32())?;
    let detail_scale = collect(count, || file.f32())?;
    let reflection = collect(count, || file.f32())?;
    let mut materials = Vec::new();
    for (i, n) in material_names.into_iter().enumerate() {
        let f = flags[i];
        materials.push(Material {
            name: n,
            wrap_u: f & 1 != 0,
            wrap_v: f & 2 != 0,
            blend: if f & 8 != 0 {
                "additive"
            } else if f & 16 != 0 {
                "subtractive"
            } else if f & 4 != 0 {
                "alpha"
            } else {
                "opaque"
            }
            .into(),
            unlit: f & 32 != 0,
            environment: f & 64 == 0,
            mipmaps: f & 128 == 0,
            reflectance_map: optional(reflectance[i])?,
            bump_map: optional(bump[i])?,
            detail_map: optional(detail[i])?,
            detail_scale: detail_scale[i],
            reflectance: reflection[i],
        });
    }
    provenance.material_flags = flags;
    file.finish()?;
    let node_names = shape_nodes
        .iter()
        .map(|n| n.name.clone())
        .collect::<Vec<_>>();
    let mut animations = Vec::new();
    for s in sequences {
        let label = s.name.clone();
        animations.push(
            lower_sequence(s, &node_names, &pools, &mut provenance)
                .with_context(|| format!("Sequence {label}"))?,
        );
    }
    let shape = Shape {
        schema_version: 1,
        id,
        nodes: shape_nodes,
        objects: shape_objects,
        details: shape_details,
        meshes: mesh_data,
        materials,
        animations,
    };
    shape.validate()?;
    Ok((shape, provenance))
}

pub fn read_dsq(data: &[u8], id: String) -> Result<(ClipSet, Provenance)> {
    let mut r = Reader::new(data);
    let version = r.u32()?;
    ensure!(version == 24, "Unsupported DSQ version {version}");
    let mut provenance = Provenance::new(version, 0);
    let count = r.count(100_000)?;
    let names = collect(count, || {
        let bytes = r.blob32(4096)?;
        Ok(String::from_utf8(bytes)?)
    })?;
    let legacy_objects = r.count(MAX)?;
    ensure!(
        legacy_objects == 0,
        "Legacy DSQ object name table requires support"
    );
    let _source_objects = r.count(MAX)?;
    let count = r.count(MAX)?;
    let rotations = collect(count, || rotation(&mut r))?;
    let count = r.count(MAX)?;
    let translations = collect(count, || Ok(axis(vec3(&mut r)?)))?;
    let count = r.count(MAX)?;
    let uniform = collect(count, || r.f32())?;
    let count = r.count(MAX)?;
    let aligned = collect(count, || Ok(scale_axis(vec3(&mut r)?)))?;
    let count = r.count(MAX)?;
    let arbitrary_rotations = collect(count, || rotation(&mut r))?;
    let arbitrary_factors = collect(count, || Ok(scale_axis(vec3(&mut r)?)))?;
    let count = r.count(MAX)?;
    let ground_translations = collect(count, || Ok(axis(vec3(&mut r)?)))?;
    let ground_rotations = collect(count, || rotation(&mut r))?;
    let count = r.count(MAX)?;
    let objects = collect(count, || state(&mut r))?;
    let count = r.count(100_000)?;
    let mut sequences = Vec::new();
    for _ in 0..count {
        let n = String::from_utf8(r.blob32(4096)?)?;
        sequences.push(sequence(&mut r, n)?);
    }
    let count = r.count(MAX)?;
    let triggers = collect(count, || {
        Ok(Trigger {
            state: r.u32()?,
            position: r.f32()?,
        })
    })?;
    r.finish()?;
    let pools = Pools {
        rotations,
        translations,
        uniform,
        aligned,
        arbitrary_rotations,
        arbitrary_factors,
        ground_translations,
        ground_rotations,
        objects,
        triggers,
    };
    let mut animations = Vec::new();
    for s in sequences {
        animations.push(lower_sequence(s, &names, &pools, &mut provenance)?);
    }
    Ok((
        ClipSet {
            schema_version: 1,
            id,
            animations,
        },
        provenance,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_quaternion_conjugation_and_axis_change() {
        let bytes: Vec<_> = [0_i16, 0, 23170, 23170]
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect();
        let q = glam::Quat::from_array(rotation(&mut Reader::new(&bytes)).unwrap());
        assert!((q * Vec3::X).distance(Vec3::Z) < 1e-5);
    }
    #[test]
    fn all_three_guards_must_match() {
        for broken in 0..3 {
            let mut wide = 0_u32.to_le_bytes();
            let mut short = 0_u16.to_le_bytes();
            let mut byte = [0_u8];
            match broken {
                0 => wide[0] = 1,
                1 => short[0] = 1,
                _ => byte[0] = 1,
            };
            let mut t = Tri {
                wide: Reader::new(&wide),
                short: Reader::new(&short),
                byte: Reader::new(&byte),
                guard: 0,
            };
            assert!(t.guard("fixture").is_err());
        }
    }
    fn clip_fixture() -> Vec<u8> {
        fn word(out: &mut Vec<u8>, v: u32) {
            out.extend(v.to_le_bytes());
        }
        fn blob(out: &mut Vec<u8>, s: &str) {
            word(out, s.len() as u32);
            out.extend(s.as_bytes());
        }
        let mut out = Vec::new();
        word(&mut out, 24);
        word(&mut out, 1);
        blob(&mut out, "root");
        for v in [0, 0, 0, 2] {
            word(&mut out, v);
        }
        for v in [1.0_f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
            word(&mut out, v.to_bits());
        }
        for _ in 0..5 {
            word(&mut out, 0);
        }
        word(&mut out, 1);
        blob(&mut out, "walk");
        for v in [16, 2, 1.0_f32.to_bits(), 0] {
            word(&mut out, v);
        }
        for _ in 0..10 {
            word(&mut out, 0);
        }
        for slot in 0..8 {
            word(&mut out, 0);
            word(&mut out, u32::from(slot == 1));
            if slot == 1 {
                word(&mut out, 1);
            }
        }
        word(&mut out, 0);
        out
    }
    #[test]
    fn clip_read_preserves_timing_axes_and_rejects_every_truncation() {
        let bytes = clip_fixture();
        let (clips, _) = read_dsq(&bytes, "test".into()).unwrap();
        let clip = &clips.animations[0];
        assert!(clip.looping);
        assert_eq!(clip.duration, 1.0);
        assert_eq!(
            clip.nodes[0].translations,
            vec![[1.0, 3.0, -2.0], [4.0, 6.0, -5.0]]
        );
        for size in 0..bytes.len() {
            assert!(
                read_dsq(&bytes[..size], "test".into()).is_err(),
                "Accepted truncated clip of {size} bytes"
            );
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert!(read_dsq(&trailing, "test".into()).is_err());
    }
}
