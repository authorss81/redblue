//! `catch error of <Type>` — a handler that matches the kind of failure it
//! caught.
//!
//! `Error::{Lexer,Parser,Analyzer,Runtime,Io}` are the host's compile-time
//! kinds, and a `catch` used to bind the untyped text `error`, so there was
//! nothing for `of <Type>` to match against: `catch error of Io` read as a
//! binder named `of` with `Io` left as a statement of its own, and the analyzer
//! refused the program with `Unknown variable 'Io'`.
//!
//! A failure a program catches is now a first-class value — a record carrying
//! the failure's kind, its message and where it happened — and `catch error of
//! <Type>` runs its body only for the kinds it names. A typed handler that does
//! not match is not a handler: the failure belongs to whatever is written around
//! it.
//!
//! Every case runs through **both** engines — the tree-walking VM and the
//! bytecode compiler plus the bytecode VM — and the two are compared on what
//! they *bound*, not only on what they printed. A form that works on one engine
//! and is an `AnalyzerError` on the other is an accident of one frontend, not a
//! language feature.

use redblue::bytecode::vm::BytecodeVm;
use redblue::failure::KINDS;
use redblue::{compile_source, Error, FailureKind, Value};

/// What one engine made of a program: the lines it printed, and either its value
/// or the failure it reported.
///
/// The same shape `tests/catch_binding_test.rs` compares with, so a failure is
/// compared by its kind and its message rather than by a rendered string, and a
/// success by the value it was worth.
#[derive(Debug, Clone)]
struct Outcome {
    output: Vec<String>,
    result: Result<Value, Error>,
}

fn tree_walk(source: &str) -> Outcome {
    tree_walk_capped(source, redblue::MAX_ITERATIONS)
}

fn tree_walk_capped(source: &str, max_iterations: usize) -> Outcome {
    let failed = |error: Error| Outcome {
        output: Vec::new(),
        result: Err(error),
    };
    let tokens = match redblue::lexer::Lexer::tokenize(source) {
        Ok(tokens) => tokens,
        Err(error) => return failed(error),
    };
    let ast = match redblue::parser::parse(tokens) {
        Ok(ast) => ast,
        Err(error) => return failed(error),
    };
    if let Err(error) = redblue::analyzer::analyze(&ast) {
        return failed(error);
    }
    let mut vm = redblue::Vm::with_max_iterations(max_iterations);
    let result = vm.run(&ast);
    Outcome {
        output: vm.take_output(),
        result,
    }
}

fn bytecode(source: &str) -> Outcome {
    bytecode_capped(source, redblue::MAX_ITERATIONS)
}

fn bytecode_capped(source: &str, max_iterations: usize) -> Outcome {
    let chunk = match compile_source(source) {
        Ok(chunk) => chunk,
        Err(error) => {
            return Outcome {
                output: Vec::new(),
                result: Err(error),
            }
        }
    };
    let mut vm = BytecodeVm::with_max_iterations(max_iterations);
    vm.set_echo(false);
    let result = vm.run(&chunk);
    Outcome {
        output: vm.take_output(),
        result,
    }
}

/// Runs `source` on both engines and answers whether they said the same thing.
///
/// A failure is compared by kind and message rather than by position: a `.rbc`
/// instruction carries a line and no column, so the bytecode VM places a
/// runtime failure at the start of the line its instruction is on while the
/// tree-walking VM places it at the column of the failing statement. That is
/// the engines' own convention — `tests/common/vm.rs` records it per failure —
/// and not something this change moves, so a program that prints a failure's
/// `column` is a tree-walking-only test and says why.
fn both_engines(source: &str) -> (Outcome, Outcome) {
    let tree = tree_walk(source);
    let bytes = bytecode(source);
    assert!(
        agree(&tree, &bytes),
        "the engines disagreed\ntree-walking: {tree:?}\nbytecode:    {bytes:?}"
    );
    (tree, bytes)
}

