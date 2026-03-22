use crate::error::{Error, Result};
use crate::value::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

pub fn call(function: &str, args: Vec<Value>) -> Result<Value> {
    match function {
        "read" | "read_file" => read(args),
        "write" | "write_file" => write(args),
        "append" => append(args),
        "exists" => exists(args),
        "delete" | "remove" => delete(args),
        "copy" => copy(args),
        "move" => move_file(args),
        "rename" => rename(args),
        "mkdir" => mkdir(args),
        "rmdir" => rmdir(args),
        "list" | "ls" => list(args),
        "info" | "stat" => info(args),
        "is_file" | "isFile" => is_file(args),
        "is_dir" | "isDirectory" => is_dir(args),
        "is_empty" | "isEmpty" => is_empty(args),
        "size" => size(args),
        "extension" => extension(args),
        "basename" => basename(args),
        "dirname" => dirname(args),
        "join" => join(args),
        "read_lines" | "readLines" => read_lines(args),
        "write_lines" | "writeLines" => write_lines(args),
        _ => Err(Error::Runtime(format!("Unknown files function '{}'", function))),
    }
}

fn read(args: Vec<Value>) -> Result<Value> {
    let path = expect_text(args.first())?;
    
    fs::read_to_string(&path)
        .map(Value::Text)
        .map_err(|e| Error::Runtime(format!("Could not read file '{}': {}", path, e)))
}

fn write(args: Vec<Value>) -> Result<Value> {
    let path = expect_text(args.first())?;
    let content = expect_text(args.get(1))?;
    
    fs::write(&path, content)
        .map(|_| Value::Nothing)
        .map_err(|e| Error::Runtime(format!("Could not write file '{}': {}", path, e)))
}

fn append(args: Vec<Value>) -> Result<Value> {
    let path = expect_text(args.first())?;
    let content = expect_text(args.get(1))?;
    
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| Error::Runtime(format!("Could not open file '{}': {}", path, e)))?;
    
    file.write_all(content.as_bytes())
        .map_err(|e| Error::Runtime(format!("Could not append to file '{}': {}", path, e)))?;
    
    Ok(Value::Nothing)
}

fn exists(args: Vec<Value>) -> Result<Value> {
    let path = expect_text(args.first())?;
    Ok(Value::YesNo(Path::new(&path).exists()))
}

fn delete(args: Vec<Value>) -> Result<Value> {
    let path = expect_text(args.first())?;
    
    if Path::new(&path).is_dir() {
        fs::remove_dir_all(&path)
    } else {
        fs::remove_file(&path)
    }
    .map(|_| Value::Nothing)
    .map_err(|e| Error::Runtime(format!("Could not delete '{}': {}", path, e)))
}

fn copy(args: Vec<Value>) -> Result<Value> {
    let from = expect_text(args.first())?;
    let to = expect_text(args.get(1))?;
    
    fs::copy(&from, &to)
        .map(|_| Value::Nothing)
        .map_err(|e| Error::Runtime(format!("Could not copy '{}' to '{}': {}", from, to, e)))
}

fn move_file(args: Vec<Value>) -> Result<Value> {
    let from = expect_text(args.first())?;
    let to = expect_text(args.get(1))?;
    
    fs::rename(&from, &to)
        .map(|_| Value::Nothing)
        .map_err(|e| Error::Runtime(format!("Could not move '{}' to '{}': {}", from, to, e)))
}

fn rename(args: Vec<Value>) -> Result<Value> {
    move_file(args)
}

fn mkdir(args: Vec<Value>) -> Result<Value> {
    let path = expect_text(args.first())?;
    let parents = args.get(1).map(expect_bool).unwrap_or(false);
    
    if parents {
        fs::create_dir_all(&path)
    } else {
        fs::create_dir(&path)
    }
    .map(|_| Value::Nothing)
    .map_err(|e| Error::Runtime(format!("Could not create directory '{}': {}", path, e)))
}

fn rmdir(args: Vec<Value>) -> Result<Value> {
    let path = expect_text(args.first())?;
    
    fs::remove_dir(&path)
        .map(|_| Value::Nothing)
        .map_err(|e| Error::Runtime(format!("Could not remove directory '{}': {}", path, e)))
}

fn list(args: Vec<Value>) -> Result<Value> {
    let path = args.first()
        .map(expect_text)
        .unwrap_or(Ok(".".to_string()))?;
    
    let entries: Result<Vec<Value>, _> = fs::read_dir(&path)
        .map_err(|e| Error::Runtime(format!("Could not read directory '{}': {}", path, e)))?
        .filter_map(|entry| entry.ok())
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            Ok(Value::Text(name))
        })
        .collect();
    
    Ok(Value::List(entries?))
}

