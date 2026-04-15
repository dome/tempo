#!/usr/bin/env python3
"""Detect which publish groups are affected by a git diff."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys


PR_ALLOY_EXTRAS = (
    "scripts/publish/alloy.sh",
    "scripts/sanitize_source.py",
)
PR_REVM_EXTRAS = ("scripts/publish/revm.sh",)
PR_SHARED_FILES = (
    "Cargo.toml",
    "scripts/sanitize_toml.py",
)


def _expand_crate_prefixes(names: str) -> list[str]:
    return [f"crates/{name.strip()}/" for name in names.split(",") if name.strip()]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-sha", required=True)
    parser.add_argument("--head-sha", required=True)
    parser.add_argument("--alloy-crates", required=True)
    parser.add_argument("--revm-crates", required=True)
    parser.add_argument("--context", choices=("pr", "release"), default="pr")
    parser.add_argument("--label", default="Files changed:")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    alloy_prefixes = _expand_crate_prefixes(args.alloy_crates)
    revm_prefixes = _expand_crate_prefixes(args.revm_crates)

    if args.context == "pr":
        alloy_prefixes += list(PR_ALLOY_EXTRAS)
        revm_prefixes += list(PR_REVM_EXTRAS)
        shared_files = set(PR_SHARED_FILES)
    else:
        shared_files = set()

    files = subprocess.check_output(
        ["git", "diff", "--name-only", args.base_sha, args.head_sha],
        text=True,
    ).splitlines()

    alloy = any(path.startswith(tuple(alloy_prefixes)) for path in files)
    revm = any(path.startswith(tuple(revm_prefixes)) for path in files)

    if any(path in shared_files for path in files):
        alloy = True
        revm = True

    output_path = os.environ.get("GITHUB_OUTPUT")
    if output_path:
        with open(output_path, "a", encoding="utf-8") as fh:
            print(f"alloy={str(alloy).lower()}", file=fh)
            print(f"revm={str(revm).lower()}", file=fh)
    else:
        print(f"alloy={str(alloy).lower()}")
        print(f"revm={str(revm).lower()}")

    print(args.label)
    for path in files:
        print(f"  {path}")
    print(f"Alloy group: {alloy}")
    print(f"Revm group: {revm}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
