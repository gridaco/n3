use bytemuck::{Pod, Zeroable};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};
use wgpu::util::DeviceExt;

use crate::camera::Camera;
use crate::edit_feedback::EditSelection;
use crate::mesh::{MeshData, ObjectRange, Vertex};
use crate::object_feedback::{HOVERED_COLOR, HOVERED_WIDTH, SELECTED_COLOR, SELECTED_WIDTH};

#[path = "edit_overlay.rs"]
mod edit_overlay;
use edit_overlay::EditOverlay;
#[path = "gizmo_overlay.rs"]
mod gizmo_overlay;
pub(crate) use super::placed_scenes::PlacedScene;
use super::placed_scenes::PlacedScenes;
use super::shading::ShadingMode;
use super::transform_gizmo::GizmoVertex;
use gizmo_overlay::GizmoOverlay;

// egui samples native textures as gamma-encoded colors. The fragment shaders
// encode their linear lighting before writing to this non-sRGB attachment.
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SAMPLE_COUNT: u32 = 4;
const MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;
const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_projection: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    eye: [f32; 4],
    wire_color: [f32; 4],
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ObjectHighlights {
    pub selected: BTreeSet<u64>,
    pub hovered: Option<u64>,
}

#[derive(Clone, Copy)]
pub struct ViewportRenderOptions {
    pub shading: ShadingMode,
    pub xray: bool,
    pub show_edges: bool,
    pub show_grid: bool,
    pub z_up: bool,
    pub background: egui::Color32,
}

pub struct SceneRenderer {
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    multisample_view: wgpu::TextureView,
    multisample_srgb: wgpu::TextureView,
    assets: PlacedScenes,
    depth_view: wgpu::TextureView,
    uniform_buffer: wgpu::Buffer,
    uniform_group: wgpu::BindGroup,
    surface_pipeline: wgpu::RenderPipeline,
    surface_depth_pipeline: wgpu::RenderPipeline,
    xray_surface_pipeline: wgpu::RenderPipeline,
    xray_edge_pipeline: wgpu::RenderPipeline,
    xray_hidden_edge_pipeline: wgpu::RenderPipeline,
    edge_pipeline: wgpu::RenderPipeline,
    wire_pipeline: wgpu::RenderPipeline,
    grid_pipeline: wgpu::RenderPipeline,
    triangle_buffer: Option<wgpu::Buffer>,
    edge_buffer: Option<wgpu::Buffer>,
    triangle_vertex_count: u32,
    edge_vertex_count: u32,
    object_ranges: Vec<ObjectRange>,
    edge_ranges: Vec<(u64, Range<u32>)>,
    loose_edge_ranges: Vec<(u64, Range<u32>)>,
    visible_objects: Option<BTreeSet<u64>>,
    edit_selection_object: Option<u64>,
    outline: ObjectOutline,
    edit: EditOverlay,
    gizmo: GizmoOverlay,
}

