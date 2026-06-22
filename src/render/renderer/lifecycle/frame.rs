#![allow(unused_imports)]

use std::time::{Duration, Instant};

use crate::render::region_pipeline::RegionDrawInstance;

use super::super::{RenderError, Renderer, helpers::draw_text_region};

const FRAME_TIME_WARN_THRESHOLD: Duration = Duration::from_millis(8);

impl Renderer {
    pub fn render(&mut self, region_instances: &[RegionDrawInstance]) -> Result<(), RenderError> {
        // NOTE: `get_current_texture()` BLOCKS on the Fifo (vsync) swapchain until a
        // backbuffer is free, so timing the frame from before it conflates the
        // vsync wait with real work (the warning then fires every frame at 120 Hz).
        // Start the clock AFTER acquisition so the warning measures CPU encode cost.
        let frame = match self.surface_state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout => return Err(RenderError::Timeout),
            wgpu::CurrentSurfaceTexture::Occluded => return Err(RenderError::Occluded),
            wgpu::CurrentSurfaceTexture::Outdated => return Err(RenderError::Outdated),
            wgpu::CurrentSurfaceTexture::Lost => return Err(RenderError::Lost),
            wgpu::CurrentSurfaceTexture::Validation => return Err(RenderError::Validation),
        };
        let frame_started_at = Instant::now();

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

        let term_cell_bg_start = leap_bg_start + leap_bg_count;
        let term_cell_bg_count = self.terminal_cell_background_instances.len() as u32;

        let buf_term_cell_bg_start = term_cell_bg_start + term_cell_bg_count;
        let buf_term_cell_bg_count = self.buffer_terminal_cell_background_instances.len() as u32;

        let term_cursor_start = buf_term_cell_bg_start + buf_term_cell_bg_count;
        let term_cursor_count = self.terminal_cursor_instances.len() as u32;

        let buf_term_cursor_start = term_cursor_start + term_cursor_count;
        let buf_term_cursor_count = self.buffer_terminal_cursor_instances.len() as u32;

        let right_term_cell_bg_start = buf_term_cursor_start + buf_term_cursor_count;
        let right_term_cell_bg_count = self.right_terminal_cell_background_instances.len() as u32;

        let right_term_cursor_start = right_term_cell_bg_start + right_term_cell_bg_count;
        let right_term_cursor_count = self.right_terminal_cursor_instances.len() as u32;

        let markdown_preview_chrome_start = right_term_cursor_start + right_term_cursor_count;
        let markdown_preview_chrome_count = self.markdown_preview_chrome_instances.len() as u32;

        // Suggestion popup chrome is a separate range drawn *after* bubble text.
        let markdown_preview_overlay_chrome_start =
            markdown_preview_chrome_start + markdown_preview_chrome_count;
        let markdown_preview_overlay_chrome_count =
            self.markdown_preview_overlay_chrome_instances.len() as u32;

        let test_runner_chrome_start =
            markdown_preview_overlay_chrome_start + markdown_preview_overlay_chrome_count;
        let test_runner_chrome_count = self.test_runner_chrome_instances.len() as u32;

        let palette_start = test_runner_chrome_start + test_runner_chrome_count;
        let palette_count = self.palette_chrome_instances.len() as u32;

        let lsp_guide_start = palette_start + palette_count;
        let lsp_guide_count = self.lsp_guide_chrome_instances.len() as u32;

        let system_dep_start = lsp_guide_start + lsp_guide_count;
        let system_dep_count = self.system_dep_chrome_instances.len() as u32;

        let whichkey_start = system_dep_start + system_dep_count;
        let whichkey_count = self.whichkey_chrome_instances.len() as u32;

        let toast_start = whichkey_start + whichkey_count;
        let toast_count = self.toast_chrome_instances.len() as u32;

        let diag_hover_start = toast_start + toast_count;
        let diag_hover_count = self.diagnostic_hover_chrome_instances.len() as u32;

        // NetherCanvas draws last (on top of everything) when active.
        let canvas_start = diag_hover_start + diag_hover_count;
        let canvas_count = self.canvas_chrome_instances.len() as u32;

        // Pointer-hover handle highlight — drawn on top of EVERYTHING (incl.
        // the canvas) so the accent band always sits above dock edges / cards.
        let hover_start = canvas_start + canvas_count;
        let hover_count = if self.pointer_hover_highlight.is_some() { 1u32 } else { 0u32 };

