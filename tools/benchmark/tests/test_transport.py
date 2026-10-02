from http.client import HTTPConnection
import json
from pathlib import Path
import tempfile
import threading
import unittest

from tools.benchmark import transport
from .support import capture_envelope


class MeasurementServerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.site = self.root / "site"
        self.site.mkdir()
        (self.site / "measure.html").write_text("measurement page")
        (self.site / "measure.js").write_text("import './pkg/n3.js'")
        (self.site / "pkg").mkdir()
        (self.site / "pkg/n3.js").write_text("wasm glue")
        (self.site / "pkg/n3_bg.wasm").write_bytes(b"wasm")
        self.source = self.root / "user model.obj"
        self.source.write_bytes(b"private selected bytes")
        self.output = self.root / "report.json"
        self.server = transport.MeasurementServer(self.site, self.source, self.output, {"options": {"sample_frames": 1, "mode": "editor"}}, token="test-token")
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.addCleanup(self.stop)

    def stop(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()

    def request(self, endpoint, method="GET", body=None, headers=None, raw=False):
        connection = HTTPConnection("127.0.0.1", self.server.server_port, timeout=5)
        connection.request(method, endpoint if raw else f"/test-token/{endpoint}", body, headers or {})
        response = connection.getresponse()
        status, response_headers, content = response.status, dict(response.getheaders()), response.read()
        connection.close()
        return status, response_headers, content

    def valid_report(self):
        return capture_envelope(self.server.config["options"])

    def post(self, body, headers=None):
        return self.request("report", "POST", body, headers or {
            "Origin": self.server.origin, "Content-Type": "application/json",
        })[0]

    def test_only_selected_input_and_explicit_site_allowlist_are_readable(self):
        for endpoint in ("measure.html", "measure.js", "pkg/n3.js", "pkg/n3_bg.wasm", "config.json", "input"):
            with self.subTest(endpoint=endpoint):
                status, headers, body = self.request(endpoint)
                self.assertEqual(status, 200)
                self.assertEqual(headers["Cache-Control"], "no-store")
                if endpoint == "input":
                    self.assertEqual(body, b"private selected bytes")
        (self.site / "other.txt").write_text("not public")
        for endpoint in ("../user%20model.obj", "%2e%2e/user%20model.obj", "other.txt", "", "pkg/../measure.html", "input/other"):
            with self.subTest(endpoint=endpoint):
                self.assertEqual(self.request(endpoint)[0], 404)
        self.assertEqual(self.request("/wrong-token/input", raw=True)[0], 404)
        self.assertEqual(self.request("/test-token-other/input", raw=True)[0], 404)
        (self.site / "measure.js").unlink()
        (self.site / "measure.js").symlink_to(self.source)
        self.assertEqual(self.request("measure.js")[0], 404)

    def test_host_and_origin_prevent_cross_origin_access(self):
        self.assertEqual(self.request("input", headers={"Host": "attacker.example"})[0], 403)
        self.assertEqual(self.request("input", headers={"Origin": "https://attacker.example"})[0], 403)
        self.assertEqual(self.post('{"samples":[{}]}', {"Content-Type": "application/json"}), 403)
        self.assertEqual(self.post('{"samples":[{}]}', {"Content-Type": "application/json", "Origin": "http://localhost:1"}), 403)
        self.assertFalse(self.output.exists())

    def test_one_bounded_json_report_writes_only_preselected_output(self):
        for body in ("not json", "[]", "{}", '{"samples":[]}', '{"samples":[{"elapsed":NaN}]}', '{"samples":[{"elapsed":1e999}]}'):
            with self.subTest(body=body):
                self.assertEqual(self.post(body), 400)
                self.assertFalse(self.output.exists())
        self.assertEqual(self.post("{}", {"Origin": self.server.origin, "Content-Type": "text/plain"}), 415)
        self.assertEqual(self.post("", {"Origin": self.server.origin, "Content-Type": "application/json", "Content-Length": str(transport.MAX_REPORT_BYTES + 1)}), 413)
        report = self.valid_report()
        report["output"] = str(self.root / "unselected.json")
        self.assertEqual(self.post(json.dumps(report)), 200)
        self.assertTrue(self.server.completed.wait(2))
        self.assertEqual(json.loads(self.output.read_text()), report)
        self.assertFalse((self.root / "unselected.json").exists())
        self.assertEqual(self.post(json.dumps(report)), 409)

    def test_invalid_mode_report_is_rejected_without_consuming_output(self):
        report = self.valid_report()
        report["validity"]["mode_contract_satisfied"] = False
        self.assertEqual(self.post(json.dumps(report)), 400)
        self.assertFalse(self.output.exists())
        self.assertFalse(self.server.completed.is_set())
        report["validity"] = {"mode_contract_satisfied": True, "all_frames_focused": False}
        self.assertEqual(self.post(json.dumps(report)), 200)
        self.assertFalse(json.loads(self.output.read_text())["validity"]["all_frames_focused"])

    def test_existing_file_and_symlink_are_not_overwritten(self):
        self.output.write_text("earlier evidence")
        self.assertEqual(self.post(json.dumps(self.valid_report())), 409)
        self.assertEqual(self.output.read_text(), "earlier evidence")
        self.output.unlink()
        self.output.symlink_to(self.source)
        self.assertEqual(self.post(json.dumps(self.valid_report())), 409)
        self.assertEqual(self.source.read_bytes(), b"private selected bytes")
        self.assertFalse(self.server.completed.is_set())
