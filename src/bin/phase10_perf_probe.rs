use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use cosmic_text::Metrics;
use netherize_editor::{
    app::app_state::AppState,
    async_runtime::scheduler::AsyncScheduler,
    syntax::syntax_engine::{LanguageId, SyntaxEngine},
    text::text_system::TextSystem,
};
use serde::{Deserialize, Serialize};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

const DEFAULT_INPUTS_DIR: &str = "benchmarks/inputs";
const FILE_10K: &str = "rust_10k_lines.rs";
const FILE_50MB: &str = "log_50mb.txt";
const DEFAULT_GPU_SAMPLES: usize = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerfSnapshot {
    generated_at_unix_s: u64,
    workspace_root: String,
    metrics: PerfMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerfMetrics {
    startup_ms: f64,
    open_10k_ms: f64,
    open_50mb_ms: f64,
    edit_loop_avg_us: f64,
    edit_loop_p95_us: f64,
    gpu_frame_avg_ms: f64,
    gpu_frame_p95_ms: f64,
    memory_rss_current_mb: f64,
    memory_rss_peak_mb: f64,
}

#[derive(Debug, Clone)]
struct PerfOptions {
    inputs_dir: PathBuf,
    json_out: Option<PathBuf>,
    baseline: Option<PathBuf>,
    gpu_samples: usize,
}

impl Default for PerfOptions {
    fn default() -> Self {
        Self {
            inputs_dir: PathBuf::from(DEFAULT_INPUTS_DIR),
            json_out: None,
            baseline: None,
            gpu_samples: DEFAULT_GPU_SAMPLES,
        }
    }
}

#[derive(Debug)]
struct MemoryTracker {
    system: System,
    pid: Option<sysinfo::Pid>,
    peak_rss_bytes: u64,
}

impl MemoryTracker {
    fn new() -> Self {
        let mut system = System::new();
        let pid = sysinfo::get_current_pid().ok();
        let mut tracker = Self {
            system: std::mem::take(&mut system),
            pid,
            peak_rss_bytes: 0,
        };
        tracker.capture();
        tracker
    }

    fn capture(&mut self) -> u64 {
        let Some(pid) = self.pid else {
            return 0;
        };
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing().with_memory(),
        );
        let current = self
            .system
            .process(pid)
            .map(|process| process.memory())
            .unwrap_or(0);
        self.peak_rss_bytes = self.peak_rss_bytes.max(current);
        current
    }

    fn current_mb(&mut self) -> f64 {
        bytes_to_mb(self.capture())
    }

    fn peak_mb(&self) -> f64 {
        bytes_to_mb(self.peak_rss_bytes)
    }
}

fn main() {
    let options = match parse_options() {
        Ok(options) => options,
        Err(err) => {
            eprintln!("phase10_perf_probe argument error: {err}");
            print_usage();
            std::process::exit(2);
        }
    };

    if let Err(err) = run(options) {
        eprintln!("phase10_perf_probe failed: {err}");
        std::process::exit(1);
    }
}

fn run(options: PerfOptions) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|err| format!("read cwd failed: {err}"))?;
    let file_10k = options.inputs_dir.join(FILE_10K);
    let file_50mb = options.inputs_dir.join(FILE_50MB);

    require_file(&file_10k)?;
    require_file(&file_50mb)?;

    println!("=== Phase 10 Perf Probe ===");
    println!("inputs_dir={}", options.inputs_dir.display());
    println!("sample_10k={}", file_10k.display());
    println!("sample_50mb={}", file_50mb.display());

    let mut memory = MemoryTracker::new();

    let startup_ms = measure_startup_ms()?;
    memory.capture();

    let open_10k_ms = measure_open_file_ms(&file_10k)?;
    memory.capture();
    let open_50mb_ms = measure_open_file_ms(&file_50mb)?;
    memory.capture();

    let (edit_avg_us, edit_p95_us) = measure_edit_loop_latency()?;
    memory.capture();

    let (gpu_avg_ms, gpu_p95_ms) = match measure_gpu_frame_time(options.gpu_samples) {
        Ok(metrics) => metrics,
        Err(err) => {
            println!("gpu_frame_probe_skipped={err}");
            (0.0, 0.0)
        }
    };
    memory.capture();

    let snapshot = PerfSnapshot {
        generated_at_unix_s: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| format!("clock drift: {err}"))?
            .as_secs(),
        workspace_root: cwd.display().to_string(),
        metrics: PerfMetrics {
            startup_ms,
            open_10k_ms,
            open_50mb_ms,
            edit_loop_avg_us: edit_avg_us,
            edit_loop_p95_us: edit_p95_us,
            gpu_frame_avg_ms: gpu_avg_ms,
            gpu_frame_p95_ms: gpu_p95_ms,
            memory_rss_current_mb: memory.current_mb(),
            memory_rss_peak_mb: memory.peak_mb(),
        },
    };

    print_snapshot(&snapshot);

    if let Some(baseline_path) = &options.baseline {
        let baseline = load_snapshot(baseline_path)?;
        print_comparison(&baseline, &snapshot);
    }

    if let Some(json_out) = &options.json_out {
        save_snapshot(json_out, &snapshot)?;
        println!("saved_json={}", json_out.display());
    }

    Ok(())
}

