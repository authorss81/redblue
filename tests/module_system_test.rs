//! `module Name ... export ... end`, and `import Name as Alias`.
//!
//! Both halves of the module system had a keyword (`module` at
//! `src/lexer.rs`, `import` with an alias slot) and no behaviour behind it:
//! `module M ... end` was a `ParserError: Unexpected token Module`, and the
//! alias an import parsed was bound to `nothing`, so nothing could be called
//! through it. `docs/GRAMMAR.md` § 3.1 and `SPEC.md` § Modules describe the
//! system these tests pin.
//!
//! The rules:
//!
//! - `import Name as Alias` binds `Alias` as a name for the module `Name`, and
//!   `Alias.member(...)` runs the same function `Name.member(...)` does.
//! - A module namespace with no file behind it — `json`, `files`, `time` — is
//!   importable under any alias; only a name that is neither a module file nor
//!   a builtin namespace is `Cannot find module 'X'`.
//! - `module Name ... export ... end` declares a module. Its body runs in a
//!   scope of its own, so a module's functions are not the program's names.
//! - `export a, b` names what the module publishes; `export all` publishes
//!   everything the module defines. A name an `export` lists that the module
//!   does not define is a clean error naming it, not a silent omission.
//! - A module declared twice in one file is refused, and a module that imports
//!   itself is a caught `Runtime` error rather than a hang.
//!
//! Every test runs the whole pipeline — lexer, parser, analyzer, VM — as
//! `rb run` does, through [`redblue::run_source_value`].

use redblue::{Error, Value};

/// Runs the whole pipeline on `source` and returns the value of its last
/// statement.
#[track_caller]
fn eval(source: &str) -> Value {
    redblue::run_source_value(source)
        .unwrap_or_else(|error| panic!("`{}` should have run, failed with {:?}", source, error))
}

/// Runs the whole pipeline on `source` and returns the error it produced.
#[track_caller]
fn eval_err(source: &str) -> Error {
    redblue::run_source_value(source).expect_err("`source` should have failed")
}

/// [`Value::Number`] for a decimal written as text.
#[track_caller]
fn number(decimal: &str) -> Value {
    Value::Number(
        decimal
            .parse::<f64>()
            .unwrap_or_else(|_| panic!("`{}` should be a decimal number", decimal)),
    )
}

/// Asserts `source` fails at runtime with exactly `expected` as its message.
#[track_caller]
fn assert_runtime_error(source: &str, expected: &str) {
    match eval_err(source) {
        Error::Runtime(message, span) => {
            assert_eq!(
                message, expected,
                "`{}` failed with the wrong message",
                source
            );
            assert!(span.is_known(), "`{}` failed without a source span", source);
        }
        other => panic!(
            "`{}` should fail with a Runtime error, got {:?}",
            source, other
        ),
    }
}

/// The whole of the phase's headline claim: an import of a builtin namespace
/// under an alias, and a call through that alias.
#[test]
fn import_alias_binds_the_module_name() {
    assert_eq!(
        eval("import json as J\nJ.stringify(1)"),
        Value::Text("1".to_string()),
        "an alias should name the module it imported"
    );
    assert_eq!(
        eval("import json\njson.stringify(1)"),
        Value::Text("1".to_string()),
        "the module's own name should still work alongside its alias"
    );
}

/// A module file's functions are reachable through the name its import binds,
/// which is what `SPEC.md` § Modules writes: `import MathUtils as M` then
/// `M.circle_area(5)`.
#[test]
fn import_alias_reaches_a_module_files_function() {
    assert_eq!(
        eval("import MathUtils as M\nM.circle_area(2)"),
        number("12.56636"),
        "the alias should reach the function the module file declares"
    );
    assert_eq!(
        eval("import MathUtils\nMathUtils.degrees_to_radians(180)"),
        number("3.14159"),
        "the module's own name should reach the same function"
    );
}

/// A module declaration is a statement the VM runs: the body's bindings stay
/// inside it, and the program's own names are untouched.
#[test]
fn module_declaration_parses_and_runs() {
    assert_eq!(
        eval("set outside to 1\nmodule M\n    set inside to 2\n    export all\nend\ngive back outside"),
        number("1"),
        "a module declaration should not disturb the surrounding program"
    );
}

/// `module M ... export ... end` with no `export` at all declares nothing the
/// program can use, and is not an error.
#[test]
fn edge_module_with_nothing_exported_binds_nothing() {
    assert_eq!(
        eval("module M\n    set hidden to 1\nend\ngive back 0"),
        number("0"),
        "a module that exports nothing should leave the program alone"
    );
    let leaked = eval_err("module M\n    set hidden to 1\nend\ngive back hidden");
    assert!(
        matches!(leaked, Error::Analyzer(_, _) | Error::Runtime(_, _)),
        "a module's own name must not leak into the importing program, got {:?}",
        leaked
    );
}

/// An alias that is already a local stays the local: the import names a module,
/// it does not overwrite a variable the program already has.
#[test]
fn edge_import_alias_shadowing_a_local_is_the_local() {
    assert_eq!(
        eval("set J to 5\nimport json as J\nJ"),
        number("5"),
        "an import must not rebind a local that already holds the name"
    );
}

