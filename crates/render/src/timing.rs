//! GPU time per stretch of a frame's command encoder, from timestamps
//! written by empty compute passes (which need only `TIMESTAMP_QUERY`, not
//! timestamps inside passes). `begin` stamps first, each `mark` ends a named
//! stretch, `end` resolves; readbacks are asynchronous, a few frames late.
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

/// Stamps per frame: the first, then one per named stretch.
pub const MAX_STAMPS: u32 = 16;
const FREE: u8 = 0;
const COPIED: u8 = 1;
const MAPPING: u8 = 2;
const READY: u8 = 3;
const SLOTS: usize = 4;
const BYTES: u64 = MAX_STAMPS as u64 * 8;

struct Slot {
    buffer: wgpu::Buffer,
    state: Arc<AtomicU8>,
    /// The stretches this slot's stamps end, in order.
    labels: Vec<&'static str>,
}

pub struct GpuTimer {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    slots: Vec<Slot>,
    period_ns: f32,
    /// The slot this frame writes, and the stretches marked so far.
    writing: Option<(usize, Vec<&'static str>)>,
    latest: Option<(Duration, Vec<(&'static str, Duration)>)>,
    /// Frames read back so far: a caller that sees it change knows `latest`
    /// is a new frame, not the one it already counted.
    readings: u64,
}

impl GpuTimer {
    /// None when the device was created without `TIMESTAMP_QUERY`.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("gpu timer"),
            ty: wgpu::QueryType::Timestamp,
            count: MAX_STAMPS,
        });
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu timer resolve"),
            size: BYTES,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let slots = (0..SLOTS)
            .map(|_| Slot {
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("gpu timer readback"),
                    size: BYTES,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }),
                state: Arc::new(AtomicU8::new(FREE)),
                labels: Vec::new(),
            })
            .collect();
        Some(Self {
            queries,
            resolve,
            slots,
            period_ns: queue.get_timestamp_period(),
            writing: None,
            latest: None,
            readings: 0,
        })
    }
    fn stamp(&self, encoder: &mut wgpu::CommandEncoder, index: u32) {
        let writes = wgpu::ComputePassTimestampWrites {
            query_set: &self.queries,
            beginning_of_pass_write_index: Some(index),
            end_of_pass_write_index: None,
        };
        drop(encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("gpu timer"),
            timestamp_writes: Some(writes),
        }));
    }
    /// The first stamp. Skips the frame when every readback is still in
    /// flight.
    pub fn begin(&mut self, encoder: &mut wgpu::CommandEncoder) {
        self.writing = self
            .slots
            .iter()
            .position(|s| s.state.load(Ordering::Acquire) == FREE)
            .map(|slot| (slot, Vec::new()));
        if self.writing.is_some() {
            self.stamp(encoder, 0);
        }
    }
    /// Ends the stretch `label` here. Marks past the stamp budget, and
    /// outside `begin`..`end`, are ignored.
    pub fn mark(&mut self, encoder: &mut wgpu::CommandEncoder, label: &'static str) {
        let Some((_, labels)) = &mut self.writing else {
            return;
        };
        if labels.len() + 1 >= MAX_STAMPS as usize {
            return;
        }
        labels.push(label);
        let index = labels.len() as u32;
        self.stamp(encoder, index);
    }
    /// The last stamp (ending `label`), before the encoder is submitted.
    pub fn end(&mut self, encoder: &mut wgpu::CommandEncoder, label: &'static str) {
        self.mark(encoder, label);
        let Some((slot, labels)) = self.writing.take() else {
            return;
        };
        let count = labels.len() as u32 + 1;
        let slot = &mut self.slots[slot];
        encoder.resolve_query_set(&self.queries, 0..count, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &slot.buffer, 0, u64::from(count) * 8);
        slot.labels = labels;
        slot.state.store(COPIED, Ordering::Release);
    }
    /// How many frames have been read back (see `collect`).
    pub fn readings(&self) -> u64 {
        self.readings
    }
    /// After submitting: start reading submitted slots and take any that
    /// finished. The latest frame known: its whole time and each stretch.
    pub fn collect(
        &mut self,
        device: &wgpu::Device,
    ) -> Option<&(Duration, Vec<(&'static str, Duration)>)> {
        for slot in &self.slots {
            if slot.state.load(Ordering::Acquire) == COPIED {
                slot.state.store(MAPPING, Ordering::Release);
                let state = slot.state.clone();
                slot.buffer
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        state.store(if result.is_ok() { READY } else { FREE }, Ordering::Release);
                    });
            }
        }
        let _ = device.poll(wgpu::PollType::Poll);
        for slot in &self.slots {
            if slot.state.load(Ordering::Acquire) != READY {
                continue;
            }
            if let Ok(view) = slot.buffer.slice(..).get_mapped_range() {
                let ticks: Vec<u64> = view
                    .chunks_exact(8)
                    .take(slot.labels.len() + 1)
                    .map(|c| u64::from_le_bytes(c.try_into().unwrap_or_default()))
                    .collect();
                drop(view);
                let span = |a: u64, b: u64| {
                    Duration::from_nanos(
                        (b.saturating_sub(a) as f64 * f64::from(self.period_ns)) as u64,
                    )
                };
                let stretches = slot
                    .labels
                    .iter()
                    .zip(ticks.windows(2))
                    .map(|(label, w)| (*label, span(w[0], w[1])))
                    .collect();
                let whole = span(ticks[0], ticks.last().copied().unwrap_or(ticks[0]));
                self.latest = Some((whole, stretches));
                self.readings += 1;
            }
            slot.buffer.unmap();
            slot.state.store(FREE, Ordering::Release);
        }
        self.latest.as_ref()
    }
}
