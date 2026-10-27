# Phase 097 — Parse and format dates from text

## What changed
| File | Lines | What |
|---|---|---|
| `src/runtime.rs` | +426 −35 | one ISO-8601 parser, date formatting, `time.parse`, date-aware `time.format` and `time.unix`, nanosecond property, bounded layout validation, and a comma decimal mark in the fraction |
| `src/stdlib.rs` | +9 −2 | register `time.parse`, add the `time.parse` member, name and signature, widen the format/unix signatures |
| `SPEC.md` | +40 −12 | document accepted and refused ISO-8601 layouts, date formatting, offset handling and the nanosecond property |
| `README.md` | +2 −1 | time quick reference names `time.parse` and a date argument to `time.format` |
| `SURVEY.md` | +21 −24 | rank 6 closed, inventory 85 builtins and 63 dotted members, citations re-anchored |
| `tests/time_builtin_test.rs` | +310 | fifteen tests for parsing, formatting, conversion, refusals and cross-engine agreement |
| `tests/unreviewed_work_ack_test.rs` | +1 | names phase-097's delivery subject in the rule that acknowledges the in-flight phase's commit |
| `tests/stdlib_module_docs_test.rs` | +5 | an answerable-program row for `time.parse` |
| `tests/doc_blocks_runs.txt` | +2 −2 | time and date documentation blocks re-anchored after the SPEC edits |

## Behaviour
`time.parse(text)` reads ISO-8601 into a date value. It accepts a date only
(`2024-02-29`), a date and time with `T` or a space, one to nine fractional
digits, and an optional `Z` or `±HH:MM` offset. An offset is applied, so
`2024-01-01T12:00:00+02:00` is the literal date `2024-01-01 10:00:00`. A text
without a zone is read as UTC, the timezone stance phase-096 already documented.
The fraction is kept in the date value and is readable as `nanosecond`.

`time.format` now accepts a date value as well as the numeric timestamp it
always accepted, and still uses `%Y-%m-%d %H:%M:%S` when no layout is given.
The numeric form returns exactly what it did before, including the UTC zone
that `%Z`, `%z`, and `%+` render. `time.unix` now accepts a date value or any
text `time.parse` accepts, and reads the old `YYYY-MM-DD HH:MM:SS` layout through
the same parser, so there is one date model instead of a separate
strftime/parse path; the parser also keeps the one-to-three-digit years the old
`%Y` path read, so old callers keep their answers. `time.from_unix` remains the
numeric inverse.

Malformed text is a catchable `Runtime` error naming the builtin and the byte
position where the expected form stopped matching, with the offending characters
quoted. A leap second (`:60`), an invalid day, an out-of-range zone offset, a
missing seconds field, a fraction with no digits or more than nine, and trailing
garbage are refused; none reaches a panic. A layout `chrono` cannot read is
refused the same way rather than panicking in its display implementation.

## Tests added
| Test | Edge class covered |
|---|---|
| `edge_time_parse_accepts_iso_offsets_against_literal_utc_dates` | offset conversion against literal calendar values |
| `edge_time_parse_keeps_fractions_and_reads_a_missing_zone_as_utc` | fraction preservation, date-only, timezone-less |
| `edge_time_format_of_a_date_pads_and_honours_the_chosen_layout` | zero padding, chosen layout, default layout, unreadable and parse-only layout refusal |
| `edge_time_unix_reads_a_date_value_and_iso_text_with_an_offset` | date-to-timestamp conversion, old layout preserved, pre-epoch boundary |
| `edge_time_parse_refuses_malformed_text_with_a_position` | empty, malformed, leap second, trailing garbage, unicode, wrong type, catchability |
| `edge_the_positive_window_boundary_round_trips_through_text` | expanded-year `+10000` through `to_text`, `time.unix`, `time.parse`, and `time.format` |
| `edge_time_parse_refuses_each_out_of_range_field_at_its_position` | month 13/00, day 32, hour 24, minute 60, offset minute 60, each with its position and catchability |
| `edge_time_parse_keeps_leap_days_and_refuses_leap_seconds` | 29 February in leap and common years, leap-second refusal |
| `edge_the_tree_walker_and_the_bytecode_vm_agree_on_parsing_and_formatting_dates` | cross-engine data and cross-engine refusal |
| `edge_time_parse_accepts_a_comma_fraction_mark_like_iso_8601` | decimal comma, nanosecond preservation, formatter agreement |
| `edge_time_format_of_a_numeric_timestamp_keeps_its_utc_zone` | numeric `%Z` and `%z` render UTC rather than an empty zone |
| `edge_time_unix_reads_the_short_years_the_old_layout_accepted` | one-to-three-digit years keep the answers chrono's `%Y` gave |
| `edge_time_parse_reads_a_space_separator_and_a_date_only_as_utc` | space separator, timezone-less date and midnight |
| `edge_time_parse_refuses_a_lone_fraction_mark_and_an_offset_without_minutes` | malformed input at an exact position, catchability |
| `edge_time_unix_refuses_malformed_text_and_wrong_types` | failure channel for malformed text and wrong argument kinds |

