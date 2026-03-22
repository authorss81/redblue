use crate::error::{Error, Result};
use crate::value::Value;

pub fn call(function: &str, args: Vec<Value>) -> Result<Value> {
    match function {
        "length" | "len" => length(args),
        "uppercase" | "upper" => uppercase(args),
        "lowercase" | "lower" => lowercase(args),
        "trim" => trim(args),
        "split" => split(args),
        "join" => join(args),
        "contains" => contains(args),
        "starts_with" | "startsWith" => starts_with(args),
        "ends_with" | "endsWith" => ends_with(args),
        "replace" => replace(args),
        "reverse" => reverse(args),
        "index_of" | "indexOf" => index_of(args),
        "substring" => substring(args),
        "repeat" => repeat(args),
        "chars" => chars(args),
        "lines" => lines(args),
        "words" => words(args),
        "is_empty" | "isEmpty" => is_empty(args),
        "is_numeric" | "isNumeric" => is_numeric(args),
        "pad_start" | "padStart" => pad_start(args),
        "pad_end" | "padEnd" => pad_end(args),
        "capitalize" => capitalize(args),
        _ => Err(Error::Runtime(format!("Unknown text function '{}'", function))),
    }
}

fn length(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    Ok(Value::Number(text.len() as f64))
}

fn uppercase(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    Ok(Value::Text(text.to_uppercase()))
}

fn lowercase(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    Ok(Value::Text(text.to_lowercase()))
}

fn trim(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    Ok(Value::Text(text.trim().to_string()))
}

fn split(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    let delimiter = args.get(1).map(expect_text).unwrap_or(Ok(",".to_string()))?;
    
    let parts: Vec<Value> = text.split(&delimiter)
        .map(|s| Value::Text(s.to_string()))
        .collect();
    
    Ok(Value::List(parts))
}

fn join(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    let delimiter = args.get(1).map(expect_text).unwrap_or(Ok("".to_string()))?;
    
    let strings: Vec<String> = list.iter()
        .map(|v| v.to_string())
        .collect();
    
    Ok(Value::Text(strings.join(&delimiter)))
}

fn contains(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    let search = expect_text(args.get(1))?;
    Ok(Value::YesNo(text.contains(&search)))
}

fn starts_with(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    let prefix = expect_text(args.get(1))?;
    Ok(Value::YesNo(text.starts_with(&prefix)))
}

fn ends_with(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    let suffix = expect_text(args.get(1))?;
    Ok(Value::YesNo(text.ends_with(&suffix)))
}

fn replace(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    let from = expect_text(args.get(1))?;
    let to = expect_text(args.get(2))?;
    Ok(Value::Text(text.replace(&from, &to)))
}

fn reverse(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    Ok(Value::Text(text.chars().rev().collect()))
}

fn index_of(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    let search = expect_text(args.get(1))?;
    
    match text.find(&search) {
        Some(idx) => Ok(Value::Number(idx as f64)),
        None => Ok(Value::Number(-1.0)),
    }
}

fn substring(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    let start = expect_number(args.get(1))? as usize;
    let end = args.get(2).map(expect_number).unwrap_or(Ok(text.len() as f64))? as usize;
    
    let chars: Vec<char> = text.chars().collect();
    let end = end.min(chars.len());
    let start = start.min(end);
    
    Ok(Value::Text(chars[start..end].iter().collect()))
}

fn repeat(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    let count = expect_number(args.get(1))? as usize;
    Ok(Value::Text(text.repeat(count)))
}

fn chars(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    let chars: Vec<Value> = text.chars()
        .map(|c| Value::Text(c.to_string()))
        .collect();
    Ok(Value::List(chars))
}

fn lines(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    let lines: Vec<Value> = text.lines()
        .map(|s| Value::Text(s.to_string()))
        .collect();
    Ok(Value::List(lines))
}

fn words(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    let words: Vec<Value> = text.split_whitespace()
        .map(|s| Value::Text(s.to_string()))
        .collect();
    Ok(Value::List(words))
}

fn is_empty(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    Ok(Value::YesNo(text.is_empty()))
}

fn is_numeric(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    Ok(Value::YesNo(text.parse::<f64>().is_ok()))
}

fn pad_start(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    let len = expect_number(args.get(1))? as usize;
    let pad = args.get(2).map(expect_text).unwrap_or(Ok(" ".to_string()))?;
    
    if text.len() >= len {
        return Ok(Value::Text(text));
    }
    
    let padding = pad.repeat(len - text.len());
    Ok(Value::Text(format!("{}{}", padding, text)))
}

fn pad_end(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    let len = expect_number(args.get(1))? as usize;
    let pad = args.get(2).map(expect_text).unwrap_or(Ok(" ".to_string()))?;
    
    if text.len() >= len {
        return Ok(Value::Text(text));
    }
    
    let padding = pad.repeat(len - text.len());
    Ok(Value::Text(format!("{}{}", text, padding)))
}

fn capitalize(args: Vec<Value>) -> Result<Value> {
    let text = expect_text(args.first())?;
    let mut chars = text.chars();
    
    match chars.next() {
        None => Ok(Value::Text(text)),
        Some(first) => {
            let rest: String = chars.collect();
            Ok(Value::Text(format!("{}{}", first.to_uppercase(), rest.to_lowercase())))
        },
    }
}

fn expect_text(value: Option<&Value>) -> Result<String> {
    match value {
        Some(Value::Text(s)) => Ok(s.clone()),
        Some(v) => Err(Error::Runtime(format!("Expected text, got {}", v))),
        None => Err(Error::Runtime("Missing argument".to_string())),
    }
}

fn expect_number(value: Option<&Value>) -> Result<f64> {
    match value {
        Some(Value::Number(n)) => Ok(*n),
        Some(v) => Err(Error::Runtime(format!("Expected number, got {}", v))),
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
