# Phase 012 — FINDINGS

Work that was found while doing phase 012 but is **not** this phase's concern.
Per AGENTS.md hard rule 7 these are recorded here for the auditor to promote
into real phases.

## 1. `new Person` does not parse, so an instance cannot carry its type

**Where:** `src/lexer.rs:61` lexes `new` as `TokenKind::New`, and
`src/parser.rs::parse_primary` has no arm for it, so
`set p to new Person` is a `ParserError`. `src/value.rs:136`
`Value::Object(String, Fields)` is therefore still never constructed by a
program — the only way to reach it is Rust code.

**Evidence.**

```
$ cat target/tmp/new_person.rb
object Person
    has name
end
set p to new Person
$ ./target/debug/rb run target/tmp/new_person.rb
Error: ParserError: Unexpected token New
  --> target/tmp/new_person.rb:3:14
3 | set p to new Person
  |              ^
```

This is why dispatch in this phase is by receiver *name*: the declaration binds
a plain `Record` (pre-existing tests pin `type_of(Person)` to `"record"`), and a
copy of it (`set copy to Person`) is a record with no type left in it.

```
$ cat target/tmp/f2.rb
object P
    to can hi()
        give back 1
    end
end
set copy to P
say copy.hi()
$ ./target/debug/rb run target/tmp/f2.rb
Error: RuntimeError: Unknown function 'copy_hi'
```

**Suggested phase:** "`new` instantiates an object; `Value::Object` is the
instance". That is the change that moves dispatch from a name to a value and
lets a field hold an object that still answers its methods.

## 2. A method cannot be called on a value, only on a name

**Where:** `src/vm.rs::call_method`. The receiver must be `Expr::Variable`, so
`Counter.bump()` dispatches and `copies[0].bump()` does not:

```
object P
end
set a to [P]
say a[0].hi()
```

`Error::Runtime("Cannot call method 'hi' on …, which is not an object")`. This
resolves itself once item 1 lands — a `Value::Object` receiver knows its own
type. Pinned as `edge_method_call_on_a_field_is_an_error` so the current
behaviour is at least deliberate and testable.

## 3. `has name: number` (a typed field) does not parse

**Where:** `docs/GRAMMAR.md:271` `property_decl = 'has' identifier [ ':'
type_expression ] [ 'default' expression ]`.

```
$ cat target/tmp/f3.rb
object P
    has n: number
end
$ ./target/debug/rb run target/tmp/f3.rb
Error: ParserError: Unexpected token Colon
  --> target/tmp/f3.rb:2:10
2 |     has n: number
  |          ^
```

**Suggested phase:** "typed fields". Until then a field holds whatever the
default is, and nothing checks a write against it — `set P.n to "text"` on a
field declared `has n default 1` is accepted.

## 4. The formatter is not idempotent for 20 of 21 example and test files

**Where:** `src/formatter.rs`. `rb format examples/time.rb` drops every comment
and emits `set now  to time . now ( )` — spaces around `.` and `(`.

This is pre-existing and unchanged by phase 012: the set of files
`rb format --check` rejects is identical before and after this phase
(20 files, measured by stashing the diff and re-running the check over
`examples/*.rb tests/*.rb modules/*.rb`).

**Suggested phase:** "formatter round-trips its own output". A formatter that
drops comments makes `rb format` lossy, which is a correctness problem for a
tool users are told to run.

## 5. `modules/MathUtils.rb` does not parse

```
$ ./target/debug/rb run modules/MathUtils.rb
Error: ParserError: Expected function name
  --> modules/MathUtils.rb:4:16
4 |     constant PI to 3.14159
  |                ^
```

`constant` (SPEC.md § Modules) is not implemented. Pre-existing; the gate
baselines it as a known failure and this phase does not touch it.

## 6. `give back` still does not return from inside a block

Phase 011 recorded this (`phases/phase-011/FINDINGS.md` § 1). It shapes the
object model: a method whose body is `if … give back … end` keeps running, so
`give back this` from a conditional constructor is not yet usable. Unchanged
here.

## 7. `rbops/verify.sh` looks for the report outside the phase workspace

**Where:** the gate resolves the report path against the pipeline repository —
`/home/runner/work/rbops/rbops/phases/phase-012/REPORT.md` — while a phase's
working directory is the language checkout,
`/home/runner/work/rbops/rbops/redblue`.

**Evidence.**

```
$ ../rbops/verify.sh phase-012
  FAIL /home/runner/work/rbops/rbops/phases/phase-012/REPORT.md missing —
  a phase without a report cannot pass
...
  ╔══════════════════════════════════════╗
  ║ VERIFY FAIL — phase-012                ║
  ║ 1 check(s) failed                    ║
  ╚══════════════════════════════════════╝
```

Every other check in that run passes. The report and these findings are written
to `phases/phase-012/` in the checkout, which is where every previous phase's
pair is committed from and where the pipeline's copy comes from:

```
$ diff -q ../phases/phase-011/REPORT.md phases/phase-011/REPORT.md   # identical
$ git log --oneline -1 -- phases/phase-011/REPORT.md
8e892de rbops: phase-011 (2026-10-04T10:26:40Z)
```

This run is instructed to read and write only inside its working directory, so
it cannot create the copy the gate reads. If a phase is ever expected to pass
`verify.sh` from inside the workspace, the gate should fall back to
`<project-root>/phases/<phase>/REPORT.md`.

**Suggested fix:** one line in the gate — try the pipeline path, then the
checkout path.
