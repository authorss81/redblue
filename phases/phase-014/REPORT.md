# Phase 014 — Standard library: real filesystem and process modules

## What changed

| File | Lines | What |
|---|---|---|
| `src/vm.rs` | +246 −42 | `parse_csv` — a quoted-field-aware CSV reader replacing `line.split(',')`; `parse_json_string` decodes `\uXXXX` (with surrogate pairs) and refuses an undefined escape; `json_escape_text` shared by text values and record keys; `parse_json_object` reports a pair with no colon; `NETWORK_TIMEOUT_SECS` / `NETWORK_CONNECT_TIMEOUT_SECS` and `network_client`, used by `network.get` and `network.post` |
| `src/lib.rs` | +1 −1 | exports `NETWORK_TIMEOUT_SECS`, `NETWORK_CONNECT_TIMEOUT_SECS` |
| `tests/stdlib_modules_test.rs` | +989 new | 39 `#[test]` functions, 20 of them named `edge_*` |

Production change is confined to the four modules this phase owns. No grammar,
no lexer, no parser, no analyzer, no `Value` variant, no `Error` variant, and
no other builtin was touched.

## Reproduction

Four defects, all reproduced on `main` before any edit with
`target/debug/rb` built from the unmodified tree.

**1. `csv.parse` split a quoted field.** `src/vm.rs:1150` before the change was
`text.lines().map(|line| line.split(',')…)` — no quote handling at all.

```redblue
$ cat target/tmp/phase014/repro.rb
say csv.parse("a,\"b,c\",d")
say json.parse("\"\\u0041\\u00e9\"")
say json.stringify(json.parse("{\"a\": [1, {\"b\": \"c,d\"}]}"))
say csv.parse("x,y\r\n1,2\r\n")

$ ./target/debug/rb run target/tmp/phase014/repro.rb
[[a, "b, c", d]]        <- 4 cells, the quoted "b,c" cut in two and its inner
                           space trimmed
u0041u00e9              <- \u0041 came back as the five characters "u0041"
{"a": [1, {"b": "c,d"}]}
[[x, y], [1, 2]]
```

**2. `json.stringify` did not escape record keys** — `json_stringify` wrote
`format!("\"{}\": …", k)` for keys while escaping values, so a key holding a
quote or a newline produced JSON no parser can read:

```redblue
$ cat target/tmp/phase014/key2.rb
set parsed to json.parse("{\"a\\\"b\": 1, \"line\\nbreak\": 2}")
say parsed
say json.stringify(parsed)

$ ./target/debug/rb run target/tmp/phase014/key2.rb
{a"b: 1, line
break: 2}
{"a"b": 1, "line
break": 2}              <- not valid JSON: an unescaped quote, a raw newline
```

**3. `network.get` had no timeout.** Both network builtins called
`reqwest::blocking::Client::new()`, which carries no timeout of its own, so a
black-holed address waited for the operating system's connect timeout and
`try … catch error` could not see it:

```redblue
$ cat target/tmp/phase014/net.rb
say network.get("http://10.255.255.1:81/")
say "returned"

$ timeout 12 ./target/debug/rb run target/tmp/phase014/net.rb
EXIT=124                <- killed by the host; the same call took 30.04s inside
                           the failing test, which is the OS connect timeout
```

**4. A JSON pair with no colon was skipped silently** —
`json.parse("{\"a\" 1}")` returned `{}`, a record missing the field the
document spelled. Found while writing `edge_json_malformed_input_is_an_error`;
it is in the same function as fix 2 and is one `if` wide.

After the change, the same commands:

```
$ ./target/debug/rb run target/tmp/phase014/repro.rb
[[a, b,c, d]]
Aé
{"a": [1, {"b": "c,d"}]}
[[x, y], [1, 2]]

$ ./target/debug/rb run target/tmp/phase014/key2.rb
{a"b: 1, line
break: 2}
{"a\"b": 1, "line\nbreak": 2}

$ time timeout 12 ./target/debug/rb run target/tmp/phase014/net.rb
Error: RuntimeError: RuntimeError: HTTP request failed: error sending request for
  url (http://10.255.255.1:81/): error trying to connect: tcp connect error:
  deadline has elapsed
  --> target/tmp/phase014/net.rb:1:1
real    0m5.049s
EXIT=1
```