/// `export` naming something the module does not define is refused, with the
/// name in the message — a silent omission would publish a member that can
/// never be called.
#[test]
fn edge_export_of_an_undefined_name_is_refused() {
    assert_runtime_error(
        "module M\n    export missing\nend",
        "Module 'M' exports 'missing', which it does not define",
    );
}

/// Two declarations of one module name in one file: the second is refused
/// rather than shadowing the first.
#[test]
fn edge_module_redeclared_in_the_same_file_is_refused() {
    assert_runtime_error(
        "module M\n    export all\nend\nmodule M\n    export all\nend",
        "Module 'M' is already declared",
    );
    let caught = "\
set after to 0
try
    module Twice
        export all
    end
    module Twice
        export all
    end
catch error
    set after to 1
end
give back after";
    assert_eq!(
        eval(caught),
        number("1"),
        "a redeclared module should be catchable and the program should finish"
    );
}

/// A module that imports itself is a caught error, not a hang: the program
/// runs to the end after the catch, which is what "not a hang" means.
#[test]
fn edge_circular_import_is_a_caught_runtime_error() {
    assert_runtime_error(
        "module M\n    import M\n    export all\nend",
        "Circular import of module 'M'",
    );
    let caught = "\
set after to 0
try
    module Loop
        import Loop
        export all
    end
catch error
    set after to 1
end
give back after";
    assert_eq!(
        eval(caught),
        number("1"),
        "a circular import should be catchable and the program should finish"
    );
}

/// A name that is neither a module file nor a builtin namespace is an error
/// that names it, and it stays catchable so a program can carry on.
#[test]
fn edge_unknown_import_name_is_an_error_that_names_it() {
    assert_runtime_error(
        "import NoSuchModuleAnywhere",
        "Cannot find module 'NoSuchModuleAnywhere'",
    );
    let source = "\
set caught to no
try
    import NoSuchModuleAnywhere as N
catch error
    set caught to yes
end
give back caught";
    assert_eq!(
        eval(source),
        Value::YesNo(true),
        "an unknown module should be catchable and the program should continue"
    );
}

/// Calling through an alias for a member the module does not have names the
/// module and the member, instead of reporting an unknown variable.
#[test]
fn edge_alias_call_of_an_unknown_member_names_it() {
    assert_runtime_error(
        "import MathUtils as M\nM.not_a_function(1)",
        "Module 'MathUtils' has no function 'not_a_function'",
    );
}

/// `export all` publishes every function the module defines, and each is
/// callable through the module's name.
#[test]
fn export_all_publishes_the_modules_functions() {
    assert_eq!(
        eval(
            "module M\n    to twice(n)\n        give back n * 2\n    end\n    export all\nend\nM.twice(4)"
        ),
        number("8"),
        "`export all` should publish the module's functions"
    );
    assert_eq!(
        eval(
            "module M\n    to twice(n)\n        give back n * 2\n    end\n    export twice\nend\nM.twice(5)"
        ),
        number("10"),
        "`export name` should publish that one function"
    );
}

/// A module's `set` is a member it publishes; a `constant` is a program global
/// whatever block declares it, so the module has no member of that name.
#[test]
fn edge_a_module_constant_is_a_global_and_not_a_published_member() {
    let source = "module M\n    set count to 1\n    constant label to \"n\"\n    export all\nend\n";
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    let program = redblue::parser::parse(tokens).expect("source should parse");
    redblue::analyzer::analyze(&program).expect("source should analyze");

    let (vm, result) = redblue::run_isolated(&program);
    result.expect("the module declaration should run");

    let members: Vec<String> = vm
        .module_member_values("M")
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(
        members,
        vec!["count".to_string()],
        "the module publishes its `set` but not its `constant`"
    );
}

/// A module declaration with no body at all is the empty boundary: it declares
/// nothing and faults on nothing.
#[test]
fn edge_empty_module_declaration_is_accepted() {
    assert_eq!(
        eval("module M\nend\ngive back 0"),
        number("0"),
        "`module M` with an empty body should be accepted"
    );
}

/// Two modules, one calling the other's published function: a call reaches
/// through the module name and the module name is still live while the second
/// module's body runs.
#[test]
fn module_membership_reaches_across_two_modules() {
    assert_eq!(
        eval(
            "module A\n    to twice(n)\n        give back n * 2\n    end\n    export all\nend\n\
             module B\n    to quad(n)\n        give back A.twice(A.twice(n))\n    end\n    export quad\nend\n\
             B.quad(3)"
        ),
        number("12"),
        "a module's function should call another module's published function"
    );
}

/// The member a module publishes is a call, and the builtin it reaches checks
/// its own arguments: a number where the module wants text is the builtin type
/// error, not a silent coercion.
#[test]
fn edge_module_member_argument_type_mismatch_is_the_builtin_error() {
    assert_runtime_error("import json as J\nJ.parse(5)", "json.parse requires text");
}

