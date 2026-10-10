//! VS Code extension manifest (`tooling/vscode/`): the snippets the editor
//! completes with, the `redblue.run` command and the keybinding that fires it.
//!
//! The manifest is data the editor reads without ever running our code, so the
//! only way it can be wrong is quietly: a snippet file that is not shipped, a
//! command nothing registers, a body that is not Redblue. Each of those is a
//! defect the user meets as "the extension does nothing", which is exactly the
//! finding phase-042 was written from. So every assertion here reads the
//! shipped files and fails by naming the thing that is missing.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use redblue::{run_source, Error};
use serde_json::Value;

/// The extension manifest, relative to the crate root.
const MANIFEST: &str = "tooling/vscode/package.json";

/// The command the phase requires, and the keybinding that triggers it.
const RUN_COMMAND: &str = "redblue.run";

/// Statements the phase requires a completion for.
const REQUIRED_SNIPPETS: [&str; 8] = ["set", "say", "if", "for", "test", "expect", "try", "catch"];

/// Snippets that are only valid *inside* a `try`. A `catch` clause is not a
/// statement — the enclosing `try` is what closes it — so its body cannot stand
/// alone the way the others do; each is wrapped in a `try ... end` before it is
/// compiled.
const FRAGMENT_SNIPPETS: [&str; 1] = ["catch"];

fn root(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

/// Read a JSON document, or fail naming the file that could not be read.
fn read_json_at(path: &std::path::Path) -> Result<Value, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    serde_json::from_str(&text).map_err(|e| format!("{} is not valid JSON: {}", path.display(), e))
}

/// Read a JSON document by its path from the crate root.
fn read_json(relative: &str) -> Result<Value, String> {
    read_json_at(&root(relative))
}

fn manifest() -> Value {
    read_json(MANIFEST).unwrap_or_else(|e| panic!("{}", e))
}

/// The `contributes` array at `key`, or a failure naming what came back.
fn contributed<'a>(manifest: &'a Value, key: &str) -> &'a [Value] {
    manifest["contributes"][key]
        .as_array()
        .unwrap_or_else(|| {
            panic!(
                "{} must contribute `{}`; contributes holds {:?}",
                MANIFEST,
                key,
                manifest["contributes"]
                    .as_object()
                    .map(|o| o.keys().collect::<Vec<_>>())
            )
        })
        .as_slice()
}

/// A `./relative` path in the manifest, resolved against `tooling/vscode/`.
fn resolve(relative: &str) -> PathBuf {
    let trimmed = relative.strip_prefix("./").unwrap_or(relative);
    root("tooling/vscode").join(trimmed)
}

/// Every snippet the manifest offers, by name, merged across contributed files.
fn snippets() -> BTreeMap<String, Value> {
    let mut all = BTreeMap::new();

    for entry in contributed(&manifest(), "snippets") {
        let path = entry["path"]
            .as_str()
            .unwrap_or_else(|| panic!("a snippets entry has no path: {}", entry));
        let file = read_json_at(&resolve(path)).unwrap_or_else(|e| panic!("{}", e));

        for (name, snippet) in file.as_object().expect("a snippet file is an object") {
            assert!(
                all.insert(name.clone(), snippet.clone()).is_none(),
                "snippet `{}` is declared twice across the contributed snippet files",
                name
            );
        }
    }

    all
}

/// The lines of a snippet body.
fn body_of(snippet: &Value) -> Vec<String> {
    snippet["body"]
        .as_array()
        .unwrap_or_else(|| panic!("a snippet body is a list of lines: {}", snippet))
        .iter()
        .map(|line| {
            line.as_str()
                .unwrap_or_else(|| panic!("a snippet body line is a string: {}", line))
                .to_string()
        })
        .collect()
}

