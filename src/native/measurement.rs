//! Opt-in native measurement lifecycle and output. The shared session owns the
//! workload and sample schema; ordinary native scheduling and preferences remain
//! unchanged when no measurement was requested.
use crate::{
    measurement::{FrameProbe, Options, Session},
    workspace_ui::WorkspaceUi,
};
use serde_json::{Value, json};
use std::{env, io::Write, path::PathBuf};
use winit::{
    dpi::PhysicalSize,
    window::{Window, WindowAttributes},
};

#[derive(Default)]
pub(super) struct Host {
    pending: Option<Options>,
    session: Option<Session>,
    output: Option<PathBuf>,
    metadata: Value,
    size: Option<PhysicalSize<u32>>,
}

impl Host {
    pub(super) fn from_environment() -> Result<Self, String> {
        let Some(options) = env::var_os("N3_VIEWPORT_MEASURE") else {
            return Ok(Self::default());
        };
        let options = options
            .into_string()
            .map_err(|_| "N3_VIEWPORT_MEASURE must be UTF-8 JSON")?;
        let options: Options = serde_json::from_str(&options)
            .map_err(|error| format!("Invalid N3_VIEWPORT_MEASURE: {error}"))?;
        options.validate()?;
        let output = env::var_os("N3_VIEWPORT_MEASURE_OUTPUT")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or("N3_VIEWPORT_MEASURE_OUTPUT must name the output JSON file")?;
        let metadata: Value = match env::var("N3_VIEWPORT_MEASURE_METADATA") {
            Ok(value) => serde_json::from_str(&value)
                .map_err(|error| format!("Invalid N3_VIEWPORT_MEASURE_METADATA: {error}"))?,
            Err(env::VarError::NotPresent) => json!({}),
            Err(error) => return Err(format!("Invalid N3_VIEWPORT_MEASURE_METADATA: {error}")),
        };
        if !metadata.is_object() {
            return Err("N3_VIEWPORT_MEASURE_METADATA must be a JSON object".into());
        }
        let size = match env::var("N3_VIEWPORT_MEASURE_SIZE") {
            Ok(value) => Some(parse_size(&value)?),
            Err(env::VarError::NotPresent) => None,
            Err(error) => return Err(format!("Invalid N3_VIEWPORT_MEASURE_SIZE: {error}")),
        };
        Ok(Self {
            pending: Some(options),
            session: None,
            output: Some(output),
            metadata,
            size,
        })
    }

    pub(super) fn configured(&self) -> bool {
        self.output.is_some()
    }

    pub(super) fn active(&self) -> bool {
        self.pending.is_some() || self.session.as_ref().is_some_and(Session::active)
    }

    pub(super) fn window_attributes(&self, attributes: WindowAttributes) -> WindowAttributes {
        match self.size {
            Some(size) => attributes
                .with_min_inner_size(PhysicalSize::new(1, 1))
                .with_inner_size(size),
            None => attributes,
        }
    }

    pub(super) fn set_adapter(&mut self, adapter: &wgpu::AdapterInfo) {
        if self.configured() {
            self.metadata["host"] = json!("native");
            self.metadata["adapter"] = json!({
                "name": adapter.name,
                "vendor": adapter.vendor,
                "device": adapter.device,
                "device_type": format!("{:?}", adapter.device_type),
                "driver": adapter.driver,
                "driver_info": adapter.driver_info,
                "backend": format!("{:?}", adapter.backend),
            });
        }
    }

    pub(super) fn without_ui(&self) -> bool {
        self.session.as_ref().is_some_and(Session::without_ui)
    }

    pub(super) fn prepare_without_ui(&self, state: &mut WorkspaceUi, probe: &mut FrameProbe) {
        if let Some(session) = &self.session {
            session.prepare_without_ui(state, probe);
        }
    }

