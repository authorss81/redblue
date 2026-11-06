//! Descriptive statistics over a list of numbers.
//!
//! These live on the shared builtin path, [`crate::runtime::builtin`], so the
//! tree-walker and the bytecode VM answer from one implementation. The exact
//! definitions are in SPEC.md § math: variance and standard deviation are the
//! population forms, an even median is the mean of the two middle values, a
//! percentile linearly interpolates between `(n-1) * p / 100` and the next
//! rank, and mode answers every tied value in ascending order.

use crate::error::{Error, Result, Span};
use crate::value::Value;

/// Answers the statistics builtins, or `None` when `name` is not one of them.
pub fn builtin(span: Span, name: &str, args: &[Value]) -> Result<Option<Value>> {
    let answer = match name {
        "mean" => {
            let numbers = one_list(span, name, args)?;
            non_empty(span, name, &numbers)?;
            let mean = mean_of(&numbers);
            Value::number(mean, span)?
        }
        "median" => {
            let mut numbers = one_list(span, name, args)?;
            Value::number(median_of(span, name, &mut numbers)?, span)?
        }
        "mode" => {
            let numbers = one_list(span, name, args)?;
            Value::list(
                modes_of(span, name, &numbers)?
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            )
        }
        "variance" => {
            let numbers = one_list(span, name, args)?;
            Value::number(variance_of(span, name, &numbers)?, span)?
        }
        "standard_deviation" => {
            let numbers = one_list(span, name, args)?;
            Value::number(variance_of(span, name, &numbers)?.sqrt(), span)?
        }
        "percentile" => {
            let numbers = one_list(span, name, args)?;
            let rank = match args.get(1) {
                Some(Value::Number(rank)) => *rank,
                Some(other) => {
                    return Err(Error::Runtime(
                        format!(
                            "percentile needs a number from 0 to 100, but was given {}",
                            other.type_name()
                        ),
                        span,
                    ))
                }
                None => {
                    return Err(Error::Runtime(
                        "percentile needs a list of numbers and a rank from 0 to 100".to_string(),
                        span,
                    ))
                }
            };
            Value::number(percentile_of(span, &numbers, rank)?, span)?
        }
        "correlation" => {
            let (left, right) = two_lists(span, name, args)?;
            Value::number(correlation_of(span, &left, &right)?, span)?
        }
        _ => return Ok(None),
    };
    Ok(Some(answer))
}

fn one_list(span: Span, name: &str, args: &[Value]) -> Result<Vec<f64>> {
    match args.first() {
        Some(Value::List(items)) => numbers_from(span, name, items),
        Some(other) => Err(Error::Runtime(
            format!(
                "{name} requires a list of numbers, but was given {}",
                other.type_name()
            ),
            span,
        )),
        None => Err(Error::Runtime(
            format!("{name} requires a list of numbers"),
            span,
        )),
    }
}

fn two_lists(span: Span, name: &str, args: &[Value]) -> Result<(Vec<f64>, Vec<f64>)> {
    let left = one_list(span, name, args)?;
    let right = match args.get(1) {
        Some(Value::List(items)) => numbers_from(span, name, items)?,
        Some(other) => {
            return Err(Error::Runtime(
                format!(
                    "{name} requires two lists of numbers, but the second is {}",
                    other.type_name()
                ),
                span,
            ))
        }
        None => {
            return Err(Error::Runtime(
                format!("{name} requires two lists of numbers"),
                span,
            ))
        }
    };
    Ok((left, right))
}

fn numbers_from(span: Span, name: &str, items: &[Value]) -> Result<Vec<f64>> {
    items
        .iter()
        .enumerate()
        .map(|(index, item)| match item {
            Value::Number(number) => Ok(*number),
            other => Err(Error::Runtime(
                format!(
                    "{name} requires a list of numbers, but item {index} is {}",
                    other.type_name()
                ),
                span,
            )),
        })
        .collect()
}

fn non_empty<'a>(span: Span, name: &str, numbers: &'a [f64]) -> Result<&'a [f64]> {
    if numbers.is_empty() {
        Err(Error::Runtime(
            format!("{name} requires a list with at least one number, but the list is empty"),
            span,
        ))
    } else {
        Ok(numbers)
    }
}

