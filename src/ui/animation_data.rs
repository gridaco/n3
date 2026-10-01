//! Read-only presentation adapter for immutable runtime animation clips.
//!
//! Construct once per source/clip and retain the result at a stable address. The
//! timeline consumes this snapshot; it never evaluates a pose or owns the source
//! animation schema. Node/channel indices are identities within that snapshot,
//! not authored document IDs or identities to preserve across a source reload.
use std::collections::{BTreeMap, BTreeSet};

use crate::scene::{Channel, Interpolation, Property, SceneAsset};

use super::timeline::{Data, Key, KeyId, TimeRange, Track, TrackId};

// This is a presentation budget, independent of runtime asset/evaluation
// limits. Text expansion and timeline indexes can be much larger than compact
// source arrays. Keep the 100k-key workbench case comfortably supported while
// bounding synchronous preparation and retained UI memory. No keys are omitted:
// exceeding a limit makes inspection unavailable, without disabling playback.
const MAX_PRESENTATION_KEYS: usize = 250_000;
const MAX_PRESENTATION_TRACKS: usize = 20_480;
const MAX_PRESENTATION_TEXT_BYTES: usize = 64 * 1024 * 1024;

pub(crate) fn clip_data(asset: &SceneAsset, clip: usize) -> Result<Data, String> {
    clip_data_with_text_budget(asset, clip, MAX_PRESENTATION_TEXT_BYTES)
}

