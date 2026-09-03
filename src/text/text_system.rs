use std::sync::Arc;

use cosmic_text::{
    Attrs, AttrsList, Buffer, BufferLine, CacheKey, Color, Fallback, Family, FontSystem,
    LineEnding, LineIter, Metrics, PlatformFallback, Scroll, Shaping, SwashCache, SwashImage, Wrap,
    fontdb,
};

const BUNDLED_GOOGLE_SANS_CODE_FONT: &[u8] =
    include_bytes!("../../config/fonts/GoogleSansCode.ttf");
const BUNDLED_HACK_NERD_FONT: &[u8] = include_bytes!("../../config/fonts/HackNerdFont-Regular.ttf");
/// Family name of the bundled Nerd Font. It is the first fallback for any glyph the
/// configured text font lacks (PUA icons, powerline), so icons never resolve to a
/// random system font. It must not be the *primary* font for text: its cmap maps
/// many precomposed Vietnamese letters (ạ, ố, ề, …) to the bare base glyph, so
/// cosmic-text sees a hit and never falls back — the diacritics silently vanish.
pub const BUNDLED_NERD_FAMILY: &str = "Hack Nerd Font";

/// cosmic-text fallback chain: bundled Nerd Font first, then the platform list.
struct IconFirstFallback {
    common: Vec<&'static str>,
}

impl IconFirstFallback {
    fn new() -> Self {
        let mut common = vec![BUNDLED_NERD_FAMILY];
        common.extend_from_slice(PlatformFallback.common_fallback());
        Self { common }
    }
}

impl Fallback for IconFirstFallback {
    fn common_fallback(&self) -> &[&'static str] {
        &self.common
    }

    fn forbidden_fallback(&self) -> &[&'static str] {
        PlatformFallback.forbidden_fallback()
    }

    fn script_fallback(&self, script: unicode_script::Script, locale: &str) -> &[&'static str] {
        PlatformFallback.script_fallback(script, locale)
    }
}

fn new_font_system() -> FontSystem {
    // Mirrors `FontSystem::new()`, which is not parameterised over the fallback list.
    let locale = sys_locale::get_locale().unwrap_or_else(|| String::from("en-US"));
    let mut db = fontdb::Database::new();
    db.set_monospace_family("Noto Sans Mono");
    db.set_sans_serif_family("Open Sans");
    db.set_serif_family("DejaVu Serif");
    FontSystem::new_with_locale_and_db_and_fallback(locale, db, IconFirstFallback::new())
}

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
    pub underline: bool,
    pub strikethrough: bool,
}

impl StyledTextSpan {
    pub const fn new(start: usize, end: usize, color_rgba: [u8; 4]) -> Self {
        Self {
            start,
            end,
            color_rgba,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
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
            underline: false,
            strikethrough: false,
        }
    }

    pub const fn with_decoration(
        start: usize,
        end: usize,
        color_rgba: [u8; 4],
        bold: bool,
        italic: bool,
        underline: bool,
        strikethrough: bool,
    ) -> Self {
        Self {
            start,
            end,
            color_rgba,
            bold,
            italic,
            underline,
            strikethrough,
        }
    }
}

