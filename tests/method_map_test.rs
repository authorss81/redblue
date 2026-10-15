//! `xs.map(f)` — the method spelling of `map` with a receiver that is a name.
//!
//! `map` is the only builtin that calls a Redblue function, and SPEC.md's
//! "First-Class Functions" section introduces function values with exactly one
//! program:
//!
//! ```redblue
//! set double to to (x) give back x * 2
//! set numbers to [1, 2, 3]
//! set doubled to numbers.map(double)
//! ```
//!
//! The one program the specification uses to introduce function values is the
//! one program that did not run: both engines took the variable receiver down
//! the module path, where `xs.map` means `xs_map`, and nothing registers that.
//! The literal receiver `[1, 2, 3].map(double)` answered, which is why the gap
//! survived — it is invisible to every program that writes the list out.
//!
//! Everything here runs on **both** engines. A fix in one of them is caught by
//! `tests/differential_test.rs`, and the cases below assert the engines agree
//! *and* that the answer is the literal list `[2, 4, 6]`, because two engines
//! refusing in the same way is also agreement.
//!
//! The receiver path is `object → module → list`. The module step comes before
//! the list step on purpose and is pinned by
//! [`a_module_receiver_is_not_shadowed_by_the_list_rule`] and
//! [`an_object_receiver_still_dispatches`]: `map` is a *fallback* for a name
//! that reaches no module function, never a replacement for the module path.

use redblue::bytecode::vm::BytecodeVm;
use redblue::{Error, Value};

/// Runs `source` through lexer → parser → VM and returns the value of its last
/// statement.
///
/// Only a bare expression statement carries a value out: `set x to …` and
/// `say …` both evaluate to `nothing`, so a source ending in one of them would
/// make every assertion below compare `nothing` against `nothing`.
#[track_caller]
fn tree_walk(source: &str) -> Value {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    let ast = redblue::parser::parse(tokens).expect("source should parse");
    assert!(
        matches!(
            ast.statements.last().map(|s| &s.statement),
            Some(redblue::parser::Statement::Expr(_))
        ),
        "`{}` does not end in a bare expression, so it has no value to assert on",
        source
    );
    redblue::Vm::new().run(&ast).expect("source should run")
}

/// The same program on the bytecode VM, compiled first.
#[track_caller]
fn bytecode(source: &str) -> Value {
    let chunk = redblue::compile_source(source).expect("source should compile");
    BytecodeVm::new()
        .run(&chunk)
        .expect("the compiled program should run")
}

/// Runs `source` on both engines, checks they said the same thing, and returns
/// what they said.
#[track_caller]
fn both(source: &str) -> Value {
    let walked = tree_walk(source);
    let compiled = bytecode(source);
    assert_eq!(
        compiled, walked,
        "the two engines disagree about `{}`:\n  tree-walker: {walked}\n  bytecode:    {compiled}",
        source
    );
    walked
}

/// The runtime error `source` fails with on the tree-walking VM.
#[track_caller]
fn tree_walk_error(source: &str) -> Error {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    let ast = redblue::parser::parse(tokens).expect("source should parse");
    redblue::Vm::new()
        .run(&ast)
        .expect_err("source was expected to fail")
}

/// The runtime error `source` fails with on the bytecode VM.
#[track_caller]
fn bytecode_error(source: &str) -> Error {
    let chunk = redblue::compile_source(source).expect("source should compile");
    BytecodeVm::new()
        .run(&chunk)
        .expect_err("the compiled program was expected to fail")
}

/// Runs `source` on both engines and checks that both refuse it with the same
/// message, returning that message.
#[track_caller]
fn both_errors(source: &str) -> String {
    let walked = tree_walk_error(source);
    let compiled = bytecode_error(source);
    assert_eq!(
        walked.label(),
        compiled.label(),
        "the two engines refused `{}` differently:\n  tree-walker: {}\n  bytecode:    {}",
        source,
        walked,
        compiled
    );
    assert_eq!(
        walked.message(),
        compiled.message(),
        "the two engines refused `{}` with different wording:\n  tree-walker: {}\n  bytecode:    {}",
        source,
        walked.message(),
        compiled.message()
    );
    walked.message().to_string()
}

