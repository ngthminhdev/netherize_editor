//! Lightweight "run-and-compare" test runner for the LeetCode / competitive
//! interview workflow.
//!
//! The user authors test cases (each an `input` fed to stdin and an `expected`
//! stdout) and runs the active source file against every case. The actual
//! stdout is captured and compared with a trim-normalized equality check.
//!
//! This module is the pure, render-free core: the state model, output
//! comparison, and run-command resolution. The async execution lives in the
//! worker layer (`async_runtime`); rendering lives in the renderer. Keeping
//! this layer pure makes the comparison and command-resolution logic unit
//! testable without spawning a process or a window.

use std::path::Path;

pub mod leetcode;
pub mod leetcode_adapter;
pub mod leetcode_api;
pub mod leetcode_cache;

/// Outcome of a single test case after (or during) a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestStatus {
    /// Authored but not yet run (or cleared after an edit).
    Pending,
    /// Currently executing.
    Running,
    /// `actual` matched `expected` (trim-normalized).
    Passed,
    /// Process exited cleanly but output did not match.
    Failed,
    /// Process failed to spawn, was killed, timed out, or exited non-zero.
    Error,
}

impl TestStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Running => "Running",
            Self::Passed => "Passed",
            Self::Failed => "Failed",
            Self::Error => "Error",
        }
    }

    /// Short status glyph shown at the head of a case row.
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Pending => "•",
            Self::Running => "…",
            Self::Passed => "✓",
            Self::Failed => "✗",
            Self::Error => "!",
        }
    }
}

/// A single authored test case plus its last result.
#[derive(Debug, Clone, PartialEq)]
pub struct TestCase {
    /// Text fed to the program's stdin.
    pub input: String,
    /// Expected stdout (compared trim-normalized).
    pub expected: String,
    /// Captured stdout from the last run. `None` until run at least once.
    pub actual: Option<String>,
    /// Captured stderr from the last run, surfaced on `Error`.
    pub stderr: Option<String>,
    /// Process exit code from the last run, if the process started.
    pub exit_code: Option<i32>,
    /// Wall-clock duration of the last run in milliseconds.
    pub duration_ms: Option<u64>,
    pub status: TestStatus,
    /// `true` if this case was produced by the AI generator (`g`), so the panel
    /// can flag it for the user to verify the expected value.
    pub ai_generated: bool,
}

impl TestCase {
    /// Author a new pending case from input/expected text.
    pub fn new(input: impl Into<String>, expected: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            expected: expected.into(),
            actual: None,
            stderr: None,
            exit_code: None,
            duration_ms: None,
            status: TestStatus::Pending,
            ai_generated: false,
        }
    }

    /// Author a new case flagged as AI-generated.
    pub fn new_ai(input: impl Into<String>, expected: impl Into<String>) -> Self {
        let mut case = Self::new(input, expected);
        case.ai_generated = true;
        case
    }

    /// Reset the result fields back to `Pending` (e.g. after the source file
    /// was edited, so stale pass/fail marks don't mislead).
    pub fn reset_result(&mut self) {
        self.actual = None;
        self.stderr = None;
        self.exit_code = None;
        self.duration_ms = None;
        self.status = TestStatus::Pending;
    }
}

/// Raw result of executing one case in the worker, before pass/fail is judged.
/// Kept free of `expected` so the comparison stays single-sourced on the main
/// thread (`TestCase::apply_outcome`). Index-aligned with the submitted cases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCaseOutcome {
    /// Captured stdout.
    pub actual: String,
    /// Captured stderr.
    pub stderr: String,
    /// Process exit code, `None` if it never started or was killed.
    pub exit_code: Option<i32>,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// Set when the process could not be spawned or timed out. Implies `Error`.
    pub spawn_error: Option<String>,
}

