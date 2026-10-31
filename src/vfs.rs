//! The filesystem the `files` and `bytes` modules talk to.
//!
//! On a native target this is `std::fs`, re-exported name for name — the same
//! functions, the same `std::io::Result`, the same error kinds — so every
//! `files.*` and `bytes.write` call is the code it always was and its error text
//! is unchanged.
//!
//! `wasm32-unknown-unknown` has no filesystem at all: `std::fs` compiles in and
//! every call fails with "operation not supported on this platform". A
//! playground with no file module cannot run `examples/files.rb`, and that
//! example is part of the specification-by-example, so on that target this
//! module supplies a filesystem in memory: a flat map from path to content that
//! lives for the life of the module instance.
//!
//! This is an I/O-boundary difference and nothing else. `files.read` still reads
//! what `files.write` wrote, `files.exists` still answers about a file another
//! call created — and about the directory that file sits in, which the native
//! `Path::exists` answers for and this has to answer for too — `files.copy` and
//! `files.rename` still move content, and a path nobody wrote is still an error
//! rather than an empty file. What differs is where the bytes go, and that a
//! browser tab cannot see the disk. What an in-memory filesystem does not offer
//! — permissions, symlinks, and *empty* directories — it does not offer
//! silently either: `files.read` of a directory-shaped path is a `NotFound`,
//! exactly as it would be against a disk with no such file, and the one
//! directory case it cannot reproduce is written down at
//! [`memory::exists`].
//!
//! **Every path into the filesystem goes through here.** A `std::fs` call left
//! in a builtin compiles on `wasm32-unknown-unknown`, fails there at runtime,
//! and passes `cargo test` on the host — so `bytes.write` would have written to
//! a disk the playground cannot see while `files.write` wrote to the map, and
//! the two would disagree only in the one environment nobody tests in.

#[cfg(not(target_arch = "wasm32"))]
pub use std::fs::{copy, read_to_string, remove_file, rename, write};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::io::{self, Write};

    /// Whether `path` names something on disk.
    pub fn exists(path: &str) -> bool {
        std::path::Path::new(path).exists()
    }

    /// Whether `path` names a *file* — a module the `import` search opens, as
    /// distinct from the `modules/Foo/` directory the versions live in.
    pub fn is_file(path: &str) -> bool {
        std::path::Path::new(path).is_file()
    }

    /// `files.append`'s builder — `create(true).append(true)` — against the
    /// real filesystem.
    pub fn append(path: &str, content: &str) -> io::Result<()> {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| file.write_all(content.as_bytes()))
    }

    /// `bytes.write`, which is bytes rather than text and must not be routed
    /// through a UTF-8 conversion to get there.
    pub fn write_bytes(path: &str, content: &[u8]) -> io::Result<()> {
        std::fs::write(path, content)
    }

    /// The names directly inside `path`, sorted so a listing is deterministic.
    ///
    /// A directory that does not exist is an error, as it is for `read_dir`
    /// itself. A directory that exists and is empty is an empty list, not an
    /// error: "there is nothing here" is an answer.
    pub fn read_dir(path: &str) -> io::Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in std::fs::read_dir(path)? {
            names.push(entry?.file_name().to_string_lossy().into_owned());
        }
        names.sort();
        Ok(names)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{append, exists, is_file, read_dir, write_bytes};

/// The path `relative` names inside `root`, as the one string a read takes.
///
/// The root is *data* rather than process state: a program at `subdir/prog.rb`
/// resolves its `import`s against `subdir/redblue.manifest` and opens
/// `subdir/modules/Foo.rb`, whichever directory the host happens to be in. This
/// is where a search path that is relative to a root and a path a filesystem can
/// be asked about become the same thing.
///
/// The two shapes of "no root" are spelled the way they come. An empty root is
/// what [`Path::parent`] gives for a bare `prog.rb`, and a root of `.` is what
/// [`Path::new`] gives for the working directory; both answer `relative` itself,
/// without a `./` in front of it. That is not tidiness. On
/// `wasm32-unknown-unknown` the filesystem is a map from path *text* to bytes,
/// so the text `files.write("modules/Foo.rb", …)` wrote is the text a module
/// loader has to look up, and `./modules/Foo.rb` would be a second name for that
/// same file which the map has never heard of.
pub fn rooted(root: &std::path::Path, relative: &str) -> String {
    if root.as_os_str().is_empty() || root == std::path::Path::new(".") {
        return relative.strip_prefix("./").unwrap_or(relative).to_string();
    }
    root.join(relative.strip_prefix("./").unwrap_or(relative))
        .to_string_lossy()
        .into_owned()
}

