use super::Stage;
use crate::{
    camera::Camera,
    settings::{Settings, ThemeMode},
    workspace_ui::WorkspaceUi,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::cell::RefCell;
#[path = "clock.rs"]
mod clock;
use clock::Instant;

const STAGES: [&str; 14] = [
    "surface_acquire",
    "ui",
    "commands_and_refresh",
    "host_prepare",
    "cache_sync",
    "tessellation_and_textures",
    "feedback",
    "scene_encode",
    "ui_encode",
    "submit_api",
    "present_api",
    "host_tail",
    "editor_prepare",
    "scene_composite",
];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Options {
    pub warmup_frames: usize,
    pub sample_frames: usize,
    pub workload: Workload,
    pub mode: ExecutionMode,
    pub selected: bool,
    pub instrument_stages: bool,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ExecutionMode {
    #[default]
    Editor,
    Viewport,
    Renderer,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Workload {
    Stationary,
    Orbit,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            warmup_frames: 60,
            sample_frames: 180,
            workload: Workload::Orbit,
            mode: ExecutionMode::Editor,
            selected: false,
            instrument_stages: true,
        }
    }
}
impl Options {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !(1..=10_000).contains(&self.sample_frames) || self.warmup_frames > 10_000 {
            return Err(
                "Measurement requires 1..=10000 samples and at most 10000 warm-up frames".into(),
            );
        }
        if self.mode == ExecutionMode::Renderer && self.selected {
            return Err("Renderer mode excludes editor selection feedback; omit selected".into());
        }
        Ok(())
    }
}

#[derive(Default, Clone, Serialize)]
struct Counters {
    mesh_uploads: usize,
    // Only base mesh position/normal/color and edge buffers; not total GPU traffic.
    base_mesh_upload_bytes: usize,
    viewport_resizes: usize,
    ui_jobs: usize,
    egui_passes: usize,
    egui_tessellations: usize,
    egui_texture_updates: usize,
    egui_composites: usize,
    editor_prepares: usize,
    editor_feedback_updates: usize,
    scene_renders: usize,
    scene_composites: usize,
}
#[derive(Default, Clone, Serialize)]
struct ProjectionStats {
    rebuilds: usize,
    vertices_tested: usize,
    cpu_ms: Option<f64>,
    geometry_rebuilds: usize,
}
#[derive(Default)]
struct Nested {
    active: bool,
    instrument: bool,
    stats: ProjectionStats,
    editor_prepares: usize,
}
thread_local! { static NESTED: RefCell<Nested> = RefCell::new(Nested::default()); }

