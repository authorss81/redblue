//! `set x, y to 1, 2` — multiple assignment, as `docs/GRAMMAR.md` § 4.1 writes.
//!
//! The grammar described a declaration that bound several names in one
//! statement, and the parser stopped at the first comma (`ParserError: Expected
//! 'to' but got Comma`). These tests pin the behaviour: every name is bound,
//! all right-hand sides are evaluated before any name is written, and a name
//! count that does not match the value count is refused.

use std::path::{Path, PathBuf};

use redblue::{Error, Value};

/// Runs the whole pipeline on `source` and returns the value of its last
/// statement.
#[track_caller]
fn eval(source: &str) -> Value {
    redblue::run_source_value(source)
        .unwrap_or_else(|error| panic!("`{}` should have run, failed with {:?}", source, error))
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

/// The headline claim: two names on the left, two values on the right.
#[test]
fn multiple_assignment_binds_each_name() {
    assert_eq!(
        eval("set x, y to 1, 2\ngive back x + y"),
        number("3"),
        "both names should be bound to their value"
    );
    assert_eq!(
        eval("set x, y to 1, 2\ngive back x"),
        number("1"),
        "the first name should keep the first value"
    );
    assert_eq!(
        eval("set x, y to 1, 2\ngive back y"),
        number("2"),
        "the second name should keep the second value"
    );
}

/// Runs the whole pipeline on `source` and returns the error it produced.
#[track_caller]
fn eval_err(source: &str) -> Error {
    redblue::run_source_value(source).expect_err("`source` should have failed")
}

/// A parse failure is what an unsupported spelling has to produce: not a
/// silently ignored `set`, and not a panic.
#[track_caller]
fn assert_parser_error(source: &str) {
    match eval_err(source) {
        Error::Parser(message, span) => {
            assert!(
                !message.is_empty(),
                "`{}` failed with an empty parser message",
                source
            );
            assert!(span.is_known(), "`{}` failed without a source span", source);
        }
        other => panic!(
            "`{}` should fail with a Parser error, got {:?}",
            source, other
        ),
    }
}

/// Three names and three values, the other size the grammar allows.
#[test]
fn multiple_assignment_three_names() {
    assert_eq!(
        eval("set x, y, z to 1, 2, 3\ngive back x * 100 + y * 10 + z"),
        number("123"),
        "three names should bind in order"
    );
}

/// Every right-hand side is evaluated before any name is written, so a swap
/// exchanges the two values instead of giving both the first one.
#[test]
fn multiple_assignment_evaluates_before_binding() {
    assert_eq!(
        eval("set x, y to 1, 2\nset x, y to y, x\ngive back x * 10 + y"),
        number("21"),
        "the right-hand sides should be read before either name is written"
    );
}

/// A trailing comma after the last name is not in the grammar, so it is refused
/// rather than accepted as an empty extra name.
#[test]
fn edge_trailing_comma_in_names_is_refused() {
    assert_parser_error("set x, y, to 1, 2");
}

/// A trailing comma after the last value has no expression behind it, so it is
/// refused.
#[test]
fn edge_trailing_comma_in_values_is_refused() {
    assert_parser_error("set x, y to 1, 2,");
}

/// More names than values: the missing right-hand side is a parse failure, not
/// a name bound to `nothing`.
#[test]
fn edge_missing_right_hand_side_is_refused() {
    assert_parser_error("set x, y to 1");
}

/// More values than names is the same arity mismatch in the other direction.
#[test]
fn edge_extra_right_hand_side_is_refused() {
    assert_parser_error("set x to 1, 2");
}

/// One name and one value is the ordinary `set`, and still parses as such.
#[test]
fn single_assignment_still_works() {
    assert_eq!(
        eval("set x to 1\ngive back x"),
        number("1"),
        "a `set` with one name should keep its old meaning"
    );
}

/// `import Mod` and `import Mod as Alias` keep working while the dotted form is
/// unsupported.
#[test]
fn existing_import_forms_still_work() {
    assert_eq!(
        eval("import MathUtils as M\nM.circle_area(2)"),
        number("12.56636"),
        "`import a as b` should still bind the alias"
    );
    assert_eq!(
        eval("import MathUtils\nMathUtils.degrees_to_radians(180)"),
        number("3.14159"),
        "`import Mod` should still bind the module's own name"
    );
    assert_eq!(
        eval("import json\njson.stringify(1)"),
        Value::Text("1".to_string()),
        "a builtin namespace should still import without an alias"
    );
}

/// `docs/GRAMMAR.md` § 3.1 marks `import a.b as c` Not Implemented; this is the
/// test that pins the refusal until a phase implements it.
#[test]
fn edge_dotted_import_with_alias_is_refused() {
    assert_parser_error("import math.sqrt as s\nsay s(9)");
}

/// A dotted import with no alias is the same unsupported production.
#[test]
fn edge_dotted_import_without_alias_is_refused() {
    assert_parser_error("import math.sqrt");
}

/// A dotted import naming a member that does not exist is refused by the same
/// parser path, before any module is loaded.
#[test]
fn edge_dotted_import_missing_name_is_refused() {
    assert_parser_error("import math.no_such_member as n\nsay n(9)");
}

/// The bytecode compiler has its own arm for multiple assignment; it has to
/// agree with the tree-walking VM, including the evaluate-before-bind rule.
#[test]
fn multiple_assignment_agrees_with_the_bytecode_vm() {
    let source = "set x, y to 1, 2\nset x, y to y, x\ngive back x * 10 + y\n";
    assert_eq!(eval(source), number("21"));

    let chunk = redblue::bytecode::compile_source(source).expect("source should compile");
    let mut vm = redblue::bytecode::vm::BytecodeVm::new();
    let result = vm.run(&chunk).expect("compiled source should run");
    assert_eq!(
        result,
        number("21"),
        "the bytecode VM should bind multiple names the same way"
    );
}

/// A constant among the targets is refused by both VMs at the same point: the
/// names are written in the same order, so a caught failure leaves the same
/// bindings behind.
#[test]
fn edge_a_constant_target_fails_the_same_way_on_both_vms() {
    let source = "\
set x to 0
constant y to 5
try
    set x, y to 1, 2
catch error
    set handled to yes
end
give back x
";
    assert_eq!(
        eval(source),
        number("0"),
        "the tree VM should refuse the constant before writing any name after it"
    );

    let chunk = redblue::bytecode::compile_source(source).expect("source should compile");
    let mut vm = redblue::bytecode::vm::BytecodeVm::new();
    let result = vm.run(&chunk).expect("compiled source should run");
    assert_eq!(
        result,
        number("0"),
        "the bytecode VM should leave the same binding behind"
    );
}

/// A constant among the *first* targets is refused before any name is written,
/// not after the names later in store order have already been bound. This is
/// the order the reverse-store path is easiest to get wrong in: a caught
/// failure must leave `y` unbound as well as leaving `x` at its constant.
#[test]
fn edge_a_constant_first_target_leaves_no_later_name_bound() {
    let source = "\
constant x to 5
try
    set x, y to 1, 2
catch error
    set caught to yes
end
set result to 0
try
    set result to y
catch error
    set result to 0
end
give back result
";
    assert_eq!(
        eval(source),
        number("0"),
        "the tree VM must not bind `y` when the first target is a constant"
    );

    let chunk = redblue::bytecode::compile_source(source).expect("source should compile");
    let mut vm = redblue::bytecode::vm::BytecodeVm::new();
    let result = vm.run(&chunk).expect("compiled source should run");
    assert_eq!(
        result,
        number("0"),
        "the bytecode VM must roll back `y` when the first target is a constant"
    );
}

/// A scratch directory of its own under `target/tmp`, so this test's module
/// cannot be another test's.
#[track_caller]
fn scratch_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/tmp/multiple-assignment-test")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the scratch directory should be creatable");
    dir
}

