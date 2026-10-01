//! Application adapter for the read-only timeline. Source data and playback
//! remain outside authored documents; the kit never knows about placed assets.
use super::*;
use crate::{
    document::AssetInstance,
    input::timeline_input::{self, TimelineKeyboardContext},
    scene::SceneAsset,
    scene_view::{Action, SceneView},
    ui::timeline::{self, CancelReason, Capabilities, Data, HostState, Request, Timeline},
};
use std::sync::Arc;

struct Source {
    key: AssetInstance,
    asset: Arc<SceneAsset>,
    clip: Option<usize>,
}

struct ScrubSession {
    gesture: u64,
    key: AssetInstance,
    baseline: SceneView,
}

pub(crate) struct AnimationPanel {
    source: Option<Source>,
    data_error: Option<String>,
    pub(crate) data: Arc<Data>,
    pub(crate) timeline: Timeline,
    pub(crate) observations: Vec<timeline::Observation>,
    scrub: Option<ScrubSession>,
    control_frame: Option<u64>,
    input_frame: Option<u64>,
    focus_pending: bool,
}

impl Default for AnimationPanel {
    fn default() -> Self {
        Self {
            source: None,
            data_error: None,
            data: Arc::new(empty_data()),
            timeline: Timeline::default(),
            observations: Vec::new(),
            scrub: None,
            control_frame: None,
            input_frame: None,
            focus_pending: false,
        }
    }
}

fn empty_data() -> Data {
    Data {
        revision: 1,
        content: timeline::TimeRange {
            start: 0.0,
            end: 0.0,
        },
        tracks: Vec::new(),
    }
}

fn host_state(view: &SceneView) -> HostState {
    HostState {
        accepted_time: f64::from(view.playback.position),
        playing: view.playback.playing,
        looping: view.playback.looping,
        speed: f64::from(view.playback.speed),
        available: true,
    }
}

impl WorkspaceUi {
    pub(super) fn focus_animation_panel(&mut self, ctx: &egui::Context) {
        self.animation.focus_pending = true;
        self.claim_animation_input(ctx);
        ctx.memory_mut(|memory| memory.request_focus(timeline_input::timeline_focus_id()));
        ctx.request_repaint();
    }

    pub(super) fn can_toggle_animation_playback(&self) -> bool {
        self.show_ui
            && self.animation_panel_is_open()
            && !self.editor.is_interacting()
            && !self.pie_owns_input()
            && !self.mouse_navigation.wants_input()
            && !self.animation.timeline.has_pointer_gesture()
            && self.animation.scrub.is_none()
            && self
                .selected_asset_view()
                .is_some_and(|view| !view.asset.animations.is_empty())
    }

    pub(super) fn toggle_animation_playback(&mut self, ctx: &egui::Context) {
        if !self.can_toggle_animation_playback() {
            return;
        }
        let playing = self.selected_asset_view().unwrap().playback.playing;
        self.dispatch_asset_action(Action::Playing(!playing), ctx);
    }

    pub(super) fn selected_asset_key(&self) -> Option<AssetInstance> {
        let object = self
            .editor
            .document
            .objects
            .iter()
            .find(|object| Some(object.id) == self.editor.selected_object)?;
        match &object.geometry {
            Geometry::Asset(key) => Some(key.clone()),
            _ => None,
        }
    }

    pub(super) fn animation_owns_input(&self, ctx: &egui::Context) -> bool {
        self.animation.input_frame == Some(ctx.cumulative_frame_nr())
    }

    fn claim_animation_input(&mut self, ctx: &egui::Context) {
        self.animation.input_frame = Some(ctx.cumulative_frame_nr());
        shortcuts::suppress_viewport_input(ctx);
    }

    /// Called before viewport held-key routing and playback ticking. Retain
    /// ownership through cancellation/release, including egui layout retries.
    pub(super) fn prepare_animation_frame(&mut self, ctx: &egui::Context) {
        if ctx.current_pass_index() != 0 {
            return;
        }
        if self.animation.timeline.has_pointer_gesture() || self.animation.scrub.is_some() {
            self.claim_animation_input(ctx);
            let source_changed = self
                .animation
                .source
                .as_ref()
                .is_some_and(|source| self.selected_asset_key().as_ref() != Some(&source.key));
            if !self.show_ui
                || !self.animation_panel_is_open()
                || !ctx.input(|input| input.focused)
                || source_changed
            {
                self.cancel_animation_interaction(ctx);
            }
        }
        let commands = self.animation_keyboard.route(
            ctx,
            TimelineKeyboardContext {
                active: self.show_ui && self.animation_panel_is_open(),
                ready: self.can_toggle_animation_playback(),
            },
        );
        if self.animation_keyboard.owns_frame {
            self.claim_animation_input(ctx);
        }
        for command in commands {
            self.dispatch(command, ctx, false);
        }
    }

