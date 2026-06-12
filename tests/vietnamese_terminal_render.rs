//! Regression test for bug-047: Vietnamese diacritics lost when rendering
//! opencode output in the right-dock terminal ("bạn" → "ban").
//!
//! Covers the full pipeline the dock uses:
//!   PTY byte chunks → AnsiParser → TerminalGrid cells → TextSystem shaping
//! including chunk boundaries that split UTF-8 sequences and graphemes,
//! NFC vs NFD input, and SGR sequences interleaved between base + mark.

use cosmic_text::Metrics;
use netherize_editor::terminal::grid::TerminalGrid;
use netherize_editor::terminal::terminal_renderer::TerminalViewRenderer;
use netherize_editor::text::atlas::GlyphAtlas;
use netherize_editor::text::raster::rasterize_glyph_alpha;
use netherize_editor::text::text_system::TextSystem;

fn visible_row_text(grid: &TerminalGrid, row: usize) -> String {
    let mut out = String::new();
    for (r, _c, cell) in grid.iter_visible_cells() {
        if r == row {
            out.push(cell.ch);
        }
    }
    out.trim_end().to_string()
}

#[test]
fn nfc_vietnamese_renders_intact() {
    let mut grid = TerminalGrid::new(40, 5);
    grid.feed_bytes("bạn cụ thể kiểu".as_bytes());
    assert_eq!(visible_row_text(&grid, 0), "bạn cụ thể kiểu");
}

#[test]
fn nfd_vietnamese_composes_into_cells() {
    let mut grid = TerminalGrid::new(40, 5);
    // NFD: "bạn" = b + a + U+0323 (dot below) + n
    grid.feed_bytes("ba\u{0323}n".as_bytes());
    assert_eq!(visible_row_text(&grid, 0), "bạn");
}

#[test]
fn chunk_split_mid_utf8_sequence_still_decodes() {
    let mut grid = TerminalGrid::new(40, 5);
    let bytes = "bạn".as_bytes(); // ạ = 3 bytes at offset 1..4
    // Split inside the 'ạ' multi-byte sequence.
    grid.feed_bytes(&bytes[..2]);
    grid.feed_bytes(&bytes[2..]);
    assert_eq!(visible_row_text(&grid, 0), "bạn");
}

#[test]
fn chunk_split_between_base_and_combining_mark() {
    let mut grid = TerminalGrid::new(40, 5);
    let nfd = "ba\u{0323}n";
    let bytes = nfd.as_bytes();
    // Split right after the base 'a' (offset 2), so the combining mark
    // arrives in a separate PTY chunk.
    grid.feed_bytes(&bytes[..2]);
    grid.feed_bytes(&bytes[2..]);
    assert_eq!(visible_row_text(&grid, 0), "bạn");
}

#[test]
fn sgr_between_base_and_combining_mark() {
    let mut grid = TerminalGrid::new(40, 5);
    // opencode styles per-segment; a color reset can land between the
    // base char and its combining mark.
    grid.feed_bytes("ba".as_bytes());
    grid.feed_bytes(b"\x1b[0m");
    grid.feed_bytes("\u{0323}n".as_bytes());
    assert_eq!(visible_row_text(&grid, 0), "bạn");
}

#[test]
fn partially_composed_circumflex_plus_dot_below() {
    let mut grid = TerminalGrid::new(40, 5);
    // 'ô' (precomposed) + combining dot below → 'ộ'
    grid.feed_bytes("c\u{00F4}\u{0323}ng".as_bytes());
    assert_eq!(visible_row_text(&grid, 0), "cộng");
}