/// Compiles `source` after analysing it against `root`, so imports resolve
/// against the directory the module file was written to rather than the process
/// working directory. [`redblue::bytecode::compile_source`] would analyse
/// against `.` and report the module's names as unknown.
#[track_caller]
fn bytecode_chunk_in(source: &str, root: &Path) -> redblue::Chunk {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    let ast = redblue::parser::parse(tokens).expect("source should parse");
    redblue::analyzer::analyze_in(&ast, root).expect("source should analyze");
    redblue::bytecode::compile_program(&ast).expect("source should compile")
}

/// A module file's `set a, b to ...` binds both names for the importing program,
/// through the analyzer and through the loader, rather than advertising names
/// it cannot produce.
#[test]
fn edge_a_module_files_multiple_assignment_binds_every_name() {
    let dir = scratch_dir("module-multiple-assignment");
    std::fs::create_dir_all(dir.join("modules"))
        .expect("the modules directory should be creatable");
    std::fs::write(dir.join("modules/Pair.rb"), "set a, b to 1, 2\n")
        .expect("the module file should be writable");

    let result = redblue::run_source_value_in("import Pair\ngive back a * 10 + b\n", &dir)
        .unwrap_or_else(|error| panic!("the importing program should run, failed with {error:?}"));
    assert_eq!(
        result,
        number("12"),
        "the import should bind both names the module's multiple assignment declares"
    );
}