/// [`both_engines`] with one loop capped at `max_iterations`, which is how a
/// program reaches the host's own [`Error::Limit`] failure quickly and without
/// an environment variable deciding whether it does.
fn both_engines_capped(source: &str, max_iterations: usize) -> (Outcome, Outcome) {
    let tree = tree_walk_capped(source, max_iterations);
    let bytes = bytecode_capped(source, max_iterations);
    assert!(
        agree(&tree, &bytes),
        "the engines disagreed\ntree-walking: {tree:?}\nbytecode:    {bytes:?}"
    );
    (tree, bytes)
}

fn agree(a: &Outcome, b: &Outcome) -> bool {
    if a.output != b.output {
        return false;
    }
    match (&a.result, &b.result) {
        (Ok(x), Ok(y)) => x == y,
        (Err(x), Err(y)) => x.label() == y.label() && x.message() == y.message(),
        _ => false,
    }
}

/// The lines a run printed, failing the test by name when the run failed.
fn lines(outcome: &Outcome) -> Vec<String> {
    assert!(
        outcome.result.is_ok(),
        "expected the program to finish, got {:?}",
        outcome.result
    );
    outcome.output.clone()
}

/// The failure a run reported, as `(kind, message)`.
///
/// Panics when the run succeeded: a test asking what a program failed with has
/// to be told, rather than handed an empty string to compare against nothing.
fn failure(outcome: &Outcome) -> (String, String) {
    match &outcome.result {
        Err(error) => (error.label().to_string(), error.message().to_string()),
        Ok(value) => panic!("expected a failure, but the program was worth {value:?}"),
    }
}

#[test]
fn typed_catch_matches_its_own_kind_and_only_its_own_kind() {
    let source = r#"
try
    set record to files.read("typed-catch-missing-file.txt")
catch error of Io
    say "caught the missing file"
end
try
    set sum to 1 + "a"
catch error of Io
    say "caught the type error as well"
end
say "program ends here"
"#;

    let (tree, _bytes) = both_engines(source);

    // The `Io` handler ran: the kind matched, and nothing but it did.
    assert_eq!(tree.output, vec!["caught the missing file"]);

    // The `Runtime` failure the second handler does not name was not taken by
    // it, so the program failed there — printing neither that handler's line nor
    // the one after the second `try`.
    let (kind, message) = failure(&tree);
    assert_eq!(kind, "RuntimeError", "got {message:?}");
    assert!(
        message.contains("add"),
        "the reported failure should be the type error the second try raised, got {message:?}"
    );
}

#[test]
fn a_typed_catch_binds_the_failure_as_a_first_class_value() {
    let source = r#"
try
    set nothing_useful to 1 + "a"
catch error of Runtime failure
    say type_of(failure)
    say "{failure.kind}"
    say "{failure.line}"
    say "{failure.message}"
    set seen to [failure]
    say length of seen
    set same to {kind: failure.kind, message: failure.message, line: failure.line, column: failure.column}
    say failure is same
    say failure.column is greater than 0
end
"#;

    let (tree, _bytes) = both_engines(source);

    let printed = lines(&tree);
    assert_eq!(printed[0], "record", "a failure is bound as a record");
    assert_eq!(
        printed[1], "Runtime",
        "the kind is the one the handler named"
    );
    // The failure is on line 3 of the program, and `line` is 1-based like every
    // other line the language reports.
    assert_eq!(printed[2], "3", "the position is where it happened");
    assert!(
        printed[3].contains("add"),
        "the message is the failure's own, got {:?}",
        printed[3]
    );
    assert_eq!(
        printed[4], "1",
        "the failure goes into a list like any value"
    );
    assert_eq!(
        printed[5], "yes",
        "a failure is equal to a record holding the same fields"
    );
    // The column is asked a question both engines answer the same way rather
    // than printed: a `.rbc` instruction carries a line and no column, so the
    // bytecode VM reports the start of the line its instruction is on while the
    // tree-walking VM reports the column of the failing statement. Both say
    // "somewhere on line 3".
    assert_eq!(printed[6], "yes", "the column is populated on both engines");
    assert_eq!(printed.len(), 7, "every line is accounted for: {printed:?}");

    // How the record prints, read from the tree-walking engine alone: the two
    // engines place a failure at a different column by their own convention — a
    // `.rbc` instruction carries a line and no column — so a printing that both
    // must agree on cannot include it.
    let rendering =
        "try\n    set sum to 1 + \"a\"\ncatch error of Runtime failure\n    say failure\nend\n";
    let printed = lines(&tree_walk(rendering));
    assert_eq!(
        printed[0],
        "{kind: Runtime, message: Cannot add non-numbers, line: 2, column: 5}"
    );
}

