//! Dojo glue on `AppShell`: the LeetCode workspace, the problem tree in the
//! left dock, the Problem tab in the right dock, timed sessions. Pure rules
//! live in `crate::dojo`.
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crate::{
    async_runtime::message::{RequestSpec, RequestTopic, TextFileOp, WorkerRequestPayload},
    core::commands::Command,
    dojo::{
        files::{
            INTERVIEWER_PROMPT, current_md, current_md_path, expand_tilde, interviewer_md_path,
            notebook_path, problem_dir, sd_dir, sd_template,
        },
        notebook::{NOTEBOOK_HEADER, format_block, format_sd_block, html_to_text, mm_ss},
        plan::Plan,
        problems::Problems,
        session::{SessionKind, phase_at, single_phase},
        state::{
            ActiveSession, Attempt, DojoState, Outcome, Status, date_str, now_millis, now_unix,
            parse_date, today_local,
        },
        view::{
            self, DojoSessionView, PanelContent, ProblemPanelModel, RowGlyph, SdView, TreeRow,
            difficulty_letter,
        },
    },
    render::renderer::SidebarRow,
    runner::{
        TestStatus,
        leetcode_cache::{LeetCodeProblemCache, cache_dir, load_cache_in},
    },
    workbench::{focus_manager::FocusTarget, panel_state::PanelTabId},
};

use super::*;
use crate::app::command_palette::{CommandPaletteAction, CommandPaletteItem};

/// Selection must rest this long before the statement preview is fetched.
const PREVIEW_DEBOUNCE: Duration = Duration::from_millis(300);

pub(in crate::app::event_loop) struct DojoRuntime {
    pub plan: Plan,
    pub problems: Problems,
    pub state: DojoState,
    /// `None` in tests: saves are skipped so the real `dojo.toml` is untouched.
    pub save_path: Option<PathBuf>,
    /// Index into `rows()` (the flattened tree).
    pub selected: usize,
    /// First tree row drawn in the left dock.
    pub list_scroll: usize,
    pub redo_only: bool,
    /// Statement scroll (lines) in the Problem tab.
    pub scroll: usize,
    pub show_hints: bool,
    /// Fetch submitted by the Dojo; matched against the fetch result's slug.
    pub pending_start: Option<String>,
    /// Debounced statement preview: (slug, earliest submit time).
    pub pending_preview: Option<(String, Instant)>,
    pub preview_inflight: Option<String>,
    pub preview_error: Option<(String, String)>,
    /// `g o` asked for a workspace switch; show the panels once it lands.
    pub open_after_switch: bool,
    pub last_phase: Option<usize>,
    pub last_tick_second: u64,
    /// Set when a session is started or resumed; gates the tick.
    pub armed: bool,
    /// Memoised per-problem cache lookup: (slug, on-disk cache or None).
    cache: Option<(String, Option<LeetCodeProblemCache>)>,
}

