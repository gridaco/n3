"""Opt in to N3's pinned Ubuntu CI environment; local recipes run natively."""

import csv
import hashlib
import io
import os
from pathlib import Path
import platform
import shutil
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
FAILURE_ARTIFACTS = ".cache/ci/linux-amd64/home/docs-failure-artifacts"
PROBE_COMMAND = [
    "cargo", "test", "--locked", "imported_render_fingerprints", "--", "--nocapture",
]

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
python3 tools/ci_runner.py --environment-receipt
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


def probe_cpu_caps(command):
    """Alternate CPU caps are restricted to the fixed renderer diagnostic probe."""
    caps = os.environ.get("N3_CI_PROBE_CPU_CAPS")
    if caps is None:
        return "sse2"
    if caps not in ("sse2", "nosse") or command != PROBE_COMMAND:
        raise RunnerError(
            "N3_CI_PROBE_CPU_CAPS accepts sse2|nosse only for "
            "test imported_render_fingerprints -- --nocapture."
        )
    return caps


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


def identity_files(uid, gid):
    """Resolve the exact host identity inside the image without root fallback."""
    if uid < 0 or gid < 0:
        raise RunnerError("The CI container requires nonnegative host uid/gid values.")
    # The PTY backend resolves its effective uid with getpwuid_r even when a
    # shell is explicitly configured. Numeric Docker users need a passwd entry.
    if uid == 0:
        passwd = f"root:x:0:{gid}:N3 CI:/n3-cache/home:/bin/sh\n"
    else:
        passwd = "root:x:0:0:root:/root:/bin/sh\n"
        passwd += f"n3:x:{uid}:{gid}:N3 CI:/n3-cache/home:/bin/sh\n"
    group = "root:x:0:\n"
    if gid != 0:
        group += f"n3:x:{gid}:\n"
    return {"passwd": passwd, "group": group}


def prepare_identity_files(root, uid, gid):
    cache = root / ".cache/ci/linux-amd64"
    cache.mkdir(parents=True, exist_ok=True)
    for name, content in identity_files(uid, gid).items():
        destination = cache / name
        if any(path.is_symlink() for path in (destination, *destination.parents) if root in path.parents):
            raise RunnerError("CI identity files must stay in a real repository-local cache.")
        destination.write_text(content, encoding="utf-8")


def container_command(root, identity, update, command, uid, gid):
    cpu_caps = probe_cpu_caps(command)
    # Historical narrow profiles stay available only for the fixed probe.
    vector_width = 128 if "N3_CI_PROBE_CPU_CAPS" in os.environ else 256
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
    for name in ("passwd", "group"):
        arguments.extend([
            "--mount", mount(
                "type=bind", f"source={cache / name}", f"target=/etc/{name}", "readonly"
            ),
        ])
    if update:
        # The secondary renderer records exact digests and local review media;
        # the canonical native guide stays read-only in every container mode.
        for relative in ("docs/baselines", ".cache/docs/linux-vulkan-lavapipe"):
            arguments.extend([
                "--mount", mount(
                    "type=bind", f"source={root / relative}", f"target=/workspace/{relative}"
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
        f"GALLIUM_OVERRIDE_CPU_CAPS={cpu_caps}",
        # Float8 sRGB uses accurate sqrt with AVX masked by the SSE2 profile.
        f"LP_NATIVE_VECTOR_WIDTH={vector_width}",
        # Mesa 25.2.8's native-code cache key omits this vector-width override.
        "MESA_SHADER_CACHE_DISABLE=true",
        # Mesa 25.2.8 reports capabilities after applying the override.
        "GALLIUM_DUMP_CPU=1",
        "LP_NUM_THREADS=2",
    ):
        arguments.extend(["--env", value])
    if os.environ.get("N3_CI_FAILURE_ARTIFACTS") == "1":
        arguments.extend([
            "--env", "N3_DOCS_FAILURE_ARTIFACTS=/n3-cache/home/docs-failure-artifacts",
        ])
    script = INITIALIZE + (CI_SCRIPT if command is None else 'exec "$@"\n')
    arguments.extend([f"n3-ci:{identity}", "/bin/sh", "-eu", "-c", script, "n3-ci"])
    if command is not None:
        arguments.extend(command)
    return arguments


def status(returncode):
    return returncode if returncode >= 0 else 128 - returncode


def cpu_features(cpuinfo):
    """Report capabilities without CPU names, identifiers, or unrelated fields."""
    features = set()
    for line in cpuinfo.splitlines():
        name, separator, value = line.partition(":")
        if separator and name.strip() in ("flags", "Features"):
            features.update(value.split())
    return sorted(features)


def print_environment_receipt():
    print(
        f"N3 CI container: architecture={platform.machine()} "
        f"libc={os.confstr('CS_GNU_LIBC_VERSION')}",
        flush=True,
    )
    features = cpu_features(Path("/proc/cpuinfo").read_text(encoding="utf-8"))
    print(f"N3 CI container CPU features: {' '.join(features)}", flush=True)


def reset_failure_artifacts(root):
    """Clear only the fixed ignored diagnostic directory before an opted-in run."""
    destination = root / FAILURE_ARTIFACTS
    if any(
        path.is_symlink()
        for path in (destination, *destination.parents)
        if root in path.parents
    ) or (destination.exists() and not destination.is_dir()):
        raise RunnerError("CI failure artifacts require a real repository-local cache directory.")
    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True)


def main(argv=None, *, root=ROOT):
    arguments = list(sys.argv[1:] if argv is None else argv)
    if arguments == ["--environment-receipt"]:
        print_environment_receipt()
        return 0
    if arguments in (["--help"], ["-h"]):
        print(USAGE)
        return 0
    try:
        update, command = parse_command(arguments)
        probe_cpu_caps(command)
    except RunnerError as error:
        print(error, file=sys.stderr)
        return 2
    try:
        root = Path(root).resolve()
        if update and ((root / "docs/guide").is_symlink() or not (root / "docs/guide").is_dir()):
            raise RunnerError("docs update requires a real docs/guide directory in this checkout.")
        if update:
            for relative in ("docs/baselines", ".cache/docs/linux-vulkan-lavapipe"):
                destination = root / relative
                if (destination.exists() and not destination.is_dir()) or any(
                    path.is_symlink()
                    for path in (destination, *destination.parents)
                    if root in path.parents
                ):
                    raise RunnerError(f"docs update requires a real {relative} directory in this checkout.")
        identity = image_identity(root)
        print(f"N3 CI host: architecture={platform.machine()}", flush=True)
        print(f"N3 CI environment: {PLATFORM}, n3-ci:{identity}", flush=True)
        result = subprocess.run(build_command(root, identity), cwd=root, check=False)
        if result.returncode:
            return status(result.returncode)
        for name in ("cargo", "target", "home"):
            (root / ".cache/ci/linux-amd64" / name).mkdir(parents=True, exist_ok=True)
        uid, gid = os.getuid(), os.getgid()
        prepare_identity_files(root, uid, gid)
        if os.environ.get("N3_CI_FAILURE_ARTIFACTS") == "1":
            reset_failure_artifacts(root)
        if update:
            for relative in ("docs/baselines", ".cache/docs/linux-vulkan-lavapipe"):
                (root / relative).mkdir(parents=True, exist_ok=True)
        result = subprocess.run(
            container_command(root, identity, update, command, uid, gid),
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
