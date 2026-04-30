use cosmic_text::{
    Attrs, Buffer, CacheKey, Color, Family, FontSystem, Metrics, Shaping, SwashCache, SwashImage,
    fontdb,
};

use crate::config::theme_config::srgb_rgba_to_linear_f32;

#[derive(Debug, Clone)]
pub struct FontFaceSummary {
    pub id: fontdb::ID,
    pub family: String,
    pub post_script_name: String,
    pub style: fontdb::Style,
    pub weight: fontdb::Weight,
    pub monospaced: bool,
    pub source: String,
}

#[derive(Debug, Clone, Copy)]
pub struct VisibleGlyph {
    pub cache_key: CacheKey,
    pub physical_x: i32,
    pub physical_y: i32,
    pub color: [f32; 4],
    /// Byte offset of this glyph's cluster inside its layout run's line text.
    /// Mirrors `cosmic_text::LayoutGlyph::start` — needed to correlate a glyph
    /// with the cursor's byte position when drawing the block-cursor overlay.
    pub byte_start: usize,
    pub byte_end: usize,
    /// Line index inside the Buffer (matches `run.line_i`).
    pub line_i: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyledTextSpan {
    pub start: usize,
    pub end: usize,
    pub color_rgba: [u8; 4],
    pub bold: bool,
    pub italic: bool,
}

impl StyledTextSpan {
    pub const fn new(start: usize, end: usize, color_rgba: [u8; 4]) -> Self {
        Self {
            start,
            end,
            color_rgba,
            bold: false,
            italic: false,
        }
    }

    pub const fn with_style(
        start: usize,
        end: usize,
        color_rgba: [u8; 4],
        bold: bool,
        italic: bool,
    ) -> Self {
        Self {
            start,
            end,
            color_rgba,
            bold,
            italic,
        }
    }
}

pub struct TextSystem {
    font_system: FontSystem,
    swash_cache: SwashCache,
    buffer: Buffer,
    font_family: Option<String>,
}

impl TextSystem {
    pub fn new(metrics: Metrics, width: Option<f32>, height: Option<f32>) -> Self {
        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let mut buffer = Buffer::new(&mut font_system, metrics);
        buffer.set_size(&mut font_system, width, height);

        Self {
            font_system,
            swash_cache,
            buffer,
            font_family: None,
        }
    }

    /// Override the default font family (e.g. "Google Sans Code"). `None` falls back
    /// to cosmic-text's built-in default family resolution.
    pub fn set_font_family(&mut self, family: Option<&str>) {
        self.font_family = family.map(str::to_string);
    }

    pub fn locale(&self) -> &str {
        self.font_system.locale()
    }

    pub fn face_count(&self) -> usize {
        self.font_system.db().faces().count()
    }

    pub fn sample_faces(&self, limit: usize) -> Vec<FontFaceSummary> {
        self.font_system
            .db()
            .faces()
            .take(limit)
            .map(Self::face_to_summary)
            .collect()
    }

    pub fn resolve_face(&self, id: fontdb::ID) -> Option<FontFaceSummary> {
        self.font_system.db().face(id).map(Self::face_to_summary)
    }

    pub fn set_size(&mut self, width: Option<f32>, height: Option<f32>) {
        self.buffer.set_size(&mut self.font_system, width, height);
    }

    pub fn set_metrics(&mut self, metrics: Metrics) {
        self.buffer.set_metrics(&mut self.font_system, metrics);
    }

    pub fn set_text(&mut self, text: &str) {
        let family = self.font_family.as_deref();
        let attrs = apply_family(Attrs::new(), family);
        self.buffer
            .set_text(&mut self.font_system, text, &attrs, Shaping::Advanced, None);
        self.buffer.shape_until_scroll(&mut self.font_system, false);
    }

