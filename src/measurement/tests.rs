use super::*;
use std::time::Duration;

fn instant_after(origin: Instant, millis: u64) -> Instant {
    origin + Duration::from_millis(millis)
}

fn renderer_probe_at(start: Instant, instrument: bool) -> FrameProbe {
    let mut probe = FrameProbe::start_at(instrument, start);
    probe.scene_render();
    probe.scene_composite();
    probe.scene_size([900, 600]);
    probe.surface_size([1280, 800]);
    probe
}

fn renderer_session(state: &mut WorkspaceUi, warmup: usize, samples: usize) -> Session {
    Session::new(
        Options {
            mode: ExecutionMode::Renderer,
            warmup_frames: warmup,
            sample_frames: samples,
            ..Options::default()
        },
        json!({}),
        state,
    )
    .unwrap()
}

fn fixture() -> WorkspaceUi {
    let mut state = WorkspaceUi::new(egui::TextureId::Managed(0));
    state
        .editor
        .insert(crate::document::PrimitiveKind::Cube)
        .unwrap();
    state.viewport = egui::Rect::from_min_size(egui::pos2(40.0, 30.0), egui::vec2(900.0, 600.0));
    state.viewport_ui_rect = state.viewport.shrink(10.0);
    state
}
#[test]
fn nearest_rank_percentiles_and_even_median_preserve_raw_order_independently() {
    let values = distribution([4.0, 1.0, 3.0, 2.0].into_iter());
    assert_eq!(values["median"], 2.5);
    assert_eq!(values["p95"], 4.0);
    assert_eq!(distribution(std::iter::empty()), Value::Null);
}

#[test]
fn summary_percentiles_use_nearest_rank_without_interpolating_tail_samples() {
    assert_eq!(
        distribution([42.0].into_iter()),
        json!({"count": 1, "median": 42.0, "p95": 42.0, "p99": 42.0, "min": 42.0, "max": 42.0})
    );
    assert_eq!(
        distribution((1..=101).rev().map(f64::from)),
        json!({"count": 101, "median": 51.0, "p95": 96.0, "p99": 100.0, "min": 1.0, "max": 101.0})
    );
}

