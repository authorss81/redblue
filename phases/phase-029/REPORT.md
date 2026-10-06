# Phase 029 — Make `mod` a keyword and implement the `in` membership operator

## What changed

| File | Lines | What |
|---|---|---|
| `src/lexer.rs` | +1 | `("mod", TokenKind::Mod)` in the `KEYWORDS` table, so `10 mod 3` lexes as the modulo operator instead of the identifier `mod` |
| `src/parser.rs` | +11 −2 | `parse_is_operator` consumes a `TokenKind::In` and returns `BinaryOp::In`; doc comment records that `in` needs no partial-word guard because it is a keyword |
| `src/formatter.rs` | +4 −1 | `BinaryOp::In` now formats as `is in`, not `in` — a bare `in` does not parse, so the old output silently produced non-Redblue source |
| `src/repl/completer.rs` | +1 | `mod` added to the REPL completion list |
| `tooling/vscode/syntaxes/redblue.tmLanguage.json` | +1 −1 | `mod` added to the keyword alternation, so the shipped grammar still matches `Lexer::keywords()` (asserted by `tooling_grammar_test`) |
| `SPEC.md` | +6 | `is in` is defined as list-only membership, with the error for a non-list haystack and a pointer to `not (x is in …)` instead of `is not in` |
| `docs/GRAMMAR.md` | +4 −4 | `comparison 'is in' additive` (was `'in'`, which never parsed); `%` added to the multiplicative precedence row |
| `tests/mod_in_test.rs` | +563 (new) | 18 `#[test]` functions |
| `tests/test_mod_in.rb` | +96 (new) | 9 Redblue `test` blocks |

Nothing else in `src/` was touched. No public type, no `Value` variant, no
`Error` variant, no grammar rule outside the two productions named above.

## Reproduction (before the change)

```
$ ./target/debug/rb run ./target/tmp/repro_mod.rb
Error: AnalyzerError: Unknown variable 'mod'
  --> ./target/tmp/repro_mod.rb:1:8
1 | say 10 mod 3
  |        ^
exit=1

$ ./target/debug/rb run ./target/tmp/repro_in.rb
Error: ParserError: Unexpected token In
  --> ./target/tmp/repro_in.rb:1:11
1 | if "a" is in ["a","b"] then
  |           ^
exit=1
```

The finding was real and current. One correction to its evidence: the claim
that `grep -rn 'BinaryOp::In' src/` "returns nothing" is stale — the variant
is already built and consumed in `src/runtime.rs:229`, `src/analyzer.rs:423`,
`src/formatter.rs:649`, `src/bytecode/codegen.rs:595` and
`src/bytecode/vm.rs:379`. What was true, and what the test proved, is that
**no parse site constructs one**.

After the change:

```
$ ./target/debug/rb run ./target/tmp/repro_mod.rb
1
$ ./target/debug/rb run ./target/tmp/repro_in.rb
yes
```

## Tests added

`tests/mod_in_test.rs` — 18 `#[test]` functions, 9 of them `edge_*`.