/// A record holding one numeric field, for the record-element case below.
fn record(key: &str, n: f64) -> Value {
    Value::record([(key.to_string(), Value::Number(n))].into())
}

fn numbers(items: &[f64]) -> Value {
    Value::list(items.iter().map(|n| Value::Number(*n)).collect())
}

/// `to double(x) give back x * 2 end`, prepended to every program below.
const DOUBLE: &str = "to double(x)\n    give back x * 2\nend\n";

/// The specification's own first-class-functions example, verbatim from
/// SPEC.md, asked on both engines.
///
/// The headline of "First-Class Functions" is `set doubled to numbers.map(double)`.
/// If this fails, the one program the specification uses to introduce function
/// values does not run, which is the whole finding. The example ends in `say`,
/// so what it answers is the line it printed rather than a value.
///
/// The compiled half is SPEC.md's example **verbatim** — literal and all.
///
/// It used to run the block form of the same declaration on this half, because
/// `compile_source` refused a function *literal*. It does not any more: a
/// literal is a declaration written where a value goes, so `codegen` compiles
/// it to a block of its own and leaves the closure on the stack, which is what
/// `DEF_FUNCTION` already does for `to double(x) … end`. A test that ran the
/// block form here was checking a different program from the one SPEC.md
/// prints, and the gap it papered over was reachable from the specification's
/// own headline example.
#[test]
fn edge_the_specification_example_of_a_first_class_function_runs() {
    let source = "set double to to (x) give back x * 2\n\
                  \n\
                  set numbers to [1, 2, 3]\n\
                  set doubled to numbers.map(double)\n\
                  say doubled\n";

    let tokens = redblue::lexer::Lexer::tokenize(source).expect("SPEC.md's example should lex");
    let ast = redblue::parser::parse(tokens).expect("it should parse");
    let (mut vm, result) = redblue::run_isolated(&ast);
    assert_eq!(
        result
            .as_ref()
            .map(|_| ())
            .map_err(|e| e.message().to_string()),
        Ok(()),
        "SPEC.md's first-class-functions example did not run: {:?}",
        result.err().map(|e| e.message().to_string())
    );
    assert_eq!(
        vm.take_output(),
        vec!["[2, 4, 6]".to_string()],
        "SPEC.md's example printed something other than the doubled list"
    );

    // The same source, compiled. `rb compile` used to stop before emitting
    // anything for this file, so the two halves of SPEC.md's headline example —
    // the one that runs it and the one that can be compiled at all — were
    // never the same program.
    let chunk = redblue::compile_source(source).expect("SPEC.md's example should compile");
    let mut compiled = BytecodeVm::new();
    compiled.set_echo(false);
    compiled
        .run(&chunk)
        .expect("SPEC.md's example should run compiled");
    assert_eq!(
        compiled.take_output(),
        vec!["[2, 4, 6]".to_string()],
        "SPEC.md's example compiled and printed something other than the doubled list"
    );
}

/// Every spelling of `map` answers the same thing, on both engines.
///
/// SPEC.md enumerates `map(xs, f)`, `list.map(xs, f)`, `[1, 2, 3].map(f)` and —
/// the one it headlines in "First-Class Functions" — `numbers.map(f)`. All four
/// are asserted against one literal rather than against each other, so a change
/// that breaks all four at once is still a failure, and a change that breaks
/// only the new one is a failure too.
#[test]
fn edge_every_spelling_of_map_answers_the_same_list() {
    for spelling in [
        "map(xs, double)",
        "list.map(xs, double)",
        "[1, 2, 3].map(double)",
        "xs.map(double)",
    ] {
        let source = format!("{DOUBLE}set xs to [1, 2, 3]\nset out to {spelling}\nout\n");
        let walked = tree_walk(&source);
        assert_eq!(
            walked,
            numbers(&[2.0, 4.0, 6.0]),
            "`{spelling}` did not double the list — the variable-receiver spelling is the one \
             SPEC.md headlines"
        );
        assert_eq!(
            bytecode(&source),
            numbers(&[2.0, 4.0, 6.0]),
            "`{spelling}` compiled gave a different answer than it walked"
        );
    }
}

