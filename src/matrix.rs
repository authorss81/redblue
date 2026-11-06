use ndarray::Array2;

use crate::error::{Error, Result, Span};
use crate::value::{Fields, Value};

pub const OBJECT_NAME: &str = "matrix";
const ROWS_FIELD: &str = "rows";
const COLS_FIELD: &str = "cols";
const DATA_FIELD: &str = "data";
const MAX_ELEMENTS: usize = 1_000_000;
/// The most scalar multiply-adds one `matrix.matmul` call may ask for.
///
/// The element ceiling bounds storage, not work: two 1,000 by 1,000 operands
/// are each exactly at the element limit and their product needs a billion
/// multiply-adds. The guard runs before the kernel, so a program cannot turn a
/// small amount of source into an unbounded native loop.
const MAX_MULTIPLY_ADDS: u128 = 500_000_000;

pub fn is_matrix(name: &str, fields: &Fields) -> bool {
    name == OBJECT_NAME
        && fields.contains_key(ROWS_FIELD)
        && fields.contains_key(COLS_FIELD)
        && fields.contains_key(DATA_FIELD)
}

pub fn is_matrix_value(value: &Value) -> bool {
    matches!(value, Value::Object(name, fields) if is_matrix(name, fields))
}

fn failure(span: Span, message: impl Into<String>) -> Error {
    Error::Runtime(message.into(), span)
}

fn shape_text(rows: usize, cols: usize) -> String {
    format!("[{rows}, {cols}]")
}

fn whole(span: Span, value: &Value, what: &str) -> Result<usize> {
    match value {
        Value::Number(n)
            if n.is_finite() && *n >= 0.0 && n.fract() == 0.0 && *n <= usize::MAX as f64 =>
        {
            Ok(*n as usize)
        }
        Value::Number(n) => Err(failure(
            span,
            format!(
                "{what} must be a whole number of at least 0, got {}",
                Value::Number(*n)
            ),
        )),
        other => Err(failure(
            span,
            format!("{what} must be a number, got {}", other.type_name()),
        )),
    }
}

fn element_count(span: Span, rows: usize, cols: usize) -> Result<usize> {
    if rows > MAX_ELEMENTS || cols > MAX_ELEMENTS {
        return Err(failure(
            span,
            format!(
                "A matrix of {rows} by {cols} has a dimension larger than the limit; the limit is {MAX_ELEMENTS}"
            ),
        ));
    }
    let elements = rows.checked_mul(cols).ok_or_else(|| {
        failure(
            span,
            format!("A matrix of {rows} by {cols} is larger than this interpreter can hold"),
        )
    })?;
    if elements > MAX_ELEMENTS {
        return Err(failure(
            span,
            format!(
                "A matrix of {rows} by {cols} would have {elements} elements; the limit is {MAX_ELEMENTS}"
            ),
        ));
    }
    Ok(elements)
}

/// Refuses a product whose kernel work is past [`MAX_MULTIPLY_ADDS`].
///
/// The count is `rows(left) * columns(left) * columns(right)` — the number of
/// scalar multiply-adds a dense product performs — and it is checked before
/// `ndarray` starts, so the refusal costs no matrix work.
fn check_matmul_work(span: Span, left: &Array2<f64>, right: &Array2<f64>) -> Result<()> {
    let work = left.nrows() as u128 * left.ncols() as u128 * right.ncols() as u128;
    if work <= MAX_MULTIPLY_ADDS {
        return Ok(());
    }
    let (left_rows, left_cols) = left.dim();
    let (right_rows, right_cols) = right.dim();
    Err(failure(
        span,
        format!(
            "Cannot multiply matrices with shapes {} and {}: the product needs {work} multiply-adds, above the limit of {MAX_MULTIPLY_ADDS}",
            shape_text(left_rows, left_cols),
            shape_text(right_rows, right_cols)
        ),
    ))
}

fn finite_number(span: Span, value: &Value) -> Result<f64> {
    match value {
        Value::Number(n) if n.is_finite() => Ok(*n),
        Value::Number(n) => Err(failure(
            span,
            format!(
                "Matrix values must be finite numbers, got {}",
                Value::Number(*n)
            ),
        )),
        other => Err(failure(
            span,
            format!("Matrix values must be numbers, got {}", other.type_name()),
        )),
    }
}

fn data_list(span: Span, fields: &Fields) -> Result<&Value> {
    fields
        .get(DATA_FIELD)
        .ok_or_else(|| failure(span, "The matrix has no data"))
}

