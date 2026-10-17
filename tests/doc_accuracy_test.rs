//! Every place the project states what works, is checked against what works.
//!
//! The finding this pins, and it is three defects in one habit — a document is
//! written, and nothing ever asks `rb` whether it is true:
//!
//! - `README.md` listed `src/vm.rs` in the project structure. That file has not
//!   existed for many phases; it was renamed to `src/interpreter.rs`. A reader
//!   who went looking for the tree-walking interpreter found nothing.
//! - `HANDOUT.md` carried a whole `formats - JSON/CSV` section built on
//!   `formats.parse_json('{"name": "Alice"}')` — a module that does not exist
//!   **and** a single-quoted string, which the lexer refuses before the module
//!   is ever consulted. `SPEC.md` says in as many words that there is no
//!   `formats` module. The two documents disagreed.
//! - `README.md`'s roadmap ticked self-hosting as *in progress*, while
//!   `bootstrap/compiler.rb` is over three thousand lines of Redblue and
//!   `tests/bootstrap_selfhost_test.rs` asserts a stage-3 fixed point — three
//!   consecutive self-compilations byte-identical to each other.
//!
//! `tests/doc_blocks_test.rs` already executes every ```redblue block and keeps
//! three checked-in buckets saying which ones run. What it cannot do is notice a
//! **prose** claim, and it cannot notice a file name in an ASCII tree. Those are
//! what this file is for, and each test below is written so that reverting the
//! document correction it guards fails it.
//!
//! Nothing here reimplements the doc-block extractor. Where a test needs to know
//! what a document says, it reads the document.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The five documents that define the language, in a fixed order so a failure
/// message reads the same on every run.
const DOCS: &[&str] = &[
    "SPEC.md",
    "README.md",
    "PHILOSOPHY.md",
    "HANDOUT.md",
    "docs/GRAMMAR.md",
];

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// The proxy environment variables `reqwest` would otherwise honour.
///
/// Removed from every child for the same reason `tests/stdlib_module_docs_test.rs`
/// removes them: a runner with `HTTP_PROXY` set sends the loopback request
/// somewhere the test does not own, and the row fails on the environment rather
/// than on the language.
const PROXY_ENV: &[&str] = &[
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
];

/// The path the binary writes this suite's scratch programs into.
///
/// `target/tmp/` is already covered by `.gitignore`, and the directory is
/// created by the run rather than assumed, so a clean checkout works.
fn scratch_dir() -> PathBuf {
    let dir = Path::new(ROOT).join("target/tmp/p082docs");
    std::fs::create_dir_all(&dir).expect("the scratch directory is creatable");
    dir
}

/// Runs `source` under `tag` and returns `(exit success, stdout, stderr)`.
///
/// Stdin is closed rather than inherited, so a program that reads the terminal
/// reads end-of-input instead of blocking forever on a console nobody is at.
/// Each run gets its own file, because `cargo test` runs tests in parallel and
/// one test's program would be another's if they shared a path.
fn run_named(tag: &str, source: &str) -> (bool, String, String) {
    let file = scratch_dir().join(format!("{tag}.rb"));
    std::fs::write(&file, source).expect("the scratch program is writable");

    let mut command = Command::new(env!("CARGO_BIN_EXE_rb"));
    command
        .arg("run")
        .arg(&file)
        .stdin(std::process::Stdio::null());
    for name in PROXY_ENV {
        command.env_remove(name);
    }
    let out = command.output().expect("rb runs");

    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Runs `source` under `tag` and asserts it printed exactly `expected`.
fn says(tag: &str, source: &str, expected: &str) {
    let (ok, stdout, stderr) = run_named(tag, source);
    assert!(
        ok,
        "the program failed, so the claim it supports is not true:\n{source}\n--- stderr\n{stderr}"
    );
    assert_eq!(
        stdout.trim_end(),
        expected,
        "wrong output for:\n{source}\n--- stderr\n{stderr}"
    );
}

/// Runs `source` under `tag` and asserts it was refused, mentioning `needle`.
///
/// This is the half of the contract that makes the rest of the file honest: a
/// document that names a capability the binary lacks is only wrong if the
/// binary really does lack it, so the refusals themselves are pinned rather than
/// assumed.
fn refuses(tag: &str, source: &str, needle: &str) {
    let (ok, _, stderr) = run_named(tag, source);
    assert!(
        !ok,
        "the program was accepted but the documents say it should be refused:\n{source}"
    );
    assert!(
        stderr.contains(needle),
        "the refusal does not mention '{needle}', so it cannot say which call was wrong:\n\
         {source}\n--- stderr\n{stderr}"
    );
}

/// Reads one of the five documents, naming the file if it is not there.
///
/// A missing document is an error rather than an empty string: an empty string
/// would satisfy every "does not contain" assertion in this file, which is the
/// cheapest possible way to make them all green without changing anything.
fn read_doc(name: &str) -> String {
    let path = Path::new(ROOT).join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("{name} should be readable from the project root: {error}");
    })
}

/// Every `(document, 1-based line, the line)` inside a fenced ```redblue block
/// that contains `needle`.
///
/// **Only fenced blocks are scanned**, and that is the whole design. A document
/// is allowed — required, even — to *say out loud* that a name does not exist:
/// `SPEC.md:1225` writes `Module 'list' has no function 'filter'` and
/// `README.md:233` writes `math.random is not implemented`. Those are the
/// corrections, not the claims, and a check that flagged them would have been
/// flagged-and-ignored within a phase. What a reader pastes into a file is the
/// code block, so the code block is what has to be true.
fn occurrences(name: &str, text: &str, needle: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut inside = false;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if !inside {
            if trimmed.starts_with("```redblue") {
                inside = true;
            }
            continue;
        }
        if trimmed.starts_with("```") {
            inside = false;
            continue;
        }
        if line.contains(needle) {
            found.push(format!("{name}:{}: {}", index + 1, trimmed));
        }
    }
    found
}

/// Whether a `'` in `body` opens a Redblue string literal.
///
/// Redblue has no single-quoted string, so a `'` is a lexer error — except where
/// it cannot be one. Two places a `'` is not a program are skipped:
///
///  - **inside a double-quoted string**, which is legal and which `SPEC.md`
///    itself uses: `say "I'm {this.name}"` is a `RUNS` entry. `\"` does not end
///    the string, so `say "he said \"hi\""` is read to its real closing quote.
///  - **inside a `//` comment**, which runs to the end of the line and cannot
///    contain a program either. `SPEC.md:143` is a catalogue of the escapes the
///    lexer refuses and writes the refusals beside them — `// LexerError:
///    Unknown escape '\q' in text` — so a rule that reported every `'` would
///    report a document that is being precise about an error.
///
/// A `'` that opens a Redblue string is outside both, by definition, so skipping
/// them costs no detection. What remains is reported.
fn quote_opens_a_string(body: &str) -> bool {
    let mut in_text = false;
    let mut in_comment = false;
    let mut escaped = false;
    for ch in body.chars() {
        if in_comment {
            in_comment = ch != '\n';
        } else if in_text {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_text = false;
            }
        } else if ch == '"' {
            in_text = true;
        } else if ch == '/' {
            in_comment = true;
        } else if ch == '\'' {
            return true;
        }
    }
    false
}

