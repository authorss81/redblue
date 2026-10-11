# Phase 047 — Substitute `{expr}` in string literals so `Expr::InterpolatedText` is reachable

## What changed

| File | Lines | What |
|---|---|---|
| `src/parser.rs` | +357 −1 | `parse_primary`'s text arm now calls `parse_text`, which builds `Expr::InterpolatedText`; `interpolated_parts` splits a literal into text runs and substitutions; `read_interpolated` reads the source between `{` and `}` as one expression through a fresh `Parser`; a shared `Rc<Cell<usize>>` `interp_depth` bounds that recursion; `unmatched_opens` marks the opens no `}` closes in one backward pass, so the walk through a literal is linear in its length |
| `bootstrap/compiler.rb` | +119 −1 | the same rule in Redblue, so stage 2 emits the `BUILD_TEXT` stage 1 emits and the S3 fixed point still holds |
| `tests/test_text.rb` | +187 −8 | `text: braces are ordinary characters in a text literal` rewritten to assert real substitution; `text: braces survive at the edges of a text literal` rewritten likewise; `edge_text_an_unknown_name_inside_braces_is_never_printed` rewritten to put the name inside braces, with `edge_text_the_same_body_without_braces_is_not_a_failure` as its control; 17 new Redblue tests |
| `tests/interpolation_test.rs` | +506 | new: 33 `#[test]`s — every one that asserts what a program *prints* runs on both engines — plus three unit tests in `src/parser.rs` for the budget, the nesting and the backward pass |
| `tests/bytecode_test.rs` | +12 −0 | the documented instruction sequence for `say "n is {1 + 2}"` now names the `PUSH_CONST ×3, ADD, BUILD_TEXT` it really compiles to |
| `tests/wasm_test.rs` | +4 −1 | `HELLO` records the substituted third line |
| `tests/common/generator.rs` | +2 −2 | a stale comment that called interpolation "documented and unimplemented" |
| `corpus/functions-0004.expected` | +1 −1 | `hi {name}` → `hi world` |
| `corpus/text-ops-0017.expected` | +1 −1 | `Hello, {name}!` → `Hello, World!` |

The construction site, `src/parser.rs:2437-2442`:

```rust
    fn parse_text(&mut self, text: String, span: Span) -> Result<Expr> {
        match self.interpolated_parts(&text, span)? {
            Some(parts) => Ok(Expr::InterpolatedText(parts)),
            None => Ok(Expr::Text(text)),
        }
    }
```

`grep -n 'InterpolatedText' src/parser.rs` now prints three lines — the declaration at `:58`, the doc comment at `:2432`, and this construction at `:2440` — against the one it printed before.

## The rule

A `{` starts a substitution only when it has a matching `}` and the source between them reads as **one whole expression**. Everything else — an unmatched `{`, a lone `}`, an empty `{}`, a pair holding a JSON document, a pair holding a prefix of a larger expression like `{1 + }` — stays the characters it was written as. A literal with no substitution is still one `Expr::Text`, so a program that writes no brace pair gets exactly the node it got yesterday.

## Definition of done

**`rb run` and `rb vm` both substitute.**

```
$ rb run target/tmp/dod.rb          # set name to "World" / say "Hello, {name}!"
Hello, World!
Value: 5
exit=0

$ rb compile target/tmp/dod.rb -o target/tmp/dod.rbc
Compiled target/tmp/dod.rb -> target/tmp/dod.rbc
exit=0

$ rb vm target/tmp/dod.rbc
Hello, World!
Value: 5
exit=0
```

**An arbitrary expression inside the braces is evaluated** (SPEC.md:297).

```
$ cat target/tmp/dod.rb
set name to "World"
say "Hello, {name}!"
set x to 2
set y to 3
say "Value: {x + y}"
```
Both engines print `Value: 5`.

**A literal with no braces is unchanged, and a brace pair is markup only when well formed.**

```
$ cat target/tmp/braces.rb
say "plain"
say "{"
say "}"
say "{x"
say "{}"
$ rb run target/tmp/braces.rb
plain
{
}
{x
{}
exit=0
```

**`examples/hello.rb` now prints `Hello, World!` on its third line.**

