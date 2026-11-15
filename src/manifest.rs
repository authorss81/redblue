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
//! - `language <low> to <high>` says which Redblue language versions the package
//!   runs on, both ends inclusive. A package's own `version` and the language
//!   version that runs it are independent numbers: a bug-fix release of a
//!   package is not a language change, and a language release is not a package
//!   change.
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

/// The version of the Redblue *language* this compiler implements.
///
/// This is deliberately not a package's version. A package's `version` says
/// what release of *that package* this is; this says which release of the
/// *language* can run it. They move for different reasons — fixing a package
/// bug changes the package version and not the language, and adding syntax
/// changes the language version and not every package — so a package that
/// needs a particular language declares [`Manifest::language`] rather than
/// encoding the language in its own version number.
///
/// Bump this when the language's semantics change, not when the compiler's
/// implementation or its packaging changes.
pub const LANGUAGE_VERSION: Version = Version {
    major: 0,
    minor: 2,
    patch: 0,
};

/// The range of Redblue language versions a package runs on, both ends
/// inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanguageRange {
    /// The oldest language version the package supports.
    pub low: Version,
    /// The newest language version the package supports.
    pub high: Version,
}

impl LanguageRange {
    /// Whether `version` is inside this range, counting both ends.
    pub fn supports(&self, version: &Version) -> bool {
        self.low <= *version && *version <= self.high
    }
}

