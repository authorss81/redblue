//! The REPL's completion vocabulary against the names the language accepts.
//!
//! The completer is the one place Redblue *invites* a name: a user who types
//! `for` and presses Tab is told what can follow it. That makes every entry in
//! its tables a promise, and all three of them were hand-kept lists that had
//! drifted from the language — in both directions, which is the part that matters:
//! the builtins offered `formats`, `parse_json`, `to_json`, `read`, `write` and
//! `exists`, none of which a program can name, and did not offer `bytes`, `csv`,
//! `json`, `sys` or `time`, all five of which are modules that answer, nor
//! twenty-one registered builtins that answer flat; the keywords offered `ask`,
//! `new`, `async`, `done` and `when`, none of which start anything, and did not
//! offer `constant`, `as`, `test`, `expect`, `might` or `fail`, all six of which
//! begin statements.
//!
//! The unit tests beside the tables check them from inside, where the tables
//! themselves can be read. These go through the public surface a user reaches —
//! the completer's own `complete`, and the interpreter that decides whether a
//! name works — so the two halves cannot disagree: a name is offered, or a
//! program can name it, and never only one.

use redblue::repl::ReplCompleter;
use redblue::stdlib;

/// What one engine did with a program.
#[derive(Debug)]
enum Outcome {
    Refused(String),
    Answered(String),
    /// The engine cannot run the program at all: the bytecode compiler refuses
    /// `to (x) … end` as an expression and says to run such a program with `rb
    /// run`. That is a fact about the engine, not a gap in a gate, and it is
    /// reported rather than counted as an answer.
    CannotRun(String),
}

/// Runs `source` on both engines — the tree-walker behind `rb run` and the
/// bytecode VM behind `rb compile` + `rb vm` — and reports what each did.
///
/// Both engines because the rules under test are *path* decisions.
/// `files.read(..)` is a `CALL_METHOD` and `files_read(..)` is a `CALL`, and both
/// hand the same string to the same resolver, so the only thing that tells them
/// apart is which instruction the program compiled to. An engine that never asked
/// the question answers `say files_read(..)`, and a gate that ran only the other
/// one is green.
fn outcomes(source: &str) -> Vec<(&'static str, Outcome)> {
    let walked = match redblue::run_source_value(source) {
        Ok(value) => Outcome::Answered(value.to_string()),
        Err(error) => Outcome::Refused(error.to_string()),
    };

    let tokens = match redblue::lexer::Lexer::tokenize(source) {
        Ok(tokens) => tokens,
        Err(error) => {
            return vec![
                ("the tree-walker", walked),
                ("the bytecode VM", Outcome::CannotRun(format!("{error}"))),
            ]
        }
    };
    let program = match redblue::parser::parse(tokens) {
        Ok(program) => program,
        Err(error) => {
            return vec![
                ("the tree-walker", walked),
                ("the bytecode VM", Outcome::CannotRun(format!("{error}"))),
            ]
        }
    };
    let compiled = match redblue::bytecode::compile_program(&program) {
        Ok(chunk) => chunk,
        Err(error) => {
            return vec![
                ("the tree-walker", walked),
                ("the bytecode VM", Outcome::CannotRun(format!("{error}"))),
            ]
        }
    };
    let bytecode = match redblue::bytecode::vm::BytecodeVm::new().run(&compiled) {
        Ok(value) => Outcome::Answered(value.to_string()),
        Err(error) => Outcome::Refused(error.to_string()),
    };

    vec![("the tree-walker", walked), ("the bytecode VM", bytecode)]
}

/// The two-word keywords, as the documents spell them.
///
/// A literal here rather than [`redblue::lexer::KEYWORD_PHRASES`], because the
/// gate below reads it to decide *which words are halves of a phrase*. Reading
/// the lexer's own declaration would make the gate agree with it by
/// construction: delete the declaration and every word becomes a word of its own,
/// every assertion passes, and the only thing left reporting the loss is the
/// count of multi-word entries. A literal is a second copy on purpose — it is
/// held equal to the declaration by
/// `edge_every_keyword_the_lexer_reserves_is_offered_by_the_repl`, and it is
/// held to what SPEC.md and `docs/GRAMMAR.md` write by
/// `tests/keyword_reservation_test.rs`, so a phrase cannot be lost by editing one
/// place.
const TWO_WORD: &[&str] = &["give back", "might fail"];

