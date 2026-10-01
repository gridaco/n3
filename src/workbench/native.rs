//! Developer-only egui window. No document, scene, or persistent settings host.
use super::Workbench;
use std::{
    error::Error,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

enum Event {
    Repaint(Instant),
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
    renderer: egui_wgpu::Renderer,
    workbench: Workbench,
    next_repaint: Option<Instant>,
}

impl NativeWindow {
    async fn new(window: Arc<Window>, proxy: EventLoopProxy<Event>) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = instance
            .create_surface(window.clone())
            .map_err(|error| error.to_string())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .map_err(|error| error.to_string())?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("n3 component workbench"),
                ..Default::default()
            })
            .await
            .map_err(|error| error.to_string())?;
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .unwrap_or(capabilities.formats[0]);
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);
        let context = egui::Context::default();
        crate::ui::workspace_ui::configure_context(&context);
        context.set_request_repaint_callback(move |info| {
            // Delayed requests matter for hover tooltips and animation endings.
            // Wake the event loop now, then wait until the requested deadline.
            if let Some(deadline) = repaint_deadline(Instant::now(), info.delay) {
                let _ = proxy.send_event(Event::Repaint(deadline));
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
        let renderer =
            egui_wgpu::Renderer::new(&device, format, egui_wgpu::RendererOptions::default());
        Ok(Self {
            instance,
            window,
            surface,
            device,
            queue,
            config,
            context,
            input,
            renderer,
            workbench: Workbench::default(),
            next_repaint: None,
        })
    }

    fn resize(&mut self) {
        let size = self.window.inner_size();
        if size.width > 0 && size.height > 0 {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.device, &self.config);
            self.window.request_redraw();
        }
    }

    fn schedule_repaint(&mut self, deadline: Instant) {
        self.next_repaint = Some(
            self.next_repaint
                .map_or(deadline, |previous| previous.min(deadline)),
        );
    }

    fn draw(&mut self) -> Result<(), String> {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        let (frame, reconfigure) = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            wgpu::CurrentSurfaceTexture::Timeout => {
                self.window.request_redraw();
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Occluded => return Ok(()),
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.resize();
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = self
                    .instance
                    .create_surface(self.window.clone())
                    .map_err(|error| error.to_string())?;
                self.resize();
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("Component workbench surface validation failed".into());
            }
        };
        self.next_repaint = None;
        let input = self.input.take_egui_input(&self.window);
        let context = self.context.clone();
        let mut output = context.run_ui(input, |ui| self.workbench.ui(ui));
        self.input
            .handle_platform_output(&self.window, output.platform_output);
        let pixels_per_point = output.pixels_per_point;
        let jobs = context.tessellate(output.shapes, pixels_per_point);
        for (id, deltas) in std::mem::take(&mut output.textures_delta.set) {
            for delta in deltas {
                self.renderer
                    .update_texture(&self.device, &self.queue, id, &delta);
            }
        }
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("component workbench frame"),
            });
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point,
        };
        let buffers =
            self.renderer
                .update_buffers(&self.device, &self.queue, &mut encoder, &jobs, &screen);
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("component workbench UI"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            self.renderer
                .render(&mut pass.forget_lifetime(), &jobs, &screen);
        }
        self.queue
            .submit(buffers.into_iter().chain(std::iter::once(encoder.finish())));
        self.window.pre_present_notify();
        self.queue.present(frame);
        for id in std::mem::take(&mut output.textures_delta.free) {
            self.renderer.free_texture(&id);
        }
        if reconfigure {
            self.resize();
        }
        let delay = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(Duration::MAX, |output| output.repaint_delay);
        if let Some(deadline) = repaint_deadline(Instant::now(), delay) {
            self.schedule_repaint(deadline);
        }
        Ok(())
    }
}

fn repaint_deadline(now: Instant, delay: Duration) -> Option<Instant> {
    (delay != Duration::MAX)
        .then(|| now.checked_add(delay))
        .flatten()
}

struct App {
    native: Option<NativeWindow>,
    proxy: EventLoopProxy<Event>,
    error: Option<String>,
}

impl ApplicationHandler<Event> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.native.is_some() {
            return;
        }
        let result = (|| {
            let window = Arc::new(
                event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_title("N3 UI component workbench")
                            .with_inner_size(LogicalSize::new(1160.0, 800.0))
                            .with_min_inner_size(LogicalSize::new(600.0, 400.0)),
                    )
                    .map_err(|error| error.to_string())?,
            );
            pollster::block_on(NativeWindow::new(window, self.proxy.clone()))
        })();
        match result {
            Ok(native) => {
                native.window.request_redraw();
                self.native = Some(native);
            }
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(native) = &mut self.native else {
            return;
        };
        if native.window.id() != id {
            return;
        }
        if native.input.on_window_event(&native.window, &event).repaint {
            native.window.request_redraw();
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) => {
                // Layout updates component bounds on the next pass. A held pie
                // must not release against the previous window dimensions.
                native
                    .workbench
                    .clear_input(crate::ui::timeline::CancelReason::Resize);
                native.resize();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                native.context.stop_dragging();
                native
                    .workbench
                    .clear_input(crate::ui::timeline::CancelReason::Resize);
                native.resize();
            }
            WindowEvent::Focused(false) => {
                native.context.stop_dragging();
                native
                    .workbench
                    .clear_input(crate::ui::timeline::CancelReason::FocusLost);
                native.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = native.draw() {
                    self.error = Some(error);
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: Event) {
        if let Some(native) = &mut self.native {
            match event {
                Event::Repaint(deadline) => native.schedule_repaint(deadline),
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(native) = &mut self.native {
            if native
                .next_repaint
                .is_some_and(|deadline| deadline <= Instant::now())
            {
                native.next_repaint = None;
                native.window.request_redraw();
            }
            if let Some(deadline) = native.next_repaint {
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
                return;
            }
        }
        event_loop.set_control_flow(ControlFlow::Wait);
    }
}

pub(super) fn run() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::<Event>::with_user_event().build()?;
    let mut app = App {
        native: None,
        proxy: event_loop.create_proxy(),
        error: None,
    };
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.error {
        return Err(std::io::Error::other(error).into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delayed_repaint_preserves_deadline_and_infinite_delay_does_not_poll() {
        let now = Instant::now();
        let delay = Duration::from_millis(350);
        assert_eq!(repaint_deadline(now, delay), Some(now + delay));
        assert_eq!(repaint_deadline(now, Duration::ZERO), Some(now));
        assert_eq!(repaint_deadline(now, Duration::MAX), None);
    }
}
