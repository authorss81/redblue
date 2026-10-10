//! `text.split` and `text.join`: what each answers, and which stdlib names are
//! reachable dotted rather than only flat.
//!
//! The finding this file pins, verified against the built binary before any
//! change:
//!
//! * `text.split("a,b", by ",")` — the spelling SPEC.md's `### text` section
//!   writes (`SPEC.md:1017`) — is **refused before it runs**:
//!   `AnalyzerError: Unknown variable 'by'`. `by` is a *positional marker* in a
//!   range loop's step (`for each i from 0 to 6 by 2`), never a named-argument
//!   label, so the call argument list has no production for it and `by` parses
//!   as an ordinary variable read that nothing binds.
//! * `text.split("a,b", ",")` does work with **or without** `import text`: the
//!   receiver of a method call is accepted when it names a module, and the
//!   import is optional rather than required (pinned below by
//!   `text_split_reaches_dotted_without_an_import_and_that_is_the_rule`). A
//!   bare read of the name — `say text` — is still refused as
//!   `Unknown variable 'text'`.
//! * The flat `split("a,b", ",")` needs no import at all.
//!
//! So the three spellings a reader of SPEC.md might write have three different
//! answers, and none of them says which. That is what this file fixes: both
//! spellings SPEC.md uses now work, and every stdlib name's reachability is
//! pinned by a table rather than left to be discovered one failure at a time.
//!
//! The file also pins both sides of the `by` label: what it labels, where, and
//! what a `by` that is *not* a label still means.

use std::path::{Path, PathBuf};
use std::process::Command;

use redblue::bytecode::vm::BytecodeVm;
use redblue::{compile_source, Error, Value};

/// Runs `source` through the whole pipeline — lexer, parser, **analyzer**, VM —
/// and returns its last value.
///
/// The analyzer is in this path deliberately: two of the findings this file
/// pins (`Unknown variable 'by'`, `Unknown variable 'text'`) are analyzer
/// refusals, and a helper that parsed and ran without it would not be able to
/// fail on either.
#[track_caller]
fn eval(source: &str) -> Value {
    redblue::run_source_value(source)
        .unwrap_or_else(|error| panic!("source should have run, failed with {error:?}\n{source}"))
}

/// Runs `source` and returns the error the pipeline produced.
#[track_caller]
fn eval_err(source: &str) -> Error {
    redblue::run_source(source).expect_err("source should have failed")
}

/// A list of text cells, the shape `split` produces.
#[track_caller]
fn cells(parts: &[&str]) -> Value {
    Value::list(
        parts
            .iter()
            .map(|p| Value::Text((*p).to_string()))
            .collect::<Vec<Value>>(),
    )
}

// ---------------------------------------------------------------------------
// The table the phase asks for. Each row is one `(subject, separator, answer)`
// triple, exercised through BOTH spellings SPEC.md writes and BOTH engines.
// ---------------------------------------------------------------------------

/// One `split` case: the subject, the separator, and what it must produce.
struct SplitCase {
    what: &'static str,
    subject: &'static str,
    separator: &'static str,
    /// `None` means the case has no list answer and must be `nothing`.
    answer: Option<&'static [&'static str]>,
}

/// One `join` case: the list, the separator, and the text it must produce.
struct JoinCase {
    what: &'static str,
    items: &'static [&'static str],
    separator: &'static str,
    answer: &'static str,
}

const SPLIT_CASES: &[SplitCase] = &[
    SplitCase {
        what: "an empty subject",
        subject: "",
        separator: ",",
        answer: Some(&[""]),
    },
    SplitCase {
        what: "a subject equal to the separator",
        subject: ",",
        separator: ",",
        answer: Some(&["", ""]),
    },
    SplitCase {
        what: "a separator absent from the subject",
        subject: "abc",
        separator: ",",
        answer: Some(&["abc"]),
    },
    SplitCase {
        what: "adjacent separators",
        subject: "a,,b",
        separator: ",",
        answer: Some(&["a", "", "b"]),
    },
    SplitCase {
        what: "a three-character separator",
        subject: "a::b::c",
        separator: "::",
        answer: Some(&["a", "b", "c"]),
    },
    SplitCase {
        what: "an empty separator",
        subject: "abc",
        separator: "",
        // Redblue has no regex, so an empty separator would make every
        // position a boundary and never advance. `src/stdlib.rs:481` turns
        // that "no answer" into a caught Runtime error naming `split`, so the
        // program never reaches its `say` at all. See `edge_empty_separator`.
        answer: None,
    },
];

const JOIN_CASES: &[JoinCase] = &[
    JoinCase {
        what: "an empty list",
        items: &[],
        separator: ",",
        answer: "",
    },
    JoinCase {
        what: "one element",
        items: &["a"],
        separator: ",",
        answer: "a",
    },
    JoinCase {
        what: "adjacent empty elements",
        items: &["a", "", "b"],
        separator: ",",
        answer: "a,,b",
    },
    JoinCase {
        what: "a three-character separator",
        items: &["a", "b", "c"],
        separator: "::",
        answer: "a::b::c",
    },
    JoinCase {
        what: "an empty separator",
        items: &["a", "b"],
        separator: "",
        answer: "ab",
    },
];

