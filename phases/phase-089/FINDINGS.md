# Phase 089 — findings

Three findings from the review round that followed the first pass, all fixed
here, and the one that was recorded and left open is now closed with them. Each
entry says what the defect was, what it is now, and which gate holds it in
place.

## 1. Stage 2 (`bootstrap/compiler.rb`) mis-compiled `catch error of <Type>`

**Severity:** blocker. **Fixed here.**

### What happened

`catch error of <Type>` is new in this phase. The Rust frontend
(`src/parser.rs`) reads `of <Type>` where the catch line is read. Stage 2 did
not: `parse_try_catch_body` had arms for `catch error <name>` and `catch
<name>` and no arm for `of`, so

```redblue
catch error of Runtime problem
```

took the `error` + same-line-identifier arm, bound a name spelled **`of`**, and
left `Runtime` as an orphan statement of its own. `compile_try` then wrote the
catch block with an empty parameter list, because there was no kind to write.
Stage 2 did not refuse the program: it wrote a `.rbc` that differed from stage
1's in bytes and failed at run time with `Unknown variable 'Runtime'`.

Every byte-identity test in `tests/bootstrap_selfhost_test.rs` compares programs
**both stages accept**, and `corpus/` held no typed catch, so the gap was an
absence rather than a failure — the same shape as `phase-080`'s `might fail`
finding, one step worse: there, stage 2 refused; here it wrote different bytes.

### What it is now

- `parse_try_catch_typed` (`bootstrap/compiler.rb`) reads `of` + a kind on the
  same line as `error`, refuses a name outside the closed list the way
  `Parser::failure_kind` does, reads the optional name after the kind, and binds
  `error` when the source wrote none. `parse_try_catch_body` asks it before the
  plain arms, so `of` can no longer be taken as a binder.
- The try node carries the kind beside `cn`, and `compile_try_catch_params`
  writes it into the catch block's parameter list — the field
  `nested_block`'s 5th argument already was and the one the VM reads the kind
  from. `arity` stays `0`.
- `of` with no kind after it is refused by **both** stages, with the same
  message, rather than read as a binder.

### What holds it in place

- `corpus/runtime-errors-0020.rb` — a typed catch in the corpus, so the
  difference is a byte-identity case on both the S2 (`stage2_is_byte_identical_
  on_its_corpus_families`) and S3 (`edge_stage3_is_byte_identical_on_every_
  corpus_program`) paths rather than an absence. Both counts moved with it:
  315 compared of 324 in the 15 families.
- `stage2_is_byte_identical_for_each_statement_kind` carries a named
  `try_catch_typed` program with both spellings and a `finally` beside them.

## 2. A `.rbc` naming a kind this build does not know caught **every** failure

**Severity:** blocker. **Fixed here.**

### What happened

`push_handler` (`src/bytecode/vm.rs`) read the catch block's parameter with
`FailureKind::from_name(name)` and collapsed "no parameter" and "a parameter
this build does not know" into one `None`, which `handle_failure` read as
*untyped* — that is, a handler that matches every failure and binds the word
`error`. The comment at the read and `docs/BYTECODE.md` both said the opposite
("a name it does not recognise matches no failure rather than every one"), and
the comment was the one that was wrong: a `.rbc` written by a build that had a
kind this one does not would have had that handler silently widened into one
that catches everything.

It is unreachable from the frontend — `Parser::failure_kind` refuses a name
outside the closed list — so no test could see it: the only way to write one is
to edit a compiled chunk.

### What it is now

`Handler::catch_kind` is `Option<Option<FailureKind>>`: the outer option is the
untyped case and the inner one is a name that does not resolve, and the decline
test asks whether it resolves as well as whether it matches. A handler naming a
kind this build does not know matches no failure, its `finally` is still owed,
and the failure leaves the program as uncaught.
`edge_a_catch_naming_a_kind_this_build_does_not_know_matches_no_failure` builds
such a chunk, runs it, and requires the body not to run and the failure to be
reported.

The recorded shape of the decision is kept: a format version that rejects an
unknown kind on decode is still not made, because this phase deliberately did
not move the encoding. What changed is that a handler in such a file now does
what it says.

## 3. `src/failure.rs` promised both engines build the same value field for field

**Severity:** major. **Fixed here.**

The module said the two engines bind "the same value, field for field". They
build the same four fields in the same order by the same function, and `kind`,
`message` and `line` agree — but `column` does not, because the two engines do
not raise the same failure: a `.rbc` instruction carries a line and no column,
so `line_span` (`src/bytecode/vm.rs`) reports column 1 where the tree-walking
VM reports the column of the failing statement. `tests/typed_catch_test.rs`
already said so for its own comparisons; the module comment did not.

The comment now promises what is true: the same build, the same four fields in
the same order, `kind`/`message`/`line` equal across engines, and `column` per
each engine's own convention.

## 4. The default typed binder was written about and never read

**Severity:** major. **Fixed here.**

`catch error of Io` with no name binds `error` to the record
(`src/parser.rs`), and every typed test in `tests/typed_catch_test.rs` spelled a
name after the kind, so the one path a program takes when it writes the short
form was covered by the formatter test alone.
`edge_a_typed_handler_that_wrote_no_name_still_binds_the_word_error` reads
`error.kind`, `error.message`, `type_of(error)` and compares the record against
one built from its own fields — on both engines.

## 5. `catch error of` with no kind bound the name `of`

**Severity:** minor. **Fixed here.**

`catch error of` on a line of its own fell through the typed read to the
binder arm, so a half-written kind compiled into an untyped handler binding a
name spelled `of` — the opposite of the closed-list refusal one token later.
`of` is now the head of a typed catch and the kind after it is required:
`Parser::failure_kind` refuses a missing one (and one on the next line, which is
the body of a handler that named none), quoting the four names that exist.
Stage 2 refuses the same two shapes with the same message.
`edge_of_without_a_kind_is_refused_rather_than_read_as_a_binder` pins all three
shapes on both engines.

## 6. `n as f64` for a position

**Severity:** minor. **Fixed here.**

`position` (`src/failure.rs`) narrowed a `usize` to the language's `f64`. Every
number Redblue holds is an `f64` and the largest it can write back out is 2^53,
so a position past that is not one a source file has and not one the conversion
rounds to — it saturates at the largest number there is instead. The rule the
comment claimed ("built through the same arithmetic every other number in the
language is") is now what the code does.

## 7. `line`/`column` are `0`, not absent, for a failure with no position

**Severity:** observation. **Not changed.**

`value_of` (`src/failure.rs`) writes `0` for a failure the host raised without a
span — every `Io`, since `Error::span` returns `None` for it
(`src/error.rs`). `0` is a line no source file has, which is how the record says
"nowhere" without a fifth field and without an `Option` a program would have to
unwrap.

Not acted on: `might fail` returns `nothing`, so the question a program usually
asks is already answered by `nothing`, and a fourth spelling of "no position"
would be a language question rather than a bug. Recorded so the choice is
visible to the next reader of `failure.rs`.