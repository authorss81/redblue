use crate::error::{Error, Result};
use crate::value::Value;
use std::collections::HashMap;

pub fn call(function: &str, args: Vec<Value>) -> Result<Value> {
    match function {
        "parse_json" | "parseJSON" | "from_json" | "fromJSON" => parse_json(args),
        "to_json" | "toJSON" | "stringify" => to_json(args),
        "parse_csv" | "parseCSV" => parse_csv(args),
        "to_csv" | "toCSV" => to_csv(args),
        "parse_xml" | "parseXML" => parse_xml(args),
        "to_xml" | "toXML" => to_xml(args),
        "parse_html" | "parseHTML" => parse_html(args),
        "escape_html" | "escapeHTML" => escape_html(args),
        "unescape_html" | "unescapeHTML" => unescape_html(args),
        "escape_json" | "escapeJSON" => escape_json(args),
        _ => Err(Error::Runtime(format!("Unknown formats function '{}'", function))),
    }
}

fn parse_json(args: Vec<Value>) -> Result<Value> {
    let json = expect_text(args.first())?;
    parse_json_value(&json).map_err(|e| Error::Runtime(format!("JSON parse error: {}", e)))
}

fn parse_json_value(json: &str) -> Result<Value> {
    let json = json.trim();
    
    if json.starts_with('{') {
        parse_json_object(json)
    } else if json.starts_with('[') {
        parse_json_array(json)
    } else if json.starts_with('"') {
        parse_json_string(json)
    } else if json == "true" {
        Ok(Value::YesNo(true))
    } else if json == "false" {
        Ok(Value::YesNo(false))
    } else if json == "null" {
        Ok(Value::Nothing)
    } else if json.contains('.') {
        json.parse::<f64>()
            .map(Value::Number)
            .map_err(|e| Error::Runtime(e.to_string()))
    } else {
        json.parse::<i64>()
            .map(|n| Value::Number(n as f64))
            .or_else(|_| json.parse::<f64>().map(Value::Number))
            .map_err(|e| Error::Runtime(e.to_string()))
    }
}

fn parse_json_object(json: &str) -> Result<Value> {
    let json = json.trim();
    if !json.starts_with('{') || !json.ends_with('}') {
        return Err(Error::Runtime("Invalid JSON object".to_string()));
    }
    
    let inner = &json[1..json.len()-1].trim();
    if inner.is_empty() {
        return Ok(Value::Record(HashMap::new()));
    }
    
    let mut map = HashMap::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut depth = 0;
    
    for ch in inner.chars() {
        if ch == '"' && (current.is_empty() || !current.ends_with('\\')) {
            in_string = !in_string;
        }
        if !in_string {
            if ch == '{' || ch == '[' { depth += 1; }
            if ch == '}' || ch == ']' { depth -= 1; }
        }
        current.push(ch);
        
        if !in_string && depth == 0 && (ch == ',' || ch == '}') {
            let part = current.trim().trim_end_matches(',').trim_end_matches('}');
            if let Some((key, value)) = part.split_once(':') {
                let key = parse_json_string(&key.trim())?;
                if let Value::Text(k) = key {
                    let value = parse_json_value(&value.trim())?;
                    map.insert(k, value);
                }
            }
            current.clear();
        }
    }
    
    Ok(Value::Record(map))
}

fn parse_json_array(json: &str) -> Result<Value> {
    let json = json.trim();
    if !json.starts_with('[') || !json.ends_with(']') {
        return Err(Error::Runtime("Invalid JSON array".to_string()));
    }
    
    let inner = &json[1..json.len()-1].trim();
    if inner.is_empty() {
        return Ok(Value::List(Vec::new()));
    }
    
    let mut items = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut depth = 0;
    
    for ch in inner.chars() {
        if ch == '"' && (current.is_empty() || !current.ends_with('\\')) {
            in_string = !in_string;
        }
        if !in_string {
            if ch == '{' || ch == '[' { depth += 1; }
            if ch == '}' || ch == ']' { depth -= 1; }
        }
        current.push(ch);
        
        if !in_string && depth == 0 && ch == ',' {
            items.push(parse_json_value(current.trim().trim_end_matches(','))?);
            current.clear();
        }
    }
    
    if !current.trim().is_empty() {
        items.push(parse_json_value(current.trim())?);
    }
    
    Ok(Value::List(items))
}

fn parse_json_string(json: &str) -> Result<Value> {
    let json = json.trim();
    if !json.starts_with('"') || !json.ends_with('"') {
        return Err(Error::Runtime("Invalid JSON string".to_string()));
    }
    
    let inner = &json[1..json.len()-1];
    let mut result = String::new();
    let mut chars = inner.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('r') => result.push('\r'),
                Some('\\') => result.push('\\'),
                Some('"') => result.push('"'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Ok(code) = u16::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(code as u32) {
                            result.push(ch);
                        }
                    }
                },
                Some(c) => {
                    result.push('\\');
                    result.push(c);
                },
                None => break,
            }
        } else {
            result.push(c);
        }
    }
    
    Ok(Value::Text(result))
}

