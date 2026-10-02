//! Browser host: canvas lifecycle and browser effects around the ordinary editor.
//! The JS bridge is an experimental application adapter, not a public modeling kernel.
use crate::{
    asset_io::LoadedDocument,
    input::{actions::ActionId, bindings},
    keyboard_input::{NumberKey, NumberKeyInput},
    navigation_events,
    render::workspace::{FrameTarget, WorkspaceRenderer},
    settings::{ResolvedTheme, SettingsStore},
    shortcuts::{Command, HostEffect, ShortcutFrame},
    workspace_ui::{WorkspaceUi, configure_context},
};
use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::Arc};
use wasm_bindgen::prelude::*;
use web_time::{Duration, Instant};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys, WindowExtWebSys},
    window::{Window, WindowId},
};

mod gestures;
#[cfg(feature = "viewport-measure")]
mod measurement;
#[cfg(not(feature = "viewport-measure"))]
#[path = "web/measurement_disabled.rs"]
mod measurement;
mod settings;
mod settings_events;
mod status;
use crate::measurement::Stage;

const SETTINGS_KEY: &str = "n3.settings.v1";
struct BrowserSettingsStore;
impl SettingsStore for BrowserSettingsStore {
    fn read(&mut self) -> Result<Option<Vec<u8>>, String> {
        storage()?
            .get_item(SETTINGS_KEY)
            .map(|v| v.map(String::into_bytes))
            .map_err(js_error)
    }
    fn compare_and_swap(
        &mut self,
        expected: Option<&[u8]>,
        replacement: &[u8],
    ) -> Result<(), String> {
        if self.read()?.as_deref() != expected {
            return Err("Browser preferences changed. Reload settings before retrying.".into());
        }
        let text = std::str::from_utf8(replacement).map_err(|e| e.to_string())?;
        storage()?.set_item(SETTINGS_KEY, text).map_err(js_error)
    }
}
fn storage() -> Result<web_sys::Storage, String> {
    web_sys::window()
        .ok_or("Browser window unavailable")?
        .local_storage()
        .map_err(js_error)?
        .ok_or("Browser preference storage is unavailable".into())
}
fn js_error(value: JsValue) -> String {
    format!("{value:?}")
}
fn resolved_window_theme(theme: Option<winit::window::Theme>) -> ResolvedTheme {
    if theme == Some(winit::window::Theme::Dark) {
        ResolvedTheme::Dark
    } else {
        ResolvedTheme::Light
    }
}
fn emit(canvas: &web_sys::HtmlCanvasElement, name: &str, detail: &str) {
    let options = web_sys::CustomEventInit::new();
    options.set_detail(&JsValue::from_str(detail));
    if let Ok(event) = web_sys::CustomEvent::new_with_event_init_dict(name, &options) {
        let _ = canvas.dispatch_event(&event);
    }
}
struct WebWindow {
    instance: wgpu::Instance,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    context: egui::Context,
    input: egui_winit::State,
    graphics: WorkspaceRenderer,
    state: WorkspaceUi,
    cursor: Option<egui::Pos2>,
    number_keys: NumberKeyInput,
    modifiers: egui::Modifiers,
    webkit_gesture: crate::input::browser_navigation::WebKitGesture,
    webkit_gesture_owned: bool,
    last_camera_tick: Instant,
    frame_clock: Instant,
    next_repaint: Option<Instant>,
    settings: settings::BrowserSettings<BrowserSettingsStore>,
    settings_clock: Instant,
    frames: u64,
    status: status::StatusCache,
    measurement: measurement::Host,
}
impl WebWindow {
    async fn new(window: Arc<Window>, proxy: EventLoopProxy<AppEvent>) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| e.to_string())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .map_err(|e| e.to_string())?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("n3 workspace device"),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);
        let context = egui::Context::default();
        configure_context(&context);
        if let Some(window) = web_sys::window()
            && let Ok(user_agent) = window.navigator().user_agent()
        {
            context.set_os(egui::os::OperatingSystem::from_user_agent(&user_agent));
        }
        let repaint_proxy = proxy.clone();
        context.set_request_repaint_callback(move |info| {
            if info.delay.is_zero() {
                let _ = repaint_proxy.send_event(AppEvent::Repaint);
            }
        });
        let input = egui_winit::State::new(
            context.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(device.limits().max_texture_dimension_2d as usize),
        );
        let (graphics, scene_texture) =
            WorkspaceRenderer::new(&device, format, [config.width, config.height]);
        let mut state = WorkspaceUi::new(scene_texture);
        state.host_capabilities = crate::workspace_ui::HostCapabilities::BROWSER;
        state.set_system_theme(resolved_window_theme(window.theme()));
        state.settings_location =
            Some("Preferences are stored in this browser for this site.".into());
        let settings_defaults = state.user_settings();
        let settings = settings::BrowserSettings::new(BrowserSettingsStore, settings_defaults)?;
        let measurement = measurement::Host::new(&window, &adapter.get_info());
        let mut native = Self {
            instance,
            window,
            surface,
            device,
            queue,
            config,
            context,
            input,
            graphics,
            state,
            cursor: None,
            number_keys: NumberKeyInput::default(),
            modifiers: egui::Modifiers::NONE,
            webkit_gesture: Default::default(),
            webkit_gesture_owned: false,
            last_camera_tick: Instant::now(),
            frame_clock: Instant::now(),
            next_repaint: None,
            settings,
            settings_clock: Instant::now(),
            frames: 0,
            status: status::StatusCache::default(),
            measurement,
        };
        native.service_settings(true);
        // Creating the canvas can focus it before async WebGPU setup completes.
        // Those events precede this adapter; focusing an already-active canvas
        // later emits no new event. Seed current focus after loading preferences
        // (queued input defers merges), before the first UI frame. Subsequent
        // ordered focus events retain their ordinary cancellation semantics.
        let _ = native.input.on_window_event(
            &native.window,
            &WindowEvent::Focused(native.window.has_focus()),
        );
        Ok(native)
    }

    fn install_opened_asset(
        &mut self,
        path: PathBuf,
        loaded: LoadedDocument,
        append: bool,
    ) -> Result<(), String> {
        self.graphics.install(
            &self.device,
            &self.queue,
            &mut self.state,
            path,
            loaded,
            append,
        )
    }
    fn navigate(&mut self, event: navigation_events::Event) {
        if self.measurement.isolated() {
            return;
        }
        navigation_events::route(
            &mut self.state,
            &self.context,
            self.cursor,
            self.window.has_focus(),
            &self.input.egui_input().events,
            event,
        );
    }
    fn reset_input(&mut self) {
        self.state.editor.cancel();
        self.context.stop_dragging();
        self.state.reset_navigation_input();
        self.cursor = None;
        self.modifiers = egui::Modifiers::NONE;
        self.webkit_gesture.reset();
        self.webkit_gesture_owned = false;
        // egui-winit keeps its own modifier snapshot for stamping subsequent
        // key/pointer events and recognizing clipboard shortcuts. Reset through
        // its adapter too, so dialogs/focus loss cannot leave that snapshot held.
        let _ = self.input.on_window_event(
            &self.window,
            &WindowEvent::ModifiersChanged(winit::keyboard::ModifiersState::empty().into()),
        );
        self.number_keys.reset();
    }
    fn resize(&mut self) {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        self.window.request_redraw();
    }
    fn event(&mut self, event: &WindowEvent, event_loop: &ActiveEventLoop) {
        // Capture physical origin before egui-winit combines top-row and numpad
        // digits. Keep its logical keys and printable text unchanged for UI use.
        let number = match event {
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } if !(*is_synthetic && event.state == ElementState::Pressed) => {
                NumberKey::from_physical_key(event.physical_key)
                    .map(|key| (key, event.state == ElementState::Pressed, event.repeat))
            }
            _ => None,
        };
        let event_start = self.input.egui_input().events.len();
        let response = self.input.on_window_event(&self.window, event);
        // egui 0.36 carries modifier changes as ordered events, no longer as
        // a RawInput field. Keep the host snapshot for native trackpad events
        // that are routed before the next egui frame.
        for event in &self.input.egui_input().events[event_start..] {
            if let egui::Event::ModifiersChanged(modifiers) = event {
                self.modifiers = *modifiers;
            }
        }
        if let Some((key, pressed, repeat)) = number {
            for (offset, event) in self.input.egui_input().events[event_start..]
                .iter()
                .enumerate()
            {
                if let egui::Event::Key { modifiers, .. } = event {
                    self.number_keys
                        .record(event_start + offset, key, pressed, repeat, *modifiers);
                    break;
                }
            }
        }
        if response.repaint {
            self.window.request_redraw();
        }
        match event {
            WindowEvent::ModifiersChanged(_) => {
                self.navigate(navigation_events::Event::ModifiersChanged(self.modifiers));
            }
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) => self.resize(),
            WindowEvent::ThemeChanged(theme) => {
                self.state
                    .set_system_theme(resolved_window_theme(Some(*theme)));
                self.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.reset_input();
                self.resize();
            }
            WindowEvent::Focused(false) => {
                self.reset_input();
                self.window.request_redraw();
            }
            WindowEvent::Focused(true) => self.service_settings(true),
            WindowEvent::CursorMoved { position, .. } => {
                let logical = position
                    .to_logical::<f32>(
                        egui_winit::pixels_per_point(&self.context, &self.window) as f64
                    );
                let current = egui::pos2(logical.x, logical.y);
                self.cursor = Some(current);
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                self.webkit_gesture.reset();
                self.webkit_gesture_owned = false;
            }
            WindowEvent::RedrawRequested => self.draw(event_loop),
            _ => {}
        }
    }
    fn draw(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.inner_size().width == 0 || self.window.inner_size().height == 0 {
            return;
        }
        let mut probe = self.measurement.begin_frame(&mut self.state);
        let (frame, reconfigure_after_present) = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            wgpu::CurrentSurfaceTexture::Timeout => {
                self.window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.resize();
                return;
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                match self.instance.create_surface(self.window.clone()) {
                    Ok(surface) => {
                        self.surface = surface;
                        self.resize();
                    }
                    Err(error) => {
                        self.emit(
                            "n3-error",
                            &format!("Failed to recreate WebGPU surface: {error}"),
                        );
                        event_loop.exit();
                    }
                }
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                self.emit(
                    "n3-error",
                    "WebGPU surface validation failed; reload to restart the editor",
                );
                event_loop.exit();
                return;
            }
        };
        probe.end(Stage::Acquire);
        // Preserve the candidate rollback snapshot across either frame path.
        let previous_assets = self.state.asset_views.clone();
        let ctx = self.context.clone();
        let mut output = if self.measurement.without_ui() {
            // Drain host events without entering egui's frame/layout machinery.
            let _ = self.input.take_egui_input(&self.window);
            self.number_keys.reset();
            self.measurement
                .prepare_without_ui(&mut self.state, &mut probe);
            None
        } else {
            let now = Instant::now();
            self.state
                .camera
                .advance_transition(now.duration_since(self.last_camera_tick));
            self.last_camera_tick = now;
            let mut input = self.input.take_egui_input(&self.window);
            if self.measurement.isolated() {
                input.events.clear();
                input.events.push(egui::Event::PointerGone);
                self.number_keys.reset();
            }
            let mut shortcuts = ShortcutFrame::with_number_events(&ctx, self.number_keys.take());
            let mut output = ctx.run_ui(input, |ui| {
                let context = ui.ctx().clone();
                let ctx = &context;
                probe.egui_pass();
                shortcuts.begin_pass(ctx);
                self.state.ui(ui);
                shortcuts
                    .collect_with_transform(ctx, self.state.transform_keyboard_context(ctx, false));
                if self.state.camera.is_transitioning() {
                    ctx.request_repaint();
                }
            });
            probe.end(Stage::Ui);
            for command in self
                .state
                .take_ui_commands()
                .into_iter()
                .chain(shortcuts.commands())
            {
                self.dispatch(command);
                self.window.request_redraw();
                if egui::Popup::is_any_open(&ctx) {
                    // A keyboard-opened menu owns the rest of this input batch.
                    break;
                }
            }
            if let Err(error) = self.state.refresh_mesh() {
                self.state.error = Some(error);
            }
            probe.end(Stage::Commands);
            self.input
                .handle_platform_output(&self.window, std::mem::take(&mut output.platform_output));
            Some(output)
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        probe.end(Stage::HostPrepare);
        if let Some(output) = &mut output {
            self.graphics.paint(
                FrameTarget {
                    device: &self.device,
                    queue: &self.queue,
                    view: &view,
                    size: [self.config.width, self.config.height],
                },
                &ctx,
                &mut self.state,
                output,
                previous_assets,
                &mut probe,
            );
        } else {
            self.measurement.paint_without_ui(
                &mut self.graphics,
                FrameTarget {
                    device: &self.device,
                    queue: &self.queue,
                    view: &view,
                    size: [self.config.width, self.config.height],
                },
                &mut self.state,
                previous_assets,
                egui_winit::pixels_per_point(&ctx, &self.window),
                &mut probe,
            );
        }
        self.window.pre_present_notify();
        self.queue.present(frame);
        // Use the browser's monotonic clock only when enabled, once per handoff.
        // This is app submission cadence, not GPU completion or display timing.
        if self.state.fps_meter.enabled() {
            self.state
                .fps_meter
                .record_submission(self.frame_clock.elapsed());
        }
        if let Some(output) = &mut output {
            self.graphics
                .finish_frame(std::mem::take(&mut output.textures_delta.free));
        }
        probe.end(Stage::Present);
        if reconfigure_after_present {
            self.resize();
        }
        let delay = output
            .as_ref()
            .and_then(|output| output.viewport_output.get(&egui::ViewportId::ROOT))
            .map(|v| v.repaint_delay)
            .unwrap_or(Duration::MAX);
        self.next_repaint = Instant::now().checked_add(delay);
        self.service_settings(false);
        if std::mem::take(&mut self.state.request_save) {
            self.state.request_save_as = false;
            self.emit("n3-request", "save");
        }
        if std::mem::take(&mut self.state.request_new) {
            self.emit("n3-request", "new");
        }
        self.frames += 1;
        let status = status::Status::read(
            &self.state,
            self.frames > 0,
            [self.config.width, self.config.height],
        );
        if let Some(snapshot) = self.status.update(status) {
            self.emit("n3-state", &snapshot);
        }
        probe.end(Stage::HostTail);
        self.measurement
            .finish_frame(probe, &self.state, self.window.has_focus());
        if self.measurement.active() {
            self.window.request_redraw();
        }
    }

    fn emit(&self, name: &str, detail: &str) {
        if let Some(canvas) = self.window.canvas() {
            emit(&canvas, name, detail);
        }
    }
    fn dispatch(&mut self, command: Command) {
        let effect = self.state.dispatch(command, &self.context, false);
        match effect {
            HostEffect::Open => self.emit("n3-request", "open"),
            HostEffect::Import => self.emit("n3-request", "import"),
            HostEffect::Quit => self.emit("n3-request", "quit"),
            _ => {}
        }
        self.window.request_redraw();
    }
    fn snapshot(&self) -> String {
        status::Status::read(
            &self.state,
            self.frames > 0,
            [self.config.width, self.config.height],
        )
        .json()
    }

    fn settings_ready_for_sync(&self) -> bool {
        // Queued input may begin a gesture in the next UI pass. Preference
        // merges, including direct wrapper exports, wait until it resolves.
        self.input.egui_input().events.is_empty()
            && self.state.settings_ready_for_sync(&self.context)
    }

    fn service_settings(&mut self, force: bool) {
        if self.measurement.isolated() {
            return;
        }
        let reload = self.state.request_reload_settings;
        let export = self.state.request_open_settings;
        if force || export {
            self.settings.request_sync();
        }
        let previous = self.state.user_settings();
        let ready = self.settings_ready_for_sync();
        let Some(result) =
            self.settings
                .sync(&previous, self.settings_clock.elapsed(), ready, reload)
        else {
            return;
        };
        self.state.request_reload_settings = false;
        self.state.request_open_settings = false;
        if export {
            self.emit("n3-request", "settings");
        }
        let _ = self.apply_settings_result(result);
    }

    fn apply_settings_result(
        &mut self,
        result: Result<crate::settings::Settings, String>,
    ) -> Result<(), String> {
        let previous = self.state.user_settings();
        let previous_error = self.state.settings_error.clone();
        let result = match result {
            Ok(settings) => {
                self.state.apply_user_settings(&settings);
                self.state.settings_error = None;
                Ok(())
            }
            Err(error) => {
                self.state.settings_error = Some(error.clone());
                Err(error)
            }
        };
        if previous != self.state.user_settings() || previous_error != self.state.settings_error {
            self.window.request_redraw();
        }
        result
    }
}

