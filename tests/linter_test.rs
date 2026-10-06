//! The linter's four rules, each with a test that proves it fires and a test
//! that proves it stays quiet, plus the edge cases a reader of `rb lint` output
//! would hit first: empty files, malformed files, unicode, deep nesting and
//! warning order.

use std::fs;
use std::path::{Path, PathBuf};

use redblue::linter::{lint, LintError, LintWarning};

fn split(source: &str) -> (Vec<LintError>, Vec<LintWarning>) {
    lint(source)
}

fn errors(source: &str) -> Vec<String> {
    split(source).0.into_iter().map(|e| e.message).collect()
}

fn warnings(source: &str) -> Vec<String> {
    split(source).1.into_iter().map(|w| w.message).collect()
}

fn every_message(source: &str) -> Vec<String> {
    let (errors, warnings) = split(source);
    let mut all: Vec<String> = errors
        .iter()
        .map(|e| e.message.clone())
        .chain(warnings.iter().map(|w| w.message.clone()))
        .collect();
    all.sort();
    all
}

// --- unused variables ------------------------------------------------------

#[test]
fn unused_variable_is_reported_with_its_line() {
    let source = "set total to 1\nsay \"hello\"\n";
    let (_, warnings) = split(source);
    assert_eq!(warnings.len(), 1, "{:?}", warnings);
    assert_eq!(warnings[0].message, "Unused variable: 'total'");
    assert_eq!(warnings[0].line, 1, "the warning must point at the `set`");
}

#[test]
fn edge_a_name_assigned_on_several_lines_is_reported_once_at_the_first_set() {
    // `set` is assignment, so the same name on three lines is one binding that
    // was assigned three times, not three bindings. Reporting it once, at the
    // line that introduced it, is what makes `tests/test_numeric_edges.rb`
    // yield a single warning for `x` rather than one per failing division.
    let source = "try\n    set x to 1 / 0\ncatch error\n    say \"caught\"\nend\n\
                  try\n    set x to 5 % 0\ncatch error\n    say \"caught\"\nend\n\
                  try\n    set x to 1e400\ncatch error\n    say \"caught\"\nend\n\
                  say \"done\"\n";

    let (lint_errors, found) = split(source);
    assert!(
        lint_errors.is_empty(),
        "the fixture must be valid Redblue: {:?}",
        lint_errors
    );
    assert_eq!(
        found.len(),
        1,
        "one name is one finding however often it is assigned: {:?}",
        found
    );
    assert_eq!(found[0].message, "Unused variable: 'x'");
    assert_eq!(
        found[0].line, 2,
        "the finding must point at the first `set x`, the line that introduced it"
    );
}

#[test]
fn a_variable_that_is_read_is_not_reported() {
    let source = "set total to 1\nset total to total + 1\nsay total\n";
    assert_eq!(
        every_message(source),
        Vec::<String>::new(),
        "reading the variable makes it used"
    );
}

#[test]
fn function_parameter_is_not_a_false_positive() {
    let source = "to greet(name)\n    say \"hello\"\nend\n\ngreet with \"world\"\n";
    assert!(
        every_message(source).is_empty(),
        "an unread function parameter is part of the signature, not an unused variable: {:?}",
        every_message(source)
    );
}

#[test]
fn field_assignment_is_not_a_false_positive() {
    let source = "object Box\nend\nset Box.label to \"crate\"\nsay Box.label\n";
    assert_eq!(
        every_message(source),
        Vec::<String>::new(),
        "a field write is not a variable binding: {:?}",
        every_message(source)
    );
}

#[test]
fn a_record_used_only_for_its_fields_is_not_reported() {
    let source = "set counter to {count: 0}\nset counter.count to counter.count + 1\n";
    assert_eq!(
        every_message(source),
        Vec::<String>::new(),
        "`counter` is read by both of its field writes: {:?}",
        every_message(source)
    );
}

#[test]
fn an_unused_object_is_reported_like_any_other_variable() {
    let source = "object Unused\nend\nsay \"hi\"\n";
    assert_eq!(warnings(source), vec!["Unused variable: 'Unused'"]);
}

