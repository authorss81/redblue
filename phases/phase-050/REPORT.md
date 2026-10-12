# Phase 050 — Reconcile SPEC.md's stdlib module names with src/stdlib.rs MODULES

## Reproduction

`target/tmp/p50/repro.rb`:

```
say text.uppercase("hi")
say math.sqrt(9)
say json.parse("{}")
say csv.parse("a,b")
say list.map([1,2,3], to (x) give back x*2 end)
```

`cargo run --bin rb -- run target/tmp/p50/repro.rb` → exit 0, printing
`HI`, `3`, `{}`, `[[a, b]]`, `[2, 4, 6]`.

**The phase's headline finding no longer reproduces.** `text` and `math` are in
`stdlib::MODULES` (`src/stdlib.rs:19-21`), `### json and csv` is what
`SPEC.md:1089` says, and `SPEC.md:1097` states there is no `formats` module.
All of that was already fixed on `main` before this phase (HEAD `5a0e54a`); the
phase prompt's `SPEC.md:942`/`950`/`959`/`994` line numbers do not match the
file either.

What *did* reproduce, and was still true on `main`:

- `README.md` headed its JSON/CSV block `### Formats` — the one remaining place
  a document named a module that does not exist and is refused by the analyzer.
- `bytes` and `sys` were in `stdlib::MODULES` and named in neither document:
  undocumented surface that works. `say bytes.from_text("hi")` answers
  `[104, 105]` and `say sys.argv()` answers `[]`; nothing tells a reader either
  exists.
- Nothing in the tree compared the three lists, so either drift would have
  stayed green.

## What changed

| File | Lines | What |
|---|---|---|
| `src/stdlib.rs` | +29 −0 | `ALIAS_MODULES` (`math`, `text` — the modules whose functions are the flat builtins) and `modules_without_functions()`, which reports any name in `MODULES` with no `<module>_<function>` builtin behind it. That is a namespace the analyzer accepts as a receiver and the runtime then refuses, which is worse than either error alone. |
| `SPEC.md` | +69 −7 | `### bytes` (`SPEC.md:1115`) and `### sys` (`SPEC.md:1129`) sections, written against `src/runtime.rs:1198-1303` — including that a byte is 0..=255, that `[0, 255]` is in range but is not text, and that `sys.argv()` is empty outside `rb run`. Round 2 rewrote § console (`SPEC.md:997`) and added `### time` (`SPEC.md:1141`). |
| `README.md` | +29 −1 | `### Formats` → `### JSON and CSV` (the stale name), plus `### Bytes` (`README.md:190`) and `### Sys` (`README.md:205`). Round 2 added `### Console` (`README.md:239`) and `### List` (`README.md:248`). |
| `tests/stdlib_module_names_test.rs` | +746 | New: the extraction test, one program per documented module, and the edge tests. Round 3 made the module table's expected output mandatory and gave `network` a loopback server. |
| `tests/stdlib_module_docs_test.rs` | +5 −2 | `time_unix` and `console_clear` out of `KNOWN_GAPS`: both answer, so their entries were permissions to stop checking names that work. Round 3 took eleven more out and made the list self-checking. |
| `nope` | −1 | A one-byte (`0x01`) file committed at the repository root by an earlier round of this phase: a `bytes.write` program that named a path relative to the working directory, which is why round 3 gave every such program an absolute path. Deleted, and a full `cargo test --all-targets` now leaves `git status` clean. |
| `tests/stdlib_module_names_test.rs` | +74 | Round 4: `edge_a_bad_module_call_is_a_catchable_error_and_not_an_abort` — the bad-argument cases again, inside the program's own `try ... catch error`. See round 4 below. |

Round 2, round 3 and round 4 moved every one of the round-1 `SPEC.md`/`README.md`
line numbers; those below are the current ones.

No interpreter behaviour changed. `MODULES` itself is unchanged — every name in
it answers and every name the documents name was already in it. `src/runtime.rs`
is untouched by round 3: the `network` defect the review found was in a test, and
was shown to be there by breaking `runtime.rs` on purpose and watching the old
row pass and the new one fail.

### Round 2 — what the first review found, and what it changed

