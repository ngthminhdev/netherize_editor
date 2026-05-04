#![allow(unused_imports)]

use std::time::{Duration, Instant};

use crate::render::region_pipeline::RegionDrawInstance;

use super::super::{RenderError, Renderer, helpers::draw_text_region};

const FRAME_TIME_WARN_THRESHOLD: Duration = Duration::from_millis(8);

impl Renderer {
    pub fn render(&mut self, region_instances: &[RegionDrawInstance]) -> Result<(), RenderError> {
        let frame_started_at = Instant::now();
        let frame = match self.surface_state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout => return Err(RenderError::Timeout),
            wgpu::CurrentSurfaceTexture::Occluded => return Err(RenderError::Occluded),
            wgpu::CurrentSurfaceTexture::Outdated => return Err(RenderError::Outdated),
            wgpu::CurrentSurfaceTexture::Lost => return Err(RenderError::Lost),
            wgpu::CurrentSurfaceTexture::Validation => return Err(RenderError::Validation),
        };

        // ── Tối ưu 1: Instance Batching ─────────────────────────────────────────
        // Gom TẤT CẢ region quads (base panels + mọi overlay layer) vào 1 Vec
        // duy nhất, upload 1 lần, rồi dùng draw_range() bên trong render pass.
        // Loại bỏ hoàn toàn các upload_instances() lặp lại giữa chừng render pass
        // (trước đây có ~10 lần/frame) làm nghẽn CPU→GPU pipeline.
        let base_count = region_instances.len() as u32;

        let ed_overlay_start = base_count;
        let ed_overlay_count = self.editor_overlay_chrome_instances.len() as u32;

        let welcome_start = ed_overlay_start + ed_overlay_count;
        let welcome_count = self.welcome_logo_chrome_instances.len() as u32;

        let leap_bg_start = welcome_start + welcome_count;
        let leap_bg_count = self.leap_label_bg_instances.len() as u32;

        let term_cursor_start = leap_bg_start + leap_bg_count;
        let term_cursor_count = self.terminal_cursor_instances.len() as u32;

        let buf_term_cursor_start = term_cursor_start + term_cursor_count;
        let buf_term_cursor_count = self.buffer_terminal_cursor_instances.len() as u32;

        let palette_start = buf_term_cursor_start + buf_term_cursor_count;
        let palette_count = self.palette_chrome_instances.len() as u32;

        let lsp_guide_start = palette_start + palette_count;
        let lsp_guide_count = self.lsp_guide_chrome_instances.len() as u32;

        let toast_start = lsp_guide_start + lsp_guide_count;
        let toast_count = self.toast_chrome_instances.len() as u32;

        let diag_hover_start = toast_start + toast_count;
        let diag_hover_count = self.diagnostic_hover_chrome_instances.len() as u32;

        // Build the flat merged Vec (immutable borrows all end here before upload).
        let total = (diag_hover_start + diag_hover_count) as usize;
        let mut all_instances: Vec<RegionDrawInstance> = Vec::with_capacity(total.max(64));
        all_instances.extend_from_slice(region_instances);
        all_instances.extend_from_slice(&self.editor_overlay_chrome_instances);
        all_instances.extend_from_slice(&self.welcome_logo_chrome_instances);
        all_instances.extend_from_slice(&self.leap_label_bg_instances);
        all_instances.extend_from_slice(&self.terminal_cursor_instances);
        all_instances.extend_from_slice(&self.buffer_terminal_cursor_instances);
        all_instances.extend_from_slice(&self.palette_chrome_instances);
        all_instances.extend_from_slice(&self.lsp_guide_chrome_instances);
        all_instances.extend_from_slice(&self.toast_chrome_instances);
        all_instances.extend_from_slice(&self.diagnostic_hover_chrome_instances);

        // Single upload — all borrows above are released.
        self.region_pipeline
            .upload_instances(&self.device, &self.queue, &all_instances);

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Netherize Encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Netherize RenderPass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            let viewport_width = self.surface_state.config.width;
            let viewport_height = self.surface_state.config.height;

            // 1. Panel backgrounds (no scissor) — single draw_range for all 34 regions.
            self.region_pipeline
                .draw_range(&mut pass, 0, base_count);

