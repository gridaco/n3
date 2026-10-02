//! Editable document state and the viewport controller shared by native and recorded input.

mod derived_edit;
pub(crate) mod edit_history;
mod geometry_edit;
mod make_face;
pub(crate) mod numeric_transform;
pub(crate) mod snapping;
mod transform_gizmo;

use geometry_edit::GeometryEdit;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::render::transform_gizmo::Endpoint;
use egui::{Color32, PointerButton, Pos2, Rect, Stroke};
use glam::{DMat4, DQuat, DVec3, Vec3};

use crate::{
    axis_gizmo,
    camera::Camera,
    controls::{self, Control},
    document::{AssetFrames, DisplayFrame, Document, Geometry, PrimitiveKind},
    edit_feedback,
    edit_history::EditHistory,
    numeric_transform::ParsedNumber,
    orientation::display_rotation,
    pointer_policy::crossed_drag_threshold,
    snapping::{SnapSettings, StepPolicy, TranslationSource},
    theme,
    ui::marquee,
};

const HISTORY_LIMIT: usize = 64;

#[cfg(test)]
#[path = "editor_snapping_tests.rs"]
mod snapping_tests;

#[cfg(test)]
#[path = "editor_numeric_tests.rs"]
mod numeric_tests;

#[cfg(test)]
mod primitive_edit_tests;

#[cfg(test)]
mod xray_tests;

#[cfg(test)]
mod asset_tests;

// Keep picking forgiving even when dense meshes use compact painted markers.
const PICK_RADIUS: f32 = 8.0;
// Loose edges have no surface target; use a fixed screen-space tolerance.
const EDGE_PICK_RADIUS: f32 = 6.0;
const VERTEX_RADIUS: f32 = 1.75;
const SELECTED_VERTEX_RADIUS: f32 = 2.5;
const HANDLE_LENGTH: f64 = 78.0;
// Camera matrices originate in f32. Keep a subpixel tolerance at silhouette
// corners, including tiny residual rotations in mathematically cardinal views.
const RAY_BARYCENTRIC_TOLERANCE: f64 = 1e-6;
const COLORS: [Color32; 3] = [
    Color32::from_rgb(235, 111, 123),
    Color32::from_rgb(141, 207, 116),
    Color32::from_rgb(112, 166, 242),
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Tool {
    View,
    #[default]
    Move,
    Rotate,
    Scale,
}

/// Interaction policy, independent of document data and undo history. Each
/// marquee captures it on press so a setting change cannot alter a held drag.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BoxSelectionPolicy {
    pub timing: SelectionTiming,
    pub hit: BoxHitPolicy,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SelectionTiming {
    #[default]
    OnRelease,
    Live,
}

/// Surface occlusion is independent of shading, object isolation, clipping,
/// and the document. X-ray changes which projected targets are eligible.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum SelectionDepth {
    #[default]
    VisibleOnly,
    Through,
}

