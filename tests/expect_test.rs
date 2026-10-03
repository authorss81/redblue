use redblue::testing::{TestError, TestHarness, TestResults};

/// Wraps a Redblue body in the `// test "name"` / `// end` markers that
/// `TestHarness::run_source` uses for discovery. The body lines are *not*
/// comment-prefixed, so they are lexed as real Redblue code.
fn test_source(name: &str, body: &str) -> String {
    format!("// test \"{}\"\n{}\n// end\n", name, body)
}

fn run(body: &str) -> TestResults {
    let mut harness = TestHarness::new();
    let source = test_source("body", body);
    harness
        .run_source(&source)
        .expect("harness source scan should not itself fail");
    harness.results()
}

/// Asserts a body passes, returning nothing on success. Panics with the
/// recorded failure message otherwise, so a broken assertion is debuggable.
#[track_caller]
fn assert_passes(body: &str) {
    let results = run(body);
    assert_eq!(
        results.passed,
        1,
        "expected `{}` to pass, but it failed: {:?}",
        body,
        failure_messages(&results)
    );
    assert_eq!(results.failed, 0, "unexpected failures in `{}`", body);
}

/// Asserts a body fails, returning the recorded failure so the caller can make
/// a further assertion about its message.
#[track_caller]
fn assert_fails(body: &str) -> TestError {
    let results = run(body);
    assert_eq!(
        results.failed, 1,
        "expected `{}` to fail, but {} passed",
        body, results.passed
    );
    assert_eq!(results.passed, 0);
    assert_eq!(results.errors.len(), 1, "exactly one error is recorded");
    results.errors[0].clone()
}

fn failure_messages(results: &TestResults) -> Vec<String> {
    results.errors.iter().map(|e| e.message.clone()).collect()
}

/// `TestHarness` terminates a discovered test body at the first line that
/// trims to `end`, so block-structured `expect` has to be driven through
/// `run_source` instead of the `// test` marker scanner.
fn run_program(source: &str) -> Result<(), redblue::Error> {
    redblue::run_source(source)
}

// ---------------------------------------------------------------------------
// The two directions the harness was previously unable to express at all.
// ---------------------------------------------------------------------------

#[test]
fn expect_mismatch_fails_the_test() {
    let error = assert_fails("expect 1 to be 2");

    assert!(
        error.message.contains('1') && error.message.contains('2'),
        "failure message must name both values, got: {}",
        error.message
    );
    assert_eq!(
        error.expected.as_deref(),
        Some("Number(2.0)"),
        "expected value must be recorded on the failure"
    );
    assert_eq!(
        error.actual.as_deref(),
        Some("Number(1.0)"),
        "actual value must be recorded on the failure"
    );
}

#[test]
fn expect_match_passes_the_test() {
    assert_passes("expect 1 to be 1");
}

#[test]
fn expect_matches_a_computed_expression() {
    assert_passes("set result to 2 + 3\nexpect result to be 5");
    assert_passes("set a to 10\nset b to 32\nexpect a * b to be 320");
}

#[test]
fn expect_tolerates_omitting_be() {
    // `expect A to B` is accepted alongside the documented `expect A to be B`.
    assert_passes("expect 1 to 1");
    assert_fails("expect 1 to 2");
}

// ---------------------------------------------------------------------------
// Empty / zero / nothing
// ---------------------------------------------------------------------------

#[test]
fn edge_empty_values_compare_equal_to_themselves() {
    assert_passes("expect nothing to be nothing");
    assert_passes("expect \"\" to be \"\"");
    assert_passes("expect [] to be []");
    assert_passes("expect {} to be {}");
    assert_passes("expect 0 to be 0");
}

#[test]
fn edge_empty_text_is_not_the_same_as_nothing() {
    // Guards against an equality that treats "" and nothing as interchangeable.
    let error = assert_fails("expect \"\" to be nothing");
    assert!(
        error.message.contains("Nothing") && error.message.contains("Text"),
        "message must distinguish nothing from empty text, got: {}",
        error.message
    );

    let error = assert_fails("expect [] to be nothing");
    assert!(
        error.message.contains("List"),
        "empty list must not equal nothing, got: {}",
        error.message
    );
}

