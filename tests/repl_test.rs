//! End-to-end tests for the REPL: `rb` with no arguments, driven over a pipe.
//!
//! The REPL is the one surface a user touches first and the one with no
//! coverage at all — every claim about it was previously made by reading
//! `src/repl/mod.rs` rather than by running it. These tests pipe a session into
//! the real binary and assert on what it printed and on how it exited.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// How long a session may take before it counts as hung.
///
/// This is a liveness bound, not a performance assertion: nothing here claims a
/// session is *fast*, only that it *finishes*. The pre-fix REPL spins forever on
/// EOF printing prompts, so without a bound a regression would be an unkillable
/// test rather than a failing one.
const SESSION_LIMIT: Duration = Duration::from_secs(20);

/// What one piped REPL session produced.
struct Session {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Session {
    /// Asserts the session exited cleanly, quoting both streams.
    fn assert_exited_ok(&self) -> &Self {
        assert_eq!(
            self.code,
            Some(0),
            "the REPL should exit 0\nstdout:\n{}\nstderr:\n{}",
            self.stdout,
            self.stderr
        );
        self
    }

    fn assert_stdout_contains(&self, needle: &str) -> &Self {
        assert!(
            self.stdout.contains(needle),
            "stdout should contain {:?}\n--- actual ---\n{}",
            needle,
            self.stdout
        );
        self
    }

    fn assert_stdout_lacks(&self, needle: &str) -> &Self {
        assert!(
            !self.stdout.contains(needle),
            "stdout should not contain {:?}\n--- actual ---\n{}",
            needle,
            self.stdout
        );
        self
    }

    /// How many times `needle` appears, for the assertions that are about *how
    /// often* a session says something rather than whether it says it at all.
    fn count_stdout(&self, needle: &str) -> usize {
        self.stdout.matches(needle).count()
    }
}

/// Pipes `input` into `rb` with no arguments and collects the session.
///
/// stdin is closed as soon as `input` is written, so the REPL sees EOF exactly
/// as it would from Ctrl-D at a terminal. Reading stdout on its own thread means
/// a child that fills the pipe buffer cannot deadlock the test while the parent
/// waits for it to exit.
fn session(input: &str) -> Session {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rb"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the rb binary should start");

    let mut stdin = child.stdin.take().expect("stdin should be piped");
    let payload = input.to_string();
    let writer = thread::spawn(move || {
        let _ = stdin.write_all(payload.as_bytes());
        drop(stdin);
    });

    let mut stdout = child.stdout.take().expect("stdout should be piped");
    let reader = thread::spawn(move || read_all(&mut stdout));
    let mut stderr = child.stderr.take().expect("stderr should be piped");
    let errors = thread::spawn(move || read_all(&mut stderr));

