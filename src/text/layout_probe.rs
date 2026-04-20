use cosmic_text::{CacheKey, SwashContent, SwashImage, fontdb};

use crate::text::text_system::TextSystem;

#[derive(Debug, Clone)]
pub struct LayoutGlyphProbe {
    pub run_index: usize,
    pub line_index: usize,
    pub glyph_index: usize,
    pub cluster: String,
    pub text_range: (usize, usize),
    pub glyph_id: u16,
    pub font_id: fontdb::ID,
    pub font_post_script_name: String,
    pub glyph_x: f32,
    pub glyph_y: f32,
    pub glyph_width: f32,
    pub baseline_y: f32,
    pub physical_x: i32,
    pub physical_y: i32,
    pub cache_key: CacheKey,
}

#[derive(Debug, Clone)]
pub struct GlyphCoverageProbe {
    pub run_index: usize,
    pub glyph_index: usize,
    pub cluster: String,
    pub content: &'static str,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub covered_pixels: u32,
    pub total_pixels: u32,
    pub coverage_ratio: f32,
    pub average_alpha: f32,
}

pub fn print_layout_probe(system: &TextSystem) -> Vec<LayoutGlyphProbe> {
    let mut probes = Vec::new();
    let mut glyph_total = 0usize;
    let mut run_total = 0usize;

    println!();
    println!("=== Layout Probe ===");
    for (run_index, run) in system.buffer().layout_runs().enumerate() {
        run_total += 1;
        println!(
            "run[{run_index}] line={} rtl={} baseline_y={:.2} top_y={:.2} height={:.2} line_w={:.2}",
            run.line_i, run.rtl, run.line_y, run.line_top, run.line_height, run.line_w
        );

        for (glyph_index, glyph) in run.glyphs.iter().enumerate() {
            let cluster_raw = run.text.get(glyph.start..glyph.end).unwrap_or("");
            let cluster = escaped(cluster_raw);
            let physical = glyph.physical((0.0, run.line_y), 1.0);
            let font_name = system
                .resolve_face(glyph.font_id)
                .map(|face| face.post_script_name)
                .unwrap_or_else(|| "<missing-face>".to_string());

            println!(
                "  glyph[{glyph_index}] cluster='{cluster}' bytes={}..{} gid={} font={:?} ({}) x={:.2} y={:.2} w={:.2} physical=({}, {})",
                glyph.start,
                glyph.end,
                glyph.glyph_id,
                glyph.font_id,
                font_name,
                glyph.x,
                glyph.y,
                glyph.w,
                physical.x,
                physical.y
            );

            probes.push(LayoutGlyphProbe {
                run_index,
                line_index: run.line_i,
                glyph_index,
                cluster,
                text_range: (glyph.start, glyph.end),
                glyph_id: glyph.glyph_id,
                font_id: glyph.font_id,
                font_post_script_name: font_name,
                glyph_x: glyph.x,
                glyph_y: glyph.y,
                glyph_width: glyph.w,
                baseline_y: run.line_y,
                physical_x: physical.x,
                physical_y: physical.y,
                cache_key: physical.cache_key,
            });

            glyph_total += 1;
        }
    }

    println!("layout_summary runs={run_total} glyphs={glyph_total}");
    probes
}

