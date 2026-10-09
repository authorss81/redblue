//! Every builtin `src/stdlib.rs` registers is answerable.
//!
//! The finding these tests pin: `stdlib::builtins` inserted `abs`, `sqrt`,
//! `uppercase`, `trim`, `split`, `join` and thirty more as `Value::Builtin`, and
//! **nothing ever dispatched `Value::Builtin`**. Both VMs asked
//! `runtime::builtin`, which matched only `Value::Function`, so every one of
//! those names answered `Unknown function '<name>'` — the message for a name
//! nothing implements, applied to names the language documents. `text` and
//! `math` were not in `MODULES` either, so the dotted spellings died one step
//! earlier, in the analyzer, as `Unknown variable 'text'`.
//!
//! `stdlib::builtin_function` was the code that implemented several of them and
//! it was **dead**: its only callers were tests. A unit test calling a function
//! directly is green whether or not the language can reach it, which is how a
//! whole builtin surface stayed broken while the suite passed.
//!
//! So every test here goes through a real `run`, never through
//! `builtin_function`. That is the whole point of the file.

use std::process::Command;

use redblue::bytecode::vm::BytecodeVm;

/// Runs `source` and returns `(exit success, stdout, stderr)`.
///
/// Each run gets its own file, named for the test that asked. A single shared
/// scratch path would be a race: `cargo test` runs these in parallel, and one
/// test's program would be another's when both reach the filesystem at once —
/// which shows up as a failure in a test that did nothing wrong.
fn run_named(tag: &str, source: &str) -> (bool, String, String) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/p32docs");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    let file = dir.join(format!("{tag}.rb"));
    std::fs::write(&file, source).expect("the program is written");

    let out = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("rb runs");

    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Runs `source` under `tag` and asserts it printed `expected`.
fn says(tag: &str, source: &str, expected: &str) {
    let (ok, stdout, stderr) = run_named(tag, source);
    assert!(ok, "the program failed:\n{source}\n--- stderr\n{stderr}");
    assert_eq!(
        stdout.trim_end(),
        expected,
        "wrong output for:\n{source}\n--- stderr\n{stderr}"
    );
}

/// Runs `source` under `tag` and asserts it was refused, mentioning `needle`.
fn refuses(tag: &str, source: &str, needle: &str) {
    let (ok, _, stderr) = run_named(tag, source);
    assert!(
        !ok,
        "the program was accepted but should have been refused:\n{source}"
    );
    assert!(
        stderr.contains(needle),
        "the refusal does not mention '{needle}':\n{source}\n--- stderr\n{stderr}"
    );
}

// ============================================================ the flat spellings

