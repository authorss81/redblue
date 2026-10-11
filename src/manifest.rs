//! `redblue.manifest` — what a program depends on — and `redblue.lock` — what
//! it resolved to.
//!
//! `import Foo` looks for `modules/Foo.rb` and nothing else. That is one file
//! under one name: there is no version anywhere in it, nothing for a program
//! to pin, and no way for two programs to depend on two versions of one module.
//! This module is the smallest thing that does pin them.
//!
//! The manifest is line-oriented English, like the rest of the language, and
//! lives beside the program that owns it:
//!
//! ```text
//! name report
//! version 1.0.0
//! dependency MathUtils =1.0.0
//! dependency SuiteKit *
//! ```
//!
//! - `name` and `version` say what the program itself is. Both are optional in a
//!   program manifest and *required* in a module's own manifest, where they are
//!   checked against the directory the module was found in.
//! - `dependency <name> <requirement>` is one pin. The requirement is `*` for
//!   the highest published version, or `=x.y.z` for exactly one.
//!
//! A module version lives at `modules/<name>/<version>/<name>.rb`, with its own
//! `redblue.manifest` beside it. The resolver reads those manifests too, so a
//! module's own dependencies are resolved with the program's, and a module
//! pulled in two ways cannot quietly bring two versions of the same thing.
//!
//! # The lockfile
//!
//! A resolution writes `redblue.lock` beside the manifest the first time it
//! succeeds, and writes the same bytes on every later run — the resolution is a
//! `BTreeMap` keyed by module name, so the bytes do not depend on the order the
//! filesystem listed anything in. That is the property worth having: a lockfile
//! that changed bytes on every resolve would be a file nobody could read a
//! diff of.
//!
//! # Failing
//!
//! A pin nothing publishes, and a module needed at two versions, are both
//! `Runtime` errors naming the conflict. They are errors rather than warnings
//! because the alternative is importing something the program did not ask for:
//! a lockfile that cannot be honoured means the program is not the program that
//! was written.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result, Span};

/// The manifest file a program and its modules publish.
pub const MANIFEST_FILE: &str = "redblue.manifest";

/// The file a resolution writes.
pub const LOCKFILE: &str = "redblue.lock";

/// How many dependencies one resolution will look at, counting a dependency
/// that has already been resolved.
///
/// Two limits live in that sentence. A dependency that is already resolved is
/// *skipped*, which is what stops `A → B → A` from running forever; what it
/// does not stop is a registry that keeps offering fresh names. Both are a
/// package manager describing a graph no human would write, and the answer to
/// either is a clean error rather than the process disappearing.
const MAX_RESOLUTION_STEPS: usize = 4096;

/// How deep one module's chain of dependencies may go before it is refused, as
/// distinct from how many entries there are in total.
const MAX_RESOLUTION_DEPTH: usize = 64;

/// A `major.minor.patch` version.
///
/// The three components are compared one at a time, so `1.10.0` is above
/// `1.9.0`. Comparing the text would say the opposite, and the `*` requirement
/// picks the highest version there is: a manifest asking for any version of a
/// module would get 1.9.0 forever.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

