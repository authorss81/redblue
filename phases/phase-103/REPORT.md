# Phase 103 — A logging module with levels, destinations and deterministic records

Resume note: the phase implementation was already merged at `HEAD`, so the
gate's diff check saw no implementation change. This run fixes a timestamp
defect on top of the preserved work and adds the state tests the old table was
missing; no preserved code was removed. The original finding no longer
reproduces because `logging` already exists. Review round 2 keeps `stdout`
records immediate, as `print` is, so a record logged before a later output (or a
later failure) is neither reordered nor discarded; the run reset now also
happens on the interpreter thread that executes the program. This run closes the
last entry point that did not start from the documented defaults: the one-shot
`bytecode::vm::run` used by `rb vm`, which had inherited the thread-local
configuration of an earlier program in the same thread. It also adds the
Unicode, trailing-newline and failure rows the old test table did not cover.
Review round 3 closes the last three findings: the nine `logging_*` names are
storage spellings rather than a second bare surface, the non-Unix stream path is
documented and uses a fallible handle write, and the bootstrap's stage-2 run
resets the logging configuration like every other one-shot run.

## What changed
| File | Lines | What |
|---|---|---|
| `src/logging.rs` | +198 | `Level`, `Destination`, thread-local `Config`, threshold and timestamp setters, record formatting, file writes, and fallible stream writes that report a closed or broken descriptor |
| `src/wasm.rs` | +25 −6 | `try_emit`, the fallible form of `emit`, and `is_capturing` so a `stdout` log record stays inside a host capture |
| `src/lib.rs` | +1 | Exports `logging` |
| `src/stdlib.rs` | +83 | `logging` in `MODULES` and `MODULE_MEMBERS`; the nine internal names are module-only, with signatures and effectful entries |
| `src/runtime.rs` | +87 | Shared dispatch for `logging.debug/info/warn/error`, `set_level`, `level`, `set_timestamps`, `set_destination` and `reset` |
| `tests/stdlib_modules_test.rs` | +512 | Fifteen `logging_module` tests covering thresholds, destinations, timestamps, parity, ordering, closed streams and failures |
| `SPEC.md` | +45 | `logging` section: level order, `off` default, destinations, formats, determinism and write failures |
| `README.md` | +18 | `logging` quick-reference block and default-behaviour paragraph |
| `SURVEY.md` | +47 −47 | Inventory re-anchored to 114 builtins, 15 modules and 101 dotted members; logging gap removed |
| `tests/stdlib_module_names_test.rs` | +5 | A documented `logging` call that runs |
| `tests/stdlib_module_docs_test.rs` | +57 | Answerable rows for all nine logging builtins |
| `tests/doc_blocks_runs.txt` | +13 −12 | New SPEC block classified RUNS; shifted SPEC blocks re-anchored |
| `tests/doc_blocks_fragments.txt` | +4 −4 | SPEC fragment anchors moved with the new section |
| `tests/doc_blocks_known_fail.txt` | +1 −1 | SPEC known-failure anchor moved with the new section |
| `tests/unreviewed_work_ack_test.rs` | +1 | `resume phase-103:` added to the delivery-subject rule |
| `src/logging.rs` (resume) | +51 −4 | a pre-epoch clock instant keeps its timestamp; a captured `stdout` record stays immediate, like `print`, rather than being deferred into the final `say` flush |
| `src/lib.rs` (resume) | +6 | both program entry points reset the logging configuration before a run |
| `src/interpreter.rs` (resume) | +7 | the spawned interpreter thread, and the inline wasm fallback, reset the logging configuration on the thread that actually runs the program |
| `src/runtime.rs` (resume) | +32 | a test-only clock injection so the pre-epoch path is exercised through `current_instant`, not only the private formatter |
| `tests/stdlib_modules_test.rs` (resume) | +158 −3 | reset restores all three defaults; a refused setter leaves the previous configuration intact; a run starts from the defaults; a `stdout` record is immediate like `print` |
| `tests/wasm_test.rs` (resume) | +29 | a `stdout` record written before a runtime failure survives it, like `print` |
| `src/bytecode/vm.rs` (resume 2) | +9 | the one-shot `bytecode::vm::run` entry point resets the logging configuration, so `rb vm` starts from the documented defaults; `BytecodeVm::run` deliberately does not reset because the REPL reuses one VM across inputs |
| `tests/stdlib_modules_test.rs` (resume 2) | +139 | four tests: the bytecode entry point starts from the defaults, Unicode messages round-trip, a trailing newline is preserved, and an unknown level/blank destination is a failure |
| `src/stdlib.rs` (resume 3) | +25 −37 | the nine `logging_*` names leave `BUILTIN_NAMES` and `builtins()`; they are module-only, resolve through `MODULE_MEMBERS`, and are allowed by the effectful-name audit |
| `src/logging.rs` (resume 3) | +34 −5 | non-Unix streams go through a fallible handle-write loop; the platform limitation is documented without weakening the Unix closed-descriptor path |
| `src/bootstrap.rs` (resume 3) | +5 | the bootstrap stage-2 run resets logging before it runs, so it does not inherit a previous thread's configuration |
| `SPEC.md` (resume 3) | +6 −1 | the logging section states the non-Unix closed-stream limitation and where that path is exercised |
| `SURVEY.md` (resume 3) | +13 −12 | registry count is 114, and the nine module-only logging names are recorded as having no flat spelling |
| `tests/stdlib_modules_test.rs` (resume 3) | +69 | the nine bare `logging_*` names are `Unknown function` on both engines, and a bootstrap stage-2 run starts from the logging defaults |