impl DojoRuntime {
    fn with(plan: Plan, problems: Problems, state: DojoState, save_path: Option<PathBuf>) -> Self {
        Self {
            plan,
            problems,
            state,
            save_path,
            selected: 0,
            list_scroll: 0,
            redo_only: false,
            scroll: 0,
            show_hints: false,
            pending_start: None,
            pending_preview: None,
            preview_inflight: None,
            preview_error: None,
            open_after_switch: false,
            last_phase: None,
            last_tick_second: 0,
            armed: false,
            cache: None,
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

    pub fn workspace(&self) -> Option<PathBuf> {
        self.state
            .workspace
            .as_deref()
            .filter(|w| !w.trim().is_empty())
            .map(expand_tilde)
    }

    pub fn rows(&self) -> Vec<TreeRow> {
        view::tree_rows(
            &self.problems,
            &self.plan,
            &self.state,
            self.redo_only,
            today_local(),
        )
    }

    pub fn header(&self) -> view::DojoHeader {
        view::header(&self.problems, &self.state, today_local())
    }

    pub fn selected_row(&self) -> Option<TreeRow> {
        self.rows().into_iter().nth(self.selected)
    }

    /// Move the selection to the row with `key` (expanding its group first).
    pub fn select_key(&mut self, key: &str) -> bool {
        if let Some(p) = self.problems.by_slug(key) {
            let category = p.category.clone();
            self.state.collapsed.retain(|c| *c != category);
        }
        let Some(idx) = self.rows().iter().position(|r| r.key() == key) else {
            return false;
        };
        self.selected = idx;
        true
    }

    fn toggle_group(&mut self, key: &str, expand: bool) -> bool {
        let is_collapsed = self.state.collapsed.iter().any(|c| c == key);
        if expand != is_collapsed {
            return false;
        }
        if expand {
            self.state.collapsed.retain(|c| c != key);
        } else {
            self.state.collapsed.push(key.to_string());
        }
        true
    }

    pub fn session_phases(&self, kind: SessionKind) -> Vec<(String, u32)> {
        match kind {
            SessionKind::Dsa => self.plan.dsa_phases.clone(),
            SessionKind::Sd => single_phase("SD", self.plan.sd_minutes),
        }
    }

    pub fn language_key(&self) -> Option<&str> {
        self.state.language.as_deref()
    }

    /// "JavaScript" / "Python"…, or "no language" until picked.
    pub fn language_label(&self) -> String {
        self.language_key()
            .and_then(crate::runner::leetcode::leetcode_template)
            .map(|t| t.label.to_string())
            .unwrap_or_else(|| "no language".to_string())
    }

    /// Per-problem LeetCode cache (statement, cases, hints), memoised per slug.
    pub fn cached(&mut self, slug: &str) -> Option<&LeetCodeProblemCache> {
        let fresh = !matches!(&self.cache, Some((s, _)) if s == slug);
        if fresh {
            let loaded = self
                .problems
                .by_slug(slug)
                .and_then(|p| load_cache_in(&cache_dir(), &p.id.to_string()));
            self.cache = Some((slug.to_string(), loaded));
        }
        self.cache.as_ref().and_then(|(_, c)| c.as_ref())
    }

    pub fn invalidate_cache(&mut self) {
        self.cache = None;
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
            .unwrap_or_else(|| "OVER".to_string());
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
        view::welcome_card(&self.problems, &self.state, today_local())
    }

    /// Earliest moment the event loop must wake for the Dojo: the next whole
    /// second while a clock runs, or the debounced preview submit.
    pub fn next_deadline(&self) -> Option<Instant> {
        let mut deadline: Option<Instant> = None;
        if self.armed && self.state.active_session.is_some() {
            let to_next_second = 1000 - (now_millis() % 1000);
            deadline = Some(Instant::now() + Duration::from_millis(to_next_second.max(1)));
        }
        if let Some((_, due)) = &self.pending_preview {
            deadline = Some(deadline.map_or(*due, |d| d.min(*due)));
        }
        deadline
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
                self.dojo.list_scroll = 0;
                self.dojo_selection_changed();
                Some(true)
            }
            Command::DojoCollapse => Some(self.dojo_fold(false)),
            Command::DojoExpand => Some(self.dojo_fold(true)),
            Command::DojoToggleHints => {
                self.dojo.show_hints = !self.dojo.show_hints;
                Some(true)
            }
            Command::DojoLanguage => Some(self.dojo_language_picker()),
            Command::DojoChooseFolder => Some(self.dojo_choose_folder_and_open()),
            Command::DojoOpenNotebook => Some(self.dojo_open_notebook()),
            Command::DojoScrollDown => {
                self.dojo.scroll = self.dojo.scroll.saturating_add(8);
                Some(true)
            }
            Command::DojoScrollUp => {
                self.dojo.scroll = self.dojo.scroll.saturating_sub(8);
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

    // ── Workspace ─────────────────────────────────────────────────────────

    /// `g o`: make sure the LeetCode workspace is the open project, then show
    /// the tree + Problem tab. No workspace yet → folder dialog first.
    pub(in crate::app::event_loop) fn dojo_open(&mut self) -> bool {
        self.dojo.open_after_switch = false;
        let Some(ws) = self.dojo.workspace() else {
            return self.dojo_choose_folder_and_open();
        };
        let already_open = self
            .app_state
            .workspace_root_path()
            .is_some_and(|root| crate::app::app_state::path_matches(root, &ws));
        if already_open {
            return self.dojo_show_panels();
        }
        if let Err(err) = std::fs::create_dir_all(&ws) {
            self.show_transient_toast_kind(
                format!("Dojo\nCannot create {}: {err}", ws.display()),
                ToastKind::Error,
            );
            return false;
        }
        self.dojo.open_after_switch = true;
        // May defer behind the dirty-buffer confirmation; the switch tail
        // calls `dojo_after_workspace_switch`.
        self.switch_workspace_with_files(ws, Vec::new())
    }

    /// Tail of `perform_workspace_switch`: finish a `g o` that had to switch.
    pub(in crate::app::event_loop) fn dojo_after_workspace_switch(&mut self, root: &Path) {
        if !self.dojo.open_after_switch {
            return;
        }
        self.dojo.open_after_switch = false;
        if self
            .dojo
            .workspace()
            .is_some_and(|ws| crate::app::app_state::path_matches(root, &ws))
        {
            self.dojo_show_panels();
        }
    }

    fn dojo_show_panels(&mut self) -> bool {
        let _ = self.release_focus_mode_to_editor();
        self.dojo_resume_if_needed();
        self.panel_state.left.visible = true;
        self.panel_state.left.switch_to_tab(PanelTabId::Dojo);
        self.panel_state.right.visible = true;
        self.panel_state.right.switch_to_tab(PanelTabId::Problem);
        if self.focus_manager.set(FocusTarget::LeftSidebar) {
            self.input_handler.clear_pending_prefix();
        }
        let _ = self.dismiss_initial_launch_welcome_if_active();
        self.dojo.clamp_selection();
        // Land on something useful the first time: the running session's
        // problem, else the suggested next problem.
        let target = match self.dojo.state.active_session.as_ref() {
            Some(s) if s.kind == SessionKind::Dsa => Some(s.slug.clone()),
            _ => match self.dojo.selected_row() {
                Some(TreeRow::Problem { .. }) | Some(TreeRow::SdCase { .. }) => None,
                _ => view::suggested_next(&self.dojo.problems, &self.dojo.state, today_local())
                    .map(|r| r.key().to_string()),
            },
        };
        if let Some(key) = target {
            let _ = self.dojo.select_key(&key);
        }
        self.dojo_selection_changed();
        self.sidebar_needs_layout = true;
        self.editor_needs_layout = true;
        true
    }

    #[cfg(not(test))]
    fn dojo_pick_folder_dialog() -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("Choose the folder for your LeetCode solutions")
            .pick_folder()
    }

    #[cfg(test)]
    fn dojo_pick_folder_dialog() -> Option<PathBuf> {
        None
    }

    /// `w` / "Dojo: Choose Folder": system folder dialog, then open the Dojo there.
    pub(in crate::app::event_loop) fn dojo_choose_folder_and_open(&mut self) -> bool {
        let Some(dir) = Self::dojo_pick_folder_dialog() else {
            self.show_transient_toast_kind(
                "Dojo\nPick a folder to keep your solutions in (Cmd+P → Dojo: Choose Folder).",
                ToastKind::Warning,
            );
            return false;
        };
        self.dojo.state.workspace = Some(dir.to_string_lossy().to_string());
        self.dojo.save();
        self.show_transient_toast_kind(
            format!("Dojo\nWorkspace: {}", dir.display()),
            ToastKind::Success,
        );
        self.dojo_open()
    }

    // ── Tree navigation ───────────────────────────────────────────────────

    fn dojo_move_selection(&mut self, delta: i32) -> bool {
        let len = self.dojo.rows().len();
        if len == 0 {
            return false;
        }
        let next = (self.dojo.selected as i64 + i64::from(delta)).clamp(0, len as i64 - 1) as usize;
        if next == self.dojo.selected {
            return false;
        }
        self.dojo.selected = next;
        self.dojo_selection_changed();
        true
    }

    /// Selection landed somewhere new: reset the statement view, keep the row
    /// on screen, and arm the debounced preview fetch.
    pub(in crate::app::event_loop) fn dojo_selection_changed(&mut self) {
        self.dojo.scroll = 0;
        self.dojo.show_hints = false;
        let visible = self.explorer_page_rows().max(1);
        if self.dojo.selected < self.dojo.list_scroll {
            self.dojo.list_scroll = self.dojo.selected;
        } else if self.dojo.selected >= self.dojo.list_scroll + visible {
            self.dojo.list_scroll = self.dojo.selected + 1 - visible;
        }
        self.dojo_request_preview();
        self.sidebar_needs_layout = true;
    }

    /// Arm a statement fetch for the selected problem unless it is cached
    /// (or already being fetched).
    fn dojo_request_preview(&mut self) {
        let Some(TreeRow::Problem { slug, .. }) = self.dojo.selected_row() else {
            self.dojo.pending_preview = None;
            return;
        };
        if self.dojo.cached(&slug).is_some()
            || self.dojo.preview_inflight.as_deref() == Some(slug.as_str())
            || self
                .dojo
                .preview_error
                .as_ref()
                .is_some_and(|(s, _)| *s == slug)
        {
            self.dojo.pending_preview = None;
            return;
        }
        self.dojo.pending_preview = Some((slug, Instant::now() + PREVIEW_DEBOUNCE));
    }

    /// `h`/`l`: fold or unfold. `h` on a problem folds its group and selects
    /// the header; `l` on an open group jumps to its first child.
    fn dojo_fold(&mut self, expand: bool) -> bool {
        let Some(row) = self.dojo.selected_row() else {
            return false;
        };
        let key = match (&row, expand) {
            (TreeRow::Group { key, .. }, _) => key.clone(),
            (TreeRow::SdGroup { .. }, _) => view::SD_GROUP_KEY.to_string(),
            (TreeRow::Problem { slug, .. }, false) => self
                .dojo
                .problems
                .by_slug(slug)
                .map(|p| p.category.clone())
                .unwrap_or_default(),
            (TreeRow::SdCase { .. }, false) => view::SD_GROUP_KEY.to_string(),
            (_, true) => return false,
        };
        if self.dojo.redo_only {
            return false;
        }
        let changed = self.dojo.toggle_group(&key, expand);
        if changed {
            self.dojo.save();
        }
        if !expand {
            // Land on the header so the cursor doesn't fall into another group.
            if let Some(idx) = self
                .dojo
                .rows()
                .iter()
                .position(|r| r.group_key() == Some(key.as_str()))
            {
                self.dojo.selected = idx;
            }
        } else if !changed && row.is_group() {
            self.dojo.selected =
                (self.dojo.selected + 1).min(self.dojo.rows().len().saturating_sub(1));
        }
        self.dojo_selection_changed();
        true
    }

    /// Left-dock rows for the renderer (Explorer row shape; no filter bar).
    pub(in crate::app::event_loop) fn dojo_sidebar_rows(&self) -> Vec<SidebarRow> {
        let theme = &self.theme;
        let fg_dim = theme.ui.fg_dim.as_f32();
        let fg_ghost = theme.ui.fg_ghost.as_f32();
        let success = theme.ui.success.as_f32();
        let warning = theme.ui.warning.as_f32();
        let error = theme.ui.error.as_f32();
        let status_color = |g: RowGlyph| match g {
            RowGlyph::Done => success,
            RowGlyph::RedoDue => warning,
            RowGlyph::RedoLater => fg_ghost,
            RowGlyph::Todo => fg_dim,
        };
        let base = SidebarRow {
            path: None,
            depth: 0,
            arrow: String::new(),
            nerd_icon: String::new(),
            icon_color: fg_dim,
            label: String::new(),
            prefix_marker: None,
            prefix_color: None,
            git_marker: None,
            git_color: None,
            is_selected: false,
            is_dim: false,
        };
        let rows = self.dojo.rows();
        if rows.is_empty() {
            return vec![SidebarRow {
                label: if self.dojo.redo_only {
                    "(no redos due)".to_string()
                } else {
                    "(no problems)".to_string()
                },
                ..base
            }];
        }
        rows.iter()
            .enumerate()
            .skip(self.dojo.list_scroll)
            .map(|(idx, row)| {
                let is_selected = idx == self.dojo.selected;
                match row {
                    TreeRow::Group {
                        key,
                        label,
                        done,
                        total,
                        expanded,
                    } => {
                        let icon = theme.icon_theme_for_path(Path::new(key), true, *expanded);
                        SidebarRow {
                            arrow: theme.sidebar_arrow(true, *expanded).to_string(),
                            nerd_icon: icon.glyph.clone(),
                            icon_color: icon.color.as_f32(),
                            label: format!("{label}  {done}/{total}"),
                            is_selected,
                            ..base.clone()
                        }
                    }
                    TreeRow::SdGroup {
                        done,
                        total,
                        expanded,
                    } => {
                        let icon = theme.icon_theme_for_path(Path::new("sd"), true, *expanded);
                        SidebarRow {
                            arrow: theme.sidebar_arrow(true, *expanded).to_string(),
                            nerd_icon: icon.glyph.clone(),
                            icon_color: icon.color.as_f32(),
                            label: format!("System Design  {done}/{total}"),
                            is_selected,
                            ..base.clone()
                        }
                    }
                    TreeRow::Problem {
                        id,
                        title,
                        difficulty,
                        glyph,
                        trailing,
                        ..
                    } => {
                        let letter = difficulty_letter(difficulty);
                        let label = if trailing.is_empty() {
                            format!("{id}. {title}")
                        } else {
                            format!("{id}. {title}  {trailing}")
                        };
                        SidebarRow {
                            depth: 1,
                            arrow: theme.sidebar_arrow(false, false).to_string(),
                            nerd_icon: glyph.symbol().to_string(),
                            icon_color: status_color(*glyph),
                            label,
                            prefix_marker: Some(letter.to_string()),
                            prefix_color: Some(match letter {
                                'E' => success,
                                'H' => error,
                                _ => warning,
                            }),
                            is_selected,
                            is_dim: *glyph == RowGlyph::Done,
                            ..base.clone()
                        }
                    }
                    TreeRow::SdCase { label, done, .. } => SidebarRow {
                        depth: 1,
                        arrow: theme.sidebar_arrow(false, false).to_string(),
                        nerd_icon: if *done { "●" } else { "○" }.to_string(),
                        icon_color: if *done { success } else { fg_dim },
                        label: label.clone(),
                        is_selected,
                        is_dim: *done,
                        ..base.clone()
                    },
                }
            })
            .collect()
    }

    /// Click on a left-dock Dojo row: select it; a header also toggles its fold.
    pub(in crate::app::event_loop) fn dojo_click_row(&mut self, row_index: usize) -> bool {
        let idx = row_index + self.dojo.list_scroll;
        let rows = self.dojo.rows();
        if idx >= rows.len() {
            return false;
        }
        self.dojo.selected = idx;
        if self.focus_manager.set(FocusTarget::LeftSidebar) {
            self.input_handler.clear_pending_prefix();
        }
        if rows[idx].is_group() {
            let key = rows[idx].group_key().unwrap_or_default().to_string();
            let expanded = matches!(
                rows[idx],
                TreeRow::Group { expanded: true, .. } | TreeRow::SdGroup { expanded: true, .. }
            );
            if self.dojo.toggle_group(&key, !expanded) {
                self.dojo.save();
            }
        }
        self.dojo_selection_changed();
        true
    }

    // ── Problem tab model ─────────────────────────────────────────────────

    /// One frame of the right-dock Problem tab. While a DSA session runs its
    /// problem stays on screen regardless of the tree selection.
    pub(in crate::app::event_loop) fn dojo_problem_model(
        &mut self,
        focused: bool,
    ) -> ProblemPanelModel {
        let today = today_local();
        let session = self
            .dojo
            .state
            .active_session
            .clone()
            .filter(|_| self.dojo.armed);
        let session_view = session.as_ref().map(|s| {
            let now = now_unix();
            let phases = self.dojo.session_phases(s.kind);
            let phase = phase_at(&phases, s.elapsed_s(now));
            DojoSessionView {
                title: s.title.clone(),
                phase: phase
                    .as_ref()
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "TIME'S UP".to_string()),
                phase_index: phase.as_ref().map(|p| p.index).unwrap_or(usize::MAX),
                remaining: mm_ss(s.remaining_s(now)),
                remaining_s: s.remaining_s(now),
                kind: s.kind,
                expired: s.is_expired(now),
            }
        });
        let shown = match &session {
            Some(s) if s.kind == SessionKind::Dsa => Some(TreeRow::Problem {
                slug: s.slug.clone(),
                id: 0,
                title: String::new(),
                difficulty: String::new(),
                glyph: RowGlyph::Todo,
                trailing: String::new(),
            }),
            _ => self.dojo.selected_row(),
        };
        let language = self.dojo.language_label();
        let content = match shown {
            Some(TreeRow::Problem { slug, .. }) => {
                match self.dojo.problems.by_slug(&slug).cloned() {
                    Some(p) => {
                        let loading = self.dojo.preview_inflight.as_deref() == Some(slug.as_str())
                            || self
                                .dojo
                                .pending_preview
                                .as_ref()
                                .is_some_and(|(s, _)| *s == slug);
                        let error = self
                            .dojo
                            .preview_error
                            .as_ref()
                            .filter(|(s, _)| *s == slug)
                            .map(|(_, m)| m.clone());
                        let cache = self.dojo.cached(&slug).cloned();
                        PanelContent::Problem(view::problem_view(
                            &p,
                            &self.dojo.state,
                            cache.as_ref(),
                            &language,
                            loading,
                            error,
                            today,
                        ))
                    }
                    None => PanelContent::Empty(format!("Unknown problem {slug}")),
                }
            }
            Some(TreeRow::SdCase { key, label, done }) => PanelContent::Sd(SdView {
                topic: self
                    .dojo
                    .plan
                    .sd_case(&key)
                    .map(|c| c.topic.clone())
                    .unwrap_or_default(),
                key,
                label,
                done,
            }),
            Some(TreeRow::Group { label, .. }) => {
                PanelContent::Empty(format!("{label} — pick a problem (j/k), Enter to start"))
            }
            Some(TreeRow::SdGroup { .. }) => {
                PanelContent::Empty("System Design — pick a case, Enter to start".to_string())
            }
            None => PanelContent::Empty("Press g o to open the Dojo".to_string()),
        };
        ProblemPanelModel {
            header: self.dojo.header(),
            content,
            session: session_view,
            show_hints: self.dojo.show_hints,
            scroll: self.dojo.scroll,
            focused,
        }
    }

    // ── Language ──────────────────────────────────────────────────────────

    /// `c` / "Dojo: Language": MRU-sorted picker; the choice is remembered.
    pub(in crate::app::event_loop) fn dojo_language_picker(&mut self) -> bool {
        let recent = &self.persistent_state.recent_leetcode_languages;
        let current = self.dojo.language_key().map(str::to_string);
        let mut templates: Vec<&crate::runner::leetcode::LeetCodeTemplate> =
            crate::runner::leetcode::leetcode_templates()
                .iter()
                .collect();
        templates.sort_by_key(|t| {
            if current.as_deref() == Some(t.key) {
                0
            } else {
                1 + recent
                    .iter()
                    .position(|k| k == t.key)
                    .unwrap_or(usize::MAX - 1)
            }
        });
        let items: Vec<CommandPaletteItem> = templates
            .into_iter()
            .map(|t| CommandPaletteItem::leetcode_language(t.key, t.label, t.hint))
            .collect();
        let current_mode = self.app_state.current_mode();
        if current_mode != EditorMode::PaletteFocus
            && !self.app_state.can_apply_mode_event(ModeEvent::OpenPalette)
        {
            return false;
        }
        self.app_state.open_dojo_language_selector_with_items(items);
        if current_mode != EditorMode::PaletteFocus
            && let Err(err) = self.app_state.apply_mode_event(ModeEvent::OpenPalette)
        {
            let _ = self.app_state.close_command_palette();
            eprintln!("[AppShell] dojo language picker mode change failed: {err:?}");
            return false;
        }
        self.arm_palette_ime_commit_suppression();
        if self.focus_manager.set(FocusTarget::OverlayLayer) {
            self.input_handler.clear_pending_prefix();
        }
        true
    }

    /// Picker confirmed: remember the language, then resume a pending start.
    pub(in crate::app::event_loop) fn confirm_dojo_language(&mut self) -> bool {
        let Some(CommandPaletteAction::CreateLeetCodeFile(key)) =
            self.app_state.command_palette_selected_action()
        else {
            return false;
        };
        let _ = self.app_state.close_command_palette();
        if self.app_state.current_mode() == EditorMode::PaletteFocus
            && let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus)
        {
            let _ = result;
        }
        self.persistent_state.push_recent_leetcode_language(&key);
        self.persistent_state.save();
        self.dojo.state.language = Some(key);
        self.dojo.save();
        self.focus_manager.set(FocusTarget::LeftSidebar);
        self.input_handler.clear_pending_prefix();
        self.show_transient_toast_kind(
            format!("Dojo\nLanguage: {}", self.dojo.language_label()),
            ToastKind::Success,
        );
        if let Some(slug) = self.dojo.pending_start.take() {
            let _ = self.dojo.select_key(&slug);
            return self.dojo_start_selected();
        }
        true
    }