fn measure_startup_ms() -> Result<f64, String> {
    let start = Instant::now();
    let _scheduler =
        AsyncScheduler::new().map_err(|err| format!("scheduler init failed: {err}"))?;
    let mut text_system = TextSystem::new(Metrics::new(16.0, 22.0), Some(800.0), Some(600.0));
    text_system.set_text("fn main() { println!(\"startup\"); }");
    let _syntax_engine =
        SyntaxEngine::new(LanguageId::Rust).map_err(|err| format!("syntax init failed: {err}"))?;
    Ok(elapsed_ms(start))
}

fn measure_open_file_ms(path: &Path) -> Result<f64, String> {
    let mut app_state = AppState::new(PathBuf::from("phase10_perf_scratch.txt"));
    let start = Instant::now();
    app_state
        .open_file(path.to_path_buf())
        .map_err(|err| format!("open {} failed: {err}", path.display()))?;
    Ok(elapsed_ms(start))
}

fn measure_edit_loop_latency() -> Result<(f64, f64), String> {
    let mut app_state =
        AppState::from_text(PathBuf::from("phase10_perf_edit.txt"), "fn main() {}\n");
    let mut samples_us = Vec::with_capacity(10_000);

    for idx in 0..10_000 {
        let start = Instant::now();
        app_state.insert_char('x');
        app_state.move_left();
        app_state.move_right();
        if idx % 5 == 0 {
            app_state.backspace();
        }
        samples_us.push(elapsed_us(start));
    }

    let avg = samples_us.iter().sum::<f64>() / samples_us.len() as f64;
    let p95 = percentile(samples_us, 0.95)?;
    Ok((avg, p95))
}

fn measure_gpu_frame_time(samples: usize) -> Result<(f64, f64), String> {
    if samples == 0 {
        return Ok((0.0, 0.0));
    }

    pollster::block_on(async move {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|err| format!("gpu adapter request failed: {err}"))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("netherize-perf-probe-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: Default::default(),
                memory_hints: Default::default(),
                trace: Default::default(),
            })
            .await
            .map_err(|err| format!("gpu device request failed: {err}"))?;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("netherize-perf-probe-target"),
            size: wgpu::Extent3d {
                width: 1280,
                height: 720,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut frame_samples = Vec::with_capacity(samples);

        for _ in 0..samples {
            let start = Instant::now();
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("netherize-perf-probe-encoder"),
            });
            {
                let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("netherize-perf-probe-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.05,
                                g: 0.06,
                                b: 0.08,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            }

            let submission_index = queue.submit(std::iter::once(encoder.finish()));
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(submission_index),
                    timeout: Some(Duration::from_millis(250)),
                })
                .map_err(|err| format!("gpu poll failed: {err:?}"))?;
            frame_samples.push(elapsed_ms(start));
        }

        let avg = frame_samples.iter().sum::<f64>() / frame_samples.len() as f64;
        let p95 = percentile(frame_samples, 0.95)?;
        Ok::<(f64, f64), String>((avg, p95))
    })
}

fn percentile(mut values: Vec<f64>, pct: f64) -> Result<f64, String> {
    if values.is_empty() {
        return Err("percentile on empty vector".to_string());
    }
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let index = ((values.len() as f64 - 1.0) * pct).round() as usize;
    Ok(values[index.min(values.len() - 1)])
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn elapsed_us(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000_000.0
}

fn bytes_to_mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn require_file(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    Err(format!(
        "missing sample file {} (run ./scripts/generate_bench_samples.sh)",
        path.display()
    ))
}

fn save_snapshot(path: &Path, snapshot: &PerfSnapshot) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("create baseline dir {} failed: {err}", parent.display()))?;
    }
    let body = serde_json::to_string_pretty(snapshot)
        .map_err(|err| format!("encode snapshot json failed: {err}"))?;
    fs::write(path, body).map_err(|err| format!("write baseline {} failed: {err}", path.display()))
}