```
$ rb run examples/hello.rb
Hello, World!
Hello, World!
Hello, World!
exit=0
```

Every file in `examples/` and `modules/` (9 files) exits 0.

**All four consumers now run**, pinned by tests: `the_parser_builds_the_node_the_ast_declares` (the construction site itself) and `interpolation_substitutes_a_bound_name_on_both_engines` (both VMs, via `both_engines_print`), `edge_the_linter_sees_a_name_used_only_inside_braces` and `edge_the_linter_reads_names_through_nested_braces` (`src/linter.rs`), `edge_the_formatter_writes_a_substitution_back_as_braces` and `edge_the_formatter_keeps_a_literal_brace_that_is_not_markup` (`src/formatter.rs`), `edge_an_expression_that_fails_inside_braces_is_a_runtime_error` (`src/interpreter.rs`, both engines). `cargo clippy --all-targets -- -D warnings` is clean and no `allow(dead_code)` was added.

**`tests/test_text.rb` is corrected, not deleted.** `text: braces are ordinary characters in a text literal` is now `text: a well formed brace pair substitutes`; its `length(...) to be 14` is replaced by `length("Hello, {name}!") to be 13`, the length of the substituted text. The second test that pinned the same broken behaviour, `text: braces survive at the edges of a text literal` (`:138`), asserted `"{word}"` is `"{word}"` with `word` bound to `"core"`; that is the same false claim in a second place, so it was corrected the same way — to `expect "{word}" to be "core"`, plus `a{word}b` and both lengths — and the genuinely literal cases moved into new `edge_text_*` tests. Neither was deleted, neither was weakened, and `edge_the_corrected_redblue_test_still_exists_and_still_passes` fails if the first is deleted or renamed and re-runs the whole file to prove it still passes.

**`edge_text_an_unknown_name_inside_braces_is_never_printed` now has braces in it.** As first written it bound `known`, set it again inside a `try`, and asserted no failure — a body that never writes a brace pair, so it passed whether interpolation resolved an unknown name, refused it, or printed its braces as text. It could not be fixed the obvious way, either: a name nothing binds anywhere is an `AnalyzerError`, raised before the program runs, which `try` cannot catch — so `say "{nope}"` under `try` fails the whole file rather than the test. What *is* observable from Redblue is the same failure arriving at runtime, and the analyzer has exactly one place it reads a name the walk has not reached: a function body, analysed against the program as it is at the call. So the test declares `constant late` **below** the body that reads `{late}` and calls the body first. The analyzer sees a well formed `{late}`; the interpreter, running before that line, does not; `try` catches it, and `printed` is still `"untouched"` because the failure comes before the assignment. `edge_text_the_same_body_without_braces_is_not_a_failure` is the control — the identical body with the braces removed, where the name is part of the text and nothing is read at all, which is exactly the outcome a parser that printed its braces as text would give the test above. And `edge_the_corrected_redblue_test_still_exists_and_still_passes` now checks both that the test still exists and that its body still contains a brace pair.

## Tests added

