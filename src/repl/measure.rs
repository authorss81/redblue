//! What `.time` and `.bench` measure, and what they say about it.
//!
//! The arithmetic and the wording only, with nothing in here that can run a
//! program: the runs themselves belong to the session, because the point of a
//! measurement taken at the prompt is that it is taken in the session the user
//! is sitting in.
//!
//! Wall time is read from the host clock, deliberately and visibly. It is
//! nondeterministic by nature — a number a test cannot predict and must not try
//! to — and every report here labels its figures `wall time` so nobody reads
//! them as instruction counts.

use std::time::Duration;

/// What `.time` runs when it is not told otherwise, and the whole of `.bench`.
///
/// Two round numbers rather than powers of two: both are the kind of sample size
/// a person reads a result at, and 100 runs of a tree-walking interpreter cost
/// milliseconds.
pub const TIME_RUNS: usize = 10;
pub const BENCH_RUNS: usize = 100;

/// The most runs either command will make.
///
/// A measurement runs user code in a loop, so a count with a dozen digits in it
/// is a hang wearing a number's clothes. Refused by name rather than attempted:
/// the cap is a limit on the request, not on the machine.
pub const MAX_RUNS: usize = 10_000;

/// The word that marks the run count, so that one is never read out of an
/// expression by accident.
///
/// `.time 1 + 1` has to time `1 + 1`, which a bare trailing count would read as
/// the expression `1 +` run once. `n times` is the language's own shape for the
/// same thing — `repeat 3 times` — and no expression ends in it.
const COUNT_WORD: &str = "times";

/// A measurement asked for: what to run, and how many times.
#[derive(Debug, PartialEq, Eq)]
pub struct Measurement<'a> {
    /// The program to run, with any run count taken off.
    pub expression: &'a str,
    /// How many runs to make, already refused if it is not a sane number.
    pub runs: usize,
}

/// Splits `argument` into the program to run and the number of runs to make.
///
/// `command` names the command that asked, and every refusal says it: `bench`
/// refusing a count is `bench` reporting its own argument, not the language
/// complaining about a number somewhere else.
pub fn split_runs<'a>(
    argument: &'a str,
    command: &str,
    default: usize,
) -> Result<Measurement<'a>, String> {
    let argument = argument.trim();
    let Some((expression, written)) = count_written(argument) else {
        return Ok(Measurement {
            expression: argument,
            runs: default,
        });
    };

    if expression.is_empty() {
        return Err(format!(
            "Error: '{}' was given {} {COUNT_WORD} and nothing to measure.",
            command, written
        ));
    }

    Ok(Measurement {
        expression,
        runs: read_count(written, command)?,
    })
}

/// The program and the count as the user wrote them, or `None` when the argument
/// carries no count.
///
/// The count is the whole of the argument's tail in the shape
/// `<expression> <count> times`, so an expression of any length keeps its
/// trailing `times` (`countdown(3) times` is a call with an argument named
/// `times`, and stays one) and a count is never mistaken for part of an
/// expression (`1 + 1` is not `1 +` run once).
fn count_written(argument: &str) -> Option<(&str, &str)> {
    let (head, written) = argument.rsplit_once(char::is_whitespace)?;
    if written != COUNT_WORD {
        return None;
    }
    let (expression, count) = head.rsplit_once(char::is_whitespace).unwrap_or(("", head));
    // A bare word is what a count looks like; a call or a string is an expression
    // that happens to end in the word, and stays one. `many times` is a count the
    // user mistyped and is reported as such; `countdown(3) times` is a call.
    if !is_bare_word(count) {
        return None;
    }
    Some((expression.trim(), count))
}

/// Whether `word` is a bare word: letters, digits, and nothing that would make
/// it part of a larger expression.
fn is_bare_word(word: &str) -> bool {
    !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// The run count as a number of runs, or the refusal that names it.
///
/// Zero and below are refused rather than divided by: a mean over no runs is
/// not a small number, it is not a number, and `.time 1 + 1 0 times` has to say
/// so instead of printing `NaN`.
fn read_count(written: &str, command: &str) -> Result<usize, String> {
    let count: i64 = written.parse().map_err(|_| {
        format!("Error: '{command}' takes a whole number of runs: '{written}' is not a number.")
    })?;

    if count <= 0 {
        return Err(format!(
            "Error: '{command}' cannot run {count} time(s): there is no run to measure."
        ));
    }
    if count as usize > MAX_RUNS {
        return Err(format!(
            "Error: '{command}' will not run more than {MAX_RUNS} times; {count} was asked for."
        ));
    }

    Ok(count as usize)
}

/// The `.time` report: what was run, how many times, and the total and the mean.
///
/// One unit for both figures, chosen from the mean so that neither of them can
/// print as zero: the unit is the largest one the mean reaches, which makes the
/// total at least `runs` of it.
pub fn time_lines(command: &str, expression: &str, samples: &[Duration]) -> Vec<String> {
    let runs = samples.len().max(1);
    let total = samples.iter().map(Duration::as_nanos).sum::<u128>() as f64;
    let mean = total / runs as f64;
    let (scale, unit) = scale_of(mean);

    vec![
        header(command, expression, samples.len()),
        row("total:", total / scale, unit),
        row("mean:", mean / scale, unit),
    ]
}

/// The `.bench` report: the distribution over the runs, in the same unit
/// throughout.
///
/// The unit is chosen from the smallest sample, for the reason [`time_lines`]
/// chooses it from the mean — so the reported `min` cannot print as zero, and
/// every other figure it is compared against is at least as large.
pub fn bench_lines(command: &str, expression: &str, samples: &[Duration]) -> Vec<String> {
    let runs = samples.len().max(1);
    let nanos: Vec<f64> = samples
        .iter()
        .map(|sample| sample.as_nanos() as f64)
        .collect();
    let min = nanos.iter().copied().fold(f64::INFINITY, f64::min);
    let max = nanos.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mean = nanos.iter().sum::<f64>() / runs as f64;
    let median = median(&nanos);
    let (scale, unit) = scale_of(min);

    vec![
        header(command, expression, samples.len()),
        row("min:", min / scale, unit),
        row("median:", median / scale, unit),
        row("max:", max / scale, unit),
        row("mean:", mean / scale, unit),
    ]
}

/// The median of `samples`, which need not be sorted.
///
/// The mean of the two middle samples when there is an even number of them, so
/// that `min <= median <= max` holds for every run count rather than only for
/// odd ones.
fn median(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));

    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[middle]
    } else {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    }
}