| Test | Edge class covered |
|---|---|
| `mod_is_a_keyword_and_not_an_identifier` | lexer: `mod` → `TokenKind::Mod`; `module` keeps its own keyword; `modify`/`modern`/`modulo`/`mode`/`moved` stay identifiers (reserving `mod` swallowed nothing) |
| `the_word_mod_computes_the_documented_remainder` | `SPEC.md:206` verbatim: `set remainder to 10 mod 3` exits 0 and prints 1, and `10 % 3` prints 1; both engines agree |
| `the_word_mod_and_the_symbol_percent_agree_on_every_operand` | boundary / numeric_boundary: 12 operand pairs compared symbol-vs-word and against Rust `%`, including `-5 mod 3` = -2 (`SPEC.md:224`), `±1` divisors, `0`, fractional and near-overflow operands |
| `the_word_mod_keeps_multiplicative_precedence` | nesting: `mod` mixes with `+ - * / ( )` exactly as `%` does |
| `edge_modulo_by_zero_is_a_caught_runtime_error` | **asserts a failure**: `5 mod 0`, `5 mod 0.0`, `5 mod -0.0`, `5 % 0`, `0 mod 0` all fail `RuntimeError: Modulo by zero`, identically on both VMs; plus an `rb run` program proving it is *caught* and exits 0 |
| `edge_modulo_rejects_non_numbers` | **asserts a failure**: `Cannot modulo non-numbers` for text/nothing/list on either side |
| `is_in_takes_the_branch_it_names` | the DoD branch pair: `"a" is in ["a","b"]` takes then, `"z"` takes else |
| `is_in_keeps_comparison_precedence` | nesting: `+ - * ( )` bind tighter, `and` binds looser |
| `is_before_a_partial_in_word_still_tests_equality` | regression: `is inside`, bare `is`, `is not`, `is equal to`, `is greater/less than` unchanged |
| `edge_empty_list_haystack_contains_nothing` | empty: `[]` contains nothing, for a number, text, `nothing` and `[]` needles, plus the `else` branch under `rb run` |
| `edge_empty_text_haystack_is_a_runtime_error` | **asserts a failure**: `""`, `"abc"` haystacks all give `Right side of 'in' must be a list` — no substring form |
| `edge_nothing_needle_matches_only_nothing` | empty/nothing: `nothing` is a value, not a wildcard; matches only a list holding `nothing` |
| `edge_needle_of_the_wrong_type_answers_no_rather_than_failing` | type_mismatch: `1 is in ["1"]`, `"1" is in [1]` answer `no`; `1.0 is in [1]` is `yes` |
| `edge_singleton_haystack_matches_only_its_one_element` | singleton: one-element list found by that element only |
| `edge_membership_in_a_record_or_a_number_is_a_caught_runtime_error` | **asserts a failure**: record, number, `nothing` haystacks give a clean `RuntimeError`, caught by `try/catch` under `rb run` — not `no`, not a panic |
| `edge_nested_and_unicode_haystacks_are_compared_structurally` | nesting + unicode: nested lists/records; `héllo`, `日本語`… , CJK, emoji, combining marks (`"é"` vs `"è"`), RTL Arabic |
| `edge_the_formatter_round_trips_the_word_forms` | **asserts a failure was fixed**: formatted output must still parse and still take the same branch |
| `the_word_forms_run_on_the_bytecode_vm_too` | differential: tree-walking and bytecode VMs print the same and fail with the same kind and message |

`tests/test_mod_in.rb` — 9 Redblue `test` blocks, all with real `expect`
assertions, 5 named `edge_*`:

| Test | Edge class covered |
|---|---|
| `mod: the word form computes the documented remainder` | `10 mod 3` = 1, equal to `10 % 3` |
| `mod: the word form and the symbol agree on the sign of the dividend` | `-5 mod 3` = -2 = `-5 % 3` |
| `edge_mod: a zero divisor is caught, not fatal` | **asserts a failure**: `try`/`catch` runs the catch clause |
| `edge_mod: a text operand is caught` | **asserts a failure**: `10 mod "2"` caught |
| `is in: membership selects the branch it names` | the DoD branch pair |
| `edge_is_in: an empty list contains nothing` | empty haystack |
| `edge_is_in: an empty text haystack is an error, not a substring search` | **asserts a failure** |
| `edge_is_in: a record or a number haystack is caught` | **asserts a failure**, both, counted |
| `edge_is_in: a nothing needle and a wrong-type needle` | empty/nothing + type_mismatch |

## Edge-case matrix (AGENTS.md section 3.2)

- **empty** — covered. `edge_empty_list_haystack_contains_nothing` (`[]`
  haystack, `nothing`/`""`/`[]` needles), `edge_nothing_needle_matches_only_nothing`,
  `edge_empty_text_haystack_is_a_runtime_error`, `0 mod 3` in the operand table.
