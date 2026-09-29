//! Native polling and retry cadence around the platform-independent controller.
//! An unchanged redraw does no filesystem work. Unsafe interaction boundaries
//! postpone polling without creating a busy event loop; local changes remain
//! pending until the UI says they can be synchronized.

use crate::settings::{Settings, SettingsController, SettingsStore};
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(500);

pub(super) struct SettingsHost<S: SettingsStore> {
    controller: SettingsController<S>,
    applied: Settings,
    attempted: Option<Settings>,
    next_poll: Instant,
}

impl<S: SettingsStore> SettingsHost<S> {
    pub fn new(store: S, defaults: Settings, now: Instant) -> Result<Self, String> {
        Ok(Self {
            controller: SettingsController::new(store, defaults.clone())?,
            applied: defaults,
            attempted: None,
            next_poll: now,
        })
    }

    pub fn deadline(&self) -> Instant {
        self.next_poll
    }

    pub fn request_poll(&mut self, now: Instant) {
        self.next_poll = now;
    }

    pub fn defer_poll(&mut self, now: Instant) {
        if self.next_poll <= now {
            self.next_poll = now + POLL_INTERVAL;
        }
    }

    pub fn has_pending(&self, local: &Settings) -> bool {
        local != &self.applied
    }

    pub fn sync(
        &mut self,
        local: &Settings,
        now: Instant,
        force: bool,
    ) -> Option<Result<Settings, String>> {
        if !force && self.attempted.as_ref() == Some(local) && now < self.next_poll {
            return None;
        }
        let result = self.controller.sync(local);
        Some(self.finish_attempt(local, now, result))
    }

    pub fn reload(&mut self, local: &Settings, now: Instant) -> Result<Settings, String> {
        let result = self.controller.reload();
        self.finish_attempt(local, now, result)
    }

    pub fn ensure_file(&mut self, local: &Settings, now: Instant) -> Result<Settings, String> {
        let result = self.controller.ensure_file(local);
        self.finish_attempt(local, now, result)
    }