impl std::fmt::Display for LanguageRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} to {}", self.low, self.high)
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
    /// The Redblue language versions it runs on, when it states them.
    ///
    /// Optional like `name` and `version`: a manifest written before the field
    /// existed keeps resolving, and the resolver only refuses a package that
    /// declared a range and is outside it.
    pub language: Option<LanguageRange>,
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
            if !matches!(fields[0], "name" | "version" | "language" | "dependency") {
                return Err(Error::Runtime(
                    format!(
                        "{label} line {number}: '{}' is not a field a manifest has; it holds \
                         'name', 'version', 'language' and 'dependency' lines",
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
                "language" => {
                    if manifest.language.is_some() {
                        return Err(Error::Runtime(
                            format!("{label} line {number}: 'language' is declared twice"),
                            Span::unknown(),
                        ));
                    }
                    manifest.language = Some(language_range_in(&fields, line, label, number)?);
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

/// The range a `language` line declares.
///
/// Shared by [`Manifest::parse`] and the partial read that recovers a range
/// from a manifest another line makes unparseable, so both refuse the same
/// malformed lines with the same words.
fn language_range_in(
    fields: &[&str],
    line: &str,
    label: &str,
    number: usize,
) -> Result<LanguageRange> {
    if fields.len() != 4 || fields[2] != "to" {
        return Err(Error::Runtime(
            format!(
                "{label} line {number}: '{line}' is not a language line: it takes two versions \
                 separated by 'to', as in 'language 0.2.0 to 0.4.0'"
            ),
            Span::unknown(),
        ));
    }
    let low = Version::parse(fields[1]).map_err(|error| {
        Error::Runtime(
            format!("{label} line {number}: {}", error.message()),
            Span::unknown(),
        )
    })?;
    let high = Version::parse(fields[3]).map_err(|error| {
        Error::Runtime(
            format!("{label} line {number}: {}", error.message()),
            Span::unknown(),
        )
    })?;
    if low > high {
        return Err(Error::Runtime(
            format!(
                "{label} line {number}: the language range {low} to {high} is empty: {low} is \
                 above {high}"
            ),
            Span::unknown(),
        ));
    }
    Ok(LanguageRange { low, high })
}

/// What the partial read recovered: the range the refusal needs, the
/// self-description that names it, and the self-description exactly as written
/// when it was malformed.
///
/// A malformed `name` or `version` line is kept as its raw token rather than
/// dropped, so an out-of-range refusal says that the package declared a
/// malformed value instead of claiming it declared none.
struct RecoveredManifest {
    manifest: Manifest,
    /// A `name` line was present but could not name a module, as written.
    malformed_name: Option<String>,
    /// A `version` line was present but was not a version, as written.
    malformed_version: Option<String>,
}

/// The language range and self-description a manifest holds, read without
/// requiring every other line to parse.
///
/// [`Manifest::parse`] refuses the whole file at the first wrong line, which is
/// right for resolution but wrong for the pre-analysis language check: a valid
/// out-of-range `language` line beside a typo in some other line must still
/// refuse the program for its language before the analyzer reports the typo.
/// This keeps only what the refusal needs — the range, and the name and version
/// it should name — and answers `None` when there is no usable language line,
/// leaving the full parse error to the analyzer at the import.
fn manifest_language_without_full_parse(text: &str, label: &str) -> Option<RecoveredManifest> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut language = None;
    let mut seen_language = false;
    let mut name = None;
    let mut version = None;
    let mut malformed_name = None;
    let mut malformed_version = None;
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        match fields[0] {
            "language" => {
                if seen_language {
                    return None;
                }
                seen_language = true;
                language = Some(language_range_in(&fields, line, label, number).ok()?);
            }
            "name" if name.is_none() && malformed_name.is_none() => {
                if fields.len() == 2 {
                    match check_module_name(fields[1], label, number) {
                        Ok(()) => name = Some(fields[1].to_string()),
                        Err(_) => malformed_name = Some(fields[1].to_string()),
                    }
                } else {
                    malformed_name = Some(fields[1..].join(" "));
                }
            }
            "version" if version.is_none() && malformed_version.is_none() => {
                if fields.len() == 2 {
                    match Version::parse(fields[1]) {
                        Ok(parsed) => version = Some(parsed),
                        Err(_) => malformed_version = Some(fields[1].to_string()),
                    }
                } else {
                    malformed_version = Some(fields[1..].join(" "));
                }
            }
            _ => {}
        }
    }
    Some(RecoveredManifest {
        manifest: Manifest {
            name,
            version,
            language: Some(language?),
            dependencies: Vec::new(),
        },
        malformed_name,
        malformed_version,
    })
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
    if is_module_name(name) {
        return Ok(());
    }
    Err(Error::Runtime(
        format!(
            "{label} line {line}: '{name}' is not a module name: a module name starts with a \
             letter or an underscore and holds only letters, digits and underscores"
        ),
        Span::unknown(),
    ))
}

/// Whether `name` could be written as a module name.
///
/// The manifest and `rb new` both name things an `import` has to be able to
/// reach, so they ask this one question. Keeping the rule in one place is what
/// stops the two validators from drifting into disagreeing about a name.
pub(crate) fn is_module_name(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && characters.all(|c| c.is_ascii_alphanumeric() || c == '_')
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
    /// The manifest the module publishes, when it publishes one.
    ///
    /// Kept beside the pin so the module's own language range can be checked at
    /// the `import` that names it rather than while the whole graph is resolved.
    pub manifest: Option<Manifest>,
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
    resolve_with_language_checks(manifest, registry, true, true, None)
}

/// Resolves `manifest`, optionally enforcing the language ranges it and its
/// modules declare.
///
/// The public [`resolve`] always enforces both, and it enforces a module's
/// range while it resolves the graph. A host that calls the resolver directly
/// therefore gets that refusal before any program runs, where no `try` can
/// reach it; that is the API's eager check, not the path a running program
/// takes. A read-only pass that only wants the module paths an `import` binds —
/// the analyzer — calls this with both flags false, so the refusal is raised by
/// the runtime [`Pins`] at the import instead of before the program runs, where
/// a `try` can catch it. A program about to run checks its own range
/// immediately but leaves each module's range to the import that names it, so a
/// healthy first import is not refused for a later module's range.
///
/// `location` is the directory the program manifest was read from, when the
/// caller has one. It is named in a refusal for an unnamed program, where the
/// manifest itself is the only thing that says which package the range belongs
/// to.
fn resolve_with_language_checks(
    manifest: &Manifest,
    registry: &impl Registry,
    check_program_language: bool,
    check_module_language: bool,
    location: Option<&Path>,
) -> Result<Resolution> {
    if check_program_language {
        check_language_support(
            manifest,
            "this program",
            manifest.version.as_ref(),
            location,
        )?;
    }

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

        let mut resolved = ResolvedModule {
            name: dependency.name.clone(),
            version: chosen,
            path,
            required_by: format!(
                "{} (required by {})",
                dependency.requirement.describe(),
                required_by
            ),
            manifest: None,
        };

        // A module brings its own dependencies in with it, and they are
        // resolved by the same loop, so they are checked against everything
        // already settled rather than only against each other.
        if let Some(text) = registry.module_manifest(&dependency.name, &chosen) {
            let label = format!("{} {}", dependency.name, chosen);
            let module_manifest = Manifest::parse(&text, &label)?;
            if check_module_language {
                check_language_support(
                    &module_manifest,
                    &dependency.name,
                    module_manifest.version.as_ref().or(Some(&chosen)),
                    None,
                )?;
            }
            for dependency in &module_manifest.dependencies {
                pending.push_back((dependency.clone(), label.clone(), depth + 1));
            }
            resolved.manifest = Some(module_manifest);
        }

        resolution.modules.insert(dependency.name.clone(), resolved);
    }

    Ok(resolution)
}

/// Refuses the program in `root` before the analyzer walks it.
///
/// The refusal this adds is the program's language range: it must precede an
/// analyzer error, so an out-of-range program with an unknown variable is
/// refused for its language rather than for the variable. A manifest whose
/// other lines are malformed still carries its `language` line, so that line is
/// read on its own when the full parse fails; otherwise a typo elsewhere would
/// hide the refusal. A manifest that cannot be read at all has no line to
/// recover, so its read error is passed on rather than treated as a pass.
///
/// `report_unparseable` says what happens when the full parse fails and no
/// usable `language` line was recovered. A program with an `import` reports the
/// typo at that import, so its caller passes `false`. A zero-import program has
/// no import at which the analyzer walk would find the manifest, so its caller
/// passes `true` and the saved parse error is reported here rather than
/// silently accepted by an analyzer-only pass such as a linter or language
/// server.
pub(crate) fn check_program_language_before_analysis(
    root: &Path,
    report_unparseable: bool,
) -> Result<()> {
    check_program_language_in(root, report_unparseable)
}

/// The program-language check itself.
///
/// `report_unparseable` decides what a manifest that fails its full parse does
/// after the language question is answered. The analyzer wants the typo
/// reported at the `import` that found it, so it passes `false` for a program
/// with an import and leaves the file to the analyzer. A caller with no
/// analyzer behind it — the VM's pre-run check, or an analyzer whose program
/// has no import — passes `true`, because otherwise a zero-import program with
/// a malformed manifest would run or analyze with no one ever reporting the
/// file.
///
/// The language refusal always comes first: a recovered, out-of-range range
/// errors in `check_language_support` before the saved parse error is reached.
/// Only an in-range range (or no usable range at all) lets the parse error
/// surface.
fn check_program_language_in(root: &Path, report_unparseable: bool) -> Result<()> {
    let (manifest, malformed_name, malformed_version, parse_error) = match manifest_in(root) {
        Ok(manifest) => (manifest, None, None, None::<Error>),
        Err(error) => {
            let text = match fs::read_to_string(root.join(MANIFEST_FILE)) {
                Ok(text) => text,
                Err(_) => return Err(error),
            };
            match manifest_language_without_full_parse(&text, MANIFEST_FILE) {
                Some(partial) => (
                    Some(partial.manifest),
                    partial.malformed_name,
                    partial.malformed_version,
                    Some(error),
                ),
                None if report_unparseable => return Err(error),
                None => (None, None, None, None),
            }
        }
    };
    let Some(manifest) = manifest else {
        return Ok(());
    };
    check_language_support_with(
        &manifest,
        "this program",
        manifest.version.as_ref(),
        Some(root),
        malformed_name.as_deref(),
        malformed_version.as_deref(),
    )?;
    if report_unparseable {
        if let Some(error) = parse_error {
            return Err(error);
        }
    }
    Ok(())
}

/// Refuses `manifest` when the language version running it is outside the range
/// the package declared.
///
/// The package's own version is named beside the range and the running language
/// version, because "this package does not support this language" without the
/// two numbers is a sentence the reader cannot act on. A manifest that states no
/// range supports every language version: the field is opt-in so that packages
/// written before it keep resolving.
fn check_language_support(
    manifest: &Manifest,
    package: &str,
    version: Option<&Version>,
    location: Option<&Path>,
) -> Result<()> {
    check_language_support_with(manifest, package, version, location, None, None)
}

/// [`check_language_support`], with the raw text of a malformed `name` or
/// `version` line so the refusal can name it as malformed rather than absent.
fn check_language_support_with(
    manifest: &Manifest,
    package: &str,
    version: Option<&Version>,
    location: Option<&Path>,
    malformed_name: Option<&str>,
    malformed_version: Option<&str>,
) -> Result<()> {
    let Some(range) = &manifest.language else {
        return Ok(());
    };
    if range.supports(&LANGUAGE_VERSION) {
        return Ok(());
    }
    let named = match (
        manifest.name.as_deref(),
        version,
        malformed_name,
        malformed_version,
    ) {
        (Some(name), Some(version), _, _) => format!("'{name}' {version}"),
        (Some(name), None, _, Some(raw)) => format!("'{name}' (version '{raw}' is malformed)"),
        (Some(name), None, _, None) => format!("'{name}' (no version declared)"),
        (None, Some(version), Some(raw), _) => {
            format!("'{package}' {version} (name '{raw}' is malformed)")
        }
        (None, Some(version), None, _) => format!("'{package}' {version} (no name declared)"),
        (None, None, Some(raw_name), Some(raw_version)) => format!(
            "'{package}' (name '{raw_name}' is malformed, version '{raw_version}' is malformed)"
        ),
        (None, None, Some(raw_name), None) => {
            format!("'{package}' (name '{raw_name}' is malformed, no version declared)")
        }
        (None, None, None, Some(raw_version)) => {
            format!("'{package}' (no name declared, version '{raw_version}' is malformed)")
        }
        (None, None, None, None) => {
            format!("'{package}' (no name declared, no version declared)")
        }
    };
    let located = match location {
        Some(root) => format!(" in {}", root.join(MANIFEST_FILE).display()),
        None => String::new(),
    };
    Err(Error::Runtime(
        format!(
            "Package {named}{located} supports Redblue {range}, but this is Redblue \
             {LANGUAGE_VERSION}: the package cannot run on this language version"
        ),
        Span::unknown(),
    ))
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
/// This is the eager resolver API: it writes `redblue.lock` and validates every
/// module's language range while it resolves the graph, so an out-of-range
/// module is refused here, before any program runs, where the program's own
/// `try` cannot reach it. It is not the path a running program takes; that is
/// [`Pins::for_running`], which leaves each module's range to the `import` that
/// names it. A pass that only answers questions about the program wants
/// [`resolve_for_reading`] instead, which resolves the same pins, touches
/// nothing on disk, and leaves the language-range refusal to the runtime
/// `import`.
pub fn resolve_in(root: &Path) -> Result<Resolution> {
    resolve_in_with(root, true)
}

/// [`resolve_in`] for a program about to run, leaving each module's own range
/// to the `import` that names it.
///
/// The program's own range is still checked here, so a run cannot resolve past
/// an out-of-range program. A module's range is not: a healthy first import must
/// not be refused because a later module the program has not reached is out of
/// range. [`Pins::module`] checks the module the import actually names.
fn resolve_in_with(root: &Path, check_module_language: bool) -> Result<Resolution> {
    let Some(manifest) = manifest_in(root)? else {
        return Ok(Resolution::default());
    };
    let resolution = resolve_with_language_checks(
        &manifest,
        &DirRegistry::new(root),
        true,
        check_module_language,
        Some(root),
    )?;
    check_lockfile(root, &resolution)?;
    write_lockfile(root, &resolution)?;
    Ok(resolution)
}

/// Resolves what the program in `root` depends on, writing nothing, for a
/// program that is about to run.
///
/// The same answers as [`resolve_in`] — a pin nothing publishes is still an
/// error, a module needed at two versions is still a conflict, and a package
/// outside its declared language range is still refused — from reads only.
/// This is the runtime read-only path, and it checks a module's range while it
/// resolves: a host calling it directly gets that refusal before the program
/// runs, where no `try` can reach it. A pass that runs *before* the program
/// and only asks what its pins bind wants [`resolve_for_reading`] instead,
/// which is the same resolve without the module language-range refusal and
/// leaves that failure to the runtime import.
///
/// Writing nothing is what a caller that is only looking needs: without this,
/// `rb lint` in a directory holding a `redblue.manifest` left a `redblue.lock`
/// beside it, which is what `cargo check` does with a `Cargo.lock` and is still
/// a write a reader of `lint` did not ask for.
pub fn resolve_without_writing(root: &Path) -> Result<Resolution> {
    let Some(manifest) = manifest_in(root)? else {
        return Ok(Resolution::default());
    };
    resolve_with_language_checks(&manifest, &DirRegistry::new(root), true, true, Some(root))
}

/// Resolves what the program in `root` depends on for a pass that only asks
/// which names an `import` binds.
///
/// This is [`resolve_without_writing`] without the *module* language-range
/// refusal. The program's own range is still checked, because that refusal is
/// the program's and is uncatchable by design; deferring it as well would make
/// this public helper a silent pass for a linter or language server that uses
/// it without the analyzer's separate pre-check. A module's range is deferred:
/// the analyzer runs before the program, so a module refusal raised here would
/// be an `Analyzer` error no `try` can catch. Leaving it to the runtime [`Pins`]
/// keeps that failure at the `import` that loads the package, where the program
/// can catch it. A pin nothing publishes is still refused here, because the
/// analyzer has no names to bind for it either way.
pub fn resolve_for_reading(root: &Path) -> Result<Resolution> {
    let Some(manifest) = manifest_in(root)? else {
        return Ok(Resolution::default());
    };
    resolve_with_language_checks(&manifest, &DirRegistry::new(root), true, false, Some(root))
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
/// `import` resolves no dependencies and writes no lockfile, which is what every
/// program written before there was a manifest must keep doing. Its own manifest
/// is still read once for the language range, because that is a fact about
/// running the program rather than about an import.
#[derive(Debug)]
pub struct Pins {
    root: PathBuf,
    /// Whether resolving this one is what writes the lockfile. A program about to
    /// be run is the resolution the lockfile records; a pass that is only looking
    /// — the analyzer, a linter, a formatter — creates nothing.
    writing: bool,
    loaded: bool,
    /// Whether the program manifest's own language range has been checked
    /// without resolving the pins, which is what a program with no `import`
    /// asks for.
    language_checked: bool,
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
    /// The same dependency answers — a pin nothing publishes is still a failure,
    /// a module needed at two versions is still a conflict — from reads only. A
    /// program's own range is checked here as well, so no caller can silently
    /// skip it. A module's declared language range is deliberately left to
    /// [`Pins::for_running`]: a pass that runs before the program cannot raise a
    /// failure the program's own `try` could catch, so the runtime path is where
    /// an out-of-range module is refused.
    pub fn for_reading(root: &Path) -> Pins {
        Pins::in_dir(root, false)
    }

    fn in_dir(root: &Path, writing: bool) -> Pins {
        Pins {
            root: root.to_path_buf(),
            writing,
            loaded: false,
            language_checked: false,
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
            // The program's own range is checked here, but each module's range
            // waits for the `import` that names it: a first import must not be
            // refused for a later module the program has not reached.
            resolve_in_with(&self.root, false)
        } else {
            resolve_for_reading(&self.root)
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
    ///
    /// The module's own language range is checked here, when the import names
    /// it, rather than while the graph was resolved. That is what makes the
    /// refusal the import's own: a `try` around this import can catch it, and a
    /// healthy earlier import is unaffected by a later module's range.
    pub fn module(&mut self, name: &str) -> Result<Option<String>> {
        self.load();
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let Some(module) = self.resolution.module(name) else {
            return Ok(None);
        };
        // A read-only pass runs before the program, where its own `try` cannot
        // catch a module refusal, so it binds names without enforcing a
        // module's range. The runtime pins check it here, at the import that
        // names the module.
        if self.writing {
            if let Some(manifest) = &module.manifest {
                check_language_support(
                    manifest,
                    &module.name,
                    manifest.version.as_ref().or(Some(&module.version)),
                    None,
                )?;
            }
        }
        Ok(Some(module.path.clone()))
    }

    /// Refuses the program's own manifest when its language range excludes this
    /// language version, without resolving its dependencies or writing a
    /// lockfile.
    ///
    /// The VM asks this before the first statement of every run, so an
    /// out-of-range program cannot execute any of its side effects first. A
    /// module's own range stays deferred to the `import` that loads it, where
    /// the program's `try` can catch that refusal.
    ///
    /// A successful check is the only thing that marks the question answered. A
    /// manifest that cannot be read is not a pass: the next call asks again, so
    /// a reused VM cannot turn a broken manifest into a silent success. A
    /// read-only load enforces the program's range too (see
    /// [`resolve_for_reading`]), but it does not mark this question answered:
    /// its failure is kept for [`Pins::failure`], and this check must still be
    /// able to answer after the manifest is repaired.
    pub fn check_program_language(&mut self) -> Result<()> {
        if self.language_checked {
            return Ok(());
        }
        check_program_language_in(&self.root, true)?;
        self.language_checked = true;
        Ok(())
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
/// [`resolve_for_reading`] instead, which creates nothing and leaves the
/// language-range refusal to the runtime import; see
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
