//! Inspect imported clips through the real timeline and evaluated viewport.
use super::{ClipSpec, Result, Session};
use crate::{
    controls::Control,
    render::shading::ShadingMode,
    scene::{EvaluatedScene, Property},
    scene_view::SceneView,
    ui::timeline::Target,
};
use std::{sync::Arc, time::Duration};

fn scene<'a>(s: &'a Session<'_>) -> &'a SceneView {
    s.state
        .selected_asset_view()
        .expect("Selected animation fixture")
}

fn moved(before: &EvaluatedScene, after: &EvaluatedScene) -> bool {
    let points = |frame: &EvaluatedScene| {
        frame
            .draws
            .iter()
            .flat_map(|draw| draw.vertices.iter().map(|vertex| vertex.position))
            .collect::<Vec<_>>()
    };
    let before = points(before);
    let after = points(after);
    before.len() == after.len()
        && before
            .iter()
            .zip(after)
            .any(|(a, b)| a.iter().zip(b).any(|(a, b)| (a - b).abs() > 1e-5))
}

fn witness_controls(s: &mut Session<'_>) -> Result<()> {
    for control in [
        Control::ToolDockTabBar,
        Control::AnimationPanelToggle,
        Control::ToolDockClose,
        Control::AnimationTimeline,
        Control::AnimationRuler,
        Control::AnimationFit,
        Control::SceneClip,
        Control::ScenePlay,
        Control::SceneRest,
        Control::SceneTime,
        Control::SceneLoop,
        Control::SceneSpeed,
    ] {
        s.witness(control)?;
    }
    Ok(())
}

fn verify_panel_placement(s: &mut Session<'_>, open: bool) -> Result<()> {
    let tabs = s.trace.get(Control::ToolDockTabBar)?.rect;
    let animation = s.trace.get(Control::AnimationPanelToggle)?.rect;
    let viewport = s.trace.get(Control::Viewport)?.rect;
    let status = s.trace.get(Control::StatusBar)?.rect;
    s.require(
        tabs.contains_rect(animation) && tabs.bottom() <= status.top(),
        "Animation is a tab in the Tool Dock tab bar, separate from the status bar",
    )?;
    if open {
        let panel = s.trace.get(Control::AnimationTimeline)?.rect;
        let close = s.trace.get(Control::ToolDockClose)?.rect;
        // Native Panel reserves its resize-edge separator outside the frame;
        // the header's own padding starts below that rounded stroke width.
        let border = s
            .ctx
            .global_style()
            .visuals
            .widgets
            .noninteractive
            .bg_stroke
            .width
            .round();
        s.require(
            panel.contains_rect(tabs)
                && tabs.contains_rect(close)
                && tabs.top() < panel.center().y
                && viewport.bottom() <= tabs.top()
                && animation.height() == close.height()
                && animation.center().y == close.center().y
                && tabs.height() == crate::theme::TOOL_DOCK_TAB_BAR_HEIGHT
                && tabs.top() - panel.top() == border
                && animation.center().y == tabs.center().y
                && animation.top() - tabs.top() == crate::theme::TOOL_DOCK_TAB_BAR_INSET
                && animation.left() - panel.left() == crate::theme::TOOL_DOCK_TAB_BAR_INSET
                && animation.left() - panel.left() == panel.right() - close.right(),
            "The open Tool Dock owns a compact 32-point header with centered controls and equal 2-point container padding",
        )
    } else {
        let stats = s.trace.get(Control::SceneInfo)?.rect;
        s.require(
            viewport.contains_rect(tabs)
                && tabs.left() < viewport.center().x
                && stats.bottom() <= tabs.top()
                && s.trace.get(Control::ToolDockClose).is_err(),
            "The closed panel exposes a floating tab bar below the scene statistics at the viewport's lower-left",
        )
    }
}

/// Use the component's current visible interval, including Fit padding. The
/// pointer still reaches the production ruler through ordinary input routing.
fn ruler_point(s: &Session<'_>, fraction: f64) -> Result<egui::Pos2> {
    let ruler = s.trace.get(Control::AnimationRuler)?.rect;
    let visible = s
        .state
        .animation
        .timeline
        .visible_range()
        .ok_or("The animation timeline has no visible time range")?;
    let content = s.state.animation.data.content;
    let time = content.start + content.duration() * fraction;
    let amount = ((time - visible.start) / visible.duration()) as f32;
    let point = egui::pos2(ruler.left() + amount * ruler.width(), ruler.center().y);
    if !ruler.contains(point) {
        return Err("The requested tutorial time is outside the live ruler".into());
    }
    Ok(point)
}

