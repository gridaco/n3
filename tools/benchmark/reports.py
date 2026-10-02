"""Auditable aggregation of independent viewport measurement runs.

Frames in one process are correlated. Each accepted process contributes one median
per metric; frames are never pooled into a pretend larger independent sample.
"""

from copy import deepcopy
import hashlib
import json
from pathlib import Path

from .contracts import (
    STAGES,
    VALIDITY_FIELDS,
    canonical as _canonical,
    contamination_reasons as _contamination,
    number as _number,
    require as _require,
    suite_provenance as _suite_provenance,
    validate_raw as _validate,
)
from .statistics import aggregate as _aggregate, distribution as _distribution

SCHEMA = "n3.viewport-suite.v1"
METHOD = {
    "independent_unit": "one fresh application/browser process",
    "estimate": "median of independent run medians; frames are not pooled",
    "spread": "unscaled median absolute deviation of run medians; relative MAD = 100 * MAD / median",
    "tails": "nearest-rank per-run p95 and maximum retained separately",
    "slow_samples_removed": False,
    "confidence_intervals": None,
    "limitations": [
        "Fewer than three accepted runs provide insufficient repeat evidence.",
        "Ranges and MAD are descriptive, not confidence intervals or significance tests.",
        "Fixed warmup frames do not prove steady state; short frame windows give sparse tail estimates.",
        "CPU API timings do not establish GPU execution time or displayed FPS.",
        "Frame-start cadence includes scheduling; semantic orbit bypasses device event delivery.",
        "editor_projection is nested inside ui/editor_prepare; do not add it to enclosing stages.",
        "Zero duration can reflect clock quantization; percentage changes from zero are undefined.",
    ],
}


def _hash(value):
    return hashlib.sha256(_canonical(value).encode()).hexdigest()


def _differences(left, right, prefix=""):
    if isinstance(left, dict) and isinstance(right, dict):
        result = []
        for key in sorted(left.keys() | right.keys()):
            name = f"{prefix}.{key}" if prefix else key
            if key not in left or key not in right:
                result.append(name)
            else:
                result.extend(_differences(left[key], right[key], name))
        return result
    return [] if left == right else [prefix]


def _identity(report):
    options = report["options"]
    return {
        "host": report["metadata"]["host"], "mode": options["mode"],
        "workload": options["workload"], "selected": options["selected"],
        "instrument_stages": options["instrument_stages"],
    }


def _case_id(identity):
    return "/".join((identity["host"], identity["mode"], identity["workload"],
                     "selected" if identity["selected"] else "unselected",
                     "stages" if identity["instrument_stages"] else "untimed"))


def _configuration(report):
    metadata = deepcopy(report["metadata"])
    source = {name: metadata.pop(name) for name in ("revision", "source_fingerprint", "dirty")}
    if "artifact_fingerprint" in metadata:
        source["artifact_fingerprint"] = metadata.pop("artifact_fingerprint")
    for name in ("recorded_at_utc", "loading", "suite"):
        metadata.pop(name, None)
    # Names identify provenance for readers; bytes/hash identify the workload.
    metadata["input"].pop("name", None)
    samples = report["samples"]
    geometry = {name: samples[0][name] for name in ("surface_pixels", "viewport_pixels", "viewport_points", "pixels_per_point")}
    # Normalize JSON's equivalent integer/float spellings before hashing matrices.
    cameras = [[float(value) if value else 0.0 for value in sample["camera_view_projection"]] for sample in samples]
    return source, {
        "metadata": metadata, "options": report["options"], "render": report["render"],
        "model": report["model"], "frame_geometry": geometry,
        "camera_sequence_sha256": _hash(cameras),
    }


def _run_metrics(report):
    samples = report["samples"]
    metrics = {name: _distribution(sample[name] for sample in samples)
               for name in ("cpu_frame_ms", "frame_start_interval_ms")}
    for stage in STAGES:
        values = [sample["projection"]["cpu_ms"] if stage == "editor_projection"
                  else (sample["cpu_stage_ms"] or {}).get(stage) for sample in samples]
        metrics[f"cpu_stage_ms.{stage}"] = _distribution(values)
    return metrics


