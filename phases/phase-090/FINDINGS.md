# phase-090 — FINDINGS

Work that belongs to a later phase. This phase shipped a document and changed
nothing under `src/`, per AGENTS.md rule 7.

## F1 — the dispatched finding cites a line that has moved

The evidence block for phase-090 says:

> `src/stdlib.rs:612` states outright 'Redblue has no regex, so this is a literal separator'

On the current tree `src/stdlib.rs:612` is `},`. The comment is at
`src/stdlib.rs:651`:

```
651:                // Redblue has no regex, so this is a literal separator.
```

Every other part of the finding reproduces on `main` and was re-verified:

```
$ rb run probe.rb        # set m to regex.match("id=42", "id=([0-9]+)")
Error: AnalyzerError: Unknown variable 'regex'
$ rb run probe.rb        # set later to time.add_days(time.now(), 7)
Error: RuntimeError: Module 'time' has no function 'add_days'
$ rb run probe.rb        # say files.glob("corpus/*.rb")
Error: RuntimeError: Module 'files' has no function 'glob'
```

`SURVEY.md` cites **651**, and `tests/stdlib_gap_survey_test.rs` reads the cited
line and fails unless it contains `no regex`, so the drift cannot silently return.
No phase needed: the citation was corrected where it is used.

## F2 — `for each` over a record iterates nothing, silently

Rank 3 of `SURVEY.md` had to be shown with a program that does not fail, which is
itself the defect:

```
set r to json.parse("{\"a\": 1, \"b\": 2}")
for each key in r
  say key
end
```

prints nothing and exits **0**. `src/interpreter.rs:1844` binds the loop variable
only inside `if let Value::List(items)`, and every other value falls out of the
match arm into `Ok(Value::Nothing)`:

```
1844:                if let Value::List(items) = iterable_value {
```

A `for each` over a text, a number or a record is therefore a program that runs to
completion having done nothing. `length` at least refuses the same record with
`src/runtime.rs:815`. This is a **silent wrong answer**, which is a worse class of
bug than a refusal and is not what a gap-ranking phase should fix.

Suggested phase: *refuse a non-list in `for each` the way `length` refuses a
record.* Both engines (`src/interpreter.rs`, `src/bytecode/vm.rs`) and the
formatter would need to agree, and the corpus would need the refusal pinned. Until
it lands, `SURVEY.md` rank 3 is correct to describe the gap and its "Note" records
the failure mode.

## F3 — the record literal documented in SPEC.md does not parse

`SPEC.md:112` shows

```redblue
set person to record {
    name: "Alice",
    age: 30
}
```

and `docs/GRAMMAR.md:99` carries the production
`record_literal = 'record' '{' [ property { ',' property } ] '}'`. Neither runs:

```
$ rb run rl.rb
Error: ParserError: Expected field name
  --> rl.rb:1:23
1 | set person to record {
```

and the type declaration at `SPEC.md:193` reaches the analyzer as two ordinary
identifiers (`Unknown variable 'record'`, `Unknown variable 'number'`). A program
can therefore only build a record through `json.parse`, which makes the F2 defect
much worse in practice: there is no literal to iterate and no literal to name.

This is out of scope here for two reasons — it is a parser change, and it is a
spec-vs-implementation question that the reviewer should rule on rather than a gap
survey (is the document wrong, or is the parser wrong?). Suggested phase: *either
implement `record_literal` or correct `SPEC.md` and `docs/GRAMMAR.md` to match.*
Whichever way it goes, `tests/stdlib_module_names_test.rs` already extracts module
names from both documents and has the machinery to do it for this production.

## F4 — gate 4 lives beside the checkout, not inside it

`./rbops/verify.sh` does not exist in the project checkout, but the resumed
pipeline supplies it one directory up as `../rbops/verify.sh`, and that is the
form the resumed prompt and this round use. The earlier attempt tried only the
in-checkout path and correctly reported the `No such file or directory`; that is
no longer a blocker. The result of the actual gate is recorded in `REPORT.md`.

## F5 — the audit of unreviewed work had been destroyed by a prior resume run

