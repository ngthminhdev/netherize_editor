use std::fs;
use std::path::Path;

use super::*;

use crate::{
    app::app_state::FloatingBoxBlock,
    async_runtime::message::FilePreviewLine,
    syntax::highlight::highlight_snippet,
};

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
                crate::syntax::highlight::HighlightCategory::Identifier => {
                    theme.syntax.identifier.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Parameter => {
                    theme.syntax.parameter.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Field => theme.syntax.field.as_u8(),
                crate::syntax::highlight::HighlightCategory::Property => {
                    theme.syntax.property.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Constant => {
                    theme.syntax.constant.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Operator => {
                    theme.syntax.operator.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Punctuation => {
                    theme.syntax.punctuation.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Macro => theme.syntax.r#macro.as_u8(),
                crate::syntax::highlight::HighlightCategory::Lifetime => {
                    theme.syntax.lifetime.as_u8()
                }
            };
            StyledTextSpan::with_style(
                span.range.start,
                span.range.end,
                color,
                span.category.is_bold(),
                span.category.is_italic(),
            )
        })
        .collect()
}

pub(super) fn build_preview_render_data(
    lines: &[FilePreviewLine],
    path: &Path,
    theme: &ThemeConfig,
) -> (String, Vec<StyledTextSpan>) {
    if lines.is_empty() {
        return (String::new(), Vec::new());
    }
    let text = lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default();
    let spans = syntax_spans_to_styled(&highlight_snippet(&text, extension, theme), theme);
    (text, spans)
}

pub(super) fn parse_hover_markdown_blocks(content: &str, theme: &ThemeConfig) -> Vec<FloatingBoxBlock> {
    let mut blocks = Vec::new();
    let mut prose_lines: Vec<String> = Vec::new();
    let mut code_lines: Vec<String> = Vec::new();
    let mut code_language = String::new();
    let mut in_code_block = false;

    let flush_prose = |blocks: &mut Vec<FloatingBoxBlock>, prose_lines: &mut Vec<String>| {
        let text = prose_lines
            .join("\n")
            .trim()
            .to_string();
        prose_lines.clear();
        if !text.is_empty() {
            blocks.push(FloatingBoxBlock::Prose(text));
        }
    };

    let flush_code = |blocks: &mut Vec<FloatingBoxBlock>, code_lines: &mut Vec<String>, code_language: &str| {
        let text = code_lines.join("\n");
        code_lines.clear();
        if text.trim().is_empty() {
            return;
        }
        let spans = syntax_spans_to_styled(
            &highlight_snippet(&text, code_language, theme),
            theme,
        );
        blocks.push(FloatingBoxBlock::Code { text, spans });
    };

    for line in content.lines() {
        let trimmed = line.trim_start();
        if let Some(fence) = trimmed.strip_prefix("```") {
            if in_code_block {
                flush_code(&mut blocks, &mut code_lines, &code_language);
                code_language.clear();
                in_code_block = false;
            } else {
                flush_prose(&mut blocks, &mut prose_lines);
                code_language = fence.trim().to_string();
                in_code_block = true;
            }
            continue;
        }

        if in_code_block {
            code_lines.push(line.to_string());
        } else {
            prose_lines.push(line.to_string());
        }
    }

    if in_code_block {
        flush_code(&mut blocks, &mut code_lines, &code_language);
    } else {
        flush_prose(&mut blocks, &mut prose_lines);
    }

    blocks
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

pub(super) fn collect_explorer_entries(app_state: &AppState) -> Vec<ExplorerEntry> {
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

    let filter_query = app_state
        .workspace_filter_query()
        .map(str::trim)
        .filter(|query| !query.is_empty())
        .map(str::to_lowercase);
    let mut subtree_matches: HashMap<PathBuf, bool> = HashMap::new();
    if let Some(query) = filter_query.as_deref() {
        compute_filter_matches(&root, &children_map, query, &mut subtree_matches);
    }

    let mut entries = Vec::new();
    collect_visible_explorer_entries(
        app_state,
        &root,
        0,
        &node_types,
        &children_map,
        filter_query.as_deref(),
        &subtree_matches,
        &mut entries,
    );
    entries
}

fn compute_filter_matches(
    path: &Path,
    children_map: &HashMap<PathBuf, Vec<PathBuf>>,
    query: &str,
    out: &mut HashMap<PathBuf, bool>,
) -> bool {
    let self_match = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.to_lowercase().contains(query));
    let mut subtree_match = self_match;

    if let Some(children) = children_map.get(path) {
        for child in children {
            subtree_match |= compute_filter_matches(child, children_map, query, out);
        }
    }

    out.insert(path.to_path_buf(), subtree_match);
    subtree_match
}

fn collect_visible_explorer_entries(
    app_state: &AppState,
    parent: &Path,
    depth: usize,
    node_types: &HashMap<PathBuf, WorkspaceNodeType>,
    children_map: &HashMap<PathBuf, Vec<PathBuf>>,
    filter_query: Option<&str>,
    subtree_matches: &HashMap<PathBuf, bool>,
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
        let name = child
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("?");
        let subtree_match =
            filter_query.is_none_or(|_| subtree_matches.get(child).copied().unwrap_or(false));
        if !subtree_match {
            continue;
        }

        let has_matching_descendant = filter_query.is_some()
            && file_type == WorkspaceNodeType::Folder
            && children_map.get(child).is_some_and(|children| {
                children
                    .iter()
                    .any(|nested| subtree_matches.get(nested).copied().unwrap_or(false))
            });
        let is_expanded = file_type == WorkspaceNodeType::Folder
            && (app_state.workspace_is_expanded(child) || has_matching_descendant);

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
                app_state,
                child,
                depth + 1,
                node_types,
                children_map,
                filter_query,
                subtree_matches,
                out,
            );
        }
    }
}