def analyze(paths: list[Path]) -> dict:
    """Read raw reports, exclude contaminated runs, and refuse incompatible repeats.

    Malformed reports and duplicate inputs raise ValueError. Exclusions remain in
    the returned receipt; a slow sample alone never makes a run contaminated.
    """
    _require(bool(paths), "at least one raw report is required")
    cases, excluded, seen_paths, seen_content, seen_evidence = {}, [], set(), set(), set()
    for path in paths:
        path = Path(path).resolve()
        _require(path not in seen_paths, f"duplicate report path: {path}")
        seen_paths.add(path)
        raw = path.read_bytes()
        digest = hashlib.sha256(raw).hexdigest()
        _require(digest not in seen_content, f"duplicate report contents: {path}; copies are not independent runs")
        seen_content.add(digest)
        try:
            report = json.loads(raw)
            _validate(report)
        except (ValueError, TypeError, KeyError, OverflowError) as error:
            raise ValueError(f"{path}: {error}") from error
        evidence_digest = _hash(report)
        _require(evidence_digest not in seen_evidence,
                 f"duplicate report evidence: {path}; reformatted copies are not independent runs")
        seen_evidence.add(evidence_digest)
        identity = _identity(report)
        case_id = _case_id(identity)
        reference = {"path": str(path), "sha256": digest, "recorded_at_utc": report["metadata"]["recorded_at_utc"]}
        suite_reasons, provenance = _suite_provenance(path, digest, report)
        reference["suite_receipt"] = provenance
        reasons = _contamination(report) + suite_reasons
        if reasons:
            excluded.append({**reference, "case_id": case_id, "reasons": reasons,
                             "validity": report["validity"], "browser_observations": report.get("browser_observations")})
            continue
        source, configuration = _configuration(report)
        if case_id in cases:
            case = cases[case_id]
            mismatch = _differences(case["configuration"], configuration, "configuration") + _differences(case["source"], source, "source")
            _require(not mismatch, f"{path}: incompatible repeats for {case_id}: {', '.join(mismatch)}")
        else:
            case = {"id": case_id, "identity": identity, "configuration": configuration,
                    "source": source, "fixture_name": report["metadata"]["input"].get("name"), "runs": []}
            cases[case_id] = case
        case["runs"].append({**reference, "frame_count": len(report["samples"]),
                             "metrics": _run_metrics(report), "validity": report["validity"],
                             "browser_observations": report.get("browser_observations"),
                             "suite": report["metadata"].get("suite")})
    for case in cases.values():
        case["run_count"] = len(case["runs"])
        case["frame_count"] = sum(run["frame_count"] for run in case["runs"])
        case["metrics"] = {name: _aggregate([run["metrics"][name] for run in case["runs"]]) for name in case["runs"][0]["metrics"]}
        case["warnings"] = []
        if case["run_count"] < 3:
            case["warnings"].append(f"Insufficient repeats: {case['run_count']} accepted run(s); use at least 3.")
        if any(run["suite_receipt"] is None for run in case["runs"]):
            case["warnings"].append("Standalone reports lack a completed suite receipt proving teardown and source/artifact stability.")
        if case["identity"]["host"] == "web" and any(run["browser_observations"] is None for run in case["runs"]):
            case["warnings"].append("Some reports lack browser visibility/focus/resize observation across the complete run.")
    warnings = []
    if excluded:
        warnings.append(f"Excluded {len(excluded)} contaminated run(s); inspect excluded_runs and retained raw files.")
    if not cases:
        warnings.append("No accepted runs; this receipt cannot establish performance.")
    return {"schema": SCHEMA, "method": deepcopy(METHOD), "input_run_count": len(paths),
            "accepted_run_count": sum(case["run_count"] for case in cases.values()),
            "excluded_run_count": len(excluded), "cases": [cases[key] for key in sorted(cases)],
            "excluded_runs": excluded, "warnings": warnings}


def render_text(summary: dict) -> str:
    _require(summary.get("schema") == SCHEMA, "unsupported suite schema")
    lines = ["Viewport measurement report", f"Accepted runs: {summary['accepted_run_count']}; excluded: {summary['excluded_run_count']}",
             "Estimate: median of independent run medians (ms); slow samples are retained."]
    lines += [f"WARNING: {warning}" for warning in summary["warnings"]]
    for case in summary["cases"]:
        geometry = case["configuration"]["frame_geometry"]
        lines += ["", f"{case['id']} — {case['run_count']} run(s), {case['frame_count']} retained frames",
                  f"Fixture: {case['fixture_name']}; surface {geometry['surface_pixels']}; scene {geometry['viewport_pixels']}",
                  f"Source: {case['source']['revision']} / {case['source']['source_fingerprint']['sha256']}"]
        lines += [f"WARNING: {warning}" for warning in case["warnings"]]
        lines.append("metric | median | run-median range | relative MAD | per-run p95 range | worst frame")
        for name, metric in case["metrics"].items():
            if metric is None:
                lines.append(f"{name} | unavailable/excluded")
                continue
            medians, tails = metric["run_medians"], metric["run_p95"]
            spread = "undefined (zero median)" if medians["relative_mad_percent"] is None else f"{medians['relative_mad_percent']:.2f}%"
            lines.append(f"{name} | {medians['median']:.3f} | {medians['min']:.3f}..{medians['max']:.3f} | {spread} | "
                         f"{tails['min']:.3f}..{tails['max']:.3f} | {metric['run_max']['max']:.3f}")
        for run in case["runs"]:
            cpu, cadence = run["metrics"]["cpu_frame_ms"], run["metrics"]["frame_start_interval_ms"]
            lines.append(f"Run: {run['path']} | CPU median/p95/max {cpu['median']:.3f}/{cpu['p95']:.3f}/{cpu['max']:.3f}"
                         + (f" | cadence median/p95/max {cadence['median']:.3f}/{cadence['p95']:.3f}/{cadence['max']:.3f}" if cadence else " | cadence unavailable"))
    for run in summary["excluded_runs"]:
        lines += ["", f"EXCLUDED: {run['path']}", *[f"  {reason}" for reason in run["reasons"]]]
    lines += ["", *[f"Note: {limitation}" for limitation in summary["method"]["limitations"]]]
    return "\n".join(lines) + "\n"