impl TestCase {
    /// Fold a worker `outcome` into this case and judge pass/fail.
    ///
    /// Precedence: a spawn/timeout error or a non-zero exit code is `Error`
    /// (the program crashed before we can trust its stdout); otherwise the
    /// trim-normalized stdout decides `Passed` vs `Failed`.
    pub fn apply_outcome(&mut self, outcome: TestCaseOutcome) {
        self.actual = Some(outcome.actual.clone());
        self.exit_code = outcome.exit_code;
        self.duration_ms = Some(outcome.duration_ms);
        self.status = if outcome.spawn_error.is_some() || outcome.exit_code != Some(0) {
            TestStatus::Error
        } else {
            match json_outputs_match(&self.expected, &outcome.actual) {
                Ok(true) => TestStatus::Passed,
                Ok(false) | Err(_) => TestStatus::Failed,
            }
        };
        // Prefer a spawn/timeout error message in the stderr slot; otherwise
        // surface real stderr only when non-empty.
        self.stderr = outcome
            .spawn_error
            .or_else(|| (!outcome.stderr.is_empty()).then_some(outcome.stderr))
            .or_else(|| {
                (outcome.exit_code == Some(0))
                    .then(|| json_outputs_match(&self.expected, &outcome.actual).err())
                    .flatten()
            });
    }
}

/// The resolved command used to execute a source file: `program` followed by
/// `leading_args`, then the file path appended as the final argument.
///
/// e.g. Go resolves to `program: "go", leading_args: ["run"]` → `go run file.go`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunCommand {
    pub program: String,
    pub leading_args: Vec<String>,
}

impl RunCommand {
    fn interpreter(program: &str) -> Self {
        Self {
            program: program.to_string(),
            leading_args: Vec::new(),
        }
    }

    fn with_args(program: &str, args: &[&str]) -> Self {
        Self {
            program: program.to_string(),
            leading_args: args.iter().map(|a| a.to_string()).collect(),
        }
    }

    /// Full argument vector for spawning: leading args + the file path.
    pub fn full_args(&self, file: &Path) -> Vec<String> {
        let mut args = self.leading_args.clone();
        args.push(file.to_string_lossy().to_string());
        args
    }

    /// Human-readable preview, e.g. `python3 solution.py`.
    pub fn preview(&self, file: &Path) -> String {
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file.to_string_lossy().to_string());
        let mut parts = vec![self.program.clone()];
        parts.extend(self.leading_args.iter().cloned());
        parts.push(name);
        parts.join(" ")
    }
}

/// Resolve the interpreter/run command for a source file by extension.
///
/// Returns `None` for compiled languages that need a separate build step
/// (Rust, C/C++, Java) — those are deferred to a later iteration. Interpreted
/// languages run directly. The chosen interpreters are the common defaults;
/// a config override layer can replace this map later.
pub fn resolve_run_command(path: &Path) -> Option<RunCommand> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "py" => RunCommand::interpreter("python3"),
        "js" | "mjs" | "cjs" => RunCommand::interpreter("node"),
        "ts" => RunCommand::with_args("npx", &["tsx"]),
        "rb" => RunCommand::interpreter("ruby"),
        "go" => RunCommand::with_args("go", &["run"]),
        "lua" => RunCommand::interpreter("lua"),
        "php" => RunCommand::interpreter("php"),
        "pl" => RunCommand::interpreter("perl"),
        "sh" | "bash" => RunCommand::interpreter("bash"),
        _ => return None,
    })
}

/// A one-time compile step run before any test case. On failure every case is
/// marked Error with the compiler's stderr.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileStep {
    pub program: String,
    pub args: Vec<String>,
}

/// How to execute a source file: an optional compile step plus the per-case run
/// command. Interpreted languages have `compile: None` and run the source
/// directly; compiled languages (currently Rust) compile to a binary first and
/// then run that binary once per case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunPlan {
    pub compile: Option<CompileStep>,
    /// Program spawned per case (interpreter, or the compiled binary).
    pub program: String,
    /// Args for the per-case run (includes the source file for interpreted
    /// languages; empty for a compiled binary).
    pub args: Vec<String>,
}

impl RunPlan {
    /// Human-readable preview, e.g. `python3 solution.py` or
    /// `rustc solution.rs → run`.
    pub fn preview(&self, path: &Path) -> String {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        if self.compile.is_some() {
            let prog = std::path::Path::new(&self.program)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| self.program.clone());
            format!("rustc {name} → {prog}")
        } else {
            let mut parts = vec![self.program.clone()];
            // Drop the trailing file path arg for display, show the basename.
            for arg in self.args.iter() {
                if arg == &path.to_string_lossy() {
                    parts.push(name.clone());
                } else {
                    parts.push(arg.clone());
                }
            }
            parts.join(" ")
        }
    }
}

