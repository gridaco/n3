//! Browser storage notifications belong to the host lifecycle, not the UI.
use super::{AppEvent, SETTINGS_KEY};
use wasm_bindgen::{JsCast, prelude::*};
use winit::event_loop::EventLoopProxy;

pub(super) struct SettingsEvents {
    window: web_sys::Window,
    callback: Closure<dyn FnMut(web_sys::StorageEvent)>,
}

impl SettingsEvents {
    pub(super) fn install(proxy: EventLoopProxy<AppEvent>) -> Result<Self, JsValue> {
        let window =
            web_sys::window().ok_or_else(|| JsValue::from_str("Browser window unavailable"))?;
        let target = window.clone();
        let callback = Closure::new(move |event: web_sys::StorageEvent| {
            // A null key means localStorage.clear(). Ignore sessionStorage and
            // unrelated host application keys when N3 is embedded in a page.
            if event.key().is_some_and(|key| key != SETTINGS_KEY) {
                return;
            }
            if let Ok(Some(storage)) = target.local_storage()
                && event.storage_area().is_some_and(|area| area == storage)
            {
                let _ = proxy.send_event(AppEvent::SettingsChanged);
            }
        });
        window.add_event_listener_with_callback("storage", callback.as_ref().unchecked_ref())?;
        Ok(Self { window, callback })
    }
}

impl Drop for SettingsEvents {
    fn drop(&mut self) {
        let _ = self
            .window
            .remove_event_listener_with_callback("storage", self.callback.as_ref().unchecked_ref());
    }
}
