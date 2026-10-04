# Phase 010 — FINDINGS

Work that was found while doing phase 010 but is **not** this phase's concern.
Per AGENTS.md hard rule 7 these are recorded here for the auditor to promote
into real phases.

## 1. `Drop for Expr` is recursive and unbounded

**Where:** `src/parser.rs` — `enum Expr` has `Box<Expr>` children, and Rust's
derived `Drop` recurses once per level.

**Evidence:** `set x to 1 + 1 + 1 …` with 50 000 terms parses in linear time and
linear stack, but *tearing the tree down* recurses 50 000 levels deep.

```
$ python3 -c "open('target/tmp/flat100k.rb','w').write('set x to ' + ' + '.join(['1']*50000) + '\n')"
$ ./target/debug/rb format target/tmp/flat100k.rb > /dev/null; echo $?
thread 'main' (5247) has overflowed its stack
fatal runtime error: stack overflow, aborting
134
```

Before phase 010 this aborted. Phase 010 makes it a bounded `Error::Parser`
instead, which is the right failure mode but treats the symptom: a flat, short,
perfectly reasonable line of source is now rejected purely because its `Drop`
would recurse. An iterative teardown (an explicit `Vec` worklist in a
`Drop for Expr` written against `ManuallyDrop`) would remove the need for the
chain guard entirely and let `MAX_NESTING_DEPTH` be raised.

**Suggested phase:** "Iterative teardown for `Expr` and `Statement`".

## 2. The analyzer and VM recurse over the AST with no depth guard

**Where:** `src/analyzer.rs`, `src/vm.rs`. Both walk the tree produced by the
parser recursively. `MAX_CALL_DEPTH` (exported from `src/vm.rs`) bounds *call*
recursion only, not AST-walk recursion.

**Evidence:** after phase 010 the reachable AST depth is at most
`MAX_NESTING_DEPTH + MAX_BLOCK_DEPTH` (64 + 64), so this is currently safe by
construction. There is no independent guard, so if the parser's limits are ever
raised — or a future phase builds AST nodes elsewhere — the analyzer and VM
become the next SIGABRT. Nothing in their code enforces a limit of their own.

**Suggested phase:** "AST-walk depth guard in the analyzer and VM", gated on
being able to construct a deep tree without going through the parser.

## 3. `MAX_NESTING_DEPTH` and `MAX_BLOCK_DEPTH` are independent

**Where:** `src/parser.rs` — `Parser::depth` (reset per statement) and
`Parser::block_depth`.

**Evidence:** a file alternating 60 nested `if`s with 60 nested `[` reaches a
combined tree depth of 120 while neither counter exceeds 64. The two limits are
not a single global tree-depth budget, so the real worst case is roughly twice
the stated limit.

**Suggested phase:** fold into finding 1 — with an iterative `Drop` the limits
can be unified into one tree-depth counter.

## 4. No maximum source size or token count

**Where:** `src/lexer.rs` — `Lexer::new` allocates one `Vec<char>` proportional
to the whole input; `src/parser.rs` allocates one `Vec<Token>` proportional to
the token count.

**Evidence:** a 500 MB file will be read into memory whole. Both stages are
linear, so there is no amplification, but there is no ceiling either. Recorded
identically at the end of phase 009's report; still open.

**Suggested phase:** "Bound source size and token count in the lexer".

## 5. No `cargo bench` target in the repository

**Where:** `Cargo.toml` has no `[[bench]]` section, and `benches/` does not
exist.

**Evidence:** phase 010's DoD asked for "parse is O(n) on a 100k-token input
(add a bench)". Adding one needs nightly's unstable `#[bench]`, so it was
delivered as two timing tests under stable `cargo test` instead. The repository
therefore has no way to measure throughput, only a pass/fail time bound.

**Suggested phase:** "Add a stable criterion or `Instant`-based bench harness".

## 6. `./rbops/verify.sh` cannot be run from this checkout

**Where:** the whole `rbops/` directory is absent (`ls: cannot access 'rbops':
No such file or directory`), so the fourth gate in AGENTS.md section 3.4 is not
executable here and hard rule 1 forbids creating it.

**Evidence:** every phase so far reports the same thing (see
`phases/phase-009/REPORT.md`). The three `cargo` gates and the `examples/*.rb`
run are reproduced by hand instead.

**Suggested phase:** not a code phase — a pipeline/dispatch fix so the checkout
given to implementers contains `rbops/verify.sh`.
