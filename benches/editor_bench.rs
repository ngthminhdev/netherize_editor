use std::path::{Path, PathBuf};

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use netherize_editor::app::app_state::AppState;
use netherize_editor::core::{
    command_dispatch::dispatch_command,
    commands::{Command, Motion, OperationTarget, Operator},
    mode::ModeEvent,
};
use netherize_editor::syntax::{
    highlight::generate_highlight_spans,
    syntax_engine::{LanguageId, SyntaxEngine},
};

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

fn bench_operator_motion_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("operator_motion_pipeline");
    group.bench_function("dw_undo_loop_10k", |b| {
        b.iter(|| {
            let mut app_state = AppState::from_text(
                PathBuf::from("bench_scratch.txt"),
                "alpha beta gamma delta epsilon zeta eta theta\n",
            );
            let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
            for _ in 0..10_000 {
                let _ = dispatch_command(
                    &mut app_state,
                    Command::Operate {
                        op: Operator::Delete,
                        target: OperationTarget::Motion(Motion::WordForward),
                    },
                );
                let _ = dispatch_command(&mut app_state, Command::Undo);
            }
        });
    });
    group.finish();
}

fn synthetic_rust_buffer(function_count: usize) -> String {
    let mut text = String::from("pub struct BenchState {\n    pub total: usize,\n}\n\n");
    for idx in 0..function_count {
        text.push_str(&format!(
            "pub fn compute_{idx}(input: usize) -> usize {{\n    let value = input + {idx};\n    if value % 2 == 0 {{\n        value / 2\n    }} else {{\n        value * 3\n    }}\n}}\n\n"
        ));
    }
    text
}

struct TypingBenchCase {
    label: &'static str,
    language_id: LanguageId,
    base_text: String,
    insert_anchor: String,
    inserted_text: &'static str,
    burst_tokens: &'static [&'static str],
}

fn synthetic_javascript_buffer(function_count: usize) -> String {
    let mut text = String::from(
        "export class BenchState {\n  constructor() {\n    this.total = 0;\n  }\n}\n\n",
    );
    for idx in 0..function_count {
        text.push_str(&format!(
            "export function compute{idx}(input) {{\n  const value = input + {idx};\n  if (value % 2 === 0) {{\n    return value / 2;\n  }}\n  return value * 3;\n}}\n\n"
        ));
    }
    text
}

fn synthetic_typescript_buffer(function_count: usize) -> String {
    let mut text = String::from(
        "type BenchState = {\n  total: number;\n};\n\nexport const initialState: BenchState = { total: 0 };\n\n",
    );
    for idx in 0..function_count {
        text.push_str(&format!(
            "export function compute{idx}(input: number): number {{\n  const value = input + {idx};\n  if (value % 2 === 0) {{\n    return value / 2;\n  }}\n  return value * 3;\n}}\n\n"
        ));
    }
    text
}

fn synthetic_go_buffer(function_count: usize) -> String {
    let mut text = String::from("package bench\n\ntype BenchState struct {\n\tTotal int\n}\n\n");
    for idx in 0..function_count {
        text.push_str(&format!(
            "func Compute{idx}(input int) int {{\n\tvalue := input + {idx}\n\tif value%2 == 0 {{\n\t\treturn value / 2\n\t}}\n\treturn value * 3\n}}\n\n"
        ));
    }
    text
}

