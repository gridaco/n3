"""Raw measurement schema and evidence admission policies.

A producer's boolean validity summary is evidence, never authority. Admission
cross-checks the observed per-frame and browser facts before aggregation.
"""

import json
import math
import struct

VALIDITY_FIELDS = (
    "all_frames_focused", "constant_surface", "constant_viewport",
    "fps_meter_constant", "mode_contract_satisfied", "no_frame_errors", "unchanged_mesh_revision",
    "viewport_matches_baseline",
)
STAGES = (
    "cache_sync", "commands_and_refresh", "editor_prepare", "feedback",
    "host_prepare", "host_tail", "present_api", "scene_composite",
    "scene_encode", "submit_api", "surface_acquire", "tessellation_and_textures",
    "ui", "ui_encode", "editor_projection",
)
COUNTERS = frozenset((
    "mesh_uploads", "base_mesh_upload_bytes", "viewport_resizes", "ui_jobs",
    "egui_passes", "egui_tessellations", "egui_texture_updates", "egui_composites",
    "editor_prepares", "editor_feedback_updates", "scene_renders", "scene_composites",
))
_COMMON_STAGES = frozenset((
    "surface_acquire", "host_prepare", "cache_sync", "scene_encode", "submit_api",
    "present_api", "host_tail",
))
_MODE_STAGES = {
    "editor": _COMMON_STAGES | {"ui", "commands_and_refresh", "tessellation_and_textures", "feedback", "ui_encode"},
    "viewport": _COMMON_STAGES | {"editor_prepare", "feedback", "scene_composite"},
    "renderer": _COMMON_STAGES | {"scene_composite"},
}
def require(condition, message):
    if not condition:
        raise ValueError(message)


def number(value, name, nullable=False, positive=False):
    if nullable and value is None:
        return
    require(type(value) in (int, float) and math.isfinite(value), f"{name} must be finite numeric data")
    require(value > 0 if positive else value >= 0, f"{name} must be {'positive' if positive else 'nonnegative'}")


def vector(value, size, name, positive=False):
    require(isinstance(value, list) and len(value) == size, f"{name} must contain {size} numbers")
    for item in value:
        require(type(item) in (int, float) and math.isfinite(item), f"{name} must contain finite numbers")
        if positive:
            require(item > 0, f"{name} must contain positive numbers")


def _pixel_size(value, name, *, positive=False):
    vector(value, 2, name, positive=positive)
    require(all(type(item) is int and item >= 0 for item in value), f"{name} must contain integer pixels")


def _rect(value, name):
    vector(value, 4, name)
    require(value[2] > value[0] and value[3] > value[1], f"{name} must have positive width and height")


def _integer(value, name, minimum=0):
    require(type(value) is int and value >= minimum, f"malformed {name}")


def _text(value, name, *, empty=False):
    require(isinstance(value, str) and (empty or bool(value)), f"malformed {name}")


def _same_scale(left, right):
    # The browser/request uses f64 while winit/egui also expose f32 scales.
    return math.isclose(left, right, rel_tol=1e-6, abs_tol=0)


def _scene_extent(bounds, scale):
    # Mirror the renderer's f32 subtraction, multiplication and positive round.
    # Its device texture limit is not in v2, so this is only an upper bound.
    def f32(value):
        return struct.unpack("f", struct.pack("f", value))[0]

    try:
        extent = []
        for minimum, maximum in zip(bounds[:2], bounds[2:]):
            length = f32(f32(maximum) - f32(minimum))
            pixels = f32(length * f32(scale))
            extent.append(max(1, math.floor(pixels + 0.5)))
        return extent
    except (OverflowError, ValueError):
        return None


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)