    let deadline = Instant::now() + SESSION_LIMIT;
    let status = loop {
        match child.try_wait().expect("child status should be readable") {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                writer.join().ok();
                let _ = reader.join();
                panic!(
                    "the REPL did not exit within {:?} of its input closing: it \
                     hangs instead of terminating on EOF.\ninput:\n{}",
                    SESSION_LIMIT, input
                );
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    };

    writer.join().expect("the writer thread should not panic");
    let stdout = reader.join().expect("the reader thread should not panic");
    let stderr = errors.join().expect("the stderr thread should not panic");

    Session {
        code: status.code(),
        stdout,
        stderr,
    }
}

/// Drains a pipe on its own thread: a child writing more than the pipe buffer
/// holds would otherwise block forever waiting for a reader.
fn read_all(pipe: &mut impl Read) -> String {
    let mut buf = Vec::new();
    let _ = pipe.read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

/// Where `.rb` files under test are written, inside the project checkout.
fn scratch_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/repl");
    std::fs::create_dir_all(&dir).expect("scratch directory should be creatable");
    dir
}

fn scratch_file(name: &str, source: &str) -> PathBuf {
    let path = scratch_path(name, "rb");
    std::fs::write(&path, source).expect("the program should be writable");
    path
}

/// A path for a file that is not a program — a saved session, mostly — created
/// empty so it exists before a session is asked to save over it.
fn scratch_path(name: &str, extension: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let serial = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = scratch_dir().join(format!(
        "{}-{}-{}.{}",
        name,
        std::process::id(),
        serial,
        extension
    ));
    std::fs::write(&path, "").expect("the file should be creatable");
    path
}

#[test]
fn repl_runs_commands_and_quits() {
    let s = session(":help\n:vars\n:funcs\n:history\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("Redblue REPL Commands:")
        .assert_stdout_contains(":history")
        .assert_stdout_contains("No variables defined.")
        .assert_stdout_contains("No functions defined.")
        .assert_stdout_contains("     1  :help")
        .assert_stdout_contains("     4  :history")
        .assert_stdout_contains("Goodbye!");
}

#[test]
fn repl_shows_the_result_of_an_expression_under_vars() {
    let s = session("2 + 3\n:vars\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("= 5")
        .assert_stdout_contains("  _ = 5");
}

#[test]
fn repl_runs_a_multiline_block() {
    let s = session("if 1 is greater than 0 then\n    say \"yes\"\nend\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("yes")
        .assert_stdout_lacks("Error");
}

#[test]
fn repl_reports_an_uncaught_error_and_stays_alive() {
    let s = session("1 / 0\nsay \"alive\"\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("Division by zero")
        .assert_stdout_contains("alive");
}

#[test]
fn repl_runs_a_program_that_catches_its_own_error() {
    let program = scratch_file(
        "caught",
        "set caught to no\n\
         try\n\
         \x20   set past_end to [1, 2, 3][999]\n\
         catch error\n\
         \x20   set caught to yes\n\
         end\n\
         if caught is yes then\n\
         \x20   say \"caught is yes\"\n\
         end\n",
    );
    let s = session(&format!(":run {}\n:quit\n", program.display()));
    let _ = std::fs::remove_file(&program);

    s.assert_exited_ok()
        .assert_stdout_contains("caught is yes")
        .assert_stdout_contains("executed successfully");
}

#[test]
fn edge_first_line_syntax_error_keeps_the_repl_alive() {
    let s = session("say \"unclosed\nsay \"still here\"\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("Error")
        .assert_stdout_contains("still here")
        .assert_stdout_contains("Goodbye!");
}

/// A block the user never closed is reported rather than dropped on the floor at
/// EOF. Discarding the buffer made an unfinished block indistinguishable from a
/// finished one: the session printed `Goodbye!` either way.
///
/// The body's own value is what says it never ran: the report quotes the buffered
/// line back, but `6 * 7` was never evaluated, so its result never appears.
#[test]
fn edge_unterminated_block_then_eof_terminates_saying_what_it_never_ran() {
    let s = session("repeat 3 times\n    say 6 * 7\n");

    s.assert_exited_ok()
        .assert_stdout_contains("Goodbye!")
        .assert_stdout_contains("incomplete block")
        .assert_stdout_contains("never closed with 'end'")
        .assert_stdout_lacks("42");
}

/// The block openers the old last-word heuristic missed, each of which used to be
/// run half-written and reported as a parse error. Every body prints its own
/// marker, so the table asserts the body ran rather than that the session merely
/// survived.
#[test]
fn repl_runs_a_block_the_last_word_heuristic_missed() {
    let cases: Vec<(&str, &str, &str)> = vec![
        (
            "repeat 3 times",
            "repeat 3 times\n    say \"REPEATED\"\nend\n",
            "REPEATED",
        ),
        (
            "for each i from 1 to 3",
            "for each i from 1 to 3\n    say \"TICKED\"\nend\n",
            "TICKED",
        ),
        (
            "unless",
            "unless 1 is nothing then\n    say \"KEPT\"\nend\n",
            "KEPT",
        ),
        (
            "try",
            "try\n    say \"TRIED\"\ncatch e\n    say \"CAUGHT\"\nend\n",
            "TRIED",
        ),
        (
            "while",
            "while 1 is less than 0\n    say \"NEVER\"\nend\nsay \"AFTER WHILE\"\n",
            "AFTER WHILE",
        ),
    ];

    for (name, input, marker) in cases {
        let s = session(&format!("{}:quit\n", input));

        s.assert_exited_ok();
        assert!(
            !s.stdout.contains("Error"),
            "{} should run without error:\n{}",
            name,
            s.stdout
        );
        if !marker.is_empty() {
            s.assert_stdout_contains(marker);
        }
    }
}

/// `to greet(name)` is a block opener the last-word heuristic missed, and calling
/// it is the next line — which is a name the analyzer used to refuse to resolve
/// there, because each line was analyzed against an empty scope.
#[test]
fn edge_a_to_declaration_is_read_as_a_block_and_callable_afterwards() {
    let s = session("to greet(name)\n    say \"HI \" + name\nend\ngreet(\"World\")\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("HI World")
        .assert_stdout_lacks("Unknown variable");
}

/// A `to` binds into a local scope, not into the globals, so scanning the globals
/// alone never offered a function the session had just defined.
#[test]
fn edge_a_stray_end_does_not_swallow_the_rest_of_the_session() {
    let s = session("say \"before\"\nend\nsay \"after\"\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("before")
        .assert_stdout_contains("after")
        .assert_stdout_contains("Goodbye!");
}

/// `:load` promises "Load and run a file", so it runs the file. It used to read
/// it and print its size, leaving the session holding none of what it declared.
#[test]
fn repl_load_runs_the_file_rather_than_only_measuring_it() {
    let program = scratch_file("load-runs", "say \"LOADED AND RAN\"\n");
    let s = session(&format!(":load {}\n:quit\n", program.display()));
    let _ = std::fs::remove_file(&program);

    s.assert_exited_ok()
        .assert_stdout_contains("LOADED AND RAN")
        .assert_stdout_lacks("Error");
}

/// A command's argument is every word after it: a path with a space in it is the
/// whole remainder, not the first word of it.
#[test]
fn edge_a_command_argument_with_a_space_in_it_is_not_truncated() {
    let dir = scratch_dir().join("with a space");
    std::fs::create_dir_all(&dir).expect("the directory should be creatable");
    let program = dir.join("prog.rb");
    std::fs::write(&program, "say \"SPACED PATH\"\n").expect("the file should be writable");

    let s = session(&format!(":load {}\n:quit\n", program.display()));
    let _ = std::fs::remove_file(&program);

    s.assert_exited_ok()
        .assert_stdout_contains("SPACED PATH")
        .assert_stdout_lacks("No such file");
}

/// `:vars` prints a listing, and a listing whose order changes from one session
/// to the next cannot be read. `variables` is a `HashMap`, so it is sorted.
#[test]
fn edge_vars_lists_the_session_bindings() {
    let s = session("2 + 3\n:vars\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("Variables:")
        .assert_stdout_contains("  _ = 5");
}

#[test]
fn edge_load_of_a_missing_path_reports_the_failure() {
    let missing = scratch_dir().join("definitely-not-here-9f3a.rb");
    assert!(
        !missing.exists(),
        "the path under test must not exist before the test runs"
    );
    let s = session(&format!(":load {}\n:quit\n", missing.display()));

    s.assert_exited_ok()
        .assert_stdout_contains("Could not load file:")
        // The OS half of the message differs by platform ("No such file ..."
        // vs "The system cannot find ..."), but both carry the os-error tag.
        // Assert that tag, not Unix phrasing.
        .assert_stdout_contains("(os error");
}

/// A command given no argument is reported as missing one, not as an unknown
/// command, and never runs against an empty path.
#[test]
fn edge_command_missing_its_argument_is_reported_not_guessed() {
    let s = session(":load\n:inspect\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("Usage: :load")
        .assert_stdout_contains("Usage: :inspect")
        .assert_stdout_lacks("Unknown command 'load");
}

/// A command word that does not exist is reported by name, and the session
/// carries on rather than treating it as source.
#[test]
fn edge_unknown_command_is_named_and_the_session_survives() {
    let s = session(":bogus\nsay \"still here\"\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("Unknown command 'bogus'")
        .assert_stdout_contains("still here");
}

/// A REPL carries state from one line to the next, and this could not be asserted
/// end to end before: each line was analyzed against an empty scope, so the second
/// line of `set x to 41` / `set y to x + 1` / `say y` was rejected with
/// `Unknown variable 'x'`. Every other test here avoided a cross-line read.
#[test]
fn repl_carries_a_binding_from_one_line_to_the_next() {
    let s = session("set x to 41\nset y to x + 1\nsay y\n:vars\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("42")
        .assert_stdout_contains("  x = 41")
        .assert_stdout_contains("  y = 42")
        .assert_stdout_lacks("Unknown variable");
}

/// `:vars` is a listing of the session's bindings, and the `set`s above are what
/// it has to show. It read only the REPL's own map, which held `_` and nothing
/// else, so a session that had bound a dozen names listed one.
#[test]
fn edge_vars_lists_the_names_the_session_bound_not_only_the_last_result() {
    let s = session("set alpha to 1\nset beta to 2\n2 + 3\n:vars\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("Variables:")
        .assert_stdout_contains("  alpha = 1")
        .assert_stdout_contains("  beta = 2")
        .assert_stdout_contains("  _ = 5");
}

/// `:funcs` names the functions the session declared. It used to print "User-
/// defined functions are stored in the VM" — which named none — and the test that
/// ran `:funcs` asserted that sentence, so a missing listing could not fail.
#[test]
fn edge_funcs_names_the_function_the_session_declared() {
    let s = session("to shout(word)\n    say word\nend\n:funcs\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("Functions:")
        .assert_stdout_contains("  shout")
        .assert_stdout_lacks("User-defined functions are stored in the VM.");
}

/// `:inspect` asks the session, not the REPL's own map. `set`, `to` and `import`
/// all bind in the VM, so `:inspect <session-name>` answered "Variable not found"
/// for every name the session had actually bound.
#[test]
fn edge_inspect_reports_a_value_the_session_bound() {
    let s = session("set counter to 7\n:inspect counter\n:inspect nosuchname\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("counter: 7")
        .assert_stdout_contains("Variable 'nosuchname' not found.");
}

/// `:inspect` names the type of the value in Redblue's own words — the name
/// `type_of` reports and a `Runtime` error quotes. It used to print
/// `std::any::type_name_of_val`, the *Rust* type of every value the VM holds, so
/// the second line read `Type: "redblue::value::Value"` for a number and for a
/// text and for a function alike and told the user nothing about any of them.
#[test]
fn edge_inspect_reports_the_redblue_type_of_what_it_found() {
    let s = session(
        "set counter to 7\nset words to [\"a\"]\n:inspect counter\n:inspect words\n:quit\n",
    );

    s.assert_exited_ok()
        .assert_stdout_contains("counter: 7\nType: number")
        .assert_stdout_contains("Type: list")
        .assert_stdout_lacks("redblue::value::Value");
}

/// A module's members are published in the VM under the internal
/// `MathUtils_circle_area` name a call through the module resolves to. That name
/// is how the VM reaches the function, not a name the program wrote: `:vars`,
/// `:funcs` and the analyzer's seed are all built from `Vm::user_names`, so
/// before it filtered them out a session that had imported one module listed a
/// dozen names no line of Redblue can call or read.
#[test]
fn edge_an_imported_modules_members_are_not_listed_as_session_names() {
    let s = session("import MathUtils\nMathUtils.circle_area(2)\n:vars\n:funcs\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("12.56636")
        .assert_stdout_contains("  TAU = 6.28318")
        .assert_stdout_contains("  MathUtils = nothing")
        .assert_stdout_lacks("MathUtils_circle_area");
}

/// `:reset` leaves the session at a prompt. It used to replace the VM and clear
/// the last result and nothing else, so a block the user had opened and not
/// closed was still buffered: the line after the reset joined it and nothing ran
/// until an `end` that nobody was going to type arrived.
#[test]
fn edge_reset_after_an_unfinished_block_returns_the_session_to_a_prompt() {
    let s = session("repeat 3 times\n:reset\nsay \"AFTER RESET\"\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("REPL state reset.")
        .assert_stdout_contains("AFTER RESET");
}

/// A command word carries one sigil. Every leading sigil used to be stripped, so
/// `::quit` ended the session: a doubled sigil was enough to quit, load or clear
/// a session nobody asked it to.
#[test]
fn edge_a_doubled_sigil_is_a_mistyped_command_and_not_the_command() {
    let s = session("::quit\n...vars\nsay \"STILL HERE\"\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("Unknown command ':quit'")
        .assert_stdout_contains("Unknown command '..vars'")
        .assert_stdout_contains("STILL HERE");
}

/// A command, in the help, from the parser: `:help` lists every command under
/// `:` and says the other sigil is the same word, because `.quit` has been
/// accepted since the command table was introduced and `:help` could not mention
/// it.
#[test]
fn edge_help_says_the_second_sigil_reaches_the_same_commands() {
    let s = session(":help\n.quit\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("Every command also answers to '.' in place of ':'");
}

/// A path is a path, not a list of words joined by one space. A file whose
/// directory name holds two consecutive spaces was named by neither.
#[test]
fn edge_a_command_argument_with_consecutive_spaces_is_not_collapsed() {
    let dir = scratch_dir().join("two  spaces");
    std::fs::create_dir_all(&dir).expect("the directory should be creatable");
    let program = dir.join("prog.rb");
    std::fs::write(&program, "say \"DOUBLE SPACE\"\n").expect("the file should be writable");

    let s = session(&format!(":load {}\n:quit\n", program.display()));
    let _ = std::fs::remove_file(&program);

    s.assert_exited_ok()
        .assert_stdout_contains("DOUBLE SPACE")
        .assert_stdout_lacks("No such file");
}

/// `:save` and `:restore` are two halves of one command between them. Nothing in
/// the REPL read the file back, so a saved session could not be loaded and the
/// round trip existed only between two histories in a unit test.
#[test]
fn edge_save_then_restore_brings_the_session_back_in_a_second_session() {
    let saved = scratch_path("saved-session", "txt");
    let first = session(&format!(
        "set remembered to 5\nsay \"REMEMBERED\"\n:save {}\n:quit\n",
        saved.display()
    ));
    first
        .assert_exited_ok()
        .assert_stdout_contains("Session saved to");

    let contents = std::fs::read_to_string(&saved).expect("the session file should be readable");
    let second = session(&format!(
        ":restore {}\nsay remembered\n:vars\n:history\n:quit\n",
        saved.display()
    ));
    let _ = std::fs::remove_file(&saved);

    // The saved session is every line it read, and `:save` is one of them: the
    // read loop records a line before it dispatches it, so the file holds the
    // command that wrote it too.
    second
        .assert_exited_ok()
        .assert_stdout_contains("Restored 3 line(s)")
        .assert_stdout_contains("     1  set remembered to 5")
        .assert_stdout_contains("     2  say \"REMEMBERED\"");
    // And the lines are run, not only listed: a restore that filled the history
    // left `say remembered` an `Unknown variable` on the next line, because the
    // binding the saved line made was not part of what `:restore` brought back.
    second
        .assert_stdout_contains("  remembered = 5")
        .assert_stdout_lacks("Unknown variable");
    assert_eq!(
        contents.lines().count(),
        3,
        "every line of the session was written to the file:\n{}",
        contents
    );
}

/// A restore of a file that is not there is reported, and the session that asked
/// for it keeps its own history.
#[test]
fn edge_restore_of_a_missing_path_reports_the_failure() {
    let missing = scratch_path("never-saved", "txt");
    std::fs::remove_file(&missing).expect("the scratch file should be removable");
    assert!(
        !missing.exists(),
        "the path under test must not exist before the test runs"
    );

    let s = session(&format!(
        "set kept to 1\n:restore {}\n:history\n:quit\n",
        missing.display()
    ));

    s.assert_exited_ok()
        .assert_stdout_contains("Could not restore session:")
        .assert_stdout_contains("     1  set kept to 1");
}

/// A block whose `end` never arrives is a pipe that will never close. The buffer
/// grew without limit, so the process grew until it was killed; past the limit the
/// session drops the block, says so, and goes back to reading lines.
#[test]
fn edge_a_piped_block_that_never_closes_does_not_grow_without_limit() {
    let mut input = String::from("repeat 3 times\n");
    for index in 0..600 {
        input.push_str(&format!("    say \"LINE {}\"\n", index));
    }
    input.push_str("say \"AFTER THE BLOCK\"\n:quit\n");

    let s = session(&input);

    s.assert_exited_ok()
        .assert_stdout_contains("a session holds at most")
        .assert_stdout_contains("AFTER THE BLOCK")
        .assert_stdout_contains("Goodbye!");
    assert!(
        s.stdout.len() < 64 * 1024,
        "the abandoned block was not echoed back in full: {} bytes",
        s.stdout.len()
    );
}

/// The report of an unclosed block quotes the lines back so the user can see what
/// never ran, and counts what it did not quote rather than printing it all.
#[test]
fn edge_the_report_of_an_unclosed_block_counts_the_lines_it_did_not_echo() {
    let mut input = String::from("repeat 3 times\n");
    for index in 0..40 {
        input.push_str(&format!("    say \"BODY {}\"\n", index));
    }

    let s = session(&input);

    s.assert_exited_ok()
        .assert_stdout_contains("incomplete block")
        .assert_stdout_contains("never closed with 'end'")
        .assert_stdout_contains("and 21 more line(s)")
        .assert_stdout_lacks("BODY 39")
        .assert_stdout_lacks("42");
}

/// `say` output belongs to the line that printed it. `Vm::run` prints the VM's
/// whole buffer at the end of a program and the REPL never drained it, so every
/// line re-printed everything the lines before it had said.
#[test]
fn edge_a_line_says_its_line_once_however_many_lines_follow() {
    let s = session("say \"ONCE\"\nsay \"SECOND\"\nsay \"THIRD\"\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("ONCE")
        .assert_stdout_contains("THIRD");
    assert_eq!(
        s.count_stdout("ONCE"),
        1,
        "the first line's output is not repeated by the lines after it:\n{}",
        s.stdout
    );
    assert_eq!(
        s.count_stdout("SECOND"),
        1,
        "nor is the second's:\n{}",
        s.stdout
    );
}

/// `:help` and `:example` are the only documentation a user has at the prompt, so
/// what they print has to be Redblue. They promised a string interpolation `say`
/// does not perform — `say "Hello, {{name}}!"` printed `Hello, {{name}}!` — and the
/// quick reference showed `if x > 5 then`, which is not a condition in Redblue.
#[test]
fn edge_help_and_examples_promise_only_what_the_session_can_run() {
    let s = session(":help\n:example\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("Quick Reference:")
        .assert_stdout_lacks("{{")
        .assert_stdout_lacks("if x > 5 then");
}

/// Every alias the parser accepts is offered by completion and named by `:help`.
/// The completer kept its own list of sixteen canonical names beside a parser that
/// took thirty aliases, so `:q`, `:h`, `:hist`, `:v`, `:f` and `:examples` all
/// worked and none of them could be completed or read about.
#[test]
fn edge_help_names_every_alias_the_repl_accepts() {
    let s = session(":help\n:quit\n");

    s.assert_exited_ok();
    for alias in [
        ":q",
        ":h",
        ":cls",
        ":hist",
        ":v",
        ":funcs",
        ":f",
        ":l",
        ":s",
        ":r",
        ":i",
        ":examples",
    ] {
        assert!(
            s.stdout.contains(alias),
            "{:?} is accepted by the parser, so :help has to name it:\n{}",
            alias,
            s.stdout
        );
    }
}

/// `:restore` is a command like any other: given no file it is reported as missing
/// its argument, not run against an empty path.
#[test]
fn edge_command_missing_its_argument_is_reported_for_restore_too() {
    let s = session(":restore\n:quit\n");

    s.assert_exited_ok()
        .assert_stdout_contains("Usage: :restore")
        .assert_stdout_lacks("Unknown command");
}
