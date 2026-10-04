# Phase 007 — Numeric edge semantics

## Reproduction

The finding reproduces, and it is worse than "no policy for division by zero".
On the commit this phase started from (`7249abe`, phase-006), before any change:

```
$ printf 'say 5 %% 0\n' > repro.rb && ./target/debug/rb run repro.rb
NaN
$ echo $?
0

$ printf 'say 5-2\n' > minus.rb && ./target/debug/rb run minus.rb
0

$ printf 'say 1e308 * 1e308\n' > inf.rb && ./target/debug/rb run inf.rb
inf

$ printf 'set y to 1e400\nsay y\n' > lit.rb && ./target/debug/rb run lit.rb
inf

$ printf 'say 99999999999999999999\n' > big.rb && ./target/debug/rb run big.rb
9223372036854775807

$ printf 'say 1.2.3\n' > bad.rb && ./target/debug/rb run bad.rb
0

$ printf 'say 1e-400\n' > under.rb && ./target/debug/rb run under.rb
0
```

Seven defects, all reachable from ordinary source, all in the same family: a
number the program did not ask for, computed and printed without comment.

1. **`5 % 0` stored a NaN in a `number` and exited 0.** `1 / 0` was already an
   error (`src/vm.rs:567`), so the two operators disagreed.
2. **`5-2` printed `0`.** The number scanner accepted `+` and `-` anywhere, so
   `5-2` was one token, `"5-2"`, which `f64` cannot parse, and
   `num_str.parse().unwrap_or(0.0)` (`src/lexer.rs:178`) turned that into a
   silent zero. This is worse than the rest: it is not a limit of `f64`, it is
   **wrong arithmetic with no diagnostic at all**.
3. **`1.2.3` and `2..3` printed `0`** for the same reason — a malformed literal
   was indistinguishable from the number zero.
4. **`1e308 * 1e308` stored an infinity**, and `say` printed the bare token
   `inf` (`Value::Display` fell through to `write!(f, "{}", n)`), which is not a
   Redblue literal — reading it back gives *text*.
5. **A literal too large to hold (`1e400`) became an infinity** at
   `Expr::Number`.
6. **`say 99999999999999999999` printed `9223372036854775807`.** `Display` cast
   through `i64`, which **saturates**, so an `f64` of 1e20 was reported as
   `i64::MAX` — a whole number the program never held.
7. **`1e-400` silently became `0`** — the underflow case, with no policy.

`json.parse("1e400")` and `random_number(-1e308, 1e308)` were the same hole by
other routes (`src/vm.rs:1163`, `src/vm.rs:1034`).

## What changed

| File | Lines | What |
|---|---|---|
| `src/lexer.rs` | +26 −7 | `read_number` reads one literal and returns `Result<f64>`: a sign belongs to a literal only after its exponent, and text that is not a number is `Invalid number '…'` instead of `0` |
| `src/value.rs` | +17 −8 | `finite_number` (the rule, for a number that stays a number), `Value::number` (the one door into `Value::Number`) |
| `src/vm.rs` | +44 −4 | `Mod` by zero is `Modulo by zero`; `+ - * /`, `Expr::Number`, `parse_json`, `random_number` go through `Value::number`; the `ForRange` counter goes through `finite_number`; `json_stringify` writes a non-finite number as `null` |
| `src/stdlib.rs` | +11 −2 | `sqrt` of a negative number answers `nothing`, not NaN; `builtin_function` documents what `None` and `Nothing` mean |
| `SPEC.md` | +59 | "Numeric semantics": the outcome table, the "Literals" shape of a number, and "Underflow" |
| `tests/numeric_edge_test.rs` | +482 new | 20 Rust tests |
| `tests/test_numeric_edges.rb` | +103 new | 10 Redblue blocks |
| `tests/test_arithmetic.rb` | +13 −5 | one block rewritten, see "Invariants touched" |

### The policy

`Value::Number` is a public `f64`, so it cannot be made unrepresentable without
changing the variant, which is a fixed invariant. Instead every value the VM
*computes* goes through one constructor:

```rust
pub fn number(n: f64, span: Span) -> Result<Value>   // src/value.rs:127
pub fn finite_number(n: f64, span: Span) -> Result<f64>  // src/value.rs:63
```

which return `Error::Runtime` for NaN and ±infinity. `Display` still defines the
three non-finite values, because a Rust embedder can still construct one
directly; it prints `not a number`, `infinity`, `negative infinity`.