/// Shape every Vietnamese cell the way `TerminalViewRenderer::build_instances`
/// does — one char per cell-sized buffer — and assert a non-empty glyph raster
/// comes out for each.
#[test]
fn dock_shaping_produces_glyphs_for_vietnamese_cells() {
    let font_size = 14.0_f32;
    let line_h = 22.0_f32;
    let cell_w = (font_size * 0.6).max(1.0);

    let mut ts = TextSystem::new(Metrics::new(font_size, line_h), None, None);
    ts.set_font_family(Some("Google Sans Code"));

    for ch in "bạn cụ thể kiểu ộ ứ ề đ ữ".chars().filter(|c| *c != ' ') {
        ts.set_size(Some(cell_w), Some(line_h));
        ts.set_text(&ch.to_string());
        let glyphs = ts.collect_visible_glyphs(0.0, 0.0, [1.0, 1.0, 1.0, 1.0], None);
        assert!(
            !glyphs.is_empty(),
            "no glyph shaped for '{ch}' (U+{:04X})",
            ch as u32
        );
        let mut any_pixels = false;
        for vg in &glyphs {
            if let Some(raster) = rasterize_glyph_alpha(&mut ts, vg.cache_key) {
                if raster.alpha.iter().any(|a| *a > 0) {
                    any_pixels = true;
                }
            }
        }
        assert!(
            any_pixels,
            "glyph for '{ch}' (U+{:04X}) rasterized to empty bitmap",
            ch as u32
        );
    }
}

/// The right dock's text system gets `nerd_font_family` from `apply_theme`
/// (lifecycle.rs) so TUI icons render. Hack Nerd Font has no Vietnamese
/// Latin Extended Additional glyphs — shaping must still produce visible
/// glyphs via font fallback.
#[test]
fn nerd_font_family_still_shapes_vietnamese_via_fallback() {
    let font_size = 14.0_f32;
    let line_h = 22.0_f32;
    let cell_w = (font_size * 0.6).max(1.0);

    let mut ts = TextSystem::new(Metrics::new(font_size, line_h), None, None);
    ts.set_font_family(Some("Hack Nerd Font"));

    for ch in "ạứềộữ".chars() {
        ts.set_size(Some(cell_w), Some(line_h));
        ts.set_text(&ch.to_string());
        let glyphs = ts.collect_visible_glyphs(0.0, 0.0, [1.0, 1.0, 1.0, 1.0], None);
        assert!(
            !glyphs.is_empty(),
            "no glyph shaped for '{ch}' (U+{:04X}) with Hack Nerd Font",
            ch as u32
        );
        let mut any_pixels = false;
        for vg in &glyphs {
            if let Some(raster) = rasterize_glyph_alpha(&mut ts, vg.cache_key) {
                if raster.alpha.iter().any(|a| *a > 0) {
                    any_pixels = true;
                }
            }
        }
        assert!(
            any_pixels,
            "glyph for '{ch}' (U+{:04X}) rasterized empty with Hack Nerd Font",
            ch as u32
        );
    }
}

/// Full dock path with a real (headless) wgpu device and the real GlyphAtlas:
/// every non-space Vietnamese cell must produce at least one glyph instance.
#[test]
fn dock_build_instances_emits_glyph_per_vietnamese_cell() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    })) {
        Ok(adapter) => adapter,
        Err(err) => {
            eprintln!("skipping: no wgpu adapter available headless: {err}");
            return;
        }
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("vietnamese-test"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        experimental_features: Default::default(),
        memory_hints: Default::default(),
        trace: Default::default(),
    }))
    .expect("request_device");

    let mut atlas = GlyphAtlas::new(&device, 2048, 2048);
    let mut ts = TextSystem::new(Metrics::new(14.0, 22.0), None, None);
    ts.set_font_family(Some("Google Sans Code"));

    let text = "Chào bạn, cụ thể kiểu cộng đồng ngôn ngữ";
    let mut grid = TerminalGrid::new(60, 5);
    grid.feed_bytes(text.as_bytes());

    let view = TerminalViewRenderer::new(8.4, 22.0, 0.0, 0.0, 14.0);
    let instances = view.build_instances(
        &grid,
        &mut atlas,
        &queue,
        &mut ts,
        [1.0, 1.0, 1.0, 1.0],
        [0.0, 0.0, 0.0, 1.0],
        2000.0,
    );

    let non_space_cells = text.chars().filter(|c| *c != ' ').count();
    assert!(
        instances.len() >= non_space_cells,
        "expected at least {non_space_cells} glyph instances (one per non-space cell), got {}",
        instances.len()
    );
}
