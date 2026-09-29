//! Stable UI identities. Labels come from the same catalog used by real widgets.
//! Recording is opt-in; the native viewer does not retain a documentation trace.
use crate::document::PolyhedronType;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Control {
    N3Menu,
    FileMenu,
    Open,
    Frame,
    Projection,
    ViewMenu,
    LocalViewMenu,
    Xray,
    ViewPie,
    ShadingPie,
    PieSolid,
    PieWireframe,
    PieTop,
    PieBottom,
    PieFront,
    PieBack,
    PieLeft,
    PieRight,
    PieSelection,
    ViewPerspective,
    ViewFront,
    ViewRight,
    ViewTop,
    ViewBack,
    ViewLeft,
    ViewBottom,
    Edges,
    Grid,
    SnapGrid,
    SnapSpacingMenu,
    SnapAdaptive,
    SnapFixed,
    SnapStep,
    SnapFeedback,
    ZUp,
    Preferences,
    PreferencesWindow,
    PreferencesTitle,
    PreferencesClose,
    AppearanceThemeMenu,
    ThemeSystem,
    ThemeLight,
    ThemeDark,
    AccentColor,
    ResetAccent,
    SettingsJson,
    ReloadSettings,
    PreciseScroll,
    AnimateViews,
    Duration,
    Return3DMenu,
    Return3DPerspective,
    Return3DOrientation,
    Return3DBoth,
    Gizmo,
    GizmoPreferences,
    NavigationPlanar,
    NavigationFree,
    AxisX,
    AxisNegX,
    AxisY,
    AxisNegY,
    AxisZ,
    AxisNegZ,
    Viewport,
    ViewportToolbar,
    EditToolbar,
    StatusBar,
    SceneInfo,
    ToastStack,
    ToastDismiss,
    ToastAction,
    ViewportMenu,
    ViewportFrame,
    FrameSelection,
    ViewportPreferences,
    SelectAll,
    DuplicateSelection,
    DeleteSelection,
    MakeFace,
    Dismiss,
    New,
    Save,
    SaveAs,
    InsertMenu,
    EmptyStateInsert,
    InsertCube,
    InsertCylinder,
    InsertCone,
    InsertTorus,
    InsertPlane,
    InsertCircle,
    InsertSphere,
    InsertPolyhedron,
    ToolView,
    ToolMove,
    ToolRotate,
    ToolScale,
    LeaveEdit,
    Inspector,
    LengthUnitMenu,
    LengthMillimeters,
    LengthCentimeters,
    LengthMeters,
    LengthInches,
    LengthFeet,
    PrimitiveX,
    PrimitiveY,
    PrimitiveZ,
    CircleRadius,
    CircleVertices,
    CircleFill,
    PositionX,
    PositionY,
    PositionZ,
    RotationX,
    RotationY,
    RotationZ,
    ScaleX,
    ScaleY,
    ScaleZ,
    VertexDeltaX,
    VertexDeltaY,
    VertexDeltaZ,
    Segments,
    MinorSegments,
    SphereRings,
    PolyhedronTypeMenu,
    PolyhedronTetrahedron,
    PolyhedronCube,
    PolyhedronOctahedron,
    PolyhedronIcosahedron,
    PolyhedronDodecahedron,
    PolyhedronSize,
    TubeRatio,
    ObjectList,
    LayerRename,
    TransformX,
    TransformY,
    TransformZ,
    TransformXY,
    TransformXZ,
    TransformYZ,
    TransformUniform,
    MoveAxisLock,
    TransformValue,
    Ruler2DHorizontal,
    Ruler2DVertical,
    Ruler2DViewToggle,
    Ruler2DHide,
}