    pub(super) fn paint_without_ui(
        &self,
        graphics: &mut crate::render::workspace::WorkspaceRenderer,
        target: crate::render::workspace::FrameTarget<'_>,
        state: &mut WorkspaceUi,
        previous_assets: std::collections::BTreeMap<
            crate::document::AssetInstance,
            crate::scene_view::SceneView,
        >,
        pixels_per_point: f32,
        probe: &mut FrameProbe,
    ) {
        let session = self.session.as_ref().expect("active no-UI measurement");
        graphics.paint_without_ui(
            target,
            state,
            previous_assets,
            pixels_per_point,
            session.mode() == crate::measurement::ExecutionMode::Viewport,
            probe,
        );
    }

    pub(super) fn begin_frame(
        &mut self,
        state: &mut WorkspaceUi,
        window: &Window,
        config: &wgpu::SurfaceConfiguration,
    ) -> Result<FrameProbe, String> {
        if !self.configured() || state.loading.is_some() {
            return Ok(FrameProbe::default());
        }
        if let Some(error) = &state.error {
            return Err(error.clone());
        }
        if state.mesh.is_none() {
            return Err("The measurement input contains no renderable mesh".into());
        }
        if !state.viewport.is_finite() || !state.viewport.is_positive() {
            return Ok(FrameProbe::default());
        }
        if let Some(options) = self.pending.take() {
            // Snapshot actual configured dimensions after initial window and UI
            // layout events; requested dimensions alone are not measurement data.
            self.metadata["surface"] = json!({
                "width": config.width,
                "height": config.height,
                "scale_factor": window.scale_factor(),
                "format": format!("{:?}", config.format),
                "present_mode": format!("{:?}", config.present_mode),
                "desired_maximum_frame_latency": config.desired_maximum_frame_latency,
                "alpha_mode": format!("{:?}", config.alpha_mode),
                "color_space": format!("{:?}", config.color_space),
            });
            self.session = Some(Session::new(options, self.metadata.take(), state)?);
        }
        Ok(self
            .session
            .as_mut()
            .map(|session| session.begin_frame(state))
            .unwrap_or_default())
    }

    /// Return true only after the complete report has reached the requested file.
    /// Serialization and filesystem work occur after collection, outside samples.
    pub(super) fn finish_frame(
        &mut self,
        probe: FrameProbe,
        state: &WorkspaceUi,
        focused: bool,
    ) -> Result<bool, String> {
        let Some(session) = &mut self.session else {
            return Ok(false);
        };
        if let Some(error) = &state.error {
            return Err(error.clone());
        }
        let Some(report) = session.finish_frame(probe, state, focused) else {
            return Ok(false);
        };
        let output = self.output.as_ref().expect("configured measurement output");
        let mut bytes = serde_json::to_vec_pretty(&report)
            .map_err(|error| format!("Could not encode viewport measurement: {error}"))?;
        bytes.push(b'\n');
        // A completed run must never replace evidence from an earlier run.
        // create_new makes this guarantee atomic even when callers race.
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .and_then(|mut file| file.write_all(&bytes))
            .map_err(|error| {
                format!(
                    "Could not write viewport measurement {}: {error}",
                    output.display()
                )
            })?;
        println!("Viewport measurement saved to {}", output.display());
        Ok(true)
    }
}

