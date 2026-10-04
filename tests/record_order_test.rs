//! Record field ordering must be deterministic.
//!
//! Records are built by inserting fields in source order. The value that comes
//! back out — via `say` (`Value::Display`) or `json.stringify` — must preserve
//! that insertion order, otherwise the same program prints different output on
//! every run.

use redblue::Error;

/// Runs `source` through lexer → parser → VM and returns the value of its last
/// statement.
#[track_caller]
fn eval(source: &str) -> redblue::Value {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    let ast = redblue::parser::parse(tokens).expect("source should parse");
    let mut vm = redblue::Vm::new();
    vm.run(&ast).expect("source should run")
}

/// Runs `source` and returns the pipeline error it produced.
#[track_caller]
fn eval_err(source: &str) -> Error {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    let ast = redblue::parser::parse(tokens).expect("source should parse");
    let mut vm = redblue::Vm::new();
    vm.run(&ast).expect_err("source should have failed")
}

/// The same 20 keys, inserted in this exact order.
const KEYS: [&str; 20] = [
    "zebra", "alpha", "mango", "beta", "omega", "delta", "sigma", "kappa", "gamma", "theta",
    "epsilon", "eta", "iota", "lambda", "mu", "nu", "xi", "omicron", "pi", "rho",
];

/// `{zebra: 0, alpha: 1, ...}` — a literal whose fields are in `KEYS` order.
fn twenty_key_record() -> String {
    let fields: Vec<String> = KEYS
        .iter()
        .enumerate()
        .map(|(i, k)| format!("{}: {}", k, i))
        .collect();
    format!("{{{}}}", fields.join(", "))
}

/// The display and JSON forms `twenty_key_record` must produce.
fn expected_twenty() -> String {
    let fields: Vec<String> = KEYS
        .iter()
        .enumerate()
        .map(|(i, k)| format!("{}: {}", k, i))
        .collect();
    format!("{{{}}}", fields.join(", "))
}

fn expected_twenty_json() -> String {
    let fields: Vec<String> = KEYS
        .iter()
        .enumerate()
        .map(|(i, k)| format!("\"{}\": {}", k, i))
        .collect();
    format!("{{{}}}", fields.join(", "))
}

// ---------------------------------------------------------------------------
// Ordering
// ---------------------------------------------------------------------------

#[test]
fn test_record_display_preserves_insertion_order() {
    let source = format!("set r to {}\nr", twenty_key_record());
    let value = eval(&source);

    assert_eq!(
        value.to_string(),
        expected_twenty(),
        "record display must list fields in insertion order"
    );
}

#[test]
fn test_json_stringify_preserves_insertion_order() {
    let source = format!("set r to {}\njson.stringify(r)", twenty_key_record());
    let value = eval(&source);

    assert_eq!(
        value.to_string(),
        expected_twenty_json(),
        "json.stringify must list fields in insertion order"
    );
}

/// Definition of done: a 20-key record prints identically 50 times in a row.
#[test]
fn edge_twenty_key_record_is_identical_across_fifty_runs() {
    let source = format!("set r to {}\nr", twenty_key_record());
    let first = eval(&source).to_string();

    for run in 0..50 {
        let again = eval(&source).to_string();
        assert_eq!(
            again,
            first,
            "run {} of 50 printed a different field order",
            run + 1
        );
    }

    assert_eq!(
        first,
        expected_twenty(),
        "the shared output must be ordered"
    );
}

#[test]
fn test_json_parse_preserves_source_order() {
    let source = r#"json.parse("{\"zebra\": 0, \"alpha\": 1, \"mango\": 2}")"#;
    let value = eval(source);

    assert_eq!(
        value.to_string(),
        "{zebra: 0, alpha: 1, mango: 2}",
        "a parsed JSON object must keep the order of its keys in the document"
    );
}

#[test]
fn test_json_stringify_round_trips_through_parse() {
    let source = format!(
        "set r to {}\nset text to json.stringify(r)\njson.parse(text)",
        twenty_key_record()
    );
    let round_tripped = eval(&source);
    let original = eval(&format!("set r to {}\nr", twenty_key_record()));

    assert_eq!(
        round_tripped.to_string(),
        original.to_string(),
        "stringify then parse must reproduce the record, order included"
    );
    assert_eq!(
        round_tripped.to_string(),
        expected_twenty(),
        "the round-tripped record must still be in insertion order"
    );
}

// ---------------------------------------------------------------------------
// Empty / singleton / boundary
// ---------------------------------------------------------------------------

#[test]
fn test_empty_record_displays_as_empty_braces() {
    let value = eval("set r to {}\nr");

    assert_eq!(
        value.to_string(),
        "{}",
        "an empty record must display as empty braces"
    );
    assert_eq!(
        eval("set r to {}\njson.stringify(r)").to_string(),
        "{}",
        "an empty record must stringify to empty braces"
    );
    assert_eq!(
        eval("json.parse(\"{}\")").to_string(),
        "{}",
        "an empty JSON object must parse to an empty record"
    );
}