impl Control {
    pub const ALL: [Self; 150] = [
        Self::N3Menu,
        Self::FileMenu,
        Self::Open,
        Self::Frame,
        Self::Projection,
        Self::ViewMenu,
        Self::LocalViewMenu,
        Self::Xray,
        Self::ViewPie,
        Self::ShadingPie,
        Self::PieSolid,
        Self::PieWireframe,
        Self::PieTop,
        Self::PieBottom,
        Self::PieFront,
        Self::PieBack,
        Self::PieLeft,
        Self::PieRight,
        Self::PieSelection,
        Self::ViewPerspective,
        Self::ViewFront,
        Self::ViewRight,
        Self::ViewTop,
        Self::ViewBack,
        Self::ViewLeft,
        Self::ViewBottom,
        Self::Edges,
        Self::Grid,
        Self::SnapGrid,
        Self::SnapSpacingMenu,
        Self::SnapAdaptive,
        Self::SnapFixed,
        Self::SnapStep,
        Self::SnapFeedback,
        Self::ZUp,
        Self::Preferences,
        Self::PreferencesWindow,
        Self::PreferencesTitle,
        Self::PreferencesClose,
        Self::AppearanceThemeMenu,
        Self::ThemeSystem,
        Self::ThemeLight,
        Self::ThemeDark,
        Self::AccentColor,
        Self::ResetAccent,
        Self::SettingsJson,
        Self::ReloadSettings,
        Self::PreciseScroll,
        Self::AnimateViews,
        Self::Duration,
        Self::Return3DMenu,
        Self::Return3DPerspective,
        Self::Return3DOrientation,
        Self::Return3DBoth,
        Self::Gizmo,
        Self::GizmoPreferences,
        Self::NavigationPlanar,
        Self::NavigationFree,
        Self::AxisX,
        Self::AxisNegX,
        Self::AxisY,
        Self::AxisNegY,
        Self::AxisZ,
        Self::AxisNegZ,
        Self::Viewport,
        Self::ViewportToolbar,
        Self::EditToolbar,
        Self::StatusBar,
        Self::SceneInfo,
        Self::ToastStack,
        Self::ToastDismiss,
        Self::ToastAction,
        Self::ViewportMenu,
        Self::ViewportFrame,
        Self::FrameSelection,
        Self::ViewportPreferences,
        Self::SelectAll,
        Self::DuplicateSelection,
        Self::DeleteSelection,
        Self::MakeFace,
        Self::Dismiss,
        Self::New,
        Self::Save,
        Self::SaveAs,
        Self::InsertMenu,
        Self::EmptyStateInsert,
        Self::InsertCube,
        Self::InsertCylinder,
        Self::InsertCone,
        Self::InsertTorus,
        Self::InsertPlane,
        Self::InsertCircle,
        Self::InsertSphere,
        Self::InsertPolyhedron,
        Self::ToolView,
        Self::ToolMove,
        Self::ToolRotate,
        Self::ToolScale,
        Self::LeaveEdit,
        Self::Inspector,
        Self::LengthUnitMenu,
        Self::LengthMillimeters,
        Self::LengthCentimeters,
        Self::LengthMeters,
        Self::LengthInches,
        Self::LengthFeet,
        Self::PrimitiveX,
        Self::PrimitiveY,
        Self::PrimitiveZ,
        Self::CircleRadius,
        Self::CircleVertices,
        Self::CircleFill,
        Self::PositionX,
        Self::PositionY,
        Self::PositionZ,
        Self::RotationX,
        Self::RotationY,
        Self::RotationZ,
        Self::ScaleX,
        Self::ScaleY,
        Self::ScaleZ,
        Self::VertexDeltaX,
        Self::VertexDeltaY,
        Self::VertexDeltaZ,
        Self::Segments,
        Self::MinorSegments,
        Self::SphereRings,
        Self::PolyhedronTypeMenu,
        Self::PolyhedronTetrahedron,
        Self::PolyhedronCube,
        Self::PolyhedronOctahedron,
        Self::PolyhedronIcosahedron,
        Self::PolyhedronDodecahedron,
        Self::PolyhedronSize,
        Self::TubeRatio,
        Self::ObjectList,
        Self::LayerRename,
        Self::TransformX,
        Self::TransformY,
        Self::TransformZ,
        Self::TransformXY,
        Self::TransformXZ,
        Self::TransformYZ,
        Self::TransformUniform,
        Self::MoveAxisLock,
        Self::TransformValue,
        Self::Ruler2DHorizontal,
        Self::Ruler2DVertical,
        Self::Ruler2DViewToggle,
        Self::Ruler2DHide,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Self::N3Menu => "app.menu",
            Self::FileMenu => "app.file",
            Self::Open => "open",
            Self::Frame => "frame",
            Self::Projection => "projection",
            Self::ViewMenu => "view",
            Self::LocalViewMenu => "view.local-view",
            Self::Xray => "view.xray",
            Self::ViewPie => "view-pie",
            Self::ShadingPie => "shading-pie",
            Self::PieSolid => "shading-pie.solid",
            Self::PieWireframe => "shading-pie.wireframe",
            Self::PieTop => "view-pie.top",
            Self::PieBottom => "view-pie.bottom",
            Self::PieFront => "view-pie.front",
            Self::PieBack => "view-pie.back",
            Self::PieLeft => "view-pie.left",
            Self::PieRight => "view-pie.right",
            Self::PieSelection => "view-pie.selection",
            Self::ViewPerspective => "view.perspective",
            Self::ViewFront => "view.front",
            Self::ViewRight => "view.right",
            Self::ViewTop => "view.top",
            Self::ViewBack => "view.back",
            Self::ViewLeft => "view.left",
            Self::ViewBottom => "view.bottom",
            Self::Edges => "edges",
            Self::Grid => "grid",
            Self::SnapGrid => "snapping.grid",
            Self::SnapSpacingMenu => "snapping.spacing",
            Self::SnapAdaptive => "snapping.adaptive",
            Self::SnapFixed => "snapping.fixed",
            Self::SnapStep => "snapping.step",
            Self::SnapFeedback => "snapping.feedback",
            Self::ZUp => "z-up",
            Self::Preferences => "preferences.open",
            Self::PreferencesWindow => "preferences.window",
            Self::PreferencesTitle => "preferences.title",
            Self::PreferencesClose => "preferences.close",
            Self::AppearanceThemeMenu => "appearance.theme",
            Self::ThemeSystem => "appearance.theme.system",
            Self::ThemeLight => "appearance.theme.light",
            Self::ThemeDark => "appearance.theme.dark",
            Self::AccentColor => "appearance.accent",
            Self::ResetAccent => "appearance.accent.reset",
            Self::SettingsJson => "preferences.settings-json",
            Self::ReloadSettings => "preferences.reload-settings",
            Self::PreciseScroll => "help.precise-scroll",
            Self::AnimateViews => "help.animate",
            Self::Duration => "help.duration",
            Self::Return3DMenu => "gizmo.return-3d",
            Self::Return3DPerspective => "gizmo.return-3d.perspective",
            Self::Return3DOrientation => "gizmo.return-3d.orientation",
            Self::Return3DBoth => "gizmo.return-3d.both",
            Self::Gizmo => "gizmo",
            Self::GizmoPreferences => "gizmo.preferences",
            Self::NavigationPlanar => "gizmo.planar",
            Self::NavigationFree => "gizmo.free",
            Self::AxisX => "gizmo.x",
            Self::AxisNegX => "gizmo.-x",
            Self::AxisY => "gizmo.y",
            Self::AxisNegY => "gizmo.-y",
            Self::AxisZ => "gizmo.z",
            Self::AxisNegZ => "gizmo.-z",
            Self::Viewport => "viewport",
            Self::ViewportToolbar => "viewport.toolbar",
            Self::EditToolbar => "viewport.edit-toolbar",
            Self::StatusBar => "status-bar",
            Self::SceneInfo => "viewport.info",
            Self::ToastStack => "toast.stack",
            Self::ToastDismiss => "toast.dismiss",
            Self::ToastAction => "toast.action",
            Self::ViewportMenu => "viewport.menu",
            Self::ViewportFrame => "viewport.frame",
            Self::FrameSelection => "frame-selection",
            Self::ViewportPreferences => "viewport.preferences",
            Self::SelectAll => "selection.all",
            Self::DuplicateSelection => "selection.duplicate",
            Self::DeleteSelection => "selection.delete",
            Self::MakeFace => "mesh.make-face",
            Self::Dismiss => "dismiss",
            Self::New => "document.new",
            Self::Save => "document.save",
            Self::SaveAs => "document.save-as",
            Self::InsertMenu => "insert",
            Self::EmptyStateInsert => "viewport.empty-insert",
            Self::InsertCube => "insert.cube",
            Self::InsertCylinder => "insert.cylinder",
            Self::InsertCone => "insert.cone",
            Self::InsertTorus => "insert.torus",
            Self::InsertPlane => "insert.plane",
            Self::InsertCircle => "insert.circle",
            Self::InsertSphere => "insert.sphere",
            Self::InsertPolyhedron => "insert.polyhedron",
            Self::ToolView => "tool.view",
            Self::ToolMove => "tool.move",
            Self::ToolRotate => "tool.rotate",
            Self::ToolScale => "tool.scale",
            Self::LeaveEdit => "edit.leave",
            Self::Inspector => "inspector",
            Self::LengthUnitMenu => "length.unit",
            Self::LengthMillimeters => "length.mm",
            Self::LengthCentimeters => "length.cm",
            Self::LengthMeters => "length.m",
            Self::LengthInches => "length.in",
            Self::LengthFeet => "length.ft",
            Self::PrimitiveX => "primitive.x",
            Self::PrimitiveY => "primitive.y",
            Self::PrimitiveZ => "primitive.z",
            Self::CircleRadius => "primitive.circle.radius",
            Self::CircleVertices => "primitive.circle.vertices",
            Self::CircleFill => "primitive.circle.fill",
            Self::PositionX => "property.position.x",
            Self::PositionY => "property.position.y",
            Self::PositionZ => "property.position.z",
            Self::RotationX => "property.rotation.x",
            Self::RotationY => "property.rotation.y",
            Self::RotationZ => "property.rotation.z",
            Self::ScaleX => "property.scale.x",
            Self::ScaleY => "property.scale.y",
            Self::ScaleZ => "property.scale.z",
            Self::VertexDeltaX => "property.vertex-delta.x",
            Self::VertexDeltaY => "property.vertex-delta.y",
            Self::VertexDeltaZ => "property.vertex-delta.z",
            Self::Segments => "primitive.segments",
            Self::MinorSegments => "primitive.minor-segments",
            Self::SphereRings => "primitive.latitude-rings",
            Self::PolyhedronTypeMenu => "primitive.polyhedron-type",
            Self::PolyhedronTetrahedron => "primitive.polyhedron.tetrahedron",
            Self::PolyhedronCube => "primitive.polyhedron.cube",
            Self::PolyhedronOctahedron => "primitive.polyhedron.octahedron",
            Self::PolyhedronIcosahedron => "primitive.polyhedron.icosahedron",
            Self::PolyhedronDodecahedron => "primitive.polyhedron.dodecahedron",
            Self::PolyhedronSize => "primitive.polyhedron.size",
            Self::TubeRatio => "primitive.tube-ratio",
            Self::ObjectList => "objects",
            Self::LayerRename => "layer.rename",
            Self::TransformX => "transform.x",
            Self::TransformY => "transform.y",
            Self::TransformZ => "transform.z",
            Self::TransformXY => "transform.xy",
            Self::TransformXZ => "transform.xz",
            Self::TransformYZ => "transform.yz",
            Self::TransformUniform => "transform.uniform",
            Self::MoveAxisLock => "move.axis-lock",
            Self::TransformValue => "transform.value",
            Self::Ruler2DHorizontal => "2d-ruler.horizontal",
            Self::Ruler2DVertical => "2d-ruler.vertical",
            Self::Ruler2DViewToggle => "view.2d-ruler",
            Self::Ruler2DHide => "2d-ruler.hide",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::N3Menu => "N3",
            Self::FileMenu => "File",
            Self::Open => "Open…",
            Self::Frame => "Frame",
            Self::Projection => "Projection",
            Self::ViewMenu => "View",
            Self::LocalViewMenu => "Local View",
            Self::Xray => "X-ray",
            Self::ViewPie => "View",
            Self::ShadingPie => "Shading",
            Self::PieSolid => "Solid",
            Self::PieWireframe => "Wireframe",
            Self::PieTop => "Top",
            Self::PieBottom => "Bottom",
            Self::PieFront => "Front",
            Self::PieBack => "Back",
            Self::PieLeft => "Left",
            Self::PieRight => "Right",
            Self::PieSelection => "Frame selection",
            Self::ViewPerspective => "Perspective",
            Self::ViewFront => "Front",
            Self::ViewRight => "Right",
            Self::ViewTop => "Top",
            Self::ViewBack => "Back",
            Self::ViewLeft => "Left",
            Self::ViewBottom => "Bottom",
            Self::Edges => "Edges",
            Self::Grid => "Grid",
            Self::SnapGrid => "Snap movement to grid",
            Self::SnapSpacingMenu => "Movement spacing",
            Self::SnapAdaptive => "Auto",
            Self::SnapFixed => "Fixed",
            Self::SnapStep => "Fixed step (cm)",
            Self::SnapFeedback => "Movement snap",
            Self::ZUp => "Z-up",
            Self::Preferences | Self::PreferencesWindow | Self::PreferencesTitle => "Preferences",
            Self::PreferencesClose => "Close",
            Self::AppearanceThemeMenu => "Theme",
            Self::ThemeSystem => "System",
            Self::ThemeLight => "Light",
            Self::ThemeDark => "Dark",
            Self::AccentColor => "Accent color",
            Self::ResetAccent => "Reset accent",
            Self::SettingsJson => "Open settings.json",
            Self::ReloadSettings => "Reload settings",
            Self::PreciseScroll => "Precise scroll zooms (for Magic Mouse)",
            Self::AnimateViews => "Animate view changes",
            Self::Duration => "Duration",
            Self::Return3DMenu => "Return to 3D",
            Self::Return3DPerspective => "Perspective only",
            Self::Return3DOrientation => "Previous orientation",
            Self::Return3DBoth => "Previous orientation + perspective",
            Self::Gizmo => "Axis gizmo",
            Self::GizmoPreferences => "Gizmo preferences…",
            Self::NavigationPlanar => "2D",
            Self::NavigationFree => "3D",
            Self::AxisX => "X",
            Self::AxisNegX => "−X",
            Self::AxisY => "Y",
            Self::AxisNegY => "−Y",
            Self::AxisZ => "Z",
            Self::AxisNegZ => "−Z",
            Self::Viewport => "Viewport",
            Self::ViewportToolbar => "Tools",
            Self::EditToolbar => "Editing object",
            Self::StatusBar => "Status bar",
            Self::SceneInfo => "Scene information",
            Self::ToastStack => "Toasts",
            Self::ToastDismiss => "Dismiss notification",
            Self::ToastAction => "Notification action",
            Self::ViewportMenu => "Viewport",
            Self::ViewportFrame => "Frame all",
            Self::FrameSelection => "Frame selection",
            Self::ViewportPreferences => "Preferences",
            Self::SelectAll => "Select all",
            Self::DuplicateSelection => "Duplicate",
            Self::DeleteSelection => "Delete",
            Self::MakeFace => "Make Face",
            Self::Dismiss => "Dismiss",
            Self::New => "New",
            Self::Save => "Save",
            Self::SaveAs => "Save as…",
            Self::InsertMenu => "Insert",
            Self::EmptyStateInsert => "Insert a shape",
            Self::InsertCube => "Cube",
            Self::InsertCylinder => "Cylinder",
            Self::InsertCone => "Cone",
            Self::InsertTorus => "Torus",
            Self::InsertPlane => "Plane",
            Self::InsertCircle => "Circle",
            Self::InsertSphere => "Sphere",
            Self::InsertPolyhedron => "Polyhedron",
            Self::ToolView => "Cursor",
            Self::ToolMove => "Move",
            Self::ToolRotate => "Rotate",
            Self::ToolScale => "Scale",
            Self::LeaveEdit => "Exit edit mode",
            Self::Inspector => "Properties",
            Self::LengthUnitMenu => "Display unit",
            Self::LengthMillimeters => "Millimeters (mm)",
            Self::LengthCentimeters => "Centimeters (cm)",
            Self::LengthMeters => "Meters (m)",
            Self::LengthInches => "Inches (in)",
            Self::LengthFeet => "Feet (ft)",
            Self::PrimitiveX => "Width",
            Self::PrimitiveY => "Height",
            Self::PrimitiveZ => "Depth",
            Self::CircleRadius => "Radius",
            Self::CircleVertices => "Vertices",
            Self::CircleFill => "Fill",
            Self::PositionX => "Position X",
            Self::PositionY => "Position Y",
            Self::PositionZ => "Position Z",
            Self::RotationX => "Rotation X",
            Self::RotationY => "Rotation Y",
            Self::RotationZ => "Rotation Z",
            Self::ScaleX => "Scale X",
            Self::ScaleY => "Scale Y",
            Self::ScaleZ => "Scale Z",
            Self::VertexDeltaX => "Vertex movement X",
            Self::VertexDeltaY => "Vertex movement Y",
            Self::VertexDeltaZ => "Vertex movement Z",
            Self::Segments => "Segments",
            Self::MinorSegments => "Tube segments",
            Self::SphereRings => "Latitude rings",
            Self::PolyhedronTypeMenu => "Polyhedron type",
            Self::PolyhedronTetrahedron => PolyhedronType::Tetrahedron.label(),
            Self::PolyhedronCube => PolyhedronType::Cube.label(),
            Self::PolyhedronOctahedron => PolyhedronType::Octahedron.label(),
            Self::PolyhedronIcosahedron => PolyhedronType::Icosahedron.label(),
            Self::PolyhedronDodecahedron => PolyhedronType::Dodecahedron.label(),
            Self::PolyhedronSize => "Polyhedron size",
            Self::TubeRatio => "Tube ratio",
            Self::ObjectList => "Layers",
            Self::LayerRename => "Rename layer",
            Self::TransformX => "X",
            Self::TransformY => "Y",
            Self::TransformZ => "Z",
            Self::TransformXY => "XY",
            Self::TransformXZ => "XZ",
            Self::TransformYZ => "YZ",
            Self::TransformUniform => "Uniform scale",
            Self::MoveAxisLock => "Transform axis lock",
            Self::TransformValue => "Transform value",
            Self::Ruler2DHorizontal => "Horizontal 2D ruler",
            Self::Ruler2DVertical => "Vertical 2D ruler",
            Self::Ruler2DViewToggle => "2D ruler",
            Self::Ruler2DHide => "Hide 2D ruler",
        }
    }
    pub fn parse(id: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|control| control.id() == id)
            .ok_or_else(|| format!("Unknown control binding: {id}"))
    }

    /// Key hints are presentation of the input bindings, separate from the
    /// witnessed control name used by accessibility and generated menu paths.
    fn menu_shortcut(self) -> Option<&'static str> {
        match self {
            Self::LocalViewMenu => Some("view.local"),
            Self::Xray => Some("view.xray"),
            Self::MakeFace => Some("mesh.make-face"),
            Self::ViewPerspective => Some("view.perspective"),
            Self::ViewFront => Some("view.front"),
            Self::ViewRight => Some("view.right"),
            Self::ViewBack => Some("view.back"),
            Self::ViewLeft => Some("view.left"),
            Self::ViewTop => Some("view.top"),
            Self::ViewBottom => Some("view.bottom"),
            Self::Ruler2DHide => Some("view.2d-ruler"),
            _ => None,
        }
    }
}

