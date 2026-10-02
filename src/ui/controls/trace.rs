//! Opt-in live UI witnesses for the executable guide and workbench.
use super::Control;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

impl Control {
    pub const ALL: [Self; 175] = [
        Self::ToolDock,
        Self::TerminalPanelToggle,
        Self::TerminalPanel,
        Self::TerminalViewport,
        Self::TerminalStatus,
        Self::AnimationPanelToggle,
        Self::ToolDockTabBar,
        Self::ToolDockClose,
        Self::AnimationTimeline,
        Self::AnimationRuler,
        Self::AnimationFit,
        Self::SceneHierarchy,
        Self::ScenePicker,
        Self::SceneClip,
        Self::ScenePlay,
        Self::SceneRest,
        Self::SceneTime,
        Self::SceneLoop,
        Self::SceneSpeed,
        Self::SceneCamera,
        Self::SceneExposure,
        Self::SceneDetails,
        Self::SceneDiagnostics,
        Self::N3Menu,
        Self::FileMenu,
        Self::Open,
        Self::Import,
        Self::Frame,
        Self::Projection,
        Self::ViewMenu,
        Self::LocalViewMenu,
        Self::Xray,
        Self::ViewPie,
        Self::ShadingPie,
        Self::PieSolid,
        Self::PieWireframe,
        Self::PieMaterialPreview,
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

    pub fn parse(id: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|control| control.id() == id)
            .ok_or_else(|| format!("Unknown control binding: {id}"))
    }
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
pub(crate) fn disable(ctx: &egui::Context) {
    ctx.data_mut(|data| data.remove::<SharedTrace>(key()));
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
