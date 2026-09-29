//! Derived edit-mode chrome. Geometry and selection changes rebuild the GPU
//! edge instances and face ranges; camera movement only updates uniforms.

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::{
    edit_feedback::{EditSelection, FACE_ALPHA, SELECTED, UNSELECTED},
    mesh::{EditObjectTopology, MeshData, Vertex},
    object_feedback::{HOVERED_COLOR, HOVERED_WIDTH, SELECTED_COLOR, SELECTED_WIDTH},
};

use super::{
    COLOR_FORMAT, DEPTH_FORMAT, ObjectHighlights, SAMPLE_COUNT, VERTEX_ATTRIBUTES,
    object_is_visible,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct EdgeInstance {
    start: [f32; 3],
    end: [f32; 3],
    selected: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct EditUniforms {
    selected: [f32; 4],
    unselected: [f32; 4],
    metrics: [f32; 4],
}

#[derive(Default)]
struct DrawData {
    edges: Vec<EdgeInstance>,
    faces: Vec<Range<u32>>,
    edited_edges: Option<Range<u32>>,
}

impl DrawData {
    fn for_selection(topology: &[EditObjectTopology], selection: &EditSelection) -> Self {
        let Some(object) = topology
            .iter()
            .find(|object| Some(object.object) == selection.object)
        else {
            return Self::default();
        };
        let points: BTreeMap<_, _> = object
            .vertices
            .iter()
            .map(|vertex| (vertex.id, vertex.position))
            .collect();
        let edges = object
            .edge_vertices
            .iter()
            .filter_map(|ids| {
                Some(EdgeInstance {
                    start: *points.get(&ids[0])?,
                    end: *points.get(&ids[1])?,
                    selected: ids.map(|id| {
                        if selection.vertex_selected(object.object, id) {
                            1.0
                        } else {
                            0.0
                        }
                    }),
                })
            })
            .collect();
        let mut faces: Vec<Range<u32>> = Vec::new();
        for face in &object.faces {
            if face.triangles.is_empty() || !selection.face_selected(object.object, &face.vertices)
            {
                continue;
            }
            if let Some(previous) = faces.last_mut()
                && previous.end == face.triangles.start
            {
                previous.end = face.triangles.end;
            } else {
                faces.push(face.triangles.clone());
            }
        }
        Self {
            edges,
            faces,
            edited_edges: Some(object.edges.clone()),
        }
    }
}

pub(super) struct EditOverlay {
    topology: Vec<EditObjectTopology>,
    selection: EditSelection,
    draw: DrawData,
    edge_buffer: Option<wgpu::Buffer>,
    // All authored edges are cached independently of edit selection. Unfilled
    // objects need edge feedback; X-ray also reveals occluded object selection
    // through these edges without another layered silhouette-mask system.
    object_edge_buffer: Option<wgpu::Buffer>,
    object_edge_ranges: Vec<(u64, Range<u32>)>,
    loose_edge_ranges: Vec<(u64, Range<u32>)>,
    uniform_buffer: wgpu::Buffer,
    group: wgpu::BindGroup,
    selected_object_uniform: wgpu::Buffer,
    selected_object_group: wgpu::BindGroup,
    hovered_object_uniform: wgpu::Buffer,
    hovered_object_group: wgpu::BindGroup,
    face_pipeline: wgpu::RenderPipeline,
    hidden_face_pipeline: wgpu::RenderPipeline,
    edge_pipeline: wgpu::RenderPipeline,
    hidden_edge_pipeline: wgpu::RenderPipeline,
    pixel_scale: f32,
    #[cfg(test)]
    pub(super) upload_revision: u64,
}

impl EditOverlay {
    pub(super) fn new(device: &wgpu::Device, scene_layout: &wgpu::BindGroupLayout) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("n3 edit overlay layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(size_of::<EditUniforms>() as u64),
                },
                count: None,
            }],
        });
        let style = |label| {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size_of::<EditUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });
            (buffer, group)
        };
        let (uniform_buffer, group) = style("n3 edit overlay style");
        let (selected_object_uniform, selected_object_group) = style("n3 selected object edges");
        let (hovered_object_uniform, hovered_object_group) = style("n3 hovered object edges");
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("n3 edit overlay pipelines"),
            bind_group_layouts: &[Some(scene_layout), Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("n3 depth-tested edit faces and edges"),
            source: wgpu::ShaderSource::Wgsl(include_str!("edit_overlay.wgsl").into()),
        });
        let edge_attributes =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];
        let face_layout = [Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERTEX_ATTRIBUTES,
        })];
        let edge_layout = [Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<EdgeInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &edge_attributes,
        })];
        let pipeline = |label,
                        vertex,
                        fragment,
                        buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
                        edge,
                        hidden| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vertex),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(if hidden {
                        wgpu::CompareFunction::Greater
                    } else {
                        wgpu::CompareFunction::LessEqual
                    }),
                    stencil: Default::default(),
                    // One representable depth step resolves equality on the
                    // polygon boundary. No broad NDC pull or slope bias exposes
                    // edges through surfaces elsewhere in the scene.
                    bias: wgpu::DepthBiasState {
                        constant: if edge { -1 } else { 0 },
                        slope_scale: 0.0,
                        clamp: 0.0,
                    },
                }),
                multisample: wgpu::MultisampleState {
                    count: SAMPLE_COUNT,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            topology: Vec::new(),
            selection: EditSelection::default(),
            draw: DrawData::default(),
            edge_buffer: None,
            object_edge_buffer: None,
            object_edge_ranges: Vec::new(),
            loose_edge_ranges: Vec::new(),
            uniform_buffer,
            group,
            selected_object_uniform,
            selected_object_group,
            hovered_object_uniform,
            hovered_object_group,
            face_pipeline: pipeline(
                "n3 selected polygon tint",
                "vs_face",
                "fs_face",
                &face_layout,
                false,
                false,
            ),
            hidden_face_pipeline: pipeline(
                "n3 x-ray occluded polygon tint",
                "vs_face",
                "fs_hidden_face",
                &face_layout,
                false,
                true,
            ),
            edge_pipeline: pipeline(
                "n3 edit endpoint edge gradients",
                "vs_edge",
                "fs_edge",
                &edge_layout,
                true,
                false,
            ),
            hidden_edge_pipeline: pipeline(
                "n3 x-ray occluded endpoint edge gradients",
                "vs_edge",
                "fs_hidden_edge",
                &edge_layout,
                true,
                true,
            ),
            pixel_scale: 1.0,
            #[cfg(test)]
            upload_revision: 0,
        }
    }

    pub(super) fn set_mesh(&mut self, device: &wgpu::Device, mesh: &MeshData) {
        self.topology = mesh.edit_topology.clone();
        let mut object_edges = Vec::new();
        self.object_edge_ranges.clear();
        self.loose_edge_ranges.clear();
        for object in &self.topology {
            let start = object_edges.len() as u32;
            for edge in mesh.edges[object.edges.start as usize..object.edges.end as usize]
                .as_chunks::<2>()
                .0
            {
                object_edges.push(EdgeInstance {
                    start: edge[0].position,
                    end: edge[1].position,
                    selected: [1.0; 2],
                });
            }
            let end = object_edges.len() as u32;
            if start < end {
                self.object_edge_ranges.push((object.object, start..end));
            }
            if !object.loose_edges.is_empty() {
                let loose_start = start + (object.loose_edges.start - object.edges.start) / 2;
                let loose_end = start + (object.loose_edges.end - object.edges.start) / 2;
                self.loose_edge_ranges
                    .push((object.object, loose_start..loose_end));
            }
        }
        self.object_edge_buffer = (!object_edges.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("n3 cached authored-edge object feedback"),
                contents: bytemuck::cast_slice(&object_edges),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        self.rebuild(device);
    }

    pub(super) fn clear_mesh(&mut self) {
        self.topology.clear();
        self.draw = DrawData::default();
        self.edge_buffer = None;
        self.object_edge_buffer = None;
        self.object_edge_ranges.clear();
        self.loose_edge_ranges.clear();
    }

    pub(super) fn set_selection(&mut self, device: &wgpu::Device, selection: EditSelection) {
        if self.selection != selection {
            self.selection = selection;
            self.rebuild(device);
        }
    }

    fn rebuild(&mut self, device: &wgpu::Device) {
        self.draw = DrawData::for_selection(&self.topology, &self.selection);
        self.edge_buffer = (!self.draw.edges.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("n3 cached edit edges"),
                contents: bytemuck::cast_slice(&self.draw.edges),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        #[cfg(test)]
        {
            self.upload_revision += 1;
        }
    }

    pub(super) fn set_pixel_scale(&mut self, pixels_per_point: f32) {
        self.pixel_scale = pixels_per_point;
    }

    pub(super) fn write_uniforms(
        &self,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        wire_color: Option<[f32; 4]>,
    ) {
        let mut selected = SELECTED.to_array().map(|byte| f32::from(byte) / 255.0);
        selected[3] = FACE_ALPHA;
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&EditUniforms {
                selected,
                unselected: wire_color
                    .unwrap_or_else(|| UNSELECTED.to_array().map(|byte| f32::from(byte) / 255.0)),
                metrics: [width as f32, height as f32, 1.25 * self.pixel_scale, 0.75],
            }),
        );
        if self.object_edge_buffer.is_some() {
            for (buffer, color, line_width) in [
                (
                    &self.selected_object_uniform,
                    SELECTED_COLOR,
                    SELECTED_WIDTH,
                ),
                (&self.hovered_object_uniform, HOVERED_COLOR, HOVERED_WIDTH),
            ] {
                let color = color.to_array().map(|byte| f32::from(byte) / 255.0);
                queue.write_buffer(
                    buffer,
                    0,
                    bytemuck::bytes_of(&EditUniforms {
                        selected: color,
                        unselected: color,
                        metrics: [
                            width as f32,
                            height as f32,
                            line_width * self.pixel_scale,
                            0.75,
                        ],
                    }),
                );
            }
        }
    }

    pub(super) fn draw_faces<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        triangles: &'a wgpu::Buffer,
        xray: bool,
    ) {
        if self.draw.faces.is_empty() {
            return;
        }
        pass.set_bind_group(1, &self.group, &[]);
        pass.set_vertex_buffer(0, triangles.slice(..));
        for pipeline in xray
            .then_some(&self.hidden_face_pipeline)
            .into_iter()
            .chain(std::iter::once(&self.face_pipeline))
        {
            pass.set_pipeline(pipeline);
            for range in &self.draw.faces {
                pass.draw(range.clone(), 0..1);
            }
        }
    }

    pub(super) fn draw_edges<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, xray: bool) {
        if let Some(buffer) = &self.edge_buffer {
            pass.set_bind_group(1, &self.group, &[]);
            pass.set_vertex_buffer(0, buffer.slice(..));
            for pipeline in xray
                .then_some(&self.hidden_edge_pipeline)
                .into_iter()
                .chain(std::iter::once(&self.edge_pipeline))
            {
                pass.set_pipeline(pipeline);
                pass.draw(0..6, 0..self.draw.edges.len() as u32);
            }
        }
    }

    pub(super) fn draw_object_highlights<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        highlights: &ObjectHighlights,
        visible: Option<&BTreeSet<u64>>,
        xray: bool,
    ) {
        let Some(buffer) = &self.object_edge_buffer else {
            return;
        };
        pass.set_vertex_buffer(0, buffer.slice(..));
        for (pipeline, ranges) in xray
            .then_some((&self.hidden_edge_pipeline, &self.object_edge_ranges))
            .into_iter()
            .chain(std::iter::once((
                &self.edge_pipeline,
                &self.loose_edge_ranges,
            )))
        {
            pass.set_pipeline(pipeline);
            for (object, range) in ranges {
                if self.selection.object == Some(*object) || !object_is_visible(visible, *object) {
                    continue;
                }
                let group = if highlights.selected.contains(object) {
                    &self.selected_object_group
                } else if highlights.hovered == Some(*object) {
                    &self.hovered_object_group
                } else {
                    continue;
                };
                pass.set_bind_group(1, group, &[]);
                pass.draw(0..6, range.clone());
            }
        }
    }

    pub(super) fn ordinary_edge_ranges(&self, count: u32) -> impl Iterator<Item = Range<u32>> {
        let ranges = match &self.draw.edited_edges {
            Some(edited) => [0..edited.start.min(count), edited.end.min(count)..count],
            None => [0..count, 0..0],
        };
        ranges.into_iter().filter(|range| !range.is_empty())
    }
}
