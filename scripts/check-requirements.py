#!/usr/bin/env python3
"""Keep docs/requirements.md honest.

Three checks, each of which has a failure mode that reads as success without it:

1. every requirement id `R-nn` in the register is referenced at least once in a design document
   under docs/design/ — a requirement nobody designed for is a requirement nobody will notice
   is missing;
2. every test name the register cites in its `pinned by` column exists as a `fn <name>` under
   crates/ — a citation of a deleted test is a claim with nothing behind it;
3. every requirement row states either a test or the literal `design` / `type` / `manifest`
   as its evidence, so "not pinned" cannot hide behind an empty cell.

Exit 0 clean, 1 findings.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REGISTER = ROOT / "docs" / "requirements.md"
DESIGN_DIR = ROOT / "docs" / "design"
CRATES = ROOT / "crates"

ROW = re.compile(r"^\|\s*(R-\d+)\s*\|(.*)\|\s*$")
TEST_REF = re.compile(r"`([a-z][a-z0-9_]*)`")
ALLOWED_EVIDENCE = {"design", "type", "manifest"}


def main() -> int:
    findings: list[str] = []
    register = REGISTER.read_text(encoding="utf-8")
    designs = "\n".join(p.read_text(encoding="utf-8") for p in sorted(DESIGN_DIR.glob("*.md")))
    sources = "\n".join(p.read_text(encoding="utf-8") for p in sorted(CRATES.rglob("*.rs")))
    defined_tests = set(re.findall(r"^\s*fn ([a-z][a-z0-9_]*)\s*\(", sources, flags=re.M))

    ids: list[str] = []
    for line in register.splitlines():
        match = ROW.match(line)
        if not match:
            continue
        rid, rest = match.group(1), match.group(2)
        ids.append(rid)
        cells = [c.strip() for c in rest.split("|")]
        evidence = cells[-1] if cells else ""
        if not re.search(rf"\b{re.escape(rid)}\b", designs):
            findings.append(f"{rid}: not referenced by any document in docs/design/")
        cited = TEST_REF.findall(evidence)
        if not cited and not any(word in ALLOWED_EVIDENCE for word in evidence.split()):
            findings.append(f"{rid}: evidence cell is empty or names neither a test nor design/type/manifest: {evidence!r}")
        for name in cited:
            if name not in defined_tests:
                findings.append(f"{rid}: cites test `{name}`, which does not exist under crates/")

    duplicates = {rid for rid in ids if ids.count(rid) > 1}
    for rid in sorted(duplicates):
        findings.append(f"{rid}: appears more than once in the register")

    if not ids:
        findings.append("no requirement rows found in docs/requirements.md")

    print(f"{len(ids)} requirement(s), {len(defined_tests)} test function(s) under crates/, {len(findings)} finding(s)")
    for finding in findings:
        print(f"  {finding}")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