/// The phrase a keyword word is offered as: the phrase it heads, or the phrase it
/// is the tail of, or nothing when it is a word of its own.
///
/// A tail is a *vocabulary member* — it is in the phrase, which is offered whole —
/// which is a different claim from being a prefix a user can complete, and the
/// gates below keep the two apart on purpose.
fn phrase_of(word: &str) -> Option<&'static str> {
    TWO_WORD
        .iter()
        .find(|phrase| phrase.split(' ').any(|part| part == word))
        .copied()
}

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

/// Every builtin a program can write without a receiver in front of it is offered.
///
/// `edge_no_name_the_repl_offers_is_one_a_program_cannot_call` (above) walks the
/// table and asks whether what it holds is real. That is one direction of a
/// promise, and the missing half was the one no gate could see: a builtin that
/// answers and is not on the list is *absent*, not wrong, so the table was green
/// while twenty-one flat builtins completed to nothing — `log` and `exp` after `tan`,
/// `starts_with`, `ends_with`, `replace`, `shift`, `append`, `is_number`,
/// `is_text`, `is_list`, `is_record`, `to_text`, `to_number`, `to_list`,
/// `type_of`, `expect`, `assert`, `input`, `random_number`, `random_choice` and
/// `random_shuffle`.
///
/// So the registry is walked the other way. `stdlib::flat_builtin_names()` is
/// `BUILTIN_NAMES` minus the `<module>_<member>` encodings, and
/// `GLOBAL_CONSTANTS` is the two values that are not callable, so between them
/// they are everything `say <name>` can be a call to.
#[test]
fn edge_every_flat_builtin_the_language_has_is_offered_by_the_repl() {
    let completer = ReplCompleter::new();

    for name in stdlib::flat_builtin_names() {
        assert!(
            stdlib::is_builtin(name),
            "{name} came from BUILTIN_NAMES, so is_builtin has to agree with it"
        );
        let offered = completer.complete(name);
        assert!(
            offered.contains(&name.to_string()),
            "{name} is a builtin a program writes flat, so typing it is completed: {offered:?}"
        );
    }
    for name in stdlib::GLOBAL_CONSTANTS {
        let offered = completer.complete(name);
        assert!(
            offered.contains(&(*name).to_string()),
            "{name} is in scope for every program, so typing it is completed: {offered:?}"
        );
    }
}