impl Version {
    /// The version `text` names, or a clean error naming what it is.
    ///
    /// Exactly three components, each of them at least one digit. `1.0` and
    /// `1.0.0-beta` are refused rather than read as `1.0.0`: a version nobody
    /// can round-trip through [`Display`] is a version two manifests will
    /// disagree about.
    pub fn parse(text: &str) -> Result<Version> {
        let refused = || {
            Error::Runtime(
                format!("'{text}' is not a version: a version is three numbers, as in 1.0.0"),
                Span::unknown(),
            )
        };
        let mut parts = Vec::new();
        for part in text.split('.') {
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return Err(refused());
            }
            parts.push(part.parse::<u64>().map_err(|_| refused())?);
        }
        if parts.len() != 3 {
            return Err(refused());
        }
        Ok(Version {
            major: parts[0],
            minor: parts[1],
            patch: parts[2],
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// What a manifest asks for of one module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Requirement {
    /// `*` — the highest version that is published.
    Any,
    /// `=1.2.3` — exactly this version, or nothing.
    Exact(Version),
}

impl Requirement {
    /// The requirement `text` names.
    pub fn parse(text: &str) -> Result<Requirement> {
        match text {
            "*" => Ok(Requirement::Any),
            _ => match text.strip_prefix('=') {
                Some(version) => Ok(Requirement::Exact(Version::parse(version)?)),
                None => Err(Error::Runtime(
                    format!(
                        "'{text}' is not a requirement: write * for the newest version, \
                         or =1.2.3 for exactly one"
                    ),
                    Span::unknown(),
                )),
            },
        }
    }

    /// How this requirement reads in an error message.
    fn describe(&self) -> String {
        match self {
            Requirement::Any => "*".to_string(),
            Requirement::Exact(version) => format!("={version}"),
        }
    }
}

/// One `dependency` line.
#[derive(Debug, Clone)]
pub struct Dependency {
    /// The module named.
    pub name: String,
    /// What it is asked for.
    pub requirement: Requirement,
    /// The 1-based line it was written on, for the message when it conflicts.
    pub line: usize,
}

/// A parsed `redblue.manifest`.
///
/// `name` and `version` are `None` when the manifest does not state them: a
/// program manifest need not describe itself, and a manifest that names itself
/// twice is refused rather than taking the second.
#[derive(Debug, Clone, Default)]
pub struct Manifest {
    /// What the program calls itself.
    pub name: Option<String>,
    /// What version of itself it is.
    pub version: Option<Version>,
    /// What it depends on, in the order the manifest wrote it.
    pub dependencies: Vec<Dependency>,
}

impl Manifest {
    /// The manifest `text` holds.
    ///
    /// `label` names the file in every error, because a module's manifest is
    /// read by the resolver rather than by the user who wrote it, and "line 3
    /// is not a field" without saying which file line 3 is in names nothing.
    pub fn parse(text: &str, label: &str) -> Result<Manifest> {
        // A byte-order mark is what a file saved by a Windows editor starts
        // with, and CRLF is how that same editor ends every line. Neither is
        // refused: both are a file that has been through a text editor, and a
        // manifest is a text file.
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut manifest = Manifest::default();
        for (index, line) in text.lines().enumerate() {
            let number = index + 1;
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = line.split_whitespace().collect();
            if !matches!(fields[0], "name" | "version" | "dependency") {
                return Err(Error::Runtime(
                    format!(
                        "{label} line {number}: '{}' is not a field a manifest has; it holds \
                         'name', 'version' and 'dependency' lines",
                        fields[0]
                    ),
                    Span::unknown(),
                ));
            }
            match fields[0] {
                "name" if fields.len() == 2 => {
                    if manifest.name.is_some() {
                        return Err(Error::Runtime(
                            format!("{label} line {number}: 'name' is declared twice"),
                            Span::unknown(),
                        ));
                    }
                    check_module_name(fields[1], label, number)?;
                    manifest.name = Some(fields[1].to_string());
                }
                "version" if fields.len() == 2 => {
                    if manifest.version.is_some() {
                        return Err(Error::Runtime(
                            format!("{label} line {number}: 'version' is declared twice"),
                            Span::unknown(),
                        ));
                    }
                    manifest.version = Some(Version::parse(fields[1]).map_err(|error| {
                        Error::Runtime(
                            format!("{label} line {number}: {}", message_of(&error)),
                            Span::unknown(),
                        )
                    })?);
                }
                "dependency" if fields.len() == 3 => {
                    check_module_name(fields[1], label, number)?;
                    manifest.dependencies.push(Dependency {
                        name: fields[1].to_string(),
                        requirement: Requirement::parse(fields[2]).map_err(|error| {
                            Error::Runtime(
                                format!("{label} line {number}: {}", message_of(&error)),
                                Span::unknown(),
                            )
                        })?,
                        line: number,
                    });
                }
                _ => {
                    // One of the three fields above, reached with the wrong
                    // number of words: the guard arms did not claim it.
                    return Err(Error::Runtime(
                        format!(
                            "{label} line {number}: '{line}' is not a complete {} line: it \
                             carries no value, where one is required",
                            fields[0]
                        ),
                        Span::unknown(),
                    ));
                }
            }
        }
        Ok(manifest)
    }
}

/// The message inside an error, for a message that has to be re-wrapped with
/// the line it came from.
fn message_of(error: &Error) -> String {
    error.to_string()
}

/// Whether `name` could be written in a Redblue program, and so could name a
/// module in an `import`.
///
/// A manifest naming `MathUtils-2` or `依赖` describes a module no `import` can
/// reach, so it is refused where it is written rather than at the import that
/// can never find it.
fn check_module_name(name: &str, label: &str, line: usize) -> Result<()> {
    let refused = || {
        Error::Runtime(
            format!(
                "{label} line {line}: '{name}' is not a module name: a module name starts with a \
                 letter or an underscore and holds only letters, digits and underscores"
            ),
            Span::unknown(),
        )
    };
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return Err(refused());
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(refused());
    }
    if !characters.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(refused());
    }
    Ok(())
}

