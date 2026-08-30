#!/usr/bin/env python3
"""Refuse a `protocol` too old to be trusted with this store.

`protocol artifact` is the only thing that writes this repository's planning store, and the
version on PATH is ambient — nothing here pins it, and nothing about running it says which build
answered.

This is not hypothetical. On 2026-08-26 the installed binary predated the store journal. Six
status moves ran through it; each printed `moved draft -> proposed (revision 2)` and appended
**nothing** to `journal.jsonl`. The files said `implemented`, the record said the artifact had only
ever been created, and the gate was green throughout — because `artifact validate` reads the files
and the files were fine.

So the version is checked before the store is. `protocol --version` prints the workspace version of
`engineering-protocols`, which as of its 0.26.0 is checked against that repository's newest release
tag by its own gate — so the number means something again, and a build older than the journal can
be told from one that has it.

Exit 0 clean, 1 findings, 2 if `protocol` cannot be run at all.
"""

from __future__ import annotations

import re
import shutil
import subprocess
import sys

# The release that made `protocol --version` meaningful, which is also the first that reports a
# number this check can compare. Anything below it either predates the journal or cannot say.
MINIMUM = (0, 26, 0)


def parse(text: str) -> tuple[int, ...] | None:
    """The first dotted number in `protocol --version`, as a tuple."""
    found = re.search(r"(\d+)\.(\d+)\.(\d+)", text)
    return tuple(int(part) for part in found.groups()) if found else None


def main() -> int:
    binary = shutil.which("protocol")
    if binary is None:
        print("protocol is not on PATH, and this repository's planning store is written with it.")
        print("  install it: cargo install --path crates/protocol-cli  (in engineering-protocols)")
        return 2

    try:
        result = subprocess.run(
            [binary, "--version"], capture_output=True, text=True, timeout=30, check=False
        )
    except OSError as error:
        print(f"protocol at {binary} could not be run: {error}")
        return 2

    if result.returncode != 0:
        print(f"protocol at {binary} exited {result.returncode} while reporting its version:")
        print(f"  {result.stdout.strip() or result.stderr.strip()!r}")
        return 1

    version = parse(result.stdout) or parse(result.stderr)
    if version is None:
        print(f"protocol at {binary} printed no version this check can read:")
        print(f"  {result.stdout.strip() or result.stderr.strip()!r}")
        return 1

    wanted = ".".join(str(part) for part in MINIMUM)
    have = ".".join(str(part) for part in version)
    if version < MINIMUM:
        print(f"protocol at {binary} reports {have}; this store needs at least {wanted}.")
        print("  A build below 0.26.0 either predates the journal — in which case a status move")
        print("  writes no record and says nothing — or reports a version that never moved, in")
        print("  which case it cannot be told from one that does.")
        print("  Reinstall: cargo install --path crates/protocol-cli  (in engineering-protocols)")
        return 1

    print(f"protocol {have} at {binary} (needs {wanted})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
