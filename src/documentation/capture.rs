//! Render frames through the real egui compositor, optionally with a scene.

use std::{sync::mpsc, time::Duration};

use crate::{camera::Camera, renderer::SceneRenderer};

#[path = "presentation.rs"]
pub(crate) mod presentation;
use presentation::WindowFrameTemplate;

#[path = "capture_backend.rs"]
mod backend;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// A completed frame, independent of its encoder. Rows are tightly packed
/// RGBA8 in top-to-bottom order. Scenario time belongs to the caller.
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct Capture {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub scene: SceneRenderer,
    pub texture: egui::TextureId,
    pub adapter: String,
    pub renderer_profile: &'static str,
    compositor: Compositor,
}

/// UI-only capture uses the same compositor, readback, framing, and encoding
/// as the public guides, without constructing a scene or a scene texture.
pub struct UiCapture {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pub adapter: String,
    pub renderer_profile: &'static str,
    compositor: Compositor,
}

struct Compositor {
    ui_renderer: egui_wgpu::Renderer,
    target: wgpu::Texture,
    target_view: wgpu::TextureView,
    width: u32,
    height: u32,
    frame_ready: bool,
    presentation: WindowFrameTemplate,
}

impl UiCapture {
    pub async fn new(width: u32, height: u32) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("Capture dimensions must be positive.".into());
        }
        let presentation = WindowFrameTemplate::new(width, height)?;
        let profile = backend::Profile::current()?;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: profile.backends(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: profile.force_fallback_adapter(),
                apply_limit_buckets: false,
            })
            .await
            .map_err(|e| format!("Could not find a {} capture adapter: {e}", profile.name()))?;
        let info = adapter.get_info();
        profile.validate(&info)?;
        let adapter_name = profile.receipt(&info);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("n3 documentation capture device"),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("Could not create the capture device: {e}"))?;
        let limit = device.limits().max_texture_dimension_2d;
        if width > limit || height > limit {
            return Err(format!(
                "Capture dimensions exceed the GPU limit of {limit}."
            ));
        }
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("n3 documentation frame"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&Default::default());
        let ui_renderer =
            egui_wgpu::Renderer::new(&device, FORMAT, egui_wgpu::RendererOptions::default());
        Ok(Self {
            device,
            queue,
            adapter: adapter_name,
            renderer_profile: profile.name(),
            compositor: Compositor {
                ui_renderer,
                target,
                target_view,
                width,
                height,
                frame_ready: false,
                presentation,
            },
        })
    }

    pub fn render(&mut self, ctx: &egui::Context, output: egui::FullOutput) -> Result<(), String> {
        self.compositor
            .render(&self.device, &self.queue, ctx, output, |_| Ok(()))
    }

    pub fn read_frame(&self) -> Result<CapturedFrame, String> {
        self.compositor.read_frame(&self.device, &self.queue)
    }

    pub fn framed_frame(&self) -> Result<CapturedFrame, String> {
        self.compositor.presentation.compose(self.read_frame()?)
    }

    pub fn framed_webp(&self) -> Result<Vec<u8>, String> {
        let frame = self.framed_frame()?;
        encode_webp(&frame.rgba, frame.width, frame.height)
    }

    pub fn framed_dimensions(&self) -> (u32, u32) {
        self.compositor.presentation.dimensions()
    }

    pub fn frame_template_name(&self) -> &'static str {
        presentation::TEMPLATE_NAME
    }
}

impl Capture {
    pub async fn new(width: u32, height: u32) -> Result<Self, String> {
        let UiCapture {
            device,
            queue,
            adapter,
            renderer_profile,
            mut compositor,
        } = UiCapture::new(width, height).await?;
        let scene = SceneRenderer::new(&device, width, height);
        let texture = compositor.ui_renderer.register_native_texture(
            &device,
            &scene.view,
            wgpu::FilterMode::Linear,
        );
        Ok(Self {
            device,
            queue,
            scene,
            texture,
            adapter,
            renderer_profile,
            compositor,
        })
    }

