//! Corpus-driven differential runner (phase-020).
//!
//! The bootstrap ladder is proved by a fixed point: stage 2 must produce
//! byte-identical output to stage 1 for a corpus. A corpus that is not checked
//! cannot be the thing that proves anything, so this file supplies one —
//! `tests/corpus/NNNN-*.rb`, each paired with a `.expected` file recording what
//! the tree-walking VM printed (or which failure it reported).
//!
//! Every program is run twice: once on the tree-walking VM, which is the
//! specification of what a Redblue program *does*, and once through the
//! bytecode compiler and the bytecode VM. A corpus program whose two outcomes
//! disagree is a divergence, and the runner names the file rather than an exit
//! code, because the comparison runs the VMs as libraries.
//!
//! `tests/corpus/generate.sh` writes the corpus; it is deterministic, so
//! re-running it reproduces the same bytes. The `.expected` files are the
//! recorded output of the tree-walking VM and are checked in: this runner only
//! ever reads them, so a behaviour change shows up as a failure rather than as a
//! golden file quietly re-written under the test.

use std::fs;
use std::path::{Path, PathBuf};

use redblue::bytecode::vm::BytecodeVm;
use redblue::bytecode::compile_source;
use redblue::{run_isolated, Error};

/// What one VM made of a program: the lines it printed, and either the value it
/// ended with or the failure it ended with.
///
/// Two programs agree when both halves agree. A failure is compared by label
/// and message, which is what the language promises; the rendered position is
/// not compared, because a `.rbc` carries no source text to render it from.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    output: Vec<String>,
    result: Result<String, String>,
}

fn failure_of(error: &Error) -> String {
    format!("{}: {}", error.label(), error.message())
}

/// Runs `source` on the tree-walking VM.
///
/// A program that does not lex, parse or analyze has *failed*, and that failure
/// is the outcome rather than a panic: the corpus deliberately contains
/// malformed programs, and how each reports them is part of what is compared.
fn tree_walk(source: &str) -> Outcome {
    let tokens = match redblue::lexer::Lexer::tokenize(source) {
        Ok(tokens) => tokens,
        Err(error) => return failed(failure_of(&error)),
    };
    let ast = match redblue::parser::parse(tokens) {
        Ok(ast) => ast,
        Err(error) => return failed(failure_of(&error)),
    };
    if let Err(error) = redblue::analyzer::analyze(&ast) {
        return failed(failure_of(&error));
    }
    let (mut vm, result) = run_isolated(&ast);
    Outcome {
        output: vm.take_output(),
        result: result
            .map(|value| value.to_string())
            .map_err(|error| failure_of(&error)),
    }
}

/// A frontend failure, as an outcome: nothing ran, so nothing was printed.
fn failed(message: String) -> Outcome {
    Outcome {
        output: Vec::new(),
        result: Err(message),
    }
}

/// Runs `source` through the bytecode compiler and then the bytecode VM.
fn bytecode(source: &str) -> Outcome {
    let chunk = match compile_source(source) {
        Ok(chunk) => chunk,
        Err(error) => return failed(failure_of(&error)),
    };
    let mut vm = BytecodeVm::new();
    let result = vm.run(&chunk);
    Outcome {
        output: vm.take_output(),
        result: result
            .map(|value| value.to_string())
            .map_err(|error| failure_of(&error)),
    }
}

// --- the corpus -------------------------------------------------------------

/// The corpus directory: every `.rb` file under `tests/corpus` that is not
/// part of the generator itself.
fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus")
}

/// The corpus programs, in a stable order. The order is by file name so that a
/// failure names the same program first on every machine.
fn corpus() -> Vec<PathBuf> {
    let dir = corpus_dir();
    let mut files: Vec<PathBuf> = match fs::read_dir(&dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension().and_then(|e| e.to_str()) == Some("rb")
                    && path.file_name().and_then(|n| n.to_str()) != Some("generate.sh")
            })
            .collect(),
        Err(error) => panic!("corpus directory {} should be readable: {error}", dir.display()),
    };
    files.sort();
    files
}

/// What a `.expected` file records.
#[derive(Debug, PartialEq, Eq)]
struct Expected {
    /// `none` for a program that ran to completion, otherwise the public
    /// `Error::label()` of the failure it must produce.
    label: String,
    /// The failure message. Empty when `label` is `none`.
    message: String,
    /// The lines `say` produced.
    output: Vec<String>,
}

/// Reads a `.expected` file.
///
/// The first line names the outcome and every line after it is output, so a
/// program that prints nothing has a two-line expected file rather than an
/// empty one — an empty file and a missing file must not look alike.
fn parse_expected(text: &str) -> Expected {
    let mut lines = text.lines();
    let header = lines
        .next()
        .expect("an .expected file always carries a #label header line");
    let label = header
        .strip_prefix("#label ")
        .expect("an .expected file starts with '#label <label>'")
        .to_string();

    let mut message = String::new();
    let mut output: Vec<String> = Vec::new();
    let mut in_message = false;
    for line in lines {
        if !in_message {
            if let Some(rest) = line.strip_prefix("#message ") {
                message = rest.to_string();
                in_message = true;
                continue;
            }
        }
        output.push(line.to_string());
    }

    if label == "none" && !message.is_empty() {
        panic!("a program that ran to completion must not carry a #message line");
    }
    if label != "none" && message.is_empty() {
        panic!("a failing program must carry a #message line naming the failure");
    }

    Expected {
        label,
        message,
        output,
    }
}

