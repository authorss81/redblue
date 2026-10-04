use std::fs;
use std::path::{Path, PathBuf};

use redblue::testing::{find_test_files, run_all_tests, TestHarness};

/// Scratch directory inside the project's own `target/tmp`, never the system
/// temp dir, so nothing outside the checkout is touched. Removed and recreated
/// on every call so each test starts from a known, empty state.
fn scratch_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/tmp/rb-discovery-test")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir should be creatable");
    dir
}

fn write_file(dir: &Path, name: &str, contents: &[u8]) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents).expect("scratch file should be writable");
    path
}

/// The Redblue harness is a Redblue *parser*. Feeding it Rust source is never
/// correct, so discovery must drop every `.rs` path.
#[test]
fn edge_discovery_never_returns_rust_files() {
    let dir = scratch_dir("rust-filter");
    write_file(&dir, "red_suite.rb", b"// test \"noop\"\n// end\n");
    write_file(&dir, "redblue_test.rs", b"#[test]\nfn x() {}\n");
    write_file(&dir, "expect_test.rs", b"#[test]\nfn y() {}\n");
    write_file(&dir, "notes.txt", b"not a test at all\n");
    write_file(&dir, "Makefile", b"all:\n");

    let found = find_test_files(dir.to_str().expect("utf-8 scratch path"))
        .expect("discovery should succeed");

    assert!(
        found.iter().all(|path| path.ends_with(".rb")),
        "discovery handed Rust source to the Redblue harness: {:?}",
        found
    );
    assert_eq!(
        found.len(),
        1,
        "expected only the single .rb file, got {:?}",
        found
    );
}

/// Singleton / boundary: exactly one collectable file, named correctly.
#[test]
fn edge_discovery_collects_a_single_rb_file() {
    let dir = scratch_dir("singleton");
    write_file(&dir, "only.rb", b"// test \"only\"\n// end\n");
    write_file(&dir, "only_test.rs", b"#[test]\nfn only() {}\n");

    let found = find_test_files(dir.to_str().expect("utf-8 scratch path"))
        .expect("discovery should succeed");

    assert_eq!(found.len(), 1, "expected exactly one file, got {:?}", found);
    assert!(
        found[0].ends_with("only.rb"),
        "expected only.rb to be collected, got {:?}",
        found[0]
    );
}

/// Nesting / recursion: `.rb` files are found at any depth, `.rs` files never.
#[test]
fn find_test_files_walks_nested_directories() {
    let dir = scratch_dir("nested");
    let deep = dir.join("a/b/c");
    fs::create_dir_all(&deep).expect("nested scratch dirs should be creatable");
    write_file(&dir, "top.rb", b"// test \"top\"\n// end\n");
    write_file(&dir, "top_test.rs", b"#[test]\nfn top() {}\n");
    write_file(&deep, "deep.rb", b"// test \"deep\"\n// end\n");
    write_file(&deep, "deep_test.rs", b"#[test]\nfn deep() {}\n");

    let found = find_test_files(dir.to_str().expect("utf-8 scratch path"))
        .expect("discovery should succeed");

    assert_eq!(found.len(), 2, "expected two .rb files, got {:?}", found);
    assert!(
        found.iter().any(|p| p.ends_with("top.rb")),
        "top level .rb missing from {:?}",
        found
    );
    assert!(
        found.iter().any(|p| p.ends_with("deep.rb")),
        "nested .rb missing from {:?}",
        found
    );
    assert!(
        found.iter().all(|p| !p.ends_with(".rs")),
        "nested Rust source leaked into discovery: {:?}",
        found
    );
}

/// Empty / resource: a directory with nothing to collect yields no files and
/// no error; a directory that does not exist is not an error either.
#[test]
fn edge_discovery_of_empty_and_missing_directory_is_empty() {
    let empty = scratch_dir("empty");
    let found = find_test_files(empty.to_str().expect("utf-8 scratch path"))
        .expect("empty directory should not error");
    assert!(found.is_empty(), "empty directory produced {:?}", found);

    let missing = empty.join("does/not/exist");
    let found = find_test_files(missing.to_str().expect("utf-8 scratch path"))
        .expect("missing directory should not error");
    assert!(found.is_empty(), "missing directory produced {:?}", found);
}

