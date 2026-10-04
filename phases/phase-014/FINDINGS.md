# phase-014 — FINDINGS

Defects found while making `files` / `json` / `csv` / `network` testable and
total that are **not** this phase's concern. Each has file:line evidence and is
left for the auditor to promote.

## 1. The lexer drops the backslash of an escape it does not know — `src/lexer.rs:227`

`Some(c) => text.push(c)` is the catch-all arm of the string reader, so `\u` in a
Redblue literal becomes the character `u` and the backslash is lost. This is the
same class of bug phase-014 fixed inside the JSON reader, one layer up:

```redblue
$ cat target/tmp/phase014/uni.rb
say json.parse("\"\u0041\"")

$ ./target/debug/rb run target/tmp/phase014/uni.rb
u0041                 # the literal was already `u0041` before json.parse saw it
```

Every `\uXXXX` test in `tests/stdlib_modules_test.rs` therefore writes
`\\uXXXX`, which the lexer turns back into `\uXXXX`. A phase that gives Redblue
string literals a `\u` escape should say so itself: it is a language-surface
change, and this one was deliberately not smuggled into a stdlib phase.

## 2. `split_json_pairs` decides "inside a string" from the previous character — `src/vm.rs:1842`

`if c == '"' && prev_char != Some('\\')` treats `\"` as escaped but not `\\"`, so
a key or value whose text ends in an escaped backslash closes the string one
character early and the split runs off into the rest of the document:

```redblue
$ cat target/tmp/phase014/esc.rb
say json.parse("{\"a\\\\\": 1, \"b\": 2}")

$ ./target/debug/rb run target/tmp/phase014/esc.rb
Error: RuntimeError: RuntimeError: Invalid JSON: 1, "b": 2
```

It is a clean error, not corruption, so it is not urgent — but
`json_escape_text` now writes `\\` for a backslash, which makes such a document
reachable from `json.stringify` output, so the next phase that works on JSON
nesting should fix the scanner rather than the symptom.

## 3. Neither JSON nor CSV input is bounded — `src/vm.rs:1543`, `src/vm.rs:1624`

`parse_json` recurses once per nesting level and `json_stringify` recurses once
per level going out; `parse_csv` scans the whole document into memory. Both
terminate — a nested document is charged to the step budget, and a CSV parse is
a single left-to-right scan — but neither has a depth limit (JSON) or a row/column
limit (CSV), so a 10-million-row CSV is parsed in full. The loop bounds phase-013
added do not cover either.

## 4. There is no process module — `src/stdlib.rs:4`

`MODULES` is `console`, `csv`, `files`, `json`, `network`, `time`. There is no
`system.exec`, `process.run` or `os.command`, so the "process modules" half of
this phase's title does not exist. The manifest's own evidence line names only
`files/time/json/csv/network`, and adding a module is a new language surface, so
it was not built here. If it is wanted, the hard question is the same one this
phase had to answer for `network`: what bounds a child process's runtime, its
output and its exit status.

## 5. `time.now` and `time.sleep` cannot be tested without a clock — `src/vm.rs:1049`, `src/vm.rs:1062`

AGENTS.md §3.1 forbids wall-clock dependence in a test, so neither is covered
here. `time.format` and `time.unix` are pure functions of their arguments and
are fully testable — a follow-up phase should cover them along with the CSV/JSON
matrix from this one. Note also that `time_format` casts a negative timestamp
with `as u64` (`src/vm.rs:1087`), which wraps to a huge positive number and is
then rejected as "Invalid timestamp"; an explicit negative-timestamp error would
say what happened.

## 6. `set record["key"] to value` is a parser error — `src/parser.rs`

`set r["a"] to 1` fails with `ParserError: Expected To but got LeftBracket`, so a
record can only be built by `json.parse` or by field assignment on an `object`
declaration. This surfaced while looking for a way to build a record with a key
containing a quote; `json.parse` covers that case, so the test uses it. A record
literal with computed keys is a grammar question, not an stdlib one.