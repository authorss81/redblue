# Phase 059 — FINDINGS

Work that is real, evidenced, and **not** this phase's. Recorded for the auditor.

## F1 — ~~an unknown variable is reported at the statement, not at the name~~ (done here)

**Evidence.** At `d514651` the analyzer recorded the span of the expression that read
a name rather than the identifier's own span, so the caret under an unknown variable
pointed at the `say` around it:

```
$ printf 'say nosuchvar\n' > probe.rb && rb run probe.rb
Error: AnalyzerError: Unknown variable 'nosuchvar'
  --> probe.rb:1:1
1 | say nosuchvar
  | ^
```

The name starts at column 5; the caret is at column 1. This was recorded here as
follow-up work because moving the span changes `rb run`, the LSP
(`src/lsp.rs:130` consumes spans) and `wasm.rs`. **Phase 059 did it**: the parser
records a span per name (`Expr::Variable(String, Span)`), the analyzer reports the
name through `name_position`, and the position lands under the identifier
(`1:5`, and `1:9` for `say 1 + nosuchvar`, `1:12` for a name inside `{}`).

The four corpus goldens that pinned the old positions and
`tests/tooling_grammar_test.rs::diagnostics_surface_a_spanned_analyzer_error` were
updated to the new ones; the behaviour is pinned by
`tests/span_test.rs::edge_an_unknown_variable_is_reported_under_the_name_it_names`.
There is nothing left to do here.

## F2 — `rbops/verify.sh` is not in this checkout

`ls rbops/` → *No such file or directory*, third phase running (057, 058, 059). The
four cargo gates were run and are logged; the fifth cannot be. Not a defect in the
tree — the pipeline lives outside it — but it means no phase in this checkout has ever
been verified by the script that is supposed to be the authority on whether work is
real, and each phase reports that in its own `GATES` table instead.