/// The names a document is not allowed to present as working Redblue.
///
/// Each entry is `(what the document would be claiming, the literal it would
/// have to contain, a program proving the claim is false)`. The third column is
/// what makes this a test rather than a grep: a claim is only wrong if the
/// binary really does refuse it, so the refusal is executed here.
const FORBIDDEN_CLAIMS: &[(&str, &str, &str)] = &[
    (
        "a `formats` module",
        "formats.",
        r#"say formats.parse_json("{}")"#,
    ),
    (
        "`list.filter`, which the module does not have",
        "list.filter",
        "say list.filter([1, 2], to (x) give back x > 1 end)",
    ),
    (
        "`math.random`, which SPEC.md says is not implemented",
        "math.random",
        "say math.random(1, 6)",
    ),
    (
        "`ask`, which the language spells `input`",
        "ask \"",
        "set answer to ask \"Your name?\"",
    ),
    (
        "a method call on a list value, which is not how a list is spelled",
        ".map(double)",
        "set xs to [1, 2]\nset ys to xs.map(to (x) give back x * 2)",
    ),
];

// ============================================ the claims the documents must not make

/// Every one of the five documents, checked against the claims it is not
/// allowed to make.
///
/// The finding named `HANDOUT.md:149` and `HANDOUT.md:154`, and it also named
/// `PHILOSOPHY.md:231` and `SPEC.md:607`, which are the same three defects in
/// two more places. Checking one document is how the next one survived: the
/// correction lands, the document is marked done, and the sibling keeps
/// promising a module that was never there. So this walks all five.
///
/// Only fenced ```redblue blocks are scanned — see [`occurrences`]. A document
/// is *required* to say out loud that `list.filter` and `math.random` do not
/// exist, and those sentences are not claims.
#[test]
fn no_document_promises_a_capability_the_binary_lacks() {
    let mut false_claims: Vec<String> = Vec::new();

    for name in DOCS {
        let text = read_doc(name);
        for (claim, needle, _) in FORBIDDEN_CLAIMS {
            for line in occurrences(name, &text, needle) {
                false_claims.push(format!(
                    "{line}\n    claims {claim}, which the binary refuses"
                ));
            }
        }
    }

    assert!(
        false_claims.is_empty(),
        "the documents claim capabilities that do not exist:\n{}",
        false_claims.join("\n")
    );
}

// ============================================ a single-quoted string

/// A `'` inside a Redblue program is a `LexerError`, and a document that shows
/// one is showing a program that does not parse.
///
/// This is both the failure assertion for the phase and the check on SPEC.md's
/// and HANDOUT.md's own claim. `HANDOUT.md:149` wrote
/// `formats.parse_json('{"name": "Alice"}')`, which dies at the *first* quote —
/// the lexer never reaches the missing module. `SPEC.md:1259` already says so
/// in prose; the block below proves it, and the test after it proves the block
/// is the only spelling a document is allowed to show.
#[test]
fn edge_a_single_quoted_string_in_a_program_is_a_lexer_error() {
    let (ok, _, stderr) = run_named(
        "single_quote",
        r#"set name to 'Alice'
say name"#,
    );
    assert!(
        !ok,
        "a single-quoted string was accepted; SPEC.md says it is a LexerError, so \
         either the language changed or the document is now wrong"
    );
    assert!(
        stderr.contains("LexerError"),
        "the refusal is not a lexer error:\n--- stderr\n{stderr}"
    );
    assert!(
        stderr.contains("'"),
        "the refusal does not quote the character it choked on:\n--- stderr\n{stderr}"
    );

    // The spelling that does work, so the assertion above is about the quote
    // and not about JSON. An escaped quote inside a double-quoted string is the
    // form SPEC.md documents.
    says(
        "double_quote",
        r#"say json.parse("{ \"name\": \"Alice\" }")"#,
        "{name: Alice}",
    );
}

/// No `RUNS` block in any of the five documents may contain a `'`.
///
/// `tests/doc_blocks_test.rs` executes the blocks it is told run and would catch
/// a `'` — but only once it fails, and only if the block is still classified
/// `RUNS`. A single-quoted example that was quietly demoted to a `KNOWN-FAIL`
/// entry, or dropped from a bucket entirely, would leave the document claiming
/// something false with every test green. This reads the RUNS bucket itself.
#[test]
fn edge_no_running_document_block_contains_a_single_quote() {
    let runs = std::fs::read_to_string(Path::new(ROOT).join("tests/doc_blocks_runs.txt"))
        .expect("the RUNS bucket is checked in");
    let keys: Vec<&str> = runs
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    assert!(
        !keys.is_empty(),
        "the RUNS bucket is empty, so this assertion would pass on nothing"
    );

    let mut offenders: Vec<String> = Vec::new();
    for key in &keys {
        let (doc, line) = key
            .split_once(':')
            .unwrap_or_else(|| panic!("a RUNS key is not `doc:line`: {key}"));
        let line: usize = line
            .parse()
            .unwrap_or_else(|_| panic!("a RUNS line number is not a number: {key}"));
        let body = block_body(doc, line);
        if quote_opens_a_string(&body) {
            offenders.push(format!(
                "{key} is classified RUNS and its body opens a single-quoted string, \
                 which the lexer refuses:\n{body}"
            ));
        }
    }

    assert!(
        offenders.is_empty(),
        "document blocks that run also carry a character the lexer refuses:\n{}",
        offenders.join("\n")
    );
}

/// The fenced block that opens on `doc:line`, or `""` when there is none.
///
/// Returns an empty string rather than panicking for a key with no block, so a
/// bucket entry pointing at a deleted block reads as "no offending quote" here
/// and is caught by `tests/doc_blocks_test.rs`, which is the test that owns that
/// check. Two tests failing for one stale key would be noise.
fn block_body(doc: &str, line: usize) -> String {
    let text = read_doc(doc);
    let lines: Vec<&str> = text.lines().collect();
    if line == 0 || line > lines.len() {
        return String::new();
    }
    if !lines[line - 1].trim().starts_with("```redblue") {
        return String::new();
    }
    let mut body = String::new();
    for line in lines.iter().skip(line) {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            break;
        }
        body.push_str(line);
        body.push('\n');
    }
    body
}

// ============================================ the project structure tree

