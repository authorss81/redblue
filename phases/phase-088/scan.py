#!/usr/bin/env python3
"""Find the tests that run a Redblue program and never assert anything about it.

The audit behind phase-088 said that "twenty tests across `tests/` have no
reachable `assert!` or `assert_eq!` at all". A count that is asserted rather
than derived is the defect this phase set out to remove, so the count has to
come out of a scanner anybody can run again.

    python3 phases/phase-088/scan.py tests            # this checkout
    python3 phases/phase-088/scan.py /tmp/base/tests   # another tree
    python3 phases/phase-088/scan.py --list tests      # the file list only

The scan, in the order it is applied:

1. Every `tests/*.rs` file is read — the top level of `tests/` only; the
   `tests/common/` helpers are not test files, they are read through the tests
   that call them.
2. Comments are masked out, and so are string literals — a test that mentions
   `take_output` inside a Redblue fixture is not a Rust assertion. The masking
   keeps every byte, so the file's line numbers and byte offsets are the same
   in the masked copy as in the original, and a second copy with only the
   comments masked keeps the fixtures.
3. Every top-level `fn` in the file is brace-matched, and the ones carrying
   `#[test]` are the tests. A `fn` nested inside another function is not a
   test, however many `#[test]` attributes are in the text before it.
4. A test is a **candidate** when it runs a program — its own body, or the
   body of a same-file helper it calls, mentions `.run(`, `run_source`,
   `run_isolated`, `run_chunk`, `tree_walk` or `bytecode`.
5. A candidate is **vacuous** when neither its own body nor the body of any
   same-file helper it calls asserts anything about the program.

What counts as an assertion, and what does not:

* `take_output`, `.output`, `stdout`, `take_expectation_failure` and
  `expect_err` count: they read what the program said, recorded or answered.
* `assert_eq!`, `assert_ne!` and `assert_matches!` count: they compare a value.
* `assert!` counts **unless** its whole condition is a reachability check —
  `is_ok()` with nothing else in it. `assert!(run_source(src).is_ok())` says
  "the program ran", which is the shape this scan is looking for. A condition
  that also names a value (`is_err()`, `==`, `contains`, `matches`) counts, so
  a test that asserts a *failure* was produced is never mistaken for a vacuous
  one.
* `panic!` does **not** count: a test that panics only on `Err` proves the
  program ran, and proves nothing about what it said.
* A Redblue-level `expect ... to be` **does** count. It lives inside a fixture,
  the program checks its own answer, and a failed `expect` is a `Runtime` error
  the run hands back (`src/interpreter.rs:2331`), so the Rust test around it
  fails when the check fails. The program's own check is an assertion, not a
  reachability check.

The helper step is the one that matters: six of the candidates an earlier scan
flagged are not defects — five of them assert through a helper (three through
`ok()` / `fails()`, whose `Result` is `.unwrap()`ed, and two through
`both_print`, which compares both engines' output to a literal) and the sixth
is a nested helper rather than a test at all — and a scan that cannot see
through a helper reads them as vacuous.

The output is one line per vacuous test: the file, the line the test is
defined on, and its name. The count of those lines is the number the report
has to state.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# The tokens that mean "this test runs a Redblue program".
RUNS_A_PROGRAM = (
    ".run(",
    "run_source",
    "run_isolated",
    "run_chunk",
    "tree_walk",
    "bytecode",
)

# The tokens that read what the program said, recorded or answered.
# `Ok(Value::` is a match on the program's own answer — `Ok(Value::YesNo(true))`
# is a value assertion written as a `match` instead of an `assert_eq!`.
READS_THE_PROGRAM = re.compile(
    r"take_output|\.output|stdout|take_expectation_failure|expect_err|Ok\(Value::"
)

# The assertion macros that compare a value.
COMPARES_A_VALUE = re.compile(r"assert_eq|assert_ne|assert_matches")

# A Redblue-level `expect`, which lives inside a fixture string.
REDBLUE_ASSERTION = re.compile(r"\bexpect\b[^\n]*\bto be\b|\bexpect\b[^\n]*\bto fail\b")


def blank(chunk: str) -> str:
    """`chunk`, with everything but its newlines blanked out."""
    return "".join("\n" if ch == "\n" else " " for ch in chunk)


def mask(text: str, mask_strings: bool) -> str:
    """Blank out comments (and optionally string literals), keeping every byte.

    Rust block comments nest, so the comment scanner counts them.
    """
    out = []
    i = 0
    n = len(text)
    while i < n:
        ch = text[i]
        if ch == "/" and i + 1 < n and text[i + 1] == "/":
            end = text.find("\n", i)
            end = n if end == -1 else end
            out.append(blank(text[i:end]))
            i = end
            continue
        if ch == "/" and i + 1 < n and text[i + 1] == "*":
            start = i
            depth = 0
            while i < n:
                if text.startswith("/*", i):
                    depth += 1
                    i += 2
                elif text.startswith("*/", i):
                    depth -= 1
                    i += 2
                    if depth == 0:
                        break
                else:
                    i += 1
            out.append(blank(text[start:i]))
            continue
        if mask_strings and ch == "r" and i + 1 < n and (text[i + 1] == '"' or text[i + 1] == "#"):
            j = i + 1
            hashes = 0
            while j < n and text[j] == "#":
                hashes += 1
                j += 1
            if j < n and text[j] == '"':
                close = '"' + "#" * hashes
                end = text.find(close, j + 1)
                end = n if end == -1 else end + len(close)
                out.append(blank(text[i:end]))
                i = end
                continue
        if mask_strings and ch == '"':
            start = i
            i += 1
            while i < n:
                if text[i] == "\\":
                    i += 2
                    continue
                if text[i] == '"':
                    i += 1
                    break
                i += 1
            out.append(blank(text[start:i]))
            continue
        if mask_strings and ch == "'":
            # A lifetime ('a) is not a char literal; a char literal is one
            # character, optionally escaped, then a closing quote.
            j = i + 1
            if j < n and text[j] == "\\":
                j += 2
                while j < n and text[j] != "'":
                    j += 1
                j += 1
            elif j < n:
                j += 1
                while j < n and text[j] != "'":
                    j += 1
                j += 1
            literal = text[i:j]
            if len(literal) <= 4 and literal.endswith("'"):
                out.append(blank(literal))
                i = j
                continue
        out.append(ch)
        i += 1
    return "".join(out)


def match_brace(text: str, start: int) -> int:
    """The index just past the `}` that closes the `{` at `start`."""
    depth = 0
    for i in range(start, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return i + 1
    return len(text)


def match_paren(text: str, start: int) -> int:
    """The index just past the `)` that closes the `(` at `start`."""
    depth = 0
    for i in range(start, len(text)):
        if text[i] == "(":
            depth += 1
        elif text[i] == ")":
            depth -= 1
            if depth == 0:
                return i + 1
    return len(text)


def find_functions(code: str):
    """Every top-level `fn` in `code`: (name, line, body, the text before it).

    A `fn` nested inside another function is not a test, however many
    `#[test]` attributes are in the text before it — the attribute belongs to
    the function it is written in.
    """
    functions = []
    for match in re.finditer(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)", code):
        if code.count("{", 0, match.start()) != code.count("}", 0, match.start()):
            continue
        brace = code.find("{", match.end())
        if brace == -1:
            continue
        end = match_brace(code, brace)
        line = code.count("\n", 0, match.start()) + 1
        functions.append((match.group(1), line, brace, end, code[: match.start()]))
    return functions


def called_names(body: str) -> set[str]:
    """The names a body calls, so a helper's body can be followed."""
    return set(re.findall(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\(", body))


def is_reachability_check(argument: str) -> bool:
    """Whether an `assert!`'s condition says only "the program ran".

    `is_ok()` with nothing else in the condition is a reachability check.
    A condition that also names a value — `is_err()`, `==`, `contains`,
    `matches` — is an assertion about the program's answer, not about whether
    it ran at all.
    """
    if "is_ok()" not in argument:
        return False
    for token in ("is_err()", "==", "!=", "contains", "matches"):
        if token in argument:
            return False
    return True


def asserts_something(code: str) -> bool:
    """Whether a body asserts anything about the program it ran."""
    if READS_THE_PROGRAM.search(code):
        return True
    if COMPARES_A_VALUE.search(code):
        return True
    for match in re.finditer(r"\bassert!", code):
        open_paren = code.find("(", match.end())
        if open_paren == -1:
            continue
        argument = code[open_paren + 1 : match_paren(code, open_paren)]
        if not is_reachability_check(argument):
            return True
    return False


def scan_file(path: Path) -> list[tuple[str, int]]:
    """The vacuous tests in one file: (name, line)."""
    raw = path.read_text(encoding="utf-8", errors="replace")
    code = mask(mask(raw, mask_strings=True), mask_strings=False)
    words = mask(raw, mask_strings=False)

    functions = find_functions(code)
    bodies = {name: (brace, end) for name, _line, brace, end, _before in functions}

    def helper_reach(name: str, seen: set[str]) -> tuple[str, str]:
        """The code and the fixture text of `name` and of every helper it calls."""
        if name in seen or name not in bodies:
            return "", ""
        seen.add(name)
        brace, end = bodies[name]
        body = code[brace:end]
        fixtures = words[brace:end]
        for callee in called_names(body):
            sub_code, sub_fixtures = helper_reach(callee, seen)
            body += sub_code
            fixtures += sub_fixtures
        return body, fixtures

    vacuous = []
    for name, line, brace, end, before in functions:
        attributes = before.rsplit("}", 1)[-1]
        if "#[test]" not in attributes:
            continue
        body = code[brace:end]
        fixtures = words[brace:end]
        reached = body
        for callee in called_names(body):
            sub_code, sub_fixtures = helper_reach(callee, set())
            reached += sub_code
            fixtures += sub_fixtures
        if not any(token in reached for token in RUNS_A_PROGRAM):
            continue
        if asserts_something(reached):
            continue
        if REDBLUE_ASSERTION.search(fixtures):
            continue
        vacuous.append((name, line))
    return vacuous


def main(argv: list[str]) -> int:
    args = argv[1:]
    list_only = False
    if args and args[0] == "--list":
        list_only = True
        args = args[1:]
    if len(args) != 1:
        print(__doc__)
        return 2
    root = Path(args[0])
    files = sorted(root.glob("*.rs"))
    if list_only:
        for path in files:
            print(path.name)
        return 0
    print(f"scanned {len(files)} files in {root}")
    total = 0
    for path in files:
        for name, line in scan_file(path):
            print(f"{path.name}:{line}\t{name}")
            total += 1
    print(f"vacuous tests: {total}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