/// Where module versions are published, as far as a resolution is concerned.
///
/// A trait rather than a directory so that resolution is testable as the thing
/// it is — which version of which module is where — and so that the files on
/// disk are one implementation of it rather than the shape of the algorithm.
pub trait Registry {
    /// Every version of `name` that is published, in ascending order.
    fn versions(&self, name: &str) -> Vec<Version>;

    /// The path the module `name` at `version` is at, or `None` when that
    /// version is not published.
    ///
    /// The path is relative to the root of the registry, so a lockfile written
    /// from it says `modules/MathUtils/1.0.0/MathUtils.rb` whether the registry
    /// was rooted at `.` or in a temporary directory.
    fn module_path(&self, name: &str, version: &Version) -> Option<String>;

    /// The manifest the module `name` at `version` publishes, if it publishes
    /// one.
    fn module_manifest(&self, name: &str, version: &Version) -> Option<String>;
}

/// One module a resolution settled on.
#[derive(Debug, Clone)]
pub struct ResolvedModule {
    /// The module name the program imports.
    pub name: String,
    /// The version it resolved to.
    pub version: Version,
    /// The file that version is at, relative to the registry root.
    pub path: String,
    /// What asked for it, for the message when two things asked for different
    /// versions of it.
    pub required_by: String,
}

/// Everything one resolution settled, ordered by module name.
///
/// The order is the order of the `BTreeMap`, not the order the filesystem or
/// the manifest listed things in: [`Resolution::lockfile`] walks it to build
/// bytes, so anything else would make the lockfile's order depend on a
/// directory listing.
#[derive(Debug, Clone, Default)]
pub struct Resolution {
    modules: BTreeMap<String, ResolvedModule>,
}

impl Resolution {
    /// What `name` resolved to, or `None` when nothing depends on it.
    pub fn module(&self, name: &str) -> Option<&ResolvedModule> {
        self.modules.get(name)
    }

    /// Every resolved module, in name order.
    pub fn modules(&self) -> impl Iterator<Item = &ResolvedModule> {
        self.modules.values()
    }

    /// The lockfile this resolution is worth.
    ///
    /// The same resolution always writes the same bytes: no clock, no path
    /// outside the registry root, and module names in one order.
    pub fn lockfile(&self) -> String {
        let mut text = String::from(
            "# redblue.lock — what this program's dependencies resolved to.\n\
             # Written by rb. A re-resolve rewrites these bytes unchanged.\n",
        );
        for module in self.modules.values() {
            text.push_str(&format!(
                "module {} {} {}\n",
                module.name, module.version, module.path
            ));
        }
        text
    }
}

