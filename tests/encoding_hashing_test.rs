//! `hex`, `base64`, `hash` and `uuid`: the encodings and digests a program
//! assumes a language has.
//!
//! Every value crosses the real language boundary through
//! [`redblue::run_source_value`], so these tests pin the Redblue surface rather
//! than a Rust helper. A byte is the existing `bytes` representation: a list of
//! whole numbers from 0 to 255.

use redblue::{Error, Value};

#[track_caller]
fn eval(source: &str) -> Value {
    redblue::run_source_value(source)
        .unwrap_or_else(|error| panic!("should have run, failed with {error:?}\n{source}"))
}

#[track_caller]
fn eval_err(source: &str) -> Error {
    redblue::run_source(source).expect_err("source should have failed")
}

fn text(value: &str) -> Value {
    Value::Text(value.to_string())
}

fn bytes(values: &[u8]) -> Value {
    Value::list(values.iter().map(|b| Value::Number(*b as f64)).collect())
}

fn list_literal(values: &[u8]) -> String {
    let mut source = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            source.push_str(", ");
        }
        source.push_str(&value.to_string());
    }
    source.push(']');
    source
}

fn runtime_message(error: &Error) -> &str {
    match error {
        Error::Runtime(message, _) => message,
        other => panic!("expected a Runtime error, got {other:?}"),
    }
}

/// Hex and base64 are byte encodings, so the round trip is asserted on the
/// bytes, not on a text that happens to survive one example.
#[test]
fn edge_hex_and_base64_round_trip_all_256_byte_values() {
    let all: Vec<u8> = (0..=255).collect();
    let literal = list_literal(&all);

    let hex_round_trip = eval(&format!("hex.decode(hex.encode({literal}))"));
    assert_eq!(
        hex_round_trip,
        bytes(&all),
        "hex did not round-trip 0..=255"
    );

    let base64_round_trip = eval(&format!("base64.decode(base64.encode({literal}))"));
    assert_eq!(
        base64_round_trip,
        bytes(&all),
        "base64 did not round-trip 0..=255"
    );
}

/// Both encodings carry UTF-8 text through unchanged: the byte list is the
/// encoding's subject, and rebuilding the text must give back every character.
#[test]
fn edge_unicode_text_survives_hex_and_base64() {
    for encoded in ["hex", "base64"] {
        let value = eval(&format!(
            r#"set original to "héllo 🌍 日本語 مرحبا e\u0301"
set bytes to bytes.from_text(original)
set text to bytes.text({encoded}.decode({encoded}.encode(bytes)))
[original, text]"#
        ));
        let Value::List(items) = value else {
            panic!("expected [original, text], got {value:?}");
        };
        assert_eq!(items[0], items[1], "{encoded} changed a UTF-8 text");
    }
}