Four further rules, each with a test that fails without it:

- **`x % 0` is an error**, matching `x / 0`.
- **A sign belongs to a literal only after its exponent.** `5-2` is `3`.
- **A string of characters that is not a number is a lex error naming it**,
  never the number `0`. `unwrap_or(0.0)` had made every such mistake invisible.
- **Underflow to zero is defined as `0`,** not refused. Overflow is different:
  the answer lies outside the reals, so the operation fails rather than naming a
  number the program does not hold. Below, the answer *is* a number the language
  has, so `1e-400` is `0` — and an ordinary zero divisor, so `1 / 1e-400` is
  `Division by zero`. SPEC.md §"Underflow" states this.

## Tests added

21 `#[test]` functions (20 in `tests/numeric_edge_test.rs`, 1 in `src/vm.rs`) and
10 Redblue blocks in `tests/test_numeric_edges.rb`.

| Test | Edge class covered |
|---|---|
| `modulo_by_zero_is_a_runtime_error` | the red test: `5 % 0` used to be a NaN, now `Modulo by zero` |
| `edge_modulo_by_a_zero_divisor_of_every_shape_is_rejected` | empty/zero (`0 % 0`), boundary (`5 % 0.5`, `5 % -0.0`), numeric (`-5 % 3`, `5 % -3`, `0 % 7`) |
| `edge_division_by_zero_still_reports_an_error_for_every_numerator` | zero (`1/0`, `0/0`, `-1/0`, `1/-0.0`), boundary (`7/2`, `6/3`) |
| `edge_overflow_to_a_non_finite_number_is_a_runtime_error` | numeric boundary: `*`, `+`, `-` overflow to ±infinity; `1e308 * 1`, `1e308 + 1e307` still fit |
| `edge_random_number_refuses_a_range_whose_width_overflows` | numeric boundary; asserts a failure; independent of the clock draw |
| `edge_a_number_literal_out_of_range_is_rejected` | malformed numeric input: `1e400`, `-1e400` refused; `f64::MAX` accepted |
| `edge_json_numbers_out_of_range_are_rejected` | malformed input: `json.parse("1e400")`; `json.parse("42")` untouched |
| `edge_negative_zero_equals_zero_and_prints_as_zero` | numeric boundary: `-0.0` is `0`, `0 * -1 is 0` is `yes`, `-0.0` as a divisor is zero |
| `edge_a_number_wider_than_2_53_is_not_printed_as_another_integer` | precision: 2^53 exact, 2^53+1 rounded, 1e20 no longer `i64::MAX` |
| `edge_non_finite_numbers_have_a_defined_display` | display of every non-finite value; `f64::MAX`/`MIN`/`MIN_POSITIVE` are not mistaken for one |
| `edge_json_never_sees_a_non_finite_number` | json output for 1e20 and 1.5 |
| `nan_is_rejected_by_the_only_door_into_value_number` | asserts a failure; `Value::number` rejects all three, accepts 0, −0, ±1, `MAX`, `MIN`, `MIN_POSITIVE` |
| `nan_never_compares_equal_so_it_cannot_be_tested_for` | asserts a failure; NaN ≠ NaN, and all five routes to a NaN from source are closed |
| `edge_sqrt_of_a_negative_number_has_no_answer` | `sqrt(-1)` → `nothing`; `sqrt(4)` → 2, `sqrt(0)` → 0 |
| `edge_non_finite_results_are_refused_inside_a_list_and_record` | nesting: inside a list literal, a record literal, and an index expression |
| `edge_an_index_at_the_numeric_limit_is_nothing_not_a_panic` | out_of_bounds: index `0`, `-1`, `len`, `999`, `±1e308` on a 3-list and on an empty list |
| **`edge_a_sign_right_after_a_number_is_an_operator_not_part_of_the_literal`** | **red test:** `5-2` was `0`, now `3`. Also `5+2`, `10-2-3`, `1e-3`, `2E-3`, `1.5e-1`, `5.`, `.5` |
| **`edge_a_malformed_number_literal_is_a_lexer_error_not_a_silent_zero`** | **malformed input; asserts a failure:** `1.2.3`, `2..3`, `3e`, `1e+`, `1.5e` name themselves; also inside a list, a record and an index |
| **`edge_a_literal_too_small_to_hold_is_the_number_zero`** | **numeric boundary:** `1e-400`, `1e-324` are `0`; `5e-324` is not; `1e-200 * 1e-200` is `0`; `1 / 1e-400` and `5 % 1e-400` are errors |
| **`edge_a_numeric_for_range_cannot_step_into_a_non_finite_number`** | **out_of_bounds / resource; asserts a failure:** a `ForRange` counter that steps past `f64::MAX` is a runtime error, not an infinity that ends the loop by accident |
| **`edge_json_writes_a_number_that_is_not_finite_as_null`** (`src/vm.rs`) | **display of every non-finite value; asserts a failure:** JSON writes `null`, not `NaN`/`inf`; the widest finite numbers are not nulled |
| `edge_modulo by zero is a catchable runtime error` (Redblue) | `try`/`catch` proves the failure is produced |
| `edge_overflow to infinity is a catchable runtime error` (Redblue) | ditto |
| `edge_a number literal out of range is refused` (Redblue) | ditto |
| `arithmetic: modulo keeps the sign of the dividend` (Redblue) | numeric boundary |
| `edge_integers are exact up to 2^53 and rounded past it` (Redblue) | precision |
| `edge_negative zero equals zero` (Redblue) | numeric boundary |
| `numeric: a number wider than i64 is still a number` (Redblue) | precision |
| **`edge_a minus sign next to a number is a subtraction`** (Redblue) | **numeric boundary:** `5-2` is 3, `10-2-3` is 5, `1e-3` is 0.001 |
| **`edge_underflow to zero is the number zero`** (Redblue) | **numeric boundary; asserts a failure:** `1e-400` is 0 and `1 / 1e-400` is caught |

