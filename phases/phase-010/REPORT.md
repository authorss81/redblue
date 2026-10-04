# Phase 010 — Parser hardening: empty file, stray tokens, deep nesting

## Reproduction

The finding reproduces on `main` (commit `2d54e2d`). The "no evidence of tests
for empty input, EOF mid-construct, or pathological nesting depth" part of the
finding is true — `tests/` had no parser robustness file. The behaviour itself
splits into two halves: the empty/malformed half already worked, the nesting
half aborted the process.

Empty, whitespace-only and comment-only input already produced an empty program
(exit 0, no output). Every malformed construct already produced a spanned error:

```
$ ./target/debug/rb run target/tmp/unclosed_end.rb
Error: ParserError: Expected End but got Eof
  --> target/tmp/unclosed_end.rb:3:1

$ ./target/debug/rb run target/tmp/unclosed_str.rb
Error: LexerError: Unterminated string
  --> target/tmp/unclosed_str.rb:1:5

$ ./target/debug/rb run target/tmp/stray_op.rb
Error: ParserError: Unexpected token Newline
  --> target/tmp/stray_op.rb:1:9

$ ./target/debug/rb run target/tmp/unclosed_bracket.rb
Error: ParserError: Unexpected token Newline
  --> target/tmp/unclosed_bracket.rb:1:18
```

Pathological nesting depth was a hard defect — SIGABRT and a core dump, not a
diagnostic. The threshold for brackets is somewhere between 800 and 1200:

```
$ python3 -c "open('target/tmp/d_800.rb','w').write('set x to ' + '['*800 + '1' + ']'*800 + '\n')"
$ ./target/debug/rb format target/tmp/d_800.rb > /dev/null; echo $?
0

$ python3 -c "open('target/tmp/d_1200.rb','w').write('set x to ' + '['*1200 + '1' + ']'*1200 + '\n')"
$ ./target/debug/rb format target/tmp/d_1200.rb > /dev/null; echo $?
thread 'main' (5345) has overflowed its stack
fatal runtime error: stack overflow, aborting
134                                     <- DEFECT: abort, not an error
```

Seven distinct inputs reached it, and the last one needed no nesting at all:

| Input | Where it died |
|---|---|
| `[`×5000 `1` `]`×5000 | `parse_primary` → `parse_expression` recursion |
| `[`×1000 `1` `]`×1000 | same; the DoD's own required case, 1000 deep |
| `(`×100000 `1` `)`×100000 | `parse_primary` → `parse_expression` recursion |
| `-`×100000 `1` | `parse_unary` recursion |
| `1 + 1 + 1 …` (50000 terms) | `Expr` is built iteratively, but **dropping** the 50000-deep left spine recurses once per level |
| `if true then`×1000 `end`×1000 | `parse_statement` → `parse_if` → `parse_statement` recursion |
| `if true then`×5000 `end`×5000 | same |

```
$ ./target/debug/rb format target/tmp/flat100k.rb > /dev/null; echo $?
thread 'main' (5247) has overflowed its stack
fatal runtime error: stack overflow, aborting
134                                     <- DEFECT: abort, not an error

$ ./target/debug/rb format target/tmp/if_5000.rb > /dev/null; echo $?
Aborted                                 <- DEFECT: abort, not an error
```

The Rust tests were written first and watched fail. Against the unfixed parser,
`edge_1000_deep_nested_list_never_overflows_the_stack` killed the whole test
process, which is the defect stated as a test:

```
$ cargo test --test parser_hardening_test
running 14 tests
thread 'edge_1000_deep_nested_list_never_overflows_the_stack' (5745) has overflowed its stack
fatal runtime error: stack overflow, aborting
error: test failed, to rerun pass `--test parser_hardening_test`
Caused by:
  process didn't exit successfully: `…/zz_red_tmp-e31f6f6fdbcab03f` (signal: 6, SIGABRT: process abort signal)
```

