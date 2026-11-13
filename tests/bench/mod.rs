//! The benchmark workloads, shared by the `rb-bench` runner and the tests.
//!
//! A measurement harness is only useful if it knows the answer is right. Every
//! workload here returns a value that has a deterministic reference answer, and
//! [`run_workload`] refuses to report a timing for an answer that differs. Wall
//! time is recorded; it is never asserted. The runner in `tests/bench/runner.rs`
//! prints and writes the table, and `tests/bench_test.rs` pins the answers
//! inside the normal test gate.

use std::fmt;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use redblue::analyzer;
use redblue::lexer::Lexer;
use redblue::parser;
use redblue::Value;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// The budget every harness workload runs under.
///
/// A benchmark that hangs must fail instead of waiting forever. Production
/// measurements run in a child process; when the budget expires the parent
/// kills and reaps that child, so a timed-out workload cannot outlive its
/// budget. The budget bounds the wait and the work. A single `run_workload`
/// spends this on one workload; `run_all` spends it once across all four, so
/// the no-argument runner is bounded by one budget rather than four.
pub const DEFAULT_BUDGET: Duration = Duration::from_secs(60);

/// How long the test-only start-marker handshake waits for a child to prove it
/// started before the measurement budget begins. Production never asks for a
/// start marker, so this never extends a real measurement.
const WORKER_START_TIMEOUT: Duration = Duration::from_secs(30);

/// The largest size each workload is allowed to run.
///
/// A larger size is refused before the reference answer or the workload's source
/// string is built, so the harness fails cleanly instead of allocating without
/// bound or burning the shared budget. The boundary test compares against these
/// maxima with [`check_size_supported`], which does not run the workload, so the
/// correctness gate never pays for a maximum-size measurement on an arbitrary
/// machine. The maxima are demonstrated by the measured baseline, not by a timed
/// assertion inside the gate.
///
/// Fibonacci is additionally capped where `f64` still represents every
/// intermediate value exactly (fib(78) < 2^53). Beyond that the expected answer
/// itself is rounded, and far enough out it becomes infinity, which would let
/// `inf == inf` pass as a verified measurement.
const MAX_FIBONACCI_SIZE: i64 = 78;
const MAX_PRIMES_SIZE: i64 = 10_000;
const MAX_CONCAT_SIZE: i64 = 100_000;
const MAX_LEX_SIZE: i64 = 250_000;

/// A workload's name, default size, largest checkable size, and the two
/// functions that make it a benchmark rather than a smoke test.
pub struct Workload {
    pub name: &'static str,
    pub default_size: i64,
    max_size: i64,
    run: fn(i64) -> Result<Value, String>,
    expected: fn(i64) -> Expected,
}

/// The deterministic answer a workload must produce for a given size.
#[derive(Debug, Clone, PartialEq)]
pub enum Expected {
    Number(f64),
    Text(String),
}

impl Expected {
    fn matches(&self, value: &Value) -> bool {
        match (self, value) {
            (Expected::Number(want), Value::Number(got)) => want == got,
            (Expected::Text(want), Value::Text(got)) => want == got,
            _ => false,
        }
    }

    fn render(&self) -> String {
        match self {
            Expected::Number(n) => n.to_string(),
            Expected::Text(text) => text.clone(),
        }
    }
}

/// What one run measured. `answer` is the checked value, not the elapsed time.
#[derive(Debug, Clone)]
pub struct Measurement {
    pub workload: &'static str,
    pub size: i64,
    pub answer: Value,
    pub elapsed: Duration,
}

/// Everything the harness can refuse to do.
#[derive(Debug, Clone, PartialEq)]
pub enum BenchError {
    Usage(String),
    Load(String),
    BudgetExceeded {
        workload: String,
        budget: Duration,
    },
    BudgetExhausted {
        workload: String,
        budget: Duration,
    },
    SizeTooLarge {
        workload: &'static str,
        size: i64,
        max: i64,
    },
    WrongAnswer {
        workload: &'static str,
        size: i64,
        expected: String,
        actual: String,
    },
}

impl fmt::Display for BenchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BenchError::Usage(message) => write!(f, "{message}"),
            BenchError::Load(message) => write!(f, "{message}"),
            BenchError::BudgetExceeded { workload, budget } => write!(
                f,
                "workload '{workload}' did not finish within its {:.3}s budget",
                budget.as_secs_f64()
            ),
            BenchError::BudgetExhausted { workload, budget } => write!(
                f,
                "the shared {:.3}s budget was exhausted before workload '{workload}' could start",
                budget.as_secs_f64()
            ),
            BenchError::SizeTooLarge {
                workload,
                size,
                max,
            } => write!(
                f,
                "workload '{workload}' size {size} exceeds the checkable maximum {max}"
            ),
            BenchError::WrongAnswer {
                workload,
                size,
                expected,
                actual,
            } => write!(
                f,
                "workload '{workload}' at size {size} computed {actual}, expected {expected}"
            ),
        }
    }
}

impl std::error::Error for BenchError {}

/// The four named workloads. Names are stable: `BENCH.md` and the tests use
/// them, and a runner invocation names one.
pub const WORKLOADS: &[Workload] = &[
    Workload {
        name: "fibonacci",
        default_size: 20,
        max_size: MAX_FIBONACCI_SIZE,
        run: run_fibonacci,
        expected: expected_fibonacci,
    },
    Workload {
        name: "primes",
        default_size: 1000,
        max_size: MAX_PRIMES_SIZE,
        run: run_primes,
        expected: expected_primes,
    },
    Workload {
        name: "concat",
        default_size: 200,
        max_size: MAX_CONCAT_SIZE,
        run: run_concat,
        expected: expected_concat,
    },
    Workload {
        name: "lex",
        default_size: 200,
        max_size: MAX_LEX_SIZE,
        run: run_lex,
        expected: expected_lex,
    },
];

/// The hidden workload the child-process timeout test drives through
/// [`run_workload`]. It is not part of [`WORKLOADS`], so it never appears in a
/// baseline, the usage text, or the no-argument run. The worker sleeps for the
/// requested size in milliseconds and writes a start marker before sleeping and
/// a finish marker after, so a test can prove the parent killed it.
pub const HANG_WORKLOAD_NAME: &str = "__hang";

/// The environment variable naming the marker path for [`HANG_WORKLOAD_NAME`].
pub const HANG_SENTINEL_ENV: &str = "RB_BENCH_HANG_SENTINEL";

/// The longest the hidden `--worker __hang` mode may sleep.
///
/// `--worker` is an internal, undocumented path, but it is reachable through
/// the shipped `rb-bench` binary. Without a cap, a caller could ask for a sleep
/// of nearly three hundred million years and pin a process indefinitely. The
/// child-timeout test sleeps ten seconds, so sixty is ample headroom and still
/// bounded.
pub const MAX_HANG_MILLIS: i64 = 60_000;

/// The duration a hidden `__hang` worker should sleep for `size` milliseconds.
///
/// Refuses anything past [`MAX_HANG_MILLIS`] rather than clamping it, so a
/// caller learns the request was rejected instead of silently getting a
/// different sleep.
fn hang_duration(size: i64) -> Result<Duration, BenchError> {
    if size > MAX_HANG_MILLIS {
        return Err(BenchError::Usage(format!(
            "hang size {size} ms exceeds the maximum {MAX_HANG_MILLIS} ms"
        )));
    }
    Ok(Duration::from_millis(size.max(0) as u64))
}

#[cfg(test)]
static HANG_WORKLOAD: Workload = Workload {
    name: HANG_WORKLOAD_NAME,
    default_size: 0,
    max_size: MAX_HANG_MILLIS,
    run: run_hang,
    expected: expected_hang,
};