            // 2. Editor text + caret + cursor overlay + gutter.
            draw_text_region(
                &mut pass,
                self.editor_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.text_pipeline.draw(render_pass);
                    self.caret_pipeline.draw(render_pass);
                    self.editor_cursor_overlay_pipeline.draw(render_pass);
                    self.gutter_text_pipeline.draw(render_pass);
                },
            );

            if ed_overlay_count > 0 {
                draw_text_region(
                    &mut pass,
                    self.editor_overlay_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline
                            .draw_range(render_pass, ed_overlay_start, ed_overlay_count);
                    },
                );
            }

            // Cheat sheet logo: drawn after editor overlay chrome, before text.
            draw_text_region(
                &mut pass,
                self.image_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.image_pipeline.draw(render_pass);
                },
            );

            draw_text_region(
                &mut pass,
                self.editor_overlay_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.editor_overlay_text_pipeline.draw(render_pass);
                },
            );

            // 3. Welcome screen card/chrome + text.
            if welcome_count > 0 {
                draw_text_region(
                    &mut pass,
                    self.welcome_logo_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline
                            .draw_range(render_pass, welcome_start, welcome_count);
                    },
                );
            }
            // Welcome logo PNG: drawn after chrome background, before text.
            draw_text_region(
                &mut pass,
                self.welcome_image_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.welcome_image_pipeline.draw(render_pass);
                },
            );
            draw_text_region(
                &mut pass,
                self.welcome_logo_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.welcome_logo_text_pipeline.draw(render_pass);
                },
            );

            // 4. Explorer sidebar.
            draw_text_region(
                &mut pass,
                self.sidebar_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.sidebar_text_pipeline.draw(render_pass);
                },
            );

            // 4b. AI Chat text — history (scissor clipped to history bounds).
            draw_text_region(
                &mut pass,
                self.ai_chat_image_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.ai_chat_header_image_pipeline.draw(render_pass);
                    self.ai_chat_hero_image_pipeline.draw(render_pass);
                },
            );
            draw_text_region(
                &mut pass,
                self.ai_chat_history_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    // Draw only the history glyphs (before input batch).
                    let total = self.ai_chat_glyph_instances.len() as u32;
                    let hist_count = match &self.ai_chat_input_batch {
                        Some(batch) => batch.range.start,
                        None => total,
                    };
                    if hist_count > 0 {
                        self.ai_chat_text_pipeline.draw_range(
                            render_pass,
                            crate::render::text_pipeline::InstanceDrawRange {
                                start: 0,
                                count: hist_count,
                            },
                        );
                    }
                },
            );

            // 4c. AI Chat text — input box (scissor clipped to input bounds).
            if let Some(batch) = &self.ai_chat_input_batch {
                draw_text_region(
                    &mut pass,
                    Some(batch.scissor),
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.ai_chat_text_pipeline
                            .draw_range(render_pass, batch.range);
                    },
                );
            }

            // 5. Leap label overlay: dim + per-char bg + label chars.
            if !self.leap_label_glyph_instances.is_empty() {
                if leap_bg_count > 0 {
                    draw_text_region(
                        &mut pass,
                        self.leap_label_scissor,
                        viewport_width,
                        viewport_height,
                        |render_pass| {
                            self.region_pipeline
                                .draw_range(render_pass, leap_bg_start, leap_bg_count);
                        },
                    );
                }
                draw_text_region(
                    &mut pass,
                    self.leap_label_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.leap_label_text_pipeline.draw(render_pass);
                    },
                );
            }

            // 6. Terminal panel.
            draw_text_region(
                &mut pass,
                self.terminal_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.terminal_text_pipeline.draw(render_pass);
                },
            );
            if term_cursor_count > 0 {
                draw_text_region(
                    &mut pass,
                    self.terminal_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline
                            .draw_range(render_pass, term_cursor_start, term_cursor_count);
                    },
                );
            }

            if let Some(header_batch) = self.buffer_terminal_header_batch {
                let total_count = self.buffer_terminal_glyph_instances.len() as u32;
                let body_start = header_batch
                    .range
                    .start
                    .saturating_add(header_batch.range.count);
                let body_count = total_count.saturating_sub(body_start);
                if body_count > 0 {
                    draw_text_region(
                        &mut pass,
                        self.buffer_terminal_scissor,
                        viewport_width,
                        viewport_height,
                        |render_pass| {
                            self.buffer_terminal_text_pipeline.draw_range(
                                render_pass,
                                crate::render::text_pipeline::InstanceDrawRange {
                                    start: body_start,
                                    count: body_count,
                                },
                            );
                        },
                    );
                }
                draw_text_region(
                    &mut pass,
                    Some(header_batch.scissor),
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.buffer_terminal_text_pipeline
                            .draw_range(render_pass, header_batch.range);
                    },
                );
            } else {
                draw_text_region(
                    &mut pass,
                    self.buffer_terminal_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.buffer_terminal_text_pipeline.draw(render_pass);
                    },
                );
            }
            if buf_term_cursor_count > 0 {
                draw_text_region(
                    &mut pass,
                    self.buffer_terminal_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw_range(
                            render_pass,
                            buf_term_cursor_start,
                            buf_term_cursor_count,
                        );
                    },
                );
            }

            // 7. Welcome empty-state chrome/text. This layer is populated when the
            // app has no open buffers; keep it below the top/status bars and
            // below palette overlays, but above the center background.
            if welcome_count > 0 {
                draw_text_region(
                    &mut pass,
                    self.welcome_logo_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline
                            .draw_range(render_pass, welcome_start, welcome_count);
                    },
                );
            }
            draw_text_region(
                &mut pass,
                self.welcome_image_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.welcome_image_pipeline.draw(render_pass);
                },
            );
            draw_text_region(
                &mut pass,
                self.welcome_logo_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.welcome_logo_text_pipeline.draw(render_pass);
                },
            );

            // 8. TopBar.
            for batch in &self.topbar_text_batches {
                draw_text_region(
                    &mut pass,
                    Some(batch.scissor),
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.topbar_text_pipeline
                            .draw_range(render_pass, batch.range);
                    },
                );
            }

            // 9. StatusBar.
            draw_text_region(
                &mut pass,
                self.statusbar_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.statusbar_text_pipeline.draw(render_pass);
                },
            );

            // 10. Command palette chrome (scrim + box) above editor text — no scissor.
            if palette_count > 0 {
                self.region_pipeline
                    .draw_range(&mut pass, palette_start, palette_count);
            }

            // 11. Command palette / file picker text (topmost layer).
            draw_text_region(
                &mut pass,
                self.palette_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.palette_text_pipeline.draw(render_pass);
                },
            );

            if lsp_guide_count > 0 {
                draw_text_region(
                    &mut pass,
                    self.lsp_guide_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline
                            .draw_range(render_pass, lsp_guide_start, lsp_guide_count);
                    },
                );
            }
            draw_text_region(
                &mut pass,
                self.lsp_guide_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.lsp_guide_text_pipeline.draw(render_pass);
                },
            );

            if toast_count > 0 {
                draw_text_region(
                    &mut pass,
                    self.toast_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline
                            .draw_range(render_pass, toast_start, toast_count);
                    },
                );
            }
            draw_text_region(
                &mut pass,
                self.toast_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.toast_text_pipeline.draw(render_pass);
                },
            );

            // 12. Diagnostic hover popup (topmost overlay).
            if diag_hover_count > 0 {
                draw_text_region(
                    &mut pass,
                    self.diagnostic_hover_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        render_pass.set_scissor_rect(0, 0, viewport_width, viewport_height);
                        self.region_pipeline
                            .draw_range(render_pass, diag_hover_start, diag_hover_count);
                    },
                );
            }
            draw_text_region(
                &mut pass,
                self.diagnostic_hover_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    render_pass.set_scissor_rect(0, 0, viewport_width, viewport_height);
                    self.diagnostic_hover_text_pipeline.draw(render_pass);
                },
            );
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        let frame_time = frame_started_at.elapsed();
        if frame_time > FRAME_TIME_WARN_THRESHOLD {
            eprintln!(
                "[Renderer] slow frame: {:.2}ms before present (target <= 8.00ms for 120FPS, regions={}, size={}x{})",
                frame_time.as_secs_f64() * 1_000.0,
                region_instances.len(),
                self.surface_state.config.width,
                self.surface_state.config.height
            );
        }
        frame.present();
        Ok(())
    }
}
