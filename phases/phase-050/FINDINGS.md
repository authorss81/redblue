# phase-050 — findings

Out-of-scope observations recorded here so the auditor can promote them to
phases rather than have them smuggled into phase-050.

## 1. The finding's own headline is stale; the reconciliation is not

The phase prompt's evidence says `text` and `math` were absent from
`stdlib::MODULES` and that `text.uppercase("hi")` answered `Unknown variable
'text'`. On `main` at `5a0e54a` they are present (`src/stdlib.rs:19-21`, with
the reason written down at `src/stdlib.rs:8-18` by an earlier phase) and the
call answers `HI`. The `formats` correction had already landed too
(`SPEC.md:1104` is `### json and csv` and `SPEC.md:1112` says there is no
`formats` module).

What was still true, and is what this phase fixed:

- `README.md` headed its JSON/CSV block `### Formats` — the only remaining
  place a document named a module that does not exist.
- `bytes` and `sys` were in `stdlib::MODULES` and named in neither document:
  undocumented surface that works, which a reader has no way to learn exists.

## 2. Builtins silently ignore extra arguments

`say text.uppercase("a", "b")` answers `A`; `say json.parse("{}", "x")` answers
`{}`; `say math.sqrt(1, 2)` answers `1`; `say sys.argv(1)` answers `[]`. Only a
builtin that checks the arguments it needs refuses — `list.map` with a third
argument, `bytes.write` with one — so a typo in a call site is dropped rather
than reported. The language's documented variadic builtins (`random`, `random_number`,
`random_number(10)`, `random()`) would have to keep their range, which is why
this is not a one-line change in a documentation-reconciliation phase.

`tests/stdlib_module_names_test.rs` pins `list.map` as the "too many
arguments" case rather than a flat-backed builtin, because a flat-backed one
does not fail today. Nothing in that file asserts the tolerance is *correct* —
it is simply not asserted either way.

## 3. `SPEC.md` § console named `ask`, and this phase corrected the section

Round 1 recorded this as "`SPEC.md:997-1003` writes three lines for `### console`
… `say` works. `print "x"` is `ParserError: Unexpected token 'print'`, and `ask
"n"` is `AnalyzerError: Unknown variable 'ask'`". Half of that was wrong and the
wrong half was repeated in round 1's review:

- `print` **is** a name. `lexer.rs:49` lexes it, `parser.rs:578` gives it a
  statement, and `print "Hello"` writes `Hello` with no newline — through the
  tree-walker and, checked here, through `rb compile` + `rb vm` as well. The
  `ParserError` claim does not reproduce.
- `ask` is not a name, and never was one the parser could read: it is a freed
  word (phase-049), so `ask "n"` is `AnalyzerError: Unknown variable 'ask'; did
  you mean 'as'?`.
- `console.log`, `console.error` and `console.clear` are in `stdlib::MODULES`
  (`src/runtime.rs:1067-1090`) and were documented nowhere — the same defect this
  phase fixed for `bytes` and `sys`, and the reason the round-1 coverage was
  exercising a spelling no reader could look up.

Round 2 fixed the section rather than recording it, because the phase's own test
was reading it: `tests/stdlib_module_names_test.rs` now runs **every line of
`### console`'s code block**, read out of SPEC.md, so a documented line the
language refuses fails the test. Reinstating `ask "Your name?"` was tried and
fails it:

```
a line SPEC.md § console writes does not run:
ask "Your name?"
Error: AnalyzerError: Unknown variable 'ask'; did you mean 'as'?
```

`ask` stays unimplemented; that is a missing feature, not a document defect, and
`tests/stdlib_module_names_test.rs` pins the refusal so the section cannot quietly
promise it again.

## 4. `SPEC.md` had no `§ time`, and round 1's own claim about `time.unix` was stale

Round 1 recorded that "`README.md` documents `time` and `SPEC.md` does not", and
that "`README.md` documents `time.unix` which is registered and unimplemented"
because `tests/stdlib_module_docs_test.rs` listed `time_unix` in `KNOWN_GAPS`.
The first half was true; the second was not. `time_unix` is implemented at
`src/runtime.rs:981` and exercised by
`tests/time_builtin_test.rs:284`, which pins the years it refuses. It was in
`KNOWN_GAPS` because that list is a hand-written record of names nobody checked
again.