#[cfg(test)]
fn run_hang(size: i64) -> Result<Value, String> {
    let duration = hang_duration(size).map_err(|error| error.to_string())?;
    thread::sleep(duration);
    Ok(Value::Number(1.0))
}

#[cfg(test)]
fn expected_hang(_size: i64) -> Expected {
    Expected::Number(1.0)
}

pub fn workload_names() -> Vec<&'static str> {
    WORKLOADS.iter().map(|workload| workload.name).collect()
}

fn workload_named(name: &str) -> Result<&'static Workload, BenchError> {
    if let Some(workload) = WORKLOADS.iter().find(|workload| workload.name == name) {
        return Ok(workload);
    }
    #[cfg(test)]
    if name == HANG_WORKLOAD_NAME {
        return Ok(&HANG_WORKLOAD);
    }
    Err(BenchError::Usage(unknown_workload(name)))
}

fn unknown_workload(name: &str) -> String {
    format!(
        "unknown workload '{name}'; choose one of: {}",
        workload_names().join(", ")
    )
}

pub fn usage() -> String {
    format!(
        "Usage: rb-bench [workload [size]]\n       rb-bench --compare <baseline>\n\
         Workloads: {}\n\
         With no arguments, runs all four at their default sizes and writes BENCH.md.\n\
         With --compare, re-runs each recorded row and prints its wall time next to the baseline.",
        workload_names().join(", ")
    )
}

/// Parses a size argument. Zero is legal and means the trivial case; a negative
/// size is a usage error rather than a silently empty loop.
pub fn parse_size(text: &str) -> Result<i64, BenchError> {
    let size: i64 = text
        .parse()
        .map_err(|_| BenchError::Usage(format!("size '{text}' is not a whole number")))?;
    if size < 0 {
        return Err(BenchError::Usage(format!(
            "size must be zero or greater, got {size}"
        )));
    }
    Ok(size)
}

/// Runs a synthetic test closure on a worker thread and gives up after `budget`.
///
/// This is the in-process helper for tests that deliberately hang or break a
/// closure. Real measurements do not use it: [`measure_with_reference`] runs the
/// workload in a child process and kills it on timeout, so a hung benchmark
/// cannot outlive its budget.
#[cfg(test)]
pub fn run_with_budget<T, F>(
    workload: &str,
    budget: Duration,
    f: F,
) -> Result<(T, Duration), BenchError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let start = Instant::now();
        let outcome = f().map(|value| (value, start.elapsed()));
        let _ = sender.send(outcome);
    });

    match receiver.recv_timeout(budget) {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(message)) => Err(BenchError::Load(message)),
        Err(RecvTimeoutError::Timeout) => Err(BenchError::BudgetExceeded {
            workload: workload.to_string(),
            budget,
        }),
        Err(RecvTimeoutError::Disconnected) => Err(BenchError::Load(format!(
            "workload '{workload}' stopped without producing an answer"
        ))),
    }
}

fn check_expected(
    workload: &'static Workload,
    size: i64,
    expected: Expected,
    answer: Value,
    elapsed: Duration,
) -> Result<Measurement, BenchError> {
    if !expected.matches(&answer) {
        return Err(BenchError::WrongAnswer {
            workload: workload.name,
            size,
            expected: expected.render(),
            actual: answer.to_string(),
        });
    }
    Ok(Measurement {
        workload: workload.name,
        size,
        answer,
        elapsed,
    })
}

#[cfg(test)]
fn measure(
    workload: &'static Workload,
    size: i64,
    budget: Duration,
    expected: Expected,
) -> Result<Measurement, BenchError> {
    let (answer, elapsed) = run_with_budget(workload.name, budget, move || (workload.run)(size))?;
    check_expected(workload, size, expected, answer, elapsed)
}

/// The reference answer is computed after the size check, so it is bounded by
/// the workload's checkable maximum and cannot hang before the budget starts.
/// The workload itself runs in a child process so a timeout can kill it.
fn measure_with_reference(
    workload: &'static Workload,
    size: i64,
    budget: Duration,
) -> Result<Measurement, BenchError> {
    ensure_workload_size_supported(workload, size)?;
    let (answer, elapsed) = measure_in_worker(workload, size, budget)?;
    let expected = (workload.expected)(size);
    check_expected(workload, size, expected, answer, elapsed)
}

fn measure_in_worker(
    workload: &'static Workload,
    size: i64,
    budget: Duration,
) -> Result<(Value, Duration), BenchError> {
    measure_in_worker_with_env(workload, size, budget, &[], None)
}

fn measure_in_worker_with_env(
    workload: &'static Workload,
    size: i64,
    budget: Duration,
    env: &[(&std::ffi::OsStr, &std::ffi::OsStr)],
    start_marker: Option<&Path>,
) -> Result<(Value, Duration), BenchError> {
    if let Some(binary) = worker_binary() {
        return measure_in_child(&binary, workload, size, budget, env, start_marker);
    }
    let _ = (env, start_marker);
    Err(BenchError::Load(format!(
        "cannot locate the rb-bench worker binary to measure '{}'; the child-process \
         measurement path must not silently fall back to an in-process run",
        workload.name
    )))
}

fn worker_binary() -> Option<PathBuf> {
    if let Some(path) = option_env!("CARGO_BIN_EXE_rb-bench") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let current = std::env::current_exe().ok()?;
    if current.file_stem().and_then(|stem| stem.to_str()) == Some("rb-bench") {
        return Some(current);
    }
    // `cargo test` runs this module both through `tests/bench_test.rs` and as
    // the `rb-bench` binary's own unit tests. In the latter the test harness
    // lives under `target/<profile>/build/...`, so the runner sits several
    // levels above it rather than beside it. Find Cargo's target directory and
    // resolve only the current profile's runner, so a stale binary elsewhere on
    // the ancestor path can never be measured; never fall back to an in-process
    // run.
    let name = format!("rb-bench{}", std::env::consts::EXE_SUFFIX);
    let target = current.ancestors().find(|directory| {
        directory.join(".rustc_info.json").is_file() || directory.join("CACHEDIR.TAG").is_file()
    })?;
    let relative = current.strip_prefix(target).ok()?;
    runner_binary_in_target(target, relative, &name)
}

/// Resolve the runner inside Cargo's target directory.
///
/// A host build places it at `target/<profile>/<name>`; a cross-target build
/// adds a triple level, `target/<triple>/<profile>/<name>`. Try the direct
/// layout first and then the nested one instead of assuming the first component
/// after the target directory is always the profile.
fn runner_binary_in_target(target: &Path, relative: &Path, name: &str) -> Option<PathBuf> {
    let mut components = relative.components();
    let first = components.next()?;
    let direct = target.join(first.as_os_str()).join(name);
    if direct.is_file() {
        return Some(direct);
    }
    let second = components.next()?;
    let nested = target
        .join(first.as_os_str())
        .join(second.as_os_str())
        .join(name);
    nested.is_file().then_some(nested)
}