    // ── Sessions ──────────────────────────────────────────────────────────

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

    /// (id, plain statement) for a slug, from the per-problem cache.
    pub(in crate::app::event_loop) fn dojo_problem_context(&mut self, slug: &str) -> (u32, String) {
        let Some(id) = self.dojo.problems.by_slug(slug).map(|p| p.id) else {
            return (0, String::new());
        };
        let text = self
            .dojo
            .cached(slug)
            .map(|c| html_to_text(&c.statement))
            .unwrap_or_default();
        (id, text)
    }

    /// Enter: fold a header, start an SD case, or start/resume a problem.
    pub(in crate::app::event_loop) fn dojo_start_selected(&mut self) -> bool {
        let Some(row) = self.dojo.selected_row() else {
            return false;
        };
        if let Some(session) = self
            .dojo
            .state
            .active_session
            .clone()
            .filter(|_| self.dojo.armed)
        {
            let same = row.key() == session.slug;
            if same || row.is_group() {
                self.dojo_open_session_file(&session.file.clone());
                self.focus_manager.set(FocusTarget::CenterEditor);
                self.input_handler.clear_pending_prefix();
                self.editor_needs_layout = true;
                return true;
            }
            self.show_transient_toast_kind(
                format!(
                    "Dojo\nFinish {} first (F5 all green, or x to give up).",
                    session.title
                ),
                ToastKind::Warning,
            );
            return false;
        }
        match row {
            TreeRow::Group { .. } | TreeRow::SdGroup { .. } => {
                let expanded = matches!(
                    row,
                    TreeRow::Group { expanded: true, .. } | TreeRow::SdGroup { expanded: true, .. }
                );
                self.dojo_fold(!expanded)
            }
            TreeRow::SdCase { key, label, .. } => self.dojo_begin_sd_session(&key, &label),
            TreeRow::Problem {
                slug, id, title, ..
            } => {
                let Some(language) = self.dojo.language_key().map(str::to_string) else {
                    self.dojo.pending_start = Some(slug);
                    return self.dojo_language_picker();
                };
                let Some(ws) = self.dojo.workspace() else {
                    return self.dojo_choose_folder_and_open();
                };
                let Some(template) = crate::runner::leetcode::leetcode_template(&language) else {
                    self.dojo.state.language = None;
                    self.dojo.pending_start = Some(slug);
                    return self.dojo_language_picker();
                };
                let dir = problem_dir(&ws, id, &slug);
                let file = dir.join(format!("solution.{}", template.extension));
                if file.exists() {
                    // Redo: keep the user's file, reload the example cases.
                    self.dojo_load_cached_cases(&slug);
                    self.dojo_begin_dsa_session(slug, title, file);
                    return true;
                }
                self.dojo.pending_start = Some(slug.clone());
                self.submit_leetcode_fetch_to(slug, language, dir);
                self.show_transient_toast_kind(
                    format!("Dojo\nFetching #{id} {title}…"),
                    ToastKind::Info,
                );
                true
            }
        }
    }