    fn finish_attempt(
        &mut self,
        local: &Settings,
        now: Instant,
        result: Result<Settings, String>,
    ) -> Result<Settings, String> {
        self.next_poll = now + POLL_INTERVAL;
        self.attempted = Some(match &result {
            Ok(settings) => {
                self.applied = settings.clone();
                settings.clone()
            }
            Err(_) => local.clone(),
        });
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    #[derive(Default)]
    struct StoreState {
        bytes: Option<Vec<u8>>,
        reads: usize,
        writes: usize,
        fail_write: bool,
    }

    #[derive(Clone, Default)]
    struct Store(Rc<RefCell<StoreState>>);
    impl SettingsStore for Store {
        fn read(&mut self) -> Result<Option<Vec<u8>>, String> {
            let mut state = self.0.borrow_mut();
            state.reads += 1;
            Ok(state.bytes.clone())
        }

        fn compare_and_swap(
            &mut self,
            expected: Option<&[u8]>,
            replacement: &[u8],
        ) -> Result<(), String> {
            let mut state = self.0.borrow_mut();
            state.writes += 1;
            if state.fail_write || state.bytes.as_deref() != expected {
                return Err("The in-memory store rejected the write".into());
            }
            state.bytes = Some(replacement.to_vec());
            Ok(())
        }
    }

    #[test]
    fn idle_redraws_do_no_io_and_missing_startup_does_not_create_a_file() {
        let store = Store::default();
        let now = Instant::now();
        let defaults = Settings::default();
        let mut host = SettingsHost::new(store.clone(), defaults.clone(), now).unwrap();
        assert_eq!(store.0.borrow().reads, 0);
        assert_eq!(host.sync(&defaults, now, false).unwrap().unwrap(), defaults);
        assert_eq!(store.0.borrow().reads, 1);
        assert_eq!(store.0.borrow().writes, 0);
        assert!(store.0.borrow().bytes.is_none());
        for ms in 1..500 {
            assert!(
                host.sync(&defaults, now + Duration::from_millis(ms), false)
                    .is_none()
            );
        }
        assert_eq!(store.0.borrow().reads, 1);
        store.0.borrow_mut().bytes = Some(br#"{"viewport.showGrid":false}"#.to_vec());
        let updated = host
            .sync(&defaults, now + POLL_INTERVAL, false)
            .unwrap()
            .unwrap();
        assert!(!updated.show_grid);
        assert!(!host.has_pending(&updated));
        assert!(host.sync(&updated, now + POLL_INTERVAL, false).is_none());
        assert_eq!(store.0.borrow().reads, 2);
        assert_eq!(store.0.borrow().writes, 0);
    }

    #[test]
    fn deferral_preserves_pending_edits_until_release_then_writes_once() {
        let store = Store::default();
        let now = Instant::now();
        let mut local = Settings::default();
        let mut host = SettingsHost::new(store.clone(), local.clone(), now).unwrap();
        host.sync(&local, now, false).unwrap().unwrap();
        local.show_grid = false;
        for ms in 500..550 {
            host.defer_poll(now + Duration::from_millis(ms));
        }
        assert_eq!(store.0.borrow().reads, 1);
        assert_eq!(store.0.borrow().writes, 0);
        assert!(host.has_pending(&local));
        // A completed UI change synchronizes immediately, even before the next
        // external poll deadline established while the drag was held.
        let release = now + Duration::from_millis(550);
        assert_eq!(host.sync(&local, release, false).unwrap().unwrap(), local);
        assert!(!host.has_pending(&local));
        assert_eq!(store.0.borrow().writes, 1);
        assert!(host.sync(&local, release, false).is_none());
    }

    #[test]
    fn failed_edits_remain_pending_without_retrying_on_every_redraw() {
        let store = Store::default();
        let now = Instant::now();
        let mut local = Settings::default();
        let mut host = SettingsHost::new(store.clone(), local.clone(), now).unwrap();
        host.sync(&local, now, false).unwrap().unwrap();
        local.show_grid = false;
        store.0.borrow_mut().fail_write = true;
        assert!(host.sync(&local, now, false).unwrap().is_err());
        assert!(host.has_pending(&local));
        for ms in 1..500 {
            assert!(
                host.sync(&local, now + Duration::from_millis(ms), false)
                    .is_none()
            );
        }
        assert_eq!(store.0.borrow().writes, 1);
        store.0.borrow_mut().fail_write = false;
        assert_eq!(
            host.sync(&local, now + POLL_INTERVAL, false)
                .unwrap()
                .unwrap(),
            local
        );
        assert_eq!(store.0.borrow().writes, 2);
        assert!(!host.has_pending(&local));
    }

    #[test]
    fn focus_reload_and_open_are_explicit_without_confusing_external_errors_with_pending_edits() {
        let store = Store::default();
        let now = Instant::now();
        let mut local = Settings::default();
        let mut host = SettingsHost::new(store.clone(), local.clone(), now).unwrap();
        host.sync(&local, now, false).unwrap().unwrap();
        store.0.borrow_mut().bytes = Some(b"malformed".to_vec());
        host.request_poll(now + Duration::from_millis(20));
        assert!(
            host.sync(&local, now + Duration::from_millis(20), false)
                .unwrap()
                .is_err()
        );
        assert!(
            !host.has_pending(&local),
            "An invalid external file must not trap normal quitting"
        );
        local.show_grid = false;
        assert!(host.has_pending(&local));
        assert!(host.reload(&local, now).is_err());
        assert!(host.has_pending(&local));
        store.0.borrow_mut().bytes = None;
        local = host.reload(&local, now).unwrap();
        assert!(local.show_grid);
        assert!(!host.has_pending(&local));
        assert!(
            store.0.borrow().bytes.is_none(),
            "Reload never creates the missing file"
        );
        host.ensure_file(&local, now).unwrap();
        assert!(store.0.borrow().bytes.is_some());
        assert_eq!(store.0.borrow().writes, 1);
        host.ensure_file(&local, now).unwrap();
        assert_eq!(store.0.borrow().writes, 1);
    }
}