def _validate_summary(summary):
    """Do not turn malformed or internally inconsistent saved summaries into deltas."""
    try:
        _require(isinstance(summary, dict) and summary.get("schema") == SCHEMA, "unsupported suite schema")
        _canonical(summary)
        _require(isinstance(summary.get("cases"), list), "missing summary cases")
        _require(isinstance(summary.get("excluded_runs"), list), "missing excluded runs")
        _require(isinstance(summary.get("warnings"), list) and all(isinstance(item, str) for item in summary["warnings"]),
                 "malformed summary warnings")
        for name in ("input_run_count", "accepted_run_count", "excluded_run_count"):
            _require(type(summary.get(name)) is int and summary[name] >= 0, f"malformed {name}")
        ids = set()
        accepted = 0
        expected_metrics = {"cpu_frame_ms", "frame_start_interval_ms"} | {f"cpu_stage_ms.{name}" for name in STAGES}
        for case in summary["cases"]:
            _require(isinstance(case, dict), "malformed case")
            _require(isinstance(case.get("id"), str) and case["id"] not in ids, "missing or duplicate case id")
            ids.add(case["id"])
            for name in ("configuration", "source", "identity", "metrics"):
                _require(isinstance(case.get(name), dict), f"malformed case {name}")
            _require(case["id"] == _case_id(case["identity"]), "case id differs from identity")
            configuration = case["configuration"]
            for name in ("metadata", "options", "render", "model", "frame_geometry"):
                _require(isinstance(configuration.get(name), dict) and configuration[name], f"missing configuration.{name}")
            _require(isinstance(configuration.get("camera_sequence_sha256"), str) and len(configuration["camera_sequence_sha256"]) == 64,
                     "missing camera sequence identity")
            _require(configuration["metadata"].get("host") == case["identity"]["host"], "configuration host differs from case")
            for name in ("mode", "workload", "selected", "instrument_stages"):
                _require(configuration["options"].get(name) == case["identity"][name], f"configuration {name} differs from case")
            _require(isinstance(case["source"].get("revision"), str) and isinstance(case["source"].get("source_fingerprint"), dict),
                     "missing source identity")
            _require(isinstance(case.get("warnings"), list) and all(isinstance(item, str) for item in case["warnings"]),
                     "malformed case warnings")
            _require(type(case.get("run_count")) is int and case["run_count"] > 0, "malformed case run_count")
            _require(isinstance(case.get("runs"), list) and len(case["runs"]) == case["run_count"], "case run count differs from receipt")
            _require(set(case["metrics"]) == expected_metrics, "missing or unknown summary metrics")
            sample_frames = configuration["options"].get("sample_frames")
            _require(type(sample_frames) is int and sample_frames > 0, "malformed requested frame count")
            _require(type(case.get("frame_count")) is int and case["frame_count"] == sample_frames * case["run_count"],
                     "case frame count differs from requested frames and runs")
            for run in case["runs"]:
                _require(isinstance(run, dict) and isinstance(run.get("metrics"), dict) and set(run["metrics"]) == expected_metrics,
                         "missing or unknown per-run metrics")
                _require(type(run.get("frame_count")) is int and run["frame_count"] == sample_frames,
                         "run frame count differs from requested frames")
                for distribution in run["metrics"].values():
                    if distribution is None:
                        continue
                    _require(isinstance(distribution, dict), "malformed per-run distribution")
                    _require(type(distribution.get("count")) is int and distribution["count"] > 0, "malformed distribution count")
                    _require(distribution["count"] <= sample_frames, "distribution count exceeds retained frames")
                    for name in ("min", "median", "p95", "max"):
                        _number(distribution.get(name), f"distribution.{name}")
                    _require(distribution["min"] <= distribution["median"] <= distribution["p95"] <= distribution["max"],
                             "inconsistent per-run distribution")
                cpu = run["metrics"]["cpu_frame_ms"]
                _require(cpu is not None and cpu["count"] == sample_frames, "CPU distribution must retain every frame")
            for name, metric in case["metrics"].items():
                expected = _aggregate([run["metrics"][name] for run in case["runs"]])
                _require(metric == expected, f"aggregate {name} differs from per-run metrics")
            accepted += case["run_count"]
        _require(accepted == summary["accepted_run_count"], "accepted run count differs from cases")
        _require(len(summary["excluded_runs"]) == summary["excluded_run_count"], "excluded run count differs from receipt")
        _require(summary["input_run_count"] == summary["accepted_run_count"] + summary["excluded_run_count"], "input run count differs from receipt")
    except (KeyError, TypeError, AttributeError, OverflowError) as error:
        raise ValueError(f"malformed suite summary: {error}") from error


