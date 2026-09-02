# Dojo v2 — Workspace + Tree List + Problem Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rework the Dojo after the first GUI round: dedicated LeetCode workspace folder (user-chosen), problem tree in the LEFT dock, problem statement/hints in a RIGHT "Problem" tab, no text prompts (no approach gate, no notebook questions), explicit language picker.

**Architecture:** Pure rules stay in `src/dojo/*` (state, tree rows, problem view). Glue stays in `commands_dojo.rs`. The left list reuses the Explorer row renderer (`SidebarRow`), the right panel replaces `build_dojo_content`. Selection triggers a debounced statement-only fetch (new worker job) so the statement shows before Enter.

**Tech Stack:** Rust, winit/wgpu renderer, tokio scheduler, rfd folder dialog, serde/toml, chrono.

**Spec:** `docs/superpowers/specs/2026-09-02-dojo-interview-prep-design.md` (§15 "v2 rework" appended by this round).

## Global Constraints

- No `.unwrap()`/`.expect()` outside tests; no heavy file I/O on the UI thread (small TOML/template writes allowed, precedent: state.toml).
- Golden data flow: keymap → `Command` → dispatch passthrough → `AppShell` handler.
- `rtk grep` lies → `/usr/bin/grep`. Tests via `AppShell::new_for_tests()` (scheduler drops requests). macOS temp paths → `canonicalize()`.
- No commits unless the user says so (user said "commit đi nếu cần save" → checkpoint commits allowed this round).

---

### Task 1: State + plan + problems (pure)

**Files:** `src/dojo/state.rs`, `src/dojo/plan.rs`, `src/dojo/problems.rs`, `src/dojo/notebook.rs`, `src/dojo/files.rs`, `config/dojo/plan.toml`

- `DojoState`: add `workspace: Option<String>`, `language: Option<String>`, `collapsed: Vec<String>` (category keys). Remove `last_group`. Remove `ActiveSession.approach` and `Attempt.approach`.
- `Plan`: remove `[[group]]`/`Group`/`Page`/`pages()`/`group_*`/`page_*`; `notebook`/`sd_dir` become `Option<String>` (None → `<workspace>/notes.md`, `<workspace>/sd`). Keep `dsa_minutes`, `dsa_phases`, `sd_minutes`, `sd_cases`.
- `problems.rs`: `category_label(key) -> &'static str` (18 NeetCode labels), `Problems::categories() -> Vec<String>` (first-seen order), `Problems::in_category(key)`.
- `notebook.rs`: `format_block(date, id, title, outcome, elapsed_s, redo)` writes a stub the user fills (`- Bí ở đâu:` …). Drop approach/answers.
- `files.rs`: `problem_dir(workspace, id, slug) -> PathBuf` (`0001-two-sum`), `notebook_path(plan, workspace)`, `sd_dir(plan, workspace)`.
- Tests: update existing ones; add `category_label_covers_all_bundled_categories`, `problem_dir_is_zero_padded`.

### Task 2: Tree rows + problem view (pure)

**Files:** `src/dojo/view.rs`

- `TreeRow { Group{key,label,done,total,expanded}, Problem{slug,id,title,difficulty,status:RowGlyph,trailing}, SdGroup{done,total,expanded}, SdCase{key,label,done} }`.
- `tree_rows(problems, plan, state, redo_only, today) -> Vec<TreeRow>`; redo_only → only due problems, groups without due rows hidden, SD hidden.
- `DojoHeader { overall_done, overall_total, streak, redo_due }`.
- `ProblemView { title, id, difficulty, category, language, status_line, statement_lines, examples: Vec<(String,String)>, hints: Vec<String>, loading: bool, error: Option<String> }` built by `problem_view(problem, state, cache: Option<&LeetCodeProblemCache>, language, today)`.
- `suggested_next`, `welcome_card`, `wrap_text` keep (no pages).
- Tests: tree order/fold/redo-only, problem_view status lines.

### Task 3: Statement-only fetch job

**Files:** `src/runner/leetcode_api.rs`, `src/runner/leetcode_cache.rs`, `src/async_runtime/scheduler/leetcode_fetch.rs`, `src/async_runtime/scheduler/dispatch.rs`, `src/async_runtime/message.rs`

- GraphQL: add `difficulty hints` to `questionData`; `LeetCodeProblem.difficulty`, `.hints`. Cache gets `hints: Vec<String>` (`#[serde(default)]`).
- `WorkerRequestPayload::FetchLeetCodeStatement { slug }` → `run_leetcode_statement_fetch` (fetch_problem + extract_test_cases + save_problem_cache) → `LeetCodeStatementFetched { slug }` / `LeetCodeStatementFetchFailed { slug, message }`.

