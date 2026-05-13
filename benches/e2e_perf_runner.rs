/// E2E Performance Runner – đo CPU-side frame preparation time.
///
/// Không cần GPU: đo phần nặng nhất trước lệnh queue.submit() —
/// tức là AppState mutations, layout engine, và glyph instance preparation.
///
/// Output: benchmarks/baselines/bench_latest.json
///
/// Chạy: cargo run --bin e2e_perf_runner
use std::{
    fs::{self, File},
    io::Write as _,
    path::PathBuf,
    sync::OnceLock,
    time::{Duration, Instant},
};

use netherize_editor::{
    app::app_state::AppState,
    workbench::{
        layout_engine::{WorkbenchLayoutConfig, WorkbenchLayoutEngine},
        panel_state::WorkbenchPanelState,
    },
};
use winit::dpi::PhysicalSize;

// ── Scenario constants ────────────────────────────────────────────────────────
const SCENARIO_ITERATIONS: usize = 100;
const LARGE_FILE_BYTES: usize = 50 * 1024 * 1024;
const LARGE_FILE_NAME: &str = "netherize_e2e_bench_50mb.log";
const WINDOW_WIDTH: u32 = 4080;
const WINDOW_HEIGHT: u32 = 2482;

fn bench_scratch_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(name)
}

fn ensure_50mb_log_file() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let path = bench_scratch_path(LARGE_FILE_NAME);
        let current_size = fs::metadata(&path).map(|m| m.len() as usize).ok();
        if current_size == Some(LARGE_FILE_BYTES) {
            return path;
        }
        let mut file = File::create(&path).expect("create 50MB benchmark input");
        let line = b"2026-05-05T00:00:00Z level=info userId=bench fps=120 rtt=4 loss=0 jitter=1 message=\"frame stable\"\n";
        let mut written = 0usize;
        while written < LARGE_FILE_BYTES {
            let n = (LARGE_FILE_BYTES - written).min(line.len());
            file.write_all(&line[..n]).expect("write benchmark input");
            written += n;
        }
        file.sync_all().expect("sync benchmark input");
        path
    })
}

struct FrameSamples {
    samples_ms: Vec<f64>,
}

impl FrameSamples {
    fn new(capacity: usize) -> Self {
        Self {
            samples_ms: Vec::with_capacity(capacity),
        }
    }

    fn record(&mut self, elapsed: Duration) {
        self.samples_ms.push(elapsed.as_secs_f64() * 1_000.0);
    }

    fn avg_ms(&self) -> f64 {
        if self.samples_ms.is_empty() {
            return 0.0;
        }
        self.samples_ms.iter().sum::<f64>() / self.samples_ms.len() as f64
    }

    fn p99_ms(&self) -> f64 {
        if self.samples_ms.is_empty() {
            return 0.0;
        }
        let mut sorted = self.samples_ms.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = ((sorted.len() as f64 * 0.99) as usize).min(sorted.len() - 1);
        sorted[idx]
    }

    fn max_ms(&self) -> f64 {
        self.samples_ms
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max)
    }
}

// ── Scenario 1: Heavy file load ───────────────────────────────────────────────
fn scenario_load_large_file() -> FrameSamples {
    let path = ensure_50mb_log_file().clone();
    let mut samples = FrameSamples::new(5);
    for _ in 0..5 {
        let t0 = Instant::now();
        let mut state = AppState::new(bench_scratch_path("e2e_scratch.txt"));
        let _ = state.open_file(path.clone());
        let _len = state.text_len_bytes();
        let _pos = state.cursor_line_col();
        samples.record(t0.elapsed());
    }
    samples
}

// ── Scenario 2: MoveToLastLine ────────────────────────────────────────────────
fn scenario_jump_to_last_line() -> FrameSamples {
    let path = ensure_50mb_log_file().clone();
    let mut state = AppState::new(bench_scratch_path("e2e_scratch.txt"));
    let _ = state.open_file(path);

    let mut samples = FrameSamples::new(10);
    for _ in 0..10 {
        let t0 = Instant::now();
        state.move_to_last_line();
        let _pos = state.cursor_line_col();
        samples.record(t0.elapsed());
    }
    samples
}

// ── Scenario 3: Insert + Scroll loop (100 iters) ─────────────────────────────
fn scenario_insert_and_scroll() -> FrameSamples {
    let path = ensure_50mb_log_file().clone();
    let mut state = AppState::new(bench_scratch_path("e2e_scratch.txt"));
    let _ = state.open_file(path);
    // Position near middle of file so scrolling is meaningful.
    state.jump_to_line_and_column(250_000, 0);

    let layout_engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
    let panel_state = WorkbenchPanelState::default();
    let window_size = PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT);

    let mut samples = FrameSamples::new(SCENARIO_ITERATIONS);
    for i in 0..SCENARIO_ITERATIONS {
        let t0 = Instant::now();

        // Simulate an InsertChar command.
        state.insert_char(char::from(b'a' + (i % 26) as u8));

        // Simulate ScrollDown by advancing target_scroll_y.
        state.target_scroll_y = (state.target_scroll_y + 3.0).min(500_000.0);
        state.current_scroll_y = state.target_scroll_y;

        // Simulate the layout engine work done every frame.
        let layout = layout_engine.compute(window_size, &panel_state);
        let _ = layout
            .model
            .find(netherize_editor::workbench::region_model::RegionId::Center);

        // Simulate text access (what renderer would do).
        let _text_len = state.text_len_bytes();
        let _rev = state.revision();
        let _pos = state.cursor_line_col();

        samples.record(t0.elapsed());
    }
    samples
}