fn begin_scrub(s: &mut Session<'_>, fraction: f64) -> Result<()> {
    let start = ruler_point(s, fraction)?;
    s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
    s.pointer_button(egui::PointerButton::Primary, true)
}

fn inspect_key(s: &mut Session<'_>) -> Result<()> {
    let object = s.state.editor.selected_object;
    let frame = scene(s).frame.clone();
    let playback = scene(s).playback.clone();
    let track = s
        .state
        .animation
        .data
        .tracks
        .iter()
        .find(|track| !track.keys.is_empty())
        .ok_or("The joint animation must expose its source keys")?;
    let target = Target::Key {
        track: track.id,
        key: track.keys[2].id,
    };
    let key = s
        .state
        .animation
        .observations
        .iter()
        .find(|observation| observation.target == target)
        .ok_or_else(|| {
            format!(
                "The real joint track must expose individual visible key targets: {:?}",
                s.state.animation.observations
            )
        })?
        .clone();
    s.click_at(key.rect.center())?;
    s.require(
        s.state.animation.timeline.selection().is_some()
            && s
                .state
                .animation
                .timeline
                .inspected_keys(&s.state.animation.data)
                .any(|key| key.metadata.is_some())
            && scene(s).playback == playback
            && Arc::ptr_eq(&frame, &scene(s).frame)
            && s.state.editor.selected_object == object,
        "Inspecting a live timeline key exposes its metadata without seeking, evaluating a pose, or changing object selection",
    )?;
    // Native egui tooltips intentionally stay hidden after clicking until the
    // pointer moves again. Re-enter after the click grace interval, then honor
    // the actual configured hover delay rather than forcing a popup open.
    s.wait(Duration::from_millis(150))?;
    s.hover(Control::AnimationRuler)?;
    s.frame(
        vec![egui::Event::PointerMoved(key.rect.center())],
        Duration::ZERO,
    )?;
    let delay = s.ctx.global_style().interaction.tooltip_delay;
    s.wait(Duration::from_secs_f32(delay + 0.25))?;
    s.require(
        egui::Tooltip::was_tooltip_open_last_frame(&s.ctx, key.id)
            && scene(s).playback == playback
            && Arc::ptr_eq(&frame, &scene(s).frame),
        "Hovering the selected source key opens its real metadata tooltip without changing the pose or playhead",
    )?;
    s.capture_tutorial("animation-inspection")
}

fn inspect_navigation(s: &mut Session<'_>) -> Result<()> {
    let frame = scene(s).frame.clone();
    let playback = scene(s).playback.clone();
    let camera = s.state.camera.view_projection(s.state.aspect());
    let before = s.state.animation.timeline.visible_range().unwrap();
    s.hover(Control::AnimationRuler)?;
    s.scroll(
        0.0,
        65.0,
        true,
        egui::Modifiers {
            command: true,
            mac_cmd: true,
            ..Default::default()
        },
    )?;
    s.modifiers_changed(egui::Modifiers::NONE)?;
    s.require(
        s.state.animation.timeline.visible_range() != Some(before)
            && scene(s).playback == playback
            && Arc::ptr_eq(&frame, &scene(s).frame)
            && camera == s.state.camera.view_projection(s.state.aspect()),
        "Scrolling over the timeline changes its visible interval without navigating the viewport or changing playback",
    )?;
    s.click(Control::AnimationFit)?;
    s.require(
        s.state.animation.timeline.visible_range() == Some(before)
            && Arc::ptr_eq(&frame, &scene(s).frame),
        &format!(
            "Fit restores the full clip interval with its original timing and no pose change (before={before:?}, after={:?}, same frame={}, fit={:?})",
            s.state.animation.timeline.visible_range(),
            Arc::ptr_eq(&frame, &scene(s).frame),
            s.trace.get(Control::AnimationFit)?.rect,
        ),
    )
}

