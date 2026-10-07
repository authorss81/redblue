//! Corpus-driven differential runner, plus a property generator (phase-020).
//!
//! The bootstrap ladder is proved by a fixed point: stage 2 must produce
//! byte-identical output to stage 1 for a corpus. A corpus that is not checked
//! cannot be the thing that proves anything, so this file supplies one —
//! `corpus/NNNN-*.rb`, each paired with a `.expected` file recording what the
//! program printed, what it was worth, and how it failed.
//!
//! Every program is run twice: once on the tree-walking VM, which is the
//! specification of what a Redblue program *does*, and once through the bytecode
//! compiler and the bytecode VM. A corpus program whose two outcomes disagree is
//! a divergence, and the runner names the file rather than an exit code, because
//! the comparison runs the VMs as libraries. It also **shrinks** the program that
//! diverged and prints the smallest one that still does, because a name alone is
//! not an actionable bug report.
//!
//! The `.expected` files are the recorded outcome of the tree-walking VM and are
//! checked in: this runner only ever **reads** them, so a behaviour change shows
//! up as a failure rather than as a golden file quietly rewritten under the test
//! — and no test here writes `corpus/`, because a test that refreshes the goldens
//! it is checking cannot fail on a regression in them.
//! `RB_WRITE_CORPUS=1 cargo test --test differential_test` writes what the
//! generator produces to `target/tmp/differential-refreshed` and still fails on
//! the difference, which is what makes it safe to run at all.
//!
//! ## The corpus lives at `corpus/`, not `tests/corpus/`
//!
//! `redblue::testing::find_test_files("tests")` walks `tests/` recursively and
//! `tests/redblue_suite_test.rs` requires every `.rb` file it finds to declare
//! Redblue `test` blocks carrying an assertion. A corpus program declares none,
//! and "a corpus program is not a test" is the right thing for that gate to say,
//! so the corpus goes where the suite's collector does not walk.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::corpus::{self, Expected};
use common::generator::{self, Fault};
use common::rng::DEFAULT_SEED;
use common::shrink;
use common::vm::{self, Outcome, Typed};

/// How many seeds the property tests draw.
const SEEDS: u64 = 200;

/// Whether this run also writes what the generator produces **somewhere else**.
///
/// It never writes `corpus/`. A test that rewrites the golden files it is
/// checking launders a regression into a green run, and every other test in this
/// file reads that directory while it would be being rewritten — so parallel
/// test threads race a half-written corpus. `RB_WRITE_CORPUS=1` therefore prints
/// a refreshed copy under `target/tmp/` and leaves the comparison below to fail
/// on the difference.
fn writing() -> bool {
    std::env::var("RB_WRITE_CORPUS").is_ok_and(|v| v == "1")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/tmp")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir should be creatable");
    dir
}

fn source_of(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} should be readable: {e}", path.display()))
}

// --- the corpus --------------------------------------------------------------

#[test]
fn corpus_holds_at_least_two_hundred_programs() {
    let programs = corpus::corpus();
    assert!(
        programs.len() >= 200,
        "the corpus is what makes a fixed point provable; it holds {} programs, not 200",
        programs.len()
    );
}

#[test]
fn every_corpus_program_has_an_expected_output_file() {
    for path in corpus::corpus() {
        let text = corpus::expectation_of(&path).unwrap_or_else(|e| panic!("{e}"));
        corpus::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }
}

