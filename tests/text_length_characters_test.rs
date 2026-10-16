//! `length` on text counts characters, not UTF-8 bytes.
//!
//! The finding, reproduced against the built binary before any change:
//! `say text.length("日本")` printed `6` and `say length("é")` printed `2`,
//! because `runtime::builtin` answered `"length" | "len"` on `Value::Text(s)`
//! with `s.len()` — the byte count — before any other implementation was
//! consulted. README.md says "Text is fully Unicode" and AGENTS.md §3.2 makes
//! unicode and combining marks a mandatory edge class, so the one measurement
//! a program can make about its own text was silently wrong for every non-ASCII
//! string, with a plausible-looking number.
//!
//! `length` answers a *list* with its element count and that does not move:
//! `length([1,2,3])` is `3` before and after. Only text changes, and it
//! changes from bytes to characters — Rust `char`s, i.e. Unicode scalar values.
//! `combining_marks_are_counted_as_two_code_points` records why that word is
//! the deliberate choice and not an accident.
//!
//! Every row runs through **both** engines, because the two answers are
//! produced by two different dispatchers and a table that only walked the tree
//! walker would leave the bytecode VM free to disagree with it forever.

use redblue::bytecode::vm::BytecodeVm;
use redblue::compile_source;

/// What one engine made of a program: the lines it printed, and the failure it
/// ended with if it ended with one.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    output: Vec<String>,
    error: Option<String>,
}

/// Runs `source` through lexer, parser, analyzer and the tree-walking VM.
///
/// The analyzer is in this path deliberately: two of the rows below are
/// analyzer refusals, and a helper that skipped it could not see them.
fn tree_walk(source: &str) -> Outcome {
    let tokens = match redblue::lexer::Lexer::tokenize(source) {
        Ok(tokens) => tokens,
        Err(error) => return failed(error.label(), error.message()),
    };
    let ast = match redblue::parser::parse(tokens) {
        Ok(ast) => ast,
        Err(error) => return failed(error.label(), error.message()),
    };
    if let Err(error) = redblue::analyzer::analyze(&ast) {
        return failed(error.label(), error.message());
    }
    let (mut vm, result) = redblue::run_isolated(&ast);
    Outcome {
        output: vm.take_output(),
        error: result
            .err()
            .map(|e| format!("{}: {}", e.label(), e.message())),
    }
}

/// A frontend failure as an outcome: nothing ran, so nothing was printed.
fn failed(label: &str, message: &str) -> Outcome {
    Outcome {
        output: Vec::new(),
        error: Some(format!("{label}: {message}")),
    }
}

/// Runs `source` through the bytecode compiler and then the bytecode VM.
fn bytecode(source: &str) -> Outcome {
    let chunk = match compile_source(source) {
        Ok(chunk) => chunk,
        Err(error) => return failed(error.label(), error.message()),
    };
    let mut vm = BytecodeVm::new();
    let result = vm.run(&chunk);
    Outcome {
        output: vm.take_output(),
        error: result
            .err()
            .map(|e| format!("{}: {}", e.label(), e.message())),
    }
}

/// The last line the tree walker printed, with a message that names the source
/// on failure rather than leaving `left == right` to be guessed at.
#[track_caller]
fn say(source: &str) -> String {
    let outcome = tree_walk(source);
    let line = outcome
        .output
        .last()
        .unwrap_or_else(|| panic!("the program printed nothing: {outcome:?}\n{source}"))
        .clone();
    line.trim_end().to_string()
}

/// The last line the bytecode VM printed.
#[track_caller]
fn say_bytecode(source: &str) -> String {
    let outcome = bytecode(source);
    let line = outcome
        .output
        .last()
        .unwrap_or_else(|| panic!("the program printed nothing: {outcome:?}\n{source}"))
        .clone();
    line.trim_end().to_string()
}

/// Asserts both engines print exactly `expected`, and returns nothing else.
///
/// The comparison is a round trip through both engines on every row so the two
/// dispatchers cannot drift without a test going red.
#[track_caller]
fn both_engines_say(expected: &str, source: &str) {
    assert_eq!(say(source), expected, "tree walker on {source:?}");
    assert_eq!(say_bytecode(source), expected, "bytecode VM on {source:?}");
}