    fn dojo_load_cached_cases(&mut self, slug: &str) {
        let cases: Vec<crate::runner::TestCase> = self
            .dojo
            .cached(slug)
            .map(|c| {
                c.cases
                    .iter()
                    .map(|case| {
                        crate::runner::TestCase::new(case.input.clone(), case.expected.clone())
                    })
                    .collect()
            })
            .unwrap_or_default();
        let runner = &mut self.app_state.test_runner;
        runner.cases = cases;
        runner.selected = (!runner.cases.is_empty()).then_some(0);
        runner.focused_field = crate::runner::TestField::Input;
        runner.is_running = false;
        runner.launch_error = None;
    }

    /// Fetch landed (or an existing file was reopened): start the clock, open
    /// the file in the center, keep the statement on the right.
    pub(in crate::app::event_loop) fn dojo_begin_dsa_session(
        &mut self,
        slug: String,
        title: String,
        file: PathBuf,
    ) {
        self.dojo.pending_start = None;
        self.dojo.state.active_session = Some(ActiveSession {
            kind: SessionKind::Dsa,
            slug: slug.clone(),
            title,
            started_unix: now_unix(),
            budget_s: self.dojo.plan.dsa_budget_s(),
            file: file.clone(),
        });
        self.dojo.armed = true;
        self.dojo.last_phase = Some(0);
        self.dojo.last_tick_second = now_unix();
        self.dojo.invalidate_cache();
        self.dojo.save();
        self.dojo_ensure_interviewer_prompt();
        self.dojo_write_current_md();
        let _ = self.dojo.select_key(&slug);
        self.dojo.scroll = 0;
        self.dojo_open_session_file(&file);
        self.panel_state.left.visible = true;
        self.panel_state.left.switch_to_tab(PanelTabId::Dojo);
        self.panel_state.right.visible = true;
        self.panel_state.right.switch_to_tab(PanelTabId::Problem);
        self.focus_manager.set(FocusTarget::CenterEditor);
        self.input_handler.clear_pending_prefix();
        self.sidebar_needs_layout = true;
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        self.show_transient_toast_kind(
            format!(
                "{} min on the clock\nF5 runs the example cases — all green = solved. x gives up.",
                self.dojo.plan.dsa_minutes
            ),
            ToastKind::Success,
        );
    }

