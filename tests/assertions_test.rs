//! Every public assertion in `src/testing/assertions.rs` must be reachable
//! from a test that can fail, in both directions.
//!
//! The list of assertions is read out of the source file at compile time, so
//! adding a `pub fn assert_*` without adding a probe here fails this file
//! rather than quietly shipping an assertion nothing exercises.

use redblue::testing::assertions::{
    assert_list_length, assert_number_in_range, assert_text_contains, assert_text_matches,
    assert_throws, assert_value_is_list, assert_value_is_number, assert_value_is_record,
    assert_value_is_text, assert_value_is_yes_no, assert_values_equal,
};
use redblue::Value;

const ASSERTION_SOURCE: &str = include_str!("../src/testing/assertions.rs");

/// A probe exercises one assertion in both directions: it must succeed on a
/// correct input and fail on a wrong one. Returning `Err` means the assertion
/// accepted something it should have rejected (or vice versa), so every probe
/// can fail.
type Probe = fn() -> Result<(), String>;

/// A type assertion from the library: `assert_value_is_number` and its
/// siblings, all of the same shape.
type TypeAssertion = fn(&Value) -> Result<(), redblue::testing::assertions::TestAssertionError>;

/// Decides whether a value is of the type its assertion accepts.
type Accepts = fn(&Value) -> bool;

/// One probe per surviving assertion. Keep sorted; `every_public_assertion_has
/// _a_probe` enforces that this table and the source agree exactly.
const PROBES: &[(&str, Probe)] = &[
    ("assert_list_length", probe_assert_list_length),
    ("assert_number_in_range", probe_assert_number_in_range),
    ("assert_text_contains", probe_assert_text_contains),
    ("assert_text_matches", probe_assert_text_matches),
    ("assert_throws", probe_assert_throws),
    ("assert_value_is_list", probe_assert_value_is_list),
    ("assert_value_is_number", probe_assert_value_is_number),
    ("assert_value_is_record", probe_assert_value_is_record),
    ("assert_value_is_text", probe_assert_value_is_text),
    ("assert_value_is_yes_no", probe_assert_value_is_yes_no),
    ("assert_values_equal", probe_assert_values_equal),
];

fn number(n: f64) -> Value {
    Value::Number(n)
}

fn text(s: &str) -> Value {
    Value::Text(s.to_string())
}

fn list(items: Vec<Value>) -> Value {
    Value::list(items)
}

fn record() -> Value {
    let mut fields = indexmap::IndexMap::new();
    fields.insert("a".to_string(), number(1.0));
    Value::record(fields)
}

fn ok(result: Result<(), redblue::testing::assertions::TestAssertionError>) -> Result<(), String> {
    result.map_err(|failure| format!("accepted a wrong input: {}", failure))
}

