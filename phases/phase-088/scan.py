#!/usr/bin/env python3
"""Find the tests that run a Redblue program and never compare what it answered.

The audit behind phase-088 said that "twenty tests across `tests/` have no
reachable `assert!` or `assert_eq!` at all". A count that is asserted rather
than derived is the defect this phase set out to remove, so the count has to
come out of a scanner anybody can run again.

    python3 phases/phase-088/scan.py tests            # this checkout
    python3 phases/phase-088/scan.py /tmp/base/tests   # another tree
    python3 phases/phase-088/scan.py --list tests      # the file list only

The scan, in the order it is applied:

1. Every `tests/*.rs` file is read — the top level of `tests/` only. The
   `tests/common/` helpers are read as well, and they are followed: the
   modules `tests/common/mod.rs` declares are loaded, a qualified call such as
   `corpus::compare(...)` is followed into `tests/common/corpus.rs`, and a
   bare call is resolved in the file being scanned first and in the modules
   after it. A test whose assertion lives in one of those helpers is
   therefore seen through it.
2. Comments are masked out, and so are string literals — a test that mentions
   `take_output` inside a Redblue fixture is not a Rust assertion. The masking
   keeps every byte, so the file's line numbers and byte offsets are the same
   in the masked copy as in the original, and a second copy with only the
   comments masked keeps the fixtures.
3. Every top-level `fn` in the file is brace-matched, and the ones carrying
   `#[test]` are the tests. A `fn` nested inside another function is not a
   test, however many `#[test]` attributes are in the text before it.
4. A test is a **candidate** when it runs a program — its own body, or the
   body of a helper it calls, mentions `.run(`, `run_source`, `run_isolated`,
   `run_chunk`, `tree_walk` or `bytecode(`. The last one is spelled with its
   bracket on purpose: `redblue::bytecode::compile_program` compiles a program
   and does not run one, so a test that reads a compiled chunk's instructions
   is not a candidate — it asserts about the compiled form, which is a real
   thing to assert.
5. A candidate is **vacuous** when nothing in it compares what the program
   answered.

What counts as a comparison, and what does not:

* A comparison is a place where the program's answer is held against
  something else: an `assert_eq!`, `assert_ne!` or `assert_matches!`; an
  `assert!` whose condition says more than `is_ok()`; an `==` or `!=`; or a
  `match` on the program's own answer against a literal, such as
  `Ok(Value::YesNo(true))`.
* The comparison only counts if what it holds against something else is the
  program's **answer**: the output it printed (`take_output`, `.output`,
  `stdout`), the error it refused with (`expect_err`), the value it ended on
  (`Ok(Value::…)`), or a name bound to one of those. A test that reads the
  program's output and never compares it is vacuous, and so is a test whose
  only `assert_eq!` compares something that never came from the program.
* A name carries the answer forward in four shapes, and all four are followed:
  it is **bound** to the answer (`let lines = vm.take_output();`), **bound** to
  a name that already carries it (`let n = out.len();`), **put into** a
  collection the answer is read back out of (`seen.insert(item)`), or it is
  the object the program was **run through** (`harness.run_source(..)` — the
  harness holds the outcome of the program it ran).
* `assert!(run_source(src).is_ok())` does **not** count: it says "the program
  ran", which is the shape this scan is looking for. A condition that also
  names a value (`is_err()`, `==`, `contains`, `matches`) counts, so a test
  that asserts a *failure* was produced is never mistaken for a vacuous one.
* `panic!` does **not** count on its own: a test that panics only on `Err`
  proves the program ran, and proves nothing about what it said. A panic that
  guards a comparison counts, because the comparison it guards is the
  assertion — the shape `tests/common/corpus.rs::compare` is called in, where
  the comparison is in the helper and the panic is in the caller.
* A Redblue-level `expect ... to be` **does** count. It lives inside a fixture,
  the program checks its own answer, and a failed `expect` is a `Runtime` error
  the run hands back (`src/interpreter.rs:2331`), so the Rust test around it
  fails when the check fails. The program's own check is an assertion, not a
  reachability check.

The helper step is the one that matters: two of the candidates an earlier scan
flagged are not defects — they assert through `both_print`, which compares both
engines' output to a literal — and a scan that cannot see through a helper reads
them as vacuous. Each function is judged **on its own**, too: a name bound in one
function says nothing about a comparison in another, and a concatenation of two
functions' text would let the first function's names satisfy the second
function's comparison.

The output is one line per vacuous test: the file, the line the test is
defined on, and its name. The count of those lines is the number the report
has to state.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# The tokens that mean "this test runs a Redblue program".
#
# `bytecode(` rather than a bare `bytecode`, on purpose:
# `redblue::bytecode::compile_program` compiles a program and does not run one,
# so a test that compiles a program and reads its instructions is not a
# candidate — it asserts about the compiled form, which is a real thing to
# assert, and there is no program whose answer it could be getting wrong.
RUNS_A_PROGRAM = (
    ".run(",
    "run_source",
    "run_isolated",
    "run_chunk",
    "tree_walk",
    "bytecode(",
)

# The tokens that mean "this runs a Redblue program and answers with it".
ANSWERS_THE_PROGRAM = RUNS_A_PROGRAM

# The tokens that read what the program said, recorded or answered.
READS_THE_PROGRAM = re.compile(
    r"take_output|\.output|stdout|take_expectation_failure|expect_err"
)

# A match on the program's own answer — `Ok(Value::YesNo(true))`.
MATCHES_THE_ANSWER = re.compile(r"Ok\(Value::")

# A Redblue-level `expect`, which lives inside a fixture string.
REDBLUE_ASSERTION = re.compile(r"\bexpect\b[^\n]*\bto be\b|\bexpect\b[^\n]*\bto fail\b")

# The words a `let` pattern may carry that are not names.
NOT_NAMES = frozenset(
    {
        "as",
        "const",
        "crate",
        "dyn",
        "false",
        "fn",
        "in",
        "let",
        "move",
        "mut",
        "ref",
        "self",
        "static",
        "super",
        "true",
        "use",
    }
)


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


def called_paths(body: str) -> set[str]:
    """The qualified names a body calls, such as `corpus::compare`."""
    return set(re.findall(r"\b((?:[A-Za-z_][A-Za-z0-9_]*::)+[A-Za-z_][A-Za-z0-9_]*)\s*\(", body))


def names_in(text: str) -> set[str]:
    """The identifiers a piece of text mentions."""
    return set(re.findall(r"[A-Za-z_][A-Za-z0-9_]*", text))


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


def statement_around(code: str, position: int) -> str:
    """The statement the character at `position` is in."""
    starts = [code.rfind(ch, 0, position) for ch in ";{}"]
    start = max(starts) + 1
    ends = [code.find(ch, position) for ch in ";}"]
    ends = [end for end in ends if end != -1]
    end = min(ends) + 1 if ends else len(code)
    return code[start:end]


def statement_end(code: str, position: int) -> int:
    """The end of the statement that starts at `position`."""
    depth = 0
    for i in range(position, len(code)):
        ch = code[i]
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            if depth == 0:
                return i
            depth -= 1
        elif ch == ";" and depth == 0:
            return i
    return len(code)


class Helpers:
    """The helper functions a scan can follow.

    `local` holds the file being scanned; `modules` holds the `tests/common/`
    modules, by module name. A call is resolved by its bare name — the file's
    own helper first, then the modules — or by a qualified name such as
    `corpus::compare`.
    """

    def __init__(self, local: dict[str, tuple[str, str]], modules: dict[str, dict[str, tuple[str, str]]]):
        self.local = local
        self.modules = modules

    def entry_of(self, path: str) -> tuple[str, str]:
        """The body and the fixture text of the function a call names."""
        if "::" in path:
            module, _, bare = path.rpartition("::")
            return self.modules.get(module, {}).get(bare, ("", ""))
        if path in self.local:
            return self.local[path]
        for functions in self.modules.values():
            if path in functions:
                return functions[path]
        return ("", "")

    def body_of(self, path: str) -> str:
        """The body of the function a call names, or the empty string."""
        return self.entry_of(path)[0]

    def fixture_of(self, path: str) -> str:
        """The fixture text of the function a call names."""
        return self.entry_of(path)[1]

    def closure(self, path: str, seen: set[str] | None = None) -> str:
        """The body of a helper and of every helper it calls."""
        if seen is None:
            seen = set()
        if path in seen:
            return ""
        seen.add(path)
        body = self.body_of(path)
        for callee in called_names(body) | called_paths(body):
            body += self.closure(callee, seen)
        return body

    def answers(self, path: str) -> bool:
        """Whether a helper reads or runs the program it is given."""
        text = self.closure(path)
        if READS_THE_PROGRAM.search(text) or MATCHES_THE_ANSWER.search(text):
            return True
        return any(token in text for token in ANSWERS_THE_PROGRAM)


def load_modules(root: Path) -> dict[str, dict[str, tuple[str, str]]]:
    """The `tests/common/` modules: module name -> {function name: (body, fixture)}."""
    common = root / "common"
    mod_file = common / "mod.rs"
    if not mod_file.is_file():
        return {}
    text = mod_file.read_text(encoding="utf-8", errors="replace")
    modules: dict[str, dict[str, tuple[str, str]]] = {}
    for match in re.finditer(r"\b(?:pub\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;", text):
        name = match.group(1)
        path = common / f"{name}.rs"
        if not path.is_file():
            continue
        modules[name] = function_bodies(path)
    return modules


def function_bodies(path: Path) -> dict[str, tuple[str, str]]:
    """Every top-level function of one file: name -> (body, fixture).

    The body has the string literals masked out, and the fixture keeps them: a
    Redblue-level `expect ... to be` lives inside a fixture, and it is the
    program checking its own answer.
    """
    raw = path.read_text(encoding="utf-8", errors="replace")
    code = mask(mask(raw, mask_strings=True), mask_strings=False)
    words = mask(raw, mask_strings=False)
    return {
        name: (code[brace:end], words[brace:end])
        for name, _line, brace, end, _before in find_functions(code)
    }


def bindings(code: str) -> list[tuple[set[str], str]]:
    """The names a body binds, and the expression each is bound to.

    Four shapes are recognised: a `let`, a `for` loop's pattern, a `match`
    arm whose pattern binds the value the match is looking at, and a plain
    assignment. The second and third matter because an answer is often read
    out of a loop or out of a `match` rather than out of a `let`.
    """
    found: list[tuple[set[str], str]] = []
    for match in re.finditer(
        r"\blet\s+(\([^()]*\)|[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*(?:\([^()]*\))?)",
        code,
    ):
        equals = code.find("=", match.end())
        if equals == -1:
            continue
        right = code[equals + 1 : statement_end(code, equals + 1)]
        names = {name for name in names_in(match.group(1)) if name not in NOT_NAMES}
        found.append((names, right))
    for match in re.finditer(
        r"\bfor\s+(\([^()]*\)|[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*)"
        r"\s+in\b",
        code,
    ):
        start = match.end()
        right = code[start : statement_end(code, start)]
        names = {name for name in names_in(match.group(1)) if name not in NOT_NAMES}
        found.append((names, right))
    for match in re.finditer(r"\bmatch\b", code):
        brace = code.find("{", match.end())
        if brace == -1:
            continue
        end = match_brace(code, brace)
        scrutinee = code[match.end() : brace]
        for arm in re.finditer(r"([^{}]*?)=>", code[brace:end]):
            names = {name for name in names_in(arm.group(1)) if name not in NOT_NAMES}
            found.append((names, scrutinee))
    for match in re.finditer(r"(?<![\w.])([A-Za-z_][A-Za-z0-9_]*)\s*=[^=]", code):
        start = match.end()
        right = code[start : statement_end(code, start)]
        found.append(({match.group(1)}, right))
    return found


def derived_names(code: str, helpers: Helpers) -> set[str]:
    """The names a body binds to the program's own answer.

    A name is derived when it is bound to the output, to the error, to the
    value the program ended on, or to the result of running one — directly, or
    through a helper that does any of those, or through another derived name.
    The last part is a fixpoint: `let n = out.len();` derives `n` because
    `out` is already derived.

    Three more shapes carry the answer forward:

    * the object a program is run **through** — `harness` holds the outcome of
      the program it ran, and `vm` holds the lines it printed;
    * a collection the answer is **put into** — `seen.insert(item)` makes
      `seen` a name the answer is read out of;
    * a name the answer is **written to** — `sorted = lines.sort()`.
    """
    pairs = bindings(code)
    names: set[str] = set()
    for match in re.finditer(
        r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\.\s*(run|run_source|run_isolated|run_chunk)\s*\(", code
    ):
        names.add(match.group(1))
    changed = True
    while changed:
        changed = False
        for bound, source in pairs:
            if not bound or bound <= names:
                continue
            if is_the_answer(source, helpers, names):
                names |= bound
                changed = True
        for match in re.finditer(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\.\s*[A-Za-z_][A-Za-z0-9_]*\s*\(", code):
            receiver = match.group(1)
            if receiver in names:
                continue
            argument = code[match.end() : match_paren(code, match.end() - 1)]
            if is_the_answer(argument, helpers, names):
                names.add(receiver)
                changed = True
    return names


def is_the_answer(text: str, helpers: Helpers, derived: set[str]) -> bool:
    """Whether `text` is the program's answer, or a value read out of it."""
    if READS_THE_PROGRAM.search(text) or MATCHES_THE_ANSWER.search(text):
        return True
    if any(token in text for token in ANSWERS_THE_PROGRAM):
        return True
    if derived & names_in(text):
        return True
    for callee in called_names(text) | called_paths(text):
        if helpers.answers(callee):
            return True
    return False


