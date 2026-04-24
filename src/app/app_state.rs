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
};
use crate::async_runtime::message::{
    FilePreviewLine, FileSystemChangeKind, FileSystemEvent, LspCompletionItem, LspDiagnostic,
};
use crate::core::commands::{TextObjectKind, TextObjectModifier};
use crate::core::mode::{
    EditorMode, ModeEvent, ModeState, ModeTransitionError, ModeTransitionResult,
};
use crate::core::text_object::find_text_object_range;
use crate::core::transaction::{CursorState, EditAction, EditHistory, Transaction};
use crate::editor_core::filetype_label_for_path;
use crate::syntax::highlight::HighlightEdit;
use crate::text::text_system::StyledTextSpan;
use crate::workspace::model::{WorkspaceModel, WorkspaceNodeType};

#[derive(Debug, Clone, PartialEq)]
pub enum SettingItem {
    ThemeSelector { current: String },
    FontFamily { current: String },
    FontSize { current: f32 },
    LineHeight { current: f32 },
    SidebarWidth { current: i32 },
    RightSidebarWidth { current: i32 },
    BottomPanelHeight { current: i32 },
    UiRounding { enabled: bool, radius_px: f32 },
}

impl SettingItem {
    pub fn title(&self) -> &'static str {
        match self {
            Self::ThemeSelector { .. } => "Theme",
            Self::FontFamily { .. } => "Font Family",
            Self::FontSize { .. } => "Font Size",
            Self::LineHeight { .. } => "Line Height",
            Self::SidebarWidth { .. } => "Left Dock Width",
            Self::RightSidebarWidth { .. } => "Right Dock Width",
            Self::BottomPanelHeight { .. } => "Bottom Dock Height",
            Self::UiRounding { .. } => "UI Rounding",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SettingsEditingKind {
    FontFamily,
    FontSize,
    LineHeight,
    SidebarWidth,
    RightSidebarWidth,
    BottomPanelHeight,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsEditingState {
    pub kind: SettingsEditingKind,
    pub draft: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsState {
    pub selected_index: usize,
    pub items: Vec<SettingItem>,
    pub editing: Option<SettingsEditingState>,
}

impl SettingsState {
    pub fn new(
        theme_profile: impl Into<String>,
        font_family: impl Into<String>,
        font_size: f32,
        line_height: f32,
        left_width: i32,
        right_width: i32,
        bottom_height: i32,
        ui_rounding_enabled: bool,
        border_radius_px: f32,
    ) -> Self {
        Self {
            selected_index: 0,
            items: vec![
                SettingItem::ThemeSelector {
                    current: theme_profile.into(),
                },
                SettingItem::FontFamily {
                    current: font_family.into(),
                },
                SettingItem::FontSize {
                    current: font_size.max(1.0),
                },
                SettingItem::LineHeight {
                    current: line_height.max(1.0),
                },
                SettingItem::SidebarWidth {
                    current: left_width.max(0),
                },
                SettingItem::RightSidebarWidth {
                    current: right_width.max(0),
                },
                SettingItem::BottomPanelHeight {
                    current: bottom_height.max(0),
                },
                SettingItem::UiRounding {
                    enabled: ui_rounding_enabled,
                    radius_px: border_radius_px.max(0.0),
                },
            ],
            editing: None,
        }
    }

    pub fn select_next(&mut self) -> bool {
        if self.selected_index + 1 < self.items.len() {
            self.selected_index += 1;
            true
        } else {
            false
        }
    }

    pub fn select_prev(&mut self) -> bool {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            true
        } else {
            false
        }
    }

    pub fn selected_item(&self) -> Option<&SettingItem> {
        self.items.get(self.selected_index)
    }

    pub fn selected_item_mut(&mut self) -> Option<&mut SettingItem> {
        self.items.get_mut(self.selected_index)
    }

    pub fn begin_editing(&mut self) -> bool {
        let Some(item) = self.selected_item() else {
            return false;
        };
        let (kind, draft) = match item {
            SettingItem::FontFamily { current } => {
                (SettingsEditingKind::FontFamily, current.clone())
            }
            SettingItem::FontSize { current } => {
                (SettingsEditingKind::FontSize, format!("{current:.1}"))
            }
            SettingItem::LineHeight { current } => {
                (SettingsEditingKind::LineHeight, format!("{current:.1}"))
            }
            SettingItem::SidebarWidth { current } => {
                (SettingsEditingKind::SidebarWidth, current.to_string())
            }
            SettingItem::RightSidebarWidth { current } => {
                (SettingsEditingKind::RightSidebarWidth, current.to_string())
            }
            SettingItem::BottomPanelHeight { current } => {
                (SettingsEditingKind::BottomPanelHeight, current.to_string())
            }
            SettingItem::ThemeSelector { .. } | SettingItem::UiRounding { .. } => return false,
        };
        self.editing = Some(SettingsEditingState { kind, draft });
        true
    }

    pub fn cancel_editing(&mut self) -> bool {
        self.editing.take().is_some()
    }

    pub fn append_editing_text(&mut self, text: &str) -> bool {
        let Some(editing) = &mut self.editing else {
            return false;
        };
        editing.draft.push_str(text);
        true
    }

    pub fn backspace_editing(&mut self) -> bool {
        let Some(editing) = &mut self.editing else {
            return false;
        };
        editing.draft.pop().is_some()
    }
}

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
    pub preview_lines: Vec<FilePreviewLine>,
    pub preview_text: String,
    pub preview_spans: Vec<StyledTextSpan>,
    pub results: Vec<CommandPaletteItem>,
}

impl FuzzyState {
    pub fn new(mode: CommandPaletteMode) -> Self {
        Self {
            mode,
            query: String::new(),
            selected_index: 0,
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
    Terminal(PtyState),
    References(ReferencesBufferState),
    Diagnostics(DiagnosticsState),
    FuzzyPicker(FuzzyState),
    SettingsTab(SettingsState),
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
            BufferContent::Terminal(state) => state.title.clone(),
            BufferContent::References(state) => state.title.clone(),
            BufferContent::Diagnostics(_) => "[Diagnostics]".to_string(),
            BufferContent::FuzzyPicker(_) => "[Fuzzy Finder]".to_string(),
            BufferContent::SettingsTab(_) => "[Settings]".to_string(),
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
    pub scroll_line: usize,
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
    current_transaction: Option<Transaction>,
    pending_highlight_edits: Vec<HighlightEdit>,
    current_overlays: Vec<EditorOverlay>,
    completion: Option<CompletionState>,
    jump_back_stack: Vec<(PathBuf, usize)>,
    jump_forward_stack: Vec<(PathBuf, usize)>,
    diagnostics: HashMap<PathBuf, Vec<LspDiagnostic>>,
    pending_explorer_rename_path: Option<PathBuf>,
}

impl AppState {
    const SELF_SAVE_IGNORE_WINDOW: Duration = Duration::from_millis(500);

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
            scroll_line: 0,
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
            current_transaction: None,
            pending_highlight_edits: Vec::new(),
            current_overlays: Vec::new(),
            completion: None,
            jump_back_stack: Vec::new(),
            jump_forward_stack: Vec::new(),
            diagnostics: HashMap::new(),
            pending_explorer_rename_path: None,
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
            scroll_line: 0,
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
            current_transaction: None,
            pending_highlight_edits: Vec::new(),
            current_overlays: Vec::new(),
            completion: None,
            jump_back_stack: Vec::new(),
            jump_forward_stack: Vec::new(),
            diagnostics: HashMap::new(),
            pending_explorer_rename_path: None,
        }
    }

    pub fn ensure_probe_file(path: &Path, content: &str) -> Result<(), String> {
        if path.exists() {
            return Ok(());
        }

        fs::write(path, content)
            .map_err(|err| format!("create probe file {:?} failed: {err}", path))
    }

    pub fn attach_workspace(&mut self, root_path: PathBuf) -> Result<(), String> {
        let workspace = WorkspaceModel::load(root_path)?;
        self.workspace_model = Some(workspace);
        Ok(())
    }

    pub fn clear_workspace_session_state(&mut self) -> bool {
        let mut changed = false;

        changed |= !self.diagnostics.is_empty();
        self.diagnostics.clear();

        changed |= !self.buffers.is_empty();
        self.buffers.clear();

        changed |= self.active_buffer_index.is_some();
        self.active_buffer_index = None;

        changed |= self.command_palette.is_visible;
        changed |= self.close_command_palette();

        changed |= self.pending_explorer_rename_path.is_some();
        self.pending_explorer_rename_path = None;

        changed |= !self.jump_back_stack.is_empty() || !self.jump_forward_stack.is_empty();
        self.jump_back_stack.clear();
        self.jump_forward_stack.clear();

        let had_active_file = self.active_file.is_some();
        let had_text = self.text.len_chars() > 0;
        let had_dirty = self.dirty;
        self.reset_text_editor_state();
        changed |= had_active_file || had_text || had_dirty;

        changed |= self.clear_current_overlays();

        if changed {
            self.bump_revision();
        }

        changed
    }

    pub fn workspace_root_path(&self) -> Option<&Path> {
        self.workspace_model
            .as_ref()
            .map(|model| model.root_path.as_path())
    }

    pub fn workspace_nodes(&self) -> Option<&[crate::workspace::model::WorkspaceNode]> {
        self.workspace_model.as_ref().map(|m| m.nodes.as_slice())
    }

    pub fn workspace_selected_path(&self) -> Option<&Path> {
        self.workspace_model
            .as_ref()
            .and_then(WorkspaceModel::selected_path)
    }

    pub fn workspace_filter_query(&self) -> Option<&str> {
        self.workspace_model
            .as_ref()
            .map(WorkspaceModel::filter_query)
    }

    pub fn workspace_show_hidden(&self) -> bool {
        self.workspace_model
            .as_ref()
            .is_some_and(WorkspaceModel::show_hidden)
    }

    pub fn workspace_show_ignored(&self) -> bool {
        self.workspace_model
            .as_ref()
            .is_some_and(WorkspaceModel::show_ignored)
    }

    pub fn workspace_visible_node_paths(&self) -> Vec<PathBuf> {
        self.workspace_model
            .as_ref()
            .map(WorkspaceModel::visible_node_paths)
            .unwrap_or_default()
    }

    pub fn workspace_has_active_filter(&self) -> bool {
        self.workspace_model
            .as_ref()
            .is_some_and(WorkspaceModel::has_active_filter)
    }

    pub fn workspace_is_inputting_filter(&self) -> bool {
        self.workspace_model
            .as_ref()
            .is_some_and(WorkspaceModel::is_inputting_filter)
    }

    pub fn workspace_start_filter_input(&mut self) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(WorkspaceModel::start_filter_input)
    }

    pub fn workspace_stop_filter_input(&mut self) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(WorkspaceModel::stop_filter_input)
    }

