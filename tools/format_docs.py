"""Run pinned Oxfmt through npx, including binding-safe guide templates."""

import os
from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
OXFMT_VERSION = "0.71.0"
USAGE = "Usage: format_docs.py --prepare|--check|--write"
BINDINGS = re.compile(r"\{\{[^{}]*\}\}")


def oxfmt(root, arguments, source=None):
    environment = os.environ.copy()
    # Native runs stay repository-local; the optional CI container supplies its
    # writable cache because its source tree is mounted read-only.
    environment.setdefault("npm_config_cache", str(root / ".cache/npm"))
    # Resolve packages in the cache, not in a leftover host node_modules tree
    # that could contain bindings for a different OS inside the Linux runner.
    # npm exec still runs Oxfmt in cwd, so discovery and ignore paths stay rooted.
    return subprocess.run(
        [
            "npx", "--yes", "--ignore-scripts", "--prefer-offline",
            "--prefix", environment["npm_config_cache"],
            f"oxfmt@{OXFMT_VERSION}", *arguments,
        ],
        cwd=root,
        env=environment,
        input=source,
        text=True,
        encoding="utf-8",
        stdout=subprocess.PIPE if source is not None else None,
        check=False,
    )


def format_templates(root, mode):
    paths = sorted(path for path in (root / "docs/templates").iterdir() if path.name.endswith(".md.in"))
    changes = []
    for path in paths:
        source = path.read_text(encoding="utf-8")
        # Ordinary discovery skips .md.in. Infer Markdown from a virtual .md
        # path while keeping the real template source and shared configuration.
        result = oxfmt(root, [
            "--config", str(root / ".oxfmtrc.json"),
            "--stdin-filepath", str(path.with_suffix("")),
        ], source)
        if result.returncode:
            print(f"Formatting failed: {path.relative_to(root)}", file=sys.stderr)
            return result.returncode
        if BINDINGS.findall(source) != BINDINGS.findall(result.stdout):
            print(f"Formatting changed guide bindings: {path.relative_to(root)}", file=sys.stderr)
            return 1
        if source != result.stdout:
            changes.append((path, result.stdout))

    # Validate every template before writing any of them. A formatter error or
    # changed binding must not leave a partially rewritten template collection.
    for path, formatted in changes:
        if mode == "--write":
            path.write_text(formatted, encoding="utf-8")
        else:
            print(f"Unformatted: {path.relative_to(root)}", file=sys.stderr)
    result_label = "formatted" if mode == "--write" else "need formatting"
    print(f"Guide templates: {len(paths)} checked, {len(changes)} {result_label}.")
    return int(mode == "--check" and bool(changes))


def main(arguments=None, *, root=ROOT):
    arguments = sys.argv[1:] if arguments is None else arguments
    if len(arguments) != 1 or arguments[0] not in ("--prepare", "--check", "--write"):
        print(USAGE, file=sys.stderr)
        return 2
    mode = arguments[0]
    try:
        if mode == "--prepare":
            return oxfmt(root, ["--version"]).returncode
        result = oxfmt(root, ["--config", str(root / ".oxfmtrc.json"), mode, "."])
        if result.returncode:
            return result.returncode
        return format_templates(root, mode)
    except (OSError, UnicodeError) as error:
        print(f"Documentation formatting failed: {error}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        return 130


if __name__ == "__main__":
    status = main()
    raise SystemExit(status if status >= 0 else 128 - status)
