//! Host-shell support for the one-shot commands a session can run.
//!
//! A shell is a property of the machine, not of Redblue, so everything in this
//! module is host-only by construction: the command table labels the features
//! as such, and a target with no shell is refused here with one sentence rather
//! than being allowed to fail somewhere in the middle of a pipe.

#[cfg(any(unix, windows))]
use std::io::{self, Read, Write};
use std::path::Path;
#[cfg(any(unix, windows))]
use std::process::{Child, Command, ExitStatus, Stdio};
#[cfg(any(unix, windows))]
use std::thread;
use std::time::Duration;
#[cfg(any(unix, windows))]
use std::time::Instant;

use super::completer::DIRECTORY_SEPARATOR;

/// How long one host command may run before the session stops it.
///
/// A shell command is not part of the language's own evaluator, so it has no
/// step budget of its own. Without a deadline `!sleep 1000000` or `!cat
/// /dev/zero` would hold the REPL forever; the session stops the process instead
/// and says so.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

/// How much of each output stream is kept.
///
/// A stream is drained through a fixed buffer: once this many bytes have been
/// kept the reader closes its end, which is what asks a command that never stops
/// producing output (`yes`, `cat /dev/zero`) to stop. The outcome carries a
/// `truncated` flag so the session can tell the user what it did.
pub const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

/// The bytes read from one pipe per `read` call.
#[cfg(any(unix, windows))]
const READ_BUFFER_BYTES: usize = 8 * 1024;

/// How often a waiting session looks at a child that has not exited.
#[cfg(any(unix, windows))]
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// How long the output pipes are drained after the child itself has exited.
///
/// A background process can inherit the pipes and outlive the shell that started
/// it. The child has answered, so waiting on that inheritance for ever would be
/// the same hang the deadline exists to prevent; the process group is stopped
/// after this so the readers can be joined.
#[cfg(any(unix, windows))]
const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// What a command left behind: its two output streams and the status the host
/// gave it.
///
/// `code` is an `Option` because a process killed by a signal has no exit code;
/// reporting `-1` for it would be inventing a status the shell did not give.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellOutcome {
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
    /// Whether either stream was closed after [`MAX_OUTPUT_BYTES`].
    pub truncated: bool,
}

/// The sentence a target with no shell answers with.
pub const NO_SHELL: &str = "Error: this host has no shell, so shell commands are unavailable.";

/// The sentence an empty `!` answers with, before any process is started.
pub const EMPTY_COMMAND: &str = "Error: ! needs a command. Usage: !<command>";

/// The sentence an empty `.pipe` answers with, before any process is started.
pub const EMPTY_PIPE: &str = "Error: .pipe needs a command. Usage: .pipe <command>";

/// The bytes one output reader kept and whether it stopped at the cap.
#[cfg(any(unix, windows))]
type CappedRead = io::Result<(Vec<u8>, bool)>;

/// A reader draining one of a child's output pipes.
#[cfg(any(unix, windows))]
type ReadThread = thread::JoinHandle<CappedRead>;

/// A command's output streams after they have been drained.
#[cfg(any(unix, windows))]
struct Collected {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    truncated: bool,
    status: ExitStatus,
}

impl ShellOutcome {
    #[cfg(any(unix, windows))]
    fn from_collected(collected: Collected) -> Self {
        Self {
            // Taking the `Vec<u8>` apart rather than copying a `Cow` keeps a
            // megabyte of command output to one allocation instead of two.
            stdout: match String::from_utf8(collected.stdout) {
                Ok(text) => text,
                Err(error) => String::from_utf8_lossy(error.as_bytes()).into_owned(),
            },
            stderr: match String::from_utf8(collected.stderr) {
                Ok(text) => text,
                Err(error) => String::from_utf8_lossy(error.as_bytes()).into_owned(),
            },
            code: collected.status.code(),
            truncated: collected.truncated,
        }
    }
}

