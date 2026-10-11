//! `redblue.manifest` — name, version, dependencies — and the lockfile a
//! resolution writes.
//!
//! Before this, `import Foo` looked for `modules/Foo.rb` and nothing else: one
//! file, no version, nothing to pin, and no way for two programs to depend on
//! two versions of one module. These tests pin the manifest schema, the
//! resolver, and the bytes of the lockfile.

use std::path::{Path, PathBuf};

use redblue::manifest::{self, Registry, Version, MAX_RESOLUTION_DEPTH, MAX_RESOLUTION_STEPS};

/// A module published to a registry, with the manifest it publishes beside it.
struct Published {
    /// The manifest the module publishes, if any.
    manifest: Option<String>,
}

/// A registry held in memory, so a resolution can be exercised without the
/// filesystem: which version of which module is where is the whole question,
/// and asking it of a directory instead would add a way for it to fail for the
/// wrong reason.
///
/// Indexed by name as well as by `(name, version)`. The graph that trips
/// [`MAX_RESOLUTION_STEPS`] holds four thousand modules and asks for the
/// versions of each of them four thousand times; scanning every key for every
/// one of those is sixteen million comparisons, which is a test measuring the
/// fixture rather than the resolver.
#[derive(Default)]
struct Memory {
    modules: std::collections::BTreeMap<(String, String), Published>,
    by_name: std::collections::BTreeMap<String, Vec<Version>>,
}

impl Memory {
    /// Publishes `name` at `version`.
    fn publish(mut self, name: &str, version: &str) -> Self {
        let parsed = Version::parse(version).expect("a fixture version");
        let versions = self.by_name.entry(name.to_string()).or_default();
        if !versions.contains(&parsed) {
            versions.push(parsed);
            versions.sort();
        }
        self.modules.insert(
            (name.to_string(), version.to_string()),
            Published { manifest: None },
        );
        self
    }

    /// Publishes `name` at `version` with a manifest of its own.
    fn with_manifest(mut self, name: &str, version: &str, text: &str) -> Self {
        if let Some(published) = self
            .modules
            .get_mut(&(name.to_string(), version.to_string()))
        {
            published.manifest = Some(text.to_string());
        }
        self
    }
}

impl Registry for Memory {
    fn versions(&self, name: &str) -> Vec<Version> {
        self.by_name.get(name).cloned().unwrap_or_default()
    }

    fn module_path(&self, name: &str, version: &Version) -> Option<String> {
        self.modules
            .contains_key(&(name.to_string(), version.to_string()))
            .then(|| format!("modules/{name}/{version}/{name}.rb"))
    }

    fn module_manifest(&self, name: &str, version: &Version) -> Option<String> {
        self.modules
            .get(&(name.to_string(), version.to_string()))
            .and_then(|published| published.manifest.clone())
    }
}

/// A registry that offers `versions` in exactly the order it was given.
///
/// [`Memory`] sorts, because its versions live in a `Vec` it keeps sorted and
/// both bundled registries sort. The resolver took `available.last()` and the
/// [`Registry`] trait documented that it would be handed versions in ascending
/// order — so it agreed with every registry it could be exercised against and
/// would have disagreed with any third-party one offering `1.10.0` before
/// `1.9.0`. This is that third-party one.
struct Unordered {
    offered: Vec<Version>,
}

impl Unordered {
    fn new(offered: Vec<String>) -> Unordered {
        Unordered {
            offered: offered
                .iter()
                .map(|version| {
                    Version::parse(version)
                        .unwrap_or_else(|error| panic!("a fixture version: {}", error.message()))
                })
                .collect(),
        }
    }
}

impl Registry for Unordered {
    fn versions(&self, _name: &str) -> Vec<Version> {
        self.offered.clone()
    }

    fn module_path(&self, name: &str, version: &Version) -> Option<String> {
        self.offered
            .contains(version)
            .then(|| format!("modules/{name}/{version}/{name}.rb"))
    }

    fn module_manifest(&self, _name: &str, _version: &Version) -> Option<String> {
        None
    }
}

/// Resolves `text` against `registry`, failing the test rather than the phase.
#[track_caller]
fn resolve(text: &str, registry: &impl Registry) -> manifest::Resolution {
    let manifest = manifest::parse_manifest(text).unwrap_or_else(|error| {
        panic!("the manifest should parse, failed with {}", error.message())
    });
    manifest::resolve(&manifest, registry).unwrap_or_else(|error| {
        panic!(
            "the manifest should resolve, failed with {}",
            error.message()
        )
    })
}

/// The error resolving `text` against `registry` produced.
#[track_caller]
fn resolve_err(text: &str, registry: &impl Registry) -> String {
    let manifest = manifest::parse_manifest(text).unwrap_or_else(|error| {
        panic!("the manifest should parse, failed with {}", error.message())
    });
    manifest::resolve(&manifest, registry)
        .err()
        .unwrap_or_else(|| panic!("'{text}' should not resolve"))
        .message()
        .to_string()
}

/// The error parsing `text` as a manifest produced.
#[track_caller]
fn parse_err(text: &str) -> String {
    manifest::parse_manifest(text)
        .err()
        .unwrap_or_else(|| panic!("'{text}' should not parse"))
        .message()
        .to_string()
}

/// The program `source` is, lexed and parsed.
#[track_caller]
fn program(source: &str) -> redblue::parser::Program {
    redblue::parser::parse(
        redblue::lexer::Lexer::tokenize(source)
            .unwrap_or_else(|error| panic!("the source should lex, failed with {error}")),
    )
    .unwrap_or_else(|error| panic!("the source should parse, failed with {error}"))
}

/// The version `name` resolved to, as text.
#[track_caller]
fn version_of(resolved: &manifest::Resolution, name: &str) -> String {
    resolved
        .module(name)
        .unwrap_or_else(|| panic!("'{name}' should have resolved"))
        .version
        .to_string()
}

/// A directory of its own under `target/tmp`, so one test's modules cannot be
/// another's. Tests run in parallel in one process, so the name is the test's.
#[track_caller]
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("tmp")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("{} should be creatable: {error}", dir.display()));
    dir
}

/// Writes `modules/<name>/<version>/<name>.rb` under `root`.
fn publish(root: &Path, name: &str, version: &str, source: &str) {
    let dir = root.join("modules").join(name).join(version);
    std::fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("{}: {error}", dir.display()));
    std::fs::write(dir.join(format!("{name}.rb")), source)
        .expect("the module file should be written");
}

/// Writes `redblue.manifest` under `root`.
fn manifest_file(root: &Path, text: &str) {
    std::fs::write(root.join(manifest::MANIFEST_FILE), text)
        .expect("the manifest should be written");
}

/// The two versions of one module, published side by side.
fn two_versions() -> Memory {
    Memory::default()
        .publish("MathUtils", "1.0.0")
        .publish("MathUtils", "2.0.0")
}

/// A registry holding a chain of modules, each depending on the next, `length`
/// of them deep.
///
/// `Link0` is the module the program pins; `Link1` depends on `Link2` and so on,
/// so the chain is `length` steps from the program — deep, and one pin wide.
fn chain(length: usize) -> Memory {
    let mut registry = Memory::default();
    for index in 0..length {
        registry = registry.publish(&format!("Link{index}"), "1.0.0");
    }
    for index in 0..length.saturating_sub(1) {
        registry = registry.with_manifest(
            &format!("Link{index}"),
            "1.0.0",
            &format!("dependency Link{} =1.0.0\n", index + 1),
        );
    }
    registry
}

/// A module named `Wide` that pins `count` leaf modules of its own.
///
/// Width, where [`chain`] is depth: a module holding a great many pins is the
/// graph the *step* budget is about, and one module holding a great many
/// dependencies is the graph the *depth* budget used to be about instead.
fn wide_module(count: usize) -> Memory {
    let mut registry = Memory::default().publish("Wide", "1.0.0");
    let mut text = String::from("name Wide\nversion 1.0.0\n");
    for index in 0..count {
        text.push_str(&format!("dependency Leaf{index} =1.0.0\n"));
        registry = registry.publish(&format!("Leaf{index}"), "1.0.0");
    }
    registry.with_manifest("Wide", "1.0.0", &text)
}

/// Runs `source` on the bytecode VM, loading modules from `root`.
///
/// The loader half of a pin: the analyzer answers which names an `import` binds,
/// and this is the file the import is read *from*. Compiled from the parsed
/// program rather than from the source, because `compile_source` analyzes
/// against the working directory — the very thing this test is about — and would
/// reject `say VALUE` for a program the analyzer was asked about with a `root`.
#[track_caller]
fn run_on_bytecode_vm(root: &Path, source: &str) -> Vec<String> {
    let chunk = redblue::bytecode::compile_program(&program(source))
        .unwrap_or_else(|error| panic!("the program should compile, failed with {error}"));
    let mut vm = redblue::bytecode::vm::BytecodeVm::new_in(root);
    vm.set_echo(false);
    vm.run(&chunk)
        .unwrap_or_else(|error| panic!("the program should run, failed with {error}"));
    vm.take_output()
}

