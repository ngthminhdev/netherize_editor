/// Dirty flags cho pipeline đồng bộ Buffer -> Layout -> Visual.
///
/// Quy ước:
/// - text_dirty: nội dung text thay đổi
/// - layout_dirty: cần shape/layout lại
/// - visual_dirty: cần cập nhật dữ liệu render (glyph/caret)
/// - file_dirty: có thay đổi chưa lưu
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DirtyFlags {
    pub text_dirty: bool,
    pub layout_dirty: bool,
    pub visual_dirty: bool,
    pub file_dirty: bool,
}

impl DirtyFlags {
    pub fn clean() -> Self {
        Self::default()
    }

    pub fn bootstrap(file_dirty: bool) -> Self {
        Self {
            text_dirty: true,
            layout_dirty: true,
            visual_dirty: true,
            file_dirty,
        }
    }

    /// Khi text đổi, layout + visual đều cần refresh.
    pub fn mark_text_changed(&mut self) {
        self.text_dirty = true;
        self.layout_dirty = true;
        self.visual_dirty = true;
    }

    /// Di chuyển caret không đổi text nhưng UI vẫn phải redraw.
    pub fn mark_cursor_or_visual_changed(&mut self) {
        self.visual_dirty = true;
    }

    /// Resize/viewport đổi => layout + visual đều cần rebuild.
    pub fn mark_layout_changed(&mut self) {
        self.layout_dirty = true;
        self.visual_dirty = true;
    }

    pub fn update_file_dirty(&mut self, is_dirty: bool) {
        self.file_dirty = is_dirty;
    }

    pub fn clear_text_layout(&mut self) {
        self.text_dirty = false;
        self.layout_dirty = false;
    }

    pub fn clear_visual(&mut self) {
        self.visual_dirty = false;
    }

    pub fn summary(&self) -> String {
        format!(
            "text_dirty={} layout_dirty={} visual_dirty={} file_dirty={}",
            self.text_dirty, self.layout_dirty, self.visual_dirty, self.file_dirty
        )
    }
}