impl SceneRenderer {
    pub fn clear_mesh(&mut self) {
        self.assets.clear();
        self.triangle_buffer = None;
        self.edge_buffer = None;
        self.triangle_vertex_count = 0;
        self.edge_vertex_count = 0;
        self.object_ranges.clear();
        self.edge_ranges.clear();
        self.loose_edge_ranges.clear();
        self.visible_objects = None;
        self.edit_selection_object = None;
        self.outline.set_ranges(&[], None);
        self.edit.clear_mesh();
        self.gizmo.clear();
    }

    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let (view, multisample_view, multisample_srgb, depth_view) =
            attachments(device, width, height);
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("n3 viewport camera and orientation"),
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("n3 viewport uniform layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(size_of::<Uniforms>() as u64),
                },
                count: None,
            }],
        });
        let uniform_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("n3 viewport uniforms"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("n3 viewport pipeline layout"),
            bind_group_layouts: &[Some(&uniform_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("n3 viewport lighting and grid"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("inspection.wgsl"),
                    "\n",
                    include_str!("viewport.wgsl")
                )
                .into(),
            ),
        });

        let make_pipeline = |label: &'static str,
                             vertex_entry: &'static str,
                             fragment_entry: &'static str,
                             topology: wgpu::PrimitiveTopology,
                             mesh_input: bool,
                             depth_write: bool,
                             depth_compare: wgpu::CompareFunction,
                             write_mask: wgpu::ColorWrites| {
            let vertex_layout = [Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &VERTEX_ATTRIBUTES,
            })];
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vertex_entry),
                    compilation_options: Default::default(),
                    buffers: if mesh_input { &vertex_layout } else { &[] },
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(depth_compare),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: SAMPLE_COUNT,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment_entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };

        let outline = ObjectOutline::new(device, width, height, &uniform_layout);
        let edit = EditOverlay::new(device, &uniform_layout);
        Self {
            view,
            width,
            height,
            multisample_view,
            multisample_srgb,
            assets: PlacedScenes::default(),
            depth_view,
            uniform_buffer,
            uniform_group,
            surface_pipeline: make_pipeline(
                "n3 double-sided surface",
                "vs_mesh",
                "fs_surface",
                wgpu::PrimitiveTopology::TriangleList,
                true,
                true,
                wgpu::CompareFunction::LessEqual,
                wgpu::ColorWrites::ALL,
            ),
            surface_depth_pipeline: make_pipeline(
                "n3 viewport nearest-surface depth",
                "vs_mesh",
                "fs_depth",
                wgpu::PrimitiveTopology::TriangleList,
                true,
                true,
                wgpu::CompareFunction::LessEqual,
                wgpu::ColorWrites::empty(),
            ),
            xray_surface_pipeline: make_pipeline(
                "n3 x-ray nearest-surface shape cue",
                "vs_mesh",
                "fs_xray_surface",
                wgpu::PrimitiveTopology::TriangleList,
                true,
                false,
                wgpu::CompareFunction::LessEqual,
                wgpu::ColorWrites::ALL,
            ),
            xray_edge_pipeline: make_pipeline(
                "n3 x-ray visible polygon edges",
                "vs_edge",
                "fs_wire",
                wgpu::PrimitiveTopology::LineList,
                true,
                false,
                wgpu::CompareFunction::LessEqual,
                wgpu::ColorWrites::ALL,
            ),
            xray_hidden_edge_pipeline: make_pipeline(
                "n3 x-ray occluded polygon edges",
                "vs_edge",
                "fs_xray_edge",
                wgpu::PrimitiveTopology::LineList,
                true,
                false,
                wgpu::CompareFunction::Greater,
                wgpu::ColorWrites::ALL,
            ),
            edge_pipeline: make_pipeline(
                "n3 original polygon edges",
                "vs_edge",
                "fs_edge",
                wgpu::PrimitiveTopology::LineList,
                true,
                false,
                wgpu::CompareFunction::LessEqual,
                wgpu::ColorWrites::ALL,
            ),
            wire_pipeline: make_pipeline(
                "n3 wireframe original polygon edges",
                "vs_edge",
                "fs_wire",
                wgpu::PrimitiveTopology::LineList,
                true,
                false,
                wgpu::CompareFunction::Always,
                wgpu::ColorWrites::ALL,
            ),
            grid_pipeline: make_pipeline(
                "n3 ground grid",
                "vs_grid",
                "fs_grid",
                wgpu::PrimitiveTopology::TriangleList,
                false,
                false,
                wgpu::CompareFunction::LessEqual,
                wgpu::ColorWrites::ALL,
            ),
            triangle_buffer: None,
            edge_buffer: None,
            triangle_vertex_count: 0,
            edge_vertex_count: 0,
            object_ranges: Vec::new(),
            edge_ranges: Vec::new(),
            loose_edge_ranges: Vec::new(),
            visible_objects: None,
            edit_selection_object: None,
            outline,
            edit,
            gizmo: GizmoOverlay::new(device),
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if (width, height) == (self.width, self.height) {
            return;
        }
        (
            self.view,
            self.multisample_view,
            self.multisample_srgb,
            self.depth_view,
        ) = attachments(device, width, height);
        self.outline.resize(device, width, height);
        self.width = width;
        self.height = height;
    }

    pub fn set_assets(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        assets: &[PlacedScene],
        frame: &crate::document::DisplayFrame,
    ) -> Result<(), String> {
        self.assets.set(device, queue, assets, frame)
    }

    /// Proxy geometry expands imported triangle indices into separate vertices
    /// and edge endpoints. Its buffers must fit independently of the indexed
    /// imported draws, before either renderer publishes a candidate.
    pub(crate) fn validate_mesh(device: &wgpu::Device, mesh: &MeshData) -> Result<(), String> {
        mesh_upload_budget(
            mesh.vertices.len(),
            mesh.edges.len(),
            device.limits().max_buffer_size,
        )
    }

    pub fn set_mesh(&mut self, device: &wgpu::Device, mesh: &MeshData) -> Result<(), String> {
        Self::validate_mesh(device, mesh)?;
        self.triangle_vertex_count = mesh.vertices.len() as u32;
        self.edge_vertex_count = mesh.edges.len() as u32;
        let upload = |label: &'static str, vertices: &[Vertex]| {
            (!vertices.is_empty()).then(|| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: bytemuck::cast_slice(vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                })
            })
        };
        self.triangle_buffer = upload("n3 mesh triangles", &mesh.vertices);
        self.edge_buffer = upload("n3 original polygon edge vertices", &mesh.edges);
        self.object_ranges.clone_from(&mesh.object_ranges);
        self.edge_ranges = mesh
            .object_ranges
            .iter()
            .map(|object| (object.object, object.edges.clone()))
            .collect();
        self.loose_edge_ranges = mesh
            .object_ranges
            .iter()
            .filter(|object| !object.loose_edges.is_empty())
            .map(|object| (object.object, object.loose_edges.clone()))
            .collect();
        self.outline
            .set_ranges(&self.object_ranges, self.visible_objects.as_ref());
        self.edit.set_mesh(device, mesh);
        Ok(())
    }

    pub fn set_edit_selection(&mut self, device: &wgpu::Device, selection: EditSelection) {
        self.edit_selection_object = selection.object;
        self.edit.set_selection(device, selection);
    }

    pub fn set_highlights(&mut self, highlights: ObjectHighlights) {
        if self.outline.highlights != highlights {
            self.outline.highlights = highlights;
            self.outline.resolve_ranges(self.visible_objects.as_ref());
        }
    }

    pub fn set_transform_gizmo(&mut self, queue: &wgpu::Queue, vertices: Vec<GizmoVertex>) {
        self.gizmo.update(queue, vertices);
    }

    /// Restrict drawing to the given object IDs without changing the uploaded
    /// mesh. `None` restores the complete scene; an empty set draws no objects.
    pub fn set_visible_objects(&mut self, visible: Option<&BTreeSet<u64>>) {
        if self.visible_objects.as_ref() != visible {
            self.visible_objects = visible.cloned();
            self.outline.resolve_ranges(self.visible_objects.as_ref());
        }
    }

    /// Keep outline widths in logical points on both Retina and capture frames.
    pub fn set_pixel_scale(&mut self, pixels_per_point: f32) {
        if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
            self.outline.pixel_scale = pixels_per_point.clamp(0.25, 8.0);
            self.edit.set_pixel_scale(self.outline.pixel_scale);
        }
    }

    pub fn render(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        camera: &Camera,
        options: ViewportRenderOptions,
    ) -> Result<(), String> {
        let model = crate::orientation::display_rotation(options.z_up);
        self.assets.prepare_view(
            queue,
            camera,
            [self.width, self.height],
            model,
            options.background,
            self.visible_objects.as_ref(),
        )?;
        let native_visible = if self.assets.is_empty()
            || options.xray
            || options.shading == ShadingMode::Wireframe
        {
            self.visible_objects.clone()
        } else {
            let asset_ids: BTreeSet<_> = self.assets.ids().collect();
            Some(
                self.object_ranges
                    .iter()
                    .filter(|range| {
                        !asset_ids.contains(&range.object)
                            && object_is_visible(self.visible_objects.as_ref(), range.object)
                    })
                    .map(|range| range.object)
                    .collect(),
            )
        };
        let uniforms = Uniforms {
            view_projection: camera
                .view_projection(self.width as f32 / self.height as f32)
                .to_cols_array_2d(),
            model: model.to_cols_array_2d(),
            eye: camera.eye().extend(1.0).to_array(),
            wire_color: wire_color(options.background),
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        self.edit.write_uniforms(
            queue,
            self.width,
            self.height,
            (options.shading == ShadingMode::Wireframe || options.xray)
                .then_some(uniforms.wire_color),
        );
        let highlight_active = self.outline.active() && self.triangle_buffer.is_some();
        if highlight_active {
            self.outline.write_uniforms(queue);
            self.outline.render_mask(
                encoder,
                &self.uniform_group,
                self.triangle_buffer.as_ref().unwrap(),
                self.triangle_vertex_count,
                &self.object_ranges,
                self.visible_objects.as_ref(),
            );
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("n3 model viewport"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.multisample_view,
                depth_slice: None,
                resolve_target: Some(&self.view),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: f64::from(options.background.r()) / 255.0,
                        g: f64::from(options.background.g()) / 255.0,
                        b: f64::from(options.background.b()) / 255.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &self.uniform_group, &[]);
        let wireframe = options.shading == ShadingMode::Wireframe;
        // Wireframe and X-ray retain the grid behind their surface cues. The
        // nearest-surface depth remains available for front/rear feedback.
        if (wireframe || options.xray) && options.show_grid {
            pass.set_pipeline(&self.grid_pipeline);
            pass.draw(0..6, 0..1);
        }
        if let Some(buffer) = &self.triangle_buffer {
            pass.set_pipeline(if wireframe || options.xray {
                &self.surface_depth_pipeline
            } else {
                &self.surface_pipeline
            });
            pass.set_vertex_buffer(0, buffer.slice(..));
            for range in triangle_draw_ranges(
                self.triangle_vertex_count,
                &self.object_ranges,
                native_visible.as_ref(),
            ) {
                pass.draw(range, 0..1);
            }
            if options.xray && !wireframe {
                // Depth is complete before blending the nearest surface once.
                // This is an editor shape cue, not transparent material rendering;
                // hidden shells cannot darken it according to submission order.
                pass.set_pipeline(&self.xray_surface_pipeline);
                for range in triangle_draw_ranges(
                    self.triangle_vertex_count,
                    &self.object_ranges,
                    native_visible.as_ref(),
                ) {
                    pass.draw(range, 0..1);
                }
            }
        }
        let imported = !self.assets.is_empty();
        if imported {
            drop(pass);
            {
                let mut imported_pass =
                    self.continue_pass(encoder, "n3 imported opaque geometry", true);
                self.assets.draw(
                    &mut imported_pass,
                    camera.view_projection(self.width as f32 / self.height as f32) * model,
                    self.visible_objects.as_ref(),
                    false,
                    options.shading,
                    options.xray,
                );
            }
            pass = self.continue_pass(encoder, "n3 shared grid", false);
            pass.set_bind_group(0, &self.uniform_group, &[]);
        }
        if !wireframe && !options.xray && options.show_grid {
            pass.set_pipeline(&self.grid_pipeline);
            pass.draw(0..6, 0..1);
        }
        if imported && options.shading == ShadingMode::MaterialPreview && !options.xray {
            drop(pass);
            {
                let mut imported_pass =
                    self.continue_pass(encoder, "n3 imported transparent geometry", true);
                self.assets.draw(
                    &mut imported_pass,
                    camera.view_projection(self.width as f32 / self.height as f32) * model,
                    self.visible_objects.as_ref(),
                    true,
                    ShadingMode::MaterialPreview,
                    false,
                );
            }
            pass = self.continue_pass(encoder, "n3 shared editor feedback", false);
            pass.set_bind_group(0, &self.uniform_group, &[]);
        }
        let edit_visible = self
            .edit_selection_object
            .is_none_or(|object| object_is_visible(self.visible_objects.as_ref(), object));
        if !wireframe
            && edit_visible
            && let Some(buffer) = &self.triangle_buffer
        {
            self.edit.draw_faces(&mut pass, buffer, options.xray);
        }
        if options.xray
            && let Some(buffer) = &self.edge_buffer
        {
            pass.set_vertex_buffer(0, buffer.slice(..));
            for pipeline in [&self.xray_hidden_edge_pipeline, &self.xray_edge_pipeline] {
                pass.set_pipeline(pipeline);
                for (object, edges) in &self.edge_ranges {
                    if !object_is_visible(self.visible_objects.as_ref(), *object) {
                        continue;
                    }
                    for ordinary in self.edit.ordinary_edge_ranges(self.edge_vertex_count) {
                        if let Some(range) = range_intersection(edges, &ordinary) {
                            pass.draw(range, 0..1);
                        }
                    }
                }
            }
        } else if (wireframe || options.show_edges || !self.loose_edge_ranges.is_empty())
            && let Some(buffer) = &self.edge_buffer
        {
            pass.set_pipeline(if wireframe {
                &self.wire_pipeline
            } else {
                &self.edge_pipeline
            });
            pass.set_vertex_buffer(0, buffer.slice(..));
            if wireframe {
                // The full authored wire remains neutral behind surfaces. Edit
                // gradients are composited only on visible edges afterwards.
                if let Some(visible) = &self.visible_objects {
                    for (object, edges) in &self.edge_ranges {
                        if visible.contains(object) {
                            pass.draw(edges.clone(), 0..1);
                        }
                    }
                } else {
                    pass.draw(0..self.edge_vertex_count, 0..1);
                }
            } else if !options.show_edges {
                // Standalone edges are geometry, not the optional overlay on
                // solid faces. Hide only the latter with the edge preference.
                for (object, edges) in &self.loose_edge_ranges {
                    // Imported lines use their evaluated buffer in Solid and
                    // Material Preview; this pass supplies native loose edges.
                    if self.assets.contains(*object) {
                        continue;
                    }
                    if !object_is_visible(self.visible_objects.as_ref(), *object) {
                        continue;
                    }
                    for ordinary in self.edit.ordinary_edge_ranges(self.edge_vertex_count) {
                        if let Some(range) = range_intersection(edges, &ordinary) {
                            pass.draw(range, 0..1);
                        }
                    }
                }
            } else if self.visible_objects.is_some() || !self.assets.is_empty() {
                for (object, edges) in &self.edge_ranges {
                    if !object_is_visible(self.visible_objects.as_ref(), *object) {
                        continue;
                    }
                    let mut edges = edges.clone();
                    if self.assets.contains(*object)
                        && let Some((_, loose)) =
                            self.loose_edge_ranges.iter().find(|(id, _)| id == object)
                    {
                        edges.end = edges.end.min(loose.start);
                    }
                    for ordinary in self.edit.ordinary_edge_ranges(self.edge_vertex_count) {
                        if let Some(range) = range_intersection(&edges, &ordinary) {
                            pass.draw(range, 0..1);
                        }
                    }
                }
            } else {
                for range in self.edit.ordinary_edge_ranges(self.edge_vertex_count) {
                    pass.draw(range, 0..1);
                }
            }
        }
        self.edit.draw_object_highlights(
            &mut pass,
            &self.outline.highlights,
            self.visible_objects.as_ref(),
            options.xray,
        );
        if edit_visible {
            self.edit.draw_edges(&mut pass, options.xray);
        }
        if highlight_active {
            pass.set_pipeline(&self.outline.overlay_pipeline);
            pass.set_bind_group(1, &self.outline.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        drop(pass);
        self.gizmo.render(
            encoder,
            &self.multisample_view,
            &self.view,
            &self.depth_view,
        );
        Ok(())
    }

    fn continue_pass<'a>(
        &self,
        encoder: &'a mut wgpu::CommandEncoder,
        label: &str,
        srgb: bool,
    ) -> wgpu::RenderPass<'a> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: if srgb {
                    &self.multisample_srgb
                } else {
                    &self.multisample_view
                },
                depth_slice: None,
                resolve_target: if srgb { None } else { Some(&self.view) },
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }
}