- **singleton** — covered. `edge_singleton_haystack_matches_only_its_one_element`;
  the unit divisors `1 % 1`, `-1 mod 1`, `1 mod -1` in the operand table.
- **boundary** — covered. `-5 mod 3` = -2 (`SPEC.md:224`), both signs on both
  operands, fractional `7.5 mod 2` / `-7.5 mod 2`, `1e308 mod 7`, `2 mod 1e308`,
  `0` dividend. `mod_is_a_keyword_and_not_an_identifier` pins that the longest
  common identifier prefixes (`module`, `modulo`) are unaffected.
- **out_of_bounds** — N/A. Neither operator indexes. `is in` scans a list
  linearly with no cursor the caller can move out of range, and the list it
  scans is a value, not a buffer. The nearest analogue — an empty haystack — is
  covered under **empty**.
- **type_mismatch** — covered. `edge_modulo_rejects_non_numbers` (text, `nothing`
  and list operands on either side); `edge_needle_of_the_wrong_type_answers_no_rather_than_failing`;
  `edge_empty_text_haystack_is_a_runtime_error`; `edge_membership_in_a_record_or_a_number_is_a_caught_runtime_error`.
- **numeric_boundary** — covered for the two that apply: zero divisor
  (`Modulo by zero`, both spellings, `0.0` and `-0.0`) and the sign of the
  dividend. `NaN`, `±Infinity` and `2^53±1` are unreachable as operands —
  `SPEC.md:210` states a Redblue `number` is always finite and is built by
  `Value::number`, which rejects non-finite results — so there is no operand
  value left to assert. The largest operand the language *can* hold
  (`1e308`) is in the operand table.
- **unicode** — covered. `edge_nested_and_unicode_haystacks_are_compared_structurally`:
  accented Latin (`"héllo"`), CJK (`"日本"`), an emoji (`"👍"`), a combining-mark
  pair that must **not** match (`"é"` vs `"è"`), and RTL Arabic (`"مرحبا"`).
- **nesting_recursion** — covered at the depth these operators have. `in`
  recurses through nested lists and records two deep
  (`[1,2] is in [[1,2],[3]]`, `{a: 1} is in [{a: 1}, {b: 2}]`), which is the
  deepest the runtime comparison reaches for membership. Neither operator
  introduces a scope, a closure or a call frame, so 3+ scopes and mutual
  recursion are N/A. `mod` nesting is pinned by
  `the_word_mod_keeps_multiplicative_precedence` (`2 * 7 mod 5`, `(10 mod 3) * 2`).
- **duplicate_missing_keys** — N/A. Neither operator reads a record. The
  nearest case, a record used as the haystack, is covered under
  **type_mismatch** and is a clean error rather than a key lookup.
- **malformed_input** — N/A for new lexer/parser paths. This phase adds one
  token to a table and one arm to an existing `match`; it adds no new character
  class, no new literal syntax and no new error path, so the existing
  `lexer_robustness_test`, `parser_hardening_test`, `span_test` and
  `comparison_lex_test` suites already cover the malformed-input row and all
  still pass unchanged.
- **resource_limit** — N/A. `mod` is one instruction on two `f64`s. `in` is a
  linear scan of an existing list, bounded by the list's own length, which the
  phase did not change and which already runs inside the VM's thread with a
  resolved stack size. The phase adds no recursion, no loop and no output, so
  there is no new unbounded resource path to bound. The suites that do bound
  resources (`call_depth_test`, `loop_bounds_test`, `index_bounds_test`) are
  untouched and green.

## Gates

