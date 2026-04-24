use crate::workbench::{debug_state::SourceLocation, region_model::RegionBounds};

#[derive(Debug, Clone, PartialEq)]
pub struct TextCoordinateMapper {
    pub editor_bounds: RegionBounds,
    pub text_origin: [f32; 2],
    pub line_height: f32,
    pub glyph_advance: f32,
    pub line_lengths: Vec<usize>,
    pub vertical_scroll_lines: usize,
}

impl TextCoordinateMapper {
    pub fn from_text(
        text: &str,
        editor_bounds: RegionBounds,
        padding_left: f32,
        padding_top: f32,
        line_height: f32,
        glyph_advance: f32,
    ) -> Self {
        let line_lengths = if text.is_empty() {
            vec![0]
        } else {
            text.lines().map(|line| line.chars().count()).collect()
        };

        Self {
            editor_bounds,
            text_origin: [
                editor_bounds.x + padding_left.max(0.0),
                editor_bounds.y + padding_top.max(0.0),
            ],
            line_height: line_height.max(1.0),
            glyph_advance: glyph_advance.max(1.0),
            line_lengths,
            vertical_scroll_lines: 0,
        }
    }

    pub fn line_count(&self) -> usize {
        self.line_lengths.len()
    }

    pub fn vertical_scroll_lines(&self) -> usize {
        self.vertical_scroll_lines
    }

    pub fn viewport_line_capacity(&self) -> usize {
        ((self.editor_bounds.height / self.line_height).floor() as usize).max(1)
    }

    pub fn max_scroll_lines(&self) -> usize {
        self.line_count()
            .saturating_sub(self.viewport_line_capacity())
    }

    pub fn set_vertical_scroll_lines(&mut self, scroll_lines: usize) {
        self.vertical_scroll_lines = scroll_lines.min(self.max_scroll_lines());
    }

    pub fn scroll_by_lines(&mut self, delta_lines: i32) -> bool {
        if delta_lines == 0 {
            return false;
        }
        let current = self.vertical_scroll_lines as i32;
        let max = self.max_scroll_lines() as i32;
        let next = (current + delta_lines).clamp(0, max);
        if next == current {
            return false;
        }
        self.vertical_scroll_lines = next as usize;
        true
    }

    pub fn map_line_column_to_rect(&self, line: usize, column: usize) -> Option<RegionBounds> {
        let line_len = *self.line_lengths.get(line)?;
        let clamped_col = column.min(line_len);
        let x = self.text_origin[0] + clamped_col as f32 * self.glyph_advance;
        let y = self.text_origin[1]
            + (line as f32 - self.vertical_scroll_lines as f32) * self.line_height;

        // Nếu dòng đã ra ngoài viewport editor sau khi scroll thì không tạo anchor.
        if y + self.line_height < self.editor_bounds.y
            || y > self.editor_bounds.y + self.editor_bounds.height
        {
            return None;
        }

        Some(RegionBounds::new(
            x,
            y,
            self.glyph_advance.max(2.0),
            self.line_height,
        ))
    }

    pub fn map_source_location_rect(&self, location: SourceLocation) -> Option<RegionBounds> {
        self.map_line_column_to_rect(location.line, location.column)
    }

    pub fn gutter_marker_rect(&self, line: usize) -> Option<RegionBounds> {
        let line_rect = self.map_line_column_to_rect(line, 0)?;
        let marker_size = (self.line_height * 0.56).clamp(6.0, 14.0);
        Some(RegionBounds::new(
            self.editor_bounds.x + 6.0,
            line_rect.y + (line_rect.height - marker_size) * 0.5,
            marker_size,
            marker_size,
        ))
    }

    pub fn line_at_screen_y(&self, y: f32) -> Option<usize> {
        if y < self.editor_bounds.y || y > self.editor_bounds.y + self.editor_bounds.height {
            return None;
        }
        if y < self.text_origin[1] {
            return None;
        }

        let local_line = ((y - self.text_origin[1]) / self.line_height).floor() as usize;
        let absolute_line = self.vertical_scroll_lines.saturating_add(local_line);
        if absolute_line >= self.line_count() {
            return None;
        }
        Some(absolute_line)
    }

    pub fn hit_test_gutter(&self, x: f32, y: f32) -> bool {
        let gutter_x_min = self.editor_bounds.x;
        let gutter_x_max = self.editor_bounds.x + 28.0;
        x >= gutter_x_min
            && x <= gutter_x_max
            && y >= self.editor_bounds.y
            && y <= self.editor_bounds.y + self.editor_bounds.height
    }
}

#[cfg(test)]
mod tests {
    use super::TextCoordinateMapper;
    use crate::workbench::region_model::RegionBounds;

    #[test]
    fn maps_line_column_to_stable_pixel_rect() {
        let mapper = TextCoordinateMapper::from_text(
            "fn main() {\n    println!(\"hi\");\n}",
            RegionBounds::new(40.0, 100.0, 600.0, 400.0),
            18.0,
            12.0,
            20.0,
            10.0,
        );

        let rect = mapper
            .map_line_column_to_rect(1, 4)
            .expect("line/column should map");
        assert!((rect.x - 98.0).abs() < 0.001);
        assert!((rect.y - 132.0).abs() < 0.001);
    }

    #[test]
    fn scroll_moves_anchor_y_and_line_lookup() {
        let mut mapper = TextCoordinateMapper::from_text(
            "l0\nl1\nl2\nl3\nl4\nl5",
            RegionBounds::new(0.0, 0.0, 200.0, 60.0),
            10.0,
            10.0,
            10.0,
            8.0,
        );
        assert_eq!(mapper.max_scroll_lines(), 0);

        mapper.editor_bounds = RegionBounds::new(0.0, 0.0, 200.0, 30.0);
        assert_eq!(mapper.max_scroll_lines(), 3);
        mapper.set_vertical_scroll_lines(2);

        let rect = mapper
            .map_line_column_to_rect(2, 0)
            .expect("line 2 should be visible after scrolling");
        assert!((rect.y - 10.0).abs() < 0.001);
        assert_eq!(mapper.line_at_screen_y(15.0), Some(2));
    }
}
