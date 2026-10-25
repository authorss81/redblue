# LIBRARIES.md — what Redblue libraries should be written in Redblue, bound, or not built

**Status**: standing policy. Derived from the ranking in [SURVEY.md](SURVEY.md),
and older than any one stdlib phase. Read this before adding a module: it decides
which of the three answers a missing capability gets, so the next phase does not
have to argue the same question again.

The short version: **write it in Redblue when interpreted speed is irrelevant;
bind it to a Rust crate when the work is numeric, native or security-sensitive;
do not build it at all when the only honest implementation is a native engine
written from scratch.**

## The rule, and the honest speed answer

Redblue's advantage is **readability**, not speed. Its interpreter is slower than Python, and it will stay
slower: an interpreted tree-walker does more work per
operation than a bytecode VM with a mature JIT. Speed is therefore not a design
goal the language should promise, and a library is never chosen for Redblue
because Redblue would make it fast. Interpreted Redblue **cannot out-run a native library**: when the cost of a task is a native kernel, no amount of Redblue source changes that. The fast path is always a binding.

That has one firm consequence. A PyTorch-equivalent — tensors plus autograd —
written in Redblue is **not achievable**, not merely a lot of effort. Tensor and
autograd work is fast because the kernels and memory movement are native; that is
exactly why NumPy and PyTorch are C/C++ with a Python surface. The numpy-style
answer is a **binding**: keep the readable Redblue call, put the kernel in Rust
(or call the crate that already wraps the native engine). Redblue already does
this with `network.get` wrapping `reqwest` and `files.read` wrapping `std::fs`.

The **native backend** is the only thing that would change the speed answer. It
is a separate compiler project, not a library, and this policy does not commit
to building it.

## 1. Written in Redblue