fn load_snapshot(path: &Path) -> Result<PerfSnapshot, String> {
    let body = fs::read_to_string(path)
        .map_err(|err| format!("read baseline {} failed: {err}", path.display()))?;
    serde_json::from_str(&body)
        .map_err(|err| format!("decode baseline {} failed: {err}", path.display()))
}

fn print_snapshot(snapshot: &PerfSnapshot) {
    let metrics = &snapshot.metrics;
    println!("=== Metrics ===");
    println!("startup_ms={:.3}", metrics.startup_ms);
    println!("open_10k_ms={:.3}", metrics.open_10k_ms);
    println!("open_50mb_ms={:.3}", metrics.open_50mb_ms);
    println!("edit_loop_avg_us={:.3}", metrics.edit_loop_avg_us);
    println!("edit_loop_p95_us={:.3}", metrics.edit_loop_p95_us);
    println!("gpu_frame_avg_ms={:.3}", metrics.gpu_frame_avg_ms);
    println!("gpu_frame_p95_ms={:.3}", metrics.gpu_frame_p95_ms);
    println!("memory_rss_current_mb={:.3}", metrics.memory_rss_current_mb);
    println!("memory_rss_peak_mb={:.3}", metrics.memory_rss_peak_mb);
}

fn print_comparison(baseline: &PerfSnapshot, current: &PerfSnapshot) {
    println!("=== Compare (Baseline -> Current) ===");
    print_delta(
        "startup_ms",
        baseline.metrics.startup_ms,
        current.metrics.startup_ms,
    );
    print_delta(
        "open_10k_ms",
        baseline.metrics.open_10k_ms,
        current.metrics.open_10k_ms,
    );
    print_delta(
        "open_50mb_ms",
        baseline.metrics.open_50mb_ms,
        current.metrics.open_50mb_ms,
    );
    print_delta(
        "edit_loop_avg_us",
        baseline.metrics.edit_loop_avg_us,
        current.metrics.edit_loop_avg_us,
    );
    print_delta(
        "edit_loop_p95_us",
        baseline.metrics.edit_loop_p95_us,
        current.metrics.edit_loop_p95_us,
    );
    print_delta(
        "gpu_frame_avg_ms",
        baseline.metrics.gpu_frame_avg_ms,
        current.metrics.gpu_frame_avg_ms,
    );
    print_delta(
        "gpu_frame_p95_ms",
        baseline.metrics.gpu_frame_p95_ms,
        current.metrics.gpu_frame_p95_ms,
    );
    print_delta(
        "memory_rss_peak_mb",
        baseline.metrics.memory_rss_peak_mb,
        current.metrics.memory_rss_peak_mb,
    );
}

fn print_delta(label: &str, baseline: f64, current: f64) {
    let delta = current - baseline;
    let pct = if baseline.abs() < f64::EPSILON {
        0.0
    } else {
        (delta / baseline) * 100.0
    };
    println!(
        "{label}: baseline={:.3} current={:.3} delta={:+.3} ({:+.2}%)",
        baseline, current, delta, pct
    );
}

fn parse_options() -> Result<PerfOptions, String> {
    let mut options = PerfOptions::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--inputs-dir" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--inputs-dir requires a value".to_string())?;
                options.inputs_dir = PathBuf::from(value);
            }
            "--json-out" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--json-out requires a value".to_string())?;
                options.json_out = Some(PathBuf::from(value));
            }
            "--baseline" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--baseline requires a value".to_string())?;
                options.baseline = Some(PathBuf::from(value));
            }
            "--gpu-samples" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--gpu-samples requires a value".to_string())?;
                options.gpu_samples = value
                    .parse::<usize>()
                    .map_err(|err| format!("invalid --gpu-samples {:?}: {err}", value))?;
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            unknown => return Err(format!("unknown argument {unknown:?}")),
        }
    }
    Ok(options)
}

fn print_usage() {
    println!("Usage:");
    println!(
        "  cargo run --bin phase10_perf_probe -- [--inputs-dir benchmarks/inputs] [--json-out <path>] [--baseline <path>] [--gpu-samples 60]"
    );
}
