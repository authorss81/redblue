# Phase 041 — FINDINGS

## The phase finding is partly stale, and the live part was narrower than stated

`split` and `join` **do exist** and are reachable in both spellings today. Phase-014
(`27d4aeb`) added them:

```
$ printf 'import text\nsay text.split("a,b", ",")\n' | rb run
[a, b]
$ printf 'set p to split("a,b", ",")\nsay p\n' | rb run
[a, b]
```

`src/runtime.rs:449` (`pub fn builtin`) is not where they live — they are in
`src/stdlib.rs:357` and `:372` (`builtin_function`), which is what
`src/stdlib.rs:472` (`stdlib::builtin`) reaches. So a future finding generated
from the same source line will be wrong again unless it checks `stdlib.rs`.

What **does** reproduce is the `by` label SPEC.md spells at `SPEC.md:1017-1018`.
That is what this phase fixed.

## Not fixed, needs a phase

### 1. `verify.sh` does not exist in the checkout

```
$ ls rbops
ls: cannot access 'rbops': No such file or directory
$ ./rbops/verify.sh phase-041
bash: ./rbops/verify.sh: No such file or directory
```

`rbops/` is absent from the working tree and the phase brief forbids inspecting
the pipeline that dispatched this phase, so the fourth gate was not run. It is
reported as **unrun** in `REPORT.md`, not as a pass. The three that could be run
(`cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --all-targets` —
1198 passed, 0 failed) are green.

### 2. A dotted spelling through the wrong module succeeds

`src/stdlib.rs:529` accepts `module_member_name` when the prefix is in `MODULES`
and the remainder is in `BUILTIN_NAMES`, with **no** check that the member
belongs to that module. So all of these are the same `split`:

```
say text.split("a,b", ",")
say math.split("a,b", ",")
say csv.split("a,b", ",")
```

Verified against the built binary: all three run and all three print `[a, b]`.

Whether to refuse the wrong module is a language decision, not a bug fix: it
would turn a spelling that works today into an error, so it needs its own phase
with its own acceptance gate. Documented in `REPORT.md`'s reachability table and
pinned for 14 names by `reachability_table_holds_for_every_row`.

### 3. The lexer has no `\u` escape

`"😀"` in a Redblue source is the nine ASCII characters `u{1F600}`,
not an emoji:

```
$ printf 'say "a\\u{1F600}b"\n' | rb run
au{1F600}b
```

`AGENTS.md` § 3.2 lists "Unicode and escapes" as a mandatory matrix row, and the
only escape the lexer has is `{interp}`. A self-hosted lexer needs to read its
own source, and any non-ASCII literal in a source file is currently
unrepresentable in the language. This is a real capability gap, not a cosmetic
one, and it is out of scope for a `split`/`join` phase. The unicode test in
`tests/text_split_join_test.rs` puts the real characters in the source and
documents why, so the gap is visible from the test that would otherwise be
expected to cover it.

### 4. `say` renders `[""]` and `[]` identically

```
$ printf 'say [""]\n' | rb run
[]
$ printf 'say []\n' | rb run
[]
```

A one-element list holding an empty string prints the same as an empty list, so
any program that prints a split result cannot tell them apart. Not a defect in
`split` — the values are correct and `length` distinguishes them — but a
rendering ambiguity that would hide a real bug elsewhere. `src/parser.rs`'s
`say` rendering is not this phase's concern; the tests here print each part in
its own brackets to route around it.

### 5. The empty separator has no answer, and that is now pinned rather than decided

`split("abc", by "")` is refused with `split has no answer for these arguments`
(`src/stdlib.rs:481` turns the `nothing` from `src/stdlib.rs:362` into an error).
Redblue has no regex, so an empty separator genuinely has no answer — but *error*
is one of at least three defensible choices (empty list, one-character split,
error), and which one is right is a language decision. This phase pins the
current one so a later change is deliberate. `join` with an empty separator is
well defined and concatenates; that asymmetry is pinned too.
## Round 1 — the four review findings, all fixed here

None of these is a new gap; they are defects in the change this phase made, and
each is fixed in `src/parser.rs` with a test that fails without the fix. They are
written up in `REPORT.md` § Round 1 rather than repeated:

1. **[BLOCKER]** the label test was a two-token blacklist, so `by` in front of an
   operator became a label — now `Parser::kind_starts_expression`, the same table
   `is_expression_start` uses.
2. **[MAJOR]** every call accepted a `by` label although SPEC.md defines it for
   `split`/`join` only — now `Parser::callee_takes_argument_label`, pinned on both
   sides.
3. **[MAJOR]** the lookahead skipped a newline the parse did not — now
   `skip_newlines()` on both sides, with the label across two lines pinned.
4. **[MINOR]** the test-file header contradicted the pinned dotted/import rule —
   corrected; the import is optional.

The gaps below were not touched by round 1 and are unchanged by it.
