use std::path::{Path, PathBuf};

use crate::{
    app::match_ranges::compute_label_match_ranges, config::theme_config::ThemeConfig,
    core::commands::PaletteVimKey, workspace::model::WorkspaceModel,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaletteVimMode {
    #[default]
    Insert,
    Normal,
    Visual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaletteVimOperator {
    #[default]
    Delete,
    Change,
    Yank,
}

/// What the event loop should do after a Vim keystroke is processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteVimAction {
    /// Text/cursor/mode/register changed — just redraw.
    Consumed,
    /// `k` in a list-picker — move result selection up.
    ListPrev,
    /// `j` in a list-picker — move result selection down.
    ListNext,
    /// `Enter` — run the active palette's confirm path.
    Confirm,
    /// `Esc` in Normal — close the palette.
    Close,
    /// Nothing happened.
    Ignore,
}

/// A borrowed, struct-agnostic view of a single-line query editor's Vim state.
/// Both [`CommandPalette`] (overlay prompts) and the fuzzy-picker `FuzzyState`
/// buffers build one of these and run the shared [`vim_line_input`] engine, so
/// there is exactly one Vim implementation for every palette surface.
pub struct VimLineView<'a> {
    pub query: &'a mut String,
    pub cursor_byte: &'a mut usize,
    pub selection_range: &'a mut Option<(usize, usize)>,
    pub vim_mode: &'a mut PaletteVimMode,
    pub pending_operator: &'a mut Option<PaletteVimOperator>,
    pub register: &'a mut String,
    /// Result-list cursor; reset to 0 whenever the query text changes so the
    /// owner can re-filter from the top (mirrors the typing path).
    pub selected_index: &'a mut usize,
}

/// Result of one [`vim_line_input`] keystroke. `text_changed` tells the owner
/// whether to refresh its result list / re-run its search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VimLineOutcome {
    pub action: PaletteVimAction,
    pub text_changed: bool,
}

impl VimLineOutcome {
    fn action(action: PaletteVimAction) -> Self {
        Self {
            action,
            text_changed: false,
        }
    }
    fn changed(action: PaletteVimAction) -> Self {
        Self {
            action,
            text_changed: true,
        }
    }
}

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
    /// Document Symbols in the active file (`@` / leader f p).
    DocumentSymbols,
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
    /// Explorer prompt để đổi tên đầy đủ file đang chọn.
    ExplorerRenameFull,
    /// Explorer prompt để đổi tên file, preselect base name và giữ extension.
    ExplorerRenameBase,
    /// Explorer prompt to choose the destination name for a pasted file.
    ExplorerPasteFile,
    /// Dirty buffer close confirmation overlay.
    BufferCloseConfirm,
    /// Recent Projects picker — hiện danh sách project đã mở gần đây.
    RecentProjects,
    /// Theme selector — liệt kê `config/themes/*.toml` để hot-reload runtime.
    ThemeSelector,
    /// LSP References — danh sách tĩnh kết quả `gr` từ LSP server.
    LspReferences,
    /// LSP Rename prompt — nhập tên mới cho symbol dưới cursor.
    LspRename,
    /// Local file history picker with live editor preview.
    FileHistory,
    /// LSP Code Action picker — danh sách các action user có thể chọn để apply.
    CodeAction,
    /// Python environment selector — opened from the command palette.
    PythonEnvSelector,
    /// Dart environment selector — opened from the command palette.
    DartEnvSelector,
    /// LeetCode language picker — choose a language to scaffold a new
    /// runnable stdin/stdout solution file. Static list, MRU-sorted.
    LeetCodeLanguageSelector,
    /// Free-text problem ID, slug, or URL prompt.
    LeetCodeProblemInput,
}

