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
- **Round 11 found a fourth copy of the list.** `src/repl/completer.rs` kept its
  own `builtins:` table for tab-completion, and it had drifted in both
  directions: it offered `formats` — the name this phase exists to retire — and
  five flat names that answer nothing, and did not offer five modules that do.
  See "Round 11" below; that is the only thing round 11 changes.

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
| `src/stdlib.rs` | +33 −7 | Round 5: `module_has_member`, and `module_member_name` now unwraps a qualified name only for a pair `MODULE_MEMBERS` holds. `length` is named under `text` and `list`. See round 5 below. |
| `tests/stdlib_module_names_test.rs` | +150 −6 | Round 5: the dotted extraction keeps the case a program writes; the member-table check compares sets; two new tests — the closed surface, and the three SPEC promises by name. |
| `tests/text_split_join_test.rs` | +44 −3 | Round 5: the reachability row names its dotted member separately from the registered name, so `json_parse` is reached as `json.parse` rather than `json.json_parse`. |
| `src/stdlib.rs` | +27 −13 | Round 6: `filter` and `reduce` are out of `MODULE_MEMBERS`' `list` row, because a builtin nothing implements is not a module member. See round 6 below. |
| `src/interpreter.rs`, `src/bytecode/vm.rs` | +26 | Round 6: both engines ask `module_has_member` before the `<module>_<member>` registry hit, so the table is the enforced surface on that path too. |
| `SPEC.md` | +36 −6 | Round 6: every member of every module written in its own section — `text.trim`/`contains`/`starts_with`/`ends_with`/`replace`/`length`, `math.abs`/`ceil`/`sin`/`cos`/`tan`/`log`/`exp`, `files.lines`/`delete`/`copy`/`rename`, `list.length` — and § list's sentence rewritten for `filter`/`reduce`. § json and csv's single-quoted JSON is corrected to an escaped text, because there are no single-quoted strings. |
| `README.md` | +18 −8 | Round 6: the same members, written dotted; § Math and § Text keep the flat spellings as a comment so the quick reference is still one. |
| `tests/stdlib_module_names_test.rs` | +216 −46 | Round 6: the refusal matrix's universe is the **short** member names and its skip list is the documents; a new gate reads the table and asks each document; the heading join-word is folded case-insensitively; four code blocks are run as SPEC.md writes them. |
| `src/stdlib.rs` | +69 | Round 7: `dotted_call_reaches` — the whole rule for a receiver that is not an object, in one place, so the two engines cannot hold two halves of it. |
| `src/interpreter.rs`, `src/bytecode/vm.rs` | +30 −25 | Round 7: both `call_method`s call it, and `is_module_name` is replaced by `is_declared_module` — the program's own modules, asked apart from [`stdlib::MODULES`]. |
| `tests/stdlib_module_names_test.rs` | +283 | Round 7: three gates — the registry's non-module prefixes are not namespaces, a program-declared module owns its own name, and `length` answers a text or a list and says so in the document. |
| `SPEC.md` | +5 −2 | Round 7: § text's `length` sentence corrected — it answered a text and a list, not a record, which four other layers already said. |
| `SPEC.md` | +9 −3 | Round 8: § list's third spelling of `map` is `[1, 2, 3].map(f)`, not `xs.map(f)` — a variable receiver is a value, not a module — and the refusal is written down rather than left as a promise no gate read. |
| `src/stdlib.rs` | +6 −6 | Round 8: the `length` comment above `MODULE_MEMBERS`' `list` row still claimed a record was answered — the same false sentence round 7 took out of SPEC.md, left in the one file a reader opens. |
| `tests/stdlib_module_names_test.rs` | +233 | Round 8: two gates — a dotted receiver that is no module answers or is written down as refused, and every spelling SPEC.md enumerates for `map` is run. Three helpers, one of which keys on the `(` so a path and a host are not read as a receiver. |
| `phases/phase-050/FINDINGS.md` | +51 −27 | Round 8: §14 split into the half this phase owns (the document) and the half it does not (making `xs.map(f)` answer), and §15 added for the engine-parity defect the new gate found on the way — the bytecode VM refuses a higher-order call the tree-walker answers. |
| `src/bytecode/vm.rs` | +158 | **Round 9, the behaviour fix**: `Frame::pending_map` and `PendingMap`; `start_map`/`advance_map` carry a `map` on the frame that started it; `call_named` answers `map`/`list_map` **before** the shared resolver; `call_method` answers the one expression receiver the language has, `[1, 2, 3].map(f)`; `discard_frames_above`/`unwind_frame` give a map up when its frame is taken away. Turns `map was called with arguments it cannot use` into `[2, 4, 6]` on the bytecode VM. See round 9 below. |
| `src/repl/completer.rs` | +98 −39 | **Round 11, the behaviour fix**: the completer's hand-kept builtin table is split — the module names are read from `stdlib::MODULES` (the table the analyzer asks), and the six entries no program can name (`formats`, `parse_json`, `to_json`, `read`, `write`, `exists`) are gone. See round 11 below. |
| `src/repl/completer.rs` | +58 | Round 11: `edge_every_module_the_language_has_is_offered` and `edge_the_repl_offers_no_builtin_a_program_cannot_call`, beside the table they check. |
| `tests/repl_module_completion_test.rs` | +78 | Round 11, new: the same two claims through the public surface a user reaches — `ReplCompleter::complete` and the interpreter that decides whether a name works. |
| `src/interpreter.rs` | +9 −1 | Round 9: `apply_map` reports a call failure under `function.name` rather than a fixed "anonymous function", so a depth limit reached inside a `map` of a *named* function names that function — which is what the bytecode VM says, and what makes the two engines' messages comparable. A function *literal* already carries `ANONYMOUS_FUNCTION`, so it says the same thing either way. Plus the doc comment at `map_builtin`, corrected to name `[1, 2, 3].map(f)` rather than the `xs.map(f)` round 8 removed from SPEC.md. |
| `src/bytecode/codegen.rs`, `src/bytecode/opcode.rs` | +8 −4 | Round 9: comments only. Both said a bare method name was "the one shape the language rejects"; a list literal receiver is now answered, so they say a bare name says the receiver was not a name at all and point at the runtime that answers it. |
| `tests/differential_test.rs` | +130 | Round 9: `edge_a_higher_order_call_answers_the_same_way_on_both_engines` — eleven cases asked on **both** engines, comparing the two outcomes rather than asking whether each succeeded. This is the gate that makes the fix unfalsifiable-by-refusal. |
| `tests/stdlib_module_names_test.rs` | +220 (round 9) | Round 9: `edge_every_spelling_the_spec_promises_for_map_answers` now walks `ENGINES` instead of `rb run` alone — the gate round 8 wrote on one engine, widened once the defect it named was fixed. The rest of round 9's additions are the `xs` receiver case and the closure-expression note in the doc comment. |
| `phases/phase-050/REPORT.md` | +122 −4 (round 10) | Round 10: **documentation only.** No production code and no test changed — the Gates table reported 1435 tests over 53 result lines where the suite is 1463 over 54, and the correction is to the table. Plus the two gates below, shown failing on the defects they name. |
| `src/stdlib.rs` | +46 | **Round 12, the first half of the review's BLOCKER**: `is_module_member_encoding` and `flat_builtin_names()` — the answer to "which builtins can a person type without a module in front of them", derived from `BUILTIN_NAMES` rather than written out. See round 12 below. |
| `src/repl/completer.rs` | +369 −91 | **Round 12, the rest of the BLOCKER and the MAJOR**: `builtins:` is now `module_names() + flat_builtin_names() + GLOBAL_CONSTANTS` (21 names that answer are added), `keywords:` is read from `lexer::KEYWORDS` with the two-word pairs joined (`might fail` arrives with `give back`), and `by` is the one named exception in `POSITIONAL_KEYWORDS`. |
| `src/repl/completer.rs`, `tests/repl_module_completion_test.rs` | +12 tests | Round 12: seven gates beside the tables and five through the public surface — a keyword parity gate, a flat-builtin superset gate, and a gate that no `<module>_<member>` encoding is offered flat. |
| `src/stdlib.rs` | +46 | **Round 13, the review's BLOCKER**: `UNIMPLEMENTED_BUILTINS` and `is_unimplemented` — the registered builtins nothing dispatches — and `flat_builtin_names()` drops them as well as the module-member encodings. `filter` and `reduce` are no longer offered. |
| `src/repl/completer.rs`, `tests/stdlib_module_docs_test.rs`, `tests/repl_module_completion_test.rs` | +4 tests | Round 13: the parity gate asks `flat_builtin_names()` rather than `is_builtin()`, and four gates now **run** what is offered — the offered names against the programs in `answerable_builtins`, each unimplemented name against a refusal, and the two tables of gaps against each other. See round 13 below. |

Round 2, round 3, round 4 and round 5 moved every one of the round-1
`SPEC.md`/`README.md` line numbers; those below are the current ones.

Rounds 1 to 4 changed no interpreter behaviour. `MODULES` itself is unchanged in
every round — every name in it answers and every name the documents name was
already in it. `src/runtime.rs` is untouched by round 3: the `network` defect the
review found was in a test, and was shown to be there by breaking `runtime.rs` on
purpose and watching the old row pass and the new one fail. Round 5 is the first
round that changed what the language answers, and only in the direction SPEC.md
already described: see "Invariants touched" below.

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

All in `tests/stdlib_module_names_test.rs` — 22 `#[test]` functions, 20 named
`edge_*`. (The previous round's table listed 9; the five it omitted are rows 4,
5, 8, 10 and the round-4 row 13, and are restored here. Round 5 adds rows 15
and 16, round 7 adds rows 17 to 19, and round 8 adds rows 20 and 21. Round 9
widened row 21 from one engine to two rather than adding a test here — its new
parity gate lives in `tests/differential_test.rs` and is listed under Round 9
below. **Round 11's rows 22 and 23 are the first tests of this phase that are not
in that file**: they belong to the REPL completer, so they sit beside the table
they read in `src/repl/completer.rs`, and rows 24 and 25 repeat both claims
through the public surface from `tests/`, because the private table is where a
gate is strongest and the public API is where a user is.)

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
| 15 | `edge_a_flat_builtin_is_not_spelled_as_a_member_of_any_other_module` | every `module.builtin` pair `MODULE_MEMBERS` does not hold — eleven modules against the whole registry — is refused by its dotted spelling, **in both engines**; a name the parser reserves is a `ParserError` naming it |
| 16 | `edge_a_module_does_not_answer_a_function_the_documents_say_it_does_not_have` | the three SPEC promises by name — no `console.input`, no `math.random`, no `json.uppercase` — each through the binary, with the flat `input` and `random` asserted to still answer |
| 17 | `edge_a_registered_builtin_is_not_a_module_namespace` | **the receiver**, which row 15 never asked about — every `<name>_<member>` in the registry whose `<name>` is not a module (14 pairs, from the registry rather than from the table) is refused in both engines, the four live `random` pairs named, and the flat spellings behind each prefix asserted to still answer so the gate cannot be satisfied by deleting the builtin |
| 18 | `edge_a_module_the_program_declares_owns_its_own_name` | all 11 stdlib module names as a program-declared `module … end` — the member it publishes answers in both engines, the member it does not have is still refused by the dotted name, and the collision with the stdlib's table is decided the way an object already decides it |
| 19 | `edge_length_answers_a_text_or_a_list_and_refuses_a_record` | the drift a prose sentence can hide — all three spellings of `length` against a text, a list and a record, **and** § text read out of SPEC.md to assert it now says the refusal rather than claiming the record |
| 20 | `edge_a_dotted_receiver_that_is_no_module_answers_or_is_written_down_as_refused` | the receiver that is neither an object nor a module nor a registry prefix — every `name.member(` a document writes on a bare identifier no module claims either answers or is **written down as refused** in SPEC.md. `api.example.com` is the keying case: the scan requires the `(` that makes a dotted thing a call, and a chain of three names is dropped |
| 21 | `edge_every_spelling_the_spec_promises_for_map_answers` | an enumerated **prose** promise rather than a code block — the sentence in § list that enumerates the spellings of `map` is found by what it contains, the backticked runs are read out of it, and each is run and expected to print `[2, 4, 6]`. **Both** engines since round 9 (row 21 as written in round 8 walked `rb run` alone; see FINDINGS §15) |
| 22 | `edge_every_module_the_language_has_is_offered` (round 11, `src/repl/completer.rs`) | the completer's module list against `stdlib::MODULES` — the missing-module direction: `bytes`, `csv`, `json`, `sys` and `time` were offered by nothing |
| 23 | `edge_the_repl_offers_no_builtin_a_program_cannot_call` (round 11, `src/repl/completer.rs`) | the unanswerable direction, over the whole table: every offered name must be a module, a registered builtin or a global constant — `formats`, `parse_json`, `to_json`, `read`, `write`, `exists` were not |
| 24 | `edge_every_module_the_language_accepts_is_offered_by_the_repl` (round 11, `tests/repl_module_completion_test.rs`) | row 22 through the public surface, and paired with the analyzer's own predicate `stdlib::is_module` rather than with the table the completer reads |
| 25 | `edge_no_name_the_repl_offers_is_one_a_program_cannot_call` (round 11, `tests/repl_module_completion_test.rs`) | **asserts a failure is produced**: for each of the six names, `complete(name)` is empty *and* `run_source_value` is an `Err` — the completion and the interpreter cannot drift apart, because either half failing is this test failing |

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