#[derive(Default)]
pub(crate) struct FrameProbe {
    data: Option<FrameData>,
}
struct FrameData {
    start: Instant,
    last: Instant,
    stages: [Option<f64>; STAGES.len()],
    instrument: bool,
    counters: Counters,
    pixels_per_point: f32,
    scene_pixels: Option<[u32; 2]>,
    surface_pixels: Option<[u32; 2]>,
}
impl FrameProbe {
    fn start(instrument: bool) -> Self {
        Self::start_at(instrument, Instant::now())
    }
    fn start_at(instrument: bool, now: Instant) -> Self {
        NESTED.with(|value| {
            *value.borrow_mut() = Nested {
                active: true,
                instrument,
                stats: ProjectionStats::default(),
                editor_prepares: 0,
            }
        });
        Self {
            data: Some(FrameData {
                start: now,
                last: now,
                stages: [None; STAGES.len()],
                instrument,
                counters: Counters::default(),
                pixels_per_point: 1.0,
                scene_pixels: None,
                surface_pixels: None,
            }),
        }
    }
    pub(crate) fn end(&mut self, stage: Stage) {
        if let Some(data) = &mut self.data
            && data.instrument
        {
            data.end_at(stage, Instant::now());
        }
    }
    pub(crate) fn mesh_upload(&mut self, bytes: usize) {
        if let Some(data) = &mut self.data {
            data.counters.mesh_uploads += 1;
            data.counters.base_mesh_upload_bytes += bytes;
        }
    }
    pub(crate) fn resized(&mut self) {
        if let Some(data) = &mut self.data {
            data.counters.viewport_resizes += 1;
        }
    }
    pub(crate) fn ui_jobs(&mut self, count: usize) {
        if let Some(data) = &mut self.data {
            data.counters.ui_jobs = count;
        }
    }
    pub(crate) fn pixel_scale(&mut self, value: f32) {
        if let Some(data) = &mut self.data {
            data.pixels_per_point = value;
        }
    }
    pub(crate) fn egui_pass(&mut self) {
        if let Some(data) = &mut self.data {
            data.counters.egui_passes += 1;
        }
    }
    pub(crate) fn egui_tessellate(&mut self) {
        if let Some(data) = &mut self.data {
            data.counters.egui_tessellations += 1;
        }
    }
    pub(crate) fn egui_texture_update(&mut self) {
        if let Some(data) = &mut self.data {
            data.counters.egui_texture_updates += 1;
        }
    }
    pub(crate) fn egui_composite(&mut self) {
        if let Some(data) = &mut self.data {
            data.counters.egui_composites += 1;
        }
    }
    pub(crate) fn scene_render(&mut self) {
        if let Some(data) = &mut self.data {
            data.counters.scene_renders += 1;
        }
    }
    pub(crate) fn scene_composite(&mut self) {
        if let Some(data) = &mut self.data {
            data.counters.scene_composites += 1;
        }
    }
    pub(crate) fn scene_size(&mut self, value: [u32; 2]) {
        if let Some(data) = &mut self.data {
            data.scene_pixels = Some(value);
        }
    }
    pub(crate) fn surface_size(&mut self, value: [u32; 2]) {
        if let Some(data) = &mut self.data {
            data.surface_pixels = Some(value);
        }
    }
    pub(crate) fn editor_feedback(&mut self) {
        if let Some(data) = &mut self.data {
            data.counters.editor_feedback_updates += 1;
        }
    }
}
impl FrameData {
    fn end_at(&mut self, stage: Stage, now: Instant) {
        *self.stages[stage as usize].get_or_insert(0.0) +=
            now.duration_since(self.last).as_secs_f64() * 1000.0;
        self.last = now;
    }
}
impl Drop for FrameProbe {
    fn drop(&mut self) {
        if self.data.is_some() {
            NESTED.with(|value| value.borrow_mut().active = false);
        }
    }
}
pub(crate) struct ProjectionProbe {
    active: bool,
    start: Option<Instant>,
}
pub(crate) fn projection_started() -> ProjectionProbe {
    NESTED.with(|value| {
        let value = value.borrow();
        ProjectionProbe {
            active: value.active,
            start: (value.active && value.instrument).then(Instant::now),
        }
    })
}
impl ProjectionProbe {
    pub(crate) fn finish(self, vertices: usize) {
        if self.active {
            NESTED.with(|value| {
                let mut value = value.borrow_mut();
                value.stats.rebuilds += 1;
                value.stats.vertices_tested += vertices;
                if let Some(start) = self.start {
                    *value.stats.cpu_ms.get_or_insert(0.0) +=
                        start.elapsed().as_secs_f64() * 1000.0;
                }
            });
        }
    }
}
pub(crate) fn editor_prepared() {
    NESTED.with(|value| {
        let mut value = value.borrow_mut();
        if value.active {
            value.editor_prepares += 1;
        }
    });
}
pub(crate) fn geometry_rebuilt() {
    NESTED.with(|value| {
        let mut value = value.borrow_mut();
        if value.active {
            value.stats.geometry_rebuilds += 1;
        }
    });
}