#[test]
fn a_parent_object_named_by_extends_is_not_reported() {
    let source = "object One\nend\n\nobject Two extends One\nend\n\nsay \"hi\"\n";
    assert_eq!(
        warnings(source),
        vec!["Unused variable: 'Two'"],
        "`extends One` reads the parent record at declaration time (src/vm.rs:1419), \
         so `One` is used and only the never-referenced child is unused"
    );
}

#[test]
fn edge_extends_of_an_undeclared_parent_is_still_a_plain_read() {
    // The analyser and the VM both reject an undeclared parent, so the linter
    // must not read `One` as declared: only `Two` can be reported, and `One`
    // is not one of the file's variables to report at all.
    let source = "object Two extends One\nend\nsay \"hi\"\n";
    assert_eq!(warnings(source), vec!["Unused variable: 'Two'"]);
}

#[test]
fn edge_every_parent_in_a_chain_of_extends_is_read() {
    let source = "object A\nend\n\nobject B extends A\nend\n\nobject C extends B\nend\n\nset c to C\nsay c.label\n";
    assert_eq!(
        every_message(source),
        Vec::<String>::new(),
        "`A` and `B` are named by `extends` and `C` is read: nothing is unused"
    );
    assert_eq!(warnings(source), Vec::<String>::new());
}

#[test]
fn edge_the_leaf_of_an_extends_chain_is_still_reported_when_unread() {
    let source = "object A\nend\n\nobject B extends A\nend\n\nset a to A\nsay a.label\n";
    assert_eq!(
        warnings(source),
        vec!["Unused variable: 'B'"],
        "reading the parent must not mark the child used"
    );
}

#[test]
fn an_underscore_prefixed_name_is_left_alone() {
    let source = "set _ignored to 1\nsay \"hi\"\n";
    assert_eq!(every_message(source), Vec::<String>::new());
}

// --- unused imports --------------------------------------------------------

#[test]
fn an_unused_file_level_import_is_reported() {
    let source = "import files\nsay \"hi\"\n";
    assert_eq!(warnings(source), vec!["Unused import: 'files'"]);
}

#[test]
fn an_import_the_file_uses_is_not_reported() {
    let source = "import files\nsay files.exists(\"a.txt\")\n";
    assert_eq!(
        every_message(source),
        Vec::<String>::new(),
        "the module is reached through its own name"
    );
}

#[test]
fn edge_import_used_through_its_alias_is_not_reported() {
    let source = "import files to f\nsay f.exists(\"a.txt\")\n";
    assert_eq!(
        every_message(source),
        Vec::<String>::new(),
        "the alias is the name the file uses: {:?}",
        every_message(source)
    );
}

#[test]
fn edge_import_used_through_its_original_name_is_not_reported() {
    let source = "import files to f\nsay files.exists(\"a.txt\")\n";
    assert_eq!(
        every_message(source),
        Vec::<String>::new(),
        "an alias leaves the original module name bound as well"
    );
}

#[test]
fn edge_import_inside_a_block_is_not_a_file_dependency() {
    let source = "test \"loads a module\"\n    import SuiteKit\n    set ok to yes\n    expect ok to be yes\nend\n";
    assert_eq!(
        every_message(source),
        Vec::<String>::new(),
        "an import inside a block is a statement run there, not a dependency of the file: {:?}",
        every_message(source)
    );
}

// --- shadowing -------------------------------------------------------------

#[test]
fn a_loop_variable_that_hides_an_outer_one_is_reported() {
    let source = "set n to 1\nfor each n in [1, 2]\n    say n\nend\n";
    assert_eq!(
        warnings(source),
        vec!["Variable 'n' shadows an outer variable"],
        "the loop body cannot see the outer `n` any more"
    );
}

#[test]
fn edge_a_loop_variable_with_a_name_of_its_own_is_not_reported() {
    let source = "set n to 1\nfor each item in [1, 2]\n    say item\nend\nsay n\n";
    assert_eq!(every_message(source), Vec::<String>::new());
}

#[test]
fn reassignment_inside_a_loop_is_not_shadowing() {
    let source = "set n to 1\nrepeat 3 times\n    set n to n + 1\nend\nsay n\n";
    assert_eq!(
        every_message(source),
        Vec::<String>::new(),
        "`set` is assignment, not declaration: {:?}",
        every_message(source)
    );
}