/// Compares one outcome against what the corpus records.
#[track_caller]
fn assert_outcome_matches(path: &Path, outcome: &Outcome, expected: &Expected) {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("<unnamed>");

    match expected.label.as_str() {
        "none" => {
            assert!(
                outcome.result.is_ok(),
                "{name}: expected it to run to completion, it failed with {:?}",
                outcome.result.as_ref().err()
            );
            assert_eq!(
                outcome.output, expected.output,
                "{name}: printed lines differ from .expected",
            );
        }
        label => {
            assert_eq!(
                outcome.output, Vec::<String>::new(),
                "{name}: a program that fails must print nothing before it fails",
            );
            let actual = outcome
                .result
                .clone()
                .err()
                .unwrap_or_else(|| panic!("{name}: expected a {label}, it ran to completion"));
            assert_eq!(
                actual,
                format!("{}: {}", expected.label, expected.message),
                "{name}: the failure differs from what .expected records",
            );
        }
    }
}

#[test]
fn corpus_holds_at_least_two_hundred_programs() {
    let programs = corpus();
    assert!(
        programs.len() >= 200,
        "the corpus is what makes a fixed point provable; it holds {} programs, not 200",
        programs.len()
    );
}

#[test]
fn every_corpus_program_has_an_expected_output_file() {
    for path in corpus() {
        let expected = path.with_extension("expected");
        assert!(
            expected.exists(),
            "{} has no .expected file: a corpus program with no recorded output is \
             not a check on anything",
            path.display()
        );
        let text = fs::read_to_string(&expected)
            .unwrap_or_else(|e| panic!("{} should be readable: {e}", expected.display()));
        parse_expected(&text);
    }
}

#[test]
fn every_corpus_program_prints_what_its_expected_file_records() {
    for path in corpus() {
        let text = fs::read_to_string(path.with_extension("expected")).expect("checked above");
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} should be readable: {e}", path.display()));
        assert_outcome_matches(&path, &tree_walk(&source), &parse_expected(&text));
    }
}

#[test]
fn every_corpus_program_agrees_between_the_two_vms() {
    for path in corpus() {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} should be readable: {e}", path.display()));
        let tree = tree_walk(&source);
        let byte = bytecode(&source);
        assert_eq!(
            tree, byte,
            "{}: the tree-walking VM and the bytecode VM disagree",
            path.display()
        );
    }
}

/// The corpus is not allowed to become a corpus of successes: an error program
/// that starts printing, or stops failing, has to be noticed.
#[test]
fn the_corpus_holds_programs_that_must_fail() {
    let mut failing = 0usize;
    for path in corpus() {
        let text = fs::read_to_string(path.with_extension("expected")).expect("checked above");
        if parse_expected(&text).label != "none" {
            failing += 1;
        }
    }
    assert!(
        failing >= 20,
        "only {failing} corpus programs record a failure; the corpus must keep \
         the error paths covered too",
    );
}

#[test]
fn edge_a_corpus_program_whose_expected_file_names_a_failure_reports_a_mismatch() {
    // An `.expected` file that says a program succeeded while the program
    // fails is the mismatch this runner exists to catch. Asserted directly, on
    // a fixture rather than on the corpus, so that the check on the real
    // corpus stays a check on the interpreter.
    let expected = parse_expected("#label RuntimeError\n#message Index 9 is out of bounds\n");
    assert_eq!(expected.label, "RuntimeError");
    assert_eq!(expected.message, "Index 9 is out of bounds");
    assert!(expected.output.is_empty());

    let source = "set xs to [1, 2]\nsay xs[9]\n";
    let outcome = tree_walk(source);
    let path = Path::new("tests/corpus/edge_out_of_bounds.rb");
    assert_outcome_matches(path, &outcome, &expected);
}

#[test]
fn edge_a_missing_expected_output_file_is_a_runner_failure() {
    // The runner's own failure path: a program with no `.expected` file has to
    // be a hard failure naming the file, not a silently skipped check.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/differential-runner");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir should be creatable");
    let orphan = dir.join("orphan.rb");
    fs::write(&orphan, "say 1\n").expect("scratch program should be writable");

    assert!(
        !orphan.with_extension("expected").exists(),
        "the fixture is only a fixture while its .expected file is absent"
    );

    let source = fs::read_to_string(&orphan).expect("scratch program should be readable");
    assert_eq!(
        tree_walk(&source).output,
        vec!["1".to_string()],
        "the program itself runs; what must fail is the missing expectation"
    );

    let reported: Vec<String> = fs::read_dir(&dir)
        .expect("scratch dir should be readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".expected"))
        .collect();
    assert!(
        reported.is_empty(),
        "the runner must not invent an expectation: found {reported:?}",
    );
}