#[test]
fn warmup_is_excluded_and_completion_retains_exactly_the_requested_samples() {
    let mut state = fixture();
    let mut session = renderer_session(&mut state, 2, 3);
    let origin = Instant::now();
    let mut report = None;
    for frame in 0..5 {
        let start = instant_after(origin, frame * 100);
        let probe = renderer_probe_at(start, true);
        // Large warm-up costs must not leak into retained frame distributions.
        let duration = if frame < 2 { 90 } else { frame + 1 };
        report = session.finish_frame_at(probe, &state, true, instant_after(start, duration));
        assert_eq!(report.is_some(), frame == 4);
        assert_eq!(session.active(), frame != 4);
    }
    let report = report.unwrap();
    assert!(
        report["validity"]
            .as_object()
            .unwrap()
            .values()
            .all(|value| value == true)
    );
    let samples = report["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 3);
    for (index, sample) in samples.iter().enumerate() {
        assert_eq!(sample["frame"], index);
        assert_eq!(sample["cpu_frame_ms"], (index + 3) as f64);
        // The first retained frame's cadence includes its warm-up predecessor.
        assert_eq!(sample["frame_start_interval_ms"], 100.0);
    }
    assert_eq!(report["summary"]["cpu_frame_ms"]["median"], 4.0);
    assert_eq!(report["summary"]["frame_start_interval_ms"]["count"], 3);
    let completed_camera = state.camera.view_projection(state.aspect());
    let inactive = session.begin_frame(&mut state);
    assert!(inactive.data.is_none());
    assert_eq!(
        state.camera.view_projection(state.aspect()),
        completed_camera
    );
    assert!(session.finish_frame(inactive, &state, true).is_none());
    assert_eq!(session.samples.len(), 3);
}

#[test]
fn zero_warmup_has_no_invented_first_interval_or_cadence_summary() {
    let state = &mut fixture();
    let mut session = renderer_session(state, 0, 1);
    let start = Instant::now();
    let report = session
        .finish_frame_at(
            renderer_probe_at(start, true),
            state,
            true,
            instant_after(start, 5),
        )
        .unwrap();
    assert_eq!(report["samples"][0]["cpu_frame_ms"], 5.0);
    assert!(report["samples"][0]["frame_start_interval_ms"].is_null());
    assert!(report["summary"]["frame_start_interval_ms"].is_null());
}

#[test]
fn fps_meter_baseline_and_samples_observe_the_initial_state_without_changing_it() {
    for enabled in [false, true] {
        let mut state = fixture();
        if enabled {
            state.fps_meter.toggle();
        }
        let mut session = renderer_session(&mut state, 1, 2);
        assert_eq!(state.fps_meter.enabled(), enabled);
        let origin = Instant::now();
        let mut report = None;
        for frame in 0..3 {
            let start = instant_after(origin, frame * 100);
            report = session.finish_frame_at(
                renderer_probe_at(start, true),
                &state,
                true,
                instant_after(start, 5),
            );
        }
        let report = report.unwrap();
        assert_eq!(report["render"]["fps_meter_enabled"], enabled);
        assert_eq!(report["validity"]["fps_meter_constant"], true);
        for sample in report["samples"].as_array().unwrap() {
            assert_eq!(sample["fps_meter_enabled"], enabled);
        }
    }
}

#[test]
fn fps_meter_toggle_during_retained_frames_is_contamination_not_a_new_baseline() {
    let mut state = fixture();
    let mut session = renderer_session(&mut state, 0, 2);
    let origin = Instant::now();
    assert!(
        session
            .finish_frame_at(
                renderer_probe_at(origin, true),
                &state,
                true,
                instant_after(origin, 5),
            )
            .is_none()
    );
    state.fps_meter.toggle();
    let report = session
        .finish_frame_at(
            renderer_probe_at(instant_after(origin, 100), true),
            &state,
            true,
            instant_after(origin, 105),
        )
        .unwrap();
    assert_eq!(report["render"]["fps_meter_enabled"], false);
    assert_eq!(report["samples"][0]["fps_meter_enabled"], false);
    assert_eq!(report["samples"][1]["fps_meter_enabled"], true);
    assert_eq!(report["validity"]["fps_meter_constant"], false);
}

#[test]
fn fps_meter_toggle_during_warmup_remains_contamination_after_restoring_baseline() {
    let mut state = fixture();
    state.fps_meter.toggle();
    let mut session = renderer_session(&mut state, 1, 1);
    state.fps_meter.toggle();
    let origin = Instant::now();
    assert!(
        session
            .finish_frame_at(
                renderer_probe_at(origin, true),
                &state,
                true,
                instant_after(origin, 5),
            )
            .is_none()
    );
    state.fps_meter.toggle();
    let report = session
        .finish_frame_at(
            renderer_probe_at(instant_after(origin, 100), true),
            &state,
            true,
            instant_after(origin, 105),
        )
        .unwrap();
    assert_eq!(report["render"]["fps_meter_enabled"], true);
    assert_eq!(report["samples"][0]["fps_meter_enabled"], true);
    assert_eq!(report["validity"]["fps_meter_constant"], false);
}

#[test]
fn retry_work_is_not_a_sample_but_its_elapsed_gap_remains_in_frame_cadence() {
    let mut state = fixture();
    let mut session = renderer_session(&mut state, 0, 2);
    let start = Instant::now();
    assert!(
        session
            .finish_frame_at(
                renderer_probe_at(start, true),
                &state,
                true,
                instant_after(start, 5)
            )
            .is_none()
    );
    let mut aborted = renderer_probe_at(instant_after(start, 100), true);
    aborted.mesh_upload(1234);
    geometry_rebuilt();
    drop(aborted);
    assert_eq!(session.samples.len(), 1);
    assert_eq!(session.frame, 1);
    assert!(!NESTED.with(|value| value.borrow().active));

    let report = session
        .finish_frame_at(
            renderer_probe_at(instant_after(start, 250), true),
            &state,
            true,
            instant_after(start, 257),
        )
        .unwrap();
    let sample = &report["samples"][1];
    assert_eq!(sample["cpu_frame_ms"], 7.0);
    assert_eq!(sample["frame_start_interval_ms"], 250.0);
    assert_eq!(sample["counters"]["mesh_uploads"], 0);
    assert_eq!(sample["projection"]["geometry_rebuilds"], 0);
    assert_eq!(report["summary"]["frame_start_interval_ms"]["count"], 1);
    assert_eq!(
        report["summary"]["frame_start_interval_ms"]["median"],
        250.0
    );
}

#[test]
fn stage_samples_distinguish_unexecuted_zero_duration_and_accumulated_work() {
    let mut state = fixture();
    let mut session = renderer_session(&mut state, 0, 1);
    let start = Instant::now();
    let mut probe = renderer_probe_at(start, true);
    let data = probe.data.as_mut().unwrap();
    data.end_at(Stage::Acquire, start);
    data.end_at(Stage::SceneEncode, instant_after(start, 3));
    data.end_at(Stage::Present, instant_after(start, 4));
    data.end_at(Stage::SceneEncode, instant_after(start, 7));
    let report = session
        .finish_frame_at(probe, &state, true, instant_after(start, 9))
        .unwrap();
    let stages = &report["samples"][0]["cpu_stage_ms"];
    let summaries = &report["summary"]["cpu_stage_ms"];
    assert_eq!(stages["surface_acquire"], 0.0);
    assert_eq!(stages["scene_encode"], 6.0);
    assert_eq!(stages["present_api"], 1.0);
    assert!(stages["ui"].is_null());
    assert_eq!(summaries["surface_acquire"]["count"], 1);
    assert_eq!(summaries["surface_acquire"]["median"], 0.0);
    assert_eq!(summaries["scene_encode"]["median"], 6.0);
    assert!(summaries["ui"].is_null());
    assert!(summaries["editor_projection"].is_null());
    // The enclosing CPU timer includes work after the last named stage.
    assert_eq!(report["samples"][0]["cpu_frame_ms"], 9.0);
}

#[test]
fn each_stage_enum_serializes_under_its_own_measurement_name() {
    let stages = [
        (Stage::Acquire, "surface_acquire"),
        (Stage::Ui, "ui"),
        (Stage::Commands, "commands_and_refresh"),
        (Stage::HostPrepare, "host_prepare"),
        (Stage::CacheSync, "cache_sync"),
        (Stage::Tessellate, "tessellation_and_textures"),
        (Stage::Feedback, "feedback"),
        (Stage::SceneEncode, "scene_encode"),
        (Stage::UiEncode, "ui_encode"),
        (Stage::Submit, "submit_api"),
        (Stage::Present, "present_api"),
        (Stage::HostTail, "host_tail"),
        (Stage::EditorPrepare, "editor_prepare"),
        (Stage::SceneComposite, "scene_composite"),
    ];
    for (stage, name) in stages {
        let mut state = fixture();
        let mut session = renderer_session(&mut state, 0, 1);
        let start = Instant::now();
        let mut probe = renderer_probe_at(start, true);
        probe
            .data
            .as_mut()
            .unwrap()
            .end_at(stage, instant_after(start, 3));
        let report = session
            .finish_frame_at(probe, &state, true, instant_after(start, 4))
            .unwrap();
        let stages = report["samples"][0]["cpu_stage_ms"].as_object().unwrap();
        assert_eq!(stages[name], 3.0, "{name}");
        assert_eq!(stages.values().filter(|value| !value.is_null()).count(), 1);
    }
}

#[test]
fn renderer_mode_rejects_every_kind_of_hidden_editor_or_ui_work() {
    type ProbeMutation = fn(&mut FrameProbe);
    let violations: [(&str, ProbeMutation); 9] = [
        ("egui pass", FrameProbe::egui_pass),
        ("egui tessellation", FrameProbe::egui_tessellate),
        ("egui texture upload", FrameProbe::egui_texture_update),
        ("egui composition", FrameProbe::egui_composite),
        ("ui jobs", |probe| probe.ui_jobs(1)),
        ("editor preparation", |_| editor_prepared()),
        ("editor feedback", FrameProbe::editor_feedback),
        ("projection", |_| projection_started().finish(8)),
        ("geometry preparation", |_| geometry_rebuilt()),
    ];
    for (name, violate) in violations {
        let mut state = fixture();
        let mut session = renderer_session(&mut state, 0, 1);
        let mut probe = renderer_probe_at(Instant::now(), false);
        violate(&mut probe);
        let report = session.finish_frame(probe, &state, true).unwrap();
        assert_eq!(
            report["validity"]["mode_contract_satisfied"], false,
            "{name}"
        );
        assert!(report["samples"][0]["cpu_stage_ms"].is_null());
    }
}

#[test]
fn every_mode_requires_exactly_one_nonempty_scene_render() {
    for mode in [
        ExecutionMode::Editor,
        ExecutionMode::Viewport,
        ExecutionMode::Renderer,
    ] {
        for violation in [
            "none",
            "missing_scene",
            "duplicate_scene",
            "empty_scene",
            "unknown_scene",
        ] {
            let mut state = fixture();
            let mut session = Session::new(
                Options {
                    mode,
                    warmup_frames: 0,
                    sample_frames: 1,
                    ..Options::default()
                },
                json!({}),
                &mut state,
            )
            .unwrap();
            let mut probe = session.begin_frame(&mut state);
            if mode == ExecutionMode::Editor {
                probe.egui_pass();
                probe.egui_tessellate();
                probe.egui_composite();
            } else {
                probe.scene_composite();
            }
            if mode != ExecutionMode::Renderer {
                editor_prepared();
                probe.editor_feedback();
            }
            if violation != "missing_scene" {
                probe.scene_render();
            }
            if violation == "duplicate_scene" {
                probe.scene_render();
            }
            if violation != "unknown_scene" {
                probe.scene_size(if violation == "empty_scene" {
                    [0, 600]
                } else {
                    [900, 600]
                });
            }
            let report = session.finish_frame(probe, &state, true).unwrap();
            assert_eq!(
                report["validity"]["mode_contract_satisfied"],
                violation == "none",
                "{mode:?}: {violation}"
            );
        }
    }
}

#[test]
fn contaminated_samples_are_retained_and_flagged_instead_of_filtered_out() {
    for contamination in ["focus", "error", "revision", "viewport"] {
        let mut state = fixture();
        let mut session = renderer_session(&mut state, 0, 2);
        let start = Instant::now();
        assert!(
            session
                .finish_frame_at(
                    renderer_probe_at(start, true),
                    &state,
                    true,
                    instant_after(start, 5),
                )
                .is_none()
        );
        match contamination {
            "error" => state.error = Some("test frame failed".into()),
            "revision" => state.mesh_revision += 1,
            "viewport" => state.viewport.max.x += 10.0,
            _ => {}
        }
        let report = session
            .finish_frame_at(
                renderer_probe_at(instant_after(start, 100), true),
                &state,
                contamination != "focus",
                instant_after(start, 150),
            )
            .unwrap();
        assert_eq!(report["samples"].as_array().unwrap().len(), 2);
        assert_eq!(report["samples"][1]["cpu_frame_ms"], 50.0);
        assert_eq!(report["summary"]["cpu_frame_ms"]["count"], 2);
        assert_eq!(report["summary"]["cpu_frame_ms"]["max"], 50.0);
        let invalid_field = match contamination {
            "focus" => "all_frames_focused",
            "error" => "no_frame_errors",
            "revision" => "unchanged_mesh_revision",
            "viewport" => "viewport_matches_baseline",
            _ => unreachable!(),
        };
        assert_eq!(report["validity"][invalid_field], false, "{contamination}");
    }
}

#[test]
fn serialized_report_matches_the_shared_python_admission_contract() {
    let contract: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tools/benchmark/tests/fixtures/measurement-schema.json"
    )))
    .unwrap();
    let keys = |value: &Value| {
        let mut names: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
        names.sort();
        json!(names)
    };
    for instrument in [true, false] {
        let mut state = fixture();
        let mut session = renderer_session(&mut state, 0, 1);
        session.options.instrument_stages = instrument;
        let start = Instant::now();
        let report = session
            .finish_frame_at(
                renderer_probe_at(start, instrument),
                &state,
                true,
                instant_after(start, 5),
            )
            .unwrap();
        assert_eq!(report["schema"], contract["schema"]);
        assert_eq!(keys(&report["validity"]), contract["validity_fields"]);
        let sample = &report["samples"][0];
        assert_eq!(keys(sample), contract["sample_fields"]);
        assert_eq!(keys(&sample["counters"]), contract["counter_names"]);
        if instrument {
            assert_eq!(keys(&sample["cpu_stage_ms"]), contract["stage_names"]);
        } else {
            assert!(sample["cpu_stage_ms"].is_null());
        }
    }
}
#[test]
fn options_bound_retained_samples_and_reject_unknown_fields() {
    assert!(
        Options {
            sample_frames: 0,
            ..Options::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        Options {
            sample_frames: 10_001,
            ..Options::default()
        }
        .validate()
        .is_err()
    );
    assert!(serde_json::from_str::<Options>(r#"{"fake":true}"#).is_err());
    let options: Options = serde_json::from_str("{}").unwrap();
    assert_eq!(options.mode, ExecutionMode::Editor);
    assert!(
        Options {
            mode: ExecutionMode::Renderer,
            selected: true,
            ..options
        }
        .validate()
        .is_err()
    );
    assert!(serde_json::from_str::<Options>(r#"{"mode":"hidden"}"#).is_err());
}

#[test]
fn all_modes_share_scene_area_and_camera_without_reusing_a_hidden_ui_layout() {
    let mut poses = Vec::new();
    for mode in [
        ExecutionMode::Editor,
        ExecutionMode::Viewport,
        ExecutionMode::Renderer,
    ] {
        let mut state = fixture();
        let viewport = state.viewport;
        let usable = state.viewport_ui_rect;
        let mut session = Session::new(
            Options {
                mode,
                ..Options::default()
            },
            json!({}),
            &mut state,
        )
        .unwrap();
        if mode != ExecutionMode::Editor {
            // A non-UI frame must use the captured layout, never a full
            // window placeholder or a stale scene rect supplied by a host.
            state.viewport = egui::Rect::NOTHING;
            state.viewport_ui_rect = egui::Rect::NOTHING;
        }
        let probe = session.begin_frame(&mut state);
        assert_eq!(state.viewport, viewport);
        assert_eq!(state.viewport_ui_rect, usable);
        poses.push(state.camera.view_projection(state.aspect()));
        assert!(state.show_ui, "measurement modes are not Hide UI");
        drop(probe);
    }
    assert!(poses.iter().all(|pose| pose == &poses[0]));
    let mut state = fixture();
    state.show_ui = false;
    assert!(Session::new(Options::default(), json!({}), &mut state).is_err());
    state.show_ui = true;
    state.show_preferences = true;
    assert!(Session::new(Options::default(), json!({}), &mut state).is_err());
}

#[test]
fn untimed_viewport_keeps_execution_and_projection_evidence() {
    let mut state = fixture();
    let mut session = Session::new(
        Options {
            mode: ExecutionMode::Viewport,
            warmup_frames: 0,
            sample_frames: 1,
            instrument_stages: false,
            ..Options::default()
        },
        json!({}),
        &mut state,
    )
    .unwrap();
    let mut probe = session.begin_frame(&mut state);
    session.prepare_without_ui(&mut state, &mut probe);
    // Exercise actual editor preparation above; signal the renderer calls
    // here without requiring a GPU in the recorder's unit tests.
    probe.editor_feedback();
    probe.scene_render();
    probe.scene_composite();
    probe.scene_size([1800, 1200]);
    probe.pixel_scale(2.0);
    let report = session.finish_frame(probe, &state, true).unwrap();
    let sample = &report["samples"][0];
    assert_eq!(report["schema"], "n3.viewport-measure.v2");
    assert_eq!(report["validity"]["mode_contract_satisfied"], true);
    assert_eq!(sample["counters"]["egui_passes"], 0);
    assert_eq!(sample["counters"]["editor_prepares"], 1);
    assert_eq!(sample["projection"]["rebuilds"], 1);
    assert_eq!(sample["projection"]["vertices_tested"], 8);
    assert!(sample["projection"]["cpu_ms"].is_null());
    assert!(sample["cpu_stage_ms"].is_null());
    assert_eq!(sample["viewport_pixels"], json!([1800, 1200]));
    assert_eq!(sample["viewport_points"], json!([40.0, 30.0, 940.0, 630.0]));
}

#[test]
fn renderer_contract_detects_ui_editor_and_missing_scene_execution() {
    for violation in ["none", "egui", "editor", "feedback", "missing_scene"] {
        let mut state = fixture();
        let mut session = Session::new(
            Options {
                mode: ExecutionMode::Renderer,
                warmup_frames: 0,
                sample_frames: 1,
                ..Options::default()
            },
            json!({}),
            &mut state,
        )
        .unwrap();
        let mut probe = session.begin_frame(&mut state);
        session.prepare_without_ui(&mut state, &mut probe);
        match violation {
            "egui" => probe.egui_pass(),
            "editor" => state
                .editor
                .prepare_viewport(state.viewport, &state.camera, state.z_up)
                .unwrap(),
            "feedback" => probe.editor_feedback(),
            _ => {}
        }
        if violation != "missing_scene" {
            probe.scene_render();
            probe.scene_composite();
            probe.scene_size([900, 600]);
        }
        let report = session.finish_frame(probe, &state, true).unwrap();
        assert_eq!(
            report["validity"]["mode_contract_satisfied"],
            violation == "none",
            "{violation}"
        );
        assert!(report["samples"][0]["cpu_stage_ms"]["ui"].is_null());
        assert!(report["samples"][0]["cpu_stage_ms"]["editor_prepare"].is_null());
        if violation != "editor" {
            assert!(report["summary"]["cpu_stage_ms"]["editor_projection"].is_null());
        }
    }
}

#[test]
fn frozen_scene_rect_does_not_hide_surface_or_scale_changes() {
    for change in ["none", "surface", "scale"] {
        let mut state = fixture();
        let mut session = Session::new(
            Options {
                mode: ExecutionMode::Renderer,
                workload: Workload::Stationary,
                warmup_frames: 0,
                sample_frames: 2,
                instrument_stages: false,
                ..Options::default()
            },
            json!({}),
            &mut state,
        )
        .unwrap();
        let mut report = None;
        for frame in 0..2 {
            let mut probe = session.begin_frame(&mut state);
            probe.scene_render();
            probe.scene_composite();
            probe.scene_size([900, 600]);
            probe.surface_size(if change == "surface" && frame == 1 {
                [1300, 800]
            } else {
                [1280, 800]
            });
            probe.pixel_scale(if change == "scale" && frame == 1 {
                2.0
            } else {
                1.0
            });
            report = session.finish_frame(probe, &state, true);
        }
        let report = report.unwrap();
        assert_eq!(report["validity"]["mode_contract_satisfied"], true);
        assert_eq!(report["validity"]["viewport_matches_baseline"], true);
        assert_eq!(report["validity"]["constant_surface"], change != "surface");
        assert_eq!(report["validity"]["constant_viewport"], change != "scale");
    }
}

#[test]
fn interrupted_frame_retries_the_same_camera_pose_and_releases_nested_probes() {
    let mut state = WorkspaceUi::new(egui::TextureId::Managed(0));
    state
        .editor
        .insert(crate::document::PrimitiveKind::Cube)
        .unwrap();
    state.viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 600.0));
    let mut session = Session::new(Options::default(), json!({}), &mut state).unwrap();
    let probe = session.begin_frame(&mut state);
    let pose = state.camera.view_projection(state.aspect());
    drop(probe);
    assert!(!NESTED.with(|value| value.borrow().active));
    let retry = session.begin_frame(&mut state);
    assert_eq!(pose, state.camera.view_projection(state.aspect()));
    assert!(session.finish_frame(retry, &state, true).is_none());
    let next = session.begin_frame(&mut state);
    assert_ne!(pose, state.camera.view_projection(state.aspect()));
    drop(next);
}

#[test]
fn projection_counters_distinguish_geometry_builds_camera_changes_and_cache_hits() {
    let mut document = crate::document::Document::default();
    document
        .insert_primitive(crate::document::PrimitiveKind::Cube)
        .unwrap();
    let mut editor = crate::editor::Editor::new(document).unwrap();
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 600.0));
    let mut camera = Camera::default();
    let frame = FrameProbe::start(true);
    editor
        .object_selection_bounds(viewport, &camera, false)
        .unwrap();
    editor
        .object_selection_bounds(viewport, &camera, false)
        .unwrap();
    NESTED.with(|value| {
        let value = value.borrow();
        assert_eq!(value.stats.geometry_rebuilds, 1);
        assert_eq!(value.stats.rebuilds, 1);
        assert_eq!(value.stats.vertices_tested, 8);
    });
    drop(frame);
    let frame = FrameProbe::start(true);
    camera.orbit(1.0, 0.0);
    editor
        .object_selection_bounds(viewport, &camera, false)
        .unwrap();
    NESTED.with(|value| {
        let value = value.borrow();
        assert_eq!(value.stats.geometry_rebuilds, 0);
        assert_eq!(value.stats.rebuilds, 1);
    });
    drop(frame);
}
