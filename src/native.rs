//! macOS window, dialogs and event-loop adapter.
#[cfg(feature = "viewport-measure")]
mod measurement;
#[cfg(not(feature = "viewport-measure"))]
#[path = "native/measurement_disabled.rs"]
mod measurement;
mod settings_host;
mod settings_store;

use crate::asset_io::LoadedDocument;
use crate::measurement::Stage;
use crate::render::workspace::{FrameTarget, WorkspaceRenderer};
use crate::{
    document_io, keyboard_input, navigation_events, scroll_input,
    settings::{ResolvedTheme, Settings, ThemeMode},
    shortcuts, workspace_ui,
};
use keyboard_input::{NumberKey, NumberKeyInput};
use settings_host::SettingsHost;
use settings_store::FileSettingsStore;
use shortcuts::{HostEffect, ShortcutFrame};
use std::{
    path::PathBuf,
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};
use workspace_ui::{WorkspaceUi, configure_context, filename};

const INITIAL_WINDOW_SIZE: LogicalSize<f64> = LogicalSize::new(1440.0, 900.0);
const MINIMUM_WINDOW_SIZE: LogicalSize<f64> = LogicalSize::new(820.0, 500.0);

fn initial_window_size(screen: Option<LogicalSize<f64>>) -> LogicalSize<f64> {
    // Winit exposes the monitor's full bounds, not its desktop work area.
    // Leave a 10% margin for window chrome and the surrounding desktop. Use
    // logical points so Retina scaling does not make the window oversized.
    let Some(screen) = screen.filter(|size| {
        size.width.is_finite() && size.height.is_finite() && size.width > 0.0 && size.height > 0.0
    }) else {
        return INITIAL_WINDOW_SIZE;
    };
    LogicalSize::new(
        INITIAL_WINDOW_SIZE
            .width
            .min((screen.width * 0.9).floor().max(1.0)),
        INITIAL_WINDOW_SIZE
            .height
            .min((screen.height * 0.9).floor().max(1.0)),
    )
}

enum AppEvent {
    Loaded {
        generation: u64,
        path: PathBuf,
        result: Result<LoadedDocument, String>,
        append: bool,
    },
    Repaint,
}

fn resolved_window_theme(theme: Option<winit::window::Theme>) -> ResolvedTheme {
    match theme {
        Some(winit::window::Theme::Dark) => ResolvedTheme::Dark,
        Some(winit::window::Theme::Light) | None => ResolvedTheme::Light,
    }
}

struct NativeWindow {
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
    applied_window_theme: ThemeMode,
    load_document: crate::document::Document,
    cursor: Option<egui::Pos2>,
    number_keys: NumberKeyInput,
    modifiers: egui::Modifiers,
    generation: u64,
    last_camera_tick: Instant,
    frame_clock: Instant,
    proxy: EventLoopProxy<AppEvent>,
    next_repaint: Option<Instant>,
    pending_open: bool,
    pending_import: bool,
    pending_quit: bool,
    settings: Option<SettingsHost<FileSettingsStore>>,
    settings_path: Option<PathBuf>,
    settings_open_error: Option<String>,
    settings_defaults: Settings,
    measurement: measurement::Host,
}