#[test]
fn edge_a_parameter_that_hides_an_outer_variable_is_reported() {
    let source = "set value to 1\nto show(value)\n    say value\nend\nshow with 2\nsay value\n";
    assert_eq!(
        warnings(source),
        vec!["Parameter 'value' shadows an outer variable"]
    );
}

#[test]
fn edge_a_catch_binding_that_hides_an_outer_variable_is_reported() {
    let source = "set err to nothing\ntry\n    say 1 / 0\ncatch err\n    say err\nend\n";
    assert_eq!(
        warnings(source),
        vec!["Parameter 'err' shadows an outer variable"]
    );
}

// --- source that does not parse -------------------------------------------

#[test]
fn a_missing_end_is_reported_as_an_error() {
    let source = "if 1 is 1 then\n    say \"hi\"\n";
    let found = errors(source);
    assert_eq!(found.len(), 1, "{:?}", found);
    assert!(
        found[0].contains("End"),
        "the error must name what is missing: {:?}",
        found
    );
}

#[test]
fn a_wellformed_program_reports_no_errors() {
    let source =
        "set total to 0\nfor each n in [1, 2, 3]\n    set total to total + n\nend\nsay total\n";
    assert_eq!(errors(source), Vec::<String>::new());
}

// --- edge cases ------------------------------------------------------------

#[test]
fn edge_empty_and_whitespace_only_source_lints_clean() {
    for source in ["", "\n", "\n\n\n", "   \n\t\n", "// just a comment\n"] {
        assert_eq!(
            every_message(source),
            Vec::<String>::new(),
            "{:?} should say nothing",
            source
        );
    }
}

#[test]
fn edge_bom_and_crlf_source_lints_clean() {
    let source = "\u{feff}set total to 1\r\nsay total\r\n";
    assert_eq!(
        every_message(source),
        Vec::<String>::new(),
        "a byte order mark and CRLF endings are not lint findings"
    );
}

#[test]
fn edge_unterminated_string_is_an_error_and_not_a_panic() {
    let found = errors("set greeting to \"hello\nsay greeting\n");
    assert_eq!(found.len(), 1, "{:?}", found);
    assert!(
        found[0].starts_with("Syntax error:"),
        "a lexer failure must still be reported: {:?}",
        found
    );
}

#[test]
fn edge_stray_token_is_an_error_and_not_a_panic() {
    assert_eq!(errors("set total to 1\n)\n").len(), 1);
}

#[test]
fn edge_unclosed_list_is_an_error_and_not_a_panic() {
    assert_eq!(errors("set items to [1, 2\n").len(), 1);
}

#[test]
fn edge_deeply_nested_blocks_are_a_diagnostic_not_a_stack_overflow() {
    let mut source = String::new();
    for _ in 0..500 {
        source.push_str("repeat 1 times\n");
    }
    source.push_str("say \"deep\"\n");
    for _ in 0..500 {
        source.push_str("end\n");
    }

    let found = errors(&source);
    assert!(
        !found.is_empty(),
        "500 nested blocks must be refused by the parser, not by the stack"
    );
}

#[test]
fn edge_deeply_nested_expressions_are_a_diagnostic_not_a_stack_overflow() {
    let source = format!("set x to {}1{}\n", "(".repeat(500), ")".repeat(500));
    assert_eq!(errors(&source).len(), 1);
}

#[test]
fn edge_unicode_source_reports_the_right_line() {
    let source = "set greeting to \"héllo 🌍 שלום\"\nsay greeting\nset spare to 1\nsay \"bye\"\n";
    let (_, warnings) = split(source);
    assert_eq!(warnings.len(), 1, "{:?}", warnings);
    assert_eq!(warnings[0].message, "Unused variable: 'spare'");
    assert_eq!(
        warnings[0].line, 3,
        "a line with multibyte characters must not shift the line count"
    );
}

#[test]
fn edge_unicode_variable_names_are_linted() {
    let source = "set café to 1\nsay \"drink\"\n";
    assert_eq!(warnings(source), vec!["Unused variable: 'café'"]);
}

