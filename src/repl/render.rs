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
//! JSON here is the same document `json.stringify` writes, for every value
//! JSON can hold, and a table cell holds what `say` would have printed for
//! that value, escaped so that a terminal cannot be mangled by it.
//!
//! One difference is worth naming up front: **`.json` is a document, and
//! `.pretty` and `.table` are renderings.** All three spell out the characters
//! a terminal acts on — the escape byte, the C1 controls, the bidi and
//! zero-width marks — rather than writing them out. `.json` uses the language's
//! own escaper, so it is the document `json.stringify` writes, byte for byte,
//! and a program can reproduce what the reader is looking at. `.pretty` keeps
//! its escape in JSON's own shape too, so the document it writes still reads
//! back through `json.parse` as the value it came from; `.table` uses its own
//! spelling, because a cell is what `say` would have printed and not a
//! document.
//!
//! Two things are this module's own, and both of them are refusals rather
//! than renderings. A value nested past [`MAX_RENDER_DEPTH`] is refused by
//! every renderer, the same way, because a printer that follows a value a
//! hundred thousand levels deep is a hundred thousand stack frames — and a
//! stack overflow in Rust is a fatal abort, not a catchable panic, so the
//! session dies instead of answering. And a value JSON has no literal for — a
//! function, a builtin, an object — is refused by `.json` rather than written
//! as the `null` that means "nothing"; `.pretty` shows those three the way
//! the language writes them.
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

/// The deepest nesting any of the three renderers will follow.
///
/// The same budget the parser allows a source expression
/// ([`crate::parser::MAX_NESTING_DEPTH`]): a value deeper than that can only
/// have been built at runtime, by a function that returns a list holding a
/// list holding a list, and the printer refuses it with a message rather than
/// recursing further than the stack can be trusted to hold.
///
/// One budget for all three, because a renderer that shows a value another one
/// cannot is a renderer that kills the process: a stack overflow in Rust is a
/// fatal abort, not a catchable panic, so the session dies instead of
/// answering.
pub const MAX_RENDER_DEPTH: usize = 64;

/// The value as JSON, on one line.
///
/// The language's own writer — the one behind `json.stringify` — so a document
/// printed here is the document a program would produce, escapes and all, for
/// every value JSON can hold. `nothing` is `null`, and a number that is not
/// finite is `null` as well: JSON has no literal for one.
///
/// **A document first and a terminal rendering second.** The document is the
/// language's own, byte for byte, so a program can reproduce what the reader
/// is looking at. That is why the language's escaper spells out the characters
/// a terminal acts on: a right-to-left override is written as a JSON escape,
/// so it cannot reorder the line the document is printed on, and `json.parse`
/// still reads the character back. [`.pretty`](pretty) and [`.table`](table)
/// spell the same characters out in their own shapes.
///
/// `Err` in two cases, both of them a refusal to write a lie:
///
/// - a value nested past [`MAX_RENDER_DEPTH`], which no document can hold;
/// - a function, a builtin or an object, for which JSON has no literal.
///   The language's writer refuses those three as well, because `null` says
///   "nothing" — about a value that is not nothing.
pub fn json(value: &Value) -> Result<String, String> {
    let mut out = String::new();
    write_json(value, 0, &mut out)?;
    Ok(out)
}

/// Writes `value` into `out` as JSON, on one line.
///
/// The same document [`crate::runtime::json_stringify`] writes, and the same
/// way: a list as `[a, b]`, a record as `{"a": 1, "b": 2}`, a text escaped by
/// the language's own escaper. Only the depth budget and the refusal for a
/// value JSON cannot hold are this module's own.
fn write_json(value: &Value, depth: usize, out: &mut String) -> Result<(), String> {
    if depth > MAX_RENDER_DEPTH {
        return Err(too_deep());
    }

    match value {
        Value::List(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write_json(item, depth + 1, out)?;
            }
            out.push(']');
        }
        Value::Record(fields) => {
            out.push('{');
            for (index, (key, field)) in fields.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                out.push_str(&crate::runtime::json_escape_text(key));
                out.push_str(": ");
                write_json(field, depth + 1, out)?;
            }
            out.push('}');
        }
        // JSON has no literal for any of these three, and `null` is a lie
        // about them: `null` is what `nothing` is, and none of them is
        // nothing. So the document is refused rather than written wrong.
        Value::Function(_) | Value::Builtin(_) | Value::Object(_, _) => {
            return Err(format!(
                "Error: JSON has no literal for a value of the kind `{}`, and `null` would \
                 say 'nothing' about a value that is not nothing.",
                value.type_name()
            ));
        }
        // Everything else — a number, a text, a yes/no, `nothing` — is a leaf,
        // and a leaf is written by the language's own JSON writer so that the
        // escaping is the escaping `json.stringify` uses.
        leaf => out.push_str(&crate::runtime::json_stringify(leaf)?),
    }
    Ok(())
}

