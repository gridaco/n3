//! PBR pipelines and cached immutable scene resources. The workbench orchestrates
//! these draws alongside native geometry using shared color and depth attachments.
use std::{collections::BTreeMap, sync::Arc};

use bytemuck::{Pod, Zeroable};
#[cfg(test)]
use glam::{DMat4, camera::rh::proj::directx as projection};
use glam::{DVec3, Mat4, Vec3};
use wgpu::util::DeviceExt;

use super::shading::ShadingMode;
use crate::scene::{self, Bounds, EvaluatedScene, SceneAsset, Topology, display_limits};

#[path = "scene_environment.rs"]
mod environment;
#[path = "scene_textures.rs"]
mod textures;

#[cfg(test)]
use crate::camera::Camera;

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SAMPLES: u32 = 4;
const MAX_LIGHTS: usize = scene::MAX_ACTIVE_LIGHTS;
// GPU geometry allocations have their own budget, independent of decoded data.
const MAX_GEOMETRY_BYTES: u64 = 512 * 1024 * 1024;
const ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
    0 => Float32x3, 1 => Float32x3, 2 => Float32x4,
    3 => Float32x2, 4 => Float32x2, 5 => Float32x4
];
type Result<T> = std::result::Result<T, String>;
use display_limits::finite_values;

