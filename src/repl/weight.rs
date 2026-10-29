//! What the session weighs, and where its time went.
//!
//! `.mem` and `.profile` are observations: they read bindings, history and the
//! timings a just-finished block left behind, and they change none of them. The
//! arithmetic lives here so it can be read and tested without a session, and so
//! the report is one document rather than a format spread over a dispatch arm.
//!
//! Sizes are logical sizes, not allocator sizes. A text costs its own bytes, a
//! list its elements, a record its keys and values. That is the number a reader
//! can check against what they put in the binding, and it is honest about being
//! an estimate: no test asserts an allocator's exact figure.

use std::collections::HashSet;

use crate::parser::{Expr, Statement, Stmt};
use crate::value::{FunctionBody, Value};

/// How many bindings the largest-first listing names.
///
/// A listing is an answer, not a dump. Ten is enough to show what a session is
/// carrying without turning a session with hundreds of bindings into a wall.
const MAX_LARGEST_BINDINGS: usize = 10;

/// One container still to be measured.
///
/// The walk is an explicit stack rather than recursion, so how deeply a value
/// nests costs heap and not the caller's stack. That is what lets `.mem` answer
/// for a value a program built a million levels deep instead of overflowing
/// before it can print a single byte count.
enum Pending<'a> {
    Value(&'a Value),
    Fields(&'a std::sync::Arc<crate::value::Fields>),
    Statement(&'a Stmt),
    Expression(&'a Expr),
}

/// The logical bytes a value holds.
///
/// Shared storage is walked once per call: a list that appears twice inside one
/// value is one allocation and is counted once. The `seen` set is keyed by the
/// `Arc` address, which is stable for the lifetime of the value, and is what
/// keeps a value that reaches itself from being walked forever. The explicit
/// stack is what keeps a deeply nested acyclic value from reaching the process
/// stack instead.
pub fn value_bytes(value: &Value) -> usize {
    let mut seen = HashSet::new();
    let mut pending = vec![Pending::Value(value)];
    let mut bytes = 0;

    while let Some(item) = pending.pop() {
        match item {
            Pending::Value(
                Value::Nothing
                | Value::Number(_)
                | Value::YesNo(_)
                | Value::Date(_)
                | Value::Duration(_),
            ) => {}
            Pending::Value(Value::Text(text)) => bytes += text.len(),
            Pending::Value(Value::Builtin(name)) => bytes += name.len(),
            Pending::Value(Value::List(items)) => {
                let address = std::sync::Arc::as_ptr(items) as *const () as usize;
                if seen.insert(address) {
                    pending.extend(items.iter().map(Pending::Value));
                }
            }
            Pending::Value(Value::Record(fields)) => pending.push(Pending::Fields(fields)),
            Pending::Value(Value::Object(name, fields)) => {
                bytes += name.len();
                pending.push(Pending::Fields(fields));
            }
            Pending::Value(Value::Function(function)) => {
                bytes +=
                    function.name.len() + function.params.iter().map(String::len).sum::<usize>();
                match &function.body {
                    FunctionBody::Statements(body) => {
                        let address = std::sync::Arc::as_ptr(body) as *const () as usize;
                        if seen.insert(address) {
                            pending.extend(body.iter().map(Pending::Statement));
                        }
                    }
                    FunctionBody::Block { chunk, path } => {
                        bytes += path.len() * std::mem::size_of::<u32>();
                        let address = std::sync::Arc::as_ptr(chunk) as *const () as usize;
                        if seen.insert(address) {
                            bytes += chunk.encode().len();
                        }
                    }
                }
                let address = std::sync::Arc::as_ptr(&function.captured) as *const () as usize;
                if seen.insert(address) {
                    for scope in function.captured.iter() {
                        for (name, value) in scope {
                            bytes += name.len();
                            pending.push(Pending::Value(value));
                        }
                    }
                }
            }
            Pending::Statement(statement) => {
                bytes += std::mem::size_of::<Stmt>();
                match &statement.statement {
                    Statement::Say(expr) | Statement::Expr(expr) | Statement::Print(expr) => {
                        pending.push(Pending::Expression(expr));
                    }
                    Statement::Set { name, value } | Statement::Constant { name, value } => {
                        bytes += name.len();
                        pending.push(Pending::Expression(value));
                    }
                    Statement::SetProperty {
                        object,
                        property,
                        value,
                    } => {
                        bytes += object.len() + property.len();
                        pending.push(Pending::Expression(value));
                    }
                    Statement::If {
                        condition,
                        then_branch,
                        else_branch,
                    } => {
                        pending.push(Pending::Expression(condition));
                        pending.extend(then_branch.iter().map(Pending::Statement));
                        pending.extend(else_branch.iter().map(Pending::Statement));
                    }
                    Statement::Unless { condition, body } => {
                        pending.push(Pending::Expression(condition));
                        pending.extend(body.iter().map(Pending::Statement));
                    }
                    Statement::ForEach {
                        variable,
                        iterable,
                        body,
                    } => {
                        bytes += variable.len();
                        pending.push(Pending::Expression(iterable));
                        pending.extend(body.iter().map(Pending::Statement));
                    }
                    Statement::ForRange {
                        variable,
                        start,
                        end,
                        step,
                        body,
                    } => {
                        bytes += variable.len();
                        pending.push(Pending::Expression(start));
                        pending.push(Pending::Expression(end));
                        if let Some(step) = step {
                            pending.push(Pending::Expression(step));
                        }
                        pending.extend(body.iter().map(Pending::Statement));
                    }
                    Statement::Repeat { count, body } => {
                        pending.push(Pending::Expression(count));
                        pending.extend(body.iter().map(Pending::Statement));
                    }
                    Statement::RepeatUntil {
                        body, condition, ..
                    } => {
                        pending.push(Pending::Expression(condition));
                        pending.extend(body.iter().map(Pending::Statement));
                    }
                    Statement::While { condition, body } => {
                        pending.push(Pending::Expression(condition));
                        pending.extend(body.iter().map(Pending::Statement));
                    }
                    Statement::Break | Statement::Skip => {}
                    Statement::Return(value) | Statement::GiveBack(value) => {
                        if let Some(value) = value {
                            pending.push(Pending::Expression(value));
                        }
                    }
                    Statement::Function {
                        name, params, body, ..
                    }
                    | Statement::Method {
                        name, params, body, ..
                    } => {
                        bytes += name.len() + params.iter().map(String::len).sum::<usize>();
                        pending.extend(body.iter().map(Pending::Statement));
                    }
                    Statement::Has { name, default } => {
                        bytes += name.len();
                        if let Some(default) = default {
                            pending.push(Pending::Expression(default));
                        }
                    }
                    Statement::Object {
                        name,
                        extends,
                        body,
                    } => {
                        bytes += name.len();
                        if let Some(extends) = extends {
                            bytes += extends.len();
                        }
                        pending.extend(body.iter().map(Pending::Statement));
                    }
                    Statement::Try {
                        body,
                        catch_var,
                        catch_kind: _,
                        catch_body,
                        finally_body,
                    } => {
                        if let Some(catch_var) = catch_var {
                            bytes += catch_var.len();
                        }
                        pending.extend(body.iter().map(Pending::Statement));
                        pending.extend(catch_body.iter().map(Pending::Statement));
                        pending.extend(finally_body.iter().map(Pending::Statement));
                    }
                    Statement::Import(items) => {
                        for item in items {
                            bytes += item.name.len();
                            if let Some(alias) = &item.alias {
                                bytes += alias.len();
                            }
                        }
                    }
                    Statement::Module { name, body } => {
                        bytes += name.len();
                        pending.extend(body.iter().map(Pending::Statement));
                    }
                    Statement::Export { names, .. } => {
                        bytes += names.iter().map(String::len).sum::<usize>();
                    }
                    Statement::Test { name, body } => {
                        bytes += name.len();
                        pending.extend(body.iter().map(Pending::Statement));
                    }
                }
            }
            Pending::Expression(expression) => {
                bytes += std::mem::size_of::<Expr>();
                match expression {
                    Expr::Number(_) | Expr::YesNo(_) | Expr::Nothing => {}
                    Expr::Text(text) => bytes += text.len(),
                    Expr::Variable(name, _) => bytes += name.len(),
                    Expr::Binary { left, right, .. } => {
                        pending.push(Pending::Expression(right));
                        pending.push(Pending::Expression(left));
                    }
                    Expr::Unary { expr, .. } => pending.push(Pending::Expression(expr)),
                    Expr::Call { name, args } => {
                        bytes += name.len();
                        pending.extend(args.iter().map(Pending::Expression));
                    }
                    Expr::Property { object, property } => {
                        bytes += property.len();
                        pending.push(Pending::Expression(object));
                    }
                    Expr::MethodCall {
                        receiver,
                        method,
                        args,
                    } => {
                        bytes += method.len();
                        pending.push(Pending::Expression(receiver));
                        pending.extend(args.iter().map(Pending::Expression));
                    }
                    Expr::Index { object, index } => {
                        pending.push(Pending::Expression(index));
                        pending.push(Pending::Expression(object));
                    }
                    Expr::InterpolatedText(parts) => {
                        pending.extend(parts.iter().map(Pending::Expression));
                    }
                    Expr::List(items) => pending.extend(items.iter().map(Pending::Expression)),
                    Expr::Record(fields) => {
                        for (name, value) in fields {
                            bytes += name.len();
                            pending.push(Pending::Expression(value));
                        }
                    }
                    Expr::FunctionLiteral { params, body } => {
                        bytes += params.iter().map(String::len).sum::<usize>();
                        pending.extend(body.iter().map(Pending::Statement));
                    }
                    Expr::Expect { actual, expected } => {
                        pending.push(Pending::Expression(expected));
                        pending.push(Pending::Expression(actual));
                    }
                    Expr::MightFail(expr) => pending.push(Pending::Expression(expr)),
                }
            }
            Pending::Fields(fields) => {
                let address = std::sync::Arc::as_ptr(fields) as *const () as usize;
                if seen.insert(address) {
                    for (name, value) in fields.iter() {
                        bytes += name.len();
                        pending.push(Pending::Value(value));
                    }
                }
            }
        }
    }

    bytes
}

/// The binary units, largest first, as `(bytes per unit, name)`.
///
/// `EiB` is the largest that fits a 64-bit `usize`, and every unit down to
/// `KiB` is here so a count is never expressed as thousands of the next unit
/// down (`1024.0 MiB` for a gibibyte).
const HUMAN_UNITS: [(u128, &str); 6] = [
    (1u128 << 60, "EiB"),
    (1u128 << 50, "PiB"),
    (1u128 << 40, "TiB"),
    (1u128 << 30, "GiB"),
    (1u128 << 20, "MiB"),
    (1u128 << 10, "KiB"),
];

/// A byte count in human units, with the raw count in parentheses.
///
/// The raw count is always present so a small session and a large one read
/// alike: `0 B (0 B)`, `512 B (512 B)`, `10.0 KiB (10240 B)` and
/// `5.0 MiB (5242880 B)` each say exactly what they mean. Base 1024, because a
/// reader comparing this to what they typed is comparing memory, not disk, so
/// the units are the binary ones: `KiB`, `MiB`, `GiB`, `TiB`, `PiB` and `EiB`,
/// not the decimal `KB`/`MB`.
///
/// The one decimal is computed with integer arithmetic. Casting the count to
/// `f64` first would round any value past its 53-bit mantissa (about 9 PiB), so
/// a session holding an exabyte would report a unit figure that is not its own
/// truncated quotient. The raw count in parentheses is exact either way.
pub fn human_bytes(bytes: usize) -> String {
    for (unit, name) in HUMAN_UNITS {
        if bytes as u128 >= unit {
            return human_unit(bytes, unit, name);
        }
    }
    format!("{} B ({} B)", bytes, bytes)
}

/// `bytes` as a count of `unit` bytes, rounded to one decimal place.
///
/// The division and the rounding are done in `u128` so neither the quotient nor
/// `remainder * 10` can overflow, and so the printed whole part is the exact
/// quotient rather than an `f64` approximation of it.
fn human_unit(bytes: usize, unit: u128, name: &str) -> String {
    format!(
        "{} {name} ({} B)",
        decimal_quotient(bytes as u128, unit, 1),
        bytes
    )
}

/// `value / unit` to `places` decimal places, rounded half up.
///
/// Integer arithmetic throughout, for the same reason [`human_unit`] uses it:
/// an `f64` division would round a value past the mantissa's exact range, and a
/// profile or a report should print the quotient the integers gave.
fn decimal_quotient(value: u128, unit: u128, places: u32) -> String {
    let factor = 10u128.pow(places);
    let mut whole = value / unit;
    let remainder = value % unit;
    let mut fraction = (remainder * factor + unit / 2) / unit;
    if fraction == factor {
        whole += 1;
        fraction = 0;
    }
    format!("{whole}.{fraction:0width$}", width = places as usize)
}

/// The `.mem` report: bindings, history and heap, each named, then the largest
/// bindings from heaviest to lightest.
///
/// The bindings are already ordered by the caller, so this does not sort: the
/// session knows how it broke ties between equal sizes, and a second sort here
/// would be a second answer. An empty session still gets a number for every
/// figure rather than a blank line.
pub fn memory_lines(
    bindings: &[(String, usize)],
    history_entries: usize,
    history_bytes: usize,
) -> Vec<String> {
    let binding_bytes: usize = bindings.iter().map(|(_, bytes)| *bytes).sum();

    let mut lines = vec![
        "Memory:".to_string(),
        format!(
            "  bindings: {} ({})",
            bindings.len(),
            human_bytes(binding_bytes)
        ),
        format!(
            "  history: {} entr{} ({})",
            history_entries,
            if history_entries == 1 { "y" } else { "ies" },
            human_bytes(history_bytes)
        ),
        format!("  heap: {}", human_bytes(binding_bytes)),
    ];

    if bindings.is_empty() {
        lines.push("  largest bindings: (none)".to_string());
        return lines;
    }

    lines.push("  largest bindings:".to_string());
    for (name, bytes) in bindings.iter().take(MAX_LARGEST_BINDINGS) {
        lines.push(format!("    {:<20}  {}", name, human_bytes(*bytes)));
    }
    lines
}

/// The timings a multiline block left behind, and the source lines they name.
///
/// One clock reading per source line, in nanoseconds, in the order the VM first
/// finished a statement on that line. The source is kept so a report can quote
/// the line beside its figure rather than asking the reader to count.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlockProfile {
    pub lines: Vec<String>,
    pub timings: Vec<(usize, u128)>,
}

/// The `.profile` report: every line of the last block, slowest first.
///
/// The total is the sum of the line figures, so the report adds up by
/// construction. That is the arithmetic a reader can check whatever the machine
/// did that day: a per-line breakdown whose parts do not sum to its total is a
/// set of numbers, not a profile.
pub fn profile_lines(block: &BlockProfile) -> Vec<String> {
    let mut rows: Vec<(usize, u128)> = (1..=block.lines.len()).map(|line| (line, 0)).collect();

    for (line, nanos) in &block.timings {
        if let Some((_, recorded)) = rows.iter_mut().find(|(number, _)| number == line) {
            *recorded += *nanos;
        } else {
            rows.push((*line, *nanos));
        }
    }

    rows.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));

    let total: u128 = rows.iter().map(|(_, nanos)| *nanos).sum();
    let largest = rows.iter().map(|(_, nanos)| *nanos).max().unwrap_or(0);
    let (scale, unit) = duration_scale(largest);

    let mut lines = vec![
        format!(
            "Profile: last block, {} line(s), wall time",
            block.lines.len()
        ),
        format!("  total: {} {}", decimal_quotient(total, scale, 3), unit),
    ];

    for (number, nanos) in rows {
        let source = number
            .checked_sub(1)
            .and_then(|index| block.lines.get(index))
            .map(|line| line.trim())
            .unwrap_or("");
        lines.push(format!(
            "  line {number}: {} {unit}  {source}",
            decimal_quotient(nanos, scale, 3)
        ));
    }

    lines
}