/// A registered builtin that nothing implements is not offered, and the program
/// the language documents it in is refused.
///
/// `filter` and `reduce` are in `stdlib::BUILTIN_NAMES`, so `is_builtin` says
/// true, the analyzer lets a program write them and the old parity gate — which
/// asked whether an offered name is *registered* — said yes. That is the whole
/// defect: `filter` was completed to a name whose every call is `filter was
/// called with arguments it cannot use`, which is the REPL making a promise the
/// language refuses to keep.
///
/// Each half is asserted, so neither the offer nor the refusal can be reinstated
/// alone: the completer completes nothing for the name, **and** a program that
/// writes it is refused by a message naming it. `stdlib::UNIMPLEMENTED_BUILTINS`
/// is walked rather than the two names listed, so a builtin that is registered,
/// dead and offered again fails here whatever it is called.
///
/// **The program is the one in `stdlib::UNIMPLEMENTED_BUILTIN_PROGRAMS`, and that
/// is the whole difficulty.** This gate ran `say filter([1, 2])` — one argument,
/// where the function is missing — which is refused for its *arity*, not for the
/// function being absent: an implemented `filter` is refused in exactly the same
/// words, so implementing it without touching this test left it green and the name
/// listed as a gap while answering. The documented call,
/// `filter([1, 2, 3], to (x) give back x > 1 end)`, is the shape that tells the
/// two apart, and both engines are asked with it because a gate that ran one
/// would miss an engine that implemented the name and the other that did not.
#[test]
fn edge_a_builtin_nothing_implements_is_not_offered_and_cannot_be_called() {
    let completer = ReplCompleter::new();
    assert!(
        !stdlib::UNIMPLEMENTED_BUILTINS.is_empty(),
        "the registry still holds builtins nothing implements, so this is checking something"
    );

    for name in stdlib::UNIMPLEMENTED_BUILTINS {
        let offered = completer.complete(name);
        assert!(
            offered.is_empty(),
            "{name} is registered and nothing implements it, so it is not a name to \
             offer: {offered:?}"
        );

        let program = stdlib::unimplemented_program(name).unwrap_or_else(|| {
            panic!(
                "{name} is listed as unimplemented and has no program in \
                 stdlib::UNIMPLEMENTED_BUILTIN_PROGRAMS, so nothing runs it"
            )
        });
        for (engine, outcome) in outcomes(program) {
            // The tree-walker refuses it by name, and that is the refusal the
            // completer's promise rests on. The bytecode VM is held to the other
            // half — it must not answer — because its compiler refuses a function
            // literal as an expression and cannot run this program at all, which
            // is a stronger statement about the engine than a refusal would be,
            // and which would be hidden if the helper reported it as a pass.
            let message = match outcome {
                Outcome::Refused(message) => message,
                Outcome::CannotRun(message) => {
                    assert!(
                        !message.contains("Unknown function"),
                        "{name} has no function behind it, so {engine} reporting it does \
                         not exist is the wrong half of the answer:\n{program}\n{engine}: \
                         {message}"
                    );
                    continue;
                }
                Outcome::Answered(value) => panic!(
                    "{name} is not offered and is listed as unimplemented, but {engine} \
                     answered it: {value}\n{program}"
                ),
            };
            assert!(
                message.contains(name) && !message.contains("Unknown function"),
                "{name} is not offered, so {engine} has to refuse the documented call by \
                 naming the builtin that refused rather than claiming the name does not \
                 exist — otherwise the name should come out of the registry instead of \
                 this list:\n{program}\n{engine}: {message}"
            );
        }
    }
}

/// A module function is offered as its module, never as the name the registry
/// happens to store it under — **and the encoding is refused by the language, so
/// the offer and the language agree.
///
/// `files_read` is a registered builtin, so it passes every check that asks
/// whether an offered name is real — and no document writes it and no user should
/// type it. It is also the same defect as the six names above, one layer
/// down: `read` and `write` are real functions that answer `Unknown function` on
/// their own because the file functions live under `files.`. The twenty-five
/// encodings are read out of `BUILTIN_NAMES` rather than listed, so a module
/// function added later is covered by this without a line changing here.
///
/// Withholding the encoding used to be a judgement rather than a fact: `say
/// files_read("x")` answered, on both engines, so the REPL was hiding a working
/// name and `stdlib::flat_builtin_names` was not the set of names a program may
/// write flat. The language now refuses it where a program writes it without a
/// receiver — the second half of this gate, and the half that makes withholding
/// it correct rather than cautious — and names the spelling that works, so the
/// refusal is an answer a user can act on rather than a locked door.
#[test]
fn edge_a_module_function_is_offered_as_its_module_and_not_as_its_encoding() {
    let completer = ReplCompleter::new();
    let encodings: Vec<&str> = stdlib::BUILTIN_NAMES
        .iter()
        .copied()
        .filter(|name| stdlib::is_module_member_encoding(name))
        .collect();
    assert!(
        !encodings.is_empty(),
        "the registry holds module functions, so this is checking something"
    );

    for name in encodings {
        assert!(
            completer.complete(name).is_empty(),
            "{name} is the registry's encoding of a module function, so typing it \
             completes to nothing: {:?}",
            completer.complete(name)
        );
        let (module, member) = name
            .split_once('_')
            .expect("an encoding has a module in it");
        assert!(
            stdlib::module_has_member(module, member),
            "{name} is dropped as an encoding, so {module}.{member} is the spelling that \
             has to hold and the member table is what says it does"
        );
        assert!(
            completer.complete(module).contains(&module.to_string()),
            "and the module is what a user types first: {:?}",
            completer.complete(module)
        );

        // The language half. No arguments, because they are not the point and
        // cannot be invented for twenty-five functions: what has to hold is that
        // a program writing the encoding without a receiver is turned away, on
        // **both** engines, and that the refusal names the dotted spelling that
        // does work. An encoding that answered on one engine and not the other
        // would mean the offer is right on one of them and wrong on the other.
        let program = format!("say {name}(\"x\")");
        for (engine, outcome) in outcomes(&program) {
            let message = match outcome {
                Outcome::Refused(message) => message,
                Outcome::Answered(value) => panic!(
                    "{name} is not offered because a program may not write it flat, but \
                     {engine} answered it anyway:\n{program}\n{engine}: {value}"
                ),
                Outcome::CannotRun(reason) => panic!(
                    "{engine} cannot run `{program}` at all: {reason}\nA gate that cannot \
                     ask an engine the question is not a gate on that engine."
                ),
            };
            assert!(
                message.contains(&format!("{module}.{member}")),
                "{name} is refused as an encoding, so the refusal has to name the spelling \
                 that works, {module}.{member}, on {engine} as on every other:\n{program}\n\
                 {engine}: {message}"
            );
        }

        // And the dotted spelling is still reached on the same two engines. The
        // refusal above is only safe if it replaced a name that worked, and the
        // two paths are told apart by which instruction the program compiled to.
        let dotted = format!("say {module}.{member}()");
        for (engine, outcome) in outcomes(&dotted) {
            if let Outcome::Refused(message) = outcome {
                assert!(
                    !message.contains("not a name on its own"),
                    "{module}.{member} was refused by the rule that refuses the encoding \
                     written without a receiver, so the dotted spelling is gone:\n{dotted}\n\
                     {engine}: {message}"
                );
            }
        }
    }
}

