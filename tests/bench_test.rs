//! The benchmark harness's correctness face.
//!
//! `BENCH.md` is what makes a performance claim something a later phase can
//! check rather than a number in a paragraph. This test is deliberately about
//! coverage and answers, not about the timings: a timing assertion is a flake
//! wearing a gate's clothes, so the harness prints wall time and these tests
//! only require that a row exists and that the computed answer is right.
//!
//! The measuring runner is `cargo run --bin rb-bench`; this file shares its
//! workloads through `bench/mod.rs`. That is the one mechanism split across two
//! masters on purpose: the runner measures, the gate checks correctness.

#[path = "bench/mod.rs"]
mod bench;

use std::fs;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bench::{BenchError, DEFAULT_BUDGET};
use redblue::Value;

const WORKLOAD_NAMES: &[&str] = &["fibonacci", "primes", "concat", "lex"];

#[test]
fn bench_md_exists_and_names_all_workloads() {
    let path = bench::checked_in_baseline_path();
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} should exist: {error}", path.display()));

    for workload in WORKLOAD_NAMES {
        assert!(
            text.contains(workload),
            "BENCH.md should record the `{workload}` workload:\n{text}"
        );
    }
    assert!(
        text.contains("## Hardware") && text.contains("CPU:"),
        "BENCH.md should have a Hardware section naming the CPU it was measured on:\n{text}"
    );
    assert!(
        text.to_lowercase().contains("workload") && text.contains("Answer"),
        "BENCH.md should be a table with a workload column and verified answers:\n{text}"
    );
    assert!(
        text.contains("Wall time"),
        "the checked-in baseline must record a measured wall time so a later phase can detect a regression:\n{text}"
    );

    let mut seen: Vec<&str> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() != 4 || !WORKLOAD_NAMES.contains(&cells[0]) {
            continue;
        }
        let name = cells[0];
        let size: i64 = cells[1].parse().unwrap_or_else(|_| {
            panic!(
                "BENCH.md size for `{name}` is not a whole number: {:?}",
                cells[1]
            )
        });
        let recorded = cells[2];
        let wall_time = bench::parse_wall_time_ms(cells[3]).unwrap_or_else(|error| {
            panic!("BENCH.md wall time for `{name}` is malformed: {error}")
        });
        assert!(
            wall_time >= 0.0 && wall_time.is_finite(),
            "BENCH.md wall time for `{name}` should be a real measurement, got {wall_time}"
        );
        assert!(
            !seen.contains(&name),
            "BENCH.md has more than one row for `{name}`:\n{text}"
        );
        assert_eq!(
            size,
            bench::default_size(name).expect("the row names a known workload"),
            "BENCH.md should record `{name}` at its default size:\n{text}"
        );
        let measured = bench::run_workload(name, size, DEFAULT_BUDGET).unwrap_or_else(|error| {
            panic!("BENCH.md row for `{name}` at size {size} should run: {error}")
        });
        assert_eq!(
            measured.answer.to_string(),
            recorded,
            "BENCH.md answer for `{name}` at size {size} disagrees with the workload:\n{text}"
        );
        seen.push(name);
    }

    let mut expected_names = WORKLOAD_NAMES.to_vec();
    expected_names.sort_unstable();
    seen.sort_unstable();
    assert_eq!(
        seen, expected_names,
        "BENCH.md should have exactly one answered row per workload:\n{text}"
    );
}

#[test]
fn bench_workloads_compute_the_documented_literals() {
    let fibonacci = bench::run_workload("fibonacci", 20, DEFAULT_BUDGET).expect("fib runs");
    assert_eq!(fibonacci.answer, Value::Number(6765.0), "fib(20)");

    let primes = bench::run_workload("primes", 1000, DEFAULT_BUDGET).expect("primes runs");
    assert_eq!(primes.answer, Value::Number(168.0), "primes below 1000");

    let concat = bench::run_workload("concat", 7, DEFAULT_BUDGET).expect("concat runs");
    assert_eq!(concat.answer, Value::Text("ababababababab".to_string()));

    let lex = bench::run_workload("lex", 100, DEFAULT_BUDGET).expect("lex runs");
    assert_eq!(
        lex.answer,
        Value::Number(100.0),
        "one statement per assignment"
    );
}