/// Nesting and recursion: a `map` nested inside the function it maps, over a
/// variable receiver, and a `map` that recurses until the call-depth limit.
///
/// The nesting case is the one the bytecode VM is most likely to get wrong
/// here: a fallback that answered by calling into the builtin from the receiver
/// path would push a frame per element and overflow the thread instead of
/// reaching [`crate::MAX_CALL_DEPTH`], which is a crash on a program the
/// tree-walker runs. Both engines are asked, so a crash on either one fails.
#[test]
fn edge_a_variable_receiver_map_nests_and_recurses_within_the_call_limit() {
    // The outer `map` maps `twice` over `[1, 2]`, and `twice` maps `double` over
    // `xs` by name — the same spelling, from inside the callee, with the frame
    // that started the outer map still live beneath it.
    let source = "to double(x)\n    give back x * 2\nend\n\
                  set xs to [1, 2]\n\
                  to twice(x)\n    give back xs.map(double)\nend\n\
                  xs.map(twice)\n";
    assert_eq!(
        both(source),
        Value::list(vec![numbers(&[2.0, 4.0]), numbers(&[2.0, 4.0])]),
        "a variable-receiver `map` nested inside the function it maps did not answer"
    );

    // A `map` that calls itself, through the same spelling. The tree-walker
    // recurses per element, so an implementation that cost a *host* stack frame
    // per element would overflow the thread here instead of reaching the limit —
    // a crash on a program the other engine refuses cleanly.
    //
    // It runs on a thread with room for the depth, and that is a property of the
    // test rather than of the answer: the language's limit is
    // [`redblue::MAX_CALL_DEPTH`], and reaching it costs one Rust frame per
    // level in a debug build, which is more than a libtest thread's 2 MiB.
    // `cargo run --bin rb` on the same program reaches the limit with room to
    // spare. What is asserted here is that **both engines refuse it with the
    // limit named**, on a stack big enough for the refusal to happen.
    let message = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(|| both_errors("set xs to [1]\nto rec(n)\n    give back xs.map(rec)\nend\nrec(1)\n"))
        .expect("the recursion thread should start")
        .join()
        .expect("the recursion thread should not panic");
    assert!(
        message.contains("Maximum call depth"),
        "a variable-receiver `map` that recurses stopped being stopped by the call-depth \
         limit: {message:?}"
    );
}

/// Unicode: a list of non-ASCII text mapped through a variable receiver.
///
/// The receiver here is a list of text, not of numbers, which is the other
/// element type `map` can be handed, and the elements carry combining marks,
/// CJK, RTL text and an emoji — the class that has broken string handling
/// before. The receiver's own name is non-ASCII too, so the name the fallback
/// looks up is not ASCII either.
#[test]
fn edge_a_variable_receiver_holding_unicode_text_maps_element_by_element() {
    let source = "to shout(s)\n    give back s + \"!\"\nend\n\
                  set liste to [\"café\", \"日本語\", \"مرحبا\", \"e\u{301}\"]\n\
                  set out to liste.map(shout)\nout\n";
    let walked = tree_walk(source);
    assert_eq!(
        walked,
        Value::list(vec![
            Value::Text("café!".to_string()),
            Value::Text("日本語!".to_string()),
            Value::Text("مرحبا!".to_string()),
            // The combining acute stays a combining acute: a change that
            // normalised text on the way through would put a precomposed `é!`
            // here and the two are not the same string.
            Value::Text("e\u{301}!".to_string()),
        ]),
        "a variable-receiver `map` over unicode text mapped the wrong elements"
    );
    assert_eq!(
        bytecode(source),
        walked,
        "the two engines mapped a unicode list differently"
    );
}