/// The refusal for a value nested past the renderers' budget.
///
/// One message for all three renderers, because the situation is one
/// situation: half a structure is worse than none.
fn too_deep() -> String {
    format!(
        "Error: this value nests more than {MAX_RENDER_DEPTH} levels deep, and there is \
         no way to show all of it."
    )
}

/// The value as an indented structure — one level of indentation per level of
/// nesting.
///
/// The same document [`json`] writes, laid out over several lines, so it reads
/// back through `json.parse` as the value it came from — except for the three
/// values JSON cannot hold, which are written the way the language writes
/// them (`<function f>`) rather than as the `null` JSON would give them.
///
/// The one difference from [`json`] is the characters a terminal acts on: a
/// text holding a right-to-left override or a zero-width mark has it spelled
/// out, in JSON's own `\uXXXX` escape, so the document is still a document and
/// the terminal is not reordered. `.json` is the language's own writer's
/// document and changes nothing in it; this is a rendering, and a rendering is
/// what the reader looks at.
pub fn pretty(value: &Value) -> Result<String, String> {
    let mut out = String::new();
    write_pretty(value, 0, &mut out)?;
    Ok(out)
}

/// Writes `value` into `out`, indented `depth` levels.
///
/// `Err` rather than a truncated document: a value nested past
/// [`MAX_RENDER_DEPTH`] is refused outright, because half a structure is
/// worse than none — a reader cannot tell what was left out.
fn write_pretty(value: &Value, depth: usize, out: &mut String) -> Result<(), String> {
    if depth > MAX_RENDER_DEPTH {
        return Err(too_deep());
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
                out.push_str(&json_text(key));
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
        // A function, a builtin and an object have no JSON form, so they are
        // written the way the language writes them — `<function f>` — and not
        // as the `null` the JSON writer would give them. `null` is what
        // `nothing` is, and a function is not nothing: a display command that
        // answers "null" about a function is telling the reader a lie.
        Value::Function(_) | Value::Builtin(_) | Value::Object(_, _) => {
            out.push_str(&value.to_string());
        }
        // A text is written by the language's own JSON writer, with the
        // characters a terminal acts on spelled out: the writer escapes what
        // JSON must escape, and a right-to-left override or a zero-width mark
        // is not one of them — a terminal acts on both of them, so a document
        // that held one raw would reorder the line it was drawn on.
        Value::Text(text) => out.push_str(&json_text(text)),
        // Everything else — a number, a yes/no, `nothing` — is a leaf, and a
        // leaf is written by the language's own JSON writer so that the
        // escaping is the escaping `json.stringify` uses.
        leaf => out.push_str(&crate::runtime::json_stringify(leaf)?),
    }
    Ok(())
}

