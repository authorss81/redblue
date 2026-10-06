//! The object model: `has` fields, `to can` methods, `extends`, and the order
//! that resolves them.
//!
//! The parser has carried `object Name extends Parent` since the grammar was
//! written, but the runtime threw the parent and the body away and bound an
//! empty record, so a program that declared fields could not be written at all
//! (`has` and `to can` were parse errors) and a cycle was accepted silently.
//!
//! The model these tests pin, in full:
//!
//! - A declaration binds its name to a *record* of its resolved fields. A plain
//!   record, which is why `type_of(Person)` is `"record"` — the declaration is
//!   the type and its prototype instance at once.
//! - Lookup order is nearest declaration first: the child's own `has` fields
//!   and `to can` methods, then the parent's, then the grandparent's. The first
//!   declaration of a name wins and the further ones are not copied in.
//! - A parent must be declared before its child, so the parent chain is
//!   acyclic by construction; `object A extends A` is reported as the cycle it
//!   is, and re-declaring a name is refused rather than rewired.
//! - `receiver.method(args)` is a method call when the receiver names a
//!   declared object, with `this` bound to the receiver for the call. For a
//!   receiver that names no object it is the module function `receiver_method`,
//!   which is what `files.read` and `json.parse` are.

use redblue::Error;
use redblue::Value;

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
    result.unwrap_or_else(|error| panic!("`{}` should have run, failed with {:?}", source, error))
}

/// Runs `source` and returns the pipeline error it produced.
#[track_caller]
fn eval_err(source: &str) -> Error {
    let (_vm, result) = redblue::run_isolated(&parse(source));
    result.expect_err("`source` should have failed")
}