/// Every keyword the lexer reserves is offered, and it is offered by its **own**
/// prefix.
///
/// `redblue::lexer::KEYWORDS` is the table the lexer lexes from, so it is the
/// whole of "a word the language reserves". The completer kept a copy of it that
/// was missing eight of them — `constant`, `as`, `test`, `expect`, `give`,
/// `back`, `might` and `fail` — so those words were absent from the vocabulary
/// even though the lexer knows them.
///
/// The question is asked of `complete`, not of the empty prefix. Asking the empty
/// prefix tests the *list*, and the list is a list of strings: `give back`
/// contains `back`, so a gate that walked it looking for the word `back` was
/// satisfied by a table that had unjoined the phrase and offered `give` and `back`
/// as two words — which is the very thing the phrase exists to prevent, and which
/// this gate passed. Completing each word by its own prefix is what a user does,
/// and it cannot be satisfied by a string that merely contains the word.
///
/// The two kinds of word are kept apart because they answer different questions:
///
/// - a **head** — `give`, and every keyword that is a word of its own — completes
///   to the entry that carries it;
/// - a **tail** — `back`, `fail` — is offered *through* its phrase, which is in
///   the vocabulary, and is not a prefix to complete: completion is a prefix
///   match, so `back` could only match after a user had already typed `give back`
///   and would complete it to a second `back`. `fail` is the same for
///   `might fail`.
///
/// A tail is asserted to be a member of the vocabulary and asserted not to be
/// offered as a word of its own, rather than being skipped: it is a keyword the
/// lexer reserves, and skipping it would be the gate quietly admitting that half
/// the table is exempt.
#[test]
fn edge_every_keyword_the_lexer_reserves_is_offered_by_the_repl() {
    let completer = ReplCompleter::new();
    let vocabulary = completer.complete("");

    // Which words pair up is read from [`TWO_WORD`] above and not from the
    // lexer's declaration, so the two have to agree — and the gate can fail when
    // they do not. Reading the declaration here would make the gate agree with it
    // by construction: empty the declaration and every word is a word of its own,
    // every assertion below passes, and a phrase is lost without a word of
    // complaint.
    assert_eq!(
        redblue::lexer::KEYWORD_PHRASES
            .iter()
            .map(|(head, tail)| format!("{head} {tail}"))
            .collect::<Vec<String>>(),
        TWO_WORD.to_vec(),
        "the lexer no longer declares the language's two-word keywords as such, so every \
         table that reads them would offer two half-keywords — and `back` would be offered \
         to a user who had already typed `give back`"
    );

    for (word, _) in redblue::lexer::KEYWORDS {
        let Some(phrase) = phrase_of(word) else {
            assert!(
                completer.complete(word).contains(&(*word).to_string()),
                "{word} is a keyword the lexer reserves and a word of its own, so typing \
                 it is offered it: {:?}",
                completer.complete(word)
            );
            continue;
        };

        // The head of a phrase completes to the phrase. The tail is the case
        // below, and the two cannot be the same assertion: `back` is not a prefix
        // of `give back`, so offering the phrase for it would be the table being
        // wrong rather than right.
        let head = phrase.split(' ').next().unwrap_or_default();
        if head == *word {
            assert!(
                completer.complete(word).contains(&phrase.to_string()),
                "{word} is half of the keyword the language spells as `{phrase}`, so \
                 typing it offers the whole phrase and not one word of it: {:?}",
                completer.complete(word)
            );
            continue;
        }

        // And the tail is in the vocabulary through that phrase.
        assert!(
            vocabulary.contains(&phrase.to_string()),
            "{word} is a keyword the lexer reserves, so the phrase carrying it is in the \
             vocabulary: {vocabulary:?}"
        );
        assert!(
            !completer.complete(word).contains(&phrase.to_string()),
            "{word} is the tail of `{phrase}`, so it is not a prefix of it and typing it \
             cannot offer it: {:?}",
            completer.complete(word)
        );
        assert!(
            !completer.complete(word).contains(&word.to_string()),
            "{word} is the tail of `{phrase}`, so it is not offered as a word of its own \
             either: {:?}",
            completer.complete(word)
        );
    }
}