pub fn shortcut_label(id: &str) -> String {
    crate::input::bindings::binding(id)
        .expect("UI shortcut must be registered in the input bindings")
        .label()
}

#[derive(Clone, Debug)]
pub struct Observed {
    pub label: String,
    pub parents: Vec<Control>,
    pub rect: egui::Rect,
    pub enabled: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Trace {
    pub controls: BTreeMap<Control, Observed>,
    parents: Vec<Control>,
    errors: Vec<String>,
}

impl Trace {
    pub fn get(&self, control: Control) -> Result<&Observed, String> {
        self.validate()?;
        self.controls
            .get(&control)
            .ok_or_else(|| format!("Control {} is not visible in the real UI", control.id()))
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors.join("\n"))
        }
    }
    pub fn path(&self, control: Control) -> Result<String, String> {
        let item = self.get(control)?;
        let mut labels = Vec::new();
        for parent in &item.parents {
            labels.push(self.get(*parent)?.label.as_str());
        }
        labels.push(&item.label);
        Ok(labels.join(" → "))
    }
}

type SharedTrace = Arc<Mutex<Trace>>;
fn key() -> egui::Id {
    egui::Id::new("n3.documentation.trace")
}
fn active(ctx: &egui::Context) -> Option<SharedTrace> {
    ctx.data(|data| data.get_temp(key()))
}