fn fails(
    result: Result<(), redblue::testing::assertions::TestAssertionError>,
) -> Result<(), String> {
    match result {
        Ok(()) => Err("accepted a wrong input".to_string()),
        Err(failure) if failure.message.is_empty() => {
            Err("rejected a wrong input but with no message".to_string())
        }
        Err(_) => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// Probes: one per surviving assertion, both directions.
// ---------------------------------------------------------------------------

fn probe_assert_list_length() -> Result<(), String> {
    ok(assert_list_length(&list(vec![number(1.0), number(2.0)]), 2))?;
    fails(assert_list_length(&list(vec![number(1.0)]), 2))?;
    fails(assert_list_length(&text("not a list"), 0))
}

fn probe_assert_number_in_range() -> Result<(), String> {
    ok(assert_number_in_range(5.0, 1.0, 10.0))?;
    fails(assert_number_in_range(50.0, 1.0, 10.0))
}

fn probe_assert_text_contains() -> Result<(), String> {
    ok(assert_text_contains("hello world", "world"))?;
    fails(assert_text_contains("hello world", "goodbye"))
}

fn probe_assert_text_matches() -> Result<(), String> {
    ok(assert_text_matches("abc123", r"\d+"))?;
    fails(assert_text_matches("abc", r"\d+"))
}

fn probe_assert_throws() -> Result<(), String> {
    ok(assert_throws(|| panic!("probe")))?;
    fails(assert_throws(|| {}))
}

fn probe_assert_value_is_list() -> Result<(), String> {
    ok(assert_value_is_list(&list(vec![])))?;
    fails(assert_value_is_list(&number(1.0)))
}

fn probe_assert_value_is_number() -> Result<(), String> {
    ok(assert_value_is_number(&number(1.0)))?;
    fails(assert_value_is_number(&text("1")))
}

fn probe_assert_value_is_record() -> Result<(), String> {
    ok(assert_value_is_record(&record()))?;
    fails(assert_value_is_record(&list(vec![])))
}

fn probe_assert_value_is_text() -> Result<(), String> {
    ok(assert_value_is_text(&text("hi")))?;
    fails(assert_value_is_text(&Value::Nothing))
}

fn probe_assert_value_is_yes_no() -> Result<(), String> {
    ok(assert_value_is_yes_no(&Value::YesNo(true)))?;
    fails(assert_value_is_yes_no(&number(1.0)))
}

fn probe_assert_values_equal() -> Result<(), String> {
    ok(assert_values_equal(&number(2.0), &number(2.0)))?;
    fails(assert_values_equal(&number(2.0), &number(1.0)))
}

/// Every `pub fn assert_*` declared in `source`, in source order, with any
/// generic parameter list removed.
fn public_assertion_names(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("pub fn assert_")?;
            let name: String = rest
                .chars()
                .take_while(|c| *c != '(' && *c != '<')
                .collect();
            if name.is_empty() {
                None
            } else {
                Some(format!("assert_{name}"))
            }
        })
        .collect()
}

#[test]
fn every_public_assertion_has_a_probe() {
    let declared = public_assertion_names(ASSERTION_SOURCE);
    let probed: Vec<&str> = PROBES.iter().map(|(name, _)| *name).collect();

    let mut missing: Vec<&str> = declared
        .iter()
        .map(String::as_str)
        .filter(|name| !probed.contains(name))
        .collect();
    missing.sort_unstable();
    assert!(
        missing.is_empty(),
        "every `pub fn assert_*` in src/testing/assertions.rs needs a probe in \
         tests/assertions_test.rs; untested: {:?} (declared: {:?})",
        missing,
        declared
    );

    let mut stale: Vec<&str> = probed
        .iter()
        .filter(|name| !declared.iter().any(|d| d == *name))
        .copied()
        .collect();
    stale.sort_unstable();
    assert!(
        stale.is_empty(),
        "tests/assertions_test.rs probes assertions that no longer exist: {:?}",
        stale
    );
}

