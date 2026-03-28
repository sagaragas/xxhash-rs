#!/usr/bin/env python3
"""
Style and hygiene gate for the xxhash-rs publication artifacts.

Checks that publication-facing files do not contain:
- TODO/TBD/FIXME markers
- Raw HTML comments
- Internal tooling tokens that should not appear in public artifacts
- Absolute local paths that would not resolve in public context
- Unscoped superlative claims

Usage:
    python3 publication/style_gate.py
"""

import json
import re
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
PUBLICATION_DIR = REPO_ROOT / "publication"
EVIDENCE_DIR = PUBLICATION_DIR / "evidence"

# Patterns that should not appear in public-facing publication artifacts
DRAFT_MARKERS = re.compile(r'\b(TODO|TBD|FIXME|HACK|XXX)\b', re.IGNORECASE)
HTML_COMMENTS = re.compile(r'<!--.*?-->', re.DOTALL)
INTERNAL_TOKENS = re.compile(r'(mission-worker|EndFeatureRun|worker-base|skill_name)', re.IGNORECASE)
ABSOLUTE_LOCAL = re.compile(r'/Users/\w+/')

# Unscoped superlative claims that require qualification
UNSCOPED_CLAIMS = re.compile(
    r'\b(full compatibility|drop-in replacement|production-ready|'
    r'always faster|universally faster|fastest|100% compatible)\b',
    re.IGNORECASE,
)


def scan_file(path: Path) -> list:
    """Scan a single file for style violations."""
    errors = []
    try:
        content = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return [f"Cannot read {path.relative_to(REPO_ROOT)}"]

    rel = str(path.relative_to(REPO_ROOT))

    # For JSON files, check string values only
    if path.suffix == ".json":
        try:
            data = json.loads(content)
            content = json.dumps(data)  # Flatten to check string values
        except json.JSONDecodeError:
            errors.append(f"{rel}: Invalid JSON")
            return errors

    for lineno, line in enumerate(content.splitlines(), 1):
        if DRAFT_MARKERS.search(line):
            errors.append(f"{rel}:{lineno}: Draft marker found: {line.strip()[:80]}")

        if HTML_COMMENTS.search(line):
            errors.append(f"{rel}:{lineno}: Raw HTML comment found")

        if INTERNAL_TOKENS.search(line):
            errors.append(f"{rel}:{lineno}: Internal tooling token found: {line.strip()[:80]}")

        if ABSOLUTE_LOCAL.search(line):
            # Allow in evidence JSON where paths are recorded from test runs
            # but flag in prose/markdown files
            if path.suffix in (".md", ".mdx", ".txt"):
                errors.append(f"{rel}:{lineno}: Absolute local path found: {line.strip()[:80]}")

        if UNSCOPED_CLAIMS.search(line):
            if path.suffix in (".md", ".mdx", ".txt"):
                errors.append(f"{rel}:{lineno}: Unscoped superlative claim: {line.strip()[:80]}")

    return errors


def main():
    print("=== Publication Style Gate ===\n")

    all_errors = []

    # Scan publication directory (excluding evidence JSON which has machine paths)
    scan_targets = []

    # Scan markdown/prose files
    for ext in ("*.md", "*.mdx", "*.txt"):
        scan_targets.extend(PUBLICATION_DIR.rglob(ext))

    # Scan evidence JSON for structural issues only
    if EVIDENCE_DIR.exists():
        for f in EVIDENCE_DIR.glob("*.json"):
            scan_targets.append(f)

    if not scan_targets:
        print("  No publication files found to scan.")
        print("\nOK: Style gate passed (no files to check)")
        return 0

    for path in sorted(set(scan_targets)):
        errors = scan_file(path)
        if errors:
            all_errors.extend(errors)

    if all_errors:
        print(f"FAIL: {len(all_errors)} style violation(s):")
        for e in all_errors:
            print(f"  - {e}")
        return 1

    print(f"  Scanned {len(scan_targets)} file(s)")
    print("\nOK: Style gate passed")
    return 0


if __name__ == "__main__":
    sys.exit(main() or 0)