    // Matches the native viewer's scene and compositor inputs.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        ctx: &egui::Context,
        output: egui::FullOutput,
        viewport: egui::Rect,
        camera: &Camera,
        shading: crate::render::shading::ShadingMode,
        xray: bool,
        show_edges: bool,
        show_grid: bool,
        z_up: bool,
        viewport_color: egui::Color32,
    ) -> Result<(), String> {
        self.render_with_assets(
            ctx,
            output,
            viewport,
            camera,
            shading,
            xray,
            show_edges,
            show_grid,
            z_up,
            viewport_color,
            &[],
            &crate::document::DisplayFrame::default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_with_assets(
        &mut self,
        ctx: &egui::Context,
        mut output: egui::FullOutput,
        viewport: egui::Rect,
        camera: &Camera,
        shading: crate::render::shading::ShadingMode,
        xray: bool,
        show_edges: bool,
        show_grid: bool,
        z_up: bool,
        viewport_color: egui::Color32,
        assets: &[crate::renderer::PlacedScene],
        display_frame: &crate::document::DisplayFrame,
    ) -> Result<(), String> {
        self.compositor.frame_ready = false;
        validate_pixel_scale(&mut output)?;
        let size = viewport.size();
        let limit = self.device.limits().max_texture_dimension_2d as f32;
        if !viewport.is_finite()
            || size.x <= 0.0
            || size.y <= 0.0
            || size.x > limit
            || size.y > limit
        {
            output.textures_delta.clear();
            return Err("Capture viewport must be finite, positive, and within GPU limits.".into());
        }
        let width = size.x.round().max(1.0) as u32;
        let height = size.y.round().max(1.0) as u32;
        let resized = self.scene.width != width || self.scene.height != height;
        if resized {
            self.scene.resize(&self.device, width, height);
        }
        self.scene
            .set_assets(&self.device, &self.queue, assets, display_frame)
            .inspect_err(|_| output.textures_delta.clear())?;
        if resized {
            self.compositor
                .ui_renderer
                .update_egui_texture_from_wgpu_texture(
                    &self.device,
                    &self.scene.view,
                    wgpu::FilterMode::Linear,
                    self.texture,
                );
        }
        self.compositor
            .render(&self.device, &self.queue, ctx, output, |encoder| {
                self.scene.render(
                    &self.queue,
                    encoder,
                    camera,
                    crate::renderer::ViewportRenderOptions {
                        shading,
                        xray,
                        show_edges,
                        show_grid,
                        z_up,
                        background: viewport_color,
                    },
                )
            })
    }

    #[cfg(test)]
    pub fn webp(&self) -> Result<Vec<u8>, String> {
        let frame = self.read_frame()?;
        encode_webp(&frame.rgba, frame.width, frame.height)
    }

    /// The canvas is copied without scaling, preserving pointer coordinates.
    pub fn framed_webp(&self) -> Result<Vec<u8>, String> {
        let frame = self.framed_frame()?;
        encode_webp(&frame.rgba, frame.width, frame.height)
    }

    pub fn framed_frame(&self) -> Result<CapturedFrame, String> {
        self.compositor.presentation.compose(self.read_frame()?)
    }

    pub fn framed_dimensions(&self) -> (u32, u32) {
        self.compositor.presentation.dimensions()
    }

    pub fn frame_template_name(&self) -> &'static str {
        presentation::TEMPLATE_NAME
    }

    /// Read the latest completed render without advancing time or input.
    pub fn read_frame(&self) -> Result<CapturedFrame, String> {
        self.compositor.read_frame(&self.device, &self.queue)
    }
}

fn validate_pixel_scale(output: &mut egui::FullOutput) -> Result<(), String> {
    if output.pixels_per_point != 1.0 {
        output.textures_delta.clear();
        return Err("Documentation capture requires exactly one pixel per point.".into());
    }
    Ok(())
}

fn validate_egui_diagnostics(output: &mut egui::FullOutput) -> Result<(), String> {
    // egui 0.36 emits this sustained-multipass diagnostic through its debug
    // painter, not PlatformOutput. Inspect the original shape before tessellation
    // rather than guessing from pass counts: a single legitimate layout retry
    // is allowed. The production-egui test below also guards this integration
    // against dependency upgrades changing the diagnostic's representation.
    fn performance_warning(shape: &egui::Shape) -> Option<&str> {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().find_map(performance_warning),
            egui::Shape::Text(text)
                if text.pos == egui::Pos2::ZERO
                    && text.fallback_color == egui::Color32::RED
                    && text
                        .galley
                        .text()
                        .starts_with("egui PERF WARNING: request_discard has been called ") =>
            {
                Some(text.galley.text())
            }
            _ => None,
        }
    }
    if let Some(warning) = output
        .shapes
        .iter()
        .find_map(|shape| performance_warning(&shape.shape))
    {
        let error = format!(
            "Capture rejected an egui layout performance diagnostic. Fix the repeated layout \
             discard before updating documentation or workbench evidence:\n{warning}"
        );
        output.textures_delta.clear();
        return Err(error);
    }
    Ok(())
}

