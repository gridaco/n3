//! Shared frame mechanics; hosts retain device, surface, clock and scheduling policy.
//! This concrete owner reuses revision caches without dynamic dispatch or a
//! browser-sized capability floor. Each caller supplies its actual GPU limits.
use super::renderer::{SceneRenderer, ViewportRenderOptions};
use crate::{
    asset_io::LoadedDocument, document::AssetInstance, scene_view::SceneView,
    workspace_ui::WorkspaceUi,
};
use std::{collections::BTreeMap, path::PathBuf};

pub(crate) struct FrameTarget<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub view: &'a wgpu::TextureView,
    pub size: [u32; 2],
}

#[cfg(test)]
mod tests;

pub(crate) struct WorkspaceRenderer {
    ui: egui_wgpu::Renderer,
    scene: SceneRenderer,
    scene_size: [u32; 2],
    uploaded_revision: u64,
}

impl WorkspaceRenderer {
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        size: [u32; 2],
    ) -> (Self, egui::TextureId) {
        let mut ui =
            egui_wgpu::Renderer::new(device, format, egui_wgpu::RendererOptions::default());
        let scene = SceneRenderer::new(device, size[0], size[1]);
        let texture = ui.register_native_texture(device, &scene.view, wgpu::FilterMode::Linear);
        (
            Self {
                ui,
                scene,
                scene_size: size,
                uploaded_revision: u64::MAX,
            },
            texture,
        )
    }

    pub(crate) fn install(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        state: &mut WorkspaceUi,
        path: PathBuf,
        loaded: LoadedDocument,
        append: bool,
    ) -> Result<(), String> {
        // Prepare against an isolated candidate before publishing either CPU or
        // GPU state. Opening/importing a file is infrequent; placement afterwards
        // reuses the normal renderer resource cache.
        let mut candidate = WorkspaceUi::new(state.scene_texture);
        let copy = LoadedDocument {
            document: loaded.document.clone(),
            assets: loaded.assets.clone(),
            diagnostics: loaded.diagnostics.clone(),
            saved_bytes: loaded.saved_bytes.clone(),
        };
        if append {
            candidate.editor = crate::editor::Editor::new(state.editor.document.clone())?;
            candidate.editor.frame = state.editor.frame;
            candidate.asset_views = state.asset_views.clone();
            candidate
                .editor
                .set_asset_frames(state.editor.asset_frames().clone())?;
            candidate.import_loaded_document(copy)?;
        } else {
            candidate.install_loaded_document(path.clone(), copy)?;
        }
        if let Some(mesh) = &candidate.mesh {
            SceneRenderer::validate_mesh(device, mesh)?;
        }
        let mut gpu =
            SceneRenderer::new(device, self.scene_size[0].max(1), self.scene_size[1].max(1));
        gpu.set_assets(
            device,
            queue,
            &candidate.placed_scenes(),
            &candidate.editor.frame,
        )?;
        if let Some(mesh) = &candidate.mesh {
            gpu.set_mesh(device, mesh)?;
        }
        if append {
            state.import_loaded_document(loaded)?;
        } else {
            state.install_loaded_document(path, loaded)?;
        }
        self.ui.update_egui_texture_from_wgpu_texture(
            device,
            &gpu.view,
            wgpu::FilterMode::Linear,
            state.scene_texture,
        );
        self.scene = gpu;
        self.uploaded_revision = state.mesh_revision;
        Ok(())
    }
    /// Synchronize shared caches, encode and submit this frame. Surface acquire,
    /// presentation notification, and presentation remain host operations.
    pub(crate) fn paint(
        &mut self,
        target: FrameTarget<'_>,
        ctx: &egui::Context,
        state: &mut WorkspaceUi,
        output: &mut egui::FullOutput,
        previous_assets: BTreeMap<AssetInstance, SceneView>,
    ) {
        let device = target.device;
        let queue = target.queue;
        let pixels_per_point = output.pixels_per_point;
        let limit = device.limits().max_texture_dimension_2d;
        let width = (state.viewport.width() * pixels_per_point)
            .round()
            .clamp(1.0, limit as f32) as u32;
        let height = (state.viewport.height() * pixels_per_point)
            .round()
            .clamp(1.0, limit as f32) as u32;
        let resized = self.scene_size != [width, height];
        if resized {
            self.scene.resize(device, width, height);
        }
        // Preflight expanded proxy buffers before updating imported geometry:
        // either cache must retain the previous frame if this GPU cannot upload.
        let upload_allowed = match state
            .mesh
            .as_ref()
            .map(|mesh| SceneRenderer::validate_mesh(device, mesh))
            .transpose()
        {
            Ok(_) => true,
            Err(error) => {
                state.error = Some(error);
                false
            }
        };
        if upload_allowed
            && let Err(error) =
                self.scene
                    .set_assets(device, queue, &state.placed_scenes(), &state.editor.frame)
        {
            state.asset_views = previous_assets;
            for view in state.asset_views.values_mut() {
                view.playback.playing = false;
            }
            let frames = state
                .asset_views
                .iter()
                .map(|(key, view)| (key.clone(), view.frame.clone()))
                .collect();
            let _ = state.editor.set_asset_frames(frames);
            let _ = state.refresh_mesh();
            state.error = Some(error);
        }
        if resized {
            self.ui.update_egui_texture_from_wgpu_texture(
                device,
                &self.scene.view,
                wgpu::FilterMode::Linear,
                state.scene_texture,
            );
            self.scene_size = [width, height];
        }
        if upload_allowed && self.uploaded_revision != state.mesh_revision {
            let result = if let Some(mesh) = &state.mesh {
                self.scene.set_mesh(device, mesh)
            } else {
                self.scene.clear_mesh();
                Ok(())
            };
            match result {
                Ok(()) => self.uploaded_revision = state.mesh_revision,
                Err(error) => state.error = Some(error),
            }
        }
        let jobs = ctx.tessellate(std::mem::take(&mut output.shapes), pixels_per_point);
        for (id, deltas) in std::mem::take(&mut output.textures_delta.set) {
            for delta in deltas {
                self.ui.update_texture(device, queue, id, &delta);
            }
        }
        let view = target.view;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("workspace frame"),
        });
        self.scene.set_visible_objects(state.visible_objects());
        self.scene.set_highlights(state.object_highlights());
        self.scene
            .set_edit_selection(device, state.edit_selection());
        self.scene.set_pixel_scale(pixels_per_point);
        let gizmo = state
            .editor
            .transform_gizmo_vertices(state.viewport, &state.camera, state.z_up)
            .unwrap_or_else(|error| {
                state.error = Some(error);
                Vec::new()
            });
        self.scene.set_transform_gizmo(queue, gizmo);
        if let Err(error) = self.scene.render(
            queue,
            &mut encoder,
            &state.camera,
            ViewportRenderOptions {
                shading: state.shading,
                xray: state.editor.xray_enabled(),
                show_edges: state.show_edges,
                show_grid: state.show_grid,
                z_up: state.z_up,
                background: crate::theme::Palette::new(state.resolved_theme(), state.accent_color)
                    .workbench_viewport,
            },
        ) {
            state.error = Some(error);
        }
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: target.size,
            pixels_per_point,
        };
        let buffers = self
            .ui
            .update_buffers(device, queue, &mut encoder, &jobs, &screen);
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            self.ui.render(&mut pass.forget_lifetime(), &jobs, &screen);
        }
        queue.submit(buffers.into_iter().chain(std::iter::once(encoder.finish())));
    }

    /// Free UI textures only after the host has presented the submitted frame.
    pub(crate) fn finish_frame(&mut self, textures: impl IntoIterator<Item = egui::TextureId>) {
        for id in textures {
            self.ui.free_texture(&id);
        }
    }
}