| Test | Edge class covered |
|---|---|
| `interpolation_substitutes_a_bound_name_on_both_engines` | the finding itself, on both engines |
| `interpolation_evaluates_an_arbitrary_expression` | SPEC.md:297, an expression and not a bare name |
| `edge_a_literal_without_braces_is_the_text_it_was_written_as` | boundary |
| `edge_an_empty_literal_is_still_empty` | empty |
| `edge_a_lone_open_brace_round_trips` | malformed / out of bounds |
| `edge_a_lone_close_brace_round_trips` | malformed / out of bounds |
| `edge_an_empty_brace_pair_is_not_a_substitution` | empty / malformed |
| `edge_a_pair_holding_something_that_is_not_an_expression_survives` | malformed input — a JSON document in a string, which is what `json.parse` is handed |
| `edge_a_pair_holding_a_prefix_of_a_larger_expression_survives` | malformed input — `{1 + }`, `{x y}` |
| `edge_two_substitutions_keep_their_order_and_the_text_between_them` | singleton / ordering |
| `edge_a_substitution_at_the_start_and_at_the_end_of_a_literal` | boundary — index 0 and index len |
| `edge_nested_braces_are_one_substitution_holding_a_record` | nesting — the record's braces close their own pair |
| `edge_a_substitution_inside_a_record_value_and_a_list_item` | nesting |
| `edge_a_list_a_record_and_an_object_all_substitute` | type coverage — list, record, object |
| `edge_the_substituted_value_is_rendered_the_way_the_interpreter_prints_it` | type mismatch — `yesno`, `nothing`, `0.5`, `[]` all render |
| `edge_unicode_around_and_inside_a_substitution_survives` | unicode — CJK, emoji, accented Latin |
| `edge_an_escaped_quote_next_to_a_substitution_is_not_a_delimiter` | escapes — `\"` before a brace pair |
| `edge_a_substitution_inside_a_function_body_reaches_the_parameters` | scope |
| `edge_a_name_nothing_binds_inside_braces_is_an_analyzer_error` | **asserts a failure** — `AnalyzerError`, the variable named, the string's line and column, both engines |
| `edge_an_expression_that_fails_inside_braces_is_a_runtime_error` | **asserts a failure** — `RuntimeError`, the string's line, and the two engines' column conventions held to separately |
| `edge_a_failure_inside_braces_is_caught_by_try_on_both_engines` | **asserts a failure is caught** — on both engines |
| `edge_a_substitution_nests_one_level_through_a_list` | nesting / recursion — a list holding an escaped literal, inside the braces |
| `edge_substitution_nests_through_a_text_literal_inside_the_braces` | nesting / recursion — three levels, and the reason the budget below is generous |
| `edge_a_very_long_literal_of_braces_is_text_and_is_scanned_once` | resource — 10 000 non-markup pairs, both engines |
| `edge_a_very_long_literal_of_unmatched_opens_is_text_and_is_scanned_once` | resource — 50 000 opens nothing closes, the shape the scan was quadratic on |
| `edge_an_unmatched_open_does_not_hide_the_pair_behind_it` | malformed input — an open that closes nothing, followed by one that does |
| `the_parser_builds_the_node_the_ast_declares` | the construction site itself |
| `a_literal_without_a_substitution_is_still_one_text_node` | the compatibility half of the rule |
| `edge_the_linter_sees_a_name_used_only_inside_braces` | the linter's use analysis, with the warning still firing when nothing is read |
| `edge_the_linter_reads_names_through_nested_braces` | the linter recursing InterpolatedText → List → Variable |
| `edge_the_formatter_writes_a_substitution_back_as_braces` | the formatter round trip, and that the formatted text still runs |
| `edge_the_formatter_keeps_a_literal_brace_that_is_not_markup` | the formatter's idempotence on a literal brace |
| `edge_the_corrected_redblue_test_still_exists_and_still_passes` | the corrected Redblue tests cannot be deleted or emptied of braces quietly |

Three more live in `src/parser.rs`'s own `#[cfg(test)]` module, where the private machinery is reachable: `edge_the_interpolation_budget_is_shared_and_bounds_the_chain` (the branch that spends the budget, and that a read which came back hands it back), `edge_interpolation_nests_through_an_escaped_text_literal` (the same three levels, from the parser side), and `edge_unmatched_opens_are_marked_in_one_backward_pass` (every case where "does a `}` reach this open?" and a forward scan could come apart, including `{ { }`).

Plus 21 new Redblue `test` blocks in `tests/test_text.rb`, 17 of them named `edge_text_*`, including `edge_text_an_unknown_name_inside_braces_is_never_printed`, `edge_text_the_same_body_without_braces_is_not_a_failure`, `edge_text_a_substitution_whose_expression_fails_is_a_caught_error`, `edge_text_nested_braces_are_one_substitution`, `edge_text_an_empty_brace_pair_is_an_ordinary_pair_of_braces`, `edge_text_an_unbalanced_close_brace`, and the list/record/object case.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass, zero warnings |
| `cargo test --all-targets` | 1397 passed, 0 failed, 0 ignored across 51 test binaries |
| `./rbops/verify.sh phase-047` | **not run — the script is not in this checkout.** There is no `rbops/` directory here; the pipeline that invoked this phase lives outside the working directory and AGENTS.md § 1 forbids inspecting it. What that gate is said to cover was run by hand instead: every file in `examples/` and `modules/` exits 0 (9 files), and `cargo test` includes `tests/discovery_test.rs::edge_project_tests_directory_has_no_failures`, which is the suite-level form of the same check. |