impl NativeWindow {
    async fn new(
        window: Arc<Window>,
        proxy: EventLoopProxy<AppEvent>,
        mut measurement: measurement::Host,
    ) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL,
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
        println!(
            "GPU: {} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend
        );
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
        measurement.set_adapter(&adapter.get_info());
        surface.configure(&device, &config);
        let context = egui::Context::default();
        configure_context(&context);
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
        // Native sessions are opt-in: guide/workbench hosts keep isolated fixtures.
        // The process starts only after opening Terminal and measuring its grid.
        state.tool_dock.terminal = crate::terminal::TerminalSession::dormant_native();
        state.set_system_theme(resolved_window_theme(window.theme()));
        let settings_defaults = state.user_settings();
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
            applied_window_theme: ThemeMode::System,
            load_document: crate::document::Document::default(),
            cursor: None,
            number_keys: NumberKeyInput::default(),
            modifiers: egui::Modifiers::NONE,
            generation: 0,
            last_camera_tick: Instant::now(),
            frame_clock: Instant::now(),
            proxy,
            next_repaint: None,
            pending_open: false,
            pending_import: false,
            pending_quit: false,
            settings: None,
            settings_path: None,
            settings_open_error: None,
            settings_defaults,
            measurement,
        };
        native.initialize_settings();
        Ok(native)
    }

    fn initialize_settings(&mut self) {
        if self.measurement.configured() {
            // Measurement sessions use isolated defaults and never touch the
            // user's global store, including while the input file is loading.
            self.state.apply_user_settings(&Settings::default());
            return;
        }
        let result = (|| {
            let store = FileSettingsStore::global()?;
            let path = store.path().to_path_buf();
            let host = SettingsHost::new(store, self.settings_defaults.clone(), Instant::now())?;
            self.state.settings_location = Some(path.display().to_string());
            self.settings_path = Some(path);
            self.settings = Some(host);
            Ok::<_, String>(())
        })();
        if let Err(error) = result {
            self.report_settings_error(Some(error));
            return;
        }
        // The controller's first synchronization loads existing settings. A
        // missing file returns defaults without creating directories or files.
        let local = self.state.user_settings();
        let result = self
            .settings
            .as_mut()
            .unwrap()
            .sync(&local, Instant::now(), true)
            .unwrap();
        self.apply_settings_result(result);
    }

    fn report_settings_error(&mut self, error: Option<String>) {
        if self.state.settings_error == error {
            return;
        }
        if error.is_some() {
            self.state.show_ui = true;
            self.state.show_preferences = true;
        }
        self.state.settings_error = error;
        self.window.request_redraw();
    }

    fn apply_settings_result(&mut self, result: Result<Settings, String>) -> bool {
        match result {
            Ok(settings) => {
                if self.state.user_settings() != settings {
                    self.state.apply_user_settings(&settings);
                    self.window.request_redraw();
                }
                self.report_settings_error(self.settings_open_error.clone());
                true
            }
            Err(error) => {
                self.report_settings_error(Some(error));
                false
            }
        }
    }

    fn service_settings(&mut self) {
        if self.measurement.configured() {
            return;
        }
        let now = Instant::now();
        // Native input can be queued before egui has processed the next pass.
        // Do not let a timer apply preferences across that pending boundary.
        if !self.input.egui_input().events.is_empty()
            || !self.state.settings_ready_for_sync(&self.context)
        {
            if let Some(host) = &mut self.settings {
                host.defer_poll(now);
            }
            return;
        }
        let reload = std::mem::take(&mut self.state.request_reload_settings);
        let open = std::mem::take(&mut self.state.request_open_settings);
        if self.settings.is_none() && (reload || open) {
            self.initialize_settings();
        }
        if self.settings.is_none() {
            return;
        }
        if reload {
            self.settings_open_error = None;
            let local = self.state.user_settings();
            let result = self.settings.as_mut().unwrap().reload(&local, now);
            self.apply_settings_result(result);
        }
        if open {
            self.settings_open_error = None;
            let local = self.state.user_settings();
            // ensure_file performs the same merge as sync, creating a missing
            // file only for this explicit user action.
            let result = self.settings.as_mut().unwrap().ensure_file(&local, now);
            let saved = self.apply_settings_result(result);
            if let Some(path) = &self.settings_path
                && (saved || path.is_file())
            {
                // A malformed existing file still needs to open for repair.
                // Pass the path as an argument, never through a shell.
                let result = Command::new("open").arg("-t").arg(path).output();
                let error = match result {
                    Ok(output) if output.status.success() => None,
                    Ok(output) => Some(format!(
                        "Could not open settings in your text editor: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    )),
                    Err(error) => Some(format!(
                        "Could not open settings in your text editor: {error}"
                    )),
                };
                if let Some(error) = error {
                    self.settings_open_error = Some(error.clone());
                    self.report_settings_error(Some(error));
                }
            }
        } else if !reload {
            let local = self.state.user_settings();
            if let Some(result) = self.settings.as_mut().unwrap().sync(&local, now, false) {
                self.apply_settings_result(result);
            }
        }
    }

    /// Called only after the user has resolved the document's Save/Discard
    /// decision. An unrelated malformed settings file must never trap quitting.
    fn flush_settings_before_quit(&mut self) -> bool {
        if self.measurement.configured() {
            return true;
        }
        let local = self.state.user_settings();
        let result = match &mut self.settings {
            Some(host) if host.has_pending(&local) => {
                host.sync(&local, Instant::now(), true).unwrap()
            }
            Some(_) => return true,
            None if local == self.settings_defaults => return true,
            None => Err(self
                .state
                .settings_error
                .clone()
                .unwrap_or_else(|| "The global settings location is unavailable.".into())),
        };
        if self.apply_settings_result(result) {
            return true;
        }
        let answer = rfd::MessageDialog::new()
            .set_title("Preferences could not be saved")
            .set_description(format!(
                "{}\n\nQuit without saving your preference changes?",
                self.state
                    .settings_error
                    .as_deref()
                    .unwrap_or("The global settings file could not be updated.")
            ))
            .set_buttons(rfd::MessageButtons::OkCancelCustom(
                "Quit without saving preferences".into(),
                "Stay open".into(),
            ))
            .show();
        match answer {
            rfd::MessageDialogResult::Ok => true,
            rfd::MessageDialogResult::Custom(label) => label == "Quit without saving preferences",
            _ => false,
        }
    }
    fn open_dialog(&mut self, append: bool) {
        if !self.state.document_action_allowed() {
            self.window.request_redraw();
            return;
        }
        self.reset_input();
        let mut dialog = rfd::FileDialog::new()
            .set_title(if append {
                "Import objects into N3"
            } else {
                "Open N3, OBJ, glTF or GLB"
            })
            .add_filter(
                "N3, Wavefront OBJ or glTF scene",
                &["json", "obj", "gltf", "glb"],
            );
        if let Some(parent) = self.state.path.as_ref().and_then(|p| p.parent()) {
            dialog = dialog.set_directory(parent);
        }
        if let Some(path) = dialog.pick_file() {
            self.load_into(path, append);
        }
        self.window.request_redraw();
    }
    fn load(&mut self, path: PathBuf) {
        self.load_into(path, false);
    }
    fn load_into(&mut self, path: PathBuf, append: bool) {
        if append {
            if !self.state.editor.can_edit() || !self.state.document_action_allowed() {
                return;
            }
        } else if !self.confirm_discard() {
            return;
        }
        self.state.hovered_file = false;
        self.reset_input();
        if !crate::asset_io::is_supported_path(&path) {
            self.state.error = Some("Choose an .n3.json, .obj, .gltf or .glb file.".into());
            self.window.request_redraw();
            return;
        }
        self.generation += 1;
        let generation = self.generation;
        self.load_document = self.state.editor.document.clone();
        self.state.loading = Some(path.clone());
        self.state.error = None;
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(|| crate::asset_io::load(&path))
                .unwrap_or_else(|_| Err("The loader could not process this file.".to_owned()));
            let _ = proxy.send_event(AppEvent::Loaded {
                generation,
                path,
                result,
                append,
            });
        });
        self.window.request_redraw();
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
    fn save_document(&mut self, save_as: bool) -> bool {
        if !self.state.editor.can_edit() {
            self.state.error = Some("This document is read-only.".into());
            return false;
        }
        if !self.state.document_action_allowed() {
            self.window.request_redraw();
            return false;
        }
        self.reset_input();
        let choose = save_as
            || self
                .state
                .save_path
                .as_ref()
                .is_none_or(|p| !document_io::is_native_path(p));
        let path = if choose {
            let mut dialog = rfd::FileDialog::new()
                .set_title("Save N3 document")
                .add_filter("N3 document", &["json"])
                .set_file_name("Untitled.n3.json");
            if let Some(parent) = self.state.path.as_ref().and_then(|p| p.parent()) {
                dialog = dialog.set_directory(parent);
            }
            let Some(mut path) = dialog.save_file() else {
                return false;
            };
            if !document_io::is_native_path(&path) {
                let stem = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Untitled".into());
                path.set_file_name(format!("{stem}.n3.json"));
                if path.exists()
                    && rfd::MessageDialog::new()
                        .set_title("Replace existing document?")
                        .set_description(format!("Replace {}?", path.display()))
                        .set_buttons(rfd::MessageButtons::YesNo)
                        .show()
                        != rfd::MessageDialogResult::Yes
                {
                    return false;
                }
            }
            path
        } else {
            self.state.save_path.clone().expect("saved path")
        };
        let expected = if !choose {
            self.state.disk_snapshot.as_deref()
        } else {
            None
        };
        match document_io::save(&path, &self.state.editor.document, expected, choose) {
            Ok(bytes) => {
                self.state.mark_saved(path, bytes);
                self.state.error = None;
                self.window.request_redraw();
                true
            }
            Err(error) => {
                self.state.error = Some(error);
                self.window.request_redraw();
                false
            }
        }
    }
    fn confirm_discard(&mut self) -> bool {
        if !self.state.editor.can_edit() {
            return true;
        }
        if !self.state.document_action_allowed() {
            self.window.request_redraw();
            return false;
        }
        self.reset_input();
        if !self.state.is_dirty() {
            return true;
        }
        let answer = rfd::MessageDialog::new()
            .set_title("Save changes to N3?")
            .set_description("Save your changes before closing this document.")
            .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
                "Save".into(),
                "Discard".into(),
                "Cancel".into(),
            ))
            .show();
        match answer {
            rfd::MessageDialogResult::Yes => self.save_document(false),
            rfd::MessageDialogResult::No => true,
            rfd::MessageDialogResult::Custom(label) if label == "Save" => self.save_document(false),
            rfd::MessageDialogResult::Custom(label) if label == "Discard" => true,
            _ => false,
        }
    }
    fn navigate(&mut self, event: navigation_events::Event) {
        if self.measurement.configured() {
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
            WindowEvent::CloseRequested
                if self.confirm_discard() && self.flush_settings_before_quit() =>
            {
                event_loop.exit();
            }
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
            WindowEvent::Focused(true) => {
                if let Some(host) = &mut self.settings {
                    host.request_poll(Instant::now());
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let logical = position
                    .to_logical::<f32>(
                        egui_winit::pixels_per_point(&self.context, &self.window) as f64
                    );
                let current = egui::pos2(logical.x, logical.y);
                self.cursor = Some(current);
            }
            WindowEvent::CursorLeft { .. } => self.cursor = None,
            WindowEvent::MouseWheel { delta, phase, .. } => {
                match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        self.navigate(navigation_events::Event::Wheel {
                            delta: egui::vec2(*x, *y),
                            modifiers: self.modifiers,
                        });
                    }
                    MouseScrollDelta::PixelDelta(delta) => {
                        let d = delta.to_logical::<f32>(egui_winit::pixels_per_point(
                            &self.context,
                            &self.window,
                        ) as f64);
                        let phase = match phase {
                            winit::event::TouchPhase::Started => scroll_input::ScrollPhase::Started,
                            winit::event::TouchPhase::Moved => scroll_input::ScrollPhase::Moved,
                            winit::event::TouchPhase::Ended => scroll_input::ScrollPhase::Ended,
                            winit::event::TouchPhase::Cancelled => {
                                scroll_input::ScrollPhase::Cancelled
                            }
                        };
                        self.navigate(navigation_events::Event::TrackpadScroll {
                            delta: egui::vec2(d.x, d.y),
                            phase,
                            modifiers: self.modifiers,
                        });
                    }
                }
                self.window.request_redraw();
            }
            WindowEvent::PinchGesture { delta, .. } => {
                self.navigate(navigation_events::Event::Pinch {
                    delta: *delta,
                    modifiers: self.modifiers,
                });
                self.window.request_redraw();
            }
            WindowEvent::RotationGesture { delta, .. } => {
                // This is a native two-finger twist, not a pointer orbit. The
                // shared adapter ignores it in Planar navigation to retain 2D.
                self.navigate(navigation_events::Event::Rotate {
                    degrees: *delta,
                    modifiers: self.modifiers,
                });
                self.window.request_redraw();
            }
            WindowEvent::HoveredFile(_) => {
                self.state.hovered_file = true;
                self.window.request_redraw();
            }
            WindowEvent::HoveredFileCancelled => {
                self.state.hovered_file = false;
                self.window.request_redraw();
            }
            WindowEvent::DroppedFile(path) => {
                // Native documents replace the document; exchange assets join
                // the current scene, just like File > Import.
                self.load_into(path.clone(), !document_io::is_native_path(path));
            }
            WindowEvent::RedrawRequested => self.draw(event_loop),
            _ => {}
        }
    }
    fn draw(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.inner_size().width == 0 || self.window.inner_size().height == 0 {
            return;
        }
        let mut probe =
            match self
                .measurement
                .begin_frame(&mut self.state, &self.window, &self.config)
            {
                Ok(probe) => probe,
                Err(error) => {
                    eprintln!("Viewport measurement failed: {error}");
                    event_loop.exit();
                    return;
                }
            };
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
                        eprintln!("Failed to recreate window surface: {error}");
                        event_loop.exit();
                    }
                }
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                eprintln!("Window surface validation failed");
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
            if self.measurement.configured() {
                // The synthetic camera workload excludes physical input latency.
                // Keep real focus state, but prevent hover or typing from changing
                // its selection, overlays, and camera while samples are collected.
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
            if std::mem::take(&mut self.state.tool_dock.terminal_start_requested) {
                // Process creation is a native-host effect, outside repeated UI passes.
                // Failure is retained in the terminal status with a Retry control.
                let _ = self.state.tool_dock.terminal.start_shell(&ctx);
                self.window.request_redraw();
            }

            if self.applied_window_theme != self.state.theme_mode {
                // Explicit preferences also theme the native titlebar. Resetting to
                // None gives macOS ownership back; ThemeChanged then tracks the OS.
                let native_theme = match self.state.theme_mode {
                    ThemeMode::System => None,
                    ThemeMode::Light => Some(winit::window::Theme::Light),
                    ThemeMode::Dark => Some(winit::window::Theme::Dark),
                };
                self.window.set_theme(native_theme);
                self.applied_window_theme = self.state.theme_mode;
                if native_theme.is_none() {
                    self.state
                        .set_system_theme(resolved_window_theme(self.window.theme()));
                }
                self.window.request_redraw();
            }
            for command in self
                .state
                .take_ui_commands()
                .into_iter()
                .chain(shortcuts.commands())
            {
                match self.state.dispatch(command, &ctx, false) {
                    HostEffect::None
                    | HostEffect::CancelNavigation
                    | HostEffect::NavigationContextChanged => {}
                    HostEffect::Open => self.pending_open = true,
                    HostEffect::Import => self.pending_import = true,
                    HostEffect::Quit => self.pending_quit = true,
                }
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
            let title = self
                .state
                .save_path
                .as_deref()
                .map(filename)
                .unwrap_or_else(|| "Untitled".into());
            self.window.set_title(&format!(
                "{}{} — N3",
                title,
                if self.state.is_dirty() { " •" } else { "" }
            ));
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
        // Passive app cadence: one clock read per successful handoff when enabled.
        // This does not observe GPU completion or change redraw scheduling.
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
        self.service_settings();
        if std::mem::take(&mut self.state.request_save) {
            let save_as = std::mem::take(&mut self.state.request_save_as);
            self.save_document(save_as);
        }
        if std::mem::take(&mut self.state.request_new) && self.confirm_discard() {
            self.generation += 1;
            self.state.new_document();
            self.window.request_redraw();
        }
        if std::mem::take(&mut self.pending_import) {
            self.open_dialog(true);
        }
        if std::mem::take(&mut self.pending_open) {
            self.open_dialog(false);
        }
        if std::mem::take(&mut self.pending_quit)
            && self.confirm_discard()
            && self.flush_settings_before_quit()
        {
            event_loop.exit();
        }
        probe.end(Stage::HostTail);
        match self
            .measurement
            .finish_frame(probe, &self.state, self.window.has_focus())
        {
            Ok(true) => event_loop.exit(),
            Ok(false) => {
                if self.measurement.active() {
                    self.window.request_redraw();
                }
            }
            Err(error) => {
                eprintln!("Viewport measurement failed: {error}");
                event_loop.exit();
            }
        }
    }
}
struct App {
    workspace: Option<NativeWindow>,
    proxy: EventLoopProxy<AppEvent>,
    initial: Option<PathBuf>,
    access: crate::editor::EditorAccess,
}
impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.workspace.is_some() {
            return;
        }
        let measurement = match measurement::Host::from_environment() {
            Ok(host) => host,
            Err(error) => {
                eprintln!("Viewport measurement configuration failed: {error}");
                event_loop.exit();
                return;
            }
        };
        if measurement.configured() && self.initial.is_none() {
            eprintln!("Viewport measurement requires an input document path.");
            event_loop.exit();
            return;
        }
        let size = initial_window_size(
            event_loop
                .primary_monitor()
                .map(|monitor| monitor.size().to_logical(monitor.scale_factor())),
        );
        let attributes = Window::default_attributes()
            .with_title("N3")
            .with_inner_size(size)
            .with_min_inner_size(LogicalSize::new(
                MINIMUM_WINDOW_SIZE.width.min(size.width),
                MINIMUM_WINDOW_SIZE.height.min(size.height),
            ));
        let attributes = measurement.window_attributes(attributes);
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("create macOS window"),
        );
        match pollster::block_on(NativeWindow::new(window, self.proxy.clone(), measurement)) {
            Ok(mut workspace) => {
                workspace.state.editor.set_access(self.access);
                if let Some(path) = self.initial.take() {
                    workspace.load(path);
                }
                workspace.window.request_redraw();
                self.workspace = Some(workspace);
            }
            Err(error) => {
                eprintln!("Unable to initialize workspace: {error}");
                event_loop.exit();
            }
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(workspace) = &mut self.workspace {
            workspace.event(&event, event_loop);
        }
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        let Some(workspace) = &mut self.workspace else {
            return;
        };
        match event {
            AppEvent::Repaint => workspace.window.request_redraw(),
            AppEvent::Loaded {
                generation,
                path,
                result,
                append,
            } if generation == workspace.generation => {
                workspace.state.loading = None;
                match result {
                    Ok(asset) => {
                        if workspace.load_document != workspace.state.editor.document
                            || workspace.state.editor.has_transform_session()
                        {
                            workspace.state.error=Some("The document changed while opening. Open the file again to replace it.".into());
                        } else {
                            let result = workspace.install_opened_asset(path, asset, append);
                            if let Err(error) = result {
                                workspace.state.error = Some(error);
                            }
                        }
                    }
                    Err(error) => {
                        eprintln!("Open failed: {error}");
                        workspace.state.error = Some(error);
                    }
                }
                if workspace.measurement.configured()
                    && let Some(error) = &workspace.state.error
                {
                    eprintln!("Viewport measurement input failed: {error}");
                    event_loop.exit();
                    return;
                }
                workspace.window.request_redraw();
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(workspace) = &mut self.workspace {
            // Polling the small settings file wakes the host, not the GPU.
            // Only changed preferences or error state request a redraw.
            workspace.service_settings();
            if workspace
                .next_repaint
                .is_some_and(|deadline| deadline <= Instant::now())
            {
                workspace.next_repaint = None;
                workspace.window.request_redraw();
            }
            let deadline = workspace
                .next_repaint
                .into_iter()
                .chain(workspace.settings.as_ref().map(SettingsHost::deadline))
                .min();
            if let Some(deadline) = deadline {
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
                return;
            }
        }
        event_loop.set_control_flow(ControlFlow::Wait);
    }
}
pub fn run(
    initial: Option<PathBuf>,
    access: crate::editor::EditorAccess,
) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::<AppEvent>::with_user_event().build()?;
    let mut app = App {
        workspace: None,
        proxy: event_loop.create_proxy(),
        initial,
        access,
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(test)]
mod window_size_tests {
    use super::*;

    #[test]
    fn larger_displays_and_missing_monitor_keep_the_preferred_launch_size() {
        assert_eq!(initial_window_size(None), INITIAL_WINDOW_SIZE);
        assert_eq!(
            initial_window_size(Some(LogicalSize::new(1920.0, 1200.0))),
            INITIAL_WINDOW_SIZE
        );
    }

    #[test]
    fn launch_size_leaves_margins_in_logical_points_on_retina_and_small_displays() {
        let retina = winit::dpi::PhysicalSize::new(2880, 1800).to_logical(2.0);
        assert_eq!(
            initial_window_size(Some(retina)),
            LogicalSize::new(1296.0, 810.0)
        );
        assert_eq!(
            initial_window_size(Some(LogicalSize::new(800.0, 600.0))),
            LogicalSize::new(720.0, 540.0)
        );
    }
}