#[test]
fn every_surviving_assertion_works_in_both_directions() {
    for (name, probe) in PROBES {
        match probe() {
            Ok(()) => println!("{name}: covered (accepts correct input, rejects wrong input)"),
            Err(reason) => panic!("{name} is not correctly implemented: {reason}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Empty / singleton
// ---------------------------------------------------------------------------

#[test]
fn edge_empty_and_singleton_inputs() {
    // Empty list of length 0, empty text, and the nothing value are all
    // distinguishable from their non-empty neighbours.
    ok(assert_list_length(&list(vec![]), 0)).unwrap();
    fails(assert_list_length(&list(vec![]), 1)).unwrap();

    ok(assert_text_contains("", "")).unwrap();
    fails(assert_text_contains("", "a")).unwrap();

    ok(assert_text_matches("", r"^$")).unwrap();
    fails(assert_text_matches("", r"\d")).unwrap();

    ok(assert_number_in_range(0.0, 0.0, 0.0)).unwrap();
    fails(assert_number_in_range(0.0, -1.0, -0.5)).unwrap();

    // A singleton is the smallest list the length assertion must accept.
    ok(assert_list_length(&list(vec![number(1.0)]), 1)).unwrap();
    ok(assert_values_equal(
        &list(vec![number(1.0)]),
        &list(vec![number(1.0)]),
    ))
    .unwrap();
    fails(assert_values_equal(&list(vec![number(1.0)]), &list(vec![]))).unwrap();
}

// ---------------------------------------------------------------------------
// Numeric boundary
// ---------------------------------------------------------------------------

#[test]
fn edge_numeric_boundaries() {
    // The range is inclusive at both ends.
    ok(assert_number_in_range(1.0, 1.0, 10.0)).unwrap();
    ok(assert_number_in_range(10.0, 1.0, 10.0)).unwrap();
    fails(assert_number_in_range(f64::NAN, 0.0, 1.0)).unwrap();
    fails(assert_number_in_range(
        f64::INFINITY,
        f64::NEG_INFINITY,
        1.0,
    ))
    .unwrap();

    // 2^53 and 2^53 + 2 are the neighbouring doubles at that magnitude, and
    // 2^53 + 1 is not representable at all, so equality is exact about the
    // number it can actually hold.
    ok(assert_values_equal(
        &number(9_007_199_254_740_992.0),
        &number(9_007_199_254_740_992.0),
    ))
    .unwrap();
    fails(assert_values_equal(
        &number(9_007_199_254_740_992.0),
        &number(9_007_199_254_740_994.0),
    ))
    .unwrap();
}

// ---------------------------------------------------------------------------
// Type mismatch
// ---------------------------------------------------------------------------

#[test]
fn edge_type_mismatch_is_rejected_by_every_type_assertion() {
    // Every value Redblue can hold, plus the builtins. Each type assertion
    // must reject all but its own variant, and each rejection must name what
    // it wanted.
    let values = [
        Value::Nothing,
        number(1.0),
        text("1"),
        list(vec![]),
        Value::YesNo(true),
        record(),
    ];

    let assertions: [(&str, TypeAssertion, Accepts); 5] = [
        ("number", assert_value_is_number, |v| {
            matches!(v, Value::Number(_))
        }),
        ("text", assert_value_is_text, |v| {
            matches!(v, Value::Text(_))
        }),
        ("list", assert_value_is_list, |v| {
            matches!(v, Value::List(_))
        }),
        ("yes/no", assert_value_is_yes_no, |v| {
            matches!(v, Value::YesNo(_))
        }),
        ("record", assert_value_is_record, |v| {
            matches!(v, Value::Record(_))
        }),
    ];

    for (kind, assertion, accepts) in assertions {
        for value in &values {
            let shown = format!("{:?}", value);
            if accepts(value) {
                assert!(
                    assertion(value).is_ok(),
                    "{kind} assertion must accept {shown}"
                );
                continue;
            }
            let failure =
                assertion(value).expect_err(&format!("{shown} must not be accepted as {kind}"));
            assert!(
                !failure.message.is_empty(),
                "{} must produce a named failure, got an empty message",
                shown
            );
            assert!(
                failure.expected.is_some() && failure.actual.is_some(),
                "{} must record what it wanted and what it got, got {:?}",
                shown,
                (failure.expected, failure.actual)
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Unicode
// ---------------------------------------------------------------------------

#[test]
fn edge_unicode_text_is_matched_by_bytes_of_the_real_string() {
    ok(assert_text_contains("héllo 🎉 日本語", "🎉")).unwrap();
    ok(assert_text_contains("日本語", "本")).unwrap();
    // A substring that spans no character boundary must not match.
    fails(assert_text_contains("日本語", "本x語")).unwrap();
    // Precomposed and decomposed accents are distinct byte sequences.
    ok(assert_text_contains("\u{e9}", "\u{e9}")).unwrap();
    fails(assert_text_contains("\u{65}\u{301}", "\u{e9}")).unwrap();
    fails(assert_text_matches("שלום 12", r"\p{Greek}")).unwrap();
    ok(assert_text_matches("שלום 12", r"\d$")).unwrap();
}

// ---------------------------------------------------------------------------
// Malformed input
// ---------------------------------------------------------------------------

#[test]
fn edge_malformed_pattern_is_reported_as_a_failure_not_a_panic() {
    // An unparseable regex must come back as Err, never as Ok and never as a
    // panic. It must also be told *apart* from a pattern that compiled and did
    // not match: both are Err, and a caller that reads only "Err" would sit
    // there passing over a string it never matched at all.
    let failure = assert_text_matches("anything", "[unclosed")
        .expect_err("an invalid pattern must not be treated as a match");
    assert!(
        failure.message.contains("[unclosed"),
        "the failure must name the pattern, got: {}",
        failure.message
    );
    assert_eq!(failure.actual.as_deref(), Some("anything"));
    assert!(
        !failure.message.contains("does not match"),
        "a pattern that will not compile never matched anything, so it must not \
         be reported as a non-match, got: {}",
        failure.message
    );

    let non_match = assert_text_matches("anything", r"\d+")
        .expect_err("a valid pattern that does not match must fail");
    assert!(
        failure.message != non_match.message,
        "a pattern that will not compile and a pattern that did not match must \
         not report the same failure, both said: {}",
        failure.message
    );
    assert!(
        non_match.message.contains("does not match"),
        "a compiled pattern that did not match says so, got: {}",
        non_match.message
    );
    assert_eq!(
        non_match.expected.as_deref(),
        Some("Matches '\\d+'"),
        "only a compiled pattern promises a match"
    );
}

/// The difference is not only in the wording: the two failures record different
/// expectations, so a caller inspecting `expected` can still tell them apart.
#[test]
fn edge_an_invalid_pattern_and_a_non_match_record_different_expectations() {
    let invalid = assert_text_matches("abc", "[unclosed").expect_err("[unclosed cannot compile");
    let non_match = assert_text_matches("abc", r"\d").expect_err("abc has no digit");

    assert_ne!(
        invalid.expected, non_match.expected,
        "an unparseable pattern must not promise the caller a match"
    );
    assert!(
        invalid
            .expected
            .as_deref()
            .is_some_and(|expected| expected.contains("A valid expression")),
        "an invalid pattern must say what a valid one would have been, got {:?}",
        invalid.expected
    );
    assert!(!invalid.message.contains("does not match"));
}

// ---------------------------------------------------------------------------
// Failure reporting: every assertion's message is usable on its own.
// ---------------------------------------------------------------------------

#[test]
fn every_rejection_names_the_failure() {
    let rejections: Vec<(&str, redblue::testing::assertions::TestAssertionError)> = vec![
        (
            "assert_values_equal",
            assert_values_equal(&number(2.0), &number(1.0)).unwrap_err(),
        ),
        (
            "assert_value_is_number",
            assert_value_is_number(&text("1")).unwrap_err(),
        ),
        (
            "assert_list_length",
            assert_list_length(&list(vec![number(1.0)]), 2).unwrap_err(),
        ),
        (
            "assert_text_contains",
            assert_text_contains("hello", "goodbye").unwrap_err(),
        ),
        (
            "assert_text_matches",
            assert_text_matches("abc", r"\d").unwrap_err(),
        ),
        (
            "assert_number_in_range",
            assert_number_in_range(50.0, 1.0, 10.0).unwrap_err(),
        ),
        ("assert_throws", assert_throws(|| {}).unwrap_err()),
    ];

    for (name, failure) in rejections {
        assert!(!failure.message.is_empty(), "{} left no message", name);
        let rendered = format!("{failure:?}");
        assert!(
            rendered.starts_with(&failure.message),
            "{} must render its message first, got: {}",
            name,
            rendered
        );
        assert!(
            !failure.to_string().is_empty(),
            "{} must render as Display",
            name
        );
    }
}

// ---------------------------------------------------------------------------
// The `expect` builtin reports through this library.
// ---------------------------------------------------------------------------

#[test]
fn redblue_expect_failure_message_is_built_by_the_assertion_library() {
    let error = redblue::run_source("expect 1 to be 2")
        .expect_err("a mismatched expect must be an error, not a pass");

    let built = assert_values_equal(&Value::Number(2.0), &Value::Number(1.0))
        .expect_err("1 and 2 are not equal")
        .to_string();

    assert!(
        error.to_string().contains(&built),
        "the interpreter's expect message must come from the assertion library.\n\
         library builds: {}\ninterpreter printed: {}",
        built,
        error
    );
}
