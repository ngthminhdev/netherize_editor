use std::fs;

use super::*;

pub(super) fn syntax_spans_to_styled(
    spans: &[HighlightSpan],
    theme: &ThemeConfig,
) -> Vec<StyledTextSpan> {
    spans
        .iter()
        .map(|span| {
            let color = match span.category {
                crate::syntax::highlight::HighlightCategory::Keyword => {
                    theme.syntax.keyword.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::String => theme.syntax.string.as_u8(),
                crate::syntax::highlight::HighlightCategory::Comment => {
                    theme.syntax.comment.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Type => theme.syntax.r#type.as_u8(),
                crate::syntax::highlight::HighlightCategory::Function => {
                    theme.syntax.function.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Number => theme.syntax.number.as_u8(),
            };
            StyledTextSpan::new(span.range.start, span.range.end, color)
        })
        .collect()
}

pub(super) fn scale_theme(base: &ThemeConfig, scale: f32) -> ThemeConfig {
    let mut theme = base.clone();
    theme.editor.font_size = scale_metric(theme.editor.font_size, scale, 8.0);
    theme.editor.line_height = scale_metric(theme.editor.line_height, scale, 12.0);
    theme.ui.sidebar_font_size = scale_metric(theme.ui.sidebar_font_size, scale, 8.0);
    theme.ui.sidebar_line_height = scale_metric(theme.ui.sidebar_line_height, scale, 12.0);
    theme.ui.panel_font_size = scale_metric(theme.ui.panel_font_size, scale, 8.0);
    theme.ui.panel_line_height = scale_metric(theme.ui.panel_line_height, scale, 12.0);
    theme.ui.sidebar_width = scale_metric(theme.ui.sidebar_width, scale, 120.0);
    theme.ui.right_sidebar_width = scale_metric(theme.ui.right_sidebar_width, scale, 120.0);
    theme.ui.bottom_panel_height = scale_metric(theme.ui.bottom_panel_height, scale, 80.0);
    theme.ui.top_bar_height = scale_metric(theme.ui.top_bar_height, scale, 22.0);
    theme.ui.status_bar_height = scale_metric(theme.ui.status_bar_height, scale, 18.0);
    theme
}

pub(super) fn scale_ui_config(base: &UiConfig, scale: f32) -> UiConfig {
    let mut ui = base.clone();
    ui.layout.region_gap = scale_metric(ui.layout.region_gap, scale, 1.0);
    ui.layout.top_bar_height = scale_metric(ui.layout.top_bar_height, scale, 20.0);
    ui.layout.status_bar_height = scale_metric(ui.layout.status_bar_height, scale, 18.0);
    ui.layout.center_min_width = scale_metric(ui.layout.center_min_width, scale, 240.0);
    ui.layout.center_min_height = scale_metric(ui.layout.center_min_height, scale, 120.0);
    ui.layout.sidebar_min_width = scale_metric(ui.layout.sidebar_min_width, scale, 140.0);
    ui.layout.bottom_min_height = scale_metric(ui.layout.bottom_min_height, scale, 80.0);

    ui.docks.left.size_px = scale_metric(ui.docks.left.size_px, scale, 120.0);
    ui.docks.right.size_px = scale_metric(ui.docks.right.size_px, scale, 120.0);
    ui.docks.bottom.size_px = scale_metric(ui.docks.bottom.size_px, scale, 80.0);

    ui.cursor.beam_width = scale_metric(ui.cursor.beam_width, scale, 1.0);
    ui.cursor.block_width = scale_metric(ui.cursor.block_width, scale, 6.0);
    ui.cursor.underline_height = scale_metric(ui.cursor.underline_height, scale, 1.0);

    ui.spacing.editor_padding = scale_metric(ui.spacing.editor_padding, scale, 4.0);
    ui.spacing.panel_padding = scale_metric(ui.spacing.panel_padding, scale, 4.0);
    ui.spacing.explorer_padding = scale_metric(ui.spacing.explorer_padding, scale, 4.0);

    ui.status_bar.padding_x = scale_metric(ui.status_bar.padding_x, scale, 4.0);
    ui.status_bar.font_size = scale_metric(ui.status_bar.font_size, scale, 8.0);
    ui.status_bar.line_height = scale_metric(ui.status_bar.line_height, scale, 12.0);
    ui
}

fn scale_metric(value: f32, scale: f32, min: f32) -> f32 {
    (value * scale).max(min)
}

pub(super) fn collect_explorer_entries(
    app_state: &AppState,
    expanded: &HashSet<PathBuf>,
) -> Vec<ExplorerEntry> {
    let Some(nodes) = app_state.workspace_nodes() else {
        return Vec::new();
    };
    let root = match app_state.workspace_root_path() {
        Some(root) => root.to_path_buf(),
        None => return Vec::new(),
    };

    let mut node_types: HashMap<PathBuf, WorkspaceNodeType> = HashMap::new();
    let mut children_map: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();

    for node in nodes {
        node_types.insert(node.path.clone(), node.file_type);
    }

    for node in nodes.iter() {
        if node.path == root {
            continue;
        }
        let Some(parent) = node.path.parent() else {
            continue;
        };
        if !parent.starts_with(&root) {
            continue;
        }
        children_map
            .entry(parent.to_path_buf())
            .or_default()
            .push(node.path.clone());
    }

    for children in children_map.values_mut() {
        children.sort_by(|left, right| {
            let left_type = node_types
                .get(left)
                .copied()
                .unwrap_or(WorkspaceNodeType::File);
            let right_type = node_types
                .get(right)
                .copied()
                .unwrap_or(WorkspaceNodeType::File);
            let left_rank = if left_type == WorkspaceNodeType::Folder {
                0
            } else {
                1
            };
            let right_rank = if right_type == WorkspaceNodeType::Folder {
                0
            } else {
                1
            };
            left_rank.cmp(&right_rank).then_with(|| left.cmp(right))
        });
    }

    let mut entries = Vec::new();
    collect_visible_explorer_entries(&root, 0, &node_types, &children_map, expanded, &mut entries);
    entries
}

fn collect_visible_explorer_entries(
    parent: &Path,
    depth: usize,
    node_types: &HashMap<PathBuf, WorkspaceNodeType>,
    children_map: &HashMap<PathBuf, Vec<PathBuf>>,
    expanded: &HashSet<PathBuf>,
    out: &mut Vec<ExplorerEntry>,
) {
    let Some(children) = children_map.get(parent) else {
        return;
    };

    for child in children {
        let file_type = node_types
            .get(child)
            .copied()
            .unwrap_or(WorkspaceNodeType::File);
        let is_expanded = file_type == WorkspaceNodeType::Folder && expanded.contains(child);
        let name = child
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("?");

        out.push(ExplorerEntry {
            path: child.clone(),
            parent_path: Some(parent.to_path_buf()),
            file_type,
            depth,
            is_expanded,
            name: name.to_string(),
        });

        if file_type == WorkspaceNodeType::Folder && is_expanded {
            collect_visible_explorer_entries(
                child,
                depth + 1,
                node_types,
                children_map,
                expanded,
                out,
            );
        }
    }
}

pub(super) fn build_sidebar_rows(
    entries: &[ExplorerEntry],
    selected_idx: usize,
) -> Vec<SidebarRow> {
    if entries.is_empty() {
        return vec![SidebarRow {
            depth: 0,
            icon: "·",
            label: "(no files)".to_string(),
            is_selected: false,
        }];
    }

    let selected = selected_idx.min(entries.len().saturating_sub(1));
    entries
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            let icon = match entry.file_type {
                WorkspaceNodeType::Folder => {
                    if entry.is_expanded {
                        "▼"
                    } else {
                        "▶"
                    }
                }
                WorkspaceNodeType::File => "·",
            };
            SidebarRow {
                depth: entry.depth,
                icon,
                label: entry.name.clone(),
                is_selected: idx == selected,
            }
        })
        .collect()
}

pub(super) fn region_color(id: RegionId, theme: &ThemeConfig) -> [f32; 4] {
    match id {
        RegionId::TopBar => theme.ui.panel_bg.as_f32(),
        RegionId::LeftSidebar => theme.ui.sidebar_bg.as_f32(),
        RegionId::Center => theme.editor.bg.as_f32(),
        RegionId::RightSidebar => theme.ui.sidebar_bg.as_f32(),
        RegionId::BottomPanel => theme.ui.panel_bg.as_f32(),
        RegionId::StatusBar => theme.ui.status_bar_bg.as_f32(),
        _ => theme.ui.border_color.as_f32(),
    }
}

pub(super) fn language_id_for_path(path: &Path) -> String {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("rs") => "rust",
        Some("js") | Some("mjs") | Some("cjs") => "javascript",
        Some("ts") | Some("tsx") => "typescript",
        Some("jsx") => "javascriptreact",
        Some("py") => "python",
        Some("go") => "go",
        Some("json") => "json",
        Some("toml") => "toml",
        Some("md") => "markdown",
        _ => "plaintext",
    }
    .to_string()
}

pub(super) fn detect_git_branch(root: &Path) -> Option<String> {
    let git_dir = find_git_dir(root)?;
    let head = fs::read_to_string(git_dir.join("HEAD")).ok()?;
    parse_git_head(head.trim())
}

fn find_git_dir(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        let dot_git = dir.join(".git");
        if dot_git.is_dir() {
            return Some(dot_git);
        }
        if dot_git.is_file() {
            let raw = fs::read_to_string(&dot_git).ok()?;
            let gitdir = raw.trim().strip_prefix("gitdir:")?.trim();
            let gitdir_path = PathBuf::from(gitdir);
            return Some(if gitdir_path.is_absolute() {
                gitdir_path
            } else {
                dir.join(gitdir_path)
            });
        }
    }
    None
}

fn parse_git_head(head: &str) -> Option<String> {
    if let Some(reference) = head.strip_prefix("ref:") {
        return reference
            .trim()
            .rsplit('/')
            .next()
            .map(str::to_string)
            .filter(|branch| !branch.is_empty());
    }

    (!head.is_empty()).then(|| {
        let short_len = head.len().min(7);
        format!("detached: {}", &head[..short_len])
    })
}