impl Compositor {
    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        ctx: &egui::Context,
        mut output: egui::FullOutput,
        render_scene: impl FnOnce(&mut wgpu::CommandEncoder) -> Result<(), String>,
    ) -> Result<(), String> {
        // A failed render must never leave the previous frame available to a
        // caller as if the current requested frame had completed successfully.
        self.frame_ready = false;
        validate_pixel_scale(&mut output)?;
        validate_egui_diagnostics(&mut output)?;
        self.presentation
            .set_palette(crate::theme::Palette::from_context(
                ctx,
                crate::settings::AccentColor::DEFAULT,
            ))?;
        // Apply deltas on every simulated frame, including frames not saved.
        for (id, deltas) in output.textures_delta.set.drain() {
            for delta in deltas {
                self.ui_renderer.update_texture(device, queue, id, &delta);
            }
        }
        let jobs = ctx.tessellate(output.shapes, 1.0);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.width, self.height],
            pixels_per_point: 1.0,
        };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("n3 documentation frame encoder"),
        });
        render_scene(&mut encoder).inspect_err(|_| output.textures_delta.clear())?;
        let buffers = self
            .ui_renderer
            .update_buffers(device, queue, &mut encoder, &jobs, &screen);
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("n3 documentation UI compositor"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.target_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            self.ui_renderer
                .render(&mut pass.forget_lifetime(), &jobs, &screen);
        }
        queue.submit(buffers.into_iter().chain(std::iter::once(encoder.finish())));
        for id in output.textures_delta.free.drain() {
            self.ui_renderer.free_texture(&id);
        }
        self.frame_ready = true;
        Ok(())
    }

    fn read_frame(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<CapturedFrame, String> {
        if !self.frame_ready {
            return Err("Render a frame before requesting a capture.".into());
        }
        let row_bytes = self.width * 4;
        let padded_row_bytes = padded_row_bytes(self.width);
        let buffer_size = u64::from(padded_row_bytes) * u64::from(self.height);
        if buffer_size > device.limits().max_buffer_size {
            return Err("Capture readback exceeds the GPU buffer limit.".into());
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("n3 documentation frame readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("n3 documentation readback encoder"),
        });
        encoder.copy_texture_to_buffer(
            self.target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row_bytes),
                    rows_per_image: Some(self.height),
                },
            },
            self.target.size(),
        );
        let submission = queue.submit([encoder.finish()]);
        let (sender, receiver) = mpsc::sync_channel(1);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(30)),
            })
            .map_err(|e| format!("Capture GPU readback did not complete: {e}"))?;
        receiver
            .recv_timeout(Duration::from_secs(1))
            .map_err(|e| format!("Capture mapping callback did not complete: {e}"))?
            .map_err(|e| format!("Could not map the capture buffer: {e}"))?;
        let mapped = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|e| format!("Could not access the mapped capture buffer: {e}"))?;
        let rgba = pack_rows(&mapped, row_bytes as usize, padded_row_bytes as usize);
        drop(mapped);
        buffer.unmap();
        Ok(CapturedFrame {
            width: self.width,
            height: self.height,
            rgba,
        })
    }
}

fn padded_row_bytes(width: u32) -> u32 {
    let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    (width * 4).div_ceil(alignment) * alignment
}

fn pack_rows(mapped: &[u8], row_bytes: usize, stride: usize) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(mapped.len() / stride * row_bytes);
    for row in mapped.chunks_exact(stride) {
        rgba.extend_from_slice(&row[..row_bytes]);
    }
    rgba
}

