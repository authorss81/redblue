//! CSV writing, TOML reading and the zip and tar archive formats.
//!
//! Every function here answers a `Result<_, String>`: the wording of a refusal
//! belongs beside the rule that produced it, and `runtime::builtin` turns it
//! into the `RuntimeError` the program catches. Nothing here panics on input a
//! program can supply.

use std::collections::HashSet;

use indexmap::IndexMap;

use crate::value::{Fields, Value};

// ------------------------------------------------------------------ CSV out

/// Writes rows of text cells as CSV.
///
/// A field is quoted when it contains a comma, a quote, a CR or an LF, when it
/// has leading or trailing whitespace, or when it is empty. The last two are
/// not cosmetic: `csv.parse` trims a bare field and treats empty text as no
/// rows, so an empty or space-padded field has to be quoted for the reader to
/// get the value back unchanged.
pub(crate) fn csv_stringify(value: &Value) -> Result<String, String> {
    let Value::List(rows) = value else {
        return Err("csv.stringify requires a list of rows".to_string());
    };

    let mut document = String::new();
    for (row_index, row) in rows.iter().enumerate() {
        let Value::List(cells) = row else {
            return Err("csv.stringify requires each row to be a list of cells".to_string());
        };
        if cells.is_empty() {
            return Err(
                "csv.stringify cannot write a row with no cells: CSV has no spelling for an \
                 empty row that csv.parse reads back as one"
                    .to_string(),
            );
        }
        if row_index > 0 {
            document.push('\n');
        }
        for (cell_index, cell) in cells.iter().enumerate() {
            if cell_index > 0 {
                document.push(',');
            }
            let cell = cell.to_string();
            write_csv_cell(&mut document, &cell);
        }
    }
    Ok(document)
}

fn write_csv_cell(out: &mut String, cell: &str) {
    let must_quote =
        cell.is_empty() || cell.trim() != cell || cell.contains([',', '"', '\n', '\r']);
    if !must_quote {
        out.push_str(cell);
        return;
    }
    out.push('"');
    for c in cell.chars() {
        if c == '"' {
            out.push('"');
        }
        out.push(c);
    }
    out.push('"');
}

// ------------------------------------------------------------------ JSON Lines

/// Writes one JSON value per line. The JSON writer already escapes a newline
/// inside a value, so a value cannot break the line structure.
pub(crate) fn json_stringify_lines(value: &Value) -> Result<String, String> {
    let Value::List(items) = value else {
        return Err("json.stringify_lines requires a list".to_string());
    };
    let lines = items
        .iter()
        .map(crate::runtime::json_stringify)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(lines.join("\n"))
}

/// Reads one JSON value per line. Blank lines are skipped, and a malformed
/// line is refused with the line number a reader can find in the file.
pub(crate) fn json_parse_lines(
    text: &str,
    parse: impl Fn(&str) -> Result<Value, String>,
) -> Result<Value, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut items = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.trim().is_empty() {
            continue;
        }
        let value = parse(line.trim()).map_err(|message| {
            format!(
                "Invalid JSON on line {}: {}",
                index + 1,
                message.trim_start_matches("Invalid JSON: ")
            )
        })?;
        items.push(value);
    }
    Ok(Value::list(items))
}

// ------------------------------------------------------------------ TOML

type TomlTable = IndexMap<String, Toml>;

const TOML_MAX_NESTING: usize = 100;
const TOML_MAX_KEY_PATH: usize = 100;

#[derive(Clone, Debug)]
enum Toml {
    Table(TomlTable),
    Array(Vec<Toml>),
    TableArray(Vec<Toml>),
    Value(Value),
}

impl Toml {
    fn into_value(self) -> Value {
        match self {
            Toml::Table(fields) => {
                let mut out = Fields::new();
                for (key, value) in fields {
                    out.insert(key, value.into_value());
                }
                Value::record(out)
            }
            Toml::Array(items) | Toml::TableArray(items) => {
                Value::list(items.into_iter().map(Toml::into_value).collect())
            }
            Toml::Value(value) => value,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Step {
    Key(String),
    Index(usize),
}

/// Parses the documented TOML subset described in SPEC.md § TOML.
pub(crate) fn toml_parse(text: &str) -> Result<Value, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut parser = TomlParser {
        chars: text.chars().collect(),
        at: 0,
        line: 1,
        nesting: 0,
        opened_tables: HashSet::new(),
    };
    let mut root = Toml::Table(TomlTable::new());
    let mut current: Vec<Step> = Vec::new();

    loop {
        parser.skip_space_and_comments(true);
        if parser.at_end() {
            break;
        }
        if parser.peek() == Some('[') {
            current = parser.parse_header(&mut root)?;
        } else {
            parser.parse_key_value(&mut root, &current)?;
        }
    }

    Ok(root.into_value())
}

struct TomlParser {
    chars: Vec<char>,
    at: usize,
    line: usize,
    nesting: usize,
    opened_tables: HashSet<Vec<Step>>,
}

impl TomlParser {
    fn at_end(&self) -> bool {
        self.at >= self.chars.len()
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.at + offset).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += 1;
        if c == '\n' {
            self.line += 1;
        }
        Some(c)
    }

    fn error<T>(&self, message: impl Into<String>) -> Result<T, String> {
        Err(format!(
            "Invalid TOML at line {}: {}",
            self.line,
            message.into()
        ))
    }

    fn error_at<T>(line: usize, message: impl Into<String>) -> Result<T, String> {
        Err(format!("Invalid TOML at line {line}: {}", message.into()))
    }

    fn skip_inline_space(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.bump();
        }
    }

    fn skip_space_and_comments(&mut self, newlines: bool) {
        loop {
            match self.peek() {
                Some(' ' | '\t' | '\r') => {
                    self.bump();
                }
                Some('\n') if newlines => {
                    self.bump();
                }
                Some('#') => {
                    while !matches!(self.peek(), None | Some('\n')) {
                        self.bump();
                    }
                }
                _ => break,
            }
        }
    }

