//! Contributor-only browser collection, isolated from origin preferences.
use super::*;
use crate::measurement::{FrameProbe, Options, Session};
use serde_json::{Value, json};

pub(super) struct Host {
    isolated: bool,
    session: Option<Session>,
    result: Option<Value>,
    adapter: Value,
}
impl Host {
    pub(super) fn new(window: &Window, adapter: &wgpu::AdapterInfo) -> Self {
        Self {
            isolated: window.canvas().is_some_and(|canvas| {
                canvas.get_attribute("data-n3-measure").as_deref() == Some("true")
            }),
            session: None,
            result: None,
            adapter: json!({"name": adapter.name, "vendor": adapter.vendor, "device": adapter.device, "device_type": format!("{:?}", adapter.device_type), "driver": adapter.driver, "driver_info": adapter.driver_info, "backend": format!("{:?}", adapter.backend)}),
        }
    }
    pub(super) fn isolated(&self) -> bool {
        self.isolated
    }
    pub(super) fn active(&self) -> bool {
        self.session.as_ref().is_some_and(Session::active)
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

    pub(super) fn begin_frame(&mut self, state: &mut WorkspaceUi) -> FrameProbe {
        self.session
            .as_mut()
            .map(|session| session.begin_frame(state))
            .unwrap_or_default()
    }
    pub(super) fn finish_frame(&mut self, probe: FrameProbe, state: &WorkspaceUi, focused: bool) {
        if let Some(session) = &mut self.session
            && let Some(result) = session.finish_frame(probe, state, focused)
        {
            self.result = Some(result);
        }
    }
}
#[wasm_bindgen]
impl WebApp {
    /// Available only in the contributor feature, on an isolated harness canvas.
    pub fn measure_start(&self, options_json: &str, metadata_json: &str) -> Result<(), JsValue> {
        self.with_mut(|workspace| {
            if !workspace.measurement.isolated || workspace.measurement.active() {
                return Err("Use an idle contributor measurement canvas".into());
            }
            let options: Options = serde_json::from_str(options_json).map_err(|error| error.to_string())?;
            options.validate()?;
            if workspace.state.loading.is_some()
                || workspace.state.mesh.is_none()
                || !workspace.state.viewport.is_finite()
                || !workspace.state.viewport.is_positive()
                || !workspace.state.document_action_allowed()
            {
                return Err("Wait for a loaded, laid-out viewport without an active edit".into());
            }
            if let Some(error) = &workspace.state.error {
                return Err(error.clone());
            }
            let mut metadata: Value = serde_json::from_str(metadata_json).map_err(|error| error.to_string())?;
            if !metadata.is_object() { return Err("Measurement metadata must be an object".into()); }
            metadata["host"] = json!("web");
            metadata["adapter"] = workspace.measurement.adapter.clone();
            metadata["surface"] = json!({"width": workspace.config.width, "height": workspace.config.height, "scale_factor": workspace.window.scale_factor(), "format": format!("{:?}", workspace.config.format), "present_mode": format!("{:?}", workspace.config.present_mode), "desired_maximum_frame_latency": workspace.config.desired_maximum_frame_latency, "alpha_mode": format!("{:?}", workspace.config.alpha_mode)});
            workspace.reset_input();
            let session = Session::new(options, metadata, &mut workspace.state)?;
            workspace.measurement.session = Some(session);
            workspace.measurement.result = None;
            workspace.window.request_redraw();
            Ok(())
        })
    }
    pub fn measure_result(&self) -> Result<String, JsValue> {
        self.with_mut(|workspace| {
            serde_json::to_string(&workspace.measurement.result).map_err(|error| error.to_string())
        })
    }
}
