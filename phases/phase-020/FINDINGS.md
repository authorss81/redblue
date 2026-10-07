# FINDINGS — phase-020

Things this phase found that are not its own work. Each is anchored to a file and
a line I read. The auditor promotes what is worth a phase.

---

## 1. Five defects in the bytecode VM and the lowering, found by the harness

The finding this phase was dispatched for — *"no differential or property testing
infrastructure exists"* — is real, and writing the infrastructure is what turned
these up. All five are fixed here because a differential harness that reports its
own corpus is not a harness; the fixes are small and each is mutation-checked in
`REPORT.md`.

1. **`src/bytecode/vm.rs` — the outermost frame threw its value away.** A program
   ending in a bare expression was worth `nothing` on the bytecode VM and the
   expression's value on the tree-walking one. `set t to 5 / say t / t` → `5` vs
   `nothing`. Fixed: the outermost frame's value is the program's value.
2. **`src/bytecode/codegen.rs` — a branch's value leaked to its enclosing block.**
   `self.statements(then_branch, …)` passed `index == last`, so a trailing
   expression in an `if` branch, a loop body or a `catch` body left its value on
   the operand stack. Invisible before, because (1) discarded whatever the stack
   finished with. Fixed by threading `reads_value`.
3. **`src/bytecode/vm.rs` — `SET_PROPERTY` leaked its receiver.** One operand-stack
   slot per field write, forever. `docs/BYTECODE.md:182` already specifies it as
   "pops a value and an object"; the implementation did not.
4. **`src/bytecode/vm.rs` — the inheritance chain was walked backwards.**
   `for (_, object) in chain.iter().rev()` gave the **most distant** ancestor the
   field, which is the opposite of what the function's own documentation says and
   of what `declare_object` does. `corpus/objects-0006.rb`:
   ```
   object Base       →  has tag default "base"
   object Middle extends Base    →  has tag default "middle"
   object Leaf  extends Middle
   say Leaf.tag
   ```
   printed `middle` on the tree-walking VM and `base` on the bytecode one. Fixed.
5. **`src/bytecode/vm.rs` — a reachable `panic!` on a nested `object`.** The VM
   keeps one `pending_object`, so a `DEF_OBJECT` inside another object's body
   overwrote the outer one's and the outer frame then reached for a declaration
   that was gone. Repro: `corpus/objects-0013.rb`, which now runs.
   `thread 'main' panicked at src/bytecode/vm.rs:1026: an object declaration
   being assembled`. Fixed in the **lowering**, not the slot: an object body
   compiles as two halves, `has`/`to can` into the block that assembles the type
   and everything else into the enclosing block after the `STORE`, which is the
   order `declare_object` uses (`src/vm.rs:1346`). The observable difference is
   that the inner declaration now runs *after* the enclosing type is registered —
   `corpus/objects-0015.rb`, an inner `object Child extends Base` written inside
   `Base`'s own body, resolves its parent.

**On `must_touch`.** This phase declares `must_touch: ["tests/"]`, and the
harness is entirely under it. The five defects above are in `src/`, and the
manifest named only `tests/`, which is a manifest bug for any phase whose whole
purpose is to find defects in the thing it tests. Fixed in the area the finding
names rather than bending the phase; the auditor may widen the entry.

---

## 2. 23 of the 25 registered builtins cannot be called from Redblue source

`src/stdlib.rs:14` registers them — `abs`, `floor`, `ceil`, `round`, `sqrt`,
`pow`, `sin`, `cos`, `tan`, `log`, `exp`, `uppercase`, `lowercase`, `trim`,
`split`, `join`, `contains`, `starts_with`, `ends_with`, `replace`, `is_number`,
`is_text`, `is_list`, `is_record`, `to_text`, `to_number`, `to_list`, `push`,
`pop`, `shift`, `filter`, `map`, `reduce`, `random_number`, `random_choice`,
`random_shuffle`, plus the constants `PI` and `E` — and the tree-walking VM
answers every bare call with `RuntimeError: Unknown function '<name>'`:

```
say upper("abc")     → RuntimeError: Unknown function 'uppercase'
say abs(-3)          → RuntimeError: Unknown function 'abs'
say is_number(1)     → RuntimeError: Unknown function 'is_number'
say PI               → AnalyzerError: Unknown variable 'PI'
```

Only `length` and `type_of` are wired (`src/vm.rs`, `call_builtin`). The
module-namespaced forms *are* wired — `files.write(…)`, `json.stringify(…)`,
`csv.parse(…)`, `network.get(…)`, `time.now()` all run — so the gap is exactly
the unqualified form. `AGENTS.md`'s "Math Functions" and "Text Functions"
sections document the unqualified form as working, and `phases/INVARIANTS.md`
is silent about it.