/// Two programs, each pinning one version of the same module, resolve to
/// different versions of it — the thing no name-only search can express.
#[test]
fn two_programs_pin_two_versions_of_one_module() {
    let registry = two_versions();

    let old = resolve(
        "name old\nversion 0.1.0\ndependency MathUtils =1.0.0\n",
        &registry,
    );
    let new = resolve(
        "name new\nversion 0.1.0\ndependency MathUtils =2.0.0\n",
        &registry,
    );

    assert_eq!(version_of(&old, "MathUtils"), "1.0.0");
    assert_eq!(version_of(&new, "MathUtils"), "2.0.0");
    assert_eq!(
        old.module("MathUtils").map(|module| module.path.clone()),
        Some("modules/MathUtils/1.0.0/MathUtils.rb".to_string()),
        "the path names the version that was pinned, not the newest one"
    );
}

/// Each program's resolution is its own: resolving the second pin leaves the
/// first resolution's answer untouched.
#[test]
fn resolutions_of_two_pins_do_not_share_state() {
    let registry = two_versions();
    let first = resolve("dependency MathUtils =1.0.0\n", &registry);
    let second = resolve("dependency MathUtils =2.0.0\n", &registry);

    assert_eq!(version_of(&first, "MathUtils"), "1.0.0");
    assert_eq!(version_of(&second, "MathUtils"), "2.0.0");
}

/// `*` takes the newest version, and "newest" is numeric rather than
/// alphabetical: `1.10.0` is above `1.9.0` and a text comparison would have it
/// the other way round.
#[test]
fn wildcard_takes_the_highest_version_by_number() {
    let registry = Memory::default()
        .publish("MathUtils", "1.9.0")
        .publish("MathUtils", "1.10.0")
        .publish("MathUtils", "0.9.9");

    let resolved = resolve("dependency MathUtils *\n", &registry);
    assert_eq!(version_of(&resolved, "MathUtils"), "1.10.0");
}

/// A module brought in by another module is resolved by the same loop, so the
/// program does not need to pin what its own dependency already pins.
#[test]
fn a_module_dependencies_are_resolved_too() {
    let registry = Memory::default()
        .publish("Report", "1.0.0")
        .with_manifest(
            "Report",
            "1.0.0",
            "name Report\nversion 1.0.0\ndependency MathUtils =1.0.0\n",
        )
        .publish("MathUtils", "1.0.0")
        .publish("MathUtils", "2.0.0");

    let resolved = resolve("dependency Report =1.0.0\n", &registry);
    assert_eq!(version_of(&resolved, "MathUtils"), "1.0.0");
}

/// A module's own pin and the program's pin disagreeing is the conflict that
/// matters: the program asked for `Report`, and got two `MathUtils`.
#[test]
fn edge_conflicting_pins_name_both_versions() {
    let registry = Memory::default()
        .publish("Report", "1.0.0")
        .with_manifest(
            "Report",
            "1.0.0",
            "name Report\nversion 1.0.0\ndependency MathUtils =1.0.0\n",
        )
        .publish("MathUtils", "1.0.0")
        .publish("MathUtils", "2.0.0");

    let message = resolve_err(
        "dependency Report =1.0.0\ndependency MathUtils =2.0.0\n",
        &registry,
    );
    assert!(
        message.contains("MathUtils") && message.contains("1.0.0") && message.contains("2.0.0"),
        "the conflict has to name the module and both versions, got: {message}"
    );
    assert!(
        message.contains("two versions"),
        "the message has to say what the conflict is, got: {message}"
    );
}

/// Two lines pinning one module at two versions is the same conflict without a
/// module manifest to cause it, and it names the line each was written on.
#[test]
fn edge_two_pins_of_one_module_in_one_manifest_conflict() {
    let registry = two_versions();
    let message = resolve_err(
        "dependency MathUtils =1.0.0\ndependency MathUtils =2.0.0\n",
        &registry,
    );
    assert!(
        message.contains("=1.0.0") && message.contains("=2.0.0"),
        "both requirements have to be named as written, got: {message}"
    );
}

/// The same pin twice is not a conflict: it is one dependency written twice,
/// and it resolves the way it would have written once.
#[test]
fn edge_the_same_pin_twice_is_one_dependency() {
    let registry = two_versions();
    let resolved = resolve(
        "dependency MathUtils =1.0.0\ndependency MathUtils =1.0.0\n",
        &registry,
    );
    assert_eq!(version_of(&resolved, "MathUtils"), "1.0.0");
    assert_eq!(
        resolved.modules().count(),
        1,
        "the same pin twice is one resolved module, not two"
    );
}

/// A pin nothing publishes names what was asked for and what is there, so the
/// reader can see which of the two they meant.
#[test]
fn edge_unresolvable_pin_lists_the_published_versions() {
    let registry = two_versions();
    let message = resolve_err("dependency MathUtils =1.5.0\n", &registry);
    assert!(
        message.contains("1.5.0"),
        "the message has to name the version asked for, got: {message}"
    );
    assert!(
        message.contains("1.0.0") && message.contains("2.0.0"),
        "the message has to say what is published, got: {message}"
    );
}

/// A pin on a module nothing publishes at all is a different question from one
/// at a version that is missing, and says so.
#[test]
fn edge_pin_on_an_unpublished_module_is_refused() {
    let registry = two_versions();
    let message = resolve_err("dependency Nothing *\n", &registry);
    assert!(
        message.contains("Nothing") && message.contains("no version"),
        "the message has to say the module is unpublished, got: {message}"
    );
}

/// The lowest version there is, pinned exactly, is a version like any other.
#[test]
fn edge_zero_version_resolves_when_it_is_published() {
    let registry = Memory::default()
        .publish("MathUtils", "0.0.0")
        .publish("MathUtils", "0.0.1");
    let resolved = resolve("dependency MathUtils =0.0.0\n", &registry);
    assert_eq!(version_of(&resolved, "MathUtils"), "0.0.0");
}

/// A manifest with nothing in it resolves to nothing: a program that depends on
/// no module is not an error, and must not produce a lockfile line.
#[test]
fn edge_empty_manifest_resolves_to_nothing() {
    let resolved = resolve("", &two_versions());
    assert_eq!(resolved.modules().count(), 0);
    assert_eq!(
        resolved
            .lockfile()
            .lines()
            .filter(|line| line.starts_with("module"))
            .count(),
        0,
        "an empty manifest writes no lockfile entries"
    );
}

/// Only comments and blank lines is still an empty manifest.
#[test]
fn edge_comment_only_manifest_resolves_to_nothing() {
    let resolved = resolve(
        "# nothing yet\r\n\r\n   \r\n# still nothing\n",
        &two_versions(),
    );
    assert_eq!(resolved.modules().count(), 0);
}

/// A byte-order mark and CRLF endings are what a manifest gets when it has been
/// through a text editor, and neither is a reason to refuse it.
#[test]
fn edge_bom_and_crlf_manifest_parses() {
    let registry = two_versions();
    let parsed = manifest::parse_manifest(
        "\u{feff}name report\r\nversion 1.0.0\r\ndependency MathUtils =1.0.0\r\n",
    )
    .expect("a Windows-written manifest should parse");
    assert_eq!(parsed.name.as_deref(), Some("report"));
    let resolved = manifest::resolve(&parsed, &registry).expect("and it should resolve");
    assert_eq!(version_of(&resolved, "MathUtils"), "1.0.0");
}

/// Every way a manifest line can be wrong names the line and what was wrong
/// with it. A message without a line number leaves the reader to search.
#[test]
fn edge_malformed_manifest_lines_are_refused_by_line() {
    for (text, expected) in [
        ("name report\nnmae other\n", "'nmae' is not a field"),
        ("name\n", "is not a complete name line"),
        (
            "dependency MathUtils\n",
            "is not a complete dependency line",
        ),
        ("version 1.0\n", "is not a version"),
        ("dependency MathUtils ~1.0.0\n", "is not a requirement"),
    ] {
        let message = parse_err(text);
        assert!(
            message.contains(expected),
            "'{text}' should be refused saying '{expected}', got: {message}"
        );
        assert!(
            message.contains("line"),
            "'{text}' should say which line, got: {message}"
        );
    }
}

/// A manifest field written twice is a manifest that says two things about
/// itself, and the second one does not quietly win.
#[test]
fn edge_a_field_declared_twice_is_refused() {
    assert!(parse_err("name one\nname two\n").contains("declared twice"));
    assert!(parse_err("version 1.0.0\nversion 2.0.0\n").contains("declared twice"));
}

/// A module name no `import` could ever write is refused where it is written,
/// rather than at the import that could never find it.
///
/// `Math Utils` reaches the line-length check before the name check — the space
/// splits it into three words where a `dependency` line takes two — so it is
/// refused either way, with the message that describes the line it actually is.
#[test]
fn edge_module_name_that_is_not_an_identifier_is_refused() {
    for name in ["Math-Utils", "2Math", "依赖", "Math.Utils"] {
        let message = parse_err(&format!("dependency {name} =1.0.0\n"));
        assert!(
            message.contains("is not a module name"),
            "'{name}' should not be a module name, got: {message}"
        );
    }

    for (line, expected) in [
        ("dependency Math Utils =1.0.0", "carries 3 values"),
        ("dependency =1.0.0", "is not a complete dependency line"),
        ("name", "is not a complete name line"),
        ("version 1.0.0 extra", "carries 2 values"),
    ] {
        let message = parse_err(&format!("{line}\n"));
        assert!(
            message.contains(expected),
            "'{line}' should be refused saying '{expected}', got: {message}"
        );
    }

    let empty = parse_err("dependency  =1.0.0\n");
    assert!(
        empty.contains("is not a complete dependency line"),
        "a dependency line naming nothing is missing its value, got: {empty}"
    );
}

