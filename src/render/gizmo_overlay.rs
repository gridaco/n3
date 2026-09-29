//! Transform endpoints have their own depth pass over the scene. Reuse the
//! viewport attachments and retain the tiny vertex buffer between frames.
use super::{COLOR_FORMAT, DEPTH_FORMAT, SAMPLE_COUNT};
use crate::render::transform_gizmo::GizmoVertex;

pub(super) struct GizmoOverlay {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    vertices: Vec<GizmoVertex>,
}

impl GizmoOverlay {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("n3 transform endpoint shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("gizmo_overlay.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("n3 transform endpoint depth pipeline"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<GizmoVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4],
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: SAMPLE_COUNT,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        // Three cones plus shaft ribbons fit without per-frame allocation.
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("n3 transform endpoints"),
            size: (324 * size_of::<GizmoVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            buffer,
            vertices: Vec::new(),
        }
    }

    pub fn update(&mut self, queue: &wgpu::Queue, vertices: Vec<GizmoVertex>) {
        assert!(
            vertices.len() <= 324,
            "transform endpoint vertex budget exceeded"
        );
        if self.vertices != vertices {
            if !vertices.is_empty() {
                queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&vertices));
            }
            self.vertices = vertices;
        }
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
    }

    pub fn active(&self) -> bool {
        !self.vertices.is_empty()
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        resolve: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) {
        if !self.active() {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("n3 transform endpoint overlay"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: Some(resolve),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.draw(0..self.vertices.len() as u32, 0..1);
    }
}