| # | Severity | What was wrong | What changed |
|---|---|---|---|
| 1 | BLOCKER | `tests/stdlib_module_names_test.rs:275` (round 1's numbering) ran `files.exists("{scratch}/console.rb")` — a path the setup at `:266` never writes (it writes `files.rb`) — and asserted only that the program exited 0. `files.exists` answers `yes` or `no` (`src/runtime.rs:865`) and *both* are a successful run, so the one program that could have failed could not. | The program asks about a file this test writes (`exists_probe.rb`) **and** about one nothing writes, and the table now carries the output each program must print, compared exactly. Verified by pointing the first line at a path nothing writes: `left: "no\nno\n" right: "yes\nno\n"`. |
| 2 | MAJOR | The `console` program was `console.log("hi")`, which neither document named, while SPEC.md § console wrote `say`, `print` and `ask` — two of which the review, repeating FINDINGS §3, believed were refused. `print` is a name and works; `ask` is not one. | § console now documents what answers (`say`, `print`, `console.log`, `console.error`, `console.clear`) and says why there is no `ask`. The test **runs every line of that section's code block**, read out of SPEC.md. Reinstating `ask "Your name?"` fails it. |
| 3 | MAJOR | The reconciliation compared the **union** of the two documents against `MODULES`, so SPEC.md omitting `time` and README.md omitting `console`/`list` stayed green. | Each document is reconciled against `MODULES` on its own, in both directions. SPEC.md gained `§ time`; README.md gained `### Console` and `### List`. Deleting SPEC.md's `§ time` now fails with *"stdlib::MODULES lists `time` and SPEC.md does not name it."* |
| 4 | MINOR | FINDINGS §4 and the known-gaps list said `time.unix` was registered and unimplemented. It is implemented (`src/runtime.rs:981`) and tested (`tests/time_builtin_test.rs:284`). | The claim is corrected in FINDINGS.md, `time_unix` and `console_clear` are out of `KNOWN_GAPS` in `tests/stdlib_module_docs_test.rs`, and SPEC.md's `§ time` is written against what answers. |

Nothing was weakened to get there: no test was deleted, nothing was ignored, no
lint was allowed. Three of the four fixes make a previously unfailable test fail
on the defect it names, which is shown above by mutation rather than asserted.

### Round 3 — the second review, and what it changed

The second review read the round-2 diff and found four things. All four were
fixed, and each fix was shown to fail on the defect it names before it was kept.

| # | Severity | What was wrong | What changed |
|---|---|---|---|
| 1 | BLOCKER | The `network` row was `set answer to might fail network.get("http://127.0.0.1:1/")` with expected output `None`. `might fail` discards any `RuntimeError` and `if let Some(expected)` then skipped the comparison, so a `network.get` that refused *every* request exited 0 and the row passed. The one module whose answer is not a function of its arguments was the one module whose answer nothing checked. | The test now owns the network: a `TcpListener` on `127.0.0.1:0` answers a fixed body to two requests and records them. The row calls **both** spellings unguarded — `network.get("{addr}/greeting")` and `network.post("{addr}/echo", "ping")` — and carries the expected output. The table's type is `Vec<(&str, String, String)>`: there is no `Option` and no "no expected output" case left, and each row asserts it is non-empty. The recorded request lines are checked too (`GET /greeting`, `POST /echo`, body `ping`), so a `network.get` that issued a POST fails even though the body it got back is right. |
| 2 | MAJOR | `bytes.write("target/tmp/p50names/ends.bin", …)` was a **cwd-relative** path inside a program, while the read-back at the next line used an absolute `CARGO_MANIFEST_DIR` path — the same file's own rule three hundred lines up, at :369-379, says a test's working directory is not a promise. `rb run` resolves a relative path against the *process's* directory, so the two names coincided only because `cargo test` happens to start in the manifest directory. | Both `bytes.write` programs are given the absolute `scratch` path interpolated into the source, the same way the `files` row is. The whole file now passes when its test binary is run from an unrelated directory. |
| 3 | MAJOR | `KNOWN_GAPS` still listed `pop`, `shift`, `is_number`, `is_text`, `is_list`, `is_record`, `to_list`, `csv_parse`, `network_get` and `network_post` as unreachable, and `list_map` besides — eleven names `src/stdlib.rs` implements, four of which `edge_every_flat_builtin_the_stdlib_registers_answers` was asserting by name *in the same file*. Each was a name the surface test had stopped checking. | All eleven are out. The list is two entries (`filter`, `reduce`), and it is no longer a claim: `known_gaps()` returns each name **with the program that still fails**, and the new `edge_a_known_gap_is_still_a_gap` runs them, so a name that starts answering fails instead of sitting in a list that reads as current. `edge_no_registered_builtin_is_unreachable_from_a_program` still walks every registered builtin and now also asserts each gap is a real `stdlib::is_builtin` name. |
| 4 | MINOR | SPEC.md:1158 wrote `Both calls block.` after describing `time.format` and `time.unix`. Both are pure computation (`src/runtime.rs:961` and `:981`); only `time.sleep` waits (`sleep_for` at `src/runtime.rs:495`). | § time now names `time.sleep` as the one call in the section that blocks, says the other three return immediately whatever the clock says, and names `network.get`/`network.post` as the other blocking calls. |

#### Round 3's failing-test-first sequence

Each defect was reintroduced into the tree, the test run, and the change undone.

| Reintroduced | Test | Real output |
|---|---|---|
| `network_get` made to refuse every request (`src/runtime.rs`) | `every_module_the_documents_name_answers_a_call` | `the documented module 'network' does not answer a call:` / `RuntimeError: HTTP request failed: broken` |
| `network_get` changed from `.get(url)` to `.post(url)` | `every_module_the_documents_name_answers_a_call` | `network.get did not issue a GET for the path it was given:` / `left: ["POST", "/greeting"] right: ["GET", "/greeting"]` |
| `bytes.write` pointed back at `target/tmp/p50names/ends.bin`, with the test binary run from `/tmp/opencode/elsewhere` | `edge_the_bytes_module_answers_a_round_trip_at_both_ends` | `panicked at tests/stdlib_module_names_test.rs:871` — the file was never written where the read-back looked |
| `("pop", "say pop([1, 2, 3])")` put back into `known_gaps()` | `edge_a_known_gap_is_still_a_gap` | ``'pop' is listed as unreachable in KNOWN_GAPS and answered:`` |

The first two are the BLOCKER proven: the same mutation that fails the new test
passes the old row, run directly —

```
$ rb run target/tmp/mut/old.rb     # set answer to might fail network.get(...) / say "ran"
ran
old-form exit=0                     # with network_get refusing every request
```

### The extracted lists (pasted as the definition of done requires)

Before, from the `## Standard Library` sections only:

| | names |
|---|---|
| SPEC.md | `console, csv, files, json, list, math, network, text` |
| README.md | `csv, files, formats, json, math, network, text, time` |
| `stdlib::MODULES` | `bytes, console, csv, files, json, list, math, network, sys, text, time` |
| documented but not in `MODULES` | `formats` |
| in `MODULES` but documented nowhere | `bytes`, `sys` |

After round 2 — each document on its own:

| | names |
|---|---|
| SPEC.md | `bytes, console, csv, files, json, list, math, network, sys, text, time` |
| README.md | `bytes, console, csv, files, json, list, math, network, sys, text, time` |
| `stdlib::MODULES` | `bytes, console, csv, files, json, list, math, network, sys, text, time` |
| documented but not in `MODULES` | none |
| in `MODULES` but undocumented in either document | none |

The test asserts each document against `MODULES`, in both directions — it does
not take the union of the two. Round 1 took the union and justified it as "a
document that is a quick reference rather than a specification"; that
justification is what let SPEC.md drop `time` and README.md drop `console` and
`list` with the suite green, which is the drift this phase exists to stop, and it
is withdrawn (FINDINGS.md §4).

## Tests added

All in `tests/stdlib_module_names_test.rs` — 14 `#[test]` functions, 12 named
`edge_*`. (The previous round's table listed 9; the five it omitted are rows 4,
5, 8, 10 and the round-4 row 13, and are restored here.)

| # | Test | Edge class covered |
|---|---|---|
| 1 | `edge_a_module_the_documents_name_is_in_the_registry` | forward reconciliation, **per document** — a documented name absent from `MODULES` |
| 2 | `edge_a_module_in_the_registry_is_named_by_a_document` | reverse reconciliation, per document — a `MODULES` name one of the two documents does not name |
| 3 | `edge_no_module_in_the_registry_has_nothing_behind_it` | a namespace with no function behind it |
| 4 | `edge_the_member_table_covers_exactly_the_modules_that_are_registered` | the table row 3 walks is itself the surface — a module missing from it would be skipped, not checked |
| 5 | `edge_a_member_the_documents_name_is_a_function_the_registry_has` | a member a document spells that resolves to no builtin — accepted by the analyzer, refused at run time |
| 6 | `every_module_the_documents_name_answers_a_call` | one `.rb` per documented name through `rb run`, exit 0 **and the output it must print**, with no row allowed to opt out; `network` is answered by a loopback server this test owns and its request lines are asserted; the table is itself checked against each document in both directions |
| 7 | `edge_the_documented_console_lines_answer` | every line of SPEC.md § console's code block, read out of the document and run; plus the newline difference between `say` and `print`, `console.log` answering `nothing`, `console.error` on stderr, and `ask` refused |
| 8 | `edge_input_reads_one_line_and_answers_it` | resource/state — `input` with a line typed, with no newline at end of input, with `\r\n`, and with EOF |
| 9 | `the_text_and_math_examples_answer_what_the_spec_claims` | the `text`/`math` values SPEC.md's own comments claim, incl. `by ","` spellings |
| 10 | `edge_length_counts_bytes_in_both_arms` | the two implementations of `length` must agree; a `é` is 2 bytes and not 1 character |
| 11 | `edge_the_bytes_module_answers_a_round_trip_at_both_ends` | empty, singleton, unicode, byte-range boundaries, out-of-range, non-UTF-8 |
| 12 | `edge_a_module_function_refuses_bad_arguments_by_name` | no arguments, too many, wrong type — each a `RuntimeError` naming the function, each asserted not to have panicked |
| 13 | `edge_a_bad_module_call_is_a_catchable_error_and_not_an_abort` | the same three refusals **inside the program's own `try … catch error`** — stdout must be exactly `caught\nafter\n`, so a refusal that ended the process fails it; plus the boundary, that an `AnalyzerError` is *not* catchable |
| 14 | `edge_an_unknown_module_name_is_refused_in_its_dotted_spelling` | `nosuchmodule.read("x")` is refused, and the message says `nosuchmodule`, not the internal `nosuchmodule_read` |

The failing-test-first sequence, with the real output:

1. `cargo test --test stdlib_module_names_test` → `edge_a_module_the_documents_name_is_in_the_registry ... FAILED`:
   *"SPEC.md or README.md names the module `formats` and stdlib::MODULES does not, so a program using it dies as 'Unknown variable 'formats''."*
   and `edge_a_module_in_the_registry_is_named_by_a_document ... FAILED`:
   *"stdlib::MODULES lists `bytes` and neither SPEC.md nor README.md names it."*
2. README.md and SPEC.md corrected.
3. Same command → `8 passed; 0 failed`.

### Round 2's failing-test-first sequence

Each defect was reintroduced into the tree, the test run, and the change undone:

| Reintroduced | Test | Real output |
|---|---|---|
| `files.exists` pointed at a file nothing writes | `every_module_the_documents_name_answers_a_call` | `left: "no\nno\n" right: "yes\nno\n"` |
| `ask "Your name?"  // get user input` put back into SPEC.md § console | `edge_the_documented_console_lines_answer` | `a line SPEC.md § console writes does not run:` / `AnalyzerError: Unknown variable 'ask'; did you mean 'as'?` |
| SPEC.md's `### time` section deleted | `edge_a_module_in_the_registry_is_named_by_a_document` | `stdlib::MODULES lists `time` and SPEC.md does not name it.` |
| SPEC.md's `### time` renamed `### timers` | `edge_a_module_the_documents_name_is_in_the_registry` | FAILED — a heading that is not a module name is refused by the forward gate |

### Round 3's failing-test-first sequence

Each defect was reintroduced into the tree, the test run, and the change undone.
This is the full list; nothing below is asserted, it is pasted.

| Reintroduced | Test | Real output |
|---|---|---|
| `network_get` in `src/runtime.rs` made to refuse every request | `every_module_the_documents_name_answers_a_call` | `the documented module 'network' does not answer a call:` / `RuntimeError: HTTP request failed: broken` |
| `network_get` changed from `.get(url)` to `.post(url)` | `every_module_the_documents_name_answers_a_call` | `network.get did not issue a GET for the path it was given:` / `left: ["POST", "/greeting"] right: ["GET", "/greeting"]` |
| `bytes.write` pointed back at the cwd-relative `target/tmp/p50names/ends.bin`, with the test binary run from `/tmp/opencode/elsewhere` | `edge_the_bytes_module_answers_a_round_trip_at_both_ends` | `panicked at tests/stdlib_module_names_test.rs:871` — the file was never written where the read-back looked |
| `("pop", "say pop([1, 2, 3])")` put back into `known_gaps()` | `edge_a_known_gap_is_still_a_gap` | `` `pop` is listed as unreachable in KNOWN_GAPS and answered: `` |

The first row is the BLOCKER, proven both ways. The mutation that fails the new
test passes the old row, run directly against the same broken `runtime.rs`:

```
$ ./target/debug/rb run target/tmp/mut/old.rb
ran
old-form exit=0
```

and the last row shows the fix for finding 3 is self-enforcing: putting an
implemented name back on the gap list fails on its own.

### Round 4 — this run

The four gates were run first, against the tree the previous rounds left. `cargo
fmt --check`, `cargo clippy --all-targets -D warnings` and `cargo test
--all-targets` were already green. Two things were not finished:

| # | Severity | What was wrong | What changed |
|---|---|---|---|
| 1 | MAJOR | The phase's definition of done asks for "each a **caught** `RuntimeError` naming the function, never a panic". `edge_a_module_function_refuses_bad_arguments_by_name` proved the *refusal* — non-zero exit, the function named in stderr, no `panicked` — which is the process-level half. Nothing proved the other half: that the refusal is something a program can catch and carry on from. A builtin that unwound instead of raising would have passed it. | `edge_a_bad_module_call_is_a_catchable_error_and_not_an_abort` runs three of the same bad calls — `text.uppercase()` (no arguments), `list.map(…, 5)` (too many), `files.read(1)` (wrong type) — inside `try … catch error`, and asserts stdout is exactly `caught\nafter\n`. `after` is on stdout only if the statement past the `try` ran, so a fatal refusal fails the assertion even though the program printed something. |
| 2 | MAJOR | The same test's sibling — and this is what makes #1 mean something — the unknown-module case is a different kind of failure. `nosuchmodule.read("x")` is an `AnalyzerError`, raised before the program starts, so `catch error` does not catch it. Lumping the two together would have claimed "module errors are catchable" on the strength of a row that is not a runtime error at all. | The new test asserts the boundary directly: the same program with `nosuchmodule.read("x")` inside the `try` exits non-zero and never prints `after`. So the three rows above are a claim about `RuntimeError` specifically, and a future change that made analyzer errors catchable — which would be wrong — fails this. |

A third item was not a finding but debris: a one-byte file `nope` (`0x01`) sat
committed at the repository root, written by an earlier round's `bytes.write`
program because it named a path relative to the working directory. Round 3
already fixed that class of bug in the test; the artifact it had left was never
cleaned up. Deleted. `git status` is clean after a full `cargo test
--all-targets`, which is the check that says the class is gone.

#### Round 4's failing-test-first sequence

The defect was introduced, the test run, and the change undone:

| Reintroduced | Test | Real output |
|---|---|---|
| `uppercase` in `src/stdlib.rs:477` made to answer `""` instead of `None` when called with no arguments — i.e. a bad call that no longer fails at all | `edge_a_bad_module_call_is_a_catchable_error_and_not_an_abort` | `left: "\nafter\n" right: "caught\nafter\n"` — `caught` was never printed, because nothing failed |

Both the reconciliation gates were re-proven against the same tree while round 4
was open, since a test file that passes proves nothing about the tree it reads:

| Reintroduced | Test | Real output |
|---|---|---|
| `"zap"` added to `stdlib::MODULES` | `edge_a_module_in_the_registry_is_named_by_a_document`, `edge_the_member_table_covers_exactly_the_modules_that_are_registered` | both FAILED |
| `"text"` removed from `stdlib::MODULES` | 7 tests in `stdlib_module_names_test`, including `edge_a_module_the_documents_name_is_in_the_registry` and `every_module_the_documents_name_answers_a_call` | `the documented module 'text' does not answer a call:` / `AnalyzerError: Unknown variable 'text'; did you mean 'test'?` |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass, exit 0, no warnings |
| `cargo test --all-targets` | **1427 passed, 0 failed, 0 ignored** across 53 result lines |
| `rb run` over `examples/*.rb` and `modules/*.rb` | 9 files, 0 failures |
| `rbops/verify.sh phase-050` | **not run** — there is no `rbops/` directory in this checkout (`ls: cannot access 'rbops': No such file or directory`), and the run instructions forbid spending a tool call on the pipeline that lives elsewhere. Recorded as FINDINGS.md §7, as phase-049 did. Nothing is claimed about it. |

1427 is the 1426 this run measured on the inherited tree plus the one test round 4
added, `edge_a_bad_module_call_is_a_catchable_error_and_not_an_abort`. Nothing
else moved: the same 53 result lines, and every count in every other file is the
one the previous round recorded. (The 1422 the previous round wrote was four
short of what the same tree measures; it is corrected here rather than carried
forward.)

## Invariants touched

- None. No `.rb` extension, grammar, `Value` variant, `Error` variant, `say`,
  trailing-comma or `{interp}` change. The language behaves identically before
  and after; words in two Markdown files, one constant in a test and the tests
  that read them moved.

## Test-requirement matrix

| Row | Status |
|---|---|
| empty | covered — `bytes.text(bytes.from_text(""))` answers empty text; `sys.argv()` outside `rb run`; `time.unix("")` is not a date and is refused |
| singleton | covered — `bytes.from_text("H")` is `[72]`, one byte |
| boundary | covered — bytes `0` and `255` round-trip and are written to a file read back as `[0, 255]` |
| out_of_bounds | covered — `bytes.text([256])` and `bytes.text([-1])` are refusals naming the value, not wrapped values |
| type_mismatch | covered — `files.read(1)`, `math.sqrt("a")`, `text.split("a", 1)`, `bytes.from_text(1)`; and each of those three is run a second time inside `try … catch error`, so the refusal is also proved to be catchable rather than fatal |
| numeric_boundary | covered — the byte range is the numeric boundary this module has: 0, 255, 256, −1, 300; `time.format(0, "%Y")` is `1970` |
| unicode | covered — `bytes.from_text("é")` is `[195, 169]` and back; `length("héllo")` is 6 in both arms |
| nesting_recursion | N/A — no new nesting or recursion; the documents' only higher-order builtin (`list.map`) is covered by the too-many-arguments case |
| duplicate_missing_keys | N/A — no record key handling changed; no module takes a record |
| malformed_input | covered — `bytes.text([0, 255])` is bytes in range that are not UTF-8, refused by name; `time.unix("")` is not a date and is refused; a bare `nosuchmodule` receiver is refused in the spelling the program wrote |
| resource_limit | N/A — this phase adds no recursion, no loop, no allocation and no limit; the existing call-depth and step budgets are untouched and unchanged. The loopback server round 3 adds is bounded by a 30-second accept deadline and a 10-second read deadline, and holds at most two exchanges. Round 4 adds none |
| process_control | N/A — nothing signals or exits; `network` is the only module that starts anything, and its transport is a loopback listener owned by the test |

## Known gaps / follow-ups

- `ask` is an ordinary identifier and nothing reads input. SPEC.md § console no
  longer documents it, and `edge_the_documented_console_lines_answer` pins the
  refusal so the section cannot promise it again — but the feature is missing,
  not the document → FINDINGS.md §3.
- Builtins silently ignore extra arguments (`text.uppercase("a", "b")` answers
  `A`). The "too many arguments" edge is pinned on `list.map`, which checks its
  arguments, and nothing asserts the tolerance is correct → FINDINGS.md §2.
- `filter` and `reduce` are the two entries left in `KNOWN_GAPS`, and they are
  the two the new `edge_a_known_gap_is_still_a_gap` runs on every build — so the
  list can no longer outlive the gap it names the way thirteen of its entries
  did. Neither has agreed semantics in SPEC.md → FINDINGS.md §4.
- Only SPEC.md's `### console` section is *executed* by its own test. The other
  sections' lines are covered by `the_text_and_math_examples_answer_what_the_spec_claims`
  and by `every_module_the_documents_name_answers_a_call`, which is one program
  per module rather than one per line. Running every line of every section is
  the honest version and is not done.
- `ROADMAP.md` still writes `formats.parse_json` → FINDINGS.md §5.
- `catch error` binds nothing. There is no `catch error as e`, so a program can
  tell *that* a module call failed and can carry on, but cannot read the message
  and cannot choose to handle one error and re-raise another. That is why
  `edge_a_bad_module_call_is_a_catchable_error_and_not_an_abort` pins catchability
  in one test and the error's wording in another: the second property is not
  reachable from inside the program. → FINDINGS.md §8.
