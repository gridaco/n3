//! Every guide has one registered scenario and template.
use super::*;

pub(super) struct Feature {
    pub(super) slug: &'static str,
    pub(super) title: &'static str,
    pub(super) template: &'static str,
    pub(super) scenario: fn(&mut Session<'_>) -> Result<()>,
}
pub(super) const FEATURES: &[Feature] = &[
    Feature {
        slug: "settings",
        title: "User settings",
        template: include_str!("../../docs/templates/settings.md.in"),
        scenario: settings::run,
    },
    Feature {
        slug: "edit-selection",
        title: "Reading vertex selections",
        template: include_str!("../../docs/templates/edit-selection.md.in"),
        scenario: edit_selection::run,
    },
    Feature {
        slug: "numeric-transforms",
        title: "Numeric transforms",
        template: include_str!("../../docs/templates/numeric-transforms.md.in"),
        scenario: numeric_transforms::run,
    },
    Feature {
        slug: "snapping",
        title: "Snapping movement",
        template: include_str!("../../docs/templates/snapping.md.in"),
        scenario: snapping::run,
    },
    Feature {
        slug: "workspace",
        title: "Workspace layout and property editing",
        template: include_str!("../../docs/templates/workspace.md.in"),
        scenario: workspace::run,
    },
    Feature {
        slug: "terminal",
        title: "Terminal",
        template: include_str!("../../docs/templates/terminal.md.in"),
        scenario: terminal::run,
    },
    Feature {
        slug: "toasts",
        title: "Toasts and shortcut hints",
        template: include_str!("../../docs/templates/toasts.md.in"),
        scenario: toasts::run,
    },
    Feature {
        slug: "length-units",
        title: "Length units",
        template: include_str!("../../docs/templates/length-units.md.in"),
        scenario: length_units::run,
    },
    Feature {
        slug: "2d",
        title: "2D",
        template: include_str!("../../docs/templates/2d.md.in"),
        scenario: two_d::run,
    },
    Feature {
        slug: "2d-ruler",
        title: "2D ruler",
        template: include_str!("../../docs/templates/2d-ruler.md.in"),
        scenario: ruler_2d::run,
    },
    Feature {
        slug: "axis-locks",
        title: "Moving along axes",
        template: include_str!("../../docs/templates/axis-locks.md.in"),
        scenario: axis_locks::run,
    },
    Feature {
        slug: "editing",
        title: "Editing geometry",
        template: include_str!("../../docs/templates/editing.md.in"),
        scenario: editing::run,
    },
    Feature {
        slug: "make-face",
        title: "Making a face",
        template: include_str!("../../docs/templates/make-face.md.in"),
        scenario: make_face::run,
    },
    Feature {
        slug: "object-feedback",
        title: "Finding and selecting objects",
        template: include_str!("../../docs/templates/object-feedback.md.in"),
        scenario: object_feedback::run,
    },
    Feature {
        slug: "selection-keys",
        title: "Selecting, duplicating and deleting with keys",
        template: include_str!("../../docs/templates/selection-keys.md.in"),
        scenario: selection_keys::run,
    },
    Feature {
        slug: "opening-models",
        title: "Opening models",
        template: include_str!("../../docs/templates/opening-models.md.in"),
        scenario: opening_models::run,
    },
    Feature {
        slug: "scene-viewer",
        title: "Imported assets",
        template: include_str!("../../docs/templates/scene-viewer.md.in"),
        scenario: scene_viewer::run,
    },
    Feature {
        slug: "animation",
        title: "Inspecting animation",
        template: include_str!("../../docs/templates/animation.md.in"),
        scenario: animation_inspection::run,
    },
    Feature {
        slug: "navigation",
        title: "Navigation",
        template: include_str!("../../docs/templates/navigation.md.in"),
        scenario: navigation::run,
    },
    Feature {
        slug: "local-view",
        title: "Local View",
        template: include_str!("../../docs/templates/local-view.md.in"),
        scenario: local_view::run,
    },
    Feature {
        slug: "view-keys",
        title: "Number keys and fitting the view",
        template: include_str!("../../docs/templates/view-keys.md.in"),
        scenario: view_keys::run,
    },
    Feature {
        slug: "view-pie",
        title: "Choosing a view with the pie",
        template: include_str!("../../docs/templates/view-pie.md.in"),
        scenario: view_pie::run,
    },
    Feature {
        slug: "xray",
        title: "Selecting through surfaces with X-ray",
        template: include_str!("../../docs/templates/xray.md.in"),
        scenario: xray::run,
    },
    Feature {
        slug: "shading",
        title: "Viewport shading",
        template: include_str!("../../docs/templates/shading.md.in"),
        scenario: shading::run,
    },
    Feature {
        slug: "hand-tool",
        title: "Panning with the hand tool",
        template: include_str!("../../docs/templates/hand-tool.md.in"),
        scenario: hand_tool::run,
    },
    Feature {
        slug: "orbit-tool",
        title: "Temporary orbit tool",
        template: include_str!("../../docs/templates/orbit-tool.md.in"),
        scenario: orbit_tool::run,
    },
    Feature {
        slug: "gizmo",
        title: "Axis gizmo",
        template: include_str!("../../docs/templates/gizmo.md.in"),
        scenario: gizmo::run,
    },
];