/// Resolve how to run `path`. Interpreted languages run directly; compiled
/// languages compile to `output_binary` first. `output_binary` is only used
/// for compiled languages (ignored otherwise) — callers can pass any path.
pub fn resolve_run_plan(path: &Path, output_binary: &Path) -> Option<RunPlan> {
    if let Some(cmd) = resolve_run_command(path) {
        let args = cmd.full_args(path);
        return Some(RunPlan {
            compile: None,
            program: cmd.program,
            args,
        });
    }

    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "rs" => Some(RunPlan {
            compile: Some(CompileStep {
                program: "rustc".to_string(),
                args: vec![
                    path.to_string_lossy().to_string(),
                    "-O".to_string(),
                    "--edition".to_string(),
                    "2021".to_string(),
                    "-o".to_string(),
                    output_binary.to_string_lossy().to_string(),
                ],
            }),
            program: output_binary.to_string_lossy().to_string(),
            args: Vec::new(),
        }),
        _ => None,
    }
}

/// Compare expected vs actual program output with competitive-judge
/// normalization: trailing whitespace on each line is stripped and trailing
/// blank lines are ignored. Internal blank lines and ordering are significant.
pub fn outputs_match(expected: &str, actual: &str) -> bool {
    normalize_output(expected) == normalize_output(actual)
}

pub fn validate_json_text(label: &str, text: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(text).map_err(|error| format!("Invalid {label} JSON: {error}"))
}

pub fn json_outputs_match(expected: &str, actual: &str) -> Result<bool, String> {
    let expected = validate_json_text("expected", expected)?;
    let actual = validate_json_text("actual output", actual)?;
    Ok(expected == actual)
}

/// Strip trailing whitespace from every line and drop trailing blank lines.
/// `\r\n` is normalized to `\n` so cross-platform captures compare equal.
pub fn normalize_output(s: &str) -> String {
    let unified = s.replace("\r\n", "\n");
    let trimmed: Vec<&str> = unified.lines().map(|line| line.trim_end()).collect();
    // Drop trailing empty lines.
    let mut end = trimmed.len();
    while end > 0 && trimmed[end - 1].is_empty() {
        end -= 1;
    }
    trimmed[..end].join("\n")
}

/// Which editable column of a case the panel cursor is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TestField {
    #[default]
    Input,
    Expected,
}

impl TestField {
    pub fn label(self) -> &'static str {
        match self {
            Self::Input => "Input",
            Self::Expected => "Expected",
        }
    }

    fn toggled(self) -> Self {
        match self {
            Self::Input => Self::Expected,
            Self::Expected => Self::Input,
        }
    }
}

/// All test-runner state for the active session. Lives in `AppState`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TestRunnerState {
    /// Authored cases in display order.
    pub cases: Vec<TestCase>,
    /// `true` while a run is in flight (UI shows a spinner / blocks re-run).
    pub is_running: bool,
    /// `true` while an AI test-case generation request is in flight, so the
    /// panel can show a non-blocking "Generating…" spinner.
    pub is_generating: bool,
    /// The command resolved for the last run, for display (e.g. `python3 sol.py`).
    pub last_command_preview: Option<String>,
    /// Why the last run could not start, if it couldn't (e.g. unsupported
    /// language, no active file). Cleared on a successful launch.
    pub launch_error: Option<String>,
    /// Index of the case the UI has focused/selected, if any.
    pub selected: Option<usize>,
    /// Which column (Input/Expected) the panel cursor is on.
    pub focused_field: TestField,
    pub scroll_offset: usize,
}