/// The divisor and the unit `nanos` should be read in: the largest unit it
/// reaches, so a reported figure is at least one of it.
fn scale_of(nanos: f64) -> (f64, &'static str) {
    if nanos >= 1e9 {
        (1e9, "s")
    } else if nanos >= 1e6 {
        (1e6, "ms")
    } else if nanos >= 1e3 {
        (1e3, "µs")
    } else {
        (1.0, "ns")
    }
}

/// What was measured, over how many runs, and that the figures are wall time.
fn header(command: &str, expression: &str, runs: usize) -> String {
    format!("{command}: `{expression}`, {runs} run(s), wall time (figures per run)")
}

/// One labelled figure, to three decimals: two orders of magnitude more than
/// the host clock resolves at any of these scales.
fn row(label: &str, value: f64, unit: &str) -> String {
    format!("  {label:<6} {value:.3} {unit}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nanos(value: f64) -> Duration {
        Duration::from_nanos(value as u64)
    }

    #[test]
    fn edge_a_count_is_only_read_from_the_tail_the_user_marked() {
        assert_eq!(
            split_runs("1 + 1", "time", TIME_RUNS),
            Ok(Measurement {
                expression: "1 + 1",
                runs: TIME_RUNS
            }),
            "an expression that ends in a number is still one expression"
        );
        assert_eq!(
            split_runs("countdown(3) times", "time", TIME_RUNS),
            Ok(Measurement {
                expression: "countdown(3) times",
                runs: TIME_RUNS
            }),
            "a call with an argument named `times` is not a run count"
        );
        assert_eq!(
            split_runs("x + 1 4 times", "time", TIME_RUNS),
            Ok(Measurement {
                expression: "x + 1",
                runs: 4
            }),
            "a marked count is taken off the end"
        );
    }

    #[test]
    fn edge_zero_and_a_count_that_is_not_a_number_are_refused_by_name() {
        let zero = split_runs("1 + 1 0 times", "bench", BENCH_RUNS).expect_err("no runs");
        assert!(
            zero.contains("bench") && zero.contains('0'),
            "refused by name: {}",
            zero
        );

        let word = split_runs("1 + 1 many times", "bench", BENCH_RUNS).expect_err("not a number");
        assert!(
            word.contains("bench") && word.contains("many"),
            "the count that could not be read is quoted: {}",
            word
        );

        let huge = split_runs("1 + 1 99999999 times", "time", TIME_RUNS).expect_err("past the cap");
        assert!(
            huge.contains("time") && huge.contains(&MAX_RUNS.to_string()),
            "the cap is reported with the command: {}",
            huge
        );
    }

    #[test]
    fn bench_orders_the_figures_it_reports() {
        let samples: Vec<Duration> = [50.0, 10.0, 30.0, 20.0].iter().map(|n| nanos(*n)).collect();

        let lines = bench_lines("bench", "say 1", &samples);
        let joined = lines.join("\n");
        assert!(joined.contains("4 run(s)"), "how many: {}", joined);
        assert!(joined.contains("wall time"), "labelled: {}", joined);

        assert!(
            lines[1].contains("10.000"),
            "the smallest sample is the min: {:?}",
            lines
        );
        assert!(
            lines[2].contains("25.000"),
            "an even count takes the mean of the two middle samples: {:?}",
            lines
        );
        assert!(lines[3].contains("50.000"), "and the max: {:?}", lines);
        assert!(lines[4].contains("27.500"), "and the mean: {:?}", lines);
    }

    #[test]
    fn time_reports_a_total_that_is_the_mean_over_the_runs() {
        let samples: Vec<Duration> = (0..10).map(|_| nanos(1000.0)).collect();

        let lines = time_lines("time", "1 + 1", &samples);

        assert!(
            lines[1].contains("10.000 µs"),
            "10000 ns is ten microseconds, in the unit the mean reaches: {:?}",
            lines
        );
        assert!(lines[2].contains("1.000 µs"), "the mean: {:?}", lines);
    }
}