    pub fn set_text_with_color(&mut self, text: &str, color_rgba: [u8; 4]) {
        let family = self.font_family.as_deref();
        let attrs = apply_family(
            Attrs::new().color(Self::rgba_u8_to_color(color_rgba)),
            family,
        );
        self.buffer
            .set_text(&mut self.font_system, text, &attrs, Shaping::Advanced, None);
        self.buffer.shape_until_scroll(&mut self.font_system, false);
    }

    /// Set text bold với màu cụ thể — dùng cho Leap label overlay.
    pub fn set_text_bold_color(&mut self, text: &str, color_rgba: [u8; 4]) {
        let family = self.font_family.as_deref();
        let attrs = apply_family(
            Attrs::new()
                .color(Self::rgba_u8_to_color(color_rgba))
                .weight(fontdb::Weight::BOLD),
            family,
        );
        self.buffer
            .set_text(&mut self.font_system, text, &attrs, Shaping::Advanced, None);
        self.buffer.shape_until_scroll(&mut self.font_system, false);
    }

    pub fn set_text_italic_color(&mut self, text: &str, color_rgba: [u8; 4]) {
        let family = self.font_family.as_deref();
        let attrs = apply_family(
            Attrs::new()
                .color(Self::rgba_u8_to_color(color_rgba))
                .style(fontdb::Style::Italic),
            family,
        );
        self.buffer
            .set_text(&mut self.font_system, text, &attrs, Shaping::Advanced, None);
        self.buffer.shape_until_scroll(&mut self.font_system, false);
    }