That first run used a throwaway copy named `tests/zz_red_tmp.rs` with the depth
constant inlined, because `MAX_NESTING_DEPTH` did not exist until the fix. The
copy was deleted; the committed file is `tests/parser_hardening_test.rs`. The
block-nesting defect was found the same way — while writing the report I claimed
nested `if`s were safe, checked, found 5000 still aborted, and added
`edge_5000_nested_blocks_never_overflows_the_stack` plus the `MAX_BLOCK_DEPTH`
guard.

After the change, all seven inputs are spanned `ParserError`s:

```
$ ./target/debug/rb format target/tmp/deep1000.rb
Format error: Parser error: ParserError: Expression nests more than 64 levels deep
  --> 65:1

$ ./target/debug/rb format target/tmp/deep_paren.rb
Format error: Parser error: ParserError: Expression nests more than 64 levels deep
  --> 65:1

$ ./target/debug/rb format target/tmp/deep_unary.rb
Format error: Parser error: ParserError: Expression nests more than 64 levels deep
  --> 65:1

$ ./target/debug/rb format target/tmp/flat100k.rb
Format error: Parser error: ParserError: Expression nests more than 64 levels deep
  --> 1:272

$ ./target/debug/rb format target/tmp/if_20000.rb
Format error: Parser error: ParserError: Blocks nest more than 64 levels deep
  --> 65:1
```

## What changed

| File | Lines | What |
|---|---|---|
| src/parser.rs | +25 −0 | `MAX_NESTING_DEPTH` (13 lines of doc + the constant) and `MAX_BLOCK_DEPTH` |
| src/parser.rs | +2 −0 | `Parser.depth` and `Parser.block_depth` fields |
| src/parser.rs | +5 −1 | `Parser::new` initialises both counters |
| src/parser.rs | +53 −0 | `enter_nesting` / `leave_nesting` (expression guard) and `opens_block` / `enter_block` / `leave_block` (block guard) |
| src/parser.rs | +6 −1 | `parse()` loop now also stops at `None`, not only at `Eof` |
| src/parser.rs | +17 −1 | `parse_statement` wrapper: takes the block level, resets `depth`, releases the block level; body moved verbatim to `parse_statement_inner` |
| src/parser.rs | +11 −0 | `parse_or`, `parse_and`, `parse_comparison`, `parse_addition`, `parse_multiplication`: one counted level per chained operator, released when the chain ends |
| src/parser.rs | +4 −0 | `parse_unary`: one counted level per `not` / `-` |
| src/parser.rs | +2 −0 | `parse_postfix`: one counted level per call-argument list and per index |
| src/parser.rs | +6 −0 | `parse_primary`: one counted level per `(`, `[` and `{` |

Total: **+150 −2**, all in `src/parser.rs` (33 hunks).

One production file, `src/parser.rs`, plus the new test file. Nothing outside
`src/parser.rs` and `tests/parser_hardening_test.rs` was touched.

### Why 64

The number is measured, not guessed. A probe test (`tests/zz_probe.rs`, since
deleted) parsed nested-list literals of increasing depth on a default libstd
thread, whose stack is 2 MiB:

```
ok 121
ok 122
ok 123
thread '<unknown>' (6387) has overflowed its stack
```

123 levels is where a 2 MiB debug-build thread dies — about 16 KiB of stack per
level, which is eleven frames (`parse_primary` → `parse_postfix` →
`parse_unary` → `parse_multiplication` → `parse_addition` → `parse_comparison`
→ `parse_and` → `parse_or` → `parse_expression` → `parse_primary`). `64` keeps
roughly a 2× margin on the smallest stack the language ever runs on, and it is
far above anything hand-written. `nesting_limit_is_a_sane_small_number` locks
both constants at a ceiling of 96 so they cannot be walked back into overflow
territory by a later phase.

### The `parse()` `None` guard

