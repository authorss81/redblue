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
invoked. Confirmed in round 4: this checkout has no `rbops/` directory at all
(`ls: cannot access 'rbops': No such file or directory`). The gates that can be
run from inside the checkout were, and their real output is in `REPORT.md`:
`cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --all-targets` (1427 passed, 0 failed), and `rb run` over every file
in `examples/` and `modules/` (9 files, 0 failures). `REPORT.md` claims nothing
about `verify.sh`.

## 8. `catch error` binds nothing, so a program cannot read what went wrong

`try … catch error` runs the handler and hands it no value. `catch error as e` is
a `ParserError: Unexpected token 'as'`. A program can therefore see that a module
call failed and carry on, but it cannot put the failure message in its own
output, log it, or decide between two `catch` clauses by error type.

This is not this phase's to fix — `SPEC.md` § Error handling documents the
language's `try`/`catch`/`finally`, and changing what `catch` binds is a
grammar and semantics question, not a module-name reconciliation. It is recorded
because round 4 had to work around it: the "caught `RuntimeError` naming the
function" requirement is split across two tests, because only half of it is
expressible inside a Redblue program. `edge_a_bad_module_call_is_a_catchable_error_and_not_an_abort`
proves the error is catchable; `edge_a_module_function_refuses_bad_arguments_by_name`
reads the name out of the process's stderr instead of out of the program.


## 9. A module was a namespace for every flat builtin, and the table did not know

Round 5's review found a BLOCKER and three smaller things, and the BLOCKER is
the most general defect this phase has produced, so the shape is recorded here
rather than only in `REPORT.md`.

`stdlib::module_member_name` unwrapped `module_member` to `member` whenever
`module` was in `MODULES` **and** `member` was any registered builtin:

```rust
if MODULES.contains(&module) && BUILTIN_NAMES.contains(&member) {
```

Every one of those conditions is necessary and neither is sufficient, so every
module became a namespace for the whole registry. Verified against the tree the
previous round left, before the fix:

```
$ rb run say-console-input.rb      # say console.input("n")
name?                    # it read a line
$ rb run say-math-random.rb       # random_seed(1) / say math.random(1, 6)
3
$ rb run say-json-uppercase.rb    # say json.uppercase("hi")
HI
```

Each of those contradicts SPEC.md in as many words — `there is no
console.input` (`SPEC.md:1024`), `math.random is not implemented`
(`SPEC.md:1064`), and "a module has the functions that are spelled
`<module>_<member>` in the registry" (`SPEC.md:1010`) — and none of them was
visible to a test, because the table the per-member gate walks
(`stdlib::MODULE_MEMBERS`) lists what a module *has*, and a spelling that is not
listed is exactly a spelling no gate was asking about.

Three points generalize:

- **An enumerated surface that is not enforced is documentation.** The table was
  introduced to make the check per-member rather than per-module, and it did that
  — but it was read, never applied. The same shape as the `KNOWN_GAPS` skip list
  in §4: the names the test does not look at are the names that rot.
- **A resolution rule that accepts "any of A and any of B" is a namespace.** The
  rule was written to make `text.uppercase` reach `uppercase`, and the narrow
  version of it — the pairs the language actually has — is no harder to state.
- **A test that builds a name by concatenation tests the concatenation.**
  `tests/text_split_join_test.rs` wrote its dotted spelling as `{module}.{builtin}`
  and reached `json.json_parse(..)`, which answered only through this same hole.
  The row was green on a spelling no reader would write.

The fix is one predicate (`stdlib::module_has_member`) used by both the resolver
and the test, and two new gates: every unlisted pair is refused by its dotted
spelling, in both engines.

## 10. The gate's own allowlist was derived from the thing it was checking

Round 6's review found the BLOCKER this section records a second time, one level
up, and the shape is worth its own entry because §4, §5 and §9 are all the same
shape.