fn typing_bench_cases() -> Vec<TypingBenchCase> {
    vec![
        TypingBenchCase {
            label: "rust",
            language_id: LanguageId::Rust,
            base_text: synthetic_rust_buffer(300),
            insert_anchor: "    let value = input + 42;".to_string(),
            inserted_text: "    let typed_value = value.saturating_add(1);\n",
            burst_tokens: &["x", "_typed", "_x\n"],
        },
        TypingBenchCase {
            label: "javascript",
            language_id: LanguageId::JavaScript,
            base_text: synthetic_javascript_buffer(300),
            insert_anchor: "  const value = input + 42;".to_string(),
            inserted_text: "  const typedValue = value + 1;\n",
            burst_tokens: &["x", "Typed", "X\n"],
        },
        TypingBenchCase {
            label: "typescript",
            language_id: LanguageId::TypeScript,
            base_text: synthetic_typescript_buffer(300),
            insert_anchor: "  const value = input + 42;".to_string(),
            inserted_text: "  const typedValue: number = value + 1;\n",
            burst_tokens: &["x", "Typed", "Value\n"],
        },
        TypingBenchCase {
            label: "go",
            language_id: LanguageId::Go,
            base_text: synthetic_go_buffer(300),
            insert_anchor: "\tvalue := input + 42".to_string(),
            inserted_text: "\ttypedValue := value + 1\n",
            burst_tokens: &["x", "Typed", "X\n"],
        },
    ]
}

fn bench_tree_sitter_typing_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("tree_sitter_typing_latency");
    for case in typing_bench_cases() {
        let insert_anchor = case
            .base_text
            .find(&case.insert_anchor)
            .expect("benchmark source must contain anchor");
        let updated_text = format!(
            "{}{}{}",
            &case.base_text[..insert_anchor],
            case.inserted_text,
            &case.base_text[insert_anchor..]
        );
        let single_insert_name = format!(
            "{}/incremental_parse_and_highlight_single_insert",
            case.label
        );
        let full_insert_name = format!("{}/full_parse_and_highlight_single_insert", case.label);
        let burst_name = format!("{}/incremental_typing_burst_32_edits", case.label);
        let base_text = case.base_text;
        let language_id = case.language_id;
        let inserted_text = case.inserted_text;
        let burst_tokens = case.burst_tokens;

        group.bench_function(&single_insert_name, |b| {
            b.iter(|| {
                let mut engine = SyntaxEngine::new(language_id)
                    .expect("benchmark syntax engine init should succeed");
                let _ = engine
                    .parse_source(&base_text, 1)
                    .expect("initial parse should succeed");
                let tree = engine
                    .parse_incremental(
                        &updated_text,
                        insert_anchor,
                        insert_anchor,
                        insert_anchor + inserted_text.len(),
                        2,
                    )
                    .expect("incremental parse should succeed");
                let spans = generate_highlight_spans(tree, &updated_text);
                criterion::black_box(spans.len());
            });
        });

        group.bench_function(&full_insert_name, |b| {
            b.iter(|| {
                let mut engine = SyntaxEngine::new(language_id)
                    .expect("benchmark syntax engine init should succeed");
                let tree = engine
                    .parse_source(&updated_text, 2)
                    .expect("full parse should succeed");
                let spans = generate_highlight_spans(tree, &updated_text);
                criterion::black_box(spans.len());
            });
        });

        group.bench_function(&burst_name, |b| {
            b.iter(|| {
                let mut engine = SyntaxEngine::new(language_id)
                    .expect("benchmark syntax engine init should succeed");
                let mut text = base_text.clone();
                let _ = engine
                    .parse_source(&text, 1)
                    .expect("initial parse should succeed");
                let mut insert_at = insert_anchor;

                for rev in 0..32 {
                    let typed = burst_tokens[rev % burst_tokens.len()];
                    text.insert_str(insert_at, typed);
                    let tree = engine
                        .parse_incremental(
                            &text,
                            insert_at,
                            insert_at,
                            insert_at + typed.len(),
                            (rev + 2) as u64,
                        )
                        .expect("incremental parse in typing burst should succeed");
                    let spans = generate_highlight_spans(tree, &text);
                    criterion::black_box(spans.len());
                    insert_at += typed.len();
                }
            });
        });
    }

    group.finish();
}

criterion_group!(
    editor_benches,
    bench_open_large_file,
    bench_edit_loop_latency,
    bench_operator_motion_pipeline,
    bench_tree_sitter_typing_latency
);
criterion_main!(editor_benches);