/// The bytecode VM compiles a module file down to the declarations it binds, so
/// a module's `set a, b to ...` has to survive that filter and bind both names
/// on the import path as well.
#[test]
fn edge_a_module_files_multiple_assignment_binds_every_name_on_the_bytecode_vm() {
    let dir = scratch_dir("module-multiple-assignment-bytecode");
    std::fs::create_dir_all(dir.join("modules"))
        .expect("the modules directory should be creatable");
    std::fs::write(dir.join("modules/Pair.rb"), "set a, b to 1, 2\n")
        .expect("the module file should be writable");

    let source = "import Pair\ngive back a * 10 + b\n";
    let chunk = bytecode_chunk_in(source, &dir);
    let mut vm = redblue::bytecode::vm::BytecodeVm::new_in(&dir);
    let result = vm
        .run(&chunk)
        .unwrap_or_else(|error| panic!("the importing program should run, failed with {error:?}"));
    assert_eq!(
        result,
        number("12"),
        "the bytecode import path should bind both names the module declares"
    );
}

/// A module's later declaration reads a name an earlier declaration of the
/// same module bound. This is the ordering `module_bindings` exists to keep,
/// and both the tree VM's loader and the bytecode import path must keep it.
#[test]
fn edge_a_module_later_declaration_reads_an_earlier_multiple_assignment() {
    let dir = scratch_dir("module-multiple-assignment-order");
    std::fs::create_dir_all(dir.join("modules"))
        .expect("the modules directory should be creatable");
    std::fs::write(
        dir.join("modules/Ordered.rb"),
        "set a, b to 1, 2
set c to a + b
",
    )
    .expect("the module file should be writable");

    let source = "import Ordered\ngive back c\n";
    let walked = redblue::run_source_value_in(source, &dir)
        .unwrap_or_else(|error| panic!("the tree VM should run, failed with {error:?}"));
    assert_eq!(
        walked,
        number("3"),
        "the module's later declaration should read both names the earlier one bound"
    );

    let chunk = bytecode_chunk_in(source, &dir);
    let mut vm = redblue::bytecode::vm::BytecodeVm::new_in(&dir);
    let compiled = vm
        .run(&chunk)
        .unwrap_or_else(|error| panic!("the bytecode VM should run, failed with {error:?}"));
    assert_eq!(
        compiled, walked,
        "the bytecode import path should bind the module's declarations in source order too"
    );
}

/// A module loader writes a multiple assignment's names in the same reverse
/// order as both VMs, but an import is atomic. With a constant among the
/// targets, `b` is attempted before the refused `a`, yet a caught import failure
/// leaves neither name bound on either VM.
#[test]
fn edge_a_modules_multiple_assignment_binds_in_the_vms_reverse_order() {
    let dir = scratch_dir("module-multiple-assignment-constant-order");
    std::fs::create_dir_all(dir.join("modules"))
        .expect("the modules directory should be creatable");
    std::fs::write(dir.join("modules/Pair.rb"), "set a, b to 1, 2\n")
        .expect("the module file should be writable");

    let source = "\
constant a to 0
try
    import Pair
catch error
    set caught to yes
end
set result to 0
try
    set result to b
catch error
    set result to 0
end
give back result
";
    let walked = redblue::run_source_value_in(source, &dir)
        .unwrap_or_else(|error| panic!("the tree VM should run, failed with {error:?}"));
    assert_eq!(
        walked,
        number("0"),
        "a failed import should leave no name bound on the tree VM"
    );

    let chunk = bytecode_chunk_in(source, &dir);
    let mut vm = redblue::bytecode::vm::BytecodeVm::new_in(&dir);
    let compiled = vm
        .run(&chunk)
        .unwrap_or_else(|error| panic!("the bytecode VM should run, failed with {error:?}"));
    assert_eq!(
        compiled, walked,
        "the loader and the bytecode VM must roll back a failed import the same way"
    );
}

