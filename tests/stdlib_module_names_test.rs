//! SPEC.md, README.md and `src/stdlib.rs` must name the same modules.
//!
//! `stdlib::MODULES` is the list the analyzer consults before a `.`: a bare
//! word that is in it is a namespace, and a bare word that is not is
//! `Unknown variable`. So the two documents and that list are one surface, and
//! a name that appears in only one of the three is a defect somebody reads:
//!
//! - a name a document names and the list does not hold is a program that dies
//!   in the analyzer, one step before the runtime could say what was wrong;
//! - a name the list holds and no document names is a namespace that exists and
//!   nothing explains, which is how `bytes` and `sys` came to be callable
//!   spellings of working functions that SPEC.md never mentioned.
//!
//! The lists are *extracted* from the documents rather than transcribed, and
//! each document is reconciled **on its own** rather than through a union of
//! the two. A union is weaker than either document: a name only one of them
//! gives still passes, so SPEC.md could drop `time` — which it had — or
//! README.md could drop `console` without a test noticing, and this phase
//! exists because a document and the registry drifted apart.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use redblue::stdlib;

/// The `## Standard Library` section of `doc`, headings included.
fn standard_library_section(doc: &str) -> String {
    let mut section = String::new();
    let mut inside = false;
    for line in doc.lines() {
        // `### name` does not match `## `: its third character is `#`, not a
        // space, so a subsection does not close the section.
        if line.starts_with("## ") {
            inside = line.trim() == "## Standard Library";
        }
        if inside {
            section.push_str(line);
            section.push('\n');
        }
    }
    section
}

/// The lines of the first fenced code block under `heading` in `doc`.
///
/// A section whose examples are each a complete statement can be *run* rather
/// than paraphrased by a test, and running them is the only way "the documented
/// examples work" is a claim that can fail. It stops at the next heading, so a
/// section's block is its own and not the one after it.
fn code_block_under(doc: &str, heading: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut under = false;
    let mut in_code = false;

    for line in doc.lines() {
        if line.starts_with("### ") {
            if under {
                break;
            }
            under = line.trim_end() == heading;
            continue;
        }
        if !under {
            continue;
        }
        if line.trim_start().starts_with("```") {
            if in_code {
                break;
            }
            in_code = true;
            continue;
        }
        if in_code {
            lines.push(line.to_string());
        }
    }

    assert!(
        !lines.is_empty(),
        "{heading} carries no code block, so there is nothing to run. A section of \
         examples a test cannot execute has to say so rather than be one."
    );
    lines
}

/// `line` with a trailing `//` comment removed, quote-aware.
///
/// `"http://x"` is a string, not the start of a comment, so a line that carries
/// one cannot be cut in half.
fn strip_comment(line: &str) -> &str {
    let chars: Vec<char> = line.chars().collect();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for (i, c) in chars.iter().enumerate() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if *c == '\\' {
                escaped = true;
            } else if *c == open {
                quote = None;
            }
            continue;
        }
        if *c == '"' || *c == '\'' {
            quote = Some(*c);
        } else if *c == '/' && chars.get(i + 1) == Some(&'/') {
            let mut end = i;
            while end > 0 && chars[end - 1].is_whitespace() {
                end -= 1;
            }
            let byte = line
                .char_indices()
                .map(|(byte, _)| byte)
                .nth(end)
                .unwrap_or(0);
            return &line[..byte];
        }
    }
    line
}

/// The module names a document names.
///
/// Two rules, and they are both needed:
///
/// 1. every dotted receiver inside a fenced code block — `text.uppercase("hi")`
///    names `text`. Headings alone are not enough (`### json and csv` is two
///    names in one heading) and calls alone are not enough for a section whose
///    first line is `set pi to PI` and whose module is named nowhere but the
///    heading.
/// 2. every word of every `### ` heading in the section, lowercased — which is
///    why the headings have to name modules rather than topics.
///
/// Both rules run over the code block text only, so prose like `` `math.PI` ``
/// is not a module receiver and `math.PI` stays the refusal
/// `stdlib_modules_test.rs` asserts it is.
fn documented_modules(section: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut in_code = false;

    for line in section.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }

        if line.starts_with("### ") {
            for word in line.trim_start_matches('#').split_whitespace() {
                let word = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
                // `### json and csv` is two names in one heading, and `and` is
                // the word joining them rather than one of them.
                if !word.is_empty() && word != "and" {
                    names.insert(word.to_lowercase());
                }
            }
            continue;
        }

        if in_code {
            names.extend(dotted_receivers(line));
        }
    }

    names
}

/// The `module.member` spellings the code blocks of `section` write.
///
/// Only code blocks, for the reason [`documented_modules`] gives: prose carries
/// names that are *not* calls — `` `math.PI` `` is the one SPEC.md uses to say a
/// module has no members — and a gate that read prose would either fail on a
/// sentence or, worse, be taught to skip it.
fn documented_members(section: &str) -> BTreeSet<String> {
    let mut members = BTreeSet::new();
    let mut in_code = false;

    for line in section.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            members.extend(
                dotted_calls(line)
                    .into_iter()
                    .map(|(module, member)| format!("{module}.{member}")),
            );
        }
    }

    members
}

/// The identifiers immediately before a `.` on `line`.
///
/// Two things that are not receivers are skipped, and both of them are in the
/// documents as written: string literals — `files.read("config.rb")` names
/// `files`, not `config`, and `network.get("https://api.example.com")` names
/// `network`, not `api` — and `//` comments, where `e.g.` is not a module.
///
/// `1.5` is not a receiver either, because the leading character is a digit, and
/// neither is the `r` of a quoted string, because a receiver is an identifier
/// followed by a `.` followed by another identifier.
fn dotted_receivers(line: &str) -> BTreeSet<String> {
    dotted_calls(line)
        .into_iter()
        .map(|(module, _)| module)
        .collect()
}