fn to_json(args: Vec<Value>) -> Result<Value> {
    let value = args.first().ok_or_else(|| Error::Runtime("Missing argument".to_string()))?;
    Ok(Value::Text(value_to_json(value, 0)))
}

fn value_to_json(value: &Value, indent: usize) -> String {
    match value {
        Value::Nothing => "null".to_string(),
        Value::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        },
        Value::Text(s) => string_to_json(s),
        Value::YesNo(b) => b.to_string(),
        Value::List(items) => {
            if items.is_empty() {
                "[]".to_string()
            } else {
                let inner: Vec<String> = items.iter()
                    .map(|v| value_to_json(v, indent + 1))
                    .collect();
                format!("[{}]", inner.join(", "))
            }
        },
        Value::Record(fields) => {
            if fields.is_empty() {
                "{}".to_string()
            } else {
                let inner: Vec<String> = fields.iter()
                    .map(|(k, v)| format!("{}: {}", string_to_json(k), value_to_json(v, indent + 1)))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
        },
        _ => "null".to_string(),
    }
}

fn string_to_json(s: &str) -> String {
    let escaped: String = s.chars()
        .map(|c| match c {
            '"' => "\\\"".to_string(),
            '\\' => "\\\\".to_string(),
            '\n' => "\\n".to_string(),
            '\r' => "\\r".to_string(),
            '\t' => "\\t".to_string(),
            c if c.is_control() => format!("\\u{:04x}", c as u32),
            c => c.to_string(),
        })
        .collect();
    format!("\"{}\"", escaped)
}

fn parse_csv(args: Vec<Value>) -> Result<Value> {
    let csv = expect_text(args.first())?;
    let delimiter = args.get(1).map(|v| expect_text(Some(v)))
        .unwrap_or(Ok(",".to_string()))?;
    
    let lines: Vec<&str> = csv.lines().collect();
    if lines.is_empty() {
        return Ok(Value::List(Vec::new()));
    }
    
    let headers: Vec<&str> = lines[0].split(&delimiter).collect();
    let mut result = Vec::new();
    
    for line in lines.iter().skip(1) {
        let values: Vec<&str> = line.split(&delimiter).collect();
        let mut record = HashMap::new();
        
        for (i, header) in headers.iter().enumerate() {
            let value = values.get(i).unwrap_or(&"");
            record.insert(header.to_string(), Value::Text(value.trim().to_string()));
        }
        
        result.push(Value::Record(record));
    }
    
    Ok(Value::List(result))
}

fn to_csv(args: Vec<Value>) -> Result<Value> {
    let records = expect_list(args.first())?;
    let delimiter = args.get(1).map(|v| expect_text(Some(v)))
        .unwrap_or(Ok(",".to_string()))?;
    
    if records.is_empty() {
        return Ok(Value::Text("".to_string()));
    }
    
    let mut lines = Vec::new();
    let mut headers = Vec::new();
    
    if let Value::Record(fields) = &records[0] {
        headers = fields.keys().cloned().collect();
        lines.push(headers.join(&delimiter));
    }
    
    for record in records {
        if let Value::Record(fields) = record {
            let values: Vec<String> = headers.iter()
                .map(|h| fields.get(h).map(|v| v.to_string()).unwrap_or_default())
                .collect();
            lines.push(values.join(&delimiter));
        }
    }
    
    Ok(Value::Text(lines.join("\n")))
}

fn parse_xml(_args: Vec<Value>) -> Result<Value> {
    Err(Error::Runtime("XML parsing not implemented yet".to_string()))
}

fn to_xml(_args: Vec<Value>) -> Result<Value> {
    Err(Error::Runtime("XML generation not implemented yet".to_string()))
}

fn parse_html(_args: Vec<Value>) -> Result<Value> {
    Err(Error::Runtime("HTML parsing not implemented yet".to_string()))
}

fn escape_html(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;");
    
    Ok(Value::Text(escaped))
}

fn unescape_html(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    
    let unescaped = text
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    
    Ok(Value::Text(unescaped))
}

fn escape_json(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    Ok(Value::Text(string_to_json(&text)))
}

fn expect_text(value: Option<&Value>) -> Result<String> {
    match value {
        Some(Value::Text(s)) => Ok(s.clone()),
        Some(v) => Err(Error::Runtime(format!("Expected text, got {}", v))),
        None => Err(Error::Runtime("Missing argument".to_string())),
    }
}

fn expect_list(value: Option<&Value>) -> Result<Vec<Value>> {
    match value {
        Some(Value::List(items)) => Ok(items.clone()),
        Some(v) => Err(Error::Runtime(format!("Expected list, got {}", v))),
        None => Err(Error::Runtime("Missing argument".to_string())),
    }
}