/// An import is one unit: a module whose second declaration errors must not
/// leave the first declaration bound after the failure is caught.
#[test]
fn edge_a_failed_import_binds_nothing() {
    let dir = scratch_dir("module-failed-import-atomic");
    std::fs::create_dir_all(dir.join("modules"))
        .expect("the modules directory should be creatable");
    std::fs::write(
        dir.join("modules/Failed.rb"),
        "set first to 1\nset second to missing_name\n",
    )
    .expect("the module file should be writable");

    let source = "\
try
    import Failed
catch error
    set caught to yes
end
set result to 0
try
    set result to first
catch error
    set result to 0
end
give back result
";
    let walked = redblue::run_source_value_in(source, &dir)
        .unwrap_or_else(|error| panic!("the tree VM should run, failed with {error:?}"));
    assert_eq!(
        walked,
        number("0"),
        "the first declaration must not survive a later declaration's failure"
    );

    let chunk = bytecode_chunk_in(source, &dir);
    let mut vm = redblue::bytecode::vm::BytecodeVm::new_in(&dir);
    let compiled = vm
        .run(&chunk)
        .unwrap_or_else(|error| panic!("the bytecode VM should run, failed with {error:?}"));
    assert_eq!(
        compiled, walked,
        "both VMs should leave a failed import fully unbound"
    );
}

/// A name repeated on the left has two values that could both win; the grammar
/// does not define that, so the statement is refused.
#[test]
fn edge_duplicate_name_is_refused() {
    assert_parser_error("set x, x to 1, 2");
}

/// Each right-hand side is a full expression, not a bare literal.
#[test]
fn edge_multiple_assignment_values_are_full_expressions() {
    assert_eq!(
        eval("set x, y to 1 + 1, 2 * 3\ngive back x * 10 + y"),
        number("26"),
        "each right-hand side should be evaluated as an expression"
    );
}

/// A name may bind text and another may bind `nothing`; the form does not
/// restrict the value type.
#[test]
fn edge_multiple_assignment_binds_text_and_nothing() {
    assert_eq!(
        eval("set a, b to \"hello\", nothing\ngive back a"),
        Value::Text("hello".to_string()),
        "a name may bind text"
    );
    assert_eq!(
        eval("set a, b to \"hello\", nothing\ngive back b"),
        Value::Nothing,
        "a name may bind nothing"
    );
}

/// A missing value on the right is a parser error, asserted directly with
/// `is_err` rather than only through the helper above.
#[test]
fn edge_multiple_assignment_missing_value_is_err() {
    let result = redblue::run_source_value("set x, y to 1");
    assert!(
        result.is_err(),
        "a missing right-hand side must be a clean error"
    );
    assert!(
        matches!(result, Err(Error::Parser(_, _))),
        "the error should be a Parser error, got {:?}",
        result
    );
}

/// A dotted import with no alias is refused directly, not silently accepted.
#[test]
fn edge_dotted_import_without_alias_is_err() {
    let result = redblue::run_source_value("import math.sqrt");
    assert!(result.is_err(), "a dotted import must be a clean error");
    assert!(
        matches!(result, Err(Error::Parser(_, _))),
        "the error should be a Parser error, got {:?}",
        result
    );
}

/// A longer list keeps its source order: name `i` gets value `i`, not the
/// reverse order the VMs use internally when they store the values.
#[test]
fn edge_five_names_bind_in_source_order() {
    assert_eq!(
        eval("set a, b, c, d, e to 1, 2, 3, 4, 5\ngive back a * 10000 + b * 1000 + c * 100 + d * 10 + e"),
        number("12345"),
        "five names should bind in the order they are written"
    );
}

/// The left-hand side is names only. A literal there is a parse failure, not a
/// name silently dropped.
#[test]
fn edge_a_non_name_target_is_a_parser_error() {
    assert_parser_error("set 1, y to 2, 3");
}

/// The dotted import is refused at the parser, asserted with `is_err` so the
/// Not Implemented note in `docs/GRAMMAR.md` is pinned by a failing-path test.
#[test]
fn edge_dotted_import_with_alias_is_a_parser_error() {
    let result = redblue::run_source_value("import math.sqrt as s");
    assert!(result.is_err(), "a dotted import must be a clean error");
    assert!(
        matches!(result, Err(Error::Parser(_, _))),
        "the error should be a Parser error, got {:?}",
        result
    );
}