/// Every `src/…` entry the README's project-structure tree names exists.
///
/// The finding is one line: `README.md` listed `src/vm.rs`, which has not
/// existed since it was renamed to `src/interpreter.rs`. A tree that names a
/// file which is not there is worse than no tree — a reader follows it and
/// concludes the interpreter is missing from the project.
///
/// Entries are read out of the tree fence rather than hard-coded, so adding a
/// file to `src/` and to the tree needs no change here, and *removing* one from
/// either is caught.
#[test]
fn every_source_file_the_project_structure_lists_exists() {
    let text = read_doc("README.md");
    let listed = source_entries(&text);

    assert!(
        !listed.is_empty(),
        "no src/ entry was read out of README.md's project structure, so this \
         assertion is not checking anything: the tree fence must have changed shape"
    );

    let missing: Vec<String> = listed
        .iter()
        .filter(|entry| !Path::new(ROOT).join(entry).exists())
        .cloned()
        .collect();

    assert!(
        missing.is_empty(),
        "README.md's project structure names source files that do not exist: {missing:?}. \
         Either the file moved or the tree is stale; both are a reader's problem, not a \
         reader's guess."
    );
}

/// Reads the `src/…` paths out of the project-structure fence.
///
/// The fence is found by its root line, `redblue/`, rather than by a heading
/// that a future edit could rename. Inside it the tree is walked as a tree: a
/// line's depth is how many four-column groups of `│` and spaces precede its
/// `├──`, a token ending in `/` is a directory at that depth, and a `.rs` token
/// is a file in the directory one level up. So `├── src/` at depth 0 makes
/// `│   ├── lexer.rs` at depth 1 the path `src/lexer.rs`, and the test asserts
/// that path exists.
fn source_entries(text: &str) -> Vec<String> {
    let fence = match project_structure_fence(text) {
        Some(fence) => fence,
        None => return Vec::new(),
    };

    let mut directories: BTreeMap<usize, String> = BTreeMap::new();
    let mut entries = Vec::new();
    for line in fence {
        let indent = line
            .chars()
            .take_while(|ch| *ch == '│' || *ch == ' ')
            .count();
        let token = match line
            .split_whitespace()
            .find(|token| token.ends_with('/') || token.ends_with(".rs"))
        {
            Some(token) => token,
            None => continue,
        };
        let level = indent / 4;
        if let Some(name) = token.strip_suffix('/') {
            directories.insert(level, name.to_string());
            continue;
        }
        let Some(parent) = directories.get(&(level.saturating_sub(1))) else {
            continue;
        };
        entries.push(format!("{parent}/{token}"));
    }
    entries
}

/// The fenced block whose first non-blank line is the tree root `redblue/`.
fn project_structure_fence(text: &str) -> Option<Vec<String>> {
    let mut inside = false;
    let mut fence: Vec<String> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if !inside {
            if trimmed.starts_with("```") {
                inside = true;
                fence.clear();
            }
            continue;
        }
        if trimmed.starts_with("```") {
            if fence.first().map(|root| root.trim() == "redblue/") == Some(true) {
                return Some(fence);
            }
            inside = false;
            continue;
        }
        if !trimmed.is_empty() {
            fence.push(line.to_string());
        }
    }
    None
}

/// The tree check is a real check, not a formality that passes on anything.
///
/// The reader is fed the tree line this phase corrected — `vm.rs` under `src/` —
/// and it has to read it as `src/vm.rs` and hand back a path that does not
/// exist. Two negative fixtures follow: a fence with no tree root in it, and a
/// file listed at the top level with no directory above it. A reader that
/// returned nothing for all three would let the real README through unchecked,
/// so the count is asserted too.
#[test]
fn edge_a_documented_source_file_that_is_not_there_is_reported() {
    let tree = |files: &[&str]| -> String {
        let mut fence = vec![
            "```".to_string(),
            "redblue/".to_string(),
            "├── src/".to_string(),
        ];
        for file in files {
            fence.push(format!("│   ├── {file:<20}# comment"));
        }
        fence.push("└── tests/             # suites".to_string());
        fence.push("```".to_string());
        fence.join("\n")
    };

    assert_eq!(
        source_entries(&tree(&["lexer.rs", "stdlib.rs"])),
        vec!["src/lexer.rs".to_string(), "src/stdlib.rs".to_string()],
        "the tree reader must read a box-drawn src/ entry as a path, not as a comment tail"
    );
    assert_eq!(
        source_entries("no fence here at all"),
        Vec::<String>::new(),
        "a document with no project-structure fence yields no entries rather than a panic"
    );
    assert_eq!(
        source_entries("```\nredblue/\n├── lexer.rs  # listed under the tree root itself\n```"),
        vec!["redblue/lexer.rs".to_string()],
        "a file listed at the top of the tree resolves under the root the fence names, \
         which is the only parent a reader has to go on"
    );

    assert_eq!(
        source_entries(&tree(&["vm.rs"])),
        vec!["src/vm.rs".to_string()],
        "the tree reader must also read the file this phase removed, and report it missing"
    );
    assert!(
        !Path::new(ROOT).join("src/vm.rs").exists(),
        "src/vm.rs exists again, so this fixture no longer proves the check has teeth; \
         either it came back or the name changed and the fixture should follow it"
    );
    assert!(
        Path::new(ROOT).join("src/interpreter.rs").exists(),
        "src/interpreter.rs should exist; if it was renamed the fixture is testing nothing"
    );
}

// ============================================ the roadmap's tick boxes

/// README's self-hosting line must agree with what the repository does.
///
/// The finding: the line read `- [ ] Self-hosting: … (in progress …)` while
/// `bootstrap/compiler.rb` is over three thousand lines of Redblue and
/// `tests/bootstrap_selfhost_test.rs` asserts that three consecutive
/// self-compilations are byte-identical to each other. A roadmap that says
/// "in progress" about work that has reached its fixed point sends the next
/// person to do it again.
///
/// Both halves are checked, and the second is the half that matters: the line is
/// only allowed to say "done" while the test that proves it is still in the
/// suite. A ticked box with the fixed-point test deleted would be a claim with
/// nothing behind it, so the assertion reads the test file rather than trusting
/// the checkbox.
#[test]
fn edge_the_self_hosting_roadmap_line_matches_the_repository() {
    let readme = read_doc("README.md");
    let line = readme
        .lines()
        .find(|line| line.contains("Self-hosting"))
        .unwrap_or_else(|| {
            panic!("README.md has no self-hosting line at all; the roadmap checkbox moved")
        });

    let suite = std::fs::read_to_string(Path::new(ROOT).join("tests/bootstrap_selfhost_test.rs"))
        .expect("the self-host suite is checked in");
    assert!(
        suite.contains("edge_three_consecutive_self_compilations_are_byte_identical"),
        "the test that proves the fixed point is gone, so README.md is not allowed to \
         claim one: {line}"
    );

    let compiler = Path::new(ROOT).join("bootstrap/compiler.rb");
    let lines = std::fs::read_to_string(&compiler)
        .expect("the self-hosted compiler is checked in")
        .lines()
        .count();
    assert!(
        lines > 1000,
        "bootstrap/compiler.rb is {lines} lines; the self-hosting claim is being made \
         about a compiler that is not there"
    );

    assert!(
        line.contains("- [x]"),
        "README.md still marks self-hosting as not done, and it is: \
         bootstrap/compiler.rb is {lines} lines and the suite asserts three consecutive \
         self-compilations are byte-identical.\n  {line}"
    );
    assert!(
        !line.contains("in progress"),
        "README.md calls self-hosting 'in progress' and also ticks it. One of the two \
         is a lie:\n  {line}"
    );
}