## Behaviour this phase defines

Nothing here was undefined before; these are the answers the tests now pin.

- **Missing file** — `Error::Io` naming the path, catchable by
  `try … catch error`. Applies to `read`, `lines`, `delete`, `copy`, `rename`.
- **Permission denied** — `Error::Io`. The test asserts the *unprivileged*
  answer and asserts the privileged one where `root` bypasses the permission
  bits, because DAC has nothing to say to uid 0; the branch is chosen by what
  the host itself does, not by a skip.
- **Path with spaces** — an ordinary path. One literal, one file.
- **Path traversal** — **not confined.** `..` is an ordinary path segment and
  relative paths resolve against the process's working directory. No sandbox is
  introduced here, because that is a language-surface decision and a phase that
  wants one should say so; `files_paths_are_relative_to_the_working_directory_and_not_confined`
  pins the current answer so a future sandbox is deliberate.
- **CSV quoting** — a field is quoted when its *first* character is `"`;
  inside it a comma, a newline and `""` are data; characters between the closing
  quote and the separator are ignored. A bare field is trimmed, which is what the
  language shipped and what the Redblue suite relies on.
- **CSV ragged rows** — allowed. Each row keeps the cells it has; no padding, no
  error.
- **CSV line endings** — `\n`, `\r\n` and a lone `\r` all end a row, and a
  trailing separator adds no empty row. Empty text is no rows; one blank line is
  one row holding one empty cell.
- **CSV unterminated quote** — `Error::Runtime("Invalid CSV: unterminated quoted
  field")`, catchable.
- **JSON escapes** — `\uXXXX` decodes, surrogate pairs combine, an undefined
  escape is an error rather than a silent rewrite of the text, and a lone
  surrogate is an error rather than U+FFFD.
- **JSON keys** — escaped exactly as values are, so `stringify` output is always
  parseable.
- **Network timeout** — 10 s for the whole request, 5 s to connect, both
  published as `redblue::NETWORK_TIMEOUT_SECS` and
  `redblue::NETWORK_CONNECT_TIMEOUT_SECS` and asserted at compile time.

## Tests added

39 `#[test]` functions in `tests/stdlib_modules_test.rs`; floor was ≥ 3 tests
and ≥ 1 `edge_*`.