def comparison_contexts(code: str) -> list[str]:
    """The text of every comparison in `code`.

    A comparison is a place where the program's answer is held against
    something else: an `assert_eq!`, `assert_ne!` or `assert_matches!`; an
    `assert!` whose condition says more than `is_ok()`; an `==` or `!=`; or a
    `match` on the program's own answer against a literal.
    """
    contexts: list[str] = []
    for match in re.finditer(r"\b(assert_eq|assert_ne|assert_matches|assert)\s*!", code):
        open_paren = code.find("(", match.end())
        if open_paren == -1:
            continue
        end = match_paren(code, open_paren)
        if match.group(1) == "assert":
            argument = code[open_paren + 1 : end]
            if is_reachability_check(argument):
                continue
        contexts.append(code[open_paren + 1 : end])
    for match in re.finditer(r"[^=!<>]==[^=]|[^=!<>]!=[^=]", code):
        contexts.append(statement_around(code, match.start()))
    for match in re.finditer(r"Ok\(Value::[A-Za-z_][A-Za-z0-9_]*(?:\(([^()]*)\))?\)", code):
        inner = match.group(1)
        if inner is not None:
            literal = re.sub(r"\b(true|false)\b", "", inner)
            if names_in(literal):
                # It binds the answer rather than comparing it.
                continue
        contexts.append(match.group(0))
    # A call whose name begins with `assert` is an assertion by name: the
    # REPL harness's `assert_stdout_contains(..)` compares what the session
    # printed, and the name is the only place that says so.
    for match in re.finditer(r"\bassert_[A-Za-z0-9_]*\s*\(", code):
        open_paren = code.find("(", match.start())
        end = match_paren(code, open_paren)
        contexts.append(code[match.start() : end])
    return contexts