## Mandatory edge-case matrix
| Row | Status |
|---|---|
| empty | covered — `time.parse("")` is refused at position 0 |
| singleton | N/A — a timestamp is fixed-field text, not a collection with a singleton case |
| boundary | covered — leap day 2024/2000, common-year 29 February, epoch `-1`, offset conversion, positive window boundary `+10000` |
| out_of_bounds | covered — 30 February, month 13/00, day 32, hour 24, minute 60, offset hour/minute 24/60, more than nine fraction digits, out-of-window Unix seconds |
| type_mismatch | covered — `time.parse(5)`, a list, no argument, a numeric layout, and a date where a timestamp was expected |
| numeric_boundary | covered — one to nine fraction digits after either mark, 1900 versus 2000 leap rules, pre-epoch timestamp, shared Unix window |
| unicode | covered — trailing CJK text and full-width digits are refused with a position and no panic |
| nesting_recursion | N/A — the parser is an iterative fixed field walk with no recursion |
| duplicate_missing_keys | N/A — timestamps have fixed fields and no user-supplied keys |
| malformed_input | covered — empty text, bad day, missing seconds, empty/oversized fraction, a decimal mark with no digits, an offset without minutes, bad offset, trailing garbage |
| resource_limit | N/A — parsing is single-pass over the input, allocates only the error snippet, and has no loop guard or recursion limit to set |

## Gates
| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 77 test binaries, 2266 passed, 0 failed |
| `../rbops/verify.sh phase-097` | pass |

## Invariants touched
- `time.format` and `time.unix` gain an additional accepted argument kind (a
  date value); their existing numeric and text behaviour is unchanged.
- `time.unix` accepts the ISO-8601 text `time.parse` accepts, a superset of the
  old fixed layout, so old callers keep their answers.
- A date property `nanosecond` is added; existing date properties are unchanged.
- The list of date values, `.rb` extension, grammar, and error kinds are
  unchanged.

## Existing test reconciliation
- `tests/doc_blocks_runs.txt` re-anchors the dates block from `SPEC.md:1439` to
  `SPEC.md:1443` and the regex block from `SPEC.md:1522` to `SPEC.md:1550`. Both
  moved because the time documentation gained examples; both were executed by
  the suite, not merely listed.
- `tests/unreviewed_work_ack_test.rs` adds `resume phase-097:` to
  `DELIVERY_SUBJECT_PREFIXES`, because the resumed delivery commit is the
  phase's own and cannot be pinned by SHA.
- `tests/stdlib_module_docs_test.rs` gains the `time_parse` answerable row that
  the registry-coverage test requires for every registered builtin.
- `SURVEY.md` inventory now reads 85 builtins and 63 dotted members; rank 6 keeps
  its historical blocked count with "0 since phase-097" so the ranking still
  descends as its own test requires.

## Known gaps / follow-ups
- `time.parse` does not take a caller-chosen input layout; it accepts the ISO
  shapes documented in `SPEC.md` and refuses everything else. A layout-based
  parser is a separate capability and is not needed for the ISO-8601 finding.
- A date's display text and `to_text` drop the fraction even though the value
  keeps it; `time.format` and the `nanosecond` property expose it.
- `time.parse` does not enforce the `time.format` numeric window; a date outside
  it can be constructed, but `time.unix` refuses to convert it, as documented.