pub fn enable(ctx: &egui::Context) {
    ctx.data_mut(|data| data.insert_temp(key(), Arc::new(Mutex::new(Trace::default()))));
}
pub fn begin_pass(ctx: &egui::Context) {
    if let Some(trace) = active(ctx) {
        *trace.lock().unwrap() = Trace::default();
    }
}
pub fn snapshot(ctx: &egui::Context) -> Trace {
    active(ctx)
        .expect("documentation tracing enabled")
        .lock()
        .unwrap()
        .clone()
}
pub fn record(ctx: &egui::Context, control: Control, label: &str, rect: egui::Rect, enabled: bool) {
    if let Some(trace) = active(ctx) {
        let mut trace = trace.lock().unwrap();
        let observed = Observed {
            label: label.to_owned(),
            parents: trace.parents.clone(),
            rect,
            enabled,
        };
        if trace.controls.insert(control, observed).is_some() {
            trace.errors.push(format!(
                "Ambiguous control: {} appears twice in one UI pass",
                control.id()
            ));
        }
    }
}
pub fn scope<R>(ctx: &egui::Context, parent: Control, draw: impl FnOnce() -> R) -> R {
    let trace = active(ctx);
    if let Some(trace) = &trace {
        trace.lock().unwrap().parents.push(parent);
    }
    let result = draw();
    if let Some(trace) = &trace {
        trace.lock().unwrap().parents.pop();
    }
    result
}
pub fn button(ui: &mut egui::Ui, control: Control) -> egui::Response {
    button_enabled(ui, control, true)
}