/// Determinism: `read_dir` yields entries in filesystem hash order, so
/// discovery must impose its own ordering or the test report changes between
/// machines. Repeated calls must also be byte-for-byte identical.
#[test]
fn edge_discovery_order_is_sorted_and_stable_across_calls() {
    let dir = scratch_dir("order");
    // Written in reverse-alphabetical creation order so that filesystem order
    // cannot accidentally look sorted.
    for name in ["h", "g", "f", "e", "d", "c", "b", "a"] {
        write_file(
            &dir,
            &format!("{}.rb", name),
            b"// test \"ordering\"\n// end\n",
        );
    }

    let first = find_test_files(dir.to_str().expect("utf-8 scratch path"))
        .expect("discovery should succeed");
    let second = find_test_files(dir.to_str().expect("utf-8 scratch path"))
        .expect("second discovery should succeed");

    assert_eq!(
        first, second,
        "discovery order must not vary between calls on the same directory"
    );

    let mut expected = first.clone();
    expected.sort();
    assert_eq!(
        first, expected,
        "discovery must return paths in sorted order, got {:?}",
        first
    );
}

/// The sort must hold across directory nesting too, not just within one level.
#[test]
fn edge_nested_discovery_is_globally_sorted() {
    let dir = scratch_dir("order-nested");
    let deep = dir.join("inner");
    fs::create_dir_all(&deep).expect("nested scratch dir should be creatable");
    write_file(&dir, "b.rb", b"// test \"b\"\n// end\n");
    write_file(&dir, "a.rb", b"// test \"a\"\n// end\n");
    write_file(&deep, "z.rb", b"// test \"z\"\n// end\n");
    write_file(&deep, "y.rb", b"// test \"y\"\n// end\n");

    let found = find_test_files(dir.to_str().expect("utf-8 scratch path"))
        .expect("discovery should succeed");
    assert_eq!(
        found.len(),
        4,
        "expected four .rb files across both levels, got {:?}",
        found
    );

    let mut expected = found.clone();
    expected.sort();
    assert_eq!(
        found, expected,
        "nested discovery must return paths in global sorted order, got {:?}",
        found
    );
}

/// Definition of done, against the real tree: none of the `tests/*_test.rs`
/// Rust sources that `cargo test` owns may ever appear in the list the
/// Redblue harness is handed.
#[test]
fn edge_repository_rust_suite_is_never_handed_to_the_harness() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let found = find_test_files(root.join("tests").to_str().expect("utf-8 path"))
        .expect("discovery should succeed");

    let rust_leaked: Vec<&String> = found.iter().filter(|p| p.ends_with(".rs")).collect();
    assert!(
        rust_leaked.is_empty(),
        "the Redblue harness was handed Rust sources: {:?}",
        rust_leaked
    );

    let known_rust = [
        "discovery_test.rs",
        "expect_test.rs",
        "redblue_test.rs",
        "span_test.rs",
    ];
    for name in known_rust {
        let tracked = root.join("tests").join(name);
        assert!(
            tracked.is_file(),
            "guard assumes {} exists; update the list if tests/ changed",
            tracked.display()
        );
        assert!(
            !found.iter().any(|p| p.ends_with(name)),
            "{} is a cargo test file and must never be discovered by the \
             Redblue harness, got {:?}",
            name,
            found
        );
    }

    assert!(
        !found.is_empty(),
        "no .rb files discovered under the real tests/ directory"
    );
}

/// Malformed input: a `.rb` file that is not valid UTF-8 is still collected,
/// and reading it surfaces a clean `Error::Io` instead of a panic.
#[test]
fn edge_non_utf8_rb_file_is_reported_as_an_io_error() {
    let dir = scratch_dir("non-utf8");
    let path = write_file(&dir, "bad.rb", &[0xff, 0xfe, 0x00, b'/']);

    let found = find_test_files(dir.to_str().expect("utf-8 scratch path"))
        .expect("discovery should succeed");
    assert_eq!(found.len(), 1, "expected the .rb file, got {:?}", found);

    let mut harness = TestHarness::new();
    let err = harness
        .run_file(path.to_str().expect("utf-8 scratch path"))
        .expect_err("non-UTF-8 .rb file must fail to read");
    let message = err.to_string();
    assert!(
        message.contains("bad.rb"),
        "error should name the offending file, got: {}",
        message
    );
}

/// The filter must not suppress genuine Redblue failures: a `.rb` test whose
/// body does not parse is still reported as a failure.
#[test]
fn edge_malformed_rb_test_is_reported_as_a_failure() {
    let mut harness = TestHarness::new();
    harness
        .run_source("// test \"broken\"\nset to to to\n// end\n")
        .expect("harness source scan should not itself fail");

    let results = harness.results();
    assert_eq!(
        results.total, 1,
        "expected one discovered test: {:?}",
        results
    );
    assert_eq!(
        results.failed, 1,
        "malformed .rb test body must be a failure, got {:?}",
        results.errors
    );
}

