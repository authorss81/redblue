# Phase 096 — Date and duration values in Redblue

## What changed
| File | Lines | What |
|---|---|---|
| `src/value.rs` | +58 | `Value::Date(NaiveDateTime)` and `Value::Duration(TimeDelta)`, display text, `type_name`, truthiness |
| `src/runtime.rs` | +408 −50 | Date/duration arithmetic, comparison, properties, calendar helpers, nine builtin arms, signed JSON/display handling |
| `src/stdlib.rs` | +76 −1 | `time` module members, globals, `BUILTIN_NAMES`, `BUILTIN_SIGNATURES` |
| `SPEC.md` | +83 | Date/duration section: constructors, operators, properties, overflow rules, timezone stance |
| `README.md` | +10 | `time` quick-reference block |
| `SURVEY.md` | +36 −31 | Registry count re-anchored (74 → 83), `time` members, rank 6 evidence updated to the surviving parse/convert gap |
| `tests/time_builtin_test.rs` | +211 | 10 date/duration tests added to the existing time suite, on both engines |
| `tests/stdlib_module_docs_test.rs` | +45 | Nine new reachable builtin rows and their documented spellings |
| `tests/doc_blocks_runs.txt` | +2 −1 | New `SPEC.md:1438` block classified RUNS; shifted block re-anchored to `SPEC.md:1517` |
| `src/repl/weight.rs`, `src/testing/runner.rs`, `tests/common/vm.rs` | +11 | Exhaustive matches extended for the two new `Value` variants |
| `src/runtime.rs` (resumed) | +32 −10 | Signed `nanoseconds` for negative durations; `is`/`is not` refuse a date or duration mixed with a number |
| `SPEC.md` (resumed) | +10 −6 | Equality refusal stated; negative duration components documented; 2023 month-end typo fixed |
| `tests/time_builtin_test.rs` (resumed) | +118 | Four more edge tests: mixed-kind equality, signed negative components, backward month-end, cross-engine operations and errors |
| `tests/doc_blocks_runs.txt` (resumed) | +1 −1 | Regex block re-anchored to `SPEC.md:1521` after the SPEC additions |
| `tests/unreviewed_work_ack_test.rs` (resumed) | +6 −4 | `resume phase-096:` added to the delivery-commit rule so HEAD's own resume commit is acknowledged |

## Behaviour
`time.date(year, month, day)` makes a date at midnight and
`time.datetime(year, month, day, hour, minute, second)` makes one at the stated
time. Both read whole numbers and refuse a component that is not real
(`2024-02-30`, month `13`, hour `24`) with a catchable `Runtime` error naming the
builtin. `time.duration(seconds)` makes a duration; the number may be negative or
fractional, rounded to the nearest millisecond.

Dates and durations are ordinary first-class values: they live in records and
lists, pass through functions, and print as data. `date - date` answers a
duration; `date ± duration` and `duration + date` move a date; durations add and
subtract. Ordering and equality are defined within a kind only; comparing a date
or duration with a number — by `<`, `is` or `is not` — is a catchable `Runtime`
error, not a panic or a silent `no`.

Calendar movers clamp rather than roll over: `2024-01-31 + 1 month` is
`2024-02-29`, `2023-01-31 + 1 month` is `2023-02-28`, and `2024-02-29 + 1 year`
is `2025-02-28`. `add_days`, `add_hours`, `add_minutes` and `add_seconds` are
fixed spans (24-hour days); the language is timezone-naive and has no DST
transitions by design.

Date properties are `year`, `month`, `day`, `hour`, `minute`, `second` and
`weekday`; duration properties are `days`, `hours`, `minutes`, `seconds`,
`total_seconds` and `nanoseconds`. A negative span carries its sign in every
component, so `time.duration(-1.5).nanoseconds` is `-500000000` and the
components sum to `total_seconds`; an unknown property answers `nothing`, which
is the existing property contract. JSON turns a date into its display text and a
duration into its total seconds.

`time.unix` is deliberately unchanged: it still answers a number. Phase 096 adds
the value types and arithmetic, not a text-to-date parser; that surviving edge is
recorded in `SURVEY.md` rank 6.

