//! `append` grows the list a name is bound to, without copying it.
//!
//! `push` (`tests/list_push_test.rs`) returns a new list and leaves its argument
//! alone. That is the right contract for a value language and the wrong one for
//! a loop that builds a list: `set xs to push(xs, v)` copies `xs` on every step,
//! so building a list of n is n^2 copies of the whole list so far. It is the one
//! shape `bootstrap/compiler.rb` uses for its token list, its instruction list
//! and its constant pool, which is why a self-compilation did not finish
//! (`phases/phase-022/FINDINGS.md` §1 measures it at n^1.94 and extrapolates to
//! roughly 24 minutes).
//!
//! `append("name", value)` is the same growth without the copy. The name is
//! looked up the way every other name is, the list is grown in place, and the
//! result is still a Redblue value: a list that two names share is copied before
//! it is grown, because growing it in place would be visible through the other
//! name. So this file pins both halves — that `append` is cheap, and that
//! "cheap" never became "aliased".
//!
//! Every case runs on both engines, for the reason `list_push_test.rs` gives:
//! `append` is a builtin, so the tree-walker and the bytecode VM both have to
//! implement it or the two disagree about a list.

use redblue::bytecode::vm::BytecodeVm;
use redblue::{Error, Value};

/// Runs `source` through lexer → parser → VM and returns the value of its last
/// statement.
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

/// Runs `source` on both engines and returns what they both said.
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

/// Runs `source` on both engines and asserts that both refuse it, returning the
/// two messages so a caller can compare them.
#[track_caller]
fn both_refuse(source: &str) -> (String, String) {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    let ast = redblue::parser::parse(tokens).expect("source should parse");
    let walked = match redblue::Vm::new().run(&ast) {
        Ok(value) => panic!("the tree-walker accepted `{source}` and returned {value}"),
        Err(error) => error.to_string(),
    };
    let compiled =
        match redblue::compile_source(source).and_then(|chunk| BytecodeVm::new().run(&chunk)) {
            Ok(value) => panic!("the bytecode VM accepted `{source}` and returned {value}"),
            Err(error) => error.to_string(),
        };
    (walked, compiled)
}

/// `append` grows the list the name is bound to, and the growth is visible
/// through that name.
///
/// The empty list and a singleton are the two boundaries worth pinning: a
/// binding that starts as `[]` has nothing to copy, and a one-element list is
/// the smallest one where "appended" and "not appended" are different answers.
#[test]
fn edge_append_grows_the_list_its_name_is_bound_to() {
    assert_eq!(
        both("set xs to []\nappend(\"xs\", 1)\nappend(\"xs\", 2)\nxs\n"),
        Value::list(vec![Value::Number(1.0), Value::Number(2.0)]),
        "append did not grow the list its name is bound to, or did not grow it in order"
    );

    // Both ends of the result, so a list that grew at the front rather than the
    // back is caught as well as one that grew by nothing.
    assert_eq!(
        both(
            "set xs to []\nappend(\"xs\", \"first\")\nappend(\"xs\", \"last\")\n\
             [length(xs), xs[0], xs[1]]\n"
        ),
        Value::list(vec![
            Value::Number(2.0),
            Value::Text("first".to_string()),
            Value::Text("last".to_string()),
        ]),
        "append put the values somewhere other than the ends of the list"
    );
}

/// A list two names share is copied before it is grown, so the name that did not
/// ask for the append sees nothing.
///
/// This is the half of the contract that a shared-storage implementation gets
/// wrong. `a` and `b` start as one value; `append` through `a` must not be
/// visible through `b`, through a record field holding the same list, or through
/// an element of a list holding it.
#[test]
fn edge_append_to_a_shared_list_leaves_the_other_names_alone() {
    let source = "set a to [1]\n\
                  set b to a\n\
                  set holder to {items: a}\n\
                  set outer to push([], a)\n\
                  append(\"a\", 2)\n\
                  [a, b, holder.items, outer[0]]\n";
    assert_eq!(
        both(source),
        Value::list(vec![
            // The name that appended: grown.
            Value::list(vec![Value::Number(1.0), Value::Number(2.0)]),
            // Every other route to the same list: unchanged.
            Value::list(vec![Value::Number(1.0)]),
            Value::list(vec![Value::Number(1.0)]),
            Value::list(vec![Value::Number(1.0)]),
        ]),
        "append wrote through the other bindings of the list, or did not grow its own"
    );

    // Appending through the *other* name grows that one, and still leaves the
    // first alone: the copy is per-append, not a permanent split.
    assert_eq!(
        both("set a to [1]\nset b to a\nappend(\"b\", 2)\n[a, b]\n"),
        Value::list(vec![
            Value::list(vec![Value::Number(1.0)]),
            Value::list(vec![Value::Number(1.0), Value::Number(2.0)]),
        ]),
        "appending through one name of a shared list changed the other name"
    );
}

