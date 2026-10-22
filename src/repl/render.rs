//! Three ways to print one value: an indented structure, JSON, and a table.
//!
//! The REPL used to have exactly one shape for every value it printed — the
//! single-line `Display` — so a two-hundred-element list, a nested record and
//! a list of records all came out as one unreadable line, and there was no way
//! to ask for JSON or for columns. Ipp ships exactly this trio (`.pretty`,
//! `.json`, `.table`) because looking at data is REPL-native work.
//!
//! None of the three is a second rendering of the value. They all reuse the
//! rendering the language already has — the JSON writer behind `json.stringify`
//! and the single-line `Display` — and only reflow it. So a value printed as
//! JSON here is the same document `json.stringify` writes, and a table cell
//! holds what `say` would have printed for that value.
//!
//! The three are pure functions over a [`Value`], so each can be tested
//! without a REPL, a terminal or a session.

use crate::value::Value;

/// How many spaces one level of nesting is indented by.
///
/// Two, because that is what every JSON pretty-printer in the world uses and
/// because a wider indent pushes the right-hand edge of a table off the screen
/// sooner.
const INDENT: usize = 2;

/// The deepest nesting [`pretty`] will follow.
///
/// The same budget the parser allows a source expression
/// ([`crate::parser::MAX_NESTING_DEPTH`]): a value deeper than that can only
/// have been built at runtime, by a function that returns a list holding a
/// list holding a list, and the printer refuses it with a message rather than
/// recursing further than the stack can be trusted to hold.
pub const MAX_PRETTY_DEPTH: usize = 64;

/// The value as JSON, on one line.
///
/// The language's own writer — the one behind `json.stringify` — so a document
/// printed here is the document a program would produce, escapes and all.
/// `nothing` is `null`, and a number that is not finite is `null` as well:
/// JSON has no literal for one.
pub fn json(value: &Value) -> String {
    crate::runtime::json_stringify(value)
}

/// The value as an indented structure — one level of indentation per level of
/// nesting.
///
/// The same document [`json`] writes, laid out over several lines, so it reads
/// back through `json.parse` as the value it came from.
pub fn pretty(value: &Value) -> Result<String, String> {
    let mut out = String::new();
    write_pretty(value, 0, &mut out)?;
    Ok(out)
}