/// The command that runs `command` in the host's own shell.
///
/// On Unix the shell leads a process group of its own, so a command that
/// times out can be stopped together with every process it started.
#[cfg(unix)]
fn shell_command(command: &str) -> Command {
    use std::os::unix::process::CommandExt;

    let mut shell = Command::new("sh");
    shell.arg("-c").arg(command);
    shell.process_group(0);
    shell
}

#[cfg(windows)]
fn shell_command(command: &str) -> Command {
    let mut shell = Command::new("cmd");
    shell.arg("/C").arg(command);
    shell
}

/// Drains one pipe, keeping at most `max` bytes.
///
/// Returning as soon as the limit is reached drops the read end of the pipe.
/// That closes it under a command that would otherwise write for ever, which is
/// the resource limit doing its job rather than a second failure to report.
#[cfg(any(unix, windows))]
fn read_capped(mut reader: impl Read, max: usize) -> CappedRead {
    let mut kept = Vec::new();
    let mut buffer = [0u8; READ_BUFFER_BYTES];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok((kept, false)),
            Ok(count) => {
                let room = max.saturating_sub(kept.len());
                if count > room {
                    kept.extend_from_slice(&buffer[..room]);
                    return Ok((kept, true));
                }
                kept.extend_from_slice(&buffer[..count]);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}

/// Waits for `child`, draining both output streams, and stops it after
/// `timeout`.
///
/// The streams are drained on their own threads so a child that fills one pipe
/// cannot deadlock a parent that is waiting for it to exit.
#[cfg(any(unix, windows))]
fn wait_for(mut child: Child, timeout: Duration, max_output: usize) -> Result<Collected, String> {
    let pid = child.id();
    let stdout = child
        .stdout
        .take()
        .map(|pipe| thread::spawn(move || read_capped(pipe, max_output)));
    let stderr = child
        .stderr
        .take()
        .map(|pipe| thread::spawn(move || read_capped(pipe, max_output)));

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                let now = Instant::now();
                if now >= deadline {
                    stop_child(&mut child);
                    // The child's whole group is gone, so the pipes reach EOF.
                    // The readers are joined rather than detached: a blocked
                    // thread outliving the command would be a leak for every
                    // timeout a session ever sees.
                    let _ = join_reader(stdout, Instant::now(), || {});
                    let _ = join_reader(stderr, Instant::now(), || {});
                    return Err(format!(
                        "Error: the shell command did not finish within {timeout:?}."
                    ));
                }
                thread::sleep(POLL_INTERVAL.min(deadline - now));
            }
            Err(error) => {
                stop_child(&mut child);
                let _ = join_reader(stdout, Instant::now(), || {});
                let _ = join_reader(stderr, Instant::now(), || {});
                return Err(format!("Error: could not wait for the shell: {error}"));
            }
        }
    };

    let drain_deadline = Instant::now() + DRAIN_TIMEOUT;
    let (stdout, stdout_truncated) =
        join_reader(stdout, drain_deadline, || stop_process_group(pid))?;
    let (stderr, stderr_truncated) =
        join_reader(stderr, drain_deadline, || stop_process_group(pid))?;

    Ok(Collected {
        stdout,
        stderr,
        truncated: stdout_truncated || stderr_truncated,
        status,
    })
}

/// Stops `child` and every process it started, then reaps it.
#[cfg(any(unix, windows))]
fn stop_child(child: &mut Child) {
    stop_process_group(child.id());
    let _ = child.kill();
    let _ = child.wait();
}

/// Sends the kill signal to the process group led by `pid`.
///
/// On Unix [`shell_command`] makes the child the leader of its own group, so a
/// negative pid reaches the descendants that inherited its pipe ends too. On
/// Windows the platform's own tree-kill does the same job.
#[cfg(unix)]
fn stop_process_group(pid: u32) {
    // The negative-pid form addresses the group whose id is the child's pid.
    // A pid beyond `i32::MAX` cannot be addressed that way; no supported host
    // allocates one, and refusing to wrap is safer than signalling the wrong
    // group.
    let Ok(pid) = i32::try_from(pid) else {
        return;
    };
    if pid == 0 {
        return;
    }
    // SAFETY: `kill` only reads the two integers passed to it and the group was
    // created for this child by `shell_command`.
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
}