/// Lexes and parses `source`, returning the parser error it produced.
#[track_caller]
fn parse_err(source: &str) -> Error {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    redblue::parser::parse(tokens).expect_err("`source` should not have parsed")
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

/// The message of the runtime error `source` produced.
#[track_caller]
fn runtime_message(source: &str) -> String {
    match eval_err(source) {
        Error::Runtime(message, _) => message,
        other => panic!(
            "`{}` should fail with a Runtime error, got {:?}",
            source, other
        ),
    }
}

/// A child inherits its parent's fields, and a field the child declares itself
/// shadows the parent's field of the same name.
#[test]
fn object_inherits_parent_fields_and_shadows_them() {
    eval(
        "object Base\n    has tag\n    has size\nend\n\
         object Derived extends Base\n    has tag\nend\n\
         set Base.tag to \"base\"\n\
         expect Derived.tag is nothing to be yes\n\
         expect Derived.size is nothing to be yes\n\
         expect Base.tag to be \"base\"\n",
    );
}

/// A field's default is the value the declaration record starts with, and a
/// child's own default is the one that survives shadowing.
#[test]
fn object_field_default_and_shadowed_default() {
    eval(
        "object Base\n    has colour default \"grey\"\nend\n\
         object Derived extends Base\n    has colour default \"blue\"\nend\n\
         expect Base.colour to be \"grey\"\n\
         expect Derived.colour to be \"blue\"\n",
    );
}

/// A method sees the receiver as `this`, and its arguments by name.
#[test]
fn object_method_dispatch_binds_this() {
    let result = eval(
        "object Person\n    has name\n    \
         to can greet(greeting)\n        give back greeting + \", \" + this.name\n    end\nend\n\
         set Person.name to \"Ada\"\n\
         Person.greet(\"hello\")\n",
    );
    assert_eq!(result, Value::Text("hello, Ada".to_string()));
}

/// The nearest declaration of a method name is the one that answers, and the
/// shadowed parent method is still reachable through the parent.
#[test]
fn object_child_method_overrides_parent_method() {
    eval(
        "object Person\n    \
         to can role()\n        give back \"person\"\n    end\n    \
         to can name()\n        give back \"person name\"\n    end\nend\n\
         object Employee extends Person\n    \
         to can role()\n        give back \"employee\"\n    end\nend\n\
         expect Employee.role() to be \"employee\"\n\
         expect Person.role() to be \"person\"\n\
         expect Employee.name() to be \"person name\"\n",
    );
}

/// A method and a field of the same name are separate namespaces: the field
/// wins on `Type.name`, and the method wins on `Type.name()`.
#[test]
fn object_field_and_method_of_the_same_name() {
    let result = eval(
        "object Row\n    has name default \"field\"\n    \
         to can name()\n        give back \"method\"\n    end\nend\n\
         expect Row.name to be \"field\"\n\
         Row.name()\n",
    );
    assert_eq!(result, Value::Text("method".to_string()));
}

/// A method writes to `this` and hands the object back, which is SPEC.md's
/// constructor. The write does not escape to the declaration on its own.
#[test]
fn object_method_writes_to_this_and_gives_it_back() {
    eval(
        "object Counter\n    \
         has total default 0\n    \
         to can bump()\n        set this.total to this.total + 1\n        give back this\n    end\nend\n\
         set first to Counter.bump()\n\
         expect first.total to be 1\n\
         expect Counter.total to be 0\n",
    );
}

/// A write to a declaration record is a write to that prototype, not to the
/// type: a child declared afterwards still starts from the declared defaults.
#[test]
fn object_writes_do_not_leak_into_a_later_child() {
    eval(
        "object Base\n    has tag default \"declared\"\nend\n\
         set Base.tag to \"written\"\n\
         object Child extends Base\n         end\n\
         expect Base.tag to be \"written\"\n\
         expect Child.tag to be \"declared\"\n",
    );
}

/// An object declared inside a function body is registered when the body runs,
/// and a nested declaration may extend it.
#[test]
fn object_nested_declaration_extends_its_own_scope() {
    let result = eval(
        "to build()\n    \
         object Inner\n        has value default 7\n    end\n    \
         object Outer extends Inner\n        has label default \"outer\"\n    end\n    \
         set total to Outer.value + Inner.value\nend\n\
         build()\n",
    );
    assert_eq!(result, Value::Nothing);

    eval(
        "to build()\n    \
         object Inner\n        has value default 7\n    end\n    \
         object Outer extends Inner\n        has label default \"outer\"\n    end\n    \
         expect Outer.value to be 7\n    \
         expect Outer.label to be \"outer\"\n\
         expect Inner.label is nothing to be yes\nend\n\
         build()\n",
    );
}

/// Three levels deep: the field and the method both come from the grandparent
/// when neither nearer declaration names them.
#[test]
fn object_grandparent_fields_and_methods_are_reachable() {
    eval(
        "object One\n    has a default 1\n    \
         to can who()\n        give back \"one\"\n    end\nend\n\
         object Two extends One\n    has b default 2\nend\n\
         object Three extends Two\n    has c default 3\nend\n\
         expect Three.a to be 1\n\
         expect Three.b to be 2\n\
         expect Three.c to be 3\n\
         expect Three.who() to be \"one\"\n",
    );
}

/// A declaration with no fields and no methods is still an empty record, which
/// is what the object tests in `tests/test_objects.rb` already pin.
#[test]
fn object_with_no_fields_and_no_methods_is_an_empty_record() {
    let result = eval("object Nothing_Here\nend\nNothing_Here\n");
    assert_eq!(
        result.to_string(),
        "{}",
        "an object with no fields should display as an empty record"
    );
}

/// One field is the smallest non-empty object, and it can be written and read.
#[test]
fn object_single_field_round_trips() {
    let result = eval(
        "object Box\n    has item\nend\n\
         set Box.item to \"crate\"\n\
         Box.item\n",
    );
    assert_eq!(result, Value::Text("crate".to_string()));
}

/// The same field declared twice is one field, and the later default is the one
/// that is kept.
#[test]
fn object_duplicate_field_declaration_keeps_the_last_default() {
    eval(
        "object Twice\n    has tag default \"first\"\n    has tag default \"second\"\nend\n\
         expect Twice.tag to be \"second\"\n",
    );
}

/// A field whose name is also a module name is an ordinary field: the module
/// call still reaches the stdlib.
#[test]
fn object_field_named_like_a_module_does_not_shadow_it() {
    eval(
        "object Holder\n    has json\nend\n\
         set Holder.json to \"not a module\"\n\
         set parsed to json.parse(\"{\\\"a\\\": 1}\")\n\
         expect parsed.a to be 1\n\
         expect Holder.json to be \"not a module\"\n",
    );
}

/// A declared object wins the `receiver.method` spelling over the module of the
/// same name, because the receiver is looked up first.
#[test]
fn object_named_like_a_module_takes_the_call() {
    let result = eval(
        "object json\n    \
         to can parse(text)\n        give back \"object parse\"\n    end\nend\n\
         json.parse(\"{}\")\n",
    );
    assert_eq!(result, Value::Text("object parse".to_string()));
}

/// Unicode in a field name, a default and a method body survives the round
/// trip: an emoji, CJK, an RTL mark and a combining mark.
#[test]
fn object_unicode_fields_defaults_and_methods() {
    let label = "h\u{e9}llo \u{1f389} \u{4e2d}\u{6587} \u{202b}e\u{301}";
    eval(&format!(
        "object Label\n    has text default \"{label}\"\n    \
         to can read()\n        give back this.text\n    end\nend\n\
         expect Label.text to be \"{label}\"\n\
         expect Label.read() to be \"{label}\"\n"
    ));
}

/// An empty default is a field, not a missing field.
#[test]
fn object_empty_text_default_is_a_field() {
    eval(
        "object Note\n    has body default \"\"\nend\n\
         expect Note.body to be \"\"\n\
         expect Note. missing is nothing to be yes\n",
    );
}

/// Fields keep the language's types: a field holding `nothing` is not a number,
/// so arithmetic on it is the ordinary type error rather than a silent zero.
#[test]
fn edge_object_field_of_nothing_is_not_zero() {
    assert_runtime_error(
        "object Counter\n    has total\nend\nset result to Counter.total + 1\n",
        "Cannot add non-numbers",
    );
}

/// A field holding a number is not a list, so `length` on it is a type error.
#[test]
fn edge_object_number_field_is_not_a_list() {
    assert_runtime_error(
        "object Counter\n    has total default 1\nend\nset result to length(Counter.total)\n",
        "length requires a list or text",
    );
}

/// A method the type does not have is an error, not a missing field: the
/// difference matters because a field reads as `nothing` and a method does not
/// exist.
#[test]
fn edge_object_method_that_does_not_exist_is_an_error() {
    assert_runtime_error(
        "object A\nend\nset result to A.missing()\n",
        "Object 'A' has no method 'missing'",
    );
}

/// A method on a field rather than on the object is refused instead of falling
/// back to a module function name that does not exist either.
#[test]
fn edge_method_call_on_a_field_is_an_error() {
    let message = runtime_message("object A\nend\nset result to A.field.m()\n");
    assert!(
        message.contains("which is not an object"),
        "expected a not-an-object error, got `{}`",
        message
    );
}

/// `this` is bound by a method call and by nothing else, so reaching for it in
/// free code is an error rather than an empty record.
#[test]
fn edge_this_outside_a_method_is_an_error() {
    match eval_err("set result to this.name\n") {
        Error::Runtime(message, _) => {
            assert_eq!(message, "Unknown variable 'this'", "wrong message")
        }
        other => panic!("expected a Runtime error, got {:?}", other),
    }
}

/// `object A extends A` is a one-object cycle in the parent chain, and it is
/// reported instead of walked.
#[test]
fn edge_self_extending_object_is_a_reported_cycle() {
    assert_runtime_error(
        "object A extends A\nend\n",
        "Object 'A' extends 'A', which is already in its own parent chain",
    );
}

/// A two-object cycle cannot be declared — `A` is already registered when `B`
/// is declared, and re-declaring it is refused — and saying so is a clean error
/// rather than a hang or an infinite loop.
#[test]
fn edge_mutually_extending_objects_are_refused_not_looped() {
    assert_runtime_error(
        "object A\nend\nobject B extends A\nend\nobject A extends B\nend\n",
        "Object 'A' is already declared",
    );
}

/// The parent chain is walked once, at declaration, so a long chain resolves
/// without recursing in Rust.
#[test]
fn edge_deep_parent_chain_resolves() {
    let mut source = String::new();
    for level in 0..200 {
        source.push_str(&format!(
            "object O{level}\n    has depth default {level}\n    to can level()\n        give back {level}\n    end\nend\n"
        ));
    }
    source.push_str(
        "object Deepest extends O198\n    has own default \"deep\"\nend\n\
         expect Deepest.depth to be 198\n\
         expect Deepest.own to be \"deep\"\n\
         expect Deepest.level() to be 198\n",
    );
    eval(&source);
}

/// Two types whose methods call each other recurse through the same call-depth
/// counter as any other call, so the limit is a `RuntimeError` and not a Rust
/// stack overflow.
#[test]
fn edge_mutually_recursive_methods_hit_the_call_depth_limit() {
    let message = runtime_message(
        "object A\n    to can ping(n)\n        give back B.pong(n + 1)\n    end\nend\n\
         object B\n    to can pong(n)\n        give back A.ping(n + 1)\n    end\nend\n\
         set result to A.ping(0)\n",
    );
    assert!(
        message.starts_with("Maximum call depth of"),
        "expected the call-depth limit, got `{}`",
        message
    );
}

/// `has` without a field name, and `to can` without a method name, are parse
/// errors rather than a declaration of something unnamed.
#[test]
fn edge_malformed_object_declarations_are_parse_errors() {
    match parse_err("object A\n    has\nend\n") {
        Error::Parser(message, span) => {
            assert_eq!(message, "Expected field name after 'has'", "wrong message");
            assert!(span.is_known(), "failed without a source span");
        }
        other => panic!("expected a Parser error, got {:?}", other),
    }

    match parse_err("object A\n    to can\n    end\nend\n") {
        Error::Parser(message, _) => assert_eq!(message, "Expected function name", "wrong message"),
        other => panic!("expected a Parser error, got {:?}", other),
    }

    // An unterminated declaration is the parser's ordinary unclosed-block
    // error, not an object that swallows the rest of the file.
    match parse_err("object A\n    has a\n") {
        Error::Parser(message, _) => assert!(
            message.contains("End"),
            "expected an error naming `End`, got `{}`",
            message
        ),
        other => panic!("expected a Parser error, got {:?}", other),
    }
}

/// An `object` written inside another `object`'s body is declared too, so a
/// body nests a second declaration rather than absorbing it.
#[test]
fn object_a_declaration_nested_in_a_body_declares_its_own_type() {
    eval(
        "object Outer\n    has o default 1\n    object Inner\n        has i default 2\n    end\n    \
         expect Inner.i to be 2\nend\n\
         expect Outer.o to be 1\n",
    );
}

/// The type is registered before the statements after its declarations run, so a
/// declaration nested in a body may name that body as its parent.
#[test]
fn object_a_nested_declaration_may_extend_the_body_it_is_written_in() {
    eval(
        "object Outer\n    has o default 1\n    object Inner extends Outer\n        has i default 2\n    end\n    \
         expect Inner.o to be 1\n    expect Inner.i to be 2\nend\n\
         expect Outer.o to be 1\n",
    );
}

/// Three levels: the innermost body inherits through a parent that is itself
/// still being declared two bodies up.
#[test]
fn object_a_nested_declaration_may_extend_two_levels_up() {
    eval(
        "object One\n    has n default 1\n    object Two\n        object Three extends One\n            has k default 3\n\
         end\n        expect Three.n to be 1\n    end\nend\n",
    );
}

/// A name an open body has already taken is refused rather than redeclared,
/// whichever body wrote it — the outer type is registered before the nested
/// declaration runs, so the inner one finds it there.
#[test]
fn edge_object_a_nested_declaration_reusing_the_enclosing_name_is_refused() {
    assert_runtime_error(
        "object A\n    has a default 1\n    object A\n        has b default 2\n    end\nend\n",
        "Object 'A' is already declared",
    );
    assert_runtime_error(
        "object A\n    object B\n        has b default 1\n        object B\n            has c default 2\n        end\n    end\nend\n",
        "Object 'B' is already declared",
    );
}

/// A parent chain that reaches back to the declaration that is walking it is the
/// cycle it is, however deep the nesting is that set it up.
#[test]
fn edge_object_a_nested_declaration_extending_its_own_name_is_a_cycle() {
    assert_runtime_error(
        "object A\n    object B extends B\n        has b default 1\n    end\nend\n",
        "Object 'B' extends 'B', which is already in its own parent chain",
    );
}

/// A failure inside a nested body is a failure of the program, so the `try` that
/// encloses both bodies catches it and neither type is left half-registered.
#[test]
fn edge_object_a_failure_inside_a_nested_body_is_caught_outside_both() {
    eval(
        "set caught to \"no\"\n\
         try\n    object Outer\n        has o default 1\n        object Inner\n            has i default 2\n            \
         set bad to 1 + \"one\"\n        end\n    end\n\
         catch error\n    set caught to \"yes\"\nend\n\
         expect caught to be \"yes\"\n",
    );
}

/// A nested body that recovers leaves both types declared, so the outer body's
/// name and the nested one's are both readable afterwards.
#[test]
fn edge_object_a_nested_body_that_recovers_leaves_both_types_declared() {
    eval(
        "object Outer\n    has o default 1\n    object Inner\n        has i default 2\n        \
         try\n            set bad to 1 + \"one\"\n        catch error\n            set inner_ok to \"caught\"\n        end\n    end\n    \
         set outer_ok to \"registered\"\nend\n\
         expect Outer.o to be 1\n\
         expect outer_ok to be \"registered\"\n\
         expect inner_ok to be \"caught\"\n",
    );
}

/// A `break` written after a nested declaration inside an `object` body written
/// in a loop still leaves the loop, and the body registers its type on the way
/// out.
#[test]
fn edge_object_an_object_body_nested_in_a_loop_can_break_out_of_it() {
    eval(
        "set n to 0\n\
         repeat 3 times\n    object Once\n        has v default 1\n        object Inner\n            has w default 2\n        end\n        \
         set n to n + 1\n        break\n    end\nend\n\
         expect n to be 1\n",
    );
}

/// A declaration inside a body is a declaration, so a failure reporting it names
/// the nested type rather than the one it is written in.
#[test]
fn edge_object_a_nested_declaration_with_an_unknown_parent_is_reported() {
    assert_runtime_error(
        "object Outer\n    object Inner extends Missing\n        has i default 1\n    end\nend\n",
        "Object 'Inner' extends 'Missing', which is not declared",
    );
}