/// Fixed rest-pose framing. Camera navigation uses display coordinates while
/// light attenuation retains centimeter-to-meter distances. Animation never
/// changes this normalization or follows a moving object's bounds implicitly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ViewTransform {
    pub center_cm: DVec3,
    /// Display units per centimeter.
    pub scale: f64,
}
impl Default for ViewTransform {
    fn default() -> Self {
        Self {
            center_cm: DVec3::ZERO,
            scale: 1.0,
        }
    }
}
impl ViewTransform {
    pub fn from_bounds(bounds: Option<Bounds>) -> Self {
        let Some(bounds) = bounds else {
            return Self::default();
        };
        let extent = bounds.max - bounds.min;
        Self {
            center_cm: bounds.min + extent * 0.5,
            scale: 2.0 / extent.max_element().max(1e-6),
        }
    }
    pub fn point(self, world_cm: DVec3) -> Vec3 {
        display_limits::point(self.center_cm, self.scale, world_cm)
    }
    fn meters_per_display_unit(self) -> f32 {
        display_limits::meters_per_display_unit(self.scale)
    }
    fn validate(self) -> Result<()> {
        display_limits::validate_frame(self.center_cm, self.scale)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    tangent: [f32; 4],
    uv0: [f32; 2],
    uv1: [f32; 2],
    color: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LightUniform {
    position_kind: [f32; 4],
    direction_range: [f32; 4],
    color_intensity: [f32; 4],
    spot: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_projection: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    eye_exposure: [f32; 4],
    settings: [f32; 4],
    environment: [f32; 4],
    lights: [LightUniform; MAX_LIGHTS],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct UvUniform {
    x: [f32; 4],
    y: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MaterialUniform {
    base_color: [f32; 4],
    emissive: [f32; 4],
    pbr: [f32; 4],
    flags: [f32; 4],
    uv: [UvUniform; 5],
}
#[derive(Clone)]
struct GpuMaterial {
    group: wgpu::BindGroup,
    blend: bool,
    double_sided: bool,
}
#[derive(Clone)]
struct GpuDraw {
    identity: (usize, usize, usize),
    vertices: Vec<Vertex>,
    indices: Arc<[u32]>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    wire: Option<(wgpu::Buffer, u32)>,
    material: usize,
    topology: Topology,
    mirrored: bool,
    center: Vec3,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct UploadCounts {
    vertices: usize,
    indices: usize,
    wire: usize,
}

pub(super) struct PreparedFrame {
    transform: ViewTransform,
    vertices: Vec<Vec<Vertex>>,
}

/// Instance-specific draw state, with pipelines and immutable resources shared.
/// Framebuffer ownership belongs to the workbench. Isolated targets exist only
/// in rendering tests, for focused material and camera contract regressions.
pub(crate) struct PbrSceneRenderer {
    #[cfg(test)]
    pub view: wgpu::TextureView,
    #[cfg(test)]
    pub width: u32,
    #[cfg(test)]
    pub height: u32,
    #[cfg(test)]
    color: wgpu::Texture,
    #[cfg(test)]
    color_view: wgpu::TextureView,
    #[cfg(test)]
    multisample: wgpu::TextureView,
    #[cfg(test)]
    depth: wgpu::TextureView,
    uniform: wgpu::Buffer,
    globals_group: wgpu::BindGroup,
    globals_layout: wgpu::BindGroupLayout,
    material_layout: wgpu::BindGroupLayout,
    environment_layout: wgpu::BindGroupLayout,
    environment: Option<environment::Environment>,
    pipelines: BTreeMap<(u8, bool, bool, bool), wgpu::RenderPipeline>,
    solid_pipelines: BTreeMap<(u8, bool), wgpu::RenderPipeline>,
    materials: Vec<GpuMaterial>,
    lights: Vec<scene::Light>,
    camera_definitions: Vec<scene::Camera>,
    frame_lights: Vec<scene::EvaluatedLight>,
    frame_cameras: Vec<scene::EvaluatedCamera>,
    draws: Vec<GpuDraw>,
    pub transform: ViewTransform,
    framing_initialized: bool,
    #[cfg(test)]
    wireframe: bool,
    #[cfg(test)]
    uploads: UploadCounts,
}

impl PbrSceneRenderer {
    pub fn new(device: &wgpu::Device) -> Self {
        #[cfg(test)]
        let (color, view, color_view, multisample, depth) = attachments(device, 1, 1);
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("n3 imported scene frame uniforms"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let global_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("n3 imported scene global layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(size_of::<Globals>() as u64),
                },
                count: None,
            }],
        });
        let globals_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("n3 imported scene globals"),
            layout: &global_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let mut entries = vec![wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(size_of::<MaterialUniform>() as u64),
            },
            count: None,
        }];
        for slot in 0..5 {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: 1 + slot * 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: 2 + slot * 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            });
        }
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("n3 scene material layout"),
            entries: &entries,
        });
        let environment_layout = environment::Environment::layout(device);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("n3 scene pipeline layout"),
            bind_group_layouts: &[
                Some(&global_layout),
                Some(&material_layout),
                Some(&environment_layout),
            ],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("n3 scene metallic roughness shader"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("inspection.wgsl"),
                    "\n",
                    include_str!("scene.wgsl")
                )
                .into(),
            ),
        });
        let mut pipelines = BTreeMap::new();
        for topology in [Topology::Triangles, Topology::Lines, Topology::Points] {
            for blend in [false, true] {
                for double_sided in [false, true] {
                    for mirrored in [false, true] {
                        let key = pipeline_key(topology, blend, double_sided, mirrored);
                        if pipelines.contains_key(&key) {
                            continue;
                        }
                        let triangles = topology == Topology::Triangles;
                        let pipeline =
                            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                                label: Some("n3 scene scene pipeline"),
                                layout: Some(&layout),
                                vertex: wgpu::VertexState {
                                    module: &shader,
                                    entry_point: Some("vs_scene"),
                                    compilation_options: Default::default(),
                                    buffers: &[Some(wgpu::VertexBufferLayout {
                                        array_stride: size_of::<Vertex>() as u64,
                                        step_mode: wgpu::VertexStepMode::Vertex,
                                        attributes: &ATTRIBUTES,
                                    })],
                                },
                                primitive: wgpu::PrimitiveState {
                                    topology: match topology {
                                        Topology::Points => wgpu::PrimitiveTopology::PointList,
                                        Topology::Lines => wgpu::PrimitiveTopology::LineList,
                                        Topology::Triangles => {
                                            wgpu::PrimitiveTopology::TriangleList
                                        }
                                    },
                                    front_face: if mirrored {
                                        wgpu::FrontFace::Cw
                                    } else {
                                        wgpu::FrontFace::Ccw
                                    },
                                    cull_mode: if triangles && !double_sided {
                                        Some(wgpu::Face::Back)
                                    } else {
                                        None
                                    },
                                    ..Default::default()
                                },
                                depth_stencil: Some(wgpu::DepthStencilState {
                                    format: DEPTH_FORMAT,
                                    depth_write_enabled: Some(!blend),
                                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                                    stencil: Default::default(),
                                    bias: Default::default(),
                                }),
                                multisample: wgpu::MultisampleState {
                                    count: SAMPLES,
                                    ..Default::default()
                                },
                                fragment: Some(wgpu::FragmentState {
                                    module: &shader,
                                    entry_point: Some(if triangles {
                                        "fs_scene"
                                    } else {
                                        "fs_unlit"
                                    }),
                                    compilation_options: Default::default(),
                                    targets: &[Some(wgpu::ColorTargetState {
                                        format: COLOR_FORMAT,
                                        blend: blend.then_some(wgpu::BlendState::ALPHA_BLENDING),
                                        write_mask: wgpu::ColorWrites::ALL,
                                    })],
                                }),
                                multiview_mask: None,
                                cache: None,
                            });
                        pipelines.insert(key, pipeline);
                    }
                }
            }
        }
        // Wireframe is a separate inspection pass, using cached triangle
        // boundaries. Evaluated scene draws do not carry authored polygon boundaries.
        for (key, topology) in [
            (3, wgpu::PrimitiveTopology::LineList),
            (4, wgpu::PrimitiveTopology::PointList),
        ] {
            pipelines.insert(
                (key, false, true, false),
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("n3 scene wire inspection"),
                    layout: Some(&layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_scene"),
                        compilation_options: Default::default(),
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: size_of::<Vertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &ATTRIBUTES,
                        })],
                    },
                    primitive: wgpu::PrimitiveState {
                        topology,
                        cull_mode: None,
                        ..Default::default()
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: DEPTH_FORMAT,
                        depth_write_enabled: Some(false),
                        depth_compare: Some(wgpu::CompareFunction::Always),
                        stencil: Default::default(),
                        bias: Default::default(),
                    }),
                    multisample: wgpu::MultisampleState {
                        count: SAMPLES,
                        ..Default::default()
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_wire"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: COLOR_FORMAT,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                }),
            );
        }
        // Solid is a view policy, not a material mutation. Preserve evaluated
        // vertex normals/poses while ignoring all authored appearance, including
        // opacity and culling. These pipelines share the cached source buffers.
        let mut solid_pipelines = BTreeMap::new();
        for (topology, mirrored) in [
            (Topology::Triangles, false),
            (Topology::Triangles, true),
            (Topology::Lines, false),
            (Topology::Points, false),
        ] {
            let key = pipeline_key(topology, false, true, mirrored);
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("n3 imported neutral solid inspection"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_scene"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &ATTRIBUTES,
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    topology: match topology {
                        Topology::Triangles => wgpu::PrimitiveTopology::TriangleList,
                        Topology::Lines => wgpu::PrimitiveTopology::LineList,
                        Topology::Points => wgpu::PrimitiveTopology::PointList,
                    },
                    front_face: if mirrored {
                        wgpu::FrontFace::Cw
                    } else {
                        wgpu::FrontFace::Ccw
                    },
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: SAMPLES,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(if topology == Topology::Triangles {
                        "fs_solid"
                    } else {
                        "fs_wire"
                    }),
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
            solid_pipelines.insert((key.0, key.3), pipeline);
        }
        Self {
            #[cfg(test)]
            view,
            #[cfg(test)]
            width: 1,
            #[cfg(test)]
            height: 1,
            #[cfg(test)]
            color,
            #[cfg(test)]
            color_view,
            #[cfg(test)]
            multisample,
            #[cfg(test)]
            depth,
            uniform,
            globals_group,
            globals_layout: global_layout,
            material_layout,
            environment_layout,
            environment: None,
            pipelines,
            solid_pipelines,
            materials: Vec::new(),
            lights: Vec::new(),
            camera_definitions: Vec::new(),
            frame_lights: Vec::new(),
            frame_cameras: Vec::new(),
            draws: Vec::new(),
            transform: ViewTransform::default(),
            framing_initialized: false,
            #[cfg(test)]
            wireframe: false,
            #[cfg(test)]
            uploads: UploadCounts::default(),
        }
    }

    #[cfg(test)]
    pub fn set_wireframe(&mut self, enabled: bool) {
        self.wireframe = enabled;
    }

    #[cfg(test)]
    pub fn new_for_test(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let mut renderer = Self::new(device);
        renderer.resize(device, width, height);
        renderer
    }

    #[cfg(test)]
    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        (
            self.color,
            self.view,
            self.color_view,
            self.multisample,
            self.depth,
        ) = attachments(device, self.width, self.height);
    }

    /// Upload immutable material/texture resources once for a new imported asset.
    pub fn set_asset(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        asset: &SceneAsset,
    ) -> Result<()> {
        let materials = textures::materials(
            device,
            queue,
            &self.material_layout,
            &asset.materials,
            &asset.images,
            &asset.textures,
        )?;
        self.materials = materials;
        self.lights.clone_from(&asset.lights);
        self.camera_definitions.clone_from(&asset.cameras);
        self.frame_cameras.clear();
        self.frame_lights.clear();
        self.draws.clear();
        self.framing_initialized = false;
        Ok(())
    }

    /// Evaluation may deform vertices or move instances. Texture and pipeline
    /// resources remain resident; equal evaluated vertices do not upload again.
    #[cfg(test)]
    pub fn set_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &EvaluatedScene,
    ) -> Result<()> {
        let prepared = self.prepare_frame(device, frame)?;
        self.apply_frame(device, queue, frame, prepared);
        Ok(())
    }

    pub(super) fn prepare_frame(
        &self,
        device: &wgpu::Device,
        frame: &EvaluatedScene,
    ) -> Result<PreparedFrame> {
        if frame.lights.len() > MAX_LIGHTS {
            return Err(format!(
                "This scene has {} punctual lights; the viewer supports at most {MAX_LIGHTS}.",
                frame.lights.len()
            ));
        }
        let transform = if self.framing_initialized {
            self.transform
        } else {
            ViewTransform::from_bounds(frame.bounds)
        };
        // The same pure contract rejects unusable authored placement before an
        // editor candidate can commit. Recheck evaluated GPU input defensively.
        display_limits::validate_display(
            transform.center_cm,
            transform.scale,
            glam::DMat4::IDENTITY,
            frame,
        )?;
        let mut bytes = 0_u64;
        // Preflight counts before allocating derived vertex/wire arrays. Then
        // validate every candidate before publishing any GPU/frame state.
        for draw in &frame.draws {
            if draw.material >= self.materials.len() {
                return Err("An imported draw references a missing material.".into());
            }
            let draw_bytes = draw_budget(
                draw.vertices.len(),
                draw.indices.len(),
                draw.topology,
                device.limits().max_buffer_size,
            )?;
            bytes = bytes
                .checked_add(draw_bytes)
                .filter(|bytes| *bytes <= MAX_GEOMETRY_BYTES)
                .ok_or("Scene exceeds the 512 MiB GPU geometry budget.")?;
            if draw
                .indices
                .iter()
                .any(|index| *index as usize >= draw.vertices.len())
            {
                return Err("An imported draw has an out-of-range vertex index.".into());
            }
        }
        for light in &frame.lights {
            if light.light >= self.lights.len() {
                return Err("An evaluated light has no definition.".into());
            }
            let definition = &self.lights[light.light];
            if !finite_values(&light.direction.as_vec3().to_array())
                || !finite_values(&definition.color)
                || !finite_values(&[definition.intensity, definition.range_cm.unwrap_or(0.0)])
            {
                return Err("A punctual light exceeds the renderer's numeric range.".into());
            }
        }
        if frame
            .cameras
            .iter()
            .any(|camera| camera.camera >= self.camera_definitions.len())
        {
            return Err("An imported camera has no definition.".into());
        }
        let converted = frame
            .draws
            .iter()
            .map(|draw| {
                draw.vertices
                    .iter()
                    .map(|vertex| {
                        let position = transform.point(DVec3::from_array(vertex.position));
                        let vertex = Vertex {
                            position: position.to_array(),
                            normal: vertex.normal,
                            tangent: vertex.tangent,
                            uv0: vertex.uv0,
                            uv1: vertex.uv1,
                            color: vertex.color,
                        };
                        if !finite_values(bytemuck::cast_slice(std::slice::from_ref(&vertex))) {
                            return Err(
                                "Animated geometry exceeds the renderer's numeric range.".into()
                            );
                        }
                        Ok(vertex)
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(PreparedFrame {
            transform,
            vertices: converted,
        })
    }

    pub(super) fn apply_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &EvaluatedScene,
        prepared: PreparedFrame,
    ) {
        let PreparedFrame {
            transform,
            vertices: converted,
        } = prepared;
        if self.environment.is_none() {
            self.environment = Some(environment::Environment::new(
                device,
                queue,
                &self.environment_layout,
            ));
        }
        self.transform = transform;
        self.framing_initialized = true;
        self.frame_lights.clone_from(&frame.lights);
        self.frame_cameras.clone_from(&frame.cameras);
        // Stable evaluation order follows scene traversal. Reuse allocations for
        // unchanged instances; a scene switch intentionally resets that mapping.
        for (index, (source, vertices)) in frame.draws.iter().zip(converted).enumerate() {
            let vertex_bytes = bytemuck::cast_slice(&vertices);
            let index_bytes = bytemuck::cast_slice(source.indices.as_ref());
            let identity = (source.node, source.mesh, source.primitive);
            let center = vertex_center(&vertices);
            if let Some(draw) = self.draws.get_mut(index)
                && draw.identity == identity
                && draw.vertices.len() == vertices.len()
                && draw.indices.len() == source.indices.len()
            {
                if draw.vertices != vertices {
                    if !vertex_bytes.is_empty() {
                        queue.write_buffer(&draw.vertex_buffer, 0, vertex_bytes);
                        #[cfg(test)]
                        {
                            self.uploads.vertices += 1;
                        }
                    }
                    draw.vertices = vertices;
                }
                let indices_changed =
                    !Arc::ptr_eq(&draw.indices, &source.indices) && draw.indices != source.indices;
                if indices_changed || draw.topology != source.topology {
                    draw.wire = wire_buffer(device, source.topology, &source.indices);
                    #[cfg(test)]
                    {
                        self.uploads.wire += usize::from(draw.wire.is_some());
                    }
                }
                if indices_changed {
                    if !index_bytes.is_empty() {
                        queue.write_buffer(&draw.index_buffer, 0, index_bytes);
                        #[cfg(test)]
                        {
                            self.uploads.indices += 1;
                        }
                    }
                    draw.indices = source.indices.clone();
                }
                draw.material = source.material;
                draw.topology = source.topology;
                draw.mirrored = source.mirrored;
                draw.center = center;
            } else {
                let vertex_buffer = buffer(
                    device,
                    "n3 imported vertices",
                    vertex_bytes,
                    wgpu::BufferUsages::VERTEX,
                );
                let index_buffer = buffer(
                    device,
                    "n3 imported indices",
                    index_bytes,
                    wgpu::BufferUsages::INDEX,
                );
                let draw = GpuDraw {
                    identity,
                    vertices,
                    indices: source.indices.clone(),
                    vertex_buffer,
                    index_buffer,
                    wire: wire_buffer(device, source.topology, &source.indices),
                    material: source.material,
                    topology: source.topology,
                    mirrored: source.mirrored,
                    center,
                };
                #[cfg(test)]
                {
                    self.uploads.vertices += 1;
                    self.uploads.indices += 1;
                    self.uploads.wire += usize::from(draw.wire.is_some());
                }
                if index < self.draws.len() {
                    self.draws[index] = draw;
                } else {
                    self.draws.push(draw);
                }
            }
        }
        self.draws.truncate(frame.draws.len());
    }

    #[cfg(test)]
    pub fn render(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        camera: &Camera,
        background: egui::Color32,
        exposure_ev: f32,
        imported_camera: Option<usize>,
    ) -> Result<()> {
        if !exposure_ev.is_finite() {
            return Err("Exposure must be finite.".into());
        }
        let (view_projection, eye_display, viewport) = if let Some(index) = imported_camera {
            self.imported_camera(index)?
        } else {
            (
                camera.view_projection(self.width as f32 / self.height as f32),
                camera.eye(),
                [0.0, 0.0, self.width as f32, self.height as f32],
            )
        };
        self.write_uniforms(
            queue,
            view_projection,
            eye_display,
            Mat4::IDENTITY,
            background,
            exposure_ev,
        )?;
        let mut order: Vec<_> = (0..self.draws.len()).collect();
        // Opaque/masked first, then whole primitives back-to-front. Intersecting
        // transparent triangles require future OIT; no depth writes for BLEND.
        order.sort_by(|a, b| {
            let a = &self.draws[*a];
            let b = &self.draws[*b];
            let a_blend = self.materials[a.material].blend;
            let b_blend = self.materials[b.material].blend;
            a_blend.cmp(&b_blend).then_with(|| {
                if a_blend {
                    let ac = view_projection * a.center.extend(1.0);
                    let bc = view_projection * b.center.extend(1.0);
                    (bc.z / bc.w).total_cmp(&(ac.z / ac.w))
                } else {
                    std::cmp::Ordering::Equal
                }
            })
        });
        let linear_background = background
            .to_array()
            .map(|value| textures::srgb_to_linear(value as f32 / 255.0));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("n3 imported PBR scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.multisample,
                resolve_target: Some(&self.color_view),
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: linear_background[0] as f64,
                        g: linear_background[1] as f64,
                        b: linear_background[2] as f64,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Discard,
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
        // Preserve an authored camera's frame instead of stretching it to the
        // editor viewport. The surrounding letterbox uses the theme background.
        pass.set_viewport(viewport[0], viewport[1], viewport[2], viewport[3], 0.0, 1.0);
        for index in order {
            self.draw(
                &mut pass,
                index,
                if self.wireframe {
                    ShadingMode::Wireframe
                } else {
                    ShadingMode::MaterialPreview
                },
            );
        }
        Ok(())
    }

    pub(super) fn write_uniforms(
        &self,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        eye_display: Vec3,
        model: Mat4,
        background: egui::Color32,
        exposure_ev: f32,
    ) -> Result<()> {
        self.transform.validate()?;
        let units = self.transform.meters_per_display_unit();
        if !finite_values(&(eye_display * units).to_array())
            || !finite_values(&view_projection.to_cols_array())
        {
            return Err("The view exceeds the renderer's coordinate range.".into());
        }
        let mut globals = Globals {
            view_projection: view_projection.to_cols_array_2d(),
            model: model.to_cols_array_2d(),
            eye_exposure: (eye_display * units)
                .extend(exposure_ev.clamp(-20.0, 20.0))
                .to_array(),
            settings: [
                units,
                0.0,
                0.35,
                if background.r() as u32 + background.g() as u32 + background.b() as u32 > 384 {
                    0.08
                } else {
                    0.7
                },
            ],
            environment: [environment::SPECULAR_MAX_LOD, 0.0, 0.0, 0.0],
            lights: [LightUniform::zeroed(); MAX_LIGHTS],
        };
        if self.frame_lights.is_empty() {
            for (index, (direction, intensity)) in [
                (Vec3::new(-0.6, -0.8, -0.7).normalize(), 2.8),
                (Vec3::new(0.8, -0.4, 0.4).normalize(), 1.0),
                (Vec3::new(0.0, 0.7, -0.3).normalize(), 0.35),
            ]
            .into_iter()
            .enumerate()
            {
                globals.lights[index] = LightUniform {
                    position_kind: [0.0; 4],
                    direction_range: direction.extend(0.0).to_array(),
                    color_intensity: [1.0, 1.0, 1.0, intensity],
                    spot: [0.0; 4],
                };
            }
            globals.settings[1] = 3.0;
        } else {
            for (index, evaluated) in self.frame_lights.iter().enumerate() {
                let light = self
                    .lights
                    .get(evaluated.light)
                    .ok_or("An evaluated light has no definition.")?;
                let (kind, spot) = match light.kind {
                    scene::LightKind::Directional => (0.0, [0.0; 4]),
                    scene::LightKind::Point => (1.0, [0.0; 4]),
                    scene::LightKind::Spot { inner, outer } => {
                        (2.0, [inner.cos(), outer.cos(), 0.0, 0.0])
                    }
                };
                globals.lights[index] = LightUniform {
                    position_kind: (model
                        .transform_point3(self.transform.point(evaluated.position_cm))
                        * units)
                        .extend(kind)
                        .to_array(),
                    direction_range: model
                        .transform_vector3(evaluated.direction.as_vec3())
                        .normalize_or_zero()
                        .extend(light.range_cm.unwrap_or(0.0) * 0.01)
                        .to_array(),
                    color_intensity: [
                        light.color[0],
                        light.color[1],
                        light.color[2],
                        light.intensity,
                    ],
                    spot,
                };
            }
            globals.settings[1] = self.frame_lights.len() as f32;
        }
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&globals));
        Ok(())
    }

    pub(super) fn draw_info(&self) -> impl Iterator<Item = (usize, bool, Vec3, Topology)> + '_ {
        self.draws.iter().enumerate().map(|(index, draw)| {
            (
                index,
                self.materials[draw.material].blend,
                draw.center,
                draw.topology,
            )
        })
    }

    /// Issue one primitive into the caller's shared scene pass. All immutable
    /// material bindings and geometry stay owned by this instance's resources.
    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, index: usize, shading: ShadingMode) {
        let draw = &self.draws[index];
        if draw.indices.is_empty() {
            return;
        }
        let material = &self.materials[draw.material];
        pass.set_bind_group(0, &self.globals_group, &[]);
        pass.set_bind_group(1, &material.group, &[]);
        if let Some(environment) = &self.environment {
            pass.set_bind_group(2, &environment.group, &[]);
        }
        pass.set_vertex_buffer(0, draw.vertex_buffer.slice(..));
        if shading == ShadingMode::Wireframe {
            let key = if draw.topology == Topology::Points {
                4
            } else {
                3
            };
            pass.set_pipeline(&self.pipelines[&(key, false, true, false)]);
            let (indices, count) = draw
                .wire
                .as_ref()
                .map(|(buffer, count)| (buffer, *count))
                .unwrap_or((&draw.index_buffer, draw.indices.len() as u32));
            pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..count, 0, 0..1);
        } else if shading == ShadingMode::Solid {
            let key = pipeline_key(draw.topology, false, true, draw.mirrored);
            pass.set_pipeline(&self.solid_pipelines[&(key.0, key.3)]);
            pass.set_index_buffer(draw.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..draw.indices.len() as u32, 0, 0..1);
        } else {
            pass.set_pipeline(
                &self.pipelines[&pipeline_key(
                    draw.topology,
                    material.blend,
                    material.double_sided,
                    draw.mirrored,
                )],
            );
            pass.set_index_buffer(draw.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..draw.indices.len() as u32, 0, 0..1);
        }
    }

    /// Share pipelines and immutable materials/textures when placing or
    /// duplicating an asset. Uniform and geometry state remain instance-owned.
    pub(super) fn fork_instance(&self, device: &wgpu::Device, reuse_geometry: bool) -> Self {
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("n3 asset instance frame uniforms"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("n3 asset instance globals"),
            layout: &self.globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        Self {
            #[cfg(test)]
            view: self.view.clone(),
            #[cfg(test)]
            width: self.width,
            #[cfg(test)]
            height: self.height,
            #[cfg(test)]
            color: self.color.clone(),
            #[cfg(test)]
            color_view: self.color_view.clone(),
            #[cfg(test)]
            multisample: self.multisample.clone(),
            #[cfg(test)]
            depth: self.depth.clone(),
            uniform,
            globals_group,
            globals_layout: self.globals_layout.clone(),
            material_layout: self.material_layout.clone(),
            environment_layout: self.environment_layout.clone(),
            environment: self.environment.clone(),
            pipelines: self.pipelines.clone(),
            solid_pipelines: self.solid_pipelines.clone(),
            materials: self.materials.clone(),
            lights: self.lights.clone(),
            camera_definitions: self.camera_definitions.clone(),
            frame_lights: self.frame_lights.clone(),
            frame_cameras: self.frame_cameras.clone(),
            draws: if reuse_geometry {
                self.draws.clone()
            } else {
                Vec::new()
            },
            transform: self.transform,
            framing_initialized: self.framing_initialized,
            #[cfg(test)]
            wireframe: self.wireframe,
            #[cfg(test)]
            uploads: if reuse_geometry {
                self.uploads
            } else {
                UploadCounts::default()
            },
        }
    }
    #[cfg(test)]
    pub(super) fn cache_identity(&self) -> (wgpu::BindGroup, wgpu::Buffer, usize) {
        (
            self.materials[0].group.clone(),
            self.draws[0].vertex_buffer.clone(),
            self.uploads.vertices,
        )
    }

    pub(super) fn set_display_frame(&mut self, frame: &crate::document::DisplayFrame) {
        self.transform = ViewTransform {
            center_cm: DVec3::from_array(frame.center),
            scale: frame.scale,
        };
        self.framing_initialized = true;
    }

    #[cfg(test)]
    fn imported_camera(&self, index: usize) -> Result<(Mat4, Vec3, [f32; 4])> {
        let camera = self
            .frame_cameras
            .get(index)
            .ok_or("The selected imported camera is unavailable.")?;
        let definition = self
            .camera_definitions
            .get(camera.camera)
            .ok_or("An imported camera has no definition.")?;
        let invalid = || {
            format!(
                "Imported camera on node {} has an unusable transform or projection.",
                camera.node
            )
        };
        let eye = self.transform.point(camera.world.w_axis.truncate());
        let x = camera
            .world
            .x_axis
            .truncate()
            .try_normalize()
            .ok_or_else(invalid)?;
        let y = camera
            .world
            .y_axis
            .truncate()
            .try_normalize()
            .ok_or_else(invalid)?;
        let z = camera
            .world
            .z_axis
            .truncate()
            .try_normalize()
            .ok_or_else(invalid)?;
        if !camera.world.is_finite()
            || !finite_values(&eye.to_array())
            || x.cross(y).dot(z).abs() < 1e-8
        {
            return Err(invalid());
        }
        // Camera scaling must not change its projection. Gram-Schmidt also
        // removes shear induced by nonuniform ancestor scale + child rotation,
        // retaining the evaluated -Z view direction and Y up as the frame.
        let x = y.cross(z).try_normalize().ok_or_else(invalid)?;
        let y = z.cross(x).try_normalize().ok_or_else(invalid)?;
        let basis = DMat4::from_cols(
            x.extend(0.0),
            y.extend(0.0),
            z.extend(0.0),
            eye.as_dvec3().extend(1.0),
        )
        .as_mat4();
        let aspect = self.width as f32 / self.height as f32;
        let scale = self.transform.scale;
        let length = |centimeters: f32| (f64::from(centimeters) * scale) as f32;
        let (projection, frame_aspect) = match definition.projection {
            scene::Projection::Perspective {
                yfov,
                aspect: authored_aspect,
                near_cm,
                far_cm,
            } => {
                let near = length(near_cm);
                let far = far_cm.map(length);
                let aspect = authored_aspect.unwrap_or(aspect);
                if !yfov.is_finite()
                    || yfov <= 0.0
                    || yfov >= std::f32::consts::PI
                    || !aspect.is_finite()
                    || aspect <= 0.0
                    || !near.is_finite()
                    || near <= 0.0
                    || far.is_some_and(|far| !far.is_finite() || far <= near)
                {
                    return Err(invalid());
                }
                (
                    match far {
                        Some(far) => projection::perspective(yfov, aspect, near, far),
                        None => projection::perspective_infinite(yfov, aspect, near),
                    },
                    aspect,
                )
            }
            scene::Projection::Orthographic {
                xmag_cm,
                ymag_cm,
                near_cm,
                far_cm,
            } => {
                let xmag = length(xmag_cm);
                let ymag = length(ymag_cm);
                let near = length(near_cm);
                let far = length(far_cm);
                if ![xmag, ymag, near, far].iter().all(|v| v.is_finite())
                    || xmag <= 0.0
                    || ymag <= 0.0
                    || near < 0.0
                    || far <= near
                {
                    return Err(invalid());
                }
                (
                    projection::orthographic(-xmag, xmag, -ymag, ymag, near, far),
                    xmag / ymag,
                )
            }
        };
        let matrix = projection * basis.inverse();
        if !matrix.is_finite() || !frame_aspect.is_finite() || frame_aspect <= 0.0 {
            return Err(invalid());
        }
        let viewport = camera_viewport(self.width, self.height, frame_aspect);
        if viewport[2] <= 0.0 || viewport[3] <= 0.0 {
            return Err(invalid());
        }
        Ok((matrix, eye, viewport))
    }
}