/// An empty file is a manifest with nothing in it, not a broken one.
#[test]
fn edge_empty_manifest_file_is_not_an_error() {
    let root = scratch("manifest_empty_file");
    manifest_file(&root, "");
    let resolved = manifest::resolve_in(&root).expect("an empty manifest resolves to nothing");
    assert_eq!(resolved.modules().count(), 0);
    assert!(
        !root.join(manifest::LOCKFILE).exists(),
        "nothing was resolved, so there is nothing to lock"
    );
}

/// A root with no manifest at all resolves to nothing and writes nothing: that
/// is every program written before there was a manifest, and their imports
/// keep working exactly as they did.
#[test]
fn edge_a_root_with_no_manifest_resolves_to_nothing() {
    let root = scratch("manifest_absent");
    let resolved = manifest::resolve_in(&root).expect("a missing manifest is not an error");
    assert_eq!(resolved.modules().count(), 0);
    assert!(!root.join(manifest::LOCKFILE).exists());
}

/// The lockfile is written on the first resolve, holds what the resolution
/// settled, and the second resolve writes the same bytes.
#[test]
fn lockfile_is_written_on_the_first_resolve_and_is_byte_identical_afterwards() {
    let root = scratch("manifest_lockfile");
    manifest_file(
        &root,
        "name report\nversion 1.0.0\ndependency MathUtils =2.0.0\ndependency SuiteKit *\n",
    );
    publish(&root, "MathUtils", "1.0.0", "set PI to 3.14");
    publish(&root, "MathUtils", "2.0.0", "set PI to 3.14159");
    publish(
        &root,
        "SuiteKit",
        "1.0.0",
        "set SUITE_KIT_NAME to \"SuiteKit\"",
    );
    publish(
        &root,
        "SuiteKit",
        "2.0.0",
        "set SUITE_KIT_NAME to \"SuiteKit\"",
    );

    let first = manifest::resolve_in(&root).expect("the manifest should resolve");
    let written = std::fs::read_to_string(root.join(manifest::LOCKFILE))
        .expect("the first resolve should write a lockfile");

    assert!(written.contains("module MathUtils 2.0.0 modules/MathUtils/2.0.0/MathUtils.rb"));
    assert!(written.contains("module SuiteKit 2.0.0 modules/SuiteKit/2.0.0/SuiteKit.rb"));
    assert!(
        written.find("MathUtils") < written.find("SuiteKit"),
        "the lockfile is in module-name order whatever order the manifest wrote them in: \
         {written}"
    );

    let second = manifest::resolve_in(&root).expect("and it resolves again");
    let rewritten = std::fs::read_to_string(root.join(manifest::LOCKFILE))
        .expect("the second resolve leaves the lockfile in place");
    assert_eq!(written, rewritten, "a re-resolve rewrites the same bytes");
    assert_eq!(first.lockfile(), second.lockfile());
}

/// The two programs of the finding, each with a manifest of its own, each
/// loading a different version of one fixture module from disk through the
/// real lexer and parser — so what is pinned is what parses, not a string a
/// test compared.
#[test]
fn two_programs_load_two_versions_of_one_fixture_module() {
    let one = scratch("manifest_load_v1");
    let two = scratch("manifest_load_v2");
    manifest_file(&one, "name old\nversion 0.1.0\ndependency Fixture =1.0.0\n");
    manifest_file(&two, "name new\nversion 0.1.0\ndependency Fixture =2.0.0\n");
    for root in [&one, &two] {
        publish(root, "Fixture", "1.0.0", "set VALUE to 1\n");
        publish(root, "Fixture", "2.0.0", "set VALUE to 2\n");
    }

    let mut values = Vec::new();
    for root in [&one, &two] {
        let resolved = manifest::resolve_in(root).expect("the manifest should resolve");
        let module = resolved.module("Fixture").expect("Fixture resolved");
        let path = manifest::DirRegistry::new(root).full_path(&module.path);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()));
        let program = redblue::parser::parse(
            redblue::lexer::Lexer::tokenize(&source)
                .unwrap_or_else(|error| panic!("the pinned module should lex: {error}")),
        )
        .unwrap_or_else(|error| panic!("the pinned module should parse: {error}"));
        assert!(
            !redblue::parser::module_body(&program).is_empty(),
            "the pinned module declares something to import"
        );
        values.push(source);
    }

    assert_ne!(
        values[0], values[1],
        "the two programs loaded different versions of the one module"
    );
    assert_eq!(values[0], "set VALUE to 1\n");
    assert_eq!(values[1], "set VALUE to 2\n");
}

/// A pin nothing publishes has to reach the user as the conflict the manifest
/// names. Without this, `import Fixture` binds no names, the next line reads one
/// of them, and the analyzer answers "Unknown variable 'VALUE'" — a message
/// about a variable, pointing at a variable, for a program whose real problem is
/// that its `redblue.manifest` cannot be resolved at all.
#[test]
fn edge_an_unresolvable_pin_is_an_analyzer_error_naming_the_conflict() {
    let root = scratch("manifest_analyzer_unresolvable");
    manifest_file(
        &root,
        "name report\nversion 1.0.0\ndependency Ghost =1.0.0\n",
    );

    let error = redblue::analyzer::analyze_in(&program("import Fixture\nsay VALUE\n"), &root)
        .expect_err("a pin nothing publishes is not a program that analyzes");

    assert!(
        error.message().contains("Ghost") && error.message().contains("1.0.0"),
        "the analyzer has to name the module and the version that could not be resolved, got: {}",
        error.message()
    );
    assert!(
        !error.message().contains("Unknown variable"),
        "the failure the manifest names is the failure the user has to be shown, not a \
         variable that went missing because of it: {}",
        error.message()
    );
}

/// The same masking, for the conflict this phase exists over: one module pinned
/// at two versions is a conflict whether the loader or the analyzer is looking.
#[test]
fn edge_two_versions_in_one_manifest_is_an_analyzer_error() {
    let root = scratch("manifest_analyzer_conflict");
    publish(&root, "Fixture", "1.0.0", "set VALUE to 1\n");
    publish(&root, "Fixture", "2.0.0", "set VALUE to 2\n");
    manifest_file(
        &root,
        "name report\nversion 1.0.0\ndependency Fixture =1.0.0\ndependency Fixture =2.0.0\n",
    );

    let error = redblue::analyzer::analyze_in(&program("import Fixture\nsay VALUE\n"), &root)
        .expect_err("two versions of one module is not a program that analyzes");

    assert!(
        error.message().contains("two versions"),
        "the analyzer has to say what the conflict is, got: {}",
        error.message()
    );
    assert!(
        !error.message().contains("Unknown variable"),
        "the conflict is the failure the user has to be shown, got: {}",
        error.message()
    );
}

/// The pin decides which file an `import` binds names from: the two published
/// versions of one fixture declare different names, and each program analyzes
/// against the version it pinned rather than against whichever file was newest.
#[test]
fn edge_an_import_binds_the_names_of_the_pinned_version() {
    let one = scratch("manifest_pinned_names_v1");
    let two = scratch("manifest_pinned_names_v2");
    manifest_file(&one, "name old\nversion 0.1.0\ndependency Fixture =1.0.0\n");
    manifest_file(&two, "name new\nversion 0.1.0\ndependency Fixture =2.0.0\n");
    for root in [&one, &two] {
        publish(root, "Fixture", "1.0.0", "set VALUE to 1\n");
        publish(root, "Fixture", "2.0.0", "set AMOUNT to 2\n");
    }

    redblue::analyzer::analyze_in(&program("import Fixture\nsay VALUE\n"), &one)
        .expect("the pinned version declares VALUE, so the read is of a bound name");
    redblue::analyzer::analyze_in(&program("import Fixture\nsay AMOUNT\n"), &two)
        .expect("the other pinned version declares AMOUNT");
    assert!(
        redblue::analyzer::analyze_in(&program("import Fixture\nsay AMOUNT\n"), &one).is_err(),
        "1.0.0 declares VALUE and not AMOUNT, so this read is of a name nothing bound"
    );
    assert!(
        redblue::analyzer::analyze_in(&program("import Fixture\nsay VALUE\n"), &two).is_err(),
        "2.0.0 declares AMOUNT and not VALUE, so this read is of a name nothing bound"
    );
}