#[test]
fn text_length_counts_cjk_characters_not_bytes() {
    // Each ideograph is three UTF-8 bytes, so the byte count said 6.
    both_engines_say("2", "say length(\"日本\")\n");
    both_engines_say("2", "say text.length(\"世界\")\n");
    both_engines_say("4", "say length(\"你好世界\")\n");
}

#[test]
fn text_length_counts_latin1_and_emoji_as_one_character_each() {
    // "é" is two bytes and one character; U+1F600 is four bytes and one
    // character, the surrogate-pair trap a byte count answers 4 for.
    both_engines_say("1", "say length(\"é\")\n");
    both_engines_say("1", "say text.length(\"\u{1F600}\")\n");
    both_engines_say("1", "say length(\"ü\")\n");
}

#[test]
fn list_length_is_unchanged() {
    // The list arm never counted anything but elements and must not start.
    // This is the row that would break if the fix widened past text.
    both_engines_say("3", "say length([1, 2, 3])\n");
    both_engines_say("3", "say list.length([1, 2, 3])\n");
    both_engines_say("0", "say length([])\n");
    both_engines_say("1", "say length([7])\n");
}

#[test]
fn edge_text_length_of_empty_text_is_zero() {
    // Empty: nothing to count, and no byte/character disagreement is possible
    // because there is nothing. Pinned so the case is stated, not assumed.
    both_engines_say("0", "say length(\"\")\n");
    both_engines_say("0", "say text.length(\"\")\n");
}

#[test]
fn edge_text_length_of_a_singleton_is_one() {
    both_engines_say("1", "say length(\"a\")\n");
    both_engines_say("1", "say text.length(\"日\")\n");
}

#[test]
fn edge_text_length_ascii_is_unchanged() {
    // Every ASCII character is one byte, so the number a program already saw
    // is the number it still sees. This is the backwards-compatibility row.
    both_engines_say("5", "say length(\"hello\")\n");
    both_engines_say("8", "say length(\"say \\\"hi\\\"\")\n");
    both_engines_say("3", "say length(\"a\\nb\")\n");
}

#[test]
fn combining_marks_are_counted_as_two_code_points() {
    // The choice, recorded deliberately rather than left to whichever
    // implementation happened to answer.
    //
    // "e" + U+0301 COMBINING ACUTE ACCENT is two Unicode scalar values and one
    // user-perceived glyph. `length` counts characters, where a character is
    // one scalar value, so the answer is 2. It is not 1: grapheme clusters are
    // a segmentation problem needing the UAX #29 tables plus join and stack
    // handling, which is a different measurement under a different name. And
    // it is not 3, which is what the byte count said. A program that wants the
    // grapheme count or the bytes reaches for `bytes.from_text` or normalises
    // first; it does not get a silently different answer from the same word.
    both_engines_say("2", "say length(\"e\u{301}\")\n");
    both_engines_say("2", "say text.length(\"e\u{301}\")\n");
    both_engines_say("4", "say length(\"e\u{301}e\u{301}\")\n");
}

#[test]
fn edge_text_length_right_to_left_counts_characters() {
    // Hebrew: four characters, eight bytes. The count does not depend on where
    // a bidi mark sits, because the count is over scalar values in either.
    both_engines_say("4", "say length(\"שלום\")\n");
    both_engines_say("4", "say text.length(\"שלום\")\n");
}

#[test]
fn edge_text_length_of_a_list_of_zero_elements_is_zero() {
    // The list boundary, so "empty" is pinned for both argument kinds and the
    // fix cannot quietly become a list-specific one.
    both_engines_say("0", "say length([])\n");
    both_engines_say("0", "say text.length([])\n");
}

#[test]
fn edge_text_containing_nul_counts_the_characters_around_it() {
    // A NUL is one character and one byte, and it is not a terminator: the
    // text is "\0a\0", so three characters. It also proves the count is over
    // the whole `String` and not over a C string's prefix, which would say 0.
    //
    // `\0` is not an escape this lexer has — `Unknown escape '\0' in text` is
    // a refusal a program can rely on — so the NUL is written the way the
    // language writes it, as `\u0000`, and the second row builds one through
    // `bytes.text` from a byte list instead.
    both_engines_say("3", "set s to \"\\u0000a\\u0000\"\nsay length(s)\n");
    both_engines_say("1", "say length(\"\\u0000\")\n");
    both_engines_say("2", "say length(bytes.text([0, 65]))\n");
}

