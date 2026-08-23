/// E2E Performance Runner – đo CPU-side frame preparation + các thao tác chính.
///
/// Không cần GPU: đo phần nặng nhất trước lệnh queue.submit() —
/// tức là AppState mutations, layout engine, và glyph instance preparation.
/// (GPU pass time được đo riêng trong app qua NETH_PERF_PROBE=1.)
///
/// Output: benchmarks/baselines/bench_latest.json
///
/// Chạy: cargo bench --bench e2e_perf_runner
use std::{
    fs::{self, File},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant},
};

use netherize_editor::{
    app::app_state::AppState,
    core::{command_dispatch::dispatch_command, commands::Command},
    syntax::syntax_engine::{LanguageId, SyntaxEngine},
    workspace::{
        fuzzy::find_file_matches,
        model::WorkspaceModel,
        scanner::{WorkspaceScanOptions, WorkspaceScanner},
    },
};
use winit::dpi::PhysicalSize;

// ── Scenario constants ────────────────────────────────────────────────────────
const SCENARIO_ITERATIONS: usize = 100;
// Editor hard-refuses interactive files > 10 MiB (INTERACTIVE_TEXT_FILE_LIMIT_BYTES),
// so this bench uses the largest openable size just under the cap.
const LARGE_FILE_BYTES: usize = 10 * 1024 * 1024 - 64 * 1024;
const LARGE_FILE_NAME: &str = "netherize_e2e_bench_10mb.log";
const WINDOW_WIDTH: u32 = 4080;
const WINDOW_HEIGHT: u32 = 2482;
const UNDO_BURST_CHARS: usize = 200;

fn bench_scratch_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(name)
}

fn ensure_large_log_file() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let path = bench_scratch_path(LARGE_FILE_NAME);
        let current_size = fs::metadata(&path).map(|m| m.len() as usize).ok();
        if current_size == Some(LARGE_FILE_BYTES) {
            return path;
        }
        let mut file = File::create(&path).expect("create large benchmark input");
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

fn open_large_file_state(scratch_name: &str) -> (AppState, Instant) {
    let t0 = Instant::now();
    let mut state = AppState::new(bench_scratch_path(scratch_name));
    // Loud failure: the old runner swallowed the >10MiB refusal here and
    // reported fake sub-millisecond numbers for a no-op open.
    state
        .open_file(ensure_large_log_file().clone())
        .expect("open large benchmark file (must be under 10 MiB cap)");
    let _ = state.text_len_bytes();
    let _ = state.cursor_line_col();
    (state, t0)
}

#[derive(Clone)]
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

    fn percentile(&self, p: f64) -> f64 {
        if self.samples_ms.is_empty() {
            return 0.0;
        }
        let mut sorted = self.samples_ms.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = ((sorted.len() as f64 * p) as usize).min(sorted.len() - 1);
        sorted[idx]
    }

    fn avg_ms(&self) -> f64 {
        if self.samples_ms.is_empty() {
            return 0.0;
        }
        self.samples_ms.iter().sum::<f64>() / self.samples_ms.len() as f64
    }

    fn max_ms(&self) -> f64 {
        self.samples_ms.iter().cloned().fold(0.0f64, f64::max)
    }
}

#[derive(Default)]
struct ResourceSample {
    rss_mb: f64,
    cpu_percent: f32,
}

fn sample_resources(cache: &mut Option<SysinfoUtils>) -> ResourceSample {
    let utils = cache.get_or_insert_with(SysinfoUtils::new);
    utils.sample()
}

/// Self-process RSS / CPU sampling via the `sysinfo` dependency.
struct SysinfoUtils {
    sys: sysinfo::System,
    pid: sysinfo::Pid,
    primed: bool,
}

impl SysinfoUtils {
    fn new() -> Self {
        let pid = sysinfo::Pid::from_u32(std::process::id());
        Self {
            sys: sysinfo::System::new(),
            pid,
            primed: false,
        }
    }

    fn refresh(&mut self) {
        use sysinfo::ProcessesToUpdate;
        self.sys
            .refresh_processes(ProcessesToUpdate::Some(&[self.pid]), true);
    }

    /// CPU% needs two refreshes spaced by the update interval; first call
    /// primes, later calls return usage accumulated since previous call.
    fn sample(&mut self) -> ResourceSample {
        if !self.primed {
            self.refresh();
            std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
            self.primed = true;
        }
        self.refresh();
        let empty = ResourceSample::default();
        let Some(process) = self.sys.process(self.pid) else {
            return empty;
        };
        ResourceSample {
            rss_mb: process.memory() as f64 / (1024.0 * 1024.0),
            cpu_percent: process.cpu_usage(),
        }
    }
}

