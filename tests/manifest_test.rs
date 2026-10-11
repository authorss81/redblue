//! `redblue.manifest` — name, version, dependencies — and the lockfile a
//! resolution writes.
//!
//! Before this, `import Foo` looked for `modules/Foo.rb` and nothing else: one
//! file, no version, nothing to pin, and no way for two programs to depend on
//! two versions of one module. These tests pin the manifest schema, the
//! resolver, and the bytes of the lockfile.

use std::path::{Path, PathBuf};

use redblue::manifest::{self, Registry, Version};

/// A module published to a registry, with the manifest it publishes beside it.
struct Published {
    /// The manifest the module publishes, if any.
    manifest: Option<String>,
}

/// A registry held in memory, so a resolution can be exercised without the
/// filesystem: which version of which module is where is the whole question,
/// and asking it of a directory instead would add a way for it to fail for the
/// wrong reason.
#[derive(Default)]
struct Memory {
    modules: std::collections::BTreeMap<(String, String), Published>,
}

impl Memory {
    /// Publishes `name` at `version`.
    fn publish(mut self, name: &str, version: &str) -> Self {
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
        let mut found: Vec<Version> = self
            .modules
            .keys()
            .filter(|(module, _)| module == name)
            .map(|(_, version)| Version::parse(version).expect("a fixture version"))
            .collect();
        found.sort();
        found
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

/// Resolves `text` against `registry`, failing the test rather than the phase.
#[track_caller]
fn resolve(text: &str, registry: &Memory) -> manifest::Resolution {
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
fn resolve_err(text: &str, registry: &Memory) -> String {
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
#[test]
fn edge_module_name_that_is_not_an_identifier_is_refused() {
    for name in [
        "Math-Utils",
        "2Math",
        "Math Utils",
        "依赖",
        "Math.Utils",
        "",
    ] {
        let message = parse_err(&format!("dependency {name} =1.0.0\n"));
        assert!(
            message.contains("is not a module name") || message.contains("is not a complete"),
            "'{name}' should not be a module name, got: {message}"
        );
    }
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
