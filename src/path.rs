//! The `path` module: the OS's idea of a path, rather than string arithmetic.
//!
//! Every program that touched the filesystem used to build a path by writing
//! `a + "/" + b`. That is wrong on Windows, where the separator is `\`, and it
//! has no answer for "what is the parent of this file", "what is its extension"
//! or "what is in this directory". Those are questions the host already answers,
//! so this module asks `std::path` rather than growing a second, platform-blind
//! path grammar in Redblue.
//!
//! A path value is an ordinary object named `path` with one field, `text`. It is
//! deliberately not a new [`Value`] variant: the language's value kinds are part
//! of its public API, and a path is a text with a platform-aware set of
//! operations on it. It prints as its text and compares equal to that text, so
//! `say path.join("a", "b")` and `path.join("a", "b") is "a/b"` both read the way
//! a reader expects, while `type_of` can still say `path`.
//!
//! `files.read` and its siblings accept either spelling — the text every existing
//! program passes, or the value this module answers — because the corpus and the
//! language's examples are written against the string form.

use std::path::{Component, Path, PathBuf};

use crate::error::{Error, Result, Span};
use crate::value::{Fields, Value};

/// The object name a path value carries, so both engines can recognise one.
pub const OBJECT_NAME: &str = "path";

/// The one field a path value holds: its text.
pub const TEXT_FIELD: &str = "text";

/// Wraps `text` as a path value.
pub fn value(text: impl Into<String>) -> Value {
    let mut fields = Fields::new();
    fields.insert(TEXT_FIELD.to_string(), Value::Text(text.into()));
    Value::object(OBJECT_NAME.to_string(), fields)
}

/// The text of a path value, or of a plain text path.
///
/// This is the one conversion every `files.*` call and every `path.*` argument
/// goes through, so the string form and the value form cannot answer differently.
pub fn text_of(value: &Value) -> Option<&str> {
    match value {
        Value::Text(text) => Some(text),
        Value::Object(name, fields) if name == OBJECT_NAME => match fields.get(TEXT_FIELD) {
            Some(Value::Text(text)) => Some(text),
            _ => None,
        },
        _ => None,
    }
}

/// Whether `value` is a path value rather than a text.
pub fn is_path(value: &Value) -> bool {
    matches!(value, Value::Object(name, _) if name == OBJECT_NAME)
}

/// Joins `parts` with the platform's separator.
///
/// An absolute part discards everything joined before it. That is the rule every
/// host path API follows, and it is what makes `path.join(root, child)` safe when
/// `child` came from somewhere that may already be absolute.
pub fn join(parts: &[String]) -> String {
    let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
    join_with_separator(&parts, std::path::MAIN_SEPARATOR)
}

/// Joins `parts` with `separator`, for both platforms and for tests that have to
/// ask what the other one would answer.
///
/// On Windows the separator is `\`, and a `/` in a part is folded to it so a
/// program that wrote a path with slashes still gets one separator. On POSIX a
/// `\` is an ordinary character in a filename, so it is left alone.
pub fn join_with_separator(parts: &[&str], separator: char) -> String {
    let mut joined = String::new();

    for raw in parts {
        if raw.is_empty() {
            continue;
        }
        let mut part = if separator == '\\' {
            raw.replace('/', "\\")
        } else {
            (*raw).to_string()
        };
        let absolute = is_absolute_with(&part, separator);

        if let Some(root) = root_with(&part, separator) {
            joined.clear();
            joined.push_str(&root);
            continue;
        }

        while part.len() > 1 && part.ends_with(separator) {
            part.pop();
        }

        if absolute {
            joined.clear();
        } else if !joined.is_empty() && !joined.ends_with(separator) {
            joined.push(separator);
        }

        if joined.ends_with(separator) && part.starts_with(separator) {
            joined.push_str(part.trim_start_matches(separator));
        } else {
            joined.push_str(&part);
        }
    }

    joined
}

/// The canonical root `part` names under `separator`, if it is only a root.
///
/// A root is more than an absolute path: joining a root has to keep its
/// separator, or `path.join("C:\\")` becomes `"C:"`, which is a drive-relative
/// path and no longer absolute. The separator is therefore preserved before
/// the ordinary trailing-separator trim, which is what a bare `/` and a drive
/// root like `C:\` have in common.
fn root_with(part: &str, separator: char) -> Option<String> {
    if separator == '\\' {
        let bytes = part.as_bytes();
        if bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'\\'
            && part[2..].chars().all(|c| c == '\\')
        {
            return Some(format!("{}:\\", bytes[0] as char));
        }
        if !part.is_empty() && part.chars().all(|c| c == '\\') {
            return Some("\\".to_string());
        }
        None
    } else if !part.is_empty() && part.chars().all(|c| c == '/') {
        Some("/".to_string())
    } else {
        None
    }
}

/// Whether `part` is absolute under `separator`.
fn is_absolute_with(part: &str, separator: char) -> bool {
    if separator == '\\' {
        let bytes = part.as_bytes();
        part.starts_with('\\')
            || (bytes.len() >= 3
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && bytes[2] == b'\\')
    } else {
        part.starts_with('/')
    }
}

/// The directory holding `text`, or `None` for a root and for a bare filename.
pub fn parent(text: &str) -> Option<String> {
    Path::new(text)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| parent.to_string_lossy().into_owned())
}

