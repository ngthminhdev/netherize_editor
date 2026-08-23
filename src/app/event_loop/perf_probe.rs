//! Env-gated end-to-end performance probe (`NETH_PERF_PROBE=1`).
//!
//! Self-drives the REAL pipeline — dispatch → AppState mutation → layout →
//! glyph shaping → wgpu encode → submit → present — measuring:
//!   * startup: process start → first presented frame
//!   * open-file: AppState::open_file(9.93 MiB log)
//!   * keystroke latency: dispatch(InsertChar) → present, p50/p95/p99
//!   * autocomplete: dispatch(TriggerCompletion) → completion popup visible
//!   * scroll: frame interval + latency over a half-page-down burst
//!   * GPU pass time via wgpu timestamp queries (see render/gpu_timing.rs)
//!   * RSS/CPU checkpoints via sysinfo
//!
//! Results land in benchmarks/baselines/app_probe.json, then the app exits.
//! Zero effect on normal launches (field stays disabled, features stay empty).

use std::{
    fs,
    io::Write as _,
    path::PathBuf,
    sync::OnceLock,
    time::{Duration, Instant},
};

use super::AppShell;
use crate::core::{command_dispatch::dispatch_command, commands::Command};

const TYPING_FRAMES: u32 = 150;
const SCROLL_FRAMES: u32 = 120;
/// rust-analyzer cold start + index of the host project can take a while;
/// give the popup a generous window before declaring it missing.
const COMPLETION_TIMEOUT: Duration = Duration::from_secs(30);
const PROBE_WARMUP: Duration = Duration::from_millis(700);
/// If no successful frame lands for this long (occluded window etc.), emit a
/// partial report instead of hanging.
const STALL_GIVE_UP: Duration = Duration::from_secs(5);

static PROCESS_START: OnceLock<Instant> = OnceLock::new();

/// Record the true process start instant. Called as the first statement of main().
pub fn mark_process_start() {
    let _ = PROCESS_START.set(Instant::now());
}

fn process_start() -> Instant {
    *PROCESS_START.get_or_init(Instant::now)
}

#[derive(Clone, Copy)]
enum Phase {
    /// Let the app finish booting before touching anything.
    Warmup { until: Instant },
    /// Open the large scratch log through AppState.
    OpenLarge,
    /// One InsertChar per frame through the real dispatcher.
    Typing { left: u32 },
    /// Switch to a small rust file so an LSP server can attach.
    OpenRust,
    /// Poll for the completion popup; re-trigger periodically until timeout.
    Completion { deadline: Instant, attempts: u32 },
    /// Half-page-down burst, one per frame.
    Scrolling { left: u32, last_present: Option<Instant> },
    Done,
}

pub(super) struct PerfProbe {
    pub(super) enabled: bool,
    phase: Phase,
    pending_t0: Option<Instant>,
    startup_ms: Option<f64>,
    open_large_ms: Option<f64>,
    typing_ms: Vec<f64>,
    gpu_ms: Vec<f64>,
    scroll_interval_ms: Vec<f64>,
    scroll_latency_ms: Vec<f64>,
    completion_ms: Option<f64>,
    completion_found: bool,
    resources: SysinfoSampler,
    rss_boot_mb: Option<f64>,
    rss_after_load_mb: Option<f64>,
    rss_end_mb: Option<f64>,
    cpu_typing_percent: Option<f32>,
    failed_frames: u32,
    last_progress_at: Instant,
    phase_entered_at: Instant,
    typing_wall_ms: Option<f64>,
    scroll_wall_ms: Option<f64>,
    completion_wall_ms: Option<f64>,
    rss_after_typing_mb: Option<f64>,
    rss_after_scroll_mb: Option<f64>,
    scroll_first_at: Option<Instant>,
    scroll_last_at: Option<Instant>,
}

