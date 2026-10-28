// src-native/src/gpu/frame_timer.rs
//
// Per-pass GPU timings, as frameTimer.ts takes them. Each pass writes a
// timestamp at its start and another at its end; after the last one they are
// resolved into a buffer and read back.

use std::sync::{Arc, Mutex};

use super::Gpu;

const MAX_PASSES: u32 = 12;
const BYTES: u64 = MAX_PASSES as u64 * 2 * 8;

#[derive(Clone, Debug)]
pub struct PassTiming {
    pub label: String,
    pub ms: f64,
}

struct Timed {
    query_set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    period_ns: f64,
}

pub struct FrameTimer {
    timed: Option<Timed>,
    labels: Vec<String>,
    resolved: Vec<String>,
    /// Copying into a mapped buffer is invalid, so a frame recorded while the
    /// previous read is outstanding goes untimed.
    map_pending: Arc<Mutex<bool>>,
}

impl FrameTimer {
    pub fn new(gpu: &Gpu) -> Self {
        let timed = gpu.can_timestamp.then(|| Timed {
            query_set: gpu.device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("Frame Timer Query Set"),
                ty: wgpu::QueryType::Timestamp,
                count: MAX_PASSES * 2,
            }),
            resolve: gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Frame Timer Resolve Buffer"),
                size: BYTES,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            read: gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Frame Timer Read Buffer"),
                size: BYTES,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            period_ns: gpu.queue.get_timestamp_period() as f64,
        });
        Self { timed, labels: Vec::new(), resolved: Vec::new(), map_pending: Arc::new(Mutex::new(false)) }
    }

    pub fn available(&self) -> bool {
        self.timed.is_some()
    }

    pub fn begin_frame(&mut self) {
        self.labels.clear();
    }

    /// Claim a slot pair for one pass, or None when this frame goes untimed.
    pub fn claim(&mut self, label: &str) -> Option<u32> {
        self.timed.as_ref()?;
        if *self.map_pending.lock().unwrap() || self.labels.len() as u32 >= MAX_PASSES {
            return None;
        }
        self.labels.push(label.to_string());
        Some(self.labels.len() as u32 - 1)
    }

    pub fn render_writes(&self, slot: Option<u32>) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let (timed, slot) = (self.timed.as_ref()?, slot?);
        Some(wgpu::RenderPassTimestampWrites {
            query_set: &timed.query_set,
            beginning_of_pass_write_index: Some(slot * 2),
            end_of_pass_write_index: Some(slot * 2 + 1),
        })
    }

    pub fn compute_writes(&self, slot: Option<u32>) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
        let (timed, slot) = (self.timed.as_ref()?, slot?);
        Some(wgpu::ComputePassTimestampWrites {
            query_set: &timed.query_set,
            beginning_of_pass_write_index: Some(slot * 2),
            end_of_pass_write_index: Some(slot * 2 + 1),
        })
    }

    /// Record the resolve and the copy out. Call after the frame's last pass.
    pub fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder) {
        self.resolved.clear();
        let Some(timed) = &self.timed else { return };
        if *self.map_pending.lock().unwrap() || self.labels.is_empty() {
            return;
        }
        let count = self.labels.len() as u32 * 2;
        encoder.resolve_query_set(&timed.query_set, 0..count, &timed.resolve, 0);
        encoder.copy_buffer_to_buffer(&timed.resolve, 0, &timed.read, 0, count as u64 * 8);
        self.resolved = self.labels.clone();
    }

    /// Read back what the frame measured, once the GPU is done with it.
    pub fn read(&mut self, done: impl FnOnce(Vec<PassTiming>) + Send + 'static) {
        let Some(timed) = &self.timed else { return done(Vec::new()) };
        if self.resolved.is_empty() || *self.map_pending.lock().unwrap() {
            return done(Vec::new());
        }
        let labels = std::mem::take(&mut self.resolved);
        let pending = self.map_pending.clone();
        *pending.lock().unwrap() = true;
        let read = timed.read.clone();
        let period = timed.period_ns;
        let len = labels.len() as u64 * 16;
        timed.read.slice(..len).map_async(wgpu::MapMode::Read, move |result| {
            let mut out = Vec::new();
            if result.is_ok() {
                {
                    let data = read.slice(..len).get_mapped_range();
                    let times: &[u64] = bytemuck::cast_slice(&data);
                    for (i, label) in labels.into_iter().enumerate() {
                        let (start, end) = (times[i * 2], times[i * 2 + 1]);
                        let ns = if end > start { (end - start) as f64 * period } else { 0.0 };
                        out.push(PassTiming { label, ms: ns / 1e6 });
                    }
                }
                read.unmap();
            }
            *pending.lock().unwrap() = false;
            done(out);
        });
    }
}