fn box_select_keys(s: &mut Session<'_>) -> Result<()> {
    let before = scene(s).clone();
    let object_selection = s.state.editor.selected_objects.clone();
    let track = s
        .state
        .animation
        .data
        .tracks
        .iter()
        .find(|track| track.keys.len() >= 4)
        .ok_or("Box selection needs four visible source keys")?;
    let targets: Vec<_> = track.keys[1..4]
        .iter()
        .map(|key| Target::Key {
            track: track.id,
            key: key.id,
        })
        .collect();
    let bounds: Vec<_> = targets
        .iter()
        .map(|target| {
            s.state
                .animation
                .observations
                .iter()
                .find(|observation| &observation.target == target)
                .map(|observation| observation.rect)
                .ok_or("The tutorial's source key must be visible")
        })
        .collect::<std::result::Result<_, _>>()?;
    let start = bounds[0].left_top() - egui::vec2(10.0, 5.0);
    let end = bounds[1].right_bottom() + egui::vec2(10.0, 5.0);
    let prior = s.state.animation.timeline.selection().cloned();
    // Leave the previous key's inspection tooltip before recording the next
    // interaction, so its values do not obscure the selection demonstration.
    s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
    s.wait(Duration::from_millis(300))?;
    s.capture_clip("animation-box-selection", ClipSpec::default(), |s| {
        s.callout(
            Control::AnimationTimeline,
            "Drag a box to inspect several keys.",
        )?;
        s.wait(Duration::from_millis(800))?;
        // The gesture cues use the viewport's lower-left corner too. Present
        // the instruction first, then leave the drag itself unobstructed.
        s.clear_callout()?;
        s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
        s.pointer_button(egui::PointerButton::Primary, true)?;
        s.move_pointer(end, Duration::from_millis(700))?;
        s.require(
            s.state.animation.timeline.is_marquee_active()
                && s.state.animation.timeline.selection() == prior.as_ref()
                && scene(s).playback == before.playback
                && Arc::ptr_eq(&scene(s).frame, &before.frame),
            "A held selection box preserves the previous selection and pose until release",
        )?;
        s.wait(Duration::from_millis(500))?;
        s.pointer_button(egui::PointerButton::Primary, false)?;
        s.require(
            s.state
                .animation
                .timeline
                .inspected_keys(&s.state.animation.data)
                .count()
                == 2
                && !s.state.animation.timeline.has_pointer_gesture(),
            "Releasing the box selects the two intersecting source keys",
        )?;
        s.wait(Duration::from_millis(750))?;
        s.callout(
            Control::AnimationTimeline,
            "Shift-drag adds to the inspected keys.",
        )?;
        s.wait(Duration::from_millis(800))?;
        s.clear_callout()?;
        s.modifiers_changed(egui::Modifiers {
            shift: true,
            ..Default::default()
        })?;
        let start = bounds[2].left_top() - egui::vec2(10.0, 5.0);
        let end = bounds[2].right_bottom() + egui::vec2(10.0, 5.0);
        s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
        s.pointer_button(egui::PointerButton::Primary, true)?;
        s.move_pointer(end, Duration::from_millis(500))?;
        s.pointer_button(egui::PointerButton::Primary, false)?;
        s.modifiers_changed(egui::Modifiers::NONE)?;
        s.require(
            s.state
                .animation
                .timeline
                .inspected_keys(&s.state.animation.data)
                .count()
                == 3,
            "Shift-box selection adds the third source key without replacing the first two",
        )?;
        s.wait(Duration::from_millis(750))?;
        s.callout(
            Control::AnimationTimeline,
            &format!("{} cancels an unfinished box.", s.shortcut_label("cancel")?),
        )?;
        s.wait(Duration::from_millis(800))?;
        s.clear_callout()?;
        let selected = s.state.animation.timeline.selection().cloned();
        s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
        s.pointer_button(egui::PointerButton::Primary, true)?;
        s.move_pointer(end, Duration::from_millis(450))?;
        s.shortcut("cancel")?;
        s.pointer_button(egui::PointerButton::Primary, false)?;
        s.require(
            s.state.animation.timeline.selection() == selected.as_ref()
                && !s.state.animation.timeline.has_pointer_gesture()
                && s.state.editor.selected_objects == object_selection
                && scene(s).playback == before.playback
                && Arc::ptr_eq(&scene(s).frame, &before.frame),
            "Escape cancels the box; the late release neither seeks nor clears the selected asset",
        )?;
        s.wait(Duration::from_millis(750))?;
        s.clear_callout()
    })
}