    fn finish_line(&mut self) -> Result<(), String> {
        self.skip_inline_space();
        if self.peek() == Some('#') {
            while !matches!(self.peek(), None | Some('\n')) {
                self.bump();
            }
        }
        match self.peek() {
            None => Ok(()),
            Some('\n') => {
                self.bump();
                Ok(())
            }
            Some('\r') if self.peek_at(1) == Some('\n') => {
                self.bump();
                self.bump();
                Ok(())
            }
            Some(c) => self.error(format!("unexpected '{c}' after the value")),
        }
    }

    fn parse_header(&mut self, root: &mut Toml) -> Result<Vec<Step>, String> {
        let line = self.line;
        self.bump();
        let array = self.peek() == Some('[');
        if array {
            self.bump();
        }
        self.skip_inline_space();
        let path = self.parse_key_path()?;
        self.skip_inline_space();
        if self.bump() != Some(']') {
            return self.error("a table header needs a closing ']'");
        }
        if array && self.bump() != Some(']') {
            return self.error("an array-of-tables header needs ']]'");
        }
        self.finish_line()?;
        self.open_table(root, &path, array, line)
    }

    fn parse_key_path(&mut self) -> Result<Vec<String>, String> {
        let mut keys = Vec::new();
        loop {
            self.skip_inline_space();
            let key = match self.peek() {
                Some('"') => self.parse_basic_string()?,
                Some('\'') => self.parse_literal_string()?,
                Some(c) if is_bare_key(c) => {
                    let mut key = String::new();
                    while let Some(c) = self.peek().filter(|c| is_bare_key(*c)) {
                        key.push(c);
                        self.bump();
                    }
                    key
                }
                _ => return self.error("expected a key"),
            };
            keys.push(key);
            if keys.len() > TOML_MAX_KEY_PATH {
                return self.error(format!(
                    "a key path is longer than the supported maximum of {TOML_MAX_KEY_PATH} components"
                ));
            }
            self.skip_inline_space();
            if self.peek() == Some('.') {
                self.bump();
                continue;
            }
            break;
        }
        Ok(keys)
    }

    fn parse_key_value(&mut self, root: &mut Toml, current: &[Step]) -> Result<(), String> {
        let key_line = self.line;
        let path = self.parse_key_path()?;
        self.skip_inline_space();
        if self.bump() != Some('=') {
            return self.error("expected '=' after a key");
        }
        self.skip_inline_space();
        let value = self.parse_value()?;
        self.finish_line()?;
        let table = table_at(root, current)?;
        insert_dotted(table, &path, value, key_line)
    }

    fn open_table(
        &mut self,
        root: &mut Toml,
        path: &[String],
        array: bool,
        line: usize,
    ) -> Result<Vec<Step>, String> {
        let mut steps = Vec::new();
        let mut table = match root {
            Toml::Table(fields) => fields,
            _ => return Self::error_at(line, "the document root is not a table"),
        };

        for (index, key) in path.iter().enumerate() {
            let last = index + 1 == path.len();
            if last && array {
                if !table.contains_key(key) {
                    table.insert(key.clone(), Toml::TableArray(Vec::new()));
                }
                let length = match table.get_mut(key).expect("just inserted") {
                    Toml::TableArray(items) => {
                        items.push(Toml::Table(TomlTable::new()));
                        items.len()
                    }
                    Toml::Array(_) => {
                        return Self::error_at(
                            line,
                            format!(
                                "'{key}' is already an array value and is not an array of tables"
                            ),
                        )
                    }
                    _ => {
                        return Self::error_at(
                            line,
                            format!("'{key}' is already defined and is not an array of tables"),
                        )
                    }
                };
                steps.push(Step::Key(key.clone()));
                steps.push(Step::Index(length - 1));
                return Ok(steps);
            }

            if !table.contains_key(key) {
                table.insert(key.clone(), Toml::Table(TomlTable::new()));
            }
            let entry = table.get_mut(key).expect("just inserted");
            match entry {
                Toml::Table(fields) => {
                    steps.push(Step::Key(key.clone()));
                    if last {
                        if self.opened_tables.contains(&steps) {
                            return Self::error_at(
                                line,
                                format!("table '{key}' is already defined"),
                            );
                        }
                        self.opened_tables.insert(steps.clone());
                    }
                    table = fields;
                }
                Toml::TableArray(items) => {
                    if last {
                        return Self::error_at(
                            line,
                            format!("'{key}' is an array of tables, not a table"),
                        );
                    }
                    let length = items.len();
                    let Some(Toml::Table(fields)) = items.last_mut() else {
                        return Self::error_at(line, format!("'{key}' is not a table"));
                    };
                    steps.push(Step::Key(key.clone()));
                    steps.push(Step::Index(length - 1));
                    table = fields;
                }
                Toml::Array(_) | Toml::Value(_) => {
                    return Self::error_at(
                        line,
                        format!("'{key}' is already a value, not a table"),
                    );
                }
            }
        }
        Ok(steps)
    }

    fn parse_value(&mut self) -> Result<Toml, String> {
        if self.nesting >= TOML_MAX_NESTING {
            return self.error(format!(
                "values nest deeper than the supported maximum of {TOML_MAX_NESTING}"
            ));
        }
        self.nesting += 1;
        let value = self.parse_value_at_current_depth();
        self.nesting -= 1;
        value
    }

    fn parse_value_at_current_depth(&mut self) -> Result<Toml, String> {
        match self.peek() {
            Some('"') => Ok(Toml::Value(Value::Text(self.parse_basic_string()?))),
            Some('\'') => Ok(Toml::Value(Value::Text(self.parse_literal_string()?))),
            Some('[') => self.parse_array(),
            Some('{') => self.parse_inline_table(),
            Some(c) if c == 't' || c == 'f' => self.parse_boolean(),
            Some(_) => self.parse_number_or_word(),
            None => self.error("expected a value"),
        }
    }

