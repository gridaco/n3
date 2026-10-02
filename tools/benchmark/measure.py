"""Build and measure the real native or browser viewport into a local JSON report."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile
import time
import webbrowser

from tools import web
from .transport import MAX_REPORT_BYTES, MeasurementServer, validate_envelope, write_report

ROOT = Path(__file__).resolve().parents[2]
PLAYWRIGHT_VERSION = "1.63.0"


def browser_environment(root=ROOT):
    environment = os.environ.copy()
    # Playwright reads these before launch, including npm's lowercase aliases.
    # Ambient debugging or Selenium configuration must not redirect this owned
    # local run or silently change a requested headless run into headed mode.
    for name in ("SELENIUM_REMOTE_URL", "PWDEBUG"):
        for key in (name, f"npm_config_{name.lower()}", f"npm_package_config_{name.lower()}"):
            environment.pop(key, None)
    environment["PLAYWRIGHT_BROWSERS_PATH"] = str(root / ".cache/viewport-browsers")
    environment.setdefault("npm_config_cache", str(root / ".cache/npm"))
    return environment


def setup_browser(root=ROOT):
    environment = browser_environment(root)
    installation = root / ".cache/viewport-tools"
    subprocess.run([
        "npm", "install", "--prefix", str(installation), "--ignore-scripts",
        "--no-audit", "--no-fund", "--save-exact", f"playwright@{PLAYWRIGHT_VERSION}",
    ], cwd=root, env=environment, check=True)
    subprocess.run([
        "node", str(installation / "node_modules/playwright/cli.js"),
        "install", "chromium", "--no-shell",
    ], cwd=root, env=environment, check=True)
    require_browser_tools(root)
    print(f"Measurement browser: Playwright {PLAYWRIGHT_VERSION} with isolated Chromium", flush=True)


def require_browser_tools(root=ROOT):
    package = root / ".cache/viewport-tools/node_modules/playwright"
    hint = "Run `python3 -m tools.benchmark.measure setup-browser` first."
    try:
        installed = json.loads((package / "package.json").read_text(encoding="utf-8"))
        if not isinstance(installed, dict) or installed.get("version") != PLAYWRIGHT_VERSION or not (package / "cli.js").is_file():
            raise ValueError("Playwright version does not match")
        executable = Path(subprocess.check_output([
            "node", "-e", "console.log(require(process.argv[1]).chromium.executablePath())", str(package),
        ], cwd=root, env=browser_environment(root), text=True).strip())
        executable.resolve().relative_to((root / ".cache/viewport-browsers").resolve())
        if not executable.is_file():
            raise ValueError("Chromium executable is missing")
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        raise RuntimeError(f"Optional measurement browser tools are missing or mismatched. {hint}") from error


def build_command(host, profile):
    command = ["cargo", "build", "--locked", "--features", "viewport-measure", "--profile", profile]
    return command + (["--target", web.TARGET, "--lib"] if host == "web" else ["--bin", "n3"])


def build(host, profile, root=ROOT, *, log=None):
    environment = web.cargo_environment(root)
    generator = web.require_bindgen(root) if host == "web" else None
    subprocess.run(build_command(host, profile), cwd=root, env=environment, check=True,
                   **({"stdout": log, "stderr": subprocess.STDOUT} if log is not None else {}))
    profile_dir = "debug" if profile == "dev" else "release"
    target = Path(environment["CARGO_TARGET_DIR"])
    if not target.is_absolute():
        target = root / target
    if host == "native":
        return target / profile_dir / ("n3.exe" if os.name == "nt" else "n3")
    output = root / "build/measure-web" / profile
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="measure-stage-", dir=output.parent) as temporary:
        stage = Path(temporary) / "site"
        stage.mkdir()
        for name in ("measure.html", "measure.js", "observations.mjs"):
            shutil.copyfile(root / "tools/benchmark/browser" / name, stage / name)
        (stage / "licenses").mkdir()
        shutil.copyfile(root / "assets/fonts/inter/LICENSE.txt", stage / "licenses/Inter.txt")
        shutil.copyfile(root / "assets/fonts/lucide/LICENSE", stage / "licenses/Lucide.txt")
        subprocess.run([
            str(generator), str(target / web.TARGET / profile_dir / "n3.wasm"),
            "--target", "web", "--out-name", "n3", "--out-dir", str(stage / "pkg"),
        ], cwd=root, check=True,
            **({"stdout": log, "stderr": subprocess.STDOUT} if log is not None else {}))
        if output.exists():
            shutil.rmtree(output)
        stage.rename(output)
    return output


def source_fingerprint(root):
    paths = subprocess.check_output([
        "git", "ls-files", "--cached", "--others", "--exclude-standard", "-z",
    ], cwd=root).split(b"\0")
    digest = hashlib.sha256()
    paths = sorted(set(path for path in paths if path))
    for name in paths:
        digest.update(len(name).to_bytes(8, "big"))
        digest.update(name)
        path = root / os.fsdecode(name)
        if path.is_symlink():
            digest.update(b"symlink\0" + os.fsencode(os.readlink(path)))
        elif path.is_file():
            digest.update(b"file\0")
            digest.update((path.stat().st_mode & 0o777).to_bytes(2, "big"))
            content = hashlib.sha256()
            with path.open("rb") as source:
                for chunk in iter(lambda: source.read(1024 * 1024), b""):
                    content.update(chunk)
            digest.update(content.digest())
        elif path.is_dir():
            digest.update(b"directory\0")
        elif not path.exists():
            digest.update(b"missing\0")
        else:
            raise RuntimeError(f"Unsupported source file type: {os.fsdecode(name)}")
    return {"sha256": digest.hexdigest(), "paths": len(paths), "scope": "sorted tracked and nonignored untracked paths, modes, symlinks, file bytes, and missing markers"}


def metadata(args, root=ROOT):
    def git(*arguments):
        return subprocess.check_output(["git", *arguments], cwd=root, text=True).strip()
    digest = hashlib.sha256()
    with args.input.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return {
        "recorded_at_utc": datetime.now(timezone.utc).isoformat(),
        "revision": git("rev-parse", "HEAD"),
        "source_fingerprint": source_fingerprint(root),
        "dirty": bool(git("status", "--porcelain")),
        "profile": args.profile,
        "browser_mode": args.browser if args.host == "web" else None,
        "requested_device_scale_factor": args.device_scale_factor if args.host == "web" and args.browser != "manual" else None,
        "build_command": build_command(args.host, args.profile),
        "rustflags": os.environ.get("RUSTFLAGS"),
        "cargo_encoded_rustflags": os.environ.get("CARGO_ENCODED_RUSTFLAGS"),
        "profile_overrides": {key: value for key, value in os.environ.items() if key.startswith("CARGO_PROFILE_")},
        "rustc": subprocess.check_output(["rustc", "--version"], cwd=root, text=True).strip(),
        "operating_system": platform.platform(),
        "architecture": platform.machine(),
        "input": {"name": args.input.name, "bytes": args.input.stat().st_size, "sha256": digest.hexdigest()},
        "requested_physical_size": [args.width, args.height],
        "display_refresh_hz": None,
        "observed_presentation_timestamps": None,
        "gpu_timestamps": None,
        "loading": None,
    }




def options(args):
    return {
        "warmup_frames": args.warmup,
        "sample_frames": args.frames,
        "workload": args.workload,
        "mode": args.mode,
        "selected": args.selected,
        "instrument_stages": not args.no_stage_timing,
    }


def stop_browser_runner(process):
    if process is None or process.poll() is not None:
        return
    # The Node owner handles SIGTERM by closing its browser and context. Give
    # that cleanup time to finish before bounding a stuck runner's lifetime.
    # The runner caps launch at 30s, then closes/kills its owned BrowserServer;
    # do not kill Node before it can complete that browser-process cleanup.
    process.terminate()
    try:
        process.wait(timeout=45)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)


def run_browser(args, artifact, info, root=ROOT, *, log=None):
    config = {"options": options(args), "metadata": info, "input_name": args.input.name,
              "width": args.width, "height": args.height, "timeout_seconds": args.timeout}
    with MeasurementServer(artifact, args.input, args.output, config) as server:
        server.timeout = 0.1
        process = None
        try:
            print(f"Viewport measurement: {server.url}", flush=True)
            if args.browser == "manual":
                print("Manual mode: press Run and keep the page visible and focused.", flush=True)
                if args.open:
                    webbrowser.open(server.url)
            else:
                process = subprocess.Popen([
                    "node", str(root / "tools/benchmark/browser/runner.mjs"), server.url,
                    args.browser, str(args.width), str(args.height),
                    str(args.device_scale_factor), str(args.timeout),
                ], cwd=root, env=browser_environment(root), start_new_session=os.name == "posix",
                    **({"stdout": log, "stderr": subprocess.STDOUT} if log is not None else {}))
            deadline = time.monotonic() + args.timeout
            while True:
                if time.monotonic() >= deadline:
                    raise RuntimeError("Browser run timed out before a complete report and browser shutdown")
                if process is not None:
                    code = process.poll()
                    if code is not None:
                        if code != 0:
                            raise RuntimeError(f"Automated measurement browser exited with status {code}")
                        if not server.completed.is_set():
                            raise RuntimeError("Automated browser exited without a complete report")
                        break
                elif server.completed.is_set():
                    break
                # Keep serving through the report response and browser cleanup;
                # a saved report alone does not prove the owned runner succeeded.
                server.handle_request()
        finally:
            stop_browser_runner(process)


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--host", choices=("native", "web"), required=True)
    result.add_argument("--profile", choices=("dev", "release"), default="release")
    result.add_argument("--input", type=Path, required=True)
    result.add_argument("--output", type=Path, required=True)
    result.add_argument("--frames", type=int, default=180)
    result.add_argument("--warmup", type=int, default=60)
    result.add_argument("--workload", choices=("stationary", "orbit"), default="orbit")
    result.add_argument("--mode", choices=("editor", "viewport", "renderer"), default="editor", help="complete editor, editor viewport without UI, or production scene renderer without UI/editor feedback")
    result.add_argument("--selected", action="store_true")
    result.add_argument("--width", type=int, default=1280, help="physical editor width in pixels")
    result.add_argument("--height", type=int, default=800, help="physical editor height in pixels")
    result.add_argument("--no-stage-timing", action="store_true", help="disable optional CPU stage instrumentation")
    result.add_argument("--browser", choices=("headless", "headed", "manual"), default="headless", help="isolated automated Chromium, or an explicit manual browser session")
    result.add_argument("--device-scale-factor", type=float, default=2.0, help="automated browser device scale factor, greater than zero and at most 4")
    result.add_argument("--open", action="store_true", help="open the local browser page only in --browser manual mode")
    result.add_argument("--timeout", type=int, default=600, help="runtime timeout in seconds, excluding build")
    return result


def validate_arguments(args, arguments):
    """Shared option policy for single runs and repeated suites."""
    if args.mode == "renderer" and args.selected:
        arguments.error("--selected requires --mode editor or viewport; renderer mode excludes editor feedback")
    if not 1 <= args.frames <= 10000 or not 0 <= args.warmup <= 10000:
        arguments.error("frames must be 1..10000 and warmup 0..10000")
    if not 64 <= args.width <= 8192 or not 64 <= args.height <= 8192:
        arguments.error("physical width and height must be 64..8192")
    if not 1 <= args.timeout <= 7200:
        arguments.error("timeout must be 1..7200 seconds")
    if not math.isfinite(args.device_scale_factor) or not 0 < args.device_scale_factor <= 4:
        arguments.error("device scale factor must be finite, greater than zero, and at most 4")
    if args.open and (args.host != "web" or args.browser != "manual"):
        arguments.error("--open requires --host web --browser manual")
    if args.host != "web" and args.browser != "headless":
        arguments.error("--browser only applies to the web host")
    args.input = args.input.expanduser().resolve()
    args.output = args.output.expanduser().absolute()


def run_prepared(args, artifact, info, *, log=None):
    """Run one fresh host using an already built artifact, preserving raw evidence."""
    if args.output.exists() or args.output.is_symlink():
        raise RuntimeError("Output already exists; choose a new report path")
    if args.host == "native":
        environment = web.cargo_environment(ROOT)
        environment.update({
            "N3_VIEWPORT_MEASURE": json.dumps(options(args)),
            "N3_VIEWPORT_MEASURE_METADATA": json.dumps(info),
            "N3_VIEWPORT_MEASURE_OUTPUT": str(args.output),
            "N3_VIEWPORT_MEASURE_SIZE": f"{args.width}x{args.height}",
        })
        subprocess.run([str(artifact), str(args.input)], cwd=ROOT, env=environment,
                       check=True, timeout=args.timeout,
                       **({"stdout": log, "stderr": subprocess.STDOUT} if log is not None else {}))
    else:
        run_browser(args, artifact, info, **({"log": log} if log is not None else {}))
    if not args.output.is_file():
        raise RuntimeError(f"{args.host.title()} run exited without writing its report")
    if args.output.stat().st_size > MAX_REPORT_BYTES:
        raise RuntimeError("Measurement report exceeds the size limit")
    return validate_envelope(json.loads(args.output.read_text(encoding="utf-8")), options(args))


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if argv[:1] == ["setup-browser"]:
        setup_parser = argparse.ArgumentParser(description="Install the optional isolated measurement browser")
        setup_parser.parse_args(argv[1:])
        try:
            setup_browser()
        except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
            setup_parser.exit(1, f"Measurement browser setup failed: {error}\n")
        return 0
    arguments = parser()
    args = arguments.parse_args(argv)
    validate_arguments(args, arguments)
    try:
        if not args.input.is_file():
            raise RuntimeError("Selected input must be an existing file")
        if args.output.exists() or args.output.is_symlink():
            raise RuntimeError("Output already exists; choose a new report path")
        if args.host == "web" and args.browser != "manual":
            require_browser_tools()
        args.output.parent.mkdir(parents=True, exist_ok=True)
        info = metadata(args)
        artifact = build(args.host, args.profile)
        run_prepared(args, artifact, info)
        print(f"Measurement report: {args.output}", flush=True)
    except KeyboardInterrupt:
        return 130
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        arguments.exit(1, f"Viewport measurement failed: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