## Invariants touched

- **`{interp}` string syntax is now what AGENTS.md § 2 says it is.** Before this phase the invariant was *violated* — the syntax was documented and unwritten. Nothing was relaxed; the invariant now holds. The `Value` variants, the `Error` variants, `to … end`, `set x to <expr>`, `say`, the `.rb` extension, trailing commas and the rest of the table are untouched.
- One behaviour that was *specified but unreachable* became reachable, which is a change: a text literal containing a well formed `{…}` used to print its braces and now evaluates. That is the phase. Every existing program whose literals are not well formed — including every JSON document in a string in `examples/`, `modules/`, `tests/` and `corpus/` — behaves exactly as before, which is what `edge_a_pair_holding_something_that_is_not_an_expression_survives` pins.

## Test matrix

- **empty** — covered: `edge_an_empty_literal_is_still_empty` (the empty literal is still `""` and `Expr::Text`), `edge_an_empty_brace_pair_is_not_a_substitution` (`"{}"` and `"a{}b"` round-trip).
- **singleton** — covered: `edge_two_substitutions_keep_their_order_and_the_text_between_them` and `edge_a_substitution_at_the_start_and_at_the_end_of_a_literal` cover one substitution and two; `interpolation_substitutes_a_bound_name_on_both_engines` is the single-substitution case.
- **boundary** — covered: `edge_a_substitution_at_the_start_and_at_the_end_of_a_literal` puts a substitution at byte 0 and at the last byte; `edge_a_literal_without_braces_is_the_text_it_was_written_as` is the zero-substitution boundary.
- **out_of_bounds** — covered as the brace-scanning boundary: `edge_a_lone_open_brace_round_trips` (an opening brace with no closing one), `edge_a_lone_close_brace_round_trips` (one, two and three closing braces). Index out of bounds inside a substitution is `tests/index_bounds_test.rs`, unchanged by this phase.
- **type_mismatch** — covered: `edge_the_substituted_value_is_rendered_the_way_the_interpreter_prints_it` substitutes a `yesno`, a `nothing`, a non-integral number and an empty list. A substitution of a value that is not text is *not* an error — the language renders every value into text — and that is what the test asserts. A type mismatch between the braces and their contents is `edge_a_pair_holding_something_that_is_not_an_expression_survives` (a JSON document where an expression was required).
- **numeric_boundary** — N/A in the narrow sense: nothing in this change does arithmetic. `edge_the_substituted_value_is_rendered_the_way_the_interpreter_prints_it` pins the rendering of `0.5`, and `edge_an_expression_that_fails_inside_braces_is_a_runtime_error` pins `1 / 0` as the failure. `0/0`, `±Infinity`, `NaN` and `2^53±1` are unchanged by this phase and are covered by `tests/numeric_edge_test.rs`.
- **unicode** — covered: `edge_unicode_around_and_inside_a_substitution_survives` (CJK inside the braces, accented Latin, emoji and an em dash around them), plus `edge_text_unicode_around_a_substitution_is_kept` in `tests/test_text.rb`. The scan is over `char`s, so a brace byte inside a multi-byte character cannot be mistaken for one.
- **nesting_recursion** — covered: `edge_nested_braces_are_one_substitution_holding_a_record` (a record inside the braces closes its own pair), `edge_a_substitution_inside_a_record_value_and_a_list_item`, `edge_a_substitution_inside_a_function_body_reaches_the_parameters`, `edge_a_substitution_nests_one_level_through_a_list`, and `edge_substitution_nests_through_a_text_literal_inside_the_braces`, which reaches three levels and is as deep as a source of any length can go — see Known gaps.
- **duplicate_missing_keys** — N/A: a substitution is an expression, not a record, so it cannot carry keys. A name nothing binds inside braces is the missing-key analogue and is covered by `edge_a_name_nothing_binds_inside_braces_is_an_analyzer_error`, and at the Redblue level by `edge_text_an_unknown_name_inside_braces_is_never_printed`, which reaches the same failure at run time because `try` cannot catch an `AnalyzerError`. Repeated keys inside a record that a substitution holds is `tests/record_order_test.rs`, unchanged.
- **malformed_input** — covered: the lone `{`, the lone `}`, `{}`, `{1 + }`, `{x y}`, the JSON document, and an escaped quote next to a brace pair. The one case a skip-everything shortcut gets wrong — an open that closes nothing *followed* by one that does — is `edge_an_unmatched_open_does_not_hide_the_pair_behind_it`. An unterminated string never reaches this code — the lexer refuses it first, which is `tests/lexer_robustness_test.rs`.
- **resource_limit** — covered: `edge_a_very_long_literal_of_braces_is_text_and_is_scanned_once` (10 000 pairs), `edge_a_very_long_literal_of_unmatched_opens_is_text_and_is_scanned_once` (50 000 opens nothing closes — the shape the scan used to be quadratic on), and `edge_unmatched_opens_are_marked_in_one_backward_pass` for the pass itself. `interp_depth` caps the recursion into a fresh parser per substitution at the same 64 levels the rest of the parser uses, and `edge_the_interpolation_budget_is_shared_and_bounds_the_chain` drives that counter to the limit and holds the branch that spends it to account, since no source can reach it — see Known gaps. The VM's loop guard, call-depth limit and output cap are unchanged and are covered by `tests/loop_bounds_test.rs`, `tests/call_depth_test.rs` and `tests/loop_instruction_test.rs`.

