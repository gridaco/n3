//! Caller-owned timeline presentation data and host requests.
//!
//! A revision identifies an immutable snapshot. Changing any track, key, or
//! content bound must bump it; the component caches preparation by snapshot.
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct TrackId(pub u64);

/// Key identities are unique within their track, rather than across all tracks.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct KeyId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TimeRange {
    pub start: f64,
    pub end: f64,
}

impl TimeRange {
    pub fn new(start: f64, end: f64) -> Result<Self, String> {
        if !start.is_finite() || !end.is_finite() {
            return Err("Timeline range endpoints must be finite.".into());
        }
        if end < start {
            return Err("Timeline range end must not precede its start.".into());
        }
        if !(end - start).is_finite() {
            return Err("Timeline range duration must be finite.".into());
        }
        Ok(Self { start, end })
    }

    pub fn duration(self) -> f64 {
        self.end - self.start
    }

    /// Fit with 5% breathing room. A point range gets at least one second of
    /// visible span; extreme finite coordinates remain finite and ordered.
    pub fn fit_visible(self) -> Self {
        let duration = self.duration();
        let padding = if duration == 0.0 {
            (self.start.abs() * 0.05).max(0.5)
        } else {
            (duration * 0.05).max(0.05)
        };
        let start = (self.start - padding).clamp(-f64::MAX, f64::MAX);
        let end = (self.end + padding).clamp(-f64::MAX, f64::MAX);
        // If an already enormous span cannot grow without overflowing its
        // duration, its unpadded positive span is the representable fit.
        Self::new(start, end).unwrap_or(self)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Data {
    pub revision: u64,
    pub content: TimeRange,
    pub tracks: Vec<Track>,
}

#[derive(Clone, Debug)]
pub(crate) struct Track {
    pub id: TrackId,
    pub parent: Option<TrackId>,
    pub label: String,
    pub keys: Vec<Key>,
}

#[derive(Clone, Debug)]
pub(crate) struct Key {
    pub id: KeyId,
    pub time: f64,
    pub metadata: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HostState {
    pub accepted_time: f64,
    pub playing: bool,
    pub looping: bool,
    pub speed: f64,
    pub available: bool,
}

impl Default for HostState {
    fn default() -> Self {
        Self {
            accepted_time: 0.0,
            playing: false,
            looping: false,
            speed: 1.0,
            available: true,
        }
    }
}

impl HostState {
    pub fn valid(self) -> bool {
        self.available
            && self.accepted_time.is_finite()
            && self.speed.is_finite()
            && self.speed > 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Capabilities {
    /// Hosts can withhold selection while another tool/session owns intent.
    /// Independent of seeking: a static timeline can still inspect keys.
    pub inspect: bool,
    pub seek: bool,
    pub play_pause: bool,
    pub looping: bool,
    pub speed: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            inspect: true,
            seek: true,
            play_pause: true,
            looping: true,
            speed: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CancelReason {
    Escape,
    FocusLost,
    Resize,
    DataChanged,
    Unavailable,
    PointerGone,
    InputOwner,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Request {
    Seek { time: f64 },
    ScrubBegin { gesture: u64, time: f64 },
    ScrubUpdate { gesture: u64, time: f64 },
    ScrubEnd { gesture: u64, time: f64 },
    ScrubCancel { gesture: u64, reason: CancelReason },
    SetPlaying(bool),
    SetLooping(bool),
    SetSpeed(f64),
}

struct Keys {
    indices: Vec<usize>,
    times: Vec<f64>,
    by_id: HashMap<KeyId, usize>,
}

/// Revision preparation owns hierarchy validation and stable identity lookup.
/// Painting can then visit visible rows and binary-search their visible keys.
pub(crate) struct Prepared {
    pub revision: u64,
    pub total_keys: usize,
    roots: Vec<usize>,
    children: Vec<Vec<usize>>,
    track_ids: Vec<TrackId>,
    tracks_by_id: HashMap<TrackId, usize>,
    keys: Vec<Keys>,
}

impl Prepared {
    pub fn new(data: &Data) -> Result<Self, String> {
        TimeRange::new(data.content.start, data.content.end)?;
        let mut tracks_by_id = HashMap::with_capacity(data.tracks.len());
        for (index, track) in data.tracks.iter().enumerate() {
            if tracks_by_id.insert(track.id, index).is_some() {
                return Err(format!(
                    "Duplicate timeline track identity: {}.",
                    track.id.0
                ));
            }
        }
        let mut roots = Vec::new();
        let mut children = vec![Vec::new(); data.tracks.len()];
        let mut parents = Vec::with_capacity(data.tracks.len());
        let mut keys = Vec::with_capacity(data.tracks.len());
        let mut total_keys = 0;
        for (index, track) in data.tracks.iter().enumerate() {
            let parent = track
                .parent
                .map(|id| {
                    tracks_by_id.get(&id).copied().ok_or_else(|| {
                        format!("Timeline track {} has missing parent {}.", track.id.0, id.0)
                    })
                })
                .transpose()?;
            if let Some(parent) = parent {
                children[parent].push(index);
            } else {
                roots.push(index);
            }
            parents.push(parent);
            let mut by_id = HashMap::with_capacity(track.keys.len());
            for (key_index, key) in track.keys.iter().enumerate() {
                if !key.time.is_finite() {
                    return Err(format!(
                        "Timeline key {} on track {} has a nonfinite time.",
                        key.id.0, track.id.0
                    ));
                }
                if by_id.insert(key.id, key_index).is_some() {
                    return Err(format!(
                        "Duplicate timeline key identity {} on track {}.",
                        key.id.0, track.id.0
                    ));
                }
            }
            let mut indices: Vec<_> = (0..track.keys.len()).collect();
            indices.sort_by(|&a, &b| {
                track.keys[a]
                    .time
                    .total_cmp(&track.keys[b].time)
                    .then_with(|| track.keys[a].id.cmp(&track.keys[b].id))
            });
            let times = indices.iter().map(|&key| track.keys[key].time).collect();
            total_keys += indices.len();
            keys.push(Keys {
                indices,
                times,
                by_id,
            });
        }
        // Walk parent chains iteratively so valid deep trees cannot overflow
        // the call stack. Each chain becomes complete before the next starts.
        let mut visits = vec![0_u8; data.tracks.len()];
        for index in 0..data.tracks.len() {
            let mut chain = Vec::new();
            let mut cursor = Some(index);
            while let Some(current) = cursor {
                match visits[current] {
                    1 => return Err("Timeline track hierarchy contains a cycle.".into()),
                    2 => break,
                    _ => {
                        visits[current] = 1;
                        chain.push(current);
                        cursor = parents[current];
                    }
                }
            }
            for current in chain {
                visits[current] = 2;
            }
        }
        Ok(Self {
            revision: data.revision,
            total_keys,
            roots,
            children,
            track_ids: data.tracks.iter().map(|track| track.id).collect(),
            tracks_by_id,
            keys,
        })
    }

    pub fn track_index(&self, track: TrackId) -> Option<usize> {
        self.tracks_by_id.get(&track).copied()
    }

    pub fn has_children(&self, track_index: usize) -> bool {
        self.children
            .get(track_index)
            .is_some_and(|children| !children.is_empty())
    }

    pub fn key_index(&self, track: TrackId, key: KeyId) -> Option<usize> {
        self.keys
            .get(self.track_index(track)?)?
            .by_id
            .get(&key)
            .copied()
    }

    /// Rebuild only when the snapshot or collapsed set changes. Stable input
    /// order determines sibling order, including forward parent references.
    pub fn visible_rows(&self, data: &Data, collapsed: &BTreeSet<TrackId>) -> Vec<(usize, usize)> {
        debug_assert_eq!(self.revision, data.revision);
        let mut rows = Vec::new();
        let mut pending: Vec<_> = self.roots.iter().rev().map(|&index| (index, 0)).collect();
        while let Some((index, depth)) = pending.pop() {
            rows.push((index, depth));
            if !collapsed.contains(&self.track_ids[index]) {
                pending.extend(
                    self.children[index]
                        .iter()
                        .rev()
                        .map(|&child| (child, depth + 1)),
                );
            }
        }
        rows
    }

    pub fn key_indices(&self, track_index: usize) -> &[usize] {
        self.keys
            .get(track_index)
            .map_or(&[], |keys| keys.indices.as_slice())
    }

    pub fn key_range(&self, track_index: usize, range: TimeRange) -> std::ops::Range<usize> {
        let Some(keys) = self.keys.get(track_index) else {
            return 0..0;
        };
        let start = keys.times.partition_point(|&time| time < range.start);
        let end = keys.times.partition_point(|&time| time <= range.end);
        start..end.max(start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: u64, parent: Option<u64>, times: &[f64]) -> Track {
        Track {
            id: TrackId(id),
            parent: parent.map(TrackId),
            label: format!("Track {id}"),
            keys: times
                .iter()
                .enumerate()
                .map(|(id, &time)| Key {
                    id: KeyId(id as u64),
                    time,
                    metadata: None,
                })
                .collect(),
        }
    }

    fn data(tracks: Vec<Track>) -> Data {
        Data {
            revision: 8,
            content: TimeRange::new(-10.0, 10.0).unwrap(),
            tracks,
        }
    }

    #[test]
    fn validates_ranges_and_fits_zero_duration_at_extreme_coordinates() {
        assert!(TimeRange::new(f64::NAN, 1.0).is_err());
        assert!(TimeRange::new(0.0, f64::INFINITY).is_err());
        assert!(TimeRange::new(1.0, 0.0).is_err());
        assert!(TimeRange::new(-f64::MAX, f64::MAX).is_err());
        for point in [0.0, -0.25, 1.0e20, -1.0e20, f64::MAX, -f64::MAX] {
            let fit = TimeRange::new(point, point).unwrap().fit_visible();
            assert!(fit.start <= point && fit.end >= point);
            assert!(fit.duration() > 0.0 && fit.duration().is_finite());
            TimeRange::new(fit.start, fit.end).unwrap();
        }
        let enormous = TimeRange::new(-f64::MAX / 2.0, f64::MAX / 2.0).unwrap();
        assert!(enormous.fit_visible().duration().is_finite());
    }

    #[test]
    fn hierarchy_identity_and_fractional_time_queries_are_prepared_once() {
        let data = data(vec![
            track(2, Some(1), &[1.5, -0.25, 0.5, 0.5, 20.0]),
            track(1, None, &[0.0]),
            track(3, Some(2), &[]),
            track(4, None, &[]),
        ]);
        let prepared = Prepared::new(&data).unwrap();
        assert_eq!(prepared.total_keys, 6);
        assert_eq!(prepared.track_index(TrackId(2)), Some(0));
        assert_eq!(prepared.key_index(TrackId(2), KeyId(2)), Some(2));
        assert_eq!(prepared.key_index(TrackId(1), KeyId(2)), None);
        assert_eq!(prepared.key_index(TrackId(9), KeyId(0)), None);
        assert_eq!(
            prepared.visible_rows(&data, &BTreeSet::new()),
            vec![(1, 0), (0, 1), (2, 2), (3, 0)]
        );
        assert_eq!(
            prepared.visible_rows(&data, &BTreeSet::from([TrackId(2)])),
            vec![(1, 0), (0, 1), (3, 0)]
        );
        assert_eq!(
            &prepared.key_indices(0)[prepared.key_range(0, TimeRange::new(-0.25, 0.5).unwrap())],
            &[1, 2, 3]
        );
        assert!(
            prepared
                .key_range(0, TimeRange::new(2.0, 3.0).unwrap())
                .is_empty()
        );
        assert_eq!(prepared.key_indices(0), &[1, 2, 3, 0, 4]);
    }

    #[test]
    fn rejects_duplicate_missing_cyclic_and_nonfinite_data() {
        for tracks in [
            vec![track(1, None, &[]), track(1, None, &[])],
            vec![track(1, Some(2), &[])],
            vec![track(1, Some(1), &[])],
            vec![track(1, Some(2), &[]), track(2, Some(1), &[])],
            vec![track(1, None, &[f64::NAN])],
        ] {
            assert!(Prepared::new(&data(tracks)).is_err());
        }
        let mut duplicate = track(1, None, &[0.0, 1.0]);
        duplicate.keys[1].id = duplicate.keys[0].id;
        assert!(Prepared::new(&data(vec![duplicate])).is_err());
        // The same key ID on separate tracks is legal.
        assert!(Prepared::new(&data(vec![track(1, None, &[0.0]), track(2, None, &[0.0])])).is_ok());
    }

    #[test]
    fn deep_hierarchy_uses_iterative_validation_and_expansion() {
        let data = data(
            (0..10_000)
                .map(|id| track(id, id.checked_sub(1), &[]))
                .collect(),
        );
        let prepared = Prepared::new(&data).unwrap();
        let rows = prepared.visible_rows(&data, &BTreeSet::new());
        assert_eq!(rows.len(), 10_000);
        assert_eq!(rows.last(), Some(&(9_999, 9_999)));
    }
}
