//! The phase's own scanner is evidence, so it has to be evidence that can be
//! checked — the number in `REPORT.md` is only worth reading if the scanner
//! that produced it can be run again and watched to make the same judgement.
//!
//! # Why this file exists
//!
//! `phases/phase-088/scan.rs` is a lot of new code that the phase put at the
//! centre of its evidence, and **nothing in the repository ran it**. The only
//! thing that depended on it was a number in a report, and a number that
//! nothing checks can drift: the scanner could have been wrong about every
//! test in the suite and the report would have carried on saying
//! "vacuous tests: 0".
//!
//! Worse, the scanner's own judgement was **unsound in the direction that
//! matters**. It counted a test as asserting something if its body merely
//! *mentioned* `take_output` / `.output` / `stdout` / `expect_err`, and it
//! counted any `assert_eq!` in the body as an assertion, without checking that
//! the value read was compared to anything. A test that reads the program's
//! output and never asserts on it therefore scored as non-vacuous — and
//! reading `take_output()` is exactly the idiom this phase introduced in
//! every new test, so the scanner could not see the defect it was written to
//! find.
//!
//! # What these tests pin
//!
//! 1. **A read that is never compared is vacuous.** The fixture holds one
//!    test that reads the program's output and asserts nothing about it, and
//!    one that compares it; the scanner must name exactly the first.
//! 2. **A comparison of something the program never said is vacuous too** —
//!    the other half of the same rule, and the one the old scanner got wrong
//!    in the other direction.
//! 3. **A comparison through a helper is not.** The helper step is what keeps
//!    the scanner from reading five real tests as defects.
//! 4. **The scanner follows `tests/common/`.** Its docs say the `tests/common/`
//!    helpers are read; this checks that they are, rather than taking the
//!    docs' word for it.
//! 5. **A refused program is not a reachability check.** `assert!(!result.is_ok())`
//!    says the program was *refused*, which is a claim about what it
//!    answered; the scanner must not read it as "the program ran" and name
//!    the test vacuous.
//! 6. **This tree has no reachability-only test left.** The scan over the
//!    repository's own `tests/` directory must answer `vacuous tests: 0` —
//!    the claim `REPORT.md` makes, checked by running the scanner rather than
//!    by believing the report.
//!
//! # What the scanner needs — nothing
//!
//! The scanner is **this repository's own Rust**: the file below is included
//! into this test binary, so the rules the tests check are the rules the
//! published scanner implements. There is no interpreter to install, no
//! subprocess to spawn, and no second toolchain to keep alive: a checkout
//! without Python runs the whole suite, and the phase's evidence is checked on
//! every machine rather than only on the ones that happen to have a script
//! interpreter.
//!
//! The scanner is still a program an auditor can run on any tree:
//!
//! ```text
//! rustc --edition 2021 phases/phase-088/scan.rs -o target/tmp/rbscan
//! target/tmp/rbscan /tmp/base/tests
//! ```
//!
//! # The fixtures and the repository's own test count
//!
//! The fixtures below are Rust source the scanner reads, and they carry a
//! `#[test]` attribute each. They are written with a `@TEST@` placeholder
//! instead, and the placeholder is substituted when the fixture is written
//! out, so that the repository's own count of `#[test]` functions — the number
//! `REPORT.md` reconciles its test count against — counts real tests and not
//! the text of a fixture.

#[path = "../phases/phase-088/scan.rs"]
mod scan;

use std::fs;
use std::path::PathBuf;

/// The attribute a fixture test carries, written so that the repository's own
/// count of `#[test]` functions does not count the fixture.
const TEST_ATTRIBUTE: &str = "@TEST@";

/// The file list the scan read, published so the next audit can check it.
const SCANNED_FILES: &str = "phases/phase-088/scanned-files.txt";

/// The repository root, so the scanner is run against this checkout and not
/// against whatever directory the test happens to run in.
fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The names the scanner gave, without the line numbers it prints with them.
fn names(result: &scan::Scan) -> Vec<String> {
    result
        .named
        .iter()
        .map(|(_, _, name)| name.clone())
        .collect()
}

