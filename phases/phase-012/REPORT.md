# Phase 012 — Object model: inheritance and method dispatch

## Decision

The object model is a **declaration-based prototype**, not a set of instances.

| Question | Decision | Why |
|---|---|---|
| What does a declaration bind? | A record of its **resolved** fields, so the declaration is the type and its prototype instance at once | `tests/test_objects.rb` already pins `type_of(Person)` to `"record"` and `Person` to `{}`, and that may not be re-scoped. `new` does not parse at all (FINDINGS § 1), so there is nothing to instantiate from. |
| Lookup order | Nearest declaration first: the child's own `has`/`to can`, then the parent's, then the grandparent's. First declaration of a name wins, later ones are not copied in | "Shadow" is what an inheriting language means by it. Merging two defaults for one name would have no answer. |
| When is a chain resolved? | Once, at declaration, into the child's own table | A lookup is then a map read, so a lookup cannot walk a cycle. |
| Cycle | `object A extends A` is reported; a two-object cycle is unwriteable, because the parent must be declared first and re-declaring a name is refused | Acyclic by construction, with the `seen` set as the guard that catches the one-object case. |
| `receiver.method(args)` | A method call when the receiver names a declared object, with `this` bound for the call; otherwise the module function `receiver_method`, which is what `files.read` and `json.parse` already were | One expression, two meanings, decided where the type is known. Rewriting the AST to `receiver_method` for modules is what made method dispatch impossible. |
| Missing method | `RuntimeError`, never a `nothing` | A missing *field* reads as `nothing`; a missing *method* is a mistake and says so. |

Documented in SPEC.md § Objects, "Lookup order and shadowing".

## Reproduction

The finding reproduces on `main` (`8e892de`). `src/vm.rs` handled
`Statement::Object` by discarding `extends` and `body` and binding an empty
`Value::Record`. Three failures, all reproduced before any edit:

```
$ cat target/tmp/repro012.rb
object Person
    has name
    to can greet()
    end
end
say Person
$ ./target/debug/rb run target/tmp/repro012.rb
Error: ParserError: Unexpected token Has
  --> target/tmp/repro012.rb:2:5
2 |     has name
  |     ^

$ ./target/debug/rb run target/tmp/repro2.rb      # to can greet()
Error: ParserError: Expected function name
  --> target/tmp/repro2.rb:2:8

$ cat target/tmp/repro3.rb
object A extends A
end
say "ok"
$ ./target/debug/rb run target/tmp/repro3.rb
ok                                        # a cycle in the parent chain, accepted
```

## What changed

| File | Lines | What |
|---|---|---|
| src/vm.rs | +202 −20 | `ObjectType` table (`objects`), `declare_object` (parent resolution, shadowing, cycle and re-declaration refusal), `call_method` (dispatch, `this`, module fallback), `Statement::Has`/`Statement::Method` handling, `Expr::MethodCall` evaluation, `call` now takes evaluated args |
| src/parser.rs | +114 −13 | `Statement::Has`, `Expr::MethodCall`, `parse_has`, `parse_method`, `parse_callable` (the tail shared with `parse_function`), `has`/`to can` recognised inside an `object` body, `this` as a primary and as a `set` target |
| src/analyzer.rs | +25 −0 | `Statement::Has` arm; `Expr::MethodCall` arm that treats a bare receiver as a module rather than an unknown variable |
| src/formatter.rs | +25 −0 | `Statement::Has` and `Expr::MethodCall` arms, so `has name default 1` and `receiver.method(a)` round-trip |
| src/linter.rs | +15 −0 | `Statement::Has` and `Expr::MethodCall` arms |
| src/stdlib.rs | +10 −0 | `MODULES` / `is_module`, the list the analyzer needs to tell `json.parse` from a variable |
| SPEC.md | +38 −0 | § Objects, "Lookup order and shadowing": the table, the four derived rules, and `this` writes |
| tests/object_model_test.rs | +441 | 26 `#[test]` functions |
| tests/test_object_model.rb | +140 | 10 Redblue `test` blocks |

## Definition of done

