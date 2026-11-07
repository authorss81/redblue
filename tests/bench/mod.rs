//! The benchmark workloads, shared by the `rb-bench` runner and the tests.
//!
//! A measurement harness is only useful if it knows the answer is right. Every
//! workload here returns a value that has a deterministic reference answer, and
//! [`run_workload`] refuses to report a timing for an answer that differs. Wall
//! time is recorded; it is never asserted. The runner in `tests/bench/runner.rs`
//! prints and writes the table, and `tests/bench_test.rs` pins the answers
//! inside the normal test gate.

use std::fmt;
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use redblue::analyzer;
use redblue::lexer::Lexer;
use redblue::parser;
use redblue::Value;

/// The budget every harness workload runs under.
///
/// A benchmark that hangs must fail instead of waiting forever. The runner
/// enforces this itself with a worker thread and `recv_timeout`; the interpreter
/// has no cancellation hook, so a timed-out worker is detached and may keep
/// running after the error. The budget bounds the wait, not the work.
pub const DEFAULT_BUDGET: Duration = Duration::from_secs(60);

/// The largest size whose reference answer can be materialized without risking
/// a multi-gigabyte allocation. A larger size is refused before the reference
/// answer or the workload's source string is built: a harness that can only
/// check small answers must fail cleanly on an uncheckable size rather than
/// hanging or aborting the process.
const MAX_FIBONACCI_SIZE: i64 = 10_000_000;
const MAX_PRIMES_SIZE: i64 = 10_000_000;
const MAX_CONCAT_SIZE: i64 = 10_000_000;
const MAX_LEX_SIZE: i64 = 1_000_000;

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
            Expected::Text(text) => format!("{text:?}"),
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
                "workload '{workload}' did not finish within its {}s budget",
                budget.as_secs()
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

pub fn workload_names() -> Vec<&'static str> {
    WORKLOADS.iter().map(|workload| workload.name).collect()
}

fn workload_named(name: &str) -> Result<&'static Workload, BenchError> {
    WORKLOADS
        .iter()
        .find(|workload| workload.name == name)
        .ok_or_else(|| BenchError::Usage(unknown_workload(name)))
}

fn unknown_workload(name: &str) -> String {
    format!(
        "unknown workload '{name}'; choose one of: {}",
        workload_names().join(", ")
    )
}

