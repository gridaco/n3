import contextlib
import io
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from tools import format_docs


class FormatDocsTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name) / "repository with spaces $(not-a-command)"
        self.templates = self.root / "docs/templates"
        self.templates.mkdir(parents=True)
        (self.root / ".oxfmtrc.json").write_text("{}\n", encoding="utf-8")

    def template(self, name, source):
        path = self.templates / name
        path.write_text(source, encoding="utf-8")
        return path

    def invoke(self, arguments, outputs=None, regular_status=0):
        outputs = outputs or {}

        def formatter(command, **kwargs):
            source = kwargs.get("input")
            if source is None:
                return subprocess.CompletedProcess(command, regular_status)
            result = outputs.get(source, source)
            status, formatted = result if isinstance(result, tuple) else (0, result)
            return subprocess.CompletedProcess(command, status, stdout=formatted)

        output = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            with patch.object(format_docs.subprocess, "run", side_effect=formatter) as run:
                status = format_docs.main(arguments, root=self.root)
        return status, run, output.getvalue()

    def formatter_arguments(self, call):
        command = call.args[0]
        self.assertIsInstance(command, list)
        self.assertEqual(command[0], "npx")
        self.assertRegex(format_docs.OXFMT_VERSION, r"\A\d+\.\d+\.\d+\Z")
        expected_package = f"oxfmt@{format_docs.OXFMT_VERSION}"
        self.assertEqual([value for value in command if value.startswith("oxfmt@")], [expected_package])
        package = command.index(expected_package)
        for flag in ("--yes", "--ignore-scripts", "--prefer-offline", "--prefix"):
            self.assertIn(flag, command[:package])
        self.assertEqual(command[command.index("--prefix") + 1], call.kwargs["env"]["npm_config_cache"])
        self.assertFalse(call.kwargs.get("shell", False))
        self.assertEqual(call.kwargs["cwd"], self.root)
        return command[package + 1:]

    def test_check_reports_all_template_drift_without_writing(self):
        a = self.template("a.md.in", "First  \n")
        b = self.template("b.md.in", "Second  \n")
        status, _, output = self.invoke(["--check"], {
            "First  \n": "First\n", "Second  \n": "Second\n",
        })
        self.assertEqual(status, 1)
        self.assertIn("Unformatted: docs/templates/a.md.in", output)
        self.assertIn("Unformatted: docs/templates/b.md.in", output)
        self.assertEqual(a.read_text(), "First  \n")
        self.assertEqual(b.read_text(), "Second  \n")

    def test_check_accepts_formatted_templates_and_preserves_literal_bindings(self):
        source = "Press {{shortcut:tool.move}}, then {{control:preferences.open}}.\n"
        path = self.template("keys.md.in", source)
        status, _, _ = self.invoke(["--check"])
        self.assertEqual(status, 0)
        self.assertEqual(path.read_text(), source)

    def test_write_updates_all_valid_templates_without_creating_virtual_markdown(self):
        original = "# Steps  \n\nPress {{shortcut:tool.move}}.  \n"
        formatted = "# Steps\n\nPress {{shortcut:tool.move}}.\n"
        a = self.template("a.md.in", original)
        b = self.template("b.md.in", "Other  \n")
        status, _, _ = self.invoke(["--write"], {
            original: formatted, "Other  \n": "Other\n",
        })
        self.assertEqual(status, 0)
        self.assertEqual(a.read_text(), formatted)
        self.assertEqual(b.read_text(), "Other\n")
        self.assertEqual(sorted(path.name for path in self.templates.iterdir()), ["a.md.in", "b.md.in"])

    def test_late_formatter_failure_prevents_every_template_write(self):
        a = self.template("a.md.in", "First  \n")
        b = self.template("b.md.in", "Second\n")
        status, run, output = self.invoke(["--write"], {
            "First  \n": "First\n", "Second\n": (17, "partial output"),
        })
        self.assertEqual(status, 17)
        self.assertEqual(run.call_count, 3)
        self.assertIn("Formatting failed: docs/templates/b.md.in", output)
        self.assertEqual(a.read_text(), "First  \n")
        self.assertEqual(b.read_text(), "Second\n")

    def test_binding_reorder_drop_duplicate_and_replacement_prevent_all_writes(self):
        first = "{{shortcut:tool.move}}"
        second = "{{control:preferences.open}}"
        source = f"{first} then {second}, then {first}.\n"
        a = self.template("a.md.in", "First  \n")
        b = self.template("b.md.in", source)
        for changed in (
            f"{second} then {first}, then {first}.\n",
            f"{first} then {second}.\n",
            f"{first} then {second}, then {first} {first}.\n",
            source.replace("tool.move", "tool.orbit"),
        ):
            with self.subTest(changed=changed):
                status, _, output = self.invoke(["--write"], {
                    "First  \n": "First\n", source: changed,
                })
                self.assertEqual(status, 1)
                self.assertIn("changed guide bindings: docs/templates/b.md.in", output)
                self.assertEqual(a.read_text(), "First  \n")
                self.assertEqual(b.read_text(), source)

    def test_regular_formatter_failure_stops_before_template_formatting(self):
        path = self.template("a.md.in", "Unformatted  \n")
        status, run, _ = self.invoke(["--write"], regular_status=23)
        self.assertEqual(status, 23)
        self.assertEqual(run.call_count, 1)
        self.assertEqual(path.read_text(), "Unformatted  \n")

    def test_missing_template_directory_fails_instead_of_reporting_zero_checked(self):
        self.templates.rmdir()
        for mode in ("--check", "--write"):
            with self.subTest(mode=mode):
                status, run, output = self.invoke([mode])
                self.assertEqual(status, 1)
                self.assertEqual(run.call_count, 1)
                self.assertIn("Documentation formatting failed", output)
                self.assertNotIn("Guide templates: 0 checked", output)
                self.assertFalse(self.templates.exists())

    def test_template_stdin_uses_shared_config_and_a_markdown_path_without_shell(self):
        source = "# Literal $(not-a-command) and `code`\n\n{{image:example}}\n"
        path = self.template("literal.md.in", source)
        status, run, _ = self.invoke(["--check"])
        self.assertEqual(status, 0)
        self.assertEqual(run.call_count, 2)
        for call in run.call_args_list:
            self.formatter_arguments(call)
        regular, template = run.call_args_list
        self.assertEqual(self.formatter_arguments(regular), [
            "--config", str(self.root / ".oxfmtrc.json"), "--check", ".",
        ])
        self.assertEqual(self.formatter_arguments(template), [
            "--config", str(self.root / ".oxfmtrc.json"),
            "--stdin-filepath", str(path.with_suffix("")),
        ])
        self.assertEqual(template.kwargs["input"], source)
        self.assertEqual(template.kwargs["encoding"], "utf-8")
        self.assertEqual(template.kwargs["stdout"], subprocess.PIPE)

    def test_prepare_only_fetches_the_exact_formatter_version(self):
        self.template("bad.md.in", "{{shortcut:keep-this}}\n")
        status, run, _ = self.invoke(["--prepare"])
        self.assertEqual(status, 0)
        self.assertEqual(run.call_count, 1)
        self.assertEqual(self.formatter_arguments(run.call_args), ["--version"])
        self.assertIsNone(run.call_args.kwargs["input"])

    def test_native_cache_defaults_to_the_repository_without_mutating_environment(self):
        with patch.dict(os.environ, {}, clear=True):
            status, run, _ = self.invoke(["--prepare"])
            self.assertEqual(status, 0)
            self.assertEqual(
                run.call_args.kwargs["env"]["npm_config_cache"], str(self.root / ".cache/npm")
            )
            self.formatter_arguments(run.call_args)
            self.assertNotIn("npm_config_cache", os.environ)

    def test_explicit_npm_cache_override_is_preserved(self):
        with patch.dict(os.environ, {"npm_config_cache": "/writable/container cache"}):
            status, run, _ = self.invoke(["--prepare"])
            self.assertEqual(status, 0)
            self.assertEqual(
                run.call_args.kwargs["env"]["npm_config_cache"], "/writable/container cache"
            )
            self.formatter_arguments(run.call_args)
            self.assertEqual(os.environ["npm_config_cache"], "/writable/container cache")

    def test_missing_npx_fails_visibly(self):
        output = io.StringIO()
        with contextlib.redirect_stderr(output), patch.object(
            format_docs.subprocess, "run", side_effect=FileNotFoundError("npx is unavailable")
        ):
            self.assertEqual(format_docs.main(["--prepare"], root=self.root), 1)
        self.assertIn("npx is unavailable", output.getvalue())

    def test_invalid_modes_fail_without_starting_formatter(self):
        for arguments in ([], ["--update"], ["--check", "extra"], ["--prepare", "--write"]):
            with self.subTest(arguments=arguments):
                status, run, output = self.invoke(arguments)
                self.assertEqual(status, 2)
                run.assert_not_called()
                self.assertIn("Usage:", output)


if __name__ == "__main__":
    unittest.main()