fn measure_in_child(
    binary: &Path,
    workload: &'static Workload,
    size: i64,
    budget: Duration,
    env: &[(&std::ffi::OsStr, &std::ffi::OsStr)],
    start_marker: Option<&Path>,
) -> Result<(Value, Duration), BenchError> {
    let mut command = Command::new(binary);
    command
        .arg("--worker")
        .arg(workload.name)
        .arg(size.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (key, value) in env {
        command.env(key, value);
    }
    let mut child = command.spawn().map_err(|error| {
        BenchError::Load(format!(
            "could not start the '{}' worker: {error}",
            workload.name
        ))
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| BenchError::Load(format!("the '{}' worker has no stdout", workload.name)))?;
    let (sender, receiver) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut output = String::new();
        let result = reader.read_to_string(&mut output).map(|_| output);
        let _ = sender.send(result);
    });

    if let Some(marker) = start_marker {
        let deadline = Instant::now() + WORKER_START_TIMEOUT;
        loop {
            if marker.is_file() {
                break;
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    let _ = reader.join();
                    return Err(BenchError::Load(format!(
                        "workload '{}' worker exited with {status} before writing its start marker {}",
                        workload.name,
                        marker.display()
                    )));
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Err(BenchError::Load(format!(
                        "could not wait for the '{}' worker start marker: {error}",
                        workload.name
                    )));
                }
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(BenchError::Load(format!(
                    "workload '{}' worker did not write its start marker {} within {:?}",
                    workload.name,
                    marker.display(),
                    WORKER_START_TIMEOUT
                )));
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    match receiver.recv_timeout(budget) {
        Ok(Ok(output)) => {
            let status = child.wait().map_err(|error| {
                BenchError::Load(format!(
                    "could not reap the '{}' worker: {error}",
                    workload.name
                ))
            })?;
            let _ = reader.join();
            let parsed = parse_worker_output(workload.name, size, &output);
            if !status.success() {
                return match parsed {
                    Err(error @ BenchError::WrongAnswer { .. }) => Err(error),
                    _ => Err(BenchError::Load(format!(
                        "workload '{}' worker exited with {status}: {}",
                        workload.name,
                        output.trim()
                    ))),
                };
            }
            parsed
        }
        Ok(Err(error)) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            Err(BenchError::Load(format!(
                "could not read the '{}' worker output: {error}",
                workload.name
            )))
        }
        Err(RecvTimeoutError::Timeout) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            Err(BenchError::BudgetExceeded {
                workload: workload.name.to_string(),
                budget,
            })
        }
        Err(RecvTimeoutError::Disconnected) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            Err(BenchError::Load(format!(
                "workload '{}' stopped without producing an answer",
                workload.name
            )))
        }
    }
}

#[cfg(not(test))]
fn measure_direct(workload: &'static Workload, size: i64) -> Result<Measurement, BenchError> {
    ensure_workload_size_supported(workload, size)?;
    let expected = (workload.expected)(size);
    let started = Instant::now();
    let answer = (workload.run)(size).map_err(BenchError::Load)?;
    let elapsed = started.elapsed();
    check_expected(workload, size, expected, answer, elapsed)
}

/// Runs one measurement for the hidden child-process mode and prints a single
/// machine-readable line on stdout. The parent kills this process on timeout.
#[cfg(not(test))]
pub fn run_worker(arguments: &[String]) -> i32 {
    if arguments.len() != 2 {
        println!("ERR worker mode expects a workload and a size");
        return 2;
    }
    if arguments[0] == HANG_WORKLOAD_NAME {
        let size = match parse_size(&arguments[1]) {
            Ok(size) => size,
            Err(error) => {
                println!("ERR {error}");
                return 1;
            }
        };
        let duration = match hang_duration(size) {
            Ok(duration) => duration,
            Err(error) => {
                println!("ERR {error}");
                return 1;
            }
        };
        if let Some(base) = std::env::var_os(HANG_SENTINEL_ENV) {
            let base = PathBuf::from(base);
            let _ = std::fs::write(base.with_extension("started"), b"started");
            thread::sleep(duration);
            let _ = std::fs::write(base.with_extension("finished"), b"finished");
        } else {
            thread::sleep(duration);
        }
        println!("OK 0 n {:016x}", 1.0f64.to_bits());
        return 0;
    }
    let workload = match workload_named(&arguments[0]) {
        Ok(workload) => workload,
        Err(error) => {
            println!("ERR {error}");
            return 1;
        }
    };
    let size = match parse_size(&arguments[1]) {
        Ok(size) => size,
        Err(error) => {
            println!("ERR {error}");
            return 1;
        }
    };
    match measure_direct(workload, size) {
        Ok(measurement) => match encode_worker_answer(&measurement) {
            Ok(line) => {
                println!("{line}");
                0
            }
            Err(error) => {
                println!("ERR {error}");
                1
            }
        },
        Err(error) => {
            println!("{}", encode_worker_error(&error));
            1
        }
    }
}

fn encode_worker_error(error: &BenchError) -> String {
    match error {
        BenchError::WrongAnswer {
            expected, actual, ..
        } => format!(
            "WRONG {} {}",
            encode_hex(expected.as_bytes()),
            encode_hex(actual.as_bytes())
        ),
        other => format!("ERR {other}"),
    }
}

#[cfg(not(test))]
fn encode_worker_answer(measurement: &Measurement) -> Result<String, BenchError> {
    let nanos = measurement.elapsed.as_nanos();
    match &measurement.answer {
        Value::Number(number) => Ok(format!("OK {nanos} n {:016x}", number.to_bits())),
        Value::Text(text) => Ok(format!("OK {nanos} t {}", encode_hex(text.as_bytes()))),
        other => Err(BenchError::Load(format!(
            "workload '{}' returned an unsupported answer kind: {other:?}",
            measurement.workload
        ))),
    }
}

fn parse_worker_output(
    workload: &'static str,
    size: i64,
    output: &str,
) -> Result<(Value, Duration), BenchError> {
    let line = output
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default();
    if let Some(payload) = line.strip_prefix("WRONG ") {
        let mut fields = payload.splitn(2, ' ');
        let (Some(expected), Some(actual)) = (fields.next(), fields.next()) else {
            return Err(BenchError::Load(format!(
                "workload '{workload}' worker produced an unreadable wrong-answer line: {line:?}"
            )));
        };
        let expected = String::from_utf8(decode_hex(expected).map_err(|error| {
            BenchError::Load(format!(
                "workload '{workload}' worker reported an unreadable expected answer: {error}"
            ))
        })?)
        .map_err(|error| {
            BenchError::Load(format!(
                "workload '{workload}' worker reported a non-UTF-8 expected answer: {error}"
            ))
        })?;
        let actual = String::from_utf8(decode_hex(actual).map_err(|error| {
            BenchError::Load(format!(
                "workload '{workload}' worker reported an unreadable actual answer: {error}"
            ))
        })?)
        .map_err(|error| {
            BenchError::Load(format!(
                "workload '{workload}' worker reported a non-UTF-8 actual answer: {error}"
            ))
        })?;
        return Err(BenchError::WrongAnswer {
            workload,
            size,
            expected,
            actual,
        });
    }
    if let Some(message) = line.strip_prefix("ERR ") {
        return Err(BenchError::Load(format!(
            "workload '{workload}' failed: {message}"
        )));
    }
    let mut parts = line.splitn(4, ' ');
    let (Some("OK"), Some(nanos), Some(tag), Some(payload)) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(BenchError::Load(format!(
            "workload '{workload}' worker produced an unreadable answer: {line:?}"
        )));
    };
    let nanos: u64 = nanos.parse().map_err(|_| {
        BenchError::Load(format!(
            "workload '{workload}' worker reported an unreadable duration: {nanos:?}"
        ))
    })?;
    let value = match tag {
        "n" => {
            let bits = u64::from_str_radix(payload, 16).map_err(|_| {
                BenchError::Load(format!(
                    "workload '{workload}' worker reported an unreadable number: {payload:?}"
                ))
            })?;
            Value::Number(f64::from_bits(bits))
        }
        "t" => {
            let bytes = decode_hex(payload).map_err(|error| {
                BenchError::Load(format!(
                    "workload '{workload}' worker reported unreadable text: {error}"
                ))
            })?;
            let text = String::from_utf8(bytes).map_err(|error| {
                BenchError::Load(format!(
                    "workload '{workload}' worker reported non-UTF-8 text: {error}"
                ))
            })?;
            Value::Text(text)
        }
        other => {
            return Err(BenchError::Load(format!(
                "workload '{workload}' worker reported an unknown answer tag: {other:?}"
            )))
        }
    };
    Ok((value, Duration::from_nanos(nanos)))
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

