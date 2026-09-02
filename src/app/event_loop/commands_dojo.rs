//! Dojo panel handlers: open/navigate the problem menu, run timed sessions,
//! collect the error-notebook entry. Pure rules live in `crate::dojo`; this
//! file is the editor glue on `AppShell`.
use std::path::PathBuf;

use crate::{
    async_runtime::message::{RequestSpec, RequestTopic, TextFileOp, WorkerRequestPayload},
    core::commands::Command,
    dojo::{
        notebook::{html_to_text, mm_ss},
        plan::{Page, Plan},
        problems::Problems,
        session::{SessionKind, phase_at, single_phase},
        state::{DojoState, Outcome, now_unix, today_local},
        view::{self, DojoPanelModel, DojoSessionView},
    },
    workbench::{focus_manager::FocusTarget, panel_state::PanelTabId},
};

use super::*;
use crate::{
    app::command_palette::CommandPaletteMode,
    dojo::{
        files::{
            INTERVIEWER_PROMPT, current_md, current_md_path, expand_tilde, interviewer_md_path,
            sd_template,
        },
        notebook::{NOTEBOOK_HEADER, format_block, format_sd_block},
        state::{ActiveSession, Attempt, Status, date_str, parse_date},
    },
    runner::TestStatus,
};

/// Notebook block being collected through the `DojoNote` prompts.
pub(in crate::app::event_loop) struct PendingNote {
    pub date: String,
    pub id: u32,
    pub title: String,
    pub outcome: Outcome,
    pub elapsed_s: u64,
    pub redo: bool,
    pub approach: String,
    pub kind: SessionKind,
    pub answers: Vec<(&'static str, String)>,
}

pub(in crate::app::event_loop) struct DojoRuntime {
    pub plan: Plan,
    pub problems: Problems,
    pub state: DojoState,
    /// `None` in tests: saves are skipped so the real `dojo.toml` is untouched.
    pub save_path: Option<PathBuf>,
    pub page: Page,
    pub selected: usize,
    pub redo_only: bool,
    pub scroll: usize,
    /// Fetch submitted by the Dojo; matched against the fetch result's slug.
    pub pending_start: Option<String>,
    pub pending_note: Option<PendingNote>,
    pub last_phase: Option<usize>,
    pub last_tick_second: u64,
    /// Set when a session is started or resumed via `g o`; gates the tick.
    pub armed: bool,
    /// (slug, plain statement) so the panel doesn't hit the disk every frame.
    pub statement_cache: Option<(String, String)>,
}

impl DojoRuntime {
    fn with(plan: Plan, problems: Problems, state: DojoState, save_path: Option<PathBuf>) -> Self {
        let page = view::initial_page(&plan, &problems, &state);
        Self {
            plan,
            problems,
            state,
            save_path,
            page,
            selected: 0,
            redo_only: false,
            scroll: 0,
            pending_start: None,
            pending_note: None,
            last_phase: None,
            last_tick_second: 0,
            armed: false,
            statement_cache: None,
        }
    }

    /// Startup load. Small TOML files; sync like `AppPersistentState::load`.
    #[cfg(not(test))]
    pub fn load() -> Self {
        let dir = crate::dojo::files::dojo_dir();
        let plan = Plan::load(&dir.join("plan.toml"));
        let problems = Problems::load(&dir.join("neetcode150.toml"));
        let path = DojoState::state_path();
        let state = DojoState::load(&path);
        Self::with(plan, problems, state, Some(path))
    }

    #[cfg(test)]
    pub fn for_tests() -> Self {
        Self::with(
            Plan::bundled(),
            Problems::bundled(),
            DojoState::default(),
            None,
        )
    }

    pub fn save(&self) {
        if let Some(path) = &self.save_path
            && let Err(err) = self.state.save(path)
        {
            eprintln!("[dojo] save failed: {err}");
        }
    }

