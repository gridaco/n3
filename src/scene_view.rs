//! Runtime preview state shared by placements of the same linked scene.
//! Playback and exposure never change authored asset content or edit history.
use std::sync::Arc;

use crate::scene::{AnimationSample, EvaluatedScene, SceneAsset};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Action {
    Clip(Option<usize>),
    Playing(bool),
    RestPose,
    Seek(f32),
    Loop(bool),
    Speed(f32),
    Exposure(f32),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Playback {
    pub clip: Option<usize>,
    /// Seconds from the selected clip's first key, not wall-clock time.
    pub position: f32,
    pub playing: bool,
    pub looping: bool,
    pub speed: f32,
}
impl Default for Playback {
    fn default() -> Self {
        Self {
            clip: None,
            position: 0.,
            playing: false,
            looping: true,
            speed: 1.,
        }
    }
}
impl Playback {
    fn advance(&mut self, elapsed: f64, duration: f32) -> bool {
        if !self.playing || self.clip.is_none() || !elapsed.is_finite() || elapsed <= 0. {
            return false;
        }
        if duration <= 0. {
            self.playing = false;
            return false;
        }
        let next = f64::from(self.position) + elapsed * f64::from(self.speed);
        let position = if self.looping {
            next.rem_euclid(f64::from(duration)) as f32
        } else {
            if next >= f64::from(duration) {
                self.playing = false;
            }
            next.min(f64::from(duration)) as f32
        };
        let changed = self.position != position;
        self.position = position;
        changed
    }
}

#[derive(Clone)]
pub(crate) struct SceneView {
    pub asset: Arc<SceneAsset>,
    pub frame: Arc<EvaluatedScene>,
    pub scene: usize,
    pub playback: Playback,
    pub exposure: f32,
    /// Re-evaluation identity, independent of scene/editor geometry revisions.
    pub revision: u64,
}
impl SceneView {
    #[cfg(test)]
    pub fn new(asset: SceneAsset) -> Result<Self, String> {
        let scene = asset.default_scene;
        Self::from_shared(Arc::new(asset), scene)
    }
    pub fn from_shared(asset: Arc<SceneAsset>, scene: usize) -> Result<Self, String> {
        let frame = asset.evaluate(scene, None)?;
        Ok(Self {
            asset,
            frame,
            scene,
            playback: Playback::default(),
            exposure: 0.,
            revision: 0,
        })
    }
    pub fn duration(&self) -> f32 {
        self.playback
            .clip
            .and_then(|index| self.asset.animations.get(index))
            .map_or(0., |clip| clip.duration)
    }
    fn evaluate(&self, scene: usize, playback: &Playback) -> Result<Arc<EvaluatedScene>, String> {
        let sample = playback.clip.map(|clip| AnimationSample {
            clip,
            time: self.asset.animations[clip].start + playback.position,
        });
        self.asset.evaluate(scene, sample)
    }
    pub fn apply(&mut self, action: Action) -> Result<(), String> {
        let mut playback = self.playback.clone();
        match action {
            Action::Clip(clip) => {
                if clip.is_some_and(|index| index >= self.asset.animations.len()) {
                    return Err("The animation no longer exists.".into());
                }
                playback.clip = clip;
                playback.position = 0.;
                playback.playing = false;
            }
            Action::Playing(playing) => {
                if playback.playing == playing || self.asset.animations.is_empty() {
                    return Ok(());
                }
                if playback.clip.is_none() {
                    playback.clip = Some(0);
                }
                let duration = self.asset.animations[playback.clip.unwrap()].duration;
                if playing && playback.position >= duration {
                    playback.position = 0.;
                }
                playback.playing = playing && duration > 0.;
            }
            Action::RestPose => {
                playback.clip = None;
                playback.position = 0.;
                playback.playing = false;
            }
            Action::Seek(seconds) => {
                if !seconds.is_finite() {
                    return Err("Animation time must be finite.".into());
                }
                playback.position = seconds.clamp(0., self.duration());
                playback.playing = false;
            }
            Action::Loop(value) => {
                self.playback.looping = value;
                return Ok(());
            }
            Action::Speed(value) => {
                if !value.is_finite() || !(0.1..=4.).contains(&value) {
                    return Err("Playback speed must be between 0.1 and 4.".into());
                }
                self.playback.speed = value;
                return Ok(());
            }
            Action::Exposure(value) => {
                if !value.is_finite() || !(-8. ..=8.).contains(&value) {
                    return Err("Exposure must be between -8 and 8 EV.".into());
                }
                self.exposure = value;
                return Ok(());
            }
        }
        self.restore_playback(playback)
    }
    /// Restore accepted runtime transport without changing authored content.
    /// A failed validation or evaluation leaves both pose and transport intact.
    /// Restoring a changed time evaluates that pose again; hosts retaining an
    /// entire `SceneView` snapshot may instead restore its already evaluated frame.
    fn restore_playback(&mut self, playback: Playback) -> Result<(), String> {
        let duration = match playback.clip {
            Some(index) => {
                self.asset
                    .animations
                    .get(index)
                    .ok_or("The animation no longer exists.")?
                    .duration
            }
            None => 0.,
        };
        if !playback.position.is_finite() || !(0. ..=duration).contains(&playback.position) {
            return Err("Animation time must be within the selected clip.".into());
        }
        if !playback.speed.is_finite() || !(0.1..=4.).contains(&playback.speed) {
            return Err("Playback speed must be between 0.1 and 4.".into());
        }
        if playback.playing && (playback.clip.is_none() || duration <= 0.) {
            return Err("Playback requires an animation with a positive duration.".into());
        }
        // Transport-only changes (notably Pause) reuse the exact visible pose.
        // Evaluate a changed pose before publication, so failures are recoverable.
        let frame =
            if playback.clip == self.playback.clip && playback.position == self.playback.position {
                self.frame.clone()
            } else {
                self.evaluate(self.scene, &playback)?
            };
        if !Arc::ptr_eq(&frame, &self.frame) {
            self.revision = self.revision.wrapping_add(1);
        }
        self.playback = playback;
        self.frame = frame;
        Ok(())
    }
    pub fn advance(&mut self, elapsed: f64) -> Result<bool, String> {
        let mut playback = self.playback.clone();
        if !playback.advance(elapsed, self.duration()) {
            self.playback = playback;
            return Ok(false);
        }
        let frame = self.evaluate(self.scene, &playback)?;
        self.playback = playback;
        self.frame = frame;
        self.revision = self.revision.wrapping_add(1);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn running(looping: bool) -> Playback {
        Playback {
            clip: Some(0),
            playing: true,
            looping,
            ..Default::default()
        }
    }
    fn view() -> SceneView {
        SceneView::new(crate::scene::test_support::animated_asset()).unwrap()
    }
    #[test]
    fn invalid_actions_leave_scene_pose_and_transport_unchanged() {
        let mut view = view();
        view.apply(Action::Playing(true)).unwrap();
        view.advance(0.5).unwrap();
        let frame = view.frame.clone();
        let playback = view.playback.clone();
        let revision = view.revision;
        for action in [
            Action::Clip(Some(usize::MAX)),
            Action::Seek(f32::NAN),
            Action::Speed(0.),
            Action::Speed(f32::INFINITY),
            Action::Exposure(9.),
        ] {
            assert!(view.apply(action).is_err());
            assert!(Arc::ptr_eq(&frame, &view.frame));
            assert_eq!(playback, view.playback);
            assert_eq!(revision, view.revision);
            assert_eq!(view.scene, view.asset.default_scene);
            assert_eq!(view.exposure, 0.);
        }
    }
    #[test]
    fn transport_and_presentation_reuse_pose_and_rest_recovers_authored_cache() {
        let mut view = view();
        let rest = view.frame.clone();
        view.apply(Action::Playing(true)).unwrap();
        view.advance(0.75).unwrap();
        let frame = view.frame.clone();
        let revision = view.revision;
        view.apply(Action::Playing(false)).unwrap();
        assert!(!view.playback.playing);
        for action in [Action::Loop(false), Action::Speed(2.), Action::Exposure(1.)] {
            view.apply(action).unwrap();
            assert!(Arc::ptr_eq(&frame, &view.frame));
            assert_eq!(revision, view.revision);
        }
        assert!(!view.advance(10.).unwrap());
        view.apply(Action::Seek(f32::MAX)).unwrap();
        assert_eq!(view.playback.position, view.duration());
        view.apply(Action::Playing(true)).unwrap();
        assert_eq!(view.playback.position, 0.);
        assert!(view.playback.playing);
        view.advance(view.duration() as f64).unwrap();
        assert!(!view.playback.playing);
        view.apply(Action::RestPose).unwrap();
        assert!(Arc::ptr_eq(&rest, &view.frame));
        assert_eq!(view.playback.clip, None);
    }
    #[test]
    fn explicit_play_state_is_idempotent_and_pause_does_not_choose_a_clip() {
        let mut view = view();
        let rest = view.frame.clone();
        view.apply(Action::Playing(false)).unwrap();
        assert_eq!(view.playback.clip, None);
        assert!(Arc::ptr_eq(&rest, &view.frame));

        view.apply(Action::Playing(true)).unwrap();
        view.advance(0.5).unwrap();
        let frame = view.frame.clone();
        let revision = view.revision;
        for playing in [true, true, false, false] {
            view.apply(Action::Playing(playing)).unwrap();
            assert_eq!(view.playback.playing, playing);
            assert_eq!(view.playback.position, 0.5);
            assert!(Arc::ptr_eq(&frame, &view.frame));
            assert_eq!(view.revision, revision);
        }
        view.apply(Action::Seek(view.duration())).unwrap();
        view.apply(Action::Playing(true)).unwrap();
        assert_eq!(view.playback.position, 0.);
        assert!(view.playback.playing);
    }
    #[test]
    fn restoring_playback_recovers_time_clip_and_transport_after_preview() {
        let mut view = view();
        let rest = view.frame.clone();
        let rest_playback = view.playback.clone();
        view.apply(Action::Playing(true)).unwrap();
        view.apply(Action::Loop(false)).unwrap();
        view.apply(Action::Speed(2.)).unwrap();
        view.advance(0.25).unwrap();
        let baseline = view.playback.clone();
        let frame = view.frame.clone();
        view.apply(Action::Seek(1.75)).unwrap();
        assert!(!view.playback.playing);
        assert_ne!(view.frame.draws[0].vertices, frame.draws[0].vertices);

        view.restore_playback(baseline.clone()).unwrap();
        assert_eq!(view.playback, baseline);
        assert_eq!(view.frame.draws[0].vertices, frame.draws[0].vertices);
        assert_eq!(view.frame.node_world, frame.node_world);
        assert_eq!(view.frame.bounds, frame.bounds);
        view.restore_playback(rest_playback).unwrap();
        assert_eq!(view.playback.clip, None);
        assert!(Arc::ptr_eq(&rest, &view.frame));
    }
    #[test]
    fn invalid_playback_snapshots_preserve_the_last_accepted_state() {
        let mut view = view();
        view.apply(Action::Playing(true)).unwrap();
        view.advance(0.5).unwrap();
        let baseline = view.playback.clone();
        let frame = view.frame.clone();
        let revision = view.revision;
        for playback in [
            Playback {
                clip: Some(usize::MAX),
                ..baseline.clone()
            },
            Playback {
                position: f32::NAN,
                ..baseline.clone()
            },
            Playback {
                position: -1.,
                ..baseline.clone()
            },
            Playback {
                position: view.duration() + 1.,
                ..baseline.clone()
            },
            Playback {
                speed: 0.,
                ..baseline.clone()
            },
            Playback {
                speed: f32::INFINITY,
                ..baseline.clone()
            },
            Playback {
                clip: None,
                position: 0.,
                ..baseline.clone()
            },
        ] {
            assert!(view.restore_playback(playback).is_err());
            assert_eq!(view.playback, baseline);
            assert!(Arc::ptr_eq(&view.frame, &frame));
            assert_eq!(view.revision, revision);
        }
    }
    #[test]
    fn failed_seek_and_restore_keep_the_visible_pose_and_transport() {
        let mut data = (*crate::scene::test_support::animated_asset()).clone();
        let channel = &mut data.animations[0].channels[0];
        channel.property = crate::scene::Property::Rotation;
        channel.components = 4;
        channel.values = vec![0., 0., 0., 1., 0., 0., 0., 1.];
        channel.interpolation = crate::scene::Interpolation::CubicHermite;
        channel.in_tangents = vec![0.; 8];
        channel.out_tangents = vec![1e200, 0., 0., 0., 0., 0., 0., 0.];
        let mut view = SceneView::new(SceneAsset::new(data).unwrap()).unwrap();
        view.apply(Action::Playing(true)).unwrap();
        let baseline = view.playback.clone();
        let frame = view.frame.clone();
        let revision = view.revision;

        assert!(
            view.apply(Action::Seek(1.))
                .unwrap_err()
                .contains("quaternion")
        );
        assert!(
            view.restore_playback(Playback {
                position: 1.,
                ..baseline.clone()
            })
            .is_err()
        );
        assert!(view.advance(1.).is_err());
        assert_eq!(view.playback, baseline);
        assert!(Arc::ptr_eq(&view.frame, &frame));
        assert_eq!(view.revision, revision);
    }
    #[test]
    fn seek_and_restore_use_time_relative_to_a_nonzero_clip_start() {
        let mut data = (*crate::scene::test_support::animated_asset()).clone();
        data.animations[0].start = 12.;
        data.animations[0].channels[0].times = vec![12., 14.];
        let mut view = SceneView::new(SceneAsset::new(data).unwrap()).unwrap();
        view.apply(Action::Clip(Some(0))).unwrap();
        view.apply(Action::Seek(0.5)).unwrap();
        assert_eq!(view.frame.draws[0].vertices[0].position, [25., 0., 0.]);
        let baseline = view.playback.clone();
        view.apply(Action::Seek(2.)).unwrap();
        assert_eq!(view.frame.draws[0].vertices[0].position, [100., 0., 0.]);
        view.restore_playback(baseline).unwrap();
        assert_eq!(view.playback.position, 0.5);
        assert_eq!(view.frame.draws[0].vertices[0].position, [25., 0., 0.]);
    }
    #[test]
    fn explicit_time_loops_and_nonlooping_stops_at_last_frame() {
        let mut playback = running(true);
        assert!(playback.advance(2.5, 2.));
        assert_eq!(playback.position, 0.5);
        assert!(playback.playing);
        playback = running(false);
        assert!(playback.advance(2.5, 2.));
        assert_eq!(playback.position, 2.);
        assert!(!playback.playing);
    }
    #[test]
    fn pause_speed_zero_duration_and_invalid_clock_are_explicit() {
        let mut playback = running(true);
        playback.speed = 2.;
        playback.advance(0.25, 2.);
        assert_eq!(playback.position, 0.5);
        let before = playback.clone();
        for delta in [f64::NAN, f64::INFINITY, -1., 0.] {
            assert!(!playback.advance(delta, 2.));
            assert_eq!(playback, before);
        }
        playback.playing = false;
        assert!(!playback.advance(1., 2.));
        playback.playing = true;
        assert!(!playback.advance(1., 0.));
        assert!(!playback.playing);
    }
}
