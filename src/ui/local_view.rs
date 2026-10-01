//! Temporary per-viewport isolation. Document geometry and edit history remain
//! authoritative; this state only controls visibility and a reversible camera.

use crate::{
    camera::Camera, editor::Editor, navigation_state::ViewNavigation, orientation::display_rotation,
};
use glam::{DVec3, Vec3};
use std::collections::BTreeSet;

pub(super) struct LocalView {
    pub(super) members: BTreeSet<u64>,
    pub(super) camera: Camera,
    pub(super) navigation: ViewNavigation,
}

impl LocalView {
    pub(super) fn new(
        editor: &Editor,
        camera: &Camera,
        navigation: &ViewNavigation,
    ) -> Option<Self> {
        let members = editor.selected_objects.clone();
        if members.is_empty() {
            return None;
        }
        // Remember the destination of an in-flight view change, not a transient
        // zoom. In particular, toggling / again during the return animation
        // must not progressively replace the original full-scene framing.
        let mut camera = camera.clone();
        camera.finish_transition();
        Some(Self {
            members,
            camera,
            navigation: navigation.clone(),
        })
    }

    /// The editor explicitly admits inserted or duplicated objects. Retain
    /// membership through undo so redo restores them, while undo of a deletion
    /// made before entering Local View cannot reveal an unrelated object.
    pub(super) fn reconcile(&mut self, editor: &Editor) -> BTreeSet<u64> {
        let current: BTreeSet<_> = editor
            .document
            .objects
            .iter()
            .map(|object| object.id)
            .collect();
        if let Some(allowed) = editor.visible_objects() {
            self.members.extend(allowed);
        }
        self.members.intersection(&current).copied().collect()
    }
}

/// Fit whole objects, including while the user is editing only a few vertices.
/// Coordinates match the display mesh and therefore the camera's view space.
pub(super) fn object_points(
    editor: &Editor,
    objects: &BTreeSet<u64>,
    z_up: bool,
) -> Result<Vec<Vec3>, String> {
    let rotation = display_rotation(z_up);
    let mut points = Vec::new();
    for object in &editor.document.objects {
        if !objects.contains(&object.id) {
            continue;
        }
        let world_points = match &object.geometry {
            crate::document::Geometry::Asset(asset) => match editor.asset_frames().get(asset) {
                Some(frame) => {
                    let points =
                        crate::model::asset_geometry::AssetGeometry::new(object, frame)?.points;
                    if points.is_empty() {
                        vec![DVec3::from_array(object.transform.translation)]
                    } else {
                        points
                    }
                }
                None => vec![DVec3::from_array(object.transform.translation)],
            },
            _ => {
                let geometry = editor.document.eval_object(object.id)?;
                let transform = object.transform.matrix();
                geometry
                    .vertices
                    .iter()
                    .map(|vertex| transform.transform_point3(DVec3::from_array(vertex.position)))
                    .collect()
            }
        };
        for world in world_points {
            let display = editor.frame.world_to_display(world).as_vec3();
            let point = rotation.transform_point3(display);
            if !point.is_finite() {
                return Err("Local View geometry is outside the display coordinate range.".into());
            }
            points.push(point);
        }
    }
    Ok(points)
}
