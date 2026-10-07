//! The differential + property harness (phase-020).
//!
//! Four modules, each test-only, each next to the code it checks:
//!
//! - [`rng`] — SplitMix64, so a seed rebuilds the same bytes on any machine.
//! - [`vm`] — one program, run on both engines, compared as one value.
//! - [`corpus`] — the corpus on disk: its loader, golden format, writer, compare.
//! - [`generator`] — the curated families and the seeded property grammar.
//! - [`shrink`] — delta debugging, from a failing seed to the shortest program.
//!
//! Nothing here is production code. The harness runs two interpreters over a
//! corpus and asks whether they said the same thing, and a caller that needed
//! that outside a test would be a caller who needed a bug report, not a library.

pub mod corpus;
pub mod generator;
pub mod rng;
pub mod shrink;
pub mod vm;