/// Writes `value` into `out`, indented `depth` levels.
///
/// `Err` rather than a truncated document: a value nested past
/// [`MAX_PRETTY_DEPTH`] is refused outright, because half a structure is
/// worse than none — a reader cannot tell what was left out.
fn write_pretty(value: &Value, depth: usize, out: &mut String) -> Result<(), String> {
    if depth > MAX_PRETTY_DEPTH {
        return Err(format!(
            "Error: this value nests more than {MAX_PRETTY_DEPTH} levels deep, and there is \
             no way to show all of it."
        ));
    }

    match value {
        Value::List(items) if !items.is_empty() => {
            out.push_str("[\n");
            for (index, item) in items.iter().enumerate() {
                push_indent(depth + 1, out);
                write_pretty(item, depth + 1, out)?;
                if index + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            push_indent(depth, out);
            out.push(']');
        }
        Value::Record(fields) if !fields.is_empty() => {
            out.push_str("{\n");
            for (index, (key, field)) in fields.iter().enumerate() {
                push_indent(depth + 1, out);
                out.push_str(&crate::runtime::json_escape_text(key));
                out.push_str(": ");
                write_pretty(field, depth + 1, out)?;
                if index + 1 < fields.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            push_indent(depth, out);
            out.push('}');
        }
        // Everything else — a number, a text, a yes/no, `nothing`, an object, a
        // function — is a leaf, and a leaf is written by the language's own
        // JSON writer so that the escaping is the escaping `json.stringify`
        // uses.
        leaf => out.push_str(&crate::runtime::json_stringify(leaf)),
    }
    Ok(())
}

/// Pushes the indentation of `depth` levels.
fn push_indent(depth: usize, out: &mut String) {
    for _ in 0..depth * INDENT {
        out.push(' ');
    }
}

/// The list as a table: a header row naming the keys, and one row per record.
///
/// The keys are the union of the keys every record holds, in the order they
/// were first seen: a record that does not hold one of them leaves that cell
/// empty rather than failing, because a list of records that is not quite
/// uniform is still a list of records.
///
/// `Err` when the value is not a list of records — a table of what is not
/// tabular is not a table, and saying so is more useful than a header row
/// with no rows under it.
pub fn table(value: &Value) -> Result<String, String> {
    let items = match value {
        Value::List(items) => items,
        other => {
            return Err(format!(
                "Error: .table needs a list of records, but this is a {}.",
                other.type_name()
            ))
        }
    };

    if items.is_empty() {
        return Ok("The list is empty: no rows, and no keys to name.\n".to_string());
    }

    for (index, item) in items.iter().enumerate() {
        if !matches!(item, Value::Record(_)) {
            return Err(format!(
                "Error: .table needs a list of records, but item {} is a {}.",
                index + 1,
                item.type_name()
            ));
        }
    }

    let keys = keys_of(items);
    // The header row is spelled the way the cells are: a key may hold a line
    // break, and a header row that broke in two would be a header row the
    // reader could not read.
    let mut rows: Vec<Vec<String>> = vec![keys.iter().map(|key| one_line(key)).collect()];
    for item in items.iter() {
        let mut row = Vec::with_capacity(keys.len());
        for key in &keys {
            row.push(cell_of(item, key));
        }
        rows.push(row);
    }

    Ok(layout(&rows))
}

/// The keys every record in `items` holds, in the order they were first seen.
fn keys_of(items: &[Value]) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for item in items {
        if let Value::Record(fields) = item {
            for key in fields.keys() {
                if !keys.iter().any(|held| held == key) {
                    keys.push(key.clone());
                }
            }
        }
    }
    keys
}

/// The cell `key` has in `item`, or the empty string when `item` does not hold
/// it.
fn cell_of(item: &Value, key: &str) -> String {
    let Value::Record(fields) = item else {
        return String::new();
    };
    match fields.get(key) {
        Some(value) => one_line(&value.to_string()),
        None => String::new(),
    }
}

/// A cell that fits on one line.
///
/// A text may hold a newline, and a row that broke in two would be a row the
/// reader could not read: the line break is written as the two characters
/// `\n` instead, which is how the JSON writer spells it too.
fn one_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

/// Lays `rows` out in columns, each as wide as its widest cell.
///
/// The width of a cell is the width a terminal draws it — a wide character is
/// two cells, a combining mark is none — so a column of CJK text lines up
/// with a column of ASCII text instead of drifting one cell per character.
fn layout(rows: &[Vec<String>]) -> String {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![0usize; columns];
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            let width = crate::error::text_cells(cell);
            if width > widths[index] {
                widths[index] = width;
            }
        }
    }

    let mut out = String::new();
    for row in rows {
        let mut line = String::new();
        for (index, cell) in row.iter().enumerate() {
            if index > 0 {
                line.push_str("  ");
            }
            line.push_str(cell);
            // The last column is not padded: trailing spaces are noise, and a
            // row that ends in them is a row a reader cannot copy cleanly.
            if index + 1 < row.len() {
                for _ in 0..widths[index] - crate::error::text_cells(cell) {
                    line.push(' ');
                }
            }
        }
        // A row shorter than the header leaves the gap behind rather than
        // padding it out: the cell is empty, and an empty cell at the end of a
        // row is nothing to see.
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{Fields, Value};

    /// A record of `fields`, in the order they are written.
    fn record(fields: Vec<(&str, Value)>) -> Value {
        let mut map = Fields::new();
        for (key, value) in fields {
            map.insert(key.to_string(), value);
        }
        Value::record(map)
    }

    /// The lines of `text`, with the trailing newline removed.
    fn lines(text: &str) -> Vec<&str> {
        text.trim_end_matches('\n').split('\n').collect()
    }

    /// The value as an indented structure, one level per depth.
    #[test]
    fn pretty_indents_one_level_per_depth() {
        let value = record(vec![
            ("name", Value::Text("ada".to_string())),
            (
                "tags",
                Value::list(vec![
                    Value::Text("x".to_string()),
                    Value::Text("y".to_string()),
                ]),
            ),
            ("meta", record(vec![("ok", Value::YesNo(true))])),
        ]);

        let shown = pretty(&value).expect("a record is showable");

        assert_eq!(
            shown,
            "{\n  \"name\": \"ada\",\n  \"tags\": [\n    \"x\",\n    \"y\"\n  ],\n  \
             \"meta\": {\n    \"ok\": true\n  }\n}",
            "one level of indentation per level of nesting"
        );
    }

    /// The three shapes that have no inside: an empty list, an empty record and
    /// a leaf. None of them may open a bracket it never closes.
    #[test]
    fn pretty_of_an_empty_thing_is_closed_on_the_same_line() {
        assert_eq!(pretty(&Value::list(Vec::new())).unwrap(), "[]");
        assert_eq!(pretty(&record(Vec::new())).unwrap(), "{}");
        assert_eq!(pretty(&Value::Number(1.5)).unwrap(), "1.5");
        assert_eq!(pretty(&Value::Text("hi".to_string())).unwrap(), "\"hi\"");
        assert_eq!(pretty(&Value::Nothing).unwrap(), "null");
        assert_eq!(pretty(&Value::YesNo(false)).unwrap(), "false");
    }

    /// Ten levels deep, the printer still answers, every level is one level
    /// further in, and no line is longer than the structure it holds.
    #[test]
    fn edge_pretty_of_a_ten_deep_nesting_terminates_with_bounded_lines() {
        let mut value = Value::Number(1.0);
        for _ in 0..10 {
            value = Value::list(vec![value]);
        }

        let shown = pretty(&value).expect("ten levels is showable");
        let lines = lines(&shown);

        assert_eq!(
            lines.len(),
            21,
            "ten opening lines, ten values, one closing line"
        );
        assert!(
            lines[0].starts_with('[') && lines[20].trim() == "]",
            "the outermost bracket opens and closes: {:?}",
            lines
        );
        for (depth, line) in lines.iter().enumerate().skip(1).take(10) {
            let indent = line.len() - line.trim_start().len();
            assert_eq!(
                indent,
                depth * INDENT,
                "line {} is indented one level per depth: {:?}",
                depth,
                line
            );
        }
        for line in &lines {
            assert!(
                line.chars().count() <= 40,
                "no line of a ten-deep structure is wider than the structure: {:?}",
                line
            );
        }
    }

    /// A value nested past the printer's budget is refused, and refused as an
    /// error: half a structure is worse than none.
    #[test]
    fn edge_pretty_refuses_a_value_nested_past_its_budget() {
        let mut value = Value::Number(1.0);
        for _ in 0..=super::MAX_PRETTY_DEPTH {
            value = Value::list(vec![value]);
        }

        let refusal = pretty(&value).expect_err("a value past the budget is refused");
        assert!(
            refusal.starts_with("Error:"),
            "and it is an error, not a truncated document: {}",
            refusal
        );
        assert!(
            refusal.contains(&super::MAX_PRETTY_DEPTH.to_string()),
            "and it names the budget: {}",
            refusal
        );
    }

    /// The JSON is JSON: a reader that knows nothing about Redblue can read
    /// it, and it reads back as the value it came from.
    #[test]
    fn json_is_valid_json_per_serde_json() {
        let value = record(vec![
            ("name", Value::Text("ada".to_string())),
            ("count", Value::Number(3.0)),
            ("flag", Value::YesNo(true)),
            ("nothing", Value::Nothing),
            (
                "list",
                Value::list(vec![Value::Number(1.0), Value::Number(2.0)]),
            ),
        ]);

        let written = json(&value);
        let read: serde_json::Value =
            serde_json::from_str(&written).expect("the document is valid JSON");

        assert_eq!(read["name"], serde_json::json!("ada"));
        assert_eq!(read["count"], serde_json::json!(3));
        assert_eq!(read["flag"], serde_json::json!(true));
        assert_eq!(read["nothing"], serde_json::json!(null));
        assert_eq!(read["list"], serde_json::json!([1, 2]));
    }

    /// JSON has no literal for a number that is not finite. The writer answers
    /// `null`, as every JSON writer does, and never writes the word `NaN`.
    #[test]
    fn edge_json_of_a_number_that_is_not_finite_is_null_and_never_a_nan_literal() {
        for (value, what) in [
            (f64::NAN, "NaN"),
            (f64::INFINITY, "infinity"),
            (f64::NEG_INFINITY, "negative infinity"),
        ] {
            let written = json(&Value::Number(value));
            assert_eq!(
                written, "null",
                "{} must be written as null, not as a JSON number",
                what
            );
            assert!(
                !written.contains("NaN") && !written.contains("Infinity"),
                "{} must not appear in the document: {}",
                what,
                written
            );
            serde_json::from_str::<serde_json::Value>(&written)
                .expect("and the document is still valid JSON");
        }
    }

    /// A table names its keys once, in a header row, and lines the columns up
    /// under it.
    #[test]
    fn table_aligns_its_columns_under_one_header_row() {
        let rows = Value::list(vec![
            record(vec![
                ("name", Value::Text("ada".to_string())),
                ("age", Value::Number(36.0)),
            ]),
            record(vec![
                ("name", Value::Text("bartholomew".to_string())),
                ("age", Value::Number(4.0)),
            ]),
        ]);

        let shown = table(&rows).expect("a list of records is a table");
        let lines = lines(&shown);

        assert_eq!(lines.len(), 3, "a header row and one row per record");
        assert_eq!(lines[0], "name         age", "the keys are named once");
        assert_eq!(lines[1], "ada          36");
        assert_eq!(lines[2], "bartholomew  4");
    }

    /// A record that does not hold one of the keys leaves that cell empty
    /// rather than failing: a list of records that is not quite uniform is
    /// still a list of records.
    #[test]
    fn edge_table_of_ragged_rows_fills_the_missing_cells_with_nothing() {
        let rows = Value::list(vec![
            record(vec![
                ("name", Value::Text("ada".to_string())),
                ("age", Value::Number(36.0)),
            ]),
            record(vec![("name", Value::Text("bob".to_string()))]),
        ]);

        let shown = table(&rows).expect("a ragged list of records is still a table");
        let lines = lines(&shown);

        assert_eq!(
            lines[0], "name  age",
            "the keys are the union of the keys held"
        );
        assert_eq!(lines[1], "ada   36");
        assert_eq!(lines[2], "bob", "and the missing cell is empty");
    }

    /// A key held by a later record and not by an earlier one still gets a
    /// column, and the earlier row leaves it empty.
    #[test]
    fn edge_table_gives_a_later_key_its_own_column() {
        let rows = Value::list(vec![
            record(vec![("a", Value::Number(1.0))]),
            record(vec![("a", Value::Number(2.0)), ("b", Value::Number(3.0))]),
        ]);

        let shown = table(&rows).expect("a ragged list of records is still a table");
        let lines = lines(&shown);

        assert_eq!(lines[0], "a  b");
        assert_eq!(lines[1], "1");
        assert_eq!(lines[2], "2  3");
    }

    /// An empty list has no rows and no keys: it is named, and it is not a
    /// crash.
    #[test]
    fn edge_table_of_an_empty_list_says_so_and_does_not_panic() {
        let shown = table(&Value::list(Vec::new())).expect("an empty list is not an error");
        assert!(
            shown.to_lowercase().contains("empty"),
            "the empty list is named: {}",
            shown
        );
    }

    /// A table of what is not tabular is refused, and the refusal says what
    /// it got.
    #[test]
    fn edge_table_refuses_anything_that_is_not_a_list_of_records() {
        let refusal = table(&Value::Number(3.0)).expect_err("a number is not a table");
        assert!(
            refusal.contains("list") && refusal.contains("number"),
            "the refusal names what it wanted and what it got: {}",
            refusal
        );

        let refusal = table(&Value::list(vec![Value::Number(1.0)]))
            .expect_err("a list of numbers is not a table");
        assert!(
            refusal.contains("item 1"),
            "and it names the item that is not a record: {}",
            refusal
        );
    }

    /// A cell that holds a newline stays on one line: a row that broke in two
    /// would be a row the reader could not read.
    #[test]
    fn edge_a_cell_holding_a_newline_stays_on_one_line() {
        let rows = Value::list(vec![record(vec![(
            "text",
            Value::Text("one\ntwo".to_string()),
        )])]);

        let shown = table(&rows).expect("a record is a table");
        let lines = lines(&shown);

        assert_eq!(lines.len(), 2, "a header row and one row");
        assert_eq!(
            lines[1], "one\\ntwo",
            "the line break is spelled, not printed"
        );
    }

    /// A **key** that holds a newline does the same: the header row is a row
    /// like any other, and a header row that broke in two would be a header row
    /// the reader could not read.
    #[test]
    fn edge_a_key_holding_a_newline_stays_on_one_line() {
        let rows = Value::list(vec![record(vec![(
            "one\ntwo",
            Value::Text("a".to_string()),
        )])]);

        let shown = table(&rows).expect("a record is a table");
        let lines = lines(&shown);

        assert_eq!(lines.len(), 2, "a header row and one row");
        assert_eq!(
            lines[0], "one\\ntwo",
            "the line break in the key is spelled, not printed"
        );
        assert_eq!(lines[1], "a", "and the row under it is a row");
    }

    /// A column of wide characters is padded by the cells a terminal draws, so
    /// the columns still line up.
    ///
    /// A cell padded by its character count drifts one cell for every wide
    /// character in it: "日本" is two characters and four cells, so a column
    /// holding it and a column holding "ada" would not meet.
    #[test]
    fn edge_table_aligns_a_wide_character_column_by_the_cells_it_is_drawn_in() {
        let rows = Value::list(vec![
            record(vec![
                ("name", Value::Text("日本".to_string())),
                ("n", Value::Number(1.0)),
            ]),
            record(vec![
                ("name", Value::Text("ada".to_string())),
                ("n", Value::Number(2.0)),
            ]),
        ]);

        let shown = table(&rows).expect("a list of records is a table");
        let lines = lines(&shown);

        assert_eq!(lines[0], "name  n", "the header row");
        assert_eq!(
            lines[1], "日本  1",
            "the wide cell is padded by cells, not by characters"
        );
        assert_eq!(
            lines[2], "ada   2",
            "and the narrow cell is padded to the same width"
        );

        let column = |line: &str, cell: &str| {
            let at = line.find(cell).expect("the cell is in the line");
            crate::error::text_cells(&line[..at])
        };
        assert_eq!(
            column(lines[1], "1"),
            column(lines[2], "2"),
            "the second column starts at the same cell in both rows: {:?}",
            lines
        );
    }

    /// A text with a quote, a backslash and a newline in it is escaped the way
    /// the language escapes it, so the document is still one document.
    #[test]
    fn json_escapes_the_characters_a_text_may_hold() {
        let value = Value::Text("a \"b\" c\\d\ne".to_string());
        let written = json(&value);

        assert_eq!(written, "\"a \\\"b\\\" c\\\\d\\ne\"");
        let read: serde_json::Value =
            serde_json::from_str(&written).expect("the document is valid JSON");
        assert_eq!(read, serde_json::json!("a \"b\" c\\d\ne"));
    }

    /// A text written in a script, in a right-to-left script and in an emoji
    /// is written as it is: the escapes are the language's, not the writer's.
    #[test]
    fn edge_json_keeps_unicode_text_as_it_is() {
        for text in ["café", "日本語", "مرحبا", "🎉", "a\u{301}b"] {
            let written = json(&Value::Text(text.to_string()));
            let read: serde_json::Value =
                serde_json::from_str(&written).expect("the document is valid JSON");
            assert_eq!(
                read,
                serde_json::json!(text),
                "{} survives the round trip",
                text
            );
        }
    }
}
