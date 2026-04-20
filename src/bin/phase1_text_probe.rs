use cosmic_text::Metrics;
use netherize_editor::text::layout_probe::{
    GlyphCoverageProbe, LayoutGlyphProbe, print_layout_probe, print_rasterization_probe,
};
use netherize_editor::text::text_system::TextSystem;
use std::collections::HashMap;

const PROBE_TEXT: &str = "Hello Apple Silicon";
const PROBE_SIZES: [f32; 3] = [12.0, 18.0, 24.0];

#[derive(Debug, Clone)]
struct SizeVerification {
    font_size: f32,
    glyph_count: usize,
    run_count: usize,
    text_span_width: f32,
    max_bitmap_width: u32,
    max_bitmap_height: u32,
    non_empty_bitmap_count: usize,
}

fn main() {
    println!("NetherizeEditor - Phase 1 CPU Text Layout & Rasterization Probe");
    println!("probe_text = {:?}", PROBE_TEXT);

    let mut reports = Vec::new();
    for (index, size) in PROBE_SIZES.iter().enumerate() {
        let include_font_scan = index == 0;
        let report = run_probe_and_verify(*size, include_font_scan)
            .unwrap_or_else(|err| panic!("verify failed at font_size={size}: {err}"));
        reports.push(report);
    }

    verify_size_scaling(&reports).unwrap_or_else(|err| panic!("verify3 failed: {err}"));

    println!();
    println!("=== Verify 3 Summary ===");
    for report in &reports {
        println!(
            "  size={} runs={} glyphs={} span_w={:.2} max_bitmap={}x{} non_empty_bitmaps={}",
            report.font_size,
            report.run_count,
            report.glyph_count,
            report.text_span_width,
            report.max_bitmap_width,
            report.max_bitmap_height,
            report.non_empty_bitmap_count
        );
    }
    println!("verify3 PASS: bounding boxes scale up and layout remains stable");
}

fn run_probe_and_verify(
    font_size: f32,
    include_font_scan: bool,
) -> Result<SizeVerification, String> {
    let line_height = font_size * 1.35;
    let mut text_system = TextSystem::new(
        Metrics::new(font_size, line_height),
        Some(1024.0),
        Some(256.0),
    );

    println!();
    println!(
        "=== Probe size={} line_height={:.2} ===",
        font_size, line_height
    );
    println!("locale = {}", text_system.locale());
    println!("discovered_system_faces = {}", text_system.face_count());

    if include_font_scan {
        let font_preview = text_system.sample_faces(10);
        println!();
        println!("=== Font Discovery (macOS DB sample) ===");
        for (index, face) in font_preview.iter().enumerate() {
            println!(
                "  face[{index}] id={:?} family='{}' ps='{}' style={:?} weight={} mono={} source={}",
                face.id,
                face.family,
                face.post_script_name,
                face.style,
                face.weight.0,
                face.monospaced,
                face.source
            );
        }
    }

    text_system.set_text(PROBE_TEXT);
    let layout_glyphs = print_layout_probe(&text_system);
    let coverages = print_rasterization_probe(&mut text_system, &layout_glyphs);

    verify_layout_invariants(&layout_glyphs)?;
    verify_raster_invariants(&coverages)?;

    let report = summarize_size(font_size, &layout_glyphs, &coverages);
    println!(
        "verify2 PASS: size={} x-order ok, finite numbers, raster stable, non-empty bitmaps={}",
        font_size, report.non_empty_bitmap_count
    );

    Ok(report)
}

fn verify_layout_invariants(layout: &[LayoutGlyphProbe]) -> Result<(), String> {
    if layout.is_empty() {
        return Err("layout has zero glyphs".to_string());
    }

    let mut last_x_per_run: HashMap<usize, f32> = HashMap::new();

    for glyph in layout {
        for (name, value) in [
            ("glyph_x", glyph.glyph_x),
            ("glyph_y", glyph.glyph_y),
            ("glyph_width", glyph.glyph_width),
            ("baseline_y", glyph.baseline_y),
        ] {
            if !value.is_finite() {
                return Err(format!(
                    "non-finite value for {name} at run={}, glyph={}",
                    glyph.run_index, glyph.glyph_index
                ));
            }
        }

        if let Some(prev_x) = last_x_per_run.get(&glyph.run_index) {
            if glyph.glyph_x <= *prev_x {
                return Err(format!(
                    "glyph x is not strictly increasing at run={} glyph={} prev_x={:.4} curr_x={:.4}",
                    glyph.run_index, glyph.glyph_index, prev_x, glyph.glyph_x
                ));
            }
        }
        last_x_per_run.insert(glyph.run_index, glyph.glyph_x);
    }

    Ok(())
}