| Test | Edge class covered |
|---|---|
| `edge_files_read_of_a_missing_file_is_a_catchable_error` | missing file, failure asserted (`Error::Io`), and catchable from Redblue |
| `edge_files_paths_that_name_nothing_readable_are_errors` | malformed path: parent is a file, NUL byte, directory read as file |
| `edge_files_unreadable_file_is_an_error_when_unprivileged` | permission denied, host-privilege dependent |
| `edge_files_argument_of_the_wrong_type_is_an_error` | type mismatch, all 9 call shapes |
| `edge_empty_file_reads_as_empty_text` | empty: zero-byte file, zero lines, `files.write("")` creates |
| `files_write_then_read_round_trips_in_a_temp_dir` | happy path against a per-test temp dir |
| `files_append_creates_then_appends` | create-then-append, boundary of file existence |
| `files_handles_a_path_with_spaces` | path with spaces |
| `files_exists_and_delete_bracket_a_files_life` | missing / present / deleted |
| `files_lines_splits_lines_and_keeps_blank_ones` | empty line, CRLF, singleton line |
| `files_copy_and_rename_move_content` | copy leaves source, rename removes it |
| `files_paths_are_relative_to_the_working_directory_and_not_confined` | path traversal, `..`, missing nested path |
| `csv_quoted_field_with_embedded_comma_is_one_cell` | the finding: embedded comma |
| `csv_quoted_field_keeps_its_inner_spaces` | quoted vs bare whitespace |
| `csv_doubled_quote_is_one_literal_quote` | escape inside a quoted field |
| `csv_quoted_field_may_contain_a_newline` | newline as data |
| `csv_parses_crlf_and_lf_the_same` | CRLF, lone CR |
| `edge_csv_ragged_rows_keep_their_own_cells` | ragged rows |
| `edge_csv_empty_text_and_a_blank_line` | empty, singleton, `,,` |
| `edge_csv_unterminated_quote_is_an_error` | malformed input, failure asserted, catchable |
| `edge_csv_row_index_out_of_bounds_is_a_clean_error` | out of bounds on a row and on the table |
| `csv_unicode_cells_survive_the_parse` | unicode: emoji, CJK, RTL, combining mark |
| `edge_csv_argument_of_the_wrong_type_is_an_error` | type mismatch |
| `json_unicode_escape_decodes_to_its_character` | the finding: `\uXXXX`, surrogate pair, `\b`, `\f`, NUL |
| `json_record_keys_are_escaped_when_written` | the finding: key escaping, round trip |
| `edge_unknown_json_escape_is_an_error_not_silent_corruption` | malformed escape, truncated escape, bad hex, lone high and lone low surrogate |
| `json_round_trips_nesting_unicode_and_empty_containers` | nesting, empty containers, escapes, round trip |
| `json_stringify_keeps_field_order` | determinism: order is not hash order |
| `edge_json_singleton_and_scalar_values` | singleton, empty containers, every scalar |
| `edge_json_numeric_boundaries` | numeric boundary: 2^53±1, −0.0, fraction, ±1e400 |
| `edge_json_missing_field_is_nothing_and_present_field_is_not` | missing key |
| `edge_json_duplicate_key_keeps_the_last_value` | duplicate key |
| `edge_json_malformed_input_is_an_error` | malformed: 8 shapes including `{"a" 1}` |
| `edge_json_control_characters_are_escaped_on_write` | control characters, write and read back |
| `json_unicode_text_survives_stringify_unchanged` | unicode through `stringify` |
| `edge_json_argument_of_the_wrong_type_is_an_error` | type mismatch |
| `edge_network_get_to_a_black_holed_address_gives_up` | resource limit: the finding; ends in bounded time with a `Runtime` error |
| `network_timeouts_are_published_and_bounded` | resource limit: the constants, asserted at compile time and against a real request |
| `edge_network_rejects_an_argument_that_is_not_a_url` | type mismatch, arity, unparseable URL without a socket |

Tests that assert a failure is produced, not just a success:
`edge_files_read_of_a_missing_file_is_a_catchable_error`,
`edge_files_paths_that_name_nothing_readable_are_errors`,
`edge_files_unreadable_file_is_an_error_when_unprivileged`,
`edge_files_argument_of_the_wrong_type_is_an_error`,
`edge_csv_unterminated_quote_is_an_error`, `edge_csv_row_index_out_of_bounds_is_a_clean_error`,
`edge_csv_argument_of_the_wrong_type_is_an_error`,
`edge_unknown_json_escape_is_an_error_not_silent_corruption`,
`edge_json_numeric_boundaries`, `edge_json_malformed_input_is_an_error`,
`edge_json_argument_of_the_wrong_type_is_an_error`,
`edge_network_get_to_a_black_holed_address_gives_up`,
`edge_network_rejects_an_argument_that_is_not_a_url`.

Matrix rows not covered, and why:

- **numeric_boundary** for `files`/`csv` — N/A. A filesystem path and a CSV cell
  are text; this phase adds no arithmetic to either. The numeric boundary that
  *is* in these modules is JSON's, covered by `edge_json_numeric_boundaries`
  (2^53±1, −0.0, ±1e400 refused as not finite).
- **nesting_recursion** — covered once, for JSON (`json_round_trips_nesting_unicode_and_empty_containers`,
  three levels of record/list). Not repeated for CSV: a CSV document is a flat
  list of flat rows by definition, and the recursion limit it would meet is the
  one `tests/call_depth_test.rs` already owns.
