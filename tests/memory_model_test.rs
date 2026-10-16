//! Tests that pin the memory model documented in [`docs/MEMORY.md`].
//!
//! The phase that added this file implemented nothing: the model is what the
//! code already does. These tests therefore exist so the *documentation*
//! cannot quietly become untrue. Each one states a claim `MEMORY.md` makes and
//! checks it against the code, and the citation test at the bottom checks that
//! the line numbers `MEMORY.md` points at still say what it says they say.
//!
//! [`docs/MEMORY.md`]: ../docs/MEMORY.md

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use redblue::Value;

/// The source file a citation in `MEMORY.md` names, read as a vector of lines.
fn source_lines(relative: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} should be readable: {}", path.display(), error));
    text.lines().map(str::to_owned).collect()
}

/// The one-based line `n` of `relative`, or a panic naming the file.
fn source_line(relative: &str, n: usize) -> String {
    let lines = source_lines(relative);
    assert!(
        n >= 1 && n <= lines.len(),
        "{} has {} lines, so line {} cannot be cited",
        relative,
        lines.len(),
        n
    );
    lines[n - 1].clone()
}

fn readme_text() -> String {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("README.md"))
        .expect("README.md should be readable")
}

fn memory_text() -> String {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/MEMORY.md"))
        .expect("docs/MEMORY.md should be readable")
}

/// Lexes and parses `source`.
#[track_caller]
fn parse(source: &str) -> redblue::parser::Program {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    redblue::parser::parse(tokens).expect("source should parse")
}

/// Runs `source` on a stack sized for the call depth and returns the value of
/// its last statement.
#[track_caller]
fn eval(source: &str) -> Value {
    let (_vm, result) = redblue::run_isolated(&parse(source));
    result.unwrap_or_else(|error| panic!("source should have run, failed with {:?}", error))
}

/// Runs `source` and returns the error it produced.
#[track_caller]
fn eval_err(source: &str) -> redblue::Error {
    let (_vm, result) = redblue::run_isolated(&parse(source));
    result.expect_err("source should have failed")
}

/// `MEMORY.md`: "values are immutable once constructed and shared".
///
/// A list, a record and an object all hand out the *same* allocation, so
/// cloning a `Value` raises the handle count rather than copying the payload.
/// `MEMORY.md` is claiming the representation is `Arc`, and this is what that
/// claim looks like from outside.
#[test]
fn list_record_and_object_storage_is_shared_not_copied() {
    let list = Value::list(vec![Value::Number(1.0), Value::Number(2.0)]);
    let Value::List(list_storage) = list.clone() else {
        panic!("Value::list should build a List, got {:?}", list)
    };
    assert_eq!(
        Arc::strong_count(&list_storage),
        2,
        "a cloned list should share one allocation, not make a second"
    );
    assert_eq!(
        list_storage.as_ref(),
        &[Value::Number(1.0), Value::Number(2.0)],
        "the shared payload should be the list that was built"
    );

    let record = Value::record(Default::default());
    let Value::Record(record_storage) = record.clone() else {
        panic!("Value::record should build a Record, got {:?}", record)
    };
    assert_eq!(
        Arc::strong_count(&record_storage),
        2,
        "a cloned record should share one allocation"
    );

    let object = Value::object("Node".to_string(), Default::default());
    let Value::Object(name, object_storage) = object.clone() else {
        panic!("Value::object should build an Object, got {:?}", object)
    };
    assert_eq!(name, "Node", "the type name is beside the shared fields");
    assert_eq!(
        Arc::strong_count(&object_storage),
        2,
        "a cloned object should share one allocation"
    );
}

/// `MEMORY.md`: "reading a variable copies rather than aliases".
///
/// This is the observable consequence, and it is the rule a program is written
/// against: a list bound to one name and grown through another name is not a
/// mutation of the first, so the two never become aliases.
#[test]
fn reading_a_variable_copies_rather_than_aliases() {
    let source = concat!(
        "set first to [1, 2]\n",
        "set second to first\n",
        "set second to push(second, 3)\n",
        "length(first) - length(second)\n",
    );

    assert_eq!(
        eval(source),
        Value::Number(-1.0),
        "growing `second` must not be visible through `first`: they are two \
         bindings of one shared allocation, and the write copies before it writes"
    );
}

/// `MEMORY.md`: the same rule, checked on the storage itself.
///
/// Two names hold two handles to one allocation, so the second name's append
/// has to copy before it writes — that is `Arc::make_mut` doing its job, and it
/// is why an append cannot be seen through the name the list was made with.
#[test]
fn edge_writing_through_one_name_copies_so_the_other_name_cannot_see_it() {
    let mut list = Value::list(vec![Value::Number(1.0)]);
    let Value::List(storage) = list.clone() else {
        panic!("Value::list should build a List")
    };
    assert_eq!(
        Arc::strong_count(&storage),
        2,
        "both names should start on the same allocation"
    );

    let before = storage.as_ref().len();
    assert!(
        list.append(Value::Number(2.0)),
        "appending to a list should report the growth"
    );
    let Value::List(after_storage) = list.clone() else {
        panic!("the appended value is still a List")
    };

    assert_eq!(
        storage.as_ref().len(),
        before,
        "the shared allocation the other name holds must not have grown"
    );
    assert_eq!(
        after_storage.as_ref().len(),
        before + 1,
        "the appended-to name should see its own element"
    );
    assert_eq!(
        before + 1,
        2,
        "growing a shared list copies, so exactly one element was added"
    );
}

