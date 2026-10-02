//! Browser preference scheduling. Storage events and local edits replace polling;
//! only a failed attempt owns a timer. The host supplies elapsed monotonic time.
use crate::settings::{Settings, SettingsController, SettingsStore};
use std::time::Duration;

pub(super) struct BrowserSettings<S: SettingsStore> {
    controller: SettingsController<S>,
    attempted: Settings,
    requested: bool,
    retry_at: Option<Duration>,
    failures: u32,
}

impl<S: SettingsStore> BrowserSettings<S> {
    pub(super) fn new(store: S, defaults: Settings) -> Result<Self, String> {
        Ok(Self {
            controller: SettingsController::new(store, defaults.clone())?,
            attempted: defaults,
            requested: true,
            retry_at: None,
            failures: 0,
        })
    }

    pub(super) fn request_sync(&mut self) {
        self.requested = true;
        self.retry_at = None;
        self.failures = 0;
    }

    pub(super) fn deadline(&self) -> Option<Duration> {
        self.retry_at
    }

    pub(super) fn sync(
        &mut self,
        local: &Settings,
        now: Duration,
        ready: bool,
        reload: bool,
    ) -> Option<Result<Settings, String>> {
        if !ready {
            // The next input/frame boundary will service this request. Leaving
            // an expired deadline here would spin while a drag or popup owns UI.
            if self.retry_at.is_some_and(|deadline| deadline <= now) {
                self.retry_at = None;
                self.requested = true;
            }
            return None;
        }
        let changed = local != &self.attempted;
        if !reload
            && !self.requested
            && !changed
            && !self.retry_at.is_some_and(|deadline| deadline <= now)
        {
            return None;
        }
        if changed || reload {
            self.failures = 0;
        }
        let result = if reload {
            self.controller.reload()
        } else {
            self.controller.sync(local)
        };
        Some(self.finish(local, now, result))
    }

    pub(super) fn ensure_file(
        &mut self,
        local: &Settings,
        now: Duration,
    ) -> Result<Settings, String> {
        let result = self.controller.ensure_file(local);
        self.finish(local, now, result)
    }

