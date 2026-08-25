#!/usr/bin/env python3
"""The tag's own CHANGELOG section, reflowed so GitHub does not break it mid-sentence.

# Why this exists

Release notes render as **GFM**, and GFM turns a single newline into a `<br>`. Confirmed against
GitHub's own API rather than assumed:

    POST /markdown  {"text": "first line\\nsecond line", "mode": "gfm"}
      -> <p>first line<br>second line</p>
    POST /markdown  {"text": "first line\\nsecond line", "mode": "markdown"}
      -> <p>first line second line</p>

`CHANGELOG.md` is hard-wrapped at 100 columns for the person reading it in an editor and in a diff,
which is the right shape for a file under review. Fed to a release verbatim, every one of those
wraps becomes a line break, and the notes arrive ragged — broken after "added" and before "the",
in spots no author chose.

So the file stays wrapped and the *notes* are reflowed. The alternative — writing the CHANGELOG in
one-line paragraphs — would make every edit a whole-paragraph diff and punish the reader who is
actually reviewing it, to please a renderer.

# What is left exactly as written

Anything where a line ending is content rather than typography: fenced code blocks, tables, headings,
list-item boundaries, blockquotes, and a line ending in two spaces, which is Markdown's own way of
asking for a break. Only the *continuation* lines of a paragraph are joined, which is the only place
the wrapping was ever typography.
"""

import re
import sys

# A list item's own marker: `-`, `*`, `+`, or `1.` / `1)`, at any indent.
LIST_ITEM = re.compile(r"^\s*(?:[-*+]|\d+[.)])\s+")


def section(text: str, version: str) -> str:
    """The lines under `## [version]`, up to the next `## [`."""
    out, found = [], False
    for line in text.splitlines():
        if line.startswith("## ["):
            if found:
                break
            found = line.startswith(f"## [{version}]")
            continue
        if found:
            out.append(line)
    return "\n".join(out)


def reflow(text: str) -> str:
    """Join the continuation lines of each paragraph, leaving everything else alone."""
    out: list[str] = []
    pending: list[str] = []
    in_fence = False

    def flush() -> None:
        if pending:
            out.append(" ".join(pending))
            pending.clear()

    for line in text.splitlines():
        stripped = line.strip()

        if stripped.startswith("```") or stripped.startswith("~~~"):
            flush()
            out.append(line)
            in_fence = not in_fence
            continue
        if in_fence:
            out.append(line)
            continue

        # A blank line ends a paragraph; a table row, heading or quote is its own line.
        if not stripped or stripped.startswith(("|", "#", ">")):
            flush()
            out.append(line)
            continue

        # A list item starts a new logical line rather than joining the one above it.
        if LIST_ITEM.match(line):
            flush()
            pending.append(line.rstrip())
            continue

        # Two trailing spaces is Markdown asking for a break. Honour it.
        if line.endswith("  "):
            pending.append(line.strip())
            flush()
            continue

        if pending:
            pending.append(stripped)
        else:
            pending.append(line.rstrip())

    flush()
    return "\n".join(out).strip() + "\n"


def _self_test() -> int:
    """`--self-test`: the shapes that must survive, and the one that must not.

    Run by the gate. A reflow that silently ate a table would produce notes nobody proofreads,
    because the whole point of generating them is that nobody proofreads them.
    """
    cases = [
        # (what it is, input, expected)
        ("a wrapped paragraph joins", "one two\nthree four\n", "one two three four\n"),
        ("a blank line still separates", "a\n\nb\n", "a\n\nb\n"),
        (
            "a table keeps every row on its own line",
            "| a | b |\n|---|---|\n| 1 | 2 |\n",
            "| a | b |\n|---|---|\n| 1 | 2 |\n",
        ),
        (
            "a fenced block is verbatim, wrapping and all",
            "```\nlet x =\n    1;\n```\n",
            "```\nlet x =\n    1;\n```\n",
        ),
        (
            "list items stay separate and their continuations join",
            "- first item\n  wrapped on\n- second item\n",
            "- first item wrapped on\n- second item\n",
        ),
        ("a heading is its own line", "## h\ntext\n", "## h\ntext\n"),
        (
            "two trailing spaces is an author asking for a break, and is kept",
            "keep this break  \nnext line\n",
            "keep this break\nnext line\n",
        ),
    ]
    failures = []
    for name, given, want in cases:
        got = reflow(given)
        if got != want:
            failures.append(f"{name}: wanted {want!r}, got {got!r}")
    for line in failures:
        print(line, file=sys.stderr)
    print(f"changelog-section: {len(cases) - len(failures)}/{len(cases)} shapes hold")
    return 1 if failures else 0


def main() -> int:
    if len(sys.argv) == 2 and sys.argv[1] == "--self-test":
        return _self_test()
    if len(sys.argv) != 3:
        print("usage: changelog-section.py <CHANGELOG.md> <version>", file=sys.stderr)
        return 2
    with open(sys.argv[1], encoding="utf-8") as handle:
        body = section(handle.read(), sys.argv[2])
    if not body.strip():
        return 1
    sys.stdout.write(reflow(body))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