    /// System-design session: outline file from the 45' framework template,
    /// single-phase clock. `x` finishes it.
    pub(in crate::app::event_loop) fn dojo_begin_sd_session(
        &mut self,
        key: &str,
        label: &str,
    ) -> bool {
        let Some(ws) = self.dojo.workspace() else {
            return self.dojo_choose_folder_and_open();
        };
        let dir = sd_dir(&self.dojo.plan, &ws);
        let path = dir.join(format!("{key}.md"));
        if !path.exists() {
            // ponytail: tiny file written sync so OpenFile below sees it
            // (state.toml precedent); a worker write would race the open.
            let written = std::fs::create_dir_all(&dir)
                .and_then(|_| std::fs::write(&path, sd_template(label, &date_str(today_local()))));
            if let Err(err) = written {
                self.show_transient_toast_kind(
                    format!("Dojo\nCannot create {}: {err}", path.display()),
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
            file: path.clone(),
        });
        self.dojo.armed = true;
        self.dojo.last_phase = Some(0);
        self.dojo.last_tick_second = now_unix();
        self.dojo.save();
        self.dojo_ensure_interviewer_prompt();
        self.dojo_write_current_md();
        self.dojo_open_session_file(&path);
        self.focus_manager.set(FocusTarget::CenterEditor);
        self.input_handler.clear_pending_prefix();
        self.editor_needs_layout = true;
        self.show_transient_toast_kind(
            format!(
                "System design · {} min\n1 Requirements 5' → 2 Scale 5' → 3 API 5' → 4 Design 10' → 5 Deep dive 15' → 6 Trade-offs 5'",
                self.dojo.plan.sd_minutes
            ),
            ToastKind::Info,
        );
        true
    }

    /// `n`: open the notebook (created with its header when missing).
    pub(in crate::app::event_loop) fn dojo_open_notebook(&mut self) -> bool {
        let Some(ws) = self.dojo.workspace() else {
            return self.dojo_choose_folder_and_open();
        };
        let path = notebook_path(&self.dojo.plan, &ws);
        if !path.exists() {
            let written = path
                .parent()
                .map(std::fs::create_dir_all)
                .unwrap_or(Ok(()))
                .and_then(|_| std::fs::write(&path, NOTEBOOK_HEADER));
            if let Err(err) = written {
                self.show_transient_toast_kind(
                    format!("Dojo\nCannot create {}: {err}", path.display()),
                    ToastKind::Error,
                );
                return false;
            }
        }
        self.dojo_open_session_file(&path);
        self.focus_manager.set(FocusTarget::CenterEditor);
        self.input_handler.clear_pending_prefix();
        self.editor_needs_layout = true;
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
            self.dojo_write_current_md();
        } else if let Some(row) = self.dojo.selected_row() {
            let (kind, key, title) = match &row {
                TreeRow::Problem { slug, title, .. } => {
                    (SessionKind::Dsa, slug.clone(), title.clone())
                }
                TreeRow::SdCase { key, label, .. } => (SessionKind::Sd, key.clone(), label.clone()),
                _ => return false,
            };
            let (id, statement) = match kind {
                SessionKind::Dsa => self.dojo_problem_context(&key),
                SessionKind::Sd => (0, String::new()),
            };
            let phases = self.dojo.session_phases(kind);
            let language = match kind {
                SessionKind::Dsa => self.dojo.language_key().unwrap_or("").to_string(),
                SessionKind::Sd => String::new(),
            };
            let text = current_md(kind, id, &title, &statement, &language, &phases);
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
        self.show_transient_toast(
            "Interviewer\nOpening claude… state your approach before coding.",
        );
        true
    }

    /// Open a session file with the same post-open plumbing the fetch handler uses.
    pub(in crate::app::event_loop) fn dojo_open_session_file(&mut self, file: &Path) {
        let report = dispatch_command(&mut self.app_state, Command::OpenFile(file.to_path_buf()));
        if !report.success {
            self.show_transient_toast_kind(
                format!("Dojo\nCannot open {}", file.display()),
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
    pub(in crate::app::event_loop) fn dojo_write_current_md(&mut self) {
        let Some(s) = self.dojo.state.active_session.clone() else {
            return;
        };
        let (id, statement) = match s.kind {
            SessionKind::Dsa => self.dojo_problem_context(&s.slug),
            SessionKind::Sd => (0, String::new()),
        };
        let phases = self.dojo.session_phases(s.kind);
        let language = match s.kind {
            SessionKind::Dsa => self.dojo.language_key().unwrap_or("").to_string(),
            SessionKind::Sd => String::new(),
        };
        let text = current_md(s.kind, id, &s.title, &statement, &language, &phases);
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
    /// An expired one ends as `timeout` on the next tick.
    fn dojo_resume_if_needed(&mut self) {
        if self.dojo.armed {
            return;
        }
        let Some(session) = self.dojo.state.active_session.clone() else {
            return;
        };
        self.dojo.armed = true;
        self.dojo.last_phase = None;
        if session.file.exists() {
            if session.kind == SessionKind::Dsa {
                self.dojo_load_cached_cases(&session.slug);
            }
            self.dojo_open_session_file(&session.file);
        }
    }

    // ── Clock + preview ───────────────────────────────────────────────────

    /// Once per event-loop turn. Returns true when the panels must redraw.
    pub(in crate::app::event_loop) fn dojo_tick(&mut self) -> bool {
        let mut changed = false;
        if let Some((slug, due)) = self.dojo.pending_preview.clone()
            && Instant::now() >= due
            && self.dojo.preview_inflight.is_none()
        {
            self.dojo.pending_preview = None;
            self.dojo.preview_inflight = Some(slug.clone());
            let _ = self.submit(RequestSpec {
                revision_id: 0,
                topic: RequestTopic::LeetCode,
                payload: WorkerRequestPayload::FetchLeetCodeStatement { slug },
            });
            changed = true;
        }
        let Some(session) = self
            .dojo
            .state
            .active_session
            .clone()
            .filter(|_| self.dojo.armed)
        else {
            return changed;
        };
        let now = now_unix();
        if session.is_expired(now) {
            self.dojo_end_session(Outcome::Timeout);
            return true;
        }
        let phases = self.dojo.session_phases(session.kind);
        if let Some(phase) = phase_at(&phases, session.elapsed_s(now))
            && self.dojo.last_phase != Some(phase.index)
        {
            self.dojo.last_phase = Some(phase.index);
            self.dojo.last_tick_second = now;
            let minutes = phases.get(phase.index).map(|p| p.1).unwrap_or(0);
            let hint = match phase.name.as_str() {
                "THINK" => "Read the statement, say the approach + complexity out loud.",
                "CODE" => "Type it out. F5 runs the cases.",
                "TEST" => "Edge cases: empty, one element, duplicates, negatives, overflow.",
                "REVIEW" => "Compare with the optimal solution; note the gap in the notebook.",
                _ => "",
            };
            self.show_transient_toast_kind(
                format!("{} · {minutes} min\n{hint}", phase.name),
                ToastKind::Info,
            );
            return true;
        }
        if self.dojo.last_tick_second != now {
            self.dojo.last_tick_second = now;
            return true;
        }
        changed
    }

    /// Statement-only fetch landed (or failed) for the preview.
    pub(in crate::app::event_loop) fn dojo_on_statement_fetched(
        &mut self,
        slug: &str,
        error: Option<String>,
    ) {
        if self.dojo.preview_inflight.as_deref() == Some(slug) {
            self.dojo.preview_inflight = None;
        }
        self.dojo.invalidate_cache();
        self.dojo.preview_error = error.map(|m| (slug.to_string(), m));
        // The selection may have moved on while the fetch ran.
        self.dojo_request_preview();
        self.request_redraw();
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
            self.show_transient_toast("Dojo\nNo session running.");
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

    /// Record the attempt, apply spaced redo, toast the summary, and append a
    /// notebook stub for the user to fill in (`n` opens it).
    pub(in crate::app::event_loop) fn dojo_end_session(&mut self, outcome: Outcome) {
        let Some(session) = self.dojo.state.active_session.take() else {
            return;
        };
        let now = now_unix();
        let elapsed_s = session.elapsed_s(now).min(session.budget_s);
        let today = today_local();
        self.dojo.state.record_attempt(
            Attempt {
                slug: session.slug.clone(),
                kind: session.kind,
                started_unix: session.started_unix,
                ended_unix: now,
                outcome,
                elapsed_s,
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
                    "{} · solved in {} · streak {streak}",
                    session.title,
                    mm_ss(elapsed_s)
                ),
                ToastKind::Success,
            ),
            _ => (
                format!(
                    "{} · {} at {} · redo on {redo_at}",
                    session.title,
                    outcome.label(),
                    mm_ss(elapsed_s)
                ),
                ToastKind::Warning,
            ),
        };
        self.show_transient_toast_kind(summary, kind);

        let contents = match session.kind {
            SessionKind::Sd => format_sd_block(&date_str(today), &session.title, elapsed_s),
            SessionKind::Dsa => format_block(
                &date_str(today),
                id,
                &session.title,
                outcome,
                elapsed_s,
                redo,
            ),
        };
        if let Some(ws) = self.dojo.workspace() {
            self.submit_text_file_ops(vec![TextFileOp::Append {
                path: notebook_path(&self.dojo.plan, &ws),
                header: NOTEBOOK_HEADER.to_string(),
                contents,
            }]);
        }
        self.sidebar_needs_layout = true;
    }
}