    fn parse_basic_string(&mut self) -> Result<String, String> {
        self.bump();
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return self.error("an unterminated string"),
                Some('"') => return Ok(out),
                Some('\n') => return self.error("a string cannot contain a raw newline"),
                Some('\\') => {
                    let escaped = match self.bump() {
                        Some('"') => '"',
                        Some('\\') => '\\',
                        Some('b') => '\u{8}',
                        Some('t') => '\t',
                        Some('n') => '\n',
                        Some('f') => '\u{c}',
                        Some('r') => '\r',
                        Some('u') => self.parse_unicode_escape(4)?,
                        Some('U') => self.parse_unicode_escape(8)?,
                        Some(other) => {
                            return self.error(format!("unknown string escape '\\{other}'"))
                        }
                        None => return self.error("an unterminated string escape"),
                    };
                    out.push(escaped);
                }
                Some(c) => out.push(c),
            }
        }
    }

    fn parse_literal_string(&mut self) -> Result<String, String> {
        self.bump();
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return self.error("an unterminated literal string"),
                Some('\'') => return Ok(out),
                Some('\n') => return self.error("a string cannot contain a raw newline"),
                Some(c) => out.push(c),
            }
        }
    }

    fn parse_unicode_escape(&mut self, digits: usize) -> Result<char, String> {
        let mut value = 0u32;
        for _ in 0..digits {
            let Some(c) = self.bump() else {
                return self.error("an incomplete unicode escape");
            };
            let Some(digit) = c.to_digit(16) else {
                return self.error(format!("'{c}' is not a hexadecimal digit"));
            };
            value = value * 16 + digit;
        }
        char::from_u32(value).ok_or_else(|| {
            format!(
                "Invalid TOML at line {}: '\\u{:X}' is not a Unicode scalar value",
                self.line, value
            )
        })
    }

    fn parse_boolean(&mut self) -> Result<Toml, String> {
        let start = self.at;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.bump();
        }
        let word: String = self.chars[start..self.at].iter().collect();
        match word.as_str() {
            "true" => Ok(Toml::Value(Value::YesNo(true))),
            "false" => Ok(Toml::Value(Value::YesNo(false))),
            _ => self.error(format!("'{word}' is not a TOML value")),
        }
    }

    fn parse_number_or_word(&mut self) -> Result<Toml, String> {
        let start = self.at;
        while self
            .peek()
            .is_some_and(|c| !matches!(c, ' ' | '\t' | '\r' | '\n' | ',' | ']' | '}' | '#'))
        {
            self.bump();
        }
        let token: String = self.chars[start..self.at].iter().collect();
        if token.is_empty() {
            return self.error("expected a value");
        }
        if matches!(
            token.as_str(),
            "inf" | "+inf" | "-inf" | "nan" | "+nan" | "-nan"
        ) {
            return self.error(format!(
                "'{token}' has no finite Redblue number, and TOML's non-finite floats are not supported"
            ));
        }
        if looks_like_date_or_time(&token) {
            return self.error(format!("date and time value '{token}' is not supported"));
        }
        if token.contains(['.', 'e', 'E'])
            && !token.starts_with("0x")
            && !token.starts_with("0o")
            && !token.starts_with("0b")
            && !token.starts_with("+0x")
            && !token.starts_with("+0o")
            && !token.starts_with("+0b")
            && !token.starts_with("-0x")
            && !token.starts_with("-0o")
            && !token.starts_with("-0b")
        {
            let cleaned = token.replace('_', "");
            let number = cleaned.parse::<f64>().map_err(|_| {
                format!(
                    "Invalid TOML at line {}: '{token}' is not a number",
                    self.line
                )
            })?;
            if !number.is_finite() {
                return self.error(format!("'{token}' is not a finite number"));
            }
            return Ok(Toml::Value(Value::Number(number)));
        }
        let (sign, rest) = match token.as_bytes().first() {
            Some(b'+') => (1.0, &token[1..]),
            Some(b'-') => (-1.0, &token[1..]),
            _ => (1.0, token.as_str()),
        };
        let radix = rest
            .strip_prefix("0x")
            .map(|hex| (16, hex))
            .or_else(|| rest.strip_prefix("0o").map(|octal| (8, octal)))
            .or_else(|| rest.strip_prefix("0b").map(|binary| (2, binary)));
        let number = if let Some((radix, digits)) = radix {
            let digits = digits.replace('_', "");
            if digits.is_empty() || sign < 0.0 {
                return self.error(format!("'{token}' is not a TOML integer"));
            }
            let value = i64::from_str_radix(&digits, radix).map_err(|_| {
                format!(
                    "Invalid TOML at line {}: '{token}' is not an integer",
                    self.line
                )
            })?;
            exact_integer(&token, value, self.line)?
        } else {
            let digits = token.replace('_', "");
            let value = digits.parse::<i64>().map_err(|_| {
                format!(
                    "Invalid TOML at line {}: '{token}' is not an integer",
                    self.line
                )
            })?;
            exact_integer(&token, value, self.line)?
        };
        Ok(Toml::Value(Value::Number(number)))
    }

    fn parse_array(&mut self) -> Result<Toml, String> {
        self.bump();
        let mut items = Vec::new();
        loop {
            self.skip_space_and_comments(true);
            if self.peek() == Some(']') {
                self.bump();
                break;
            }
            items.push(self.parse_value()?);
            self.skip_space_and_comments(true);
            match self.bump() {
                Some(',') => continue,
                Some(']') => break,
                Some(c) => {
                    return self.error(format!("expected ',' or ']' in an array, got '{c}'"))
                }
                None => return self.error("an unterminated array"),
            }
        }
        Ok(Toml::Array(items))
    }

    fn parse_inline_table(&mut self) -> Result<Toml, String> {
        self.bump();
        let mut fields = TomlTable::new();
        self.skip_inline_space();
        if self.peek() == Some('}') {
            self.bump();
            return Ok(Toml::Table(fields));
        }
        loop {
            self.skip_inline_space();
            let key_line = self.line;
            let path = self.parse_key_path()?;
            self.skip_inline_space();
            if self.bump() != Some('=') {
                return self.error("expected '=' in an inline table");
            }
            self.skip_inline_space();
            let value = self.parse_value()?;
            insert_dotted(&mut fields, &path, value, key_line)?;
            self.skip_inline_space();
            match self.bump() {
                Some(',') => continue,
                Some('}') => break,
                Some(c) => {
                    return self.error(format!(
                        "expected ',' or '}}' in an inline table, got '{c}'"
                    ))
                }
                None => return self.error("an unterminated inline table"),
            }
        }
        Ok(Toml::Table(fields))
    }
}