/// The playground's filesystem: a flat path → bytes map that lives as long as
/// the module instance.
///
/// It is compiled on **every** target, not only on `wasm32-unknown-unknown`, and
/// that is deliberate. The logic in here is the whole of what the playground
/// does differently from a native `rb`, and a `#[cfg]` would put all of it
/// outside the reach of `cargo test`: the suite would pass while every line of it
/// went unexecuted, because the host never compiles it. Compiled here, natively
/// and tested there, the two builds' filesystem agrees by construction and the
/// test that says so is one `cargo test` runs.
pub mod memory {
    use std::collections::{BTreeMap, BTreeSet};
    use std::io;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// One instance per module. `wasm32-unknown-unknown` is single-threaded, but
    /// a `Mutex` rather than a `RefCell` so this module has one shape on every
    /// target and a `static` that needs no const initialiser.
    ///
    /// Bytes rather than text, because `bytes.write` puts bytes here that are
    /// not text at all; `read_to_string` is the one entry point that has to
    /// answer for a file that does not decode.
    type Store = BTreeMap<String, Vec<u8>>;

    fn store() -> &'static Mutex<Store> {
        static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
        STORE.get_or_init(|| Mutex::new(BTreeMap::new()))
    }

    fn lock() -> MutexGuard<'static, Store> {
        store()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn read_to_string(path: &str) -> io::Result<String> {
        let bytes = lock().get(path).cloned().ok_or_else(|| not_found(path))?;
        String::from_utf8(bytes).map_err(|_| {
            // Exactly the sentence `std::fs` gives, with nothing added. The
            // caller in `runtime.rs` prefixes the path itself, so a path in
            // here as well would print it twice in the playground and once
            // natively — the same program reading two different messages.
            io::Error::new(
                io::ErrorKind::InvalidData,
                "stream did not contain valid UTF-8",
            )
        })
    }

    pub fn write(path: &str, content: &str) -> io::Result<()> {
        lock().insert(path.to_string(), content.as_bytes().to_vec());
        Ok(())
    }

    /// `bytes.write`, against the same map as `files.write` — a byte-oriented
    /// path has to reach the same filesystem, or `bytes.write` would land
    /// somewhere `files.read` cannot see.
    pub fn write_bytes(path: &str, content: &[u8]) -> io::Result<()> {
        lock().insert(path.to_string(), content.to_vec());
        Ok(())
    }

    /// Unlike a disk, `append` does not create: `std::fs::OpenOptions` with
    /// `create(true).append(true)` creates, so the native build has to match it
    /// here, and it does.
    pub fn append(path: &str, content: &str) -> io::Result<()> {
        let mut files = lock();
        match files.get_mut(path) {
            Some(entry) => {
                entry.extend_from_slice(content.as_bytes());
                Ok(())
            }
            None => {
                files.insert(path.to_string(), content.as_bytes().to_vec());
                Ok(())
            }
        }
    }

    /// Whether `path` names something in this filesystem.
    ///
    /// A file, or a directory — because a flat map has no directory entries of
    /// its own and the native `Path::exists` this replaces answers `true` for
    /// one, so `files.exists("modules")` after a program wrote
    /// `modules/thing.rb` has to answer the same thing here or the two builds
    /// disagree about the same filesystem.
    ///
    /// A directory is implied by its contents: it exists when something is
    /// stored underneath it. That is the one case this cannot answer for — an
    /// *empty* directory on disk exists and this one does not, because nothing
    /// in the language creates one and nothing in the map could remember it.
    /// See [`directory_prefix`].
    pub fn exists(path: &str) -> bool {
        let files = lock();
        if files.contains_key(path) {
            return true;
        }
        let prefix = match directory_prefix(path) {
            Some(prefix) => prefix,
            None => return false,
        };
        files.keys().any(|stored| stored.starts_with(&prefix))
    }

    /// Whether `path` names a file rather than a directory.
    ///
    /// The map holds only files, so a key is a file and anything else is not:
    /// the directory `modules/Foo/` is implied by a file underneath it and is
    /// never a key of its own, which is exactly what a `std::fs::Path::is_file`
    /// against the equivalent tree answers. The `import` search needs the
    /// distinction — `modules/Foo/` is where the *versions* of `Foo` are
    /// published, and opening it is not what an import means.
    pub fn is_file(path: &str) -> bool {
        lock().contains_key(path)
    }

    pub fn remove_file(path: &str) -> io::Result<()> {
        lock()
            .remove(path)
            .map(|_| ())
            .ok_or_else(|| not_found(path))
    }

    pub fn copy(from: &str, to: &str) -> io::Result<u64> {
        let mut files = lock();
        let content = files.get(from).ok_or_else(|| not_found(from))?.clone();
        let length = content.len() as u64;
        files.insert(to.to_string(), content);
        Ok(length)
    }

    pub fn rename(from: &str, to: &str) -> io::Result<()> {
        let mut files = lock();
        let content = files.remove(from).ok_or_else(|| not_found(from))?;
        files.insert(to.to_string(), content);
        Ok(())
    }

    /// The names directly inside `path`, sorted so a listing is deterministic.
    ///
    /// A directory is implied by the keys underneath it, so a path with no
    /// stored key under it is not a directory this filesystem has: it is the
    /// same `NotFound` the native `read_dir` gives for a path that is not
    /// there. That is also the one case a flat map cannot reproduce — an empty
    /// directory that exists on a disk has no key here, so it is reported as
    /// missing rather than as an empty list; see [`exists`](self::exists).
    pub fn read_dir(path: &str) -> io::Result<Vec<String>> {
        let files = lock();
        if files.contains_key(path) {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                format!("Not a directory: {path}"),
            ));
        }
        let Some(prefix) = directory_prefix(path) else {
            return Err(not_found(path));
        };
        let mut names = BTreeSet::new();
        for stored in files.keys() {
            let Some(rest) = stored.strip_prefix(&prefix) else {
                continue;
            };
            if rest.is_empty() {
                continue;
            }
            let name = rest.split(['/', '\\']).next().unwrap_or(rest);
            if !name.is_empty() {
                names.insert(name.to_string());
            }
        }
        if names.is_empty() {
            return Err(not_found(path));
        }
        Ok(names.into_iter().collect())
    }

    /// Empties the map. Only a test calls this: the store is a process-wide
    /// singleton, so a test that wrote into it has to be able to start from
    /// nothing, and nothing a program can reach.
    pub fn clear() {
        lock().clear();
    }

    /// The wording `std::fs` uses for a path that is not there, so a Redblue
    /// program that reads the message reads the same sentence in both builds.
    fn not_found(path: &str) -> io::Error {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("No such file or directory (os error 2): {path}"),
        )
    }

    /// The prefix a stored path has to carry for `path` to name a directory
    /// holding it — `"notes/"` for `"notes"`, `"notes/"` for `"notes/"`.
    ///
    /// The trailing separator is the whole of it, and it is why
    /// [`exists`](self::exists) does not confuse the directory `notes` with a
    /// file called `notes.txt`. `None` for a path that cannot name a directory
    /// at all: the empty path, which `Path::new("").exists()` also answers
    /// `false` for on a disk.
    fn directory_prefix(path: &str) -> Option<String> {
        if path.is_empty() {
            return None;
        }
        if path.ends_with('/') {
            Some(path.to_string())
        } else {
            Some(format!("{path}/"))
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use memory::{
    append, copy, exists, is_file, read_dir, read_to_string, remove_file, rename, write,
    write_bytes,
};