## Behaviour
`logging.debug`, `logging.info`, `logging.warn` and `logging.error` take one
message. A record is written only when its level is at or above the current
threshold. The levels are ordered `debug`, `info`, `warn`, `error`; the threshold
is one of those four or `off`, and the default is `off`. A library can therefore
add logging calls without changing what a program that has not opted in emits.

`logging.set_level` accepts the five threshold names and
`logging.set_destination` accepts `stderr`, `stdout`, or any other non-empty text
as a file path. `logging.level` reads the threshold back, `logging.set_timestamps`
turns timestamps on or off, and `logging.reset` restores `off`, `stderr` and
timestamps on. Invalid names, empty destinations, wrong argument types and extra
arguments are catchable `Runtime` errors that name the builtin.

The default destination is `stderr`, not the program's normal output stream. A
record has the form `[LEVEL] message` in deterministic mode and
`[YYYY-MM-DDTHH:MM:SS.mmmZ] [LEVEL] message` when timestamps are on, always
ending with one newline. The timestamp is read from the same clock as
`time.now()`, including the playground clock a host supplies through
`rb_set_clock`, rather than from a second clock the logging module owns. A host
with no clock writes the record without a timestamp rather than failing the
run, the same fallback SPEC documents; `time.now()` still answers that there is
no clock. An instant before the Unix epoch is a real instant, not a missing
clock: it keeps its timestamp (as 1969-12-31T…Z) rather than losing the field.
An empty or multi-line message is written unchanged.
File destinations append one record per write through the same `vfs` path as
`files.append`, so a record can be read by either engine. A file that cannot be
written raises a catchable `IoError` naming `logging`; it does not abort the
process.

The `stdout` and `stderr` destinations are written through a fallible path too,
so a descriptor that is closed, read-only, or a pipe with no reader raises the
same catchable `IoError` instead of silently losing the record. On Unix the
record is written to the descriptor directly because the standard library
treats `EBADF` on a standard stream as success; a `stdout` record stays inside a
host capture only when the calling thread belongs to the run that opened it, so
one run's logging cannot be collected by another run's capture.

Every program run starts from those defaults. The configuration is thread-local,
so the reset happens at the tree-walker entry point (which is the thread the
inline wasm run shares), on the interpreter thread a native run spawns, and at
the one-shot `bytecode::vm::run` entry point `rb vm` uses. The REPL's resident
`BytecodeVm` is the exception by design: one VM spans many inputs, so a
`logging.set_level` in one line is visible to the next. Without the resets,
the second program in one page, or a later program in one process, would inherit
the first one's threshold, destination and timestamp mode. The reset is part of
the run, not something a program or a test has to remember.

A captured `stdout` record is written when the call runs, exactly as `print` is
and unlike `say`, which the interpreter buffers until the program finishes. It
therefore keeps its place among printed output, comes before every `say` line,
and survives a later failure the same way a `print` does. The configuration is
thread-local and both engines dispatch through the same `runtime::builtin` arms.
There is no second formatter or threshold check in the bytecode VM, so the same
program produces the same records under both engines.