#[test]
fn bench_table_lists_four_workloads_and_wall_time() {
    let rows = bench::run_all(DEFAULT_BUDGET).expect("every workload runs at its default size");
    let table = bench::render_table(&rows);

    assert!(
        table.contains("workload") && table.contains("wall time"),
        "the harness should print a named table:\n{table}"
    );
    for name in WORKLOAD_NAMES {
        assert!(table.contains(name), "{name} should have a row:\n{table}");
        let row = table
            .lines()
            .find(|line| line.starts_with(name))
            .unwrap_or_else(|| panic!("{name} row should exist:\n{table}"));
        assert!(
            row.contains("ms"),
            "{name} should carry a wall time, not only an answer:\n{table}"
        );
    }
    assert_eq!(
        table.lines().count(),
        WORKLOAD_NAMES.len() + 1,
        "one header and one row per workload:\n{table}"
    );
}

#[test]
fn edge_zero_size_completes_with_trivial_answers() {
    let cases: &[(&str, Value)] = &[
        ("fibonacci", Value::Number(0.0)),
        ("primes", Value::Number(0.0)),
        ("concat", Value::Text(String::new())),
        ("lex", Value::Number(0.0)),
    ];
    for (name, expected) in cases {
        let row = bench::run_workload(name, 0, DEFAULT_BUDGET)
            .unwrap_or_else(|error| panic!("{name} at size 0 should run: {error}"));
        assert_eq!(&row.answer, expected, "{name} at size zero");
    }
}

#[test]
fn edge_singleton_sizes_are_exact() {
    assert_eq!(
        bench::run_workload("fibonacci", 1, DEFAULT_BUDGET)
            .expect("fib(1)")
            .answer,
        Value::Number(1.0)
    );
    assert_eq!(
        bench::run_workload("primes", 1, DEFAULT_BUDGET)
            .expect("primes below 1")
            .answer,
        Value::Number(0.0)
    );
    assert_eq!(
        bench::run_workload("primes", 2, DEFAULT_BUDGET)
            .expect("primes below 2")
            .answer,
        Value::Number(1.0)
    );
    assert_eq!(
        bench::run_workload("concat", 1, DEFAULT_BUDGET)
            .expect("concat once")
            .answer,
        Value::Text("ab".to_string())
    );
    assert_eq!(
        bench::run_workload("lex", 1, DEFAULT_BUDGET)
            .expect("one assignment")
            .answer,
        Value::Number(1.0)
    );
}

#[test]
fn edge_empty_workload_name_lists_the_four() {
    let refused = bench::run_workload("", 10, DEFAULT_BUDGET);
    assert!(
        refused.is_err(),
        "an empty workload name must produce an error"
    );
    let error = refused.expect_err("empty name must fail");
    let message = error.to_string();
    assert!(
        matches!(error, BenchError::Usage(_)),
        "an empty name is a usage error, got {error:?}"
    );
    for name in WORKLOAD_NAMES {
        assert!(
            message.contains(name),
            "the usage error should list `{name}`: {message}"
        );
    }
}

#[test]
fn edge_negative_and_malformed_sizes_are_refused() {
    let negative = bench::parse_size("-1").expect_err("a negative size must fail");
    assert!(matches!(negative, BenchError::Usage(_)), "{negative:?}");
    assert!(negative.to_string().contains("zero or greater"));

    let text = bench::parse_size("many").expect_err("text is not a size");
    assert!(matches!(text, BenchError::Usage(_)), "{text:?}");
    assert!(text.to_string().contains("many"));

    let workload = bench::run_workload("fibonacci", -1, DEFAULT_BUDGET)
        .expect_err("a negative workload size must fail");
    assert!(matches!(workload, BenchError::Usage(_)), "{workload:?}");
}

