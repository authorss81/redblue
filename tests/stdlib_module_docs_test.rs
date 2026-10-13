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

use redblue::bytecode::vm::BytecodeVm;
use redblue::repl::ReplCompleter;

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

/// The scratch directory this file's own runs share: `target/tmp/p32docs`.
///
/// A test that needs to *run* rows out of [`answerable_builtins`] passes a
/// different one, so two gates walking the same table cannot reach the same file
/// at once — `cargo test` runs them in parallel.
const SCRATCH: &str = "p32docs";

/// Runs `source` in the scratch directory `scratch` and returns
/// `(exit success, stdout, stderr)`.
///
/// Each run gets its own file, named for the test that asked. A single shared
/// scratch path would be a race: `cargo test` runs these in parallel, and one
/// test's program would be another's when both reach the filesystem at once —
/// which shows up as a failure in a test that did nothing wrong.
///
/// Stdin is closed rather than inherited, so the `input` row reads end-of-input
/// immediately instead of waiting on a terminal nobody is sitting at.
fn run_in(scratch: &str, tag: &str, source: &str) -> (bool, String, String) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/tmp")
        .join(scratch);
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

/// [`run_in`] in this file's shared scratch directory.
fn run_named(tag: &str, source: &str) -> (bool, String, String) {
    run_in(SCRATCH, tag, source)
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
/// `scratch` is the directory the paths below are built in, and it is a parameter
/// because two gates in this file now *run* rows out of this table: `cargo test`
/// runs them in parallel, and one gate's `files.write` reaching another's scratch
/// file is a failure neither did anything to deserve.
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
fn answerable_builtins(address: &str, scratch: &str) -> Vec<(String, String, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/tmp")
        .join(scratch);
    std::fs::create_dir_all(&dir).expect("scratch directory");
    let path = |name: &str| dir.join(name).display().to_string();

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
            "time_unix",
            r#"say time.unix("2024-01-01 00:00:00")"#.into(),
            "1704067200\n".into(),
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

/// This list and [`redblue::stdlib::UNIMPLEMENTED_BUILTINS`] are the same set,
/// checked in both directions.
///
/// The second list is what the language itself reads: [`flat_builtin_names`]
/// drops a name in it, so the REPL does not offer a name whose every call is a
/// refusal, and that filter is a *promise* — a completion says a name can be
/// called. A promise gated by a list in a test file is a promise that expires
/// when the test file is not read, so the two are held equal here and the other
/// half of the property is held by the two gates below: `known_gaps()` runs each
/// entry and fails when one starts answering, and
/// `edge_every_builtin_the_repl_offers_is_run_and_answers` fails when a name the
/// completer offers has no row here.
///
/// So a name implemented comes out of both, and a new dead builtin has to go into
/// both, or one of the three fails.
///
/// [`flat_builtin_names`]: redblue::stdlib::flat_builtin_names
#[test]
fn edge_the_gap_list_is_the_set_of_builtins_the_language_calls_unimplemented() {
    let gaps: Vec<&str> = known_gaps().iter().map(|(name, _)| *name).collect();
    let unimplemented = redblue::stdlib::UNIMPLEMENTED_BUILTINS;

    assert_eq!(
        gaps.len(),
        unimplemented.len(),
        "known_gaps() and stdlib::UNIMPLEMENTED_BUILTINS have to be the same set: the \
         first is checked by running each entry, the second is what the completer reads \
         to decide not to offer a name that answers nothing"
    );

    for name in unimplemented {
        assert!(
            gaps.contains(name),
            "{name} is in stdlib::UNIMPLEMENTED_BUILTINS, so it has a program in \
             known_gaps() that is still refused. Without one nothing runs it, and the \
             list can outlive the gap it names the way thirteen of its entries did."
        );
    }
    for (name, _) in known_gaps() {
        assert!(
            unimplemented.contains(name),
            "{name} is in known_gaps() and not in stdlib::UNIMPLEMENTED_BUILTINS, so the \
             REPL still offers it: the completer reads the second list to decide what \
             not to offer, and this entry is not on it."
        );
    }

    // And the *programs* are the same programs, not merely the same names.
    //
    // `stdlib::UNIMPLEMENTED_BUILTIN_PROGRAMS` is the copy the library itself
    // reads — the REPL's own gates run it, from `src/repl/completer.rs` and
    // `tests/repl_module_completion_test.rs` — so a gap is checkable from the
    // code that depends on it rather than only from a test file nobody opens
    // when the language changes. That makes this list a second copy, and a
    // second copy that drifts is worse than none: `filter` would be run as a
    // two-argument call by one file and a one-argument call by the other, and the
    // one-argument form is refused for its *arity* by an implemented `filter`
    // too — so the copy that had drifted would be the one that stayed green
    // through the implementation. Hence the equality, in both directions.
    let mine: Vec<(&str, &str)> = known_gaps().to_vec();
    let theirs = redblue::stdlib::UNIMPLEMENTED_BUILTIN_PROGRAMS;
    for (name, program) in &mine {
        assert_eq!(
            redblue::stdlib::unimplemented_program(name),
            Some(*program),
            "known_gaps() runs `{program}` for {name} and the library runs a different \
             program for it, so the two gates are not testing the same claim"
        );
    }
    for (name, program) in theirs {
        assert!(
            mine.iter().any(|(listed, _)| listed == name),
            "{name} has a program in stdlib::UNIMPLEMENTED_BUILTIN_PROGRAMS and none in \
             known_gaps(), so `{program}` is run by the REPL's gates and by no gate that \
             checks the documentation"
        );
    }
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
    let rows = answerable_builtins(&address, SCRATCH);

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

/// The address [`answerable_builtins`] writes into its two `network` rows.
///
/// Nothing runs them here: `network_get` and `network_post` are the registry's
/// encodings of `network.get` and `network.post`, so neither is a flat builtin and
/// no name the completer offers reaches them. The loopback server is therefore not
/// started, and this is never dialled — it is spelled out rather than left to be
/// inferred from the two signatures.
const UNUSED_ADDRESS: &str = "http://127.0.0.1:1";

/// Every builtin the REPL offers is a name a program calls, and the call answers.
///
/// This is the gate the completer's own parity test could not be. That one walks
/// the offered list and asks whether each entry is *registered* — and
/// `filter` and `reduce` are registered, so they passed while every call to either
/// of them is `filter was called with arguments it cannot use`. A completion is the
/// one place the language invites a name, so "it is spelled correctly" is not the
/// promise; "calling it works" is.
///
/// So this runs the offered names rather than asking the registry about them. For
/// each one it takes the row [`answerable_builtins`] carries — the only table in
/// this file that holds a program which *uses* a builtin and the output it must
/// print — and runs it, pinning both halves. Three failures are all it takes to
/// break it, and each is a way the defect comes back:
///
/// - a name is offered with no row: the panic names it, and says to add a row or
///   to add the name to [`known_gaps`] and [`redblue::stdlib::UNIMPLEMENTED_BUILTINS`];
/// - a row stops answering: the second assertion fails with the program's output,
///   for an offered name only — the coverage gate above asks the same of every
///   registered name, gap or not;
/// - a name that answers nothing is offered at all: taken out of
///   `UNIMPLEMENTED_BUILTINS` while [`known_gaps`] still lists it, `filter` has no
///   row, so it fails here — and it fails
///   `edge_the_gap_list_is_the_set_of_builtins_the_language_calls_unimplemented`
///   first, which names the drift rather than the missing row.
///
/// It walks `complete("")` rather than [`redblue::stdlib::flat_builtin_names`]
/// because the list has to be the completer's own answer: a builtin the completer
/// stops offering has to fail the *other* gate, and one it starts offering has to
/// fail this one. Keywords and REPL commands come back from that call too and are
/// not builtins, so the walk keeps the names the registry holds; whether every
/// offered word is a word the language has is
/// `edge_the_repl_offers_no_builtin_a_program_cannot_call`
/// (`src/repl/completer.rs`) and `edge_no_name_the_repl_offers_is_one_a_program_
/// cannot_call` (`tests/repl_module_completion_test.rs`).
#[test]
fn edge_every_builtin_the_repl_offers_is_run_and_answers() {
    let rows = answerable_builtins(UNUSED_ADDRESS, "p32offered");
    let offered = ReplCompleter::new().complete("");

    let builtins = offered
        .iter()
        .filter(|name| redblue::stdlib::BUILTIN_NAMES.contains(&name.as_str()))
        .collect::<Vec<&String>>();
    assert!(
        builtins.len() > 30,
        "the completer offers most of the registry, so this is checking something rather \
         than nothing: {builtins:?}"
    );

    for name in builtins {
        let Some((_, source, expected)) = rows.iter().find(|(row, _, _)| row == name) else {
            panic!(
                "{name} is offered by the REPL and nothing here checks that a program can \
                 call it. Add a row to answerable_builtins() with the output it must print, \
                 or — if it is deliberately unimplemented — add it to known_gaps() and to \
                 stdlib::UNIMPLEMENTED_BUILTINS, which is what stops it being offered."
            );
        };
        let (ok, stdout, stderr) = run_in("p32offered", &format!("offered_{name}"), source);
        assert!(
            ok,
            "{name} is offered by the REPL, so the program a caller would write has to \
             answer:\n{source}\n--- stderr\n{stderr}\nEither implement it, or take it out \
             of stdlib::UNIMPLEMENTED_BUILTINS so the completer stops offering it."
        );
        assert_eq!(
            stdout, *expected,
            "{name} is offered by the REPL and answered something else:\n{source}\n\
             --- stdout\n{stdout}--- stderr\n{stderr}"
        );
    }

    for (name, _) in known_gaps() {
        assert!(
            !offered.contains(&name.to_string()),
            "{name} answers nothing, so the completer must not offer it: {offered:?}"
        );
    }
}
