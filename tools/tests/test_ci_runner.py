import contextlib
import csv
import io
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from tools import ci_runner as runner


class CIRunnerTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve() / "repository with spaces, and comma"
        self.root.mkdir()
        for relative in runner.IMAGE_INPUTS:
            destination = self.root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_text(f"fixture for {relative}\n")
        (self.root / "docs/guide").mkdir(parents=True)
        self.identity = runner.image_identity(self.root)

    def invoke(self, arguments, results):
        output = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            with patch.object(runner.subprocess, "run", side_effect=results) as run:
                result = runner.main(arguments, root=self.root)
        return result, run, output.getvalue()

    def mounts(self, arguments):
        return [
            next(csv.reader([arguments[index + 1]]))
            for index, value in enumerate(arguments) if value == "--mount"
        ]

    def command(self, arguments):
        update, command = runner.parse_command(arguments)
        return runner.container_command(self.root, self.identity, update, command, 501, 20)

    def test_identity_tracks_every_image_input_but_not_application_source(self):
        (self.root / "source.rs").write_text("application edits use the source mount")
        self.assertEqual(runner.image_identity(self.root), self.identity)
        for relative in runner.IMAGE_INPUTS:
            with self.subTest(relative=relative):
                path = self.root / relative
                original = path.read_bytes()
                path.write_bytes(original + b"changed\n")
                self.assertNotEqual(runner.image_identity(self.root), self.identity)
                path.write_bytes(original)

    def test_build_pins_architecture_and_preserves_paths_as_arguments(self):
        arguments = runner.build_command(self.root, self.identity)
        self.assertEqual(arguments[:4], ["docker", "build", "--platform", "linux/amd64"])
        self.assertEqual(arguments[arguments.index("--file") + 1], str(self.root / "tools/ci/Dockerfile"))
        self.assertEqual(arguments[-1], str(self.root))
        self.assertEqual(arguments[arguments.index("--tag") + 1], f"n3-ci:{self.identity}")

    def test_check_keeps_sources_readonly_and_caches_writable(self):
        arguments = self.command(["docs", "check"])
        mounts = self.mounts(arguments)
        self.assertIn(["type=bind", f"source={self.root}", "target=/workspace", "readonly"], mounts)
        self.assertEqual(len(mounts), 4)
        self.assertFalse(any("target=/workspace/docs/guide" in mount for mount in mounts))
        self.assertEqual(arguments[arguments.index("--user") + 1], "501:20")
        self.assertIn("npm_config_cache=/n3-cache/home/.npm", arguments)
        for name in ("cargo", "target", "home"):
            self.assertIn([
                "type=bind", f"source={self.root / '.cache/ci/linux-amd64' / name}",
                f"target=/n3-cache/{name}",
            ], mounts)

    def test_update_adds_only_the_guide_as_a_writable_source_mount(self):
        checked = self.mounts(self.command(["docs", "check"]))
        updated = self.mounts(self.command(["docs", "update"]))
        self.assertEqual(updated[:-1], checked)
        self.assertEqual(updated[-1], [
            "type=bind", f"source={self.root / 'docs/guide'}", "target=/workspace/docs/guide",
        ])

    def test_test_arguments_are_passed_literally_and_never_interpolated_into_shell(self):
        supplied = ["filter with spaces", "--", "--exact", "$(touch sentinel);`false`"]
        arguments = self.command(["test", *supplied])
        self.assertEqual(arguments[-(len(supplied) + 3):], ["cargo", "test", "--locked", *supplied])
        script = arguments[arguments.index("-c") + 1]
        self.assertIn('exec "$@"', script)
        for value in supplied:
            self.assertNotIn(value, script)

    def test_ci_runs_full_checks_without_recursing_through_just_or_runner(self):
        arguments = self.command(["ci"])
        script = arguments[arguments.index("-c") + 1]
        for command in (
            "python3 tools/format_docs.py --check", "cargo fmt --check",
            "cargo clippy --locked --all-targets -- -D warnings",
            "python3 -m unittest discover -s tools/tests -p 'test_*.py'",
            "exec cargo test --locked",
        ):
            self.assertIn(command, script)
        self.assertNotIn("just", script)
        self.assertNotIn("ci_runner", script)
        self.assertIn("PYTHONDONTWRITEBYTECODE=1", arguments)
        self.assertIn("GALLIUM_OVERRIDE_CPU_CAPS=sse2", arguments)
        self.assertIn("LP_NUM_THREADS=2", arguments)

    def test_build_failure_stops_before_run_and_preserves_status(self):
        result, run, _ = self.invoke(["test"], [subprocess.CompletedProcess([], 7)])
        self.assertEqual(result, 7)
        self.assertEqual(run.call_count, 1)
        self.assertFalse((self.root / ".cache").exists())

    def test_verifier_status_is_propagated_and_caches_are_created(self):
        result, run, _ = self.invoke(["test"], [
            subprocess.CompletedProcess([], 0), subprocess.CompletedProcess([], 9),
        ])
        self.assertEqual(result, 9)
        self.assertEqual(run.call_count, 2)
        self.assertEqual(run.call_args_list[1].kwargs, {"cwd": self.root, "check": False})
        for name in ("cargo", "target", "home"):
            self.assertTrue((self.root / ".cache/ci/linux-amd64" / name).is_dir())

    def test_clean_checkout_needs_no_project_packages_and_prepares_writable_caches(self):
        project_packages = ("package.json", "package-lock.json", "node_modules")
        calls = []

        def docker(arguments, **kwargs):
            calls.append(arguments[1])
            for relative in project_packages:
                self.assertFalse((self.root / relative).exists())
            if arguments[1] == "run":
                for name in ("cargo", "target", "home"):
                    self.assertTrue((self.root / ".cache/ci/linux-amd64" / name).is_dir())
                self.assertIn("npm_config_cache=/n3-cache/home/.npm", arguments)
                self.assertFalse(any("target=/workspace/node_modules" in mount for mount in self.mounts(arguments)))
            return subprocess.CompletedProcess(arguments, 0)

        output = io.StringIO()
        with contextlib.redirect_stdout(output), patch.object(runner.subprocess, "run", side_effect=docker):
            self.assertEqual(runner.main(["ci"], root=self.root), 0)
        self.assertEqual(calls, ["build", "run"])

    def test_missing_docker_and_signal_status_fail_visibly(self):
        result, _, output = self.invoke(["ci"], [FileNotFoundError("docker is unavailable")])
        self.assertEqual(result, 1)
        self.assertIn("docker is unavailable", output)
        result, _, _ = self.invoke(["ci"], [subprocess.CompletedProcess([], -15)])
        self.assertEqual(result, 143)

    def test_bad_modes_and_update_symlinks_fail_before_docker(self):
        for arguments in ([], ["docs"], ["docs", "serve"], ["docs", "update", "extra"], ["ci", "extra"]):
            with self.subTest(arguments=arguments):
                result, run, output = self.invoke(arguments, [])
                self.assertEqual(result, 2)
                run.assert_not_called()
                self.assertIn("Usage:", output)
        guide = self.root / "docs/guide"
        guide.rmdir()
        guide.symlink_to(self.root / "docs", target_is_directory=True)
        result, run, output = self.invoke(["docs", "update"], [])
        self.assertEqual(result, 1)
        run.assert_not_called()
        self.assertIn("real docs/guide directory", output)


if __name__ == "__main__":
    unittest.main()