/// A directory of its own for one fixture, removed when the test is done.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    /// A fresh directory under the system's temporary directory.
    fn new(name: &str) -> Fixture {
        let root = std::env::temp_dir().join(format!(
            "redblue-phase-088-scanner-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root)
            .unwrap_or_else(|error| panic!("could not create {}: {error}", root.display()));
        Fixture { root }
    }

    /// Write a file into the fixture.
    fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("could not create {}: {error}", parent.display()));
        }
        let contents = contents.replace(TEST_ATTRIBUTE, "#[test]");
        fs::write(&path, contents)
            .unwrap_or_else(|error| panic!("could not write {}: {error}", path.display()));
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// The fixture the reviewer's finding names: one test that reads the program's
/// output and never asserts on it, and one that compares it.
const READS_WITHOUT_ASSERTING: &str = r#"
@TEST@
fn edge_a_test_that_reads_the_output_and_never_asserts_on_it() {
    let (mut vm, outcome) = run_isolated(&program);
    outcome.expect("the fixture runs");
    let lines = vm.take_output();
    let _ = lines;
}

@TEST@
fn edge_a_test_that_compares_the_output() {
    let (mut vm, outcome) = run_isolated(&program);
    outcome.expect("the fixture runs");
    let lines = vm.take_output();
    assert_eq!(lines, vec!["a line".to_string()]);
}
"#;

/// A test that reads the output and asserts something the program never said.
const ASSERTS_SOMETHING_ELSE: &str = r#"
@TEST@
fn edge_a_test_that_asserts_something_the_program_never_said() {
    let (mut vm, outcome) = run_isolated(&program);
    outcome.expect("the fixture runs");
    let lines = vm.take_output();
    let _ = lines;
    assert_eq!(counter, 3);
}
"#;

/// A test that compares the output through a helper.
const COMPARES_THROUGH_A_HELPER: &str = r#"
fn printed(source: &str) -> Vec<String> {
    let (mut vm, outcome) = run_isolated(&program);
    outcome.expect("the fixture runs");
    vm.take_output()
}

@TEST@
fn edge_a_test_that_compares_the_output_through_a_helper() {
    let lines = printed(source);
    assert_eq!(lines, vec!["a line".to_string()]);
}
"#;

/// A test that asserts the value the run handed back.
const COMPARES_THE_VALUE: &str = r#"
@TEST@
fn edge_a_test_that_asserts_the_value_the_run_handed_back() {
    let (mut vm, outcome) = run_isolated(&program);
    assert_eq!(
        outcome.expect("the fixture runs"),
        Value::Number(10.0),
        "the program's value is the number, not the text that renders as `10`"
    );
}
"#;

/// A refused program, and a program that ran — the two the scanner has to tell
/// apart.
///
/// `assert!(result.is_ok())` says "the program ran" and nothing more, which is
/// the shape this scan exists to find. `assert!(!result.is_ok())` says the
/// program was **refused**, which is a claim about what it answered: a program
/// that answered wrongly instead of refusing would fail that assertion. The
/// scanner used to read the negated form as a reachability check too, and
/// called the test vacuous.
const REFUSAL_IS_NOT_REACHABILITY: &str = r#"
@TEST@
fn edge_a_program_that_was_refused_is_not_a_reachability_check() {
    let result = run_source(src);
    assert!(!result.is_ok(), "the program must be refused, not answered");
}

@TEST@
fn edge_a_program_that_ran_is_a_reachability_check() {
    let result = run_source(src);
    assert!(result.is_ok(), "the program must run");
}
"#;

