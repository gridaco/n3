"""Bounded local transport and capture envelopes, not performance-data admission.

The collector preserves raw reports. contracts.py independently checks the complete
record and observed facts before any saved sample enters statistical analysis.
"""

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
import secrets
import shutil
import threading
from urllib.parse import urlsplit

MAX_REPORT_BYTES = 32 * 1024 * 1024
STATIC_FILES = {
    "measure.html": "text/html; charset=utf-8",
    "measure.js": "text/javascript; charset=utf-8",
    "observations.mjs": "text/javascript; charset=utf-8",
    "pkg/n3.js": "text/javascript; charset=utf-8",
    "pkg/n3_bg.wasm": "application/wasm",
    "licenses/Inter.txt": "text/plain; charset=utf-8",
    "licenses/Lucide.txt": "text/plain; charset=utf-8",
}



def validate_envelope(report, expected_options):
    if not isinstance(report, dict) or report.get("schema") != "n3.viewport-measure.v2":
        raise ValueError("Expected an n3.viewport-measure.v2 report")
    if report.get("options") != expected_options:
        raise ValueError("Report options do not match this run")
    samples = report.get("samples")
    if not isinstance(samples, list) or len(samples) != expected_options["sample_frames"]:
        raise ValueError("Report does not contain the requested number of samples")
    for index, sample in enumerate(samples):
        if not isinstance(sample, dict) or sample.get("frame") != index:
            raise ValueError("Report sample indices must be consecutive")
        elapsed = sample.get("cpu_frame_ms")
        if type(elapsed) not in (int, float) or elapsed < 0:
            raise ValueError("Report contains an invalid CPU frame duration")
    validity = report.get("validity")
    if not isinstance(validity, dict) or validity.get("mode_contract_satisfied") is not True:
        raise ValueError("Report does not satisfy the requested measurement mode contract")
    # Reject JSON's permissive NaN/Infinity inputs, including overflowing floats.
    json.dumps(report, allow_nan=False)
    return report


def write_report(output, report):
    # Exclusive creation protects earlier evidence and prevents a late symlink
    # replacement from redirecting this one explicitly selected output file.
    with output.open("x", encoding="utf-8") as destination:
        json.dump(report, destination, indent=2, allow_nan=False)
        destination.write("\n")


class MeasurementServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, root, selected_input, output, config, port=0, token=None):
        self.root = root.resolve()
        self.selected_input = selected_input
        self.output = output
        self.config = config
        self.token = token or secrets.token_urlsafe(24)
        self.completed = threading.Event()
        self.report_lock = threading.Lock()
        super().__init__(("127.0.0.1", port), MeasurementHandler)
        self.origin = f"http://127.0.0.1:{self.server_port}"
        self.url = f"{self.origin}/{self.token}/measure.html"


class MeasurementHandler(BaseHTTPRequestHandler):
    def setup(self):
        super().setup()
        self.connection.settimeout(15)

    def endpoint(self):
        expected_host = f"127.0.0.1:{self.server.server_port}"
        if self.headers.get("Host") != expected_host:
            self.send_error(403)
            return None
        origin = self.headers.get("Origin")
        if origin is not None and origin != self.server.origin:
            self.send_error(403)
            return None
        parsed = urlsplit(self.path)
        prefix = f"/{self.server.token}/"
        if parsed.scheme or parsed.netloc or not parsed.path.startswith(prefix):
            self.send_error(404)
            return None
        return parsed.path[len(prefix):]

    def do_GET(self):
        endpoint = self.endpoint()
        if endpoint is None:
            return
        if endpoint == "config.json":
            self.send_bytes(json.dumps(self.server.config).encode(), "application/json")
        elif endpoint == "input":
            self.send_file(self.server.selected_input, "application/octet-stream")
        elif endpoint in STATIC_FILES:
            path = self.server.root / endpoint
            # Do not let a symlink in generated output expose another directory.
            try:
                path.resolve(strict=True).relative_to(self.server.root)
            except (OSError, RuntimeError, ValueError):
                self.send_error(404)
                return
            self.send_file(path, STATIC_FILES[endpoint])
        else:
            self.send_error(404)

    def send_file(self, path, content_type):
        try:
            source = path.open("rb")
        except OSError:
            self.send_error(404)
            return
        with source:
            self.send_response(200)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(os.fstat(source.fileno()).st_size))
            self.end_headers()
            shutil.copyfileobj(source, self.wfile)

    def send_bytes(self, body, content_type, status=200):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        endpoint = self.endpoint()
        if endpoint is None:
            return
        if endpoint != "report":
            self.send_error(404)
            return
        # Reports require same-origin JS, not a cross-origin HTML form. The
        # capability token also guards selected model bytes and configuration.
        if self.headers.get("Origin") != self.server.origin:
            self.send_error(403)
            return
        if self.headers.get_content_type() != "application/json" or self.headers.get("Transfer-Encoding"):
            self.send_error(415)
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
        except ValueError:
            self.send_error(400)
            return
        if not 0 < length <= MAX_REPORT_BYTES:
            self.send_error(413)
            return
        try:
            body = self.rfile.read(length)
            if len(body) != length:
                raise ValueError("Incomplete report")
            report = json.loads(body)
            validate_envelope(report, self.server.config["options"])
        except (ValueError, UnicodeError, RecursionError):
            self.send_error(400)
            return
        with self.server.report_lock:
            if self.server.completed.is_set():
                self.send_error(409)
                return
            try:
                write_report(self.server.output, report)
            except OSError:
                self.send_error(409)
                return
            try:
                self.send_bytes(b'{"saved":true}', "application/json")
            finally:
                self.server.completed.set()

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Referrer-Policy", "no-referrer")
        self.send_header("Cross-Origin-Resource-Policy", "same-origin")
        super().end_headers()

    def log_message(self, format, *args):
        # Never print the capability token or paths to a user's selected model.
        pass
