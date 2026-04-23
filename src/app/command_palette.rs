use std::path::{Path, PathBuf};

use crate::{config::theme_config::ThemeConfig, workspace::model::WorkspaceModel};

const DEFAULT_MAX_RESULTS: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandPaletteMode {
    /// File Picker (Space f f) — Box 800px, top-center, badges theo ext
    FilePicker,
    /// Command Palette (Cmd+P) — Box 500px, screen center, minimalist NvChad style
    CommandPalette,
    /// Vim Command (:) — Box 500px, compact
    VimCommand,
    /// Workspace Symbols (@ prefix)
    WorkspaceSymbols,
    /// Live Grep (Space f w) — Box 800px giống File Picker, prompt `grep> `
    LiveGrep,
    /// In-file search (`/`) — compact prompt, live highlights in the active buffer.
    InFileSearch,
    /// Explorer prompt để tạo file mới tại node đang chọn.
    ExplorerCreateFile,
    /// Explorer prompt để tạo folder mới tại node đang chọn.
    ExplorerCreateFolder,
    /// Explorer delete confirmation overlay.
    ExplorerDeleteConfirm,
    /// Dirty buffer close confirmation overlay.
    BufferCloseConfirm,
    /// Recent Projects picker — hiện danh sách project đã mở gần đây.
    RecentProjects,
    /// Theme selector — liệt kê `config/themes/*.toml` để hot-reload runtime.
    ThemeSelector,
    /// LSP References — danh sách tĩnh kết quả `gr` từ LSP server.
    LspReferences,
}