pub(super) fn build_sidebar_rows(
    entries: &[ExplorerEntry],
    selected_idx: usize,
    theme: &ThemeConfig,
    filter_active: bool,
    scroll_offset_rows: usize,
) -> Vec<SidebarRow> {
    if entries.is_empty() {
        return vec![SidebarRow {
            path: None,
            depth: 0,
            arrow: theme.sidebar_arrow(false, false).to_string(),
            nerd_icon: theme.get_icon_for_file("", false).to_string(),
            icon_color: theme.icons.default_file.color.as_f32(),
            label: if filter_active {
                "(no matches)".to_string()
            } else {
                "(no files)".to_string()
            },
            is_selected: false,
        }];
    }

    let selected = selected_idx.min(entries.len().saturating_sub(1));
    let scroll_start = scroll_offset_rows.min(entries.len().saturating_sub(1));
    entries
        .iter()
        .enumerate()
        .skip(scroll_start)
        .map(|(idx, entry)| {
            let is_dir = entry.file_type == WorkspaceNodeType::Folder;
            let arrow = theme.sidebar_arrow(is_dir, entry.is_expanded).to_string();
            let icon = theme.get_icon_for_file(&entry.name, is_dir).to_string();
            let icon_theme = theme.icon_theme_for_path(&entry.path, is_dir, entry.is_expanded);
            SidebarRow {
                path: Some(entry.path.clone()),
                depth: entry.depth,
                arrow,
                nerd_icon: icon,
                icon_color: icon_theme.color.as_f32(),
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
        RegionId::BottomPanel => theme.ui.terminal_bg.as_f32(),
        RegionId::StatusBar => theme.ui.status_bar_bg.as_f32(),
        _ => theme.ui.border_color.as_f32(),
    }
}

pub(super) fn language_id_for_path(path: &Path) -> String {
    if let Some(profile) = crate::lsp::registry::language_profile_for_path(path) {
        return profile.language_id.to_string();
    }

    match path.extension().and_then(|ext| ext.to_str()) {
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

#[cfg(test)]
mod tests {
    use super::syntax_spans_to_styled;
    use crate::{
        config::theme_config::ThemeConfig,
        syntax::highlight::{HighlightCategory, HighlightSpan},
    };

    #[test]
    fn syntax_spans_to_styled_applies_theme_colors_and_emphasis() {
        let theme = ThemeConfig::builtin_dark();
        let spans = vec![
            HighlightSpan {
                range: 0..4,
                category: HighlightCategory::Comment,
            },
            HighlightSpan {
                range: 5..12,
                category: HighlightCategory::Macro,
            },
            HighlightSpan {
                range: 13..17,
                category: HighlightCategory::Parameter,
            },
        ];

        let styled = syntax_spans_to_styled(&spans, &theme);

        assert_eq!(styled[0].color_rgba, theme.syntax.comment.as_u8());
        assert!(styled[0].italic);
        assert!(!styled[0].bold);

        assert_eq!(styled[1].color_rgba, theme.syntax.r#macro.as_u8());
        assert!(styled[1].bold);
        assert!(!styled[1].italic);

        assert_eq!(styled[2].color_rgba, theme.syntax.parameter.as_u8());
        assert!(!styled[2].bold);
        assert!(!styled[2].italic);
    }
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
