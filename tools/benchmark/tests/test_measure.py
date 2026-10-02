import hashlib
import json
import os
from pathlib import Path
import tempfile
import threading
import unittest
from unittest.mock import MagicMock, patch

from tools.benchmark import measure, transport
from .support import capture_envelope




class MeasurementBuildTests(unittest.TestCase):
    def test_modes_default_and_forward_into_both_host_configurations(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "input.glb"
            source.write_bytes(b"fixture")
            for host in ("native", "web"):
                for mode in ("editor", "viewport", "renderer"):
                    output = root / f"{host}-{mode}.json"
                    arguments = ["--host", host, "--input", str(source), "--output", str(output), "--frames", "2"]
                    if mode != "editor":
                        arguments += ["--mode", mode]
                    expected = measure.options(measure.parser().parse_args(arguments))
                    self.assertEqual(expected["mode"], mode)

                    def native(command, **kwargs):
                        received = json.loads(kwargs["env"]["N3_VIEWPORT_MEASURE"])
                        self.assertEqual(received, expected)
                        output.write_text(json.dumps(capture_envelope(received)))

                    def browser(args, artifact, info):
                        received = measure.options(args)
                        self.assertEqual(received, expected)
                        output.write_text(json.dumps(capture_envelope(received)))

                    with self.subTest(host=host, mode=mode), patch.object(measure, "metadata", return_value={}), patch.object(
                        measure, "build", return_value=root / "artifact",
                    ), patch.object(measure, "require_browser_tools"), patch.object(measure.subprocess, "run", side_effect=native), patch.object(
                        measure, "run_browser", side_effect=browser,
                    ), patch("builtins.print"):
                        self.assertEqual(measure.main(arguments), 0)

    def test_renderer_selection_and_unknown_modes_fail_before_building(self):
        base = ["--host", "native", "--input", "input.glb", "--output", "report.json"]
        for extra in (["--mode", "renderer", "--selected"], ["--mode", "hidden"]):
            with self.subTest(extra=extra), patch.object(measure, "build") as build, self.assertRaises(SystemExit) as failed:
                measure.main(base + extra)
            self.assertEqual(failed.exception.code, 2)
            build.assert_not_called()
        for mode in ("editor", "viewport"):
            self.assertTrue(measure.options(measure.parser().parse_args(base + ["--mode", mode, "--selected"]))["selected"])

    def test_profiles_compile_real_host_with_feature_and_matching_artifact(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for relative in ("tools/benchmark/browser/measure.html", "tools/benchmark/browser/measure.js", "tools/benchmark/browser/observations.mjs", "assets/fonts/inter/LICENSE.txt", "assets/fonts/lucide/LICENSE"):
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(relative)
            generator = root / "wasm-bindgen"

            def run(command, **kwargs):
                if command[0] == str(generator):
                    output = Path(command[-1])
                    output.mkdir()
                    (output / "n3.js").write_text("glue")
                    (output / "n3_bg.wasm").write_bytes(b"wasm")

            with patch.dict(os.environ, {"CARGO_TARGET_DIR": "custom-target"}, clear=True), patch.object(
                measure.web, "require_bindgen", return_value=generator,
            ), patch.object(measure.subprocess, "run", side_effect=run) as process:
                for profile, directory in (("dev", "debug"), ("release", "release")):
                    executable = measure.build("native", profile, root)
                    self.assertEqual(executable, root / "custom-target" / directory / "n3")
                    native_command = process.call_args.args[0]
                    self.assertIn("viewport-measure", native_command)
                    self.assertEqual(native_command[-2:], ["--bin", "n3"])
                    self.assertEqual(native_command[native_command.index("--profile") + 1], profile)
                    site = measure.build("web", profile, root)
                    compile_call, glue_call = process.call_args_list[-2:]
                    self.assertEqual(compile_call.args[0][-3:], ["--target", measure.web.TARGET, "--lib"])
                    self.assertEqual(glue_call.args[0][1], str(root / "custom-target" / measure.web.TARGET / directory / "n3.wasm"))
                    self.assertEqual(site, root / "build/measure-web" / profile)
                    self.assertTrue((site / "pkg/n3_bg.wasm").is_file())
                    self.assertTrue((site / "licenses/Inter.txt").is_file())
                    self.assertTrue((site / "licenses/Lucide.txt").is_file())
                    self.assertFalse((root / "build/web").exists())

    def test_failed_glue_preserves_previous_measurement_site(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for relative in ("tools/benchmark/browser/measure.html", "tools/benchmark/browser/measure.js", "tools/benchmark/browser/observations.mjs", "assets/fonts/inter/LICENSE.txt", "assets/fonts/lucide/LICENSE", "build/measure-web/release/measure.html"):
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("previous")
            with patch.object(measure.web, "require_bindgen", return_value="generator"), patch.object(
                measure.subprocess, "run", side_effect=[None, OSError("bindgen failed")],
            ), self.assertRaisesRegex(OSError, "bindgen failed"):
                measure.build("web", "release", root)
            self.assertEqual((root / "build/measure-web/release/measure.html").read_text(), "previous")
            self.assertEqual(list((root / "build/measure-web").glob("measure-stage-*")), [])

    def test_metadata_records_actual_input_build_flags_and_unknowns(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "fixture.obj"
            source.write_bytes(b"one selected model")
            args = measure.parser().parse_args([
                "--host", "web", "--profile", "release", "--input", str(source),
                "--output", str(Path(temporary) / "report.json"),
            ])

            def version(command, **kwargs):
                if command == ["git", "rev-parse", "HEAD"]:
                    return "test-revision\n"
                if command[:2] == ["git", "ls-files"]:
                    return b"src/missing-file.rs\0"
                if command[0] == "git":
                    return " M src/render.rs\n"
                return "rustc test-version\n"

            with patch.object(measure.platform, "platform", return_value="test-os"), patch.object(measure.subprocess, "check_output", side_effect=version), patch.dict(
                os.environ, {"RUSTFLAGS": "-C target-cpu=native", "CARGO_PROFILE_RELEASE_LTO": "true"}, clear=True,
            ):
                info = measure.metadata(args)
            self.assertTrue(info["dirty"])
            self.assertEqual(info["revision"], "test-revision")
            self.assertEqual(info["input"], {"name": "fixture.obj", "bytes": 18, "sha256": hashlib.sha256(source.read_bytes()).hexdigest()})
            self.assertEqual(info["profile"], "release")
            self.assertEqual(info["rustflags"], "-C target-cpu=native")
            self.assertEqual(info["profile_overrides"], {"CARGO_PROFILE_RELEASE_LTO": "true"})
            self.assertIsNone(info["display_refresh_hz"])
            self.assertIsNone(info["observed_presentation_timestamps"])
            self.assertIsNone(info["gpu_timestamps"])
            self.assertNotIn(str(source), json.dumps(info))


    def test_source_fingerprint_tracks_bytes_modes_symlinks_and_deletions(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            first = root / "a.rs"
            first.write_text("first")
            second = root / "b.rs"
            second.write_text("second")
            link = root / "alias"
            link.symlink_to("a.rs")
            with patch.object(measure.subprocess, "check_output", return_value=b"b.rs\0a.rs\0alias\0"):
                baseline = measure.source_fingerprint(root)
            with patch.object(measure.subprocess, "check_output", return_value=b"a.rs\0alias\0b.rs\0a.rs\0"):
                self.assertEqual(baseline, measure.source_fingerprint(root))
                first.write_text("changed")
                changed = measure.source_fingerprint(root)
                self.assertNotEqual(baseline["sha256"], changed["sha256"])
                second.unlink()
                deleted = measure.source_fingerprint(root)
                self.assertNotEqual(changed["sha256"], deleted["sha256"])
                link.unlink()
                link.symlink_to("missing")
                self.assertNotEqual(deleted["sha256"], measure.source_fingerprint(root)["sha256"])

    def test_native_success_requires_complete_report_not_only_exit_code(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "input.obj"
            source.write_bytes(b"fixture")
            output = root / "report.json"
            arguments = ["--host", "native", "--input", str(source), "--output", str(output), "--frames", "2"]
            with patch.object(measure, "metadata", return_value={}), patch.object(measure, "build", return_value=root / "n3"), patch.object(
                measure.subprocess, "run", return_value=None,
            ), self.assertRaises(SystemExit) as missing:
                measure.main(arguments)
            self.assertEqual(missing.exception.code, 1)

            def incomplete(command, **kwargs):
                expected = json.loads(kwargs["env"]["N3_VIEWPORT_MEASURE"])
                report = capture_envelope(expected)
                report["samples"].pop()
                output.write_text(json.dumps(report))

            with patch.object(measure, "metadata", return_value={}), patch.object(measure, "build", return_value=root / "n3"), patch.object(
                measure.subprocess, "run", side_effect=incomplete,
            ), self.assertRaises(SystemExit) as partial:
                measure.main(arguments)
            self.assertEqual(partial.exception.code, 1)
            self.assertTrue(output.exists())

    def test_report_validation_rejects_bad_schema_counts_order_and_durations(self):
        options = {"sample_frames": 2, "mode": "editor"}
        valid = capture_envelope(options)
        self.assertIs(transport.validate_envelope(valid, options), valid)
        for field, value in (("schema", "unknown"), ("options", {"sample_frames": 1}), ("samples", [])):
            with self.subTest(field=field), self.assertRaises(ValueError):
                transport.validate_envelope({**valid, field: value}, options)
        for sample in ({"frame": 2, "cpu_frame_ms": 1}, {"frame": 1, "cpu_frame_ms": -1}, {"frame": 1, "cpu_frame_ms": True}, {"frame": 1, "cpu_frame_ms": float("inf")}):
            with self.subTest(sample=sample), self.assertRaises(ValueError):
                transport.validate_envelope({**valid, "samples": [valid["samples"][0], sample]}, options)

    def test_report_requires_mode_contract_but_preserves_comparison_contamination(self):
        options = {"sample_frames": 1, "mode": "viewport"}
        report = capture_envelope(options)
        for validity in (None, {}, {"mode_contract_satisfied": False}, {"mode_contract_satisfied": 1}):
            with self.subTest(validity=validity), self.assertRaisesRegex(ValueError, "mode contract"):
                transport.validate_envelope({**report, "validity": validity}, options)
        report["validity"].update({"all_frames_focused": False, "constant_viewport": False})
        self.assertIs(transport.validate_envelope(report, options), report)
        self.assertFalse(report["validity"]["all_frames_focused"])


class MeasurementBrowserTests(unittest.TestCase):
    def arguments(self, root, *extra):
        return measure.parser().parse_args([
            "--host", "web", "--input", str(root / "input.obj"),
            "--output", str(root / "report.json"), *extra,
        ])

    def server(self):
        server = MagicMock()
        server.__enter__.return_value = server
        server.url = "http://127.0.0.1:1234/test-token/measure.html"
        server.completed = threading.Event()
        return server

    def test_ambient_selenium_and_debug_aliases_cannot_redirect_local_browser(self):
        root = Path("/test-workspace")
        inherited = {
            "SELENIUM_REMOTE_URL": "https://remote.example/session",
            "npm_config_selenium_remote_url": "https://other.example/session",
            "npm_package_config_selenium_remote_url": "https://package.example/session",
            "PWDEBUG": "1",
            "npm_config_pwdebug": "console",
            "npm_package_config_pwdebug": "1",
            "PLAYWRIGHT_BROWSERS_PATH": "/personal-browser-cache",
            "HTTPS_PROXY": "http://proxy.example:8080",
        }
        with patch.dict(os.environ, inherited, clear=True):
            environment = measure.browser_environment(root)
            self.assertEqual(dict(os.environ), inherited)
        for key in inherited:
            if "selenium" in key.lower() or "pwdebug" in key.lower():
                self.assertNotIn(key, environment)
        self.assertEqual(environment["PLAYWRIGHT_BROWSERS_PATH"], str(root / ".cache/viewport-browsers"))
        self.assertEqual(environment["HTTPS_PROXY"], inherited["HTTPS_PROXY"])

    def test_optional_setup_pins_package_and_keeps_browsers_in_repository_cache(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(measure.subprocess, "run") as process, patch.object(
                measure, "require_browser_tools",
            ) as require, patch("builtins.print"), patch.dict(os.environ, {}, clear=True):
                measure.setup_browser(root)
            install, browser = process.call_args_list
            self.assertEqual(install.args[0], [
                "npm", "install", "--prefix", str(root / ".cache/viewport-tools"),
                "--ignore-scripts", "--no-audit", "--no-fund", "--save-exact",
                "playwright@1.63.0",
            ])
            self.assertEqual(browser.args[0], [
                "node", str(root / ".cache/viewport-tools/node_modules/playwright/cli.js"),
                "install", "chromium", "--no-shell",
            ])
            for call in process.call_args_list:
                self.assertEqual(call.kwargs["env"]["PLAYWRIGHT_BROWSERS_PATH"], str(root / ".cache/viewport-browsers"))
                self.assertEqual(call.kwargs["cwd"], root)
            require.assert_called_once_with(root)

    def test_missing_mismatched_or_external_browser_fails_before_compilation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.assertRaisesRegex(RuntimeError, "setup-browser"):
                measure.require_browser_tools(root)
            package = root / ".cache/viewport-tools/node_modules/playwright"
            package.mkdir(parents=True)
            (package / "cli.js").write_text("cli")
            (package / "package.json").write_text('{"version":"wrong"}')
            with self.assertRaisesRegex(RuntimeError, "mismatched"):
                measure.require_browser_tools(root)
            (package / "package.json").write_text(json.dumps({"version": measure.PLAYWRIGHT_VERSION}))
            external = root / "personal-browser"
            external.write_text("not an owned executable")
            with patch.object(measure.subprocess, "check_output", return_value=str(external)), self.assertRaisesRegex(RuntimeError, "setup-browser"):
                measure.require_browser_tools(root)
            installed = root / ".cache/viewport-browsers/chromium/browser"
            installed.parent.mkdir(parents=True)
            installed.write_text("owned executable")
            with patch.object(measure.subprocess, "check_output", return_value=str(installed)):
                measure.require_browser_tools(root)
            (root / "input.obj").write_text("fixture")
            arguments = ["--host", "web", "--input", str(root / "input.obj"), "--output", str(root / "report.json")]
            with patch.object(measure, "require_browser_tools", side_effect=RuntimeError("missing setup-browser")), patch.object(
                measure, "build",
            ) as build, self.assertRaises(SystemExit) as failed:
                measure.main(arguments)
            self.assertEqual(failed.exception.code, 1)
            build.assert_not_called()

    def test_runner_success_waits_for_report_response_and_browser_exit(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            args = self.arguments(root, "--mode", "viewport")
            server = self.server()
            server.handle_request.side_effect = lambda: server.completed.set()
            process = MagicMock()
            process.poll.side_effect = [None, None, 0, 0]
            with patch.object(measure, "MeasurementServer", return_value=server) as server_factory, patch.object(
                measure.subprocess, "Popen", return_value=process,
            ) as launch, patch("builtins.print"):
                measure.run_browser(args, root / "site", {}, root)
            self.assertEqual(server.handle_request.call_count, 2)
            self.assertEqual(server_factory.call_args.args[3]["options"], measure.options(args))
            self.assertEqual(server_factory.call_args.args[3]["options"]["mode"], "viewport")
            self.assertEqual(launch.call_args.args[0], [
                "node", str(root / "tools/benchmark/browser/runner.mjs"), server.url,
                "headless", "1280", "800", "2.0", "600",
            ])
            self.assertEqual(launch.call_args.kwargs["env"]["PLAYWRIGHT_BROWSERS_PATH"], str(root / ".cache/viewport-browsers"))
            process.terminate.assert_not_called()

    def test_child_failure_and_zero_exit_without_report_fail_immediately(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for code, expected in ((7, "status 7"), (0, "without a complete report")):
                server = self.server()
                process = MagicMock()
                process.poll.return_value = code
                with self.subTest(code=code), patch.object(measure, "MeasurementServer", return_value=server), patch.object(
                    measure.subprocess, "Popen", return_value=process,
                ), patch("builtins.print"), self.assertRaisesRegex(RuntimeError, expected):
                    measure.run_browser(self.arguments(root), root / "site", {}, root)
                server.handle_request.assert_not_called()

    def test_timeout_and_interrupt_close_only_the_owned_runner(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            server = self.server()
            process = MagicMock()
            process.poll.return_value = None
            with patch.object(measure, "MeasurementServer", return_value=server), patch.object(
                measure.subprocess, "Popen", return_value=process,
            ), patch.object(measure.time, "monotonic", side_effect=[0, 2]), patch("builtins.print"), self.assertRaisesRegex(RuntimeError, "timed out"):
                measure.run_browser(self.arguments(root, "--timeout", "1"), root / "site", {}, root)
            process.terminate.assert_called_once()
            process.wait.assert_called_once_with(timeout=45)
            process.kill.assert_not_called()
            process.reset_mock()
            server.handle_request.side_effect = KeyboardInterrupt
            with patch.object(measure, "MeasurementServer", return_value=server), patch.object(
                measure.subprocess, "Popen", return_value=process,
            ), patch("builtins.print"), self.assertRaises(KeyboardInterrupt):
                measure.run_browser(self.arguments(root), root / "site", {}, root)
            process.terminate.assert_called_once()
            process.wait.assert_called_once_with(timeout=45)

    def test_stuck_runner_cleanup_is_bounded(self):
        process = MagicMock()
        process.poll.return_value = None
        process.wait.side_effect = [measure.subprocess.TimeoutExpired("node", 45), 0]
        measure.stop_browser_runner(process)
        process.terminate.assert_called_once()
        process.kill.assert_called_once()
        self.assertEqual([call.kwargs["timeout"] for call in process.wait.call_args_list], [45, 5])

    def test_invalid_browser_options_fail_without_build_or_launch(self):
        base = ["--host", "web", "--input", "fixture.obj", "--output", "report.json"]
        for extra in (["--open"], ["--device-scale-factor", "0"], ["--device-scale-factor", "nan"], ["--device-scale-factor", "5"]):
            with self.subTest(extra=extra), patch.object(measure, "build") as build, patch.object(
                measure.subprocess, "Popen",
            ) as launch, self.assertRaises(SystemExit) as failed:
                measure.main(base + extra)
            self.assertEqual(failed.exception.code, 2)
            build.assert_not_called()
            launch.assert_not_called()
        self.assertEqual(self.arguments(Path("."), "--browser", "manual", "--open").browser, "manual")




if __name__ == "__main__":
    unittest.main()