fn decode_fields(span: Span, fields: &Fields) -> Result<Array2<f64>> {
    let rows = fields
        .get(ROWS_FIELD)
        .ok_or_else(|| failure(span, "The matrix has no row count"))
        .and_then(|value| whole(span, value, "The matrix row count"))?;
    let cols = fields
        .get(COLS_FIELD)
        .ok_or_else(|| failure(span, "The matrix has no column count"))
        .and_then(|value| whole(span, value, "The matrix column count"))?;
    let expected = element_count(span, rows, cols)?;
    let Value::List(items) = data_list(span, fields)? else {
        return Err(failure(span, "The matrix data is not a list"));
    };
    if items.len() != expected {
        return Err(failure(
            span,
            format!(
                "The matrix says it is {} but holds {} values",
                shape_text(rows, cols),
                items.len()
            ),
        ));
    }
    let mut flat = Vec::with_capacity(expected);
    for item in items.iter() {
        flat.push(finite_number(span, item)?);
    }
    Array2::from_shape_vec((rows, cols), flat)
        .map_err(|_| failure(span, "The matrix shape does not match its data"))
}

pub fn array_of(span: Span, value: &Value) -> Result<Array2<f64>> {
    match value {
        Value::Object(name, fields) if is_matrix(name, fields) => decode_fields(span, fields),
        other => Err(failure(
            span,
            format!("Expected a matrix, got {}", other.type_name()),
        )),
    }
}

pub fn value_of(span: Span, array: &Array2<f64>) -> Result<Value> {
    let (rows, cols) = array.dim();
    let mut data = Vec::with_capacity(rows * cols);
    for number in array.iter() {
        if !number.is_finite() {
            return Err(failure(
                span,
                "A matrix operation produced a number that is not finite",
            ));
        }
        data.push(Value::Number(*number));
    }
    let mut fields = Fields::new();
    fields.insert(ROWS_FIELD.to_string(), Value::Number(rows as f64));
    fields.insert(COLS_FIELD.to_string(), Value::Number(cols as f64));
    fields.insert(DATA_FIELD.to_string(), Value::list(data));
    Ok(Value::object(OBJECT_NAME.to_string(), fields))
}

pub fn text(name: &str, fields: &Fields) -> Option<String> {
    if !is_matrix(name, fields) {
        return None;
    }
    let rows = match fields.get(ROWS_FIELD) {
        Some(Value::Number(n)) if n.is_finite() && *n >= 0.0 && n.fract() == 0.0 => *n as usize,
        _ => return None,
    };
    let cols = match fields.get(COLS_FIELD) {
        Some(Value::Number(n)) if n.is_finite() && *n >= 0.0 && n.fract() == 0.0 => *n as usize,
        _ => return None,
    };
    let Some(Value::List(items)) = fields.get(DATA_FIELD) else {
        return None;
    };
    if items.len() != rows.checked_mul(cols)? {
        return None;
    }
    let mut rendered = Vec::with_capacity(rows);
    for row in 0..rows {
        let start = row * cols;
        let line = items[start..start + cols]
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        rendered.push(format!("[{line}]"));
    }
    Some(format!(
        "matrix({}x{}, [{}])",
        rows,
        cols,
        rendered.join(", ")
    ))
}

fn dimensions(span: Span, args: &[Value], function: &str) -> Result<(usize, usize)> {
    let [rows, cols] = args else {
        return Err(failure(
            span,
            format!("{function} requires a row count and a column count"),
        ));
    };
    Ok((
        whole(span, rows, "The row count")?,
        whole(span, cols, "The column count")?,
    ))
}

fn two_matrices<'a>(
    span: Span,
    args: &'a [Value],
    function: &str,
) -> Result<(&'a Value, &'a Value)> {
    let [left, right] = args else {
        return Err(failure(span, format!("{function} requires two matrices")));
    };
    Ok((left, right))
}

fn same_shape(span: Span, function: &str, left: &Array2<f64>, right: &Array2<f64>) -> Result<()> {
    if left.dim() == right.dim() {
        return Ok(());
    }
    let (left_rows, left_cols) = left.dim();
    let (right_rows, right_cols) = right.dim();
    Err(failure(
        span,
        format!(
            "Cannot {function} matrices with shapes {} and {}",
            shape_text(left_rows, left_cols),
            shape_text(right_rows, right_cols)
        ),
    ))
}

fn index_of(span: Span, value: &Value, len: usize, what: &str) -> Result<usize> {
    let Value::Number(n) = value else {
        return Err(failure(span, format!("The {what} index must be a number")));
    };
    if !n.is_finite() || n.fract() != 0.0 {
        return Err(failure(
            span,
            format!(
                "Index {} is out of bounds: the matrix has {} {}{}, so a {what} index must be a whole number",
                Value::Number(*n),
                len,
                what,
                if len == 1 { "" } else { "s" }
            ),
        ));
    }
    let offset = if *n < 0.0 { None } else { Some(*n as i64) }
        .and_then(|offset| usize::try_from(offset).ok());
    match offset {
        Some(offset) if offset < len => Ok(offset),
        _ => Err(failure(
            span,
            format!(
                "Index {} is out of bounds: the matrix has {} {}{}",
                Value::Number(*n),
                len,
                what,
                if len == 1 { "" } else { "s" }
            ),
        )),
    }
}

