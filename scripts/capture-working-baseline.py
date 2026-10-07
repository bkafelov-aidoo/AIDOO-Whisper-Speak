#!/usr/bin/env python3
"""Capture an exact, non-disruptive Git snapshot for a user-confirmed method."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import re
import subprocess
import tempfile


METHOD_PATTERN = re.compile(r"[a-z0-9][a-z0-9-]*")
VERSION_PATTERN = re.compile(r"v[1-9][0-9]*")


def git(repo: Path, *args: str, env: dict[str, str] | None = None, stdin: str | None = None) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=repo,
        env=env,
        input=stdin,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip() or "unknown Git error"
        raise SystemExit(detail)
    return result.stdout.strip()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Create a protected working/<method>/vN tag without changing the working tree."
    )
    parser.add_argument("method_id", help="lowercase method id, for example browser-status-presentation")
    parser.add_argument("version", help="confirmed version, for example v1")
    parser.add_argument("description", help="short user-confirmed behavior description")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if METHOD_PATTERN.fullmatch(args.method_id) is None:
        raise SystemExit("method_id must contain only lowercase letters, digits, and hyphens")
    if VERSION_PATTERN.fullmatch(args.version) is None:
        raise SystemExit("version must use v followed by a positive integer")
    if not args.description.strip() or "\n" in args.description:
        raise SystemExit("description must be one non-empty line")

    script_dir = Path(__file__).resolve().parent
    repo = Path(git(script_dir, "rev-parse", "--show-toplevel"))
    tag = f"working/{args.method_id}/{args.version}"
    if git(repo, "tag", "--list", tag):
        raise SystemExit(f"confirmed baseline already exists: {tag}")

    with tempfile.TemporaryDirectory(prefix="aidoo-working-baseline-") as temp_dir:
        snapshot_env = os.environ.copy()
        snapshot_env["GIT_INDEX_FILE"] = str(Path(temp_dir) / "index")
        git(repo, "read-tree", "HEAD", env=snapshot_env)
        git(repo, "add", "-A", "--", ".", env=snapshot_env)
        tree = git(repo, "write-tree", env=snapshot_env)

    message = f"working baseline {args.method_id} {args.version}\n\n{args.description.strip()}"
    commit = git(repo, "commit-tree", tree, "-p", "HEAD", stdin=message)
    git(repo, "tag", "-a", tag, commit, "-m", message)
    print(f"Captured {tag} at {commit}")


if __name__ == "__main__":
    main()