enum AppEvent {
    Repaint,
    SettingsChanged,
    Stop,
}
struct App {
    shared: Rc<RefCell<Option<WebWindow>>>,
    canvas: web_sys::HtmlCanvasElement,
    proxy: EventLoopProxy<AppEvent>,
    initializing: bool,
}
impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.initializing {
            return;
        }
        self.initializing = true;
        let attributes = Window::default_attributes()
            .with_title("N3")
            .with_canvas(Some(self.canvas.clone()))
            .with_prevent_default(true)
            .with_inner_size(LogicalSize::new(
                self.canvas.client_width().max(1) as f64,
                self.canvas.client_height().max(1) as f64,
            ));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                emit(&self.canvas, "n3-error", &error.to_string());
                return;
            }
        };
        let shared = self.shared.clone();
        let canvas = self.canvas.clone();
        let proxy = self.proxy.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match WebWindow::new(window, proxy).await {
                Ok(workspace) => {
                    workspace.window.request_redraw();
                    *shared.borrow_mut() = Some(workspace);
                    emit(&canvas, "n3-ready", "WebGPU");
                }
                Err(error) => emit(&canvas, "n3-error", &error),
            }
        });
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if let Some(workspace) = self.shared.borrow_mut().as_mut() {
            workspace.event(&event, event_loop);
        }
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::Stop => {
                self.shared.borrow_mut().take();
                event_loop.exit();
            }
            AppEvent::Repaint => {
                if let Some(workspace) = self.shared.borrow().as_ref() {
                    workspace.window.request_redraw();
                }
            }
            AppEvent::SettingsChanged => {
                if let Some(workspace) = self.shared.borrow_mut().as_mut() {
                    workspace.service_settings(true);
                }
            }
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let mut shared = self.shared.borrow_mut();
        if let Some(workspace) = shared.as_mut() {
            workspace.service_settings(false);
            if workspace
                .next_repaint
                .is_some_and(|deadline| deadline <= Instant::now())
            {
                workspace.next_repaint = None;
                workspace.window.request_redraw();
            }
            // Idle tabs have no preference timer. Storage/focus events and local
            // edits wake synchronization; only failed attempts schedule retries.
            let deadline = workspace
                .next_repaint
                .into_iter()
                .chain(
                    workspace
                        .settings
                        .deadline()
                        .and_then(|deadline| workspace.settings_clock.checked_add(deadline)),
                )
                .min();
            if let Some(deadline) = deadline {
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
                return;
            }
        }
        event_loop.set_control_flow(ControlFlow::Wait);
    }
}