fn compensated_sum<I>(numbers: I) -> Option<f64>
where
    I: IntoIterator<Item = f64>,
{
    let mut sum = 0.0;
    let mut correction = 0.0;
    for number in numbers {
        let next = sum + number;
        if !next.is_finite() {
            return None;
        }
        if sum.abs() >= number.abs() {
            correction += (sum - next) + number;
        } else {
            correction += (number - next) + sum;
        }
        sum = next;
    }
    let total = sum + correction;
    total.is_finite().then_some(total)
}

fn overflow_scale(count: usize) -> f64 {
    let exponent = count.max(1).next_power_of_two().trailing_zeros() + 1;
    2f64.powi(exponent as i32)
}

fn scaled_compensated_sum(numbers: &[f64], scale: f64) -> f64 {
    compensated_sum(numbers.iter().map(|number| number / scale))
        .unwrap_or_else(|| numbers.iter().map(|number| number / scale).sum())
}

fn mean_of(numbers: &[f64]) -> f64 {
    let count = numbers.len() as f64;
    if let Some(sum) = compensated_sum(numbers.iter().copied()) {
        return sum / count;
    }
    let scale = overflow_scale(numbers.len());
    let scaled = scaled_compensated_sum(numbers, scale);
    let restored = scaled * scale;
    if restored.is_finite() {
        restored / count
    } else {
        (scaled / count) * scale
    }
}

fn sorted(numbers: &[f64]) -> Vec<f64> {
    let mut sorted = numbers.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted
}

fn median_of(span: Span, name: &str, numbers: &mut [f64]) -> Result<f64> {
    non_empty(span, name, numbers)?;
    numbers.sort_by(f64::total_cmp);
    let middle = numbers.len() / 2;
    Ok(if numbers.len().is_multiple_of(2) {
        // Halves first so two values near `f64::MAX` cannot overflow to
        // infinity on the way to their mean.
        numbers[middle - 1] / 2.0 + numbers[middle] / 2.0
    } else {
        numbers[middle]
    })
}

fn modes_of(span: Span, name: &str, numbers: &[f64]) -> Result<Vec<f64>> {
    non_empty(span, name, numbers)?;
    // `-0.0` and `0.0` are equal to a program (`is`), so they are one value
    // here rather than two modes with one occurrence each.
    let mut sorted: Vec<f64> = numbers
        .iter()
        .map(|number| if *number == 0.0 { 0.0 } else { *number })
        .collect();
    sorted.sort_by(f64::total_cmp);

    let mut runs: Vec<(f64, usize)> = Vec::new();
    for number in sorted {
        match runs.last_mut() {
            Some((value, count)) if *value == number => *count += 1,
            _ => runs.push((number, 1)),
        }
    }
    let highest = runs.iter().map(|(_, count)| *count).max().unwrap_or(0);
    let mut modes: Vec<f64> = runs
        .into_iter()
        .filter(|(_, count)| *count == highest)
        .map(|(value, _)| value)
        .collect();
    modes.sort_by(f64::total_cmp);
    Ok(modes)
}

fn variance_of(span: Span, name: &str, numbers: &[f64]) -> Result<f64> {
    non_empty(span, name, numbers)?;
    let mean = mean_of(numbers);
    // Average the squared deviations instead of summing them and dividing.
    // Their total can overflow to infinity even when their mean is a finite
    // number: [1e154, -1e154] has variance 1e308, but 1e308 + 1e308 is not
    // representable. `mean_of` scales only when the plain sum overflows, so
    // ordinary lists keep the direct path.
    let squares: Vec<f64> = numbers
        .iter()
        .map(|number| (number - mean) * (number - mean))
        .collect();
    Ok(mean_of(&squares))
}

fn percentile_of(span: Span, numbers: &[f64], rank: f64) -> Result<f64> {
    if !rank.is_finite() || !(0.0..=100.0).contains(&rank) {
        return Err(Error::Runtime(
            format!("percentile needs a rank from 0 to 100, but was given {rank}"),
            span,
        ));
    }
    non_empty(span, "percentile", numbers)?;
    let sorted = sorted(numbers);
    let position = (sorted.len() - 1) as f64 * rank / 100.0;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    let weight = position - lower as f64;
    Ok(sorted[lower] * (1.0 - weight) + sorted[upper] * weight)
}

