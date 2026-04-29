use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use ropey::Rope;

use crate::app::{
    command_palette::{
        CommandPalette, CommandPaletteAction, CommandPaletteItem, CommandPaletteMode,
        CommandPaletteRenderModel,
    },
    file_picker::FilePickerEntry,
    match_ranges::score_label_match,
    resolved_keymap::parse_key_sequence,
};
use crate::async_runtime::message::{
    FilePreviewLine, FileSystemChangeKind, FileSystemEvent, LspCompletionItem, LspDiagnostic,
    PersistedHistoryEnvelope,
};
use crate::config::keymap_loader::KeymapLoader;
use crate::config::ui_config::IndentConfig;
use crate::core::commands::{
    FindMotionKind, Motion, OperationTarget, Operator, TextObjectKind, TextObjectModifier,
};
use crate::core::mode::{
    EditorMode, ModeEvent, ModeState, ModeTransitionError, ModeTransitionResult,
};
use crate::core::text_object::find_text_object_range;
use crate::core::transaction::{CursorState, EditAction, EditHistory, Transaction};
use crate::editor_core::filetype_label_for_path;
use crate::syntax::highlight::HighlightEdit;
use crate::text::text_system::StyledTextSpan;
use crate::workspace::model::{WorkspaceModel, WorkspaceNodeType};
use overlays::build_completion_display_items;

mod buffers;
mod editor;
mod overlays;
mod palette;
mod settings;
mod state;
mod workspace;

#[cfg(test)]
mod tests;

pub use settings::*;