/// Malformed declarations are refused by the parser, with a span, rather than
/// swallowing the rest of the file: an unclosed module, and an `export` with
/// nothing to export.
#[test]
fn edge_malformed_module_declarations_are_parser_errors() {
    match eval_err("module M\n    set x to 1") {
        Error::Parser(message, span) => {
            assert_eq!(
                message, "Expected 'end' but got Eof",
                "an unclosed module should say what it wanted"
            );
            assert!(span.is_known(), "an unclosed module failed without a span");
        }
        other => panic!(
            "an unclosed module should be a Parser error, got {:?}",
            other
        ),
    }
    match eval_err("module M\n    export\nend") {
        Error::Parser(message, span) => {
            assert_eq!(
                message, "Expected a name to export after 'export'",
                "an empty export should say what it wanted"
            );
            assert!(span.is_known(), "an empty export failed without a span");
        }
        other => panic!("an empty export should be a Parser error, got {:?}", other),
    }
}

/// A module name that is not an identifier is refused where it is written: the
/// declaration is what names the module, so it cannot start without one.
#[test]
fn edge_module_declaration_without_a_name_is_refused() {
    match eval_err("module 123\nend") {
        Error::Parser(message, span) => {
            assert_eq!(
                message, "Expected module name",
                "a nameless module should say what it wanted"
            );
            assert!(span.is_known(), "a nameless module failed without a span");
        }
        other => panic!(
            "a nameless module should be a Parser error, got {:?}",
            other
        ),
    }
}

/// The same name exported twice is published once: the list is a set of members
/// even though it is written with commas, and a module that exports a name it
/// does not define is still refused before anything runs.
#[test]
fn edge_duplicate_export_names_publish_once() {
    assert_eq!(
        eval(
            "module M\n    to twice(n)\n        give back n * 2\n    end\n    export twice, twice\nend\n\
             M.twice(6)"
        ),
        number("12"),
        "a name listed twice should still publish once"
    );
    assert_runtime_error(
        "module M\n    to twice(n)\n        give back n * 2\n    end\n    export missing, twice\nend",
        "Module 'M' exports 'missing', which it does not define",
    );
}

/// The old `to` spelling of the alias is still accepted, so a program written
/// before `as` existed keeps running.
#[test]
fn import_alias_accepts_the_older_to_spelling() {
    assert_eq!(
        eval("import json to J\nJ.stringify(2)"),
        Value::Text("2".to_string()),
        "`import json to J` should still bind the alias"
    );
    assert_eq!(
        eval("import files, json as J\nJ.stringify(3)"),
        Value::Text("3".to_string()),
        "a multi-import with an alias on the second name should resolve both"
    );
}

/// Declared command-line flags: `sys.parse` and the `Cli` module.
///
/// `sys.argv()` was the whole of the command-line surface, so every program
/// that wanted a flag wrote the same loop and its own usage text. `sys.parse`
/// takes a list of flag records, reads `sys.argv()` (or an explicit argument
/// list), and answers a record of typed fields. The declaration is the only
/// place a flag is named, so the parser and the usage text cannot disagree.
mod cli_flags {
    use std::path::Path;

    use super::*;

