use super::{
    local_view::{self, LocalView},
    lucide, menu,
    toast::{Toast, ToastKind, Toaster},
    typography,
};
use crate::input::actions::ActionId;
use crate::input::temporary_navigation::{HeldNavigation, OrderedInput, ordered_input};
use crate::{
    axis_gizmo,
    camera::{Camera, Transition, View},
    controls::{self, Control, shortcut_label},
    document::{
        DisplayFrame, Document, Geometry, PolyhedronType, Primitive, PrimitiveKind, Transform,
    },
    edit_feedback::EditSelection,
    editor::{Editor, Tool},
    mesh::MeshData,
    move_input,
    navigation_input::{NavigationInput, NavigationMotion, NavigationTool},
    navigation_state::{PlanarExit, ViewNavigation},
    object_feedback,
    pie_input::{PieContext, PieInput},
    renderer::ObjectHighlights,
    ruler_2d::{self, Ruler2DModel},
    scroll_input::{ScrollInput, ScrollMotion, ScrollPhase},
    settings::{AccentColor, ResolvedTheme, ThemeMode},
    shortcuts::{self, Command, HostEffect},
    snapping::{StepPolicy, TranslationSource},
    theme::{self, Palette},
    units::{self, LengthUnit},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::Duration,
};

// Executable guide/workbench observations are separate from the shared runtime.
#[cfg(not(target_arch = "wasm32"))]
mod inspection;
#[cfg(not(target_arch = "wasm32"))]
pub use inspection::SavedLayout;

#[path = "action_state.rs"]
mod action_state;
#[path = "animation_panel.rs"]
mod animation_panel;
#[path = "asset_instances.rs"]
mod asset_instances;
#[path = "tool_dock.rs"]
mod tool_dock;
#[path = "user_settings.rs"]
mod user_settings;
pub(crate) use tool_dock::ToolDockPanel;

/// Numeric entry stays exact; pointer scrubbing shares the viewport movement
/// policy. egui retains precise scrub accumulation internally, so the field can
/// display the applied snapped value without discarding sub-step motion.
fn property_translation_source(response: &egui::Response) -> TranslationSource {
    if response.dragged() || response.drag_stopped() {
        TranslationSource::Interactive
    } else {
        TranslationSource::Exact
    }
}

fn movement_snap_label(step_cm: f64, unit: LengthUnit) -> String {
    let converted = unit.from_centimeters(step_cm);
    let (value, unit) = if converted.is_finite() && converted > 0.0 {
        (converted, unit)
    } else {
        // Extreme values may not be representable in every presentation unit.
        // Fall back to centimeters rather than claim a zero or infinite step.
        (step_cm, LengthUnit::Centimeters)
    };
    // This read-only badge may round display precision. Editable fields retain
    // their round-trippable formatter so mere focus changes never mutate data.
    let readable = format!("{value:.5e}")
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(value);
    format!(
        "Snap: {} {}",
        units::format_length_number(readable),
        unit.symbol()
    )
}

pub use crate::settings::{DEFAULT_VIEW_DURATION_MS, MAX_VIEW_DURATION_MS};
fn pending_transform_message() -> String {
    format!(
        "Apply the transform preview with {}, or cancel with {}, before continuing.",
        shortcut_label("edit.confirm"),
        shortcut_label("cancel")
    )
}
const POSITION_SCRUB_CM_PER_POINT: f64 = 0.02;
const VERTEX_SCRUB_CM_PER_POINT: f64 = 0.01;
const DEFAULT_PANEL_WIDTH: f32 = theme::size::STEP_60;
const N3_MENU_SIZE: f32 = theme::size::STEP_8;
const N3_LOGO_SIZE: f32 = theme::size::STEP_6;
const N3_LOGO_PNG: &[u8] = include_bytes!("../../assets/logo/n3-logo-01-crisp.png");
const HIERARCHY_MIN_WIDTH: f32 = 145.0;
const INSPECTOR_MIN_WIDTH: f32 = 232.0;
const LAYER_ICON_INSET: f32 = theme::space::LG;
// Icon inset, 16-point glyph, and one 8-point gap.
const LAYER_TEXT_INSET: f32 = theme::space::XL_4;

fn workspace_side_panel_frame(style: &egui::Style, fill: egui::Color32) -> egui::Frame {
    // Sections own their padding; separators and scrollbars use the full panel.
    egui::Frame::side_top_panel(style)
        .inner_margin(egui::Margin::ZERO)
        .fill(fill)
}

fn workspace_panel_section<R>(
    ui: &mut egui::Ui,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    egui::Frame::NONE
        .inner_margin(theme::space::margin(theme::space::LG))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = theme::space::MD;
            content(ui)
        })
}

fn workspace_panel_separator(ui: &mut egui::Ui) {
    ui.add(egui::Separator::default().spacing(theme::space::PX));
}

fn viewport_toolbar_frame(style: &egui::Style) -> egui::Frame {
    let frame = egui::Frame::popup(style);
    // The painted frame includes its stroke outside the content inset.
    // Match the outer curve to each button's radius plus that full inset.
    let inset = frame.inner_margin.leftf() + frame.stroke.width;
    let radius = (f32::from(theme::radius::MD) + inset).round() as u8;
    frame.corner_radius(radius)
}

fn viewport_toolbar_width(style: &egui::Style) -> f32 {
    let frame = viewport_toolbar_frame(style);
    theme::size::XL_4 + 2.0 * (frame.inner_margin.leftf() + frame.stroke.width)
}

fn apply_sidebar_colors(ui: &mut egui::Ui, palette: Palette) {
    let visuals = ui.visuals_mut();
    visuals.widgets.noninteractive.bg_fill = palette.sidebar;
    visuals.widgets.noninteractive.bg_stroke.color = palette.sidebar_border;
    visuals.widgets.noninteractive.fg_stroke.color = palette.sidebar_foreground;
    visuals.widgets.hovered.bg_fill = palette.sidebar_accent;
    visuals.widgets.hovered.weak_bg_fill = palette.sidebar_accent;
    visuals.widgets.hovered.fg_stroke.color = palette.sidebar_accent_foreground;
    visuals.widgets.active.bg_stroke.color = palette.sidebar_ring;
    visuals.selection.stroke.color = palette.sidebar_primary;
}

/// A section is a heading and its existing controls. Keeping this spacing in
/// one place gives the inspector a consistent rhythm as controls are added.
fn inspector_section(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .inner_margin(egui::Margin {
            left: theme::space::LG as i8,
            right: theme::space::LG as i8,
            top: theme::SECTION_TOP as i8,
            bottom: theme::SECTION_BOTTOM as i8,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = theme::INSPECTOR_ITEM_GAP;
            let muted = ui.visuals().weak_text_color();
            ui.label(
                theme::strong(title)
                    .size(theme::text::SECTION_TITLE_13)
                    .color(muted),
            );
            ui.add_space(theme::SECTION_GAP);
            content(ui);
        });
    workspace_panel_separator(ui);
}

/// Lay out a label and three equal numeric fields on one line. The row budget
/// fits a 240-point inspector without forcing the resizable panel wider.
fn inspector_axis_row(
    ui: &mut egui::Ui,
    label: &str,
    mut field: impl FnMut(&mut egui::Ui, usize, f32) -> egui::Response,
) -> [egui::Response; 3] {
    let width = ui.available_width();
    let gap = theme::ROW_GAP;
    let label_width = (width * 0.28).clamp(72.0, 116.0);
    // Reserve a little space past Z. egui's button frame expands by a pixel
    // when focused or dragged; the inspector divider must never clip it.
    let field_width = ((width - label_width - 3.0 * gap - theme::space::LG) / 3.0).max(40.0);
    let result = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        inspector_row_label(ui, label, label_width);
        std::array::from_fn(|index| field(ui, index, field_width))
    });
    result.inner
}

fn inspector_row_label(ui: &mut egui::Ui, label: &str, width: f32) {
    let (slot, _) =
        ui.allocate_exact_size(egui::vec2(width, theme::ROW_HEIGHT), egui::Sense::hover());
    let content = slot.shrink2(egui::vec2(theme::LABEL_INSET, 0.0));
    let mut label_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    label_ui.set_clip_rect(ui.clip_rect().intersect(content));
    label_ui.label(label).on_hover_text(label);
}

fn inspector_scalar_row(
    ui: &mut egui::Ui,
    label: &str,
    field: impl FnOnce(&mut egui::Ui, f32) -> egui::Response,
) -> egui::Response {
    let width = ui.available_width();
    let gap = theme::ROW_GAP;
    let label_width = (width * 0.28).clamp(72.0, 116.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        inspector_row_label(ui, label, label_width);
        field(ui, (width - label_width - gap).min(104.0))
    })
    .inner
}

/// Keep a numeric field inside its allocated slot. egui's `add_sized` may grow
/// its parent to fit a long number (notably values shown in inches), which
/// would change the inspector width and the viewport while merely repainting.
/// The field still keeps its own ID and response for editing and replay.
fn inspector_numeric_value(
    ui: &mut egui::Ui,
    axis: Option<&str>,
    width: f32,
    value: egui::DragValue<'_>,
) -> egui::Response {
    let palette = Palette::from_context(ui.ctx(), AccentColor::DEFAULT);
    ui.scope(|ui| {
        let style = ui.style_mut();
        style.spacing.button_padding = egui::vec2(theme::FIELD_PADDING_X, theme::FIELD_PADDING_Y);
        for visuals in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
        ] {
            visuals.corner_radius = egui::CornerRadius::same(theme::radius::MD);
        }
        let (slot, _) =
            ui.allocate_exact_size(egui::vec2(width, theme::ROW_HEIGHT), egui::Sense::hover());
        let mut field_ui = ui.new_child(egui::UiBuilder::new().max_rect(slot).layout(
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
        ));
        field_ui.set_clip_rect(ui.clip_rect().intersect(slot));
        let value = if axis.is_some() {
            value.prefix("    ")
        } else {
            value
        };
        let mut response = field_ui.add(value);
        // Preserve the actual widget response but expose its visible bounds
        // to control tracing and active edit-session ownership.
        response.rect = response.rect.intersect(slot);
        if let Some(axis) = axis
            && !response.has_focus()
        {
            ui.painter().text(
                response.rect.left_center() + egui::vec2(theme::space::MD + theme::space::PX, 0.0),
                egui::Align2::LEFT_CENTER,
                axis,
                egui::FontId::proportional(theme::text::AXIS_LABEL_11),
                palette.muted_foreground,
            );
        }
        response
    })
    .inner
}

/// The axis letter is painted over the value's button, so each compact pill
/// remains one semantic DragValue for focus, hover, replay, and edit sessions.
/// While typing, the letter is hidden to give the entire field to the editor.
fn inspector_axis_value(
    ui: &mut egui::Ui,
    axis: &str,
    width: f32,
    value: egui::DragValue<'_>,
) -> egui::Response {
    inspector_numeric_value(ui, Some(axis), width, value)
}

struct LayerRow {
    id: u64,
    rect: egui::Rect,
    painter: egui::Painter,
    name: String,
    icon: lucide::Icon,
    text_position: egui::Pos2,
    font: egui::FontId,
    text_color: egui::Color32,
}