#[derive(Serialize)]
struct Sample {
    frame: usize,
    frame_start_interval_ms: Option<f64>,
    cpu_frame_ms: f64,
    cpu_stage_ms: Option<std::collections::BTreeMap<&'static str, Option<f64>>>,
    projection: ProjectionStats,
    counters: Counters,
    focused: bool,
    fps_meter_enabled: bool,
    viewport_pixels: Option<[u32; 2]>,
    surface_pixels: Option<[u32; 2]>,
    viewport_points: [f32; 4],
    camera_view_projection: [f32; 16],
    pixels_per_point: f32,
    mesh_revision: u64,
    error: Option<String>,
}
pub(crate) struct Session {
    options: Options,
    metadata: Value,
    frame: usize,
    previous_start: Option<Instant>,
    samples: Vec<Sample>,
    finished: bool,
    initial_camera: Camera,
    viewport: egui::Rect,
    viewport_ui_rect: egui::Rect,
    initial_fps_meter_enabled: bool,
    fps_meter_constant: bool,
}
impl Session {
    pub(crate) fn new(
        options: Options,
        metadata: Value,
        state: &mut WorkspaceUi,
    ) -> Result<Self, String> {
        options.validate()?;
        if state.editor.document.objects.is_empty() || state.editor.is_interacting() {
            return Err("Measurement needs a loaded document without an active edit".into());
        }
        if !state.show_ui || state.show_preferences || state.tool_dock.active.is_some() {
            return Err(
                "Measurement needs the full editor layout with Preferences and the Tool Dock closed"
                    .into(),
            );
        }
        if !state.viewport.is_finite() || !state.viewport.is_positive() || state.is_local_view() {
            return Err("Measurement needs a laid-out viewport showing the whole document".into());
        }
        if state
            .asset_views
            .values()
            .any(|view| view.playback.playing || view.playback.clip.is_some())
        {
            return Err("Measurement requires imported assets in their static rest pose".into());
        }
        state.apply_user_settings(&Settings::default());
        state.theme_mode = ThemeMode::Light;
        state.shading = crate::render::shading::ShadingMode::Solid;
        state.show_edges = true;
        state.show_grid = true;
        state.show_ui = true;
        state.reset_navigation_input();
        state.toasts.clear();
        state.hovered_file = false;
        state.editor.leave_edit();
        state.editor.set_tool(crate::editor::Tool::default());
        state.editor.set_xray(false);
        state.editor.deselect();
        if options.selected {
            state
                .editor
                .select_object(state.editor.document.objects[0].id)?;
        }
        state.editor.hovered_object = None;
        state.camera = Camera::default();
        state.camera.frame(state.aspect());
        Ok(Self {
            samples: Vec::with_capacity(options.sample_frames),
            options,
            metadata,
            frame: 0,
            previous_start: None,
            finished: false,
            initial_camera: state.camera.clone(),
            viewport: state.viewport,
            viewport_ui_rect: state.viewport_ui_rect,
            initial_fps_meter_enabled: state.fps_meter.enabled(),
            fps_meter_constant: true,
        })
    }
    pub(crate) fn active(&self) -> bool {
        !self.finished
    }
    pub(crate) fn mode(&self) -> ExecutionMode {
        self.options.mode
    }
    pub(crate) fn without_ui(&self) -> bool {
        self.active() && self.mode() != ExecutionMode::Editor
    }
    pub(crate) fn prepare_without_ui(&self, state: &mut WorkspaceUi, probe: &mut FrameProbe) {
        if self.active() && self.mode() == ExecutionMode::Viewport {
            if let Err(error) =
                state
                    .editor
                    .prepare_viewport(state.viewport, &state.camera, state.z_up)
            {
                state.error = Some(error);
            }
            probe.end(Stage::EditorPrepare);
        }
    }
    pub(crate) fn begin_frame(&mut self, state: &mut WorkspaceUi) -> FrameProbe {
        if self.finished {
            return FrameProbe::default();
        }
        if self.without_ui() {
            // Keep the same scene area as the baseline editor layout. Skipping
            // UI execution must not increase render resolution or change framing.
            state.viewport = self.viewport;
            state.viewport_ui_rect = self.viewport_ui_rect;
        }
        // Fixed angular path per rendered frame, through the shared navigation
        // operation. This measures rendering throughput, not DOM/OS input latency.
        state.editor.hovered_object = None;
        state.camera = self.initial_camera.clone();
        if matches!(self.options.workload, Workload::Orbit) {
            // Derive from completed frames, so a surface retry cannot advance
            // the camera without an accompanying rendered sample.
            state.orbit((self.frame + 1) as f32, 0.0);
        }
        FrameProbe::start(self.options.instrument_stages)
    }
    pub(crate) fn finish_frame(
        &mut self,
        probe: FrameProbe,
        state: &WorkspaceUi,
        focused: bool,
    ) -> Option<Value> {
        self.finish_frame_at(probe, state, focused, Instant::now())
    }
    fn finish_frame_at(
        &mut self,
        mut probe: FrameProbe,
        state: &WorkspaceUi,
        focused: bool,
        finished_at: Instant,
    ) -> Option<Value> {
        let mut data = probe.data.take()?;
        let elapsed = finished_at.duration_since(data.start).as_secs_f64() * 1000.0;
        let interval = self
            .previous_start
            .replace(data.start)
            .map(|previous| data.start.duration_since(previous).as_secs_f64() * 1000.0);
        let projection = NESTED.with(|value| {
            let mut value = value.borrow_mut();
            value.active = false;
            data.counters.editor_prepares = value.editor_prepares;
            value.stats.clone()
        });
        let fps_meter_enabled = state.fps_meter.enabled();
        // Include warm-up observations: changing the overlay workload and then
        // restoring it must not make a contaminated run look constant.
        self.fps_meter_constant &= fps_meter_enabled == self.initial_fps_meter_enabled;
        if self.frame >= self.options.warmup_frames {
            self.samples.push(Sample {
                frame: self.samples.len(),
                frame_start_interval_ms: interval,
                cpu_frame_ms: elapsed,
                cpu_stage_ms: data
                    .instrument
                    .then(|| STAGES.into_iter().zip(data.stages).collect()),
                projection,
                counters: data.counters,
                focused,
                fps_meter_enabled,
                viewport_pixels: data.scene_pixels,
                surface_pixels: data.surface_pixels,
                viewport_points: rect_points(state.viewport),
                camera_view_projection: state
                    .camera
                    .view_projection(state.aspect())
                    .to_cols_array(),
                pixels_per_point: data.pixels_per_point,
                mesh_revision: state.mesh_revision,
                error: state.error.clone(),
            });
        }
        self.frame += 1;
        if self.samples.len() != self.options.sample_frames {
            return None;
        }
        self.finished = true;
        let mut stages = serde_json::Map::new();
        if self.options.instrument_stages {
            for name in STAGES {
                stages.insert(
                    name.into(),
                    distribution(self.samples.iter().filter_map(|sample| {
                        sample.cpu_stage_ms.as_ref().and_then(|values| values[name])
                    })),
                );
            }
            stages.insert(
                "editor_projection".into(),
                distribution(
                    self.samples
                        .iter()
                        .filter_map(|sample| sample.projection.cpu_ms),
                ),
            );
        }
        let mesh = state.mesh.as_ref();
        Some(json!({
            "schema": "n3.viewport-measure.v2", "options": self.options, "metadata": self.metadata,
            "scope": "Production host rendering with a deterministic semantic camera workload. CPU timings are elapsed API costs, not GPU completion or display latency.",
            "limitations": ["stationary is forced rendering, not idle power/wakeup observation", "GPU durations and actual presentation times are unavailable", "input device delivery and latency are excluded", "stage probes add overhead; compare instrument_stages=false runs", "no missed-display-frame inference without refresh/presentation observations"],
            "render": {"fps_meter_enabled": self.initial_fps_meter_enabled, "shading": "solid", "edges": true, "grid": true, "theme": "light", "selected_objects": state.editor.selected_objects.len(), "hovered_object": state.editor.hovered_object, "pixels_per_point": self.samples.last().map(|sample| sample.pixels_per_point), "baseline_viewport_points": rect_points(self.viewport), "baseline_viewport_ui_points": rect_points(self.viewport_ui_rect), "initial_camera_view_projection": self.initial_camera.view_projection(self.viewport.width() / self.viewport.height()).to_cols_array()},
            "model": {"objects": state.editor.document.objects.len(), "vertices": mesh.map(|mesh| mesh.vertex_count), "faces": mesh.map(|mesh| mesh.face_count), "triangles": mesh.map(|mesh| mesh.triangle_count), "linked_assets": state.asset_views.len()},
            "validity": {"fps_meter_constant": self.fps_meter_constant, "all_frames_focused": self.samples.iter().all(|sample| sample.focused), "no_frame_errors": self.samples.iter().all(|sample| sample.error.is_none()), "constant_viewport": self.samples.iter().all(|sample| sample.viewport_pixels.is_some_and(|size| size[0] > 0 && size[1] > 0) && sample.viewport_pixels == self.samples[0].viewport_pixels && sample.viewport_points == self.samples[0].viewport_points && sample.pixels_per_point == self.samples[0].pixels_per_point), "constant_surface": self.samples.iter().all(|sample| sample.surface_pixels.is_some_and(|size| size[0] > 0 && size[1] > 0) && sample.surface_pixels == self.samples[0].surface_pixels), "viewport_matches_baseline": self.samples.iter().all(|sample| sample.viewport_points == rect_points(self.viewport)), "unchanged_mesh_revision": self.samples.iter().all(|sample| sample.mesh_revision == self.samples[0].mesh_revision), "mode_contract_satisfied": self.samples.iter().all(|sample| sample.mode_contract_satisfied(self.mode()))},
            "summary": {"cpu_frame_ms": distribution(self.samples.iter().map(|sample| sample.cpu_frame_ms)), "frame_start_interval_ms": distribution(self.samples.iter().filter_map(|sample| sample.frame_start_interval_ms)), "cpu_stage_ms": stages},
            "samples": self.samples,
        }))
    }
}
impl Sample {
    fn mode_contract_satisfied(&self, mode: ExecutionMode) -> bool {
        let counts = &self.counters;
        if counts.scene_renders != 1
            || !self
                .viewport_pixels
                .is_some_and(|size| size[0] > 0 && size[1] > 0)
        {
            return false;
        }
        if mode == ExecutionMode::Editor {
            return counts.egui_passes > 0
                && counts.egui_tessellations == 1
                && counts.egui_composites == 1
                && counts.editor_prepares > 0
                && counts.editor_feedback_updates == 1
                && counts.scene_composites == 0;
        }
        let without_egui = counts.egui_passes == 0
            && counts.egui_tessellations == 0
            && counts.egui_texture_updates == 0
            && counts.egui_composites == 0
            && counts.ui_jobs == 0;
        without_egui
            && counts.scene_composites == 1
            && match mode {
                ExecutionMode::Editor => unreachable!(),
                ExecutionMode::Viewport => {
                    counts.editor_prepares > 0 && counts.editor_feedback_updates == 1
                }
                ExecutionMode::Renderer => {
                    counts.editor_prepares == 0
                        && counts.editor_feedback_updates == 0
                        && self.projection.rebuilds == 0
                        && self.projection.vertices_tested == 0
                        && self.projection.geometry_rebuilds == 0
                }
            }
    }
}
fn rect_points(rect: egui::Rect) -> [f32; 4] {
    [rect.min.x, rect.min.y, rect.max.x, rect.max.y]
}
fn distribution(values: impl Iterator<Item = f64>) -> Value {
    let mut values: Vec<_> = values.collect();
    values.sort_by(f64::total_cmp);
    if values.is_empty() {
        return Value::Null;
    }
    let n = values.len();
    let median = (values[(n - 1) / 2] + values[n / 2]) / 2.0;
    json!({"count": n, "median": median, "p95": values[(n * 95).div_ceil(100) - 1], "p99": values[(n * 99).div_ceil(100) - 1], "min": values[0], "max": values[n - 1]})
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