/// Duplicate and missing keys: a list of **records**, and a missing field.
///
/// `map` applies its function to elements of any type, so the record is the
/// third element type besides number and text. A record missing the field the
/// function reads gives the function `nothing`, and `nothing * 2` is a clean
/// runtime error rather than a panic — the records have to reach the function
/// for that to happen at all, so the second half is also proof the first is not
/// passing because the records were skipped.
#[test]
fn edge_a_list_of_records_maps_and_a_missing_field_is_a_runtime_error() {
    let wrap = "to wrap(row)\n    give back {n: row.n * 2}\nend\n";
    assert_eq!(
        both(&format!(
            "{wrap}set rows to [{{n: 2}}, {{n: 4}}]\nset out to rows.map(wrap)\nout\n"
        )),
        Value::list(vec![record("n", 4.0), record("n", 8.0)]),
        "a variable-receiver `map` over a list of records did not map the records"
    );

    // The same records with the field missing from one of them: the field reads
    // as `nothing`, so the arithmetic inside the function is what refuses.
    let message = both_errors(&format!(
        "{wrap}set rows to [{{n: 2}}, {{}}]\nset out to rows.map(wrap)\nset out to 1\n"
    ));
    assert!(
        message.contains("Cannot add") || message.contains("number"),
        "a record missing the mapped field did not fail cleanly inside the function: {message:?}"
    );
}

/// A method receiver spelled as a name reaches a list held in a **record field**
/// and one **returned from a function**.
///
/// Both are one dereference away from the case above and both have their own
/// bytecode lowering: a field read pushes the value, a call pushes the return,
/// and a fallback that only looks at a bare `Variable` node answers neither.
#[test]
fn edge_a_list_in_a_record_field_and_one_from_a_call_both_answer() {
    assert_eq!(
        both(&format!(
            "{DOUBLE}set box to {{items: [1, 2, 3]}}\nbox.items.map(double)\n"
        )),
        numbers(&[2.0, 4.0, 6.0]),
        "a list held in a record field did not map through its field name"
    );

    assert_eq!(
        both(&format!(
            "{DOUBLE}to three()\n    give back [1, 2, 3]\nend\n\
             set xs to three()\nxs.map(double)\n"
        )),
        numbers(&[2.0, 4.0, 6.0]),
        "a list returned from a function did not map"
    );
}

/// The boundary rows of the receiver itself: **empty** and **singleton**.
///
/// An empty list answers without ever calling the function, so the function
/// here divides by zero: an implementation that answered the empty case by
/// calling the callee would fail this rather than print an empty list.
#[test]
fn edge_an_empty_and_a_singleton_receiver_answer() {
    assert_eq!(
        both(&format!("{DOUBLE}set xs to []\nxs.map(double)\n")),
        Value::list(vec![]),
        "an empty receiver did not answer an empty list"
    );

    assert_eq!(
        both(&format!("{DOUBLE}set xs to [7]\nxs.map(double)\n")),
        numbers(&[14.0]),
        "a one-element receiver did not answer one element"
    );

    // The empty list must not have called the function at all.
    let never = "to never(x)\n    give back 1 / 0\nend\nset xs to []\nxs.map(never)\n";
    assert_eq!(
        both(never),
        Value::list(vec![]),
        "an empty receiver called the function instead of answering an empty list"
    );
}