#[test]
fn edge_warning_order_is_stable_and_follows_the_source() {
    let mut source = String::new();
    for name in ["zeta", "alpha", "mu", "beta"] {
        source.push_str(&format!("set {} to 1\n", name));
    }
    source.push_str("say \"hi\"\n");

    let expected = vec![
        "Unused variable: 'zeta'",
        "Unused variable: 'alpha'",
        "Unused variable: 'mu'",
        "Unused variable: 'beta'",
    ];
    for _ in 0..8 {
        assert_eq!(
            warnings(&source),
            expected,
            "warning order must not depend on hash order"
        );
    }
}

#[test]
fn edge_empty_and_singleton_collections_are_linted() {
    let source = "set empty to []\nset one to [7]\nset nothing_at_all to nothing\nfor each item in empty\n    say item\nend\nfor each only in one\n    say only\nend\nsay nothing_at_all\n";
    assert_eq!(every_message(source), Vec::<String>::new());
}

#[test]
fn edge_an_unread_loop_variable_is_reported() {
    let source = "for each item in [1, 2, 3]\n    say \"tick\"\nend\n";
    assert_eq!(warnings(source), vec!["Unused variable: 'item'"]);
}

#[test]
fn edge_numeric_and_index_edges_are_not_lint_errors() {
    // Numeric limits, out of range indexes and type mismatches are runtime
    // facts. The linter must not claim to know any of them.
    let source = "set big to 9007199254740993\nset tiny to -9223372036854775808\nset huge to 1e308\nset one_over_zero to 1 / 0\nset items to [1]\nsay items[999]\nsay items[-1]\nsay 1 + \"one\"\nsay big\nsay tiny\nsay huge\nsay one_over_zero\nsay items\n";
    assert_eq!(
        errors(source),
        Vec::<String>::new(),
        "the linter must not invent runtime diagnostics: {:?}",
        errors(source)
    );
}

#[test]
fn edge_a_record_with_a_repeated_key_is_not_a_lint_error() {
    let source = "set r to {a: 1, a: 2}\nsay r.a\n";
    assert_eq!(
        errors(source),
        Vec::<String>::new(),
        "which key wins is the runtime's business, not the linter's"
    );
}

#[test]
fn edge_a_missing_field_is_not_a_lint_error() {
    let source = "set r to {a: 1}\nsay r.b\n";
    assert_eq!(errors(source), Vec::<String>::new());
}

#[test]
fn edge_a_long_source_is_linted_without_truncation() {
    let mut source = String::new();
    for _ in 0..2000 {
        source.push_str("say \"a line of output\"\n");
    }
    assert_eq!(every_message(&source), Vec::<String>::new());
}

#[test]
fn nested_scopes_do_not_leak_names_into_each_other() {
    let source = "set outer to 1\nrepeat 2 times\n    for each inner in [1]\n        set middle to inner\n    end\n    say middle\nend\nsay outer\n";
    // `inner` and `middle` are both read, and neither hides `outer`.
    assert_eq!(every_message(source), Vec::<String>::new());
}

#[test]
fn edge_a_name_bound_in_a_nested_scope_only_is_still_reported() {
    let source = "repeat 2 times\n    set middle to 1\nend\nsay \"hi\"\n";
    assert_eq!(warnings(source), vec!["Unused variable: 'middle'"]);
}

// --- the corpus ------------------------------------------------------------

/// Every `.rb` file in `examples/`, `modules/` and `tests/` is the language's
/// specification-by-example. The linter may only speak about them when it has
/// something true to say, so the corpus must lint clean.
fn corpus() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for dir in ["examples", "modules", "tests"] {
        let Ok(entries) = fs::read_dir(root.join(dir)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("rb") {
                files.push(path);
            }
        }
    }
    files.sort();
    assert!(!files.is_empty(), "no corpus files found");
    files
}

