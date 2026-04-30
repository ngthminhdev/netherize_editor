//! Overlay rendering: Command Palette, File Picker, Recent Projects, Leap labels.

mod file_picker;
mod highlighted_label;
mod leap;
mod live_grep;
mod minimal;
mod recent_projects;

use crate::{app::command_palette::CommandPaletteRenderModel, render::renderer::Renderer};

impl Renderer {
    // ── Public entry point ─────────────────────────────────────────────────────

    /// Layout + upload command palette overlay (chrome + text).
    ///
    /// The palette model is precomputed by `CommandPalette::render()` so the
    /// renderer only consumes geometry/text and uploads GPU instances.
    pub fn update_palette_content(&mut self, model: &CommandPaletteRenderModel) {
        if self.last_palette_model.as_ref() == Some(model) {
            return;
        }
        if matches!(
            model.mode,
            crate::app::command_palette::CommandPaletteMode::RecentProjects
                | crate::app::command_palette::CommandPaletteMode::ThemeSelector
        ) {
            self.render_recent_projects(model);
        } else if model.mode == crate::app::command_palette::CommandPaletteMode::LiveGrep {
            self.render_live_grep_picker(model);
        } else if model.mode.is_complex_picker() {
            self.render_file_picker_complex(model);
        } else {
            self.render_command_palette_minimalist(model);
        }
        self.palette_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.palette_glyph_instances,
        );
        self.last_palette_model = Some(model.clone());
    }

    pub fn clear_palette(&mut self) {
        self.palette_scissor = None;
        self.palette_chrome_instances.clear();
        self.palette_glyph_instances.clear();
        self.last_palette_model = None;
        self.palette_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }
}