    pub fn rows(&self) -> Vec<view::DojoRow> {
        view::list_rows(
            &self.plan,
            &self.problems,
            &self.state,
            self.page,
            self.redo_only,
            today_local(),
        )
    }

    pub fn header(&self) -> view::DojoHeader {
        view::header(
            &self.plan,
            &self.problems,
            &self.state,
            self.page,
            today_local(),
        )
    }

    pub fn selected_row(&self) -> Option<view::DojoRow> {
        self.rows().into_iter().nth(self.selected)
    }

    pub fn session_phases(&self, kind: SessionKind) -> Vec<(String, u32)> {
        match kind {
            SessionKind::Dsa => self.plan.dsa_phases.clone(),
            SessionKind::Sd => single_phase("SD", self.plan.sd_minutes),
        }
    }

    /// Statusbar chip: `⏱ CODE 11:42` + color code (0 info / 1 accent /
    /// 2 warning / 3 magenta / 4 cyan / 5 error). Field-level borrow so the
    /// frame builder can call it while the renderer is borrowed mutably.
    pub fn statusbar_chip(&self) -> Option<(String, u8)> {
        let s = self.state.active_session.as_ref().filter(|_| self.armed)?;
        let now = now_unix();
        let phases = self.session_phases(s.kind);
        let phase = phase_at(&phases, s.elapsed_s(now));
        let name = phase
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "HẾT".to_string());
        let remaining = s.remaining_s(now);
        let code = if remaining < 60 {
            5
        } else {
            match (s.kind, phase.map(|p| p.index)) {
                (SessionKind::Sd, _) => 4,
                (_, Some(0)) => 0,
                (_, Some(1)) => 1,
                (_, Some(2)) => 2,
                _ => 3,
            }
        };
        Some((format!("⏱ {name} {}", mm_ss(remaining)), code))
    }

    /// Welcome-screen card text (title, subtitle). Field-level borrow.
    pub fn welcome_card(&self) -> (String, String) {
        view::welcome_card(
            &self.plan,
            &self.problems,
            &self.state,
            self.page,
            today_local(),
        )
    }

    fn clamp_selection(&mut self) {
        let len = self.rows().len();
        self.selected = if len == 0 {
            0
        } else {
            self.selected.min(len - 1)
        };
    }
}

impl AppShell {
    pub(in crate::app::event_loop) fn handle_dojo_command(
        &mut self,
        command: &Command,
    ) -> Option<bool> {
        match command {
            Command::DojoOpen => Some(self.dojo_open()),
            Command::DojoSelectNext => Some(self.dojo_move_selection(1)),
            Command::DojoSelectPrev => Some(self.dojo_move_selection(-1)),
            Command::DojoToggleRedo => {
                self.dojo.redo_only = !self.dojo.redo_only;
                self.dojo.selected = 0;
                Some(true)
            }
            Command::DojoPageNext => Some(self.dojo_turn_page(1)),
            Command::DojoPagePrev => Some(self.dojo_turn_page(-1)),
            Command::DojoScrollDown => {
                self.dojo.scroll = self.dojo.scroll.saturating_add(10);
                Some(true)
            }
            Command::DojoScrollUp => {
                self.dojo.scroll = self.dojo.scroll.saturating_sub(10);
                Some(true)
            }
            Command::DojoUnfocus => {
                let changed = self.focus_manager.set(FocusTarget::CenterEditor);
                if changed {
                    self.input_handler.clear_pending_prefix();
                }
                Some(true)
            }
            Command::DojoStart => Some(self.dojo_start_selected()),
            Command::DojoGiveUp => Some(self.dojo_give_up()),
            Command::DojoInterviewer => Some(self.dojo_launch_interviewer()),
            _ => None,
        }
    }

