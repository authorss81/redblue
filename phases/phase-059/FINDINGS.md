# Phase 059 — FINDINGS

Work that is real, evidenced, and **not** this phase's. Recorded for the auditor.

## F1 — an unknown variable is reported at the statement, not at the name

**Evidence.** `src/analyzer.rs` records the span of the expression that read the name
rather than the identifier's own span. Reproduced at `d514651` and unchanged since:

```
$ printf 'say nosuchvar\n' > probe.rb && rb run probe.rb
Error: AnalyzerError: Unknown variable 'nosuchvar'
  --> probe.rb:1:1
1 | say nosuchvar
  | ^
```

The name starts at column 5; the caret is at column 1.

**Why it matters.** A diagnostic is most useful when the caret is under the thing
that is wrong. Today the reader is shown the line (good) and then told the *statement*
is at fault, which is the same information as the line above it. Cases where the span
does land mid-line are already good: `set x to 1 +` → `1:13`, an unterminated string →
the `~~~` over the literal.

**Why it is not phase-059.** Phase 059's finding is that the REPL discards the
rendering the language already owns. Fixing that means showing the same caret `rb run`
shows, which is what this phase does. Moving the span is a change to what a position
*means* for one class of analyzer error, it changes `rb run`, the LSP
(`src/lsp.rs` consumes spans) and `wasm.rs`, and it would make existing golden
diagnostics move. It needs its own phase with its own acceptance gate.

**Suggested phase.** "Point an unknown-variable diagnostic at the identifier", with
`must_touch: ["src/analyzer.rs"]`, gated on the existing span tests plus a new one
asserting `1 | say nosuchvar` / `  |     ^` from `rb run`.

## F2 — `rbops/verify.sh` is not in this checkout

`ls rbops/` → *No such file or directory*, third phase running (057, 058, 059). The
four cargo gates were run and are logged; the fifth cannot be. Not a defect in the
tree — the pipeline lives outside it — but it means no phase in this checkout has ever
been verified by the script that is supposed to be the authority on whether work is
real, and each phase reports that in its own `GATES` table instead.