#[test]
fn a_runtime_handler_catches_a_runtime_failure() {
    let source = r#"
try
    set sum to 1 + "a"
catch error of Runtime failure
    say "runtime: {failure.kind}"
end
"#;

    let (tree, _bytes) = both_engines(source);
    assert_eq!(lines(&tree), vec!["runtime: Runtime"]);
}

#[test]
fn an_io_handler_catches_an_io_failure() {
    let source = r#"
try
    set contents to files.read("typed-catch-missing-file.txt")
catch error of Io failure
    say "io: {failure.kind}"
end
"#;

    let (tree, _bytes) = both_engines(source);
    assert_eq!(lines(&tree), vec!["io: Io"]);
}

#[test]
fn a_limit_handler_catches_a_resource_limit() {
    // A loop that never finishes, capped at 50 turns by the host rather than by
    // the program: the failure is a `Limit`, which is a kind of its own and not
    // a `Runtime` that reads like one.
    let source = r#"
set stopped to 0
try
    while stopped is 0
        set stopped to stopped
    end
catch error of Limit failure
    say "limit: {failure.kind}"
end
say "after the loop was stopped"
"#;

    let (tree, _bytes) = both_engines_capped(source, 50);
    assert_eq!(
        lines(&tree),
        vec!["limit: Limit", "after the loop was stopped"]
    );
}

#[test]
fn edge_the_all_kinds_form_catches_every_kind_a_program_can_name() {
    // `Error` is the supertype: the typed spelling of what the untyped `catch`
    // has always done. All three kinds it covers are caught by one handler, and
    // the kind each value carries still says which one it was — matching every
    // kind must not lose the answer to which.
    let source = r#"
try
    set sum to 1 + "a"
catch error of Error failure
    say "first {failure.kind}"
end
try
    set contents to files.read("typed-catch-missing-file.txt")
catch error of Error failure
    say "second {failure.kind}"
end
try
    set stopped to 0
    while stopped is 0
        set stopped to stopped
    end
catch error of Error failure
    say "third {failure.kind}"
end
"#;

    let (tree, _bytes) = both_engines_capped(source, 50);
    assert_eq!(
        lines(&tree),
        vec!["first Runtime", "second Io", "third Limit"]
    );
}

#[test]
fn edge_a_typed_handler_that_matches_nothing_falls_through_to_the_untyped_one_around_it() {
    // The typed handler declines, and the failure belongs to whatever is
    // written around it. An untyped `catch` around this one takes it, which is
    // the case a matcher that caught everything would never reach.
    let source = r#"
try
    try
        set sum to 1 + "a"
    catch error of Io
        say "the typed handler must not see a Runtime failure"
    end
catch error problem
    say "outer untyped handler: {problem}"
end
say "program ends here"
"#;

    let (tree, _bytes) = both_engines(source);
    assert_eq!(
        lines(&tree),
        vec!["outer untyped handler: error", "program ends here"]
    );
}

#[test]
fn edge_a_typed_handler_that_declines_still_runs_the_finally_it_is_owed() {
    // Declining is not leaving: the region's cleanup was promised whatever the
    // region does, and the failure is the thing that left the cleanup wanted.
    let source = r#"
try
    try
        set sum to 1 + "a"
    catch error of Io
        say "must not run"
    finally
        say "cleanup"
    end
catch error
    say "outer handler took it"
end
"#;

    let (tree, _bytes) = both_engines(source);
    assert_eq!(lines(&tree), vec!["cleanup", "outer handler took it"]);
}