#[cfg(windows)]
fn stop_process_group(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/T", "/F", "/PID"])
        .arg(pid.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Joins one output reader, stopping the process group if the deadline passes
/// before the reader finishes.
///
/// The reader is joined either way: a detached reader would hold the pipe and
/// the thread itself for as long as an inherited write end stays open. Stopping
/// the group is what lets the read return.
///
/// A reader that panicked is an error rather than empty output: the command did
/// write something, and reporting nothing would hide both the loss and the
/// failure that caused it.
#[cfg(any(unix, windows))]
fn join_reader(
    handle: Option<ReadThread>,
    deadline: Instant,
    stop: impl Fn(),
) -> Result<(Vec<u8>, bool), String> {
    let Some(handle) = handle else {
        return Ok((Vec::new(), false));
    };
    while !handle.is_finished() {
        if Instant::now() >= deadline {
            stop();
            break;
        }
        thread::sleep(POLL_INTERVAL);
    }
    let read = handle.join().map_err(|_| {
        "Error: the shell command's output reader stopped unexpectedly.".to_string()
    })?;
    read.map_err(|error| format!("Error: could not read the shell command's output: {error}"))
}

/// Joins the thread writing a command's standard input, stopping the process
/// group if the deadline passes first.
///
/// Like [`join_reader`], the handle is always joined: a command that stopped
/// reading has already closed the pipe, and a command that is still running is
/// stopped so the write can return.
#[cfg(any(unix, windows))]
fn join_writer(
    handle: thread::JoinHandle<io::Result<()>>,
    deadline: Instant,
    stop: impl Fn(),
) -> Result<io::Result<()>, String> {
    while !handle.is_finished() {
        if Instant::now() >= deadline {
            stop();
            break;
        }
        thread::sleep(POLL_INTERVAL);
    }
    handle
        .join()
        .map_err(|_| "Error: the shell command's input writer stopped unexpectedly.".to_string())
}

/// Runs `command` in the host shell and waits for it to finish.
pub fn run(command: &str) -> Result<ShellOutcome, String> {
    run_with(command, COMMAND_TIMEOUT, MAX_OUTPUT_BYTES)
}

#[cfg(any(unix, windows))]
fn run_with(command: &str, timeout: Duration, max_output: usize) -> Result<ShellOutcome, String> {
    if command.trim().is_empty() {
        return Err(EMPTY_COMMAND.to_string());
    }
    let child = shell_command(command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Error: could not start the shell: {error}"))?;
    let collected = wait_for(child, timeout, max_output)?;
    Ok(ShellOutcome::from_collected(collected))
}

#[cfg(not(any(unix, windows)))]
fn run_with(command: &str, _timeout: Duration, _max_output: usize) -> Result<ShellOutcome, String> {
    if command.trim().is_empty() {
        return Err(EMPTY_COMMAND.to_string());
    }
    Err(NO_SHELL.to_string())
}

/// Sends `input` to `command`'s standard input and returns what it wrote.
///
/// The writing happens on its own thread while this one drains both output
/// streams through [`wait_for`]. A command like `cat` writes back everything it
/// reads, so writing the whole payload first would fill the output pipe and
/// deadlock the pair: the child waiting for more input, the parent waiting for
/// the child to read it.
pub fn pipe(command: &str, input: &str) -> Result<ShellOutcome, String> {
    pipe_with(command, input, COMMAND_TIMEOUT, MAX_OUTPUT_BYTES)
}

#[cfg(any(unix, windows))]
fn pipe_with(
    command: &str,
    input: &str,
    timeout: Duration,
    max_output: usize,
) -> Result<ShellOutcome, String> {
    if command.trim().is_empty() {
        return Err(EMPTY_PIPE.to_string());
    }
    let mut child = shell_command(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Error: could not start the shell: {error}"))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Error: the shell command has no standard input.".to_string())?;

    let pid = child.id();
    let payload = input.to_string();
    let writer = thread::spawn(move || -> io::Result<()> {
        stdin.write_all(payload.as_bytes())?;
        stdin.flush()
    });
    let collected = match wait_for(child, timeout, max_output) {
        Ok(collected) => collected,
        Err(error) => {
            // `wait_for` has stopped the group, so the writer's pipe is broken
            // and it can be joined. Returning first would detach it.
            let _ = join_writer(writer, Instant::now(), || {});
            return Err(error);
        }
    };

    let writer = join_writer(writer, Instant::now() + DRAIN_TIMEOUT, || {
        stop_process_group(pid)
    })?;

    match writer {
        // A command that stops reading early (`head`, a failing command) closes
        // the pipe under us. That is the command's answer, and its exit code is
        // what the user is shown; the broken pipe is not a second failure.
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {}
        Err(error) => {
            return Err(format!(
                "Error: could not send input to the shell command: {error}"
            ))
        }
    }

    Ok(ShellOutcome::from_collected(collected))
}

#[cfg(not(any(unix, windows)))]
fn pipe_with(
    command: &str,
    _input: &str,
    _timeout: Duration,
    _max_output: usize,
) -> Result<ShellOutcome, String> {
    if command.trim().is_empty() {
        return Err(EMPTY_PIPE.to_string());
    }
    Err(NO_SHELL.to_string())
}

/// The visible entries of `path`, sorted the way completion sorts them and with
/// a trailing [`DIRECTORY_SEPARATOR`] on every directory.
///
/// The separator is completion's own constant rather than a second copy, so
/// `.ls` and TAB cannot drift apart on what a directory looks like. Names that
/// begin with `.` are hidden, as they are in completion: a session listing is
/// for finding a file to load, and dot files are noise in that search.
pub fn listing(path: &Path) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(path)
        .map_err(|error| format!("Error: could not list {}: {error}", path.display()))?;

    let mut names = Vec::new();
    for entry in entries.flatten() {
        // A name the host cannot render as text is still an entry: replacing
        // the undecodable bytes keeps it visible instead of letting `.ls`
        // silently disagree with the directory.
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let separator = if entry
            .file_type()
            .map(|kind| kind.is_dir() || (kind.is_symlink() && entry.path().is_dir()))
            .unwrap_or(false)
        {
            DIRECTORY_SEPARATOR
        } else {
            ""
        };
        names.push(format!("{name}{separator}"));
    }

    names.sort_by_key(|name| (name.to_lowercase(), name.clone()));
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::{listing, run, EMPTY_COMMAND};

    /// A command that keeps the shell busy past any test deadline.
    #[cfg(any(unix, windows))]
    fn sleep_command() -> &'static str {
        if cfg!(windows) {
            "ping -n 6 127.0.0.1 > nul"
        } else {
            "exec sleep 5"
        }
    }

    /// A command that writes far more than a small output limit.
    #[cfg(any(unix, windows))]
    fn endless_output_command() -> &'static str {
        if cfg!(windows) {
            "for /L %i in (1,1,1000000) do @echo x"
        } else {
            "yes"
        }
    }

    /// A command that succeeds without reading its standard input.
    #[cfg(any(unix, windows))]
    fn immediate_command() -> &'static str {
        if cfg!(windows) {
            "exit /b 0"
        } else {
            "true"
        }
    }

    #[test]
    fn empty_shell_command_is_refused_without_starting_a_process() {
        assert_eq!(run(" ").unwrap_err(), EMPTY_COMMAND);
    }

    #[test]
    fn listing_of_a_missing_directory_names_the_path() {
        let missing = std::path::Path::new("no-such-directory-phase-068-list");
        let refusal = listing(missing).expect_err("a missing directory cannot be listed");
        assert!(
            refusal.contains("no-such-directory-phase-068-list"),
            "the refusal names the path: {refusal}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn edge_listing_keeps_names_that_are_not_utf8() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/tmp/repl-shell/non-utf8-listing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory should be creatable");
        std::fs::write(dir.join(OsStr::from_bytes(b"bad-\xff-name")), b"x")
            .expect("a non-UTF-8 name is still a file name");

        let names = listing(&dir).expect("the directory should be listable");
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            names
                .iter()
                .any(|entry| entry.contains("bad-") && entry.contains("-name")),
            "the undecodable name is listed rather than silently skipped: {names:?}"
        );
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn edge_a_command_that_overruns_its_deadline_is_stopped() {
        let refusal = super::run_with(
            sleep_command(),
            std::time::Duration::from_millis(100),
            super::MAX_OUTPUT_BYTES,
        )
        .expect_err("a command that outlives its deadline is stopped");

        assert!(
            refusal.contains("did not finish"),
            "the refusal says the command was stopped rather than waiting for ever: {refusal}"
        );
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn edge_a_pipe_command_that_overruns_its_deadline_is_stopped() {
        let refusal = super::pipe_with(
            sleep_command(),
            "input",
            std::time::Duration::from_millis(100),
            super::MAX_OUTPUT_BYTES,
        )
        .expect_err("a pipe command that outlives its deadline is stopped");

        assert!(
            refusal.contains("did not finish"),
            "the refusal says the pipe command was stopped: {refusal}"
        );
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn edge_output_over_the_limit_is_closed_and_marked_truncated() {
        let outcome = super::run_with(
            endless_output_command(),
            std::time::Duration::from_secs(5),
            32,
        )
        .expect("a command whose output is capped still finishes");

        assert!(
            outcome.truncated,
            "the outcome says the output was cut short"
        );
        assert_eq!(
            outcome.stdout.len(),
            32,
            "and exactly the limit was kept, not the whole stream"
        );
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn edge_pipe_to_a_command_that_stops_reading_is_not_a_repl_failure() {
        let outcome = super::pipe_with(
            immediate_command(),
            &"x".repeat(1024 * 1024),
            std::time::Duration::from_secs(5),
            super::MAX_OUTPUT_BYTES,
        )
        .expect("a command closing its stdin early is the command's own answer");

        assert_eq!(
            outcome.code,
            Some(0),
            "and its exit status is still reported"
        );
        assert!(
            !outcome.truncated,
            "nothing was written back, so nothing was truncated"
        );
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn edge_a_panicking_output_reader_is_reported_instead_of_dropping_output() {
        let handle = std::thread::spawn(|| -> std::io::Result<(Vec<u8>, bool)> {
            panic!("the reader failed");
        });

        let result = super::join_reader(
            Some(handle),
            std::time::Instant::now() + std::time::Duration::from_secs(1),
            || {},
        );

        let refusal = result.expect_err("a panicked reader is a failure, not empty output");
        assert!(
            refusal.contains("reader"),
            "the refusal names the failed reader: {refusal}"
        );
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn edge_a_reader_past_the_drain_deadline_is_joined_not_detached() {
        let handle = std::thread::spawn(|| -> std::io::Result<(Vec<u8>, bool)> {
            std::thread::sleep(std::time::Duration::from_millis(50));
            Ok((b"late-output".to_vec(), false))
        });

        let (output, truncated) =
            super::join_reader(Some(handle), std::time::Instant::now(), || {})
                .expect("a reader that finishes after the deadline still joins");

        assert_eq!(
            output, b"late-output",
            "the output is read rather than discarded when the reader outlives the deadline"
        );
        assert!(!truncated, "the reader reached its own end of the stream");
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn edge_a_writer_past_the_drain_deadline_is_joined_not_detached() {
        let handle = std::thread::spawn(|| -> std::io::Result<()> {
            std::thread::sleep(std::time::Duration::from_millis(50));
            Ok(())
        });

        let writer = super::join_writer(handle, std::time::Instant::now(), || {})
            .expect("a writer that finishes after the deadline still joins");

        assert!(
            writer.is_ok(),
            "the writer's own result is carried back, not replaced by a detach"
        );
    }
}
