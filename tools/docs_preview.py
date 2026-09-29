"""Read-only local preview of N3's existing Markdown and generated guide media."""

import argparse
import functools
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import mimetypes
from pathlib import Path, PurePosixPath
import re
import shutil
from urllib.parse import unquote, urlsplit
import webbrowser


ROOT = Path(__file__).resolve().parents[1]
GUIDE = "docs/guide/README.md"
PUBLIC_DIRECTORIES = {"docs", "src", "examples", "fixtures", "assets"}
PUBLIC_FILES = {"README.md", "AGENTS.md", "CONTRIBUTING.md", "LICENSE"}


def public_path(root, request_path):
    """Serve public project material only; never expose caches or directory lists."""
    decoded = unquote(urlsplit(request_path).path)
    if not decoded.startswith("/") or "\x00" in decoded or "\\" in decoded:
        return None
    relative = PurePosixPath(decoded.lstrip("/"))
    if any(part.startswith(".") for part in relative.parts):
        return None
    if not relative.parts:
        relative = PurePosixPath("docs/preview/index.html")
    if relative.parts[0] not in PUBLIC_DIRECTORIES and str(relative) not in PUBLIC_FILES:
        return None
    root = root.resolve()
    try:
        target = (root / relative).resolve(strict=True)
        resolved = target.relative_to(root)
    except (OSError, RuntimeError, ValueError):
        return None
    if not target.is_file():
        return None
    # A symlink must not bypass the public-route boundary either.
    if any(part.startswith(".") for part in resolved.parts):
        return None
    if resolved.parts[0] not in PUBLIC_DIRECTORIES and str(resolved) not in PUBLIC_FILES:
        return None
    return target


def sidebar(root):
    """Derive navigation from the generated index, not a second feature registry.

    Docsify resolves sidebar links against the active page. Absolute links keep
    navigation working when following a guide link into the contributor docs.
    The bullet format here is owned by our Rust guide generator.
    """
    index = (root / GUIDE).read_text(encoding="utf-8")
    links = re.findall(r"^- \[([^\]\n]+)\]\(([^)\n]+\.md)\)$", index, re.MULTILINE)
    return "- [Overview](/docs/guide/README.md)\n" + "".join(
        f"- [{label}](/docs/guide/{target})\n" for label, target in links
    )


class PreviewHandler(BaseHTTPRequestHandler):
    def __init__(self, *args, root=ROOT, **kwargs):
        self.root = root
        super().__init__(*args, **kwargs)

    def do_GET(self):
        self.respond(include_body=True)

    def do_HEAD(self):
        self.respond(include_body=False)

    def respond(self, include_body):
        if urlsplit(self.path).path == "/_sidebar.md":
            try:
                body = sidebar(self.root).encode("utf-8")
            except OSError:
                self.send_error(404, "Guide index is missing")
                return
            self.headers_for("text/markdown; charset=utf-8", len(body))
            if include_body:
                self.wfile.write(body)
            return
        path = public_path(self.root, self.path)
        if path is None:
            self.send_error(404)
            return
        try:
            source = path.open("rb")
        except OSError:
            self.send_error(404)
            return
        with source:
            mime = {
                ".md": "text/markdown; charset=utf-8",
                ".rs": "text/plain; charset=utf-8",
                ".webp": "image/webp",
            }.get(path.suffix) or mimetypes.guess_type(path.name)[0] or "text/plain; charset=utf-8"
            self.headers_for(mime, path.stat().st_size)
            if include_body:
                shutil.copyfileobj(source, self.wfile)

    def headers_for(self, content_type, size):
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(size))
        self.end_headers()

    def end_headers(self):
        # Refresh after `just docs update` without stale browser assets.
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        super().end_headers()

    def log_message(self, format, *args):
        # Keep the foreground command quiet; show failed requests for diagnosis.
        if len(args) > 1 and str(args[1]) != "200":
            super().log_message(format, *args)


def create_server(root=ROOT, port=0):
    return ThreadingHTTPServer(
        ("127.0.0.1", port), functools.partial(PreviewHandler, root=root)
    )


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=0, help="fixed port (default: choose a free port)")
    parser.add_argument("--no-open", action="store_true", help="print URL without opening a browser")
    args = parser.parse_args(argv)
    if not 0 <= args.port <= 65535:
        parser.error("port must be between 0 and 65535")
    try:
        server = create_server(port=args.port)
    except OSError as error:
        parser.exit(1, f"Cannot start docs preview: {error}\n")
    with server:
        url = f"http://127.0.0.1:{server.server_port}/#/docs/guide/README"
        print(f"N3 docs: {url}\nRefresh after regenerating docs. Ctrl-C stops the server.", flush=True)
        if not args.no_open:
            try:
                opened = webbrowser.open(url)
            except webbrowser.Error:
                opened = False
            if not opened:
                print("Browser could not be opened; use the URL above.", flush=True)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