/// A Redblue text literal for `value`, with `\` and `"` escaped.
#[track_caller]
fn rb_text(value: &str) -> String {
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

/// A Redblue list literal of text cells.
#[track_caller]
fn rb_list(items: &[&str]) -> String {
    let cells: Vec<String> = items.iter().map(|i| rb_text(i)).collect();
    format!("[{}]", cells.join(", "))
}

/// The program one `SPLIT_CASES` row is exercised with.
///
/// It prints the COUNT and then each part in brackets, one per line, because
/// `say` renders a list as `[a, b]` — which prints `[""]` and `[]` identically.
/// Bracketing each part separately is what makes an empty part visible as `[]`
/// rather than indistinguishable from an empty list.
fn source_for_split(case: &SplitCase) -> String {
    format!(
        "import text\nset parts to text.split({}, by {})\nsay length(parts)\nfor each p in parts\n    say \"[\" + p + \"]\"\nend\n",
        rb_text(case.subject),
        rb_text(case.separator),
    )
}

/// The output `source_for_split` must produce for a row's answer: the count,
/// then one bracketed line per part.
fn expected_split_output(parts: &[&str]) -> String {
    let mut out = format!("{}\n", parts.len());
    for p in parts {
        out.push_str(&format!("[{p}]\n"));
    }
    out
}

// ---------------------------------------------------------------------------
// `rb run` vs `rb compile` + `rb vm`: byte-identical stdout, for every row.
// ---------------------------------------------------------------------------

/// A scratch directory under `target/tmp`, removed first so an interrupted run
/// cannot leave a file behind that the next run of the same test would read.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/tmp/phase041")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch directory should be creatable");
    dir
}

/// The `rb` CLI, run in `dir` so a relative path in a program resolves there.
fn rb(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rb"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("rb should be runnable")
}

/// Writes `source` to `name.rb` in a fresh directory and returns `(stdout of
/// `rb run`, stdout of `rb vm` over `rb compile`'s output)`.
///
/// The two engines are compared as *bytes of stdout*, which is the property the
/// phase requires: a program that prints the same lines through the bytecode as
/// through the tree-walker, including the trailing newline.
#[track_caller]
fn run_on_both_engines(name: &str, source: &str) -> (String, String) {
    let dir = scratch(name);
    let program = dir.join(format!("{name}.rb"));
    std::fs::write(&program, source).expect("the program should be written");

    let interpreted = rb(&dir, &["run", &program.to_string_lossy()]);
    assert!(
        interpreted.status.success(),
        "`rb run` failed for {name}:\n{source}\n--- stderr\n{}",
        String::from_utf8_lossy(&interpreted.stderr)
    );

    let compiled = rb(&dir, &["compile", &program.to_string_lossy()]);
    assert!(
        compiled.status.success(),
        "`rb compile` failed for {name}:\n{source}\n--- stderr\n{}",
        String::from_utf8_lossy(&compiled.stderr)
    );

    let bytecode_path = dir.join(format!("{name}.rbc"));
    let executed = rb(&dir, &["vm", &bytecode_path.to_string_lossy()]);
    assert!(
        executed.status.success(),
        "`rb vm` failed for {name}:\n{source}\n--- stderr\n{}",
        String::from_utf8_lossy(&executed.stderr)
    );

    (
        String::from_utf8_lossy(&interpreted.stdout).into_owned(),
        String::from_utf8_lossy(&executed.stdout).into_owned(),
    )
}

/// The same pair, for a program that is *supposed* to be refused.
///
/// Returns `(refused, stdout, stderr)` for `rb run`, and asserts that `rb vm`
/// refuses the compiled program identically — same message, same exit, and
/// nothing printed by either. A program that printed something before failing
/// is a different failure from one that refused cleanly, so `stdout` is
/// returned rather than discarded.
#[track_caller]
fn refuse_on_both_engines(name: &str, source: &str) -> (bool, String, String) {
    let dir = scratch(name);
    let program = dir.join(format!("{name}.rb"));
    std::fs::write(&program, source).expect("the program should be written");
    let path = program.to_string_lossy().into_owned();

    let interpreted = rb(&dir, &["run", &path]);
    let compiled = rb(&dir, &["compile", &path]);
    assert!(
        compiled.status.success(),
        "`rb compile` should accept a program the VMs will refuse, for {name}"
    );
    let executed = rb(
        &dir,
        &["vm", &dir.join(format!("{name}.rbc")).to_string_lossy()],
    );

    // Only the first line is compared: the rendered error carries the source
    // path, which differs between the two runs only because `rb vm` is handed
    // a `.rbc`. The MESSAGE is what must be identical.
    let run_err = String::from_utf8_lossy(&interpreted.stderr).into_owned();
    let vm_err = String::from_utf8_lossy(&executed.stderr).into_owned();
    assert_eq!(
        run_err.lines().next(),
        vm_err.lines().next(),
        "the two engines should refuse {name} with the same message"
    );
    assert_eq!(
        interpreted.status.success(),
        executed.status.success(),
        "the two engines should agree on whether {name} is refused"
    );
    let stdout = String::from_utf8_lossy(&interpreted.stdout).into_owned();
    assert_eq!(
        stdout,
        String::from_utf8_lossy(&executed.stdout).into_owned(),
        "the two engines should print the same thing for {name}"
    );

    (!interpreted.status.success(), stdout, run_err)
}

// ---------------------------------------------------------------------------
// The dotted-versus-flat reachability table
// ---------------------------------------------------------------------------

