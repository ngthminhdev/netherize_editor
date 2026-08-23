use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
    sync::OnceLock,
};

use criterion::{BatchSize, Criterion, Throughput, black_box, criterion_group, criterion_main};
use netherize_editor::{
    app::app_state::AppState,
    syntax::{
        highlight::generate_highlight_spans_in_byte_window,
        syntax_engine::{LanguageId, SyntaxEngine},
    },
};

const EDIT_LOOP_INSERTIONS: u64 = 10_000;
// Editor hard-refuses interactive files > 5 MiB (INTERACTIVE_TEXT_FILE_LIMIT_BYTES),
// so the largest openable bench file sits just under the cap.
const LARGE_FILE_BYTES: usize = 5 * 1024 * 1024 - 64 * 1024;
const LARGE_FILE_NAME: &str = "netherize_editor_bench_large.log";

struct BenchLanguageCase {
    label: &'static str,
    language_id: LanguageId,
    edit_fixture: fn() -> String,
    edit_anchor: &'static str,
    parse_fixture: fn() -> String,
    parse_anchor: &'static str,
    parse_inserted: &'static str,
}

fn bench_scratch_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(name)
}

fn ensure_50mb_log_file() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let path = bench_scratch_path(LARGE_FILE_NAME);
        let current_size = fs::metadata(&path).map(|meta| meta.len() as usize).ok();
        if current_size == Some(LARGE_FILE_BYTES) {
            return path;
        }

        let mut file = File::create(&path).expect("create large benchmark input");
        let line = b"2026-04-30T00:00:00Z level=info userId=bench fps=120 rtt=4 loss=0 jitter=1 message=\"frame stable\"\n";
        let mut written = 0usize;
        while written < LARGE_FILE_BYTES {
            let remaining = LARGE_FILE_BYTES - written;
            let n = remaining.min(line.len());
            file.write_all(&line[..n])
                .expect("write 50MB benchmark input");
            written += n;
        }
        file.sync_all().expect("sync 50MB benchmark input");
        path
    })
}

fn rust_edit_fixture() -> String {
    "fn main() {\n    let mut value = 0usize;\n    // typing anchor \n    value += 1;\n}\n"
        .to_string()
}

fn rust_10k_line_fixture() -> String {
    let mut text =
        String::from("pub fn compute_10k_lines() -> usize {\n    let mut acc = 0usize;\n");
    for idx in 0..10_000usize {
        text.push_str(&format!("    acc += {idx};\n"));
    }
    text.push_str("    acc\n}\n");
    text
}

fn javascript_edit_fixture() -> String {
    "export function main() {\n  let value = 0;\n  // typing anchor \n  value += 1;\n}\n"
        .to_string()
}

fn javascript_10k_line_fixture() -> String {
    let mut text = String::from("export function compute10kLines() {\n  let acc = 0;\n");
    for idx in 0..10_000usize {
        text.push_str(&format!("  acc += {idx};\n"));
    }
    text.push_str("  return acc;\n}\n");
    text
}

fn typescript_edit_fixture() -> String {
    "export function main(): number {\n  let value = 0;\n  // typing anchor \n  value += 1;\n  return value;\n}\n"
        .to_string()
}

fn typescript_10k_line_fixture() -> String {
    let mut text = String::from("export function compute10kLines(): number {\n  let acc = 0;\n");
    for idx in 0..10_000usize {
        text.push_str(&format!("  acc += {idx};\n"));
    }
    text.push_str("  return acc;\n}\n");
    text
}

fn go_edit_fixture() -> String {
    "package main\n\nfunc main() {\n\tvalue := 0\n\t// typing anchor \n\tvalue += 1\n}\n"
        .to_string()
}

fn go_10k_line_fixture() -> String {
    let mut text = String::from("package bench\n\nfunc Compute10kLines() int {\n\tacc := 0\n");
    for idx in 0..10_000usize {
        text.push_str(&format!("\tacc += {idx}\n"));
    }
    text.push_str("\treturn acc\n}\n");
    text
}

fn bench_language_cases() -> [BenchLanguageCase; 4] {
    [
        BenchLanguageCase {
            label: "rust",
            language_id: LanguageId::Rust,
            edit_fixture: rust_edit_fixture,
            edit_anchor: "// typing anchor ",
            parse_fixture: rust_10k_line_fixture,
            parse_anchor: "    acc += 5000;",
            parse_inserted: " + 1",
        },
        BenchLanguageCase {
            label: "javascript",
            language_id: LanguageId::JavaScript,
            edit_fixture: javascript_edit_fixture,
            edit_anchor: "// typing anchor ",
            parse_fixture: javascript_10k_line_fixture,
            parse_anchor: "  acc += 5000;",
            parse_inserted: " + 1",
        },
        BenchLanguageCase {
            label: "typescript",
            language_id: LanguageId::TypeScript,
            edit_fixture: typescript_edit_fixture,
            edit_anchor: "// typing anchor ",
            parse_fixture: typescript_10k_line_fixture,
            parse_anchor: "  acc += 5000;",
            parse_inserted: " + 1",
        },
        BenchLanguageCase {
            label: "go",
            language_id: LanguageId::Go,
            edit_fixture: go_edit_fixture,
            edit_anchor: "// typing anchor ",
            parse_fixture: go_10k_line_fixture,
            parse_anchor: "\tacc += 5000",
            parse_inserted: " + 1",
        },
    ]
}

