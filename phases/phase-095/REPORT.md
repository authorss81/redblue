# Phase 095 — Expose the regex crate to Redblue programs

## What changed
| File | Lines | What |
|---|---|---|
| `src/stdlib.rs` | +296 −2 | `regex` module in `MODULES`, five members, registry/signature entries, and the `regex` crate binding: `match`, `is_match`, `capture`, `replace`, `split` |
| `SPEC.md` | +33 | `### regex` section: match records, capture references, errors, and the literal `text.split` boundary |
| `README.md` | +10 | `### Regex` quick reference |
| `SURVEY.md` | +40 −29 | Rank 2 recorded as closed by phase-095; registry counts and regex evidence re-anchored |
| `tests/regex_module_test.rs` | +300 | 16 new tests for the module on both engines |
| `tests/stdlib_module_docs_test.rs` | +82 | Five regex rows in the reachability table plus three regex tests through the real `rb run` path |
| `tests/stdlib_module_names_test.rs` | +5 | The documented-module runner now calls `regex.is_match` |
| `tests/stdlib_gap_survey_test.rs` | +34 −96 | The obsolete "regex is refused" assertion became "regex is answerable on both engines"; citation check now anchors the binding |
| `tests/doc_blocks_runs.txt` | +1 | New `SPEC.md:1434` example classified as RUNS |

## Behaviour
`regex` is a binding to the `regex` crate named in `LIBRARIES.md` § 2, not an
engine written in Redblue. The Redblue surface is:

- `regex.match(text, pattern)` — search anywhere; a record of `text`, `start`,
  `finish`, `groups` (whole match at `groups[0]`), or `nothing`.
- `regex.is_match(text, pattern)` — `yes`/`no`.
- `regex.capture(text, pattern, group)` — captured text, or `nothing` for a
  non-participating group; an out-of-range group is a `Runtime` failure.
- `regex.replace(text, pattern, replacement)` — non-overlapping replacement,
  with `$1`, `$name` and `${name}` references; an unknown reference is a
  `Runtime` failure that names the group.
- `regex.split(text, pattern)` — text between non-overlapping matches.

A bad pattern is a catchable `Error::Runtime` carrying the crate's message,
never a panic. Match records are ordinary data, so two matches with the same
captures compare equal (pinned by
`edge_regex_match_records_compare_by_their_captures`). `text.split` is
untouched and remains a literal separator; `regex.split` is the pattern form.

## Tests added
| Test | Edge class covered |
|---|---|
| `edge_regex_match_exposes_the_whole_match_and_its_captures` | happy path + character positions |
| `edge_regex_match_records_compare_by_their_captures` | equality by data, not identity; position zero |
| `edge_regex_split_uses_a_pattern_and_text_split_stays_literal` | split match / no match, literal-vs-pattern |
| `edge_regex_replace_expands_numbered_and_named_captures` | capture references |
| `edge_regex_invalid_group_reference_is_a_catchable_runtime_error` | malformed replacement; failure channel |
| `edge_regex_invalid_pattern_is_a_catchable_runtime_error` | malformed input; failure channel |
| `edge_regex_capture_out_of_bounds_and_non_participating_groups` | out of bounds, a huge group number, + `nothing` |
| `edge_regex_match_answers_nothing_when_the_pattern_does_not_match` | no match answers `nothing` |
| `edge_regex_empty_pattern_matches_at_zero` | empty + position zero |
| `edge_regex_split_with_an_empty_pattern_keeps_the_empty_parts` | empty pattern split at every character boundary |
| `edge_regex_overlapping_matches_are_not_double_counted` | overlapping matches |
| `edge_regex_non_ascii_pattern_and_subject` | Unicode |
| `edge_regex_catastrophic_pattern_terminates` | resource/backtracking |
| `edge_regex_type_mismatch_is_refused_by_name` | type mismatch; failure channel |
| `edge_the_two_engines_answer_regex_identically` | cross-engine value equality |
| `edge_regex_is_documented_and_documented_functions_are_registered` | documents ↔ registry |
| `edge_regex_module_answers_a_match_record_and_its_captures` | match record + character positions through `rb run` |
| `edge_regex_module_answers_is_match_replace_and_split` | documented boolean/replacement/split spellings |
| `edge_regex_module_refuses_a_bad_pattern_by_name` | malformed pattern on the CLI failure channel |

## Mandatory edge-case matrix
| Row | Status |
|---|---|
| empty | covered — empty pattern match and split; empty capture; empty split part in the literal comparison |
| singleton | covered — one capture, one-element/no-match split |
| boundary | covered — match at position zero, first character, end offset |
| out_of_bounds | covered — capture group past `captures_len` is a clean `Runtime` error |
| type_mismatch | covered — number where text is required is refused by name |
| numeric_boundary | N/A — the module has no arithmetic; positions are character counts and the only index is a capture number |
| unicode | covered — non-ASCII pattern and subject, positions counted as characters |
| nesting_recursion | N/A — regex matching does not recurse through Redblue values or scopes |
| duplicate_missing_keys | N/A — match records have a fixed set of four keys; no user-supplied keys |
| malformed_input | covered — invalid pattern and invalid replacement reference |
| resource_limit | covered — the catastrophic-backtracking pattern finishes because `regex` is linear-time; no Redblue step loop is involved |

## Gates
| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 77 test binaries, 2233 passed, 0 failed |
| `../rbops/verify.sh phase-095` | pass — report, diff hygiene, fmt, clippy, 2233 tests, doc sweep, test policy, report honesty |

## Invariants touched
- `text.split` keeps its literal separator; its existing tests pass unmodified.
- No change to `Value`, `Error`, `.rb`, or the language's statement grammar.
- The regex engine remains the `regex` crate; no Redblue-side matcher was added.

## Existing test reconciliation
Two assertions in the tree encoded the *old* finding and could not remain true
once the gap closed:

- `tests/stdlib_gap_survey_test.rs` asserted `regex.match` was refused and that
  no file outside `src/testing/` used `regex::`. It now asserts the probe runs
  to completion on both engines, the crate is reached from `src/`, and the survey
  entry records phase-095. This is the same gate moved to the new contract, not a
  weakened assertion.
- The survey's citation check now anchors `regex_builtin` instead of the deleted
  "Redblue has no regex" comment.

## Review round 3
The review reported that `regex.capture` answered empty text for a
non-participating group. `regex_capture` now branches explicitly on
`Captures::get`: `Some(matched)` is text, `None` is `Value::Nothing`. The
distinction is kept from the other side too — a group that participated with an
empty span still answers `text("")`, which `edge_regex_empty_pattern_matches_at_zero`
pins through `m.groups[0]`. The non-participating case is pinned by
`edge_regex_capture_out_of_bounds_and_non_participating_groups` on both the
direct `regex.capture` result and `m.groups[2]`.

All four gates were re-run on this tree after the change: `cargo fmt --all --
--check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets`
(77 test binaries, 2233 passed, 0 failed), and `../rbops/verify.sh phase-095`.
No gate was relaxed, no test was removed, and no skip or lint suppression was
added.

## Known gaps / follow-ups
- `regex` follows the crate's linear-time engine: there are no backreferences or
  look-around, by design.
- The module does not expose every match of a search (only the first, plus
  replacement/split which visit all matches). A future `regex.find_all` would be
  a natural addition, not a defect in this phase.
