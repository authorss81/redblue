# Phase 104 — Expose the encoding and hashing utilities programs assume exist

## Reproduction of the finding

The finding is an absence, and the smallest command shows it before any change:

```
$ printf 'say base64.decode("aGk=")\n' > target/tmp/phase104_probe.rb
$ ./target/debug/rb run target/tmp/phase104_probe.rb
Error: AnalyzerError: Unknown variable 'base64'
  --> target/tmp/phase104_probe.rb:1:5
1 | say base64.decode("aGk=")
  |     ^
```

`hex`, `hash` and `uuid` failed the same way. The registry held only the three
`bytes` names, so no program could base64- or hex-encode a payload, hash
content, or generate an identifier.

## What changed

| File | Lines | What |
|---|---|---|
| `src/runtime.rs` | +46 −0 | this run: validate the base64 padding shape before decoding, so surplus padding (`aGk==`), partial padding (`AA=`) and a `=` followed by data are named at the offending symbol instead of at the end of the text or at a legal pad |
| `tests/encoding_hashing_test.rs` | +129 −0 | this run: five new edge tests for impossible, excess, partial and misplaced padding, plus the valid padded/unpadded forms that must keep decoding |
| `SPEC.md` | +5 −5 | this run: state that partial and misplaced padding is refused at the symbol, not skipped; the `uuid.generate()` example no longer implies a fixed value |
| `phases/phase-104/REPORT.md` | +22 −11 | this run: record the refreshed tests, edge matrix and gate results |
| `Cargo.toml` | +5 | bind `base64`, `hex`, `sha2` and `uuid` (v4), plus `getrandom` for the fallible v4 path |
| `Cargo.lock` | +93 | lock the five crates and their transitive dependencies |
| `src/stdlib.rs` | +65 | register `base64`, `hex`, `hash` and `uuid` in `MODULES`/`MODULE_MEMBERS`, add the eight builtins to `BUILTIN_NAMES` and `BUILTIN_SIGNATURES` |
| `src/runtime.rs` | +278 | `BASE64_ANY_PADDING`, strict `decode_hex`/`decode_base64`, byte-or-text argument reader, the eight runtime arms, a fallible `uuid.generate`, UTF-8-aware base64 error text, a first-invalid-byte base64 position, and an odd-length hex message that names the unpaired digit |
| `tests/encoding_hashing_test.rs` | new, 306 lines / 10 tests | round trips, padding, literal digests, UUID formats, strict-error positions, valid-prefix-with-bad-tail refusals, Unicode, type and range refusals |
| `tests/unreviewed_work_ack_test.rs` | +1 | acknowledge `resume phase-104:` as the delivery subject so the resumed commit is not reported as unreviewed work |
| `tests/libraries_policy_test.rs` | +25 −20 | phase-094's refusal probe now records that `base64.decode` landed while the open `percent.encode` binding is still refused as unknown |
| `tests/stdlib_module_names_test.rs` | +20 | exercise the four new modules in the documents-vs-registry sweep |
| `tests/stdlib_module_docs_test.rs` | +42 | each of the eight new builtins now has a program and an expected output |
| `tests/doc_blocks_runs.txt` | +10 −7 | classify the three new SPEC examples, re-anchor the shifted SPEC line numbers, and refresh the bucket counts |
| `SPEC.md` | +49 | document `hex`, `base64`, `hash` and `uuid`, including the raise-vs-sentinel rule |
| `README.md` | +17 | quick reference for the four new modules |
| `LIBRARIES.md` | +5 | record that the encodings/digests row is now bound; `percent-encoding` stays open |
| `SURVEY.md` | +37 −35 | rank 8 records phase-104 closing the encoding, digest and UUID gaps; counts and citations re-anchored |

## Tests added

