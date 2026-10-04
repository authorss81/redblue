# Phase 007 — Numeric edge semantics

## Reproduction

The finding reproduces, and the first half of it was worse than described. On
this commit, before any change:

```
$ printf 'say 5 %% 0\n' > repro.rb && ./target/debug/rb run repro.rb
NaN
$ echo $?
0

$ printf 'say 1e308 * 1e308\n' > inf.rb && ./target/debug/rb run inf.rb
inf

$ printf 'set y to 1e400\nsay y\n' > lit.rb && ./target/debug/rb run lit.rb
inf

$ printf 'say 99999999999999999999\n' > big.rb && ./target/debug/rb run big.rb
9223372036854775807
```

Four defects, all reachable from ordinary source:

1. `5 % 0` stored a NaN in a `number` and exited 0. `1 / 0` was already an
   error (`src/vm.rs:567`), so the two operators disagreed.
2. `1e308 * 1e308` stored an infinity. `say` printed the bare token `inf`
   (`Value::Display` fell through to `write!(f, "{}", n)`), which is not a
   Redblue literal — reading it back gives *text*.
3. A literal too large to hold (`1e400`) became an infinity at `Expr::Number`.
4. `say 99999999999999999999` printed `9223372036854775807`. `Display` cast
   through `i64`, which **saturates**, so a `f64` of 1e20 was reported as
   `i64::MAX` — a whole number the program never held.

`json.parse("1e400")` and `random_number(-1e308, 1e308)` were the same hole by
other routes (`src/vm.rs:1163`, `src/vm.rs:1034`).

## What changed

| File | Lines | What |
|---|---|---|
| `src/value.rs` | +62 −3 | `MAX_EXACT_INT` (2^53), `non_finite_display`, `non_finite_name`, `Value::number` (the one door that refuses a non-finite number), `Display` for `Number` |
| `src/vm.rs` | +12 −7 | `Mod` by zero is `Modulo by zero`; `+ - * /`, `Expr::Number`, `parse_json`, `random_number` all go through `Value::number` |
| `src/stdlib.rs` | +11 −2 | `sqrt` of a negative number answers `nothing`, not NaN; `builtin_function` documents what `None` and `Nothing` mean |
| `SPEC.md` | +30 | "Numeric semantics": the table of outcomes and the three consequences |
| `tests/numeric_edge_test.rs` | +355 new | 16 Rust tests |
| `tests/test_numeric_edges.rb` | +77 new | 8 Redblue blocks |
| `tests/test_arithmetic.rb` | +13 −5 | one block rewritten, see "Invariants touched" |

### The policy

`Value::Number` is a public `f64`, so it cannot be made unrepresentable without
changing the variant, which is a fixed invariant. Instead every value the VM
*computes* goes through one constructor:

```rust
pub fn number(n: f64, span: Span) -> Result<Value>   // src/value.rs:105
```

which returns `Error::Runtime` for NaN and ±infinity. `Display` still defines
the three non-finite values, because a Rust embedder can still construct one
directly; it prints `not a number`, `infinity`, `negative infinity`.

## Tests added

16 `#[test]` functions in `tests/numeric_edge_test.rs`, 8 Redblue blocks in
`tests/test_numeric_edges.rb`, 1 Redblue block rewritten in
`tests/test_arithmetic.rb`.