/// Fill a snippet body the way the editor does once every tab stop is left.
///
/// `${1:default}` becomes `default`, a bare `$1` becomes `1`, and `fills`
/// overrides the named tab stop. Nothing about VS Code's grammar is relied on
/// beyond that: the point is to hand the compiler the source a user ends up
/// with.
fn expand(body: &str, fills: &[(&str, &str)]) -> String {
    let mut out = String::new();
    let mut chars = body.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            '$' => match chars.peek().copied() {
                Some('{') => {
                    chars.next();
                    let mut field = String::new();
                    for ch in chars.by_ref() {
                        if ch == '}' {
                            break;
                        }
                        field.push(ch);
                    }
                    match field.split_once(':') {
                        Some((name, default)) => match fills
                            .iter()
                            .find(|(tab, _)| *tab == name)
                            .map(|(_, text)| *text)
                        {
                            Some(text) => out.push_str(text),
                            None => out.push_str(default),
                        },
                        None => out.push('1'),
                    }
                }
                Some(d) if d.is_ascii_digit() => {
                    let mut name = String::new();
                    while let Some(d) = chars.peek().copied() {
                        if !d.is_ascii_digit() {
                            break;
                        }
                        name.push(d);
                        chars.next();
                    }
                    match fills.iter().find(|(tab, _)| *tab == name).map(|(_, t)| *t) {
                        Some(text) => out.push_str(text),
                        None => out.push('1'),
                    }
                }
                _ => out.push('$'),
            },
            _ => out.push(c),
        }
    }

    out
}