    fn finish(
        &mut self,
        local: &Settings,
        now: Duration,
        result: Result<Settings, String>,
    ) -> Result<Settings, String> {
        self.requested = false;
        self.attempted = result.as_ref().unwrap_or(local).clone();
        if result.is_ok() {
            self.retry_at = None;
            self.failures = 0;
        } else {
            let seconds = (1_u64 << self.failures.min(5)).min(30);
            self.retry_at = Some(now + Duration::from_secs(seconds));
            self.failures = self.failures.saturating_add(1);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    #[derive(Default)]
    struct Stored {
        bytes: Option<Vec<u8>>,
        reads: usize,
        writes: usize,
        fail: bool,
    }

    #[derive(Clone, Default)]
    struct Store(Rc<RefCell<Stored>>);

    impl SettingsStore for Store {
        fn read(&mut self) -> Result<Option<Vec<u8>>, String> {
            let mut stored = self.0.borrow_mut();
            stored.reads += 1;
            if stored.fail {
                return Err("Storage unavailable".into());
            }
            Ok(stored.bytes.clone())
        }

        fn compare_and_swap(
            &mut self,
            expected: Option<&[u8]>,
            replacement: &[u8],
        ) -> Result<(), String> {
            let mut stored = self.0.borrow_mut();
            if stored.fail || stored.bytes.as_deref() != expected {
                return Err("Storage changed".into());
            }
            stored.writes += 1;
            stored.bytes = Some(replacement.to_vec());
            Ok(())
        }
    }

    #[test]
    fn idle_frames_do_no_io_and_local_edits_wait_for_safe_boundary() {
        let store = Store::default();
        let mut local = Settings::default();
        let mut host = BrowserSettings::new(store.clone(), local.clone()).unwrap();
        local = host
            .sync(&local, Duration::ZERO, true, false)
            .unwrap()
            .unwrap();
        assert_eq!(store.0.borrow().reads, 1);
        for frame in 1..600 {
            assert!(
                host.sync(&local, Duration::from_millis(frame * 16), true, false)
                    .is_none()
            );
        }
        assert_eq!(store.0.borrow().reads, 1);
        assert_eq!(host.deadline(), None);
        local.show_grid = false;
        assert!(
            host.sync(&local, Duration::from_secs(10), false, false)
                .is_none()
        );
        assert_eq!(store.0.borrow().writes, 0);
        local = host
            .sync(&local, Duration::from_secs(10), true, false)
            .unwrap()
            .unwrap();
        assert!(!local.show_grid);
        assert_eq!(store.0.borrow().writes, 1);
        assert!(
            host.sync(&local, Duration::from_secs(11), true, false)
                .is_none()
        );
        assert_eq!(store.0.borrow().writes, 1);
    }

    #[test]
    fn external_event_merges_pending_changes_and_reload_is_explicit() {
        let store = Store::default();
        let mut local = Settings::default();
        let mut host = BrowserSettings::new(store.clone(), local.clone()).unwrap();
        local = host
            .sync(&local, Duration::ZERO, true, false)
            .unwrap()
            .unwrap();
        store.0.borrow_mut().bytes = Some(br#"{"viewport.showEdges":false}"#.to_vec());
        local.show_grid = false;
        host.request_sync();
        assert!(host.sync(&local, Duration::ZERO, false, false).is_none());
        local = host
            .sync(&local, Duration::ZERO, true, false)
            .unwrap()
            .unwrap();
        assert!(!local.show_edges && !local.show_grid);
        local.show_grid = true;
        local = host
            .sync(&local, Duration::ZERO, true, true)
            .unwrap()
            .unwrap();
        assert!(
            !local.show_grid,
            "reload discards unsaved local preference edits"
        );
        assert_eq!(host.deadline(), None);
    }

    #[test]
    fn failures_back_off_and_expired_retry_does_not_spin_during_interaction() {
        let store = Store::default();
        store.0.borrow_mut().fail = true;
        let local = Settings::default();
        let mut host = BrowserSettings::new(store.clone(), local.clone()).unwrap();
        assert!(
            host.sync(&local, Duration::ZERO, true, false)
                .unwrap()
                .is_err()
        );
        assert_eq!(host.deadline(), Some(Duration::from_secs(1)));
        for ms in 0..1000 {
            assert!(
                host.sync(&local, Duration::from_millis(ms), true, false)
                    .is_none()
            );
        }
        assert_eq!(store.0.borrow().reads, 1);
        assert!(
            host.sync(&local, Duration::from_secs(1), false, false)
                .is_none()
        );
        assert_eq!(host.deadline(), None);
        assert!(
            host.sync(&local, Duration::from_secs(2), true, false)
                .unwrap()
                .is_err()
        );
        assert_eq!(host.deadline(), Some(Duration::from_secs(4)));
        let mut now = Duration::from_secs(4);
        for _ in 0..8 {
            assert!(host.sync(&local, now, true, false).unwrap().is_err());
            let next = host.deadline().unwrap();
            assert!(next > now && next - now <= Duration::from_secs(30));
            now = next;
        }
        store.0.borrow_mut().fail = false;
        host.request_sync();
        assert!(host.sync(&local, now, true, false).unwrap().is_ok());
        assert_eq!(host.deadline(), None);
    }

    #[test]
    fn exporting_materializes_once_and_keeps_idle_service_clean() {
        let store = Store::default();
        let local = Settings::default();
        let mut host = BrowserSettings::new(store.clone(), local.clone()).unwrap();
        assert_eq!(host.ensure_file(&local, Duration::ZERO).unwrap(), local);
        assert_eq!(store.0.borrow().writes, 1);
        assert!(
            host.sync(&local, Duration::from_secs(100), true, false)
                .is_none()
        );
        assert_eq!(host.deadline(), None);
    }
}