fn layer_icon(geometry: &Geometry) -> lucide::Icon {
    match geometry {
        Geometry::Primitive(primitive) => match primitive.kind {
            PrimitiveKind::Cube => lucide::Icon::Box,
            PrimitiveKind::Cylinder => lucide::Icon::Cylinder,
            PrimitiveKind::Cone => lucide::Icon::Cone,
            PrimitiveKind::Torus => lucide::Icon::Torus,
            PrimitiveKind::Plane => lucide::Icon::RectangleHorizontal,
            PrimitiveKind::Circle => lucide::Icon::Circle,
            // TODO: Add dedicated icons for these primitives in the icon pass.
            PrimitiveKind::Sphere | PrimitiveKind::Polyhedron => lucide::Icon::ScanBox,
        },
        Geometry::Mesh(_) => lucide::Icon::ScanBox,
        Geometry::Asset(_) => lucide::Icon::Box,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PropertyField {
    PrimitiveSize(usize),
    Segments,
    MinorSegments,
    TubeRatio,
    Translation(usize),
    Rotation(usize),
    Scale(usize),
    VertexDelta(usize),
}

impl PropertyField {
    fn translation_sensitivity(self) -> Option<f64> {
        match self {
            Self::Translation(_) => Some(POSITION_SCRUB_CM_PER_POINT),
            Self::VertexDelta(_) => Some(VERTEX_SCRUB_CM_PER_POINT),
            _ => None,
        }
    }
}

struct PropertySession {
    object_id: u64,
    field: PropertyField,
    rect: egui::Rect,
}

struct LayerRename {
    id: u64,
    value: String,
    select_all: bool,
}

/// Cardinal orthographic projection needs only the display-space bounding box
/// of each selected group. Cache those eight corners so pan/zoom never evaluate
/// primitives or clone the selected mesh again.
struct Ruler2DSelectionCache {
    revision: u64,
    frame: DisplayFrame,
    active_object: Option<u64>,
    objects: BTreeSet<u64>,
    vertices: BTreeSet<u64>,
    edit_mode: bool,
    z_up: bool,
    corners: Vec<Vec<glam::Vec3>>,
}

/// Host-owned availability, separate from documents, preferences and history.
/// Shared controls and dispatch consult these capabilities instead of defining
/// another feature UI for each platform.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HostCapabilities {
    pub local_terminal: bool,
    pub open_settings_file: bool,
}

impl Default for HostCapabilities {
    fn default() -> Self {
        Self {
            local_terminal: true,
            open_settings_file: true,
        }
    }
}

impl HostCapabilities {
    pub(crate) const BROWSER: Self = Self {
        local_terminal: false,
        open_settings_file: false,
    };
}

/// Shared application state and UI. I/O is dispatched by the host.
pub struct WorkspaceUi {
    pub(crate) host_capabilities: HostCapabilities,
    pub scene_texture: egui::TextureId,
    n3_logo_texture: Option<egui::TextureHandle>,
    pub camera: Camera,
    /// Full scene bounds for rendering, camera projection, and geometry picking.
    pub viewport: egui::Rect,
    /// Usable area for floating controls and pointer input, inset by the 2D ruler.
    pub viewport_ui_rect: egui::Rect,
    pub ruler_2d_model: Option<Ruler2DModel>,
    ruler_2d_selection: Option<Ruler2DSelectionCache>,
    pub editor: Editor,
    pub asset_views: BTreeMap<crate::document::AssetInstance, crate::scene_view::SceneView>,
    pub asset_diagnostics: Vec<String>,
    pub(crate) animation: animation_panel::AnimationPanel,
    pub(crate) tool_dock: tool_dock::ToolDock,
    /// Live bounds of the closed panel launcher, excluded from viewport input.
    animation_keyboard: crate::input::timeline_input::TimelineKeyboard,
    scene_tick_time: Option<f64>,
    pub mesh: Option<MeshData>,
    pub mesh_revision: u64,
    derived_revision: u64,
    pub path: Option<PathBuf>,
    pub save_path: Option<PathBuf>,
    pub disk_snapshot: Option<Vec<u8>>,
    saved_document: Option<Document>,
    pub loading: Option<PathBuf>,
    pub error: Option<String>,
    /// Transient, nonblocking notifications; never saved in documents or history.
    pub toasts: Toaster,
    shown_shortcut_hints: BTreeSet<&'static str>,
    pending_ui_commands: Vec<Command>,
    pub hovered_file: bool,
    pub shading: crate::render::shading::ShadingMode,
    pub show_edges: bool,
    pub show_2d_ruler: bool,
    pub show_grid: bool,
    pub show_ui: bool,
    pub show_preferences: bool,
    preferences_focus_pending: bool,
    pub settings_error: Option<String>,
    pub settings_location: Option<String>,
    pub request_open_settings: bool,
    pub request_reload_settings: bool,
    pub z_up: bool,
    pub precise_scroll_zoom: bool,
    pub animate_views: bool,
    pub view_duration_ms: u32,
    pub request_save: bool,
    pub request_save_as: bool,
    pub request_new: bool,
    inspector_key: Option<(u64, u64)>,
    primitive_draft: Option<Primitive>,
    transform_draft: Transform,
    rotation_degrees: [f64; 3],
    vertex_delta: [f64; 3],
    property_session: Option<PropertySession>,
    layer_rename: Option<LayerRename>,
    last_layer_click: Option<(u64, f64)>,
    layer_rows: Vec<LayerRow>,
    preferences_rect: Option<egui::Rect>,
    mouse_navigation: NavigationInput,
    viewport_menu_position: egui::Pos2,
    mouse_escape_handled: bool,
    mouse_owns_primary_press: bool,
    primary_pointer_down: bool,
    held_navigation: HeldNavigation,
    navigation_frame_input: Vec<OrderedInput>,
    navigation_keys_available: bool,
    pie_input: PieInput,
    insert_menu_id: Option<egui::Id>,
    nudge_direction: Option<(i8, i8)>,
    trackpad_input: ScrollInput,
    navigation: ViewNavigation,
    local_view: Option<LocalView>,
    gizmo_planar_entry: Option<ViewNavigation>,
    pub return_3d: PlanarExit,
    pub display_unit: LengthUnit,
    pub theme_mode: ThemeMode,
    pub accent_color: AccentColor,
    system_theme: ResolvedTheme,
    applied_style: Option<(ResolvedTheme, AccentColor)>,
}

pub fn configure_context(context: &egui::Context) {
    typography::install(context);
    lucide::install(context);
    theme::apply_context(context, ResolvedTheme::Light, AccentColor::DEFAULT);
}

impl WorkspaceUi {
    pub fn new(scene_texture: egui::TextureId) -> Self {
        let mut state = Self {
            host_capabilities: HostCapabilities::default(),
            scene_texture,
            n3_logo_texture: None,
            camera: Camera::default(),
            viewport: egui::Rect::NOTHING,
            viewport_ui_rect: egui::Rect::NOTHING,
            ruler_2d_model: None,
            ruler_2d_selection: None,
            editor: Editor::new(Document::default()).expect("empty document is valid"),
            asset_views: BTreeMap::new(),
            asset_diagnostics: Vec::new(),
            animation: animation_panel::AnimationPanel::default(),
            tool_dock: tool_dock::ToolDock::default(),
            animation_keyboard: Default::default(),
            scene_tick_time: None,
            mesh: None,
            mesh_revision: 0,
            derived_revision: 0,
            path: None,
            save_path: None,
            disk_snapshot: None,
            saved_document: Some(Document::default()),
            loading: None,
            error: None,
            toasts: Toaster::default(),
            shown_shortcut_hints: BTreeSet::new(),
            pending_ui_commands: Vec::new(),
            hovered_file: false,
            shading: crate::render::shading::ShadingMode::Solid,
            show_edges: true,
            show_2d_ruler: true,
            show_grid: true,
            show_ui: true,
            show_preferences: false,
            preferences_focus_pending: false,
            settings_error: None,
            settings_location: None,
            request_open_settings: false,
            request_reload_settings: false,
            z_up: false,
            precise_scroll_zoom: false,
            animate_views: true,
            view_duration_ms: DEFAULT_VIEW_DURATION_MS,
            request_save: false,
            request_save_as: false,
            request_new: false,
            inspector_key: None,
            primitive_draft: None,
            transform_draft: Transform::default(),
            rotation_degrees: [0.; 3],
            vertex_delta: [0.; 3],
            property_session: None,
            layer_rename: None,
            last_layer_click: None,
            layer_rows: Vec::new(),
            preferences_rect: None,
            mouse_navigation: NavigationInput::default(),
            viewport_menu_position: egui::Pos2::ZERO,
            mouse_escape_handled: false,
            mouse_owns_primary_press: false,
            primary_pointer_down: false,
            held_navigation: HeldNavigation::default(),
            navigation_frame_input: Vec::new(),
            navigation_keys_available: false,
            pie_input: PieInput::default(),
            insert_menu_id: None,
            nudge_direction: None,
            trackpad_input: ScrollInput::default(),
            navigation: ViewNavigation::default(),
            local_view: None,
            gizmo_planar_entry: None,
            return_3d: PlanarExit::default(),
            display_unit: LengthUnit::Centimeters,
            theme_mode: ThemeMode::System,
            accent_color: AccentColor::DEFAULT,
            system_theme: ResolvedTheme::Light,
            applied_style: None,
        };
        state.apply_user_settings(&crate::settings::Settings::default());
        state
    }

    pub fn set_system_theme(&mut self, theme: ResolvedTheme) {
        self.system_theme = theme;
    }

    pub fn resolved_theme(&self) -> ResolvedTheme {
        self.theme_mode.resolve(self.system_theme)
    }

    fn sync_theme(&mut self, ctx: &egui::Context) {
        let current = (self.resolved_theme(), self.accent_color);
        let egui_theme = match current.0 {
            ResolvedTheme::Light => egui::Theme::Light,
            ResolvedTheme::Dark => egui::Theme::Dark,
        };
        // egui-winit can also update Context on a native ThemeChanged event.
        // Explicit preferences must win even if that event arrives mid-session.
        if self.applied_style != Some(current) || ctx.theme() != egui_theme {
            theme::apply_context(ctx, current.0, current.1);
            self.applied_style = Some(current);
            ctx.request_repaint();
        }
    }

    pub fn install_loaded_document(
        &mut self,
        path: PathBuf,
        loaded: crate::asset_io::LoadedDocument,
    ) -> Result<(), String> {
        let crate::asset_io::LoadedDocument {
            document,
            assets,
            mut diagnostics,
            saved_bytes: snapshot,
        } = loaded;
        let native = snapshot.is_some();
        let views = asset_instances::prepare_views(&document, &assets, &mut diagnostics);
        crate::asset_io::validate_resource_cache(&assets)?;
        let mut editor = Editor::new(document.clone())?;
        editor.snapping = self.editor.snapping;
        editor.snap_policy = self.editor.snap_policy;
        editor.set_access(self.editor.access());
        let frames = views
            .iter()
            .map(|(key, view)| (key.clone(), view.frame.clone()))
            .collect();
        editor.frame = DisplayFrame::from_document_with_assets(&document, &frames)?;
        editor.set_asset_frames(frames)?;
        if !document.objects.is_empty() {
            editor.render_mesh()?;
        }
        self.asset_views = views;
        self.animation = animation_panel::AnimationPanel::default();
        self.asset_diagnostics = diagnostics;
        self.scene_tick_time = None;
        self.editor = editor;
        self.local_view = None;
        self.ruler_2d_selection = None;
        self.ruler_2d_model = None;
        self.reset_navigation_input();
        self.layer_rows.clear();
        self.save_path = native.then(|| path.clone());
        self.disk_snapshot = snapshot;
        self.saved_document = native.then_some(document);
        self.path = Some(path);
        self.loading = None;
        self.error = None;
        self.hovered_file = false;
        self.inspector_key = None;
        self.property_session = None;
        self.layer_rename = None;
        self.last_layer_click = None;
        self.z_up = false;
        self.camera = Camera::default();
        self.navigation = ViewNavigation::default();
        self.camera.frame(self.aspect());
        self.derived_revision = u64::MAX;
        self.refresh_mesh()
    }
    pub fn new_document(&mut self) {
        self.animation = animation_panel::AnimationPanel::default();
        self.asset_views.clear();
        self.asset_diagnostics.clear();
        self.scene_tick_time = None;
        self.reset_navigation_input();
        let snapping = self.editor.snapping;
        let snap_policy = self.editor.snap_policy;
        let access = self.editor.access();
        self.editor = Editor::new(Document::default()).expect("empty document");
        self.editor.set_access(access);
        self.local_view = None;
        self.editor.snapping = snapping;
        self.editor.snap_policy = snap_policy;
        self.ruler_2d_selection = None;
        self.ruler_2d_model = None;
        self.layer_rows.clear();
        self.path = None;
        self.loading = None;
        self.hovered_file = false;
        self.save_path = None;
        self.disk_snapshot = None;
        self.saved_document = Some(Document::default());
        self.inspector_key = None;
        self.property_session = None;
        self.layer_rename = None;
        self.last_layer_click = None;
        self.error = None;
        self.camera = Camera::default();
        self.navigation = ViewNavigation::default();
        self.z_up = false;
        self.derived_revision = u64::MAX;
        if let Err(e) = self.refresh_mesh() {
            self.error = Some(e)
        }
    }
    pub fn is_dirty(&self) -> bool {
        self.saved_document.as_ref() != Some(&self.editor.document)
    }

    /// Native dialogs reset input and may lose focus. Resolve a transform preview
    /// explicitly before any host action can persist or replace the document.
    pub fn document_action_allowed(&mut self) -> bool {
        if self.editor.has_transform_session() {
            self.error = Some(pending_transform_message());
            false
        } else {
            true
        }
    }

    fn refresh_ruler_2d_selection(&mut self) -> Result<(), String> {
        let current = self.ruler_2d_selection.as_ref().is_some_and(|cached| {
            cached.revision == self.editor.revision
                && cached.frame == self.editor.frame
                && cached.active_object == self.editor.selected_object
                && cached.objects == self.editor.selected_objects
                && cached.vertices == self.editor.selected_vertices
                && cached.edit_mode == self.editor.edit_mode
                && cached.z_up == self.z_up
        });
        if current {
            return Ok(());
        }
        let groups = self.editor.selection_point_groups(self.z_up)?;
        let corners = groups
            .into_iter()
            .map(|points| {
                let min = points
                    .iter()
                    .copied()
                    .fold(glam::Vec3::splat(f32::INFINITY), glam::Vec3::min);
                let max = points
                    .iter()
                    .copied()
                    .fold(glam::Vec3::splat(f32::NEG_INFINITY), glam::Vec3::max);
                (0..8)
                    .map(|corner| {
                        glam::Vec3::new(
                            if corner & 1 == 0 { min.x } else { max.x },
                            if corner & 2 == 0 { min.y } else { max.y },
                            if corner & 4 == 0 { min.z } else { max.z },
                        )
                    })
                    .collect()
            })
            .collect();
        self.ruler_2d_selection = Some(Ruler2DSelectionCache {
            revision: self.editor.revision,
            frame: self.editor.frame,
            active_object: self.editor.selected_object,
            objects: self.editor.selected_objects.clone(),
            vertices: self.editor.selected_vertices.clone(),
            edit_mode: self.editor.edit_mode,
            z_up: self.z_up,
            corners,
        });
        Ok(())
    }

    pub fn object_highlights(&self) -> ObjectHighlights {
        if self.editor.edit_mode {
            return ObjectHighlights::default();
        }
        let exists = |id: &u64| {
            self.editor.is_object_visible(*id)
                && self
                    .editor
                    .document
                    .objects
                    .iter()
                    .any(|object| object.id == *id)
        };
        ObjectHighlights {
            selected: self
                .editor
                .selected_objects
                .iter()
                .copied()
                .filter(exists)
                .collect(),
            hovered: self
                .editor
                .hovered_object
                .filter(|id| !self.editor.is_interacting() && exists(id)),
        }
    }

    /// Component feedback is transient view state. It never changes the mesh
    /// revision or document history, and uses the same payload in both hosts.
    pub fn edit_selection(&self) -> EditSelection {
        if !self.editor.edit_mode {
            return EditSelection::default();
        }
        EditSelection {
            object: self.editor.selected_object,
            vertices: self.editor.selected_vertices.clone(),
        }
    }

    pub fn layer_row_color(&self, id: u64) -> Option<egui::Color32> {
        if !self
            .editor
            .document
            .objects
            .iter()
            .any(|object| object.id == id)
        {
            return None;
        }
        let highlights = self.object_highlights();
        // The active object's row stays selected while editing its vertices;
        // only viewport object outlines and object hover are suppressed there.
        object_feedback::color(
            self.editor.selected_objects.contains(&id),
            highlights.hovered == Some(id),
        )
    }
    /// A host calls this only after persistence has acknowledged the exact bytes.
    #[allow(dead_code)] // Download-only hosts cannot acknowledge a completed save.
    pub fn mark_saved(&mut self, path: PathBuf, bytes: Vec<u8>) {
        self.path = Some(path.clone());
        self.save_path = Some(path);
        self.disk_snapshot = Some(bytes);
        self.saved_document = Some(self.editor.document.clone());
    }
    pub fn refresh_mesh(&mut self) -> Result<(), String> {
        self.reconcile_local_view();
        if self.derived_revision != self.editor.revision {
            self.mesh = if self.editor.document.objects.is_empty() {
                None
            } else {
                Some(self.editor.render_mesh()?)
            };
            self.derived_revision = self.editor.revision;
            self.mesh_revision = self.mesh_revision.wrapping_add(1);
        }
        Ok(())
    }
    pub fn aspect(&self) -> f32 {
        self.viewport.width().max(1.) / self.viewport.height().max(1.)
    }
    pub fn frame_all(&mut self) {
        // View commands cannot commit or discard an in-progress edit.
        if self.editor.blocks_navigation() {
            return;
        }
        if let Some(local) = &self.local_view {
            match local_view::object_points(&self.editor, &local.members, self.z_up) {
                Ok(points) => {
                    self.camera.frame_points(&points, self.aspect());
                    self.cancel_trackpad_scroll();
                }
                Err(error) => self.error = Some(error),
            }
            return;
        }
        if self.editor.has_transform_session() {
            // Reframing the document changes render normalization and cancels
            // edits. Fit the preview in its existing frame instead, keeping
            // both the session baseline and subsequent drag coordinates intact.
            let members = self
                .editor
                .document
                .objects
                .iter()
                .map(|object| object.id)
                .collect();
            match local_view::object_points(&self.editor, &members, self.z_up) {
                Ok(points) => {
                    self.camera.frame_points(&points, self.aspect());
                    self.cancel_trackpad_scroll();
                }
                Err(error) => self.error = Some(error),
            }
            return;
        }
        if let Err(e) = self.editor.reframe() {
            self.error = Some(e);
            return;
        }
        self.camera.frame(self.aspect());
        self.navigation.leave_planar();
        self.ruler_2d_model = None;
        self.cancel_trackpad_scroll();
    }
    fn insert_shape(&mut self, kind: PrimitiveKind) {
        let planar_shape = matches!(kind, PrimitiveKind::Plane | PrimitiveKind::Circle)
            && self.is_planar_navigation();
        let result = match kind {
            kind if planar_shape => {
                self.finish_planar_transition();
                let display_normal = self.camera.nearest_axis_direction();
                let source_normal = crate::orientation::display_rotation(self.z_up)
                    .inverse()
                    .transform_vector3(display_normal)
                    .as_dvec3();
                let rotation = glam::DQuat::from_rotation_arc(glam::DVec3::Z, source_normal);
                self.editor.insert_with_rotation(kind, rotation)
            }
            kind => self.editor.insert(kind),
        };
        if result.is_ok() {
            if planar_shape {
                self.frame_selection();
            } else {
                self.frame_all();
            }
        }
        self.report(result);
    }
    pub fn frame_selection(&mut self) {
        if self.editor.blocks_navigation() {
            return;
        }
        match self.editor.selection_points(self.z_up) {
            Ok(points) => {
                let mut camera = self.camera.clone();
                if self.is_planar_navigation() {
                    camera.finish_transition();
                }
                if camera.frame_points(&points, self.aspect()) {
                    self.camera = camera;
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub fn is_local_view(&self) -> bool {
        self.local_view.is_some()
    }

    pub fn visible_objects(&self) -> Option<&BTreeSet<u64>> {
        self.editor.visible_objects()
    }

    /// Isolation is a view state, not a document edit. The first toggle saves
    /// the camera destination/navigation state; the second animates back to it.
    pub fn toggle_local_view(&mut self) {
        if self.editor.is_interacting() {
            return;
        }
        if self.local_view.is_some() {
            self.exit_local_view();
            return;
        }
        let Some(local) = LocalView::new(&self.editor, &self.camera, &self.navigation) else {
            return;
        };
        let points = match local_view::object_points(&self.editor, &local.members, self.z_up) {
            Ok(points) => points,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        self.cancel_trackpad_scroll();
        let mut destination = local.camera.clone();
        if destination.frame_points(&points, self.aspect()) {
            self.camera
                .transition_to(&destination, self.view_transition());
        }
        self.editor.set_visible_objects(Some(local.members.clone()));
        self.local_view = Some(local);
        self.ruler_2d_model = None;
    }

    fn exit_local_view(&mut self) {
        if let Some(local) = self.local_view.take() {
            self.camera
                .transition_to(&local.camera, self.view_transition());
            self.navigation = local.navigation;
            self.editor.set_visible_objects(None);
            self.ruler_2d_model = None;
            self.cancel_trackpad_scroll();
        }
    }

    fn reconcile_local_view(&mut self) {
        let Some(local) = &mut self.local_view else {
            return;
        };
        let visible = local.reconcile(&self.editor);
        if visible.is_empty() {
            // Deleting the final isolated object must not strand an empty view.
            self.exit_local_view();
        } else {
            self.editor.set_visible_objects(Some(visible));
        }
    }
    pub fn view_transition(&self) -> Transition {
        if self.animate_views {
            Transition::Animated {
                duration: Duration::from_millis(self.view_duration_ms.into()),
            }
        } else {
            Transition::Instant
        }
    }
    pub fn set_view(&mut self, view: View) {
        if self.editor.blocks_navigation() {
            return;
        }
        self.cancel_trackpad_scroll();
        if matches!(view, View::Perspective) {
            self.navigation.leave_planar();
            self.ruler_2d_model = None;
        } else {
            self.navigation.enter_planar(&self.camera);
        }
        match self.view_transition() {
            Transition::Instant => self.camera.set_view(view),
            t => self.camera.set_view_with_transition(view, t),
        }
    }

    pub fn is_planar_navigation(&self) -> bool {
        self.navigation.is_planar()
    }

    fn has_planar_view(&self) -> bool {
        // Navigation owns the mode; intermediate camera angles do not leave 2D.
        // Wait for perspective entry to finish, but keep rulers through 2D snaps
        // and while a gizmo press pauses their animation.
        self.is_planar_navigation() && self.camera.is_orthographic()
    }

    fn finish_planar_transition(&mut self) {
        if self.is_planar_navigation() {
            self.camera.finish_transition();
        }
    }

    pub fn set_planar_navigation(&mut self, planar: bool) {
        if self.editor.blocks_navigation() {
            return;
        }
        self.cancel_trackpad_scroll();
        if planar {
            if !self.is_planar_navigation() {
                self.show_2d_ruler = true;
            }
            // Remember the Free orientation before any axis/projection change.
            // Moving between Planar axes must not replace the return view.
            self.navigation.enter_planar(&self.camera);
            if !ruler_2d::eligible(&self.camera, self.z_up) {
                let direction = self.camera.nearest_axis_direction();
                self.camera
                    .look_from_with_transition(direction, self.view_transition());
            }
        } else {
            let transition = self.view_transition();
            self.navigation
                .return_to_free(&mut self.camera, self.return_3d, transition);
            self.ruler_2d_model = None;
        }
    }

    fn toggle_projection(&mut self) {
        if self.editor.blocks_navigation() {
            return;
        }
        self.cancel_trackpad_scroll();
        self.camera.toggle_projection();
        if !self.camera.is_orthographic() {
            self.navigation.leave_planar();
            self.ruler_2d_model = None;
        }
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        if !self.editor.blocks_navigation()
            && !self.pie_owns_input()
            && dx.is_finite()
            && dy.is_finite()
            && (dx != 0.0 || dy != 0.0)
        {
            self.navigation.leave_planar();
            self.ruler_2d_model = None;
            self.camera.orbit(dx, dy)
        }
    }
    pub fn pan(&mut self, dx: f32, dy: f32) {
        if !self.editor.blocks_navigation()
            && !self.pie_owns_input()
            && dx.is_finite()
            && dy.is_finite()
            && (dx != 0.0 || dy != 0.0)
        {
            self.finish_planar_transition();
            self.camera.pan(dx, dy, self.viewport.height())
        }
    }
    fn zoom(&mut self, amount: f32, pointer: Option<egui::Pos2>) {
        if amount.is_finite() && amount != 0.0 {
            self.finish_planar_transition();
            let anchor = pointer
                .filter(|p| p.is_finite() && self.viewport.contains(*p))
                .map(|p| {
                    glam::Vec2::new(
                        2.0 * (p.x - self.viewport.center().x) / self.viewport.width(),
                        -2.0 * (p.y - self.viewport.center().y) / self.viewport.height(),
                    )
                });
            let aspect = self.aspect();
            self.navigation
                .zoom(&mut self.camera, amount, anchor, aspect);
        }
    }
    pub fn scroll(
        &mut self,
        dx: f32,
        dy: f32,
        precise: bool,
        modifiers: egui::Modifiers,
        pointer: Option<egui::Pos2>,
    ) {
        self.cancel_trackpad_scroll();
        if self.editor.blocks_navigation() || self.pie_owns_input() {
            return;
        }
        if !precise {
            self.zoom(dy * 0.12, pointer)
        } else {
            self.apply_scroll(self.precise_scroll_motion(modifiers), dx, dy, pointer);
        }
    }

    fn precise_scroll_motion(&self, modifiers: egui::Modifiers) -> ScrollMotion {
        if self.is_planar_navigation() && modifiers.command {
            ScrollMotion::Zoom
        } else if self.is_planar_navigation() || modifiers.shift {
            ScrollMotion::Pan
        } else if self.precise_scroll_zoom {
            ScrollMotion::Zoom
        } else {
            ScrollMotion::Orbit
        }
    }

    fn apply_scroll(
        &mut self,
        motion: ScrollMotion,
        dx: f32,
        dy: f32,
        pointer: Option<egui::Pos2>,
    ) {
        match motion {
            ScrollMotion::Pan => self.pan(dx, dy),
            ScrollMotion::Orbit => self.orbit(dx, dy),
            ScrollMotion::Zoom => self.zoom(dy * 0.008, pointer),
        }
    }

    /// Native precise-scroll phases and executable guides share this adapter.
    /// Keep the chosen motion through a gesture even after orbit leaves an axis.
    pub fn trackpad_scroll(
        &mut self,
        dx: f32,
        dy: f32,
        modifiers: egui::Modifiers,
        phase: ScrollPhase,
        pointer: Option<egui::Pos2>,
    ) {
        if self.editor.blocks_navigation() || self.pie_owns_input() {
            self.cancel_trackpad_scroll();
            return;
        }
        let desired = self.precise_scroll_motion(modifiers);
        if let Some(motion) = self.trackpad_input.update(
            phase,
            modifiers.shift,
            modifiers.command,
            desired,
            egui::vec2(dx, dy),
        ) {
            self.apply_scroll(motion, dx, dy, pointer);
        }
    }

    pub fn cancel_trackpad_scroll(&mut self) {
        self.trackpad_input.reset();
    }

    pub fn navigation_modifiers_changed(&mut self, modifiers: egui::Modifiers) {
        self.trackpad_input
            .modifiers_changed(modifiers.shift, modifiers.command);
    }
    pub fn pinch(&mut self, delta: f64, pointer: Option<egui::Pos2>) {
        if !self.editor.blocks_navigation() && !self.pie_owns_input() {
            self.zoom(delta as f32 * 1.5, pointer)
        }
    }
    /// Handle native trackpad twist separately from explicit orbit controls.
    pub fn trackpad_rotate(&mut self, degrees: f32) {
        // A user reproduced an unintended 2D exit with a two-finger twist on
        // macOS: RotationGesture used to go straight to orbit. Ignore twist
        // throughout Planar navigation, including a pending axis transition,
        // without interrupting scroll/pinch or their gesture ownership. Option-
        // left drag, right drag and gizmo orbit remain deliberate exit controls.
        // Synthetic tests verify this routing; device recognition/feel needs a
        // real trackpad check and cannot be established by headless replay.
        if self.is_planar_navigation() {
            return;
        }
        self.orbit(-degrees.to_radians() / 0.006, 0.)
    }
    /// Dispatch a resolved command once. UI ownership and key bindings live in
    /// `shortcuts`; selection/mode transitions belong to `Editor`.
    pub fn dispatch(
        &mut self,
        command: Command,
        ctx: &egui::Context,
        navigation_active: bool,
    ) -> HostEffect {
        // Notification feedback and focus must not finish a nudge or cancel
        // another operation. Share the visibility policy with the presenter.
        if matches!(command, Command::ShortcutHint { .. } | Command::FocusToasts) {
            if self.toasts_available(ctx)
                && !navigation_active
                && !ctx.input(|input| input.pointer.any_down())
                && shortcuts::viewport_keys_available(ctx)
            {
                if command == Command::FocusToasts {
                    self.toasts.focus(ctx);
                    return HostEffect::None;
                }
                // Explicit examples keep hints predictable. Labels and actions
                // come from the live binding catalog, not a second keymap.
                if let Command::ShortcutHint { binding } = command
                    && binding == "selection.all"
                    && self.shown_shortcut_hints.insert(binding)
                {
                    let action = crate::input::bindings::required(binding)
                        .command
                        .expect("Select all has a semantic action");
                    self.toasts.push(
                        Toast::new(format!(
                            "Looking for Select all? Use {}.",
                            shortcut_label(binding)
                        ))
                        .kind(ToastKind::Info)
                        .action(Control::SelectAll.label(), action),
                    );
                    ctx.request_repaint();
                }
            }
            return HostEffect::None;
        }
        if !matches!(command, Command::Nudge { .. } | Command::EndNudge { .. }) {
            self.nudge_direction = None;
        }
        if matches!(
            command,
            Command::New | Command::Open | Command::Import | Command::Save { .. } | Command::Quit
        ) && !self.document_action_allowed()
        {
            ctx.request_repaint();
            return HostEffect::None;
        }
        if self.editor.has_transform_session()
            && matches!(
                command,
                Command::SelectAll
                    | Command::DuplicateSelection
                    | Command::DeleteSelection
                    | Command::MakeFace
                    | Command::CycleSelection { .. }
            )
        {
            return HostEffect::None;
        }
        if let Some(action) = ActionId::from_command(command)
            && !self.action_state(action).enabled
        {
            return HostEffect::None;
        }
        match command {
            Command::ToggleAnimationPanel => {
                self.toggle_tool_dock_panel(ToolDockPanel::Animation, ctx)
            }
            Command::ToggleTerminalPanel => {
                self.toggle_tool_dock_panel(ToolDockPanel::Terminal, ctx)
            }
            Command::CloseToolDock => self.close_tool_dock(ctx),
            Command::ToggleAnimationPlayback => self.toggle_animation_playback(ctx),
            Command::Scene(action) => self.dispatch_asset_action(action, ctx),
            Command::ShortcutHint { .. } | Command::FocusToasts => {
                unreachable!("handled before editor dispatch")
            }
            Command::OpenPreferences => {
                if !self.show_preferences {
                    self.preferences_focus_pending = true;
                    // A floating window needs an initial keyboard owner too:
                    // until a field is clicked, Space must not reach the
                    // timeline that was focused underneath it.
                    ctx.memory_mut(|memory| memory.request_focus(Self::preferences_focus_id()));
                }
                self.show_ui = true;
                self.show_preferences = true;
                ctx.request_repaint();
            }
            Command::EndNudge {
                horizontal,
                vertical,
            } => {
                if self.nudge_direction == Some((horizontal, vertical)) {
                    self.nudge_direction = None;
                }
            }
            Command::Nudge {
                horizontal,
                vertical,
                fast,
                repeat,
            } => {
                let direction = (horizontal, vertical);
                if self.move_keys_available(ctx, navigation_active)
                    && (!repeat || self.nudge_direction == Some(direction))
                    && let Some(delta) = move_input::nudge_delta(
                        &self.camera,
                        self.z_up,
                        self.editor.transform_axis,
                        horizontal,
                        vertical,
                        move_input::DEFAULT_NUDGE_CM
                            * if fast {
                                move_input::COARSE_NUDGE_MULTIPLIER
                            } else {
                                1.0
                            },
                    )
                {
                    let result = self.editor.nudge(delta, repeat);
                    self.nudge_direction = result
                        .as_ref()
                        .ok()
                        .filter(|changed| **changed)
                        .map(|_| direction);
                    self.report(result);
                    ctx.request_repaint();
                } else {
                    self.nudge_direction = None;
                }
            }
            Command::ToggleTransformAxis(axis) => {
                if self.transform_keys_available(ctx, navigation_active) {
                    let result = self.editor.toggle_transform_axis(axis);
                    self.report(result);
                    ctx.request_repaint();
                }
            }
            Command::TransformCharacter(character) => {
                if self
                    .transform_keyboard_context(ctx, navigation_active)
                    .can_transform
                    && self.editor.numeric_input_available()
                {
                    let result = self.editor.numeric_input(character);
                    self.report(result);
                    ctx.request_repaint();
                }
            }
            Command::TransformBackspace => {
                if self
                    .transform_keyboard_context(ctx, navigation_active)
                    .can_transform
                    && self.editor.numeric_input_available()
                {
                    let result = self.editor.numeric_backspace();
                    self.report(result);
                    ctx.request_repaint();
                }
            }
            Command::Open => return HostEffect::Open,
            Command::Import => return HostEffect::Import,
            Command::Insert(kind) => self.insert_shape(kind),
            Command::InsertMenu => {
                // Opening a menu cannot cancel or commit an in-progress edit.
                if self.editor.can_edit()
                    && !self.editor.is_interacting()
                    && !navigation_active
                    && !self.mouse_navigation.wants_input()
                    && !self.pie_owns_input()
                    && !ctx.egui_is_using_pointer()
                    && !ctx.input(|input| input.pointer.any_down())
                    && shortcuts::viewport_keys_available(ctx)
                    && self.insert_menu_id.is_some()
                {
                    self.open_insert_menu(ctx);
                }
            }
            Command::Quit => return HostEffect::Quit,
            Command::New => self.request_new = true,
            Command::Save { save_as } => {
                self.request_save = true;
                self.request_save_as = save_as;
            }
            Command::Undo => {
                self.editor.undo();
            }
            Command::Redo => {
                self.editor.redo();
            }
            Command::ToggleUi => {
                self.editor.finish_property_edit(true);
                self.property_session = None;
                self.inspector_key = None;
                self.show_ui = !self.show_ui;
                if !self.show_ui {
                    self.show_preferences = false;
                    self.pie_input.reset();
                    self.layer_rename = None;
                }
                ctx.memory_mut(|memory| memory.request_focus(shortcuts::viewport_focus_id()));
                ctx.request_repaint();
            }
            Command::SelectAll => {
                self.cancel_mouse_navigation();
                let result = self
                    .editor
                    .select_all(self.viewport, &self.camera, self.z_up);
                self.report(result);
                ctx.stop_dragging();
                ctx.request_repaint();
            }
            Command::DuplicateSelection => {
                // Duplicate is an idle object action: an unfinished edit or
                // camera gesture must not be implicitly committed or cancelled.
                if self.editor.can_duplicate_selection()
                    && !navigation_active
                    && !self.mouse_navigation.wants_input()
                {
                    let result = self.editor.duplicate_selection();
                    self.report(result);
                    ctx.request_repaint();
                }
            }
            Command::DeleteSelection => {
                self.cancel_mouse_navigation();
                let result = self.editor.delete_selection();
                self.report(result);
                ctx.stop_dragging();
                ctx.request_repaint();
            }
            Command::MakeFace => {
                // An immediate topology edit must not cancel an armed transform,
                // publish a preview, or steal a held camera/selection gesture.
                if self.editor.edit_mode
                    && !self.editor.is_interacting()
                    && self.editor.transform_axis.is_none()
                    && !navigation_active
                    && !self.mouse_navigation.wants_input()
                    && !self.pie_owns_input()
                    && self.held_navigation.preferred().is_none()
                    && !ctx.input(|input| input.pointer.any_down())
                {
                    match self.editor.make_face() {
                        Ok(false) => {
                            self.error = None;
                            self.toasts
                                .push(Toast::new("A face already exists for these vertices."));
                        }
                        result => self.report(result),
                    }
                    ctx.request_repaint();
                }
            }
            Command::CycleSelection { reverse } => {
                self.cancel_mouse_navigation();
                let result =
                    self.editor
                        .cycle_selection(reverse, self.viewport, &self.camera, self.z_up);
                self.report(result);
                ctx.stop_dragging();
                ctx.request_repaint();
            }
            Command::Frame => self.frame_all(),
            Command::FrameSelection => self.frame_selection(),
            Command::ToggleLocalView => {
                if !navigation_active
                    && !self.mouse_navigation.wants_input()
                    && !self.pie_owns_input()
                    && !ctx.input(|input| input.pointer.any_down())
                {
                    self.toggle_local_view();
                    ctx.request_repaint();
                }
            }
            Command::OrbitView {
                horizontal,
                vertical,
            } => {
                if !self.editor.blocks_navigation() {
                    self.cancel_trackpad_scroll();
                    self.navigation.leave_planar();
                    self.ruler_2d_model = None;
                    self.camera
                        .orbit_direction(horizontal, vertical, self.view_transition());
                }
            }
            Command::SetShading(mode) => {
                self.shading = mode;
                ctx.request_repaint();
            }
            Command::ToggleXray => {
                // Option may arm temporary orbit, but a chord without a pointer
                // gesture only changes selection depth. Never reinterpret a
                // marquee or navigation drag in progress, or finish its session.
                if !navigation_active
                    && !self.mouse_navigation.wants_input()
                    && !self.pie_owns_input()
                    && !ctx.input(|input| input.pointer.any_down())
                    && self.editor.set_xray(!self.editor.xray_enabled())
                {
                    ctx.request_repaint();
                }
            }
            Command::ToggleEdges => {
                self.show_edges = !self.show_edges;
                ctx.request_repaint();
            }
            Command::ToggleProjection => {
                self.toggle_projection();
            }
            Command::TogglePlanarNavigation => {
                // The gesture decides when to invoke this action; navigation
                // owns nearest-axis alignment and the configured 3D return.
                if !navigation_active
                    && !self.mouse_navigation.wants_input()
                    && !self.pie_owns_input()
                    && !ctx.egui_is_using_pointer()
                {
                    self.set_planar_navigation(!self.is_planar_navigation());
                    ctx.request_repaint();
                }
            }
            Command::ToggleRuler2D => {
                if self.is_planar_navigation() && !self.editor.blocks_navigation() {
                    self.show_2d_ruler = !self.show_2d_ruler;
                    self.ruler_2d_model = None;
                    ctx.request_repaint();
                }
            }
            Command::Tool(tool) => {
                self.editor.set_tool(tool);
                self.mouse_navigation.context_changed();
                return HostEffect::NavigationContextChanged;
            }
            Command::LeaveEdit => {
                if self.editor.has_transform_session() {
                    let result = self.editor.confirm();
                    self.report(result);
                    ctx.stop_dragging();
                } else {
                    self.editor.leave_edit();
                }
                self.mouse_navigation.context_changed();
                return HostEffect::NavigationContextChanged;
            }
            Command::Confirm => {
                self.mouse_navigation.context_changed();
                let was_interacting = self.editor.is_interacting();
                let result = self.editor.confirm();
                self.report(result);
                if was_interacting {
                    ctx.stop_dragging();
                }
                return HostEffect::NavigationContextChanged;
            }
            Command::Escape => {
                if self.show_preferences {
                    self.show_preferences = false;
                    self.preferences_focus_pending = false;
                    // Returning from a window is a focus transition, including
                    // retries that reprocess this same physical Escape.
                    shortcuts::claim_viewport_input(ctx);
                } else if !self.editor.blocks_navigation()
                    && (navigation_active
                        || self.mouse_navigation.wants_input()
                        || self.mouse_escape_handled
                        || ctx.egui_is_using_pointer())
                {
                    // A camera gesture temporarily owns input over a released
                    // transform. Escape ends that gesture, keeping the preview;
                    // a later idle Escape can cancel the transform itself.
                    ctx.stop_dragging();
                    self.cancel_mouse_navigation();
                    self.mouse_escape_handled = false;
                    return HostEffect::CancelNavigation;
                } else if self.editor.is_interacting() {
                    let session = self.editor.has_transform_session();
                    self.editor.escape();
                    if session {
                        self.error = None;
                    }
                    ctx.stop_dragging();
                } else {
                    self.editor.escape();
                }
            }
            Command::View(view) => self.set_view(view),
        }
        HostEffect::None
    }

    /// Menu and toast actions leave egui before dispatch, preserving native host effects
    /// and guaranteeing one execution even when layout requests another pass.
    pub fn take_ui_commands(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.pending_ui_commands)
    }
    fn move_keys_available(&self, ctx: &egui::Context, navigation_active: bool) -> bool {
        self.editor.tool == Tool::Move
            && self.editor.numeric_text().is_none()
            && self.transform_keys_available(ctx, navigation_active)
    }

    fn transform_keys_available(&self, ctx: &egui::Context, navigation_active: bool) -> bool {
        self.editor.tool != Tool::View
            && !self.editor.is_pointer_interacting()
            && !navigation_active
            && !self.mouse_navigation.wants_input()
            && !self.pie_owns_input()
            && self.held_navigation.preferred().is_none()
            && !ctx.egui_is_using_pointer()
            && !ctx.input(|input| input.pointer.any_down())
            && shortcuts::viewport_keys_available(ctx)
    }

    /// The keyboard recognizer gets selection eligibility separately from the
    /// current axis so it can route `E`, `D`, `9`, `0` in one event batch. A
    /// locked viewport drag may hand ownership to typing; other interactions
    /// (property fields, selection, navigation) retain their input.
    pub fn transform_keyboard_context(
        &self,
        ctx: &egui::Context,
        navigation_active: bool,
    ) -> shortcuts::TransformKeyboardContext {
        shortcuts::TransformKeyboardContext {
            tool: self.editor.tool,
            axis: self.editor.transform_axis,
            numeric_active: self.editor.numeric_text().is_some(),
            can_transform: self.editor.selected_object.is_some()
                && (!self.editor.edit_mode || !self.editor.selected_vertices.is_empty())
                && !self.editor.has_property_edit()
                && (!self.editor.is_pointer_interacting() || self.editor.is_transforming())
                && !navigation_active
                && !self.mouse_navigation.wants_input()
                && !self.pie_owns_input()
                && self.held_navigation.preferred().is_none()
                && (!ctx.egui_is_using_pointer() || self.editor.is_transforming())
                && shortcuts::viewport_keys_available(ctx),
        }
    }
    fn report<T>(&mut self, result: Result<T, String>) {
        self.error = result.err();
    }

    pub fn cancel_mouse_navigation(&mut self) {
        self.mouse_navigation.cancel();
    }

    /// Focus loss and document replacement end held input as well as gestures.
    /// Semantic commands only cancel the gesture; held keys retain their state.
    pub fn reset_navigation_input(&mut self) {
        self.cancel_trackpad_scroll();
        self.nudge_direction = None;
        self.cancel_mouse_navigation();
        self.held_navigation.reset();
        self.navigation_frame_input.clear();
        self.primary_pointer_down = false;
        self.gizmo_planar_entry = None;
        self.pie_input.reset();
    }

    pub fn pie_owns_input(&self) -> bool {
        self.pie_input.active() || self.pie_input.owns_frame
    }

    fn can_frame_selection(&self) -> bool {
        if self.editor.edit_mode {
            !self.editor.selected_vertices.is_empty()
        } else {
            !self.editor.selected_objects.is_empty()
        }
    }

    /// Leave the scene legible while its transient menu owns interaction.
    fn pie_underlay(&self, ui: &mut egui::Ui) {
        if self.pie_input.owns_frame {
            let opacity = ui.visuals().disabled_alpha;
            ui.visuals_mut().disabled_alpha = 1.0;
            ui.disable();
            ui.visuals_mut().disabled_alpha = opacity;
        }
    }

    pub fn mouse_navigation_active(&self) -> bool {
        self.mouse_navigation.wants_input()
    }

    /// Temporary overrides never replace the persistent editing tool. Resolve
    /// them only for an eligible viewport press; a future contextual Alt-drag
    /// action belongs at that same press boundary, not in the modifier binding.
    fn temporary_navigation_tool(&self) -> Option<NavigationTool> {
        if self.pie_owns_input() || self.editor.blocks_navigation() {
            return None;
        }
        if let Some(tool) = self.mouse_navigation.primary_drag_tool() {
            return self.held_navigation.held(tool).then_some(tool);
        }
        if self.primary_pointer_down || self.mouse_navigation.wants_input() {
            return None;
        }
        self.held_navigation.preferred()
    }

    pub fn hand_tool_active(&self) -> bool {
        self.temporary_navigation_tool() == Some(NavigationTool::Pan)
    }

    pub fn orbit_tool_active(&self) -> bool {
        self.temporary_navigation_tool() == Some(NavigationTool::Orbit)
    }

    pub fn temporary_navigation_active(&self) -> bool {
        self.hand_tool_active() || self.orbit_tool_active()
    }

    fn release_inactive_navigation(&mut self) {
        if self
            .mouse_navigation
            .primary_drag_tool()
            .is_some_and(|tool| !self.held_navigation.held(tool))
        {
            self.mouse_navigation.cancel_primary_drag();
        }
    }

    fn navigation_cursor(&self, ctx: &egui::Context) {
        let Some(tool) = self.temporary_navigation_tool() else {
            return;
        };
        if self.mouse_navigation.primary_drag_active() {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if ctx.pointer_hover_pos().is_some_and(|position| {
            crate::navigation_events::viewport_accepts_pointer(ctx, self.viewport_ui_rect, position)
        }) {
            ctx.set_cursor_icon(match tool {
                NavigationTool::Pan => egui::CursorIcon::Grab,
                NavigationTool::Orbit => egui::CursorIcon::AllScroll,
            });
        }
    }

    /// Native windows and executable guides use this same ordered pointer path.
    /// Trackpad scroll, pinch and rotation remain separate host gesture inputs.
    fn viewport_mouse(&mut self, ctx: &egui::Context) {
        if ctx.current_pass_index() != 0 {
            return;
        }
        self.mouse_escape_handled = false;
        if self.pie_input.owns_frame || self.tool_dock_owns_input(ctx) {
            self.nudge_direction = None;
            self.held_navigation.block();
            self.held_navigation
                .modifiers(ctx.input(|input| input.modifiers), false);
            self.navigation_frame_input.clear();
            self.primary_pointer_down = ctx.input(|input| input.pointer.primary_down());
            self.mouse_owns_primary_press = true;
            return;
        }
        // Preserve ownership on the release frame too: releasing a held key before
        // the pointer cannot expose a click or double-click to the editor.
        self.mouse_owns_primary_press = (self.mouse_owns_primary_press
            && self.primary_pointer_down)
            || self.mouse_navigation.primary_drag_active();
        let blocked = !ctx.input(|input| input.focused)
            || egui::Popup::is_any_open(ctx)
            || ctx.memory(|memory| memory.top_modal_layer().is_some());
        if blocked {
            self.reset_navigation_input();
            self.held_navigation
                .modifiers(ctx.input(|input| input.modifiers), false);
            return;
        }
        let keys_available =
            self.navigation_keys_available && shortcuts::viewport_keys_available(ctx);
        if !keys_available {
            self.nudge_direction = None;
            self.held_navigation.block();
            self.mouse_navigation.cancel_primary_drag();
        }
        let mut primary_owns_pointer = self.primary_pointer_down || self.editor.blocks_navigation();
        let mut motions = Vec::new();
        for input in std::mem::take(&mut self.navigation_frame_input) {
            let event = match input {
                OrderedInput::Modifiers(modifiers) => {
                    self.held_navigation.modifiers(modifiers, keys_available);
                    self.release_inactive_navigation();
                    continue;
                }
                OrderedInput::Event(event) => event,
            };
            match event {
                egui::Event::Key {
                    key,
                    pressed,
                    repeat,
                    modifiers,
                    ..
                } if Some(key) == crate::input::bindings::required("navigation.pan").key() => {
                    self.held_navigation
                        .space(pressed, repeat, modifiers, keys_available);
                    self.release_inactive_navigation();
                }
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    ..
                } => {
                    if pressed {
                        self.nudge_direction = None;
                    }
                    let over_viewport = self.viewport_ui_rect.contains(pos)
                        && !self
                            .tool_dock
                            .floating_tabs_rect
                            .is_some_and(|rect| rect.contains(pos))
                        && (!self.navigation_gizmo_visible()
                            || !axis_gizmo::bounds(self.viewport_ui_rect).contains(pos))
                        && ctx.layer_id_at(pos) == Some(egui::LayerId::background());
                    let tool = self.temporary_navigation_tool();
                    let navigation_press = pressed
                        && button == egui::PointerButton::Primary
                        && over_viewport
                        && tool.is_some();
                    if button == egui::PointerButton::Primary {
                        self.mouse_owns_primary_press |=
                            pressed && (navigation_press || self.mouse_navigation.wants_input());
                        self.primary_pointer_down = pressed;
                        primary_owns_pointer = pressed;
                    }
                    if navigation_press {
                        self.mouse_navigation
                            .queue_primary_press(tool.unwrap(), true, pos);
                    } else if pressed {
                        let eligible = !primary_owns_pointer
                            && !self.editor.blocks_navigation()
                            && over_viewport;
                        self.mouse_navigation.queue_press(button, eligible, pos);
                    } else {
                        self.mouse_navigation.queue_release(button, pos);
                    }
                }
                egui::Event::PointerMoved(pos) => self.mouse_navigation.queue_motion(pos),
                egui::Event::Key {
                    key: egui::Key::Escape,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if self.mouse_navigation.wants_input()
                    && (modifiers.is_none()
                        || (modifiers.alt
                            && !modifiers.ctrl
                            && !modifiers.command
                            && self.mouse_navigation.primary_drag_tool()
                                == Some(NavigationTool::Orbit))) =>
                {
                    self.cancel_mouse_navigation();
                    self.mouse_escape_handled = true;
                }
                egui::Event::PointerGone | egui::Event::WindowFocused(false) => {
                    self.reset_navigation_input()
                }
                _ => {}
            }
            // Advance ownership in event order, including releases before a
            // different button is pressed in the same native input batch.
            motions.extend(self.mouse_navigation.flush());
        }
        for motion in motions {
            match motion {
                NavigationMotion::Begin => {
                    // Pan completes a pending planar snap; orbit interrupts it.
                    // Keep the destination until the following motion decides.
                    if !self.is_planar_navigation() {
                        self.camera.cancel_transition();
                    }
                }
                NavigationMotion::Orbit(delta) => self.orbit(delta.x, delta.y),
                NavigationMotion::Pan(delta) => self.pan(delta.x, delta.y),
                NavigationMotion::ContextClick(pos) => {
                    if self.viewport_ui_rect.contains(pos)
                        && (!self.navigation_gizmo_visible()
                            || !axis_gizmo::bounds(self.viewport_ui_rect).contains(pos))
                    {
                        self.viewport_menu_position = pos;
                        egui::Popup::open_id(ctx, egui::Id::new(Control::ViewportMenu.id()));
                    }
                }
            }
            ctx.request_repaint();
        }
    }

    fn viewport_context_menu(&mut self, ctx: &egui::Context) {
        let popup = egui::Popup::new(
            egui::Id::new(Control::ViewportMenu.id()),
            ctx.clone(),
            self.viewport_menu_position,
            egui::LayerId::background(),
        )
        .kind(egui::PopupKind::Menu)
        .style(theme::menu_style)
        .open_memory(None)
        .layout(egui::Layout::top_down_justified(egui::Align::Min))
        .show(|ui| {
            menu::content(ui, Control::ViewportMenu, |ui| {
                for &action in crate::ui::workspace_menus::CONTEXT_EDIT_ACTIONS {
                    self.menu_action(ui, action, None);
                }
                menu::separator(ui);
                for &action in crate::ui::workspace_menus::CONTEXT_VIEW_ACTIONS {
                    let control = match action {
                        ActionId::FrameAll => Some(Control::ViewportFrame),
                        ActionId::Preferences => Some(Control::ViewportPreferences),
                        _ => None,
                    };
                    self.menu_action(ui, action, control);
                }
            })
        });
        if let Some(popup) = popup {
            controls::record(
                ctx,
                Control::ViewportMenu,
                Control::ViewportMenu.label(),
                popup.response.rect,
                true,
            );
        }
    }

    fn n3_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let logo = self.n3_logo_texture.get_or_insert_with(|| {
            let image = image::load_from_memory_with_format(N3_LOGO_PNG, image::ImageFormat::Png)
                .expect("bundled N3 logo is a valid PNG")
                .to_rgba8();
            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                [image.width() as usize, image.height() as usize],
                image.as_raw(),
            );
            ctx.load_texture(
                "n3-logo-01-crisp",
                color_image,
                egui::TextureOptions::LINEAR,
            )
        });
        let logo_id = logo.id();
        let (app_menu, _) = menu::dropdown(
            egui::Button::new("")
                .min_size(egui::Vec2::splat(N3_MENU_SIZE))
                .corner_radius(theme::radius::MD)
                .frame_when_inactive(false),
        )
        .ui(ui, |ui| {
            menu::content(ui, Control::N3Menu, |ui| {
                menu::submenu(ui, Control::FileMenu, |ui| {
                    for &action in crate::ui::workspace_menus::FILE_ACTIONS {
                        self.menu_action(ui, action, None);
                    }
                });
                ui.add_enabled_ui(!self.editor.blocks_navigation(), |ui| {
                    menu::submenu(ui, Control::ViewMenu, |ui| {
                        for action in [
                            ActionId::ViewPerspective,
                            ActionId::ViewFront,
                            ActionId::ViewRight,
                            ActionId::ViewBack,
                            ActionId::ViewLeft,
                            ActionId::ViewTop,
                            ActionId::ViewBottom,
                        ] {
                            self.menu_action(ui, action, None);
                        }
                        menu::separator(ui);
                        self.menu_action(ui, ActionId::LocalView, Some(Control::LocalViewMenu));
                        menu::separator(ui);
                        self.menu_action(ui, ActionId::FrameAll, Some(Control::Frame));
                        menu::separator(ui);
                        for &action in crate::ui::workspace_menus::VIEW_TOGGLE_ACTIONS {
                            self.menu_action(ui, action, None);
                        }
                    });
                });
                menu::separator(ui);
                self.menu_action(ui, ActionId::Preferences, None);
            })
        });
        // Paint inside the existing button so the menu geometry and hit target
        // stay independent of the logo's pixels.
        ui.painter().image(
            logo_id,
            egui::Rect::from_center_size(app_menu.rect.center(), egui::Vec2::splat(N3_LOGO_SIZE)),
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        app_menu.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                app_menu.enabled(),
                Control::N3Menu.label(),
            )
        });
        controls::record(
            ctx,
            Control::N3Menu,
            Control::N3Menu.label(),
            app_menu.rect,
            app_menu.enabled(),
        );
    }

    fn open_insert_menu(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.insert_menu_id {
            egui::Popup::open_id(ctx, id);
            ctx.request_repaint();
        }
    }

    fn viewport_empty_state(&mut self, viewport_ui: &egui::Ui) {
        if !self.editor.can_edit() {
            return;
        }
        if !self.show_ui
            || self.is_planar_navigation()
            || !self.editor.document.objects.is_empty()
            || self.hovered_file
            || !self.viewport_ui_rect.is_positive()
        {
            return;
        }
        let ctx = viewport_ui.ctx();
        let palette = Palette::from_context(ctx, self.accent_color);
        egui::Area::new(egui::Id::new("n3.viewport.empty-state"))
            .order(egui::Order::Middle)
            .fixed_pos(self.viewport_ui_rect.center() - egui::Vec2::Y * theme::size::XL_4)
            .pivot(egui::Align2::CENTER_CENTER)
            .show(ctx, |ui| {
                self.pie_underlay(ui);
                let button = ui.add_enabled(
                    !self.editor.is_interacting(),
                    egui::Button::new("")
                        .min_size(egui::Vec2::splat(theme::size::STEP_14))
                        .corner_radius(theme::radius::FULL)
                        .frame_when_inactive(false),
                );
                let center = button.rect.center();
                let arm = theme::size::MD;
                let stroke = egui::Stroke::new(2.0, palette.foreground);
                ui.painter().line_segment(
                    [center - egui::Vec2::X * arm, center + egui::Vec2::X * arm],
                    stroke,
                );
                ui.painter().line_segment(
                    [center - egui::Vec2::Y * arm, center + egui::Vec2::Y * arm],
                    stroke,
                );
                button.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        button.enabled(),
                        Control::EmptyStateInsert.label(),
                    )
                });
                controls::record(
                    ctx,
                    Control::EmptyStateInsert,
                    Control::EmptyStateInsert.label(),
                    button.rect,
                    button.enabled(),
                );
                if button.clicked() {
                    self.open_insert_menu(ctx);
                }
            });
        // Supporting copy does not own pointer input; only the + button does.
        let center = self.viewport_ui_rect.center();
        viewport_ui.painter().text(
            center + egui::Vec2::Y * theme::text::LG,
            egui::Align2::CENTER_CENTER,
            "Insert a shape",
            egui::FontId::proportional(theme::text::XL),
            palette.foreground,
        );
        viewport_ui.painter().text(
            center + egui::Vec2::Y * theme::size::STEP_10,
            egui::Align2::CENTER_CENTER,
            "or drop an OBJ, glTF or GLB file",
            egui::FontId::proportional(theme::text::SM),
            palette.muted_foreground,
        );
    }

    fn viewport_insert_menu(&mut self, ctx: &egui::Context, toolbar: egui::Rect) {
        if !self.viewport_ui_rect.is_positive() {
            return;
        }
        let idle = self.editor.can_edit() && !self.editor.has_transform_session();
        let palette = Palette::from_context(ctx, self.accent_color);
        egui::Area::new(egui::Id::new("n3.viewport.insert"))
            .order(egui::Order::Middle)
            .fixed_pos(egui::pos2(
                toolbar.left(),
                self.viewport_ui_rect.top() + theme::space::XL,
            ))
            .show(ctx, |ui| {
                self.pie_underlay(ui);
                // Reserve paint order before the button so its fill and border
                // cover the soft shadow without changing the hit target.
                let shadow = ui.painter().add(egui::Shape::Noop);
                // Keep a constant stroke width; the fill supplies hover feedback.
                let visuals = ui.visuals_mut();
                for widget in [
                    &mut visuals.widgets.inactive,
                    &mut visuals.widgets.open,
                    &mut visuals.widgets.hovered,
                    &mut visuals.widgets.active,
                ] {
                    widget.bg_stroke = egui::Stroke::new(1.0, palette.border);
                }
                visuals.widgets.inactive.weak_bg_fill = palette.card;
                visuals.widgets.open.weak_bg_fill = palette.accent;
                visuals.widgets.hovered.weak_bg_fill = palette.accent;
                visuals.widgets.active.weak_bg_fill = palette.accent;
                let menu = ui
                    .add_enabled_ui(idle, |ui| {
                        menu::dropdown(
                            egui::Button::new("")
                                .min_size(egui::Vec2::splat(toolbar.width()))
                                .corner_radius(theme::radius::FULL),
                        )
                        .ui(ui, |ui| {
                            menu::content(ui, Control::InsertMenu, |ui| {
                                for action in [
                                    ActionId::InsertCube,
                                    ActionId::InsertCylinder,
                                    ActionId::InsertCone,
                                    ActionId::InsertTorus,
                                    ActionId::InsertPlane,
                                    ActionId::InsertCircle,
                                    ActionId::InsertSphere,
                                    ActionId::InsertPolyhedron,
                                ] {
                                    self.menu_action(ui, action, None);
                                }
                            })
                        })
                    })
                    .inner
                    .0;
                ui.painter().set(
                    shadow,
                    ui.visuals()
                        .popup_shadow
                        .as_shape(menu.rect, egui::CornerRadius::same(theme::radius::FULL)),
                );
                // egui left-aligns button text inside a larger min_size. Paint
                // this simple mark from the measured button center instead.
                let center = menu.rect.center();
                let arm = theme::size::MD;
                let color = if menu.enabled() {
                    palette.card_foreground
                } else {
                    ui.visuals().weak_text_color()
                };
                let stroke = egui::Stroke::new(2.0, color);
                ui.painter().line_segment(
                    [center - egui::Vec2::X * arm, center + egui::Vec2::X * arm],
                    stroke,
                );
                ui.painter().line_segment(
                    [center - egui::Vec2::Y * arm, center + egui::Vec2::Y * arm],
                    stroke,
                );
                let popup_id = egui::Popup::default_response_id(&menu);
                self.insert_menu_id = Some(popup_id);
                menu.clone().on_hover_text(format!(
                    "{} — insert a shape",
                    shortcut_label("insert.open")
                ));
                menu.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        menu.enabled(),
                        Control::InsertMenu.label(),
                    )
                });
                controls::record(
                    ctx,
                    Control::InsertMenu,
                    Control::InsertMenu.label(),
                    menu.rect,
                    menu.enabled(),
                );
            });
    }

    pub fn ui(&mut self, root_ui: &mut egui::Ui) {
        let context = root_ui.ctx().clone();
        let ctx = &context;
        self.sync_theme(ctx);
        // run_ui creates the root before preferences and native appearance are
        // resolved. Panels inherit its style, so refresh that snapshot too.
        root_ui.set_style(ctx.global_style());
        self.tool_dock.floating_tabs_rect = None;
        self.prepare_tool_dock_frame(ctx);
        self.prepare_animation_frame(ctx);
        self.tick_assets(ctx);
        if ctx.current_pass_index() == 0 {
            // Snapshot the ordered host queue before a widget can consume a
            // key or modifier transition needed by held viewport navigation.
            let events = ctx.input(|input| (input.events.clone(), input.modifiers));
            self.navigation_frame_input = ordered_input(events.0, events.1);
        }
        if ctx.input(|input| input.pointer.primary_pressed())
            && ctx
                .input(|input| input.pointer.interact_pos())
                .is_some_and(|position| {
                    !self
                        .layer_rows
                        .iter()
                        .any(|row| row.rect.contains(position))
                })
        {
            self.last_layer_click = None;
        }
        // A click elsewhere accepts the current property preview before a menu,
        // tool, or viewport action can take ownership of that same press.
        if let Some(session) = &self.property_session {
            if !self.editor.has_property_edit() {
                self.property_session = None;
                self.inspector_key = None;
            } else if ctx.input(|input| {
                input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pos| input.pointer.any_pressed() && !session.rect.contains(pos))
            }) {
                self.editor.finish_property_edit(true);
                self.property_session = None;
                self.inspector_key = None;
            }
        }
        if ctx.current_pass_index() == 0 {
            // Snapshot before widgets can surrender focus or close a popup.
            // Held keys used by UI must not leak into the viewport in that pass.
            self.navigation_keys_available = shortcuts::viewport_keys_available(ctx);
            let commands = if self.show_ui {
                self.pie_input.begin(
                    ctx,
                    self.viewport_ui_rect,
                    PieContext {
                        keys_available: self.navigation_keys_available,
                        can_start: !self.editor.blocks_navigation()
                            && !self.tool_dock_owns_input(ctx)
                            && !self.mouse_navigation.wants_input(),
                        hand_held: self.held_navigation.held(NavigationTool::Pan),
                        fit_enabled: self.can_frame_selection(),
                        tool: self.editor.tool,
                    },
                )
            } else {
                Vec::new()
            };
            for command in commands {
                self.dispatch(command, ctx, false);
                ctx.request_repaint();
            }
        }
        controls::begin_pass(ctx);
        let feedback_before = (
            self.editor.selected_objects.clone(),
            self.editor.hovered_object,
            self.editor.edit_mode,
        );
        self.layer_rows.clear();
        self.preferences_rect = None;
        if !ctx.input(|i| i.focused) {
            self.editor.cancel();
            self.property_session = None;
            self.inspector_key = None;
            self.last_layer_click = None;
            // Reset held state without releasing an already captured cancellation
            // batch. The next PieInput::begin starts a fresh ownership frame.
            let pie_owns_frame = self.pie_input.owns_frame;
            self.reset_navigation_input();
            self.pie_input.owns_frame = pie_owns_frame;
        }
        // Undo, a tool change, or focus loss can also cancel the preview. An
        // earlier document-action notice must not outlive that transaction.
        if !self.editor.has_transform_session()
            && self
                .error
                .as_ref()
                .is_some_and(|error| *error == pending_transform_message())
        {
            self.error = None;
        }
        if self.show_ui {
            let status_bar = egui::Panel::bottom("status_bar")
                .exact_size(theme::size::XL_4)
                .frame(
                    egui::Frame::side_top_panel(root_ui.style()).inner_margin(
                        theme::space::symmetric_margin(theme::space::LG, theme::space::LG),
                    ),
                )
                .show(root_ui, |ui| {
                self.pie_underlay(ui);
                if self.orbit_tool_active() {
                    let key = shortcut_label("navigation.orbit");
                    ui.weak(format!("Orbit · {key} + left drag · Release {key} to return to your tool"));
                } else if self.hand_tool_active() {
                    let key = shortcut_label("navigation.pan");
                    ui.weak(format!("Pan · {key} + left drag · Release {key} to return to your tool"));
                } else if let Some(axis) = self.editor.transform_axis {
                    let color = theme::AXIS_COLORS[axis];
                    let mut operation = match self.editor.tool {
                        Tool::Move => "move · Type cm",
                        Tool::Rotate => "rotate · Type degrees",
                        Tool::Scale => "scale · Type a multiplier",
                        Tool::View => "",
                    }.to_owned();
                    if self.editor.numeric_text().is_some() {
                        operation.push_str(&format!(" · {}: edit", egui::Key::Backspace.name()));
                    } else if self.editor.tool == Tool::Move {
                        operation.push_str(&format!(" · {}: 1 cm · {}: 10 cm", shortcut_label("nudge.up"), shortcut_label("nudge.fast-up")));
                    }
                    let label = format!("{} {operation} · {} / double-click: apply · {}: cancel", ["X", "Y", "Z"][axis], shortcut_label("edit.confirm"), shortcut_label("cancel"));
                    let response = ui.colored_label(color, &label);
                    controls::record(ctx, Control::MoveAxisLock, &label, response.rect, true);
                } else if self.editor.has_transform_session() {
                    let label = format!("Transform preview · {} / {} / {}: axis · {} / double-click: apply · {}: cancel", shortcut_label("transform.axis-x"), shortcut_label("transform.axis-y"), shortcut_label("transform.axis-z"), shortcut_label("edit.confirm"), shortcut_label("cancel"));
                    let response = ui.colored_label(Palette::from_context(ctx, self.accent_color).primary, &label);
                    controls::record(ctx, Control::MoveAxisLock, &label, response.rect, true);
                } else if !self.editor.can_edit() {
                    ui.weak("Read-only · Select and navigate to inspect this document");
                } else if self.editor.edit_mode {
                    ui.weak(format!("Left drag: select · Middle: pan · Right drag: orbit · {}: finish · {}: step back", shortcut_label("edit.confirm"), shortcut_label("cancel")));
                } else {
                    ui.weak(format!("Left drag: box select · Middle: pan · Right drag: orbit · Right click: menu · {}: edit", shortcut_label("edit.confirm")));
                }
                });
            controls::record(
                ctx,
                Control::StatusBar,
                Control::StatusBar.label(),
                status_bar.response.rect,
                true,
            );
            self.hierarchy(root_ui);
            self.inspector(root_ui);
        }
        self.tool_dock_panel(root_ui);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(root_ui, |ui| {
                self.pie_underlay(ui);
                let outer = ui.available_rect_before_wrap();
                let show_rulers = self.show_ui
                    && self.show_2d_ruler
                    && self.has_planar_view()
                    && outer.width() > ruler_2d::THICKNESS * 2.0
                    && outer.height() > ruler_2d::THICKNESS * 2.0;
                // Ruler strips overlay the scene. Only UI placement and the
                // interactive area are inset; projection and render size stay fixed.
                self.viewport = outer;
                self.viewport_ui_rect = if show_rulers {
                    ruler_2d::content_rect(outer)
                } else {
                    outer
                };
                ui.put(
                    self.viewport,
                    egui::Image::new((self.scene_texture, self.viewport.size()))
                        .sense(egui::Sense::hover()),
                );
                self.viewport_empty_state(ui);
                // Establish overlay bounds before routing viewport presses.
                // egui Areas otherwise hit-test their previous-frame geometry.
                self.tool_dock.floating_tabs_rect = self.floating_tool_dock_tabs(ctx);
                let panel_tab_press = ctx.input(|input| {
                    input.events.iter().any(|event| {
                        matches!(event, egui::Event::PointerButton {
                        pos, button: egui::PointerButton::Primary, pressed: true, ..
                    } if self.tool_dock.floating_tabs_rect.is_some_and(|rect| rect.contains(*pos)))
                    })
                });
                let response = ui.interact(
                    self.viewport_ui_rect,
                    shortcuts::viewport_focus_id(),
                    egui::Sense::click_and_drag(),
                );
                let press = ctx.input(|input| {
                    input
                        .pointer
                        .any_pressed()
                        .then(|| input.pointer.interact_pos())
                        .flatten()
                });
                let eligible_press = press.is_some_and(|position| {
                    response.rect.contains(position)
                        && !self
                            .tool_dock
                            .floating_tabs_rect
                            .is_some_and(|rect| rect.contains(position))
                        && (!self.navigation_gizmo_visible()
                            || !axis_gizmo::bounds(response.rect).contains(position))
                        && ctx.layer_id_at(position) == Some(egui::LayerId::background())
                }) && !egui::Popup::is_any_open(ctx)
                    && !ctx.memory(|memory| memory.top_modal_layer().is_some());
                shortcuts::viewport_interaction(ui, &response, eligible_press);
                controls::record(
                    ctx,
                    Control::Viewport,
                    Control::Viewport.label(),
                    response.rect,
                    response.enabled(),
                );
                self.viewport_mouse(ui.ctx());
                if let Some(error) = self.editor.ui_with_navigation(
                    ui,
                    &response,
                    self.viewport,
                    &self.camera,
                    self.z_up,
                    self.pie_input.owns_frame
                        || self.tool_dock_owns_input(ctx)
                        || self.mouse_navigation.wants_input()
                        || self.mouse_owns_primary_press
                        || panel_tab_press,
                ) {
                    self.error = Some(error);
                }
                if self.editor.is_transforming() && self.editor.is_pointer_interacting() {
                    self.camera.cancel_transition();
                }
                if self.hovered_file {
                    ui.painter().text(
                        self.viewport_ui_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Release to open",
                        egui::FontId::proportional(theme::text::XL_2),
                        Palette::from_context(ui.ctx(), self.accent_color).muted_foreground,
                    );
                }
                if self.navigation_gizmo_visible() {
                    let transition = self.view_transition();
                    let planar = self.is_planar_navigation();
                    // A press freezes animated handles before it becomes a click
                    // or orbit. Preserve a prospective entry before that freeze,
                    // including the identity of an interrupted 3D return. Publish
                    // it only for a snap; dragging never recalls the saved angle.
                    let mut planar_entry = self.navigation.clone();
                    planar_entry.enter_planar(&self.camera);
                    let gizmo = ui
                        .add_enabled_ui(!self.editor.blocks_navigation(), |ui| {
                            axis_gizmo::show(
                                ui,
                                self.viewport_ui_rect,
                                &mut self.camera,
                                self.z_up,
                                planar,
                            )
                        })
                        .inner;
                    if ctx.current_pass_index() == 0
                        && gizmo.primary_held
                        && ctx.input(|input| input.pointer.primary_pressed())
                    {
                        self.gizmo_planar_entry = Some(planar_entry.clone());
                    }
                    if gizmo.open_preferences {
                        self.dispatch(Command::OpenPreferences, ctx, false);
                    }
                    if let Some(action) = gizmo.action {
                        match action {
                            axis_gizmo::GizmoAction::Snap(direction) => {
                                self.cancel_trackpad_scroll();
                                self.navigation =
                                    self.gizmo_planar_entry.take().unwrap_or(planar_entry);
                                self.camera.look_from_with_transition(direction, transition);
                            }
                            axis_gizmo::GizmoAction::Orbit => {
                                // A drag already owns the camera: continue from the
                                // visible pose instead of starting a return animation.
                                self.cancel_trackpad_scroll();
                                self.gizmo_planar_entry = None;
                                self.navigation.leave_planar();
                                self.ruler_2d_model = None;
                            }
                            axis_gizmo::GizmoAction::Navigation(planar) => {
                                self.set_planar_navigation(planar)
                            }
                            axis_gizmo::GizmoAction::ToggleProjection => {
                                self.dispatch(Command::ToggleProjection, ctx, false);
                                self.gizmo_planar_entry = None;
                            }
                        }
                        ctx.request_repaint();
                    }
                    if !gizmo.primary_held {
                        self.gizmo_planar_entry = None;
                    }
                    // A held axis click pauses the moving handles but remains in
                    // Planar until release, preserving the original 3D return view.
                    // A body click or edit can leave a frozen oblique camera.
                    if self.is_planar_navigation()
                        && !self.camera.is_transitioning()
                        && !ruler_2d::eligible(&self.camera, self.z_up)
                        && !gizmo.primary_held
                    {
                        self.navigation.leave_planar();
                        ctx.request_repaint();
                    }
                }
                if self.show_ui {
                    self.viewport_context_menu(ui.ctx());
                }
                self.ruler_2d_model = None;
                if show_rulers && self.has_planar_view() {
                    match self.refresh_ruler_2d_selection() {
                        Ok(()) => {
                            self.ruler_2d_model = Ruler2DModel::new_in_unit(
                                self.viewport,
                                &self.camera,
                                self.z_up,
                                &self.editor.frame,
                                &self.ruler_2d_selection.as_ref().unwrap().corners,
                                self.display_unit,
                            );
                        }
                        Err(error) => self.error = Some(error),
                    }
                    if let Some(model) = &self.ruler_2d_model {
                        ruler_2d::paint(ui, model);
                        let strips = [
                            (Control::Ruler2DHorizontal, model.horizontal.rect),
                            (Control::Ruler2DVertical, model.vertical.rect),
                        ];
                        for (control, rect) in strips {
                            let response =
                                ui.interact(rect, ui.id().with(control.id()), egui::Sense::click());
                            controls::record(ctx, control, control.label(), response.rect, true);
                            menu::context(&response).show(|ui| {
                                menu::content(ui, control, |ui| {
                                    self.menu_action(
                                        ui,
                                        ActionId::Ruler2D,
                                        Some(Control::Ruler2DHide),
                                    );
                                })
                            });
                        }
                    }
                }
            });
        if self.show_ui {
            let toolbar = self.viewport_toolbar(ctx);
            if let Some(toolbar) = toolbar {
                self.viewport_insert_menu(ctx, toolbar);
            }
            let scene_info = self.viewport_info(ctx);
            let overlay_stack = scene_info.or(self.tool_dock.floating_tabs_rect);
            let snap_feedback = self.viewport_transform_feedback(ctx, overlay_stack);
            self.viewport_error(ctx, snap_feedback.or(overlay_stack));
            self.preferences_window(ctx);
        }
        let toast_available = self.toasts_available(ctx);
        self.pending_ui_commands.extend(self.toasts.show(
            ctx,
            self.viewport_ui_rect,
            Palette::from_context(ctx, self.accent_color),
            toast_available,
        ));
        self.resolve_object_hover(ctx);
        self.navigation_cursor(ctx);
        self.paint_layer_rows();
        if self.show_ui {
            self.pie_input
                .paint(ctx, self.can_frame_selection(), self.shading);
        }
        if self.pie_input.owns_frame {
            // Disabled underlay widgets surrender focus while being laid out.
            // Return it after layout, including the menu's dismissal frame.
            shortcuts::claim_viewport_input(ctx);
        }
        if feedback_before
            != (
                self.editor.selected_objects.clone(),
                self.editor.hovered_object,
                self.editor.edit_mode,
            )
        {
            // The inspector precedes viewport interactions in panel layout.
            // Refresh it even when the native host otherwise renders on demand.
            ctx.request_repaint();
        }
        if let Err(e) = self.refresh_mesh() {
            self.error = Some(e)
        }
        menu::finish_frame(ctx);
    }

    fn toasts_available(&self, ctx: &egui::Context) -> bool {
        self.show_ui
            && ctx.input(|input| input.focused)
            && !self.show_preferences
            && !egui::Popup::is_any_open(ctx)
            && ctx.memory(|memory| memory.top_modal_layer().is_none())
            && !self.editor.is_interacting()
            && !self.mouse_navigation.wants_input()
            && !self.pie_owns_input()
            && self.held_navigation.preferred().is_none()
    }

    fn viewport_toolbar(&mut self, ctx: &egui::Context) -> Option<egui::Rect> {
        if !self.viewport_ui_rect.is_positive() {
            return None;
        }
        let area = egui::Area::new(egui::Id::new("n3.viewport.toolbar"))
            .order(egui::Order::Middle)
            .fixed_pos(egui::pos2(
                self.viewport_ui_rect.left() + theme::space::XL,
                self.viewport_ui_rect.top()
                    + theme::space::XL
                    + viewport_toolbar_width(ctx.global_style().as_ref())
                    + theme::space::LG,
            ))
            .show(ctx, |ui| {
                self.pie_underlay(ui);
                viewport_toolbar_frame(ui.style()).show(ui, |ui| {
                    controls::scope(ctx, Control::ViewportToolbar, || {
                        // egui includes frame stroke in button measurement. The
                        // tool selection and hover fills provide feedback here.
                        let visuals = ui.visuals_mut();
                        for widget in [
                            &mut visuals.widgets.inactive,
                            &mut visuals.widgets.open,
                            &mut visuals.widgets.hovered,
                            &mut visuals.widgets.active,
                        ] {
                            widget.bg_stroke = egui::Stroke::NONE;
                        }
                        ui.vertical(|ui| {
                            for (control, tool, icon) in [
                                (Control::ToolView, Tool::View, lucide::Icon::MousePointer2),
                                (Control::ToolMove, Tool::Move, lucide::Icon::Move3d),
                                (Control::ToolRotate, Tool::Rotate, lucide::Icon::Rotate3d),
                                (Control::ToolScale, Tool::Scale, lucide::Icon::Scale3d),
                            ] {
                                let response = ui.add_enabled(
                                    self.editor.can_edit() || tool == Tool::View,
                                    egui::Button::selectable(
                                        self.editor.tool == tool,
                                        icon.text(theme::text::TOOL_ICON_17),
                                    )
                                    .min_size(egui::Vec2::splat(theme::size::XL_4))
                                    .corner_radius(theme::radius::MD),
                                );
                                response.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::WidgetType::Button,
                                        response.enabled(),
                                        control.label(),
                                    )
                                });
                                let response = if tool == Tool::View {
                                    response.on_hover_text(format!(
                                        "{} {} ({} also works)",
                                        shortcut_label("tool.cursor"),
                                        control.label(),
                                        shortcut_label("tool.cursor-alternate")
                                    ))
                                } else {
                                    let binding = match tool {
                                        Tool::Move => "tool.move",
                                        Tool::Rotate => "tool.rotate",
                                        Tool::Scale => "tool.scale",
                                        Tool::View => unreachable!(),
                                    };
                                    response.on_hover_text(format!(
                                        "{} {}",
                                        shortcut_label(binding),
                                        control.label()
                                    ))
                                };
                                controls::record(
                                    ctx,
                                    control,
                                    control.label(),
                                    response.rect,
                                    response.enabled(),
                                );
                                if response.clicked() {
                                    self.nudge_direction = None;
                                    self.editor.set_tool(tool);
                                }
                            }
                        });
                    });
                });
            });
        controls::record(
            ctx,
            Control::ViewportToolbar,
            Control::ViewportToolbar.label(),
            area.response.rect,
            area.response.enabled(),
        );
        self.viewport_edit_toolbar(ctx);
        Some(area.response.rect)
    }

    fn viewport_edit_toolbar(&mut self, ctx: &egui::Context) {
        if !self.editor.edit_mode {
            return;
        }
        let area = egui::Area::new(egui::Id::new("n3.viewport.edit-toolbar"))
            .order(egui::Order::Middle)
            .fade_in(false)
            .pivot(egui::Align2::CENTER_BOTTOM)
            .fixed_pos(egui::pos2(
                self.viewport_ui_rect.center().x,
                self.viewport_ui_rect.bottom() - theme::space::XL,
            ))
            .show(ctx, |ui| {
                self.pie_underlay(ui);
                viewport_toolbar_frame(ui.style()).show(ui, |ui| {
                    controls::scope(ctx, Control::EditToolbar, || {
                        ui.horizontal(|ui| {
                            ui.label(theme::strong(Control::EditToolbar.label()));
                            ui.separator();
                            let response = ui
                                .add_enabled_ui(!self.editor.has_transform_session(), |ui| {
                                    ui.add_sized(
                                        egui::Vec2::splat(theme::size::STEP_7),
                                        egui::Button::new(lucide::Icon::X.text(theme::text::BASE))
                                            .corner_radius(theme::radius::MD),
                                    )
                                })
                                .inner
                                .on_hover_text(format!(
                                    "{} · {}",
                                    Control::LeaveEdit.label(),
                                    shortcut_label("edit.leave")
                                ));
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    response.enabled(),
                                    Control::LeaveEdit.label(),
                                )
                            });
                            controls::record(
                                ctx,
                                Control::LeaveEdit,
                                Control::LeaveEdit.label(),
                                response.rect,
                                response.enabled(),
                            );
                            if response.clicked() {
                                self.editor.leave_edit();
                            }
                        });
                    });
                });
            });
        controls::record(
            ctx,
            Control::EditToolbar,
            Control::EditToolbar.label(),
            area.response.rect,
            area.response.enabled(),
        );
    }

    fn viewport_info(&mut self, ctx: &egui::Context) -> Option<egui::Rect> {
        if !self.viewport_ui_rect.is_positive() {
            return None;
        }
        let palette = Palette::from_context(ctx, self.accent_color);
        // Paint in the viewport's own layer. A foreground Area, even with
        // interactable(false), still masks pointer hit testing for geometry.
        let painter = ctx.layer_painter(egui::LayerId::background());
        let mut lines = Vec::new();
        if self.editor.xray_enabled() {
            lines.push((
                format!("X-ray · {} to turn off", shortcut_label("view.xray")),
                palette.primary,
            ));
        }
        if let Some(path) = &self.loading {
            lines.push((
                format!("Opening {}…", filename(path)),
                palette.muted_foreground,
            ));
        }
        if let Some(visible) = self.visible_objects() {
            lines.push((
                format!(
                    "Local View · {} object{} · {} to exit",
                    visible.len(),
                    if visible.len() == 1 { "" } else { "s" },
                    shortcut_label("view.local")
                ),
                palette.primary,
            ));
        }
        if let Some(mesh) = &self.mesh {
            let color = palette.muted_foreground;
            lines.push((
                format!("{} vertices · {} faces", mesh.vertex_count, mesh.face_count),
                color,
            ));
            lines.push((
                format!(
                    "{} triangles · {} objects",
                    mesh.triangle_count, mesh.object_count
                ),
                color,
            ));
            let notice_count = mesh.warnings.len() + self.asset_diagnostics.len();
            if notice_count > 0 {
                lines.push((
                    format!("{} notice(s)", notice_count),
                    ctx.global_style().visuals.warn_fg_color,
                ));
            }
        }
        if !lines.is_empty() {
            let font = egui::FontId::proportional(theme::text::XS);
            let galleys: Vec<_> = lines
                .into_iter()
                .map(|(label, color)| (painter.layout_no_wrap(label, font.clone(), color), color))
                .collect();
            let width = galleys
                .iter()
                .map(|(galley, _)| galley.size().x)
                .fold(0.0_f32, f32::max);
            let height = galleys
                .iter()
                .map(|(galley, _)| galley.size().y)
                .sum::<f32>()
                + (theme::space::XS + theme::space::PX) * galleys.len().saturating_sub(1) as f32;
            let size = egui::vec2(width + theme::space::XL, height + theme::space::XL);
            let rect = egui::Rect::from_min_size(
                egui::pos2(
                    self.viewport_ui_rect.left() + theme::space::XL,
                    self.tool_dock
                        .floating_tabs_rect
                        .map_or(self.viewport_ui_rect.bottom() - theme::space::XL, |tabs| {
                            tabs.top() - theme::space::LG
                        })
                        - size.y,
                ),
                size,
            );
            painter.rect_filled(rect, theme::radius::MD, palette.workbench_hud);
            painter.rect_stroke(
                rect,
                theme::radius::MD,
                egui::Stroke::new(1.0, palette.workbench_hud_border),
                egui::StrokeKind::Inside,
            );
            let mut y = rect.top() + theme::space::MD;
            for (galley, color) in galleys {
                let line_height = galley.size().y;
                painter.galley(egui::pos2(rect.left() + theme::space::MD, y), galley, color);
                y += line_height + theme::space::XS + theme::space::PX;
            }
            controls::record(
                ctx,
                Control::SceneInfo,
                Control::SceneInfo.label(),
                rect,
                true,
            );
            return Some(rect);
        }
        None
    }

    fn viewport_error(&mut self, ctx: &egui::Context, stack_top: Option<egui::Rect>) {
        if let Some(error) = self.error.clone() {
            let bottom = stack_top
                .map_or(self.viewport_ui_rect.bottom() - theme::space::XL, |rect| {
                    rect.top() - theme::space::LG
                });
            egui::Area::new(egui::Id::new("n3.viewport.error"))
                .order(egui::Order::Foreground)
                .pivot(egui::Align2::LEFT_BOTTOM)
                .fixed_pos(egui::pos2(
                    self.viewport_ui_rect.left() + theme::space::XL,
                    bottom,
                ))
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                            if controls::button(ui, Control::Dismiss).clicked() {
                                self.error = None;
                            }
                        });
                    });
                });
        }
    }

    fn viewport_transform_feedback(
        &mut self,
        ctx: &egui::Context,
        scene_info: Option<egui::Rect>,
    ) -> Option<egui::Rect> {
        let moving_property = self
            .property_session
            .as_ref()
            .is_some_and(|session| session.field.translation_sensitivity().is_some());
        if !self.viewport_ui_rect.is_positive() {
            return None;
        }
        let palette = Palette::from_context(ctx, self.accent_color);
        let numeric = self.editor.numeric_text().is_some();
        // Paint only: the centered readout must not intercept a drag or the
        // double-click that accepts the transform underneath it.
        let painter = ctx
            .layer_painter(egui::LayerId::background())
            .with_clip_rect(self.viewport_ui_rect);
        let wrap_width = if numeric {
            (self.viewport_ui_rect.width() - 2.0 * theme::space::XL_4).clamp(40.0, 480.0)
        } else {
            (self.viewport_ui_rect.width() - 2.0 * theme::space::XL_3).clamp(40.0, 360.0)
        };
        let layout = |text: String, size, color, max_rows| {
            let mut job = egui::text::LayoutJob::simple(
                text,
                egui::FontId::proportional(size),
                color,
                wrap_width,
            );
            job.wrap.max_rows = max_rows;
            job.wrap.break_anywhere = true;
            painter.layout_job(job)
        };
        let (control, label, lines) = if let Some(text) = self.editor.numeric_text() {
            let axis = self
                .editor
                .transform_axis
                .map_or("", |index| ["X", "Y", "Z"][index]);
            let (operation, unit) = match self.editor.tool {
                Tool::Move => ("Move", " cm"),
                Tool::Rotate => ("Rotate", "°"),
                Tool::Scale => ("Scale", "×"),
                Tool::View => return None,
            };
            let value = if text.is_empty() { "…" } else { text };
            let mut label = format!("{operation} {axis}: {value}{unit}");
            let mut lines = vec![
                layout(
                    format!("{operation} · {axis}"),
                    theme::text::SM,
                    palette.muted_foreground,
                    1,
                ),
                layout(
                    format!("{value}{unit}"),
                    theme::text::XL_4,
                    palette.foreground,
                    1,
                ),
            ];
            if let Some(error) = self.editor.numeric_error() {
                label.push_str(&format!("\n{error}"));
                lines.push(layout(
                    error.to_owned(),
                    theme::text::SM,
                    ctx.global_style().visuals.error_fg_color,
                    3,
                ));
            }
            (Control::TransformValue, label, lines)
        } else {
            if self.editor.tool != Tool::Move && !moving_property {
                return None;
            }
            let step_cm = self
                .editor
                .movement_snap_step(self.viewport, &self.camera, self.z_up)?;
            let label = movement_snap_label(step_cm, self.display_unit);
            let galley = layout(
                label.clone(),
                theme::text::XS,
                palette.muted_foreground,
                usize::MAX,
            );
            (Control::SnapFeedback, label, vec![galley])
        };
        let padding = if numeric {
            egui::vec2(theme::space::XL_3, theme::space::XL + theme::space::XS)
        } else {
            egui::Vec2::splat(theme::space::MD)
        };
        let gap = if numeric {
            theme::space::SM
        } else {
            theme::space::NONE
        };
        let content = egui::vec2(
            lines
                .iter()
                .map(|line| line.size().x)
                .fold(0.0_f32, f32::max),
            lines.iter().map(|line| line.size().y).sum::<f32>() + gap * (lines.len() - 1) as f32,
        );
        let size = content + padding * 2.0;
        let rect = if numeric {
            egui::Rect::from_center_size(self.viewport_ui_rect.center(), size)
        } else {
            let bottom = scene_info
                .map_or(self.viewport_ui_rect.bottom() - theme::space::XL, |rect| {
                    rect.top() - theme::space::LG
                });
            egui::Rect::from_min_size(
                egui::pos2(
                    self.viewport_ui_rect.left() + theme::space::XL,
                    bottom - size.y,
                ),
                size,
            )
        };
        painter.rect_filled(
            rect,
            if numeric {
                theme::radius::XL
            } else {
                theme::radius::MD
            },
            palette.workbench_hud,
        );
        let mut y = rect.top() + padding.y;
        for galley in lines {
            let x = if numeric {
                rect.center().x - galley.size().x * 0.5
            } else {
                rect.left() + padding.x
            };
            let height = galley.size().y;
            painter.galley(egui::pos2(x, y), galley, palette.foreground);
            y += height + gap;
        }
        controls::record(ctx, control, &label, rect, true);
        (control == Control::SnapFeedback).then_some(rect)
    }
    fn hierarchy(&mut self, root_ui: &mut egui::Ui) {
        let context = root_ui.ctx().clone();
        let ctx = &context;
        egui::Panel::left("hierarchy")
            .resizable(true)
            .default_size(DEFAULT_PANEL_WIDTH)
            .size_range(HIERARCHY_MIN_WIDTH..=420.0)
            .frame(workspace_side_panel_frame(
                root_ui.style(),
                Palette::from_context(ctx, self.accent_color).sidebar,
            ))
            .show(root_ui, |ui| {
                apply_sidebar_colors(ui, Palette::from_context(ctx, self.accent_color));
                self.pie_underlay(ui);
                ui.spacing_mut().item_spacing.y = 0.0;
                workspace_panel_section(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.set_min_height(N3_MENU_SIZE);
                        self.n3_menu(ui, ctx);
                    });
                });
                workspace_panel_separator(ui);
                if self.editor.has_transform_session() {
                    ui.disable();
                }
                let panel = ui.max_rect();
                controls::record(
                    ctx,
                    Control::ObjectList,
                    Control::ObjectList.label(),
                    panel,
                    ui.is_enabled(),
                );
                workspace_panel_section(ui, |ui| {
                    ui.label(theme::strong(Control::ObjectList.label()));
                });
                workspace_panel_separator(ui);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    workspace_panel_section(ui, |ui| {
                        controls::scope(ctx, Control::ObjectList, || {
                            let objects: Vec<_> = self
                                .editor
                                .document
                                .objects
                                .iter()
                                .map(|o| (o.id, o.name.clone(), layer_icon(&o.geometry)))
                                .collect();
                            if objects.is_empty() {
                                ui.weak("No objects yet.");
                            }
                            for (id, name, icon) in objects {
                                let available = self.editor.is_object_visible(id);
                                if self
                                    .layer_rename
                                    .as_ref()
                                    .is_some_and(|rename| rename.id == id && available)
                                {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(
                                            ui.available_width(),
                                            ui.spacing().interact_size.y,
                                        ),
                                        egui::Sense::hover(),
                                    );
                                    if let Some(color) = self.layer_row_color(id) {
                                        ui.painter().rect_filled(
                                            rect,
                                            theme::radius::SM,
                                            color.gamma_multiply(0.18),
                                        );
                                        ui.painter().rect_stroke(
                                            rect,
                                            theme::radius::SM,
                                            egui::Stroke::new(1.0, color),
                                            egui::StrokeKind::Inside,
                                        );
                                    }
                                    icon.paint(
                                        ui.painter(),
                                        rect.left_center() + egui::vec2(LAYER_ICON_INSET, 0.0),
                                        ui.visuals().text_color(),
                                    );
                                    let edit_rect = egui::Rect::from_min_max(
                                        rect.min + egui::vec2(LAYER_TEXT_INSET, 0.0),
                                        rect.max,
                                    );
                                    let rename = self.layer_rename.as_mut().unwrap();
                                    let edit_id = egui::Id::new(("n3.layer.rename", id));
                                    if rename.select_all {
                                        let mut state = egui::TextEdit::load_state(ctx, edit_id)
                                            .unwrap_or_default();
                                        state.cursor.set_char_range(Some(
                                            egui::text::CCursorRange::two(
                                                egui::text::CCursor::new(0),
                                                egui::text::CCursor::new(
                                                    rename.value.chars().count(),
                                                ),
                                            ),
                                        ));
                                        egui::TextEdit::store_state(ctx, edit_id, state);
                                        rename.select_all = false;
                                    }
                                    let response = ui.place(
                                        edit_rect,
                                        egui::TextEdit::singleline(&mut rename.value)
                                            .id(edit_id)
                                            .font(egui::TextStyle::Body)
                                            .text_color(ui.visuals().text_color())
                                            .vertical_align(egui::Align::Center)
                                            .frame(egui::Frame::NONE)
                                            .min_size(edit_rect.size()),
                                    );
                                    controls::record(
                                        ctx,
                                        Control::LayerRename,
                                        Control::LayerRename.label(),
                                        response.rect,
                                        response.enabled(),
                                    );
                                    let enter =
                                        ui.input(|input| input.key_pressed(egui::Key::Enter));
                                    let escape =
                                        ui.input(|input| input.key_pressed(egui::Key::Escape));
                                    if enter {
                                        let value = rename.value.clone();
                                        response.surrender_focus();
                                        self.layer_rename = None;
                                        let result = self.editor.rename_object(id, value);
                                        self.report(result);
                                    } else if escape || response.lost_focus() {
                                        self.layer_rename = None;
                                    }
                                    continue;
                                }
                                let (rect, response) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                                    if available {
                                        egui::Sense::click()
                                    } else {
                                        egui::Sense::hover()
                                    },
                                );
                                let response = if available {
                                    response.on_hover_cursor(egui::CursorIcon::PointingHand)
                                } else {
                                    response.on_hover_text("Hidden in Local View")
                                };
                                let now = ui.input(|input| input.time);
                                let same_row_twice = response.double_clicked()
                                    && self.last_layer_click.is_some_and(|(previous, time)| {
                                        previous == id && now - time <= 0.45
                                    });
                                if same_row_twice && self.editor.can_edit() {
                                    self.layer_rename = Some(LayerRename {
                                        id,
                                        value: name.clone(),
                                        select_all: true,
                                    });
                                    self.last_layer_click = None;
                                    ui.memory_mut(|memory| {
                                        memory.request_focus(egui::Id::new(("n3.layer.rename", id)))
                                    });
                                } else if response.clicked() {
                                    self.last_layer_click = Some((id, now));
                                    let result = self.editor.select_object_with_modifier(
                                        id,
                                        ui.input(|input| input.modifiers.shift),
                                    );
                                    self.report(result);
                                    self.inspector_key = None;
                                }
                                response.widget_info(|| {
                                    egui::WidgetInfo::selected(
                                        egui::WidgetType::SelectableLabel,
                                        response.enabled() && available,
                                        self.editor.selected_objects.contains(&id),
                                        &name,
                                    )
                                });
                                let visible = rect.intersect(ui.clip_rect());
                                if visible.is_positive() {
                                    self.layer_rows.push(LayerRow {
                                        id,
                                        rect: visible,
                                        painter: ui.painter().with_clip_rect(visible),
                                        name,
                                        icon,
                                        text_position: rect.left_center()
                                            + egui::vec2(LAYER_TEXT_INSET, 0.0),
                                        font: egui::TextStyle::Body.resolve(ui.style()),
                                        text_color: if available {
                                            ui.visuals().text_color()
                                        } else {
                                            ui.visuals().weak_text_color()
                                        },
                                    });
                                }
                            }
                        });
                    });
                });
            });
    }

    fn resolve_object_hover(&mut self, ctx: &egui::Context) {
        let position =
            ctx.input(|input| input.focused.then(|| input.pointer.hover_pos()).flatten());
        let allowed = !self.pie_input.owns_frame
            && !self.editor.edit_mode
            && !self.editor.is_interacting()
            && !self.temporary_navigation_active()
            && !self.mouse_navigation.wants_input()
            && !egui::Popup::is_any_open(ctx)
            && !ctx.memory(|memory| memory.top_modal_layer().is_some());
        let Some(position) = position.filter(|position| {
            allowed
                && !self
                    .preferences_rect
                    .is_some_and(|rect| rect.contains(*position))
                && ctx.layer_id_at(*position) == Some(egui::LayerId::background())
        }) else {
            self.editor.hover_object(None);
            return;
        };
        let hovered = if let Some(row) = self
            .layer_rows
            .iter()
            .find(|row| row.rect.contains(position))
        {
            Some(row.id)
        } else if self.viewport_ui_rect.contains(position)
            && (!self.navigation_gizmo_visible()
                || !axis_gizmo::bounds(self.viewport_ui_rect).contains(position))
        {
            match self
                .editor
                .object_at(position, self.viewport, &self.camera, self.z_up)
            {
                Ok(object) => object,
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            }
        } else {
            None
        };
        self.editor.hover_object(hovered);
    }

    fn paint_layer_rows(&self) {
        // Paint after viewport picking so a row reflects current-frame selection
        // and hover even though the side panel was laid out before the viewport.
        for row in &self.layer_rows {
            if let Some(color) = self.layer_row_color(row.id) {
                row.painter
                    .rect_filled(row.rect, theme::radius::SM, color.gamma_multiply(0.18));
                row.painter.rect_stroke(
                    row.rect,
                    theme::radius::SM,
                    egui::Stroke::new(1.0, color),
                    egui::StrokeKind::Inside,
                );
            }
            row.icon.paint(
                &row.painter,
                row.rect.left_center() + egui::vec2(LAYER_ICON_INSET, 0.0),
                row.text_color,
            );
            row.painter.text(
                row.text_position,
                egui::Align2::LEFT_CENTER,
                &row.name,
                row.font.clone(),
                row.text_color,
            );
        }
    }

    fn property_field(
        &mut self,
        ctx: &egui::Context,
        object_id: u64,
        field: PropertyField,
        response: &egui::Response,
        change: impl FnOnce(&mut Editor) -> Result<bool, String>,
    ) {
        if response.changed() {
            let same = self
                .property_session
                .as_ref()
                .is_some_and(|session| session.object_id == object_id && session.field == field);
            if !same {
                self.editor.finish_property_edit(true);
                self.property_session = None;
                let begin = if let Some(sensitivity) = field.translation_sensitivity() {
                    self.editor.begin_property_translation(sensitivity)
                } else {
                    Ok(self.editor.begin_property_edit())
                };
                let started = match begin {
                    Ok(started) => started,
                    Err(error) => {
                        self.error = Some(error);
                        false
                    }
                };
                if started {
                    self.property_session = Some(PropertySession {
                        object_id,
                        field,
                        rect: response.rect,
                    });
                }
            }
            if self.property_session.is_some() {
                let result = change(&mut self.editor);
                self.report(result);
                // Preserve the active widget draft while previews increment the
                // document revision. The following idle pass resyncs from it.
                self.inspector_key = Some((object_id, self.editor.revision));
                ctx.request_repaint();
            }
        }
        if let Some(session) = &mut self.property_session
            && session.object_id == object_id
            && session.field == field
        {
            session.rect = response.rect;
            if response.drag_stopped() || response.lost_focus() {
                let accept = !ctx.input(|input| input.key_pressed(egui::Key::Escape));
                self.editor.finish_property_edit(accept);
                self.property_session = None;
                self.inspector_key = None;
                if matches!(field, PropertyField::VertexDelta(_)) {
                    self.vertex_delta = [0.0; 3];
                }
                ctx.request_repaint();
            }
        }
    }
    fn inspector(&mut self, root_ui: &mut egui::Ui) {
        let context = root_ui.ctx().clone();
        let ctx = &context;
        egui::Panel::right("inspector")
            .resizable(true)
            .default_size(DEFAULT_PANEL_WIDTH)
            .size_range(INSPECTOR_MIN_WIDTH..=480.0)
            .frame(workspace_side_panel_frame(
                root_ui.style(),
                Palette::from_context(ctx, self.accent_color).sidebar,
            ))
            .show(root_ui, |ui| {
                apply_sidebar_colors(ui, Palette::from_context(ctx, self.accent_color));
                self.pie_underlay(ui);
                if self.editor.has_transform_session() {
                    ui.disable();
                }
                ui.spacing_mut().item_spacing.y = 0.0;
                let panel = ui.max_rect();
                controls::record(
                    ctx,
                    Control::Inspector,
                    Control::Inspector.label(),
                    panel,
                    ui.is_enabled(),
                );
                workspace_panel_section(ui, |ui| {
                    ui.label(theme::strong(Control::Inspector.label()));
                });
                workspace_panel_separator(ui);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let style = ui.style_mut();
                    style.spacing.item_spacing.y = 0.0;
                    style.text_styles.insert(
                        egui::TextStyle::Body,
                        egui::FontId::proportional(theme::text::INSPECTOR_BODY_12_5),
                    );
                    style.text_styles.insert(
                        egui::TextStyle::Button,
                        egui::FontId::proportional(theme::text::XS),
                    );
                    controls::scope(ctx, Control::Inspector, || {
                        if self.editor.selected_objects.len() > 1 {
                            workspace_panel_section(ui, |ui| {
                                ui.label(theme::strong(format!(
                                    "{} objects selected",
                                    self.editor.selected_objects.len()
                                )));
                                ui.label(
                                "Drag handles to move or rotate the group, or scale it uniformly.",
                            );
                                ui.weak("Select one object to edit its properties or vertices.");
                            });
                            return;
                        }
                        let Some(id) = self.editor.selected_object else {
                            workspace_panel_section(ui, |ui| {
                                ui.weak("Select an object to see its properties.");
                            });
                            return;
                        };
                        let Some(object) = self.editor.document.objects.iter().find(|o| o.id == id)
                        else {
                            return;
                        };
                        workspace_panel_section(ui, |ui| {
                            ui.label(&object.name);
                        });
                        if self.property_session.is_none()
                            && self.inspector_key != Some((id, self.editor.revision))
                        {
                            self.primitive_draft = match &object.geometry {
                                Geometry::Primitive(p) => Some(p.clone()),
                                _ => None,
                            };
                            self.transform_draft = object.transform.clone();
                            let q = glam::DQuat::from_array(object.transform.rotation);
                            let (x, y, z) = q.to_euler(glam::EulerRot::XYZ);
                            self.rotation_degrees =
                                [x.to_degrees(), y.to_degrees(), z.to_degrees()];
                            self.inspector_key = Some((id, self.editor.revision));
                        }
                        ui.add_enabled_ui(self.editor.can_edit(), |ui| {
                            if self.editor.edit_mode {
                                inspector_section(ui, "Vertices", |ui| {
                                    ui.weak(format!(
                                        "{} selected · document axes",
                                        self.editor.selected_vertices.len()
                                    ));
                                    ui.add_space(theme::space::MD);
                                    let responses =
                                        inspector_axis_row(ui, "Move by", |ui, i, width| {
                                            length_field_with_axis(
                                                ui,
                                                &mut self.vertex_delta[i],
                                                VERTEX_SCRUB_CM_PER_POINT,
                                                self.display_unit,
                                                Some((["X", "Y", "Z"][i], width)),
                                                None,
                                            )
                                        });
                                    for (i, response) in responses.into_iter().enumerate() {
                                        let control = [
                                            Control::VertexDeltaX,
                                            Control::VertexDeltaY,
                                            Control::VertexDeltaZ,
                                        ][i];
                                        controls::record(
                                            ctx,
                                            control,
                                            control.label(),
                                            response.rect,
                                            response.enabled(),
                                        );
                                        let value = self.vertex_delta[i];
                                        let source = property_translation_source(&response);
                                        self.property_field(
                                            ctx,
                                            id,
                                            PropertyField::VertexDelta(i),
                                            &response,
                                            |editor| {
                                                editor
                                                    .preview_property_translation(i, value, source)
                                            },
                                        );
                                        if response.dragged()
                                            && let Some(applied) =
                                                self.editor.property_translation_value(i)
                                        {
                                            // DragValue retains its precise accumulator while
                                            // the visible draft shows the applied snapped delta.
                                            self.vertex_delta[i] = applied;
                                        }
                                    }
                                });
                            } else {
                                inspector_section(ui, "Transform", |ui| {
                                    let responses =
                                        inspector_axis_row(ui, "Position", |ui, i, width| {
                                            length_field_with_axis(
                                                ui,
                                                &mut self.transform_draft.translation[i],
                                                POSITION_SCRUB_CM_PER_POINT,
                                                self.display_unit,
                                                Some((["X", "Y", "Z"][i], width)),
                                                None,
                                            )
                                        });
                                    for (i, response) in responses.into_iter().enumerate() {
                                        let control = [
                                            Control::PositionX,
                                            Control::PositionY,
                                            Control::PositionZ,
                                        ][i];
                                        controls::record(
                                            ctx,
                                            control,
                                            control.label(),
                                            response.rect,
                                            response.enabled(),
                                        );
                                        let value = self.transform_draft.translation[i];
                                        let source = property_translation_source(&response);
                                        self.property_field(
                                            ctx,
                                            id,
                                            PropertyField::Translation(i),
                                            &response,
                                            |editor| {
                                                editor
                                                    .preview_property_translation(i, value, source)
                                            },
                                        );
                                        if response.dragged()
                                            && let Some(applied) =
                                                self.editor.property_translation_value(i)
                                        {
                                            self.transform_draft.translation[i] = applied;
                                        }
                                    }
                                    let responses =
                                        inspector_axis_row(ui, "Scale", |ui, i, width| {
                                            inspector_axis_value(
                                                ui,
                                                ["X", "Y", "Z"][i],
                                                width,
                                                egui::DragValue::new(
                                                    &mut self.transform_draft.scale[i],
                                                )
                                                .speed(0.01),
                                            )
                                        });
                                    for (i, response) in responses.into_iter().enumerate() {
                                        let control =
                                            [Control::ScaleX, Control::ScaleY, Control::ScaleZ][i];
                                        controls::record(
                                            ctx,
                                            control,
                                            control.label(),
                                            response.rect,
                                            response.enabled(),
                                        );
                                        let value = self.transform_draft.scale[i];
                                        self.property_field(
                                            ctx,
                                            id,
                                            PropertyField::Scale(i),
                                            &response,
                                            |editor| {
                                                editor.preview_property_edit(|doc| {
                                                    doc.objects
                                                        .iter_mut()
                                                        .find(|o| o.id == id)
                                                        .ok_or("Missing object")?
                                                        .transform
                                                        .scale[i] = value;
                                                    Ok(())
                                                })
                                            },
                                        );
                                    }
                                    let responses =
                                        inspector_axis_row(ui, "Rotation", |ui, i, width| {
                                            inspector_axis_value(
                                                ui,
                                                ["X", "Y", "Z"][i],
                                                width,
                                                egui::DragValue::new(&mut self.rotation_degrees[i])
                                                    .speed(1.0)
                                                    .suffix("°"),
                                            )
                                        });
                                    for (i, response) in responses.into_iter().enumerate() {
                                        let control = [
                                            Control::RotationX,
                                            Control::RotationY,
                                            Control::RotationZ,
                                        ][i];
                                        controls::record(
                                            ctx,
                                            control,
                                            control.label(),
                                            response.rect,
                                            response.enabled(),
                                        );
                                        let degrees = self.rotation_degrees;
                                        self.property_field(
                                            ctx,
                                            id,
                                            PropertyField::Rotation(i),
                                            &response,
                                            |editor| {
                                                editor.preview_property_edit(|doc| {
                                                    doc.objects
                                                        .iter_mut()
                                                        .find(|o| o.id == id)
                                                        .ok_or("Missing object")?
                                                        .transform
                                                        .rotation = glam::DQuat::from_euler(
                                                        glam::EulerRot::XYZ,
                                                        degrees[0].to_radians(),
                                                        degrees[1].to_radians(),
                                                        degrees[2].to_radians(),
                                                    )
                                                    .to_array();
                                                    Ok(())
                                                })
                                            },
                                        );
                                    }
                                });
                                if let Some(mut p) = self.primitive_draft.clone() {
                                    inspector_section(ui, "Shape", |ui| {
                                        ui.weak(p.kind.label());
                                        ui.add_space(theme::space::MD);
                                        if p.kind == PrimitiveKind::Polyhedron {
                                            let current =
                                                p.polyhedron_type.expect("validated recipe");
                                            let mut chosen = None;
                                            // Names with face counts need the panel's full width.
                                            ui.label("Type");
                                            let width = ui.available_width();
                                            let response = menu::value_dropdown(
                                                ui,
                                                egui::ComboBox::from_id_salt((
                                                    "polyhedron-type",
                                                    id,
                                                ))
                                                .selected_text(current.label())
                                                .width(width)
                                                .truncate(),
                                                Control::PolyhedronTypeMenu,
                                                |ui| {
                                                    for (control, kind) in [
                                                        (
                                                            Control::PolyhedronTetrahedron,
                                                            PolyhedronType::Tetrahedron,
                                                        ),
                                                        (
                                                            Control::PolyhedronCube,
                                                            PolyhedronType::Cube,
                                                        ),
                                                        (
                                                            Control::PolyhedronOctahedron,
                                                            PolyhedronType::Octahedron,
                                                        ),
                                                        (
                                                            Control::PolyhedronDodecahedron,
                                                            PolyhedronType::Dodecahedron,
                                                        ),
                                                        (
                                                            Control::PolyhedronIcosahedron,
                                                            PolyhedronType::Icosahedron,
                                                        ),
                                                    ] {
                                                        let item = menu::selectable_label(
                                                            ui,
                                                            current == kind,
                                                            control.label(),
                                                        );
                                                        controls::record(
                                                            ctx,
                                                            control,
                                                            control.label(),
                                                            item.rect,
                                                            item.enabled(),
                                                        );
                                                        if item.clicked() {
                                                            chosen = Some(kind);
                                                        }
                                                    }
                                                },
                                            )
                                            .response;
                                            controls::record(
                                                ctx,
                                                Control::PolyhedronTypeMenu,
                                                Control::PolyhedronTypeMenu.label(),
                                                response.rect,
                                                response.enabled(),
                                            );
                                            if let Some(kind) = chosen {
                                                let mut updated = p.clone();
                                                updated.polyhedron_type = Some(kind);
                                                let result = self.editor.commit(
                                                    "Change polyhedron type",
                                                    |document| {
                                                        document
                                                            .objects
                                                            .iter_mut()
                                                            .find(|object| object.id == id)
                                                            .ok_or("Missing object")?
                                                            .geometry =
                                                            Geometry::Primitive(updated.clone());
                                                        Ok(())
                                                    },
                                                );
                                                if result.as_ref().is_ok_and(|changed| *changed) {
                                                    p.polyhedron_type = Some(kind);
                                                    self.property_session = None;
                                                    self.inspector_key = None;
                                                }
                                                self.report(result);
                                            }
                                        }
                                        if matches!(
                                            p.kind,
                                            PrimitiveKind::Polyhedron | PrimitiveKind::Circle
                                        ) {
                                            let circle = p.kind == PrimitiveKind::Circle;
                                            let control = if circle {
                                                Control::CircleRadius
                                            } else {
                                                Control::PolyhedronSize
                                            };
                                            let mut value =
                                                if circle { p.size[0] * 0.5 } else { p.size[0] };
                                            let response = inspector_scalar_row(
                                                ui,
                                                if circle { "Radius" } else { "Size" },
                                                |ui, width| {
                                                    length_field_with_axis(
                                                        ui,
                                                        &mut value,
                                                        0.02,
                                                        self.display_unit,
                                                        None,
                                                        Some(width),
                                                    )
                                                },
                                            );
                                            if response.changed() {
                                                if circle {
                                                    p.size[0] = value * 2.0;
                                                    p.size[1] = value * 2.0;
                                                } else {
                                                    p.size = [value; 3];
                                                }
                                            }
                                            controls::record(
                                                ctx,
                                                control,
                                                control.label(),
                                                response.rect,
                                                response.enabled(),
                                            );
                                            let primitive = p.clone();
                                            self.property_field(
                                                ctx,
                                                id,
                                                PropertyField::PrimitiveSize(0),
                                                &response,
                                                |editor| {
                                                    editor.preview_property_edit(|document| {
                                                        document
                                                            .objects
                                                            .iter_mut()
                                                            .find(|object| object.id == id)
                                                            .ok_or("Missing object")?
                                                            .geometry =
                                                            Geometry::Primitive(primitive);
                                                        Ok(())
                                                    })
                                                },
                                            );
                                        } else {
                                            let mut size_field =
                                                |ui: &mut egui::Ui, i: usize, width| {
                                                    length_field_with_axis(
                                                        ui,
                                                        &mut p.size[i],
                                                        0.02,
                                                        self.display_unit,
                                                        (p.kind != PrimitiveKind::Plane)
                                                            .then_some((["X", "Y", "Z"][i], width)),
                                                        (p.kind == PrimitiveKind::Plane)
                                                            .then_some(width),
                                                    )
                                                };
                                            let responses: Vec<_> = if p.kind
                                                == PrimitiveKind::Plane
                                            {
                                                ["Width", "Height"]
                                                    .into_iter()
                                                    .enumerate()
                                                    .map(|(i, label)| {
                                                        inspector_scalar_row(
                                                            ui,
                                                            label,
                                                            |ui, width| size_field(ui, i, width),
                                                        )
                                                    })
                                                    .collect()
                                            } else {
                                                inspector_axis_row(ui, "Size", &mut size_field)
                                                    .into()
                                            };
                                            for (i, response) in responses.into_iter().enumerate() {
                                                let control = [
                                                    Control::PrimitiveX,
                                                    Control::PrimitiveY,
                                                    Control::PrimitiveZ,
                                                ][i];
                                                controls::record(
                                                    ctx,
                                                    control,
                                                    control.label(),
                                                    response.rect,
                                                    response.enabled(),
                                                );
                                                let primitive = p.clone();
                                                self.property_field(
                                                    ctx,
                                                    id,
                                                    PropertyField::PrimitiveSize(i),
                                                    &response,
                                                    |editor| {
                                                        editor.preview_property_edit(|doc| {
                                                            doc.objects
                                                                .iter_mut()
                                                                .find(|o| o.id == id)
                                                                .ok_or("Missing object")?
                                                                .geometry =
                                                                Geometry::Primitive(primitive);
                                                            Ok(())
                                                        })
                                                    },
                                                );
                                            }
                                        }
                                        if matches!(
                                            p.kind,
                                            PrimitiveKind::Circle
                                                | PrimitiveKind::Cylinder
                                                | PrimitiveKind::Cone
                                                | PrimitiveKind::Torus
                                                | PrimitiveKind::Sphere
                                        ) {
                                            let circle = p.kind == PrimitiveKind::Circle;
                                            let control = if circle {
                                                Control::CircleVertices
                                            } else {
                                                Control::Segments
                                            };
                                            let response = inspector_scalar_row(
                                                ui,
                                                control.label(),
                                                |ui, width| {
                                                    inspector_numeric_value(
                                                        ui,
                                                        None,
                                                        width,
                                                        egui::DragValue::new(&mut p.segments)
                                                            .range(
                                                                3..=if circle { 256 } else { 128 },
                                                            ),
                                                    )
                                                },
                                            );
                                            controls::record(
                                                ctx,
                                                control,
                                                control.label(),
                                                response.rect,
                                                response.enabled(),
                                            );
                                            let primitive = p.clone();
                                            self.property_field(
                                                ctx,
                                                id,
                                                PropertyField::Segments,
                                                &response,
                                                |editor| {
                                                    editor.preview_property_edit(|doc| {
                                                        doc.objects
                                                            .iter_mut()
                                                            .find(|o| o.id == id)
                                                            .ok_or("Missing object")?
                                                            .geometry =
                                                            Geometry::Primitive(primitive);
                                                        Ok(())
                                                    })
                                                },
                                            );
                                        }
                                        if p.kind == PrimitiveKind::Circle {
                                            let mut fill = p.fill;
                                            if controls::checkbox(
                                                ui,
                                                Control::CircleFill,
                                                &mut fill,
                                            )
                                            .changed()
                                            {
                                                let mut updated = p.clone();
                                                updated.fill = fill;
                                                let result = self.editor.commit(
                                                    "Change circle fill",
                                                    |document| {
                                                        document
                                                            .objects
                                                            .iter_mut()
                                                            .find(|object| object.id == id)
                                                            .ok_or("Missing object")?
                                                            .geometry =
                                                            Geometry::Primitive(updated.clone());
                                                        Ok(())
                                                    },
                                                );
                                                if result.as_ref().is_ok_and(|changed| *changed) {
                                                    p = updated;
                                                    self.property_session = None;
                                                    self.inspector_key = None;
                                                }
                                                self.report(result);
                                            }
                                        }
                                        if matches!(
                                            p.kind,
                                            PrimitiveKind::Torus | PrimitiveKind::Sphere
                                        ) {
                                            let ring_control = if p.kind == PrimitiveKind::Sphere {
                                                Control::SphereRings
                                            } else {
                                                Control::MinorSegments
                                            };
                                            let response = inspector_scalar_row(
                                                ui,
                                                ring_control.label(),
                                                |ui, width| {
                                                    inspector_numeric_value(
                                                        ui,
                                                        None,
                                                        width,
                                                        egui::DragValue::new(&mut p.minor_segments)
                                                            .range(3..=64),
                                                    )
                                                },
                                            );
                                            controls::record(
                                                ctx,
                                                ring_control,
                                                ring_control.label(),
                                                response.rect,
                                                response.enabled(),
                                            );
                                            let primitive = p.clone();
                                            self.property_field(
                                                ctx,
                                                id,
                                                PropertyField::MinorSegments,
                                                &response,
                                                |editor| {
                                                    editor.preview_property_edit(|doc| {
                                                        doc.objects
                                                            .iter_mut()
                                                            .find(|o| o.id == id)
                                                            .ok_or("Missing object")?
                                                            .geometry =
                                                            Geometry::Primitive(primitive);
                                                        Ok(())
                                                    })
                                                },
                                            );
                                        }
                                        if p.kind == PrimitiveKind::Torus {
                                            let response = inspector_scalar_row(
                                                ui,
                                                "Tube ratio",
                                                |ui, width| {
                                                    inspector_numeric_value(
                                                        ui,
                                                        None,
                                                        width,
                                                        egui::DragValue::new(&mut p.minor_radius)
                                                            .speed(0.01)
                                                            .range(0.01..=0.49),
                                                    )
                                                },
                                            );
                                            controls::record(
                                                ctx,
                                                Control::TubeRatio,
                                                Control::TubeRatio.label(),
                                                response.rect,
                                                response.enabled(),
                                            );
                                            let primitive = p.clone();
                                            self.property_field(
                                                ctx,
                                                id,
                                                PropertyField::TubeRatio,
                                                &response,
                                                |editor| {
                                                    editor.preview_property_edit(|doc| {
                                                        doc.objects
                                                            .iter_mut()
                                                            .find(|o| o.id == id)
                                                            .ok_or("Missing object")?
                                                            .geometry =
                                                            Geometry::Primitive(primitive);
                                                        Ok(())
                                                    })
                                                },
                                            );
                                        }
                                    });
                                    self.primitive_draft = Some(p.clone());
                                }
                            }
                        });
                        self.asset_inspector(ui);
                    });
                });
            });
    }
    fn preferences_focus_id() -> egui::Id {
        egui::Id::new("n3.preferences.keyboard")
    }

    fn preferences_window(&mut self, ctx: &egui::Context) {
        if !self.show_preferences {
            self.preferences_focus_pending = false;
            return;
        }
        // This initial window owner has no native widget action for Cancel.
        // Fields and popups keep their own Escape handling; only the untouched
        // title owner forwards the canonical action. egui may already have
        // surrendered that focus at begin_pass when Escape was pressed.
        let title_owns_keys = ctx.memory(|memory| {
            memory.has_focus(Self::preferences_focus_id())
                || (memory.focused().is_none()
                    && memory.had_focus_last_frame(Self::preferences_focus_id()))
        });
        let cancel = crate::input::bindings::required("cancel");
        if title_owns_keys
            && !egui::Popup::is_any_open(ctx)
            && !ctx.memory(|memory| memory.top_modal_layer().is_some())
            && ctx.input(|input| {
                input.focused
                    && input.events.iter().any(|event| {
                        matches!(
                            event,
                            egui::Event::Key {
                                key, modifiers, pressed: true, repeat: false, ..
                            } if cancel.matches_key(*key, *modifiers)
                        )
                    })
            })
        {
            self.dispatch(Command::Escape, ctx, false);
            return;
        }
        let mut open = self.show_preferences;
        let mut close = false;
        let window = egui::Window::new(Control::PreferencesWindow.label())
            .enabled(!self.pie_input.owns_frame)
            .id(egui::Id::new(Control::PreferencesWindow.id()))
            .open(&mut open)
            .title_bar(false)
            .default_pos(self.viewport_ui_rect.left_top() + egui::vec2(20.0, 60.0))
            .default_width(360.0)
            .default_height(460.0)
            .max_height((ctx.content_rect().height() - 110.0).max(120.0))
            .frame(
                egui::Frame::window(ctx.global_style().as_ref())
                    .fill(Palette::from_context(ctx, self.accent_color).card)
                    .inner_margin(egui::Margin {
                        top: ctx.global_style().spacing.window_margin.top,
                        ..egui::Margin::ZERO
                    }),
            )
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.visuals_mut().widgets.noninteractive.fg_stroke.color =
                    Palette::from_context(ctx, self.accent_color).card_foreground;
                let (header, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), theme::size::XL_4),
                    egui::Sense::empty(),
                );
                let title_rect = egui::Rect::from_min_size(
                    header.min + egui::vec2(theme::space::XL, 0.0),
                    egui::vec2(theme::size::STEP_32, header.height()),
                );
                let close_rect = egui::Rect::from_min_size(
                    egui::pos2(
                        header.right() - theme::space::XL - theme::size::XL_4,
                        header.top(),
                    ),
                    egui::Vec2::splat(theme::size::XL_4),
                );
                controls::scope(ctx, Control::PreferencesWindow, || {
                    let title = ui.interact(
                        title_rect,
                        Self::preferences_focus_id(),
                        // Native controls can take focus from this initial
                        // window owner through ordinary click/Tab navigation.
                        egui::Sense::focusable_noninteractive(),
                    );
                    // Window's invisible sizing pass registers disabled
                    // widgets and clears their focus. Complete the opening
                    // transition only once the real title is available.
                    if self.preferences_focus_pending && title.enabled() && ui.is_visible() {
                        title.request_focus();
                        self.preferences_focus_pending = false;
                    }
                    ui.painter().text(
                        title_rect.left_center(),
                        egui::Align2::LEFT_CENTER,
                        Control::PreferencesTitle.label(),
                        egui::TextStyle::Heading.resolve(ui.style()),
                        ui.visuals().text_color(),
                    );
                    title.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Label,
                            title.enabled(),
                            Control::PreferencesTitle.label(),
                        )
                    });
                    controls::record(
                        ctx,
                        Control::PreferencesTitle,
                        Control::PreferencesTitle.label(),
                        title.rect,
                        title.enabled(),
                    );
                    let response = ui.interact(
                        close_rect,
                        ui.id().with(Control::PreferencesClose.id()),
                        egui::Sense::click(),
                    );
                    let visuals = ui.style().interact(&response);
                    if response.hovered() || response.has_focus() {
                        ui.painter().rect_filled(
                            close_rect,
                            theme::radius::MD,
                            visuals.weak_bg_fill,
                        );
                    }
                    lucide::Icon::X.paint_centered(
                        ui.painter(),
                        close_rect.center(),
                        theme::text::BASE,
                        visuals.fg_stroke.color,
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            response.enabled(),
                            Control::PreferencesClose.label(),
                        )
                    });
                    controls::record(
                        ctx,
                        Control::PreferencesClose,
                        Control::PreferencesClose.label(),
                        response.rect,
                        response.enabled(),
                    );
                    close = response.clicked();
                });
                ui.separator();
                // Keep the titlebar in place while a real egui scroll area
                // handles the growing set of preferences below it.
                egui::ScrollArea::vertical()
                    .id_salt("preferences-content")
                    .max_height((ctx.content_rect().height() - 180.0).clamp(160.0, 420.0))
                    .show(ui, |ui| {
                    egui::Frame::NONE
                        .inner_margin(ui.style().spacing.window_margin)
                        .show(ui, |ui| {
                    controls::scope(ctx, Control::PreferencesWindow, || {
                    ui.label(theme::strong("Appearance"));
                    let chosen = match self.theme_mode {
                        ThemeMode::System => Control::ThemeSystem,
                        ThemeMode::Light => Control::ThemeLight,
                        ThemeMode::Dark => Control::ThemeDark,
                    };
                    let theme_menu = ui.horizontal(|ui| {
                        ui.label(Control::AppearanceThemeMenu.label());
                        menu::value_dropdown(
                            ui,
                            egui::ComboBox::from_id_salt("appearance-theme")
                                .selected_text(chosen.label())
                                .width(120.0),
                            Control::AppearanceThemeMenu,
                            |ui| {
                                for (control, mode) in [
                                    (Control::ThemeSystem, ThemeMode::System),
                                    (Control::ThemeLight, ThemeMode::Light),
                                    (Control::ThemeDark, ThemeMode::Dark),
                                ] {
                                    let response = menu::selectable_value(
                                        ui, &mut self.theme_mode, mode, control.label());
                                    controls::record(ctx, control, control.label(),
                                        response.rect, response.enabled());
                                }
                            },
                        )
                    }).inner;
                    controls::record(ctx, Control::AppearanceThemeMenu,
                        Control::AppearanceThemeMenu.label(), theme_menu.response.rect,
                        theme_menu.response.enabled());
                    ui.horizontal(|ui| {
                        ui.label(Control::AccentColor.label());
                        let mut rgb = self.accent_color.rgb();
                        let color = ui.color_edit_button_srgb(&mut rgb);
                        if color.changed() {
                            self.accent_color = AccentColor::new(rgb[0], rgb[1], rgb[2]);
                        }
                        controls::record(ctx, Control::AccentColor,
                            Control::AccentColor.label(), color.rect, color.enabled());
                        let reset = ui.add_enabled(
                            self.accent_color != AccentColor::DEFAULT,
                            egui::Button::new(Control::ResetAccent.label()),
                        );
                        if reset.clicked() {
                            self.accent_color = AccentColor::DEFAULT;
                        }
                        controls::record(ctx, Control::ResetAccent,
                            Control::ResetAccent.label(), reset.rect, reset.enabled());
                    });
                    ui.separator();
                    ui.label(theme::strong("Viewport"));
                    controls::checkbox(ui, Control::Grid, &mut self.show_grid);
                    ui.add_enabled_ui(!self.editor.has_transform_session(), |ui| {
                        controls::checkbox(ui, Control::ZUp, &mut self.z_up)
                            .on_hover_text("Display Z up for this view; source coordinates are unchanged.")
                    });
                    ui.separator();
                    ui.label(theme::strong("Units"));
                    let menu = ui.horizontal(|ui| {
                        ui.label(Control::LengthUnitMenu.label());
                        menu::value_dropdown(
                            ui,
                            egui::ComboBox::from_id_salt("display-length-unit")
                                .selected_text(self.display_unit.symbol())
                                .width(120.0),
                            Control::LengthUnitMenu,
                            |ui| {
                                for unit in LengthUnit::ALL {
                                    let control = length_unit_control(unit);
                                    let response = menu::selectable_value(
                                        ui,
                                        &mut self.display_unit,
                                        unit,
                                        control.label(),
                                    );
                                    controls::record(ctx, control, control.label(),
                                        response.rect, response.enabled());
                                }
                            },
                        )
                    }).inner;
                    controls::record(ctx, Control::LengthUnitMenu,
                        Control::LengthUnitMenu.label(), menu.response.rect,
                        menu.response.enabled());
                    menu.response.on_hover_text(
                        "Changes ruler and inspector units without resizing geometry. Values are stored in centimeters; arrows move 1 cm.");
                    ui.separator();
                    ui.label(theme::strong("Snapping"));
                    controls::checkbox(ui, Control::SnapGrid, &mut self.editor.snapping.enabled)
                        .on_hover_text("Snap movement to clean lengths. Auto adapts to zoom before each drag; typed values remain exact.");
                    ui.add_enabled_ui(self.editor.snapping.enabled, |ui| {
                        ui.label(Control::SnapSpacingMenu.label());
                        let selected = if matches!(self.editor.snap_policy, StepPolicy::Fixed) {
                            Control::SnapFixed
                        } else {
                            Control::SnapAdaptive
                        };
                        let menu = menu::value_dropdown(
                            ui,
                            egui::ComboBox::from_id_salt("movement-snap-spacing")
                                .selected_text(selected.label())
                                .width(120.0),
                            Control::SnapSpacingMenu,
                            |ui| {
                                for control in [Control::SnapAdaptive, Control::SnapFixed] {
                                    let response = menu::selectable_label(ui, selected == control, control.label());
                                    controls::record(ctx, control, control.label(), response.rect, response.enabled());
                                    if response.clicked() && selected != control {
                                        self.editor.snap_policy = if control == Control::SnapFixed {
                                            StepPolicy::Fixed
                                        } else {
                                            StepPolicy::default()
                                        };
                                    }
                                }
                            },
                        );
                        controls::record(ctx, Control::SnapSpacingMenu, Control::SnapSpacingMenu.label(), menu.response.rect, menu.response.enabled());
                        menu.response.on_hover_text("Auto chooses a clean step at the current view scale and keeps it throughout the drag. Numeric-field drags use their own sensitivity.");
                        ui.add_enabled_ui(matches!(self.editor.snap_policy, StepPolicy::Fixed), |ui| {
                            ui.horizontal(|ui| {
                                ui.label(Control::SnapStep.label());
                                let mut step = self.editor.snapping.step_cm;
                                let response = length_field(ui, &mut step, 0.01, LengthUnit::Centimeters);
                                if response.changed() && step.is_finite() && step > 0.0 {
                                    self.editor.snapping.step_cm = step;
                                }
                                controls::record(ctx, Control::SnapStep, Control::SnapStep.label(), response.rect, response.enabled());
                            });
                        });
                    });
                    ui.separator();
                    ui.label(theme::strong("Gizmo"));
                    if controls::checkbox(ui, Control::AnimateViews, &mut self.animate_views)
                        .changed()
                        && !self.animate_views
                    {
                        self.camera.finish_transition();
                    }
                    let duration = ui
                        .add_enabled(
                            self.animate_views,
                            egui::Slider::new(&mut self.view_duration_ms, 0..=MAX_VIEW_DURATION_MS)
                                .suffix(" ms")
                                .text(Control::Duration.label()),
                        )
                        .on_hover_text(
                            "Applies to axis views, the return to 3D, and Local View. Zero is instant.",
                        );
                    controls::record(
                        ctx,
                        Control::Duration,
                        Control::Duration.label(),
                        duration.rect,
                        duration.enabled(),
                    );
                    let return_label = match self.return_3d {
                        PlanarExit::PerspectiveOnly => Control::Return3DPerspective,
                        PlanarExit::OrientationOnly => Control::Return3DOrientation,
                        PlanarExit::OrientationAndPerspective => Control::Return3DBoth,
                    };
                    ui.label(Control::Return3DMenu.label());
                    let return_menu = menu::value_dropdown(
                        ui,
                        egui::ComboBox::from_id_salt("return-to-3d")
                            .selected_text(return_label.label())
                            .width(240.0),
                        Control::Return3DMenu,
                        |ui| {
                            for (control, policy) in [
                                (Control::Return3DPerspective, PlanarExit::PerspectiveOnly),
                                (Control::Return3DOrientation, PlanarExit::OrientationOnly),
                                (Control::Return3DBoth, PlanarExit::OrientationAndPerspective),
                            ] {
                                let response = menu::selectable_value(
                                    ui, &mut self.return_3d, policy, control.label());
                                controls::record(ctx, control, control.label(),
                                    response.rect, response.enabled());
                            }
                        },
                    );
                    controls::record(ctx, Control::Return3DMenu,
                        Control::Return3DMenu.label(), return_menu.response.rect,
                        return_menu.response.enabled());
                    return_menu.response.on_hover_text(
                        "Returning with the 3D tab keeps current pan and zoom. Orbit drags continue from the visible view.");
                    ui.separator();
                    ui.label(theme::strong("Navigation"));
                    controls::checkbox(ui, Control::PreciseScroll, &mut self.precise_scroll_zoom)
                        .on_hover_text(format!("In Free navigation, precise scrolling zooms. Planar navigation and Shift-scroll pan. {} + left drag orbits.", shortcut_label("navigation.orbit")));
                    ui.separator();
                    if let Some(error) = &self.settings_error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    ui.horizontal(|ui| {
                        if self.host_capabilities.open_settings_file {
                            self.request_open_settings |= controls::button(ui, Control::SettingsJson)
                                .on_hover_text(self.settings_location.as_deref()
                                    .unwrap_or("Edit global preferences as JSON. Changes apply to all N3 documents."))
                                .clicked();
                        }
                        self.request_reload_settings |= controls::button(ui, Control::ReloadSettings)
                            .on_hover_text("Reload global preferences, discarding unsaved preference changes. Your document is unchanged.")
                            .clicked();
                    });
                    });
                        });
                });
            });
        if let Some(window) = window {
            self.preferences_rect = Some(window.response.rect);
            controls::record(
                ctx,
                Control::PreferencesWindow,
                Control::PreferencesWindow.label(),
                window.response.rect,
                window.response.enabled(),
            );
        }
        self.show_preferences = open && !close;
    }
}

