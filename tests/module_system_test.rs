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
                message, "Expected End but got Eof",
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
