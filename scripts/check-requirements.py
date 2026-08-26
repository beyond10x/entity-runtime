#!/usr/bin/env python3
"""Keep docs/requirements.md honest.

Five checks, each of which has a failure mode that reads as success without it:

1. every requirement id `R-nn` in the register is referenced at least once in a design document
   under docs/design/ — a requirement nobody designed for is a requirement nobody will notice
   is missing;
2. every test name the register cites in its `pinned by` column is a real **test** — a `fn`
   carrying `#[test]` under crates/. Any function used to satisfy the regex would make the pin a
   citation with nothing behind it, which is the defect this script exists to catch;
3. a cited test is not `#[ignore]`d — a pin that never runs is not a pin;
4. every requirement row states either a test or the literal `design` / `type` / `manifest`
   as its evidence, so "not pinned" cannot hide behind an empty cell;
5. every `R-nn` the register mentions actually has a row. A row whose id cell carried a marker
   once failed to match the row pattern, so the requirement was checked by nothing at all and the
   count silently dropped — a check that cannot see a row reads exactly like a row that passed.

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
# `#[test]`, then any further attributes, then the function. Captures the attributes so an
# `#[ignore]`d test can be told from a live one.
TEST_FN = re.compile(r"#\[test\]((?:\s*#\[[^\]]*\])*)\s*fn\s+([a-z][a-z0-9_]*)\s*\(")
ALLOWED_EVIDENCE = {"design", "type", "manifest"}


def collect_tests() -> tuple[set[str], set[str]]:
    """Every `#[test]` function under crates/, and the subset that is ignored."""
    live: set[str] = set()
    ignored: set[str] = set()
    for path in sorted(CRATES.rglob("*.rs")):
        for attributes, name in TEST_FN.findall(path.read_text(encoding="utf-8")):
            if "ignore" in attributes:
                ignored.add(name)
            else:
                live.add(name)
    return live, ignored


def main() -> int:
    findings: list[str] = []
    register = REGISTER.read_text(encoding="utf-8")
    designs = "\n".join(p.read_text(encoding="utf-8") for p in sorted(DESIGN_DIR.glob("*.md")))
    live, ignored = collect_tests()

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
            findings.append(
                f"{rid}: evidence cell names neither a test nor design/type/manifest: {evidence!r}"
            )
        for name in cited:
            if name in ignored:
                findings.append(f"{rid}: cites `{name}`, which is #[ignore]d and never runs")
            elif name not in live:
                findings.append(
                    f"{rid}: cites `{name}`, which is not a #[test] function under crates/"
                )

    mentioned = set(re.findall(r"\bR-\d+\b", register))
    for rid in sorted(mentioned - set(ids)):
        findings.append(f"{rid}: mentioned in the register but has no row the checker can parse")

    duplicates = {rid for rid in ids if ids.count(rid) > 1}
    for rid in sorted(duplicates):
        findings.append(f"{rid}: appears more than once in the register")

    if not ids:
        findings.append("no requirement rows found in docs/requirements.md")

    print(
        f"{len(ids)} requirement(s), {len(live)} test function(s) under crates/, "
        f"{len(findings)} finding(s)"
    )
    for finding in findings:
        print(f"  {finding}")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