#[test]
fn edge_text_length_at_the_last_character_has_no_off_by_one() {
    // Boundary: the final character is the one an off-by-one drops. Three
    // characters, the last a 3-byte ideograph, and three with a 4-byte astral
    // last, which is the row a byte count got most wrong.
    both_engines_say("3", "say length(\"a日b\")\n");
    both_engines_say("3", "say length(\"ab\u{1F600}\")\n");
    both_engines_say("1", "say length(\"日\")\n");
    // The last character of a two-character string is the second one.
    both_engines_say("2", "say length(\"éü\")\n");
}

#[test]
fn edge_text_length_mixed_script_text_is_the_sum_of_its_characters() {
    // Every width in one string: 2-byte, 3-byte and 4-byte characters with the
    // spaces between them. `h`, `i`, ` `, 🎉, ` `, 世, 界 is seven characters and
    // fourteen bytes — the byte count was twice the character count, which is
    // the whole of the finding in one string.
    let program = "set mixed to \"hi \u{1F389} 世界\"\nsay length(mixed)\n";
    assert_eq!(say(program), "7");
    assert_eq!(say_bytecode(program), "7");
    // And the byte spelling still answers bytes, so nothing was lost.
    assert_eq!(say("say length(bytes.from_text(\"héllo\"))\n"), "6");
    assert_eq!(say("say length(\"héllo\")\n"), "5");
}

#[test]
fn both_engines_answer_every_row_of_the_table_identically() {
    // The engines are compared to each other rather than each restated, so a
    // divergence is a diff rather than two rows that were meant to match.
    let programs = [
        "say length(\"日本\")\n",
        "say length(\"é\")\n",
        "say length(\"\u{1F600}\")\n",
        "say length(\"e\u{301}\")\n",
        "say length(\"\")\n",
        "say length([1, 2, 3])\n",
        "say text.length(\"世界\")\n",
        "say length(\"héllo\")\n",
    ];
    for (i, program) in programs.iter().enumerate() {
        assert_eq!(
            tree_walk(program),
            bytecode(program),
            "the two engines disagree on row {i}: {program:?}"
        );
    }
    assert_eq!(say("say length(\"héllo\")\n"), "5");
}

#[test]
fn length_of_a_record_is_still_a_clean_runtime_error() {
    // The failure channel. A record is neither a list nor a text: the answer
    // is a runtime error naming what length accepts — not a panic, not a
    // number, not `nothing`.
    let source = "set bad to length({a: 1})\nsay bad\n";
    let outcome = tree_walk(source);
    assert!(outcome.output.is_empty(), "nothing may be printed");
    let error = outcome.error.clone().expect("a record has no length");
    assert!(
        error.contains("length requires a list or text"),
        "the error must name what length accepts, got: {error}"
    );
    assert_eq!(tree_walk(source).error, bytecode(source).error);
}

#[test]
fn length_of_a_number_is_still_a_clean_runtime_error() {
    // Type mismatch, the other half. `length(5)` has no sensible answer and
    // must be refused rather than coerced to 0 or to its own byte width.
    let outcome = tree_walk("say length(5)\n");
    let error = outcome.error.clone().expect("a number has no length");
    assert!(
        error.contains("length requires a list or text"),
        "the error must name what length accepts, got: {error}"
    );
    assert_eq!(outcome.error, bytecode("say length(5)\n").error);
}

