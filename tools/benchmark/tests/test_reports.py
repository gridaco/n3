from copy import deepcopy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from tools.benchmark import reports as report
from tools.benchmark import contracts


def raw_report(values, *, run=1, host="native", mode="editor", stages=True, fps_meter=False):
    """Small raw fixture, with deliberately wrong precomputed summaries."""
    result = {
        "schema": "n3.viewport-measure.v2",
        "metadata": {
            "host": host, "adapter": {"name": "test GPU", "backend": "Metal", "device_type": "IntegratedGpu",
                                       "vendor": 0, "device": 0, "driver": "", "driver_info": ""},
            "architecture": "arm64", "browser_mode": None, "build_command": ["cargo", "build", "--release"],
            "cargo_encoded_rustflags": None, "dirty": True,
            "input": {"name": "test.glb", "bytes": 100, "sha256": "f" * 64},
            "operating_system": "test OS", "profile": "release", "profile_overrides": {},
            "recorded_at_utc": f"2026-10-02T00:00:{run:02d}Z", "requested_device_scale_factor": None,
            "requested_physical_size": [1280, 800], "revision": "test revision", "rustc": "test compiler",
            "rustflags": None, "source_fingerprint": {"sha256": "a" * 64, "paths": 10, "scope": "source"},
            "surface": {"width": 1280, "height": 800, "present_mode": "Fifo", "scale_factor": 2,
                        "format": "Bgra8Unorm", "alpha_mode": "Opaque", "desired_maximum_frame_latency": 2},
            "loading": {"duration": run * 100},
            "suite": {"round": run, "position": run, "seed": 17},
        },
        "options": {"mode": mode, "workload": "orbit", "selected": False, "instrument_stages": stages,
                    "sample_frames": len(values), "warmup_frames": 20},
        "model": {"vertices": 100, "triangles": 150, "faces": 0, "objects": 1, "linked_assets": 1},
        "render": {"baseline_viewport_points": [240, 0, 400, 368.5], "pixels_per_point": 2,
                   "initial_camera_view_projection": [0] * 16, "shading": "solid", "grid": True, "edges": True,
                   "fps_meter_enabled": fps_meter, "theme": "light", "selected_objects": 0, "hovered_object": None,
                   "baseline_viewport_ui_points": [240, 0, 400, 368.5]},
        "validity": {
            "all_frames_focused": True, "constant_surface": True, "constant_viewport": True,
            "mode_contract_satisfied": True, "no_frame_errors": True, "unchanged_mesh_revision": True,
            "viewport_matches_baseline": True,
            "fps_meter_constant": True,
        },
        "samples": [], "summary": {"cpu_frame_ms": {"median": 99999999}},
    }
    if host == "web":
        result["metadata"].update(browser_mode="headed", requested_device_scale_factor=2, browser={
            "user_agent": "test browser", "device_pixel_ratio": 2, "physical_canvas": [1280, 800],
            "css_canvas": [640, 400], "visibility": "visible", "automation": {
                "browser_version": "test version", "mode": "headed", "tool": "playwright", "version": "pinned",
                "launch_arguments": ["--user-data-dir=[temporary-profile]"],
                "device_scale_factor": 2,
                "gpu_process": {"software_renderer_detected": False},
            },
        })
        result["browser_observations"] = {"comparable": True, "focus_losses": 0, "visibility_changes": 0, "size_changed": False}
    for index, value in enumerate(values):
        stage_values = {
            "surface_acquire": 0, "host_prepare": 0, "cache_sync": 0,
            "scene_encode": 0, "submit_api": 0, "present_api": 0, "host_tail": 0,
            "ui": None, "commands_and_refresh": None, "tessellation_and_textures": None,
            "feedback": None, "ui_encode": None, "editor_prepare": None, "scene_composite": None,
        }
        counters = {
            "mesh_uploads": 0, "base_mesh_upload_bytes": 0, "viewport_resizes": 0,
            "ui_jobs": 0, "egui_passes": 0, "egui_tessellations": 0, "egui_texture_updates": 0,
            "egui_composites": 0, "editor_prepares": 0, "editor_feedback_updates": 0,
            "scene_renders": 1, "scene_composites": 0,
        }
        stage_values["scene_encode"] = value / 10
        if mode == "editor":
            stage_values.update(ui=value / 2, commands_and_refresh=0, tessellation_and_textures=0, feedback=0, ui_encode=0)
            counters.update(ui_jobs=1, egui_passes=1, egui_tessellations=1, egui_composites=1,
                            editor_prepares=1, editor_feedback_updates=1)
        elif mode == "viewport":
            stage_values.update(editor_prepare=value / 2, feedback=0, scene_composite=0)
            counters.update(editor_prepares=1, editor_feedback_updates=1, scene_composites=1)
        else:
            stage_values["scene_composite"] = 0
            counters["scene_composites"] = 1
        result["samples"].append({
            "frame": index, "cpu_frame_ms": value, "frame_start_interval_ms": value + 1,
            "camera_view_projection": [index + 1] * 16,
            "counters": counters, "cpu_stage_ms": stage_values if stages else None,
            "error": None, "focused": True, "mesh_revision": 1, "pixels_per_point": 2,
            "fps_meter_enabled": fps_meter,
            "projection": {"cpu_ms": value / 4 if stages and mode != "renderer" else None,
                           "geometry_rebuilds": 0, "rebuilds": 0, "vertices_tested": 0},
            "surface_pixels": [1280, 800], "viewport_pixels": [320, 737], "viewport_points": [240, 0, 400, 368.5],
        })
    return result


class ViewportReportTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.counter = 0

    def save(self, value):
        self.counter += 1
        raw_directory = self.directory / "raw"
        raw_directory.mkdir(exist_ok=True)
        path = raw_directory / f"raw-{self.counter}.json"
        path.write_text(json.dumps(value), encoding="utf-8")
        if "suite" in value.get("metadata", {}):
            manifest_path = self.directory / "manifest.json"
            manifest = json.loads(manifest_path.read_text()) if manifest_path.exists() else {
                "schema": "n3.viewport-suite-manifest.v1", "runs": [],
            }
            manifest["runs"].append({"report": path.relative_to(self.directory).as_posix(), "status": "complete",
                                     "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        return path

    def analyze(self, *values):
        return report.analyze([self.save(value) for value in values])

    def test_shared_wire_fixture_matches_admitted_schema_and_independent_raw_fixture(self):
        # Rust checks these literal names against its actual serialized output.
        # This consumer check catches either language silently changing the wire.
        wire = json.loads((Path(__file__).parent / "fixtures/measurement-schema.json").read_text())
        value = raw_report([1, 2])
        sample = value["samples"][0]
        self.assertEqual(value["schema"], wire["schema"])
        self.assertEqual(sorted(sample), wire["sample_fields"])
        self.assertEqual(sorted(sample["counters"]), wire["counter_names"])
        self.assertEqual(sorted(sample["cpu_stage_ms"]), wire["stage_names"])
        self.assertEqual(sorted(value["validity"]), wire["validity_fields"])
        self.assertEqual(sorted(contracts.COUNTERS), wire["counter_names"])
        self.assertEqual(sorted(set(contracts.STAGES) - {"editor_projection"}), wire["stage_names"])
        self.assertEqual(sorted(contracts.VALIDITY_FIELDS), wire["validity_fields"])
        contracts.validate_raw(value)

    def test_each_mode_requires_actual_execution_even_with_successful_summary(self):
        corruptions = {
            "editor": [("scene_renders", 0), ("egui_passes", 0), ("egui_tessellations", 0),
                       ("egui_composites", 0), ("editor_prepares", 0), ("editor_feedback_updates", 0),
                       ("scene_composites", 1)],
            "viewport": [("scene_renders", 0), ("scene_composites", 0), ("editor_prepares", 0),
                         ("editor_feedback_updates", 0), ("egui_passes", 1), ("egui_tessellations", 1),
                         ("egui_texture_updates", 1), ("egui_composites", 1), ("ui_jobs", 1)],
            "renderer": [("scene_renders", 2), ("scene_composites", 0), ("editor_prepares", 1),
                         ("editor_feedback_updates", 1), ("egui_passes", 1), ("egui_tessellations", 1),
                         ("egui_texture_updates", 1), ("egui_composites", 1), ("ui_jobs", 1)],
        }
        for mode, cases in corruptions.items():
            for name, value in cases:
                for stages in (True, False):
                    with self.subTest(mode=mode, counter=name, stages=stages):
                        raw = raw_report([1, 2], mode=mode, stages=stages)
                        raw["samples"][1]["counters"][name] = value
                        summary = self.analyze(raw)
                        self.assertEqual(summary["accepted_run_count"], 0)
                        self.assertTrue(any("execution counters" in reason for reason in summary["excluded_runs"][0]["reasons"]))

    def test_incomplete_or_unknown_counter_schema_is_malformed(self):
        for counter in ("scene_renders", "egui_passes", "ui_jobs"):
            with self.subTest(counter=counter):
                value = raw_report([1, 2])
                del value["samples"][0]["counters"][counter]
                with self.assertRaisesRegex(ValueError, "execution counters"):
                    self.analyze(value)
        value = raw_report([1, 2])
        value["samples"][0]["counters"]["future_counter"] = 0
        with self.assertRaisesRegex(ValueError, "execution counters"):
            self.analyze(value)

    def test_mode_requires_honest_stage_availability_and_renderer_excludes_projection(self):
        cases = (("renderer", "ui", 0), ("editor", "scene_composite", 0), ("editor", "ui", None),
                 ("viewport", "editor_prepare", None), ("renderer", "scene_encode", None))
        for mode, stage, duration in cases:
            with self.subTest(mode=mode, stage=stage):
                value = raw_report([1, 2], mode=mode)
                value["samples"][0]["cpu_stage_ms"][stage] = duration
                summary = self.analyze(value)
                self.assertEqual(summary["accepted_run_count"], 0)
                self.assertTrue(any("stage availability" in reason for reason in summary["excluded_runs"][0]["reasons"]))
        for name in ("rebuilds", "geometry_rebuilds", "vertices_tested", "cpu_ms"):
            with self.subTest(projection=name):
                value = raw_report([1, 2], mode="renderer")
                value["samples"][0]["projection"][name] = 1
                self.assertEqual(self.analyze(value)["accepted_run_count"], 0)

    def test_browser_observation_facts_override_comparable_flag(self):
        for name, observed in (("focus_losses", 1), ("visibility_changes", 1), ("size_changed", True)):
            with self.subTest(name=name):
                value = raw_report([1, 2], host="web")
                value["browser_observations"][name] = observed
                self.assertTrue(value["browser_observations"]["comparable"])
                summary = self.analyze(value)
                self.assertEqual(summary["accepted_run_count"], 0)
                self.assertTrue(any("raw browser observations" in reason for reason in summary["excluded_runs"][0]["reasons"]))
        for name, observed in (("focus_losses", -1), ("visibility_changes", True), ("size_changed", 0)):
            with self.subTest(malformed=name):
                value = raw_report([1, 2], host="web")
                value["browser_observations"][name] = observed
                with self.assertRaisesRegex(ValueError, "malformed browser_observations"):
                    self.analyze(value)

    def test_requested_and_observed_environment_must_agree(self):
        mutations = [
            lambda value: value["metadata"].update(requested_physical_size=[1920, 1080]),
            lambda value: value["metadata"]["surface"].update(width=1920),
            lambda value: value["render"].update(pixels_per_point=1),
            lambda value: value["metadata"]["browser"].update(physical_canvas=[1920, 1080]),
            lambda value: value["metadata"]["browser"]["automation"].update(mode="headless"),
        ]
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                value = raw_report([1, 2], host="web")
                mutation(value)
                self.assertEqual(self.analyze(value)["accepted_run_count"], 0)

    def test_hardware_and_workload_fields_cannot_be_missing_or_malformed(self):
        fields = [
            ("adapter", ("metadata", "adapter"), "backend", ""),
            ("adapter name", ("metadata", "adapter"), "name", None),
            ("adapter vendor", ("metadata", "adapter"), "vendor", True),
            ("surface format", ("metadata", "surface"), "format", ""),
            ("surface scale", ("metadata", "surface"), "scale_factor", 0),
            ("model objects", ("model",), "objects", 0),
            ("model vertices", ("model",), "vertices", None),
            ("render shading", ("render",), "shading", ""),
            ("render grid", ("render",), "grid", 1),
            ("render selection", ("render",), "selected_objects", -1),
            ("browser version", ("metadata", "browser", "automation"), "browser_version", ""),
            ("requested scale", ("metadata",), "requested_device_scale_factor", None),
        ]
        for label, parents, field, invalid in fields:
            for missing in (True, False):
                with self.subTest(field=label, missing=missing):
                    value = raw_report([1, 2], host="web")
                    container = value
                    for parent in parents:
                        container = container[parent]
                    if missing:
                        container.pop(field)
                    else:
                        container[field] = invalid
                    with self.assertRaises(ValueError):
                        self.analyze(value)

    def test_observed_geometry_and_selection_override_successful_validity_flags(self):
        mutations = [
            ("viewport pixels", lambda value: [sample.update(viewport_pixels=[8192, 8192]) for sample in value["samples"]]),
            ("logical viewport", lambda value: [sample.update(viewport_pixels=[321, 737]) for sample in value["samples"]]),
            ("CSS canvas", lambda value: value["metadata"]["browser"].update(css_canvas=[320, 200])),
            ("overflowing CSS canvas", lambda value: value["metadata"]["browser"].update(css_canvas=[1e308, 1e308])),
            ("device pixel ratio", lambda value: value["metadata"]["browser"].update(device_pixel_ratio=4)),
            ("requested scale", lambda value: value["metadata"].update(requested_device_scale_factor=1)),
            ("automation scale", lambda value: value["metadata"]["browser"]["automation"].update(device_scale_factor=1)),
            ("surface scale", lambda value: value["metadata"]["surface"].update(scale_factor=1)),
            ("selected workload", lambda value: value["render"].update(selected_objects=1)),
            ("document objects", lambda value: value["model"].update(linked_assets=2)),
        ]
        for label, mutation in mutations:
            with self.subTest(observation=label):
                value = raw_report([1, 2], host="web")
                mutation(value)
                self.assertTrue(all(value["validity"].values()))
                self.assertEqual(self.analyze(value)["accepted_run_count"], 0)

    def test_scene_extent_uses_f32_rounding_and_allows_device_clamping(self):
        value = raw_report([1, 2])
        # This decimal rounds to 100.25 in f32, then 200.5 physical pixels
        # rounds up to 201 in Rust. Python's round-to-even would be wrong.
        bounds = [0, 0, 100.249999, 100]
        value["render"]["baseline_viewport_points"] = bounds
        value["render"]["baseline_viewport_ui_points"] = bounds
        for sample in value["samples"]:
            sample.update(viewport_points=bounds, viewport_pixels=[201, 200])
        self.assertEqual(self.analyze(value)["accepted_run_count"], 1)
        for sample in value["samples"]:
            sample["viewport_pixels"] = [128, 128]
        self.assertEqual(self.analyze(value)["accepted_run_count"], 1)
        for sample in value["samples"]:
            sample["viewport_pixels"] = [202, 200]
        self.assertEqual(self.analyze(value)["accepted_run_count"], 0)

    def test_fractional_css_pixels_and_f32_scale_remain_valid(self):
        value = raw_report([1, 2], host="web")
        scale = 1.2999999523162842
        value["metadata"]["requested_device_scale_factor"] = 1.3
        value["metadata"]["surface"]["scale_factor"] = scale
        value["metadata"]["browser"].update(device_pixel_ratio=1.3, css_canvas=[984.609375, 615.375])
        value["metadata"]["browser"]["automation"]["device_scale_factor"] = 1.3
        value["render"]["pixels_per_point"] = scale
        for sample in value["samples"]:
            sample.update(pixels_per_point=scale, viewport_pixels=[208, 479])
        self.assertEqual(self.analyze(value)["accepted_run_count"], 1)

    def test_browser_adapter_can_explicitly_withhold_identifying_strings(self):
        value = raw_report([1, 2], host="web")
        value["metadata"]["adapter"].update(name="", backend="BrowserWebGpu", device_type="Other")
        self.assertEqual(self.analyze(value)["accepted_run_count"], 1)

    def test_camera_workload_claim_must_match_observed_changes(self):
        value = raw_report([1, 2])
        value["options"]["workload"] = "stationary"
        self.assertEqual(self.analyze(value)["accepted_run_count"], 0)
        for sample in value["samples"]:
            sample["camera_view_projection"] = value["render"]["initial_camera_view_projection"]
        self.assertEqual(self.analyze(value)["accepted_run_count"], 1)
        value["options"]["workload"] = "orbit"
        self.assertEqual(self.analyze(value)["accepted_run_count"], 0)

    def test_manual_browser_without_automation_has_no_false_hardware_claim(self):
        value = raw_report([1, 2], host="web")
        value["metadata"]["browser_mode"] = "manual"
        value["metadata"]["requested_device_scale_factor"] = None
        value["metadata"]["browser"]["automation"] = None
        self.assertEqual(self.analyze(value)["accepted_run_count"], 1)

    def test_malformed_browser_diagnostics_cannot_bypass_admission(self):
        for diagnostics in ("unknown", 0, {"software_renderer_detected": "false"}):
            with self.subTest(diagnostics=diagnostics):
                value = raw_report([1, 2], host="web")
                value["metadata"]["browser"]["automation"]["gpu_process"] = diagnostics
                with self.assertRaisesRegex(ValueError, "gpu_process"):
                    self.analyze(value)
        value = raw_report([1, 2], host="web")
        value["metadata"]["browser_mode"] = "manual"
        value["metadata"]["requested_device_scale_factor"] = None
        value["metadata"]["browser"]["automation"] = 42
        with self.assertRaisesRegex(ValueError, "automation"):
            self.analyze(value)

    def test_independent_run_medians_recomputed_without_pooling_or_outlier_removal(self):
        summary = self.analyze(*[raw_report([index, index, 100], run=index) for index in (1, 2, 3)])
        case = summary["cases"][0]
        metric = case["metrics"]["cpu_frame_ms"]
        self.assertEqual(metric["run_medians"], {"median": 2, "min": 1, "max": 3, "mad": 1, "relative_mad_percent": 50})
        # Pooling these nine frames would report 3 rather than the independent-run median 2.
        self.assertEqual(metric["run_p95"], {"median": 100, "min": 100, "max": 100})
        self.assertEqual(metric["run_max"]["max"], 100)
        self.assertEqual(case["run_count"], 3)
        self.assertEqual(case["frame_count"], 9)
        self.assertEqual(case["warnings"], [])
        self.assertEqual(case["runs"][1]["suite"]["round"], 2)
        self.assertNotIn("suite", case["configuration"]["metadata"])
        self.assertNotIn("loading", case["configuration"]["metadata"])
        self.assertIn("100.000", report.render_text(summary))

    def test_even_median_and_nearest_rank_p95_are_not_interpolated(self):
        summary = self.analyze(raw_report(list(range(1, 21))))
        metric = summary["cases"][0]["runs"][0]["metrics"]["cpu_frame_ms"]
        self.assertEqual(metric, {"count": 20, "min": 1, "median": 10.5, "p95": 19, "max": 20})
        self.assertIn("Insufficient repeats", summary["cases"][0]["warnings"][0])

    def test_host_and_mode_cases_remain_separate(self):
        summary = self.analyze(raw_report([1, 2]), raw_report([3, 4], host="web"), raw_report([5, 6], mode="viewport"))
        self.assertEqual(len(summary["cases"]), 3)
        self.assertEqual(summary["accepted_run_count"], 3)

    def test_fps_meter_observations_cannot_be_overridden_by_true_constant_flag(self):
        for initial in (False, True):
            for observed_frames in ((0,), (0, 1)):
                with self.subTest(initial=initial, observed_frames=observed_frames):
                    value = raw_report([1, 2], fps_meter=initial)
                    for index in observed_frames:
                        value["samples"][index]["fps_meter_enabled"] = not initial
                    summary = self.analyze(value)
                    self.assertEqual(summary["accepted_run_count"], 0)
                    self.assertIn("raw FPS meter state differs from initial render configuration",
                                  summary["excluded_runs"][0]["reasons"])

    def test_fps_meter_warmup_contamination_is_retained_even_with_constant_samples(self):
        value = raw_report([1, 2], fps_meter=True)
        value["validity"]["fps_meter_constant"] = False
        summary = self.analyze(value)
        self.assertEqual(summary["accepted_run_count"], 0)
        self.assertIn("validity.fps_meter_constant=false", summary["excluded_runs"][0]["reasons"])

    def test_fps_meter_extension_requires_complete_boolean_observations(self):
        mutations = (
            lambda value: value["render"].pop("fps_meter_enabled"),
            lambda value: value["validity"].pop("fps_meter_constant"),
            lambda value: value["samples"][0].pop("fps_meter_enabled"),
            lambda value: value["render"].update(fps_meter_enabled=None),
            lambda value: value["render"].update(fps_meter_enabled=0),
            lambda value: value["validity"].update(fps_meter_constant=1),
            lambda value: value["samples"][0].update(fps_meter_enabled="false"),
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                value = raw_report([1, 2])
                mutation(value)
                with self.assertRaisesRegex(ValueError, "fps_meter"):
                    self.analyze(value)

    def test_legacy_fps_meter_state_remains_unknown_and_cannot_pool_with_observed_state(self):
        legacy = raw_report([1, 2])
        del legacy["render"]["fps_meter_enabled"]
        del legacy["validity"]["fps_meter_constant"]
        for sample in legacy["samples"]:
            del sample["fps_meter_enabled"]
        summary = self.analyze(legacy)
        self.assertEqual(summary["accepted_run_count"], 1)
        self.assertNotIn("fps_meter_enabled", summary["cases"][0]["configuration"]["render"])
        partial_extensions = (
            lambda value: value["render"].update(fps_meter_enabled=False),
            lambda value: value["validity"].update(fps_meter_constant=True),
            lambda value: value["samples"][1].update(fps_meter_enabled=False),
        )
        for extension in partial_extensions:
            with self.subTest(extension=extension):
                partial = deepcopy(legacy)
                extension(partial)
                with self.assertRaisesRegex(ValueError, "fps_meter"):
                    self.analyze(partial)
        for enabled in (False, True):
            with self.subTest(enabled=enabled):
                with self.assertRaisesRegex(ValueError, "incompatible repeats.*fps_meter_enabled"):
                    self.analyze(legacy, raw_report([2, 3], run=2, fps_meter=enabled))

    def test_fps_meter_on_and_off_are_valid_but_incompatible_repetitions(self):
        off = raw_report([1, 2])
        on = raw_report([2, 3], run=2, fps_meter=True)
        self.assertEqual(self.analyze(off)["accepted_run_count"], 1)
        self.assertEqual(self.analyze(on)["accepted_run_count"], 1)
        with self.assertRaisesRegex(ValueError, "incompatible repeats.*fps_meter_enabled"):
            self.analyze(off, on)
        comparison = report.compare(self.analyze(off), self.analyze(on))
        self.assertTrue(any("fps_meter_enabled" in reason for reason in comparison["cases"][0]["reasons"]))

    def test_contamination_is_visible_and_excluded_without_dropping_slow_samples(self):
        bad = raw_report([999, 999], run=2)
        bad["validity"]["all_frames_focused"] = False
        bad["samples"][0]["focused"] = False
        summary = self.analyze(raw_report([1, 10000]), bad)
        self.assertEqual(summary["accepted_run_count"], 1)
        self.assertEqual(summary["excluded_run_count"], 1)
        self.assertEqual(summary["cases"][0]["metrics"]["cpu_frame_ms"]["run_max"]["max"], 10000)
        self.assertIn("validity.all_frames_focused=false", summary["excluded_runs"][0]["reasons"])
        self.assertTrue(Path(summary["excluded_runs"][0]["path"]).exists())
        self.assertIn("EXCLUDED:", report.render_text(summary))

    def test_raw_contamination_cannot_be_hidden_by_true_validity_flags(self):
        mutations = [
            lambda value: value["samples"][0].update(focused=False),
            lambda value: value["samples"][0].update(error="lost surface"),
            lambda value: value["samples"][1].update(mesh_revision=2),
            lambda value: value["samples"][1].update(viewport_pixels=[640, 737]),
            lambda value: value["samples"][0].update(surface_pixels=None),
        ]
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                value = raw_report([1, 2])
                mutation(value)
                summary = self.analyze(value)
                self.assertEqual(summary["accepted_run_count"], 0)
                self.assertEqual(summary["excluded_run_count"], 1)
                self.assertTrue(summary["excluded_runs"][0]["reasons"])

    def test_web_contamination_and_missing_observation_receipt(self):
        bad = raw_report([1, 2], host="web")
        bad["browser_observations"]["comparable"] = False
        summary = self.analyze(bad)
        self.assertEqual(summary["excluded_run_count"], 1)
        old = raw_report([1, 2], host="web")
        old.pop("browser_observations")
        self.assertIn("lack browser", self.analyze(old)["cases"][0]["warnings"][1])
        software = raw_report([1, 2], host="web")
        software["metadata"]["browser"]["automation"]["gpu_process"]["software_renderer_detected"] = True
        self.assertEqual(self.analyze(software)["excluded_run_count"], 1)

    def test_skipped_and_disabled_stages_stay_null_and_projection_is_named_separately(self):
        renderer = self.analyze(raw_report([2, 4], mode="renderer"))["cases"][0]
        self.assertIsNone(renderer["metrics"]["cpu_stage_ms.editor_projection"])
        self.assertIsNone(renderer["metrics"]["cpu_stage_ms.ui"])
        self.assertIsNotNone(renderer["metrics"]["cpu_stage_ms.scene_encode"])
        untimed = self.analyze(raw_report([2, 4], stages=False))["cases"][0]
        self.assertTrue(all(value is None for name, value in untimed["metrics"].items() if name.startswith("cpu_stage_ms.")))
        editor = self.analyze(raw_report([2, 4]))["cases"][0]
        self.assertEqual(editor["metrics"]["cpu_stage_ms.editor_projection"]["run_medians"]["median"], 0.75)
        self.assertEqual(editor["metrics"]["cpu_frame_ms"]["run_medians"]["median"], 3)

    def test_initial_missing_interval_is_not_zero_and_zero_durations_do_not_divide(self):
        value = raw_report([0, 0])
        value["samples"][0]["frame_start_interval_ms"] = None
        case = self.analyze(value)["cases"][0]
        self.assertEqual(case["runs"][0]["metrics"]["frame_start_interval_ms"]["count"], 1)
        self.assertIsNone(case["metrics"]["cpu_frame_ms"]["run_medians"]["relative_mad_percent"])
        compared = report.compare(self.analyze(value), self.analyze(raw_report([1, 1])))
        metric = compared["cases"][0]["metrics"]["cpu_frame_ms"]
        self.assertEqual(metric["delta_ms"], 1)
        self.assertIsNone(metric["delta_percent"])
        self.assertIn("undefined from zero", report.render_comparison(compared))

    def test_missing_validity_nonfinite_negative_and_malformed_samples_fail(self):
        mutations = [
            lambda value: value.pop("validity"),
            lambda value: value["validity"].pop("no_frame_errors"),
            lambda value: value["validity"].update(no_frame_errors=1),
            lambda value: value["samples"][0].update(cpu_frame_ms=float("nan")),
            lambda value: value["metadata"].update(rustflags=float("inf")),
            lambda value: value["samples"][0].update(cpu_frame_ms=-1),
            lambda value: value["samples"][0].update(cpu_frame_ms=True),
            lambda value: value["samples"][0].update(frame=2),
            lambda value: value["samples"].pop(),
            lambda value: value["samples"][1].update(frame_start_interval_ms=None),
            lambda value: value["samples"][0]["cpu_stage_ms"].pop("ui"),
        ]
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                value = raw_report([1, 2])
                mutation(value)
                with self.assertRaises(ValueError):
                    self.analyze(value)

    def test_identical_paths_or_copied_reports_are_not_independent_repetitions(self):
        first = self.save(raw_report([1, 2]))
        with self.assertRaisesRegex(ValueError, "duplicate report path"):
            report.analyze([first, first])
        copied = self.save(raw_report([1, 2]))
        with self.assertRaisesRegex(ValueError, "copies are not independent"):
            report.analyze([first, copied])

    def test_reformatting_raw_json_does_not_create_an_independent_run(self):
        value = raw_report([1, 2])
        value["metadata"].pop("suite")
        first = self.save(value)
        copied = self.directory / "reformatted.json"
        copied.write_text(json.dumps(value, indent=4, sort_keys=True))
        self.assertNotEqual(first.read_bytes(), copied.read_bytes())
        with self.assertRaisesRegex(ValueError, "reformatted copies are not independent"):
            report.analyze([first, copied])

    def test_repeat_incompatibility_names_source_fixture_environment_or_camera_field(self):
        mutations = [
            ("source", lambda value: value["metadata"]["source_fingerprint"].update(sha256="b" * 64)),
            ("input", lambda value: value["metadata"]["input"].update(sha256="b" * 64)),
            ("rustc", lambda value: value["metadata"].update(rustc="different compiler")),
            ("profile", lambda value: value["metadata"].update(profile="dev")),
            ("operating_system", lambda value: value["metadata"].update(operating_system="other OS")),
            ("adapter", lambda value: value["metadata"]["adapter"].update(name="other GPU")),
            ("shading", lambda value: value["render"].update(shading="material")),
            ("camera_sequence", lambda value: value["samples"][1].update(camera_view_projection=[42] * 16)),
        ]
        for expected, mutation in mutations:
            with self.subTest(expected=expected):
                other = raw_report([1, 2], run=2)
                mutation(other)
                with self.assertRaisesRegex(ValueError, expected):
                    self.analyze(raw_report([1, 2]), other)

    def test_browser_version_and_headless_mode_are_incompatible_repeats(self):
        for field, new in (("browser_version", "other version"), ("mode", "headless")):
            with self.subTest(field=field):
                other = raw_report([1, 2], run=2, host="web")
                other["metadata"]["browser"]["automation"][field] = new
                if field == "mode":
                    other["metadata"]["browser_mode"] = new
                with self.assertRaisesRegex(ValueError, field):
                    self.analyze(raw_report([1, 2], host="web"), other)

    def test_compare_allows_source_change_and_reports_percentage_and_both_run_ranges(self):
        baseline = self.analyze(raw_report([10, 10], run=1), raw_report([12, 12], run=2), raw_report([14, 14], run=3))
        candidate_runs = [raw_report([value, value], run=index) for index, value in enumerate((8, 9, 10), 1)]
        for value in candidate_runs:
            value["metadata"]["revision"] = "candidate revision"
            value["metadata"]["source_fingerprint"]["sha256"] = "b" * 64
        comparison = report.compare(baseline, self.analyze(*candidate_runs))
        row = comparison["cases"][0]
        self.assertEqual(row["status"], "comparable")
        metric = row["metrics"]["cpu_frame_ms"]
        self.assertEqual(metric["delta_percent"], -25)
        self.assertEqual(metric["delta_ms"], -3)
        text = report.render_comparison(comparison)
        self.assertIn("[10.000..14.000]", text)
        self.assertIn("[8.000..10.000]", text)
        self.assertIn("-25.00%", text)

    def test_compare_fixture_dimensions_and_missing_case_are_explicitly_unsupported(self):
        baseline = self.analyze(raw_report([1, 2]))
        changed = raw_report([1, 2])
        changed["metadata"]["requested_physical_size"] = [1920, 1080]
        changed["metadata"]["surface"].update(width=1920, height=1080)
        for sample in changed["samples"]:
            sample["surface_pixels"] = [1920, 1080]
        candidate = self.analyze(changed, raw_report([1, 2], mode="renderer"))
        comparison = report.compare(baseline, candidate)
        self.assertEqual([case["status"] for case in comparison["cases"]], ["unsupported", "unsupported"])
        self.assertTrue(any("requested_physical_size" in reason for reason in comparison["cases"][0]["reasons"]))
        self.assertIn("missing", comparison["cases"][1]["reasons"][0])
        changed["metadata"]["requested_physical_size"] = [1280, 800]
        changed["metadata"]["surface"].update(width=1280, height=800)
        for sample in changed["samples"]:
            sample["surface_pixels"] = [1280, 800]
        changed["metadata"]["input"]["sha256"] = "c" * 64
        self.assertIn("input.sha256", report.compare(baseline, self.analyze(changed))["cases"][0]["reasons"][0])

    def test_suite_raw_cannot_be_reaccepted_after_failed_teardown_or_missing_receipt(self):
        for corruption in ("failed", "missing", "hash", "schema"):
            with self.subTest(corruption=corruption):
                path = self.save(raw_report([1, 2], run=self.counter + 1))
                manifest_path = self.directory / "manifest.json"
                manifest = json.loads(manifest_path.read_text())
                if corruption == "failed":
                    manifest["runs"][-1]["status"] = "failed"
                elif corruption == "hash":
                    manifest["runs"][-1]["sha256"] = "0" * 64
                elif corruption == "schema":
                    manifest["schema"] = "unknown"
                manifest_path.write_text(json.dumps(manifest))
                if corruption == "missing":
                    manifest_path.unlink()
                summary = report.analyze([path])
                self.assertEqual(summary["accepted_run_count"], 0)
                self.assertIn("suite receipt", summary["excluded_runs"][0]["reasons"][0])
                manifest_path.unlink(missing_ok=True)

    def test_standalone_legacy_raw_is_accepted_with_evidence_limitation(self):
        value = raw_report([1, 2])
        value["metadata"].pop("suite")
        summary = self.analyze(value)
        self.assertEqual(summary["accepted_run_count"], 1)
        self.assertTrue(any("Standalone" in warning for warning in summary["cases"][0]["warnings"]))

    def test_artifact_identity_must_match_repeats_but_can_change_in_comparison(self):
        before = raw_report([1, 2])
        before["metadata"]["artifact_fingerprint"] = {"sha256": "a" * 64, "files": []}
        after = raw_report([1, 2], run=2)
        after["metadata"]["artifact_fingerprint"] = {"sha256": "b" * 64, "files": []}
        with self.assertRaisesRegex(ValueError, "source.artifact_fingerprint"):
            self.analyze(before, after)
        comparison = report.compare(self.analyze(before), self.analyze(after))
        self.assertEqual(comparison["cases"][0]["status"], "comparable")

    def test_saved_summary_validation_rejects_malformed_or_inconsistent_deltas(self):
        valid = self.analyze(raw_report([1, 2]))
        invalid = [None, [], {}, {"schema": report.SCHEMA}]
        missing = deepcopy(valid)
        missing["cases"][0].pop("configuration")
        invalid.append(missing)
        duplicate = deepcopy(valid)
        duplicate["cases"] *= 2
        invalid.append(duplicate)
        nan = deepcopy(valid)
        nan["cases"][0]["metrics"]["cpu_frame_ms"]["run_medians"]["median"] = float("nan")
        invalid.append(nan)
        forged = deepcopy(valid)
        forged["cases"][0]["metrics"]["cpu_frame_ms"]["run_medians"]["median"] = 0.001
        invalid.append(forged)
        invented_frames = deepcopy(valid)
        invented_frames["cases"][0]["frame_count"] = 1000
        invalid.append(invented_frames)
        dropped_frames = deepcopy(valid)
        dropped_frames["cases"][0]["runs"][0]["metrics"]["cpu_frame_ms"]["count"] = 1
        invalid.append(dropped_frames)
        for value in invalid:
            with self.subTest(value=value), self.assertRaises(ValueError):
                report.compare(valid, value)

    def test_all_contaminated_receipt_and_empty_comparison_never_claim_performance(self):
        bad = raw_report([1, 2])
        bad["validity"]["no_frame_errors"] = False
        summary = self.analyze(bad)
        self.assertEqual(summary["cases"], [])
        self.assertIn("No accepted runs", report.render_text(summary))
        comparison = report.compare(summary, summary)
        self.assertEqual(comparison["cases"][0]["status"], "unsupported")
        self.assertIn("no accepted", report.render_comparison(comparison))
        with self.assertRaisesRegex(ValueError, "at least one"):
            report.analyze([])


if __name__ == "__main__":
    unittest.main()