/// Splits a line into the identifiers it mentions. Text and punctuation are
/// separators, so `set x to items[0]` yields `set`, `x`, `to`, `items`.
fn identifiers(line: &str) -> Vec<String> {
    line.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

/// Blanks out comments and text literals. A name inside `"..."` is a string,
/// not a read: `{name}` is a literal brace pair in Redblue today, so the name
/// in it is text.
fn code_only(line: &str) -> String {
    if line.trim_start().starts_with("//") {
        return String::new();
    }
    let mut out = String::new();
    let mut in_text = false;
    let mut escaped = false;
    for c in line.chars() {
        if in_text {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_text = false;
            }
            continue;
        }
        if c == '"' {
            in_text = true;
            continue;
        }
        out.push(c);
    }
    out
}

/// Blanks out the property half of every `a.b` chain, so that a record field
/// written as `data.rows[0].cells` is not mistaken for a read of a variable
/// named `cells`.
fn without_properties(line: &str) -> String {
    let mut out = String::new();
    for (index, part) in line.split('.').enumerate() {
        if index == 0 {
            out.push_str(part);
        } else {
            let rest = part.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_');
            out.push_str(rest);
        }
        out.push('.');
    }
    out
}

/// True when `tokens[index]` names a record field rather than a variable.
fn is_field_key(tokens: &[String], index: usize) -> bool {
    tokens.get(index + 1).map(String::as_str) == Some(":")
}

/// True when the mention of `name` on this line binds it rather than reads it.
/// `extends` is deliberately absent: `object Child extends Parent` binds
/// nothing and reads `Parent`, so naming it here would hide exactly the false
/// positive the corpus check exists to find.
fn is_binding_mention(tokens: &[String], index: usize) -> bool {
    let before = index.checked_sub(1).map(|i| tokens[i].as_str());
    let after = tokens.get(index + 1).map(String::as_str);
    matches!(
        (before, after),
        (Some("set"), Some("to"))
            | (Some("for"), Some("in"))
            | (Some("for"), Some("from"))
            | (Some("each"), Some("in"))
            | (Some("object"), _)
            | (Some("has"), _)
            | (Some("to"), _)
    )
}

/// Reads `source` line by line, reporting every mention of `name` that is a
/// use rather than a binding. A linter warning that names a variable this
/// finds is a false positive.
fn uses_after(source: &str, name: &str, line: usize) -> Vec<usize> {
    let mut found = Vec::new();
    for (offset, text) in source.lines().enumerate() {
        let number = offset + 1;
        if number <= line {
            continue;
        }
        let tokens = identifiers(&without_properties(&code_only(text)));
        let mentions: Vec<usize> = (0..tokens.len())
            .filter(|index| tokens[*index] == name)
            .collect();
        if mentions.is_empty() {
            continue;
        }
        // A line that binds the name is not evidence that it is read: the linter
        // would not have warned had the binding also read it.
        if mentions.iter().any(|i| is_binding_mention(&tokens, *i)) {
            continue;
        }
        if mentions.iter().all(|i| is_field_key(&tokens, *i)) {
            continue;
        }
        found.push(number);
    }
    found
}

/// Every mention of `name`, binding or not, on any line.
fn mentioned_anywhere(source: &str, name: &str) -> bool {
    source.lines().any(|text| {
        identifiers(&without_properties(&code_only(text)))
            .iter()
            .any(|token| token == name)
    })
}

fn warned_name(message: &str, prefix: &str) -> Option<String> {
    let rest = message.strip_prefix(prefix)?;
    rest.strip_suffix('\'').map(str::to_string)
}

#[test]
fn the_corpus_produces_no_lint_errors() {
    for path in corpus() {
        let source = fs::read_to_string(&path).expect("corpus file should be readable");
        let found = errors(&source);
        assert!(found.is_empty(), "{}: {:?}", path.display(), found);
    }
}

#[test]
fn the_examples_lint_without_warnings() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut files: Vec<PathBuf> = fs::read_dir(&root)
        .expect("examples/ should be readable")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("rb"))
        .collect();
    files.sort();

    let mut reported = Vec::new();
    for path in files {
        let source = fs::read_to_string(&path).expect("example should be readable");
        for message in warnings(&source) {
            reported.push(format!("{}: {}", path.display(), message));
        }
    }
    assert!(
        reported.is_empty(),
        "the examples are the language's specification-by-example and must lint clean, found:\n{}",
        reported.join("\n")
    );
}