## Tests added
| Test | Edge class covered |
|---|---|
| `records_at_or_above_the_threshold_reach_a_file_destination` | happy path; below, at and above the threshold |
| `edge_every_adjacent_threshold_pair_filters_exactly` | every adjacent pair plus `off` |
| `default_logging_emits_nothing_into_a_programs_output` | default `off`; no file, no stdout, no stderr record |
| `edge_the_stdout_destination_joins_the_programs_captured_output` | explicit stdout destination and captured output |
| `edge_the_default_destination_is_stderr_not_stdout` | native CLI split between stdout and stderr |
| `a_record_carries_a_timestamp_unless_deterministic_mode_omits_it` | timestamp shape and deterministic mode |
| `the_tree_walker_and_the_bytecode_vm_emit_equal_records` | cross-engine output and cross-engine files |
| `edge_empty_and_multiline_messages_are_written_unchanged` | empty message, embedded newline and trailing newline |
| `edge_nested_calls_interleave_records_in_call_order` | nested calls and stable ordering |
| `edge_an_unwritable_destination_is_a_catchable_error` | missing directory, `IoError`, both engines |
| `edge_invalid_levels_and_destinations_are_refused` | bad level, empty destination, wrong types and arity |
| `edge_a_closed_stderr_is_a_catchable_error` | a read-only standard stream, `IoError`, not a silent drop |
| `edge_a_closed_stdout_is_a_catchable_error` | a read-only output stream, `IoError`, caught and recorded |
| `edge_a_readerless_pipe_is_a_catchable_error` | a pipe with no reader, `EPIPE`, caught and recorded |
| `edge_a_directory_destination_is_a_catchable_error` | a directory as a file destination, both engines |
| `edge_an_unrelated_thread_is_not_part_of_an_open_capture` | capture membership is scoped to the run that opened it, not the process |
| `edge_a_pre_epoch_instant_keeps_its_timestamp` | numeric boundary: an instant before the Unix epoch keeps a timestamp rather than being treated as "no clock" |
| `edge_a_pre_epoch_host_clock_reaches_the_timestamp_formatter` | numeric boundary: the same instant reaches the formatter through `runtime::current_instant`, not only through the private helper |
| `edge_a_run_starts_from_the_default_logging_configuration` | state: a run resets the threshold, destination and timestamp mode before the program starts |
| `edge_a_stdout_record_is_written_when_the_call_runs` | ordering: a record is written at its call, like `print` — before a later `say` and after an earlier `print` |
| `edge_a_failure_keeps_a_stdout_record_written_before_it` | failure: a `stdout` record logged before a runtime error is still in the captured output |
| `edge_a_refused_setting_leaves_the_previous_configuration_intact` | malformed input / state: a refused level and destination leave the previous threshold and file destination unchanged, on both engines |
| `edge_reset_restores_the_default_configuration` | boundary / state: reset restores `off`, `stderr` and timestamps on, proven through the CLI |
| `edge_the_bytecode_entry_point_starts_from_the_default_logging_configuration` | state / boundary: a second `bytecode::vm::run` starts at `off` and writes no record to the first program's destination |
| `edge_unicode_messages_are_written_unchanged` | Unicode: CJK, RTL, emoji and a combining mark round-trip byte-for-byte on both engines |
| `edge_a_trailing_newline_in_a_message_is_preserved` | empty / boundary: a message that already ends with a newline keeps it and the record adds exactly one more |
| `edge_an_unknown_level_and_a_blank_destination_are_refused` | malformed input: the setters return an error, the threshold is unchanged, and a program sees the refusal as an error |
| `edge_the_logging_calls_have_no_bare_spelling` | module surface: each of the nine internal names answers `Unknown function` on both engines |
| `edge_the_bootstrap_stage_two_run_starts_from_the_default_logging_configuration` | state / entry point: a stage-2 bootstrap run does not inherit the previous thread's threshold or destination |

## Mandatory edge-case matrix
| Row | Status |
|---|---|
| empty | covered — an empty message writes an empty body; an empty destination is refused |
| singleton | covered — one record at the threshold is written once, and one below it writes nothing |
| boundary | covered — every adjacent threshold pair, the highest level and `off`; a reset returns to the boundary defaults; a second one-shot bytecode run starts from them too |
| out_of_bounds | covered — an unknown level and a destination whose directory does not exist, or is a directory, are refused |
| type_mismatch | covered — a number where a level or destination is wanted, a non-boolean timestamp switch, and extra arguments to `level`/`reset` |
| numeric_boundary | covered — a number is not accepted as a level or destination; logging performs no numeric formatting; a pre-epoch clock instant keeps a negative-second timestamp instead of dropping it |
| unicode | covered — the message is opaque text, and a test pins CJK, RTL, emoji and a combining mark as written byte-for-byte on both engines |
| nesting_recursion | covered — a function called inside another writes its records in call order |
| duplicate_missing_keys | N/A — logging has no user-supplied keys or collections |
| malformed_input | covered — unknown level, empty destination, missing message and wrong argument types |
| resource_limit | covered — a closed, read-only or readerless stream and an unwritable file are catchable `IoError`s, not aborts or silent drops |

