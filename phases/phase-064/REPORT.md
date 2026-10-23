# Phase 064 — Close the file loop: `.reload`, `.format`, and format-on-enter

## What changed

| File | Lines | What |
|---|---|---|
| src/repl/commands.rs | +38 −0 | `Reload` and `Format(Option<String>)` variants, their command-table rows (`:reload`, `.reload`, `:format`, `.format`), and `Format` in `argument_text` |
| src/repl/mod.rs | +221 −4 | `LastFile` (loaded vs run), `last_file`/`format_on_entry` session fields, `.reload` dispatch, `.format` buffer formatting and `on`/`off` toggle, format-before-execute hook in `execute_line`, path in `:load`/`:run` failures, and eight unit/integration-facing behaviours tested in the module |
| tests/repl_test.rs | +111 −0 | end-to-end sessions for reload (success, no file, deleted file), empty-buffer `.format`, and a formatter-rejected line under the toggle |

`.reload` replays the last `:load` through `load_file` and the last `:run` through `run_file`, so reloading does not quietly change where a file's bindings land. Both reuse the existing formatter; nothing about what `formatter::format` accepts changed.

## Tests added

Three new `#[test]` functions in `src/repl/mod.rs` and seven new `#[test]`
functions in `tests/repl_test.rs`; zero ignored, zero skipped, zero
`allow(clippy::` suppressions.

| Test | Edge class covered |
|---|---|
| `format_prints_the_canonical_block_without_running_it` | boundary — ugly-but-valid buffer becomes canonical while bindings stay unchanged |
| `edge_format_with_an_empty_buffer_is_a_no_op` | empty — no block, no crash, nothing run |
| `edge_format_on_entry_survives_reset_on_both_sides` | singleton/state — toggle on and off both survive `:reset`; asserted on both sides |
| `reload_reruns_the_last_loaded_file_and_reproduces_its_output` | singleton — one remembered file runs twice and its output is reproduced |
| `edge_reload_with_no_file_loaded_names_load` | resource/state failure — no remembered file is a clean error naming `:load`, never a panic |
| `edge_reload_after_the_file_was_deleted_names_its_path` | resource_limit — a deleted file is an I/O error carrying the path |
| `edge_format_with_an_empty_buffer_is_a_no_op` (integration) | empty — the real session prints no error and the next line still runs |
| `edge_format_toggle_leaves_a_line_the_formatter_rejects_to_fail` | malformed_input — a formatter error falls back to the typed line and still fails |
| `reload_reruns_the_last_run_file_and_reproduces_its_output` | review round 1 — the `LastFile::Ran` branch: a `:run` file is run again, not reloaded into the session |
| `edge_format_with_an_invalid_argument_is_refused_and_the_session_goes_on` | review round 1 — malformed input: a `.format` word other than `on`/`off` is a reported error and leaves the preference alone |

### Mandatory edge-case matrix

- **empty** — covered: both `edge_format_with_an_empty_buffer_is_a_no_op` tests (unit and integration).
- **singleton** — covered: one loaded file and one formatted line in the reload/format tests.
- **boundary** — covered: a valid buffer is canonicalised without running; the toggle preserves a line the formatter accepts.
- **out_of_bounds** — N/A: these commands take no index; `.reload` names a path and `.format` acts on the buffer.
- **type_mismatch** — N/A: there are no typed operands; the only inputs are a command word (`on`/`off`) and source text, and a wrong word is reported as usage.
- **numeric_boundary** — N/A: no numbers are parsed or computed by this change.
- **unicode** — N/A: the formatter is reused unchanged; this phase adds no new text transformation beyond calling it.
- **nesting_recursion** — N/A: `.format` delegates whole-buffer formatting to `rbfmt`; it adds no recursion of its own.
- **duplicate_missing_keys** — N/A: no records or keys are involved.
- **malformed_input** — covered: `edge_format_toggle_leaves_a_line_the_formatter_rejects_to_fail` proves a rejected line is still run (and still fails) under the toggle. A rejected held buffer is likewise reported by `.format` and never executed.
- **resource_limit** — covered for the file channel by `edge_reload_after_the_file_was_deleted_names_its_path`; the existing block-size limits in `execute_line` still apply to the buffer, now to its formatted form too.

### Review round 1 — findings fixed

The reviewer returned two MAJOR findings and one MINOR; all three are fixed
here, and nothing was weakened to fix them.

| # | Severity | Finding | Fix |
|---|---|---|---|
| 1 | MAJOR | `src/repl/commands.rs:858` — the alias-table test did not cover `.reload`/`:reload` or `.format`/`:format`, and its carried-argument match did not read `Format(Some(..))`. | Six rows now pin both commands through both sigils and both `.format on|off` shapes; the match reads the carried `Format` word. |
| 2 | MAJOR | `tests/repl_test.rs` — no integration test reached the `LastFile::Ran` branch of `reload_file`. | `reload_reruns_the_last_run_file_and_reproduces_its_output` runs a `:run` file twice through `.reload` and asserts both the program output and the run message appear twice. |
| 3 | MINOR | `src/repl/mod.rs:1282` — `.format` with an invalid argument printed usage with no error state and had no test. | The argument is now reported as an `Error:` naming the word that was not `on` or `off`, and the integration test proves the session still runs the next line without changing the preference. |

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2063 passed, 0 failed, 0 ignored |
| cargo test --doc | 2 passed, 0 failed |
| ../rbops/verify.sh phase-064 | pass — `VERIFY PASS — phase-064` |

## Invariants touched

- None of the language surface: no grammar, keyword, `Value` variant or `Error` variant changed, and `examples/*.rb` and `modules/*.rb` are untouched.
- The formatter's accepted language is unchanged; `.format` and format-on-enter call `crate::formatter::format` as it is.
- Existing command messages that mention a failed `:load` keep the `Could not load file:` prefix; the path is now included after it so `.reload` can name what it could not read.

## Known gaps / follow-ups

- `.format` on a block still being typed asks the formatter to parse an incomplete program, so it reports the parser's unclosed-block error rather than formatting a fragment. Formatting a fragment would change what the formatter accepts, which this phase forbids. → FINDINGS.md
- Format-on-enter is line-by-line, so an unclosed opener is left as typed (the formatter rejects it) and the block's later lines are formatted only once they are individually valid. This is the same fallback that keeps a formatter error from becoming a silent acceptance.
