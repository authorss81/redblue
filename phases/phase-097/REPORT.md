# Phase 097 — Parse and format dates from text

## What changed
| File | Lines | What |
|---|---|---|
| `src/runtime.rs` | +505 −40 | one ISO-8601 parser, date formatting, `time.parse`, date-aware `time.format` and `time.unix`, nanosecond property, bounded layout validation, a comma decimal mark in the fraction, a fractional `time.unix` answer whose window edge is checked as an instant, truncation toward zero for numeric instants (their pre-phase-097 behaviour), a near-one clamp, a legacy-layout fallback, and a year-aware refusal; JSON dates now use the shared text form |
| `src/value.rs` | +13 −1 | one `date_text` printer shared by `say`, `to_text` and JSON, so a parsed fraction survives a round trip instead of being dropped |
| `src/stdlib.rs` | +9 −2 | register `time.parse`, add the `time.parse` member, name and signature, widen the format/unix signatures |
| `SPEC.md` | +55 −12 | document accepted and refused ISO-8601 layouts, date formatting, offset handling, the nanosecond property, fraction-preserving date text, fractional Unix answers, the legacy truncation of a numeric instant, the legacy-layout fallback, and the window's final-second rule |
| `README.md` | +2 −1 | time quick reference names `time.parse` and a date argument to `time.format` |
| `SURVEY.md` | +24 −27 | rank 6 closed, inventory 85 builtins and 63 dotted members, citations re-anchored to the current runtime lines |
| `tests/time_builtin_test.rs` | +605 −4 | twenty-eight tests for parsing, formatting, conversion, refusals, fraction-preserving text, fractional timestamps, legacy-layout compatibility and cross-engine agreement |
| `tests/unreviewed_work_ack_test.rs` | +1 | names phase-097's delivery subject in the rule that acknowledges the in-flight phase's commit |
| `tests/stdlib_module_docs_test.rs` | +5 | an answerable-program row for `time.parse` |
| `tests/doc_blocks_runs.txt` | +2 −2 | time and date documentation blocks re-anchored after the SPEC edits |

The ISO-8601 parser, `time.parse`, the date-aware `time.format`/`time.unix` and
their tests were merged by earlier attempts. A resumed run shared one `date_text`
printer so a parsed fraction survives `say`/`to_text`/JSON. This resumed run
finished the other conversion direction: `time.unix` dropped the fraction the
date value held, so a fractional date now answers a fractional timestamp, and
the final second of the shared window is refused when it carries a fraction
rather than yielding a number `time.format` will not take. Three new tests pin
both changes. No working code was removed.

Review round 1 fixed the conversion edges those changes exposed: a fraction
within half an ulp of 1 is clamped below the next second, so `.999999999` at
2024 and at the window edge cannot name the following second; `time.unix` falls
back to the old chrono layout for unpadded fields and the leap second, while
`time.parse` stays strict ISO; and an out-of-range year is refused at the year
field. The SURVEY citations were re-anchored.

That round also changed numeric instants from truncation toward zero to floor
and re-scoped the pre-existing truncation test to the new answer. This resumed
run reverses that compatibility regression: the phase's contract says a
builtin must keep answering what it answered for inputs it already handled, and
AGENTS.md forbids re-scoping an existing test. `seconds_for_instant` truncates
again, `edge_time_from_unix_truncates_a_fractional_second_like_time_format`
carries its original name and assertions, and `time.format(-0.5)` is
`1970-01-01 00:00:00` as it was before the phase. The date-to-number direction
still preserves a pre-epoch fraction (`time.unix(time.parse(
"1969-12-31T23:59:59.5Z"))` is `-0.5`), so the compatibility rule does not make
that direction lie. Three new tests pin the truncation across positive,
negative and negative-zero numbers, the pre-epoch fraction `time.unix` still
answers, and a year sign with no digits being refused at its position.

## Behaviour
`time.parse(text)` reads ISO-8601 into a date value. It accepts a date only
(`2024-02-29`), a date and time with `T` or a space, one to nine fractional
digits, and an optional `Z` or `±HH:MM` offset. An offset is applied, so
`2024-01-01T12:00:00+02:00` is the literal date `2024-01-01 10:00:00`. A text
without a zone is read as UTC, the timezone stance phase-096 already documented.
The fraction is kept in the date value, is readable as `nanosecond`, and is
written by `say`, `to_text` and JSON: `to_text(time.parse(...))` now carries the
fraction with three, six or nine digits, so the text round-trips through
`time.parse`. A date with no fraction keeps the text it always had.