impl TestRunnerState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of authored cases.
    pub fn len(&self) -> usize {
        self.cases.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cases.is_empty()
    }

    /// Append a new pending case and select it (Input column).
    pub fn add_case(&mut self, input: impl Into<String>, expected: impl Into<String>) {
        self.cases.push(TestCase::new(input, expected));
        self.selected = Some(self.cases.len() - 1);
        self.focused_field = TestField::Input;
    }

    /// Remove the case at `idx`, fixing up the selection. Returns `true` if a
    /// case was removed.
    pub fn remove_case(&mut self, idx: usize) -> bool {
        if idx >= self.cases.len() {
            return false;
        }
        self.cases.remove(idx);
        self.selected = match self.cases.len() {
            0 => None,
            len => Some(idx.min(len - 1)),
        };
        true
    }

    /// Reset every case's result to `Pending` (e.g. after the source changed).
    pub fn reset_results(&mut self) {
        for case in &mut self.cases {
            case.reset_result();
        }
    }

    /// Mark all cases `Running` at the start of a run.
    pub fn mark_all_running(&mut self) {
        self.is_running = true;
        self.launch_error = None;
        for case in &mut self.cases {
            case.status = TestStatus::Running;
            case.actual = None;
            case.stderr = None;
            case.exit_code = None;
            case.duration_ms = None;
        }
    }

    /// Count of passed cases after a run.
    pub fn passed_count(&self) -> usize {
        self.cases
            .iter()
            .filter(|c| c.status == TestStatus::Passed)
            .count()
    }

    /// `true` when every case passed and there is at least one case.
    pub fn all_passed(&self) -> bool {
        !self.cases.is_empty() && self.passed_count() == self.cases.len()
    }

    /// The currently selected case, if any.
    pub fn selected_case(&self) -> Option<&TestCase> {
        self.selected.and_then(|idx| self.cases.get(idx))
    }

    /// Move the selection to the next case (wraps). Exits edit mode. No-op when
    /// there are fewer than two cases. Returns `true` if anything changed.
    pub fn select_next(&mut self) -> bool {
        self.move_selection(1)
    }

    /// Move the selection to the previous case (wraps). Exits edit mode.
    pub fn select_prev(&mut self) -> bool {
        self.move_selection(-1)
    }

    fn move_selection(&mut self, delta: isize) -> bool {
        let len = self.cases.len();
        if len == 0 {
            self.selected = None;
            return false;
        }
        let cur = self.selected.unwrap_or(0) as isize;
        let next = ((cur + delta).rem_euclid(len as isize)) as usize;
        if self.selected == Some(next) {
            return false;
        }
        self.selected = Some(next);
        true
    }

    /// Switch the focused column between Input and Expected.
    pub fn toggle_field(&mut self) -> bool {
        self.focused_field = self.focused_field.toggled();
        true
    }

    pub fn validate_cases_json(&mut self) -> Result<(), String> {
        for (index, case) in self.cases.iter().enumerate() {
            if let Err(error) = validate_json_text("input", &case.input) {
                self.selected = Some(index);
                self.focused_field = TestField::Input;
                return Err(error);
            }
            if let Err(error) = validate_json_text("expected", &case.expected) {
                self.selected = Some(index);
                self.focused_field = TestField::Expected;
                return Err(error);
            }
        }
        Ok(())
    }

    pub fn scroll_cases(&mut self, delta: isize) -> bool {
        let max_offset = self.cases.len().saturating_sub(1);
        let next = (self.scroll_offset as isize + delta).clamp(0, max_offset as isize) as usize;
        if next == self.scroll_offset {
            return false;
        }
        self.scroll_offset = next;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn normalize_strips_trailing_ws_and_blank_lines() {
        assert_eq!(normalize_output("ab  \ncd\n\n\n"), "ab\ncd");
        assert_eq!(normalize_output("ab\r\ncd\r\n"), "ab\ncd");
        assert_eq!(normalize_output("   "), "");
    }

    #[test]
    fn outputs_match_ignores_trailing_whitespace_and_newlines() {
        assert!(outputs_match("42\n", "42"));
        assert!(outputs_match("1 2 3", "1 2 3   \n\n"));
        assert!(!outputs_match("1\n2", "2\n1")); // ordering significant
        assert!(!outputs_match("1\n2\n3", "1\n\n2\n3")); // internal blank significant
    }

    #[test]
    fn json_outputs_match_by_value_not_formatting_or_object_key_order() {
        assert_eq!(
            json_outputs_match(r#"{"a":1,"b":[2,3]}"#, "{\n  \"b\": [2, 3],\n  \"a\": 1\n}"),
            Ok(true)
        );
        assert_eq!(json_outputs_match("[1,2]", "[2,1]"), Ok(false));
    }

    #[test]
    fn json_validation_identifies_the_invalid_side() {
        assert!(validate_json_text("input", r#"{"nums":[1,2]}"#).is_ok());
        let error = validate_json_text("expected", "[1,").expect_err("invalid json");
        assert!(error.contains("expected JSON"));
    }

    #[test]
    fn resolve_run_command_maps_known_extensions() {
        assert_eq!(
            resolve_run_command(&PathBuf::from("sol.py")),
            Some(RunCommand::interpreter("python3"))
        );
        assert_eq!(
            resolve_run_command(&PathBuf::from("sol.go")),
            Some(RunCommand::with_args("go", &["run"]))
        );
        assert_eq!(resolve_run_command(&PathBuf::from("sol.rs")), None);
        assert_eq!(resolve_run_command(&PathBuf::from("noext")), None);
    }

    #[test]
    fn run_command_builds_full_args_and_preview() {
        let cmd = RunCommand::with_args("go", &["run"]);
        let file = PathBuf::from("/tmp/proj/main.go");
        assert_eq!(
            cmd.full_args(&file),
            vec!["run".to_string(), "/tmp/proj/main.go".to_string()]
        );
        assert_eq!(cmd.preview(&file), "go run main.go");
    }

    #[test]
    fn add_remove_keeps_selection_sane() {
        let mut state = TestRunnerState::new();
        state.add_case("1", "1");
        state.add_case("2", "4");
        state.add_case("3", "9");
        assert_eq!(state.selected, Some(2));
        assert!(state.remove_case(2));
        assert_eq!(state.selected, Some(1));
        assert!(state.remove_case(0));
        assert_eq!(state.selected, Some(0));
        assert!(state.remove_case(0));
        assert_eq!(state.selected, None);
        assert!(!state.remove_case(0));
    }

    #[test]
    fn mark_all_running_clears_prior_results() {
        let mut state = TestRunnerState::new();
        state.add_case("in", "out");
        state.cases[0].status = TestStatus::Passed;
        state.cases[0].actual = Some("out".to_string());
        state.mark_all_running();
        assert!(state.is_running);
        assert_eq!(state.cases[0].status, TestStatus::Running);
        assert_eq!(state.cases[0].actual, None);
    }

    fn outcome(actual: &str, exit: Option<i32>) -> TestCaseOutcome {
        TestCaseOutcome {
            actual: actual.to_string(),
            stderr: String::new(),
            exit_code: exit,
            duration_ms: 3,
            spawn_error: None,
        }
    }

    #[test]
    fn apply_outcome_judges_pass_fail_and_error() {
        let mut pass = TestCase::new("in", "42");
        pass.apply_outcome(outcome("42\n", Some(0)));
        assert_eq!(pass.status, TestStatus::Passed);
        assert_eq!(pass.duration_ms, Some(3));

        let mut fail = TestCase::new("in", "42");
        fail.apply_outcome(outcome("43", Some(0)));
        assert_eq!(fail.status, TestStatus::Failed);

        let mut nonzero = TestCase::new("in", "42");
        nonzero.apply_outcome(outcome("42", Some(1)));
        assert_eq!(nonzero.status, TestStatus::Error);

        let mut crashed = TestCase::new("in", "42");
        crashed.apply_outcome(TestCaseOutcome {
            actual: String::new(),
            stderr: String::new(),
            exit_code: None,
            duration_ms: 0,
            spawn_error: Some("spawn failed: not found".to_string()),
        });
        assert_eq!(crashed.status, TestStatus::Error);
        assert_eq!(crashed.stderr.as_deref(), Some("spawn failed: not found"));
    }

    #[test]
    fn navigation_wraps() {
        let mut state = TestRunnerState::new();
        state.add_case("1", "1");
        state.add_case("2", "2");
        assert_eq!(state.selected, Some(1));
        assert!(state.select_next()); // wraps 1 -> 0
        assert_eq!(state.selected, Some(0));
        assert!(state.select_prev());
        assert_eq!(state.selected, Some(1));
    }

    #[test]
    fn all_passed_requires_nonempty() {
        let mut state = TestRunnerState::new();
        assert!(!state.all_passed());
        state.add_case("1", "1");
        state.cases[0].status = TestStatus::Passed;
        assert!(state.all_passed());
        state.add_case("2", "2");
        assert!(!state.all_passed());
    }
}