fn exact_integer(token: &str, value: i64, line: usize) -> Result<f64, String> {
    let number = value as f64;
    if number as i128 != i128::from(value) {
        return Err(format!(
            "Invalid TOML at line {line}: '{token}' is an integer that cannot be represented exactly as a Redblue number"
        ));
    }
    Ok(number)
}

fn looks_like_date_or_time(token: &str) -> bool {
    let bytes = token.as_bytes();
    token.contains(':')
        || (bytes.len() >= 5 && bytes[4] == b'-' && bytes[..4].iter().all(u8::is_ascii_digit))
}

fn is_bare_key(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn table_at<'a>(root: &'a mut Toml, steps: &[Step]) -> Result<&'a mut TomlTable, String> {
    let mut current = root;
    for step in steps {
        current = match (step, current) {
            (Step::Key(key), Toml::Table(fields)) => fields
                .get_mut(key)
                .ok_or_else(|| format!("the table '{key}' does not exist"))?,
            (Step::Index(index), Toml::Array(items) | Toml::TableArray(items)) => items
                .get_mut(*index)
                .ok_or_else(|| "an array-of-tables index is out of range".to_string())?,
            _ => return Err("the table path does not lead to a table".to_string()),
        };
    }
    match current {
        Toml::Table(fields) => Ok(fields),
        _ => Err("the table path does not lead to a table".to_string()),
    }
}

fn insert_dotted(
    table: &mut TomlTable,
    keys: &[String],
    value: Toml,
    line: usize,
) -> Result<(), String> {
    let Some((first, rest)) = keys.split_first() else {
        return Err(format!("Invalid TOML at line {line}: a key is empty"));
    };
    if rest.is_empty() {
        if table.contains_key(first) {
            return Err(format!(
                "Invalid TOML at line {line}: duplicate key '{first}'"
            ));
        }
        table.insert(first.clone(), value);
        return Ok(());
    }
    if !table.contains_key(first) {
        table.insert(first.clone(), Toml::Table(TomlTable::new()));
    }
    let Some(Toml::Table(fields)) = table.get_mut(first) else {
        return Err(format!(
            "Invalid TOML at line {line}: '{first}' is a value, not a table"
        ));
    };
    insert_dotted(fields, rest, value, line)
}

// ------------------------------------------------------------------ archives

/// Reads a record of `name -> text` entries, or a byte list, into the bytes an
/// archive stores.
fn archive_entries(value: &Value) -> Result<Vec<(String, Vec<u8>)>, String> {
    let Value::Record(fields) = value else {
        return Err("an archive requires a record of entry names to contents".to_string());
    };
    fields
        .iter()
        .map(|(name, value)| {
            if name.contains('\0') {
                return Err(format!(
                    "archive entry name '{}' contains a NUL character, which archives cannot store",
                    name.escape_default()
                ));
            }
            let bytes = match value {
                Value::Text(text) => text.as_bytes().to_vec(),
                Value::List(items) => {
                    let mut bytes = Vec::with_capacity(items.len());
                    for item in items.iter() {
                        let Value::Number(number) = item else {
                            return Err(format!(
                                "archive entry '{name}' has a list member that is not a byte"
                            ));
                        };
                        if number.fract() != 0.0 || !(0.0..=255.0).contains(number) {
                            return Err(format!(
                                "archive entry '{name}' has '{number}', which is not a byte"
                            ));
                        }
                        bytes.push(*number as u8);
                    }
                    bytes
                }
                other => {
                    return Err(format!(
                    "archive entry '{name}' is {other}, but an entry must be text or a byte list"
                ))
                }
            };
            Ok((name.clone(), bytes))
        })
        .collect()
}

fn archive_value(entries: Vec<(String, Vec<u8>)>) -> Value {
    let mut fields = Fields::new();
    for (name, bytes) in entries {
        let value = match String::from_utf8(bytes) {
            Ok(text) => Value::Text(text),
            Err(error) => Value::list(
                error
                    .into_bytes()
                    .into_iter()
                    .map(|byte| Value::Number(f64::from(byte)))
                    .collect(),
            ),
        };
        fields.insert(name, value);
    }
    Value::record(fields)
}

// ------------------------------------------------------------------ tar

pub(crate) fn tar_write(value: &Value) -> Result<Vec<u8>, String> {
    let entries = archive_entries(value)?;
    let mut out = Vec::new();
    for (name, data) in entries {
        out.extend_from_slice(&tar_header(&name, data.len())?);
        out.extend_from_slice(&data);
        let padding = (512 - data.len() % 512) % 512;
        out.extend(std::iter::repeat_n(0u8, padding));
    }
    out.extend(std::iter::repeat_n(0u8, 1024));
    Ok(out)
}

fn tar_header(name: &str, size: usize) -> Result<[u8; 512], String> {
    let mut header = [0u8; 512];
    let (prefix, name) = tar_split_name(name)?;
    write_field(&mut header[0..100], name.as_bytes());
    write_octal(&mut header[100..108], 0o644)?;
    write_octal(&mut header[108..116], 0)?;
    write_octal(&mut header[116..124], 0)?;
    write_octal(&mut header[124..136], size as u64).map_err(|_| {
        format!("tar cannot store the entry '{name}': its size of {size} bytes is too large")
    })?;
    write_octal(&mut header[136..148], 0)?;
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    write_field(&mut header[345..500], prefix.as_bytes());
    for byte in &mut header[148..156] {
        *byte = b' ';
    }
    let checksum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
    write_checksum(&mut header[148..156], checksum);
    Ok(header)
}

fn tar_split_name(name: &str) -> Result<(&str, &str), String> {
    if name.len() <= 100 {
        return Ok(("", name));
    }
    let bytes = name.as_bytes();
    for index in (0..bytes.len()).rev() {
        if bytes[index] != b'/' {
            continue;
        }
        let (prefix, rest) = name.split_at(index);
        let rest = &rest[1..];
        if prefix.len() <= 155 && !rest.is_empty() && rest.len() <= 100 {
            return Ok((prefix, rest));
        }
    }
    Err(format!(
        "tar cannot store the entry name '{name}': it is longer than 100 bytes and cannot be split into a 155-byte prefix and a 100-byte name"
    ))
}

fn write_field(field: &mut [u8], text: &[u8]) {
    let length = text.len().min(field.len());
    field[..length].copy_from_slice(&text[..length]);
}