`c95062eb02a6e336f011a918daac7938f0efee6d`, whose subject is
`ack: resume phase-088: add resume commit to ACKNOWLEDGED`, replaced
`tests/unreviewed_work_ack_test.rs` with the single line
`<the file content updated>` (`git show --numstat c95062e` → `1  927`). Thirteen
tests, twelve of them `edge_*`, including the audit that is the only thing in the
repository that reads `git log HEAD`, were gone. `222aa83` replaced the invalid
Rust with a seven-line placeholder whose only assertion was `assert!(true, ..)`,
which fails both gates this phase is measured by:

```
$ cargo fmt --all -- --check
Diff in .../tests/unreviewed_work_ack_test.rs:3:  ... FMT_EXIT=1
$ cargo clippy --all-targets -- -D warnings
error: this assertion is always `true`      (clippy::assertions_on_constants)
```

The previous phase-090 attempt restored the file from `e008f0b` (927 lines) and
added an entry for its own delivery commit, `3c2085b7…`. That attempt was parked,
and `main` then landed phase-084, which rewrote the audit into its current
1,930-line form: the history repair is serialised, the delivery commit is
acknowledged by its **subject** rather than a SHA, and the phase-061/062 resumes
and `c95062eb` are acknowledged. The resumed merge therefore could not take
either side wholesale.

The resolution keeps the newer `main` audit and merges the older attempt's
intent:

- `373513e4` (phase-061), `0647108e` (phase-062) and `c95062eb` stay
  acknowledged — they are on `HEAD` and dropping them would make the audit
  report them as unreviewed work.
- `c95062eb`'s reason carries both halves: it is the phase-088 acknowledgement
  commit, and it truncated this file to `<the file content updated>` (+1/−927).
- `3c2085b7` is **not** listed: it is the parked attempt's delivery commit and
  is not an ancestor of `HEAD`, so a SHA-pinned entry for it would fail the
  stale check on this tree. Its intent is preserved instead by extending
  `DELIVERY_SUBJECT_PREFIXES` to `resume phase-090:`, which acknowledges this
  phase's rebuilt delivery commit by subject.

The content was restored rather than the history rewritten: AGENTS.md §1.6 forbids
rewriting published commits, and erasing them would make the audit green by
destroying what it exists to record. The broader defect — a resume tool that
writes a placeholder over a file instead of appending to it — is not a Redblue
bug and is out of this phase's reach, so it is recorded here rather than fixed.

## F6 — `cargo clippy --all-targets` was already red on `main`

`tests/memory_model_test.rs:22` used `Arc<Node>` where `Node` holds a `RefCell`,
so `Node` is `!Sync` and the value is not `Send`:

```
error: usage of an `Arc` that is not `Send` and `Sync`
  --> tests/memory_model_test.rs:22:5
```

That file is byte-identical to `main`, so the lint predates this phase. CI runs
`cargo clippy -- -D warnings` (no `--all-targets`), so CI does not see it; the
phase's own method block asks for `--all-targets`, so this phase did. `Node` and
its cycle now use `Rc`, which is what the value always was — constructed and
dropped inside one test, never sent to a thread — and `Arc` is kept for the
`Value` itself, where the test is pinning what `src/value.rs` uses. Same
assertions, same strong counts, one less lint.

Suggested phase: none needed for the lint. The thing worth auditing is the CI
command: `cargo clippy -- -D warnings` lints no test target, so a lint in any of
the 66 files under `tests/` cannot fail the build today.

## F7 — `{nothing}` interpolates as the word `nothing`, silently

Found while writing the malformed-input cases for this phase:

```redblue
say "a{nothing}b"
```

prints `anothingb` and the statement's value is `Nothing`. `Value::Nothing` has no
display of its own, so the interpolation path renders its debug name instead of
refusing. Every other interpolation of a non-text value is a type error; this one
is a silent wrong answer, which is the F2 class.

This is an interpreter change, so it is not this phase's work. Suggested phase:
*refuse `{nothing}` in an interpolation, or render it as `nothing` deliberately
and document that choice in `SPEC.md`* — a reviewer has to rule which, because
both are defensible and they are not the same behaviour.
