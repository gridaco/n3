import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock


SPEC = importlib.util.spec_from_file_location("lgtmd_web_verify", Path(__file__).resolve().parents[1] / "verify.py")
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)


class PortablePayloadTests(unittest.TestCase):
    def test_copy_has_only_portable_sources_and_preserves_fixture_paths(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "source"
            destination = Path(temporary) / "copy"
            root.mkdir()
            (root / "src").mkdir()
            (root / "src/private-n3.rs").write_text("not portable", encoding="utf-8")
            (root / "Cargo.lock").write_text("host lock", encoding="utf-8")
            (root / "package.json").write_text(json.dumps({"workspaces": ["packages/lgtmd", "examples/lgtmd-web"]}), encoding="utf-8")
            (root / "package-lock.json").write_text("{}", encoding="utf-8")
            for relative in VERIFY.SOURCES:
                package = root / relative
                package.mkdir(parents=True)
                (package / "LICENSE").write_text("license", encoding="utf-8")
                for disposable in (".cache", "target", "node_modules", "dist", "native"):
                    (package / disposable).mkdir()
                    (package / disposable / "stale").write_text("do not copy", encoding="utf-8")
            fixture = root / "crates/lgtmd/tests/fixtures/target/.ownership.json"
            fixture.parent.mkdir(parents=True)
            fixture.write_text("fixture bytes", encoding="utf-8")
            VERIFY.payload(root, destination)
            self.assertEqual((destination / "Cargo.toml").read_text(encoding="utf-8"), VERIFY.WORKSPACE)
            self.assertFalse((destination / "Cargo.lock").exists())
            self.assertFalse((destination / "src").exists())
            self.assertEqual((destination / "crates/lgtmd/tests/fixtures/target/.ownership.json").read_text(encoding="utf-8"), "fixture bytes")
            for relative in VERIFY.SOURCES:
                self.assertTrue((destination / relative / "LICENSE").exists())
                for disposable in (".cache", "target", "node_modules", "dist", "native"):
                    self.assertFalse((destination / relative / disposable).exists())

    def test_relative_explicit_cargo_overrides_keep_the_invocation_base(self):
        with tempfile.TemporaryDirectory() as temporary, mock.patch.dict(os.environ, {"CARGO_HOME": "cache/cargo", "CARGO_TARGET_DIR": "cache/target", "npm_config_cache": "cache/npm"}):
            environment = VERIFY.environment_for(Path(temporary))
            self.assertEqual(environment["CARGO_HOME"], str(Path.cwd() / "cache/cargo"))
            self.assertEqual(environment["CARGO_TARGET_DIR"], str(Path.cwd() / "cache/target"))
            self.assertEqual(environment["npm_config_cache"], str(Path.cwd() / "cache/npm"))


if __name__ == "__main__":
    unittest.main()