/// A read that is never compared is vacuous, and a comparison is not.
///
/// This is the finding itself, as a test. The old scanner called **both** of
/// these non-vacuous — the first because its body mentions `take_output`, the
/// second because its body contains an `assert_eq!` — and neither of those
/// mentions has anything to do with whether the program was right.
#[test]
fn edge_the_scanner_names_a_read_that_is_never_compared() {
    let fixture = Fixture::new("read-without-asserting");
    fixture.write("fixture_test.rs", READS_WITHOUT_ASSERTING);

    let result = scan::scan_directory(&fixture.root);

    assert_eq!(
        result.count(),
        1,
        "exactly one of the two fixture tests is vacuous, and it is the one \
         that reads the output and asserts nothing:\n{:?}",
        result.named
    );
    assert_eq!(
        names(&result),
        vec!["edge_a_test_that_reads_the_output_and_never_asserts_on_it".to_string()],
        "the scanner must name the test that reads the output and never compares it, \
         and must not name the one that compares it:\n{:?}",
        result.named
    );
}

/// The other half of the same rule: a comparison of something the program
/// never said is not an assertion about the program.
///
/// The old scanner called this non-vacuous too, because the body contains an
/// `assert_eq!` — and the `assert_eq!` compares a counter the program never
/// touched.
#[test]
fn edge_the_scanner_names_a_comparison_of_something_else() {
    let fixture = Fixture::new("asserts-something-else");
    fixture.write("fixture_test.rs", ASSERTS_SOMETHING_ELSE);

    let result = scan::scan_directory(&fixture.root);

    assert_eq!(
        names(&result),
        vec!["edge_a_test_that_asserts_something_the_program_never_said".to_string()],
        "a comparison the program never took part in is not an assertion about it:\n{:?}",
        result.named
    );
}

/// A comparison through a helper is a comparison, and the scanner follows the
/// helper to find it.
///
/// The helper step is what keeps the scanner from reading the suite's real
/// tests as defects: five of them assert through a helper, and a scanner that
/// cannot see through one reports them as vacuous.
#[test]
fn edge_the_scanner_follows_a_helper_to_the_comparison() {
    let fixture = Fixture::new("helper");
    fixture.write("fixture_test.rs", COMPARES_THROUGH_A_HELPER);

    let result = scan::scan_directory(&fixture.root);

    assert_eq!(
        result.count(),
        0,
        "a test that compares the output through a helper asserts the output:\n{:?}",
        result.named
    );
}

/// The value the run handed back is the program's answer too.
///
/// `say` renders a value and the run hands one back, and a test that pins the
/// value has pinned the program's answer even when the program printed
/// nothing at all.
#[test]
fn edge_the_scanner_sees_the_value_the_run_handed_back() {
    let fixture = Fixture::new("value");
    fixture.write("fixture_test.rs", COMPARES_THE_VALUE);

    let result = scan::scan_directory(&fixture.root);

    assert_eq!(
        result.count(),
        0,
        "a test that asserts the value the run handed back asserts the program's answer:\n{:?}",
        result.named
    );
}

/// The scanner follows the `tests/common/` helpers.
///
/// Its docs say they are read; this checks that they are, rather than taking
/// the docs' word for it. A test whose comparison lives in `tests/common/` is
/// not a defect, and a scanner that cannot see the helper would report it as
/// one.
#[test]
fn edge_the_scanner_follows_the_common_helpers() {
    let fixture = Fixture::new("common");
    fixture.write("common/mod.rs", "pub mod helper;\n");
    fixture.write(
        "common/helper.rs",
        r#"
pub fn printed(source: &str) -> Vec<String> {
    let (mut vm, outcome) = run_isolated(&program);
    outcome.expect("the fixture runs");
    vm.take_output()
}
"#,
    );
    fixture.write(
        "fixture_test.rs",
        r#"
mod common;

use common::helper;

@TEST@
fn edge_a_test_that_compares_the_output_through_a_common_helper() {
    let lines = helper::printed(source);
    assert_eq!(lines, vec!["a line".to_string()]);
}
"#,
    );

    let result = scan::scan_directory(&fixture.root);

    assert_eq!(
        result.count(),
        0,
        "the scanner must follow a `tests/common/` helper to the comparison \
         inside it:\n{:?}",
        result.named
    );
}