The matrix built for §9 walked every `module.builtin` pair and skipped a pair
when `stdlib::member_resolves(module, builtin)` said the pair resolves — and
`member_resolves` is derived from `MODULE_MEMBERS`, the table the matrix exists to
audit. So the one hole the table could open was the one hole the matrix could not
see: put `uppercase` in `csv`'s row and `csv.uppercase` answered, *and* the
matrix skipped `csv.uppercase`, and the suite was green. The guard under the loop
(`checked > BUILTIN_NAMES.len()`) asked whether there was anything left to
refuse, not whether the right things were being refused, so a table that granted
everything still left hundreds of pairs to check and the guard passed.

Two generalizations, and they are the reason this is recorded rather than left in
`REPORT.md`:

- **An allowlist in a gate has to come from somewhere the gate is not auditing.**
  The skip list is now the `module.member` pairs the documents' code blocks
  write, which is a third source: the table, the registry and the documents can
  each disagree with the others, and a gate derived from one of them cannot catch
  that one drifting. This is §4's `KNOWN_GAPS` again — a name nobody checks — and
  it has a harder form here, because the skip list was not a list of names but a
  *predicate*, which reads like a fact about the language rather than like a
  list somebody wrote.
- **A count is not a coverage claim.** The counters are now three and they are
  asserted to add up to the size of the matrix, so a fourth column — one more
  `continue` — fails rather than being absorbed, and the guard is
  `refused > promised_rows`: the allowlist has to stay the smaller half of what
  the gate walks.

The same review found that the matrix was asking about the wrong names. It walked
`stdlib::BUILTIN_NAMES`, which holds *qualified* names (`files_read`,
`json_parse`, `network_get`), so it asked whether `files.files_read` — a spelling
no reader writes — was refused, and never asked about `json.read`, `text.get` or
`files.parse`. A test of a closed surface has to enumerate the surface a caller
would write, not the surface the registry stores. That one is the general form of
§9's third bullet: a name built by concatenating the storage with the syntax is a
name the language never had.

## 11. A table that the resolver consults is not the same as a table the tests consult

Also from round 6, and smaller but the same family.

`stdlib::MODULE_MEMBERS` was documented as "the predicate both engines resolve a
dotted call against", and both engines consulted it on **one** of the two paths a
dotted call takes: the flat-backed one (`text.uppercase` unwraps to `uppercase`),
but not the `<module>_<member>` one, which asked `get_var("files_read")` first. So
`files_read` being registered was on its own a reason `files.read` answered, and
the claim in the doc comment was false for half the resolution.

The general shape is the one §9 records — an enumerated surface that is read but
not applied is documentation — and the detail worth keeping is where the check
went. It went **before** the `get_var` hit rather than beside it, and it was
scoped to `stdlib::is_module(name)` rather than `is_module_name(name)`, because
the latter is also true for a `module … end` the program declared, whose members
are published by the program and about which the stdlib table says nothing. A
check written against the broader predicate would have broken every user module.

## 12. A namespace is a decision, and the receiver is where the decision was missing

Round 7's BLOCKER and its MAJOR are one defect seen from two sides, and it is the
third round in a row that has found the *same* class in a new place — which is why
this is recorded rather than only fixed.

Round 6 closed "a module is a namespace for every flat builtin" by making
`stdlib::MODULE_MEMBERS` the predicate. Round 7's review showed the predicate was
being asked about the **member** and never about the **receiver**. Both engines
resolved a dotted call in this order:

```rust
if stdlib::is_module(&module) && !stdlib::module_has_member(&module, method) { … }
if self.get_var(&qualified).is_none() && !answered_by_builtin && self.is_module_name(&module) { … }
```

Every condition in those two lines is *necessary* and none is *sufficient*, so a
name that is not a module at all fell straight through to `call(&qualified)`:

```
$ rb run /tmp/opencode/p50r3/r1.rb      # say random.seed(1)
nothing
exit=0
```

`random` is a registered builtin, so `src/analyzer.rs` accepts it as a receiver —
correctly, since `PI.times(..)` needs exactly that — and `random_seed` is in the
registry, so the first refusal did not fire and the second did not either. There is
no `random` module: `stdlib::MODULES` does not list it and neither document names
it. `random.number`, `random.choice` and `random.shuffle` answered the same way, in
both engines (`rb compile` + `rb vm` printed `nothing` and `[2, 3, 1]`).