/// Padding is part of the base64 text a program sends, and the decoder accepts
/// the same bytes with or without it.
#[test]
fn edge_base64_uses_canonical_padding_and_decodes_padded_or_unpadded() {
    assert_eq!(eval(r#"base64.encode(bytes.from_text("h"))"#), text("aA=="));
    assert_eq!(
        eval(r#"base64.encode(bytes.from_text("hi"))"#),
        text("aGk=")
    );
    assert_eq!(
        eval(r#"base64.encode(bytes.from_text("hi!"))"#),
        text("aGkh")
    );
    assert_eq!(eval(r#"base64.encode(bytes.from_text(""))"#), text(""));
    assert_eq!(eval(r#"base64.decode("aGk=")"#), bytes(&[104, 105]));
    assert_eq!(eval(r#"base64.decode("aGk")"#), bytes(&[104, 105]));
    assert_eq!(eval(r#"base64.decode("")"#), bytes(&[]));
}

/// A single byte is the boundary between an empty payload and a padded one, and
/// hex has literal answers for both ends of the byte range.
#[test]
fn edge_hex_single_byte_empty_and_uppercase_inputs() {
    assert_eq!(eval(r#"hex.encode(bytes.from_text("Hi"))"#), text("4869"));
    assert_eq!(eval(r#"hex.decode("4869")"#), bytes(&[72, 105]));
    assert_eq!(eval(r#"hex.decode("486A")"#), bytes(&[72, 106]));
    assert_eq!(eval(r#"hex.decode("")"#), bytes(&[]));
    assert_eq!(eval("hex.encode([0])"), text("00"));
    assert_eq!(eval("hex.encode([255])"), text("ff"));
}

/// A decoder that silently skips bad input is how corrupt data reaches a
/// program unnoticed. Both decoders name the position of the first bad character.
#[test]
fn edge_invalid_hex_and_base64_name_the_offending_position() {
    let cases = [
        (r#"hex.decode("48 69")"#, "position 2"),
        (r#"hex.decode("486")"#, "position 3"),
        (r#"hex.decode("zz")"#, "position 0"),
        (r#"base64.decode("aGk*")"#, "position 3"),
        (r#"base64.decode("aGk=\n")"#, "position 4"),
    ];

    for (source, position) in cases {
        let error = eval_err(source);
        let message = runtime_message(&error);
        assert!(
            message.contains(position),
            "{source} was refused as `{message}`, which does not name `{position}`"
        );
    }

    // An odd-length hex string names the digit that has no pair, not only the
    // end of the text: `486` is a refusal at the 6, which is position 2.
    let error = eval_err(r#"hex.decode("486")"#);
    let message = runtime_message(&error);
    assert!(
        message.contains("position 2"),
        "the unpaired digit was not named in `{message}`"
    );

    let error = eval_err(r#"base64.decode("é")"#);
    let message = runtime_message(&error);
    assert!(
        message.contains("invalid byte 'é'") && message.contains("position 0"),
        "a non-ASCII byte was rendered as `{message}`, not as the character the \
         program wrote"
    );
}

/// A decoder that stops at the first bad character has silently dropped every
/// byte after it. A valid prefix followed by garbage is still a refusal at the
/// tail, never a shorter byte list that looks like success.
#[test]
fn edge_decoders_do_not_answer_a_valid_prefix_when_the_tail_is_bad() {
    for (source, position) in [
        (r#"hex.decode("4869zz")"#, "position 4"),
        (r#"base64.decode("aGk!!")"#, "position 3"),
        (r#"base64.decode("aGk!!!")"#, "position 3"),
    ] {
        let error = eval_err(source);
        let message = runtime_message(&error);
        assert!(
            message.contains(position),
            "{source} was refused as `{message}`, which does not name `{position}`"
        );
    }
}

/// A decoding error is a normal Redblue failure, not a process abort: `try`
/// catches it and the handler sees the message.
#[test]
fn edge_decoding_errors_are_catchable() {
    let value = eval(
        r#"set caught to "not caught"
try
    hex.decode("zz")
catch error of Runtime problem
    set caught to problem.message
end
caught"#,
    );
    let Value::Text(message) = value else {
        panic!("the handler should have bound a text message, got {value:?}");
    };
    assert!(
        message.contains("hex.decode") && message.contains("position 0"),
        "the caught message was `{message}`"
    );
}

/// Digests are asserted against values written down here, never computed by the
/// implementation under test.
#[test]
fn edge_hashes_match_literal_known_digests() {
    let cases = [
        (
            r#"hash.sha256("")"#,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            r#"hash.sha256("abc")"#,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            r#"hash.sha256("hello")"#,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        ),
        (
            r#"hash.sha512("abc")"#,
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
             2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
        ),
    ];

    for (source, expected) in cases {
        assert_eq!(eval(source), text(expected), "wrong digest for {source}");
    }

    let same = eval(r#"[hash.sha256("abc"), hash.sha256("abc"), hash.sha256("abd")]"#);
    let Value::List(items) = same else {
        panic!("expected a list of digests, got {same:?}");
    };
    assert_eq!(items[0], items[1], "the same content must hash equally");
    assert_ne!(
        items[0], items[2],
        "different content must hash differently"
    );

    assert_eq!(
        eval(r#"hash.sha256(bytes.from_text("abc"))"#),
        eval(r#"hash.sha256("abc")"#),
        "text and its bytes must hash to the same digest"
    );
}

/// A generated UUID is canonical, parses back to itself, and two draws differ.
/// A parser that only accepts one spelling would reject the forms APIs send.
#[test]
fn edge_uuid_generates_parseable_values_and_parses_every_format() {
    let generated = eval("set id to uuid.generate()\n[uuid.parse(id), length(id), id]");
    let Value::List(items) = generated else {
        panic!("expected [parsed, length, raw], got {generated:?}");
    };
    assert_eq!(items[0], items[2], "a generated UUID must parse to itself");
    assert_eq!(items[1], Value::Number(36.0), "a UUID is 36 characters");

    assert_eq!(
        eval("uuid.generate() == uuid.generate()"),
        Value::YesNo(false),
        "two successive UUIDs must differ"
    );

    let canonical = "550e8400-e29b-41d4-a716-446655440000";
    for spelling in [
        "550e8400-e29b-41d4-a716-446655440000",
        "550E8400-E29B-41D4-A716-446655440000",
        "550e8400e29b41d4a716446655440000",
        "{550e8400-e29b-41d4-a716-446655440000}",
        "urn:uuid:550e8400-e29b-41d4-a716-446655440000",
    ] {
        assert_eq!(
            eval(&format!("uuid.parse(\"{spelling}\")")),
            text(canonical),
            "uuid.parse did not canonicalise {spelling}"
        );
    }

    for extreme in [
        "00000000-0000-0000-0000-000000000000",
        "ffffffff-ffff-ffff-ffff-ffffffffffff",
    ] {
        assert_eq!(
            eval(&format!("uuid.parse(\"{extreme}\")")),
            text(extreme),
            "uuid.parse refused the boundary value {extreme}"
        );
    }
}

/// A wrong type and a malformed UUID are clean refusals naming the call, not
/// panics and not sentinel values.
#[test]
fn edge_bad_hasher_and_uuid_inputs_are_refused_by_name() {
    for (source, name) in [
        ("hash.sha256(5)", "hash.sha256"),
        ("hex.encode(5)", "hex.encode"),
        ("hex.encode([256])", "hex.encode"),
        ("base64.encode([-1])", "base64.encode"),
        (r#"uuid.parse("not-a-uuid")"#, "uuid.parse"),
    ] {
        let error = eval_err(source);
        let message = runtime_message(&error);
        assert!(
            message.contains(name),
            "{source} was refused as `{message}`, which does not name `{name}`"
        );
    }
}