#[test]
fn edge_a_hung_workload_fails_loudly_at_its_budget() {
    assert_eq!(
        DEFAULT_BUDGET,
        Duration::from_secs(60),
        "the harness promises a 60-second budget"
    );

    let budget = Duration::from_millis(20);
    let error = bench::run_with_budget("hung", budget, || {
        thread::sleep(Duration::from_millis(250));
        Ok(Value::Number(1.0))
    })
    .expect_err("a workload past its budget must fail");

    match error {
        BenchError::BudgetExceeded { workload, budget } => {
            assert_eq!(workload, "hung");
            assert_eq!(budget, Duration::from_millis(20));
        }
        other => panic!("expected a budget error, got {other:?}"),
    }
}

#[test]
fn edge_exhausted_shared_budget_is_reported_as_exhausted() {
    let error = bench::run_all(Duration::ZERO)
        .expect_err("an exhausted shared budget must fail before the first workload starts");

    match &error {
        BenchError::BudgetExhausted { workload, budget } => {
            assert_eq!(workload, WORKLOAD_NAMES[0]);
            assert_eq!(*budget, Duration::ZERO);
        }
        other => panic!("expected an exhausted-budget error, got {other:?}"),
    }

    let message = error.to_string();
    assert!(
        message.contains("exhausted"),
        "the error should say the shared budget ran out: {message}"
    );
    assert!(
        !message.contains("did not finish within"),
        "a workload that never started must not claim its own timeout: {message}"
    );
}

#[test]
fn edge_bench_md_path_is_the_working_directory() {
    if std::env::var_os("RB_BENCH_OUTPUT").is_none() {
        let cwd = std::env::current_dir().expect("the test process has a working directory");
        assert_eq!(
            bench::bench_md_path(),
            cwd.join("BENCH.md"),
            "the installed binary must write relative to where it was invoked"
        );
    }
}

#[test]
fn edge_reference_computation_is_inside_the_budget() {
    let budget = Duration::from_millis(20);
    let started = Instant::now();
    let error = bench::run_workload("fibonacci", i64::MAX, budget)
        .expect_err("an enormous reference answer must fail, not hang the caller");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the caller waited {:?}; the reference computation escaped the budget",
        started.elapsed()
    );
    match &error {
        BenchError::SizeTooLarge {
            workload,
            size,
            max,
        } => {
            assert_eq!(*workload, "fibonacci");
            assert_eq!(*size, i64::MAX);
            assert_eq!(*max, bench::max_size("fibonacci").expect("known workload"));
        }
        other => {
            panic!("expected a size-too-large error for an oversized reference, got {other:?}")
        }
    }
    let message = error.to_string();
    assert!(
        message.contains("exceeds the checkable maximum"),
        "the refusal should say the size is too large: {message}"
    );
    assert!(
        !message.contains("did not finish within"),
        "an oversized size never started, so it must not claim a timeout: {message}"
    );
}

#[test]
fn edge_oversized_references_are_refused_before_allocation() {
    let budget = Duration::from_millis(20);
    let cases: &[(&str, i64)] = &[
        ("fibonacci", i64::MAX),
        ("primes", 100_000_000_000),
        ("concat", 500_000_000_000),
        ("lex", i64::MAX),
    ];
    for (name, size) in cases {
        let started = Instant::now();
        let error = bench::run_workload(name, *size, budget)
            .expect_err("an allocation-sized reference must be refused, not attempted");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{name} at {size} took {:?} before refusing",
            started.elapsed()
        );
        assert!(
            matches!(error, BenchError::SizeTooLarge { .. }),
            "{name} at {size} should report an uncheckable size as too large, got {error:?}"
        );
    }
}

#[test]
fn edge_size_limit_boundary_is_exact_per_workload() {
    for name in WORKLOAD_NAMES {
        let max = bench::max_size(name).expect("a listed workload has a maximum");
        assert!(
            max >= bench::default_size(name).expect("a listed workload has a default"),
            "`{name}` must be able to check its default size"
        );
        let row = bench::run_workload(name, max, DEFAULT_BUDGET).unwrap_or_else(|error| {
            panic!("`{name}` at its maximum {max} should be measurable: {error}")
        });
        assert_eq!(row.workload, *name);
        assert_eq!(row.size, max);

        let error = bench::check_size_supported(name, max + 1)
            .expect_err("one past the checkable maximum must be refused");
        match &error {
            BenchError::SizeTooLarge {
                workload,
                size,
                max: reported,
            } => {
                assert_eq!(*workload, *name);
                assert_eq!(*size, max + 1);
                assert_eq!(*reported, max);
            }
            other => panic!("expected a size-too-large error for `{name}`, got {other:?}"),
        }
    }
}