def validate_raw(report):
    require(isinstance(report, dict) and report.get("schema") == "n3.viewport-measure.v2",
             "expected n3.viewport-measure.v2 raw report")
    # Reject NaN, infinity and overflowing JSON numbers anywhere, including metadata.
    canonical(report)
    for name in ("metadata", "options", "render", "model", "validity"):
        require(isinstance(report.get(name), dict), f"missing or malformed {name}")
    metadata, options = report["metadata"], report["options"]
    required_metadata = (
        "host", "adapter", "architecture", "browser_mode", "build_command",
        "cargo_encoded_rustflags", "dirty", "input", "operating_system", "profile",
        "profile_overrides", "recorded_at_utc", "requested_device_scale_factor",
        "requested_physical_size", "revision", "rustc", "rustflags",
        "source_fingerprint", "surface",
    )
    for name in required_metadata:
        require(name in metadata, f"missing metadata.{name}")
    require(metadata["host"] in ("native", "web"), "unsupported metadata.host")
    for name in ("architecture", "operating_system", "profile", "revision", "rustc", "recorded_at_utc"):
        require(isinstance(metadata[name], str) and bool(metadata[name]), f"malformed metadata.{name}")
    for name in ("adapter", "input", "profile_overrides", "source_fingerprint", "surface"):
        require(isinstance(metadata[name], dict), f"malformed metadata.{name}")
    adapter = metadata["adapter"]
    for name in ("name", "driver", "driver_info", "backend", "device_type"):
        # BrowserWebGpu deliberately leaves identifying strings empty on some
        # browsers. Preserve that observed unknown, but require the wire field.
        _text(adapter.get(name), f"metadata.adapter.{name}", empty=name in ("name", "driver", "driver_info"))
    for name in ("vendor", "device"):
        _integer(adapter.get(name), f"metadata.adapter.{name}")
    for container, name in ((metadata["input"], "input"), (metadata["source_fingerprint"], "source_fingerprint")):
        digest = container.get("sha256")
        require(isinstance(digest, str) and len(digest) == 64 and all(c in "0123456789abcdef" for c in digest),
                 f"malformed metadata.{name}.sha256")
    require(type(metadata["input"].get("bytes")) is int and metadata["input"]["bytes"] > 0,
             "malformed metadata.input.bytes")
    require(type(metadata["dirty"]) is bool, "malformed metadata.dirty")
    _pixel_size(metadata["requested_physical_size"], "metadata.requested_physical_size", positive=True)
    for name in ("width", "height"):
        value = metadata["surface"].get(name)
        require(type(value) is int and value > 0, f"malformed metadata.surface.{name}")
    surface = metadata["surface"]
    for name in ("format", "present_mode", "alpha_mode"):
        _text(surface.get(name), f"metadata.surface.{name}")
    number(surface.get("scale_factor"), "metadata.surface.scale_factor", positive=True)
    _integer(surface.get("desired_maximum_frame_latency"), "metadata.surface.desired_maximum_frame_latency")
    require(isinstance(metadata["build_command"], list) and metadata["build_command"]
             and all(isinstance(value, str) for value in metadata["build_command"]), "malformed build_command")
    require(options.get("mode") in ("editor", "viewport", "renderer"), "unsupported mode")
    require(options.get("workload") in ("orbit", "stationary"), "unsupported workload")
    for name in ("instrument_stages", "selected"):
        require(type(options.get(name)) is bool, f"malformed options.{name}")
    for name in ("sample_frames", "warmup_frames"):
        require(type(options.get(name)) is int and options[name] >= (1 if name == "sample_frames" else 0),
                 f"malformed options.{name}")
    require(not (options["mode"] == "renderer" and options["selected"]), "renderer cannot include selection")
    model, render = report["model"], report["render"]
    for name in ("objects", "vertices", "faces", "triangles", "linked_assets"):
        _integer(model.get(name), f"model.{name}", minimum=1 if name == "objects" else 0)
    for name in ("shading", "theme"):
        _text(render.get(name), f"render.{name}")
    for name in ("edges", "grid"):
        require(type(render.get(name)) is bool, f"malformed render.{name}")
    _integer(render.get("selected_objects"), "render.selected_objects")
    require("hovered_object" in render, "missing render.hovered_object")
    if render["hovered_object"] is not None:
        _integer(render["hovered_object"], "render.hovered_object")
    _rect(render.get("baseline_viewport_ui_points"), "baseline viewport UI")
    validity = report["validity"]
    samples = report.get("samples")
    require(isinstance(samples, list) and len(samples) == options["sample_frames"], "sample count differs from requested frames")
    # The first v2 captures predate this observation. Preserve their missing
    # render key as unknown (and incompatible with both observed on and off).
    # A partially present extension is corruption, never a legacy default.
    observes_fps_meter = ("fps_meter_enabled" in report["render"]
                          or "fps_meter_constant" in validity
                          or any(isinstance(sample, dict) and "fps_meter_enabled" in sample for sample in samples))
    for name in VALIDITY_FIELDS:
        if name == "fps_meter_constant" and not observes_fps_meter:
            continue
        require(type(validity.get(name)) is bool, f"missing or malformed validity.{name}")
    require(all(type(value) is bool for value in validity.values()), "validity flags must be boolean")
    if observes_fps_meter:
        require(type(report["render"].get("fps_meter_enabled")) is bool,
                 "missing or malformed render.fps_meter_enabled")
    _rect(report["render"].get("baseline_viewport_points"), "baseline viewport")
    vector(report["render"].get("initial_camera_view_projection"), 16, "initial camera")
    number(report["render"].get("pixels_per_point"), "render.pixels_per_point", positive=True)
    for index, sample in enumerate(samples):
        require(isinstance(sample, dict) and type(sample.get("frame")) is int and sample["frame"] == index,
                 "sample frame indices must be consecutive")
        number(sample.get("cpu_frame_ms"), "cpu_frame_ms")
        require("frame_start_interval_ms" in sample, "missing frame_start_interval_ms")
        number(sample["frame_start_interval_ms"], "frame_start_interval_ms", nullable=index == 0)
        require(type(sample.get("focused")) is bool, "malformed sample.focused")
        if observes_fps_meter:
            require(type(sample.get("fps_meter_enabled")) is bool,
                     "missing or malformed sample.fps_meter_enabled")
        require("error" in sample and (sample["error"] is None or isinstance(sample["error"], str)), "malformed sample.error")
        require(type(sample.get("mesh_revision")) is int and sample["mesh_revision"] >= 0, "malformed mesh_revision")
        # Null dimensions are an observed invalid frame, not corrupt JSON.
        for name in ("surface_pixels", "viewport_pixels"):
            require(name in sample, f"missing {name}")
            if sample[name] is not None:
                _pixel_size(sample[name], name)
        _rect(sample.get("viewport_points"), "viewport_points")
        vector(sample.get("camera_view_projection"), 16, "camera_view_projection")
        number(sample.get("pixels_per_point"), "pixels_per_point", positive=True)
        projection = sample.get("projection")
        require(isinstance(projection, dict) and "cpu_ms" in projection, "missing projection.cpu_ms")
        number(projection["cpu_ms"], "projection.cpu_ms", nullable=True)
        for name in ("rebuilds", "geometry_rebuilds", "vertices_tested"):
            require(type(projection.get(name)) is int and projection[name] >= 0, f"malformed projection.{name}")
        counters = sample.get("counters")
        require(isinstance(counters, dict) and set(counters) == COUNTERS and all(type(value) is int and value >= 0 for value in counters.values()),
                 "missing or malformed execution counters")
        require("cpu_stage_ms" in sample, "missing cpu_stage_ms")
        stages = sample["cpu_stage_ms"]
        if options["instrument_stages"]:
            require(isinstance(stages, dict) and set(stages) == set(STAGES) - {"editor_projection"}, "missing or unknown CPU stages")
            for name, value in stages.items():
                number(value, f"cpu_stage_ms.{name}", nullable=True)
        else:
            require(stages is None and projection["cpu_ms"] is None, "disabled stage timing must be null")
    if metadata["host"] == "web":
        browser = metadata.get("browser")
        require(isinstance(browser, dict), "missing browser metadata")
        for name in ("user_agent", "device_pixel_ratio", "physical_canvas", "css_canvas", "visibility"):
            require(name in browser, f"missing browser.{name}")
        number(browser["device_pixel_ratio"], "browser.device_pixel_ratio", positive=True)
        _text(browser["user_agent"], "browser.user_agent")
        require(browser["visibility"] in ("visible", "hidden"), "malformed browser.visibility")
        _pixel_size(browser["physical_canvas"], "browser.physical_canvas", positive=True)
        vector(browser["css_canvas"], 2, "browser.css_canvas", positive=True)
        require(metadata["browser_mode"] in ("headed", "headless", "manual"), "unsupported browser_mode")
        requested_scale = metadata["requested_device_scale_factor"]
        if metadata["browser_mode"] == "manual":
            require(requested_scale is None, "manual browser must observe rather than request device scale")
        else:
            number(requested_scale, "metadata.requested_device_scale_factor", positive=True)
        automation = browser.get("automation")
        require(automation is None or isinstance(automation, dict), "malformed browser automation metadata")
        if metadata["browser_mode"] != "manual":
            require(isinstance(automation, dict), "missing browser automation metadata")
            for name in ("browser_version", "mode", "tool", "version", "launch_arguments", "gpu_process"):
                require(name in automation, f"missing browser.automation.{name}")
        if automation:
            require(automation.get("mode") in ("headed", "headless"), "malformed browser.automation.mode")
            for name in ("browser_version", "tool", "version"):
                _text(automation.get(name), f"browser.automation.{name}")
            arguments = automation.get("launch_arguments")
            require(isinstance(arguments, list) and all(isinstance(value, str) for value in arguments),
                     "malformed browser.automation.launch_arguments")
            if "device_scale_factor" in automation:
                number(automation["device_scale_factor"], "browser.automation.device_scale_factor", positive=True)
            gpu = automation.get("gpu_process")
            require(gpu is None or isinstance(gpu, dict), "malformed browser.automation.gpu_process")
            if gpu is not None and "software_renderer_detected" in gpu:
                require(type(gpu["software_renderer_detected"]) is bool,
                        "malformed browser.automation.gpu_process.software_renderer_detected")
        observations = report.get("browser_observations")
        if observations is not None:
            require(isinstance(observations, dict) and type(observations.get("comparable")) is bool,
                     "malformed browser_observations")
            for name in ("visibility_changes", "focus_losses"):
                require(type(observations.get(name)) is int and observations[name] >= 0,
                        f"malformed browser_observations.{name}")
            require(type(observations.get("size_changed")) is bool,
                    "malformed browser_observations.size_changed")


