//! Imported asset instances draw into the workbench attachments. Source resources
//! are immutable and shared; placement, evaluated poses and uniforms are per object.
use super::scene_renderer::PbrSceneRenderer;
use super::shading::ShadingMode;
use crate::{
    camera::Camera,
    document::{DisplayFrame, Transform},
    scene::{EvaluatedScene, SceneAsset},
};
use glam::{DMat3, DVec3, Mat4};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

type Result<T> = std::result::Result<T, String>;
#[derive(Clone)]
pub(crate) struct PlacedScene {
    pub object: u64,
    pub asset: Arc<SceneAsset>,
    pub frame: Arc<EvaluatedScene>,
    pub transform: Transform,
    pub exposure: f32,
}
struct Instance {
    source: PlacedScene,
    display: DisplayFrame,
    renderer: PbrSceneRenderer,
}
#[derive(Default)]
pub(super) struct PlacedScenes {
    entries: BTreeMap<u64, Instance>,
}
impl PlacedScenes {
    #[cfg(test)]
    pub fn cache_identity(&self, object: u64) -> (wgpu::BindGroup, wgpu::Buffer, usize) {
        self.entries[&object].renderer.cache_identity()
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    pub fn contains(&self, object: u64) -> bool {
        self.entries.contains_key(&object)
    }
    pub fn ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.entries.keys().copied()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sources: &[PlacedScene],
        display: &DisplayFrame,
    ) -> Result<()> {
        let mut ids = BTreeSet::new();
        let mut pending = Vec::new();
        let mut uniform_updates = Vec::new();
        for source in sources {
            if !source.exposure.is_finite() {
                return Err("Asset exposure must be finite.".into());
            }
            if !ids.insert(source.object) {
                return Err("An asset object is submitted more than once.".into());
            }
            let old = self.entries.get(&source.object);
            if old.is_some_and(|old| {
                Arc::ptr_eq(&old.source.asset, &source.asset)
                    && Arc::ptr_eq(&old.source.frame, &source.frame)
                    && old.source.transform == source.transform
                    && old.display == *display
            }) {
                // Appearance-only changes retain pose and placement buffers.
                uniform_updates.push(source.clone());
                continue;
            }
            let same = old.filter(|old| Arc::ptr_eq(&old.source.asset, &source.asset));
            let shared = same
                .or_else(|| {
                    self.entries
                        .values()
                        .find(|entry| Arc::ptr_eq(&entry.source.asset, &source.asset))
                })
                .or_else(|| {
                    pending
                        .iter()
                        .find_map(|(entry, _, _): &(Instance, EvaluatedScene, _)| {
                            Arc::ptr_eq(&entry.source.asset, &source.asset).then_some(entry)
                        })
                });
            let mut renderer = if let Some(shared) = shared {
                shared.renderer.fork_instance(device, same.is_some())
            } else if let Some(template) = self.entries.values().next() {
                let mut renderer = template.renderer.fork_instance(device, false);
                renderer.set_asset(device, queue, &source.asset)?;
                renderer
            } else {
                let mut renderer = PbrSceneRenderer::new(device);
                renderer.set_asset(device, queue, &source.asset)?;
                renderer
            };
            renderer.set_display_frame(display);
            let frame = place_frame(&source.frame, &source.transform)?;
            let prepared = renderer.prepare_frame(device, &frame)?;
            pending.push((
                Instance {
                    source: source.clone(),
                    display: *display,
                    renderer,
                },
                frame,
                prepared,
            ));
        }
        // Every fallible operation precedes writes to reused geometry buffers or
        // publication. A rejected import/pose leaves the previous viewport intact.
        for (mut instance, frame, prepared) in pending {
            instance
                .renderer
                .apply_frame(device, queue, &frame, prepared);
            self.entries.insert(instance.source.object, instance);
        }
        for source in uniform_updates {
            let object = source.object;
            self.entries.get_mut(&object).unwrap().source = source;
        }
        self.entries.retain(|object, _| ids.contains(object));
        Ok(())
    }
    pub fn prepare_view(
        &self,
        queue: &wgpu::Queue,
        camera: &Camera,
        size: [u32; 2],
        model: Mat4,
        background: egui::Color32,
        visible: Option<&BTreeSet<u64>>,
    ) -> Result<()> {
        let view = camera.view_projection(size[0] as f32 / size[1] as f32);
        for (id, entry) in &self.entries {
            if visible.is_none_or(|v| v.contains(id)) {
                entry.renderer.write_uniforms(
                    queue,
                    view,
                    camera.eye(),
                    model,
                    background,
                    entry.source.exposure,
                )?;
            }
        }
        Ok(())
    }
    pub fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        view_model: Mat4,
        visible: Option<&BTreeSet<u64>>,
        transparent: bool,
        shading: ShadingMode,
        xray: bool,
    ) {
        let wireframe = shading == ShadingMode::Wireframe;
        let material_preview = shading == ShadingMode::MaterialPreview;
        let mut draws = Vec::new();
        for (object, entry) in &self.entries {
            if !visible.is_none_or(|v| v.contains(object)) {
                continue;
            }
            for (index, blend, center, topology) in entry.renderer.draw_info() {
                // Triangle boundaries and lines share the workbench wire/edge
                // overlays. Point primitives have no edge proxy.
                if (wireframe || xray) && topology != crate::scene::Topology::Points {
                    continue;
                }
                if !wireframe && !xray && (blend && material_preview) != transparent {
                    continue;
                }
                let clip = view_model * center.extend(1.0);
                draws.push((entry, index, clip.z / clip.w));
            }
        }
        if transparent && !wireframe {
            // Sorting spans all asset instances, not just each source file.
            draws.sort_by(|a, b| b.2.total_cmp(&a.2));
        }
        for (entry, index, _) in draws {
            entry.renderer.draw(
                pass,
                index,
                if wireframe || xray {
                    ShadingMode::Wireframe
                } else {
                    shading
                },
            );
        }
    }
}