#[test]
fn edge_an_empty_typed_handler_still_handles_the_failure_it_names() {
    // A handler with no body is a handler: the failure it matches is handled,
    // and the program carries on where the `try` was. An empty body is the
    // boundary case of "runs only for the kinds it names".
    let source = r#"
try
    set contents to files.read("typed-catch-missing-file.txt")
catch error of Io
end
try
    set sum to 1 + "a"
catch error of Io
end
say "unreachable"
"#;

    let (tree, _bytes) = both_engines(source);

    assert_eq!(tree.output, Vec::<String>::new());
    let (kind, message) = failure(&tree);
    assert_eq!(kind, "RuntimeError", "got {message:?}");
}

#[test]
fn edge_two_hundred_handles_in_a_row_cost_nothing_that_accumulates() {
    // 200 failures caught one after another, each bound and dropped before the
    // next. A failure value that cost stack per handler — a chain walked to
    // build it, a handler list nothing ever popped — is 200 deep here, and the
    // program either fails or never finishes.
    let source = r#"
repeat 200 times
    try
        set sum to 1 + "a"
    catch error of Runtime failure
        say failure.kind
    end
end
say "done"
"#;

    let (tree, _bytes) = both_engines(source);

    let printed = lines(&tree);
    assert_eq!(printed.len(), 201, "one line per catch, plus the last");
    assert!(printed[..200].iter().all(|line| line == "Runtime"));
    assert_eq!(printed[200], "done");
}

#[test]
fn edge_handlers_nested_as_deep_as_the_language_allows_each_bind_or_decline_a_failure() {
    // 60 nested typed handlers, the deepest a program may nest blocks — the
    // parser and the bytecode both refuse 64 — each with its own failure live
    // while the one inside it runs. Two directions, because a handler stack that
    // finds the *outermost* handler instead of the innermost one would satisfy
    // either test on its own.
    let depth = 60;

    // Level 0 is the innermost, and it catches the failure: the other 59 are
    // installed, live, and none of them is the one that takes it.
    let mut catching =
        "try\n    set sum to 1 + \"a\"\ncatch error of Runtime failure0\n    say \"0\"\nend\n"
            .to_string();
    for level in (1..depth).rev() {
        // The next level out goes in the *protected body*, so the innermost
        // handler is the one that is reached first.
        catching = format!(
            "try\n{}catch error of Runtime failure{level}\n    say \"{level}\"\nend\n",
            indent(&catching)
        );
    }
    catching.push_str("say \"done\"\n");
    let (tree, _bytes) = both_engines(&catching);
    assert_eq!(
        lines(&tree),
        vec!["0".to_string(), "done".to_string()],
        "only the innermost of {depth} nested handlers may take the failure"
    );

    // The other direction: all 60 decline a `Runtime` failure, because each one
    // names `Io`, and the untyped `catch` around them takes it — 60 declines
    // deep, which is where a VM that popped a handler it had already run would
    // hand the failure to the wrong level or drop it.
    let mut declining = String::from("try\n");
    for level in (0..depth).rev() {
        declining.push_str(&indent(&format!(
            "try\n    set sum to 1 + \"a\"\ncatch error of Io failure{level}\n    say \"must not run\"\nend\n"
        )));
    }
    declining.push_str("catch error problem\n    say \"outer took it: {problem}\"\nend\n");
    declining.push_str("say \"done\"\n");
    let (tree, _bytes) = both_engines(&declining);
    assert_eq!(
        lines(&tree),
        vec!["outer took it: error", "done"],
        "every one of {depth} nested handlers must decline it"
    );
}