fn correlation_of(span: Span, left: &[f64], right: &[f64]) -> Result<f64> {
    if left.len() != right.len() {
        return Err(Error::Runtime(
            format!(
                "correlation requires two lists of the same length, but they hold {} and {} numbers",
                left.len(),
                right.len()
            ),
            span,
        ));
    }
    if left.len() < 2 {
        return Err(Error::Runtime(
            "correlation requires at least two pairs of numbers".to_string(),
            span,
        ));
    }
    let left_scale = left
        .iter()
        .fold(0.0f64, |scale, value| scale.max(value.abs()));
    let right_scale = right
        .iter()
        .fold(0.0f64, |scale, value| scale.max(value.abs()));
    if left_scale == 0.0 || right_scale == 0.0 {
        return Err(Error::Runtime(
            "correlation has no answer when either list never varies".to_string(),
            span,
        ));
    }
    let left_mean = mean_of(left) / left_scale;
    let right_mean = mean_of(right) / right_scale;
    let mut numerator = 0.0;
    let mut left_squares = 0.0;
    let mut right_squares = 0.0;
    for (left_item, right_item) in left.iter().zip(right) {
        let left_delta = left_item / left_scale - left_mean;
        let right_delta = right_item / right_scale - right_mean;
        numerator += left_delta * right_delta;
        left_squares += left_delta * left_delta;
        right_squares += right_delta * right_delta;
    }
    if !numerator.is_finite() || !left_squares.is_finite() || !right_squares.is_finite() {
        return Err(Error::Runtime(
            "correlation is too large to hold as a finite number".to_string(),
            span,
        ));
    }
    if left_squares == 0.0 || right_squares == 0.0 {
        return Err(Error::Runtime(
            "correlation has no answer when either list never varies".to_string(),
            span,
        ));
    }
    // Rounding can put a perfect correlation a hair outside [-1, 1]; the
    // definition has no value there, so the answer is clamped rather than
    // allowed to state a correlation greater than one.
    let denominator = (left_squares * right_squares).sqrt();
    if !denominator.is_finite() {
        return Err(Error::Runtime(
            "correlation is too large to hold as a finite number".to_string(),
            span,
        ));
    }
    Ok((numerator / denominator).clamp(-1.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbers(texts: &[&str]) -> Vec<f64> {
        texts
            .iter()
            .map(|text| text.parse::<f64>().expect("test number"))
            .collect()
    }

    fn run(name: &str, args: &[Value]) -> Result<Value> {
        builtin(Span::new(1, 1), name, args)
            .map(|answer| answer.expect("name should be a statistics builtin"))
    }

    fn list(items: &[f64]) -> Value {
        Value::list(items.iter().copied().map(Value::Number).collect())
    }

    #[test]
    fn mean_and_median_agree_with_hand_computation() {
        let values = numbers(&["1", "2", "3", "4"]);
        assert_eq!(run("mean", &[list(&values)]).unwrap(), Value::Number(2.5));
        assert_eq!(run("median", &[list(&values)]).unwrap(), Value::Number(2.5));
        let odd = numbers(&["1", "2", "40"]);
        assert_eq!(run("median", &[list(&odd)]).unwrap(), Value::Number(2.0));
    }

    #[test]
    fn edge_statistics_refuse_an_empty_list() {
        for name in ["mean", "median", "mode", "variance", "standard_deviation"] {
            assert!(
                run(name, &[list(&[])]).is_err(),
                "{name} accepted an empty list"
            );
        }
        assert!(run("percentile", &[list(&[]), Value::Number(50.0)]).is_err());
    }

    #[test]
    fn mode_answers_every_tie_in_ascending_order() {
        let values = numbers(&["3", "1", "3", "1", "2"]);
        assert_eq!(
            run("mode", &[list(&values)]).unwrap(),
            Value::list(vec![Value::Number(1.0), Value::Number(3.0)])
        );
    }

    #[test]
    fn edge_mean_and_variance_do_not_overflow_on_large_finite_values() {
        let values = numbers(&["1e308", "1e308"]);
        assert_eq!(run("mean", &[list(&values)]).unwrap(), Value::Number(1e308));
        assert_eq!(
            run("variance", &[list(&values)]).unwrap(),
            Value::Number(0.0)
        );
        let maximum = numbers(&[
            "1.7976931348623157e308",
            "1.7976931348623157e308",
            "1.7976931348623157e308",
        ]);
        assert_eq!(
            run("mean", &[list(&maximum)]).unwrap(),
            Value::Number(f64::MAX)
        );
        let cancelling = numbers(&["1e308", "1e308", "-1e308", "-1e308", "1e-308"]);
        assert_eq!(
            run("mean", &[list(&cancelling)]).unwrap(),
            Value::Number(2e-309)
        );
    }

    #[test]
    fn variance_and_deviation_are_the_population_forms() {
        let values = numbers(&["2", "4", "4", "4", "5", "5", "7", "9"]);
        assert_eq!(
            run("variance", &[list(&values)]).unwrap(),
            Value::Number(4.0)
        );
        assert_eq!(
            run("standard_deviation", &[list(&values)]).unwrap(),
            Value::Number(2.0)
        );
    }

    #[test]
    fn percentile_interpolates_between_ranks() {
        let values = numbers(&["1", "2", "3", "4"]);
        assert_eq!(
            run("percentile", &[list(&values), Value::Number(25.0)]).unwrap(),
            Value::Number(1.75)
        );
        assert_eq!(
            run("percentile", &[list(&values), Value::Number(0.0)]).unwrap(),
            Value::Number(1.0)
        );
        assert_eq!(
            run("percentile", &[list(&values), Value::Number(100.0)]).unwrap(),
            Value::Number(4.0)
        );
    }

    #[test]
    fn edge_percentile_refuses_a_rank_outside_zero_to_one_hundred() {
        let values = numbers(&["1", "2"]);
        assert!(run("percentile", &[list(&values), Value::Number(-1.0)]).is_err());
        assert!(run("percentile", &[list(&values), Value::Number(101.0)]).is_err());
        assert!(run("percentile", &[list(&values), Value::Number(f64::NAN)]).is_err());
    }

    #[test]
    fn correlation_of_a_line_is_one_and_of_opposites_is_minus_one() {
        let x = numbers(&["1", "2", "3"]);
        let y = numbers(&["2", "4", "6"]);
        let rising = match run("correlation", &[list(&x), list(&y)]).unwrap() {
            Value::Number(number) => number,
            other => panic!("correlation should answer a number, got {other:?}"),
        };
        assert!((rising - 1.0).abs() < 1e-12, "the rising line was {rising}");
        let z = numbers(&["6", "4", "2"]);
        let falling = match run("correlation", &[list(&x), list(&z)]).unwrap() {
            Value::Number(number) => number,
            other => panic!("correlation should answer a number, got {other:?}"),
        };
        assert!(
            (falling + 1.0).abs() < 1e-12,
            "the falling line was {falling}"
        );
    }

    #[test]
    fn edge_correlation_does_not_overflow_on_very_large_finite_values() {
        let left = numbers(&["1e200", "-1e200"]);
        let right = numbers(&["1", "2"]);
        assert_eq!(
            run("correlation", &[list(&left), list(&right)]).unwrap(),
            Value::Number(-1.0)
        );
        let maximum = numbers(&["1.7976931348623157e308", "-1.7976931348623157e308"]);
        let answer = run("correlation", &[list(&maximum), list(&right)]).unwrap();
        let Value::Number(answer) = answer else {
            panic!("correlation should answer a number, got {answer:?}");
        };
        assert!(
            (answer + 1.0).abs() < 1e-12,
            "the extreme falling line was {answer}"
        );
    }

    #[test]
    fn edge_correlation_refuses_a_flat_list_and_mismatched_lengths() {
        let flat = numbers(&["1", "1", "1"]);
        let rising = numbers(&["1", "2", "3"]);
        assert!(run("correlation", &[list(&flat), list(&rising)]).is_err());
        assert!(run("correlation", &[list(&flat), list(&[1.0, 2.0])]).is_err());
    }

    #[test]
    fn type_mismatch_is_a_runtime_error_naming_the_item() {
        let bad = Value::list(vec![Value::Number(1.0), Value::Text("two".to_string())]);
        let error = run("mean", &[bad]).expect_err("text in a number list should fail");
        assert!(error.to_string().contains("item 1 is text"), "{error}");
    }
}
