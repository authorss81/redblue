//! A function value must carry the bindings that were in scope where it was
//! declared.
//!
//! `Value::Function` stored a name and a parameter list, and the body lived in
//! a flat `HashMap<String, Vec<Stmt>>` keyed by that name. A nested declaration
//! therefore lost its enclosing bindings, two nested declarations of the same
//! name overwrote each other, and a call through an alias found no body at all.
//!
//! The policy this file pins down:
//!
//! * **Capture by value, at declaration time.** A declaration copies every live
//!   local scope. A variable the body reads is the value it had where the
//!   function was written; a variable it assigns is assigned for that one call
//!   only and leaves the enclosing scope alone.
//! * **Globals are not captured.** They are read at call time, so a closure
//!   that counts in a global counts for every call.
//! * **Captured scopes sit above the caller's frames.** A closure that has
//!   escaped its defining activation reads its own environment, never a
//!   same-named binding of whoever called it.
//!
//! Note on the programs below: `give back` does not leave a function, so a
//! conditional result is computed into a variable and returned after the block.
//! See FINDINGS.md.

use redblue::Error;
use redblue::Value;

/// Lexes and parses `source`.
#[track_caller]
fn parse(source: &str) -> redblue::parser::Program {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    redblue::parser::parse(tokens).expect("source should parse")
}

/// Runs `source` and returns the value of its last statement.
#[track_caller]
fn eval(source: &str) -> Value {
    let (_vm, result) = redblue::run_isolated(&parse(source));
    result.unwrap_or_else(|error| panic!("source should have run, failed with {:?}", error))
}

/// Runs `source` and returns the error it produced.
#[track_caller]
fn eval_err(source: &str) -> Error {
    let (_vm, result) = redblue::run_isolated(&parse(source));
    result.expect_err("source should have failed")
}

/// Asserts `source` runs and its last statement reads as `expected`.
#[track_caller]
fn assert_yields(source: &str, expected: &str) {
    assert_eq!(eval(source).to_string(), expected, "\nprogram:\n{source}");
}

/// A caller's own bindings must not leak into a closure's body.
///
/// This is the case that pins the order of the scope stack: `x` is live in the
/// caller's frame with a different value, and the closure must still read the
/// `x` it was written next to.
#[test]
fn nested_declaration_sees_the_enclosing_binding() {
    assert_yields(
        "
        to make_adder(x)
            set bias to 1
            to add(y)
                give back x + y + bias
            end
            give back add
        end
        set add_tens to make_adder(10)
        give back add_tens(5)
        ",
        "16",
    );
}

#[test]
fn edge_escaped_closure_reads_its_own_environment_not_the_callers() {
    assert_yields(
        "
        to make_adder(x)
            to add(y)
                give back x + y
            end
            give back add
        end
        set add_ten to make_adder(10)
        to caller()
            set x to 1000
            give back add_ten(5)
        end
        give back caller()
        ",
        "15",
    );
}

/// Two declarations of the same nested name are two different functions.
///
/// Under the old body-by-name map the second declaration overwrote the first,
/// so both closures ran the same body.
#[test]
fn same_nested_name_declared_twice_keeps_both_bodies() {
    assert_yields(
        "
        to make_a(start)
            set count to start
            to inner()
                give back count + 1
            end
            give back inner
        end
        to make_b(start)
            set count to start
            to inner()
                give back count + 100
            end
            give back inner
        end
        set a to make_a(1)
        set b to make_b(1)
        set from_a to a()
        set from_b to b()
        give back [from_a, from_b]
        ",
        "[2, 101]",
    );
}

/// A parameter shadows a captured binding of the same name.
#[test]
fn a_parameter_shadows_what_the_declaration_captured() {
    assert_yields(
        "
        to outer(v)
            to inner(v)
                give back v * 2
            end
            give back inner(21)
        end
        give back outer(5)
        ",
        "42",
    );
}