fn decode_hex(text: &str) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(2) {
        return Err("odd-length hex payload".to_string());
    }
    let mut decoded = Vec::with_capacity(text.len() / 2);
    for pair in text.as_bytes().chunks(2) {
        let high = hex_digit(pair[0])?;
        let low = hex_digit(pair[1])?;
        decoded.push((high << 4) | low);
    }
    Ok(decoded)
}

fn hex_digit(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!("invalid hex digit {:?}", byte as char)),
    }
}

/// Refuses a size whose reference answer cannot be materialized safely.
///
/// The alternative is an allocation abort or an unbounded loop on the caller's
/// thread. This is a distinct error from a timeout because the workload never
/// started: telling the user it ran out of budget would be a false statement
/// about elapsed work.
fn ensure_workload_size_supported(
    workload: &'static Workload,
    size: i64,
) -> Result<(), BenchError> {
    if size > workload.max_size {
        return Err(BenchError::SizeTooLarge {
            workload: workload.name,
            size,
            max: workload.max_size,
        });
    }
    Ok(())
}

/// Checks a size against a workload's checkable maximum without running it.
///
/// Exposed so a test can pin `max_size` and `max_size + 1` without
/// materializing a multi-million-element answer.
#[cfg(test)]
pub fn check_size_supported(name: &str, size: i64) -> Result<(), BenchError> {
    if size < 0 {
        return Err(BenchError::Usage(format!(
            "size must be zero or greater, got {size}"
        )));
    }
    let workload = workload_named(name)?;
    ensure_workload_size_supported(workload, size)
}

/// The largest size whose reference answer a named workload can check.
#[cfg(test)]
pub fn max_size(name: &str) -> Result<i64, BenchError> {
    Ok(workload_named(name)?.max_size)
}

/// Measures one named workload at one size, enforcing both the budget and the
/// reference answer. A wrong answer is an error, never a fast success.
pub fn run_workload(name: &str, size: i64, budget: Duration) -> Result<Measurement, BenchError> {
    if size < 0 {
        return Err(BenchError::Usage(format!(
            "size must be zero or greater, got {size}"
        )));
    }
    let workload = workload_named(name)?;
    measure_with_reference(workload, size, budget)
}

/// Measures one named workload at one size against a caller-supplied answer.
///
/// `run_workload` supplies the workload's own reference answer. Tests use this
/// to drive the wrong-answer refusal with a deliberately wrong one.
#[cfg(test)]
pub fn run_workload_with(
    name: &str,
    size: i64,
    budget: Duration,
    expected: Expected,
) -> Result<Measurement, BenchError> {
    if size < 0 {
        return Err(BenchError::Usage(format!(
            "size must be zero or greater, got {size}"
        )));
    }
    let workload = workload_named(name)?;
    ensure_workload_size_supported(workload, size)?;
    measure(workload, size, budget, expected)
}

/// Measures one named workload in a child with an explicit environment and an
/// optional start-marker handshake.
///
/// Tests use this to drive the hidden sleeping worker without mutating the
/// process-global environment that sibling tests share. The handshake lets the
/// test start the budget only after the child has proved it is running, so a
/// slow spawn cannot make a timeout test flaky.
#[cfg(test)]
pub fn run_workload_with_env(
    name: &str,
    size: i64,
    budget: Duration,
    env: &[(&std::ffi::OsStr, &std::ffi::OsStr)],
    start_marker: Option<&Path>,
) -> Result<Measurement, BenchError> {
    if size < 0 {
        return Err(BenchError::Usage(format!(
            "size must be zero or greater, got {size}"
        )));
    }
    let workload = workload_named(name)?;
    ensure_workload_size_supported(workload, size)?;
    let (answer, elapsed) = measure_in_worker_with_env(workload, size, budget, env, start_marker)?;
    let expected = (workload.expected)(size);
    check_expected(workload, size, expected, answer, elapsed)
}

pub fn default_size(name: &str) -> Result<i64, BenchError> {
    Ok(workload_named(name)?.default_size)
}

/// One table for all workloads, with wall time in milliseconds.
///
/// Columns are padded by terminal display width rather than by character count,
/// so a wide (CJK) answer occupies the same number of cells as any other and
/// the columns stay aligned in a terminal.
pub fn render_table(rows: &[Measurement]) -> String {
    let mut table = String::new();
    table.push_str(&format!(
        "{} {} {} {}\n",
        pad_right("workload", 12),
        pad_left("size", 8),
        pad_left("answer", 20),
        pad_left("wall time", 14)
    ));
    for row in rows {
        let answer = truncate(&row.answer.to_string(), 20);
        let elapsed = format!("{:.3} ms", row.elapsed.as_secs_f64() * 1000.0);
        table.push_str(&format!(
            "{} {} {} {}\n",
            pad_right(row.workload, 12),
            pad_left(&row.size.to_string(), 8),
            pad_left(&answer, 20),
            pad_left(&elapsed, 14)
        ));
    }
    table
}

/// Pad `text` on the left with spaces until it occupies `width` terminal cells.
fn pad_left(text: &str, width: usize) -> String {
    let padding = width.saturating_sub(text.width());
    let mut padded = String::with_capacity(text.len() + padding);
    for _ in 0..padding {
        padded.push(' ');
    }
    padded.push_str(text);
    padded
}

/// Pad `text` on the right with spaces until it occupies `width` terminal cells.
fn pad_right(text: &str, width: usize) -> String {
    let padding = width.saturating_sub(text.width());
    let mut padded = String::with_capacity(text.len() + padding);
    padded.push_str(text);
    for _ in 0..padding {
        padded.push(' ');
    }
    padded
}

/// Shorten `text` to at most `width` terminal cells, ending in `...` when it
/// does not fit.
///
/// The budget is counted in display cells, not Unicode scalar values, so a wide
/// (CJK) answer is cut when the terminal column budget is exhausted rather than
/// at a character count that would draw past the column. The ellipsis is part of
/// the budget.
fn truncate(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    let budget = width.saturating_sub(3);
    let mut short = String::new();
    let mut used = 0;
    for character in text.chars() {
        let cells = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + cells > budget {
            break;
        }
        short.push(character);
        used += cells;
    }
    short.push_str("...");
    short
}