Consequence for this phase: the corpus's `stdlib` family is 18 programs of
`length` and `type_of` where it should be ~60, and the generator's text grammar
has four arms (`+`, `+ literal`, `+ literal first`, doubling) where it should have
`uppercase`, `lowercase`, `trim` and `replace`.

---

## 3. `{interp}` string interpolation is documented, an invariant, and unimplemented

`AGENTS.md` §2 lists *"`{interp}` string syntax"* as a language invariant that may
not change. It does not work:

```
set name to "World"
say "Hello, {name}!"
  → prints   Hello, {name}!
  → should   Hello, World!
say "{1 + 1}"      → prints {1 + 1}
```

`examples/hello.rb:10` contains exactly this, inside `to greet(name)`, and it
prints the braces. `AGENTS.md` §2 also requires `examples/*.rb` to keep working —
they do, in the sense that nothing fails; the example just says something other
than what it says.

The corpus records the current behaviour (`corpus/text-ops-0017.rb`) rather than
the documented behaviour, because a golden corpus records what the interpreter
does. **Either the invariant list or the interpreter is wrong and one of them has
to change.**

---

## 4. Trailing tokens on a line are silently accepted

```
say 1 2 3        → prints 1, and the program is worth 3
say "a" "b"      → prints a, and the program is worth b
set x to 1 2     → binds x to 1, and the program is worth 2
```

The parser stops at the end of the expression it was building and the rest of the
line is not an error. A stray token is either a typo the author wants told about
or a bug in the expression they did write, and today it is neither reported nor
ignored — it silently becomes the program's value. `corpus/text-ops-0023.rb`,
`corpus/text-ops-0022.rb` and `corpus/value-tails-0043.rb` pin the three shapes.

---

## 5. `skip` and `break` are accepted and do nothing

`corpus/loop-forms-0012.rb` prints `found`, `after`: the `break` neither leaves
the loop nor skips the rest of the body. `corpus/loop-forms-0013.rb` puts `skip`
in the same place and prints `2`, `after`: it does not skip.

Both engines agree, so those two programs pin the current behaviour rather than a
divergence. A keyword that lexes, parses and does nothing is worse than a missing
one: `rb diagnostics` cannot report it and a reader cannot see it.

---

## 6. `length` counts bytes, not characters

`length("日本語")` is 9 and `length("🎉")` is 4
(`corpus/unicode-0001.rb`, `corpus/unicode-0002.rb`). `Span`'s own documentation
(`src/error.rs:4-6`) says columns count characters, not bytes, so the two
disagree about what a character is inside one project.

---

## 7. A differential harness is blind to a defect both engines share

Eight of the nine items above are limitations **both** VMs have. A differential
test compares two engines and can only see where they differ, so the whole class
of "both are wrong the same way" needs an oracle the project does not have: the
SPEC, written as executable assertions. That is a different piece of work from
this phase and the auditor may want it.

The one item a differential harness *can* see is §1, and only because the
comparison asks what a program is **worth**, not only what it printed.

---

## 8. An `object` written inside another `object` does not bind its name outside

```
object Base
    has tag default "base"
    object Child extends Base
        has extra default 1
    end
end
say Child.extra
  → AnalyzerError: Unknown variable 'Child'
```

The declaration runs — `corpus/objects-0015.rb` reads `Base.tag` after one and
gets `base`, so the inner `extends` resolved and the body executed — but the
**analyzer** does not declare `Child` in the enclosing scope. A nested type is
reachable only from inside the body it was written in, which makes it close to
unreachable: there is no `new`, so the only way to make one is to write a
statement in the enclosing scope, and there is nowhere to read it from. Both VMs
agree, so the corpus pins the current behaviour rather than a divergence
(`corpus/objects-0013.rb`, `0014.rb`, `0015.rb`).

---

## 9. The harness's VMs take their limits from the environment

`vm::tree_walk` goes through `redblue::run_isolated`, which builds `Vm::new()`,
and `vm::bytecode` builds `BytecodeVm::new()`. Both read `REDBLUE_MAX_STEPS`,
`REDBLUE_MAX_ITERATIONS` and `REDBLUE_MAX_CALL_DEPTH` (`src/vm.rs:314`,
`src/bytecode/vm.rs:532`). So a machine or a CI job that sets them low makes the
corpus fail — the same property every other test in the tree has, and this phase
did not widen it. `Vm::with_limits` / `BytecodeVm::with_limits` /
`run_isolated_with`, which would let the harness pin its own budget, are a small
additive change nobody has asked for; they are not in this phase's diff.

---

## 10. `else if` is in the grammar and in the spec, and neither engine parses it

