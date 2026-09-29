from http.client import HTTPConnection
from pathlib import Path
import re
import tempfile
import threading
import unittest
from unittest.mock import patch

from tools import docs_preview


class PreviewTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for name, content in {
            "docs/preview/index.html": "<h1>N3 guide</h1>",
            "docs/guide/README.md": "# Guide\n- [Editing](editing.md)\n- [Gizmo](gizmo.md)\n",
            "docs/guide/editing.md": "# Editing\n",
            "docs/guide/media/editing.webp": "image bytes",
            "README.md": "# N3\n",
            "LICENSE": "MIT License\n",
            ".git/config": "private",
            ".cache/private.md": "private",
            "settings.json": "private",
        }.items():
            destination = self.root / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_text(content, encoding="utf-8")

    def test_public_routes_and_root_shell(self):
        self.assertEqual(
            docs_preview.public_path(self.root, "/"),
            self.root.resolve() / "docs/preview/index.html",
        )
        for path in ["/README.md", "/LICENSE", "/docs/guide/editing.md?reload=1"]:
            self.assertIsNotNone(docs_preview.public_path(self.root, path))

    def test_private_paths_traversal_and_directories_are_not_served(self):
        for path in [
            "/.git/config", "/.cache/private.md", "/settings.json",
            "/docs/../README.md", "/docs/%2e%2e/README.md",
            "/docs/%2e%2e/%2e%2e/etc/passwd", "/docs/%00.md",
            "/docs\\guide\\editing.md", "/docs/guide", "/docs/missing.md",
        ]:
            with self.subTest(path=path):
                self.assertIsNone(docs_preview.public_path(self.root, path))

    def test_symlinks_cannot_escape_public_files(self):
        for name, target in [
            ("private.md", self.root / ".cache/private.md"),
            ("outside.md", self.root.parent),
            ("root.md", self.root),
            ("settings.md", self.root / "settings.json"),
        ]:
            (self.root / "docs" / name).symlink_to(target)
            self.assertIsNone(docs_preview.public_path(self.root, f"/docs/{name}"))

    def test_sidebar_follows_the_generated_index_with_absolute_links(self):
        self.assertEqual(
            docs_preview.sidebar(self.root),
            "- [Overview](/docs/guide/README.md)\n"
            "- [Editing](/docs/guide/editing.md)\n"
            "- [Gizmo](/docs/guide/gizmo.md)\n",
        )
        (self.root / docs_preview.GUIDE).write_text("- [New feature](new.md)\n")
        self.assertIn("[New feature](/docs/guide/new.md)", docs_preview.sidebar(self.root))
        self.assertNotIn("Editing", docs_preview.sidebar(self.root))

    def start_server(self):
        server = docs_preview.create_server(self.root)
        self.assertEqual(server.server_address[0], "127.0.0.1")
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()

        def stop():
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)

        self.addCleanup(stop)
        return server

    def request(self, server, path, method="GET"):
        connection = HTTPConnection("127.0.0.1", server.server_port, timeout=3)
        try:
            connection.request(method, path)
            response = connection.getresponse()
            return response.status, dict(response.getheaders()), response.read()
        finally:
            connection.close()

    def test_http_get_head_media_and_live_disk_changes(self):
        server = self.start_server()
        status, headers, body = self.request(server, "/")
        self.assertEqual(status, 200)
        self.assertIn(b"N3 guide", body)
        self.assertEqual(headers["Cache-Control"], "no-store")
        self.assertEqual(headers["Content-Type"], "text/html")
        status, headers, body = self.request(server, "/docs/guide/media/editing.webp", "HEAD")
        self.assertEqual(status, 200)
        self.assertEqual(headers["Content-Type"], "image/webp")
        self.assertEqual(int(headers["Content-Length"]), len("image bytes"))
        self.assertEqual(body, b"")
        (self.root / "docs/guide/editing.md").write_text("Updated guide")
        self.assertEqual(self.request(server, "/docs/guide/editing.md")[2], b"Updated guide")
        self.assertIn(b"/docs/guide/gizmo.md", self.request(server, "/_sidebar.md")[2])
        status, headers, body = self.request(server, "/LICENSE")
        self.assertEqual(status, 200)
        self.assertTrue(headers["Content-Type"].startswith("text/plain"))
        self.assertEqual(body, b"MIT License\n")

    def test_http_rejects_writes_and_private_files(self):
        server = self.start_server()
        with patch.object(docs_preview.PreviewHandler, "log_message"):
            self.assertEqual(self.request(server, "/.git/config")[0], 404)
            self.assertEqual(self.request(server, "/docs/guide")[0], 404)
            self.assertEqual(self.request(server, "/README.md", "POST")[0], 501)
        self.assertEqual((self.root / "README.md").read_text(), "# N3\n")

    def test_default_command_opens_browser_and_no_open_does_not(self):
        for args, should_open in [([], True), (["--no-open"], False)]:
            with self.subTest(args=args), patch("builtins.print"), patch.object(
                docs_preview.webbrowser, "open", return_value=True
            ) as open_browser, patch.object(
                docs_preview.ThreadingHTTPServer, "serve_forever", side_effect=KeyboardInterrupt
            ):
                self.assertEqual(docs_preview.main(args), 0)
                self.assertEqual(open_browser.called, should_open)
                if should_open:
                    self.assertRegex(
                        open_browser.call_args.args[0],
                        r"^http://127\.0\.0\.1:\d+/#/docs/guide/README$",
                    )

    def test_repository_shell_assets_and_generated_sidebar_links_exist(self):
        root = docs_preview.ROOT
        shell = (root / "docs/preview/index.html").read_text()
        assets = re.findall(r'(?:href|src)="(/[^\"]+)"', shell)
        self.assertGreaterEqual(len(assets), 2)
        links = re.findall(r"\]\((/[^)]+)\)", docs_preview.sidebar(root))
        self.assertGreater(len(links), 1)
        for route in assets + links:
            with self.subTest(route=route):
                self.assertIsNotNone(docs_preview.public_path(root, route))


if __name__ == "__main__":
    unittest.main()
