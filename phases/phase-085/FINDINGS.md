# FINDINGS — phase-085

Work that does not belong to this phase, or that this phase could not finish.
File:line anchored, per AGENTS.md § 6.4.

## 1. `\u` above U+FFFF has no spelling — candidate for a grammar phase

`docs/GRAMMAR.md:80` fixes the escape at four `hex_digit`s, so U+FFFF is the
largest code point it can name and `😀` (U+1F600) cannot be written as an
escape at all. Implemented behaviour is pinned by
`tests/unicode_escape_test.rs::edge_five_hex_digits_are_an_escape_then_ordinary_text`:
`\u1f600` decodes as `\u1f60` followed by a literal `0`, and `\u{1f600}` is
refused.

A braced form (`\u{1F600}`, Rust's, or `\U0001F600` as JavaScript's) would widen
this, and the extension is cheap: `src/lexer.rs:342` `read_unicode_escape` is
the only place that reads the digits. **This phase does not do it** — a grammar
change is AGENTS.md § 1.8 "no unbounded speculation" and belongs in its own
phase, with its own decision about surrogate pairs.

## 2. `rbops/verify.sh` is not in this checkout

The phase prompt names `./rbops/verify.sh phase-085` as the fourth gate, and
AGENTS.md § 3.4 lists it. The directory does not exist under the project root;
the RBOPS pipeline lives elsewhere and this run was instructed not to inspect
it. The other three gates were run and are green, and the things `verify.sh`
adds beyond them were run by hand — all of it is listed in `REPORT.md` under
**Gates**. Nothing about this phase's correctness depends on that file; the
gate result itself is recorded as not-run rather than claimed.

## 3. `cargo build` runs locally

AGENTS.md § 1.4 says CI is the only place `cargo build` runs. It was needed
once, to check the phase's first definition-of-done line end to end
(`rb run`, then `rb compile` followed by `rb vm`, on a file carrying a `\u`
escape). Everything else used `cargo check`/`cargo test`. Noted rather than
hidden; the alternative was to claim the binary check without running it.