fn row_values(array: &Array2<f64>, row: usize) -> Value {
    let mut values = Vec::with_capacity(array.ncols());
    for col in 0..array.ncols() {
        values.push(Value::Number(array[[row, col]]));
    }
    Value::list(values)
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

fn scaled_compensated_sum(array: &Array2<f64>, scale: f64) -> f64 {
    compensated_sum(array.iter().map(|number| number / scale))
        .unwrap_or_else(|| array.iter().map(|number| number / scale).sum())
}

fn sum_of(array: &Array2<f64>) -> f64 {
    if let Some(sum) = compensated_sum(array.iter().copied()) {
        return sum;
    }
    let scale = overflow_scale(array.len());
    scaled_compensated_sum(array, scale) * scale
}

fn mean_of(array: &Array2<f64>) -> f64 {
    let count = array.len() as f64;
    if let Some(sum) = compensated_sum(array.iter().copied()) {
        return sum / count;
    }
    let scale = overflow_scale(array.len());
    let scaled = scaled_compensated_sum(array, scale);
    let restored = scaled * scale;
    if restored.is_finite() {
        restored / count
    } else {
        (scaled / count) * scale
    }
}

pub fn index_row(span: Span, value: &Value, index: &Value) -> Result<Value> {
    let array = array_of(span, value)?;
    let row = index_of(span, index, array.nrows(), "row")?;
    Ok(row_values(&array, row))
}

pub fn builtin(span: Span, name: &str, args: &[Value]) -> Result<Option<Value>> {
    let result = match name {
        "matrix_zeros" => {
            let (rows, cols) = dimensions(span, args, name)?;
            element_count(span, rows, cols)?;
            value_of(span, &Array2::zeros((rows, cols)))?
        }
        "matrix_ones" => {
            let (rows, cols) = dimensions(span, args, name)?;
            element_count(span, rows, cols)?;
            value_of(span, &Array2::ones((rows, cols)))?
        }
        "matrix_identity" => {
            let [size] = args else {
                return Err(failure(span, format!("{name} requires one size")));
            };
            let size = whole(span, size, "The size")?;
            element_count(span, size, size)?;
            value_of(span, &Array2::eye(size))?
        }
        "matrix_from_lists" => {
            let [Value::List(rows)] = args else {
                return Err(failure(span, format!("{name} requires a list of rows")));
            };
            if rows.is_empty() {
                value_of(span, &Array2::zeros((0, 0)))?
            } else {
                let mut cols = None;
                let mut flat = Vec::new();
                for (row_number, row) in rows.iter().enumerate() {
                    let Value::List(values) = row else {
                        return Err(failure(
                            span,
                            format!(
                                "Each matrix row must be a list, but row {row_number} is {}",
                                row.type_name()
                            ),
                        ));
                    };
                    match cols {
                        Some(width) if width != values.len() => {
                            return Err(failure(
                                span,
                                format!(
                                    "Matrix rows must all have the same length, but row 0 has {width} and row {row_number} has {}",
                                    values.len()
                                ),
                            ))
                        }
                        None => cols = Some(values.len()),
                        _ => {}
                    }
                    element_count(span, row_number + 1, values.len())?;
                    for value in values.iter() {
                        flat.push(finite_number(span, value)?);
                    }
                }
                let rows_count = rows.len();
                let cols_count = cols.unwrap_or(0);
                let array = Array2::from_shape_vec((rows_count, cols_count), flat)
                    .map_err(|_| failure(span, "The matrix rows do not form a rectangle"))?;
                value_of(span, &array)?
            }
        }
        "matrix_to_lists" => {
            let [value] = args else {
                return Err(failure(span, format!("{name} requires a matrix")));
            };
            let array = array_of(span, value)?;
            let mut rows = Vec::with_capacity(array.nrows());
            for row in 0..array.nrows() {
                let mut values = Vec::with_capacity(array.ncols());
                for col in 0..array.ncols() {
                    values.push(Value::Number(array[[row, col]]));
                }
                rows.push(Value::list(values));
            }
            Value::list(rows)
        }
        "matrix_shape" => {
            let [value] = args else {
                return Err(failure(span, format!("{name} requires a matrix")));
            };
            let array = array_of(span, value)?;
            let (rows, cols) = array.dim();
            Value::list(vec![Value::Number(rows as f64), Value::Number(cols as f64)])
        }
        "matrix_add" | "matrix_subtract" | "matrix_multiply" | "matrix_divide" => {
            let (left, right) = two_matrices(span, args, name)?;
            let left = array_of(span, left)?;
            let right = array_of(span, right)?;
            same_shape(span, name.trim_start_matches("matrix_"), &left, &right)?;
            let array = match name {
                "matrix_add" => &left + &right,
                "matrix_subtract" => &left - &right,
                "matrix_multiply" => &left * &right,
                _ => {
                    if right.iter().any(|number| *number == 0.0) {
                        return Err(failure(span, "Cannot divide by zero in a matrix"));
                    }
                    &left / &right
                }
            };
            value_of(span, &array)?
        }
        "matrix_scale" => {
            let [value, factor] = args else {
                return Err(failure(
                    span,
                    format!("{name} requires a matrix and a number"),
                ));
            };
            let array = array_of(span, value)?;
            let factor = finite_number(span, factor)?;
            value_of(span, &(&array * factor))?
        }
        "matrix_matmul" => {
            let (left, right) = two_matrices(span, args, name)?;
            let left = array_of(span, left)?;
            let right = array_of(span, right)?;
            if left.ncols() != right.nrows() {
                let (left_rows, left_cols) = left.dim();
                let (right_rows, right_cols) = right.dim();
                return Err(failure(
                    span,
                    format!(
                        "Cannot multiply matrices with shapes {} and {}",
                        shape_text(left_rows, left_cols),
                        shape_text(right_rows, right_cols)
                    ),
                ));
            }
            element_count(span, left.nrows(), right.ncols())?;
            check_matmul_work(span, &left, &right)?;
            value_of(span, &left.dot(&right))?
        }
        "matrix_transpose" => {
            let [value] = args else {
                return Err(failure(span, format!("{name} requires a matrix")));
            };
            let array = array_of(span, value)?;
            value_of(span, &array.t().to_owned())?
        }
        "matrix_sum" => {
            let [value] = args else {
                return Err(failure(span, format!("{name} requires a matrix")));
            };
            let array = array_of(span, value)?;
            Value::number(sum_of(&array), span)?
        }
        "matrix_mean" => {
            let [value] = args else {
                return Err(failure(span, format!("{name} requires a matrix")));
            };
            let array = array_of(span, value)?;
            if array.is_empty() {
                return Err(failure(span, "Cannot take the mean of an empty matrix"));
            }
            Value::number(mean_of(&array), span)?
        }
        "matrix_min" | "matrix_max" => {
            let [value] = args else {
                return Err(failure(span, format!("{name} requires a matrix")));
            };
            let array = array_of(span, value)?;
            if array.is_empty() {
                return Err(failure(
                    span,
                    format!(
                        "Cannot take the {} of an empty matrix",
                        name.trim_start_matches("matrix_")
                    ),
                ));
            }
            let answer = if name == "matrix_min" {
                array.iter().copied().fold(f64::INFINITY, f64::min)
            } else {
                array.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            };
            Value::number(answer, span)?
        }
        "matrix_at" => {
            let [value, row, col] = args else {
                return Err(failure(
                    span,
                    format!("{name} requires a matrix, a row and a column"),
                ));
            };
            let array = array_of(span, value)?;
            let row = index_of(span, row, array.nrows(), "row")?;
            let col = index_of(span, col, array.ncols(), "column")?;
            Value::Number(array[[row, col]])
        }
        "matrix_row" | "matrix_column" => {
            let [value, index] = args else {
                return Err(failure(
                    span,
                    format!("{name} requires a matrix and an index"),
                ));
            };
            let array = array_of(span, value)?;
            if name == "matrix_row" {
                let row = index_of(span, index, array.nrows(), "row")?;
                row_values(&array, row)
            } else {
                let col = index_of(span, index, array.ncols(), "column")?;
                let mut values = Vec::with_capacity(array.nrows());
                for row in 0..array.nrows() {
                    values.push(Value::Number(array[[row, col]]));
                }
                Value::list(values)
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_matmul_work_limit_allows_the_exact_threshold() {
        let left = Array2::<f64>::zeros((500, 1000));
        let right = Array2::<f64>::zeros((1000, 1000));
        assert_eq!(
            500u128 * 1000 * 1000,
            MAX_MULTIPLY_ADDS,
            "the fixture must sit exactly on the threshold"
        );
        assert!(
            check_matmul_work(Span::unknown(), &left, &right).is_ok(),
            "a product at exactly the limit must be allowed"
        );
    }

    #[test]
    fn edge_matmul_work_limit_refuses_one_step_over() {
        let left = Array2::<f64>::zeros((501, 1000));
        let right = Array2::<f64>::zeros((1000, 1000));
        let error = check_matmul_work(Span::unknown(), &left, &right)
            .expect_err("a product one multiply-add over the limit must be refused");
        let Error::Runtime(message, _) = error else {
            panic!("the refusal must be a catchable runtime error, got {error:?}");
        };
        assert!(
            message.contains("multiply-adds") && message.contains("limit of 500000000"),
            "the refusal must name the work and the limit, got {message}"
        );
    }
}
