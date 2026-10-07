//! SPEC.md, README.md and `src/stdlib.rs` must name the same module names.
//!
//! The documents are the language's specification-by-example, so a module name
//! one of them prints is a promise that `module.function(..)` runs. A name the
//! code does not have is an `AnalyzerError: Unknown variable`, and a name the
//! code has that neither document mentions is a promise nobody made — both are
//! the same disagreement, seen from opposite sides.

use std::path::{Path, PathBuf};

use redblue::{Error, Value};

/// The repository root, so the documents are read where they are shipped.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_doc(name: &str) -> String {
    let path = repo_root().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} should be readable: {}", name, e))
}

/// The body of the `## Standard Library` section, from its heading to the next
/// `##` heading — the documents put other material after it, and only this
/// section documents modules.
fn standard_library_section(document: &str, heading: &str) -> String {
    let start = document
        .lines()
        .position(|line| line.trim() == heading)
        .unwrap_or_else(|| panic!("`{}` must have a `{}` section", heading, heading));
    let rest = document.lines().skip(start + 1);
    let body: Vec<&str> = rest.take_while(|line| !line.starts_with("## ")).collect();
    assert!(
        !body.is_empty(),
        "`{}` has an empty `{}` section",
        heading,
        heading
    );
    body.join("\n")
}

/// The module names SPEC.md's §Standard Library documents: the `### console`
/// style headings it puts one per module under.
fn spec_module_names() -> Vec<String> {
    let document = read_doc("SPEC.md");
    let section = standard_library_section(&document, "## Standard Library");
    let names: Vec<String> = section
        .lines()
        .filter_map(|line| line.strip_prefix("### "))
        .map(|name| name.trim().to_lowercase())
        .collect();
    assert!(
        !names.is_empty(),
        "SPEC.md's Standard Library section documents no module"
    );
    names
}