/// One row of the reachability table.
///
/// Every stdlib name has TWO spellings — dotted (`text.split`) and flat
/// (`split`) — and the phase requires the answer to be pinned for both. A row
/// carries the call to make under each spelling plus the value that call must
/// produce, so neither form is asserted only against the other: if both broke
/// the same way, a comparison of the two would still pass.
struct Reach {
    /// The name `src/stdlib.rs` registers, which is also the flat spelling.
    builtin: &'static str,
    /// The module a reader reaches for dotted.
    module: &'static str,
    /// The argument list, used verbatim in both spellings.
    args: &'static str,
    /// What the call must produce.
    answer: Answer,
}

/// What a reachability row's call produces. Small enough to be `const`, which
/// is what lets the table itself be a `const`.
enum Answer {
    Cells(&'static [&'static str]),
    Text(&'static str),
    YesNo(bool),
    Number(f64),
    /// `json.parse`, whose answer is a record.
    Record,
    /// `csv.parse`, whose answer is a list of lists.
    Rows(&'static [&'static [&'static str]]),
    /// `files.read`, whose answer is a file this test reads itself.
    HelloFile,
}

const REACH: &[Reach] = &[
    Reach {
        builtin: "split",
        module: "text",
        args: "\"a,b\", by \",\"",
        answer: Answer::Cells(&["a", "b"]),
    },
    Reach {
        builtin: "join",
        module: "text",
        args: "[\"a\", \"b\"], by \",\"",
        answer: Answer::Text("a,b"),
    },
    Reach {
        builtin: "uppercase",
        module: "text",
        args: "\"aB\"",
        answer: Answer::Text("AB"),
    },
    Reach {
        builtin: "lowercase",
        module: "text",
        args: "\"aB\"",
        answer: Answer::Text("ab"),
    },
    Reach {
        builtin: "trim",
        module: "text",
        args: "\" a \"",
        answer: Answer::Text("a"),
    },
    Reach {
        builtin: "contains",
        module: "text",
        args: "\"abc\", \"b\"",
        answer: Answer::YesNo(true),
    },
    Reach {
        builtin: "starts_with",
        module: "text",
        args: "\"abc\", \"ab\"",
        answer: Answer::YesNo(true),
    },
    Reach {
        builtin: "ends_with",
        module: "text",
        args: "\"abc\", \"bc\"",
        answer: Answer::YesNo(true),
    },
    Reach {
        builtin: "replace",
        module: "text",
        args: "\"aXa\", \"X\", \"b\"",
        answer: Answer::Text("aba"),
    },
    Reach {
        builtin: "length",
        module: "list",
        args: "\"abc\"",
        answer: Answer::Number(3.0),
    },
    Reach {
        builtin: "sqrt",
        module: "math",
        args: "9",
        answer: Answer::Number(3.0),
    },
    Reach {
        builtin: "json_parse",
        module: "json",
        args: "\"{\\\"a\\\": 1}\"",
        answer: Answer::Record,
    },
    Reach {
        builtin: "csv_parse",
        module: "csv",
        args: "\"a,b\\n1,2\"",
        answer: Answer::Rows(&[&["a", "b"], &["1", "2"]]),
    },
    Reach {
        builtin: "files_read",
        module: "files",
        args: "\"examples/hello.rb\"",
        answer: Answer::HelloFile,
    },
];

impl Answer {
    /// The value this row's call must produce.
    fn expected(&self) -> Value {
        match self {
            Answer::Cells(parts) => cells(parts),
            Answer::Text(text) => Value::Text((*text).to_string()),
            Answer::YesNo(b) => Value::YesNo(*b),
            Answer::Number(n) => Value::Number(*n),
            Answer::Record => Value::record(
                [("a".to_string(), Value::Number(1.0))]
                    .into_iter()
                    .collect(),
            ),
            Answer::Rows(rows) => Value::list(rows.iter().map(|r| cells(r)).collect()),
            Answer::HelloFile => Value::Text(
                std::fs::read_to_string(
                    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/hello.rb"),
                )
                .expect("examples/hello.rb should be readable"),
            ),
        }
    }
}

#[test]
fn table_every_split_case_answers_the_same_on_both_engines() {
    for case in SPLIT_CASES {
        let name = format!("split_{}", case.what.replace(' ', "_"));
        let (run_out, vm_out) = match &case.answer {
            Some(parts) => {
                let expected = expected_split_output(parts);
                let (run_out, vm_out) = run_on_both_engines(&name, &source_for_split(case));
                assert_eq!(
                    run_out, expected,
                    "`rb run` disagrees for split with {}",
                    case.what
                );
                assert_eq!(
                    vm_out, expected,
                    "`rb vm` disagrees for split with {}",
                    case.what
                );
                (run_out, vm_out)
            }
            None => {
                // The empty separator has no answer, so the program fails on
                // both engines. Both must fail, must say `split`, and must
                // print nothing at all — a program that half-ran would print.
                let (refused, out, err) = refuse_on_both_engines(&name, &source_for_split(case));
                assert!(refused, "`rb run` should refuse, printed {out:?}");
                assert!(
                    err.contains("split"),
                    "the refusal should name `split`, got {err}"
                );
                (out.clone(), out)
            }
        };
        assert_eq!(
            run_out, vm_out,
            "the two engines disagree for split with {}",
            case.what
        );
    }
}

#[test]
fn table_every_join_case_answers_the_same_on_both_engines() {
    for case in JOIN_CASES {
        let name = format!("join_{}", case.what.replace(' ', "_"));
        let source = format!(
            "import text\nset joined to text.join({}, by {})\nsay joined\n",
            rb_list(case.items),
            rb_text(case.separator),
        );
        let (run_out, vm_out) = run_on_both_engines(&name, &source);
        let expected = format!("{}\n", case.answer);
        assert_eq!(
            run_out, expected,
            "`rb run` disagrees for join with {}",
            case.what
        );
        assert_eq!(
            vm_out, expected,
            "`rb vm` disagrees for join with {}",
            case.what
        );
        assert_eq!(
            run_out, vm_out,
            "the two engines disagree for join with {}",
            case.what
        );
    }
}

#[test]
fn spec_md_text_section_runs_verbatim() {
    // The four lines of SPEC.md § text, spelled exactly as SPEC.md spells them.
    let source = concat!(
        "import text\n",
        "set upper to text.uppercase(\"hello\")  // \"HELLO\"\n",
        "set lower to text.lowercase(\"HELLO\")  // \"hello\"\n",
        "set parts to text.split(\"a,b,c\", by \",\")  // [\"a\", \"b\", \"c\"]\n",
        "set joined to text.join([\"a\", \"b\", \"c\"], by \",\")  // \"a,b,c\"\n",
        "say upper\n",
        "say lower\n",
        "say parts\n",
        "say joined\n",
    );
    let (run_out, vm_out) = run_on_both_engines("spec_text_section", source);
    assert_eq!(
        run_out, "HELLO\nhello\n[a, b, c]\na,b,c\n",
        "SPEC.md § text does not run as written\n--- stderr\n{run_out}"
    );
    assert_eq!(
        vm_out, run_out,
        "the two engines disagree on SPEC.md § text"
    );
}

#[test]
fn dotted_and_flat_spellings_agree_for_every_reachable_builtin() {
    // `text.split(x, by ",")` and `split(x, by ",")` must be the same function,
    // not two that happen to look alike. Both are exercised here through the
    // same input, and each is asserted on its own value rather than only
    // against the other, so a change that broke both the same way would fail.
    let dotted = eval("import text\ntext.split(\"a,b\", \",\")\n");
    let flat = eval("split(\"a,b\", \",\")\n");
    assert_eq!(dotted, cells(&["a", "b"]), "`text.split` is wrong dotted");
    assert_eq!(flat, cells(&["a", "b"]), "`split` is wrong flat");

    let dotted_join = eval("import text\ntext.join([\"a\", \"b\"], \",\")\n");
    let flat_join = eval("join([\"a\", \"b\"], \",\")\n");
    assert_eq!(
        dotted_join,
        Value::Text("a,b".to_string()),
        "`text.join` is wrong dotted"
    );
    assert_eq!(
        flat_join,
        Value::Text("a,b".to_string()),
        "`join` is wrong flat"
    );

    // The dotted label spelling and the positional spelling are the same call.
    let by_label = eval("import text\ntext.split(\"a,b\", by \",\")\n");
    assert_eq!(by_label, dotted, "`by` labels an argument positionally");
}

#[test]
fn reachability_table_holds_for_every_row() {
    for row in REACH {
        // The module name IS a module. `stdlib::is_module` is what lets the
        // analyzer accept `text` as a receiver at all (src/analyzer.rs:459).
        assert!(
            redblue::stdlib::is_module(row.module),
            "`{}` should be a module name",
            row.module
        );

        let expected = row.answer.expected();

        // Dotted: `text.split(..)` resolves to the bare builtin through
        // `stdlib::resolve` (src/stdlib.rs:541), so it is ONE function reached
        // two ways, not two that look alike.
        let dotted = eval(&format!(
            "import {m}\n{m}.{b}({a})\n",
            m = row.module,
            b = row.builtin,
            a = row.args
        ));
        assert_eq!(
            dotted,
            expected,
            "`{m}.{b}` is wrong dotted",
            m = row.module,
            b = row.builtin
        );

        // Flat: the same builtin under its own name, with no import at all.
        let flat = eval(&format!("{b}({a})\n", b = row.builtin, a = row.args));
        assert_eq!(flat, expected, "`{b}` is wrong flat", b = row.builtin);
    }
}

#[test]
fn text_split_reaches_dotted_without_an_import_and_that_is_the_rule() {
    // `src/analyzer.rs:459-471` is what makes this true: the receiver of a
    // method call is an unknown variable only when it is neither in scope nor a
    // module. So `text.split(..)` is legal with no `import text` anywhere,
    // while a bare read of `text` — `say text`, or the property access
    // `text.length` used as a value — is not, and is refused by name.
    assert_eq!(
        eval("text.split(\"a,b\", \",\")\n"),
        cells(&["a", "b"]),
        "`text.split` should work with no import"
    );
    assert_eq!(
        eval("text.join([\"a\", \"b\"], \",\")\n"),
        Value::Text("a,b".to_string()),
        "`text.join` should work with no import"
    );

    // The same program with the import runs identically, so a reader who
    // writes it either way gets one answer.
    assert_eq!(
        eval("import text\ntext.split(\"a,b\", \",\")\n"),
        eval("text.split(\"a,b\", \",\")\n"),
        "the import should not change the answer"
    );

    // A bare read of the module name is still refused, and by name.
    match eval_err("say text\n") {
        Error::Analyzer(message, _) => assert!(
            message.contains("'text'"),
            "the refusal should name `text`, got {message}"
        ),
        other => panic!("a bare read of `text` should be refused, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Failure channel
// ---------------------------------------------------------------------------

#[test]
fn split_and_join_refuse_a_wrongly_typed_argument_and_name_the_builtin() {
    // Every one of these must be a CATCHABLE Runtime error that names the
    // builtin. A panic, an unwrap, or a message that does not say which builtin
    // refused is a failure of this test.
    let cases: &[(&str, &str)] = &[
        ("split", "split(1, \",\")\n"),
        ("split", "split(\"a,b\", 1)\n"),
        ("split", "split(\"a,b\")\n"),
        ("split", "split()\n"),
        ("join", "join(\"abc\", \",\")\n"),
        ("join", "join([\"a\"], 1)\n"),
        ("join", "join([\"a\"])\n"),
        ("join", "join()\n"),
    ];
    for (builtin, source) in cases {
        match eval_err(source) {
            Error::Runtime(message, span) => {
                assert!(
                    message.contains(builtin),
                    "the message for `{source}` should name `{builtin}`, got `{message}`"
                );
                assert!(span.is_known(), "`{source}` failed without a span");
            }
            other => panic!("`{source}` should be a caught Runtime error, got {other:?}"),
        }

        // The dotted spelling refuses the same way: the error names the builtin,
        // not the module-qualified spelling the program wrote.
        let dotted = source.replacen(builtin, &format!("text.{builtin}"), 1);
        let program = dotted.clone();
        match eval_err(&program) {
            Error::Runtime(message, span) => {
                assert!(
                    message.contains(builtin),
                    "the dotted message should name `{builtin}`, got `{message}`"
                );
                assert!(span.is_known(), "`{dotted}` failed without a span");
            }
            other => panic!("`{dotted}` should be a caught Runtime error, got {other:?}"),
        }
    }
}

#[test]
fn a_wrongly_typed_split_argument_is_caught_by_try_catch_in_a_redblue_program() {
    // The failure channel as a Redblue program sees it: `try`/`catch error`
    // rather than a process abort.
    let source = concat!(
        "import text\n",
        "set caught to no\n",
        "try\n",
        "    set parts to text.split(1, by \",\")\n",
        "catch error\n",
        "    set caught to yes\n",
        "end\n",
        "expect caught to be yes\n",
    );
    match redblue::run_source(source) {
        Ok(()) => {}
        Err(error) => panic!("a wrongly typed split must be caught, not fatal: {error:?}"),
    }
}

// ---------------------------------------------------------------------------
// Edge cases the phase names explicitly
// ---------------------------------------------------------------------------

#[test]
fn edge_empty_subject_splits_to_one_empty_part() {
    // `""` has one part and it is empty. Not zero parts: a separator cannot
    // appear in an empty subject, so the subject is the answer.
    let result = eval("import text\ntext.split(\"\", by \",\")\n");
    assert_eq!(
        result,
        cells(&[""]),
        "an empty subject splits to one empty part"
    );
    assert_eq!(eval("import text\nlength(text.split(\"\", by \",\"))\n"), {
        Value::Number(1.0)
    });
}

#[test]
fn edge_empty_list_joins_to_empty_text() {
    assert_eq!(
        eval("import text\ntext.join([], by \",\")\n"),
        Value::Text(String::new()),
        "an empty list joins to the empty text, not to nothing"
    );
    // A one-element list is the boundary case: the separator appears zero
    // times, so the answer is that element unchanged.
    assert_eq!(
        eval("import text\ntext.join([\"only\"], by \",\")\n"),
        Value::Text("only".to_string()),
        "one element joins to itself"
    );
}

#[test]
fn edge_empty_separator_is_a_caught_error_and_join_with_one_still_concatenates() {
    // An empty separator would make every position a boundary and never
    // advance. Redblue has no regex, so there is no answer — and
    // `src/stdlib.rs:481` turns "no answer" into a caught Runtime error that
    // names `split`, rather than a silent `nothing` or a hang. Pinned here so
    // a change that made it loop, or turned it into a per-character split,
    // would fail.
    match eval_err("import text\ntext.split(\"abc\", by \"\")\n") {
        Error::Runtime(message, span) => {
            assert!(
                message.contains("split"),
                "the refusal should name `split`, got `{message}`"
            );
            assert!(span.is_known(), "the refusal carried no span");
        }
        other => panic!("an empty separator should be refused, got {other:?}"),
    }

    // Through the CLI, on BOTH engines, with the same message — and nothing
    // printed by either, because the program fails before it reaches its `say`.
    let source = "import text\nsay text.split(\"abc\", by \"\")\n";
    let (refused, stdout, stderr) = refuse_on_both_engines("edge_empty_separator", source);
    assert!(
        refused,
        "an empty separator should be refused under `rb run`"
    );
    assert!(
        stdout.is_empty(),
        "a refused program should print nothing, got {stdout:?}"
    );
    assert!(
        stderr.contains("split"),
        "the refusal should name `split`, got {stderr}"
    );

    // The empty separator on `join` is different and must stay different:
    // joining with it is well defined and concatenates.
    assert_eq!(
        eval("import text\ntext.join([\"a\", \"b\"], by \"\")\n"),
        Value::Text("ab".to_string()),
        "joining with an empty separator concatenates"
    );
}

#[test]
fn edge_subject_longer_than_the_separator_keeps_every_part() {
    // A separator that is shorter than the subject, with matches at both ends
    // and in the middle, so a scan that stops early or drops a boundary shows.
    assert_eq!(
        eval("import text\ntext.split(\",a,\", by \",\")\n"),
        cells(&["", "a", ""]),
        "a separator at both ends must produce both empty parts"
    );
    assert_eq!(
        eval("import text\ntext.split(\"aaa\", by \",\")\n"),
        cells(&["aaa"]),
        "a subject of separators only in the separator keeps its one part"
    );
    // A separator longer than the subject is a single part, not an error.
    assert_eq!(
        eval("import text\ntext.split(\"a\", by \"----\")\n"),
        cells(&["a"]),
        "a separator longer than the subject leaves it whole"
    );
    // The separator longer than the subject, found: exactly one boundary.
    assert_eq!(
        eval("import text\ntext.split(\"a----b\", by \"----\")\n"),
        cells(&["a", "b"]),
        "a four-character separator found once splits in two"
    );
}

#[test]
fn edge_unicode_subject_splits_on_character_boundaries() {
    // The source carries the real characters, not `\u` escapes: Redblue's
    // lexer has no `\u` escape, so `"\u{1F600}"` in a Redblue literal is the
    // nine ASCII characters `u{1F600}`. This test is therefore about the
    // lexer's UTF-8 handling, which is the thing a byte-wise separator scan
    // would break.
    let emoji = "\u{1F600}";
    let cjk_a = "\u{4F60}\u{597D}";
    let cjk_b = "\u{4E16}\u{754C}";
    let ellipsis = "\u{2026}";

    // An emoji between separators must survive whole.
    assert_eq!(
        eval(&format!(
            "import text\ntext.split(\"a,{emoji},b\", by \",\")\n"
        )),
        cells(&["a", emoji, "b"]),
        "an emoji between separators must survive whole"
    );
    // CJK splits on character boundaries.
    assert_eq!(
        eval(&format!(
            "import text\ntext.split(\"{cjk_a},{cjk_b}\", by \",\")\n"
        )),
        cells(&[cjk_a, cjk_b]),
        "CJK must split on character boundaries"
    );
    // A separator that is itself multi-byte, which a byte-wise scan would miss
    // because the boundary falls inside a character.
    assert_eq!(
        eval(&format!(
            "import text\ntext.split(\"a{ellipsis}b\", by \"{ellipsis}\")\n"
        )),
        cells(&["a", "b"]),
        "a multi-byte separator must be found whole"
    );
    // And the round trip: joining does not truncate at a byte boundary either.
    assert_eq!(
        eval(&format!(
            "import text\ntext.join([\"{emoji}\", \"{cjk_b}\"], by \"-\")\n"
        )),
        Value::Text(format!("{emoji}-{cjk_b}")),
        "joining must not truncate at a byte boundary"
    );
    // A combining mark stays attached to the character it modifies.
    let combined = "e\u{0301}";
    assert_eq!(
        eval(&format!(
            "import text\ntext.split(\"{combined},x\", by \",\")\n"
        )),
        cells(&[combined, "x"]),
        "a combining mark must stay with its base character"
    );
}

#[test]
fn edge_a_number_is_where_a_text_is_wanted_is_refused_cleanly_on_both_engines() {
    // The type-mismatch row, through the CLI as well as the library, so the
    // error the user sees is the error the library produces. Both engines, one
    // message, nothing printed.
    let source = "import text\nsay text.split(\"a,b\", by 1)\n";
    let (refused, stdout, stderr) = refuse_on_both_engines("edge_type_mismatch", source);
    assert!(
        refused,
        "a number where a separator is wanted must be refused"
    );
    assert!(
        stdout.is_empty(),
        "a refused program should print nothing, got {stdout:?}"
    );
    assert!(
        stderr.contains("split"),
        "the refusal should name `split`, got {stderr}"
    );

    // And the flat spelling refuses with the same message — one builtin, two
    // spellings, one error.
    let flat = refuse_on_both_engines("edge_type_mismatch_flat", "say split(\"a,b\", by 1)\n");
    assert!(flat.0, "the flat spelling must refuse too");
    assert_eq!(
        flat.2.lines().next(),
        stderr.lines().next(),
        "the dotted and flat spellings should refuse identically"
    );
}

#[test]
fn edge_nested_and_deep_subjects_do_not_recurse() {
    // `split` and `join` are iterative on purpose: a subject with many
    // separators must not be bounded by any recursion limit, and must produce
    // every part.
    let mut source = String::from("import text\nset parts to text.split(");
    let text = "x".repeat(2000);
    source.push_str(&format!(
        "{}, by \",\")\nsay length(parts)\n",
        rb_text(&format!("{text},{text},{text}"))
    ));
    let (run_out, vm_out) = run_on_both_engines("edge_deep_subject", &source);
    assert_eq!(run_out, "3\n", "a 6000-character subject splits in three");
    assert_eq!(
        vm_out, run_out,
        "the two engines disagree on a long subject"
    );

    // A list of 1000 elements joined is equally unbounded.
    let items: Vec<String> = (0..1000).map(|i| i.to_string()).collect();
    let joined = eval(&format!(
        "import text\ntext.join({}, by \",\")\n",
        rb_list(&items.iter().map(String::as_str).collect::<Vec<&str>>())
    ));
    assert_eq!(
        joined,
        Value::Text(items.join(",")),
        "a thousand-element list joins in order"
    );
}

#[test]
fn edge_a_record_where_a_list_is_wanted_is_refused_not_iterated() {
    // A record is not a list, and Redblue has no silent iteration of one.
    match eval_err("import text\ntext.join({a: 1}, by \",\")\n") {
        Error::Runtime(message, _) => assert!(
            message.contains("join"),
            "the refusal should name `join`, got {message}"
        ),
        other => panic!("a record should be refused by `join`, got {other:?}"),
    }
    match eval_err("import text\ntext.split({a: 1}, by \",\")\n") {
        Error::Runtime(message, _) => assert!(
            message.contains("split"),
            "the refusal should name `split`, got {message}"
        ),
        other => panic!("a record should be refused by `split`, got {other:?}"),
    }
    // A number where a list is wanted is refused the same way.
    assert!(
        matches!(
            eval_err("import text\ntext.join(7, by \",\")\n"),
            Error::Runtime(_, _)
        ),
        "a number where a list is wanted must be refused"
    );
}

// ---------------------------------------------------------------------------
// The bytecode VM agrees with the tree-walker, as a library
// ---------------------------------------------------------------------------

#[test]
fn the_bytecode_vm_answers_split_and_join_the_same_as_the_interpreter() {
    // Row by row, the two engines are compared on the VALUE they produce. The
    // one row with no value — an empty separator — is compared on the ERROR
    // instead, because "both refuse" is the property there.
    for case in SPLIT_CASES {
        let source = format!(
            "import text\ntext.split({}, by {})\n",
            rb_text(case.subject),
            rb_text(case.separator),
        );
        // Both outcomes are rendered to text, so the two branches compare with
        // one assertion instead of two near-identical ones.
        let interpreted = match &case.answer {
            Some(_) => format!("{:?}", eval(&source)),
            None => format!("{:?}", eval_err(&source)),
        };
        let mut vm = BytecodeVm::new();
        let bytecode = match compile_source(&source) {
            Ok(chunk) => match vm.run(&chunk) {
                Ok(value) => value,
                Err(error) => {
                    assert!(
                        case.answer.is_none(),
                        "the bytecode VM refused a case with an answer: {error:?}"
                    );
                    // The interpreter must refuse it too, with the same message.
                    assert_eq!(
                        format!("{:?}", error),
                        interpreted,
                        "the engines should refuse an empty separator identically"
                    );
                    continue;
                }
            },
            Err(error) => panic!("{source} should compile: {error:?}"),
        };
        assert_eq!(
            format!("{bytecode:?}"),
            interpreted,
            "the engines disagree for split with {}",
            case.what
        );
    }

    for case in JOIN_CASES {
        let source = format!(
            "import text\ntext.join({}, by {})\n",
            rb_list(case.items),
            rb_text(case.separator),
        );
        let chunk =
            compile_source(&source).unwrap_or_else(|e| panic!("{source} should compile: {e:?}"));
        let mut vm = BytecodeVm::new();
        let bytecode = vm
            .run(&chunk)
            .unwrap_or_else(|e| panic!("{source} should run under bytecode: {e:?}"));
        assert_eq!(
            format!("{bytecode:?}"),
            format!("{:?}", eval(&source)),
            "the engines disagree for join with {}",
            case.what
        );
    }
}

// ---------------------------------------------------------------------------
// What `by` labels, and every way of not labelling
// ---------------------------------------------------------------------------

#[test]
fn the_by_label_is_read_for_split_and_join_and_for_no_other_call() {
    // SPEC.md:1017-1018 writes the label for exactly two builtins, in both
    // spellings. Those four calls must keep working — this is the accept side.
    assert_eq!(
        eval("import text\ntext.split(\"a,b\", by \",\")\n"),
        cells(&["a", "b"]),
        "`text.split` takes a `by` label"
    );
    assert_eq!(
        eval("text.join([\"a\", \"b\"], by \",\")\n"),
        Value::Text("a,b".to_string()),
        "`text.join` takes a `by` label"
    );
    assert_eq!(
        eval("split(\"a,b\", by \",\")\n"),
        cells(&["a", "b"]),
        "flat `split` takes a `by` label"
    );
    assert_eq!(
        eval("join([\"a\", \"b\"], by \",\")\n"),
        Value::Text("a,b".to_string()),
        "flat `join` takes a `by` label"
    );

    // The refuse side, for a builtin that has no label in the specification.
    // `by` is bound to 3 here, so a parser that read `by` as a label and DROPPED
    // it would answer `pow(2, 10)` — 1024. Answering 8 is proof that the word
    // survived as the variable it also has the right to be.
    assert_eq!(
        eval("set by to 3\npow(2, by 10)\n"),
        Value::Number(8.0),
        "`pow` does not take a `by` label, so `by` is the variable 3"
    );

    // The same for a program-defined function: `by` is a parameter name the
    // language has always allowed (`tests/bytecode_test.rs:614`), and no `by`
    // in scope means the analyzer refuses the read by name rather than the
    // parser quietly eating the word.
    match eval_err("to scale(x)\n    give back x * 2\nend\nscale(by 2)\n") {
        Error::Analyzer(message, _) => assert!(
            message.contains("'by'"),
            "the refusal should name `by`, got {message}"
        ),
        other => panic!("a `by` label on a user function should be refused, got {other:?}"),
    }

    // And the whole argument can still BE the variable, which is the case the
    // label must never swallow: `)` follows `by`, so it is not a label in either
    // the old or the new rule.
    assert_eq!(
        eval("set by to \"a,b\"\ntext.split(by, \",\")\n"),
        cells(&["a", "b"]),
        "`by` as the whole first argument is the variable"
    );
    assert_eq!(
        eval("set by to \",\"\ntext.split(\"a,b\", by)\n"),
        cells(&["a", "b"]),
        "`by` as the whole second argument is the variable"
    );
}

#[test]
fn a_by_in_front_of_an_operator_is_a_variable_and_never_a_label() {
    // A label is only a label when the word after `by` can BEGIN an expression,
    // and that is the positive half of the expression-start table — never a
    // blacklist of the tokens that cannot. A blacklist read `by + ","` as a
    // label, dropped the `by`, and turned working programs into parse errors.
    //
    // Each program below therefore pins a *value*, not an error: a parser that
    // dropped the `by` could not produce it, because what is left over after
    // the drop is `+ ","`, which is not an expression.
    assert_eq!(
        eval("set by to \",\"\ntext.join([\"a\", \"b\"], by + by)\n"),
        Value::Text("a,,b".to_string()),
        "`by + by` is one expression using the variable twice"
    );
    assert_eq!(
        eval("set by to 3\npow(2, by + 1)\n"),
        Value::Number(16.0),
        "`by + 1` is one expression, not a label followed by `+ 1`"
    );

    // `is` produces a comparison, which no label could be standing in front of
    // either: the separator here is a boolean, so `split` refuses it by name.
    // The refusal is the assertion — it is only reachable when `by` was read as
    // the variable and the `is` was kept.
    match eval_err("import text\nset by to \"x\"\ntext.split(\"a,b\", by is \"x\")\n") {
        Error::Runtime(message, _) => assert!(
            message.contains("split"),
            "the refusal should name `split`, got {message}"
        ),
        other => panic!("`by is \"x\"` is a comparison, not a label, got {other:?}"),
    }

    // The unbound half: with no `by` in scope, `pow(2, by + 1)` is refused by the
    // analyzer by name instead of reaching `pow`.
    match eval_err("pow(2, by + 1)\n") {
        Error::Analyzer(message, _) => assert!(
            message.contains("'by'"),
            "the refusal should name `by`, got {message}"
        ),
        other => panic!("an unbound `by` should be refused, got {other:?}"),
    }
}

#[test]
fn a_label_and_the_value_it_labels_may_be_written_on_different_lines() {
    // The decision to read `by` as a label looks past a newline to the token
    // after it, so the parse that follows has to skip the same newline. When it
    // did not, the shape was classified one way and read the other: the word
    // `by` was accepted and then the expression was parsed from a newline token.
    let multiline_split =
        "import text\nset parts to text.split(\"a,b\", by\n\",\")\nsay length(parts)\n";
    let single_line_split =
        "import text\nset parts to text.split(\"a,b\", by \",\")\nsay length(parts)\n";

    let (run_out, vm_out) = run_on_both_engines("label_multiline_split", multiline_split);
    assert_eq!(
        run_out, "2\n",
        "a `by` label split across two lines must split like the single-line one"
    );
    assert_eq!(
        vm_out, run_out,
        "the two engines disagree on a label split across lines"
    );
    assert_eq!(
        eval(multiline_split),
        eval(single_line_split),
        "moving the separator to the next line must not change the value"
    );

    let multiline_join =
        "import text\nset joined to text.join([\"a\", \"b\"], by\n\",\")\nsay joined\n";
    let (run_out, vm_out) = run_on_both_engines("label_multiline_join", multiline_join);
    assert_eq!(
        run_out, "a,b\n",
        "a label split across two lines must join as written"
    );
    assert_eq!(
        vm_out, run_out,
        "the two engines disagree on a label split across lines"
    );
    assert_eq!(
        eval(multiline_join),
        eval("import text\nset joined to text.join([\"a\", \"b\"], by \",\")\nsay joined\n"),
        "a label split across two lines must be the same call"
    );

    // The newline is skipped around the LABEL, and only around the label: an
    // argument list broken over a line *before* the `by` is a different shape,
    // one the argument loop has never accepted. It is pinned here so the rule
    // above cannot widen by accident into "argument lists may wrap".
    match eval_err("import text\ntext.join([\"a\", \"b\"],\nby \",\")\n") {
        Error::Parser(message, _) => assert!(
            message.contains("Newline"),
            "the refusal should name the newline it stopped at, got {message}"
        ),
        other => panic!("an argument broken before its label is still refused, got {other:?}"),
    }

    // A newline is not a label on its own: `by` followed by nothing but the
    // closing paren is the variable, and the `)` on its own line is a shape the
    // argument loop has never accepted — so this is refused, exactly as the
    // newline before the label above is. The lookahead must not turn the
    // refusal into an acceptance.
    match eval_err("import text\nset by to \",\"\ntext.split(\"a,b\", by\n)\n") {
        Error::Parser(message, _) => assert!(
            message.contains("Newline"),
            "the refusal should name the newline it stopped at, got {message}"
        ),
        other => panic!("`by` before a closing paren is a variable, not a label: {other:?}"),
    }
}