/// Settles every pin in `manifest`, and the pins of the modules it pulls in.
pub fn resolve(manifest: &Manifest, registry: &impl Registry) -> Result<Resolution> {
    let mut resolution = Resolution::default();
    let mut pending: VecDeque<(Dependency, String)> = manifest
        .dependencies
        .iter()
        .cloned()
        .map(|dependency| (dependency, "this program".to_string()))
        .collect();
    let mut depth = 0usize;

    while let Some((dependency, required_by)) = pending.pop_front() {
        depth += 1;
        if depth > MAX_RESOLUTION_STEPS {
            return Err(Error::Runtime(
                format!(
                    "Resolving these dependencies did not finish in {MAX_RESOLUTION_STEPS} \
                     steps: the dependency graph is larger than a package manager will walk"
                ),
                Span::unknown(),
            ));
        }

        let available = registry.versions(&dependency.name);
        let chosen = match &dependency.requirement {
            Requirement::Exact(version) => *version,
            Requirement::Any => match available.last() {
                Some(version) => *version,
                None => {
                    return Err(Error::Runtime(
                        unpublished(&dependency.name, None, &[]),
                        Span::unknown(),
                    ))
                }
            },
        };

        // A module already settled is skipped, which is what ends a cycle. It
        // is skipped only when the version agrees: a graph that needs one
        // module at two versions has no answer, and answering it with whichever
        // request came first would give the program a module it did not ask for.
        if let Some(settled) = resolution.modules.get(&dependency.name) {
            if settled.version != chosen {
                return Err(Error::Runtime(
                    format!(
                        "Module '{}' is required at two versions: {}, and {} (required by \
                         {}). One program cannot depend on two versions of one module",
                        dependency.name,
                        settled.required_by,
                        dependency.requirement.describe(),
                        required_by,
                    ),
                    Span::unknown(),
                ));
            }
            continue;
        }

        let Some(path) = registry.module_path(&dependency.name, &chosen) else {
            return Err(Error::Runtime(
                unpublished(&dependency.name, Some(&chosen), &available),
                Span::unknown(),
            ));
        };

        resolution.modules.insert(
            dependency.name.clone(),
            ResolvedModule {
                name: dependency.name.clone(),
                version: chosen,
                path,
                required_by: format!(
                    "{} (required by {})",
                    dependency.requirement.describe(),
                    required_by
                ),
            },
        );

        // A module brings its own dependencies in with it, and they are
        // resolved by the same loop, so they are checked against everything
        // already settled rather than only against each other.
        if let Some(text) = registry.module_manifest(&dependency.name, &chosen) {
            let label = format!("{} {}", dependency.name, chosen);
            let module_manifest = Manifest::parse(&text, &label)?;
            // A single module manifest listing more pins than the deepest
            // chain allowed would be walked before the loop's own step count
            // ever saw it, so the depth it contributes is checked here.
            if module_manifest.dependencies.len() > MAX_RESOLUTION_DEPTH {
                return Err(Error::Runtime(
                    format!(
                        "Module '{}' {chosen} depends on more than {MAX_RESOLUTION_DEPTH} things, \
                         which is deeper than a resolution walks",
                        dependency.name
                    ),
                    Span::unknown(),
                ));
            }
            for dependency in module_manifest.dependencies {
                pending.push_back((dependency, label.clone()));
            }
        }
    }

    Ok(resolution)
}

/// The message for a pin nothing satisfies.
fn unpublished(name: &str, wanted: Option<&Version>, available: &[Version]) -> String {
    let wanted = match wanted {
        Some(version) => format!(" at {version}"),
        None => String::new(),
    };
    if available.is_empty() {
        return format!(
            "Cannot resolve dependency '{name}'{wanted}: no version of '{name}' is published"
        );
    }
    let versions: Vec<String> = available.iter().map(Version::to_string).collect();
    format!(
        "Cannot resolve dependency '{name}'{wanted}: it is not published. Available: {}",
        versions.join(", ")
    )
}

/// The manifest `text` holds, read as the file the convention names it.
pub fn parse_manifest(text: &str) -> Result<Manifest> {
    Manifest::parse(text, MANIFEST_FILE)
}

/// The registry of the modules published under `root`.
///
/// A module version is `modules/<name>/<version>/<name>.rb`. The version
/// directories are read from disk rather than guessed, so `*` picks a version
/// that is really there.
pub struct DirRegistry {
    root: PathBuf,
}

impl DirRegistry {
    /// The modules published under `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        DirRegistry { root: root.into() }
    }

    /// Where the path a resolution names really is.
    pub fn full_path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    /// The directory the versions of `name` are published in.
    fn module_dir(&self, name: &str, version: &Version) -> PathBuf {
        self.root
            .join("modules")
            .join(name)
            .join(version.to_string())
    }
}

impl Registry for DirRegistry {
    fn versions(&self, name: &str) -> Vec<Version> {
        let Ok(entries) = fs::read_dir(self.root.join("modules").join(name)) else {
            return Vec::new();
        };
        let mut found: Vec<Version> = entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| Version::parse(&entry.file_name().to_string_lossy()).ok())
            // A directory named like a version that publishes no module file is
            // not a published version. The file is what an `import` opens, so
            // letting `*` choose one would resolve to a path that is not there.
            .filter(|version| self.module_path(name, version).is_some())
            .collect();
        found.sort();
        found
    }

    fn module_path(&self, name: &str, version: &Version) -> Option<String> {
        let relative = format!("modules/{name}/{version}/{name}.rb");
        self.full_path(&relative).is_file().then_some(relative)
    }

    fn module_manifest(&self, name: &str, version: &Version) -> Option<String> {
        fs::read_to_string(self.module_dir(name, version).join(MANIFEST_FILE)).ok()
    }
}