#[derive(Debug, Clone, Default)]
pub struct ExternalChangeReport {
    pub workspace_reloaded: bool,
    pub active_file_reloaded: bool,
    pub conflict_detected: bool,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualSelectionRange {
    pub start_char: usize,
    pub end_char: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub start_byte_in_line: usize,
    pub end_byte_in_line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardRecordKind {
    Charwise,
    Linewise,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorBuffer {
    pub path: PathBuf,
    pub language_id: Option<String>,
    pub git_baseline: Option<String>,
    pub git_line_statuses: HashMap<usize, GitLineStatus>,
}

impl EditorBuffer {
    pub fn new(path: PathBuf, language_id: Option<String>) -> Self {
        Self {
            path,
            language_id,
            git_baseline: None,
            git_line_statuses: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitLineStatus {
    Added,
    Modified,
    DeletedAbove,
    DeletedBelow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageBuffer {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub rgba: Option<Vec<u8>>,
    pub error: Option<String>,
}

pub fn is_supported_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "bmp" | "ico" | "webp"
            )
        })
        .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtyState {
    pub session_id: Option<u64>,
    pub title: String,
    pub working_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencesBufferItem {
    pub path: PathBuf,
    pub relative_path: String,
    pub line: usize,
    pub column: usize,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencesBufferState {
    pub title: String,
    pub origin_path: Option<PathBuf>,
    pub origin_line: usize,
    pub items: Vec<ReferencesBufferItem>,
    pub selected_index: usize,
    pub preview_lines: Vec<FilePreviewLine>,
    pub preview_text: String,
    pub preview_spans: Vec<StyledTextSpan>,
    pub loading: bool,
    pub status_message: Option<String>,
    pub pending_request_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticItem {
    pub file_path: PathBuf,
    pub line: usize,
    pub col: usize,
    pub message: String,
    pub severity: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticsState {
    pub results: Vec<DiagnosticItem>,
    pub selected_index: usize,
    pub preview_lines: Vec<FilePreviewLine>,
    pub preview_text: String,
    pub preview_spans: Vec<StyledTextSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpSection {
    pub title: String,
    pub mode_hint: String,
    pub entries: Vec<HelpEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpEntry {
    pub keys: Vec<String>,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpState {
    pub title: String,
    pub subtitle: String,
    pub profile_name: String,
    pub source_label: String,
    pub sections: Vec<HelpSection>,
    pub lines: Vec<String>,
}

impl HelpState {
    pub fn new() -> Self {
        Self::from_bindings(
            "nvim-ultimate",
            "default.toml",
            &KeymapLoader::load_active(None),
        )
    }

    pub fn from_bindings(
        profile_name: &str,
        source_label: &str,
        bindings: &[crate::config::keymap_config::KeyBinding],
    ) -> Self {
        Self {
            title: "[Cheat Sheet]".to_string(),
            subtitle: "KEYMAP CHEAT SHEET".to_string(),
            profile_name: profile_name.to_string(),
            source_label: source_label.to_string(),
            sections: build_help_sections(bindings),
            lines: build_help_lines(profile_name, source_label, bindings),
        }
    }
}

fn build_help_sections(bindings: &[crate::config::keymap_config::KeyBinding]) -> Vec<HelpSection> {
    let specs = [
        (None, "GLOBAL", "any mode"),
        (Some("insert"), "INSERT", "mode = insert"),
        (Some("palette"), "PALETTE", "mode = palette"),
        (Some("normal"), "NORMAL", "mode = normal"),
        (Some("visual"), "VISUAL", "mode = visual"),
    ];

    specs
        .into_iter()
        .filter_map(|(mode, title, mode_hint)| {
            let entries = bindings
                .iter()
                .filter(|binding| binding.mode.as_deref() == mode)
                .filter_map(|binding| {
                    Some(HelpEntry {
                        keys: format_key_binding_for_help(&binding.key)?
                            .split(' ')
                            .map(str::to_string)
                            .collect(),
                        label: command_label_for_help(&binding.command),
                    })
                })
                .collect::<Vec<_>>();
            (!entries.is_empty()).then_some(HelpSection {
                title: title.to_string(),
                mode_hint: mode_hint.to_string(),
                entries,
            })
        })
        .collect()
}

fn build_help_lines(
    profile_name: &str,
    source_label: &str,
    bindings: &[crate::config::keymap_config::KeyBinding],
) -> Vec<String> {
    let mut lines = vec![
        "Netherize Cheat Sheet".to_string(),
        format!("profile: {profile_name} · source: {source_label}"),
        "".to_string(),
        "Command palette".to_string(),
    ];

    append_help_binding(
        &mut lines,
        bindings,
        "app.open_file_picker",
        "Open file picker",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "app.open_command_palette",
        "Open command palette",
    );
    lines.push("  :help / :h            Open this cheat sheet buffer".to_string());
    lines.push("".to_string());

    lines.push("Buffers".to_string());
    lines.push("  :bn / :bp             Next / previous buffer".to_string());
    lines.push("  :bd                   Close current buffer".to_string());
    lines.push("  :enew                 New scratch buffer".to_string());
    lines.push("".to_string());

    lines.push("Navigation".to_string());
    append_help_binding(&mut lines, bindings, "editor.move_left", "Move left");
    append_help_binding(&mut lines, bindings, "editor.move_down", "Move down");
    append_help_binding(&mut lines, bindings, "editor.move_up", "Move up");
    append_help_binding(&mut lines, bindings, "editor.move_right", "Move right");
    append_help_binding(
        &mut lines,
        bindings,
        "editor.open_in_file_search",
        "Search in current file",
    );
    lines.push("".to_string());

    lines.push("Editing".to_string());
    append_help_binding(
        &mut lines,
        bindings,
        "mode.enter_insert",
        "Enter insert mode",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "mode.enter_normal",
        "Return to normal mode",
    );
    lines.push("  :w                    Save current file".to_string());
    lines.push("".to_string());

    lines.push("Tools".to_string());
    append_help_binding(&mut lines, bindings, "app.open_settings", "Open settings");
    append_help_binding(
        &mut lines,
        bindings,
        "app.search_in_files",
        "Search in files",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "diagnostics.open_picker",
        "Open diagnostics picker",
    );

    lines
}

fn command_label_for_help(command_id: &str) -> String {
    let label = match command_id {
        "editor.save_file" => "Save file",
        "editor.open_folder" => "Open folder / project",
        "app.open_command_palette" => "Command palette",
        "app.open_settings" => "Open settings",
        "app.focus_explorer" => "Focus explorer",
        "app.toggle_left_dock" => "Toggle left dock",
        "app.focus_back" => "Focus back to editor",
        "app.toggle_bottom_dock" => "Toggle bottom dock",
        "app.focus_terminal" => "Focus terminal",
        "mode.enter_normal" => "→ Normal mode",
        "mode.enter_insert" => "Insert mode",
        "mode.enter_visual" => "→ Visual mode",
        "mode.enter_visual_line" => "→ Visual line",
        "editor.move_left" => "Move left",
        "editor.move_down" => "Move down",
        "editor.move_up" => "Move up",
        "editor.move_right" => "Move right",
        "editor.move_word_forward" => "Word forward",
        "editor.move_word_backward" => "Word backward",
        "editor.move_word_end" => "Word end",
        "editor.move_to_line_start" => "Line start",
        "editor.move_to_line_end" => "Line end",
        "editor.move_to_first_non_whitespace" => "First non-whitespace",
        "editor.move_to_first_line" => "First line",
        "editor.move_to_last_line" => "Last line",
        "editor.scroll_half_page_up" => "½ page up",
        "editor.scroll_half_page_down" => "½ page down",
        "editor.center_cursor_line" => "Center cursor line",
        "editor.search_next" => "Next match",
        "editor.search_prev" => "Prev match",
        "editor.search_word_under_cursor" => "Search word under cursor",
        "editor.clear_search_highlights" => "Clear highlights",
        "editor.open_in_file_search" => "Search in file",
        "editor.delete_char" => "Delete char",
        "editor.delete_current_line" => "Delete line",
        "editor.delete_word_forward" => "Delete word →",
        "editor.delete_word_backward" => "Delete word ←",
        "editor.change_word_forward" => "Change word →",
        "editor.change_word_backward" => "Change word ←",
        "editor.toggle_line_comment" => "Toggle line comment",
        "editor.toggle_selection_comment" => "Toggle comment",
        "editor.paste" => "Paste",
        "editor.undo" => "Undo",
        "editor.redo" => "Redo",
        "editor.insert_tab" => "Insert tab",
        "editor.newline" => "New line",
        "editor.backspace" => "Backspace",
        "editor.append_after_cursor" => "Append after cursor",
        "editor.append_at_line_end" => "Append at line end",
        "editor.insert_at_line_start" => "Insert at line start",
        "editor.insert_line_below" => "New line below",
        "editor.insert_line_above" => "New line above",
        "editor.delete_selection" => "Delete selection",
        "editor.yank_selection" => "Yank selection",
        "editor.change_selection" => "Change selection",
        "completion.next" => "Select next",
        "completion.prev" => "Select previous",
        "completion.accept" => "Accept completion",
        "completion.close" => "Close completion",
        "app.open_file_picker" => "Open file picker",
        "app.search_in_files" => "Search in files",
        "app.open_workspace_symbols" => "Workspace symbols",
        "buffer.next" => "Next buffer",
        "buffer.prev" => "Prev buffer",
        "buffer.close_current" => "Close current buffer",
        "app.next_panel_tab" => "Next panel tab",
        "app.prev_panel_tab" => "Prev panel tab",
        "lsp.hover" => "Hover docs",
        "lsp.go_to_definition" => "Go to definition",
        "lsp.references" => "References",
        "lsp.format_document" => "Format document",
        _ => command_id,
    };
    label.to_string()
}

fn append_help_binding(
    lines: &mut Vec<String>,
    bindings: &[crate::config::keymap_config::KeyBinding],
    command_id: &str,
    label: &str,
) {
    let keys = bindings
        .iter()
        .filter(|binding| binding.command == command_id)
        .filter_map(|binding| format_key_binding_for_help(&binding.key))
        .collect::<Vec<_>>();

    if keys.is_empty() {
        return;
    }

    lines.push(format!("  {:<22} {}", keys.join(" / "), label));
}

fn format_key_binding_for_help(key: &str) -> Option<String> {
    let parts = parse_key_sequence(key)?
        .into_iter()
        .map(|spec| spec.display_token())
        .collect::<Vec<_>>();
    Some(parts.join(" "))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionTriggerPosition {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionPrefixInfo {
    pub start_col: usize,
    pub prefix: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionDisplayItem {
    pub item: LspCompletionItem,
    pub match_ranges: Vec<(usize, usize)>,
    pub score: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionState {
    pub raw_items: Vec<LspCompletionItem>,
    pub filtered_items: Vec<CompletionDisplayItem>,
    pub selected_index: usize,
    pub typed_prefix: String,
    pub trigger_pos: CompletionTriggerPosition,
    pub anchor_line: usize,
    pub anchor_col: usize,
}

impl CompletionState {
    pub fn from_lsp_items(
        items: Vec<LspCompletionItem>,
        anchor_line: usize,
        anchor_col: usize,
        prefix_start_col: usize,
        prefix: String,
    ) -> Self {
        let filtered_items = build_completion_display_items(&items, &prefix);

        Self {
            raw_items: items,
            filtered_items,
            selected_index: 0,
            typed_prefix: prefix,
            trigger_pos: CompletionTriggerPosition {
                line: anchor_line,
                col: prefix_start_col,
            },
            anchor_line,
            anchor_col,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzyState {
    pub mode: CommandPaletteMode,
    pub query: String,
    pub selected_index: usize,
    pub source_file_path: Option<PathBuf>,
    pub preview_lines: Vec<FilePreviewLine>,
    pub preview_text: String,
    pub preview_spans: Vec<StyledTextSpan>,
    pub results: Vec<CommandPaletteItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHistoryEntrySummary {
    pub index: usize,
    pub label: String,
    pub secondary_label: Option<String>,
}

#[derive(Debug, Clone)]
struct StoredFileHistory {
    history: EditHistory,
}

#[derive(Debug, Clone)]
struct EditorViewSnapshot {
    text: Rope,
    cursor: CursorState,
    selection_anchor_char_idx: Option<usize>,
    visual_line_mode: bool,
    target_scroll_y: f32,
    current_scroll_y: f32,
    scroll_column: usize,
    dirty: bool,
}

#[derive(Debug, Clone)]
struct FileHistoryPreviewSession {
    baseline_view: EditorViewSnapshot,
    baseline_history: EditHistory,
    preview_index: Option<usize>,
}

impl FuzzyState {
    pub fn new(mode: CommandPaletteMode) -> Self {
        Self {
            mode,
            query: String::new(),
            selected_index: 0,
            source_file_path: None,
            preview_lines: Vec::new(),
            preview_text: String::new(),
            preview_spans: Vec::new(),
            results: Vec::new(),
        }
    }

    pub fn append_query(&mut self, text: &str) -> bool {
        self.query.push_str(text);
        self.selected_index = 0;
        self.preview_lines.clear();
        self.preview_text.clear();
        self.preview_spans.clear();
        true
    }

    pub fn backspace_query(&mut self) -> bool {
        if self.query.is_empty() {
            return false;
        }
        self.query.pop();
        self.selected_index = 0;
        self.preview_lines.clear();
        self.preview_text.clear();
        self.preview_spans.clear();
        true
    }

    pub fn select_next(&mut self) -> bool {
        if self.results.is_empty() {
            return false;
        }
        if self.selected_index + 1 < self.results.len() {
            self.selected_index += 1;
            self.preview_lines.clear();
            self.preview_text.clear();
            self.preview_spans.clear();
            true
        } else {
            false
        }
    }

    pub fn select_prev(&mut self) -> bool {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.preview_lines.clear();
            self.preview_text.clear();
            self.preview_spans.clear();
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BufferContent {
    Text(EditorBuffer),
    Image(ImageBuffer),
    Terminal(PtyState),
    References(ReferencesBufferState),
    Diagnostics(DiagnosticsState),
    FuzzyPicker(FuzzyState),
    SettingsTab(SettingsState),
    Help(HelpState),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BufferEntry {
    pub content: BufferContent,
}

impl BufferEntry {
    pub fn label(&self) -> String {
        match &self.content {
            BufferContent::Text(buffer) => buffer
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
                .unwrap_or_else(|| buffer.path.display().to_string()),
            BufferContent::Image(buffer) => buffer
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
                .unwrap_or_else(|| buffer.path.display().to_string()),
            BufferContent::Terminal(state) => state.title.clone(),
            BufferContent::References(state) => state.title.clone(),
            BufferContent::Diagnostics(_) => "[Diagnostics]".to_string(),
            BufferContent::FuzzyPicker(_) => "[Fuzzy Finder]".to_string(),
            BufferContent::SettingsTab(_) => "[Settings]".to_string(),
            BufferContent::Help(state) => state.title.clone(),
        }
    }

    pub fn is_dirty(&self, is_active: bool, active_editor_dirty: bool) -> bool {
        match &self.content {
            BufferContent::Text(_) => is_active && active_editor_dirty,
            BufferContent::Image(_)
            | BufferContent::Terminal(_)
            | BufferContent::References(_)
            | BufferContent::Diagnostics(_)
            | BufferContent::FuzzyPicker(_)
            | BufferContent::SettingsTab(_)
            | BufferContent::Help(_) => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayColorToken {
    UiFgGhost,
}

/// Style cho FloatingBox overlay — xác định màu border và tiêu đề.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FloatingBoxStyle {
    /// Hover documentation (K) — border màu accent.
    DocHover,
    /// Code peek preview (gD) — border màu warning, header rộng hơn.
    PeekWindow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FloatingBoxBlock {
    Prose(String),
    Code {
        text: String,
        spans: Vec<StyledTextSpan>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorOverlay {
    VirtualText {
        line: usize,
        column: usize,
        text: String,
        color_token: OverlayColorToken,
    },
    /// Floating popup hiển thị multi-line text (doc hover, code peek).
    FloatingBox {
        /// Dòng cursor lúc trigger (0-indexed) — dùng để định vị popup bên dưới.
        anchor_line: usize,
        anchor_col: usize,
        /// Nội dung đã được parse thành prose/code blocks.
        blocks: Vec<FloatingBoxBlock>,
        style: FloatingBoxStyle,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClipboardRecord {
    text: String,
    kind: ClipboardRecordKind,
}

/// AppState giữ editor state tối thiểu cho phase 4.
///
/// Đây là nơi duy nhất được phép mutate text/cursor khi command dispatch chạy.
#[derive(Debug, Clone)]
pub struct AppState {
    text: Rope,
    cursor_char_idx: usize,
    target_col: usize,
    revision: u64,
    mode_state: ModeState,
    active_file: Option<PathBuf>,
    selection_anchor_char_idx: Option<usize>,
    visual_line_mode: bool,
    buffers: Vec<BufferEntry>,
    active_buffer_index: Option<usize>,
    default_save_path: PathBuf,
    dirty: bool,
    pub target_scroll_y: f32,
    pub current_scroll_y: f32,
    pub scroll_column: usize,
    workspace_model: Option<WorkspaceModel>,
    command_palette: CommandPalette,
    file_picker_results_cache: Vec<FilePickerEntry>,
    last_search_query: String,
    search_highlights: Vec<(usize, usize)>,
    search_whole_word: bool,
    terminal_panel_open: bool,
    external_conflict: Option<String>,
    external_notice: Option<String>,
    last_saved_at: Option<Instant>,
    clipboard_record: Option<ClipboardRecord>,
    history: EditHistory,
    stored_file_histories: HashMap<PathBuf, StoredFileHistory>,
    current_transaction: Option<Transaction>,
    file_history_preview: Option<FileHistoryPreviewSession>,
    pending_highlight_edits: Vec<HighlightEdit>,
    current_overlays: Vec<EditorOverlay>,
    completion: Option<CompletionState>,
    inline_suggestion: Option<String>,
    jump_back_stack: Vec<(PathBuf, usize)>,
    jump_forward_stack: Vec<(PathBuf, usize)>,
    diagnostics: HashMap<PathBuf, Vec<LspDiagnostic>>,
    pending_explorer_rename_path: Option<PathBuf>,
    indent_config: IndentConfig,
    is_initial_launch_welcome: bool,
}

impl AppState {
    const SELF_SAVE_IGNORE_WINDOW: Duration = Duration::from_secs(2);

    pub fn new(default_save_path: PathBuf) -> Self {
        Self {
            text: Rope::new(),
            cursor_char_idx: 0,
            target_col: 0,
            revision: 0,
            mode_state: ModeState::default(),
            active_file: None,
            selection_anchor_char_idx: None,
            visual_line_mode: false,
            buffers: Vec::new(),
            active_buffer_index: None,
            default_save_path,
            dirty: false,
            target_scroll_y: 0.0,
            current_scroll_y: 0.0,
            scroll_column: 0,
            workspace_model: None,
            command_palette: CommandPalette::default(),
            file_picker_results_cache: Vec::new(),
            last_search_query: String::new(),
            search_highlights: Vec::new(),
            search_whole_word: false,
            terminal_panel_open: false,
            external_conflict: None,
            external_notice: None,
            last_saved_at: None,
            clipboard_record: None,
            history: EditHistory::new(),
            stored_file_histories: HashMap::new(),
            current_transaction: None,
            file_history_preview: None,
            pending_highlight_edits: Vec::new(),
            current_overlays: Vec::new(),
            completion: None,
            inline_suggestion: None,
            jump_back_stack: Vec::new(),
            jump_forward_stack: Vec::new(),
            diagnostics: HashMap::new(),
            pending_explorer_rename_path: None,
            indent_config: IndentConfig::default(),
            is_initial_launch_welcome: true,
        }
    }

    pub fn from_text(default_save_path: PathBuf, text: &str) -> Self {
        Self {
            text: Rope::from(text),
            cursor_char_idx: 0,
            target_col: 0,
            revision: 0,
            mode_state: ModeState::default(),
            active_file: None,
            selection_anchor_char_idx: None,
            visual_line_mode: false,
            buffers: Vec::new(),
            active_buffer_index: None,
            default_save_path,
            dirty: false,
            target_scroll_y: 0.0,
            current_scroll_y: 0.0,
            scroll_column: 0,
            workspace_model: None,
            command_palette: CommandPalette::default(),
            file_picker_results_cache: Vec::new(),
            last_search_query: String::new(),
            search_highlights: Vec::new(),
            search_whole_word: false,
            terminal_panel_open: false,
            external_conflict: None,
            external_notice: None,
            last_saved_at: None,
            clipboard_record: None,
            history: EditHistory::new(),
            stored_file_histories: HashMap::new(),
            current_transaction: None,
            file_history_preview: None,
            pending_highlight_edits: Vec::new(),
            current_overlays: Vec::new(),
            completion: None,
            inline_suggestion: None,
            jump_back_stack: Vec::new(),
            jump_forward_stack: Vec::new(),
            diagnostics: HashMap::new(),
            pending_explorer_rename_path: None,
            indent_config: IndentConfig::default(),
            is_initial_launch_welcome: false,
        }
    }

    pub fn is_initial_launch_welcome(&self) -> bool {
        self.is_initial_launch_welcome
    }

    pub fn set_initial_launch_welcome(&mut self, enabled: bool) -> bool {
        if self.is_initial_launch_welcome == enabled {
            return false;
        }
        self.is_initial_launch_welcome = enabled;
        self.bump_revision();
        true
    }

    pub fn dismiss_initial_launch_welcome(&mut self) -> bool {
        self.set_initial_launch_welcome(false)
    }

    pub fn set_indent_config(&mut self, config: IndentConfig) {
        self.indent_config = config;
    }

    pub fn ensure_probe_file(path: &Path, content: &str) -> Result<(), String> {
        if path.exists() {
            return Ok(());
        }

        fs::write(path, content)
            .map_err(|err| format!("create probe file {:?} failed: {err}", path))
    }
}