/// A root with no manifest is every program written before there was one: the
/// analyzer has nothing to complain about, and adding a root to `analyze` must
/// not have made it complain about something else.
#[test]
fn edge_a_root_with_no_manifest_analyses_as_it_always_did() {
    let root = scratch("manifest_analyzer_absent");
    std::fs::create_dir_all(root.join("modules")).expect("the modules directory is creatable");
    std::fs::write(root.join("modules").join("Fixture.rb"), "set VALUE to 1\n")
        .expect("the module file is writable");

    redblue::analyzer::analyze_in(&program("import Fixture\nsay VALUE\n"), &root)
        .expect("a manifest-less program binds what it always bound");
    assert!(
        !root.join(manifest::LOCKFILE).exists(),
        "analyzing is not resolving: no manifest, no lockfile"
    );
}

/// A program that only imports, and reads nothing from the module, is the one
/// shape where a broken manifest used to surface. It still does, and through the
/// same message.
#[test]
fn edge_an_unresolvable_pin_is_reported_even_with_nothing_read() {
    let root = scratch("manifest_analyzer_bare_import");
    manifest_file(&root, "dependency Ghost =1.0.0\n");

    let error = redblue::analyzer::analyze_in(&program("import Fixture\n"), &root)
        .expect_err("a pin nothing publishes is not a program that analyzes");
    assert!(
        error.message().contains("Ghost"),
        "an import that binds nothing is still an import that failed, got: {}",
        error.message()
    );
}

// ---------------------------------------------------------------------------
// A conflict inside a body is the same conflict, and stops the walk there
// ---------------------------------------------------------------------------

/// The shapes an `import` can be written in below the top level.
///
/// Every one of them holds a body the walk enters, and a body the walk entered
/// and then kept walking after the conflict was recorded is how one unanswerable
/// import became a page of consequences again — a function, a method inside an
/// object, an `if`, a loop, a `try` and a declared `module` are all of it.
const BODIES: &[(&str, &str)] = &[
    (
        "function",
        "to read_it()\n    import Fixture\n    say VALUE\nend\nread_it\n",
    ),
    (
        "if",
        "if yes then\n    import Fixture\n    say VALUE\nend\n",
    ),
    (
        "unless",
        "unless no then\n    import Fixture\n    say VALUE\nend\n",
    ),
    (
        "for each",
        "for each n in [1]\n    import Fixture\n    say VALUE\nend\n",
    ),
    (
        "repeat",
        "repeat 1 times\n    import Fixture\n    say VALUE\nend\n",
    ),
    (
        "repeat until",
        "repeat\n    import Fixture\n    say VALUE\nuntil yes\n",
    ),
    (
        "while",
        "while no\n    import Fixture\n    say VALUE\nend\n",
    ),
    (
        "object method",
        "object Reporter\n    has tag default \"none\"\n    to report()\n        \
         import Fixture\n        say VALUE\n    end\nend\n",
    ),
    (
        "try",
        "try\n    import Fixture\n    say VALUE\ncatch error\n    say VALUE\nend\n",
    ),
    (
        "module",
        "module Report\n    import Fixture\n    say VALUE\nend\n",
    ),
    (
        "test",
        "test \"a body\"\n    import Fixture\n    say VALUE\nend\n",
    ),
];

/// An `import` inside a body binds what it binds anywhere else.
///
/// The counterpart to the test below, so that the failure that one reports is the
/// broken manifest's doing rather than the shape's: with a manifest that
/// resolves, every shape below analyzes and the names the import binds are
/// readable where they are written.
#[test]
fn edge_an_import_in_a_body_binds_names_when_the_manifest_resolves() {
    let root = scratch("manifest_body_healthy");
    manifest_file(
        &root,
        "name report\nversion 1.0.0\ndependency Fixture =1.0.0\n",
    );
    publish(&root, "Fixture", "1.0.0", "set VALUE to 1\n");

    for (shape, source) in BODIES {
        redblue::analyzer::analyze_in(&program(source), &root)
            .unwrap_or_else(|error| panic!("a {shape} body should analyze: {}", error.message()));
    }
}

/// An `import` inside a body is an import: when the manifest cannot be resolved,
/// the failure the user is shown is the conflict, not the names that went missing
/// because of it.
///
/// The stop was written for the top-level loop only, so every shape below it kept
/// walking after the conflict was recorded, and each read of a name the failed
/// import would have bound answered "unknown variable" — a message about a
/// variable, pointing at a variable, for a program whose real problem is in its
/// manifest.
#[test]
fn edge_an_unresolvable_pin_inside_a_body_is_the_conflict() {
    let root = scratch("manifest_body_unresolvable");
    manifest_file(
        &root,
        "name report\nversion 1.0.0\ndependency Ghost =1.0.0\n",
    );

    for (shape, source) in BODIES {
        let error = redblue::analyzer::analyze_in(&program(source), &root).expect_err(&format!(
            "an import inside a {shape} body is an import, and a pin nothing publishes \
                 is still a failure"
        ));
        assert!(
            error.message().contains("Ghost"),
            "a {shape} body has to name the conflict it found, got: {}",
            error.message()
        );
        assert!(
            !error.message().contains("Unknown variable"),
            "a {shape} body has to report the conflict rather than the names that went \
             missing because of it, got: {}",
            error.message()
        );
    }
}

/// The conflict is reported once.
///
/// A body that kept walking after it recorded the conflict produced one error per
/// unread name in the body, which is the page of consequences the whole mechanism
/// exists to avoid — and the reader was sent to the page rather than the cause.
#[test]
fn edge_a_conflict_in_a_body_is_reported_once() {
    let root = scratch("manifest_body_once");
    manifest_file(&root, "dependency Ghost =1.0.0\n");

    let error = redblue::analyzer::analyze_in(
        &program(
            "to read_it()\n    import Fixture\n    say VALUE\n    say OTHER\n    \
             say THIRD\nend\nread_it\n",
        ),
        &root,
    )
    .expect_err("the conflict is still a failure");

    assert_eq!(
        error.message().lines().count(),
        1,
        "one conflict is one error, not one error per unread name: {}",
        error.message()
    );
}

// ---------------------------------------------------------------------------
// Depth is how deep the chain goes, not how wide the module is
// ---------------------------------------------------------------------------

/// A chain one pin wide and deeper than the limit is refused.
///
/// `MAX_RESOLUTION_DEPTH` is documented as how deep one module's chain of
/// dependencies may go. It was enforced as how many pins one module's manifest
/// held, so a chain seventy long passed — seventy steps, far inside the step
/// budget — while a module with seventy pins was refused. The two are the
/// opposite of each other, and the chain is the one the constant is named for.
#[test]
fn edge_a_chain_deeper_than_the_limit_is_refused() {
    let message = resolve_err(
        "dependency Link0 =1.0.0\n",
        &chain(MAX_RESOLUTION_DEPTH + 5),
    );
    assert!(
        message.contains(&format!(
            "Dependency 'Link{}' is {} steps from this program",
            MAX_RESOLUTION_DEPTH,
            MAX_RESOLUTION_DEPTH + 1
        )),
        "the refusal has to name the first link past the limit and how deep it was, got: \
         {message}"
    );
    assert!(
        message.contains(&format!("walks at most {MAX_RESOLUTION_DEPTH}")),
        "the refusal has to name the limit it hit, got: {message}"
    );
}

/// The same chain, exactly as deep as the limit allows, resolves.
///
/// The limit is a boundary, and a boundary only means something if both sides of
/// it are pinned: refusing everything, or nothing, would satisfy the test above.
#[test]
fn edge_a_chain_exactly_at_the_depth_limit_resolves() {
    let resolved = resolve("dependency Link0 =1.0.0\n", &chain(MAX_RESOLUTION_DEPTH));
    assert_eq!(
        resolved.modules().count(),
        MAX_RESOLUTION_DEPTH,
        "every link of the chain is resolved, not only the first"
    );
    assert_eq!(
        version_of(&resolved, &format!("Link{}", MAX_RESOLUTION_DEPTH - 1)),
        "1.0.0",
        "the deepest link the limit allows is resolved like any other"
    );
}

/// A module holding more pins than the depth limit is *wide*, not deep, and a
/// wide manifest is one somebody wrote on purpose.
///
/// Width is the step budget's question — a hundred pins is a hundred steps, well
/// inside the budget — so a module with a hundred of them resolves. Refusing it
/// was the depth limit doing width's job, which is what it used to do.
#[test]
fn edge_a_module_with_more_pins_than_the_depth_limit_resolves() {
    let pins = MAX_RESOLUTION_DEPTH + 36; // a hundred: past the limit by half as much again
    let resolved = resolve("dependency Wide =1.0.0\n", &wide_module(pins));
    assert_eq!(
        resolved.modules().count(),
        pins + 1,
        "a hundred pins one module holds is width, and width is the step budget's question"
    );
}

/// A graph larger than the step budget is refused with a clean error.
///
/// The other of the two limits, and the one that needed a registry offering
/// fresh names forever to reach. It does not: the `Registry` trait is exactly the
/// seam for it, and a module manifest naming four thousand distinct leaves is
/// four thousand steps for a resolver to walk.
#[test]
fn edge_a_graph_larger_than_the_step_budget_is_refused() {
    let message = resolve_err(
        "dependency Wide =1.0.0\n",
        &wide_module(MAX_RESOLUTION_STEPS),
    );
    assert!(
        message.contains(&format!("did not finish in {MAX_RESOLUTION_STEPS} steps")),
        "the refusal has to name the budget it hit, got: {message}"
    );
}