fn clip_data_with_text_budget(
    asset: &SceneAsset,
    clip: usize,
    text_limit: usize,
) -> Result<Data, String> {
    let animation = asset
        .animations
        .get(clip)
        .ok_or("The animation no longer exists.")?;
    let mut key_count = 0usize;
    for channel in &animation.channels {
        key_count = key_count
            .checked_add(channel.times.len())
            .filter(|&count| count <= MAX_PRESENTATION_KEYS)
            .ok_or("Timeline inspection exceeds its 250,000-key presentation limit. Playback remains available.")?;
    }
    // SceneAsset validates a forest at publication. Include every target and its
    // ancestors, even when it is outside the currently displayed scene: joints
    // and animated ancestors can still contribute to the visible deformation.
    let mut parents = vec![None; asset.nodes.len()];
    for (parent, node) in asset.nodes.iter().enumerate() {
        for &child in &node.children {
            parents[child] = Some(parent);
        }
    }
    let mut included = BTreeSet::new();
    for channel in &animation.channels {
        let mut current = Some(channel.node);
        while let Some(node) = current {
            if !included.insert(node) {
                break;
            }
            current = parents[node];
        }
    }
    let track_count = included.len() + animation.channels.len();
    if track_count > MAX_PRESENTATION_TRACKS {
        return Err("Timeline inspection exceeds its 20,480-track presentation limit. Playback remains available.".into());
    }
    let mut names = BTreeMap::new();
    for &node in &included {
        *names.entry(asset.nodes[node].name.as_str()).or_insert(0) += 1;
    }
    let mut tracks = Vec::with_capacity(track_count);
    let mut text_bytes = 0;
    for &node in &included {
        let name = &asset.nodes[node].name;
        // Reject an oversized source label before cloning it; charge the final
        // label below, including any generated disambiguation suffix.
        if name.len() > text_limit.saturating_sub(text_bytes) {
            return Err(text_budget_error());
        }
        let label = if name.trim().is_empty() {
            format!("Node {}", node + 1)
        } else if names[name.as_str()] > 1 {
            format!("{name} (node {})", node + 1)
        } else {
            name.clone()
        };
        charge_text(&mut text_bytes, label.len(), text_limit)?;
        tracks.push(Track {
            id: node_id(node),
            parent: parents[node].map(node_id),
            label,
            keys: Vec::new(),
        });
    }
    let start = f64::from(animation.start);
    let mut end = f64::from(animation.duration);
    for (index, channel) in animation.channels.iter().enumerate() {
        let keys = channel
            .times
            .iter()
            .enumerate()
            .map(|(key, &time)| {
                // Subtract in f64 consistently. The stored f32 duration can be
                // rounded below this final key, so keep content bounds inclusive.
                let time = f64::from(time) - start;
                end = end.max(time);
                let metadata = key_metadata(channel, key);
                charge_text(&mut text_bytes, metadata.len(), text_limit)?;
                Ok(Key {
                    id: KeyId(key as u64),
                    time,
                    metadata: Some(metadata),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let label = match channel.property {
            Property::Translation => "Translation (cm)",
            Property::Rotation => "Rotation (quaternion)",
            Property::Scale => "Scale",
            Property::Weights => "Morph weights",
        };
        charge_text(&mut text_bytes, label.len(), text_limit)?;
        tracks.push(Track {
            id: channel_id(index),
            parent: Some(node_id(channel.node)),
            label: label.into(),
            keys,
        });
    }
    Ok(Data {
        // Immutable snapshots are replaced rather than mutated. The component
        // also keys preparation by address; the host retains this Data in Arc.
        revision: 1,
        content: TimeRange::new(0., end)?,
        tracks,
    })
}

fn charge_text(used: &mut usize, bytes: usize, limit: usize) -> Result<(), String> {
    *used = used
        .checked_add(bytes)
        .filter(|&total| total <= limit)
        .ok_or_else(text_budget_error)?;
    Ok(())
}

fn text_budget_error() -> String {
    "Timeline inspection exceeds its 64 MiB text presentation limit. Playback remains available."
        .into()
}

fn node_id(node: usize) -> TrackId {
    TrackId(node as u64 * 2)
}

fn channel_id(channel: usize) -> TrackId {
    TrackId(channel as u64 * 2 + 1)
}

fn key_metadata(channel: &Channel, key: usize) -> String {
    let start = key * channel.components;
    let end = start + channel.components;
    let values = &channel.values[start..end];
    let value = match channel.property {
        Property::Translation => format!("Local translation XYZ (cm): {}", vector(values)),
        Property::Rotation => format!("Local rotation quaternion XYZW: {}", vector(values)),
        Property::Scale => format!("Local scale factors XYZ: {}", vector(values)),
        // The runtime model does not retain morph target names. These indexes
        // are zero-based source target slots, not guessed semantic names.
        Property::Weights => format!(
            "Morph weights (zero-based target indexes): {}",
            values
                .iter()
                .enumerate()
                .map(|(index, value)| format!("{index}: {value}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let interpolation = match channel.interpolation {
        Interpolation::Step => "Step",
        Interpolation::Linear if channel.property == Property::Rotation => {
            "Linear (spherical quaternion interpolation)"
        }
        Interpolation::Linear => "Linear",
        Interpolation::CubicHermite => "Cubic Hermite",
    };
    let mut text = format!("{value}\nInterpolation: {interpolation}");
    if channel.interpolation == Interpolation::CubicHermite {
        let unit = if channel.property == Property::Translation {
            "cm/s"
        } else {
            "component units/s"
        };
        text.push_str(&format!(
            "\nIncoming derivative ({unit}): {}\nOutgoing derivative ({unit}): {}",
            vector(&channel.in_tangents[start..end]),
            vector(&channel.out_tangents[start..end]),
        ));
    }
    text
}

fn vector(values: &[f64]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        scene::{Animation, Morph, test_support},
        ui::timeline::data::Prepared,
    };

    fn asset_with_all_properties() -> SceneAsset {
        let asset = test_support::animated_asset();
        let mut data = (*asset).clone();
        let primitive = &mut data.meshes[0].primitives[0];
        primitive.morphs = vec![
            Morph {
                positions: vec![[0., 0., 1.]; 3],
                normals: Vec::new(),
                tangents: Vec::new(),
            };
            2
        ];
        data.meshes[0].weights = vec![0., 0.];
        let times = vec![2.25, 2.375, 4.5];
        data.animations = vec![Animation {
            name: "All properties".into(),
            start: 2.25,
            duration: 2.25,
            channels: [
                (Property::Translation, 3, vec![10., 20., 30.]),
                (Property::Rotation, 4, vec![0., 0., 0., 1.]),
                (Property::Scale, 3, vec![1., 2., 3.]),
                (Property::Weights, 2, vec![0.25, 0.75]),
            ]
            .into_iter()
            .enumerate()
            .map(|(index, (property, components, value))| {
                let interpolation = match index {
                    0 => Interpolation::CubicHermite,
                    3 => Interpolation::Step,
                    _ => Interpolation::Linear,
                };
                let tangents = if interpolation == Interpolation::CubicHermite {
                    vec![2.; times.len() * components]
                } else {
                    Vec::new()
                };
                Channel {
                    node: 0,
                    property,
                    interpolation,
                    times: times.clone(),
                    values: value.repeat(times.len()),
                    in_tangents: tangents.clone(),
                    out_tangents: tangents,
                    components,
                }
            })
            .collect(),
        }];
        SceneAsset::new(data).unwrap()
    }

    #[test]
    fn all_properties_keep_original_values_and_honest_inspection_metadata() {
        let asset = asset_with_all_properties();
        let data = clip_data(&asset, 0).unwrap();
        Prepared::new(&data).unwrap();
        assert_eq!(data.tracks.len(), 5);
        let metadata: Vec<_> = data.tracks[1..]
            .iter()
            .map(|track| track.keys[0].metadata.as_deref().unwrap())
            .collect();
        assert!(metadata[0].contains("Local translation XYZ (cm): [10, 20, 30]"));
        assert!(metadata[0].contains("Cubic Hermite"));
        assert!(metadata[0].contains("Incoming derivative (cm/s): [2, 2, 2]"));
        assert!(metadata[0].contains("Outgoing derivative (cm/s): [2, 2, 2]"));
        assert!(metadata[1].contains("quaternion XYZW: [0, 0, 0, 1]"));
        assert!(metadata[1].contains("spherical quaternion"));
        assert!(!metadata[1].contains("Euler"));
        assert!(metadata[2].contains("Local scale factors XYZ: [1, 2, 3]"));
        assert!(metadata[3].contains("zero-based target indexes): 0: 0.25, 1: 0.75"));
        assert!(metadata[3].contains("Interpolation: Step"));
    }

    #[test]
    fn local_time_keeps_irregular_keys_and_stable_identities() {
        let asset = asset_with_all_properties();
        let a = clip_data(&asset, 0).unwrap();
        let b = clip_data(&asset, 0).unwrap();
        assert_eq!(a.content, TimeRange::new(0., 2.25).unwrap());
        for (channel, (a, b)) in a.tracks[1..].iter().zip(&b.tracks[1..]).enumerate() {
            assert_eq!(a.id, channel_id(channel));
            assert_eq!(a.id, b.id);
            assert_eq!(a.parent, Some(node_id(0)));
            let actual: Vec<_> = a.keys.iter().map(|key| (key.id, key.time)).collect();
            assert_eq!(
                actual,
                vec![(KeyId(0), 0.), (KeyId(1), 0.125), (KeyId(2), 2.25)]
            );
        }
    }

    #[test]
    fn clip_hierarchy_includes_ancestors_and_targets_outside_active_scene() {
        let asset = test_support::animated_asset();
        let mut source = (*asset).clone();
        let mut node = source.nodes[0].clone();
        node.mesh = None;
        node.name = "Joint".into();
        source.nodes.extend([node.clone(), node]);
        source.nodes[1].children = vec![2];
        source.animations[0].channels[0].node = 2;
        // Node 1 and its child 2 are deliberately absent from scene 0's roots.
        let data = clip_data(&SceneAsset::new(source).unwrap(), 0).unwrap();
        Prepared::new(&data).unwrap();
        assert_eq!(data.tracks.len(), 3);
        assert_eq!(data.tracks[0].id, node_id(1));
        assert_eq!(data.tracks[0].parent, None);
        assert_eq!(data.tracks[0].label, "Joint (node 2)");
        assert_eq!(data.tracks[1].id, node_id(2));
        assert_eq!(data.tracks[1].parent, Some(node_id(1)));
        assert_eq!(data.tracks[1].label, "Joint (node 3)");
        assert_eq!(data.tracks[2].parent, Some(node_id(2)));
    }

    #[test]
    fn endpoint_is_inclusive_despite_source_duration_rounding() {
        let asset = test_support::animated_asset();
        let mut source = (*asset).clone();
        let clip = &mut source.animations[0];
        clip.start = 0.1;
        clip.channels[0].times = vec![0.1, 1.];
        clip.duration = 1. - clip.start;
        let source_duration = f64::from(clip.duration);
        let data = clip_data(&SceneAsset::new(source).unwrap(), 0).unwrap();
        let last = data.tracks[1].keys.last().unwrap().time;
        assert!(last > source_duration);
        assert_eq!(data.content.end, last);
        Prepared::new(&data).unwrap();
    }

    fn dense_clip(key_count: usize) -> SceneAsset {
        let asset = test_support::animated_asset();
        let mut source = (*asset).clone();
        let clip = &mut source.animations[0];
        clip.duration = (key_count - 1) as f32;
        clip.channels[0].times = (0..key_count).map(|index| index as f32).collect();
        clip.channels[0].values = vec![0.; key_count * 3];
        SceneAsset::new(source).unwrap()
    }

    #[test]
    fn presentation_budget_accepts_dense_case_and_rejects_larger_valid_source() {
        let asset = dense_clip(100_000);
        let data = clip_data(&asset, 0).unwrap();
        assert_eq!(data.tracks[1].keys.len(), 100_000);
        Prepared::new(&data).unwrap();
        let oversized = dense_clip(MAX_PRESENTATION_KEYS + 1);
        assert!(
            clip_data(&oversized, 0)
                .unwrap_err()
                .contains("250,000-key presentation limit")
        );
        // Presentation rejection cannot change the source or disable ordinary
        // evaluation. This does not allocate its expanded inspection strings.
        oversized
            .evaluate(0, Some(crate::scene::AnimationSample { clip: 0, time: 1. }))
            .unwrap();
    }

    #[test]
    fn text_budget_counts_actual_metadata_and_labels_without_truncation() {
        let asset = asset_with_all_properties();
        let data = clip_data(&asset, 0).unwrap();
        let bytes = data
            .tracks
            .iter()
            .map(|track| {
                track.label.len()
                    + track
                        .keys
                        .iter()
                        .map(|key| key.metadata.as_ref().unwrap().len())
                        .sum::<usize>()
            })
            .sum::<usize>();
        clip_data_with_text_budget(&asset, 0, bytes).unwrap();
        assert!(
            clip_data_with_text_budget(&asset, 0, bytes - 1)
                .unwrap_err()
                .contains("text presentation limit")
        );
        // A fresh accepted snapshot still contains all keys and metadata.
        assert_eq!(clip_data(&asset, 0).unwrap().tracks[1].keys.len(), 3);
    }

    #[test]
    fn single_key_clip_stays_zero_duration_and_missing_clip_is_reported() {
        let asset = test_support::animated_asset();
        let mut source = (*asset).clone();
        let clip = &mut source.animations[0];
        clip.start = -3.;
        clip.duration = 0.;
        clip.channels[0].times = vec![-3.];
        clip.channels[0].values.truncate(3);
        let asset = SceneAsset::new(source).unwrap();
        let data = clip_data(&asset, 0).unwrap();
        assert_eq!(data.content, TimeRange::new(0., 0.).unwrap());
        assert_eq!(data.tracks[1].keys[0].time, 0.);
        Prepared::new(&data).unwrap();
        assert!(
            clip_data(&asset, 1)
                .unwrap_err()
                .contains("no longer exists")
        );
    }
}