fn encode_webp(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut bytes)
        .encode(rgba, width, height, image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("Could not encode the capture as lossless WebP: {e}"))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic_output(ctx: &egui::Context, discard: bool) -> egui::FullOutput {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(129.0, 97.0),
                )),
                ..Default::default()
            },
            |ui| {
                ui.label("Ordinary application content");
                if discard {
                    ui.ctx().request_discard("test sustained layout retry");
                }
            },
        )
    }

    #[test]
    fn capture_diagnostic_guard_rejects_the_actual_egui_performance_warning() {
        let ctx = egui::Context::default();
        // egui counts completed logical frames, then paints the diagnostic on
        // the next frame. This uses its production warning, not a fabricated
        // text shape that could drift away from the pinned dependency.
        for _ in 0..3 {
            let mut output = diagnostic_output(&ctx, true);
            assert_eq!(output.platform_output.num_completed_passes, 2);
            validate_egui_diagnostics(&mut output).unwrap();
            output.textures_delta.clear();
        }
        let mut output = diagnostic_output(&ctx, true);
        output
            .textures_delta
            .free
            .insert(egui::TextureId::Managed(999));
        let error = validate_egui_diagnostics(&mut output).unwrap_err();
        assert!(error.contains("egui PERF WARNING"));
        assert!(error.contains("test sustained layout retry"));
        assert!(output.textures_delta.is_empty());
    }

    #[test]
    fn capture_diagnostic_guard_accepts_ordinary_frames_and_isolated_layout_retry() {
        let ctx = egui::Context::default();
        let mut ordinary = diagnostic_output(&ctx, false);
        validate_egui_diagnostics(&mut ordinary).unwrap();
        ordinary.textures_delta.clear();
        let mut retried = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.label("A newly measured layout");
            if ui.ctx().current_pass_index() == 0 {
                ui.ctx().request_discard("initial layout measurement");
            }
        });
        assert_eq!(retried.platform_output.num_completed_passes, 2);
        validate_egui_diagnostics(&mut retried).unwrap();
        retried.textures_delta.clear();
        for _ in 0..5 {
            let mut ordinary = diagnostic_output(&ctx, false);
            validate_egui_diagnostics(&mut ordinary).unwrap();
            ordinary.textures_delta.clear();
        }
    }

    #[test]
    fn capture_diagnostic_guard_invalidates_the_previously_completed_frame() {
        let mut capture = pollster::block_on(UiCapture::new(129, 97)).unwrap();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            capture.render(&ctx, diagnostic_output(&ctx, true)).unwrap();
        }
        assert!(capture.read_frame().is_ok());
        let error = capture
            .render(&ctx, diagnostic_output(&ctx, true))
            .unwrap_err();
        assert!(error.contains("egui PERF WARNING"));
        assert!(capture.read_frame().is_err());
        assert!(capture.framed_webp().is_err());
    }

    /// Two separate surfaces, optionally hidden by a larger unselected surface.
    /// Their triangle diagonal must never become a selection outline.
    fn feedback_mesh(occluded: bool) -> crate::mesh::MeshData {
        use crate::mesh::{MeshData, ObjectRange, Vertex};
        let mut mesh = MeshData {
            vertices: Vec::new(),
            edges: Vec::new(),
            object_ranges: Vec::new(),
            edit_topology: Vec::new(),
            vertex_count: 0,
            face_count: 0,
            triangle_count: 0,
            object_count: 0,
            source_extent: [2.8, 1.8, 0.3],
            warnings: Vec::new(),
        };
        let mut quads = vec![(17, -1.2, -0.2, 0.7, 0.0), (42, 0.2, 1.2, 0.7, 0.0)];
        if occluded {
            quads.push((99, -1.4, 1.4, 0.9, 0.3));
        }
        for (object, left, right, half_height, z) in quads {
            let points = [
                [left, -half_height, z],
                [right, -half_height, z],
                [right, half_height, z],
                [left, half_height, z],
            ];
            let start = mesh.vertices.len() as u32;
            mesh.vertices.extend([0, 1, 2, 0, 2, 3].map(|index| Vertex {
                position: points[index],
                normal: [0.0, 0.0, 1.0],
            }));
            mesh.object_ranges.push(ObjectRange {
                object,
                triangles: start..mesh.vertices.len() as u32,
                edges: 0..0,
                loose_edges: 0..0,
            });
            mesh.vertex_count += 4;
            mesh.face_count += 1;
            mesh.triangle_count += 2;
            mesh.object_count += 1;
        }
        mesh
    }

    fn feedback_frame(
        capture: &mut Capture,
        ctx: &egui::Context,
        viewport: egui::Rect,
        camera: &Camera,
        highlights: crate::renderer::ObjectHighlights,
    ) -> Vec<u8> {
        capture.scene.set_highlights(highlights);
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(
                        capture.compositor.width as f32,
                        capture.compositor.height as f32,
                    ),
                )),
                ..Default::default()
            },
            |ui| {
                ui.ctx().layer_painter(egui::LayerId::background()).image(
                    capture.texture,
                    viewport,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            },
        );
        capture
            .render(
                ctx,
                output,
                viewport,
                camera,
                crate::render::shading::ShadingMode::Solid,
                false,
                false,
                false,
                false,
                crate::theme::Palette::new(
                    crate::settings::ResolvedTheme::Dark,
                    crate::settings::AccentColor::DEFAULT,
                )
                .workbench_viewport,
            )
            .unwrap();
        capture.read_frame().unwrap().rgba
    }

    #[test]
    fn transform_overlay_depth_is_order_independent_and_clears_without_scene_changes() {
        use crate::{render::transform_gizmo::GizmoVertex, renderer::ObjectHighlights};
        let mut capture = pollster::block_on(Capture::new(256, 192)).unwrap();
        let ctx = egui::Context::default();
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(256.0, 192.0));
        let mut camera = Camera::default();
        camera.set_view(crate::camera::View::Front);
        capture
            .scene
            .set_mesh(&capture.device, &feedback_mesh(true))
            .unwrap();
        let base = feedback_frame(
            &mut capture,
            &ctx,
            viewport,
            &camera,
            ObjectHighlights::default(),
        );
        let triangle = |z, color| {
            [[-0.7, -0.7], [0.7, -0.7], [0.0, 0.7]].map(|[x, y]| GizmoVertex {
                position: [x, y, z, 1.0],
                color,
            })
        };
        // Both lie behind the scene in depth. The overlay must still be visible,
        // but the nearer red face must occlude the green face in either draw order.
        let near = triangle(0.995, [1.0, 0.0, 0.0, 1.0]);
        let far = triangle(0.999, [0.0, 1.0, 0.0, 1.0]);
        capture
            .scene
            .set_transform_gizmo(&capture.queue, [near, far].concat());
        let first = feedback_frame(
            &mut capture,
            &ctx,
            viewport,
            &camera,
            ObjectHighlights::default(),
        );
        let center = (96 * 256 + 128) * 4;
        assert_eq!(&first[center..center + 4], &[255, 0, 0, 255]);
        capture
            .scene
            .set_transform_gizmo(&capture.queue, [far, near].concat());
        assert_eq!(
            first,
            feedback_frame(
                &mut capture,
                &ctx,
                viewport,
                &camera,
                ObjectHighlights::default()
            )
        );
        capture
            .scene
            .set_transform_gizmo(&capture.queue, Vec::new());
        assert_eq!(
            base,
            feedback_frame(
                &mut capture,
                &ctx,
                viewport,
                &camera,
                ObjectHighlights::default()
            )
        );
    }

    #[test]
    fn object_outlines_are_distinct_visible_silhouettes_without_geometry_reupload() {
        use crate::renderer::ObjectHighlights;
        let mut capture = pollster::block_on(Capture::new(256, 192)).unwrap();
        let ctx = egui::Context::default();
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(256.0, 192.0));
        let mut camera = Camera::default();
        camera.set_view(crate::camera::View::Front);
        capture
            .scene
            .set_mesh(&capture.device, &feedback_mesh(false))
            .unwrap();
        let selected = ObjectHighlights {
            selected: [17].into(),
            hovered: None,
        };
        let hovered = ObjectHighlights {
            selected: Default::default(),
            hovered: Some(17),
        };
        let both = ObjectHighlights {
            selected: [17].into(),
            hovered: Some(42),
        };
        let base = feedback_frame(
            &mut capture,
            &ctx,
            viewport,
            &camera,
            ObjectHighlights::default(),
        );
        let selection = feedback_frame(&mut capture, &ctx, viewport, &camera, selected);
        let hover = feedback_frame(&mut capture, &ctx, viewport, &camera, hovered);
        let simultaneous = feedback_frame(&mut capture, &ctx, viewport, &camera, both.clone());
        let group = ObjectHighlights {
            selected: [17, 42].into(),
            hovered: None,
        };
        let group_frame = feedback_frame(&mut capture, &ctx, viewport, &camera, group.clone());
        assert_ne!(
            group_frame, selection,
            "Every object in a box selection receives a selected outline"
        );
        assert_ne!(
            group_frame, simultaneous,
            "The second selected object uses selected rather than hover styling"
        );
        assert_eq!(
            group_frame,
            feedback_frame(
                &mut capture,
                &ctx,
                viewport,
                &camera,
                ObjectHighlights {
                    hovered: Some(42),
                    ..group
                }
            ),
            "Hover cannot weaken any member of a multiple-object selection"
        );
        let same_object = feedback_frame(
            &mut capture,
            &ctx,
            viewport,
            &camera,
            ObjectHighlights {
                selected: [17].into(),
                hovered: Some(17),
            },
        );
        assert_ne!(
            base, selection,
            "Selection must render with polygon edges disabled"
        );
        assert_ne!(base, hover, "Hover must render with polygon edges disabled");
        assert_ne!(
            selection, hover,
            "Hover and selection need distinguishable feedback"
        );
        assert_eq!(
            selection, same_object,
            "Selection wins over hover on the same object"
        );
        for (index, (normal, highlighted)) in base
            .as_chunks::<4>()
            .0
            .iter()
            .zip(selection.as_chunks::<4>().0.iter())
            .enumerate()
        {
            if index % 256 >= 128 {
                assert_eq!(
                    normal, highlighted,
                    "Selection must not leak to another object"
                );
            }
        }
        assert!(
            simultaneous
                .as_chunks::<4>()
                .0
                .iter()
                .zip(selection.as_chunks::<4>().0.iter())
                .enumerate()
                .any(|(index, (a, b))| index % 256 >= 128 && a != b),
            "A different hovered object remains visible beside selection"
        );
        let center = camera
            .view_projection(256.0 / 192.0)
            .project_point3(glam::vec3(-0.7, 0.0, 0.0));
        let x = ((center.x + 1.0) * 128.0) as usize;
        let y = ((1.0 - center.y) * 96.0) as usize;
        for py in y - 4..=y + 4 {
            for px in x - 4..=x + 4 {
                let i = (py * 256 + px) * 4;
                assert_eq!(
                    &base[i..i + 4],
                    &selection[i..i + 4],
                    "The face interior and its triangulation diagonal stay unoutlined"
                );
            }
        }
        // Clearing feedback must remove every outline without replacing the mesh.
        assert_eq!(
            base,
            feedback_frame(
                &mut capture,
                &ctx,
                viewport,
                &camera,
                ObjectHighlights::default()
            )
        );
        capture
            .scene
            .set_mesh(&capture.device, &feedback_mesh(true))
            .unwrap();
        let occluded_base = feedback_frame(
            &mut capture,
            &ctx,
            viewport,
            &camera,
            ObjectHighlights::default(),
        );
        assert_eq!(
            occluded_base,
            feedback_frame(&mut capture, &ctx, viewport, &camera, both.clone()),
            "An unselected foreground surface fully occludes the highlighted objects"
        );
        // Resize the attachments while feedback is active, then clear the scene.
        capture
            .scene
            .set_mesh(&capture.device, &feedback_mesh(false))
            .unwrap();
        let smaller = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(211.0, 157.0));
        capture.scene.set_pixel_scale(2.0);
        let resized_base = feedback_frame(
            &mut capture,
            &ctx,
            smaller,
            &camera,
            ObjectHighlights::default(),
        );
        assert_ne!(
            resized_base,
            feedback_frame(&mut capture, &ctx, smaller, &camera, both.clone())
        );
        capture.scene.clear_mesh();
        let empty = feedback_frame(&mut capture, &ctx, smaller, &camera, both.clone());
        assert_eq!(
            empty,
            feedback_frame(
                &mut capture,
                &ctx,
                smaller,
                &camera,
                ObjectHighlights::default()
            )
        );
    }

    #[test]
    fn readback_removes_row_padding_without_reordering_pixels() {
        let width = 65;
        let stride = padded_row_bytes(width) as usize;
        assert_eq!(stride, 512);
        assert_eq!(padded_row_bytes(64), 256);
        let mut padded = vec![0xee; stride * 2];
        padded[..260].fill(0x12);
        padded[stride..stride + 260].fill(0x34);
        let rgba = pack_rows(&padded, 260, stride);
        assert_eq!(rgba.len(), 520);
        assert_eq!(&rgba[..260], &[0x12; 260]);
        assert_eq!(&rgba[260..], &[0x34; 260]);
    }

    #[test]
    fn webp_roundtrip_preserves_rgba_exactly() {
        let rgba = [
            255, 0, 0, 255, 0, 0, 255, 255, 14, 61, 123, 127, 9, 8, 7, 255,
        ];
        let encoded = encode_webp(&rgba, 2, 2).unwrap();
        let decoded = image::load_from_memory_with_format(&encoded, image::ImageFormat::WebP)
            .unwrap()
            .to_rgba8();
        assert_eq!(decoded.dimensions(), (2, 2));
        assert_eq!(decoded.as_raw(), &rgba);
    }

    #[test]
    fn real_gpu_capture_composites_egui_and_scene() {
        let mut capture = pollster::block_on(Capture::new(129, 97)).unwrap();
        assert!(capture.read_frame().is_err());
        assert!(capture.webp().is_err());
        let ctx = egui::Context::default();
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(129.0, 97.0));
        let mut frames = Vec::new();
        for show_grid in [false, true] {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(viewport),
                    ..Default::default()
                },
                |ui| {
                    let painter = ui.ctx().layer_painter(egui::LayerId::background());
                    painter.image(
                        capture.texture,
                        viewport,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                    painter.rect_filled(
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(12.0, 12.0)),
                        crate::theme::radius::NONE,
                        egui::Color32::RED,
                    );
                    painter.rect_filled(
                        egui::Rect::from_min_max(egui::pos2(117.0, 85.0), egui::pos2(129.0, 97.0)),
                        crate::theme::radius::NONE,
                        egui::Color32::BLUE,
                    );
                    painter.text(
                        egui::pos2(15.0, 2.0),
                        egui::Align2::LEFT_TOP,
                        "Capture",
                        egui::FontId::proportional(crate::theme::text::XS),
                        egui::Color32::WHITE,
                    );
                },
            );
            capture
                .render(
                    &ctx,
                    output,
                    viewport,
                    &Camera::default(),
                    crate::render::shading::ShadingMode::Solid,
                    false,
                    false,
                    show_grid,
                    false,
                    crate::theme::Palette::new(
                        crate::settings::ResolvedTheme::Dark,
                        crate::settings::AccentColor::DEFAULT,
                    )
                    .workbench_viewport,
                )
                .unwrap();
            let encoded = capture.webp().unwrap();
            let frame = capture.read_frame().unwrap();
            let decoded = image::load_from_memory_with_format(&encoded, image::ImageFormat::WebP)
                .unwrap()
                .to_rgba8();
            assert_eq!(decoded.dimensions(), (129, 97));
            assert_eq!((frame.width, frame.height), decoded.dimensions());
            assert_eq!(
                frame.rgba,
                *decoded.as_raw(),
                "Still and future animated exports share the exact rendered pixels"
            );
            assert_eq!(decoded.get_pixel(5, 5).0, [255, 0, 0, 255]);
            assert_eq!(decoded.get_pixel(123, 91).0, [0, 0, 255, 255]);
            let presented = image::load_from_memory_with_format(
                &capture.framed_webp().unwrap(),
                image::ImageFormat::WebP,
            )
            .unwrap()
            .to_rgba8();
            assert_eq!(presented.dimensions(), capture.framed_dimensions());
            let corner = presentation::CORNER_RADIUS.ceil() as u32;
            for y in 0..frame.height - corner {
                for x in 1..frame.width - 1 {
                    assert_eq!(
                        presented.get_pixel(
                            x + presentation::SIDE,
                            y + presentation::TOP + presentation::TITLE
                        ),
                        decoded.get_pixel(x, y),
                        "The documentation frame must not resize or recolor interior app pixels"
                    );
                }
            }
            assert!(
                presented
                    .get_pixel(
                        presentation::SIDE,
                        presentation::TOP + presentation::TITLE + frame.height - 1
                    )
                    .0[3]
                    < 255,
                "The lower content corner is clipped to the rounded window silhouette"
            );
            frames.push(decoded);
        }
        assert_ne!(
            frames[0], frames[1],
            "the shared scene grid must appear in the capture"
        );
    }

    #[test]
    fn ui_only_capture_shares_lossless_readback_and_unscaled_window_framing() {
        let mut capture = pollster::block_on(UiCapture::new(129, 97)).unwrap();
        assert!(capture.read_frame().is_err());
        assert_eq!(capture.frame_template_name(), presentation::TEMPLATE_NAME);
        let ctx = egui::Context::default();
        crate::ui::typography::install(&ctx);
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(129.0, 97.0));
        let mut previous = None;
        let mut presented_frames = Vec::new();
        for (sample, theme) in [
            egui::Theme::Light,
            egui::Theme::Light,
            egui::Theme::Dark,
            egui::Theme::Light,
            egui::Theme::Light,
        ]
        .into_iter()
        .enumerate()
        {
            ctx.set_theme(theme);
            let time = sample as f64;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(bounds),
                    time: Some(time),
                    ..Default::default()
                },
                |ui| {
                    let painter = ui.painter();
                    painter.rect_filled(bounds, 0, egui::Color32::WHITE);
                    let clipped = painter.with_clip_rect(egui::Rect::from_min_size(
                        egui::pos2(20.0, 20.0),
                        egui::vec2(20.0, 20.0),
                    ));
                    clipped.rect_filled(bounds, 0, egui::Color32::BLUE);
                    painter.text(
                        egui::pos2(64.0, 60.0),
                        egui::Align2::CENTER_CENTER,
                        "⌘S",
                        egui::FontId::proportional(crate::theme::text::SM),
                        egui::Color32::BLACK,
                    );
                },
            );
            capture.render(&ctx, output).unwrap();
            let frame = capture.read_frame().unwrap();
            let pixel = |x, y| {
                let index = ((y * frame.width + x) * 4) as usize;
                &frame.rgba[index..index + 4]
            };
            assert_eq!(pixel(5, 5), [255, 255, 255, 255]);
            assert_eq!(pixel(25, 25), [0, 0, 255, 255]);
            assert_eq!(pixel(45, 25), [255, 255, 255, 255]);
            if let Some((previous_theme, ref previous_pixels)) = previous
                && previous_theme == theme
            {
                assert!(
                    frame.rgba == *previous_pixels,
                    "UI-only frames remain deterministic within the same appearance"
                );
            }
            let framed = capture.framed_frame().unwrap();
            let decoded = image::load_from_memory_with_format(
                &capture.framed_webp().unwrap(),
                image::ImageFormat::WebP,
            )
            .unwrap()
            .to_rgba8();
            assert_eq!(decoded.dimensions(), capture.framed_dimensions());
            assert_eq!(decoded.as_raw(), &framed.rgba);
            for y in 10..frame.height - 20 {
                for x in 10..frame.width - 20 {
                    assert_eq!(
                        decoded.get_pixel(x, y + presentation::TITLE).0,
                        pixel(x, y),
                        "The UI-only canvas uses the same unscaled frame presentation"
                    );
                }
            }
            let palette =
                crate::theme::Palette::from_context(&ctx, crate::settings::AccentColor::DEFAULT);
            let border = palette.border.to_srgba_unmultiplied();
            assert_eq!(
                decoded.get_pixel(90, presentation::TITLE / 2).0,
                palette.titlebar.to_srgba_unmultiplied()
            );
            assert_eq!(decoded.get_pixel(0, presentation::TITLE + 40).0, border);
            assert_eq!(decoded.get_pixel(64, framed.height - 1).0, border);
            presented_frames.push(framed.rgba);
            previous = Some((theme, frame.rgba));
        }
        assert!(presented_frames[0] != presented_frames[2]);
        assert!(
            presented_frames[0] == presented_frames[3],
            "Reusing a capture across appearances must restore the exact Light frame"
        );
        let scaled = egui::FullOutput {
            pixels_per_point: 2.0,
            ..Default::default()
        };
        assert!(capture.render(&ctx, scaled).is_err());
        assert!(capture.read_frame().is_err());
    }
}
