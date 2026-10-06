//! Gate over the Redblue-language suite in `tests/*.rb`.
//!
//! `tests/suite.rb`, `tests/test_arithmetic.rb` and `tests/integration_test.rb`
//! used to ship with every `test` block commented out, so the Redblue suite
//! reported a non-zero test count while asserting nothing at all. These tests
//! fail if that regresses.

use std::path::Path;

/// Every `.rb` file the Redblue harness collects.
fn suite_files() -> Vec<String> {
    let mut files = redblue::testing::find_test_files("tests").expect("tests/ should be readable");
    files.sort();
    assert!(
        !files.is_empty(),
        "no .rb files found under tests/ - the Redblue suite is empty"
    );
    files
}

/// Source of one suite file.
fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {}", path, e))
}

/// Keywords that open a Redblue block and are closed by `end`. `else`,
/// `catch`, `finally` and `until` continue an open block and add no depth.
const BLOCK_OPENERS: [&str; 10] = [
    "test", "if", "unless", "for", "repeat", "while", "try", "object", "to", "module",
];

fn opens_a_block(line: &str) -> bool {
    let first = line.split_whitespace().next().unwrap_or("");
    BLOCK_OPENERS.contains(&first)
}

/// A test block starts at a `test "..."` header line and ends at the `end` that
/// matches it. Nested `if`/`unless`/`for`/`try`/`object`/`to` blocks inside a
/// test carry their own `end`, so the body is read with a depth counter. Every
/// keyword in [`BLOCK_OPENERS`] must be listed: a missing one makes the counter
/// reach zero at the inner `end` and silently truncate the body, which reads as
/// "the block has no assertion" rather than as a harness bug.
fn test_blocks(source: &str) -> Vec<(usize, String, String)> {
    let lines: Vec<&str> = source.lines().collect();
    let mut blocks = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        if let Some(rest) = trimmed.strip_prefix("test ") {
            let name = rest.trim().trim_matches('"').to_string();
            let header_line = i + 1;
            let mut body = Vec::new();
            let mut depth = 1usize;
            i += 1;
            while i < lines.len() && depth > 0 {
                let line = lines[i];
                let t = line.trim();
                if opens_a_block(t) {
                    depth += 1;
                } else if t == "end" {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                if depth > 0 {
                    body.push(line);
                }
                i += 1;
            }
            blocks.push((header_line, name, body.join("\n")));
        }
        i += 1;
    }

    blocks
}

#[test]
fn edge_suite_reports_at_least_40_redblue_tests() {
    let results = redblue::testing::run_all_tests().expect("Redblue suite should be collectable");

    assert_eq!(
        results.failed, 0,
        "Redblue suite has failing tests: {:#?}",
        results.errors
    );
    assert!(
        results.total >= 40,
        "Redblue suite must hold >= 40 test blocks, found {} \
         ({} passed, {} failed, {} skipped)",
        results.total,
        results.passed,
        results.failed,
        results.skipped
    );
}

#[test]
fn edge_suite_runs_no_skipped_tests() {
    let results = redblue::testing::run_all_tests().expect("Redblue suite should be collectable");

    assert_eq!(
        results.skipped, 0,
        "Redblue suite skips {} test(s); skipped tests are muted evidence",
        results.skipped
    );
    assert_eq!(
        results.total, results.passed,
        "every collected Redblue test must pass"
    );
}

#[test]
fn edge_suite_declares_no_skip_markers() {
    for path in suite_files() {
        let source = read(&path);
        for (n, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            assert!(
                !trimmed.starts_with("// skip") && !trimmed.starts_with("# skip"),
                "{}:{} carries a skip marker: {}",
                path,
                n + 1,
                trimmed
            );
        }
    }
}

#[test]
fn every_test_block_carries_an_assertion() {
    let mut blocks_seen = 0usize;

    for path in suite_files() {
        let source = read(&path);
        let blocks = test_blocks(&source);
        assert!(
            !blocks.is_empty(),
            "{} declares no test blocks; a suite of zero tests is not a suite",
            path
        );

        for (line, name, body) in blocks {
            blocks_seen += 1;
            let asserts = body.contains("expect ") || body.contains("assert.");
            assert!(
                asserts,
                "{}:{} `test \"{}\"` has no assertion - a smoke test proves nothing",
                path, line, name
            );
        }
    }

    assert!(
        blocks_seen >= 40,
        "expected >= 40 Redblue test blocks across tests/*.rb, found {}",
        blocks_seen
    );
}

#[test]
fn edge_suite_covers_every_required_area() {
    let mut source = String::new();
    for path in suite_files() {
        source.push_str(&read(&path));
        source.push('\n');
    }
    let lower = source.to_lowercase();

    for area in [
        "arithmetic",
        "text",
        "list",
        "record",
        "control",
        "function",
        "object",
        "module",
    ] {
        assert!(
            lower.contains(area),
            "no suite file covers `{}`; the phase requires all eight areas",
            area
        );
    }
}

#[test]
fn edge_suite_contains_named_edge_tests() {
    let mut edges = 0usize;

    for path in suite_files() {
        for (_, name, _) in test_blocks(&read(&path)) {
            if name.starts_with("edge_") {
                edges += 1;
            }
        }
    }

    assert!(edges >= 8, "expected >= 8 edge_* tests, found {}", edges);
}

#[test]
fn edge_suite_asserts_that_failures_are_produced() {
    let mut failures = 0usize;

    for path in suite_files() {
        let source = read(&path);
        for (_, _, body) in test_blocks(&source) {
            if body.contains("catch ") {
                failures += 1;
            }
        }
    }

    assert!(
        failures >= 5,
        "expected >= 5 tests that assert a failure is produced via `catch`, found {}",
        failures
    );
}

#[test]
fn edge_suite_files_are_not_all_commented_out() {
    let mut live_blocks = 0usize;

    for path in suite_files() {
        let base = Path::new(&path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if base == "integration_test.rs" || base.ends_with("_test.rs") {
            continue;
        }
        live_blocks += test_blocks(&read(&path)).len();
    }

    assert!(
        live_blocks >= 40,
        "Redblue-language test blocks dropped to {}; the suite is commented out again",
        live_blocks
    );
}