impl PerfProbe {
    pub(super) fn new() -> Self {
        Self {
            enabled: crate::render::gpu_timing::requested_by_env(),
            phase: Phase::Warmup {
                until: Instant::now() + PROBE_WARMUP,
            },
            pending_t0: None,
            startup_ms: None,
            open_large_ms: None,
            typing_ms: Vec::new(),
            gpu_ms: Vec::new(),
            scroll_interval_ms: Vec::new(),
            scroll_latency_ms: Vec::new(),
            completion_ms: None,
            completion_found: false,
            resources: SysinfoSampler::new(),
            rss_boot_mb: None,
            rss_after_load_mb: None,
            rss_end_mb: None,
            cpu_typing_percent: None,
            failed_frames: 0,
            last_progress_at: Instant::now(),
            phase_entered_at: Instant::now(),
            typing_wall_ms: None,
            scroll_wall_ms: None,
            completion_wall_ms: None,
            rss_after_typing_mb: None,
            rss_after_scroll_mb: None,
            scroll_first_at: None,
            scroll_last_at: None,
        }
    }

    fn active(&self) -> bool {
        self.enabled && !matches!(self.phase, Phase::Done)
    }
}

/// Self-process RSS / CPU sampling (mirrors the e2e runner helper).
struct SysinfoSampler {
    sys: sysinfo::System,
    pid: sysinfo::Pid,
    primed: bool,
}

impl SysinfoSampler {
    fn new() -> Self {
        Self {
            sys: sysinfo::System::new(),
            pid: sysinfo::Pid::from_u32(std::process::id()),
            primed: false,
        }
    }

    fn refresh(&mut self) {
        use sysinfo::ProcessesToUpdate;
        self.sys
            .refresh_processes(ProcessesToUpdate::Some(&[self.pid]), true);
    }

    fn sample(&mut self) -> (f64, f32) {
        if !self.primed {
            self.refresh();
            std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
            self.primed = true;
        }
        self.refresh();
        match self.sys.process(self.pid) {
            Some(process) => (
                process.memory() as f64 / (1024.0 * 1024.0),
                process.cpu_usage(),
            ),
            None => (0.0, 0.0),
        }
    }
}

fn ensure_large_log_file() -> PathBuf {
    const BYTES: usize = 5 * 1024 * 1024 - 64 * 1024;
    let path = std::env::temp_dir().join("netherize_probe_large.log");
    let ok_size = fs::metadata(&path).map(|m| m.len() as usize == BYTES).unwrap_or(false);
    if !ok_size {
        let mut file = fs::File::create(&path).expect("create probe log");
        let line =
            b"2026-05-05T00:00:00Z level=info userId=probe fps=120 rtt=4 loss=0 jitter=1 message=\"frame stable\"\n";
        let mut written = 0usize;
        while written < BYTES {
            let n = (BYTES - written).min(line.len());
            file.write_all(&line[..n]).expect("write probe log");
            written += n;
        }
    }
    path
}

