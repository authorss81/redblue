# Phase 109 — Findings

## F-109-01 — `docs/BYTECODE.md` opening line says format version 4, while the build reads and writes 6

- **Status:** `[OPEN]`
- **Location:** `docs/BYTECODE.md:3`, contradicted by `docs/BYTECODE.md:56` and `src/bytecode/format.rs:81`.
- **Reproduction:**
  ```text
  $ grep -n "^\*\*Format version" docs/BYTECODE.md
  3:**Format version: 4.** This document is the normative description of the file
  $ grep -n "This build reads and writes" docs/BYTECODE.md
  56:A `u16`. This build reads and writes `6`. Any other value is refused with
  $ grep -n "pub const FORMAT_VERSION" src/bytecode/format.rs
  81:pub const FORMAT_VERSION: u16 = 6;
  ```
- **Impact:** A reader of the normative bytecode document can believe version 4 is current, while the encoder writes version 6 and the decoder refuses anything but 6. The rest of the same document and the code agree on 6. NATIVE.md uses 6 and records this drift.
- **Why it was not fixed here:** Phase 109 is a feasibility study and must not change the bytecode format or its normative documentation. The one-line correction belongs to a documentation/spec-drift phase.
