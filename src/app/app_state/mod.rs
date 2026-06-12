use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
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
use crate::core::transaction::{CursorState, EditHistory, EditTransaction, Transaction};
use crate::editor_core::filetype_label_for_path;
use crate::syntax::highlight::HighlightEdit;
use crate::text::text_system::StyledTextSpan;
use crate::workspace::model::{WorkspaceModel, WorkspaceNodeType};

mod buffers;
mod editor;
mod multi_cursor;
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
    pub conflict_path: Option<PathBuf>,
    /// Inactive buffers whose content was reloaded from disk. The shell must
    /// re-sync these with the LSP server: a stale didOpen overlay would keep
    /// shadowing the new on-disk content for cross-file diagnostics.
    pub inactive_reloaded_paths: Vec<PathBuf>,
    /// Buffers whose content must be re-read from disk. The shell submits a
    /// `ReadExternalFiles` worker request for these and applies the results
    /// via `apply_external_file_contents` — the UI thread never reads files.
    pub pending_reload_paths: Vec<PathBuf>,
    /// The workspace tree shape may have changed; the shell submits an async
    /// `RescanWorkspace` and applies fresh nodes via `apply_workspace_rescan`.
    pub workspace_rescan_needed: bool,
    pub notices: Vec<String>,
}

/// `WorkDoneProgress` lifecycle kind sent by the LSP server inside a
/// `$/progress` notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspProgressKind {
    Begin,
    Report,
    End,
}

/// Snapshot of the most recent `$/progress` notification reported by an LSP
/// server (e.g. rust-analyzer's `PrimeCaches`/`Indexing`). When `Some`, the
/// status bar shows a "[⏳ LSP: …]" hint so the user knows the server is busy.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LspProgressEntry {
    pub server: String,
    pub token: String,
    pub title: Option<String>,
    pub message: Option<String>,
    pub percentage: Option<u32>,
}