impl AppShell {
    /// Runs at the very top of `redraw()` — dispatches the current phase's
    /// operation so THIS frame reflects it (latency = dispatch → present).
    pub(super) fn perf_probe_pre(&mut self) {
        if !self.perf_probe.active() {
            return;
        }
        match self.perf_probe.phase {
            Phase::Warmup { until } => {
                if Instant::now() >= until {
                    // Bring the window to front so macOS doesn't throttle or
                    // occlude the frames the probe needs to measure.
                    if let Some(window) = &self.window {
                        window.focus_window();
                    }
                    if self.perf_probe.rss_boot_mb.is_none() {
                        self.perf_probe.rss_boot_mb = Some(self.perf_probe.resources.sample().0);
                    }
                    self.perf_probe.phase = Phase::OpenLarge;
                    eprintln!("[perf_probe] warmup done → open large file");
                    self.perf_probe_pre(); // fall through into OpenLarge now
                }
            }
            Phase::OpenLarge => {
                let path = ensure_large_log_file();
                let t0 = Instant::now();
                let opened = self.app_state.open_file(path);
                self.perf_probe.open_large_ms =
                    Some(t0.elapsed().as_secs_f64() * 1_000.0);
                debug_assert!(opened.is_ok(), "probe log must be under the 5 MiB cap");
                self.perf_probe.rss_after_load_mb = Some(self.perf_probe.resources.sample().0);
                // Park mid-file so typing + scroll exercise the big buffer.
                let _ = self.app_state.jump_to_line_and_column(150_000, 40);
                self.perf_probe.phase = Phase::Typing {
                    left: TYPING_FRAMES,
                };
                self.perf_probe.phase_entered_at = Instant::now();
                eprintln!("[perf_probe] large file opened in {:.1}ms → typing burst",
                    self.perf_probe.open_large_ms.unwrap_or(0.0));
            }
            Phase::OpenRust => {
                // A small real rust file from the repo → LSP can attach.
                let candidate = std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join("src/main.rs");
                if candidate.exists() {
                    let _ = self.app_state.open_file(candidate);
                }
                self.perf_probe.phase = Phase::Completion {
                    deadline: Instant::now() + COMPLETION_TIMEOUT,
                    attempts: 0,
                };
                self.perf_probe.phase_entered_at = Instant::now();
                eprintln!("[perf_probe] rust file open → waiting for completion popup");
            }
            Phase::Completion { deadline, attempts } => {
                // Re-arm periodically: first pass enters insert mode (`a`),
                // subsequent passes re-trigger like ctrl+space.
                if attempts == 0 || attempts % 20 == 19 {
                    let _ = dispatch_command(&mut self.app_state, Command::AppendAfterCursor);
                    let _ = dispatch_command(&mut self.app_state, Command::TriggerCompletion);
                    self.perf_probe.pending_t0 = Some(Instant::now());
                }
                self.perf_probe.phase = Phase::Completion {
                    deadline,
                    attempts: attempts + 1,
                };
                if self.app_state.completion().is_some() && !self.perf_probe.completion_found {
                    self.perf_probe.completion_found = true;
                    self.perf_probe.completion_ms = self
                        .perf_probe
                        .pending_t0
                        .map(|t| t.elapsed().as_secs_f64() * 1_000.0);
                }
                if self.perf_probe.completion_found || Instant::now() > deadline {
                    self.perf_probe.completion_wall_ms = Some(
                        Instant::now()
                            .saturating_duration_since(self.perf_probe.phase_entered_at)
                            .as_secs_f64()
                            * 1_000.0,
                    );
                    self.perf_probe.phase = Phase::Done;
                    self.perf_probe_finish();
                }
            }
            Phase::Typing { .. } | Phase::Scrolling { .. } => {
                self.perf_probe_dispatch_next();
            }
            Phase::Done => {}
        }
    }