/// The expanded body of `name`, filled with `fills`.
fn expand_snippet(name: &str, fills: &[(&str, &str)]) -> String {
    let offered = snippets();
    let snippet = offered
        .get(name)
        .unwrap_or_else(|| panic!("the extension offers no `{}` snippet", name));

    body_of(snippet)
        .join("\n")
        .lines()
        .map(|line| expand(line, fills))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Ids declared more than once under `contributes[key]`.
fn duplicate_ids(manifest: &Value, key: &str) -> Vec<String> {
    let entries = manifest["contributes"][key]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let mut seen = BTreeSet::new();
    let mut repeated = BTreeSet::new();
    for entry in &entries {
        let id = match entry["command"].as_str() {
            Some(id) => id.to_string(),
            None => continue,
        };
        if !seen.insert(id.clone()) {
            repeated.insert(id);
        }
    }

    repeated.into_iter().collect()
}

// ---------------------------------------------------------------------------
// Snippets
// ---------------------------------------------------------------------------

#[test]
fn extension_manifest_contributes_snippets_for_the_core_statements() {
    let manifest = manifest();
    let entries = contributed(&manifest, "snippets");

    assert!(
        !entries.is_empty(),
        "{} must contribute at least one snippet file",
        MANIFEST
    );

    for entry in entries {
        assert_eq!(
            entry["language"].as_str(),
            Some("redblue"),
            "a snippet file must be scoped to the redblue language: {}",
            entry
        );
        let path = entry["path"]
            .as_str()
            .unwrap_or_else(|| panic!("a snippets entry declares no path: {}", entry));
        assert!(
            resolve(path).is_file(),
            "{} contributes `{}`, which is not in the repository",
            MANIFEST,
            path
        );
    }

    let offered = snippets();
    let names: BTreeSet<&str> = offered.keys().map(String::as_str).collect();
    for required in REQUIRED_SNIPPETS {
        assert!(
            names.contains(required),
            "the extension must offer a `{}` completion; it offers {:?}",
            required,
            names
        );
    }
}

#[test]
fn edge_snippet_bodies_are_redblue_once_their_tab_stops_are_filled() {
    for (name, snippet) in snippets() {
        let body = body_of(&snippet).join("\n");

        assert!(!body.is_empty(), "snippet `{}` expands to nothing", name);
        assert!(
            snippet["prefix"].as_str().is_some_and(|p| !p.is_empty()),
            "snippet `{}` must declare the prefix that triggers it",
            name
        );

        let expanded = expand_snippet(&name, &[]);
        assert!(
            !expanded.contains('$'),
            "snippet `{}` expands to a literal `$` tab stop: {:?}",
            name,
            expanded
        );

        // A `catch` clause is only a clause inside a `try`; everything else has
        // to stand on its own, because that is what a user gets at a blank line.
        let program = if FRAGMENT_SNIPPETS.contains(&name.as_str()) {
            format!("try\n    say \"before\"\n{}\nend\n", expanded)
        } else {
            format!("{}\n", expanded)
        };

        run_source(&program).unwrap_or_else(|e| {
            panic!(
                "snippet `{}` does not expand to Redblue.\n--- expansion ---\n{}\n--- error ---\n{}",
                name, expanded, e
            )
        });
    }
}

#[test]
fn edge_filling_a_snippet_tab_stop_past_the_end_of_a_list_is_a_clean_error() {
    // The `expect` snippet is `expect ${1:1 + 1} to be ${2:2}`. Filling the
    // first tab stop with an index past the end of a two element list is exactly
    // the mistake a user makes while editing, and it has to arrive as an
    // ordinary runtime failure rather than a panic.
    let body = expand_snippet("expect", &[("1", "items[999]"), ("2", "2")]);
    let program = format!("set items to [1, 2]\n{}\n", body);

    let error = run_source(&program)
        .expect_err("index 999 of a two element list must fail, not silently read nothing");

    assert!(
        matches!(error, Error::Runtime(..)),
        "an out-of-range index is the program's own failure, not a host limit: {}",
        error
    );
    assert!(
        error.message().contains("999") || error.message().to_lowercase().contains("index"),
        "the failure must name the index that went wrong, got: {}",
        error.message()
    );
}

// ---------------------------------------------------------------------------
// The run command and its keybinding
// ---------------------------------------------------------------------------

#[test]
fn extension_manifest_contributes_a_run_command_and_a_keybinding() {
    let manifest = manifest();

    let command = contributed(&manifest, "commands")
        .iter()
        .find(|entry| entry["command"].as_str() == Some(RUN_COMMAND))
        .unwrap_or_else(|| {
            panic!(
                "{} must contribute the `{}` command; it contributes {:?}",
                MANIFEST,
                RUN_COMMAND,
                contributed(&manifest, "commands")
            )
        });

    assert!(
        command["title"].as_str().is_some_and(|t| !t.is_empty()),
        "the `{}` command must carry the title VS Code shows in the palette",
        RUN_COMMAND
    );

    let keybindings = contributed(&manifest, "keybindings");
    assert!(
        !keybindings.is_empty(),
        "{} must contribute at least one keybinding",
        MANIFEST
    );

    let binding = keybindings
        .iter()
        .find(|entry| entry["command"].as_str() == Some(RUN_COMMAND))
        .unwrap_or_else(|| {
            panic!(
                "a keybinding must fire `{}`; these fire {:?}",
                RUN_COMMAND,
                keybindings
                    .iter()
                    .filter_map(|k| k["command"].as_str())
                    .collect::<Vec<_>>()
            )
        });

    let key = binding["key"]
        .as_str()
        .unwrap_or_else(|| panic!("a keybinding must name a key: {}", binding));
    assert!(
        !key.is_empty(),
        "the `{}` keybinding names no key",
        RUN_COMMAND
    );

    let when = binding["when"].as_str().unwrap_or_else(|| {
        panic!(
            "the `{}` keybinding must be scoped with `when`",
            RUN_COMMAND
        )
    });
    assert!(
        when.contains("redblue"),
        "the keybinding must fire on Redblue files only, got: `{}`",
        when
    );
}

#[test]
fn edge_a_contributed_command_nothing_registers_is_rejected() {
    // A command in the manifest with no `registerCommand` behind it appears in
    // the command palette and then fails when it is chosen. The activation
    // source is what makes the contribution real, so it is checked both ways.
    let manifest = manifest();
    let commands = contributed(&manifest, "commands");

    let main = manifest["main"]
        .as_str()
        .unwrap_or_else(|| {
            panic!(
                "{} must declare `main`: a contributed command needs an extension \
                 host to register it",
                MANIFEST
            )
        })
        .to_string();
    assert!(
        resolve(&main).is_file(),
        "{} points `main` at `{}`, which is not in the repository",
        MANIFEST,
        main
    );

    let source = fs::read_to_string(resolve(&main))
        .unwrap_or_else(|e| panic!("{} must be readable: {}", main, e));

    let activations = manifest["activationEvents"]
        .as_array()
        .unwrap_or_else(|| panic!("{} must declare activationEvents", MANIFEST))
        .clone();

    let mut registered: Vec<&str> = Vec::new();
    for entry in commands {
        let id = entry["command"]
            .as_str()
            .unwrap_or_else(|| panic!("a commands entry declares no command id: {}", entry));
        registered.push(id);

        assert!(
            source.contains(&format!("registerCommand(\"{}\"", id)),
            "the manifest contributes `{}` but {} never registers it",
            id,
            main
        );

        let event = format!("onCommand:{}", id);
        assert!(
            activations
                .iter()
                .any(|e| e.as_str() == Some(event.as_str())),
            "{} contributes `{}` without declaring `{}`, so the keybinding would \
             have nothing to run",
            MANIFEST,
            id,
            event
        );
    }

    assert!(
        registered.contains(&RUN_COMMAND),
        "{} must contribute `{}`",
        MANIFEST,
        RUN_COMMAND
    );
}

#[test]
fn edge_duplicate_command_and_keybinding_entries_are_rejected() {
    // Two entries with the same id: the palette lists the command twice and the
    // second keybinding shadows the first, and nothing says so anywhere. The
    // shipped manifest must be clean, and the check that says so must actually
    // fail when the manifest is not.
    let manifest = manifest();

    assert_eq!(
        duplicate_ids(&manifest, "commands"),
        Vec::<String>::new(),
        "no command may be contributed twice"
    );
    assert_eq!(
        duplicate_ids(&manifest, "keybindings"),
        Vec::<String>::new(),
        "no keybinding may fire the same command twice"
    );

    let duplicated: Value = serde_json::json!({
        "contributes": {
            "commands": [
                {"command": RUN_COMMAND, "title": "Run"},
                {"command": RUN_COMMAND, "title": "Run again"}
            ],
            "keybindings": [
                {"command": RUN_COMMAND, "key": "f5", "when": "editorLangId == redblue"},
                {"command": RUN_COMMAND, "key": "f5", "when": "editorLangId == redblue"}
            ]
        }
    });

    assert_eq!(
        duplicate_ids(&duplicated, "commands"),
        vec![RUN_COMMAND.to_string()],
        "the duplicate check must catch a command contributed twice"
    );
    assert_eq!(
        duplicate_ids(&duplicated, "keybindings"),
        vec![RUN_COMMAND.to_string()],
        "the duplicate check must catch a keybinding bound twice"
    );
}

#[test]
fn edge_malformed_manifest_json_is_reported_rather_than_ignored() {
    // Every JSON file the editor loads has to parse, and the loader has to say
    // which file did not. A malformed manifest silently disables the whole
    // extension, which is the failure mode this phase exists to remove.
    let mut shipped = vec![resolve("package.json")];
    shipped.extend(
        contributed(&manifest(), "snippets")
            .iter()
            .filter_map(|entry| entry["path"].as_str())
            .map(resolve),
    );
    shipped.push(resolve("language-configuration.json"));
    shipped.push(resolve("syntaxes/redblue.tmLanguage.json"));

    for path in &shipped {
        read_json_at(path).unwrap_or_else(|e| panic!("{}", e));
    }

    let scratch = root("target/tmp/tooling_extension_test");
    fs::create_dir_all(&scratch).unwrap_or_else(|e| panic!("scratch dir: {}", e));
    let broken = scratch.join("broken.json");
    fs::write(&broken, "{\"contributes\": ").unwrap_or_else(|e| panic!("scratch write: {}", e));

    let error = read_json("target/tmp/tooling_extension_test/broken.json")
        .expect_err("a truncated manifest must not parse");
    assert!(
        error.contains("broken.json") && error.contains("not valid JSON"),
        "the failure must name the file and say it is not JSON, got: {}",
        error
    );

    let absent = read_json("target/tmp/tooling_extension_test/missing.json")
        .expect_err("a file that is not there must not parse");
    assert!(
        absent.contains("missing.json"),
        "the failure must name the file that is missing, got: {}",
        absent
    );
}