fn info(args: Vec<Value>) -> Result<Value> {
    let path = expect_text(args.first())?;
    
    let metadata = fs::metadata(&path)
        .map_err(|e| Error::Runtime(format!("Could not get info for '{}': {}", path, e)))?;
    
    let mut record = HashMap::new();
    record.insert("size".to_string(), Value::Number(metadata.len() as f64));
    record.insert("is_file".to_string(), Value::YesNo(metadata.is_file()));
    record.insert("is_dir".to_string(), Value::YesNo(metadata.is_dir()));
    record.insert("is_symlink".to_string(), Value::YesNo(metadata.file_type().is_symlink()));
    
    if let Ok(modified) = metadata.modified() {
        let duration = modified.duration_since(std::time::UNIX_EPOCH).unwrap();
        record.insert("modified".to_string(), Value::Number(duration.as_secs() as f64));
    }
    
    Ok(Value::Record(record))
}

fn is_file(args: Vec<Value>) -> Result<Value> {
    let path = expect_text(args.first())?;
    Ok(Value::YesNo(Path::new(&path).is_file()))
}

fn is_dir(args: Vec<Value>) -> Result<Value> {
    let path = expect_text(args.first())?;
    Ok(Value::YesNo(Path::new(&path).is_dir()))
}

fn is_empty(args: Vec<Value>) -> Result<Value> {
    let path = expect_text(args.first())?;
    let path = Path::new(&path);
    
    if path.is_dir() {
        match fs::read_dir(path) {
            Ok(mut entries) => Ok(Value::YesNo(entries.next().is_none())),
            Err(_) => Ok(Value::YesNo(false)),
        }
    } else if path.is_file() {
        match fs::metadata(path) {
            Ok(meta) => Ok(Value::YesNo(meta.len() == 0)),
            Err(_) => Ok(Value::YesNo(false)),
        }
    } else {
        Ok(Value::YesNo(false))
    }
}

fn size(args: Vec<Value>) -> Result<Value> {
    let path = expect_text(args.first())?;
    
    let metadata = fs::metadata(&path)
        .map_err(|e| Error::Runtime(format!("Could not get size of '{}': {}", path, e)))?;
    
    Ok(Value::Number(metadata.len() as f64))
}

fn extension(args: Vec<Value>) -> Result<Value> {
    let path = expect_text(args.first())?;
    
    match Path::new(&path).extension() {
        Some(ext) => Ok(Value::Text(ext.to_string_lossy().to_string())),
        None => Ok(Value::Nothing),
    }
}

fn basename(args: Vec<Value>) -> Result<Value> {
    let path = expect_text(args.first())?;
    
    match Path::new(&path).file_name() {
        Some(name) => Ok(Value::Text(name.to_string_lossy().to_string())),
        None => Ok(Value::Text(path)),
    }
}

fn dirname(args: Vec<Value>) -> Result<Value> {
    let path = expect_text(args.first())?;
    
    match Path::new(&path).parent() {
        Some(parent) => Ok(Value::Text(parent.to_string_lossy().to_string())),
        None => Ok(Value::Text(".".to_string())),
    }
}

fn join(args: Vec<Value>) -> Result<Value> {
    if args.is_empty() {
        return Ok(Value::Text("".to_string()));
    }
    
    let parts: Vec<String> = args.iter()
        .map(|v| expect_text(Some(v)))
        .collect::<Result<Vec<_>>>()?;
    
    let path = parts.join(std::path::MAIN_SEPARATOR_STR);
    Ok(Value::Text(path))
}

fn read_lines(args: Vec<Value>) -> Result<Value> {
    let path = expect_text(args.first())?;
    
    let content = fs::read_to_string(&path)
        .map_err(|e| Error::Runtime(format!("Could not read file '{}': {}", path, e)))?;
    
    let lines: Vec<Value> = content.lines()
        .map(|s| Value::Text(s.to_string()))
        .collect();
    
    Ok(Value::List(lines))
}

fn write_lines(args: Vec<Value>) -> Result<Value> {
    let path = expect_text(args.first())?;
    let lines = expect_list(args.get(1))?;
    
    let content: Vec<&str> = lines.iter()
        .map(|v| v.to_string().as_str())
        .collect();
    
    fs::write(&path, content.join("\n"))
        .map(|_| Value::Nothing)
        .map_err(|e| Error::Runtime(format!("Could not write file '{}': {}", path, e)))
}

fn expect_text(value: Option<&Value>) -> Result<String> {
    match value {
        Some(Value::Text(s)) => Ok(s.clone()),
        Some(v) => Err(Error::Runtime(format!("Expected text, got {}", v))),
        None => Err(Error::Runtime("Missing argument".to_string())),
    }
}

fn expect_bool(value: Option<&Value>) -> Result<bool> {
    match value {
        Some(Value::YesNo(b)) => Ok(*b),
        Some(v) => Err(Error::Runtime(format!("Expected yes/no, got {}", v))),
        None => Ok(false),
    }
}

fn expect_list(value: Option<&Value>) -> Result<Vec<Value>> {
    match value {
        Some(Value::List(items)) => Ok(items.clone()),
        Some(v) => Err(Error::Runtime(format!("Expected list, got {}", v))),
        None => Err(Error::Runtime("Missing argument".to_string())),
    }
}
