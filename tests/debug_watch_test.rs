//! The rules the debug sub-REPL's watch scratch VM answers by.
//!
//! A watch is evaluated on a VM of its own, against the bindings and the
//! language context of the step being observed. Two things about that VM are
//! worth a test of their own, because neither is visible from the outside of
//! the REPL: the step budget of one evaluation must not be spent by the next
//! one, and the language context the program built must be what a dotted call
//! is resolved against.

use std::collections::HashMap;

use redblue::parser::Program;
use redblue::Vm;

/// The program `source` is, parsed but not analysed.
fn program(source: &str) -> Program {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("the source should lex");
    redblue::parser::parse(tokens).expect("the source should parse")
}

/// Six statements: more than half of a ten-step budget, so two of them do not
/// fit in one budget together.
fn six_statements() -> Program {
    program(
        "set a to 1\n\
         set b to 2\n\
         set c to 3\n\
         set d to 4\n\
         set e to 5\n\
         set f to 6\n",
    )
}

/// The step budget of one watch evaluation does not carry into the next.
///
/// The scratch VM that answers a watch is one VM for every watch and every
/// step, so its budget is a budget for the whole session unless the counters
/// are restarted with the bindings. They are: a watch evaluation is a run of
/// its own, and the steps the last one spent are not the steps this one has.
///
/// Without the restart the second run here starts with six steps already spent
/// and fails half way through — a watch that fails for a reason that has
/// nothing to do with what it watches.
#[test]
fn edge_the_step_budget_of_one_watch_evaluation_does_not_carry_into_the_next() {
    let mut vm = Vm::with_max_steps(10);

    vm.run_quietly(&six_statements())
        .expect("the first run should fit in the budget");

    // The bindings of an observed step replace what the scratch was holding,
    // and the counters of a run restart with them.
    vm.observe_bindings(&[], &HashMap::new());

    vm.run_quietly(&six_statements())
        .expect("the second run should have a budget of its own");
}

/// A watch is evaluated against the language context the program built, not
/// only against the program's bindings.
///
/// A module the program declared is a namespace with a member in it, and the
/// member is reached under the name the module publishes it as. Without the
/// context the watch is evaluated against, that call is a "no such function"
/// the watch cannot answer — and the watch is the only thing that reports it.
#[test]
fn edge_a_watch_is_evaluated_against_the_language_context_the_program_built() {
    let mut vm = Vm::new();
    let declared = program(
        "module Greeter\n\
         \x20   to greet(who)\n\
         \x20       give back \"hello \" + who\n\
         \x20   end\n\
         \x20   export all\n\
         end\n",
    );
    vm.run_quietly(&declared)
        .expect("the module declaration should run");

    // The scratch is a VM of its own, pointed at the program's bindings and at
    // the program's context — and nothing else.
    let mut scratch = Vm::new();
    scratch.observe_context(vm.call_context());
    let (scopes, globals) = vm.current_bindings();
    scratch.observe_bindings(scopes, globals);

    let answer = scratch
        .run_quietly(&program("Greeter.greet(\"world\")"))
        .expect("the watch should reach the module's member");
    assert_eq!(
        answer.to_string(),
        "hello world",
        "the watch called the member the program published"
    );
}
