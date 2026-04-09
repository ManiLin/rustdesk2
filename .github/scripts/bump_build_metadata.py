#!/usr/bin/env python3
"""Increment +build.N in Cargo.toml and workflow VERSION lines (kept in sync with CI)."""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FILES = [
    ROOT / "Cargo.toml",
    ROOT / ".github" / "workflows" / "flutter-build.yml",
    ROOT / ".github" / "workflows" / "playground.yml",
]


def bump_ver(v: str) -> str:
    m = re.search(r"\+build\.(\d+)\s*$", v.strip())
    if m:
        n = int(m.group(1)) + 1
        return re.sub(r"\+build\.\d+\s*$", f"+build.{n}", v.strip())
    if re.match(r"^\d+\.\d+\.\d+$", v.strip()):
        return v.strip() + "+build.1"
    sys.exit(f"Cannot bump version (expected ...+build.N): {v!r}")


def main() -> None:
    cargo_txt = FILES[0].read_text(encoding="utf-8")
    m = re.search(r'^version = "([^"]+)"', cargo_txt, re.M)
    if not m:
        sys.exit("Cargo.toml: version not found")
    old = m.group(1)
    new = bump_ver(old)

    for p in FILES:
        text = p.read_text(encoding="utf-8")
        if p.name == "Cargo.toml":
            text2 = re.sub(
                r'^version = "[^"]+"',
                f'version = "{new}"',
                text,
                count=1,
                flags=re.M,
            )
        else:
            text2 = re.sub(
                r'^  VERSION: "[^"]+"',
                f'  VERSION: "{new}"',
                text,
                count=1,
                flags=re.M,
            )
        if text == text2:
            sys.exit(f"No version line updated in {p}")
        p.write_text(text2, encoding="utf-8")

    print(f"bump_build_metadata: {old} -> {new}")


if __name__ == "__main__":
    main()