fn line_col_for_byte(text: &str, byte_idx: usize) -> (usize, usize) {
    let clamped = byte_idx.min(text.len());
    let mut line = 0usize;
    let mut line_start = 0usize;
    for (idx, ch) in text.char_indices() {
        if idx >= clamped {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = idx + ch.len_utf8();
        }
    }
    (line, text[line_start..clamped].chars().count())
}

fn highlight_window(text_len: usize, edit_byte: usize) -> std::ops::Range<usize> {
    edit_byte.saturating_sub(512)..edit_byte.saturating_add(512).min(text_len)
}

fn bench_edit_loop_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("edit_loop_latency");
    group.throughput(Throughput::Elements(EDIT_LOOP_INSERTIONS));
    for case in bench_language_cases() {
        group.bench_function(
            &format!("insert_10k_chars_with_incremental_parse/{}", case.label),
            |b| {
                b.iter_batched(
                    || {
                        let text = (case.edit_fixture)();
                        let anchor = text.find(case.edit_anchor).expect("typing anchor exists")
                            + case.edit_anchor.len();
                        let (line, col) = line_col_for_byte(&text, anchor);
                        let mut app_state = AppState::from_text(
                            bench_scratch_path(&format!("edit_loop_{}.txt", case.label)),
                            &text,
                        );
                        let _ = app_state.jump_to_line_and_column(line, col);
                        let mut engine =
                            SyntaxEngine::new(case.language_id).expect("create syntax engine");
                        let _ = engine.parse_source(&text, app_state.revision());
                        (app_state, engine)
                    },
                    |(mut app_state, mut engine)| {
                        for _ in 0..EDIT_LOOP_INSERTIONS {
                            let edit_byte = app_state.cursor_byte_idx();
                            app_state.insert_char('x');
                            let text = app_state.text_string();
                            let tree = engine
                                .parse_incremental(
                                    &text,
                                    edit_byte,
                                    edit_byte,
                                    edit_byte + 1,
                                    app_state.revision(),
                                )
                                .expect("incremental edit-loop parse");
                            let spans = generate_highlight_spans_in_byte_window(
                                tree,
                                &text,
                                highlight_window(text.len(), edit_byte),
                            );
                            black_box(spans.len());
                        }
                        black_box(app_state.text_len_bytes());
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }
    group.finish();
}

fn bench_large_file_load(c: &mut Criterion) {
    let path = ensure_50mb_log_file().clone();
    let mut group = c.benchmark_group("large_file_load");
    group.throughput(Throughput::Bytes(LARGE_FILE_BYTES as u64));
    group.bench_function("read_large_load_buffer_initial_line_col", |b| {
        b.iter(|| {
            let mut app_state = AppState::new(bench_scratch_path("large_file_scratch.txt"));
            // Refusal (>10MiB cap) must fail the bench loudly — a silent `let _`
            // here is what let the old 50MB scenario report fake numbers.
            app_state
                .open_file(path.clone())
                .expect("load large benchmark file (must be under 5 MiB cap)");
            black_box(app_state.text_len_bytes());
            black_box(app_state.cursor_line_col());
        });
    });
    group.finish();
}

fn bench_incremental_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("incremental_parse");
    group.throughput(Throughput::Elements(1));
    for case in bench_language_cases() {
        let base_text = (case.parse_fixture)();
        let anchor = base_text
            .find(case.parse_anchor)
            .expect("10k line fixture anchor exists")
            + case.parse_anchor.len();
        let updated_text = format!(
            "{}{}{}",
            &base_text[..anchor],
            case.parse_inserted,
            &base_text[anchor..]
        );

        group.bench_function(&format!("{}_10k_lines_single_line_edit", case.label), |b| {
            b.iter_batched(
                || {
                    let mut engine =
                        SyntaxEngine::new(case.language_id).expect("create syntax engine");
                    let _ = engine
                        .parse_source(&base_text, 1)
                        .expect("initial 10k-line parse");
                    engine
                },
                |mut engine| {
                    let tree = engine
                        .parse_incremental(
                            &updated_text,
                            anchor,
                            anchor,
                            anchor + case.parse_inserted.len(),
                            2,
                        )
                        .expect("incremental 10k-line parse");
                    black_box(tree.root_node().kind());
                    black_box(tree.revision());
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(
    editor_benches,
    bench_edit_loop_latency,
    bench_large_file_load,
    bench_incremental_parse
);
criterion_main!(editor_benches);
