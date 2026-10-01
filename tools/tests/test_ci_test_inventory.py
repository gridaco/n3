import contextlib
import io
import subprocess
import unittest
from unittest.mock import patch

from tools import ci_test_inventory as inventory


def listing(names):
    return "\n".join([*(f"{name}: test" for name in names), "", f"{len(names)} tests, 0 benchmarks", ""])


class CITestInventoryTests(unittest.TestCase):
    def test_complete_partition_counts_ignored_tests_and_similarly_named_regressions(self):
        guide = inventory.GUIDE_TEST
        checks = ["asset_io::gltf::tests::fixture_evaluation_timings", guide + "_regression"]
        outputs = [listing([guide, *checks]), listing([guide]), listing(checks), listing([])]
        results = [subprocess.CompletedProcess([], 0, stdout=text) for text in outputs]
        output = io.StringIO()
        with patch.object(inventory.subprocess, "run", side_effect=results) as run, contextlib.redirect_stdout(output):
            self.assertEqual(inventory.main([]), 0)
        self.assertEqual(tuple(call.args[0] for call in run.call_args_list), inventory.LIST_COMMANDS)
        self.assertTrue(all(call.kwargs == {"stdout": subprocess.PIPE, "text": True, "check": False} for call in run.call_args_list))
        self.assertIn("3 total = 1 guide + 2 checks (including ignored tests)", output.getvalue())

    def test_ignored_mandatory_guide_fails_even_when_the_partition_is_complete(self):
        guide = inventory.GUIDE_TEST
        checks = ["model::tests::roundtrip"]
        outputs = [listing([guide, *checks]), listing([guide]), listing(checks), listing([guide])]
        results = [subprocess.CompletedProcess([], 0, stdout=text) for text in outputs]
        output = io.StringIO()
        with patch.object(inventory.subprocess, "run", side_effect=results), contextlib.redirect_stderr(output):
            self.assertEqual(inventory.main([]), 1)
        self.assertIn("mandatory full-guide test must not be ignored", output.getvalue())

    def test_unknown_missing_or_overlapping_tests_fail_the_partition(self):
        guide = inventory.GUIDE_TEST
        full = {guide, "model::tests::roundtrip"}
        for selected, checks in (
            (set(), full),
            ({guide, "model::tests::roundtrip"}, set()),
            ({guide}, full),
            ({guide}, set()),
            ({guide}, {"model::tests::roundtrip", "model::tests::unexpected"}),
        ):
            with self.subTest(guide=selected, checks=checks), self.assertRaises(inventory.InventoryError):
                inventory.validate_partition(full, selected, checks)

    def test_duplicate_malformed_or_inconsistent_libtest_output_is_rejected(self):
        for output in (
            "",
            "model::tests::roundtrip: test\n",
            "model::tests::roundtrip: test\n0 tests, 0 benchmarks\n",
            "model::tests::roundtrip: test\nmodel::tests::roundtrip: test\n2 tests, 0 benchmarks\n",
            "model::tests::roundtrip: ignored\n1 test, 0 benchmarks\n",
            "model::tests::roundtrip: test\n1 test, 1 benchmark\n",
            "model::tests::roundtrip: test\n1 test, 0 benchmarks\n1 test, 0 benchmarks\n",
            "0 tests, 0 benchmarks\nmodel::tests::late: test\n",
            "unexpected output\n0 tests, 0 benchmarks\n",
        ):
            with self.subTest(output=output), self.assertRaises(inventory.InventoryError):
                inventory.parse_inventory(output)
        self.assertEqual(inventory.parse_inventory("\n0 tests, 0 benchmarks\n\n"), set())
        self.assertEqual(inventory.parse_inventory("root_test: test\n\n1 test, 0 benchmarks\n"), {"root_test"})

    def test_failed_discovery_stops_before_later_commands_and_reports_failure(self):
        result = subprocess.CompletedProcess([], 9, stdout="")
        output = io.StringIO()
        with patch.object(inventory.subprocess, "run", return_value=result) as run, contextlib.redirect_stderr(output):
            self.assertEqual(inventory.main([]), 1)
        self.assertEqual(run.call_count, 1)
        self.assertIn("failed with status 9", output.getvalue())

    def test_invalid_inventory_or_missing_cargo_returns_failure(self):
        for result in (subprocess.CompletedProcess([], 0, stdout="unexpected output"), OSError("cargo missing")):
            with self.subTest(result=result):
                output = io.StringIO()
                kwargs = {"side_effect": result} if isinstance(result, OSError) else {"return_value": result}
                with patch.object(inventory.subprocess, "run", **kwargs), contextlib.redirect_stderr(output):
                    self.assertEqual(inventory.main([]), 1)
                self.assertIn("N3 CI test inventory:", output.getvalue())

    def test_arguments_cannot_change_the_fixed_inventory_commands(self):
        for arguments in (["--ignored"], ["--skip", inventory.GUIDE_TEST], ["guide;false"]):
            with self.subTest(arguments=arguments), patch.object(inventory.subprocess, "run") as run, contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(inventory.main(arguments), 2)
            run.assert_not_called()