## Known gaps / follow-ups

- **`interp_depth`'s limit branch cannot be reached from source — and a test now holds it to account anyway.** This corrects an earlier claim in this file. Nesting is *not* structurally impossible: the outer lexer resolving `\"` to `"` is exactly what lets the inner source carry its own delimiters, so `"{\"{\"{a}\"}\"}\""` substitutes at three levels, and `edge_substitution_nests_through_a_text_literal_inside_the_braces` pins it on both engines. What cannot be reached is the *limit*, because each level costs an escaped copy of the one below — the source for *n* levels grows like 2ⁿ, and 64 levels is a source no file holds. So the branch is driven directly: `edge_the_interpolation_budget_is_shared_and_bounds_the_chain` sets the shared counter to `MAX_NESTING_DEPTH` and holds the diagnostic, its message and its span to account, and holds that a read which came back — whether it read an expression or refused to find one — hands the budget back. The counter is kept: it costs one `Cell` and is the only thing bounding the recursion if the grammar ever gains a way to nest without paying the doubling.
- **The stage-2 compiler still scans for a matching `}` the quadratic way.** `interp_close` (`bootstrap/compiler.rb:2070`) answers the same question `unmatched_opens` now answers in one pass, by walking forward from every `{`. Stage 1 and stage 2 therefore still agree on every answer — which is what the S3 fixed point checks, and which this change does not touch — and differ only in how long stage 2 takes to reach it. No example, module or corpus program writes a literal long enough to notice. → `FINDINGS.md`.
- **A brace pair inside a quoted inner expression is counted.** `interpolated_parts` counts `{` and `}` without regard for quotes, so `"{ "}" }"`-shaped literals are read as unbalanced and stay literal. This is deliberate and documented at the function: honouring quotes would need the lexer to hand back the raw source span of a literal, which it does not keep. Both engines implement the same rule, which is what the S3 fixed point checks.
- **Braces cannot be escaped.** `"\{x\}"` is `{x}` — `Lexer::read_text` resolves `\{` to `{` — so there is no spelling for a literal brace pair that also reads as an expression. This is unchanged by the phase: it is why `tests/test_text.rb` uses an unbound name for its "not well formed" cases rather than an escape.
- The stage-2 rule for a brace pair whose contents the stage-2 parser *refuses* is a `try`/`catch` around the attempt, so a refusal is answered as "not well formed markup", which is what stage 1 answers. The one visible difference is diagnostic noise on stderr: `refuse` prints before it fails, and stage 2 has no quiet mode. It cannot affect the bytes and it does not affect any example, module or corpus program, all of which are silent — but a user running `rb run bootstrap/compiler.rb` on a program with a malformed brace pair will see one line on stderr and a `0` on stdout. → `FINDINGS.md`.