#[test]
fn edge_a_typed_handler_that_wrote_no_name_still_binds_the_word_error() {
    // `catch error of Io` names no binder of its own, so it binds `error` — the
    // word the untyped handler binds and the one it is written where. The
    // default is the only thing the two forms share, and it is what a handler
    // written as `catch error of Io` has to reach the record through, so it is
    // read here rather than only written about: a change that dropped the
    // default binder would leave every test that spells a name passing.
    let source = r#"
try
    set contents to files.read("typed-catch-missing-file.txt")
catch error of Io
    say type_of(error)
    say error.kind
    say error.message
end
say "after"
"#;

    let (tree, _bytes) = both_engines(source);

    let printed = lines(&tree);
    assert_eq!(
        printed[0], "record",
        "the default binder is the failure itself"
    );
    assert_eq!(
        printed[1], "Io",
        "and it carries the kind the handler named"
    );
    assert!(
        printed[2].contains("typed-catch-missing-file.txt"),
        "and the failure's own message, got {:?}",
        printed[2]
    );
    assert_eq!(printed[3], "after", "the handler handled it: {printed:?}");

    // The same record by every field the default binder reaches, on both engines.
    let same = r#"
try
    set sum to 1 + "a"
catch error of Runtime
    set copy to {kind: error.kind, message: error.message, line: error.line, column: error.column}
    say error is copy
    say error.line is greater than 0
end
"#;
    let (tree, _bytes) = both_engines(same);
    let printed = lines(&tree);
    assert_eq!(
        printed[0], "yes",
        "the record equals one holding its own fields"
    );
    assert_eq!(printed[1], "yes", "and it says where it happened");
}

#[test]
fn edge_of_without_a_kind_is_refused_rather_than_read_as_a_binder() {
    // `of` is the head of a typed catch, so `catch error of` with nothing after
    // it names no kind and there is no kind it can fall back to: reading it as a
    // binder spelled `of` would leave the handler matching everything and would
    // make a half-written kind a program that runs. Both engines refuse it, and
    // the refusal says what was expected.
    for source in [
        "try\n    set sum to 1 + \"a\"\ncatch error of\n    say \"nope\"\nend\n",
        // A kind on the next line is the body of a handler that named none.
        "try\n    set sum to 1 + \"a\"\ncatch error of\n    Runtime\nend\n",
        "try\n    set sum to 1 + \"a\"\ncatch error of 1\n    say \"nope\"\nend\n",
    ] {
        let (tree, _bytes) = both_engines(source);
        let (label, message) = failure(&tree);
        assert_eq!(label, "ParserError", "for {source:?}, got {message:?}");
        assert!(
            message.contains("expected a failure kind"),
            "the refusal says what was expected, got {message:?}"
        );
        for name in ["Error", "Runtime", "Io", "Limit"] {
            assert!(
                message.contains(name),
                "the refusal must list {name}, got {message:?}"
            );
        }
    }
}

#[test]
fn edge_a_catch_naming_a_kind_this_build_does_not_know_matches_no_failure() {
    // The kind travels into the `.rbc` as text, so a file can carry one this
    // build has no name for — written by a build that had it. Reading that as
    // "no kind at all" would turn a handler that matches nothing into one that
    // catches everything, which is the opposite of what the file says, so the
    // two absences are kept apart: a catch block with no parameter is untyped
    // and catches everything, and a catch block with a parameter this build
    // cannot resolve catches nothing.
    let mut chunk = compile_source(
        "try\n    set sum to 1 + \"a\"\ncatch error of Runtime problem\n    say \"must not run\"\nend\nsay \"after\"\n",
    )
    .expect("the typed program compiles");
    assert_eq!(
        chunk.main.blocks[0].params,
        vec!["Runtime".to_string()],
        "the kind is the catch block's only parameter"
    );

    chunk.main.blocks[0].params = vec!["Banana".to_string()];
    let mut vm = BytecodeVm::new();
    vm.set_echo(false);
    let result = vm.run(&chunk);
    let printed = vm.take_output();

    assert!(
        printed.is_empty(),
        "a handler matching no failure must not run its body, got {printed:?}"
    );
    let error = result.expect_err("the failure is not handled, so it is reported");
    assert_eq!(error.label(), "RuntimeError", "got {}", error.message());
    assert!(
        error.message().contains("add"),
        "and it is the failure the handler declined, got {}",
        error.message()
    );
}