impl LspProgressEntry {
    pub fn status_label(&self) -> String {
        let head = match (self.title.as_deref(), self.message.as_deref()) {
            (Some(t), Some(m)) if !m.is_empty() && t != m => format!("{t}: {m}"),
            (Some(t), _) => t.to_string(),
            (None, Some(m)) => m.to_string(),
            (None, None) => "working".to_string(),
        };
        match self.percentage {
            Some(pct) => format!("{head} {pct}%"),
            None => head,
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq)]
struct TextBufferViewState {
    cursor: CursorState,
    selection_anchor_char_idx: Option<usize>,
    visual_line_mode: bool,
    target_scroll_y: f32,
    current_scroll_y: f32,
    scroll_column: usize,
}

impl Default for TextBufferViewState {
    fn default() -> Self {
        Self {
            cursor: CursorState {
                char_idx: 0,
                target_col: 0,
            },
            selection_anchor_char_idx: None,
            visual_line_mode: false,
            target_scroll_y: 0.0,
            current_scroll_y: 0.0,
            scroll_column: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorBuffer {
    pub path: PathBuf,
    pub language_id: Option<String>,
    pub git_baseline: Option<String>,
    pub git_line_statuses: HashMap<usize, GitLineStatus>,
    /// Per-buffer RAM-only undo/redo stack. Lives for the current app session.
    pub history: EditHistory,
    /// Last in-memory text snapshot for this tab in the current app session.
    pub in_memory_text: Option<Rope>,
    /// Dirty flag captured when this buffer lost focus.
    pub dirty: bool,
    /// True when the backing file is missing on disk (for example after switching git branch).
    pub missing_on_disk: bool,
    view_state: TextBufferViewState,
    pub last_known_modified_time: Option<std::time::SystemTime>,
}

impl EditorBuffer {
    pub fn new(path: PathBuf, language_id: Option<String>) -> Self {
        let last_known_modified_time = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        Self {
            path,
            language_id,
            git_baseline: None,
            git_line_statuses: HashMap::new(),
            history: EditHistory::new(),
            in_memory_text: None,
            dirty: false,
            missing_on_disk: false,
            view_state: TextBufferViewState::default(),
            last_known_modified_time,
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

/// A virtual cursor placed on an additional match during MultiCursor mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtualCursor {
    /// Current position of this cursor in the buffer (char index).
    pub char_idx: usize,
    /// Start of the selected word range (inclusive char index).
    pub selection_start: Option<usize>,
    /// End of the selected word range (exclusive char index).
    pub selection_end: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualBlockRange {
    pub start_line: usize,
    pub end_line: usize,
    pub start_col: usize,
    pub end_col: usize,
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
    pub path_counts: std::collections::HashMap<String, usize>,
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

#[derive(Debug, Clone, PartialEq)]
pub struct HelpState {
    pub title: String,
    pub subtitle: String,
    pub profile_name: String,
    pub source_label: String,
    pub sections: Vec<HelpSection>,
    pub lines: Vec<String>,
    pub scroll_y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionCategory {
    CliTools,
    LanguageServers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionsTab {
    All,
    Installed,
    Available,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionItem {
    pub name: String,
    pub subtitle: String,
    pub binary: String,
    pub category: ExtensionCategory,
    pub tag: String,
    pub macos_install: String,
    pub linux_install: String,
    pub macos_uninstall: String,
    pub linux_uninstall: String,
    pub extensions: Vec<String>,
    pub installed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionsManagerState {
    pub title: String,
    pub platform: String,
    pub filter: String,
    pub filter_focused: bool,
    pub selected_index: usize,
    pub tab: ExtensionsTab,
    pub expanded_binary: Option<String>,
    pub items: Vec<ExtensionItem>,
    pub command: Option<ExtensionCommandState>,
    /// False until the first async `which` sweep returns — installed states are
    /// unknown before that and must render as "checking", not as a guess.
    pub deps_checked: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionCommandState {
    pub binary: String,
    pub uninstall: bool,
    pub running: bool,
    pub success: Option<bool>,
    pub exit_code: Option<i32>,
    pub logs: Vec<String>,
}

impl ExtensionsManagerState {
    pub fn new() -> Self {
        Self {
            title: "Extensions".to_string(),
            platform: if cfg!(target_os = "macos") {
                "macOS".to_string()
            } else {
                "Linux".to_string()
            },
            filter: String::new(),
            filter_focused: false,
            selected_index: 0,
            tab: ExtensionsTab::All,
            expanded_binary: None,
            items: default_extension_items(),
            command: None,
            deps_checked: false,
        }
    }

    pub fn installed_count(&self) -> usize {
        self.items.iter().filter(|item| item.installed).count()
    }

    pub fn available_count(&self) -> usize {
        self.items.len().saturating_sub(self.installed_count())
    }

    pub fn category_counts(&self, category: ExtensionCategory) -> (usize, usize) {
        let total = self
            .items
            .iter()
            .filter(|item| item.category == category)
            .count();
        let installed = self
            .items
            .iter()
            .filter(|item| item.category == category && item.installed)
            .count();
        (installed, total)
    }

    pub fn visible_item_indices(&self) -> Vec<usize> {
        let query = self.filter.trim().to_ascii_lowercase();
        self.items
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                let matches_tab = match self.tab {
                    ExtensionsTab::All => true,
                    ExtensionsTab::Installed => item.installed,
                    ExtensionsTab::Available => !item.installed,
                };
                let matches_query = query.is_empty()
                    || item.name.to_ascii_lowercase().contains(&query)
                    || item.subtitle.to_ascii_lowercase().contains(&query)
                    || item.binary.to_ascii_lowercase().contains(&query)
                    || item.tag.to_ascii_lowercase().contains(&query)
                    || item
                        .extensions
                        .iter()
                        .any(|ext| ext.to_ascii_lowercase().contains(&query));
                if matches_tab && matches_query {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn selected_item_index(&self) -> Option<usize> {
        let visible = self.visible_item_indices();
        visible
            .get(self.selected_index.min(visible.len().saturating_sub(1)))
            .copied()
    }

    pub fn selected_item(&self) -> Option<&ExtensionItem> {
        self.selected_item_index()
            .and_then(|idx| self.items.get(idx))
    }

    pub fn select_next(&mut self) -> bool {
        let len = self.visible_item_indices().len();
        if self.selected_index + 1 < len {
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

    pub fn append_filter(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        self.filter.push_str(text);
        self.selected_index = 0;
        true
    }

    pub fn backspace_filter(&mut self) -> bool {
        if self.filter.pop().is_some() {
            self.selected_index = 0;
            true
        } else {
            false
        }
    }

    pub fn switch_tab_next(&mut self) -> bool {
        self.tab = match self.tab {
            ExtensionsTab::All => ExtensionsTab::Installed,
            ExtensionsTab::Installed => ExtensionsTab::Available,
            ExtensionsTab::Available => ExtensionsTab::All,
        };
        self.selected_index = 0;
        true
    }

    pub fn switch_tab_prev(&mut self) -> bool {
        self.tab = match self.tab {
            ExtensionsTab::All => ExtensionsTab::Available,
            ExtensionsTab::Installed => ExtensionsTab::All,
            ExtensionsTab::Available => ExtensionsTab::Installed,
        };
        self.selected_index = 0;
        true
    }

    pub fn toggle_expanded_selected(&mut self) -> bool {
        let Some(item) = self.selected_item() else {
            return false;
        };
        let binary = item.binary.clone();
        if self.expanded_binary.as_deref() == Some(binary.as_str()) {
            self.expanded_binary = None;
        } else {
            self.expanded_binary = Some(binary);
        }
        true
    }

    pub fn start_command(&mut self, binary: String, uninstall: bool) {
        self.command = Some(ExtensionCommandState {
            binary,
            uninstall,
            running: true,
            success: None,
            exit_code: None,
            logs: Vec::new(),
        });
    }

    pub fn push_command_log(&mut self, binary: &str, line: String) -> bool {
        let Some(command) = self.command.as_mut() else {
            return false;
        };
        if command.binary != binary {
            return false;
        }
        command.logs.push(line);
        if command.logs.len() > 200 {
            let drain_count = command.logs.len().saturating_sub(200);
            command.logs.drain(0..drain_count);
        }
        true
    }

    pub fn finish_command(
        &mut self,
        binary: &str,
        uninstall: bool,
        success: bool,
        exit_code: Option<i32>,
    ) -> bool {
        let Some(command) = self.command.as_mut() else {
            return false;
        };
        if command.binary != binary {
            return false;
        }
        command.uninstall = uninstall;
        command.running = false;
        command.success = Some(success);
        command.exit_code = exit_code;
        true
    }
}

fn default_extension_items() -> Vec<ExtensionItem> {
    let mut items = vec![
        cli_extension(
            "fzf",
            "Fuzzy finder for file picker & live grep",
            "fzf",
            "SEARCH",
            "brew install fzf",
            "sudo apt install fzf",
            false,
        ),
        cli_extension(
            "ripgrep",
            "Fast text search for live grep",
            "rg",
            "SEARCH",
            "brew install ripgrep",
            "sudo apt install ripgrep",
            false,
        ),
        cli_extension(
            "fd",
            "Fast file finder (alternative to find)",
            "fd",
            "SEARCH",
            "brew install fd",
            "sudo apt install fd-find",
            false,
        ),
        cli_extension(
            "lazygit",
            "Git TUI integration",
            "lazygit",
            "GIT",
            "brew install lazygit",
            "sudo apt install lazygit",
            false,
        ),
        cli_extension(
            "lazydocker",
            "Docker TUI integration",
            "lazydocker",
            "DEVOPS",
            "brew install lazydocker",
            "sudo apt install lazydocker",
            false,
        ),
        cli_extension(
            "bat",
            "Syntax-highlighted file previews",
            "bat",
            "UTILITY",
            "brew install bat",
            "sudo apt install bat",
            false,
        ),
        cli_extension(
            "delta",
            "Git diff viewer with syntax highlighting",
            "delta",
            "GIT",
            "brew install git-delta",
            "sudo apt install git-delta",
            false,
        ),
        cli_extension(
            "opencode",
            "AI code assistant",
            "opencode",
            "AI",
            "curl -fsSL https://opencode.ai/install | sh",
            "curl -fsSL https://opencode.ai/install | sh",
            false,
        ),
    ];

    items.extend([
        lsp_extension(
            "Rust",
            "rust-analyzer",
            "rustup component add rust-analyzer",
            vec![".rs"],
            false,
        ),
        lsp_extension(
            "JavaScript",
            "typescript-language-server",
            "npm install -g typescript typescript-language-server",
            vec![".js", ".mjs", ".cjs"],
            false,
        ),
        lsp_extension(
            "JSX",
            "typescript-language-server",
            "npm install -g typescript typescript-language-server",
            vec![".jsx"],
            false,
        ),
        lsp_extension(
            "TypeScript",
            "typescript-language-server",
            "npm install -g typescript typescript-language-server",
            vec![".ts"],
            false,
        ),
        lsp_extension(
            "TSX",
            "typescript-language-server",
            "npm install -g typescript typescript-language-server",
            vec![".tsx"],
            false,
        ),
        lsp_extension(
            "Go",
            "gopls",
            "go install golang.org/x/tools/gopls@latest",
            vec![".go"],
            false,
        ),
        // Dart's language server ships inside the SDK (`dart language-server`),
        // so detection checks the `dart` binary itself (Flutter/FVM also provide it).
        lsp_extension(
            "Dart",
            "dart",
            "brew tap dart-lang/dart && brew install dart",
            vec![".dart"],
            false,
        ),
        lsp_extension(
            "Python",
            "pylsp",
            "pip install python-lsp-server",
            vec![".py"],
            false,
        ),
        lsp_extension("Java", "jdtls", "brew install jdtls", vec![".java"], false),
        lsp_extension(
            "SQL",
            "sqls",
            "go install github.com/sqls-server/sqls@latest",
            vec![".sql"],
            false,
        ),
        lsp_extension(
            "YAML",
            "yaml-language-server",
            "npm install -g yaml-language-server",
            vec![".yaml", ".yml"],
            false,
        ),
        lsp_extension(
            "Dockerfile",
            "docker-langserver",
            "npm install -g dockerfile-language-server-nodejs",
            vec!["Dockerfile*"],
            false,
        ),
        lsp_extension(
            "JSON",
            "vscode-json-language-server",
            "npm install -g vscode-langservers-extracted",
            vec![".json"],
            false,
        ),
        lsp_extension(
            "Bash",
            "bash-language-server",
            "npm install -g bash-language-server",
            vec![".sh"],
            false,
        ),
    ]);
    items
}

fn cli_extension(
    name: &str,
    subtitle: &str,
    binary: &str,
    tag: &str,
    macos: &str,
    linux: &str,
    installed: bool,
) -> ExtensionItem {
    let macos_uninstall = macos
        .strip_prefix("brew install ")
        .map(|package| format!("brew uninstall {package}"))
        .unwrap_or_default();
    let linux_uninstall = linux
        .strip_prefix("sudo apt install ")
        .map(|package| format!("sudo apt remove -y {package}"))
        .unwrap_or_default();
    ExtensionItem {
        name: name.to_string(),
        subtitle: subtitle.to_string(),
        binary: binary.to_string(),
        category: ExtensionCategory::CliTools,
        tag: tag.to_string(),
        macos_install: macos.to_string(),
        linux_install: linux.to_string(),
        macos_uninstall,
        linux_uninstall,
        extensions: Vec::new(),
        installed,
    }
}

fn lsp_extension(
    language: &str,
    binary: &str,
    install: &str,
    extensions: Vec<&str>,
    installed: bool,
) -> ExtensionItem {
    ExtensionItem {
        name: format!("({binary})"),
        subtitle: format!("{language} language server"),
        binary: binary.to_string(),
        category: ExtensionCategory::LanguageServers,
        tag: language.to_uppercase(),
        macos_install: install.to_string(),
        linux_install: install.to_string(),
        macos_uninstall: uninstall_command_for_lsp(binary, install),
        linux_uninstall: uninstall_command_for_lsp(binary, install),
        extensions: extensions.into_iter().map(str::to_string).collect(),
        installed,
    }
}

fn uninstall_command_for_lsp(binary: &str, install: &str) -> String {
    if let Some(package) = install.strip_prefix("brew install ") {
        return format!("brew uninstall {package}");
    }
    if let Some(package) = install.strip_prefix("npm install -g ") {
        return format!("npm uninstall -g {package}");
    }
    if install.starts_with("pip install ") {
        return "pip uninstall -y python-lsp-server".to_string();
    }
    if binary == "rust-analyzer" {
        return "rustup component remove rust-analyzer".to_string();
    }
    if binary == "gopls" {
        return "rm -f $(go env GOPATH)/bin/gopls".to_string();
    }
    if binary == "sqls" {
        return "rm -f $(go env GOPATH)/bin/sqls".to_string();
    }
    String::new()
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarkdownPreviewState {
    pub visible: bool,
    pub scroll_y: f32,
    pub max_scroll: f32,
    pub source_path: Option<PathBuf>,
    pub source_text: String,
    pub rendered_lines: Vec<MarkdownPreviewLine>,
    pub source_revision: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarkdownPreviewLine {
    pub text: String,
    pub spans: Vec<StyledTextSpan>,
    pub block_type: MarkdownBlockType,
    pub code_language: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkdownBlockType {
    Heading(u8),
    Paragraph,
    CodeBlock,
    BlockQuote,
    ListItem,
    HorizontalRule,
    TableHeader,
    TableRow,
    Empty,
}

impl Default for MarkdownPreviewState {
    fn default() -> Self {
        Self {
            visible: false,
            scroll_y: 0.0,
            max_scroll: 0.0,
            source_path: None,
            source_text: String::new(),
            rendered_lines: Vec::new(),
            source_revision: 0,
        }
    }
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
            scroll_y: 0.0,
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
        (Some("terminal"), "TERMINAL", "mode = terminal"),
        (
            Some("terminal_normal"),
            "TERMINAL NORMAL",
            "mode = terminal_normal",
        ),
        (Some("explorer"), "EXPLORER", "mode = explorer"),
        (Some("multicursor"), "MULTICURSOR", "mode = multicursor"),
        (Some("multiinsert"), "MULTIINSERT", "mode = multiinsert"),
    ];

    let mut sections: Vec<HelpSection> = specs
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
        .collect();

    // ── Getting Started section (beginner guide) ────────────────────────────
    // Insert at the front so it appears first in the card grid.
    let getting_started = HelpSection {
        title: "GETTING STARTED".to_string(),
        mode_hint: "for new users".to_string(),
        entries: vec![
            HelpEntry {
                keys: vec!["Normal".into()],
                label: "Default mode — navigate & run commands".into(),
            },
            HelpEntry {
                keys: vec!["Insert".into()],
                label: "Type text — press i to enter, Esc to exit".into(),
            },
            HelpEntry {
                keys: vec!["Visual".into()],
                label: "Select text — press v (line: V), Esc to exit".into(),
            },
            HelpEntry {
                keys: vec!["leader".into()],
                label: "= Space — prefix for custom shortcuts".into(),
            },
            HelpEntry {
                keys: vec!["cmd".into()],
                label: "= ⌘ on macOS — app shortcuts".into(),
            },
            HelpEntry {
                keys: vec!["count".into()],
                label: "Number prefix — repeat N times (5j = ↓5)".into(),
            },
            HelpEntry {
                keys: vec![":".into()],
                label: "Open vim command line (:w, :q, :help)".into(),
            },
            HelpEntry {
                keys: vec!["Esc".into()],
                label: "Cancel / return to Normal mode".into(),
            },
        ],
    };
    sections.insert(0, getting_started);

    // ── Vim Commands section ─────────────────────────────────────────────────
    let vim_commands = HelpSection {
        title: "VIM COMMANDS".to_string(),
        mode_hint: "type : in normal mode".to_string(),
        entries: vec![
            HelpEntry {
                keys: vec![":w".into()],
                label: "Save file".into(),
            },
            HelpEntry {
                keys: vec![":q".into()],
                label: "Close buffer".into(),
            },
            HelpEntry {
                keys: vec![":wq".into()],
                label: "Save & close".into(),
            },
            HelpEntry {
                keys: vec![":help".into()],
                label: "Open cheat sheet".into(),
            },
            HelpEntry {
                keys: vec![":enew".into()],
                label: "New scratch buffer".into(),
            },
            HelpEntry {
                keys: vec![":bn".into()],
                label: "Next buffer".into(),
            },
            HelpEntry {
                keys: vec![":bp".into()],
                label: "Previous buffer".into(),
            },
            HelpEntry {
                keys: vec![":bd".into()],
                label: "Close buffer".into(),
            },
            HelpEntry {
                keys: vec![":42".into()],
                label: "Jump to line 42".into(),
            },
        ],
    };
    sections.insert(1, vim_commands);

    // ── Append built-in leader sequences not already in TOML ────────────────
    let normal_extra = vec![
        (vec!["spc".into(), "e".into()], "Focus explorer"),
        (vec!["spc".into(), "i".into()], "Focus inspector"),
        (vec!["spc".into(), "s".into()], "Leap jump"),
    ];
    if let Some(section) = sections.iter_mut().find(|s| s.title == "NORMAL") {
        for (keys, label) in normal_extra {
            if !section.entries.iter().any(|e| e.keys == keys) {
                section.entries.push(HelpEntry {
                    keys,
                    label: label.to_string(),
                });
            }
        }
    }

    let visual_extra: Vec<(Vec<String>, &str)> = vec![];
    if let Some(section) = sections.iter_mut().find(|s| s.title == "VISUAL") {
        for (keys, label) in visual_extra {
            if !section.entries.iter().any(|e| e.keys == keys) {
                section.entries.push(HelpEntry {
                    keys,
                    label: label.to_string(),
                });
            }
        }
    }

    let global_extra = vec![
        (vec!["spc".into(), "e".into()], "Focus explorer"),
        (vec!["spc".into(), "i".into()], "Focus inspector"),
    ];
    if let Some(section) = sections.iter_mut().find(|s| s.title == "GLOBAL") {
        for (keys, label) in global_extra {
            if !section.entries.iter().any(|e| e.keys == keys) {
                section.entries.push(HelpEntry {
                    keys,
                    label: label.to_string(),
                });
            }
        }
    }

    sections
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
    ];

    // ── Getting Started ─────────────────────────────────────────────────────
    lines.push("Getting Started".to_string());
    lines.push("".to_string());
    lines.push("  Netherize is a Vim-inspired editor. You operate in different MODES:".to_string());
    lines.push("".to_string());
    lines.push("  NORMAL mode   — Default. Navigate & issue commands (like Vim).".to_string());
    lines.push("  INSERT mode   — Type text. Enter with i, exit with Escape.".to_string());
    lines.push("  VISUAL mode   — Select text. Enter with v, line-select with V.".to_string());
    lines.push("  PALETTE mode  — Command palette / file picker overlay.".to_string());
    lines.push("".to_string());
    lines.push("  Key concepts:".to_string());
    lines.push(
        "    leader = Space     Prefix key for custom shortcuts (press Space then key)."
            .to_string(),
    );
    lines.push(
        "    mod    = Cmd (macOS) / Ctrl (Linux)   Modifier key for app shortcuts.".to_string(),
    );
    lines.push(
        "    count  = number    Repeat a command N times (e.g. 5j = move down 5 lines)."
            .to_string(),
    );
    lines.push("".to_string());

    // ── Command Palette & Vim Commands ──────────────────────────────────────
    lines.push("Command Palette & Vim Commands".to_string());
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
    append_help_binding(
        &mut lines,
        bindings,
        "app.open_vim_command",
        "Vim command line",
    );
    lines.push("".to_string());
    lines.push("  Vim commands (type : in normal mode):".to_string());
    lines.push("    :w                     Save current file".to_string());
    lines.push("    :q                     Close current buffer".to_string());
    lines.push("    :wq                    Save and close".to_string());
    lines.push("    :help / :h             Open this cheat sheet".to_string());
    lines.push("    :enew                  New scratch buffer".to_string());
    lines.push("    :bn / :bp              Next / previous buffer".to_string());
    lines.push("    :bd                    Close current buffer".to_string());
    lines.push("    :<number>              Jump to line number".to_string());
    lines.push("".to_string());

    // ── Modes ───────────────────────────────────────────────────────────────
    lines.push("Mode Switching".to_string());
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
    append_help_binding(
        &mut lines,
        bindings,
        "mode.enter_visual",
        "Enter visual mode",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "mode.enter_visual_line",
        "Visual line mode",
    );
    lines.push("".to_string());

    // ── Navigation ──────────────────────────────────────────────────────────
    lines.push("Navigation (Normal / Visual)".to_string());
    append_help_binding(&mut lines, bindings, "editor.move_left", "Move left");
    append_help_binding(&mut lines, bindings, "editor.move_down", "Move down");
    append_help_binding(&mut lines, bindings, "editor.move_up", "Move up");
    append_help_binding(&mut lines, bindings, "editor.move_right", "Move right");
    append_help_binding(
        &mut lines,
        bindings,
        "editor.move_word_forward",
        "Word forward",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.move_word_backward",
        "Word backward",
    );
    append_help_binding(&mut lines, bindings, "editor.move_word_end", "Word end");
    append_help_binding(
        &mut lines,
        bindings,
        "editor.move_to_line_start",
        "Line start",
    );
    append_help_binding(&mut lines, bindings, "editor.move_to_line_end", "Line end");
    append_help_binding(
        &mut lines,
        bindings,
        "editor.move_to_first_non_whitespace",
        "First non-blank",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.move_to_first_line",
        "First line",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.move_to_last_line",
        "Last line",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.move_paragraph_up",
        "Paragraph up",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.move_paragraph_down",
        "Paragraph down",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.scroll_half_page_up",
        "½ page up",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.scroll_half_page_down",
        "½ page down",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.center_cursor_line",
        "Center cursor",
    );
    lines.push("  f/F <char>            Find char on line (→/←), highlights all matches".to_string());
    lines.push("  t/T <char>            Till char on line (→/←)".to_string());
    lines.push("  n / N                 Repeat find-char or search jump (→/←)".to_string());
    lines.push("".to_string());

    // ── Editing ─────────────────────────────────────────────────────────────
    lines.push("Editing (Normal mode)".to_string());
    append_help_binding(
        &mut lines,
        bindings,
        "editor.append_after_cursor",
        "Append after cursor",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.append_at_line_end",
        "Append at line end",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.insert_at_line_start",
        "Insert at line start",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.insert_line_below",
        "New line below",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.insert_line_above",
        "New line above",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.substitute_line",
        "Substitute line",
    );
    append_help_binding(&mut lines, bindings, "editor.delete_char", "Delete char");
    append_help_binding(
        &mut lines,
        bindings,
        "editor.delete_current_line",
        "Delete line",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.delete_to_line_end",
        "Delete to line end",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.delete_word_forward",
        "Delete word →",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.delete_word_backward",
        "Delete word ←",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.change_word_forward",
        "Change word →",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.change_word_backward",
        "Change word ←",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.change_to_line_end",
        "Change to line end",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.yank_to_line_end",
        "Yank to line end",
    );
    append_help_binding(&mut lines, bindings, "editor.join_lines", "Join lines");
    append_help_binding(
        &mut lines,
        bindings,
        "editor.toggle_line_comment",
        "Toggle comment",
    );
    append_help_binding(&mut lines, bindings, "editor.paste_after", "Paste after");
    append_help_binding(&mut lines, bindings, "editor.paste_before", "Paste before");
    append_help_binding(&mut lines, bindings, "editor.undo", "Undo");
    append_help_binding(&mut lines, bindings, "editor.redo", "Redo");
    lines.push("".to_string());

    // ── Visual mode ─────────────────────────────────────────────────────────
    lines.push("Visual Mode".to_string());
    append_help_binding(
        &mut lines,
        bindings,
        "editor.delete_selection",
        "Delete selection",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.change_selection",
        "Change selection",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.yank_selection",
        "Yank (copy) selection",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.toggle_selection_comment",
        "Toggle comment",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.wrap_selection_with_star",
        "Wrap with *",
    );
    lines.push("".to_string());

    // ── Search ──────────────────────────────────────────────────────────────
    lines.push("Search".to_string());
    append_help_binding(
        &mut lines,
        bindings,
        "editor.open_in_file_search",
        "Search in file",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "editor.search_word_under_cursor",
        "Search word under cursor",
    );
    append_help_binding(&mut lines, bindings, "editor.search_next", "Next match");
    append_help_binding(&mut lines, bindings, "editor.search_prev", "Previous match");
    append_help_binding(
        &mut lines,
        bindings,
        "editor.clear_search_highlights",
        "Clear highlights",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "app.search_in_files",
        "Search in files (project)",
    );
    lines.push("".to_string());

    // ── Buffers ─────────────────────────────────────────────────────────────
    lines.push("Buffers & Tabs".to_string());
    append_help_binding(&mut lines, bindings, "buffer.next", "Next buffer");
    append_help_binding(&mut lines, bindings, "buffer.prev", "Previous buffer");
    append_help_binding(&mut lines, bindings, "buffer.close_current", "Close buffer");
    lines.push("  tip: mod+1 .. mod+9   Jump to buffer 1-9 directly".to_string());
    lines.push("".to_string());

    // ── LSP ─────────────────────────────────────────────────────────────────
    lines.push("LSP / Code Intelligence".to_string());
    append_help_binding(&mut lines, bindings, "lsp.hover", "Hover documentation");
    append_help_binding(
        &mut lines,
        bindings,
        "lsp.go_to_definition",
        "Go to definition",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "lsp.preview_definition",
        "Preview definition",
    );
    append_help_binding(&mut lines, bindings, "lsp.references", "Find references");
    append_help_binding(&mut lines, bindings, "lsp.rename", "Rename symbol");
    append_help_binding(
        &mut lines,
        bindings,
        "lsp.format_document",
        "Format document",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "lsp.trigger_completion",
        "Trigger completion",
    );
    lines.push("".to_string());

    // ── Explorer ────────────────────────────────────────────────────────────
    lines.push("File Explorer".to_string());
    append_help_binding(&mut lines, bindings, "app.focus_explorer", "Focus explorer");
    append_help_binding(
        &mut lines,
        bindings,
        "explorer.toggle_or_open",
        "Open file / toggle dir",
    );
    append_help_binding(&mut lines, bindings, "explorer.create_file", "Create file");
    append_help_binding(
        &mut lines,
        bindings,
        "explorer.create_folder",
        "Create folder",
    );
    append_help_binding(&mut lines, bindings, "explorer.delete_node", "Delete");
    append_help_binding(&mut lines, bindings, "explorer.rename_full", "Rename");
    append_help_binding(
        &mut lines,
        bindings,
        "explorer.toggle_hidden",
        "Toggle hidden files",
    );
    lines.push("".to_string());

    // ── Terminal ────────────────────────────────────────────────────────────
    lines.push("Terminal".to_string());
    append_help_binding(
        &mut lines,
        bindings,
        "app.toggle_terminal",
        "Toggle terminal",
    );
    append_help_binding(&mut lines, bindings, "app.focus_terminal", "Focus terminal");
    append_help_binding(&mut lines, bindings, "terminal.tab_new", "New terminal tab");
    append_help_binding(
        &mut lines,
        bindings,
        "terminal.tab_close",
        "Close terminal tab",
    );
    lines.push("  tip: Ctrl+Q in terminal → terminal normal mode (navigate with hjkl)".to_string());
    lines.push("".to_string());

    // ── AI ──────────────────────────────────────────────────────────────────
    lines.push("AI Assistant".to_string());
    append_help_binding(&mut lines, bindings, "ai.chat_toggle", "Toggle AI chat");
    append_help_binding(&mut lines, bindings, "ai.chat_focus", "Focus AI chat");
    append_help_binding(
        &mut lines,
        bindings,
        "ai.chat_stop",
        "Stop AI chat generation",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "ai.accept_inline",
        "Accept inline suggestion",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "ai.accept_inline_word",
        "Accept inline suggestion word",
    );
    lines.push("".to_string());

    // ── Tools ───────────────────────────────────────────────────────────────
    lines.push("Tools & Misc".to_string());
    append_help_binding(&mut lines, bindings, "app.open_settings", "Open settings");
    append_help_binding(
        &mut lines,
        bindings,
        "app.open_theme_selector",
        "Theme selector",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "app.open_file_history",
        "File history",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "diagnostics.open_picker",
        "Diagnostics picker",
    );
    append_help_binding(&mut lines, bindings, "git.open_lazygit", "Open lazygit");
    append_help_binding(&mut lines, bindings, "git.blame_line", "Git blame line");
    append_help_binding(
        &mut lines,
        bindings,
        "app.toggle_markdown_preview",
        "Markdown preview",
    );
    append_help_binding(
        &mut lines,
        bindings,
        "app.focus_markdown_preview",
        "Focus markdown preview",
    );
    append_help_binding(&mut lines, bindings, "editor.leap_start", "Leap jump");

    lines
}

fn command_label_for_help(command_id: &str) -> String {
    let label = match command_id {
        // ── File & app ────────────────────────────────────────────────────
        "editor.save_file" => "Save file",
        "editor.open_folder" => "Open folder / project",
        "editor.open_file" => "Open file",
        "app.new_instance" => "New instance",
        "app.open_command_palette" => "Command palette",
        "app.open_settings" => "Open settings",
        "app.open_help" => "Open cheat sheet",
        "app.open_vim_command" => "Vim command line",
        "app.open_file_picker" => "Open file picker",
        "app.open_file_finder" => "File finder (center)",
        "app.open_file_history" => "File history",
        "app.open_theme_selector" => "Theme selector",
        "app.search_in_files" => "Search in files",
        "app.open_workspace_symbols" => "Workspace symbols",
        "app.open_document_symbols" => "Find symbol in file",
        "app.toggle_markdown_preview" => "Toggle markdown preview",
        "app.close_sidebars" => "Close sidebars",
        "app.focus_markdown_preview" => "Focus markdown preview",
        // ── Focus & docks ─────────────────────────────────────────────────
        "app.focus_explorer" => "Focus explorer",
        "app.focus_terminal" => "Focus terminal",
        "app.focus_editor" => "Focus editor",
        "app.focus_inspector" => "Focus inspector",
        "app.focus_back" => "Focus back to editor",
        "app.focus_left" => "Focus left",
        "app.focus_right" => "Focus right",
        "app.focus_up" => "Focus up",
        "app.focus_down" => "Focus down",
        "app.move_focus_cycle" => "Cycle focus",
        "app.toggle_left_dock" => "Toggle left dock",
        "app.toggle_bottom_dock" => "Toggle bottom dock",
        "app.next_panel_tab" => "Next panel tab",
        "app.prev_panel_tab" => "Prev panel tab",
        // ── Mode transitions ──────────────────────────────────────────────
        "mode.enter_normal" => "→ Normal mode",
        "mode.enter_insert" => "Insert mode",
        "mode.enter_visual" => "→ Visual mode",
        "mode.enter_visual_line" => "→ Visual line",
        "mode.enter_terminal_focus" => "→ Terminal focus",
        // ── Movement ──────────────────────────────────────────────────────
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
        "editor.move_paragraph_up" => "Paragraph up",
        "editor.move_paragraph_down" => "Paragraph down",
        "editor.scroll_half_page_up" => "½ page up",
        "editor.scroll_half_page_down" => "½ page down",
        "editor.center_cursor_line" => "Center cursor line",
        // ── Search ────────────────────────────────────────────────────────
        "editor.search_next" => "Next match",
        "editor.search_prev" => "Prev match",
        "editor.search_word_under_cursor" => "Search word under cursor",
        "editor.clear_search_highlights" => "Clear highlights",
        "editor.open_in_file_search" => "Search in file",
        // ── Editing ───────────────────────────────────────────────────────
        "editor.insert_char" => "Insert character",
        "editor.delete_char" => "Delete char",
        "editor.delete_current_line" => "Delete line",
        "editor.delete_to_line_end" => "Delete to line end",
        "editor.delete_word_forward" => "Delete word →",
        "editor.delete_word_backward" => "Delete word ←",
        "editor.delete_selection" => "Delete selection",
        "editor.change_word_forward" => "Change word →",
        "editor.change_word_backward" => "Change word ←",
        "editor.change_to_line_end" => "Change to line end",
        "editor.change_selection" => "Change selection",
        "editor.substitute_line" => "Substitute line",
        "editor.yank_selection" => "Yank selection",
        "editor.yank_current_line" => "Yank line",
        "editor.yank_to_line_end" => "Yank to line end",
        "editor.toggle_line_comment" => "Toggle line comment",
        "editor.toggle_selection_comment" => "Toggle comment",
        "editor.wrap_selection_with_star" => "Wrap with *",
        "editor.join_lines" => "Join lines",
        "editor.paste" => "Paste",
        "editor.paste_after" => "Paste after",
        "editor.paste_before" => "Paste before",
        "editor.visual_paste" => "Visual paste",
        "editor.undo" => "Undo",
        "editor.redo" => "Redo",
        "editor.replace_char" => "Replace char",
        "editor.insert_tab" => "Insert tab",
        "editor.newline" => "New line",
        "editor.backspace" => "Backspace",
        "editor.append_after_cursor" => "Append after cursor",
        "editor.append_at_line_end" => "Append at line end",
        "editor.insert_at_line_start" => "Insert at line start",
        "editor.insert_line_below" => "New line below",
        "editor.insert_line_above" => "New line above",
        "editor.insert_newline" => "Insert newline",
        // ── Completion ────────────────────────────────────────────────────
        "completion.next" => "Select next",
        "completion.prev" => "Select previous",
        "completion.accept" => "Accept completion",
        "completion.close" => "Close completion",
        // ── Buffer ────────────────────────────────────────────────────────
        "buffer.new" => "New scratch buffer",
        "buffer.next" => "Next buffer",
        "buffer.prev" => "Prev buffer",
        "buffer.close_current" => "Close current buffer",
        "buffer.goto_1" => "Go to buffer 1",
        "buffer.goto_2" => "Go to buffer 2",
        "buffer.goto_3" => "Go to buffer 3",
        "buffer.goto_4" => "Go to buffer 4",
        "buffer.goto_5" => "Go to buffer 5",
        "buffer.goto_6" => "Go to buffer 6",
        "buffer.goto_7" => "Go to buffer 7",
        "buffer.goto_8" => "Go to buffer 8",
        "buffer.goto_9" => "Go to buffer 9",
        // ── LSP ───────────────────────────────────────────────────────────
        "lsp.hover" => "Hover docs",
        "lsp.go_to_definition" => "Go to definition",
        "lsp.preview_definition" => "Preview definition",
        "lsp.references" => "References",
        "lsp.rename" => "Rename symbol",
        "lsp.format_document" => "Format document",
        "lsp.trigger_completion" => "Trigger completion",
        // ── Explorer ──────────────────────────────────────────────────────
        "explorer.move_up" => "Move up",
        "explorer.move_down" => "Move down",
        "explorer.collapse_or_parent" => "Collapse / parent",
        "explorer.expand_node" => "Expand node",
        "explorer.expand_or_child" => "Expand / child",
        "explorer.toggle_or_open" => "Toggle / open",
        "explorer.delete_node" => "Delete node",
        "explorer.create_file" => "Create file",
        "explorer.create_folder" => "Create folder",
        "explorer.rename_full" => "Rename (full)",
        "explorer.rename_base" => "Rename (base)",
        "explorer.toggle_hidden" => "Toggle hidden",
        "explorer.toggle_ignored" => "Toggle ignored",
        "explorer.move_to_top" => "Move to top",
        "explorer.move_to_bottom" => "Move to bottom",
        "explorer.collapse_node" => "Collapse node",
        "explorer.collapse_all_under_node" => "Collapse all",
        "explorer.expand_all_under_node" => "Expand all",
        "explorer.start_filter" => "Start filter",
        "explorer.clear_filter" => "Clear filter",
        // ── Terminal ──────────────────────────────────────────────────────
        "app.toggle_terminal" => "Toggle terminal",
        "terminal.paste" => "Terminal paste",
        "terminal.enter_normal_mode" => "Enter terminal normal",
        "terminal.search_open" => "Search in terminal",
        "terminal.tab_new" => "New terminal tab",
        "terminal.tab_close" => "Close terminal tab",
        "terminal.tab_switch_1" => "Terminal tab 1",
        "terminal.tab_switch_2" => "Terminal tab 2",
        "terminal.tab_switch_3" => "Terminal tab 3",
        "terminal.tab_switch_4" => "Terminal tab 4",
        "terminal.tab_switch_5" => "Terminal tab 5",
        "terminal.tab_switch_6" => "Terminal tab 6",
        "terminal.tab_switch_7" => "Terminal tab 7",
        "terminal.tab_switch_8" => "Terminal tab 8",
        "terminal.tab_switch_9" => "Terminal tab 9",
        // ── Git ───────────────────────────────────────────────────────────
        "git.open_lazygit" => "Open lazygit",
        "git.open_lazydocker" => "Open lazydocker",
        "git.blame_line" => "Git blame line",
        // ── Diagnostics ───────────────────────────────────────────────────
        "diagnostics.open_picker" => "Diagnostics picker",
        // ── AI ────────────────────────────────────────────────────────────
        "ai.accept_inline" => "Accept AI suggestion",
        "ai.accept_inline_word" => "Accept AI suggestion word",
        "ai.chat_toggle" => "Toggle AI chat",
        "ai.chat_send" => "Send AI message",
        "ai.chat_stop" => "Stop AI chat generation",
        "ai.chat_focus" => "Focus AI chat",
        "ai.chat_close" => "Close AI chat",
        "ai.chat_add_selection_context" => "Add selection to AI",
        // ── Leap ──────────────────────────────────────────────────────────
        "editor.leap_start" => "Leap jump",
        // ── Jump list ─────────────────────────────────────────────────────
        "editor.jump_back" => "Jump back",
        "editor.jump_forward" => "Jump forward",
        // ── Multi-cursor ──────────────────────────────────────────────────
        "multicursor.add_next" => "Add next match",
        "multicursor.skip" => "Skip match",
        "multicursor.insert_before" => "Insert before cursors",
        "multicursor.append_after" => "Append after cursors",
        "multicursor.change" => "Change at cursors",
        "multicursor.delete" => "Delete at cursors",
        // ── Overlay / palette ─────────────────────────────────────────────
        "overlay.select_prev" => "Select previous",
        "overlay.select_next" => "Select next",
        // ── Misc ──────────────────────────────────────────────────────────
        "app.toggle_maximize_focus" => "Toggle maximize focus",
        "projects.recent" => "Recent projects",
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
pub enum CompletionItemSource {
    Lsp,
    WorkspaceSymbol,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionDisplayItem {
    pub item: LspCompletionItem,
    pub match_ranges: Vec<(usize, usize)>,
    pub score: i64,
    pub source: CompletionItemSource,
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
    pub prefix_col: usize,
    /// Hover documentation fetched on-demand for the selected item.
    pub hover_doc: Option<String>,
    /// `true` once we've received a final answer (success, failure, or "skip — already inline")
    /// so the UI can stop showing the "Loading…" hint.
    pub hover_doc_resolved: bool,
    /// Monotonic counter bumped on every selection change. Each in-flight
    /// `completionItem/resolve` (or fallback hover) carries the revision it
    /// was issued for; results whose revision != `current_revision` on arrival
    /// are silently dropped so a slow/late doc never lands on a newer item.
    pub current_revision: u64,
    /// Cached full completion items (LSP + workspace symbols) for client-side incremental filtering.
    /// When the user types more characters, we filter this cache instead of re-requesting from LSP.
    pub cached_full_items: Vec<CompletionDisplayItem>,
    /// Language ID for checking indexing status
    pub language_id: Option<String>,
}

impl CompletionState {
    pub fn from_lsp_items(
        items: Vec<LspCompletionItem>,
        anchor_line: usize,
        anchor_col: usize,
        prefix_start_col: usize,
        prefix: String,
        cache: &crate::lsp::WorkspaceSymbolCache,
        language_id: Option<&str>,
    ) -> Self {
        let full_items = overlays::build_completion_display_items_with_cache(
            &items,
            &prefix,
            cache,
            language_id,
            5, // Minimum items before querying workspace symbols
        );

        Self {
            raw_items: items,
            filtered_items: full_items.clone(),
            selected_index: 0,
            typed_prefix: prefix,
            trigger_pos: CompletionTriggerPosition {
                line: anchor_line,
                col: prefix_start_col,
            },
            anchor_line,
            anchor_col,
            prefix_col: prefix_start_col,
            hover_doc: None,
            hover_doc_resolved: false,
            current_revision: 0,
            cached_full_items: full_items,
            language_id: language_id.map(|s| s.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FuzzyState {
    pub mode: CommandPaletteMode,
    pub query: String,
    pub selected_index: usize,
    pub source_file_path: Option<PathBuf>,
    pub preview_lines: Vec<FilePreviewLine>,
    pub preview_text: String,
    pub preview_spans: Vec<StyledTextSpan>,
    pub results: Vec<CommandPaletteItem>,
    pub live_grep_case_sensitive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHistoryEntrySummary {
    pub index: usize,
    pub label: String,
    pub secondary_label: Option<String>,
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
struct PendingTransaction {
    before_text: Rope,
    before_cursor: CursorState,
}

#[derive(Debug, Clone)]
struct FileHistoryPreviewSession {
    baseline_view: EditorViewSnapshot,
    baseline_history: EditHistory,
    preview_index: Option<usize>,
    preview_view: Option<EditorViewSnapshot>,
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
            live_grep_case_sensitive: false,
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
    MarkdownPreview(MarkdownPreviewState),
    FuzzyPicker(FuzzyState),
    SettingsTab(SettingsState),
    Help(HelpState),
    ExtensionsManager(ExtensionsManagerState),
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
            BufferContent::MarkdownPreview(state) => state
                .source_path
                .as_ref()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .map(|name| format!("{name} preview"))
                .unwrap_or_else(|| "[Markdown Preview]".to_string()),
            BufferContent::FuzzyPicker(_) => "[Fuzzy Finder]".to_string(),
            BufferContent::SettingsTab(_) => "[Settings]".to_string(),
            BufferContent::Help(state) => state.title.clone(),
            BufferContent::ExtensionsManager(state) => format!("[{}]", state.title),
        }
    }

    pub fn is_dirty(&self, is_active: bool, active_editor_dirty: bool) -> bool {
        match &self.content {
            BufferContent::Text(buffer) => {
                if is_active {
                    active_editor_dirty
                } else {
                    buffer.dirty
                }
            }
            BufferContent::Image(_)
            | BufferContent::Terminal(_)
            | BufferContent::References(_)
            | BufferContent::Diagnostics(_)
            | BufferContent::MarkdownPreview(_)
            | BufferContent::FuzzyPicker(_)
            | BufferContent::SettingsTab(_)
            | BufferContent::Help(_)
            | BufferContent::ExtensionsManager(_) => false,
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
pub struct FloatingBoxScrollState {
    pub offset_lines: usize,
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
        scroll: FloatingBoxScrollState,
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
    visual_block_anchor_line: Option<usize>,
    visual_block_anchor_col: Option<usize>,
    buffers: Vec<BufferEntry>,
    /// Session-only cache for closed text buffers. Reopen restores undo/redo and unsaved text.
    closed_text_buffers: HashMap<PathBuf, EditorBuffer>,
    active_buffer_index: Option<usize>,
    default_save_path: PathBuf,
    dirty: bool,
    pub target_scroll_y: f32,
    pub current_scroll_y: f32,
    pub scroll_column: usize,
    workspace_model: Option<WorkspaceModel>,
    pub(crate) command_palette: CommandPalette,
    file_picker_results_cache: Vec<FilePickerEntry>,
    last_search_query: String,
    search_highlights: Vec<(usize, usize)>,
    semantic_symbol_highlights: Vec<(usize, usize)>,
    search_whole_word: bool,
    search_case_sensitive: bool,
    live_grep_case_sensitive: bool,
    terminal_panel_open: bool,
    external_conflict: Option<String>,
    external_notice: Option<String>,
    clipboard_record: Option<ClipboardRecord>,
    history: EditHistory,
    current_transaction: Option<PendingTransaction>,
    file_history_preview: Option<FileHistoryPreviewSession>,
    pending_highlight_edits: Vec<HighlightEdit>,
    current_overlays: Vec<EditorOverlay>,
    completion: Option<CompletionState>,
    completion_loading: bool,
    inline_suggestion: Option<String>,
    jump_back_stack: Vec<(PathBuf, usize, usize)>,
    jump_forward_stack: Vec<(PathBuf, usize, usize)>,
    diagnostics: HashMap<PathBuf, Vec<LspDiagnostic>>,
    /// Latest `$/progress` snapshot, keyed by `(server, token)` so concurrent
    /// progress streams don't clobber each other. The status bar reads the
    /// most recently updated entry.
    lsp_progress: HashMap<(String, String), LspProgressEntry>,
    lsp_progress_active_key: Option<(String, String)>,
    pending_explorer_rename_path: Option<PathBuf>,
    indent_config: IndentConfig,
    is_initial_launch_welcome: bool,
    pub markdown_preview: MarkdownPreviewState,
    // ── MultiCursor state ──────────────────────────────────────────────────────
    virtual_cursors: Vec<VirtualCursor>,
    mc_search_word: Option<String>,
    mc_search_start: usize,
    /// When false (Visual-seeded search), use plain substring matching instead
    /// of whole-word matching for subsequent Ctrl+n calls.
    mc_whole_word: bool,
    // ── Code folding ──────────────────────────────────────────────────────────
    // Ranges are start/end inclusive. The start line remains visible as the
    // fold marker; every following line through end is hidden from layout.
    folded_ranges: Vec<(usize, usize)>,
    foldable_ranges_cache: Option<Vec<(usize, usize)>>,
    auto_folded_long_lines: Vec<usize>,
    // ── Performance: Line start position cache ────────────────────────────────
    /// Cached byte offsets for the start of each line. Invalidated on text edits.
    /// Eliminates O(n) rebuild on every highlight request for large files.
    cached_line_starts: Option<Vec<usize>>,
    // ── Workspace symbol cache ────────────────────────────────────────────────
    /// Pre-indexed workspace symbols for fast import suggestions.
    workspace_symbol_cache: Arc<crate::lsp::WorkspaceSymbolCache>,
}

impl AppState {
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
            visual_block_anchor_line: None,
            visual_block_anchor_col: None,
            buffers: Vec::new(),
            closed_text_buffers: HashMap::new(),
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
            search_case_sensitive: true,
            live_grep_case_sensitive: false,
            terminal_panel_open: false,
            external_conflict: None,
            external_notice: None,
            clipboard_record: None,
            history: EditHistory::new(),
            current_transaction: None,
            file_history_preview: None,
            pending_highlight_edits: Vec::new(),
            current_overlays: Vec::new(),
            completion: None,
            completion_loading: false,
            inline_suggestion: None,
            jump_back_stack: Vec::new(),
            jump_forward_stack: Vec::new(),
            diagnostics: HashMap::new(),
            lsp_progress: HashMap::new(),
            lsp_progress_active_key: None,
            pending_explorer_rename_path: None,
            indent_config: IndentConfig::default(),
            is_initial_launch_welcome: true,
            markdown_preview: MarkdownPreviewState::default(),
            semantic_symbol_highlights: Vec::new(),
            virtual_cursors: Vec::new(),
            mc_search_word: None,
            mc_search_start: 0,
            mc_whole_word: true,
            folded_ranges: Vec::new(),
            foldable_ranges_cache: None,
            auto_folded_long_lines: Vec::new(),
            cached_line_starts: None,
            workspace_symbol_cache: Arc::new(crate::lsp::WorkspaceSymbolCache::new()),
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
            visual_block_anchor_line: None,
            visual_block_anchor_col: None,
            buffers: Vec::new(),
            closed_text_buffers: HashMap::new(),
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
            search_case_sensitive: true,
            live_grep_case_sensitive: false,
            terminal_panel_open: false,
            external_conflict: None,
            external_notice: None,
            clipboard_record: None,
            history: EditHistory::new(),
            current_transaction: None,
            file_history_preview: None,
            pending_highlight_edits: Vec::new(),
            current_overlays: Vec::new(),
            completion: None,
            completion_loading: false,
            inline_suggestion: None,
            jump_back_stack: Vec::new(),
            jump_forward_stack: Vec::new(),
            diagnostics: HashMap::new(),
            lsp_progress: HashMap::new(),
            lsp_progress_active_key: None,
            pending_explorer_rename_path: None,
            indent_config: IndentConfig::default(),
            is_initial_launch_welcome: false,
            markdown_preview: MarkdownPreviewState::default(),
            semantic_symbol_highlights: Vec::new(),
            virtual_cursors: Vec::new(),
            mc_search_word: None,
            mc_search_start: 0,
            mc_whole_word: true,
            folded_ranges: Vec::new(),
            foldable_ranges_cache: None,
            auto_folded_long_lines: Vec::new(),
            cached_line_starts: None,
            workspace_symbol_cache: Arc::new(crate::lsp::WorkspaceSymbolCache::new()),
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

    // ── Code folding ──────────────────────────────────────────────────────────

    pub fn folded_ranges(&self) -> &[(usize, usize)] {
        &self.folded_ranges
    }

    /// Returns true when `line_idx` is hidden by a folded range.
    ///
    /// The marker/start line is intentionally not considered folded because it
    /// still participates in layout and cursor navigation.
    pub fn is_line_folded(&self, line_idx: usize) -> bool {
        self.folded_ranges
            .iter()
            .any(|&(s, e)| s != e && s < line_idx && line_idx <= e)
    }

    pub fn is_fold_marker_line(&self, line_idx: usize) -> bool {
        self.folded_ranges.iter().any(|&(s, _)| s == line_idx)
            || self.auto_folded_long_lines.contains(&line_idx)
    }

    pub fn is_auto_folded_long_line(&self, line_idx: usize) -> bool {
        self.auto_folded_long_lines.contains(&line_idx)
    }

    pub fn folded_line_count_at_marker(&self, line_idx: usize) -> Option<usize> {
        if self.auto_folded_long_lines.contains(&line_idx) {
            return Some(1);
        }
        self.folded_ranges
            .iter()
            .find(|&&(s, _)| s == line_idx)
            .map(|&(s, e)| e.saturating_sub(s))
    }

    pub fn fold_marker_line_for_hidden_line(&self, line_idx: usize) -> Option<usize> {
        self.folded_ranges
            .iter()
            .find(|&&(s, e)| s < line_idx && line_idx <= e)
            .map(|&(s, _)| s)
    }

    pub fn folded_visual_y_offset_before(&self, line_idx: usize, line_height: f32) -> f32 {
        self.folded_ranges
            .iter()
            .filter(|&&(_s, e)| e < line_idx)
            .map(|&(s, e)| e.saturating_sub(s) as f32 * line_height)
            .sum()
    }

    pub fn next_visible_line_after(&self, line_idx: usize) -> usize {
        let total = self.text.len_lines();
        if line_idx + 1 >= total {
            return line_idx;
        }

        let mut candidate = line_idx + 1;
        loop {
            let Some(&(_s, e)) = self
                .folded_ranges
                .iter()
                .find(|&&(s, e)| s < candidate && candidate <= e)
            else {
                break;
            };
            candidate = e.saturating_add(1);
            if candidate >= total {
                return line_idx;
            }
        }
        candidate
    }

    pub fn previous_visible_line_before(&self, line_idx: usize) -> usize {
        if line_idx == 0 {
            return line_idx;
        }

        let candidate = line_idx - 1;
        self.fold_marker_line_for_hidden_line(candidate)
            .unwrap_or(candidate)
    }

    pub fn set_foldable_ranges_cache(&mut self, ranges: Vec<(usize, usize)>) {
        self.foldable_ranges_cache = Some(ranges);
    }

    pub fn foldable_ranges_cache(&self) -> Option<&[(usize, usize)]> {
        self.foldable_ranges_cache.as_deref()
    }

    pub fn auto_fold_pathological_long_lines(&mut self) -> bool {
        const AUTO_FOLD_LINE_CHAR_THRESHOLD: usize = 200;
        let mut lines = Vec::new();
        for line_idx in 0..self.text.len_lines() {
            let line = self.text.line(line_idx);
            if line.len_chars() > AUTO_FOLD_LINE_CHAR_THRESHOLD {
                lines.push(line_idx);
            }
        }
        if lines == self.auto_folded_long_lines {
            return false;
        }

        self.folded_ranges
            .retain(|range| !self.auto_folded_long_lines.contains(&range.0) || range.0 != range.1);
        self.auto_folded_long_lines = lines;
        for &line_idx in &self.auto_folded_long_lines {
            if !self.folded_ranges.contains(&(line_idx, line_idx)) {
                self.folded_ranges.push((line_idx, line_idx));
            }
        }
        self.folded_ranges.sort_by_key(|&(start, _)| start);
        self.bump_revision();
        true
    }

    pub fn visible_line_count(&self) -> usize {
        let total = self.text.len_lines().max(1);
        let last_line = total.saturating_sub(1);
        let hidden: usize = self
            .folded_ranges
            .iter()
            .filter(|&&(s, _)| s < total)
            .map(|&(s, e)| e.min(last_line).saturating_sub(s))
            .sum();
        total.saturating_sub(hidden).max(1)
    }

    pub fn compute_visible_line_map(&self) -> Vec<usize> {
        let total = self.text.len_lines();
        let mut map = Vec::with_capacity(total);
        let mut logical = 0;
        for &(s, e) in &self.folded_ranges {
            while logical < s && logical < total {
                map.push(logical);
                logical += 1;
            }
            if s < total {
                map.push(s);
                logical = e.saturating_add(1);
            }
        }
        while logical < total {
            map.push(logical);
            logical += 1;
        }
        map
    }

    pub fn logical_to_visible_line(&self, logical: usize) -> Option<usize> {
        if self.is_line_folded(logical) {
            return None;
        }
        let map = self.compute_visible_line_map();
        map.iter().position(|&l| l == logical)
    }

    pub fn visible_to_logical_line(&self, visible: usize) -> usize {
        let map = self.compute_visible_line_map();
        map.get(visible).copied().unwrap_or(visible)
    }

    pub fn toggle_fold_at_line(&mut self, logical_line: usize) -> bool {
        if let Some(pos) = self
            .auto_folded_long_lines
            .iter()
            .position(|&line| line == logical_line)
        {
            self.auto_folded_long_lines.remove(pos);
            self.folded_ranges
                .retain(|&range| range != (logical_line, logical_line));
            self.bump_revision();
            return true;
        }

        if let Some(pos) = self
            .folded_ranges
            .iter()
            .position(|&(s, e)| s == logical_line || (s < logical_line && logical_line <= e))
        {
            self.folded_ranges.remove(pos);
            self.bump_revision();
            return true;
        }

        let cache = match self.foldable_ranges_cache.as_ref() {
            Some(c) => c.clone(),
            None => return false,
        };

        let mut best_match: Option<(usize, usize)> = None;
        for &(s, e) in &cache {
            if s <= logical_line && logical_line <= e {
                if let Some((_, best_e)) = best_match {
                    if e < best_e {
                        best_match = Some((s, e));
                    }
                } else {
                    best_match = Some((s, e));
                }
            }
        }

        if let Some((s, e)) = best_match {
            let overlaps = self
                .folded_ranges
                .iter()
                .any(|&(fs, fe)| s <= fe && fs <= e);
            if overlaps {
                return false;
            }

            self.folded_ranges.push((s, e));
            self.folded_ranges.sort_by_key(|&(start, _)| start);
            self.folded_ranges = merge_fold_ranges(&self.folded_ranges);

            if let Some(marker_line) =
                self.fold_marker_line_for_hidden_line(self.cursor_line_col().0)
            {
                let line_start = self.text.line_to_char(marker_line);
                self.cursor_char_idx = line_start;
                let (_, col) = self.cursor_line_col();
                self.target_col = col;
            }
            self.bump_revision();
            return true;
        }

        false
    }

    pub fn toggle_fold_all(&mut self) -> bool {
        if !self.folded_ranges.is_empty() {
            return self.unfold_all();
        }
        self.fold_all()
    }

    pub fn unfold_all(&mut self) -> bool {
        if self.folded_ranges.is_empty() {
            return false;
        }
        self.folded_ranges.clear();
        self.bump_revision();
        true
    }

    pub fn fold_all(&mut self) -> bool {
        let cache = match self.foldable_ranges_cache.as_ref() {
            Some(c) if !c.is_empty() => c.clone(),
            _ => return false,
        };

        let mut new_ranges: Vec<(usize, usize)> = cache.into_iter().collect();
        new_ranges.sort_by_key(|&(start, _)| start);
        self.folded_ranges = merge_fold_ranges(&new_ranges);
        let (cursor_line, _) = self.cursor_line_col();
        if let Some(marker_line) = self.fold_marker_line_for_hidden_line(cursor_line) {
            let line_start = self.text.line_to_char(marker_line);
            self.cursor_char_idx = line_start;
            let (_, col) = self.cursor_line_col();
            self.target_col = col;
        }
        self.bump_revision();
        true
    }
}

fn merge_fold_ranges(ranges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    if ranges.is_empty() {
        return Vec::new();
    }

    let mut regular: Vec<(usize, usize)> =
        ranges.iter().copied().filter(|&(s, e)| s != e).collect();
    let mut point_folds: Vec<(usize, usize)> =
        ranges.iter().copied().filter(|&(s, e)| s == e).collect();
    regular.sort_by_key(|&(s, _)| s);
    point_folds.sort_by_key(|&(s, _)| s);
    point_folds.dedup();

    let mut merged: Vec<(usize, usize)> = Vec::new();
    if let Some(first) = regular.first().copied() {
        let mut current = first;
        for &(s, e) in &regular[1..] {
            if s <= current.1 {
                current.1 = current.1.max(e);
            } else {
                merged.push(current);
                current = (s, e);
            }
        }
        merged.push(current);
    }

    merged.extend(point_folds);
    merged.sort_by_key(|&(start, _)| start);
    merged
}
