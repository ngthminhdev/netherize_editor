use std::path::PathBuf;

use crate::{
    config::theme_config::ThemeConfig,
    workspace::{
        fuzzy::{WorkspaceMatch, find_file_matches},
        model::WorkspaceModel,
    },
};

const DEFAULT_MAX_RESULTS: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandPaletteMode {
    FilePicker,
    CommandPalette,
    VimCommand,
    WorkspaceSymbols,
}

impl CommandPaletteMode {
    pub fn prompt_prefix(self) -> &'static str {
        match self {
            Self::FilePicker => "find> ",
            Self::CommandPalette => "> ",
            Self::VimCommand => ":",
            Self::WorkspaceSymbols => "@ ",
        }
    }

    pub fn empty_hint(self) -> &'static str {
        match self {
            Self::FilePicker => "type to search files...",
            Self::CommandPalette => "type a command...",
            Self::VimCommand => "type a vim command...",
            Self::WorkspaceSymbols => "type to search symbols...",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::FilePicker => "File Picker",
            Self::CommandPalette => "Command Palette",
            Self::VimCommand => "Vim Command",
            Self::WorkspaceSymbols => "Workspace Symbols",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandPaletteAction {
    OpenFile(PathBuf),
    ExecuteCommand(String),
    ExecuteVimCommand(String),
    JumpToSymbol(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPaletteItem {
    pub label: String,
    pub action: CommandPaletteAction,
}

impl CommandPaletteItem {
    pub fn file_match(relative_path: String, absolute_path: PathBuf) -> Self {
        Self {
            label: relative_path,
            action: CommandPaletteAction::OpenFile(absolute_path),
        }
    }

    pub fn command(id: &str, label: &str) -> Self {
        Self {
            label: label.to_string(),
            action: CommandPaletteAction::ExecuteCommand(id.to_string()),
        }
    }

    pub fn symbol(name: &str) -> Self {
        Self {
            label: name.to_string(),
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
            action: CommandPaletteAction::ExecuteVimCommand(trimmed.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandPaletteRenderModel {
    pub overlay_bounds: [f32; 4],
    pub panel_bounds: [f32; 4],
    pub prompt_line: String,
    pub result_labels: Vec<String>,
    pub selected_index: usize,
    pub line_height: f32,
    pub panel_padding: f32,
    pub title: String,
    pub border_color: [f32; 4],
    pub panel_bg: [f32; 4],
    pub selection_bg: [f32; 4],
    pub text_color: [f32; 4],
    pub hint_color: [f32; 4],
    pub scrim_color: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct CommandPalette {
    pub mode: CommandPaletteMode,
    pub query: String,
    pub results: Vec<CommandPaletteItem>,
    pub selected_index: usize,
    pub is_visible: bool,
    max_results: usize,
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
        }
    }
}

impl CommandPalette {
    pub fn open(&mut self, mode: CommandPaletteMode, workspace: Option<&WorkspaceModel>) -> usize {
        self.mode = mode;
        self.query.clear();
        self.selected_index = 0;
        self.is_visible = true;
        self.refresh_results(workspace);
        self.results.len()
    }

    pub fn close(&mut self) -> bool {
        let was_open = self.is_visible;
        self.is_visible = false;
        self.query.clear();
        self.selected_index = 0;
        self.results.clear();
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
        let next = (self.selected_index + 1).min(self.results.len() - 1);
        let changed = next != self.selected_index;
        self.selected_index = next;
        changed
    }

    pub fn select_prev(&mut self) -> bool {
        if self.results.is_empty() {
            return false;
        }
        let prev = self.selected_index.saturating_sub(1);
        let changed = prev != self.selected_index;
        self.selected_index = prev;
        changed
    }

    pub fn selected_action(&self) -> Option<CommandPaletteAction> {
        self.results
            .get(self.selected_index)
            .map(|entry| entry.action.clone())
    }

    pub fn refresh_results(&mut self, workspace: Option<&WorkspaceModel>) {
        self.results = match self.mode {
            CommandPaletteMode::FilePicker => workspace
                .map(|ws| {
                    find_file_matches(ws, &self.query, self.max_results)
                        .into_iter()
                        .map(workspace_match_to_item)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            CommandPaletteMode::CommandPalette => {
                command_palette_items(&self.query, self.max_results)
            }
            CommandPaletteMode::VimCommand => vim_command_items(&self.query),
            CommandPaletteMode::WorkspaceSymbols => {
                workspace_symbol_items(&self.query, self.max_results)
            }
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
        let line_height = theme.ui.sidebar_line_height.max(20.0);
        let visible_rows = self.results.len().min(12);
        let panel_width = (width * 0.72).clamp(420.0, 980.0).min(width.max(1.0));
        let panel_height = (line_height * (3 + visible_rows) as f32 + panel_padding * 2.0)
            .min((height - 32.0).max(line_height * 4.0));
        let panel_x = x + ((width - panel_width) * 0.5).max(0.0);
        let panel_y = y + (height * 0.14).max(18.0);
        let panel_bounds = [panel_x, panel_y, panel_width, panel_height];

        let prompt = if self.query.is_empty() {
            format!("{}{}", self.mode.prompt_prefix(), self.mode.empty_hint())
        } else {
            format!("{}{}", self.mode.prompt_prefix(), self.query)
        };

        let mut scrim = theme.editor.bg.as_f32();
        scrim[3] = 0.72;

        Some(CommandPaletteRenderModel {
            overlay_bounds,
            panel_bounds,
            prompt_line: prompt,
            result_labels: self
                .results
                .iter()
                .map(|entry| entry.label.clone())
                .collect(),
            selected_index: self.selected_index,
            line_height,
            panel_padding,
            title: self.mode.title().to_string(),
            border_color: theme.ui.border_color.as_f32(),
            panel_bg: theme.ui.panel_bg.as_f32(),
            selection_bg: theme.ui.selection_bg.as_f32(),
            text_color: theme.editor.fg.as_f32(),
            hint_color: theme.syntax.comment.as_f32(),
            scrim_color: scrim,
        })
    }
}

fn workspace_match_to_item(found: WorkspaceMatch) -> CommandPaletteItem {
    CommandPaletteItem::file_match(found.relative_path, found.absolute_path)
}

fn command_palette_items(query: &str, max_results: usize) -> Vec<CommandPaletteItem> {
    let candidates = [
        ("editor.undo", "Undo"),
        ("editor.redo", "Redo"),
        ("editor.save_file", "Save File"),
        ("app.open_file_picker", "Open File Picker"),
        ("app.open_file_finder", "Open File Finder"),
        ("app.search_in_files", "Search In Files"),
        ("app.open_workspace_symbols", "Open Workspace Symbols"),
        ("app.open_vim_command", "Open Vim Command"),
        ("app.toggle_terminal", "Toggle Terminal"),
        ("app.toggle_explorer", "Toggle Explorer"),
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
    if query.trim().is_empty() {
        return vec![
            CommandPaletteItem::vim_input("w"),
            CommandPaletteItem::vim_input("q"),
            CommandPaletteItem::vim_input("wq"),
            CommandPaletteItem::vim_input("enew"),
            CommandPaletteItem::vim_input("bn"),
            CommandPaletteItem::vim_input("bp"),
            CommandPaletteItem::vim_input("bd"),
            CommandPaletteItem::vim_input("e src/main.rs"),
        ];
    }
    vec![CommandPaletteItem::vim_input(query)]
}