/// The checked-in baseline. It records both the verified answer and the wall
/// time measured on the landing machine named below. The answer is what the
/// tests assert; the wall time is recorded so a later phase can compare against
/// it with `rb-bench --compare BENCH.md`, and is never asserted because timing
/// assertions are flakes wearing a gate's clothes.
///
/// Timings are only comparable on the same machine and build: a debug build on
/// one CPU is not a release build on another. Cross-machine runs should compare
/// the ratio between workloads rather than the absolute milliseconds. The
/// per-run record lives in the untracked `BENCH_TIMINGS.md`; this file is the
/// landing-machine reference.
pub fn render_markdown(rows: &[Measurement], hardware: &str) -> String {
    let mut markdown = String::new();
    markdown.push_str("# Redblue benchmarks\n\n");
    markdown.push_str(
        "Generated by `cargo run --bin rb-bench`. This checked-in baseline \
         records the verified answer and the wall time measured on the landing \
         machine named below; the untracked `BENCH_TIMINGS.md` holds the record \
         for the latest run. Only the answer is asserted by the test gate. The \
         wall time is a debug-build reference: compare it on the same machine \
         with `rb-bench --compare BENCH.md`, or compare the ratio between \
         workloads across machines. Timings vary by machine and by run and are \
         never asserted. The hardware line is best-effort; on a platform that \
         does not expose a CPU model it says so explicitly.\n\n",
    );
    markdown.push_str("| Workload | Size | Answer | Wall time |\n");
    markdown.push_str("|---|---:|---:|---:|\n");
    for row in rows {
        markdown.push_str(&format!(
            "| {} | {} | {} | {:.3} ms |\n",
            row.workload,
            row.size,
            row.answer,
            row.elapsed.as_secs_f64() * 1000.0
        ));
    }
    markdown.push_str("\n## Hardware\n\n");
    markdown.push_str(&format!("- {hardware}\n"));
    markdown.push_str(&format!(
        "- Build: debug (`cargo run --bin rb-bench`), {} {}\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    markdown
}

/// Parses the wall-time cell of a `BENCH.md` row, for example `"1.234 ms"`.
///
/// The baseline records a timing so a later phase can compare against it, but
/// no test asserts its value. This parser only proves the recorded cell is a
/// finite, non-negative number of milliseconds: a corrupted baseline must be a
/// clean error, not a silently ignored row.
pub fn parse_wall_time_ms(text: &str) -> Result<f64, String> {
    let trimmed = text.trim();
    let number = trimmed
        .strip_suffix("ms")
        .ok_or_else(|| format!("wall time '{trimmed}' is missing its ms unit"))?
        .trim();
    let value: f64 = number
        .parse()
        .map_err(|_| format!("wall time '{trimmed}' is not a number of milliseconds"))?;
    if !value.is_finite() || value < 0.0 {
        return Err(format!(
            "wall time '{trimmed}' must be finite and non-negative"
        ));
    }
    Ok(value)
}

/// The untracked per-run timing record for the machine that just ran the
/// harness. It carries the same columns as the checked-in `BENCH.md`, which is
/// the landing-machine reference; neither file's wall times are ever asserted.
pub fn render_timings(rows: &[Measurement], hardware: &str) -> String {
    let mut markdown = String::new();
    markdown.push_str("# Redblue benchmark timings\n\n");
    markdown.push_str(
        "Per-run record, machine-specific and not checked in. The checked-in \
         landing-machine baseline is `BENCH.md`; this file records the latest \
         local run. Wall time is printed and recorded here, but never asserted: \
         timing assertions are flakes wearing a gate's clothes.\n\n",
    );
    markdown.push_str("| Workload | Size | Answer | Wall time |\n");
    markdown.push_str("|---|---:|---:|---:|\n");
    for row in rows {
        markdown.push_str(&format!(
            "| {} | {} | {} | {:.3} ms |\n",
            row.workload,
            row.size,
            row.answer,
            row.elapsed.as_secs_f64() * 1000.0
        ));
    }
    markdown.push_str("\n## Hardware\n\n");
    markdown.push_str(&format!("- {hardware}\n"));
    markdown
}

/// Best-effort hardware note for the machine that produced `BENCH.md`.
///
/// When the platform does not expose a CPU model the note says so explicitly
/// rather than pretending the empty string is a machine name, so a baseline
/// written on that platform is not silently unidentifiable.
pub fn hardware_note() -> String {
    let cpu = cpu_model().unwrap_or_else(|| {
        format!(
            "unknown CPU ({} {} does not expose a model name)",
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    });
    format!(
        "CPU: {cpu} ({} CPUs)",
        std::thread::available_parallelism().map_or(1, |n| n.get())
    )
}

#[cfg(target_os = "linux")]
fn cpu_model() -> Option<String> {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("model name"))
                .and_then(|line| line.split(':').nth(1))
                .map(str::trim)
                .filter(|model| !model.is_empty())
                .map(str::to_string)
        })
}

#[cfg(target_os = "macos")]
fn cpu_model() -> Option<String> {
    let output = std::process::Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let model = String::from_utf8(output.stdout).ok()?;
    let model = model.trim();
    if model.is_empty() {
        None
    } else {
        Some(model.to_string())
    }
}

#[cfg(target_os = "windows")]
fn cpu_model() -> Option<String> {
    std::env::var("PROCESSOR_IDENTIFIER")
        .ok()
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn cpu_model() -> Option<String> {
    None
}

/// The path `BENCH.md` is written to and read from: the current working
/// directory, or an explicit `RB_BENCH_OUTPUT` override.
///
/// The build-time source directory is deliberately not used. An installed
/// `rb-bench` may run where that directory is read-only or absent, and a
/// measurement should land where the user invoked it.
pub fn bench_md_path() -> std::path::PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    bench_md_path_from(std::env::var_os("RB_BENCH_OUTPUT").as_deref(), &cwd)
}

pub fn bench_md_path_from(
    override_path: Option<&std::ffi::OsStr>,
    cwd: &Path,
) -> std::path::PathBuf {
    match override_path {
        Some(path) => std::path::PathBuf::from(path),
        None => cwd.join("BENCH.md"),
    }
}

/// The repository's checked-in `BENCH.md`, resolved from the crate root.
///
/// Tests read this rather than [`bench_md_path`]: the runner is allowed to
/// redirect its output with `RB_BENCH_OUTPUT` or by changing directory, but a
/// test that validates the checked-in baseline must always read this checkout's
/// file, never a foreign artifact from the environment it happens to run in.
#[cfg(test)]
pub fn checked_in_baseline_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("BENCH.md")
}

/// Where the untracked timing artifact for `output` is written.
///
/// The artifact is a sibling of a file output so it lands beside `BENCH.md`.
/// When `output` is a directory, writing the baseline there fails; the timing
/// artifact stays inside that directory rather than replacing its name and
/// escaping to the parent.
pub fn timings_path(output: &Path) -> std::path::PathBuf {
    if output.is_dir() {
        output.join("BENCH_TIMINGS.md")
    } else {
        output
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("BENCH_TIMINGS.md")
    }
}

/// One row read from a checked-in `BENCH.md`, used by `--compare`.
#[derive(Debug, Clone, PartialEq)]
pub struct BaselineRow {
    pub workload: String,
    pub size: i64,
    pub answer: String,
    pub wall_time_ms: f64,
}

/// True for the table's header row, whose first cell names the workload column.
fn is_table_header(cells: &[&str]) -> bool {
    cells
        .first()
        .is_some_and(|cell| cell.eq_ignore_ascii_case("workload"))
}

/// True for a markdown separator row such as `|---|---:|---:|---:|`.
fn is_table_separator(cells: &[&str]) -> bool {
    !cells.is_empty()
        && cells.iter().all(|cell| {
            !cell.is_empty() && cell.chars().all(|character| matches!(character, '-' | ':'))
        })
}