#[test]
fn edge_no_unused_variable_warning_in_the_corpus_names_a_read_variable() {
    let mut checked = 0;
    for path in corpus() {
        let source = fs::read_to_string(&path).expect("corpus file should be readable");
        for warning in split(&source).1 {
            let Some(name) = warned_name(&warning.message, "Unused variable: '") else {
                continue;
            };
            checked += 1;
            let reads = uses_after(&source, &name, warning.line);
            assert!(
                reads.is_empty(),
                "{}:{} claims '{}' is unused but it is read on {:?}",
                path.display(),
                warning.line,
                name,
                reads
            );
        }
    }
    assert!(
        checked > 0,
        "the corpus should exercise the unused variable rule at least once"
    );
}

#[test]
fn edge_no_unused_import_warning_in_the_corpus_names_a_used_module() {
    for path in corpus() {
        let source = fs::read_to_string(&path).expect("corpus file should be readable");
        for warning in split(&source).1 {
            let Some(name) = warned_name(&warning.message, "Unused import: '") else {
                continue;
            };
            assert!(
                !mentioned_anywhere(&source, &name),
                "{}:{} claims import '{}' is unused but the file mentions it",
                path.display(),
                warning.line,
                name
            );
        }
    }
}

#[test]
fn edge_every_shadow_warning_in_the_corpus_hides_a_real_outer_binding() {
    for path in corpus() {
        let source = fs::read_to_string(&path).expect("corpus file should be readable");
        for warning in split(&source).1 {
            let Some(name) = warned_name(&warning.message, "Variable '")
                .or_else(|| warned_name(&warning.message, "Parameter '"))
            else {
                continue;
            };

            let outer = (1..warning.line).any(|line| {
                let text = source.lines().nth(line - 1).unwrap_or_default();
                let tokens = identifiers(&without_properties(&code_only(text)));
                tokens
                    .iter()
                    .enumerate()
                    .any(|(index, token)| token == &name && is_binding_mention(&tokens, index))
            });
            assert!(
                outer,
                "{}:{} claims '{}' shadows an outer variable, but nothing before it binds that name",
                path.display(),
                warning.line,
                name
            );
        }
    }
}

// --- the output of `rb lint` itself ---------------------------------------

/// Runs `rb lint` on a file in the project's scratch directory and returns
/// `(exit code, stderr)`. A diagnostic about a real file is only worth
/// printing if it says where the file is wrong.
fn lint_on_the_binary(source: &str) -> (i32, String) {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/linter");
    fs::create_dir_all(&dir).expect("scratch directory should be creatable");
    let serial = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let path = dir.join(format!("lint-{}-{}.rb", std::process::id(), serial));
    fs::write(&path, source).expect("lint target should be writable");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("lint")
        .arg(&path)
        .output()
        .expect("the rb binary should run");
    let _ = fs::remove_file(&path);

    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn edge_rb_lint_prints_the_line_of_every_warning() {
    let source = "set a to 1\nset b to 2\nset c to 3\nsay \"hi\"\n";
    let (code, stderr) = lint_on_the_binary(source);
    assert_eq!(code, 0, "warnings alone do not fail the lint: {stderr}");
    assert!(
        stderr.contains("line 1") && stderr.contains("Unused variable: 'a'"),
        "the first warning must name line 1: {stderr}"
    );
    assert!(
        stderr.contains("line 3") && stderr.contains("Unused variable: 'c'"),
        "the third warning must name line 3: {stderr}"
    );
}

#[test]
fn edge_rb_lint_prints_the_line_of_a_syntax_error_and_exits_nonzero() {
    let (code, stderr) = lint_on_the_binary("say \"hi\"\nif yes then\n    say \"no\"\n");
    assert_eq!(code, 1, "a missing `end` must fail the lint: {stderr}");
    assert!(
        stderr.contains("Error:") && stderr.contains("Expected End"),
        "a missing `end` must be named as such: {stderr}"
    );
    // The parser reports at the point the `end` was due, which is end of file,
    // so the line is the last line rather than the line the `if` opened on.
    assert!(
        stderr.contains("line 4"),
        "the syntax error must name the line it is on: {stderr}"
    );
}
