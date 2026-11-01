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

use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Component, PathBuf};

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

/// The text of the object `name` with `fields`, if that object is a path value.
///
/// A path value has exactly one field, `text`. An object that shares the name
/// but carries another field is an ordinary object, not a path: treating it as
/// a path would make it equal to its text while remaining unequal to a real
/// path value, which breaks the transitivity `collections.counter` and
/// `defaultdict` depend on.
pub fn object_text<'a>(name: &str, fields: &'a Fields) -> Option<&'a str> {
    if name != OBJECT_NAME || fields.len() != 1 {
        return None;
    }
    match fields.get(TEXT_FIELD) {
        Some(Value::Text(text)) => Some(text),
        _ => None,
    }
}

/// The text of a path value, or of a plain text path.
///
/// This is the one conversion every `files.*` call and every `path.*` argument
/// goes through, so the string form and the value form cannot answer differently.
pub fn text_of(value: &Value) -> Option<&str> {
    match value {
        Value::Text(text) => Some(text),
        Value::Object(name, fields) => object_text(name, fields),
        _ => None,
    }
}

/// Whether `value` is a path value rather than a text.
pub fn is_path(value: &Value) -> bool {
    matches!(value, Value::Object(name, fields) if object_text(name, fields).is_some())
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

        // A Windows prefix with no root, such as `D:other`, replaces the whole
        // path before it, exactly as `PathBuf::push` does. It is not absolute
        // (the drive is relative to that drive's own working directory), and it
        // is not root-relative either (the drive itself is a new prefix), so
        // neither of the branches below would do the right thing with it.
        if separator == '\\' {
            let prefix = windows_prefix_len(&part);
            if prefix > 0 && part.as_bytes().get(prefix) != Some(&b'\\') {
                joined.clear();
                joined.push_str(&part);
                continue;
            }
        }

        if let Some(root) = root_with(&part, separator) {
            if separator == '\\' && root == "\\" {
                let prefix = windows_prefix_len(&joined);
                joined.truncate(prefix);
            } else {
                joined.clear();
            }
            joined.push_str(&root);
            continue;
        }

        while part.len() > 1 && part.ends_with(separator) {
            part.pop();
        }

        if absolute {
            if separator == '\\' && part.starts_with('\\') && windows_prefix_len(&part) == 0 {
                let prefix = windows_prefix_len(&joined);
                joined.truncate(prefix);
            } else {
                joined.clear();
            }
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
        // A UNC share is a prefix, not a root, until a separator follows it.
        // Keeping that separator is what makes `\\server\share\` absolute; the
        // ordinary trailing-separator trim would otherwise leave only the
        // prefix `\\server\share`, which names no root at all.
        let prefix = windows_prefix_len(part);
        if prefix > 2
            && bytes.get(prefix) == Some(&b'\\')
            && part[prefix..].chars().all(|c| c == '\\')
        {
            return Some(format!("{}\\", &part[..prefix]));
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

/// The length of the Windows prefix in `path`, or zero when it has none.
///
/// A prefix is a drive such as `C:` or a UNC share such as `\\server\share`.
/// It is what `PathBuf::push` keeps when the part being joined is rooted but
/// carries no prefix of its own: joining `C:\base` and `\other` is
/// `C:\other`, not `\other`, because the drive belongs to the path before the
/// root-relative part rather than to that part.
fn windows_prefix_len(path: &str) -> usize {
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return 2;
    }
    let is_separator = |byte: u8| byte == b'\\' || byte == b'/';
    if bytes.len() >= 2 && is_separator(bytes[0]) && is_separator(bytes[1]) {
        let rest = &path[2..];
        let mut parts = rest.splitn(3, ['\\', '/']);
        let server = parts.next().unwrap_or("");
        let share = parts.next().unwrap_or("");
        if !server.is_empty() && !share.is_empty() {
            return 2 + server.len() + 1 + share.len();
        }
    }
    0
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
#[cfg(not(target_arch = "wasm32"))]
pub fn resolve(text: &str) -> std::io::Result<String> {
    let path = Path::new(text);
    #[cfg(windows)]
    if has_prefix_without_root(text) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "cannot resolve '{text}': it has a Windows prefix but no root, so it is \
                 relative to that drive's own working directory; use a rooted path instead"
            ),
        ));
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(normalize(&absolute).to_string_lossy().into_owned())
}