/// A word the lexer does not reserve is not offered as a keyword — and cannot be,
/// because a program that starts with it does not run.
///
/// These are the eight the completer used to offer. Each half is asserted: the
/// completer no longer offers it, **and** a program that begins with it is
/// refused, so the word could not be reinstated on the strength of a stale
/// memory of the language.
///
/// `by` is deliberately not on this list. It is not a reserved word either, and
/// `src/parser.rs` reads it positionally — but it *does* start something
/// (`for each i from 0 to 10 by 5`, `split("a,b", by ",")`), so it is the one
/// completion the keyword table offers on the parser's word rather than the
/// lexer's, and it is gated apart in `src/repl/completer.rs`.
#[test]
fn edge_a_word_the_lexer_does_not_reserve_is_not_offered_as_a_keyword() {
    let completer = ReplCompleter::new();

    for word in [
        "are", "when", "ask", "new", "async", "wait", "parallel", "done",
    ] {
        assert!(
            !completer.complete(word).contains(&word.to_string()),
            "{word} is not a keyword the lexer reserves, so it is not offered: {:?}",
            completer.complete(word)
        );

        let result = redblue::run_source_value(&format!("say {word}"));
        assert!(
            result.is_err(),
            "{word} starts nothing, so a program cannot begin with it either: {result:?}"
        );
    }
}

/// The two words of `give back` are one thing to complete, and so are the two of
/// `might fail`.
///
/// The lexer gives each pair the same token and the language writes each as the
/// pair — `might f()` is refused by name rather than read as a guard — so the
/// whole phrase is what a user completing the head of it is offered. The tail is
/// not offered as a word of its own: completion is a prefix match, so `back`
/// would only match after `give back` had been typed, and would complete it to a
/// second `back`.
#[test]
fn edge_a_two_word_keyword_is_offered_whole_rather_than_a_half_at_a_time() {
    let completer = ReplCompleter::new();

    assert_eq!(
        completer.complete("giv"),
        vec!["give back".to_string()],
        "the pair is one keyword to complete"
    );
    assert_eq!(
        completer.complete("migh"),
        vec!["might fail".to_string()],
        "and so is the other pair, which is derived from the same rule rather than \
         named here a second time"
    );
    for tail in ["back", "fail"] {
        assert!(
            completer.complete(tail).is_empty(),
            "{tail} is the tail of a keyword the language spells as a pair, so it \
             completes to nothing: {:?}",
            completer.complete(tail)
        );
    }
}