/// End-to-end definition of done: the project's own `tests/` directory, which
/// is mostly Rust source, must produce zero failures.
#[test]
fn edge_project_tests_directory_has_no_failures() {
    let results = run_all_tests().expect("run_all_tests should succeed");
    assert_eq!(
        results.failed, 0,
        "`rb test` reported failures for the Rust suite: {:?}",
        results.errors
    );
    assert!(
        results.total > 0,
        "`rb test` discovered no Redblue tests at all"
    );
}

/// Malformed input (CRLF): a Windows-line-ending `.rb` file must still be
/// discovered *and* its `// test` block actually executed, not merely found.
#[test]
fn edge_crlf_rb_file_is_discovered_and_executed() {
    let dir = scratch_dir("crlf");
    write_file(
        &dir,
        "windows.rb",
        b"// test \"crlf\"\r\nset x to 1\r\nexpect x to be 1\r\n// end\r\n",
    );

    let found = find_test_files(dir.to_str().expect("utf-8 scratch path"))
        .expect("discovery should succeed");
    assert_eq!(found.len(), 1, "expected the .rb file, got {:?}", found);

    let mut harness = TestHarness::new();
    harness
        .run_file(&found[0])
        .expect("harness should read the CRLF file");
    let results = harness.results();
    assert_eq!(
        results.total, 1,
        "CRLF `// test` marker was not recognised: {:?}",
        results
    );
    assert_eq!(
        results.failed, 0,
        "CRLF test body failed: {:?}",
        results.errors
    );
}

/// Unicode / escapes: emoji, CJK and combining marks survive discovery and
/// the harness, and a wrong value in that text is still a real failure.
#[test]
fn edge_unicode_rb_test_is_discovered_and_asserted() {
    let dir = scratch_dir("unicode");
    write_file(
        &dir,
        "unicode.rb",
        "// test \"unicode\"\nset s to \"h\u{e9}llo \u{1f389} \u{4e16}\u{754c}\"\nexpect s to be \"h\u{e9}llo \u{1f389} \u{4e16}\u{754c}\"\n// end\n"
            .as_bytes(),
    );

    let found = find_test_files(dir.to_str().expect("utf-8 scratch path"))
        .expect("discovery should succeed");
    assert_eq!(found.len(), 1, "expected the .rb file, got {:?}", found);

    let mut harness = TestHarness::new();
    harness.run_file(&found[0]).expect("harness reads the file");
    let results = harness.results();
    assert_eq!(
        (results.total, results.passed, results.failed),
        (1, 1, 0),
        "unicode test did not pass cleanly: {:?}",
        results.errors
    );

    // Same file, wrong expectation: the unicode text must not be silently
    // accepted because of an encoding slip.
    write_file(
        &dir,
        "unicode_bad.rb",
        "// test \"unicode bad\"\nset s to \"\u{4e16}\u{754c}\"\nexpect s to be \"\u{4e16}\"\n// end\n"
            .as_bytes(),
    );
    let found = find_test_files(dir.to_str().expect("utf-8 scratch path"))
        .expect("discovery should succeed");
    let mut harness = TestHarness::new();
    harness
        .run_file(
            found
                .iter()
                .find(|p| p.ends_with("unicode_bad.rb"))
                .expect("bad file collected"),
        )
        .expect("harness reads the file");
    assert_eq!(
        harness.results().failed,
        1,
        "mismatched unicode expectation must fail"
    );
}

/// Resource / state: a `.rb` path containing spaces is a legal path and must
/// be discovered and run.
#[test]
fn edge_rb_path_with_spaces_is_discovered_and_run() {
    let dir = scratch_dir("spaces");
    let nested = dir.join("my test dir");
    fs::create_dir_all(&nested).expect("spaced scratch dir should be creatable");
    write_file(
        &nested,
        "a suite with spaces.rb",
        b"// test \"spaced\"\nexpect 1 to be 1\n// end\n",
    );

    let found = find_test_files(dir.to_str().expect("utf-8 scratch path"))
        .expect("discovery should succeed");
    assert_eq!(found.len(), 1, "expected the .rb file, got {:?}", found);

    let mut harness = TestHarness::new();
    harness
        .run_file(&found[0])
        .expect("harness reads the spaced path");
    let results = harness.results();
    assert_eq!(
        (results.total, results.passed, results.failed),
        (1, 1, 0),
        "spaced-path test did not pass cleanly: {:?}",
        results.errors
    );
}