def mode_contract_satisfied(sample, mode):
    """Independently check what executed, matching the Rust producer contract."""
    counts = sample["counters"]
    if counts["scene_renders"] != 1 or not sample["viewport_pixels"] or min(sample["viewport_pixels"]) <= 0:
        return False
    if mode == "editor":
        return (counts["egui_passes"] > 0 and counts["egui_tessellations"] == 1
                and counts["egui_composites"] == 1 and counts["editor_prepares"] > 0
                and counts["editor_feedback_updates"] == 1 and counts["scene_composites"] == 0)
    if any(counts[name] != 0 for name in (
        "egui_passes", "egui_tessellations", "egui_texture_updates", "egui_composites", "ui_jobs",
    )) or counts["scene_composites"] != 1:
        return False
    if mode == "viewport":
        return counts["editor_prepares"] > 0 and counts["editor_feedback_updates"] == 1
    return (counts["editor_prepares"] == 0 and counts["editor_feedback_updates"] == 0
            and all(sample["projection"][name] == 0 for name in ("rebuilds", "vertices_tested", "geometry_rebuilds"))
            and sample["projection"]["cpu_ms"] is None)


def contamination_reasons(report):
    reasons = [f"validity.{name}=false" for name, value in report["validity"].items() if not value]
    samples = report["samples"]
    if "fps_meter_enabled" in report["render"] and any(
        sample["fps_meter_enabled"] != report["render"]["fps_meter_enabled"] for sample in samples
    ):
        reasons.append("raw FPS meter state differs from initial render configuration")
    if any(not sample["focused"] for sample in samples):
        reasons.append("raw samples include lost focus")
    if any(sample["error"] is not None for sample in samples):
        reasons.append("raw samples include frame errors")
    if any(sample["mesh_revision"] != samples[0]["mesh_revision"] for sample in samples):
        reasons.append("raw samples changed mesh revision")
    mode = report["options"]["mode"]
    if any(not mode_contract_satisfied(sample, mode) for sample in samples):
        reasons.append("raw execution counters violate measurement mode contract")
    if report["options"]["instrument_stages"]:
        expected = _MODE_STAGES[mode]
        if any({name for name, value in sample["cpu_stage_ms"].items() if value is not None} != expected
               for sample in samples):
            reasons.append("raw stage availability violates measurement mode contract")
    for name in ("surface_pixels", "viewport_pixels"):
        if any(not sample[name] or min(sample[name]) <= 0 or sample[name] != samples[0][name] for sample in samples):
            reasons.append(f"raw samples have invalid or changing {name}")
    for name in ("viewport_points", "pixels_per_point"):
        if any(sample[name] != samples[0][name] for sample in samples):
            reasons.append(f"raw samples changed {name}")
    if any(sample["viewport_points"] != report["render"]["baseline_viewport_points"] for sample in samples):
        reasons.append("raw viewport differs from baseline")
    requested = report["metadata"]["requested_physical_size"]
    surface = report["metadata"]["surface"]
    if any(sample["surface_pixels"] != requested for sample in samples):
        reasons.append("raw surface differs from requested physical size")
    if [surface["width"], surface["height"]] != requested:
        reasons.append("surface metadata differs from requested physical size")
    if any(sample["pixels_per_point"] != report["render"]["pixels_per_point"] for sample in samples):
        reasons.append("raw pixel scale differs from render metadata")
    if any(not _same_scale(sample["pixels_per_point"], surface["scale_factor"]) for sample in samples):
        reasons.append("raw pixel scale differs from surface scale")
    # The production renderer may clamp the offscreen texture to its device
    # limit. Smaller dimensions are possible; dimensions larger than the rounded
    # logical extent or the complete surface contradict the recorded geometry.
    for sample in samples:
        pixels, bounds = sample["viewport_pixels"], sample["viewport_points"]
        scale = sample["pixels_per_point"]
        extent = _scene_extent(bounds, scale)
        if extent is None or (pixels and any(pixels[index] > requested[index] or pixels[index] > extent[index]
                                            for index in range(2))):
            reasons.append("raw viewport pixels exceed the surface or logical viewport extent")
            break
    if report["render"]["selected_objects"] != int(report["options"]["selected"]):
        reasons.append("observed selection differs from requested workload")
    if report["render"]["selected_objects"] > report["model"]["objects"]:
        reasons.append("observed selection exceeds document objects")
    if report["model"]["linked_assets"] > report["model"]["objects"]:
        reasons.append("linked asset count exceeds document objects")
    cameras = [sample["camera_view_projection"] for sample in samples]
    if report["options"]["workload"] == "stationary":
        if any(camera != report["render"]["initial_camera_view_projection"] for camera in cameras):
            reasons.append("stationary workload changed camera")
    elif any(left == right for left, right in zip(cameras, cameras[1:])):
        reasons.append("orbit workload did not advance camera between completed frames")
    browser = report["metadata"].get("browser", {})
    automation = browser.get("automation") or {}
    if browser.get("visibility", "visible") != "visible":
        reasons.append("browser was not visible")
    if (automation.get("gpu_process") or {}).get("software_renderer_detected") is True:
        reasons.append("browser software renderer detected")
    if browser and browser["physical_canvas"] != requested:
        reasons.append("browser canvas differs from requested physical size")
    if browser:
        ratio = browser["device_pixel_ratio"]
        physical_css = [css * ratio for css in browser["css_canvas"]]
        if any(not math.isfinite(observed) or math.floor(observed + 0.5) != pixels
               for observed, pixels in zip(physical_css, browser["physical_canvas"])):
            reasons.append("browser CSS canvas and device pixel ratio disagree with physical canvas")
        if not _same_scale(ratio, surface["scale_factor"]):
            reasons.append("browser device pixel ratio differs from surface scale")
        requested_scale = report["metadata"]["requested_device_scale_factor"]
        if requested_scale is not None and not _same_scale(ratio, requested_scale):
            reasons.append("browser device pixel ratio differs from requested scale")
        if "device_scale_factor" in automation and not _same_scale(ratio, automation["device_scale_factor"]):
            reasons.append("browser device pixel ratio differs from automation scale")
    if automation and automation["mode"] != report["metadata"]["browser_mode"]:
        reasons.append("browser automation mode differs from requested mode")
    observations = report.get("browser_observations", {})
    if observations.get("comparable") is False:
        reasons.append("browser_observations.comparable=false")
    if observations.get("visibility_changes", 0) > 0:
        reasons.append("raw browser observations include visibility changes")
    if observations.get("focus_losses", 0) > 0:
        reasons.append("raw browser observations include focus loss")
    if observations.get("size_changed") is True:
        reasons.append("raw browser observations include size changes")
    return reasons



def suite_provenance(path, digest, report):
    if "suite" not in report["metadata"]:
        return [], None
    manifest_path = path.parent.parent / "manifest.json"
    try:
        raw = manifest_path.read_bytes()
        manifest = json.loads(raw)
        canonical(manifest)
        require(isinstance(manifest, dict) and manifest.get("schema") == "n3.viewport-suite-manifest.v1",
                 "unsupported manifest schema")
        require(isinstance(manifest.get("runs"), list), "missing manifest runs")
        relative = path.relative_to(manifest_path.parent).as_posix()
        entries = [entry for entry in manifest["runs"] if isinstance(entry, dict) and entry.get("report") == relative]
        require(len(entries) == 1, "manifest must identify this raw report exactly once")
        entry = entries[0]
        require(entry.get("status") == "complete", "suite run did not complete teardown and stability checks")
        require(entry.get("sha256") == digest, "raw report hash differs from completed suite receipt")
        return [], {"path": str(manifest_path), "entry": entry}
    except (OSError, ValueError, TypeError) as error:
        return [f"suite receipt unavailable or invalid: {error}"], {"path": str(manifest_path)}
