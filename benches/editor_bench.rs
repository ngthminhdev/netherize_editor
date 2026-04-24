use std::path::{Path, PathBuf};

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use netherize_editor::app::app_state::AppState;

const INPUTS_DIR: &str = "benchmarks/inputs";
const FILE_10K: &str = "rust_10k_lines.rs";
const FILE_50MB: &str = "log_50mb.txt";

fn sample_path(file_name: &str) -> PathBuf {
    Path::new(INPUTS_DIR).join(file_name)
}

fn require_sample(path: &Path) {
    assert!(
        path.exists(),
        "sample file not found: {}. run ./scripts/generate_bench_samples.sh first",
        path.display()
    );
}

fn bench_open_large_file(c: &mut Criterion) {
    let mut group = c.benchmark_group("open_large_file");

    for (label, file_name) in [("10k_lines", FILE_10K), ("50mb_log", FILE_50MB)] {
        let path = sample_path(file_name);
        require_sample(&path);

        group.bench_with_input(BenchmarkId::new("open", label), &path, |b, path| {
            b.iter(|| {
                let mut app_state = AppState::new(PathBuf::from("bench_scratch.txt"));
                app_state
                    .open_file(path.clone())
                    .expect("open file in benchmark should succeed");
            });
        });
    }
    group.finish();
}

fn bench_edit_loop_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("edit_loop_latency");
    group.bench_function("insert_move_backspace_20k", |b| {
        b.iter(|| {
            let mut app_state =
                AppState::from_text(PathBuf::from("bench_scratch.txt"), "fn main() {}\n");
            for idx in 0..20_000 {
                app_state.insert_char('x');
                app_state.move_left();
                app_state.move_right();
                if idx % 4 == 0 {
                    app_state.backspace();
                }
            }
        });
    });
    group.finish();
}

criterion_group!(
    editor_benches,
    bench_open_large_file,
    bench_edit_loop_latency
);
criterion_main!(editor_benches);
