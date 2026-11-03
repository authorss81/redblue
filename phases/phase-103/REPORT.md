# Phase 103 — A logging module with levels, destinations and deterministic records

## What changed
| File | Lines | What |
|---|---|---|
| `src/logging.rs` | +151 | `Level`, `Destination`, thread-local `Config`, threshold and timestamp setters, record formatting, stdout/stderr/file writes |
| `src/lib.rs` | +1 | Exports `logging` |
| `src/stdlib.rs` | +83 | `logging` in `MODULES` and `MODULE_MEMBERS`; nine builtins, names, signatures and effectful entries |
| `src/runtime.rs` | +87 | Shared dispatch for `logging.debug/info/warn/error`, `set_level`, `level`, `set_timestamps`, `set_destination` and `reset` |
| `tests/stdlib_modules_test.rs` | +391 | Eleven `logging_module` tests covering thresholds, destinations, timestamps, parity, ordering and failures |
| `SPEC.md` | +45 | `logging` section: level order, `off` default, destinations, formats, determinism and write failures |
| `README.md` | +18 | `logging` quick-reference block and default-behaviour paragraph |
| `SURVEY.md` | +47 −47 | Inventory re-anchored to 123 builtins, 15 modules and 101 dotted members; logging gap removed |
| `tests/stdlib_module_names_test.rs` | +5 | A documented `logging` call that runs |
| `tests/stdlib_module_docs_test.rs` | +57 | Answerable rows for all nine logging builtins |
| `tests/doc_blocks_runs.txt` | +7 −1 | New SPEC block classified RUNS; shifted SPEC blocks re-anchored |
| `tests/doc_blocks_fragments.txt` | +4 −4 | SPEC fragment anchors moved with the new section |
| `tests/doc_blocks_known_fail.txt` | +1 −1 | SPEC known-failure anchor moved with the new section |

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
ending with one newline. An empty or multi-line message is written unchanged.
File destinations append one record per write through the same `vfs` path as
`files.append`, so a record can be read by either engine. A file that cannot be
written raises a catchable `IoError` naming `logging`; it does not abort the
process.

The configuration is thread-local and both engines dispatch through the same
`runtime::builtin` arms. There is no second formatter or threshold check in the
bytecode VM, so the same program produces the same records under both engines.

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

## Mandatory edge-case matrix
| Row | Status |
|---|---|
| empty | covered — an empty message writes an empty body; an empty destination is refused |
| singleton | covered — one record at the threshold is written once, and one below it writes nothing |
| boundary | covered — every adjacent threshold pair, the highest level and `off` |
| out_of_bounds | covered — an unknown level and a destination whose directory does not exist are refused |
| type_mismatch | covered — a number where a level or destination is wanted, a non-boolean timestamp switch, and extra arguments to `level`/`reset` |
| numeric_boundary | covered — a number is not accepted as a level or destination; logging performs no numeric formatting |
| unicode | N/A — messages are opaque text passed through unchanged; this facility adds no Unicode-specific boundary |
| nesting_recursion | covered — a function called inside another writes its records in call order |
| duplicate_missing_keys | N/A — logging has no user-supplied keys or collections |
| malformed_input | covered — unknown level, empty destination, missing message and wrong argument types |
| resource_limit | N/A — each call writes one bounded record and the module retains no history |

## Gates
| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | pass — 79 test binaries, 2497 passed, 0 failed, 0 ignored |
| `../rbops/verify.sh "phase-103"` | pass |

## Invariants touched
- `stdlib::MODULES` gains `logging`; `MODULE_MEMBERS` gains its nine documented
  members. The analyzer's existing dotted-call path needs no grammar change.
- `BUILTIN_NAMES`, `BUILTIN_SIGNATURES` and `EFFECTFUL_BUILTINS` gain the nine
  `logging_*` entries, so documentation, reachability and effect auditing agree
  with dispatch.
- No lexer, parser, analyzer, `Value` or bytecode instruction changed. Both
  engines already call the shared `runtime::builtin` function.
- The default is deliberately inert: threshold `off`, destination `stderr`, and
  timestamps on. A program must enable a level before any record exists.
- No new dependency: timestamps use `chrono`, which was already in `Cargo.toml`;
  file writes use the existing `vfs` abstraction.
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
- `SURVEY.md` now states 123 builtins, 15 modules and 101 dotted members, lists
  the logging row, removes the old "no logging" gap, and re-anchors citations
  displaced by the new registry entries.

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