/// One four-space stop in front of every line of `source`.
fn indent(source: &str) -> String {
    source
        .lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[test]
fn the_untyped_catch_still_binds_the_word_error() {
    // The untyped form is untouched by any of this: it binds the word `error`,
    // on both engines, and what a program can read out of it is the text it read
    // before. `tests/catch_binding_test.rs` holds the rest of that suite and runs
    // unmodified; this pins the three spellings here against their exact output,
    // because a typed catch shares this parse path and a change to it would show
    // up as these three lines changing.
    let cases = [
        (
            "try\n    set sum to 1 + \"a\"\ncatch error problem\n    say type_of(problem)\n    say problem\nend\n",
            vec!["text", "error"],
        ),
        (
            "try\n    set sum to 1 + \"a\"\ncatch e\n    say type_of(e)\n    say e\nend\n",
            vec!["text", "error"],
        ),
        (
            "try\n    set sum to 1 + \"a\"\ncatch\n    say \"caught\"\nend\n",
            vec!["caught"],
        ),
    ];

    for (source, expected) in cases {
        let (tree, _bytes) = both_engines(source);
        assert_eq!(lines(&tree), expected, "for {source:?}");
    }
}

#[test]
fn every_kind_the_language_documents_is_the_only_kind_a_catch_may_name() {
    // The closed list, from both ends. Each name the parser accepts catches the
    // failures of its own kind, and a name outside the list is refused by name
    // with the real ones in the message — so a program cannot write a handler
    // that quietly matches nothing.
    //
    // One failure per kind that is cheap to raise, and the cross-product against
    // the handler that must decline it: a matcher that catches everything is
    // proved by nothing, so every pairing here has a direction that has to fail.
    let failures = [
        ("Runtime", "set sum to 1 + \"a\""),
        (
            "Io",
            "set contents to files.read(\"typed-catch-missing-file.txt\")",
        ),
        (
            "Limit",
            "set stopped to 0\n    while stopped is 0\n        set stopped to stopped\n    end",
        ),
    ];
    let cap = 50;

    // The label a failure is *reported* under when it is not caught, which is
    // not the same word as the kind it is matched by: a `Limit` is a kind of its
    // own and renders as a `RuntimeError`, because a limit is a runtime failure.
    fn label_of(kind: &str) -> &'static str {
        match kind {
            "Io" => "IoError",
            _ => "RuntimeError",
        }
    }

    for (name, _) in KINDS.iter().map(|kind| (kind.name(), *kind)) {
        assert_eq!(
            FailureKind::from_name(name),
            Some(FailureKind::from_name(name).expect("a name in the list resolves")),
            "{name} must name a kind"
        );
    }

    for (kind, raising) in &failures {
        let source = format!(
            "try\n    {raising}\ncatch error of {kind} failure\n    say \"{kind} took it\"\nend\nsay \"done\"\n"
        );
        let (tree, _bytes) = both_engines_capped(&source, cap);
        assert_eq!(
            lines(&tree),
            vec![format!("{kind} took it"), "done".to_string()],
            "a {kind} handler must catch a {kind} failure"
        );

        for (other, _) in failures.iter().filter(|(name, _)| name != kind) {
            let source = format!(
                "try\n    {raising}\ncatch error of {other} declined\n    say \"must not run\"\nend\nsay \"must not run\"\n"
            );
            let (tree, _bytes) = both_engines_capped(&source, cap);
            let (label, _) = failure(&tree);
            assert_eq!(
                label,
                label_of(kind),
                "a {other} handler must not catch a {kind} failure"
            );
        }

        // `Error` catches all three, and the value it binds still says which.
        let source = format!(
            "try\n    {raising}\ncatch error of Error failure\n    say failure.kind\nend\nsay \"done\"\n"
        );
        let (tree, _bytes) = both_engines_capped(&source, cap);
        assert_eq!(lines(&tree), vec![kind.to_string(), "done".to_string()]);
    }

    assert_eq!(
        FailureKind::from_name("Banana"),
        None,
        "a name outside the closed list must not resolve to a kind"
    );

    let invented =
        "try\n    set sum to 1 + \"a\"\ncatch error of Banana failure\n    say \"nope\"\nend\n";
    let (tree, _bytes) = both_engines(invented);
    let (label, message) = failure(&tree);
    assert_eq!(label, "ParserError", "got {message:?}");
    assert!(
        message.contains("Banana"),
        "the refusal names what was written, got {message:?}"
    );
    for name in ["Error", "Runtime", "Io", "Limit"] {
        assert!(
            message.contains(name),
            "the refusal must list {name}, got {message:?}"
        );
    }
}

