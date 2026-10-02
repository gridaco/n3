//! Application actions shared by menus and other command presenters.
//!
//! Metadata is stable; availability and checked state are resolved from the
//! current workspace. Physical shortcuts remain owned by `bindings`.
use super::{
    bindings::{BINDINGS, Binding, Trigger},
    shortcuts::Command,
};
use crate::{camera::View, document::PrimitiveKind};

/// Live action metadata resolved by the workspace before presentation or
/// dispatch. It is transient UI state, never authored document data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionState {
    pub visible: bool,
    pub enabled: bool,
    pub checked: Option<bool>,
}

impl Default for ActionState {
    fn default() -> Self {
        Self {
            visible: true,
            enabled: true,
            checked: None,
        }
    }
}

macro_rules! actions {
    ($($variant:ident => ($id:literal, $label:literal, $command:expr, $help:expr)),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum ActionId {
            $($variant),+
        }

        impl ActionId {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub fn id(self) -> &'static str {
                match self { $(Self::$variant => $id),+ }
            }

            pub fn label(self) -> &'static str {
                match self { $(Self::$variant => $label),+ }
            }

            pub fn command(self) -> Command {
                match self { $(Self::$variant => $command),+ }
            }

            pub fn help(self) -> Option<&'static str> {
                match self { $(Self::$variant => $help),+ }
            }
        }
    };
}

actions! {
    New => ("document.new", "New", Command::New, None),
    Open => ("document.open", "Open…", Command::Open, Some("Open N3, OBJ, glTF or GLB")),
    Import => ("document.import", "Import…", Command::Import, Some("Add objects from another file to this document")),
    Save => ("document.save", "Save", Command::Save { save_as: false }, None),
    SaveAs => ("document.save-as", "Save as…", Command::Save { save_as: true }, None),
    Preferences => ("preferences.open", "Preferences", Command::OpenPreferences, None),
    AnimationPanel => ("animation.toggle", "Animation", Command::ToggleAnimationPanel, Some("Show or hide the animation panel")),
    TerminalPanel => ("terminal.toggle", "Terminal", Command::ToggleTerminalPanel, Some("Show or hide the Terminal panel in the Tool Dock")),
    CloseToolDock => ("tool-dock.close", "Close Tool Dock", Command::CloseToolDock, None),
    AnimationPlayback => ("animation.play-pause", "Play / pause", Command::ToggleAnimationPlayback, Some("Play or pause the selected clip when the timeline is focused")),
    SelectAll => ("selection.all", "Select all", Command::SelectAll, None),
    Duplicate => ("selection.duplicate", "Duplicate", Command::DuplicateSelection, Some("Duplicate selected objects in place")),
    Delete => ("selection.delete", "Delete", Command::DeleteSelection, Some("Delete the selected objects or vertices")),
    MakeFace => ("mesh.make-face", "Make Face", Command::MakeFace, Some("Create one face from three vertices or a single planar boundary")),
    FrameAll => ("view.frame", "Frame all", Command::Frame, Some("Fit the document")),
    FrameSelection => ("view.frame-selection", "Frame selection", Command::FrameSelection, Some("Fit the selected objects or vertices")),
    LocalView => ("view.local", "Local View", Command::ToggleLocalView, Some("Isolate selected objects; activate again to restore the scene")),
    Xray => ("view.xray", "X-ray", Command::ToggleXray, Some("See and select through surfaces")),
    Ruler2D => ("view.2d-ruler", "2D ruler", Command::ToggleRuler2D, None),
    ToggleFpsMeter => ("view.fps-meter.toggle", "Show FPS Meter", Command::ToggleFpsMeter, Some("Measure completed application frame cadence without requesting extra frames")),
    Edges => ("view.edges", "Edges", Command::ToggleEdges, Some("Original polygon boundaries")),
    ViewPerspective => ("view.perspective", "Perspective", Command::View(View::Perspective), None),
    ViewFront => ("view.front", "Front", Command::View(View::Front), None),
    ViewRight => ("view.right", "Right", Command::View(View::Right), None),
    ViewBack => ("view.back", "Back", Command::View(View::Back), None),
    ViewLeft => ("view.left", "Left", Command::View(View::Left), None),
    ViewTop => ("view.top", "Top", Command::View(View::Top), None),
    ViewBottom => ("view.bottom", "Bottom", Command::View(View::Bottom), None),
    InsertCube => ("insert.cube", "Cube", Command::Insert(PrimitiveKind::Cube), None),
    InsertCylinder => ("insert.cylinder", "Cylinder", Command::Insert(PrimitiveKind::Cylinder), None),
    InsertCone => ("insert.cone", "Cone", Command::Insert(PrimitiveKind::Cone), None),
    InsertTorus => ("insert.torus", "Torus", Command::Insert(PrimitiveKind::Torus), None),
    InsertPlane => ("insert.plane", "Plane", Command::Insert(PrimitiveKind::Plane), None),
    InsertCircle => ("insert.circle", "Circle", Command::Insert(PrimitiveKind::Circle), None),
    InsertSphere => ("insert.sphere", "Sphere", Command::Insert(PrimitiveKind::Sphere), None),
    InsertPolyhedron => ("insert.polyhedron", "Polyhedron", Command::Insert(PrimitiveKind::Polyhedron), None),
}

impl ActionId {
    /// Binding order defines the primary presentation when several inputs run
    /// the same action. Aliases remain available to the input router.
    /// Return structured input so presenters can choose words or key glyphs.
    pub fn shortcut(self) -> Option<Binding> {
        let command = self.command();
        BINDINGS
            .iter()
            .find(|binding| binding.trigger == Trigger::Press && binding.command == Some(command))
            .copied()
    }

    pub fn from_command(command: Command) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|action| action.command() == command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_commands_and_expected_shortcuts_stay_consistent() {
        let mut ids = std::collections::BTreeSet::new();
        for &action in ActionId::ALL {
            assert!(
                ids.insert(action.id()),
                "Duplicate action ID: {}",
                action.id()
            );
            assert!(!action.label().is_empty());
            assert_eq!(ActionId::from_command(action.command()), Some(action));

            let unbound = matches!(
                action,
                ActionId::Import
                    | ActionId::AnimationPanel
                    | ActionId::TerminalPanel
                    | ActionId::CloseToolDock
                    | ActionId::ToggleFpsMeter
                    | ActionId::Edges
                    | ActionId::InsertCube
                    | ActionId::InsertCylinder
                    | ActionId::InsertCone
                    | ActionId::InsertTorus
                    | ActionId::InsertPlane
                    | ActionId::InsertCircle
                    | ActionId::InsertSphere
                    | ActionId::InsertPolyhedron
            );
            if unbound {
                assert!(
                    action.shortcut().is_none(),
                    "{} is intentionally unbound",
                    action.id()
                );
            } else {
                let binding = action.shortcut().expect("Bound action lost its shortcut");
                assert_eq!(binding.command, Some(action.command()));
                assert_eq!(binding.id, action.id());
            }
        }
        assert_eq!(ActionId::Delete.shortcut().unwrap().id, "selection.delete");
        assert_eq!(ActionId::ViewFront.shortcut().unwrap().id, "view.front");
        assert_eq!(
            ActionId::from_command(Command::TransformCharacter('3')),
            None
        );
    }
}