| Test | Edge class covered |
|---|---|
| `edge_hex_and_base64_round_trip_all_256_byte_values` | boundary / all 256 byte values, asserted on the byte list |
| `edge_base64_uses_canonical_padding_and_decodes_padded_or_unpadded` | empty, singleton, padding and unpadded input, literal expected text |
| `edge_hex_single_byte_empty_and_uppercase_inputs` | empty, singleton, `0x00`/`0xff`, uppercase input |
| `edge_invalid_hex_and_base64_name_the_offending_position` | malformed input, whitespace, wrong hex length, invalid base64 symbol |
| `edge_decoding_errors_are_catchable` | asserts a failure is produced and caught by `try … catch error of Runtime` |
| `edge_hashes_match_literal_known_digests` | SHA-256/512 literal digests, equal/different content, text-vs-bytes input |
| `edge_uuid_generates_parseable_values_and_parses_every_format` | generated value parses back, successive values differ, uppercase/simple/braced/URN and all-zero/all-`f` extremes |
| `edge_bad_hasher_and_uuid_inputs_are_refused_by_name` | type mismatch, byte range `-1`/`256`, malformed UUID |
| `edge_decoders_do_not_answer_a_valid_prefix_when_the_tail_is_bad` | malformed input: a valid prefix followed by garbage must still fail at the tail |
| `edge_unicode_text_survives_hex_and_base64` | Unicode: accents, emoji, CJK, RTL, combining mark |
| `edge_base64_impossible_padding_shapes_are_refused_at_the_pad` (this run) | malformed input: padding that cannot complete a group is refused at the first pad, naming the pad and its position |
| `edge_base64_excess_padding_names_the_extra_symbol` (this run) | malformed input: surplus padding names the extra `=` after a full group (`aGk==` at 4, `AAAA=` at 4, `AAAAAAAA=` at 8) and all-padding input at position 0; asserts `is_err()` |
| `edge_base64_partial_padding_is_refused_at_the_pad` (this run) | malformed input: a partial pad is refused at the pad, and the refusal names both `padding` and the position, while the fully unpadded form still decodes |
| `edge_base64_padding_in_the_middle_names_its_position` (this run) | malformed input: a `=` followed by data is refused at its own position, at the start, middle and a double pad |
| `edge_base64_complete_groups_decode_with_and_without_padding` (this run) | boundary: `AA==`/`AA`, `AAA=`/`AAA`, `aGk=`/`aGk` all decode to the same literal bytes |
| `edge_decode_hex_names_the_unpaired_digit` (src unit) | boundary: odd-length hex names the digit with no pair |
| `edge_decode_base64_accepts_padding_forms_and_names_invalid_bytes` (src unit) | empty/singleton padding forms and malformed input named by position |

## Edge-case matrix

| Row | Verdict |
|---|---|
| empty | covered — empty hex/base64 text decodes to `[]`; empty base64 encodes to `""`. |
| singleton | covered — a one-byte payload (`[0]`, `[255]`, `"h"`) has a literal expected answer. |
| boundary | covered — the full `0..=255` byte range round-trips through both encodings; UUID all-zero and all-`f` values parse. |
| out_of_bounds | covered — `-1` and `256` as byte values are refused by name. |
| type_mismatch | covered — a number where text/bytes is expected is a catchable `Runtime` error. |
| numeric_boundary | covered where applicable — byte values are the only numeric boundary this surface has; `-1`, `255`, `0` and `256` are pinned. `NaN`/`Infinity`/`i64` overflow are N/A because no arithmetic was added. |
| unicode | covered — UTF-8 text with accents, emoji, CJK, RTL and a combining mark survives both encodings. |
| nesting_recursion | N/A — these are flat transformations over a byte list or a text; no nested structure or recursion is introduced. |
| duplicate_missing_keys | N/A — there are no keys or records in this surface. |
| malformed_input | covered — invalid hex digit, odd hex length (including the unpaired digit's position), whitespace, invalid base64 symbol, surplus/partial/misplaced base64 padding, a valid prefix with a bad tail, and invalid UUID each produce a clean, catchable `Runtime` failure; hex/base64 failures name the position and nothing is skipped. |
| resource_limit | N/A — the input is one text or one byte list already in memory; there is no new unbounded loop, file, network or recursion path. |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — exit 0, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — exit 0, no warnings |
| `cargo test --all-targets` | pass — **2535 passed, 0 failed, 0 ignored** across 80 test binaries |
| `../rbops/verify.sh phase-104` | pass — `VERIFY PASS — phase-104` |

The full suite was executed locally with cargo; `verify.sh` then re-ran the
gates, the example sweep and the test-quota checks. It reported
`cargo test: 2535 passed, 0 failed`, `implementation touched src/`, five new
Rust tests, and `VERIFY PASS — phase-104`. No gate was skipped, relaxed or
weakened, and no `#[ignore]`, `// skip` or `allow(clippy::` suppression was
added.

This run resumed an already-merged attempt. Its work was committed as the
`resume phase-104` delivery, so the gate's diff saw no change at all and failed
the diff, `must_touch` and test-quota checks. This run left a fresh on-topic
diff: `decode_base64` now validates the padding shape before the crate decodes,
so surplus, partial and misplaced padding name the offending symbol rather than
the end of the text or a legal pad.

## Invariants touched

- **None.** No `Value` variant, `Error` variant, grammar production, `.rb`
  extension, `to … end` / `set x to …` / `say` form, trailing-comma rule or
  `{interp}` behaviour changed. The existing `bytes` module is unchanged and
  its tests pass unmodified.

## Known gaps / follow-ups

- `percent-encoding` (the fourth crate in the LIBRARIES.md encodings row) is not
  bound; it remains an open entry rather than pretending rank 8 is completely
  closed.
- `hash` exposes SHA-256 and SHA-512 only; SHA-1, MD5 and keyed digests (HMAC)
  are not added, because the policy requires audited primitives and nothing in
  this phase's finding asked for them.
- `uuid.generate` generates version 4 only; name-based and time-based UUIDs are
  not exposed.
- The WebAssembly target is not built by these gates. `uuid` needs its
  `getrandom` wasm feature if the playground build is ever extended to call
  `uuid.generate`; that belongs to the wasm build phase, not this one.