// ---------------------------------------------------------------------------
// Singleton and boundary
// ---------------------------------------------------------------------------

#[test]
fn edge_singleton_list_compares_elementwise() {
    assert_passes("expect [1] to be [1]");
    assert_fails("expect [1] to be [2]");
    assert_fails("expect [1] to be []");
    assert_fails("expect [1] to be [1, 1]");
}

#[test]
fn edge_index_boundaries_feed_the_actual_value() {
    assert_passes("expect [10, 20, 30][0] to be 10");
    assert_passes("expect [10, 20, 30][2] to be 30");
    assert_passes("expect [10, 20, 30][-1] to be 30");
    assert_fails("expect [10, 20, 30][1] to be 30");
}

// ---------------------------------------------------------------------------
// Type mismatch
// ---------------------------------------------------------------------------

#[test]
fn edge_type_mismatch_fails() {
    let cases = [
        "expect 1 to be \"one\"",
        "expect \"one\" to be 1",
        "expect yes to be 1",
        "expect [1] to be 1",
        "expect 1 to be nothing",
        "expect nothing to be 0",
        "expect {a: 1} to be 1",
    ];

    for body in cases {
        let error = assert_fails(body);
        assert!(
            !error.message.is_empty(),
            "`{}` must produce a named failure, got an empty message",
            body
        );
        assert!(
            error.expected.is_some() && error.actual.is_some(),
            "`{}` is a division-by-zero error, not a value mismatch",
            body
        );
    }
}

// ---------------------------------------------------------------------------
// Numeric boundaries
// ---------------------------------------------------------------------------

#[test]
fn edge_numeric_boundaries_compare_exactly() {
    // -0.0 == 0.0 under the equality expect uses.
    assert_passes("expect -0.0 to be 0");
    assert_passes("expect 0 to be -0");
    // Floats are compared exactly, so this is a genuine mismatch and not a
    // silent pass.
    assert_fails("expect 0.1 + 0.2 to be 0.3");
}

#[test]
fn edge_division_by_zero_is_a_runtime_error_not_an_assertion() {
    let mut harness = TestHarness::new();
    let source = test_source("div", "expect 1 / 0 to be 1");
    harness
        .run_source(&source)
        .expect("harness source scan should not itself fail");
    let results = harness.results();

    assert_eq!(results.failed, 1);
    let error = &results.errors[0];
    assert!(
        error.message.contains("Division by zero"),
        "expected a division-by-zero runtime error, got: {}",
        error.message
    );
    assert!(
        error.expected.is_none() && error.actual.is_none(),
        "a runtime error is not an expectation mismatch, got {:?}",
        (&error.expected, &error.actual)
    );
}

// ---------------------------------------------------------------------------
// Unicode and escapes
// ---------------------------------------------------------------------------

#[test]
fn edge_unicode_compares_by_whole_string() {
    assert_passes("expect \"héllo\" to be \"héllo\"");
    assert_passes("expect \"日本語\" to be \"日本語\"");
    assert_passes("expect \"🎉🎉\" to be \"🎉🎉\"");
    // RTL text and a precomposed accent must survive comparison intact.
    assert_passes("expect \"שלום\" to be \"שלום\"");
    assert_passes("expect \"é\" to be \"é\"");

    assert_fails("expect \"🎉\" to be \"🎊\"");
    assert_fails("expect \"日本語\" to be \"日本\"");
    assert_fails("expect \"héllo\" to be \"hello\"");
}

#[test]
fn edge_escapes_are_compared_literally() {
    assert_passes("expect \"a\\nb\" to be \"a\\nb\"");
    assert_fails("expect \"a\\nb\" to be \"a b\"");
    assert_passes("expect \"say \\\"hi\\\"\" to be \"say \\\"hi\\\"\"");

    // Redblue has no `\u{...}` escape, so it is a literal backslash-u. Assert
    // the real behaviour rather than a wish: the strings differ.
    assert_fails("expect \"e\\u{301}\" to be \"é\"");
}