Round 2:

- `SPEC.md` has a `### time` section, written against what answers —
  `time.now()` is a record of `seconds` and `nanoseconds`, `time.sleep` blocks,
  `time.format(1704067200, "%Y-%m-%d")` is `"2024-01-01"`, and
  `time.unix("2024-01-01 00:00:00")` is `1704067200` read as UTC (checked with
  `TZ=America/New_York`, which answers the same).
- `time_unix` and `console_clear` are out of `KNOWN_GAPS`. Both answer; a list
  that keeps the names of implemented builtins is a list nobody can trust.
- Round 1's argument for reconciling the **union** of the two documents — that
  "requiring each document to name all eleven would fail on a document that is a
  quick reference rather than a specification" — is withdrawn. It was the reason
  SPEC.md could drop `time` and README.md could drop `console` with the suite
  green, which is the drift this phase exists to stop. Each document is now
  reconciled against `MODULES` on its own, in both directions; README.md gained
  `### Console` and `### List` to meet it.

Round 3 found that removing the two stale entries was not enough, because the
list is a *skip* list: `edge_no_registered_builtin_is_unreachable_from_a_program`
walks the registry and skips whatever `KNOWN_GAPS` names, so an entry is a
permission to stop checking that never expires. Eleven more had gone stale
underneath the two that had been noticed — `pop`, `shift`, `is_number`,
`is_text`, `is_list`, `is_record`, `to_list`, `csv_parse`, `network_get`,
`network_post` and `list_map`, all implemented in `src/stdlib.rs`, four of them
asserted by name in the *same file* by
`edge_every_flat_builtin_the_stdlib_registers_answers`. Thirteen of the
fourteen entries were false.

All eleven are out and the list is two entries. The change that matters is not
the removal but the shape: `known_gaps()` now returns each name **with the
program that still fails**, and `edge_a_known_gap_is_still_a_gap` runs them, so a
name that starts answering fails on its own instead of sitting in a list that
reads as current. The general lesson for any other skip list in this tree — a
name in an exemption list is untested unless the exemption itself is tested.

## 5. A guarded call plus an optional expectation is a test that cannot fail

The round-3 BLOCKER is worth recording because the shape, not the mistake, is
what hid it. The `network` row was

```
set answer to might fail network.get("http://127.0.0.1:1/")
say "ran"
```

with an expected output of `None`, and the runner branched on
`if let Some(expected)`. Two separate escapes from the same assertion: `might
fail` discards the `RuntimeError`, and `None` discards the comparison. With
`network_get` deliberately broken to refuse every request, that program still
prints `ran` and still exits 0 — it was run, and the answer was 0.

The fix is a loopback `TcpListener` in the test, so the answer is a constant the
test owns rather than a thing it must not check. Two points generalize:

- A test that reaches the network does not have to reach the *internet*. A
  listener on `127.0.0.1:0` answers `network.get` with a fixed body in
  milliseconds and leaves the machine untouched.
- An exemption from an assertion (`None`, an `if let`, a `might fail`, a `skip`
  list) is an assertion in its own right and needs its own test. That is the
  same finding as §4's, in a different file.

## 6. `ROADMAP.md` still writes `formats.parse_json`

`ROADMAP.md` § Async HTTP Request builds an example on `formats.parse_json`,
a module that has never existed. `SPEC.md:1112` says so since this correction.
Already recorded by phase-049's finding 4; unchanged here because ROADMAP is
not one of the three documents this phase reconciles.

## 7. `verify.sh` was not run

It lives at `../rbops/verify.sh`, outside this project checkout, and the run
instructions for this phase state that the pipeline that invoked it must not be
inspected and that no tool call is to be spent on it. It was therefore not
invoked. The gates that can be run from inside the checkout were, and their
real output is in `REPORT.md`: `cargo fmt --all -- --check`, `cargo clippy
--all-targets -- -D warnings`, `cargo test --all-targets` (1422 passed, 0
failed), and `rb run` over every file in `examples/` and `modules/` (9 files, 0
failures). `REPORT.md` claims nothing about `verify.sh`.