fn write_octal(field: &mut [u8], value: u64) -> Result<(), String> {
    let digits = field.len() - 1;
    let text = format!("{:0width$o}", value, width = digits);
    if text.len() > digits {
        return Err(format!(
            "the value {value} does not fit in a {digits}-digit octal field"
        ));
    }
    field[..digits].copy_from_slice(text.as_bytes());
    field[digits] = 0;
    Ok(())
}

fn write_checksum(field: &mut [u8], value: u64) {
    let text = format!("{:06o}\0 ", value);
    field.copy_from_slice(text.as_bytes());
}

pub(crate) fn tar_read(bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() < 512 {
        return Err("not a tar archive: the file is shorter than one header".to_string());
    }
    let mut entries = Vec::new();
    let mut at = 0usize;
    let mut saw_zero = false;

    while at + 512 <= bytes.len() {
        let header = &bytes[at..at + 512];
        if header.iter().all(|byte| *byte == 0) {
            saw_zero = true;
            break;
        }
        verify_tar_checksum(header)?;
        let name = tar_name(header)?;
        let size = tar_octal(&header[124..136])
            .map_err(|_| format!("tar entry '{name}' has a malformed size field"))?
            as usize;
        let typeflag = header[156];
        at += 512;
        if at + size > bytes.len() {
            return Err(format!(
                "tar entry '{name}' is shorter than its declared size"
            ));
        }
        match typeflag {
            b'0' | 0 => entries.push((name, bytes[at..at + size].to_vec())),
            b'5' => {}
            other => {
                return Err(format!(
                    "tar entry '{name}' has unsupported typeflag '{}'",
                    other as char
                ))
            }
        }
        at += size;
        at += (512 - size % 512) % 512;
    }

    if !saw_zero {
        return Err("not a tar archive: it has no end-of-archive blocks".to_string());
    }
    Ok(archive_value(entries))
}

fn verify_tar_checksum(header: &[u8]) -> Result<(), String> {
    let stored = tar_octal(&header[148..156])
        .map_err(|_| "not a tar archive: its header checksum is malformed".to_string())?;
    let mut copy = header.to_vec();
    for byte in &mut copy[148..156] {
        *byte = b' ';
    }
    let computed: u64 = copy.iter().map(|byte| u64::from(*byte)).sum();
    if stored != computed {
        return Err(format!(
            "not a tar archive: a header checksum is {stored} but its bytes sum to {computed}"
        ));
    }
    Ok(())
}

fn tar_octal(field: &[u8]) -> Result<u64, ()> {
    let end = field
        .iter()
        .position(|byte| *byte == 0 || *byte == b' ')
        .unwrap_or(field.len());
    let text = std::str::from_utf8(&field[..end]).map_err(|_| ())?;
    if text.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(text.trim(), 8).map_err(|_| ())
}

fn tar_name(header: &[u8]) -> Result<String, String> {
    let name = field_text(&header[0..100])?;
    let prefix = field_text(&header[345..500])?;
    if prefix.is_empty() {
        Ok(name)
    } else {
        Ok(format!("{prefix}/{name}"))
    }
}

fn field_text(field: &[u8]) -> Result<String, String> {
    let end = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    std::str::from_utf8(&field[..end])
        .map(str::to_owned)
        .map_err(|_| {
            format!(
                "not a tar archive: an entry name is not UTF-8: {:?}",
                &field[..end]
            )
        })
}

// ------------------------------------------------------------------ zip

const ZIP_LOCAL: u32 = 0x0403_4b50;
const ZIP_CENTRAL: u32 = 0x0201_4b50;
const ZIP_END: u32 = 0x0605_4b50;

struct ZipEntry {
    crc: u32,
    offset: u32,
}

pub(crate) fn zip_write(value: &Value) -> Result<Vec<u8>, String> {
    let entries = archive_entries(value)?;
    let mut out = Vec::new();
    let mut central = Vec::new();

    for (name, data) in entries {
        let offset = u32::try_from(out.len())
            .map_err(|_| "the zip archive is too large to write".to_string())?;
        let crc = crc32(&data);
        let name_bytes = name.as_bytes();
        let name_len = u16::try_from(name_bytes.len()).map_err(|_| {
            format!("zip cannot store the entry name '{name}': it is longer than 65535 bytes")
        })?;
        let size = u32::try_from(data.len())
            .map_err(|_| format!("zip entry '{name}' is too large to write"))?;

        push_u32(&mut out, ZIP_LOCAL);
        push_u16(&mut out, 20);
        push_u16(&mut out, 0x0800);
        push_u16(&mut out, 0);
        push_u16(&mut out, 0);
        push_u16(&mut out, 0);
        push_u32(&mut out, crc);
        push_u32(&mut out, size);
        push_u32(&mut out, size);
        push_u16(&mut out, name_len);
        push_u16(&mut out, 0);
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(&data);

        let entry = ZipEntry { crc, offset };
        push_central(&mut central, &entry, name.as_bytes(), size)?;
    }

    let central_offset = u32::try_from(out.len())
        .map_err(|_| "the zip archive is too large to write".to_string())?;
    let central_size = u32::try_from(central.len())
        .map_err(|_| "the zip archive is too large to write".to_string())?;
    let count = u16::try_from(central_chunks(&central))
        .map_err(|_| "the zip archive has too many entries".to_string())?;
    out.extend_from_slice(&central);

    push_u32(&mut out, ZIP_END);
    push_u16(&mut out, 0);
    push_u16(&mut out, 0);
    push_u16(&mut out, count);
    push_u16(&mut out, count);
    push_u32(&mut out, central_size);
    push_u32(&mut out, central_offset);
    push_u16(&mut out, 0);
    Ok(out)
}

fn central_chunks(central: &[u8]) -> usize {
    let mut at = 0usize;
    let mut count = 0usize;
    while at + 46 <= central.len() {
        if read_u32(central, at) != Some(ZIP_CENTRAL) {
            break;
        }
        let name = read_u16(central, at + 28).unwrap_or(0) as usize;
        let extra = read_u16(central, at + 30).unwrap_or(0) as usize;
        let comment = read_u16(central, at + 32).unwrap_or(0) as usize;
        at += 46 + name + extra + comment;
        count += 1;
    }
    count
}

