//! Optional GPU pass timing via wgpu timestamp queries.
//!
//! Only enabled when `NETH_PERF_PROBE=1` so normal launches keep
//! `required_features: Features::empty()` and never fail on adapters that
//! lack timestamp support. Measures the wall-GPU duration of the main render
//! pass (begin → end), independent of vsync wait and CPU encode time.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

const QUERY_COUNT: u32 = 2;
const RESULT_BYTES: u64 = QUERY_COUNT as u64 * 8;

/// True when the perf probe asked for GPU instrumentation.
pub fn requested_by_env() -> bool {
    std::env::var("NETH_PERF_PROBE").as_deref() == Ok("1")
}

pub(super) struct GpuTiming {
    query_set: wgpu::QuerySet,
    resolve_buf: wgpu::Buffer,
    read_buf: wgpu::Buffer,
    period_ns: f32,
    latest_pass_ms_bits: Arc<AtomicU64>,
    inflight: Arc<AtomicBool>,
}

impl GpuTiming {
    pub(super) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("netherize_gpu_timing_queries"),
            ty: wgpu::QueryType::Timestamp,
            count: QUERY_COUNT,
        });
        let resolve_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("netherize_gpu_timing_resolve"),
            size: RESULT_BYTES,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("netherize_gpu_timing_read"),
            size: RESULT_BYTES,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            query_set,
            resolve_buf,
            read_buf,
            period_ns: queue.get_timestamp_period(),
            latest_pass_ms_bits: Arc::new(AtomicU64::new(0)),
            inflight: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(super) fn timestamp_writes(&self) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        Some(wgpu::RenderPassTimestampWrites {
            query_set: &self.query_set,
            beginning_of_pass_write_index: Some(0),
            end_of_pass_write_index: Some(1),
        })
    }

    /// Resolve + copy + async-map the finished pass's timestamps. Skipped while
    /// a previous readback is still in flight (skip frames rather than stall).
    pub(super) fn submit_readback(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.inflight.load(Ordering::Relaxed) {
            return;
        }
        self.inflight.store(true, Ordering::Relaxed);

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("netherize_gpu_timing_readback"),
        });
        encoder.resolve_query_set(&self.query_set, 0..QUERY_COUNT, &self.resolve_buf, 0);
        encoder.copy_buffer_to_buffer(&self.resolve_buf, 0, &self.read_buf, 0, RESULT_BYTES);
        queue.submit([encoder.finish()]);

        let bits = Arc::clone(&self.latest_pass_ms_bits);
        let inflight = Arc::clone(&self.inflight);
        let period_ns = self.period_ns;
        let buf = self.read_buf.clone();
        // map_async consumes the handle, so drive it from a clone while the
        // original stays available for get_mapped_range inside the callback.
        let mapped = buf.clone();
        mapped.map_async(wgpu::MapMode::Read, .., move |result| {
            if result.is_ok() {
                let view = buf.get_mapped_range(..);
                let ticks: &[u64] = bytemuck::cast_slice(&view);
                if ticks.len() >= 2 && ticks[1] >= ticks[0] {
                    let ms = (ticks[1] - ticks[0]) as f64 * period_ns as f64 / 1e6;
                    bits.store(ms.to_bits(), Ordering::Relaxed);
                }
            }
            buf.unmap();
            inflight.store(false, Ordering::Relaxed);
        });
    }

    pub fn latest_pass_ms(&self) -> Option<f64> {
        let bits = self.latest_pass_ms_bits.load(Ordering::Relaxed);
        if bits == 0 {
            None
        } else {
            Some(f64::from_bits(bits))
        }
    }
}