/// One step under the budget is still resolved, so the refusal above is the
/// budget firing rather than the resolver refusing anything large.
#[test]
fn edge_a_graph_one_step_under_the_budget_resolves() {
    let resolved = resolve(
        "dependency Wide =1.0.0\n",
        &wide_module(MAX_RESOLUTION_STEPS - 2),
    );
    assert_eq!(
        resolved.modules().count(),
        MAX_RESOLUTION_STEPS - 1,
        "the whole graph settles, and the step it did not spend is the program's own pin"
    );
}

// ---------------------------------------------------------------------------
// Both engines resolve an import against the same root
// ---------------------------------------------------------------------------

/// The two programs of the finding, loaded through the bytecode VM from their own
/// directory: each imports the version *its* manifest pins, and the first is
/// loaded again afterwards to show nothing about the second stuck to it.
///
/// The VM resolved its imports against the working directory while the analyzer
/// resolved the same program's against the root it was handed, so the two engines
/// disagreed about every program not run from its own directory — and nothing
/// exercised that against a scratch root, which is how it survived.
#[test]
fn two_programs_load_their_pinned_version_on_the_bytecode_vm() {
    let one = scratch("manifest_bytecode_v1");
    let two = scratch("manifest_bytecode_v2");
    manifest_file(&one, "name old\nversion 0.1.0\ndependency Fixture =1.0.0\n");
    manifest_file(&two, "name new\nversion 0.1.0\ndependency Fixture =2.0.0\n");
    for root in [&one, &two] {
        publish(root, "Fixture", "1.0.0", "set VALUE to 1\n");
        publish(root, "Fixture", "2.0.0", "set VALUE to 2\n");
    }
    let source = "import Fixture\nsay VALUE\n";

    assert_eq!(
        run_on_bytecode_vm(&one, source),
        vec!["1".to_string()],
        "the first program's pin decides what its import loads"
    );
    assert_eq!(
        run_on_bytecode_vm(&two, source),
        vec!["2".to_string()],
        "the second program's pin decides what its import loads, not the first's"
    );
    assert_eq!(
        run_on_bytecode_vm(&one, source),
        vec!["1".to_string()],
        "a program run again after another one is answered from its own manifest"
    );
}

/// The bytecode VM and the analyzer are asked about the same program and the same
/// root, and agree about what the import binds and what it loads.
///
/// The two used to answer from different manifests — the analyzer from the `root`
/// it was given, the VM from the working directory — so a program run from
/// anywhere else was checked against one set of pins and run against another.
#[test]
fn edge_both_engines_agree_on_which_version_an_import_binds() {
    let root = scratch("manifest_both_engines");
    manifest_file(
        &root,
        "name report\nversion 1.0.0\ndependency Fixture =1.0.0\n",
    );
    publish(&root, "Fixture", "1.0.0", "set VALUE to 1\n");
    publish(&root, "Fixture", "2.0.0", "set AMOUNT to 2\n");
    let source = "import Fixture\nsay VALUE\n";

    redblue::analyzer::analyze_in(&program(source), &root)
        .expect("the pinned version declares VALUE, so the analyzer binds it");
    assert_eq!(
        run_on_bytecode_vm(&root, source),
        vec!["1".to_string()],
        "the version the analyzer bound names from is the version the VM loaded"
    );
    assert!(
        redblue::analyzer::analyze_in(&program("import Fixture\nsay AMOUNT\n"), &root).is_err(),
        "2.0.0 declares AMOUNT and not VALUE, and neither engine may pick it for this root"
    );
}

/// A manifest whose pins cannot be resolved is refused by the loader too, under
/// the root it was given — the same failure, from whichever engine is asked
/// first.
#[test]
fn edge_the_bytecode_vm_refuses_an_unresolvable_pin_from_its_own_root() {
    let root = scratch("manifest_bytecode_refusal");
    manifest_file(
        &root,
        "name report\nversion 1.0.0\ndependency Ghost =1.0.0\n",
    );

    let chunk = redblue::bytecode::compile_program(&program("import Fixture\n"))
        .expect("a bare import compiles");
    let mut vm = redblue::bytecode::vm::BytecodeVm::new_in(&root);
    let error = vm.run(&chunk).expect_err("the pin cannot be resolved");

    assert_eq!(error.label(), "RuntimeError");
    assert!(
        error.message().contains("Ghost") && !error.message().contains("Cannot find module"),
        "a pin nothing publishes is a manifest failure, not a missing file, got: {}",
        error.message()
    );
}

// ---------------------------------------------------------------------------
// A static pass reads; it does not resolve
// ---------------------------------------------------------------------------

/// Analyzing a program whose manifest resolves writes nothing.
///
/// The analysis resolves the pins to know which version each import binds names
/// from, and every resolve went through the writing path — so `rb lint`,
/// `rb format` and `analyze_in` each left a `redblue.lock` beside a program they
/// were only reading, once per import.
#[test]
fn edge_analysing_a_program_writes_no_lockfile() {
    let root = scratch("manifest_analysis_writes_nothing");
    manifest_file(
        &root,
        "name report\nversion 1.0.0\ndependency Fixture =1.0.0\n",
    );
    publish(&root, "Fixture", "1.0.0", "set VALUE to 1\n");
    publish(&root, "Fixture", "2.0.0", "set VALUE to 2\n");

    // Four imports, so that "once per import" would have written four times.
    let source = "import Fixture\nimport Fixture\nimport Fixture\nimport Fixture\n\
                  say VALUE\n";
    redblue::analyzer::analyze_in(&program(source), &root)
        .expect("the manifest resolves, so the program analyzes");
    assert!(
        !root.join(manifest::LOCKFILE).exists(),
        "a static pass answers its question by reading; it does not write a lockfile"
    );

    // The writing path is still the one that writes, so this is a pass that
    // stopped writing rather than a pass that stopped resolving.
    manifest::resolve_in(&root).expect("the resolving path still resolves");
    assert!(
        root.join(manifest::LOCKFILE).exists(),
        "resolving is what writes the lockfile"
    );
}

/// A failure the analyzer reports is one failure, not one carrying the label of
/// the error it came from.
///
/// The manifest's error was rendered with `Error::to_string` — `RuntimeError:
/// Cannot resolve …` — and stored in an `Analyzer` error, so the user read
/// `AnalyzerError: RuntimeError: Cannot resolve dependency 'Ghost' …` for one
/// conflict: two labels, neither of which was theirs.
#[test]
fn edge_a_manifest_failure_is_reported_once_without_a_second_label() {
    let root = scratch("manifest_one_label");
    manifest_file(
        &root,
        "name report\nversion 1.0.0\ndependency Ghost =1.0.0\n",
    );

    let error = redblue::analyzer::analyze_in(&program("import Fixture\nsay VALUE\n"), &root)
        .expect_err("the pin cannot be resolved");

    assert_eq!(
        error.label(),
        "AnalyzerError",
        "the analyzer reports this, so it is an analyzer failure"
    );
    assert!(
        !error.message().contains("RuntimeError"),
        "the label of the error the message came from must not be printed inside it, got: {}",
        error.message()
    );
    assert!(
        error
            .message()
            .starts_with("Cannot resolve dependency 'Ghost'"),
        "the message is the manifest's own, unprefixed, got: {}",
        error.message()
    );
    let span = error
        .span()
        .expect("a failure found at an import is reported at that import");
    assert_eq!(span.line, 1, "the caret points at the import that found it");
}

/// A manifest whose *syntax* is wrong is reported against the line it is wrong
/// on, in the message, with no second label — and the caret stays on the program,
/// because that is the text a caller renders it against.
///
/// A `Span` is a position in a source text, drawn as a caret in one. The manifest
/// is a second file: a manifest line carried as a span would render as
/// `--> report.rb:2:1` over a line of the *program*, which is not line 2 of
/// anything the reader was looking at.
#[test]
fn edge_a_malformed_manifest_keeps_its_line_and_the_import_keeps_the_caret() {
    let root = scratch("manifest_malformed_line");
    manifest_file(&root, "name report\nnmae other\n");

    let error = redblue::analyzer::analyze_in(&program("import Fixture\n"), &root)
        .expect_err("a manifest that does not parse is a program that does not analyze");

    assert!(
        error.message().contains("redblue.manifest line 2"),
        "the line the manifest is wrong on travels with the message, got: {}",
        error.message()
    );
    assert!(
        !error.message().contains("RuntimeError"),
        "one failure, one label, got: {}",
        error.message()
    );
    let span = error
        .span()
        .expect("the failure is reported at the import that found it");
    assert_eq!(
        span.line, 1,
        "the caret is drawn on the program being analyzed, never on a line of another file"
    );
}

