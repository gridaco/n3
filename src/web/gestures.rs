//! Canvas-scoped DOM input port. Browser zoom signals are not keyboard input:
//! winit's wheel conversion loses that distinction and has no WebKit gestures.
//! Capture before winit, route camera input once, and preserve egui UI scrolling.
use super::WebWindow;
use crate::{input::browser_navigation, navigation_events};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{HtmlCanvasElement, WheelEvent};

struct Listener {
    canvas: HtmlCanvasElement,
    name: &'static str,
    callback: Closure<dyn FnMut(web_sys::Event)>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self.canvas.remove_event_listener_with_callback_and_bool(
            self.name,
            self.callback.as_ref().unchecked_ref(),
            true,
        );
    }
}

pub(super) struct CanvasGestures {
    _listeners: Vec<Listener>,
}

impl CanvasGestures {
    pub(super) fn install(
        canvas: &HtmlCanvasElement,
        shared: &Rc<RefCell<Option<WebWindow>>>,
    ) -> Result<Self, JsValue> {
        let options = web_sys::AddEventListenerOptions::new();
        options.set_capture(true);
        options.set_passive(false);
        let mut listeners = Vec::new();
        for name in ["wheel", "gesturestart", "gesturechange", "gestureend"] {
            let weak = Rc::downgrade(shared);
            let target = canvas.clone();
            let callback = Closure::new(move |event: web_sys::Event| {
                // Prevent page magnification and winit's duplicate camera/UI
                // delivery, including when a popup or edit owns the canvas.
                // This listener never observes input outside this canvas.
                event.prevent_default();
                event.stop_immediate_propagation();
                let Some(shared) = weak.upgrade() else {
                    return;
                };
                let Ok(mut borrowed) = shared.try_borrow_mut() else {
                    return;
                };
                let Some(workspace) = borrowed.as_mut() else {
                    return;
                };
                if name == "wheel" {
                    if let Some(wheel) = event.dyn_ref::<WheelEvent>() {
                        workspace.browser_wheel(&target, wheel);
                    }
                } else {
                    workspace.browser_gesture(&target, name, &event);
                }
            });
            canvas.add_event_listener_with_callback_and_add_event_listener_options(
                name,
                callback.as_ref().unchecked_ref(),
                &options,
            )?;
            listeners.push(Listener {
                canvas: canvas.clone(),
                name,
                callback,
            });
        }
        Ok(Self {
            _listeners: listeners,
        })
    }
}

fn number(event: &web_sys::Event, key: &str) -> Option<f64> {
    js_sys::Reflect::get(event, &JsValue::from_str(key))
        .ok()?
        .as_f64()
        .filter(|value| value.is_finite())
}

impl WebWindow {
    fn browser_position(&self, canvas: &HtmlCanvasElement, x: f64, y: f64) -> Option<egui::Pos2> {
        let bounds = canvas.get_bounding_client_rect();
        let ppp = egui_winit::pixels_per_point(&self.context, &self.window) as f64;
        let point = egui::pos2(
            ((x - bounds.left()) * f64::from(canvas.width()) / bounds.width() / ppp) as f32,
            ((y - bounds.top()) * f64::from(canvas.height()) / bounds.height() / ppp) as f32,
        );
        point.is_finite().then_some(point)
    }

    fn browser_navigation_owned(&self) -> bool {
        !self.state.editor.blocks_navigation()
            && navigation_events::accepts(
                &self.state,
                &self.context,
                self.cursor,
                self.window.has_focus(),
                &self.input.egui_input().events,
            )
    }

    fn browser_wheel(&mut self, canvas: &HtmlCanvasElement, wheel: &WheelEvent) {
        use browser_navigation::{Wheel, WheelUnit};
        // Some engines can expose both gesture families. WebKit's active
        // sequence owns zoom until its end/cancel; do not apply ctrl-wheel twice.
        if wheel.ctrl_key() && self.webkit_gesture.is_active() {
            return;
        }
        self.cursor =
            self.browser_position(canvas, wheel.client_x().into(), wheel.client_y().into());
        let unit = match wheel.delta_mode() {
            WheelEvent::DOM_DELTA_PIXEL => WheelUnit::Pixels,
            WheelEvent::DOM_DELTA_LINE => WheelUnit::Lines,
            WheelEvent::DOM_DELTA_PAGE => WheelUnit::Pages,
            _ => return,
        };
        let bounds = canvas.get_bounding_client_rect();
        // ctrlKey is synthesized for pinch. Only the keyboard's actual Control
        // state may become egui's command modifier or affect scroll ownership.
        let mac = self.context.os() == egui::os::OperatingSystem::Mac;
        let modifiers = egui::Modifiers {
            alt: wheel.alt_key(),
            shift: wheel.shift_key(),
            ctrl: self.modifiers.ctrl,
            mac_cmd: mac && wheel.meta_key(),
            command: if mac {
                wheel.meta_key()
            } else {
                self.modifiers.ctrl
            },
        };
        let Some(event) = browser_navigation::wheel(Wheel {
            delta: egui::vec2(wheel.delta_x() as f32, wheel.delta_y() as f32),
            unit,
            ctrl: wheel.ctrl_key(),
            modifiers,
            points_per_css_pixel: canvas.height() as f32
                / bounds.height() as f32
                / egui_winit::pixels_per_point(&self.context, &self.window),
            page_height_css: bounds.height() as f32,
        }) else {
            return;
        };
        if let Some(pointer) = self.cursor {
            self.input
                .egui_input_mut()
                .events
                .push(egui::Event::PointerMoved(pointer));
        }
        self.navigate(event);
        if !wheel.ctrl_key() {
            // Panels still receive ordinary scrolling through the same event
            // conversion used by executable replay. Zoom belongs to the camera,
            // never egui's global UI scale or an unrelated focused numeric field.
            if let Some(event) = event.egui_event() {
                self.input.egui_input_mut().events.push(event);
            }
        }
        self.window.request_redraw();
    }

    fn browser_gesture(&mut self, canvas: &HtmlCanvasElement, name: &str, event: &web_sys::Event) {
        // WebKit's nonstandard event does not have web-sys bindings. Require
        // finite cumulative values; use its location when present, otherwise
        // the last real pointer position. Never invent a viewport center.
        if let (Some(x), Some(y)) = (number(event, "clientX"), number(event, "clientY")) {
            self.cursor = self.browser_position(canvas, x, y);
        }
        let values = number(event, "scale").zip(number(event, "rotation"));
        if name == "gesturestart" {
            self.webkit_gesture.reset();
            self.webkit_gesture_owned = self.browser_navigation_owned();
            if let Some((scale, rotation)) = values {
                self.webkit_gesture.begin(scale, rotation as f32);
            }
        } else {
            // Ownership cannot jump from UI to viewport midway through a pinch.
            self.webkit_gesture_owned &= self.browser_navigation_owned();
            if let Some((scale, rotation)) = values
                && let Some(events) =
                    self.webkit_gesture
                        .update(scale, rotation as f32, self.modifiers)
                && self.webkit_gesture_owned
            {
                for event in events {
                    self.navigate(event);
                }
                self.window.request_redraw();
            }
            if name == "gestureend" {
                self.webkit_gesture.reset();
                self.webkit_gesture_owned = false;
            }
        }
    }
}