#[test]
fn test_singleton_record_displays_its_only_field() {
    assert_eq!(
        eval("set r to {only: 7}\nr").to_string(),
        "{only: 7}",
        "a one-field record must display that field"
    );
    assert_eq!(
        eval("set r to {only: 7}\njson.stringify(r)").to_string(),
        "{\"only\": 7}",
        "a one-field record must stringify to one pair"
    );
}

#[test]
fn test_first_and_last_field_are_reachable_in_order() {
    let display = eval(&format!("set r to {}\nr", twenty_key_record())).to_string();

    assert!(
        display.starts_with("{zebra: 0, "),
        "the head of the display must be the first-inserted field, got {}",
        display
    );
    assert!(
        display.ends_with(", rho: 19}"),
        "the tail of the display must be the last-inserted field, got {}",
        display
    );
    assert_eq!(
        eval(&format!("set r to {}\nr.zebra", twenty_key_record())).to_string(),
        "0",
        "the first-inserted field must be readable"
    );
    assert_eq!(
        eval(&format!("set r to {}\nr.rho", twenty_key_record())).to_string(),
        "19",
        "the last-inserted field must be readable"
    );
}

// ---------------------------------------------------------------------------
// Duplicate / missing keys
// ---------------------------------------------------------------------------

#[test]
fn test_duplicate_key_takes_the_last_value_and_the_first_position() {
    assert_eq!(
        eval("set r to {a: 1, b: 2, a: 3}\nr").to_string(),
        "{a: 3, b: 2}",
        "a repeated key must win with its last value, at its first position"
    );
    assert_eq!(
        eval("set r to {b: 2, a: 1, b: 9, a: 8}\nr").to_string(),
        "{b: 9, a: 8}",
        "every repeated key must resolve to its last value"
    );
    assert_eq!(
        eval("set r to {a: 1, a: 2, a: 3}\nr").to_string(),
        "{a: 3}",
        "a key repeated three times must collapse to one field"
    );
}

#[test]
fn test_duplicate_key_in_json_takes_the_last_value() {
    assert_eq!(
        eval("json.parse(\"{\\\"a\\\": 1, \\\"b\\\": 2, \\\"a\\\": 3}\")").to_string(),
        "{a: 3, b: 2}",
        "a duplicate JSON key must resolve to its last value"
    );
}

#[test]
fn test_missing_key_reads_as_nothing_and_does_not_grow_the_record() {
    assert_eq!(
        eval("set r to {a: 1}\nr.missing").to_string(),
        "nothing",
        "a missing field must read as nothing"
    );
    assert_eq!(
        eval("set r to {a: 1, b: 2}\nr.missing").to_string(),
        "nothing",
        "a missing field in a two-field record must read as nothing"
    );
    assert_eq!(
        eval("set r to {a: 1}\nr").to_string(),
        "{a: 1}",
        "reading a missing field must not add a field to the record"
    );
}

#[test]
fn test_set_property_updates_in_place_without_reordering() {
    let source = "set r to {first: 1, second: 2, third: 3}\nset r.second to 20\nr";
    assert_eq!(
        eval(source).to_string(),
        "{first: 1, second: 20, third: 3}",
        "updating an existing field must keep it in its original position"
    );

    let adding = "set r to {first: 1}\nset r.second to 2\nr";
    assert_eq!(
        eval(adding).to_string(),
        "{first: 1, second: 2}",
        "a newly added field must go at the end"
    );
}

// ---------------------------------------------------------------------------
// Nesting
// ---------------------------------------------------------------------------

#[test]
fn test_nested_records_preserve_order_at_every_level() {
    let source = "set r to {zebra: {yak: 1, xerus: 2}, alpha: {yak: 3, xerus: 4}}\nr";
    assert_eq!(
        eval(source).to_string(),
        "{zebra: {yak: 1, xerus: 2}, alpha: {yak: 3, xerus: 4}}",
        "each nested record must keep its own insertion order"
    );
    assert_eq!(
        eval("set r to {zebra: {yak: 1, xerus: 2}, alpha: {yak: 3, xerus: 4}}\njson.stringify(r)")
            .to_string(),
        "{\"zebra\": {\"yak\": 1, \"xerus\": 2}, \"alpha\": {\"yak\": 3, \"xerus\": 4}}",
        "json.stringify must order nested records too"
    );
}

#[test]
fn test_deeply_nested_records_preserve_order() {
    let source = "set r to {b: {d: {f: 1, e: 2}, c: 3}, a: 4, cc: 5}\nr";
    assert_eq!(
        eval(source).to_string(),
        "{b: {d: {f: 1, e: 2}, c: 3}, a: 4, cc: 5}",
        "three levels of nesting must each keep insertion order"
    );
}

#[test]
fn test_records_inside_lists_preserve_order() {
    let source = "set r to [{zebra: 1, alpha: 2}, {yak: 3, xerus: 4}]\nr";
    assert_eq!(
        eval(source).to_string(),
        "[{zebra: 1, alpha: 2}, {yak: 3, xerus: 4}]",
        "records nested in a list must keep insertion order"
    );
}

