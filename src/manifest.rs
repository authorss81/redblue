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
//! It is also *read*. A lockfile that is only written records what the last
//! resolve found, and a program whose manifest pins `*` would be handed a new
//! version the moment one is published, in silence, with nothing to notice. So a
//! lockfile that disagrees with the resolution is refused, by
//! [`check_lockfile`], naming the module that moved.
//!
//! # Failing
//!
//! A pin nothing publishes, and a module needed at two versions, are both
//! `Runtime` errors naming the conflict. They are errors rather than warnings
//! because the alternative is importing something the program did not ask for:
//! a lockfile that cannot be honoured means the program is not the program that
//! was written.
//!
//! Those errors carry their position — the file and the line — in the message
//! rather than in a [`Span`], because a span is rendered as a caret against the
//! program being run and a manifest is a second file. See [`Manifest::parse`].

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
///
/// Public so that a test can build the graph that trips it rather than a smaller
/// one that does not: the number is the contract, and a test that writes 4096
/// hard-coded next to a constant that says 4096 is a test that rots quietly when
/// the constant moves.
pub const MAX_RESOLUTION_STEPS: usize = 4096;

/// How deep one module's chain of dependencies may go before it is refused, as
/// distinct from how many entries there are in total.
///
/// Counted per *chain*, not per module: the program's own pins are one step
/// deep, a pin of a module one step from the program is two, and so on. A
/// hundred-long chain of one dependency each is a hundred steps — inside
/// [`MAX_RESOLUTION_STEPS`] — and is refused here. Width is the other question
/// and [`MAX_RESOLUTION_STEPS`] is what answers it: a manifest listing a
/// thousand pins is wide, not deep, and a thousand pins is a manifest somebody
/// wrote on purpose rather than a graph that ran away.
///
/// Public for the same reason as [`MAX_RESOLUTION_STEPS`]: the test builds a
/// chain of exactly this length and one of exactly one more.
pub const MAX_RESOLUTION_DEPTH: usize = 64;

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
    ///
    /// The label and the line are in the *message*, and every failure here is
    /// [`Span::unknown`]. That is not a gap in the reporting: a `Span` is a
    /// position in a source text, and it is rendered by drawing a caret against
    /// one — the program that was being analyzed or run. A manifest is a second
    /// file, so a manifest line carried as a span would render as
    /// `--> report.rb:3:1` over a line of `report.rb` that has nothing to do
    /// with it. The position the reader needs travels with the words instead.
    /// A caller that surfaces the failure against the *program* — the analyzer,
    /// which reports a conflict at the `import` that found it — therefore passes
    /// this message on as it is, label and line intact and no `RuntimeError:`
    /// prefix, so that one failure is reported once.
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
                            format!("{label} line {number}: {}", error.message()),
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
                                format!("{label} line {number}: {}", error.message()),
                                Span::unknown(),
                            )
                        })?,
                        line: number,
                    });
                }
                _ if fields.len() < words_in_a_line(fields[0]) => {
                    // One of the three fields above, reached with too few words:
                    // the guard arms did not claim it because the value it wants
                    // is the one that is not there.
                    return Err(Error::Runtime(
                        format!(
                            "{label} line {number}: '{line}' is not a complete {} line: it \
                             carries no value, where one is required",
                            fields[0]
                        ),
                        Span::unknown(),
                    ));
                }
                _ => {
                    // Too *many* words this time, which is a different mistake:
                    // everything the line needs is written and something else is
                    // written beside it. Saying it "carries no value" would be
                    // backwards — `name report notes` is missing nothing.
                    let wanted = words_in_a_line(fields[0]) - 1;
                    return Err(Error::Runtime(
                        format!(
                            "{label} line {number}: '{line}' carries {} values, where a {} line \
                             takes {wanted}",
                            fields.len() - 1,
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

/// How many words a line of `field` is written in, counting the field itself.
///
/// `name` and `version` name one value; `dependency` names two. The count is
/// what tells a line that is *missing* its value from one that has an extra,
/// because the two reach the same place otherwise and the same sentence cannot
/// describe both.
fn words_in_a_line(field: &str) -> usize {
    match field {
        "dependency" => 3,
        _ => 2,
    }
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
    /// Every version of `name` that is published.
    ///
    /// In whatever order the registry finds them in: [`resolve`] asks for the
    /// highest version itself rather than trusting that the last one offered is
    /// the highest, because a third-party [`Registry`] that returned `1.10.0`,
    /// `1.9.0` would have `*` resolve to `1.9.0` and say nothing about why.
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

/// One line of a lockfile: the module, the version it is held at, and the file
/// that version is at, relative to the root it was written under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedModule {
    /// The module the line names.
    pub name: String,
    /// The version it is held at.
    pub version: Version,
    /// The file that version is at, relative to the root.
    pub path: String,
}

/// The lockfile `text` holds.
///
/// A lockfile is read, not only written. Written alone it is a log of what the
/// last resolve found, and the world moves under it: publish a new version of a
/// module a manifest pins with `*`, and the next resolve quietly hands the
/// program different code from the one the file records, with nothing to notice.
/// Reading it back is what makes it a lock — see [`check_lockfile`], which
/// refuses exactly that disagreement.
///
/// The errors are [`Error::Runtime`] naming the line, like [`Manifest::parse`]
///'s and for the same reason: a lockfile is a second file, so its position
/// travels in the message rather than as a [`Span`] drawn against the program.
pub fn parse_lockfile(text: &str) -> Result<Vec<LockedModule>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut locked = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields[0] != "module" {
            return Err(Error::Runtime(
                format!(
                    "{LOCKFILE} line {number}: '{}' is not a field a lockfile has; it holds \
                     'module' lines",
                    fields[0]
                ),
                Span::unknown(),
            ));
        }
        if fields.len() != 4 {
            return Err(Error::Runtime(
                format!(
                    "{LOCKFILE} line {number}: '{line}' is not a complete module line: it takes \
                     a name, a version and a path, and it carries {}",
                    fields.len().saturating_sub(1)
                ),
                Span::unknown(),
            ));
        }
        check_module_name(fields[1], LOCKFILE, number)?;
        let version = Version::parse(fields[2]).map_err(|error| {
            Error::Runtime(
                format!("{LOCKFILE} line {number}: {}", error.message()),
                Span::unknown(),
            )
        })?;
        locked.push(LockedModule {
            name: fields[1].to_string(),
            version,
            path: fields[3].to_string(),
        });
    }
    Ok(locked)
}

/// Settles every pin in `manifest`, and the pins of the modules it pulls in.
///
/// Each queued dependency carries the depth of the chain that reached it: the
/// program's own pins are one deep, and a module's pins are one deeper than
/// the module itself. That is what [`MAX_RESOLUTION_DEPTH`] is checked against,
/// so a dependency that arrives too deep is refused by the chain it is in
/// rather than by how wide the graph around it happens to be.
pub fn resolve(manifest: &Manifest, registry: &impl Registry) -> Result<Resolution> {
    let mut resolution = Resolution::default();
    let mut pending: VecDeque<(Dependency, String, usize)> = manifest
        .dependencies
        .iter()
        .cloned()
        .map(|dependency| (dependency, "this program".to_string(), 1))
        .collect();
    let mut steps = 0usize;

    while let Some((dependency, required_by, depth)) = pending.pop_front() {
        steps += 1;
        if steps > MAX_RESOLUTION_STEPS {
            return Err(Error::Runtime(
                format!(
                    "Resolving these dependencies did not finish in {MAX_RESOLUTION_STEPS} \
                     steps: the dependency graph is larger than a package manager will walk"
                ),
                Span::unknown(),
            ));
        }
        if depth > MAX_RESOLUTION_DEPTH {
            return Err(Error::Runtime(
                format!(
                    "Dependency '{}' is {depth} steps from this program (required by {required_by}), \
                     and a resolution walks at most {MAX_RESOLUTION_DEPTH}: the chain of \
                     dependencies under it is longer than one a program meant",
                    dependency.name
                ),
                Span::unknown(),
            ));
        }

        let available = registry.versions(&dependency.name);
        let chosen = match &dependency.requirement {
            Requirement::Exact(version) => *version,
            // The maximum, not the last one offered. Both bundled registries
            // happen to sort, so `available.last()` agreed with this everywhere
            // it could be exercised — and would silently resolve `*` to the
            // wrong file for any registry that does not.
            Requirement::Any => match available.iter().max() {
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
            for dependency in module_manifest.dependencies {
                pending.push_back((dependency, label.clone(), depth + 1));
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

/// The manifest the program in `root` publishes, or `None` when it publishes
/// none.
///
/// A missing manifest is not an error: the pins are all optional, and every
/// program written before there was a manifest has to keep working. A manifest
/// that is there and cannot be read or parsed is an error, naming the file.
fn manifest_in(root: &Path) -> Result<Option<Manifest>> {
    let text = match fs::read_to_string(root.join(MANIFEST_FILE)) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(Error::Io(format!(
                "Cannot read '{}': {}",
                root.join(MANIFEST_FILE).display(),
                error
            )))
        }
    };
    parse_manifest(&text).map(Some)
}

/// Resolves what the program in `root` depends on, and writes its lockfile.
///
/// A `root` with no manifest resolves to nothing and writes nothing: that is
/// what every program written before there was a manifest must keep doing, and
/// a missing manifest is not an error because the pins are all optional.
///
/// This is the *resolving* path — it is what writes `redblue.lock`, and what a
/// program that is about to be run wants. A pass that only answers questions
/// about the program wants [`resolve_without_writing`] instead, which resolves
/// the same pins and touches nothing on disk.
pub fn resolve_in(root: &Path) -> Result<Resolution> {
    let Some(manifest) = manifest_in(root)? else {
        return Ok(Resolution::default());
    };
    let resolution = resolve(&manifest, &DirRegistry::new(root))?;
    check_lockfile(root, &resolution)?;
    write_lockfile(root, &resolution)?;
    Ok(resolution)
}

/// Resolves what the program in `root` depends on, writing nothing.
///
/// The same answers as [`resolve_in`] — a pin nothing publishes is still an
/// error, a module needed at two versions is still a conflict — from reads
/// only. A static pass (the analyzer, a linter, a formatter) asks what a
/// program's pins bind and has no reason to create a file to answer it:
/// without this, `rb lint` in a directory holding a `redblue.manifest` left a
/// `redblue.lock` beside it, which is what `cargo check` does with a
/// `Cargo.lock` and is still a write a reader of `lint` did not ask for.
pub fn resolve_without_writing(root: &Path) -> Result<Resolution> {
    let Some(manifest) = manifest_in(root)? else {
        return Ok(Resolution::default());
    };
    resolve(&manifest, &DirRegistry::new(root))
}

/// Refuses a lockfile that says something other than what this resolve found.
///
/// A lockfile nobody reads is a log. Reading it back is what makes it a lock: a
/// program whose `redblue.lock` holds `MathUtils 2.0.0` was run against
/// `2.0.0`, and a `MathUtils/3.0.0/` published since then — which is what a
/// `dependency MathUtils *` resolves to — must not be handed to it in silence.
/// The world moved under the lock, so the lock is refused rather than quietly
/// rewritten by the same resolve that noticed.
///
/// The way out is named in the message: delete `redblue.lock`. That is a
/// deliberate act, and it is the right one — a program that has not been run
/// since a module was published should not change under the reader's feet
/// because a directory was added somewhere else.
///
/// No lockfile, or one that cannot be read, is nothing to disagree with; only a
/// lockfile that is there and says something different is refused. A malformed
/// one *is* a disagreement with the grammar, and [`parse_lockfile`] says so.
fn check_lockfile(root: &Path, resolution: &Resolution) -> Result<()> {
    let path = root.join(LOCKFILE);
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

    for locked in parse_lockfile(&text)? {
        let disagreement = match resolution.module(&locked.name) {
            Some(module) if module.version == locked.version => continue,
            Some(module) => format!(
                "it is locked at {} and this manifest resolves it to {}",
                locked.version, module.version
            ),
            None => format!(
                "it is locked at {} and this manifest does not resolve it at all",
                locked.version
            ),
        };
        return Err(Error::Runtime(
            format!(
                "Cannot honour '{LOCKFILE}': module '{}' disagrees with the resolution — \
                 {disagreement}. The world moved under the lock; delete '{LOCKFILE}' to resolve \
                 again against what is published now",
                locked.name
            ),
            Span::unknown(),
        ));
    }
    Ok(())
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

/// One program's pins, resolved once and answered to every import it has.
///
/// A resolve is a manifest read and a scan of `modules/<name>/` for each module
/// it settles. A program imports what it imports — four `import Fixture` lines,
/// one import at the top of every function of a file — so asked per import that
/// is the same work done once per import to produce one answer. The cache that
/// used to hold the first answer was keyed on nothing at all, so it answered a
/// second program in the same process from the *first* program's manifest. This
/// is neither: one resolution, held by the thing that is running the program,
/// which is the only thing that knows which program it is.
///
/// Read on first question, not on construction: a VM built for a program with no
/// `import` reads no manifest and writes no lockfile, which is what every
/// program written before there was a manifest must keep doing.
#[derive(Debug)]
pub struct Pins {
    root: PathBuf,
    /// Whether resolving this one is what writes the lockfile. A program about to
    /// be run is the resolution the lockfile records; a pass that is only looking
    /// — the analyzer, a linter, a formatter — creates nothing.
    writing: bool,
    loaded: bool,
    resolution: Resolution,
    failure: Option<Error>,
}

impl Pins {
    /// The pins of the manifest in `root`, for a program that is about to be run.
    pub fn for_running(root: &Path) -> Pins {
        Pins::in_dir(root, true)
    }

    /// The pins of the manifest in `root`, for a pass that is only looking.
    ///
    /// The same answers — a pin nothing publishes is still a failure, a module
    /// needed at two versions is still a conflict — from reads only.
    pub fn for_reading(root: &Path) -> Pins {
        Pins::in_dir(root, false)
    }

    fn in_dir(root: &Path, writing: bool) -> Pins {
        Pins {
            root: root.to_path_buf(),
            writing,
            loaded: false,
            resolution: Resolution::default(),
            failure: None,
        }
    }

    fn load(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        let outcome = if self.writing {
            resolve_in(&self.root)
        } else {
            resolve_without_writing(&self.root)
        };
        match outcome {
            Ok(resolution) => self.resolution = resolution,
            Err(error) => self.failure = Some(error),
        }
    }

    /// The directory these pins are the manifest of.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Why the manifest cannot be resolved, or `None` when it can — and `None`
    /// when there is no manifest at all, which is not a failure.
    ///
    /// A whole program is answerable from one manifest, so this asks about the
    /// manifest rather than about one module: a conflict between two modules is
    /// raised whichever module is asked about, which is what makes it usable by a
    /// walk that only wants to know whether to carry on.
    ///
    /// The *message*, with no `RuntimeError:` label in front of it. The caller is
    /// the analyzer, which reports this as an `Analyzer` error at the `import`
    /// that found it — handing it `Error::to_string` produced `AnalyzerError:
    /// RuntimeError: Cannot resolve …` for one failure, two labels and a name the
    /// user never typed. The manifest's own location stays in the message, where
    /// it belongs: the caret an `Error`'s span draws is drawn against the
    /// *program*, so a manifest line carried as a span would put `report.rb:3:1`
    /// under a line of the program that is not line 3 of the manifest. See
    /// [`Manifest::parse`].
    pub fn failure(&mut self) -> Option<String> {
        self.load();
        self.failure
            .as_ref()
            .map(|error| error.message().to_string())
    }

    /// Everything this program resolved to, or nothing when it could not be
    /// resolved — ask [`Pins::failure`] for why.
    pub fn resolution(&mut self) -> &Resolution {
        self.load();
        &self.resolution
    }

    /// The file the module `name` is pinned to, or `None` when nothing depends
    /// on it.
    ///
    /// `Err` is a manifest this program cannot be resolved from, and is passed
    /// on rather than swallowed: a pin nothing publishes must not turn into an
    /// import of whichever file happens to be at `modules/<name>.rb`.
    pub fn module(&mut self, name: &str) -> Result<Option<String>> {
        self.load();
        match &self.failure {
            Some(error) => Err(error.clone()),
            None => Ok(self.resolution.module(name).map(|m| m.path.clone())),
        }
    }
}

/// The path the module `name` is pinned to by the manifest in the working
/// directory, or `None` when nothing depends on it.
///
/// One [`Pins`] against the working directory, asked once. A caller running a
/// whole program holds one [`Pins`] for it instead; this is for a caller with a
/// single question and no run of its own to hang them on.
///
/// This is the *resolving* path and it writes the lockfile, because it is called
/// by a loader — a program is about to be run, and running it is the resolution
/// that the lockfile records. A pass that only asks what the pins bind uses
/// [`resolve_without_writing`] instead and creates nothing; see
/// [`crate::analyzer::analyze_in`], which reads once per program and hands its
/// pins to every import rather than resolving per import.
pub fn pinned_module_path(name: &str) -> Result<Option<String>> {
    pinned_module_path_in(Path::new("."), name)
}

/// The path the module `name` under `root` is pinned to, or `None` when nothing
/// depends on it.
///
/// `root` is its own parameter so that a caller holding a program from somewhere
/// other than the working directory resolves against the manifest that program
/// was found beside rather than against whichever directory the host happens to
/// be in.
pub fn pinned_module_path_in(root: &Path, name: &str) -> Result<Option<String>> {
    Pins::for_running(root).module(name)
}