`Lexer::tokenize` always terminates the stream with `Eof`, so this is not
reachable from `rb run`. `Parser::new` is public, so it is reachable from a
caller that hands over an arbitrary token list: `current()` returns `None`,
`parse_statement` returns `Ok(None)` without advancing, and the old loop spun
forever. `token_stream_without_eof_terminates_instead_of_looping_forever` drives
exactly that and would hang without the fix.

## Tests added

`tests/parser_hardening_test.rs`, 23 new `#[test]` functions (floor is 3).
15 are named `edge_*` (floor is 1). 17 assert that a failure is produced.

| Test | Edge class covered |
|---|---|
| `edge_empty_file_parses_to_empty_program` | empty — `""` yields zero statements |
| `edge_whitespace_only_and_comment_only_are_empty_programs` | empty — spaces, `\n\n\n`, tabs, CRLF, comment-only |
| `edge_unclosed_constructs_produce_spanned_parser_errors` | malformed_input — asserts failure: unclosed `end`, unclosed function, unclosed `for`, unclosed `[`, unclosed `(`, unclosed `{`, stray `+`, stray `@`; each must satisfy `error.span().is_some()` |
| `edge_unclosed_string_is_a_spanned_lexer_error` | malformed_input — asserts failure: `Error::Lexer` with a known span and `Unterminated string` in the message |
| `edge_deep_nesting_is_a_clean_error_not_a_stack_overflow` | nesting_recursion / resource_limit — asserts failure for `[`×65, `(`×65, `{a: `×65; would SIGABRT before the fix |
| `edge_deep_unary_chain_is_a_clean_error_not_a_stack_overflow` | nesting_recursion — asserts failure for `-`×65 |
| `edge_long_flat_binary_chain_is_a_clean_error_not_a_stack_overflow` | resource_limit — asserts failure for a 65-operator chain; this is the case that died in `Expr`'s recursive `Drop` |
| `edge_1000_deep_nested_list_never_overflows_the_stack` | nesting_recursion — the exact input that killed the test process in the red run; accepts success **or** a spanned error |
| `edge_pathological_deep_nesting_at_100k_levels_is_rejected_quickly` | resource_limit — asserts failure at 100 000 levels, not a hang |
| `edge_deeply_nested_blocks_are_a_clean_error_not_a_stack_overflow` | nesting_recursion / resource_limit — asserts failure for `if` / `while` / `repeat` / `object` / `try` / `test` × 65 |
| `edge_deeply_nested_functions_are_a_clean_error_not_a_stack_overflow` | nesting_recursion — asserts failure for `to f()` × 65 |
| `edge_5000_nested_blocks_never_overflow_the_stack` | resource_limit — asserts failure; the exact input that aborted the process before `MAX_BLOCK_DEPTH` existed |
| `edge_100k_token_flat_input_parses_linearly` | resource_limit — 25 000 statements (~100k tokens) parse with the exact statement count asserted and a 20 s ceiling |
| `edge_100k_token_flat_input_scales_sub_quadratically` | resource_limit — 10× the tokens must cost under 40× the time, after a warm-up run |
| `edge_single_element_collections_and_unicode_survive` | singleton + unicode — `[]`, `[1]`, `{}`, `{a: 1}`, and a string of emoji + CJK, all five statements asserted |
| `nesting_at_the_depth_limit_is_accepted_and_one_past_is_rejected` | boundary — exactly `MAX_NESTING_DEPTH` parses, `MAX_NESTING_DEPTH + 1` is a spanned error |
| `block_nesting_at_the_depth_limit_is_accepted_and_one_past_is_rejected` | boundary — the exact off-by-one on `MAX_BLOCK_DEPTH` |
| `sibling_blocks_do_not_spend_each_others_budget` | boundary — 200 consecutive sibling `if`s are 200 statements, not 200 levels; asserts all 200 parse |
| `nesting_limit_is_a_sane_small_number` | boundary / resource_limit — both constants stay in 16..=96 |
| `token_stream_without_eof_terminates_instead_of_looping_forever` | resource_limit — asserts a 2-statement program from an `Eof`-less token list; hung before the fix |
| `empty_token_stream_parses_to_an_empty_program` | empty — `Parser::new(Vec::new())` |
| `malformed_input_error_names_what_it_expected` | malformed_input — asserts failure **and** that the message is useful: `Expected End but got Eof` at line 3, `Unexpected token` for `say 1 +`, and a real message for the unclosed `[` |
| `type_mismatch_shaped_input_is_a_parser_error_not_a_panic` | type_mismatch — asserts failure: `set 1 to 2`, `for x in [1]`, `{1: 2}`, bare `import`, `set to 5` |