    /// Inspection selection and scrubbing share panel ownership, but only a
    /// scrub has playback to restore. A cancelled box must not seek on release.
    pub(super) fn cancel_animation_interaction(&mut self, ctx: &egui::Context) {
        self.animation.timeline.cancel(CancelReason::Unavailable);
        self.cancel_animation_scrub(ctx);
    }

    pub(super) fn cancel_animation_scrub(&mut self, ctx: &egui::Context) {
        let Some(session) = self.animation.scrub.take() else {
            return;
        };
        self.animation.timeline.cancel(CancelReason::Unavailable);
        // A replaced source or new document must never resurrect old resources.
        if let Some(current) = self.asset_views.get(&session.key)
            && Arc::ptr_eq(&current.asset, &session.baseline.asset)
        {
            let mut baseline = session.baseline;
            baseline.revision = current.revision.wrapping_add(1);
            let result = self.publish_asset_view(&session.key, baseline);
            self.report(result);
        }
        self.scene_tick_time = Some(ctx.input(|input| input.time));
        ctx.request_repaint();
    }

    fn animation_seek(
        &mut self,
        key: &AssetInstance,
        time: f64,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        if !time.is_finite() {
            return Err("Animation time must be finite.".into());
        }
        let mut candidate = self
            .asset_views
            .get(key)
            .ok_or("Animation source is unavailable.")?
            .clone();
        // Inspecting the first clip at Rest does not evaluate it until a seek
        // or Play request. Apply both operations before publishing any state.
        if candidate.playback.clip.is_none() {
            candidate.apply(Action::Clip(Some(0)))?;
        }
        let requested = time;
        let time = time.clamp(0.0, f64::from(candidate.duration())) as f32;
        candidate.apply(Action::Seek(time))?;
        self.publish_asset_view(key, candidate)?;
        // Imported playback currently evaluates f32 seconds. Acknowledge exact
        // acceptance at that precision, without lying about the host playhead
        // or adding a tolerance to the format-agnostic kit. Additional clamping
        // (for example a drag outside the clip) remains visible to the user.
        if requested as f32 == time {
            self.animation.timeline.acknowledge_seek(requested);
        }
        self.scene_tick_time = Some(ctx.input(|input| input.time));
        ctx.request_repaint();
        Ok(())
    }

    fn animation_request(&mut self, key: &AssetInstance, request: Request, ctx: &egui::Context) {
        // The kit gates input too; repeat the capability check at the effect
        // boundary so a late selection/edit change cannot bypass it.
        if matches!(request, Request::ScrubCancel { .. }) {
            if let Request::ScrubCancel { gesture, .. } = request
                && self
                    .animation
                    .scrub
                    .as_ref()
                    .is_some_and(|s| s.gesture == gesture && &s.key == key)
            {
                self.cancel_animation_scrub(ctx);
            }
            return;
        }
        if self.editor.is_interacting() {
            return;
        }
        let result = match request {
            Request::ScrubBegin { gesture, time } => {
                self.cancel_animation_scrub(ctx);
                if let Some(baseline) = self.asset_views.get(key).cloned() {
                    self.animation.scrub = Some(ScrubSession {
                        gesture,
                        key: key.clone(),
                        baseline,
                    });
                }
                self.animation_seek(key, time, ctx)
            }
            Request::ScrubUpdate { gesture, time } | Request::ScrubEnd { gesture, time } => {
                if !self
                    .animation
                    .scrub
                    .as_ref()
                    .is_some_and(|s| s.gesture == gesture && &s.key == key)
                {
                    return;
                }
                let result = self.animation_seek(key, time, ctx);
                if matches!(request, Request::ScrubEnd { .. }) {
                    self.animation.scrub = None;
                }
                result
            }
            Request::Seek { time } => self.animation_seek(key, time, ctx),
            Request::SetPlaying(playing) => {
                self.apply_asset_action(key, Action::Playing(playing), ctx)
            }
            Request::SetLooping(looping) => {
                self.apply_asset_action(key, Action::Loop(looping), ctx)
            }
            Request::SetSpeed(speed) => {
                self.apply_asset_action(key, Action::Speed(speed as f32), ctx)
            }
            Request::ScrubCancel { .. } => unreachable!(),
        };
        if let Err(error) = result {
            // Failed candidates never replace the accepted pose. Pause the
            // failing source so subsequent frames do not retry indefinitely.
            if !matches!(request, Request::SetSpeed(_) | Request::SetLooping(_))
                && let Some(view) = self.asset_views.get_mut(key)
            {
                view.playback.playing = false;
            }
            self.error = Some(error);
        }
    }

