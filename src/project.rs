//! Projects: `rb new`, walk-up root discovery, and a project's own tests.
//!
//! A Redblue program is a file; a Redblue *project* is a directory carrying a
//! `redblue.manifest` (the schema phase-044 owns), an entry point under `src/`,
//! and its tests under `tests/`. This module is the three things that turn one
//! into the other: [`create_project`] writes the layout, [`find_project_root`]
//! finds it from anywhere inside it, and [`check_manifest`] refuses a manifest
//! that does not parse before a command acts on the project.

use std::fs;
use std::io::ErrorKind;
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
/// `redblue.manifest` entry. The walk follows every ancestor to the filesystem
/// root, so a directory nested inside a project finds that project. A directory
/// is outside every project only when neither it nor any ancestor holds a
/// manifest entry; then the answer is `None`. Discovery stops at an entry even
/// when it is a symlink or a directory, leaving [`check_manifest`] to refuse it:
/// walking past an unusable manifest would silently run an ancestor project's
/// suite instead. `start` may be a directory or a file;
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
        if has_manifest(&current) {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Whether `dir` holds an entry named like a manifest.
///
/// Discovery must not decide whether that entry is usable: a symlink or a
/// directory named `redblue.manifest` is still the nearest project marker, and
/// walking past it would silently run an *ancestor* project's suite. Stop here
/// and let [`check_manifest`] classify the entry and refuse it with a message
/// naming the manifest.
fn has_manifest(dir: &Path) -> bool {
    match fs::symlink_metadata(dir.join(MANIFEST_FILE)) {
        Ok(_) => true,
        Err(error) => error.kind() != ErrorKind::NotFound,
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
///
/// A symbolic link in the project's place is always refused, even when it
/// points at an empty directory or at nothing at all. Following it would write
/// the scaffold somewhere other than `parent`, which is the same escape a path
/// separator would allow and is never what `rb new` was asked for.
pub fn create_project(parent: &Path, name: &str) -> Result<PathBuf> {
    check_project_name(name)?;

    let root = parent.join(name);
    let root_metadata = match fs::symlink_metadata(&root) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => {
            return Err(Error::Io(format!(
                "Cannot inspect '{}': {}",
                root.display(),
                error
            )))
        }
    };
    let root_existed = root_metadata.is_some();
    if let Some(metadata) = root_metadata {
        if metadata.file_type().is_symlink() {
            return Err(Error::Io(format!(
                "Cannot create project '{}': {} already exists and is a symbolic link",
                name,
                root.display()
            )));
        }
        if !metadata.is_dir() {
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

    let result = (|| -> Result<()> {
        fs::create_dir_all(&source).map_err(|error| {
            Error::Io(format!("Cannot create '{}': {}", source.display(), error))
        })?;
        fs::create_dir_all(&tests).map_err(|error| {
            Error::Io(format!("Cannot create '{}': {}", tests.display(), error))
        })?;

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

        Ok(())
    })();

    if let Err(error) = result {
        // A failed write must not leave a partial layout behind. The next
        // `rb new` refuses a non-empty directory, so leftover `src/` or
        // `tests/` would turn a transient failure into a permanent one.
        cleanup_scaffold(&root, root_existed);
        return Err(error);
    }

    Ok(root)
}

/// Removes what [`create_project`] wrote before a failure.
///
/// When the project directory already existed (and was empty), only the entries
/// this call created are removed, leaving the caller's empty directory in place
/// so a retry is accepted. When this call created the directory too, removing it
/// whole is what makes the retry possible.
fn cleanup_scaffold(root: &Path, root_existed: bool) {
    if root_existed {
        let _ = fs::remove_file(root.join(MANIFEST_FILE));
        let _ = fs::remove_dir_all(root.join(SOURCE_DIR));
        let _ = fs::remove_dir_all(root.join(TESTS_DIR));
    } else {
        let _ = fs::remove_dir_all(root);
    }
}

/// Parses the manifest of the project in `root`, naming the file and line when
/// it does not parse.
///
/// Discovery only proves the file is there. A command about to run a project's
/// tests has to know the project is one it can read, and the manifest's own
/// parser is the single authority on that schema. A directory with no manifest
/// is refused too: it is not a project, so treating it as one would be a silent
/// success for a caller that never checked for the file.
pub fn check_manifest(root: &Path) -> Result<()> {
    let path = root.join(MANIFEST_FILE);
    // Discovery stops at any entry named like a manifest, including a link or
    // a directory, so this is where it is classified. A caller can also reach
    // this function with a root of its own. Inspect the entry itself rather
    // than letting `read_to_string` follow a link out of the project.
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| Error::Io(format!("Cannot inspect '{}': {}", path.display(), error)))?;
    if metadata.file_type().is_symlink() {
        return Err(Error::Io(format!(
            "Cannot read '{}': it is a symbolic link, not a regular file",
            path.display()
        )));
    }
    if !metadata.file_type().is_file() {
        return Err(Error::Io(format!(
            "Cannot read '{}': it is not a regular file",
            path.display()
        )));
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| Error::Io(format!("Cannot read '{}': {}", path.display(), error)))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/tmp/project-cleanup-test")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("cleanup scratch dir should be creatable");
        dir
    }

    /// A failure after this call created the project directory must remove the
    /// whole partial layout, so a retry does not find it occupying the name.
    #[test]
    fn cleanup_scaffold_removes_a_root_created_by_the_failed_attempt() {
        let parent = scratch("created-root");
        let root = parent.join("hello");
        fs::create_dir_all(root.join(SOURCE_DIR)).expect("fixture src");
        fs::create_dir_all(root.join(TESTS_DIR)).expect("fixture tests");
        fs::write(root.join(SOURCE_DIR).join("main.rb"), "partial").expect("fixture entry");
        fs::write(root.join(MANIFEST_FILE), "partial").expect("fixture manifest");

        cleanup_scaffold(&root, false);

        assert!(
            !root.exists(),
            "a project directory created by the failed scaffold must be removed: {}",
            root.display()
        );
    }

    /// A pre-existing empty directory must survive cleanup and be left empty,
    /// which is what lets the caller retry the scaffold in the same place.
    #[test]
    fn cleanup_scaffold_leaves_a_pre_existing_directory_empty() {
        let parent = scratch("existing-root");
        let root = parent.join("hello");
        fs::create_dir_all(&root).expect("fixture root");
        fs::create_dir_all(root.join(SOURCE_DIR)).expect("fixture src");
        fs::create_dir_all(root.join(TESTS_DIR)).expect("fixture tests");
        fs::write(root.join(SOURCE_DIR).join("main.rb"), "partial").expect("fixture entry");
        fs::write(root.join(MANIFEST_FILE), "partial").expect("fixture manifest");

        cleanup_scaffold(&root, true);

        assert!(
            root.is_dir(),
            "the pre-existing directory must not be removed: {}",
            root.display()
        );
        assert_eq!(
            fs::read_dir(&root).expect("fixture root").count(),
            0,
            "cleanup must remove every entry the failed scaffold wrote"
        );
    }
}