Run from the project root, in order.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — 0 warnings, `Finished dev profile in 9.14s` |
| `cargo test --all-targets` | pass — 29 binaries, **575 passed, 0 failed, 0 ignored**, 0 skipped (557 before this phase, so +18 — the 9 new Redblue `test` blocks run inside `edge_suite_reports_at_least_40_redblue_tests`, which is unchanged and green) |
| `cargo test --doc` | pass — 1 passed, 0 failed |
| `./rbops/verify.sh phase-029` | **NOT RUN — `rbops/` does not exist in this checkout** (`ls -d rbops` → `ls: cannot access 'rbops': No such file or directory`). Reported honestly rather than claimed. The three cargo gates above are what I could run. |

Zero new `#[ignore]`, `// skip` or `allow(clippy::` suppressions. Zero
newly-failing pre-existing tests: measured by stashing the phase's changes and
re-running the suite, 557 tests passed before and 557 of those same 557 still
pass after, with the 18 new ones added on top.

### Examples and modules

`examples/*.rb` and `modules/*.rb` all exit 0 (loop run against
`./target/debug/rb run`, 0 failures). `MathUtils.rb` was expected to be the
baselined exception; it passes, so no baseline note is needed.

## Definition of done

- [x] `rb run` of a file with `set m to 10 mod 3` exits 0 and prints 1; the same
      file using `10 % 3` prints the same value —
      `the_word_mod_computes_the_documented_remainder` asserts `stdout == "1\n1"`.
- [x] `x mod y` and `x % y` agree on the `SPEC.md:224` boundary cases including
      `-5 mod 3` = -2, and on `% 1`, `mod 1`, `% 0`, `mod 0` — the last two
      caught `RuntimeError`s naming `Modulo by zero` —
      `the_word_mod_and_the_symbol_percent_agree_on_every_operand` and
      `edge_modulo_by_zero_is_a_caught_runtime_error`.
- [x] `if "a" is in ["a","b"] then` takes the then-branch and `if "z" is in
      ["a","b"] then` takes the else-branch — `is_in_takes_the_branch_it_names`,
      run end to end through `rb run`, asserts `stdout == "yes\nno"`.
- [x] `is in` on a text haystack is defined by a test **and** the spec is
      corrected — `edge_empty_text_haystack_is_a_runtime_error` (plus the
      non-empty `"abc"` case) pins list-only, and `SPEC.md` now says so
      explicitly.
- [x] `edge_*` tests cover an empty haystack, an empty-text haystack, a
      `nothing` needle and a wrong-type needle.
- [x] `edge_*` tests assert a failure for `is in` on a record and on a number,
      as a caught `RuntimeError`.
- [x] `cargo test --all-targets` reports 0 failures and every file in
      `examples/` and `modules/` exits 0.

## Invariants touched

- None. `.rb` is still the source extension; no `Value` or `Error` variant
  added, removed or renamed; `set x to <expr>`, `to … end`, `say` and string
  interpolation unchanged.
- One behaviour that was previously unreachable is now reachable: `is in`.
  Before this phase `BinaryOp::In` could be built by nothing, so no shipped
  program's behaviour changed. `rb format` on a hand-written file that already
  contained `is in` used to rewrite it to `in`, which no longer parsed; it now
  round-trips.

## Known gaps / follow-ups

All three are in `phases/phase-029/FINDINGS.md`:

1. **`catch <name>` binds the literal text `"error"`** (`src/vm.rs:740`), not the
   error's message — so a Redblue program can prove *that* it caught an error but
   not *which*. phase-029 pins messages from Rust and catchability from Redblue.
2. **`length of <expr>` does not parse**, though `SPEC.md:322` and
   `docs/GRAMMAR.md:448` document it; `rb run` gives
   `AnalyzerError: Unknown variable 'of'`. Same family of gap, different
   operator.
3. **No gate catches a dead enum variant.** `BinaryOp::In` was fully
   implemented in four places and unreachable from the parser, and no existing
   test could see it. A gate that fails when a `BinaryOp` variant is never
   constructed in `src/` would have caught this phase's finding before it was
   filed.