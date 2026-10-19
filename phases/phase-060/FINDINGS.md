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
  `module_search_paths` (`src/interpreter.rs:517`) — `modules/<name>.rb`,
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
`src/repl/mod.rs:679`, and `execute_line(&expanded)` is handed that trimmed
text. So a block typed with indentation

```
repeat 2 times
    say "tick"
end
```

is recorded — in the transcript *and* in the history, which takes `line.trim_start()`
at `:688` — as

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

## F5 — `:run` is not in the export, and `:load` is

`src/repl/mod.rs:1584` — `load_file(&mut self, …)` calls `execute_code_from`, which
records the exchange, so `:load notes.rb` appears in `.export` with what it said.
`src/repl/mod.rs:1816` — `run_file(&self, …)` does not: it builds a VM of its own,
runs the file in it, prints, and drops the bindings. There is nowhere for the
transcript to go, because the method holds `&self` and `Transcript::record` needs
`&mut self`.

So a session that did everything with `:run` exports only the lines it typed, and
the same session doing it with `:load` exports the file too. Verified on this
tree:

```
$ printf ':run modules/MathUtils.rb\n.export\n:quit\n' | ./target/debug/rb | grep -c '```redblue'
0
$ printf ':load modules/MathUtils.rb\n.export\n:quit\n' | ./target/debug/rb | grep -c '```redblue'
1
```

**Needed:** a decision first, then code. Either `:run`'s output belongs in the
session's export — which means `run_file` becomes `&mut self` and records what its
VM said, as `load_file` does — or it does not, and the asymmetry is deliberate and
worth a line in `:help`. The phase's definition of done says the export holds "the
session"; whether a file run in a throwaway VM is part of the session is a design
question, not a bug, so it was recorded rather than changed.

## F6 — TAB does not complete inside an opening quote

`path_argument` (`src/repl/completer.rs`) takes everything after the first
whitespace as the fragment, so `:load "my` completes against the literal prefix
`"my` and offers nothing, while `:load my` offers `"my file.rb"` with the quotes
already in it. `:load` opens the quotes it is given — `ReplCommand::unquote_path`
strips a matched pair — so the two halves disagree about the same name.

The whole case is a name with a space in it, and it is rare enough that the
completer offering nothing is not wrong so much as unhelpful. Fixing it is a
predicate change with one test: strip a leading `"` from the fragment before
matching, and keep the quotes in the replacement.

**Needed:** a phase, or a line in this one if the auditor judges it small enough.
It was not done here because the phase's diff was already a fix to the round trip,
and a second behaviour change in the same file is how a bug-fix phase turns into a
refactor.

## F7 — The formatter rewrites words, not only layout — and `rb format` never writes

Found while implementing format-on-enter, both verified on this tree:

```
$ printf 'import files, network as net\n' > i.rb && rb format i.rb
import files, network to net
```

The formatter normalises `import … as x` to `… to x`. Both spellings parse
(`src/parser.rs:654` accepts `TokenKind::As | TokenKind::To`), so the rewrite is
meaning-preserving — but it is a *word* change, and it is the reason
`Repl::format_on_enter` refuses any rewrite whose token stream differs
(`src/repl/mod.rs`). `Repl::same_tokens` is the guard, and this phase added it
along with format-on-enter; `repl::tests::format_on_enter::
edge_an_import_the_formatter_would_rewrite_is_returned_byte_identical` is what
fails if it goes. (Note that `as` and `to` are *different* kinds — `TokenKind::As`
and `TokenKind::To` — so this particular rewrite is caught by the kind
comparison. The `word` comparison beside it is carried by the pairs that share
one kind: `give`/`back`, `might`/`fail`, `mod`/`%`.)

It is also a documentation question: `docs/GRAMMAR.md` should say which spelling
the formatter emits, because a `.rb` file that comes back from `.format` is not
the file that went in.

**Needed:** a docs line, not code. The parser and formatter agree; only the spec
is silent.

## F8 — `rb format <file>` still prints and does not write

This phase added `:format`, which writes. `rb format <file>` (src/lib.rs:248) was
left exactly as it was: it prints the formatted source to stdout and leaves the
file alone. That is the correct Unix behaviour for a filter and the wrong
behaviour for the one command whose name is the file's name, but changing it
would be a breaking CLI change and this phase owns the REPL.

**Needed:** a decision — `rb format -w <file>` beside the existing form, or a
documented note that `rb format` is a filter and `:format` is the one that
writes. Recorded rather than changed: a CLI break smuggled into a REPL phase is
the kind of diff a reviewer has to unpick.

## F9 — Two TAB tests read the real checkout, and one bound is only true today

`src/repl/editor.rs` — `edge_tab_after_a_command_that_takes_a_path_offers_paths`
completes `:load `, `:load s` and `:load sr` against
`completer::current_dir()`, which is `std::env::current_dir()`. It asserts
`Cargo.toml` is offered and that `src/` is offered for `sr`, and that
`complete(":load sr", 8).len() < 20`. All three are statements about *this
checkout*, not about the completer:

- they hold because `cargo test` runs integration tests with the package root as
  the working directory, and would not hold from any other one;
- the `< 20` bound is a claim about how many entries in a repository root start
  with `sr`. Nothing enforces it. A checkout with 20 such files turns the test
  red for a reason that has nothing to do with the code.

`a_file_command_completes_the_paths_it_could_load`
(`src/repl/completer.rs:714`) does the same claim properly: it builds a scratch
directory and asserts against what it put there. The editor test does not, because
`ReplCompleter::complete_path` takes its directory from the process and the test
cannot hand it one.

**Needed:** give the editor's completer the same seam `complete_path_in` already
has — a directory parameter, or a cwd override for the test — so the two levels
test the same thing against a directory they own. It is a test-only change and a
one-parameter signature; it is not in this phase because this phase's diff is a
behaviour fix and re-shaping a test seam under it would make the diff harder to
read than the fix it contains.

## F10 — Nothing reads an exported document back

Recorded by the phase-060 round-1 review. `Transcript::read_markdown`
(`src/repl/transcript.rs`) is the reading half of the export's round trip and has
no caller under `src/` at all: the only callers in the tree are this module's own
tests and `tests/repl_test.rs`, which reaches it as
`redblue::repl::Transcript::read_markdown`.

```
$ grep -rn 'read_markdown' src/ --include='*.rs' | grep -v 'transcript.rs'
(no output)
```

`.export` (`src/repl/mod.rs`, `export_session`) writes a document and stops. `:load`
and `:restore` replay a `:save` *history* file, which is one line per input with
no outputs and no fences — a different format, not a subset of this one. So the
property this phase spent its diff on, "a document this writer wrote reads back as
the session that wrote it", is asserted against a reader that no user can reach:
the round trip is pinned, but nothing in the product performs it.

The obvious consumer is a command that runs an exported document back into a
session, and it is not small — it needs a second history load, a decision about
whether `:load` should accept a Markdown document at all or whether that wants its
own name, and the escaping rules that a hand-edited document brings with it. That
is a phase, not a line in this one.

**Needed:** a phase that adds a reader command (or extends `:load`) and makes
`read_markdown`'s rules the contract between `.export` and it. Until then the
honest statement is the one now in the module doc and in the report: the writer is
the part a session reaches, and the reader is what this file's two halves are
written against.