    fn sync_animation_source(&mut self, key: &AssetInstance, ctx: &egui::Context) {
        let view = self.asset_views.get(key).cloned();
        let clip = view.as_ref().and_then(|view| {
            view.playback
                .clip
                .or_else(|| (!view.asset.animations.is_empty()).then_some(0))
        });
        let changed = match (&self.animation.source, &view) {
            (Some(source), Some(view)) => {
                &source.key != key
                    || source.clip != clip
                    || !Arc::ptr_eq(&source.asset, &view.asset)
            }
            (None, None) => false,
            _ => true,
        };
        if changed {
            self.cancel_animation_interaction(ctx);
            self.animation.timeline = Timeline::default();
            self.animation.source = view.as_ref().map(|view| Source {
                key: key.clone(),
                asset: view.asset.clone(),
                clip,
            });
            self.animation.data_error = None;
            self.animation.data = Arc::new(match (&view, clip) {
                (Some(view), Some(clip)) => crate::ui::animation_data::clip_data(&view.asset, clip)
                    .unwrap_or_else(|error| {
                        self.animation.data_error = Some(error);
                        Data {
                            content: timeline::TimeRange {
                                start: 0.0,
                                end: f64::from(view.asset.animations[clip].duration),
                            },
                            ..empty_data()
                        }
                    }),
                _ => empty_data(),
            });
        }
    }

    pub(super) fn animation_panel_contents(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let key = self.selected_asset_key();
        if let Some(key) = &key {
            self.sync_animation_source(key, &ctx);
        } else {
            self.cancel_animation_interaction(&ctx);
            self.animation.observations.clear();
        }
        controls::scope(&ctx, Control::AnimationTimeline, || {
            if let Some(key) = &key {
                self.animation_contents(ui, key);
            } else {
                self.animation_empty(ui, "Select an imported asset to inspect its animation.");
            }
        });
    }

    fn animation_empty(&mut self, ui: &mut egui::Ui, message: &str) {
        ui.weak(message);
        let response = ui.interact(
            ui.available_rect_before_wrap(),
            timeline_input::timeline_focus_id(),
            egui::Sense::click(),
        );
        if response.clicked() || self.animation.focus_pending {
            ui.ctx()
                .memory_mut(|memory| memory.request_focus(response.id));
            self.animation.focus_pending = false;
        }
    }