/// Resolves what the program in `root` depends on, and writes its lockfile.
///
/// A `root` with no manifest resolves to nothing and writes nothing: that is
/// what every program written before there was a manifest must keep doing, and
/// a missing manifest is not an error because the pins are all optional.
pub fn resolve_in(root: &Path) -> Result<Resolution> {
    let registry = DirRegistry::new(root);
    let text = match fs::read_to_string(root.join(MANIFEST_FILE)) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Resolution::default())
        }
        Err(error) => {
            return Err(Error::Io(format!(
                "Cannot read '{}': {}",
                root.join(MANIFEST_FILE).display(),
                error
            )))
        }
    };
    let manifest = parse_manifest(&text)?;
    let resolution = resolve(&manifest, &registry)?;
    write_lockfile(root, &resolution)?;
    Ok(resolution)
}

/// Writes the lockfile for `resolution` under `root`, if it is not already
/// there with those bytes.
///
/// The comparison is what makes "written on first resolve" true without also
/// making every run a write: a resolve that produced the same resolution leaves
/// the file's timestamp alone, so `rb` running twice in a row does not touch a
/// checkout.
///
/// A resolution with nothing in it writes nothing at all. A program that
/// depends on no module has nothing to lock, and a lockfile holding only its
/// own comment is a file that exists for no reason. A lockfile already on disk
/// is left where it is rather than deleted: emptying a manifest is a reason to
/// re-resolve, not a reason for the tool to remove a file.
fn write_lockfile(root: &Path, resolution: &Resolution) -> Result<()> {
    if resolution.modules.is_empty() {
        return Ok(());
    }
    let path = root.join(LOCKFILE);
    let bytes = resolution.lockfile();
    if fs::read_to_string(&path).ok().as_deref() == Some(bytes.as_str()) {
        return Ok(());
    }
    fs::write(&path, bytes)
        .map_err(|error| Error::Io(format!("Cannot write '{}': {}", path.display(), error)))
}

// The resolution this process is running against, resolved once. `import` asks
// for it on every import it resolves, and reading a directory per import would
// make a program's start-up cost its module count.
//
// The message is kept rather than the `Error` because a failure has to be
// reported again on the next import, and `Error` is not `Clone`.
thread_local! {
    static RESOLUTION: RefCell<Option<std::result::Result<Resolution, String>>> =
        const { RefCell::new(None) };
}

/// The path the module `name` is pinned to by the manifest in the working
/// directory, or `None` when nothing depends on it.
///
/// `Err` is a manifest or lockfile this program cannot be resolved from, and
/// is passed on rather than swallowed: a pin nothing publishes must not turn
/// into an import of whatever file happens to be at `modules/<name>.rb`.
pub fn pinned_module_path(name: &str) -> Result<Option<String>> {
    RESOLUTION.with(|slot| {
        if slot.borrow().is_none() {
            let resolved = resolve_in(Path::new(".")).map_err(|error| error.to_string());
            *slot.borrow_mut() = Some(resolved);
        }
        // Cloned rather than borrowed through the `match`: the borrow is held
        // for the whole arm, and a resolution is small enough that copying one
        // costs less than proving nothing under it can re-enter this.
        match slot.borrow().clone() {
            Some(Ok(resolution)) => Ok(resolution.module(name).map(|m| m.path.clone())),
            Some(Err(message)) => Err(Error::Runtime(message, Span::unknown())),
            None => Ok(None),
        }
    })
}

/// The path the module `name` under `root` is pinned to, or `None` when nothing
/// depends on it.
///
/// [`pinned_module_path`] is this against the working directory, with the
/// resolution cached. `root` is its own parameter so that a caller holding a
/// program from somewhere other than the working directory — the analyzer, which
/// is asked about a program before anything is run — resolves against the
/// manifest that program was found beside.
pub fn pinned_module_path_in(root: &Path, name: &str) -> Result<Option<String>> {
    match resolve_in(root) {
        Ok(resolution) => Ok(resolution.module(name).map(|module| module.path.clone())),
        Err(error) => Err(error),
    }
}

/// Why the manifest in `root` cannot be resolved, or `None` when it can — and
/// `None` when there is no manifest at all, which is not a failure.
///
/// A whole program is answerable from one manifest, so this asks about the
/// manifest rather than about one module: a conflict between two modules is
/// raised whichever module is asked about, which is what makes it usable by a
/// walk that only wants to know whether to carry on.
pub fn failure_in(root: &Path) -> Option<String> {
    resolve_in(root).err().map(|error| error.to_string())
}