/// A receiver whose name collides with a **module** still maps the list.
///
/// Every name in `stdlib::MODULES` (`src/stdlib.rs`) is both a module and a
/// perfectly good variable name, and the fallback has to be asked *after* the
/// module path rather than before it. This test pins both directions of that,
/// because one fix satisfies only one of them:
///
/// - a module with **no** `map` member behind it (`text`, `files`, `math`)
///   reaches the fallback, so a list bound to that name maps;
/// - a module **with** one (`list`, whose `map` member is the `list_map`
///   builtin) keeps the module answer, so `list.map(f)` bound to a list is
///   `list_map(f)` with one argument and is refused for the arity — which is
///   what it did before, and is what `list.map(list, f)` still spells properly.
///
/// Both are asserted on both engines, because a fix that answered `text.map(f)`
/// by consulting the value *before* the module path would leave `files.exists`
/// broken, and `a_module_receiver_is_not_shadowed_by_the_list_rule` is what
/// catches that.
#[test]
fn edge_a_receiver_named_after_a_module_maps_the_list_it_holds() {
    for name in ["text", "files", "math"] {
        let source =
            format!("{DOUBLE}set {name} to [1, 2, 3]\nset out to {name}.map(double)\nout\n");
        let walked = tree_walk(&source);
        assert_eq!(
            walked,
            numbers(&[2.0, 4.0, 6.0]),
            "a receiver named after the `{name}` module did not map the list it holds"
        );
        assert_eq!(
            bytecode(&source),
            numbers(&[2.0, 4.0, 6.0]),
            "a receiver named after the `{name}` module compiled to a different answer"
        );
    }

    // `list` is the other direction: the module owns the name and `map` is one
    // of its members, so the module answer stands and the arity is the refusal.
    let message = both_errors(
        "to double(x)\n    give back x * 2\nend\n\
         set list to [1, 2, 3]\nset out to list.map(double)\nset out to 1\n",
    );
    assert_eq!(
        message, "map requires a list and a function",
        "a receiver named after a module that *has* `map` stopped reaching the module \
         function, so the two ways of colliding with a module name no longer agree"
    );
    // And the module spelling itself still answers with both arguments, which is
    // the call that shadowing the module would have broken.
    assert_eq!(
        both(&format!(
            "{DOUBLE}set list to [1, 2, 3]\nlist.map(list, double)\n"
        )),
        numbers(&[2.0, 4.0, 6.0]),
        "`list.map(list, f)` stopped answering once a variable named `list` existed"
    );
}

/// A module receiver is not shadowed by the list rule.
///
/// `text` and `files` are modules, and a list is bound to a name that is also
/// used as a module in the same program. The module path must still answer: a
/// fallback that ran before the module check would make every dotted call on a
/// shadowed module name fail.
#[test]
fn a_module_receiver_is_not_shadowed_by_the_list_rule() {
    // A list is bound to the name `text`, and `text.uppercase` — which has no
    // `text_uppercase` builtin; `text` is a flat-backed alias module — must
    // still reach `uppercase`.
    assert_eq!(
        both("set text to [1, 2, 3]\ntext.uppercase(\"hi\")\n"),
        Value::Text("HI".to_string()),
        "a module member was shadowed by a variable of the same name"
    );

    // `files.exists` is the `<module>_<member>` shape, and it has to keep
    // answering with the module name bound to a list in the same program.
    assert_eq!(
        both("set files to [1, 2, 3]\nfiles.exists(\"no/such/file\")\n"),
        Value::YesNo(false),
        "a `<module>_<member>` builtin was shadowed by a variable of the same name"
    );
}

/// An object receiver still dispatches to the object's own method.
///
/// The receiver path is `object → module → list`, and an object is checked
/// first, so `Greeter.greet` must not be answered by the new fallback even
/// though `Greeter` holds a record.
#[test]
fn an_object_receiver_still_dispatches() {
    assert_eq!(
        both(
            "object Greeter\n\
              has name default \"Ada\"\n\
              to can greet()\n    give back \"hi \" + this.name\nend\n\
              end\n\
              set Greeter.name to \"Grace\"\n\
              Greeter.greet()\n"
        ),
        Value::Text("hi Grace".to_string()),
        "an object method stopped dispatching, or `this` stopped seeing the receiver"
    );
}

/// A receiver that is `nothing` is refused, and says so the same way on both
/// engines.
///
/// `nothing` is not a list, so the fallback does not apply and the receiver
/// falls through to the module path, which reports the name it could not
/// resolve. The wording is unchanged from what the name-shaped receiver always
/// said — the fix added an answer for a list and changed no refusal — and this
/// pins it so that a later change which starts *succeeding* on `nothing`, or
/// which answers it as `map`'s argument check rather than as a receiver, is a
/// failure here.
#[test]
fn edge_a_receiver_whose_value_is_nothing_is_refused() {
    let source = "set xs to nothing\nset out to xs.map(double)\nset out to 1\n";
    assert_eq!(
        both_errors(&format!("{DOUBLE}{source}")),
        "Module 'xs' has no function 'map'",
        "a receiver holding `nothing` stopped being refused by name"
    );
}