/// Whether `text` carries a Windows prefix but no root after it.
///
/// A drive-relative path such as `C:foo` has a drive but no separator, so it
/// names a file relative to that drive's current directory rather than to the
/// process's. `PathBuf::push` would truncate the working directory to `C:foo`
/// and leave it relative; [`resolve`] refuses it instead of answering a path
/// that is not absolute. The same applies to a UNC share with no trailing
/// separator, which is a prefix without a root.
///
/// A root may be spelled with either separator: Windows accepts `C:/folder`
/// as readily as `C:\folder`, and refusing the forward-slash spelling would
/// turn a rooted path into an error. The helper is compiled under `test` on
/// every platform so the host suite can pin both spellings; the `resolve`
/// caller is still Windows-only because that is where the rule applies.
#[cfg(any(windows, test))]
fn has_prefix_without_root(text: &str) -> bool {
    let prefix = windows_prefix_len(text);
    prefix > 0 && !matches!(text.as_bytes().get(prefix), Some(b'\\') | Some(b'/'))
}

/// The refusal the WebAssembly playground gives for [`resolve`].
///
/// It is a function the host build also compiles under `test`, so `cargo test`
/// can assert the kind and wording of the refusal a wasm program receives. A
/// wasm-only `Err` would be compiled only for a target this checkout's test
/// suite does not build, leaving the playground's promise unpinned.
#[cfg(any(target_arch = "wasm32", test))]
fn no_working_directory() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "path.resolve cannot read the working directory in the WebAssembly playground: \
         the platform has none. Run it with the native rb binary to resolve a path.",
    )
}

/// The playground has no working directory, so there is nothing to resolve
/// against. `current_dir()` is not available on `wasm32-unknown-unknown`; a
/// silent empty answer or a host-dependent virtual root would be a path the
/// program cannot read, so the refusal names the platform instead.
#[cfg(target_arch = "wasm32")]
pub fn resolve(_text: &str) -> std::io::Result<String> {
    Err(no_working_directory())
}