#[test]
fn every_corpus_program_prints_what_its_expected_file_records() {
    let mut checked = 0usize;
    for path in corpus::corpus() {
        let expected = corpus::expectation(&path).unwrap_or_else(|e| panic!("{e}"));
        let outcome = vm::tree_walk(&source_of(&path));
        if let Err(e) = corpus::compare(&path, &outcome, &expected) {
            panic!(
                "{e}\n--- the recorded outcome ---\n{}",
                corpus::render(&expected)
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "the comparison ran over nothing");
}

#[test]
fn every_corpus_program_agrees_between_the_two_vms() {
    let mut divergences = Vec::new();
    for path in corpus::corpus() {
        let source = source_of(&path);
        let tree = vm::tree_walk(&source);
        let byte = vm::bytecode(&source);
        if !tree.agrees_within_format_limits(&byte) {
            let minimal = shrink::shrinks_to(&source, |candidate| {
                !vm::tree_walk(candidate).agrees_within_format_limits(&vm::bytecode(candidate))
            });
            divergences.push(match minimal {
                Some(minimal) => format!(
                    "{}:\n  tree-walking VM: {:?}\n  bytecode VM:    {:?}\n{}",
                    path.display(),
                    tree,
                    byte,
                    shrink::report(0, &source, &minimal),
                ),
                None => format!("{}:\n  {tree:?}\n  {byte:?}", path.display()),
            });
        }
    }
    assert!(
        divergences.is_empty(),
        "{} corpus program(s) the two VMs disagree about:\n{}",
        divergences.len(),
        divergences.join("\n"),
    );
}

/// The corpus is not allowed to become a corpus of successes.
#[test]
fn the_corpus_holds_programs_that_must_fail() {
    let mut failing = 0usize;
    let mut labels: Vec<String> = Vec::new();
    for path in corpus::corpus() {
        let expected = corpus::expectation(&path).unwrap_or_else(|e| panic!("{e}"));
        if expected.must_fail() {
            failing += 1;
            labels.push(expected.label.clone());
        }
    }
    assert!(
        failing >= 40,
        "only {failing} corpus programs record a failure; the corpus must keep the \
         error paths covered too",
    );
    labels.sort();
    labels.dedup();
    for label in ["AnalyzerError", "LexerError", "ParserError", "RuntimeError"] {
        assert!(
            labels.iter().any(|l| l == label),
            "no corpus program records a {label}; the corpus holds {labels:?}",
        );
    }
}

/// And not allowed to become a corpus of failures either: a program that is
/// worth nothing cannot tell a correct value from a wrong one.
#[test]
fn the_corpus_holds_programs_that_are_worth_something() {
    let mut valued = 0usize;
    let mut distinct: Vec<String> = Vec::new();
    for path in corpus::corpus() {
        let expected = corpus::expectation(&path).unwrap_or_else(|e| panic!("{e}"));
        if expected.must_fail() || expected.value == Typed::Nothing {
            continue;
        }
        valued += 1;
        distinct.push(expected.value.tagged());
        let outcome = vm::bytecode(&source_of(&path));
        assert_eq!(
            outcome.result,
            Ok(expected.value.clone()),
            "{}: the bytecode VM is worth something the corpus does not record",
            path.display(),
        );
    }
    distinct.sort();
    distinct.dedup();
    assert!(
        valued >= 20,
        "only {valued} corpus programs are worth something",
    );
    assert!(
        distinct.len() >= 10,
        "the corpus records {} distinct values, not 10: {distinct:?}",
        distinct.len(),
    );
}

/// The corpus is a function of the generator and nothing else.
///
/// Regeneration runs into a scratch directory and is compared **byte for byte**
/// against `corpus/`, so the claim needs no environment variable to be checked —
/// and nothing here writes `corpus/`, because a test that refreshes the goldens
/// it is checking cannot fail on a regression in them.
#[test]
fn edge_regenerating_the_corpus_reproduces_the_checked_in_one_byte_for_byte() {
    let sources = generator::corpus_sources();

    let regenerated = scratch("differential-regenerated");
    corpus::write_into(&regenerated, &sources).expect("scratch corpus should be writable");

    if writing() {
        let refreshed = scratch("differential-refreshed");
        corpus::write_into(&refreshed, &sources).expect("the refreshed copy should be writable");
        eprintln!(
            "the generator's corpus is at {}; `corpus/` was not touched, and the \
             comparison below still fails on any difference",
            refreshed.display()
        );
    }

    // The two counts are the generator's and the directory's, so this can fail:
    // a program the generator produces and nobody checked in is a hole in the
    // corpus, and the loop below never reads a checked-in `.rb` to notice.
    let checked_in = corpus::corpus();
    assert_eq!(
        checked_in.len(),
        sources.len(),
        "the checked-in corpus holds {} programs and the generator produces {}",
        checked_in.len(),
        sources.len(),
    );

    let mut differences = Vec::new();
    for (name, source) in &sources {
        let fresh_path = regenerated.join(format!("{name}.rb"));
        let fresh_source = fs::read_to_string(&fresh_path).expect("regenerated source");
        if &fresh_source != source {
            differences.push(format!("{name}.rb: the writer changed the source"));
        }
        let fresh_expected = fs::read_to_string(regenerated.join(format!("{name}.expected")))
            .expect("regenerated expectation");
        let expected_path = corpus::corpus_dir().join(format!("{name}.expected"));
        let checked_in_expected = fs::read_to_string(&expected_path).unwrap_or_else(|e| {
            differences.push(format!("{}: {e}", expected_path.display()));
            String::new()
        });
        if fresh_expected != checked_in_expected {
            differences.push(format!(
                "{name}.expected differs:\n--- regenerated ---\n{fresh_expected}\
                 --- checked in ---\n{checked_in_expected}"
            ));
        }
    }
    let _ = fs::remove_dir_all(&regenerated);
    assert!(
        differences.is_empty(),
        "{} file(s) of the checked-in corpus are not what the generator produces:\n{}",
        differences.len(),
        differences.join("\n"),
    );
}

/// The other direction: the directory on disk holds exactly the programs the
/// generator produces, by name as well as by content.
#[test]
fn the_checked_in_corpus_is_the_one_the_generator_produces() {
    let generated: Vec<String> = generator::corpus_sources()
        .into_iter()
        .map(|(name, _)| format!("{name}.rb"))
        .collect();
    let on_disk: Vec<String> = corpus::corpus()
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string))
        .collect();

    let missing: Vec<&String> = generated.iter().filter(|n| !on_disk.contains(n)).collect();
    let extra: Vec<&String> = on_disk.iter().filter(|n| !generated.contains(n)).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "the corpus and the generator disagree: missing {missing:?}, unexpected {extra:?}",
    );

    for (name, source) in generator::corpus_sources() {
        let path = corpus::corpus_dir().join(format!("{name}.rb"));
        assert_eq!(
            source_of(&path),
            source,
            "{}: the file on disk is not the program's source",
            path.display(),
        );
    }
}