pub fn print_rasterization_probe(
    system: &mut TextSystem,
    layout_glyphs: &[LayoutGlyphProbe],
) -> Vec<GlyphCoverageProbe> {
    let mut probes = Vec::new();
    let mut misses = 0usize;

    println!();
    println!("=== Rasterization Probe ===");
    for glyph in layout_glyphs {
        match system.rasterize_cache_key(glyph.cache_key) {
            Some(image) => {
                let (covered_pixels, total_pixels, average_alpha) = coverage_from_image(&image);
                let coverage_ratio = if total_pixels == 0 {
                    0.0
                } else {
                    covered_pixels as f32 / total_pixels as f32
                };
                let content = content_name(&image.content);

                println!(
                    "  run={} glyph={} cluster='{}' gid={} content={} img={}x{} place(left={}, top={}) covered={}/{} ({:.2}%) avg_alpha={:.2}",
                    glyph.run_index,
                    glyph.glyph_index,
                    glyph.cluster,
                    glyph.glyph_id,
                    content,
                    image.placement.width,
                    image.placement.height,
                    image.placement.left,
                    image.placement.top,
                    covered_pixels,
                    total_pixels,
                    coverage_ratio * 100.0,
                    average_alpha
                );

                probes.push(GlyphCoverageProbe {
                    run_index: glyph.run_index,
                    glyph_index: glyph.glyph_index,
                    cluster: glyph.cluster.clone(),
                    content,
                    pixel_width: image.placement.width,
                    pixel_height: image.placement.height,
                    covered_pixels,
                    total_pixels,
                    coverage_ratio,
                    average_alpha,
                });
            }
            None => {
                misses += 1;
                println!(
                    "  run={} glyph={} cluster='{}' gid={} raster=MISS",
                    glyph.run_index, glyph.glyph_index, glyph.cluster, glyph.glyph_id
                );
            }
        }
    }

    let total_pixels: u64 = probes
        .iter()
        .map(|probe| u64::from(probe.total_pixels))
        .sum();
    let covered_pixels: u64 = probes
        .iter()
        .map(|probe| u64::from(probe.covered_pixels))
        .sum();
    let ratio = if total_pixels == 0 {
        0.0
    } else {
        covered_pixels as f32 / total_pixels as f32
    };

    println!(
        "raster_summary glyphs={} misses={} covered={}/{} ({:.2}%)",
        probes.len(),
        misses,
        covered_pixels,
        total_pixels,
        ratio * 100.0
    );

    probes
}

fn escaped(text: &str) -> String {
    text.chars().flat_map(char::escape_default).collect()
}

fn content_name(content: &SwashContent) -> &'static str {
    match content {
        SwashContent::Mask => "mask",
        SwashContent::Color => "color",
        SwashContent::SubpixelMask => "subpixel_mask",
    }
}

fn coverage_from_image(image: &SwashImage) -> (u32, u32, f32) {
    match image.content {
        SwashContent::Mask => {
            let total_pixels = image.data.len() as u32;
            let covered_pixels = image.data.iter().filter(|alpha| **alpha > 0).count() as u32;
            let alpha_sum: u64 = image.data.iter().map(|alpha| u64::from(*alpha)).sum();
            let average_alpha = if total_pixels == 0 {
                0.0
            } else {
                alpha_sum as f32 / total_pixels as f32
            };
            (covered_pixels, total_pixels, average_alpha)
        }
        SwashContent::Color => {
            let mut covered_pixels = 0u32;
            let mut alpha_sum = 0u64;
            let mut total_pixels = 0u32;

            for pixel in image.data.chunks_exact(4) {
                let alpha = pixel[3];
                if alpha > 0 {
                    covered_pixels += 1;
                }
                alpha_sum += u64::from(alpha);
                total_pixels += 1;
            }

            let average_alpha = if total_pixels == 0 {
                0.0
            } else {
                alpha_sum as f32 / total_pixels as f32
            };
            (covered_pixels, total_pixels, average_alpha)
        }
        SwashContent::SubpixelMask => {
            let mut covered_pixels = 0u32;
            let mut alpha_sum = 0u64;
            let mut total_pixels = 0u32;

            for channels in image.data.chunks_exact(3) {
                let alpha = channels[0].max(channels[1]).max(channels[2]);
                if alpha > 0 {
                    covered_pixels += 1;
                }
                alpha_sum += u64::from(alpha);
                total_pixels += 1;
            }

            let average_alpha = if total_pixels == 0 {
                0.0
            } else {
                alpha_sum as f32 / total_pixels as f32
            };
            (covered_pixels, total_pixels, average_alpha)
        }
    }
}