    /// Called after every `renderer.render()` attempt. Latency/GPU samples are
    /// only recorded on a successful present, but phase progression happens
    /// regardless so an occluded/hidden window can never wedge the probe.
    pub(super) fn perf_probe_post(&mut self, render_ok: bool) {
        if !self.perf_probe.enabled || matches!(self.perf_probe.phase, Phase::Done) {
            return;
        }
        let now = Instant::now();
        if self.perf_probe.startup_ms.is_none() && render_ok {
            self.perf_probe.startup_ms =
                Some(now.saturating_duration_since(process_start()).as_secs_f64() * 1_000.0);
        }
        // Stall guard: if frames keep failing (occluded window etc.), give up
        // and emit whatever was collected instead of hanging forever.
        if !render_ok {
            self.perf_probe.failed_frames += 1;
            if now.saturating_duration_since(self.perf_probe.last_progress_at)
                > STALL_GIVE_UP
            {
                eprintln!("[perf_probe] render stalled; emitting partial report");
                self.perf_probe.rss_end_mb.get_or_insert_with(|| self.perf_probe.resources.sample().0);
                self.perf_probe.phase = Phase::Done;
                self.perf_probe_finish();
                return;
            }
        } else {
            self.perf_probe.last_progress_at = now;
        }
        let gpu_latest = if render_ok {
            self.renderer.as_ref().and_then(|r| r.latest_gpu_pass_ms())
        } else {
            None
        };
        if let Some(gpu) = gpu_latest {
            self.perf_probe.gpu_ms.push(gpu);
        }

        match self.perf_probe.phase {
            Phase::Typing { left } => {
                if render_ok && let Some(t0) = self.perf_probe.pending_t0.take() {
                    self.perf_probe.typing_ms.push(t0.elapsed().as_secs_f64() * 1_000.0);
                } else {
                    self.perf_probe.pending_t0 = None;
                }
                // Fine-grained RSS curve while typing (retention diagnosis).
                if self.perf_probe.typing_ms.len() % 25 == 0 {
                    let (rss, _) = self.perf_probe.resources.sample();
                    eprintln!(
                        "[perf_probe] typing rss_after_{}_inserts={:.0}MB",
                        self.perf_probe.typing_ms.len(),
                        rss
                    );
                }
                if left == 0 {
                    self.perf_probe.typing_wall_ms = Some(
                        now.saturating_duration_since(self.perf_probe.phase_entered_at)
                            .as_secs_f64()
                            * 1_000.0,
                    );
                    self.perf_probe.rss_after_typing_mb =
                        Some(self.perf_probe.resources.sample().0);
                    self.perf_probe.cpu_typing_percent =
                        Some(self.perf_probe.resources.sample().1);
                    self.perf_probe.pending_t0 = None;
                    self.perf_probe.phase_entered_at = now;
                    self.perf_probe.phase = Phase::Scrolling {
                        left: SCROLL_FRAMES,
                        last_present: Some(now),
                    };
                    eprintln!("[perf_probe] typing done → scrolling");
                } else {
                    self.perf_probe.phase = Phase::Typing { left: left - 1 };
                }
            }
            Phase::Scrolling { left, last_present } => {
                if render_ok {
                    if let Some(prev) = last_present {
                        self.perf_probe.scroll_interval_ms.push(
                            now.saturating_duration_since(prev).as_secs_f64() * 1_000.0,
                        );
                    }
                    self.perf_probe
                        .scroll_first_at
                        .get_or_insert(now);
                    self.perf_probe.scroll_last_at = Some(now);
                    if let Some(t0) = self.perf_probe.pending_t0.take() {
                        self.perf_probe
                            .scroll_latency_ms
                            .push(t0.elapsed().as_secs_f64() * 1_000.0);
                    }
                } else {
                    self.perf_probe.pending_t0 = None;
                }
                if left == 0 {
                    self.perf_probe.scroll_wall_ms = Some(
                        now.saturating_duration_since(self.perf_probe.phase_entered_at)
                            .as_secs_f64()
                            * 1_000.0,
                    );
                    self.perf_probe.rss_after_scroll_mb =
                        Some(self.perf_probe.resources.sample().0);
                    self.perf_probe.pending_t0 = None;
                    // Switch to a rust file for the LSP/autocomplete phase.
                    self.perf_probe.phase = Phase::OpenRust;
                    eprintln!("[perf_probe] scrolling done → opening rust file");
                } else {
                    self.perf_probe.phase = Phase::Scrolling {
                        left: left - 1,
                        last_present: last_present.or(Some(now)),
                    };
                }
            }
            _ => {}
        }
    }

    fn perf_probe_dispatch_next(&mut self) {
        match self.perf_probe.phase {
            Phase::Typing { .. } => {
                self.perf_probe.pending_t0 = Some(Instant::now());
                let ch = char::from(b'a' + (self.perf_probe.typing_ms.len() as u8 % 26));
                let _ = dispatch_command(&mut self.app_state, Command::InsertChar(ch));
            }
            Phase::Scrolling { .. } => {
                self.perf_probe.pending_t0 = Some(Instant::now());
                let _ = dispatch_command(&mut self.app_state, Command::ScrollHalfPageDown);
            }
            _ => {}
        }
    }

