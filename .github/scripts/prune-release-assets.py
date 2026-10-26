#!/usr/bin/env python3
"""Delete release assets of old app versions, keeping the newest N versions.

Usage:
    prune-release-assets.py <owner/repo> <base-version> <keep> <tag> [<tag> ...]

Example:
    prune-release-assets.py ManiLin/rustdesk2 1.4.9 3 upgrade-1.4.9 upgrade-1.4.9-cashdesk

Assets are grouped by the build number in their name: `1.4.9-15` -> 15,
plain `1.4.9` -> 0. Assets without the base version in their name are kept.
Releases that do not exist are skipped. Set DRY_RUN=1 to only print.
Needs the `gh` CLI with GH_TOKEN (contents: write).
"""
import json
import os
import re
import subprocess
import sys


def gh(*args: str) -> str:
    return subprocess.run(
        ["gh", *args], check=True, capture_output=True, text=True
    ).stdout


def main() -> int:
    if len(sys.argv) < 5:
        print(__doc__)
        return 2
    repo, base, keep = sys.argv[1], sys.argv[2], int(sys.argv[3])
    tags = sys.argv[4:]
    dry_run = os.environ.get("DRY_RUN") == "1"
    version_re = re.compile(re.escape(base) + r"(?:-(\d+))?(?=[-_.+])")

    for tag in tags:
        try:
            release = json.loads(gh("api", f"repos/{repo}/releases/tags/{tag}"))
        except subprocess.CalledProcessError:
            print(f"{tag}: no such release, skipped")
            continue
        lines = gh(
            "api",
            "--paginate",
            f"repos/{repo}/releases/{release['id']}/assets?per_page=100",
            "--jq",
            ".[] | {id, name}",
        ).splitlines()
        assets = [json.loads(line) for line in lines if line.strip()]
        versioned = []
        for asset in assets:
            match = version_re.search(asset["name"])
            if match:
                versioned.append((int(match.group(1) or 0), asset))
        newest = sorted({n for n, _ in versioned}, reverse=True)[:keep]
        old = [(n, a) for n, a in versioned if n not in newest]
        kept = ", ".join(f"{base}-{n}" for n in newest) or "-"
        print(
            f"{tag}: {len(assets)} assets, keeping {kept}, "
            f"{'would delete' if dry_run else 'deleting'} {len(old)}"
        )
        for n, asset in sorted(old, key=lambda x: (x[0], x[1]["name"])):
            print(f"  - {asset['name']}")
            if not dry_run:
                gh("api", "-X", "DELETE", f"repos/{repo}/releases/assets/{asset['id']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
