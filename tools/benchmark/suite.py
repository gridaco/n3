"""Repeat real viewport measurements and report run-level variation or comparisons."""

import argparse
from contextlib import redirect_stderr, redirect_stdout
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import random
import shutil
import subprocess
import sys

from . import measure, reports


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    commands = result.add_subparsers(dest="command", required=True)
    run = commands.add_parser("run", help="build once, repeat fresh serial host sessions, and aggregate")
    run.add_argument("--input", type=Path, required=True)
    run.add_argument("--output", type=Path, required=True, help="new directory for raw reports, logs, and summaries")
    run.add_argument("--hosts", nargs="+", choices=("native", "web"), default=["native", "web"])
    run.add_argument("--modes", nargs="+", choices=("editor", "viewport", "renderer"), default=["editor", "viewport", "renderer"])
    run.add_argument("--repeats", type=int, default=5)
    run.add_argument("--seed", type=int, default=0, help="reproducible shuffled case order within each round")
    run.add_argument("--profile", choices=("dev", "release"), default="release")
    run.add_argument("--frames", type=int, default=180)
    run.add_argument("--warmup", type=int, default=60)
    run.add_argument("--workload", choices=("orbit", "stationary"), default="orbit")
    run.add_argument("--selected", action="store_true")
    run.add_argument("--width", type=int, default=1280, help="physical editor surface pixels, not inner viewport width")
    run.add_argument("--height", type=int, default=800)
    run.add_argument("--no-stage-timing", action="store_true")
    run.add_argument("--browser", choices=("headed", "headless"), default="headed")
    run.add_argument("--device-scale-factor", type=float, default=2.0)
    run.add_argument("--timeout", type=int, default=600, help="seconds per session, excluding builds")
    report = commands.add_parser("report", help="recompute a summary from saved raw measurement JSON files")
    report.add_argument("inputs", nargs="+", type=Path)
    report.add_argument("--output", type=Path, required=True, help="new summary directory")
    compare = commands.add_parser("compare", help="describe baseline/candidate changes for compatible cases")
    compare.add_argument("baseline", type=Path)
    compare.add_argument("candidate", type=Path)
    compare.add_argument("--output", type=Path, required=True, help="new comparison directory")
    return result


def timestamp():
    return datetime.now(timezone.utc).isoformat()


def plan_cases(args):
    cases = [(host, mode) for host in args.hosts for mode in args.modes]
    generator = random.Random(args.seed)
    plan = []
    for repeat in range(1, args.repeats + 1):
        order = list(cases)
        generator.shuffle(order)
        for host, mode in order:
            name = f"{host}-{mode}-{repeat:02d}"
            plan.append({"host": host, "mode": mode, "repeat": repeat,
                         "report": f"raw/{name}.json", "log": f"logs/{name}.log",
                         "status": "pending"})
    return plan


def session_arguments(args, entry, arguments):
    values = vars(args).copy()
    values.update(host=entry["host"], mode=entry["mode"],
                  output=args.output / entry["report"], open=False,
                  browser=args.browser if entry["host"] == "web" else "headless")
    session = argparse.Namespace(**values)
    measure.validate_arguments(session, arguments)
    return session


def new_output(path):
    path = path.expanduser().absolute()
    # A suite writes evidence while checking source identity. Nonignored output
    # inside the checkout would contaminate that identity on the first run.
    try:
        path.resolve().relative_to(measure.ROOT.resolve())
    except ValueError:
        pass
    else:
        ignored = subprocess.run(["git", "check-ignore", "--quiet", str(path)], cwd=measure.ROOT)
        if ignored.returncode != 0:
            raise ValueError("Benchmark output inside the repository must be ignored; use .cache/measure/<new-name>")
    path.mkdir(parents=True, exist_ok=False)
    return path


def write_json(path, value):
    measure.write_report(path, value)