fn push_central(out: &mut Vec<u8>, entry: &ZipEntry, name: &[u8], size: u32) -> Result<(), String> {
    let name_len = u16::try_from(name.len()).map_err(|_| {
        format!(
            "zip cannot store the entry name '{}': it is longer than 65535 bytes",
            String::from_utf8_lossy(name)
        )
    })?;
    push_u32(out, ZIP_CENTRAL);
    push_u16(out, 20);
    push_u16(out, 20);
    push_u16(out, 0x0800);
    push_u16(out, 0);
    push_u16(out, 0);
    push_u16(out, 0);
    push_u32(out, entry.crc);
    push_u32(out, size);
    push_u32(out, size);
    push_u16(out, name_len);
    push_u16(out, 0);
    push_u16(out, 0);
    push_u16(out, 0);
    push_u16(out, 0);
    push_u32(out, 0);
    push_u32(out, entry.offset);
    out.extend_from_slice(name);
    Ok(())
}

/// The Unicode code points CP437 gives the bytes `0x80..=0xFF`. Bytes below
/// `0x80` are ASCII in CP437, so only the high half needs a table.
const CP437_HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', 'É', 'æ', 'Æ',
    'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', 'á', 'í', 'ó', 'ú', 'ñ', 'Ñ',
    'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕',
    '╣', '║', '╗', '╝', '╜', '╛', '┐', '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦',
    '╠', '═', '╬', '╧', '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐',
    '▀', 'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', '≡', '±',
    '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];

fn decode_zip_name(name: &[u8], utf8: bool) -> Result<String, String> {
    if utf8 {
        return String::from_utf8(name.to_vec())
            .map_err(|_| "not a zip archive: an entry name is not UTF-8".to_string());
    }
    Ok(name
        .iter()
        .map(|byte| {
            if *byte < 0x80 {
                char::from(*byte)
            } else {
                CP437_HIGH[usize::from(*byte) - 0x80]
            }
        })
        .collect())
}

pub(crate) fn zip_read(bytes: &[u8]) -> Result<Value, String> {
    let end = find_zip_end(bytes)
        .ok_or_else(|| "not a zip archive: no end-of-central-directory record".to_string())?;
    let count = read_u16(bytes, end + 10)
        .ok_or_else(|| "not a zip archive: the directory count is missing".to_string())?
        as usize;
    let central_size = read_u32(bytes, end + 12)
        .ok_or_else(|| "not a zip archive: the directory size is missing".to_string())?
        as usize;
    let central_offset = read_u32(bytes, end + 16)
        .ok_or_else(|| "not a zip archive: the directory offset is missing".to_string())?
        as usize;
    let central_end = central_offset
        .checked_add(central_size)
        .ok_or_else(|| "not a zip archive: the directory size overflows".to_string())?;
    if central_end > end {
        return Err(
            "not a zip archive: the central directory runs past its end record".to_string(),
        );
    }

    let mut entries = Vec::new();
    let mut at = central_offset;
    for _ in 0..count {
        if read_u32(bytes, at) != Some(ZIP_CENTRAL) {
            return Err("not a zip archive: a central-directory entry is malformed".to_string());
        }
        let name_start = at.checked_add(46).ok_or_else(|| {
            "not a zip archive: a central-directory entry offset overflows".to_string()
        })?;
        if name_start > bytes.len() {
            return Err("not a zip archive: a central-directory entry is truncated".to_string());
        }
        let flags = read_u16(bytes, at + 8).unwrap_or(0);
        let method = read_u16(bytes, at + 10).unwrap_or(0);
        let stored_crc = read_u32(bytes, at + 16).unwrap_or(0);
        let compressed = read_u32(bytes, at + 20).unwrap_or(0) as usize;
        let uncompressed = read_u32(bytes, at + 24).unwrap_or(0) as usize;
        let name_len = read_u16(bytes, at + 28).unwrap_or(0) as usize;
        let extra_len = read_u16(bytes, at + 30).unwrap_or(0) as usize;
        let comment_len = read_u16(bytes, at + 32).unwrap_or(0) as usize;
        let local_offset = read_u32(bytes, at + 42).unwrap_or(0) as usize;
        let name_end = name_start
            .checked_add(name_len)
            .ok_or_else(|| "not a zip archive: an entry name offset overflows".to_string())?;
        if name_end > bytes.len() {
            return Err("not a zip archive: an entry name is truncated".to_string());
        }
        let name = decode_zip_name(&bytes[name_start..name_end], flags & 0x0800 != 0)?;

        if flags & 1 != 0 {
            return Err(format!(
                "zip entry '{name}' is encrypted, and encrypted zip entries are not supported"
            ));
        }
        if method != 0 {
            return Err(format!(
                "zip entry '{name}' uses unsupported compression method {method}{}",
                compression_name(method)
            ));
        }
        if compressed != uncompressed {
            return Err(format!(
                "zip entry '{name}' is stored without compression but its sizes disagree"
            ));
        }
        if read_u32(bytes, local_offset) != Some(ZIP_LOCAL) {
            return Err(format!("zip entry '{name}' has no local header"));
        }
        let local_name = local_offset
            .checked_add(26)
            .and_then(|offset| read_u16(bytes, offset))
            .ok_or_else(|| {
                format!("not a zip archive: the local header of entry '{name}' overflows")
            })? as usize;
        let local_extra = local_offset
            .checked_add(28)
            .and_then(|offset| read_u16(bytes, offset))
            .ok_or_else(|| {
                format!("not a zip archive: the local header of entry '{name}' overflows")
            })? as usize;
        let data_start = local_offset
            .checked_add(30)
            .and_then(|offset| offset.checked_add(local_name))
            .and_then(|offset| offset.checked_add(local_extra))
            .ok_or_else(|| {
                format!("not a zip archive: the local header of entry '{name}' overflows")
            })?;
        let data_end = data_start
            .checked_add(compressed)
            .ok_or_else(|| format!("zip entry '{name}' is truncated"))?;
        if data_end > bytes.len() {
            return Err(format!("zip entry '{name}' is truncated"));
        }
        let data = &bytes[data_start..data_end];
        let actual_crc = crc32(data);
        if actual_crc != stored_crc {
            return Err(format!(
                "zip entry '{name}' has CRC-32 {actual_crc} but the central directory records {stored_crc}"
            ));
        }
        entries.push((name, data.to_vec()));
        at = name_end
            .checked_add(extra_len)
            .and_then(|offset| offset.checked_add(comment_len))
            .ok_or_else(|| "not a zip archive: a central-directory entry overflows".to_string())?;
    }

    if at != central_end {
        return Err(
            "not a zip archive: the central directory size does not match its entries".to_string(),
        );
    }

    Ok(archive_value(entries))
}