/// The value `append` stores is the value it was given, not a live reference to
/// the name it was read from.
#[test]
fn edge_append_stores_the_value_it_was_given() {
    assert_eq!(
        both(
            "set tok to {kind: \"number\"}\n\
             set xs to []\n\
             append(\"xs\", tok)\n\
             set tok.kind to \"text\"\n\
             [xs[0].kind, tok.kind]\n"
        ),
        Value::list(vec![
            Value::Text("number".to_string()),
            Value::Text("text".to_string()),
        ]),
        "the value appended changed with the name it was read from, or the change did not take"
    );
}

/// `append` refuses a name it cannot grow, on both engines, with a message that
/// says what was wrong.
///
/// Four refusals: a name that is not bound at all, a number, text, and an
/// unbound field of a record. The last two matter because `type_of` says
/// `nothing` for an unbound name, so a refusal that only checks the type would
/// report a type error for a name that was never there.
#[test]
fn edge_append_refuses_a_name_it_cannot_grow() {
    let unbound = both_refuse("set other to [1]\nappend(\"missing\", 2)\n");
    assert!(
        unbound.0.contains("missing"),
        "the refusal for an unbound name does not name it: {}",
        unbound.0
    );
    assert_eq!(
        unbound.0, unbound.1,
        "the two engines refused an unbound name differently"
    );

    let wrong_type = both_refuse("set n to 7\nappend(\"n\", 8)\n");
    assert!(
        wrong_type.0.contains("number"),
        "the refusal for a number does not say what it was handed: {}",
        wrong_type.0
    );
    assert_eq!(
        wrong_type.0, wrong_type.1,
        "the two engines refused a number differently"
    );

    let text = both_refuse("set t to \"hello\"\nappend(\"t\", \"!\")\n");
    assert_eq!(
        text.0, text.1,
        "the two engines refused text differently: {} / {}",
        text.0, text.1
    );

    let field = both_refuse("set r to {}\nappend(\"r.items\", 1)\n");
    assert!(
        field.0.contains("items"),
        "the refusal for a missing field does not name the field: {}",
        field.0
    );
    assert_eq!(
        field.0, field.1,
        "the two engines refused a missing field differently"
    );
}

/// Twenty thousand appends is the shape that makes the difference.
///
/// On a path that copies, each append copies the whole list so far: 20 000
/// appends is 200 million record copies, which is minutes. On a path that grows
/// in place it is 20 000 steps, well under a second. The bound below is the
/// middle of a factor-of-more-than-two-hundred gap, not a measurement of this
/// machine's speed — a run on shared CI hardware that is ten times slower still
/// passes, and a path that copies cannot pass at any speed.
///
/// The count is asserted as well as the time, so a test that fails here says
/// *which* of the two went wrong.
#[test]
fn edge_append_is_a_step_and_not_a_copy_of_the_list_so_far() {
    const N: usize = 20_000;
    let source = format!(
        "set xs to []\n\
         set i to 0\n\
         while i < {N}\n\
         append(\"xs\", i)\n\
         set i to i + 1\n\
         end\n\
         [length(xs), xs[0], xs[19999], xs[12345]]\n"
    );

    let started = std::time::Instant::now();
    let value = both(&source);
    let elapsed = started.elapsed();

    assert_eq!(
        value,
        Value::list(vec![
            Value::Number(N as f64),
            Value::Number(0.0),
            Value::Number(19999.0),
            Value::Number(12345.0),
        ]),
        "the appended list does not hold every value, in order"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(30),
        "{N} appends took {elapsed:?}, which is the cost of copying the list each time \
         rather than of growing it"
    );
}

/// The four gates run `cargo test --all-targets`, and this file is part of it:
/// an `Error` import that is unused would be a warning, not a failure, so the
/// refusals above assert on the message rather than on the variant — but the
/// variant is what a caller of the library matches on, so it is checked here.
#[test]
fn edge_append_refusals_are_runtime_errors() {
    let tokens = redblue::lexer::Lexer::tokenize("append(\"nope\", 1)\n").expect("lexes");
    let ast = redblue::parser::parse(tokens).expect("parses");
    let error = redblue::Vm::new()
        .run(&ast)
        .expect_err("an unbound name is refused");
    assert!(
        matches!(error, Error::Runtime(_, _)),
        "an unbound name gave {error:?}, which is not a Runtime error"
    );
}