## Edge-case matrix

| Row | Status |
|---|---|
| empty / zero / nothing | covered — `0 % 0`, `0 / 0`, `0 % 7`, `1e308 * 0`, `sqrt(0)`, `Value::number(0.0)`, `-0.0`, `1e-400` |
| singleton and boundary | covered — `1e308` is the last finite magnitude; `f64::MAX`/`MIN`/`MIN_POSITIVE` are the last representable doubles and are not mistaken for infinity; `5e-324` is the last positive one; 2^53 is the last exact whole number; `5 % 0.5`, `7 / 2`, `random_number(1, 2)`; a one-iteration `ForRange` |
| out of bounds | covered — `edge_an_index_at_the_numeric_limit_is_nothing_not_a_panic`: index `0`, `-1` (counts from the end), `3`, `999`, `1e308`, `-1e308` and an empty list are each a value or `nothing`, never a panic, and an index wider than a double (`1e400`) is refused at the literal. `edge_a_numeric_for_range_cannot_step_into_a_non_finite_number`: a loop counter past `f64::MAX` is a clean error. The defined outcome for a missing element is `nothing`, not an error (`tests/test_lists.rb:122`) — unchanged by this phase |
| type mismatch | covered — unchanged behaviour, still an error: `"a" + 1`, `1 + "a"`, `1 - "a"`, `1 * "a"`, `1 % "a"` all `Cannot … non-numbers`; the numeric paths are the `if let (Value::Number, Value::Number)` arms this phase changed. A text where a number is required is `Index must be a number` |
| numeric boundary | covered — the whole phase: `0/0`, `1/0`, `-1/0`, `5%0`, `-0.0`, `NaN` (unreachable, and refused at the door), `sqrt(-1)`, `±Infinity`, 2^53±1, past `i64`, `5e-324`, `1e-324`, `1e-400`, `f64::MAX` |
| unicode | N/A — no text handling was added and no text token was touched. The malformed-literal message quotes the literal itself, which may hold any character after the first bad one only in the sense that the scanner stops at it (`1.2.3😀` → `Invalid number '1.2.3'`), so an emoji next to a bad number still lexes and still gets the ASCII error. Program text containing emoji or RTL reaches this code unchanged (covered by the pre-existing text suite, `tests/test_text.rb`) |
| nesting / recursion | covered — a refused number inside a list literal, a record literal and an index expression; a refused literal inside a list, a record and an index. Recursion is bounded by phase-006 and is unaffected by an arithmetic error (a runtime error unwinds one frame and is catchable) |
| duplicate / missing keys | N/A — no record field access was added. `parse_json_object` keeps its `insert`-last-wins behaviour, untouched |
| malformed input | covered — `1e400` as a literal, `1e400` inside valid JSON, `1.2.3`, `2..3`, `3e`, `1e+`, `1.5e` as literals, a negative divisor, a literal at exactly `f64::MAX`, and the same malformed literals in four positions (bare, `say`, list, record, index) |
| resource and state | **N/A with a reason** — this phase adds no unbounded work: the loops are `for each`, `repeat` and `while`, and no number computed here feeds a loop count except the `ForRange` counter, which is now checked against `f64::MAX`. But `repeat` is still unbounded in principle (`src/vm.rs:313`, verified: `repeat 1e18 times` still running when killed at 5 s) because the loop count itself is finite and legal. That is a pre-existing loop-guard gap, not a numeric one. → `FINDINGS.md` §3 |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, 0 warnings |
| `cargo test --all-targets` | 124 passed, 0 failed, 0 ignored (119 before this run, 5 added) |
| `cargo clippy -- -D warnings` (what `.github/workflows/ci.yml` runs) | pass |
| `rb test` (Redblue suite) | 202 passed, 0 failed (200 before this run, 2 added) |
| `examples/*.rb`, `modules/*.rb`, `tests/*.rb` | every file's stdout+stderr+exit code captured before and after this run's change and compared: byte-identical except the wall-clock timestamp in `examples/time.rb`. `modules/MathUtils.rb` exits 1 (`ParserError: Expected function name`) both before and after — pre-existing, not a regression |
| `rb lint` on all 18 `.rb` files | 0 errors on all 18 |
| `./rbops/verify.sh phase-007` | **could not run — `rbops/` is not in this checkout.** `./rbops/verify.sh phase-007` → `bash: ./rbops/verify.sh: No such file or directory` (exit 127); `ls rbops/` → `No such file or directory`. Only `.github/workflows/ci.yml` exists, and its three steps (`cargo build`, `cargo test`, `cargo clippy -- -D warnings`) were all run by hand and pass. Nothing was authored under `rbops/` to work around it, and no gate was weakened to compensate. |