def save_manifest(directory, manifest):
    temporary = directory / "manifest.tmp"
    temporary.write_text(json.dumps(manifest, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    temporary.replace(directory / "manifest.json")


def summarize(paths, output):
    summary = reports.analyze(paths)
    rendered = reports.render_text(summary)
    write_json(output / "summary.json", summary)
    (output / "summary.txt").write_text(rendered, encoding="utf-8")
    print(rendered)
    return summary


def unchanged(before, after):
    for name in ("source_fingerprint", "input", "rustc", "profile", "rustflags",
                 "cargo_encoded_rustflags", "profile_overrides", "operating_system", "architecture"):
        if before[name] != after[name]:
            raise RuntimeError(f"{name} changed during the suite; saved evidence is retained, start a new suite")


def artifact_fingerprint(artifact):
    files = sorted(artifact.rglob("*")) if artifact.is_dir() else [artifact]
    entries = []
    for path in files:
        if path.is_file():
            digest = hashlib.sha256()
            with path.open("rb") as source:
                for chunk in iter(lambda: source.read(1024 * 1024), b""):
                    digest.update(chunk)
            entries.append({"path": str(path.relative_to(artifact)) if artifact.is_dir() else path.name,
                            "sha256": digest.hexdigest(), "bytes": path.stat().st_size,
                            "mode": path.stat().st_mode & 0o777})
    if not entries:
        raise RuntimeError(f"Built artifact is missing or empty: {artifact}")
    return {"sha256": hashlib.sha256(json.dumps(entries, sort_keys=True).encode()).hexdigest(), "files": entries}


def snapshot_artifact(artifact, destination):
    """Freeze this build so another Cargo/web build cannot replace a later repeat."""
    before = artifact_fingerprint(artifact)
    if artifact.is_dir():
        shutil.copytree(artifact, destination)
        snapshot = destination
    else:
        destination.mkdir(parents=True)
        snapshot = destination / artifact.name
        shutil.copy2(artifact, snapshot)
    if artifact_fingerprint(artifact) != before or artifact_fingerprint(snapshot) != before:
        raise RuntimeError("Built artifact changed while copying; start a new suite")
    return snapshot, before


def run_suite(args, arguments):
    if not 1 <= args.repeats <= 100:
        arguments.error("repeats must be 1..100")
    if len(set(args.hosts)) != len(args.hosts) or len(set(args.modes)) != len(args.modes):
        arguments.error("hosts and modes must not contain duplicates")
    args.input = args.input.expanduser().resolve()
    args.output = args.output.expanduser().absolute()
    plan = plan_cases(args)
    # Validate every case before creating output, building, or opening a window.
    for entry in plan:
        session_arguments(args, entry, arguments)
    if not args.input.is_file():
        raise ValueError("Selected input must be an existing file")
    if "web" in args.hosts:
        measure.require_browser_tools()
    args.output = new_output(args.output)
    (args.output / "raw").mkdir()
    (args.output / "logs").mkdir()
    manifest = {"schema": "n3.viewport-suite-manifest.v1", "started_at_utc": timestamp(),
                "status": "running", "seed": args.seed, "repeats": args.repeats,
                "ordering": "serial seeded shuffle within each round",
                "configuration": {key: str(value) if isinstance(value, Path) else value
                                  for key, value in vars(args).items()},
                "runs": plan, "builds": []}
    completed = []
    failure = None
    exit_code = 0
    save_manifest(args.output, manifest)
    try:
        base = measure.metadata(session_arguments(args, plan[0], arguments))
        manifest["source"] = base
        artifacts = {}
        fingerprints = {}
        for host in args.hosts:
            print(f"Building {host} ({args.profile}); build time is excluded.", flush=True)
            record = {"host": host, "log": f"logs/build-{host}.log", "status": "running"}
            manifest["builds"].append(record)
            save_manifest(args.output, manifest)
            with (args.output / record["log"]).open("x", encoding="utf-8") as log:
                with redirect_stdout(log), redirect_stderr(log):
                    artifact = measure.build(host, args.profile, log=log)
            artifacts[host], fingerprints[host] = snapshot_artifact(artifact, args.output / "artifacts" / host)
            record.update(status="complete", artifact=str(artifacts[host].relative_to(args.output)),
                          artifact_fingerprint=fingerprints[host])
        unchanged(base, measure.metadata(session_arguments(args, plan[0], arguments)))
        save_manifest(args.output, manifest)
        for index, entry in enumerate(plan):
            session = session_arguments(args, entry, arguments)
            info = measure.metadata(session)
            unchanged(base, info)
            if artifact_fingerprint(artifacts[entry["host"]]) != fingerprints[entry["host"]]:
                raise RuntimeError("Prepared artifact changed before a session; start a new suite")
            info["artifact_fingerprint"] = fingerprints[entry["host"]]
            info["suite"] = {"round": entry["repeat"], "position": index + 1, "seed": args.seed}
            entry.update(status="running", started_at_utc=timestamp())
            save_manifest(args.output, manifest)
            print(f"[{index + 1}/{len(plan)}] {entry['host']} / {entry['mode']}, repeat {entry['repeat']}", flush=True)
            with (args.output / entry["log"]).open("x", encoding="utf-8") as log:
                with redirect_stdout(log), redirect_stderr(log):
                    measure.run_prepared(session, artifacts[entry["host"]], info, log=log)
            unchanged(base, measure.metadata(session))
            if artifact_fingerprint(artifacts[entry["host"]]) != fingerprints[entry["host"]]:
                raise RuntimeError("Prepared artifact changed during a session; start a new suite")
            entry.update(status="complete", finished_at_utc=timestamp(),
                         sha256=hashlib.sha256(session.output.read_bytes()).hexdigest())
            completed.append(session.output)
            save_manifest(args.output, manifest)
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError, KeyboardInterrupt) as error:
        failure = str(error) or type(error).__name__
        exit_code = 130 if isinstance(error, KeyboardInterrupt) else 1
        for entry in [*manifest["builds"], *plan]:
            if entry["status"] == "running":
                entry.update(status="failed", error=failure)
        print(f"Benchmark stopped: {failure}. Evidence retained in {args.output}", file=sys.stderr)
    finally:
        manifest.update(status="failed" if exit_code else "aggregating", error=failure)
        save_manifest(args.output, manifest)
    # A failed process can still leave JSON (for example, browser teardown failure).
    # Only sessions with successful completion and unchanged source are aggregated.
    if completed:
        try:
            summary = summarize(completed, args.output)
        except (OSError, ValueError) as error:
            manifest.update(status="failed", finished_at_utc=timestamp(), error=f"Aggregation failed: {error}")
            save_manifest(args.output, manifest)
            raise
        if summary["excluded_runs"]:
            exit_code = exit_code or 1
            manifest["status"] = "contaminated" if not failure else "failed"
    if manifest["status"] == "aggregating":
        manifest["status"] = "complete"
    manifest["finished_at_utc"] = timestamp()
    save_manifest(args.output, manifest)
    return exit_code


def main(argv=None):
    arguments = parser()
    args = arguments.parse_args(argv)
    try:
        if args.command == "run":
            return run_suite(args, arguments)
        if args.command == "report":
            output = new_output(args.output)
            summary = summarize(args.inputs, output)
            return int(bool(summary["excluded_runs"]))
        baseline = json.loads(args.baseline.read_text(encoding="utf-8"))
        candidate = json.loads(args.candidate.read_text(encoding="utf-8"))
        comparison = reports.compare(baseline, candidate)
        output = new_output(args.output)
        write_json(output / "comparison.json", comparison)
        rendered = reports.render_comparison(comparison)
        (output / "comparison.txt").write_text(rendered, encoding="utf-8")
        print(rendered)
        return int(any(case["status"] != "comparable" for case in comparison["cases"]))
    except KeyboardInterrupt:
        return 130
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        arguments.exit(1, f"Viewport benchmark failed: {error}\n")


if __name__ == "__main__":
    raise SystemExit(main())