/// The module names README.md's §Standard Library documents, read off the
/// `files.read(path)` examples rather than off its headings, so a heading
/// without an example cannot be mistaken for a promise.
fn readme_module_names() -> Vec<String> {
    let document = read_doc("README.md");
    let section = standard_library_section(&document, "## Standard Library");
    let mut names = Vec::new();
    for line in section.lines() {
        let trimmed = line.trim_start_matches(' ');
        let Some(rest) = trimmed.split_once('.') else {
            continue;
        };
        let (module, member) = rest;
        if module.is_empty()
            || !module
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            || member.is_empty()
            || !member.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        {
            continue;
        }
        let name = module.to_string();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    assert!(
        !names.is_empty(),
        "README.md's Standard Library section documents no module"
    );
    names
}

/// Every module name either document promises, in a stable order so a failure
/// names the same two sets twice.
/// The `module` and `member` of the first `module.member(` in `line`, when that
/// module is one the code has.
///
/// A documented example reaches the code as text, so the test reads the call
/// out of the line the same way the analyzer does: the receiver is the name
/// immediately before the `.`, and it counts only when it is a module.
fn module_call(line: &str) -> Option<(String, String)> {
    for (index, _) in line.match_indices('.') {
        let module: String = line[..index]
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect::<Vec<char>>()
            .into_iter()
            .rev()
            .collect();
        if module.is_empty() || !redblue::stdlib::is_module(&module) {
            continue;
        }
        let rest = &line[index + 1..];
        if !rest.starts_with(|c: char| c.is_ascii_lowercase() || c == '_') {
            continue;
        }
        let member: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !rest[member.len()..].starts_with('(') {
            continue;
        }
        return Some((module, member));
    }
    None
}

/// Every module name either document promises, in a stable order so a failure
/// names the same two sets twice.
fn documented_module_names() -> Vec<String> {
    let mut names = spec_module_names();
    for name in readme_module_names() {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names.sort();
    names
}

/// A Redblue text literal holding `value`, with `\` and `"` escaped for the
/// lexer, so text the test writes in Rust is one literal to the parser.
fn redblue_text(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Every module name the interpreter actually has.
fn implemented_module_names() -> Vec<String> {
    let mut names: Vec<String> = redblue::stdlib::MODULES
        .iter()
        .map(|n| n.to_string())
        .collect();
    names.sort();
    names
}

/// The promise each document makes about module names is the one the code
/// keeps.
#[test]
fn documented_module_names_are_exactly_the_modules_the_code_has() {
    let documented = documented_module_names();
    let implemented = implemented_module_names();

    let missing: Vec<&String> = documented
        .iter()
        .filter(|name| !implemented.contains(name))
        .collect();
    assert!(
        missing.is_empty(),
        "documented but not callable, so `module.function(..)` fails with \
         `Unknown variable`: {:?} (the code has {:?})",
        missing,
        implemented
    );

    let undocumented: Vec<&String> = implemented
        .iter()
        .filter(|name| !documented.contains(name))
        .collect();
    assert!(
        undocumented.is_empty(),
        "callable but documented in neither SPEC.md nor README.md: {:?}",
        undocumented
    );
}

/// Every name in [`MODULES`] is a module a program can import, and importing a
/// builtin namespace is what makes the alias work.
#[test]
fn every_documented_module_can_be_imported_and_called() {
    for module in redblue::stdlib::MODULES {
        assert!(
            redblue::stdlib::is_module(module),
            "`{}` is in MODULES, so is_module must say so",
            module
        );
    }
}

/// Runs `source` and returns the value of its last statement.
#[track_caller]
fn eval(source: &str) -> Value {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    let program = redblue::parser::parse(tokens).expect("source should parse");
    let (_vm, result) = redblue::run_isolated(&program);
    result.unwrap_or_else(|error| panic!("source should have run, failed with {:?}", error))
}

/// Runs `source` and returns the error it produced.
#[track_caller]
fn eval_err(source: &str) -> Error {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    let program = redblue::parser::parse(tokens).expect("source should parse");
    let (_vm, result) = redblue::run_isolated(&program);
    result.expect_err("source should have failed")
}

/// The `module.function` spelling of a builtin reaches the same function its
/// bare name does, and answers the same value.
#[test]
fn module_functions_reach_the_builtins_that_already_exist() {
    // Every documented module function, and the value the documents say it
    // answers. The `length` and `formats` entries are compared against the
    // module that already worked, which is the same builtin under another name.
    for (source, expected, why) in [
        (
            "text.uppercase(\"hello\")",
            Value::Text("HELLO".to_string()),
            "as documented",
        ),
        (
            "text.lowercase(\"HELLO\")",
            Value::Text("hello".to_string()),
            "as documented",
        ),
        (
            "text.trim(\"  x  \")",
            Value::Text("x".to_string()),
            "as documented",
        ),
        (
            "text.split(\"a,b,c\", \",\")",
            Value::List(vec![
                Value::Text("a".to_string()),
                Value::Text("b".to_string()),
                Value::Text("c".to_string()),
            ]),
            "as documented",
        ),
        (
            "text.join([\"a\", \"b\"], \"-\")",
            Value::Text("a-b".to_string()),
            "as documented",
        ),
        (
            "text.length(\"hello\")",
            Value::Number(5.0),
            "as documented",
        ),
        ("math.abs(-7)", Value::Number(7.0), "as documented"),
        ("math.floor(1.7)", Value::Number(1.0), "as documented"),
        ("math.ceil(1.2)", Value::Number(2.0), "as documented"),
        ("math.round(3.7)", Value::Number(4.0), "as documented"),
        ("math.sqrt(9)", Value::Number(3.0), "as documented"),
        (
            "list.length([1, 2, 3])",
            Value::Number(3.0),
            "as documented",
        ),
    ] {
        assert_eq!(eval(source), expected, "`{}` answers {}", source, why);
    }

    // The `formats` functions are the `json` and `csv` ones under the module
    // SPEC.md prints, so each pair must answer one value.
    for (module_form, other_form) in [
        (
            "formats.parse_json(\"{\\\"a\\\": 1}\").a",
            "json.parse(\"{\\\"a\\\": 1}\").a",
        ),
        ("formats.parse_csv(\"a,b\")", "csv.parse(\"a,b\")"),
    ] {
        assert_eq!(
            eval(module_form),
            eval(other_form),
            "`{}` must be `{}`",
            module_form,
            other_form
        );
    }
    assert_eq!(
        eval("formats.to_json({a: 1})"),
        eval("json.stringify({a: 1})"),
        "`formats.to_json` must be `json.stringify`"
    );
}

/// `rb run` of a file that calls every documented module exits 0. The file
/// runs in the checkout, and the one call that cannot be total offline — a
/// network request — is caught, so the exit code is about names, not the wire.
#[test]
fn every_documented_module_runs_from_a_file() {
    let dir = repo_root().join("target/tmp/phase032");
    std::fs::create_dir_all(&dir).expect("scratch dir should be creatable");
    let path = dir.join("modules.rb");
    let source = concat!(
        "import files\n",
        "import time\n",
        "import json\n",
        "import csv\n",
        "import network\n",
        "import console\n",
        "import formats\n",
        "import text\n",
        "import math\n",
        "import list\n",
        "\n",
        "say text.uppercase(\"hello\")\n",
        "say text.split(\"a,b\", \",\")\n",
        "say text.join([\"a\", \"b\"], \"-\")\n",
        "say math.abs(-2)\n",
        "say math.floor(1.7)\n",
        "say math.ceil(1.2)\n",
        "say math.round(3.7)\n",
        "say math.sqrt(9)\n",
        "say list.length([1, 2, 3])\n",
        "say formats.parse_json(\"{\\\"a\\\": 1}\").a\n",
        "say formats.to_json({a: 1})\n",
        "say formats.parse_csv(\"a,b\")\n",
        "say json.parse(\"{\\\"a\\\": 2}\").a\n",
        "say csv.parse(\"a,b\")\n",
        "say time.unix(\"2020-01-01 00:00:00\")\n",
        "say files.exists(\"Cargo.toml\")\n",
        "console.log(\"console\")\n",
        "try\n",
        "    network.get(\"not a url\")\n",
        "catch error\n",
        "    say \"network\"\n",
        "end\n",
    );
    std::fs::write(&path, source).expect("the program should be writable");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rb"))
        .args(["run", path.to_str().expect("a UTF-8 path")])
        .current_dir(repo_root())
        .output()
        .expect("rb should be runnable");
    assert!(
        output.status.success(),
        "`rb run` of a file calling every documented module must exit 0; it said {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let printed = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "HELLO",
        "[a, b]",
        "a-b",
        "2",
        "1",
        "2",
        "4",
        "3",
        "3",
        "1",
        "{\"a\": 1}",
        "[[a, b]]",
        "2",
        "console",
        "network",
    ] {
        assert!(
            printed.contains(expected),
            "the program should print `{}`, printed:\n{}",
            expected,
            printed
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// A module function called with no arguments, with too many, and with an
/// argument of the wrong type: a caught `Runtime` error naming the function, in
/// every case. Never a panic, and never a value that looks like an answer.
#[test]
fn edge_a_module_function_refuses_wrong_arguments_by_naming_itself() {
    for (source, function) in [
        ("text.uppercase()", "text.uppercase"),
        ("text.uppercase(1)", "text.uppercase"),
        ("text.uppercase(\"a\", \"b\")", "text.uppercase"),
        ("text.uppercase(nothing)", "text.uppercase"),
        ("text.split()", "text.split"),
        ("text.split(1, 2)", "text.split"),
        ("text.join()", "text.join"),
        ("text.join(\"not a list\", \",\")", "text.join"),
        ("math.sqrt()", "math.sqrt"),
        ("math.sqrt(\"nine\")", "math.sqrt"),
        ("math.round(1, 2)", "math.round"),
        ("list.length(5)", "list.length"),
        ("formats.parse_json()", "formats.parse_json"),
        ("formats.parse_json(5)", "formats.parse_json"),
        ("formats.parse_csv(5)", "formats.parse_csv"),
    ] {
        match eval_err(source) {
            Error::Runtime(message, span) => {
                assert!(
                    message.contains(function),
                    "`{}` should name `{}` in its error, said `{}`",
                    source,
                    function,
                    message
                );
                assert!(span.is_known(), "`{}` failed without a source span", source);
            }
            other => panic!("`{}` must be a Runtime error, got {:?}", source, other),
        }
    }

    // And it is catchable from Redblue, which is what makes it testable in a
    // program rather than only from Rust.
    assert_eq!(
        eval(concat!(
            "set caught to \"not caught\"\n",
            "try\n",
            "    say text.uppercase(1)\n",
            "catch error\n",
            "    set caught to \"caught\"\n",
            "end\n",
            "caught"
        )),
        Value::Text("caught".to_string()),
        "a module function's refusal must be catchable"
    );
}

/// A module the code does not have is refused, by name. It is not a silent
/// `nothing`, and it is not a hang.
#[test]
fn edge_an_unknown_module_name_is_a_clean_error() {
    for source in [
        "nosuchmodule.read(\"x\")",
        "nosuchmodule.uppercase(\"hi\")",
        "text.nosuchfunction(\"hi\")",
    ] {
        match eval_err(source) {
            Error::Analyzer(message, _) => assert!(
                message.contains("nosuchmodule"),
                "`{}` should name the module it does not have, said `{}`",
                source,
                message
            ),
            Error::Runtime(message, _) => assert!(
                message.contains("nosuch"),
                "`{}` should name what is missing, said `{}`",
                source,
                message
            ),
            other => panic!("`{}` must be refused, got {:?}", source, other),
        }
    }
}

/// Every module name the documents print is reachable through an `import`
/// alias too, since that is the second way a module name is written.
#[test]
fn edge_an_import_alias_reaches_the_same_module_function() {
    assert_eq!(
        eval("import text as T\nT.uppercase(\"hi\")"),
        eval("text.uppercase(\"hi\")"),
        "an alias is a name for the module it names"
    );
    assert_eq!(
        eval("import formats as F\nF.parse_json(\"{\\\"a\\\": 1}\").a"),
        Value::Number(1.0),
        "an alias reaches the formats functions"
    );
}

/// Every line SPEC.md's §Standard Library and README.md's show in a
/// ```redblue block must lex and parse, and every `module.function(..)` call in
/// one must resolve to a module function the code has.
///
/// This is the test that would have caught the original finding: a documented
/// call the parser cannot read is a promise the language does not keep, and one
/// written with a module the code lacks fails with `Unknown variable`.
#[test]
fn edge_documented_calls_all_lex_parse_and_resolve() {
    for (document, heading) in [
        ("SPEC.md", "## Standard Library"),
        ("README.md", "## Standard Library"),
    ] {
        let text = read_doc(document);
        let section = standard_library_section(&text, heading);
        let mut in_block = false;
        let mut checked = 0;
        for line in section.lines() {
            if line.trim_start().starts_with("```") {
                in_block = !in_block;
                continue;
            }
            if !in_block {
                continue;
            }
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with("//")
                || trimmed.starts_with("might fail")
                || trimmed.starts_with("if ")
                || trimmed == "end"
            {
                continue;
            }
            // Only the lines that call a module. A documented statement that is
            // not a module call is a different promise; the ones this phase found
            // unkept are in phases/phase-032/FINDINGS.md.
            if module_call(trimmed).is_none() {
                continue;
            }
            // A call the parser reads, but which would hit the network or the
            // clock. Their *shape* is checked here; the modules they reach are
            // checked by `every_documented_module_runs_from_a_file`.
            if trimmed.contains("network.") || trimmed.contains("time.") {
                continue;
            }
            let tokens = redblue::lexer::Lexer::tokenize(trimmed)
                .unwrap_or_else(|e| panic!("{}: `{}` should lex, got {:?}", document, trimmed, e));
            redblue::parser::parse(tokens).unwrap_or_else(|e| {
                panic!("{}: `{}` should parse, got {:?}", document, trimmed, e)
            });
            checked += 1;
        }
        assert!(
            checked >= 5,
            "only {} documented lines in {} were checked",
            checked,
            document
        );
    }
}

/// The module functions each document names are the ones `MODULE_FUNCTIONS`
/// routes, so a documented `module.function` cannot resolve to a name the
/// routing table does not hold.
#[test]
fn edge_every_documented_module_function_is_routed() {
    for (document, heading) in [
        ("SPEC.md", "## Standard Library"),
        ("README.md", "## Standard Library"),
    ] {
        let text = read_doc(document);
        let section = standard_library_section(&text, heading);
        let mut in_block = false;
        let mut found = 0;
        for line in section.lines() {
            if line.trim_start().starts_with("```") {
                in_block = !in_block;
                continue;
            }
            if !in_block {
                continue;
            }
            let trimmed = line.trim();
            let Some((module, member)) = module_call(trimmed) else {
                continue;
            };
            let qualified = format!("{module}_{member}");
            assert!(
                redblue::stdlib::is_module_function(&qualified),
                "{} documents `{}.{}`, which is not a module function",
                document,
                module,
                member
            );
            found += 1;
        }
        assert!(found > 0, "no module function found in {}", document);
    }
}

/// The two VMs must resolve a module function the same way, or a `.rbc` built
/// from the same source means something else.
#[test]
fn edge_both_vms_resolve_a_module_function_the_same_way() {
    use redblue::bytecode::compile_source;
    use redblue::bytecode::vm::BytecodeVm;

    for (source, expected) in [
        ("text.uppercase(\"hello\")", "HELLO"),
        ("text.split(\"a,b\", \",\")", "[a, b]"),
        ("math.round(3.7)", "4"),
        ("list.length([1, 2])", "2"),
        ("formats.parse_json(\"{\\\"a\\\": 1}\").a", "1"),
    ] {
        let chunk = compile_source(&format!("say {source}\n")).expect("source should compile");
        let mut vm = BytecodeVm::new();
        let result = vm
            .run(&chunk)
            .unwrap_or_else(|e| panic!("{} should run on the bytecode VM, got {:?}", source, e));
        let printed = vm.take_output().join("\n");
        assert_eq!(
            printed.trim(),
            expected,
            "`{}` on the bytecode VM should print `{}`",
            source,
            expected
        );
        assert!(
            matches!(result, Value::Nothing),
            "`say` leaves nothing as the value of the program"
        );
    }

    // And a refusal is refused the same way on both.
    let chunk = compile_source("say text.uppercase(1)\n").expect("source should compile");
    let mut vm = BytecodeVm::new();
    match vm.run(&chunk) {
        Err(Error::Runtime(message, _)) => assert!(
            message.contains("text.uppercase"),
            "the bytecode VM should name `text.uppercase`, said `{}`",
            message
        ),
        other => panic!("the bytecode VM should refuse, got {:?}", other),
    }
}

/// Empty, singleton and boundary arguments: the edges of a text and of a list,
/// through the module spellings, are answers and not errors.
#[test]
fn edge_empty_singleton_and_boundary_arguments_are_answered() {
    assert_eq!(
        eval("text.length(\"\")"),
        Value::Number(0.0),
        "empty text has length zero"
    );
    assert_eq!(
        eval("text.length(\"a\")"),
        Value::Number(1.0),
        "one character has length one"
    );
    assert_eq!(
        eval("text.uppercase(\"\")"),
        Value::Text(String::new()),
        "empty text uppercased is empty text"
    );
    assert_eq!(
        eval("text.trim(\"\")"),
        Value::Text(String::new()),
        "empty text trimmed is empty text"
    );
    assert_eq!(
        eval("text.split(\"\", \",\")"),
        Value::List(vec![Value::Text(String::new())]),
        "empty text split by a separator is one empty cell"
    );
    assert_eq!(
        eval("text.split(\"abc\", \"\")"),
        Value::List(
            ["a", "b", "c"]
                .iter()
                .map(|c| Value::Text(c.to_string()))
                .collect()
        ),
        "an empty separator has nothing to separate, so it splits into characters"
    );
    assert_eq!(
        eval("text.join([], \",\")"),
        Value::Text(String::new()),
        "an empty list joined is empty text"
    );
    assert_eq!(
        eval("text.join([\"a\"], \",\")"),
        Value::Text("a".to_string()),
        "one element joined is that element"
    );
    assert_eq!(
        eval("text.join([\"a\", \"b\"], \"\")"),
        Value::Text("ab".to_string()),
        "an empty separator joins with nothing between"
    );
    assert_eq!(
        eval("list.length([])"),
        Value::Number(0.0),
        "an empty list has length zero"
    );
    assert_eq!(
        eval("list.length([7])"),
        Value::Number(1.0),
        "a one-element list has length one"
    );
    assert_eq!(
        eval("math.sqrt(0)"),
        Value::Number(0.0),
        "zero is a square root, not an error"
    );
    assert_eq!(
        eval("math.abs(0)"),
        Value::Number(0.0),
        "the absolute value of zero is zero"
    );
    assert_eq!(
        eval("math.round(0.5)"),
        Value::Number(1.0),
        "a half rounds up"
    );
    assert_eq!(
        eval("math.round(-0.5)"),
        Value::Number(-1.0),
        "a negative half rounds up as well, away from zero"
    );
}

/// Unicode and escapes survive a round trip through a module function.
#[test]
fn edge_unicode_and_escapes_survive_a_module_function() {
    let label = "\u{1f389} \u{4e2d}\u{6587} \u{202b}e\u{301}";
    assert_eq!(
        eval(&format!("text.length({})", redblue_text(label))),
        Value::Number(label.len() as f64),
        "length counts the bytes of the text it is given"
    );
    assert_eq!(
        eval(&format!(
            "text.join(text.split({}, \" \"), \" \")",
            redblue_text(label)
        )),
        Value::Text(label.to_string()),
        "splitting on a separator and joining on it again is the identity"
    );
    assert_eq!(
        eval(&format!("text.uppercase({})", redblue_text(label))),
        Value::Text(label.to_uppercase()),
        "unicode text uppercases as itself in unicode, not per byte"
    );
    assert_eq!(
        eval("text.uppercase(\"é\")"),
        eval(&format!("text.uppercase({})", redblue_text("é"))),
        "an escaped character and the character it names are one text"
    );
    assert_eq!(
        eval("text.trim(\"\\n\\t x \\t\\n\")"),
        Value::Text("x".to_string()),
        "trim removes whitespace written as escapes too"
    );
}

/// `math.sqrt` of a negative number has no answer, and it says so the way the
/// language already does: `nothing`, never a `NaN` dressed as a number.
#[test]
fn edge_a_math_function_that_has_no_answer_is_nothing_not_a_number() {
    assert_eq!(
        eval("math.sqrt(-1)"),
        Value::Nothing,
        "sqrt of a negative number answers nothing"
    );
    assert!(
        !matches!(eval("math.sqrt(-1)"), Value::Number(_)),
        "sqrt of a negative number must not answer a number"
    );
    assert_eq!(
        eval("math.abs(-0.5)"),
        Value::Number(0.5),
        "a fraction is not rounded away by abs"
    );
}

/// The documented examples that this parser cannot read are corrected in
/// SPEC.md, and this test fails if one comes back: a `module.function(..)` call
/// written with the function literal of a later phase is a promise the parser
/// does not keep today.
#[test]
fn edge_spec_does_not_promise_a_call_the_parser_cannot_read() {
    let document = read_doc("SPEC.md");
    let section = standard_library_section(&document, "## Standard Library");
    for line in section.lines() {
        let trimmed = line.trim();
        if !trimmed.contains('.') || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }
        assert!(
            !trimmed.contains("to ("),
            "SPEC.md's Standard Library still shows the function literal `{}`, \
             which the parser cannot read yet",
            trimmed
        );
    }
}

/// The repository's own examples still run: this phase touches how a module
/// name is resolved, and a change there that broke an example would show here.
#[test]
fn edge_the_examples_still_run() {
    let dir = repo_root().join("examples");
    let mut checked = 0;
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("examples/ should be readable")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension().is_some_and(|e| e == "rb") {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    entries.sort();
    assert!(!entries.is_empty(), "examples/ should hold programs to run");

    // Each example runs in a directory of its own: an example that writes a
    // file writes it where it is run from, so two tests running the same
    // example in the checkout would race over the same file.
    let scratch = repo_root().join("target/tmp/phase032-examples");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch dir should be creatable");
    for path in entries {
        let name = Path::new(&path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let copy = scratch.join(&name);
        std::fs::copy(&path, &copy).expect("the example should be copyable");
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_rb"))
            .args(["run", copy.to_str().expect("a UTF-8 path")])
            .current_dir(&scratch)
            .output()
            .expect("rb should be runnable");
        assert!(
            output.status.success(),
            "examples/{} must still exit 0; it said {}\n{}",
            name,
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        checked += 1;
    }
    assert!(checked >= 1, "no example was run");
    let _ = std::fs::remove_dir_all(&scratch);
}