fn mesh_upload_budget(vertices: usize, edges: usize, max_buffer_size: u64) -> Result<(), String> {
    for (label, count) in [("triangle", vertices), ("edge", edges)] {
        u32::try_from(count)
            .map_err(|_| format!("The viewport {label} proxy exceeds its draw-count limit."))?;
        let bytes = (count as u64)
            .checked_mul(size_of::<Vertex>() as u64)
            .ok_or_else(|| format!("The viewport {label} proxy buffer size overflowed."))?;
        if bytes > max_buffer_size {
            return Err(format!(
                "The viewport {label} proxy requires {bytes} bytes; this GPU supports at most {max_buffer_size} bytes per buffer."
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod mesh_upload_tests {
    use super::*;

    #[test]
    fn imported_proxy_expansion_is_checked_without_allocating_geometry() {
        // Two million indexed triangles can share only three source vertices.
        // Their 12-million-endpoint edge proxy still exceeds a 256 MiB buffer.
        let limit = 256 * 1024 * 1024;
        assert!(
            mesh_upload_budget(6_000_000, 12_000_000, limit)
                .unwrap_err()
                .contains("edge proxy")
        );
        assert!(
            mesh_upload_budget(12_000_000, 0, limit)
                .unwrap_err()
                .contains("triangle proxy")
        );
        let exact = 24 * size_of::<Vertex>() as u64;
        assert!(mesh_upload_budget(24, 24, exact).is_ok());
        assert!(mesh_upload_budget(24, 25, exact).is_err());
        assert!(mesh_upload_budget(0, 0, 0).is_ok());
    }

    #[test]
    fn proxy_draw_counts_cannot_wrap_even_with_an_unlimited_buffer_budget() {
        let overflow = u32::MAX as usize + 1;
        assert!(
            mesh_upload_budget(overflow, 0, u64::MAX)
                .unwrap_err()
                .contains("draw-count")
        );
        assert!(mesh_upload_budget(0, overflow, u64::MAX).is_err());
    }
}

fn wire_color(background: egui::Color32) -> [f32; 4] {
    // These unlit, gamma-encoded neutrals adapt to the viewport rather than to
    // scene lighting. Keep axes, selection, and error colors semantic.
    let brightness = 0.2126 * f32::from(background.r())
        + 0.7152 * f32::from(background.g())
        + 0.0722 * f32::from(background.b());
    if brightness > 128.0 {
        [0.23, 0.25, 0.28, 0.88]
    } else {
        [0.70, 0.73, 0.77, 0.88]
    }
}

fn object_is_visible(visible: Option<&BTreeSet<u64>>, object: u64) -> bool {
    visible.is_none_or(|objects| objects.contains(&object))
}

/// Preserve the single full-buffer draw when isolation is off. During Local
/// View, ranges come from the uploaded mesh rather than a rebuilt geometry set.
fn triangle_draw_ranges<'a>(
    count: u32,
    ranges: &'a [ObjectRange],
    visible: Option<&'a BTreeSet<u64>>,
) -> impl Iterator<Item = Range<u32>> + 'a {
    (visible.is_none()).then_some(0..count).into_iter().chain(
        ranges
            .iter()
            .filter(move |range| {
                visible.is_some_and(|objects| objects.contains(&range.object))
                    && !range.triangles.is_empty()
            })
            .map(|range| range.triangles.clone()),
    )
}

fn range_intersection(a: &Range<u32>, b: &Range<u32>) -> Option<Range<u32>> {
    let overlap = a.start.max(b.start)..a.end.min(b.end);
    (!overlap.is_empty()).then_some(overlap)
}

#[cfg(test)]
#[path = "edit_overlay_tests.rs"]
mod edit_overlay_tests;

#[cfg(test)]
#[path = "shading_tests.rs"]
mod shading_tests;

#[cfg(test)]
mod local_view_tests {
    use super::*;
    use crate::{
        camera::View,
        doc_capture::Capture,
        mesh::{EditFace, EditObjectTopology, EditVertex},
    };

    const WIDTH: u32 = 320;
    const HEIGHT: u32 = 240;

    fn quads(include_occluder: bool) -> MeshData {
        let mut mesh = MeshData {
            vertices: Vec::new(),
            edges: Vec::new(),
            object_ranges: Vec::new(),
            edit_topology: Vec::new(),
            vertex_count: 0,
            face_count: 0,
            triangle_count: 0,
            object_count: 0,
            source_extent: [2.0, 2.0, 0.4],
            warnings: Vec::new(),
        };
        let mut objects = vec![(17, 0.8, 0.0)];
        if include_occluder {
            objects.push((99, 1.0, 0.4));
        }
        for (object, size, depth) in objects {
            let points = [
                [-size, -size, depth],
                [size, -size, depth],
                [size, size, depth],
                [-size, size, depth],
            ];
            let vertex = |index: usize| Vertex {
                position: points[index],
                normal: [0.0, 0.0, 1.0],
            };
            let triangles_start = mesh.vertices.len() as u32;
            mesh.vertices.extend([0, 1, 2, 0, 2, 3].map(vertex));
            let triangles = triangles_start..mesh.vertices.len() as u32;
            let edges_start = mesh.edges.len() as u32;
            mesh.edges.extend([0, 1, 1, 2, 2, 3, 3, 0].map(vertex));
            mesh.object_ranges.push(ObjectRange {
                object,
                triangles: triangles.clone(),
                edges: edges_start..edges_start + 8,
                loose_edges: edges_start..edges_start,
            });
            mesh.edit_topology.push(EditObjectTopology {
                object,
                edges: edges_start..mesh.edges.len() as u32,
                loose_edges: mesh.edges.len() as u32..mesh.edges.len() as u32,
                vertices: points
                    .into_iter()
                    .enumerate()
                    .map(|(index, position)| EditVertex {
                        id: index as u64 + 1,
                        position,
                    })
                    .collect(),
                edge_vertices: vec![[1, 2], [2, 3], [3, 4], [4, 1]],
                faces: vec![EditFace {
                    vertices: vec![1, 2, 3, 4],
                    triangles,
                }],
            });
            mesh.vertex_count += 4;
            mesh.face_count += 1;
            mesh.triangle_count += 2;
            mesh.object_count += 1;
        }
        mesh
    }

    fn frame(capture: &mut Capture, ctx: &egui::Context, camera: &Camera) -> Vec<u8> {
        let viewport =
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(WIDTH as f32, HEIGHT as f32));
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
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
                true,
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
    fn local_view_filters_surface_edges_and_outline_occlusion_without_reupload() {
        let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
        let ctx = egui::Context::default();
        let mut camera = Camera::default();
        camera.set_view(View::Front);
        capture
            .scene
            .set_mesh(&capture.device, &quads(true))
            .unwrap();
        capture.scene.set_highlights(ObjectHighlights {
            selected: BTreeSet::from([17]),
            hovered: Some(99),
        });
        capture.scene.set_edit_selection(
            &capture.device,
            EditSelection {
                object: Some(17),
                vertices: BTreeSet::from([1, 2, 3, 4]),
            },
        );
        let full = frame(&mut capture, &ctx, &camera);
        let triangle_buffer = capture.scene.triangle_buffer.as_ref().unwrap() as *const _;
        let edge_buffer = capture.scene.edge_buffer.as_ref().unwrap() as *const _;

        capture
            .scene
            .set_visible_objects(Some(&BTreeSet::from([17])));
        let isolated = frame(&mut capture, &ctx, &camera);
        assert!(std::ptr::eq(
            triangle_buffer,
            capture.scene.triangle_buffer.as_ref().unwrap()
        ));
        assert!(std::ptr::eq(
            edge_buffer,
            capture.scene.edge_buffer.as_ref().unwrap()
        ));
        assert_ne!(full, isolated, "The foreground occluder must disappear");

        capture
            .scene
            .set_mesh(&capture.device, &quads(false))
            .unwrap();
        assert_eq!(
            isolated,
            frame(&mut capture, &ctx, &camera),
            "Isolated surfaces, edges, face tint, and object outline match a one-object scene"
        );

        capture
            .scene
            .set_mesh(&capture.device, &quads(true))
            .unwrap();
        capture.scene.set_visible_objects(Some(&BTreeSet::new()));
        let empty = frame(&mut capture, &ctx, &camera);
        capture.scene.clear_mesh();
        assert_eq!(
            empty,
            frame(&mut capture, &ctx, &camera),
            "An empty filter must also hide edit chrome and object outlines"
        );
        capture
            .scene
            .set_mesh(&capture.device, &quads(true))
            .unwrap();
        capture.scene.set_edit_selection(
            &capture.device,
            EditSelection {
                object: Some(17),
                vertices: BTreeSet::from([1, 2, 3, 4]),
            },
        );
        capture.scene.set_visible_objects(None);
        assert_eq!(full, frame(&mut capture, &ctx, &camera));
    }

    #[test]
    fn unfiltered_scene_keeps_one_full_triangle_draw() {
        let ranges = [
            ObjectRange {
                object: 17,
                triangles: 0..6,
                edges: 0..0,
                loose_edges: 0..0,
            },
            ObjectRange {
                object: 99,
                triangles: 6..12,
                edges: 0..0,
                loose_edges: 0..0,
            },
        ];
        assert_eq!(
            triangle_draw_ranges(12, &ranges, None).collect::<Vec<_>>(),
            vec![0..12]
        );
        assert_eq!(
            triangle_draw_ranges(12, &ranges, Some(&BTreeSet::from([99]))).collect::<Vec<_>>(),
            vec![6..12]
        );
        assert_eq!(range_intersection(&(0..8), &(4..12)), Some(4..8));
        assert_eq!(range_intersection(&(0..4), &(4..12)), None);
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct OutlineUniforms {
    selected: [f32; 4],
    hovered: [f32; 4],
    widths: [f32; 4],
}

struct ObjectOutline {
    mask: wgpu::TextureView,
    depth: wgpu::TextureView,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    bind_layout: wgpu::BindGroupLayout,
    mask_pipeline: wgpu::RenderPipeline,
    overlay_pipeline: wgpu::RenderPipeline,
    ranges: BTreeMap<u64, Range<u32>>,
    selected: Vec<Range<u32>>,
    hovered: Option<Range<u32>>,
    highlights: ObjectHighlights,
    pixel_scale: f32,
}

impl ObjectOutline {
    fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        scene_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let (mask, depth) = mask_attachments(device, width, height);
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("n3 object outline style"),
            size: size_of::<OutlineUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("n3 object outline layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Uint,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(
                                size_of::<OutlineUniforms>() as u64
                            ),
                        },
                        count: None,
                    },
                ],
            });
        let bind_group = outline_group(device, &bind_layout, &mask, &uniform_buffer);
        let mask_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("n3 object mask pipeline layout"),
            bind_group_layouts: &[Some(scene_layout)],
            immediate_size: 0,
        });
        let overlay_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("n3 object outline pipeline layout"),
            bind_group_layouts: &[Some(scene_layout), Some(&bind_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("n3 visible object silhouettes"),
            source: wgpu::ShaderSource::Wgsl(include_str!("object_outline.wgsl").into()),
        });
        let mask_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("n3 visible object mask"),
            layout: Some(&mask_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_mask"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &VERTEX_ATTRIBUTES,
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
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_mask"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: MASK_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let overlay_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("n3 visible silhouette overlay"),
            layout: Some(&overlay_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_outline"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: SAMPLE_COUNT,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_outline"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            mask,
            depth,
            uniform_buffer,
            bind_group,
            bind_layout,
            mask_pipeline,
            overlay_pipeline,
            ranges: BTreeMap::new(),
            selected: Vec::new(),
            hovered: None,
            highlights: ObjectHighlights::default(),
            pixel_scale: 1.0,
        }
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        (self.mask, self.depth) = mask_attachments(device, width, height);
        self.bind_group =
            outline_group(device, &self.bind_layout, &self.mask, &self.uniform_buffer);
    }

    fn set_ranges(&mut self, ranges: &[ObjectRange], visible: Option<&BTreeSet<u64>>) {
        self.ranges = ranges
            .iter()
            .filter(|range| !range.triangles.is_empty())
            .map(|range| (range.object, range.triangles.clone()))
            .collect();
        self.resolve_ranges(visible);
    }

    fn resolve_ranges(&mut self, visible: Option<&BTreeSet<u64>>) {
        self.selected = self
            .highlights
            .selected
            .iter()
            .filter(|id| object_is_visible(visible, **id))
            .filter_map(|id| self.ranges.get(id).cloned())
            .collect();
        self.hovered = self
            .highlights
            .hovered
            .filter(|id| object_is_visible(visible, *id) && !self.highlights.selected.contains(id))
            .and_then(|id| self.ranges.get(&id).cloned());
    }

    fn active(&self) -> bool {
        !self.selected.is_empty() || self.hovered.is_some()
    }

    fn write_uniforms(&self, queue: &wgpu::Queue) {
        let uniforms = OutlineUniforms {
            selected: SELECTED_COLOR
                .to_array()
                .map(|byte| f32::from(byte) / 255.0),
            hovered: HOVERED_COLOR.to_array().map(|byte| f32::from(byte) / 255.0),
            widths: [
                SELECTED_WIDTH * self.pixel_scale,
                HOVERED_WIDTH * self.pixel_scale,
                0.0,
                0.0,
            ],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    fn render_mask(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scene_group: &wgpu::BindGroup,
        triangles: &wgpu::Buffer,
        count: u32,
        ranges: &[ObjectRange],
        visible: Option<&BTreeSet<u64>>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("n3 visible object mask"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.mask,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.mask_pipeline);
        pass.set_bind_group(0, scene_group, &[]);
        pass.set_vertex_buffer(0, triangles.slice(..));
        // Only drawn surfaces establish depth. Hidden objects must not occlude
        // a visible object's selected or hovered silhouette.
        // instance_index is a compact mask state, never a canonical object ID.
        for range in triangle_draw_ranges(count, ranges, visible) {
            pass.draw(range, 0..1);
        }
        if let Some(range) = &self.hovered {
            pass.draw(range.clone(), 2..3);
        }
        for range in &self.selected {
            pass.draw(range.clone(), 1..2);
        }
    }
}

fn outline_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    mask: &wgpu::TextureView,
    uniforms: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("n3 object outline mask and style"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(mask),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniforms.as_entire_binding(),
            },
        ],
    })
}