/// The `module.member` pairs written on `line`, as `("module", "member")`.
///
/// The same scan as [`dotted_receivers`], keeping the half of each hit that says
/// *which* function the module is being asked for. The receiver alone is not
/// enough for the per-member check: `list_map` on its own made the `list`
/// namespace look complete while `list.filter` and `list.reduce` reached builtins
/// that are registered and refuse every call, and a per-module check cannot see
/// either of them.
fn dotted_calls(line: &str) -> BTreeSet<(String, String)> {
    let identifier_start = |c: char| c.is_ascii_alphabetic() || c == '_';
    let identifier_part = |c: char| c.is_ascii_alphanumeric() || c == '_';

    let mut names = BTreeSet::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    while i < chars.len() {
        let c = chars[i];

        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == open {
                quote = None;
            }
            i += 1;
            continue;
        }

        if c == '"' || c == '\'' {
            quote = Some(c);
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            break;
        }
        if !identifier_start(c) {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && identifier_part(chars[i]) {
            i += 1;
        }
        let word: String = chars[start..i].iter().collect();

        let mut j = i;
        while j < chars.len() && chars[j].is_whitespace() {
            j += 1;
        }
        let is_a_receiver = j < chars.len()
            && chars[j] == '.'
            && chars.get(j + 1).is_some_and(|c| identifier_start(*c));
        if !is_a_receiver {
            continue;
        }

        let member_start = j + 1;
        let mut k = member_start;
        while k < chars.len() && identifier_part(chars[k]) {
            k += 1;
        }
        let member: String = chars[member_start..k].iter().collect();
        names.insert((word.to_lowercase(), member.to_lowercase()));
        i = k;
    }
    names
}

fn read_doc(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{name} is readable: {error}"))
}

/// The module names each document gives, keyed by the document they came from.
///
/// Each document is reconciled **on its own**, and that is the whole point. A
/// union of the two is weaker than either: a name only one of them gives still
/// passes, so SPEC.md could drop `time` — which it did — or README.md could drop
/// `console` without a test noticing. This phase exists because a document and
/// the registry drifted apart, so "the two documents between them cover
/// everything" is not the property that keeps them from drifting again.
fn names_each_document_gives() -> Vec<(&'static str, BTreeSet<String>)> {
    ["SPEC.md", "README.md"]
        .into_iter()
        .map(|name| {
            let documented = documented_modules(&standard_library_section(&read_doc(name)));
            (name, documented)
        })
        .collect()
}

/// The `## Standard Library` section of each document, keyed by the document.
fn standard_library_sections() -> Vec<(&'static str, String)> {
    ["SPEC.md", "README.md"]
        .into_iter()
        .map(|name| (name, standard_library_section(&read_doc(name))))
        .collect()
}

/// The environment variables that decide whether `network` talks to a proxy.
///
/// They are removed from every child this file runs. `runtime::network_client`
/// builds a `reqwest::blocking::Client` with its defaults, and those defaults
/// read the proxy variables of whatever process happens to be running — which is
/// how the `network` row used to pass locally and fail on a CI runner with
/// `HTTP_PROXY` set: the request for `127.0.0.1` left the machine, the loopback
/// server never saw it, and the answer was a proxy error rather than the body the
/// test pinned. A test that owns the network has to own the route to it.
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

/// Runs `source` under `tag` and returns `(exit success, stdout, stderr)`.
///
/// Stdin is closed rather than inherited. A program that calls `input` reads
/// end-of-input immediately and answers the empty text instead of blocking a test
/// suite on a terminal nobody is sitting at.
fn run_named(tag: &str, source: &str) -> (bool, String, String) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/p50names");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    let file = dir.join(format!("{tag}.rb"));
    std::fs::write(&file, source).expect("the program is written");

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

