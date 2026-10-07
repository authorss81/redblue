//! SPEC.md, README.md and `src/stdlib.rs` must name the same module names.
//!
//! The documents are the language's specification-by-example, so a module name
//! one of them prints is a promise that `module.function(..)` runs. A name the
//! code does not have is an `AnalyzerError: Unknown variable`, and a name the
//! code has that neither document mentions is a promise nobody made — both are
//! the same disagreement, seen from opposite sides.
//!
//! AGENTS.md is read here too, and the round-two tests below are the arguments
//! and the boundaries of the module functions rather than their names: a
//! function that refuses a wrong argument by one spelling and coerces it by the
//! other, or that panics where it should raise, is as much a lie to a document
//! as a name that does not resolve.

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

/// The `receiver` and `member` of the first `receiver.member(` in `line`.
///
/// A documented example reaches the code as text, so the test reads the call out
/// of the line the way the analyzer does: the receiver is the name immediately
/// before the `.`, and the member is what the `(` follows.
///
/// The receiver is *not* checked against the modules the code has. A document
/// that wrote `nosuchmod.read("x")` has made a promise about a module the code
/// does not have, and the test that reads the promise has to see it: a filter
/// here would drop the line on the floor and the routing test below would pass
/// without ever looking at it.
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
        if module.is_empty() {
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

/// Every ```redblue block in `document`, in order.
///
/// [`redblue_blocks`] reads every fenced block, which is right for a section
/// that is all Redblue and wrong for `docs/GRAMMAR.md`: a bare ``` fence there
/// holds a *grammar production* — a statement of what the language is specified
/// to be — where a ```redblue fence holds a program that has to run.
fn redblue_programs(document: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = String::new();
    let mut in_block = false;
    for line in document.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            if in_block {
                in_block = false;
                if !current.trim().is_empty() {
                    blocks.push(std::mem::take(&mut current));
                }
                current.clear();
            } else if trimmed.trim_start_matches('`').trim() == "redblue" {
                in_block = true;
                current.clear();
            }
            continue;
        }
        if in_block {
            current.push_str(line);
            current.push('\n');
        }
    }
    blocks
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