/// Parses the data rows of a `BENCH.md` table.
///
/// Header, separator, and prose lines are ignored. A row that names a known
/// workload but carries the wrong number of cells, an unreadable size, or an
/// unreadable wall time is an error rather than a silently skipped row, so a
/// corrupted baseline cannot compare as if it had fewer workloads. Any other
/// four-cell row is an error too, whether or not its size cell parses: a
/// misspelled workload name must not silently shrink the baseline. A second
/// row for a workload is refused, whatever its size: the baseline records one
/// row per workload at its default size, and `--compare` re-runs each row, so
/// two sizes would render two current rows for one workload.
pub fn parse_baseline_markdown(text: &str) -> Result<Vec<BaselineRow>, String> {
    let mut rows: Vec<BaselineRow> = Vec::new();
    let known = workload_names();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if is_table_header(&cells) || is_table_separator(&cells) {
            continue;
        }
        if cells.len() != 4 {
            if cells.first().is_some_and(|cell| known.contains(cell)) {
                return Err(format!(
                    "baseline row for `{}` has {} cells, expected 4 (workload, size, answer, wall time)",
                    cells[0],
                    cells.len()
                ));
            }
            continue;
        }
        if !known.contains(&cells[0]) {
            return Err(format!(
                "baseline row names unknown workload `{}`; choose one of: {}",
                cells[0],
                known.join(", ")
            ));
        }
        let size: i64 = cells[1].parse().map_err(|_| {
            format!(
                "baseline size for `{}` is not a whole number: {:?}",
                cells[0], cells[1]
            )
        })?;
        if size < 0 {
            return Err(format!(
                "baseline size for `{}` must be zero or greater, got {size}",
                cells[0]
            ));
        }
        if let Some(workload) = WORKLOADS.iter().find(|workload| workload.name == cells[0]) {
            if size > workload.max_size {
                return Err(format!(
                    "baseline size {size} for `{}` exceeds the checkable maximum {}",
                    cells[0], workload.max_size
                ));
            }
        }
        if rows.iter().any(|row| row.workload == cells[0]) {
            return Err(format!(
                "baseline has more than one row for `{}`; the baseline records one row per \
                 workload at its default size",
                cells[0]
            ));
        }
        let wall_time_ms = parse_wall_time_ms(cells[3])
            .map_err(|error| format!("baseline wall time for `{}`: {error}", cells[0]))?;
        rows.push(BaselineRow {
            workload: cells[0].to_string(),
            size,
            answer: cells[2].to_string(),
            wall_time_ms,
        });
    }
    Ok(rows)
}

/// Formats the advisory `--compare` table. A zero baseline shows `n/a` rather
/// than an infinite ratio. Nothing here is a pass/fail gate.
pub fn render_comparison(baseline: &[BaselineRow], current: &[Measurement]) -> String {
    let mut table = String::new();
    table.push_str(&format!(
        "{:<12} {:>8} {:>16} {:>14} {:>10}\n",
        "workload", "size", "baseline", "current", "ratio"
    ));
    for row in baseline {
        let measured = current.iter().find(|measurement| {
            measurement.workload == row.workload && measurement.size == row.size
        });
        let current_ms = measured.map(|measurement| measurement.elapsed.as_secs_f64() * 1000.0);
        let current_cell = current_ms.map_or_else(
            || "not run".to_string(),
            |milliseconds| format!("{milliseconds:.3} ms"),
        );
        let ratio = match current_ms {
            Some(milliseconds) if row.wall_time_ms > 0.0 => {
                format!("{:.2}x", milliseconds / row.wall_time_ms)
            }
            _ => "n/a".to_string(),
        };
        table.push_str(&format!(
            "{:<12} {:>8} {:>16} {:>14} {:>10}\n",
            row.workload,
            row.size,
            format!("{:.3} ms", row.wall_time_ms),
            current_cell,
            ratio
        ));
    }
    table
}

/// Re-runs every row recorded in `path` and prints the current wall time next
/// to the baseline. The ratios are advisory and are never a gate.
fn compare_with_baseline(path: &Path) -> i32 {
    match comparison_table(path) {
        Ok(table) => {
            print!("{table}");
            println!(
                "\nRatios are advisory and only comparable on the same machine and build; \
                 wall time is never asserted."
            );
            0
        }
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

/// Reads `path`, re-runs every recorded row under one shared budget, and
/// returns the advisory comparison table.
pub fn comparison_table(path: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let baseline = parse_baseline_markdown(&text)
        .map_err(|error| format!("could not parse {}: {error}", path.display()))?;
    if baseline.is_empty() {
        return Err(format!(
            "{} has no benchmark rows to compare",
            path.display()
        ));
    }
    let missing: Vec<&str> = WORKLOADS
        .iter()
        .map(|workload| workload.name)
        .filter(|name| !baseline.iter().any(|row| row.workload == *name))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "{} is missing benchmark rows for: {}",
            path.display(),
            missing.join(", ")
        ));
    }

    let started = Instant::now();
    let mut current = Vec::with_capacity(baseline.len());
    for row in &baseline {
        let remaining = DEFAULT_BUDGET.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err(BenchError::BudgetExhausted {
                workload: row.workload.clone(),
                budget: DEFAULT_BUDGET,
            }
            .to_string());
        }
        let measurement =
            run_workload(&row.workload, row.size, remaining).map_err(|error| error.to_string())?;
        let actual = measurement.answer.to_string();
        if actual != row.answer {
            return Err(format!(
                "baseline answer for `{}` at size {} is {:?}, but the workload computed {actual:?}",
                row.workload, row.size, row.answer
            ));
        }
        current.push(measurement);
    }

    Ok(render_comparison(&baseline, &current))
}

/// Runs every workload at its default size, in declaration order.
///
/// The whole run shares one budget rather than granting each workload a fresh
/// one, so the no-argument runner cannot block for four times the advertised
/// bound before reporting a failure.
pub fn run_all(budget: Duration) -> Result<Vec<Measurement>, BenchError> {
    let started = Instant::now();
    let mut rows = Vec::with_capacity(WORKLOADS.len());
    for workload in WORKLOADS {
        let remaining = budget.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err(BenchError::BudgetExhausted {
                workload: workload.name.to_string(),
                budget,
            });
        }
        rows.push(run_workload(
            workload.name,
            workload.default_size,
            remaining,
        )?);
    }
    Ok(rows)
}

/// The runner and its exit codes. Kept here so the binary is a one-line shell
/// and the argument handling is testable.
pub fn run_cli(arguments: &[String]) -> i32 {
    run_cli_with_path(arguments, &bench_md_path())
}

/// The action a command line selects, resolved before anything is measured.
///
/// Exposed so a test can assert that a named workload without an explicit size
/// resolves to its declared default size, rather than only asserting that the
/// command exited zero. `run_cli_with_path` selects through this same function,
/// so the assertion covers the shipped selection path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliSelection<'a> {
    /// Measure one workload at an explicit or default size.
    Measure { name: &'a str, size: i64 },
    /// Print usage and succeed.
    Help,
    /// Compare against the recorded baseline at this path.
    Compare(&'a str),
    /// Measure every workload at its default size and write the baseline.
    All,
}

/// Resolve the command line into the action it selects.
pub fn select_cli(arguments: &[String]) -> Result<CliSelection<'_>, BenchError> {
    if arguments
        .first()
        .is_some_and(|argument| argument == "--compare")
    {
        return match arguments {
            [_, baseline] => Ok(CliSelection::Compare(baseline)),
            _ => Err(BenchError::Usage(
                "--compare takes exactly one baseline path".to_string(),
            )),
        };
    }

    match arguments {
        [] => Ok(CliSelection::All),
        [name] if name == "--help" || name == "-h" => Ok(CliSelection::Help),
        [name] => Ok(CliSelection::Measure {
            name: name.as_str(),
            size: default_size(name)?,
        }),
        [name, size] => {
            let size = parse_size(size)?;
            default_size(name)?;
            Ok(CliSelection::Measure {
                name: name.as_str(),
                size,
            })
        }
        _ => Err(BenchError::Usage(
            "too many arguments: expected a workload and an optional size".to_string(),
        )),
    }
}