/// Runs `source` under `tag` with `typed` on its standard input.
fn run_with_stdin(tag: &str, source: &str, typed: &str) -> (bool, String, String) {
    use std::io::Write;
    use std::process::Stdio;

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/p50names");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    let file = dir.join(format!("{tag}.rb"));
    std::fs::write(&file, source).expect("the program is written");

    let mut child = {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rb"));
        command.arg("run").arg(&file);
        for name in PROXY_ENV {
            command.env_remove(name);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("rb runs")
    };
    child
        .stdin
        .take()
        .expect("the child's stdin was piped")
        .write_all(typed.as_bytes())
        .expect("the input reaches the program");

    let out = child.wait_with_output().expect("rb finishes");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// ============================================================ one name, one place

/// The forward direction: a module a document names must be in `MODULES`.
///
/// This is the direction a user's program fails in. `text.uppercase("hi")` is
/// written in SPEC.md, and without `text` in the list it is `AnalyzerError:
/// Unknown variable 'text'` — the analyzer's message for a name it has never
/// heard of, applied to a name the specification spells out.
#[test]
fn edge_a_module_the_documents_name_is_in_the_registry() {
    for (document, documented) in names_each_document_gives() {
        for name in &documented {
            assert!(
                stdlib::is_module(name),
                "{document} names the module `{name}` and stdlib::MODULES does not, \
                 so a program using it dies as 'Unknown variable \\'{name}\\''. \
                 Add `{name}` to stdlib::MODULES, or correct the document. \
                 {document} currently names: {documented:?}"
            );
        }
    }
}

/// The reverse direction: a module in `MODULES` must be named by **each**
/// document.
///
/// `MODULES` is what the analyzer will accept before a `.`, so every name in it
/// is a spelling a program may use. One that neither SPEC.md nor README.md
/// mentions is undocumented surface: it works, nobody knows it exists, and a
/// reader who finds it in the source has no way to know it is not a typo. One
/// that only *one* of them mentions is half of that — the specification is the
/// document that is read as the language, so a module it does not name is a
/// module a reader of the specification cannot learn about.
#[test]
fn edge_a_module_in_the_registry_is_named_by_a_document() {
    let documents = names_each_document_gives();
    for name in stdlib::MODULES {
        for (document, documented) in &documents {
            assert!(
                documented.contains(*name),
                "stdlib::MODULES lists `{name}` and {document} does not name it. \
                 Either document the module or drop the name from MODULES — a \
                 namespace a document does not explain is surface nobody agreed to. \
                 {document} currently names: {documented:?}"
            );
        }
    }
}

/// A module member that has no function behind it is a spelling the analyzer
/// accepts and the runtime then refuses, which is worse than either error alone:
/// the program passes analysis and fails at the first call.
///
/// **Per member, not per module.** The check this replaced asked whether *any*
/// builtin started with `<module>_`, and one stub answers that for the whole
/// module: `list_map` made `list` look complete while `list.filter` and
/// `list.reduce` reached builtins that are registered and refuse every call.
/// Deleting `filter` from the registry entirely would have left the old gate
/// green. Every member is now named in `stdlib::MODULE_MEMBERS` and checked on
/// its own, so a member that resolves to nothing is one line rather than a whole
/// namespace nobody looked at.
#[test]
fn edge_no_module_in_the_registry_has_nothing_behind_it() {
    assert_eq!(
        stdlib::members_without_functions(),
        Vec::<String>::new(),
        "a module member in stdlib::MODULE_MEMBERS reaches no builtin, so calling it \
         passes the analyzer and is refused at run time. Register it, or drop it from \
         MODULE_MEMBERS — which is also what MODULES and ALIAS_MODULES are reconciled \
         against.",
    );

    assert_eq!(
        stdlib::modules_without_functions(),
        Vec::<&str>::new(),
        "a module in stdlib::MODULES has no function behind it at all. Add the \
         function, or list the module in stdlib::ALIAS_MODULES if its functions are \
         flat builtins.",
    );
}

/// `MODULE_MEMBERS` is the surface the check above walks, so it has to *be* the
/// surface: every module in `MODULES` listed once, and nothing listed that is not
/// a module. A module missing from the table would be skipped rather than checked,
/// which is the same skip-list shape the per-module version had.
#[test]
fn edge_the_member_table_covers_exactly_the_modules_that_are_registered() {
    let listed: Vec<&str> = stdlib::MODULE_MEMBERS
        .iter()
        .map(|(module, _)| *module)
        .collect();
    let mut sorted = listed.clone();
    sorted.sort_unstable();
    sorted.dedup();

    assert_eq!(
        sorted.len(),
        listed.len(),
        "stdlib::MODULE_MEMBERS lists a module twice: {listed:?}"
    );
    assert_eq!(
        sorted,
        stdlib::MODULES,
        "stdlib::MODULE_MEMBERS and stdlib::MODULES are not the same list, so the \
         per-member check walks a surface that is not the registered one. \
         MODULES: {:?} — MODULE_MEMBERS: {listed:?}",
        stdlib::MODULES
    );

    for (module, members) in stdlib::MODULE_MEMBERS {
        assert!(
            !members.is_empty(),
            "stdlib::MODULE_MEMBERS gives `{module}` no members, so nothing is \
             checked for it"
        );
    }
}

/// The forward direction at member level: a `module.member` the documents write
/// must resolve to a builtin.
///
/// `edge_a_module_the_documents_name_is_in_the_registry` answers "is the receiver
/// a module", which `list.map` passes whether or not `list.filter` exists. This
/// asks about the *pair*, so a documented member with nothing behind it fails
/// here rather than in a reader's program — where it would be an analyzer-level
/// `Unknown function` naming a spelling SPEC.md printed.
#[test]
fn edge_a_member_the_documents_name_is_a_function_the_registry_has() {
    for (document, section) in standard_library_sections() {
        let members = documented_members(&section);
        assert!(
            !members.is_empty(),
            "{document} § Standard Library writes no `module.member` call, so this \
             gate would pass on a document that stopped documenting any function."
        );

        for member in &members {
            let (module, name) = member
                .split_once('.')
                .unwrap_or_else(|| panic!("{member} is not a module.member pair"));
            assert!(
                stdlib::is_module(module),
                "{document} writes `{member}` and `{module}` is not in \
                 stdlib::MODULES, so the receiver is an Unknown variable. \
                 {document} writes: {members:?}"
            );
            assert!(
                stdlib::member_resolves(module, name),
                "{document} writes `{member}` and no builtin answers it, so a program \
                 copying the line is refused at run time. Register `{module}_{name}` \
                 (or the flat `{name}`, for an ALIAS_MODULES entry), or drop the line \
                 from {document}. {document} writes: {members:?}"
            );
            assert!(
                stdlib::MODULE_MEMBERS
                    .iter()
                    .any(|(m, ms)| *m == module && ms.contains(&name)),
                "{document} writes `{member}` and stdlib::MODULE_MEMBERS does not list \
                 it, so the per-member check above never looks at it. Add it there. \
                 {document} writes: {members:?}"
            );
        }
    }
}

// ============================================================ each name answers

/// The body the loopback server in `every_module_the_documents_name_answers_a_call`
/// answers, and the answer that program is pinned against.
const NETWORK_BODY: &str = r#"{"ok":true}"#;

/// One HTTP request as the loopback server saw it.
///
/// Recorded rather than thrown away, so the test can assert what was *asked for*
/// as well as what came back. A `network.get` that issued a POST, or that asked
/// for a path it was never given, would answer the right body and still be
/// wrong, and only the request line tells the two apart.
#[derive(Clone, Debug)]
struct Exchange {
    request_line: String,
    body: String,
}

/// The method and path of a recorded request, ignoring the HTTP version.
///
/// The version is not the claim; the method and the path are.
fn asked(exchange: &Exchange) -> Vec<String> {
    exchange
        .request_line
        .split_whitespace()
        .take(2)
        .map(str::to_string)
        .collect()
}

/// A loopback HTTP server that answers `body` to `requests` requests, and the
/// thread answering them.
///
/// `network` is the one documented module whose answer is not a function of its
/// arguments, and no test may reach the network. So the test *is* the network:
/// a listener on `127.0.0.1:0` — an ephemeral port on the loopback interface,
/// which no packet leaves the machine on — answering a fixed body and recording
/// what it was asked. The answer is pinned rather than skipped.
///
/// `accept` is non-blocking with a deadline rather than blocking, so the thread
/// ends even if the program never arrives: a blocking `accept` on a connection
/// that is never made is a test that hangs, which is a worse failure than a test
/// that fails.
fn loopback_http(body: &str, requests: usize) -> (String, std::thread::JoinHandle<Vec<Exchange>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    listener
        .set_nonblocking(true)
        .expect("the listener accepts without blocking");
    let address = format!(
        "http://{}",
        listener.local_addr().expect("the loopback address")
    );
    let body = body.to_string();

    let handle = std::thread::spawn(move || {
        let mut seen: Vec<Exchange> = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while seen.len() < requests && std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                        .expect("a read deadline");
                    seen.push(answer_once(stream, &body));
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(error) => panic!("the loopback listener failed: {error}"),
            }
        }
        seen
    });

    (address, handle)
}