## Edge-case matrix

- empty — **covered**: `edge_empty_file_parses_to_empty_program`,
  `edge_whitespace_only_and_comment_only_are_empty_programs` (including CRLF
  and comment-only), `empty_token_stream_parses_to_an_empty_program`.
- singleton — **covered**: `edge_single_element_collections_and_unicode_survive`
  asserts one-element list, one-element record, empty list and empty record all
  parse; `token_stream_without_eof_...` asserts a 2-statement program.
- boundary — **covered**: `nesting_at_the_depth_limit_is_accepted_and_one_past_is_rejected`
  and `block_nesting_at_the_depth_limit_is_accepted_and_one_past_is_rejected`
  are the exact off-by-one on the two new limits;
  `sibling_blocks_do_not_spend_each_others_budget` proves the block counter is
  per-path and not per-file; `nesting_limit_is_a_sane_small_number` bounds the
  limits themselves.
- out_of_bounds — N/A + why: this phase adds no indexing, slicing or
  collection access. `parse_postfix`'s index expression is unchanged except for
  the nesting counter around it. Runtime index bounds are covered by
  `tests/index_bounds_test.rs` (phase 008).
- type_mismatch — **covered** at the only level this phase can reach:
  `type_mismatch_shaped_input_is_a_parser_error_not_a_panic` feeds the parser
  text where an identifier, `each`, a record key or an import name is required.
  Type mismatch *between values* is the analyzer/VM's job and is untouched.
- numeric_boundary — N/A + why: no numeric literal is read, converted or
  compared here. The guards count nesting levels, not values, and fire at the
  same depth whether the innermost operand is `1`, `0` or `-0.0`.
  Arithmetic boundaries stay in `tests/numeric_edge_test.rs` (phase 007).
- unicode — **covered**:
  `edge_single_element_collections_and_unicode_survive` parses an emoji + CJK
  string, and every negative test funnels through `assert_spanned_parser_error`,
  which requires `span.is_known()`. Byte-offset vs character-column behaviour
  for Unicode is locked by `tests/lexer_robustness_test.rs` (phase 009).
- nesting_recursion — **covered**: nine of the tests above, across all four
  bracket kinds, both prefix operators, both left-associative chain shapes, six
  block forms, nested functions, and a 100k-level case.
- duplicate_missing_keys — N/A + why: the parser is the only stage changed and
  it neither builds nor reads records' duplicate-key semantics; it stores fields
  in a `Vec` and the last write wins in the VM. Record key ordering is covered
  by `tests/record_order_test.rs` (phase 006).
- malformed_input — **covered**: the DoD's four (unclosed `end`, unclosed
  string, stray operator, unclosed bracket) plus unclosed function, unclosed
  `for`, unclosed `(`, unclosed `{`, and six wrong-token-type cases.
- resource_limit — **covered**: both depth limits (six `edge_*` tests), the
  missing-`Eof` infinite loop, a 100 000-level case, a 5000-block case, and
  100k-token linearity in absolute and relative terms.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass (0 warnings) |
| `cargo test` | pass — 171 passed, 0 failed, 0 ignored (14 binaries incl. doc-tests) |
| `./rbops/verify.sh phase-010` | **not run — `rbops/` is not present in this checkout** |

Per-binary: 6, 0 (main), 11, 13, 21, 8, 16, 20, **23 (new)**, 25, 8, 5, 15, 0
(doc-tests). `cargo test --all-targets` is green with the same 171.

