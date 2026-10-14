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
/// Redblue has no single-quoted string, so a `'` is a lexer error — except when
/// it is an apostrophe inside a double-quoted one, which is legal and which
/// `SPEC.md` itself uses: `say "I'm {this.name}"` is a `RUNS` entry. The two
/// are told apart by what precedes the quote: an opening quote follows
/// whitespace, an opening bracket, a comma or nothing at all, while an
/// apostrophe follows a word character, a closing bracket or a closing quote.
///
/// Getting this wrong in the permissive direction is the bug this test exists to
/// prevent — a `RUNS` block with `'{"name": "Alice"}'` passes today only because
/// the check is not this blunt.
fn quote_opens_a_string(body: &str) -> bool {
    body.char_indices().any(|(index, ch)| {
        ch == '\''
            && match body[..index].chars().next_back() {
                None => true,
                Some(previous) => {
                    !previous.is_alphanumeric()
                        && previous != ')'
                        && previous != ']'
                        && previous != '"'
                }
            }
    })
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