#[cfg(test)]
fn camera_viewport(width: u32, height: u32, aspect: f32) -> [f32; 4] {
    let (width, height) = (width as f32, height as f32);
    if width / height > aspect {
        let fitted = height * aspect;
        [(width - fitted) * 0.5, 0.0, fitted, height]
    } else {
        let fitted = width / aspect;
        [0.0, (height - fitted) * 0.5, width, fitted]
    }
}

fn draw_budget(
    vertices: usize,
    indices: usize,
    topology: Topology,
    device_limit: u64,
) -> Result<u64> {
    let size = |count: usize, stride: u64| {
        (count as u64)
            .checked_mul(stride)
            .filter(|size| *size <= device_limit)
            .ok_or_else(|| "An imported primitive exceeds the GPU buffer limit.".to_owned())
    };
    let vertices = size(vertices, size_of::<Vertex>() as u64)?;
    let index_bytes = size(indices, 4)?;
    let wire_bytes = if topology == Topology::Triangles {
        size(indices, 8)?
    } else {
        0
    };
    if indices > u32::MAX as usize
        || (topology == Topology::Triangles && indices > u32::MAX as usize / 2)
    {
        return Err("An imported primitive exceeds the GPU index limit.".into());
    }
    let primitive_size = match topology {
        Topology::Points => 1,
        Topology::Lines => 2,
        Topology::Triangles => 3,
    };
    if !indices.is_multiple_of(primitive_size) {
        return Err("An imported primitive has an incomplete index group.".into());
    }
    vertices
        .checked_add(index_bytes)
        .and_then(|bytes| bytes.checked_add(wire_bytes))
        .ok_or_else(|| "Imported GPU geometry size overflow.".into())
}