/// `path` with `.` removed and `..` folded into its parent where possible.
#[cfg(not(target_arch = "wasm32"))]
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
    fn edge_joining_a_unc_share_root_keeps_it_absolute() {
        let root = r"\\server\share\";
        assert_eq!(
            join_with_separator(&[root], '\\'),
            root,
            "a UNC root must keep the separator that makes it a root"
        );
        assert_eq!(
            join_with_separator(&[r"\\server\share\\"], '\\'),
            root,
            "a UNC root is one root however many separators spell it"
        );
        assert_eq!(
            join_with_separator(&[root, "folder"], '\\'),
            r"\\server\share\folder"
        );
        assert!(
            is_absolute_with(&join_with_separator(&[root], '\\'), '\\'),
            "the joined UNC root must still be absolute"
        );
    }

    #[test]
    fn edge_join_keeps_the_drive_for_a_root_relative_part() {
        assert_eq!(
            join_with_separator(&["C:\\base", "\\other"], '\\'),
            "C:\\other",
            "a root-relative part keeps the drive of the path before it"
        );
        assert_eq!(join_with_separator(&["C:\\base", "\\"], '\\'), "C:\\");
        assert_eq!(
            join_with_separator(&["\\\\server\\share\\base", "\\other"], '\\'),
            "\\\\server\\share\\other",
            "a UNC share is a prefix too"
        );
        assert_eq!(
            join_with_separator(&["a", "\\b"], '\\'),
            "\\b",
            "with no prefix to keep, a root-relative part still replaces the rest"
        );
    }

    #[test]
    fn edge_a_drive_relative_part_replaces_the_path_before_it() {
        assert_eq!(
            join_with_separator(&["C:\\base", "D:other"], '\\'),
            "D:other",
            "a part with a prefix but no root replaces the whole path, as PathBuf::push does"
        );
        assert_eq!(join_with_separator(&["C:\\base", "C:"], '\\'), "C:");
        assert_eq!(
            join_with_separator(&["a", "C:other"], '/'),
            "a/C:other",
            "on POSIX a colon is an ordinary filename character, not a drive prefix"
        );
    }

    #[test]
    fn edge_a_forward_slash_drive_root_is_not_a_prefix_without_a_root() {
        for text in ["C:/folder", "C:\\folder", "C:/", "C:\\"] {
            assert!(
                !has_prefix_without_root(text),
                "`{text}` is rooted, so resolve must not refuse it"
            );
        }
    }

    #[test]
    fn edge_a_drive_without_a_root_is_a_prefix_without_a_root() {
        for text in ["C:", "C:folder", "D:other"] {
            assert!(
                has_prefix_without_root(text),
                "`{text}` has a drive but no root, so resolve must refuse it"
            );
        }
        assert!(
            !has_prefix_without_root("folder"),
            "an ordinary relative path has no prefix to refuse"
        );
    }

    #[test]
    fn edge_a_unc_share_without_a_root_is_a_prefix_without_a_root() {
        for text in [r"\\server\share", "//server/share"] {
            assert!(
                has_prefix_without_root(text),
                "`{text}` is a UNC share with no trailing separator, so it names no root"
            );
        }
        for text in [
            "\\\\server\\share\\",
            r"\\server\share\folder",
            "//server/share/",
            "//server/share/folder",
        ] {
            assert!(
                !has_prefix_without_root(text),
                "`{text}` is rooted, so resolve must not refuse it"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn edge_resolve_accepts_a_forward_slash_drive_root() {
        let resolved = resolve("C:/Windows/..").expect("a rooted forward-slash path is absolute");
        assert!(
            Path::new(&resolved).is_absolute(),
            "resolve must keep a forward-slash drive root absolute, got {resolved}"
        );
        assert_eq!(
            Path::new(&resolved),
            Path::new("C:\\"),
            "resolving the parent of a drive folder must answer the drive root"
        );
    }

    #[test]
    fn edge_the_playground_refuses_resolve_without_a_working_directory() {
        let refusal = no_working_directory();
        assert_eq!(
            refusal.kind(),
            std::io::ErrorKind::Unsupported,
            "a platform with no working directory is Unsupported, not a missing file: {refusal}"
        );
        let message = refusal.to_string();
        assert!(
            message.contains("path.resolve"),
            "the refusal must name the operation the program called: {message}"
        );
        assert!(
            message.contains("WebAssembly"),
            "the refusal must name the platform rather than a host-dependent path: {message}"
        );
        assert!(
            message.contains("working directory"),
            "the refusal must say what the platform lacks: {message}"
        );
        assert!(
            message.contains("native rb binary"),
            "the refusal must say what the reader can run instead: {message}"
        );
    }

    #[test]
    fn edge_resolve_removes_dot_segments() {
        let resolved = resolve("a/./b/../c").expect("the working directory is readable");
        let separator = std::path::MAIN_SEPARATOR;
        let dot = format!("{separator}.{separator}");
        let dotdot = format!("{separator}..{separator}");
        assert!(
            !resolved.contains(&dot),
            "resolve must remove `.` segments on this platform, got {resolved}"
        );
        assert!(
            !resolved.contains(&dotdot),
            "resolve must remove `..` segments on this platform, got {resolved}"
        );
        assert_eq!(
            Path::new(&resolved)
                .file_name()
                .and_then(|name| name.to_str()),
            Some("c")
        );
    }

    #[cfg(windows)]
    #[test]
    fn edge_resolve_refuses_a_drive_relative_path() {
        let error =
            resolve("C:foo").expect_err("a drive-relative path has no root to resolve against");
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::InvalidInput,
            "a drive-relative path is not a missing file: {error}"
        );
        let message = error.to_string();
        assert!(
            message.contains("C:foo"),
            "the refusal must name the path the program passed: {message}"
        );
        assert!(
            message.contains("prefix but no root"),
            "the refusal must say why it cannot be made absolute: {message}"
        );
        assert!(
            message.contains("rooted path"),
            "the refusal must say what the reader can pass instead: {message}"
        );
    }
}