/// The nine builtins `stdlib::builtin_function` implements and no VM called.
///
/// Each of these is a documented name that answered `Unknown function`. This
/// test fails on every one of them without the dispatch wired in, and the test
/// above it in the file history — `numeric_edge_test.rs` — passes, because it
/// calls `builtin_function` directly and so never asks whether the language can
/// reach it.
#[test]
fn edge_every_flat_builtin_the_stdlib_registers_answers() {
    says("abs", "say abs(-3)", "3");
    says("floor", "say floor(1.7)", "1");
    says("ceil", "say ceil(1.2)", "2");
    says("round", "say round(1.5)", "2");
    says("sqrt", "say sqrt(9)", "3");
    says("pow", "say pow(2, 10)", "1024");
    says("uppercase", r#"say uppercase("hi")"#, "HI");
    says("lowercase", r#"say lowercase("HI")"#, "hi");
    says("trim", r#"say trim("  x  ")"#, "x");
    says("length_text", r#"say length("abcd")"#, "4");
    says("contains", r#"say contains("hello", "ell")"#, "yes");
    says("starts_with", r#"say starts_with("hello", "he")"#, "yes");
    says("ends_with", r#"say ends_with("hello", "lo")"#, "yes");
    says("replace", r#"say replace("a-b", "-", "+")"#, "a+b");
    says("is_number", "say is_number(1)", "yes");
    says("is_text", r#"say is_text("a")"#, "yes");
    says("is_list", "say is_list([1])", "yes");
    says("pop", "say pop([1, 2, 3])", "3");
    says("shift", "say shift([1, 2, 3])", "1");
    says("to_list", r#"say to_list("a b")"#, "[a, b]");
}

/// `sqrt` of a negative has no real answer, and handing back `NaN` would put a
/// value into `Value::Number` that no comparison and no display can be trusted
/// on. It is refused by name rather than silently becoming `NaN`.
/// `sqrt` of a negative has no real answer, and a program that asks for one is
/// refused **by name** rather than handed `NaN` — a `Value::Number` no
/// comparison and no display can be trusted on.
///
/// Two existing tests meet here and they are worth reading together.
/// `numeric_edge_test.rs` asserts `builtin_function("sqrt", [-1])` is
/// `Some(Value::Nothing)` — the helper's own contract — and separately that
/// `set x to sqrt(-1)` is a `RuntimeError` at the language level. Both hold,
/// because the helper answers `nothing` and the language turns that into a
/// refusal naming `sqrt`. Before this phase neither was reachable from a
/// program at all, and the second test was green because `sqrt` answered
/// `Unknown function`, which is also a `RuntimeError`.
#[test]
fn edge_sqrt_of_a_negative_is_refused_by_name() {
    refuses("sqrt_neg", "say sqrt(-1)", "sqrt");
    // The boundary at zero is still an answer, so the guard is the sign and not
    // a blanket refusal of small roots.
    says("sqrt_zero", "say sqrt(0)", "0");
    says("sqrt_pos", "say sqrt(9)", "3");
}

/// A builtin called with arguments it cannot use is refused **by name**. It is
/// not `Unknown function`, which is the message for a name nothing implements:
/// `uppercase` exists, and a program that spelled it correctly deserves to be
/// told what was wrong with the call.
#[test]
fn edge_a_builtin_refuses_bad_arguments_by_name() {
    refuses("bad_args_upper", r#"say uppercase(1)"#, "uppercase");
    refuses("bad_args_split", r#"say text.split("a", 1)"#, "split");
}

// ============================================================ the module spellings

/// `text.uppercase("hi")` died in the analyzer as `Unknown variable 'text'`,
/// because `MODULES` listed eight names and none of them was `text`. The
/// receiver is a module, so the analyzer must accept it and the function must
/// be found behind it.
#[test]
fn edge_a_module_spelling_of_a_documented_builtin_runs() {
    says("mod_upper", r#"say text.uppercase("hi")"#, "HI");
    says("mod_trim", r#"say text.trim("  x  ")"#, "x");
    says("mod_abs", "say math.abs(-3)", "3");
    says("mod_sqrt", "say math.sqrt(9)", "3");
    says("mod_json", r#"say json.parse("{}")"#, "{}");
    says("mod_csv", r#"say csv.parse("a,b")"#, "[[a, b]]");
    says(
        "mod_list_map",
        "say list.map([1, 2], to (x) x + 1)",
        "[2, 3]",
    );
}

/// `math.PI` was in SPEC.md's Properties example, where a module is asked for a
/// *member*. A module has functions and no members, so the name is not in the
/// language and the analyzer is right to refuse it — `PI` is the global.
///
/// `formats` is the same class of error and is asserted here because it is the
/// one a reader is most likely to try: SPEC.md had a whole § formats for a
/// module that never existed. It stays a refusal rather than becoming a
/// namespace that answers nothing.
#[test]
fn edge_a_module_has_functions_and_not_members() {
    says("mod_pi", "say PI", "3.141592653589793");
    refuses("mod_pi_member", "say math.PI", "math");
    refuses("mod_formats", r#"say formats.parse_json("{}")"#, "formats");
}

/// `text.split` and `text.join` are in SPEC.md and README.md and were answered
/// by nothing at all. Both directions are pinned, because a `split` that
/// returns the whole string and a `join` that returns an empty one would each
/// pass a one-sided test.
#[test]
fn edge_text_split_and_join_round_trip() {
    says("split", r#"say text.split("a,b,c", ",")"#, "[a, b, c]");
    says("join", r#"say text.join(["a", "b"], "-")"#, "a-b");
    says(
        "roundtrip",
        r#"say text.join(text.split("x|y|z", "|"), "+")"#,
        "x+y+z",
    );
}

/// A module name that is not a module must say so in the spelling the program
/// wrote. The internal encoding is `module_function`, so an error built from
/// the qualified name alone leaks `nosuchmodule_read` instead of
/// `nosuchmodule.read`.
#[test]
fn edge_an_unknown_module_function_names_the_dotted_spelling() {
    refuses(
        "unknown_module",
        r#"say nosuchmodule.read("x")"#,
        "nosuchmodule.read",
    );
}

// ============================================================ both engines agree

/// The tree-walker and the bytecode VM are two implementations of one language,
/// so a builtin that answers in one and not the other is a language that
/// changes meaning with its execution strategy.
///
/// The assertion is deliberately "did not answer `Unknown function`" rather than
/// a captured stdout: `say` writes to the process's real stdout, which an
/// in-process test cannot read back, and a test that captured it would be
/// testing its own plumbing. What is at stake is reachability, and that is
/// visible in the error.
#[test]
fn edge_the_bytecode_vm_answers_the_same_builtins_the_walker_does() {
    for source in [
        "say abs(-3)",
        r#"say uppercase("hi")"#,
        r#"say trim("  x  ")"#,
        "say sqrt(9)",
        r#"say text.uppercase("hi")"#,
        r#"say text.split("a,b", ",")"#,
    ] {
        let tokens = redblue::lexer::Lexer::tokenize(source)
            .unwrap_or_else(|error| panic!("{source} should lex: {error:?}"));
        let program = redblue::parser::parse(tokens)
            .unwrap_or_else(|error| panic!("{source} should parse: {error:?}"));
        let chunk = redblue::bytecode::compile_program(&program)
            .unwrap_or_else(|error| panic!("{source} should compile: {error:?}"));

        match BytecodeVm::new().run(&chunk) {
            Ok(_) => {}
            Err(error) => {
                panic!("the bytecode VM refused `{source}` where the walker answers it: {error:?}")
            }
        }
    }
}

// ============================================================ the surface itself

/// `builtins()` registers a name; something has to answer it. This walks the
/// registry and asks each name whether the language can call it, so a builtin
/// added to the table without an implementation fails here rather than in a
/// user's program.
///
/// The list of names that are *deliberately* registered but unreachable is
/// spelled out, with the reason, rather than being skipped silently — that is
/// what makes this a test of the surface instead of a test of today's surface.
#[test]
fn edge_no_registered_builtin_is_unreachable_from_a_program() {
    // Names `builtins()` registers that no program can call today. Each is
    // listed with why it is registered: these are declared-but-unimplemented,
    // and FINDINGS.md §2 carries them as work for a later phase.
    const KNOWN_GAPS: &[&str] = &[
        // Higher-order: they must call a Redblue function, which needs a walker's
        // captured scopes. `map` is implemented in the walker; these are not.
        "filter",
        "reduce",
        "list_map", // `map` is handled by `Vm::map_builtin`.
        // List builtins with no agreed semantics in SPEC.md yet.
        "pop",
        "shift", // Type predicates and conversions: registered, not implemented.
        "is_number",
        "is_text",
        "is_list",
        "is_record",
        "to_list", // Module members that are documented but not implemented.
        "console_clear",
        "time_unix",
        "csv_parse",
        "network_get",
        "network_post",
    ];

    for (name, value) in redblue::stdlib::builtins() {
        let redblue::Value::Builtin(builtin) = value else {
            continue;
        };
        if KNOWN_GAPS.contains(&builtin.as_str()) {
            continue;
        }
        // `say <name>()` with no arguments: a name nothing answers says
        // `Unknown function`, and a name something answers says what it wanted.
        let (ok, _, stderr) = run_named(&format!("surface_{name}"), &format!("say {name}()"));
        assert!(
            !stderr.contains("Unknown function"),
            "`{name}` is registered in stdlib::builtins and answered by nothing.\n\
             Either implement it or add it to KNOWN_GAPS in this file with its reason.\n\
             stderr was:\n{stderr}"
        );
        let _ = ok;
    }
}