def compare(baseline: dict, candidate: dict) -> dict:
    """Compare matching cases, allowing source changes but no workload/env drift."""
    _validate_summary(baseline)
    _validate_summary(candidate)
    before = {case["id"]: case for case in baseline["cases"]}
    after = {case["id"]: case for case in candidate["cases"]}
    rows = []
    for case_id in sorted(before.keys() | after.keys()):
        row = {"id": case_id, "status": "unsupported", "reasons": [], "metrics": {}}
        rows.append(row)
        if case_id not in before or case_id not in after:
            row["reasons"].append("case missing from " + ("baseline" if case_id not in before else "candidate"))
            continue
        left, right = before[case_id], after[case_id]
        row.update({"baseline_source": left["source"], "candidate_source": right["source"],
                    "baseline_run_count": left["run_count"], "candidate_run_count": right["run_count"],
                    "warnings": left["warnings"] + right["warnings"]})
        mismatch = _differences(left["configuration"], right["configuration"])
        if mismatch:
            row["reasons"] = [f"incompatible {name}" for name in mismatch]
            continue
        row["status"] = "comparable"
        for name in sorted(left["metrics"].keys() | right["metrics"].keys()):
            old, new = left["metrics"].get(name), right["metrics"].get(name)
            metric = {"baseline": old, "candidate": new, "delta_ms": None, "delta_percent": None}
            row["metrics"][name] = metric
            if old is None or new is None:
                metric["status"] = "unavailable"
                continue
            old_median, new_median = old["run_medians"]["median"], new["run_medians"]["median"]
            metric["status"] = "descriptive"
            metric["delta_ms"] = new_median - old_median
            if old_median:
                metric["delta_percent"] = 100 * (new_median - old_median) / old_median
    if not rows:
        rows.append({"id": "(suite)", "status": "unsupported", "reasons": ["no accepted cases in either receipt"], "metrics": {}})
    return {"schema": "n3.viewport-comparison.v1", "cases": rows,
            "warnings": baseline["warnings"] + candidate["warnings"],
            "interpretation": "Positive delta means more time; negative means less. Descriptive comparison, not a significance test."}


def render_comparison(comparison: dict) -> str:
    _require(comparison.get("schema") == "n3.viewport-comparison.v1", "unsupported comparison schema")
    lines = ["Viewport baseline/candidate comparison", comparison["interpretation"]]
    lines += [f"WARNING: {warning}" for warning in comparison["warnings"]]
    for case in comparison["cases"]:
        lines += ["", f"{case['id']}: {case['status']}"]
        lines += [f"  {reason}" for reason in case["reasons"]]
        if case["status"] != "comparable":
            continue
        lines += [f"WARNING: {warning}" for warning in case["warnings"]]
        lines.append("metric | baseline median [run range] | candidate median [run range] | delta")
        for name, metric in case["metrics"].items():
            if metric["status"] == "unavailable":
                lines.append(f"{name} | unavailable/excluded")
                continue
            left, right = metric["baseline"]["run_medians"], metric["candidate"]["run_medians"]
            delta = "undefined from zero baseline" if metric["delta_percent"] is None else f"{metric['delta_percent']:+.2f}%"
            lines.append(f"{name} | {left['median']:.3f} [{left['min']:.3f}..{left['max']:.3f}] | "
                         f"{right['median']:.3f} [{right['min']:.3f}..{right['max']:.3f}] | {delta} ({metric['delta_ms']:+.3f} ms)")
    return "\n".join(lines) + "\n"