#[test]
fn the_failure_a_handler_binds_is_the_record_the_host_documents() {
    // The shape, read from the host side: `redblue::failure_value` is what both
    // engines build, so asserting on it here pins the fields and their order for
    // a program that reads them by name.
    let error = Error::Runtime(
        "Cannot add non-numbers".to_string(),
        redblue::Span::new(7, 5),
    );
    let value = redblue::failure_value(&error);

    let fields = match &value {
        Value::Record(fields) => fields,
        other => panic!("a failure must be bound as a record, got {other:?}"),
    };
    let names: Vec<&str> = fields.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        vec!["kind", "message", "line", "column"],
        "the fields and the order a program reads them in"
    );
    assert_eq!(fields["kind"], Value::Text("Runtime".to_string()));
    assert_eq!(
        fields["message"],
        Value::Text("Cannot add non-numbers".to_string())
    );
    assert_eq!(fields["line"], Value::Number(7.0));
    assert_eq!(fields["column"], Value::Number(5.0));

    // A failure with no position says so with zeroes, which is a line no source
    // file has, rather than leaving the fields out.
    let io = redblue::failure_value(&Error::Io("no such file".to_string()));
    match &io {
        Value::Record(fields) => {
            assert_eq!(fields["kind"], Value::Text("Io".to_string()));
            assert_eq!(fields["line"], Value::Number(0.0));
            assert_eq!(fields["column"], Value::Number(0.0));
        }
        other => panic!("a failure must be bound as a record, got {other:?}"),
    }

    // The same failure is the same value every time it is read, so a program
    // comparing two catches of the same failure is not comparing two shapes.
    assert_eq!(redblue::failure_value(&error), value);
}

#[test]
fn the_formatter_keeps_the_kind_a_handler_matches() {
    // Dropping the kind would widen the handler into one that catches failures
    // the program wrote it not to catch, so a format round trip has to carry
    // it — and has to say it once.
    let source = r#"try
    set sum to 1 + "a"
catch error of Runtime failure
    say failure.kind
end
"#;

    let formatted = redblue::formatter::format(source).expect("a typed catch should format");
    assert!(
        formatted.contains("catch error of Runtime failure"),
        "the kind and the name must both survive formatting, got:\n{formatted}"
    );

    // And a handler that named no other name says `error` once.
    let default = r#"try
    set sum to 1 + "a"
catch error of Io
    say 1
end
"#;
    let formatted = redblue::formatter::format(default).expect("a typed catch should format");
    assert!(
        formatted.contains("catch error of Io\n"),
        "the default binder must not be written twice, got:\n{formatted}"
    );
}