impl SelectionDepth {
    fn admits(self, vertex: &ProjectedVertex) -> bool {
        self == Self::Through || !vertex.occluded
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BoxHitPolicy {
    #[default]
    Intersects,
    Contains,
}

impl BoxHitPolicy {
    fn matches(self, marquee: Rect, bounds: Rect) -> bool {
        // A zero-area vertex bound is still selectable. The marquee itself
        // must cover a nonempty viewport region.
        marquee.is_positive()
            && if self == Self::Contains {
                marquee.contains_rect(bounds)
            } else {
                marquee.intersects(bounds)
            }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscapeOutcome {
    InteractionCancelled,
    AxisUnlocked,
    VerticesDeselected,
    EditModeLeft,
    ObjectDeselected,
    NothingToDo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfirmOutcome {
    InteractionFinished,
    EditModeEntered,
    EditModeLeft,
    NothingToDo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HandleKind {
    Axis(usize),
    Plane(usize, usize),
    Uniform,
}

impl HandleKind {
    fn control(self) -> Control {
        match self {
            Self::Axis(0) => Control::TransformX,
            Self::Axis(1) => Control::TransformY,
            Self::Axis(_) => Control::TransformZ,
            Self::Plane(0, 1) => Control::TransformXY,
            Self::Plane(0, 2) => Control::TransformXZ,
            Self::Plane(_, _) => Control::TransformYZ,
            Self::Uniform => Control::TransformUniform,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Snapshot {
    document: Document,
    selected_object: Option<u64>,
    selected_objects: BTreeSet<u64>,
    selected_vertices: BTreeSet<u64>,
    edit_mode: bool,
    geometry_edit: Option<Arc<GeometryEdit>>,
}

enum Gesture {
    Marquee {
        start: Pos2,
        current: Pos2,
        additive: bool,
        before: BTreeSet<u64>,
        pending: BTreeSet<u64>,
        policy: BoxSelectionPolicy,
        dragged: bool,
    },
    ObjectMarquee {
        start: Pos2,
        current: Pos2,
        additive: bool,
        before: BTreeSet<u64>,
        before_active: Option<u64>,
        pending: BTreeSet<u64>,
        policy: BoxSelectionPolicy,
        projection: Box<Projection>,
        dragged: bool,
    },
    Transform(Box<TransformDrag>),
}

struct TransformDrag {
    before: Snapshot,
    snapping: SnapSettings,
    projection: Projection,
    pivot: DVec3,
    axis: DVec3,
    plane_normal: Option<DVec3>,
    start: DVec3,
    length: f64,
    kind: HandleKind,
    tool: Tool,
    locked_transform: bool,
    session_transform: bool,
    pointer_start: Pos2,
    dragged: bool,
    // A camera-parallel locked axis uses screen-up for its positive direction.
    screen_axis: Option<(Pos2, f64)>,
    // An armed rotation accepts a drag anywhere, including its projected
    // center or an edge-on ring, using the same signed axis-angle transform.
    screen_rotation: Option<Pos2>,
}

struct NumericTransform {
    text: String,
    error: Option<String>,
    /// Typing replaces the total operation, including any earlier pointer or
    /// keyboard preview. Every digit evaluates this same session baseline.
    before: Snapshot,
    pivot: DVec3,
}

/// Transient permission independent of geometry kind and file format. Reading,
/// selecting and navigating remain available in either mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditorAccess {
    #[default]
    ReadWrite,
    #[allow(dead_code)] // Hosts may opt into the shared read-only access policy.
    ReadOnly,
}

pub struct Editor {
    pub document: Document,
    pub frame: DisplayFrame,
    pub selected_object: Option<u64>,
    /// Selection truth; selected_object is the active member, or None if empty.
    pub selected_objects: BTreeSet<u64>,
    /// Transient pointer feedback; never serialized or included in undo history.
    pub hovered_object: Option<u64>,
    pub selected_vertices: BTreeSet<u64>,
    /// Transient viewport isolation. None shows the full document; Some shows
    /// only these object IDs. It is never serialized or included in history.
    visible_object_ids: Option<BTreeSet<u64>>,
    selection_depth: SelectionDepth,
    pub edit_mode: bool,
    pub tool: Tool,
    pub box_selection: BoxSelectionPolicy,
    /// Movement policy, independent of document units, presentation and history.
    pub snapping: SnapSettings,
    /// Resolves pointer precision into a concrete grid at gesture start.
    pub snap_policy: StepPolicy,
    /// Ephemeral transform axis; Scale uses an object's local axes, matching
    /// its handles. Never part of the document or history.
    pub transform_axis: Option<usize>,
    pub revision: u64,
    access: EditorAccess,
    asset_frames: AssetFrames,
    history: EditHistory<Snapshot>,
    property_transaction: bool,
    property_snapping: SnapSettings,
    property_movement: bool,
    gesture: Option<Gesture>,
    suppress_release: bool,
    cache: Option<GeometryCache>,
    last_nudge: Option<Snapshot>,
    numeric: Option<NumericTransform>,
    // Provenance for the open vertex-edit mode, separate from each operation's
    // history transaction. Shared by snapshots; never serialized in documents.
    geometry_edit: Option<Arc<GeometryEdit>>,
}

impl Editor {
    pub fn new(document: Document) -> Result<Self, String> {
        document.validate()?;
        let frame = DisplayFrame::from_document(&document)?;
        Ok(Self {
            document,
            frame,
            selected_object: None,
            selected_objects: BTreeSet::new(),
            hovered_object: None,
            selected_vertices: BTreeSet::new(),
            visible_object_ids: None,
            selection_depth: SelectionDepth::default(),
            edit_mode: false,
            tool: Tool::View,
            box_selection: BoxSelectionPolicy::default(),
            snapping: SnapSettings::default(),
            snap_policy: StepPolicy::default(),
            transform_axis: None,
            revision: 0,
            access: EditorAccess::ReadWrite,
            asset_frames: AssetFrames::new(),
            history: EditHistory::new(HISTORY_LIMIT),
            property_transaction: false,
            property_snapping: SnapSettings::default(),
            property_movement: false,
            gesture: None,
            suppress_release: false,
            cache: None,
            last_nudge: None,
            numeric: None,
            geometry_edit: None,
        })
    }

    pub fn access(&self) -> EditorAccess {
        self.access
    }

    pub fn can_edit(&self) -> bool {
        self.access == EditorAccess::ReadWrite
    }

    /// Locking cancels any unaccepted preview while write access still exists.
    /// History is retained, so unlocking can resume normal undo/redo.
    pub fn set_access(&mut self, access: EditorAccess) {
        if self.access == access {
            return;
        }
        self.cancel();
        self.leave_edit();
        self.tool = Tool::View;
        self.last_nudge = None;
        self.access = access;
    }

    fn require_write(&self) -> Result<(), String> {
        if self.can_edit() {
            Ok(())
        } else {
            Err("This document is read-only.".into())
        }
    }

    pub(crate) fn asset_frames(&self) -> &AssetFrames {
        &self.asset_frames
    }

    /// Publish evaluated resources atomically. References and history stay intact;
    /// revision invalidation refreshes only derived geometry and picking caches.
    pub(crate) fn set_asset_frames(&mut self, frames: AssetFrames) -> Result<(), String> {
        if frames.len() == self.asset_frames.len()
            && frames.iter().all(|(key, value)| {
                self.asset_frames
                    .get(key)
                    .is_some_and(|old| Arc::ptr_eq(old, value))
            })
        {
            return Ok(());
        }
        self.document
            .render_mesh_with_assets(&self.frame, &frames)?;
        self.asset_frames = frames;
        self.changed();
        Ok(())
    }

    pub(crate) fn render_mesh(&self) -> Result<crate::mesh::MeshData, String> {
        self.document
            .render_mesh_with_assets(&self.frame, &self.asset_frames)
    }

    pub fn set_tool(&mut self, tool: Tool) {
        let tool = if self.can_edit() { tool } else { Tool::View };
        if self.tool != tool {
            self.cancel();
            self.last_nudge = None;
        }
        self.tool = tool;
        if tool == Tool::View {
            self.transform_axis = None;
        }
    }

    pub fn toggle_transform_axis(&mut self, axis: usize) -> Result<(), String> {
        self.require_write()?;
        if axis > 2 {
            return Err("Transform axis must be X, Y, or Z.".into());
        }
        if self.tool == Tool::View {
            return Err("Choose Move, Rotate, or Scale before locking an axis.".into());
        }
        if self.tool == Tool::Scale && !self.edit_mode && self.selected_objects.len() > 1 {
            return Err(
                "Multiple objects support uniform scaling only; use the uniform scale handle."
                    .into(),
            );
        }
        if self.is_pointer_interacting() {
            return Err("Release the current drag before changing its axis.".into());
        }
        self.last_nudge = None;
        self.transform_axis = (self.transform_axis != Some(axis)).then_some(axis);
        if self.numeric.is_some() {
            if self.transform_axis.is_none() {
                let numeric = self.numeric.take().unwrap();
                self.publish_numeric_candidate(numeric.before.document)?;
            } else {
                self.preview_numeric()?;
            }
        }
        Ok(())
    }

    pub fn numeric_input_available(&self) -> bool {
        if !self.can_edit() {
            return false;
        }
        self.tool != Tool::View
            && self.transform_axis.is_some_and(|axis| axis < 3)
            && !self.property_transaction
            && self.selected_object.is_some()
            && !self.selected_objects.is_empty()
            && (!self.edit_mode || !self.selected_vertices.is_empty())
            && !(self.tool == Tool::Scale && !self.edit_mode && self.selected_objects.len() > 1)
            && !matches!(
                self.gesture,
                Some(Gesture::Marquee { .. } | Gesture::ObjectMarquee { .. })
            )
    }

    /// Accepted text owns the transform until acknowledged or cancelled. The
    /// first character retires a pointer gesture so later motion/release cannot
    /// silently overwrite the number. Incomplete text keeps the last preview.
    pub fn numeric_input(&mut self, character: char) -> Result<(), String> {
        if !self.numeric_input_available() {
            return Err(
                "Select geometry and lock a transform axis before entering a value.".into(),
            );
        }
        if !numeric_transform::accepts(character) {
            return Err("Transform values use signed decimal numbers.".into());
        }
        if self.numeric.is_none() {
            let before = self
                .history
                .transaction_baseline()
                .cloned()
                .unwrap_or_else(|| self.snapshot());
            let pivot = snapshot_pivot(&before, &self.frame, &self.asset_frames)?;
            self.history.begin_transaction(before.clone());
            self.numeric = Some(NumericTransform {
                text: String::new(),
                error: None,
                before,
                pivot,
            });
        }
        self.gesture = None;
        self.suppress_release = true;
        self.last_nudge = None;
        let numeric = self.numeric.as_mut().unwrap();
        // Bound transient text independently of f64's range; overflow remains
        // visible and editable instead of accepting a truncated number.
        if numeric.text.len() >= 512 {
            let error = "The transform value is too long. Use Backspace to shorten it.";
            numeric.error = Some(error.into());
            return Err(error.into());
        }
        numeric.text.push(character);
        self.preview_numeric()
    }

    pub fn numeric_backspace(&mut self) -> Result<(), String> {
        if let Some(numeric) = &mut self.numeric {
            numeric.text.pop();
            self.preview_numeric()?;
        }
        Ok(())
    }

    pub fn numeric_text(&self) -> Option<&str> {
        self.numeric.as_ref().map(|numeric| numeric.text.as_str())
    }

    pub fn numeric_error(&self) -> Option<&str> {
        self.numeric
            .as_ref()
            .and_then(|numeric| numeric.error.as_deref())
    }

    fn preview_numeric(&mut self) -> Result<(), String> {
        let Some(numeric) = &self.numeric else {
            return Ok(());
        };
        let parsed = numeric_transform::parse(&numeric.text);
        let error = match parsed {
            ParsedNumber::Prefix => Some("Complete the transform value before applying it."),
            ParsedNumber::Invalid => Some("Enter a finite signed decimal number."),
            ParsedNumber::Value(value) if self.tool == Tool::Scale && value <= 0.0 => {
                Some("A scale multiplier must be greater than zero.")
            }
            _ => None,
        };
        if let Some(error) = error {
            self.numeric.as_mut().unwrap().error = Some(error.into());
            return Ok(());
        }
        let mut candidate = numeric.before.document.clone();
        let result = (|| -> Result<(), String> {
            if let ParsedNumber::Value(value) = parsed {
                let index = self
                    .transform_axis
                    .ok_or("The numeric transform lost its axis.")?;
                let mut axis = [DVec3::X, DVec3::Y, DVec3::Z][index];
                if self.tool == Tool::Move {
                    translate_selection(
                        &mut candidate,
                        &numeric.before,
                        axis * value,
                        std::array::from_fn(|component| component == index),
                        self.snapping,
                        TranslationSource::Exact,
                    )?;
                } else {
                    if self.tool == Tool::Scale && !numeric.before.edit_mode {
                        let object = candidate
                            .objects
                            .iter()
                            .find(|object| Some(object.id) == numeric.before.selected_object)
                            .ok_or("The selected object no longer exists.")?;
                        axis = DQuat::from_array(object.transform.rotation) * axis;
                    }
                    SelectionTransform {
                        tool: self.tool,
                        kind: HandleKind::Axis(index),
                        pivot: numeric.pivot,
                        axis,
                        rotation: if self.tool == Tool::Rotate {
                            DQuat::from_axis_angle(axis, value.to_radians())
                        } else {
                            DQuat::IDENTITY
                        },
                        factor: if self.tool == Tool::Scale { value } else { 1.0 },
                    }
                    .apply(&mut candidate, &numeric.before)?;
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.numeric.as_mut().unwrap().error = Some(error);
            return Ok(());
        }
        if let Err(error) = self.publish_numeric_candidate(candidate) {
            self.numeric.as_mut().unwrap().error = Some(error);
            return Ok(());
        }
        self.numeric.as_mut().unwrap().error = None;
        Ok(())
    }

    fn publish_numeric_candidate(&mut self, mut candidate: Document) -> Result<(), String> {
        self.require_write()?;
        self.validate_candidate(&mut candidate)?;
        if candidate != self.document {
            self.document = candidate;
            self.changed();
        }
        Ok(())
    }

    /// Translate in document units. A locked or pending Move previews within its
    /// session. Otherwise repeats join only the preceding unchanged nudge result.
    pub fn nudge(&mut self, delta_world: DVec3, repeat: bool) -> Result<bool, String> {
        self.require_write()?;
        if self.numeric.is_some() {
            return Err("Apply or cancel the numeric transform before nudging.".into());
        }
        if !delta_world.is_finite() {
            return Err("Move displacement must be finite.".into());
        }
        if self.tool != Tool::Move || delta_world == DVec3::ZERO {
            return Ok(false);
        }
        let in_session = self.transform_axis.is_some() || self.has_transform_session();
        if in_session && self.is_pointer_interacting() {
            return Err("Release the current drag before nudging.".into());
        }
        if !in_session && self.cancel() {
            self.last_nudge = None;
        }
        let before = self.snapshot();
        let coalesce =
            repeat && self.last_nudge.as_ref() == Some(&before) && self.history.undo_len() != 0;
        let mut candidate = self.document.clone();
        translate_selection(
            &mut candidate,
            &before,
            delta_world,
            [true; 3],
            // Keyboard nudges have an explicit centimeter contract. Neither
            // adaptive viewport precision nor a coarse fixed pointer grid may
            // swallow an arrow press. Off-grid alignment retains its semantics.
            SnapSettings {
                step_cm: crate::move_input::DEFAULT_NUDGE_CM,
                ..self.snapping
            },
            TranslationSource::Interactive,
        )?;
        self.validate_candidate(&mut candidate)?;
        if candidate == self.document {
            return Ok(false);
        }
        self.document = candidate;
        self.reconcile_selection();
        if in_session {
            self.history.begin_transaction(before);
        } else if coalesce {
            self.history.clear_redo();
        } else {
            self.push_undo(before);
        }
        self.changed();
        self.last_nudge = (!in_session).then(|| self.snapshot());
        Ok(true)
    }

    /// Validate a complete candidate before publishing it or adding an undo entry.
    pub fn commit(
        &mut self,
        _label: &str,
        change: impl FnOnce(&mut Document) -> Result<(), String>,
    ) -> Result<bool, String> {
        self.require_write()?;
        self.cancel();
        self.last_nudge = None;
        let mut candidate = self.document.clone();
        change(&mut candidate)?;
        self.validate_candidate(&mut candidate)?;
        if candidate == self.document {
            return Ok(false);
        }
        let before = self.snapshot();
        self.document = candidate;
        self.reconcile_selection();
        self.push_undo(before);
        self.changed();
        Ok(true)
    }

    /// One numeric-field interaction owns one document transaction. Preview
    /// always starts from the captured baseline, so revising a value cannot
    /// accumulate geometry or quaternion roundoff across UI frames.
    pub fn begin_property_edit(&mut self) -> bool {
        if !self.can_edit() || self.is_interacting() || self.transform_axis.is_some() {
            return false;
        }
        self.last_nudge = None;
        if !self.history.begin_transaction(self.snapshot()) {
            return false;
        }
        self.property_transaction = true;
        self.property_snapping = self.snapping;
        self.property_movement = false;
        true
    }

    /// Numeric scrubs use the field's physical sensitivity, independently of
    /// camera zoom, display units and backing pixel density. Capture once so
    /// feeding the snapped value back to the widget cannot change its precision.
    pub fn begin_property_translation(&mut self, cm_per_point: f64) -> Result<bool, String> {
        let snapping = self.snap_policy.resolve(self.snapping, cm_per_point)?;
        if !self.begin_property_edit() {
            return Ok(false);
        }
        self.property_snapping = snapping;
        self.property_movement = true;
        Ok(true)
    }

    /// A length scrub and a typed coordinate share the translation boundary.
    /// Object values are absolute positions; vertex values are a shared offset
    /// from the field session's baseline. Typed values explicitly bypass snaps.
    pub fn preview_property_translation(
        &mut self,
        axis: usize,
        value: f64,
        source: TranslationSource,
    ) -> Result<bool, String> {
        if axis > 2 || !value.is_finite() {
            return Err("A translation needs a valid axis and finite length.".into());
        }
        if !self.property_movement {
            return Err("No translation field edit is active.".into());
        }
        let snapping = self.property_snapping;
        self.preview_property_edit_from_baseline(|document, before| {
            let object = before
                .document
                .objects
                .iter()
                .find(|object| Some(object.id) == before.selected_object)
                .ok_or("The edited object no longer exists.")?;
            let mut delta = DVec3::ZERO;
            delta[axis] = if before.edit_mode {
                value
            } else {
                value - object.transform.translation[axis]
            };
            let mut axes = [false; 3];
            axes[axis] = true;
            translate_selection(document, before, delta, axes, snapping, source)?;
            // Preserve an exact typed destination, including tiny fractional
            // values which subtraction from the baseline could otherwise lose.
            if !before.edit_mode && source == TranslationSource::Exact {
                document
                    .objects
                    .iter_mut()
                    .find(|object| Some(object.id) == before.selected_object)
                    .ok_or("The edited object no longer exists.")?
                    .transform
                    .translation[axis] = value;
            }
            Ok(())
        })
    }

    /// Applied field value for scrub presentation. egui retains its independent
    /// high-precision drag accumulator, while the visible number matches the
    /// snapped document preview rather than the raw pointer displacement.
    pub fn property_translation_value(&self, axis: usize) -> Option<f64> {
        if axis > 2 || !self.property_transaction {
            return None;
        }
        let before = self.history.transaction_baseline()?;
        let original = before
            .document
            .objects
            .iter()
            .find(|object| Some(object.id) == before.selected_object)?;
        let current = self
            .document
            .objects
            .iter()
            .find(|object| object.id == original.id)?;
        if !before.edit_mode {
            return Some(current.transform.translation[axis]);
        }
        let old = geometry_edit::evaluated(&original.geometry).ok()?;
        let new = geometry_edit::evaluated(&current.geometry).ok()?;
        let from = old
            .vertices
            .iter()
            .find(|vertex| before.selected_vertices.contains(&vertex.id))?;
        let to = new.vertices.iter().find(|vertex| vertex.id == from.id)?;
        let local_delta = DVec3::from_array(to.position) - DVec3::from_array(from.position);
        Some(current.transform.matrix().transform_vector3(local_delta)[axis])
    }

    pub fn preview_property_edit(
        &mut self,
        change: impl FnOnce(&mut Document) -> Result<(), String>,
    ) -> Result<bool, String> {
        self.preview_property_edit_from_baseline(|document, _| change(document))
    }

    fn preview_property_edit_from_baseline(
        &mut self,
        change: impl FnOnce(&mut Document, &Snapshot) -> Result<(), String>,
    ) -> Result<bool, String> {
        self.require_write()?;
        if !self.property_transaction {
            return Err("No property edit is active.".into());
        }
        let before = self
            .history
            .transaction_baseline()
            .ok_or("The property edit lost its baseline.")?;
        let mut candidate = before.document.clone();
        change(&mut candidate, before)?;
        self.validate_candidate(&mut candidate)?;
        if candidate == self.document {
            return Ok(false);
        }
        self.document = candidate;
        self.reconcile_selection();
        self.changed();
        Ok(true)
    }

    pub fn finish_property_edit(&mut self, accept: bool) -> bool {
        if !self.property_transaction {
            return false;
        }
        self.property_transaction = false;
        self.property_movement = false;
        if accept {
            self.history.commit_transaction(&self.snapshot());
        } else if let Some(before) = self.history.cancel_transaction() {
            self.restore(before);
        }
        true
    }

    pub fn has_property_edit(&self) -> bool {
        self.property_transaction
    }

    pub fn rename_object(&mut self, id: u64, name: String) -> Result<bool, String> {
        let name = name.trim().to_owned();
        if name.is_empty() {
            return Err("Object name cannot be empty.".into());
        }
        self.commit("Rename object", |document| {
            document
                .objects
                .iter_mut()
                .find(|object| object.id == id)
                .ok_or("Object no longer exists.")?
                .name = name;
            Ok(())
        })
    }

    pub fn undo(&mut self) -> bool {
        if !self.can_edit() {
            return false;
        }
        self.last_nudge = None;
        if self.cancel() {
            return true;
        }
        let Some(previous) = self.history.undo(self.snapshot()) else {
            return false;
        };
        self.restore(previous);
        true
    }

    pub fn redo(&mut self) -> bool {
        if !self.can_edit() {
            return false;
        }
        self.last_nudge = None;
        if self.cancel() {
            return true;
        }
        let Some(next) = self.history.redo(self.snapshot()) else {
            return false;
        };
        self.restore(next);
        true
    }

    pub fn cancel(&mut self) -> bool {
        self.numeric = None;
        self.transform_axis = None;
        self.property_transaction = false;
        self.property_movement = false;
        if let Some(before) = self.history.cancel_transaction() {
            self.gesture = None;
            self.last_nudge = None;
            self.restore(before);
            self.suppress_release = true;
            return true;
        }
        match self.gesture.take() {
            Some(Gesture::Transform(drag)) => self.restore(drag.before),
            Some(Gesture::Marquee { before, .. }) => self.selected_vertices = before,
            Some(Gesture::ObjectMarquee {
                before,
                before_active,
                ..
            }) => {
                self.set_object_selection_set(before, before_active);
            }
            None => return false,
        }
        self.suppress_release = true;
        true
    }

    /// Finish a pending interaction, or toggle editing for the selected mesh.
    pub fn confirm(&mut self) -> Result<ConfirmOutcome, String> {
        if !self.can_edit() {
            return Ok(ConfirmOutcome::NothingToDo);
        }
        if let Some(error) = self.numeric_error() {
            return Err(error.to_owned());
        }
        if self.has_property_edit() {
            self.finish_property_edit(true);
            return Ok(ConfirmOutcome::InteractionFinished);
        }
        if self.has_transform_session() || self.transform_axis.is_some() {
            self.numeric = None;
            self.suppress_release = true;
            self.gesture = None;
            self.transform_axis = None;
            self.last_nudge = None;
            if self.history.has_transaction() {
                if let Err(error) = self
                    .document
                    .validate()
                    .and_then(|()| self.render_mesh().map(|_| ()))
                {
                    let before = self.history.cancel_transaction().unwrap();
                    self.restore(before);
                    return Err(error);
                }
                // Selection belongs to undo snapshots, but changing selection
                // alone never creates a document history entry.
                if self
                    .history
                    .transaction_baseline()
                    .is_some_and(|before| self.document != before.document)
                {
                    self.history.commit_transaction(&self.snapshot());
                } else {
                    self.history.cancel_transaction();
                }
            }
            return Ok(ConfirmOutcome::InteractionFinished);
        }
        if self.is_interacting() {
            // egui's stop_dragging leaves a possible click alive. The release
            // of this finished gesture must not become a selection click.
            self.suppress_release = true;
        }
        if self.finish_gesture()? {
            Ok(ConfirmOutcome::InteractionFinished)
        } else if self.edit_mode {
            self.leave_edit();
            Ok(ConfirmOutcome::EditModeLeft)
        } else if self.selected_object.is_some() {
            if self.enter_edit()? {
                Ok(ConfirmOutcome::EditModeEntered)
            } else {
                Ok(ConfirmOutcome::NothingToDo)
            }
        } else {
            Ok(ConfirmOutcome::NothingToDo)
        }
    }

    fn finish_gesture(&mut self) -> Result<bool, String> {
        match self.gesture.take() {
            Some(Gesture::Transform(drag)) => {
                if self.document != drag.before.document {
                    if let Err(error) = self
                        .document
                        .validate()
                        .and_then(|()| self.render_mesh().map(|_| ()))
                    {
                        if self.has_transform_session() {
                            self.cancel();
                        } else {
                            self.restore(drag.before);
                        }
                        return Err(error);
                    }
                    if !self.has_transform_session() {
                        self.push_undo(drag.before);
                    }
                }
            }
            Some(Gesture::Marquee {
                current,
                additive,
                before,
                pending,
                dragged,
                ..
            }) => {
                if dragged {
                    self.set_vertex_selection(pending);
                } else {
                    self.selected_vertices = before;
                    self.select_vertex(current, additive);
                }
            }
            Some(Gesture::ObjectMarquee {
                current,
                additive,
                before,
                before_active,
                pending,
                projection,
                dragged,
                ..
            }) => {
                if dragged {
                    self.set_object_selection_set(pending, before_active);
                } else {
                    self.set_object_selection_set(before, before_active);
                    let object = self.cache.as_ref().unwrap().object_at(current, &projection);
                    self.apply_object_click(object, additive);
                }
            }
            None => return Ok(false),
        }
        Ok(true)
    }

    /// Unwind one interaction or selection level without adding document history.
    pub fn escape(&mut self) -> EscapeOutcome {
        let armed = self.transform_axis.is_some();
        if self.cancel() {
            EscapeOutcome::InteractionCancelled
        } else if armed {
            self.last_nudge = None;
            EscapeOutcome::AxisUnlocked
        } else if self.edit_mode && !self.selected_vertices.is_empty() {
            self.selected_vertices.clear();
            EscapeOutcome::VerticesDeselected
        } else if self.edit_mode {
            self.leave_edit();
            EscapeOutcome::EditModeLeft
        } else if self.selected_object.is_some() {
            self.deselect();
            EscapeOutcome::ObjectDeselected
        } else {
            EscapeOutcome::NothingToDo
        }
    }

    /// Explicitly return to an idle object-mode selection. Pending edits roll back.
    pub fn deselect(&mut self) {
        self.cancel();
        self.set_object_selection(None);
    }

    /// Object mode includes every object in the current view. Vertex mode
    /// follows the same explicit depth policy as pointer selection.
    pub fn select_all(
        &mut self,
        viewport: Rect,
        camera: &Camera,
        z_up: bool,
    ) -> Result<(), String> {
        self.cancel();
        if self.edit_mode {
            let selected = self
                .selectable_vertex_order(viewport, camera, z_up)?
                .into_iter()
                .collect();
            self.set_vertex_selection(selected);
        } else {
            let objects: BTreeSet<_> = self
                .document
                .objects
                .iter()
                .filter(|object| self.is_object_visible(object.id))
                .map(|object| object.id)
                .collect();
            let active = self
                .selected_object
                .filter(|id| objects.contains(id))
                .or_else(|| {
                    self.document
                        .objects
                        .iter()
                        .find(|object| objects.contains(&object.id))
                        .map(|object| object.id)
                });
            self.set_object_selection_set(objects, active);
        }
        Ok(())
    }

    /// Remove the selected objects, or selected mesh vertices and every incident
    /// polygon or loose edge. Surviving IDs, loose vertices, and empty objects are
    /// retained. This is deletion, not dissolve, remeshing, or cleanup.
    pub fn delete_selection(&mut self) -> Result<bool, String> {
        self.require_write()?;
        self.cancel();
        if self.edit_mode {
            if self.selected_vertices.is_empty() {
                return Ok(false);
            }
            let object_id = self
                .selected_object
                .ok_or("Select an object before deleting vertices.")?;
            let selected = self.selected_vertices.clone();
            self.commit("Delete vertices", |document| {
                let object = document
                    .objects
                    .iter_mut()
                    .find(|object| object.id == object_id)
                    .ok_or("The edited object no longer exists.")?;
                let mesh = geometry_edit::editable(&mut object.geometry)?;
                mesh.faces
                    .retain(|face| !face.vertices.iter().any(|id| selected.contains(id)));
                mesh.edges
                    .retain(|edge| !edge.iter().any(|id| selected.contains(id)));
                mesh.vertices
                    .retain(|vertex| !selected.contains(&vertex.id));
                Ok(())
            })
        } else {
            if self.selected_objects.is_empty() {
                return Ok(false);
            }
            let selected = self.selected_objects.clone();
            self.commit("Delete objects", |document| {
                document
                    .objects
                    .retain(|object| !selected.contains(&object.id));
                Ok(())
            })
        }
    }

    /// Traverse document order, not numeric IDs. In vertex mode only currently
    /// eligible vertices qualify; forward starts after the last selected eligible
    /// vertex, and reverse starts before the first. Traversal wraps and reduces
    /// a multiple selection to one element. No eligible elements is a no-op.
    pub fn cycle_selection(
        &mut self,
        reverse: bool,
        viewport: Rect,
        camera: &Camera,
        z_up: bool,
    ) -> Result<(), String> {
        self.cancel();
        if self.edit_mode {
            let order = self.selectable_vertex_order(viewport, camera, z_up)?;
            if order.is_empty() {
                return Ok(());
            }
            let anchor = if reverse {
                order
                    .iter()
                    .position(|id| self.selected_vertices.contains(id))
            } else {
                order
                    .iter()
                    .rposition(|id| self.selected_vertices.contains(id))
            };
            self.set_vertex_selection(BTreeSet::from([
                order[cycle_index(order.len(), anchor, reverse)]
            ]));
        } else {
            let order: Vec<_> = self
                .document
                .objects
                .iter()
                .filter(|object| self.is_object_visible(object.id))
                .map(|object| object.id)
                .collect();
            if order.is_empty() {
                return Ok(());
            }
            let anchor = order
                .iter()
                .position(|id| Some(*id) == self.selected_object);
            let index = cycle_index(order.len(), anchor, reverse);
            self.set_object_selection(Some(order[index]));
        }
        Ok(())
    }

    fn selectable_vertex_order(
        &mut self,
        viewport: Rect,
        camera: &Camera,
        z_up: bool,
    ) -> Result<Vec<u64>, String> {
        let object_id = self
            .selected_object
            .ok_or("Select an object before selecting vertices.")?;
        let eligible: BTreeSet<_> = self
            .selectable_vertices(viewport, camera, z_up)?
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let object = self
            .document
            .objects
            .iter()
            .find(|object| object.id == object_id)
            .ok_or("The edited object no longer exists.")?;
        let mesh = geometry_edit::evaluated(&object.geometry)?;
        Ok(mesh
            .vertices
            .iter()
            .filter(|vertex| eligible.contains(&vertex.id))
            .map(|vertex| vertex.id)
            .collect())
    }

    pub fn is_interacting(&self) -> bool {
        self.is_pointer_interacting() || self.history.has_transaction()
    }

    /// A released transform preview owns its undo baseline, not the camera.
    /// Pointer gestures and inspector edits retain exclusive input ownership.
    pub fn blocks_navigation(&self) -> bool {
        self.is_pointer_interacting() || self.has_property_edit()
    }

    pub fn has_transform_session(&self) -> bool {
        self.history.has_transaction() && !self.property_transaction
    }

    pub fn is_pointer_interacting(&self) -> bool {
        self.gesture.is_some()
    }

    pub fn is_transforming(&self) -> bool {
        self.has_transform_session() || matches!(self.gesture, Some(Gesture::Transform(_)))
    }

    pub fn xray_enabled(&self) -> bool {
        self.selection_depth == SelectionDepth::Through
    }

    /// Change viewport selection depth without touching geometry, selection,
    /// or history. An active pointer/inspector gesture or numeric input keeps
    /// exclusive ownership; a released transform preview retains its baseline.
    pub fn set_xray(&mut self, enabled: bool) -> bool {
        if self.is_pointer_interacting()
            || self.property_transaction
            || self.numeric.is_some()
            || self.xray_enabled() == enabled
        {
            return false;
        }
        self.selection_depth = if enabled {
            SelectionDepth::Through
        } else {
            SelectionDepth::VisibleOnly
        };
        true
    }

    /// Restrict editor hit testing and selection to a transient set of objects.
    /// A visibility change cancels an in-progress interaction and does not edit
    /// the document or record history. Passing None restores the full scene.
    pub fn set_visible_objects(&mut self, visible: Option<BTreeSet<u64>>) {
        if self.visible_object_ids == visible {
            return;
        }
        self.cancel();
        self.visible_object_ids = visible;
        self.cache = None;
        self.hovered_object = None;
        self.reconcile_selection();
    }

    pub fn visible_objects(&self) -> Option<&BTreeSet<u64>> {
        self.visible_object_ids.as_ref()
    }

    pub fn is_object_visible(&self, id: u64) -> bool {
        self.visible_object_ids
            .as_ref()
            .is_none_or(|visible| visible.contains(&id))
    }

    pub fn select_object(&mut self, id: u64) -> Result<(), String> {
        self.select_object_with_modifier(id, false)
    }

    pub fn select_object_with_modifier(&mut self, id: u64, additive: bool) -> Result<(), String> {
        if !self.document.objects.iter().any(|object| object.id == id) {
            return Err("The selected object no longer exists.".into());
        }
        if !self.is_object_visible(id) {
            return Err("The object is hidden from the current view.".into());
        }
        self.cancel();
        self.apply_object_click(Some(id), additive);
        Ok(())
    }

    pub fn hover_object(&mut self, object: Option<u64>) {
        self.hovered_object = object.filter(|id| {
            !self.edit_mode
                && self.is_object_visible(*id)
                && self.document.objects.iter().any(|object| object.id == *id)
        });
    }

    /// Query cached surfaces and loose edges without projecting every vertex.
    /// The caller owns overlay and window hit testing.
    pub fn object_at(
        &mut self,
        position: Pos2,
        viewport: Rect,
        camera: &Camera,
        z_up: bool,
    ) -> Result<Option<u64>, String> {
        if !viewport.contains(position) {
            return Ok(None);
        }
        let projection = Projection::new(viewport, camera, z_up)?;
        if self.cache.is_none() {
            self.cache = Some(GeometryCache::with_assets(
                &self.document,
                &self.frame,
                self.visible_object_ids.as_ref(),
                &self.asset_frames,
            )?);
        }
        Ok(self
            .cache
            .as_ref()
            .unwrap()
            .object_at(position, &projection))
    }

    fn set_object_selection(&mut self, object: Option<u64>) {
        self.set_object_selection_set(object.into_iter().collect(), object);
    }

    fn set_object_selection_set(
        &mut self,
        mut objects: BTreeSet<u64>,
        preferred_active: Option<u64>,
    ) {
        let valid: BTreeSet<_> = self
            .document
            .objects
            .iter()
            .filter(|object| self.is_object_visible(object.id))
            .map(|object| object.id)
            .collect();
        objects.retain(|id| valid.contains(id));
        let next_active = preferred_active
            .filter(|id| objects.contains(id))
            .or_else(|| objects.first().copied());
        if self.edit_mode || objects != self.selected_objects || next_active != self.selected_object
        {
            self.transform_axis = None;
            self.last_nudge = None;
        }
        self.selected_object = preferred_active
            .filter(|id| objects.contains(id))
            .or_else(|| objects.first().copied());
        self.selected_objects = objects;
        self.selected_vertices.clear();
        self.edit_mode = false;
        self.geometry_edit = None;
    }

    fn apply_object_click(&mut self, object: Option<u64>, additive: bool) {
        if !additive {
            self.set_object_selection(object);
            return;
        }
        let mut selected = self.selected_objects.clone();
        let mut active = self.selected_object;
        if let Some(id) = object {
            if selected.remove(&id) {
                if active == Some(id) {
                    active = None;
                }
            } else {
                selected.insert(id);
                active = Some(id);
            }
        }
        self.set_object_selection_set(selected, active);
    }

    /// Explicit conversion remains available to fixture setup. User editing
    /// enters directly and converts together with its first effective change.
    #[cfg(test)]
    pub fn convert_selected(&mut self) -> Result<bool, String> {
        if self.selected_objects.len() > 1 {
            return Err("Select one object before converting its geometry.".into());
        }
        let id = self.selected_object.ok_or("Select an object first.")?;
        self.leave_edit();
        self.commit("Convert to mesh", |document| {
            document.convert_object(id)?;
            Ok(())
        })
    }

    pub fn insert(&mut self, kind: PrimitiveKind) -> Result<u64, String> {
        self.insert_with_rotation(kind, DQuat::IDENTITY)
    }

    pub fn insert_with_rotation(
        &mut self,
        kind: PrimitiveKind,
        rotation: DQuat,
    ) -> Result<u64, String> {
        let mut id = 0;
        self.commit("Insert primitive", |document| {
            id = document.insert_primitive(kind)?;
            document
                .objects
                .last_mut()
                .ok_or("Inserted object is missing")?
                .transform
                .rotation = rotation.to_array();
            Ok(())
        })?;
        // New objects belong to the view in which they were created.
        if let Some(visible) = &mut self.visible_object_ids {
            visible.insert(id);
            self.cache = None;
        }
        self.select_object(id)?;
        Ok(id)
    }

    /// Append imported objects atomically with their resolved resources. All
    /// object IDs are remapped; linked source payloads stay outside history.
    pub(crate) fn import_objects(
        &mut self,
        mut objects: Vec<crate::document::Object>,
        frames: AssetFrames,
    ) -> Result<Vec<u64>, String> {
        self.require_write()?;
        if self.is_interacting() || self.transform_axis.is_some() {
            return Err("Apply or cancel the current interaction before importing objects.".into());
        }
        if objects.is_empty() {
            return Ok(Vec::new());
        }
        let mut next = self
            .document
            .objects
            .iter()
            .map(|object| object.id)
            .max()
            .unwrap_or(0);
        let mut ids = Vec::with_capacity(objects.len());
        for object in &mut objects {
            next = next.checked_add(1).ok_or("Object ID space exhausted")?;
            object.id = next;
            ids.push(next);
        }
        let mut candidate = self.document.clone();
        candidate.objects.extend(objects);
        let mut resources = self.asset_frames.clone();
        resources.extend(frames);
        candidate.validate()?;
        // With no existing world to preserve, establish normalization before
        // validating the first imported asset's GPU-coordinate boundary.
        let frame = if self.document.objects.is_empty() {
            DisplayFrame::from_document_with_assets(&candidate, &resources)?
        } else {
            self.frame
        };
        candidate.render_mesh_with_assets(&frame, &resources)?;
        let before = self.snapshot();
        self.document = candidate;
        self.frame = frame;
        self.asset_frames = resources;
        if let Some(visible) = &mut self.visible_object_ids {
            visible.extend(ids.iter().copied());
        }
        self.set_object_selection_set(ids.iter().copied().collect(), ids.first().copied());
        self.last_nudge = None;
        self.push_undo(before);
        self.changed();
        Ok(ids)
    }

    #[cfg(test)]
    pub(crate) fn insert_asset(
        &mut self,
        asset: crate::document::AssetInstance,
        name: String,
    ) -> Result<u64, String> {
        let mut id = 0;
        self.commit("Insert linked asset", |document| {
            id = document.insert_asset(asset, name)?;
            Ok(())
        })?;
        if let Some(visible) = &mut self.visible_object_ids {
            visible.insert(id);
            self.cache = None;
        }
        self.select_object(id)?;
        Ok(id)
    }

    pub fn can_duplicate_selection(&self) -> bool {
        self.can_edit()
            && !self.edit_mode
            && !self.selected_objects.is_empty()
            && !self.is_interacting()
            && self.transform_axis.is_none()
    }

    /// Append independent copies in source document order, without displacement.
    /// Object IDs are fresh; vertex/face IDs remain scoped to each copied object.
    /// Publish geometry and the copied selection together as one undoable edit.
    pub fn duplicate_selection(&mut self) -> Result<bool, String> {
        self.require_write()?;
        if !self.can_duplicate_selection() {
            return Ok(false);
        }
        let mut next_id = self
            .document
            .objects
            .iter()
            .map(|object| object.id)
            .max()
            .unwrap_or(0);
        let mut copies = Vec::new();
        let mut copied_ids = BTreeMap::new();
        for object in &self.document.objects {
            if self.selected_objects.contains(&object.id) {
                next_id = next_id.checked_add(1).ok_or("Object ID space exhausted")?;
                let mut copy = object.clone();
                copy.id = next_id;
                copy.name = format!("{} copy", object.name);
                copied_ids.insert(object.id, copy.id);
                copies.push(copy);
            }
        }
        if copies.is_empty() {
            return Ok(false);
        }
        let mut candidate = self.document.clone();
        candidate.objects.extend(copies);
        self.validate_candidate(&mut candidate)?;
        let before = self.snapshot();
        let active = self
            .selected_object
            .and_then(|id| copied_ids.get(&id).copied());
        self.document = candidate;
        let copies: BTreeSet<_> = copied_ids.into_values().collect();
        if let Some(visible) = &mut self.visible_object_ids {
            visible.extend(copies.iter().copied());
        }
        self.set_object_selection_set(copies, active);
        self.push_undo(before);
        self.changed();
        Ok(true)
    }

    pub fn enter_edit(&mut self) -> Result<bool, String> {
        self.require_write()?;
        if self
            .selected_object
            .and_then(|id| self.document.objects.iter().find(|object| object.id == id))
            .is_some_and(|object| matches!(object.geometry, Geometry::Asset(_)))
        {
            return Ok(false);
        }
        self.cancel();
        if self.selected_objects.len() > 1 {
            return Err("Select one object before editing its vertices.".into());
        }
        let id = self.selected_object.ok_or("Select an object first.")?;
        let object = self
            .document
            .objects
            .iter()
            .find(|object| object.id == id)
            .ok_or("The selected object no longer exists.")?;
        let changed = !self.edit_mode;
        if changed {
            self.geometry_edit = GeometryEdit::capture(object)?.map(Arc::new);
            self.transform_axis = None;
            self.last_nudge = None;
        }
        self.edit_mode = true;
        self.hovered_object = None;
        Ok(changed)
    }

    pub fn leave_edit(&mut self) {
        self.cancel();
        if self.edit_mode {
            self.set_object_selection(self.selected_object);
        }
    }

    pub fn reframe(&mut self) -> Result<(), String> {
        self.cancel();
        self.frame = DisplayFrame::from_document_with_assets(&self.document, &self.asset_frames)?;
        self.changed();
        Ok(())
    }

    /// Current selected geometry in the exact display coordinates used by rendering.
    /// Reading the selection does not reframe, change history, or cancel a preview.
    /// Edit-mode selection stays selected even when its vertices become occluded.
    pub fn selection_points(&self, z_up: bool) -> Result<Vec<Vec3>, String> {
        Ok(self
            .selection_point_groups(z_up)?
            .into_iter()
            .flatten()
            .collect())
    }

    /// One nonempty group per selected object, in document order. Keeping these
    /// groups separate preserves gaps when displaying selection extents. Vertex
    /// editing produces one group of selected vertices, without a visibility filter.
    pub fn selection_point_groups(&self, z_up: bool) -> Result<Vec<Vec<Vec3>>, String> {
        let rotation = display_rotation(z_up);
        let mut groups = Vec::new();
        for object in &self.document.objects {
            if !self.selected_objects.contains(&object.id)
                || (self.edit_mode && Some(object.id) != self.selected_object)
            {
                continue;
            }
            if let Geometry::Asset(asset) = &object.geometry {
                let world_points = if let Some(evaluated) = self.asset_frames.get(asset) {
                    crate::model::asset_geometry::AssetGeometry::new(object, evaluated)?
                        .into_framing_points(object)
                } else {
                    vec![DVec3::from_array(object.transform.translation)]
                };
                let points: Vec<_> = world_points
                    .into_iter()
                    .map(|point| {
                        rotation.transform_point3(self.frame.world_to_display(point).as_vec3())
                    })
                    .collect();
                if points.iter().any(|point| !point.is_finite()) {
                    return Err("Selected asset is outside the display coordinate range.".into());
                }
                if !points.is_empty() {
                    groups.push(points);
                }
                continue;
            }
            let geometry = self.document.eval_object(object.id)?;
            let transform = object.transform.matrix();
            let mut points = Vec::new();
            for vertex in &geometry.vertices {
                if self.edit_mode && !self.selected_vertices.contains(&vertex.id) {
                    continue;
                }
                let world = transform.transform_point3(DVec3::from_array(vertex.position));
                let display = self.frame.world_to_display(world).as_vec3();
                let point = rotation.transform_point3(display);
                if !point.is_finite() {
                    return Err("Selected geometry is outside the display coordinate range.".into());
                }
                points.push(point);
            }
            if !points.is_empty() {
                groups.push(points);
            }
        }
        Ok(groups)
    }

    /// Markers selectable in this viewport under the current X-ray policy.
    /// Surface occlusion is ignored in X-ray; isolation and clipping never are.
    pub fn selectable_vertices(
        &mut self,
        viewport: Rect,
        camera: &Camera,
        z_up: bool,
    ) -> Result<Vec<(u64, Pos2)>, String> {
        let projection = Projection::new(viewport, camera, z_up)?;
        self.prepare(&projection)?;
        Ok(self
            .cache
            .as_ref()
            .unwrap()
            .eligible_vertices(self.selection_depth)
            .filter(|vertex| Some(vertex.object) == self.selected_object)
            .map(|vertex| (vertex.id, vertex.screen))
            .collect())
    }

    /// Eligible projected bounds for box selection. Without X-ray, visibility is
    /// vertex-sampled: a face sliver whose vertices are occluded does not qualify.
    /// Near-clipped or otherwise unprojectable objects are excluded. The configurable
    /// hit policy compares these screen bounds, not the exact mesh silhouette.
    #[allow(dead_code)] // Read-only projection query used by executable guides.
    pub fn object_selection_bounds(
        &mut self,
        viewport: Rect,
        camera: &Camera,
        z_up: bool,
    ) -> Result<Vec<(u64, Rect)>, String> {
        let projection = Projection::new(viewport, camera, z_up)?;
        self.prepare(&projection)?;
        Ok(self.selection_bounds(&projection))
    }

    fn selection_bounds(&self, projection: &Projection) -> Vec<(u64, Rect)> {
        let cache = self.cache.as_ref().unwrap();
        let eligible: BTreeSet<_> = cache
            .eligible_vertices(self.selection_depth)
            .filter(|vertex| {
                cache
                    .selectable_vertices
                    .contains(&(vertex.object, vertex.id))
            })
            .map(|vertex| vertex.object)
            .collect();
        let mut bounds = BTreeMap::<u64, Rect>::new();
        let mut clipped = BTreeSet::new();
        for vertex in &cache.vertices {
            if let Some(screen) = projection.screen(vertex.position) {
                bounds
                    .entry(vertex.object)
                    .or_insert(Rect::NOTHING)
                    .extend_with(screen);
            } else {
                clipped.insert(vertex.object);
            }
        }
        bounds
            .into_iter()
            .filter(|(id, _)| eligible.contains(id) && !clipped.contains(id))
            .collect()
    }

    #[cfg(test)]
    pub fn handle_rects(
        &mut self,
        viewport: Rect,
        camera: &Camera,
        z_up: bool,
    ) -> Result<Vec<(HandleKind, Rect)>, String> {
        let projection = Projection::new(viewport, camera, z_up)?;
        self.prepare(&projection)?;
        Ok(self
            .handles(&projection)
            .into_iter()
            .map(|handle| (handle.kind, handle.target))
            .collect())
    }

    #[cfg(test)]
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        camera: &Camera,
        z_up: bool,
    ) -> Option<String> {
        self.ui_with_navigation(ui, response, response.rect, camera, z_up, false)
    }

    /// Project geometry against the full scene viewport; the response can cover
    /// a smaller interactive area when the 2D ruler overlays its edges.
    pub fn ui_with_navigation(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        viewport: Rect,
        camera: &Camera,
        z_up: bool,
        navigation_active: bool,
    ) -> Option<String> {
        match self.update_ui(ui, response, viewport, camera, z_up, navigation_active) {
            Ok(()) => None,
            Err(error) => {
                self.cancel();
                Some(error)
            }
        }
    }

    fn update_ui(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        viewport: Rect,
        camera: &Camera,
        z_up: bool,
        navigation_active: bool,
    ) -> Result<(), String> {
        if self.tool == Tool::View {
            self.transform_axis = None;
        }
        let projection = Projection::new(viewport, camera, z_up)?;
        let first_pass = ui.ctx().current_pass_index() == 0;
        if first_pass && !ui.input(|input| input.focused) {
            self.cancel();
        }
        self.prepare(&projection)?;
        let pointer = ui.input(|input| input.pointer.interact_pos());
        let primary_pressed =
            first_pass && ui.input(|input| input.pointer.button_pressed(PointerButton::Primary));
        let primary_released =
            first_pass && ui.input(|input| input.pointer.button_released(PointerButton::Primary));
        if primary_pressed {
            // A primary press routed to an active camera gesture must not
            // become a selection click if navigation ends before its release.
            self.suppress_release = navigation_active;
        }
        let focused = ui.input(|input| input.focused);
        let blocked = egui::Popup::is_any_open(ui.ctx())
            || ui.ctx().memory(|memory| memory.top_modal_layer().is_some());
        // Popups and navigation can own input while a released preview remains
        // pending. Only an active editor pointer gesture needs interruption;
        // navigation must never cancel the pending transaction or its axis.
        if first_pass && (blocked || navigation_active) && self.is_pointer_interacting() {
            self.cancel();
        }
        let available = focused && !blocked && !navigation_active;
        let suppress_release = primary_released && self.suppress_release;
        let mut consumed = self.is_interacting() || suppress_release;
        let confirm_transform = first_pass
            && available
            && (self.has_transform_session() || self.transform_axis.is_some())
            && !suppress_release
            && response.double_clicked_by(PointerButton::Primary)
            && pointer.is_some_and(|point| {
                response.rect.contains(point) && !axis_gizmo::bounds(response.rect).contains(point)
            });
        if confirm_transform {
            // Incomplete numeric input retains its editable buffer and last
            // valid preview; the HUD explains why it cannot be acknowledged.
            if self.numeric_error().is_none() {
                self.confirm()?;
            }
            consumed = true;
        }
        if available
            && !confirm_transform
            && self.numeric.is_none()
            && primary_pressed
            && response.is_pointer_button_down_on()
            && let Some(position) = pointer
            && response.rect.contains(position)
            && !axis_gizmo::bounds(response.rect).contains(position)
        {
            let handle = self.locked_transform_handle(&projection).or_else(|| {
                let mut handles = self.handles(&projection);
                let index = transform_gizmo::pick_handle(&handles, &projection, position)?;
                Some(handles.swap_remove(index))
            });
            if let Some(handle) = handle {
                self.begin_transform(&projection, &handle, position)?;
                consumed = true;
            } else if self.has_transform_session()
                || (self.tool != Tool::View && self.transform_axis.is_some())
            {
                // A pending transform still owns selection when its axis is toggled
                // off, or when the armed axis has no movable selection.
                consumed = true;
            } else if self.edit_mode {
                self.gesture = Some(Gesture::Marquee {
                    start: position,
                    current: position,
                    additive: ui.input(|input| input.modifiers.shift),
                    before: self.selected_vertices.clone(),
                    pending: self.selected_vertices.clone(),
                    policy: self.box_selection,
                    dragged: false,
                });
            } else {
                self.gesture = Some(Gesture::ObjectMarquee {
                    start: position,
                    current: position,
                    additive: ui.input(|input| input.modifiers.shift),
                    before: self.selected_objects.clone(),
                    before_active: self.selected_object,
                    pending: self.selected_objects.clone(),
                    policy: self.box_selection,
                    projection: Box::new(projection.clone()),
                    dragged: false,
                });
            }
        }
        let mut object_marquee = None;
        if first_pass && let Some(position) = pointer {
            match self.gesture.as_mut() {
                Some(Gesture::Marquee {
                    start,
                    current,
                    additive,
                    before,
                    pending,
                    policy,
                    dragged,
                }) => {
                    *current = position;
                    *dragged |= crossed_drag_threshold(*start, position);
                    if *dragged {
                        let rect = marquee::rectangle(*start, position, viewport);
                        let mut selected = if *additive {
                            before.clone()
                        } else {
                            BTreeSet::new()
                        };
                        selected.extend(
                            self.cache
                                .as_ref()
                                .unwrap()
                                .eligible_vertices(self.selection_depth)
                                .filter(|vertex| {
                                    Some(vertex.object) == self.selected_object
                                        && policy.hit.matches(
                                            rect,
                                            Rect::from_min_max(vertex.screen, vertex.screen),
                                        )
                                })
                                .map(|vertex| vertex.id),
                        );
                        *pending = selected;
                        if policy.timing == SelectionTiming::Live {
                            self.selected_vertices = pending.clone();
                        }
                    }
                }
                Some(Gesture::ObjectMarquee {
                    start,
                    current,
                    additive,
                    before,
                    before_active,
                    projection,
                    policy,
                    dragged,
                    ..
                }) => {
                    *current = position;
                    *dragged |= crossed_drag_threshold(*start, position);
                    if *dragged {
                        object_marquee = Some((
                            Rect::from_two_pos(*start, position),
                            *additive,
                            before.clone(),
                            *before_active,
                            projection.clone(),
                            *policy,
                        ));
                    }
                }
                Some(Gesture::Transform(_)) => self.preview_transform(position)?,
                None => {}
            }
        }
        if let Some((rect, additive, before, active, marquee_projection, policy)) = object_marquee {
            let rect = rect.intersect(viewport);
            let mut selected = if additive { before } else { BTreeSet::new() };
            // Visibility and bounds must use the same press-time camera, even
            // if a view transition was already running when the drag began.
            self.prepare(&marquee_projection)?;
            selected.extend(
                self.selection_bounds(&marquee_projection)
                    .into_iter()
                    .filter(|(_, bounds)| policy.hit.matches(rect, *bounds))
                    .map(|(id, _)| id),
            );
            if let Some(Gesture::ObjectMarquee { pending, .. }) = &mut self.gesture {
                *pending = selected.clone();
            }
            if policy.timing == SelectionTiming::Live {
                self.set_object_selection_set(selected, active);
            }
        }
        // Marquee selection also owns clicks. Handle this before its release
        // consumes the double-click, and test scene surfaces rather than just
        // the selected mesh's nearby vertices.
        let selection_enabled =
            !confirm_transform && !self.has_transform_session() && self.transform_axis.is_none();
        if available
            && first_pass
            && selection_enabled
            && !ui.input(|input| input.modifiers.shift)
            && !suppress_release
            && !self.is_transforming()
            && response.double_clicked_by(PointerButton::Primary)
            && let Some(position) = pointer
            && response.rect.contains(position)
            && !axis_gizmo::bounds(response.rect).contains(position)
            && !self
                .handles(&projection)
                .iter()
                .any(|handle| handle.hit(position, &projection))
        {
            let hit = self
                .cache
                .as_ref()
                .unwrap()
                .object_at(position, &projection);
            if self.edit_mode && hit.is_none() {
                self.leave_edit();
                consumed = true;
            } else if !self.edit_mode
                && let Some(hit) = hit
            {
                self.select_object(hit)?;
                if self.can_edit() {
                    self.enter_edit()?;
                }
                consumed = true;
            }
        }
        if primary_released && self.finish_gesture()? {
            consumed = true;
        }
        if primary_released {
            self.suppress_release = false;
        }
        if available
            && first_pass
            && !consumed
            && selection_enabled
            && response.clicked_by(PointerButton::Primary)
            && let Some(position) = pointer
            && !axis_gizmo::bounds(response.rect).contains(position)
        {
            if self.edit_mode {
                self.select_vertex(position, ui.input(|input| input.modifiers.shift));
            } else {
                let selected = self
                    .cache
                    .as_ref()
                    .unwrap()
                    .object_at(position, &projection);
                self.apply_object_click(selected, ui.input(|input| input.modifiers.shift));
            }
        }
        self.prepare(&projection)?;
        let locked_drag =
            matches!(&self.gesture, Some(Gesture::Transform(drag)) if drag.locked_transform);
        if available
            && self.tool == Tool::Move
            && self.transform_axis.is_some()
            && (locked_drag
                || (self.locked_transform_handle(&projection).is_some()
                    && response.contains_pointer()
                    && pointer.is_some_and(|point| {
                        response.rect.contains(point)
                            && !axis_gizmo::bounds(response.rect).contains(point)
                    })))
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
        }
        self.paint(ui, &projection);
        Ok(())
    }

    fn set_vertex_selection(&mut self, vertices: BTreeSet<u64>) {
        if vertices != self.selected_vertices {
            self.transform_axis = None;
            self.last_nudge = None;
        }
        self.selected_vertices = vertices;
    }

    fn select_vertex(&mut self, position: Pos2, additive: bool) {
        let nearest = self
            .cache
            .as_ref()
            .unwrap()
            .eligible_vertices(self.selection_depth)
            .filter(|vertex| Some(vertex.object) == self.selected_object)
            .filter_map(|vertex| {
                let distance = vertex.screen.distance(position);
                (distance <= PICK_RADIUS).then_some((distance, vertex.depth, vertex.id))
            })
            .min_by(|a, b| {
                a.0.total_cmp(&b.0)
                    .then(a.1.total_cmp(&b.1))
                    .then(a.2.cmp(&b.2))
            })
            .map(|(_, _, id)| id);
        let mut selected = if additive {
            self.selected_vertices.clone()
        } else {
            BTreeSet::new()
        };
        if let Some(id) = nearest
            && (!additive || !selected.remove(&id))
        {
            selected.insert(id);
        }
        self.set_vertex_selection(selected);
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            document: self.document.clone(),
            selected_object: self.selected_object,
            selected_objects: self.selected_objects.clone(),
            selected_vertices: self.selected_vertices.clone(),
            edit_mode: self.edit_mode,
            geometry_edit: self.geometry_edit.clone(),
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        if self.selected_objects != snapshot.selected_objects
            || self.selected_object != snapshot.selected_object
            || self.selected_vertices != snapshot.selected_vertices
            || self.edit_mode != snapshot.edit_mode
        {
            self.transform_axis = None;
        }
        let changed = self.document != snapshot.document;
        self.document = snapshot.document;
        self.selected_object = snapshot.selected_object;
        self.selected_objects = snapshot.selected_objects;
        self.selected_vertices = snapshot.selected_vertices;
        self.edit_mode = snapshot.edit_mode;
        self.geometry_edit = snapshot.geometry_edit;
        self.hovered_object = None;
        if self.visible_object_ids.is_some() {
            // History owns document and selection snapshots, not the current
            // viewport isolation. A restored selection must remain in view.
            self.reconcile_selection();
        }
        if changed {
            self.changed();
        }
    }

    fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.cache = None;
        self.hovered_object = None;
    }

    /// Every editing path resolves representation before validating and
    /// comparing the candidate. Conversion cannot become its own history step.
    fn validate_candidate(&self, document: &mut Document) -> Result<(), String> {
        if let Some(edit) = &self.geometry_edit {
            edit.normalize(document)?;
        }
        document.validate()?;
        document.render_mesh_with_assets(&self.frame, &self.asset_frames)?;
        Ok(())
    }

    fn push_undo(&mut self, before: Snapshot) {
        if self.document != before.document {
            self.history.record(before, &self.snapshot());
        }
    }

    fn reconcile_selection(&mut self) {
        let previous_vertices = self.selected_vertices.clone();
        let previous_mode = self.edit_mode;
        let valid: BTreeSet<_> = self
            .document
            .objects
            .iter()
            .filter(|object| self.is_object_visible(object.id))
            .map(|object| object.id)
            .collect();
        if self.selected_objects.iter().any(|id| !valid.contains(id)) {
            self.transform_axis = None;
        }
        self.selected_objects.retain(|id| valid.contains(id));
        self.selected_object = self
            .selected_object
            .filter(|id| self.selected_objects.contains(id))
            .or_else(|| self.selected_objects.first().copied());
        if self.selected_objects.len() != 1 {
            self.selected_vertices.clear();
            self.edit_mode = false;
        }
        let object = self
            .document
            .objects
            .iter()
            .find(|object| Some(object.id) == self.selected_object);
        if let Some(object) = object {
            let mesh = if let Some(edit) = &self.geometry_edit
                && edit.object == object.id
            {
                edit.evaluated(&object.geometry)
            } else {
                geometry_edit::evaluated(&object.geometry)
            };
            if let Ok(mesh) = mesh {
                self.selected_vertices
                    .retain(|id| mesh.vertices.iter().any(|vertex| vertex.id == *id));
            } else {
                self.selected_vertices.clear();
                self.edit_mode = false;
            }
        } else {
            self.set_object_selection(None);
        }
        if !self.edit_mode
            || self
                .geometry_edit
                .as_ref()
                .is_some_and(|edit| Some(edit.object) != self.selected_object)
        {
            self.geometry_edit = None;
        }
        if previous_vertices != self.selected_vertices || previous_mode != self.edit_mode {
            self.transform_axis = None;
            self.last_nudge = None;
        }
    }

    fn prepare(&mut self, projection: &Projection) -> Result<(), String> {
        if self.cache.is_none() {
            self.cache = Some(GeometryCache::with_assets(
                &self.document,
                &self.frame,
                self.visible_object_ids.as_ref(),
                &self.asset_frames,
            )?);
        }
        self.cache.as_mut().unwrap().project(projection);
        Ok(())
    }

    fn selection_pivot(&self) -> Option<DVec3> {
        selection_pivot_in_display(
            self.cache.as_ref()?,
            &self.document,
            &self.frame,
            &self.selected_objects,
            &self.selected_vertices,
            self.edit_mode,
        )
    }

    fn locked_transform_handle(&self, projection: &Projection) -> Option<Handle> {
        if !self.can_edit() {
            return None;
        }
        let index = self
            .transform_axis
            .filter(|axis| *axis < 3 && self.tool != Tool::View)?;
        if self.tool == Tool::Scale && !self.edit_mode && self.selected_objects.len() > 1 {
            return None;
        }
        let pivot = self.selection_pivot()?;
        let mut axis = [DVec3::X, DVec3::Y, DVec3::Z][index];
        let mut rotation = DQuat::IDENTITY;
        if self.tool == Tool::Scale && !self.edit_mode {
            let object = self
                .document
                .objects
                .iter()
                .find(|object| Some(object.id) == self.selected_object)?;
            rotation = DQuat::from_array(object.transform.rotation);
            axis = rotation * axis;
        }
        let center = projection.screen(pivot)?;
        let unit = projection.pixel_size(pivot)?;
        let (direction, _) = projection.axis_direction(pivot, axis)?;
        let start = center;
        let end = center + direction * HANDLE_LENGTH as f32;
        let points = if self.tool == Tool::Rotate {
            let u = axis.any_orthonormal_vector();
            let v = axis.cross(u);
            (0..=64)
                .filter_map(|step| {
                    let angle = step as f64 * std::f64::consts::TAU / 64.0;
                    projection
                        .screen(pivot + (u * angle.cos() + v * angle.sin()) * unit * HANDLE_LENGTH)
                })
                .collect()
        } else {
            vec![start, end]
        };
        let target = points
            .iter()
            .copied()
            .find(|point| projection.viewport.contains(*point))
            .unwrap_or(end);
        Some(
            Handle {
                kind: HandleKind::Axis(index),
                pivot,
                axis,
                normal: (self.tool == Tool::Rotate).then_some(axis),
                length: unit * HANDLE_LENGTH,
                points,
                target: Rect::from_center_size(
                    if self.tool == Tool::Rotate {
                        target
                    } else {
                        end
                    },
                    egui::Vec2::splat(10.0),
                ),
                filled: false,
                endpoint: None,
                shaft: None,
            }
            .with_endpoint(projection, self.tool, rotation, {
                // A locked axis retains its fixed screen-length affordance, including
                // the depth-axis fallback. Unproject that anchor so solid and target agree.
                let clip = projection.matrix * pivot.extend(1.0);
                [
                    projection.unproject(start, clip.z / clip.w)?,
                    projection.unproject(end, clip.z / clip.w)?,
                ]
            }),
        )
    }

    fn handles(&self, projection: &Projection) -> Vec<Handle> {
        if !self.can_edit() {
            return Vec::new();
        }
        if self.tool != Tool::View && self.transform_axis.is_some() {
            return self
                .locked_transform_handle(projection)
                .into_iter()
                .filter_map(|mut handle| {
                    handle.target = handle.target.intersect(projection.viewport);
                    handle.target.is_positive().then_some(handle)
                })
                .collect();
        }
        if self.tool == Tool::View {
            return Vec::new();
        }
        let Some(object) = self
            .document
            .objects
            .iter()
            .find(|object| Some(object.id) == self.selected_object)
        else {
            return Vec::new();
        };
        let Some(pivot) = self.selection_pivot() else {
            return Vec::new();
        };
        let group = !self.edit_mode && self.selected_objects.len() > 1;
        let Some(center) = projection.screen(pivot) else {
            return Vec::new();
        };
        let Some(unit) = projection.pixel_size(pivot) else {
            return Vec::new();
        };
        let length = unit * HANDLE_LENGTH;
        if group && self.tool == Tool::Scale {
            // A group can contain differently rotated objects. World-axis
            // nonuniform scaling would require shear, which our TRS document
            // cannot represent. Offer only an explicit uniform scale handle.
            let clip = projection.matrix * pivot.extend(1.0);
            let Some(diagonal) =
                projection.unproject(center + egui::vec2(1.0, -1.0), clip.z / clip.w)
            else {
                return Vec::new();
            };
            let axis = (diagonal - pivot).normalize_or_zero();
            let (Some(start), Some(end)) = (
                projection.screen(pivot),
                projection.screen(pivot + axis * length),
            ) else {
                return Vec::new();
            };
            if start.distance(end) < 12.0
                || !projection.viewport.contains(end)
                || axis_gizmo::bounds(projection.viewport).contains(end)
            {
                return Vec::new();
            }
            return vec![
                Handle {
                    kind: HandleKind::Uniform,
                    pivot,
                    axis,
                    normal: None,
                    length,
                    points: vec![start, end],
                    target: Rect::from_center_size(end, egui::Vec2::splat(10.0)),
                    filled: false,
                    endpoint: None,
                    shaft: None,
                }
                .with_endpoint(
                    projection,
                    self.tool,
                    DQuat::IDENTITY,
                    [pivot, pivot + axis * length],
                ),
            ];
        }
        let rotation = if self.tool == Tool::Scale && !self.edit_mode {
            DQuat::from_array(object.transform.rotation)
        } else {
            DQuat::IDENTITY
        };
        let axes = [DVec3::X, DVec3::Y, DVec3::Z].map(|axis| rotation * axis);
        let mut handles = Vec::new();
        for (index, axis) in axes.into_iter().enumerate() {
            if self.tool == Tool::Rotate {
                let u = axis.any_orthonormal_vector();
                let v = axis.cross(u);
                let points: Vec<_> = (0..=64)
                    .filter_map(|step| {
                        let angle = step as f64 * std::f64::consts::TAU / 64.0;
                        projection.screen(pivot + (u * angle.cos() + v * angle.sin()) * length)
                    })
                    .collect();
                if points.len() != 65 {
                    continue;
                }
                // A ring is manipulable only when its plane has a stable intersection.
                let Some(ray) = projection.ray(center) else {
                    continue;
                };
                if ray.direction.dot(axis).abs() < 0.025 {
                    continue;
                }
                let target = points
                    .iter()
                    .copied()
                    .max_by(|a, b| a.distance(center).total_cmp(&b.distance(center)))
                    .unwrap();
                handles.push(Handle {
                    kind: HandleKind::Axis(index),
                    pivot,
                    axis,
                    normal: Some(axis),
                    length,
                    points,
                    target: Rect::from_center_size(target, egui::Vec2::splat(10.0)),
                    filled: false,
                    endpoint: None,
                    shaft: None,
                });
            } else if let (Some(start), Some(end)) = (
                projection.screen(pivot),
                projection.screen(pivot + axis * length),
            ) {
                if start.distance(end) < 12.0 {
                    continue;
                }
                handles.push(
                    Handle {
                        kind: HandleKind::Axis(index),
                        pivot,
                        axis,
                        normal: None,
                        length,
                        points: vec![start, end],
                        target: Rect::from_center_size(end, egui::Vec2::splat(10.0)),
                        filled: false,
                        endpoint: None,
                        shaft: None,
                    }
                    .with_endpoint(
                        projection,
                        self.tool,
                        rotation,
                        [pivot, pivot + axis * length],
                    ),
                );
            }
        }
        if self.tool == Tool::Move {
            for (a, b) in [(0, 1), (0, 2), (1, 2)] {
                let normal = axes[a].cross(axes[b]);
                let Some(ray) = projection.ray(center) else {
                    continue;
                };
                if ray.direction.dot(normal).abs() < 0.025 {
                    continue;
                }
                let points: Vec<_> = [(0.22, 0.22), (0.40, 0.22), (0.40, 0.40), (0.22, 0.40)]
                    .into_iter()
                    .filter_map(|(x, y)| {
                        projection.screen(pivot + (axes[a] * x + axes[b] * y) * length)
                    })
                    .collect();
                if points.len() != 4 {
                    continue;
                }
                let middle = points
                    .iter()
                    .map(|point| point.to_vec2())
                    .fold(egui::Vec2::ZERO, |sum, value| sum + value)
                    / 4.0;
                handles.push(Handle {
                    kind: HandleKind::Plane(a, b),
                    pivot,
                    axis: axes[a],
                    normal: Some(normal),
                    length,
                    points,
                    target: Rect::from_center_size(middle.to_pos2(), egui::Vec2::splat(8.0)),
                    filled: true,
                    endpoint: None,
                    shaft: None,
                });
            }
        }
        // Plane handles have a smaller, explicit hit region; give them priority.
        handles.sort_by_key(|handle| !handle.filled);
        let targets: Vec<_> = handles
            .iter()
            .enumerate()
            .map(|(index, handle)| {
                // Recorded targets obey the same picking priority as real presses.
                // In particular, crossing rotation rings cannot share a claimed target.
                std::iter::once(handle.target.center())
                    .chain(handle.points.iter().copied())
                    .find(|point| {
                        projection.viewport.contains(*point)
                            && !axis_gizmo::bounds(projection.viewport).contains(*point)
                            && transform_gizmo::pick_handle(&handles, projection, *point)
                                == Some(index)
                    })
            })
            .collect();
        let mut reachable = Vec::new();
        for (mut handle, target) in handles.into_iter().zip(targets) {
            if let Some(target) = target {
                handle.target = Rect::from_center_size(target, handle.target.size());
                reachable.push(handle);
            }
        }
        reachable
    }

    fn pointer_snapping(
        &self,
        projection: &Projection,
        pivot: DVec3,
    ) -> Result<SnapSettings, String> {
        // This is camera-plane scale at the manipulation depth, in logical UI
        // points. Dividing by a foreshortened axis instead would explode near
        // an end-on view. Source-up display rotation does not change lengths.
        let cm_per_point = projection.pixel_size(pivot).unwrap_or(f64::NAN) / self.frame.scale;
        self.snap_policy.resolve(self.snapping, cm_per_point)
    }

    /// Show the captured increment during an interaction and the next gesture's
    /// increment when idle. This query never changes document or history state.
    pub fn movement_snap_step(
        &mut self,
        viewport: Rect,
        camera: &Camera,
        z_up: bool,
    ) -> Option<f64> {
        if self.numeric.is_some() {
            return None;
        }
        if let Some(Gesture::Transform(drag)) = &self.gesture {
            return (drag.tool == Tool::Move && drag.snapping.enabled)
                .then_some(drag.snapping.step_cm);
        }
        if self.property_transaction && self.property_movement {
            return self
                .property_snapping
                .enabled
                .then_some(self.property_snapping.step_cm);
        }
        if !self.snapping.enabled
            || self.selected_objects.is_empty()
            || (self.edit_mode && self.selected_vertices.is_empty())
        {
            return None;
        }
        let projection = Projection::new(viewport, camera, z_up).ok()?;
        if self.cache.is_none() {
            self.cache = Some(
                GeometryCache::with_assets(
                    &self.document,
                    &self.frame,
                    self.visible_object_ids.as_ref(),
                    &self.asset_frames,
                )
                .ok()?,
            );
        }
        let pivot = self.selection_pivot()?;
        self.pointer_snapping(&projection, pivot)
            .ok()
            .map(|settings| settings.step_cm)
    }

    fn begin_transform(
        &mut self,
        projection: &Projection,
        handle: &Handle,
        pointer: Pos2,
    ) -> Result<(), String> {
        self.require_write()?;
        if self.numeric.is_some() {
            return Err("Apply or cancel the numeric transform before dragging.".into());
        }
        let ray = projection
            .ray(pointer)
            .ok_or("Cannot create a pointer ray.")?;
        self.last_nudge = None;
        let locked_transform = self.tool != Tool::View
            && matches!((self.transform_axis, handle.kind),
                (Some(locked), HandleKind::Axis(axis)) if locked == axis);
        let parallel = locked_transform
            && projection
                .axis_direction(handle.pivot, handle.axis)
                .is_some_and(|(_, parallel)| parallel);
        let constrained = constraint_point(ray, handle.pivot, handle.axis, handle.normal);
        let screen_rotation = (locked_transform && self.tool == Tool::Rotate).then_some(pointer);
        let screen_axis =
            if locked_transform && self.tool != Tool::Rotate && (parallel || constrained.is_none())
            {
                Some((
                    pointer,
                    projection
                        .pixel_size(handle.pivot)
                        .ok_or("Cannot determine the current Move scale.")?,
                ))
            } else {
                None
            };
        let start = if screen_axis.is_some() || screen_rotation.is_some() {
            handle.pivot
        } else {
            constrained
                .ok_or("This transform handle is parallel to the view. Choose another view.")?
        };
        let snapping = if self.tool == Tool::Move {
            self.pointer_snapping(projection, handle.pivot)?
        } else {
            self.snapping
        };
        let session_transform = locked_transform || self.has_transform_session();
        if session_transform && !self.has_transform_session() {
            self.history.begin_transaction(self.snapshot());
        }
        self.gesture = Some(Gesture::Transform(Box::new(TransformDrag {
            before: self.snapshot(),
            snapping,
            projection: projection.clone(),
            pivot: handle.pivot,
            axis: handle.axis,
            plane_normal: handle.normal,
            start,
            length: handle.length,
            kind: handle.kind,
            tool: self.tool,
            locked_transform,
            session_transform,
            pointer_start: pointer,
            dragged: false,
            screen_axis,
            screen_rotation,
        })));
        Ok(())
    }

    fn preview_transform(&mut self, pointer: Pos2) -> Result<(), String> {
        self.require_write()?;
        if self.numeric.is_some() {
            return Ok(());
        }
        let Some(Gesture::Transform(drag)) = &mut self.gesture else {
            return Ok(());
        };
        if drag.session_transform {
            drag.dragged |= crossed_drag_threshold(drag.pointer_start, pointer);
            if !drag.dragged {
                return Ok(());
            }
        }
        let current = if let Some(start) = drag.screen_rotation {
            Some(drag.start + drag.axis * f64::from((pointer.x - start.x) - (pointer.y - start.y)))
        } else if let Some((start, unit)) = drag.screen_axis {
            Some(drag.start + drag.axis * (f64::from(start.y) - f64::from(pointer.y)) * unit)
        } else {
            drag.projection
                .ray(pointer)
                .and_then(|ray| constraint_point(ray, drag.pivot, drag.axis, drag.plane_normal))
        };
        let Some(current) = current else {
            return Ok(());
        };
        // A press without movement (or a return to the exact starting constraint)
        // must not acquire floating-point roundoff or create an undo entry.
        if current.abs_diff_eq(drag.start, 1e-12) {
            let original = drag.before.document.clone();
            if self.document != original {
                self.document = original;
                self.changed();
            }
            return Ok(());
        }
        let mut candidate = drag.before.document.clone();
        let (delta, rotation, factor) = match drag.tool {
            Tool::Move => (current - drag.start, DQuat::IDENTITY, 1.0),
            Tool::Rotate => {
                if let Some(start) = drag.screen_rotation {
                    let angle =
                        f64::from((pointer.x - start.x) - (pointer.y - start.y)) / HANDLE_LENGTH;
                    (DVec3::ZERO, DQuat::from_axis_angle(drag.axis, angle), 1.0)
                } else {
                    let a = (drag.start - drag.pivot).normalize_or_zero();
                    let b = (current - drag.pivot).normalize_or_zero();
                    if a.length_squared() < 0.5 || b.length_squared() < 0.5 {
                        return Ok(());
                    }
                    let angle = drag.axis.dot(a.cross(b)).atan2(a.dot(b));
                    (DVec3::ZERO, DQuat::from_axis_angle(drag.axis, angle), 1.0)
                }
            }
            Tool::Scale => (
                DVec3::ZERO,
                DQuat::IDENTITY,
                (1.0 + (current - drag.start).dot(drag.axis) / drag.length).clamp(0.001, 1000.0),
            ),
            Tool::View => return Ok(()),
        };
        if drag.tool == Tool::Move {
            // Translate from the original canonical coordinates so unaffected
            // components do not acquire a display-frame round trip.
            let axes = match drag.kind {
                HandleKind::Axis(axis) => std::array::from_fn(|i| i == axis),
                HandleKind::Plane(a, b) => std::array::from_fn(|i| i == a || i == b),
                HandleKind::Uniform => [true; 3],
            };
            translate_selection(
                &mut candidate,
                &drag.before,
                delta / self.frame.scale,
                axes,
                drag.snapping,
                TranslationSource::Interactive,
            )?;
        } else {
            SelectionTransform {
                tool: drag.tool,
                kind: drag.kind,
                pivot: self.frame.display_to_world(drag.pivot),
                axis: drag.axis,
                rotation,
                factor,
            }
            .apply(&mut candidate, &drag.before)?;
        }
        self.validate_candidate(&mut candidate)?;
        if candidate != self.document {
            self.document = candidate;
            self.changed();
        }
        Ok(())
    }

    fn paint(&self, ui: &egui::Ui, projection: &Projection) {
        let painter = ui.painter().with_clip_rect(projection.viewport);
        if self.edit_mode {
            // Draw occluded markers first so an overlapping front marker wins.
            let vertices = [true, false].into_iter().flat_map(|occluded| {
                self.cache
                    .as_ref()
                    .unwrap()
                    .eligible_vertices(self.selection_depth)
                    .filter(move |vertex| vertex.occluded == occluded)
            });
            for vertex in vertices {
                if Some(vertex.object) != self.selected_object {
                    continue;
                }
                let selected = self.selected_vertices.contains(&vertex.id);
                let radius = if selected {
                    SELECTED_VERTEX_RADIUS
                } else {
                    VERTEX_RADIUS
                };
                // Rear markers remain readable through the surface without
                // competing with front markers. Selection stays warm orange.
                let opacity = if vertex.occluded {
                    if selected { 0.7 } else { 0.45 }
                } else {
                    1.0
                };
                painter.circle_filled(
                    vertex.screen,
                    radius,
                    (if selected {
                        edit_feedback::SELECTED
                    } else {
                        edit_feedback::UNSELECTED
                    })
                    .gamma_multiply(opacity),
                );
                painter.circle_stroke(
                    vertex.screen,
                    radius,
                    Stroke::new(0.75, edit_feedback::VERTEX_BORDER.gamma_multiply(opacity)),
                );
            }
        }
        if let Some(handle) = self.locked_transform_handle(projection)
            && let Some(center) = projection.screen(handle.pivot)
            && let Some((direction, depth)) = projection.axis_direction(handle.pivot, handle.axis)
            && clipped_axis_line(projection.viewport, center, direction).is_some()
        {
            let axis = self.transform_axis.unwrap();
            let label = ["X", "Y", "Z"][axis];
            painter.text(
                center + egui::vec2(9.0, 9.0),
                egui::Align2::LEFT_TOP,
                if depth {
                    format!("{label} · depth")
                } else {
                    label.to_owned()
                },
                egui::FontId::proportional(theme::text::UI_BODY_13),
                COLORS[axis],
            );
        }
        for handle in self.handles(projection) {
            let color = match handle.kind {
                HandleKind::Axis(axis) => COLORS[axis],
                // The plane takes the color of its normal axis: XY is blue,
                // XZ is green, and YZ is red.
                HandleKind::Plane(a, b) => COLORS[3 - a - b],
                HandleKind::Uniform => crate::object_feedback::SELECTED_COLOR,
            };
            if handle.filled {
                painter.add(egui::Shape::convex_polygon(
                    handle.points.clone(),
                    color.gamma_multiply(0.25),
                    Stroke::new(1.5, color),
                ));
            } else if self.tool == Tool::Rotate {
                painter.add(egui::Shape::line(
                    handle.points.clone(),
                    Stroke::new(2.5, color),
                ));
            }
            let control = handle.kind.control();
            controls::record(ui.ctx(), control, control.label(), handle.target, true);
        }
        let marquee = match &self.gesture {
            Some(
                Gesture::Marquee { start, current, .. }
                | Gesture::ObjectMarquee { start, current, .. },
            ) => Some((*start, *current)),
            _ => None,
        };
        if let Some((start, current)) = marquee
            && crossed_drag_threshold(start, current)
        {
            marquee::paint(
                &painter,
                marquee::rectangle(start, current, projection.viewport),
            );
        }
    }
}

/// Shared rotation/scale geometry path for pointer and exact numeric input.
/// Coordinates and pivot are canonical world centimeters, avoiding repeated
/// normalization round trips. Object axis scaling retains local TRS semantics.
struct SelectionTransform {
    tool: Tool,
    kind: HandleKind,
    pivot: DVec3,
    axis: DVec3,
    rotation: DQuat,
    factor: f64,
}

impl SelectionTransform {
    fn point(&self, point: DVec3) -> DVec3 {
        if self.tool == Tool::Rotate {
            self.pivot + self.rotation * (point - self.pivot)
        } else if self.kind == HandleKind::Uniform {
            self.pivot + (point - self.pivot) * self.factor
        } else {
            point + self.axis * ((point - self.pivot).dot(self.axis) * (self.factor - 1.0))
        }
    }

    fn apply(&self, document: &mut Document, selected: &Snapshot) -> Result<(), String> {
        if self.tool == Tool::Scale
            && !selected.edit_mode
            && selected.selected_objects.len() > 1
            && self.kind != HandleKind::Uniform
        {
            return Err("Multiple objects support uniform scaling only.".into());
        }
        if self.tool == Tool::Scale && (!self.factor.is_finite() || self.factor <= 0.0) {
            return Err("A scale multiplier must be finite and greater than zero.".into());
        }
        if !self.rotation.is_finite() || !self.pivot.is_finite() || !self.axis.is_finite() {
            return Err("The transform exceeds the supported numeric range.".into());
        }
        // Identity previews preserve the original bits, including transformed
        // vertices, and never create an otherwise empty history entry.
        if (self.tool == Tool::Rotate && self.rotation == DQuat::IDENTITY)
            || (self.tool == Tool::Scale && self.factor == 1.0)
        {
            return Ok(());
        }
        if selected.edit_mode {
            let object = document
                .objects
                .iter_mut()
                .find(|object| Some(object.id) == selected.selected_object)
                .ok_or("The edited object no longer exists.")?;
            let model = object.transform.matrix();
            let inverse = model.inverse();
            let mesh = geometry_edit::editable(&mut object.geometry)?;
            for vertex in &mut mesh.vertices {
                if selected.selected_vertices.contains(&vertex.id) {
                    vertex.position = inverse
                        .transform_point3(
                            self.point(model.transform_point3(DVec3::from_array(vertex.position))),
                        )
                        .to_array();
                }
            }
        } else {
            for object in document
                .objects
                .iter_mut()
                .filter(|object| selected.selected_objects.contains(&object.id))
            {
                object.transform.translation = self
                    .point(DVec3::from_array(object.transform.translation))
                    .to_array();
                if self.tool == Tool::Rotate {
                    object.transform.rotation = (self.rotation
                        * DQuat::from_array(object.transform.rotation))
                    .normalize()
                    .to_array();
                } else {
                    match self.kind {
                        HandleKind::Uniform => {
                            object.transform.scale = object
                                .transform
                                .scale
                                .map(|component| component * self.factor)
                        }
                        HandleKind::Axis(axis) => object.transform.scale[axis] *= self.factor,
                        HandleKind::Plane(_, _) => {
                            return Err("Plane scaling is unsupported.".into());
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// Pointer handles and numeric input use one pivot definition. Compute it in
/// normalized display coordinates first: summing large absolute world positions
/// can overflow even when each authored vertex and the resulting center fit.
fn selection_pivot_in_display(
    cache: &GeometryCache,
    document: &Document,
    frame: &DisplayFrame,
    selected_objects: &BTreeSet<u64>,
    selected_vertices: &BTreeSet<u64>,
    edit_mode: bool,
) -> Option<DVec3> {
    let mut placements_without_samples = selected_objects.clone();
    let mut points = Vec::new();
    for vertex in &cache.vertices {
        if selected_objects.contains(&vertex.object)
            && (!edit_mode || selected_vertices.contains(&vertex.id))
        {
            points.push(vertex.position);
            placements_without_samples.remove(&vertex.object);
        }
    }
    if !edit_mode {
        // Each missing or geometry-empty placement contributes its origin to
        // group transforms, without adding selectable proxy geometry.
        points.extend(
            document
                .objects
                .iter()
                .filter(|object| placements_without_samples.contains(&object.id))
                .map(|object| {
                    frame.world_to_display(DVec3::from_array(object.transform.translation))
                }),
        );
    }
    if points.is_empty() {
        return None;
    }
    Some(
        if !edit_mode
            && (selected_objects.len() > 1
                || document.objects.iter().any(|object| {
                    selected_objects.contains(&object.id)
                        && matches!(object.geometry, Geometry::Asset(_))
                }))
        {
            let minimum = points
                .iter()
                .copied()
                .fold(DVec3::splat(f64::INFINITY), DVec3::min);
            let maximum = points
                .iter()
                .copied()
                .fold(DVec3::splat(f64::NEG_INFINITY), DVec3::max);
            minimum + (maximum - minimum) * 0.5
        } else {
            points.iter().copied().sum::<DVec3>() / points.len() as f64
        },
    )
}

fn snapshot_pivot(
    selected: &Snapshot,
    frame: &DisplayFrame,
    assets: &AssetFrames,
) -> Result<DVec3, String> {
    let cache = GeometryCache::with_assets(&selected.document, frame, None, assets)?;
    let display = selection_pivot_in_display(
        &cache,
        &selected.document,
        frame,
        &selected.selected_objects,
        &selected.selected_vertices,
        selected.edit_mode,
    )
    .ok_or("Select geometry before transforming it.")?;
    let world = frame.display_to_world(display);
    if !world.is_finite() {
        return Err("The transform pivot exceeds the supported numeric range.".into());
    }
    Ok(world)
}

fn translate_selection(
    document: &mut Document,
    selected: &Snapshot,
    raw_delta: DVec3,
    axes: [bool; 3],
    snapping: SnapSettings,
    source: TranslationSource,
) -> Result<(), String> {
    let Some(anchor) = translation_anchor(selected)? else {
        return Ok(());
    };
    let resolution = snapping.resolve_translation(anchor, raw_delta, axes, source)?;
    let delta = resolution.delta;
    if delta == DVec3::ZERO {
        return Ok(());
    }
    if selected.edit_mode {
        if selected.selected_vertices.is_empty() {
            return Ok(());
        }
        let object = document
            .objects
            .iter_mut()
            .find(|object| Some(object.id) == selected.selected_object)
            .ok_or("The edited object no longer exists.")?;
        let local_delta = object.transform.matrix().inverse().transform_vector3(delta);
        let mesh = geometry_edit::editable(&mut object.geometry)?;
        for vertex in &mut mesh.vertices {
            if selected.selected_vertices.contains(&vertex.id) {
                vertex.position = (DVec3::from_array(vertex.position) + local_delta).to_array();
            }
        }
    } else {
        for object in document
            .objects
            .iter_mut()
            .filter(|object| selected.selected_objects.contains(&object.id))
        {
            // The active origin is authoritative. Other members receive the
            // same displacement, preserving group offsets rather than rounding
            // their independent coordinates and changing the arrangement.
            object.transform.translation = if Some(object.id) == selected.selected_object {
                resolution.target
            } else {
                DVec3::from_array(object.transform.translation) + delta
            }
            .to_array();
        }
    }
    Ok(())
}

/// Select one anchor in canonical world centimeters, before display-frame or
/// source-up-axis conversion. A vertex edit snaps its shared center, never its
/// individual vertices; this preserves topology and the shape of the selection.
fn translation_anchor(selected: &Snapshot) -> Result<Option<DVec3>, String> {
    let Some(id) = selected.selected_object else {
        return Ok(None);
    };
    let object = selected
        .document
        .objects
        .iter()
        .find(|object| object.id == id)
        .ok_or("The selected object no longer exists.")?;
    if !selected.edit_mode {
        return Ok(Some(DVec3::from_array(object.transform.translation)));
    }
    if selected.selected_vertices.is_empty() {
        return Ok(None);
    }
    let mesh = geometry_edit::evaluated(&object.geometry)?;
    let model = object.transform.matrix();
    let mut center = DVec3::ZERO;
    let mut count = 0;
    for vertex in mesh
        .vertices
        .iter()
        .filter(|vertex| selected.selected_vertices.contains(&vertex.id))
    {
        count += 1;
        let point = model.transform_point3(DVec3::from_array(vertex.position));
        center += (point - center) / f64::from(count);
    }
    Ok((count != 0).then_some(center))
}

fn clipped_axis_line(viewport: Rect, point: Pos2, direction: egui::Vec2) -> Option<[Pos2; 2]> {
    let mut low = f64::NEG_INFINITY;
    let mut high = f64::INFINITY;
    for (origin, delta, minimum, maximum) in [
        (point.x, direction.x, viewport.left(), viewport.right()),
        (point.y, direction.y, viewport.top(), viewport.bottom()),
    ] {
        if delta.abs() < 1e-8 {
            if origin < minimum || origin > maximum {
                return None;
            }
        } else {
            let a = f64::from(minimum - origin) / f64::from(delta);
            let b = f64::from(maximum - origin) / f64::from(delta);
            low = low.max(a.min(b));
            high = high.min(a.max(b));
        }
    }
    if !low.is_finite() || !high.is_finite() || low > high {
        return None;
    }
    Some([
        point + direction * low as f32,
        point + direction * high as f32,
    ])
}

fn cycle_index(count: usize, anchor: Option<usize>, reverse: bool) -> usize {
    match (anchor, reverse) {
        (Some(index), true) => (index + count - 1) % count,
        (Some(index), false) => (index + 1) % count,
        (None, true) => count - 1,
        (None, false) => 0,
    }
}

struct Handle {
    kind: HandleKind,
    pivot: DVec3,
    axis: DVec3,
    normal: Option<DVec3>,
    length: f64,
    points: Vec<Pos2>,
    target: Rect,
    filled: bool,
    endpoint: Option<Endpoint>,
    shaft: Option<[DVec3; 2]>,
}

#[derive(Clone, PartialEq)]
struct Projection {
    matrix: DMat4,
    inverse: DMat4,
    viewport: Rect,
}

impl Projection {
    fn new(viewport: Rect, camera: &Camera, z_up: bool) -> Result<Self, String> {
        if !viewport.is_finite() || !viewport.is_positive() {
            return Err("The viewport has no usable size.".into());
        }
        let matrix =
            (camera.view_projection(viewport.aspect_ratio()) * display_rotation(z_up)).as_dmat4();
        let inverse = matrix.inverse();
        if !inverse.is_finite() {
            return Err("The camera projection cannot be inverted.".into());
        }
        Ok(Self {
            matrix,
            inverse,
            viewport,
        })
    }

    fn screen(&self, point: DVec3) -> Option<Pos2> {
        let clip = self.matrix * point.extend(1.0);
        if !clip.is_finite() || clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        if !(0.0..=1.0).contains(&ndc.z) {
            return None;
        }
        Some(egui::pos2(
            (f64::from(self.viewport.left())
                + (ndc.x + 1.0) * 0.5 * f64::from(self.viewport.width())) as f32,
            (f64::from(self.viewport.top())
                + (1.0 - ndc.y) * 0.5 * f64::from(self.viewport.height())) as f32,
        ))
    }

    fn segment(&self, points: [DVec3; 2]) -> Option<ProjectedSegment> {
        let clips = points.map(|point| self.matrix * point.extend(1.0));
        if clips.iter().any(|clip| !clip.is_finite()) {
            return None;
        }
        // Clip in homogeneous space so edges crossing the near/far planes
        // remain pickable only along the part the renderer can display.
        let mut start: f64 = 0.0;
        let mut end: f64 = 1.0;
        for [a, b] in [
            [clips[0].z, clips[1].z],
            [clips[0].w - clips[0].z, clips[1].w - clips[1].z],
        ] {
            if a < 0.0 && b < 0.0 {
                return None;
            }
            if a < 0.0 {
                start = start.max(-a / (b - a));
            }
            if b < 0.0 {
                end = end.min(-a / (b - a));
            }
        }
        if start > end {
            return None;
        }
        let points = [
            points[0].lerp(points[1], start),
            points[0].lerp(points[1], end),
        ];
        let clips = points.map(|point| self.matrix * point.extend(1.0));
        if clips.iter().any(|clip| clip.w <= 0.0) {
            return None;
        }
        let ndc = clips.map(|clip| clip.truncate() / clip.w);
        let screen = ndc.map(|point| {
            egui::pos2(
                (f64::from(self.viewport.left())
                    + (point.x + 1.0) * 0.5 * f64::from(self.viewport.width()))
                    as f32,
                (f64::from(self.viewport.top())
                    + (1.0 - point.y) * 0.5 * f64::from(self.viewport.height()))
                    as f32,
            )
        });
        Some(ProjectedSegment {
            points,
            screen,
            reciprocal_w: clips.map(|clip| 1.0 / clip.w),
            depth: ndc.map(|point| point.z),
        })
    }

    fn unproject(&self, point: Pos2, depth: f64) -> Option<DVec3> {
        let x = 2.0 * (f64::from(point.x) - f64::from(self.viewport.left()))
            / f64::from(self.viewport.width())
            - 1.0;
        let y = 1.0
            - 2.0 * (f64::from(point.y) - f64::from(self.viewport.top()))
                / f64::from(self.viewport.height());
        self.unproject_ndc(DVec3::new(x, y, depth))
    }

    fn unproject_ndc(&self, point: DVec3) -> Option<DVec3> {
        let world = self.inverse * point.extend(1.0);
        if !world.is_finite() || world.w.abs() < 1e-15 {
            return None;
        }
        Some(world.truncate() / world.w)
    }

    fn ray(&self, point: Pos2) -> Option<Ray> {
        let origin = self.unproject(point, 0.0)?;
        let direction = (self.unproject(point, 1.0)? - origin).try_normalize()?;
        Some(Ray { origin, direction })
    }

    fn pixel_size(&self, point: DVec3) -> Option<f64> {
        let clip = self.matrix * point.extend(1.0);
        if !clip.is_finite() || clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        let next = self.unproject_ndc(ndc + DVec3::X * (2.0 / f64::from(self.viewport.width())))?;
        let size = (next - point).length();
        (size.is_finite() && size > 0.0).then_some(size)
    }

    fn axis_direction(&self, point: DVec3, axis: DVec3) -> Option<(egui::Vec2, bool)> {
        let clip = self.matrix * point.extend(1.0);
        let delta = self.matrix * axis.extend(0.0);
        if !clip.is_finite() || clip.w <= 0.0 {
            return None;
        }
        let derivative =
            (delta.truncate() * clip.w - clip.truncate() * delta.w) / (clip.w * clip.w);
        let screen = egui::vec2(
            (derivative.x * f64::from(self.viewport.width()) * 0.5) as f32,
            (-derivative.y * f64::from(self.viewport.height()) * 0.5) as f32,
        );
        let unit = self.pixel_size(point)?;
        if !screen.is_finite() {
            return None;
        }
        let parallel = f64::from(screen.length()) * unit < 0.025;
        Some((
            if parallel {
                -egui::Vec2::Y
            } else {
                screen.normalized()
            },
            parallel,
        ))
    }

    fn ray_through(&self, point: DVec3) -> Option<Ray> {
        let clip = self.matrix * point.extend(1.0);
        if !clip.is_finite() || clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        // Visibility must not round a known vertex through a screen-space f32
        // position: that can move its ray just outside a silhouette's bounds.
        let origin = self.unproject_ndc(DVec3::new(ndc.x, ndc.y, 0.0))?;
        let far = self.unproject_ndc(DVec3::new(ndc.x, ndc.y, 1.0))?;
        Some(Ray {
            origin,
            direction: (far - origin).try_normalize()?,
        })
    }
}

#[derive(Clone, Copy)]
struct Ray {
    origin: DVec3,
    direction: DVec3,
}

struct ProjectedSegment {
    points: [DVec3; 2],
    screen: [Pos2; 2],
    reciprocal_w: [f64; 2],
    depth: [f64; 2],
}

impl ProjectedSegment {
    fn nearest(&self, position: Pos2) -> (f32, DVec3) {
        let delta = self.screen[1] - self.screen[0];
        let length_squared = delta.length_sq();
        let fraction = if length_squared > f32::EPSILON {
            ((position - self.screen[0]).dot(delta) / length_squared).clamp(0.0, 1.0)
        } else if self.depth[0] <= self.depth[1] {
            0.0
        } else {
            1.0
        };
        let distance = position.distance(self.screen[0] + delta * fraction);
        // Screen-linear interpolation needs perspective correction to locate
        // the actual edge point for its depth and surface-occlusion query.
        let fraction = f64::from(fraction);
        let weight = fraction * self.reciprocal_w[1];
        let fraction = weight / ((1.0 - fraction) * self.reciprocal_w[0] + weight);
        (distance, self.points[0].lerp(self.points[1], fraction))
    }
}

fn constraint_point(ray: Ray, pivot: DVec3, axis: DVec3, normal: Option<DVec3>) -> Option<DVec3> {
    if let Some(normal) = normal {
        let denominator = ray.direction.dot(normal);
        if denominator.abs() < 1e-8 {
            return None;
        }
        let distance = (pivot - ray.origin).dot(normal) / denominator;
        (distance >= 0.0 && distance.is_finite()).then_some(ray.origin + ray.direction * distance)
    } else {
        let dot = ray.direction.dot(axis);
        let denominator = 1.0 - dot * dot;
        if denominator < 1e-8 {
            return None;
        }
        let offset = ray.origin - pivot;
        let distance = (offset.dot(axis) - dot * offset.dot(ray.direction)) / denominator;
        distance.is_finite().then_some(pivot + axis * distance)
    }
}

struct WorldVertex {
    object: u64,
    id: u64,
    position: DVec3,
}
struct ProjectedVertex {
    object: u64,
    id: u64,
    screen: Pos2,
    depth: f64,
    occluded: bool,
}
struct WorldEdge {
    object: u64,
    points: [DVec3; 2],
}
struct GeometryCache {
    vertices: Vec<WorldVertex>,
    selectable_vertices: BTreeSet<(u64, u64)>,
    loose_edges: Vec<WorldEdge>,
    bvh: Bvh,
    projection: Option<Projection>,
    projected: Vec<ProjectedVertex>,
}

impl GeometryCache {
    #[cfg(test)]
    fn new(
        document: &Document,
        frame: &DisplayFrame,
        visible_objects: Option<&BTreeSet<u64>>,
    ) -> Result<Self, String> {
        Self::with_assets(document, frame, visible_objects, &AssetFrames::new())
    }

    fn with_assets(
        document: &Document,
        frame: &DisplayFrame,
        visible_objects: Option<&BTreeSet<u64>>,
        assets: &AssetFrames,
    ) -> Result<Self, String> {
        crate::model::asset_geometry::validate_budget(document, assets)?;
        let mut vertices = Vec::new();
        let mut selectable_vertices = BTreeSet::new();
        let mut loose_edges = Vec::new();
        let mut triangles = Vec::new();
        for object in &document.objects {
            if visible_objects.is_some_and(|visible| !visible.contains(&object.id)) {
                continue;
            }
            if let Geometry::Asset(reference) = &object.geometry {
                let Some(evaluated) = assets.get(reference) else {
                    continue;
                };
                let geometry = crate::model::asset_geometry::AssetGeometry::new(object, evaluated)?;
                let points: Vec<_> = geometry
                    .points
                    .iter()
                    .map(|&point| frame.world_to_display(point))
                    .collect();
                if points.iter().any(|point| !point.is_finite()) {
                    return Err("Linked asset exceeds the display coordinate range.".into());
                }
                for indices in &geometry.triangles {
                    selectable_vertices.extend(indices.map(|index| (object.id, index as u64)));
                }
                for indices in &geometry.lines {
                    selectable_vertices.extend(indices.map(|index| (object.id, index as u64)));
                }
                selectable_vertices.extend(
                    geometry
                        .standalone_points
                        .iter()
                        .map(|&index| (object.id, index as u64)),
                );
                for indices in geometry.triangles {
                    triangles.push(Triangle {
                        object: object.id,
                        points: indices.map(|index| points[index]),
                    });
                }
                for indices in geometry.lines {
                    loose_edges.push(WorldEdge {
                        object: object.id,
                        points: indices.map(|index| points[index]),
                    });
                }
                for index in geometry.standalone_points {
                    loose_edges.push(WorldEdge {
                        object: object.id,
                        points: [points[index]; 2],
                    });
                }
                // IDs identify object-selection samples only. Asset contents
                // cannot enter vertex edit mode or authored edit topology.
                vertices.extend(points.into_iter().enumerate().map(|(index, position)| {
                    WorldVertex {
                        object: object.id,
                        id: index as u64,
                        position,
                    }
                }));
                continue;
            }
            let mesh = document.eval_object(object.id)?;
            let transform = object.transform.matrix();
            let positions: BTreeMap<_, _> = mesh
                .vertices
                .iter()
                .map(|vertex| {
                    (
                        vertex.id,
                        frame.world_to_display(
                            transform.transform_point3(DVec3::from_array(vertex.position)),
                        ),
                    )
                })
                .collect();
            for ids in mesh.triangles()? {
                selectable_vertices.extend(ids.map(|id| (object.id, id)));
                triangles.push(Triangle {
                    object: object.id,
                    points: ids.map(|id| positions[&id]),
                });
            }
            for ids in &mesh.edges {
                selectable_vertices.extend(ids.map(|id| (object.id, id)));
                loose_edges.push(WorldEdge {
                    object: object.id,
                    points: ids.map(|id| positions[&id]),
                });
            }
            vertices.extend(positions.into_iter().map(|(id, position)| WorldVertex {
                object: object.id,
                id,
                position,
            }));
        }
        Ok(Self {
            vertices,
            selectable_vertices,
            loose_edges,
            bvh: Bvh::build(triangles),
            projection: None,
            projected: Vec::new(),
        })
    }

    fn object_at(&self, position: Pos2, projection: &Projection) -> Option<u64> {
        let ray = projection.ray(position)?;
        let mut nearest = self.bvh.hit(ray);
        for edge in &self.loose_edges {
            let Some(segment) = projection.segment(edge.points) else {
                continue;
            };
            let (screen_distance, point) = segment.nearest(position);
            if screen_distance > EDGE_PICK_RADIUS {
                continue;
            }
            let distance = (point - ray.origin).dot(ray.direction);
            if nearest.as_ref().is_some_and(|hit| hit.distance < distance) {
                continue;
            }
            let Some(edge_ray) = projection.ray_through(point) else {
                continue;
            };
            let depth = (point - edge_ray.origin).dot(edge_ray.direction);
            if self
                .bvh
                .hit(edge_ray)
                .is_some_and(|hit| hit.distance + 2e-5 < depth)
            {
                continue;
            }
            nearest = Some(Hit {
                object: edge.object,
                distance,
            });
        }
        nearest.map(|hit| hit.object)
    }

    fn eligible_vertices(&self, depth: SelectionDepth) -> impl Iterator<Item = &ProjectedVertex> {
        self.projected
            .iter()
            .filter(move |vertex| depth.admits(vertex))
    }

    fn project(&mut self, projection: &Projection) {
        if self.projection.as_ref() == Some(projection) {
            return;
        }
        self.projected.clear();
        for vertex in &self.vertices {
            let Some(screen) = projection.screen(vertex.position) else {
                continue;
            };
            if !projection.viewport.contains(screen) {
                continue;
            }
            let Some(ray) = projection.ray_through(vertex.position) else {
                continue;
            };
            let depth = (vertex.position - ray.origin).dot(ray.direction);
            let occluded = self
                .bvh
                .hit(ray)
                .is_some_and(|hit| hit.distance + 2e-5 < depth);
            self.projected.push(ProjectedVertex {
                object: vertex.object,
                id: vertex.id,
                screen,
                depth,
                occluded,
            });
        }
        self.projection = Some(projection.clone());
    }
}

#[derive(Clone)]
struct Triangle {
    object: u64,
    points: [DVec3; 3],
}
struct Hit {
    object: u64,
    distance: f64,
}
struct Bounds {
    min: DVec3,
    max: DVec3,
}
enum Bvh {
    Leaf {
        bounds: Bounds,
        triangles: Vec<Triangle>,
    },
    Branch {
        bounds: Bounds,
        left: Box<Bvh>,
        right: Box<Bvh>,
    },
}

impl Bvh {
    fn build(mut triangles: Vec<Triangle>) -> Self {
        let mut bounds = Bounds {
            min: DVec3::splat(f64::INFINITY),
            max: DVec3::splat(f64::NEG_INFINITY),
        };
        for triangle in &triangles {
            for point in triangle.points {
                bounds.min = bounds.min.min(point);
                bounds.max = bounds.max.max(point);
            }
        }
        // Include boundary rays in the broad phase; triangle tests make the
        // precise hit decision with their own small barycentric tolerance.
        if !triangles.is_empty() {
            let margin =
                (bounds.max - bounds.min).max_element() * (2.0 * RAY_BARYCENTRIC_TOLERANCE) + 1e-9;
            bounds.min -= DVec3::splat(margin);
            bounds.max += DVec3::splat(margin);
        }
        if triangles.len() <= 8 {
            return Self::Leaf { bounds, triangles };
        }
        let extent = bounds.max - bounds.min;
        let axis = if extent.x >= extent.y && extent.x >= extent.z {
            0
        } else if extent.y >= extent.z {
            1
        } else {
            2
        };
        triangles.sort_by(|a, b| {
            a.points
                .iter()
                .map(|p| p[axis])
                .sum::<f64>()
                .total_cmp(&b.points.iter().map(|p| p[axis]).sum::<f64>())
        });
        let right = triangles.split_off(triangles.len() / 2);
        Self::Branch {
            bounds,
            left: Box::new(Self::build(triangles)),
            right: Box::new(Self::build(right)),
        }
    }

    fn hit(&self, ray: Ray) -> Option<Hit> {
        let mut closest = None;
        self.visit(ray, &mut closest);
        closest
    }

    fn visit(&self, ray: Ray, closest: &mut Option<Hit>) {
        let bounds = match self {
            Self::Leaf { bounds, .. } | Self::Branch { bounds, .. } => bounds,
        };
        let mut near: f64 = 0.0;
        let mut far = closest.as_ref().map_or(f64::INFINITY, |hit| hit.distance);
        for axis in 0..3 {
            if ray.direction[axis].abs() < 1e-14 {
                if ray.origin[axis] < bounds.min[axis] || ray.origin[axis] > bounds.max[axis] {
                    return;
                }
            } else {
                let a = (bounds.min[axis] - ray.origin[axis]) / ray.direction[axis];
                let b = (bounds.max[axis] - ray.origin[axis]) / ray.direction[axis];
                near = near.max(a.min(b));
                far = far.min(a.max(b));
                if near > far {
                    return;
                }
            }
        }
        match self {
            Self::Leaf { triangles, .. } => {
                for triangle in triangles {
                    if let Some(distance) = ray_triangle(ray, triangle.points)
                        && closest.as_ref().is_none_or(|hit| distance < hit.distance)
                    {
                        *closest = Some(Hit {
                            object: triangle.object,
                            distance,
                        });
                    }
                }
            }
            Self::Branch { left, right, .. } => {
                left.visit(ray, closest);
                right.visit(ray, closest);
            }
        }
    }
}

fn ray_triangle(ray: Ray, points: [DVec3; 3]) -> Option<f64> {
    let edge_a = points[1] - points[0];
    let edge_b = points[2] - points[0];
    let cross = ray.direction.cross(edge_b);
    let determinant = edge_a.dot(cross);
    if determinant.abs() < 1e-14 {
        return None;
    }
    let offset = ray.origin - points[0];
    let u = offset.dot(cross) / determinant;
    let cross = offset.cross(edge_a);
    let v = ray.direction.dot(cross) / determinant;
    if u < -RAY_BARYCENTRIC_TOLERANCE
        || v < -RAY_BARYCENTRIC_TOLERANCE
        || u + v > 1.0 + RAY_BARYCENTRIC_TOLERANCE
    {
        return None;
    }
    let distance = edge_b.dot(cross) / determinant;
    (distance >= 0.0 && distance.is_finite()).then_some(distance)
}

fn segment_distance(point: Pos2, a: Pos2, b: Pos2) -> f32 {
    let delta = b - a;
    let t = ((point - a).dot(delta) / delta.length_sq().max(1e-10)).clamp(0.0, 1.0);
    point.distance(a + delta * t)
}

fn inside_polygon(point: Pos2, polygon: &[Pos2]) -> bool {
    let mut inside = false;
    for (a, b) in polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .take(polygon.len())
    {
        if (a.y > point.y) != (b.y > point.y)
            && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
        {
            inside = !inside;
        }
    }
    inside
}

#[cfg(test)]
#[path = "editor_tests.rs"]
mod tests;