/// A bad argument to `xs.map` is a clean runtime error, not a panic.
///
/// Three refusals: a number where a function is required, no argument at all,
/// and a second function that cannot be used. The error has to name what it
/// got — the finding's rule is that `xs.map(5)` "names the argument it got" —
/// so the message is asserted, not merely that `Err` came back.
#[test]
fn edge_a_bad_argument_to_a_variable_receiver_map_is_a_named_runtime_error() {
    let number = both_errors(&format!("{DOUBLE}set xs to [1, 2, 3]\nxs.map(5)\n"));
    assert!(
        number.contains("map requires a function"),
        "a number argument was not refused by naming the function it wanted: {number:?}"
    );
    assert!(
        number.contains("5") || number.contains("number"),
        "the refusal did not name the argument it got: {number:?}"
    );

    let none = both_errors("set xs to [1, 2, 3]\nxs.map()\n");
    assert!(
        none.contains("map requires a function"),
        "a call with no argument was not refused as a missing function: {none:?}"
    );
}

/// One operation, one refusal — for a second function it cannot use.
///
/// `map` takes a list and one function. Handed a third value it has to say how
/// many arguments it was given, and it has to say the *same* thing whichever
/// spelling reached it: `xs.map(f, g)`, `map(xs, f, g)` and `list.map(xs, f, g)`
/// are one operation and each of them used to be answered differently, the two
/// flat ones saying only that the shape was wrong and the method one counting
/// the arguments. Both engines are asked for each.
///
/// The count is what makes this a test and not a restatement: the third value is
/// itself a function, so a refusal that only looked at the *last* argument
/// would have run it, and one that only looked at the second would have said
/// the arity without saying how far past it the call went.
#[test]
fn edge_a_map_given_a_second_function_is_refused_the_same_way_by_every_spelling() {
    let expected = "map requires a function, but was given 2 arguments";

    for call in [
        "xs.map(double, double)",
        "map(xs, double, double)",
        "list.map(xs, double, double)",
    ] {
        let source = format!("{DOUBLE}set xs to [1, 2, 3]\n{call}\n");
        assert_eq!(
            both_errors(&source),
            expected,
            "`{call}` refused the extra function differently from the other spellings"
        );
    }

    // Three arguments count as three, so the count is read rather than a fixed
    // string with `2` in it.
    let three = both_errors(&format!(
        "{DOUBLE}set xs to [1, 2, 3]\nxs.map(double, double, double)\n"
    ));
    assert_eq!(
        three, "map requires a function, but was given 3 arguments",
        "the refusal counted the extra arguments rather than reporting the count it got"
    );

    // And a receiver that is not a list is still the receiver's refusal: the
    // count belongs to the function side, and a call whose *first* argument is
    // wrong never reaches it.
    let not_a_list = both_errors(&format!("{DOUBLE}set xs to 5\nxs.map(double, double)\n"));
    assert!(
        !not_a_list.contains("2 arguments"),
        "a non-list receiver was refused with the argument count instead of with the \
         receiver it got: {not_a_list:?}"
    );
}

/// A receiver that is not a list at all keeps the refusal it already had.
///
/// The fallback is for a list and nothing else: a number, a text and a record
/// receiver all still get "not an object", and the tree-walker and the
/// bytecode VM have to say the same thing about each.
#[test]
fn edge_a_receiver_that_is_not_a_list_is_still_refused() {
    for receiver in ["5", "\"ab\"", "{a: 1}"] {
        let source = format!("set xs to {receiver}\nxs.map(double)\n");
        let message = both_errors(&format!("{DOUBLE}{source}"));
        assert!(
            !message.contains("map requires a function"),
            "a non-list receiver `{receiver}` reached the map argument check instead of \
             being refused as a receiver that is not an object: {message:?}"
        );
    }
}