/// The last component of `text`, ignoring trailing separators.
///
/// `filename("data/records/")` is `records`: a trailing separator says the path
/// names a directory, not that it has no name. A root has no filename.
pub fn filename(text: &str) -> Option<String> {
    Path::new(text)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

/// The filename without its last extension.
pub fn stem(text: &str) -> Option<String> {
    Path::new(text)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
}

/// The last extension, without its dot. A leading dot is part of the name, so
/// `.gitignore` has no extension.
pub fn extension(text: &str) -> Option<String> {
    Path::new(text)
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned())
}

/// Whether `text` is absolute according to the host.
pub fn is_absolute(text: &str) -> bool {
    Path::new(text).is_absolute()
}

/// Makes `text` absolute against the working directory and removes `.` and `..`
/// segments.
///
/// This is lexical: it does not consult the filesystem, so it answers for a path
/// that does not exist yet and does not resolve symlinks. That is the useful
/// answer for building a path before creating it, and it is what SPEC.md states.
pub fn resolve(text: &str) -> std::io::Result<String> {
    let path = Path::new(text);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(normalize(&absolute).to_string_lossy().into_owned())
}

/// `path` with `.` removed and `..` folded into its parent where possible.
fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if normalized.file_name().is_some() {
                    normalized.pop();
                } else if !normalized.has_root() {
                    normalized.push(component.as_os_str());
                }
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

/// The entries directly inside `text`, sorted by name.
pub fn list(text: &str) -> std::io::Result<Vec<String>> {
    crate::vfs::read_dir(text)
}

/// Answers one `path.*` call.
///
/// Both VMs reach this through [`crate::runtime::builtin`], so a path cannot
/// answer in one engine and be refused by the other.
pub fn builtin(span: Span, name: &str, args: &[Value]) -> Result<Value> {
    match name {
        "path_join" => {
            if args.is_empty() {
                return Err(Error::Runtime(
                    "path.join requires at least one path".to_string(),
                    span,
                ));
            }
            let mut parts = Vec::with_capacity(args.len());
            for arg in args {
                let Some(text) = text_of(arg) else {
                    return Err(Error::Runtime(
                        "path.join requires text paths or path values".to_string(),
                        span,
                    ));
                };
                parts.push(text.to_string());
            }
            Ok(value(join(&parts)))
        }
        "path_parent" => {
            let text = one_path(span, "path.parent", args)?;
            Ok(match parent(&text) {
                Some(parent) => value(parent),
                None => Value::Nothing,
            })
        }
        "path_filename" => {
            let text = one_path(span, "path.filename", args)?;
            Ok(filename(&text).map(Value::Text).unwrap_or(Value::Nothing))
        }
        "path_stem" => {
            let text = one_path(span, "path.stem", args)?;
            Ok(stem(&text).map(Value::Text).unwrap_or(Value::Nothing))
        }
        "path_extension" => {
            let text = one_path(span, "path.extension", args)?;
            Ok(extension(&text).map(Value::Text).unwrap_or(Value::Nothing))
        }
        "path_is_absolute" => {
            let text = one_path(span, "path.is_absolute", args)?;
            Ok(Value::YesNo(is_absolute(&text)))
        }
        "path_resolve" => {
            let text = one_path(span, "path.resolve", args)?;
            resolve(&text)
                .map(value)
                .map_err(|error| Error::Io(format!("Failed to resolve '{text}': {error}")))
        }
        "path_list" => {
            let text = one_path(span, "path.list", args)?;
            let entries = list(&text)
                .map_err(|error| Error::Io(format!("Failed to list '{text}': {error}")))?;
            Ok(Value::list(entries.into_iter().map(Value::Text).collect()))
        }
        _ => Err(Error::Runtime(
            format!("Unknown path function '{name}'"),
            span,
        )),
    }
}

/// The one path argument `name` takes, as text.
fn one_path(span: Span, name: &str, args: &[Value]) -> Result<String> {
    match args {
        [arg] => text_of(arg)
            .map(str::to_string)
            .ok_or_else(|| Error::Runtime(format!("{name} requires a text path"), span)),
        _ => Err(Error::Runtime(
            format!("{name} requires one text path"),
            span,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_join_with_each_separator_is_distinct() {
        assert_eq!(join_with_separator(&["a", "b"], '/'), "a/b");
        assert_eq!(join_with_separator(&["a", "b"], '\\'), "a\\b");
    }

    #[test]
    fn edge_joining_a_drive_root_keeps_it_absolute() {
        assert_eq!(join_with_separator(&["C:\\"], '\\'), "C:\\");
        assert_eq!(join_with_separator(&["C:\\", ""], '\\'), "C:\\");
        assert_eq!(join_with_separator(&["C:\\", "Users"], '\\'), "C:\\Users");
        assert_eq!(join_with_separator(&["D:/"], '\\'), "D:\\");
        assert!(is_absolute_with("C:\\", '\\'));
        assert_eq!(join_with_separator(&["/"], '/'), "/");
        assert_eq!(join_with_separator(&["/", "tmp"], '/'), "/tmp");
    }

    #[test]
    fn edge_resolve_removes_dot_segments() {
        let resolved = resolve("a/./b/../c").expect("the working directory is readable");
        assert!(!resolved.contains("/./"));
        assert!(!resolved.contains("/../"));
        assert!(resolved.ends_with("c"));
    }
}