    fn animation_contents(&mut self, ui: &mut egui::Ui, key: &AssetInstance) {
        let ctx = ui.ctx().clone();
        let Some(view) = self.asset_views.get(key).cloned() else {
            self.animation.observations.clear();
            self.animation_empty(
                ui,
                "Source unavailable. Restore the source and reopen the document.",
            );
            return;
        };
        let idle = !self.editor.is_interacting() && !self.pie_input.owns_frame;
        let has_clips = !view.asset.animations.is_empty();
        let object_name = self
            .editor
            .document
            .objects
            .iter()
            .find(|object| Some(object.id) == self.editor.selected_object)
            .map_or("Imported asset", |object| object.name.as_str())
            .to_owned();
        let mut header_action = None;
        ui.horizontal_wrapped(|ui| {
            ui.weak(&object_name);
            let mut selected = view.playback.clip;
            let combo = ui.add_enabled_ui(idle && has_clips, |ui| {
                menu::value_dropdown(ui,
                    egui::ComboBox::from_id_salt("animation.clip")
                        .width(180.0)
                        .selected_text(selected.map_or("Rest pose", |i| view.asset.animations[i].name.as_str())),
                    Control::SceneClip, |ui| {
                        menu::selectable_value(ui, &mut selected, None, "Rest pose");
                        for (index, animation) in view.asset.animations.iter().enumerate() {
                            menu::selectable_value(ui, &mut selected, Some(index), &animation.name);
                        }
                    })
            }).inner;
            controls::record(&ctx, Control::SceneClip, Control::SceneClip.label(), combo.response.rect, idle && has_clips);
            if selected != view.playback.clip { header_action = Some(Action::Clip(selected)); }
            if controls::button_enabled(ui, Control::SceneRest, idle).clicked() { header_action = Some(Action::RestPose); }
            ui.weak("Read-only tracks").on_hover_text("All channels in the source clip, including joints and targets outside the placed source scene.");
        });
        if let Some(action) = header_action
            && self.animation.control_frame != Some(ctx.cumulative_frame_nr())
        {
            self.animation.control_frame = Some(ctx.cumulative_frame_nr());
            self.cancel_animation_interaction(&ctx);
            let result = self.apply_asset_action(key, action, &ctx);
            self.report(result);
            self.sync_animation_source(key, &ctx);
        }
        // Keep this status row present at both Rest and an evaluated pose.
        // Removing it on ScrubBegin resizes the kit and cancels its gesture.
        // Reserve geometry now; present its accepted-state text after requests.
        let status_rect = ui
            .allocate_exact_size(
                egui::vec2(
                    ui.available_width(),
                    ui.text_style_height(&egui::TextStyle::Body),
                ),
                egui::Sense::hover(),
            )
            .0;
        if let Some(error) = &self.animation.data_error {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!("Timeline tracks unavailable: {error}"),
            );
        }
        let host = host_state(&self.asset_views[key]);
        let cap = Capabilities {
            inspect: idle && has_clips,
            seek: idle && has_clips,
            play_pause: idle && has_clips,
            looping: idle && has_clips,
            speed: idle && has_clips,
        };
        let data = self.animation.data.clone();
        let prepared =
            self.animation
                .timeline
                .prepare(ui, timeline_input::timeline_id(), &data, &host, &cap);
        // Resolve the ordered requests before painting the accepted playhead.
        // This avoids rebuilding the entire workspace on every scrub frame.
        // The kit still suppresses effect repetition on genuine layout retries.
        for request in prepared.requests().iter().copied() {
            self.animation_request(key, request, &ctx);
        }
        let accepted = &self.asset_views[key];
        let output = self
            .animation
            .timeline
            .finish(ui, prepared, &host_state(accepted), &cap);
        let status = if !has_clips {
            "No animation clips in this source.".to_owned()
        } else if accepted.playback.clip.is_none() {
            format!(
                "Rest pose · showing keys for {}",
                accepted.asset.animations[0].name
            )
        } else {
            "Source clip · all channels · seconds from clip start".to_owned()
        };
        ui.new_child(
            egui::UiBuilder::new()
                .id(egui::Id::new("n3.animation.status"))
                .max_rect(status_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        )
        .add(egui::Label::new(egui::RichText::new(&status).weak()).truncate())
        .on_hover_text(&status);
        for observation in &output.observations {
            let control = match observation.target {
                timeline::Target::Ruler => Some(Control::AnimationRuler),
                timeline::Target::Transport(control) => Some(match control {
                    timeline::transport::TransportControl::PlayPause => Control::ScenePlay,
                    timeline::transport::TransportControl::Loop => Control::SceneLoop,
                    timeline::transport::TransportControl::Speed => Control::SceneSpeed,
                    timeline::transport::TransportControl::Time => Control::SceneTime,
                    timeline::transport::TransportControl::Fit => Control::AnimationFit,
                }),
                _ => None,
            };
            if let Some(control) = control {
                controls::record(
                    &ctx,
                    control,
                    control.label(),
                    observation.rect,
                    observation.enabled,
                );
            }
        }
        if self.animation.timeline.has_pointer_gesture() || self.animation.scrub.is_some() {
            self.claim_animation_input(&ctx);
        }
        // Copy pointer state before consulting Context again; nested Context
        // reads inside input() can deadlock egui's state lock.
        let pointer_press = ctx.input(|input| {
            input
                .pointer
                .any_pressed()
                .then(|| input.pointer.interact_pos())
                .flatten()
        });
        if let Some(canvas) = output
            .observations
            .iter()
            .find(|o| o.target == timeline::Target::Canvas)
            && idle
            && !ctx.text_edit_focused()
            && !egui::Popup::is_any_open(&ctx)
            && !ctx.memory(|memory| memory.top_modal_layer().is_some())
            && (self.animation.focus_pending
                || self.animation_owns_input(&ctx)
                || pointer_press.is_some_and(|p| {
                    // Canvas observations include the whole kit.
                    // Only its body/ruler take keyboard ownership;
                    // native transport fields and covering windows
                    // must keep their own focus.
                    canvas.rect.contains(p)
                        && ctx.layer_id_at(p) == Some(ui.layer_id())
                        && output.observations.iter().any(|observation| {
                            observation.target == timeline::Target::Ruler
                                && (observation.rect.contains(p)
                                    || p.y >= observation.rect.bottom())
                        })
                }))
        {
            ctx.memory_mut(|memory| memory.request_focus(canvas.id));
            self.animation.focus_pending = false;
        }
        self.animation.observations = output.observations;
    }
}