/// Reads one HTTP request off `stream`, answers it with `body`, and reports what
/// arrived.
///
/// The head is read to the blank line and the body to `Content-Length`, because
/// a client that keeps the connection open never sends an EOF — reading to the
/// end of the stream would wait for a hang-up that only this side can cause.
fn answer_once(mut stream: std::net::TcpStream, body: &str) -> Exchange {
    use std::io::{Read, Write};

    let mut raw: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 1024];
    let head_end = loop {
        if let Some(at) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
            break at + 4;
        }
        let read = stream
            .read(&mut chunk)
            .expect("the request head can be read");
        if read == 0 {
            break raw.len();
        }
        raw.extend_from_slice(&chunk[..read]);
    };

    let head = String::from_utf8_lossy(&raw).into_owned();
    let length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.trim().eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);

    let mut payload = raw[head_end.min(raw.len())..].to_vec();
    while payload.len() < length {
        let read = stream
            .read(&mut chunk)
            .expect("the request body can be read");
        if read == 0 {
            break;
        }
        payload.extend_from_slice(&chunk[..read]);
    }
    payload.truncate(length);

    let response_head = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: text/plain\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(response_head.as_bytes())
        .expect("the response head can be written");
    stream
        .write_all(body.as_bytes())
        .expect("the response body can be written");
    stream.flush().expect("the response can be flushed");
    let _ = stream.shutdown(std::net::Shutdown::Write);

    Exchange {
        request_line: head.lines().next().unwrap_or_default().to_string(),
        body: String::from_utf8_lossy(&payload).into_owned(),
    }
}

