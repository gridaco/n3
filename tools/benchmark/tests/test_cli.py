"""Exercise the public offline CLI with reviewed synthetic evidence, without mocks."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
FIXTURE = Path(__file__).parent / "fixtures/editor-capture.json"


class BenchmarkCliTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)

    def invoke(self, *arguments):
        return subprocess.run([sys.executable, "-m", "tools.benchmark", *map(str, arguments)],
                              cwd=ROOT, env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
                              text=True, capture_output=True, check=False, timeout=30)

    def capture(self, name, values, source="a"):
        raw = json.loads(FIXTURE.read_text())
        raw["metadata"]["recorded_at_utc"] = name
        raw["metadata"]["source_fingerprint"]["sha256"] = source * 64
        for frame, value in zip(raw["samples"], values):
            frame["cpu_frame_ms"] = value
            frame["frame_start_interval_ms"] = value + 1
        path = self.directory / f"{name}.json"
        path.write_text(json.dumps(raw))
        return path

    def test_report_and_compare_use_raw_samples_and_independent_run_estimates(self):
        baseline = [self.capture(f"before-{i}", values) for i, values in enumerate(
            ([10, 20, 90], [11, 21, 91], [12, 22, 92]))]
        candidate = [self.capture(f"after-{i}", values, "b") for i, values in enumerate(
            ([20, 40, 180], [22, 42, 182], [24, 44, 184]))]
        before, after, delta = (self.directory / name for name in ("before", "after", "delta"))
        for paths, output in ((baseline, before), (candidate, after)):
            result = self.invoke("report", *paths, "--output", output)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("3 run(s)", result.stdout)
        summary = json.loads((before / "summary.json").read_text())
        cpu = summary["cases"][0]["metrics"]["cpu_frame_ms"]
        self.assertEqual(cpu["run_medians"]["median"], 21)
        self.assertEqual(cpu["run_medians"]["mad"], 1)
        self.assertEqual(cpu["run_max"]["max"], 92)
        self.assertIn("Standalone reports", (before / "summary.txt").read_text())
        result = self.invoke("compare", before / "summary.json", after / "summary.json", "--output", delta)
        self.assertEqual(result.returncode, 0, result.stderr)
        metrics = json.loads((delta / "comparison.json").read_text())["cases"][0]["metrics"]
        self.assertEqual(metrics["cpu_frame_ms"]["delta_ms"], 21)
        self.assertEqual(metrics["cpu_frame_ms"]["delta_percent"], 100)
        self.assertIn("+100.00%", (delta / "comparison.txt").read_text())

    def test_claimed_validity_cannot_admit_impossible_execution(self):
        source = self.capture("false-mode", [10, 20, 90])
        raw = json.loads(source.read_text())
        raw["samples"][0]["counters"]["scene_renders"] = 0
        source.write_text(json.dumps(raw))
        output = self.directory / "rejected"
        result = self.invoke("report", source, "--output", output)
        self.assertNotEqual(result.returncode, 0)
        summary = json.loads((output / "summary.json").read_text())
        self.assertEqual(summary["accepted_run_count"], 0)
        self.assertEqual(summary["excluded_run_count"], 1)
        self.assertIn("EXCLUDED", result.stdout)

    def test_duplicate_copies_and_existing_outputs_cannot_inflate_or_replace_evidence(self):
        source = self.capture("first", [10, 20, 90])
        copy = self.directory / "copy.json"
        copy.write_text(json.dumps(json.loads(source.read_text()), separators=(",", ":")))
        result = self.invoke("report", source, copy, "--output", self.directory / "duplicates")
        self.assertNotEqual(result.returncode, 0)
        output = self.directory / "existing"
        output.mkdir()
        marker = output / "summary.txt"
        marker.write_text("preserved evidence")
        result = self.invoke("report", source, "--output", output)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(marker.read_text(), "preserved evidence")