/// Capture is a copy: assigning to a captured name inside the body changes that
/// one call and nothing outside it.
///
/// `1211` rather than `1223`: if the assignment escaped to the enclosing scope
/// the second call would start from 11 and the arithmetic would come out as
/// `1223`.
#[test]
fn capture_is_by_value_so_an_assignment_does_not_escape() {
    assert_yields(
        "
        to outer(seed)
            to bump(amount)
                set seed to seed + amount
                give back seed
            end
            set first to bump(1)
            set second to bump(1)
            give back first * 1000 + second
        end
        give back outer(10)
        ",
        "11011",
    );
}

/// A global is not part of the capture, so a closure that counts in one counts
/// for every call, including through several levels of nesting.
#[test]
fn a_global_is_read_at_call_time_not_captured() {
    assert_yields(
        "
        set ticks to 0
        to make_ticker()
            to tick()
                set ticks to ticks + 1
                give back ticks
            end
            give back tick
        end
        set a_tick to make_ticker()
        set b_tick to make_ticker()
        set one to a_tick()
        set two to b_tick()
        set three to a_tick()
        give back [one, two, three]
        ",
        "[1, 2, 3]",
    );
}

/// Three levels of nesting, each level capturing the one outside it.
#[test]
fn three_levels_of_nesting_each_capture_the_level_outside() {
    assert_yields(
        "
        to level_one(a)
            to level_two(b)
                set from_two to b * 2
                to level_three(c)
                    set from_three to c + 1
                    give back a + b + from_two + from_three
                end
                give back level_three(1)
            end
            give back level_two(10)
        end
        give back level_one(100)
        ",
        "132",
    );
}

/// Mutual recursion between two nested declarations. The second can only be
/// reached because it was declared in the scope the first closes over.
#[test]
fn nested_declarations_reach_each_other() {
    assert_yields(
        "
        to outer(n)
            to even(k)
                set answer to yes
                if k is 0 then
                    set answer to yes
                else
                    set answer to odd(k - 1)
                end
                give back answer
            end
            to odd(k)
                set answer to no
                if k is 0 then
                    set answer to no
                else
                    set answer to even(k - 1)
                end
                give back answer
            end
            give back even(n)
        end
        set ten to outer(10)
        set seven to outer(7)
        set zero to outer(0)
        give back [ten, seven, zero]
        ",
        "[yes, no, yes]",
    );
}

/// A closure that calls itself reaches its own name through the scope it was
/// declared in, 500 calls deep.
#[test]
fn a_closure_recurses_through_its_own_declaration_scope() {
    assert_yields(
        "
        to countdown(n)
            to go(k)
                set label to \"tick\"
                if k is 0 then
                    set label to \"tick\"
                else
                    set label to go(k - 1)
                end
                give back label
            end
            give back go(n)
        end
        give back countdown(500)
        ",
        "tick",
    );
}

/// Top-level mutual recursion, which must keep working: neither function closes
/// over anything.
#[test]
fn top_level_mutual_recursion_is_unchanged() {
    assert_yields(
        "
        to is_even(n)
            set answer to yes
            if n is 0 then
                set answer to yes
            else
                set answer to is_odd(n - 1)
            end
            give back answer
        end
        to is_odd(n)
            set answer to no
            if n is 0 then
                set answer to no
            else
                set answer to is_even(n - 1)
            end
            give back answer
        end
        set ten_even to is_even(10)
        set seven_even to is_even(7)
        set ten_odd to is_odd(10)
        set zero to is_even(0)
        give back [ten_even, seven_even, ten_odd, zero]
        ",
        "[yes, no, no, yes]",
    );
}

/// Reading a function value and calling it through another name: the alias must
/// run the body it was bound to. Before, the call looked the body up by the
/// *called* name and found nothing, so the alias yielded `nothing`.
#[test]
fn a_function_value_called_through_an_alias_runs_its_own_body() {
    assert_yields(
        "
        to add(a, b)
            give back a + b
        end
        set alias to add
        give back alias(2, 3)
        ",
        "5",
    );
}

/// Two closures stored in one list keep separate captures, reached by index.
#[test]
fn edge_two_closures_in_one_list_keep_separate_captures() {
    assert_yields(
        "
        to pair(a, b)
            to first(x)
                give back x + a
            end
            to second(x)
                give back x + b
            end
            give back [first, second]
        end
        set both to pair(1, 100)
        set low to both[0]
        set high to both[1]
        give back [low(0), high(0)]
        ",
        "[1, 100]",
    );
}