fn parse_size(value: &str) -> Result<PhysicalSize<u32>, String> {
    let invalid = || "N3_VIEWPORT_MEASURE_SIZE must be positive WIDTHxHEIGHT pixels".to_owned();
    let (width, height) = value.split_once('x').ok_or_else(invalid)?;
    let width = width.parse::<u32>().map_err(|_| invalid())?;
    let height = height.parse::<u32>().map_err(|_| invalid())?;
    if width == 0 || height == 0 {
        return Err(invalid());
    }
    Ok(PhysicalSize::new(width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct OutputDirectory(PathBuf);

    impl OutputDirectory {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let path = env::temp_dir().join(format!(
                "n3-native-measure-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn report(&self) -> PathBuf {
            self.0.join("report.json")
        }
    }

    impl Drop for OutputDirectory {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn fixture() -> WorkspaceUi {
        let mut state = WorkspaceUi::new(egui::TextureId::User(0));
        state.viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/obj/cube-quads.obj");
        let loaded = crate::asset_io::load(&path).unwrap();
        state.install_loaded_document(path, loaded).unwrap();
        state
    }

    fn collecting_host(options: Value, output: PathBuf, state: &mut WorkspaceUi) -> Host {
        let options = serde_json::from_value(options).unwrap();
        Host {
            session: Some(Session::new(options, json!({"host": "native"}), state).unwrap()),
            output: Some(output),
            ..Host::default()
        }
    }

    fn next_frame(host: &mut Host, state: &mut WorkspaceUi) -> Result<bool, String> {
        let probe = host.session.as_mut().unwrap().begin_frame(state);
        host.finish_frame(probe, state, true)
    }

    #[test]
    fn measurement_size_is_explicit_positive_physical_pixels() {
        assert_eq!(
            parse_size("1280x720").unwrap(),
            PhysicalSize::new(1280, 720)
        );
        for invalid in ["0x720", "1280x0", "-1x720", "1280", "1280x720x2", "inf x 1"] {
            assert!(parse_size(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn report_is_written_only_after_warmup_and_all_samples_without_mutating_document() {
        let output = OutputDirectory::new();
        let mut state = fixture();
        let document = state.editor.document.clone();
        let revision = state.mesh_revision;
        let mut host = collecting_host(
            json!({
                "warmup_frames": 2,
                "sample_frames": 2,
                "workload": "stationary",
                "selected": true,
                "instrument_stages": false,
            }),
            output.report(),
            &mut state,
        );
        for _ in 0..3 {
            assert!(!next_frame(&mut host, &mut state).unwrap());
            assert!(!output.report().exists());
            assert!(host.active());
        }
        assert!(next_frame(&mut host, &mut state).unwrap());
        assert!(!host.active());
        assert!(
            host.configured(),
            "settings remain isolated after collection"
        );
        let bytes = std::fs::read(output.report()).unwrap();
        let report: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(report["schema"], "n3.viewport-measure.v2");
        assert_eq!(report["metadata"]["host"], "native");
        assert_eq!(report["samples"].as_array().unwrap().len(), 2);
        assert_eq!(report["samples"][0]["frame"], 0);
        assert_eq!(report["samples"][1]["frame"], 1);
        assert!(report["samples"][0]["cpu_stage_ms"].is_null());
        assert_eq!(report["render"]["selected_objects"], 1);
        assert_eq!(state.editor.document, document);
        assert_eq!(state.mesh_revision, revision);
    }

    #[test]
    fn existing_report_is_never_replaced() {
        let output = OutputDirectory::new();
        let previous = b"existing measurement evidence\n";
        std::fs::write(output.report(), previous).unwrap();
        let mut state = fixture();
        let mut host = collecting_host(
            json!({"warmup_frames": 0, "sample_frames": 1}),
            output.report(),
            &mut state,
        );
        let error = next_frame(&mut host, &mut state).unwrap_err();
        assert!(error.contains("Could not write viewport measurement"));
        assert_eq!(std::fs::read(output.report()).unwrap(), previous);
    }

    #[test]
    fn frame_failure_does_not_publish_a_completed_report() {
        let output = OutputDirectory::new();
        let mut state = fixture();
        let mut host = collecting_host(
            json!({"warmup_frames": 0, "sample_frames": 1}),
            output.report(),
            &mut state,
        );
        state.error = Some("Rejected GPU upload".into());
        assert_eq!(
            next_frame(&mut host, &mut state).unwrap_err(),
            "Rejected GPU upload"
        );
        assert!(!output.report().exists());
    }
}