/// Two roots resolved in one process keep their own pins, in either order and on
/// either side of a repeat.
///
/// The resolution used to be cached per thread and keyed on nothing at all, so
/// the second program in a process — a REPL line, a test harness running files one
/// after another — was answered from the first program's pins, or from the first
/// program's failure, for the rest of the process.
#[test]
fn edge_two_roots_in_one_process_keep_their_own_pins() {
    let one = scratch("manifest_process_v1");
    let two = scratch("manifest_process_v2");
    manifest_file(&one, "name old\nversion 0.1.0\ndependency Fixture =1.0.0\n");
    manifest_file(&two, "name new\nversion 0.1.0\ndependency Fixture =2.0.0\n");
    for root in [&one, &two] {
        publish(root, "Fixture", "1.0.0", "set VALUE to 1\n");
        publish(root, "Fixture", "2.0.0", "set AMOUNT to 2\n");
    }
    let reads_value = "import Fixture\nsay VALUE\n";
    let reads_amount = "import Fixture\nsay AMOUNT\n";

    for _ in 0..2 {
        redblue::analyzer::analyze_in(&program(reads_value), &one)
            .expect("the first program's pin declares VALUE");
        redblue::analyzer::analyze_in(&program(reads_amount), &two)
            .expect("the second program's pin declares AMOUNT");
        assert!(
            redblue::analyzer::analyze_in(&program(reads_amount), &one).is_err(),
            "AMOUNT is bound by the second program's pin, not the first's"
        );
        assert!(
            redblue::analyzer::analyze_in(&program(reads_value), &two).is_err(),
            "VALUE is bound by the first program's pin, not the second's"
        );
    }
}

// ---------------------------------------------------------------------------
// `rb run` resolves against the program, not against the working directory
// ---------------------------------------------------------------------------

/// Runs the file at `path` the way `rb run` does — [`redblue::run_file`] — and
/// returns what it wrote to `record`.
///
/// Through a file rather than [`redblue::run_source_in`] on a string, because
/// the whole of this section is about the *file* having a directory: the root is
/// derived from the path, and a string has no path to derive it from. The
/// program records the value it loaded rather than printing it, because
/// `run_file` prints and returns only whether it succeeded.
#[track_caller]
fn run_file_recording(root: &Path, relative: &str, record: &Path) -> String {
    if let Some(parent) = root.join(relative).parent() {
        std::fs::create_dir_all(parent).expect("the program's directory should be creatable");
    }
    let path = root.join(relative);
    let program = format!(
        "import Fixture\nfiles.write(\"{}\", VALUE)\n",
        record.display()
    );
    std::fs::write(&path, program).expect("the program should be written");

    redblue::run_file(path.to_str().expect("a utf-8 scratch path"))
        .unwrap_or_else(|error| panic!("{} should run, failed with {error}", path.display()));
    std::fs::read_to_string(record).expect("the program should have written what it loaded")
}

/// The finding itself: a program at `subdir/prog.rb` resolves against
/// `subdir/redblue.manifest`.
///
/// Every other test of this phase drove a rooted API and handed it the root. The
/// production path did not — `run_file` read the file and then ran it against
/// the working directory — so `rb run subdir/prog.rb` consulted
/// `./redblue.manifest` and loaded `./modules/Fixture/1.0.0/Fixture.rb` while the
/// two rooted APIs the phase added were tested against roots nobody ran anything
/// through. This is the two-programs-two-versions scenario as a user writes it,
/// from a directory that is not the program's own.
#[test]
fn running_a_program_resolves_the_manifest_beside_it() {
    let root = scratch("manifest_run_file_root");
    let program_root = root.join("subdir");
    std::fs::create_dir_all(&program_root).expect("the program's directory should be creatable");

    // The manifest where the program is, and one where it is not. Both name the
    // same module, so the two disagree about what `import Fixture` means.
    manifest_file(
        &program_root,
        "name old\nversion 0.1.0\ndependency Fixture =1.0.0\n",
    );
    manifest_file(
        &root,
        "name other\nversion 0.1.0\ndependency Fixture =2.0.0\n",
    );
    publish(
        &program_root,
        "Fixture",
        "1.0.0",
        "set VALUE to \"beside\"\n",
    );
    publish(
        &program_root,
        "Fixture",
        "2.0.0",
        "set VALUE to \"above\"\n",
    );

    let record = root.join("loaded.txt");
    let loaded = run_file_recording(&root, "subdir/prog.rb", &record);

    assert_eq!(
        loaded, "beside",
        "a program resolves the manifest beside it, not the one in the working directory"
    );
}

/// The other half: the directory a program is *not* in cannot change what its
/// imports mean.
///
/// The failing shape above can also be reached from the other side — a manifest
/// in the working directory with no manifest at all beside the program — and the
/// two are not the same claim. The first says the program's own manifest wins;
/// this one says the absence of one falls back to the old search paths rather
/// than to a manifest the program never mentioned.
#[test]
fn edge_a_program_with_no_manifest_of_its_own_still_imports() {
    let root = scratch("manifest_run_file_bare");
    let program_root = root.join("subdir");
    std::fs::create_dir_all(&program_root).expect("the program's directory should be creatable");

    // A manifest in the working directory pinning a version the program's own
    // directory does not have.
    manifest_file(
        &root,
        "name other\nversion 0.1.0\ndependency Fixture =2.0.0\n",
    );
    publish(&root, "Fixture", "2.0.0", "set VALUE to \"wrong\"\n");
    // And the plain, pre-manifest shape the search has always answered: a module
    // file beside the program at `modules/Fixture.rb`, reached with no manifest
    // at all.
    std::fs::create_dir_all(program_root.join("modules"))
        .expect("the modules directory should be creatable");
    std::fs::write(
        program_root.join("modules").join("Fixture.rb"),
        "set VALUE to \"bare\"\n",
    )
    .expect("the module file should be written");

    let record = root.join("loaded.txt");
    let loaded = run_file_recording(&root, "subdir/prog.rb", &record);

    assert_eq!(
        loaded, "bare",
        "a program with no manifest is unaffected: its own modules/Fixture.rb is what it imports"
    );
}

/// All three engines agree about the program a file names, and the lockfile
/// lands beside the program rather than in the working directory.
///
/// `analyze_in`, the tree-walking VM and the bytecode VM each took a root, and
/// `run_file` took none: the file's own pipeline resolved against `.` while the
/// two rooted engines resolved against the program's directory. This asks all
/// three about one file and reads the lockfile each of them should have written.
#[test]
fn edge_every_engine_agrees_about_the_program_a_file_names() {
    let root = scratch("manifest_run_file_engines");
    let program_root = root.join("nested");
    std::fs::create_dir_all(&program_root).expect("the program's directory should be creatable");
    manifest_file(
        &program_root,
        "name old\nversion 0.1.0\ndependency Fixture =1.0.0\n",
    );
    manifest_file(
        &root,
        "name other\nversion 0.1.0\ndependency Fixture =2.0.0\n",
    );
    publish(&program_root, "Fixture", "1.0.0", "set VALUE to \"one\"\n");
    publish(&program_root, "Fixture", "2.0.0", "set AMOUNT to \"two\"\n");
    let source = "import Fixture\nsay VALUE\n";

    redblue::analyzer::analyze_in(&program(source), &program_root)
        .expect("the program's own pin declares VALUE");
    assert!(
        redblue::analyzer::analyze_in(&program("import Fixture\nsay AMOUNT\n"), &program_root)
            .is_err(),
        "the other manifest's version is not this program's"
    );

    let record = root.join("loaded.txt");
    assert_eq!(
        run_file_recording(&root, "nested/prog.rb", &record),
        "one",
        "the loader resolves the same pin the analyzer was given"
    );
    assert!(
        program_root.join(manifest::LOCKFILE).exists(),
        "the lockfile is written beside the program whose manifest it locks"
    );
    assert!(
        !root.join(manifest::LOCKFILE).exists(),
        "and not in the working directory, which is not this program's"
    );
}

// ---------------------------------------------------------------------------
// The lockfile is read, not only written
// ---------------------------------------------------------------------------

/// The lockfile the first resolve writes is the lockfile the second resolve
/// reads back and agrees with.
///
/// The file was written by `write_lockfile` and never parsed by anything, so it
/// was a log of the last resolve rather than a lock. This is the round trip:
/// what `Resolution::lockfile` writes, [`manifest::parse_lockfile`] reads, with
/// the same names, versions and paths in the same order.
#[test]
fn the_lockfile_round_trips_through_its_own_parser() {
    let root = scratch("manifest_lockfile_round_trip");
    manifest_file(
        &root,
        "name report\nversion 1.0.0\ndependency MathUtils =1.0.0\ndependency SuiteKit *\n",
    );
    publish(&root, "MathUtils", "1.0.0", "set PI to 3.14");
    publish(&root, "MathUtils", "2.0.0", "set PI to 3.14159");
    publish(&root, "SuiteKit", "1.0.0", "set SUITE to 1");

    let resolution = manifest::resolve_in(&root).expect("the manifest should resolve");
    let locked =
        manifest::parse_lockfile(&resolution.lockfile()).expect("what was written should parse");

    assert_eq!(locked.len(), 2, "both pins are in the lockfile");
    assert_eq!(
        locked,
        vec![
            manifest::LockedModule {
                name: "MathUtils".to_string(),
                version: Version::parse("1.0.0").unwrap(),
                path: "modules/MathUtils/1.0.0/MathUtils.rb".to_string(),
            },
            manifest::LockedModule {
                name: "SuiteKit".to_string(),
                version: Version::parse("1.0.0").unwrap(),
                path: "modules/SuiteKit/1.0.0/SuiteKit.rb".to_string(),
            },
        ],
        "what the resolution settles and what the lockfile holds are the same thing"
    );
}