These belong in `modules/*.rb` (or in the language's own tooling) **because** the
work is control flow, text and data structures over values the interpreter
already holds. Interpreted speed is irrelevant at the sizes these libraries are
used, and keeping them in Redblue makes them readable, debuggable and
self-hosting material.

| Library | Ranking | Why Redblue, not a binding |
|---|---|---|
| Collections and higher-order list functions — `filter`, `reduce`, `any`, `all`, `zip`, `chunk`, `group_by`, `sort_by`, `take`, `skip`, `reverse`, `unique` | `SURVEY.md` rank 1 | `map` already calls a Redblue closure and `push` already exists; the missing functions are loops and comparisons, not native kernels. |
| Record field enumeration — `keys`, `values`, iterating a record | `SURVEY.md` rank 3 | A record is a language value with insertion order; exposing its fields is value semantics, not a numeric kernel. |
| Data structures — stacks, queues, sets, trees, graphs, priority queues | class — no top-ten entry | These are algorithms over values the interpreter already owns; at script sizes their cost is control flow, not a native kernel, and keeping them in Redblue keeps them inspectable. |
| Ordering, min/max, sum, mean, median | `SURVEY.md` rank 4 | Sorting and folding are O(n log n) and O(n) over values the interpreter already owns; the readability win is the whole point and a native sort would add a boundary for no practical gain. |
| Text slicing, searching and padding — `slice`, `index_of`, `pad_left`, `repeat`, `chars` | `SURVEY.md` rank 5 | `length` already counts characters in the interpreter; slicing by character is the same Unicode work, written once in Redblue. |
| Number formatting and parsing — fixed precision, separators, a stated base | `SURVEY.md` rank 9 | This is presentation over a number Redblue already has; formatting is arithmetic and text building, not a native dependency. |
| CSV and JSON writers | `SURVEY.md` rank 10 | The readers already produce Redblue lists and records; writing them back out is serialising existing `Value`s, not a new engine. |
| Parsers, tokenizers and compilers | class — no top-ten entry | Scanning text and building trees is exactly what a readable language is good at; the bootstrap compiler is the standing proof that this is Redblue work. |
| Servers and HTTP routing | class — no top-ten entry | Once the socket/HTTP client is bound (`reqwest`), routing, headers and JSON handling are control flow over text and records; a server author benefits from readability more than from native dispatch. |
| Developer tools — formatter, linter, test runner, REPL helpers | class — no top-ten entry | These run at human speed on source files and are the language's own bootstrap surface; writing them in Redblue keeps the tooling honest about the language. |
| Test frameworks and assertions | class — no top-ten entry | `expect`, test blocks and catchable errors already exist; a richer harness is closures and reporting, not a native engine. |

## 2. Bindings to Rust crates

These must be **bindings**, following the numpy pattern, **because** the work is
numeric, native, or security-sensitive. The Redblue surface stays small and
readable; the implementation is a Rust crate behind it, exactly as
`network.get` wraps `reqwest` and `files.read` wraps `std::fs`. Every crate named
here either is already a dependency in `Cargo.toml` or exists on crates.io.

| Capability | Ranking | Rust crate | Why it must be a binding |
|---|---|---|---|
| Regular expressions | `SURVEY.md` rank 2 | `regex` (already a dependency) | A correct, linear-time regex engine is a specialist native engine; hand-rolling one in Redblue would be slower, buggier and a denial-of-service risk. |
| Date and time arithmetic | `SURVEY.md` rank 6 | `chrono` (already a dependency) | Calendar arithmetic, leap years and time zones are a correctness problem; `chrono` already backs `time.format` and `time.unix`. |
| Directories, globbing and stat | `SURVEY.md` rank 7 | `walkdir`, `glob` (crates.io) | Walking the filesystem and matching paths are OS-facing operations with platform edge cases; no amount of Redblue loop removes that. |
| Encodings and digests | `SURVEY.md` rank 8 | `base64`, `hex`, `percent-encoding`, `sha2` (crates.io) | Bit-level conversion and hashing are native work, and a digest implemented in interpreted code is both slow and easy to get wrong. |
| HTTP client — `network.get` / `network.post` | precedent | `reqwest` (already a dependency) | The existing numpy-pattern example: readable Redblue call, native TLS and HTTP stack underneath. |
| File reading and writing — `files.*` | precedent | `std::fs` (Rust standard library) | The other existing example: no reason to reimplement the OS file API in Redblue. |
| Tensor and autograd | class — not in the survey | `tch` (libtorch bindings) or `candle-core` (crates.io) | Kernels and memory movement must be native; this is the PyTorch case and the reason the answer is a binding, not an implementation. |
| Matrix and linear algebra | class — not in the survey | `ndarray`, `nalgebra` (crates.io) | Dense numeric loops are exactly where an interpreter loses by orders of magnitude; bind the optimised loop. |
| Statistics | class — not in the survey | `statrs` (crates.io) | Distributions and special functions are numeric precision work; a Redblue implementation would be slower and less accurate. |
| Cryptography | class — not in the survey | `ring`, `aes-gcm`, `ed25519-dalek`, `sha2` (crates.io) | Primitives must be audited and constant-time; a readable-language implementation invites timing side channels. Bind, never hand-roll. |

A binding is acceptable only when it keeps the Redblue surface readable: one
module name and a small set of English-shaped calls, errors caught as Redblue
errors, and no leak of the crate's types into the language.

## 3. Not built at all

These should not be built, **because** the honest implementation is a native
engine or a specialist project, and building it here would be both worse than the
binding and a distraction from the language.

| Not built | Why |
|---|---|
| A PyTorch-equivalent tensor/autograd library written in Redblue | Not achievable in interpreted Redblue: tensor and autograd work is fast because it is native, which is why NumPy and PyTorch are C/C++ with a Python surface. Bind `tch` or `candle-core` instead. This is the numpy-style answer, not a compromise. |
| A regular-expression engine written from scratch | Correctness, linear-time behaviour and security are specialist work. `regex` already exists and is already linked; writing our own would be slower and less safe for no readability gain. |
| Cryptographic primitives written from scratch | Auditing and constant-time behaviour matter more than readability. Use `ring`, `aes-gcm`, `ed25519-dalek` and `sha2`; do not invent a cipher because it reads nicely. |
| A GPU runtime or CUDA binding written from scratch | Vendor APIs and kernels are native through and through. Reach a GPU through `tch`/`candle-core`, not by reimplementing the runtime. |
| A native backend or optimising JIT as a "library" | It is a compiler project, not a library, and it is the only thing that would change the speed answer. This policy does not commit to building one; if it is ever built, it gets its own phase and its own fixed-point gate. |
| A packaging/distribution ecosystem built before the language can run its own tools | Version resolution, registries and supply-chain policy are not blocked on speed and depend on the language's own tooling maturing first; building them early freezes decisions the language has not earned. Treat as a separate roadmap item, not a library phase. |

## Applying the policy

1. Find the capability in this file. If it is in section 1, write `modules/*.rb`
   and test it in Redblue. If it is in section 2, add a thin binding and test the
   Redblue surface plus its failure channel. If it is in section 3, do not build
   it; bind the crate named above or record the gap in `FINDINGS.md`.
2. Re-measure against [SURVEY.md](SURVEY.md) when a gap closes. A binding that
   starts working turns a `binding-required` entry into an existing surface, not
   into permission to rewrite it in Redblue.
3. Never trade readability for a speed claim the interpreter cannot keep. The
   only honest speed claim is that a native backend would change the answer.