## Tests added
| Test | Edge class covered |
|---|---|
| `edge_date_constructs_and_compares` | happy path, equality and ordering |
| `edge_subtracting_dates_spans_a_month_and_a_leap_day` | date subtraction across a month and a leap day |
| `edge_adding_months_clamps_at_the_end_of_the_month_both_ways` | month-end clamping, forward and backward |
| `edge_adding_a_year_to_a_leap_day_lands_on_the_last_day_of_february` | leap-year boundary |
| `edge_duration_components_and_negative_and_zero_spans` | zero, negative and component decomposition |
| `edge_adding_a_duration_is_fixed_24_hour_days_with_no_dst` | fixed-span arithmetic, no DST |
| `edge_a_date_is_first_class_in_records_lists_and_functions` | nesting and first-class use |
| `edge_comparing_a_date_with_a_number_is_a_catchable_type_error` | cross-kind type mismatch |
| `edge_malformed_dates_and_arithmetic_are_refused_by_name` | malformed components, arity and argument types |
| `edge_the_tree_walker_and_the_bytecode_vm_agree_on_dates` | cross-engine agreement |
| `edge_equality_between_a_date_or_duration_and_a_number_is_a_catchable_type_error` | type mismatch, equality and `is not` |
| `edge_negative_duration_components_are_signed_and_sum_to_the_total` | negative, zero and fractional duration boundaries |
| `edge_month_end_overflow_backwards_in_leap_and_non_leap_years` | month-end clamping backward and across a year rollover |
| `edge_the_tree_walker_and_the_bytecode_vm_agree_on_time_operations_and_errors` | cross-engine arithmetic and cross-engine error text |

## Mandatory edge-case matrix
| Row | Status |
|---|---|
| empty | covered — zero-length duration displays and decomposes as `0 seconds` |
| singleton | covered — a single date and a single duration are first-class values in a record and a list |
| boundary | covered — month-end clamping forward and backward, leap and non-leap years, year rollover and negative spans |
| out_of_bounds | covered — day `30` in February, month `13` and hour `24` are refused by name |
| type_mismatch | covered — date/duration versus number ordering and equality, and a number passed to `add_days` |
| numeric_boundary | covered — leap years, negative durations, signed nanoseconds and fractional seconds rounded to the nearest millisecond |
| unicode | N/A — dates and durations are numeric values, not text |
| nesting_recursion | covered — dates inside records/lists and through a user function |
| duplicate_missing_keys | N/A — the new values have fixed properties and no user-supplied keys; an unknown property answers `nothing` |
| malformed_input | covered — invalid calendar components, wrong arity and wrong argument types |
| resource_limit | N/A — date arithmetic is fixed-size and allocates no growing structure |

## Gates
| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 77 test binaries, 2247 passed, 0 failed |
| `../rbops/verify.sh phase-096` | pass — report, diff hygiene, fmt, clippy, 2247 tests, doc sweep, test policy, report honesty |

## Invariants touched
- `Value` gains `Date` and `Duration`. This is the value-contract change the
  phase was dispatched for; every exhaustive match in `src/` and the test helpers
  was extended rather than bypassed.
- No grammar, lexer, parser or analyzer change. Dates are reached through the
  existing `time` module and the existing operators.
- The existing `time.now`, `time.sleep`, `time.format` and `time.unix` behaviour
  is unchanged; `time.unix` still answers a number.
- No new dependency: `chrono` was already in `Cargo.toml`.
- No skip, no ignored test and no lint suppression was added.

## Existing test reconciliation
Four existing expectations encoded the old inventory and moved with the change:

- `tests/doc_blocks_runs.txt` re-anchors the block that shifted from
  `SPEC.md:1434` to `SPEC.md:1517` and classifies the new `SPEC.md:1438` block as
  RUNS. The resumed SPEC additions moved the regex block again to
  `SPEC.md:1521`; that anchor was updated and both blocks were executed, not
  merely listed.
- `tests/unreviewed_work_ack_test.rs` gains `resume phase-096:` in
  `DELIVERY_SUBJECT_PREFIXES`. This is the acknowledgement the audit requires
  for HEAD's own delivery commit; the rule acknowledges it by subject because
  the pipeline rebuilds that commit on every round.
- `SURVEY.md` now states 83 builtins and 61 dotted members, lists the new `time`
  members, and re-anchors citations that moved in `src/stdlib.rs`. Rank 6 keeps
  its blocked count because the parse/convert gap survives, but its evidence now
  says what phase-096 added and what still cannot be done.
- `tests/stdlib_module_docs_test.rs` gains the nine new reachable builtins; no
  assertion was weakened.

## Known gaps / follow-ups
- There is no `time.parse` and no `time.from_unix`; text and Unix timestamps
  cannot enter date arithmetic. This is the remaining half of `SURVEY.md` rank 6.
- Timezone-naive by design: no zones and no DST, so `add_days` is always 24 hours.
- `time.duration` accepts fractional seconds only to millisecond precision, and
  date constructors take whole seconds.
- `time.format` still works on a Unix timestamp number, not on a `Value::Date`; a
  future phase could accept both without changing existing callers.
