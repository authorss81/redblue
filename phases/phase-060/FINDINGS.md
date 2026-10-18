# Phase 060 — FINDINGS

Work that this phase found but did not do, each one with the place to look. None
of these belongs to phase-060: they are recorded so the auditor can decide
whether to promote them to phases.

## F1 — `./rbops/verify.sh` cannot run in this checkout

`ls rbops` → *No such file or directory*, and `./rbops/verify.sh phase-060` →
*No such file or directory*. The tree carries `phases/`, `AGENTS.md` and
`src/`, and the pipeline that dispatches the phase lives elsewhere.

Recorded identically by phases 057, 058 and 059, so this is the third phase to
hit it and not a one-off. What was run here instead: `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets` (1825
passed, 0 failed), `cargo test --doc` (2 passed), and every file in `examples/`
and `modules/` through `rb run` by hand (9 of 9 exit 0).

**Needed:** a decision about whether a phase is allowed to claim the gate when
the gate is not present in the tree it was given, or whether the dispatching
pipeline must mount `rbops/` into the working directory. Right now three
reports in a row say the same thing and a reviewer has to take it on trust each
time.

## F2 — A filename typed inside a Redblue program is still typed blind

TAB now completes the *argument* of `:load`, `:save`, `:run`, `:restore` and
`.export` (`src/repl/completer.rs`: `path_argument` + `complete_path_in`). What
it does not do is complete anything that names a file *inside a program*, and
there is a short list of those in the tree, all verified on this tree:

- `files.read("…")`, `files.write("…", …)`, `files.lines(…)` and the rest of the
  files module (`AGENTS.md` §Standard Library) take paths as string arguments to
  a call, not as arguments to a command, so the gate added here deliberately
  leaves them alone.
- `import MathUtils` names a *module*, not a path, and the parser rejects a
  quoted one: `printf 'import "modules/MathUtils"\n' > t.rb && rb run t.rb`
  answers `ParserError: Expected module name`. The module then resolves through
  `module_search_paths` (`src/interpreter.rs:516`) — `modules/<name>.rb`,
  `./modules/<name>`, `<name>` — so what a user types is a *name*, and the names
  that resolve are the `.rb` files in `modules/` (2 in this checkout:
  `MathUtils.rb`, `SuiteKit.rb`). TAB could offer those; it cannot do it today,
  because the completer knows nothing about the module search path.

Completing a string argument *inside a Redblue expression* is a different piece of
work from completing a command argument: it has to know where the string ends,
what is inside it, and that the user is typing text rather than a filename at
all. That is why it is not in this phase.

**Needed:** a phase that extends the completer to string arguments of the files
module and to `import` names, with the three properties this one has — the offset
the completion replaces, the quoting rule, and a test per shape. `path_argument`
is the seam: it answers "where is the cursor in something that is a path", and a
second predicate over expressions and over the module table would reuse it.

## F3 — An exported block has no indentation, because the line that ran had none

The read loop trims each line before running it: `let input = line.trim();` at
`src/repl/mod.rs:667`, and `execute_line(&expanded)` is handed that trimmed
text. So a block typed with indentation

```
repeat 2 times
    say "tick"
end
```

is recorded — in the transcript *and* in the history, which takes `line.trim_start()`
at `:676` — as

```
repeat 2 times
say "tick"
end
```

Redblue does not read indentation, so nothing about the run changes. But an
exported block that a reader can see was indented and is not reads as a defect
of the export, and `transcript::a_block_is_one_exchange_holding_every_line_it_was_typed_on`
pins the *with*-indentation shape only because it builds a `Transcript` directly
rather than going through the read loop.

**Needed:** a decision, not code. Either leave it — the transcript is the source
that ran, and the history has always been trimmed the same way — or record the
line as typed rather than as trimmed, which changes `:history`, `!!` and
`keeps_argument_whitespace` along with it. The second is a phase of its own and
is not this one.

## F4 — `main` was red before this phase started, in two files this phase did not own

Running the four gates first, as the resume instructions require, reported two
failures that no part of phase-060 had caused. Both were in `tests/` and both were
compile errors under the gate, not failing assertions:

| File | Reported by | From |
|---|---|---|
| `tests/unreviewed_work_ack_test.rs` | `cargo fmt --check` (a diff) and `clippy -D warnings` (`assert!(true)` is `clippy::assertions_on_constants`, so the test target does not compile) | `222aa83` "tests: replace invalid placeholder causing compile failure" |
| `tests/memory_model_test.rs` | `clippy -D warnings` (`Arc::new(Node { .. })` is `clippy::arc_with_non_send_sync`: `Node` holds a `RefCell`, so `Arc<Node>` is neither `Send` nor `Sync` and the test target does not compile) | `2948f23` "Document the memory model, and pin every claim in it" |

Both were fixed here rather than reported and abandoned, because a phase cannot be
green while the gates it must pass are red, and because both fixes are small,
`#[allow]`-free and assertion-preserving: the node helper holds `Rc<Node>` instead
of `Arc<Node>` (the strong-count and weak-count assertions are the same numbers
from the same counter), and the placeholder test became three real assertions
about `tests/checkpoint_ancestry_ack.txt`.

**Needed:** a phase that asks "does `main` build under `-D warnings`?" *before* a
phase starts, and one that makes `cargo test` and the linter the same commit's
business. A commit can satisfy `cargo test` and still leave `main` unable to
compile under the gate, because `cargo test` compiles the library's tests with the
test's own lint level and `clippy --all-targets` compiles every target with
warnings denied. That gap is how two red gates reached `main` unnoticed.

The deeper question the same finding raises: a fix like this one is a phase that
edits a file it does not own, in a diff whose subject is `.export`. It was done
because the alternative was a red gate and an honest report of a red gate — but a
pipeline that fixed the gates before dispatching a phase would not have to choose.