### Round 5 — the third review

Four findings, all fixed. The BLOCKER is in `src/stdlib.rs`; the other three are
in the test file that is supposed to *be* this phase's gate, which is the order
that matters: a defect in a gate is worse than the defect it was written to catch,
because it is the one place the tree stops asking.

| # | Severity | What was wrong | What changed |
|---|---|---|---|
| 1 | BLOCKER | `src/stdlib.rs:679` unwrapped `module_member` to `member` whenever `MODULES.contains(&module) && BUILTIN_NAMES.contains(&member)` — any module against any registered builtin. Every module was therefore a namespace for the whole registry: `console.input("n")` read a line, `math.random(1, 6)` drew one, `json.uppercase("hi")` answered `HI`, `csv.type_of(1)` answered `number`, each contradicting SPEC.md in as many words (`there is no console.input`, SPEC.md:1024; `math.random is not implemented`, SPEC.md:1064; `a module has the functions that are spelled <module>_<member> in the registry`, SPEC.md:1010). | `module_has_member(module, member)` is the predicate, and `module_member_name` unwraps only for a pair `MODULE_MEMBERS` holds *and* whose bare name is registered. Every other pair is `None`, which both engines already turn into `Module '<m>' has no function '<x>'`. The flat spellings are untouched — the rule is about the dotted spelling and nothing else. |
| 2 | MAJOR | `tests/stdlib_module_names_test.rs:1169` ran `say text.length("héllo")` as a case that answers, and `MODULE_MEMBERS` listed no `length` under `text`. The table the per-member gate walks was therefore *not* the set of spellings that answered — which is only visible once finding 1 is fixed, and is the same defect in the table. | `length` is named under `text` and under `list`, the second because `tests/text_split_join_test.rs:415` pins `list.length` and would otherwise have failed for a reason nobody could read. The table is now the surface the resolver enforces, not only the one the test walks. |
| 3 | MAJOR | `dotted_calls` lowercased both halves of every pair it extracted, and so did the heading rule. `stdlib::is_module` compares exactly, so a document writing `JSON.Parse(..)` or `TEXT.UPPERCASE(..)` passed reconciliation and then failed every reader who copied from it with `Unknown variable`. | A dotted pair is kept **as written**, because that is the only spelling the language can be asked about. A *heading* is folded onto the module it names case-insensitively by `module_spelling`, because a heading is prose and prose capitalises — README.md writes `### Files` and `### JSON and CSV` where SPEC.md writes `### files`. Both directions are covered by mutations below. |
| 4 | MINOR | `edge_the_member_table_covers_exactly_the_modules_that_are_registered` compared `sorted(MODULE_MEMBERS)` against `MODULES` as a sequence, so reordering `MODULES` — which neither table promises not to do — failed the gate. | Both sides are sorted before the comparison, so it is a set equality and nothing else. The duplicate check above it is unchanged. |

Finding 1 was also a defect in a test that happened to pass because of it:
`tests/text_split_join_test.rs` built its dotted spelling as
`{module}.{builtin}` and so reached `json.json_parse(..)`, `csv.csv_parse(..)`
and `files.files_read(..)` — spellings no reader writes, which answered only
through the hole. The row now names its dotted member separately from the
registered name, asserts the member is in `stdlib::MODULE_MEMBERS` and resolves,
and then calls `json.parse(..)`. That is the same test doing more, not less.

#### Round 5's failing-test-first sequence

Every defect was reintroduced into the finished tree, the test run, and the
change undone. Nothing below is asserted; it is what the runs printed.