/// Splits `source` into the physical lines the lexer numbers.
///
/// The lexer increments its line on `\n`, including a `\n` that sits inside a
/// text literal, because a literal is still source and its positions are the
/// ones a statement span records. `str::lines` agrees on those positions today,
/// but this says the rule explicitly: split on `\n`, take the `\r` of a `\r\n`
/// pair off the line it belongs to, and do not invent a line after a trailing
/// break. A report that used a different rule would quote one statement beside
/// another statement's figure.
pub fn source_lines(source: &str) -> Vec<String> {
    if source.is_empty() {
        return Vec::new();
    }

    let mut lines: Vec<String> = source
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
        .collect();
    if source.ends_with('\n') {
        lines.pop();
    }
    lines
}

/// The divisor and unit the largest line figure should be read in, so that the
/// slowest line is at least one of the unit and no line prints as `0 ns` unless
/// it really took no time the clock could see.
fn duration_scale(nanos: u128) -> (u128, &'static str) {
    if nanos >= 1_000_000_000 {
        (1_000_000_000, "s")
    } else if nanos >= 1_000_000 {
        (1_000_000, "ms")
    } else if nanos >= 1_000 {
        (1_000, "µs")
    } else {
        (1, "ns")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(bytes: usize) -> Value {
        Value::Text("x".repeat(bytes))
    }

    fn function_with_body(body: FunctionBody) -> Value {
        Value::Function(crate::value::FunctionValue::new(
            "f".to_string(),
            vec!["a".to_string()],
            body,
            std::sync::Arc::new(Vec::new()),
        ))
    }

    #[test]
    fn edge_human_bytes_names_zero_and_keeps_the_raw_count() {
        assert_eq!(human_bytes(0), "0 B (0 B)");
        assert_eq!(human_bytes(999), "999 B (999 B)");
        assert_eq!(human_bytes(1024), "1.0 KiB (1024 B)");
        assert_eq!(human_bytes(10 * 1024), "10.0 KiB (10240 B)");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.0 MiB (5242880 B)");
        assert_eq!(human_bytes(1024 * 1024 * 1024), "1.0 GiB (1073741824 B)");
        assert_eq!(
            human_bytes(2 * 1024 * 1024 * 1024 * 1024),
            "2.0 TiB (2199023255552 B)"
        );
    }

    /// Past `f64`'s exact-integer range the human figure must still be the
    /// integer quotient, not the mantissa-rounded approximation.
    #[test]
    fn edge_human_bytes_stays_exact_past_the_float_mantissa() {
        let unit = 1024 * 1024;
        let bytes = usize::MAX - (usize::MAX % unit);
        let rendered = human_unit(bytes, unit as u128, "MiB");
        let whole = bytes as u128 / unit as u128;

        assert!(
            rendered.ends_with(&format!("({} B)", bytes)),
            "the raw count is exact: {}",
            rendered
        );
        assert!(
            rendered.starts_with(&format!("{whole}.")),
            "the unit figure is the exact quotient: {}",
            rendered
        );

        let (top_unit, _) = HUMAN_UNITS[0];
        let bytes = usize::MAX - (usize::MAX % top_unit as usize);
        let rendered = human_bytes(bytes);
        let whole = bytes as u128 / top_unit;
        assert!(
            rendered.ends_with(&format!("({} B)", bytes)),
            "the largest unit keeps the raw count exact: {}",
            rendered
        );
        assert!(
            rendered.starts_with(&format!("{whole}.")),
            "the largest unit is the exact quotient: {}",
            rendered
        );
    }

    #[test]
    fn val_bytes_covers_text_lists_records_and_sharing() {
        assert_eq!(value_bytes(&Value::Nothing), 0);
        assert_eq!(value_bytes(&text(10)), 10);

        let shared = std::sync::Arc::new(vec![text(3), text(4)]);
        let list = Value::List(std::sync::Arc::clone(&shared));
        assert_eq!(value_bytes(&list), 7, "both elements are counted");

        let mut fields = crate::value::Fields::new();
        fields.insert("key".to_string(), text(5));
        assert_eq!(value_bytes(&Value::record(fields)), 3 + 5);
    }

    #[test]
    fn val_bytes_covers_scalars_builtins_objects_and_functions() {
        assert_eq!(value_bytes(&Value::Number(42.0)), 0);
        assert_eq!(value_bytes(&Value::YesNo(true)), 0);
        assert_eq!(value_bytes(&Value::Builtin("say".to_string())), 3);

        let mut object_fields = crate::value::Fields::new();
        object_fields.insert("x".to_string(), text(2));
        assert_eq!(
            value_bytes(&Value::object("Point".to_string(), object_fields)),
            "Point".len() + "x".len() + 2
        );

        let mut captured = crate::value::CapturedScope::new();
        captured.insert("x".to_string(), text(5));
        let function = crate::value::FunctionValue::new(
            "f".to_string(),
            vec!["a".to_string(), "bb".to_string()],
            crate::value::FunctionBody::Statements(std::sync::Arc::new(Vec::new())),
            std::sync::Arc::new(vec![captured]),
        );
        assert_eq!(
            value_bytes(&Value::Function(function)),
            "f".len() + "a".len() + "bb".len() + "x".len() + 5
        );
    }

    /// A function body held as statements is part of what the function value
    /// keeps alive, so its statements and the text they name are measured, not
    /// only the name, parameters and captured scopes.
    #[test]
    fn val_bytes_covers_a_statements_function_body() {
        let empty = function_with_body(FunctionBody::Statements(std::sync::Arc::new(Vec::new())));
        let body = vec![
            crate::parser::Stmt {
                span: crate::error::Span::new(1, 1),
                statement: crate::parser::Statement::Say(crate::parser::Expr::Text(
                    "hello".to_string(),
                )),
            },
            crate::parser::Stmt {
                span: crate::error::Span::new(2, 1),
                statement: crate::parser::Statement::Break,
            },
        ];
        let filled = function_with_body(FunctionBody::Statements(std::sync::Arc::new(body)));

        let empty_bytes = value_bytes(&empty);
        let filled_bytes = value_bytes(&filled);

        assert!(
            filled_bytes > empty_bytes,
            "a non-empty body adds weight: {filled_bytes} vs {empty_bytes}"
        );
        assert!(
            filled_bytes - empty_bytes >= "hello".len(),
            "the body's text is counted: {filled_bytes} vs {empty_bytes}"
        );
    }

    /// A function body held as a compiled block keeps the whole chunk alive, so
    /// the measurement counts the chunk's encoded bytes and the path that names
    /// the block rather than stopping at the function's name.
    #[test]
    fn val_bytes_covers_a_compiled_block_function_body() {
        let chunk =
            crate::Chunk::compile("to add(a, b)\n    give back a + b\nend\nsay add(1, 2)\n")
                .expect("the fixture compiles");
        let encoded = chunk.encode().len();
        let path = vec![0u32];

        let bytes = value_bytes(&function_with_body(FunctionBody::Block {
            chunk: std::sync::Arc::new(chunk),
            path: path.clone(),
        }));

        assert_eq!(
            bytes,
            "f".len() + "a".len() + path.len() * std::mem::size_of::<u32>() + encoded,
            "the compiled body's chunk and path are both counted"
        );
    }

    /// A deep acyclic value is measured without touching the process stack.
    /// The walk is iterative, so the text at the bottom is still counted.
    #[test]
    fn edge_value_bytes_measures_a_deep_value_without_recursing() {
        let mut value = Value::Text("x".to_string());
        for _ in 0..100_000 {
            value = Value::list(vec![value]);
        }

        assert_eq!(value_bytes(&value), 1);
        // Dropping the fixture recursively would be the stack overflow this
        // test is about; the measurement has already proved the walk is safe.
        std::mem::forget(value);
    }

    #[test]
    fn edge_memory_lines_report_every_zero_on_an_empty_session() {
        let lines = memory_lines(&[], 0, 0);
        let joined = lines.join("\n");

        assert!(joined.contains("bindings: 0 (0 B (0 B))"), "{}", joined);
        assert!(joined.contains("history: 0 entries"), "{}", joined);
        assert!(joined.contains("heap: 0 B (0 B)"), "{}", joined);
        assert!(joined.contains("(none)"), "{}", joined);
    }

    #[test]
    fn memory_lines_order_the_largest_first_and_stop_at_the_listing_limit() {
        let mut bindings: Vec<(String, usize)> = (0..MAX_LARGEST_BINDINGS + 5)
            .map(|index| (format!("name{index:02}"), index * 100))
            .collect();
        bindings.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));

        let lines = memory_lines(&bindings, 3, 42);
        let listing: Vec<&String> = lines
            .iter()
            .filter(|line| line.trim_start().starts_with("name"))
            .collect();

        assert_eq!(
            listing.len(),
            MAX_LARGEST_BINDINGS,
            "the listing is bounded: {:?}",
            lines
        );
        assert!(
            listing[0].contains("name14"),
            "the heaviest binding is named first: {:?}",
            listing
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("history: 3 entries (42 B (42 B))")),
            "history has its own count and bytes: {:?}",
            lines
        );
    }

    #[test]
    fn edge_profile_on_an_empty_block_still_names_a_zero_total() {
        let lines = profile_lines(&BlockProfile::default());
        let joined = lines.join("\n");

        assert!(joined.contains("total: 0.000 ns"), "{}", joined);
        assert!(
            joined.contains("0 line(s)"),
            "an empty block says it has no lines rather than printing nothing: {}",
            joined
        );
    }

    #[test]
    fn profile_lines_are_slowest_first_and_sum_to_the_total() {
        let block = BlockProfile {
            lines: vec![
                "for each i from 1 to 3".to_string(),
                "    set x to x + i".to_string(),
                "end".to_string(),
            ],
            timings: vec![(1, 150), (2, 700), (3, 50)],
        };

        let lines = profile_lines(&block);
        let rows: Vec<&String> = lines
            .iter()
            .filter(|line| line.trim_start().starts_with("line "))
            .collect();

        assert!(
            rows[0].trim_start().starts_with("line 2:"),
            "the slowest line is first: {:?}",
            rows
        );
        let total = lines
            .iter()
            .find(|line| line.trim_start().starts_with("total:"))
            .expect("a total is reported");
        assert!(
            total.contains("900.000 ns"),
            "the total is the sum of the parts: {}",
            total
        );
    }

    /// A timing whose line is past the quoted source is still reported and is
    /// still part of the total. Dropping it would make the parts disagree with
    /// the total the report promises adds up.
    #[test]
    fn edge_profile_lines_keep_a_timing_past_the_source() {
        let block = BlockProfile {
            lines: vec!["first".to_string(), "second".to_string()],
            timings: vec![(1, 100), (3, 900)],
        };

        let lines = profile_lines(&block);
        let joined = lines.join("\n");

        assert!(
            joined.contains("line 3: 900.000 ns"),
            "the out-of-range timing keeps its own row: {}",
            joined
        );
        assert!(
            joined.contains("total: 1000.000 ns"),
            "the total still includes every timing: {}",
            joined
        );
    }

    /// A profile figure is printed by integer division, so a timing past the
    /// `f64` mantissa's exact-integer range is still the quotient the integers
    /// gave rather than the nearest float.
    #[test]
    fn edge_profile_stays_exact_past_the_float_mantissa() {
        let nanos = (1u128 << 80) + 12_345;
        let block = BlockProfile {
            lines: vec!["slow".to_string()],
            timings: vec![(1, nanos)],
        };
        let unit = 1_000_000_000u128;
        let mut whole = nanos / unit;
        let mut fraction = ((nanos % unit) * 1000 + unit / 2) / unit;
        if fraction == 1000 {
            whole += 1;
            fraction = 0;
        }
        let expected = format!("{whole}.{fraction:03} s");

        let joined = profile_lines(&block).join("\n");

        assert!(
            joined.contains(&format!("total: {expected}")),
            "the total is the integer quotient: {}",
            joined
        );
        assert!(
            joined.contains(&format!("line 1: {expected}")),
            "the row is the same integer quotient: {}",
            joined
        );
    }

    /// The source map uses the lexer's line increments, including a newline
    /// inside a text literal, so every recorded span names the line it meant.
    #[test]
    fn edge_source_lines_match_the_lexers_line_numbers() {
        let source = "say \"first\nsecond\"\r\nset x to 1\n";
        let lines = source_lines(source);

        assert_eq!(
            lines,
            vec!["say \"first", "second\"", "set x to 1"],
            "the split is the lexer's, not a different idea of a line: {:?}",
            lines
        );

        let tokens = crate::lexer::Lexer::tokenize(source).expect("the source is a valid program");
        let set_line = tokens
            .iter()
            .find(|token| token.kind == crate::lexer::TokenKind::Set)
            .map(|token| token.line)
            .expect("the fixture has a set statement");
        assert_eq!(set_line, 3);
        assert_eq!(lines[set_line - 1], "set x to 1");
    }
}