    fn perf_probe_finish(&mut self) {
        self.perf_probe.rss_end_mb.get_or_insert_with(|| self.perf_probe.resources.sample().0);
        // FPS from the actual time span of ok scroll frames — immune to
        // per-interval instrumentation quirks.
        let scroll_fps = match (
            self.perf_probe.scroll_first_at,
            self.perf_probe.scroll_last_at,
        ) {
            (Some(first), Some(last)) if last > first => {
                let span = last.saturating_duration_since(first).as_secs_f64();
                let frames = self.perf_probe.scroll_latency_ms.len().max(2) as f64;
                Some((frames - 1.0) / span)
            }
            _ => None,
        };
        write_report(&PerfReport {
            timestamp: iso_now(),
            startup_ms: self.perf_probe.startup_ms,
            open_large_ms: self.perf_probe.open_large_ms,
            typing_p50_ms: percentile(&self.perf_probe.typing_ms, 0.50),
            typing_p95_ms: percentile(&self.perf_probe.typing_ms, 0.95),
            typing_p99_ms: percentile(&self.perf_probe.typing_ms, 0.99),
            typing_max_ms: max_of(&self.perf_probe.typing_ms),
            typing_samples: self.perf_probe.typing_ms.len(),
            scroll_fps,
            scroll_interval_p95_ms: percentile(&self.perf_probe.scroll_interval_ms, 0.95),
            scroll_latency_p50_ms: percentile(&self.perf_probe.scroll_latency_ms, 0.50),
            scroll_latency_max_ms: max_of(&self.perf_probe.scroll_latency_ms),
            scroll_samples: self.perf_probe.scroll_interval_ms.len(),
            gpu_pass_p50_ms: percentile(&self.perf_probe.gpu_ms, 0.50),
            gpu_pass_p99_ms: percentile(&self.perf_probe.gpu_ms, 0.99),
            gpu_pass_max_ms: max_of(&self.perf_probe.gpu_ms),
            gpu_samples: self.perf_probe.gpu_ms.len(),
            completion_ms: self.perf_probe.completion_ms,
            completion_found: self.perf_probe.completion_found,
            completion_wall_ms: self.perf_probe.completion_wall_ms,
            typing_wall_ms: self.perf_probe.typing_wall_ms,
            scroll_wall_ms: self.perf_probe.scroll_wall_ms,
            rss_boot_mb: self.perf_probe.rss_boot_mb,
            rss_after_load_mb: self.perf_probe.rss_after_load_mb,
            rss_after_typing_mb: self.perf_probe.rss_after_typing_mb,
            rss_after_scroll_mb: self.perf_probe.rss_after_scroll_mb,
            rss_end_mb: self.perf_probe.rss_end_mb,
            cpu_typing_percent: self.perf_probe.cpu_typing_percent,
        });
        self.exit_requested = true;
    }
}

// Called from pre/post hooks: keep the per-frame dispatch centralized.
impl AppShell {
    pub(super) fn perf_probe_request_continuous_frames(&self) -> bool {
        self.perf_probe.active()
    }
}

