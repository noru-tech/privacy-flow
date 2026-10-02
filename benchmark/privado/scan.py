#!/usr/bin/env python3
"""Run Privado's engine on one corpus application, offline, as PROTOCOL.md §8 fixes it.

    python3 benchmark/privado/scan.py <app name> --rules <privado rules checkout> --out <dir>

Copies the application's scope (minus corpus.json excludes) to a work directory, because the
engine writes .privado/ into the directory it scans; runs the pinned image with networking
disabled and uploads and metrics off; keeps privado.json and the engine's log; and maps the
result with map.py. Needs Docker on linux/amd64 (the image has no other platform).
"""
import argparse
import json
import pathlib
import shutil
import subprocess
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
IMAGE = "public.ecr.aws/privado/privado@sha256:349fdd5a01c01acb4b17c9db0df782c8ad2fb4d87b5940247720fc3cac8e7f15"
LANGUAGE = {"typescript": "javascript", "python": "python"}


def prepare_work(entry, src, work):
    """Copy the application's scope to `work` minus corpus.json excludes; return files removed."""
    if work.exists():
        shutil.rmtree(work, ignore_errors=True)
    shutil.copytree(src, work, ignore=shutil.ignore_patterns(".git"), symlinks=True)
    removed = 0
    for glob in entry["exclude"]:
        for p in sorted(work.glob(glob), reverse=True):
            if p.is_file() or p.is_symlink():
                p.unlink()
                removed += 1
    return removed


def command(work, rules, language, memory_gib, swap_gib, heap_gib, run_args=("--rm",)):
    """The protocol's Privado invocation (PROTOCOL.md §8): pinned image, no network, no upload."""
    return [
        "docker", "run", *run_args, "--network", "none", "--platform", "linux/amd64",
        "--memory", f"{memory_gib}g", "--memory-swap", f"{memory_gib + swap_gib}g",
        "-e", "PRIVADO_METRICS_ENABLED=false", "-e", f"JAVA_TOOL_OPTIONS=-Xmx{heap_gib}g",
        "-v", f"{work}:/app/code", "-v", f"{pathlib.Path(rules).resolve()}:/app/rules:ro",
        IMAGE,
        "/app/code", "-ic", "/app/rules",
        "--skip-upload", "--skip-download-dependencies", "--offline-mode",
        "-fl", LANGUAGE[language],
    ]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("name")
    ap.add_argument("--rules", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--cache", default=str(ROOT / ".benchmark-cache"))
    ap.add_argument("--memory-gib", type=int, default=14)
    ap.add_argument("--swap-gib", type=int, default=0, help="extra swap the container may use (the runner must have it)")
    ap.add_argument("--heap-gib", type=int, default=0, help="JVM heap; default: memory minus 2 GiB")
    args = ap.parse_args()

    corpus = json.loads((ROOT / "benchmark/corpus.json").read_text())
    entry = next(e for e in corpus["repositories"] if e["name"] == args.name)
    src = pathlib.Path(args.cache) / entry["name"] / entry["scope"]
    out = pathlib.Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    # Outside `out` (which is uploaded); the container writes .privado/ as root, so the copy may
    # not be fully removable afterwards without privileges.
    work = out.parent / f".work-{entry['name']}"
    removed = prepare_work(entry, src, work)
    heap = args.heap_gib or max(2, args.memory_gib - 2)
    cmd = command(work, args.rules, entry["language"], args.memory_gib, args.swap_gib, heap)
    start = time.perf_counter()
    proc = subprocess.run(cmd, capture_output=True, text=True)
    wall = time.perf_counter() - start
    (out / "engine.log").write_text(proc.stdout + "\n--- stderr ---\n" + proc.stderr)
    result = work / ".privado" / "privado.json"
    record = {"name": entry["name"], "image": IMAGE, "command": cmd[cmd.index(IMAGE):],
              "exit_code": proc.returncode, "wall_seconds": round(wall, 1), "excluded_files": removed,
              "privado_json": result.exists(), "memory_gib": args.memory_gib, "swap_gib": args.swap_gib, "heap_gib": heap}
    if result.exists():
        shutil.copy(result, out / "privado.json")
        meta = json.loads(result.read_text())
        record["privado_versions"] = {k: meta.get(k) for k in ("privadoCoreVersion", "privadoMainVersion", "privadoLanguageEngineVersion", "privadoCLIVersion")}
        with open(out / "flows.jsonl", "w") as f:
            subprocess.run([sys.executable, str(HERE / "map.py"), str(result), entry["name"]], stdout=f, check=True)
    (out / "run.json").write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    shutil.rmtree(work, ignore_errors=True)
    print(json.dumps(record, sort_keys=True))
    if not result.exists():
        sys.exit(f"{entry['name']}: Privado produced no privado.json (exit {proc.returncode}); see engine.log")


if __name__ == "__main__":
    main()
