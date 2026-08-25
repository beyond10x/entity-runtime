#!/usr/bin/env python3
"""Say whether the pinned upstream fixture is still what upstream ships.

`check-pin.py` holds the copy honest against its own `PIN.md`. It cannot say anything about the
thing the copy is a copy *of* — and that is the half that went wrong: `vision.yaml` landed in
`engineering-protocols` 0.14.0 and this repository stayed green, its equivalence test asserting
agreement about eight ladders while nine existed. A guard that cannot see the fault it was built
for reads exactly like one that passed.

This is deliberately **not** a step of `task check`. The gate reaches no network, and a test whose
coverage depends on a sibling checkout says a different thing on a machine that has none — which is
the whole reason the fixture is committed. So the signal comes from outside the gate, on its own
clock, where a red run means one thing: upstream moved and somebody has to decide about it.

Three findings, each of which is a decision rather than a merge conflict:

1. a pinned ladder whose upstream content changed — the rungs moved;
2. an upstream ladder nothing pins — a new kind, like `vision`;
3. a pinned ladder that is gone upstream — a kind was retired.

Usage: check-upstream-pin.py <path-to-engineering-protocols-checkout>

Exit 0 in step, 1 drifted, 2 the upstream checkout is not one.
"""

from __future__ import annotations

import hashlib
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIXTURE = ROOT / "crates/entity-yaml/tests/fixtures/aep-lifecycles"
PIN = FIXTURE / "PIN.md"
UPSTREAM_LADDERS = "artifacts/lifecycles"

PINNED_COMMIT = re.compile(r"\|\s*pinned commit\s*\|\s*`([0-9a-f]{7,40})`")


def pinned_commit() -> str:
    match = PINNED_COMMIT.search(PIN.read_text())
    return match.group(1) if match else "unrecorded"


def upstream_head(checkout: Path) -> str:
    """The upstream commit to pin to, and the tag if it carries one — for a copy-pasteable refresh."""
    try:
        commit = subprocess.run(
            ["git", "-C", str(checkout), "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        return "unknown"
    # `--exact-match`, not `--abbrev=0`: the latter returns the nearest *reachable* tag, which is
    # not a tag on this commit. That is exactly how PIN.md acquired a label pointing at a different
    # sha than the one beside it, and an independent review had to find it.
    tag = subprocess.run(
        ["git", "-C", str(checkout), "describe", "--tags", "--exact-match", "HEAD"],
        capture_output=True,
        text=True,
        check=False,
    ).stdout.strip()
    return f"{commit} (tagged {tag})" if tag else f"{commit} (no tag on this commit)"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__.strip().splitlines()[-3])
        return 2
    checkout = Path(sys.argv[1])
    ladders = checkout / UPSTREAM_LADDERS
    if not ladders.is_dir():
        print(f"{ladders} is not a directory: that checkout is not engineering-protocols")
        return 2

    # `rglob`, and both spellings. A ladder upstream renamed `vision.yml`, or moved into a
    # subdirectory, would otherwise report zero findings and exit 0 — the same silent green this
    # script was written to end, reappearing inside it.
    def ladders_in(directory: Path) -> dict[str, Path]:
        found: dict[str, Path] = {}
        for path in sorted(directory.rglob("*")):
            if path.is_file() and path.suffix in {".yaml", ".yml"}:
                found[str(path.relative_to(directory))] = path
        return found

    here = ladders_in(FIXTURE)
    there = ladders_in(ladders)

    findings: list[str] = []
    for name in sorted(set(here) | set(there)):
        if name not in there:
            findings.append(f"`{name}` is pinned here and gone upstream — a kind was retired")
        elif name not in here:
            findings.append(f"`{name}` is a ladder upstream ships and nothing here pins")
        elif sha256(here[name]) != sha256(there[name]):
            findings.append(f"`{name}` differs — the rungs moved upstream")

    print(
        f"{len(here)} pinned, {len(there)} upstream, {len(findings)} finding(s); "
        f"pinned at {pinned_commit()}, upstream at {upstream_head(checkout)}"
    )
    for finding in findings:
        print(f"  {finding}")
    if findings:
        print(
            "\nRefresh: copy the ladders again into "
            "crates/entity-yaml/tests/fixtures/aep-lifecycles/, update PIN.md's commit and sums, "
            "add or remove the matching definition under examples/aep/, and run `task check`. "
            "The equivalence test failing after a refresh is the point — it means a ladder moved "
            "and examples/aep/ has not."
        )
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
