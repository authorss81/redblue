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
/// running after the error. The budget bounds the wait, not the work. A single
/// `run_workload` spends this on one workload; `run_all` spends it once across
/// all four, so the no-argument runner is bounded by one budget rather than
/// four.
pub const DEFAULT_BUDGET: Duration = Duration::from_secs(60);

/// The largest size each workload is allowed to run.
///
/// A larger size is refused before the reference answer or the workload's source
/// string is built, so the harness fails cleanly instead of allocating without
/// bound or burning the shared budget. Every maximum here is a size the harness
/// has actually measured inside [`DEFAULT_BUDGET`]; the boundary test runs each
/// workload at its maximum, so the advertised limit is demonstrated rather than
/// merely compared.
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
    ensure_workload_size_supported(workload, size)?;
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

fn bench_md_path_from(override_path: Option<&std::ffi::OsStr>, cwd: &Path) -> std::path::PathBuf {
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

/// Parses the data rows of a `BENCH.md` table.
///
/// Header, separator, and prose lines are ignored. A row that names a known
/// workload but carries the wrong number of cells, an unreadable size, or an
/// unreadable wall time is an error rather than a silently skipped row, so a
/// corrupted baseline cannot compare as if it had fewer workloads. A four-cell
/// row with a numeric size but an unknown workload name is also an error, and a
/// repeated `(workload, size)` row is refused.
pub fn parse_baseline_markdown(text: &str) -> Result<Vec<BaselineRow>, String> {
    let mut rows: Vec<BaselineRow> = Vec::new();
    let known = workload_names();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
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
            if cells[1].parse::<i64>().is_ok() {
                return Err(format!(
                    "baseline row names unknown workload `{}`; choose one of: {}",
                    cells[0],
                    known.join(", ")
                ));
            }
            continue;
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
        if rows
            .iter()
            .any(|row| row.workload == cells[0] && row.size == size)
        {
            return Err(format!(
                "baseline has more than one row for `{}` at size {size}",
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

/// `run_cli` with an injectable output path, so the no-argument branch and the
/// write-failure branch can be exercised without touching the checked-in
/// `BENCH.md`.
pub fn run_cli_with_path(arguments: &[String], output: &Path) -> i32 {
    if arguments
        .first()
        .is_some_and(|argument| argument == "--compare")
    {
        return match arguments {
            [_, baseline] => compare_with_baseline(Path::new(baseline)),
            _ => {
                eprintln!("--compare takes exactly one baseline path");
                eprintln!("{}", usage());
                2
            }
        };
    }

    if arguments.is_empty() {
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
        .expect_err("an oversized workload must be refused before it starts");
        assert!(
            matches!(error, BenchError::SizeTooLarge { .. }),
            "{error:?}"
        );
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
}