pub fn button_with_icon(
    ui: &mut egui::Ui,
    control: Control,
    icon: super::lucide::Icon,
) -> egui::Response {
    let response = ui.add(egui::Button::new((
        icon.text(ui.spacing().icon_width),
        control.label(),
    )));
    // The glyph is decorative; accessibility and guide bindings keep the name.
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            response.enabled(),
            control.label(),
        )
    });
    record(
        ui.ctx(),
        control,
        control.label(),
        response.rect,
        response.enabled(),
    );
    response
}

pub fn button_enabled(ui: &mut egui::Ui, control: Control, enabled: bool) -> egui::Response {
    button_enabled_min_width(ui, control, enabled, 0.0)
}

pub fn button_enabled_min_width(
    ui: &mut egui::Ui,
    control: Control,
    enabled: bool,
    min_width: f32,
) -> egui::Response {
    let mut button = egui::Button::new(control.label());
    if let Some(id) = control.menu_shortcut() {
        button = button.shortcut_text(shortcut_label(id));
    }
    button = button.min_size(egui::vec2(min_width, 0.0));
    let response = ui.add_enabled(enabled, button);
    record(
        ui.ctx(),
        control,
        control.label(),
        response.rect,
        response.enabled(),
    );
    response
}
pub fn checkbox(ui: &mut egui::Ui, control: Control, value: &mut bool) -> egui::Response {
    let response = ui.checkbox(value, control.label());
    record(
        ui.ctx(),
        control,
        control.label(),
        response.rect,
        response.enabled(),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(20., 20.))
    }
    #[test]
    fn catalog_ids_are_unique_and_unknown_ids_fail() {
        let ids: std::collections::BTreeSet<_> =
            Control::ALL.map(Control::id).into_iter().collect();
        assert_eq!(ids.len(), Control::ALL.len());
        assert!(Control::parse("removed.save").is_err());
        for control in Control::ALL {
            if let Some(id) = control.menu_shortcut() {
                assert!(crate::input::bindings::binding(id).is_ok());
            }
        }
    }
    #[test]
    fn paths_follow_actual_scope_and_displayed_label() {
        let ctx = egui::Context::default();
        enable(&ctx);
        record(&ctx, Control::PreferencesWindow, "Settings", rect(), true);
        scope(&ctx, Control::PreferencesWindow, || {
            record(&ctx, Control::AnimateViews, "Animate", rect(), true)
        });
        assert_eq!(
            snapshot(&ctx).path(Control::AnimateViews).unwrap(),
            "Settings → Animate"
        );
        begin_pass(&ctx);
        record(&ctx, Control::AnimateViews, "Animate", rect(), true);
        assert_eq!(
            snapshot(&ctx).path(Control::AnimateViews).unwrap(),
            "Animate"
        );
    }
    #[test]
    fn missing_and_duplicate_controls_fail_instead_of_falling_back_to_catalog() {
        let ctx = egui::Context::default();
        enable(&ctx);
        assert!(snapshot(&ctx).path(Control::Frame).is_err());
        record(&ctx, Control::Frame, "Frame", rect(), true);
        record(&ctx, Control::Frame, "Frame", rect(), true);
        assert!(
            snapshot(&ctx)
                .get(Control::Frame)
                .unwrap_err()
                .contains("Ambiguous")
        );
    }
}
