#!/usr/bin/env python3
"""Fetch every corpus repository at its pinned commit into a local cache (never committed).

    python3 benchmark/fetch.py [--cache .benchmark-cache] [name ...]

Each repository is fetched by commit SHA with depth 1, so a moved branch or a force-push cannot
change what is analysed; the checked-out commit is verified against corpus.json afterwards.
This is the only benchmark step that uses the network.
"""
import argparse
import json
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent


def git(*args, cwd=None):
    return subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True, text=True).stdout.strip()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cache", default=str(HERE.parent / ".benchmark-cache"))
    ap.add_argument("names", nargs="*")
    args = ap.parse_args()
    corpus = json.loads((HERE / "corpus.json").read_text())
    cache = pathlib.Path(args.cache)
    cache.mkdir(parents=True, exist_ok=True)
    for entry in corpus["repositories"]:
        if args.names and entry["name"] not in args.names:
            continue
        dest = cache / entry["name"]
        if not (dest / ".git").exists():
            dest.mkdir(parents=True, exist_ok=True)
            git("init", "-q", cwd=dest)
            git("remote", "add", "origin", f"https://github.com/{entry['repository']}.git", cwd=dest)
        if current_head(dest) != entry["commit"]:
            git("fetch", "-q", "--depth", "1", "origin", entry["commit"], cwd=dest)
            git("checkout", "-q", "--detach", "FETCH_HEAD", cwd=dest)
        head = git("rev-parse", "HEAD", cwd=dest)
        if head != entry["commit"]:
            sys.exit(f"{entry['name']}: checked out {head}, corpus.json pins {entry['commit']}")
        print(f"{entry['name']}: {head}")


def current_head(dest):
    r = subprocess.run(["git", "rev-parse", "--verify", "-q", "HEAD"], cwd=dest, capture_output=True, text=True)
    return r.stdout.strip() if r.returncode == 0 else None


if __name__ == "__main__":
    main()
