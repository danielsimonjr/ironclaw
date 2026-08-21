#!/usr/bin/env python3
"""Collapse duplicated release sections in CHANGELOG.md.

THE DEFECT. With `publish = false`, release-plz determines which versions are already
released FROM GIT TAGS. The `release-plz-pr` job ran concurrently with
`release-plz-release`, so it could generate a changelog before the tag it should
measure from existed -- and a missing tag reads as "nothing was ever released", so it
emitted the ENTIRE project history under the next version heading.

That happened roughly 9 times across 23 version sections. The file reached 2371 lines,
with individual entries appearing 12 times each.

THE RULE. A bullet belongs to the release in which the work SHIPPED, which is its
EARLIEST occurrence in the file. Sections are therefore processed oldest-first and the
first occurrence of each bullet is kept; every later repeat is a re-emission and is
dropped. Nothing is rewritten or reworded -- lines are only removed -- so anything
surviving is text that was already there.

Deliberately conservative:
  * `## [Unreleased]` and the file preamble are untouched.
  * A subsection (`### Added`) is removed only when every bullet under it went.
  * A version section that ends up empty keeps its heading: the release happened, and
    a heading with no bullets is honest about a release that shipped no described
    change. Deleting it would rewrite the release history rather than de-duplicate it.

Run with --check to report without writing (exit 1 if duplicates remain).
"""
from __future__ import annotations

import argparse
import io
import re
from pathlib import Path

VERSION_RE = re.compile(r"^## \[")
SUBSECTION_RE = re.compile(r"^### ")
BULLET_RE = re.compile(r"^- ")


def split_sections(lines):
    """Split the file into (preamble, [(heading_line, body_lines), ...])."""
    first = next((i for i, ln in enumerate(lines) if VERSION_RE.match(ln)), len(lines))
    preamble, sections, cur = lines[:first], [], None
    for ln in lines[first:]:
        if VERSION_RE.match(ln):
            cur = (ln, [])
            sections.append(cur)
        elif cur is not None:
            cur[1].append(ln)
    return preamble, sections


def dedupe(sections):
    """Drop each bullet from every section after the earliest one containing it.

    `sections` arrives newest-first, as the file is written. It is walked in reverse
    so the OLDEST release claims each bullet -- that is the release in which the work
    actually shipped.
    """
    seen, removed, kept = set(), 0, 0
    out = [None] * len(sections)
    for idx in range(len(sections) - 1, -1, -1):
        heading, body = sections[idx]
        if "[Unreleased]" in heading:
            out[idx] = (heading, body)
            continue
        new_body, current_sub, sub_kept = [], None, 0

        def flush(sub, count, dest):
            # A subsection header survives only if something under it did.
            if sub is not None and count:
                dest.insert(sub["at"], sub["line"])

        for ln in body:
            if SUBSECTION_RE.match(ln):
                flush(current_sub, sub_kept, new_body)
                current_sub, sub_kept = {"line": ln, "at": len(new_body)}, 0
                continue
            if BULLET_RE.match(ln):
                key = ln.strip()
                if key in seen:
                    removed += 1
                    continue
                seen.add(key)
                kept += 1
                sub_kept += 1
                new_body.append(ln)
                continue
            new_body.append(ln)
        flush(current_sub, sub_kept, new_body)
        # Collapse runs of blank lines left behind by removed bullets.
        squeezed, blank = [], False
        for ln in new_body:
            if ln.strip() == "":
                if blank:
                    continue
                blank = True
            else:
                blank = False
            squeezed.append(ln)
        while squeezed and squeezed[-1].strip() == "":
            squeezed.pop()
        out[idx] = (heading, squeezed + [""])
    return out, kept, removed


def main() -> int:
    """De-duplicate CHANGELOG.md, or report on it with --check."""
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--path", default="CHANGELOG.md")
    ap.add_argument("--check", action="store_true", help="report only; do not write")
    args = ap.parse_args()

    path = Path(args.path)
    lines = io.open(path, encoding="utf-8").read().splitlines()
    preamble, sections = split_sections(lines)

    total_bullets = sum(1 for ln in lines if BULLET_RE.match(ln))
    distinct = len({ln.strip() for ln in lines if BULLET_RE.match(ln)})
    print("  before: %d lines, %d section(s), %d bullet(s), %d distinct"
          % (len(lines), len(sections), total_bullets, distinct))

    new_sections, kept, removed = dedupe(sections)
    rebuilt = list(preamble)
    for heading, body in new_sections:
        rebuilt.append(heading)
        rebuilt.extend(body)
    while rebuilt and rebuilt[-1].strip() == "":
        rebuilt.pop()

    print("  after:  %d lines, %d bullet(s) kept, %d duplicate(s) removed"
          % (len(rebuilt) + 1, kept, removed))

    # The invariant worth asserting: de-duplication must not invent or lose content.
    # Every distinct bullet that existed must still exist exactly once.
    after_bullets = [ln.strip() for ln in rebuilt if BULLET_RE.match(ln)]
    assert len(after_bullets) == len(set(after_bullets)), "a duplicate survived"
    assert set(after_bullets) == {ln.strip() for ln in lines if BULLET_RE.match(ln)}, \
        "the distinct bullet set changed -- content was lost or invented"
    print("  invariant OK: every distinct bullet survives exactly once")

    if args.check:
        return 1 if removed else 0
    io.open(path, "w", encoding="utf-8", newline="\n").write("\n".join(rebuilt) + "\n")
    print("  written: %s" % path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