fn mask_attachments(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::TextureView, wgpu::TextureView) {
    let make_view = |label, format, usage| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
            .create_view(&Default::default())
    };
    (
        make_view(
            "n3 visible object mask",
            MASK_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        ),
        make_view(
            "n3 object mask depth",
            DEPTH_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        ),
    )
}

fn attachments(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (
    wgpu::TextureView,
    wgpu::TextureView,
    wgpu::TextureView,
    wgpu::TextureView,
) {
    let texture = |label, format, sample_count, usage, view_formats: &[wgpu::TextureFormat]| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats,
        })
    };
    let resolved = texture(
        "n3 viewport resolved color",
        COLOR_FORMAT,
        1,
        wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        &[],
    );
    // Both views refer to the same samples. Workbench shaders encode their
    // historical gamma output; PBR uses hardware SRGB conversion for linear
    // blending. This is attachment reuse, never compositing finished images.
    let color = texture(
        "n3 shared viewport MSAA",
        COLOR_FORMAT,
        SAMPLE_COUNT,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
        &[wgpu::TextureFormat::Rgba8UnormSrgb],
    );
    let srgb = color.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        ..Default::default()
    });
    let depth = texture(
        "n3 shared viewport depth",
        DEPTH_FORMAT,
        SAMPLE_COUNT,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
        &[],
    );
    (
        resolved.create_view(&Default::default()),
        color.create_view(&Default::default()),
        srgb,
        depth.create_view(&Default::default()),
    )
}

#[cfg(test)]
#[path = "unified_tests.rs"]
mod unified_tests;