impl CommandPaletteMode {
    pub fn prompt_prefix(self) -> &'static str {
        match self {
            Self::FilePicker => "find> ",
            Self::CommandPalette => "> ",
            Self::VimCommand => ":",
            Self::WorkspaceSymbols => "@ ",
            Self::DocumentSymbols => "@ ",
            Self::LiveGrep => "grep> ",
            Self::InFileSearch => "/",
            Self::ExplorerCreateFile => "file> ",
            Self::ExplorerCreateFolder => "dir> ",
            Self::ExplorerDeleteConfirm => "delete> ",
            Self::ExplorerRenameFull | Self::ExplorerRenameBase => "rename> ",
            Self::ExplorerPasteFile => "paste> ",
            Self::BufferCloseConfirm => "close> ",
            Self::RecentProjects => "project> ",
            Self::ThemeSelector => "Select Theme> ",
            Self::LspReferences => "refs> ",
            Self::LspRename => "rename> ",
            Self::FileHistory => "history> ",
            Self::CodeAction => "action> ",
            Self::PythonEnvSelector => "python> ",
            Self::DartEnvSelector => "dart> ",
            Self::LeetCodeLanguageSelector => "language> ",
            Self::LeetCodeProblemInput => "leetcode> ",
        }
    }

    pub fn empty_hint(self) -> &'static str {
        match self {
            Self::FilePicker => "type to search files...",
            Self::CommandPalette => "type a command...",
            Self::VimCommand => "type a vim command...",
            Self::WorkspaceSymbols => "type to search symbols...",
            Self::DocumentSymbols => "type to search symbols in file...",
            Self::LiveGrep => "type to grep workspace...",
            Self::InFileSearch => "type to search in current file...",
            Self::ExplorerCreateFile => "enter a new file path...",
            Self::ExplorerCreateFolder => "enter a new folder path...",
            Self::ExplorerDeleteConfirm => "Delete selected item? (y/n)",
            Self::ExplorerRenameFull | Self::ExplorerRenameBase => "enter a new file name...",
            Self::ExplorerPasteFile => "enter destination file name...",
            Self::BufferCloseConfirm => "Save changes before closing? (y/n)",
            Self::RecentProjects => "type to filter projects...",
            Self::ThemeSelector => "type to filter themes...",
            Self::LspReferences => "no references found",
            Self::LspRename => "enter a new symbol name...",
            Self::FileHistory => "no local history entries",
            Self::CodeAction => "no code actions available",
            Self::PythonEnvSelector => "scanning Python environments...",
            Self::DartEnvSelector => "scanning Dart environments...",
            Self::LeetCodeLanguageSelector => "type to filter languages...",
            Self::LeetCodeProblemInput => "enter problem ID, slug, or URL...",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::FilePicker => "FIND",
            Self::CommandPalette => "COMMANDS",
            Self::VimCommand => "VIM",
            Self::WorkspaceSymbols => "SYMBOLS",
            Self::DocumentSymbols => "SYMBOLS",
            Self::LiveGrep => "GREP",
            Self::InFileSearch => "SEARCH",
            Self::ExplorerCreateFile => "NEW FILE",
            Self::ExplorerCreateFolder => "NEW FOLDER",
            Self::ExplorerDeleteConfirm => "DELETE",
            Self::ExplorerRenameFull | Self::ExplorerRenameBase => "RENAME",
            Self::ExplorerPasteFile => "PASTE",
            Self::BufferCloseConfirm => "CLOSE",
            Self::RecentProjects => "RECENT",
            Self::ThemeSelector => "THEMES",
            Self::LspReferences => "REFS",
            Self::LspRename => "RENAME",
            Self::FileHistory => "HISTORY",
            Self::CodeAction => "ACTIONS",
            Self::PythonEnvSelector => "PYTHON ENV",
            Self::DartEnvSelector => "DART ENV",
            Self::LeetCodeLanguageSelector => "NEW LEETCODE",
            Self::LeetCodeProblemInput => "FETCH LEETCODE",
        }
    }

    /// File Picker, Live Grep, Recent Projects, Theme Selector và Command
    /// Palette dùng UI phức tạp (badge header + footer + group rows).
    pub fn is_complex_picker(self) -> bool {
        matches!(
            self,
            Self::FilePicker
                | Self::LiveGrep
                | Self::RecentProjects
                | Self::ThemeSelector
                | Self::LspReferences
                | Self::FileHistory
                | Self::DocumentSymbols
                | Self::CommandPalette
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
    JumpToDocumentSymbol {
        name: String,
        line: u32,
        column: u32,
    },
    SelectFileHistoryEntry(usize),
    /// Áp dụng code action tại index đã chọn trong pending_code_actions của AppShell.
    ApplyCodeAction(usize),
    /// Chọn Python environment path để restart LSP.
    SelectPythonEnv(PathBuf),
    /// Chọn Dart/Flutter SDK path để restart LSP.
    SelectDartEnv(PathBuf),
    /// Chọn ngôn ngữ để scaffold một file LeetCode mới (key = template key).
    CreateLeetCodeFile(String),
    FetchLeetCodeWithLanguage {
        problem_input: String,
        language_key: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommandPaletteItemTone {
    #[default]
    Default,
    Added,
    Removed,
    Modified,
    Function,
    Type,
    Variable,
    Module,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandPaletteItem {
    pub label: String,
    pub secondary_label: Option<String>,
    /// Optional built-in icon id (e.g. `built_in:symbol-function`) drawn in a
    /// fixed-width slot before the label. Only the symbol picker sets this; other
    /// pickers leave it `None` so their labels start flush at the row edge.
    pub icon: Option<String>,
    pub action: CommandPaletteAction,
    pub tone: CommandPaletteItemTone,
    pub preview_colors: Vec<[f32; 4]>,
}

impl CommandPaletteItem {
    pub fn file_match(relative_path: String, absolute_path: PathBuf) -> Self {
        Self {
            label: relative_path,
            secondary_label: None,
            icon: None,
            action: CommandPaletteAction::OpenFile(absolute_path),
            tone: CommandPaletteItemTone::Default,
            preview_colors: Vec::new(),
        }
    }

    pub fn recent_project(path: &std::path::Path) -> Self {
        Self::recent_project_with_meta(path, None, None)
    }

    pub fn recent_project_with_meta(
        path: &std::path::Path,
        icon_source: Option<&str>,
        last_opened_unix_secs: Option<u64>,
    ) -> Self {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let secondary_label = match (icon_source, last_opened_unix_secs) {
            (Some(icon), Some(secs)) => Some(format!("icon={icon};last={secs}")),
            (Some(icon), None) => Some(format!("icon={icon}")),
            (None, Some(secs)) => Some(format!("last={secs}")),
            (None, None) => None,
        };
        Self {
            label: name,
            secondary_label,
            icon: None,
            action: CommandPaletteAction::OpenFile(path.to_path_buf()),
            tone: CommandPaletteItemTone::Default,
            preview_colors: Vec::new(),
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
            icon: None,
            action: CommandPaletteAction::OpenSearchMatch { path, line, column },
            tone: CommandPaletteItemTone::Default,
            preview_colors: Vec::new(),
        }
    }

    pub fn command(id: &str, label: &str) -> Self {
        Self {
            label: label.to_string(),
            // Shown right-aligned & dimmed so keymap users see the id to bind.
            secondary_label: Some(id.to_string()),
            icon: None,
            action: CommandPaletteAction::ExecuteCommand(id.to_string()),
            tone: CommandPaletteItemTone::Default,
            preview_colors: Vec::new(),
        }
    }

    pub fn theme(name: &str, path: &Path) -> Self {
        let preview_colors = ThemeConfig::load(name)
            .map(|theme| theme_selector_preview_colors(&theme))
            .unwrap_or_default();
        Self {
            label: name.to_string(),
            secondary_label: Some(path.display().to_string()),
            icon: None,
            action: CommandPaletteAction::SelectTheme(name.to_string()),
            tone: CommandPaletteItemTone::Default,
            preview_colors,
        }
    }

    pub fn symbol(name: &str) -> Self {
        Self {
            label: name.to_string(),
            secondary_label: None,
            icon: None,
            action: CommandPaletteAction::JumpToSymbol(name.to_string()),
            tone: CommandPaletteItemTone::Default,
            preview_colors: Vec::new(),
        }
    }

    pub fn document_symbol(symbol: &crate::async_runtime::message::LspDocumentSymbol) -> Self {
        let icon = symbol_icon(&symbol.kind);
        let line = symbol.range.start.line + 1;
        let column = symbol.range.start.character + 1;
        Self {
            // Label is JUST the name so every row shares a left edge; the kind
            // icon is drawn in a fixed slot, and the kind/line live in the
            // right-aligned secondary label.
            label: symbol.name.clone(),
            secondary_label: Some(format!("{}  ·  Ln {}, Col {}", symbol.kind, line, column)),
            icon: Some(icon.to_string()),
            action: CommandPaletteAction::JumpToDocumentSymbol {
                name: symbol.name.clone(),
                line: symbol.range.start.line,
                column: symbol.range.start.character,
            },
            tone: symbol_tone(&symbol.kind),
            preview_colors: Vec::new(),
        }
    }

    pub fn file_history_entry(
        label: String,
        secondary_label: Option<String>,
        index: usize,
        tone: CommandPaletteItemTone,
    ) -> Self {
        Self {
            label,
            secondary_label,
            icon: None,
            action: CommandPaletteAction::SelectFileHistoryEntry(index),
            tone,
            preview_colors: Vec::new(),
        }
    }

    pub fn leetcode_language(key: &str, label: &str, hint: &str) -> Self {
        Self {
            label: label.to_string(),
            secondary_label: Some(hint.to_string()),
            icon: None,
            action: CommandPaletteAction::CreateLeetCodeFile(key.to_string()),
            tone: CommandPaletteItemTone::Default,
            preview_colors: Vec::new(),
        }
    }

    pub fn leetcode_fetch_language(
        problem_input: &str,
        key: &str,
        label: &str,
        hint: &str,
    ) -> Self {
        Self {
            label: label.to_string(),
            secondary_label: Some(hint.to_string()),
            icon: None,
            action: CommandPaletteAction::FetchLeetCodeWithLanguage {
                problem_input: problem_input.to_string(),
                language_key: key.to_string(),
            },
            tone: CommandPaletteItemTone::Default,
            preview_colors: Vec::new(),
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
            icon: None,
            action: CommandPaletteAction::ExecuteVimCommand(trimmed.to_string()),
            tone: CommandPaletteItemTone::Default,
            preview_colors: Vec::new(),
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
    /// Per-row section label ("RECENT"/"COMMANDS"). Empty = no grouping; the
    /// renderer draws a group header whenever the value changes between rows.
    pub row_group_labels: Vec<String>,
    /// Match byte ranges (start..end) trong label tương ứng.
    pub result_match_ranges: Vec<Vec<(usize, usize)>>,
    pub selected_index: usize,
    pub scroll_offset_rows: usize,
    pub line_height: f32,
    pub row_height: f32,
    pub panel_padding: f32,
    /// Tiêu đề ngắn ("FIND", "COMMANDS"...) — hiện trong badge header của file picker
    pub title: String,
    /// Tổng số kết quả trước khi limit — dùng cho counter "8/847"
    pub total_results: usize,
    pub is_loading: bool,
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
    pub magenta_color: [f32; 4],
    pub cyan_color: [f32; 4],
    pub amber_color: [f32; 4],
    pub item_tones: Vec<CommandPaletteItemTone>,
    /// Optional built-in icon id per result (symbol picker only). `None` rows
    /// draw no icon and keep their label flush at the row edge.
    pub item_icons: Vec<Option<String>>,
    pub item_preview_colors: Vec<Vec<[f32; 4]>>,
    pub search_case_sensitive: bool,
    pub prompt_cursor_byte: usize,
    pub prompt_selection_range: Option<(usize, usize)>,
    pub vim_mode_label: Option<&'static str>,
    pub vim_mode_color: Option<[f32; 4]>,
    pub vim_caret_block: bool,
    pub collapsed_paths: std::collections::HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct CommandPalette {
    pub mode: CommandPaletteMode,
    pub query: String,
    pub results: Vec<CommandPaletteItem>,
    pub selected_index: usize,
    pub is_visible: bool,
    pub is_loading: bool,
    pub cursor_byte: usize,
    pub selection_range: Option<(usize, usize)>,
    /// Current Vim sub-mode for the single-line query editor.
    pub vim_mode: PaletteVimMode,
    /// Pending operator (`d`/`c`/`y`) awaiting a motion.
    pending_operator: Option<PaletteVimOperator>,
    /// Internal unnamed register for `x`/`d`/`c`/`y` ↔ `p`/`P`.
    vim_register: String,
    max_results: usize,
    /// Items pre-populated externally (e.g. recent projects). Used by
    /// RecentProjects mode where `refresh_results` would otherwise clear them.
    static_items: Vec<CommandPaletteItem>,
    /// Header badge text overriding the mode's default title, for pickers that
    /// reuse another mode's plumbing (worktree switcher reuses RecentProjects).
    title_override: Option<String>,
    /// Most-recently-run command ids (from persistence), newest first. Shown
    /// as the RECENT group at the top of CommandPalette while the query is
    /// empty.
    recent_command_ids: Vec<String>,
    /// How many rows at the top of `results` came from `recent_command_ids`
    /// in the latest refresh (0 whenever a query is active).
    recent_rows_in_results: usize,
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self {
            mode: CommandPaletteMode::FilePicker,
            query: String::new(),
            results: Vec::new(),
            selected_index: 0,
            is_visible: false,
            is_loading: false,
            cursor_byte: 0,
            selection_range: None,
            vim_mode: PaletteVimMode::Insert,
            pending_operator: None,
            vim_register: String::new(),
            max_results: DEFAULT_MAX_RESULTS,
            static_items: Vec::new(),
            title_override: None,
            recent_command_ids: Vec::new(),
            recent_rows_in_results: 0,
        }
    }
}

impl CommandPalette {
    pub fn open(&mut self, mode: CommandPaletteMode, workspace: Option<&WorkspaceModel>) -> usize {
        self.mode = mode;
        self.query.clear();
        self.selected_index = 0;
        self.is_visible = true;
        self.is_loading = false;
        self.cursor_byte = 0;
        self.selection_range = None;
        self.vim_mode = PaletteVimMode::Insert;
        self.pending_operator = None;
        // vim_register intentionally persists across opens.
        self.static_items.clear();
        self.title_override = None;
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
        self.is_loading = false;
        self.cursor_byte = 0;
        self.selection_range = None;
        // Every fresh open starts in Insert so the user can type to filter
        // immediately (matches `open`). Without this the picker inherits the
        // stale vim mode from a previous session and opens in Normal.
        self.vim_mode = PaletteVimMode::Insert;
        self.pending_operator = None;
        self.title_override = None;
        self.static_items = items.clone();
        self.results = items;
        self.results.len()
    }

    /// Restore the interaction state after a static picker list was rebuilt
    /// in place (e.g. `x` removed a recent project): keep the query filter,
    /// stay in the same vim mode and hold the selection near the removed row
    /// so repeated deletes chain without leaving Normal mode.
    pub fn restore_picker_interaction(
        &mut self,
        query: &str,
        vim_mode: PaletteVimMode,
        selected_index: usize,
    ) {
        if !query.is_empty() {
            self.query = query.to_string();
            self.cursor_byte = self.query.len();
            self.refresh_results(None);
        }
        self.vim_mode = vim_mode;
        self.selected_index = if self.results.is_empty() {
            0
        } else {
            selected_index.min(self.results.len() - 1)
        };
    }

    /// Override the header badge text for the current open (cleared by the
    /// next `open`/`open_with_items`/`close`).
    pub fn set_title_override(&mut self, title: Option<String>) {
        self.title_override = title;
    }

    /// Feed the persisted most-recently-run command ids into the palette and
    /// rebuild results so the RECENT group shows up on open.
    pub fn set_recent_commands(&mut self, ids: Vec<String>) {
        if self.recent_command_ids == ids {
            return;
        }
        self.recent_command_ids = ids;
        if self.is_visible && self.mode == CommandPaletteMode::CommandPalette {
            self.refresh_results(None);
        }
    }

    /// Move the item whose action selects `profile` to the top of the list and
    /// select it — the theme picker opens showing the active theme first, so
    /// the open itself never changes the previewed theme.
    pub fn promote_theme(&mut self, profile: &str) -> bool {
        let position = self.static_items.iter().position(|item| {
            matches!(&item.action,
                CommandPaletteAction::SelectTheme(name) if name.eq_ignore_ascii_case(profile))
        });
        let Some(position) = position else {
            return false;
        };
        let item = self.static_items.remove(position);
        self.static_items.insert(0, item);
        self.results = self.static_items.clone();
        self.selected_index = 0;
        true
    }

    pub fn set_hidden_items(
        &mut self,
        mode: CommandPaletteMode,
        items: Vec<CommandPaletteItem>,
    ) -> bool {
        let selected_index = self.selected_index;
        let changed = self.mode != mode || self.results.len() != items.len();
        self.mode = mode;
        self.query.clear();
        self.title_override = None;
        self.is_visible = false;
        self.is_loading = false;
        self.cursor_byte = 0;
        self.selection_range = None;
        self.static_items = items.clone();
        self.results = items;
        self.selected_index = if self.results.is_empty() {
            0
        } else {
            selected_index.min(self.results.len() - 1)
        };
        changed
    }

    pub fn close(&mut self) -> bool {
        let was_open = self.is_visible;
        self.is_visible = false;
        self.is_loading = false;
        self.query.clear();
        self.selected_index = 0;
        self.results.clear();
        self.static_items.clear();
        self.title_override = None;
        self.cursor_byte = 0;
        self.selection_range = None;
        was_open
    }

    pub fn set_loading(&mut self, is_loading: bool) -> bool {
        if self.is_loading == is_loading {
            return false;
        }
        self.is_loading = is_loading;
        true
    }

    pub fn append_query(&mut self, text: &str, workspace: Option<&WorkspaceModel>) -> bool {
        if text.is_empty() || !self.is_visible {
            return false;
        }
        if let Some((start, end)) = self.normalized_selection_range() {
            self.query.replace_range(start..end, text);
            self.cursor_byte = start + text.len();
            self.selection_range = None;
        } else {
            self.query.push_str(text);
            self.cursor_byte = self.query.len();
        }
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
        self.cursor_byte = self.query.len();
        self.selection_range = None;
        self.refresh_results(workspace);
        true
    }

    pub fn backspace_query(&mut self, workspace: Option<&WorkspaceModel>) -> bool {
        if !self.is_visible || self.query.is_empty() {
            return false;
        }
        if let Some((start, end)) = self.normalized_selection_range() {
            self.query.replace_range(start..end, "");
            self.cursor_byte = start;
            self.selection_range = None;
        } else {
            self.query.pop();
            self.cursor_byte = self.query.len();
        }
        self.selected_index = 0;
        self.refresh_results(workspace);
        true
    }

    pub fn move_cursor_left(&mut self) -> bool {
        if !self.is_visible {
            return false;
        }
        if let Some((start, _end)) = self.normalized_selection_range() {
            self.cursor_byte = start;
            self.selection_range = None;
            return true;
        }
        if self.cursor_byte == 0 {
            return false;
        }
        let new_byte = self.prev_char_boundary(self.cursor_byte);
        let changed = new_byte != self.cursor_byte;
        self.cursor_byte = new_byte;
        changed
    }

    pub fn move_cursor_right(&mut self) -> bool {
        if !self.is_visible {
            return false;
        }
        if let Some((_start, end)) = self.normalized_selection_range() {
            self.cursor_byte = end;
            self.selection_range = None;
            return true;
        }
        if self.cursor_byte >= self.query.len() {
            return false;
        }
        let new_byte = self.next_char_boundary(self.cursor_byte);
        let changed = new_byte != self.cursor_byte;
        self.cursor_byte = new_byte;
        changed
    }

    pub fn move_cursor_to_start(&mut self) -> bool {
        if !self.is_visible {
            return false;
        }
        if let Some((start, _end)) = self.normalized_selection_range() {
            self.cursor_byte = start;
            self.selection_range = None;
            return true;
        }
        if self.cursor_byte == 0 {
            return false;
        }
        self.cursor_byte = 0;
        true
    }

    pub fn move_cursor_to_end(&mut self) -> bool {
        if !self.is_visible {
            return false;
        }
        if let Some((_start, end)) = self.normalized_selection_range() {
            self.cursor_byte = end;
            self.selection_range = None;
            return true;
        }
        if self.cursor_byte >= self.query.len() {
            return false;
        }
        self.cursor_byte = self.query.len();
        true
    }

    pub fn delete_char_forward(&mut self, workspace: Option<&WorkspaceModel>) -> bool {
        if !self.is_visible || self.query.is_empty() {
            return false;
        }
        if let Some((start, end)) = self.normalized_selection_range() {
            self.query.replace_range(start..end, "");
            self.cursor_byte = start;
            self.selection_range = None;
        } else {
            if self.cursor_byte >= self.query.len() {
                return false;
            }
            let end = self.next_char_boundary(self.cursor_byte);
            self.query.replace_range(self.cursor_byte..end, "");
        }
        self.selected_index = 0;
        self.refresh_results(workspace);
        true
    }

    fn prev_char_boundary(&self, byte: usize) -> usize {
        if byte == 0 {
            return 0;
        }
        let mut i = byte - 1;
        while i > 0 && !self.query.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    fn next_char_boundary(&self, byte: usize) -> usize {
        if byte >= self.query.len() {
            return self.query.len();
        }
        let mut i = byte + 1;
        while i < self.query.len() && !self.query.is_char_boundary(i) {
            i += 1;
        }
        i
    }

    /// Run the shared Vim engine over this overlay palette's query, then refresh
    /// the result list if the text changed. Thin wrapper over [`vim_line_input`].
    pub fn vim_input(
        &mut self,
        key: PaletteVimKey,
        has_result_list: bool,
        workspace: Option<&WorkspaceModel>,
    ) -> PaletteVimAction {
        let mut view = VimLineView {
            query: &mut self.query,
            cursor_byte: &mut self.cursor_byte,
            selection_range: &mut self.selection_range,
            vim_mode: &mut self.vim_mode,
            pending_operator: &mut self.pending_operator,
            register: &mut self.vim_register,
            selected_index: &mut self.selected_index,
        };
        let outcome = vim_line_input(&mut view, key, has_result_list);
        if outcome.text_changed {
            self.refresh_results(workspace);
        }
        outcome.action
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
        // LspReferences / CodeAction / PythonEnvSelector: static list populated by async results.
        if matches!(
            self.mode,
            CommandPaletteMode::LspReferences
                | CommandPaletteMode::FileHistory
                | CommandPaletteMode::CodeAction
                | CommandPaletteMode::PythonEnvSelector
                | CommandPaletteMode::DartEnvSelector
        ) {
            self.results = self.static_items.clone();
            if self.results.is_empty() {
                self.selected_index = 0;
            } else {
                self.selected_index = self.selected_index.min(self.results.len() - 1);
            }
            return;
        }

        // Static-list pickers: same fuzzy matcher as the command palette,
        // ranked over label + secondary text (path for recent projects).
        if matches!(
            self.mode,
            CommandPaletteMode::RecentProjects
                | CommandPaletteMode::ThemeSelector
                | CommandPaletteMode::DocumentSymbols
                | CommandPaletteMode::LeetCodeLanguageSelector
        ) {
            self.results = if self.query.is_empty() {
                self.static_items.clone()
            } else {
                let q = self.query.trim().to_lowercase();
                let mut scored: Vec<(i64, usize)> = self
                    .static_items
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, item)| {
                        fuzzy_score(&item.label, &item_secondary_text(item), &q)
                            .map(|score| (score, idx))
                    })
                    .collect();
                scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
                scored
                    .into_iter()
                    .map(|(_, idx)| self.static_items[idx].clone())
                    .collect()
            };
            if self.results.is_empty() {
                self.selected_index = 0;
            } else {
                self.selected_index = self.selected_index.min(self.results.len() - 1);
            }
            return;
        }

        self.recent_rows_in_results = 0;
        self.results = match self.mode {
            CommandPaletteMode::FilePicker => Vec::new(),
            CommandPaletteMode::CommandPalette => {
                // Full action registry — never truncated here; the panel
                // scrolls through whatever the filter leaves.
                let (items, recent_rows) =
                    command_palette_items(&self.query, &self.recent_command_ids);
                self.recent_rows_in_results = recent_rows;
                items
            }
            CommandPaletteMode::VimCommand => vim_command_items(&self.query),
            CommandPaletteMode::WorkspaceSymbols => {
                workspace_symbol_items(&self.query, self.max_results)
            }
            CommandPaletteMode::DocumentSymbols => unreachable!("handled above"),
            CommandPaletteMode::LiveGrep => Vec::new(),
            CommandPaletteMode::InFileSearch => Vec::new(),
            CommandPaletteMode::ExplorerCreateFile
            | CommandPaletteMode::ExplorerCreateFolder
            | CommandPaletteMode::ExplorerDeleteConfirm
            | CommandPaletteMode::ExplorerRenameFull
            | CommandPaletteMode::ExplorerRenameBase
            | CommandPaletteMode::ExplorerPasteFile
            | CommandPaletteMode::LspRename
            | CommandPaletteMode::BufferCloseConfirm => Vec::new(),
            CommandPaletteMode::LeetCodeProblemInput => Vec::new(),
            CommandPaletteMode::RecentProjects => unreachable!("handled above"),
            CommandPaletteMode::ThemeSelector => unreachable!("handled above"),
            CommandPaletteMode::LspReferences => unreachable!("handled above"),
            CommandPaletteMode::FileHistory => unreachable!("handled above"),
            CommandPaletteMode::CodeAction => unreachable!("handled above"),
            CommandPaletteMode::LeetCodeLanguageSelector => unreachable!("handled above"),
            CommandPaletteMode::PythonEnvSelector => Vec::new(),
            CommandPaletteMode::DartEnvSelector => Vec::new(),
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

    pub fn set_selection_range(&mut self, range: Option<(usize, usize)>) -> bool {
        let normalized = range.map(|(start, end)| {
            let start = start.min(self.query.len());
            let end = end.min(self.query.len());
            if start <= end {
                (start, end)
            } else {
                (end, start)
            }
        });
        if self.selection_range == normalized {
            return false;
        }
        self.selection_range = normalized;
        true
    }

    fn normalized_selection_range(&self) -> Option<(usize, usize)> {
        self.selection_range.and_then(|(start, end)| {
            let start = start.min(self.query.len());
            let end = end.min(self.query.len());
            (start < end).then_some((start, end))
        })
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

    pub fn replace_static_results(&mut self, results: Vec<CommandPaletteItem>) -> bool {
        let previous_static = self.static_items.clone();
        self.static_items = results;
        let was_loading = self.set_loading(false);
        self.refresh_results(None);
        previous_static != self.static_items || was_loading
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
                | CommandPaletteMode::ExplorerRenameFull
                | CommandPaletteMode::ExplorerRenameBase
                | CommandPaletteMode::LspRename
                | CommandPaletteMode::BufferCloseConfirm
        );
        let requested_visible_rows = if show_results {
            self.results.len().min(max_items)
        } else {
            0
        };

        // ── Layout: tách Command Palette vs File Picker/LiveGrep ─────────────
        let (panel_width, panel_x, panel_y, panel_height, visible_result_rows) = if self.mode
            == CommandPaletteMode::LiveGrep
        {
            let pw = complex_panel_width(width);
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
            // Every complex picker shares ONE width so the components read as
            // the same surface — only row content differs per mode.
            let pw = complex_panel_width(width);
            let px = x + ((width - pw) * 0.5).max(0.0);
            let body_rows = if self.mode == CommandPaletteMode::FilePicker
                && self.results.is_empty()
                && self.query.trim().is_empty()
            {
                0
            } else {
                complex_picker_body_rows(
                    self.mode,
                    height,
                    panel_padding,
                    line_height,
                    row_height,
                    max_items,
                )
            };
            let min_picker_height = if body_rows == 0 {
                complex_picker_reserved_height(self.mode, panel_padding, line_height)
            } else {
                row_height + panel_padding * 2.0
            };
            let ph = (complex_picker_reserved_height(self.mode, panel_padding, line_height)
                + row_height * body_rows as f32)
                .min((height - 32.0).max(min_picker_height));
            // TRUE CENTER: 50/50
            let py = y + ((height - ph) * 0.5).max(16.0).min(height - ph - 16.0);
            (pw, px, py, ph, body_rows)
        } else {
            // Command Palette — min 30% screen width, TRUE CENTER vũa dọc vũa ngang
            let is_confirmation = matches!(
                self.mode,
                CommandPaletteMode::ExplorerDeleteConfirm | CommandPaletteMode::BufferCloseConfirm
            );
            let min_w = if is_confirmation {
                (width * 0.34).max(380.0)
            } else {
                (width * 0.30).max(300.0)
            };
            let ideal_w: f32 = if is_confirmation { 640.0 } else { 520.0 };
            let pw = min_w.max(ideal_w.min(width - 48.0));
            let px = x + ((width - pw) * 0.5).max(0.0);
            // Dòng input + separator + items (nếu VimCommand: chỉ dòng input)
            let content_rows = (requested_visible_rows
                + if requested_visible_rows > 0 { 1 } else { 0 })
            .max(1) as f32;
            let ph = if is_confirmation {
                (line_height * 6.1 + panel_padding * 2.0 + 36.0)
                    .min((height - 64.0).max(line_height + panel_padding * 2.0))
            } else {
                // Must match the minimalist renderer's row math exactly, else
                // the panel is shorter than the rows it claims to show and the
                // last result gets clipped by the scissor. Layout per row:
                //   prompt_h (= line_height + 10, min 30) + separator (8px)
                //   + N * row_height (= line_height + 8) + top/bottom padding.
                let _ = content_rows;
                let prompt_h = (line_height + 10.0).max(30.0);
                let body_h = if requested_visible_rows > 0 {
                    8.0 + row_height * requested_visible_rows as f32
                } else {
                    4.0
                };
                (prompt_h + body_h + panel_padding * 2.0)
                    .min((height - 64.0).max(line_height + panel_padding * 2.0))
            };
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
                        CommandPaletteAction::OpenFile(path) => match &entry.secondary_label {
                            Some(meta) if !meta.is_empty() => {
                                format!("{}\u{1f}{}", path.display(), meta)
                            }
                            _ => path.display().to_string(),
                        },
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
        } else if matches!(
            self.mode,
            CommandPaletteMode::LiveGrep
                | CommandPaletteMode::DocumentSymbols
                | CommandPaletteMode::LeetCodeLanguageSelector
                | CommandPaletteMode::CommandPalette
        ) {
            self.results
                .iter()
                .map(|entry| entry.secondary_label.clone().unwrap_or_default())
                .collect()
        } else {
            Vec::new()
        };

        let row_group_labels: Vec<String> = if self.mode == CommandPaletteMode::CommandPalette
            && self.query.is_empty()
            && self.recent_rows_in_results > 0
        {
            (0..self.results.len())
                .map(|idx| {
                    if idx < self.recent_rows_in_results {
                        "RECENT".to_string()
                    } else {
                        "COMMANDS".to_string()
                    }
                })
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
                            compute_label_match_ranges(preview, q)
                        } else {
                            compute_label_match_ranges(label, q)
                        }
                    })
                    .unwrap_or_default()
            })
            .collect();

        let mut scrim = theme.ui.overlay_bg.as_f32();
        scrim[3] = scrim[3].max(0.72);
        let mut panel_bg = theme.ui.panel_bg.as_f32();
        panel_bg[3] = panel_bg[3].max(0.98);
        if self.mode == CommandPaletteMode::ThemeSelector {
            // Live preview must show THROUGH the picker: near-invisible scrim,
            // translucent panel. The old "méo" look came from the misaligned
            // frost/veil quads and the faint border (both removed), not from
            // the translucency itself — keep the panel frame crisp.
            scrim[3] = 0.10;
            panel_bg = theme.ui.overlay_bg.as_f32();
            panel_bg[3] = panel_bg[3].clamp(0.55, 0.68);
        }

        Some(CommandPaletteRenderModel {
            mode: self.mode,
            overlay_bounds,
            panel_bounds,
            prompt_prefix: self.mode.prompt_prefix().to_string(),
            prompt_query: if self.query.is_empty() {
                if self.is_loading {
                    "loading symbols...".to_string()
                } else {
                    self.mode.empty_hint().to_string()
                }
            } else {
                self.query.clone()
            },
            result_labels,
            secondary_labels,
            row_group_labels,
            result_match_ranges,
            selected_index: self.selected_index,
            scroll_offset_rows,
            line_height,
            row_height,
            panel_padding,
            title: self
                .title_override
                .clone()
                .unwrap_or_else(|| self.mode.title().to_string()),
            total_results: self.results.len(),
            is_loading: self.is_loading,
            show_results,
            border_color: theme.ui.border_color.as_f32(),
            panel_bg,
            selection_bg: theme.ui.selection_bg.as_f32(),
            text_color: theme.ui.fg.as_f32(),
            hint_color: theme.syntax.comment.as_f32(),
            match_color: theme.ui.accent.as_f32(),
            label_color: theme.ui.fg_dim.as_f32(),
            scrim_color: scrim,
            success_color: theme.ui.success.as_f32(),
            warning_color: theme.ui.warning.as_f32(),
            info_color: theme.ui.info.as_f32(),
            magenta_color: theme.ui.magenta.as_f32(),
            cyan_color: theme.ui.cyan.as_f32(),
            amber_color: theme.ui.amber.as_f32(),
            item_tones: self.results.iter().map(|entry| entry.tone).collect(),
            item_icons: self
                .results
                .iter()
                .map(|entry| entry.icon.clone())
                .collect(),
            item_preview_colors: self
                .results
                .iter()
                .map(|entry| entry.preview_colors.clone())
                .collect(),
            search_case_sensitive: false,
            prompt_cursor_byte: self.cursor_byte,
            prompt_selection_range: self.normalized_selection_range(),
            vim_mode_label: match self.vim_mode {
                PaletteVimMode::Insert => Some("INSERT"),
                PaletteVimMode::Normal => Some("NORMAL"),
                PaletteVimMode::Visual => Some("VISUAL"),
            },
            vim_mode_color: match self.vim_mode {
                PaletteVimMode::Insert => Some(theme.ui.mode_insert.as_f32()),
                PaletteVimMode::Normal => Some(theme.ui.mode_normal.as_f32()),
                PaletteVimMode::Visual => Some(theme.ui.mode_visual.as_f32()),
            },
            vim_caret_block: matches!(
                self.vim_mode,
                PaletteVimMode::Normal | PaletteVimMode::Visual
            ),
            collapsed_paths: std::collections::HashSet::new(),
        })
    }
}

fn theme_selector_preview_colors(theme: &ThemeConfig) -> Vec<[f32; 4]> {
    vec![
        theme.ui.accent.as_f32(),
        theme.syntax.keyword.as_f32(),
        theme.syntax.string.as_f32(),
        theme.syntax.function.as_f32(),
        theme.ui.warning.as_f32(),
        theme.ui.info.as_f32(),
    ]
}

pub(crate) fn symbol_icon(kind: &str) -> &'static str {
    match kind {
        // LSP symbol kinds mapped to SVG icons in assets/bearded-icons/symbol-*.svg
        "Function" => "built_in:symbol-function",
        "Method" => "built_in:symbol-method",
        "Constructor" => "built_in:symbol-constructor",
        "Field" => "built_in:symbol-field",
        "Property" => "built_in:symbol-property",
        "Variable" => "built_in:symbol-variable",
        "Constant" | "EnumMember" => "built_in:symbol-constant",
        "Class" => "built_in:symbol-class",
        "Interface" => "built_in:symbol-interface",
        "Struct" => "built_in:symbol-struct",
        "Enum" => "built_in:symbol-enum",
        "TypeParameter" => "built_in:symbol-type-parameter",
        "Module" | "Namespace" | "Package" => "built_in:symbol-module",
        "Keyword" => "built_in:symbol-keyword",
        "Operator" => "built_in:symbol-operator",
        "Event" => "built_in:symbol-event",
        "Reference" => "built_in:symbol-reference",
        "File" => "built_in:file",
        "Folder" => "built_in:folder",
        "Object" => "built_in:symbol-object",
        "Array" => "built_in:symbol-array",
        "String" => "built_in:symbol-keyword",
        "Number" => "built_in:symbol-operator",
        "Boolean" | "Null" => "built_in:symbol-constant",
        "Key" => "built_in:key",
        _ => "built_in:identifier",
    }
}

fn symbol_tone(kind: &str) -> CommandPaletteItemTone {
    match kind {
        "Function" | "Method" | "Constructor" => CommandPaletteItemTone::Function,
        "Class" | "Struct" | "Interface" | "Enum" | "TypeParameter" => CommandPaletteItemTone::Type,
        "Variable" | "Constant" | "Field" | "Property" | "EnumMember" => {
            CommandPaletteItemTone::Variable
        }
        "Namespace" | "Module" | "Package" => CommandPaletteItemTone::Module,
        _ => CommandPaletteItemTone::Default,
    }
}

/// Every palette-worthy action, VS Code style: real actions only, no
/// per-keystroke motions/editing primitives (those live in the keymap).
/// Both the label and the command id are matched by the filter, so power
/// users can type either "worktree" or "projects.worktrees".
pub(crate) const COMMAND_PALETTE_ACTIONS: &[(&str, &str)] = &[
    // ── Workspace / projects ──────────────────────────────────────────
    ("editor.open_folder", "Open Folder…"),
    ("projects.recent", "Open Recent Project"),
    ("projects.worktrees", "Switch Git Worktree"),
    ("workspace.reload", "Reload Workspace"),
    ("app.new_instance", "New Instance"),
    ("cli.install", "Shell Command: Install 'netherize' in PATH"),
    (
        "cli.uninstall",
        "Shell Command: Uninstall 'netherize' from PATH",
    ),
    // ── Files / search ────────────────────────────────────────────────
    ("editor.save_file", "Save File"),
    ("app.open_file_picker", "Open File Picker"),
    ("app.open_file_finder", "Open File Finder"),
    ("app.search_in_files", "Search In Files"),
    ("app.open_workspace_symbols", "Open Workspace Symbols"),
    ("app.open_document_symbols", "Find Symbol in File"),
    ("app.open_file_history", "Open File History"),
    ("editor.open_in_file_search", "Search In Current File"),
    ("editor.search_next", "Search Next Match"),
    ("editor.search_prev", "Search Previous Match"),
    (
        "editor.search_word_under_cursor",
        "Search Word Under Cursor",
    ),
    ("editor.clear_search_highlights", "Clear Search Highlights"),
    // ── Editing ───────────────────────────────────────────────────────
    ("editor.undo", "Undo"),
    ("editor.redo", "Redo"),
    ("editor.toggle_line_comment", "Toggle Line Comment"),
    (
        "editor.toggle_selection_comment",
        "Toggle Selection Comment",
    ),
    ("editor.paste", "Paste System Clipboard"),
    ("editor.join_lines", "Join Lines"),
    ("editor.toggle_fold", "Toggle Fold"),
    ("editor.toggle_fold_all", "Toggle All Folds"),
    ("multicursor.select_all", "Select All Occurrences"),
    // ── Language / LSP ────────────────────────────────────────────────
    ("lsp.format_document", "Format Document"),
    ("lsp.rename", "Rename Symbol"),
    ("lsp.go_to_definition", "Go to Definition"),
    ("lsp.preview_definition", "Peek Definition"),
    ("lsp.references", "Find References"),
    ("lsp.hover", "Show Hover"),
    ("lsp.code_action", "Code Action"),
    ("lsp.restart", "Restart Language Server"),
    ("lsp.select_python_env", "Change Python Venv"),
    ("lsp.select_dart_env", "Change Dart/Flutter SDK"),
    ("diagnostics.open_picker", "Open Diagnostics"),
    // ── Git / tools ───────────────────────────────────────────────────
    ("git.open_lazygit", "Open Lazygit"),
    ("git.blame_line", "Git Blame Line"),
    ("docker.open_lazydocker", "Open Lazydocker"),
    ("codegraph.open_graph_hud", "Open Code Graph"),
    ("canvas.open", "Open Canvas"),
    ("canvas.auto_arrange", "Canvas: Auto Arrange"),
    ("ai.chat_toggle", "Toggle AI Chat"),
    // ── Runner / LeetCode ─────────────────────────────────────────────
    ("runner.run", "Run Test Cases"),
    ("runner.new_leetcode_file", "New LeetCode File"),
    ("runner.fetch_leetcode_problem", "Fetch LeetCode Problem"),
    // ── View / layout ─────────────────────────────────────────────────
    ("app.open_theme_selector", "Select Theme"),
    ("app.open_settings", "Open Settings"),
    ("app.open_extensions_manager", "Open Extensions"),
    ("app.open_help", "Open Cheat Sheet"),
    ("app.open_vim_command", "Open Vim Command"),
    ("app.toggle_terminal", "Toggle Terminal"),
    ("app.toggle_bottom_dock", "Toggle Bottom Dock"),
    ("app.toggle_left_dock", "Toggle Left Dock"),
    ("app.toggle_maximize_focus", "Toggle Zen Mode"),
    ("view.toggle_minimap", "Toggle Minimap"),
    ("app.toggle_markdown_preview", "Toggle Markdown Preview"),
    ("app.close_sidebars", "Close Sidebars"),
    // ── Focus ─────────────────────────────────────────────────────────
    ("app.focus_editor", "Focus Editor"),
    ("app.focus_explorer", "Focus Explorer"),
    ("app.focus_outline", "Focus Outline"),
    ("app.focus_terminal", "Focus Terminal"),
    ("app.focus_inspector", "Focus Inspector"),
    ("app.focus_left", "Focus Left"),
    ("app.focus_right", "Focus Right"),
    ("app.focus_up", "Focus Up"),
    ("app.focus_down", "Focus Down"),
    ("app.focus_back", "Focus Back"),
    ("app.move_focus_cycle", "Cycle Focus"),
    // ── Buffers / terminal tabs ───────────────────────────────────────
    ("buffer.new", "New Buffer"),
    ("buffer.next", "Next Buffer"),
    ("buffer.prev", "Previous Buffer"),
    ("buffer.close_current", "Close Current Buffer"),
    ("terminal.tab_new", "New Terminal Tab"),
    ("terminal.tab_close", "Close Terminal Tab"),
    ("terminal.search_open", "Search Terminal"),
    // ── Explorer ──────────────────────────────────────────────────────
    ("explorer.create_file", "Explorer: New File"),
    ("explorer.create_folder", "Explorer: New Folder"),
    ("explorer.rename_base", "Explorer: Rename File"),
    ("explorer.copy_file", "Explorer: Copy File"),
    ("explorer.paste_file", "Explorer: Paste File"),
    ("explorer.toggle_hidden", "Explorer: Toggle Hidden Files"),
    ("explorer.toggle_ignored", "Explorer: Toggle Ignored Files"),
    (
        "explorer.toggle_git_changes_only",
        "Explorer: Git Changes Only",
    ),
    ("explorer.start_filter", "Explorer: Filter Files"),
];

/// Shared fuzzy matcher for every in-process picker (commands, recent
/// projects, themes, symbols). Ranking: substring in the primary label
/// (earlier = better) > substring in the secondary text (command id, path…)
/// > in-order subsequence of the label (fewer skipped chars = better).
/// `query` must already be lowercase. `None` = no match.
fn fuzzy_score(label: &str, secondary: &str, query: &str) -> Option<i64> {
    let label_lower = label.to_ascii_lowercase();
    if let Some(pos) = label_lower.find(query) {
        return Some(2000 - pos as i64 * 2);
    }
    if !secondary.is_empty()
        && let Some(pos) = secondary.to_ascii_lowercase().find(query)
    {
        return Some(1000 - pos as i64);
    }
    let mut score = 500i64;
    let mut label_chars = label_lower.chars();
    for query_char in query.chars() {
        loop {
            match label_chars.next() {
                Some(c) if c == query_char => break,
                Some(_) => score -= 1,
                None => return None,
            }
        }
    }
    Some(score)
}

/// The searchable secondary text of a palette item: the visible secondary
/// label (meta suffix after \u{1f} stripped), falling back to the target path
/// for file-opening items so "users/dev" finds a project by its path.
fn item_secondary_text(item: &CommandPaletteItem) -> String {
    let secondary = item
        .secondary_label
        .as_deref()
        .map(|s| s.split('\u{1f}').next().unwrap_or(s))
        .unwrap_or("");
    if !secondary.is_empty() {
        return secondary.to_string();
    }
    match &item.action {
        CommandPaletteAction::OpenFile(path) => path.to_string_lossy().into_owned(),
        _ => String::new(),
    }
}

/// Build the CommandPalette result list. Empty query: RECENT group (persisted
/// most-recently-run ids, deduped out of the main list) followed by the full
/// registry; returns how many leading rows are the RECENT group. Non-empty
/// query: fuzzy-ranked matches, no grouping.
fn command_palette_items(query: &str, recents: &[String]) -> (Vec<CommandPaletteItem>, usize) {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        let mut items = Vec::new();
        for recent_id in recents {
            if let Some((id, label)) = COMMAND_PALETTE_ACTIONS
                .iter()
                .find(|(id, _)| id == recent_id)
            {
                items.push(CommandPaletteItem::command(id, label));
            }
        }
        let recent_rows = items.len();
        for (id, label) in COMMAND_PALETTE_ACTIONS {
            if !recents.iter().any(|recent| recent == id) {
                items.push(CommandPaletteItem::command(id, label));
            }
        }
        return (items, recent_rows);
    }

    let mut scored: Vec<(i64, usize)> = COMMAND_PALETTE_ACTIONS
        .iter()
        .enumerate()
        .filter_map(|(idx, (id, label))| fuzzy_score(label, id, &q).map(|s| (s, idx)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let items = scored
        .into_iter()
        .map(|(_, idx)| {
            let (id, label) = COMMAND_PALETTE_ACTIONS[idx];
            CommandPaletteItem::command(id, label)
        })
        .collect();
    (items, 0)
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
    let command_text = trimmed.strip_prefix(':').unwrap_or(trimmed).trim();
    if command_text.is_empty() {
        // Không gợi ý gì cả — giống nvim thật: chờ người dùng gõ
        return Vec::new();
    }

    // Nếu query là số thuần tuý → jump to line
    if let Ok(n) = command_text.parse::<usize>() {
        return vec![CommandPaletteItem {
            label: format!("Go to line {n}"),
            secondary_label: None,
            icon: None,
            action: CommandPaletteAction::ExecuteVimCommand(trimmed.to_string()),
            tone: CommandPaletteItemTone::Default,
            preview_colors: Vec::new(),
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
/// The ONE panel width every complex picker uses (file picker, command
/// palette, recent projects, themes, live grep, history, symbols) — a picker
/// with a different width immediately reads as a different component.
fn complex_panel_width(overlay_width: f32) -> f32 {
    let available = (overlay_width - 48.0).max(320.0);
    let min_w = (overlay_width * 0.35).max(400.0);
    (overlay_width * 0.62).clamp(min_w.min(available), available)
}

fn palette_max_items(mode: CommandPaletteMode) -> usize {
    match mode {
        CommandPaletteMode::LiveGrep => 10,
        // Visible-row cap only — the result list itself holds the full action
        // registry (see refresh_results) and the panel scrolls through it.
        CommandPaletteMode::CommandPalette => 10,
        mode if mode.is_complex_picker() => 12,
        _ => 10,
    }
}

fn palette_row_height(mode: CommandPaletteMode, line_height: f32) -> f32 {
    match mode {
        CommandPaletteMode::LiveGrep => line_height * 2.0 + 16.0,
        CommandPaletteMode::ThemeSelector => line_height + 18.0,
        CommandPaletteMode::CommandPalette => line_height + 14.0,
        CommandPaletteMode::RecentProjects => line_height * 2.0 + 20.0,
        CommandPaletteMode::DocumentSymbols | CommandPaletteMode::FilePicker => {
            (line_height + 16.0) * 1.5
        }
        _ => line_height + 8.0,
    }
}

fn complex_picker_reserved_height(
    mode: CommandPaletteMode,
    panel_padding: f32,
    line_height: f32,
) -> f32 {
    // Keep this in sync with `src/render/renderer/palette.rs` renderer metrics:
    // badge height (`line + 10`), header bottom pad (12), separator/gap (7),
    // and footer height (`line + 36 + 1`). If this underestimates, scroll
    // math thinks selected rows are visible while the renderer has already
    // clipped them behind the footer.
    let header_gap = 12.0 + 1.0 + 6.0;
    let footer_height = line_height + 37.0;
    let header_height = if mode == CommandPaletteMode::RecentProjects {
        let badge_height = line_height + 10.0 + header_gap;
        let column_header_height = line_height + 1.0 + 4.0;
        badge_height + column_header_height
    } else {
        line_height + 10.0 + header_gap
    };
    panel_padding * 2.0 + header_height + footer_height
}

fn live_grep_reserved_height(panel_padding: f32, line_height: f32) -> f32 {
    let footer_height = line_height + 37.0;
    let header_height = line_height + 10.0 + 12.0 + 1.0 + 6.0;
    panel_padding * 2.0 + header_height + footer_height
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VimCharClass {
    Whitespace,
    Word,
    Punct,
}

fn vim_char_class(c: char) -> VimCharClass {
    if c.is_whitespace() {
        VimCharClass::Whitespace
    } else if c.is_alphanumeric() || c == '_' {
        VimCharClass::Word
    } else {
        VimCharClass::Punct
    }
}

// ── Shared single-line Vim engine (used by CommandPalette + FuzzyState) ──────

fn prev_boundary(s: &str, byte: usize) -> usize {
    if byte == 0 {
        return 0;
    }
    let mut i = byte - 1;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn next_boundary(s: &str, byte: usize) -> usize {
    if byte >= s.len() {
        return s.len();
    }
    let mut i = byte + 1;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

fn vim_word_forward_in(s: &str, byte: usize) -> usize {
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    if chars.is_empty() {
        return 0;
    }
    let mut i = chars
        .iter()
        .position(|(b, _)| *b >= byte)
        .unwrap_or(chars.len());
    if i >= chars.len() {
        return s.len();
    }
    let start_class = vim_char_class(chars[i].1);
    if start_class != VimCharClass::Whitespace {
        while i < chars.len() && vim_char_class(chars[i].1) == start_class {
            i += 1;
        }
    }
    while i < chars.len() && vim_char_class(chars[i].1) == VimCharClass::Whitespace {
        i += 1;
    }
    if i >= chars.len() {
        s.len()
    } else {
        chars[i].0
    }
}

fn vim_word_backward_in(s: &str, byte: usize) -> usize {
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    if chars.is_empty() || byte == 0 {
        return 0;
    }
    let mut i = chars
        .iter()
        .position(|(b, _)| *b >= byte)
        .unwrap_or(chars.len());
    if i == 0 {
        return 0;
    }
    i -= 1;
    while i > 0 && vim_char_class(chars[i].1) == VimCharClass::Whitespace {
        i -= 1;
    }
    let cls = vim_char_class(chars[i].1);
    while i > 0 && vim_char_class(chars[i - 1].1) == cls {
        i -= 1;
    }
    chars[i].0
}

fn vim_word_end_in(s: &str, byte: usize) -> usize {
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    if chars.is_empty() {
        return 0;
    }
    let mut i = chars
        .iter()
        .position(|(b, _)| *b >= byte)
        .unwrap_or(chars.len());
    i += 1; // move at least one forward (vim `e`)
    while i < chars.len() && vim_char_class(chars[i].1) == VimCharClass::Whitespace {
        i += 1;
    }
    if i >= chars.len() {
        return chars.last().map(|(b, _)| *b).unwrap_or(0);
    }
    let cls = vim_char_class(chars[i].1);
    while i + 1 < chars.len() && vim_char_class(chars[i + 1].1) == cls {
        i += 1;
    }
    chars[i].0
}

fn normalized_range(s: &str, range: Option<(usize, usize)>) -> Option<(usize, usize)> {
    range.and_then(|(start, end)| {
        let start = start.min(s.len());
        let end = end.min(s.len());
        (start < end).then_some((start, end))
    })
}

fn first_non_whitespace(s: &str) -> usize {
    s.char_indices()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(b, _)| b)
        .unwrap_or(0)
}

/// The single, struct-agnostic Vim state machine. Operates entirely on the
/// borrowed [`VimLineView`]; never touches result lists or async search — the
/// owner refreshes those when `VimLineOutcome::text_changed` is true.
pub fn vim_line_input(
    view: &mut VimLineView<'_>,
    key: PaletteVimKey,
    has_result_list: bool,
) -> VimLineOutcome {
    match *view.vim_mode {
        PaletteVimMode::Insert => match key {
            PaletteVimKey::Esc => {
                *view.vim_mode = PaletteVimMode::Normal;
                *view.selection_range = None;
                *view.pending_operator = None;
                if *view.cursor_byte > 0 {
                    *view.cursor_byte = prev_boundary(view.query, *view.cursor_byte);
                }
                VimLineOutcome::action(PaletteVimAction::Consumed)
            }
            PaletteVimKey::Enter => VimLineOutcome::action(PaletteVimAction::Confirm),
            PaletteVimKey::Char(_) => VimLineOutcome::action(PaletteVimAction::Ignore),
        },
        PaletteVimMode::Normal => vim_line_input_normal(view, key, has_result_list),
        PaletteVimMode::Visual => vim_line_input_visual(view, key),
    }
}

fn vim_line_input_normal(
    view: &mut VimLineView<'_>,
    key: PaletteVimKey,
    has_result_list: bool,
) -> VimLineOutcome {
    let c = match key {
        PaletteVimKey::Enter => return VimLineOutcome::action(PaletteVimAction::Confirm),
        PaletteVimKey::Esc => {
            if view.pending_operator.take().is_some() {
                return VimLineOutcome::action(PaletteVimAction::Consumed);
            }
            return VimLineOutcome::action(PaletteVimAction::Close);
        }
        PaletteVimKey::Char(c) => c,
    };

    if view.pending_operator.is_some() {
        return vim_apply_operator_motion(view, c);
    }

    let consumed = VimLineOutcome::action(PaletteVimAction::Consumed);
    match c {
        'h' => {
            if *view.cursor_byte > 0 {
                *view.cursor_byte = prev_boundary(view.query, *view.cursor_byte);
            }
            consumed
        }
        'l' => {
            if *view.cursor_byte < view.query.len() {
                *view.cursor_byte = next_boundary(view.query, *view.cursor_byte);
            }
            consumed
        }
        'w' => {
            *view.cursor_byte = vim_word_forward_in(view.query, *view.cursor_byte);
            consumed
        }
        'b' => {
            *view.cursor_byte = vim_word_backward_in(view.query, *view.cursor_byte);
            consumed
        }
        'e' => {
            *view.cursor_byte = vim_word_end_in(view.query, *view.cursor_byte);
            consumed
        }
        '0' => {
            *view.cursor_byte = 0;
            consumed
        }
        '^' => {
            *view.cursor_byte = first_non_whitespace(view.query);
            consumed
        }
        '$' => {
            *view.cursor_byte = view.query.len();
            consumed
        }
        'i' => {
            *view.vim_mode = PaletteVimMode::Insert;
            consumed
        }
        'a' => {
            if *view.cursor_byte < view.query.len() {
                *view.cursor_byte = next_boundary(view.query, *view.cursor_byte);
            }
            *view.vim_mode = PaletteVimMode::Insert;
            consumed
        }
        'I' => {
            *view.cursor_byte = first_non_whitespace(view.query);
            *view.vim_mode = PaletteVimMode::Insert;
            consumed
        }
        'A' => {
            *view.cursor_byte = view.query.len();
            *view.vim_mode = PaletteVimMode::Insert;
            consumed
        }
        'x' => {
            if *view.cursor_byte < view.query.len() {
                let end = next_boundary(view.query, *view.cursor_byte);
                *view.register = view.query[*view.cursor_byte..end].to_string();
                view.query.replace_range(*view.cursor_byte..end, "");
                if *view.cursor_byte > view.query.len() {
                    *view.cursor_byte = view.query.len();
                }
                *view.selected_index = 0;
                return VimLineOutcome::changed(PaletteVimAction::Consumed);
            }
            consumed
        }
        'v' => {
            *view.vim_mode = PaletteVimMode::Visual;
            *view.selection_range = Some((*view.cursor_byte, *view.cursor_byte));
            consumed
        }
        'd' | 'c' | 'y' => {
            *view.pending_operator = Some(match c {
                'd' => PaletteVimOperator::Delete,
                'c' => PaletteVimOperator::Change,
                _ => PaletteVimOperator::Yank,
            });
            consumed
        }
        'p' | 'P' => vim_paste_register(view, c == 'p'),
        // `q` closes the palette in Normal (matches the fuzzy picker's q/Esc),
        // for quick keyboard dismissal of list pickers.
        'q' => VimLineOutcome::action(PaletteVimAction::Close),
        'j' => {
            if has_result_list {
                VimLineOutcome::action(PaletteVimAction::ListNext)
            } else {
                VimLineOutcome::action(PaletteVimAction::Ignore)
            }
        }
        'k' => {
            if has_result_list {
                VimLineOutcome::action(PaletteVimAction::ListPrev)
            } else {
                VimLineOutcome::action(PaletteVimAction::Ignore)
            }
        }
        _ => VimLineOutcome::action(PaletteVimAction::Ignore),
    }
}

fn vim_apply_operator_motion(view: &mut VimLineView<'_>, motion: char) -> VimLineOutcome {
    let op = match view.pending_operator.take() {
        Some(op) => op,
        None => return VimLineOutcome::action(PaletteVimAction::Consumed),
    };
    let start = *view.cursor_byte;
    let target = match (op, motion) {
        (_, 'w') => vim_word_forward_in(view.query, start),
        (_, 'b') => vim_word_backward_in(view.query, start),
        (_, 'e') => next_boundary(view.query, vim_word_end_in(view.query, start)),
        (_, '$') => view.query.len(),
        (_, '0' | '^') => 0,
        _ => return VimLineOutcome::action(PaletteVimAction::Consumed), // unsupported motion
    };
    let (lo, hi) = if target >= start {
        (start, target)
    } else {
        (target, start)
    };
    let lo = lo.min(view.query.len());
    let hi = hi.min(view.query.len());
    if lo == hi {
        return VimLineOutcome::action(PaletteVimAction::Consumed);
    }
    *view.register = view.query[lo..hi].to_string();
    match op {
        PaletteVimOperator::Yank => {
            *view.cursor_byte = lo;
            VimLineOutcome::action(PaletteVimAction::Consumed)
        }
        PaletteVimOperator::Delete | PaletteVimOperator::Change => {
            view.query.replace_range(lo..hi, "");
            *view.cursor_byte = lo;
            *view.selected_index = 0;
            if matches!(op, PaletteVimOperator::Change) {
                *view.vim_mode = PaletteVimMode::Insert;
            }
            VimLineOutcome::changed(PaletteVimAction::Consumed)
        }
    }
}

fn vim_paste_register(view: &mut VimLineView<'_>, after: bool) -> VimLineOutcome {
    if view.register.is_empty() {
        return VimLineOutcome::action(PaletteVimAction::Consumed);
    }
    let at = if after && *view.cursor_byte < view.query.len() {
        next_boundary(view.query, *view.cursor_byte)
    } else {
        *view.cursor_byte
    };
    let reg = view.register.clone();
    view.query.insert_str(at, &reg);
    *view.cursor_byte = at + reg.len();
    *view.selected_index = 0;
    VimLineOutcome::changed(PaletteVimAction::Consumed)
}

fn vim_line_input_visual(view: &mut VimLineView<'_>, key: PaletteVimKey) -> VimLineOutcome {
    let anchor = view
        .selection_range
        .map(|(a, _)| a)
        .unwrap_or(*view.cursor_byte);
    let c = match key {
        PaletteVimKey::Enter => return VimLineOutcome::action(PaletteVimAction::Confirm),
        PaletteVimKey::Esc => {
            *view.vim_mode = PaletteVimMode::Normal;
            *view.selection_range = None;
            return VimLineOutcome::action(PaletteVimAction::Consumed);
        }
        PaletteVimKey::Char(c) => c,
    };

    let moved = match c {
        'h' => {
            if *view.cursor_byte > 0 {
                *view.cursor_byte = prev_boundary(view.query, *view.cursor_byte);
            }
            true
        }
        'l' => {
            if *view.cursor_byte < view.query.len() {
                *view.cursor_byte = next_boundary(view.query, *view.cursor_byte);
            }
            true
        }
        'w' => {
            *view.cursor_byte = vim_word_forward_in(view.query, *view.cursor_byte);
            true
        }
        'b' => {
            *view.cursor_byte = vim_word_backward_in(view.query, *view.cursor_byte);
            true
        }
        'e' => {
            *view.cursor_byte =
                next_boundary(view.query, vim_word_end_in(view.query, *view.cursor_byte));
            true
        }
        '0' => {
            *view.cursor_byte = 0;
            true
        }
        '$' => {
            *view.cursor_byte = view.query.len();
            true
        }
        _ => false,
    };
    if moved {
        *view.selection_range = Some((anchor, *view.cursor_byte));
        return VimLineOutcome::action(PaletteVimAction::Consumed);
    }

    if matches!(c, 'd' | 'c' | 'y') {
        if let Some((lo, hi)) = normalized_range(view.query, *view.selection_range) {
            *view.register = view.query[lo..hi].to_string();
            let text_changed = c != 'y';
            if c == 'y' {
                *view.cursor_byte = lo;
            } else {
                view.query.replace_range(lo..hi, "");
                *view.cursor_byte = lo;
                *view.selected_index = 0;
            }
            *view.selection_range = None;
            *view.vim_mode = if c == 'c' {
                PaletteVimMode::Insert
            } else {
                PaletteVimMode::Normal
            };
            return VimLineOutcome {
                action: PaletteVimAction::Consumed,
                text_changed,
            };
        }
        return VimLineOutcome::action(PaletteVimAction::Consumed);
    }

    VimLineOutcome::action(PaletteVimAction::Ignore)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::theme_config::ThemeConfig, core::commands::PaletteVimKey};

    fn make_item(label: &str) -> CommandPaletteItem {
        CommandPaletteItem::command("test.command", label)
    }

    fn make_theme_item(name: &str) -> CommandPaletteItem {
        CommandPaletteItem {
            label: name.to_string(),
            secondary_label: None,
            icon: None,
            action: CommandPaletteAction::SelectTheme(name.to_string()),
            tone: CommandPaletteItemTone::Default,
            preview_colors: Vec::new(),
        }
    }

    #[test]
    fn command_palette_actions_are_valid_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for (id, label) in COMMAND_PALETTE_ACTIONS {
            assert!(
                crate::core::command_ids::is_valid(id),
                "palette action '{id}' ({label}) is not a registered command id"
            );
            assert!(seen.insert(*id), "duplicate palette action id '{id}'");
        }
        // The regressions this table exists for: discoverability of the new
        // dev-workflow commands.
        for required in [
            "projects.worktrees",
            "app.open_theme_selector",
            "cli.install",
            "app.open_settings",
            "lsp.format_document",
        ] {
            assert!(seen.contains(required), "palette must list '{required}'");
        }
    }

    #[test]
    fn command_palette_filter_matches_command_id_too() {
        let (items, _) = command_palette_items("worktree", &[]);
        assert!(
            items.iter().any(|item| item.label == "Switch Git Worktree"),
            "typing part of the id must surface the command"
        );
        let (all, recent_rows) = command_palette_items("", &[]);
        assert_eq!(
            all.len(),
            COMMAND_PALETTE_ACTIONS.len(),
            "empty query lists the full registry"
        );
        assert_eq!(recent_rows, 0);
    }

    #[test]
    fn command_palette_fuzzy_matches_and_ranks() {
        // Substring lands the obvious hit on top.
        let (items, _) = command_palette_items("zen", &[]);
        assert_eq!(items[0].label, "Toggle Zen Mode");

        // In-order subsequence with gaps still matches ("tglzen").
        let (items, _) = command_palette_items("tglzen", &[]);
        assert!(
            items.iter().any(|item| item.label == "Toggle Zen Mode"),
            "subsequence query must match"
        );

        // Garbage matches nothing.
        let (items, _) = command_palette_items("qqxxzz", &[]);
        assert!(items.is_empty());
    }

    #[test]
    fn command_palette_recents_group_on_top_without_duplicates() {
        let recents = vec![
            "app.open_settings".to_string(),
            "editor.undo".to_string(),
            "gone.command".to_string(), // stale id — silently dropped
        ];
        let (items, recent_rows) = command_palette_items("", &recents);

        assert_eq!(recent_rows, 2);
        assert_eq!(items[0].label, "Open Settings");
        assert_eq!(items[1].label, "Undo");
        assert_eq!(
            items.len(),
            COMMAND_PALETTE_ACTIONS.len(),
            "recents are moved, not duplicated"
        );
        assert_eq!(items.iter().filter(|item| item.label == "Undo").count(), 1);

        // Typing a query dissolves the group.
        let (_, recent_rows) = command_palette_items("undo", &recents);
        assert_eq!(recent_rows, 0);
    }

    #[test]
    fn all_complex_pickers_share_one_panel_width() {
        let theme = ThemeConfig::builtin_dark();
        let overlay = [0.0, 0.0, 1400.0, 900.0];
        let width_of = |mode: CommandPaletteMode| {
            let mut palette = CommandPalette::default();
            palette.open_with_items(mode, vec![make_item("row")]);
            palette
                .render(&theme, overlay)
                .expect("render model")
                .panel_bounds[2]
        };

        let file_picker = width_of(CommandPaletteMode::FilePicker);
        for mode in [
            CommandPaletteMode::CommandPalette,
            CommandPaletteMode::RecentProjects,
            CommandPaletteMode::ThemeSelector,
            CommandPaletteMode::LiveGrep,
            CommandPaletteMode::DocumentSymbols,
            CommandPaletteMode::FileHistory,
        ] {
            assert_eq!(
                width_of(mode),
                file_picker,
                "{mode:?} must share the file picker's panel width"
            );
        }
    }

    #[test]
    fn recent_projects_filter_is_fuzzy_over_name_and_path() {
        let mut palette = CommandPalette::default();
        palette.open_with_items(
            CommandPaletteMode::RecentProjects,
            vec![
                CommandPaletteItem::recent_project(std::path::Path::new(
                    "/Users/dev/other-project",
                )),
                CommandPaletteItem::recent_project(std::path::Path::new(
                    "/Users/dev/netherize_editor",
                )),
            ],
        );

        // Subsequence on the project name ("nthrz" ⊂ "netherize_editor").
        palette.query = "nthrz".to_string();
        palette.refresh_results(None);
        assert_eq!(palette.results.len(), 1);
        assert_eq!(palette.results[0].label, "netherize_editor");

        // Substring on the full path still matches too.
        palette.query = "users/dev".to_string();
        palette.refresh_results(None);
        assert_eq!(palette.results.len(), 2);
    }

    #[test]
    fn theme_selector_filter_is_fuzzy() {
        let mut palette = CommandPalette::default();
        palette.open_with_items(
            CommandPaletteMode::ThemeSelector,
            vec![
                make_theme_item("gruvbox-dark"),
                make_theme_item("nether-dark"),
            ],
        );

        palette.query = "gvbx".to_string();
        palette.refresh_results(None);
        assert_eq!(palette.results.len(), 1);
        assert_eq!(palette.results[0].label, "gruvbox-dark");
    }

    #[test]
    fn command_palette_render_model_carries_group_labels() {
        let theme = ThemeConfig::builtin_dark();
        let mut palette = CommandPalette::default();
        palette.set_recent_commands(vec!["editor.undo".to_string()]);
        palette.open(CommandPaletteMode::CommandPalette, None);

        let model = palette
            .render(&theme, [0.0, 0.0, 1400.0, 900.0])
            .expect("render model");
        assert_eq!(
            model.row_group_labels.first().map(String::as_str),
            Some("RECENT")
        );
        assert_eq!(
            model.row_group_labels.get(1).map(String::as_str),
            Some("COMMANDS")
        );
        assert_eq!(model.row_group_labels.len(), model.result_labels.len());
        assert_eq!(
            model.secondary_labels.first().map(String::as_str),
            Some("editor.undo"),
            "command id shows as the row's secondary label"
        );
    }

    #[test]
    fn promote_theme_moves_current_to_front_and_selects_it() {
        let mut palette = CommandPalette::default();
        palette.open_with_items(
            CommandPaletteMode::ThemeSelector,
            vec![
                make_theme_item("aurora"),
                make_theme_item("Nether-Dark"),
                make_theme_item("zephyr"),
            ],
        );
        palette.selected_index = 2;

        assert!(palette.promote_theme("nether-dark"));

        assert_eq!(palette.results[0].label, "Nether-Dark");
        assert_eq!(palette.selected_index, 0);
        assert_eq!(palette.results.len(), 3);
        // Unknown profile: no reorder, no panic.
        assert!(!palette.promote_theme("missing-theme"));
    }

    #[test]
    fn title_override_shows_in_render_and_clears_on_close() {
        let theme = ThemeConfig::builtin_dark();
        let mut palette = CommandPalette::default();
        palette.open_with_items(
            CommandPaletteMode::RecentProjects,
            vec![make_item("worktree-a")],
        );
        palette.set_title_override(Some("WORKTREES".to_string()));

        let model = palette
            .render(&theme, [0.0, 0.0, 1200.0, 800.0])
            .expect("render model");
        assert_eq!(model.title, "WORKTREES");

        palette.close();
        palette.open_with_items(CommandPaletteMode::RecentProjects, vec![make_item("proj")]);
        let model = palette
            .render(&theme, [0.0, 0.0, 1200.0, 800.0])
            .expect("render model");
        assert_eq!(
            model.title,
            CommandPaletteMode::RecentProjects.title(),
            "override must not leak into the next open"
        );
    }

    #[test]
    fn theme_selector_panel_stays_translucent_for_live_preview() {
        let theme = ThemeConfig::builtin_dark();
        let mut palette = CommandPalette::default();
        palette.open_with_items(
            CommandPaletteMode::ThemeSelector,
            vec![make_theme_item("aurora")],
        );

        let model = palette
            .render(&theme, [0.0, 0.0, 1200.0, 800.0])
            .expect("render model");
        // The whole point of the picker is seeing the previewed theme through
        // and around the panel — panel translucent, scrim near-invisible.
        assert!(
            model.panel_bg[3] <= 0.75,
            "theme picker panel must stay translucent, got alpha {}",
            model.panel_bg[3]
        );
        assert!(
            model.scrim_color[3] <= 0.15,
            "theme picker scrim must stay light, got alpha {}",
            model.scrim_color[3]
        );
    }

    #[test]
    fn command_palette_lists_everything_but_panel_stays_short() {
        let theme = ThemeConfig::builtin_dark();
        let mut palette = CommandPalette::default();
        palette.open(CommandPaletteMode::CommandPalette, None);

        let model = palette
            .render(&theme, [0.0, 0.0, 1400.0, 900.0])
            .expect("render model");
        assert_eq!(
            model.total_results,
            COMMAND_PALETTE_ACTIONS.len(),
            "full registry stays in the result list"
        );
        assert!(
            model.panel_bounds[3] <= 900.0 * 0.62,
            "panel must not swallow the screen, got height {} of 900",
            model.panel_bounds[3]
        );
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
        assert_eq!(model.scroll_offset_rows, 6);
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

        // 744 = complex_panel_width(1200) — the single shared picker width.
        assert_eq!(empty_model.panel_bounds, [228.0, 325.0, 744.0, 150.0]);
        assert_eq!(populated_model.panel_bounds, [228.0, 40.0, 744.0, 720.0]);
        assert_eq!(
            empty_model.scroll_offset_rows,
            populated_model.scroll_offset_rows
        );
    }

    #[test]
    fn render_uses_opaque_panel_background_separate_from_scrim() {
        let palette = CommandPalette {
            mode: CommandPaletteMode::FilePicker,
            is_visible: true,
            results: vec![make_item("default-dark")],
            ..CommandPalette::default()
        };

        let theme = ThemeConfig::builtin_dark();
        let model = palette
            .render(&theme, [0.0, 0.0, 1200.0, 800.0])
            .expect("render model");

        let expected_panel = theme.ui.panel_bg.as_f32();
        let expected_scrim = theme.ui.overlay_bg.as_f32();
        assert_eq!(model.panel_bg[0], expected_panel[0]);
        assert_eq!(model.panel_bg[1], expected_panel[1]);
        assert_eq!(model.panel_bg[2], expected_panel[2]);
        assert!(model.panel_bg[3] >= 0.98);
        assert_eq!(model.scrim_color[0], expected_scrim[0]);
        assert_eq!(model.scrim_color[1], expected_scrim[1]);
        assert_eq!(model.scrim_color[2], expected_scrim[2]);
        assert!(model.scrim_color[3] >= 0.72);
    }

    #[test]
    fn paste_overlay_render_exposes_vim_mode_label_and_block_caret() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("docker-compose (1).yml", None);

        // Insert (default): thin caret, INSERT label.
        let insert = p
            .render(&ThemeConfig::builtin_dark(), [0.0, 0.0, 1200.0, 800.0])
            .expect("insert render model");
        assert_eq!(insert.vim_mode_label, Some("INSERT"));
        assert!(!insert.vim_caret_block);

        // After Esc -> Normal: block caret, NORMAL label.
        p.vim_input(PaletteVimKey::Esc, false, None);
        let normal = p
            .render(&ThemeConfig::builtin_dark(), [0.0, 0.0, 1200.0, 800.0])
            .expect("normal render model");
        assert_eq!(normal.vim_mode_label, Some("NORMAL"));
        assert!(normal.vim_caret_block);
    }

    #[test]
    fn paste_overlay_render_exposes_vim_mode_color_per_mode() {
        let theme = ThemeConfig::builtin_dark();
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("docker-compose (1).yml", None);

        // Insert: color must match theme.ui.mode_insert.
        let insert = p
            .render(&theme, [0.0, 0.0, 1200.0, 800.0])
            .expect("insert render model");
        assert_eq!(insert.vim_mode_color, Some(theme.ui.mode_insert.as_f32()));

        // Normal: color must match theme.ui.mode_normal.
        p.vim_input(PaletteVimKey::Esc, false, None);
        let normal = p
            .render(&theme, [0.0, 0.0, 1200.0, 800.0])
            .expect("normal render model");
        assert_eq!(normal.vim_mode_color, Some(theme.ui.mode_normal.as_f32()));
    }

    #[test]
    fn document_symbols_render_loading_state_then_filter_static_results() {
        let mut palette = CommandPalette::default();
        palette.open_with_items(CommandPaletteMode::DocumentSymbols, Vec::new());
        palette.set_loading(true);

        let loading_model = palette
            .render(&ThemeConfig::builtin_dark(), [0.0, 0.0, 1200.0, 800.0])
            .expect("loading render model");
        assert!(loading_model.is_loading);
        assert_eq!(loading_model.prompt_query, "loading symbols...");

        let symbols = vec![
            crate::async_runtime::message::LspDocumentSymbol {
                name: "render_file".to_string(),
                kind: "Function".to_string(),
                range: crate::async_runtime::message::LspRange {
                    start: crate::async_runtime::message::LspPosition {
                        line: 9,
                        character: 4,
                    },
                    end: crate::async_runtime::message::LspPosition {
                        line: 12,
                        character: 1,
                    },
                },
                ancestors: Vec::new(),
            },
            crate::async_runtime::message::LspDocumentSymbol {
                name: "AppState".to_string(),
                kind: "Struct".to_string(),
                range: crate::async_runtime::message::LspRange {
                    start: crate::async_runtime::message::LspPosition {
                        line: 20,
                        character: 0,
                    },
                    end: crate::async_runtime::message::LspPosition {
                        line: 40,
                        character: 1,
                    },
                },
                ancestors: Vec::new(),
            },
        ];
        let items = symbols
            .iter()
            .map(CommandPaletteItem::document_symbol)
            .collect();
        assert!(palette.replace_static_results(items));
        assert!(!palette.is_loading);

        palette.set_query("state", None);
        assert_eq!(palette.results.len(), 1);
        // Label is the bare symbol name (so rows align); the kind icon and the
        // kind/line metadata are carried separately.
        assert_eq!(palette.results[0].label, "AppState");
        assert_eq!(
            palette.results[0].icon.as_deref(),
            Some("built_in:symbol-struct")
        );
        assert_eq!(
            palette.results[0].secondary_label.as_deref(),
            Some("Struct  ·  Ln 21, Col 1")
        );
    }

    #[test]
    fn document_symbol_item_carries_jump_position_and_tone() {
        let symbol = crate::async_runtime::message::LspDocumentSymbol {
            name: "build_picker".to_string(),
            kind: "Function".to_string(),
            range: crate::async_runtime::message::LspRange {
                start: crate::async_runtime::message::LspPosition {
                    line: 42,
                    character: 8,
                },
                end: crate::async_runtime::message::LspPosition {
                    line: 43,
                    character: 1,
                },
            },
            ancestors: Vec::new(),
        };

        let item = CommandPaletteItem::document_symbol(&symbol);
        assert_eq!(item.label, "build_picker");
        assert_eq!(item.icon.as_deref(), Some("built_in:symbol-function"));
        assert_eq!(
            item.secondary_label.as_deref(),
            Some("Function  ·  Ln 43, Col 9")
        );
        assert_eq!(item.tone, CommandPaletteItemTone::Function);
        assert_eq!(
            item.action,
            CommandPaletteAction::JumpToDocumentSymbol {
                name: "build_picker".to_string(),
                line: 42,
                column: 8,
            }
        );
    }

    #[test]
    fn open_with_items_resets_vim_mode_to_insert() {
        let mut palette = CommandPalette::default();
        // A previous session left the palette parked in Normal mode.
        palette.vim_mode = PaletteVimMode::Normal;

        palette.open_with_items(CommandPaletteMode::RecentProjects, Vec::new());

        // Every fresh open must start in Insert so the user can type to filter.
        assert_eq!(palette.vim_mode, PaletteVimMode::Insert);
        assert_eq!(palette.pending_operator, None);
    }

    #[test]
    fn cursor_moves_left_and_right() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.append_query("file (1).txt", None);
        assert_eq!(p.cursor_byte, "file (1).txt".len());

        p.move_cursor_left();
        assert_eq!(p.cursor_byte, "file (1).tx".len());

        p.move_cursor_right();
        assert_eq!(p.cursor_byte, "file (1).txt".len());
    }

    #[test]
    fn arrow_clears_selection_and_moves_to_edge() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("file (1).txt", None);
        p.set_selection_range(Some((0, "file (1).txt".len())));

        p.move_cursor_left();
        assert_eq!(p.cursor_byte, 0);
        assert!(p.selection_range.is_none());

        p.set_selection_range(Some((0, "file (1).txt".len())));
        p.move_cursor_right();
        assert_eq!(p.cursor_byte, "file (1).txt".len());
        assert!(p.selection_range.is_none());
    }

    #[test]
    fn cursor_moves_to_start_and_end() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.append_query("file (1).txt", None);
        assert_eq!(p.cursor_byte, "file (1).txt".len());

        p.move_cursor_to_start();
        assert_eq!(p.cursor_byte, 0);

        p.move_cursor_to_end();
        assert_eq!(p.cursor_byte, "file (1).txt".len());
    }

    #[test]
    fn cursor_to_start_end_clears_selection() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("file (1).txt", None);
        p.set_selection_range(Some((0, "file (1).txt".len())));

        p.move_cursor_to_start();
        assert_eq!(p.cursor_byte, 0);
        assert!(p.selection_range.is_none());

        p.set_selection_range(Some((0, "file (1).txt".len())));
        p.move_cursor_to_end();
        assert_eq!(p.cursor_byte, "file (1).txt".len());
        assert!(p.selection_range.is_none());
    }

    #[test]
    fn cursor_respects_utf8_boundaries() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        // "文件" is 6 bytes (3 bytes per CJK character) plus ASCII suffix.
        p.set_query("文件.txt", None);
        // Cursor starts at end (10 bytes: 3 + 3 + 1 + 3).
        assert_eq!(p.cursor_byte, "文件.txt".len());

        // Step left through the ASCII suffix one char at a time.
        p.move_cursor_left(); // before 't' (byte 9)
        assert_eq!(p.cursor_byte, 9);
        p.move_cursor_left(); // before 'x' (byte 8)
        assert_eq!(p.cursor_byte, 8);
        p.move_cursor_left(); // before 't' (byte 7)
        assert_eq!(p.cursor_byte, 7);

        // Next step jumps over the '.' to before it (byte 6).
        p.move_cursor_left();
        assert_eq!(p.cursor_byte, 6);

        // Next step jumps over the whole CJK char '件' (3 bytes) to byte 3.
        p.move_cursor_left();
        assert_eq!(p.cursor_byte, 3);

        // Next step jumps over '文' to byte 0.
        p.move_cursor_left();
        assert_eq!(p.cursor_byte, 0);

        // Step right over one CJK char (3 bytes).
        p.move_cursor_right();
        assert_eq!(p.cursor_byte, 3);

        // Delete forward from before '件' should remove '件'.
        p.cursor_byte = 3;
        p.delete_char_forward(None);
        assert_eq!(p.query, "文.txt");
    }

    #[test]
    fn vim_word_motions_ascii() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("foo bar.baz qux", None);
        // forward from 0: "foo" -> "bar"
        assert_eq!(vim_word_forward_in(&p.query, 0), 4);
        // forward from 4 ("bar"): punctuation '.' is its own word
        assert_eq!(vim_word_forward_in(&p.query, 4), 7);
        // word end from 0: end of "foo" is the 'o' at byte 2
        assert_eq!(vim_word_end_in(&p.query, 0), 2);
        // backward from 4 ("bar") -> start of "foo"
        assert_eq!(vim_word_backward_in(&p.query, 4), 0);
    }

    #[test]
    fn vim_esc_enters_normal_and_clamps_left() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("abc", None);
        p.cursor_byte = 3;
        let action = p.vim_input(PaletteVimKey::Esc, false, None);
        assert_eq!(action, PaletteVimAction::Consumed);
        assert_eq!(p.vim_mode, PaletteVimMode::Normal);
        assert_eq!(p.cursor_byte, 2); // clamped one char left
    }

    #[test]
    fn vim_normal_hl_and_word_motions() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("foo bar", None);
        p.vim_input(PaletteVimKey::Esc, false, None); // -> Normal, cursor 6
        p.cursor_byte = 0;
        p.vim_input(PaletteVimKey::Char('l'), false, None);
        assert_eq!(p.cursor_byte, 1);
        p.vim_input(PaletteVimKey::Char('h'), false, None);
        assert_eq!(p.cursor_byte, 0);
        p.vim_input(PaletteVimKey::Char('w'), false, None);
        assert_eq!(p.cursor_byte, 4); // start of "bar"
        p.vim_input(PaletteVimKey::Char('$'), false, None);
        assert_eq!(p.cursor_byte, "foo bar".len());
        p.vim_input(PaletteVimKey::Char('0'), false, None);
        assert_eq!(p.cursor_byte, 0);
    }

    #[test]
    fn vim_insert_transitions_and_x() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("abc", None);
        p.vim_input(PaletteVimKey::Esc, false, None); // Normal, cursor 2
        p.cursor_byte = 0;
        p.vim_input(PaletteVimKey::Char('x'), false, None);
        assert_eq!(p.query, "bc");
        assert_eq!(p.vim_register, "a");
        p.vim_input(PaletteVimKey::Char('A'), false, None);
        assert_eq!(p.vim_mode, PaletteVimMode::Insert);
        assert_eq!(p.cursor_byte, "bc".len());
    }

    #[test]
    fn vim_jk_in_list_picker_returns_list_actions() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::FilePicker, None);
        p.set_query("x", None);
        p.vim_input(PaletteVimKey::Esc, false, None); // Normal
        assert_eq!(
            p.vim_input(PaletteVimKey::Char('j'), true, None),
            PaletteVimAction::ListNext
        );
        assert_eq!(
            p.vim_input(PaletteVimKey::Char('k'), true, None),
            PaletteVimAction::ListPrev
        );
        // single-line prompt: j/k ignored
        assert_eq!(
            p.vim_input(PaletteVimKey::Char('j'), false, None),
            PaletteVimAction::Ignore
        );
    }

    #[test]
    fn vim_enter_returns_confirm_esc_normal_returns_close() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("a", None);
        assert_eq!(
            p.vim_input(PaletteVimKey::Enter, false, None),
            PaletteVimAction::Confirm
        );
        p.vim_input(PaletteVimKey::Esc, false, None); // Insert -> Normal
        assert_eq!(
            p.vim_input(PaletteVimKey::Esc, false, None),
            PaletteVimAction::Close
        );
    }

    #[test]
    fn vim_dw_cw_yw_and_paste() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("foo bar baz", None);
        p.vim_input(PaletteVimKey::Esc, false, None);
        p.cursor_byte = 0;
        // dw deletes "foo " -> "bar baz", register = "foo "
        p.vim_input(PaletteVimKey::Char('d'), false, None);
        p.vim_input(PaletteVimKey::Char('w'), false, None);
        assert_eq!(p.query, "bar baz");
        assert_eq!(p.vim_register, "foo ");
        // p pastes register after cursor
        p.cursor_byte = p.query.len();
        p.vim_input(PaletteVimKey::Char('p'), false, None);
        assert_eq!(p.query, "bar bazfoo ");
    }

    #[test]
    fn vim_cw_enters_insert() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("foo bar", None);
        p.vim_input(PaletteVimKey::Esc, false, None);
        p.cursor_byte = 0;
        p.vim_input(PaletteVimKey::Char('c'), false, None);
        p.vim_input(PaletteVimKey::Char('w'), false, None);
        assert_eq!(p.query, "bar"); // "foo " removed (cw)
        assert_eq!(p.vim_mode, PaletteVimMode::Insert);
    }

    #[test]
    fn vim_d_dollar_deletes_to_end() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("hello world", None);
        p.vim_input(PaletteVimKey::Esc, false, None);
        p.cursor_byte = 5;
        p.vim_input(PaletteVimKey::Char('d'), false, None);
        p.vim_input(PaletteVimKey::Char('$'), false, None);
        assert_eq!(p.query, "hello");
        assert_eq!(p.vim_register, " world");
    }

    #[test]
    fn vim_visual_select_and_delete() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("foo bar", None);
        p.vim_input(PaletteVimKey::Esc, false, None);
        p.cursor_byte = 0;
        p.vim_input(PaletteVimKey::Char('v'), false, None);
        assert_eq!(p.vim_mode, PaletteVimMode::Visual);
        p.vim_input(PaletteVimKey::Char('w'), false, None); // extend to "bar"
        p.vim_input(PaletteVimKey::Char('d'), false, None); // delete selection
        assert_eq!(p.query, "bar");
        assert_eq!(p.vim_mode, PaletteVimMode::Normal);
        assert_eq!(p.vim_register, "foo ");
    }

    #[test]
    fn vim_visual_yank_keeps_text() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("hello", None);
        p.vim_input(PaletteVimKey::Esc, false, None);
        p.cursor_byte = 0;
        p.vim_input(PaletteVimKey::Char('v'), false, None);
        p.vim_input(PaletteVimKey::Char('$'), false, None);
        p.vim_input(PaletteVimKey::Char('y'), false, None);
        assert_eq!(p.query, "hello");
        assert_eq!(p.vim_mode, PaletteVimMode::Normal);
        assert_eq!(p.vim_register, "hello");
    }

    #[test]
    fn vim_visual_esc_returns_to_normal() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("ab", None);
        p.vim_input(PaletteVimKey::Esc, false, None);
        p.vim_input(PaletteVimKey::Char('v'), false, None);
        p.vim_input(PaletteVimKey::Esc, false, None);
        assert_eq!(p.vim_mode, PaletteVimMode::Normal);
        assert!(p.selection_range.is_none());
    }
}