`time.format` now accepts a date value as well as the numeric timestamp it
always accepted, and still uses `%Y-%m-%d %H:%M:%S` when no layout is given.
The numeric form returns exactly what it did before, including the UTC zone
that `%Z`, `%z`, and `%+` render. `time.unix` now accepts a date value or any
text `time.parse` accepts, and reads the old `YYYY-MM-DD HH:MM:SS` layout through
a compatibility fallback to chrono's `%Y-%m-%d %H:%M:%S`, so unpadded month,
day, hour, minute and second fields and the leap second `:60` keep the answers
old callers had; `time.parse` remains strict ISO and refuses both. The ISO
parser also keeps the one-to-three-digit years the old `%Y` path read. A date
that holds a fractional second answers a fractional number of seconds —
`time.unix(time.parse("2024-02-29T23:59:58.5Z"))` is `1709251198.5` — so the
timestamp, the date value and `time.format` name the same instant. A numeric
instant keeps the conversion it always had: `as i64` truncates toward zero, so
`time.format(-0.5)` and `time.from_unix(-0.5)` name `1970-01-01 00:00:00`, and
`time.format(-1.9)` names `1969-12-31 23:59:59`. That is the legacy
number-to-date direction and it is not applied to a date value: a date that
holds a pre-epoch fraction still answers a fractional timestamp. A fraction
that would round up to the next whole second at `f64` magnitude is clamped
below it, so the number never names a second after the date value, even at the
window's final second. The window ends
exactly at `10000-01-01T00:00:00Z`; a fraction in that final second is past the
end and is a catchable `time.unix` error.

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
| `edge_time_parse_refuses_an_offset_that_moves_an_edge_date_out_of_range` | offset conversion past both calendar edges, catchable refusal |
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
| `edge_time_unix_preserves_a_parsed_fraction` | fraction preservation through `time.unix`, binary-exact literals, pre-epoch negative fraction, offset conversion, whole second unchanged |
| `edge_time_unix_fraction_agrees_across_engines_and_text` | cross-engine fractional timestamp, text round trip, formatter agreement |
| `edge_time_unix_refuses_a_fraction_past_the_positive_window_edge` | out-of-bounds instant at the window's final second, catchability, whole boundary second still accepted |
| `edge_date_text_keeps_a_parsed_fraction_and_round_trips` | fraction preservation through `to_text` and JSON, zero-fraction boundary, `time.parse` round trip |
| `edge_date_text_uses_the_shortest_fraction_without_losing_precision` | numeric boundary: three, six and nine fraction digits, zero fraction writes no mark |
| `edge_date_text_round_trip_refuses_trailing_fraction_text` | malformed input: a second fraction is refused at position 23 and is catchable |
| `edge_the_tree_walker_and_the_bytecode_vm_agree_on_fractional_date_text` | cross-engine text, JSON and nanosecond agreement for a fractional date |
| `edge_time_from_unix_truncates_a_fractional_second_like_time_format` | compatibility boundary: positive, negative and zero numeric instants keep the pre-phase answer |
| `edge_time_format_of_a_fractional_number_still_truncates_toward_zero` | numeric boundary: `0.5`, `1.9`, `-0.5`, `-1.9` and `-0.0` across both numeric builtins |
| `edge_time_unix_keeps_a_pre_epoch_fraction_even_though_from_unix_truncates` | pre-epoch fractional date still answers a signed fraction; legacy truncation is only number-to-date |
| `edge_time_parse_refuses_a_year_sign_without_digits` | malformed input: a lone sign, a signed non-date and a sign before a space, each refused at its position |

## Mandatory edge-case matrix
| Row | Status |
|---|---|
| empty | covered — `time.parse("")` is refused at position 0 |
| singleton | N/A — a timestamp is fixed-field text, not a collection with a singleton case |
| boundary | covered — leap day 2024/2000, common-year 29 February, epoch `-1`, offset conversion, positive window boundary `+10000`, and a fraction in its final second refused |
| out_of_bounds | covered — 30 February, month 13/00, day 32, hour 24, minute 60, offset hour/minute 24/60, more than nine fraction digits, out-of-window Unix seconds, and a fraction past the window edge |
| type_mismatch | covered — `time.parse(5)`, a list, no argument, a numeric layout, and a date where a timestamp was expected |
| numeric_boundary | covered — one to nine fraction digits after either mark, 1900 versus 2000 leap rules, pre-epoch fractional timestamp `-0.5`, truncation of `0.5`/`1.9`/`-0.5`/`-1.9`/`-0.0`, shared Unix window |
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
| `cargo test --all-targets` | 77 test binaries, 2280 passed, 0 failed |
| `../rbops/verify.sh phase-097` | pass |

## Invariants touched
- `time.format` and `time.unix` gain an additional accepted argument kind (a
  date value); their existing numeric and text behaviour is unchanged.
- `time.unix` accepts the ISO-8601 text `time.parse` accepts, a superset of the
  old fixed layout, so old callers keep their answers.
- A date property `nanosecond` is added; existing date properties are unchanged.
- A date's printed text now keeps a non-zero fraction. Dates without a fraction
  print exactly as before, so every existing `say`/`to_text`/JSON answer for
  them is unchanged.
- `time.unix` now answers a fractional number for a date that holds a fraction.
  Whole-second dates answer exactly the integer they did before, so every
  pre-existing input keeps its answer.
- The list of date values, `.rb` extension, grammar, and error kinds are
  unchanged.

## Existing test reconciliation
- `tests/doc_blocks_runs.txt` re-anchors the dates block to `SPEC.md:1457` and
  the regex block to `SPEC.md:1565`. Both moved again because the time
  documentation changed the numeric-instant paragraph; both were executed by
  the suite, not merely listed.
- `tests/time_builtin_test.rs` restored the pre-existing
  `edge_time_from_unix_truncates_a_fractional_second_like_time_format` name and
  assertions instead of leaving the floor re-scope in place. The three tests
  added by this run sit beside it.
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
- A date value is timezone-naive, so the offset a parsed text carried is applied
  and then forgotten. The instant is correct, but printing it back cannot name
  the original offset.
- `time.parse` does not enforce the `time.format` numeric window; a date outside
  it can be constructed, but `time.unix` refuses to convert it, as documented.
  A fraction in the window's final second is refused for the same reason.
- `time.format` of a numeric timestamp still truncates a fractional second, the
  behaviour it had before this phase; only a date value carries the fraction
  into a layout.
- A pre-epoch fractional timestamp does not round-trip through
  `time.from_unix(time.unix(date))`, because the legacy number-to-date
  conversion truncates toward zero: `-0.5` names the epoch second, while the
  date it came from was in second `-1`. This is the pre-phase behaviour, kept
  deliberately; the date-to-number direction still returns the fraction, and a
  fix would have to change an input `time.format` already handled.