#[test]
fn edge_fibonacci_is_capped_where_f64_is_exact() {
    let max = bench::max_size("fibonacci").expect("known workload");
    assert_eq!(max, 78, "fib(78) is the last value below 2^53");
    let row = bench::run_workload("fibonacci", max, DEFAULT_BUDGET)
        .expect("the capped maximum should still run and check");
    assert_eq!(row.answer, Value::Number(8_944_394_323_791_464.0));

    let error = bench::run_workload("fibonacci", max + 1, DEFAULT_BUDGET)
        .expect_err("one past the exact f64 range must be refused");
    assert!(
        matches!(error, BenchError::SizeTooLarge { .. }),
        "{error:?}"
    );
}

#[test]
fn edge_a_wrong_answer_is_refused_instead_of_measured() {
    let error = bench::run_workload_with(
        "fibonacci",
        20,
        DEFAULT_BUDGET,
        bench::Expected::Number(-1.0),
    )
    .expect_err("a wrong answer must be an error, never a fast success");

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
fn edge_a_workload_error_is_reported_as_load() {
    let error = bench::run_with_budget("broken", DEFAULT_BUDGET, || {
        Err::<Value, String>("the workload broke".to_string())
    })
    .expect_err("a workload error must be an error");

    match error {
        BenchError::Load(message) => {
            assert!(
                message.contains("the workload broke"),
                "the load error should carry the workload message: {message}"
            );
        }
        other => panic!("expected a load error, got {other:?}"),
    }
}

#[test]
fn edge_a_workload_that_stops_without_an_answer_is_a_load_error() {
    let error = bench::run_with_budget("vanished", DEFAULT_BUDGET, || -> Result<Value, String> {
        panic!("workload stopped before answering")
    })
    .expect_err("a disconnected worker must be an error");

    match error {
        BenchError::Load(message) => {
            assert!(
                message.contains("vanished"),
                "the load error should name the lost workload: {message}"
            );
        }
        other => panic!("expected a load error, got {other:?}"),
    }
}

#[test]
fn bench_markdown_shape_names_answers_and_hardware() {
    let rows = bench::run_all(DEFAULT_BUDGET).expect("every workload runs");
    let markdown = bench::render_markdown(&rows, &bench::hardware_note());

    assert!(
        markdown.contains("## Hardware") && markdown.contains("CPU:"),
        "the generated baseline should have a Hardware section naming the CPU:\n{markdown}"
    );
    assert!(markdown.contains("Answer"), "{markdown}");
    assert!(
        markdown.contains("Wall time") && markdown.contains("ms"),
        "the checked-in baseline should record the measured wall time:\n{markdown}"
    );
    for name in WORKLOAD_NAMES {
        assert!(markdown.contains(name), "missing {name}:\n{markdown}");
        let row = markdown
            .lines()
            .find(|line| line.starts_with(&format!("| {name} |")))
            .unwrap_or_else(|| panic!("missing {name} row:\n{markdown}"));
        let cells: Vec<&str> = row.trim_matches('|').split('|').map(str::trim).collect();
        assert_eq!(
            cells.len(),
            4,
            "the {name} row should carry a wall time:\n{row}"
        );
        assert!(
            bench::parse_wall_time_ms(cells[3]).is_ok(),
            "the {name} wall time should be a measurement, got {:?}",
            cells[3]
        );
    }

    let timings = bench::render_timings(&rows, &bench::hardware_note());
    assert!(timings.contains("Wall time"), "{timings}");
    assert!(timings.contains("ms"), "{timings}");
    for name in WORKLOAD_NAMES {
        assert!(timings.contains(name), "missing {name}:\n{timings}");
    }
}

#[test]
fn edge_cli_empty_workload_name_is_a_usage_error() {
    assert_eq!(
        bench::run_cli(&[String::new()]),
        2,
        "an empty workload name is a usage error"
    );
    assert_eq!(bench::run_cli(&["--help".to_string()]), 0);
}

#[test]
fn edge_cli_unknown_workload_with_size_is_a_usage_error() {
    assert_eq!(
        bench::run_cli(&["bogus".to_string(), "5".to_string()]),
        2,
        "an unknown workload with an explicit size is a usage error"
    );
    assert_eq!(
        bench::run_cli(&["bogus".to_string()]),
        2,
        "an unknown workload without a size is a usage error"
    );
}

#[test]
fn edge_cli_too_many_arguments_is_a_usage_error() {
    assert_eq!(
        bench::run_cli(&[
            "fibonacci".to_string(),
            "20".to_string(),
            "extra".to_string(),
        ]),
        2,
        "a third argument must be refused rather than silently ignored"
    );
}

#[test]
fn edge_unknown_workload_error_names_every_workload() {
    let error = bench::run_workload("bogus", 1, DEFAULT_BUDGET)
        .expect_err("an unknown workload must be refused");
    assert!(matches!(error, BenchError::Usage(_)), "{error:?}");
    let message = error.to_string();
    for name in WORKLOAD_NAMES {
        assert!(
            message.contains(name),
            "the unknown-workload error should list `{name}`: {message}"
        );
    }
}

#[test]
fn edge_size_parsing_boundaries() {
    assert_eq!(bench::parse_size("0").expect("zero is legal"), 0);
    assert_eq!(
        bench::parse_size(&i64::MAX.to_string()).expect("i64::MAX parses"),
        i64::MAX
    );
    let overflow =
        bench::parse_size("9223372036854775808").expect_err("one past i64::MAX must fail");
    assert!(matches!(overflow, BenchError::Usage(_)), "{overflow:?}");
}

fn unique_temp_dir(tag: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after the epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("rb-bench-{tag}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).expect("the temporary directory is creatable");
    dir
}

#[test]
fn edge_cli_no_arguments_runs_all_and_writes_a_parseable_baseline() {
    let dir = unique_temp_dir("baseline");
    let path = dir.join("BENCH.md");

    assert_eq!(
        bench::run_cli_with_path(&[], &path),
        0,
        "the documented no-argument runner should succeed"
    );

    let text = fs::read_to_string(&path).expect("the no-argument runner writes its baseline");
    assert!(
        text.contains("Wall time"),
        "the written baseline should record the measured wall times:\n{text}"
    );
    let mut seen: Vec<&str> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() != 4 || !WORKLOAD_NAMES.contains(&cells[0]) {
            continue;
        }
        let size: i64 = cells[1]
            .parse()
            .unwrap_or_else(|_| panic!("a whole size, got {:?}", cells[1]));
        assert!(
            !cells[2].is_empty(),
            "the written baseline should carry the verified answer for `{}`",
            cells[0]
        );
        assert!(
            bench::parse_wall_time_ms(cells[3]).is_ok(),
            "the written baseline should carry a wall time for `{}`, got {:?}",
            cells[0],
            cells[3]
        );
        assert_eq!(
            size,
            bench::default_size(cells[0]).expect("the row names a known workload"),
            "the written baseline should record `{}` at its default size",
            cells[0]
        );
        seen.push(cells[0]);
    }
    let mut expected = WORKLOAD_NAMES.to_vec();
    expected.sort_unstable();
    seen.sort_unstable();
    assert_eq!(
        seen, expected,
        "the written baseline should have one row per workload:\n{text}"
    );

    let timings = bench::timings_path(&path);
    let timing_text =
        fs::read_to_string(&timings).expect("the no-argument runner writes its timing artifact");
    assert!(
        timing_text.contains("Wall time") && timing_text.contains("ms"),
        "the timing artifact should carry machine-specific wall times:\n{timing_text}"
    );
    for name in WORKLOAD_NAMES {
        assert!(
            timing_text.contains(name),
            "the timing artifact should carry `{name}`:\n{timing_text}"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn edge_cli_no_arguments_reports_a_write_failure() {
    let dir = unique_temp_dir("write-failure");

    assert_eq!(
        bench::run_cli_with_path(&[], &dir),
        1,
        "a baseline path that cannot be written should fail loudly"
    );
    assert!(
        dir.join("BENCH_TIMINGS.md").is_file(),
        "the timing artifact should stay inside the sandbox, not escape to the parent directory"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn edge_cli_compare_reruns_every_baseline_row() {
    let dir = unique_temp_dir("compare");
    let baseline_path = dir.join("BASELINE.md");
    let baseline = "\
# Redblue benchmarks\n\n\
| Workload | Size | Answer | Wall time |\n\
|---|---:|---:|---:|\n\
| fibonacci | 10 | 55 | 1.000 ms |\n\
| primes | 50 | 15 | 2.000 ms |\n";
    fs::write(&baseline_path, baseline).expect("the temporary baseline is writable");
    let output = dir.join("BENCH.md");

    let table = bench::comparison_table(&baseline_path)
        .expect("a well-formed baseline should be re-run and rendered");
    assert!(table.contains("fibonacci"), "{table}");
    assert!(table.contains("primes"), "{table}");
    assert!(table.contains("10"), "{table}");
    assert!(table.contains("50"), "{table}");
    assert!(table.contains("ms"), "{table}");
    assert!(
        !table.contains("not run"),
        "every baseline row should be re-run:\n{table}"
    );

    assert_eq!(
        bench::run_cli_with_path(
            &["--compare".to_string(), baseline_path.display().to_string()],
            &output,
        ),
        0,
        "comparing a well-formed baseline should succeed"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn edge_cli_compare_refuses_a_missing_baseline() {
    let dir = unique_temp_dir("compare-missing");
    let missing_path = dir.join("MISSING.md");
    let output = dir.join("BENCH.md");

    let error = bench::comparison_table(&missing_path)
        .expect_err("a missing baseline must be refused, not treated as empty");
    assert!(
        error.contains("could not read") && error.contains("MISSING.md"),
        "the refusal should say the baseline could not be read: {error}"
    );

    assert_eq!(
        bench::run_cli_with_path(
            &["--compare".to_string(), missing_path.display().to_string()],
            &output,
        ),
        1,
        "the CLI should fail when the baseline file does not exist"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn edge_cli_compare_refuses_an_empty_baseline() {
    let dir = unique_temp_dir("compare-empty");
    let baseline_path = dir.join("EMPTY.md");
    fs::write(&baseline_path, "").expect("the temporary baseline is writable");
    let output = dir.join("BENCH.md");

    let error = bench::comparison_table(&baseline_path)
        .expect_err("an empty baseline must be refused, not compare zero workloads");
    assert!(
        error.contains("no benchmark rows") && error.contains("EMPTY.md"),
        "the refusal should say the baseline has no rows: {error}"
    );

    assert_eq!(
        bench::run_cli_with_path(
            &["--compare".to_string(), baseline_path.display().to_string()],
            &output,
        ),
        1,
        "the CLI should fail when the baseline has no rows"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn edge_cli_compare_requires_exactly_one_baseline_path() {
    let dir = unique_temp_dir("compare-arity");
    let output = dir.join("BENCH.md");

    assert_eq!(
        bench::run_cli_with_path(&["--compare".to_string()], &output),
        2,
        "`--compare` without a path should be a usage error"
    );
    assert_eq!(
        bench::run_cli_with_path(
            &[
                "--compare".to_string(),
                "first.md".to_string(),
                "second.md".to_string(),
            ],
            &output,
        ),
        2,
        "`--compare` with two paths should be a usage error"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn edge_cli_compare_refuses_a_corrupted_baseline_answer() {
    let dir = unique_temp_dir("compare-answer");
    let baseline_path = dir.join("BASELINE.md");
    let baseline = "\
# Redblue benchmarks\n\n\
| Workload | Size | Answer | Wall time |\n\
|---|---:|---:|---:|\n\
| fibonacci | 10 | 54 | 1.000 ms |\n";
    fs::write(&baseline_path, baseline).expect("the temporary baseline is writable");
    let output = dir.join("BENCH.md");

    let error = bench::comparison_table(&baseline_path)
        .expect_err("a baseline answer that disagrees with the workload must be refused");
    assert!(
        error.contains("fibonacci") && error.contains("54") && error.contains("55"),
        "the refusal should name the workload and both answers: {error}"
    );

    assert_eq!(
        bench::run_cli_with_path(
            &["--compare".to_string(), baseline_path.display().to_string()],
            &output,
        ),
        1,
        "the CLI should fail when a recorded answer is wrong"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn edge_workload_names_are_unique_and_defaults_are_positive() {
    let names = bench::workload_names();
    let mut unique = names.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        names.len(),
        "a duplicated workload name would make rows ambiguous: {names:?}"
    );
    for name in &names {
        assert!(!name.is_empty(), "a workload name must not be empty");
        assert!(
            bench::default_size(name).expect("a listed workload has a default") > 0,
            "`{name}` should have a positive default size"
        );
    }
}

#[test]
fn edge_recorded_wall_times_are_present_for_every_workload() {
    let path = bench::checked_in_baseline_path();
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} should exist: {error}", path.display()));

    let mut seen: Vec<&str> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() != 4 || !WORKLOAD_NAMES.contains(&cells[0]) {
            continue;
        }
        let milliseconds = bench::parse_wall_time_ms(cells[3]).unwrap_or_else(|error| {
            panic!(
                "the checked-in baseline must record a usable wall time for `{}`: {error}",
                cells[0]
            )
        });
        assert!(
            milliseconds.is_finite() && milliseconds >= 0.0,
            "`{}` should record a real wall time, got {milliseconds}",
            cells[0]
        );
        seen.push(cells[0]);
    }

    let mut expected = WORKLOAD_NAMES.to_vec();
    expected.sort_unstable();
    seen.sort_unstable();
    assert_eq!(
        seen, expected,
        "every workload needs a recorded wall time in BENCH.md:\n{text}"
    );
}

#[test]
fn edge_malformed_recorded_wall_time_is_rejected() {
    for malformed in ["", "1.0", "fast ms", "-1 ms", "NaN ms", "inf ms"] {
        let error = bench::parse_wall_time_ms(malformed)
            .expect_err("a malformed wall-time cell must be refused, not ignored");
        assert!(
            !error.is_empty(),
            "the refusal for {malformed:?} should carry a message"
        );
    }

    assert_eq!(
        bench::parse_wall_time_ms("0 ms").expect("zero is a legal timing"),
        0.0
    );
    assert!(
        bench::parse_wall_time_ms("1.500 ms").expect("a positive timing is legal") > 0.0,
        "a positive wall time should parse"
    );
}

#[test]
fn edge_baseline_parser_reads_every_recorded_row() {
    let rows = bench::parse_baseline_markdown(
        "# Redblue benchmarks\n\n\
         | Workload | Size | Answer | Wall time |\n\
         |---|---:|---:|---:|\n\
         | fibonacci | 20 | 6765 | 1.250 ms |\n\
         | primes | 1000 | 168 | 2.000 ms |\n",
    )
    .expect("a well-formed baseline parses");

    assert_eq!(rows.len(), 2, "both data rows should be read");
    assert_eq!(rows[0].workload, "fibonacci");
    assert_eq!(rows[0].size, 20);
    assert_eq!(rows[0].answer, "6765");
    assert_eq!(rows[0].wall_time_ms, 1.25);
}

#[test]
fn edge_baseline_parser_rejects_a_corrupted_known_row() {
    let size_error = bench::parse_baseline_markdown("| fibonacci | many | 6765 | 1.250 ms |\n")
        .expect_err("a corrupted size on a known workload must not be skipped");
    assert!(
        size_error.contains("fibonacci"),
        "the refusal should name the workload: {size_error}"
    );

    let time_error = bench::parse_baseline_markdown("| primes | 1000 | 168 | slow ms |\n")
        .expect_err("a corrupted wall time on a known workload must not be skipped");
    assert!(
        time_error.contains("primes"),
        "the refusal should name the workload: {time_error}"
    );

    let negative_size = bench::parse_baseline_markdown("| fibonacci | -1 | 0 | 1.000 ms |\n")
        .expect_err("a negative size on a known workload must not be skipped");
    assert!(
        negative_size.contains("fibonacci") && negative_size.contains("-1"),
        "the refusal should name the workload and size: {negative_size}"
    );
}

#[test]
fn edge_baseline_parser_rejects_malformed_known_rows() {
    let short = bench::parse_baseline_markdown("| fibonacci | 20 | 6765 |\n")
        .expect_err("a known workload with three cells must not be skipped");
    assert!(
        short.contains("fibonacci") && short.contains('3'),
        "the refusal should name the workload and the cell count: {short}"
    );

    let long = bench::parse_baseline_markdown("| primes | 1000 | 168 | 1.000 ms | extra |\n")
        .expect_err("a known workload with five cells must not be skipped");
    assert!(
        long.contains("primes") && long.contains('5'),
        "the refusal should name the workload and the cell count: {long}"
    );
}

#[test]
fn edge_baseline_parser_rejects_unknown_duplicate_and_oversized_rows() {
    let unknown = bench::parse_baseline_markdown("| fibonacii | 20 | 6765 | 1.250 ms |\n")
        .expect_err("an unknown workload with a numeric size must not be skipped");
    assert!(
        unknown.contains("fibonacii"),
        "the refusal should name the unknown workload: {unknown}"
    );

    let duplicate = bench::parse_baseline_markdown(
        "| fibonacci | 20 | 6765 | 1.250 ms |\n\
         | fibonacci | 20 | 6765 | 1.250 ms |\n",
    )
    .expect_err("a duplicate row must be refused");
    assert!(
        duplicate.contains("fibonacci"),
        "the refusal should name the duplicated workload: {duplicate}"
    );

    let oversized = bench::parse_baseline_markdown("| fibonacci | 79 | 0 | 1.000 ms |\n")
        .expect_err("a row beyond the checkable maximum must be refused");
    assert!(
        oversized.contains("fibonacci"),
        "the refusal should name the oversized workload: {oversized}"
    );
}

#[test]
fn edge_comparison_is_advisory_and_handles_a_zero_baseline() {
    let baseline = vec![
        bench::BaselineRow {
            workload: "fibonacci".to_string(),
            size: 20,
            answer: "6765".to_string(),
            wall_time_ms: 1.0,
        },
        bench::BaselineRow {
            workload: "primes".to_string(),
            size: 1000,
            answer: "168".to_string(),
            wall_time_ms: 0.0,
        },
    ];
    let current = vec![bench::Measurement {
        workload: "fibonacci",
        size: 20,
        answer: Value::Number(6765.0),
        elapsed: Duration::from_millis(2),
    }];

    let table = bench::render_comparison(&baseline, &current);
    assert!(table.contains("fibonacci"), "{table}");
    assert!(
        table.contains("2.00x"),
        "the ratio is advisory, not a gate: {table}"
    );
    assert!(table.contains("primes"), "{table}");
    assert!(
        table.contains("n/a"),
        "a zero baseline must not become an infinite ratio: {table}"
    );
    assert!(
        table.contains("not run"),
        "a workload with no measurement should say so: {table}"
    );
}

#[test]
fn edge_render_markdown_records_each_measurement_wall_time() {
    let rows = vec![
        bench::Measurement {
            workload: "fibonacci",
            size: 20,
            answer: Value::Number(6765.0),
            elapsed: Duration::from_millis(1),
        },
        bench::Measurement {
            workload: "primes",
            size: 1000,
            answer: Value::Number(168.0),
            elapsed: Duration::from_millis(2),
        },
    ];
    let markdown = bench::render_markdown(&rows, "CPU: test machine");

    assert!(
        markdown.contains("CPU: test machine"),
        "the baseline should carry its hardware note:\n{markdown}"
    );
    for row in &rows {
        let prefix = format!("| {} |", row.workload);
        let line = markdown
            .lines()
            .find(|line| line.starts_with(&prefix))
            .unwrap_or_else(|| panic!("missing {} row:\n{markdown}", row.workload));
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        assert_eq!(cells.len(), 4, "the row should carry a wall time:\n{line}");
        assert_eq!(
            cells[3],
            format!("{:.3} ms", row.elapsed.as_secs_f64() * 1000.0),
            "the recorded wall time should be the measured elapsed time:\n{line}"
        );
    }
}
