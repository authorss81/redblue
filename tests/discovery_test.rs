use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use redblue::manifest;
use redblue::project::find_project_root;
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

fn is_rb(path: &Path) -> bool {
    path.extension() == Some("rb".as_ref())
}

fn is_rs(path: &Path) -> bool {
    path.extension() == Some("rs".as_ref())
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
        found.iter().all(|path| is_rb(path)),
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
        found.iter().all(|p| !is_rs(p)),
        "nested Rust source leaked into discovery: {:?}",
        found
    );
}

/// Resource / recursion: `Path::is_dir` follows symlinks, so a link such as
/// `loop -> .` must not make discovery recurse until the stack overflows.
/// Directory symlinks are skipped; real files are still collected.
#[cfg(unix)]
#[test]
fn edge_discovery_does_not_follow_symlinked_directories() {
    use std::os::unix::fs::symlink;

    let dir = scratch_dir("symlink-cycle");
    write_file(&dir, "only.rb", b"// test \"only\"\n// end\n");
    symlink(&dir, dir.join("loop")).expect("directory symlink should be creatable");

    let found = find_test_files(&dir).expect("a symlink cycle must not break discovery");
    assert_eq!(found.len(), 1, "symlink cycle produced {:?}", found);
    assert!(
        found[0].ends_with("only.rb"),
        "expected only.rb to be collected, got {:?}",
        found
    );
}

/// Traversal: a symlinked `.rb` file is still a link, not a regular file.
/// Following it would run a suite from outside the directory discovery was
/// pointed at, which is the same escape a directory symlink would allow.
#[cfg(unix)]
#[test]
fn edge_discovery_does_not_collect_symlinked_rb_files() {
    use std::os::unix::fs::symlink;

    let outside = scratch_dir("symlink-file-outside");
    let outside_file = write_file(
        &outside,
        "escape.rb",
        b"// test \"outside\"\nexpect 1 to be 1\n// end\n",
    );

    let dir = scratch_dir("symlink-file");
    write_file(&dir, "inside.rb", b"// test \"inside\"\n// end\n");
    symlink(&outside_file, dir.join("escape.rb")).expect("file symlink should be creatable");

    let found = find_test_files(&dir).expect("a file symlink must not break discovery");
    assert_eq!(found.len(), 1, "file symlink produced {:?}", found);
    assert!(
        found[0].ends_with("inside.rb"),
        "expected only inside.rb to be collected, got {:?}",
        found
    );

    let results = redblue::testing::run_tests_in(&dir).expect("the suite should run");
    assert_eq!(
        results.total, 1,
        "a symlinked .rb file was executed from outside the suite: {:?}",
        results.errors
    );
}

/// Resource / recursion: a real directory tree deep enough to overflow the
/// stack is refused with a reported error rather than a crash. A symlink cycle
/// is not the only unbounded shape discovery has to survive.
#[test]
fn edge_discovery_refuses_a_too_deep_directory_tree() {
    let dir = scratch_dir("too-deep");
    let mut deep = dir.clone();
    for _ in 0..100 {
        deep.push("d");
    }
    fs::create_dir_all(&deep).expect("a deep but real directory tree should be creatable");

    let error = find_test_files(&dir).expect_err("an unbounded tree must be refused");
    let message = error.to_string();
    assert!(
        message.contains("levels deep"),
        "the refusal should explain the depth bound, got: {message}"
    );
}

/// Empty / library contract: a directory with nothing to collect yields no
/// files and no error; a directory that does not exist is not an error either.
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

/// The library runner keeps its previous contract for an empty suite: a
/// zero-count result, not an error. The command layer owns the rejection.
#[test]
fn edge_run_tests_in_empty_directory_is_a_zero_count_result() {
    let empty = scratch_dir("empty-suite");

    let results =
        redblue::testing::run_tests_in(&empty).expect("an empty suite is not a library error");

    assert_eq!(
        (results.total, results.passed, results.failed),
        (0, 0, 0),
        "an empty suite should report zero tests, got {:?}",
        results
    );
}

/// A directory with no manifest is not a project, so `check_manifest` refuses
/// it instead of reporting a silent success for a caller that never checked.
#[test]
fn edge_check_manifest_refuses_a_directory_without_a_manifest() {
    let dir = scratch_dir("missing-manifest");

    let error = redblue::project::check_manifest(&dir)
        .expect_err("a directory without a manifest is not a project");

    assert!(
        error.to_string().contains("redblue.manifest"),
        "the refusal should name the missing manifest, got: {error}"
    );
}