fn pipeline_key(
    topology: Topology,
    blend: bool,
    double_sided: bool,
    mirrored: bool,
) -> (u8, bool, bool, bool) {
    match topology {
        Topology::Triangles => (0, blend, double_sided, mirrored),
        Topology::Lines => (1, blend, true, false),
        Topology::Points => (2, blend, true, false),
    }
}
fn wire_buffer(
    device: &wgpu::Device,
    topology: Topology,
    indices: &[u32],
) -> Option<(wgpu::Buffer, u32)> {
    if topology != Topology::Triangles {
        return None;
    }
    // Packed edges keep temporary memory within the same conservative byte
    // budget checked before conversion; tree-node overhead would not be bounded
    // by that estimate. Sorting gives stable deduplication without another copy.
    let mut edges = Vec::with_capacity(indices.len());
    for triangle in indices.as_chunks::<3>().0 {
        for [a, b] in [
            [triangle[0], triangle[1]],
            [triangle[1], triangle[2]],
            [triangle[2], triangle[0]],
        ] {
            edges.push([a.min(b), a.max(b)]);
        }
    }
    edges.sort_unstable();
    edges.dedup();
    Some((
        buffer(
            device,
            "n3 imported wire indices",
            bytemuck::cast_slice(&edges),
            wgpu::BufferUsages::INDEX,
        ),
        edges.len() as u32 * 2,
    ))
}
fn vertex_center(vertices: &[Vertex]) -> Vec3 {
    let mut low = Vec3::splat(f32::INFINITY);
    let mut high = Vec3::splat(f32::NEG_INFINITY);
    for vertex in vertices {
        let position = Vec3::from_array(vertex.position);
        low = low.min(position);
        high = high.max(position);
    }
    if vertices.is_empty() {
        Vec3::ZERO
    } else {
        low + (high - low) * 0.5
    }
}
fn buffer(
    device: &wgpu::Device,
    label: &str,
    contents: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: if contents.is_empty() {
            &[0; 4]
        } else {
            contents
        },
        usage: usage | wgpu::BufferUsages::COPY_DST,
    })
}
#[cfg(test)]
fn attachments(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (
    wgpu::Texture,
    wgpu::TextureView,
    wgpu::TextureView,
    wgpu::TextureView,
    wgpu::TextureView,
) {
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("n3 imported scene SRGB target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: COLOR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
    });
    let view = color.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba8Unorm),
        ..Default::default()
    });
    let color_view = color.create_view(&Default::default());
    let attachment = |label, format| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: SAMPLES,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default())
    };
    (
        color,
        view,
        color_view,
        attachment("n3 imported scene MSAA", COLOR_FORMAT),
        attachment("n3 imported scene depth", DEPTH_FORMAT),
    )
}

#[cfg(test)]
#[path = "scene_renderer_tests.rs"]
mod tests;
