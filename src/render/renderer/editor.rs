//! Editor viewport rendering modules.

mod buffers;
mod completion;
mod fuzzy;
mod help;
mod overlays;
mod selections;
mod settings;
mod viewport;

use crate::{
    app::app_state::AppState, async_runtime::message::LspDiagnostic, render::renderer::Renderer,
};

use super::helpers::gutter_width_for_editor;

pub(super) fn run_x_for_byte(
    text_area_x: f32,
    run: &cosmic_text::LayoutRun,
    byte_in_line: usize,
) -> f32 {
    if run.glyphs.is_empty() {
        return text_area_x;
    }

    let mut x = text_area_x + run.line_w;
    for glyph in run.glyphs {
        let left = text_area_x + glyph.x;
        let right = left + glyph.w;
        if byte_in_line <= glyph.start {
            return left;
        }
        if byte_in_line < glyph.end {
            return left;
        }
        x = right;
    }
    x
}

pub(super) fn wrap_text_lines(text: &str, max_chars: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let max_chars = max_chars.max(8);
    let mut out = Vec::new();
    for raw_line in text.lines() {
        if raw_line.chars().count() <= max_chars {
            out.push(raw_line.to_string());
            continue;
        }

        let mut current = String::new();
        for word in raw_line.split_whitespace() {
            let word_chars: Vec<char> = word.chars().collect();
            let word_len = word_chars.len();
            let current_len = current.chars().count();
            let sep = if current.is_empty() { 0 } else { 1 };

            // Flush current line if this word won't fit alongside it.
            if !current.is_empty() && current_len + sep + word_len > max_chars {
                out.push(std::mem::take(&mut current));
            }

            if word_len > max_chars {
                // Oversized token: chunk into max_chars slices.
                // current is guaranteed empty here (either was already empty,
                // or was flushed above).
                let mut start = 0;
                while start + max_chars < word_len {
                    out.push(word_chars[start..start + max_chars].iter().collect());
                    start += max_chars;
                }
                // Last (partial) chunk goes into current — may merge with next word.
                current = word_chars[start..].iter().collect();
            } else {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(word);
            }
        }

        if !current.is_empty() {
            out.push(current);
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

pub(super) fn cursor_diagnostic(app_state: &AppState) -> Option<&LspDiagnostic> {
    let cursor_line = app_state.cursor_line_col().0;
    app_state
        .active_file()
        .and_then(|path| app_state.diagnostics_for_path(path))
        .and_then(|items| {
            items
                .iter()
                .filter(|diag| {
                    let start_line = diag.range.start.line as usize;
                    let end_line = diag.range.end.line as usize;
                    cursor_line >= start_line && cursor_line <= end_line
                })
                .min_by_key(|diag| (diag.severity.unwrap_or(u32::MAX), diag.range.start.line))
        })
}

#[cfg(test)]
mod tests {
    use super::wrap_text_lines;

    #[test]
    fn wrap_text_lines_keeps_output_non_empty() {
        let wrapped = wrap_text_lines("hello world", 3);
        assert!(!wrapped.is_empty());
    }
}

pub(super) struct EditorViewportGeometry {
    pub(super) line_height: f32,
    pub(super) font_size: f32,
    pub(super) gutter_width: f32,
    pub(super) viewport_text_left: f32,
    pub(super) viewport_text_width: f32,
    pub(super) origin_x: f32,
    pub(super) origin_y: f32,
}

pub(super) fn editor_viewport_geometry(
    renderer: &Renderer,
    app_state: &AppState,
    center_bounds: [f32; 4],
) -> EditorViewportGeometry {
    let line_height = renderer.theme.editor.line_height;
    let font_size = renderer.theme.editor.font_size;
    let total_lines = app_state.total_lines().max(1);
    let gutter_digits = total_lines.to_string().len().max(3);
    let gutter_width = gutter_width_for_editor(gutter_digits, font_size, line_height);
    let scroll_y = app_state.scroll_line as f32 * line_height;
    let scroll_x = app_state.scroll_column as f32 * (font_size * 0.6).max(1.0);
    let viewport_text_left = center_bounds[0] + renderer.editor_padding_x + gutter_width;
    let origin_x = viewport_text_left - scroll_x;
    let origin_y = center_bounds[1] + renderer.editor_padding_y + line_height - scroll_y;
    let viewport_text_width =
        (center_bounds[2] - renderer.editor_padding_x - gutter_width).max(1.0);

    EditorViewportGeometry {
        line_height,
        font_size,
        gutter_width,
        viewport_text_left,
        viewport_text_width,
        origin_x,
        origin_y,
    }
}