// ── Scenario 1: Heavy file load ───────────────────────────────────────────────
fn scenario_load_large_file() -> FrameSamples {
    let mut samples = FrameSamples::new(5);
    for _ in 0..5 {
        let (_, t0) = open_large_file_state("e2e_scratch.txt");
        samples.record(t0.elapsed());
    }
    samples
}

// ── Scenario 2: MoveToLastLine ────────────────────────────────────────────────
fn scenario_jump_to_last_line() -> FrameSamples {
    let (mut state, _) = open_large_file_state("e2e_scratch.txt");

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
fn scenario_insert_and_scroll(resources: &mut ResourceSample) -> FrameSamples {
    let (mut state, _) = open_large_file_state("e2e_scratch.txt");
    // Position near middle of file so scrolling is meaningful.
    state.jump_to_line_and_column(150_000, 0);

    let layout_engine = netherize_editor::workbench::layout_engine::WorkbenchLayoutEngine::new(
        netherize_editor::workbench::layout_engine::WorkbenchLayoutConfig::default(),
    );
    let panel_state = netherize_editor::workbench::panel_state::WorkbenchPanelState::default();
    let window_size = PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT);

    let pre_cpu = sample_resources(&mut None);
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
    *resources = sample_resources(&mut None);
    resources.cpu_percent += pre_cpu.cpu_percent; // informational only
    samples
}