/// A refusal is not a reachability check.
///
/// `assert!(!result.is_ok())` asserts that the program was **refused**. That
/// is a claim about what the program answered — a program that answered
/// wrongly instead of refusing would fail the assertion — and the scanner's
/// own rules say so: "a test that asserts a *failure* was produced is never
/// mistaken for a vacuous one".
///
/// The rule used to be written as "an `assert!` whose condition contains
/// `is_ok()` and none of `is_err()`, `==`, `!=`, `contains`, `matches`", which
/// read the **negated** form as a reachability check as well, and so named a
/// test that asserts a refusal as vacuous.
#[test]
fn edge_the_scanner_does_not_read_a_refusal_as_a_reachability_check() {
    let fixture = Fixture::new("refusal");
    fixture.write("fixture_test.rs", REFUSAL_IS_NOT_REACHABILITY);

    let result = scan::scan_directory(&fixture.root);

    assert_eq!(
        names(&result),
        vec!["edge_a_program_that_ran_is_a_reachability_check".to_string()],
        "the scanner must name the test that only asks whether the program ran, \
         and must not name the one that asserts the program was refused:\n{:?}",
        result.named
    );
}

/// This tree has no reachability-only test left.
///
/// The claim `REPORT.md` makes, checked by running the scanner rather than by
/// believing the report: the scan over this repository's own `tests/`
/// directory must answer `vacuous tests: 0`.
#[test]
fn phase_088_has_no_reachability_only_test_left() {
    let root = project_root().join("tests");
    let result = scan::scan_directory(&root);

    assert_eq!(
        result.count(),
        0,
        "every test in this tree that runs a program must compare what the \
         program answered; the scanner named these:\n{:?}",
        result.named
    );
}

/// The count the scanner prints is the count it named.
///
/// The report quotes the scanner's own output, so the two have to agree: a
/// scanner that printed a count its own lines do not add up to would make
/// every quoted number a guess. The command line is driven here too, because
/// that is the shape an auditor runs.
#[test]
fn edge_the_scanner_prints_the_count_it_named() {
    let fixture = Fixture::new("printed-count");
    fixture.write("fixture_test.rs", READS_WITHOUT_ASSERTING);

    let result = scan::scan_directory(&fixture.root);
    let printed = result.render(&fixture.root);

    assert!(
        printed.contains("scanned 1 files in "),
        "the scanner must say how many files it read:\n{printed}"
    );
    assert!(
        printed.contains("vacuous tests: 1"),
        "the scanner must print the count of the tests it named:\n{printed}"
    );
    assert_eq!(
        printed.lines().filter(|line| line.contains('\t')).count(),
        result.count(),
        "the printed count must be the number of lines the scanner named:\n{printed}"
    );

    let from_the_command_line = scan::run(&[fixture.root.display().to_string()])
        .unwrap_or_else(|error| panic!("the scanner's command line must run: {error}"));
    assert_eq!(
        from_the_command_line, printed,
        "the command line must print what the scan rendered"
    );
    let listed = scan::run(&["--list".to_string(), fixture.root.display().to_string()])
        .unwrap_or_else(|error| panic!("the scanner's command line must run: {error}"));
    assert_eq!(
        listed, "fixture_test.rs",
        "the file list must name the files the scan read"
    );
}

/// The file list the scan read is published, and it is current.
///
/// `phases/phase-088/scanned-files.txt` is the list the scan read, published
/// so that the next audit can see which files the number covers. A file
/// listed there that no longer exists would make the list a claim about a
/// tree that is not this one.
#[test]
fn edge_the_published_file_list_names_files_that_still_exist() {
    let published = fs::read_to_string(project_root().join(SCANNED_FILES))
        .unwrap_or_else(|error| panic!("{SCANNED_FILES} must be readable: {error}"));
    let tests = project_root().join("tests");

    for name in published
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        assert!(
            tests.join(name).is_file(),
            "the published file list names {name}, which is not a file in tests/ any more"
        );
    }
}