fn compression_name(method: u16) -> &'static str {
    match method {
        8 => " (deflate)",
        9 => " (deflate64)",
        12 => " (bzip2)",
        14 => " (lzma)",
        93 => " (zstd)",
        95 => " (xz)",
        98 => " (ppmd)",
        99 => " (AES encrypted)",
        _ => "",
    }
}

fn find_zip_end(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 22 {
        return None;
    }
    let start = bytes.len().saturating_sub(22 + 65_535);
    (start..=bytes.len() - 22).rev().find(|at| {
        if read_u32(bytes, *at) != Some(ZIP_END) {
            return false;
        }
        let comment_len = (*at)
            .checked_add(20)
            .and_then(|offset| read_u16(bytes, offset));
        comment_len.is_some_and(|comment_len| {
            (*at)
                .checked_add(22)
                .and_then(|offset| offset.checked_add(usize::from(comment_len)))
                == Some(bytes.len())
        })
    })
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let end = at.checked_add(2)?;
    let slice = bytes.get(at..end)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    let slice = bytes.get(at..end)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;

    fn record(pairs: &[(&str, &str)]) -> Value {
        let mut fields = Fields::new();
        for (name, text) in pairs {
            fields.insert((*name).to_string(), Value::Text((*text).to_string()));
        }
        Value::record(fields)
    }

    #[test]
    fn edge_zip_refuses_an_unsupported_compression_method_by_name() {
        let mut bytes = Vec::new();
        push_u32(&mut bytes, ZIP_LOCAL);
        push_u16(&mut bytes, 20);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 8);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 1);
        push_u32(&mut bytes, 1);
        push_u16(&mut bytes, 1);
        push_u16(&mut bytes, 0);
        bytes.push(b'a');
        bytes.push(0);
        let local_end = bytes.len();

        let mut central = Vec::new();
        push_u32(&mut central, ZIP_CENTRAL);
        push_u16(&mut central, 20);
        push_u16(&mut central, 20);
        push_u16(&mut central, 0);
        push_u16(&mut central, 8);
        push_u16(&mut central, 0);
        push_u16(&mut central, 0);
        push_u32(&mut central, 0);
        push_u32(&mut central, 1);
        push_u32(&mut central, 1);
        push_u16(&mut central, 1);
        push_u16(&mut central, 0);
        push_u16(&mut central, 0);
        push_u16(&mut central, 0);
        push_u16(&mut central, 0);
        push_u32(&mut central, 0);
        push_u32(&mut central, 0);
        central.push(b'a');

        let central_offset = local_end as u32;
        bytes.extend_from_slice(&central);
        push_u32(&mut bytes, ZIP_END);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 1);
        push_u16(&mut bytes, 1);
        push_u32(&mut bytes, central.len() as u32);
        push_u32(&mut bytes, central_offset);
        push_u16(&mut bytes, 0);

        let error = zip_read(&bytes).expect_err("deflate is not supported");
        assert!(
            error.contains("compression method 8") && error.contains("deflate"),
            "the refusal must name the method and what it is: {error}"
        );
        assert!(
            error.contains("'a'"),
            "the refusal must name the entry: {error}"
        );
    }

    #[test]
    fn edge_tar_refuses_a_truncated_archive() {
        let written = tar_write(&record(&[("a.txt", "alpha")])).expect("tar writes");
        let error = tar_read(&written[..600]).expect_err("a truncated tar is refused");
        assert!(
            error.contains("tar archive"),
            "a truncated tar must be refused as one: {error}"
        );
    }

    #[test]
    fn edge_archive_round_trips_a_non_utf8_entry_as_bytes() {
        let mut fields = Fields::new();
        fields.insert(
            "raw.bin".to_string(),
            Value::list(vec![Value::Number(0.0), Value::Number(255.0)]),
        );
        let value = Value::record(fields);
        let zip = zip_write(&value).expect("zip writes");
        let tar = tar_write(&value).expect("tar writes");
        let expected = Value::list(vec![Value::Number(0.0), Value::Number(255.0)]);
        for (what, bytes) in [("zip", zip), ("tar", tar)] {
            let read = if what == "zip" {
                zip_read(&bytes).expect("zip reads")
            } else {
                tar_read(&bytes).expect("tar reads")
            };
            let Value::Record(fields) = read else {
                panic!("{what} must answer a record");
            };
            assert_eq!(
                fields.get("raw.bin"),
                Some(&expected),
                "{what} must return a non-UTF-8 entry as its bytes"
            );
        }
    }

    #[test]
    fn edge_csv_refuses_a_row_with_no_cells() {
        let error = csv_stringify(&Value::list(vec![Value::list(Vec::new())]))
            .expect_err("an empty row has no CSV spelling");
        assert!(error.contains("no cells"), "{error}");
        assert!(csv_stringify(&Value::Number(1.0)).is_err());
    }

    #[test]
    fn edge_zip_refuses_an_entry_name_longer_than_its_field() {
        let name = "x".repeat(65_536);
        let mut fields = Fields::new();
        fields.insert(name, Value::Text(String::new()));
        let error = zip_write(&Value::record(fields)).expect_err("the name does not fit");
        assert!(
            error.contains("longer than 65535"),
            "an oversized name must be refused by name: {error}"
        );
    }

    #[test]
    fn edge_tar_refuses_an_entry_larger_than_its_size_field() {
        let too_large = 1usize << 33;
        let error = tar_header("big.bin", too_large).expect_err("the size does not fit");
        assert!(
            error.contains("big.bin") && error.contains("too large"),
            "an oversized size must name the entry: {error}"
        );
    }

    #[test]
    fn edge_zip_reads_a_cp437_name_when_utf8_is_unset() {
        let data = b"hi";
        let name = [b'c', b'a', b'f', 0x82];
        let mut bytes = Vec::new();
        push_u32(&mut bytes, ZIP_LOCAL);
        push_u16(&mut bytes, 20);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u32(&mut bytes, crc32(data));
        push_u32(&mut bytes, data.len() as u32);
        push_u32(&mut bytes, data.len() as u32);
        push_u16(&mut bytes, name.len() as u16);
        push_u16(&mut bytes, 0);
        bytes.extend_from_slice(&name);
        bytes.extend_from_slice(data);
        let local_end = bytes.len();

        let mut central = Vec::new();
        push_u32(&mut central, ZIP_CENTRAL);
        push_u16(&mut central, 20);
        push_u16(&mut central, 20);
        push_u16(&mut central, 0);
        push_u16(&mut central, 0);
        push_u16(&mut central, 0);
        push_u16(&mut central, 0);
        push_u32(&mut central, crc32(data));
        push_u32(&mut central, data.len() as u32);
        push_u32(&mut central, data.len() as u32);
        push_u16(&mut central, name.len() as u16);
        push_u16(&mut central, 0);
        push_u16(&mut central, 0);
        push_u16(&mut central, 0);
        push_u16(&mut central, 0);
        push_u32(&mut central, 0);
        push_u32(&mut central, 0);
        central.extend_from_slice(&name);

        let central_offset = local_end as u32;
        bytes.extend_from_slice(&central);
        push_u32(&mut bytes, ZIP_END);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 1);
        push_u16(&mut bytes, 1);
        push_u32(&mut bytes, central.len() as u32);
        push_u32(&mut bytes, central_offset);
        push_u16(&mut bytes, 0);

        let read = zip_read(&bytes).expect("a CP437 name is valid when bit 11 is clear");
        let Value::Record(fields) = read else {
            panic!("zip.read must answer a record");
        };
        assert_eq!(
            fields.get("café"),
            Some(&Value::Text("hi".to_string())),
            "a name without bit 11 must be decoded as CP437"
        );
    }

    #[test]
    fn edge_archive_refuses_an_entry_name_with_a_nul() {
        let mut fields = Fields::new();
        fields.insert("bad\0name".to_string(), Value::Text("x".to_string()));
        let value = Value::record(fields);
        for (what, result) in [("zip", zip_write(&value)), ("tar", tar_write(&value))] {
            let error = result.expect_err("a name with a NUL cannot round-trip");
            assert!(
                error.contains("NUL"),
                "{what} must refuse a NUL name by name: {error}"
            );
        }
    }

    #[test]
    fn edge_zip_refuses_a_corrupted_stored_entry() {
        let written = zip_write(&record(&[("a.txt", "alpha")])).expect("zip writes");
        let mut corrupted = written.clone();
        let position = corrupted
            .windows(5)
            .position(|window| window == &b"alpha"[..])
            .expect("the stored bytes are in the archive");
        corrupted[position] ^= 1;
        let error = zip_read(&corrupted).expect_err("corrupted data must be refused");
        assert!(
            error.contains("a.txt") && error.contains("CRC-32"),
            "a corrupt entry must be refused and named: {error}"
        );
    }

    #[test]
    fn edge_toml_refuses_an_array_of_tables_over_a_value_array() {
        let error = toml_parse("x = [1]\n[[x]]\na = 1\n")
            .expect_err("a value array cannot become an array of tables");
        assert!(
            error.contains("array value") && error.contains("array of tables"),
            "a value array must not be re-opened as an array of tables: {error}"
        );
    }

    #[test]
    fn edge_toml_refuses_a_table_defined_twice() {
        let error =
            toml_parse("[a]\nx = 1\n[a]\ny = 2\n").expect_err("a table cannot be defined twice");
        assert!(
            error.contains("already defined"),
            "a duplicate table header must be refused: {error}"
        );
    }

    #[test]
    fn edge_toml_refuses_an_integer_a_number_cannot_hold_exactly() {
        let error = toml_parse("n = 9223372036854775807\n")
            .expect_err("the largest i64 is not exactly a Redblue number");
        assert!(
            error.contains("line 1") && error.contains("exactly"),
            "a non-exact integer must be refused with its line: {error}"
        );
        toml_parse("n = 9007199254740994\n").expect("2^53 + 2 is exact");
    }

    #[test]
    fn edge_toml_refuses_time_only_and_single_digit_dates_as_dates() {
        for document in [
            "t = 07:32:00\n",
            "d = 2024-1-01\n",
            "d = 2024-01-01T00:00:00Z\n",
        ] {
            let error = toml_parse(document).expect_err("a date or time is not supported");
            assert!(
                error.contains("date and time") && error.contains("not supported"),
                "'{document}' must be refused as a date or time: {error}"
            );
        }
    }

    #[test]
    fn edge_tar_refuses_a_non_utf8_entry_name() {
        let mut bytes = tar_write(&record(&[("a.txt", "alpha")])).expect("tar writes");
        let name = b"bad\xff.txt";
        bytes[..name.len()].copy_from_slice(name);
        for byte in &mut bytes[148..156] {
            *byte = b' ';
        }
        let checksum: u64 = bytes[..512].iter().map(|byte| u64::from(*byte)).sum();
        write_checksum(&mut bytes[148..156], checksum);
        let error = tar_read(&bytes).expect_err("a non-UTF-8 name is refused");
        assert!(
            error.contains("not a tar archive") && error.contains("UTF-8"),
            "a non-UTF-8 entry name must be refused as such: {error}"
        );
    }

    #[test]
    fn edge_zip_refuses_a_local_offset_past_the_archive() {
        let mut bytes = zip_write(&record(&[("a.txt", "alpha")])).expect("zip writes");
        let central = bytes
            .windows(4)
            .position(|window| window == &ZIP_CENTRAL.to_le_bytes()[..])
            .expect("the central directory is present");
        bytes[central + 42..central + 46].copy_from_slice(&u32::MAX.to_le_bytes());
        let error = zip_read(&bytes).expect_err("an out-of-range local offset is refused");
        assert!(
            error.contains("a.txt") && error.contains("local header"),
            "an out-of-range local offset must be refused by entry: {error}"
        );
    }
}