| Test | Edge class covered |
|---|---|
| `modulo_by_zero_is_a_runtime_error` | the red test: `5 % 0` used to return `nothing`, now `Modulo by zero` |
| `edge_modulo_by_a_zero_divisor_of_every_shape_is_rejected` | empty/zero (`0 % 0`), boundary (`5 % 0.5`, `5 % -0.0`), numeric (`-5 % 3`, `5 % -3`, `0 % 7`) |
| `edge_division_by_zero_still_reports_an_error_for_every_numerator` | zero (`1/0`, `0/0`, `-1/0`, `1/-0.0`), boundary (`7/2`, `6/3`) |
| `edge_overflow_to_a_non_finite_number_is_a_runtime_error` | numeric boundary: `*`, `+`, `-` overflow to ±infinity; `1e308 * 1`, `1e308 + 1e307` still fit |
| `edge_random_number_refuses_a_range_whose_width_overflows` | numeric boundary; asserts a failure; a range wider than a double is refused whatever the clock draw is |
| `edge_a_number_literal_out_of_range_is_rejected` | malformed numeric input: `1e400`, `-1e400` refused; `f64::MAX` accepted |
| `edge_json_numbers_out_of_range_are_rejected` | malformed input: `json.parse("1e400")`; `json.parse("42")` untouched |
| `edge_negative_zero_equals_zero_and_prints_as_zero` | numeric boundary: `-0.0` is `0`, `0 * -1 is 0` is `yes`, `-0.0` as a divisor is zero |
| `edge_a_number_wider_than_2_53_is_not_printed_as_another_integer` | precision: 2^53 exact, 2^53+1 rounded, 1e20 no longer `i64::MAX` |
| `edge_non_finite_numbers_have_a_defined_display` | display of every non-finite value; `f64::MAX`/`MIN`/`MIN_POSITIVE` are not mistaken for one |
| `edge_json_never_sees_a_non_finite_number` | json output for 1e20 and 1.5; the language cannot hand it a non-finite value |
| `nan_is_rejected_by_the_only_door_into_value_number` | asserts a failure; `Value::number` rejects all three, accepts 0, −0, ±1, `MAX`, `MIN`, `MIN_POSITIVE` |
| `nan_never_compares_equal_so_it_cannot_be_tested_for` | asserts a failure; NaN ≠ NaN, and all five routes to a NaN from source are closed |
| `edge_sqrt_of_a_negative_number_has_no_answer` | `sqrt(-1)` → `nothing`; `sqrt(4)` → 2, `sqrt(0)` → 0 |
| `edge_non_finite_results_are_refused_inside_a_list_and_record` | nesting: inside a list literal, a record literal, and an index expression |
| `edge_an_index_at_the_numeric_limit_is_nothing_not_a_panic` | out_of_bounds: index `0`, `-1`, `len`, `999`, `±1e308` on a 3-list and on an empty list |
| `edge_modulo by zero is a catchable runtime error` (Redblue) | `try`/`catch` proves the failure is produced |
| `edge_overflow to infinity is a catchable runtime error` (Redblue) | ditto |
| `edge_a number literal out of range is refused` (Redblue) | ditto |
| `arithmetic: modulo keeps the sign of the dividend` (Redblue) | numeric boundary |
| `edge_integers are exact up to 2^53 and rounded past it` (Redblue) | precision |
| `edge_negative zero equals zero` (Redblue) | numeric boundary |
| `numeric: a number wider than i64 is still a number` (Redblue) | precision |

## Edge-case matrix

