"""Install N3's local hook and verify the checkout represented by a push."""

import argparse
import os
from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
HOOKS_PATH = ".githooks"
OBJECT_ID = re.compile(r"(?:[0-9a-f]{40}|[0-9a-f]{64})\Z")


class HookError(Exception):
    pass


def git(root, *args, missing_ok=False):
    result = subprocess.run(
        ["git", *args], cwd=root, capture_output=True, text=True, check=False
    )
    if missing_ok and result.returncode == 1:
        return None
    if result.returncode:
        raise HookError(result.stderr.strip() or "Git command failed.")
    return result.stdout.strip()


def install(root):
    configured = git(root, "config", "--get", "core.hooksPath", missing_ok=True)
    if configured not in (None, HOOKS_PATH):
        raise HookError(
            "core.hooksPath already points elsewhere; keep the existing hooks "
            "and integrate N3's pre-push check explicitly."
        )
    hook = root / HOOKS_PATH / "pre-push"
    if not hook.is_file() or not os.access(hook, os.X_OK):
        raise HookError("The checked-in .githooks/pre-push must exist and be executable.")
    if configured is None:
        active = Path(git(root, "rev-parse", "--git-path", "hooks"))
        if not active.is_absolute():
            active = root / active
        # Changing hooksPath redirects every hook, not only pre-push.
        conflicts = sorted(
            path.name for path in active.iterdir()
            if not path.name.endswith(".sample") and path.is_file() and os.access(path, os.X_OK)
        ) if active.is_dir() else []
        if conflicts:
            raise HookError(
                f"Active Git hooks already exist: {', '.join(conflicts)}. "
                "Keep them and integrate N3's check explicitly."
            )
    git(root, "config", "--local", "core.hooksPath", HOOKS_PATH)
    print("N3 pre-push hook installed for this repository.")
    return 0


def pushed_objects(source):
    objects = []
    for line in source.splitlines():
        fields = line.split()
        if not fields:
            continue
        if (
            len(fields) != 4
            or not OBJECT_ID.fullmatch(fields[1])
            or not OBJECT_ID.fullmatch(fields[3])
        ):
            raise HookError("Malformed Git pre-push ref input.")
        local_ref, local_object, _, _ = fields
        if set(local_object) != {"0"}:
            objects.append((local_ref, local_object))
    return objects


def require_checkout(root, expected_head):
    if git(root, "rev-parse", "--verify", "HEAD^{commit}") != expected_head:
        raise HookError("HEAD changed during pre-push verification; run the push again.")
    if git(
        root, "status", "--porcelain=v1", "--untracked-files=all", "--ignore-submodules=none"
    ):
        raise HookError(
            "Pre-push verification requires a clean worktree, including nonignored "
            "untracked files. Commit or set aside your changes before pushing."
        )


def pre_push(root, source):
    objects = pushed_objects(source)
    if not objects:
        return 0
    head = git(root, "rev-parse", "--verify", "HEAD^{commit}")
    for local_ref, local_object in objects:
        commit = git(root, "rev-parse", "--verify", f"{local_object}^{{commit}}")
        if commit != head:
            raise HookError(
                f"Pushed ref {local_ref} does not point to the checked-out HEAD. "
                "Check out that commit and push it separately so it can be verified."
            )
    require_checkout(root, head)
    print("N3 pre-push: running just verify for the clean checked-out commit.", flush=True)
    result = subprocess.run(["just", "verify"], cwd=root, stdin=subprocess.DEVNULL, check=False)
    if result.returncode:
        return result.returncode if result.returncode > 0 else 128 - result.returncode
    require_checkout(root, head)
    return 0


def main(argv=None, *, root=ROOT, input_stream=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("install", "pre-push"))
    args = parser.parse_args(argv)
    try:
        if args.command == "install":
            return install(Path(root))
        return pre_push(Path(root), (input_stream or sys.stdin).read())
    except (HookError, OSError) as error:
        print(f"N3 git hooks: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