        // Build the flat merged Vec (immutable borrows all end here before upload).
        let total = (hover_start + hover_count) as usize;
        let mut all_instances: Vec<RegionDrawInstance> = Vec::with_capacity(total.max(64));
        all_instances.extend_from_slice(region_instances);
        all_instances.extend_from_slice(&self.editor_overlay_chrome_instances);
        all_instances.extend_from_slice(&self.welcome_logo_chrome_instances);
        all_instances.extend_from_slice(&self.leap_label_bg_instances);
        all_instances.extend_from_slice(&self.terminal_cell_background_instances);
        all_instances.extend_from_slice(&self.buffer_terminal_cell_background_instances);
        all_instances.extend_from_slice(&self.terminal_cursor_instances);
        all_instances.extend_from_slice(&self.buffer_terminal_cursor_instances);
        all_instances.extend_from_slice(&self.right_terminal_cell_background_instances);
        all_instances.extend_from_slice(&self.right_terminal_cursor_instances);
        all_instances.extend_from_slice(&self.markdown_preview_chrome_instances);
        all_instances.extend_from_slice(&self.markdown_preview_overlay_chrome_instances);
        all_instances.extend_from_slice(&self.test_runner_chrome_instances);
        all_instances.extend_from_slice(&self.palette_chrome_instances);
        all_instances.extend_from_slice(&self.lsp_guide_chrome_instances);
        all_instances.extend_from_slice(&self.system_dep_chrome_instances);
        all_instances.extend_from_slice(&self.whichkey_chrome_instances);
        all_instances.extend_from_slice(&self.toast_chrome_instances);
        all_instances.extend_from_slice(&self.diagnostic_hover_chrome_instances);
        all_instances.extend_from_slice(&self.canvas_chrome_instances);
        if let Some(quad) = self.pointer_hover_highlight {
            all_instances.push(quad);
        }

        // Single upload — all borrows above are released.
        self.region_pipeline
            .upload_instances(&self.device, &self.queue, &all_instances);

        // Merge bottom-dock outer tab strip icons into the sidebar icon
        // pipeline so they can be drawn with their own scissor via draw_range.
        let sidebar_icon_count = self.sidebar_icon_instances.len() as u32;
        self.sidebar_icon_instances
            .extend(self.bottom_dock_tab_icon_instances.drain(..));
        self.sidebar_icon_pipeline.upload_instances(
            &self.device,
            &self.sidebar_icon_instances,
            [
                self.surface_state.config.width,
                self.surface_state.config.height,
            ],
        );

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
            self.region_pipeline.draw_range(&mut pass, 0, base_count);

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
                        self.region_pipeline.draw_range(
                            render_pass,
                            ed_overlay_start,
                            ed_overlay_count,
                        );
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
                    self.editor_overlay_icon_pipeline.draw(render_pass);
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
            // Welcome logo/icons: drawn after chrome background, before text.
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
                    self.welcome_icon_pipeline.draw(render_pass);
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