The other side of the same defect was the mirror image: the table **was** asked
about a receiver it says nothing about. `module text … end` publishing `greet` is
`Module 'text' has no function 'greet'`, and all eleven names in `MODULES` refused
the same way. `src/interpreter.rs:1056` only rejects a *second* declaration of a
name, so the program had declared a legal module and the runtime had disagreed.

Three points generalize, and they are the third recurrence of one lesson:

- **A gate walks the universe it was built for.** The refusal matrix walks
  `stdlib::MODULES` × short member names, so it asked 646 questions about eleven
  receivers and none about the other side of the `MODULES` predicate. The new gate
  walks the *registry's* prefixes instead — a source neither the table nor the
  documents can skew — which is FINDINGS §10's "an allowlist in a gate has to come
  from somewhere the gate is not auditing", applied to the loop rather than to the
  skip list. Fourteen pairs, of which four were live.
- **Two predicates that both answer "is this a namespace?" are not one predicate.**
  `stdlib::is_module` asks the crate's list and `is_module_name` asked the
  program's; the round-6 check was written against the crate's and the round-6
  fall-through against the program's, and between them every receiver that was
  neither was undefined. `stdlib::dotted_call_reaches` is now the single place all
  three questions are asked, in order, for both engines.
- **A rule written in two engines is a rule the next change applies to one of them.**
  `is_module_name` was already duplicated in `src/interpreter.rs` and
  `src/bytecode/vm.rs` with a comment each way. The new rule is a free function in
  `src/stdlib.rs` with both engines as callers, so the "both engines refuse the
  same pairs" claim is structural rather than tested-into-agreement.

## 13. `length` was documented as answering three kinds of value and answers two

Round 7's second MAJOR, and the smallest drift in this phase's history because
every other layer already had it right.

`SPEC.md:1056` said "`length` answers a text, a list or a record alike".
`src/runtime.rs` and `src/stdlib.rs` both refuse a record — `length requires a
list or text` — and `tests/test_text.rb:132` pins that as a **caught** error while
`corpus/stdlib-0005` and `corpus/stdlib-0006` record it. Four layers against one
sentence, and the sentence was the one nothing could check: no test runs prose.

The sentence is corrected, and the new gate
`edge_length_answers_a_text_or_a_list_and_refuses_a_record` walks all three
spellings against the runtime **and** reads § text out of SPEC.md to assert it
says the refusal. A document claim written only in prose needs its own gate; that
is §4's `KNOWN_GAPS` and §10's skip list in a third place.

## 14. `xs.map(f)` is in SPEC.md and does not answer

Out of scope, recorded so it is promoted rather than smuggled in. Found while
proving the BLOCKER above, and it is **pre-existing** — it predates every round of
this phase and is not a consequence of anything here.

`SPEC.md:1147` says `list.map` "works in all three spellings: `map(xs, f)`,
`list.map(xs, f)` and `xs.map(f)`". The third does not answer:

```
$ rb run /tmp/opencode/p50r3/m1.rb      # set ys to xs.map(to (x) give back x * 2 end)
Error: RuntimeError: Module 'xs' has no function 'map'
$ rb run /tmp/opencode/p50r3/m2.rb      # say [1,2,3].map(to (x) give back x * 2 end)
[2, 4, 6]
```

A *literal* list receiver works because `call_method` evaluates a non-name
receiver and handles `map` there; a receiver that is a variable takes the
name path, which is the module path, and a variable is not a module. Making it
answer is a semantics decision rather than a documentation fix — it says which
methods a list *value* has, and `xs` has to be in scope for the example to be
runnable at all — so it belongs in its own phase, with a documented rule, rather
than in a reconciliation one.

The wording did change, and for the better: the refusal now names the receiver the
program wrote (`Module 'xs' has no function 'map'`) instead of the internal
encoding (`Unknown function 'xs_map'`), which is the same thing
`edge_an_unknown_module_name_is_refused_in_its_dotted_spelling` requires of a
receiver that is no module at all. Nothing in the suite pinned the old wording.