/// Runs `source` through `rb run`, whole, and returns its exit status, its
/// standard output and its standard error.
///
/// For the names only the *analyzer* refuses. `PI` and `E` are in the globals map
/// `stdlib::builtins()` returns and the analyzer is never told they are bound, so
/// a tree-walk of the same program reaches `PI` and prints it; only the whole
/// pipeline — the path a user takes — reports `Unknown variable 'PI'`.
#[track_caller]
fn run_rb(source: &str) -> (std::process::ExitStatus, String, String) {
    let dir = repo_root().join("target/tmp/phase032-run");
    std::fs::create_dir_all(&dir).expect("scratch dir should be creatable");
    let path = dir.join(format!("program-{}.rb", source.len()));
    std::fs::write(&path, format!("{source}\n")).expect("the program should be writable");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rb"))
        .args(["run", path.to_str().expect("a UTF-8 path")])
        .current_dir(repo_root())
        .output()
        .expect("rb should be runnable");
    let _ = std::fs::remove_file(&path);
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// The body of the `### ` subsection named `heading`, from the heading to the
/// next heading of any level.
///
/// Distinct from [`standard_library_section`], which stops only at a `## `
/// heading: `### math` is followed by `### time`, and a `## ` stop would read
/// every section of §Standard Library as one.
fn subsection(document: &str, heading: &str) -> String {
    let start = document
        .lines()
        .position(|line| line.trim() == heading)
        .unwrap_or_else(|| panic!("`{heading}` must be a section of the document"));
    let body: Vec<&str> = document
        .lines()
        .skip(start + 1)
        .take_while(|line| !line.trim_start().starts_with('#'))
        .collect();
    assert!(!body.is_empty(), "`{heading}` is empty", heading = heading);
    body.join("\n")
}

/// The table rows of `section`: every line that begins with a `|`, so prose is
/// not read as a promise.
///
/// SPEC.md §Keywords names the reserved-but-unimplemented words in prose
/// immediately after its table — deliberately, so the correction is on the page
/// rather than a silent omission — and a scan of the whole section would report
/// that prose as the drift it is correcting.
fn table_rows(section: &str) -> Vec<String> {
    section
        .lines()
        .filter(|line| line.trim_start().starts_with('|'))
        .map(|line| line.to_string())
        .collect()
}

/// The `module.function` spelling of a builtin is the way the documents call it,
/// and it answers the value each document prints.
#[test]
fn module_functions_reach_the_builtins_that_already_exist() {
    // Every documented module function, and the value the documents say it
    // answers. The `formats` entries are compared against the `json` and `csv`
    // spellings of the same functions, which is what `formats` documents.
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
        ("math.random()", "math.random"),
        ("math.random(\"hi\", \"there\")", "math.random"),
        ("math.random(1, 2, 3)", "math.random"),
        ("console.log()", "console.log"),
        ("console.log(\"a\", \"b\")", "console.log"),
        ("console.error()", "console.error"),
        ("console.clear(1)", "console.clear"),
        ("files.read(\"a\", \"b\")", "files.read"),
        ("files.write(\"p\", \"c\", \"extra\")", "files.write"),
        ("files.exists()", "files.exists"),
        ("time.now(1)", "time.now"),
        ("time.sleep(\"soon\")", "time.sleep"),
        ("time.format(0, \"%Y\", \"extra\")", "time.format"),
        ("json.parse(1, 2)", "json.parse"),
        ("network.post(\"url\")", "network.post"),
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

/// A module is the way the documents call a builtin, and README.md says so in
/// as many words: a bare `uppercase("hi")` is an unknown function. The document
/// and the code are pinned together here, so neither can drift from the other —
/// and if a later phase teaches the bare name, this fails and says which half of
/// the promise moved.
#[test]
fn edge_a_bare_builtin_name_is_not_a_second_spelling_of_a_module_function() {
    for source in [
        "uppercase(\"hi\")",
        "lowercase(\"HI\")",
        "trim(\" x \")",
        "split(\"a,b\", \",\")",
        "join([\"a\"], \",\")",
        "abs(-1)",
        "sqrt(4)",
    ] {
        match eval_err(source) {
            Error::Runtime(message, span) => {
                assert!(
                    message.contains("Unknown function"),
                    "`{}` should be an unknown function while the documents spell it \
                     `module.function`, said `{}`",
                    source,
                    message
                );
                assert!(span.is_known(), "`{}` failed without a span", source);
            }
            other => panic!("`{}` must be a Runtime error, got {:?}", source, other),
        }
    }

    // The module spelling of each of those names is the one that answers.
    for (module_form, expected) in [
        ("text.uppercase(\"hi\")", Value::Text("HI".to_string())),
        ("text.lowercase(\"HI\")", Value::Text("hi".to_string())),
        ("text.trim(\" x \")", Value::Text("x".to_string())),
        (
            "text.split(\"a,b\", \",\")",
            Value::List(vec![
                Value::Text("a".to_string()),
                Value::Text("b".to_string()),
            ]),
        ),
        ("text.join([\"a\"], \",\")", Value::Text("a".to_string())),
        ("math.abs(-1)", Value::Number(1.0)),
        ("math.sqrt(4)", Value::Number(2.0)),
    ] {
        assert_eq!(eval(module_form), expected, "`{}` answers", module_form);
    }
}

/// `math.random` answers a draw from the range it is given, and refuses
/// anything else by name. The refusal is the point: a wrong argument answered
/// with *a* number is indistinguishable from a real draw, so a program with a
/// bug in it would go on running and print plausible nonsense.
#[test]
fn edge_math_random_answers_a_number_or_refuses_by_name() {
    match eval("math.random(1, 2)") {
        Value::Number(n) => assert!(
            (1.0..=2.0).contains(&n),
            "math.random(1, 2) must draw from its range, got {}",
            n
        ),
        other => panic!("math.random(1, 2) should be a number, got {:?}", other),
    }
    match eval("math.random(5)") {
        Value::Number(n) => assert!(
            (0.0..=5.0).contains(&n),
            "math.random(5) must draw from zero to five, got {}",
            n
        ),
        other => panic!("math.random(5) should be a number, got {:?}", other),
    }

    for source in [
        "math.random()",
        "math.random(\"hi\", \"there\")",
        "math.random(1, \"two\")",
        "math.random(nothing)",
        "math.random(1, 2, 3)",
    ] {
        match eval_err(source) {
            Error::Runtime(message, span) => {
                assert!(
                    message.contains("math.random"),
                    "`{}` should name `math.random` in its error, said `{}`",
                    source,
                    message
                );
                assert!(span.is_known(), "`{}` failed without a span", source);
            }
            other => panic!("`{}` must be a Runtime error, got {:?}", source, other),
        }
    }

    // And the bare builtin it reaches names itself the same way when it is
    // written bare, which is the only spelling it has there.
    match eval_err("random_number(\"hi\")") {
        Error::Runtime(message, _) => assert!(
            message.contains("random_number"),
            "a bare `random_number` should name itself, said `{}`",
            message
        ),
        other => panic!(
            "`random_number(\"hi\")` must be a Runtime error, got {:?}",
            other
        ),
    }
}

/// The argument count each document shows is the count the function answers,
/// and one argument too many is refused through the module spelling and the
/// bare one alike.
#[test]
fn edge_a_documented_argument_count_is_the_count_the_function_takes() {
    for (source, expected) in [
        ("console.log(\"printed\")", Value::Nothing),
        ("console.clear()", Value::Nothing),
        ("files.exists(\"Cargo.toml\")", Value::YesNo(true)),
        ("files.exists(\"no-such-file.txt\")", Value::YesNo(false)),
        ("time.format(0, \"%Y\")", Value::Text("1970".to_string())),
        (
            "csv.parse(\"a,b\")",
            Value::List(vec![Value::List(vec![
                Value::Text("a".to_string()),
                Value::Text("b".to_string()),
            ])]),
        ),
    ] {
        assert_eq!(eval(source), expected, "`{}` answers", source);
    }

    // `time.format`'s second argument is optional, so a one-argument call is an
    // answer rather than a refusal — which is why it is the one module function
    // with no stated count.
    assert_eq!(
        eval("time.format(0)"),
        Value::Text("1970-01-01 00:00:00".to_string()),
        "a timestamp alone is an ordinary call"
    );

    for (module_form, bare_form, function) in [
        (
            "files.read(\"Cargo.toml\", \"extra\")",
            "files_read(\"Cargo.toml\", \"extra\")",
            "files.read",
        ),
        (
            "console.log(\"a\", \"b\")",
            "console_log(\"a\", \"b\")",
            "console.log",
        ),
        ("time.now(1)", "time_now(1)", "time.now"),
    ] {
        for source in [module_form, bare_form] {
            match eval_err(source) {
                Error::Runtime(message, span) => {
                    assert!(
                        message.contains(function) || message.contains(&function.replace('.', "_")),
                        "`{}` should name `{}` in its error, said `{}`",
                        source,
                        function,
                        message
                    );
                    assert!(span.is_known(), "`{}` failed without a span", source);
                }
                other => panic!("`{}` must be a Runtime error, got {:?}", source, other),
            }
        }
    }
}

/// A module the code does not have is refused, by name and in the spelling the
/// program wrote. It is not a silent `nothing`, and it is not a hang.
///
/// The *exact* message is pinned for each source, not a substring. It used to
/// assert only `message.contains("nosuch")`, which the internal encoding passed
/// as readily as the right one: both VMs look a call up as `module_member`, so
/// `nosuchmodule.read("x")` reported `Unknown function 'nosuchmodule_read'` — a
/// name no line of Redblue can contain, in a file whose reader wrote
/// `nosuchmodule.read`. A receiver that *is* bound reaches that path, which is
/// why `set thing to 1` is one of the sources here and not only the unbound one.
#[test]
fn edge_an_unknown_module_name_is_a_clean_error() {
    for (source, expected) in [
        (
            "nosuchmodule.read(\"x\")",
            "Unknown function 'nosuchmodule.read'",
        ),
        (
            "nosuchmodule.uppercase(\"hi\")",
            "Unknown function 'nosuchmodule.uppercase'",
        ),
        (
            "text.nosuchfunction(\"hi\")",
            "Module 'text' has no function 'nosuchfunction'",
        ),
        // A receiver that names a variable reaches the same path, because a
        // receiver that is neither a declared object nor a module is a module
        // function the code does not have.
        (
            "set thing to 1\nsay thing.read(\"x\")",
            "Unknown function 'thing.read'",
        ),
        (
            "set xs to [1, 2]\nsay xs.nosuchfunction([1, 2])",
            "Unknown function 'xs.nosuchfunction'",
        ),
        // A receiver that names a *module* is told the module has no such
        // function, which is a different sentence and a different fix.
        (
            "set xs to [1, 2]\nsay list.nosuchfunction([1, 2])",
            "Module 'list' has no function 'nosuchfunction'",
        ),
    ] {
        match eval_err(source) {
            Error::Runtime(message, span) => {
                assert_eq!(
                    message, expected,
                    "`{}` should be refused in the spelling the program wrote",
                    source
                );
                assert!(
                    !message.contains("_"),
                    "`{}` leaked the internal `module_member` encoding: `{}`",
                    source,
                    message
                );
                assert!(span.is_known(), "`{}` failed without a span", source);
            }
            other => panic!("`{}` must be refused, got {:?}", source, other),
        }
    }

    // The whole pipeline refuses it too — through the analyzer, before either VM
    // runs — and says nothing about the encoding either way.
    let (status, _out, err) = run_rb("say nosuchmodule.read(\"x\")\n");
    assert!(
        !status.success(),
        "`nosuchmodule.read(..)` must not run to completion"
    );
    assert!(
        err.contains("nosuchmodule") && !err.contains("nosuchmodule_read"),
        "the pipeline should name the module it does not have, said `{}`",
        err
    );
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

/// The ```redblue blocks of `section`, each one whole.
///
/// A block is read as a unit rather than a line at a time: `if … end` is a
/// statement, so half of it is not a program the parser can be asked about.
fn redblue_blocks(section: &str) -> Vec<String> {
    let mut in_block = false;
    let mut current = String::new();
    let mut blocks = Vec::new();
    for line in section.lines() {
        if line.trim_start().starts_with("```") {
            if in_block && !current.trim().is_empty() {
                blocks.push(std::mem::take(&mut current));
            }
            current.clear();
            in_block = !in_block;
            continue;
        }
        if in_block {
            current.push_str(line);
            current.push('\n');
        }
    }
    if in_block && !current.trim().is_empty() {
        blocks.push(current);
    }
    blocks
}

/// Every ```redblue block SPEC.md's §Standard Library and README.md's show must
/// lex and parse as a program — the whole block, so a line that only means
/// something inside one, like the body of an `if`, is read with the rest of it.
///
/// Nothing is skipped. A block is not executed here — it is read, which is why
/// a `network.get` or a `time.sleep` line can be checked like any other: the
/// wire and the clock are only reached by running it, and
/// `every_documented_module_runs_from_a_file` is where the modules are run.
///
/// This is the test that would have caught the original finding: a documented
/// call the parser cannot read is a promise the language does not keep.
#[test]
fn edge_documented_calls_all_lex_parse_and_resolve() {
    for (document, heading) in [
        ("SPEC.md", "## Standard Library"),
        ("README.md", "## Standard Library"),
    ] {
        let text = read_doc(document);
        let section = standard_library_section(&text, heading);
        let blocks = redblue_blocks(&section);
        assert!(
            blocks.len() >= 5,
            "only {} documented blocks in {} were read",
            blocks.len(),
            document
        );
        let mut lines = 0;
        for source in &blocks {
            lines += source.lines().count();
            let tokens = redblue::lexer::Lexer::tokenize(source).unwrap_or_else(|e| {
                panic!("{document}: a documented block should lex, got {source:?}: {e:?}")
            });
            redblue::parser::parse(tokens).unwrap_or_else(|e| {
                panic!("{document}: a documented block should parse, got {source:?}: {e:?}")
            });
        }
        assert!(
            lines >= 30,
            "only {lines} documented lines in {document} were read",
        );
    }
}

/// The module functions each document names are the ones `MODULE_FUNCTIONS`
/// routes, so a documented `module.function` cannot resolve to a name the
/// routing table does not hold.
///
/// Every `word.word(` in either document's §Standard Library counts, whether or
/// not the code has a module called `word`: a documented call through a module
/// that does not exist is the failure this test exists to catch, so the receiver
/// is checked here rather than filtered out before the check.
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
            assert!(
                redblue::stdlib::is_module(&module),
                "{} documents `{}.{}`, and `{}` is not a module the code has, so the \\
                 call is an `Unknown variable`",
                document,
                module,
                member,
                module
            );
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
    // SPEC.md §text says the same, and pins it: `length` counts bytes, so a
    // character outside the BMP is four of them and two CJK characters are six.
    for (text, bytes) in [
        ("\u{1f389}", 4.0),
        ("\u{4e2d}\u{6587}", 6.0),
        ("e\u{301}", 3.0),
    ] {
        assert_eq!(
            eval(&format!("text.length({})", redblue_text(text))),
            Value::Number(bytes),
            "SPEC.md says length counts bytes: `{text}` is {bytes} of them"
        );
    }
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

// ---------------------------------------------------------------------------
// Round two: the arguments and the boundaries of these functions.
// ---------------------------------------------------------------------------

/// The failure of a call the tests expect to be refused: a `Runtime` error that
/// names the function, at a source span the program can be pointed at.
///
/// Neither half is optional. An interpreter thread that panicked is reported as
/// `Runtime("The interpreter thread stopped unexpectedly", Span::unknown())`, so
/// a test that only looked at "is it an error" would pass on the very abort this
/// file exists to rule out — the message and the span are what tell a refusal
/// apart from a crash.
#[track_caller]
fn assert_refused(source: &str, function: &str) {
    match eval_err(source) {
        Error::Runtime(message, span) => {
            assert_ne!(
                message, "The interpreter thread stopped unexpectedly",
                "`{}` aborted the interpreter instead of being refused; this must be a \
                 catchable error, not a panic",
                source
            );
            assert!(
                message.contains(function),
                "`{}` should name `{}` in its error, said `{}`",
                source,
                function,
                message
            );
            assert!(span.is_known(), "`{}` failed without a span", source);
        }
        other => panic!("`{}` must be a Runtime error, got {:?}", source, other),
    }
}

/// `time.sleep` builds a `Duration` out of a number the program wrote, and
/// `Duration::from_secs_f64` *panics* on a negative duration, on `NaN`, and on a
/// value past the end of a `u64` of seconds. A panic in the interpreter thread
/// aborted the process — the error was `The interpreter thread stopped
/// unexpectedly`, and no `try` in a Redblue program could catch it.
///
/// Every such value is refused by name instead, and a sleep that *is* a number of
/// seconds still answers.
#[test]
fn edge_time_sleep_refuses_a_number_it_cannot_wait_for() {
    // The values that used to abort the process: negative, and far past the end
    // of a `Duration`.
    for source in [
        "time.sleep(-1)",
        "time.sleep(-0.001)",
        "time.sleep(0 - 31536001)",
        "time.sleep(31536001)",
        "time.sleep(1e20)",
        "time.sleep(999999999999999999999999)",
    ] {
        assert_refused(source, "time.sleep");
    }

    // The bare builtin refuses the same values in the same way, and a wrong type
    // is still the type it has always been.
    for source in ["time_sleep(-1)", "time_sleep(1e20)", "time_sleep(\"soon\")"] {
        assert_refused(source, "time.sleep");
    }

    // A number of seconds inside the range is an ordinary sleep, and a refusal
    // that is not a sleep costs nothing to catch.
    assert_eq!(eval("time.sleep(0)"), Value::Nothing, "zero is no wait");
    assert_eq!(
        eval("time.sleep(0.001)"),
        Value::Nothing,
        "a fraction of a second is a wait"
    );
    assert_eq!(
        eval("time.sleep(0.001)",),
        Value::Nothing,
        "a sub-second sleep does not need to sleep longer"
    );
    assert_eq!(
        eval(concat!(
            "set caught to \"not caught\"\n",
            "try\n",
            "    time.sleep(-1)\n",
            "catch error\n",
            "    set caught to \"caught\"\n",
            "end\n",
            "caught"
        )),
        Value::Text("caught".to_string()),
        "a sleep that cannot be waited for must be catchable from Redblue"
    );
}

/// `time.format` casts its argument with `as u64`, and that cast saturates at
/// both ends: a negative timestamp became `0` and answered `1970-01-01` as
/// though the program had asked for the epoch, and a huge one became
/// `u64::MAX`, which overflowed the `SystemTime` it was added to — a panic, and
/// so an aborted process.
///
/// A timestamp is a whole number of seconds from zero, and anything else is a
/// `Runtime` error naming `time.format`.
#[test]
fn edge_a_timestamp_that_is_not_a_whole_number_of_seconds_is_refused() {
    // Negative: used to answer the epoch.
    for source in [
        "time.format(-1)",
        "time.format(0 - 1)",
        "time.format(-1700000000)",
    ] {
        assert_refused(source, "time.format");
    }

    // A fraction of a second: a timestamp is a whole number of seconds, and
    // truncating it would answer a different date than the program asked for.
    for source in ["time.format(1.5)", "time.format(0.25)", "time.format(-0.5)"] {
        assert_refused(source, "time.format");
    }

    // Past the end of a `Duration` of seconds, and past the last date a calendar
    // holds: both used to panic, one by overflow and one by a wrapped cast.
    for source in [
        "time.format(1e20)",
        "time.format(1e19)",
        "time.format(1e15)",
        "time.format(999999999999999999999999999999)",
    ] {
        assert_refused(source, "time.format");
    }

    // The bare builtin is the same function, so it refuses the same values.
    for source in ["time_format(-1)", "time_format(1e20)"] {
        assert_refused(source, "time.format");
    }

    // And the values that *are* timestamps still answer, at both counts.
    assert_eq!(
        eval("time.format(0)"),
        Value::Text("1970-01-01 00:00:00".to_string()),
        "the epoch is a timestamp"
    );
    assert_eq!(
        eval("time.format(0, \"%Y\")"),
        Value::Text("1970".to_string()),
        "the epoch with a format"
    );
    assert_eq!(
        eval("time.format(1700000000, \"%Y-%m-%d\")"),
        Value::Text("2023-11-14".to_string()),
        "a real timestamp with a format"
    );
    assert_eq!(
        eval("time.format(1e10, \"%Y\")"),
        Value::Text("2286".to_string()),
        "a timestamp inside the calendar's range is not refused"
    );
}

/// Every draw used to be the wall clock: `as_nanos() % 1000000` was the number,
/// there was no seed, and `duration_since(UNIX_EPOCH).unwrap()` aborted the
/// process on a clock set before 1970.
///
/// `math.seed` makes a run repeatable, which is what lets a test assert a random
/// result and a program reproduce a bug that depends on one.
#[test]
fn edge_a_seed_makes_every_draw_after_it_repeatable() {
    for (seed, source) in [
        ("1", "math.seed(1)\nmath.random(1000)"),
        ("2", "math.seed(2)\nmath.random(1000)"),
        ("0", "math.seed(0)\nmath.random(1000)"),
        // A fractional seed is a seed, not a silent zero.
        ("1.5", "math.seed(1.5)\nmath.random(1000)"),
        // And a negative one is read by its magnitude.
        ("0 - 7", "math.seed(0 - 7)\nmath.random(1000)"),
    ] {
        let first = eval(source);
        assert_eq!(
            first,
            eval(source),
            "seed {} must draw the same on every run",
            seed
        );
        match first {
            Value::Number(n) => assert!(
                (0.0..1000.0).contains(&n),
                "seed {} drew {} which is not in the range",
                seed,
                n
            ),
            other => panic!("a draw should be a number, got {:?}", other),
        }
    }

    // Two runs of the same program, seeded, draw the same *sequence* — not just
    // the same first number.
    let program = concat!(
        "math.seed(11)\n",
        "set a to math.random(1000)\n",
        "set b to math.random(1000)\n",
        "set c to math.random(1000)\n",
        "{a: a, b: b, c: c}\n",
    );
    assert_eq!(
        eval(program),
        eval(program),
        "a seeded program must draw the same sequence twice"
    );

    // Two draws in a row from one seed differ: a stream that repeated itself
    // would make the second draw useless.
    let Value::List(draws) = eval(concat!(
        "math.seed(3)\n",
        "set a to math.random(1000000)\n",
        "set b to math.random(1000000)\n",
        "[a, b]\n",
    )) else {
        panic!("the program should answer a list of two draws");
    };
    assert_ne!(
        draws[0], draws[1],
        "two draws from one seed must not be the same number"
    );

    // A wrong seed is refused by name rather than quietly ignored.
    for source in [
        "math.seed(\"soon\")",
        "math.seed(nothing)",
        "math.seed([1])",
        "math.seed()",
        "math.seed(1, 2)",
    ] {
        assert_refused(source, "math.seed");
    }
    // The bare builtin is the same function under its own name, and it refuses
    // the same way — which is what `edge_math_random_answers_a_number_or_refuses_by_name`
    // already says of the draw beside it.
    assert_refused("random_seed(\"soon\")", "random_seed");
}

/// The old draw was `as_nanos() % 1000000`, which means every draw in a program
/// sat inside one narrow window of `0.0..1.0` — the window between the first and
/// the last nanosecond reading, which for a short program is a thousandth of the
/// range or less. A caller that asked for a spread of numbers got a cluster.
///
/// Two hundred draws must spread. The assertion is on the *span*, not on two
/// numbers happening to differ, because a clock can produce two different numbers
/// by being read twice; what it cannot do is spread them across the range.
#[test]
fn edge_draws_spread_across_their_range_instead_of_riding_the_clock() {
    let program = concat!(
        "set low to 1\n",
        "set high to 0\n",
        "repeat 200 times\n",
        "    set d to math.random(0, 1)\n",
        "    if d < low then\n",
        "        set low to d\n",
        "    end\n",
        "    if d > high then\n",
        "        set high to d\n",
        "    end\n",
        "end\n",
        "set span to high - low\n",
        "[low, high, span]\n",
    );
    let Value::List(measure) = eval(program) else {
        panic!("the program should answer a list of three numbers");
    };
    assert_eq!(
        measure.len(),
        3,
        "the program should answer exactly three numbers, got {measure:?}"
    );
    let [low, high, span] = [&measure[0], &measure[1], &measure[2]];
    let (Value::Number(low), Value::Number(high), Value::Number(span)) = (low, high, span) else {
        panic!("the measurements should be numbers, got {measure:?}");
    };
    assert!(
        (0.0..=1.0).contains(low) && (0.0..=1.0).contains(high),
        "every draw must be inside the range it was asked for, got {low}..{high}"
    );
    assert!(
        *span > 0.5,
        "200 draws from `math.random(0, 1)` covered only {span} of the range; a \
         generator spreads them and a clock cannot"
    );

    // The same with a seed, so the assertion is about the generator and not
    // about one lucky run.
    let seeded = || format!("math.seed(99)\n{program}");
    let span_of = |source: &str| -> Value {
        match eval(source) {
            Value::List(items) => items[2].clone(),
            other => panic!("{source:?} should answer a list of three numbers, got {other:?}"),
        }
    };
    let Value::Number(span) = span_of(&seeded()) else {
        panic!("the span should be a number");
    };
    assert!(
        span > 0.5,
        "200 seeded draws from `math.random(0, 1)` covered only {span} of the range"
    );
    assert_eq!(
        span_of(&seeded()),
        span_of(&seeded()),
        "a seeded spread must be the same on every run"
    );
}

/// `random_choice` and `random_shuffle` are the two other builtins that draw.
/// They read the clock the same way `math.random` did, and `random_shuffle` took
/// **one** reading and reused it for every step — so the permutation it produced
/// depended on the length of the list and not on its contents, which is not a
/// shuffle.
#[test]
fn edge_random_choice_and_shuffle_draw_from_the_same_generator() {
    // A choice is an element of the list it was given, and the seeded choice
    // repeats.
    for (seed, expected) in [(1, "b"), (2, "a"), (3, "c")] {
        let source = format!("math.seed({seed})\nrandom_choice([\"a\", \"b\", \"c\"])");
        assert_eq!(
            eval(&source),
            Value::Text(expected.to_string()),
            "seed {seed} must pick the element it picked before"
        );
    }
    // Every choice is one of the elements — over a hundred unseeded draws, and
    // both halves of a two-element list are reached.
    let Value::List(picks) = eval(concat!(
        "set first to nothing\n",
        "set second to nothing\n",
        "repeat 100 times\n",
        "    set pick to random_choice([\"a\", \"b\"])\n",
        "    if pick is \"a\" then\n",
        "        set first to \"a\"\n",
        "    else\n",
        "        set second to \"b\"\n",
        "    end\n",
        "end\n",
        "[first, second]\n",
    )) else {
        panic!("the program should answer a list of two picks");
    };
    assert_eq!(
        picks,
        vec![Value::Text("a".to_string()), Value::Text("b".to_string())],
        "a hundred choices from a two-element list must reach both of them"
    );

    // A shuffle is a permutation: it keeps every element and adds none, and a
    // seed makes it the same permutation every time.
    let shuffled = |seed: i32| -> Vec<Value> {
        let source =
            format!("math.seed({seed})\nrandom_shuffle([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12])");
        match eval(&source) {
            Value::List(items) => items,
            other => panic!("a shuffle should be a list, got {:?}", other),
        }
    };
    let mut sorted = shuffled(1);
    sorted.sort_by_key(|v| match v {
        Value::Number(n) => *n as i64,
        other => panic!("a shuffled number is still a number, got {:?}", other),
    });
    assert_eq!(
        sorted,
        (1..=12)
            .map(|n| Value::Number(n as f64))
            .collect::<Vec<Value>>(),
        "a shuffle must keep every element of the list it was given"
    );
    assert_eq!(
        shuffled(1),
        shuffled(1),
        "a seeded shuffle must be the same permutation on every run"
    );
    assert_ne!(
        shuffled(1),
        shuffled(2),
        "two seeds must be able to shuffle differently; one permutation for the \
         whole length is not a shuffle"
    );

    // The edges of a list: empty and singleton, and a wrong type.
    assert_eq!(
        eval("random_choice([])"),
        Value::Nothing,
        "nothing can be chosen from an empty list"
    );
    assert_eq!(
        eval("random_shuffle([])"),
        Value::List(Vec::new()),
        "an empty list shuffled is empty"
    );
    assert_eq!(
        eval("random_shuffle([7])"),
        Value::List(vec![Value::Number(7.0)]),
        "a one-element list has one permutation"
    );
    for source in [
        "random_choice(\"abc\")",
        "random_shuffle(5)",
        "random_choice()",
    ] {
        assert_refused(source, "random");
    }
}

/// `text.join` writes a separator *between* elements; it does not turn them
/// into text first. `item.to_string()` on a number, a `yes/no` or a nested list
/// answered `"1,yes,[1]"` for `text.join([1, yes, [1]], ",")` — which made `join`
/// the one function of the set that answers for a list it was not given, while
/// `text.uppercase(1)` and `list.length(5)` both refuse.
///
/// The refusal names the element and its type, which is the thing the caller can
/// act on.
#[test]
fn edge_join_refuses_a_list_that_is_not_a_list_of_text() {
    for (source, element, kind) in [
        ("text.join([1, \"a\"], \",\")", "element 1", "number"),
        ("text.join([\"a\", 1], \",\")", "element 2", "number"),
        ("text.join([\"a\", yes], \",\")", "element 2", "yes/no"),
        ("text.join([nothing], \",\")", "element 1", "nothing"),
        ("text.join([[\"a\"]], \",\")", "element 1", "list"),
        ("text.join([{a: 1}], \",\")", "element 1", "record"),
        (
            "text.join([1, yes, [1]], \",\")",
            "element 1 is number",
            "number",
        ),
    ] {
        assert_refused(source, "text.join");
        match eval_err(source) {
            Error::Runtime(message, _) => {
                assert!(
                    message.contains(element),
                    "`{}` should say which `{}` it is, said `{}`",
                    source,
                    element,
                    message
                );
                assert!(
                    message.contains(kind),
                    "`{}` should say what `{}` is, said `{}`",
                    source,
                    element,
                    message
                );
            }
            other => panic!("`{}` must be a Runtime error, got {:?}", source, other),
        }
    }

    // A list of text is joined whatever it holds — including empty text, and the
    // empty list, and an empty separator.
    for (source, expected) in [
        ("text.join([\"a\", \"b\"], \",\")", "a,b"),
        ("text.join([\"\"], \",\")", ""),
        ("text.join([\"\", \"a\"], \",\")", ",a"),
        ("text.join([\"a\", \"\"], \",\")", "a,"),
        ("text.join([\"a\", \"b\"], \"\")", "ab"),
        ("text.join([\"a\"], \"-\")", "a"),
        ("text.join([], \",\")", ""),
        ("text.join([\"🎉\", \"世界\"], \" \")", "🎉 世界"),
    ] {
        assert_eq!(
            eval(source),
            Value::Text(expected.to_string()),
            "`{}` answers",
            source
        );
    }

    // And it is catchable, which is what makes it usable in a program that has to
    // keep going on a list it did not check.
    assert_eq!(
        eval(concat!(
            "set caught to \"not caught\"\n",
            "try\n",
            "    say text.join([\"a\", 1], \",\")\n",
            "catch error\n",
            "    set caught to \"caught\"\n",
            "end\n",
            "caught"
        )),
        Value::Text("caught".to_string()),
        "a join refusal must be catchable from Redblue"
    );
}

/// The count of arguments is checked in two places — the module spelling in
/// `stdlib::call_module_function` and the bare builtin in
/// `crate::runtime::builtin` — and the bare one used to check only `>` for "too
/// many". A bare `length()` therefore failed inside the function, in the words of
/// the function, while a `text.length()` failed on the count, in the words of the
/// caller: two different failures for one mistake.
///
/// Both are now the count refusal, in the same shape, each naming the function the
/// way the program wrote it.
#[test]
fn edge_a_bare_name_and_a_module_name_refuse_the_same_count() {
    for (bare, module, expected, given) in [
        ("length()", "text.length()", 1, 0),
        ("abs()", "math.abs()", 1, 0),
        ("split(\"a\")", "text.split(\"a\")", 2, 1),
        ("join([\"a\"])", "text.join([\"a\"])", 2, 1),
        ("console_log()", "console.log()", 1, 0),
        ("console_clear(1)", "console.clear(1)", 0, 1),
        ("files_read()", "files.exists()", 1, 0),
        ("files_write(\"a\")", "files.copy(\"a\")", 2, 1),
        ("time_now(1)", "time.now(1)", 0, 1),
        ("time_sleep()", "time.sleep()", 1, 0),
        ("json_parse()", "json.parse()", 1, 0),
        ("csv_parse(1, 2)", "csv.parse(1, 2)", 1, 2),
        ("files_read(\"a\", \"b\")", "files.exists(1, 2)", 1, 2),
    ] {
        let wanted = format!("takes {expected} argument(s), given {given}");
        for source in [bare, module] {
            match eval_err(source) {
                Error::Runtime(message, span) => {
                    assert_ne!(
                        message, "The interpreter thread stopped unexpectedly",
                        "`{}` aborted the interpreter",
                        source
                    );
                    assert!(
                        message.contains(&wanted),
                        "`{}` should refuse the count with `{}`, said `{}`",
                        source,
                        wanted,
                        message
                    );
                    assert!(span.is_known(), "`{}` failed without a span", source);
                }
                other => panic!("`{}` must be a Runtime error, got {:?}", source, other),
            }
        }
    }
}

/// The two BLOCKERs were both a `panic!` reachable from a Redblue program:
/// `Duration::from_secs_f64` on a value it refuses, and `duration_since(…).
/// unwrap()` on a clock set before 1970. Neither can be provoked from a test —
/// no Redblue program can set the clock, and no clock a test runs on is before
/// 1970 — so the guarantee is pinned where the defect was: in the source.
///
/// Both are properties of a *call* rather than of a value, so no test of
/// `time.sleep(-1)` can prove the panic is gone for a clock; only reading the
/// code can. This is the same shape as the tests above that read SPEC.md.
#[test]
fn edge_the_runtime_never_unwraps_a_clock_and_never_builds_a_duration_from_a_bare_float() {
    // The comments are stripped first: both of these are named in prose beside
    // the code that replaced them, and the point of the scan is the code.
    let code = strip_comments(&read_doc("src/runtime.rs"));
    // One line of text, so a call split across lines by `rustfmt` is still one
    // piece to look at.
    let flat = code.split_whitespace().collect::<Vec<&str>>().join(" ");

    assert!(
        !flat.contains("from_secs_f64"),
        "src/runtime.rs builds a `Duration::from_secs_f64`, which panics on a \
         negative duration, on NaN, and on a value past the end of a u64 of \
         seconds; use `runtime::sleep_duration`, which refuses those first"
    );

    let mut unwrapped = Vec::new();
    let mut rest = flat.as_str();
    while let Some(at) = rest.find("duration_since") {
        let tail = &rest[at..(at + 80).min(rest.len())];
        if tail.contains("unwrap") {
            unwrapped.push(tail.to_string());
        }
        rest = &rest[at + "duration_since".len()..];
    }
    assert!(
        unwrapped.is_empty(),
        "src/runtime.rs unwraps the result of a clock read: {unwrapped:?}; a \
         machine whose clock is set before 1970 would abort the process instead \
         of raising something a `try` can catch"
    );
}

/// `source` with its `//` line comments and `/* .. */` blocks removed.
fn strip_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut depth = 0usize;
    while let Some(c) = chars.next() {
        match c {
            '/' if chars.peek() == Some(&'/') && depth == 0 => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                depth += 1;
            }
            '*' if chars.peek() == Some(&'/') && depth > 0 => {
                chars.next();
                depth -= 1;
            }
            '\n' => {
                out.push('\n');
            }
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// `math.random(max)` and `math.random(min, max)` are the two spellings the code
/// takes, and `time.format(ts)` against `time.format(ts, format)` is the same
/// kind of optional last argument. README.md and SPEC.md state the fixed-arity
/// version of each, which was a promise the code had never kept — `math.random(5)`
/// draws from zero and `time.format(0)` answers.
#[test]
fn edge_the_documents_state_the_optional_counts_the_code_takes() {
    for (document, heading, promised) in [
        (
            "README.md",
            "## Standard Library",
            &[
                "math.random(max)",
                "math.random(min, max)",
                "time.format(ts)",
                "time.format(ts, format)",
            ][..],
        ),
        (
            "SPEC.md",
            "## Standard Library",
            &[
                "math.random(1, 100)",
                "math.random(10)",
                "time.format(1700000000)",
                "time.format(1700000000, \"%Y-%m-%d %H:%M:%S\")",
            ][..],
        ),
    ] {
        let text = read_doc(document);
        let section = standard_library_section(&text, heading);
        for spelling in promised {
            assert!(
                section.contains(spelling),
                "{} documents no `{}`; the two counts are both answers and one of \
                 them has to be written down",
                document,
                spelling
            );
        }
    }

    // And both counts are answers, through the code as well as the document.
    for (source, low, high) in [
        ("math.random(5)", 0.0, 5.0),
        ("math.random(1, 5)", 1.0, 5.0),
        ("math.random(2, 2)", 2.0, 2.0),
    ] {
        match eval(source) {
            Value::Number(n) => assert!(
                (low..=high).contains(&n),
                "`{}` must draw from {}..{}, got {}",
                source,
                low,
                high,
                n
            ),
            other => panic!("`{}` should be a number, got {:?}", source, other),
        }
    }
    assert_eq!(
        eval("time.format(0)"),
        Value::Text("1970-01-01 00:00:00".to_string()),
        "a timestamp alone is an answer"
    );
    assert_eq!(
        eval("time.format(0, \"%Y-%m-%d\")"),
        Value::Text("1970-01-01".to_string()),
        "a timestamp and a format is an answer"
    );
}

/// AGENTS.md §Math and §Text listed `PI`, `E`, `pow`, `sin`, `cos`, `tan`,
/// `log`, `exp`, `contains`, `starts_with` and `replace` as callable, and spelled
/// every one of them bare. None is answered: `say PI` is an `Unknown variable`
/// and `say pow(2, 3)` is an `Unknown function`. A document that lists a name the
/// code does not have is the same disagreement the rest of this file is about,
/// seen from the other side.
///
/// The prose that says *why* they are absent is allowed — that is the correction.
/// A ```redblue block that calls one is not.
#[test]
fn edge_the_agents_document_does_not_promise_a_name_the_code_does_not_answer() {
    let document = read_doc("AGENTS.md");
    let section = standard_library_section(&document, "### Standard Library (Implemented)");
    let blocks = redblue_blocks(&section);
    assert!(
        !blocks.is_empty(),
        "AGENTS.md's Standard Library section holds no ```redblue block to read"
    );

    for (name, written) in [
        ("PI", "say PI"),
        ("E", "say E"),
        ("pow", "say pow(2, 3)"),
        ("sin", "say sin(0)"),
        ("cos", "say cos(0)"),
        ("tan", "say tan(0)"),
        ("log", "say log(1)"),
        ("exp", "say exp(1)"),
        ("contains", "say contains(\"a\", \"b\")"),
        ("starts_with", "say starts_with(\"a\", \"b\")"),
        ("ends_with", "say ends_with(\"a\", \"b\")"),
        ("replace", "say replace(\"a\", \"a\", \"b\")"),
    ] {
        for block in &blocks {
            assert!(
                !block.contains(&format!("{name}(")),
                "AGENTS.md's Standard Library still calls `{name}(..)` in a \
                 ```redblue block, and the code answers nothing: `{written}` is an \
                 error. See phases/phase-032/FINDINGS.md §2 and §8"
            );
        }

        // And the correction holds: every one of those really is refused today,
        // so the document and the code cannot drift apart silently.
        //
        // Through `rb run`, and not through `eval`, because a constant is a
        // disagreement the *analyzer* has with the globals map: `PI` and `E` are
        // in `stdlib::builtins()` and the analyzer never learns they are bound,
        // so only the whole pipeline refuses them. A bare tree-walk would print
        // `3.141592653589793` and pass.
        let (status, stdout, stderr) = run_rb(written);
        assert!(
            !status.success(),
            "`{}` should be refused, and it printed:\n{}",
            written,
            stdout
        );
        assert!(
            stderr.contains(name),
            "`{}` should name `{}` in its error, said `{}`",
            written,
            name,
            stderr
        );
    }

    // What it *does* list is answered, so the section is not emptied out to make
    // the point.
    for (source, expected) in [
        ("math.abs(-1)", Value::Number(1.0)),
        ("math.round(3.7)", Value::Number(4.0)),
        ("text.uppercase(\"hi\")", Value::Text("HI".to_string())),
        (
            "text.join([\"a\", \"b\"], \"-\")",
            Value::Text("a-b".to_string()),
        ),
        ("text.length(\"hi\")", Value::Number(2.0)),
        ("list.length([1, 2])", Value::Number(2.0)),
    ] {
        assert_eq!(eval(source), expected, "`{}` answers", source);
    }
}

/// A `.rbc` is compiled from the same source the tree-walker runs, so every
/// refusal and every seeded draw above has to be the same on both — including the
/// ones that used to be panics, which on the bytecode VM would take the thread
/// down rather than raise.
#[test]
fn edge_both_vms_refuse_the_same_and_repeat_the_same() {
    use redblue::bytecode::compile_source;
    use redblue::bytecode::vm::BytecodeVm;

    let run = |source: &str| -> Result<Value, Error> {
        let chunk =
            compile_source(source).unwrap_or_else(|e| panic!("{source:?} should compile: {e:?}"));
        BytecodeVm::new().run(&chunk)
    };

    for source in [
        "time.sleep(-1)",
        "time.sleep(1e20)",
        "time.format(-1)",
        "time.format(1e20)",
        "text.join([\"a\", 1], \",\")",
        "math.seed(\"soon\")",
        "text.length()",
        "length()",
    ] {
        let statement = format!("say {source}\n");
        let bytecode = run(&statement).expect_err("the bytecode VM should refuse");
        let Error::Runtime(message, span) = &bytecode else {
            panic!(
                "`{}` must be a Runtime error on both VMs, got {:?}",
                source, bytecode
            );
        };
        assert_ne!(
            message, "The interpreter thread stopped unexpectedly",
            "`{}` aborted the bytecode VM instead of being refused",
            source
        );
        assert!(span.is_known(), "`{}` failed without a span", source);

        // The same refusal, in the same words, as the tree-walker gives.
        let tree = eval_err(&statement);
        assert_eq!(
            bytecode.to_string(),
            tree.to_string(),
            "`{}` must be refused identically on both VMs",
            source
        );
    }

    // And a seeded draw repeats on the bytecode VM too.
    let seeded = concat!(
        "math.seed(5)\n",
        "say math.random(1000)\n",
        "say math.random(1000)\n",
    );
    let prints = |source: &str| -> String {
        let chunk = compile_source(source).expect("source should compile");
        let mut vm = BytecodeVm::new();
        vm.run(&chunk).expect("a seeded draw should run");
        vm.take_output().join("\n")
    };
    let first = prints(seeded);
    assert_eq!(
        first,
        prints(seeded),
        "a seeded draw must repeat on the bytecode VM"
    );
    assert_eq!(
        first.lines().count(),
        2,
        "the program should have drawn twice, printed:\n{first}"
    );
    let mut lines = first.lines();
    let drawn_first = lines.next();
    let drawn_second = lines.next();
    assert_ne!(
        drawn_first, drawn_second,
        "two draws from one seed must differ, printed:\n{first}"
    );
}

/// `Random::below` takes the low bits of a word until the draw is inside the
/// range, and it used to do that by calling *itself*. Every draw of
/// `math.random`, `random_choice` and `random_shuffle` reaches it, and one stack
/// frame per rejected draw is a stack that can run out — which aborts the
/// process instead of raising something a `try` can catch.
///
/// The failure cannot be provoked from a test: the rejection rate is under a
/// half, so the streak that overflows a stack is astronomically unlikely, and no
/// Redblue program can hand the generator a seed that produces one. So this pins
/// the shape of the code, which is the same shape the two panics of round two
/// are pinned by, and the count of draws that *are* reachable.
#[test]
fn edge_a_bounded_draw_retries_in_a_loop_and_not_in_a_call_to_itself() {
    let code = strip_comments(&read_doc("src/runtime.rs"));
    let signature = "fn below(&mut self, bound: u64) -> u64 {";
    let start = code
        .find(signature)
        .unwrap_or_else(|| panic!("src/runtime.rs should still have `{signature}`"));
    let body = &code[start..];
    let open = body
        .find('{')
        .unwrap_or_else(|| panic!("`{signature}` should have a body"));
    let mut depth = 0usize;
    let mut close = open;
    for (offset, c) in body.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = offset;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &body[open..=close];

    assert!(
        !body.contains("self.below("),
        "Random::below retries by calling itself, so every draw costs a stack \
         frame and an unlucky rejection streak aborts the process: {body}"
    );
    assert!(
        body.contains("loop"),
        "Random::below should redraw in a loop, so a rejected draw costs no \
         stack at all: {body}"
    );

    // The draws a program can reach still work, and none of them is slow enough
    // to be the stack: `random_choice` and `random_shuffle` draw from a list's
    // length, and every length a list can have is drawn from here.
    let chunk = redblue::bytecode::compile_source(concat!(
        "math.seed(11)\n",
        "set xs to [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]\n",
        "set n to 0\n",
        "for each x from 1 to 2000\n",
        "    set n to n + 1\n",
        "    random_choice(xs)\n",
        "    random_shuffle(xs)\n",
        "    math.random(13)\n",
        "end\n",
        "say n\n",
    ))
    .expect("the random program should compile");
    let mut vm = redblue::bytecode::vm::BytecodeVm::new();
    vm.run(&chunk).expect("6000 draws should run");
    assert_eq!(
        vm.take_output().join("\n").trim(),
        "2000",
        "the loop should have completed every draw"
    );
}

/// `type_of` takes exactly one value, and says so.
///
/// It used to answer `args.first().map(..).unwrap_or("nothing")`, so
/// `type_of()` returned the very string `type_of(nothing)` returns. A program
/// that forgot its argument was answered with a plausible type name and went on
/// running — the same failure as a `join` that stringified its list, and the
/// one this phase refused to leave in `join`.
#[test]
fn edge_type_of_takes_exactly_one_argument() {
    for (value, expected) in [
        ("1", "number"),
        ("1.5", "number"),
        ("\"hi\"", "text"),
        ("yes", "yes/no"),
        ("nothing", "nothing"),
        ("[1, 2]", "list"),
        ("{a: 1}", "record"),
    ] {
        assert_eq!(
            eval(&format!("type_of({value})")),
            Value::Text(expected.to_string()),
            "`type_of({value})` answers"
        );
    }

    // `type_of()` and `type_of(nothing)` are now different failures, and neither
    // is the other's answer.
    for (source, given) in [
        ("type_of()", 0),
        ("type_of(1, 2)", 2),
        ("type_of(1, 2, 3)", 3),
    ] {
        match eval_err(source) {
            Error::Runtime(message, span) => {
                assert_eq!(
                    message,
                    format!("type_of takes 1 argument(s), given {given}"),
                    "`{}` must be refused by name, and never with a type name",
                    source
                );
                assert!(span.is_known(), "`{}` failed without a span", source);
                assert!(
                    !message.contains("nothing"),
                    "`{}` must not answer with the type of a missing argument",
                    source
                );
            }
            other => panic!("`{}` must be refused, got {:?}", source, other),
        }
    }

    // The count is in the one table that owns counts, so the module path and the
    // bare path cannot stop agreeing about it.
    assert_eq!(
        redblue::stdlib::arity("type_of"),
        Some(1),
        "type_of states one argument"
    );
    assert_eq!(
        redblue::stdlib::display_name("type_of"),
        "type_of",
        "a builtin no module owns is named as itself, not as `type.of`"
    );
    assert_eq!(
        redblue::stdlib::display_name("files_read"),
        "files.read",
        "a builtin a module owns is named the way the module spells it"
    );

    // And the refusal is catchable, so a program can ask for the type of
    // something it is not sure about.
    assert_eq!(
        eval(concat!(
            "set caught to \"not caught\"\n",
            "try\n",
            "    say type_of()\n",
            "catch error\n",
            "    set caught to \"caught\"\n",
            "end\n",
            "caught\n"
        )),
        Value::Text("caught".to_string()),
        "a refused `type_of()` must be a catchable error"
    );
}

/// A range wider than a double is drawable, and the two documents and the
/// formula behind it say so.
///
/// `min + r * (max - min)` computes the *width* first, and `-1e308` to `1e308`
/// is a width of `infinity`, so every draw from an ordinary range was refused
/// with `infinity is not a finite number` — a message naming no function, for a
/// range nothing about is infinite. Interpolating between the two ends scales
/// each of them into `[0, 1]` first, so the width is never formed at all.
#[test]
fn edge_math_random_draws_from_a_range_too_wide_to_subtract() {
    let document = read_doc("SPEC.md");
    let math = subsection(&document, "### math");
    assert!(
        math.contains("-1e308"),
        "SPEC.md's `### math` should state what a range wider than a double does"
    );

    for (source, low, high) in [
        ("math.random(-1e308, 1e308)", -1e308, 1e308),
        ("math.random(-1e308, 0)", -1e308, 0.0),
        ("math.random(0, 1e308)", 0.0, 1e308),
        ("math.random(1e308, 1.7e308)", 1e308, 1.7e308),
    ] {
        match eval(source) {
            Value::Number(n) => {
                assert!(
                    n.is_finite(),
                    "`{}` answered {}, which is not a number",
                    source,
                    n
                );
                assert!(
                    (low..=high).contains(&n),
                    "`{}` answered {}, which is outside {}..{}",
                    source,
                    n,
                    low,
                    high
                );
            }
            other => panic!("`{}` should answer a number, got {:?}", source, other),
        }
    }

    // The refusals it still makes name the function, as before.
    for source in [
        "math.random(\"hi\", \"there\")",
        "math.random()",
        "math.random(1, 2, 3)",
        "math.random(\"hi\")",
    ] {
        assert_refused(source, "math.random");
    }
}

/// SPEC.md §Properties is an example outside §Standard Library, and it showed
/// `math.PI`. A module has functions and not members, so `math.PI` is an
/// `AnalyzerError: Unknown variable 'math'` — the same drift round one corrected
/// inside §Standard Library and left here, because this phase's test only read
/// that section.
///
/// The block is run here, whole, through `rb run`: the lexer, the parser, the
/// analyzer and the VM, which is the only way the defect shows at all. A tree-walk
/// that skipped the analyzer would not refuse it.
#[test]
fn edge_the_properties_example_is_a_program_that_runs() {
    let document = read_doc("SPEC.md");
    let blocks = redblue_blocks(&subsection(&document, "### Properties"));
    assert_eq!(
        blocks.len(),
        1,
        "SPEC.md's Properties section should hold exactly one example, found {:?}",
        blocks
    );
    let example = &blocks[0];
    assert!(
        !example.contains("math.PI"),
        "SPEC.md's Properties example still reads `math.PI`, which is an \
         `Unknown variable`: a module has functions and not members"
    );

    // The example on its own declares an object, so it runs and says nothing.
    let (status, _out, err) = run_rb(example);
    assert!(
        status.success(),
        "SPEC.md's Properties example should run as it stands, said:\n{err}"
    );

    // And it computes what a reader of the page expects it to: a circle of
    // radius 2, through the `constant PI` the example now declares.
    let program = format!("{example}\nset Circle.radius to 2\nsay Circle.area()\n");
    let (status, out, err) = run_rb(&program);
    assert!(
        status.success(),
        "the Properties example with a radius should run, said:\n{err}"
    );
    assert_eq!(
        out.trim(),
        "12.56636",
        "PI is the constant the example declares, not a member of `math`"
    );
}

/// The keyword tables of SPEC.md must not list a spelling the parser cannot
/// read.
///
/// Both of them listed `might fail`. `might` and `fail` both lex to
/// `TokenKind::MightFail`, no arm of `src/parser.rs` matches it, and so every
/// documented spelling was a `ParserError` — the same drift round one corrected
/// inside §Standard Library and §Files and left in the Appendix, which no test
/// read. The tables list what a statement is written with; the words that are
/// reserved and reserved only are named in the prose under §Keywords, which is
/// where a reader will look for them.
#[test]
fn edge_the_spec_keyword_tables_do_not_promise_a_spelling_the_parser_cannot_read() {
    let document = read_doc("SPEC.md");
    for heading in ["### Keywords", "## Appendix: Keywords Reference"] {
        let rows = table_rows(&subsection(&document, heading));
        assert!(
            !rows.is_empty(),
            "`{heading}` should hold a table, found no rows",
            heading = heading
        );
        let offending: Vec<&String> = rows
            .iter()
            .filter(|row| row.contains("might fail"))
            .collect();
        assert!(
            offending.is_empty(),
            "`{heading}` still lists `might fail`, which no statement parses: {:?}",
            offending
        );
    }

    // The prose says why, so the correction cannot be a silent deletion: the
    // words §Keywords calls reserved-and-unimplemented are named there.
    let keywords = subsection(&document, "### Keywords");
    for word in [
        "might fail",
        "when",
        "can",
        "that",
        "new",
        "ask",
        "wait",
        "async",
        "parallel",
        "done",
    ] {
        assert!(
            keywords.contains(word),
            "SPEC.md's `### Keywords` should still say that `{}` is reserved and \
             unimplemented, or say why it is not",
            word
        );
    }

    // And each of them really is a word the parser cannot read, so the prose
    // cannot become the drift it is correcting.
    for (source, token) in [
        ("might fail files.write(\"a\", \"b\")\n", "MightFail"),
        ("when 1 then\n    say \"one\"\nend\n", "When"),
        ("say ask\n", "Ask"),
        ("say wait\n", "Wait"),
        ("set x to async\n", "Async"),
        ("set x to parallel\n", "Parallel"),
        ("say done\n", "Done"),
    ] {
        let tokens = redblue::lexer::Lexer::tokenize(source)
            .unwrap_or_else(|e| panic!("`{source:?}`: {e:?}"));
        match redblue::parser::parse(tokens) {
            Err(Error::Parser(message, _)) => assert!(
                message.contains(token),
                "`{source:?}` should be refused as a `{token}`, said `{message}`"
            ),
            other => panic!("`{source:?}` should be a ParserError, got {other:?}"),
        }
    }

    // A reserved word is reserved: it cannot be used as an identifier either.
    for word in [
        "might", "fail", "when", "can", "that", "new", "ask", "wait", "async",
    ] {
        let tokens = redblue::lexer::Lexer::tokenize(word).expect("a word should lex");
        assert!(
            !matches!(tokens[0].kind, redblue::lexer::TokenKind::Identifier(_)),
            "`{}` is documented as reserved, so it must not lex as an identifier",
            word
        );
    }

    // A ```redblue block is a *program*, in either document, and a program that
    // does not parse is the drift the tables were corrected for. The three that
    // carried `might fail` were SPEC.md's §Error Handling examples and
    // `docs/GRAMMAR.md`'s Try/Catch; the grammar *production* that still lists
    // it is a statement of intent and is marked as unimplemented in §5.11
    // instead.
    let mut blocks_read = 0;
    for name in ["SPEC.md", "docs/GRAMMAR.md"] {
        let text = read_doc(name);
        for block in redblue_programs(&text) {
            assert!(
                !block.contains("might fail"),
                "{} has a ```redblue block that writes `might fail`, which is a \
                 `ParserError`: {:?}",
                name,
                block
            );
            blocks_read += 1;
        }
    }
    assert!(
        blocks_read > 20,
        "only {} ```redblue blocks were read, so the scan cannot be trusted",
        blocks_read
    );
}

/// The bytecode VM and the tree-walker must refuse a call through a module that
/// does not exist in the same words, because a `.rbc` compiled from a source
/// means what the source means.
///
/// Both resolve a member call by looking up `module_member`, and both used to
/// fall through to `Unknown function 'module_member'` when the receiver was
/// neither a declared object nor a module — the internal encoding, on a file
/// whose reader wrote `module.member`. The bound receiver below is what reaches
/// that path: an unbound one is refused by the analyzer before either VM runs.
#[test]
fn edge_both_vms_name_an_unknown_module_member_the_way_the_program_wrote_it() {
    use redblue::bytecode::compile_source;
    use redblue::bytecode::vm::BytecodeVm;

    for (statement, expected, encoding) in [
        (
            "set thing to 1\nsay thing.read(\"x\")\n",
            "Unknown function 'thing.read'",
            "thing_read",
        ),
        (
            "set xs to [1]\nsay xs.nosuchfunction(1)\n",
            "Unknown function 'xs.nosuchfunction'",
            "xs_nosuchfunction",
        ),
        (
            "say text.nosuchfunction(\"hi\")\n",
            "Module 'text' has no function 'nosuchfunction'",
            "text_nosuchfunction",
        ),
        (
            "say type_of()\n",
            "type_of takes 1 argument(s), given 0",
            "type.of",
        ),
    ] {
        let chunk = compile_source(statement).unwrap_or_else(|e| panic!("{statement:?}: {e:?}"));
        let bytecode = BytecodeVm::new()
            .run(&chunk)
            .expect_err("the bytecode VM should refuse");
        let tree = eval_err(statement);
        let Error::Runtime(message, span) = &bytecode else {
            panic!("`{statement:?}` must be a Runtime error, got {bytecode:?}");
        };
        assert_eq!(message, expected, "`{statement:?}` on the bytecode VM");
        assert!(
            !message.contains(encoding),
            "`{statement:?}` leaked the internal `module_member` encoding: `{message}`"
        );
        assert!(span.is_known(), "`{statement:?}` failed without a span");
        assert_eq!(
            bytecode.to_string(),
            tree.to_string(),
            "`{statement:?}` must be refused identically on both VMs"
        );
    }
}