#[test]
fn edge_normalisation_is_not_applied_to_text_comparison() {
    // U+00E9 (NFC) and U+0065 U+0301 (NFD) are distinct strings and must not
    // compare equal just because they render the same.
    assert_passes("expect \"e\u{301}\" to be \"e\u{301}\"");
    assert_fails("expect \"e\u{301}\" to be \"\u{e9}\"");

    let error = assert_fails("expect \"e\u{301}\" to be \"\u{e9}\"");
    assert!(
        error.message.contains('1') || error.message.contains("Text"),
        "a text mismatch must name both texts, got: {}",
        error.message
    );
}

// ---------------------------------------------------------------------------
// Nesting
// ---------------------------------------------------------------------------

#[test]
fn edge_nested_containers_compare_structurally() {
    assert_passes("expect [1, [2, [3]]] to be [1, [2, [3]]]");
    assert_passes("expect {a: [1, {b: 2}]} to be {a: [1, {b: 2}]}");
    assert_fails("expect [1, [2, [3]]] to be [1, [2, [4]]]");
    assert_fails("expect [[1]] to be [1]");
}

#[test]
fn expect_inside_a_control_flow_block() {
    // Not reachable through the `// test` scanner, which stops at `end`.
    assert!(run_program("if yes then\nexpect 1 to be 1\nend").is_ok());

    let err = run_program("if yes then\nexpect 1 to be 2\nend")
        .expect_err("expect inside a taken if-branch must fail");
    assert!(
        err.to_string().contains('1') && err.to_string().contains('2'),
        "failure must name both values, got: {}",
        err
    );

    run_program("if no then\nexpect 1 to be 2\nend")
        .expect("a skipped branch must not evaluate its expect");
}

#[test]
fn edge_a_loop_evaluates_every_expect() {
    let err = run_program("for each i in [1, 2, 3]\nexpect i to be 2\nend")
        .expect_err("the first mismatching iteration must fail");
    assert!(
        err.to_string().contains('1'),
        "the failing iteration's value must be named, got: {}",
        err
    );
}

// ---------------------------------------------------------------------------
// Duplicate and missing keys
// ---------------------------------------------------------------------------

#[test]
fn edge_missing_record_key_differs_from_present_key() {
    assert_fails("expect {a: 1} to be {a: 1, b: 2}");
    assert_fails("expect {a: 1, b: 2} to be {a: 1}");

    // Value::Record is a HashMap, so its rendering order is not stable. Assert
    // only on which keys appear, never on their order.
    let error = assert_fails("expect {a: 1} to be {a: 1, b: 2}");
    assert!(
        error.message.contains("a") && error.message.contains("b"),
        "both records must be named in the message, got: {}",
        error.message
    );
}

// ---------------------------------------------------------------------------
// Malformed input
// ---------------------------------------------------------------------------

#[test]
fn edge_malformed_expect_is_a_parse_error() {
    for body in ["expect", "expect 1 to", "expect 1 to be", "expect to be 1"] {
        let error = assert_fails(body);
        assert!(
            error.message.contains("ParserError"),
            "`{}` must be rejected as a parser error, got: {}",
            body,
            error.message
        );
        assert!(
            error.expected.is_none(),
            "`{}` is a parse error, not a value mismatch",
            body
        );
    }
}

#[test]
fn edge_an_empty_test_body_still_passes() {
    // Documents the known gap: a discovered test with no assertion is reported
    // as a pass. Tracked in FINDINGS.md, not silently "fixed" here.
    let mut harness = TestHarness::new();
    harness
        .run_source("// test \"no assertions\"\n// end\n")
        .expect("scan should succeed");
    let results = harness.results();
    assert_eq!(results.passed, 1);
    assert_eq!(results.failed, 0);
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn edge_expect_results_are_deterministic_across_runs() {
    let body = "set total to 0\nexpect total to be 99";
    let first = run(body);
    for _ in 0..4 {
        let again = run(body);
        assert_eq!(first.passed, again.passed);
        assert_eq!(first.failed, again.failed);
        assert_eq!(
            failure_messages(&first),
            failure_messages(&again),
            "repeated runs must produce identical failure messages"
        );
    }
    assert_eq!(first.failed, 1);
}