    fn cli_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
    }

    /// What both engines made of a program: the value or failure, and the lines
    /// printed. Every CLI case runs through this, so a tree-walking answer that
    /// the bytecode VM does not share fails the test rather than passing half
    /// the contract.
    struct CliRun {
        result: Result<Value, Error>,
        output: Vec<String>,
    }

    #[track_caller]
    fn cli_ran(source: &str) -> CliRun {
        let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
        let ast = redblue::parser::parse(tokens).expect("source should parse");
        redblue::analyzer::analyze_in(&ast, cli_root()).expect("source should analyze");

        let (mut walker, walked) = redblue::run_isolated_in(&ast, cli_root());
        let walked_output = walker.take_output();

        let chunk = redblue::bytecode::compile_program(&ast)
            .unwrap_or_else(|error| panic!("`{source}` should compile, failed with {error:?}"));
        let mut vm = redblue::bytecode::vm::BytecodeVm::new_in(cli_root());
        vm.set_echo(false);
        let compiled = vm.run(&chunk);
        let compiled_output = vm.take_output();

        match (&walked, &compiled) {
            (Ok(walked_value), Ok(compiled_value)) => assert_eq!(
                walked_value, compiled_value,
                "the two engines answered differently for `{source}`"
            ),
            (Err(walked_error), Err(compiled_error)) => {
                assert_eq!(
                    format!("{}: {}", walked_error.label(), walked_error.message()),
                    format!("{}: {}", compiled_error.label(), compiled_error.message()),
                    "the two engines failed differently for `{source}`"
                );
                assert_eq!(
                    walked_error.span(),
                    compiled_error.span(),
                    "the two engines failed at different source spans for `{source}`"
                );
            }
            _ => panic!(
                "the two engines disagree about `{source}`: walked {walked:?}, compiled {compiled:?}"
            ),
        }
        assert_eq!(
            walked_output, compiled_output,
            "the two engines printed different lines for `{source}`"
        );

        CliRun {
            result: walked,
            output: walked_output,
        }
    }

    #[track_caller]
    fn cli_eval(source: &str) -> Value {
        cli_ran(source)
            .result
            .unwrap_or_else(|error| panic!("`{source}` should have run, failed with {error:?}"))
    }

    #[track_caller]
    fn cli_eval_err(source: &str) -> Error {
        cli_ran(source)
            .result
            .expect_err("`source` should have failed")
    }

    #[track_caller]
    fn cli_assert_runtime_error(source: &str, expected: &str) {
        match cli_eval_err(source) {
            Error::Runtime(message, span) => {
                assert_eq!(
                    message, expected,
                    "`{source}` failed with the wrong message"
                );
                assert!(span.is_known(), "`{source}` failed without a source span");
            }
            other => panic!("`{source}` should fail with a Runtime error, got {other:?}"),
        }
    }

    /// Runs `source` and answers the lines it printed, asserting the run succeeded.
    #[track_caller]
    fn cli_output(source: &str) -> Vec<String> {
        let run = cli_ran(source);
        run.result
            .unwrap_or_else(|error| panic!("`{source}` should have run, failed with {error:?}"));
        run.output
    }

    #[track_caller]
    fn cli_field<'a>(record: &'a Value, name: &str) -> &'a Value {
        let Value::Record(fields) = record else {
            panic!("expected a record, got {}", record.type_name());
        };
        fields
            .get(name)
            .unwrap_or_else(|| panic!("the record should have a `{name}` field"))
    }

    /// The headline claim: one declaration gives typed, defaulted fields, and
    /// the parser handles a repeated flag (last wins) and a flag after a
    /// positional.
    #[test]
    fn declared_flags_parse_into_a_typed_record() {
        let parsed = cli_eval(
            "set options to [{name: \"output\", type: \"text\", default: \"out.txt\"}, {name: \"count\", type: \"number\", required: yes}, {name: \"verbose\", type: \"yes/no\"}]\n\
             set parsed to sys.parse(options, [\"first\", \"--count\", \"1\", \"--verbose\", \"--count=3\", \"--output\", \"x\"])\n\
             give back parsed",
        );

        assert_eq!(cli_field(&parsed, "count"), &Value::Number(3.0));
        assert_eq!(cli_field(&parsed, "verbose"), &Value::YesNo(true));
        assert_eq!(cli_field(&parsed, "output"), &Value::Text("x".to_string()));
        assert_eq!(cli_field(&parsed, "help"), &Value::YesNo(false));
        assert_eq!(
            cli_field(&parsed, "arguments"),
            &Value::list(vec![Value::Text("first".to_string())]),
            "a flag after a positional and the positional itself should both survive"
        );
        assert!(
            cli_field(&parsed, "usage")
                .to_string()
                .contains("--count <number> (required)"),
            "usage should be generated from the declaration"
        );
    }

    /// A path flag is a path value, not merely the text a path is made of.
    #[test]
    fn edge_path_flags_are_path_values_and_defaults_fill_in() {
        let parsed = cli_eval(
            "set options to [{name: \"where\", type: \"path\", default: \"out.txt\"}, {name: \"flag\", type: \"yes/no\"}]\n\
             set parsed to sys.parse(options)\n\
             give back parsed",
        );

        let where_value = cli_field(&parsed, "where");
        assert_eq!(where_value.type_name(), "path");
        assert_eq!(
            redblue::path::text_of(where_value),
            Some("out.txt"),
            "the default path should carry its text"
        );
        assert_eq!(
            cli_field(&parsed, "flag"),
            &Value::YesNo(false),
            "an absent boolean flag is no, not nothing"
        );
    }

    /// The empty string is a value, `--flag=value` is a spelling, and no
    /// arguments at all still produces the record.
    #[test]
    fn edge_flag_value_forms_cover_equals_empty_and_no_arguments() {
        let parsed = cli_eval(
            "set options to [{name: \"name\", type: \"text\"}]\n\
             give back sys.parse(options, [\"--name=\", \"--name\", \"value\"])",
        );
        assert_eq!(
            cli_field(&parsed, "name"),
            &Value::Text("value".to_string())
        );

        let empty = cli_eval(
            "set options to [{name: \"name\", type: \"text\"}]\n\
             give back sys.parse(options, [\"--name=\", \"after\"])",
        );
        assert_eq!(
            cli_field(&empty, "name"),
            &Value::Text(String::new()),
            "an explicitly empty value is a value, not a missing one"
        );
        assert_eq!(
            cli_field(&empty, "arguments"),
            &Value::list(vec![Value::Text("after".to_string())])
        );

        let none = cli_eval(
            "set options to [{name: \"name\", type: \"text\", default: \"fallback\"}]\n\
             give back sys.parse(options, [])",
        );
        assert_eq!(
            cli_field(&none, "name"),
            &Value::Text("fallback".to_string()),
            "no arguments should leave the default in place"
        );
    }

    /// A `yes/no` flag without `takes_value`, so a switch, refuses an inline
    /// value rather than silently discarding it. That includes `=false`: the
    /// way to spell an explicit boolean is `takes_value: yes` and a separate
    /// value, not an inline value on a switch.
    #[test]
    fn edge_inline_value_on_a_value_less_flag_is_refused() {
        cli_assert_runtime_error(
            "sys.parse([{name: \"verbose\", type: \"yes/no\"}], [\"--verbose=yes\"])",
            "Flag '--verbose' does not take a value",
        );
        cli_assert_runtime_error(
            "sys.parse([{name: \"verbose\", type: \"yes/no\"}], [\"--verbose=false\"])",
            "Flag '--verbose' does not take a value",
        );
    }

    /// Help is a request about the declaration, not a run with it: a required
    /// flag that was not given is not an error, and `--` ends flag parsing.
    #[test]
    fn edge_help_skips_required_flags_and_double_dash_ends_flags() {
        let parsed = cli_eval(
            "set options to [{name: \"count\", type: \"number\", required: yes}]\n\
             give back sys.parse(options, [\"--help\"])",
        );
        assert_eq!(cli_field(&parsed, "help"), &Value::YesNo(true));
        assert_eq!(
            cli_field(&parsed, "count"),
            &Value::Nothing,
            "help answers the declaration with no required-flag failure"
        );

        let after = cli_eval(
            "set options to [{name: \"count\", type: \"number\"}]\n\
             give back sys.parse(options, [\"--\", \"--count\", \"3\"])",
        );
        assert_eq!(
            cli_field(&after, "arguments"),
            &Value::list(vec![
                Value::Text("--count".to_string()),
                Value::Text("3".to_string())
            ]),
            "everything after `--` is positional, even text that looks like a flag"
        );
    }

    /// Unknown, missing and wrongly typed values are catchable failures naming
    /// the flag, rather than a silent `nothing` that travels on as a value.
    #[test]
    fn failures_name_the_flag_that_was_wrong() {
        cli_assert_runtime_error(
            "set options to [{name: \"count\", type: \"number\", required: yes}]\n\
             sys.parse(options, [\"--count\", \"three\"])",
            "Flag '--count' expects a number, but 'three' is not a number",
        );
        cli_assert_runtime_error(
            "set options to [{name: \"count\", type: \"number\", required: yes}]\n\
             sys.parse(options, [])",
            "Missing required flag '--count'",
        );
        cli_assert_runtime_error(
            "set options to [{name: \"count\", type: \"number\"}]\n\
             sys.parse(options, [\"--nope\", \"1\"])",
            "Unknown flag '--nope'",
        );
        cli_assert_runtime_error(
            "set options to [{name: \"name\"}]\n\
             sys.parse(options, [])",
            "Flag '--name' has no type",
        );
    }

    /// The `Cli` module prints the generated usage and does not call the body
    /// when help was asked for; without help the body is called.
    #[test]
    fn cli_module_prints_usage_and_skips_the_body_for_help() {
        let help = "import Cli\n\
                    set options to [{name: \"count\", type: \"number\", required: yes}]\n\
                    Cli.run_with(options, to(parsed)\n\
                        say \"body ran\"\n\
                    end, [\"--help\"])";
        let lines = cli_output(help);
        assert!(
            lines
                .iter()
                .any(|line| line.contains("--count <number> (required)")),
            "help should print the declaration's usage, got {lines:?}"
        );
        assert!(
            lines.iter().any(|line| line.contains("--help, -h")),
            "help should mention the help flag itself, got {lines:?}"
        );

        let run = "import Cli\n\
                   set options to [{name: \"count\", type: \"number\", required: yes}]\n\
                   Cli.run_with(options, to(parsed)\n\
                       say \"body ran\"\n\
                   end, [\"--count\", \"1\"])";
        assert_eq!(
            cli_output(run),
            vec!["body ran".to_string()],
            "without help the body is called"
        );

        let through_argv = "import Cli\n\
                            set options to [{name: \"count\", type: \"number\", default: 2}]\n\
                            Cli.run(options, to(parsed)\n\
                                say parsed.count\n\
                            end)";
        assert_eq!(
            cli_output(through_argv),
            vec!["2".to_string()],
            "the two-argument form reads sys.argv(), empty in this harness"
        );
    }

    /// A value-taking flag followed by another flag is a missing value, not a
    /// wrong-type value that quotes the other flag's spelling back at the user.
    #[test]
    fn edge_missing_value_before_another_flag_names_the_flag_and_type() {
        let source = "set options to [{name: \"count\", type: \"number\"}, {name: \"verbose\", type: \"yes/no\"}]\n\
                      sys.parse(options, [\"--count\", \"--verbose\"])";
        match cli_eval_err(source) {
            Error::Runtime(message, span) => {
                assert_eq!(
                    message, "Flag '--count' needs a value of type number",
                    "the failure should name the flag and its type"
                );
                assert!(span.is_known(), "the failure should carry a source span");
            }
            other => panic!("expected a Runtime error, got {other:?}"),
        }

        let ended = "set options to [{name: \"count\", type: \"number\"}]\n\
                     sys.parse(options, [\"--count\", \"--\"])";
        match cli_eval_err(ended) {
            Error::Runtime(message, span) => {
                assert_eq!(
                    message, "Flag '--count' needs a value of type number",
                    "`--` is not a value for `--count`"
                );
                assert!(span.is_known(), "the failure should carry a source span");
            }
            other => panic!("expected a Runtime error, got {other:?}"),
        }

        let unknown = "set options to [{name: \"count\", type: \"number\"}]\n\
                       sys.parse(options, [\"--count\", \"--bogus\"])";
        match cli_eval_err(unknown) {
            Error::Runtime(message, span) => {
                assert_eq!(
                    message, "Unknown flag '--bogus'",
                    "an undeclared flag in value position should name itself, not be swallowed"
                );
                assert!(span.is_known(), "the failure should carry a source span");
            }
            other => panic!("expected a Runtime error, got {other:?}"),
        }

        let unknown_text = "set options to [{name: \"name\", type: \"text\"}]\n\
                            sys.parse(options, [\"--name\", \"--bogus\"])";
        match cli_eval_err(unknown_text) {
            Error::Runtime(message, span) => {
                assert_eq!(
                    message, "Unknown flag '--bogus'",
                    "a text flag must not silently store an undeclared flag as its value"
                );
                assert!(span.is_known(), "the failure should carry a source span");
            }
            other => panic!("expected a Runtime error, got {other:?}"),
        }
    }

    /// The usage text is derived from the declaration, so adding a flag to the
    /// declaration is enough to add it to help.
    #[test]
    fn edge_usage_lists_a_flag_added_to_the_declaration() {
        let before = cli_eval(
            "set options to [{name: \"output\", type: \"text\", help: \"where to write\"}]\n\
             give back sys.parse(options, []).usage",
        );
        let after = cli_eval(
            "set options to [{name: \"output\", type: \"text\", help: \"where to write\"}, \
             {name: \"colour\", type: \"text\", default: \"auto\", help: \"when to colour\"}]\n\
             give back sys.parse(options, []).usage",
        );

        let before_text = match before {
            Value::Text(text) => text,
            other => panic!("usage should be text, got {}", other.type_name()),
        };
        let after_text = match after {
            Value::Text(text) => text,
            other => panic!("usage should be text, got {}", other.type_name()),
        };

        assert!(
            before_text.contains("--output <text>"),
            "the original declaration should be in help, got {before_text:?}"
        );
        assert!(
            !before_text.contains("--colour"),
            "the flag was not declared yet, so help should not invent it"
        );
        assert!(
            after_text.contains("--colour <text> (default: auto)  when to colour"),
            "a newly declared flag should appear in help with its value type, default and help text, got {after_text:?}"
        );
        assert!(
            after_text.contains("--help, -h"),
            "help should always describe its own flag, got {after_text:?}"
        );
    }

    /// A boolean flag may take an explicit yes/no value, and Unicode text and
    /// path values survive unchanged.
    #[test]
    fn edge_boolean_values_and_unicode_survive_the_parse() {
        let parsed = cli_eval(
            "set options to [{name: \"enabled\", type: \"yes/no\", takes_value: yes}, \
             {name: \"name\", type: \"text\"}, {name: \"where\", type: \"path\"}]\n\
             give back sys.parse(options, [\"--enabled\", \"false\", \"--name\", \"héllo 🌍\", \"--where\", \"dír/文件\"])",
        );

        assert_eq!(cli_field(&parsed, "enabled"), &Value::YesNo(false));
        assert_eq!(
            cli_field(&parsed, "name"),
            &Value::Text("héllo 🌍".to_string())
        );
        assert_eq!(
            redblue::path::text_of(cli_field(&parsed, "where")),
            Some("dír/文件"),
            "a path flag should keep its Unicode text"
        );

        let yes = cli_eval(
            "set options to [{name: \"enabled\", type: \"yes/no\", takes_value: yes}]\n\
             give back sys.parse(options, [\"--enabled=yes\"])",
        );
        assert_eq!(cli_field(&yes, "enabled"), &Value::YesNo(true));
    }

    /// The parse failures are ordinary catchable failures in Redblue, not a
    /// process abort, so a program can print its own message and carry on.
    #[test]
    fn edge_parse_failures_are_catchable_in_redblue() {
        let lines = cli_output(
            "set options to [{name: \"count\", type: \"number\", required: yes}]\n\
             set caught to no\n\
             try\n\
                 sys.parse(options, [\"--count\", \"three\"])\n\
             catch error problem\n\
                 set caught to yes\n\
             end\n\
             say caught\n\
             say \"after\"",
        );
        assert_eq!(
            lines,
            vec!["yes".to_string(), "after".to_string()],
            "the wrong-type failure should be caught and execution should continue"
        );
    }

    /// A declaration is a list of records with known fields, and a shape that
    /// is not one is refused before any argument is read.
    #[test]
    fn declaration_shape_failures_name_the_declaration() {
        cli_assert_runtime_error(
            "sys.parse(\"not a list\", [])",
            "sys.parse expects a list of flag declarations, but was given text",
        );
        cli_assert_runtime_error(
            "sys.parse([\"not a record\"], [])",
            "flag declaration 1 must be a record, but was given text",
        );
        cli_assert_runtime_error(
            "sys.parse([{type: \"text\"}], [])",
            "flag declaration 1 has no name",
        );
        cli_assert_runtime_error(
            "sys.parse([{name: 5, type: \"text\"}], [])",
            "flag declaration 1 has a number name where text belongs",
        );
        cli_assert_runtime_error(
            "sys.parse([{name: \"count\"}], [])",
            "Flag '--count' has no type",
        );
        cli_assert_runtime_error(
            "sys.parse([{name: \"count\", type: 5}], [])",
            "Flag '--count' has a number type where text belongs",
        );
        cli_assert_runtime_error(
            "sys.parse([{name: \"count\", type: \"integer\"}], [])",
            "Flag '--count' has unknown type 'integer'; expected text, number, yes/no or path",
        );
        cli_assert_runtime_error(
            "sys.parse([{name: \"enabled\", type: \"boolean\"}], [])",
            "Flag '--enabled' has unknown type 'boolean'; expected text, number, yes/no or path",
        );
    }

    /// The field values of a declaration have types too, and a flag may not
    /// both require a value and supply one.
    #[test]
    fn malformed_declarations_are_refused() {
        cli_assert_runtime_error(
            "sys.parse([{name: \"count\", type: \"number\", bogus: yes}], [])",
            "Flag '--count' has an unknown field 'bogus'",
        );
        cli_assert_runtime_error(
            "sys.parse([{name: \"count\", type: \"text\", takes_value: no}], [])",
            "Flag '--count' is text and takes no value, but only a yes/no flag may take no value",
        );
        cli_assert_runtime_error(
            "sys.parse([{name: \"count\", type: \"number\", required: yes, default: 1}], [])",
            "Flag '--count' cannot be required and have a default",
        );
        cli_assert_runtime_error(
            "sys.parse([{name: \"count\", type: \"number\", takes_value: \"yes\"}], [])",
            "Flag '--count' has a text takes_value where yes/no belongs",
        );
        cli_assert_runtime_error(
            "sys.parse([{name: \"count\", type: \"number\", required: \"yes\"}], [])",
            "Flag '--count' has a text required where yes/no belongs",
        );
        cli_assert_runtime_error(
            "sys.parse([{name: \"count\", type: \"number\", help: 5}], [])",
            "Flag '--count' has a number help where text belongs",
        );
    }

    /// A default must already have the flag's type, because it is handed to the
    /// program without going through the text conversion.
    #[test]
    fn defaults_must_have_the_declared_type() {
        for (declaration, expected) in [
            (
                "{name: \"name\", type: \"text\", default: 1}",
                "Flag '--name' has a number default where text belongs",
            ),
            (
                "{name: \"count\", type: \"number\", default: \"three\"}",
                "Flag '--count' has a text default where number belongs",
            ),
            (
                "{name: \"enabled\", type: \"yes/no\", default: 1}",
                "Flag '--enabled' has a number default where yes/no belongs",
            ),
            (
                "{name: \"where\", type: \"path\", default: 1}",
                "Flag '--where' has a number default where path belongs",
            ),
        ] {
            let source = format!("sys.parse([{declaration}], [])");
            cli_assert_runtime_error(&source, expected);
        }
    }

    /// A name written twice would silently overwrite the first declaration, and
    /// a name the parser reserves would collide with the record it answers.
    #[test]
    fn duplicate_reserved_and_unusable_names_are_refused() {
        cli_assert_runtime_error(
            "sys.parse([{name: \"count\", type: \"number\"}, {name: \"count\", type: \"text\"}], [])",
            "Flag '--count' is declared twice",
        );
        for reserved in ["arguments", "help", "usage"] {
            let source = format!("sys.parse([{{name: \"{reserved}\", type: \"text\"}}], [])");
            cli_assert_runtime_error(
                &source,
                &format!("'{reserved}' is a field sys.parse reserves, so it cannot name a flag"),
            );
        }
        cli_assert_runtime_error(
            "sys.parse([{name: \"\", type: \"text\"}], [])",
            "a flag name cannot be empty",
        );
        for unusable in ["-count", "two words", "a=b"] {
            let source = format!("sys.parse([{{name: \"{unusable}\", type: \"text\"}}], [])");
            cli_assert_runtime_error(&source, &format!("'{unusable}' is not a usable flag name"));
        }
    }

    /// The explicit argument list is a list of text, and one item that is not
    /// text names its position rather than being coerced.
    #[test]
    fn argument_list_failures_name_the_item() {
        cli_assert_runtime_error(
            "sys.parse([{name: \"count\", type: \"number\"}], [1])",
            "sys.parse expects a list of text arguments, but item 1 is number",
        );
        cli_assert_runtime_error(
            "sys.parse([{name: \"count\", type: \"number\"}], 5)",
            "sys.parse expects its arguments as a list of text, but was given number",
        );
    }

    /// Numbers that parse as a float but are not finite are refused just as
    /// text that is not a number is; negative values are still values because
    /// only another flag ends one.
    #[test]
    fn numeric_boundary_refuses_non_finite_text_and_keeps_negatives() {
        for text in ["NaN", "inf", "-inf", "1e400"] {
            let source = format!(
                "sys.parse([{{name: \"count\", type: \"number\"}}], [\"--count\", \"{text}\"])"
            );
            cli_assert_runtime_error(
                &source,
                &format!("Flag '--count' expects a number, but '{text}' is not a finite number"),
            );
        }

        let negative = cli_eval(
            "give back sys.parse([{name: \"count\", type: \"number\"}], [\"--count\", \"-3\"]).count",
        );
        assert_eq!(negative, Value::Number(-3.0));

        let finite = cli_eval(
            "give back sys.parse([{name: \"count\", type: \"number\"}], [\"--count\", \"1.5\"]).count",
        );
        assert_eq!(finite, Value::Number(1.5));
    }

    /// A negative number is a positional value, not an unknown short flag, so
    /// a command-line tool can be given `-5`; a real unknown short flag is
    /// still refused and names itself.
    #[test]
    fn edge_negative_positionals_are_values_not_unknown_flags() {
        let parsed = cli_eval(
            "set options to [{name: \"count\", type: \"number\"}]\n\
             give back sys.parse(options, [\"-5\", \"--count\", \"-2\", \"-0.5\"])",
        );
        assert_eq!(
            cli_field(&parsed, "arguments"),
            &Value::list(vec![
                Value::Text("-5".to_string()),
                Value::Text("-0.5".to_string())
            ]),
            "negative numbers are positional values, not short flags"
        );
        assert_eq!(cli_field(&parsed, "count"), &Value::Number(-2.0));

        let failed = cli_ran("sys.parse([], [\"-x\"])");
        assert!(
            failed.result.is_err(),
            "an unknown short flag must still be refused, got {:?}",
            failed.result
        );
        match failed.result {
            Err(Error::Runtime(message, span)) => {
                assert_eq!(message, "Unknown flag '-x'");
                assert!(span.is_known(), "the failure should carry a source span");
            }
            other => panic!("expected a Runtime error for `-x`, got {other:?}"),
        }
    }

    /// A repeated flag uses its last value, and a flag after a positional is
    /// still parsed rather than being swallowed into the positionals.
    #[test]
    fn repeated_flags_last_wins_after_a_positional() {
        let parsed = cli_eval(
            "set options to [{name: \"name\", type: \"text\", default: \"fallback\"}]\n\
             give back sys.parse(options, [\"first\", \"--name=one\", \"second\", \"--name\", \"two\"])",
        );
        assert_eq!(
            cli_field(&parsed, "name"),
            &Value::Text("two".to_string()),
            "a repeated flag should keep its last value"
        );
        assert_eq!(
            cli_field(&parsed, "arguments"),
            &Value::list(vec![
                Value::Text("first".to_string()),
                Value::Text("second".to_string())
            ]),
            "flags between positionals should not become positionals"
        );
    }

    /// Help must not run a body that would fail loudly, because help is a
    /// request about the declaration rather than a run with it.
    #[test]
    fn edge_help_does_not_run_a_body_that_would_fail() {
        let lines = cli_output(
            "import Cli\n\
             set options to [{name: \"count\", type: \"number\", required: yes}]\n\
             Cli.run_with(options, to(parsed)\n\
                 sys.parse(\"the body must not run\")\n\
             end, [\"--help\"])",
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("--count <number> (required)")),
            "help should print the generated usage, got {lines:?}"
        );
        assert!(
            lines.iter().any(|line| line.contains("--help, -h")),
            "help should mention itself, got {lines:?}"
        );
    }

    /// An empty declaration is legal: no flags, no arguments, and usage that
    /// still describes help itself.
    #[test]
    fn edge_empty_declaration_and_no_arguments() {
        let parsed = cli_eval("give back sys.parse([], [])");
        assert_eq!(
            cli_field(&parsed, "arguments"),
            &Value::list(Vec::new()),
            "no arguments means an empty positional list"
        );
        assert_eq!(cli_field(&parsed, "help"), &Value::YesNo(false));
        let usage = cli_field(&parsed, "usage").to_string();
        assert!(
            usage.contains("Options:") && usage.contains("--help, -h"),
            "usage should still describe help, got {usage:?}"
        );
    }

    /// Help is a request about the declaration even when it stands where a
    /// value was expected, so it does not become a missing-value error.
    #[test]
    fn edge_help_in_value_position_answers_usage() {
        for help in ["--help", "-h"] {
            let source = format!(
                "set options to [{{name: \"count\", type: \"number\", required: yes}}]\n\
                 give back sys.parse(options, [\"--count\", \"{help}\"])"
            );
            let parsed = cli_eval(&source);
            assert_eq!(cli_field(&parsed, "help"), &Value::YesNo(true));
            assert_eq!(
                cli_field(&parsed, "count"),
                &Value::Nothing,
                "help should skip the required flag even in value position"
            );
        }
    }
}
