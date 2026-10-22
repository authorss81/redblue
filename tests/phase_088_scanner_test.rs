//! The phase's own scanner is evidence, so it has to be evidence that can be
//! checked — the number in `REPORT.md` is only worth reading if the scanner
//! that produced it can be run again and watched to make the same judgement.
//!
//! # Why this file exists
//!
//! `phases/phase-088/scan.py` is hundreds of lines of new code that the phase
//! put at the centre of its evidence, and **nothing in the repository ran it**.
//! The only thing that depended on it was a number in a report, and a number
//! that nothing checks can drift: the scanner could have been wrong about
//! every test in the suite and the report would have carried on saying
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
//! 4. **The scanner follows `tests/common/`.** Its docstring says the
//!    `tests/common/` helpers are read; this checks that they are, rather than
//!    taking the docstring's word for it.
//! 5. **This tree has no reachability-only test left.** The scan over the
//!    repository's own `tests/` directory must answer `vacuous tests: 0` —
//!    the claim `REPORT.md` makes, checked by running the scanner rather than
//!    by believing the report.
//!
//! # What the scanner needs
//!
//! Python 3. The scanner is a Python program and these tests run it, so a
//! checkout without `python3` cannot verify this phase's evidence; the tests
//! say so rather than passing quietly.
//!
//! # The fixtures and the repository's own test count
//!
//! The fixtures below are Rust source the scanner reads, and they carry a
//! `#[test]` attribute each. They are written with a `@TEST@` placeholder
//! instead, and the placeholder is substituted when the fixture is written
//! out, so that the repository's own count of `#[test]` functions — the number
//! `REPORT.md` reconciles its test count against — counts real tests and not
//! the text of a fixture.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The scanner this phase published.
const SCANNER: &str = "phases/phase-088/scan.py";

/// The attribute a fixture test carries, written so that the repository's own
/// count of `#[test]` functions does not count the fixture.
const TEST_ATTRIBUTE: &str = "@TEST@";

/// The repository root, so the scanner is run against this checkout and not
/// against whatever directory the test happens to run in.
fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The python this checkout can run.
fn python() -> String {
    for candidate in ["python3", "python"] {
        let works = Command::new(candidate)
            .arg("-c")
            .arg("print(1)")
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false);
        if works {
            return candidate.to_string();
        }
    }
    panic!(
        "the phase's scanner is a Python program, so verifying this phase's \
         evidence needs python3 on PATH; neither `python3` nor `python` runs here"
    );
}

/// What the scanner said: the vacuous tests it named, and the count it printed.
#[derive(Debug, PartialEq, Eq)]
struct Scan {
    /// The tests it named, as `file:line\tname`.
    named: Vec<String>,
    /// The count on its `vacuous tests:` line.
    count: usize,
}

/// Run the scanner over `root` and read what it said.
fn scan(root: &Path) -> Scan {
    let output = Command::new(python())
        .arg(project_root().join(SCANNER))
        .arg(root)
        .output()
        .unwrap_or_else(|error| panic!("could not run the scanner: {error}"));
    assert!(
        output.status.success(),
        "the scanner must run cleanly over {}; it said: {}",
        root.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

    let mut named = Vec::new();
    let mut count = None;
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("vacuous tests:") {
            count =
                Some(rest.trim().parse::<usize>().unwrap_or_else(|_| {
                    panic!("the scanner printed an unreadable count: {line:?}")
                }));
            continue;
        }
        if line.starts_with("scanned ") || line.trim().is_empty() {
            continue;
        }
        named.push(line.to_string());
    }
    let count = count.unwrap_or_else(|| {
        panic!(
            "the scanner printed no count over {}:\n{stdout}",
            root.display()
        )
    });
    assert_eq!(
        named.len(),
        count,
        "the scanner's count must match the lines it named:\n{stdout}"
    );
    Scan { named, count }
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

/// The names the scanner gave, without the line numbers it prints with them.
fn names(scan: &Scan) -> Vec<String> {
    scan.named
        .iter()
        .map(|entry| entry.split('\t').nth(1).unwrap_or(entry).to_string())
        .collect()
}

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

    let scan = scan(&fixture.root);

    assert_eq!(
        scan.count, 1,
        "exactly one of the two fixture tests is vacuous, and it is the one \
         that reads the output and asserts nothing:\n{:?}",
        scan.named
    );
    assert_eq!(
        names(&scan),
        vec!["edge_a_test_that_reads_the_output_and_never_asserts_on_it".to_string()],
        "the scanner must name the test that reads the output and never compares it, \
         and must not name the one that compares it:\n{:?}",
        scan.named
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

    let scan = scan(&fixture.root);

    assert_eq!(
        names(&scan),
        vec!["edge_a_test_that_asserts_something_the_program_never_said".to_string()],
        "a comparison the program never took part in is not an assertion about it:\n{:?}",
        scan.named
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

    let scan = scan(&fixture.root);

    assert_eq!(
        scan.count, 0,
        "a test that compares the output through a helper asserts the output:\n{:?}",
        scan.named
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

    let scan = scan(&fixture.root);

    assert_eq!(
        scan.count, 0,
        "a test that asserts the value the run handed back asserts the program's answer:\n{:?}",
        scan.named
    );
}

/// The scanner follows the `tests/common/` helpers.
///
/// Its docstring says they are read; this checks that they are, rather than
/// taking the docstring's word for it. A test whose comparison lives in
/// `tests/common/` is not a defect, and a scanner that cannot see the helper
/// would report it as one.
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

    let scan = scan(&fixture.root);

    assert_eq!(
        scan.count, 0,
        "the scanner must follow a `tests/common/` helper to the comparison \
         inside it:\n{:?}",
        scan.named
    );
}

/// This tree has no reachability-only test left.
///
/// The claim `REPORT.md` makes, checked by running the scanner rather than by
/// believing the report: the scan over this repository's own `tests/`
/// directory must answer `vacuous tests: 0`.
#[test]
fn phase_088_has_no_reachability_only_test_left() {
    let scan = scan(&project_root().join("tests"));

    assert_eq!(
        scan.count, 0,
        "every test in this tree that runs a program must compare what the \
         program answered; the scanner named these:\n{:?}",
        scan.named
    );
}