    pub fn workspace_clear_filter(&mut self) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(WorkspaceModel::clear_filter)
    }

    pub fn workspace_append_filter_text(&mut self, text: &str) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(|workspace| workspace.append_filter_text(text))
    }

    pub fn workspace_backspace_filter(&mut self) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(WorkspaceModel::backspace_filter)
    }

    pub fn workspace_is_expanded(&self, path: &Path) -> bool {
        self.workspace_model
            .as_ref()
            .is_some_and(|workspace| workspace.is_expanded(path))
    }

    pub fn workspace_select_path(&mut self, path: &Path) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(|workspace| workspace.select_path(path))
    }

    pub fn workspace_expand_path(&mut self, path: &Path) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(|workspace| workspace.expand_path(path))
    }

    pub fn workspace_collapse_path(&mut self, path: &Path) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(|workspace| workspace.collapse_path(path))
    }

    pub fn workspace_collapse_path_and_descendants(&mut self, path: &Path) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(|workspace| workspace.collapse_path_and_descendants(path))
    }

    pub fn workspace_expand_path_and_descendants(&mut self, path: &Path) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(|workspace| workspace.expand_path_and_descendants(path))
    }

    pub fn workspace_expand_to_path(&mut self, path: &Path) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(|workspace| workspace.expand_to_path(path))
    }

    pub fn workspace_reveal_path(&mut self, path: &Path) -> bool {
        self.workspace_expand_to_path(path)
    }

    pub fn workspace_scroll_to_selected_node(
        &mut self,
        viewport_height: f32,
        line_height: f32,
    ) -> bool {
        self.workspace_model.as_mut().is_some_and(|workspace| {
            workspace.scroll_to_selected_node(viewport_height, line_height)
        })
    }

    pub fn workspace_scroll_offset_rows(&self, line_height: f32) -> usize {
        self.workspace_model
            .as_ref()
            .map_or(0, |workspace| workspace.scroll_offset_rows(line_height))
    }

    pub fn rescan_workspace(&mut self) -> Result<bool, String> {
        let Some(workspace) = self.workspace_model.as_mut() else {
            return Ok(false);
        };
        workspace.rescan()?;
        if self.is_file_picker_open() {
            let _ = self.refresh_file_picker_results_if_open()?;
        }
        Ok(true)
    }

    pub fn workspace_toggle_show_hidden(&mut self) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(WorkspaceModel::toggle_show_hidden)
    }

    pub fn workspace_toggle_show_ignored(&mut self) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(WorkspaceModel::toggle_show_ignored)
    }

    pub fn workspace_file_count(&self) -> usize {
        self.workspace_model
            .as_ref()
            .map(|model| {
                model
                    .nodes
                    .iter()
                    .filter(|node| node.file_type == WorkspaceNodeType::File)
                    .count()
            })
            .unwrap_or(0)
    }

    pub fn open_command_palette_mode(&mut self, mode: CommandPaletteMode) -> Result<usize, String> {
        let workspace = self.workspace_model.as_ref();
        if matches!(
            mode,
            CommandPaletteMode::FilePicker
                | CommandPaletteMode::LiveGrep
                | CommandPaletteMode::ExplorerCreateFile
                | CommandPaletteMode::ExplorerCreateFolder
                | CommandPaletteMode::ExplorerRenameFull
                | CommandPaletteMode::ExplorerRenameBase
        ) && workspace.is_none()
        {
            return Err("workspace is not attached".to_string());
        }

        let count = self.command_palette.open(mode, workspace);
        self.sync_file_picker_cache();
        Ok(count)
    }

    /// Push current file+line onto the jump back stack before a jump (e.g. gd).
    /// Clears the forward stack since jumping starts a new branch.
    pub fn push_jump(&mut self) {
        let Some(path) = self.active_file.clone() else {
            return;
        };
        let line = self.cursor_line_col().0;
        self.jump_back_stack.push((path, line));
        self.jump_forward_stack.clear();
    }

    /// Push an explicit file+line onto the jump back stack.
    /// Useful when the current active surface is a non-file buffer.
    pub fn push_jump_entry(&mut self, path: PathBuf, line: usize) {
        self.jump_back_stack.push((path, line));
        self.jump_forward_stack.clear();
    }

    /// Pop from the back stack and return (path, line). Pushes current pos onto forward stack.
    pub fn pop_jump_back(&mut self) -> Option<(PathBuf, usize)> {
        let entry = self.jump_back_stack.pop()?;
        let current_path = self.active_file.clone().unwrap_or_default();
        let current_line = self.cursor_line_col().0;
        self.jump_forward_stack.push((current_path, current_line));
        Some(entry)
    }

    /// Pop from the forward stack and return (path, line). Pushes current pos onto back stack.
    pub fn pop_jump_forward(&mut self) -> Option<(PathBuf, usize)> {
        let entry = self.jump_forward_stack.pop()?;
        let current_path = self.active_file.clone().unwrap_or_default();
        let current_line = self.cursor_line_col().0;
        self.jump_back_stack.push((current_path, current_line));
        Some(entry)
    }

    /// Mở Command Palette ở LspReferences mode với danh sách references tĩnh từ LSP.
    pub fn open_lsp_references_palette(
        &mut self,
        items: Vec<crate::app::command_palette::CommandPaletteItem>,
    ) -> Result<(), String> {
        self.command_palette
            .open_with_items(CommandPaletteMode::LspReferences, items);
        Ok(())
    }

    pub fn open_recent_projects_palette(
        &mut self,
        recent: &[std::path::PathBuf],
    ) -> Result<(), String> {
        use crate::app::command_palette::CommandPaletteItem;
        let items = recent
            .iter()
            .map(|path| CommandPaletteItem::recent_project(path))
            .collect();
        self.command_palette
            .open_with_items(CommandPaletteMode::RecentProjects, items);
        Ok(())
    }

    pub fn sync_welcome_recent_projects(&mut self, recent: &[std::path::PathBuf]) -> bool {
        use crate::app::command_palette::CommandPaletteItem;
        let items: Vec<_> = recent
            .iter()
            .map(|path| CommandPaletteItem::recent_project(path))
            .collect();
        self.command_palette
            .set_hidden_items(CommandPaletteMode::RecentProjects, items)
    }

    pub fn open_theme_selector_palette(
        &mut self,
        themes: &[crate::config::theme_config::ThemeProfileEntry],
    ) -> Result<usize, String> {
        use crate::app::command_palette::CommandPaletteItem;
        let items = themes
            .iter()
            .map(|theme| CommandPaletteItem::theme(&theme.profile, &theme.path))
            .collect();
        Ok(self
            .command_palette
            .open_with_items(CommandPaletteMode::ThemeSelector, items))
    }

    pub fn close_command_palette(&mut self) -> bool {
        let changed = self.command_palette.close();
        self.sync_file_picker_cache();
        changed
    }

    pub fn is_command_palette_visible(&self) -> bool {
        self.command_palette.is_visible
    }

    pub fn command_palette_mode(&self) -> Option<CommandPaletteMode> {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
            }) = self.buffers.get(index)
            {
                return Some(state.mode);
            }
        }
        if self.command_palette.is_visible {
            Some(self.command_palette.mode)
        } else {
            None
        }
    }

    pub fn command_palette_query_text(&self) -> &str {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
            }) = self.buffers.get(index)
            {
                return &state.query;
            }
        }
        &self.command_palette.query
    }

    pub fn command_palette_selected_index(&self) -> usize {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
            }) = self.buffers.get(index)
            {
                return state.selected_index;
            }
        }
        self.command_palette.selected_index
    }

    pub fn command_palette_result_labels(&self) -> Vec<String> {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
            }) = self.buffers.get(index)
            {
                return state
                    .results
                    .iter()
                    .map(|entry| entry.label.clone())
                    .collect();
            }
        }
        self.command_palette
            .results
            .iter()
            .map(|entry| entry.label.clone())
            .collect()
    }

    pub fn command_palette_append_query(&mut self, text: &str) -> Result<bool, String> {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
            }) = self.buffers.get_mut(index)
            {
                let changed = state.append_query(text);
                self.bump_revision();
                return Ok(changed);
            }
        }

        let workspace = self.workspace_model.as_ref();
        if matches!(
            self.command_palette.mode,
            CommandPaletteMode::FilePicker
                | CommandPaletteMode::LiveGrep
                | CommandPaletteMode::ExplorerCreateFile
                | CommandPaletteMode::ExplorerCreateFolder
                | CommandPaletteMode::ExplorerRenameFull
                | CommandPaletteMode::ExplorerRenameBase
        ) && workspace.is_none()
        {
            return Err("workspace is not attached".to_string());
        }

        let changed = self.command_palette.append_query(text, workspace);
        if changed {
            self.sync_file_picker_cache();
        }
        Ok(changed)
    }

    pub fn set_command_palette_query(&mut self, text: &str) -> Result<bool, String> {
        let workspace = self.workspace_model.as_ref();
        if matches!(
            self.command_palette.mode,
            CommandPaletteMode::FilePicker
                | CommandPaletteMode::LiveGrep
                | CommandPaletteMode::ExplorerCreateFile
                | CommandPaletteMode::ExplorerCreateFolder
                | CommandPaletteMode::ExplorerRenameFull
                | CommandPaletteMode::ExplorerRenameBase
        ) && workspace.is_none()
        {
            return Err("workspace is not attached".to_string());
        }

        let changed = self.command_palette.set_query(text, workspace);
        if changed {
            self.sync_file_picker_cache();
        }
        Ok(changed)
    }

    pub fn command_palette_backspace_query(&mut self) -> Result<bool, String> {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
            }) = self.buffers.get_mut(index)
            {
                let changed = state.backspace_query();
                self.bump_revision();
                return Ok(changed);
            }
        }

        let workspace = self.workspace_model.as_ref();
        if matches!(
            self.command_palette.mode,
            CommandPaletteMode::FilePicker
                | CommandPaletteMode::LiveGrep
                | CommandPaletteMode::ExplorerCreateFile
                | CommandPaletteMode::ExplorerCreateFolder
                | CommandPaletteMode::ExplorerRenameFull
                | CommandPaletteMode::ExplorerRenameBase
        ) && workspace.is_none()
        {
            return Err("workspace is not attached".to_string());
        }

        let changed = self.command_palette.backspace_query(workspace);
        if changed {
            self.sync_file_picker_cache();
        }
        Ok(changed)
    }

    pub fn command_palette_select_next(&mut self) -> bool {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
            }) = self.buffers.get_mut(index)
            {
                let changed = state.select_next();
                if changed {
                    self.bump_revision();
                }
                return changed;
            }
        }
        self.command_palette.select_next()
    }

    pub fn command_palette_select_prev(&mut self) -> bool {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
            }) = self.buffers.get_mut(index)
            {
                let changed = state.select_prev();
                if changed {
                    self.bump_revision();
                }
                return changed;
            }
        }
        self.command_palette.select_prev()
    }

    pub fn command_palette_selected_action(&self) -> Option<CommandPaletteAction> {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
            }) = self.buffers.get(index)
            {
                return state
                    .results
                    .get(state.selected_index)
                    .map(|item| item.action.clone());
            }
        }
        self.command_palette.selected_action()
    }

    pub fn set_command_palette_results(
        &mut self,
        mode: CommandPaletteMode,
        query: &str,
        items: Vec<CommandPaletteItem>,
    ) -> bool {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
            }) = self.buffers.get_mut(index)
            {
                if state.mode == mode && state.query == query {
                    state.results = items;
                    state.selected_index = state
                        .selected_index
                        .min(state.results.len().saturating_sub(1));
                    state.preview_lines.clear();
                    state.preview_text.clear();
                    state.preview_spans.clear();
                    self.bump_revision();
                    return true;
                }
            }
        }

        if !self.command_palette.is_visible
            || self.command_palette.mode != mode
            || self.command_palette.query != query
        {
            return false;
        }

        let changed = self.command_palette.replace_results(items);
        if changed {
            self.sync_file_picker_cache();
        }
        changed
    }

    pub fn command_palette_render_model(
        &self,
        theme: &crate::config::theme_config::ThemeConfig,
        overlay_bounds: [f32; 4],
    ) -> Option<CommandPaletteRenderModel> {
        self.command_palette.render(theme, overlay_bounds)
    }

    pub fn set_command_palette_selection_range(&mut self, range: Option<(usize, usize)>) -> bool {
        self.command_palette.set_selection_range(range)
    }

    pub fn pending_explorer_rename_path(&self) -> Option<&Path> {
        self.pending_explorer_rename_path.as_deref()
    }

    pub fn set_pending_explorer_rename_path(&mut self, path: Option<PathBuf>) -> bool {
        if self.pending_explorer_rename_path == path {
            return false;
        }
        self.pending_explorer_rename_path = path;
        true
    }

    pub fn open_file_picker(&mut self) -> Result<usize, String> {
        self.open_command_palette_mode(CommandPaletteMode::FilePicker)
    }

    pub fn close_file_picker(&mut self) -> bool {
        if !self.is_file_picker_open() {
            return false;
        }
        self.close_command_palette()
    }

    pub fn set_fuzzy_picker_preview(
        &mut self,
        lines: Vec<FilePreviewLine>,
        preview_text: String,
        preview_spans: Vec<StyledTextSpan>,
    ) -> bool {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
            }) = self.buffers.get_mut(index)
            {
                state.preview_lines = lines;
                state.preview_text = preview_text;
                state.preview_spans = preview_spans;
                self.bump_revision();
                return true;
            }
        }
        false
    }

    pub fn set_active_references_preview(
        &mut self,
        lines: Vec<FilePreviewLine>,
        preview_text: String,
        preview_spans: Vec<StyledTextSpan>,
    ) -> bool {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::References(state),
            }) = self.buffers.get_mut(index)
            {
                state.preview_lines = lines;
                state.preview_text = preview_text;
                state.preview_spans = preview_spans;
                self.bump_revision();
                return true;
            }
        }
        false
    }

    pub fn set_active_diagnostics_preview(
        &mut self,
        lines: Vec<FilePreviewLine>,
        preview_text: String,
        preview_spans: Vec<StyledTextSpan>,
    ) -> bool {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::Diagnostics(state),
            }) = self.buffers.get_mut(index)
            {
                if state.preview_lines == lines
                    && state.preview_text == preview_text
                    && state.preview_spans == preview_spans
                {
                    return false;
                }
                state.preview_lines = lines;
                state.preview_text = preview_text;
                state.preview_spans = preview_spans;
                self.bump_revision();
                return true;
            }
        }
        false
    }

    pub fn active_fuzzy_picker_buffer(&self) -> Option<&FuzzyState> {
        if let Some(index) = self.active_buffer_index {
            if let Some(buffer) = self.buffers.get(index) {
                if let BufferContent::FuzzyPicker(state) = &buffer.content {
                    return Some(state);
                }
            }
        }
        None
    }

    pub fn diagnostics(&self) -> &HashMap<PathBuf, Vec<LspDiagnostic>> {
        &self.diagnostics
    }

    pub fn diagnostics_for_path(&self, path: &Path) -> Option<&[LspDiagnostic]> {
        let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.diagnostics.get(&normalized).map(Vec::as_slice)
    }

    pub fn set_file_diagnostics(&mut self, path: PathBuf, diagnostics: Vec<LspDiagnostic>) -> bool {
        let normalized = path.canonicalize().unwrap_or(path);
        if diagnostics.is_empty() {
            let removed = self.diagnostics.remove(&normalized).is_some();
            if removed {
                self.bump_revision();
            }
            return removed;
        }

        let changed = self.diagnostics.get(&normalized) != Some(&diagnostics);
        if changed {
            self.diagnostics.insert(normalized, diagnostics);
            self.bump_revision();
        }
        changed
    }

    pub fn open_diagnostics_buffer(&mut self, items: Vec<DiagnosticItem>) -> Result<usize, String> {
        if items.is_empty() {
            return Err("cannot open diagnostics buffer without items".to_string());
        }

        self.buffers.push(BufferEntry {
            content: BufferContent::Diagnostics(DiagnosticsState {
                results: items,
                selected_index: 0,
                preview_lines: Vec::new(),
                preview_text: String::new(),
                preview_spans: Vec::new(),
            }),
        });

        let index = self.buffers.len().saturating_sub(1);
        self.reset_text_editor_state();
        self.active_buffer_index = Some(index);
        let _ = self.clear_current_overlays();
        self.bump_revision();
        Ok(index)
    }

    pub fn active_buffer_is_fuzzy_picker(&self) -> bool {
        if let Some(index) = self.active_buffer_index {
            if let Some(buffer) = self.buffers.get(index) {
                return matches!(buffer.content, BufferContent::FuzzyPicker(_));
            }
        }
        false
    }

    pub fn active_buffer_is_settings(&self) -> bool {
        self.active_buffer()
            .is_some_and(|buffer| matches!(buffer.content, BufferContent::SettingsTab(_)))
    }

    pub fn active_settings_buffer(&self) -> Option<&SettingsState> {
        match self.active_buffer().map(|buffer| &buffer.content) {
            Some(BufferContent::SettingsTab(state)) => Some(state),
            _ => None,
        }
    }

    pub fn active_settings_buffer_mut(&mut self) -> Option<&mut SettingsState> {
        self.active_buffer_index
            .and_then(|idx| self.buffers.get_mut(idx))
            .and_then(|buffer| match &mut buffer.content {
                BufferContent::SettingsTab(state) => Some(state),
                _ => None,
            })
    }

    pub fn settings_is_editing(&self) -> bool {
        self.active_settings_buffer()
            .and_then(|state| state.editing.as_ref())
            .is_some()
    }

    pub fn settings_begin_editing(&mut self) -> bool {
        let Some(state) = self.active_settings_buffer_mut() else {
            return false;
        };
        let changed = state.begin_editing();
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn settings_cancel_editing(&mut self) -> bool {
        let Some(state) = self.active_settings_buffer_mut() else {
            return false;
        };
        let changed = state.cancel_editing();
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn settings_append_editing_text(&mut self, text: &str) -> bool {
        let Some(state) = self.active_settings_buffer_mut() else {
            return false;
        };
        let changed = state.append_editing_text(text);
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn settings_backspace_editing(&mut self) -> bool {
        let Some(state) = self.active_settings_buffer_mut() else {
            return false;
        };
        let changed = state.backspace_editing();
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn is_file_picker_open(&self) -> bool {
        self.command_palette.is_visible
            && self.command_palette.mode == CommandPaletteMode::FilePicker
    }

    pub fn file_picker_query_text(&self) -> &str {
        self.command_palette_query_text()
    }

    pub fn file_picker_selected_index(&self) -> usize {
        self.command_palette_selected_index()
    }

    pub fn file_picker_results(&self) -> &[FilePickerEntry] {
        &self.file_picker_results_cache
    }

    pub fn file_picker_append_query(&mut self, text: &str) -> Result<bool, String> {
        self.command_palette_append_query(text)
    }

    pub fn file_picker_backspace_query(&mut self) -> Result<bool, String> {
        self.command_palette_backspace_query()
    }

    pub fn file_picker_select_next(&mut self) -> bool {
        self.command_palette_select_next()
    }

    pub fn file_picker_select_prev(&mut self) -> bool {
        self.command_palette_select_prev()
    }

    pub fn file_picker_selected_path(&self) -> Option<PathBuf> {
        match self.command_palette_selected_action() {
            Some(CommandPaletteAction::OpenFile(path)) => Some(path),
            _ => None,
        }
    }

    pub fn is_terminal_panel_open(&self) -> bool {
        self.terminal_panel_open
    }

    pub fn set_terminal_panel_open(&mut self, open: bool) -> bool {
        if self.terminal_panel_open == open {
            return false;
        }
        self.terminal_panel_open = open;
        true
    }

    pub fn open_terminal_buffer(
        &mut self,
        title: impl Into<String>,
        working_dir: Option<PathBuf>,
    ) -> usize {
        self.buffers.push(BufferEntry {
            content: BufferContent::Terminal(PtyState {
                session_id: None,
                title: title.into(),
                working_dir,
            }),
        });
        let index = self.buffers.len().saturating_sub(1);
        self.active_buffer_index = Some(index);
        self.active_file = None;
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        self.external_conflict = None;
        self.bump_revision();
        index
    }

    pub fn open_references_buffer(
        &mut self,
        title: impl Into<String>,
        origin_path: Option<PathBuf>,
        origin_line: usize,
        items: Vec<ReferencesBufferItem>,
    ) -> Result<usize, String> {
        if items.is_empty() {
            return Err("cannot open references buffer without items".to_string());
        }

        self.buffers.push(BufferEntry {
            content: BufferContent::References(ReferencesBufferState {
                title: title.into(),
                origin_path,
                origin_line,
                items,
                selected_index: 0,
                preview_lines: Vec::new(),
                preview_text: String::new(),
                preview_spans: Vec::new(),
                loading: false,
                status_message: None,
                pending_request_id: None,
            }),
        });

        let index = self.buffers.len().saturating_sub(1);
        self.reset_text_editor_state();
        self.active_buffer_index = Some(index);
        let _ = self.clear_current_overlays();
        self.bump_revision();
        Ok(index)
    }

    pub fn open_pending_references_buffer(
        &mut self,
        title: impl Into<String>,
        origin_path: Option<PathBuf>,
        origin_line: usize,
        pending_request_id: u64,
    ) -> usize {
        self.buffers.push(BufferEntry {
            content: BufferContent::References(ReferencesBufferState {
                title: title.into(),
                origin_path,
                origin_line,
                items: Vec::new(),
                selected_index: 0,
                preview_lines: Vec::new(),
                preview_text: String::new(),
                preview_spans: Vec::new(),
                loading: true,
                status_message: Some("Loading references...".to_string()),
                pending_request_id: Some(pending_request_id),
            }),
        });

        let index = self.buffers.len().saturating_sub(1);
        self.reset_text_editor_state();
        self.active_buffer_index = Some(index);
        let _ = self.clear_current_overlays();
        self.bump_revision();
        index
    }

    pub fn finish_pending_references_buffer(
        &mut self,
        pending_request_id: u64,
        title: impl Into<String>,
        items: Vec<ReferencesBufferItem>,
    ) -> bool {
        let Some(buffer) = self.buffers.iter_mut().find(|buffer| {
            matches!(
                &buffer.content,
                BufferContent::References(state)
                    if state.pending_request_id == Some(pending_request_id)
            )
        }) else {
            return false;
        };

        let BufferContent::References(state) = &mut buffer.content else {
            return false;
        };

        state.title = title.into();
        state.items = items;
        state.selected_index = 0;
        state.preview_lines.clear();
        state.preview_text.clear();
        state.preview_spans.clear();
        state.loading = false;
        state.pending_request_id = None;
        state.status_message = if state.items.is_empty() {
            Some("No references found".to_string())
        } else {
            None
        };
        self.bump_revision();
        true
    }

    pub fn fail_pending_references_buffer(
        &mut self,
        pending_request_id: u64,
        message: impl Into<String>,
    ) -> bool {
        let Some(buffer) = self.buffers.iter_mut().find(|buffer| {
            matches!(
                &buffer.content,
                BufferContent::References(state)
                    if state.pending_request_id == Some(pending_request_id)
            )
        }) else {
            return false;
        };

        let BufferContent::References(state) = &mut buffer.content else {
            return false;
        };

        state.title = "References (0)".to_string();
        state.items.clear();
        state.selected_index = 0;
        state.preview_lines.clear();
        state.preview_text.clear();
        state.preview_spans.clear();
        state.loading = false;
        state.pending_request_id = None;
        state.status_message = Some(message.into());
        self.bump_revision();
        true
    }

    pub fn open_fuzzy_picker_buffer(&mut self, mode: CommandPaletteMode) -> usize {
        let state = FuzzyState::new(mode);
        self.buffers.push(BufferEntry {
            content: BufferContent::FuzzyPicker(state),
        });

        let index = self.buffers.len().saturating_sub(1);
        self.reset_text_editor_state();
        self.active_buffer_index = Some(index);
        let _ = self.clear_current_overlays();
        self.bump_revision();
        index
    }

    pub fn open_settings_buffer(
        &mut self,
        theme_profile: impl Into<String>,
        font_family: impl Into<String>,
        font_size: f32,
        line_height: f32,
        left_width: i32,
        right_width: i32,
        bottom_height: i32,
        ui_rounding_enabled: bool,
        border_radius_px: f32,
    ) -> usize {
        if let Some(existing_idx) = self
            .buffers
            .iter()
            .position(|buffer| matches!(buffer.content, BufferContent::SettingsTab(_)))
        {
            self.reset_text_editor_state();
            self.active_buffer_index = Some(existing_idx);
            let _ = self.clear_current_overlays();
            self.bump_revision();
            return existing_idx;
        }

        let state = SettingsState::new(
            theme_profile,
            font_family,
            font_size,
            line_height,
            left_width,
            right_width,
            bottom_height,
            ui_rounding_enabled,
            border_radius_px,
        );
        self.buffers.push(BufferEntry {
            content: BufferContent::SettingsTab(state),
        });

        let index = self.buffers.len().saturating_sub(1);
        self.reset_text_editor_state();
        self.active_buffer_index = Some(index);
        let _ = self.clear_current_overlays();
        self.bump_revision();
        index
    }

    pub fn settings_select_next(&mut self) -> bool {
        let Some(state) = self.active_settings_buffer_mut() else {
            return false;
        };
        let changed = state.select_next();
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn settings_select_prev(&mut self) -> bool {
        let Some(state) = self.active_settings_buffer_mut() else {
            return false;
        };
        let changed = state.select_prev();
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn bind_terminal_buffer_session(
        &mut self,
        buffer_index: usize,
        session_id: u64,
        working_dir: PathBuf,
    ) -> bool {
        let Some(buffer) = self.buffers.get_mut(buffer_index) else {
            return false;
        };
        let BufferContent::Terminal(state) = &mut buffer.content else {
            return false;
        };

        let changed = state.session_id != Some(session_id)
            || state.working_dir.as_deref() != Some(working_dir.as_path());
        state.session_id = Some(session_id);
        state.working_dir = Some(working_dir);
        changed
    }

    pub fn mark_terminal_buffer_closed(&mut self, session_id: u64) -> bool {
        let Some(index) = self.terminal_buffer_index_for_session(session_id) else {
            return false;
        };
        let Some(buffer) = self.buffers.get_mut(index) else {
            return false;
        };
        let BufferContent::Terminal(state) = &mut buffer.content else {
            return false;
        };
        if state.session_id.is_none() {
            return false;
        }
        state.session_id = None;
        true
    }

    pub fn refresh_file_picker_results_if_open(&mut self) -> Result<bool, String> {
        if !self.is_file_picker_open() {
            return Ok(false);
        }
        if matches!(
            self.command_palette.mode,
            CommandPaletteMode::FilePicker
                | CommandPaletteMode::LiveGrep
                | CommandPaletteMode::LspReferences
        ) {
            return Ok(false);
        }
        let workspace = self
            .workspace_model
            .as_ref()
            .ok_or_else(|| "workspace is not attached".to_string())?;
        let changed = self.command_palette.refresh_if_open(Some(workspace));
        if changed {
            self.sync_file_picker_cache();
        }
        Ok(changed)
    }

    pub fn external_conflict_message(&self) -> Option<&str> {
        self.external_conflict.as_deref()
    }

    pub fn last_external_notice(&self) -> Option<&str> {
        self.external_notice.as_deref()
    }

    pub fn apply_external_file_events(
        &mut self,
        events: &[FileSystemEvent],
    ) -> Result<ExternalChangeReport, String> {
        let mut report = ExternalChangeReport::default();
        if events.is_empty() {
            return Ok(report);
        }

        let requires_workspace_rescan = events.iter().any(|event| {
            matches!(
                event.kind,
                FileSystemChangeKind::Create
                    | FileSystemChangeKind::Delete
                    | FileSystemChangeKind::Rename
            ) || (matches!(event.kind, FileSystemChangeKind::Modify) && !event.path.exists())
        });

        // Chỉ rescan khi tree shape có thể đổi (create/delete/rename).
        // Modify-only thường không đổi cấu trúc workspace, tránh quét cả cây quá nhiều.
        if requires_workspace_rescan && let Some(workspace) = self.workspace_model.as_mut() {
            workspace.rescan()?;
            report.workspace_reloaded = true;
        }

        if report.workspace_reloaded && self.is_file_picker_open() {
            if self.refresh_file_picker_results_if_open()? {
                let note = format!(
                    "file picker refreshed ({} results)",
                    self.file_picker_results().len()
                );
                self.external_notice = Some(note.clone());
                report.notices.push(note);
            }
        }

        let Some(active_path) = self.active_file.clone() else {
            return Ok(report);
        };

        for event in events {
            let touches_active = path_matches(&event.path, &active_path)
                || event
                    .new_path
                    .as_ref()
                    .is_some_and(|new_path| path_matches(new_path, &active_path));
            if !touches_active {
                continue;
            }

            if self.is_dirty() {
                let warning = format!(
                    "external {:?} detected on active file while dirty: {}",
                    event.kind,
                    active_path.display()
                );
                self.external_conflict = Some(warning.clone());
                self.external_notice = Some(warning.clone());
                report.conflict_detected = true;
                report.notices.push(warning);
                continue;
            }

            match event.kind {
                FileSystemChangeKind::Modify | FileSystemChangeKind::Create => {
                    if matches!(event.kind, FileSystemChangeKind::Modify)
                        && self.should_ignore_self_save_event()
                    {
                        continue;
                    }

                    match self.load_buffer_from_file(&active_path) {
                        Ok(()) => {
                            self.active_file = Some(active_path.clone());
                            self.register_open_text_buffer(active_path.clone());
                            self.dirty = false;
                            let note = format!(
                                "auto reloaded active file from disk: {}",
                                active_path.display()
                            );
                            self.external_notice = Some(note.clone());
                            self.external_conflict = None;
                            report.active_file_reloaded = true;
                            report.notices.push(note);
                        }
                        Err(err) => {
                            let note = format!(
                                "auto reload skipped for active file {}: {}",
                                active_path.display(),
                                err
                            );
                            self.external_notice = Some(note.clone());
                            report.notices.push(note);
                        }
                    }
                }
                FileSystemChangeKind::Rename => {
                    if let Some(new_path) = &event.new_path {
                        match self.open_file(new_path.clone()) {
                            Ok(()) => {
                                let note = format!(
                                    "active file renamed externally, reloaded: {} -> {}",
                                    active_path.display(),
                                    new_path.display()
                                );
                                self.external_notice = Some(note.clone());
                                self.external_conflict = None;
                                report.active_file_reloaded = true;
                                report.notices.push(note);
                            }
                            Err(err) => {
                                let note = format!(
                                    "active file rename detected but reload failed {} -> {}: {}",
                                    active_path.display(),
                                    new_path.display(),
                                    err
                                );
                                self.external_notice = Some(note.clone());
                                report.notices.push(note);
                            }
                        }
                    }
                }
                FileSystemChangeKind::Delete => {
                    let note = format!(
                        "active file deleted externally: {} (buffer kept in memory)",
                        active_path.display()
                    );
                    self.external_notice = Some(note.clone());
                    report.notices.push(note);
                }
            }
        }

        Ok(report)
    }

    pub fn insert_char(&mut self, ch: char) {
        self.apply_insert(self.cursor_char_idx, ch.to_string());
        self.cursor_char_idx += 1;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
    }

    pub fn step_over_closing_char(&mut self, ch: char) -> bool {
        if self.char_at_cursor() != Some(ch) {
            return false;
        }

        let before_cursor = self.cursor_char_idx;
        self.move_right();
        if self.cursor_char_idx == before_cursor {
            return false;
        }

        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn insert_auto_pair(&mut self, open: char) -> bool {
        let Some(close) = matching_close_char(open) else {
            return false;
        };

        let insert_at = self.cursor_char_idx;
        if !self.apply_insert(insert_at, format!("{open}{close}")) {
            return false;
        }

        self.cursor_char_idx = insert_at + 1;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn smart_insert_newline(&mut self) -> bool {
        let left = self.char_before_cursor();
        let right = self.char_at_cursor();

        if !matches_matching_bracket_pair(left, right) {
            self.insert_char('\n');
            return true;
        }

        let (line_idx, _) = self.cursor_line_col();
        let current_indent = self.line_indent_string(line_idx);
        let indent_unit = self.indent_unit_for_line(&current_indent);
        let insert_at = self.cursor_char_idx;
        let inserted = format!("\n{}{}\n{}", current_indent, indent_unit, current_indent);

        if !self.apply_insert(insert_at, inserted) {
            return false;
        }

        let cursor_after_first_line_break =
            insert_at + 1 + current_indent.chars().count() + indent_unit.chars().count();
        self.cursor_char_idx = cursor_after_first_line_break;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn backspace(&mut self) -> bool {
        if self.visual_selection_range().is_some() {
            return self.delete_visual_selection();
        }

        if self.cursor_char_idx == 0 {
            return false;
        }

        let start = self.cursor_char_idx - 1;
        let delete_len = match (self.char_before_cursor(), self.char_at_cursor()) {
            (Some(left), Some(right)) if matches_matching_bracket_pair(Some(left), Some(right)) => {
                2
            }
            _ => 1,
        };

        if !self.apply_delete(start, delete_len) {
            return false;
        }

        self.cursor_char_idx = start;

        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn insert_line_below(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let insert_at = self.line_content_end_char_idx(line_idx);

        self.apply_insert(insert_at, "\n".to_string());
        let target_line = (line_idx + 1).min(self.text.len_lines().saturating_sub(1));
        self.cursor_char_idx = self.text.line_to_char(target_line);
        self.target_col = 0;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn insert_line_above(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_start = self.text.line_to_char(line_idx);

        self.apply_insert(line_start, "\n".to_string());
        self.cursor_char_idx = line_start;
        self.target_col = 0;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn move_to_line_first_non_blank(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_start = self.text.line_to_char(line_idx);
        let line_end = self.line_content_end_char_idx(line_idx);

        let mut target_idx = line_start;
        for char_idx in line_start..line_end {
            let ch = self.text.char(char_idx);
            if ch != ' ' && ch != '\t' {
                target_idx = char_idx;
                break;
            }
        }

        let previous_cursor = self.cursor_char_idx;
        let changed = self.update_cursor_position(target_idx);
        if self.cursor_char_idx != previous_cursor {
            let (_, col) = self.cursor_line_col();
            self.target_col = col;
        }
        changed
    }

    pub fn move_to_line_start(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let target_idx = self.text.line_to_char(line_idx);
        let changed = self.update_cursor_position(target_idx);
        self.target_col = 0;
        changed
    }

    // ── Leap navigation helpers ────────────────────────────────────────────────

    /// Nhảy cursor trực tiếp đến char_idx (Leap jump).
    /// Sử dụng `update_cursor_position` để đảm bảo clamp và revision bump.
    pub fn leap_jump_to_char(&mut self, char_idx: usize) -> bool {
        let clamped = char_idx.min(self.text.len_chars().saturating_sub(1));
        let changed = self.update_cursor_position(clamped);
        if changed {
            let (_, col) = self.cursor_line_col();
            self.target_col = col;
            self.bump_revision();
        }
        changed
    }

    /// Trả về char_idx đầu tiên của một line (dùng để tính viewport scan range).
    pub fn char_idx_for_line(&self, line: usize) -> usize {
        let clamped = line.min(self.text.len_lines());
        self.text.line_to_char(clamped)
    }

    /// Tổng số chars trong text (để Leap biết khi nào dừng scan).
    pub fn text_len_chars(&self) -> usize {
        self.text.len_chars()
    }

    /// Convert byte offset trong một line sang char offset trong cùng line đó.
    /// Dùng bởi renderer để map cosmic-text glyph.start → char_idx trong Rope.
    pub fn byte_to_char_in_line(&self, line_idx: usize, byte_in_line: usize) -> usize {
        let line = self
            .text
            .line(line_idx.min(self.text.len_lines().saturating_sub(1)));
        // Rope::line() trả về RopeSlice — đếm chars tới byte_in_line
        let mut char_count = 0usize;
        let mut byte_count = 0usize;
        for ch in line.chars() {
            if byte_count >= byte_in_line {
                break;
            }
            byte_count += ch.len_utf8();
            char_count += 1;
        }
        char_count
    }

    pub fn move_to_first_non_whitespace(&mut self) -> bool {
        self.move_to_line_first_non_blank()
    }

    pub fn move_to_line_end(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let target_idx = self.line_content_end_char_idx(line_idx);
        let changed = self.update_cursor_position(target_idx);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        changed
    }

    pub fn move_word_forward(&mut self) -> bool {
        let next_idx = next_word_start(&self.text, self.cursor_char_idx);
        let changed = self.update_cursor_position(next_idx);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        changed
    }

    pub fn move_word_backward(&mut self) -> bool {
        let next_idx = previous_word_start(&self.text, self.cursor_char_idx);
        let changed = self.update_cursor_position(next_idx);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        changed
    }

    pub fn move_word_end(&mut self) -> bool {
        let next_idx =
            word_end_at_or_after(&self.text, self.cursor_char_idx).unwrap_or(self.cursor_char_idx);
        let changed = self.update_cursor_position(next_idx);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        changed
    }

    pub fn append_after_cursor(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_end = self.line_content_end_char_idx(line_idx);
        let target_idx = if self.cursor_char_idx < line_end {
            self.cursor_char_idx + 1
        } else {
            line_end
        };

        let changed = self.update_cursor_position(target_idx);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        changed
    }

    pub fn substitute_current_line(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_start = self.text.line_to_char(line_idx);
        let line_end = self.line_content_end_char_idx(line_idx);
        let old_cursor = self.cursor_char_idx;

        let text_changed = line_end > line_start;
        if text_changed {
            self.apply_delete(line_start, line_end - line_start);
            self.dirty = true;
        }

        self.cursor_char_idx = line_start.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;

        let cursor_changed = self.cursor_char_idx != old_cursor;
        if text_changed || cursor_changed {
            self.bump_revision();
            return true;
        }
        false
    }

    pub fn delete_char_at_cursor(&mut self) -> bool {
        if self.text.len_chars() == 0 {
            return false;
        }

        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        let line_start = self.text.line_to_char(line_idx);
        let line_end = self.line_content_end_char_idx(line_idx);
        if line_start == line_end {
            return false;
        }

        let mut delete_idx = if self.cursor_char_idx < line_end {
            self.cursor_char_idx
        } else {
            line_end.saturating_sub(1)
        };
        if delete_idx < line_start {
            delete_idx = line_start;
        }
        if delete_idx >= self.text.len_chars() {
            return false;
        }

        self.apply_delete(delete_idx, 1);
        self.cursor_char_idx = delete_idx.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    /// Vim `dw`: delete from cursor to the start of the next word on the same line,
    /// including trailing spaces/tabs. If cursor sits on a newline, delete just that
    /// newline (join with next line). No-op at EOF.
    pub fn delete_word_forward(&mut self) -> bool {
        let n = self.text.len_chars();
        if self.cursor_char_idx >= n {
            return false;
        }

        let end = next_word_start(&self.text, self.cursor_char_idx);
        if end <= self.cursor_char_idx {
            return false;
        }

        self.apply_delete(self.cursor_char_idx, end - self.cursor_char_idx);
        // Cursor stays at the same char index — the tail shifted in. Clamp in case
        // we just deleted everything after it (including a trailing newline).
        self.cursor_char_idx = self.cursor_char_idx.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn delete_word_backward(&mut self) -> bool {
        if self.cursor_char_idx == 0 {
            return false;
        }

        let start = previous_word_start(&self.text, self.cursor_char_idx);
        if start >= self.cursor_char_idx {
            return false;
        }

        self.apply_delete(start, self.cursor_char_idx - start);
        self.cursor_char_idx = start;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn change_word_forward(&mut self) -> bool {
        let n = self.text.len_chars();
        if self.cursor_char_idx >= n {
            return false;
        }

        let end = next_word_start(&self.text, self.cursor_char_idx);
        if end <= self.cursor_char_idx {
            return false;
        }

        self.apply_delete(self.cursor_char_idx, end - self.cursor_char_idx);
        self.cursor_char_idx = self.cursor_char_idx.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn change_word_backward(&mut self) -> bool {
        if self.cursor_char_idx == 0 {
            return false;
        }

        let start = previous_word_start(&self.text, self.cursor_char_idx);
        if start >= self.cursor_char_idx {
            return false;
        }

        self.apply_delete(start, self.cursor_char_idx - start);
        self.cursor_char_idx = start;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn replace_char_at_cursor(&mut self, ch: char) -> bool {
        if self.text.len_chars() == 0 {
            return false;
        }

        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        let line_start = self.text.line_to_char(line_idx);
        let line_end = self.line_content_end_char_idx(line_idx);
        if line_start == line_end {
            return false;
        }

        let mut replace_idx = if self.cursor_char_idx < line_end {
            self.cursor_char_idx
        } else {
            line_end.saturating_sub(1)
        };
        if replace_idx < line_start {
            replace_idx = line_start;
        }
        if replace_idx >= self.text.len_chars() {
            return false;
        }

        self.apply_delete(replace_idx, 1);
        self.apply_insert(replace_idx, ch.to_string());
        self.cursor_char_idx = replace_idx.min(self.text.len_chars().saturating_sub(1));
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn delete_current_line(&mut self) -> bool {
        if self.text.len_lines() == 0 {
            return false;
        }

        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        let line_start = self.text.line_to_char(line_idx);
        let mut line_end = if line_idx + 1 < self.text.len_lines() {
            self.text.line_to_char(line_idx + 1)
        } else {
            self.text.len_chars()
        };
        let mut delete_start = line_start;

        // Trailing empty line (from a final '\n') has an empty range.
        // Delete the previous '\n' so one logical line is removed.
        if delete_start == line_end && line_idx > 0 {
            delete_start = delete_start.saturating_sub(1);
            line_end = line_end.max(delete_start);
        }

        if delete_start == line_end {
            return false;
        }

        self.apply_delete(delete_start, line_end - delete_start);
        let remaining_lines = self.text.len_lines();
        let target_line = line_idx.min(remaining_lines.saturating_sub(1));
        self.cursor_char_idx = self.text.line_to_char(target_line);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn move_left(&mut self) {
        let (line_idx, col) = self.cursor_line_col();
        let previous_cursor = self.cursor_char_idx;
        let mut target_idx = self.cursor_char_idx;
        let mut next_target_col = self.target_col;

        if col > 0 {
            target_idx = self.cursor_char_idx.saturating_sub(1);
            next_target_col = col - 1;
        } else if line_idx > 0 {
            // Ở đầu dòng và đi trái -> sang cuối dòng trước.
            target_idx = self.cursor_char_idx.saturating_sub(1);
            next_target_col = self.max_col_for_line(line_idx - 1);
        }

        let _ = self.update_cursor_position(target_idx);
        if self.cursor_char_idx != previous_cursor {
            self.target_col = next_target_col;
        }
    }

    pub fn move_right(&mut self) {
        let (line_idx, col) = self.cursor_line_col();
        let max_col = self.max_col_for_line(line_idx);
        let previous_cursor = self.cursor_char_idx;
        let mut target_idx = self.cursor_char_idx;
        let mut next_target_col = self.target_col;
        if col < max_col {
            target_idx = self.cursor_char_idx + 1;
            next_target_col = col + 1;
        } else {
            // Ở cuối dòng, ArrowRight sẽ nhảy sang đầu dòng kế tiếp (nếu có).
            let total_lines = self.text.len_lines();
            if line_idx + 1 < total_lines {
                target_idx = self.text.line_to_char(line_idx + 1);
                next_target_col = 0;
            }
        }

        let _ = self.update_cursor_position(target_idx);
        if self.cursor_char_idx != previous_cursor {
            self.target_col = next_target_col;
        }
    }

    pub fn move_up(&mut self) {
        let (line_idx, _) = self.cursor_line_col();
        let next_idx = if line_idx == 0 {
            self.cursor_char_idx
        } else {
            let target_line = line_idx - 1;
            let line_start = self.text.line_to_char(target_line);
            let new_col = self.target_col.min(self.max_col_for_line(target_line));
            line_start + new_col
        };
        let _ = self.update_cursor_position(next_idx);
    }

    pub fn move_down(&mut self) {
        let (line_idx, _) = self.cursor_line_col();
        let total_lines = self.text.len_lines();
        let next_idx = if line_idx + 1 >= total_lines {
            self.cursor_char_idx
        } else {
            let target_line = line_idx + 1;
            let line_start = self.text.line_to_char(target_line);
            let new_col = self.target_col.min(self.max_col_for_line(target_line));
            line_start + new_col
        };
        let _ = self.update_cursor_position(next_idx);
    }

    pub fn total_lines(&self) -> usize {
        self.text.len_lines()
    }

    pub fn move_to_first_line(&mut self) -> bool {
        let cursor_changed = self.update_cursor_position(0);
        let scroll_changed = if self.scroll_line != 0 {
            self.scroll_line = 0;
            true
        } else {
            false
        };
        self.target_col = 0;
        if scroll_changed && !cursor_changed {
            self.bump_revision();
        }
        cursor_changed || scroll_changed
    }

    pub fn move_to_last_line(&mut self) -> bool {
        let total = self.text.len_lines();
        let last = if total > 0 { total - 1 } else { 0 };
        let changed = self.update_cursor_position(self.text.line_to_char(last));
        self.target_col = 0;
        changed
    }

    /// Nhảy đến `line_idx` (0-indexed). Dùng bởi `:N` vim command.
    /// Trả về true nếu cursor thực sự thay đổi.
    pub fn jump_to_line(&mut self, line_idx: usize) -> bool {
        let total = self.text.len_lines();
        let target_line = line_idx.min(total.saturating_sub(1));
        let char_idx = self.text.line_to_char(target_line);
        let changed = self.update_cursor_position(char_idx);
        self.target_col = 0;
        // Scroll: đặt target_line vào giữa màn hình nếu scroll_line cần update
        if changed {
            // Dùng auto_scroll_to_cursor sẽ được gọi bởi renderer
            // Ở đây chỉ reset scroll_line về target_line để viewport thấy dòng đó
            self.scroll_line = target_line.saturating_sub(10);
            self.bump_revision();
        }
        changed
    }

    pub fn center_cursor_line(&mut self, viewport_lines: usize) {
        let (cursor_line, _) = self.cursor_line_col();
        self.scroll_line = cursor_line.saturating_sub(viewport_lines / 2);
    }

    pub fn scroll_half_page_up(&mut self, half: usize) {
        self.scroll_line = self.scroll_line.saturating_sub(half);
        let new_line = self.cursor_line_col().0.saturating_sub(half);
        self.cursor_char_idx = self.text.line_to_char(new_line);
        self.target_col = 0;
        self.bump_revision();
    }

    pub fn scroll_half_page_down(&mut self, half: usize) {
        let total = self.text.len_lines();
        self.scroll_line = (self.scroll_line + half).min(total.saturating_sub(1));
        let new_line = (self.cursor_line_col().0 + half).min(total.saturating_sub(1));
        self.cursor_char_idx = self.text.line_to_char(new_line);
        self.target_col = 0;
        self.bump_revision();
    }

    /// Adjust scroll_line so the cursor is within the viewport.
    pub fn auto_scroll_to_cursor(&mut self, viewport_lines: usize) {
        let (cursor_line, _) = self.cursor_line_col();
        let margin = 3usize;
        if cursor_line < self.scroll_line + margin {
            self.scroll_line = cursor_line.saturating_sub(margin);
        } else if viewport_lines > margin
            && cursor_line + margin >= self.scroll_line + viewport_lines
        {
            self.scroll_line = cursor_line + margin + 1 - viewport_lines;
        }
    }

    pub fn open_file(&mut self, path: PathBuf) -> Result<(), String> {
        let canonical_path = path
            .canonicalize()
            .map_err(|err| format!("canonicalize file {:?} failed: {err}", path))?;
        let language_id = crate::lsp::registry::language_profile_for_path(&canonical_path)
            .map(|profile| profile.language_id.to_string());
        let active_idx = match self
            .buffers
            .iter()
            .position(|buffer| matches!(&buffer.content, BufferContent::Text(buffer) if buffer.path == canonical_path))
        {
            Some(idx) => idx,
            None => {
                self.buffers.push(BufferEntry {
                    content: BufferContent::Text(EditorBuffer {
                        path: canonical_path.clone(),
                        language_id,
                    }),
                });
                self.buffers.len().saturating_sub(1)
            }
        };
        self.activate_buffer_index(active_idx)?;
        Ok(())
    }

    pub fn save_file(&mut self) -> Result<PathBuf, String> {
        if self.active_buffer_is_terminal() {
            return Err("cannot save terminal buffer".to_string());
        }
        if self.active_buffer_is_references() {
            return Err("cannot save references buffer".to_string());
        }
        if self.active_buffer_is_diagnostics() {
            return Err("cannot save diagnostics buffer".to_string());
        }

        let path = self
            .active_file
            .clone()
            .unwrap_or_else(|| self.default_save_path.clone());

        fs::write(&path, self.text.to_string())
            .map_err(|err| format!("save file {:?} failed: {err}", path))?;
        self.last_saved_at = Some(Instant::now());

        let canonical_path = path
            .canonicalize()
            .map_err(|err| format!("canonicalize saved file {:?} failed: {err}", path))?;

        self.active_file = Some(canonical_path.clone());
        self.register_open_text_buffer(canonical_path.clone());
        let _ = self.workspace_expand_to_path(&canonical_path);
        self.dirty = false;
        Ok(canonical_path)
    }

    pub fn new_empty_buffer(&mut self) -> bool {
        let changed = self.active_file.is_some()
            || self.active_buffer_index.is_some()
            || self.dirty
            || self.text.len_chars() > 0;
        if !changed {
            return false;
        }

        self.reset_text_editor_state();
        self.active_buffer_index = None;
        let _ = self.clear_current_overlays();
        self.bump_revision();
        true
    }

    pub fn buffer_next(&mut self) -> Result<bool, String> {
        self.cycle_buffer(true)
    }

    pub fn buffer_prev(&mut self) -> Result<bool, String> {
        self.cycle_buffer(false)
    }

    pub fn close_current_buffer(&mut self) -> Result<bool, String> {
        let Some(current_idx) = self.active_buffer_index else {
            return Ok(false);
        };

        self.buffers.remove(current_idx);
        if self.buffers.is_empty() {
            self.reset_text_editor_state();
            self.active_buffer_index = None;
            let _ = self.clear_current_overlays();
            self.bump_revision();
            return Ok(true);
        }

        let mut next_idx = current_idx.min(self.buffers.len().saturating_sub(1));
        while !self.buffers.is_empty() {
            match self.activate_buffer_index(next_idx) {
                Ok(()) => return Ok(true),
                Err(_) => {
                    self.buffers.remove(next_idx);
                    if self.buffers.is_empty() {
                        return Ok(self.new_empty_buffer());
                    }
                    if next_idx >= self.buffers.len() {
                        next_idx = 0;
                    }
                }
            }
        }

        Ok(self.new_empty_buffer())
    }

    pub fn begin_visual_selection(&mut self) -> bool {
        self.visual_line_mode = false;
        let anchor = if self.text.len_chars() == 0 {
            0
        } else {
            self.cursor_char_idx
                .min(self.text.len_chars().saturating_sub(1))
        };
        if self.selection_anchor_char_idx == Some(anchor) {
            return false;
        }
        self.selection_anchor_char_idx = Some(anchor);
        true
    }

    pub fn begin_visual_line_selection(&mut self) -> bool {
        self.visual_line_mode = true;
        let line_idx = self.text.char_to_line(
            self.cursor_char_idx
                .min(self.text.len_chars().saturating_sub(1).max(0)),
        );
        let anchor = self.text.line_to_char(line_idx);
        self.selection_anchor_char_idx = Some(anchor);
        // Move cursor to the last char of the line (before newline)
        let line_end = self.line_content_end_char_idx(line_idx);
        if self.cursor_char_idx != line_end {
            self.cursor_char_idx = line_end;
            let (_, col) = self.cursor_line_col();
            self.target_col = col;
        }
        true
    }

    pub fn clear_visual_selection(&mut self) -> bool {
        if self.selection_anchor_char_idx.is_none() && !self.visual_line_mode {
            return false;
        }
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        true
    }

    pub fn visual_selection_range(&self) -> Option<VisualSelectionRange> {
        if self.current_mode() != EditorMode::Visual {
            return None;
        }
        let anchor = self.selection_anchor_char_idx?;
        let len_chars = self.text.len_chars();
        if len_chars == 0 {
            return None;
        }

        let anchor_idx = anchor.min(len_chars.saturating_sub(1));
        let focus_idx = self.cursor_char_idx.min(len_chars.saturating_sub(1));
        let (start_char, end_char) = if self.visual_line_mode {
            // Expand to full lines: from start of first line to start of line after last
            let first_line = self.text.char_to_line(anchor_idx.min(focus_idx));
            let last_line = self.text.char_to_line(anchor_idx.max(focus_idx));
            let sc = self.text.line_to_char(first_line);
            let ec = if last_line + 1 < self.text.len_lines() {
                self.text.line_to_char(last_line + 1)
            } else {
                len_chars
            };
            (sc, ec)
        } else {
            let sc = anchor_idx.min(focus_idx);
            let ec = anchor_idx.max(focus_idx).saturating_add(1).min(len_chars);
            (sc, ec)
        };

        if start_char >= end_char {
            return None;
        }

        let start_line = self.text.char_to_line(start_char);
        let end_line = self.text.char_to_line(end_char.saturating_sub(1));
        let start_byte = self.text.char_to_byte(start_char);
        let end_byte = self.text.char_to_byte(end_char);
        let start_byte_in_line = start_byte.saturating_sub(self.text.line_to_byte(start_line));
        let end_byte_in_line = end_byte.saturating_sub(self.text.line_to_byte(end_line));

        Some(VisualSelectionRange {
            start_char,
            end_char,
            start_line,
            end_line,
            start_byte_in_line,
            end_byte_in_line,
        })
    }

    pub fn visual_selection_text(&self) -> Option<String> {
        let selection = self.visual_selection_range()?;
        self.char_range_text(selection.start_char, selection.end_char)
    }

    pub fn delete_char_text_at_cursor(&self) -> Option<String> {
        let (start, end) = self.delete_char_range_at_cursor()?;
        self.char_range_text(start, end)
    }

    pub fn delete_current_line_text(&self) -> Option<String> {
        let (start, end) = self.current_line_delete_range()?;
        self.linewise_text_for_range(start, end)
    }

    pub fn yank_current_line_text(&self) -> Option<String> {
        let (start, end) = self.current_line_delete_range()?;
        self.linewise_text_for_range(start, end)
    }

    pub fn delete_word_forward_text(&self) -> Option<String> {
        let (start, end) = self.delete_word_forward_range()?;
        self.char_range_text(start, end)
    }

    pub fn yank_to_word_end_text(&self) -> Option<String> {
        let (start, end) = self.yank_word_end_range()?;
        self.char_range_text(start, end)
    }

    pub fn delete_word_backward_text(&self) -> Option<String> {
        let (start, end) = self.delete_word_backward_range()?;
        self.char_range_text(start, end)
    }

    pub fn substitute_current_line_text(&self) -> Option<String> {
        let (start, end) = self.current_line_content_range()?;
        self.char_range_text(start, end)
    }

    pub fn delete_visual_selection(&mut self) -> bool {
        let Some(selection) = self.visual_selection_range() else {
            return false;
        };

        self.apply_delete(
            selection.start_char,
            selection.end_char - selection.start_char,
        );
        self.cursor_char_idx = selection.start_char.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.selection_anchor_char_idx = None;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn select_text_object(
        &mut self,
        modifier: TextObjectModifier,
        kind: TextObjectKind,
    ) -> bool {
        let Some((start, end)) =
            find_text_object_range(&self.text, self.cursor_char_idx, modifier, kind)
        else {
            return false;
        };
        let len = self.text.len_chars();
        if len == 0 {
            return false;
        }

        // Clamp để tránh out-of-bounds.
        let anchor = start.min(len.saturating_sub(1));
        let focus = end.saturating_sub(1).min(len.saturating_sub(1));

        // Chuyển sang Visual mode nếu cần.
        if self.current_mode() != EditorMode::Visual {
            if self.can_apply_mode_event(ModeEvent::EnterVisual) {
                let _ = self.apply_mode_event(ModeEvent::EnterVisual);
            } else {
                return false;
            }
        }

        self.visual_line_mode = false;
        self.selection_anchor_char_idx = Some(anchor);
        self.cursor_char_idx = focus;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.bump_revision();
        true
    }

    /// Lấy char range text cho một text object (dùng trước khi xóa/yank).
    pub fn text_object_text(
        &self,
        modifier: TextObjectModifier,
        kind: TextObjectKind,
    ) -> Option<String> {
        let (start, end) =
            find_text_object_range(&self.text, self.cursor_char_idx, modifier, kind)?;
        self.char_range_text(start, end)
    }

    /// Xóa text object tại vị trí con trỏ và trả về true nếu thành công.
    pub fn delete_text_object(
        &mut self,
        modifier: TextObjectModifier,
        kind: TextObjectKind,
    ) -> bool {
        let Some((start, end)) =
            find_text_object_range(&self.text, self.cursor_char_idx, modifier, kind)
        else {
            return false;
        };
        if start >= end {
            return false;
        }
        self.apply_delete(start, end - start);
        self.cursor_char_idx = start.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn paste_after(&mut self, text: &str) -> bool {
        let insert_text = text.to_string();
        if insert_text.is_empty() {
            return false;
        }

        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        let line_end = self.line_content_end_char_idx(line_idx);
        let insert_at = if self.cursor_char_idx < line_end {
            self.cursor_char_idx + 1
        } else {
            line_end
        };

        if !self.apply_insert(insert_at, insert_text.clone()) {
            return false;
        }

        let inserted_chars = insert_text.chars().count();
        self.cursor_char_idx =
            (insert_at + inserted_chars.saturating_sub(1)).min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn paste_before(&mut self, text: &str) -> bool {
        let insert_text = text.to_string();
        if insert_text.is_empty() {
            return false;
        }

        let insert_at = self.cursor_char_idx.min(self.text.len_chars());
        if !self.apply_insert(insert_at, insert_text.clone()) {
            return false;
        }

        let inserted_chars = insert_text.chars().count();
        self.cursor_char_idx =
            (insert_at + inserted_chars.saturating_sub(1)).min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn insert_text_at_cursor(&mut self, text: &str) -> bool {
        let insert_text = text.to_string();
        if insert_text.is_empty() {
            return false;
        }

        let insert_at = self.cursor_char_idx.min(self.text.len_chars());
        if !self.apply_insert(insert_at, insert_text.clone()) {
            return false;
        }

        let inserted_chars = insert_text.chars().count();
        self.cursor_char_idx = (insert_at + inserted_chars).min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn replace_completion_prefix_at_cursor(
        &mut self,
        prefix_len_chars: usize,
        text: &str,
    ) -> bool {
        let insert_text = text.to_string();
        if insert_text.is_empty() {
            return false;
        }

        let cursor = self.cursor_char_idx.min(self.text.len_chars());
        let line_idx = self.text.char_to_line(cursor);
        let line_start = self.text.line_to_char(line_idx);
        let delete_start = cursor.saturating_sub(prefix_len_chars).max(line_start);
        let delete_len = cursor.saturating_sub(delete_start);

        let mut changed = false;
        if delete_len > 0 {
            changed |= self.apply_delete(delete_start, delete_len);
        }
        changed |= self.apply_insert(delete_start, insert_text.clone());
        if !changed {
            return false;
        }

        self.cursor_char_idx = delete_start + insert_text.chars().count();
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        let _ = self.commit_transaction();
        self.bump_revision();
        true
    }

    pub fn paste_linewise_after(&mut self, text: &str) -> bool {
        self.paste_linewise(text, false)
    }

    pub fn paste_linewise_before(&mut self, text: &str) -> bool {
        self.paste_linewise(text, true)
    }

    pub fn toggle_line_comment(&mut self) -> bool {
        let (line_idx, _) = self.cursor_line_col();
        self.toggle_comments_on_lines(line_idx, line_idx)
    }

    pub fn toggle_selection_comment(&mut self) -> bool {
        let Some(selection) = self.visual_selection_range() else {
            return false;
        };
        self.toggle_comments_on_lines(selection.start_line, selection.end_line)
    }

    pub fn commit_transaction(&mut self) -> bool {
        let Some(mut transaction) = self.current_transaction.take() else {
            return false;
        };
        if transaction.is_empty() {
            return false;
        }

        transaction.after_cursor = self.cursor_state();
        self.history.undo_stack.push(transaction);
        self.history.redo_stack.clear();
        true
    }

    pub fn undo(&mut self) -> bool {
        let Some(transaction) = self.history.undo_stack.pop() else {
            return false;
        };

        self.current_transaction = None;
        for action in transaction.actions.iter().rev() {
            match action {
                EditAction::Insert { index, text } => {
                    self.record_delete_highlight_edit(*index, text.chars().count());
                    let _ = self.apply_delete_raw(*index, text.chars().count());
                }
                EditAction::Delete { index, text } => {
                    self.record_insert_highlight_edit(*index, text);
                    self.apply_insert_raw(*index, text);
                }
            }
        }

        self.restore_cursor_state(transaction.before_cursor);
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        self.dirty = true;
        self.history.redo_stack.push(transaction);
        self.bump_revision();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(transaction) = self.history.redo_stack.pop() else {
            return false;
        };

        self.current_transaction = None;
        for action in &transaction.actions {
            match action {
                EditAction::Insert { index, text } => {
                    self.record_insert_highlight_edit(*index, text);
                    self.apply_insert_raw(*index, text);
                }
                EditAction::Delete { index, text } => {
                    self.record_delete_highlight_edit(*index, text.chars().count());
                    let _ = self.apply_delete_raw(*index, text.chars().count());
                }
            }
        }

        self.restore_cursor_state(transaction.after_cursor);
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        self.dirty = true;
        self.history.undo_stack.push(transaction);
        self.bump_revision();
        true
    }

    pub fn cursor_line_col(&self) -> (usize, usize) {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_start = self.text.line_to_char(line_idx);
        let col_idx = self.cursor_char_idx - line_start;
        (line_idx, col_idx)
    }

    pub fn cursor_char_idx(&self) -> usize {
        self.cursor_char_idx
    }

    pub fn cursor_byte_idx(&self) -> usize {
        self.text.char_to_byte(self.cursor_char_idx)
    }

    /// Byte offset tương đối trong dòng hiện tại (không phải toàn buffer).
    /// Hữu ích khi map với glyph.start/end của cosmic-text theo từng line run.
    pub fn cursor_byte_in_line(&self) -> usize {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_start_byte = self.text.line_to_byte(line_idx);
        self.cursor_byte_idx().saturating_sub(line_start_byte)
    }

    pub fn completion_prefix_info_at(
        &self,
        line_idx: usize,
        cursor_col: usize,
    ) -> CompletionPrefixInfo {
        if self.text.len_lines() == 0 {
            return CompletionPrefixInfo {
                start_col: 0,
                prefix: String::new(),
            };
        }

        let clamped_line = line_idx.min(self.text.len_lines().saturating_sub(1));
        let line_text = self.text.line(clamped_line).to_string();
        let line_content = line_text.strip_suffix('\n').unwrap_or(&line_text);
        let chars: Vec<char> = line_content.chars().collect();
        let cursor_col = cursor_col.min(chars.len());
        let mut start_col = cursor_col;

        while start_col > 0 && is_completion_identifier_char(chars[start_col - 1]) {
            start_col -= 1;
        }

        CompletionPrefixInfo {
            start_col,
            prefix: chars[start_col..cursor_col].iter().collect(),
        }
    }

    pub fn last_search_query(&self) -> &str {
        &self.last_search_query
    }

    pub fn search_highlights(&self) -> &[(usize, usize)] {
        &self.search_highlights
    }

    pub fn remember_clipboard_text(&mut self, text: String, kind: ClipboardRecordKind) {
        if text.is_empty() {
            return;
        }
        self.clipboard_record = Some(ClipboardRecord { text, kind });
    }

    pub fn clipboard_record_kind_for_text(&self, text: &str) -> Option<ClipboardRecordKind> {
        self.clipboard_record
            .as_ref()
            .filter(|record| record.text == text)
            .map(|record| record.kind)
    }

    pub fn set_in_file_search_query(&mut self, query: &str) -> bool {
        self.set_search_query_internal(query, false)
    }

    pub fn search_next(&mut self) -> bool {
        self.jump_to_search_match(true)
    }

    pub fn search_prev(&mut self) -> bool {
        self.jump_to_search_match(false)
    }

    pub fn search_word_under_cursor(&mut self) -> bool {
        let Some(query) = self.word_under_cursor() else {
            return false;
        };

        let changed = self.set_search_query_internal(&query, true);
        let moved = self.search_next();
        changed || moved
    }

    pub fn clear_search_highlights(&mut self) -> bool {
        self.set_search_query_internal("", false)
    }

    pub fn jump_to_line_and_column(&mut self, line_idx: usize, col_idx: usize) -> bool {
        if self.text.len_lines() == 0 {
            return false;
        }

        let target_line = line_idx.min(self.text.len_lines().saturating_sub(1));
        let line_start = self.text.line_to_char(target_line);
        let target_char = line_start + col_idx.min(self.max_col_for_line(target_line));
        self.move_cursor_to_char_idx(target_char)
    }

    pub fn byte_to_char_idx(&self, byte_idx: usize) -> usize {
        self.text.byte_to_char(byte_idx.min(self.text.len_bytes()))
    }

    pub fn byte_to_line_idx(&self, byte_idx: usize) -> usize {
        if self.text.len_bytes() == 0 {
            return 0;
        }
        self.text
            .byte_to_line(byte_idx.min(self.text.len_bytes().saturating_sub(1)))
    }

    pub fn line_start_byte_idx(&self, line_idx: usize) -> usize {
        if self.text.len_lines() == 0 {
            return 0;
        }
        self.text
            .line_to_byte(line_idx.min(self.text.len_lines().saturating_sub(1)))
    }

    pub fn line_end_byte_idx(&self, line_idx: usize) -> usize {
        if self.text.len_lines() == 0 {
            return 0;
        }
        let clamped = line_idx.min(self.text.len_lines().saturating_sub(1));
        if clamped + 1 < self.text.len_lines() {
            self.text.line_to_byte(clamped + 1)
        } else {
            self.text.len_bytes()
        }
    }

    pub fn line_content_end_byte_idx(&self, line_idx: usize) -> usize {
        let line_end_char = self.line_content_end_char_idx(line_idx);
        self.text.char_to_byte(line_end_char)
    }

    pub fn line_char_to_byte_idx(&self, line_idx: usize, char_in_line: usize) -> usize {
        if self.text.len_lines() == 0 {
            return 0;
        }

        let clamped_line = line_idx.min(self.text.len_lines().saturating_sub(1));
        let line_start_char = self.text.line_to_char(clamped_line);
        let target_char = line_start_char + char_in_line.min(self.max_col_for_line(clamped_line));
        self.text.char_to_byte(target_char)
    }

    pub fn text_len_bytes(&self) -> usize {
        self.text.len_bytes()
    }

    pub fn text_string(&self) -> String {
        self.text.to_string()
    }

    pub fn take_highlight_edits(&mut self) -> Vec<HighlightEdit> {
        std::mem::take(&mut self.pending_highlight_edits)
    }

    /// Lấy prefix text để render mode file lớn mà không cần clone toàn bộ buffer.
    pub fn prefix_text(&self, max_chars: usize) -> String {
        self.text.chars().take(max_chars).collect()
    }

    pub fn active_file(&self) -> Option<&Path> {
        self.active_file.as_deref()
    }

    pub fn buffers(&self) -> &[BufferEntry] {
        &self.buffers
    }

    pub fn active_buffer_index(&self) -> Option<usize> {
        self.active_buffer_index
    }

    pub fn active_buffer(&self) -> Option<&BufferEntry> {
        self.active_buffer_index
            .and_then(|idx| self.buffers.get(idx))
    }

    pub fn active_text_buffer(&self) -> Option<&EditorBuffer> {
        let buffer = self.active_buffer()?;
        match &buffer.content {
            BufferContent::Text(text) => Some(text),
            _ => None,
        }
    }

    pub fn active_buffer_is_terminal(&self) -> bool {
        self.active_buffer()
            .is_some_and(|buffer| matches!(buffer.content, BufferContent::Terminal(_)))
    }

    pub fn active_buffer_is_references(&self) -> bool {
        self.active_buffer()
            .is_some_and(|buffer| matches!(buffer.content, BufferContent::References(_)))
    }

    pub fn active_buffer_is_diagnostics(&self) -> bool {
        self.active_buffer()
            .is_some_and(|buffer| matches!(buffer.content, BufferContent::Diagnostics(_)))
    }

    pub fn active_references_buffer(&self) -> Option<&ReferencesBufferState> {
        match self.active_buffer().map(|buffer| &buffer.content) {
            Some(BufferContent::References(state)) => Some(state),
            _ => None,
        }
    }

    pub fn active_diagnostics_buffer(&self) -> Option<&DiagnosticsState> {
        match self.active_buffer().map(|buffer| &buffer.content) {
            Some(BufferContent::Diagnostics(state)) => Some(state),
            _ => None,
        }
    }

    pub fn active_references_origin(&self) -> Option<(PathBuf, usize)> {
        let state = self.active_references_buffer()?;
        Some((state.origin_path.clone()?, state.origin_line))
    }

    pub fn selected_reference_item(&self) -> Option<&ReferencesBufferItem> {
        let state = self.active_references_buffer()?;
        state.items.get(state.selected_index)
    }

    pub fn selected_reference_item_cloned(&self) -> Option<ReferencesBufferItem> {
        self.selected_reference_item().cloned()
    }

    pub fn selected_diagnostic_item(&self) -> Option<&DiagnosticItem> {
        let state = self.active_diagnostics_buffer()?;
        state.results.get(state.selected_index)
    }

    pub fn selected_diagnostic_item_cloned(&self) -> Option<DiagnosticItem> {
        self.selected_diagnostic_item().cloned()
    }

    pub fn active_terminal_session_id(&self) -> Option<u64> {
        match self.active_buffer().map(|buffer| &buffer.content) {
            Some(BufferContent::Terminal(state)) => state.session_id,
            _ => None,
        }
    }

    pub fn terminal_buffer_index_for_session(&self, session_id: u64) -> Option<usize> {
        self.buffers
            .iter()
            .position(|buffer| match &buffer.content {
                BufferContent::Terminal(state) => state.session_id == Some(session_id),
                BufferContent::Text(_)
                | BufferContent::References(_)
                | BufferContent::Diagnostics(_)
                | BufferContent::FuzzyPicker(_)
                | BufferContent::SettingsTab(_) => false,
            })
    }

    pub fn references_select_next(&mut self) -> bool {
        let Some(BufferContent::References(state)) = self
            .active_buffer_index
            .and_then(|idx| self.buffers.get_mut(idx))
            .map(|buffer| &mut buffer.content)
        else {
            return false;
        };

        if state.items.is_empty() {
            return false;
        }
        let next = (state.selected_index + 1) % state.items.len();
        if next == state.selected_index {
            return false;
        }
        state.selected_index = next;
        state.preview_lines.clear();
        state.preview_text.clear();
        state.preview_spans.clear();
        self.bump_revision();
        true
    }

    pub fn references_select_prev(&mut self) -> bool {
        let Some(BufferContent::References(state)) = self
            .active_buffer_index
            .and_then(|idx| self.buffers.get_mut(idx))
            .map(|buffer| &mut buffer.content)
        else {
            return false;
        };

        if state.items.is_empty() {
            return false;
        }
        let next = if state.selected_index == 0 {
            state.items.len().saturating_sub(1)
        } else {
            state.selected_index - 1
        };
        if next == state.selected_index {
            return false;
        }
        state.selected_index = next;
        state.preview_lines.clear();
        state.preview_text.clear();
        state.preview_spans.clear();
        self.bump_revision();
        true
    }

    pub fn diagnostics_select_next(&mut self) -> bool {
        let Some(BufferContent::Diagnostics(state)) = self
            .active_buffer_index
            .and_then(|idx| self.buffers.get_mut(idx))
            .map(|buffer| &mut buffer.content)
        else {
            return false;
        };

        if state.results.is_empty() {
            return false;
        }
        let next = (state.selected_index + 1) % state.results.len();
        if next == state.selected_index {
            return false;
        }
        state.selected_index = next;
        state.preview_lines.clear();
        state.preview_text.clear();
        state.preview_spans.clear();
        self.bump_revision();
        true
    }

    pub fn diagnostics_select_prev(&mut self) -> bool {
        let Some(BufferContent::Diagnostics(state)) = self
            .active_buffer_index
            .and_then(|idx| self.buffers.get_mut(idx))
            .map(|buffer| &mut buffer.content)
        else {
            return false;
        };

        if state.results.is_empty() {
            return false;
        }
        let next = if state.selected_index == 0 {
            state.results.len().saturating_sub(1)
        } else {
            state.selected_index - 1
        };
        if next == state.selected_index {
            return false;
        }
        state.selected_index = next;
        state.preview_lines.clear();
        state.preview_text.clear();
        state.preview_spans.clear();
        self.bump_revision();
        true
    }

    pub fn current_overlays(&self) -> &[EditorOverlay] {
        &self.current_overlays
    }

    pub fn completion(&self) -> Option<&CompletionState> {
        self.completion.as_ref()
    }

    pub fn has_completion(&self) -> bool {
        self.completion.is_some()
    }

    pub fn set_completion(&mut self, completion: CompletionState) -> bool {
        if self.completion.as_ref() == Some(&completion) {
            return false;
        }
        self.completion = Some(completion);
        self.bump_revision();
        true
    }

    pub fn clear_completion(&mut self) -> bool {
        if self.completion.is_none() {
            return false;
        }
        self.completion = None;
        self.bump_revision();
        true
    }

    pub fn completion_select_next(&mut self) -> bool {
        let Some(state) = self.completion.as_mut() else {
            return false;
        };
        if state.filtered_items.is_empty() {
            return false;
        }
        let next = (state.selected_index + 1) % state.filtered_items.len();
        if next == state.selected_index {
            return false;
        }
        state.selected_index = next;
        self.bump_revision();
        true
    }

    pub fn completion_select_prev(&mut self) -> bool {
        let Some(state) = self.completion.as_mut() else {
            return false;
        };
        if state.filtered_items.is_empty() {
            return false;
        }
        let len = state.filtered_items.len();
        let next = (state.selected_index + len - 1) % len;
        if next == state.selected_index {
            return false;
        }
        state.selected_index = next;
        self.bump_revision();
        true
    }

    pub fn selected_completion_item(&self) -> Option<&LspCompletionItem> {
        let state = self.completion.as_ref()?;
        state
            .filtered_items
            .get(state.selected_index)
            .map(|entry| &entry.item)
    }

    pub fn refresh_completion_with_prefix(&mut self, prefix: &str) -> bool {
        let Some(state) = self.completion.as_mut() else {
            return false;
        };

        let mut filtered_items = build_completion_display_items(&state.raw_items, prefix);

        let next_selected = if filtered_items.is_empty() { 0 } else { 0 };
        let changed = state.filtered_items != filtered_items
            || state.selected_index != next_selected
            || state.typed_prefix != prefix;
        if !changed {
            return false;
        }

        state.filtered_items.clear();
        state.filtered_items.append(&mut filtered_items);
        state.selected_index = next_selected;
        state.typed_prefix = prefix.to_string();
        self.bump_revision();
        true
    }

    pub fn set_current_overlays(&mut self, overlays: Vec<EditorOverlay>) -> bool {
        if self.current_overlays == overlays {
            return false;
        }
        self.current_overlays = overlays;
        true
    }

    pub fn clear_current_overlays(&mut self) -> bool {
        if self.current_overlays.is_empty() {
            return false;
        }
        self.current_overlays.clear();
        true
    }

    pub fn active_filetype_label(&self) -> &'static str {
        if self.active_buffer_is_terminal() {
            return "Terminal";
        }
        if self.active_buffer_is_diagnostics() {
            return "Diagnostics";
        }
        if self.active_buffer_is_references() {
            return "References";
        }
        self.active_file
            .as_deref()
            .map(filetype_label_for_path)
            .unwrap_or("Plain Text")
    }

    pub fn default_save_path(&self) -> &Path {
        &self.default_save_path
    }

    pub fn len_chars(&self) -> usize {
        self.text.len_chars()
    }

    pub fn len_lines(&self) -> usize {
        self.text.len_lines()
    }

    pub fn len_bytes(&self) -> usize {
        self.text.len_bytes()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn current_mode(&self) -> EditorMode {
        self.mode_state.current()
    }

    pub fn can_apply_mode_event(&self, event: ModeEvent) -> bool {
        self.mode_state.can_apply(event)
    }

    pub fn apply_mode_event(
        &mut self,
        event: ModeEvent,
    ) -> Result<ModeTransitionResult, ModeTransitionError> {
        let result = self.mode_state.apply(event)?;
        if result.from == EditorMode::Insert && result.to != EditorMode::Insert {
            let _ = self.commit_transaction();
        }
        Ok(result)
    }

    pub fn preview(&self, max_chars: usize) -> String {
        let mut preview = String::new();
        for ch in self.text.chars().take(max_chars) {
            for escaped in ch.escape_default() {
                preview.push(escaped);
            }
        }

        if preview.is_empty() {
            return "<empty>".to_string();
        }

        if self.text.len_chars() > max_chars {
            preview.push_str("...");
        }
        preview
    }

    pub fn debug_state_line(&self) -> String {
        let (line, col) = self.cursor_line_col();
        let file_text = self
            .active_file()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<none>".to_string());
        let palette_query = if self.is_command_palette_visible() {
            self.command_palette_query_text()
        } else {
            ""
        };
        let palette_mode = self
            .command_palette_mode()
            .map(|mode| format!("{mode:?}"))
            .unwrap_or_else(|| "None".to_string());
        let selection = self
            .visual_selection_range()
            .map(|range| format!("{}..{}", range.start_char, range.end_char))
            .unwrap_or_else(|| "-".to_string());

        format!(
            "mode={} cursor=({},{}) chars={} lines={} bytes={} dirty={} rev={} palette_visible={} palette_mode={} terminal_open={} open_buffers={} active_buffer_index={:?} visual_selection={} palette_query={:?} palette_results={} conflict={:?} notice={:?} file={} preview=\"{}\"",
            self.current_mode().as_str(),
            line,
            col,
            self.len_chars(),
            self.len_lines(),
            self.len_bytes(),
            self.is_dirty(),
            self.revision(),
            self.is_command_palette_visible(),
            palette_mode,
            self.is_terminal_panel_open(),
            self.buffers.len(),
            self.active_buffer_index,
            selection,
            palette_query,
            self.command_palette.results.len(),
            self.external_conflict_message(),
            self.last_external_notice(),
            file_text,
            self.preview(48)
        )
    }

    fn cursor_state(&self) -> CursorState {
        CursorState {
            char_idx: self.cursor_char_idx,
            target_col: self.target_col,
        }
    }

    fn restore_cursor_state(&mut self, state: CursorState) {
        self.cursor_char_idx = state.char_idx.min(self.text.len_chars());
        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        self.target_col = state.target_col.min(self.max_col_for_line(line_idx));
    }

    fn ensure_current_transaction(&mut self) -> &mut Transaction {
        if self.current_transaction.is_none() {
            self.current_transaction = Some(Transaction::new(self.cursor_state()));
        }
        self.current_transaction
            .as_mut()
            .expect("current transaction initialized")
    }

    fn apply_insert(&mut self, index: usize, text: String) -> bool {
        if text.is_empty() {
            return false;
        }

        let insert_at = index.min(self.text.len_chars());
        self.record_insert_highlight_edit(insert_at, &text);
        self.apply_insert_raw(insert_at, &text);
        self.ensure_current_transaction()
            .actions
            .push(EditAction::Insert {
                index: insert_at,
                text,
            });
        true
    }

    fn apply_delete(&mut self, index: usize, len_chars: usize) -> bool {
        if len_chars == 0 || index >= self.text.len_chars() {
            return false;
        }

        let end = (index + len_chars).min(self.text.len_chars());
        self.record_delete_highlight_edit(index, end - index);
        let Some(text) = self.apply_delete_raw(index, end - index) else {
            return false;
        };

        self.ensure_current_transaction()
            .actions
            .push(EditAction::Delete { index, text });
        true
    }

    fn apply_insert_raw(&mut self, index: usize, text: &str) {
        if text.is_empty() {
            return;
        }

        let insert_at = index.min(self.text.len_chars());
        self.text.insert(insert_at, text);
        let _ = self.refresh_active_search_highlights();
    }

    fn apply_delete_raw(&mut self, index: usize, len_chars: usize) -> Option<String> {
        if len_chars == 0 || index >= self.text.len_chars() {
            return None;
        }

        let end = (index + len_chars).min(self.text.len_chars());
        if end <= index {
            return None;
        }

        let deleted = self.text.slice(index..end).to_string();
        self.text.remove(index..end);
        let _ = self.refresh_active_search_highlights();
        Some(deleted)
    }

    fn char_range_text(&self, start: usize, end: usize) -> Option<String> {
        if start >= end || start >= self.text.len_chars() {
            return None;
        }

        let end = end.min(self.text.len_chars());
        if end <= start {
            return None;
        }

        Some(self.text.slice(start..end).to_string())
    }

    fn current_line_content_range(&self) -> Option<(usize, usize)> {
        if self.text.len_chars() == 0 {
            return None;
        }

        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        let line_start = self.text.line_to_char(line_idx);
        let line_end = self.line_content_end_char_idx(line_idx);
        (line_start < line_end).then_some((line_start, line_end))
    }

    fn linewise_text_for_range(&self, start: usize, end: usize) -> Option<String> {
        let mut text = self.char_range_text(start, end)?;
        if !text.ends_with('\n') {
            text.push('\n');
        }
        Some(text)
    }

    fn delete_char_range_at_cursor(&self) -> Option<(usize, usize)> {
        if self.text.len_chars() == 0 {
            return None;
        }

        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        let line_start = self.text.line_to_char(line_idx);
        let line_end = self.line_content_end_char_idx(line_idx);
        if line_start == line_end {
            return None;
        }

        let mut delete_idx = if self.cursor_char_idx < line_end {
            self.cursor_char_idx
        } else {
            line_end.saturating_sub(1)
        };
        if delete_idx < line_start {
            delete_idx = line_start;
        }
        if delete_idx >= self.text.len_chars() {
            return None;
        }

        Some((delete_idx, delete_idx + 1))
    }

    fn current_line_delete_range(&self) -> Option<(usize, usize)> {
        if self.text.len_lines() == 0 {
            return None;
        }

        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        let line_start = self.text.line_to_char(line_idx);
        let mut line_end = if line_idx + 1 < self.text.len_lines() {
            self.text.line_to_char(line_idx + 1)
        } else {
            self.text.len_chars()
        };
        let mut delete_start = line_start;

        if delete_start == line_end && line_idx > 0 {
            delete_start = delete_start.saturating_sub(1);
            line_end = line_end.max(delete_start);
        }

        (delete_start < line_end).then_some((delete_start, line_end))
    }

    fn delete_word_forward_range(&self) -> Option<(usize, usize)> {
        let n = self.text.len_chars();
        if self.cursor_char_idx >= n {
            return None;
        }

        let end = next_word_start(&self.text, self.cursor_char_idx);
        (end > self.cursor_char_idx).then_some((self.cursor_char_idx, end))
    }

    fn delete_word_backward_range(&self) -> Option<(usize, usize)> {
        if self.cursor_char_idx == 0 {
            return None;
        }

        let start = previous_word_start(&self.text, self.cursor_char_idx);
        (start < self.cursor_char_idx).then_some((start, self.cursor_char_idx))
    }

    fn yank_word_end_range(&self) -> Option<(usize, usize)> {
        let n = self.text.len_chars();
        if self.cursor_char_idx >= n {
            return None;
        }

        let end = word_end_from_cursor(&self.text, self.cursor_char_idx)?;
        (end >= self.cursor_char_idx).then_some((self.cursor_char_idx, end + 1))
    }

    fn paste_linewise(&mut self, text: &str, before: bool) -> bool {
        let mut insert_text = text.to_string();
        if insert_text.is_empty() {
            return false;
        }

        let total_chars = self.text.len_chars();
        let line_idx = if total_chars == 0 {
            0
        } else {
            self.text
                .char_to_line(self.cursor_char_idx.min(total_chars))
        };
        let line_start = self.text.line_to_char(line_idx);
        let has_following_line = line_idx + 1 < self.text.len_lines();
        let insert_at = if before {
            line_start
        } else if has_following_line {
            self.text.line_to_char(line_idx + 1)
        } else {
            total_chars
        };

        let buffer_has_trailing_newline =
            total_chars > 0 && self.text.char(total_chars.saturating_sub(1)) == '\n';
        let inserted_line_start = if before {
            insert_at
        } else if total_chars == 0 || has_following_line || buffer_has_trailing_newline {
            insert_at
        } else {
            insert_text = format!("\n{insert_text}");
            insert_at + 1
        };

        if !self.apply_insert(insert_at, insert_text) {
            return false;
        }

        self.cursor_char_idx = inserted_line_start.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    fn clear_history(&mut self) {
        self.history.clear();
        self.current_transaction = None;
        self.pending_highlight_edits.clear();
    }

    fn record_insert_highlight_edit(&mut self, index: usize, text: &str) {
        if text.is_empty() {
            return;
        }

        let insert_at = index.min(self.text.len_chars());
        let start_byte = self.text.char_to_byte(insert_at);
        self.pending_highlight_edits
            .push(HighlightEdit::insert(start_byte, text.len()));
    }

    fn record_delete_highlight_edit(&mut self, index: usize, len_chars: usize) {
        if len_chars == 0 || index >= self.text.len_chars() {
            return;
        }

        let start_char = index.min(self.text.len_chars());
        let end_char = (start_char + len_chars).min(self.text.len_chars());
        if start_char >= end_char {
            return;
        }

        let start_byte = self.text.char_to_byte(start_char);
        let end_byte = self.text.char_to_byte(end_char);
        self.pending_highlight_edits
            .push(HighlightEdit::delete(start_byte, end_byte));
    }

    fn sync_file_picker_cache(&mut self) {
        if !self.is_file_picker_open() {
            self.file_picker_results_cache.clear();
            return;
        }

        self.file_picker_results_cache = self
            .command_palette
            .results
            .iter()
            .filter_map(|item| match &item.action {
                CommandPaletteAction::OpenFile(path) => Some(FilePickerEntry {
                    absolute_path: path.clone(),
                    relative_path: item.label.clone(),
                    score: 0,
                }),
                _ => None,
            })
            .collect();
    }

    fn set_search_query_internal(&mut self, query: &str, whole_word: bool) -> bool {
        let query_changed = self.last_search_query != query;
        let whole_word_changed = self.search_whole_word != whole_word;
        self.last_search_query = query.to_string();
        self.search_whole_word = whole_word;
        let highlights_changed = self.refresh_active_search_highlights();
        query_changed || whole_word_changed || highlights_changed
    }

    fn refresh_active_search_highlights(&mut self) -> bool {
        let next = if self.last_search_query.is_empty() {
            Vec::new()
        } else {
            let text = self.text.to_string();
            collect_search_highlights(&text, &self.last_search_query, self.search_whole_word)
        };

        if self.search_highlights == next {
            return false;
        }

        self.search_highlights = next;
        true
    }

    fn jump_to_search_match(&mut self, forward: bool) -> bool {
        if self.search_highlights.is_empty() {
            return false;
        }

        let cursor_byte = self.cursor_byte_idx();
        let target = if forward {
            self.search_highlights
                .iter()
                .copied()
                .find(|(start, _)| *start > cursor_byte)
                .or_else(|| self.search_highlights.first().copied())
        } else {
            self.search_highlights
                .iter()
                .copied()
                .rev()
                .find(|(_, end)| *end <= cursor_byte)
                .or_else(|| self.search_highlights.last().copied())
        };

        let Some((start_byte, _)) = target else {
            return false;
        };
        self.move_cursor_to_char_idx(self.byte_to_char_idx(start_byte))
    }

    fn move_cursor_to_char_idx(&mut self, char_idx: usize) -> bool {
        let changed = self.update_cursor_position(char_idx);
        let (_, col) = self.cursor_line_col();
        let target_changed = self.target_col != col;
        self.target_col = col;
        changed || target_changed
    }

    fn char_at_cursor(&self) -> Option<char> {
        (self.cursor_char_idx < self.text.len_chars()).then(|| self.text.char(self.cursor_char_idx))
    }

    pub fn char_before_cursor(&self) -> Option<char> {
        (self.cursor_char_idx > 0).then(|| self.text.char(self.cursor_char_idx - 1))
    }

    fn line_indent_string(&self, line_idx: usize) -> String {
        if self.text.len_lines() == 0 {
            return String::new();
        }

        let clamped_line = line_idx.min(self.text.len_lines().saturating_sub(1));
        let line_text = self.text.line(clamped_line).to_string();
        let line_content = line_text.strip_suffix('\n').unwrap_or(&line_text);
        line_content
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .collect()
    }

    fn indent_unit_for_line(&self, current_indent: &str) -> String {
        if current_indent.contains('\t') {
            "\t".to_string()
        } else {
            "    ".to_string()
        }
    }

    fn word_under_cursor(&self) -> Option<String> {
        let len_chars = self.text.len_chars();
        if len_chars == 0 {
            return None;
        }

        let focus = self.cursor_char_idx.min(len_chars.saturating_sub(1));
        if classify_char(self.text.char(focus)) != WordClass::Word {
            return None;
        }

        let mut start = focus;
        while start > 0 && classify_char(self.text.char(start - 1)) == WordClass::Word {
            start -= 1;
        }

        let mut end = focus + 1;
        while end < len_chars && classify_char(self.text.char(end)) == WordClass::Word {
            end += 1;
        }

        self.char_range_text(start, end)
    }

    fn active_comment_syntax(&self) -> Option<CommentSyntax> {
        self.active_file
            .as_deref()
            .or(Some(self.default_save_path.as_path()))
            .and_then(active_comment_syntax_for_path)
    }

    fn toggle_comments_on_lines(&mut self, start_line: usize, end_line: usize) -> bool {
        let Some(syntax) = self.active_comment_syntax() else {
            return false;
        };
        if self.text.len_lines() == 0 {
            return false;
        }

        let last_line = self.text.len_lines().saturating_sub(1);
        let start_line = start_line.min(last_line);
        let end_line = end_line.min(last_line);
        let plans: Vec<LineCommentPlan> = (start_line..=end_line)
            .map(|line_idx| line_comment_plan(&self.text, line_idx, syntax.line_prefix))
            .collect();
        let should_uncomment =
            !plans.is_empty() && plans.iter().all(|plan| plan.removal_len_chars.is_some());

        let edits: Vec<CommentEdit> = if should_uncomment {
            plans
                .into_iter()
                .filter_map(|plan| {
                    plan.removal_len_chars.map(|len_chars| CommentEdit::Delete {
                        at: plan.edit_char_idx,
                        len_chars,
                    })
                })
                .collect()
        } else {
            let insert_text = format!("{} ", syntax.line_prefix);
            plans
                .into_iter()
                .map(|plan| CommentEdit::Insert {
                    at: plan.edit_char_idx,
                    text: insert_text.clone(),
                })
                .collect()
        };

        if edits.is_empty() {
            return false;
        }

        let mut cursor = self.cursor_char_idx.min(self.text.len_chars());
        let mut offset: isize = 0;
        let mut changed = false;

        for edit in edits {
            match edit {
                CommentEdit::Insert { at, text } => {
                    let current_at = shift_char_position(at, offset).min(self.text.len_chars());
                    let inserted_chars = text.chars().count();
                    if self.apply_insert(current_at, text) {
                        cursor = adjust_cursor_after_insert(cursor, current_at, inserted_chars);
                        offset += inserted_chars as isize;
                        changed = true;
                    }
                }
                CommentEdit::Delete { at, len_chars } => {
                    let current_at = shift_char_position(at, offset).min(self.text.len_chars());
                    if self.apply_delete(current_at, len_chars) {
                        cursor = adjust_cursor_after_delete(cursor, current_at, len_chars);
                        offset -= len_chars as isize;
                        changed = true;
                    }
                }
            }
        }

        if !changed {
            return false;
        }

        self.dirty = true;
        let moved = self.move_cursor_to_char_idx(cursor.min(self.text.len_chars()));
        if !moved {
            self.bump_revision();
        }
        true
    }

    fn max_col_for_line(&self, line_idx: usize) -> usize {
        let line = self.text.line(line_idx);
        let len_chars = line.len_chars();
        if len_chars == 0 {
            return 0;
        }

        // Rope line thường chứa '\n' ở cuối (trừ dòng cuối của file).
        // Cursor nên dừng ở "cuối nội dung dòng", không đứng sau '\n'.
        if line.char(len_chars - 1) == '\n' {
            len_chars - 1
        } else {
            // Dòng không có '\n' => cho phép caret đi tới vị trí sau ký tự cuối.
            len_chars
        }
    }

    fn line_content_end_char_idx(&self, line_idx: usize) -> usize {
        let clamped_line = line_idx.min(self.text.len_lines().saturating_sub(1));
        let line_start = self.text.line_to_char(clamped_line);
        line_start + self.max_col_for_line(clamped_line)
    }

    /// Unified cursor update path used by all motion commands.
    ///
    /// - In `Visual` mode: keep `selection_anchor_char_idx` untouched and move
    ///   cursor/focus only.
    /// - In non-visual modes: clear stale selection anchor while moving.
    fn update_cursor_position(&mut self, new_index: usize) -> bool {
        let clamped = new_index.min(self.text.len_chars());
        let mut changed = false;

        if clamped != self.cursor_char_idx {
            self.cursor_char_idx = clamped;
            changed = true;
        }

        if self.current_mode() != EditorMode::Visual
            && self.selection_anchor_char_idx.take().is_some()
        {
            changed = true;
        }

        if changed {
            self.bump_revision();
        }
        changed
    }

    fn bump_revision(&mut self) {
        self.revision += 1;
    }

    fn should_ignore_self_save_event(&self) -> bool {
        self.last_saved_at.is_some_and(|saved_at| {
            Instant::now().saturating_duration_since(saved_at) < Self::SELF_SAVE_IGNORE_WINDOW
        })
    }

    fn load_buffer_from_file(&mut self, canonical_path: &Path) -> Result<(), String> {
        let content = fs::read_to_string(canonical_path)
            .map_err(|err| format!("open file {:?} failed: {err}", canonical_path))?;
        self.replace_text_buffer_preserving_view(content.as_str());
        let _ = self.refresh_active_search_highlights();
        Ok(())
    }

    fn load_buffer_from_file_resetting_view(
        &mut self,
        canonical_path: &Path,
    ) -> Result<(), String> {
        let content = fs::read_to_string(canonical_path)
            .map_err(|err| format!("open file {:?} failed: {err}", canonical_path))?;
        self.text = Rope::from(content.as_str());
        self.cursor_char_idx = 0;
        self.target_col = 0;
        self.scroll_line = 0;
        self.scroll_column = 0;
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        self.clear_history();
        let _ = self.refresh_active_search_highlights();
        Ok(())
    }

    fn replace_text_buffer_preserving_view(&mut self, content: &str) {
        let old_cursor = self.cursor_char_idx;
        let old_selection_anchor = self.selection_anchor_char_idx;
        let old_scroll_line = self.scroll_line;
        let old_scroll_column = self.scroll_column;
        let old_visual_line_mode = self.visual_line_mode;

        self.text = Rope::from(content);

        let max_char_idx = self.text.len_chars();
        self.cursor_char_idx = old_cursor.min(max_char_idx);
        self.selection_anchor_char_idx =
            old_selection_anchor.map(|anchor| anchor.min(max_char_idx));

        if self.selection_anchor_char_idx == Some(self.cursor_char_idx) {
            self.selection_anchor_char_idx = None;
        }

        let (_, clamped_col) = self.cursor_line_col();
        self.target_col = clamped_col;
        self.scroll_line = old_scroll_line.min(self.text.len_lines().saturating_sub(1));
        self.scroll_column = old_scroll_column;
        self.visual_line_mode = old_visual_line_mode && self.selection_anchor_char_idx.is_some();
        self.clear_history();
    }

    fn register_open_text_buffer(&mut self, active_path: PathBuf) {
        let language_id = crate::lsp::registry::language_profile_for_path(&active_path)
            .map(|profile| profile.language_id.to_string());
        if let Some(existing_idx) = self
            .buffers
            .iter()
            .position(|buffer| matches!(&buffer.content, BufferContent::Text(buffer) if buffer.path == active_path))
        {
            self.active_buffer_index = Some(existing_idx);
            return;
        }

        self.buffers.push(BufferEntry {
            content: BufferContent::Text(EditorBuffer {
                path: active_path,
                language_id,
            }),
        });
        self.active_buffer_index = Some(self.buffers.len().saturating_sub(1));
    }

    fn cycle_buffer(&mut self, forward: bool) -> Result<bool, String> {
        if self.buffers.is_empty() {
            return Ok(false);
        }

        let current_idx = self
            .active_buffer_index
            .filter(|idx| *idx < self.buffers.len());

        let next_idx = match current_idx {
            Some(idx) if forward => (idx + 1) % self.buffers.len(),
            Some(idx) => {
                if idx == 0 {
                    self.buffers.len() - 1
                } else {
                    idx - 1
                }
            }
            None if forward => 0,
            None => self.buffers.len() - 1,
        };

        if current_idx == Some(next_idx) {
            return Ok(false);
        }

        let mut candidate_idx = next_idx;
        let mut attempts = self.buffers.len();
        while attempts > 0 && !self.buffers.is_empty() {
            attempts -= 1;
            match self.activate_buffer_index(candidate_idx) {
                Ok(()) => return Ok(true),
                Err(_) => {
                    self.buffers.remove(candidate_idx);
                    if self.buffers.is_empty() {
                        return Ok(self.new_empty_buffer());
                    }
                    if candidate_idx >= self.buffers.len() {
                        candidate_idx = 0;
                    }
                }
            }
        }

        Ok(false)
    }

    fn activate_buffer_index(&mut self, index: usize) -> Result<(), String> {
        let Some(buffer) = self.buffers.get(index).cloned() else {
            return Err(format!("buffer index {index} out of range"));
        };

        match buffer.content {
            BufferContent::Text(buffer) => {
                self.load_buffer_from_file_resetting_view(&buffer.path)?;
                self.active_file = Some(buffer.path.clone());
                self.active_buffer_index = Some(index);
                self.selection_anchor_char_idx = None;
                self.dirty = false;
                self.external_conflict = None;
                self.visual_line_mode = false;
                let _ = self.workspace_expand_to_path(&buffer.path);
            }
            BufferContent::Terminal(_) => {
                self.active_file = None;
                self.active_buffer_index = Some(index);
                self.selection_anchor_char_idx = None;
                self.visual_line_mode = false;
                self.external_conflict = None;
            }
            BufferContent::References(_)
            | BufferContent::Diagnostics(_)
            | BufferContent::FuzzyPicker(_)
            | BufferContent::SettingsTab(_) => {
                self.reset_text_editor_state();
                self.active_buffer_index = Some(index);
                let _ = self.clear_current_overlays();
            }
        }

        self.bump_revision();
        Ok(())
    }

    fn reset_text_editor_state(&mut self) {
        self.text = Rope::new();
        self.cursor_char_idx = 0;
        self.target_col = 0;
        self.scroll_line = 0;
        self.scroll_column = 0;
        self.active_file = None;
        self.selection_anchor_char_idx = None;
        self.dirty = false;
        self.external_conflict = None;
        self.visual_line_mode = false;
        self.clear_history();
        let _ = self.refresh_active_search_highlights();
    }
}

#[derive(Debug, Clone, Copy)]
struct CommentSyntax {
    line_prefix: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct LineCommentPlan {
    edit_char_idx: usize,
    removal_len_chars: Option<usize>,
}

#[derive(Debug, Clone)]
enum CommentEdit {
    Insert { at: usize, text: String },
    Delete { at: usize, len_chars: usize },
}

fn active_comment_syntax_for_path(path: &Path) -> Option<CommentSyntax> {
    if let Some(file_name) = path.file_name().and_then(|name| name.to_str())
        && file_name.eq_ignore_ascii_case("makefile")
    {
        return Some(CommentSyntax { line_prefix: "#" });
    }

    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase());
    let line_prefix = match extension.as_deref() {
        Some(
            "rs" | "go" | "js" | "jsx" | "ts" | "tsx" | "c" | "cc" | "cpp" | "h" | "hpp" | "java"
            | "kt" | "kts" | "swift" | "cs" | "dart" | "scala" | "scss" | "proto" | "php",
        ) => "//",
        Some(
            "py" | "sh" | "bash" | "zsh" | "fish" | "rb" | "yml" | "yaml" | "toml" | "ini" | "cfg"
            | "conf" | "properties",
        ) => "#",
        Some("sql" | "lua") => "--",
        _ => "//",
    };

    Some(CommentSyntax { line_prefix })
}

fn line_comment_plan(text: &Rope, line_idx: usize, line_prefix: &str) -> LineCommentPlan {
    let clamped_line = line_idx.min(text.len_lines().saturating_sub(1));
    let line_start = text.line_to_char(clamped_line);
    let line_text = text.line(clamped_line).to_string();
    let line_content = line_text.strip_suffix('\n').unwrap_or(&line_text);
    let indent_byte_idx = line_content
        .char_indices()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(idx, _)| idx)
        .unwrap_or(line_content.len());
    let indent_chars = line_content[..indent_byte_idx].chars().count();
    let rest = &line_content[indent_byte_idx..];

    LineCommentPlan {
        edit_char_idx: line_start + indent_chars,
        removal_len_chars: line_comment_removal_len(rest, line_prefix),
    }
}

fn line_comment_removal_len(rest: &str, line_prefix: &str) -> Option<usize> {
    if !rest.starts_with(line_prefix) {
        return None;
    }

    let after_prefix = &rest[line_prefix.len()..];
    if line_prefix == "//" && (after_prefix.starts_with('/') || after_prefix.starts_with('!')) {
        return None;
    }
    if line_prefix == "#" && after_prefix.starts_with('!') {
        return None;
    }

    let mut len_chars = line_prefix.chars().count();
    if after_prefix
        .chars()
        .next()
        .is_some_and(|ch| ch.is_whitespace())
    {
        len_chars += 1;
    }
    Some(len_chars)
}

fn matching_close_char(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        '\'' => Some('\''),
        _ => None,
    }
}

fn matches_matching_bracket_pair(left: Option<char>, right: Option<char>) -> bool {
    matches!(
        (left, right),
        (Some('('), Some(')'))
            | (Some('['), Some(']'))
            | (Some('{'), Some('}'))
            | (Some('"'), Some('"'))
            | (Some('\''), Some('\''))
    )
}

fn shift_char_position(position: usize, delta: isize) -> usize {
    if delta.is_negative() {
        position.saturating_sub(delta.unsigned_abs())
    } else {
        position.saturating_add(delta as usize)
    }
}

fn adjust_cursor_after_insert(cursor: usize, insert_at: usize, len_chars: usize) -> usize {
    if insert_at <= cursor {
        cursor.saturating_add(len_chars)
    } else {
        cursor
    }
}

fn adjust_cursor_after_delete(cursor: usize, delete_at: usize, len_chars: usize) -> usize {
    if delete_at >= cursor {
        return cursor;
    }

    cursor.saturating_sub(len_chars.min(cursor.saturating_sub(delete_at)))
}

/// Vim word-class for `dw` boundary detection.
///   Word  = alphanumeric + `_`
///   Punct = non-whitespace, non-word
///   Space = space/tab (newline handled separately)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WordClass {
    Word,
    Punct,
    Space,
    Newline,
}

fn classify_char(ch: char) -> WordClass {
    if ch == '\n' {
        WordClass::Newline
    } else if ch.is_whitespace() {
        WordClass::Space
    } else if ch.is_alphanumeric() || ch == '_' {
        WordClass::Word
    } else {
        WordClass::Punct
    }
}

fn is_completion_identifier_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '$')
}

/// Returns the char index where vim's `dw` motion should stop, counting from
/// `cursor`. Crosses same-line whitespace after the current token so `dw` eats
/// one "word-like run" plus trailing spaces. Stops at newline.
fn next_word_start(text: &Rope, cursor: usize) -> usize {
    let n = text.len_chars();
    if cursor >= n {
        return cursor;
    }

    let start_class = classify_char(text.char(cursor));

    // Cursor sitting on a newline: delete just that one newline (line join).
    if start_class == WordClass::Newline {
        return cursor + 1;
    }

    let mut i = cursor;

    // If cursor starts on whitespace, skip same-line whitespace only.
    if start_class == WordClass::Space {
        while i < n {
            let cls = classify_char(text.char(i));
            if cls != WordClass::Space {
                break;
            }
            i += 1;
        }
        return i;
    }

    // On Word or Punct: skip the current run of the same class, then skip
    // same-line trailing whitespace to land at start of next token.
    while i < n && classify_char(text.char(i)) == start_class {
        i += 1;
    }
    while i < n && classify_char(text.char(i)) == WordClass::Space {
        i += 1;
    }
    i
}

fn previous_word_start(text: &Rope, cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }

    let mut i = cursor.saturating_sub(1);
    while i > 0 {
        let cls = classify_char(text.char(i));
        if cls != WordClass::Space && cls != WordClass::Newline {
            break;
        }
        i -= 1;
    }

    if classify_char(text.char(i)) == WordClass::Space
        || classify_char(text.char(i)) == WordClass::Newline
    {
        return i;
    }

    let cls = classify_char(text.char(i));
    while i > 0 && classify_char(text.char(i - 1)) == cls {
        i -= 1;
    }
    i
}

fn word_end_at_or_after(text: &Rope, cursor: usize) -> Option<usize> {
    let n = text.len_chars();
    if n == 0 || cursor >= n {
        return None;
    }

    let mut i = cursor;

    // If already at a word-end (non-space char whose next char is a different class),
    // step forward one so we land on the NEXT word (Vim `e` behavior).
    if i + 1 < n {
        let cur_cls = classify_char(text.char(i));
        let next_cls = classify_char(text.char(i + 1));
        if cur_cls != WordClass::Space && cur_cls != WordClass::Newline && next_cls != cur_cls {
            i += 1;
        }
    }

    while i < n {
        let cls = classify_char(text.char(i));
        if cls != WordClass::Space && cls != WordClass::Newline {
            break;
        }
        i += 1;
    }
    if i >= n {
        return None;
    }

    let cls = classify_char(text.char(i));
    while i + 1 < n && classify_char(text.char(i + 1)) == cls {
        i += 1;
    }
    Some(i)
}

fn word_end_from_cursor(text: &Rope, cursor: usize) -> Option<usize> {
    let n = text.len_chars();
    if n == 0 || cursor >= n {
        return None;
    }

    let cls = classify_char(text.char(cursor));
    if cls == WordClass::Space || cls == WordClass::Newline {
        return None;
    }

    let mut i = cursor;
    while i + 1 < n && classify_char(text.char(i + 1)) == cls {
        i += 1;
    }
    Some(i)
}

fn collect_search_highlights(text: &str, query: &str, whole_word: bool) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }

    text.match_indices(query)
        .filter_map(|(start, matched)| {
            let end = start + matched.len();
            if whole_word && !is_whole_word_match(text, start, end) {
                return None;
            }
            Some((start, end))
        })
        .collect()
}

fn build_completion_display_items(
    items: &[LspCompletionItem],
    prefix: &str,
) -> Vec<CompletionDisplayItem> {
    if prefix.is_empty() {
        return items
            .iter()
            .cloned()
            .map(|item| CompletionDisplayItem {
                item,
                match_ranges: Vec::new(),
                score: 0,
            })
            .collect();
    }

    let mut scored = items
        .iter()
        .enumerate()
        .filter_map(|(original_idx, item)| {
            score_completion_match(&item.label, prefix)
                .map(|(score, match_ranges)| (original_idx, item.clone(), score, match_ranges))
        })
        .collect::<Vec<_>>();

    scored.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));

    scored
        .into_iter()
        .map(|(_, item, score, match_ranges)| CompletionDisplayItem {
            item,
            match_ranges,
            score,
        })
        .collect()
}

fn score_completion_match(label: &str, query: &str) -> Option<(i64, Vec<(usize, usize)>)> {
    score_label_match(label, query)
}

fn is_whole_word_match(text: &str, start: usize, end: usize) -> bool {
    let left_ok = if start == 0 {
        true
    } else {
        text[..start]
            .chars()
            .next_back()
            .is_none_or(|ch| classify_char(ch) != WordClass::Word)
    };
    let right_ok = if end >= text.len() {
        true
    } else {
        text[end..]
            .chars()
            .next()
            .is_none_or(|ch| classify_char(ch) != WordClass::Word)
    };
    left_ok && right_ok
}

fn path_matches(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }

    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };

    use crate::app::command_palette::{CommandPaletteItem, CommandPaletteMode};
    use crate::async_runtime::message::{FilePreviewLine, FileSystemChangeKind, FileSystemEvent};
    use crate::core::commands::{TextObjectKind, TextObjectModifier};
    use crate::core::mode::{EditorMode, ModeEvent};

    use super::{AppState, ReferencesBufferItem};
    use crate::syntax::highlight::HighlightEdit;

    fn unique_temp_path(suffix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock drift")
            .as_nanos();
        std::env::temp_dir().join(format!("netherize_phase4_{suffix}_{nanos}.txt"))
    }

    fn unique_temp_dir(suffix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock drift")
            .as_nanos();
        std::env::temp_dir().join(format!("netherize_phase4_dir_{suffix}_{nanos}"))
    }

    #[test]
    fn insert_move_and_backspace_flow() {
        let mut state = AppState::new(unique_temp_path("scratch"));
        state.insert_char('a');
        state.insert_char('b');
        state.insert_char('c');
        assert_eq!(state.len_chars(), 3);

        state.move_left();
        state.backspace();

        assert_eq!(state.len_chars(), 2);
        assert_eq!(state.preview(16), "ac");
        assert!(state.is_dirty());
    }

    #[test]
    fn text_edits_record_highlight_byte_deltas() {
        let mut state = AppState::from_text(unique_temp_path("scratch"), "");

        state.insert_char('a');
        assert_eq!(
            state.take_highlight_edits(),
            vec![HighlightEdit::insert(0, 1)]
        );

        assert!(state.backspace());
        assert_eq!(
            state.take_highlight_edits(),
            vec![HighlightEdit::delete(0, 1)]
        );
    }

    #[test]
    fn backspace_between_empty_auto_pair_deletes_both_chars() {
        let mut state = AppState::from_text(unique_temp_path("smart_backspace_pair"), "()");
        state.move_right();

        assert!(state.backspace());

        assert_eq!(state.text_string(), "");
        assert_eq!(state.cursor_char_idx(), 0);
        assert_eq!(
            state.take_highlight_edits(),
            vec![HighlightEdit::delete(0, 2)]
        );
    }

    #[test]
    fn backspace_between_empty_quotes_deletes_both_chars() {
        let mut state = AppState::from_text(unique_temp_path("smart_backspace_quotes"), "\"\"");
        state.move_right();

        assert!(state.backspace());

        assert_eq!(state.text_string(), "");
        assert_eq!(state.cursor_char_idx(), 0);
        assert_eq!(
            state.take_highlight_edits(),
            vec![HighlightEdit::delete(0, 2)]
        );
    }

    #[test]
    fn save_then_open_roundtrip() {
        let save_path = unique_temp_path("save");
        let open_path = unique_temp_path("open");

        let mut state = AppState::from_text(save_path.clone(), "hello");
        let saved = state.save_file().expect("save should succeed");
        let canonical_save_path = save_path
            .canonicalize()
            .expect("canonical save path should exist");
        assert_eq!(saved, canonical_save_path);
        assert!(!state.is_dirty());

        std::fs::write(&open_path, "world").expect("write open file");
        state
            .open_file(open_path.clone())
            .expect("open should succeed");

        assert_eq!(state.preview(16), "world");
        assert!(!state.is_dirty());
        let canonical_open_path = open_path
            .canonicalize()
            .expect("canonical open path should exist");
        assert_eq!(
            state.active_file().expect("has file"),
            canonical_open_path.as_path()
        );

        let _ = std::fs::remove_file(save_path);
        let _ = std::fs::remove_file(open_path);
    }

    #[test]
    fn buffer_cycle_and_close_follow_open_document_ring() {
        let mut state = AppState::new(unique_temp_path("buffer_ring"));
        let root = unique_temp_dir("buffer_ring");
        fs::create_dir_all(&root).expect("create buffer ring root");
        let file_a = root.join("a.rs");
        let file_b = root.join("b.rs");
        let file_c = root.join("c.rs");
        fs::write(&file_a, "alpha\n").expect("write a");
        fs::write(&file_b, "beta\n").expect("write b");
        fs::write(&file_c, "gamma\n").expect("write c");

        state.open_file(file_a.clone()).expect("open a");
        state.open_file(file_b.clone()).expect("open b");
        state.open_file(file_c.clone()).expect("open c");
        assert!(state.active_file().expect("active file").ends_with("c.rs"));

        assert!(state.buffer_prev().expect("buffer prev"));
        assert!(state.active_file().expect("active file").ends_with("b.rs"));
        assert!(state.buffer_next().expect("buffer next"));
        assert!(state.active_file().expect("active file").ends_with("c.rs"));
        assert!(state.buffer_next().expect("buffer next wrap"));
        assert!(state.active_file().expect("active file").ends_with("a.rs"));
        assert!(state.buffer_prev().expect("buffer prev wrap"));
        assert!(state.active_file().expect("active file").ends_with("c.rs"));

        assert!(state.close_current_buffer().expect("close current"));
        assert!(state.active_file().expect("active file").ends_with("b.rs"));
        assert!(state.close_current_buffer().expect("close current"));
        assert!(state.active_file().expect("active file").ends_with("a.rs"));
        assert!(state.close_current_buffer().expect("close current"));
        assert!(state.active_file().is_none());
        assert!(state.text_string().is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn terminal_buffer_entries_are_tracked_in_tab_ring() {
        let mut state = AppState::new(unique_temp_path("terminal_buffer"));
        let root = unique_temp_dir("terminal_buffer");
        fs::create_dir_all(&root).expect("create terminal buffer root");
        let file_a = root.join("a.rs");
        fs::write(&file_a, "alpha\n").expect("write a");

        state.open_file(file_a.clone()).expect("open a");
        let terminal_idx = state.open_terminal_buffer("[Lazygit]", Some(root.clone()));

        assert_eq!(state.buffers().len(), 2);
        assert_eq!(state.active_buffer_index(), Some(terminal_idx));
        assert!(state.active_buffer_is_terminal());
        assert_eq!(state.active_filetype_label(), "Terminal");
        assert_eq!(state.buffers()[terminal_idx].label(), "[Lazygit]");

        assert!(state.buffer_prev().expect("switch back to text"));
        assert!(state.active_file().expect("active file").ends_with("a.rs"));
        assert!(!state.active_buffer_is_terminal());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn move_right_reaches_end_of_last_line_without_newline() {
        let mut state = AppState::from_text(unique_temp_path("cursor"), "abc");

        // Column có thể đi tới 3 (sau ký tự 'c').
        state.move_right();
        state.move_right();
        state.move_right();
        assert_eq!(state.cursor_line_col(), (0, 3));

        // Đi tiếp là no-op.
        state.move_right();
        assert_eq!(state.cursor_line_col(), (0, 3));
    }

    #[test]
    fn move_right_crosses_to_next_line_at_end_of_line() {
        let mut state = AppState::from_text(unique_temp_path("cursor"), "abc\ndef");

        state.move_right(); // col 1
        state.move_right(); // col 2
        state.move_right(); // col 3 (end of line 0)
        assert_eq!(state.cursor_line_col(), (0, 3));

        // ArrowRight ở cuối line 0 -> đầu line 1
        state.move_right();
        assert_eq!(state.cursor_line_col(), (1, 0));
    }

    #[test]
    fn delete_word_forward_eats_word_plus_trailing_space() {
        let mut state = AppState::from_text(unique_temp_path("dw"), "foo   bar baz");
        // cursor at col 0, on 'f'
        assert!(state.delete_word_forward());
        assert_eq!(state.text_string(), "bar baz");
        assert_eq!(state.cursor_line_col(), (0, 0));
        assert!(state.is_dirty());
    }

    #[test]
    fn delete_word_forward_stops_at_newline_and_preserves_next_line() {
        let mut state = AppState::from_text(unique_temp_path("dw_nl"), "foo\nbar");
        // cursor on 'f'; dw should eat "foo" but NOT the '\n'.
        assert!(state.delete_word_forward());
        assert_eq!(state.text_string(), "\nbar");
        assert_eq!(state.cursor_line_col(), (0, 0));
    }

    #[test]
    fn delete_word_forward_on_newline_joins_lines() {
        let mut state = AppState::from_text(unique_temp_path("dw_join"), "ab\ncd");
        // Two move_right steps leave the cursor ON the '\n' (char idx 2). A third
        // move_right would cross into line 1, per move_right's end-of-line jump.
        state.move_right();
        state.move_right();
        assert_eq!(state.cursor_line_col(), (0, 2));
        let cursor_before = state.cursor_char_idx();

        assert!(state.delete_word_forward());
        assert_eq!(state.text_string(), "abcd");
        assert_eq!(state.cursor_char_idx(), cursor_before);
    }

    #[test]
    fn delete_word_forward_on_punct_run_eats_only_punct() {
        let mut state = AppState::from_text(unique_temp_path("dw_punct"), "!!!foo");
        assert!(state.delete_word_forward());
        // Punct class is "!!!", then no trailing space, so cursor lands on 'f'.
        assert_eq!(state.text_string(), "foo");
    }

    #[test]
    fn delete_word_forward_at_eof_is_noop() {
        let mut state = AppState::from_text(unique_temp_path("dw_eof"), "abc");
        state.move_right();
        state.move_right();
        state.move_right(); // col 3 == len_chars
        assert!(!state.delete_word_forward());
        assert_eq!(state.text_string(), "abc");
    }

    #[test]
    fn delete_word_backward_erases_previous_word_span() {
        let mut state = AppState::from_text(unique_temp_path("db"), "foo   bar");
        for _ in 0..6 {
            state.move_right();
        }
        assert_eq!(state.cursor_line_col(), (0, 6));

        assert!(state.delete_word_backward());
        assert_eq!(state.text_string(), "bar");
        assert_eq!(state.cursor_line_col(), (0, 0));
    }

    #[test]
    fn append_after_cursor_moves_one_step_or_stays_at_line_end() {
        let mut state = AppState::from_text(unique_temp_path("a"), "abc");
        assert!(state.append_after_cursor());
        assert_eq!(state.cursor_line_col(), (0, 1));

        state.move_to_line_end();
        assert!(!state.append_after_cursor());
        assert_eq!(state.cursor_line_col(), (0, 3));
    }

    #[test]
    fn change_word_forward_deletes_span_and_sets_dirty() {
        let mut state = AppState::from_text(unique_temp_path("cw"), "foo   bar");
        assert!(state.change_word_forward());
        assert_eq!(state.text_string(), "bar");
        assert_eq!(state.cursor_line_col(), (0, 0));
        assert!(state.is_dirty());
    }

    #[test]
    fn change_word_backward_deletes_previous_span_and_sets_dirty() {
        let mut state = AppState::from_text(unique_temp_path("cb"), "foo   bar");
        for _ in 0..6 {
            state.move_right();
        }
        assert_eq!(state.cursor_line_col(), (0, 6));

        assert!(state.change_word_backward());
        assert_eq!(state.text_string(), "bar");
        assert_eq!(state.cursor_line_col(), (0, 0));
        assert!(state.is_dirty());
    }

    #[test]
    fn replace_char_at_cursor_replaces_without_mode_change() {
        let mut state = AppState::from_text(unique_temp_path("r"), "abc");
        let mode_before = state.current_mode();

        assert!(state.replace_char_at_cursor('X'));
        assert_eq!(state.text_string(), "Xbc");
        assert_eq!(state.current_mode(), mode_before);
        assert!(state.is_dirty());
    }

    #[test]
    fn visual_selection_anchor_focus_and_delete_work() {
        let mut state = AppState::from_text(unique_temp_path("visual"), "abcdef");
        state.move_right(); // anchor at char index 1 ('b')
        state
            .apply_mode_event(ModeEvent::EnterVisual)
            .expect("normal -> visual");
        assert!(state.begin_visual_selection());

        state.move_right();
        state.move_right(); // focus at index 3 ('d') -> selected "bcd"

        let selection = state.visual_selection_range().expect("selection exists");
        assert_eq!(selection.start_char, 1);
        assert_eq!(selection.end_char, 4);
        assert!(state.delete_visual_selection());
        assert_eq!(state.text_string(), "aef");
    }

    #[test]
    fn move_word_forward_jumps_to_next_word_start() {
        let mut state = AppState::from_text(unique_temp_path("w"), "foo   bar");
        assert!(state.move_word_forward());
        assert_eq!(state.cursor_line_col(), (0, 6));
    }

    #[test]
    fn move_word_backward_jumps_to_previous_word_start() {
        let mut state = AppState::from_text(unique_temp_path("b"), "foo   bar");
        for _ in 0..8 {
            state.move_right();
        }
        assert_eq!(state.cursor_line_col(), (0, 8));

        assert!(state.move_word_backward());
        assert_eq!(state.cursor_line_col(), (0, 6));
        assert!(state.move_word_backward());
        assert_eq!(state.cursor_line_col(), (0, 0));
    }

    #[test]
    fn move_word_end_stops_at_last_char_of_word() {
        let mut state = AppState::from_text(unique_temp_path("e"), "foo   bar");
        assert!(state.move_word_end());
        assert_eq!(state.cursor_line_col(), (0, 2));

        assert!(state.move_word_forward());
        assert!(state.move_word_end());
        assert_eq!(state.cursor_line_col(), (0, 8));
    }

    #[test]
    fn line_start_and_first_non_whitespace_motions_work() {
        let mut state = AppState::from_text(unique_temp_path("line_motion"), "   abc");
        for _ in 0..5 {
            state.move_right();
        }
        assert_eq!(state.cursor_line_col(), (0, 5));

        assert!(state.move_to_line_start());
        assert_eq!(state.cursor_line_col(), (0, 0));

        assert!(state.move_to_first_non_whitespace());
        assert_eq!(state.cursor_line_col(), (0, 3));
    }

    #[test]
    fn mode_state_is_centralized_and_defaults_to_normal() {
        let state = AppState::new(unique_temp_path("mode"));
        assert_eq!(state.current_mode(), EditorMode::Normal);
    }

    #[test]
    fn mode_transition_normal_to_insert_via_app_state() {
        let mut state = AppState::new(unique_temp_path("mode"));
        assert_eq!(state.current_mode(), EditorMode::Normal);

        let result = state
            .apply_mode_event(ModeEvent::EnterInsert)
            .expect("normal -> insert should be valid");

        assert_eq!(result.from, EditorMode::Normal);
        assert_eq!(result.to, EditorMode::Insert);
        assert_eq!(state.current_mode(), EditorMode::Insert);
    }

    #[test]
    fn invalid_mode_transition_is_rejected_from_app_state() {
        let mut state = AppState::new(unique_temp_path("mode"));
        state
            .apply_mode_event(ModeEvent::EnterInsert)
            .expect("normal -> insert should be valid");
        let error = state
            .apply_mode_event(ModeEvent::EnterVisual)
            .expect_err("insert -> visual should be invalid");
        assert_eq!(
            error,
            crate::core::mode::ModeTransitionError::InvalidTransition {
                from: EditorMode::Insert,
                event: ModeEvent::EnterVisual
            }
        );
        assert_eq!(state.current_mode(), EditorMode::Insert);
    }

    #[test]
    fn focus_mode_returns_to_previous_mode_via_app_state() {
        let mut state = AppState::new(unique_temp_path("mode"));
        state
            .apply_mode_event(ModeEvent::OpenPalette)
            .expect("normal -> palette focus");
        assert_eq!(state.current_mode(), EditorMode::PaletteFocus);

        state
            .apply_mode_event(ModeEvent::ExitFocus)
            .expect("palette -> previous");
        assert_eq!(state.current_mode(), EditorMode::Normal);
    }

    #[test]
    fn workspace_and_file_picker_state_are_tracked() {
        let mut state = AppState::new(unique_temp_path("workspace"));
        let root = unique_temp_dir("workspace");
        fs::create_dir_all(root.join("src")).expect("create src");
        fs::write(root.join("src/picker.rs"), "pub fn picker() {}\n").expect("write source");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace should succeed");
        assert!(state.workspace_file_count() >= 1);

        let count = state.open_file_picker().expect("open picker");
        assert_eq!(count, 0);
        assert!(state.is_file_picker_open());

        let changed = state
            .file_picker_append_query("picker")
            .expect("append query");
        assert!(changed);
        assert_eq!(state.file_picker_query_text(), "picker");
        assert!(state.set_command_palette_results(
            CommandPaletteMode::FilePicker,
            "picker",
            vec![CommandPaletteItem::file_match(
                "src/picker.rs".to_string(),
                root.join("src/picker.rs"),
            )],
        ));
        assert_eq!(state.file_picker_results().len(), 1);
        assert!(
            state
                .file_picker_results()
                .iter()
                .any(|entry| entry.relative_path.ends_with("src/picker.rs"))
        );

        let _ = state.close_file_picker();
        assert!(!state.is_file_picker_open());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_picker_results_refresh_while_overlay_is_open_after_external_create() {
        let mut state = AppState::new(unique_temp_path("workspace_picker_refresh"));
        let root = unique_temp_dir("workspace_picker_refresh");
        fs::create_dir_all(root.join("src")).expect("create src");
        fs::write(root.join("src/old.rs"), "pub fn old() {}\n").expect("write old");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace should succeed");
        state.open_file_picker().expect("open picker");
        state
            .file_picker_append_query("old")
            .expect("append old query");
        assert!(state.set_command_palette_results(
            CommandPaletteMode::FilePicker,
            "old",
            vec![CommandPaletteItem::file_match(
                "src/old.rs".to_string(),
                root.join("src/old.rs"),
            )],
        ));

        assert!(
            state
                .file_picker_results()
                .iter()
                .all(|entry| !entry.relative_path.ends_with("src/new_file.rs"))
        );

        let created_path = root.join("src/new_file.rs");
        fs::write(&created_path, "pub fn new_file() {}\n").expect("write new file");
        let report = state
            .apply_external_file_events(&[FileSystemEvent {
                kind: FileSystemChangeKind::Create,
                path: created_path,
                new_path: None,
            }])
            .expect("apply external create");

        assert!(report.workspace_reloaded);
        assert!(state.is_file_picker_open());
        assert!(state.set_command_palette_results(
            CommandPaletteMode::FilePicker,
            "old",
            vec![
                CommandPaletteItem::file_match("src/old.rs".to_string(), root.join("src/old.rs")),
                CommandPaletteItem::file_match(
                    "src/new_file.rs".to_string(),
                    root.join("src/new_file.rs"),
                ),
            ],
        ));
        assert!(
            state
                .file_picker_results()
                .iter()
                .any(|entry| entry.relative_path.ends_with("src/new_file.rs"))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn external_modify_reloads_when_clean_and_warns_when_dirty() {
        let save_path = unique_temp_path("external");
        let mut state = AppState::new(save_path);
        let root = unique_temp_dir("external");
        fs::create_dir_all(root.join("src")).expect("create src");
        let active = root.join("src/main.rs");
        fs::write(&active, "fn main() {}\n").expect("write initial");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace should succeed");
        state.open_file(active.clone()).expect("open active file");

        fs::write(&active, "fn main() { println!(\"reload\"); }\n").expect("write modified");
        let report = state
            .apply_external_file_events(&[FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path: active.clone(),
                new_path: None,
            }])
            .expect("apply external clean");

        assert!(report.active_file_reloaded);
        assert!(state.preview(48).contains("reload"));

        state.insert_char('x');
        let dirty_report = state
            .apply_external_file_events(&[FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path: active.clone(),
                new_path: None,
            }])
            .expect("apply external dirty");
        assert!(dirty_report.conflict_detected);
        assert!(state.external_conflict_message().is_some());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn save_file_preserves_cursor_and_selection_state() {
        let root = unique_temp_dir("save_preserve_cursor");
        fs::create_dir_all(&root).expect("create temp dir");
        let file_path = root.join("sample.rs");
        fs::write(&file_path, "alpha\nbeta\ngamma\n").expect("write initial");

        let mut state = AppState::new(unique_temp_path("save_preserve_cursor_fallback"));
        state.open_file(file_path.clone()).expect("open file");
        state.move_down();
        assert!(state.move_to_line_end());
        state
            .apply_mode_event(ModeEvent::EnterVisual)
            .expect("enter visual");
        assert!(state.begin_visual_selection());
        state.move_left();

        let cursor_before = state.cursor_char_idx();
        let selection_before = state.selection_anchor_char_idx;

        let saved_path = state.save_file().expect("save file");

        assert_eq!(saved_path, file_path.canonicalize().expect("canonicalize"));
        assert_eq!(state.cursor_char_idx(), cursor_before);
        assert_eq!(state.selection_anchor_char_idx, selection_before);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn self_save_modify_event_is_ignored_without_reloading_cursor() {
        let root = unique_temp_dir("self_save_ignore");
        fs::create_dir_all(&root).expect("create temp dir");
        let file_path = root.join("main.rs");
        fs::write(&file_path, "one\ntwo\nthree\n").expect("write initial");

        let mut state = AppState::new(unique_temp_path("self_save_ignore_fallback"));
        state.open_file(file_path.clone()).expect("open file");
        state.move_down();
        assert!(state.move_to_line_end());
        let cursor_before = state.cursor_char_idx();

        state.save_file().expect("save file");

        fs::write(
            &file_path,
            "changed externally but should be ignored in debounce window\n",
        )
        .expect("rewrite file quickly");

        let report = state
            .apply_external_file_events(&[FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path: file_path.clone(),
                new_path: None,
            }])
            .expect("apply modify event");

        assert!(!report.active_file_reloaded);
        assert_eq!(state.cursor_char_idx(), cursor_before);
        assert!(!state.preview(128).contains("changed externally"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn external_reload_clamps_cursor_and_selection_to_new_buffer_length() {
        let root = unique_temp_dir("external_reload_clamp");
        fs::create_dir_all(&root).expect("create temp dir");
        let file_path = root.join("main.rs");
        fs::write(&file_path, "alpha\nbeta\ngamma\ndelta").expect("write initial");

        let mut state = AppState::new(unique_temp_path("external_reload_clamp_fallback"));
        state.open_file(file_path.clone()).expect("open file");
        assert!(state.move_to_last_line());
        assert!(state.move_to_line_end());
        state
            .apply_mode_event(ModeEvent::EnterVisual)
            .expect("enter visual");
        assert!(state.begin_visual_selection());
        state.move_up();

        fs::write(&file_path, "x\n").expect("write shorter file");
        state.last_saved_at = Some(Instant::now() - AppState::SELF_SAVE_IGNORE_WINDOW);

        let report = state
            .apply_external_file_events(&[FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path: file_path.clone(),
                new_path: None,
            }])
            .expect("apply external modify");

        assert!(report.active_file_reloaded);
        assert!(state.preview(16).starts_with('x'));
        assert_eq!(state.cursor_char_idx(), state.len_chars());
        assert!(
            state
                .selection_anchor_char_idx
                .is_none_or(|anchor| anchor <= state.len_chars())
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn external_reload_error_does_not_abort_workspace_updates() {
        let save_path = unique_temp_path("external_reload_error");
        let mut state = AppState::new(save_path);
        let root = unique_temp_dir("external_reload_error");
        fs::create_dir_all(root.join("src")).expect("create src");
        let active = root.join("src/main.rs");
        fs::write(&active, "fn main() {}\n").expect("write initial");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace should succeed");
        state.open_file(active.clone()).expect("open active file");
        state.open_file_picker().expect("open picker");
        state
            .file_picker_append_query("created")
            .expect("append created query");

        // Mô phỏng file active bị xóa từ bên ngoài trước khi có event modify/reload.
        fs::remove_file(&active).expect("remove active file");

        let created_path = root.join("src/created_after_delete.rs");
        fs::write(&created_path, "pub fn created() {}\n").expect("write created file");
        let report = state
            .apply_external_file_events(&[
                FileSystemEvent {
                    kind: FileSystemChangeKind::Modify,
                    path: active.clone(),
                    new_path: None,
                },
                FileSystemEvent {
                    kind: FileSystemChangeKind::Create,
                    path: created_path.clone(),
                    new_path: None,
                },
            ])
            .expect("apply external events should not fail");

        assert!(report.workspace_reloaded);
        assert!(state.set_command_palette_results(
            CommandPaletteMode::FilePicker,
            "created",
            vec![CommandPaletteItem::file_match(
                "src/created_after_delete.rs".to_string(),
                created_path.clone(),
            )],
        ));
        assert!(
            state
                .file_picker_results()
                .iter()
                .any(|entry| entry.relative_path.ends_with("src/created_after_delete.rs"))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn modify_event_on_missing_path_triggers_workspace_rescan_for_rename_like_flow() {
        let mut state = AppState::new(unique_temp_path("rename_like_modify"));
        let root = unique_temp_dir("rename_like_modify");
        fs::create_dir_all(root.join("src")).expect("create src");
        let old_path = root.join("src/old_name.rs");
        let new_path = root.join("src/new_name.rs");
        fs::write(&old_path, "pub fn old_name() {}\n").expect("write old");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace should succeed");
        state.open_file_picker().expect("open picker");
        state
            .file_picker_append_query("old_name")
            .expect("append old_name query");
        assert!(state.set_command_palette_results(
            CommandPaletteMode::FilePicker,
            "old_name",
            vec![CommandPaletteItem::file_match(
                "src/old_name.rs".to_string(),
                old_path.clone(),
            )],
        ));
        assert!(
            state
                .file_picker_results()
                .iter()
                .any(|entry| entry.relative_path.ends_with("src/old_name.rs"))
        );

        fs::rename(&old_path, &new_path).expect("rename file");
        let report = state
            .apply_external_file_events(&[FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path: old_path.clone(),
                new_path: None,
            }])
            .expect("apply external rename-like modify");

        assert!(report.workspace_reloaded);
        assert!(state.set_command_palette_results(
            CommandPaletteMode::FilePicker,
            "old_name",
            vec![CommandPaletteItem::file_match(
                "src/new_name.rs".to_string(),
                new_path.clone(),
            )],
        ));
        assert!(
            state
                .file_picker_results()
                .iter()
                .any(|entry| entry.relative_path.ends_with("src/new_name.rs"))
        );
        assert!(
            state
                .file_picker_results()
                .iter()
                .all(|entry| !entry.relative_path.ends_with("src/old_name.rs"))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn in_file_search_collects_matches_and_wraps_navigation() {
        let mut state = AppState::from_text(unique_temp_path("search"), "alpha beta alpha");

        assert!(state.set_in_file_search_query("alpha"));
        assert_eq!(state.search_highlights().len(), 2);
        assert_eq!(state.last_search_query(), "alpha");

        assert!(state.search_next());
        assert_eq!(state.cursor_char_idx(), 11);

        assert!(state.search_prev());
        assert_eq!(state.cursor_char_idx(), 0);
    }

    #[test]
    fn search_word_under_cursor_uses_whole_word_matches() {
        let mut state = AppState::from_text(unique_temp_path("star"), "foo foobar foo");

        assert!(state.search_word_under_cursor());
        assert_eq!(state.last_search_query(), "foo");
        assert_eq!(state.search_highlights().len(), 2);
        assert_eq!(state.cursor_char_idx(), 11);
    }

    #[test]
    fn clear_search_highlights_resets_query_and_matches() {
        let mut state = AppState::from_text(unique_temp_path("clear_search"), "alpha beta alpha");

        assert!(state.set_in_file_search_query("alpha"));
        assert_eq!(state.search_highlights().len(), 2);

        assert!(state.clear_search_highlights());
        assert!(state.last_search_query().is_empty());
        assert!(state.search_highlights().is_empty());
        assert!(!state.clear_search_highlights());
    }

    // ── find_text_object_bounds tests ────────────────────────────────────────

    #[test]
    fn text_object_select_enters_visual_mode() {
        let mut s = AppState::from_text(std::path::PathBuf::from("t.txt"), "foo(bar)");
        s.cursor_char_idx = 4; // trên 'b'
        // Bắt đầu từ Normal mode
        assert_eq!(s.current_mode(), EditorMode::Normal);
        let ok = s.select_text_object(TextObjectModifier::Inner, TextObjectKind::Bracket('(', ')'));
        assert!(ok);
        assert_eq!(s.current_mode(), EditorMode::Visual);
        // anchor nên là idx 4 ('b'), focus là idx 6 ('r')
        assert_eq!(s.selection_anchor_char_idx, Some(4));
        assert_eq!(s.cursor_char_idx, 6);
    }

    #[test]
    fn text_object_delete_removes_inner() {
        let mut s = AppState::from_text(std::path::PathBuf::from("t.txt"), "foo(bar)end");
        s.cursor_char_idx = 5; // 'a' inside parens
        let ok = s.delete_text_object(TextObjectModifier::Inner, TextObjectKind::Bracket('(', ')'));
        assert!(ok);
        // "foo()end" phải còn lại
        assert_eq!(s.text_string(), "foo()end");
        assert_eq!(s.cursor_char_idx, 4); // cursor dừng ở chỗ xóa
    }

    #[test]
    fn open_file_reveals_active_path_in_workspace_tree() {
        let mut state = AppState::new(unique_temp_path("workspace_reveal_on_open"));
        let root = unique_temp_dir("workspace_reveal_on_open");
        let nested_dir = root.join("src/ui");
        let active = nested_dir.join("tabs.rs");
        fs::create_dir_all(&nested_dir).expect("create nested dirs");
        fs::write(&active, "pub fn tabs() {}\n").expect("write active file");
        let canonical_root = root.canonicalize().expect("canonical root");
        let canonical_nested_dir = canonical_root.join("src/ui");
        let canonical_active = canonical_nested_dir.join("tabs.rs");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace should succeed");
        state.open_file(active.clone()).expect("open active file");

        assert_eq!(
            state.workspace_selected_path(),
            Some(canonical_active.as_path())
        );
        assert!(state.workspace_is_expanded(&canonical_root.join("src")));
        assert!(state.workspace_is_expanded(&canonical_nested_dir));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn references_buffer_tracks_selection_and_origin() {
        let mut state = AppState::new(unique_temp_path("references_buffer_state"));
        let first_path = PathBuf::from("/tmp/refs/a.rs");
        let second_path = PathBuf::from("/tmp/refs/b.rs");
        let origin_path = PathBuf::from("/tmp/origin.rs");
        let items = vec![
            ReferencesBufferItem {
                path: first_path.clone(),
                relative_path: "src/a.rs".to_string(),
                line: 10,
                column: 4,
                summary: "first reference".to_string(),
            },
            ReferencesBufferItem {
                path: second_path.clone(),
                relative_path: "src/b.rs".to_string(),
                line: 20,
                column: 7,
                summary: "second reference".to_string(),
            },
        ];

        let opened_index = state
            .open_references_buffer("References (2)", Some(origin_path.clone()), 6, items)
            .expect("references buffer should open");

        assert_eq!(state.active_buffer_index(), Some(opened_index));
        assert!(state.active_buffer_is_references());
        assert_eq!(state.active_filetype_label(), "References");
        assert_eq!(
            state
                .selected_reference_item()
                .map(|item| item.path.as_path()),
            Some(first_path.as_path())
        );
        assert_eq!(
            state.active_references_origin(),
            Some((origin_path.clone(), 6))
        );

        assert!(state.references_select_next());
        assert_eq!(
            state
                .selected_reference_item()
                .map(|item| item.path.as_path()),
            Some(second_path.as_path())
        );

        assert!(state.references_select_next());
        assert_eq!(
            state
                .selected_reference_item()
                .map(|item| item.path.as_path()),
            Some(first_path.as_path())
        );

        assert!(state.references_select_prev());
        assert_eq!(
            state
                .selected_reference_item()
                .map(|item| item.path.as_path()),
            Some(second_path.as_path())
        );

        assert_eq!(
            state
                .save_file()
                .expect_err("references buffer cannot be saved"),
            "cannot save references buffer"
        );
    }

    #[test]
    fn pending_references_buffer_accepts_async_results_and_preview() {
        let mut state = AppState::new(unique_temp_path("pending_references_buffer"));
        let origin_path = PathBuf::from("/tmp/origin.rs");
        let item_path = PathBuf::from("/tmp/refs/a.rs");
        let request_id = 77;

        state.open_pending_references_buffer(
            "References",
            Some(origin_path.clone()),
            8,
            request_id,
        );
        assert!(state.active_buffer_is_references());
        let loading = state
            .active_references_buffer()
            .expect("references buffer should be active");
        assert!(loading.loading);
        assert_eq!(loading.pending_request_id, Some(request_id));
        assert!(loading.items.is_empty());

        assert!(state.finish_pending_references_buffer(
            request_id,
            "References (2)",
            vec![
                ReferencesBufferItem {
                    path: item_path.clone(),
                    relative_path: "src/a.rs".to_string(),
                    line: 10,
                    column: 4,
                    summary: "Ln 11, Col 5".to_string(),
                },
                ReferencesBufferItem {
                    path: PathBuf::from("/tmp/refs/b.rs"),
                    relative_path: "src/b.rs".to_string(),
                    line: 20,
                    column: 2,
                    summary: "Ln 21, Col 3".to_string(),
                },
            ],
        ));

        assert_eq!(
            state
                .selected_reference_item()
                .map(|item| item.path.as_path()),
            Some(item_path.as_path())
        );
        let loaded = state
            .active_references_buffer()
            .expect("references buffer should stay active");
        assert!(!loaded.loading);
        assert_eq!(loaded.pending_request_id, None);
        assert!(loaded.preview_lines.is_empty());

        assert!(state.set_active_references_preview(
            vec![FilePreviewLine {
                line_number: 11,
                text: "call()".to_string(),
                is_target: true,
            }],
            String::new(),
            Vec::new(),
        ));
        assert_eq!(
            state
                .active_references_buffer()
                .expect("references buffer")
                .preview_lines
                .len(),
            1
        );

        assert!(state.references_select_next());
        assert!(
            state
                .active_references_buffer()
                .expect("references buffer")
                .preview_lines
                .is_empty()
        );
    }

    #[test]
    fn failing_pending_references_buffer_surfaces_status() {
        let mut state = AppState::new(unique_temp_path("pending_references_failure"));

        state.open_pending_references_buffer("References", None, 0, 91);

        assert!(state.fail_pending_references_buffer(91, "No references found"));

        let references = state
            .active_references_buffer()
            .expect("references buffer should stay active");
        assert!(!references.loading);
        assert_eq!(references.title, "References (0)");
        assert_eq!(
            references.status_message.as_deref(),
            Some("No references found")
        );
        assert!(references.items.is_empty());
    }

    #[test]
    fn completion_prefix_info_stops_at_member_access_boundary() {
        let state = AppState::from_text(unique_temp_path("completion_prefix"), "MessageManager.ge");
        let info = state.completion_prefix_info_at(0, "MessageManager.ge".chars().count());

        assert_eq!(info.start_col, "MessageManager.".chars().count());
        assert_eq!(info.prefix, "ge");
    }

    #[test]
    fn replace_completion_prefix_at_cursor_deletes_prefix_then_inserts_item() {
        let mut state =
            AppState::from_text(unique_temp_path("completion_replace"), "MessageManager.ge");
        assert!(state.jump_to_line_and_column(0, "MessageManager.ge".chars().count()));

        assert!(state.replace_completion_prefix_at_cursor(2, "getInstance()"));
        assert_eq!(state.text_string(), "MessageManager.getInstance()");
        assert_eq!(
            state.cursor_char_idx(),
            "MessageManager.getInstance()".chars().count()
        );
        assert!(state.undo());
        assert_eq!(state.text_string(), "MessageManager.ge");
    }
}