            // 4. Explorer sidebar icons + bottom-dock outer tab strip icons.
            if sidebar_icon_count > 0 {
                draw_text_region(
                    &mut pass,
                    self.sidebar_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.sidebar_icon_pipeline
                            .draw_range(render_pass, 0, sidebar_icon_count);
                    },
                );
            }
            let bottom_dock_icon_count =
                self.sidebar_icon_instances.len() as u32 - sidebar_icon_count;
            if bottom_dock_icon_count > 0 {
                let outer_tab_scissor = self.terminal_outer_tab_batch.map(|b| b.scissor);
                draw_text_region(
                    &mut pass,
                    outer_tab_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.sidebar_icon_pipeline.draw_range(
                            render_pass,
                            sidebar_icon_count,
                            bottom_dock_icon_count,
                        );
                    },
                );
            }
            draw_text_region(
                &mut pass,
                self.sidebar_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.sidebar_text_pipeline.draw(render_pass);
                },
            );

            // 4b. Markdown Preview text — history (scissor clipped to history bounds).
            draw_text_region(
                &mut pass,
                self.markdown_preview_image_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.markdown_preview_header_image_pipeline
                        .draw(render_pass);
                    self.markdown_preview_hero_image_pipeline.draw(render_pass);
                },
            );
            if markdown_preview_chrome_count > 0 {
                draw_text_region(
                    &mut pass,
                    self.markdown_preview_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw_range(
                            render_pass,
                            markdown_preview_chrome_start,
                            markdown_preview_chrome_count,
                        );
                    },
                );
            }
            // 4b-i. Draw message-bubble glyphs up to where suggestion text begins.
            //        If there is no suggestion, this draws all history glyphs at once.
            {
                let total = self.markdown_preview_glyph_instances.len() as u32;
                let hist_count = match &self.markdown_preview_input_batch {
                    Some(batch) => batch.range.start,
                    None => total,
                };
                let pre_suggestion = self
                    .markdown_preview_overlay_glyph_start
                    .unwrap_or(hist_count)
                    .min(hist_count);
                if pre_suggestion > 0 {
                    draw_text_region(
                        &mut pass,
                        self.markdown_preview_scissor,
                        viewport_width,
                        viewport_height,
                        |render_pass| {
                            self.markdown_preview_text_pipeline.draw_range(
                                render_pass,
                                crate::render::text_pipeline::InstanceDrawRange {
                                    start: 0,
                                    count: pre_suggestion,
                                },
                            );
                        },
                    );
                }
                // 4b-ii. Suggestion popup chrome on top of bubble text.
                if markdown_preview_overlay_chrome_count > 0 {
                    draw_text_region(
                        &mut pass,
                        self.markdown_preview_scissor,
                        viewport_width,
                        viewport_height,
                        |render_pass| {
                            self.region_pipeline.draw_range(
                                render_pass,
                                markdown_preview_overlay_chrome_start,
                                markdown_preview_overlay_chrome_count,
                            );
                        },
                    );
                }
                // 4b-iii. Suggestion text on top of popup chrome.
                if let Some(sug_start) = self.markdown_preview_overlay_glyph_start {
                    let sug_count = hist_count.saturating_sub(sug_start);
                    if sug_count > 0 {
                        draw_text_region(
                            &mut pass,
                            self.markdown_preview_scissor,
                            viewport_width,
                            viewport_height,
                            |render_pass| {
                                self.markdown_preview_text_pipeline.draw_range(
                                    render_pass,
                                    crate::render::text_pipeline::InstanceDrawRange {
                                        start: sug_start,
                                        count: sug_count,
                                    },
                                );
                            },
                        );
                    }
                }
            }

            // 4c. Markdown Preview text — input box (scissor clipped to input bounds).
            if let Some(batch) = &self.markdown_preview_input_batch {
                draw_text_region(
                    &mut pass,
                    Some(batch.scissor),
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.markdown_preview_text_pipeline
                            .draw_range(render_pass, batch.range);
                    },
                );
            }

            // 4d. Test Runner panel: chrome (case rows / fields) then text.
            if test_runner_chrome_count > 0 {
                draw_text_region(
                    &mut pass,
                    self.test_runner_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw_range(
                            render_pass,
                            test_runner_chrome_start,
                            test_runner_chrome_count,
                        );
                    },
                );
            }
            if !self.test_runner_icon_instances.is_empty() {
                draw_text_region(
                    &mut pass,
                    self.test_runner_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.test_runner_icon_pipeline.draw(render_pass);
                    },
                );
            }
            if !self.test_runner_glyph_instances.is_empty() {
                draw_text_region(
                    &mut pass,
                    self.test_runner_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.test_runner_text_pipeline.draw(render_pass);
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
                            self.region_pipeline.draw_range(
                                render_pass,
                                leap_bg_start,
                                leap_bg_count,
                            );
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

            // 5b. Bottom-dock outer tab strip text — drawn above terminal bodies.
            if let Some(outer_tab_batch) = self.terminal_outer_tab_batch {
                draw_text_region(
                    &mut pass,
                    Some(outer_tab_batch.scissor),
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.terminal_text_pipeline
                            .draw_range(render_pass, outer_tab_batch.range);
                    },
                );
            }

            // 6. Terminal panel.
            if term_cell_bg_count > 0 {
                draw_text_region(
                    &mut pass,
                    self.terminal_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw_range(
                            render_pass,
                            term_cell_bg_start,
                            term_cell_bg_count,
                        );
                    },
                );
            }
            if let Some(body_batch) = self.terminal_body_batch {
                draw_text_region(
                    &mut pass,
                    Some(body_batch.scissor),
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.terminal_text_pipeline
                            .draw_range(render_pass, body_batch.range);
                    },
                );
                if let Some(tab_batch) = self.terminal_tab_bar_batch {
                    draw_text_region(
                        &mut pass,
                        Some(tab_batch.scissor),
                        viewport_width,
                        viewport_height,
                        |render_pass| {
                            self.terminal_text_pipeline
                                .draw_range(render_pass, tab_batch.range);
                        },
                    );
                }
            } else {
                draw_text_region(
                    &mut pass,
                    self.terminal_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.terminal_text_pipeline.draw(render_pass);
                    },
                );
            }
            if term_cursor_count > 0 {
                draw_text_region(
                    &mut pass,
                    self.terminal_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw_range(
                            render_pass,
                            term_cursor_start,
                            term_cursor_count,
                        );
                    },
                );
            }

            if buf_term_cell_bg_count > 0 {
                draw_text_region(
                    &mut pass,
                    self.buffer_terminal_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw_range(
                            render_pass,
                            buf_term_cell_bg_start,
                            buf_term_cell_bg_count,
                        );
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

            // 6b. Right-dock terminal (opencode) — dedicated pipeline.
            if right_term_cell_bg_count > 0 {
                draw_text_region(
                    &mut pass,
                    self.right_terminal_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw_range(
                            render_pass,
                            right_term_cell_bg_start,
                            right_term_cell_bg_count,
                        );
                    },
                );
            }
            if let Some(body_batch) = self.right_terminal_body_batch {
                draw_text_region(
                    &mut pass,
                    Some(body_batch.scissor),
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.right_terminal_text_pipeline
                            .draw_range(render_pass, body_batch.range);
                    },
                );
            } else {
                draw_text_region(
                    &mut pass,
                    self.right_terminal_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.right_terminal_text_pipeline.draw(render_pass);
                    },
                );
            }
            if right_term_cursor_count > 0 {
                draw_text_region(
                    &mut pass,
                    self.right_terminal_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw_range(
                            render_pass,
                            right_term_cursor_start,
                            right_term_cursor_count,
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
                    self.welcome_icon_pipeline.draw(render_pass);
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

            // 8b. TopBar icons.
            draw_text_region(
                &mut pass,
                self.topbar_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.topbar_icon_pipeline.draw(render_pass);
                },
            );

            // 8c. TopBar logo.
            draw_text_region(
                &mut pass,
                self.topbar_logo_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.topbar_logo_image_pipeline.draw(render_pass);
                },
            );

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

            // 11. Command palette / file picker icons + text (topmost layer).
            draw_text_region(
                &mut pass,
                self.palette_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.palette_icon_pipeline.draw(render_pass);
                },
            );
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
                        self.region_pipeline.draw_range(
                            render_pass,
                            lsp_guide_start,
                            lsp_guide_count,
                        );
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

            if system_dep_count > 0 {
                draw_text_region(
                    &mut pass,
                    self.system_dep_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw_range(
                            render_pass,
                            system_dep_start,
                            system_dep_count,
                        );
                    },
                );
            }
            draw_text_region(
                &mut pass,
                self.system_dep_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.system_dep_text_pipeline.draw(render_pass);
                },
            );

            if whichkey_count > 0 {
                draw_text_region(
                    &mut pass,
                    self.whichkey_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw_range(
                            render_pass,
                            whichkey_start,
                            whichkey_count,
                        );
                    },
                );
            }
            draw_text_region(
                &mut pass,
                self.whichkey_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.whichkey_text_pipeline.draw(render_pass);
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
                        self.region_pipeline.draw_range(
                            render_pass,
                            diag_hover_start,
                            diag_hover_count,
                        );
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

            // 13. NetherCanvas full-screen layer — drawn last so its opaque
            // backdrop + blocks occlude the editor while the canvas is active.
            if canvas_count > 0 {
                draw_text_region(
                    &mut pass,
                    self.canvas_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        render_pass.set_scissor_rect(0, 0, viewport_width, viewport_height);
                        self.region_pipeline
                            .draw_range(render_pass, canvas_start, canvas_count);
                    },
                );
                draw_text_region(
                    &mut pass,
                    self.canvas_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.canvas_icon_pipeline.draw(render_pass);
                    },
                );
                draw_text_region(
                    &mut pass,
                    self.canvas_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.canvas_text_pipeline.draw(render_pass);
                    },
                );
            }

            // 14. Pointer-hover handle highlight — always on top, full-screen
            // scissor so it can paint over dock edges and canvas cards alike.
            if hover_count > 0 {
                pass.set_scissor_rect(0, 0, viewport_width, viewport_height);
                self.region_pipeline
                    .draw_range(&mut pass, hover_start, hover_count);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        // Drop the bottom-dock tab icons that were merged into the sidebar icon
        // buffer for this frame's draw, so they don't accumulate across frames
        // when the sidebar itself isn't rebuilt.
        self.sidebar_icon_instances
            .truncate(sidebar_icon_count as usize);

        let frame_time = frame_started_at.elapsed();
        if frame_time > FRAME_TIME_WARN_THRESHOLD {
            eprintln!(
                "[Renderer] slow frame: {:.2}ms CPU encode (excl. vsync wait; target <= 8.00ms for 120FPS, regions={}, size={}x{})",
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