// --- the property tests ------------------------------------------------------

#[test]
fn the_two_vms_agree_on_every_generated_program() {
    let mut divergences = Vec::new();
    for seed in 0..SEEDS {
        let generated = generator::generate(seed);
        let tree = vm::tree_walk(&generated.source);
        let byte = vm::bytecode(&generated.source);
        if !tree.agrees_within_format_limits(&byte) {
            let minimal = shrink::shrinks_to(&generated.source, |candidate| {
                !vm::tree_walk(candidate).agrees_within_format_limits(&vm::bytecode(candidate))
            });
            divergences.push(match minimal {
                Some(minimal) => shrink::report(seed, &generated.source, &minimal),
                None => format!("seed {seed}: {tree:?} vs {byte:?}"),
            });
        }
    }
    assert!(
        divergences.is_empty(),
        "{} generated program(s) diverge:\n{}",
        divergences.len(),
        divergences.join("\n---\n"),
    );
}

/// The typed-grammar invariant: `FAULTS` is the *only* source of failure.
///
/// A program drawn with no fault must run to completion, and a program drawn
/// with one must fail on **that** fault and no other. It is worth having because
/// it finds things: it caught three generator defects, the first of which was a
/// `text` arm that summed a number into a text.
///
/// Both engines are asked, per seed, because the invariant is about the *program*
/// and not about one interpreter: the tree-walking VM alone would leave the
/// bytecode one unheld to it, and a fault the compiler lowers into something else
/// — or loses — is exactly the shape of bug the compiler can have.
#[test]
fn edge_a_generated_program_only_faults_on_a_fault_that_was_injected() {
    let mut no_fault_faulted = Vec::new();
    let mut wrong_fault = Vec::new();
    for seed in 0..SEEDS {
        let generated = generator::generate(seed);
        for (engine, outcome) in [
            ("tree-walking VM", vm::tree_walk(&generated.source)),
            ("bytecode VM", vm::bytecode(&generated.source)),
        ] {
            match (generated.fault, &outcome.result) {
                (Fault::None, Ok(_)) => {}
                (Fault::None, Err(failure)) => no_fault_faulted.push(format!(
                    "{engine}, seed {seed}: no fault injected, yet {}\n{}",
                    vm::show(failure),
                    generated.source
                )),
                (fault, Ok(_)) => wrong_fault.push(format!(
                    "{engine}, seed {seed}: {} injected, yet it ran to completion\n{}",
                    fault.name(),
                    generated.source
                )),
                (fault, Err(failure)) => {
                    let text = vm::show(failure);
                    let expected = fault
                        .message()
                        .unwrap_or_else(|| panic!("{fault:?} declares no message"));
                    assert!(
                        text.contains(expected),
                        "{engine}, seed {seed}: {} injected, but the failure is {text:?}, \
                         which does not name {expected:?}\n{}",
                        fault.name(),
                        generated.source,
                    );
                }
            }
        }
    }
    assert!(
        no_fault_faulted.is_empty(),
        "{} generated program run(s) failed with no fault injected:\n{}",
        no_fault_faulted.len(),
        no_fault_faulted.join("\n---\n"),
    );
    assert!(
        wrong_fault.is_empty(),
        "{} generated program run(s) ran despite an injected fault:\n{}",
        wrong_fault.len(),
        wrong_fault.join("\n---\n"),
    );
}

