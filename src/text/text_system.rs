use cosmic_text::{
    Attrs, Buffer, CacheKey, Color, FontSystem, Metrics, Shaping, SwashCache, SwashImage, fontdb,
};

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyledTextSpan {
    pub start: usize,
    pub end: usize,
    pub color_rgba: [u8; 4],
}

impl StyledTextSpan {
    pub const fn new(start: usize, end: usize, color_rgba: [u8; 4]) -> Self {
        Self {
            start,
            end,
            color_rgba,
        }
    }
}

pub struct TextSystem {
    font_system: FontSystem,
    swash_cache: SwashCache,
    buffer: Buffer,
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
        }
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
        let attrs = Attrs::new();
        self.buffer
            .set_text(&mut self.font_system, text, &attrs, Shaping::Advanced, None);
        self.buffer.shape_until_scroll(&mut self.font_system, true);
    }

    pub fn set_text_with_color(&mut self, text: &str, color_rgba: [u8; 4]) {
        let attrs = Attrs::new().color(Self::rgba_u8_to_color(color_rgba));
        self.buffer
            .set_text(&mut self.font_system, text, &attrs, Shaping::Advanced, None);
        self.buffer.shape_until_scroll(&mut self.font_system, true);
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

        let default_attrs = Attrs::new().color(Self::rgba_u8_to_color(default_color_rgba));
        let mut sanitized: Vec<StyledTextSpan> = spans
            .iter()
            .filter_map(|span| Self::sanitize_span(text, *span))
            .collect();

        sanitized.sort_by_key(|span| (span.start, span.end));
        if sanitized.is_empty() {
            self.set_text_with_color(text, default_color_rgba);
            return;
        }

        // Merge spans cùng màu nếu dính nhau/chồng nhau để giảm phân mảnh khi set_rich_text.
        let mut merged: Vec<StyledTextSpan> = Vec::with_capacity(sanitized.len());
        for span in sanitized {
            if let Some(last) = merged.last_mut() {
                if last.color_rgba == span.color_rgba && span.start <= last.end {
                    last.end = last.end.max(span.end);
                    continue;
                }
            }
            merged.push(span);
        }

        let mut segments: Vec<(&str, Attrs<'static>)> = Vec::with_capacity(merged.len() * 2 + 1);
        let mut cursor = 0usize;

        for span in merged {
            let start = span.start.max(cursor);
            if cursor < start {
                segments.push((&text[cursor..start], default_attrs.clone()));
            }
            if start < span.end {
                let attrs = Attrs::new().color(Self::rgba_u8_to_color(span.color_rgba));
                segments.push((&text[start..span.end], attrs));
                cursor = span.end;
            }
        }

        if cursor < text.len() {
            segments.push((&text[cursor..text.len()], default_attrs.clone()));
        }
        if segments.is_empty() {
            segments.push((text, default_attrs.clone()));
        }

        self.buffer.set_rich_text(
            &mut self.font_system,
            segments.into_iter(),
            &default_attrs,
            Shaping::Advanced,
            None,
        );
        self.buffer.shape_until_scroll(&mut self.font_system, true);
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
                });
            }
        }

        glyphs
    }

    fn rgba_u8_to_color(rgba: [u8; 4]) -> Color {
        Color::rgba(rgba[0], rgba[1], rgba[2], rgba[3])
    }

    fn rgba_f32_from_color(color: Color) -> [f32; 4] {
        [
            f32::from(color.r()) / 255.0,
            f32::from(color.g()) / 255.0,
            f32::from(color.b()) / 255.0,
            f32::from(color.a()) / 255.0,
        ]
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

        (start < end).then_some(StyledTextSpan::new(start, end, span.color_rgba))
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
