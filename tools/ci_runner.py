"""Opt in to N3's pinned Ubuntu CI environment; local recipes run natively."""

import csv
import hashlib
import io
import os
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
PLATFORM = "linux/amd64"
IMAGE_INPUTS = (
    "tools/ci/Dockerfile",
    "tools/ci/Dockerfile.dockerignore",
    "rust-toolchain.toml",
)
USAGE = "Usage: ci_runner.py test [CARGO TEST ARGS...] | docs check|update | ci"

# These are fixed shell statements, never interpolated with paths or user input.
# The ICD filename changed between Mesa packages; accept only the two known
# lavapipe names, and fail rather than choosing a different Vulkan implementation.
INITIALIZE = """\
mkdir -p /tmp/runtime-n3
chmod 700 /tmp/runtime-n3
export XDG_RUNTIME_DIR=/tmp/runtime-n3
if [ -f /usr/share/vulkan/icd.d/lvp_icd.x86_64.json ]; then
    export VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json
elif [ -f /usr/share/vulkan/icd.d/lvp_icd.json ]; then
    export VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json
else
    echo 'The CI image is missing its lavapipe Vulkan driver.' >&2
    exit 1
fi
"""
CI_SCRIPT = """\
python3 tools/format_docs.py --check
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
python3 -m unittest discover -s tools/tests -p 'test_*.py'
exec cargo test --locked
"""


class RunnerError(Exception):
    pass


def parse_command(arguments):
    if arguments and arguments[0] == "test":
        return False, ["cargo", "test", "--locked", *arguments[1:]]
    if len(arguments) == 2 and arguments[0] == "docs" and arguments[1] in ("check", "update"):
        return arguments[1] == "update", [
            "cargo", "run", "--locked", "--", "--docs", arguments[1]
        ]
    if arguments == ["ci"]:
        return False, None
    raise RunnerError(USAGE)


def image_identity(root):
    digest = hashlib.sha256()
    for relative in IMAGE_INPUTS:
        data = (root / relative).read_bytes()
        # Include names and lengths so boundaries cannot create ambiguous input.
        digest.update(relative.encode("utf-8") + b"\0")
        digest.update(len(data).to_bytes(8, "big"))
        digest.update(data)
    return digest.hexdigest()


def mount(*fields):
    """Docker --mount uses CSV; preserve commas as well as spaces in host paths."""
    output = io.StringIO()
    csv.writer(output, lineterminator="").writerow(fields)
    return output.getvalue()


def build_command(root, identity):
    return [
        "docker", "build", "--platform", PLATFORM,
        "--file", str(root / "tools/ci/Dockerfile"),
        "--tag", f"n3-ci:{identity}", str(root),
    ]


def container_command(root, identity, update, command, uid, gid):
    cache = root / ".cache/ci/linux-amd64"
    arguments = [
        "docker", "run", "--rm", "--init", "--platform", PLATFORM,
        "--user", f"{uid}:{gid}", "--workdir", "/workspace",
        "--mount", mount("type=bind", f"source={root}", "target=/workspace", "readonly"),
    ]
    for name in ("cargo", "target", "home"):
        arguments.extend([
            "--mount", mount(
                "type=bind", f"source={cache / name}", f"target=/n3-cache/{name}"
            ),
        ])
    if update:
        arguments.extend([
            "--mount", mount(
                "type=bind", f"source={root / 'docs/guide'}", "target=/workspace/docs/guide"
            ),
        ])
    for value in (
        "CARGO_HOME=/n3-cache/cargo",
        "CARGO_TARGET_DIR=/n3-cache/target",
        "RUSTUP_HOME=/opt/rustup",
        "HOME=/n3-cache/home",
        "npm_config_cache=/n3-cache/home/.npm",
        "PYTHONDONTWRITEBYTECODE=1",
        "GALLIUM_DRIVER=llvmpipe",
        "GALLIUM_OVERRIDE_CPU_CAPS=sse2",
        "LP_NUM_THREADS=2",
    ):
        arguments.extend(["--env", value])
    script = INITIALIZE + (CI_SCRIPT if command is None else 'exec "$@"\n')
    arguments.extend([f"n3-ci:{identity}", "/bin/sh", "-eu", "-c", script, "n3-ci"])
    if command is not None:
        arguments.extend(command)
    return arguments


def status(returncode):
    return returncode if returncode >= 0 else 128 - returncode


def main(argv=None, *, root=ROOT):
    arguments = list(sys.argv[1:] if argv is None else argv)
    if arguments in (["--help"], ["-h"]):
        print(USAGE)
        return 0
    try:
        update, command = parse_command(arguments)
    except RunnerError as error:
        print(error, file=sys.stderr)
        return 2
    try:
        root = Path(root).resolve()
        if update and ((root / "docs/guide").is_symlink() or not (root / "docs/guide").is_dir()):
            raise RunnerError("docs update requires a real docs/guide directory in this checkout.")
        identity = image_identity(root)
        print(f"N3 CI environment: {PLATFORM}, n3-ci:{identity}", flush=True)
        result = subprocess.run(build_command(root, identity), cwd=root, check=False)
        if result.returncode:
            return status(result.returncode)
        for name in ("cargo", "target", "home"):
            (root / ".cache/ci/linux-amd64" / name).mkdir(parents=True, exist_ok=True)
        result = subprocess.run(
            container_command(root, identity, update, command, os.getuid(), os.getgid()),
            cwd=root, check=False,
        )
        return status(result.returncode)
    except (RunnerError, OSError) as error:
        print(f"N3 CI runner: {error}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        # Docker shares the foreground process group and forwards its interrupt
        # to the container; report the conventional interrupt status to callers.
        return 130


if __name__ == "__main__":
    raise SystemExit(main())