/// The differential test's value half: the two engines are asked what a program
/// is **worth**, not only what it printed.
#[test]
fn edge_the_generated_corpus_compares_a_program_s_value_not_only_its_output() {
    let mut completions = 0usize;
    let mut valued = 0usize;
    let mut distinct: Vec<String> = Vec::new();
    for seed in 0..SEEDS {
        let generated = generator::generate(seed);
        let tree = vm::tree_walk(&generated.source);
        let byte = vm::bytecode(&generated.source);
        if let Ok(value) = &tree.result {
            completions += 1;
            if *value != Typed::Nothing {
                valued += 1;
                distinct.push(value.tagged());
            }
        }
        assert!(
            tree.agrees_within_format_limits(&byte),
            "seed {seed} is worth two things",
        );
    }
    distinct.sort();
    distinct.dedup();
    assert!(
        completions > 0 && valued * 2 >= completions,
        "only {valued} of {completions} completed generated programs are worth \
         something: a corpus of `nothing` cannot see a wrong value",
    );
    assert!(
        distinct.len() >= 5,
        "the generated corpus records {} distinct values: {distinct:?}",
        distinct.len(),
    );
}

/// A seed is only actionable if the program behind it can be read.
#[test]
fn edge_a_generated_program_can_be_seen() {
    let generated = generator::generate(DEFAULT_SEED);
    assert!(
        generated
            .source
            .contains(&format!("// generated: seed {}", DEFAULT_SEED)),
        "a generated program must name the seed that drew it:\n{}",
        generated.source,
    );
    assert!(
        generated
            .source
            .contains(&format!("// fault: {}", generated.fault.name())),
        "a generated program must name the fault it carries:\n{}",
        generated.source,
    );
    assert!(
        generated.source.ends_with('\n'),
        "a source without a trailing newline is not a source a text editor writes",
    );
}

#[test]
fn a_generated_program_runs_the_same_way_every_time() {
    for seed in 0..32u64 {
        let generated = generator::generate(seed);
        let first = vm::tree_walk(&generated.source);
        let second = vm::tree_walk(&generated.source);
        assert_eq!(first, second, "seed {seed} is not deterministic");
        assert!(
            first.agrees_within_format_limits(&vm::bytecode(&generated.source)),
            "seed {seed} diverges",
        );
    }
}

/// Both channels must occur, and a failure must name a public label, a message
/// and a position.
#[test]
fn a_generated_program_either_completes_or_reports_a_failure() {
    let labels = [
        "LexerError",
        "ParserError",
        "AnalyzerError",
        "RuntimeError",
        "IoError",
    ];
    let mut completed = 0usize;
    let mut failed = 0usize;
    for seed in 0..SEEDS {
        let generated = generator::generate(seed);
        let outcome = vm::tree_walk(&generated.source);
        match &outcome.result {
            Ok(_) => completed += 1,
            Err(failure) => {
                failed += 1;
                let text = vm::show(failure);
                assert!(
                    labels.iter().any(|l| text.starts_with(l)),
                    "seed {seed}: {text:?} does not name a public error label",
                );
                assert!(
                    !failure.message().is_empty(),
                    "seed {seed}: the failure has an empty message",
                );
            }
        }
        let byte = vm::bytecode(&generated.source);
        assert!(
            outcome.agrees_within_format_limits(&byte),
            "seed {seed}: the two engines said different things: {outcome:?} and {byte:?}",
        );
    }
    assert!(
        completed > 0 && failed > 0,
        "a generated corpus must exercise both channels: {completed} completed, \
         {failed} failed",
    );
}

// --- positions ---------------------------------------------------------------

#[test]
fn edge_every_recorded_failure_records_the_place_it_happened_at() {
    let mut failures = 0usize;
    let mut off_column_one = 0usize;
    for path in corpus::corpus() {
        let expected = corpus::expectation(&path).unwrap_or_else(|e| panic!("{e}"));
        let (Some((line, column)), true) = (expected.position, expected.must_fail()) else {
            continue;
        };
        failures += 1;
        assert!(
            line > 0 && column > 0,
            "{}: {line}:{column}",
            path.display()
        );
        if column != 1 {
            off_column_one += 1;
        }
    }
    assert!(
        failures >= 40,
        "only {failures} recorded failures carry a position"
    );
    assert!(
        off_column_one > 0,
        "every recorded failure is at column 1, so the column half is arithmetic \
         rather than a check",
    );
}