`SPEC.md:353` writes `if x > 0 … else if x < 0 … else … end` as the example of an
`if`, and `docs/GRAMMAR.md:206` puts `{ 'else if' expression 'then' … }` in
`if_statement` between the `then` block and the optional `else`. Both engines
answer every one of the corpus's three `else if` programs with
`ParserError: Expected End but got Eof`, at the line past the last:

```
if 1 is 1 then
    say "a"
else if 1 is 2 then          ← the parser stops the if here and wants `end`
    say "b"
else
    say "c"
end
```

So the `if/else if/else` coverage this phase's first report claimed was
`if`/`else`, with three refusals counted as successes — the property generator
draws only bare `if` (`tests/common/generator.rs:452`), because a generated
program carrying an `else if` and no injected fault would break the
typed-grammar invariant on the first seed. `tests/differential_test.rs`'s
`edge_the_control_flow_family_covers_the_if_else_it_has_and_pins_the_else_if_it_does_not`
now measures both halves rather than claiming the one that does not exist:
three corpus programs writing an `else if` and all three refused by **both**
engines, and six that run a two-arm `if`/`else`. Which of the two is wrong — the
grammar or the parser — is a decision for whoever owns the language, not for a
testing phase.

---

## 11. A field's `default` may declare an object, and one slot could not hold it

A `has` field's `default` is compiled as an ordinary expression
(`src/bytecode/codegen.rs:353`), so it may call a function — and that function
may declare an `object` of its own, which is still open when the enclosing body
declares its *next* field. The bytecode VM kept **one** `pending_object`, so the
inner declaration took the outer one's place, the outer body's `DefField` wrote
into the inner type, and the outer body then finished a declaration that was gone.
The lowering fix below (`REPORT.md`, §1) narrowed the hole to the lowering but
did not close the shape: the slot is now a stack, and an underflow is a clean
error rather than a `panic!`.

`corpus/objects-0023.rb` is the repro, and both engines now print `7` and `2` for
it.

---

## 12. `cargo test` is flaky, and the flake is two tests sharing `output.txt`

`cargo test --all-targets` failed **twice in six runs** during this round, both
times in `tests/bytecode_vm_test.rs`:

```
---- edge_a_decoded_chunk_runs_identically_to_the_compiled_one stdout ----
thread '…' panicked at tests/bytecode_vm_test.rs:1798:
assertion `left == right` failed: examples/files.rb ran differently after a round trip through bytes
  left:  Outcome { output: [], result: Err("IoError: Failed to read 'output.txt': No such file or directory (os error 2)") }
 right: Outcome { output: [], result: Ok("nothing") }
```

`a_corpus_of_programs_runs_identically_on_both_vms` and
`edge_a_decoded_chunk_runs_identically_to_the_compiled_one` are two threads in one
binary, both of them run every program under `examples/`, and `examples/files.rb`
writes `output.txt` **in the repository root**, reads it, and deletes it at the
end. One thread's delete lands between the other's write and its read. Nothing
about the language is involved, and the deterministic repro needs no test runner
at all:

```
$ for i in $(seq 1 60); do ./target/debug/rb run examples/files.rb >/dev/null 2>/tmp/f$i.err & done; wait
$ grep -c IoError /tmp/f*.err | grep -v ':0' | wc -l
42                       # 42 of 60 concurrent runs fail
```

It is **not this phase's regression**: a `git worktree` of the resume commit
`25e9b68`, with none of this round's changes, flakes the same way (one failure in
twelve single-binary runs). The fix is a `static Mutex` around the two tests, or
running `examples/files.rb` in a scratch directory — the second is better, since a
program under test that writes into the source tree is not hermetic whatever
serialises it — but both touch a pre-existing test file, which this phase does not
touch. Until somebody does, "the suite is green" is a statement about the run and
not about the tree, and any report of it should say which runs it saw.

---

## 13. `rbops/verify.sh` is not in this checkout

`ls rbops` → *No such file or directory*. There is no `rbops/` directory and no
`phases.json` at all, and the task instructions say the pipeline that dispatched
this phase lives outside the checkout and is not to be inspected. The fourth gate
therefore could not be run. In its place the backwards-compatibility check
`AGENTS.md` §2 names was run by hand, because two files under `src/` changed:

```
$ for f in examples/*.rb modules/*.rb; do ./target/debug/rb run "$f"; done
ok examples/files.rb   ok examples/fizzbuzz.rb   ok examples/formats.rb
ok examples/hello.rb   ok examples/test_arithmetic.rb   ok examples/time.rb
ok modules/MathUtils.rb   ok modules/SuiteKit.rb
```

All eight run clean. `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`
and `cargo test --doc` were all run; the counts are in `REPORT.md`.