/// A node whose back-reference slot is filled in *after* the handle to it
/// exists, which is what it takes to make an `Arc` graph cyclic.
struct Node {
    payload: u32,
    next: Mutex<Option<Arc<Node>>>,
}

/// Builds a two-node `Arc` cycle and returns the only observer a cycle
/// collector would have to consult: a `Weak` back-reference.
///
/// The handle here is a plain `Arc`, which is exactly what Redblue's `Shared`
/// is (`pub type Shared<T> = std::sync::Arc<T>;`, `src/value.rs:276`). Redblue's
/// own containers are filled *before* the handle to them is made, which is why
/// no Redblue program can reach this shape — see the test below.
fn build_cycle() -> Weak<Node> {
    let head = Arc::new(Node {
        payload: 7,
        next: Mutex::new(None),
    });
    let tail = Arc::new(Node {
        payload: 9,
        next: Mutex::new(None),
    });
    *head.next.lock().expect("the head is not poisoned") = Some(tail.clone());
    *tail.next.lock().expect("the tail is not poisoned") = Some(head.clone());

    let weak = Arc::downgrade(&head);
    drop(head);
    drop(tail);
    weak
}

/// `MEMORY.md`: "a reference cycle is never collected and leaks".
///
/// Nothing in `src/` creates a `Weak`, so if this weak can still be upgraded
/// after the last external handle is dropped, the cycle is still holding its
/// own payload and will do so forever.
#[test]
fn edge_a_reference_cycle_is_never_reclaimed() {
    let weak = build_cycle();
    assert_eq!(
        weak.strong_count(),
        1,
        "after both external handles are dropped the head is held only by the \
         cycle's own back-edge"
    );

    let head = weak
        .upgrade()
        .expect("an `Arc` cycle with no `Weak` back-reference is never freed");
    assert_eq!(
        head.payload, 7,
        "the payload outlives every external handle"
    );

    let tail = {
        let held = head.next.lock().expect("the head is not poisoned");
        Arc::clone(
            held.as_ref()
                .expect("the head should still point at the tail"),
        )
    };
    assert_eq!(tail.payload, 9, "the other half of the cycle survived too");

    let back = {
        let held = tail.next.lock().expect("the tail is not poisoned");
        Arc::clone(
            held.as_ref()
                .expect("the cycle's back-edge should still be set"),
        )
    };
    assert!(
        Arc::ptr_eq(&back, &head),
        "the cycle should still close: head -> tail -> head, with nothing \
         outside it holding either node"
    );
}

/// `MEMORY.md`: "no Redblue program can build a cycle today".
///
/// The nearest a program gets is storing a record through its own field. The
/// write goes through `Arc::make_mut` (src/interpreter.rs:1626), and because
/// the value being stored is itself a live handle the storage is copied before
/// it is written to — so the copy `node.self` holds is the record *as it was
/// before* the write, not the record after it. A cycle would need the storage
/// to contain a handle to itself, and here it does not.
#[test]
fn a_redblue_program_cannot_build_a_reference_cycle() {
    let source = concat!(
        "set node to { name: \"node\", self: nothing }\n",
        "set node.self to node\n",
        "node.self.self\n",
    );

    assert_eq!(
        eval(source),
        Value::Nothing,
        "`node.self` must hold the pre-write record, whose `self` was nothing: \
         an alias or a cycle would make this field the record itself"
    );
}

/// `MEMORY.md`: "there is no cycle collection".
///
/// Asserted against the tree rather than against the document, so a collector
/// cannot be added without this failing. `Rc` is allowed outside the value
/// representation — the parser's AST and the REPL's editor sessions use it —
/// so the check is scoped to `src/value.rs`, and the absolute `Weak`/`Gc` check
/// covers the whole crate.
#[test]
fn no_cycle_collector_exists_anywhere_in_the_crate() {
    let mut files = Vec::new();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    collect_rust_files(&src, &mut files);
    assert!(!files.is_empty(), "src/ should contain Rust source files");

    for file in &files {
        let relative = file
            .strip_prefix(env!("CARGO_MANIFEST_DIR"))
            .unwrap_or(file)
            .display()
            .to_string();
        let text = std::fs::read_to_string(file).expect("source file should be readable");
        for forbidden in ["Weak", "WeakRef", "Gc", "gc::", "weak_count"] {
            assert!(
                !text.contains(forbidden),
                "{} mentions `{}`: there is no cycle collector, so nothing \
                 in the crate may reach for one",
                relative,
                forbidden
            );
        }
    }

    let value_rs = std::fs::read_to_string(src.join("value.rs")).expect("value.rs readable");
    assert!(
        !value_rs.contains("std::rc::Rc") && !value_rs.contains("Rc::"),
        "src/value.rs must not hold a `Rc`: a value that outlives its scope \
         cannot be dropped when the count reaches zero"
    );
}

fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("{} should be readable: {}", dir.display(), error));
    for entry in entries {
        let entry = entry.expect("directory entry should be readable");
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

/// `MEMORY.md`: deep recursion raises a *catchable* `Error::Limit`.
///
/// The claim has two halves and both are checked here: the error is a `Limit`
/// rather than an abort, and a program can catch it and carry on.
#[test]
fn deep_recursion_raises_a_catchable_limit_not_a_stack_overflow() {
    let err = eval_err("to boom(n)\n    boom(n + 1)\nend\n\nboom(0)\n");

    assert!(
        matches!(err, redblue::Error::Limit(_, _)),
        "unbounded recursion should raise Error::Limit, got {:?}",
        err
    );
    assert!(
        err.is_resource_limit(),
        "the error should report itself as a limit, got {:?}",
        err
    );
    assert!(
        !err.to_string().contains("stack overflow"),
        "the native stack was exhausted instead of the counter stopping it: {:?}",
        err
    );

    let caught = concat!(
        "to boom(n)\n",
        "    boom(n + 1)\n",
        "end\n",
        "set reached to no\n",
        "try\n",
        "    boom(0)\n",
        "catch error\n",
        "    set reached to yes\n",
        "end\n",
        "expect reached to be yes\n",
    );
    let (_vm, result) = redblue::run_isolated(&parse(caught));
    assert!(
        result.is_ok(),
        "a depth error must be catchable by `try ... catch error`, got {:?}",
        result.err()
    );
}

/// `MEMORY.md`: the limit is `REDBLUE_MAX_CALL_DEPTH`-overridable.
#[test]
fn the_call_depth_limit_has_the_documented_environment_override() {
    assert_eq!(
        redblue::MAX_CALL_DEPTH_ENV,
        "REDBLUE_MAX_CALL_DEPTH",
        "MEMORY.md names this variable"
    );
    assert!(
        !redblue::MAX_CALL_DEPTH.to_string().is_empty(),
        "the default limit must be a usable depth, got {}",
        redblue::MAX_CALL_DEPTH
    );

    let message =
        eval_err("to ping(n)\n    pong(n + 1)\nend\nto pong(n)\n    ping(n + 1)\nend\n\nping(0)\n")
            .to_string();
    assert!(
        message.contains(&redblue::MAX_CALL_DEPTH.to_string()),
        "the error should name the default limit {}, got {:?}",
        redblue::MAX_CALL_DEPTH,
        message
    );
}

/// Every `file:line` `MEMORY.md` cites must still say what the document claims.
///
/// This is the test that makes the rest of the file's citations worth anything.
#[test]
fn memory_md_citations_still_match_the_code() {
    let memory = memory_text();

    let shared = source_line("src/value.rs", 276);
    assert_eq!(
        shared.trim(),
        "pub type Shared<T> = std::sync::Arc<T>;",
        "src/value.rs:276 is the `Shared` alias MEMORY.md cites"
    );
    assert!(
        memory.contains("src/value.rs:276"),
        "MEMORY.md must cite src/value.rs:276 for the `Shared` alias"
    );

    for (line, variant) in [(253, "List"), (254, "Record"), (255, "Object")] {
        let text = source_line("src/value.rs", line);
        assert!(
            text.contains(&format!("{}(", variant)) && text.contains("Shared<"),
            "src/value.rs:{} should be the `Shared` {} variant, got {:?}",
            line,
            variant,
            text
        );
    }
    assert!(
        memory.contains("src/value.rs:253"),
        "MEMORY.md must cite the variant lines for the Arc-shared values"
    );

    let clone_rule = source_lines("src/value.rs")[87..90].join(" ");
    assert!(
        clone_rule.contains("reading any variable clones the value bound to"),
        "src/value.rs:88-90 states the copy-on-read rule, got {:?}",
        clone_rule
    );
    assert!(
        memory.contains("src/value.rs:88"),
        "MEMORY.md must cite src/value.rs:88 for the copy-on-read rule"
    );

    let vm_claim = source_lines("src/bytecode/vm.rs")[17..21].join(" ");
    assert!(
        vm_claim.contains("no path from a `.rbc` to a native stack overflow"),
        "src/bytecode/vm.rs:18-21 states the no-stack-overflow claim, got {:?}",
        vm_claim
    );
    assert!(
        memory.contains("src/bytecode/vm.rs"),
        "MEMORY.md must cite src/bytecode/vm.rs for the recursion bound"
    );
}

/// `MEMORY.md` has to exist, and `README.md` has to point at it — otherwise the
/// model is documented nowhere a user would look, which is the whole finding.
#[test]
fn memory_md_is_linked_from_the_readme() {
    let memory = memory_text();
    assert!(
        memory.contains("src/value.rs:276"),
        "MEMORY.md must state the value semantics and cite src/value.rs:276"
    );
    assert!(
        memory.to_lowercase().contains("cycle"),
        "MEMORY.md must name the cycle leak as a known limitation"
    );

    assert!(
        readme_text().contains("MEMORY.md"),
        "README.md must link MEMORY.md"
    );
}