| Row | Status |
|---|---|
| empty / zero / nothing | covered — `0 % 0`, `0 / 0`, `0 % 7`, `1e308 * 0`, `sqrt(0)`, `Value::number(0.0)`, `-0.0` |
| singleton and boundary | covered — `1e308` is the last finite magnitude; 2^53 is the last exact whole number; `5 % 0.5`, `7 / 2`; `random_number(1, 2)` |
| out of bounds | covered — `edge_an_index_at_the_numeric_limit_is_nothing_not_a_panic`: index `0`, `-1` (counts from the end), `3`, `999`, `1e308`, `-1e308` and an empty list are each a value or `nothing`, never a panic. An index wider than a double (`1e400`) is refused at the literal. Note the defined outcome for a missing element is `nothing`, not an error (`tests/test_lists.rb:122`) — unchanged by this phase |
| type mismatch | covered — unchanged behaviour, still an error: `"a" + 1`, `1 + "a"`, `1 - "a"`, `1 * "a"`, `1 % "a"` all `Cannot … non-numbers`; the numeric paths are the `if let (Value::Number, Value::Number)` arms this phase changed |
| numeric boundary | covered — the whole phase: `0/0`, `1/0`, `-1/0`, `-0.0`, `NaN` (unreachable, and refused at the door), `sqrt(-1)`, `±Infinity`, 2^53±1, past `i64` |
| unicode | N/A — no text handling added. The three non-finite display words are ASCII, and the lexer/parser are not touched, so a program containing emoji or RTL text reaches this code unchanged (covered by the pre-existing text suite) |
| nesting / recursion | covered — a refused number inside a list literal, a record literal and an index expression; recursion is bounded by phase-006 and is unaffected by an arithmetic error (a runtime error unwinds one frame and is catchable) |
| duplicate / missing keys | N/A — no record field access added. `parse_json_object` keeps its `insert`-last-wins behaviour, untouched |
| malformed input | covered — `1e400` as a literal, `1e400` inside valid JSON, a negative divisor, a literal at exactly `f64::MAX` |
| resource and state | **N/A with a reason** — this phase adds no unbounded work: the loops are `for each`, `repeat` and `while`, and no number computed here feeds a loop count. But `repeat 1e18 times` is still unbounded (`src/vm.rs:313`, verified: still running when killed at 5 s), which is a pre-existing loop-guard gap, not a numeric one. My change does not widen it — `1e18` was always a finite, legal count. → `FINDINGS.md` §3 |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, 0 warnings |
| `cargo test --all-targets` | 119 passed, 0 failed, 0 ignored (103 before, 16 added) |
| `rb test` (Redblue suite) | 200 passed, 0 failed (192 before, 8 added) |
| `examples/*.rb` | all 6 exit 0, and their exit codes are identical to `main` (compared with `git stash` + re-run; output text was not diffed) |
| `./rbops/verify.sh phase-007` | **could not run — `rbops/` is not in this checkout.** `./rbops/verify.sh phase-007` → `bash: ./rbops/verify.sh: No such file or directory` (exit 127); `ls rbops/` → `No such file or directory`. Only `.github/workflows/ci.yml` exists. Same as `phases/phase-006/REPORT.md`. Nothing was authored under `rbops/` to work around it, and no gate was weakened to compensate. |

## Invariants touched

- **None of the language-surface invariants.** `.rb`, `to … end`,
  `set x to <expr>`, `say`, the `Value` variants and the `Error` variants are
  unchanged. `Value::number` is additive.
- **Behaviour change: `5 % 0` is now a runtime error.** It used to store a NaN
  and continue. This is the definition of done for this phase ("x/0 and x%0
  produce a Runtime error"), and it is what made the one pre-existing test that
  asserted the old behaviour fail. That test was **rewritten, not deleted or
  loosened**: `tests/test_arithmetic.rb`
  `edge_arithmetic_modulo_by_zero_yields_a_number_not_a_crash` →
  `edge_arithmetic_modulo_by_zero_is_a_catchable_runtime_error`, and it now
  asserts the failure with `try`/`catch error` + `expect caught to be yes`. No
  assertion was removed, weakened or skipped; no `#[ignore]`, no `.skip`, no
  `allow(clippy::` was added.
- **Behaviour change: overflowing to infinity is an error.** `1e308 * 1e308`
  used to print `inf`; it is now `infinity is not a finite number`.
- **Behaviour change: a whole number wider than 2^53 prints in full.**
  `99999999999999999999` used to print `9223372036854775807`.
- `random_number` over a range wider than a double is now an error instead of
  returning `infinity`.

## Known gaps / follow-ups

- Non-finite values remain reachable by a Rust embedder through the public
  `Value::Number` variant. Closing that needs the variant itself to change,
  which is an invariant. The display is defined for all three, and `json.stringify`
  of one is unreachable from the language. → `FINDINGS.md` §6
- `sqrt` of a negative number answers `nothing` rather than raising. There is
  no error-raising convention in `stdlib::builtin_function`, and `sqrt` is not
  reachable from the VM at all. → `FINDINGS.md` §1
- `Statement::ForRange` (`src/vm.rs:288`) is unreachable — `parse_for` only
  builds `ForEach` — and it still builds `Value::Number` without the finite
  check. Left alone deliberately: it is dead code and fixing it is not this
  phase. → `FINDINGS.md` §4
- No loop guard on `repeat`/`while`. → `FINDINGS.md` §3
- SPEC.md's `Comparison` and `Arithmetic` sections document syntax the lexer
  does not have (`>`, `==`, `is greater than`, `mod`). → `FINDINGS.md` §2