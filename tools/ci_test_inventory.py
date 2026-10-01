"""Check that N3's two CI test lanes cover the complete Rust test inventory."""

import re
import subprocess
import sys


GUIDE_TEST = "documentation::tests::generated_documentation_is_current"
LIST_COMMANDS = (
    ["cargo", "test", "--locked", "--", "--list"],
    ["cargo", "test", "--locked", GUIDE_TEST, "--", "--exact", "--list"],
    ["cargo", "test", "--locked", "--", "--skip", GUIDE_TEST, "--exact", "--list"],
    # Ordinary --list includes ignored tests without marking them. The guide
    # must execute, even if its registration is accidentally marked ignored.
    ["cargo", "test", "--locked", GUIDE_TEST, "--", "--exact", "--ignored", "--list"],
)
TEST_LINE = re.compile(r"([A-Za-z_][A-Za-z_0-9]*(?:::[A-Za-z_][A-Za-z_0-9]*)*): test")
SUMMARY_LINE = re.compile(r"([0-9]+) tests?, ([0-9]+) benchmarks?")


class InventoryError(Exception):
    pass


def parse_inventory(output):
    """Read libtest's complete list, including tests normally marked ignored."""
    names = set()
    summary = None
    for line in output.splitlines():
        if not line.strip():
            continue
        if summary is not None:
            raise InventoryError("Unexpected output after the Rust test inventory summary.")
        if match := TEST_LINE.fullmatch(line):
            name = match[1]
            if name in names:
                raise InventoryError(f"Duplicate Rust test inventory entry: {name}")
            names.add(name)
        elif match := SUMMARY_LINE.fullmatch(line):
            summary = (int(match[1]), int(match[2]))
        else:
            raise InventoryError(f"Malformed Rust test inventory line: {line!r}")
    if summary != (len(names), 0):
        raise InventoryError("Rust test inventory summary is missing or does not match its entries.")
    return names


def validate_partition(full, guide, checks):
    if guide != {GUIDE_TEST}:
        raise InventoryError("The CI guide lane must contain exactly the mandatory full-guide test.")
    if guide & checks:
        raise InventoryError("The CI guide and checks test inventories overlap.")
    if guide | checks != full:
        missing = sorted(full - (guide | checks))
        extra = sorted((guide | checks) - full)
        raise InventoryError(f"The CI test partition does not cover the full inventory: missing={missing}, extra={extra}")


def discover():
    inventories = []
    for command in LIST_COMMANDS:
        result = subprocess.run(command, stdout=subprocess.PIPE, text=True, check=False)
        if result.returncode:
            raise InventoryError(f"Rust test inventory discovery failed with status {result.returncode}.")
        inventories.append(parse_inventory(result.stdout))
    full, guide, checks, ignored_guide = inventories
    validate_partition(full, guide, checks)
    if ignored_guide:
        raise InventoryError("The mandatory full-guide test must not be ignored.")
    return full, guide, checks


def main(argv=None):
    arguments = list(sys.argv[1:] if argv is None else argv)
    if arguments:
        print("Usage: ci_test_inventory.py", file=sys.stderr)
        return 2
    try:
        full, guide, checks = discover()
    except (InventoryError, OSError) as error:
        print(f"N3 CI test inventory: {error}", file=sys.stderr)
        return 1
    print(
        f"N3 CI test inventory: {len(full)} total = {len(guide)} guide + {len(checks)} checks "
        "(including ignored tests)",
        flush=True,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