impl Default for StyledTextSpan {
    fn default() -> Self {
        Self {
            start: 0,
            end: 0,
            color_rgba: [255, 255, 255, 255],
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
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
        let mut font_system = new_font_system();
        register_bundled_fonts(&mut font_system);
        let swash_cache = SwashCache::new();
        let mut buffer = Buffer::new(&mut font_system, metrics);
        buffer.set_size(&mut font_system, width, height);
        // Break at word boundaries when possible; fall back to glyph boundary so that
        // long tokens without spaces (JWT, base64) wrap at the viewport edge instead of
        // overflowing the buffer or pushing subsequent lines to wrong Y positions.
        buffer.set_wrap(&mut font_system, Wrap::WordOrGlyph);

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

    pub fn buffer_metrics(&self) -> Metrics {
        self.buffer.metrics()
    }

    /// Compute pixel rectangles for underline decorations over the given byte ranges.
    /// Returns `Vec<[x, y, w, h]>` in the buffer's local coordinate space.
    pub fn underline_rects(
        &self,
        ranges: &[(usize, usize)],
        origin_x: f32,
        origin_y: f32,
    ) -> Vec<[f32; 4]> {
        if ranges.is_empty() {
            return Vec::new();
        }
        let mut rects = Vec::new();
        for run in self.buffer.layout_runs() {
            let run_y = origin_y + run.line_y;
            for glyph in run.glyphs.iter() {
                let g_start = glyph.start;
                let g_end = glyph.end;
                for &(r_start, r_end) in ranges {
                    if g_start < r_end && g_end > r_start {
                        let x = origin_x + glyph.x;
                        let w = glyph.w;
                        let underline_y = run_y + 2.0;
                        rects.push([x, underline_y, w, 1.0]);
                        break;
                    }
                }
            }
        }
        rects
    }

    pub fn tab_width(&self) -> u16 {
        self.buffer.tab_width()
    }

    pub fn set_tab_width(&mut self, tab_width: u16) {
        self.buffer.set_tab_width(&mut self.font_system, tab_width);
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
        // Per-line diff thay vì `Buffer::set_text` (clear + reshape TOÀN BỘ buffer):
        // `BufferLine::set_text` giữ nguyên shaping khi text/ending/attrs không đổi,
        // nên gõ 1 phím trên buffer lớn chỉ reshape đúng dòng bị sửa thay vì trả
        // ~800ms reshape mọi dòng mỗi keystroke (cold-edit spike, handoff §4.3).
        // ponytail: chèn/xoá newline làm lệch index → mọi dòng phía sau reshape lại;
        // nâng cấp là line-identity theo rope (virtualized shaping, blocker #2).
        let mut line_i = 0usize;
        for (range, ending) in LineIter::new(text) {
            let attrs_list = AttrsList::new(&attrs);
            match self.buffer.lines.get_mut(line_i) {
                Some(line) => {
                    line.set_text(&text[range], ending, attrs_list);
                }
                None => self.buffer.lines.push(BufferLine::new(
                    &text[range],
                    ending,
                    attrs_list,
                    Shaping::Advanced,
                )),
            }
            line_i += 1;
        }
        // Giữ contract của upstream set_text: luôn kết thúc bằng một dòng
        // LineEnding::None (caret đứng được sau newline cuối).
        let needs_trailing = line_i == 0
            || self
                .buffer
                .lines
                .get(line_i - 1)
                .is_none_or(|line| line.ending() != LineEnding::None);
        if needs_trailing {
            let attrs_list = AttrsList::new(&attrs);
            match self.buffer.lines.get_mut(line_i) {
                Some(line) => {
                    line.set_text("", LineEnding::None, attrs_list);
                }
                None => self.buffer.lines.push(BufferLine::new(
                    "",
                    LineEnding::None,
                    attrs_list,
                    Shaping::Advanced,
                )),
            }
            line_i += 1;
        }
        self.buffer.lines.truncate(line_i);
        self.buffer.set_scroll(Scroll::default());
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

        // Fix trailing newline empty line missing in set_rich_text
        if self
            .buffer
            .lines
            .last()
            .map(|line| line.ending())
            .unwrap_or(cosmic_text::LineEnding::None)
            != cosmic_text::LineEnding::None
        {
            self.buffer.lines.push(cosmic_text::BufferLine::new(
                "",
                cosmic_text::LineEnding::None,
                cosmic_text::AttrsList::new(&default_attrs),
                Shaping::Advanced,
            ));
        }

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
    ///
    /// `viewport_clip` (None = không cull) là cặp `(top_y, bottom_y)` trong window
    /// space. Khi Some, các run nằm hoàn toàn ngoài khoảng này bị bỏ qua —
    /// quan trọng với file lớn (10k dòng @ 4K) để tránh build instance cho
    /// mọi dòng. Cosmic-text trả `layout_runs()` theo thứ tự `line_top` tăng
    /// dần nên có thể `break` sớm khi vượt đáy.
    pub fn collect_visible_glyphs(
        &self,
        origin_x: f32,
        origin_y: f32,
        fallback_color: [f32; 4],
        viewport_clip: Option<(f32, f32)>,
    ) -> Vec<VisibleGlyph> {
        self.collect_visible_glyphs_with_folds(
            origin_x,
            origin_y,
            fallback_color,
            viewport_clip,
            &[],
        )
    }

    pub fn collect_visible_glyphs_with_folds(
        &self,
        origin_x: f32,
        origin_y: f32,
        fallback_color: [f32; 4],
        viewport_clip: Option<(f32, f32)>,
        folded_ranges: &[(usize, usize)],
    ) -> Vec<VisibleGlyph> {
        let mut glyphs = Vec::new();
        let has_folds = !folded_ranges.is_empty();

        for run in self.buffer.layout_runs() {
            let line_i = run.line_i;
            if has_folds && is_line_hidden_by_fold(line_i, folded_ranges) {
                continue;
            }

            // Adjust Y position by subtracting accumulated offset from folded lines
            let y_offset = folded_visual_y_offset_before(line_i, run.line_height, folded_ranges);
            let adjusted_line_y = run.line_y - y_offset;
            let adjusted_line_top = run.line_top - y_offset;

            if let Some((clip_top, clip_bottom)) = viewport_clip {
                let line_top_window = origin_y + adjusted_line_top;
                let line_bottom_window = line_top_window + run.line_height;
                if line_bottom_window < clip_top {
                    continue;
                }
                if line_top_window > clip_bottom {
                    break;
                }
            }

            let mut line_visible_bytes = 0usize;

            for glyph in run.glyphs {
                let physical = glyph.physical((origin_x, origin_y + adjusted_line_y), 1.0);
                let color = glyph
                    .color_opt
                    .map(Self::rgba_f32_from_color)
                    .unwrap_or(fallback_color);

                // Check if this line is folded (truncated)
                if folded_ranges.contains(&(line_i, line_i)) {
                    let glyph_bytes = glyph.end.saturating_sub(glyph.start);
                    if line_visible_bytes + glyph_bytes > 100 {
                        // Stop adding glyphs after 100 bytes
                        break;
                    }
                    line_visible_bytes += glyph_bytes;
                }

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

fn is_line_hidden_by_fold(line_i: usize, folded_ranges: &[(usize, usize)]) -> bool {
    folded_ranges
        .iter()
        .any(|&(s, e)| s < line_i && line_i <= e)
}

fn folded_visual_y_offset_before(
    line_i: usize,
    line_height: f32,
    folded_ranges: &[(usize, usize)],
) -> f32 {
    folded_ranges
        .iter()
        .filter(|&&(_s, e)| e < line_i)
        .map(|&(s, e)| e.saturating_sub(s) as f32 * line_height)
        .sum()
}

fn apply_family<'a>(attrs: Attrs<'a>, family: Option<&'a str>) -> Attrs<'a> {
    match family {
        Some(name) if !name.trim().is_empty() => attrs.family(Family::Name(name)),
        _ => attrs,
    }
}

fn register_bundled_fonts(font_system: &mut FontSystem) {
    let db = font_system.db_mut();
    // Load system fonts first to provide fallback for emoji and Unicode characters.
    // Bundled fonts loaded after will take priority for their supported glyphs.
    db.load_system_fonts();
    db.load_font_source(fontdb::Source::Binary(Arc::new(
        BUNDLED_GOOGLE_SANS_CODE_FONT.to_vec(),
    )));
    db.load_font_source(fontdb::Source::Binary(Arc::new(
        BUNDLED_HACK_NERD_FONT.to_vec(),
    )));
}

#[cfg(test)]
mod tests {
    use cosmic_text::{CacheKey, Metrics};

    use super::{BUNDLED_NERD_FAMILY, StyledTextSpan, TextSystem};

    #[test]
    fn unbounded_height_shapes_deep_lines() {
        let total_lines = 96usize;
        let text = (0..total_lines)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");

        let mut system = TextSystem::new(Metrics::new(16.0, 22.0), Some(900.0), None);
        system.set_text(&text);
        let glyphs = system.collect_visible_glyphs(0.0, 0.0, [1.0, 1.0, 1.0, 1.0], None);
        let max_line = glyphs.iter().map(|glyph| glyph.line_i).max().unwrap_or(0);

        assert!(
            max_line >= total_lines.saturating_sub(2),
            "deep lines were not shaped: max_line={max_line}, total_lines={total_lines}"
        );
    }

    #[test]
    fn folded_glyph_collection_excludes_hidden_end_line() {
        let mut system = TextSystem::new(Metrics::new(16.0, 22.0), Some(900.0), None);
        system.set_text("start\nhidden\nend\nnext");

        let glyphs = system.collect_visible_glyphs_with_folds(
            0.0,
            0.0,
            [1.0, 1.0, 1.0, 1.0],
            None,
            &[(0, 2)],
        );
        let lines = glyphs.iter().map(|glyph| glyph.line_i).collect::<Vec<_>>();

        assert!(lines.contains(&0));
        assert!(!lines.contains(&1));
        assert!(!lines.contains(&2));
        assert!(lines.contains(&3));
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

        let glyphs = system.collect_visible_glyphs(0.0, 0.0, [1.0, 1.0, 1.0, 1.0], None);
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

        let glyphs = system.collect_visible_glyphs(0.0, 0.0, [1.0, 1.0, 1.0, 1.0], None);
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

        let glyphs = system.collect_visible_glyphs(0.0, 0.0, [1.0, 1.0, 1.0, 1.0], None);
        assert!(
            !glyphs.is_empty(),
            "missing font family should still shape text via fallback"
        );
    }

    #[test]
    fn tab_width_controls_shaped_tab_advance() {
        let mut system = TextSystem::new(Metrics::new(16.0, 22.0), Some(900.0), None);
        system.set_tab_width(8);
        system.set_text("\tvalue");
        let width_with_eight = system
            .buffer()
            .layout_runs()
            .next()
            .map(|run| run.line_w)
            .unwrap_or(0.0);

        system.set_tab_width(4);
        let width_with_four = system
            .buffer()
            .layout_runs()
            .next()
            .map(|run| run.line_w)
            .unwrap_or(0.0);

        assert!(width_with_four > 0.0);
        assert!(
            width_with_four < width_with_eight,
            "tab width should shrink shaped tab advance: four={width_with_four}, eight={width_with_eight}"
        );
    }

    #[test]
    fn incremental_set_text_with_color_matches_full_set_text_semantics() {
        let mut system = TextSystem::new(Metrics::new(16.0, 22.0), Some(900.0), None);
        let color = [0xF0, 0xF0, 0xF0, 0xFF];

        // Trailing-newline contract: một dòng rỗng LineEnding::None ở cuối.
        system.set_text_with_color("abc\n", color);
        assert_eq!(system.buffer().layout_runs().count(), 2);

        // Sửa một dòng giữa buffer: nội dung mới phải hiện ra, số dòng đúng.
        system.set_text_with_color("abc\ndef\nghi", color);
        assert_eq!(system.buffer().layout_runs().count(), 3);
        system.set_text_with_color("abc\nXYZ\nghi", color);
        let lines: Vec<String> = system
            .buffer()
            .lines
            .iter()
            .map(|l| l.text().to_string())
            .collect();
        assert_eq!(lines, ["abc", "XYZ", "ghi"]);

        // Thu ngắn buffer phải truncate dòng thừa.
        system.set_text_with_color("solo", color);
        assert_eq!(system.buffer().layout_runs().count(), 1);
        assert_eq!(system.buffer().lines.len(), 1);
    }

    #[test]
    fn test_trailing_newline_layout_runs() {
        let mut system = TextSystem::new(Metrics::new(16.0, 22.0), Some(900.0), None);
        system.set_text_with_spans(
            "abc\n",
            [0xF0, 0xF0, 0xF0, 0xFF],
            &[StyledTextSpan::new(0, 3, [0x22, 0x88, 0xFF, 0xFF])],
        );
        let runs: Vec<_> = system.buffer().layout_runs().collect();
        assert_eq!(runs.len(), 2);
        assert!(!runs[0].glyphs.is_empty());
        assert!(runs[1].glyphs.is_empty());
    }
    fn single_glyph(ts: &mut TextSystem, ch: char) -> CacheKey {
        ts.set_text(&ch.to_string());
        let glyphs = ts.collect_visible_glyphs(0.0, 0.0, [1.0, 1.0, 1.0, 1.0], None);
        assert_eq!(
            glyphs.len(),
            1,
            "expected one glyph for U+{:04X}",
            ch as u32
        );
        glyphs[0].cache_key
    }

    /// Regression: the terminal used the Nerd Font as its text font, whose cmap maps
    /// ạ/ố/ề/ớ/ụ to the bare base glyph — diacritics vanished. Text must shape with
    /// the editor font, where every Vietnamese letter has its own glyph.
    #[test]
    fn vietnamese_letters_keep_their_diacritics_in_the_editor_font() {
        let mut ts = TextSystem::new(Metrics::new(14.0, 20.0), Some(200.0), Some(40.0));
        ts.set_font_family(Some("Google Sans Code"));
        let base = single_glyph(&mut ts, 'a');
        for ch in [
            '\u{1ea1}', '\u{1ed1}', '\u{1ed5}', '\u{1ec1}', '\u{1edb}', '\u{1ee5}',
        ] {
            let key = single_glyph(&mut ts, ch);
            assert_eq!(
                key.font_id, base.font_id,
                "U+{:04X} left the primary font",
                ch as u32
            );
            assert_ne!(
                key.glyph_id, base.glyph_id,
                "U+{:04X} collapsed to the base glyph",
                ch as u32
            );
        }
    }

    /// PUA icons missing from the editor font must resolve to the bundled Nerd Font,
    /// never to whatever system font happens to carry that code point.
    #[test]
    fn pua_icons_fall_back_to_the_bundled_nerd_font() {
        let mut nerd = TextSystem::new(Metrics::new(14.0, 20.0), Some(200.0), Some(40.0));
        nerd.set_font_family(Some(BUNDLED_NERD_FAMILY));
        let mut ts = TextSystem::new(Metrics::new(14.0, 20.0), Some(200.0), Some(40.0));
        ts.set_font_family(Some("Google Sans Code"));
        for icon in ['\u{e0a0}', '\u{f0e7}', '\u{f15c}', '\u{e5ff}', '\u{f0001}'] {
            let expected = single_glyph(&mut nerd, icon);
            let got = single_glyph(&mut ts, icon);
            assert_eq!(
                got.glyph_id, expected.glyph_id,
                "U+{:04X} glyph",
                icon as u32
            );
        }
    }
}
