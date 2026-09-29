//! Shared geometry for static world chunks. Chunks live in a few large vertex
//! and index buffers ("blocks") instead of a buffer pair each, so the world
//! pass binds buffers once per block and draws many chunks with one indirect
//! multi-draw. Blocks never grow or move: a chunk that fits nowhere gets a new
//! block twice the size of the last (capped), and a chunk's ranges return to
//! its block when the chunk is dropped.
use crate::scene::SceneVertex;
use anyhow::{Result, ensure};
use std::{
    ops::Range,
    sync::{Arc, Mutex},
};

/// The first block's capacity in vertices (4.5 MB); later blocks double.
const FIRST_BLOCK_VERTICES: u32 = 1 << 16;
/// No block is made larger than this (72 MB of vertices) unless one chunk
/// needs more.
const MAX_BLOCK_VERTICES: u32 = 1 << 20;
/// Indices per vertex a block reserves: quads are 4 vertices, 6 indices.
const INDICES_PER_VERTEX: u32 = 2;

/// First-fit free list over a block's ranges, kept sorted and coalesced.
#[derive(Debug)]
pub(crate) struct Ranges {
    free: Vec<Range<u32>>,
}
impl Ranges {
    pub(crate) fn new(capacity: u32) -> Self {
        Self {
            free: vec![0..capacity],
        }
    }
    pub(crate) fn allocate(&mut self, size: u32) -> Option<Range<u32>> {
        let i = self.free.iter().position(|r| r.end - r.start >= size)?;
        let start = self.free[i].start;
        self.free[i].start += size;
        if self.free[i].is_empty() {
            self.free.remove(i);
        }
        Some(start..start + size)
    }
    pub(crate) fn release(&mut self, range: Range<u32>) {
        if range.is_empty() {
            return;
        }
        let i = self.free.partition_point(|r| r.start < range.start);
        self.free.insert(i, range);
        // Merge with the next, then the previous neighbour.
        if i + 1 < self.free.len() && self.free[i].end == self.free[i + 1].start {
            self.free[i].end = self.free[i + 1].end;
            self.free.remove(i + 1);
        }
        if i > 0 && self.free[i - 1].end == self.free[i].start {
            self.free[i - 1].end = self.free[i].end;
            self.free.remove(i);
        }
    }
    #[cfg(test)]
    fn free_total(&self) -> u32 {
        self.free.iter().map(|r| r.end - r.start).sum()
    }
}

struct Space {
    vertices: Ranges,
    indices: Ranges,
}

pub(crate) struct Block {
    pub(crate) vertices: wgpu::Buffer,
    pub(crate) indices: wgpu::Buffer,
    space: Mutex<Space>,
}

/// A chunk's place in a block; returns it when dropped.
pub(crate) struct Slot {
    pub(crate) block: Arc<Block>,
    pub(crate) vertices: Range<u32>,
    pub(crate) indices: Range<u32>,
}
impl Drop for Slot {
    fn drop(&mut self) {
        if let Ok(mut space) = self.block.space.lock() {
            space.vertices.release(self.vertices.clone());
            space.indices.release(self.indices.clone());
        }
    }
}

#[derive(Default)]
pub(crate) struct GeometryPool {
    blocks: Mutex<Vec<Arc<Block>>>,
}

impl GeometryPool {
    /// Copy a chunk's geometry into a block. Indices stay chunk-local; draws
    /// add the slot's base vertex and first index.
    pub(crate) fn store(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[SceneVertex],
        indices: &[u32],
    ) -> Result<Slot> {
        ensure!(
            !vertices.is_empty() && !indices.is_empty(),
            "Pooled chunks hold geometry"
        );
        let (vertex_count, index_count) = (
            u32::try_from(vertices.len())?,
            u32::try_from(indices.len())?,
        );
        let mut blocks = self
            .blocks
            .lock()
            .map_err(|_| anyhow::anyhow!("geometry pool poisoned"))?;
        let mut found = None;
        for block in blocks.iter() {
            let mut space = block
                .space
                .lock()
                .map_err(|_| anyhow::anyhow!("geometry block poisoned"))?;
            let Some(v) = space.vertices.allocate(vertex_count) else {
                continue;
            };
            let Some(i) = space.indices.allocate(index_count) else {
                space.vertices.release(v);
                continue;
            };
            found = Some((block.clone(), v, i));
            break;
        }
        let (block, v, i) = match found {
            Some(found) => found,
            None => {
                let last = blocks.last().map_or(FIRST_BLOCK_VERTICES / 2, |b| {
                    (b.vertices.size() / std::mem::size_of::<SceneVertex>() as u64) as u32
                });
                let capacity = (last.saturating_mul(2))
                    .min(MAX_BLOCK_VERTICES)
                    .max(vertex_count)
                    .max(index_count.div_ceil(INDICES_PER_VERTEX));
                let index_capacity = (capacity * INDICES_PER_VERTEX).max(index_count);
                let limit = device.limits().max_buffer_size;
                ensure!(
                    u64::from(capacity) * std::mem::size_of::<SceneVertex>() as u64 <= limit
                        && u64::from(index_capacity) * 4 <= limit,
                    "Chunk buffer exceeds device limits"
                );
                let block = Arc::new(Block {
                    vertices: device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("pooled chunk vertices"),
                        size: u64::from(capacity) * std::mem::size_of::<SceneVertex>() as u64,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }),
                    indices: device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("pooled chunk indices"),
                        size: u64::from(index_capacity) * 4,
                        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }),
                    space: Mutex::new(Space {
                        vertices: Ranges::new(capacity),
                        indices: Ranges::new(index_capacity),
                    }),
                });
                let (v, i) = {
                    let mut space = block.space.lock().expect("new block");
                    (
                        space.vertices.allocate(vertex_count).expect("sized"),
                        space.indices.allocate(index_count).expect("sized"),
                    )
                };
                blocks.push(block.clone());
                (block, v, i)
            }
        };
        queue.write_buffer(
            &block.vertices,
            u64::from(v.start) * std::mem::size_of::<SceneVertex>() as u64,
            bytemuck::cast_slice(vertices),
        );
        queue.write_buffer(
            &block.indices,
            u64::from(i.start) * 4,
            bytemuck::cast_slice(indices),
        );
        Ok(Slot {
            block,
            vertices: v,
            indices: i,
        })
    }
    /// Blocks in use, and their total bytes.
    pub(crate) fn usage(&self) -> (usize, u64) {
        self.blocks.lock().map_or((0, 0), |blocks| {
            (
                blocks.len(),
                blocks
                    .iter()
                    .map(|b| b.vertices.size() + b.indices.size())
                    .sum(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_allocate_first_fit_and_coalesce_on_release() {
        let mut r = Ranges::new(100);
        let a = r.allocate(30).unwrap();
        let b = r.allocate(30).unwrap();
        let c = r.allocate(30).unwrap();
        assert_eq!((a.clone(), b.clone(), c.clone()), (0..30, 30..60, 60..90));
        assert!(r.allocate(20).is_none());
        r.release(b);
        assert_eq!(r.allocate(20).unwrap(), 30..50);
        r.release(a);
        r.release(30..50);
        r.release(c);
        assert_eq!(r.free, vec![0..100]);
        assert_eq!(r.free_total(), 100);
        r.release(0..0);
        assert_eq!(r.free, vec![0..100]);
    }
}