/// One program per documented module, each of which must exit 0 **and answer
/// exactly what it is asserted to answer**.
///
/// The point is not that the programs run — it is that the namespace is
/// *callable*: the analyzer accepts the receiver and the runtime has a function
/// to give. A module whose every call fails is a module that is in `MODULES` and
/// in the documents and still does not work.
///
/// Every program is checked against the output it is supposed to produce, because
/// "exits 0" is not a claim a module can fail. `files.exists` is the case in
/// point: it answers `yes` or `no`, both of them a successful run, so a program
/// that checked a path which was never there passed on the absence of the file it
/// was looking for. `network` was the case in point for the other reason: it was
/// written `set answer to might fail network.get(...)` with *no* expected output,
/// so `might fail` discarded any `RuntimeError` and the missing comparison
/// discarded the rest — a `network.get` that refused every request, or answered
/// nothing, exited 0 and passed. Every row now carries a `String`, there is no
/// "no expected output" case to fall back on, and the `network` row is answered
/// by a loopback server this test owns.
///
/// Every path a program writes is absolute and interpolated, because a test's
/// working directory is not a promise: `rb run` resolves a relative path against
/// the *process's* directory, which is the manifest directory only because
/// `cargo test` happens to run there.
///
/// The table is checked against each document in both directions, so a module
/// added to a document without a line here fails rather than going untested.
#[test]
fn every_module_the_documents_name_answers_a_call() {
    // `files` is given an absolute path to a file this test writes first,
    // because a test's working directory is not a promise. The name is this
    // test's own: the scratch directory is shared with every other program in
    // this file, so a path another test happens to write is not a promise
    // either. The second line asks about a file nothing writes, because a
    // `files.exists` that answered `yes` for every path would pass a test that
    // only ever asked about one.
    let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/p50names");
    std::fs::create_dir_all(&scratch).expect("scratch directory");
    std::fs::write(scratch.join("exists_probe.rb"), "say 1\n").expect("the file is written");
    let scratch = scratch.display().to_string();

    // The server that answers the `network` program. Started before the table so
    // its address can be interpolated into it, and joined after every program
    // has run, so the requests the table actually made are on record.
    let (address, server) = loopback_http(NETWORK_BODY, 2);

    let calls: Vec<(&str, String, String)> = vec![
        (
            "bytes",
            r#"say length(bytes.from_text("hi"))"#.to_string(),
            "2\n".to_string(),
        ),
        // `console.log` writes its argument and answers `nothing`, which `say`
        // then prints. Both lines are in SPEC.md § console.
        (
            "console",
            r#"say console.log("hi")"#.to_string(),
            "hi\nnothing\n".to_string(),
        ),
        (
            "csv",
            r#"say csv.parse("a,b")"#.to_string(),
            "[[a, b]]\n".to_string(),
        ),
        (
            "files",
            format!(
                r#"say files.exists("{scratch}/exists_probe.rb")
say files.exists("{scratch}/no_such_file_here.rb")"#
            ),
            "yes\nno\n".to_string(),
        ),
        (
            "json",
            r#"say json.parse("{}")"#.to_string(),
            "{}\n".to_string(),
        ),
        (
            "list",
            r#"say list.map([1], to (x) give back x)"#.to_string(),
            "[1]\n".to_string(),
        ),
        ("math", r#"say math.sqrt(9)"#.to_string(), "3\n".to_string()),
        // Both spellings, both answered by the loopback server, neither guarded
        // by `might fail`. This row used to be `set answer to might fail
        // network.get("http://127.0.0.1:1/")` with no expected output at all, so
        // the failure was discarded by the program and the missing comparison
        // discarded it again — `network.get` could refuse every request and the
        // test would pass. Nothing here leaves the machine: `address` is
        // `127.0.0.1` on a port this test was given by the kernel.
        (
            "network",
            format!(
                r#"say network.get("{address}/greeting")
say network.post("{address}/echo", "ping")"#
            ),
            format!("{NETWORK_BODY}\n{NETWORK_BODY}\n"),
        ),
        (
            "sys",
            r#"say length(sys.argv())"#.to_string(),
            "0\n".to_string(),
        ),
        (
            "text",
            r#"say text.uppercase("hi")"#.to_string(),
            "HI\n".to_string(),
        ),
        (
            "time",
            r#"say time.format(0, "%Y")"#.to_string(),
            "1970\n".to_string(),
        ),
    ];

    let documents = names_each_document_gives();
    for (document, documented) in &documents {
        for name in documented {
            assert!(
                calls.iter().any(|(module, _, _)| module == name),
                "{document} names the module `{name}` and no program in \
                 every_module_the_documents_name_answers_a_call calls it. \
                 Add a line for `{name}` there. {document} names: {documented:?}"
            );
        }
    }
    for (name, _, _) in &calls {
        for (document, documented) in &documents {
            assert!(
                documented.contains(*name),
                "every_module_the_documents_name_answers_a_call exercises `{name}` and \
                 {document} does not name it, so the test would pass on a module the \
                 reader of that document cannot find."
            );
        }
    }

    for (name, source, expected) in &calls {
        assert!(
            !expected.is_empty(),
            "the row for `{name}` asserts no output, so a module whose answer \
             nothing compares still passes. Every row carries the output it must \
             print; there is no 'no expected output' case left to fall back on."
        );
        let (ok, stdout, stderr) = run_named(name, source);
        assert!(
            ok,
            "the documented module `{name}` does not answer a call:\n{source}\n\
             --- stderr\n{stderr}\nEither the module is not in stdlib::MODULES, or \
             it has no function behind it."
        );
        assert_eq!(
            stdout, *expected,
            "the documented module `{name}` ran but answered something else:\n{source}\n\
             --- stdout\n{stdout}--- stderr\n{stderr}"
        );
    }

    // What the `network` program asked the loopback server for, not only what it
    // got back. A `network.get` that issued a POST, or asked for a path it was
    // never given, would answer the pinned body and still be wrong.
    let exchanges = server.join().expect("the loopback server thread");
    assert_eq!(
        exchanges.len(),
        2,
        "the network program reached the loopback server {} times and not twice, \
         so one of network.get / network.post never sent a request:\n{exchanges:?}",
        exchanges.len()
    );
    assert_eq!(
        asked(&exchanges[0]),
        ["GET", "/greeting"],
        "network.get did not issue a GET for the path it was given:\n{:?}",
        exchanges[0]
    );
    assert_eq!(
        asked(&exchanges[1]),
        ["POST", "/echo"],
        "network.post did not issue a POST for the path it was given:\n{:?}",
        exchanges[1]
    );
    assert_eq!(
        exchanges[1].body, "ping",
        "network.post did not send the body it was given"
    );
}

// ============================================================ the calls fail clean

/// Every line SPEC.md's `### console` section writes, run, and its answer read.
///
/// The coverage in `every_module_the_documents_name_answers_a_call` used to be
/// `console.log("hi")`, a spelling neither document named, while the section
/// itself wrote `say`, `print` and `ask`. So the gate was green on a line no
/// reader could look up, and `ask "Your name?"` — the one documented line that
/// did not work — was never run. This runs each line the section now carries,
/// which is the only way "the documented examples run" is a claim with teeth: a
/// line added to the section that is refused fails here rather than in a
/// reader's program.
#[test]
fn edge_the_documented_console_lines_answer() {
    // First: the section's own lines, read out of SPEC.md and run as written.
    // Every one of them is a complete statement, so nothing here is a
    // transcription of them.
    let section = code_block_under(&read_doc("SPEC.md"), "### console");
    for line in &section {
        let source = strip_comment(line).trim().to_string();
        if source.is_empty() {
            continue;
        }
        let (ok, _, stderr) = run_named("console_line", &source);
        assert!(
            ok,
            "a line SPEC.md § console writes does not run:\n{source}\n\
             --- stderr\n{stderr}\nEither the line is not a name in the language, \
             or it is one the language does not answer."
        );
    }

    // Then what each of them is *for*: the newline, and the stream.
    // `say` prints a newline; `print` does not. The difference is the whole
    // reason both are keywords, so it is asserted rather than eyeballed.
    let (ok, stdout, stderr) = run_named("console_say", r#"say "Hello""#);
    assert!(ok, "say was refused:\n--- stderr\n{stderr}");
    assert_eq!(stdout, "Hello\n", "say does not print a newline");

    let (ok, stdout, stderr) = run_named("console_print", r#"print "Hello""#);
    assert!(ok, "print was refused:\n--- stderr\n{stderr}");
    assert_eq!(
        stdout, "Hello",
        "print printed a newline, which is the only difference from say"
    );

    // `console.log` is `say` under its module spelling and answers `nothing`.
    let (ok, stdout, stderr) = run_named("console_log", r#"console.log("Hello")"#);
    assert!(ok, "console.log was refused:\n--- stderr\n{stderr}");
    assert_eq!(stdout, "Hello\n", "console.log does not print its argument");

    let (ok, stdout, stderr) = run_named("console_log_value", r#"say console.log("Hello")"#);
    assert!(ok, "console.log was refused:\n--- stderr\n{stderr}");
    assert_eq!(
        stdout, "Hello\nnothing\n",
        "console.log does not answer nothing, so `say` it is not the same as `say`"
    );

    // `console.error` writes to stderr, so a program that collects its output
    // keeps its diagnostics out of it.
    let (ok, stdout, stderr) = run_named(
        "console_error",
        r#"console.error("Could not read")
say "Hello""#,
    );
    assert!(ok, "console.error was refused:\n--- stderr\n{stderr}");
    assert_eq!(stdout, "Hello\n", "console.error wrote to stdout");
    assert_eq!(stderr, "Could not read\n", "console.error wrote nothing");

    // `console.clear` writes the terminal's clear sequence. It is written to a
    // stream that is not a terminal here, so the sequence is still what comes
    // out — which is the only part of the line a test can check.
    let (ok, stdout, stderr) = run_named("console_clear", r#"console.clear()"#);
    assert!(ok, "console.clear was refused:\n--- stderr\n{stderr}");
    assert_eq!(
        stdout, "\u{1b}[2J\u{1b}[1H",
        "console.clear did not write the clear sequence"
    );

    // `ask` is the line this section used to carry, and it is still not a name
    // in the language: it is a freed keyword, so `set ask to 1` is a program and
    // `ask "n"` is `Unknown variable 'ask'`. The section says so, and this holds
    // it to it in all three directions — both spellings refused, and the
    // identifier honoured.
    for source in [
        // No parentheses: not a call at all, and the analyzer refuses the name.
        r#"ask "Your name?""#,
        // Parentheses: a call, and `runtime::builtin` used to answer it by
        // reading stdin. This is the spelling the section's own past is written
        // in, and it was the one that worked — `wasm/check-examples.js` has
        // always exercised `input` in this shape.
        r#"ask("Your name?")"#,
    ] {
        let (ok, stdout, stderr) = run_named("console_ask", source);
        assert!(
            !ok,
            "ask is documented nowhere and nothing implements it, so a program that \
             writes it must not run:\n{source}\n--- stdout\n{stdout}"
        );
        assert!(
            stderr.contains("ask"),
            "the refusal does not name the word the program wrote:\n{source}\n\
             --- stderr\n{stderr}"
        );
        assert_eq!(
            stdout, "",
            "the refused program wrote a prompt before refusing:\n{source}\n{stdout}"
        );
    }

    let (ok, stdout, stderr) = run_named("console_ask_name", "set ask to 1\nsay ask");
    assert!(
        ok,
        "`ask` is not an ordinary identifier any more:\n--- stderr\n{stderr}\n\
         If it became a function, tests/keyword_reservation_test.rs's \
         `the_freed_words_lex_as_identifiers_and_run_as_names` is wrong too."
    );
    assert_eq!(stdout, "1\n", "`ask` no longer names what it was set to");
}

/// `input` reads a line and answers it, and the line it answers is the line that
/// was typed.
///
/// The registration is the first claim, because it is the one that was false.
/// `runtime::builtin` has answered `input` since phase-028 — and answered it
/// *reachably*, since a callee written with parentheses is resolved at run time
/// and never asked of the analyzer, so `input("n")` read stdin on `main` while
/// `stdlib::builtins()` did not hold the name. SPEC.md said the language read
/// nothing from the program it is running in, and `tests/test_text.rb` says
/// nothing either. A name that answers without being registered is the same
/// shape as the one this file's own header describes: the registry and the
/// language describing different sets.
#[test]
fn edge_input_reads_one_line_and_answers_it() {
    assert!(
        redblue::stdlib::is_builtin("input"),
        "`input` answers a call but stdlib::builtins does not register it, so the \
         registry and the language are two different sets. Register it, or take the \
         runtime arm out — either way the two must agree."
    );

    // A prompt goes out, a line comes back. The prompt is on stdout because that
    // is the stream the answer is being collected next to.
    let (ok, stdout, stderr) = run_with_stdin(
        "input_line",
        r#"set answer to input("Your name? ")
say answer"#,
        "Alice\n",
    );
    assert!(ok, "input was refused:\n--- stderr\n{stderr}");
    assert_eq!(
        stdout, "Your name? Alice\n",
        "input did not write its prompt and answer the line it read"
    );

    // The line terminator goes; the rest of the line does not. A pipe whose last
    // line has no newline is still a line, and `read_line` keeps whatever it
    // read — `input.pop()` dropped the last character whether or not there was a
    // newline to drop, so `abc` came back as `ab`. This is the case that could
    // not be written before the name was registered, which is why it is here.
    for (typed, what) in [
        ("abc", "no trailing newline"),
        ("abc\n", "one newline"),
        ("abc\r\n", "CRLF"),
    ] {
        let (ok, stdout, stderr) = run_with_stdin("input_eof", r#"say length(input())"#, typed);
        assert!(ok, "input was refused for {what}:\n--- stderr\n{stderr}");
        assert_eq!(
            stdout.trim_end(),
            "3",
            "input did not answer the three characters typed for the case of {what}"
        );
    }

    // The end of the input is an empty answer, not a refusal: a program that
    // asks past the end gets text it can test, rather than an error it has to
    // catch.
    let (ok, stdout, stderr) = run_with_stdin("input_eof_empty", r#"say length(input())"#, "");
    assert!(
        ok,
        "input refused the end of its input:\n--- stderr\n{stderr}"
    );
    assert_eq!(
        stdout.trim_end(),
        "0",
        "input did not answer the empty text at the end of its input"
    );

    // Whitespace is part of the line. A program that wants it gone trims it, and
    // one that does not gets it.
    let (ok, stdout, stderr) = run_with_stdin(
        "input_spaces",
        r#"set answer to input()
say answer"#,
        "  padded  \n",
    );
    assert!(ok, "input was refused:\n--- stderr\n{stderr}");
    assert_eq!(
        stdout, "  padded  \n",
        "input answered something other than the line that was typed"
    );
}

/// The `text` and `math` examples in SPEC.md, run.
///
/// This is the pair the finding is named for: `text.uppercase("hi")` answered
/// `Unknown variable 'text'` and `math.sqrt(9)` answered `Unknown variable
/// 'math'`, one step before the runtime could say which argument was wrong. The
/// names are in `MODULES` now, and this pins the *values* the section's own
/// comments claim, so a module that is in the registry and returns something
/// else still fails.
#[test]
fn the_text_and_math_examples_answer_what_the_spec_claims() {
    // `// "HELLO"`, `// "hello"`, `// ["a", "b", "c"]`, `// "a,b,c"` and
    // `// 4`, `// 1024`, `// 1` are what SPEC.md writes beside these lines.
    let cases = [
        (r#"say text.uppercase("hello")"#, "HELLO"),
        (r#"say text.lowercase("HELLO")"#, "hello"),
        (r#"say text.split("a,b,c", by ",")"#, "[a, b, c]"),
        (r#"say text.join(["a", "b", "c"], by ",")"#, "a,b,c"),
        (r#"say math.sqrt(9)"#, "3"),
        (r#"say math.round(3.7)"#, "4"),
        (r#"say math.pow(2, 10)"#, "1024"),
        (r#"say math.floor(1.7)"#, "1"),
        (r#"say text.trim("  x  ")"#, "x"),
        (r#"say text.replace("a-b", "-", "+")"#, "a+b"),
    ];

    for (source, expected) in cases {
        let (ok, stdout, stderr) = run_named("spec_example", source);
        assert!(
            ok,
            "a documented example did not run:\n{source}\n--- stderr\n{stderr}"
        );
        assert_eq!(
            stdout.trim_end(),
            expected,
            "a documented example answered something else:\n{source}\n--- stderr\n{stderr}"
        );
    }
}

/// `length` counts bytes, and both implementations that can answer it say so.
///
/// There were two, and they disagreed. `runtime::builtin` — the one both VMs
/// reach for, and so the one that answered — counted **bytes**;
/// `stdlib::builtin_function` counted **characters**. The second was dead code
/// no caller reached, which is why the disagreement survived a suite that
/// exercised both engines, and it would have surfaced the day `length` stopped
/// being handled early: every program that measures a string would have changed
/// answer by a factor of two to four, with no change to any of them.
///
/// Bytes is the language's contract, not the reachable arm's accident:
/// `tests/test_text.rb` names it in six tests
/// (`edge_text_length_counts_bytes_not_characters` and its neighbours, one of
/// which spells out *why* — `é` is one glyph and three bytes) and
/// `corpus/unicode-0001..0008` record it. So the fix went into the arm that
/// disagreed rather than into the six tests.
#[test]
fn edge_length_counts_bytes_in_both_arms() {
    let cases = [
        // The two spellings agree with each other, which is the claim.
        (r#"say length("héllo")"#, "6", "six bytes, five characters"),
        (
            r#"say text.length("héllo")"#,
            "6",
            "the module spelling agrees",
        ),
        (
            r#"say length(bytes.from_text("héllo"))"#,
            "6",
            "the byte list agrees",
        ),
        // One glyph, three bytes: the case `tests/test_text.rb` writes the
        // comment for. Written as an escape here because the two forms look
        // identical in an editor — `"é"` is two bytes when it is precomposed
        // U+00E9 and three when it is `e` followed by U+0301 COMBINING ACUTE
        // ACCENT, which is the one this means.
        ("say length(\"e\u{301}\")", "3", "one glyph, three bytes"),
        (r#"say length("世界")"#, "6", "six bytes, two characters"),
        (r#"say length("")"#, "0", "no bytes at all"),
        (r#"say length("a\nb")"#, "3", "an escape is one byte"),
        (
            r#"say length([1, 2, 3])"#,
            "3",
            "a list is still its own length",
        ),
    ];

    for (source, expected, what) in cases {
        let (ok, stdout, stderr) = run_named("length_case", source);
        assert!(ok, "{source} did not run:\n--- stderr\n{stderr}");
        assert_eq!(
            stdout.trim_end(),
            expected,
            "length did not answer {what} for {source}"
        );
    }
}

/// `bytes` is the module this phase added to the documents, so it is the one
/// whose edges are pinned here: the two ends of the byte range and one past
/// them, the empty text and the empty byte list, a single byte, and a character
/// that is not one.
#[test]
fn edge_the_bytes_module_answers_a_round_trip_at_both_ends() {
    // Empty in, empty out. `say ""` is a newline, so the answer is compared
    // against the empty string rather than against `""`.
    let (ok, stdout, stderr) = run_named("bytes_empty", r#"say bytes.text(bytes.from_text(""))"#);
    assert!(
        ok,
        "the empty round trip did not run:\n--- stderr\n{stderr}"
    );
    assert_eq!(
        stdout, "\n",
        "the empty round trip did not answer empty text"
    );

    // A single byte, a character that is not one, and the low end of the byte
    // range in text that is still text.
    let cases = [
        (r#"say bytes.from_text("H")"#, "[72]"),
        (r#"say bytes.from_text("é")"#, "[195, 169]"),
        (r#"say bytes.text([0, 65])"#, "\u{0}A"),
    ];
    for (source, expected) in cases {
        let (ok, stdout, stderr) = run_named("bytes_case", source);
        assert!(ok, "{source} did not run:\n--- stderr\n{stderr}");
        assert_eq!(
            stdout.trim_end_matches('\n'),
            expected,
            "wrong answer for {source}"
        );
    }

    // Both ends of the byte range are bytes, and `bytes.write` puts exactly
    // them in the file — checked by reading it back, so a write that wrapped
    // 255 into 0 would fail rather than pass on `nothing`.
    //
    // The path is absolute and the *same* absolute path goes into the program.
    // `rb run` resolves a relative path against the process's working
    // directory, which is the manifest directory only because `cargo test`
    // happens to start there; a relative path in a program is a promise about
    // the runner that nothing in this file makes.
    let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/p50names");
    std::fs::create_dir_all(&scratch).expect("scratch directory");
    let path = scratch.join("ends.bin");
    let (ok, _, stderr) = run_named(
        "bytes_ends",
        &format!(r#"bytes.write("{}", [0, 255])"#, path.display()),
    );
    assert!(ok, "bytes 0 and 255 were refused:\n--- stderr\n{stderr}");
    assert_eq!(
        std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display())),
        vec![0u8, 255],
        "bytes.write did not write the two bytes it was given"
    );

    // Past either end there is no byte, and the refusal names the value rather
    // than wrapping it into the file. `[0, 255]` is in range but is not text, so
    // it is refused too — by the other rule, and still by name. The path is
    // absolute here for the same reason as the write above.
    let out_of_range = scratch.join("out.bin");
    for source in [
        r#"say bytes.text([256])"#.to_string(),
        r#"say bytes.text([-1])"#.to_string(),
        r#"say bytes.text([0, 255])"#.to_string(),
        format!(r#"say bytes.write("{}", [256])"#, out_of_range.display()),
        r#"say bytes.from_text(1)"#.to_string(),
    ] {
        let (ok, _, stderr) = run_named("bytes_range", &source);
        assert!(!ok, "a value that is not a byte was accepted:\n{source}");
        assert!(
            stderr.contains("bytes"),
            "the refusal does not name the module the program wrote:\n{source}\n{stderr}"
        );
    }
}

/// A documented module function called wrongly is a `RuntimeError` naming the
/// function, never a panic and never a silent answer.
///
/// Each case is a different way of being wrong: no argument at all, one
/// argument too many, and an argument of the wrong type. `list.map` is the one
/// used for "too many" because it checks the arguments it needs rather than
/// taking the first `n` — which is why the other two are pinned on names that
/// do check.
#[test]
fn edge_a_module_function_refuses_bad_arguments_by_name() {
    let cases = [
        // No arguments.
        (r#"say text.uppercase()"#, "uppercase"),
        (r#"say files.read()"#, "files.read"),
        (r#"say json.parse()"#, "json.parse"),
        // Too many arguments.
        (r#"say list.map([1], to (x) give back x, 5)"#, "map"),
        // Wrong type: a number where text is required, and a text where a
        // number is required.
        (r#"say files.read(1)"#, "files.read"),
        (r#"say math.sqrt("a")"#, "sqrt"),
        (r#"say text.split("a", 1)"#, "split"),
    ];

    for (source, needle) in cases {
        let (ok, _, stderr) = run_named("bad_args", source);
        assert!(
            !ok,
            "the program was accepted but should have been refused:\n{source}"
        );
        assert!(
            stderr.contains(needle),
            "the refusal does not name '{needle}', so it cannot say which call was \
             wrong:\n{source}\n--- stderr\n{stderr}"
        );
        assert!(
            !stderr.contains("panicked"),
            "a bad argument to a module function panicked instead of being refused:\n\
             {source}\n--- stderr\n{stderr}"
        );
    }
}

/// The same bad calls again, this time where the program can see them.
///
/// The test above proves the call is *refused*; it does not prove the refusal is
/// something a program can carry on from. Those are different properties, and a
/// run that ends the process is the worse one — `list.map` on a list of the wrong
/// shape should be a case a caller can write `try ... catch error` around, not a
/// case that kills everything the caller had already done.
///
/// `catch error` binds nothing, so the message cannot be read back here; the test
/// above is where the *name* in the message is pinned. What is asserted here is
/// that the handler runs and the statement after the `try` runs too — `after` is
/// on stdout only if the failure was caught rather than fatal, so a program that
/// aborts at the call fails this even though it "printed" something.
///
/// The unknown-module case is deliberately absent from this list: an
/// `AnalyzerError` is not a `RuntimeError`, so `try` does not catch one, and a
/// `catch error` around `nosuchmodule.read("x")` leaves the program dead.
/// `edge_an_unknown_module_name_is_refused_in_its_dotted_spelling` is where that
/// name is pinned, and this test is what says why the two are not the same row.
#[test]
fn edge_a_bad_module_call_is_a_catchable_error_and_not_an_abort() {
    let cases = [
        // No arguments, too many, and a wrong type — the same three ways of
        // being wrong as the test above, one per kind.
        r#"say text.uppercase()"#,
        r#"say list.map([1], to (x) give back x, 5)"#,
        r#"say files.read(1)"#,
    ];

    for source in cases {
        let program =
            format!("try\n    {source}\ncatch error\n    say \"caught\"\nend\nsay \"after\"\n");
        let (ok, stdout, stderr) = run_named("caught_module_error", &program);
        assert!(
            ok,
            "a bad call to a module function ended the program instead of being \
             catchable:\n{source}\n--- stderr\n{stderr}"
        );
        assert_eq!(
            stdout, "caught\nafter\n",
            "the failure did not reach the program's own `catch error`, or the \
             statement after the `try` did not run, so the program could not carry \
             on:\n{source}\n--- stdout\n{stdout}\n--- stderr\n{stderr}"
        );
    }

    // The boundary the row above sits on: an error the analyzer raises is not
    // catchable, and pretending otherwise would be the wrong thing to promise.
    // `try` leaves the program dead here, so the process exits non-zero and
    // `caught` is never printed — which is what makes the three rows above a
    // statement about `RuntimeError` rather than about errors in general.
    let (ok, stdout, _) = run_named(
        "uncaught_analyzer_error",
        r#"try
    say nosuchmodule.read("x")
catch error
    say "caught"
end
say "after"
"#,
    );
    assert!(
        !ok,
        "an AnalyzerError was caught by `catch error`, so the rows above prove \
         nothing about RuntimeError being catchable:\n--- stdout\n{stdout}"
    );
    assert!(
        !stdout.contains("after"),
        "the program carried on past an error `catch error` did not handle:\n\
         --- stdout\n{stdout}"
    );
}

/// A bare receiver that is not a module is refused in the spelling the program
/// wrote.
///
/// The internal name is `module_function`, so an error built from the qualified
/// name alone leaks `nosuchmodule_read` and tells the reader nothing about which
/// word in their program was wrong.
#[test]
fn edge_an_unknown_module_name_is_refused_in_its_dotted_spelling() {
    let (ok, _, stderr) = run_named("unknown_module", r#"say nosuchmodule.read("x")"#);
    assert!(
        !ok,
        "a module that does not exist was accepted: nosuchmodule.read(\"x\")"
    );
    assert!(
        stderr.contains("nosuchmodule"),
        "the refusal does not name the module the program wrote:\n--- stderr\n{stderr}"
    );
    assert!(
        !stderr.contains("nosuchmodule_"),
        "the refusal leaks the internal encoding instead of the dotted spelling:\n{stderr}"
    );
}
