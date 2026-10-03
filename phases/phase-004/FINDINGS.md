# Phase 004 — FINDINGS

Out-of-scope work discovered while fixing deterministic record ordering. Not done here
(AGENTS.md rule 7: one phase, one concern). Each item is anchored to a line that was read.

## 1. `rbops/` and `phases/` are missing from this checkout — the fourth gate cannot run

```
$ ls rbops
ls: cannot access 'rbops': No such file or directory
$ ls phases
ls: cannot access 'phases': No such file or directory
$ find . -name verify.sh -not -path './target/*'
(no output)
```

`rbops/verify.sh`, `rbops/phases.json`, `rbops/dispatch.sh` and `phases/` are all absent,
even though AGENTS.md and the phase prompt reference them as the sources of truth. The
phase directory `phases/phase-004/` was created by hand to hold `REPORT.md`.

Impact: `./rbops/verify.sh phase-004` could not be run. AGENTS.md section 3.4 makes it a
required gate. Substitutes run and reported in `REPORT.md`: the full `examples/` +
`modules/` + `tests/` sweep via `rb run`, `rb test` on both Redblue test files, `rb lint`,
and the exact CI clippy command.

**Needed:** either restore `rbops/` into the checkout, or correct the prompt to stop
citing a gate that does not exist. As it stands a phase cannot honestly claim gate 4.

## 2. `modules/MathUtils.rb` does not parse

```
$ ./target/debug/rb run modules/MathUtils.rb
Error: ParserError: Expected function name      # exit 1
```

Confirmed pre-existing: identical error and exit code on the pre-phase tree (source files
stashed, rebuilt). The file uses `constant PI to 3.14159` (line 3) and
`constant TAU to 6.28318` (line 5) at top level. AGENTS.md section 2 names
`modules/*.rb` as part of the language's specification-by-example, so a module file that
does not parse is a real gap — but it is a parser/`constant` feature question, nothing to
do with record ordering.

**Needed:** a phase for top-level `constant` declarations (or for whatever the intended
module syntax is), with the error anchored at the `constant` parse site in `src/parser.rs`.

## 3. `Value::Object` is dead

`grep -rn "Value::Object(" src/ tests/` returns five hits, all of them *matches*, none
constructions:

- `src/value.rs:52` — `Display` arm
- `src/value.rs:66` — `is_truthy` arm
- `src/testing/runner.rs:295` — `assert_type` arm
- `src/vm.rs:889` — `type_name` arm
- `src/vm.rs:1076` — `json_stringify` arm

Meanwhile `src/vm.rs:270-278`, `Statement::Object`, builds
`Value::Record(Fields::new())` — an empty *record*, discarding the `name`, `extends` and
`body` it was given. So `object Foo ... end` produces a plain empty record, and nothing in
the language can ever produce a `Value::Object`. The phase prompt asked to "preserve
insertion order for records **and objects**"; the object half is untestable until objects
are constructed.

**Needed:** a phase implementing `Statement::Object` against `SPEC.md`'s object section
(SPEC.md:459 `say "I'm {this.name}"`, SPEC.md:484 `set this.name to name`). Ordering for
objects comes for free once the payload is `Fields`.

## 4. `stdlib::builtins()` is still a `HashMap`

`src/stdlib.rs:4` — `pub fn builtins() -> HashMap<String, Value>`.

Not user-visible today (nothing iterates it into output), and out of scope for an
ordering-of-records phase, so it was deliberately left as `HashMap`. If any future builtin
iterates the global namespace — `dir()`, autocompletion in the REPL, or a bootstrap
compiler dumping its symbol table — it will inherit the same nondeterminism this phase just
removed from records.

**Needed:** fold into whichever phase first iterates the builtin table, or a small
housekeeping phase converting it to the same `Fields` alias.

## 5. Non-determinism is not asserted anywhere else

`edge_twenty_key_record_is_identical_across_fifty_runs` runs the pipeline 50× in one
process. That catches per-process seed variation (the original bug), but not a
second-order source: two *different processes* on the same machine. The original
reproduction needed 6 separate `rb run` invocations to show 3 distinct orderings.

**Needed:** a CLI-level test that shells out to the built binary N times and diffs
stdout. That needs a test that locates `env!("CARGO_BIN_EXE_rb")`, which no test in
`tests/` currently does.
