#![allow(unused_imports)]

use crate::render::region_pipeline::RegionDrawInstance;

use super::super::{RenderError, Renderer, helpers::draw_text_region};

impl Renderer {
    pub fn render(&mut self, region_instances: &[RegionDrawInstance]) -> Result<(), RenderError> {
        let frame = match self.surface_state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout => return Err(RenderError::Timeout),
            wgpu::CurrentSurfaceTexture::Occluded => return Err(RenderError::Occluded),
            wgpu::CurrentSurfaceTexture::Outdated => return Err(RenderError::Outdated),
            wgpu::CurrentSurfaceTexture::Lost => return Err(RenderError::Lost),
            wgpu::CurrentSurfaceTexture::Validation => return Err(RenderError::Validation),
        };

        self.region_pipeline
            .upload_instances(&self.device, &self.queue, region_instances);

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

            // 1. Panel backgrounds (no scissor).
            self.region_pipeline.draw(&mut pass);

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

            if !self.editor_overlay_chrome_instances.is_empty() {
                self.region_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &self.editor_overlay_chrome_instances,
                );
                draw_text_region(
                    &mut pass,
                    self.editor_overlay_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw(render_pass);
                    },
                );
            }

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
            if !self.welcome_logo_chrome_instances.is_empty() {
                self.region_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &self.welcome_logo_chrome_instances,
                );
                draw_text_region(
                    &mut pass,
                    self.welcome_logo_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw(render_pass);
                    },
                );
            }
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

            // 5. Leap label overlay: dim + per-char bg + label chars.
            if !self.leap_label_glyph_instances.is_empty() {
                if !self.leap_label_bg_instances.is_empty() {
                    self.region_pipeline.upload_instances(
                        &self.device,
                        &self.queue,
                        &self.leap_label_bg_instances,
                    );
                    draw_text_region(
                        &mut pass,
                        self.leap_label_scissor,
                        viewport_width,
                        viewport_height,
                        |render_pass| {
                            self.region_pipeline.draw(render_pass);
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
            if !self.terminal_cursor_instances.is_empty() {
                self.region_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &self.terminal_cursor_instances,
                );
                draw_text_region(
                    &mut pass,
                    self.terminal_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw(render_pass);
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
            if !self.buffer_terminal_cursor_instances.is_empty() {
                self.region_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &self.buffer_terminal_cursor_instances,
                );
                draw_text_region(
                    &mut pass,
                    self.buffer_terminal_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw(render_pass);
                    },
                );
            }

            // 7. Welcome empty-state chrome/text. This layer is populated when the
            // app has no open buffers; keep it below the top/status bars and
            // below palette overlays, but above the center background.
            if !self.welcome_logo_chrome_instances.is_empty() {
                self.region_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &self.welcome_logo_chrome_instances,
                );
                draw_text_region(
                    &mut pass,
                    self.welcome_logo_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw(render_pass);
                    },
                );
            }
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

            // 10. Command palette chrome (scrim + box) above editor text.
            if !self.palette_chrome_instances.is_empty() {
                self.region_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &self.palette_chrome_instances,
                );
                self.region_pipeline.draw(&mut pass);
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

            if !self.lsp_guide_chrome_instances.is_empty() {
                self.region_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &self.lsp_guide_chrome_instances,
                );
                draw_text_region(
                    &mut pass,
                    self.lsp_guide_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw(render_pass);
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

            if !self.toast_chrome_instances.is_empty() {
                self.region_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &self.toast_chrome_instances,
                );
                draw_text_region(
                    &mut pass,
                    self.toast_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw(render_pass);
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
            if !self.diagnostic_hover_chrome_instances.is_empty() {
                self.region_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &self.diagnostic_hover_chrome_instances,
                );
                draw_text_region(
                    &mut pass,
                    self.diagnostic_hover_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        render_pass.set_scissor_rect(0, 0, viewport_width, viewport_height);
                        self.region_pipeline.draw(render_pass);
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
        frame.present();
        Ok(())
    }
}