/// A recorded position must be a real position in the program it belongs to.
///
/// A parser failure at the end of a source is legitimately reported one line
/// *past* the last: that is where the token it wanted would have been. The
/// convention is counted rather than forbidden, and held to under a quarter.
#[test]
fn edge_every_recorded_failure_points_at_a_line_of_its_own_program() {
    let mut failures = 0usize;
    let mut at_end_of_input = 0usize;
    let mut offenders = Vec::new();
    for path in corpus::corpus() {
        let expected = corpus::expectation(&path).unwrap_or_else(|e| panic!("{e}"));
        let Some((line, _)) = expected.position else {
            continue;
        };
        if !expected.must_fail() {
            continue;
        }
        failures += 1;
        let source = source_of(&path);
        let lines = source.lines().count();
        if line > lines + 1 {
            offenders.push(format!("{}: line {line} of {lines}", path.display()));
            continue;
        }
        if line == lines + 1 {
            at_end_of_input += 1;
            continue;
        }
        // A column may be one past the last character: a caret is drawn *after*
        // the token that was wanted, so the end of a line is a position too.
        let text = source.lines().nth(line - 1).unwrap_or_default();
        let characters = text.chars().count();
        let column = expected.position.expect("checked above").1;
        if column > characters + 1 {
            offenders.push(format!(
                "{}: column {column} of a line {} characters wide",
                path.display(),
                characters
            ));
        }
    }
    assert!(
        offenders.is_empty(),
        "{} recorded position(s) are not in their own program:\n{}",
        offenders.len(),
        offenders.join("\n"),
    );
    assert!(failures >= 40, "only {failures} recorded failures");
    assert!(
        at_end_of_input * 4 < failures,
        "{} of {failures} recorded failures sit past the end of their program; \
         the end-of-input convention is meant to be rare, not the rule",
        at_end_of_input,
    );
}

#[test]
fn edge_both_vms_place_every_failure_in_the_same_place() {
    let mut compared = 0usize;
    let mut differences = Vec::new();
    for path in corpus::corpus() {
        let source = source_of(&path);
        let tree = vm::tree_walk(&source);
        let byte = vm::bytecode(&source);
        let (Err(a), Err(b)) = (&tree.result, &byte.result) else {
            continue;
        };
        compared += 1;
        if !vm::same_place(a, b) {
            differences.push(format!(
                "{}: tree-walking VM says {}, bytecode VM says {}",
                path.display(),
                a.position()
                    .map(|(l, c)| format!("{l}:{c}"))
                    .unwrap_or_else(|| "nothing".to_string()),
                b.position()
                    .map(|(l, c)| format!("{l}:{c}"))
                    .unwrap_or_else(|| "nothing".to_string()),
            ));
        }
    }
    assert!(compared >= 40, "only {compared} failures were compared");
    assert!(
        differences.is_empty(),
        "{} failure(s) are placed differently by the two VMs:\n{}",
        differences.len(),
        differences.join("\n"),
    );
}

/// The limit on comparing a column is **per failure**, and this is what it says.
///
/// A `.rbc` instruction carries a line and no column, so the bytecode VM places
/// a runtime failure at the start of the line its instruction is on, while the
/// tree-walking VM places it at the column of the failing statement. The limit is
/// recorded in each failure rather than inferred from its label, and the engine
/// that cannot name a column is **held to the convention** instead of excused
/// from comparison — which is what a blanket "a `RuntimeError`'s column does not
/// count" rule could not do. The counts are here so the rule cannot be satisfied
/// by a corpus that has stopped exercising either side of it.
#[test]
fn edge_a_column_is_compared_exactly_wherever_both_engines_can_name_one() {
    let mut frontend = 0usize;
    let mut runtime = 0usize;
    let mut off_column_one = 0usize;
    for path in corpus::corpus() {
        let source = source_of(&path);
        let tree = vm::tree_walk(&source);
        let byte = vm::bytecode(&source);
        let (Err(a), Err(b)) = (&tree.result, &byte.result) else {
            continue;
        };
        // The tree-walking VM reads the source, so every failure it reports can
        // name a column. If that ever stops being true the gate above is
        // comparing less than it says it compares.
        assert!(
            a.column_exact(),
            "{}: the tree-walking VM has the source in hand",
            path.display(),
        );
        if b.column_exact() {
            frontend += 1;
        } else {
            runtime += 1;
            // The line-granular convention, asserted over the whole corpus: a
            // runtime failure on the bytecode VM is at the start of its line or
            // it is somewhere the format never put it.
            assert_eq!(
                b.position().map(|(_, column)| column),
                Some(1),
                "{}: a runtime failure the bytecode VM cannot place by column is \
                 reported at the start of its line",
                path.display(),
            );
            // And the limit is a check rather than an exemption: a column that
            // moved on the engine that cannot name one is a divergence, asked of
            // this corpus and not of a fixture.
            assert!(
                !vm::same_place(a, &b.with_column(2)),
                "{}: a moved column on the bytecode VM is invisible to the \
                 comparison, so the format limit has become a blanket exemption",
                path.display(),
            );
        }
        if a.position().map(|(_, column)| column) != Some(1) {
            off_column_one += 1;
        }
    }
    assert!(
        frontend >= 20 && runtime >= 20,
        "the corpus holds {frontend} failures both engines place by column and \
         {runtime} only one can: both sides of the rule have to be exercised",
    );
    assert!(
        off_column_one > 0,
        "every failure the two engines agree on is at column 1, so the column \
         half of the comparison is arithmetic rather than a check",
    );
}