| Requirement | Where |
|---|---|
| Inheritance resolves method lookup in a defined order | `declare_object` walks the chain nearest-first and merges with `entry().or_insert_with`, so the child's method wins and the grandparent's is still there. Pinned by `object_child_method_overrides_parent_method`, `object_grandparent_fields_and_methods_are_reachable` |
| Field shadowing defined and documented | Same rule for fields; SPEC.md § Objects table. Pinned by `object_inherits_parent_fields_and_shadows_them`, `object_field_default_and_shadowed_default`, `object_field_and_method_of_the_same_name` |
| Extending a frozen/nested object tested | `object_writes_do_not_leak_into_a_later_child` (a written prototype does not become a child's default — the "frozen" case), `object_nested_declaration_extends_its_own_scope` (a declaration inside a function body, and a nested declaration extending it) |
| Cycle in the parent chain → clean error, no infinite loop | The chain is walked once at declaration with every visited name collected. `edge_self_extending_object_is_a_reported_cycle` (a real one-object cycle), `edge_mutually_extending_objects_are_refused_not_looped`, `edge_deep_parent_chain_resolves` (200 levels, no recursion in Rust) |

## Tests added

| Test | Edge class covered |
|---|---|
| `object_inherits_parent_fields_and_shadows_them` | duplicate/shadowed keys |
| `object_field_default_and_shadowed_default` | duplicate keys, defaults |
| `object_method_dispatch_binds_this` | method dispatch, `this` |
| `object_method_writes_to_this_and_gives_it_back` | mutation through `this`; the prototype is not written |
| `object_child_method_overrides_parent_method` | lookup order |
| `object_field_and_method_of_the_same_name` | field vs method namespace |
| `object_writes_do_not_leak_into_a_later_child` | frozen/written prototype |
| `object_nested_declaration_extends_its_own_scope` | nesting |
| `object_grandparent_fields_and_methods_are_reachable` | nesting, 3 levels |
| `object_with_no_fields_and_no_methods_is_an_empty_record` | empty |
| `object_single_field_round_trips` | singleton |
| `object_duplicate_field_declaration_keeps_the_last_default` | duplicate keys |
| `object_field_named_like_a_module_does_not_shadow_it` | name collision with the stdlib |
| `object_named_like_a_module_takes_the_call` | name collision, precedence |
| `object_unicode_fields_defaults_and_methods` | unicode (emoji, CJK, RTL mark, combining mark) |
| `object_empty_text_default_is_a_field` | empty text vs missing field |
| `edge_object_field_of_nothing_is_not_zero` | type mismatch |
| `edge_object_number_field_is_not_a_list` | type mismatch |
| `edge_object_method_that_does_not_exist_is_an_error` | **asserts a failure** |
| `edge_method_call_on_a_field_is_an_error` | **asserts a failure** |
| `edge_this_outside_a_method_is_an_error` | **asserts a failure** |
| `edge_self_extending_object_is_a_reported_cycle` | **asserts a failure**; cycle |
| `edge_mutually_extending_objects_are_refused_not_looped` | **asserts a failure**; cycle, no hang |
| `edge_deep_parent_chain_resolves` | resource limit (200-level chain) |
| `edge_mutually_recursive_methods_hit_the_call_depth_limit` | resource limit, recursion between two types |
| `edge_malformed_object_declarations_are_parse_errors` | malformed input (`has` with no name, `to can` with no name, unterminated `object`) |
| 10 Redblue blocks in `tests/test_object_model.rb` | the same rules at the language level, two of them `try … catch error` |

Failure-asserting tests: 5 in Rust, 2 in Redblue. `edge_*` tests: 10 in Rust.

### Edge-case matrix

| Row | Covered / N/A |
|---|---|
| empty | `object_with_no_fields_and_no_methods_is_an_empty_record`, `object_empty_text_default_is_a_field` |
| singleton | `object_single_field_round_trips` |
| boundary | `object_duplicate_field_declaration_keeps_the_last_default` (last default kept), `object_field_and_method_of_the_same_name` (both namespaces) |
| out_of_bounds | **N/A** — the model adds no indexable position. A field reads as `nothing` when absent (`object_inherits_parent_fields_and_shadows_them` pins that), and index bounds are already covered by `tests/index_bounds_test.rs` |
| type_mismatch | `edge_object_field_of_nothing_is_not_zero`, `edge_object_number_field_is_not_a_list`, `edge_method_call_on_a_field_is_an_error` |
| numeric_boundary | **N/A** — a field stores a `Value` and the object model computes nothing. `2^53±1`, `0/0` and `±Infinity` are already covered by `tests/numeric_edge_test.rs`, and a non-finite number cannot enter a field because every computed number goes through `Value::number` |
| unicode | `object_unicode_fields_defaults_and_methods` |
| nesting_recursion | `object_grandparent_fields_and_methods_are_reachable`, `object_nested_declaration_extends_its_own_scope`, `edge_mutually_recursive_methods_hit_the_call_depth_limit` |
| duplicate_missing_keys | `object_duplicate_field_declaration_keeps_the_last_default`, `object_inherits_parent_fields_and_shadows_them`, `object_field_named_like_a_module_does_not_shadow_it` |
| malformed_input | `edge_malformed_object_declarations_are_parse_errors` |
| resource_limit | `edge_deep_parent_chain_resolves` (chain walk), `edge_mutually_recursive_methods_hit_the_call_depth_limit` (call depth) |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 223 passed, 0 failed (16 binaries) |
| `rb test` (Redblue suite) | 229 passed, 0 failed |
| `rbops/verify.sh phase-012` | **fail**, and only on its own report check: `FAIL /home/runner/work/rbops/rbops/phases/phase-012/REPORT.md missing`. Every other check passed — diff hygiene (+1010 −33), `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test` (223 passed, 0 failed), doc tests, the example sweep (`modules/MathUtils.rb` baselined as a pre-existing failure), test policy (26 rust / 10 redblue, 15 edge, 2 failure-asserting, none skipped). See "Where the gate looks for this file". |

### Where the gate looks for this file

`rbops/verify.sh` resolves the report path against the **pipeline** repository,
`/home/runner/work/rbops/rbops/phases/phase-012/REPORT.md`, while this phase's
working directory is the language checkout,
`/home/runner/work/rbops/rbops/redblue`. Every previous phase has the report in
*both* places, byte-identical, and the copy in the checkout is what the
`rbops: phase-011` commit contains — so the pipeline copies it there at the end
of a phase. This run is instructed to read and write only inside its working
directory, so the report is written where every previous phase's report is
committed from:

```
$ diff -q ../phases/phase-011/REPORT.md phases/phase-011/REPORT.md
(identical)
$ git log --oneline -1 -- phases/phase-011/REPORT.md
8e892de rbops: phase-011 (2026-10-04T10:26:40Z)
```

Both files this phase produces are at `phases/phase-012/` in the checkout:
`REPORT.md` and `FINDINGS.md`. Nothing else in the gate is red.

## Invariants touched

- **None.** `.rb`, `to … end`, `set x to …`, `say`, the `Value` variants, the
  `Error` variants, trailing commas and `{interp}` are all unchanged. No
  existing test was weakened, deleted, re-scoped, ignored or skipped, and
  `tests/test_objects.rb` still passes untouched.
- `Expr::Call { name }` for `receiver.function(...)` is replaced by
  `Expr::MethodCall { receiver, method, args }` in the AST. `Expr` is a public
  type, but no program behaviour changes: for a receiver that names no object
  the call still resolves to `receiver_method`, and every module test in
  `tests/test_modules.rb` and `tests/suite.rb` passes unmodified. The formatter
  now prints `files.read(path)` where it used to print `files_read(path)`, which
  is the source form and re-parses to the same call.
- `set this.field to …` now parses. `this` is an ordinary variable that only a
  method call binds, so `this` outside a method is still
  `RuntimeError("Unknown variable 'this'")`.
- A declared object takes precedence over a stdlib module of the same name for
  `receiver.method(...)`: `object json … end` makes `json.parse` the object's
  method. Pinned by `object_named_like_a_module_takes_the_call`.

## Known gaps / follow-ups

- `new Person` does not parse, so dispatch is by receiver name and a copy of an
  object loses its type → FINDINGS § 1, § 2
- Typed fields (`has n: number`) do not parse and no write is checked against a
  declared default → FINDINGS § 3
- The formatter drops comments and is not idempotent (pre-existing, unchanged) →
  FINDINGS § 4
- `give back` inside a block still does not return (phase-011 finding), so a
  conditional constructor is not yet expressible → FINDINGS § 6
