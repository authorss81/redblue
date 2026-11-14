# SURVEY.md — what a Python programmer cannot do in Redblue

**Status**: survey. It is measured against the tree this commit sits on, and
every claim below carries the line that proves it. Rank 2 was closed by
phase-095; rank 5 gained wrapping, dedenting, templates, padding, alignment
and diffing in phase-101; rank 6 gained date arithmetic in phase-096 and text
parsing and formatting in phase-097; rank 8 gained encodings, digests and UUIDs
in phase-104; rank 10 gained the CSV writer, JSON Lines, TOML and archives in
phase-105; rank 1 gained `filter` and `reduce` and rank 4 gained `sort` and
`reverse` in phase-077. The entries are kept so the ranking and its history stay
readable.

- **Builtins in the registry**: 164 — `stdlib::BUILTIN_NAMES` at `src/stdlib.rs:2149`,
  which closes with `"input"` at `src/stdlib.rs:2313`. The six module-only
  `text.wrap`, `text.diff`, `text.dedent`, `text.template`, `text.pad` and
  `text.align` utilities are deliberately not in this table, and neither is
  `sys.parse` nor the nine `logging` calls, whose only spellings are
  `sys.parse` and `logging.<member>`.
- **Modules a dotted call may name**: 24 — `stdlib::MODULES` at `src/stdlib.rs:19`.
- **Dotted members those modules accept**: 155 — `stdlib::MODULE_MEMBERS` at
  `src/stdlib.rs:85`.

The two tables are not a summary of what Redblue *could* do; they are the whole
of what a dotted call is allowed to reach, because a name not in them is refused
before a call happens (`src/stdlib.rs:300`, `module_has_member`). So the ceiling
of the language is knowable by reading one file, and that is what this survey
reads.

## Method

1. **Enumerate what exists.** `MODULES` (`src/stdlib.rs:19`) and
   `MODULE_MEMBERS` (`src/stdlib.rs:85`) are the authoritative surface, and
   `tests/stdlib_module_names_test.rs` reconciles them against `SPEC.md` and
   `README.md` in both directions. Anything absent from those tables is absent
   from the language, whether or not a registry entry exists.