- **resource_limit** — covered for the network module, which is the only one of
  the four that could hold a host resource open. Not applicable to `files`
  beyond the paths that name nothing (`edge_files_paths_that_name_nothing_readable_are_errors`),
  to `json.parse`, whose input is bounded by the program text already charged to
  the step budget, or to `csv.parse`, which is a single left-to-right scan with
  no recursion.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 284 passed, 0 failed, 0 ignored (245 before) |
| `./rbops/verify.sh phase-014` | **not run — the script is not in this checkout** |

On the fourth gate: `rbops/` does not exist under the project root
(`ls rbops` → `No such file or directory`), and the task instructions say the
RBOPS pipeline lives outside the working directory and must not be inspected. I
did not run it and I am not claiming a result for it. What I ran in its place,
all green:

```
cargo test --all-targets       284 passed, 0 failed, 0 ignored
cargo test --doc               0 passed, 0 failed
./target/debug/rb test         Tests run: 229  Passed: 229  Failed: 0
examples/*.rb                  all 6 run with no error output
tests/stdlib_modules_test.rs   3 consecutive runs, 39 passed each
```

`modules/MathUtils.rb` does not run as a program — `constant PI to 3.14159` is a
parser error — but that is **pre-existing**: verified by `git stash -u`, rebuild,
and the same `ParserError: Expected function name` on the unmodified tree. The
module is meant to be `import`ed, and `tests/test_modules.rb` passes.

`rb format --check` is red for all 21 tracked `.rb` files (`for f in $(git
ls-files '*.rb'); do rb format --check "$f"; done` → 21 of 21 would be
reformatted), and `rb lint examples/hello.rb` warns `Unused variable: 'name'`
(the lint subcommand takes one file at a time). Both are **pre-existing**: this
phase touched `src/vm.rs`, `src/lib.rs` and one new Rust test file — no `.rb`
file and no formatter code. Recorded in `FINDINGS.md` rather than fixed here.

## Invariants touched

- None. No `.rb` extension change, no `to … end` / `set x to` / `say` change,
  no `Value` variant, no `Error` variant, no grammar change, no parser change,
  no change to any builtin outside `files`, `json`, `csv` and `network`.
- Two behaviours that were previously *silently wrong* are now errors. Both
  tighten rather than widen: `\q` inside a JSON string used to parse as `q`
  (silent corruption) and `{"a" 1}` used to parse as `{}` (a dropped field);
  both are now `Error::Runtime`, and both remain catchable. A program relying on
  the old behaviour was relying on data loss.
- New: `network.get` / `network.post` fail after 10 s (5 s to connect) instead of
  after the operating system's own timeout. `Error::Runtime`, catchable, and the
  timeouts are published constants.
- Existing tests in `tests/`: 245 before, 245 after, all still passing. `rb test`
  229 before, 229 after.

## Known gaps / follow-ups

- The Redblue *lexer* has the same escape weakness this phase fixed in the JSON
  reader: `\u` in a Redblue string literal drops the backslash and yields `u`.
  Every `\uXXXX` test therefore writes `\\uXXXX` in the source. Fixing it is a
  language-surface change and belongs in its own phase. → `FINDINGS.md` §1
- `split_json_pairs` decides "am I inside a string" by looking at the previous
  character, so a key ending in an escaped backslash (`"a\\"`) ends the string
  one character early. Pre-existing and untouched here. → `FINDINGS.md` §2
- `json.parse` has no depth limit on nesting beyond the step budget, and
  `json.stringify` recurses once per level. → `FINDINGS.md` §3
- `csv.parse` has no column limit and no row limit; a 10-million-row document is
  parsed in full. It is a single scan, so it terminates, but it is not bounded in
  memory. → `FINDINGS.md` §3
- No process module exists (`system.exec`, `process.run`). The phase title says
  "filesystem and process modules"; the manifest evidence line names only
  files/time/json/csv/network, and inventing a `system` module would be a new
  language surface. Recorded, not built. → `FINDINGS.md` §4
- `time.now`, `time.sleep`, `time.format` and `time.unix` remain untested here
  for `now` and `sleep` only because they are wall-clock dependent, which
  AGENTS.md §3.1 forbids in a test. `time.format` / `time.unix` are pure and
  belong to a follow-up. → `FINDINGS.md` §5