/// The `else if` the phase's summary claimed does not exist.
///
/// `SPEC.md:353` and `docs/GRAMMAR.md:206` both write `else if` as part of an
/// `if`, and **neither engine parses it**: every corpus program that writes one
/// is recorded as a `ParserError`, so the claimed `if/else if/else` coverage was
/// `if/else` with three refusals counted as successes. Rather than claim a form
/// the corpus does not hold, this measures both halves — the two-arm `else` that
/// works, and the `else if` that does not, pinned on both engines — so a day when
/// `else if` parses, this test says so instead of the summary quietly becoming
/// true.
#[test]
fn edge_the_control_flow_family_covers_the_if_else_it_has_and_pins_the_else_if_it_does_not() {
    let mut with_else = 0usize;
    let mut with_else_if = Vec::new();
    for path in corpus::corpus() {
        let source = source_of(&path);
        let expected = corpus::expectation(&path).unwrap_or_else(|e| panic!("{e}"));
        if source.contains("else if") {
            with_else_if.push(path.clone());
            assert_eq!(
                expected.label,
                "ParserError",
                "{}: `else if` parses now, so its golden and the coverage claim \
                 both need revisiting rather than a passing test",
                path.display(),
            );
            // Both engines: a refusal is a property of the language, not of one
            // of them.
            for (engine, outcome) in [
                ("tree-walking VM", vm::tree_walk(&source)),
                ("bytecode VM", vm::bytecode(&source)),
            ] {
                assert_eq!(
                    outcome.label(),
                    Some("ParserError"),
                    "{engine}: {} does not refuse an `else if`",
                    path.display(),
                );
            }
        } else if !expected.must_fail() && source.contains("else\n") {
            with_else += 1;
        }
    }
    assert!(
        with_else_if.len() >= 3,
        "only {} corpus program(s) write an `else if`; the refusal is the only \
         coverage there is of it",
        with_else_if.len(),
    );
    assert!(
        with_else >= 5,
        "only {with_else} corpus programs run an `if`/`else` that takes its \
         `else` branch; the two-arm form is the one the family really covers",
    );
}

/// `break` and `skip` are accepted, and neither does anything.
///
/// `FINDINGS.md` §5: both engines parse the keywords and neither acts on them, so
/// `corpus/loop-forms-0012` and `-0013` record a **completion** whose output is
/// the no-op's. A golden that records a defect as a success is read as coverage
/// by whoever reads it next, so these programs are named in
/// [`corpus::KNOWN_DEFECT_PROGRAMS`] and this test asserts the defect on both
/// engines: the keyword is in the program, the loop runs to its end anyway, and
/// both engines say the same thing. The day either engine implements `break`,
/// this fails and the two goldens are expected to change — which is the whole
/// reason the no-op is pinned rather than ignored.
#[test]
fn edge_a_break_and_a_skip_are_pinned_as_the_defect_they_are() {
    let mut checked = 0usize;
    for (name, why) in corpus::KNOWN_DEFECT_PROGRAMS {
        let path = corpus::corpus_dir().join(format!("{name}.rb"));
        assert!(
            path.exists(),
            "{name} is named as a known defect and is not in the corpus: {why}",
        );
        let source = source_of(&path);
        let expected = corpus::expectation(&path).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            source.contains("break") || source.contains("skip"),
            "{name} is named as a `break`/`skip` defect and holds neither keyword",
        );
        assert!(
            !expected.must_fail(),
            "{name}: the defect is that the keyword is *accepted*, so this program \
             completes. If it fails now, the defect is fixed and this table, the \
             goldens and FINDINGS.md §5 all need revisiting",
        );
        for (engine, outcome) in [
            ("tree-walking VM", vm::tree_walk(&source)),
            ("bytecode VM", vm::bytecode(&source)),
        ] {
            assert_eq!(
                outcome.result,
                Ok(Typed::Nothing),
                "{engine}: {name} is the recorded no-op, not a new behaviour",
            );
            assert_eq!(
                outcome.output, expected.output,
                "{engine}: {name} runs to the end of the loop, which is the defect",
            );
        }
        checked += 1;
    }
    assert!(
        checked >= 2,
        "only {checked} known-defect programs are pinned; a table that shrinks is \
         a defect that stopped being checked",
    );
}