fn verify_raster_invariants(coverages: &[GlyphCoverageProbe]) -> Result<(), String> {
    if coverages.is_empty() {
        return Err("rasterization returned zero glyphs".to_string());
    }

    let mut non_empty_bitmap_count = 0usize;
    for coverage in coverages {
        if !coverage.coverage_ratio.is_finite() || !coverage.average_alpha.is_finite() {
            return Err(format!(
                "non-finite raster stats at run={} glyph={}",
                coverage.run_index, coverage.glyph_index
            ));
        }

        if coverage.pixel_width > 0 && coverage.pixel_height > 0 {
            non_empty_bitmap_count += 1;
        }
    }

    if non_empty_bitmap_count < 3 {
        return Err(format!(
            "expected at least 3 non-empty bitmaps, got {non_empty_bitmap_count}"
        ));
    }

    Ok(())
}

fn summarize_size(
    font_size: f32,
    layout: &[LayoutGlyphProbe],
    coverages: &[GlyphCoverageProbe],
) -> SizeVerification {
    let run_count = layout
        .iter()
        .map(|glyph| glyph.run_index)
        .max()
        .map_or(0, |max_run| max_run + 1);
    let glyph_count = layout.len();

    let min_x = layout
        .iter()
        .map(|glyph| glyph.glyph_x)
        .fold(f32::INFINITY, f32::min);
    let max_x = layout
        .iter()
        .map(|glyph| glyph.glyph_x + glyph.glyph_width)
        .fold(f32::NEG_INFINITY, f32::max);
    let text_span_width = if min_x.is_finite() && max_x.is_finite() {
        max_x - min_x
    } else {
        0.0
    };

    let max_bitmap_width = coverages
        .iter()
        .map(|coverage| coverage.pixel_width)
        .max()
        .unwrap_or(0);
    let max_bitmap_height = coverages
        .iter()
        .map(|coverage| coverage.pixel_height)
        .max()
        .unwrap_or(0);
    let non_empty_bitmap_count = coverages
        .iter()
        .filter(|coverage| coverage.pixel_width > 0 && coverage.pixel_height > 0)
        .count();

    SizeVerification {
        font_size,
        glyph_count,
        run_count,
        text_span_width,
        max_bitmap_width,
        max_bitmap_height,
        non_empty_bitmap_count,
    }
}

fn verify_size_scaling(reports: &[SizeVerification]) -> Result<(), String> {
    if reports.len() < 2 {
        return Err("need at least 2 reports to compare scaling".to_string());
    }

    for pair in reports.windows(2) {
        let prev = &pair[0];
        let curr = &pair[1];

        if curr.glyph_count != prev.glyph_count {
            return Err(format!(
                "glyph count changed between size {} and {} ({} -> {})",
                prev.font_size, curr.font_size, prev.glyph_count, curr.glyph_count
            ));
        }
        if curr.run_count != prev.run_count {
            return Err(format!(
                "run count changed between size {} and {} ({} -> {})",
                prev.font_size, curr.font_size, prev.run_count, curr.run_count
            ));
        }
        if curr.text_span_width <= prev.text_span_width {
            return Err(format!(
                "text span width did not increase (size {} -> {}: {:.2} -> {:.2})",
                prev.font_size, curr.font_size, prev.text_span_width, curr.text_span_width
            ));
        }
        if curr.max_bitmap_height <= prev.max_bitmap_height {
            return Err(format!(
                "max bitmap height did not increase (size {} -> {}: {} -> {})",
                prev.font_size, curr.font_size, prev.max_bitmap_height, curr.max_bitmap_height
            ));
        }
    }

    Ok(())
}
