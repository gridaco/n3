//! Format-neutral runtime scenes, separate from the editable document model.
//! Every spatial length is a canonical centimeter. Render topology, index handles,
//! and interpolation curves describe immutable evaluated content, not authoring IDs.
pub(crate) mod budgets;
pub(crate) mod display_limits;
mod evaluate;
pub(crate) mod validate;

use std::sync::{Arc, Mutex};

use glam::{DMat4, DQuat, DVec3};

pub(crate) type Result<T> = std::result::Result<T, String>;
pub(crate) const MAX_ACTIVE_LIGHTS: usize = 32;
type StaticSceneCache = Arc<Mutex<Option<(usize, Arc<EvaluatedScene>)>>>;

#[derive(Clone, Debug)]
pub(crate) struct SceneAsset {
    data: SceneData,
    // Bound cached evaluation memory to the most recently viewed rest scene.
    static_scene: StaticSceneCache,
}
impl std::ops::Deref for SceneAsset {
    type Target = SceneData;
    fn deref(&self) -> &SceneData {
        &self.data
    }
}
impl SceneAsset {
    /// Adapters and internal scene producers share one validation boundary.
    /// Immutable publication keeps cached evaluation valid without revision tricks.
    pub(crate) fn new(data: SceneData) -> Result<Self> {
        validate::data(&data)?;
        let asset = Self {
            data,
            static_scene: Arc::new(Mutex::new(None)),
        };
        asset.evaluate(asset.default_scene, None)?;
        Ok(asset)
    }

