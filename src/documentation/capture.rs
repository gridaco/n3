//! Render documentation frames through the viewer's real scene and egui pipelines.

use std::{sync::mpsc, time::Duration};

use crate::{camera::Camera, renderer::SceneRenderer};

#[path = "presentation.rs"]
pub(crate) mod presentation;
use presentation::WindowFrameTemplate;

#[path = "capture_backend.rs"]
mod backend;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// A completed scene + UI + tutorial overlay frame, independent of its encoder.
/// Rows are tightly packed RGBA8, in top-to-bottom order. Scenario time belongs
/// to the caller, so a future clip recorder can attach a duration to each frame.
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
    ui_renderer: egui_wgpu::Renderer,
    target: wgpu::Texture,
    target_view: wgpu::TextureView,
    width: u32,
    height: u32,
    frame_ready: bool,
    presentation: WindowFrameTemplate,
}

impl Capture {
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
        let scene = SceneRenderer::new(&device, width, height);
        let mut ui_renderer =
            egui_wgpu::Renderer::new(&device, FORMAT, egui_wgpu::RendererOptions::default());
        let texture =
            ui_renderer.register_native_texture(&device, &scene.view, wgpu::FilterMode::Linear);
        Ok(Self {
            device,
            queue,
            scene,
            texture,
            adapter: adapter_name,
            renderer_profile: profile.name(),
            ui_renderer,
            target,
            target_view,
            width,
            height,
            frame_ready: false,
            presentation,
        })
    }

    // Matches the native viewer's scene and compositor inputs.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
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
    ) -> Result<(), String> {
        if output.pixels_per_point != 1.0 {
            output.textures_delta.clear();
            return Err("Documentation capture requires exactly one pixel per point.".into());
        }
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
        if self.scene.width != width || self.scene.height != height {
            self.scene.resize(&self.device, width, height);
            self.ui_renderer.update_egui_texture_from_wgpu_texture(
                &self.device,
                &self.scene.view,
                wgpu::FilterMode::Linear,
                self.texture,
            );
        }
        // Apply deltas on every simulated frame, including frames that are not saved.
        for (id, deltas) in output.textures_delta.set.drain() {
            for delta in deltas {
                self.ui_renderer
                    .update_texture(&self.device, &self.queue, id, &delta);
            }
        }
        let jobs = ctx.tessellate(output.shapes, 1.0);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.width, self.height],
            pixels_per_point: 1.0,
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("n3 documentation frame encoder"),
            });
        self.scene.render(
            &self.queue,
            &mut encoder,
            camera,
            crate::renderer::ViewportRenderOptions {
                shading,
                xray,
                show_edges,
                show_grid,
                z_up,
                background: viewport_color,
            },
        );
        let buffers = self.ui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &jobs,
            &screen,
        );
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
        self.queue
            .submit(buffers.into_iter().chain(std::iter::once(encoder.finish())));
        for id in output.textures_delta.free.drain() {
            self.ui_renderer.free_texture(&id);
        }
        self.frame_ready = true;
        Ok(())
    }

    #[cfg(test)]
    pub fn webp(&self) -> Result<Vec<u8>, String> {
        let frame = self.read_frame()?;
        encode_webp(&frame.rgba, frame.width, frame.height)
    }

    /// Documentation media shares one static title-bar presentation. The app
    /// canvas is copied without scaling, so scenario pointer coordinates stay
    /// in the original capture space.
    pub fn framed_webp(&self) -> Result<Vec<u8>, String> {
        let frame = self.framed_frame()?;
        encode_webp(&frame.rgba, frame.width, frame.height)
    }

    pub fn framed_frame(&self) -> Result<CapturedFrame, String> {
        self.presentation.compose(self.read_frame()?)
    }

    pub fn framed_dimensions(&self) -> (u32, u32) {
        self.presentation.dimensions()
    }

    pub fn frame_template_name(&self) -> &'static str {
        presentation::TEMPLATE_NAME
    }

    /// Read the latest completed render without advancing time, changing input,
    /// or requiring the camera to be settled. Suitable for in-flight clip frames.
    pub fn read_frame(&self) -> Result<CapturedFrame, String> {
        if !self.frame_ready {
            return Err("Render a frame before requesting a capture.".into());
        }
        let row_bytes = self.width * 4;
        let padded_row_bytes = padded_row_bytes(self.width);
        let buffer_size = u64::from(padded_row_bytes) * u64::from(self.height);
        if buffer_size > self.device.limits().max_buffer_size {
            return Err("Capture readback exceeds the GPU buffer limit.".into());
        }
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("n3 documentation frame readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
        let submission = self.queue.submit([encoder.finish()]);
        let (sender, receiver) = mpsc::sync_channel(1);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        self.device
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
                    egui::vec2(capture.width as f32, capture.height as f32),
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
            .set_mesh(&capture.device, &feedback_mesh(true));
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
            .set_mesh(&capture.device, &feedback_mesh(false));
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
            .set_mesh(&capture.device, &feedback_mesh(true));
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
            .set_mesh(&capture.device, &feedback_mesh(false));
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
                for x in 0..frame.width {
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
}