/// A program that prints before it fails must be recorded *and* compared.
///
/// The rule that survives is narrower than "a failure prints nothing": a
/// **frontend** failure prints nothing, because nothing ran.
#[test]
fn edge_a_program_that_prints_before_it_faults_is_recorded_not_asserted_away() {
    let mut printed_then_failed = 0usize;
    let mut frontend_silent = 0usize;
    for path in corpus::corpus() {
        let expected = corpus::expectation(&path).unwrap_or_else(|e| panic!("{e}"));
        if !expected.must_fail() {
            continue;
        }
        let frontend = expected.label != "RuntimeError";
        if !expected.output.is_empty() {
            assert!(
                !frontend,
                "{}: a {label} printed {:?}",
                path.display(),
                expected.output,
                label = expected.label,
            );
            printed_then_failed += 1;
        }
        if frontend {
            frontend_silent += 1;
        }
    }
    assert!(
        printed_then_failed >= 10,
        "only {printed_then_failed} corpus programs print before they fail",
    );
    assert!(
        frontend_silent >= 10,
        "only {frontend_silent} corpus programs fail in the frontend",
    );
}

/// A program that never ends must be stopped by the interpreter's own budget
/// rather than hanging the suite.
///
/// The assertion is deliberately weak: *that* it returns, and that what it
/// returns is a clean error. The message and the limit are the interpreter's to
/// choose and this phase did not choose them.
#[test]
fn edge_a_program_that_never_ends_is_stopped_rather_than_hanging() {
    let source = "set i to 0\nwhile i < 1\n    say i\nend\n";
    let tree = vm::tree_walk(source);
    let byte = vm::bytecode(source);
    for (engine, outcome) in [("tree-walking VM", &tree), ("bytecode VM", &byte)] {
        let failure = outcome
            .result
            .as_ref()
            .err()
            .unwrap_or_else(|| panic!("{engine}: an endless loop must not run to completion"));
        assert!(
            failure.message().contains("step") || failure.message().contains("iteration"),
            "{engine}: the guard must say what it stopped for, not just that it \
             stopped: {}",
            failure.message(),
        );
    }
}

/// A call stack that cannot come back is a clean failure with a position, on
/// both engines, and not a stack overflow.
#[test]
fn edge_a_call_stack_that_cannot_come_back_is_a_clean_failure() {
    let source = "to f(n)\n    give back f(n + 1)\nend\nsay f(0)\n";
    for outcome in [vm::tree_walk(source), vm::bytecode(source)] {
        let failure = outcome.result.as_ref().expect_err("must be stopped");
        assert_eq!(
            failure.message(),
            "Maximum call depth of 1000 reached while calling 'f'"
        );
        assert!(
            failure.position().is_some(),
            "a failure with no position cannot be acted on",
        );
        assert!(
            outcome.output.is_empty(),
            "an unbounded recursion says nothing before it fails: {:?}",
            outcome.output,
        );
    }
}

/// A deeply nested value is data, not a stack: seven levels of list and record
/// have to read back on both engines.
#[test]
fn edge_a_deeply_nested_value_is_read_back_on_both_engines() {
    let source = "set r to {a: {b: {c: {d: {e: {f: {g: 1}}}}}}}\nsay r.a.b.c.d.e.f.g\n";
    for outcome in [vm::tree_walk(source), vm::bytecode(source)] {
        assert_eq!(outcome.output, vec!["1".to_string()], "{outcome:?}");
        assert_eq!(outcome.result, Ok(Typed::Nothing), "{outcome:?}");
    }
}

