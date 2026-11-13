//! Projects: `rb new`, walk-up root discovery, and a project's own tests.
//!
//! A Redblue program is a file; a Redblue *project* is a directory carrying a
//! `redblue.manifest` (the schema phase-044 owns), an entry point under `src/`,
//! and its tests under `tests/`. This module is the three things that turn one
//! into the other: [`create_project`] writes the layout, [`find_project_root`]
//! finds it from anywhere inside it, and [`check_manifest`] refuses a manifest
//! that does not parse before a command acts on the project.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::error::{Error, Result};
use crate::manifest::{is_module_name, Manifest, MANIFEST_FILE};

/// The directory holding a project's entry point.
pub const SOURCE_DIR: &str = "src";

/// The directory holding a project's tests.
pub const TESTS_DIR: &str = "tests";

/// The entry point `rb new` writes.
pub const ENTRY_FILE: &str = "src/main.rb";

/// The test file `rb new` writes.
pub const TEST_FILE: &str = "tests/main_test.rb";

/// The version `rb new` gives a fresh project.
pub const DEFAULT_VERSION: &str = "0.1.0";

/// The root of the project containing `start`, or `None` when there is none.
///
/// A project root is the nearest directory at or above `start` holding a
/// `redblue.manifest`. The walk follows every ancestor to the filesystem root,
/// so a directory nested inside a project finds that project. A directory is
/// outside every project only when neither it nor any ancestor holds a
/// manifest; then the answer is `None`. `start` may be a directory or a file;
/// a relative `start` is resolved against the working directory and the path is
/// canonicalised (or lexically cleaned when it does not exist), so `..`
/// components walk toward the parent rather than being treated as a child.
pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    let absolute = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(start)
    };
    let start = fs::canonicalize(&absolute).unwrap_or_else(|_| clean_path(&absolute));
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start
    };
    loop {
        if current.join(MANIFEST_FILE).is_file() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Removes `.` and resolves `..` in `path` without touching the filesystem.
///
/// [`find_project_root`] prefers [`fs::canonicalize`]; this is the fallback for
/// a path that does not exist yet, where `PathBuf::pop` would otherwise remove
/// `..` as if it were a child directory name.
fn clean_path(path: &Path) -> PathBuf {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => clean.push(prefix.as_os_str()),
            Component::RootDir => clean.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !clean.pop() && !path.is_absolute() {
                    clean.push("..");
                }
            }
            Component::Normal(part) => clean.push(part),
        }
    }
    clean
}

/// Creates a project named `name` under `parent` and returns its root.
///
/// The name is the directory and the manifest's `name`, so it must be an
/// identifier the manifest accepts: a name carrying a path separator would
/// create a directory somewhere other than `parent`, which is never what
/// `rb new` was asked for.
///
/// An existing directory is refused when it already holds anything. Writing
/// into occupied ground would either overwrite a file the user owns or merge a
/// second project into a directory that is not empty, and neither is something
/// a scaffold should do silently. An existing *empty* directory is accepted: it
/// is not occupied ground.
pub fn create_project(parent: &Path, name: &str) -> Result<PathBuf> {
    check_project_name(name)?;

    let root = parent.join(name);
    if root.exists() {
        if !root.is_dir() {
            return Err(Error::Io(format!(
                "Cannot create project '{}': {} already exists and is not a directory",
                name,
                root.display()
            )));
        }
        let mut entries = fs::read_dir(&root)
            .map_err(|error| Error::Io(format!("Cannot read '{}': {}", root.display(), error)))?;
        if entries.next().is_some() {
            return Err(Error::Io(format!(
                "Cannot create project '{}': {} already exists and is not empty",
                name,
                root.display()
            )));
        }
    }

    let source = root.join(SOURCE_DIR);
    let tests = root.join(TESTS_DIR);
    fs::create_dir_all(&source)
        .map_err(|error| Error::Io(format!("Cannot create '{}': {}", source.display(), error)))?;
    fs::create_dir_all(&tests)
        .map_err(|error| Error::Io(format!("Cannot create '{}': {}", tests.display(), error)))?;

    // The manifest is written last: a discovery walk finds a project by its
    // manifest, so a half-written layout must not become one.
    write(
        &root.join(ENTRY_FILE),
        &format!("say \"Hello from {name}\"\n"),
    )?;
    write(
        &root.join(TEST_FILE),
        &format!("test \"{name}\"\n    expect 1 + 1 to be 2\nend\n"),
    )?;
    write(
        &root.join(MANIFEST_FILE),
        &format!("name {name}\nversion {DEFAULT_VERSION}\n"),
    )?;

    Ok(root)
}

/// Parses the manifest of the project in `root`, naming the file and line when
/// it does not parse.
///
/// Discovery only proves the file is there. A command about to run a project's
/// tests has to know the project is one it can read, and the manifest's own
/// parser is the single authority on that schema.
pub fn check_manifest(root: &Path) -> Result<()> {
    let path = root.join(MANIFEST_FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(Error::Io(format!(
                "Cannot read '{}': {}",
                path.display(),
                error
            )))
        }
    };
    Manifest::parse(&text, &path.display().to_string()).map(|_| ())
}

/// The `tests/` directory of the project in `root`.
pub fn tests_dir(root: &Path) -> PathBuf {
    root.join(TESTS_DIR)
}

/// Refuses a name that is not a manifest module name.
fn check_project_name(name: &str) -> Result<()> {
    if is_module_name(name) {
        return Ok(());
    }
    Err(Error::Runtime(
        format!(
            "'{name}' is not a project name: a project name starts with a letter or an \
             underscore and holds only letters, digits and underscores, with no path \
             separator"
        ),
        crate::error::Span::unknown(),
    ))
}

fn write(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents)
        .map_err(|error| Error::Io(format!("Cannot write '{}': {}", path.display(), error)))
}
