//! The REPL's completion vocabulary against the names the language accepts.
//!
//! The completer is the one place Redblue *invites* a name: a user who types
//! `for` and presses Tab is told what can follow it. That makes every entry in
//! its builtin table a promise, and the table was a hand-kept list that had
//! drifted from the language in both directions — it offered `formats`,
//! `parse_json`, `to_json`, `read`, `write` and `exists`, none of which a
//! program can name, and did not offer `bytes`, `csv`, `json`, `sys` or `time`,
//! all five of which are modules that answer.
//!
//! The unit tests beside the table check it from inside, where the table itself
//! can be read. These two go through the public surface a user reaches — the
//! completer's own `complete`, and the interpreter that decides whether a name
//! works — so the two halves cannot disagree: a name is offered, or a program
//! can name it, and never only one.

use redblue::repl::ReplCompleter;
use redblue::stdlib;

/// Every module the analyzer accepts as the receiver of a dotted call is offered
/// by the completer.
///
/// `stdlib::MODULES` is the analyzer's own predicate — `src/analyzer.rs:591` asks
/// it for any name it has not bound — so this asserts the pairing a user
/// experiences: a module name completes *because* the analyzer will take it, not
/// because somebody remembered to add it to a second list. The other direction
/// (nothing that is not a module is offered) is
/// `edge_the_repl_offers_no_builtin_a_program_cannot_call`, which needs the table
/// itself and so lives beside it in `src/repl/completer.rs`.
#[test]
fn edge_every_module_the_language_accepts_is_offered_by_the_repl() {
    let completer = ReplCompleter::new();

    for module in stdlib::MODULES {
        assert!(
            stdlib::is_module(module),
            "{module} is in MODULES, so is_module has to agree with it"
        );
        let offered = completer.complete(module);
        assert!(
            offered.contains(&(*module).to_string()),
            "{module} is a module a program can write, so typing it is completed: {offered:?}"
        );
    }
}

/// A name the completer offers is a name a program can use.
///
/// The six this pins are the ones the table held and the language refuses:
/// `formats` is not a module (the analyzer says `Unknown variable 'formats'`),
/// and `parse_json`, `to_json`, `read`, `write` and `exists` are not builtins
/// (`RuntimeError: Unknown function`) — JSON is `json.parse` and the file
/// functions are `files.read`. Each half is asserted: the completer does not
/// offer it, **and** a program that writes it is refused. Reinstating a name in
/// the table fails this test whichever side of it drifts back.
#[test]
fn edge_no_name_the_repl_offers_is_one_a_program_cannot_call() {
    let completer = ReplCompleter::new();

    for name in [
        "formats",
        "parse_json",
        "to_json",
        "read",
        "write",
        "exists",
    ] {
        let offered = completer.complete(name);
        assert!(
            offered.is_empty(),
            "{name} is not a name Redblue has, so it is not offered: {offered:?}"
        );

        let source = format!("say {name}(\"x\")");
        let result = redblue::run_source_value(&source);
        assert!(
            result.is_err(),
            "{name} is not offered, so a program must not be able to call it either: {result:?}"
        );
    }
}