/// The text as a JSON string, with the characters a terminal acts on spelled
/// out.
///
/// The language's own writer escapes what JSON must escape — the quote, the
/// backslash, the line break — and the characters a terminal acts on: the C1
/// controls, and the bidi and zero-width marks. A right-to-left override
/// reorders the line it is on, and a zero-width mark takes no room at all
/// while still changing what is around it, so neither is written raw.
///
/// The escape is JSON's own — `\uXXXX` — and not the table's `\u{XXXX}`: the
/// document is still a document, so `json.parse` reads the character back and
/// the value the document came from is the value that comes back.
///
/// `.json` uses the same escape because it is the document `json.stringify`
/// writes, byte for byte. `.pretty` is a rendering too, and it asks the same
/// escaper, so the two documents and the language's own writer cannot drift.
fn json_text(text: &str) -> String {
    let escaped = crate::runtime::json_escape_text(text);
    let mut out = String::with_capacity(escaped.len());
    for c in escaped.chars() {
        if crate::runtime::mangles_terminal(c) {
            out.push_str(&format!("\\u{:04x}", c as u32));
        } else {
            out.push(c);
        }
    }
    out
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
/// with no rows under it — and when a cell holds a value nested past
/// [`MAX_RENDER_DEPTH`], which no row can be drawn around.
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
    if keys.is_empty() {
        // The list is not empty, but there is nothing to name: every record in
        // it holds no key at all. An empty table would say nothing at all, and
        // a reader cannot tell an empty table from a failure — so the case is
        // named the way the empty list is named.
        return Ok("The records hold no keys: there is nothing to name.\n".to_string());
    }
    // The header row is spelled the way the cells are: a key may hold a line
    // break, and a header row that broke in two would be a header row the
    // reader could not read.
    let mut rows: Vec<Vec<String>> = vec![keys.iter().map(|key| one_line(key)).collect()];
    for item in items.iter() {
        let mut row = Vec::with_capacity(keys.len());
        for key in &keys {
            row.push(cell_of(item, key)?);
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
fn cell_of(item: &Value, key: &str) -> Result<String, String> {
    let Value::Record(fields) = item else {
        return Ok(String::new());
    };
    match fields.get(key) {
        Some(value) => Ok(one_line(&single_line(value)?)),
        None => Ok(String::new()),
    }
}

/// The value on one line, the way the language writes it.
///
/// The same shape [`crate::value::Value`]'s own `Display` writes — a list as
/// `[a, b]`, a record as `{a: 1}`, a function as `<function f>` — with the
/// renderers' depth budget on it.
///
/// The budget is the whole point: the `Display` this replaces follows the
/// value as deep as it goes, and a value built at runtime a hundred thousand
/// levels deep is walked by a hundred thousand stack frames, which in Rust is
/// a fatal abort rather than a catchable panic. A cell is a cell: a table
/// that cannot be drawn is a message, not a dead session.
fn single_line(value: &Value) -> Result<String, String> {
    single_line_at(value, 0)
}

/// The value on one line, `depth` levels down.
fn single_line_at(value: &Value, depth: usize) -> Result<String, String> {
    if depth > MAX_RENDER_DEPTH {
        return Err(too_deep());
    }

    match value {
        Value::List(items) => {
            let mut parts = Vec::with_capacity(items.len());
            for item in items.iter() {
                parts.push(single_line_at(item, depth + 1)?);
            }
            Ok(format!("[{}]", parts.join(", ")))
        }
        Value::Record(fields) => {
            let mut parts = Vec::with_capacity(fields.len());
            for (key, field) in fields.iter() {
                parts.push(format!("{}: {}", key, single_line_at(field, depth + 1)?));
            }
            Ok(format!("{{{}}}", parts.join(", ")))
        }
        // A leaf is written by the language's own `Display`, so a cell holds
        // exactly what `say` would have printed for that value.
        leaf => Ok(leaf.to_string()),
    }
}

/// A cell that fits on one line.
///
/// A text may hold a newline, and a row that broke in two would be a row the
/// reader could not read: the line break is written as the two characters
/// `\n` instead, which is how the JSON writer spells it too.
///
/// And a text may hold anything else a terminal would act on — an escape
/// sequence that colours the rest of the row, a right-to-left override that
/// reorders it — so those characters are spelled out rather than written out:
/// what a terminal would mangle is written as the name of the character
/// instead of the character. The escaped form is also what the column is
/// padded by, so the padding counts the cells the terminal draws and not the
/// cells the escape sequence would have swallowed.
///
/// Only the characters a terminal acts on are escaped. A quote, an
/// apostrophe, a backslash, a combining mark and a letter of any script are
/// drawn as they are, and a cell that escaped them would show the reader a
/// character the value does not hold: the cell is what `say` would have
/// printed, and the only thing it may change is what the terminal would do
/// with it.
fn one_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ if crate::runtime::mangles_terminal(c) => {
                out.push_str(&format!("\\u{{{:x}}}", c as u32));
            }
            _ => out.push(c),
        }
    }
    out
}

/// How many terminal cells `text` is drawn in.
///
/// The width of a whole text as a terminal draws it: a wide character is two
/// cells, a combining mark is none, and an emoji that is one glyph — a face
/// with a skin tone, a heart with a variation selector — is the width of
/// that one glyph rather than the sum of its characters.
///
/// [`unicode_width`] is the reference for that. The crate's own table of wide
/// ranges belongs to the error renderer, and it counts characters one at a
/// time, so an emoji it does not know is wide is one cell to it and two to
/// the reader: a column padded by it drifts a cell for every emoji in it, and
/// the column beside it starts further along in that row than in the others.
fn cell_width(text: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(text)
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
            let width = cell_width(cell);
            if width > widths[index] {
                widths[index] = width;
            }
        }
    }

    let mut out = String::new();
    for row in rows {
        let mut line = String::new();
        // Where the content of the row ends: the byte after the last cell
        // that holds something. Everything past it is the layout's own.
        let mut content_end = 0;
        for (index, cell) in row.iter().enumerate() {
            if index > 0 {
                line.push_str("  ");
            }
            line.push_str(cell);
            if !cell.is_empty() {
                content_end = line.len();
            }
            // The last column is not padded: the padding is the layout's own,
            // and the layout's own padding belongs between two columns, not
            // after the last one.
            if index + 1 < row.len() {
                for _ in 0..widths[index] - cell_width(cell) {
                    line.push(' ');
                }
            }
        }
        // What is dropped is the padding the layout added and the content
        // never brought: a row whose last cell is empty ends in the column
        // separator and the padding before it. What is kept is the content's
        // own trailing spaces, in the last column as much as in any other —
        // the same value must not be printed two different ways depending on
        // where it sat.
        out.push_str(without_layout_padding(&line, content_end));
        out.push('\n');
    }
    out
}