// ---------------------------------------------------------------------------
// Unicode keys and values
// ---------------------------------------------------------------------------

#[test]
fn test_unicode_keys_and_values_preserve_order() {
    let source = "set r to {zebra: \"\u{1f600}\", alpha: \"\u{5bff}\", mango: \"\u{5e2}\", beta: \"caf\u{e9}\"}\nr";
    assert_eq!(
        eval(source).to_string(),
        "{zebra: \u{1f600}, alpha: \u{5bff}, mango: \u{5e2}, beta: caf\u{e9}}",
        "emoji, CJK, RTL and combining-mark values must not disturb key order"
    );
}

#[test]
fn test_unicode_keys_keep_insertion_order() {
    // Keys are identifiers, so the ordering is checked on the values that the
    // identifier keys carry; a decomposed key sorts apart from its composed form
    // only if ordering is not insertion-based.
    let source = "set r to {caf\u{e9}: 1, cafe: 2, zebra: 3, alpha: 4}\nr";
    assert_eq!(
        eval(source).to_string(),
        "{caf\u{e9}: 1, cafe: 2, zebra: 3, alpha: 4}",
        "combining-mark and ASCII keys must stay in insertion order"
    );
}

// ---------------------------------------------------------------------------
// Numeric boundaries inside records
// ---------------------------------------------------------------------------

#[test]
fn test_numeric_boundary_values_keep_field_order() {
    let source = "set r to {zero: 0, negzero: -0.0, big: 9007199254740993, small: 0.1, neg: -9007199254740993}\nr";
    let value = eval(source);

    assert_eq!(
        value.to_string(),
        "{zero: 0, negzero: 0, big: 9007199254740992, small: 0.1, neg: -9007199254740992}",
        "field order must be insertion order regardless of how each number prints"
    );
    assert_eq!(
        eval(&format!("{}\njson.stringify(r)", source)).to_string(),
        "{\"zero\": 0, \"negzero\": 0, \"big\": 9007199254740992, \"small\": 0.1, \"neg\": -9007199254740992}",
        "json.stringify must order fields the same way for boundary numbers"
    );
}

// ---------------------------------------------------------------------------
// Failure paths — these must be errors, never a wrong-but-plausible order
// ---------------------------------------------------------------------------

#[test]
fn test_property_access_on_a_list_is_a_runtime_error() {
    let err = eval_err("set r to [1, 2, 3]\nr.alpha");

    assert!(
        matches!(&err, Error::Runtime(m) if m.contains("Cannot access property")),
        "expected a Runtime error about property access, got {:?}",
        err
    );
}

#[test]
fn test_malformed_json_object_is_an_error_not_a_reordered_record() {
    let err = eval_err("json.parse(\"{\\\"a\\\": 1\")");

    assert!(
        matches!(&err, Error::Runtime(_)),
        "an unterminated JSON object must be a Runtime error, got {:?}",
        err
    );
}

#[test]
fn test_record_literal_missing_colon_is_a_parse_error() {
    let tokens = redblue::lexer::Lexer::tokenize("set r to {a 1}").expect("should lex");
    let err = redblue::parser::parse(tokens).expect_err("`{a 1}` must not parse");

    assert!(
        matches!(&err, Error::Parser(_)),
        "a record field without a colon must be a Parser error, got {:?}",
        err
    );
}

// ---------------------------------------------------------------------------
// Equality is still order-insensitive
// ---------------------------------------------------------------------------

/// Runs one `// test "name"` block through the Redblue test harness and reports
/// whether it passed.
fn redblue_test_passes(body: &str) -> bool {
    let mut harness = redblue::testing::TestHarness::new();
    harness
        .run_source(&format!("// test \"record order\"\n{}\n// end\n", body))
        .expect("harness scan should not fail");
    let results = harness.results();
    assert_eq!(results.passed + results.failed, 1, "exactly one test ran");
    if results.failed > 0 {
        println!("failure: {:?}", results.errors);
    }
    results.failed == 0
}

#[test]
fn test_record_equality_ignores_field_order() {
    // Display order is now insertion order, but two records holding the same
    // fields must still compare equal regardless of the order they were built
    // in — `expect` relies on that.
    assert!(
        redblue_test_passes(
            "    set a to {x: 1, y: 2}\n    set b to {y: 2, x: 1}\n    expect a to be b"
        ),
        "records with the same fields in a different order must compare equal"
    );
}

#[test]
fn test_expect_on_a_record_compares_field_values() {
    assert!(
        !redblue_test_passes(
            "    set a to {x: 1, y: 2}\n    set b to {x: 1, z: 2}\n    expect a to be b"
        ),
        "records with different field names must not compare equal"
    );
    assert!(
        !redblue_test_passes(
            "    set a to {x: 1, y: 2}\n    set b to {x: 1, y: 9}\n    expect a to be b"
        ),
        "`expect` must fail when a record field value differs, regardless of order"
    );
}