/// Unreadable / contract: a regular file is not a directory that can be
/// searched. `read_dir` answers a non-`NotFound` failure, which discovery must
/// surface rather than reporting an empty suite.
#[test]
fn edge_discovery_unreadable_directory_is_an_error() {
    let dir = scratch_dir("not-a-directory");
    let file = write_file(&dir, "not-a-directory.rb", b"// test \"noop\"\n// end\n");

    let error = find_test_files(&file).expect_err("a regular file is not a readable directory");
    let message = error.to_string();
    assert!(
        message.contains("Cannot read test directory"),
        "error should say the test directory could not be read, got: {message}"
    );
    assert!(
        message.contains("not-a-directory.rb"),
        "error should name the offending path, got: {message}"
    );
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

    let rust_leaked: Vec<&PathBuf> = found.iter().filter(|p| is_rs(p)).collect();
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

/// A project suite that cannot read one of its `.rb` files must name that file
/// in the failure, not report the anonymous `unknown` location.
#[test]
fn edge_run_tests_in_names_an_unreadable_file() {
    let dir = scratch_dir("unreadable-named");
    write_file(&dir, "bad.rb", &[0xff, 0xfe, 0x00, b'/']);

    let results = redblue::testing::run_tests_in(&dir).expect("the suite should run");
    assert_eq!(
        results.failed, 1,
        "an unreadable .rb file must be a failure: {:?}",
        results.errors
    );
    let error = &results.errors[0];
    assert!(
        error.file.ends_with("bad.rb"),
        "the failure should name the unreadable file, got {:?}",
        error.file
    );
}

/// A directory whose own name is not valid UTF-8 is still a real directory:
/// discovery and the harness must keep the `Path` rather than round-tripping
/// the path through lossy text and looking somewhere else.
#[cfg(unix)]
#[test]
fn edge_non_utf8_directory_path_is_discovered_and_run() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let parent = scratch_dir("non-utf8-path");
    let dir = parent.join(OsStr::from_bytes(b"suite-\xff"));
    fs::create_dir_all(&dir).expect("non-UTF-8 scratch dir should be creatable");
    write_file(&dir, "only.rb", b"// test \"non utf8 path\"\n// end\n");

    let found = find_test_files(&dir).expect("non-UTF-8 directory should be searched");
    assert_eq!(found.len(), 1, "expected the .rb file, got {:?}", found);
    assert!(
        found[0].ends_with("only.rb"),
        "expected only.rb to be collected, got {:?}",
        found[0]
    );

    let results = redblue::testing::run_tests_in(&dir).expect("the suite should run");
    assert_eq!(
        (results.total, results.passed, results.failed),
        (1, 1, 0),
        "non-UTF-8 directory test did not pass cleanly: {:?}",
        results.errors
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
/// A fresh, empty directory under the project's own `target/tmp/`.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/tmp/project-test")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir should be creatable");
    dir
}

/// A fresh, empty directory outside every Redblue project, removed on drop.
///
/// `TMPDIR` is caller-controlled and may point inside the checkout, so the
/// system temp directory alone cannot guarantee "outside a project": discovery
/// walks every ancestor. Starting there, climb above the outermost project that
/// contains it, then assert the result really has no manifest above it.
struct OutsideScratch {
    path: PathBuf,
}

impl OutsideScratch {
    fn new(name: &str) -> Self {
        let mut base = std::env::temp_dir();
        while let Some(root) = find_project_root(&base) {
            let Some(parent) = root.parent() else {
                break;
            };
            base = parent.to_path_buf();
        }

        let path = base
            .join("rb-redblue-discovery")
            .join(format!("{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("outside scratch dir should be creatable");
        assert!(
            find_project_root(&path).is_none(),
            "{} is inside a Redblue project, so it cannot test `rb test` outside one",
            path.display()
        );
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for OutsideScratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// The `rb` binary under test.
fn rb() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rb"))
}

/// Runs `rb` in `dir` with `args`.
fn run_in(dir: &Path, args: &[&str]) -> Output {
    rb().current_dir(dir)
        .args(args)
        .output()
        .expect("rb should be runnable")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// End-to-end: `rb new hello` writes the layout, its manifest parses through
/// phase-044's schema, and bare `rb test` inside it runs the scaffolded test.
#[test]
fn rb_new_scaffolds_a_project_that_tests_green() {
    let parent = scratch("scaffold");

    let created = run_in(&parent, &["new", "hello"]);
    assert!(
        created.status.success(),
        "`rb new hello` failed: {}",
        stderr(&created)
    );

    let root = parent.join("hello");
    let manifest_path = root.join("redblue.manifest");
    let entry = root.join("src/main.rb");
    let tests = root.join("tests/main_test.rb");
    for path in [&manifest_path, &entry, &tests] {
        assert!(path.is_file(), "expected {} to exist", path.display());
    }

    let text = fs::read_to_string(&manifest_path).expect("manifest should be readable");
    let parsed = manifest::parse_manifest(&text)
        .unwrap_or_else(|error| panic!("fresh manifest is not valid: {error}"));
    assert_eq!(parsed.name.as_deref(), Some("hello"));
    assert_eq!(
        parsed.version.map(|v| v.to_string()).as_deref(),
        Some("0.1.0")
    );
    assert!(
        parsed.dependencies.is_empty(),
        "`rb new` must not invent dependencies: {:?}",
        parsed.dependencies
    );

    let entry_ran = run_in(&root, &["run", "src/main.rb"]);
    assert!(
        entry_ran.status.success(),
        "the scaffolded entry point does not run: {}{}",
        stdout(&entry_ran),
        stderr(&entry_ran)
    );
    assert!(
        stdout(&entry_ran).contains("Hello from hello"),
        "the scaffolded entry point did not greet the project: {}",
        stdout(&entry_ran)
    );

    let ran = run_in(&root, &["test"]);
    assert!(
        ran.status.success(),
        "`rb test` in a fresh project failed: {}{}",
        stdout(&ran),
        stderr(&ran)
    );
    assert!(
        stdout(&ran).contains("Passed: 1"),
        "the scaffolded test did not run: {}",
        stdout(&ran)
    );
}

/// `rb new` must never merge into occupied ground. The refused directory keeps
/// the file it already had, and no scaffold file appears beside it.
#[test]
fn edge_rb_new_refuses_an_existing_non_empty_directory() {
    let parent = scratch("occupied");
    let root = parent.join("hello");
    fs::create_dir_all(&root).expect("fixture dir");
    fs::write(root.join("keep.txt"), b"mine\n").expect("fixture file");

    let created = run_in(&parent, &["new", "hello"]);
    assert!(
        !created.status.success(),
        "`rb new` into an occupied directory must fail, got: {}{}",
        stdout(&created),
        stderr(&created)
    );
    assert!(
        stderr(&created).contains("hello"),
        "the refusal must name the directory, got: {}",
        stderr(&created)
    );
    assert_eq!(
        fs::read_to_string(root.join("keep.txt")).expect("file survives"),
        "mine\n",
        "`rb new` overwrote an existing file"
    );
    assert!(
        !root.join("redblue.manifest").exists(),
        "`rb new` wrote a manifest into occupied ground"
    );
}

/// A file in the project's place is not occupied ground to merge into: the
/// scaffold is refused and the file is left untouched.
#[test]
fn edge_rb_new_refuses_a_file_named_after_the_project() {
    let parent = scratch("file-in-the-way");
    let root = parent.join("hello");
    fs::write(&root, b"not a directory\n").expect("fixture file");

    let created = run_in(&parent, &["new", "hello"]);
    assert!(
        !created.status.success(),
        "`rb new` over a file must fail, got: {}{}",
        stdout(&created),
        stderr(&created)
    );
    assert!(
        stderr(&created).contains("not a directory"),
        "the refusal should explain that the path is not a directory, got: {}",
        stderr(&created)
    );
    assert_eq!(
        fs::read_to_string(&root).expect("fixture file survives"),
        "not a directory\n",
        "`rb new` overwrote the file in its way"
    );
    assert!(
        !root.join("redblue.manifest").exists(),
        "`rb new` tried to build a project inside a file"
    );
}

/// An existing empty directory is not occupied ground, so it is accepted and
/// the scaffold is written into it.
#[test]
fn edge_rb_new_accepts_an_existing_empty_directory() {
    let parent = scratch("empty-existing");
    let root = parent.join("hello");
    fs::create_dir_all(&root).expect("fixture dir");

    let created = run_in(&parent, &["new", "hello"]);
    assert!(
        created.status.success(),
        "`rb new` into an empty directory must succeed, got: {}{}",
        stdout(&created),
        stderr(&created)
    );
    for path in [
        root.join("redblue.manifest"),
        root.join("src/main.rb"),
        root.join("tests/main_test.rb"),
    ] {
        assert!(path.is_file(), "expected {} to exist", path.display());
    }

    let ran = run_in(&root, &["test"]);
    assert!(
        ran.status.success(),
        "`rb test` in the accepted empty directory failed: {}{}",
        stdout(&ran),
        stderr(&ran)
    );
    assert!(
        stdout(&ran).contains("Passed: 1"),
        "the scaffolded test did not run: {}",
        stdout(&ran)
    );
}

/// A missing name is a usage error, not a project called ``.
#[test]
fn edge_rb_new_without_a_name_is_a_usage_error() {
    let parent = scratch("no-name");

    let created = run_in(&parent, &["new"]);
    assert!(
        !created.status.success(),
        "bare `rb new` must fail, got: {}{}",
        stdout(&created),
        stderr(&created)
    );
    assert!(
        stderr(&created).contains("project name"),
        "the error should say a name is needed, got: {}",
        stderr(&created)
    );
    assert_eq!(
        fs::read_dir(&parent).expect("scratch dir").count(),
        0,
        "bare `rb new` created something in the working directory"
    );
}

/// A name carrying a path separator is refused and creates nothing anywhere:
/// the scaffold stays in the working directory or does not happen at all.
#[test]
fn edge_rb_new_with_a_path_separator_is_refused_in_place() {
    let parent = scratch("separator");

    let created = run_in(&parent, &["new", "nested/hello"]);
    assert!(
        !created.status.success(),
        "`rb new nested/hello` must be refused, got: {}{}",
        stdout(&created),
        stderr(&created)
    );
    assert!(
        stderr(&created).contains("path separator"),
        "the refusal should name the path separator, got: {}",
        stderr(&created)
    );
    assert!(
        !parent.join("nested").exists(),
        "`rb new` created a directory outside the working directory"
    );
    assert!(
        !parent.join("hello").exists(),
        "`rb new` created the leaf in the working directory"
    );
    assert_eq!(
        fs::read_dir(&parent).expect("scratch dir").count(),
        0,
        "the refused scaffold left files behind"
    );
}

/// Outside any project, bare `rb test` is a clean error naming `rb new`, not a
/// silent success that ran nothing.
#[test]
fn edge_rb_test_outside_a_project_names_rb_new() {
    let dir = OutsideScratch::new("outside");

    let ran = run_in(dir.path(), &["test"]);
    assert!(
        !ran.status.success(),
        "`rb test` outside a project must fail, got: {}{}",
        stdout(&ran),
        stderr(&ran)
    );
    assert!(
        stderr(&ran).contains("rb new"),
        "the error should tell the reader about `rb new`, got: {}",
        stderr(&ran)
    );
}

/// Root discovery walks up: from three directories inside `src/`, bare
/// `rb test` finds the project root and runs the root's `tests/` directory.
#[test]
fn edge_rb_test_walks_up_from_a_nested_directory() {
    let parent = scratch("walk-up");
    let created = run_in(&parent, &["new", "hello"]);
    assert!(created.status.success(), "{}", stderr(&created));

    let root = parent.join("hello");
    let deep = root.join("src/a/b");
    fs::create_dir_all(&deep).expect("nested fixture dirs");

    let ran = run_in(&deep, &["test"]);
    assert!(
        ran.status.success(),
        "`rb test` from a nested directory failed: {}{}",
        stdout(&ran),
        stderr(&ran)
    );
    assert!(
        stdout(&ran).contains("Passed: 1"),
        "the project's own test did not run from a nested directory: {}",
        stdout(&ran)
    );
}

/// A path with `..` components is canonicalised before the walk, so it answers
/// the project root itself rather than a path that still contains the parent
/// components.
#[test]
fn edge_project_root_canonicalises_parent_components() {
    let parent = scratch("root-dotdot");
    let created = run_in(&parent, &["new", "hello"]);
    assert!(created.status.success(), "{}", stderr(&created));

    let root = parent.join("hello");
    let deep = root.join("src/a/b");
    fs::create_dir_all(&deep).expect("nested fixture dirs");

    let found = find_project_root(&deep.join("../../..")).expect("project root should be found");
    let expected = fs::canonicalize(&root).expect("fixture root should canonicalise");
    assert_eq!(
        found, expected,
        "root discovery should answer the canonical project root"
    );
}

/// A project with no test files is a failure, not a report of zero tests that
/// exits successfully. A missing `tests/` and an empty one both say so.
#[test]
fn edge_rb_test_in_a_project_with_no_test_files_fails() {
    for (name, remove_directory) in [("no-tests-dir", true), ("empty-tests-dir", false)] {
        let parent = scratch(name);
        let created = run_in(&parent, &["new", "hello"]);
        assert!(created.status.success(), "{}", stderr(&created));

        let root = parent.join("hello");
        let tests = root.join("tests");
        if remove_directory {
            fs::remove_dir_all(&tests).expect("fixture tests dir");
        } else {
            for entry in fs::read_dir(&tests).expect("fixture tests dir") {
                let path = entry.expect("tests entry").path();
                fs::remove_file(path).expect("fixture test file");
            }
        }

        let ran = run_in(&root, &["test"]);
        assert!(
            !ran.status.success(),
            "`rb test` with no test files must fail, got: {}{}",
            stdout(&ran),
            stderr(&ran)
        );
        assert!(
            stderr(&ran).contains("No Redblue test files"),
            "the error should say no test files were found, got: {}",
            stderr(&ran)
        );
    }
}

/// A `tests/` directory with `.rb` files but no `test` blocks has executed
/// nothing, so `rb test` must fail instead of reporting a zero-test success.
#[test]
fn edge_rb_test_that_defines_no_tests_fails() {
    let cases = [
        ("comment-only", b"// nothing here\n".as_slice()),
        ("empty-file", b"".as_slice()),
    ];

    for (name, contents) in cases {
        let parent = scratch(&format!("zero-tests-{name}"));
        let created = run_in(&parent, &["new", "hello"]);
        assert!(created.status.success(), "{}", stderr(&created));

        let root = parent.join("hello");
        fs::write(root.join("tests/main_test.rb"), contents).expect("fixture suite");

        let ran = run_in(&root, &["test"]);
        assert!(
            !ran.status.success(),
            "`rb test` with zero defined tests must fail for {name}, got: {}{}",
            stdout(&ran),
            stderr(&ran)
        );
        assert!(
            stderr(&ran).contains("No tests were defined"),
            "the error should say no tests were defined for {name}, got: {}",
            stderr(&ran)
        );
    }
}

/// The explicit-path form has the same zero-execution guard as the project
/// form: `rb test empty.rb` must not exit 0 after defining no tests.
#[test]
fn edge_rb_test_with_a_path_that_defines_no_tests_fails() {
    for (name, contents) in [
        ("comment-only", b"// nothing here\n".as_slice()),
        ("empty-file", b"".as_slice()),
    ] {
        let parent = scratch(&format!("explicit-zero-tests-{name}"));
        let created = run_in(&parent, &["new", "hello"]);
        assert!(created.status.success(), "{}", stderr(&created));

        let root = parent.join("hello");
        fs::write(root.join("empty.rb"), contents).expect("fixture file");

        let ran = run_in(&root, &["test", "empty.rb"]);
        assert!(
            !ran.status.success(),
            "`rb test <file>` with zero defined tests must fail for {name}, got: {}{}",
            stdout(&ran),
            stderr(&ran)
        );
        assert!(
            stderr(&ran).contains("No tests were defined"),
            "the error should say no tests were defined for {name}, got: {}",
            stderr(&ran)
        );
    }
}

/// A project whose manifest does not parse is refused before any test runs,
/// naming the manifest and the offending line.
#[test]
fn edge_rb_test_with_a_malformed_manifest_names_the_file_and_line() {
    let parent = scratch("malformed-manifest");
    let created = run_in(&parent, &["new", "hello"]);
    assert!(created.status.success(), "{}", stderr(&created));

    let root = parent.join("hello");
    let manifest_path = root.join("redblue.manifest");
    fs::write(&manifest_path, "name hello\nbogus value\n").expect("fixture manifest");

    let ran = run_in(&root, &["test"]);
    assert!(
        !ran.status.success(),
        "a malformed manifest must fail `rb test`, got: {}{}",
        stdout(&ran),
        stderr(&ran)
    );
    let message = stderr(&ran);
    assert!(
        message.contains("redblue.manifest"),
        "the error should name the manifest, got: {message}"
    );
    assert!(
        message.contains("line 2"),
        "the error should name the offending line, got: {message}"
    );
}

/// `rb test <path>` keeps its old meaning: it runs exactly that file, even
/// inside a project whose own `tests/` would fail.
#[test]
fn edge_rb_test_with_a_path_runs_only_that_file() {
    let parent = scratch("explicit-path");
    let created = run_in(&parent, &["new", "hello"]);
    assert!(created.status.success(), "{}", stderr(&created));

    let root = parent.join("hello");
    fs::write(
        root.join("tests/main_test.rb"),
        "test \"project suite\"\n    expect 1 to be 2\nend\n",
    )
    .expect("fixture suite");
    fs::write(
        root.join("passing.rb"),
        "test \"explicit\"\n    expect 1 + 1 to be 2\nend\n",
    )
    .expect("fixture file");

    let ran = run_in(&root, &["test", "passing.rb"]);
    assert!(
        ran.status.success(),
        "`rb test <path>` must run only the named file: {}{}",
        stdout(&ran),
        stderr(&ran)
    );
}

/// The explicit-path form still runs exactly the named file, and a failure
/// names the file it ran instead of the old generic `inline` label.
#[test]
fn edge_rb_test_with_a_path_names_the_file_in_failure_output() {
    let parent = scratch("explicit-path-failure");
    let created = run_in(&parent, &["new", "hello"]);
    assert!(created.status.success(), "{}", stderr(&created));

    let root = parent.join("hello");
    fs::write(
        root.join("failing.rb"),
        "test \"explicit failure\"\n    expect 1 to be 2\nend\n",
    )
    .expect("fixture file");

    let ran = run_in(&root, &["test", "failing.rb"]);
    assert!(
        !ran.status.success(),
        "a failing explicit file must exit non-zero: {}{}",
        stdout(&ran),
        stderr(&ran)
    );
    let output = format!("{}{}", stdout(&ran), stderr(&ran));
    assert!(
        output.contains("explicit failure"),
        "the failure output must name the failing test, got: {output}"
    );
    assert!(
        output.contains("Location:") && output.contains("failing.rb"),
        "the failure output must point at the named file, got: {output}"
    );
}

/// A whole-file parse failure still names the file and the line it came from;
/// discarding the error's span would leave the reader at `Location: unknown`.
#[test]
fn edge_rb_test_with_a_path_reports_a_parse_failure_file_and_line() {
    let parent = scratch("explicit-path-parse-failure");
    let created = run_in(&parent, &["new", "hello"]);
    assert!(created.status.success(), "{}", stderr(&created));

    let root = parent.join("hello");
    fs::write(
        root.join("broken.rb"),
        "test \"broken\"\n    expect x to be\nend\n",
    )
    .expect("fixture file");

    let ran = run_in(&root, &["test", "broken.rb"]);
    assert!(
        !ran.status.success(),
        "a file that does not parse must fail: {}{}",
        stdout(&ran),
        stderr(&ran)
    );
    let output = format!("{}{}", stdout(&ran), stderr(&ran));
    assert!(
        output.contains("broken.rb:2"),
        "the failure must name the file and line, got: {output}"
    );
}

/// A failing project suite must say which test failed and why, not only print
/// a count. The count alone leaves the reader unable to find the failure.
#[test]
fn edge_rb_test_reports_the_failing_test_name_and_assertion() {
    let parent = scratch("failing-report");
    let created = run_in(&parent, &["new", "hello"]);
    assert!(created.status.success(), "{}", stderr(&created));

    let root = parent.join("hello");
    fs::write(
        root.join("tests/main_test.rb"),
        "test \"math works\"\n    expect 1 + 1 to be 3\nend\n",
    )
    .expect("fixture suite");

    let ran = run_in(&root, &["test"]);
    assert!(
        !ran.status.success(),
        "a failing project suite must exit non-zero: {}{}",
        stdout(&ran),
        stderr(&ran)
    );
    let output = format!("{}{}", stdout(&ran), stderr(&ran));
    assert!(
        output.contains("math works"),
        "the failure output must name the failing test, got: {output}"
    );
    assert!(
        output.contains("FAILED"),
        "the failure output must mark the test as failed, got: {output}"
    );
    assert!(
        output.contains("Location:") && output.contains("main_test.rb"),
        "the failure output must point at the test file, got: {output}"
    );
    assert!(
        output.contains("Error:"),
        "the failure output must carry the assertion message, got: {output}"
    );
}

/// Every failure is reported, not just the first one. A runner that stops at
/// the first failure hides the rest of the suite behind a rerun loop.
#[test]
fn edge_rb_test_reports_every_failing_test_not_just_the_first() {
    let parent = scratch("all-failures");
    let created = run_in(&parent, &["new", "hello"]);
    assert!(created.status.success(), "{}", stderr(&created));

    let root = parent.join("hello");
    fs::write(
        root.join("tests/main_test.rb"),
        "test \"first failure\"\n    expect 1 to be 2\nend\n\n\
         test \"second failure\"\n    expect 3 to be 4\nend\n",
    )
    .expect("fixture suite");

    let ran = run_in(&root, &["test"]);
    assert!(
        !ran.status.success(),
        "a suite with two failures must exit non-zero: {}{}",
        stdout(&ran),
        stderr(&ran)
    );
    let output = format!("{}{}", stdout(&ran), stderr(&ran));
    for name in ["first failure", "second failure"] {
        assert!(
            output.contains(name),
            "the failure report must name {name:?}, got: {output}"
        );
    }
}

/// An empty project name is refused before any directory is created.
#[test]
fn edge_create_project_refuses_an_empty_name() {
    let parent = scratch("empty-project-name");
    let result = redblue::project::create_project(&parent, "");
    assert!(
        result.is_err(),
        "an empty project name must be an error, got: {:?}",
        result
    );
    assert_eq!(
        fs::read_dir(&parent).expect("scratch dir").count(),
        0,
        "a refused name must not leave a directory behind"
    );
}

/// A symbolic link in the project's place must not be followed. Writing through
/// it would put the scaffold outside the working directory, which is the same
/// escape a path separator would allow.
#[cfg(unix)]
#[test]
fn edge_rb_new_refuses_a_symlinked_directory_in_place() {
    use std::os::unix::fs::symlink;

    let parent = scratch("symlink-empty");
    let outside = scratch("symlink-empty-target");
    let root = parent.join("hello");
    symlink(&outside, &root).expect("fixture symlink");

    let created = run_in(&parent, &["new", "hello"]);
    assert!(
        !created.status.success(),
        "`rb new` over a symlinked directory must fail, got: {}{}",
        stdout(&created),
        stderr(&created)
    );
    let message = stderr(&created);
    assert!(
        message.contains("symbolic link"),
        "the refusal should name the symbolic link, got: {message}"
    );
    assert!(
        message.contains("hello"),
        "the refusal must name the project, got: {message}"
    );
    assert_eq!(
        fs::read_dir(&outside).expect("outside dir").count(),
        0,
        "`rb new` wrote through the symlink into {}",
        outside.display()
    );
    assert!(
        root.symlink_metadata()
            .expect("symlink survives")
            .file_type()
            .is_symlink(),
        "`rb new` replaced the caller's symbolic link"
    );
}

/// A dangling symbolic link is still a link, not free ground: it is refused and
/// its target is not created.
#[cfg(unix)]
#[test]
fn edge_rb_new_refuses_a_dangling_symlink() {
    use std::os::unix::fs::symlink;

    let parent = scratch("symlink-dangling");
    let target = parent.join("missing-target");
    let root = parent.join("hello");
    symlink(&target, &root).expect("fixture symlink");

    let created = run_in(&parent, &["new", "hello"]);
    assert!(
        !created.status.success(),
        "`rb new` over a dangling symlink must fail, got: {}{}",
        stdout(&created),
        stderr(&created)
    );
    assert!(
        stderr(&created).contains("symbolic link"),
        "the refusal should name the symbolic link, got: {}",
        stderr(&created)
    );
    assert!(
        !target.exists(),
        "`rb new` created the dangling link's target at {}",
        target.display()
    );
    assert!(
        root.symlink_metadata()
            .expect("symlink survives")
            .file_type()
            .is_symlink(),
        "`rb new` replaced the caller's dangling link"
    );
}

/// The library entry point refuses a symlinked root for the same reason the
/// command does: the scaffold must stay under `parent`.
#[cfg(unix)]
#[test]
fn edge_create_project_refuses_a_symlinked_root() {
    use std::os::unix::fs::symlink;

    let parent = scratch("symlink-api");
    let outside = scratch("symlink-api-target");
    let root = parent.join("hello");
    symlink(&outside, &root).expect("fixture symlink");

    let result = redblue::project::create_project(&parent, "hello");
    assert!(
        result.is_err(),
        "create_project must refuse a symlinked root, got: {result:?}"
    );
    assert_eq!(
        fs::read_dir(&outside).expect("outside dir").count(),
        0,
        "create_project wrote through the symlink into {}",
        outside.display()
    );
}

/// A skip still increments `total`, so a suite whose only marker is a skip has
/// executed no test. Both `rb test` and `rb test <file>` must reject it instead
/// of exiting 0 with `Tests run: 1`.
#[test]
fn edge_rb_test_that_only_skips_fails() {
    let parent = scratch("skip-only");
    let created = run_in(&parent, &["new", "hello"]);
    assert!(created.status.success(), "{}", stderr(&created));

    let root = parent.join("hello");
    let source = "// skip\nnot implemented yet\n";
    fs::write(root.join("tests/main_test.rb"), source).expect("fixture suite");

    let project_run = run_in(&root, &["test"]);
    assert!(
        !project_run.status.success(),
        "`rb test` with only a skip must fail, got: {}{}",
        stdout(&project_run),
        stderr(&project_run)
    );
    assert!(
        stderr(&project_run).contains("No tests were run")
            && stderr(&project_run).contains("(1 skipped)"),
        "the project run should say the only test was skipped, got: {}",
        stderr(&project_run)
    );

    fs::write(root.join("skip_only.rb"), source).expect("fixture file");
    let file_run = run_in(&root, &["test", "skip_only.rb"]);
    assert!(
        !file_run.status.success(),
        "`rb test <file>` with only a skip must fail, got: {}{}",
        stdout(&file_run),
        stderr(&file_run)
    );
    assert!(
        stderr(&file_run).contains("No tests were run")
            && stderr(&file_run).contains("(1 skipped)"),
        "the explicit run should say the only test was skipped, got: {}",
        stderr(&file_run)
    );
}

/// A syntax error is a failure even when the file has no `test` block to label
/// it. Dropping the error made `rb test <file>` answer `No tests were defined`,
/// hiding the line the reader has to fix.
#[test]
fn edge_rb_test_reports_a_syntax_error_without_a_test_block() {
    let parent = scratch("syntax-error-no-block");
    let created = run_in(&parent, &["new", "hello"]);
    assert!(created.status.success(), "{}", stderr(&created));

    let root = parent.join("hello");
    fs::write(root.join("broken.rb"), "say \"hi\"\nset x to (\n").expect("fixture file");

    let ran = run_in(&root, &["test", "broken.rb"]);
    assert!(
        !ran.status.success(),
        "a file that does not parse must fail: {}{}",
        stdout(&ran),
        stderr(&ran)
    );
    let output = format!("{}{}", stdout(&ran), stderr(&ran));
    assert!(
        !output.contains("No tests were defined"),
        "the syntax error was hidden behind the zero-test guard: {output}"
    );
    assert!(
        output.contains("FAILED") && output.contains("broken.rb:2"),
        "the failure must name the syntax error and its line, got: {output}"
    );
}

/// A syntax error outside a marker body is still the whole file's error. A
/// passing marker test must not hide a stray unparsable line after `// end`,
/// which would let `rb test` exit 0 over a file that does not parse.
#[test]
fn edge_rb_test_reports_a_syntax_error_outside_a_marker_test() {
    let parent = scratch("syntax-error-outside-marker");
    let created = run_in(&parent, &["new", "hello"]);
    assert!(created.status.success(), "{}", stderr(&created));

    let root = parent.join("hello");
    let source = "// test \"passing marker\"\nexpect 1 to be 1\n// end\nset x to (\n";
    fs::write(root.join("tests/main_test.rb"), source).expect("fixture suite");

    let project_run = run_in(&root, &["test"]);
    assert!(
        !project_run.status.success(),
        "a syntax error outside the marker body must fail: {}{}",
        stdout(&project_run),
        stderr(&project_run)
    );
    let project_output = format!("{}{}", stdout(&project_run), stderr(&project_run));
    assert!(
        project_output.contains("FAILED") && project_output.contains("main_test.rb:4"),
        "the failure must name the syntax error and its line, got: {project_output}"
    );

    fs::write(root.join("trailing.rb"), source).expect("fixture file");
    let file_run = run_in(&root, &["test", "trailing.rb"]);
    assert!(
        !file_run.status.success(),
        "the explicit form must report the same syntax error: {}{}",
        stdout(&file_run),
        stderr(&file_run)
    );
    let file_output = format!("{}{}", stdout(&file_run), stderr(&file_run));
    assert!(
        file_output.contains("FAILED") && file_output.contains("trailing.rb:4"),
        "the explicit failure must name the syntax error and its line, got: {file_output}"
    );
}

/// A benchmark body that fails must be reported as a failure. Dropping the
/// outcome let the zero-execution guard answer `No tests were defined` and hide
/// the benchmark's own syntax or runtime error.
#[test]
fn edge_rb_test_reports_a_failing_benchmark() {
    let parent = scratch("benchmark-error");
    let created = run_in(&parent, &["new", "hello"]);
    assert!(created.status.success(), "{}", stderr(&created));

    let root = parent.join("hello");
    let source = "// bench \"broken\"\nset to to to\n// end\n";
    fs::write(root.join("bench.rb"), source).expect("fixture benchmark");

    let ran = run_in(&root, &["test", "bench.rb"]);
    assert!(
        !ran.status.success(),
        "a failing benchmark must fail the run: {}{}",
        stdout(&ran),
        stderr(&ran)
    );
    let output = format!("{}{}", stdout(&ran), stderr(&ran));
    assert!(
        !output.contains("No tests were defined"),
        "the benchmark failure was hidden behind the zero-test guard: {output}"
    );
    assert!(
        output.contains("broken") && output.contains("FAILED"),
        "the failure must name the benchmark, got: {output}"
    );
}

/// A benchmark that runs to completion is executed work, not an empty file. It
/// must therefore satisfy the zero-execution guard, both through an explicit
/// path and as the only file in a project's suite.
#[test]
fn edge_rb_test_reports_a_passing_benchmark() {
    let parent = scratch("benchmark-passing");
    let created = run_in(&parent, &["new", "hello"]);
    assert!(created.status.success(), "{}", stderr(&created));

    let root = parent.join("hello");
    let tests_dir = root.join("tests");
    // Leave the benchmark as the project's only test, so bare `rb test` proves
    // a passing benchmark is not mistaken for an empty suite.
    fs::remove_file(tests_dir.join("main_test.rb")).expect("remove scaffold test");
    let source = "// bench \"passing\"\nset total to 1 + 1\n// end\n";
    fs::write(tests_dir.join("bench.rb"), source).expect("fixture benchmark");

    let file_run = run_in(&root, &["test", "tests/bench.rb"]);
    assert!(
        file_run.status.success(),
        "a passing benchmark must exit 0: {}{}",
        stdout(&file_run),
        stderr(&file_run)
    );
    let file_output = format!("{}{}", stdout(&file_run), stderr(&file_run));
    assert!(
        !file_output.contains("No tests were defined"),
        "the passing benchmark was hidden behind the zero-test guard: {file_output}"
    );
    assert!(
        file_output.contains("BENCHMARK") && file_output.contains("ns/iter"),
        "the passing benchmark must report its measurement, got: {file_output}"
    );

    let project_run = run_in(&root, &["test"]);
    assert!(
        project_run.status.success(),
        "a project whose only file is a passing benchmark must exit 0: {}{}",
        stdout(&project_run),
        stderr(&project_run)
    );
    let project_output = format!("{}{}", stdout(&project_run), stderr(&project_run));
    assert!(
        !project_output.contains("No tests were defined"),
        "the project run hid the passing benchmark behind the zero-test guard: {project_output}"
    );
    assert!(
        project_output.contains("Passed: 1"),
        "the passing benchmark should count as one executed test, got: {project_output}"
    );
}

/// `read_dir` follows a symlink for the path it is given, so a symlinked root
/// would run a suite from outside the project. Discovery must inspect the root
/// itself and refuse it before opening anything.
#[cfg(unix)]
#[test]
fn edge_discovery_refuses_a_symlinked_root() {
    use std::os::unix::fs::symlink;

    let target = scratch_dir("symlinked-root-target");
    write_file(&target, "outside.rb", b"// test \"outside\"\n// end\n");
    let parent = scratch_dir("symlinked-root");
    let root = parent.join("tests");
    symlink(&target, &root).expect("fixture symlink");

    let error = find_test_files(&root).expect_err("a symlinked root must not be followed");
    let message = error.to_string();
    assert!(
        message.contains("symbolic link"),
        "the refusal should name the symbolic link, got: {message}"
    );
    assert!(
        message.contains("tests"),
        "the refusal should name the root, got: {message}"
    );
}

/// A trailing separator forces the OS to resolve through the final link, so
/// the root has to be normalised before inspection. `run_all_tests` supplies
/// `tests/` with exactly that separator.
#[cfg(unix)]
#[test]
fn edge_discovery_refuses_a_symlinked_root_with_a_trailing_separator() {
    use std::os::unix::fs::symlink;

    let target = scratch_dir("symlinked-root-separator-target");
    write_file(&target, "outside.rb", b"// test \"outside\"\n// end\n");
    let parent = scratch_dir("symlinked-root-separator");
    let root = parent.join("tests");
    symlink(&target, &root).expect("fixture symlink");

    let with_separator = format!("{}/", root.display());
    let error = find_test_files(&with_separator)
        .expect_err("a symlinked root with a trailing separator must not be followed");
    let message = error.to_string();
    assert!(
        message.contains("symbolic link"),
        "the refusal should name the symbolic link, got: {message}"
    );
    assert!(
        message.contains("tests"),
        "the refusal should name the root, got: {message}"
    );
}