pub fn usage() -> String {
    format!(
        "Usage: rb-bench [workload [size]]\n\
         Workloads: {}\n\
         With no arguments, runs all four at their default sizes and writes BENCH.md.",
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

/// Runs `f` on a worker thread and gives up after `budget`.
///
/// The worker cannot be cancelled: the interpreter has no cancellation hook, so
/// a timed-out thread is detached and may keep consuming CPU until the process
/// exits. What this bounds is the caller's wait — a hung workload returns a loud
/// error at the deadline instead of blocking the gate forever. A normal
/// benchmark is far below the budget, so the thread is only a safety net.
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

/// The reference answer is computed on the same worker as the workload, so the
/// budget covers both. Computing it on the caller's thread first made an
/// enormous size hang or abort before any deadline could fire.
fn measure_with_reference(
    workload: &'static Workload,
    size: i64,
    budget: Duration,
) -> Result<Measurement, BenchError> {
    ensure_size_supported(workload, size, budget)?;
    let ((answer, expected, elapsed), _) = run_with_budget(workload.name, budget, move || {
        let expected = (workload.expected)(size);
        let started = Instant::now();
        let answer = (workload.run)(size)?;
        Ok((answer, expected, started.elapsed()))
    })?;
    check_expected(workload, size, expected, answer, elapsed)
}

/// Refuses a size whose reference answer cannot be materialized safely.
///
/// The alternative is an allocation abort or an unbounded loop on the caller's
/// thread. Reporting it as over budget keeps the harness's contract: it fails
/// loudly instead of waiting forever, and it never reports a timing it could
/// not check.
fn ensure_size_supported(
    workload: &'static Workload,
    size: i64,
    budget: Duration,
) -> Result<(), BenchError> {
    if size > workload.max_size {
        return Err(BenchError::BudgetExceeded {
            workload: workload.name.to_string(),
            budget,
        });
    }
    Ok(())
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
    ensure_size_supported(workload, size, budget)?;
    measure(workload, size, budget, expected)
}

pub fn default_size(name: &str) -> Result<i64, BenchError> {
    Ok(workload_named(name)?.default_size)
}

/// One table for all workloads, with wall time in milliseconds.
pub fn render_table(rows: &[Measurement]) -> String {
    let mut table = String::new();
    table.push_str(&format!(
        "{:<12} {:>8} {:>20} {:>14}\n",
        "workload", "size", "answer", "wall time"
    ));
    for row in rows {
        table.push_str(&format!(
            "{:<12} {:>8} {:>20} {:>11.3} ms\n",
            row.workload,
            row.size,
            truncate(&row.answer.to_string(), 20),
            row.elapsed.as_secs_f64() * 1000.0
        ));
    }
    table
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let mut short: String = text.chars().take(width.saturating_sub(3)).collect();
    short.push_str("...");
    short
}

/// The checked-in record. Timings are informational; the answer column is what
/// the measuring machine actually verified while producing this table.
pub fn render_markdown(rows: &[Measurement], hardware: &str) -> String {
    let mut markdown = String::new();
    markdown.push_str("# Redblue benchmarks\n\n");
    markdown.push_str(
        "Generated by `cargo run --bin rb-bench`. Wall time is printed and \
         recorded, but never asserted: timing assertions are flakes wearing a \
         gate's clothes. The answer column is verified by the harness before a \
         row is written.\n\n",
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

/// Best-effort hardware note for the machine that produced `BENCH.md`.
pub fn hardware_note() -> String {
    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("model name"))
                .and_then(|line| line.split(':').nth(1))
                .map(str::trim)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "unknown CPU".to_string());
    format!(
        "CPU: {cpu} ({} CPUs)",
        std::thread::available_parallelism().map_or(1, |n| n.get())
    )
}

/// The path `BENCH.md` is written to and read from: the project root.
pub fn bench_md_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("BENCH.md")
}

/// Runs every workload at its default size, in declaration order.
pub fn run_all(budget: Duration) -> Result<Vec<Measurement>, BenchError> {
    WORKLOADS
        .iter()
        .map(|workload| run_workload(workload.name, workload.default_size, budget))
        .collect()
}

/// The runner and its exit codes. Kept here so the binary is a one-line shell
/// and the argument handling is testable.
pub fn run_cli(arguments: &[String]) -> i32 {
    if arguments.is_empty() {
        let rows = match run_all(DEFAULT_BUDGET) {
            Ok(rows) => rows,
            Err(error) => {
                eprintln!("{error}");
                return 1;
            }
        };
        print!("{}", render_table(&rows));
        let markdown = render_markdown(&rows, &hardware_note());
        if let Err(error) = std::fs::write(bench_md_path(), markdown) {
            eprintln!("could not write {}: {error}", bench_md_path().display());
            return 1;
        }
        println!("wrote {}", bench_md_path().display());
        return 0;
    }

    let selected: (&str, i64) = match arguments {
        [name] if name == "--help" || name == "-h" => {
            println!("{}", usage());
            return 0;
        }
        [name] => match default_size(name) {
            Ok(size) => (name.as_str(), size),
            Err(error) => {
                eprintln!("{error}");
                eprintln!("{}", usage());
                return 2;
            }
        },
        [name, size] => {
            let size = match parse_size(size) {
                Ok(size) => size,
                Err(error) => {
                    eprintln!("{error}");
                    eprintln!("{}", usage());
                    return 2;
                }
            };
            match default_size(name) {
                Ok(_) => (name.as_str(), size),
                Err(error) => {
                    eprintln!("{error}");
                    eprintln!("{}", usage());
                    return 2;
                }
            }
        }
        _ => {
            eprintln!("{}", usage());
            return 2;
        }
    };

    match run_workload(selected.0, selected.1, DEFAULT_BUDGET) {
        Ok(row) => {
            print!("{}", render_table(&[row]));
            0
        }
        Err(error) => {
            eprintln!("{error}");
            1
        }
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
        .expect_err("an oversized workload must not escape the budget");
        assert!(
            matches!(error, BenchError::BudgetExceeded { .. }),
            "{error:?}"
        );
    }
}