/// The line without the padding the layout added at its end.
///
/// `content_end` is where the content of the row ends: the byte after the
/// last cell that holds something. Everything past it is the layout's own —
/// the padding of the last column that holds a cell, and the separator that
/// follows it — and the layout's own whitespace is the layout's to drop.
///
/// The content's own trailing spaces are content, and they are kept wherever
/// they sit: in the last column as much as in any other, and in a column
/// that is followed by an empty cell as much as in one that is not. A row
/// whose last cell is empty ends in the separator and the padding before it,
/// and dropping that must not take the previous column's own trailing spaces
/// with it: the same value must not be printed two different ways depending
/// on where it sat.
fn without_layout_padding(line: &str, content_end: usize) -> &str {
    &line[..content_end]
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

    /// A function named `name`, with no body worth calling.
    fn function(name: &str) -> Value {
        Value::Function(crate::value::FunctionValue {
            name: name.to_string(),
            params: Vec::new(),
            body: crate::value::FunctionBody::Statements(std::sync::Arc::new(Vec::new())),
            captured: std::sync::Arc::new(Vec::new()),
        })
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
        for _ in 0..=super::MAX_RENDER_DEPTH {
            value = Value::list(vec![value]);
        }

        let refusal = pretty(&value).expect_err("a value past the budget is refused");
        assert!(
            refusal.starts_with("Error:"),
            "and it is an error, not a truncated document: {}",
            refusal
        );
        assert!(
            refusal.contains(&super::MAX_RENDER_DEPTH.to_string()),
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

        let written = json(&value).expect("a record is writable");
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
            let written = json(&Value::Number(value)).expect("a number is writable");
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
    ///
    /// The columns line up because the widest cell sets the width of its
    /// column — "bartholomew" is eleven cells wide, so the header's "name" is
    /// padded to eleven and the second column starts at the same cell in every
    /// row. The assertion below is about that: the strings are pinned, and so
    /// is the cell the second column starts at, because a table whose rows
    /// disagree about where a column starts is a table that does not line up.
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

        let column = |line: &str, cell: &str| {
            let at = line.find(cell).expect("the cell is in the line");
            cell_width(&line[..at])
        };
        assert_eq!(
            column(lines[0], "age"),
            column(lines[1], "36"),
            "the second column starts at the same cell in the header and the first row: {:?}",
            lines
        );
        assert_eq!(
            column(lines[1], "36"),
            column(lines[2], "4"),
            "and in the second row: {:?}",
            lines
        );
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

    /// A list of records that hold no keys at all is named too.
    ///
    /// The empty list is named because a reader cannot tell an empty table
    /// from a failure, and the same is true of a list whose every record is
    /// empty: there are rows, and there is nothing to name them by. A table
    /// with no columns is two blank lines and says nothing at all, so the case
    /// is named the way the empty list is named.
    #[test]
    fn edge_a_table_of_records_with_no_keys_says_so() {
        let rows = Value::list(vec![record(Vec::new()), record(Vec::new())]);

        let shown = table(&rows).expect("a list of records with no keys is not an error");
        assert!(
            shown.to_lowercase().contains("no keys"),
            "the records holding no keys are named: {:?}",
            shown
        );
        assert!(
            !shown.trim().is_empty(),
            "and the answer is not silence: {:?}",
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
            cell_width(&line[..at])
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
        let written = json(&value).expect("a text is writable");

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
            let written = json(&Value::Text(text.to_string())).expect("a text is writable");
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

    /// A function is not `nothing`, so no renderer may answer `null` about one.
    ///
    /// `null` is what `nothing` is written as, and the JSON writer has no
    /// literal for a function — so the writer writes `null` for it. A *display*
    /// command that answers `null` about a function is telling the reader that
    /// the value is nothing, which is a lie: `.type` answers `kind: function`
    /// about the very same value.
    #[test]
    fn edge_pretty_of_a_function_is_not_null() {
        let shown = pretty(&function("f")).expect("a function is showable");

        assert_eq!(
            shown, "<function f>",
            "a function is written the way the language writes it"
        );
        assert_ne!(shown, "null", "and never as the `null` that means nothing");
    }

    /// The same leaf in the JSON renderer: JSON has no literal for a function,
    /// so the document is refused with a reason rather than written with a
    /// `null` in it.
    #[test]
    fn edge_json_of_a_function_is_refused_and_not_null() {
        let refusal = json(&function("f")).expect_err("JSON cannot hold a function");

        assert!(
            refusal.starts_with("Error:"),
            "and it is an error, not a document: {}",
            refusal
        );
        assert!(
            refusal.contains("function"),
            "and it names what it cannot write: {}",
            refusal
        );
        assert!(
            !refusal.trim_end().ends_with("null"),
            "and it does not answer `null`: {}",
            refusal
        );
    }

    /// A builtin and an object are refused the same way, and named for what
    /// they are.
    #[test]
    fn edge_json_of_a_builtin_or_an_object_is_refused_and_not_null() {
        let builtin =
            json(&Value::Builtin("uppercase".to_string())).expect_err("JSON cannot hold a builtin");
        assert!(
            builtin.contains("builtin"),
            "a builtin is named for what it is: {}",
            builtin
        );

        let object = json(&Value::object("Counter".to_string(), Fields::new()))
            .expect_err("JSON cannot hold an object");
        assert!(
            object.contains("object"),
            "an object is named for what it is: {}",
            object
        );
    }

    /// A cell holding a function holds the function's own form: the cell is
    /// what `say` would have printed, and `say` does not print `null`.
    #[test]
    fn edge_a_table_cell_holding_a_function_is_not_null() {
        let rows = Value::list(vec![record(vec![("f", function("f"))])]);

        let shown = table(&rows).expect("a record is a table");
        let lines = lines(&shown);

        assert_eq!(
            lines[1], "<function f>",
            "the cell holds the function's own form"
        );
        assert!(
            !shown.contains("null"),
            "and never the `null` that means nothing: {}",
            shown
        );
    }

    /// A value nested past the renderers' budget is refused by every one of
    /// them, the same way.
    ///
    /// The budget is what keeps a value built at runtime from being written by
    /// a recursion the stack cannot hold: a stack overflow in Rust is a fatal
    /// abort, not a catchable panic, so an unbounded renderer does not fail —
    /// it takes the session down with it. `.pretty` had the budget; `.json`
    /// and `.table` had none, so the same value was refused cleanly by one
    /// command and killed the process by another.
    #[test]
    fn edge_json_of_a_value_nested_past_its_budget_is_refused() {
        let mut value = Value::Number(1.0);
        for _ in 0..=super::MAX_RENDER_DEPTH {
            value = Value::list(vec![value]);
        }

        let refusal = json(&value).expect_err("a value past the budget is refused");
        assert!(
            refusal.starts_with("Error:"),
            "and it is an error, not a truncated document: {}",
            refusal
        );
        assert!(
            refusal.contains(&super::MAX_RENDER_DEPTH.to_string()),
            "and it names the budget: {}",
            refusal
        );
    }

    /// The same budget for the table, whose cells hold the value they were
    /// given.
    #[test]
    fn edge_table_of_a_value_nested_past_its_budget_is_refused() {
        let mut value = Value::Number(1.0);
        for _ in 0..=super::MAX_RENDER_DEPTH {
            value = Value::list(vec![value]);
        }
        let rows = Value::list(vec![record(vec![("deep", value)])]);

        let refusal = table(&rows).expect_err("a value past the budget is refused");
        assert!(
            refusal.starts_with("Error:"),
            "and it is an error, not a truncated cell: {}",
            refusal
        );
        assert!(
            refusal.contains(&super::MAX_RENDER_DEPTH.to_string()),
            "and it names the budget: {}",
            refusal
        );
    }

    /// A value just inside the budget is still written, by every renderer: the
    /// budget is a refusal, not a truncation.
    #[test]
    fn edge_a_value_just_inside_the_budget_is_still_written() {
        let mut value = Value::Number(1.0);
        for _ in 0..super::MAX_RENDER_DEPTH {
            value = Value::list(vec![value]);
        }

        let written = json(&value).expect("a value inside the budget is writable");
        assert!(
            written.starts_with('[') && written.ends_with(']'),
            "the document is whole: {}",
            written
        );

        let rows = Value::list(vec![record(vec![("deep", value)])]);
        let tabled = table(&rows).expect("a value inside the budget is tableable");
        assert!(
            tabled.contains("1"),
            "and the cell holds the value: {}",
            tabled
        );
    }

    /// A cell holding something a terminal would act on is escaped rather than
    /// written out: an escape sequence that colours the rest of the row, a
    /// right-to-left override that reorders it.
    ///
    /// The cell is also padded by what the terminal draws, so the column stays
    /// aligned: an escape sequence the terminal swallows is zero cells, and
    /// the escaped form the reader sees is what is counted.
    ///
    /// Both halves are in the fixture — an escape byte *and* a right-to-left
    /// override — because an assertion about a character the fixture does not
    /// hold cannot fail, whatever the renderer does with it.
    #[test]
    fn edge_a_cell_a_terminal_would_mangle_is_escaped_rather_than_written_out() {
        let rows = Value::list(vec![
            record(vec![("name", Value::Text("\u{1b}[31mred".to_string()))]),
            record(vec![("name", Value::Text("a\u{202e}b".to_string()))]),
            record(vec![("name", Value::Text("ada".to_string()))]),
        ]);

        let shown = table(&rows).expect("a record is a table");
        let lines = lines(&shown);

        assert!(
            !shown.contains('\u{1b}'),
            "no raw escape sequence reaches the terminal: {:?}",
            shown
        );
        assert!(
            lines[1].contains("\\u{1b}"),
            "the escape is spelled out instead: {:?}",
            lines[1]
        );
        assert!(
            !shown.contains('\u{202E}'),
            "and no right-to-left override either: {:?}",
            shown
        );
        assert!(
            lines[2].contains("\\u{202e}"),
            "the right-to-left override is spelled out as well: {:?}",
            lines[2]
        );

        // The column is padded by the cells the terminal draws, so the second
        // row's second column starts where the first row's does.
        let column = |line: &str, cell: &str| {
            let at = line.find(cell).expect("the cell is in the line");
            cell_width(&line[..at])
        };
        assert_eq!(
            column(lines[1], "\\u{1b}[31mred"),
            column(lines[3], "ada"),
            "the columns line up: {:?}",
            lines
        );
    }

    /// A cell shows the characters a terminal draws as they are.
    ///
    /// Only the characters a terminal acts on are spelled out: a quote, an
    /// apostrophe and a combining mark are drawn by every terminal there is,
    /// so a cell that spelled them out would show the reader a character the
    /// value does not hold. The cell is what `say` would have printed, and
    /// the only thing it may change is what the terminal would do with it.
    #[test]
    fn edge_a_cell_shows_the_characters_a_terminal_draws_as_they_are() {
        let rows = Value::list(vec![record(vec![(
            "text",
            Value::Text("He said \"hi\" — it's a\u{301}b".to_string()),
        )])]);

        let shown = table(&rows).expect("a record is a table");
        let lines = lines(&shown);

        assert_eq!(
            lines[1], "He said \"hi\" — it's a\u{301}b",
            "the cell holds the text as the value holds it: {:?}",
            lines
        );
    }

    /// A rendering is drawn on a terminal, so the characters a terminal acts
    /// on are spelled out rather than written out.
    ///
    /// `.table` spells them out in its own shape (`\u{202e}`); `.pretty`
    /// spells them out in JSON's own (`\u202e`), because `.pretty` writes a
    /// document and a document has to stay a document: `json.parse` reads the
    /// character back, so the value the document came from is the value that
    /// comes back.
    ///
    /// The three families are all in the fixture — a bidi override, a
    /// zero-width mark and a C1 control — because an assertion about a
    /// character the fixture does not hold cannot fail, whatever the renderer
    /// does with it.
    #[test]
    fn edge_pretty_spells_out_the_characters_a_terminal_would_mangle() {
        let value = record(vec![(
            "key\u{202e}",
            Value::Text("text\u{200b}with\u{85}marks".to_string()),
        )]);

        let shown = pretty(&value).expect("a record is showable");

        for raw in ['\u{202e}', '\u{200b}', '\u{85}'] {
            assert!(
                !shown.contains(raw),
                "no character a terminal acts on reaches the terminal raw: {:?}",
                shown
            );
        }
        assert!(
            shown.contains("\\u202e"),
            "the key's right-to-left override is spelled out: {:?}",
            shown
        );
        assert!(
            shown.contains("\\u200b"),
            "and the text's zero-width mark is: {:?}",
            shown
        );
        assert!(
            shown.contains("\\u0085"),
            "and so is the C1 control: {:?}",
            shown
        );

        let read: serde_json::Value =
            serde_json::from_str(&shown).expect("the document is still valid JSON");
        assert_eq!(
            read,
            serde_json::json!({ "key\u{202e}": "text\u{200b}with\u{85}marks" }),
            "and the document still reads back as the value it came from: {:?}",
            shown
        );
    }

    /// `.json` is the document the language's own writer writes, byte for byte.
    ///
    /// The identity is the whole point of the command: a reader who copies the
    /// document into a program gets the same document back. So it is checked
    /// here, against the language's own writer, instead of being taken on
    /// trust — and the characters a terminal acts on are left as they are,
    /// because a document that differed from the one `json.stringify` writes
    /// would be a document no program could produce.
    #[test]
    fn edge_the_json_document_is_the_language_s_own_writer_s_document() {
        let value = record(vec![
            ("name", Value::Text("ada".to_string())),
            ("count", Value::Number(3.0)),
            ("flag", Value::YesNo(true)),
            ("missing", Value::Nothing),
            (
                "list",
                Value::list(vec![Value::Number(1.0), Value::Text("x".to_string())]),
            ),
            ("nested", record(vec![("deep", Value::YesNo(false))])),
        ]);

        let written = json(&value).expect("a record is writable");

        assert_eq!(
            written,
            crate::runtime::json_stringify(&value).expect("a record is writable"),
            "the document is the one the language's own writer writes"
        );
    }

    /// A cell that ends in spaces keeps them, in the last column as much as in
    /// any other.
    ///
    /// The layout's own padding is the layout's to drop; the content's own
    /// trailing spaces are the content's to keep. A table that drops them in
    /// the last column only prints the same value two different ways
    /// depending on where it sat.
    ///
    /// And the same is true of a cell that is *followed* by an empty one: the
    /// layout's separator and padding come after it, and dropping them must
    /// not take the content's own trailing spaces with them.
    #[test]
    fn edge_a_cell_ending_in_spaces_keeps_them_in_every_column() {
        let rows = Value::list(vec![
            record(vec![
                ("name", Value::Text("ada ".to_string())),
                ("note", Value::Text("keep ".to_string())),
            ]),
            record(vec![
                ("name", Value::Text("bo".to_string())),
                ("note", Value::Text("x".to_string())),
            ]),
        ]);

        let shown = table(&rows).expect("a list of records is a table");
        let lines = lines(&shown);

        assert_eq!(
            lines[1], "ada   keep ",
            "the spaces of the last cell are kept: {:?}",
            lines
        );
        assert!(
            lines[1].ends_with(' '),
            "and the row ends in the content's own space: {:?}",
            lines[1]
        );
        assert_eq!(
            lines[2], "bo    x",
            "and a row whose last cell is empty is not padded out: {:?}",
            lines
        );
    }

    /// The same value in the same column is printed the same way whether the
    /// cell after it holds something or nothing.
    ///
    /// The row's own trailing spaces are content, and the layout's separator
    /// and padding after them are the layout's. A row whose last cell is empty
    /// used to lose the content's own trailing spaces with them, so `ada ` was
    /// printed as `ada` in one row and as `ada ` in the next — the same value,
    /// printed two ways, depending on where it sat.
    #[test]
    fn edge_a_cell_ending_in_spaces_before_an_empty_cell_keeps_them_too() {
        let rows = Value::list(vec![
            record(vec![
                ("name", Value::Text("ada ".to_string())),
                ("note", Value::Text("x".to_string())),
            ]),
            record(vec![("name", Value::Text("ada ".to_string()))]),
        ]);

        let shown = table(&rows).expect("a list of records is a table");
        let lines = lines(&shown);

        assert_eq!(
            lines[1], "ada   x",
            "the row whose last cell holds something: {:?}",
            lines
        );
        assert_eq!(
            lines[2], "ada ",
            "and the row whose last cell is empty keeps the same spaces: {:?}",
            lines
        );
        assert!(
            lines[2].ends_with(' '),
            "the content's own space is still there: {:?}",
            lines[2]
        );
        assert!(
            !lines[2].ends_with("   "),
            "and the layout's own separator is not: {:?}",
            lines[2]
        );
    }

    /// A column holding an emoji is padded by the cells the emoji is drawn in,
    /// so the columns still line up.
    ///
    /// An emoji is one character and two cells, so a column padded by
    /// characters drifts one cell for every emoji in it: the second column
    /// starts one cell further along in that row than in the others, and the
    /// reader sees a table that does not line up.
    #[test]
    fn edge_a_table_column_holding_an_emoji_is_padded_by_the_cells_it_is_drawn_in() {
        let rows = Value::list(vec![
            record(vec![
                ("name", Value::Text("\u{2b50}".to_string())),
                ("n", Value::Number(1.0)),
            ]),
            record(vec![
                ("name", Value::Text("ada".to_string())),
                ("n", Value::Number(2.0)),
            ]),
        ]);

        let shown = table(&rows).expect("a list of records is a table");
        let lines = lines(&shown);

        assert_eq!(
            lines[1], "\u{2b50}    1",
            "the emoji is drawn in two cells, so it is padded by two: {:?}",
            lines
        );
        assert_eq!(
            lines[2], "ada   2",
            "and the narrow cell is padded to the same width: {:?}",
            lines
        );

        let column = |line: &str, cell: &str| {
            let at = line.find(cell).expect("the cell is in the line");
            cell_width(&line[..at])
        };
        assert_eq!(
            column(lines[1], "1"),
            column(lines[2], "2"),
            "the second column starts at the same cell in both rows: {:?}",
            lines
        );
    }

    /// An emoji that is one glyph is padded by the width of that one glyph.
    ///
    /// A face with a skin tone and a heart with a variation selector are two
    /// characters each, and a terminal draws each of them as one glyph two
    /// cells wide. A column padded by characters counts them as four cells and
    /// as two, so the column drifts by two cells for every one of them.
    #[test]
    fn edge_a_table_column_holding_an_emoji_glyph_is_padded_by_the_glyph_it_is_drawn_in() {
        for (what, glyph) in [
            ("a face with a skin tone", "\u{1f44d}\u{1f3fb}"),
            ("a heart with a variation selector", "\u{2764}\u{fe0f}"),
        ] {
            let rows = Value::list(vec![
                record(vec![
                    ("name", Value::Text(glyph.to_string())),
                    ("n", Value::Number(1.0)),
                ]),
                record(vec![
                    ("name", Value::Text("ada".to_string())),
                    ("n", Value::Number(2.0)),
                ]),
            ]);

            let shown = table(&rows).expect("a list of records is a table");
            let lines = lines(&shown);

            let column = |line: &str, cell: &str| {
                let at = line.find(cell).expect("the cell is in the line");
                cell_width(&line[..at])
            };
            assert_eq!(
                column(lines[1], "1"),
                column(lines[2], "2"),
                "{} is one glyph, so the columns still line up: {:?}",
                what,
                lines
            );
            assert_eq!(
                cell_width(glyph),
                2,
                "{} is drawn in two cells: {:?}",
                what,
                glyph
            );
        }
    }

    /// A function inside a record is refused the way a function on its own is.
    ///
    /// The refusal is not the top of the document that is refused: the
    /// document is walked until the value that cannot be written is found, and
    /// the refusal names *that* value. A document that answered `null` about a
    /// function nested in a record would be a document that lies about the
    /// shape of the value it came from.
    #[test]
    fn edge_json_of_a_function_nested_in_a_record_is_refused_and_names_it() {
        let value = record(vec![
            ("name", Value::Text("ada".to_string())),
            ("work", function("work")),
        ]);

        let outcome: Result<String, String> = json(&value);
        assert!(
            outcome.is_err(),
            "a document holding a function is refused: {:?}",
            outcome
        );

        let refusal = outcome.expect_err("a document holding a function is refused");
        assert!(
            refusal.starts_with("Error:"),
            "and it is an error, not a document: {}",
            refusal
        );
        assert!(
            refusal.contains("function"),
            "and it names the value it cannot write: {}",
            refusal
        );
        assert_ne!(
            refusal.trim(),
            "null",
            "and it does not answer `null` about it: {}",
            refusal
        );
    }

    /// A list that holds something that is not a record is refused, and the
    /// refusal names the item and what it is.
    ///
    /// The kind is part of the message because the reader has to know *which*
    /// item is the wrong one: a list of records with one text in it is a list
    /// of records with a typo in it, and "item 2 is a text" says which one.
    #[test]
    fn edge_table_of_a_list_holding_a_text_is_refused_and_names_the_item() {
        let rows = Value::list(vec![
            record(vec![("name", Value::Text("ada".to_string()))]),
            Value::Text("bob".to_string()),
        ]);

        let outcome: Result<String, String> = table(&rows);
        assert!(
            outcome.is_err(),
            "a list holding a text is not a list of records: {:?}",
            outcome
        );

        let refusal = outcome.expect_err("a list holding a text is refused");
        assert!(
            refusal.contains("item 2"),
            "and it names the item that is not a record: {}",
            refusal
        );
        assert!(
            refusal.contains("text"),
            "and it says what the item is: {}",
            refusal
        );
    }
}
