#!/usr/bin/env python3
"""Keep every pinned upstream fixture honest.

A fixture copied from another repository is only evidence if what is on disk is what the pin says
it is. `PIN.md` records a sha256 per file; nothing checked it, so a refresh that updated the YAML
and forgot the sums — or the reverse — went green while the whole claim the fixture supports had
quietly stopped being true. That failure reads exactly like success, which is what this script
exists to stop.

Every `PIN.md` under crates/ is a pin. Its fenced block holds `<sha256>  <filename>` lines, and
each names a file beside it. Three findings:

1. a listed file is missing;
2. a listed file's contents no longer hash to the recorded sum;
3. a file sits beside the pin and is listed by nothing — an unpinned fixture is one nobody is
   comparing against, which is the same gap wearing different clothes.

Exit 0 clean, 1 findings.
"""

from __future__ import annotations

import hashlib
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATES = ROOT / "crates"

SUM_LINE = re.compile(r"^([0-9a-f]{64})\s+(\S+)$")


def recorded_sums(pin: Path) -> dict[str, str]:
    """The `<sha256>  <name>` lines of a pin, whatever else the document says around them."""
    sums: dict[str, str] = {}
    for line in pin.read_text().splitlines():
        match = SUM_LINE.match(line.strip())
        if match:
            sums[match.group(2)] = match.group(1)
    return sums


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check(pin: Path) -> list[str]:
    findings: list[str] = []
    where = pin.relative_to(ROOT)
    sums = recorded_sums(pin)
    if not sums:
        return [f"{where}: records no sha256 sums, so it pins nothing"]

    for name, expected in sorted(sums.items()):
        target = pin.parent / name
        if not target.is_file():
            findings.append(f"{where}: pins `{name}`, which is not there")
            continue
        actual = sha256(target)
        if actual != expected:
            findings.append(
                f"{where}: `{name}` is {actual[:12]}…, pinned as {expected[:12]}… — "
                "the copy and the pin disagree, so decide which one moved"
            )

    beside = {path.name for path in pin.parent.iterdir() if path.is_file()}
    for name in sorted(beside - set(sums) - {pin.name}):
        findings.append(f"{where}: `{name}` sits beside the pin and is pinned by nothing")

    return findings


def main() -> int:
    pins = sorted(CRATES.rglob("PIN.md"))
    findings = [finding for pin in pins for finding in check(pin)]

    files = sum(len(recorded_sums(pin)) for pin in pins)
    print(f"{len(pins)} pin(s), {files} pinned file(s), {len(findings)} finding(s)")
    for finding in findings:
        print(f"  {finding}")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