// ── Scenario 4: Layout engine at 4K (34 regions) ─────────────────────────────
fn scenario_layout_engine_4k() -> FrameSamples {
    let layout_engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
    let panel_state = WorkbenchPanelState::default();
    let window_size = PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT);

    let mut samples = FrameSamples::new(200);
    for _ in 0..200 {
        let t0 = Instant::now();
        let layout = layout_engine.compute(window_size, &panel_state);
        let regions: Vec<_> = layout.model.flatten();
        std::hint::black_box(regions.len());
        samples.record(t0.elapsed());
    }
    samples
}

// ── Report ────────────────────────────────────────────────────────────────────

fn write_json_report(
    load_samples: &FrameSamples,
    jump_samples: &FrameSamples,
    edit_samples: &FrameSamples,
    layout_samples: &FrameSamples,
) {
    let out_dir = PathBuf::from("benchmarks/baselines");
    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("[e2e_perf_runner] cannot create output dir: {e}");
        return;
    }
    let out_path = out_dir.join("bench_latest.json");
    let json = format!(
        r#"{{
  "timestamp": "{ts}",
  "target_fps": 120,
  "target_frame_ms": 8.0,
  "window": {{ "width": {ww}, "height": {wh} }},
  "scenarios": {{
    "load_50mb_file": {{
      "iterations": {li},
      "avg_ms": {la:.3},
      "p99_ms": {lp:.3},
      "max_ms": {lm:.3}
    }},
    "jump_to_last_line": {{
      "iterations": {ji},
      "avg_ms": {ja:.3},
      "p99_ms": {jp:.3},
      "max_ms": {jm:.3}
    }},
    "insert_and_scroll_100iters": {{
      "iterations": {ei},
      "avg_ms": {ea:.3},
      "p99_ms": {ep:.3},
      "max_ms": {em:.3}
    }},
    "layout_engine_4k_200iters": {{
      "iterations": {oli},
      "avg_ms": {oa:.3},
      "p99_ms": {op:.3},
      "max_ms": {om:.3}
    }}
  }}
}}"#,
        ts = chrono_now_iso(),
        ww = WINDOW_WIDTH,
        wh = WINDOW_HEIGHT,
        li = load_samples.samples_ms.len(),
        la = load_samples.avg_ms(),
        lp = load_samples.p99_ms(),
        lm = load_samples.max_ms(),
        ji = jump_samples.samples_ms.len(),
        ja = jump_samples.avg_ms(),
        jp = jump_samples.p99_ms(),
        jm = jump_samples.max_ms(),
        ei = edit_samples.samples_ms.len(),
        ea = edit_samples.avg_ms(),
        ep = edit_samples.p99_ms(),
        em = edit_samples.max_ms(),
        oli = layout_samples.samples_ms.len(),
        oa = layout_samples.avg_ms(),
        op = layout_samples.p99_ms(),
        om = layout_samples.max_ms(),
    );
    match fs::write(&out_path, &json) {
        Ok(()) => println!("[e2e_perf_runner] Report written → {}", out_path.display()),
        Err(e) => eprintln!("[e2e_perf_runner] Failed to write report: {e}"),
    }
}

fn chrono_now_iso() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple ISO-8601 approximation (UTC, seconds precision).
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    // Days → date (Gregorian, rough but sufficient for a log label).
    let epoch_year = 1970u64;
    let mut year = epoch_year;
    let mut rem = days;
    loop {
        let y_days = if is_leap(year) { 366 } else { 365 };
        if rem < y_days {
            break;
        }
        rem -= y_days;
        year += 1;
    }
    let months = [31u64, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1u64;
    for (i, &days_in_month) in months.iter().enumerate() {
        let dim = if i == 1 && is_leap(year) {
            29
        } else {
            days_in_month
        };
        if rem < dim {
            break;
        }
        rem -= dim;
        month += 1;
    }
    let day = rem + 1;
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn print_scenario(label: &str, s: &FrameSamples) {
    let target = 8.0f64;
    let status = if s.avg_ms() <= target { "✓" } else { "✗" };
    println!(
        "  [{status}] {label:<40} avg={:.3}ms  p99={:.3}ms  max={:.3}ms",
        s.avg_ms(),
        s.p99_ms(),
        s.max_ms()
    );
}

fn main() {
    println!("=== Netherize Editor – E2E CPU Frame Benchmark ===");
    println!("    Target: avg_frame_ms <= 8.00ms for 120 FPS");
    println!("    Resolution: {}x{}\n", WINDOW_WIDTH, WINDOW_HEIGHT);

    print!("[1/4] Loading 50 MB file (5 iters)... ");
    let _ = std::io::stdout().flush();
    let load = scenario_load_large_file();
    println!("done");

    print!("[2/4] Jump to last line (10 iters)... ");
    let _ = std::io::stdout().flush();
    let jump = scenario_jump_to_last_line();
    println!("done");

    print!("[3/4] Insert + Scroll loop ({SCENARIO_ITERATIONS} iters)... ");
    let _ = std::io::stdout().flush();
    let edit = scenario_insert_and_scroll();
    println!("done");

    print!("[4/4] Layout engine 4K (200 iters)... ");
    let _ = std::io::stdout().flush();
    let layout = scenario_layout_engine_4k();
    println!("done\n");

    println!("── Results ──────────────────────────────────────────────────────");
    print_scenario("load_50mb_file (avg per open)", &load);
    print_scenario("jump_to_last_line (avg per move)", &jump);
    print_scenario("insert_char + scroll_down (per iter)", &edit);
    print_scenario("layout_engine compute @ 4K (per frame)", &layout);
    println!("─────────────────────────────────────────────────────────────────\n");

    write_json_report(&load, &jump, &edit, &layout);
}