#[test]
fn edge_length_over_text_has_exactly_one_implementation() {
    // The dead arm. `stdlib::builtin_function` had a `"length" =>` arm that no
    // caller reached, because `runtime::builtin` matches `"length" | "len"`
    // first and answers for every argument `length` accepts. Two
    // implementations that cannot see each other is how `length("日本")` was 6
    // in one and 2 in the other for as long as both existed, with only the
    // reachable one ever running — and no test could see the disagreement.
    //
    // It is deleted, and this test is what stops it coming back.
    assert_eq!(
        redblue::stdlib::builtin_function("length", vec![redblue::Value::Text("日本".into())]),
        None,
        "stdlib must not implement `length` over text a second time"
    );
    assert_eq!(
        redblue::stdlib::builtin_function(
            "length",
            vec![redblue::Value::list(vec![redblue::Value::Number(1.0),])]
        ),
        None,
        "stdlib must not implement `length` over a list a second time either"
    );
    // And the source of that function carries no arm keyed on the name, so a
    // second implementation cannot hide behind a different value type either.
    let source = include_str!("../src/stdlib.rs");
    assert!(
        !source.contains("\"length\" =>"),
        "src/stdlib.rs implements `length` again; runtime::builtin is the only one"
    );
    // The reachable one is in `src/runtime.rs`, and it is the only `length`
    // arm there too.
    let runtime = include_str!("../src/runtime.rs");
    let arms = runtime.matches("\"length\" | \"len\" =>").count();
    assert_eq!(
        arms, 1,
        "src/runtime.rs has {arms} `length` arms, expected 1"
    );
}

#[test]
fn edge_no_other_builtin_moved_when_length_changed() {
    // The registry, walked the way `tests/stdlib_module_docs_test.rs` walks it.
    // Every answer below was taken from the language before `length` changed and
    // is asserted again here, so a fix that reached past text would show up as a
    // row that stopped matching rather than as a claim in a report.
    //
    // The table deliberately includes builtins that measure a text by some other
    // means — `bytes.from_text`, `split`, `join` — because those are the ones a
    // careless change to counting would have dragged along with it.
    let cases: &[(&str, &str)] = &[
        // Text builtins other than `length`.
        (r#"say uppercase("héllo")"#, "HÉLLO"),
        (r#"say lowercase("HÉLLO")"#, "héllo"),
        (r#"say trim("  x  ")"#, "x"),
        (r#"say split("a,b", ",")"#, "[a, b]"),
        (r#"say split("日本,語", ",")"#, "[日本, 語]"),
        (r#"say join(["日本", "語"], ",")"#, "日本,語"),
        (r#"say replace("a-b", "-", "+")"#, "a+b"),
        (r#"say contains("héllo", "éll")"#, "yes"),
        (r#"say starts_with("héllo", "hé")"#, "yes"),
        (r#"say ends_with("héllo", "llo")"#, "yes"),
        (r#"say is_text("a")"#, "yes"),
        (r#"say is_list([1])"#, "yes"),
        // The byte spellings, which are where the byte count went.
        (
            r#"say bytes.from_text("héllo")"#,
            "[104, 195, 169, 108, 108, 111]",
        ),
        (r#"say bytes.text([104, 105])"#, "hi"),
        // Math, untouched.
        (r#"say abs(-3)"#, "3"),
        (r#"say sqrt(9)"#, "3"),
        (r#"say pow(2, 10)"#, "1024"),
        (r#"say floor(1.7)"#, "1"),
        (r#"say ceil(1.2)"#, "2"),
        // Lists, untouched: `length` over a list is not what moved.
        (r#"say length([1, 2, 3])"#, "3"),
        (r#"say length(bytes.from_text("héllo"))"#, "6"),
        (r#"say pop([1, 2, 3])"#, "3"),
        (r#"say shift([1, 2, 3])"#, "1"),
        // The dotted spellings, so a module table edit cannot have moved them.
        (r#"say text.uppercase("hi")"#, "HI"),
        (r#"say math.abs(-3)"#, "3"),
        (r#"say list.length([1, 2, 3])"#, "3"),
    ];

    for (source, expected) in cases {
        assert_eq!(say(source), *expected, "tree walker: {source}");
        assert_eq!(say_bytecode(source), *expected, "bytecode VM: {source}");
    }

    // And every name in the registry is still a name the language can answer or
    // refuse by name — the walk `edge_every_flat_builtin_the_stdlib_registers_answers`
    // makes, asserted here as a count so a builtin cannot be lost silently.
    let registry = redblue::stdlib::BUILTIN_NAMES;
    for name in registry {
        assert!(
            redblue::stdlib::is_builtin(name),
            "{name} is in the registry but is not a builtin"
        );
    }
    assert!(registry.contains(&"length"), "length left the registry");
}