    pub(in crate::app::event_loop) fn dojo_open(&mut self) -> bool {
        let _ = self.release_focus_mode_to_editor();
        self.dojo_resume_if_needed();
        self.panel_state.right.visible = true;
        self.panel_state.right.switch_to_tab(PanelTabId::Dojo);
        let focus_changed = self.focus_manager.set(FocusTarget::RightSidebar);
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }
        let _ = self.dismiss_initial_launch_welcome_if_active();
        self.dojo.clamp_selection();
        self.sidebar_needs_layout = true;
        true
    }

    fn dojo_move_selection(&mut self, delta: i32) -> bool {
        let len = self.dojo.rows().len();
        if len == 0 {
            return false;
        }
        let next = (self.dojo.selected as i64 + i64::from(delta)).clamp(0, len as i64 - 1) as usize;
        let changed = next != self.dojo.selected;
        self.dojo.selected = next;
        changed
    }

    /// Fire-and-forget small text writes (notebook, current.md…); failures toast.
    pub(in crate::app::event_loop) fn submit_text_file_ops(&mut self, ops: Vec<TextFileOp>) {
        if ops.is_empty() {
            return;
        }
        let _ = self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::FileOperation,
            payload: WorkerRequestPayload::WriteTextFiles { ops },
        });
    }

    /// (id, plain statement) for a slug. The statement comes from the LeetCode
    /// per-problem cache once a fetch has run; cached in memory so the panel
    /// doesn't touch the disk every frame.
    pub(in crate::app::event_loop) fn dojo_problem_context(&mut self, slug: &str) -> (u32, String) {
        let Some(id) = self.dojo.problems.by_slug(slug).map(|p| p.id) else {
            return (0, String::new());
        };
        if let Some((cached_slug, text)) = &self.dojo.statement_cache
            && cached_slug == slug
        {
            return (id, text.clone());
        }
        let text = crate::runner::leetcode_cache::load_cache_in(
            &crate::runner::leetcode_cache::cache_dir(),
            &id.to_string(),
        )
        .map(|cache| html_to_text(&cache.statement))
        .unwrap_or_default();
        if !text.is_empty() {
            self.dojo.statement_cache = Some((slug.to_string(), text.clone()));
        }
        (id, text)
    }

    fn dojo_session_phases(&self, kind: SessionKind) -> Vec<(String, u32)> {
        self.dojo.session_phases(kind)
    }

    /// One frame's worth of panel data (rows + optional session view).
    pub(in crate::app::event_loop) fn dojo_panel_model(&mut self, focused: bool) -> DojoPanelModel {
        let session = self
            .dojo
            .state
            .active_session
            .clone()
            .filter(|_| self.dojo.armed);
        let session = session.map(|s| {
            let now = now_unix();
            let phases = self.dojo_session_phases(s.kind);
            let phase = phase_at(&phases, s.elapsed_s(now));
            let (id, statement) = match s.kind {
                SessionKind::Dsa => self.dojo_problem_context(&s.slug),
                SessionKind::Sd => (0, String::new()),
            };
            DojoSessionView {
                title: if id > 0 {
                    format!("#{id} {}", s.title)
                } else {
                    s.title.clone()
                },
                phase: phase
                    .as_ref()
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "HẾT GIỜ".to_string()),
                phase_index: phase.as_ref().map(|p| p.index).unwrap_or(usize::MAX),
                remaining: mm_ss(s.remaining_s(now)),
                remaining_s: s.remaining_s(now),
                statement_lines: statement.split("\n\n").map(str::to_string).collect(),
                approach: s.approach.clone(),
                kind: s.kind,
                expired: s.is_expired(now),
            }
        });
        DojoPanelModel {
            header: self.dojo.header(),
            rows: self.dojo.rows(),
            selected: self.dojo.selected,
            scroll: self.dojo.scroll,
            redo_only: self.dojo.redo_only,
            session,
            focused,
        }
    }

    fn dojo_session_language(&self) -> String {
        self.persistent_state
            .recent_leetcode_languages
            .first()
            .cloned()
            .unwrap_or_else(|| "javascript".to_string())
    }

    /// Enter on a row. With a live session: reopen the approach gate (DSA,
    /// no approach yet) or jump to the Test Runner. Otherwise start the row.
    pub(in crate::app::event_loop) fn dojo_start_selected(&mut self) -> bool {
        if let Some(session) = self
            .dojo
            .state
            .active_session
            .clone()
            .filter(|_| self.dojo.armed)
        {
            if session.kind == SessionKind::Dsa && session.approach.is_none() {
                return self.open_prompt_overlay(CommandPaletteMode::DojoApproach);
            }
            return self.handle_test_runner_focus();
        }
        let Some(row) = self.dojo.selected_row() else {
            return false;
        };
        match row.kind {
            SessionKind::Sd => self.dojo_begin_sd_session(&row.slug, &row.title),
            SessionKind::Dsa => {
                let language = self.dojo_session_language();
                self.dojo.pending_start = Some(row.slug.clone());
                self.submit_leetcode_fetch(row.slug.clone(), language);
                self.show_transient_toast_kind(
                    format!("Dojo\nĐang tải #{} {}…", row.id, row.title),
                    ToastKind::Info,
                );
                true
            }
        }
    }

    /// System-design session: outline file from the 45' framework template,
    /// markdown preview beside it, single-phase clock. `x` finishes it.
    pub(in crate::app::event_loop) fn dojo_begin_sd_session(
        &mut self,
        key: &str,
        label: &str,
    ) -> bool {
        let dir = expand_tilde(&self.dojo.plan.sd_dir);
        let path = dir.join(format!("{key}.md"));
        if !path.exists() {
            // ponytail: tiny file written sync so OpenFile below sees it
            // (state.toml precedent); a worker write would race the open.
            let written = std::fs::create_dir_all(&dir)
                .and_then(|_| std::fs::write(&path, sd_template(label, &date_str(today_local()))));
            if let Err(err) = written {
                self.show_transient_toast_kind(
                    format!("Dojo\nKhông tạo được {}: {err}", path.display()),
                    ToastKind::Error,
                );
                return false;
            }
        }
        self.dojo.state.active_session = Some(ActiveSession {
            kind: SessionKind::Sd,
            slug: key.to_string(),
            title: label.to_string(),
            started_unix: now_unix(),
            budget_s: self.dojo.plan.sd_budget_s(),
            approach: None,
            file: path.clone(),
        });
        self.dojo.armed = true;
        self.dojo.last_phase = Some(0);
        self.dojo.last_tick_second = now_unix();
        self.dojo.statement_cache = None;
        self.dojo.save();
        self.dojo_ensure_interviewer_prompt();
        self.dojo_write_current_md(None);
        self.dojo_open_session_file(&path);
        // ponytail: no auto markdown preview — ToggleMarkdownPreview swaps the
        // center buffer for a preview and hides the file; the user toggles it.
        self.focus_manager.set(FocusTarget::CenterEditor);
        self.input_handler.clear_pending_prefix();
        self.show_transient_toast_kind(
            format!(
                "SD · {} phút\n1 Yêu cầu 5' → 2 Quy mô 5' → 3 API 5' → 4 Kiến trúc 10' → 5 Đào sâu 15' → 6 Đánh đổi 5'",
                self.dojo.plan.sd_minutes
            ),
            ToastKind::Info,
        );
        true
    }

    /// `i`: Claude Code with the interviewer prompt in the right dock, primed
    /// with `current.md` for the running session or the selected row.
    pub(in crate::app::event_loop) fn dojo_launch_interviewer(&mut self) -> bool {
        let Some(agent) = crate::app::ai_agents::ai_agent("interviewer").copied() else {
            return false;
        };
        self.dojo_ensure_interviewer_prompt();
        if self.dojo.state.active_session.is_some() && self.dojo.armed {
            self.dojo_write_current_md(None);
        } else if let Some(row) = self.dojo.selected_row() {
            let (id, statement) = match row.kind {
                SessionKind::Dsa => self.dojo_problem_context(&row.slug),
                SessionKind::Sd => (0, String::new()),
            };
            let phases = self.dojo.session_phases(row.kind);
            let language = match row.kind {
                SessionKind::Dsa => self.dojo_session_language(),
                SessionKind::Sd => String::new(),
            };
            let text = current_md(
                row.kind, id, &row.title, &statement, &language, &phases, None,
            );
            self.submit_text_file_ops(vec![TextFileOp::Write {
                path: current_md_path(),
                contents: text,
            }]);
        }
        if !self.panel_state.right.visible {
            self.panel_state.right.visible = true;
            self.sidebar_needs_layout = true;
        }
        self.panel_state.right.switch_to_tab(PanelTabId::AiChat);
        self.spawn_right_agent_terminal(agent.command, agent.label);
        self.focus_manager.set(FocusTarget::RightSidebar);
        self.input_handler.clear_pending_prefix();
        if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::FocusTerminal) {
            let _ = result;
        }
        self.right_terminal_needs_layout = true;
        self.show_transient_toast("Interviewer\nĐang mở claude… nói hướng làm trước khi code.");
        true
    }

    /// Fetch landed for a Dojo start: open the session in THINK with the file
    /// still closed, and ask for the approach line.
    pub(in crate::app::event_loop) fn dojo_begin_dsa_session(
        &mut self,
        slug: String,
        title: String,
        file: PathBuf,
        _language: String,
    ) {
        let budget_s = self.dojo.plan.dsa_budget_s();
        self.dojo.state.active_session = Some(ActiveSession {
            kind: SessionKind::Dsa,
            slug,
            title,
            started_unix: now_unix(),
            budget_s,
            approach: None,
            file,
        });
        self.dojo.armed = true;
        self.dojo.last_phase = Some(0);
        self.dojo.last_tick_second = now_unix();
        self.dojo.statement_cache = None;
        self.dojo.save();
        self.dojo_ensure_interviewer_prompt();
        self.dojo_write_current_md(None);
        self.dojo_open();
        let minutes = self.dojo.plan.dsa_phases.first().map(|p| p.1).unwrap_or(3);
        self.show_transient_toast_kind(
            format!("THINK · {minutes} phút\nĐọc đề, gõ hướng làm + độ phức tạp."),
            ToastKind::Info,
        );
        if !self.open_prompt_overlay(CommandPaletteMode::DojoApproach) {
            self.show_transient_toast("Dojo\nEnter trong panel để nhập hướng làm.");
        }
    }

    /// Approach prompt confirmed: a non-empty line unlocks the solution file.
    pub(in crate::app::event_loop) fn confirm_dojo_approach(&mut self) -> bool {
        let text = self
            .app_state
            .command_palette_query_text()
            .trim()
            .to_string();
        if text.is_empty() {
            self.show_transient_toast_kind(
                "Dojo\nGõ hướng làm trước đã (Esc = để sau).",
                ToastKind::Warning,
            );
            return true;
        }
        let Some(file) = self.dojo.state.active_session.as_mut().map(|s| {
            s.approach = Some(text.clone());
            s.file.clone()
        }) else {
            return false;
        };
        self.dojo.save();
        self.dojo_write_current_md(Some(&text));
        let _ = self.app_state.close_command_palette();
        if self.app_state.current_mode() == EditorMode::PaletteFocus
            && let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus)
        {
            let _ = result;
        }
        self.dojo_open_session_file(&file);
        self.panel_state.right.visible = true;
        self.panel_state.right.switch_to_tab(PanelTabId::TestRunner);
        self.focus_manager.set(FocusTarget::CenterEditor);
        self.input_handler.clear_pending_prefix();
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        let code_minutes = self.dojo.plan.dsa_phases.get(1).map(|p| p.1).unwrap_or(15);
        self.show_transient_toast_kind(
            format!("CODE · {code_minutes} phút\nF5 chạy case. Pass hết = xong."),
            ToastKind::Success,
        );
        true
    }

    /// Open a session file with the same post-open plumbing the fetch handler uses.
    pub(in crate::app::event_loop) fn dojo_open_session_file(&mut self, file: &std::path::Path) {
        let report = dispatch_command(&mut self.app_state, Command::OpenFile(file.to_path_buf()));
        if !report.success {
            self.show_transient_toast_kind(
                format!("Dojo\nKhông mở được {}", file.display()),
                ToastKind::Error,
            );
            return;
        }
        self.clear_highlight_layers();
        self.submit_workspace_rescan();
        self.submit_active_buffer_git_baseline_refresh();
        self.submit_parse_for_active_buffer(true);
        self.submit_lsp_did_open_for_active_file();
        self.explorer_reveal_file(file);
        self.submit_lsp_check_for_path(file.to_path_buf());
    }

    /// Rewrite `current.md` (what the AI interviewer reads) for the live session.
    pub(in crate::app::event_loop) fn dojo_write_current_md(&mut self, approach: Option<&str>) {
        let Some(s) = self.dojo.state.active_session.clone() else {
            return;
        };
        let (id, statement) = match s.kind {
            SessionKind::Dsa => self.dojo_problem_context(&s.slug),
            SessionKind::Sd => (0, String::new()),
        };
        let phases = self.dojo_session_phases(s.kind);
        let language = match s.kind {
            SessionKind::Dsa => self.dojo_session_language(),
            SessionKind::Sd => String::new(),
        };
        let text = current_md(
            s.kind,
            id,
            &s.title,
            &statement,
            &language,
            &phases,
            approach.or(s.approach.as_deref()),
        );
        self.submit_text_file_ops(vec![TextFileOp::Write {
            path: current_md_path(),
            contents: text,
        }]);
    }

    /// Ship the default interviewer prompt once; the user may edit it afterwards.
    pub(in crate::app::event_loop) fn dojo_ensure_interviewer_prompt(&mut self) {
        self.submit_text_file_ops(vec![TextFileOp::WriteIfMissing {
            path: interviewer_md_path(),
            contents: INTERVIEWER_PROMPT.to_string(),
        }]);
    }

    /// A session saved before a restart: arm it again on the first `g o`.
    /// An expired one ends as `timeout` on the next tick (note prompts follow).
    fn dojo_resume_if_needed(&mut self) {
        if self.dojo.armed {
            return;
        }
        let Some(session) = self.dojo.state.active_session.clone() else {
            return;
        };
        self.dojo.armed = true;
        self.dojo.last_phase = None;
        self.dojo.statement_cache = None;
        if session.approach.is_some() && session.file.exists() {
            self.dojo_open_session_file(&session.file);
        }
    }

    // ── Clock ─────────────────────────────────────────────────────────────

    /// Once per event-loop turn. Returns true when the chip/panel must redraw.
    pub(in crate::app::event_loop) fn dojo_tick(&mut self) -> bool {
        self.dojo_flush_abandoned_note();
        let Some(session) = self
            .dojo
            .state
            .active_session
            .clone()
            .filter(|_| self.dojo.armed)
        else {
            return false;
        };
        let now = now_unix();
        if session.is_expired(now) {
            self.dojo_end_session(Outcome::Timeout);
            return true;
        }
        let phases = self.dojo_session_phases(session.kind);
        if let Some(phase) = phase_at(&phases, session.elapsed_s(now))
            && self.dojo.last_phase != Some(phase.index)
        {
            self.dojo.last_phase = Some(phase.index);
            self.dojo.last_tick_second = now;
            let minutes = phases.get(phase.index).map(|p| p.1).unwrap_or(0);
            let hint = match phase.name.as_str() {
                "CODE" => "Gõ đi. F5 để chạy case.",
                "TEST" => "Tự test: rỗng, 1 phần tử, trùng, âm, tràn.",
                "REVIEW" => "Xem lời giải tối ưu, ghi sổ nếu lệch.",
                _ => "",
            };
            self.show_transient_toast_kind(
                format!("{} · {minutes} phút\n{hint}", phase.name),
                ToastKind::Info,
            );
            return true;
        }
        if self.dojo.last_tick_second != now {
            self.dojo.last_tick_second = now;
            return true;
        }
        false
    }

    // ── Session end ───────────────────────────────────────────────────────

    /// Test Runner finished: every case green on the session file = pass.
    pub(in crate::app::event_loop) fn dojo_on_run_completed(&mut self, all_passed: bool) {
        if !all_passed {
            return;
        }
        let Some(session) = self
            .dojo
            .state
            .active_session
            .clone()
            .filter(|s| self.dojo.armed && s.kind == SessionKind::Dsa)
        else {
            return;
        };
        let same_file = self.app_state.active_file().is_some_and(|active| {
            active == session.file || active.canonicalize().ok() == session.file.canonicalize().ok()
        });
        if same_file {
            self.dojo_end_session(Outcome::Pass);
        }
    }

    /// `x`: end early. DSA → fail (had red cases) or giveup; SD → finished.
    pub(in crate::app::event_loop) fn dojo_give_up(&mut self) -> bool {
        let Some(session) = self
            .dojo
            .state
            .active_session
            .clone()
            .filter(|_| self.dojo.armed)
        else {
            self.show_transient_toast("Dojo\nKhông có phiên nào đang chạy.");
            return false;
        };
        let outcome = match session.kind {
            SessionKind::Sd => Outcome::Pass,
            SessionKind::Dsa => {
                if self
                    .app_state
                    .test_runner
                    .cases
                    .iter()
                    .any(|c| c.status == TestStatus::Failed)
                {
                    Outcome::Fail
                } else {
                    Outcome::Giveup
                }
            }
        };
        self.dojo_end_session(outcome);
        true
    }

    /// Record the attempt, apply spaced redo, toast the summary, then collect
    /// the notebook entry through the `DojoNote` prompts.
    pub(in crate::app::event_loop) fn dojo_end_session(&mut self, outcome: Outcome) {
        let Some(session) = self.dojo.state.active_session.take() else {
            return;
        };
        let now = now_unix();
        let elapsed_s = session.elapsed_s(now).min(session.budget_s);
        let today = today_local();
        let approach = session.approach.clone().unwrap_or_default();
        self.dojo.state.record_attempt(
            Attempt {
                slug: session.slug.clone(),
                kind: session.kind,
                started_unix: session.started_unix,
                ended_unix: now,
                outcome,
                elapsed_s,
                approach: approach.clone(),
            },
            today,
        );
        self.dojo.armed = false;
        self.dojo.last_phase = None;
        self.dojo.save();
        self.submit_text_file_ops(vec![TextFileOp::Remove {
            path: current_md_path(),
        }]);

        let id = match session.kind {
            SessionKind::Dsa => self.dojo_problem_context(&session.slug).0,
            SessionKind::Sd => 0,
        };
        let redo = self.dojo.state.status_of(&session.slug) == Status::Redo;
        let redo_at = self
            .dojo
            .state
            .progress_of(&session.slug)
            .redo_at
            .as_deref()
            .and_then(parse_date)
            .map(|d| d.format("%d/%m").to_string())
            .unwrap_or_default();
        let streak = self.dojo.state.streak(today);
        let (summary, kind) = match outcome {
            Outcome::Pass => (
                format!(
                    "{} · pass {} · streak {streak}",
                    session.title,
                    mm_ss(elapsed_s)
                ),
                ToastKind::Success,
            ),
            _ => (
                format!(
                    "{} · {} {} · redo {redo_at}",
                    session.title,
                    outcome.label(),
                    mm_ss(elapsed_s)
                ),
                ToastKind::Warning,
            ),
        };
        self.show_transient_toast_kind(summary, kind);

        self.dojo.pending_note = Some(PendingNote {
            date: date_str(today),
            id,
            title: session.title.clone(),
            outcome,
            elapsed_s,
            redo,
            approach,
            kind: session.kind,
            answers: Vec::new(),
        });
        self.dojo_open();
        let step = if outcome == Outcome::Pass || session.kind == SessionKind::Sd {
            0
        } else {
            1
        };
        if !self.open_prompt_overlay(CommandPaletteMode::DojoNote(step)) {
            self.dojo_flush_pending_note();
        }
    }

    /// Notebook prompt step confirmed. Step 0 (pass note) is single; fail
    /// walks 1 → 2 → 3. The block is written when the last step lands.
    pub(in crate::app::event_loop) fn confirm_dojo_note(&mut self, step: u8) -> bool {
        let text = self
            .app_state
            .command_palette_query_text()
            .trim()
            .to_string();
        let label = match step {
            0 => "Ghi chú",
            1 => "Bí",
            2 => "Pattern",
            _ => "Dấu hiệu",
        };
        if let Some(note) = self.dojo.pending_note.as_mut() {
            note.answers.push((label, text));
        }
        if step == 0 || step >= 3 {
            self.dojo_flush_pending_note();
            let _ = self.app_state.close_command_palette();
            if self.app_state.current_mode() == EditorMode::PaletteFocus
                && let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus)
            {
                let _ = result;
            }
            self.focus_manager.set(FocusTarget::RightSidebar);
            self.input_handler.clear_pending_prefix();
            return true;
        }
        let opened = self.open_prompt_overlay(CommandPaletteMode::DojoNote(step + 1));
        let _ = self.app_state.set_command_palette_query("");
        opened
    }

    /// Append the collected block to the notebook (worker write).
    pub(in crate::app::event_loop) fn dojo_flush_pending_note(&mut self) {
        let Some(note) = self.dojo.pending_note.take() else {
            return;
        };
        let contents = match note.kind {
            SessionKind::Sd => format_sd_block(
                &note.date,
                &note.title,
                note.elapsed_s,
                note.answers.first().map(|a| a.1.as_str()).unwrap_or(""),
            ),
            SessionKind::Dsa => {
                let answers: Vec<(&str, &str)> = note
                    .answers
                    .iter()
                    .map(|(label, answer)| (*label, answer.as_str()))
                    .collect();
                format_block(
                    &note.date,
                    note.id,
                    &note.title,
                    note.outcome,
                    note.elapsed_s,
                    note.redo,
                    &note.approach,
                    &answers,
                )
            }
        };
        let path = expand_tilde(&self.dojo.plan.notebook);
        self.submit_text_file_ops(vec![TextFileOp::Append {
            path,
            header: NOTEBOOK_HEADER.to_string(),
            contents,
        }]);
    }

    /// Esc mid-way through the note prompts: write what was collected so far.
    pub(in crate::app::event_loop) fn dojo_flush_abandoned_note(&mut self) {
        if self.dojo.pending_note.is_some()
            && !matches!(
                self.app_state.command_palette_mode(),
                Some(CommandPaletteMode::DojoNote(_))
            )
        {
            self.dojo_flush_pending_note();
        }
    }

    fn dojo_turn_page(&mut self, delta: i32) -> bool {
        let pages = self.dojo.plan.pages();
        let Some(idx) = pages.iter().position(|p| *p == self.dojo.page) else {
            return false;
        };
        let next = (idx as i64 + i64::from(delta)).clamp(0, pages.len() as i64 - 1) as usize;
        if next == idx {
            return false;
        }
        self.dojo.page = pages[next];
        self.dojo.selected = 0;
        self.dojo.scroll = 0;
        self.dojo.state.last_group = Some(self.dojo.plan.page_key(self.dojo.page));
        self.dojo.save();
        true
    }
}