On the fourth gate: `rbops/` does not exist in the working directory
(`ls: cannot access 'rbops': No such file or directory`), and hard rule 1
forbids creating anything under `rbops/`. The gate is documented to run
`examples/*.rb` on top of the three gates above, so I ran that part myself:

```
$ for f in examples/*.rb; do ./target/debug/rb run "$f" || echo "FAIL $f"; done
```

All six examples exit 0 (`hello`, `fizzbuzz`, `files`, `formats`, `time`,
`test_arithmetic`). The Redblue suite is also unaffected:

```
$ ./target/debug/rb test
Tests run: 203
Passed: 203
Failed: 0
```

`rb lint` exits 0 on all 11 `tests/*.rb` and both `modules/*.rb`.

## Invariants touched

- None of the language invariants in AGENTS.md section 2. `.rb`, `to … end`,
  `set x to <expr>`, `say`, the `Value` and `Error` variants, and the
  trailing-comma / `{interp}` string syntax are all unchanged. No public type
  was renamed; the only additions to the public surface are the new
  `pub const MAX_NESTING_DEPTH` and `pub const MAX_BLOCK_DEPTH`, which mirror
  the existing `pub const MAX_CALL_DEPTH` in `src/vm.rs`.
- Two deliberate behaviour changes, both of the form "the process died, now it
  reports a column":
  1. An expression nested more than 64 levels — by brackets, by prefix
     operators, or by a run of same-precedence operators — is a spanned
     `Error::Parser`.
  2. `... end` blocks nested more than 64 deep is a spanned `Error::Parser`.

  The largest run of chained `+`/`-` anywhere in `examples/`, `modules/` or
  `tests/` is 0, and the deepest block nesting in the shipped corpus is 4
  (`examples/fizzbuzz.rb`, three `if`s inside `for each`), so no shipped source
  is affected.

## Known gaps / follow-ups

- **Both limits are single shared constants.** 64 is chosen for a 2 MiB
  debug-build stack. A future phase that ran the parser on a dedicated thread
  with a larger stack could raise both substantially without risk; the ceiling
  assertion in `nesting_limit_is_a_sane_small_number` would have to move with
  it.
- **The guards bound recursion, not total work.** A 10-million-token flat file
  still parses in linear time and allocates proportionally; there is no maximum
  source size. Same gap phase 009 recorded for the lexer.
- **`Drop for Expr` is still recursive.** The limit makes the depth safe, but
  the mechanism is indirect. A future phase could give `Expr` an iterative
  teardown (an explicit worklist in a `Drop` impl with `ManuallyDrop`) and then
  the flat-chain case would no longer need the guard at all. Not attempted here:
  it is a refactor, and this is a bug-fix phase.
- **The analyzer and VM recurse over the same AST** and so are now bounded by
  the same 64 levels, which is a strict improvement, but neither has its own
  guard. `MAX_CALL_DEPTH` covers *call* recursion only, not AST-walk recursion.
  Not in this phase's scope.
- **`MAX_BLOCK_DEPTH` counts `... end` forms, not list/record nesting.** A file
  that alternates 60 nested `if`s and 60 nested `[` would still build a 120-deep
  mixed tree; the two counters are independent and neither sees the other. Fine
  in practice — the expensive frames are the expression ones — but it means the
  two limits are not a single global tree-depth budget.
- **`cargo bench` was not added.** The DoD asks for a bench on a 100k-token
  input; `cargo bench` needs nightly's unstable `#[bench]`. It is expressed
  instead as two timing tests (`edge_100k_token_flat_input_parses_linearly`,
  `edge_100k_token_flat_input_scales_sub_quadratically`) that run under stable
  `cargo test` and therefore actually gate the build.
- **Timing tests are wall-clock based.** Both use generous ceilings (20 s
  absolute, 40× for a 10× input) and the relative one warms up the allocator
  first, so they can only fail on genuinely quadratic behaviour — not on a slow
  machine. If they ever prove flaky in CI they should be tightened, not muted.