| Reintroduced | Test | Real output |
|---|---|---|
| `module_member_name` back to `MODULES.contains(&module) && BUILTIN_NAMES.contains(&member)` | `edge_a_flat_builtin_is_not_spelled_as_a_member_of_any_other_module`, `edge_a_module_does_not_answer_a_function_the_documents_say_it_does_not_have` | both FAILED |
| README.md's `json.parse(text)` respelled `JSON.parse(text)`, extraction keeping case | `edge_a_member_the_documents_name_is_a_function_the_registry_has`, `edge_a_module_the_documents_name_is_in_the_registry`, `every_module_the_documents_name_answers_a_call` | all three FAILED |
| …the same README mutation with the old lowercasing extraction | *(the hole, shown as what it was)* | `16 passed; 0 failed` — which is the defect, run |
| `MODULES` reordered to `"time", "sys", "bytes", …` | *(none — this is finding 4's fix)* | `16 passed; 0 failed`, where the old assertion failed on the same tree |

The BLOCKER, before the fix, against the binary:

```
$ ./target/debug/rb run /tmp/opencode/p50/a.rb     # say console.input("name? ")
name?
exit=0
$ ./target/debug/rb run /tmp/opencode/p50/b.rb     # random_seed(1) / say math.random(1, 6)
3
exit=0
$ ./target/debug/rb run /tmp/opencode/p50/c.rb     # say json.uppercase("hi")
HI
exit=0
```

and after it, the same three files:

```
$ ./target/debug/rb run /tmp/opencode/p50/a.rb
Error: RuntimeError: Module 'console' has no function 'input'
exit=1
$ ./target/debug/rb run /tmp/opencode/p50/b.rb
Error: RuntimeError: Module 'math' has no function 'random'
exit=1
$ ./target/debug/rb run /tmp/opencode/p50/c.rb
Error: RuntimeError: Module 'json' has no function 'uppercase'
exit=1
```

### Round 6 — the review of round 5

Five findings: one BLOCKER, two MAJOR, two MINOR. All five are fixed, and the
order they are fixed in is the order they matter in, because the BLOCKER was in
the gate that was supposed to be proving the MAJORs.

| # | Severity | What was wrong | What changed |
|---|---|---|---|
| 1 | BLOCKER | `tests/stdlib_module_names_test.rs:639` skipped a pair when `stdlib::member_resolves(module, builtin)` said so — a predicate derived from the very table the matrix is about. Adding `uppercase` to `csv`'s row made `csv.uppercase` answer *and* made the matrix skip `csv.uppercase`. The guard below it (`checked > BUILTIN_NAMES.len()`) could not see that either: a table that grants everything still leaves hundreds of pairs to refuse. | The skip list is `promised_by_the_documents()` — the `module.member` pairs the code blocks of SPEC.md and README.md write — which is independent of both tables. A skipped pair is now also asserted to be in `MODULE_MEMBERS`, and the three counters are asserted to add up to the size of the matrix, so a fourth column (a new `continue`) cannot appear without failing. The guard became `refused > promised_rows`: the promises have to stay the smaller half. |
| 2 | MAJOR | `length` was a member of `text` and of `list` in the table, pinned by `edge_length_counts_bytes_in_both_arms`, and written by **neither** document — SPEC.md § text and README.md § List both wrote it flat. Nothing checked the table against the documents (`tests/stdlib_module_names_test.rs:596` only read the documents and asked the table), so the phase had enshrined answering surface in exactly the drift it exists to remove. | Both documents now write every member of every module, dotted: `text.length`, `list.length`, `text.trim`/`contains`/`starts_with`/`ends_with`/`replace`, `math.abs`/`ceil`/`sin`/`cos`/`tan`/`log`/`exp`, `files.lines`/`delete`/`copy`/`rename`. The new `edge_a_member_the_registry_holds_is_written_by_a_document` reads the table and asks **each** document, so a member that answers without being written down fails by name. |
| 3 | MAJOR | The matrix walked `stdlib::BUILTIN_NAMES`, which holds qualified names (`files_read`, `json_parse`, `network_get`), so it asked about `files.files_read` — a spelling no reader writes — and never about `json.read`, `text.get` or `files.parse`. The pairs that have to be refused are the short ones. | `short_member_names()` builds the universe from the members `MODULE_MEMBERS` names plus every short name behind a registered builtin: the bare ones and the suffix of a qualified one (`files_read` is `read`). 64 names against 11 modules: **646 refused, 11 unparseable (`expect` is a reserved word), 47 promised**, where the old matrix asked 759 pairs of which none was a documented one. |
| 4 | MINOR | Both engines checked `get_var(<module>_<member>)` *before* the table, so a `<module>_<member>` entry in the registry answered a dotted call whether or not `MODULE_MEMBERS` held it — while `src/stdlib.rs` documented `module_has_member` as "the predicate both engines resolve a dotted call against". | Both engines ask `module_has_member` first, for a receiver that names a **stdlib** module (a declared `module … end` keeps the rule below it, since `stdlib::is_module` says nothing about a program's own modules). Verified: registering `text_input` and reverting the two checks answers `text.input("name? ")` through a builtin; with the checks in place it is `Module 'text' has no function 'input'`, and the matrix fails on that tree. |
| 5 | MINOR | The heading join-word skip was `word != "and"` while `module_spelling` folds case-insensitively, so `### JSON AND CSV` inserted a module called `AND`. | `!word.eq_ignore_ascii_case("and")`. |

Two things the new gates found in the documents while they were being written
down, which is the second time this has happened and the reason § json and csv
is in the list of sections that are run:

- `json.parse('{"name": "Alice"}')` — SPEC.md wrote it with single quotes, and
  there are no single-quoted strings: `LexerError: Unexpected character '\''`.
  Corrected to an escaped text, and the section now says so in a sentence.
- `set has to text.contains("abc", "b")` — the line I added first, and `has` is
  a reserved word (`object Name has x`). Caught by
  `the_text_and_math_examples_answer_what_the_spec_claims`, which now runs the
  whole code block of § text, § math, § list and § json and csv out of the file
  rather than only the lines it transcribes.

`filter` and `reduce` went the other way: they were in `MODULE_MEMBERS`, they
resolve to registered flat builtins, and neither builtin answers anything.
A member the table holds must be documented, and neither can be — so they are out
of the table, and `list.filter(xs, f)` is now `Module 'list' has no function
'filter'`, the same refusal SPEC.md already gives `math.random`. Both stay in
`known_gaps()` in `tests/stdlib_module_docs_test.rs` with the program that still
fails, which is where an unimplemented builtin belongs: a list of names that
cannot be called, rather than a namespace that accepts them.

#### Round 6's failing-test-first sequence

Every defect was reintroduced into the finished tree, the test run, and the change
undone. Nothing below is asserted; it is what the runs printed.

| Reintroduced | Test | Real output |
|---|---|---|
| `"uppercase"` added to `csv`'s member row | `edge_a_member_the_registry_holds_is_written_by_a_document`, `edge_a_flat_builtin_is_not_spelled_as_a_member_of_any_other_module` | both FAILED — `stdlib::MODULE_MEMBERS holds \`csv.uppercase\` … SPEC.md § Standard Library does not`, and `` `csv.uppercase` was refused, but not by the rule that says a module has no such member `` |
| `module_has_member` back to `MODULES.contains(&module) && BUILTIN_NAMES.contains(&member)` | 6 tests in this file, the matrix among them | `FAILED. 11 passed; 6 failed` — `` `bytes.abs` was refused, but not by the rule that says a module has no such member `` |
| `text_input` registered in `builtins()` and `BUILTIN_NAMES`, and the two new engine checks reverted | `edge_a_flat_builtin_is_not_spelled_as_a_member_of_any_other_module` | FAILED — `` `text.input` was refused, but not by the rule that says a module has no such member `` / `RuntimeError: text_input was called with arguments it cannot use` |
| SPEC.md § text's `set size to text.length("héllo")` line deleted | `edge_a_member_the_registry_holds_is_written_by_a_document` | FAILED — `stdlib::MODULE_MEMBERS holds \`text.length\`, so a program may write it, and SPEC.md § Standard Library does not` |
| README.md's `### JSON and CSV` capitalised, with the case-sensitive join-word check | `edge_a_module_the_documents_name_is_in_the_registry`, `every_module_the_documents_name_answers_a_call` | both FAILED — `README.md names the module \`AND\` and stdlib::MODULES does not` |
| …the same capitalisation with the case-insensitive check in place | *(the fix, shown as what it is)* | `17 passed; 0 failed` |

`text.input("name? ")` before and after the engine change, against the binary:

```
$ ./target/debug/rb run /tmp/opencode/p50/ti.rb      # checks reverted
Error: RuntimeError: text_input was called with arguments it cannot use
exit=1
$ ./target/debug/rb run /tmp/opencode/p50/ti.rb      # checks in place
Error: RuntimeError: Module 'text' has no function 'input'
exit=1
```

### Round 7 — the review of round 6

Three findings: one BLOCKER and two MAJOR. The BLOCKER and the first MAJOR are the
same defect seen from two sides — the receiver — and the shape is FINDINGS.md §12,
which is the third round in a row that has found this class in a new place.

| # | Severity | What was wrong | What changed |
|---|---|---|---|
| 1 | BLOCKER | `src/interpreter.rs:2119` and `src/bytecode/vm.rs:2757` asked `is_module` and `module_has_member` about the **member** and never about the **receiver**, so a name that is not a module at all fell through to `call(&qualified)`: `say random.seed(1)` answered `nothing`, as did `random.number`, `random.choice` and `random.shuffle`, in both engines. `random` is a registered builtin, so the analyzer is right to accept it as a receiver — `PI.times(..)` needs that — and `random_seed` being in the registry was on its own enough to make the call. There is no `random` module: `MODULES` does not list it and neither document names it. | `stdlib::dotted_call_reaches` is the whole rule, in one place, and both engines are callers of it: the receiver has to name a module, a module the program declared or imported owns the name, and a stdlib module answers only for a pair `MODULE_MEMBERS` holds. `random.seed(1)` is `RuntimeError: Module 'random' has no function 'seed'`. |
| 2 | MAJOR | The round-6 check was `stdlib::is_module(&module) && !module_has_member(&module, method)`, asked **before** the program's own module table — so a `module text … end` publishing `greet` was `Module 'text' has no function 'greet'`, and all eleven names in `MODULES` refused the same way. `declare_module` only rejects a second declaration, so the program had declared a legal module and the runtime had disagreed with it. | The declared-module case is asked **first**, and the stdlib table is not consulted for it at all: the program's own published members decide. This is the rule an object already follows (`tests/object_model_test.rs::object_named_like_a_module_takes_the_call` — a declared `object json` takes the `json.parse(..)` spelling over the module of the same name), so it is the same answer for a module. |
| 3 | MAJOR | `SPEC.md:1056` said "`length` answers a text, a list or a record alike". `src/runtime.rs` and `src/stdlib.rs` both refuse a record, `tests/test_text.rb:132` pins that as a **caught** error and `corpus/stdlib-0005`/`0006` record it. Four layers against one sentence, and the sentence was the one nothing could check. | The sentence is corrected, and `edge_length_answers_a_text_or_a_list_and_refuses_a_record` walks all three spellings against the runtime **and** reads § text out of SPEC.md to assert it now says the refusal — a prose claim needs its own gate, which is FINDINGS §4 and §10 in a third place. |

Finding 1 is also the defect that was invisible to a gate by construction: the
refusal matrix walks `stdlib::MODULES` × short member names, so it asked 646
questions about eleven receivers and none about the other side of that predicate.
The new `edge_a_registered_builtin_is_not_a_module_namespace` walks the
**registry's** prefixes instead — a source neither the table nor the documents can
skew, which is FINDINGS §10's rule applied to the loop rather than to the skip
list. Fourteen pairs, four of them live.

#### Round 7's failing-test-first sequence

Each defect was reintroduced into the finished tree, the test run, and the change
undone. Nothing below is asserted; it is what the runs printed.

| Reintroduced | Test | Real output |
|---|---|---|
| `dotted_call_reaches` back to the pre-fix fall-through (any bound `<receiver>_<member>` answers) | `edge_a_registered_builtin_is_not_a_module_namespace` | FAILED — `` `random.number` answered, so a builtin spelled `random_number` made `random` a module. The registry encodes how a module function is stored; it is not a list of what the language has. `` |
| the stdlib table asked first for a module the program declared (round 6's order) | `edge_a_module_the_program_declares_owns_its_own_name` | FAILED — `a module the program declared does not answer its own member, because the stdlib member table decided a receiver it says nothing about` |
| the drifted `length` sentence put back into SPEC.md | `edge_length_answers_a_text_or_a_list_and_refuses_a_record` | FAILED — `SPEC.md § text does not say that a record is refused as \`length requires a list or text\`, which is what every spelling above does` |

Before the fix, against the binary, in both engines:

```
$ ./target/debug/rb run /tmp/opencode/p50r3/r1.rb          # say random.seed(1)
nothing
exit=0
$ ./target/debug/rb compile /tmp/opencode/p50r3/r1.rb && ./target/debug/rb vm /tmp/opencode/p50r3/q.rbc
nothing
$ ./target/debug/rb run /tmp/opencode/p50r3/r3.rb          # module text … export all … end
Error: RuntimeError: Module 'text' has no function 'greet'
exit=1
```

and after it, the same files:

```
$ ./target/debug/rb run /tmp/opencode/p50r3/r1.rb
Error: RuntimeError: Module 'random' has no function 'seed'
exit=1
$ ./target/debug/rb run /tmp/opencode/p50r3/r3.rb
hi ada
exit=0
```

### Round 8 — the resumed run

The four gates were run **before** anything was changed. `cargo fmt --all -- --check`
was clean, `cargo clippy --all-targets -- -D warnings` exited 0, and `cargo test
--all-targets` reported 1433 passed / 0 failed across the same 53 result lines
round 7 recorded. `rbops/verify.sh` still does not exist in this checkout. So the
previous round's *green* claims held; what was left was the question of whether
they were **complete**, and one answer was no.

> **Round 10 corrects the number.** The same command on the same tree reports
> **1463 passed across 54 result lines**. 1433 is short by 28, which is exactly
> `tests/wasm_test.rs`, and 53 is short by one result line. This round could not
> determine whether that suite ran and was miscounted or did not run at all; what
> it can say is that the figure recorded here is not the suite's size, and every
> later round's total was copied forward from it rather than re-measured.

The hole is the third kind of dotted receiver. Round 7 added the registry's
prefixes to the set of things a gate refuses, and round 5–6 bounded the ones that
are modules, and the code-block runner runs what is inside a code block — which
leaves a bare identifier that is **neither**: a name in no module, in no registry
prefix, written into SPEC.md § list's prose.

```
$ rb run target/tmp/p50res/m1.rb   # set xs to [1, 2, 3] / say xs.map(to (x) give back x * 2 end)
Error: RuntimeError: Module 'xs' has no function 'map'
```

and SPEC.md:1148 said, in a sentence and not in a code block: "`list.map` is the
only higher-order builtin, and it works in all three spellings: `map(xs, f)`,
`list.map(xs, f)` and `xs.map(f)`." A reader copies the third and gets a runtime
error. FINDINGS §14 had recorded it as out of scope; the resumed run is the same
hand that wrote it, and the phase prompt settles half of it directly — *"or
SPEC.md is corrected to the names the code uses and the correction is named in
REPORT.md"* — so the document half was taken here and the semantics half left for
a promotion.

Two gates, one general and one specific, and the general one is the one that stops
this coming back:

| Test | What it holds |
|---|---|
| `edge_a_dotted_receiver_that_is_no_module_answers_or_is_written_down_as_refused` | Every `name.member(` that SPEC.md or README.md writes on a bare identifier no module claims either **answers** or is **written down as refused**. This is the receiver the other three universes never asked about. |
| `edge_every_spelling_the_spec_promises_for_map_answers` | The enumerating sentence in § list is read **out of the document** and every spelling in it is run through `rb run`, expected `3` doubled. A fourth spelling written into that sentence is run rather than believed. |

Plus three helpers (`subheadings`, `called_dotted_receivers`, `backticked_calls`)
and one comment correction in `src/stdlib.rs`.

`called_dotted_receivers` keys on the `(`. That is the whole filter and it is
load-bearing: § Standard Library writes `config.rb`, `out.bin`, `copy.rb`,
`moved.rb`, `data.rb`, `log.rb`, `output.rb`, `test_text.rb`, `README.md` and
`api.example.com`, and an identifier in this language cannot contain a `.` — so a
scan without it reads every dotted thing a document mentions as a receiver. A
chain of three or more names is dropped for the same reason. `api.example.com` was
the mutation that proved it: it is the only dotted thing in the two documents
that is neither a module, a path with a known extension, nor a call, and before
the `(` was required it was `api.example` — a receiver.

#### Round 8's failing-test-first sequence

Both tests were written and run against the tree **before** the SPEC.md sentence
was touched. Both failed, for the reason they name:

```
$ cargo test --test stdlib_module_names_test edge_
test edge_a_dotted_receiver_that_is_no_module_answers_or_is_written_down_as_refused ... FAILED
test edge_every_spelling_the_spec_promises_for_map_answers ... FAILED

panicked at tests/stdlib_module_names_test.rs:2233:
SPEC.md ### list writes `xs.map`, which does not answer and does not say it is refused —
so a reader who copies it gets an error instead of the answer the document promised.
panicked at tests/stdlib_module_names_test.rs:2304:
SPEC.md § list promises `xs.map(f)` and `rb run` refused it:
```

After the sentence was corrected, both pass — and two further mutations were
introduced into the finished tree to show the gates are not vacuous. Each was
reverted; neither is in the diff:

| Reintroduced | Test | Real output |
|---|---|---|
| `xs.reverse()` added to § list's prose with no refusal written down | `edge_a_dotted_receiver_that_is_no_module_answers_or_is_written_down_as_refused` | FAILED — ``SPEC.md ### list writes `xs.reverse`, which does not answer and does not say it is refused`` |
| `xs.each(f)` added to the enumerating sentence as a fourth spelling | `edge_every_spelling_the_spec_promises_for_map_answers` | FAILED — `SPEC.md § list promises `xs.each(f)` and `rb run` refused it:` |

The first is the gate's own property on a name it has never seen; the second is
the gate's own extraction on a sentence that has grown. Neither depends on `xs.map`
being in the document.

#### Round 8 also corrected a comment that round 7 left contradicting itself

Round 7 corrected SPEC.md's claim that `length` answers a record. The comment
above `MODULE_MEMBERS`' `list` row in `src/stdlib.rs` was not corrected with it and
still said `length` "answers a list, a text and a record alike" — the same false
sentence, in the one file a reader opens to find out what the language has. It now
says what `runtime::builtin` has always done: it answers a list and a text, and
refuses a record.

### Round 9 — the engine-parity defect round 8 recorded, fixed

Round 8 found §15 and ran its gate on one engine rather than two. That was the
right trade *for round 8* — a gate that asserts a known defect is a gate that
stops passing once the defect is fixed — but it left the defect open, and round
9's review was right to name it: a phase that ends with a green gate and an open
defect it documented has not reconciled anything, it has described something.

So the defect was fixed rather than the gate narrowed.

```
$ rb run n4.rb                     # to f(x) give back x * 2 end, then the three spellings
[2, 4, 6]
[2, 4, 6]
[2, 4, 6]
$ rb compile n4.rb -o n4.rbc && rb vm n4.rbc   # round 8: map was called with arguments it cannot use
[2, 4, 6]
[2, 4, 6]
[2, 4, 6]
```

The fix was smaller than §15 guessed. FINDINGS wrote the cause as a
value-representation mismatch — the VM putting something other than a
`Value::Function` on the stack — and it is not: the VM did put a function there.
The resolver simply got asked first, and `map` is a registered builtin whose
`runtime::builtin` arm cannot *call* a Redblue function, so it claimed the name
and refused it as one it cannot use. `src/interpreter.rs` already excluded those
two names ahead of its own resolver call; `src/bytecode/vm.rs` now does the same,
in `call_named` and in `call_method` for a literal receiver.

The part that was not optional is where the map is carried. The obvious
implementation drives each element with a nested `Self::drive`, which costs one
**host** stack frame per nesting level: a `map` inside the function it calls would
overflow the thread — a crash, not a diagnostic — on a program the tree-walker
runs fine, and at a depth far below `MAX_CALL_DEPTH`. So the map lives on the
frame that started it (`Frame::pending_map`), each element is one ordinary
`call_function` that charges and releases the depth exactly as a call a program
writes does, and `advance_map` hands the next element its turn when the previous
one returns. That is why `a map of a map` is one of the differential gate's
cases and why it is asked on both engines: it is the case that would crash rather
than disagree.

Two gates hold it, and neither can be satisfied by making the engines agree the
wrong way:

| Test | What it holds |
|---|---|
| `edge_a_higher_order_call_answers_the_same_way_on_both_engines` (`tests/differential_test.rs`) | Eleven cases on both engines, comparing the two **outcomes**. A VM that agreed by *refusing* what the tree-walker answers fails here, which is the failure mode round 8's one-engine gate could not have seen — it never asked. Covers the three spellings, a captured binding, output written from inside a mapped function, a map of a map, an empty list that never calls the function, and failures under `might fail` and `catch`. |
| `edge_every_spelling_the_spec_promises_for_map_answers` (`tests/stdlib_module_names_test.rs`) | The spellings read **out of** SPEC.md § list, now walked through `ENGINES` rather than `rb run`. A fourth spelling written into that sentence is still run rather than believed, now on both engines. |

The one thing §15 still holds is that a closure **expression** (`set f to to (x) …
end`) does not compile for the bytecode VM at all:

```
$ rb compile n2.rb -o n2.rbc
ParserError: `to (x) ... end` as an expression is not compiled yet; run programs that use one with `rb run`
```

That is a compiler gap rather than a runtime one, so it is recorded and left for
a bytecode phase. It is not a skip: every gate above uses a **named** declaration
(`to f(x) … end`), which both engines compile, and the doc comment on
`edge_every_spelling_the_spec_promises_for_map_answers` says why, so a reader who
wonders why the gates do not use a lambda is told rather than left guessing.

Nothing was weakened to get here: `ENGINES` gained no skip, no test was deleted,
nothing was `#[ignore]`d, and no lint was allowed. The one gate that changed is
the same gate widened from one engine to two.

### Round 10 — a report that did not match its own gate

The three gates that exist in this checkout were run first, before anything was
changed:

```
$ cargo fmt --all -- --check          # exit 0, no diff
$ cargo clippy --all-targets -- -D warnings   # exit 0, no warnings
$ cargo test --all-targets            # 1463 passed, 0 failed, 0 ignored
```

So round 9's work is green and nothing in it regressed. What this round found was
in the report rather than in the tree: the Gates table said **1435 passed across
53 result lines**, and that number was wrong.

```
$ cargo test --all-targets -- --list | grep -c ": test$"
1463
$ ls tests/*.rs | wc -l
52                       # + the library + the rb binary = 54 result lines
$ cargo test --all-targets | grep -c '^test result'
54
```

Three independent measurements agree on 1463 over 54 result lines. The table was
short by 28 tests and by one result line, and 28 is exactly `tests/wasm_test.rs` —
a suite that has existed since phase-047 and is not gated on anything. The figure
in the table was round 7's, carried forward through rounds 8 and 9 as a running
total of gates added rather than re-measured each round, and a total nobody
re-read is the one number in a report that can drift furthest from the truth
without anything turning red.

That is the whole of round 10's findings: **no line of production code changed,
and no test changed.** The correction is to the table, and it is measured rather
than asserted. It is worth naming what kind of finding this is, because it is the
one AGENTS.md § 4 calls fake completion and because a gate cannot catch it: a
wrong count is not a failing assertion, it is a sentence nobody is checking. The
suite was green and complete the whole time; the *report* was not.

### Round 10's two gates, shown failing

A green suite says nothing on its own about whether a gate can fail, and round 9 is
the round whose claims sit on top of that stale count. So both of round 10's checks
were made to fail on purpose, on the defect each one names, and then undone.

**The reconciliation gate, against a drifted document.** `SPEC.md:1258` reads
`### time`; it was changed to `### wall clock` and nothing else:

```
$ cargo test --test stdlib_module_names_test
test edge_a_module_the_documents_name_is_in_the_registry ... FAILED
SPEC.md names the module `clock` and stdlib::MODULES does not, so a program using
it dies as 'Unknown variable 'clock''. Add `clock` to stdlib::MODULES, or correct
the document.
test every_module_the_documents_name_answers_a_call ... FAILED
test result: FAILED. 20 passed; 2 failed; 0 ignored
```

`SPEC.md` was restored from a copy taken before the edit, and `git status` says so.

**The round-9 parity gate, against the round-9 fix itself.** `call_named`'s early
answer for `map`/`list_map` — the four lines that stop the shared resolver from
claiming the name first — were disabled with `if false &&`:

```
$ cargo test --test differential_test edge_a_higher_order_call
test edge_a_higher_order_call_answers_the_same_way_on_both_engines ... FAILED
the flat spelling: the two engines disagree.
failed: RuntimeError: map was called with arguments it cannot use at 5:1
test result: FAILED. 0 passed; 1 failed
```

on the **first** of the eleven cases, with the exact message §15 records. Round 9
reported the defect and the fix but not this, and a gate whose failure nobody has
watched is a gate this phase would be asked about, so round 10 watched it.
`src/bytecode/vm.rs` was restored from a copy taken before the edit and is
byte-identical to `HEAD`.

Two things this round checked rather than assumed, because round 9's claims were
the ones on the table:

```
$ cargo run --quiet --bin rb -- run target/tmp/p50/m.rb   # the three spellings
[2, 4, 6]
[2, 4, 6]
[2, 4, 6]
$ cargo run --quiet --bin rb -- compile target/tmp/p50/m.rb -o target/tmp/p50/m.rbc
$ cargo run --quiet --bin rb -- vm target/tmp/p50/m.rbc  # the bytecode VM
[2, 4, 6]
[2, 4, 6]
[2, 4, 6]
$ cargo run --quiet --bin rb -- run target/tmp/p50/nope.rb   # say nosuchmodule.read("x")
Error: AnalyzerError: Unknown variable 'nosuchmodule'
exit=1
$ for f in examples/*.rb modules/*.rb; do cargo run --quiet --bin rb -- run "$f"; done
9 files, exit 0 each
$ git status --short      # after all of the above
(empty)
```

`git status` being empty is the check worth keeping: round 1 left a one-byte file
at the repository root and round 3 had to delete it, so a run that reports nine
green files and a dirty tree would be telling the reader something the table above
does not.

### Round 11 — a fourth copy of the module list, in the one place that invites a user to type one

Rounds 1 to 10 reconciled SPEC.md, README.md and `stdlib::MODULES` and put gates on
all three. The gate walks **documents** and the registry. It does not walk the
REPL, and the REPL is the one place Redblue *offers* a name rather than waiting to
be asked: a user who types `for` and presses Tab is told what can follow it.

`src/repl/completer.rs` kept its own `builtins:` table. It had drifted in **both**
directions:

```
$ cargo test --lib repl::completer        # the two new tests, before the fix
---- edge_every_module_the_language_has_is_offered stdout ----
bytes is a module, so a user typing it is offered it: []
---- edge_the_repl_offers_no_builtin_a_program_cannot_call stdout ----
formats is offered as a builtin, so it has to be one a program can name:
it is not a module, not a registered builtin, and not a global
test result: FAILED. 15 passed; 2 failed
```

and the six names the table offered that no program can name, each refused by the
built interpreter:

```
$ cargo run --quiet --bin rb -- run formats.rb      # say formats.parse_json("{}")
Error: AnalyzerError: Unknown variable 'formats'
$ cargo run --quiet --bin rb -- run parsejson.rb     # say parse_json("{}")
Error: RuntimeError: Unknown function 'parse_json'
$ cargo run --quiet --bin rb -- run tojson.rb       # say to_json({a: 1})
Error: RuntimeError: Unknown function 'to_json'
$ cargo run --quiet --bin rb -- run flat_read.rb     # say read("x")
Error: RuntimeError: Unknown function 'read'
   …and the same for write and exists — the file functions are files.read etc.
```

`formats` is this phase's own headline name: SPEC.md and README.md stopped writing
it in rounds 1 and 2, and the completer went on offering it — which is worse than a
document, because a document is read and a completion is *accepted*. `parse_json`
and `to_json` are that same name flat-spelled; `read`, `write` and `exists` are
real functions that live under `files.` and answer to nothing on their own. And
the list was short five modules that work — `bytes`, `csv`, `json`, `sys`,
`time` — so `json` and `csv` completed to nothing at all while `json.parse` is
the first call of the code block SPEC.md opens its JSON section with
(`SPEC.md:1220`).

Both sides of that, measured rather than argued — the pre-round-11 table restored
from the commit, then the fixed one, each answering the same six prefixes:

```
prefix        before                                after
json          []                                    ["json"]
csv           []                                    ["csv"]
time          ["times"]        (the keyword)        ["time", "times"]
j             ["join"]         (a builtin)          ["join", "json"]
formats       ["formats"]      (a name with no       []
                              meaning in the
                              language)
read          ["read"]         (Unknown function)    []
parse_json    ["parse_json"]   (Unknown function)    []
```

`time` is the one worth reading twice: the keyword `times` was the *only* thing
offered, so a user who typed `time` and pressed Tab got `times` back and would
have accepted it.

**The fix is the one this file already had a precedent for**, in the same struct:
`commands: commands::ReplCommand::bare_names()` reads the REPL's command words
from the parser's table, because a hand-kept copy of those had drifted too
(`edge_the_command_vocabulary_is_the_parsers_own_table`,
`src/repl/completer.rs:505`). `builtins:` now reads its module half from
`stdlib::MODULES` — the table `src/analyzer.rs:591` asks for a bare receiver — and
its flat half holds only names `stdlib::BUILTIN_NAMES` and `stdlib::GLOBAL_CONSTANTS`
hold. Nothing else about completion changes: the same words are offered for the
same prefixes, and the six that were dropped were names a program could never
call.

**Both new tests were watched failing first, and both mutations were watched failing
afterwards.** Reinstating `formats` (with `parse_json` and `read`) in the table:

```
$ cargo test --lib repl::completer::tests::edge_the_repl_offers_no_builtin
test repl::completer::tests::edge_the_repl_offers_no_builtin_a_program_cannot_call ... FAILED
formats is offered as a builtin, so it has to be one a program can name: …
test result: FAILED. 0 passed; 1 failed
$ cargo test --test repl_module_completion_test
test edge_no_name_the_repl_offers_is_one_a_program_cannot_call ... FAILED
formats is not a name Redblue has, so it is not offered: ["formats"]
test result: FAILED. 1 passed; 1 failed
```

and dropping one module from what is offered, which is the other direction:

```
test edge_every_module_the_language_has_is_offered ... FAILED
time is a module, so a user typing it is offered it: ["times"]
test edge_every_module_the_language_accepts_is_offered_by_the_repl ... FAILED
time is a module a program can write, so typing it is completed: ["times"]
```

`src/repl/completer.rs` was restored from a copy taken before each mutation and is
byte-identical to the fixed file.

**What round 11 did not fix, on purpose.** The `keywords:` table in the same struct
has drifted the same way — it offers `ask`, `new`, `async`, `done` and `when`,
none of which start anything, and does not offer `constant`, `might`, `fail`,
`as`, `test` or `expect`, all six of which are in `lexer::KEYWORDS`. That is a
question about statements rather than about module names, so it is FINDINGS.md §16
with the full list and the two wrinkles a fixing phase has to decide (`give back`
is two keywords; `by` is read positionally and is not in `lexer::KEYWORDS`).
Nothing was changed there. **Round 12 changed both of those, and the flat half of
`builtins:` with them** — see below.

### Round 12 — the review of round 11: the two tables that were left

The review of round 11 found a BLOCKER and a MAJOR, and both were in the work
round 11 shipped rather than in what it left alone:

| # | Severity | What was wrong | What changed |
|---|---|---|---|
| 1 | BLOCKER | Round 11 replaced the **module** half of the completer's `builtins:` table with a read of `stdlib::MODULES` and left the flat half as a hand-written `vec!`. So the second table survived next to the first: it was short **21 names that answer** (`log` and `exp` after `tan`, `starts_with`, `ends_with`, `replace`, `append`, `shift`, `is_number`, `is_text`, `is_list`, `is_record`, `to_text`, `to_number`, `to_list`, `type_of`, `expect`, `assert`, `input`, `random_number`, `random_choice`, `random_shuffle`), and the gate round 11 added could not see it. `edge_the_repl_offers_no_builtin_a_program_cannot_call` walks the table and asks whether each entry is *real*, which is one direction; a name that answers and is **absent** is neither right nor wrong, so the gate stayed green through all of it. | `stdlib::flat_builtin_names()` (`src/stdlib.rs:807`) derives the flat half from `BUILTIN_NAMES` by dropping the `<module>_<member>` encodings, and `builtin_names()` (`src/repl/completer.rs:28`) chains it with the modules and `GLOBAL_CONSTANTS`. Two new gates walk the table the other way: `edge_every_flat_builtin_the_language_has_is_offered` (`src/repl/completer.rs:686`) and its public-surface twin `edge_every_flat_builtin_the_language_has_is_offered_by_the_repl` (`tests/repl_module_completion_test.rs:103`). A third pair covers the half of the registry that must **not** be offered: `edge_no_module_function_is_offered_under_its_registry_encoding` (`src/repl/completer.rs:716`) and `edge_a_module_function_is_offered_as_its_module_and_not_as_its_encoding` (`tests/repl_module_completion_test.rs:137`). |
| 2 | MAJOR | The `keywords:` table had drifted both ways against `lexer::KEYWORDS` and nothing in the suite compared the two. It offered eight words the lexer does not reserve and no program can contain — `are`, `when`, `ask`, `new`, `async`, `wait`, `parallel`, `done` — so `ne` was completed to `new` and `as` to `ask`; and it offered none of `constant`, `as`, `test`, `expect`, `give`, `back`, `might`, `fail`, which do start statements. This was FINDINGS.md §16, recorded in round 11 and **shipped**. | `keyword_names()` (`src/repl/completer.rs:89`) reads `lexer::KEYWORDS`, joining the words that share a token, and `edge_the_keyword_vocabulary_is_the_lexers_own_table` (`:344`) gates both directions — every keyword offered, nothing offered that the lexer does not reserve. The public-surface pair is `edge_every_keyword_the_lexer_reserves_is_offered_by_the_repl` and `edge_a_word_the_lexer_does_not_reserve_is_not_offered_as_a_keyword` (`tests/repl_module_completion_test.rs:185`, `:213`), the second of which also **runs** the eight words and is refused, so none of them could be reinstated from memory. |

**The two decisions §16 said a fixing phase would have to make.**

- **`give back` and `might fail` are phrases, and the join is derived rather than
  written down.** The lexer gives both words of each pair the *same* `TokenKind`,
  and the language writes both as the pair: `might f()` is refused by name
  (`Parser::parse_might_fail`, `src/parser.rs`) rather than read as a guard. So
  words sharing a kind are offered joined, which makes `might fail` arrive with
  `give back` rather than needing its own line — and
  `edge_a_two_word_keyword_is_offered_whole_from_its_first_word`
  (`src/repl/completer.rs:381`) is the gate that says the join happened for *both*
  pairs. The tail is not offered as a word of its own: completion is a prefix
  match, so `back` would only ever match after `give back` had been typed, and
  would complete it to a second `back`.
- **`by` stays, and it is the only word the lexer does not reserve that the completer
  offers.** `Parser::at_range_step_marker` (`src/parser.rs:954`) reads it
  positionally — reserving it would refuse `to can grow(by)` — but it still starts
  two clauses a program writes: `for each i from 0 to 10 by 5` and
  `split("a,b", by ",")`. It is named in `POSITIONAL_KEYWORDS`
  (`src/repl/completer.rs:57`) with that reason, and
  `edge_the_one_non_reserved_keyword_is_a_word_the_parser_reads` (`:409`) runs both
  programs, so the exception is a claim about the language that fails if the
  language stops agreeing.

**Measured, the same eleven prefixes before and after**, the round-11 tables
restored and then the fixed ones:

```
 prefix    before (round 11)         after (round 12)
 log       []                        ["log"]
 ex        ["export", "extends"]     ["exp", "expect", "export", "extends"]
 starts    []                        ["starts_with"]
 to_t      []                        ["to_text"]
 type_o    []                        ["type_of"]
 inp       []                        ["input"]
 expe      []                        ["expect"]
 as        ["ask", "async"]          ["as", "assert"]
 ne        ["network", "new"]        ["network"]
 mig       []                        ["might fail"]
 git       []                        ["give back"]
```

`as` and `ne` are the two rows worth reading twice: what was offered first was a
word no program can contain, and what is offered now is the keyword — and
`assert` beside it, which is a name the registry has held all along.

**Twelve gates, and each was watched failing on the defect it names.** Nothing was
weakened: no test deleted, nothing ignored, no lint allowed. Four mutations, each
undone from a copy taken first (`src/repl/completer.rs` is byte-identical to the
fixed file afterwards):

| Mutation | What failed |
|---|---|
| Two of the old keywords put back (`new`, `ask`) | `edge_the_keyword_vocabulary_is_the_lexers_own_table` → *"new is not a keyword the lexer reserves"*; `edge_a_word_the_lexer_does_not_reserve_is_not_offered_as_a_keyword` → *"ask is not a keyword the lexer reserves, so it is not offered: [\"ask\"]"* |
| The join broken for `back` and `fail` (the pre-round-12 shape) | `complete_matches_a_keyword`, `edge_a_half_of_a_two_word_keyword_is_not_offered_on_its_own`, `edge_a_two_word_keyword_is_offered_whole_from_its_first_word` and the public-surface `edge_a_two_word_keyword_is_offered_whole_rather_than_a_half_at_a_time` — four tests, and the parity gate correctly stayed green, because each half *is* a keyword |
| Two answering builtins dropped from the table (`type_of`, `input`) | `edge_every_flat_builtin_the_language_has_is_offered` and `edge_every_flat_builtin_the_language_has_is_offered_by_the_repl` → *"type_of is a builtin a program writes flat, so typing it is completed: []"* |
| Two registry encodings offered flat (`files_read`, `json_parse`) | `edge_no_module_function_is_offered_under_its_registry_encoding` and `edge_a_module_function_is_offered_as_its_module_and_not_as_its_encoding` → *"files_read is the registry's encoding of a module function, so typing it completes to nothing"* |

**This round reverses a decision round 11 recorded as deliberate**, and that is
worth saying plainly rather than editing quietly. Round 11's known-gaps list held
*"offering all 69 is a decision about how much a completion list should contain, not
a reconciliation, and the gate added here says which way it errs — never offer a
name that cannot be called, never guarantee to offer one that can."* The review was
right that this is the wrong way round: a name that answers and is not offered is a
name the language has and the REPL hides, and a completion is a promise in both
directions — the same table is trusted for what it offers as for what it does not.
The rule the completer now holds is **offer everything the language can name, and
nothing it cannot**, and each half has its own gate. The bullet is rewritten below.

**Round 12 changes what the REPL offers and nothing a `.rb` program can do.** No
line of the lexer, parser, analyzer, interpreter, either VM, `BUILTIN_NAMES`,
`MODULES`, `GLOBAL_CONSTANTS` or the registry changed. The two new `pub` functions
in `src/stdlib.rs` are reads over tables that already existed, and nothing in the
language asks them but the completer. `filter` and `reduce` are offered here as
they were in round 11 — registered builtins that answer nothing today, which is
FINDINGS.md §2's gap rather than a spelling one. **Round 13 takes that last
sentence back**: it is not a spelling question, it is the completion promise.

### Round 13 — the review of round 12: a name that is registered and answers nothing

| # | Severity | What was wrong | What changed |
|---|---|---|---|
| 1 | BLOCKER | `builtin_names()` offered `filter` and `reduce`. Both are in `stdlib::BUILTIN_NAMES`, so `is_builtin` is true, the analyzer lets a program write them and the parity gate beside the table — `edge_the_repl_offers_no_builtin_a_program_cannot_call`, asking `is_module ‖ is_builtin ‖ is_global` — said yes. Nothing dispatches either: `say filter([1, 2, 3], to (x) give back x > 1 end)` is `filter was called with arguments it cannot use` on both engines, and `tests/stdlib_module_docs_test.rs` carries them in `known_gaps()` for exactly that reason. So the completer was *inviting* a name the language refuses: a user who typed `filt` and pressed Tab was handed a call that cannot succeed. Round 12 wrote this down as "`filter` and `reduce` are offered here as they were in round 11 … FINDINGS.md §2's gap rather than a spelling one", and that was the review's point — the completer's rule is **offer every name the language can name and call, and nothing it cannot**, and a registered name that answers nothing is the second half of it. | `stdlib::UNIMPLEMENTED_BUILTINS` and `stdlib::is_unimplemented` (`src/stdlib.rs:789`) name the registered builtins nothing dispatches, and `flat_builtin_names()` drops them as well as the module-member encodings, so the offered list is derived rather than remembered. Four gates, below. |
| 2 | MAJOR | The other direction of the same promise is still open and is **recorded, not fixed here**: the 25 `<module>_<member>` encodings the completer deliberately withholds all *answer* when a program writes them flat — `say files_read("/etc/hostname")` prints the file and `say list_map([1, 2], f)` prints `[2, 3]`, through the flat path in `src/interpreter.rs:1986` / `src/bytecode/vm.rs:2735` with the analyzer accepting the name. So `flat_builtin_names()` is not quite "every name a program may write flat": the encodings may be written flat and are not offered. Rounds 11 and 12 both assumed the opposite without running it. | Not changed: refusing them is a decision about what the language answers for 25 registered names on both engines, which is the same kind of decision as round 5's and round 6's and belongs in its own round. It is FINDINGS.md §18, the doc comments that claimed the stronger thing now say what is true, and no gate was weakened to accommodate it. |

**Four gates, and each was watched failing on the defect it names.** Nothing was
weakened: no test deleted, nothing ignored, no lint allowed, and the round-12 gate
was *widened* (`is_builtin` → `flat_builtin_names`) rather than narrowed.

| Gate | What it asks, and what the old one could not |
|---|---|
| `edge_the_repl_offers_no_builtin_a_program_cannot_call` (`src/repl/completer.rs`) | Every offered builtin is in `flat_builtin_names()` — not merely registered — and is not in `UNIMPLEMENTED_BUILTINS`. The second half is written separately so the failure names the defect rather than the table. |
| `edge_no_unimplemented_builtin_is_offered_and_each_one_refuses` (`src/repl/completer.rs`) | Walks `UNIMPLEMENTED_BUILTINS` and, for each name, runs `say <name>([1, 2])` through the whole pipeline: not in `flat_builtin_names()`, not in the offered table, `complete(name)` empty, and the refusal names the builtin rather than claiming the name does not exist. A name that starts answering fails here, which is what forces it out of the list. |
| `edge_every_builtin_the_repl_offers_is_run_and_answers` (`tests/stdlib_module_docs_test.rs`) | The review's "extend the gate to run each offered builtin", run rather than asserted. It walks `complete("")`, keeps the names `BUILTIN_NAMES` holds, and for each one takes the row `answerable_builtins()` carries — the only table in this file that holds a program which *uses* a builtin and the output it must print — and runs it, pinning the output. Forty-two programs, in a scratch directory of their own because `cargo test` runs this beside the coverage gate. |
| `edge_the_gap_list_is_the_set_of_builtins_the_language_calls_unimplemented` (`tests/stdlib_module_docs_test.rs`) | The two lists that say a builtin answers nothing — `known_gaps()` here, `UNIMPLEMENTED_BUILTINS` in `src/stdlib.rs` — are the same set, in both directions. `UNIMPLEMENTED_BUILTINS` is what `flat_builtin_names()` reads, and a list that decides what the REPL offers cannot live only in a test file. |
| `edge_a_builtin_nothing_implements_is_not_offered_and_cannot_be_called` (`tests/repl_module_completion_test.rs`) | The same claim through the public surface: the completer completes nothing for the name **and** a program that writes it is refused by a message naming it. |

**Three mutations, each undone from a copy taken first** (`src/stdlib.rs` and
`tests/stdlib_module_docs_test.rs` are byte-identical to the fixed files
afterwards, and no `.rb` file is left in the repository root):

| Mutation | What failed |
|---|---|
| `flat_builtin_names()` stops dropping the gaps — the round-12 shape | `edge_the_repl_offers_no_builtin_a_program_cannot_call` → *"filter is registered and nothing implements it, so it is not a name to offer"*; `edge_no_unimplemented_builtin_is_offered_and_each_one_refuses` → *"filter is not a name a program may write flat"*; `edge_a_builtin_nothing_implements_is_not_offered_and_cannot_be_called` → *"filter is registered and nothing implements it, so it is not a name to offer: [\"filter\"]"*; `edge_every_builtin_the_repl_offers_is_run_and_answers` → *"filter is offered by the REPL and nothing here checks that a program can call it"* |
| `reduce` dropped from `UNIMPLEMENTED_BUILTINS` while staying unimplemented — the drift the two tables exist to catch | `edge_the_gap_list_is_the_set_of_builtins_the_language_calls_unimplemented` → *"reduce is in known_gaps() and not in stdlib::UNIMPLEMENTED_BUILTINS, so the REPL still offers it"*; `edge_every_builtin_the_repl_offers_is_run_and_answers` → the same no-row panic, on `reduce` |

**Round 13 changes what the REPL offers and nothing a `.rb` program can do.** The
two new `pub` items in `src/stdlib.rs` are a table of names and a predicate over
it; `BUILTIN_NAMES`, `MODULES`, `MODULE_MEMBERS`, `GLOBAL_CONSTANTS` and the
registry are untouched, and so is every engine. `filter` and `reduce` are still
registered and still refuse — the refusal `edge_a_known_gap_is_still_a_gap` runs
on every build is unchanged — and the REPL no longer offers them.

### Round 14 — the review of round 13: the encodings are refused, and the gates that asked the wrong question

| # | Severity | What was wrong | What changed |
|---|---|---|---|
| 1 | BLOCKER | The twenty-five `<module>_<member>` encodings the completer withholds all **answered** when a program wrote them flat — `say files_read("/etc/hostname")` printed the file and `say list_map([1, 2], f)` printed `[2, 3]`, through the flat path in `src/interpreter.rs` / `src/bytecode/vm.rs` with the analyzer accepting the name. So `stdlib::flat_builtin_names()` was not "every name a program may write flat": it was the flat half of the registry minus twenty-five spellings a program was free to write, and the REPL was hiding working names rather than keeping a promise. FINDINGS.md §18 recorded the two directions and took neither. | **The direction taken is the one §18 named first: refuse them where a program calls them without a receiver**, naming the spelling that works. `stdlib::encoding_written_without_a_receiver` decides the message; both engines ask it on the path a call takes *without* a receiver — `Expr::Call` and the `CALL` instruction — which is the only place the two can be told apart, because the dotted path builds the same string and hands it to the same resolver. `files.read(..)` answers and `files_read(..)` is `files_read is not a name on its own: it is how files.read is stored`, on either engine. |
| 2 | MAJOR | Both gates that run a gap ran the wrong program: `say filter([1, 2])`, one argument, where the function is missing. That is a refusal for its **arity**, and an implemented `filter` is refused in exactly the same words — so implementing `filter` without touching either gate left both green while the name answered and was still listed as a gap. | `stdlib::UNIMPLEMENTED_BUILTIN_PROGRAMS` holds the program each gap is refused by, in the shape a program writes it: `filter([1, 2, 3], to (x) give back x > 1 end)`. Both gates run it, `known_gaps()` in `tests/stdlib_module_docs_test.rs` is gated on being the same programs, and a new gate asserts the properties that made the old program useless — the call is to the name it is filed under, and it passes that name's arguments. |
| 3 | MAJOR | "Every keyword the lexer reserves is offered" was asked of the empty prefix by substring (`entry.split(' ').any(\|part\| part == *word)`), while the tail of each pair was pinned to complete to nothing. `back` and `fail` were therefore simultaneously "offered" and "not offered", and the gate passed whether the phrase was joined or not — the unjoined table satisfied it as well as the joined one. | The gate asks `complete` about each word, which is what a user does: a head completes to the phrase that carries it, and a tail is a **vocabulary member through that phrase** and is not a prefix to complete. The pair list it reads is a literal in the test rather than the lexer's declaration, so the two are held equal instead of agreeing by construction. |
| 4 | MINOR | The phrase join read adjacency: `phrases.last_mut()` merged a run of same-`TokenKind` entries, and nothing in `lexer::KEYWORDS` promises adjacency — alphabetising that table silently unjoined `give back` and `might fail`, with the count of multi-word entries as the only tripwire. | `lexer::KEYWORD_PHRASES` declares the pairs, in the order they are written, and `keyword_names()` joins by the declaration. `tests/keyword_reservation_test.rs` holds the declaration to `KEYWORDS` in **both** directions — a declared pair whose words are not keywords, or do not share one kind, fails; and so does an adjacent same-kind pair that is not declared, which is what a new two-token keyword is until somebody says which phrase it is written as. |

**Four gates, each watched failing on the defect it names.** Nothing was weakened:
no test deleted, nothing ignored, no lint allowed, and every gate round 13 wrote is
still here — two of them widened rather than replaced.

| Gate | What it asks, and what the old one could not |
|---|---|
| `edge_every_registered_builtin_outside_the_flat_table_is_refused_flat` (`src/repl/completer.rs`) | Walks every name `BUILTIN_NAMES` holds that `flat_builtin_names()` drops, and asks the language: the name is an encoding or a gap and nothing else — a *third* kind would make the table's name false without anything noticing — it is not offered, and a program calling it flat is refused by a message naming it, and an encoding's refusal names the dotted spelling that works. |
| `edge_the_dotted_spelling_of_an_encoding_still_reaches_the_function` (`src/repl/completer.rs`) | The half the refusal makes necessary. A refusal is only safe if it replaced a spelling that works, and the two paths are told apart by which instruction the program compiled to — a mistake there breaks the dotted spelling while every gate about the *offer* stays green, because tables do not know which path a call takes. |
| `edge_a_gap_program_is_the_call_the_language_documents_and_not_a_short_one` (`src/repl/completer.rs`) | The properties that make a gap's program falsifiable: the program is filed under a name in `UNIMPLEMENTED_BUILTINS` and vice versa, it calls that name, and it passes that name's arguments. A one-argument `filter` fails here with the reason in the message. |
| `edge_every_keyword_the_lexer_reserves_is_offered_by_the_repl` (`tests/repl_module_completion_test.rs`, rewritten) | Each keyword by its own prefix: a head completes to the phrase, a tail is in the vocabulary through the phrase and is offered neither as a word of its own nor as a prefix. Plus the equality between the test's pair list and `lexer::KEYWORD_PHRASES`. |
| `edge_a_module_function_is_offered_as_its_module_and_not_as_its_encoding` (`tests/repl_module_completion_test.rs`, widened) | The language half of finding 1, on **both engines**: the flat encoding is refused, the refusal names `module.member`, and `module.member()` still reaches the resolver rather than being turned away at the door. |
| `edge_a_declared_keyword_phrase_is_two_keywords_of_one_kind_and_every_pair_is_declared` (`tests/keyword_reservation_test.rs`) | The declaration against `KEYWORDS` in both directions, and the phrase against the documents' two-word list. |
| `edge_the_declared_phrases_are_the_spellings_the_language_writes` (`tests/keyword_reservation_test.rs`) | That a declared phrase is a phrase in the language and not only in the table: `might 1` is refused by name, and both words lex to the kind the guard arm wants. |

**Five mutations, each undone from a copy taken first.** No test was deleted and
no gate weakened to accommodate any of them.

| Mutation | What failed |
|---|---|
| The encoding refusal disabled in both engines — round 13's language | `edge_every_registered_builtin_outside_the_flat_table_is_refused_flat` → *"files_read is not offered, so a program writing it must be refused by a message naming it"*; `edge_a_module_function_is_offered_as_its_module_and_not_as_its_encoding` → *"files_read is not offered because a program may not write it flat, but the tree-walker answered it anyway"* |
| `UNIMPLEMENTED_BUILTIN_PROGRAMS` holding `say filter([1, 2])` — the round-13 shape | `edge_a_gap_program_is_the_call_the_language_documents_and_not_a_short_one` → *"calls filter with one argument, and one argument is refused for its arity rather than for the function being missing"*; `edge_the_gap_list_is_the_set_of_builtins_the_language_calls_unimplemented` → *"known_gaps() runs `…to (x) give back x > 1 end` for filter and the library runs a different program for it"* |
| `lexer::KEYWORD_PHRASES` emptied — the phrases lost | `edge_every_keyword_the_lexer_reserves_is_offered_by_the_repl` → *"the lexer no longer declares the language's two-word keywords as such"*; `edge_a_declared_keyword_phrase_…` → the `undeclared` assertion; `edge_a_two_word_keyword_is_offered_whole_rather_than_a_half_at_a_time` → *"`giv` completes to `["give"]`"* |
| `keyword_names()` stops joining — the completer unjoined | the same two keyword gates, the second with *"offers the whole phrase and not one word of it"* |

**Round 14 changes what the language answers, in one direction.** `say
files_read("x")` and `say list_map(xs, f)` were undocumented answers on both
engines and are now `RuntimeError`s naming the spelling that works; nothing that
answered stopped answering, `files.read(..)` and `list.map(..)` are unchanged on
both engines, and `edge_every_dropped_encoding_is_reachable_through_the_module_offered`
plus the dotted half of the widened encoding gate hold that. This is the same kind
of trade rounds 5 and 6 made — a refusal replacing an undocumented answer — and
`flat_builtin_names`'s name is now true of the language rather than only of the
REPL.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass, exit 0, no warnings |
| `cargo test --all-targets` | **1483 passed, 0 failed, 0 ignored** across 55 result lines |
| `cargo test --all-targets -- --list \| grep -c ": test$"` | 1483 — the same total, counted without running anything |
| `rb run` over `examples/*.rb` and `modules/*.rb` | 9 files, 0 failures |
| `rbops/verify.sh phase-050` | **not run** — there is no `rbops/` directory in this checkout (`ls: cannot access 'rbops': No such file or directory`), and the run instructions forbid spending a tool call on the pipeline that lives elsewhere. Recorded as FINDINGS.md §7, as phase-049 did. Nothing is claimed about it. |

**Round 13 re-measured it once more**, for the reason rounds 10 and 12 gave. 1479 +
4 is 1483 — `edge_no_unimplemented_builtin_is_offered_and_each_one_refuses` and
nothing else in `src/repl/completer.rs`, `edge_a_builtin_nothing_implements_is_not_
offered_and_cannot_be_called` in `tests/repl_module_completion_test.rs`, and
`edge_the_gap_list_is_the_set_of_builtins_the_language_calls_unimplemented` plus
`edge_every_builtin_the_repl_offers_is_run_and_answers` in
`tests/stdlib_module_docs_test.rs` — over the same 55 result lines. Both figures
come from the run above, once running and once from `--list`.

**Round 12 re-measured the table again**, for the reason round 10 gave: a gates
table carried forward is a number nobody has read. 1467 + 12 is 1479 — the seven
gates in `src/repl/completer.rs` and the five in
`tests/repl_module_completion_test.rs` — over the same 55 result lines round 11
measured. Both figures come from the run above, once running and once from `--list`.

**Round 11 re-measured the whole table rather than carrying it forward**, since
round 10's finding was a table whose numbers had not been re-read. 1463 + 4 is
1467: the two unit tests in `src/repl/completer.rs` and the two in
`tests/repl_module_completion_test.rs`, over one more result line — 53 test
binaries plus the library and the `rb` binary, which is what
`cargo test --all-targets` runs. Both numbers come from the run above, measured
once running and once from `--list`.

**The 1435 and 53 this table used to carry were wrong, and round 10 found it.** See
"Round 10 — a report that did not match its own gate" below: the figure was 28
short by exactly the size of `tests/wasm_test.rs`, and one result line short of the
52 test binaries plus the library and the `rb` binary that the command runs. The
corrected total is measured twice above, and `tests/stdlib_module_names_test.rs`
is 22 of it.

`git status` is clean after the full run apart from this phase's own files — no
program left a file at the repository root. Round 10 re-ran all nine
`examples/`/`modules/` files and re-checked that; round 11 did the same and added
the twenty programs it wrote under `target/tmp/`, which is ignored.

## Invariants touched

- No `.rb` extension, grammar, `Value` variant, `Error` variant, `say`,
  trailing-comma or `{interp}` change, in any round.
- Rounds 1 to 4 left the language behaving identically before and after.
- **Round 5 does change what the language answers**, in one direction only:
  `<module>.<member>` for a pair `stdlib::MODULE_MEMBERS` does not hold is now
  `RuntimeError: Module '<module>' has no function '<member>'` where it used to
  reach a flat builtin. That is a refusal replacing an undocumented answer, which
  is the same trade every previous round made in the other direction, and every
  spelling the documents give still answers — `every_module_the_documents_name_answers_a_call`
  and `the_text_and_math_examples_answer_what_the_spec_claims` are unchanged and
  still pass. Nothing that used to answer stopped answering except
  `console.input`, `math.random`, `json.uppercase` and the rest of that family,
  each of which SPEC.md denies in as many words.
- **Round 6 changes two more things, both in the same direction.** `list.filter`
  and `list.reduce` are refused by name rather than reaching a builtin that
  answers nothing, and the `<module>_<member>` registry path is behind the table
  on both engines — so a builtin registered under a stdlib module's prefix is no
  longer reachable by a dotted spelling the table does not hold. Neither changes
  an answer any document gives: the documents now write every member of every
  module, and `edge_a_member_the_documents_name_is_a_function_the_registry_has`
  plus `every_module_the_documents_name_answers_a_call` hold that.
- **Round 7 changes two more, and one of them is the first in seven rounds that
  puts an answer back.**
  - A dotted call whose receiver names no module is refused: `random.seed(1)`,
    `random.number(..)`, `random.choice(..)` and `random.shuffle(..)` were
    answering through `random_seed`, `random_number`, `random_choice` and
    `random_shuffle` and now are `RuntimeError: Module 'random' has no function
    '<member>'`. There is no `random` module, so nothing a document gives stopped
    answering; the flat spellings (`random_seed(1)`, `random(1, 6)`,
    `type_of(1)`, `ends_with(..)`) are asserted unchanged by the gate that carries
    the claim, because a fix that deleted the builtins would otherwise satisfy it.
  - A `module … end` the program declared now answers its own members: `module
    text … end` publishing `greet` was `Module 'text' has no function 'greet'` in
    both engines and is `hi ada`. This is the only round that turns a refusal into
    an answer, and it does so for a program `declare_module` always accepted.
  - One wording change falls out of the first and is worth naming: a receiver that
    is neither an object nor a module is now refused as `Module 'xs' has no
    function 'map'` where it was `Unknown function 'xs_map'`. That is the dotted
    spelling the program wrote rather than the internal encoding, which is what
    `edge_an_unknown_module_name_is_refused_in_its_dotted_spelling` requires of
    `nosuchmodule`; the program it names is `xs.map(f)`, which does not answer
    either way and is recorded at FINDINGS.md §14.
  - `MODULE_MEMBERS`, `MODULES`, `ALIAS_MODULES`, `BUILTIN_NAMES` and the registry
    are unchanged in round 7. Only the rule that reads them moved, and it moved
    into `stdlib::dotted_call_reaches`, which both engines call.
- **Round 8 changed no behaviour at all.** As of round 8 itself no line of
  `src/interpreter.rs`, `src/bytecode/vm.rs` or `src/runtime.rs` was touched;
  `src/stdlib.rs` gained a comment correction and nothing else. What round 8
  removed was a **promise**: § list no longer says `xs.map(f)` answers, and it now
  says which spelling does — `[1, 2, 3].map(f)`, a literal receiver, which is what
  the interpreter had always done. The refusal itself was unchanged from round 7
  (`Module 'xs' has no function 'map'`) and was written down in the document that
  used to deny it. A program that copies § list today runs; a program that copied
  it yesterday died on the third line.
- **Round 9 changes the bytecode VM's answer, and it is the second round in nine to
  put one back.** `map([1, 2, 3], f)` and `list.map([1, 2, 3], f)`, with `f` a
  named `to` function, were `RuntimeError: map was called with arguments it cannot
  use` / `list_map was called with arguments it cannot use` on the bytecode VM and
  are now `[2, 4, 6]`, matching the tree-walker. The tree-walker's answers are
  unchanged, and the three names round 8 established still hold: a variable
  receiver (`xs.map(f)`) is refused in both engines as `Module 'xs' has no
  function 'map'`, and so is every other expression receiver on the bytecode VM.
  - The cause was ordering, not representation: the VM asked the shared resolver
    before it could answer the call itself, and the resolver claims `map` as a
    registered builtin it cannot apply. The tree-walker already excluded those two
    names ahead of its own resolver call and the VM now does the same, in
    `call_named` and in `call_method` for a literal receiver.
  - **The call-depth budget is part of the fix, not a detail of it.** The map is
    carried on the frame that started it (`Frame::pending_map`, `start_map`,
    `advance_map`) rather than by nested `drive` runs, so each element is one
    ordinary `call_function` that charges and releases the depth. A `map` nesting
    inside the function it calls therefore reaches `MAX_CALL_DEPTH` as a
    `Limit` error on both engines, instead of putting one Rust stack frame on the
    host per level and overflowing the thread on a program the tree-walker runs.
    `edge_a_higher_order_call_answers_the_same_way_on_both_engines` carries that
    case (`a map of a map`) on both engines precisely because it is the one that
    would crash rather than disagree.
  - A frame that is taken away from under a running map (`break`, `try`,
    `might fail`) gives the map up with itself, in `discard_frames_above` and in
    `unwind_frame` — otherwise the guarded expression would be handed one element
    of a list instead of the failure the program asked to catch. The gate carries
    both directions (`a guard takes a failure raised inside an element`, and the
    mapped function writing to the program's output).
- **Round 10 changed nothing the interpreter can observe.** No line under `src/`
  and no line under `tests/` changed: the round corrected a number in this
  document, and `git status` carries the two files it touched and nothing else.
  The two mutations it ran were undone from copies taken before the edit, and
  `src/bytecode/vm.rs` is byte-identical to `HEAD`.
- **Round 11 changes what the REPL offers and nothing a `.rb` program can do.** No
  line of the lexer, parser, analyzer, interpreter or either VM changed, so the
  language's answers are identical before and after — `stdlib::MODULES`,
  `BUILTIN_NAMES`, `GLOBAL_CONSTANTS` and the registry are untouched. What changed
  is a suggestion: `formats`, `parse_json`, `to_json`, `read`, `write` and `exists`
  are no longer offered by tab-completion (a program naming any of them was already
  refused, and still is), and `bytes`, `csv`, `json`, `sys` and `time` are now
  offered, all five of which answered before this round and still do. Nothing that
  used to complete stopped completing except six names that could only ever produce
  `Unknown variable` or `Unknown function`.
- **Round 12 changes what the REPL offers, in the other direction, and nothing a
  `.rb` program can do.** The same files are untouched: no line of the lexer,
  parser, analyzer, interpreter or either VM, and `MODULES`, `BUILTIN_NAMES`,
  `GLOBAL_CONSTANTS` and the registry are unchanged. The two `pub` functions added
  to `src/stdlib.rs` are reads over tables that already existed, and the completer
  is their only caller. What changed is the other half of the promise: 21 builtins
  that answered are now offered (`log`, `to_text`, `type_of`, `input`,
  `starts_with`, `random_number` and seventeen more), the eight words no program
  can contain are no longer offered, six keywords that begin statements are, and
  `might fail` is offered whole beside `give back`.
- **Round 13 also changes what the REPL offers and nothing a `.rb` program can
  do**, in the same direction as round 12 and for the same reason: `filter` and
  `reduce` stop being offered, because neither is a name a call can succeed with.
  `BUILTIN_NAMES`, `MODULES`, `MODULE_MEMBERS`, `GLOBAL_CONSTANTS` and the
  registry are unchanged, no engine line changed, and `say filter(..)` is still
  `filter was called with arguments it cannot use` on both — which is what
  `edge_a_known_gap_is_still_a_gap` still runs on every build. The two new `pub`
  items are a table of names and a predicate over it, and the only caller of
  either is `flat_builtin_names()` and the gates that read it.

## Test-requirement matrix

| Row | Status |
|---|---|
| empty | covered — `bytes.text(bytes.from_text(""))` answers empty text; `sys.argv()` outside `rb run`; `time.unix("")` is not a date and is refused |
| singleton | covered — `bytes.from_text("H")` is `[72]`, one byte |
| boundary | covered — bytes `0` and `255` round-trip and are written to a file read back as `[0, 255]` |
| out_of_bounds | covered — `bytes.text([256])` and `bytes.text([-1])` are refusals naming the value, not wrapped values |
| type_mismatch | covered — `files.read(1)`, `math.sqrt("a")`, `text.split("a", 1)`, `bytes.from_text(1)`; each of those three is run a second time inside `try … catch error`, so the refusal is also proved to be catchable rather than fatal; and round 7 added the other direction — `length({a: 1})` is refused in all three of its spellings while a text and a list answer, which is the `type_mismatch` the drifted SPEC sentence claimed did not exist |
| numeric_boundary | covered — the byte range is the numeric boundary this module has: 0, 255, 256, −1, 300; `time.format(0, "%Y")` is `1970` |
| unicode | covered — `bytes.from_text("é")` is `[195, 169]` and back; `length("héllo")` is 6 in both arms |
| nesting_recursion | N/A — no new nesting or recursion; the documents' only higher-order builtin (`list.map`) is covered by the too-many-arguments case |
| duplicate_missing_keys | N/A — no record key handling changed; no module takes a record |
| malformed_input | covered — `bytes.text([0, 255])` is bytes in range that are not UTF-8, refused by name; `time.unix("")` is not a date and is refused; a bare `nosuchmodule` receiver is refused in the spelling the program wrote; and round 8 added the document-side half — a `name.member(` a document writes on a receiver that is no module at all is read back out of the document, so a promise no program can keep is a failing gate rather than a reader's lost afternoon |
| unknown_member | covered (round 5; widened in round 6; **the other side of the predicate in round 7**) — every `module.member` pair of 11 modules and 64 short names that **neither document writes** is refused by its dotted spelling in both engines: 646 pairs, including `console.input`, `math.random`, `json.uppercase` (each of which SPEC.md denies), `json.read`, `text.get` and `files.parse` (the short suffixes the old qualified-name walk never asked about), and `list.filter` (round 6). A name the parser reserves (`bytes.expect`) does not parse at all, which is asserted rather than skipped, and the three counters are asserted to add up to the size of the matrix. Round 7 added the **receiver** half: every `<name>_<member>` in the registry whose `<name>` is not a module — 14 pairs, a source neither the table nor the documents can skew — is refused in both engines, with the four `random` pairs named and the flat spellings behind the prefix asserted to still answer. It also walks all 11 stdlib module names as a program-declared `module … end`, which is the collision the matrix could not see |
| resource_limit | N/A — this phase adds no recursion, no loop, no allocation and no limit; the existing call-depth and step budgets are untouched and unchanged. The loopback server round 3 adds is bounded by a 30-second accept deadline and a 10-second read deadline, and holds at most two exchanges. Rounds 4, 5, 6 and 7 add none, round 8's gate runs three one-line programs with a four-line preamble, and round 11 adds a table walk over at most 69 names — no I/O, no thread, no clock |
| process_control | N/A — nothing signals or exits; `network` is the only module that starts anything, and its transport is a loopback listener owned by the test |
| engine_parity | covered — `edge_every_spelling_the_spec_promises_for_map_answers` walks **both** engines and `edge_a_higher_order_call_answers_the_same_way_on_both_engines` (`tests/differential_test.rs`) compares the two **outcomes** for eleven cases, so a fix that made the bytecode VM agree by refusing what the tree-walker answers fails the gate rather than passing one that ran a subset. This row was `partly N/A` in round 8 and is not any more: round 9 closed the defect round 8 recorded at FINDINGS.md §15 instead of enshrining it. `map([1, 2, 3], f)` with `f` a named `to` function was `map was called with arguments it cannot use` on the bytecode VM, because the shared resolver claimed the name before the engine could; the VM now answers `map`/`list_map` itself before the resolver (`src/bytecode/vm.rs`, `call_named`), carries the map on the frame that started it (`start_map`/`advance_map`), and answers a literal receiver in `call_method`. The one limitation §15 still records is real and still open: the bytecode compiler refuses a closure *expression* (`set f to to (x) … end`), so the gates use a **named** declaration, which both engines compile. Nothing is skipped and no gate was narrowed to reach this |
| stale_suggestion (round 11) | covered — the new row for what this round changed. Two directions, both asserted: a module the language has and nothing offered (`bytes`, `csv`, `json`, `sys`, `time`), and a name offered that nothing answers (`formats`, `parse_json`, `to_json`, `read`, `write`, `exists` — each of which is *also* run and refused, so the two tables are checked against each other and not against a copy). **Round 12 closed the third direction round 11's gate could not see**: a name the language has and nothing offers. `stdlib::flat_builtin_names()` (44 flat names) and `GLOBAL_CONSTANTS` are walked and every one must complete, and the 25 `<module>_<member>` encodings must not — with the keyword table now read from `lexer::KEYWORDS` and gated both ways, the completer has no hand-kept vocabulary left in it |

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
  did. Neither has agreed semantics in SPEC.md, and round 6 took them out of
  `MODULE_MEMBERS` for the same reason `math.random` is not a member → FINDINGS.md
  §4.
- Four of SPEC.md's sections are now executed as they are written — § console,
  and round 6 added § text, § math, § list and § json and csv. `### files` and
  `### bytes` are still not, because their examples write to paths a test cannot
  promise: they are covered by `examples/files.rb` (gate 4 runs it) and by
  `edge_the_bytes_module_answers_a_round_trip_at_both_ends`, which is coverage of
  the functions rather than of the document. Running the remaining sections
  verbatim needs a scratch directory the examples agree to; that is not done.
- `ROADMAP.md` still writes `formats.parse_json` → FINDINGS.md §5.
- `HANDOUT.md` is worse than ROADMAP.md and out of scope: it writes
  `formats.parse_json`, `ask "Your name?"` and — the one this round caused —
  `list.filter([1, 2, 3, 4], to (x) give back x mod 2 is 0)`, which was already a
  builtin that answers nothing and is now refused by name. It is not one of the
  three documents this phase reconciles, and it was already wrong about two
  modules and a keyword before round 6 touched it.
- `catch error` binds nothing. There is no `catch error as e`, so a program can
  tell *that* a module call failed and can carry on, but cannot read the message
  and cannot choose to handle one error and re-raise another. That is why
  `edge_a_bad_module_call_is_a_catchable_error_and_not_an_abort` pins catchability
  in one test and the error's wording in another: the second property is not
  reachable from inside the program. → FINDINGS.md §8.
- `xs.map(f)` was in SPEC.md § list and did not answer. The **document** half is
  fixed: § list names `[1, 2, 3].map(f)` as the third spelling, says the receiver
  is taken literally, and writes the refusal down, with two gates holding both
  halves of that. The **semantics** half — making a list *value* answer `map` on a
  variable receiver — is untouched and still open, because it is a decision about
  which methods a value has rather than a reconciliation →
  FINDINGS.md §14.
- The two engines disagreed about a higher-order call: `map([1, 2, 3], f)` with `f`
  a named `to` function answered on the tree-walker and was
  `map was called with arguments it cannot use` on the bytecode VM. Found in round 8
  while writing §14's gate and **fixed in round 9** — the VM answers `map`/`list_map`
  itself, before the shared resolver can claim the name, and carries the map on the
  frame that started it, so the two engines charge the same call-depth budget →
  FINDINGS.md §15. What is still open is the *other* half of §15: a closure
  **expression** (`set f to to (x) give back x * 2 end`) does not compile for the
  bytecode VM at all, so every gate that needs a function value uses a named
  declaration.
- A dotted call's receiver must now name a module, which is the one rule in this
  report that is enforced from `src/stdlib.rs` rather than written twice. The two
  engines had drifted onto two different halves of it, and the drift was the
  BLOCKER → FINDINGS.md §12.
- **The completer's keyword table had drifted the same way its builtin table had**,
  and round 11 deliberately left it: it offered `ask`, `new`, `async`, `done` and
  `when`, none of which start anything, and did not offer `constant`, `might`,
  `fail`, `as`, `test` or `expect`. It was recorded with the two decisions a
  fixing phase would have to make (`give back` is two keywords, `by` is read
  positionally) → FINDINGS.md §16, and **fixed in round 12**: the table is read
  from `lexer::KEYWORDS`, the pairs are joined by the rule that shares a token,
  `by` is the one named exception with a gate that runs the two programs that use
  it, and `edge_the_keyword_vocabulary_is_the_lexers_own_table` fails if the two
  drift apart again. What is still open is the language, not the completer: `ask`
  is still an ordinary identifier and nothing reads input (FINDINGS §3).
- The completer's flat builtins were 23 of the 69 in `stdlib::BUILTIN_NAMES`, so
  `starts_with` or `replace` completed to nothing even though they answer — the
  list was hand-kept "on purpose" in round 11, on the grounds that offering all 69
  is a decision about how much a completion list should contain. **Round 12
  reverses that**: `flat_builtin_names()` derives the flat half of the registry and
  `edge_every_flat_builtin_the_language_has_is_offered` walks it in the other
  direction, so a name the language has cannot go unlisted again. **Round 13
  closes the last half**: the two of the 44 that answered nothing — `filter` and
  `reduce` — are not offered either, so all 42 offered flat builtins answer and the
  two that do not are gone from the list. `flat_builtin_names()` now filters on
  "answers" as well as on "spelled flat", and the two names it drops for that
  reason live in `stdlib::UNIMPLEMENTED_BUILTINS`, which
  `edge_the_gap_list_is_the_set_of_builtins_the_language_calls_unimplemented` holds
  equal to the test file's `known_gaps()` in both directions.
- **The reverse half of the same promise is open, and is FINDINGS.md §18.** The 25
  `<module>_<member>` encodings the completer deliberately withholds — `files_read`,
  `list_map`, `json_parse` — **all answer** when a program writes them flat:
  `say files_read("/etc/hostname")` prints the file, `say list_map([1, 2], f)`
  prints `[2, 3]`, through the flat path on both engines with the analyzer
  accepting the name. So `flat_builtin_names()` is the flat half of the registry
  *minus the spellings no document uses*, not quite "every name a program may
  write flat", and the doc comments now say that rather than the stronger thing.
  Rounds 11 and 12 both assumed the encodings were unreachable flat and neither
  ran it. Settling it is a decision about what the language answers for 25 names,
  which is round 5's and round 6's kind of change and not a review round's.
- The REPL offers a name and nothing after it: completing `files` gives the module
  and not `read`, `write` or `append`. The member surface is `MODULE_MEMBERS` and
  it is gated where the language uses it, but no gate carries it to the completer,
  and this round did not add one — the defect is an absent feature (dotted
  completion) rather than a wrong name, so nothing here promises it.