/// A version published *after* the lock was written is refused, not handed to
/// the program in silence.
///
/// This is what a lockfile is for. `dependency Fixture *` resolved to `1.0.0`
/// and wrote `module Fixture 1.0.0 …`. Publishing `2.0.0` and running the
/// program again used to give it `2.0.0` — different code, no message, and a
/// lockfile that still said `1.0.0`. The disagreement is the finding: the world
/// moved under the lock.
#[test]
fn edge_a_version_published_after_the_lock_is_refused() {
    let root = scratch("manifest_lockfile_moved");
    manifest_file(&root, "name report\nversion 0.1.0\ndependency Fixture *\n");
    publish(&root, "Fixture", "1.0.0", "set VALUE to 1\n");

    let first = manifest::resolve_in(&root).expect("the first resolve locks 1.0.0");
    let written = manifest::parse_lockfile(&first.lockfile()).expect("what was written parses");
    assert_eq!(written[0].version.to_string(), "1.0.0");

    publish(&root, "Fixture", "2.0.0", "set VALUE to 2\n");
    let moved = manifest::resolve_in(&root)
        .expect_err("a version published under a lock does not quietly replace it");

    assert!(
        moved.message().contains("Fixture") && moved.message().contains("1.0.0"),
        "the refusal has to name the module and the version it was held at, got: {}",
        moved.message()
    );
    assert!(
        moved.message().contains("2.0.0"),
        "and the version it would have been handed instead, got: {}",
        moved.message()
    );
    assert!(
        moved.message().contains(manifest::LOCKFILE),
        "and the file to delete, because refusing is not a way out by itself, got: {}",
        moved.message()
    );
    assert_eq!(
        std::fs::read_to_string(root.join(manifest::LOCKFILE)).ok(),
        Some(first.lockfile()),
        "a refused resolve writes nothing: the lock it could not honour is still there"
    );
}

/// Deleting the lockfile is the way out, and it works.
///
/// A lock that cannot be honoured has to be escapable, or the refusal is a dead
/// end the user has to answer with an editor. This is the same program as the
/// test above, one step further on.
#[test]
fn edge_deleting_the_lockfile_re_resolves() {
    let root = scratch("manifest_lockfile_escape");
    manifest_file(&root, "name report\nversion 0.1.0\ndependency Fixture *\n");
    publish(&root, "Fixture", "1.0.0", "set VALUE to 1\n");
    manifest::resolve_in(&root).expect("the first resolve locks 1.0.0");
    publish(&root, "Fixture", "2.0.0", "set VALUE to 2\n");
    manifest::resolve_in(&root).expect_err("the world moved under the lock");

    std::fs::remove_file(root.join(manifest::LOCKFILE)).expect("the lockfile should be removable");
    let after = manifest::resolve_in(&root).expect("with the lock gone there is nothing to refuse");

    assert_eq!(
        after.module("Fixture").map(|m| m.version.to_string()),
        Some("2.0.0".to_string()),
        "deleting the lock is the re-resolve, and it takes what is published now"
    );
}

/// A lockfile that disagrees because a pin was *removed* is refused too.
///
/// The other direction of the same question: the manifest no longer asks for the
/// module the lock holds. Rewriting the lock silently would make `redblue.lock`
/// a record of the last manifest rather than an agreement with it, and a reader
/// of the lockfile would have no way to tell which one it was looking at.
#[test]
fn edge_a_locked_module_the_manifest_no_longer_pins_is_refused() {
    let root = scratch("manifest_lockfile_dropped");
    manifest_file(
        &root,
        "name report\nversion 0.1.0\ndependency Fixture =1.0.0\ndependency SuiteKit =1.0.0\n",
    );
    publish(&root, "Fixture", "1.0.0", "set VALUE to 1\n");
    publish(&root, "SuiteKit", "1.0.0", "set SUITE to 1\n");
    manifest::resolve_in(&root).expect("the first resolve locks both");

    manifest_file(
        &root,
        "name report\nversion 0.1.0\ndependency Fixture =1.0.0\n",
    );
    let dropped = manifest::resolve_in(&root)
        .expect_err("a pin the manifest no longer has is a disagreement");

    assert!(
        dropped.message().contains("SuiteKit"),
        "the refusal names the module that fell out, got: {}",
        dropped.message()
    );
    assert!(
        dropped.message().contains("does not resolve it at all"),
        "and says which way the two disagree, got: {}",
        dropped.message()
    );
}

/// A lockfile nothing but `rb` wrote is refused rather than half-read.
///
/// The parser is what makes a lockfile a lock, so it has to hold to the same
/// standard as the manifest beside it: a line that is not a `module` line, one
/// that is missing a version, and a version that is not a version are each a
/// named failure rather than a silently skipped line.
#[test]
fn edge_a_malformed_lockfile_is_refused_by_line() {
    for (text, expected) in [
        ("modulex Fixture 1.0.0 p\n", "is not a field a lockfile has"),
        ("module Fixture\n", "is not a complete module line"),
        ("module Fixture 1.0 p\n", "is not a version"),
        ("module Math.Utils 1.0.0 p\n", "is not a module name"),
    ] {
        let message = manifest::parse_lockfile(text)
            .err()
            .unwrap_or_else(|| panic!("'{text}' should not parse"))
            .message()
            .to_string();
        assert!(
            message.contains(expected),
            "'{text}' should be refused saying '{expected}', got: {message}"
        );
        assert!(
            message.contains("line 1"),
            "'{text}' should say which line, got: {message}"
        );
    }
}

/// A lockfile that was written by the same resolution, with a comment and a
/// blank line in it, is not a disagreement.
///
/// Comments and blank lines are what the writer emits, so a parser that refused
/// them would refuse every lockfile it had itself written.
#[test]
fn edge_a_lockfile_with_comments_and_blank_lines_parses() {
    let locked = manifest::parse_lockfile(
        "# redblue.lock — what this program's dependencies resolved to.\n\
         # Written by rb.\n\
         \n\
         module Fixture 1.0.0 modules/Fixture/1.0.0/Fixture.rb\n\
         \n",
    )
    .expect("what the writer emits has to parse");

    assert_eq!(
        locked.len(),
        1,
        "one module line, whatever else is in the file"
    );
    assert_eq!(locked[0].name, "Fixture");
}

// ---------------------------------------------------------------------------
// The resolver chooses; it does not trust the order it was handed
// ---------------------------------------------------------------------------

/// A registry that offers its versions newest-first still has `*` resolve to the
/// highest.
///
/// `Requirement::Any` took `available.last()` and the [`Registry`] trait
/// documented that it would be handed versions in ascending order. Both bundled
/// registries sort, so this passed everywhere it could be reached — and any
/// third-party registry offering `1.10.0, 1.9.0` would have had `*` resolve to
/// `1.9.0` with nothing to say so.
#[test]
fn edge_wildcard_takes_the_highest_version_whichever_order_they_arrive_in() {
    for order in [["2.0.0", "1.0.0"], ["1.0.0", "2.0.0"]] {
        let registry = Unordered::new(
            order
                .iter()
                .map(|version| (*version).to_string())
                .collect::<Vec<String>>(),
        );

        let resolved = resolve("dependency MathUtils *\n", &registry);
        assert_eq!(
            version_of(&resolved, "MathUtils"),
            "2.0.0",
            "offered as {order:?}, `*` still takes the highest version"
        );
    }
}

/// A registry offering versions newest-first is the one that breaks a numeric
/// comparison written as text, so `1.10.0` against `1.9.0` is the case worth
/// naming.
#[test]
fn edge_wildcard_is_numeric_however_the_registry_orders_them() {
    let registry = Unordered::new(vec!["1.10.0".to_string(), "1.9.0".to_string()]);

    assert_eq!(
        version_of(&resolve("dependency MathUtils *\n", &registry), "MathUtils"),
        "1.10.0",
        "1.10.0 is above 1.9.0 numerically, and a text comparison would say otherwise"
    );
}

/// A registry whose offer is *not* a two-way street is still asked honestly: the
/// message naming what is published lists what it was actually given.
#[test]
fn edge_the_unpublished_message_lists_what_the_registry_offered() {
    let registry = Unordered::new(vec!["2.0.0".to_string(), "1.0.0".to_string()]);
    let message = resolve_err("dependency MathUtils =3.0.0\n", &registry);

    assert!(
        message.contains("2.0.0") && message.contains("1.0.0"),
        "every published version is named, got: {message}"
    );
}

// ---------------------------------------------------------------------------
// One manifest read per program
// ---------------------------------------------------------------------------