fn cancel_on_focus_loss(s: &mut Session<'_>) -> Result<()> {
    s.click(Control::ScenePlay)?;
    s.wait(Duration::from_millis(150))?;
    let before = scene(s).clone();
    begin_scrub(s, 0.75)?;
    s.require(
        s.state.animation.timeline.is_scrubbing() && !scene(s).playback.playing,
        "Starting a scrub pauses a previously playing clip",
    )?;
    s.frame(vec![egui::Event::WindowFocused(false)], Duration::ZERO)?;
    s.require(
        !s.state.animation.timeline.is_scrubbing()
            && scene(s).playback == before.playback
            && Arc::ptr_eq(&scene(s).frame, &before.frame),
        "Focus loss cancels a scrub and restores its exact time, pose, and playing state",
    )?;
    s.frame(vec![egui::Event::WindowFocused(true)], Duration::ZERO)?;
    // Losing focus clears the virtual/native pointer position. Re-enter the
    // window before delivering the late release of the cancelled gesture.
    let position = ruler_point(s, 0.75)?;
    s.frame(vec![egui::Event::PointerMoved(position)], Duration::ZERO)?;
    s.pointer_button(egui::PointerButton::Primary, false)?;
    s.click(Control::SceneRest)?;
    Ok(())
}

fn type_time(s: &mut Session<'_>) -> Result<()> {
    let rest = scene(s).frame.clone();
    s.click(Control::SceneTime)?;
    s.shortcut("selection.all")?;
    s.frame(vec![egui::Event::Text("1.25".into())], Duration::ZERO)?;
    s.shortcut("edit.confirm")?;
    s.require(
        scene(s).playback.clip == Some(0)
            && scene(s).playback.position == 1.25
            && !scene(s).playback.playing
            && moved(&rest, &scene(s).frame),
        "Typing a fractional time selects and evaluates the first clip from Rest while leaving playback paused",
    )?;
    s.click(Control::SceneRest)?;
    Ok(())
}