fn place_frame(source: &EvaluatedScene, placement: &Transform) -> Result<EvaluatedScene> {
    let matrix = placement.matrix();
    if !matrix.is_finite() {
        return Err("Asset placement must be finite.".into());
    }
    let x = matrix.x_axis.truncate();
    let y = matrix.y_axis.truncate();
    let z = matrix.z_axis.truncate();
    let reflected = matrix.determinant() < 0.0;
    let normal_matrix =
        DMat3::from_cols(y.cross(z), z.cross(x), x.cross(y)) * if reflected { -1.0 } else { 1.0 };
    let mut frame = source.clone();
    frame.bounds = None;
    for draw in &mut frame.draws {
        draw.mirrored ^= reflected;
        for vertex in &mut draw.vertices {
            let position = matrix.transform_point3(DVec3::from_array(vertex.position));
            let normal = (normal_matrix * DVec3::from_array(vertex.normal.map(f64::from)))
                .normalize_or(DVec3::Z);
            let tangent = matrix.transform_vector3(DVec3::new(
                vertex.tangent[0] as f64,
                vertex.tangent[1] as f64,
                vertex.tangent[2] as f64,
            ));
            let tangent = (tangent - normal * normal.dot(tangent)).normalize_or_zero();
            if !position.is_finite() {
                return Err("Asset placement exceeds the finite coordinate range.".into());
            }
            vertex.position = position.to_array();
            vertex.normal = normal.as_vec3().to_array();
            vertex.tangent = [
                tangent.x as f32,
                tangent.y as f32,
                tangent.z as f32,
                vertex.tangent[3] * if reflected { -1.0 } else { 1.0 },
            ];
        }
    }
    for light in &mut frame.lights {
        light.position_cm = matrix.transform_point3(light.position_cm);
        light.direction = matrix
            .transform_vector3(light.direction)
            .normalize_or(DVec3::NEG_Z);
    }
    for camera in &mut frame.cameras {
        camera.world = matrix * camera.world;
    }
    for world in &mut frame.node_world {
        *world = matrix * *world;
    }
    Ok(frame)
}