2. **Take a sample of programs, not a wishlist.** Thirty-two small programs a
   working programmer is asked for constantly — listed in
   [the sample](#the-sample). They are deliberately mundane: log parsing, CSV
   reports, renaming a downloads folder, deduplicating a list. A gap that blocks
   nothing in that list is not in the top ten however interesting it is.
3. **Score by blocked programs.** A program is **blocked** by a gap when its
   central step is a capability the tables name no member for, and the remaining
   language cannot express that step at all. Rewriting the program so that its
   central step disappears does not count as unblocked. A program is counted for
   *every* gap that blocks it, so the counts overlap; the `blocked` list under
   each rank says exactly which numbers.
4. **Label each gap.** `implementable-in-Redblue` means a closure, a loop and the
   existing value types are enough — a `modules/*.rb` file would do it.
   `binding-required` means the work is a wrapper around code that does not exist
   in this language (a regex engine, a directory walker, a digest), and the entry
   names the crate that would supply it.
5. **Break ties by cost.** Where two gaps block the same number of programs, the
   `implementable-in-Redblue` one ranks higher: it unblocks the same number for
   less work. Ranks 9 through 11 are all "2 programs" and are ordered by exactly
   this rule.
6. **Verify by running.** Every blocked program in this document was written to a
   file and run on this commit. The line under each block is the error the
   interpreter actually printed. A gap whose program *succeeds* does not belong
   here.

`tests/stdlib_gap_survey_test.rs` re-runs the mechanical parts of this: it opens
every `path:line` in this file and asks for that line, it reconciles the module
list above against `stdlib::MODULES` and the count against
`stdlib::BUILTIN_NAMES`, it checks the ranks descend, and it checks that the
program under rank 1 is still refused. A citation that goes stale fails the build
rather than misleading a reader.

## The sample

The 32 programs every blocked-program count is a fraction of. Numbered so the
per-rank `blocked` lists can be checked against this.

| # | Program | # | Program |
|---|---|---|---|
| 1 | Parse an access log, count 404s per path | 17 | Split a list into pages of ten |
| 2 | Rename downloaded files by the date in their name | 18 | Zip two lists into pairs |
| 3 | Deduplicate a list of email addresses | 19 | Word-frequency table, print the top ten |
| 4 | Read a CSV of sales, print the top five by revenue | 20 | Base64-decode an `Authorization` header |
| 5 | Normalise phone numbers and write them back as CSV | 21 | Percent-decode a query string |
| 6 | Find every TODO/FIXME in a repository, print file and line | 22 | Print an amount as money, thousands separators |
| 7 | Extract every URL from an HTML page and fetch each | 23 | Left-pad an id to width six so it sorts |
| 8 | Report the median and p90 of a list of timings | 24 | Take the substring between two delimiters |
| 9 | Given a birthdate, print the age in years | 25 | Index of the last `/` in a path |
| 10 | Parse an ISO-8601 timestamp into its fields | 26 | Sort rows by one key, then a tiebreaker |
| 11 | Turn a Unix timestamp into "3 days ago" | 27 | Write a CSV report |
| 12 | Count lines per extension across a directory tree | 28 | Generate a UUID |
| 13 | Copy every `.rb` file into a mirror tree | 29 | Retry an HTTP request three times, growing delay |
| 14 | Merge two JSON config files recursively | 30 | Compute the SHA-256 of a file |
| 15 | Round-trip a record through JSON, preserving key order | 31 | Read `DATABASE_URL` from the environment |
| 16 | Group transactions by account and total them | 32 | Exit nonzero when a check fails |

## What exists today

Every module and every member it is allowed to expose. This is the ceiling this
survey measures against, and it is short.

| Module | Members | Line |
|---|---|---|
| `base64` | `encode`, `decode` | `src/stdlib.rs:86` |
| `bytes` | `from_text`, `text`, `write` | `src/stdlib.rs:87` |
| `collections` | `counter`, `count`, `most_common`, `defaultdict`, `get`, `put`, `deque`, `push_left`, `push_right`, `pop_left`, `pop_right`, `namedtuple`, `field` | `src/stdlib.rs:88` |
| `console` | `log`, `error`, `clear` | `src/stdlib.rs:106` |
| `logging` | `debug`, `info`, `warn`, `error`, `set_level`, `level`, `set_timestamps`, `set_destination`, `reset` | `src/stdlib.rs:107` |
| `csv` | `parse`, `stringify` | `src/stdlib.rs:121` |
| `decimal` | `from_text`, `from_number`, `to_text`, `to_number`, `sum` | `src/stdlib.rs:185` |
| `files` | `read`, `write`, `append`, `exists`, `lines`, `delete`, `copy`, `rename` | `src/stdlib.rs:122` |
| `hash` | `sha256`, `sha512` | `src/stdlib.rs:128` |
| `hex` | `encode`, `decode` | `src/stdlib.rs:129` |
| `json` | `parse`, `stringify`, `parse_lines`, `stringify_lines` | `src/stdlib.rs:130` |
| `list` | `map`, `length`, `filter`, `reduce`, `sort`, `reverse`, `zip`, `enumerate` | `src/stdlib.rs:149` |
| `math` | `abs`, `floor`, `ceil`, `round`, `sqrt`, `pow`, `sin`, `cos`, `tan`, `log`, `exp`, `mean`, `median`, `mode`, `variance`, `standard_deviation`, `percentile`, `correlation` | `src/stdlib.rs:162` |
| `matrix` | `zeros`, `ones`, `identity`, `from_lists`, `to_lists`, `shape`, `add`, `subtract`, `multiply`, `divide`, `scale`, `matmul`, `transpose`, `sum`, `mean`, `min`, `max`, `at`, `row`, `column` | `src/stdlib.rs:189` |
| `network` | `get`, `post` | `src/stdlib.rs:214` |
| `path` | `join`, `parent`, `filename`, `stem`, `extension`, `is_absolute`, `resolve`, `normalize`, `list` | `src/stdlib.rs:215` |
| `regex` | `match`, `is_match`, `capture`, `replace`, `split` | `src/stdlib.rs:229` |
| `sys` | `argv`, `parse` | `src/stdlib.rs:233` |
| `tar` | `read`, `write` | `src/stdlib.rs:265` |
| `text` | `uppercase`, `lowercase`, `trim`, `split`, `join`, `contains`, `starts_with`, `ends_with`, `replace`, `length`, `wrap`, `diff`, `dedent`, `template`, `pad`, `align` | `src/stdlib.rs:234` |
| `time` | `now`, `sleep`, `format`, `parse`, `unix`, `from_unix`, `date`, `datetime`, `duration`, `add_years`, `add_months`, `add_days`, `add_hours`, `add_minutes`, `add_seconds` | `src/stdlib.rs:266` |
| `toml` | `parse` | `src/stdlib.rs:262` |
| `uuid` | `generate`, `parse` | `src/stdlib.rs:263` |
| `zip` | `read`, `write` | `src/stdlib.rs:264` |
Two observations from that table that the rest of this document depends on:

- **`text` has no way to take a piece of a text.** `split` and `join` can get
  there with loss (`src/stdlib.rs:930` — the separator is a literal, so
  `split("a1b22c", "2")` loses the separators), but there is no `slice`, no
  `index_of`, no `pad_left`. Every "extract the field between the colons" task is
  rank 5.
- **`list` has exactly one higher-order function, and `filter` and `reduce` are
  registered without being implemented.** `src/stdlib.rs:143` says so in the table
  itself, and `tests/stdlib_module_docs_test.rs:1461` carries the two programs
  that still fail as `known_gaps()`.

## How to use this ranking

Each entry below is a phase-sized unit of work with an acceptance gate that is
already written down: the blocked program, run on both engines, printing what
Python prints.

- **Start at rank 1 and go down.** The order is blocked-program count, not
  interest. A regex engine is the most *interesting* thing on this list and the
  third most blocking one.
- **Take the `implementable-in-Redblue` entries first.** Ranks 1, 3, 4, 5, 9 and
  10 need no new dependency, and five of the six are a single file in
  `modules/` where the language already has closures and `push`.
- **Do not do rank 2 and rank 6 in the same phase.** Both are bindings, both
  change `Cargo.toml`, and both need an error-message decision (what does a bad
  pattern say, in Redblue's words) that is worth making once.
- **A gap here is not a roadmap item until its blocked program runs.** The
  acceptance gate for any of these phases is the ```redblue``` block below it,
  executed and printing the Python answer.

## 1. No higher-order list functions beyond `map` — closed by phase-077

- **Blocked programs**: 6 — 1, 12, 14, 16, 17, 18 (at survey time; 0 since phase-077)
- **Implementable**: implementable-in-Redblue
- **What phase-077 changed**: `filter` and `reduce` now call a Redblue function
  once per item on both engines, and both are members of `list`. They left
  `known_gaps()` and joined the answerable surface checked by
  `tests/stdlib_module_docs_test.rs`.
- **Still missing**: `any`, `all`, `flatten`, `chunk`, `unique`, `group_by`,
  `sort_by`. These are separate helpers, not the two functions this entry
  measured.

```redblue
say filter([1, 2, 3], to (x) give back x > 1 end)
```

Answers `[2, 3]` since phase-077.

## 2. No regular expressions, anywhere in the language — closed by phase-095

- **Blocked programs**: 4 — 1, 6, 7, 21 (at survey time; 0 since phase-095)
- **Implementable**: binding-required
- **Rust crate**: `regex`
- **Evidence**: `src/stdlib.rs:1778` — `regex_builtin` binds the crate the ranking
  named. The crate is **already a dependency**, `Cargo.toml:31` (`regex = "1.10"`,
  and again at `Cargo.toml:38` as a dev-dependency), and it had been reachable
  **only** from the test harness: `src/testing/assertions.rs:130` builds a
  `Regex` for an assertion pattern, and `src/testing/runner.rs:252` is the
  `assert_matches_regex` helper behind it.
- **What phase-095 changed**: `regex` is now a module with `match`, `is_match`,
  `capture`, `replace` and `split`. `regex.match` answers a data record with
  `text`, `start`, `finish` and `groups`; `replace` expands `$1`/`$name` capture
  references and refuses a group the pattern does not have; a bad pattern is a
  catchable `Runtime` failure. Both engines answer it because both reach
  `stdlib::builtin`.
- **Why it was ranked here**: log parsing, source scanning, HTML scraping and
  percent-decoding are four separate programs, and every one of them used to be
  a hand-rolled state machine. It ranked second, not first, because a careful
  `split`-and-`contains` loop still finished three of the four — slowly, and with
  the delimiters lost.

```redblue
say regex.match("id=42", "id=([0-9]+)")
```

Answers a match record: `{text: id=42, start: 0, finish: 5, groups: [id=42, 42]}`.
Before phase-095 the same program was refused as
`AnalyzerError: Unknown variable 'regex'`, because `regex` was not in `MODULES`.

## 3. No way to enumerate a record's fields

- **Blocked programs**: 4 — 12, 14, 15, 16
- **Implementable**: implementable-in-Redblue
- **Evidence**: `src/interpreter.rs:1844` — `for each` binds its variable only
  inside `if let Value::List(items)`, so a record is not a list and the loop body
  never runs. `src/runtime.rs:1525` — `length` refuses a record outright with
  *"length requires a list or text"*. And the registry holds no `keys` and no
  `values`: `BUILTIN_NAMES` runs to `"input"` at `src/stdlib.rs:2199` without one.
- **Why it matters more than it looks**: a record is Redblue's only map type. A
  program cannot count a record, iterate it, merge one into another, or turn one
  back into text. Grouping is the operation almost every data program performs
  first, and the container it needs for the result cannot be read back out.
- **Note**: the loop is *silent* — it is not an error, it is zero iterations and
  exit 0. That is worse than a refusal and is filed in
  `phases/phase-090/FINDINGS.md`.

```redblue
set r to json.parse("{\"a\": 1, \"b\": 2}")
for each key in r
  say key
end
```

Prints nothing and exits 0. The same record answers `r.a` and refuses `length(r)`.

## 4. No sorting, ordering, or min/max — partly closed by phase-077

- **Blocked programs**: 4 — 4, 8, 19, 26 (at survey time; the plain sort cases
  answer since phase-077)
- **Implementable**: implementable-in-Redblue
- **What phase-077 changed**: `sort(list)` orders a homogeneous list ascending,
  `sort(list, field)` orders records by one field, and both are stable.
  `reverse(list)` answers the list backwards. Both are members of `list`.
- **Still missing**: `sort_by`, `min`, `max`, `sum`, `first` and `last`.
- **Why it mattered**: every report ends in an ordering. "Top five", "p90", "most
  common word" and "sorted by name then by id" are the last line of the program,
  and the plain ordering lines are writable now.

```redblue
say sort([3, 1, 2])
```

Answers `[1, 2, 3]` since phase-077.

## 5. No slicing or searching of text

- **Blocked programs**: 4 — 21, 23, 24, 25 (at survey time; 23 since phase-101)
- **Implementable**: implementable-in-Redblue
- **Evidence**: `src/stdlib.rs:181` — the `text` row now carries the ten original
  names plus `wrap`, `diff`, `dedent`, `template`, `pad` and `align`, none of
  which takes an index or produces a substring. `split` is documented at
  `src/stdlib.rs:930` as taking a literal separator, and `src/stdlib.rs:930` is
  the comment explaining why there is no pattern to give it. `length` counts
  **characters**, not bytes (`src/runtime.rs:1522`), so a slicing design has a
  defined unit to slice by; the gap is that no slicing or searching call exists.
- **What phase-101 changed**: `text.wrap` wraps to a width, `text.dedent` removes
  common indentation, `text.template` substitutes `<<name>>` placeholders,
  `text.pad` and `text.align` produce fixed widths, and `text.diff` produces a
  standard unified diff. Program 23 is therefore no longer blocked by padding.
- **What is still missing**: `slice`, `substring`, `index_of`, `last_index_of`,
  `count`, `repeat`, `chars`, `bytes`, `title`, `capitalize`.
- **Why**: "everything after the last separator" and "the text between two
  delimiters" are not exotic. They are the extraction half of programs 21, 24
  and 25, and each still needs a `split`-and-rejoin loop that throws away the
  information it needs.

```redblue
say text.slice("a1b22c", 1, 2)
```

Refused as `RuntimeError: Module 'text' has no function 'slice'`.

## 6. No date or time arithmetic, and one parseable date format

- **Blocked programs**: 4 — 2, 9, 10, 11 (at survey time; 0 since phase-097)
- **Implementable**: binding-required
- **Rust crate**: `chrono`
- **Evidence**: `src/stdlib.rs:245` — the `time` row carries the date and
duration surface added by phase-096 (`date`, `datetime`, `duration`, and the six
`add_*` movers), plus `time.parse` added by phase-097. `src/runtime.rs:720`
parses the ISO-8601 forms, including an offset and a fraction, into the date
value; `src/runtime.rs:1708` formats a date or a numeric timestamp. `time.unix`
now reads the same parser and also accepts a date value, so a birthdate read
from text can enter the arithmetic. `time.from_unix` remains the numeric inverse.
- **What phase-097 changed**: text parsing and caller-chosen formatting are now
one date model. `time.parse` accepts a date-only or date-and-time ISO-8601 text,
optionally with `.fraction`, `Z` or `±HH:MM`; a zone-free text is read as UTC.
Malformed text is a catchable `Runtime` error naming the byte position.
- **Note**: `chrono` is already a dependency (`Cargo.toml:32`) and already used
  at `src/runtime.rs:720`. The gap was the surface, not the crate.
- **Why**: age in years, "3 days ago", a month-end report and a file renamed by
  its date all need arithmetic. A language that can read the clock but cannot move
  it is a language that can log and not schedule.

```redblue
set born to time.parse("1990-03-04T00:00:00Z")
say time.add_days(born, 365)
```

## 7. No directories, no globbing, no stat

- **Blocked programs**: 4 — 2, 6, 12, 13
- **Implementable**: binding-required
- **Rust crate**: `walkdir`, `glob`
- **Evidence**: `src/stdlib.rs:122` — the `files` row is eight names, every one of
  them taking a whole path as text: `read`, `write`, `append`, `exists`, `lines`,
  `delete`, `copy`, `rename`. There is no `list_dir`, no `mkdir`, no `glob`, no
  `size`, no `modified`. Nothing in the language can ask what is in a directory:
  `src/stdlib.rs:122` is the whole of the module.
- **Why**: four programs in the sample start with "go and look at the files".
  A script that cannot enumerate a directory cannot process a repository, a
  downloads folder, or a log directory — which is most of what scripts are for.
  `network` has no timeout control either (`src/stdlib.rs:151`), so program 29 is
  blocked here as well.

```redblue
say files.glob("corpus/*.rb")
```

Refused as `RuntimeError: Module 'files' has no function 'glob'`.

## 8. No percent-decoding, and no file read as bytes

- **Blocked programs**: 2 — 21, 30
- **Implementable**: binding-required
- **Rust crate**: `base64`, `hex`, `percent-encoding`, `sha2`, `uuid`
- **Evidence**: `src/stdlib.rs:85`, `src/stdlib.rs:127`, `src/stdlib.rs:128`
  and `src/stdlib.rs:241` — the `base64`, `hash`, `hex` and `uuid` rows now
  bind the `base64`, `sha2`, `hex` and `uuid` crates. The dependency block in
  `Cargo.toml` holds all four.
- **Why**: base64 auth headers, percent-encoded URLs and content hashes are
  interoperability, not convenience. They are also the parts of "read this API"
  that cannot be worked around by any amount of loop.
- **Note**: phase-104 closed the encoding, digest and UUID parts of this rank;
  program 20 (base64) and program 28 (UUID) now answer. Program 21 still needs
  percent-decoding, and program 30 is still blocked because there is no way to
  read a file as bytes: `files.read` answers text, so `hash.sha256` cannot be
  handed the bytes of an arbitrary file. `percent-encoding` remains an open
  binding entry.

```redblue
say percent.decode("a%20b")
```

Refused as `AnalyzerError: Unknown variable 'percent'`.

## 9. No number formatting or number parsing

- **Blocked programs**: 2 — 10, 22
- **Implementable**: implementable-in-Redblue
- **Evidence**: `src/stdlib.rs:551` and `src/stdlib.rs:553` — `to_text` and
  `to_number` are the only two conversions between a number and anything else,
  and neither takes a second argument. There is no precision, no base, no
  thousands separator and no padding. The nearest thing to formatting is
  `src/stdlib.rs:2400`, `to_text(any) -> text`, which is the default rendering.
- **Why**: "print $1,234.50" is the last line of a money program, and
  "parse the integer after the marker" is a step of program 10. Both need the
  same thing — a format the caller states.

```redblue
say to_fixed(1234.5, 2)
```

Refused as `RuntimeError: Unknown function 'to_fixed'`.

## 10. No writer for structured formats (closed by phase-105)

- **Blocked programs**: 2 — 5, 27
- **Implementable**: implementable-in-Redblue
- **Evidence**: `src/stdlib.rs:120` now exposes `csv.parse` and
  `csv.stringify`; `src/stdlib.rs:130` exposes both JSON directions and both
  line-delimited directions; `src/stdlib.rs:240` reads the documented TOML
  subset; `src/stdlib.rs:242` and `src/stdlib.rs:243` read and write zip and tar
  archives.
- **Why it was ranked**: reading data was solved and writing it back was not.
  A program that normalised, filtered or summarised a CSV had nowhere to put its
  result. Phase-105 closed the half that was missing.

```redblue
say csv.stringify([["a", 1], ["b", 2]])
say toml.parse("name = \"Ada\"")
```

## Not surveyed

The gaps below the line, with the same one-line evidence, so a reader who thinks
something is missing can check rather than ask. They are not ranked: each blocks
one program or fewer in the sample, and the tie-break rule puts the ranked
entries first.

| Gap | Evidence | Blocks |
|---|---|---|
| No `sys` surface beyond `argv` and CLI flags: no environment variables, exit code, cwd, platform | `src/stdlib.rs:211` | 31, 32 |
| No set algebra: `unique`, `flatten`, `reverse`, intersection | `src/stdlib.rs:151` | 3 |
| No integer maths or `min`/`max` over a list: `gcd`, `log2` and inverse trig are still absent, and the descriptive statistics (`mean`, `median`, `mode`, `variance`, `standard_deviation`, `percentile`, `correlation`) are now in `math` | `src/stdlib.rs:162` | 8 |
| No token generation beyond `uuid.generate`; `random_choice`/`random_shuffle` exist but no token-shaped call | `src/stdlib.rs:746` | 28 |
| No concurrency, no timeout or retry control on `network.get` | `src/stdlib.rs:192` | 29 |
| No `inspect`-style pretty printer; no assertion library beyond `expect`/`assert` | `src/stdlib.rs:718` | — |
| No YAML/XML reader or writer | `src/stdlib.rs:19` | — |

Four limits of this survey, stated so they are not mistaken for more:

- **The sample is 32 mundane programs, not the world's software.** A gap that
  blocks nothing here can still be the only thing missing from someone's project.
  The ranking is a tie-break, not a proof of importance.
- **Blocked counts overlap.** A program can be listed under several gaps, so the
  counts do not sum to 32 and are not meant to.
- **Nothing here is about the language surface.** Closures, `for each`, records,
  objects and `try` are all present and are not assessed. `modules/*.rb` can
  already add pure-language functions, which is why so many ranks are labelled
  `implementable-in-Redblue`.
- **This survey changed no line under `src/` when phase-090 wrote it.** Where a
  row here could be fixed by one file in `modules/`, that remains a
  recommendation. Phase-101 has since implemented the text operations noted
  under rank 5; the other entries are unchanged.

## What a reader should do with this

Take rank 1 (`filter`, `reduce` and the rest) as the next phase: it unblocks six
of the sample, it needs no dependency, and two of its functions are already wired
into the registry and answering nothing. Then rank 3 (record enumeration), which
is the other half of every data program and whose current failure mode is a
silent zero-iteration loop rather than an error. Phase-101 has taken the padding
half of rank 5; the slicing and searching half remains.

## Re-verification on resume

This document was re-checked against the resumed tree. The registry counts are
reconciled and every `path:line` citation is opened by
`tests/stdlib_gap_survey_test.rs`, which refuses one that does not resolve. The
regex and `module_has_member` citations are also read against the code they
name. Phase-101 added six text builtins, so the count above and the
`src/stdlib.rs` citations were re-anchored to the tree that now ships.