    /// Set rich text bằng styled spans theo byte range.
    ///
    /// `spans` phải dùng byte-offset trong `text` (khớp output của tree-sitter).
    /// Hàm này tự sanitize boundary để tránh lỗi UTF-8 khi tạo slices.
    pub fn set_text_with_spans(
        &mut self,
        text: &str,
        default_color_rgba: [u8; 4],
        spans: &[StyledTextSpan],
    ) {
        if spans.is_empty() {
            self.set_text_with_color(text, default_color_rgba);
            return;
        }

        let sanitized: Vec<StyledTextSpan> = spans
            .iter()
            .filter_map(|span| Self::sanitize_span(text, *span))
            .collect();
        if sanitized.is_empty() {
            self.set_text_with_color(text, default_color_rgba);
            return;
        }

        let family = self.font_family.as_deref();
        let default_attrs = apply_family(
            Attrs::new().color(Self::rgba_u8_to_color(default_color_rgba)),
            family,
        );

        // Split on every span boundary and let later spans override earlier ones.
        // This keeps diagnostic spans able to sit on top of syntax spans cleanly.
        let mut boundaries = Vec::with_capacity(sanitized.len() * 2 + 2);
        boundaries.push(0usize);
        boundaries.push(text.len());
        for span in &sanitized {
            boundaries.push(span.start);
            boundaries.push(span.end);
        }
        boundaries.sort_unstable();
        boundaries.dedup();

        let mut resolved_segments: Vec<(usize, usize, Option<StyledTextSpan>)> =
            Vec::with_capacity(boundaries.len().saturating_sub(1));
        for pair in boundaries.windows(2) {
            let start = pair[0];
            let end = pair[1];
            if start >= end {
                continue;
            }

            let active_span = sanitized
                .iter()
                .rev()
                .find(|span| span.start < end && span.end > start)
                .copied();
            if let Some(last) = resolved_segments.last_mut()
                && last.2 == active_span
                && last.1 == start
            {
                last.1 = end;
                continue;
            }
            resolved_segments.push((start, end, active_span));
        }

        let mut segments: Vec<(&str, Attrs<'_>)> = Vec::with_capacity(resolved_segments.len());
        for (start, end, style) in resolved_segments {
            let attrs = if let Some(span) = style {
                let mut attrs = Attrs::new().color(Self::rgba_u8_to_color(span.color_rgba));
                if span.bold {
                    attrs = attrs.weight(fontdb::Weight::BOLD);
                }
                if span.italic {
                    attrs = attrs.style(fontdb::Style::Italic);
                }
                apply_family(attrs, family)
            } else {
                default_attrs.clone()
            };
            segments.push((&text[start..end], attrs));
        }

        self.buffer.set_rich_text(
            &mut self.font_system,
            segments.into_iter(),
            &default_attrs,
            Shaping::Advanced,
            None,
        );
        self.buffer.shape_until_scroll(&mut self.font_system, false);
    }

    pub fn shape_until_scroll(&mut self, prune: bool) {
        self.buffer.shape_until_scroll(&mut self.font_system, prune);
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn rasterize_cache_key(&mut self, cache_key: CacheKey) -> Option<SwashImage> {
        self.swash_cache
            .get_image(&mut self.font_system, cache_key)
            .clone()
    }

    /// Thu thập glyph đã layout sẵn theo tọa độ vật lý để render.
    ///
    /// `origin_x`, `origin_y` là offset pixel trong window.
    /// Ví dụ origin=(40, 80) sẽ đẩy text vào trong màn hình thay vì dính góc.
    pub fn collect_visible_glyphs(
        &self,
        origin_x: f32,
        origin_y: f32,
        fallback_color: [f32; 4],
    ) -> Vec<VisibleGlyph> {
        let mut glyphs = Vec::new();

        for run in self.buffer.layout_runs() {
            for glyph in run.glyphs {
                let physical = glyph.physical((origin_x, origin_y + run.line_y), 1.0);
                let color = glyph
                    .color_opt
                    .map(Self::rgba_f32_from_color)
                    .unwrap_or(fallback_color);
                glyphs.push(VisibleGlyph {
                    cache_key: physical.cache_key,
                    physical_x: physical.x,
                    physical_y: physical.y,
                    color,
                    byte_start: glyph.start,
                    byte_end: glyph.end,
                    line_i: run.line_i,
                });
            }
        }

        glyphs
    }

    fn rgba_u8_to_color(rgba: [u8; 4]) -> Color {
        Color::rgba(rgba[0], rgba[1], rgba[2], rgba[3])
    }

    fn rgba_f32_from_color(color: Color) -> [f32; 4] {
        // cosmic-text 0.18 stores raw RGBA bytes; treat them as sRGB and decode
        // to linear before handing the color to wgpu.
        srgb_rgba_to_linear_f32(color.as_rgba())
    }

    fn sanitize_span(text: &str, span: StyledTextSpan) -> Option<StyledTextSpan> {
        if text.is_empty() {
            return None;
        }

        let len = text.len();
        let mut start = span.start.min(len);
        let mut end = span.end.min(len);
        if start >= end {
            return None;
        }

        while start > 0 && !text.is_char_boundary(start) {
            start -= 1;
        }
        while end < len && !text.is_char_boundary(end) {
            end += 1;
        }

        (start < end).then_some(StyledTextSpan::with_style(
            start,
            end,
            span.color_rgba,
            span.bold,
            span.italic,
        ))
    }

    fn face_to_summary(face: &fontdb::FaceInfo) -> FontFaceSummary {
        let family = face
            .families
            .first()
            .map(|(name, _)| name.clone())
            .unwrap_or_else(|| "<unknown>".to_string());

        FontFaceSummary {
            id: face.id,
            family,
            post_script_name: face.post_script_name.clone(),
            style: face.style,
            weight: face.weight,
            monospaced: face.monospaced,
            source: format!("{:?}", face.source),
        }
    }
}

fn apply_family<'a>(attrs: Attrs<'a>, family: Option<&'a str>) -> Attrs<'a> {
    match family {
        Some(name) => attrs.family(Family::Name(name)),
        None => attrs,
    }
}

#[cfg(test)]
mod tests {
    use cosmic_text::Metrics;

    use super::{StyledTextSpan, TextSystem};

    #[test]
    fn unbounded_height_shapes_deep_lines() {
        let total_lines = 96usize;
        let text = (0..total_lines)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");

        let mut system = TextSystem::new(Metrics::new(16.0, 22.0), Some(900.0), None);
        system.set_text(&text);
        let glyphs = system.collect_visible_glyphs(0.0, 0.0, [1.0, 1.0, 1.0, 1.0]);
        let max_line = glyphs.iter().map(|glyph| glyph.line_i).max().unwrap_or(0);

        assert!(
            max_line >= total_lines.saturating_sub(2),
            "deep lines were not shaped: max_line={max_line}, total_lines={total_lines}"
        );
    }

    #[test]
    fn later_spans_override_earlier_overlaps() {
        let mut system = TextSystem::new(Metrics::new(16.0, 22.0), Some(300.0), Some(200.0));
        system.set_text_with_spans(
            "abcdef",
            [0xF0, 0xF0, 0xF0, 0xFF],
            &[
                StyledTextSpan::new(0, 6, [0x22, 0x88, 0xFF, 0xFF]),
                StyledTextSpan::with_style(2, 4, [0xFF, 0x55, 0x55, 0xFF], true, false),
            ],
        );

        let glyphs = system.collect_visible_glyphs(0.0, 0.0, [1.0, 1.0, 1.0, 1.0]);
        let mut colors_by_byte = glyphs
            .into_iter()
            .map(|glyph| (glyph.byte_start, glyph.color))
            .collect::<Vec<_>>();
        colors_by_byte.sort_by_key(|(byte_start, _)| *byte_start);

        let blue =
            TextSystem::rgba_f32_from_color(TextSystem::rgba_u8_to_color([0x22, 0x88, 0xFF, 0xFF]));
        let red =
            TextSystem::rgba_f32_from_color(TextSystem::rgba_u8_to_color([0xFF, 0x55, 0x55, 0xFF]));

        assert_eq!(colors_by_byte[0].1, blue);
        assert_eq!(colors_by_byte[1].1, blue);
        assert_eq!(colors_by_byte[2].1, red);
        assert_eq!(colors_by_byte[3].1, red);
        assert_eq!(colors_by_byte[4].1, blue);
        assert_eq!(colors_by_byte[5].1, blue);
    }

    #[test]
    fn invalid_utf8_span_boundaries_are_sanitized_without_dropping_valid_text() {
        let text = "a🦀z";
        let mut system = TextSystem::new(Metrics::new(16.0, 22.0), Some(300.0), Some(200.0));
        system.set_text_with_spans(
            text,
            [0xEE, 0xEE, 0xEE, 0xFF],
            &[
                StyledTextSpan::with_style(1, 3, [0xFF, 0x66, 0x66, 0xFF], true, false),
                StyledTextSpan::new(0, text.len() + 10, [0x66, 0xAA, 0xFF, 0xFF]),
            ],
        );

        let glyphs = system.collect_visible_glyphs(0.0, 0.0, [1.0, 1.0, 1.0, 1.0]);
        assert!(
            !glyphs.is_empty(),
            "sanitized spans should still produce glyphs"
        );

        let starts: Vec<usize> = glyphs.iter().map(|glyph| glyph.byte_start).collect();
        assert!(starts.contains(&0));
        assert!(starts.contains(&1));
        assert!(starts.contains(&(text.len() - 1)));
    }

    #[test]
    fn missing_font_family_falls_back_to_default_font_resolution() {
        let mut system = TextSystem::new(Metrics::new(16.0, 22.0), Some(400.0), Some(200.0));
        system.set_font_family(Some("__netherize_missing_font_family__"));
        system.set_text("fallback text abc 123");

        let glyphs = system.collect_visible_glyphs(0.0, 0.0, [1.0, 1.0, 1.0, 1.0]);
        assert!(
            !glyphs.is_empty(),
            "missing font family should still shape text via fallback"
        );
    }
}