/// One editor canvas per module instance. A wrapper owns DOM, files and lifecycle.
#[wasm_bindgen]
pub struct WebApp {
    shared: Rc<RefCell<Option<WebWindow>>>,
    proxy: EventLoopProxy<AppEvent>,
    gestures: RefCell<Option<gestures::CanvasGestures>>,
    settings_events: RefCell<Option<settings_events::SettingsEvents>>,
}
#[wasm_bindgen]
pub fn start(canvas: web_sys::HtmlCanvasElement) -> Result<WebApp, JsValue> {
    console_error_panic_hook::set_once();
    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let proxy = event_loop.create_proxy();
    let shared = Rc::new(RefCell::new(None));
    let gestures = gestures::CanvasGestures::install(&canvas, &shared)?;
    let settings_events = settings_events::SettingsEvents::install(proxy.clone())?;
    event_loop.spawn_app(App {
        shared: shared.clone(),
        canvas,
        proxy: proxy.clone(),
        initializing: false,
    });
    Ok(WebApp {
        shared,
        proxy,
        gestures: RefCell::new(Some(gestures)),
        settings_events: RefCell::new(Some(settings_events)),
    })
}
#[wasm_bindgen]
impl WebApp {
    pub fn command(&self, id: &str) -> Result<(), JsValue> {
        let command = ActionId::ALL
            .iter()
            .find(|action| action.id() == id)
            .map(|a| a.command())
            .or_else(|| {
                bindings::BINDINGS
                    .iter()
                    .find(|binding| binding.id == id)
                    .and_then(|b| b.command)
            })
            .ok_or_else(|| JsValue::from_str("Unknown semantic action"))?;
        self.with_mut(|workspace| {
            workspace.dispatch(command);
            Ok(())
        })
    }
    pub fn load_file(&self, name: &str, bytes: &[u8], append: bool) -> Result<(), JsValue> {
        self.with_mut(|workspace| {
            if !workspace.state.document_action_allowed() {
                return Err("Apply or cancel the active edit before loading a file".into());
            }
            let loaded = crate::asset_io::load_bytes(name, bytes)?;
            workspace.reset_input();
            workspace.install_opened_asset(PathBuf::from(name), loaded, append)?;
            workspace.window.request_redraw();
            Ok(())
        })
    }
    pub fn document_json(&self) -> Result<String, JsValue> {
        self.with_mut(|workspace| {
            if !workspace.state.document_action_allowed() {
                return Err("Apply or cancel the active edit before downloading".into());
            }
            workspace.state.editor.document.to_json()
        })
    }
    /// Explicit replacement after the wrapper has resolved unsaved changes.
    pub fn new_document(&self) -> Result<(), JsValue> {
        self.with_mut(|workspace| {
            if !workspace.state.document_action_allowed() {
                return Err("Apply or cancel the active edit before creating a document".into());
            }
            workspace.reset_input();
            workspace.state.new_document();
            workspace.window.request_redraw();
            Ok(())
        })
    }
    pub fn snapshot(&self) -> Result<String, JsValue> {
        self.with_mut(|workspace| Ok(workspace.snapshot()))
    }
    pub fn settings_json(&self) -> Result<String, JsValue> {
        self.with_mut(|workspace| {
            if workspace.measurement.isolated() {
                return Err("Preference storage is disabled in the measurement harness".into());
            }
            if !workspace.settings_ready_for_sync() {
                return Err(
                    "Finish the active input or edit, then retry exporting preferences.".into(),
                );
            }
            let result = workspace.settings.ensure_file(
                &workspace.state.user_settings(),
                workspace.settings_clock.elapsed(),
            );
            // A direct JS call can arrive while winit is waiting indefinitely.
            // Wake it even on failure so the retry deadline reaches WaitUntil.
            workspace.window.request_redraw();
            workspace.apply_settings_result(result)?;
            let bytes = BrowserSettingsStore
                .read()?
                .ok_or("Browser preferences are unavailable")?;
            String::from_utf8(bytes).map_err(|e| e.to_string())
        })
    }
    /// Size is in CSS pixels; winit applies the device scale factor.
    pub fn resize(&self, width: f64, height: f64) -> Result<(), JsValue> {
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(JsValue::from_str("Canvas size must be positive and finite"));
        }
        self.with_mut(|workspace| {
            let _ = workspace
                .window
                .request_inner_size(LogicalSize::new(width, height));
            workspace.window.request_redraw();
            Ok(())
        })
    }
    pub fn destroy(&self) {
        self.gestures.borrow_mut().take();
        self.settings_events.borrow_mut().take();
        let _ = self.proxy.send_event(AppEvent::Stop);
    }
}
impl WebApp {
    fn with_mut<T>(
        &self,
        run: impl FnOnce(&mut WebWindow) -> Result<T, String>,
    ) -> Result<T, JsValue> {
        let mut shared = self.shared.try_borrow_mut().map_err(|_| {
            JsValue::from_str("Editor is busy; defer wrapper callbacks to a microtask")
        })?;
        let workspace = shared
            .as_mut()
            .ok_or_else(|| JsValue::from_str("Editor is not ready or has been destroyed"))?;
        run(workspace).map_err(|e| JsValue::from_str(&e))
    }
}