#[test]
fn edge_a_typed_catch_changes_no_operand_shape_in_the_bytecode() {
    // The kind travels in the catch block's parameter list, which is a field
    // every block header already had. So: the same opcodes, the same operands,
    // the same block tree, the same format version — and a `.rbc` written by a
    // build that never heard of a typed catch still decodes.
    //
    // The two programs differ in one word and nothing else: same body, same
    // binder, same statements. The kind is in the pool, so a `PUSH_CONST` index
    // moves by one and is compared by the value it names rather than by its
    // number — the number is a pool position, not an operand shape.
    let untyped = "try\n    set sum to 1 + \"a\"\ncatch error problem\n    say problem\nend\n";
    let typed =
        "try\n    set sum to 1 + \"a\"\ncatch error of Runtime problem\n    say problem\nend\n";

    let plain = compile_source(untyped).expect("the untyped program compiles");
    let typed_chunk = compile_source(typed).expect("the typed program compiles");

    // Every instruction, in every block, spelled with its operands resolved.
    let listing_of = |chunk: &redblue::bytecode::Chunk| -> Vec<String> {
        fn block_lines(
            chunk: &redblue::bytecode::Chunk,
            block: &redblue::bytecode::Block,
            path: &str,
            into: &mut Vec<String>,
        ) {
            into.push(format!(
                "{path} ({}, arity {}) params {:?}",
                block.kind.name(),
                block.arity,
                block.params
            ));
            for instruction in &block.code {
                let operand = |value: u32| match instruction.opcode {
                    redblue::bytecode::Opcode::PushConst => {
                        format!(" = {:?}", chunk.constants[value as usize])
                    }
                    _ => value.to_string(),
                };
                into.push(format!(
                    "{:?} {} {}",
                    instruction.opcode,
                    operand(instruction.arg),
                    operand(instruction.aux)
                ));
            }
            for (index, child) in block.blocks.iter().enumerate() {
                block_lines(chunk, child, &format!("{path}.blocks[{index}]"), into);
            }
        }
        let mut lines = Vec::new();
        block_lines(chunk, &chunk.main, "main", &mut lines);
        lines
    };

    let plain_lines = listing_of(&plain);
    let typed_lines = listing_of(&typed_chunk);
    let difference: Vec<&String> = typed_lines
        .iter()
        .zip(plain_lines.iter())
        .filter(|pair| pair.0 != pair.1)
        .map(|(typed, _)| typed)
        .collect();
    assert_eq!(
        typed_lines.len(),
        plain_lines.len(),
        "the typed program must compile to the same number of instructions"
    );
    assert_eq!(
        difference.len(),
        1,
        "the only difference between the two listings must be the catch block's \
         parameter list, not an instruction:\n  typed: {typed_lines:?}\n  plain: \
         {plain_lines:?}"
    );
    assert!(
        difference[0].contains("params [\"Runtime\"]"),
        "and the difference must be the kind, got {:?}",
        difference[0]
    );
    assert!(
        !plain_lines.iter().any(|line| line.contains("Runtime")),
        "an untyped handler must carry no kind at all"
    );

    // `rb dis` writes it in the block header, and the `TRY` line is line for line
    // the line an untyped handler's is: no new operand, no new column.
    let listing = redblue::bytecode::disassemble(&typed_chunk);
    assert!(
        listing.contains("main.blocks[0] (catch body, arity 0) (Runtime)"),
        "the kind must be readable in the listing, got:\n{listing}"
    );
    let try_line = |listing: &str| {
        listing
            .lines()
            .find(|line| line.contains(" TRY "))
            .unwrap_or_else(|| panic!("no TRY instruction in:\n{listing}"))
            .to_string()
    };
    assert_eq!(
        try_line(&redblue::bytecode::disassemble(&plain)),
        try_line(&listing),
        "a typed handler must not change how TRY is written"
    );

    // The format did not move, so a `.rbc` written today decodes and still names
    // the kind the handler matches.
    assert_eq!(
        redblue::bytecode::FORMAT_VERSION,
        6,
        "the format version is unchanged"
    );
    let round_tripped =
        redblue::bytecode::Chunk::decode(&typed_chunk.encode()).expect("a .rbc decodes");
    assert_eq!(
        round_tripped.main.blocks[0].params,
        vec!["Runtime".to_string()],
        "the kind must survive the encoding"
    );
    let mut vm = BytecodeVm::new();
    vm.set_echo(false);
    let value = vm.run(&round_tripped).expect("the decoded chunk runs");
    let printed = vm.take_output();
    assert_eq!(
        printed.len(),
        1,
        "the handler says one thing, got {printed:?}"
    );
    assert!(
        printed[0].contains("kind: Runtime"),
        "the decoded handler must still bind the failure it matches, got {:?}",
        printed[0]
    );
    assert_eq!(
        value,
        Value::Nothing,
        "the program says its piece and stops"
    );
}