/// `run_cli` with an injectable output path, so the no-argument branch and the
/// write-failure branch can be exercised without touching the checked-in
/// `BENCH.md`.
pub fn run_cli_with_path(arguments: &[String], output: &Path) -> i32 {
    let selection = match select_cli(arguments) {
        Ok(selection) => selection,
        Err(error) => {
            eprintln!("{error}");
            eprintln!("{}", usage());
            return 2;
        }
    };

    match selection {
        CliSelection::Compare(baseline) => compare_with_baseline(Path::new(baseline)),
        CliSelection::All => {
            let rows = match run_all(DEFAULT_BUDGET) {
                Ok(rows) => rows,
                Err(error) => {
                    eprintln!("{error}");
                    return 1;
                }
            };
            print!("{}", render_table(&rows));
            let hardware = hardware_note();
            let timings = timings_path(output);
            if let Err(error) = std::fs::write(&timings, render_timings(&rows, &hardware)) {
                eprintln!("could not write {}: {error}", timings.display());
                return 1;
            }
            let markdown = render_markdown(&rows, &hardware);
            if let Err(error) = std::fs::write(output, markdown) {
                eprintln!("could not write {}: {error}", output.display());
                return 1;
            }
            println!("wrote {} and {}", output.display(), timings.display());
            0
        }
        CliSelection::Help => {
            println!("{}", usage());
            0
        }
        CliSelection::Measure { name, size } => match run_workload(name, size, DEFAULT_BUDGET) {
            Ok(row) => {
                print!("{}", render_table(&[row]));
                0
            }
            Err(error) => {
                eprintln!("{error}");
                1
            }
        },
    }
}

// --- fibonacci ---------------------------------------------------------------

fn run_fibonacci(size: i64) -> Result<Value, String> {
    let source = format!(
        "set result to 0\n\
         if {size} is greater than 0 then\n\
             set previous to 0\n\
             set current to 1\n\
             if {size} is greater than 1 then\n\
                 for each i from 2 to {size}\n\
                     set next to previous + current\n\
                     set previous to current\n\
                     set current to next\n\
                 end\n\
                 set result to current\n\
             else\n\
                 set result to 1\n\
             end\n\
         end\n\
         result\n"
    );
    run_source(&source)
}

fn expected_fibonacci(size: i64) -> Expected {
    if size <= 0 {
        return Expected::Number(0.0);
    }
    if size == 1 {
        return Expected::Number(1.0);
    }
    let (mut previous, mut current) = (0.0, 1.0);
    for _ in 2..=size {
        let next = previous + current;
        previous = current;
        current = next;
    }
    Expected::Number(current)
}

// --- primes ------------------------------------------------------------------

fn run_primes(size: i64) -> Result<Value, String> {
    let source = format!(
        "set tally to 0\n\
         for each candidate from 2 to {size}\n\
             set prime to yes\n\
             set divisor to 2\n\
             while divisor * divisor is less than or equal to candidate\n\
                 if candidate mod divisor is 0 then\n\
                     set prime to no\n\
                 end\n\
                 set divisor to divisor + 1\n\
             end\n\
             if prime then\n\
                 set tally to tally + 1\n\
             end\n\
         end\n\
         tally\n"
    );
    run_source(&source)
}

fn expected_primes(size: i64) -> Expected {
    Expected::Number(primes_below(size) as f64)
}

fn primes_below(size: i64) -> u64 {
    if size < 2 {
        return 0;
    }
    let limit = size as usize;
    let mut composite = vec![false; limit + 1];
    let mut count = 0;
    for candidate in 2..=limit {
        if composite[candidate] {
            continue;
        }
        count += 1;
        let mut multiple = candidate * candidate;
        while multiple <= limit {
            composite[multiple] = true;
            multiple += candidate;
        }
    }
    count
}

// --- concat ------------------------------------------------------------------

fn run_concat(size: i64) -> Result<Value, String> {
    let source = format!(
        "set joined to \"\"\n\
         for each i from 1 to {size}\n\
             set joined to joined + \"ab\"\n\
         end\n\
         joined\n"
    );
    run_source(&source)
}

fn expected_concat(size: i64) -> Expected {
    Expected::Text("ab".repeat(size.max(0) as usize))
}

// --- lex-heavy compile -------------------------------------------------------

fn run_lex(size: i64) -> Result<Value, String> {
    let mut source = String::new();
    for index in 0..size {
        source.push_str(&format!("set value{index} to {index}\n"));
    }
    let tokens = Lexer::tokenize(&source).map_err(|error| error.to_string())?;
    let program = parser::parse(tokens).map_err(|error| error.to_string())?;
    analyzer::analyze(&program).map_err(|error| error.to_string())?;
    Ok(Value::Number(program.statements.len() as f64))
}

fn expected_lex(size: i64) -> Expected {
    Expected::Number(size.max(0) as f64)
}