    /// Retained rest-pose payload for host cache budgets. Inspect the current
    /// cache without evaluating, replacing it, or advancing any animation.
    /// Shared index buffers are charged conservatively per draw.
    pub(crate) fn cached_frame_payload_bytes(&self) -> Result<usize> {
        let cached = self
            .static_scene
            .lock()
            .map_err(|_| "Scene cache lock was poisoned.")?;
        let Some((_, frame)) = &*cached else {
            return Ok(0);
        };
        let mut bytes = 0usize;
        let mut add = |count: usize, size: usize| {
            bytes = bytes.saturating_add(count.saturating_mul(size));
        };
        add(frame.draws.capacity(), std::mem::size_of::<EvaluatedDraw>());
        add(
            frame.cameras.capacity(),
            std::mem::size_of::<EvaluatedCamera>(),
        );
        add(
            frame.lights.capacity(),
            std::mem::size_of::<EvaluatedLight>(),
        );
        add(frame.node_world.capacity(), std::mem::size_of::<DMat4>());
        for draw in &frame.draws {
            add(draw.vertices.capacity(), std::mem::size_of::<SceneVertex>());
            add(draw.indices.len(), std::mem::size_of::<u32>());
        }
        Ok(bytes)
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SceneData {
    pub name: String,
    pub scenes: Vec<SceneDefinition>,
    pub default_scene: usize,
    pub nodes: Vec<Node>,
    pub materials: Vec<Material>,
    pub images: Vec<Image>,
    pub textures: Vec<Texture>,
    pub cameras: Vec<Camera>,
    pub lights: Vec<Light>,
    pub animations: Vec<Animation>,
    pub warnings: Vec<String>,
    pub meshes: Vec<Mesh>,
    pub skins: Vec<Skin>,
}

#[derive(Clone, Debug)]
pub(crate) struct SceneDefinition {
    pub name: String,
    pub roots: Vec<usize>,
}

#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub name: String,
    pub children: Vec<usize>,
    pub mesh: Option<usize>,
    pub skin: Option<usize>,
    pub camera: Option<usize>,
    pub light: Option<usize>,
    pub weights: Option<Vec<f32>>,
    pub transform: Transform,
}

#[derive(Clone, Debug)]
pub(crate) enum Transform {
    /// Affine matrix with translation in centimeters.
    Matrix(DMat4),
    Trs {
        /// Canonical centimeters.
        translation: DVec3,
        rotation: DQuat,
        scale: DVec3,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Topology {
    Points,
    Lines,
    Triangles,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SceneVertex {
    /// Canonical centimeters: local in a primitive, world in an evaluated draw.
    /// Normalize only at the f32 GPU boundary.
    pub position: [f64; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    /// Normalized texture coordinates with top-left origin; wrapping may use
    /// coordinates outside [0, 1]. Both UV sets share this convention.
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Clone, Debug)]
pub(crate) struct EvaluatedDraw {
    pub node: usize,
    pub mesh: usize,
    pub primitive: usize,
    pub material: usize,
    pub topology: Topology,
    pub vertices: Vec<SceneVertex>,
    pub indices: Arc<[u32]>,
    /// A reflection reverses front-face winding even after world evaluation.
    pub mirrored: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct EvaluatedScene {
    pub draws: Vec<EvaluatedDraw>,
    pub bounds: Option<Bounds>,
    pub cameras: Vec<EvaluatedCamera>,
    pub lights: Vec<EvaluatedLight>,
    /// World matrices in centimeters, including nodes without geometry.
    pub node_world: Vec<DMat4>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Bounds {
    pub min: DVec3,
    pub max: DVec3,
}

#[derive(Clone, Debug)]
/// RGBA8 pixels with top-left origin and top-to-bottom row order.
/// Slot semantics determine the color transfer function.
pub(crate) struct Image {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wrap {
    Clamp,
    Repeat,
    Mirror,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Filter {
    Nearest,
    Linear,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Sampler {
    pub wrap_s: Wrap,
    pub wrap_t: Wrap,
    pub mag: Filter,
    pub min: Filter,
    /// None explicitly disables mipmaps; Some supplies their interpolation.
    pub mipmap: Option<Filter>,
}
#[derive(Clone, Debug)]
pub(crate) struct Texture {
    pub image: usize,
    pub sampler: Sampler,
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// UV transform: scale, then rotation in radians, then offset.
pub(crate) struct TextureTransform {
    pub offset: [f32; 2],
    pub scale: [f32; 2],
    pub rotation: f32,
}
impl Default for TextureTransform {
    fn default() -> Self {
        Self {
            offset: [0.; 2],
            scale: [1.; 2],
            rotation: 0.,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct TextureInfo {
    pub texture: usize,
    pub tex_coord: u32,
    pub transform: TextureTransform,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AlphaMode {
    Opaque,
    Mask,
    Blend,
}

#[derive(Clone, Debug)]
/// Metallic/roughness PBR presentation contract. Factors and vertex colors are
/// linear; base-color/emissive image RGB uses sRGB, other slots use linear bytes.
/// Metallic/roughness packing uses G=roughness/B=metallic; occlusion uses R.
/// Normal RGB maps to [-1,1] in a +Y tangent basis, with tangent.w handedness.
pub(crate) struct Material {
    pub name: String,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
    pub base_color_texture: Option<TextureInfo>,
    pub metallic_roughness_texture: Option<TextureInfo>,
    pub normal_texture: Option<TextureInfo>,
    pub normal_scale: f32,
    pub occlusion_texture: Option<TextureInfo>,
    pub occlusion_strength: f32,
    pub emissive_texture: Option<TextureInfo>,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub double_sided: bool,
    pub unlit: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Camera {
    pub name: String,
    pub projection: Projection,
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum Projection {
    Perspective {
        yfov: f32,
        aspect: Option<f32>,
        near_cm: f32,
        far_cm: Option<f32>,
    },
    Orthographic {
        xmag_cm: f32,
        ymag_cm: f32,
        near_cm: f32,
        far_cm: f32,
    },
}
#[derive(Clone, Debug)]
pub(crate) struct EvaluatedCamera {
    pub node: usize,
    pub camera: usize,
    pub world: DMat4,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LightKind {
    Directional,
    Point,
    Spot { inner: f32, outer: f32 },
}
#[derive(Clone, Debug)]
pub(crate) struct Light {
    pub name: String,
    pub kind: LightKind,
    pub color: [f32; 3],
    /// Candela (point/spot) or lux (directional); render attenuation in meters.
    pub intensity: f32,
    pub range_cm: Option<f32>,
}
#[derive(Clone, Debug)]
pub(crate) struct EvaluatedLight {
    pub node: usize,
    pub light: usize,
    pub position_cm: DVec3,
    pub direction: DVec3,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AnimationSample {
    pub clip: usize,
    pub time: f32,
}
#[derive(Clone, Debug)]
pub(crate) struct Animation {
    pub name: String,
    pub start: f32,
    pub duration: f32,
    pub channels: Vec<Channel>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Interpolation {
    Step,
    Linear,
    CubicHermite,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Property {
    Translation,
    Rotation,
    Scale,
    Weights,
}
#[derive(Clone, Debug)]
pub(crate) struct Channel {
    pub node: usize,
    pub property: Property,
    pub interpolation: Interpolation,
    pub times: Vec<f32>,
    /// One component vector per key, independent of interpolation storage.
    pub values: Vec<f64>,
    /// Derivatives per second for CubicHermite; empty for Step/Linear.
    pub in_tangents: Vec<f64>,
    pub out_tangents: Vec<f64>,
    pub components: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct Mesh {
    pub name: String,
    pub primitives: Vec<Primitive>,
    pub weights: Vec<f32>,
}
#[derive(Clone, Debug)]
pub(crate) struct Primitive {
    pub topology: Topology,
    pub material: usize,
    pub vertices: Vec<SceneVertex>,
    pub indices: Arc<[u32]>,
    pub morphs: Vec<Morph>,
    pub influences: Vec<Vec<([u16; 4], [f32; 4])>>,
    pub flat_normals: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct Morph {
    pub positions: Vec<[f64; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 3]>,
}
#[derive(Clone, Debug)]
pub(crate) struct Skin {
    pub name: String,
    pub joints: Vec<usize>,
    pub inverse_bind: Vec<DMat4>,
}

#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
