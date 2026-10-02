"""Build and locally serve N3's WebAssembly application host."""

import argparse
import functools
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import mimetypes
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tempfile
from urllib.parse import unquote, urlsplit
import webbrowser


ROOT = Path(__file__).resolve().parents[1]
TARGET = "wasm32-unknown-unknown"


def bindgen_version(root):
    # The macOS system Python is 3.9, so avoid requiring tomllib or a package.
    manifest = (root / "Cargo.toml").read_text(encoding="utf-8")
    versions = re.findall(r'^wasm-bindgen = "=([0-9.]+)"$', manifest, re.MULTILINE)
    if len(versions) != 1:
        raise RuntimeError("Cargo.toml must pin one exact wasm-bindgen version.")
    return versions[0]


def cargo_environment(root):
    environment = os.environ.copy()
    environment.setdefault("CARGO_HOME", str(root / ".cache/cargo"))
    environment.setdefault("CARGO_TARGET_DIR", str(root / "target"))
    return environment


def require_bindgen(root):
    generator = root / ".cache/web-tools/bin/wasm-bindgen"
    if not generator.is_file():
        raise RuntimeError("Browser tooling is missing. Run `just web-setup` first.")
    version = subprocess.check_output([str(generator), "--version"], text=True).strip()
    expected = f"wasm-bindgen {bindgen_version(root)}"
    if version != expected:
        raise RuntimeError(f"Expected {expected}, found {version}. Run `just web-setup`.")
    return generator


def setup(root=ROOT):
    version = bindgen_version(root)
    environment = cargo_environment(root)
    # rustup resolves the checked-in toolchain in this directory, including on
    # cold CI runners. Keep optional WASM setup out of the native verifier.
    subprocess.run(
        ["rustup", "target", "add", TARGET], cwd=root, env=environment, check=True,
    )
    try:
        require_bindgen(root)
        return
    except (OSError, RuntimeError, subprocess.CalledProcessError):
        pass
    environment["CARGO_TARGET_DIR"] = str(Path(environment["CARGO_TARGET_DIR"]) / "web-tools")
    subprocess.run([
        "cargo", "install", "wasm-bindgen-cli", "--version", version, "--locked",
        "--root", str(root / ".cache/web-tools"), "--force",
    ], cwd=root, env=environment, check=True)
    require_bindgen(root)


def build(root=ROOT, *, profile="web"):
    if profile not in ("web", "release"):
        raise ValueError("Browser build profile must be web or release")
    environment = cargo_environment(root)
    generator = require_bindgen(root)
    subprocess.run(
        ["cargo", "build", "--locked", "--target", TARGET, "--lib", "--profile", profile],
        cwd=root, env=environment, check=True,
    )
    # Build in a staging directory so failures preserve the last runnable site.
    output = root / "build/web"
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="web-stage-", dir=output.parent) as temporary:
        stage = Path(temporary) / "site"
        shutil.copytree(root / "web", stage)
        licenses = stage / "licenses"
        licenses.mkdir(exist_ok=True)
        shutil.copyfile(root / "assets/fonts/inter/LICENSE.txt", licenses / "Inter.txt")
        shutil.copyfile(root / "assets/fonts/lucide/LICENSE", licenses / "Lucide.txt")
        artifact = Path(environment["CARGO_TARGET_DIR"]) / TARGET / profile / "n3.wasm"
        subprocess.run([
            str(generator), str(artifact), "--target", "web", "--out-name", "n3",
            "--out-dir", str(stage / "pkg"),
        ], cwd=root, check=True)
        if output.exists():
            shutil.rmtree(output)
        stage.rename(output)
    print(f"Browser build: {output}", flush=True)


def public_path(root, request_path):
    decoded = unquote(urlsplit(request_path).path)
    if not decoded.startswith("/") or "\x00" in decoded or "\\" in decoded:
        return None
    relative = PurePosixPath(decoded.lstrip("/") or "index.html")
    if any(part.startswith(".") for part in relative.parts):
        return None
    try:
        target = (root / relative).resolve(strict=True)
        target.relative_to(root.resolve())
    except (OSError, RuntimeError, ValueError):
        return None
    return target if target.is_file() else None


class WebHandler(BaseHTTPRequestHandler):
    def __init__(self, *args, root, **kwargs):
        self.root = root
        super().__init__(*args, **kwargs)

    def do_GET(self):
        self.respond(include_body=True)

    def do_HEAD(self):
        self.respond(include_body=False)

    def respond(self, include_body):
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
                ".wasm": "application/wasm",
                ".js": "text/javascript; charset=utf-8",
                ".json": "application/json",
            }.get(path.suffix) or mimetypes.guess_type(path.name)[0] or "application/octet-stream"
            self.send_response(200)
            self.send_header("Content-Type", mime)
            self.send_header("Content-Length", str(path.stat().st_size))
            self.end_headers()
            if include_body:
                shutil.copyfileobj(source, self.wfile)

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        super().end_headers()

    def log_message(self, format, *args):
        if len(args) > 1 and str(args[1]) != "200":
            super().log_message(format, *args)


def create_server(root=ROOT / "build/web", port=0):
    return ThreadingHTTPServer(
        ("127.0.0.1", port), functools.partial(WebHandler, root=root)
    )


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("setup", help="install the optional target and matching glue generator")
    build_parser = commands.add_parser("build", help="compile WASM and copy the authored web wrapper")
    build_parser.add_argument("--profile", choices=("web", "release"), default="web")
    serve = commands.add_parser("serve", help="serve the existing build on localhost")
    serve.add_argument("--port", type=int, default=8000)
    serve.add_argument("--no-open", action="store_true")
    args = parser.parse_args(argv)
    try:
        if args.command == "setup":
            setup()
            return 0
        if args.command == "build":
            build(profile=args.profile)
            return 0
        if not 0 <= args.port <= 65535:
            parser.error("port must be between 0 and 65535")
        if not (ROOT / "build/web/pkg/n3_bg.wasm").is_file():
            raise RuntimeError("Browser build is missing. Run `just web-build` first.")
        with create_server(port=args.port) as server:
            url = f"http://127.0.0.1:{server.server_port}/"
            print(f"N3 web: {url}\nCtrl-C stops the server.", flush=True)
            if not args.no_open:
                webbrowser.open(url)
            server.serve_forever()
    except KeyboardInterrupt:
        return 0
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Web host failed: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
