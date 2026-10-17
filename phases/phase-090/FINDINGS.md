# phase-090 — FINDINGS

Work that belongs to a later phase. This phase shipped a document and changed
nothing under `src/`, per AGENTS.md rule 7.

## F1 — the dispatched finding cites a line that has moved

The evidence block for phase-090 says:

> `src/stdlib.rs:612` states outright 'Redblue has no regex, so this is a literal separator'

`src/stdlib.rs:612` is `"uppercase" => {`. The comment is at
`src/stdlib.rs:637`:

```
637:                // Redblue has no regex, so this is a literal separator.
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

`SURVEY.md` cites **637**, and `tests/stdlib_gap_survey_test.rs` reads the cited
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

prints nothing and exits **0**. `src/interpreter.rs:1671` binds the loop variable
only inside `if let Value::List(items)`, and every other value falls out of the
match arm into `Ok(Value::Nothing)`:

```
1671:                if let Value::List(items) = iterable_value {
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

## F4 — gate 4 cannot be run from this checkout

`./rbops/verify.sh` does not exist here:

```
$ ./rbops/verify.sh phase-090
bash: ./rbops/verify.sh: No such file or directory
```

There is no `rbops/` directory in the project at all (the dispatcher lives outside
this checkout). Gates 1-3 were run directly and are in `REPORT.md`; gate 4 was
attempted and its failure is reported rather than papered over. Nothing else about
this phase is blocked by it — `.github/workflows/ci.yml` shards every
`tests/*.rs` target, so `stdlib_gap_survey_test` runs in CI without anything in
`rbops/` naming it.