// --- the runner's own failure paths -----------------------------------------

#[test]
fn edge_a_corpus_program_whose_expected_file_names_a_failure_reports_a_mismatch() {
    // An `.expected` file that says a program succeeded while the program fails
    // is the mismatch this runner exists to catch. Asserted on a fixture rather
    // than on the corpus, so that the check on the real corpus stays a check on
    // the interpreter.
    let text = "#label RuntimeError\n#position 2:1\n#value nothing\n\
                #message Index 9 is out of bounds: length is 2, valid indexes are 0 to 1\n";
    let expected = corpus::parse(text).expect("the fixture is well formed");
    assert_eq!(expected.label, "RuntimeError");
    assert!(expected.must_fail());
    assert!(expected.output.is_empty());

    let source = "set xs to [1, 2]\nsay xs[9]\n";
    let outcome = vm::tree_walk(source);
    let path = Path::new("corpus/edge_out_of_bounds.rb");
    corpus::compare(path, &outcome, &expected).expect("the recorded failure is the real one");

    let wrong = corpus::compare(
        path,
        &outcome,
        &Expected::of(&Outcome {
            output: outcome.output.clone(),
            result: Ok(Typed::Nothing),
        }),
    );
    assert!(
        wrong.is_err(),
        "an .expected that says the program ran must not accept a failure",
    );
}

#[test]
fn edge_a_missing_expected_output_file_is_a_runner_failure() {
    let dir = scratch("differential-runner");
    let orphan = dir.join("orphan.rb");
    fs::write(&orphan, "say 1\n").expect("scratch program should be writable");

    assert!(
        !orphan.with_extension("expected").exists(),
        "the fixture is only a fixture while its .expected file is absent",
    );

    let outcome = vm::tree_walk(&source_of(&orphan));
    assert_eq!(
        outcome.output,
        vec!["1".to_string()],
        "the program itself runs; what must fail is the missing expectation",
    );

    let reported = corpus::expectation_of(&orphan).expect_err("an orphan must be refused");
    assert!(
        reported.contains("orphan.rb") && reported.contains("no readable .expected file"),
        "{reported}",
    );

    let invented: Vec<String> = fs::read_dir(&dir)
        .expect("scratch dir should be readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".expected"))
        .collect();
    assert!(
        invented.is_empty(),
        "the runner must not invent an expectation: found {invented:?}",
    );
}

#[test]
fn edge_an_expected_file_naming_the_wrong_label_is_a_comparison_failure() {
    let source = "set xs to [1]\nsay xs[4]\n";
    let outcome = vm::tree_walk(source);
    let path = Path::new("corpus/edge_label.rb");
    let mut recorded = Expected::of(&outcome);
    assert!(corpus::compare(path, &outcome, &recorded).is_ok());
    recorded.label = "ParserError".to_string();
    let wrong = corpus::compare(path, &outcome, &recorded).expect_err("a changed label");
    assert!(wrong.contains("recorded a ParserError"), "{wrong}");

    let mut ran = Expected::of(&outcome);
    ran.label = "none".to_string();
    ran.message = String::new();
    ran.position = None;
    let wrong = corpus::compare(path, &outcome, &ran).expect_err("a recorded completion");
    assert!(wrong.contains("recorded a completion"), "{wrong}");
}

#[test]
fn edge_a_mismatch_names_the_program_it_is_about() {
    let path = Path::new("corpus/objects-0001.rb");
    let outcome = Outcome {
        output: vec!["1".to_string()],
        result: Ok(Typed::Nothing),
    };
    let expected = Expected::of(&outcome);
    let mut recorded = expected.clone();
    recorded.output = vec!["2".to_string()];
    let e = corpus::compare(path, &outcome, &recorded).expect_err("a changed output");
    assert!(e.contains("objects-0001.rb"), "{e}");

    let mut valued = expected.clone();
    valued.value = Typed::Number(1.0);
    let e = corpus::compare(path, &outcome, &valued).expect_err("a changed value");
    assert!(e.contains("objects-0001.rb"), "{e}");
    assert!(e.contains("number:1"), "{e}");
}

#[test]
fn edge_a_shrunk_program_still_diverges() {
    // The shrinker must not reduce a program that does not fail.
    assert_eq!(
        shrink::shrinks_to("say 1\nsay 2\n", |candidate| {
            !vm::tree_walk(candidate).agrees_within_format_limits(&vm::bytecode(candidate))
        }),
        None,
    );
}