impl CommandPaletteMode {
    pub fn prompt_prefix(self) -> &'static str {
        match self {
            Self::FilePicker => "find> ",
            Self::CommandPalette => "> ",
            Self::VimCommand => ":",
            Self::WorkspaceSymbols => "@ ",
            Self::LiveGrep => "grep> ",
            Self::InFileSearch => "/",
            Self::ExplorerCreateFile => "file> ",
            Self::ExplorerCreateFolder => "dir> ",
            Self::ExplorerDeleteConfirm => "delete> ",
            Self::BufferCloseConfirm => "close> ",
            Self::RecentProjects => "project> ",
            Self::ThemeSelector => "Select Theme> ",
            Self::LspReferences => "refs> ",
        }
    }

    pub fn empty_hint(self) -> &'static str {
        match self {
            Self::FilePicker => "type to search files...",
            Self::CommandPalette => "type a command...",
            Self::VimCommand => "type a vim command...",
            Self::WorkspaceSymbols => "type to search symbols...",
            Self::LiveGrep => "type to grep workspace...",
            Self::InFileSearch => "type to search in current file...",
            Self::ExplorerCreateFile => "enter a new file path...",
            Self::ExplorerCreateFolder => "enter a new folder path...",
            Self::ExplorerDeleteConfirm => "Delete selected item? (y/n)",
            Self::BufferCloseConfirm => "Save changes before closing? (y/n)",
            Self::RecentProjects => "no recent projects",
            Self::ThemeSelector => "type to filter themes...",
            Self::LspReferences => "no references found",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::FilePicker => "FIND",
            Self::CommandPalette => "COMMANDS",
            Self::VimCommand => "VIM",
            Self::WorkspaceSymbols => "SYMBOLS",
            Self::LiveGrep => "GREP",
            Self::InFileSearch => "SEARCH",
            Self::ExplorerCreateFile => "NEW FILE",
            Self::ExplorerCreateFolder => "NEW FOLDER",
            Self::ExplorerDeleteConfirm => "DELETE",
            Self::BufferCloseConfirm => "CLOSE",
            Self::RecentProjects => "RECENT",
            Self::ThemeSelector => "THEMES",
            Self::LspReferences => "REFS",
        }
    }

    /// File Picker, Live Grep, Recent Projects và Theme Selector dùng UI phức tạp.
    pub fn is_complex_picker(self) -> bool {
        matches!(
            self,
            Self::FilePicker
                | Self::LiveGrep
                | Self::RecentProjects
                | Self::ThemeSelector
                | Self::LspReferences
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandPaletteAction {
    OpenFile(PathBuf),
    OpenSearchMatch {
        path: PathBuf,
        line: u32,
        column: u32,
    },
    ExecuteCommand(String),
    ExecuteVimCommand(String),
    SelectTheme(String),
    JumpToSymbol(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPaletteItem {
    pub label: String,
    pub secondary_label: Option<String>,
    pub action: CommandPaletteAction,
}

impl CommandPaletteItem {
    pub fn file_match(relative_path: String, absolute_path: PathBuf) -> Self {
        Self {
            label: relative_path,
            secondary_label: None,
            action: CommandPaletteAction::OpenFile(absolute_path),
        }
    }

    pub fn recent_project(path: &std::path::Path) -> Self {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        Self {
            label: name,
            secondary_label: None,
            action: CommandPaletteAction::OpenFile(path.to_path_buf()),
        }
    }

    pub fn search_match(
        label: String,
        secondary_label: Option<String>,
        path: PathBuf,
        line: u32,
        column: u32,
    ) -> Self {
        Self {
            label,
            secondary_label,
            action: CommandPaletteAction::OpenSearchMatch { path, line, column },
        }
    }

    pub fn command(id: &str, label: &str) -> Self {
        Self {
            label: label.to_string(),
            secondary_label: None,
            action: CommandPaletteAction::ExecuteCommand(id.to_string()),
        }
    }

    pub fn theme(name: &str, path: &Path) -> Self {
        Self {
            label: name.to_string(),
            secondary_label: Some(path.display().to_string()),
            action: CommandPaletteAction::SelectTheme(name.to_string()),
        }
    }

    pub fn symbol(name: &str) -> Self {
        Self {
            label: name.to_string(),
            secondary_label: None,
            action: CommandPaletteAction::JumpToSymbol(name.to_string()),
        }
    }

    pub fn vim_input(query: &str) -> Self {
        let trimmed = query.trim();
        Self {
            label: if trimmed.is_empty() {
                "(empty command)".to_string()
            } else {
                trimmed.to_string()
            },
            secondary_label: None,
            action: CommandPaletteAction::ExecuteVimCommand(trimmed.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandPaletteRenderModel {
    /// Mode hiện tại — quyết định renderer dùng hàm nào
    pub mode: CommandPaletteMode,
    pub overlay_bounds: [f32; 4],
    pub panel_bounds: [f32; 4],
    /// Phần prefix cố định: "find> ", "> ", ":" — hiện với hint_color mờ.
    pub prompt_prefix: String,
    /// Phần query user đang gõ — hiện với text_color sáng.
    pub prompt_query: String,
    /// Label hiển thị cho mỗi kết quả (filename hoặc folder name).
    pub result_labels: Vec<String>,
    /// Secondary label cho mỗi kết quả (full path) — dùng cho 2-column modes như RecentProjects.
    pub secondary_labels: Vec<String>,
    /// Match byte ranges (start..end) trong label tương ứng.
    pub result_match_ranges: Vec<Vec<(usize, usize)>>,
    pub selected_index: usize,
    pub scroll_offset_rows: usize,
    pub line_height: f32,
    pub panel_padding: f32,
    /// Tiêu đề ngắn ("FIND", "COMMANDS"...) — hiện trong badge header của file picker
    pub title: String,
    /// Tổng số kết quả trước khi limit — dùng cho counter "8/847"
    pub total_results: usize,
    /// Nếu false, renderer không render danh sách kết quả (VimCommand mode — 1 dòng input thôi)
    pub show_results: bool,
    pub border_color: [f32; 4],
    pub panel_bg: [f32; 4],
    pub selection_bg: [f32; 4],
    /// Màu cho query text + kết quả đang chọn (suy ra từ ui.fg)
    pub text_color: [f32; 4],
    /// Màu mờ cho prefix prompt, title, "(no matches)" (suy ra từ syntax.comment)
    pub hint_color: [f32; 4],
    /// Màu cho matched chars trong labels (suy ra từ ui.accent)
    pub match_color: [f32; 4],
    /// Màu cho label text bình thường — xám nhạt (suy ra từ ui.fg_dim)
    pub label_color: [f32; 4],
    pub scrim_color: [f32; 4],
    /// Màu success (xanh lá) — dùng cho badge "FIND" / "GREP" trong header
    pub success_color: [f32; 4],
    /// Màu error/warning — dùng cho ext badges của .rs, .toml ...
    pub warning_color: [f32; 4],
    /// Màu info (xanh dương) — dùng cho ext badges của .ts, .go ...
    pub info_color: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct CommandPalette {
    pub mode: CommandPaletteMode,
    pub query: String,
    pub results: Vec<CommandPaletteItem>,
    pub selected_index: usize,
    pub is_visible: bool,
    max_results: usize,
    /// Items pre-populated externally (e.g. recent projects). Used by
    /// RecentProjects mode where `refresh_results` would otherwise clear them.
    static_items: Vec<CommandPaletteItem>,
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self {
            mode: CommandPaletteMode::FilePicker,
            query: String::new(),
            results: Vec::new(),
            selected_index: 0,
            is_visible: false,
            max_results: DEFAULT_MAX_RESULTS,
            static_items: Vec::new(),
        }
    }
}

impl CommandPalette {
    pub fn open(&mut self, mode: CommandPaletteMode, workspace: Option<&WorkspaceModel>) -> usize {
        self.mode = mode;
        self.query.clear();
        self.selected_index = 0;
        self.is_visible = true;
        self.static_items.clear();
        self.refresh_results(workspace);
        self.results.len()
    }

    /// Open the palette with a pre-populated, static item list (e.g. recent projects).
    /// `refresh_results` will not overwrite these items while mode is active.
    pub fn open_with_items(
        &mut self,
        mode: CommandPaletteMode,
        items: Vec<CommandPaletteItem>,
    ) -> usize {
        self.mode = mode;
        self.query.clear();
        self.selected_index = 0;
        self.is_visible = true;
        self.static_items = items.clone();
        self.results = items;
        self.results.len()
    }

    pub fn close(&mut self) -> bool {
        let was_open = self.is_visible;
        self.is_visible = false;
        self.query.clear();
        self.selected_index = 0;
        self.results.clear();
        self.static_items.clear();
        was_open
    }

    pub fn append_query(&mut self, text: &str, workspace: Option<&WorkspaceModel>) -> bool {
        if text.is_empty() || !self.is_visible {
            return false;
        }
        self.query.push_str(text);
        self.selected_index = 0;
        self.refresh_results(workspace);
        true
    }

    pub fn set_query(&mut self, text: &str, workspace: Option<&WorkspaceModel>) -> bool {
        if !self.is_visible {
            return false;
        }
        if self.query == text {
            return false;
        }
        self.query = text.to_string();
        self.selected_index = 0;
        self.refresh_results(workspace);
        true
    }

    pub fn backspace_query(&mut self, workspace: Option<&WorkspaceModel>) -> bool {
        if !self.is_visible || self.query.is_empty() {
            return false;
        }
        self.query.pop();
        self.selected_index = 0;
        self.refresh_results(workspace);
        true
    }

    pub fn select_next(&mut self) -> bool {
        if self.results.is_empty() {
            return false;
        }
        let next = (self.selected_index + 1) % self.results.len();
        let changed = next != self.selected_index;
        self.selected_index = next;
        changed
    }

    pub fn select_prev(&mut self) -> bool {
        if self.results.is_empty() {
            return false;
        }
        let prev = if self.selected_index == 0 {
            self.results.len() - 1
        } else {
            self.selected_index - 1
        };
        let changed = prev != self.selected_index;
        self.selected_index = prev;
        changed
    }

    pub fn selected_action(&self) -> Option<CommandPaletteAction> {
        self.results
            .get(self.selected_index)
            .map(|entry| entry.action.clone())
    }

    pub fn refresh_results(&mut self, _workspace: Option<&WorkspaceModel>) {
        // LspReferences: static list, never overwrite with fzf results.
        if matches!(self.mode, CommandPaletteMode::LspReferences) {
            self.results = self.static_items.clone();
            if self.results.is_empty() {
                self.selected_index = 0;
            } else {
                self.selected_index = self.selected_index.min(self.results.len() - 1);
            }
            return;
        }

        // RecentProjects: filter static_items by query (name OR full path).
        if matches!(
            self.mode,
            CommandPaletteMode::RecentProjects | CommandPaletteMode::ThemeSelector
        ) {
            self.results = if self.query.is_empty() {
                self.static_items.clone()
            } else {
                let q = self.query.to_lowercase();
                self.static_items
                    .iter()
                    .filter(|item| {
                        item.label.to_lowercase().contains(&q)
                            || item
                                .secondary_label
                                .as_deref()
                                .map(|secondary| secondary.to_lowercase().contains(&q))
                                .unwrap_or(false)
                            || matches!(&item.action,
                                CommandPaletteAction::OpenFile(path)
                                    if path.to_string_lossy().to_lowercase().contains(&q))
                    })
                    .cloned()
                    .collect()
            };
            if self.results.is_empty() {
                self.selected_index = 0;
            } else {
                self.selected_index = self.selected_index.min(self.results.len() - 1);
            }
            return;
        }

        self.results = match self.mode {
            CommandPaletteMode::FilePicker => Vec::new(),
            CommandPaletteMode::CommandPalette => {
                command_palette_items(&self.query, self.max_results)
            }
            CommandPaletteMode::VimCommand => vim_command_items(&self.query),
            CommandPaletteMode::WorkspaceSymbols => {
                workspace_symbol_items(&self.query, self.max_results)
            }
            CommandPaletteMode::LiveGrep => Vec::new(),
            CommandPaletteMode::InFileSearch => Vec::new(),
            CommandPaletteMode::ExplorerCreateFile
            | CommandPaletteMode::ExplorerCreateFolder
            | CommandPaletteMode::ExplorerDeleteConfirm
            | CommandPaletteMode::BufferCloseConfirm => Vec::new(),
            CommandPaletteMode::RecentProjects => unreachable!("handled above"),
            CommandPaletteMode::ThemeSelector => unreachable!("handled above"),
            CommandPaletteMode::LspReferences => unreachable!("handled above"),
        };

        if self.results.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(self.results.len() - 1);
        }
    }

    pub fn refresh_if_open(&mut self, workspace: Option<&WorkspaceModel>) -> bool {
        if !self.is_visible {
            return false;
        }
        let old_results = self.results.clone();
        let old_selected = self.selected_action();
        self.refresh_results(workspace);
        old_results != self.results || old_selected != self.selected_action()
    }

    pub fn replace_results(&mut self, results: Vec<CommandPaletteItem>) -> bool {
        let selected_before = self.selected_action();
        let results_before = self.results.clone();
        self.results = results;
        if self.results.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(self.results.len() - 1);
        }

        results_before != self.results || selected_before != self.selected_action()
    }

    pub fn render(
        &self,
        theme: &ThemeConfig,
        overlay_bounds: [f32; 4],
    ) -> Option<CommandPaletteRenderModel> {
        if !self.is_visible {
            return None;
        }

        let [x, y, width, height] = overlay_bounds;
        if width < 1.0 || height < 1.0 {
            return None;
        }

        let panel_padding = 20.0;
        let line_height = theme.ui.sidebar_line_height.max(22.0);
        let row_height = palette_row_height(self.mode, line_height);
        let max_items = palette_max_items(self.mode);
        // VimCommand + Explorer prompts chỉ hiển thị 1 dòng input, không có danh sách kết quả
        let show_results = !matches!(
            self.mode,
            CommandPaletteMode::VimCommand
                | CommandPaletteMode::InFileSearch
                | CommandPaletteMode::ExplorerCreateFile
                | CommandPaletteMode::ExplorerCreateFolder
                | CommandPaletteMode::ExplorerDeleteConfirm
                | CommandPaletteMode::BufferCloseConfirm
        );
        let requested_visible_rows = if show_results {
            self.results.len().min(max_items)
        } else {
            0
        };

        // ── Layout: tách Command Palette vs File Picker/LiveGrep ─────────────
        let (panel_width, panel_x, panel_y, panel_height, visible_result_rows) =
            if self.mode == CommandPaletteMode::LiveGrep {
                let max_w = (width - 48.0).max(320.0);
                let pw = if max_w >= 720.0 {
                    (width * 0.82).clamp(720.0, max_w)
                } else {
                    max_w
                };
                let px = x + ((width - pw) * 0.5).max(0.0);
                let min_height =
                    live_grep_reserved_height(panel_padding, line_height) + row_height * 4.0;
                let max_h = (height - 32.0).max(row_height + panel_padding * 2.0);
                let ph = if max_h >= min_height {
                    (height * 0.52).clamp(min_height, max_h)
                } else {
                    max_h
                };
                let py = y + ((height - ph) * 0.5).max(16.0).min(height - ph - 16.0);
                let body_height =
                    (ph - live_grep_reserved_height(panel_padding, line_height)).max(row_height);
                let visible_result_rows = (body_height / row_height).floor() as usize;
                (pw, px, py, ph, visible_result_rows.max(1))
            } else if self.mode.is_complex_picker() {
                // File Picker / LiveGrep — min 30% screen, TRUE CENTER giống command palette
                // Rộng hơn command palette một chút để hiển thị đường dẫn file
                let min_w = (width * 0.35).max(400.0);
                let pw = min_w.max(660.0_f32.min(width - 48.0));
                let px = x + ((width - pw) * 0.5).max(0.0);
                let body_rows = complex_picker_body_rows(
                    self.mode,
                    height,
                    panel_padding,
                    line_height,
                    row_height,
                    max_items,
                );
                let ph = (complex_picker_reserved_height(self.mode, panel_padding, line_height)
                    + row_height * body_rows as f32)
                    .min((height - 32.0).max(row_height + panel_padding * 2.0));
                // TRUE CENTER: 50/50
                let py = y + ((height - ph) * 0.5).max(16.0).min(height - ph - 16.0);
                (pw, px, py, ph, body_rows)
            } else {
                // Command Palette — min 30% screen width, TRUE CENTER vũa dọc vũa ngang
                let min_w = (width * 0.30).max(300.0);
                let pw = min_w.max(520.0_f32.min(width - 48.0));
                let px = x + ((width - pw) * 0.5).max(0.0);
                // Dòng input + separator + items (nếu VimCommand: chỉ dòng input)
                let content_rows = (requested_visible_rows
                    + if requested_visible_rows > 0 { 1 } else { 0 })
                .max(1) as f32;
                let ph = (line_height * content_rows
                    + panel_padding * 2.0
                    + if requested_visible_rows > 0 {
                        12.0
                    } else {
                        4.0
                    })
                .min((height - 64.0).max(line_height + panel_padding * 2.0));
                // TRUE CENTER: phân bố 50/50 trần-sàn
                let py = y + ((height - ph) * 0.5).max(16.0).min(height - ph - 16.0);
                let visible_result_rows = if show_results {
                    (((ph - panel_padding * 2.0 - line_height - 8.0) / row_height).floor() as usize)
                        .max(1)
                } else {
                    0
                };
                (pw, px, py, ph, visible_result_rows)
            };

        let panel_bounds = [panel_x, panel_y, panel_width, panel_height];

        let scroll_offset_rows = if visible_result_rows == 0 || self.results.is_empty() {
            0
        } else {
            let max_offset = self.results.len().saturating_sub(visible_result_rows);
            self.selected_index
                .saturating_add(1)
                .saturating_sub(visible_result_rows)
                .min(max_offset)
        };

        let query_for_match = if self.query.is_empty() {
            None
        } else {
            Some(self.query.trim().to_lowercase())
        };

        let result_labels: Vec<String> = self
            .results
            .iter()
            .map(|entry| entry.label.clone())
            .collect();

        let secondary_labels: Vec<String> = if matches!(
            self.mode,
            CommandPaletteMode::RecentProjects | CommandPaletteMode::ThemeSelector
        ) {
            self.results
                .iter()
                .map(|entry| match self.mode {
                    CommandPaletteMode::RecentProjects => match &entry.action {
                        CommandPaletteAction::OpenFile(path) => path.display().to_string(),
                        CommandPaletteAction::OpenSearchMatch { path, .. } => {
                            path.display().to_string()
                        }
                        _ => String::new(),
                    },
                    CommandPaletteMode::ThemeSelector => {
                        entry.secondary_label.clone().unwrap_or_default()
                    }
                    _ => String::new(),
                })
                .collect()
        } else if matches!(self.mode, CommandPaletteMode::LiveGrep) {
            self.results
                .iter()
                .map(|entry| entry.secondary_label.clone().unwrap_or_default())
                .collect()
        } else {
            Vec::new()
        };

        let result_match_ranges: Vec<Vec<(usize, usize)>> = result_labels
            .iter()
            .enumerate()
            .map(|(idx, label)| {
                query_for_match
                    .as_deref()
                    .map(|q| {
                        if self.mode == CommandPaletteMode::LiveGrep {
                            let preview =
                                secondary_labels.get(idx).map(String::as_str).unwrap_or("");
                            compute_match_ranges(preview, q)
                        } else {
                            compute_match_ranges(label, q)
                        }
                    })
                    .unwrap_or_default()
            })
            .collect();

        let mut scrim = theme.ui.overlay_bg.as_f32();
        scrim[3] = scrim[3].max(0.72);

        Some(CommandPaletteRenderModel {
            mode: self.mode,
            overlay_bounds,
            panel_bounds,
            prompt_prefix: self.mode.prompt_prefix().to_string(),
            prompt_query: if self.query.is_empty() {
                self.mode.empty_hint().to_string()
            } else {
                self.query.clone()
            },
            result_labels,
            secondary_labels,
            result_match_ranges,
            selected_index: self.selected_index,
            scroll_offset_rows,
            line_height,
            panel_padding,
            title: self.mode.title().to_string(),
            total_results: self.results.len(),
            show_results,
            border_color: theme.ui.border_color.as_f32(),
            panel_bg: theme.ui.overlay_bg.as_f32(),
            selection_bg: theme.ui.selection_bg.as_f32(),
            text_color: theme.ui.fg.as_f32(),
            hint_color: theme.syntax.comment.as_f32(),
            match_color: theme.ui.accent.as_f32(),
            label_color: theme.ui.fg_dim.as_f32(),
            scrim_color: scrim,
            success_color: theme.ui.success.as_f32(),
            warning_color: theme.ui.warning.as_f32(),
            info_color: theme.ui.info.as_f32(),
        })
    }
}

fn command_palette_items(query: &str, max_results: usize) -> Vec<CommandPaletteItem> {
    let candidates = [
        ("editor.undo", "Undo"),
        ("editor.redo", "Redo"),
        ("editor.toggle_line_comment", "Toggle Line Comment"),
        (
            "editor.toggle_selection_comment",
            "Toggle Selection Comment",
        ),
        ("editor.open_in_file_search", "Search In Current File"),
        ("editor.search_next", "Search Next Match"),
        ("editor.search_prev", "Search Previous Match"),
        (
            "editor.search_word_under_cursor",
            "Search Word Under Cursor",
        ),
        ("editor.clear_search_highlights", "Clear Search Highlights"),
        ("editor.paste", "Paste System Clipboard"),
        ("editor.save_file", "Save File"),
        ("app.open_file_picker", "Open File Picker"),
        ("app.open_file_finder", "Open File Finder"),
        ("app.search_in_files", "Search In Files"),
        ("app.open_workspace_symbols", "Open Workspace Symbols"),
        ("app.open_vim_command", "Open Vim Command"),
        ("app.toggle_terminal", "Toggle Terminal"),
        ("app.toggle_left_dock", "Toggle Left Dock"),
        ("app.focus_editor", "Focus Editor"),
        ("app.focus_explorer", "Focus Explorer"),
        ("app.focus_terminal", "Focus Terminal"),
        ("app.focus_inspector", "Focus Inspector"),
        ("app.focus_left", "Focus Left"),
        ("app.focus_right", "Focus Right"),
        ("app.focus_up", "Focus Up"),
        ("app.focus_down", "Focus Down"),
        ("buffer.new", "New Buffer"),
        ("buffer.next", "Next Buffer"),
        ("buffer.prev", "Previous Buffer"),
        ("buffer.close_current", "Close Current Buffer"),
    ];
    let q = query.trim().to_ascii_lowercase();
    candidates
        .into_iter()
        .filter(|(_, label)| q.is_empty() || label.to_ascii_lowercase().contains(&q))
        .take(max_results)
        .map(|(id, label)| CommandPaletteItem::command(id, label))
        .collect()
}

fn workspace_symbol_items(query: &str, max_results: usize) -> Vec<CommandPaletteItem> {
    let symbols = [
        "main",
        "AppShell::resumed",
        "AppShell::window_event",
        "Renderer::render",
        "WorkbenchLayoutEngine::compute",
        "dispatch_command",
        "InputMap::resolve",
    ];
    let q = query.trim().to_ascii_lowercase();
    symbols
        .into_iter()
        .filter(|name| q.is_empty() || name.to_ascii_lowercase().contains(&q))
        .take(max_results)
        .map(CommandPaletteItem::symbol)
        .collect()
}

fn vim_command_items(query: &str) -> Vec<CommandPaletteItem> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        // Không gợi ý gì cả — giống nvim thật: chờ người dùng gõ
        return Vec::new();
    }

    // Nếu query là số thuần tuý → jump to line
    if let Ok(n) = trimmed.parse::<usize>() {
        return vec![CommandPaletteItem {
            label: format!("Go to line {n}"),
            secondary_label: None,
            action: CommandPaletteAction::ExecuteVimCommand(trimmed.to_string()),
        }];
    }

    // Gợi ý chỉ dựa trên những gì đã gõ — không tự ý thêm
    vec![CommandPaletteItem::vim_input(trimmed)]
}

/// Tính danh sách (start_byte, end_byte) trong `label` khớp với `query` (lowercase).
///
/// - Trước tiên thử substring match → trả về đúng 1 range bao phủ toàn bộ match.
/// - Nếu không có substring, thử fuzzy subsequence → trả về từng char khớp
///   dưới dạng range 1-byte (hoặc char boundary).
///
/// Kết quả dùng bởi renderer để tô màu `match_color` lên phần khớp.
fn compute_match_ranges(label: &str, query: &str) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }

    let label_lower = label.to_lowercase();
    let query_lower = query.trim();

    // 1. Substring match (ưu tiên cao): highlight liên tục
    if let Some(byte_start) = label_lower.find(query_lower) {
        let byte_end = byte_start + query_lower.len();
        return vec![(byte_start, byte_end)];
    }

    // 2. Fuzzy subsequence: highlight từng char riêng
    let mut ranges = Vec::new();
    let mut search_start = 0usize;
    for q_char in query_lower.chars() {
        let found = label_lower[search_start..]
            .char_indices()
            .find(|(_, c)| *c == q_char)
            .map(|(off, c)| {
                let abs = search_start + off;
                (abs, abs + c.len_utf8())
            });
        if let Some((s, e)) = found {
            ranges.push((s, e));
            search_start = e;
        } else {
            // Query không match fuzzy — không highlight gì
            return Vec::new();
        }
    }
    ranges
}

fn palette_max_items(mode: CommandPaletteMode) -> usize {
    match mode {
        CommandPaletteMode::LiveGrep => 10,
        CommandPaletteMode::ThemeSelector => 16,
        mode if mode.is_complex_picker() => 12,
        _ => 10,
    }
}

fn palette_row_height(mode: CommandPaletteMode, line_height: f32) -> f32 {
    match mode {
        CommandPaletteMode::LiveGrep => line_height * 2.0 + 16.0,
        _ => line_height + 8.0,
    }
}

fn complex_picker_reserved_height(
    mode: CommandPaletteMode,
    panel_padding: f32,
    line_height: f32,
) -> f32 {
    let footer_height = line_height + 11.0;
    let header_height = if matches!(
        mode,
        CommandPaletteMode::RecentProjects | CommandPaletteMode::ThemeSelector
    ) {
        line_height * 2.0 + 10.0
    } else {
        line_height + 6.0
    };
    panel_padding * 2.0 + header_height + footer_height
}

fn live_grep_reserved_height(panel_padding: f32, line_height: f32) -> f32 {
    panel_padding * 2.0 + line_height + 6.0 + (line_height + 11.0)
}

fn complex_picker_body_rows(
    mode: CommandPaletteMode,
    overlay_height: f32,
    panel_padding: f32,
    line_height: f32,
    row_height: f32,
    preferred_rows: usize,
) -> usize {
    let preferred_rows = preferred_rows.max(1);
    let reserved_height = complex_picker_reserved_height(mode, panel_padding, line_height);
    let available_height = (overlay_height - 32.0).max(reserved_height + row_height);
    (((available_height - reserved_height) / row_height).floor() as usize).clamp(1, preferred_rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::theme_config::ThemeConfig;

    fn make_item(label: &str) -> CommandPaletteItem {
        CommandPaletteItem::command("test.command", label)
    }

    #[test]
    fn select_next_wraps_to_start() {
        let mut palette = CommandPalette {
            is_visible: true,
            results: vec![make_item("one"), make_item("two"), make_item("three")],
            selected_index: 2,
            ..CommandPalette::default()
        };

        assert!(palette.select_next());
        assert_eq!(palette.selected_index, 0);
    }

    #[test]
    fn select_prev_wraps_to_end() {
        let mut palette = CommandPalette {
            is_visible: true,
            results: vec![make_item("one"), make_item("two"), make_item("three")],
            selected_index: 0,
            ..CommandPalette::default()
        };

        assert!(palette.select_prev());
        assert_eq!(palette.selected_index, 2);
    }

    #[test]
    fn render_keeps_selected_row_visible_with_scroll_offset() {
        let mut palette = CommandPalette {
            mode: CommandPaletteMode::FilePicker,
            is_visible: true,
            results: (0..20)
                .map(|idx| make_item(&format!("item-{idx}")))
                .collect(),
            selected_index: 15,
            ..CommandPalette::default()
        };
        palette.query = "it".to_string();

        let model = palette
            .render(&ThemeConfig::builtin_dark(), [0.0, 0.0, 1200.0, 800.0])
            .expect("render model");

        assert_eq!(model.selected_index, 15);
        assert_eq!(model.scroll_offset_rows, 4);
        assert!(model.selected_index < model.scroll_offset_rows + 12);
    }

    #[test]
    fn complex_picker_panel_bounds_stay_stable_across_async_result_updates() {
        let mut palette = CommandPalette {
            mode: CommandPaletteMode::FilePicker,
            is_visible: true,
            ..CommandPalette::default()
        };

        let empty_model = palette
            .render(&ThemeConfig::builtin_dark(), [0.0, 0.0, 1200.0, 800.0])
            .expect("empty render model");

        palette.results = (0..9)
            .map(|idx| make_item(&format!("src/file_{idx}.rs")))
            .collect();
        let populated_model = palette
            .render(&ThemeConfig::builtin_dark(), [0.0, 0.0, 1200.0, 800.0])
            .expect("populated render model");

        assert_eq!(empty_model.panel_bounds, populated_model.panel_bounds);
        assert_eq!(
            empty_model.scroll_offset_rows,
            populated_model.scroll_offset_rows
        );
    }
}