/// A repeated record key keeps the last value, a missing field reads as
/// `nothing`, and a closure held in a record field is still a function value.
#[test]
fn edge_a_closure_in_a_record_with_duplicate_and_missing_keys() {
    assert_yields(
        "
        to wrap(prefix)
            to label(value)
                give back prefix + value
            end
            give back {tag: \"first\", tag: \"second\", fn: label, missing: nothing}
        end
        set box to wrap(\"n=\")
        set held to box.fn
        give back [box.tag, box.missing, type_of(box.fn), type_of(held)]
        ",
        "[second, nothing, function, function]",
    );
}

/// Empty text and an empty list captured by value. A function value yields
/// `nothing` — the value of its last statement — rather than the empty list, so
/// the closure is asked for the captured values one at a time.
#[test]
fn edge_empty_captured_values_survive() {
    assert_yields(
        "
        to outer()
            set text to \"\"
            set items to []
            to describe(which)
                set answer to text
                if which is 2 then
                    set answer to items
                end
                give back answer
            end
            give back describe
        end
        set d to outer()
        set the_text to d(1)
        set the_list to d(2)
        give back [type_of(the_text), length(the_text), the_list]
        ",
        "[text, 0, []]",
    );
}

/// A declaration that captures nothing is still a function.
#[test]
fn edge_a_declaration_with_no_captured_bindings_is_a_function() {
    assert_yields(
        "
        to outer()
            to inner()
                give back 7
            end
            give back type_of(inner)
        end
        give back outer()
        ",
        "function",
    );
}

/// Numeric boundaries inside a closure body: a whole number at `2^53`, and the
/// division by zero the language rejects.
#[test]
fn edge_numeric_boundaries_inside_a_closure_are_unchanged() {
    assert_yields(
        "
        to outer(n)
            to half(x)
                give back x / 2
            end
            give back half(n)
        end
        give back outer(9007199254740993)
        ",
        "4503599627370496",
    );
    let error = eval_err(
        "
        to outer(n)
            to over(x)
                give back x / 0
            end
            give back over(n)
        end
        give back outer(1)
        ",
    );
    assert!(
        matches!(&error, Error::Runtime(message, _) if message.contains("zero")),
        "division by zero inside a closure must stay a Runtime error, got {error:?}"
    );
}

/// Unicode text captured by value and read back.
#[test]
fn edge_unicode_capture_survives() {
    assert_yields(
        "
        to outer(greeting)
            to greet(name)
                give back greeting + \" \" + name
            end
            give back greet
        end
        set greet to outer(\"こんにちは\")
        give back greet(\"🌍\")
        ",
        "こんにちは 🌍",
    );
}

/// The display of a function value, and `type_of` for one, must not leak the
/// capture or the body.
#[test]
fn edge_a_function_value_displays_as_its_declaration_name() {
    assert_yields(
        "
        to outer(n)
            to inner()
                give back n
            end
            give back [inner, type_of(inner)]
        end
        give back outer(1)
        ",
        "[<function inner>, function]",
    );
}

/// Calling a name that is not in scope inside a closure is a clean runtime
/// error naming it, not `nothing` and not a panic.
#[test]
fn edge_a_closure_calling_an_undefined_name_is_a_caught_error() {
    let error = eval_err(
        "
        to outer()
            to inner()
                no_such_helper(1)
            end
            give back inner
        end
        set broken to outer()
        give back broken()
        ",
    );
    let message = match &error {
        Error::Runtime(message, _) => message.clone(),
        other => panic!("expected a Runtime error, got {other:?}"),
    };
    assert_eq!(
        message, "Unknown function 'no_such_helper'",
        "the error must name the function that is missing"
    );
}

/// The scope stack must be balanced after a caught failure inside a closure, so
/// the same program can keep running.
#[test]
fn edge_a_caught_failure_inside_a_closure_leaves_the_stack_balanced() {
    assert_yields(
        "
        to outer(n)
            to blow_up()
                give back n / 0
            end
            to fine()
                give back n * 2
            end
            give back [blow_up, fine]
        end
        set both to outer(21)
        set caught to 0
        try
            set bad to both[0]
            set ignored to bad()
        catch error
            set caught to 1
        end
        set good to both[1]
        give back caught * 1000 + good()
        ",
        "1042",
    );
}

/// An unbounded closure recursion is a clean depth error, and the caught error
/// leaves the program able to call the closure again afterwards.
#[test]
fn edge_unbounded_closure_recursion_is_a_caught_error() {
    assert_yields(
        "
        to outer()
            to spin(n)
                spin(n + 1)
            end
            give back spin
        end
        set spin to outer()
        set caught to 0
        try
            spin(0)
        catch error
            set caught to 1
        end
        set caught_again to 0
        try
            spin(0)
        catch error
            set caught_again to 1
        end
        give back caught * 10 + caught_again
        ",
        "11",
    );
}

/// A closure declared inside a loop body must not carry the previous
/// iteration's binding: each iteration captures the scope it was written in.
/// `60` rather than `90` (all three iterations reading the last `n`).
#[test]
fn edge_a_closure_declared_in_a_loop_captures_that_iteration() {
    assert_yields(
        "
        set total to 0
        for each n in [10, 20, 30]
            to scaled(x)
                give back x + n
            end
            set total to total + scaled(0)
        end
        give back total
        ",
        "60",
    );
}

/// The same nested name declared in two sibling scopes stays separate.
#[test]
fn edge_the_same_nested_name_in_two_sibling_scopes_stays_separate() {
    assert_yields(
        "
        to outer(a, b)
            to first(x)
                give back x + a
            end
            set low to first(0)
            to second(x)
                give back x + b
            end
            set high to second(0)
            give back [low, high]
        end
        give back outer(1, 2)
        ",
        "[1, 2]",
    );
}

/// Assigning to a variable of an enclosing scope reaches that scope.
///
/// A read and a write of one name now resolve the same way, which is what makes
/// capture-by-value coherent: a body that assigns to a captured name updates
/// its own copy. The same resolution also applies outside a closure — assigning
/// to a parameter from inside a loop body updates the parameter rather than
/// silently creating a global of that name. Before, `add_to(10)` answered `10`
/// and left a global `base` behind, because the loop body's scope was the only
/// one a write could reach.
#[test]
fn edge_assigning_to_an_enclosing_binding_reaches_that_binding() {
    assert_yields(
        "
        to add_to(base)
            for each v in [1, 2]
                set base to base + v
            end
            give back base
        end
        give back add_to(10)
        ",
        "13",
    );
    assert_yields(
        "
        to make()
            set hidden to 5
            give back 1
        end
        give back make()
        set seen to hidden
        give back seen
        ",
        "5",
    );
}

/// A body that assigns to a captured name updates its own copy and leaves the
/// enclosing scope's binding alone, so two closures over one name do not see
/// each other's assignments.
#[test]
fn edge_two_closures_over_one_name_do_not_share_an_assignment() {
    assert_yields(
        "
        to outer(seed)
            to left(amount)
                set seed to seed + amount
                give back seed
            end
            to right(amount)
                set seed to seed + amount
                give back seed
            end
            set one to left(10)
            set two to right(1000)
            give back [one, two]
        end
        set values to outer(5)
        set one to values[0]
        set two to values[1]
        give back [one, two]
        ",
        "[15, 1005]",
    );
}

/// A `FunctionValue` built by a Rust caller carries its declared body, so the
/// public `Value` variant is usable without the parser.
#[test]
fn a_hand_built_function_value_is_usable_and_displayed() {
    use redblue::FunctionValue;

    let tokens = redblue::lexer::Lexer::tokenize("say 1").expect("tokens");
    let program = redblue::parser::parse(tokens).expect("program");
    let body = program.statements;

    let function = Value::Function(FunctionValue {
        name: "hand_made".to_string(),
        params: Vec::new(),
        body: std::sync::Arc::new(body),
        captured: std::sync::Arc::new(Vec::new()),
    });

    assert_eq!(function.to_string(), "<function hand_made>");
    assert!(function.is_truthy());
    assert_ne!(function, Value::Nothing);
}