pub(crate) fn filename(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

fn length_unit_control(unit: LengthUnit) -> Control {
    match unit {
        LengthUnit::Millimeters => Control::LengthMillimeters,
        LengthUnit::Centimeters => Control::LengthCentimeters,
        LengthUnit::Meters => Control::LengthMeters,
        LengthUnit::Inches => Control::LengthInches,
        LengthUnit::Feet => Control::LengthFeet,
    }
}

/// Store drafts in canonical centimeters. Merely painting in another unit must
/// never round-trip a value through conversion, dirty geometry, or create undo.
fn length_field(
    ui: &mut egui::Ui,
    centimeters: &mut f64,
    speed_cm: f64,
    unit: LengthUnit,
) -> egui::Response {
    length_field_with_axis(ui, centimeters, speed_cm, unit, None, None)
}

fn length_field_with_axis(
    ui: &mut egui::Ui,
    centimeters: &mut f64,
    speed_cm: f64,
    unit: LengthUnit,
    axis: Option<(&str, f32)>,
    width: Option<f32>,
) -> egui::Response {
    let mut displayed = unit.from_centimeters(*centimeters);
    let value = egui::DragValue::new(&mut displayed)
        .speed(unit.from_centimeters(speed_cm))
        // egui reparses the formatted text on blur, even without typing.
        // A round-trippable string preserves small and precise distances.
        .custom_formatter(|value, _| units::format_length_number(value))
        .custom_parser(move |text| {
            units::parse_length(text, unit).map(|cm| unit.from_centimeters(cm))
        });
    // Length inputs show bare numbers to preserve width. Display units are a
    // global preference; explicit unit suffixes remain valid when typing.
    let response = if let Some((letter, width)) = axis {
        inspector_axis_value(ui, letter, width, value)
    } else if let Some(width) = width {
        ui.add_sized([width, theme::ROW_HEIGHT], value)
    } else {
        ui.add(value)
    };
    if response.changed() {
        let value = unit.to_centimeters(displayed);
        if value.is_finite() {
            *centimeters = value;
        }
    }
    let help = format!(
        "{} {}. Enter a length, optionally followed by mm, cm, m, in, or ft.",
        units::format_length_number(displayed),
        unit.symbol()
    );
    response.on_hover_text(help)
}

#[cfg(test)]
#[path = "length_field_tests.rs"]
mod length_field_tests;

#[cfg(test)]
#[path = "workspace_ui_tests.rs"]
mod tests;
