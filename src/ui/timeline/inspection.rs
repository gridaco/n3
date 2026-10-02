//! Inspection and isolated fixtures for executable native tooling.
use super::*;

impl Timeline {
    pub(crate) fn visible_range(&self) -> Option<TimeRange> {
        self.visible
    }

    pub(crate) fn selection(&self) -> Option<&Selection> {
        self.selected.as_ref()
    }

    pub(crate) fn is_scrubbing(&self) -> bool {
        self.scrub.is_some()
    }

    /// Read only selected identities through the current revision's indexes.
    /// Hosts can present metadata without searching/cloning the whole snapshot.
    pub(crate) fn inspected_keys<'a>(&'a self, data: &'a Data) -> impl Iterator<Item = &'a Key> {
        self.selected.iter().flat_map(move |selection| {
            selection.keys.iter().filter_map(move |key| {
                let prepared = self.prepared.as_ref()?;
                if self.source != Some((std::ptr::from_ref(data) as usize, data.revision)) {
                    return None;
                }
                let track = prepared.track_index(key.track)?;
                data.tracks
                    .get(track)?
                    .keys
                    .get(prepared.key_index(key.track, key.key)?)
            })
        })
    }
}