## Gates
| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | pass — 2518 passed, 0 failed, 0 ignored |
| `../rbops/verify.sh "phase-103"` | pass |


## Invariants touched
- `stdlib::MODULES` gains `logging`; `MODULE_MEMBERS` gains its nine documented
  members. The analyzer's existing dotted-call path needs no grammar change.
- `BUILTIN_SIGNATURES` and `EFFECTFUL_BUILTINS` name the nine `logging_*`
  entries, and both engines dispatch them. `BUILTIN_NAMES` and `builtins()`
  deliberately omit them: they are module-only storage spellings, reachable as
  `logging.<member>` and refused as bare names. Documentation, reachability and
  effect auditing agree with dispatch.
- No lexer, parser, analyzer, `Value` or bytecode instruction changed. Both
  engines already call the shared `runtime::builtin` function.
- The default is deliberately inert: threshold `off`, destination `stderr`, and
  timestamps on. A program must enable a level before any record exists.
- No new dependency: timestamps use `chrono`, file writes use the existing `vfs`
  abstraction, and the Unix descriptor write uses the existing `libc`
  target-dependency. `say`/`print` still go through `emit` unchanged; only
  logging uses the fallible `try_emit`.
- No test was skipped, ignored or weakened, and no lint suppression was added.

## Existing test reconciliation
- `tests/stdlib_module_names_test.rs` gains the `logging` module and a call that
  answers `off` before any configuration. The existing bidirectional module and
  member checks then cover the new surface.
- `tests/stdlib_module_docs_test.rs` gains an answerable program row for every
  `logging_*` builtin.
- `tests/doc_blocks_runs.txt` classifies the new `SPEC.md:1231` block as RUNS and
  re-anchors later SPEC blocks whose line numbers moved. The untouched README
  example uses the document's existing plain fence, so it is not a
  `redblue`-tagged block for the executor.
- `tests/doc_blocks_fragments.txt` and `tests/doc_blocks_known_fail.txt` were
  re-anchored, not reclassified. The phase-088 scanner exposed five logging
  tests whose only success check was `is_ok()`; each now also compares the
  program's captured output, so the scanner remains at zero vacuous tests.
- `SURVEY.md` now states 114 builtins, 15 modules and 101 dotted members, lists
  the logging row, removes the old "no logging" gap, and re-anchors citations
  displaced by the new registry entries. The nine `logging_*` names are recorded
  as module-only, not as flat builtins.
- `tests/unreviewed_work_ack_test.rs` gains the `resume phase-103:` delivery
  subject. The rule already covers every phase in flight; this phase's delivery
  commit was the one subject it had not been told about yet. No entry was
  removed and no assertion was loosened.

## Known gaps / follow-ups
- Configuration is thread-local. That isolates tests and matches the
  single-threaded CLI, but two host threads do not share a threshold or
  destination.
- There is no rotation, size limit, retention policy, file locking or structured
  field support. Each enabled call appends one record.
- The timestamp format is fixed: UTC with millisecond precision. There is no
  custom format, local timezone, or caller-supplied clock.
- `stdout` and `stderr` are reserved destination names, so a file literally
  named `stdout` or `stderr` cannot be selected without a path prefix.
- There is no per-module logger or level filter; one threshold and one
  destination apply to every call in the thread.
- The REPL keeps one `BytecodeVm` across inputs, so it intentionally does not
  reset logging per input; a user's `logging.set_level` survives to the next
  line. Every one-shot run (`rb run`, `rb vm`, `run_program`, the bootstrap's
  stage-2 run) does reset.
- Closed or broken `stdout`/`stderr` descriptors are exercised on Unix, where
  the suite can hand a child a read-only descriptor or a readerless pipe. Other
  targets use the standard handle's fallible write and cannot be tested that way
  without a platform binding.