// ── Scenario 4: Layout engine at 4K (34 regions) ─────────────────────────────
fn scenario_layout_engine_4k() -> FrameSamples {
    let layout_engine = netherize_editor::workbench::layout_engine::WorkbenchLayoutEngine::new(
        netherize_editor::workbench::layout_engine::WorkbenchLayoutConfig::default(),
    );
    let panel_state = netherize_editor::workbench::panel_state::WorkbenchPanelState::default();
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

// ── Scenario 5: Undo/redo burst on the large buffer ──────────────────────────
fn scenario_undo_redo(resources: &mut ResourceSample) -> (FrameSamples, FrameSamples) {
    let (mut state, _) = open_large_file_state("e2e_scratch.txt");
    state.jump_to_line_and_column(150_000, 0);
    let baseline_len = state.text_len_bytes();

    // Type a realistic burst first (what the user would undo).
    for i in 0..UNDO_BURST_CHARS {
        state.insert_char(char::from(b'a' + (i % 26) as u8));
    }

    let mut undo_samples = FrameSamples::new(16);
    let t_all = Instant::now();
    while state.text_len_bytes() > baseline_len {
        let t0 = Instant::now();
        let undone = state.undo();
        undo_samples.record(t0.elapsed());
        if !undone {
            break;
        }
    }
    let undo_total_ms = t_all.elapsed().as_secs_f64() * 1_000.0;

    let mut redo_samples = FrameSamples::new(16);
    let t_all = Instant::now();
    while state.text_len_bytes() < baseline_len + UNDO_BURST_CHARS as usize {
        let t0 = Instant::now();
        let redone = state.redo();
        redo_samples.record(t0.elapsed());
        if !redone {
            break;
        }
    }
    let redo_total_ms = t_all.elapsed().as_secs_f64() * 1_000.0;

    *resources = sample_resources(&mut None);
    println!(
        "  undo burst ({} chars): total {:.1}ms | redo burst: total {:.1}ms",
        UNDO_BURST_CHARS, undo_total_ms, redo_total_ms
    );
    (undo_samples, redo_samples)
}

// ── Scenario 6: In-buffer search via real command path ───────────────────────
fn scenario_search_buffer() -> (FrameSamples, usize) {
    let (mut state, _) = open_large_file_state("e2e_scratch.txt");
    // Park cursor inside an identifier-like word ("level") mid-file.
    state.jump_to_line_and_column(150_000, 40);

    let mut samples = FrameSamples::new(8);
    // First invocation computes highlights across the whole buffer.
    let t0 = Instant::now();
    let _ = dispatch_command(&mut state, Command::SearchWordUnderCursor);
    samples.record(t0.elapsed());
    let matches = state.search_highlights().len();

    // Subsequent SearchNext hops between existing highlights.
    for _ in 0..50 {
        let t0 = Instant::now();
        let _ = dispatch_command(&mut state, Command::SearchNext);
        samples.record(t0.elapsed());
    }
    (samples, matches)
}

// ── Scenario 7: Save the large buffer ────────────────────────────────────────
fn scenario_save_file(resources: &mut ResourceSample) -> (FrameSamples, f64) {
    let (mut state, _) = open_large_file_state("e2e_save_scratch.txt");
    state.jump_to_line_and_column(150_000, 0);

    let mut samples = FrameSamples::new(3);
    let bytes = state.text_len_bytes() as f64;
    for _ in 0..3 {
        let t0 = Instant::now();
        let saved = state.save_file().expect("save large buffer");
        samples.record(t0.elapsed());
        let _ = saved;
    }
    *resources = sample_resources(&mut None);
    let mb_per_s = bytes / (1_048_576.0) / (samples.avg_ms() / 1_000.0);
    (samples, mb_per_s)
}

// ── Scenario 8: Full syntax parse (10k-line rust fixture) ────────────────────
fn scenario_full_parse() -> FrameSamples {
    let mut text =
        String::from("pub fn compute_10k_lines() -> usize {\n    let mut acc = 0usize;\n");
    for idx in 0..10_000usize {
        text.push_str(&format!("    acc += {idx};\n"));
    }
    text.push_str("    acc\n}\n");

    let mut samples = FrameSamples::new(10);
    for rev in 0..10u64 {
        let t0 = Instant::now();
        let mut engine = SyntaxEngine::new(LanguageId::Rust).expect("create syntax engine");
        let _ = engine.parse_source(&text, rev);
        samples.record(t0.elapsed());
    }
    samples
}

// ── Scenario 9: Workspace scan (= project indexing) ──────────────────────────
fn scenario_workspace_scan() -> (FrameSamples, usize) {
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut samples = FrameSamples::new(3);
    let mut nodes = 0usize;
    for _ in 0..3 {
        let t0 = Instant::now();
        let scanner = WorkspaceScanner::new(
            netherize_editor::workspace::model::WorkspaceIgnoreRules::new([
                "target",
                ".git",
                "node_modules",
            ]),
            WorkspaceScanOptions::default(),
        );
        let scanned = scanner.scan(&root).expect("workspace scan");
        nodes = scanned.len();
        samples.record(t0.elapsed());
    }
    (samples, nodes)
}

// ── Scenario 10: Fuzzy file finder ────────────────────────────────────────────
fn scenario_fuzzy_find() -> (FrameSamples, usize) {
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let model = WorkspaceModel::load(root).expect("workspace model");
    let mut samples = FrameSamples::new(20);
    let mut results = 0usize;
    for query in ["renderer", "command_dispatch", "app_state", "frame"] {
        let t0 = Instant::now();
        let matches = find_file_matches(&model, query, 20);
        results = results.max(matches.len());
        samples.record(t0.elapsed());
    }
    (samples, results)
}

// ── Scenario 11: Content grep across project (= live grep engine) ────────────
fn scenario_content_grep() -> (FrameSamples, usize) {
    fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_dir() {
                if !matches!(name, "target" | ".git" | "node_modules" | ".codegraph") {
                    walk_files(&path, out);
                }
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    let needle = "dispatch_command_with_clipboard_count_with_terminal";
    let mut samples = FrameSamples::new(5);
    let mut hits = 0usize;
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..5 {
        let t0 = Instant::now();
        let mut files = Vec::new();
        walk_files(&root, &mut files);
        hits = 0;
        for file in &files {
            let Ok(size) = fs::metadata(file).map(|m| m.len()) else {
                continue;
            };
            if size > 2 * 1024 * 1024 {
                continue;
            }
            let mut buf = String::new();
            if File::open(file)
                .and_then(|mut f| f.read_to_string(&mut buf))
                .is_err()
            {
                continue;
            }
            if buf.contains(needle) {
                hits += 1;
            }
        }
        samples.record(t0.elapsed());
    }
    (samples, hits)
}

// ── Scenario 12: User-supplied 100 MB log vs the 10 MiB interactive cap ──────
fn scenario_open_user_100mb() -> (FrameSamples, String) {
    let path = PathBuf::from("large.log");
    let mut samples = FrameSamples::new(3);
    let mut status = String::from("missing");
    for _ in 0..3 {
        if !path.exists() {
            break;
        }
        let t0 = Instant::now();
        let mut state = AppState::new(bench_scratch_path("e2e_scratch.txt"));
        match state.open_file(path.clone()) {
            Ok(_) => status = "opened".to_string(),
            Err(e) => status = format!("refused: {e}"),
        }
        samples.record(t0.elapsed());
    }
    (samples, status)
}

// ── Report ────────────────────────────────────────────────────────────────────

struct ScenarioStat {
    label: &'static str,
    samples: FrameSamples,
    extra: Option<String>,
}

impl ScenarioStat {
    fn new(label: &'static str, samples: FrameSamples) -> Self {
        Self {
            label,
            samples,
            extra: None,
        }
    }

    fn with_extra(label: &'static str, samples: FrameSamples, extra: String) -> Self {
        Self {
            label,
            samples,
            extra: Some(extra),
        }
    }
}

fn write_json_report(stats: &[ScenarioStat], mem: &[(&str, ResourceSample)]) {
    let out_dir = PathBuf::from("benchmarks/baselines");
    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("[e2e_perf_runner] cannot create output dir: {e}");
        return;
    }
    let out_path = out_dir.join("bench_latest.json");

    let mut scenarios = serde_json::Map::new();
    for s in stats {
        scenarios.insert(
            s.label.to_string(),
            serde_json::json!({
                "iterations": s.samples.samples_ms.len(),
                "avg_ms": round3(s.samples.avg_ms()),
                "p50_ms": round3(s.samples.percentile(0.50)),
                "p95_ms": round3(s.samples.percentile(0.95)),
                "p99_ms": round3(s.samples.percentile(0.99)),
                "max_ms": round3(s.samples.max_ms()),
                "extra": s.extra,
            }),
        );
    }
    let mut memory = serde_json::Map::new();
    for (label, sample) in mem {
        memory.insert(
            (*label).to_string(),
            serde_json::json!({
                "rss_mb": (sample.rss_mb * 1000.0).round() / 1000.0,
                "cpu_percent": sample.cpu_percent,
            }),
        );
    }

    let json = serde_json::json!({
        "timestamp": chrono_now_iso(),
        "target_fps": 120,
        "target_frame_ms": 8.0,
        "window": { "width": WINDOW_WIDTH, "height": WINDOW_HEIGHT },
        "large_file_cap_bytes": 10 * 1024 * 1024,
        "scenarios": scenarios,
        "memory_cpu": memory,
    });

    match fs::write(&out_path, serde_json::to_string_pretty(&json).unwrap_or_default()) {
        Ok(()) => println!("[e2e_perf_runner] Report written → {}", out_path.display()),
        Err(e) => eprintln!("[e2e_perf_runner] Failed to write report: {e}"),
    }
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn chrono_now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple ISO-8601 approximation (UTC, seconds precision).
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
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
        month += 1;
    }
    let day = rem + 1;
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn print_scenario(stat: &ScenarioStat) {
    let target = 8.0f64;
    let status = if stat.samples.avg_ms() <= target {
        "✓"
    } else {
        "✗"
    };
    let extra = stat
        .extra
        .as_deref()
        .map(|e| format!("  ({e})"))
        .unwrap_or_default();
    println!(
        "  [{status}] {:<44} avg={:>8.3}ms  p95={:>8.3}ms  p99={:>8.3}ms  max={:>8.3}ms{}",
        stat.label,
        stat.samples.avg_ms(),
        stat.samples.percentile(0.95),
        stat.samples.percentile(0.99),
        stat.samples.max_ms(),
        extra
    );
}

fn main() {
    println!("=== Netherize Editor – E2E CPU Benchmark ===");
    println!("    Target: avg_frame_ms <= 8.00ms for 120 FPS");
    println!(
        "    Resolution: {}x{} | Large-file cap: 10 MiB (editor limit)\n",
        WINDOW_WIDTH, WINDOW_HEIGHT
    );

    let mut resource_cache: Option<SysinfoUtils> = None;
    let mut mem_points: Vec<(&str, ResourceSample)> = Vec::new();

    macro_rules! run_step {
        ($idx:expr, $total:expr, $label:expr) => {
            print!("[{}/{}] {}... ", $idx, $total, $label);
            let _ = std::io::stdout().flush();
        };
    }

    const TOTAL_STEPS: usize = 12;

    run_step!(1, TOTAL_STEPS, "Loading 10MB file (5 iters)");
    let load = scenario_load_large_file();
    mem_points.push(("after_open_large_file", sample_resources(&mut resource_cache)));
    println!("done");

    run_step!(2, TOTAL_STEPS, "Jump to last line (10 iters)");
    let jump = scenario_jump_to_last_line();
    println!("done");

    run_step!(3, TOTAL_STEPS, "Insert + scroll loop (100 iters)");
    let mut edit_resource = ResourceSample::default();
    let edit = scenario_insert_and_scroll(&mut edit_resource);
    mem_points.push(("after_typing_burst", edit_resource));
    println!("done");

    run_step!(4, TOTAL_STEPS, "Layout engine 4K (200 iters)");
    let layout = scenario_layout_engine_4k();
    println!("done");

    run_step!(5, TOTAL_STEPS, "Undo/redo burst (200 chars)");
    let mut undo_resource = ResourceSample::default();
    let (undo, redo) = scenario_undo_redo(&mut undo_resource);
    mem_points.push(("after_undo_redo", undo_resource));
    println!("done");

    run_step!(6, TOTAL_STEPS, "Buffer search (whole 10MB)");
    let (search, search_matches) = scenario_search_buffer();
    println!("done ({} matches)", search_matches);

    run_step!(7, TOTAL_STEPS, "Save 10MB buffer (3 iters)");
    let mut save_resource = ResourceSample::default();
    let (save, save_throughput) = scenario_save_file(&mut save_resource);
    println!("done ({:.0} MB/s)", save_throughput);

    run_step!(8, TOTAL_STEPS, "Full syntax parse (10k-line rust)");
    let full_parse = scenario_full_parse();
    println!("done");

    run_step!(9, TOTAL_STEPS, "Workspace scan (this repo)");
    let (ws_scan, ws_nodes) = scenario_workspace_scan();
    println!("done ({} nodes)", ws_nodes);

    run_step!(10, TOTAL_STEPS, "Fuzzy file finder");
    let (fuzzy, fuzzy_best) = scenario_fuzzy_find();
    println!("done");

    run_step!(11, TOTAL_STEPS, "Content grep (.rs files)");
    let (grep, grep_hits) = scenario_content_grep();
    println!("done ({} files hit)", grep_hits);

    run_step!(12, TOTAL_STEPS, "Open user 100MB large.log");
    let (open100, open100_status) = scenario_open_user_100mb();
    println!("done ({})", open100_status);

    let stats = [
        ScenarioStat::with_extra(
            "open_9.93mb_file",
            load.clone(),
            format!("{:.0} MB/s", (LARGE_FILE_BYTES as f64 / 1_048_576.0) / (load.avg_ms() / 1_000.0)),
        ),
        ScenarioStat::new("jump_to_last_line", jump),
        ScenarioStat::new("insert_char_plus_scroll", edit),
        ScenarioStat::new("layout_engine_compute_4k", layout),
        ScenarioStat::with_extra(
            "undo_single_op_burst_200",
            undo,
            String::new(),
        ),
        ScenarioStat::new("redo_single_op_burst_200", redo),
        ScenarioStat::with_extra(
            "search_whole_buffer_first_pass",
            FrameSamples {
                samples_ms: search.samples_ms[..1].to_vec(),
            },
            format!("{search_matches} matches"),
        ),
        ScenarioStat::new("search_next_hop_x50", FrameSamples {
            samples_ms: search.samples_ms[1..].to_vec(),
        }),
        ScenarioStat::with_extra(
            "save_9.93mb_file",
            save,
            format!("{save_throughput:.0} MB/s"),
        ),
        ScenarioStat::new("full_syntax_parse_10k_rust", full_parse),
        ScenarioStat::with_extra(
            "workspace_scan_indexing",
            ws_scan,
            format!("{ws_nodes} nodes"),
        ),
        ScenarioStat::with_extra(
            "fuzzy_file_finder",
            fuzzy,
            format!("best {fuzzy_best} results"),
        ),
        ScenarioStat::with_extra(
            "content_grep_project_rs",
            grep,
            format!("{grep_hits} files hit"),
        ),
        ScenarioStat::with_extra(
            "open_user_100mb_large_log",
            open100,
            open100_status,
        ),
    ];

    println!("\n── Results ──────────────────────────────────────────────────────");
    for stat in &stats {
        print_scenario(stat);
    }
    println!("  memory / cpu:");
    for (label, sample) in &mem_points {
        println!(
            "    {:<28} rss={:>8.1} MB   cpu={:>5.1}%",
            label, sample.rss_mb, sample.cpu_percent
        );
    }
    println!("─────────────────────────────────────────────────────────────────\n");

    write_json_report(&stats, &mem_points);
}
