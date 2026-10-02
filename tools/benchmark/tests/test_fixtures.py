"""Verify the maintained benchmark asset's pinned bytes without regenerating it."""

import hashlib
import os
from pathlib import Path
import subprocess
import sys
import unittest


ROOT = Path(__file__).resolve().parents[3]
CHESS_SET = ROOT / "fixtures/benchmarks/chess-set"


def fixture_snapshot():
    return {
        path.relative_to(CHESS_SET).as_posix(): (
            hashlib.sha256(path.read_bytes()).hexdigest(), path.stat().st_mtime_ns,
        )
        for path in CHESS_SET.rglob("*") if path.is_file()
    }


class BenchmarkFixtureTests(unittest.TestCase):
    def test_chess_source_hashes_and_exact_glb_reproduction_are_read_only(self):
        before = fixture_snapshot()
        # No --write: the recipe checks every source against provenance.json,
        # then compares the packed bytes with both the pinned hash and stored GLB.
        result = subprocess.run(
            [sys.executable, str(CHESS_SET / "pack.py")],
            cwd=ROOT, env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
            stdin=subprocess.DEVNULL, capture_output=True, text=True,
            check=False, timeout=30,
        )
        self.assertEqual(fixture_snapshot(), before, "Fixture verification must not write files")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
