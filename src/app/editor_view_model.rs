use crate::{
    app::{app_state::AppState, dirty_flags::DirtyFlags},
    core::{
        command_dispatch::{DispatchReport, dispatch_command},
        commands::Command,
    },
    render::caret::CaretScreenRect,
    syntax::highlight::{HighlightPalette, HighlightSpan},
    text::{
        atlas::GlyphAtlas,
        layout_sync::{compute_caret_layout, rebuild_layout_projection},
        text_system::{StyledTextSpan, TextSystem},
    },
};

#[derive(Debug, Clone, Copy)]
pub struct ViewportState {
    pub origin: [f32; 2],
}

impl ViewportState {
    pub fn new(origin_x: f32, origin_y: f32) -> Self {
        Self {
            origin: [origin_x, origin_y],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ViewSyncStats {
    pub layout_rebuilt: bool,
    pub glyph_count: usize,
    pub caret_visible: bool,
    pub redraw_required: bool,
    pub highlight_span_count: usize,
    pub syntax_revision: Option<u64>,
    pub dirty_flags: DirtyFlags,
}

/// EditorViewModel là lớp projection:
/// Buffer/AppState -> Layout data + Caret + Dirty flags cho render.
pub struct EditorViewModel {
    viewport: ViewportState,
    text_color: [f32; 4],
    caret_color: [f32; 4],
    caret_width: f32,
    glyph_instances: Vec<crate::render::glyph_instance::GlyphInstance>,
    caret_rect: Option<CaretScreenRect>,
    dirty_flags: DirtyFlags,
    redraw_required: bool,
    highlight_palette: HighlightPalette,
    styled_highlight_spans: Vec<StyledTextSpan>,
    highlight_span_count: usize,
    syntax_revision: Option<u64>,
}

impl EditorViewModel {
    pub fn new(viewport: ViewportState, text_color: [f32; 4], caret_color: [f32; 4]) -> Self {
        Self {
            viewport,
            text_color,
            caret_color,
            caret_width: 2.0,
            glyph_instances: Vec::new(),
            caret_rect: None,
            dirty_flags: DirtyFlags::bootstrap(false),
            redraw_required: true,
            highlight_palette: HighlightPalette::default(),
            styled_highlight_spans: Vec::new(),
            highlight_span_count: 0,
            syntax_revision: None,
        }
    }

    pub fn apply_command(&mut self, app_state: &mut AppState, command: Command) -> DispatchReport {
        let command_for_flags = command.clone();
        let report = dispatch_command(app_state, command);

        self.dirty_flags.update_file_dirty(app_state.is_dirty());

        if report.success && report.state_changed {
            match command_for_flags {
                Command::InsertChar(_)
                | Command::InsertText(_)
                | Command::Newline
                | Command::Backspace
                | Command::OpenFile(_)
                | Command::FilePickerConfirmSelection => {
                    // Text vừa đổi thì highlight cũ không còn khớp offset, chờ async reparse mới.
                    self.styled_highlight_spans.clear();
                    self.highlight_span_count = 0;
                    self.syntax_revision = None;
                    self.dirty_flags.mark_text_changed();
                }
                Command::MoveLeft | Command::MoveRight | Command::MoveUp | Command::MoveDown => {
                    self.dirty_flags.mark_cursor_or_visual_changed();
                }
                Command::SwitchMode(_) => {
                    // Mode đổi có thể ảnh hưởng caret/render overlay.
                    self.dirty_flags.mark_cursor_or_visual_changed();
                }
                Command::OpenCommandPalette
                | Command::OpenFileFinder
                | Command::FilePickerAppendQuery(_)
                | Command::FilePickerBackspaceQuery
                | Command::FilePickerSelectNext
                | Command::FilePickerSelectPrev
                | Command::CloseFilePicker
                | Command::ToggleTerminal => {
                    // Workflow command đổi focus mode cần refresh visual state.
                    self.dirty_flags.mark_cursor_or_visual_changed();
                }
                Command::SaveFile => {}
                // Workbench navigation commands — handled by event_loop, not text state.
                Command::ToggleExplorer
                | Command::FocusEditor
                | Command::FocusExplorer
                | Command::FocusTerminal
                | Command::FocusInspector
                | Command::MoveFocusCycle
                | Command::FocusBack
                | Command::ExplorerMoveUp
                | Command::ExplorerMoveDown
                | Command::ExplorerCollapseOrParent
                | Command::ExplorerExpandOrChild
                | Command::ExplorerToggleOrOpen
                | Command::ExplorerExpandCollapse
                | Command::ExplorerOpenFile
                | Command::TerminalWriteInput(_)
                | Command::NextPanelTab
                | Command::PrevPanelTab => {
                    self.dirty_flags.mark_cursor_or_visual_changed();
                }
            }
        }

        if report.request_redraw {
            self.redraw_required = true;
        }

        report
    }

    pub fn mark_layout_dirty(&mut self) {
        self.dirty_flags.mark_layout_changed();
        self.redraw_required = true;
    }

    pub fn sync_projection(
        &mut self,
        app_state: &AppState,
        text_system: &mut TextSystem,
        atlas: &mut GlyphAtlas,
        queue: &wgpu::Queue,
    ) -> Result<ViewSyncStats, String> {
        let text_snapshot = app_state.text_string();
        self.sync_projection_with_text(app_state, text_system, atlas, queue, &text_snapshot)
    }

    pub fn sync_projection_with_text(
        &mut self,
        app_state: &AppState,
        text_system: &mut TextSystem,
        atlas: &mut GlyphAtlas,
        queue: &wgpu::Queue,
        text_snapshot: &str,
    ) -> Result<ViewSyncStats, String> {
        let mut layout_rebuilt = false;

        if self.dirty_flags.layout_dirty {
            let projection = rebuild_layout_projection(
                text_snapshot,
                app_state,
                text_system,
                atlas,
                queue,
                self.viewport.origin,
                self.text_color,
                &self.styled_highlight_spans,
            )?;
            self.glyph_instances = projection.glyph_instances;
            self.caret_rect = Some(CaretScreenRect::from_layout(
                projection.caret_layout.x,
                projection.caret_layout.top,
                projection.caret_layout.height,
                self.caret_width,
                self.caret_color,
            ));
            self.dirty_flags.clear_text_layout();
            self.dirty_flags.clear_visual();
            layout_rebuilt = true;
            self.redraw_required = true;
        } else if self.dirty_flags.visual_dirty {
            let caret_layout = compute_caret_layout(text_system, app_state, self.viewport.origin);
            self.caret_rect = Some(CaretScreenRect::from_layout(
                caret_layout.x,
                caret_layout.top,
                caret_layout.height,
                self.caret_width,
                self.caret_color,
            ));
            self.dirty_flags.clear_visual();
            self.redraw_required = true;
        }

        self.dirty_flags.update_file_dirty(app_state.is_dirty());

        Ok(ViewSyncStats {
            layout_rebuilt,
            glyph_count: self.glyph_instances.len(),
            caret_visible: self.caret_rect.is_some(),
            redraw_required: self.redraw_required,
            highlight_span_count: self.highlight_span_count,
            syntax_revision: self.syntax_revision,
            dirty_flags: self.dirty_flags,
        })
    }

    pub fn apply_syntax_highlights(&mut self, revision: u64, spans: &[HighlightSpan]) {
        self.styled_highlight_spans = spans
            .iter()
            .map(|span| {
                StyledTextSpan::new(
                    span.range.start,
                    span.range.end,
                    self.highlight_palette.color_for(span.category),
                )
            })
            .collect();
        self.highlight_span_count = self.styled_highlight_spans.len();
        self.syntax_revision = Some(revision);
        self.mark_layout_dirty();
    }

    pub fn clear_syntax_highlights(&mut self) {
        self.styled_highlight_spans.clear();
        self.highlight_span_count = 0;
        self.syntax_revision = None;
        self.mark_layout_dirty();
    }

    pub fn glyph_instances(&self) -> &[crate::render::glyph_instance::GlyphInstance] {
        &self.glyph_instances
    }

    pub fn caret_rect(&self) -> Option<CaretScreenRect> {
        self.caret_rect
    }

    pub fn take_redraw_required(&mut self) -> bool {
        let required = self.redraw_required;
        self.redraw_required = false;
        required
    }

    pub fn dirty_flags(&self) -> DirtyFlags {
        self.dirty_flags
    }

    pub fn viewport_origin(&self) -> [f32; 2] {
        self.viewport.origin
    }

    pub fn set_viewport_origin(&mut self, origin: [f32; 2]) {
        if self.viewport.origin == origin {
            return;
        }
        self.viewport.origin = origin;
        self.mark_layout_dirty();
    }
}