fn run_source(source: &str) -> Result<Value, String> {
    redblue::run_source_value(source).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_a_caller_supplied_answer_is_refused_within_the_budget() {
        let error = run_workload_with(
            "fibonacci",
            20,
            Duration::from_secs(5),
            Expected::Number(-1.0),
        )
        .expect_err("a wrong caller-supplied answer must be refused");
        assert!(matches!(error, BenchError::WrongAnswer { .. }), "{error:?}");

        let error = run_workload_with(
            "fibonacci",
            i64::MAX,
            Duration::from_millis(1),
            Expected::Number(1.0),
        )
        .expect_err("an oversized workload must be refused before it starts");
        assert!(
            matches!(error, BenchError::SizeTooLarge { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn edge_child_measurement_accepts_an_explicit_environment() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the clock is after the epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "rb-bench-child-env-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("the temporary directory is creatable");
        let sentinel = dir.join("sentinel");
        let started_marker = sentinel.with_extension("started");
        let finished_marker = sentinel.with_extension("finished");
        let sentinel_key = std::ffi::OsStr::new(HANG_SENTINEL_ENV);
        let env = [(sentinel_key, sentinel.as_os_str())];

        let row = run_workload_with_env(HANG_WORKLOAD_NAME, 0, Duration::from_secs(5), &env, None)
            .expect("an explicit environment should not change the measurement");
        assert_eq!(row.answer, Value::Number(1.0));
        assert!(
            started_marker.is_file(),
            "the child did not observe {HANG_SENTINEL_ENV}; it never wrote {}",
            started_marker.display()
        );
        assert!(
            finished_marker.is_file(),
            "the child did not observe {HANG_SENTINEL_ENV}; it never wrote {}",
            finished_marker.display()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn edge_worker_binary_resolves_a_cross_target_layout() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the clock is after the epoch")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("rb-bench-target-{}-{unique}", std::process::id()));
        let name = format!("rb-bench{}", std::env::consts::EXE_SUFFIX);
        let triple = "x86_64-unknown-linux-gnu";
        let profile = "debug";

        let cross = dir.join(triple).join(profile).join(&name);
        std::fs::create_dir_all(cross.parent().expect("the runner has a parent"))
            .expect("the cross-target directory is creatable");
        std::fs::write(&cross, b"stub").expect("the runner fixture is writable");
        let relative = Path::new(triple)
            .join(profile)
            .join("deps")
            .join("bench-test");
        assert_eq!(
            runner_binary_in_target(&dir, &relative, &name),
            Some(cross),
            "a cross-target build resolves target/<triple>/<profile>/<name>"
        );

        let plain = dir.join(profile).join(&name);
        std::fs::create_dir_all(plain.parent().expect("the runner has a parent"))
            .expect("the profile directory is creatable");
        std::fs::write(&plain, b"stub").expect("the runner fixture is writable");
        let relative = Path::new(profile).join("deps").join("bench-test");
        assert_eq!(
            runner_binary_in_target(&dir, &relative, &name),
            Some(plain),
            "a host build still resolves target/<profile>/<name>"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn edge_the_size_limit_is_enforced_without_running_the_workload() {
        let max = max_size("fibonacci").expect("a listed workload has a maximum");
        assert!(check_size_supported("fibonacci", max).is_ok());
        let error = check_size_supported("fibonacci", max + 1)
            .expect_err("one past the checkable maximum must be refused");
        assert!(
            matches!(error, BenchError::SizeTooLarge { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn edge_checked_in_baseline_is_resolved_from_the_crate_root() {
        let path = checked_in_baseline_path();
        assert!(
            path.ends_with("BENCH.md"),
            "the checked-in baseline should be BENCH.md, got {}",
            path.display()
        );
        assert!(
            path.is_file(),
            "the checked-in baseline should exist at {}",
            path.display()
        );
    }

    #[test]
    fn edge_bench_md_path_is_relative_to_the_working_directory_or_override() {
        let cwd = Path::new("/work");
        assert_eq!(
            bench_md_path_from(None, cwd),
            cwd.join("BENCH.md"),
            "without an override the baseline belongs in the current directory"
        );

        let override_path = std::ffi::OsStr::new("/tmp/custom-bench.md");
        assert_eq!(
            bench_md_path_from(Some(override_path), cwd),
            Path::new("/tmp/custom-bench.md"),
            "an explicit override wins over the working directory"
        );
    }

    #[test]
    fn edge_wall_time_cell_parser_rejects_malformed_text() {
        assert_eq!(
            parse_wall_time_ms("1.250 ms").expect("a valid wall-time cell parses"),
            1.25
        );
        for malformed in ["", "1.250", "slow ms", "-1 ms", "NaN ms", "inf ms"] {
            assert!(
                parse_wall_time_ms(malformed).is_err(),
                "{malformed:?} should be refused as a wall time"
            );
        }
    }

    #[test]
    fn edge_the_hidden_hang_workload_is_not_a_listed_workload() {
        assert!(
            !workload_names().contains(&HANG_WORKLOAD_NAME),
            "the timeout helper must not appear in the baseline or the usage text"
        );
        assert_eq!(HANG_SENTINEL_ENV, "RB_BENCH_HANG_SENTINEL");
    }

    #[test]
    fn edge_the_hidden_hang_size_is_capped() {
        assert_eq!(
            hang_duration(MAX_HANG_MILLIS).expect("the maximum is allowed"),
            Duration::from_millis(MAX_HANG_MILLIS as u64)
        );
        let error = hang_duration(MAX_HANG_MILLIS + 1)
            .expect_err("a hang past the maximum must be refused, not clamped");
        assert!(matches!(error, BenchError::Usage(_)), "{error:?}");
        assert!(
            error.to_string().contains("exceeds the maximum"),
            "the refusal should name the bound: {error}"
        );
        assert_eq!(
            hang_duration(0).expect("zero is allowed"),
            Duration::ZERO,
            "zero must stay the trivial sleep"
        );
    }

    #[test]
    fn edge_worker_output_parser_decodes_well_formed_lines() {
        let (number, elapsed) = parse_worker_output("fibonacci", 20, "OK 42 n 4008000000000000\n")
            .expect("a well-formed number line parses");
        assert_eq!(elapsed, Duration::from_nanos(42));
        assert_eq!(number, Value::Number(3.0));

        let (text, elapsed) = parse_worker_output("concat", 200, "OK 7 t 6869\n")
            .expect("a well-formed text line parses");
        assert_eq!(elapsed, Duration::from_nanos(7));
        assert_eq!(text, Value::Text("hi".to_string()));
    }

    #[test]
    fn edge_worker_output_parser_refuses_an_error_line() {
        let error = parse_worker_output("fibonacci", 20, "ERR the workload broke\n")
            .expect_err("an ERR line must be a load error, never an answer");
        match error {
            BenchError::Load(message) => assert!(
                message.contains("the workload broke"),
                "the load error should carry the worker message: {message}"
            ),
            other => panic!("expected a load error, got {other:?}"),
        }
    }

    #[test]
    fn edge_worker_output_parser_decodes_a_wrong_answer() {
        let line = encode_worker_error(&BenchError::WrongAnswer {
            workload: "fibonacci",
            size: 20,
            expected: "-1".to_string(),
            actual: "6765".to_string(),
        });
        assert!(line.starts_with("WRONG "), "unexpected worker line: {line}");

        let error = parse_worker_output("fibonacci", 20, &format!("{line}\n"))
            .expect_err("a WRONG line must be a wrong-answer error, never an answer");
        match error {
            BenchError::WrongAnswer {
                workload,
                size,
                expected,
                actual,
            } => {
                assert_eq!(workload, "fibonacci");
                assert_eq!(size, 20);
                assert_eq!(expected, "-1");
                assert_eq!(actual, "6765");
            }
            other => panic!("expected a wrong-answer error, got {other:?}"),
        }
    }

    #[test]
    fn edge_worker_output_parser_refuses_unreadable_fields() {
        for (line, expected) in [
            ("OK 12 n\n", "unreadable answer"),
            (
                "OK not-a-number n 4008000000000000\n",
                "unreadable duration",
            ),
            ("OK 12 n nothex\n", "unreadable number"),
            ("OK 12 x 4008000000000000\n", "unknown answer tag"),
        ] {
            let error = parse_worker_output("workload", 1, line)
                .expect_err("a malformed worker line must be a load error");
            match error {
                BenchError::Load(message) => assert!(
                    message.contains(expected),
                    "{line:?} should mention {expected:?}: {message}"
                ),
                other => panic!("expected a load error for {line:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn edge_worker_output_parser_refuses_bad_text_payloads() {
        for (line, expected) in [
            ("OK 12 t ff\n", "non-UTF-8 text"),
            ("OK 12 t abc\n", "unreadable text"),
            ("OK 12 t zz\n", "unreadable text"),
        ] {
            let error = parse_worker_output("workload", 1, line)
                .expect_err("a bad text payload must be a load error");
            match error {
                BenchError::Load(message) => assert!(
                    message.contains(expected),
                    "{line:?} should mention {expected:?}: {message}"
                ),
                other => panic!("expected a load error for {line:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn edge_hex_roundtrip_covers_empty_and_uppercase() {
        assert_eq!(
            decode_hex("").expect("empty hex is empty bytes"),
            Vec::<u8>::new()
        );
        assert_eq!(
            decode_hex("4A6b").expect("both hex cases decode"),
            b"Jk".to_vec()
        );

        let bytes = b"Hello \xE4\xB8\x96\xE7\x95\x8C";
        assert_eq!(
            decode_hex(&encode_hex(bytes)).expect("encoded bytes decode"),
            bytes
        );
        assert_eq!(encode_hex(b""), "");

        let error = decode_hex("4").expect_err("an odd-length payload must be refused");
        assert!(error.contains("odd-length"), "{error}");
    }

    #[test]
    fn edge_truncate_stays_within_small_widths_and_respects_wide_characters() {
        assert_eq!(truncate("", 0), "");
        assert_eq!(truncate("abc", 3), "abc");
        assert_eq!(truncate("abcd", 3), "...");
        assert_eq!(truncate("abcdef", 4), "a...");
        assert_eq!(truncate("你好世界", 4), "...");

        for (text, width) in [("abcdef", 5), ("你好世界", 5), ("你好世界", 8)] {
            let shortened = truncate(text, width);
            assert!(
                UnicodeWidthStr::width(shortened.as_str()) <= width.max(3),
                "{text:?} at width {width} became {shortened:?} ({} cells)",
                UnicodeWidthStr::width(shortened.as_str())
            );
        }
    }
}
