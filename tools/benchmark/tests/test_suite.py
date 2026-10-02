import copy
import io
import json
from pathlib import Path
import tempfile
import unittest
from contextlib import ExitStack, redirect_stderr, redirect_stdout
from unittest.mock import patch

from tools.benchmark import suite as bench


def source_info():
    return {"source_fingerprint": {"sha256": "source"}, "input": {"sha256": "fixture"},
            "rustc": "compiler", "profile": "release", "rustflags": None,
            "cargo_encoded_rustflags": None, "profile_overrides": {},
            "operating_system": "test-os", "architecture": "arm64"}


class BenchmarkSuiteTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.input = self.root / "chess.glb"
        self.input.write_bytes(b"test scene")
        self.output = self.root / "suite"
        self.arguments = ["run", "--input", str(self.input), "--output", str(self.output),
                          "--repeats", "3", "--frames", "2", "--warmup", "1"]

    def mocked_runner(self, stack):
        stack.enter_context(redirect_stdout(io.StringIO()))
        stack.enter_context(redirect_stderr(io.StringIO()))
        metadata = stack.enter_context(patch.object(bench.measure, "metadata", side_effect=lambda _: source_info()))
        def build_artifact(host, *unused, **kwargs):
            artifact = self.root / host
            artifact.write_bytes(host.encode())
            return artifact

        build = stack.enter_context(patch.object(bench.measure, "build", side_effect=build_artifact))
        browser = stack.enter_context(patch.object(bench.measure, "require_browser_tools"))

        def run(args, artifact, info, **kwargs):
            self.assertEqual(artifact.name, args.host)
            args.output.write_text(json.dumps({"options": bench.measure.options(args)}))

        prepared = stack.enter_context(patch.object(bench.measure, "run_prepared", side_effect=run))
        analyze = stack.enter_context(patch.object(bench.reports, "analyze", return_value={"excluded_runs": []}))
        stack.enter_context(patch.object(bench.reports, "render_text", return_value="summary\n"))
        return metadata, build, browser, prepared, analyze

    def test_serial_rounds_build_each_host_once_and_retain_evidence(self):
        with ExitStack() as stack:
            _, build, browser, run, analyze = self.mocked_runner(stack)
            self.assertEqual(bench.main(self.arguments), 0)
        self.assertEqual(build.call_count, 2)
        browser.assert_called_once()
        self.assertEqual(run.call_count, 18)
        self.assertEqual(len(set(call.args[0].output for call in run.call_args_list)), 18)
        for call in run.call_args_list:
            args, _, info = call.args
            self.assertEqual(args.browser, "headed" if args.host == "web" else "headless")
            self.assertEqual(args.frames, 2)
            self.assertEqual(args.warmup, 1)
            self.assertIn("suite", info)
        manifest = json.loads((self.output / "manifest.json").read_text())
        self.assertEqual(manifest["status"], "complete")
        for repeat in range(1, 4):
            cases = [(entry["host"], entry["mode"]) for entry in manifest["runs"] if entry["repeat"] == repeat]
            self.assertEqual(len(cases), 6)
            self.assertEqual(len(set(cases)), 6)
        self.assertTrue(all(entry["status"] == "complete" for entry in manifest["runs"]))
        self.assertEqual(len(analyze.call_args.args[0]), 18)
        self.assertTrue((self.output / "summary.txt").is_file())
        self.assertTrue((self.output / "summary.json").is_file())

    def test_case_order_is_seeded_and_not_grouped_by_host_across_repeats(self):
        args = bench.parser().parse_args(self.arguments)
        plan = bench.plan_cases(args)
        self.assertEqual(plan, bench.plan_cases(args))
        args.seed += 1
        self.assertNotEqual(plan, bench.plan_cases(args))
        self.assertEqual([entry["repeat"] for entry in plan], [1] * 6 + [2] * 6 + [3] * 6)

    def test_invalid_case_and_existing_output_fail_before_build(self):
        for extra in (["--selected"], ["--repeats", "0"], ["--hosts", "native", "native"], ["--frames", "0"]):
            with ExitStack() as stack:
                _, build, _, run, _ = self.mocked_runner(stack)
                with self.assertRaises(SystemExit) as failed:
                    bench.main(self.arguments + extra)
                self.assertEqual(failed.exception.code, 2)
                build.assert_not_called()
                run.assert_not_called()
                self.assertFalse(self.output.exists())
        self.output.mkdir()
        sentinel = self.output / "keep.txt"
        sentinel.write_text("old evidence")
        with ExitStack() as stack:
            _, build, _, run, _ = self.mocked_runner(stack)
            with self.assertRaises(SystemExit) as failed:
                bench.main(self.arguments)
            self.assertEqual(failed.exception.code, 1)
            build.assert_not_called()
            run.assert_not_called()
        self.assertEqual(sentinel.read_text(), "old evidence")

    def test_failed_shutdown_does_not_aggregate_leftover_report_or_retry(self):
        with ExitStack() as stack:
            _, _, _, run, analyze = self.mocked_runner(stack)
            count = 0

            def incomplete(args, *unused, **kwargs):
                nonlocal count
                count += 1
                args.output.write_text("{}")
                if count == 2:
                    raise RuntimeError("browser teardown failed")

            run.side_effect = incomplete
            self.assertEqual(bench.main(self.arguments), 1)
        self.assertEqual(run.call_count, 2)
        self.assertEqual(len(analyze.call_args.args[0]), 1)
        manifest = json.loads((self.output / "manifest.json").read_text())
        self.assertEqual(manifest["status"], "failed")
        self.assertEqual([entry["status"] for entry in manifest["runs"]][:3], ["complete", "failed", "pending"])
        self.assertTrue((self.output / manifest["runs"][1]["report"]).exists())

    def test_source_change_during_session_stops_and_does_not_accept_that_run(self):
        with ExitStack() as stack:
            metadata, _, _, run, analyze = self.mocked_runner(stack)
            changed = copy.deepcopy(source_info())
            changed["source_fingerprint"]["sha256"] = "edited during measurement"
            metadata.side_effect = [source_info(), source_info(), source_info(), changed]
            self.assertEqual(bench.main(self.arguments), 1)
        self.assertEqual(run.call_count, 1)
        analyze.assert_not_called()
        manifest = json.loads((self.output / "manifest.json").read_text())
        self.assertEqual(manifest["runs"][0]["status"], "failed")
        self.assertIn("source_fingerprint", manifest["error"])

    def test_contamination_is_reported_without_silent_retries(self):
        with ExitStack() as stack:
            _, _, _, run, analyze = self.mocked_runner(stack)
            analyze.return_value = {"excluded_runs": [{"reason": "focus lost"}]}
            self.assertEqual(bench.main(self.arguments), 1)
        self.assertEqual(run.call_count, 18)
        self.assertEqual(json.loads((self.output / "manifest.json").read_text())["status"], "contaminated")

    def test_aggregation_failure_is_not_a_complete_suite(self):
        with ExitStack() as stack:
            _, _, _, _, analyze = self.mocked_runner(stack)
            analyze.side_effect = ValueError("incompatible environment")
            with self.assertRaises(SystemExit) as failed:
                bench.main(self.arguments)
            self.assertEqual(failed.exception.code, 1)
        manifest = json.loads((self.output / "manifest.json").read_text())
        self.assertEqual(manifest["status"], "failed")
        self.assertIn("Aggregation failed", manifest["error"])
        self.assertTrue(all(entry["status"] == "complete" for entry in manifest["runs"]))

    def test_build_artifacts_are_frozen_and_tampering_rejects_the_run(self):
        with ExitStack() as stack:
            _, _, _, run, analyze = self.mocked_runner(stack)

            def tamper(args, artifact, *unused, **kwargs):
                self.assertTrue(artifact.is_relative_to(self.output / "artifacts"))
                self.assertNotEqual(artifact, self.root / args.host)
                args.output.write_text("{}")
                artifact.write_bytes(b"changed executable")

            run.side_effect = tamper
            self.assertEqual(bench.main(self.arguments), 1)
        analyze.assert_not_called()
        manifest = json.loads((self.output / "manifest.json").read_text())
        self.assertIn("Prepared artifact changed", manifest["error"])
        self.assertEqual(manifest["runs"][0]["status"], "failed")

    def test_web_snapshot_preserves_full_site_and_detects_modification(self):
        site = self.root / "site"
        (site / "pkg").mkdir(parents=True)
        (site / "pkg/n3.wasm").write_bytes(b"wasm")
        (site / "measure.html").write_text("html")
        frozen, fingerprint = bench.snapshot_artifact(site, self.root / "frozen")
        self.assertEqual(bench.artifact_fingerprint(frozen), fingerprint)
        (site / "pkg/n3.wasm").write_bytes(b"new build")
        self.assertEqual((frozen / "pkg/n3.wasm").read_bytes(), b"wasm")
        self.assertNotEqual(bench.artifact_fingerprint(site), fingerprint)


if __name__ == "__main__":
    unittest.main()