### Task 4: Commands, keymap, palette modes

**Files:** `src/core/commands.rs`, `src/core/command_ids.rs`, `src/core/command_dispatch/mod.rs`, `src/core/command_dispatch/editing.rs`, `config/keymaps/default.toml`, `src/app/command_palette.rs`, `src/app/event_loop/commands_palette.rs`, `src/app/input_map/mod.rs`, `src/app/input/handler.rs`, `src/app/input/tests.rs`

- Commands: remove `DojoPageNext/Prev`; add `DojoCollapse`, `DojoExpand`, `DojoToggleHints`, `DojoLanguage`, `DojoChooseFolder`, `DojoOpenNotebook`.
- IDs: `dojo.open`, `dojo.language`, `dojo.choose_folder` in ALL_IDS + parse + palette catalog ("Dojo: Open (LeetCode workspace)", "Dojo: Language", "Dojo: Choose Folder").
- Palette: remove `DojoApproach`, `DojoNote`; add `DojoLanguage` selector mode (items = `leetcode_language`, action `CreateLeetCodeFile(key)` reinterpreted) with confirm → `confirm_dojo_language`.
- `InputFocusContext::DojoProblem` (+ `as_str` "dojo_problem", leader, sequence). setup.rs: left `Dojo` → `Dojo`, right `Problem` → `DojoProblem`.
- `route_dojo_input`: Esc/q, Enter, j/k/↑↓, h/←, l/→, r, c (language), w (folder), n (notebook), i, x, `?` hints, Ctrl-d/u. `route_dojo_problem_input`: j/k/Ctrl-d/u scroll, `?`, Enter → test runner, x, i, Esc/q.

### Task 5: Panel tabs + runtime glue

**Files:** `src/workbench/panel_state.rs`, `src/app/event_loop/commands_dojo.rs`, `src/app/event_loop/commands_explorer.rs`, `src/app/event_loop/commands_terminal.rs`, `src/app/event_loop/async_results/leetcode_fetch.rs`, `src/app/event_loop/async_results/runner.rs`, `src/app/event_loop/application.rs`

- `PanelTabId::Problem` ("Problem", icon `\u{f15c}`); left default `[Explorer, Outline, Dojo]`, right `[AiChat, TestRunner, Problem]`.
- `DojoRuntime`: `selected`, `list_scroll`, `redo_only`, `scroll` (statement), `show_hints`, `pending_start`, `pending_preview: Option<(String, Instant)>`, `preview_inflight`, `preview_error`, `open_after_switch`, `armed`, `last_phase`, `last_tick_second`, `cache: Option<(String, LeetCodeProblemCache)>`.
- `dojo_open`: no workspace → `dojo_choose_folder`; different root → `open_after_switch = true; switch_workspace_with_files`; `perform_workspace_switch` tail calls `dojo_after_workspace_switch(&root)`; else `dojo_show_panels`.
- `dojo_start_selected`: group → toggle fold; SD → sd session; problem → language missing → picker (pending_start) else existing `solution.<ext>` → open + session; else `submit_leetcode_fetch_to(slug, lang, problem_dir)`.
- Fetch result dojo branch: cases → test runner, `dojo_begin_dsa_session` opens the file immediately, right tab Problem, focus editor.
- `dojo_tick`: expiry, phase toasts, debounced preview submit.
- `dojo_end_session`: record + save + toast + auto-append notebook block; no prompts.
- `dojo_sidebar_rows(theme) -> Vec<SidebarRow>` for the left dock; `dojo_problem_model()` for the right.
- Mouse: click on left Dojo rows selects (double = start) using `sidebar_line_height` geometry; right dock hit-test not needed.

### Task 6: Renderer

**Files:** `src/render/renderer/ui/sidebar.rs` (none if `explorer_rows` reused), `src/render/renderer/ui/test_runner.rs`

- Left: pass Dojo `SidebarRow`s through the existing `explorer_rows` slot when the left tab is Dojo (no filter bar).
- Right: replace `build_dojo_content` with `build_problem_content(bounds, &ProblemPanelModel, inner_padding)`.

### Task 7: Tests, docs, verification

- Rewrite Dojo tests in `commands_tests.rs` + `input/tests.rs`; `cargo test --lib`; clippy on touched lines; `npx gitnexus analyze`; README + lessons + spec §15 + memory.
