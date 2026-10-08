# FINDINGS — phase-033

Defects found while testing the REPL that this phase did **not** fix, because
fixing them is not "test the REPL and route it through `ReplCommand`". Each one
is anchored to a line read during this phase and is a candidate for the auditor
to promote to a phase.

**Status after the second review round:** #1, #2, #4 and #5 were fixed there and
say so below. #3 was fixed in the first round. #6 is the only one still open.

## 1. The REPL cannot use a variable it just set — `src/analyzer.rs`

```
$ printf 'set caught to yes\nsay caught\n' | rb
>>> Error: AnalyzerError: Unknown variable 'caught'
```

`Repl::run_code` (`src/repl/mod.rs:422`) lexes, parses and analyzes each line on
its own, and the analyzer's scope is built from that line alone. The VM is
reused across lines, so the *value* survives — but nothing tells the analyzer the
name exists, so the very next line is rejected statically. The same happens to a
function defined with `to`, and to an `import`ed module.

This makes the REPL unusable for the thing a REPL is for: state does not survive
a line. `2 + 3` works only because it has no free names. Note that `:vars` is
downstream of the same gap — see #2.

**Fixed in the second review round.** `Analyzer::with_bound_names` and
`analyzer::analyze_with_bound_names` seed the program scope from the session's own
bindings, and `Repl::run_code` passes `Repl::session_names` — `Vm::user_names`
plus the REPL's own `_` — into every line. `set x to 1` then `say x` runs, a `to`
is callable on the line after it, and an `import`ed module is readable from the
line that imported it. A name the session never bound is still reported as
unknown: the check is carried across lines, not switched off.

## 2. `:vars` reports only the last result value, under `_` — `src/repl/mod.rs`

`Repl::execute_code` (`src/repl/mod.rs:396`) inserts into `self.variables`
exactly one key, `"_"`, and only when a line produced a non-`Nothing` value:

```
$ printf 'set x to 1 + 1\n:vars\n' | rb
>>> No variables defined.
```

`print_variables` (`src/repl/mod.rs:526`) is therefore reachable with either
nothing or a single `_` entry, and it iterates a `HashMap` to print — which is
fine at 0 or 1 entries and nondeterministic at 2. Both halves of this should
change together: read the session's names from the VM, and sort before printing.
`tests/repl_test.rs:repl_shows_the_result_of_an_expression_under_vars` pins the
current `_` behaviour deliberately, so that phase has a red test to start from.

**Both halves are fixed.** The ordering half after the first review round:
`Repl::variable_lines` sorts the names before printing. The content half after the
second, once #1 made a `set` a name the next line can read:
`Repl::session_bindings` reads the VM's bindings as well as the REPL's own map, so
`:vars` lists what the session has bound — `set alpha to 1` then `:vars` answers
with `alpha` rather than with `No variables defined.`

## 3. A stray `end` leaves the REPL stuck in continuation mode — `src/repl/mod.rs`

The multi-line detector was a suffix heuristic (`Repl::needs_continuation`,
`src/repl/mod.rs`, called from the read loop at `src/repl/mod.rs:144`; the heuristic
it replaced is now `parser::open_block_depth`, src/parser.rs:2036):
a line opens a block if it ends with `then`/`end`/`to`/`else`/`while`/`repeat`.
Entering `..  ` mode is therefore easy and leaving it needs the exact line `end`.

```
$ printf 'say "hi"\nend\nsay "bye"\n' | rb
>>> hi
>>> ..  ..  Goodbye!      <- "bye" was buffered and never executed or reported
```

Two consequences, both from the heuristic rather than from a parse: a stray `end`
captures every later line, and a block opener the heuristic does not know about
(`try`, `to greet(name)`, `for each i from 1 to 10`) runs alone and fails with a
parse error. The honest fix is to ask the parser whether the buffer is complete —
open/closed `end` counting or a `Parser::is_incomplete(&tokens)` predicate —
instead of matching on the last word of a line. `docs/GRAMMAR.md:296` is where
the block forms are listed.

**Fixed after review.** `parser::open_block_depth(&tokens)` counts unclosed blocks
from the token stream, and `Repl::needs_continuation` asks it. A stray `end` is
reported as a parse error and the session carries on, `try`, `to greet(name)`,
`for each i from 1 to 10`, `unless` and `while` all open continuation mode, and a
block typed on one line needs no `end`. Two things the count has to know: an
opener is counted only where a statement can begin, so the range bound in `for
each i from 1 to 10` and the alias in `import MathUtils to M` are not function
declarations; and `else`/`catch`/`finally` are branches rather than statements,
so the block inside `else if x then` is still counted. Counting tokens rather than
text also stops `say "end"` from closing a block.

The other half of this finding — a block the user never closed being dropped
silently at EOF — is fixed too: `Repl::report_unclosed_block` prints the buffered
lines and says they were never closed, where the session used to print `Goodbye!`
and lose them.

## 4. `say` does not interpolate, though the REPL's own help says it does — `src/repl/mod.rs`

`print_help` (`src/repl/mod.rs:454`) and `show_examples` (`src/repl/mod.rs:768`) both tell the user
`say "Hello, {{name}}!"`, and `AGENTS.md` lists `{interp}` string syntax as an
invariant:

```
$ printf 'set n to "World"\nsay "Hello, {{n}}!"\n' | rb
>>> Hello, {{n}}!
```

**Fixed in the second review round**, by removing the promise. `{{name}}` is not
Redblue — the grammar has no interpolation production and `Expr::InterpolatedText` is
never built by the parser — so the help was wrong, not `say`. `:example` and the
help's quick reference are now `EXAMPLES` and `QUICK_REFERENCE`: source rather than
prose, with a test that runs every entry, so neither can show something the
session refuses. `say "Hello, " + name + "!"` is shown instead. The quick
reference had the same class of defect — it printed `if x > 5 then`, which is not
Redblue either.

If string interpolation is wanted as a language feature it is its own phase, and
`Expr::InterpolatedText` plus the arm in `Vm::evaluate` that already matches it are
where it would land.

## 5. `ReplHistory::save_to_file`/`load_from_file` took `&str`, not a path — now `src/repl/history.rs:76`

The signature as it stood when this finding was filed:

```rust
pub fn save_to_file(&self, path: &str) -> std::io::Result<()> {
```

The REPL hands these the raw word after `:save`, so a path that is not UTF-8
cannot be saved at all, and neither call site has to deal with a `Path`. **Fixed in the second review round.** Both take `impl AsRef<Path>`, the test
helper that lossily converted a `PathBuf` into a `&str` is gone, and a
`#[cfg(unix)]` test round-trips a history through a file whose own *name* is not
UTF-8 — which the `&str` signature could not express at all.

## 6. There is no way to reach the completer from the binary — `src/repl/mod.rs:111`

`Repl::complete` exists and is tested, but `run` reads with
`std::io::stdin().read_line`, which gives no line editing and no completion
hook. Wiring it needs either `rustyline` as a dependency or a hand-rolled
raw-mode reader. This phase made the completer *correct and reachable as API*; it
did not make it reachable *for a user*, and no test here pretends otherwise.