// ============================================ the builtins KNOWN_GAPS once exempted

/// The five names `KNOWN_GAPS` used to exempt as "documented but not
/// implemented", each called for its value.
///
/// The finding's worst sentence: `console.clear()`, `time.unix(...)`,
/// `csv.parse(...)`, `network.get(...)` and `network.post(...)` **all work**,
/// and the exemption list was stale in the permissive direction — it stopped
/// testing things that answer. The list has since been reduced to `filter` and
/// `reduce`, so this test no longer catches a re-added exemption by reading the
/// list; it catches one by calling the name and asking for the answer.
///
/// `network_get` and `network_post` get the same loopback treatment
/// `tests/stdlib_module_docs_test.rs` gives them: a listener on `127.0.0.1:0`,
/// an ephemeral port on the loopback interface from which no packet leaves the
/// machine, so the answer is a constant the test owns.
#[test]
fn edge_the_builtins_known_gaps_exempted_all_answer_their_value() {
    says("p082_clear", "console.clear()", "\u{1b}[2J\u{1b}[1H");
    says(
        "p082_time_unix",
        r#"say time.unix("2024-01-01 00:00:00")"#,
        "1704067200",
    );
    says(
        "p082_csv",
        r#"say csv.parse("a,b\n1,2")"#,
        "[[a, b], [1, 2]]",
    );
    says(
        "p082_json",
        r#"say json.parse("{\"name\": \"Alice\"}")"#,
        "{name: Alice}",
    );

    let (address, server) = loopback_http();
    for (tag, source) in [
        (
            "p082_get",
            format!(r#"say network.get("{address}/greeting")"#),
        ),
        (
            "p082_post",
            format!(r#"say network.post("{address}/echo", "ping")"#),
        ),
    ] {
        let (ok, stdout, stderr) = run_named(tag, &source);
        assert!(
            ok,
            "the builtin was exempt from the registry walk and now refuses:\n{source}\n\
             --- stderr\n{stderr}"
        );
        assert!(
            stdout.contains("loopback"),
            "the loopback body did not come back:\n{source}\n--- stdout\n{stdout}"
        );
    }
    server.join().expect("the loopback thread ends");
}

/// A loopback listener and the thread answering two requests with
/// `NETWORK_BODY`.
///
/// `accept` is non-blocking with a deadline rather than blocking, so the thread
/// ends even if no program ever arrives and this test cannot hang the suite.
fn loopback_http() -> (String, std::thread::JoinHandle<()>) {
    const NETWORK_BODY: &str = "loopback";
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    listener
        .set_nonblocking(true)
        .expect("the listener accepts without blocking");
    let address = format!(
        "http://{}",
        listener.local_addr().expect("the loopback address")
    );

    let handle = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        for _ in 0..2 {
            let mut stream = None;
            while std::time::Instant::now() < deadline {
                match listener.accept() {
                    Ok((accepted, _)) => {
                        stream = Some(accepted);
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(_) => return,
                }
            }
            let mut stream = match stream {
                Some(stream) => stream,
                None => return,
            };
            use std::io::Write as _;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{NETWORK_BODY}",
                NETWORK_BODY.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    (address, handle)
}

// ============================================ the refusals the corrections rest on

/// Each forbidden claim is still false, so the documents that dropped it did
/// not drop it because it became true.
///
/// This is the test that keeps the corrections honest. A phase that "fixed" the
/// documentation by implementing `formats` or `list.filter` would make the
/// claim true — and would make the five documents true too, which sounds like
/// success. This says no: the refusal is the specification. If one of these ever
/// starts answering, that is a language change and it needs its own phase with
/// its own decision, not a doc edit discovered by a test.
#[test]
fn edge_every_claim_the_documents_dropped_is_still_a_refusal() {
    refuses("gap_formats", r#"say formats.parse_json("{}")"#, "formats");
    refuses(
        "gap_filter",
        "say list.filter([1, 2], to (x) give back x > 1 end)",
        "filter",
    );
    refuses("gap_random", "say math.random(1, 6)", "random");
    refuses("gap_ask", "set answer to ask \"Your name?\"", "ask");
    refuses(
        "gap_method",
        "set ys to xs.map(to (x) give back x * 2)",
        "map",
    );
}

// ============================================ the corrected examples run

/// The two corrections the finding named — `formats` and `list.filter` — are
/// pinned by name, in the document they were wrong in, and their replacements
/// are executed.
///
/// `no_document_promises_a_capability_the_binary_lacks` sweeps all five
/// documents for a list of literals. This is the same property written once per
/// defect, so that reverting either correction fails a test that says what the
/// defect was: the `formats` section went out of `HANDOUT.md` because SPEC.md
/// already said the module does not exist, and `list.filter` went out because
/// `tests/stdlib_module_docs_test.rs` already carries it in `KNOWN_GAPS`.
#[test]
fn edge_the_formats_and_list_filter_corrections_are_pinned() {
    let handout = read_doc("HANDOUT.md");

    for (needle, was) in [
        (
            "formats.parse_json",
            "the module SPEC.md says does not exist",
        ),
        ("formats.to_json", "the module SPEC.md says does not exist"),
        (
            "list.filter",
            "a function `KNOWN_GAPS` lists as still missing",
        ),
    ] {
        let left = occurrences("HANDOUT.md", &handout, needle);
        assert!(
            left.is_empty(),
            "HANDOUT.md shows `{needle}` again, and it is {was}:\n{}",
            left.join("\n")
        );
    }

    // The section it became is not an absence, it is a working pair. Both
    // modules, both directions, and the CSV one a reader is as likely to reach
    // for as the JSON one.
    assert!(
        handout.contains("### json and csv"),
        "HANDOUT.md's json/csv section is gone, so the two modules it replaced \
         `formats` with are documented nowhere a newcomer looks"
    );
    assert!(
        handout.contains("There is no `formats` module"),
        "HANDOUT.md dropped the example without saying why, which is how it comes back"
    );
    assert!(
        handout.contains("There is no `list.filter`"),
        "HANDOUT.md dropped `list.filter` without saying why"
    );

    says(
        "pin_json",
        r#"set data to json.parse("{ \"name\": \"Alice\" }")
say json.stringify(data)"#,
        r#"{"name": "Alice"}"#,
    );
    says("pin_csv", r#"say csv.parse("a,b")"#, "[[a, b]]");
}

/// The two names still in `KNOWN_GAPS` are still gaps, and no document presents
/// either as working.
///
/// `KNOWN_GAPS` is the last place in the test layer where a name is allowed to
/// answer nothing. `tests/stdlib_module_docs_test.rs` proves the names are
/// refused; this proves the documents agree, so a `filter` that starts working
/// fails here and in there rather than in neither.
#[test]
fn edge_the_remaining_known_gaps_are_documented_as_missing() {
    for name in ["filter", "reduce"] {
        let (ok, _, stderr) = run_named(
            &format!("known_{name}"),
            &format!("say list.{name}([1, 2, 3], to (a, b) give back a + b end)"),
        );
        assert!(
            !ok,
            "list.{name} answers now; take it out of KNOWN_GAPS in \
             tests/stdlib_module_docs_test.rs and add a row that calls it"
        );
        assert!(
            stderr.contains(name),
            "the refusal does not name `{name}`:\n--- stderr\n{stderr}"
        );
        for doc in DOCS {
            let text = read_doc(doc);
            assert!(
                occurrences(doc, &text, &format!("list.{name}")).is_empty(),
                "{doc} presents list.{name} as working, and it is a known gap"
            );
        }
    }
}

// ============================================ the corrected examples run

/// The examples that replaced the false claims are executed, not merely
/// written down.
///
/// A correction that is only checked by eye is a correction that can be
/// reverted silently, which is exactly how the `formats` section survived in
/// the first place: nothing ever ran it, so nothing ever noticed. Each program
/// here is the shape the corrected document shows, and its exact output is
/// pinned.
#[test]
fn edge_the_replacement_examples_answer() {
    // The `formats` section, rewritten as the two modules that exist. Both the
    // empty object and a populated one, because `json.parse` that answers
    // nothing for `{}` is a different bug from one that answers nothing for a
    // record.
    says("p082_json_empty", r#"say json.parse("{}")"#, "{}");
    says(
        "p082_json_record",
        r#"say json.parse("{ \"name\": \"Alice\" }")"#,
        "{name: Alice}",
    );
    says(
        "p082_json_back",
        r#"say json.stringify(json.parse("{ \"n\": 1 }"))"#,
        r#"{"n": 1}"#,
    );
    says("p082_csv", r#"say csv.parse("a,b")"#, "[[a, b]]");

    // The `list` section, rewritten around the one function it has.
    says(
        "p082_list_map",
        "set doubled to list.map([1, 2, 3], to (x) give back x * 2)\nsay doubled",
        "[2, 4, 6]",
    );
    // The empty list and the single element, because `map` over a list of
    // nothing is the case a one-element fixture cannot see.
    says(
        "p082_list_map_empty",
        "say list.map([], to (x) give back x * 2)",
        "[]",
    );
    says(
        "p082_list_map_one",
        "say list.map([5], to (x) give back x + 1)",
        "[6]",
    );
    // The closure body may see the surrounding scope, or `map` is a lie by
    // omission the same way `list.filter` was.
    says(
        "p082_list_map_captures",
        "set factor to 10\nsay list.map([1, 2], to (x) give back x * factor)",
        "[10, 20]",
    );
}

// ============================================ the project structure: directories

/// Every `/`-suffixed entry the README's project-structure tree names exists.
///
/// [`every_source_file_the_project_structure_lists_exists`] reads the same fence
/// but keeps only the `.rs` tokens, because the finding it pins was a missing
/// *file* (`src/vm.rs`). The directories were therefore never checked by
/// anything: `source_entries` records them as parent directories and returns
/// nothing for them. A tree naming `src/repl/` or `tooling/vscode/` that has
/// moved would pass every gate in this repository, and a reader who follows it
/// concludes the REPL or the editor extension is not shipped.
///
/// Nine directories are named and all nine are checked, including the deepest
/// one (`tooling/vscode/`, two levels below the tree root), because a reader
/// that stopped at depth 1 would find the top eight and miss the ninth.
#[test]
fn every_directory_the_project_structure_names_exists() {
    let listed = directory_entries(&read_doc("README.md"));

    assert!(
        !listed.is_empty(),
        "no directory was read out of README.md's project structure, so this \
         assertion is not checking anything: the tree fence must have changed shape"
    );

    let missing: Vec<String> = listed
        .iter()
        .filter(|entry| !Path::new(ROOT).join(entry).is_dir())
        .cloned()
        .collect();

    assert!(
        missing.is_empty(),
        "README.md's project structure names directories that are not there: {missing:?}. \
         A reader follows a directory line before a file line, so a missing one sends \
         them looking for a subtree that does not exist."
    );
}

/// The `/`-suffixed paths out of the project-structure fence.
///
/// The mirror image of [`source_entries`]: same fence, same depth arithmetic,
/// and the two halves of the tree collected separately so a missing file and a
/// missing directory are reported as the different problems they are. A token
/// ending in `/` is a directory *at* its own depth and a `.rs` token is a file
/// in the directory one level up, so `├── src/` at depth 0 makes
/// `│   ├── bytecode/` at depth 1 the path `src/bytecode/` and
/// `│   ├── lexer.rs` at depth 1 the path `src/lexer.rs`.
fn directory_entries(text: &str) -> Vec<String> {
    let Some(fence) = project_structure_fence(text) else {
        return Vec::new();
    };

    let mut directories: BTreeMap<usize, String> = BTreeMap::new();
    let mut entries = Vec::new();
    // The fence's first line is the tree root — the checkout's own directory
    // name — and not a child of anything, so it is not an entry.
    for line in fence.iter().skip(1) {
        let indent = line
            .chars()
            .take_while(|ch| *ch == '│' || *ch == ' ')
            .count();
        let Some(token) = line
            .split_whitespace()
            .find(|token| token.ends_with('/') || token.ends_with(".rs"))
        else {
            continue;
        };
        let level = indent / 4;
        if let Some(name) = token.strip_suffix('/') {
            let parent = if level == 0 {
                String::new()
            } else {
                match directories.get(&(level - 1)) {
                    Some(parent) => format!("{parent}/"),
                    None => continue,
                }
            };
            directories.insert(level, name.to_string());
            entries.push(format!("{parent}{name}"));
        }
    }
    entries
}

/// The directory reader reports what is not there, and reads what is.
///
/// The fixtures are the two mistakes that would each make the real assertion
/// pass on nothing: a tree naming `src/vmr/`, one level deep and spelled like
/// the directory that does exist, and `tooling/emacs/`, the missing sibling of
/// the `tooling/vscode/` the README really names. Both have to come back as
/// paths that do not resolve, and the reader has to reach depth 2 to find the
/// second one. A reader that dropped directories, or stopped at the first level,
/// would return nothing for either and let a stale README through.
#[test]
fn edge_a_directory_the_project_structure_names_that_is_not_there_is_reported() {
    // Joined rather than written with `\` line continuations, which would strip
    // the leading `│   ` and the nesting the fixture is about.
    let tree = [
        "```",
        "redblue/",
        "├── src/",
        "│   ├── bytecode/       # Bytecode format, compiler, VM",
        "│   ├── vmr/            # the directory this phase checks for",
        "│   └── lexer.rs        # Tokenizer",
        "├── tests/              # Rust + Redblue test suites",
        "└── tooling/",
        "    └── emacs/          # a sibling of the real tooling/vscode/",
        "```",
    ]
    .join("\n");

    assert_eq!(
        directory_entries(&tree),
        vec![
            "src".to_string(),
            "src/bytecode".to_string(),
            "src/vmr".to_string(),
            "tests".to_string(),
            "tooling".to_string(),
            "tooling/emacs".to_string(),
        ],
        "the directory reader must join a nested entry to its own parent, not to the root"
    );

    let resolved = |name: &str| Path::new(ROOT).join(name).is_dir();
    assert!(
        !resolved("src/vmr"),
        "src/vmr exists, so this fixture no longer proves the check has teeth; \
         either it was created or the name changed and the fixture should follow it"
    );
    assert!(
        !resolved("tooling/emacs"),
        "tooling/emacs exists, so the nested fixture proves nothing"
    );
    assert!(
        resolved("src/bytecode") && resolved("tooling/vscode"),
        "the real directories must exist or the fixtures beside them prove nothing"
    );

    // Boundary: a document with no fence, and one whose fence has no tree root,
    // both yield no entries rather than a panic.
    assert_eq!(
        directory_entries("no fence here at all"),
        Vec::<String>::new()
    );
    assert_eq!(
        directory_entries("```\nsome other code block\n```"),
        Vec::<String>::new()
    );
}

// ============================================ every bucket, not just RUNS

/// The `doc:line` keys a bucket file holds, comments and blanks removed.
///
/// One reader for all three files, so the three scans below cannot drift apart
/// in how they parse a key: a key that is not `doc:<number>` is a `panic!`
/// naming it, because a bucket this file cannot read is a bucket it cannot
/// vouch for.
fn bucket_keys(name: &str) -> Vec<String> {
    let path = Path::new(ROOT).join("tests").join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{} must be checked in for this scan: {error}",
            path.display()
        )
    });
    parse_bucket_keys(name, &text)
}

/// [`bucket_keys`] on text given directly, so the refusals can be driven from a
/// fixture rather than only from a checked-in file.
///
/// Every malformed input is a `panic!` naming the line, never an `unwrap` and
/// never a key skipped in silence: a bucket entry this reader cannot read is an
/// entry every scan below would silently not check.
fn parse_bucket_keys(name: &str, text: &str) -> Vec<String> {
    let keys: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let key = line.split_once('\t').map_or(line, |(key, _)| key);
            let (doc, rest) = key
                .split_once(':')
                .unwrap_or_else(|| panic!("{name} holds a key that is not `doc:line`: {line}"));
            let number: usize = rest
                .trim()
                .parse()
                .unwrap_or_else(|_| panic!("{name} holds a line that is not a number: {line}"));
            assert!(
                DOCS.contains(&doc),
                "{name} holds a key naming a document this file never reads: {doc}:{number}. \
                 The key would be checked against nothing."
            );
            format!("{doc}:{number}")
        })
        .collect();
    assert!(
        !keys.is_empty(),
        "{name} is empty, so scanning it would pass on nothing"
    );
    keys
}

/// The 1-based line of every opening ```redblue fence in `text`.
fn block_openings(text: &str) -> Vec<usize> {
    let mut lines = Vec::new();
    let mut inside = false;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("```redblue") {
            inside = true;
            lines.push(index + 1);
        } else if inside && trimmed.starts_with("```") {
            inside = false;
        }
    }
    lines
}

/// No block in *any* of the three buckets may contain a `'`, not only the ones
/// in the RUNS file.
///
/// [`edge_no_running_document_block_contains_a_single_quote`] reads one bucket.
/// The other two are where a single-quoted example can hide: demote a broken
/// block from RUNS to FRAGMENT with a plausible reason and nothing executes it,
/// or move it to KNOWN-FAIL and the recorded stderr stops being about the block
/// it names. Both routes end with `HANDOUT.md` promising `formats.parse_json`
/// again and every gate green. All three files are read here, which is the
/// only way that route is closed.
#[test]
fn edge_no_document_block_in_any_bucket_contains_a_single_quote() {
    let mut scanned = 0usize;
    for bucket in [
        "doc_blocks_runs.txt",
        "doc_blocks_fragments.txt",
        "doc_blocks_known_fail.txt",
    ] {
        for key in bucket_keys(bucket) {
            let (doc, line) = key
                .split_once(':')
                .unwrap_or_else(|| panic!("{bucket} produced a key that is not `doc:line`: {key}"));
            let body = block_body(
                doc,
                line.parse()
                    .unwrap_or_else(|_| panic!("{bucket} produced a non-numeric line: {key}")),
            );
            assert!(
                !body.trim().is_empty(),
                "{bucket} lists {key} and there is no block there. \
                 tests/doc_blocks_test.rs owns that check; asserting it here too would \
                 make one stale key fail two tests."
            );
            scanned += 1;
            if let Err(offence) = first_single_quoted(&[(key, body)]) {
                panic!("classified in {bucket}, {offence}");
            }
        }
    }

    assert!(
        scanned > 100,
        "only {scanned} blocks were scanned for a quote the lexer refuses; the three \
         buckets hold more than a hundred keys between them, so the scan missed some"
    );
}

/// The first `(key, body)` whose body opens a single-quoted string, or `Ok(())`.
///
/// A `Result`, not a `bool`, because the caller needs the *which block* and not
/// only the *that one exists*: three buckets are scanned, and "a document
/// contains a single quote" is not an actionable failure message while
/// `HANDOUT.md:149 is classified in doc_blocks_runs.txt and its body opens a
/// single-quoted string` is. The failure channel is exercised directly by
/// [`edge_the_single_quote_scan_names_the_block_it_found`], so this cannot be a
/// function that has only ever been shown the clean case.
fn first_single_quoted(blocks: &[(String, String)]) -> Result<(), String> {
    for (key, body) in blocks {
        if quote_opens_a_string(body) {
            return Err(format!(
                "{key} opens a single-quoted string, which the lexer refuses:\n{body}"
            ));
        }
    }
    Ok(())
}

/// The scan reports the block it found, and is silent when there is nothing.
///
/// A scan whose every outcome is `Ok` cannot be shown to find anything, which is
/// the gap this file closed in its previous attempt: three tests asserted the
/// absence of a quote in the documents and none of them could fail for the
/// right reason if the rule underneath them stopped working. So the rule's own
/// checker is handed the exact body this phase removed from `HANDOUT.md` — the
/// `formats.parse_json('{"name": "Alice"}')` the finding quotes — and must come
/// back `Err` naming it.
///
/// The neighbours are here too, because a checker that reported every `'` would
/// fail on `SPEC.md:143`, which writes the escape it refuses inside a comment,
/// and one that reported none would fail on the first case.
#[test]
fn edge_the_single_quote_scan_names_the_block_it_found() {
    let reverted = (
        "HANDOUT.md:149".to_string(),
        r#"set data to formats.parse_json('{"name": "Alice"}')"#.to_string(),
    );
    let err = first_single_quoted(&[reverted])
        .expect_err("the body this phase removed opens a single-quoted string");
    assert!(
        err.contains("HANDOUT.md:149"),
        "the report does not name the block, so nobody can act on it: {err}"
    );
    assert!(
        err.contains("single-quoted"),
        "the report does not say what is wrong: {err}"
    );

    // Empty and singleton: nothing to find, and one clean block.
    assert!(
        first_single_quoted(&[]).is_ok(),
        "an empty scan has nothing to report and must say so"
    );
    assert!(
        first_single_quoted(&[("SPEC.md:1".to_string(), "say \"hi\"".to_string())]).is_ok(),
        "one clean block is not an offence"
    );

    // The two shapes a blunt rule gets wrong, in the other direction.
    for (key, body) in [
        (
            "SPEC.md:684",
            r#"object Person
    has name
    to introduce()
        say "I'm {this.name}"
    end
end"#,
        ),
        (
            "SPEC.md:143",
            "say \"\\q\"            // LexerError: Unknown escape '\\q' in text",
        ),
    ] {
        assert!(
            first_single_quoted(&[(key.to_string(), body.to_string())]).is_ok(),
            "{key} contains a quote that cannot open a string, so reporting it would \
             make the scan a ban on apostrophes"
        );
    }

    // The first offence is the one reported, out of a list of three.
    let many = [
        ("README.md:9".to_string(), "say \"hi\"".to_string()),
        ("README.md:33".to_string(), "say 'hi'".to_string()),
        ("README.md:46".to_string(), "say 'bye'".to_string()),
    ];
    let err = first_single_quoted(&many).expect_err("two of the three offend");
    assert!(
        err.contains("README.md:33") && !err.contains("README.md:46"),
        "the first offence in order is the one to report: {err}"
    );
}

/// A bucket key that is not `doc:<number>` is refused, and the refusal names it.
///
/// The reader below turns three checked-in files into the keys every scan in
/// this file runs on, so a key it cannot parse is a key nothing checks. Silently
/// dropping it would leave a whole bucket unscanned with every test green, which
/// is how a block ends up in a list that no longer describes it.
#[test]
#[should_panic(expected = "holds a key that is not `doc:line`")]
fn edge_a_bucket_key_that_is_not_doc_line_is_rejected() {
    parse_bucket_keys("fixture.txt", "# a comment\n\nREADME.md:9\nREADME.md\n");
}

/// A bucket line whose number is not a number is refused the same way.
#[test]
#[should_panic(expected = "holds a line that is not a number")]
fn edge_a_bucket_line_that_is_not_a_number_is_rejected() {
    parse_bucket_keys("fixture.txt", "README.md:nine\n");
}

/// A key naming a document this file never reads is refused.
///
/// Otherwise the key resolves against a document no scan opens, and the block it
/// names is one nothing has looked at.
#[test]
#[should_panic(expected = "naming a document this file never reads")]
fn edge_a_bucket_key_naming_an_unread_document_is_rejected() {
    parse_bucket_keys("fixture.txt", "ROADMAP.md:1\n");
}

/// An empty bucket file is a failure, not a clean scan.
#[test]
#[should_panic(expected = "is empty, so scanning it would pass on nothing")]
fn edge_an_empty_bucket_file_is_rejected() {
    parse_bucket_keys("fixture.txt", "# every line is a comment\n\n");
}

/// Every ```redblue block in the five documents is in one of the three buckets
/// this file scans.
///
/// The hole the previous test closes is only closed if the buckets *cover* the
/// documents. A block in none of them is executed by nothing, corrected by
/// nothing and described by nothing: the `formats` section could be deleted
/// from `HANDOUT.md` entirely, or a new `list.filter` example added and left out
/// of every list, and the quote scan and the claims scan would both read a
/// document that no longer says what it says now and say nothing.
///
/// So this asserts the union of the three buckets is exactly the set of blocks —
/// no key pointing at nothing, and no block missing from all three.
#[test]
fn edge_every_documentation_block_is_in_a_bucket_this_file_scans() {
    let mut keys: Vec<String> = Vec::new();
    for bucket in [
        "doc_blocks_runs.txt",
        "doc_blocks_fragments.txt",
        "doc_blocks_known_fail.txt",
    ] {
        keys.extend(bucket_keys(bucket));
    }

    let mut blocks: Vec<String> = Vec::new();
    for doc in DOCS {
        for line in block_openings(&read_doc(doc)) {
            blocks.push(format!("{doc}:{line}"));
        }
    }

    let duplicated: Vec<String> = keys
        .iter()
        .filter(|key| keys.iter().filter(|other| *other == *key).count() > 1)
        .cloned()
        .collect();
    assert!(
        duplicated.is_empty(),
        "a block is in two buckets at once, so this file cannot say which one it was \
         checked against: {duplicated:?}"
    );

    let unclassified: Vec<String> = blocks
        .iter()
        .filter(|key| !keys.contains(key))
        .cloned()
        .collect();
    assert!(
        unclassified.is_empty(),
        "these ```redblue blocks are in none of the three buckets, so nothing in this \
         file reads them: {unclassified:?}. An example that no bucket holds is an example \
         nothing runs."
    );

    assert_eq!(
        blocks.len(),
        keys.len(),
        "the buckets hold {} keys and the documents hold {} blocks; the two must be the \
         same set",
        keys.len(),
        blocks.len()
    );
}

/// The quote rule catches an opening quote and spares an apostrophe.
///
/// Both directions, because the check in
/// [`edge_no_document_block_in_any_bucket_contains_a_single_quote`] is only
/// worth running if the rule underneath it distinguishes the two. Ban every
/// apostrophe and the scan goes green by refusing `say "I'm {this.name}"`,
/// which is `SPEC.md`'s own object example and a `RUNS` entry. Catch every
/// opening quote and the scan goes red on a block Redblue runs.
///
/// The tricky case is the one Redblue itself needs: a `'` straight after a
/// closing `"`, which reads like an opening quote after the eye has finished
/// the string and is not one.
#[test]
fn edge_the_single_quote_rule_spares_an_apostrophe_and_catches_a_quote() {
    for body in [
        r#"set data to formats.parse_json('{"name": "Alice"}')"#,
        "say 'hi'",
        "say json.parse('{ \"n\": 1 }')",
        "set xs to ['a', 'b']",
        r#"say "it's" + 'no'"#,
    ] {
        assert!(
            quote_opens_a_string(body),
            "a single-quoted string is here and the rule missed it, so a broken example \
             would pass the scan:\n{body}"
        );
    }

    for body in [
        r#"say "I'm {this.name}""#,
        r#"say "don't say that""#,
        "say \"it's {x}'s\"",
        r#"set greeting to "hello, world""#,
        "",
        "set apostrophe to 39",
    ] {
        assert!(
            !quote_opens_a_string(body),
            "no quote opens a string in this body, and the rule claimed one did, so \
             `say \"I'm {{this.name}}\"` would be reported as a broken example:\n{body}"
        );
    }

    // The empty string and the singleton are the boundaries: one character of
    // body, and one `'` with nothing around it at all.
    assert!(!quote_opens_a_string(""));
    assert!(quote_opens_a_string("'"));
    assert!(!quote_opens_a_string(r#"""#));

    // Premise for the scan above: the block the rule must spare is a real one in
    // a real document, in the RUNS bucket, so sparing it is not a way of saying
    // "the scan does not reach it".
    let spec = read_doc("SPEC.md");
    let runs = bucket_keys("doc_blocks_runs.txt");
    let spared: Vec<String> = runs
        .iter()
        .filter(|key| {
            let (doc, line) = key.split_once(':').expect("a key is doc:line");
            block_body(doc, line.parse().expect("a key line is a number")).contains(r#"I'm"#)
        })
        .cloned()
        .collect();
    assert!(
        !spared.is_empty(),
        "no RUNS block in SPEC.md uses an apostrophe inside a string, so the \
         `quote_opens_a_string` cases above are checked against a body the \
         repository does not actually contain: {spec:?}"
    );
}

// ============================================ the corrections are executed

/// The 1-based line of the fenced ```redblue block in `text` whose body
/// contains `needle`, or `None` when no block does.
///
/// The line, not the literal, because the bucket files are keyed on it — so a
/// correction is located by *what it says* and then looked up in the buckets,
/// which means moving an example up a document does not quietly drop it out of
/// the bucket and out of this test.
fn block_line_containing(text: &str, needle: &str) -> Option<usize> {
    let lines: Vec<&str> = text.lines().collect();
    let mut inside = false;
    let mut start = 0usize;
    let mut body = String::new();
    let mut found = None;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("```redblue") {
            inside = true;
            start = index + 1;
            body.clear();
        } else if inside && trimmed.starts_with("```") {
            inside = false;
            if body.contains(needle) && found.is_none() {
                found = Some(start);
            }
        } else if inside {
            body.push_str(line);
            body.push('\n');
        }
    }
    found
}

/// Each corrected example is in the RUNS bucket, so the suite executes it.
///
/// The phase's own definition of done says the corrected examples must be in a
/// bucket produced by the harness *and* executed by a test. Being executed is
/// checked here and by [`edge_the_replacement_examples_answer`]; this is the
/// other half — that the *document's* block, the one a reader copies, is the
/// same program and is classified as working. A correction written in one place
/// while the document keeps a fragment that was never run would satisfy the
/// first and not this.
///
/// Each needle is a line of a corrected document, not a claim about one, so
/// deleting the correction fails the same way reverting it does.
#[test]
fn edge_the_corrected_examples_are_executed_from_the_bucket_that_holds_them() {
    let runs = bucket_keys("doc_blocks_runs.txt");
    let fragments = bucket_keys("doc_blocks_fragments.txt");
    let known_fail = bucket_keys("doc_blocks_known_fail.txt");

    for (doc, needle, what) in [
        (
            "HANDOUT.md",
            "json.parse",
            "the section that replaced `formats`",
        ),
        (
            "HANDOUT.md",
            "csv.parse",
            "the CSV half of the same replacement",
        ),
        ("SPEC.md", "list.map(", "the `list` module example"),
        (
            "PHILOSOPHY.md",
            "set active to yes",
            "the block the `formats.parse_json` and `set active` corrections landed in",
        ),
    ] {
        let key = block_line_containing(&read_doc(doc), needle).unwrap_or_else(|| {
            panic!(
                "no ```redblue block in {doc} contains `{needle}` any more, so the \
                 correction this phase made there is gone. {what} is not documented."
            )
        });
        let key = format!("{doc}:{key}");

        assert!(
            runs.contains(&key),
            "{key} holds {what} but is not in tests/doc_blocks_runs.txt, so nothing \
             executes the example a reader copies out of {doc}. It is in FRAGMENT: \
             {}, or in KNOWN-FAIL: {}.",
            fragments.contains(&key),
            known_fail.contains(&key)
        );
        assert!(
            !quote_opens_a_string(&block_body(
                doc,
                key.rsplit(':').next().unwrap().parse().unwrap_or(0)
            )),
            "{key} holds {what} and opens a single-quoted string, which is the defect \
             this phase corrected"
        );
    }
}

// ============================================ a refusal, in full

/// The refusal a single-quoted program gets is a caret on the quote, not just a
/// word in a message.
///
/// [`edge_a_single_quoted_string_in_a_program_is_a_lexer_error`] pins the
/// message. This pins what a person sees, because a `LexerError` that named the
/// character but not the place would send them to the start of the file. The
/// file is one line, so `1:5` is the only position it can be: the quote is the
/// fifth character of `say 'hi'`.
///
/// The unterminated string is the neighbouring failure and is here so the two
/// cannot be confused: both are lexer errors at `1:5`, and only the message
/// says which of the two mistakes was made.
#[test]
fn edge_a_single_quoted_program_is_refused_with_a_caret_at_the_quote() {
    let (ok, _, stderr) = run_named("p082_quote", "say 'hi'\n");

    assert!(
        !ok,
        "a single-quoted program was accepted; the lexer must refuse it"
    );
    assert!(
        stderr.contains("LexerError"),
        "the refusal is not a lexer error, so it is not saying what is wrong:\n{stderr}"
    );
    assert!(
        stderr.contains("Unexpected character"),
        "the refusal does not name the offending character:\n{stderr}"
    );
    assert!(
        stderr.contains(r"'\''"),
        "the refusal does not show the quote itself:\n{stderr}"
    );
    assert!(
        stderr.contains(":1:5"),
        "the refusal does not point at column 5 of line 1, which is where the quote is:\n{stderr}"
    );
    assert!(
        stderr
            .lines()
            .any(|line| line.trim_start().starts_with('|') && line.contains('^')),
        "the refusal prints no caret, so a reader is told what is wrong and not where:\n{stderr}"
    );
    assert!(
        stderr.contains("say 'hi'"),
        "the refusal does not echo the offending line:\n{stderr}"
    );

    // The neighbouring failure: same position, different message.
    let (ok, _, stderr) = run_named("p082_unterminated", "say \"hi\n");
    assert!(!ok, "an unterminated string was accepted");
    assert!(
        stderr.contains("Unterminated string"),
        "the two lexer failures must not be confused:\n{stderr}"
    );
    assert!(
        !stderr.contains("Unexpected character"),
        "an unterminated string is not an unexpected character:\n{stderr}"
    );
}
