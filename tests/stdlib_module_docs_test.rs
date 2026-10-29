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

use std::io::{Read, Write};
use std::process::Command;

use redblue::bytecode::compile_source;
use redblue::bytecode::vm::BytecodeVm;
use redblue::{run_source_value, Error, Value};

/// The environment variables that decide whether `network` talks to a proxy.
///
/// Removed from every child, for the same reason
/// `tests/stdlib_module_names_test.rs` removes them: `reqwest`'s default client
/// builder reads them, so a runner with `HTTP_PROXY` set would send the loopback
/// request somewhere else and the `network` rows below would fail on the
/// environment rather than on the language.
const PROXY_ENV: &[&str] = &[
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
];

/// Runs `source` and returns `(exit success, stdout, stderr)`.
///
/// Each run gets its own file, named for the test that asked. A single shared
/// scratch path would be a race: `cargo test` runs these in parallel, and one
/// test's program would be another's when both reach the filesystem at once —
/// which shows up as a failure in a test that did nothing wrong.
///
/// Stdin is closed rather than inherited, so the `input` row reads end-of-input
/// immediately instead of waiting on a terminal nobody is sitting at.
fn run_named(tag: &str, source: &str) -> (bool, String, String) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/p32docs");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    let file = dir.join(format!("{tag}.rb"));
    std::fs::write(&file, source).expect("the program is written");

    let mut command = Command::new(env!("CARGO_BIN_EXE_rb"));
    command
        .arg("run")
        .arg(&file)
        .stdin(std::process::Stdio::null());
    for name in PROXY_ENV {
        command.env_remove(name);
    }
    let out = command.output().expect("rb runs");

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
/// Reachability is not enough, and this test used to be reachability and nothing
/// else: it ran six builtins and failed only if one of them refused, so `sqrt`
/// answering `1` and `text.split` answering one element instead of two both
/// passed. So the answer is read, not merely its absence: each expected line is
/// written out **literally** below, because an expectation derived from the
/// interpreter passes whatever the interpreter did.
///
/// Both engines are held to the same literal. `take_output` is how an in-process
/// caller reads what a program said — `say` also writes to the process's real
/// stdout, which a test cannot read back — and reading it is not testing the
/// test's own plumbing, it is the only channel the language has for "what did you
/// answer".
#[test]
fn edge_the_bytecode_vm_answers_the_same_builtins_the_walker_does() {
    for (source, expected) in [
        ("say abs(-3)", "3"),
        (r#"say uppercase("hi")"#, "HI"),
        (r#"say trim("  x  ")"#, "x"),
        ("say sqrt(9)", "3"),
        (r#"say text.uppercase("hi")"#, "HI"),
        (r#"say text.split("a,b", ",")"#, "[a, b]"),
    ] {
        let tokens = redblue::lexer::Lexer::tokenize(source)
            .unwrap_or_else(|error| panic!("{source} should lex: {error:?}"));
        let program = redblue::parser::parse(tokens)
            .unwrap_or_else(|error| panic!("{source} should parse: {error:?}"));
        let chunk = redblue::bytecode::compile_program(&program)
            .unwrap_or_else(|error| panic!("{source} should compile: {error:?}"));

        let mut vm = BytecodeVm::new();
        // The lines are taken, not read off stdout, so a failure report is not
        // interleaved with the program's own output.
        vm.set_echo(false);
        vm.run(&chunk).unwrap_or_else(|error| {
            panic!("the bytecode VM refused `{source}` where the walker answers it: {error:?}")
        });
        assert_eq!(
            vm.take_output(),
            vec![expected.to_string()],
            "the bytecode VM must answer `{source}` with `{expected}`, not merely not fail"
        );

        let (mut walker, outcome) = redblue::run_isolated(&program);
        outcome.unwrap_or_else(|error| {
            panic!("the walker refused `{source}` where the bytecode VM answers it: {error:?}")
        });
        assert_eq!(
            walker.take_output(),
            vec![expected.to_string()],
            "the walker must answer `{source}` with `{expected}`, not merely not fail"
        );
    }
}

/// A builtin that answers the **wrong value** fails the suite on both engines.
///
/// This is the mutation the test above exists to catch, pinned on its own: an
/// `abs` that dropped its sign, a `sqrt` that answered `1` whatever it was
/// given, an `uppercase` that answered nothing at all. Each row is one builtin,
/// one literal expected line, and the empty-string row is here because "the
/// program said nothing" and "the program said the empty string" are the same
/// `Vec` to a test that only counts lines.
#[test]
fn edge_a_builtin_that_answers_the_wrong_value_fails_on_both_engines() {
    for (source, expected) in [
        // The value, not the name: `sqrt(9)` is `3` and not `9` or `1`.
        ("say sqrt(9)", "3"),
        // A sign.
        ("say abs(-3)", "3"),
        // A list of two, not one: a `split` that did not split would answer `[a, b]`
        // and this is the row that tells it from `[a]`.
        (r#"say text.split("a,b", ",")"#, "[a, b]"),
        // A non-empty line against an empty one: `uppercase("")` answers `""`,
        // which is one empty line, and `uppercase("hi")` answers `HI`.
        (r#"say uppercase("hi")"#, "HI"),
        (r#"say uppercase("")"#, ""),
        // Whitespace removed from both ends and nothing else removed.
        (r#"say trim("  x  ")"#, "x"),
        // …including when there is none to remove, and when what is left is not
        // ASCII: a `trim` that measured bytes would eat half a CJK character.
        (r#"say trim("日本語")"#, "日本語"),
    ] {
        let tokens = redblue::lexer::Lexer::tokenize(source)
            .unwrap_or_else(|error| panic!("{source} should lex: {error:?}"));
        let program = redblue::parser::parse(tokens)
            .unwrap_or_else(|error| panic!("{source} should parse: {error:?}"));
        let chunk = redblue::bytecode::compile_program(&program)
            .unwrap_or_else(|error| panic!("{source} should compile: {error:?}"));

        let (mut walker, outcome) = redblue::run_isolated(&program);
        outcome.unwrap_or_else(|error| panic!("the walker refused `{source}`: {error:?}"));
        assert_eq!(
            walker.take_output(),
            vec![expected.to_string()],
            "the walker must answer `{source}` with `{expected}`"
        );

        let mut vm = BytecodeVm::new();
        vm.set_echo(false);
        vm.run(&chunk)
            .unwrap_or_else(|error| panic!("the bytecode VM refused `{source}`: {error:?}"));
        assert_eq!(
            vm.take_output(),
            vec![expected.to_string()],
            "the bytecode VM must answer `{source}` with `{expected}`"
        );
    }
}

/// A builtin given the wrong type is refused by **name**, on both engines, and
/// says nothing first.
///
/// The counterpart to the two tests above: a builtin that answered a wrong value
/// for the wrong type would satisfy "produced a line" and fail this, so the pair
/// together is "the right answer, or a named refusal — never anything else".
#[test]
fn edge_a_builtin_given_a_text_says_nothing_and_names_the_refusal() {
    for (source, name) in [
        ("say sqrt(\"nine\")", "sqrt"),
        ("say abs(\"three\")", "abs"),
        ("say text.split(1, \",\")", "split"),
    ] {
        let tokens = redblue::lexer::Lexer::tokenize(source)
            .unwrap_or_else(|error| panic!("{source} should lex: {error:?}"));
        let program = redblue::parser::parse(tokens)
            .unwrap_or_else(|error| panic!("{source} should parse: {error:?}"));
        let chunk = redblue::bytecode::compile_program(&program)
            .unwrap_or_else(|error| panic!("{source} should compile: {error:?}"));

        let (mut walker, walked) = redblue::run_isolated(&program);
        assert!(
            walked.is_err(),
            "`{source}` must be refused by the walker: {walked:?}"
        );
        let walker_error = walked.expect_err(&format!("`{source}` must be refused by the walker"));
        let said_by_walker = walker.take_output();
        assert!(
            said_by_walker.is_empty(),
            "`{source}` said {said_by_walker:?} before refusing"
        );

        let mut vm = BytecodeVm::new();
        vm.set_echo(false);
        let vm_result = vm.run(&chunk);
        assert!(
            vm_result.is_err(),
            "`{source}` must be refused by the bytecode VM: {vm_result:?}"
        );
        let vm_error =
            vm_result.expect_err(&format!("`{source}` must be refused by the bytecode VM"));
        let said_by_vm = vm.take_output();
        assert!(
            said_by_vm.is_empty(),
            "`{source}` said {said_by_vm:?} before refusing"
        );

        // Both engines refuse it, and both name it the same way: the refusal
        // says which builtin could not answer, not merely that something could
        // not.
        assert!(
            walker_error.message().contains(name),
            "`{source}` should name `{name}` in its refusal, got: {}",
            walker_error.message()
        );
        assert!(
            walker_error.message().contains("argument"),
            "`{source}` should say its arguments were the problem, got: {}",
            walker_error.message()
        );
        assert_eq!(
            vm_error.message(),
            walker_error.message(),
            "`{source}` must be refused the same way on both engines"
        );
    }
}

/// A builtin that cannot answer is refused **by the library**, and the refusal
/// is a `Result` a caller can act on — not a wrong answer and not a panic.
///
/// The test above asks the two VMs directly. This one asks
/// [`redblue::run_source`], the entry point an embedding application calls, and
/// requires three things of a bad argument:
///
/// 1. the call comes back `Err`, so a caller can branch on it;
/// 2. the error is a runtime error and names the builtin that could not answer,
///    so a reader of the message knows which one to look at;
/// 3. nothing was printed before the refusal, on either engine — a program that
///    printed a line and *then* failed has not answered the question.
///
/// Point 3 is the reachability point of this phase in the negative direction:
/// "it did not print the answer" and "it printed a wrong answer and then
/// complained" are the same observation to a test that only counts lines.
#[test]
fn edge_a_builtin_that_cannot_answer_is_refused_by_the_library_not_answered_wrongly() {
    for (source, name) in [
        ("say sqrt(\"nine\")", "sqrt"),
        ("say abs(\"three\")", "abs"),
        ("say text.split(1, \",\")", "split"),
        ("say nosuchfunction(1)", "nosuchfunction"),
    ] {
        let result = redblue::run_source(source);
        assert!(
            result.is_err(),
            "`{source}` must be refused, not answered: {result:?}"
        );
        let error = result.expect_err(&format!("`{source}` must be refused, not answered"));
        assert!(
            matches!(error, redblue::Error::Runtime(..)),
            "`{source}` should fail at runtime, got: {error:?}"
        );
        assert!(
            error.message().contains(name),
            "`{source}` should name `{name}` in its refusal, got: {}",
            error.message()
        );

        // The same program, asked of each engine directly, so the refusal is
        // read from the run rather than from the library's plumbing.
        let tokens = redblue::lexer::Lexer::tokenize(source)
            .unwrap_or_else(|error| panic!("{source} should lex: {error:?}"));
        let program = redblue::parser::parse(tokens)
            .unwrap_or_else(|error| panic!("{source} should parse: {error:?}"));
        let chunk = redblue::bytecode::compile_program(&program)
            .unwrap_or_else(|error| panic!("{source} should compile: {error:?}"));

        let (mut walker, walked) = redblue::run_isolated(&program);
        assert!(
            walked.is_err(),
            "`{source}` must be refused by the walker: {walked:?}"
        );
        let walker_error = walked.expect_err(&format!("`{source}` must be refused by the walker"));
        assert_eq!(
            walker_error.message(),
            error.message(),
            "`{source}` must be refused the same way by the library and by the walker"
        );
        let said_by_walker = walker.take_output();
        assert!(
            said_by_walker.is_empty(),
            "`{source}` said {said_by_walker:?} before refusing"
        );

        let mut vm = BytecodeVm::new();
        vm.set_echo(false);
        let vm_result = vm.run(&chunk);
        assert!(
            vm_result.is_err(),
            "`{source}` must be refused by the bytecode VM: {vm_result:?}"
        );
        let vm_error =
            vm_result.expect_err(&format!("`{source}` must be refused by the bytecode VM"));
        assert_eq!(
            vm_error.message(),
            error.message(),
            "`{source}` must be refused the same way by the library and by the bytecode VM"
        );
        let said_by_vm = vm.take_output();
        assert!(
            said_by_vm.is_empty(),
            "`{source}` said {said_by_vm:?} before refusing"
        );
    }
}

// ============================================================ the surface itself

/// A loopback HTTP server that answers `NETWORK_BODY` to two requests, and the
/// thread answering them.
///
/// `network_get` and `network_post` are the only two builtins whose answer is not
/// a function of their arguments, so they cannot be probed with a literal. They
/// are probed with a listener on `127.0.0.1:0` — an ephemeral port on the
/// loopback interface, from which no packet leaves the machine — which makes the
/// answer a constant the test owns instead of a thing it must not check.
///
/// `accept` is non-blocking with a deadline rather than blocking, so the thread
/// ends even if no program ever arrives.
fn loopback_http() -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    listener
        .set_nonblocking(true)
        .expect("the listener accepts without blocking");
    let address = format!(
        "http://{}",
        listener.local_addr().expect("the loopback address")
    );

    let handle = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        for _ in 0..2 {
            let mut stream = None;
            while std::time::Instant::now() < deadline {
                match listener.accept() {
                    Ok((accepted, _)) => {
                        stream = Some(accepted);
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(error) => panic!("the loopback listener failed: {error}"),
                }
            }
            let Some(mut stream) = stream else { break };
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .expect("a read deadline");

            let mut raw = Vec::new();
            let mut chunk = [0u8; 1024];
            let head_end = loop {
                if let Some(at) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
                    break at + 4;
                }
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break raw.len(),
                    Ok(read) => raw.extend_from_slice(&chunk[..read]),
                }
            };

            let head = String::from_utf8_lossy(&raw).into_owned();
            let length = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.trim()
                        .eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            let mut payload = raw[head_end.min(raw.len())..].to_vec();
            while payload.len() < length {
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => payload.extend_from_slice(&chunk[..read]),
                }
            }

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{NETWORK_BODY}",
                NETWORK_BODY.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("the response can be written");
            let _ = stream.flush();
            let _ = stream.shutdown(std::net::Shutdown::Write);
        }
    });

    (address, handle)
}

/// The body the loopback server answers, and the answer those two rows are
/// pinned against.
const NETWORK_BODY: &str = r#"{"ok":true}"#;

/// A working call for **every** name `stdlib::builtins()` registers as a
/// `Value::Builtin`, with the exact output the program has to print.
///
/// This table is the answer to the question `builtins() registers a name;
/// something has to answer it`, and it replaced a walk that asked the question
/// in a way almost nothing could fail. That walk ran `say <name>()` — no
/// arguments — and accepted any error that was not `Unknown function`, throwing
/// the exit status away with `let _ = ok;`. A builtin that refuses *every*
/// argument passed: `files_read()` is `files.read requires a text path`, which
/// is neither an unknown name nor a failure, so a `files.read` with no
/// implementation at all would have gone green. Nearly every row in the old walk
/// was that row.
///
/// So each name carries a program that *uses* it, and the output it must print.
/// A name whose program is refused fails; a name whose program answers something
/// else fails; and a name that is in neither this table nor [`known_gaps`] fails
/// the coverage assertion below, so the table cannot fall behind the registry
/// the way the skip-list did.
fn answerable_builtins(address: &str) -> Vec<(String, String, String)> {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/p32docs");
    std::fs::create_dir_all(&scratch).expect("scratch directory");
    // Forward slashes, always. The path is interpolated into Redblue source,
    // where a backslash starts an escape — so on Windows a `\t` in the path
    // became a tab and `\r` a carriage return, and `files.write` was handed a
    // filename with the separators eaten out of it. The Windows API takes `/`
    // everywhere, so this is the same path on every platform and the test does
    // not need a branch for it.
    let path = |name: &str| scratch.join(name).to_string_lossy().replace('\\', "/");

    let rows: Vec<(&str, String, String)> = vec![
        ("abs", "say abs(-3)".into(), "3\n".into()),
        ("floor", "say floor(1.7)".into(), "1\n".into()),
        ("ceil", "say ceil(1.2)".into(), "2\n".into()),
        ("round", "say round(1.5)".into(), "2\n".into()),
        ("sqrt", "say sqrt(9)".into(), "3\n".into()),
        ("pow", "say pow(2, 10)".into(), "1024\n".into()),
        // The trigonometric and logarithmic functions answer a number or nothing
        // at all, and `nothing` is a refusal by name — so the row asks whether
        // the answer is a number rather than pinning a value that would differ
        // by machine.
        ("sin", "say is_number(sin(0))".into(), "yes\n".into()),
        ("cos", "say is_number(cos(0))".into(), "yes\n".into()),
        ("tan", "say is_number(tan(0))".into(), "yes\n".into()),
        ("log", "say is_number(log(1))".into(), "yes\n".into()),
        ("exp", "say is_number(exp(1))".into(), "yes\n".into()),
        ("uppercase", r#"say uppercase("hi")"#.into(), "HI\n".into()),
        ("lowercase", r#"say lowercase("HI")"#.into(), "hi\n".into()),
        ("trim", r#"say trim("  x  ")"#.into(), "x\n".into()),
        (
            "split",
            r#"say split("a,b", ",")"#.into(),
            "[a, b]\n".into(),
        ),
        (
            "join",
            r#"say join(["a", "b"], "-")"#.into(),
            "a-b\n".into(),
        ),
        (
            "contains",
            r#"say contains("hello", "ell")"#.into(),
            "yes\n".into(),
        ),
        (
            "starts_with",
            r#"say starts_with("hello", "he")"#.into(),
            "yes\n".into(),
        ),
        (
            "ends_with",
            r#"say ends_with("hello", "lo")"#.into(),
            "yes\n".into(),
        ),
        (
            "replace",
            r#"say replace("a-b", "-", "+")"#.into(),
            "a+b\n".into(),
        ),
        (
            "length",
            r#"say length("abcd")
say length(bytes.from_text("héllo"))"#
                .into(),
            "4\n6\n".into(),
        ),
        // `push` answers the longer list rather than mutating the one it was
        // given — Redblue's lists are copy-on-write — so the row prints the
        // answer, not the binding it was handed.
        ("push", "say push([1], 2)".into(), "[1, 2]\n".into()),
        // `append` is the one builtin that takes a *name*: it has to bind.
        (
            "append",
            "set xs to [1]\nappend(\"xs\", 2)\nsay xs".into(),
            "[1, 2]\n".into(),
        ),
        ("pop", "say pop([1, 2, 3])".into(), "3\n".into()),
        ("shift", "say shift([1, 2, 3])".into(), "1\n".into()),
        (
            "map",
            "say map([1, 2], to (x) give back x + 1 end)".into(),
            "[2, 3]\n".into(),
        ),
        (
            "list_map",
            "say list.map([1, 2], to (x) give back x + 1 end)".into(),
            "[2, 3]\n".into(),
        ),
        (
            "chain",
            "say chain([1, 2], [3])".into(),
            "[1, 2, 3]\n".into(),
        ),
        (
            "zip",
            r#"say zip([1, 2], ["a", "b"])"#.into(),
            "[[1, a], [2, b]]\n".into(),
        ),
        (
            "enumerate",
            r#"say enumerate(["a", "b"])"#.into(),
            "[[0, a], [1, b]]\n".into(),
        ),
        (
            "product",
            r#"say product([1, 2], ["a"])"#.into(),
            "[[1, a], [2, a]]\n".into(),
        ),
        (
            "groupby",
            "say groupby([2, 1, 2, 1])".into(),
            "[{key: 2, items: [2, 2]}, {key: 1, items: [1, 1]}]\n".into(),
        ),
        ("take", "say take([1, 2, 3], 2)".into(), "[1, 2]\n".into()),
        ("skip", "say skip([1, 2, 3], 2)".into(), "[3]\n".into()),
        ("is_number", "say is_number(1)".into(), "yes\n".into()),
        ("is_text", r#"say is_text("a")"#.into(), "yes\n".into()),
        ("is_list", "say is_list([1])".into(), "yes\n".into()),
        ("is_record", "say is_record({a: 1})".into(), "yes\n".into()),
        ("to_text", "say to_text(1)".into(), "1\n".into()),
        ("to_number", r#"say to_number("2")"#.into(), "2\n".into()),
        ("to_list", r#"say to_list("a b")"#.into(), "[a, b]\n".into()),
        ("type_of", "say type_of(1)".into(), "number\n".into()),
        (
            "files_read",
            format!(
                r#"files.write("{}", "written")
say files.read("{}")"#,
                path("rows_read.txt"),
                path("rows_read.txt")
            ),
            "written\n".into(),
        ),
        (
            "files_write",
            format!(
                r#"files.write("{}", "a")
say files.exists("{}")"#,
                path("rows_write.txt"),
                path("rows_write.txt")
            ),
            "yes\n".into(),
        ),
        (
            "files_append",
            format!(
                r#"files.write("{}", "a")
files.append("{}", "b")
say files.read("{}")"#,
                path("rows_append.txt"),
                path("rows_append.txt"),
                path("rows_append.txt")
            ),
            "ab\n".into(),
        ),
        (
            "files_exists",
            format!(r#"say files.exists("{}")"#, path("rows_absent.txt")),
            "no\n".into(),
        ),
        (
            "files_lines",
            format!(
                r#"files.write("{}", "a\nb")
say files.lines("{}")"#,
                path("rows_lines.txt"),
                path("rows_lines.txt")
            ),
            "[a, b]\n".into(),
        ),
        (
            "files_delete",
            format!(
                r#"files.write("{}", "a")
files.delete("{}")
say files.exists("{}")"#,
                path("rows_delete.txt"),
                path("rows_delete.txt"),
                path("rows_delete.txt")
            ),
            "no\n".into(),
        ),
        (
            "files_copy",
            format!(
                r#"files.write("{}", "a")
files.copy("{}", "{}")
say files.exists("{}")"#,
                path("rows_copy.txt"),
                path("rows_copy.txt"),
                path("rows_copy_to.txt"),
                path("rows_copy_to.txt")
            ),
            "yes\n".into(),
        ),
        (
            "files_rename",
            format!(
                r#"files.write("{}", "a")
files.rename("{}", "{}")
say files.exists("{}")"#,
                path("rows_rename.txt"),
                path("rows_rename.txt"),
                path("rows_renamed.txt"),
                path("rows_renamed.txt")
            ),
            "yes\n".into(),
        ),
        // `time.now()` is a clock: the row asks what kind of answer it is, not
        // what the clock said.
        (
            "time_now",
            "say is_record(time.now())".into(),
            "yes\n".into(),
        ),
        (
            "time_sleep",
            r#"time.sleep(0)
say "slept""#
                .into(),
            "slept\n".into(),
        ),
        (
            "time_format",
            r#"say time.format(0, "%Y")"#.into(),
            "1970\n".into(),
        ),
        (
            "time_parse",
            r#"say to_text(time.parse("2024-02-29T23:59:58Z"))"#.into(),
            "2024-02-29 23:59:58\n".into(),
        ),
        (
            "time_unix",
            r#"say time.unix("2024-01-01 00:00:00")"#.into(),
            "1704067200\n".into(),
        ),
        (
            "time_from_unix",
            "say to_text(time.from_unix(1709164800))".into(),
            "2024-02-29 00:00:00\n".into(),
        ),
        (
            "time_date",
            "say to_text(time.date(2024, 2, 29))".into(),
            "2024-02-29 00:00:00\n".into(),
        ),
        (
            "time_datetime",
            "say to_text(time.datetime(2024, 2, 29, 13, 45, 59))".into(),
            "2024-02-29 13:45:59\n".into(),
        ),
        (
            "time_duration",
            "say to_text(time.duration(90000))".into(),
            "1 day, 1 hour\n".into(),
        ),
        (
            "time_add_years",
            "say to_text(time.add_years(time.date(2024, 2, 29), 1))".into(),
            "2025-02-28 00:00:00\n".into(),
        ),
        (
            "time_add_months",
            "say to_text(time.add_months(time.date(2024, 1, 31), 1))".into(),
            "2024-02-29 00:00:00\n".into(),
        ),
        (
            "time_add_days",
            "say to_text(time.add_days(time.date(2024, 1, 31), 1))".into(),
            "2024-02-01 00:00:00\n".into(),
        ),
        (
            "time_add_hours",
            "say to_text(time.add_hours(time.date(2024, 1, 31), 24))".into(),
            "2024-02-01 00:00:00\n".into(),
        ),
        (
            "time_add_minutes",
            "say to_text(time.add_minutes(time.date(2024, 1, 31), 60))".into(),
            "2024-01-31 01:00:00\n".into(),
        ),
        (
            "time_add_seconds",
            "say to_text(time.add_seconds(time.date(2024, 1, 31), 1))".into(),
            "2024-01-31 00:00:01\n".into(),
        ),
        (
            "json_parse",
            r#"say json.parse("{}")"#.into(),
            "{}\n".into(),
        ),
        (
            "json_stringify",
            "say json.stringify(1)".into(),
            "1\n".into(),
        ),
        (
            "csv_parse",
            r#"say csv.parse("a,b")"#.into(),
            "[[a, b]]\n".into(),
        ),
        (
            "network_get",
            format!(r#"say network.get("{address}/greeting")"#),
            format!("{NETWORK_BODY}\n"),
        ),
        (
            "network_post",
            format!(r#"say network.post("{address}/echo", "ping")"#),
            format!("{NETWORK_BODY}\n"),
        ),
        (
            "regex_match",
            r#"say regex.match("id=42", "id=([0-9]+)").groups[1]"#.into(),
            "42\n".into(),
        ),
        (
            "regex_is_match",
            r#"say regex.is_match("id=42", "^id")"#.into(),
            "yes\n".into(),
        ),
        (
            "regex_capture",
            r#"say regex.capture("id=42", "id=([0-9]+)", 1)"#.into(),
            "42\n".into(),
        ),
        (
            "regex_replace",
            r#"say regex.replace("id=42", "id=([0-9]+)", "user-$1")"#.into(),
            "user-42\n".into(),
        ),
        (
            "regex_split",
            r#"say regex.split("a1b22c", "[0-9]+")"#.into(),
            "[a, b, c]\n".into(),
        ),
        (
            "expect",
            "expect 1 to be 1\nsay \"expected\"".into(),
            "expected\n".into(),
        ),
        (
            "assert",
            "assert 1 is 1\nsay \"asserted\"".into(),
            "asserted\n".into(),
        ),
        ("console_log", r#"console.log("hi")"#.into(), "hi\n".into()),
        (
            "console_error",
            "console.error(\"e\")\nsay \"kept\"".into(),
            "kept\n".into(),
        ),
        (
            "console_clear",
            "console.clear()".into(),
            "\u{1b}[2J\u{1b}[1H".into(),
        ),
        // Seeding first makes each draw reproducible; what is pinned is the *kind*
        // of answer, because a whole number from 1 to 6 is not a constant.
        (
            "random",
            "random_seed(42)\nsay is_number(random(1, 6))".into(),
            "yes\n".into(),
        ),
        (
            "random_seed",
            "random_seed(42)\nsay \"seeded\"".into(),
            "seeded\n".into(),
        ),
        (
            "random_number",
            "random_seed(42)\nsay is_number(random_number(0, 1))".into(),
            "yes\n".into(),
        ),
        (
            "random_choice",
            "say random_choice([1])".into(),
            "1\n".into(),
        ),
        (
            "random_shuffle",
            "say length(random_shuffle([1, 2]))".into(),
            "2\n".into(),
        ),
        (
            "bytes_from_text",
            r#"say bytes.from_text("hi")"#.into(),
            "[104, 105]\n".into(),
        ),
        (
            "bytes_write",
            format!(
                r#"bytes.write("{}", [0, 255])
say files.exists("{}")"#,
                path("rows_write.bin"),
                path("rows_write.bin")
            ),
            "yes\n".into(),
        ),
        (
            "bytes_text",
            "say bytes.text([104, 105])".into(),
            "hi\n".into(),
        ),
        ("sys_argv", "say length(sys.argv())".into(), "0\n".into()),
        // Standard input is closed for every child `run_named` starts, so this
        // row reads end-of-input and gets the empty text. That is an answer, and
        // answering is the claim: the alternative — refusing — would be a
        // `RuntimeError` and the row would fail.
        ("input", "say length(input())".into(), "0\n".into()),
    ];

    rows.into_iter()
        .map(|(name, source, expected)| (name.to_string(), source, expected))
        .collect()
}

/// Names `builtins()` registers that no program can call today, each with a
/// program that is the shape a caller would write.
///
/// The second half is the point. A name alone in a skip-list is a permission to
/// stop checking that never expires on its own, and this one did not expire:
/// eleven of its entries — `pop`, `shift`, `is_number`, `is_text`, `is_list`,
/// `is_record`, `to_list`, `csv_parse`, `network_get`, `network_post` and
/// `list_map` — had been implemented in `src/stdlib.rs` and were still listed,
/// while `edge_every_flat_builtin_the_stdlib_registers_answers` above was
/// asserting several of them by name. Carrying the program means
/// `edge_a_known_gap_is_still_a_gap` can run it, so an entry that starts
/// answering fails here instead of sitting in a list that reads as current.
///
/// Each is a higher-order builtin: it has to call a Redblue function, which
/// needs a walker's captured scopes. `map` has them, and `list.map` is the same
/// builtin under its module spelling — which is why `list_map` left this list
/// when it was found answering.
fn known_gaps() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "filter",
            "say filter([1, 2, 3], to (x) give back x > 1 end)",
        ),
        (
            "reduce",
            "say reduce([1, 2, 3], to (a, b) give back a + b end, 0)",
        ),
    ]
}

/// `builtins()` registers a name; something has to answer it.
///
/// The names are reconciled with the registry in **both** directions before any
/// of them is run: a name in the registry that appears in neither
/// [`answerable_builtins`] nor [`known_gaps`] is a builtin nothing checks, and a
/// row for a name the registry does not hold is a test of a name that is gone.
/// That is the property the old skip-list version could not have: adding a
/// builtin to `stdlib::builtins()` without a program here fails immediately.
#[test]
fn edge_no_registered_builtin_is_unreachable_from_a_program() {
    let (address, server) = loopback_http();
    let rows = answerable_builtins(&address);

    let registry = redblue::stdlib::builtins();
    let registered: Vec<&str> = registry
        .iter()
        .filter_map(|(name, value)| match value {
            redblue::Value::Builtin(_) => Some(name.as_str()),
            _ => None,
        })
        .collect();

    for (name, _) in known_gaps() {
        assert!(
            redblue::stdlib::is_builtin(name),
            "KNOWN_GAPS lists `{name}` and stdlib::is_builtin does not know it, so \
             the skip has nothing to skip: either the builtin was renamed or the \
             entry is left over from a name that is gone."
        );
    }

    for name in &registered {
        let answered = rows.iter().any(|(row, _, _)| row == name);
        let skipped = known_gaps().iter().any(|(gap, _)| gap == name);
        assert!(
            answered || skipped,
            "`{name}` is registered in stdlib::builtins and nothing checks it: it \
             is neither in answerable_builtins() nor in known_gaps(). Add a program \
             it answers — with the output it must print — or, if it is deliberately \
             unimplemented, add it to known_gaps() with the program that still fails."
        );
    }

    for (name, _, _) in &rows {
        assert!(
            registered.contains(&name.as_str()),
            "answerable_builtins() has a row for `{name}` and stdlib::builtins does \
             not register it, so the table is testing a name that is gone. Drop the \
             row, or register the builtin."
        );
    }

    for (name, source, expected) in &rows {
        assert!(
            !expected.is_empty(),
            "the row for `{name}` pins no output, so a builtin that answers something \
             other than nothing still passes it. Every row carries the exact stdout \
             the program has to print; there is no 'no expected output' case left."
        );
        let (ok, stdout, stderr) = run_named(&format!("surface_{name}"), source);
        assert!(
            ok,
            "`{name}` is registered in stdlib::builtins and its program was refused:\n\
             {source}\n--- stderr\n{stderr}\nEither implement it, or add it to \
             known_gaps() in this file with the program that still fails."
        );
        assert_eq!(
            stdout, *expected,
            "`{name}` answered something else than the output it is pinned to:\n\
             {source}\n--- stdout\n{stdout}--- stderr\n{stderr}"
        );
    }

    server.join().expect("the loopback server thread");
}

/// Every name in [`known_gaps`] is still a gap.
///
/// Without this the skip-list is a claim rather than a check: `edge_no_registered_
/// builtin_is_unreachable_from_a_program` skips whatever it names and never asks
/// again, so an entry that begins answering is invisible. This runs each entry's
/// own program and fails if it succeeds — which is the moment the entry has to
/// come out of the list and into the table above.
///
/// The eleven names that came out of this list were not found answering during
/// this phase — they had been answering for some time, and the list had simply
/// stopped asking. Putting one back is the check: `("pop", "say pop([1, 2, 3])")`
/// added to [`known_gaps`] answers `3` and fails here with *"`pop` is listed as
/// unreachable in KNOWN_GAPS and answered"*.
#[test]
fn edge_a_known_gap_is_still_a_gap() {
    for (name, source) in known_gaps() {
        let (ok, _, stderr) = run_named(&format!("gap_{name}"), source);
        assert!(
            !ok,
            "`{name}` is listed as unreachable in KNOWN_GAPS and answered:\n{source}\n\
             It is implemented, so it must come out of the list rather than go on \
             skipping a builtin that works."
        );
        assert!(
            stderr.contains(name),
            "the refusal does not name '{name}', so it cannot say which call was \
             wrong:\n{source}\n--- stderr\n{stderr}"
        );
        assert!(
            !stderr.contains("Unknown function"),
            "`{name}` was dropped rather than implemented — nothing implements it \
             any more, and its entry in stdlib::builtins should go with it.\n\
             --- stderr\n{stderr}"
        );
    }
}

// ============================================================ regex spelling

/// `regex` was the rank-2 gap in `SURVEY.md`: the crate was linked and no
/// program could reach it. A match is now a record, and the record is ordinary
/// data — `groups[0]` is the whole match and `groups[1]` the first capture.
#[test]
fn edge_regex_module_answers_a_match_record_and_its_captures() {
    says(
        "docs_regex_match",
        r#"say regex.match("id=42", "id=([0-9]+)").groups[1]"#,
        "42",
    );
    says(
        "docs_regex_whole_match",
        r#"say regex.match("id=42", "id=([0-9]+)").groups[0]"#,
        "id=42",
    );
    says(
        "docs_regex_start",
        r#"say regex.match("id=42", "([0-9]+)").start"#,
        "3",
    );
}

/// The documented boolean and replacement spellings, and the separators that
/// are not reserved words. `regex.is_match` is the name because `test` belongs
/// to the test grammar, and `finish` is the field because `end` closes a block.
#[test]
fn edge_regex_module_answers_is_match_replace_and_split() {
    says(
        "docs_regex_is_match",
        r#"say regex.is_match("id=42", "^id")"#,
        "yes",
    );
    says(
        "docs_regex_replace",
        r#"say regex.replace("id=42", "id=([0-9]+)", "user-$1")"#,
        "user-42",
    );
    says(
        "docs_regex_split",
        r#"say regex.split("a1b22c", "[0-9]+")"#,
        "[a, b, c]",
    );
}

/// A program that asks regex for a pattern the crate cannot compile gets a
/// refusal that names the module, not `Unknown function` and not a panic.
#[test]
fn edge_regex_module_refuses_a_bad_pattern_by_name() {
    refuses(
        "docs_regex_bad_pattern",
        r#"say regex.match("x", "(")"#,
        "regex",
    );
}

// ============================================================ phase-098 iteration helpers

/// The tree-walking engine's value for `source`.
#[track_caller]
fn tree(source: &str) -> Value {
    run_source_value(source)
        .unwrap_or_else(|error| panic!("tree-walker should run:\n{source}\n{error:?}"))
}

/// The bytecode engine's value for `source`.
#[track_caller]
fn bytecode(source: &str) -> Value {
    let chunk = compile_source(source)
        .unwrap_or_else(|error| panic!("source should compile:\n{source}\n{error:?}"));
    let mut vm = BytecodeVm::new();
    vm.run(&chunk)
        .unwrap_or_else(|error| panic!("bytecode VM should run:\n{source}\n{error:?}"))
}

/// Runs `source` on both engines and asserts they answer the same text, then
/// returns the tree-walker's value.
#[track_caller]
fn both(source: &str) -> Value {
    let walked = tree(source);
    let compiled = bytecode(source);
    assert_eq!(
        walked.to_string(),
        compiled.to_string(),
        "the two engines disagree about:\n{source}"
    );
    walked
}

/// Asserts `source` answers `expected` on both engines.
#[track_caller]
fn yields(source: &str, expected: &str) {
    let value = both(source);
    assert_eq!(value.to_string(), expected, "wrong answer for:\n{source}");
}

/// The failure `source` produces on the tree-walker.
#[track_caller]
fn fails(source: &str) -> Error {
    run_source_value(source).expect_err("source should fail")
}

fn message(error: &Error) -> String {
    error.message().to_string()
}

#[test]
fn iteration_helpers_cover_the_empty_list() {
    yields("chain([], [])", "[]");
    yields("zip([], [])", "[]");
    yields("zip([1], [])", "[]");
    yields("zip([], [1])", "[]");
    yields("enumerate([])", "[]");
    yields("product([], [])", "[]");
    yields("product([1], [])", "[]");
    yields("groupby([])", "[]");
    yields("take([], 3)", "[]");
    yields("skip([], 3)", "[]");
}

#[test]
fn iteration_helpers_cover_singletons_and_many_elements() {
    yields("chain([1], [2])", "[1, 2]");
    yields("zip([1], [2])", "[[1, 2]]");
    yields("enumerate([9])", "[[0, 9]]");
    yields("product([1], [2])", "[[1, 2]]");
    yields("groupby([7])", "[{key: 7, items: [7]}]");
    yields("take([7], 1)", "[7]");
    yields("skip([7], 0)", "[7]");

    yields("chain([1, 2], [3, 4])", "[1, 2, 3, 4]");
    yields("zip([1, 2, 3], [4, 5, 6])", "[[1, 4], [2, 5], [3, 6]]");
    yields("enumerate([\"a\", \"b\"])", "[[0, a], [1, b]]");
    yields(
        "product([1, 2], [3, 4])",
        "[[1, 3], [1, 4], [2, 3], [2, 4]]",
    );
    yields("take([1, 2, 3], 2)", "[1, 2]");
    yields("skip([1, 2, 3], 1)", "[2, 3]");
}

#[test]
fn edge_zip_stops_at_the_shorter_input() {
    yields("zip([1, 2, 3], [\"a\"])", "[[1, a]]");
    yields("zip([1], [\"a\", \"b\", \"c\"])", "[[1, a]]");
    // The rule is not padding with a missing value, so the answer is exactly
    // as long as the shorter side and no longer.
    assert_eq!(
        tree("length(zip([1, 2, 3, 4], [\"a\", \"b\"]))").to_string(),
        "2"
    );
}

#[test]
fn edge_groupby_keeps_first_appearance_order_and_groups_by_equality() {
    // 2 appears first, so its group is first even though 1 is smaller. The
    // two lists equal to ["a"] share a group instead of making separate ones.
    yields(
        "groupby([2, 1, 2, 1, 3])",
        "[{key: 2, items: [2, 2]}, {key: 1, items: [1, 1]}, {key: 3, items: [3]}]",
    );
    yields(
        "groupby([[\"a\"], [\"b\"], [\"a\"]])",
        "[{key: [a], items: [[a], [a]]}, {key: [b], items: [[b]]}]",
    );
}

#[test]
fn edge_take_and_skip_clamp_at_both_ends() {
    yields("take([1, 2, 3], 0)", "[]");
    yields("take([1, 2, 3], -2)", "[]");
    yields("take([1, 2, 3], 99)", "[1, 2, 3]");
    yields("skip([1, 2, 3], 0)", "[1, 2, 3]");
    yields("skip([1, 2, 3], -2)", "[1, 2, 3]");
    yields("skip([1, 2, 3], 99)", "[]");
}

/// A count is a number of whole items. A positive fraction has no meaning, so
/// it is refused rather than silently truncated: `take(xs, 1.9)` used to answer
/// one item, which makes the caller's arithmetic disappear without a word. A
/// negative count is still the empty/whole-list clamp, fraction or not.
#[test]
fn edge_take_and_skip_refuse_a_fractional_count() {
    for (source, name) in [
        ("take([1, 2, 3], 1.5)", "take"),
        ("skip([1, 2, 3], 2.5)", "skip"),
    ] {
        let walked = message(&fails(source));
        assert!(
            walked.contains(name) && walked.contains("whole number"),
            "the tree-walker should refuse `{source}` as a whole number, it said: {walked}"
        );

        let chunk = compile_source(source).expect("the failing program should compile");
        let mut vm = BytecodeVm::new();
        let compiled = vm
            .run(&chunk)
            .expect_err("the bytecode VM should refuse a fractional count");
        assert!(
            compiled.message().contains(name) && compiled.message().contains("whole number"),
            "the bytecode VM should refuse `{source}` as a whole number, it said: {}",
            compiled.message()
        );
    }

    // The negative clamp still wins over the whole-number rule, so the two
    // edges stay separate rather than one swallowing the other.
    yields("take([1, 2, 3], -1.5)", "[]");
    yields("skip([1, 2, 3], -1.5)", "[1, 2, 3]");
}

/// The helpers move values without inspecting them, so text of any script has
/// to survive unchanged. A byte-length or grapheme rewrite anywhere on the path
/// would mangle an emoji, a CJK word, an RTL word or a combining mark.
#[test]
fn edge_iteration_helpers_preserve_unicode_items() {
    yields("chain([\"日\", \"é\"], [\"🙂\"])", "[日, é, 🙂]");
    yields("zip([\"日\"], [\"é\"])", "[[日, é]]");
    yields("enumerate([\"🙂\", \"שלום\"])", "[[0, 🙂], [1, שלום]]");
    yields("product([\"é\"], [\"🙂\"])", "[[é, 🙂]]");
    yields(
        "groupby([\"日\", \"é\", \"日\"])",
        "[{key: 日, items: [日, 日]}, {key: é, items: [é]}]",
    );
    yields("take([\"日\", \"é\"], 1)", "[日]");
    yields("skip([\"日\", \"é\"], 1)", "[é]");
    yields("chain([], [\"e\u{301}\"])", "[e\u{301}]");
}

/// A key is compared with Redblue equality, so two records with the same fields
/// and values share a group even when they were built separately, while a
/// record with a different or missing field stays its own group.
#[test]
fn edge_groupby_groups_equal_records_and_keeps_missing_fields_distinct() {
    yields(
        "groupby([{a: 1}, {a: 2}, {a: 1}])",
        "[{key: {a: 1}, items: [{a: 1}, {a: 1}]}, {key: {a: 2}, items: [{a: 2}]}]",
    );
    yields(
        "groupby([{a: 1}, {b: 1}, {}])",
        "[{key: {a: 1}, items: [{a: 1}]}, {key: {b: 1}, items: [{b: 1}]}, \
         {key: {}, items: [{}]}]",
    );
}

/// Two list arguments with equal contents are one cache key, and two records
/// with equal fields are one key too; a different list or a record missing a
/// field is a different key. The call counts are literal, so identity-only or
/// never-hitting keys both fail.
#[test]
fn edge_lru_cache_keys_on_structural_argument_equality() {
    let source = r#"
set list_calls to 0
lru_cache(10) to same_list(xs)
    set list_calls to list_calls + 1
    give back xs
end
same_list([1, 2])
same_list([1, 2])
same_list([2, 1])
set record_calls to 0
lru_cache(10) to same_record(r)
    set record_calls to record_calls + 1
    give back r
end
same_record({a: 1, b: 2})
same_record({a: 1, b: 2})
same_record({a: 1})
[list_calls, record_calls]
"#;
    yields(source, "[2, 2]");
}

#[test]
fn edge_iteration_helpers_refuse_wrong_types() {
    for (source, name) in [
        ("chain([1], \"two\")", "chain"),
        ("zip([1], 2)", "zip"),
        ("enumerate(\"text\")", "enumerate"),
        ("product([1], {a: 1})", "product"),
        ("groupby(1)", "groupby"),
        ("take(\"text\", 1)", "take"),
        ("skip([1], \"one\")", "skip"),
    ] {
        let text = message(&fails(source));
        assert!(
            text.contains(name),
            "the refusal for `{source}` should name `{name}`, it said: {text}"
        );
    }
}

/// Two Fibonacci functions, one cached and one not: the same result, fewer
/// calls for the cached one. The call counts are asserted literally, so a cache
/// that never hits fails instead of passing on the result alone.
#[test]
fn lru_cache_makes_fewer_recursive_calls_than_uncached() {
    let source = r#"
set cached_calls to 0
lru_cache(50) to cached_fib(n)
    set cached_calls to cached_calls + 1
    set answer to n
    if n >= 2 then
        set answer to cached_fib(n - 1) + cached_fib(n - 2)
    end
    give back answer
end
set plain_calls to 0
to plain_fib(n)
    set plain_calls to plain_calls + 1
    set answer to n
    if n >= 2 then
        set answer to plain_fib(n - 1) + plain_fib(n - 2)
    end
    give back answer
end
set cached to cached_fib(10)
set plain to plain_fib(10)
[cached_calls < plain_calls, cached is plain, cached_calls, plain_calls]
"#;
    yields(source, "[yes, yes, 11, 177]");
}

/// At capacity two, the sequence visits three distinct arguments and then the
/// first one again. With no eviction the last call would be a hit and the count
/// would be 3; the answer 4 is the proof that the entry was evicted.
#[test]
fn edge_lru_cache_evicts_at_its_capacity() {
    let source = r#"
set calls to 0
lru_cache(2) to f(n)
    set calls to calls + 1
    give back n
end
f(1)
f(1)
f(2)
f(3)
f(1)
calls
"#;
    yields(source, "4");
}

#[test]
fn edge_lru_cache_requires_a_positive_capacity() {
    let zero = message(&fails("lru_cache(0) to f(n)\n    give back n\nend\nf(1)\n"));
    assert!(
        zero.contains("at least 1"),
        "a zero capacity should be refused, it said: {zero}"
    );

    let fractional = message(&fails(
        "lru_cache(1.5) to f(n)\n    give back n\nend\nf(1)\n",
    ));
    assert!(
        fractional.contains("whole number"),
        "a fractional capacity should be refused, it said: {fractional}"
    );

    let non_numeric = message(&fails(
        "lru_cache(\"x\") to f(n)\n    give back n\nend\nf(1)\n",
    ));
    assert!(
        non_numeric.contains("must be a number"),
        "a non-numeric capacity should be refused, it said: {non_numeric}"
    );
}

#[test]
fn lru_cache_defaults_to_a_capacity_of_128() {
    // 128 distinct arguments fit; the 129th evicts the least recently used
    // entry. Then asking for the first argument again is a miss, which is the
    // proof that the omitted capacity really was 128 and not unbounded.
    let source = r#"
set calls to 0
lru_cache to f(n)
    set calls to calls + 1
    give back n
end
for each i from 1 to 128
    f(i)
end
set filled to calls
f(129)
f(1)
[filled, calls]
"#;
    yields(source, "[128, 130]");
}

#[test]
fn edge_lru_cache_memoises_a_body_that_ends_in_an_expression() {
    // The body's last statement is an ordinary expression rather than `give
    // back`; the compiled engine finishes such a frame in `unwind_frame`, which
    // must record the answer too or only one engine memoises.
    let source = r#"
set calls to 0
lru_cache(10) to f(n)
    set calls to calls + 1
    n
end
f(1)
f(1)
calls
"#;
    yields(source, "1");
}

#[test]
fn edge_redeclaring_a_cached_function_undecorated_drops_its_memory() {
    // A cache belongs to the declaration that asked for it. Rebinding the name
    // with an ordinary `to` must not let the old declaration answer for the new
    // one on either engine.
    let source = r#"
lru_cache(10) to f(n)
    give back n
end
f(1)
to f(n)
    give back n + 100
end
f(1)
"#;
    yields(source, "101");
}

#[test]
fn edge_lru_cache_does_not_survive_a_bytecode_run() {
    let mut vm = BytecodeVm::new();
    let first = compile_source(
        "lru_cache(10) to f(n)\n\
             say \"called\"\n\
             give back n\n\
         end\n\
         f(1)",
    )
    .expect("the first program should compile");
    assert_eq!(
        vm.run(&first)
            .expect("the first run should work")
            .to_string(),
        "1"
    );
    assert_eq!(vm.take_output(), vec!["called".to_string()]);

    let second = compile_source("f(1)").expect("the second program should compile");
    assert_eq!(
        vm.run(&second)
            .expect("the second run should work")
            .to_string(),
        "1"
    );
    assert_eq!(
        vm.take_output(),
        vec!["called".to_string()],
        "a cache from the previous run must not skip this run's body"
    );
}

#[test]
fn edge_a_user_function_named_like_the_internal_cache_call_still_runs() {
    // The internal declaration call must be a name no source can spell, or a
    // legal identifier would run the user's body on one engine and be
    // intercepted on the other.
    let source =
        "to __lru_cache_declare(a, b)\n    give back a + b\nend\n__lru_cache_declare(2, 3)\n";
    yields(source, "5");
}

#[test]
fn edge_a_variable_named_lru_cache_still_works() {
    // `lru_cache` is not reserved: a variable of that name is an ordinary
    // binding, and only the decorator shape in front of `to` changes meaning.
    yields("set lru_cache to 5\nlru_cache + 1", "6");
}

#[test]
fn iteration_helpers_agree_across_both_engines() {
    for source in [
        "chain([1, 2], [3])",
        "zip([1, 2, 3], [4, 5])",
        "enumerate([\"a\", \"b\", \"c\"])",
        "product([1, 2], [3, 4])",
        "groupby([3, 1, 3, 2, 1])",
        "take([1, 2, 3], 2)",
        "skip([1, 2, 3], 1)",
    ] {
        both(source);
    }
}

/// SPEC.md is the language's contract, so the laziness statement has to be
/// there and has to name each helper. If a helper is later made lazy, this
/// fails until the document says so — which is the point of writing the answer
/// down rather than leaving it to be inferred.
#[test]
fn spec_says_each_iteration_helper_materialises() {
    let spec = include_str!("../SPEC.md");
    for name in [
        "chain",
        "zip",
        "enumerate",
        "product",
        "groupby",
        "take",
        "skip",
    ] {
        let needle = format!("| `{name}(");
        let line = spec
            .lines()
            .find(|line| line.contains(&needle))
            .unwrap_or_else(|| panic!("SPEC.md § Iteration helpers has no row for `{name}`"));
        assert!(
            line.contains("materialis"),
            "the row for `{name}` should say it materialises: {line}"
        );
    }
    assert!(
        spec.contains("zip") && spec.contains("stops at the shorter"),
        "SPEC.md should state that zip stops at the shorter input"
    );
}

/// A cache is a property of the declaration, not of one way of calling it. A
/// decorated function passed to `map` is still the same function, so repeated
/// arguments must be answered from memory on both engines. The bytecode VM
/// looked for a hit only on a direct `CALL`, so `map([1, 1], f)` ran the body
/// twice there while the tree-walker ran it once.
#[test]
fn edge_lru_cache_hits_inside_map_on_both_engines() {
    let source = r#"
set calls to 0
lru_cache(10) to doubled(n)
    set calls to calls + 1
    give back n * 2
end
set mapped to map([1, 1, 1], doubled)
[mapped, calls]
"#;
    yields(source, "[[2, 2, 2], 1]");
}

/// A `map` over one repeated value is all cache hits after the first element.
/// The bytecode VM used to route every hit through the next element's call, so
/// a large list recursed once per element in Rust and overflowed the host stack
/// instead of finishing. The count is literal so a map that stopped early — or
/// stopped answering from memory — is caught too.
#[test]
fn edge_lru_cache_map_of_a_large_repeated_value_does_not_recurse() {
    let count = 100_000;
    let repeated = std::iter::repeat_n("1", count)
        .collect::<Vec<_>>()
        .join(" ");
    let source = format!(
        "set calls to 0\n\
         lru_cache(1) to once(n)\n\
             set calls to calls + 1\n\
             give back n\n\
         end\n\
         set mapped to map(text.split(\"{repeated}\", \" \"), once)\n\
         [length(mapped), calls]\n"
    );
    yields(&source, &format!("[{count}, 1]"));
}

/// Cached declarations inside a loop create a new function value every turn.
/// A closure that escapes keeps its own memory, while the turns that did not
/// escape must not leave their caches behind. The call count is the observable
/// half: three escaped closures answer their second call from memory.
#[test]
fn edge_lru_cache_in_a_loop_keeps_only_escaped_closures_memory() {
    let source = r#"
set calls to 0
set saved to []
for each turn from 1 to 3
    lru_cache(10) to add(n)
        set calls to calls + 1
        give back n
    end
    set saved to push(saved, add)
end
set first to saved[0]
set second to saved[1]
set third to saved[2]
[first(1), first(1), second(1), second(1), third(1), third(1), calls]
"#;
    yields(source, "[1, 1, 1, 1, 1, 1, 3]");
}

/// A failed call is not an answer, so it must not enter the cache. Both engines
/// are asked directly for the failure first, then each attempt is caught and
/// counted: the body runs on every attempt, which is the observable proof that
/// nothing was remembered.
#[test]
fn edge_lru_cache_does_not_record_a_failed_call() {
    let bad = "lru_cache(2) to boom(n)\n    give back 1 / n\nend\nboom(0)\n";
    assert!(
        run_source_value(bad).is_err(),
        "the tree-walker should refuse a division by zero"
    );
    let chunk = compile_source(bad).expect("the failing program should compile");
    let mut vm = BytecodeVm::new();
    assert!(
        vm.run(&chunk).is_err(),
        "the bytecode VM should refuse a division by zero"
    );

    let source = r#"
set calls to 0
lru_cache(2) to boom(n)
    set calls to calls + 1
    give back 1 / n
end
try
    boom(0)
catch error
    set caught to yes
end
try
    boom(0)
catch error
    set caught to yes
end
calls
"#;
    yields(source, "2");
}

/// A count far past the end is still the whole list (or nothing, for `skip`),
/// not an overflow. The cast from the finite number saturates instead of
/// wrapping to a small count, so both ends keep the clamping rule the ordinary
/// bounds test states.
#[test]
fn edge_take_and_skip_saturate_a_count_far_past_the_end() {
    yields("take([1, 2, 3], 1e300)", "[1, 2, 3]");
    yields("skip([1, 2, 3], 1e300)", "[]");
    yields("take([1, 2, 3], -1e300)", "[]");
    yields("skip([1, 2, 3], -1e300)", "[1, 2, 3]");
}

/// A cache belongs to one function value, not to every declaration that shares
/// its name. Two calls of one factory build two `add` closures over different
/// offsets; a name-keyed memory answered the second closure with the first
/// closure's result.
#[test]
fn edge_lru_cache_is_not_shared_by_two_same_named_closures() {
    let source = r#"
to make(offset)
    lru_cache(10) to add(n)
        give back n + offset
    end
    give back add
end
set hundreds to make(100)
set ones to make(1)
[hundreds(5), hundreds(5), ones(5), ones(5)]
"#;
    yields(source, "[105, 105, 6, 6]");
}

/// The same collision, observed through the call count: each closure's first
/// call runs its body and its second call is a hit of *its own* memory. A shared
/// cache answers the second closure with the first one's label and count.
#[test]
fn edge_lru_cache_same_named_closures_fill_independently() {
    let source = r#"
to make(label, calls)
    lru_cache(10) to remember(n)
        set calls to calls + 1
        give back [label, calls]
    end
    give back remember
end
set first to make("first", 0)
set second to make("second", 0)
[first(1), first(1), second(1), second(1)]
"#;
    yields(source, "[[first, 1], [first, 1], [second, 1], [second, 1]]");
}

/// A cached value in one closure must never hide a failure in another closure
/// of the same name. The first declaration caches `n / 1`; the second divides by
/// zero, which is an error even though its argument equals the cached one.
#[test]
fn edge_lru_cache_does_not_let_one_closure_answer_for_another() {
    let failing = r#"
to make(divisor)
    lru_cache(10) to divide(n)
        give back n / divisor
    end
    give back divide
end
set safe to make(1)
set broken to make(0)
safe(7)
broken(7)
"#;
    assert!(
        run_source_value(failing).is_err(),
        "the tree-walker should refuse the second closure's division by zero"
    );

    let chunk = compile_source(failing).expect("the failing program should compile");
    let mut vm = BytecodeVm::new();
    assert!(
        vm.run(&chunk).is_err(),
        "the bytecode VM should refuse the second closure's division by zero"
    );

    let caught = r#"
to make(divisor)
    lru_cache(10) to divide(n)
        give back n / divisor
    end
    give back divide
end
set safe to make(1)
set broken to make(0)
set safe_answer to safe(7)
try
    set broken_answer to broken(7)
catch error
    set broken_answer to "refused"
end
[safe_answer, broken_answer]
"#;
    yields(caught, "[7, refused]");
}

/// A cache belongs to the closure value, so declaring a same-named closure in
/// a sibling call must not take the first closure's memory with it. The count
/// is the observable half: both closures compute once and answer from memory
/// after that, on both engines.
#[test]
fn edge_lru_cache_survives_sibling_closure_declarations() {
    let source = r#"
set calls to 0
to factory(offset)
    lru_cache(2) to add(n)
        set calls to calls + 1
        give back n + offset
    end
    give back add
end
set first to factory(10)
set second to factory(20)
[first(1), first(1), second(1), second(1), calls]
"#;
    yields(source, "[11, 11, 21, 21, 2]");
}

/// A hit is a use, so it refreshes the entry's place in the LRU order. With a
/// capacity of two, reading `1` again after adding `2` must make `2` the
/// eviction victim; the fourth call then recomputes `2` and the count says so.
#[test]
fn edge_lru_cache_hit_refreshes_recency() {
    let source = r#"
set calls to 0
lru_cache(2) to track(n)
    set calls to calls + 1
    give back n
end
[track(1), track(2), track(1), track(3), track(2), calls]
"#;
    yields(source, "[1, 2, 1, 3, 2, 4]");
}

/// A capacity beyond `usize::MAX` cannot index a cache, so the parser refuses
/// it instead of saturating the cast to a size the declaration never stated.
/// This is the numeric boundary of the decorator's capacity.
#[test]
fn edge_lru_cache_refuses_a_capacity_beyond_a_count_it_can_hold() {
    let too_large_source = "lru_cache(1e300) to f(n)
    give back n
end
f(1)
";
    assert!(
        run_source_value(too_large_source).is_err(),
        "the tree-walker should refuse a capacity past usize::MAX"
    );
    assert!(
        compile_source(too_large_source).is_err(),
        "the compiler should refuse a capacity past usize::MAX"
    );
    let too_large = message(&fails(too_large_source));
    assert!(
        too_large.contains("at least 1") && too_large.contains("whole number"),
        "a capacity past usize::MAX should be refused, it said: {too_large}"
    );

    let negative = message(&fails(
        "lru_cache(-1) to f(n)
    give back n
end
f(1)
",
    ));
    assert!(
        negative.contains("lru_cache capacity"),
        "a negative capacity should be refused by name, it said: {negative}"
    );
}

/// SPEC.md must say what a function argument means as a cache key, because the
/// answer is deliberately different from the language's `is`.
#[test]
fn spec_says_a_cache_matches_function_arguments_by_identity() {
    let spec = include_str!("../SPEC.md");
    assert!(
        spec.contains("matched by identity"),
        "SPEC.md § Memoisation should say a function argument is matched by identity"
    );
}

/// `nothing` is a value the helpers cannot use as either a list or a count.
/// Each refusal names the helper rather than surfacing as `Unknown function` or
/// a panic, which is the same channel the wrong-type test uses for other kinds.
#[test]
fn edge_iteration_helpers_refuse_nothing() {
    for (source, name) in [
        ("chain(nothing, [1])", "chain"),
        ("zip([1], nothing)", "zip"),
        ("enumerate(nothing)", "enumerate"),
        ("product(nothing, [1])", "product"),
        ("groupby(nothing)", "groupby"),
        ("take([1], nothing)", "take"),
        ("skip(nothing, 1)", "skip"),
    ] {
        let text = message(&fails(source));
        assert!(
            text.contains(name),
            "the refusal for `{source}` should name `{name}`, it said: {text}"
        );
    }
}

/// Both helpers name what they cannot take, and neither engine truncates a
/// text into a count or panics on it. This is the failure channel for the
/// take/skip argument check.
#[test]
fn edge_take_and_skip_refuse_a_text_count() {
    for source in ["take([1, 2, 3], \"2\")", "skip([1, 2, 3], \"2\")"] {
        let walked = message(&fails(source));
        assert!(
            walked.contains("requires a list and a number"),
            "the tree-walker should refuse `{source}` by name, it said: {walked}"
        );

        let chunk = compile_source(source).expect("the failing program should compile");
        let mut vm = BytecodeVm::new();
        let compiled = vm
            .run(&chunk)
            .expect_err("the bytecode VM should refuse a text count");
        assert!(
            compiled.message().contains("requires a list and a number"),
            "the bytecode VM should refuse `{source}` by name, it said: {}",
            compiled.message()
        );
    }
}
