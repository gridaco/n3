//! Measurement-only presentation of the production offscreen scene. No egui
//! frame, texture registration, tessellation, buffers or compositor execute here.
use super::{FrameTarget, WorkspaceRenderer};
use crate::{
    document::AssetInstance,
    measurement::{FrameProbe, Stage},
    scene_view::SceneView,
    workspace_ui::WorkspaceUi,
};
use std::collections::BTreeMap;

impl WorkspaceRenderer {
    pub(crate) fn paint_without_ui(
        &mut self,
        target: FrameTarget<'_>,
        state: &mut WorkspaceUi,
        previous_assets: BTreeMap<AssetInstance, SceneView>,
        pixels_per_point: f32,
        editor_feedback: bool,
        probe: &mut FrameProbe,
    ) {
        probe.surface_size(target.size);
        self.sync_scene(
            target.device,
            target.queue,
            state,
            previous_assets,
            pixels_per_point,
            probe,
        );
        if !editor_feedback {
            self.prepare_feedback(
                target.device,
                target.queue,
                state,
                pixels_per_point,
                false,
                probe,
            );
        }
        probe.end(Stage::CacheSync);
        let mut encoder = target
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("workspace frame without UI"),
            });
        if editor_feedback {
            self.prepare_feedback(
                target.device,
                target.queue,
                state,
                pixels_per_point,
                true,
                probe,
            );
            probe.end(Stage::Feedback);
        }
        self.encode_scene(target.queue, &mut encoder, state, probe);
        probe.end(Stage::SceneEncode);
        self.presentation.render(
            &target,
            &mut encoder,
            &self.scene.view,
            state.viewport,
            pixels_per_point,
        );
        probe.scene_composite();
        probe.end(Stage::SceneComposite);
        target.queue.submit([encoder.finish()]);
        probe.end(Stage::Submit);
    }
}

pub(super) struct ScenePresentation {
    format: wgpu::TextureFormat,
    resources: Option<Resources>,
}
struct Resources {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    rect: wgpu::Buffer,
    scene: Option<wgpu::BindGroup>,
}
impl ScenePresentation {
    pub(super) fn new(format: wgpu::TextureFormat) -> Self {
        Self {
            format,
            resources: None,
        }
    }
    pub(super) fn invalidate(&mut self) {
        if let Some(resources) = &mut self.resources {
            resources.scene = None;
        }
    }
    fn render(
        &mut self,
        target: &FrameTarget<'_>,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        viewport: egui::Rect,
        pixels_per_point: f32,
    ) {
        let resources = self
            .resources
            .get_or_insert_with(|| Resources::new(target.device, self.format));
        let scene = resources.scene.get_or_insert_with(|| {
            target.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("measurement scene presentation"),
                layout: &resources.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(scene),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&resources.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: resources.rect.as_entire_binding(),
                    },
                ],
            })
        });
        // Preserve the editor's scene rectangle and pixel density. Expanding to
        // the surface would silently benchmark more pixels and a different view.
        let scale = [
            pixels_per_point / target.size[0] as f32,
            pixels_per_point / target.size[1] as f32,
        ];
        let rect = [
            viewport.min.x * scale[0] * 2.0 - 1.0,
            1.0 - viewport.min.y * scale[1] * 2.0,
            viewport.max.x * scale[0] * 2.0 - 1.0,
            1.0 - viewport.max.y * scale[1] * 2.0,
        ];
        target
            .queue
            .write_buffer(&resources.rect, 0, bytemuck::cast_slice(&rect));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene-only composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target.view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&resources.pipeline);
        pass.set_bind_group(0, &*scene, &[]);
        pass.draw(0..6, 0..1);
    }
}
impl Resources {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("measurement scene presentation layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("measurement scene presentation sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let rect = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("measurement scene presentation rectangle"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("measurement scene presentation shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene_composite.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("measurement scene presentation pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("measurement scene presentation pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(if format.is_srgb() {
                    "fs_srgb"
                } else {
                    "fs_gamma"
                }),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            layout,
            sampler,
            rect,
            scene: None,
        }
    }
}