/// The tree-walking VM resolves the manifest once for a program, not once per
/// `import`.
///
/// Each `import` used to call `pinned_module_path_in`, which re-reads
/// `redblue.manifest`, re-scans `modules/*/*` and re-compares the lockfile — for
/// one answer it had already been given. The analyzer had been fixed to read once
/// for the whole program; the loader had not. This asks the question in a way
/// that has exactly one right answer: a program that deletes its own manifest
/// between its first and second import. A resolver that re-reads finds nothing
/// the second time and the pin is gone; a resolver that read once for the program
/// answers both imports from the pins the program started with.
#[test]
fn edge_the_tree_walking_vm_resolves_the_manifest_once_per_program() {
    let root = scratch("manifest_vm_resolved_once");
    manifest_file(
        &root,
        "name report\nversion 0.1.0\ndependency Fixture =1.0.0\n",
    );
    publish(&root, "Fixture", "1.0.0", "set VALUE to 1\n");
    publish(&root, "Fixture", "2.0.0", "set AMOUNT to 2\n");
    let manifest = root.join(manifest::MANIFEST_FILE);

    let source = format!(
        "import Fixture\n\
         files.delete(\"{}\")\n\
         import Fixture\n\
         say VALUE\n",
        manifest.display()
    );
    let chunk =
        redblue::bytecode::compile_program(&program(&source)).expect("the program should compile");

    // The bytecode VM first: it is the engine whose `IMPORT` opcode resolved per
    // instruction, and the program runs as compiled so no analyzer stands
    // between the loader and the manifest.
    let mut vm = redblue::bytecode::vm::BytecodeVm::new_in(&root);
    vm.set_echo(false);
    let result = vm.run(&chunk);
    assert!(
        result.is_ok(),
        "the second import is answered from the pins the program resolved at its start: {:?}",
        result.err()
    );
    assert_eq!(
        vm.take_output(),
        vec!["1".to_string()],
        "and it loaded the version the program started with"
    );

    // And the tree-walker, which is what `rb run` actually runs.
    let root = scratch("manifest_vm_resolved_once_tree");
    manifest_file(
        &root,
        "name report\nversion 0.1.0\ndependency Fixture =1.0.0\n",
    );
    publish(&root, "Fixture", "1.0.0", "set VALUE to 1\n");
    publish(&root, "Fixture", "2.0.0", "set AMOUNT to 2\n");
    let manifest = root.join(manifest::MANIFEST_FILE);
    let source = format!(
        "import Fixture\n\
         files.delete(\"{}\")\n\
         import Fixture\n\
         say VALUE\n",
        manifest.display()
    );

    let (mut vm, result) = redblue::run_isolated_in(&program(&source), &root);
    assert!(
        result.is_ok(),
        "the tree-walking loader resolves the manifest once for the program too: {:?}",
        result.err()
    );
    assert_eq!(
        vm.take_output(),
        vec!["1".to_string()],
        "and it loaded the version the program started with"
    );
}
// ---------------------------------------------------------------------------
// The finding, with `rb run` in it rather than a rooted API
// ---------------------------------------------------------------------------

/// Two programs, two roots, two pins, one process — each loaded the way a user
/// loads one, from a file, in whatever order the files are run.
///
/// The phase's own headline scenario, and the reason the root was threaded at
/// all: without it this only worked after a `cd`.
#[test]
fn two_programs_run_from_their_own_directories_pin_two_versions() {
    let one = scratch("manifest_run_one");
    let two = scratch("manifest_run_two");
    manifest_file(&one, "name old\nversion 0.1.0\ndependency Fixture =1.0.0\n");
    manifest_file(&two, "name new\nversion 0.1.0\ndependency Fixture =2.0.0\n");
    for root in [&one, &two] {
        publish(root, "Fixture", "1.0.0", "set VALUE to \"one\"\n");
        publish(root, "Fixture", "2.0.0", "set VALUE to \"two\"\n");
    }

    let first = run_file_recording(&one, "prog.rb", &one.join("got.txt"));
    let second = run_file_recording(&two, "prog.rb", &two.join("got.txt"));

    assert_eq!(first, "one", "the first program loaded its own pin");
    assert_eq!(
        second, "two",
        "the second program loaded its own pin, not the first's"
    );
}

// ---------------------------------------------------------------------------
// Module files are read through the filesystem abstraction
// ---------------------------------------------------------------------------

/// One at a time, because the in-memory filesystem is a process-wide singleton
/// and the tests using it run in parallel threads of this one process.
fn memory_filesystem() -> std::sync::MutexGuard<'static, ()> {
    static STORE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    STORE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The module loader asks two questions of a filesystem, and on
/// `wasm32-unknown-unknown` — where `std::fs` compiles and every call fails —
/// both have to be answerable by the in-memory one.
///
/// `read_to_string` for the module's bytes and `is_file` for whether a search
/// path names a file at all. `is_file` is the newer of the two and the one a
/// flat map gets wrong: `modules/Fixture/` is implied by the version files
/// underneath it, so `exists` answers `true` for it and only the distinction
/// between "stored" and "implied" keeps the import search from opening a
/// directory.
#[test]
fn edge_the_module_search_can_be_answered_from_the_in_memory_filesystem() {
    use redblue::vfs::memory;

    let _serialised = memory_filesystem();
    memory::clear();

    memory::write("modules/Fixture/1.0.0/Fixture.rb", "set VALUE to 1")
        .expect("a module file should be writable");

    let module = "modules/Fixture/1.0.0/Fixture.rb";
    assert!(
        memory::is_file(module),
        "a stored path is a file, which is what the import search opens"
    );
    assert_eq!(
        memory::read_to_string(module).expect("the module file should be readable"),
        "set VALUE to 1",
        "and its bytes come out of the same map a files.write put them in"
    );

    assert!(
        !memory::is_file("modules/Fixture"),
        "modules/Fixture/ is implied by the version files under it, but it is a directory and the \
         search must skip it rather than open it"
    );
    assert!(
        memory::exists("modules/Fixture"),
        "it does exist, which is the other question — so the two have to be different functions"
    );
    assert!(
        !memory::is_file("modules/Nothing.rb"),
        "a path nothing wrote is not a file, and the search has to go on to the next one"
    );

    memory::clear();
}

/// A root is joined to a search path, and the join is what both engines open.
///
/// The two shapes of "no root" are spelled the way they arrive — an empty parent
/// from `Path::parent` for a bare `prog.rb`, and `.` from the working directory
/// — and both answer the search path itself rather than a path with a `./` in
/// front of it. That is not tidiness: on the playground the filesystem is a map
/// from path *text*, and `modules/Fixture.rb` and `./modules/Fixture.rb` would
/// be two entries where the program wrote one.
#[test]
fn edge_a_root_is_joined_to_a_search_path_the_way_a_program_names_it() {
    use redblue::vfs::rooted;

    assert_eq!(
        rooted(Path::new(""), "modules/Fixture.rb"),
        "modules/Fixture.rb",
        "the working directory is a path relative to itself"
    );
    assert_eq!(
        rooted(Path::new("."), "modules/Fixture.rb"),
        "modules/Fixture.rb",
        "and so is `.`, written as a path"
    );
    assert_eq!(
        rooted(Path::new("."), "./modules/Fixture"),
        "modules/Fixture",
        "a search path written with a leading `./` names the same file"
    );
    assert_eq!(
        rooted(Path::new("subdir"), "modules/Fixture.rb"),
        "subdir/modules/Fixture.rb",
        "a program in a subdirectory reads its modules from its subdirectory"
    );
    assert_eq!(
        rooted(Path::new("a/b"), "./modules/Fixture.rb"),
        "a/b/modules/Fixture.rb",
        "and a rooted path is joined to a relative one without doubling the separator"
    );
}

/// No loader reaches for `std::fs` behind the abstraction's back.
///
/// Every other test in this file runs natively, where `crate::vfs` *is*
/// `std::fs` and the two are indistinguishable — so a `std::fs::read_to_string`
/// left in a module loader passes every test in the suite and fails on
/// `wasm32-unknown-unknown`, the one target with no filesystem, where the same
/// program loads fine through `rb run`. That is the whole of the invariant, and
/// it cannot be observed from behaviour on this target at all.
///
/// So it is observed from the source. The three readers are named because the
/// wasm target compiles with each of them present and working in the other two
/// builds.
#[test]
fn edge_no_module_loader_reads_a_file_behind_the_filesystem_abstraction() {
    for (file, readers) in [
        (include_str!("../src/interpreter.rs"), "src/interpreter.rs"),
        (include_str!("../src/runtime.rs"), "src/runtime.rs"),
        (include_str!("../src/bytecode/vm.rs"), "src/bytecode/vm.rs"),
    ] {
        for needle in [
            "std::fs::read_to_string",
            "std::path::Path::new(&path).is_file()",
        ] {
            assert!(
                !file.contains(needle),
                "{readers} reaches the filesystem with `{needle}`, which compiles on every target \
                 and fails only on the one with no filesystem"
            );
        }
    }

    // And the three that are allowed to say the name, because they *are* the
    // abstraction.
    for reader in [
        include_str!("../src/runtime.rs"),
        include_str!("../src/interpreter.rs"),
    ] {
        assert!(
            reader.contains("crate::vfs::read_to_string"),
            "the module reader must be the one `crate::vfs` re-exports per target"
        );
    }
}