def mentions_the_answer(text: str, derived: set[str], helpers: Helpers) -> bool:
    """Whether `text` holds the program's answer against something else."""
    if READS_THE_PROGRAM.search(text) or MATCHES_THE_ANSWER.search(text):
        return True
    if derived & names_in(text):
        return True
    for callee in called_names(text) | called_paths(text):
        if helpers.answers(callee):
            return True
    return False


def asserts_something(bodies: list[str], helpers: Helpers) -> bool:
    """Whether any of these bodies compares what the program answered.

    Each body is judged **on its own**. A name bound in one function says
    nothing about a comparison in another, and a concatenation of two
    functions' text would let the first function's names satisfy the second
    function's comparison — which is a false "not vacuous" of exactly the kind
    this scan exists to remove.
    """
    for body in bodies:
        derived = derived_names(body, helpers)
        for context in comparison_contexts(body):
            if mentions_the_answer(context, derived, helpers):
                return True
    return False


def scan_file(path: Path, modules: dict[str, dict[str, tuple[str, str]]]) -> list[tuple[str, int]]:
    """The vacuous tests in one file: (name, line)."""
    raw = path.read_text(encoding="utf-8", errors="replace")
    code = mask(mask(raw, mask_strings=True), mask_strings=False)
    words = mask(raw, mask_strings=False)

    functions = find_functions(code)
    local = {
        name: (code[brace:end], words[brace:end])
        for name, _line, brace, end, _before in functions
    }
    helpers = Helpers(local, modules)

    def reached(name: str, seen: set[str]) -> list[str]:
        """The function a call names, and every helper it calls."""
        if name in seen:
            return []
        seen.add(name)
        names = [name]
        body = helpers.body_of(name)
        for callee in called_names(body) | called_paths(body):
            names += reached(callee, seen)
        return names

    vacuous = []
    for name, line, brace, end, before in functions:
        attributes = before.rsplit("}", 1)[-1]
        if "#[test]" not in attributes:
            continue
        names = reached(name, set())
        bodies = [helpers.body_of(n) for n in names]
        if not any(any(token in body for token in RUNS_A_PROGRAM) for body in bodies):
            continue
        if asserts_something(bodies, helpers):
            continue
        fixtures = "".join(helpers.fixture_of(n) for n in names)
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
    modules = load_modules(root)
    print(f"scanned {len(files)} files in {root}")
    total = 0
    for path in files:
        for name, line in scan_file(path, modules):
            print(f"{path.name}:{line}\t{name}")
            total += 1
    print(f"vacuous tests: {total}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