struct PerfReport {
    timestamp: String,
    startup_ms: Option<f64>,
    open_large_ms: Option<f64>,
    typing_p50_ms: Option<f64>,
    typing_p95_ms: Option<f64>,
    typing_p99_ms: Option<f64>,
    typing_max_ms: Option<f64>,
    typing_samples: usize,
    scroll_fps: Option<f64>,
    scroll_interval_p95_ms: Option<f64>,
    scroll_latency_p50_ms: Option<f64>,
    scroll_latency_max_ms: Option<f64>,
    scroll_samples: usize,
    gpu_pass_p50_ms: Option<f64>,
    gpu_pass_p99_ms: Option<f64>,
    gpu_pass_max_ms: Option<f64>,
    gpu_samples: usize,
    completion_ms: Option<f64>,
    completion_found: bool,
    completion_wall_ms: Option<f64>,
    typing_wall_ms: Option<f64>,
    scroll_wall_ms: Option<f64>,
    rss_boot_mb: Option<f64>,
    rss_after_load_mb: Option<f64>,
    rss_after_typing_mb: Option<f64>,
    rss_after_scroll_mb: Option<f64>,
    rss_end_mb: Option<f64>,
    cpu_typing_percent: Option<f32>,
}

fn percentile(samples: &[f64], p: f64) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((sorted.len() as f64 * p) as usize).min(sorted.len() - 1);
    Some(sorted[idx])
}

fn max_of(samples: &[f64]) -> Option<f64> {
    samples.iter().cloned().fold(None::<f64>, |acc, v| {
        Some(acc.map_or(v, |m: f64| m.max(v)))
    })
}

fn iso_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

fn round(v: Option<f64>, digits: u32) -> serde_json::Value {
    match v {
        None => serde_json::Value::Null,
        Some(x) => {
            let factor = 10f64.powi(digits as i32);
            serde_json::json!((x * factor).round() / factor)
        }
    }
}

fn write_report(report: &PerfReport) {
    let json = serde_json::json!({
        "timestamp": report.timestamp,
        "target_frame_ms": 8.0,
        "startup_ms": round(report.startup_ms, 1),
        "open_large_ms": round(report.open_large_ms, 2),
        "autocomplete": {
            "first_popup_ms": round(report.completion_ms, 1),
            "found": report.completion_found,
            "waited_ms": round(report.completion_wall_ms, 0),
        },
        "typing": {
            "samples": report.typing_samples,
            "wall_ms": round(report.typing_wall_ms, 0),
            "p50_ms": round(report.typing_p50_ms, 3),
            "p95_ms": round(report.typing_p95_ms, 3),
            "p99_ms": round(report.typing_p99_ms, 3),
            "max_ms": round(report.typing_max_ms, 3),
        },
        "scroll": {
            "samples": report.scroll_samples,
            "wall_ms": round(report.scroll_wall_ms, 0),
            "fps_avg": round(report.scroll_fps, 1),
            "interval_p95_ms": round(report.scroll_interval_p95_ms, 3),
            "latency_p50_ms": round(report.scroll_latency_p50_ms, 3),
            "latency_max_ms": round(report.scroll_latency_max_ms, 3),
        },
        "gpu_pass": {
            "samples": report.gpu_samples,
            "p50_ms": round(report.gpu_pass_p50_ms, 3),
            "p99_ms": round(report.gpu_pass_p99_ms, 3),
            "max_ms": round(report.gpu_pass_max_ms, 3),
        },
        "resources": {
            "rss_boot_mb": round(report.rss_boot_mb, 1),
            "rss_after_load_mb": round(report.rss_after_load_mb, 1),
            "rss_after_typing_mb": round(report.rss_after_typing_mb, 1),
            "rss_after_scroll_mb": round(report.rss_after_scroll_mb, 1),
            "rss_end_mb": round(report.rss_end_mb, 1),
            "cpu_typing_percent": report.cpu_typing_percent.map(|c| (c * 10.0).round() / 10.0),
        },
    });

    let out_dir = PathBuf::from("benchmarks/baselines");
    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("[perf_probe] cannot create output dir: {e}");
        return;
    }
    let out_path = out_dir.join("app_probe.json");
    match fs::write(&out_path, serde_json::to_string_pretty(&json).unwrap_or_default()) {
        Ok(()) => println!("[perf_probe] Report written → {}", out_path.display()),
        Err(e) => eprintln!("[perf_probe] Failed to write report: {e}"),
    }
}
