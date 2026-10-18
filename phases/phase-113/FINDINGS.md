# Findings — phase-113

Work adjacent to the phase's finding that was **not** done here, because
AGENTS.md § 1.7 puts one concern in one phase. Each item is file:line anchored so
the auditor can turn it into a phase without re-measuring.

## 1. `record_literal` in `docs/GRAMMAR.md` §1.4 is not implemented either

`docs/GRAMMAR.md:99` (before this phase's edits; §1.4 "Literals") spells

```
record_literal     = 'record' '{' [ property { ',' property } ] '}'
property           = identifier ':' expression
```

`record` is not a reserved word, so this is the same defect this phase fixed in
§2 — a production that reads a keyword nothing reserves, with no disclaimer.
Verified against the built binary on `main`:

```
$ printf 'set r to record { x: 1 }\nsay r.x\n' > t.rb && rb run t.rb
Error: AnalyzerError: Unknown variable 'record'
  --> t.rb:1:10
```

The record literal Redblue *does* have is spelled `{ x: 1 }`, with the braces
and no leading word. This phase's keyword scan
(`tests/type_annotation_test.rs::every_word_a_marked_production_spells_as_a_literal_is_reserved_or_named_in_its_note`)
deliberately scans only §2 and the five annotation productions; widening it to
§1.4 would have been a documentation sweep, which is the refactor this phase was
told not to smuggle in. The §1.4 note belongs to whoever owns literals.

## 2. `isnt` in `docs/GRAMMAR.md` §1.5 / §5.6 is not implemented

`docs/GRAMMAR.md:120` spells `'isnt'` as an operator alternative in
`not_op`'s neighbourhood (`docs/GRAMMAR.md:104`), and `docs/GRAMMAR.md:130`-era
comparison productions spell it too. It is not a keyword and there is no parser
arm:

```
$ printf 'say 1 isnt 2\n' > t.rb && rb run t.rb
Error: AnalyzerError: Unknown variable 'isnt'
  --> t.rb:1:7
```

Same shape as finding 1 and as this phase's finding: a word in the grammar with
no rule that reads it. Left alone for the same reason.

## 3. Grammar literals outside the productions this phase owns

A scan of every single-quoted word literal in `docs/GRAMMAR.md`'s fenced
productions that is not in §1.3's keyword table turns up, besides §2 and the
five annotation productions:

| Word | Section | State |
|---|---|---|
| `record` | §1.4 Literals | not implemented (finding 1) |
| `isnt` | §1.5 Operators, §5.6 | not implemented (finding 2) |
| `all` | §3.1 Module (`export all`) | parses — `module M` / `export all` / `end` runs |
| `at` | §5.2 Postfix (`items at 0`) | parses — `set items to [1,2]` / `say items at 0` prints `1` |
| `by` | §4.3 Control Flow | parses — `split("a b", by " ")` prints `[a, b]` |
| `length` | §5.1 Primary (`length of items`) | parses — prints `2` |
| `of` | §1.5, §5.1, §5.6 | contextual, not reserved; parses in the spellings above |

The four that parse are the mirror image of this phase's defect: they are words
the grammar spells as literals and that *are* implemented but are *not* in
§1.3, because §1.3 lists only reserved words. That is consistent with the table's
own stated rule ("A word is listed here only while the parser has a rule that
reads it") and needs no change; it is recorded so the next audit does not
re-derive it.

## 4. `SPEC.md:673` still shows a type-annotated signature

`tests/doc_blocks_fragments.txt:59` classifies the block at `SPEC.md:673` as an
excerpt with the reason "a type-annotated signature; `a as number` and `as
number` are not forms the parser accepts". The note added by this phase is in
"Complex Types" and "Explicit Types"; this third block sits in the Functions
section and is a fourth promise of the same unimplemented syntax. It is
classified correctly (a fragment, not a promise the suite runs), so no gate is
red — but a reader who lands there meets no note. Promoting the "Explicit Types"
note's wording to that section is a one-line change for whoever owns the
Functions section.