## Invariants touched

- **None of the language-surface invariants.** `.rb`, `to … end`,
  `set x to <expr>`, `say`, the `Value` variants and the `Error` variants are
  unchanged. `Value::number` and `value::finite_number` are additive.
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
- **Behaviour change: `5-2` is `3`, not `0`.** `10-2-3`, `5+2`, `1-2` likewise.
  No file in `examples/`, `modules/` or `tests/` contained one, so nothing else
  moved.
- **Behaviour change: a malformed literal is a lex error.** `1.2.3`, `2..3`,
  `3e`, `1e+`, `1.5e` used to be `0`; they now name themselves in the error.
- **Behaviour change: overflowing to infinity is an error.** `1e308 * 1e308`
  used to print `inf`; it is now `infinity is not a finite number`.
- **Behaviour change: a whole number wider than 2^53 prints in full.**
  `99999999999999999999` used to print `9223372036854775807`.
- **Behaviour change: `json.stringify` of a non-finite number is `null`**
  instead of `NaN`/`inf`, which is not JSON. SPEC.md already promised `null`;
  the code did not do it.
- `random_number` over a range wider than a double is now an error instead of
  returning `infinity`.

## Known gaps / follow-ups

- **The total-ness of `Value::Number` stops at the public variant.** A Rust
  embedder can still write `Value::Number(f64::NAN)`. Its display is defined
  (`not a number`, `infinity`, `negative infinity`), it is written as `null` in
  JSON, and no Redblue program can produce one. Closing the rest needs the
  variant to change, which is an invariant. → `FINDINGS.md` §6
- **Underflow to zero is silently zero.** Deliberate and documented, but it does
  mean `say 1e-400` prints `0` rather than complaining. Revisit if a phase gives
  the language a decimal or arbitrary-precision number. → SPEC.md §"Underflow"
- **`sqrt` of a negative number answers `nothing` rather than raising.** There
  is no error-raising convention in `stdlib::builtin_function`, and `sqrt` is
  not reachable from the VM at all. → `FINDINGS.md` §1
- **A very large number prints as hundreds of digits.** `say 1e308` is 309
  characters. It is honest — it is exactly the number the double holds — but
  unreadable, and exponent notation is a grammar decision, not a formatting
  tweak. → `FINDINGS.md` §7
- **No loop guard on `repeat`/`while`.** → `FINDINGS.md` §3
- **`2..3` is a lex error, not a range.** A phase that adds a range operator has
  to teach the lexer about it first. → `FINDINGS.md` §8
- **SPEC.md's `Comparison` and `Arithmetic` sections document syntax the lexer
  does not have** (`>`, `==`, `is greater than`, `mod`). → `FINDINGS.md` §2