fn inspect_clip(s: &mut Session<'_>, fixture: &str, capture: &str, skinned: bool) -> Result<()> {
    s.load_scene_fixture(fixture)?;
    s.require(
        !s.state.animation_panel_is_open() && s.trace.get(Control::AnimationTimeline).is_err(),
        "Importing and selecting an animated asset does not open the animation panel",
    )?;
    // Return focus from the previous clip's numeric transport controls to the
    // viewport before demonstrating a viewport-owned shading shortcut.
    s.click(Control::Viewport)?;
    super::shading::select_mode(s, ShadingMode::MaterialPreview)?;
    if skinned {
        s.hover(Control::Viewport)?;
        s.shortcut("view.front")?;
        s.wait(Duration::from_millis(s.state.view_duration_ms.into()))?;
    }
    verify_panel_placement(s, false)?;
    if skinned {
        s.wait(crate::doc_input::CUE_LIFETIME)?;
        s.hover(Control::Viewport)?;
        s.capture_image("animation-panel-closed")?;
    }
    s.click(Control::AnimationPanelToggle)?;
    s.require(
        s.state.animation_panel_is_open() && s.trace.get(Control::AnimationTimeline).is_ok(),
        "Animation explicitly opens the panel below the same editor viewport",
    )?;
    verify_panel_placement(s, true)?;
    witness_controls(s)?;
    if skinned {
        s.wait(crate::doc_input::CUE_LIFETIME)?;
        s.hover(Control::Viewport)?;
        s.capture_image("animation-panel-open")?;
    }
    let asset = scene(s).asset.clone();
    let rest = scene(s).frame.clone();
    let document = s.state.editor.document.clone();
    let dirty = s.state.is_dirty();
    let camera = s.state.camera.view_projection(s.state.aspect());
    let playback_before_tab = scene(s).playback.clone();
    s.click(Control::AnimationPanelToggle)?;
    s.require(
        s.state.animation_panel_is_open()
            && scene(s).playback == playback_before_tab
            && Arc::ptr_eq(&rest, &scene(s).frame)
            && s.state.editor.document == document
            && camera == s.state.camera.view_projection(s.state.aspect()),
        "Activating the selected Animation tab keeps the panel open and focuses inspection without seeking, evaluating, or changing the document",
    )?;
    let clip = &asset.animations[0];
    let channel = &clip.channels[0];
    let projected_keys = s.state.animation.data.tracks.iter().find(|track| {
        track.keys.len() == channel.times.len()
            && track
                .keys
                .iter()
                .zip(&channel.times)
                .all(|(key, time)| (key.time - f64::from(*time - clip.start)).abs() < 1e-6)
    });
    s.require(
        asset.animations.len() == 1
            && scene(s).playback.clip.is_none()
            && !scene(s).playback.playing
            && projected_keys.is_some()
            && if skinned {
                channel.property == Property::Rotation
                    && asset.skins.len() == 1
                    && asset.skins[0].joints.contains(&channel.node)
            } else {
                channel.property == Property::Weights && channel.components == 2
            },
        "The timeline exposes the real clip's source keys in relative seconds while the viewport remains at Rest pose; joint rotation and vector morph weights retain their channel meaning",
    )?;
    inspect_navigation(s)?;
    if skinned {
        inspect_key(s)?;
        box_select_keys(s)?;
    }

    // Opening gives the timeline keyboard ownership, independently of the
    // numeric controls or Fit button used during the preceding inspection.
    s.click(Control::ToolDockClose)?;
    s.click(Control::AnimationPanelToggle)?;
    s.hover(Control::ScenePlay)?;
    // Instructions and physical-input cues share the viewport's lower-left
    // presentation area. Finish setup cues before narrating, then clear each
    // instruction before demonstrating its real input.
    s.wait(crate::doc_input::CUE_LIFETIME)?;
    s.capture_clip(capture, ClipSpec::default(), |s| {
        s.callout(
            Control::ScenePlay,
            &if skinned {
                format!("{} to play: joint rotation bends the skin.", s.shortcut_label("animation.play-pause")?)
            } else {
                format!("{} to play: morph weights change the cube's shape.", s.shortcut_label("animation.play-pause")?)
            },
        )?;
        s.wait(Duration::from_millis(1000))?;
        s.clear_callout()?;
        s.shortcut("animation.play-pause")?;
        s.wait(Duration::from_millis(1200))?;
        s.require(
            scene(s).playback.playing
                && scene(s).playback.clip == Some(0)
                && scene(s).playback.position > 0.7
                && moved(&rest, &scene(s).frame)
                && (!skinned || rest.node_world != scene(s).frame.node_world)
                && camera == s.state.camera.view_projection(s.state.aspect()),
            "Focused playback shortcut starts the first clip from Rest and changes evaluated geometry while keeping the viewport framing stable",
        )?;
        s.shortcut("animation.play-pause")?;
        let paused = scene(s).frame.clone();
        let time = scene(s).playback.position;
        s.wait(crate::doc_input::CUE_LIFETIME)?;
        s.require(
            !scene(s).playback.playing
                && scene(s).playback.position == time
                && Arc::ptr_eq(&paused, &scene(s).frame),
            "The same focused playback shortcut pauses and holds the accepted time and exact evaluated frame",
        )?;
        s.callout(Control::AnimationRuler, "Drag the ruler to inspect the pose.")?;
        s.wait(Duration::from_millis(700))?;
        s.clear_callout()?;
        begin_scrub(s, 0.25)?;
        let end = ruler_point(s, 0.68)?;
        s.move_pointer(end, Duration::from_millis(650))?;
        s.require(
            s.state.animation.timeline.is_scrubbing()
                && !scene(s).playback.playing
                && scene(s).playback.position != time
                && moved(&paused, &scene(s).frame),
            "The held ruler drag previews a changed pose through the production evaluator and remains paused",
        )?;
        s.pointer_button(egui::PointerButton::Primary, false)?;
        let accepted = scene(s).clone();
        s.wait(Duration::from_millis(800))?;
        s.require(
            !s.state.animation.timeline.is_scrubbing()
                && !scene(s).playback.playing
                && scene(s).playback == accepted.playback
                && Arc::ptr_eq(&accepted.frame, &scene(s).frame),
            "Releasing the scrub accepts and holds the inspected pose without resuming playback",
        )?;
        if skinned {
            s.callout(
                Control::AnimationRuler,
                &format!(
                    "Drag again; while holding, {} cancels the scrub.",
                    s.shortcut_label("cancel")?
                ),
            )?;
            s.wait(Duration::from_millis(800))?;
            s.clear_callout()?;
            begin_scrub(s, 0.8)?;
            let end = ruler_point(s, 0.12)?;
            s.move_pointer(end, Duration::from_millis(550))?;
            s.require(
                s.state.animation.timeline.is_scrubbing()
                    && moved(&accepted.frame, &scene(s).frame),
                "The second held scrub previews a different pose before cancellation",
            )?;
            s.wait(Duration::from_millis(200))?;
            s.shortcut("cancel")?;
            s.pointer_button(egui::PointerButton::Primary, false)?;
            s.require(
                !s.state.animation.timeline.is_scrubbing()
                    && scene(s).playback == accepted.playback
                    && Arc::ptr_eq(&accepted.frame, &scene(s).frame),
                "Escape restores the exact pre-scrub playback and pose; releasing afterward cannot commit a cancelled preview",
            )?;
            s.wait(Duration::from_millis(800))?;
        }
        s.callout(Control::SceneRest, "Return to the original Rest pose.")?;
        s.wait(Duration::from_millis(800))?;
        s.clear_callout()?;
        s.click(Control::SceneRest)?;
        s.require(
            !scene(s).playback.playing
                && scene(s).playback.clip.is_none()
                && scene(s).playback.position == 0.0
                && Arc::ptr_eq(&rest, &scene(s).frame),
            "Rest stops playback and restores the exact cached source pose",
        )?;
        s.wait(Duration::from_millis(800))?;
        s.clear_callout()
    })?;

    if skinned {
        cancel_on_focus_loss(s)?;
        type_time(s)?;
    }
    s.click(Control::SceneLoop)?;
    s.require(!scene(s).playback.looping, "Loop disables repeat playback")?;
    s.click(Control::SceneLoop)?;
    s.drag(Control::SceneSpeed, egui::vec2(35.0, 0.0))?;
    s.require(
        scene(s).playback.looping
            && scene(s).playback.speed != 1.0
            && Arc::ptr_eq(&rest, &scene(s).frame),
        "Loop and speed are preview controls that preserve the Rest-pose frame",
    )?;
    let no_history = !s.state.editor.undo();
    s.require(
        s.state.editor.document == document
            && s.state.is_dirty() == dirty
            && no_history
            && Arc::ptr_eq(&asset, &scene(s).asset),
        "Animation inspection, playback, scrubbing, cancellation, Fit, loop, and speed keep the authored document, dirty state, source data, and Undo history unchanged",
    )?;
    let playback = scene(s).playback.clone();
    let frame = scene(s).frame.clone();
    s.click(Control::ToolDockClose)?;
    s.require(
        !s.state.animation_panel_is_open()
            && s.trace.get(Control::AnimationTimeline).is_err()
            && scene(s).playback == playback
            && Arc::ptr_eq(&frame, &scene(s).frame),
        "Closing the idle panel preserves its accepted preview and leaves the ordinary viewport available",
    )?;
    verify_panel_placement(s, false)
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    inspect_clip(
        s,
        "SimpleSkin/glTF/SimpleSkin.gltf",
        "animation-skinning",
        true,
    )?;
    inspect_clip(
        s,
        "AnimatedMorphCube/glTF/AnimatedMorphCube.gltf",
        "animation-morphs",
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documentation::{Capture, HEIGHT, WIDTH};

    #[test]
    fn animation_guide_replays_tracks_deformation_and_scrub_recovery() {
        let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
        let mut session = Session::new(&mut capture).unwrap();
        run(&mut session).unwrap();
        assert_eq!(session.images.len(), 6);
        assert_eq!(session.animations.len(), 3);
        assert!(
            session
                .images
                .contains_key("assets/animation-inspection.webp")
        );
        assert!(
            session
                .images
                .contains_key("assets/animation-panel-closed.webp")
        );
        assert!(
            session
                .images
                .contains_key("assets/animation-panel-open.webp")
        );
        assert!(
            session
                .animations
                .contains("assets/animation-skinning.webp")
        );
        assert!(session.animations.contains("assets/animation-morphs.webp"));
        assert!(
            session
                .animations
                .contains("assets/animation-box-selection.webp")
        );
    }
}
