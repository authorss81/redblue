//! `push` returns a new list; it does not write through the one it was given.
//!
//! `push` is the only way a Redblue program grows a list, so every list a
//! program builds goes through it — including the two hot loops of
//! `bootstrap/compiler.rb`. Its contract is therefore load-bearing twice over:
//! the value semantics are what every other program relies on, and the cost of
//! the call is what makes self-compilation slow (`phases/phase-021/FINDINGS.md`
//! §3b measures `push` of a record at n^2, and §3d says why the obvious fix to
//! that needs these tests standing first).
//!
//! Both are pinned here. A fix for the quadratic that shares a list's storage
//! between two names would keep every example in this file passing while
//! silently changing what a Redblue program means, so the semantics below are
//! asserted on their own and not as a side effect of anything about speed.
//!
//! Every case runs on both engines: `push` is a runtime builtin, so a change to
//! it has to hold for the tree-walker and for the bytecode VM or the two
//! disagree on a list.

use redblue::bytecode::vm::BytecodeVm;
use redblue::Value;

/// Runs `source` through lexer → parser → VM and returns the value of its last
/// statement.
///
/// Only a bare expression statement carries a value out: `set x to …` and
/// `say …` both evaluate to `nothing`, so a source ending in one of them would
/// make every assertion below compare `nothing` against `nothing` and pass
/// without testing anything.
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

/// A list bound to two names is one value, and appending through one name must
/// not be visible through the other.
///
/// This is the whole of the value semantics in one program: `a` and `b` are
/// bound to the same list, and only the result of `push` may have grown.
#[test]
fn edge_push_does_not_alias_its_argument() {
    // Two names, one list. `b` is read after `a` has been appended to.
    assert_eq!(
        both("set a to [1, 2]\nset b to a\nset a to push(a, 3)\nb\n"),
        Value::List(vec![Value::Number(1.0), Value::Number(2.0)]),
        "a push through one name was visible through the name the list was \
         already bound to"
    );

    // And the appending name did grow, so the assertion above is not passing
    // because `push` appended nothing.
    assert_eq!(
        both("set a to [1, 2]\nset b to a\nset a to push(a, 3)\na\n"),
        Value::List(vec![
            Value::Number(1.0),
            Value::Number(2.0),
            Value::Number(3.0)
        ]),
        "push did not return the appended list"
    );
}

/// A list shared through three routes is copied once per `push`, not written
/// through — and the copy holds the new value exactly once.
///
/// `a`, `b` and a record's field all reach the same list. A `push` that wrote
/// through the shared storage would put the new value in all of them at once,
/// which is the failure mode a copy-on-write fix is most likely to have.
#[test]
fn edge_push_of_a_shared_list_copies_rather_than_writing_through() {
    // Three bindings to one list, then a push through the third route.
    let source = "set a to [1]\n\
                  set b to a\n\
                  set holder to {items: a}\n\
                  set a to push(holder.items, 2)\n\
                  [a, b, holder.items]\n";
    assert_eq!(
        both(source),
        Value::List(vec![
            // The push's own result: grown.
            Value::List(vec![Value::Number(1.0), Value::Number(2.0)]),
            // The names that still hold the original: unchanged.
            Value::List(vec![Value::Number(1.0)]),
            Value::List(vec![Value::Number(1.0)]),
        ]),
        "a push into a list held by three bindings wrote through the other two, \
         or its own result did not hold the new value exactly once"
    );

    // Chained pushes: each step takes the previous result, so the value must
    // accumulate once per step and in order.
    assert_eq!(
        both("set a to []\nset a to push(a, 1)\nset a to push(a, 2)\nset a to push(a, 3)\na\n"),
        Value::List(vec![
            Value::Number(1.0),
            Value::Number(2.0),
            Value::Number(3.0)
        ]),
        "chained pushes did not accumulate in order, or one of them appended twice"
    );
}

/// The value `push` appends is stored as it was given, and a later change to the
/// name it was read from does not reach into the list.
///
/// This is the record case `FINDINGS.md` §3b measured: the compiler's tokens and
/// its AST nodes are records, which is why `push` of a record is the slow shape
/// rather than a marginal one. `set tok.kind to …` rebinds the field on the
/// value `tok` holds; a `push` that stored a reference would carry that change
/// into the list with it.
#[test]
fn edge_push_stores_the_value_it_was_given_without_aliasing_it() {
    assert_eq!(
        both(
            "set tok to {kind: \"number\", text: \"12345\"}\n\
              set tokens to push([], tok)\n\
              set tok.kind to \"text\"\n\
              [length(tokens), tokens[0].kind, tok.kind]\n"
        ),
        Value::List(vec![
            Value::Number(1.0),
            // Inside the list: the record as it was when it was pushed.
            Value::Text("number".to_string()),
            // Through the name it was read from: the later field binding.
            Value::Text("text".to_string()),
        ]),
        "the record pushed into the list changed with the name it was read from, \
         or the change did not take effect at all"
    );

    // The same shape with a list on both sides: the list pushed in is not the
    // list it was pushed onto.
    assert_eq!(
        both("set inner to [2, 3]\nset outer to push([1], inner)\nset inner to push(inner, 4)\nouter\n"),
        Value::List(vec![Value::Number(1.0), Value::List(vec![Value::Number(2.0), Value::Number(3.0)])]),
        "a list pushed into another list kept aliasing the one it was pushed from"
    );
}
