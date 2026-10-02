from http.client import HTTPConnection
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import unittest
from unittest.mock import patch

from tools import web


class WebBuildTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "workspace with 'spaces'"
        self.root.mkdir()
        self.version = "0.2.129"
        (self.root / "Cargo.toml").write_text(
            f'wasm-bindgen = "={self.version}"\n', encoding="utf-8",
        )
        self.generator = self.root / ".cache/web-tools/bin/wasm-bindgen"
        self.environment = patch.dict(os.environ, {}, clear=True)
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def install_generator(self):
        self.generator.parent.mkdir(parents=True, exist_ok=True)
        self.generator.write_bytes(b"test generator")

    def prepare_site(self):
        for name, content in {
            "web/index.html": "new wrapper",
            "web/n3.js": "new interface",
            "assets/fonts/inter/LICENSE.txt": "Inter notice",
            "assets/fonts/lucide/LICENSE": "Lucide notice",
            "build/web/index.html": "previous runnable wrapper",
            "build/web/pkg/n3_bg.wasm": "previous runnable WASM",
        }.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")

    def test_cold_setup_installs_exact_manifest_version_and_optional_target(self):
        def run(command, **kwargs):
            if command[:2] == ["cargo", "install"]:
                self.install_generator()

        with patch.object(web.subprocess, "run", side_effect=run) as process, patch.object(
            web.subprocess, "check_output", return_value=f"wasm-bindgen {self.version}\n",
        ):
            web.setup(self.root)

        target, install = process.call_args_list
        self.assertEqual(target.args[0], ["rustup", "target", "add", web.TARGET])
        self.assertEqual(install.args[0], [
            "cargo", "install", "wasm-bindgen-cli", "--version", self.version,
            "--locked", "--root", str(self.root / ".cache/web-tools"), "--force",
        ])
        self.assertEqual(install.kwargs["cwd"], self.root)
        self.assertEqual(
            install.kwargs["env"]["CARGO_TARGET_DIR"], str(self.root / "target/web-tools"),
        )
        self.assertNotIn("CARGO_TARGET_DIR", os.environ)

    def test_matching_tool_cache_still_ensures_target_without_reinstalling(self):
        self.install_generator()
        with patch.object(web.subprocess, "run") as process, patch.object(
            web.subprocess, "check_output", return_value=f"wasm-bindgen {self.version}\n",
        ):
            web.setup(self.root)
        self.assertEqual(process.call_count, 1)
        self.assertEqual(process.call_args.args[0], ["rustup", "target", "add", web.TARGET])

    def test_stale_tool_cache_is_replaced_and_new_version_verified(self):
        self.install_generator()
        with patch.object(web.subprocess, "run") as process, patch.object(
            web.subprocess, "check_output",
            side_effect=["wasm-bindgen 0.2.1\n", f"wasm-bindgen {self.version}\n"],
        ) as version:
            web.setup(self.root)
        self.assertEqual(process.call_count, 2)
        self.assertIn("--force", process.call_args.args[0])
        self.assertEqual(version.call_count, 2)

    def test_failed_tool_replacement_does_not_report_success(self):
        self.install_generator()
        with patch.object(web.subprocess, "run"), patch.object(
            web.subprocess, "check_output", return_value="wasm-bindgen 0.2.1\n",
        ), self.assertRaisesRegex(RuntimeError, "Expected wasm-bindgen"):
            web.setup(self.root)

    def test_mismatched_generator_preserves_site_and_does_not_compile(self):
        self.install_generator()
        self.prepare_site()
        with patch.object(web.subprocess, "run") as process, patch.object(
            web.subprocess, "check_output", return_value="wasm-bindgen 0.2.1\n",
        ), self.assertRaisesRegex(RuntimeError, "Expected wasm-bindgen"):
            web.build(self.root)
        process.assert_not_called()
        self.assertEqual(
            (self.root / "build/web/index.html").read_text(), "previous runnable wrapper",
        )

    def test_compile_and_glue_failures_preserve_previous_site_and_remove_staging(self):
        self.install_generator()
        self.prepare_site()
        for failure in ["cargo", str(self.generator)]:
            with self.subTest(failure=failure):
                def run(command, **kwargs):
                    if command[0] == failure:
                        raise subprocess.CalledProcessError(1, command)

                with patch.object(web.subprocess, "run", side_effect=run), patch.object(
                    web.subprocess, "check_output", return_value=f"wasm-bindgen {self.version}",
                ), self.assertRaises(subprocess.CalledProcessError):
                    web.build(self.root)
                self.assertEqual(
                    (self.root / "build/web/index.html").read_text(), "previous runnable wrapper",
                )
                self.assertEqual(
                    (self.root / "build/web/pkg/n3_bg.wasm").read_text(), "previous runnable WASM",
                )
                self.assertEqual(list((self.root / "build").glob("web-stage-*")), [])

    def test_successful_build_honors_cargo_overrides_and_ships_font_notices(self):
        self.install_generator()
        self.prepare_site()
        cargo_home = str(self.root / "custom cargo")
        target_dir = str(self.root / "custom target")

        def run(command, **kwargs):
            if command[0] == str(self.generator):
                output = Path(command[command.index("--out-dir") + 1])
                output.mkdir()
                (output / "n3.js").write_text("new glue", encoding="utf-8")
                (output / "n3_bg.wasm").write_bytes(b"new WASM")

        with patch.dict(os.environ, {"CARGO_HOME": cargo_home, "CARGO_TARGET_DIR": target_dir}), patch.object(
            web.subprocess, "run", side_effect=run,
        ) as process, patch.object(
            web.subprocess, "check_output", return_value=f"wasm-bindgen {self.version}",
        ), patch("builtins.print"):
            web.build(self.root)
            self.assertEqual(os.environ["CARGO_TARGET_DIR"], target_dir)
        compile_step, glue = process.call_args_list
        self.assertEqual(compile_step.kwargs["env"]["CARGO_HOME"], cargo_home)
        self.assertEqual(compile_step.kwargs["env"]["CARGO_TARGET_DIR"], target_dir)
        self.assertEqual(glue.args[0][1], str(Path(target_dir) / web.TARGET / "web/n3.wasm"))
        site = self.root / "build/web"
        self.assertEqual((site / "index.html").read_text(), "new wrapper")
        self.assertEqual((site / "n3.js").read_text(), "new interface")
        self.assertEqual((site / "pkg/n3_bg.wasm").read_bytes(), b"new WASM")
        self.assertEqual((site / "licenses/Inter.txt").read_text(), "Inter notice")
        self.assertEqual((site / "licenses/Lucide.txt").read_text(), "Lucide notice")
        self.assertEqual(list((self.root / "build").glob("web-stage-*")), [])


class WebServerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for name, content in {
            "index.html": b"<canvas></canvas>",
            "pkg/n3.js": b"export default function init() {}",
            "pkg/n3_bg.wasm": b"\x00asm\x01\x00\x00\x00",
            ".private": b"private",
        }.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)

    def test_routes_reject_traversal_directories_and_symlink_escape(self):
        self.assertEqual(web.public_path(self.root, "/"), self.root.resolve() / "index.html")
        self.assertEqual(
            web.public_path(self.root, "/pkg/n3.js?revision=2"),
            self.root.resolve() / "pkg/n3.js",
        )
        (self.root / "outside").symlink_to(self.root.parent)
        for route in [
            "/.private", "/pkg", "/missing", "/../index.html", "/%2e%2e/index.html",
            "/pkg/../index.html", "/pkg\\n3.js", "/%00", "/outside/anything",
        ]:
            with self.subTest(route=route):
                self.assertIsNone(web.public_path(self.root, route))

    def test_browser_mime_cache_head_and_read_only_contract(self):
        server = web.create_server(self.root)
        self.assertEqual(server.server_address[0], "127.0.0.1")
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            for method, route, status, mime in [
                ("GET", "/pkg/n3_bg.wasm", 200, "application/wasm"),
                ("HEAD", "/pkg/n3.js", 200, "text/javascript; charset=utf-8"),
                ("POST", "/index.html", 501, None),
                ("GET", "/.private", 404, None),
            ]:
                connection = HTTPConnection("127.0.0.1", server.server_port, timeout=3)
                try:
                    with patch.object(web.WebHandler, "log_message"):
                        connection.request(method, route)
                        response = connection.getresponse()
                        body = response.read()
                    self.assertEqual(response.status, status)
                    if mime is not None:
                        self.assertEqual(response.getheader("Content-Type"), mime)
                        self.assertEqual(response.getheader("Cache-Control"), "no-store")
                    if method == "HEAD":
                        self.assertEqual(body, b"")
                    if route.endswith(".wasm"):
                        self.assertEqual(body, b"\x00asm\x01\x00\x00\x00")
                finally:
                    connection.close()
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)


if __name__ == "__main__":
    unittest.main()
