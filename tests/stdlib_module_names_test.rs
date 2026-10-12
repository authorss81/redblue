//! SPEC.md, README.md and `src/stdlib.rs` must name the same modules — and the
//! same members.
//!
//! `stdlib::MODULES` is the list the analyzer consults before a `.`: a bare
//! word that is in it is a namespace, and a bare word that is not is
//! `Unknown variable`. `stdlib::MODULE_MEMBERS` is the list both engines resolve
//! a dotted call against, so a `module.member` the table does not hold is
//! `RuntimeError: Module '<m>' has no function '<x>'` whatever the registry
//! holds. The two documents and those two tables are one surface, and a name
//! that appears in only one of them is a defect somebody reads:
//!
//! - a name a document names and the table does not hold is a program that dies
//!   in the analyzer or at the first call, one step before the runtime could say
//!   what was wrong;
//! - a name the table holds and no document names is a namespace that exists and
//!   nothing explains, which is how `bytes` and `sys` came to be callable
//!   spellings of working functions that SPEC.md never mentioned — and how
//!   `text.length` and `list.length` came to answer while both documents wrote
//!   `length` flat.
//!
//! The lists are *extracted* from the documents rather than transcribed, and each
//! document is reconciled **on its own** rather than through a union of the two.
//! A union is weaker than either document: a name only one of them gives still
//! passes, so SPEC.md could drop `time` — which it had — or README.md could drop
//! `console` without a test noticing, and this phase exists because a document
//! and the registry drifted apart.
//!
//! The one union is [`promised_by_the_documents`], which is a different question:
//! "may this spelling answer at all?", rather than "can a reader of this document
//! find it?". A spelling answers only if the table holds it, and the table is
//! reconciled against each document on its own, so the union is bounded by both.

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

/// The whole of the `### ` section `heading` opens in `doc`, prose included.
///
/// [`code_block_under`] is the runnable part; this is the part a reader reads,
/// and a claim written only in prose is still a claim — so a test that says
/// "the document does not promise X" needs the prose, not just the examples.
fn section_under(doc: &str, heading: &str) -> String {
    let mut section = String::new();
    let mut under = false;
    for line in doc.lines() {
        if line.starts_with("### ") {
            if under {
                break;
            }
            under = line.trim_end() == heading;
            continue;
        }
        if under {
            section.push_str(line);
            section.push('\n');
        }
    }
    section
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
/// 2. every word of every `### ` heading in the section, folded onto the module
///    it names by [`module_spelling`] — which is why the headings have to name
///    modules rather than topics.
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
                // the word joining them rather than one of them. Folded
                // case-insensitively, because [`module_spelling`] folds a
                // heading case-insensitively too: a heading that capitalised its
                // joiner would otherwise insert a module called `AND` and fail
                // `edge_a_module_the_documents_name_is_in_the_registry` with
                // `AND not in MODULES` — a failure about prose capitalisation,
                // read as a failure about the language.
                if !word.is_empty() && !word.eq_ignore_ascii_case("and") {
                    names.insert(module_spelling(word));
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

/// The spelling of the module a heading word names, or the word as written.
///
/// A heading is prose, and prose capitalises: README.md writes `### Files`,
/// `### JSON and CSV` and `### Time` where SPEC.md writes `### files`. So a
/// heading word that names a module case-insensitively is folded onto the
/// module's own spelling, and a word that names none is left exactly as written —
/// where it stays a word and never becomes a spelling a program is told to use.
///
/// The fold is for headings and nothing else. A dotted receiver is kept as the
/// program writes it, because that is how the language reads it: `stdlib::is_module`
/// compares exactly, so a document carrying `JSON.Parse(..)` would send a reader
/// to `Unknown variable 'JSON'`.
fn module_spelling(word: &str) -> String {
    stdlib::MODULES
        .iter()
        .copied()
        .find(|module| module.eq_ignore_ascii_case(word))
        .unwrap_or(word)
        .to_string()
}

/// The `module.member` spellings the code blocks of `section` write, as
/// `("module", "member")` pairs.
///
/// Only code blocks, for the reason [`documented_modules`] gives: prose carries
/// names that are *not* calls — `` `math.PI` `` is the one SPEC.md uses to say a
/// module has no members — and a gate that read prose would either fail on a
/// sentence or, worse, be taught to skip it.
///
/// Code blocks only is also what makes this set usable as the **independent**
/// half of a gate. A pair listed here is a spelling a document promises, and a
/// promise is written as an example; a sentence in prose can promise a name
/// *and* deny it in the same breath (`there is no console.input`), which is
/// exactly the pair a refusal gate must go on asking about.
fn documented_member_pairs(section: &str) -> BTreeSet<(String, String)> {
    let mut pairs = BTreeSet::new();
    let mut in_code = false;

    for line in section.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            pairs.extend(dotted_calls(line));
        }
    }

    pairs
}

/// The same pairs, written the way a reader sees them: `text.uppercase`.
fn documented_members(section: &str) -> BTreeSet<String> {
    documented_member_pairs(section)
        .into_iter()
        .map(|(module, member)| format!("{module}.{member}"))
        .collect()
}

/// Every `module.member` spelling **either** document writes, which is the whole
/// of the dotted surface the language is allowed to answer.
///
/// The union, and deliberately so, unlike
/// [`edge_a_module_in_the_registry_is_named_by_a_document`] which is checked on
/// each document alone. The two questions are different: "is this module named?"
/// is about what a reader of *this* document can find, and a quick reference that
/// drops one is a document that has drifted; "may this spelling answer?" is about
/// the language, and the answer cannot be "yes, because SPEC.md said so, as long
/// as README.md did not say otherwise".
fn promised_by_the_documents() -> BTreeSet<(String, String)> {
    standard_library_sections()
        .into_iter()
        .flat_map(|(_, section)| documented_member_pairs(&section))
        .collect()
}

/// Every short name a program could ask a module for, from either direction.
///
/// Two sources, and the second is the one this used to be missing:
///
/// 1. every member [`stdlib::MODULE_MEMBERS`] names, and
/// 2. every short name behind a registered builtin — the bare ones (`uppercase`,
///    `random`, `input`, `expect`) and the suffix of a qualified one
///    (`files_read` is `read` as far as a program writing `files.read` is
///    concerned).
///
/// The old version walked [`stdlib::BUILTIN_NAMES`] directly, which holds
/// *qualified* names — `files_read`, `json_parse`, `network_get` — so it asked
/// about `files.files_read` (a spelling no reader writes) and never asked about
/// `json.read`, `text.get` or `files.parse`. The pairs that have to be refused
/// are the short ones: those are the ones a program writes and a resolver
/// regression would answer.
fn short_member_names() -> BTreeSet<String> {
    let mut names = BTreeSet::new();

    for (_, members) in stdlib::MODULE_MEMBERS {
        names.extend(members.iter().map(|member| member.to_string()));
    }

    for builtin in stdlib::BUILTIN_NAMES {
        match stdlib::MODULES
            .iter()
            .find(|module| builtin.starts_with(&format!("{module}_")))
        {
            Some(module) => names.insert(builtin[module.len() + 1..].to_string()),
            None => names.insert(builtin.to_string()),
        };
    }

    names
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
/// namespace look complete while `text.uppercase` and `math.sqrt` — nothing to do
/// with `list` at all — were never named, and a per-module check cannot see
/// either of them.
///
/// Both halves are kept **as written**, because that is the only spelling the
/// language can be asked about: `stdlib::is_module` and the member table are
/// exact, so a document carrying `TEXT.UPPERCASE(..)` names something that is
/// `Unknown variable 'TEXT'` to every program. Folding the case here would have
/// let such a document pass reconciliation and fail every reader who copied from
/// it — and folding is right for a *heading*, which is prose, and wrong for a
/// call, which is a program.
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
        names.insert((word, member));
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

/// What running a program answers: exit success, what it printed, what it said.
type Ran = (bool, String, String);

/// Runs `source` under `tag`, on one engine, and answers [`Ran`].
type Run = fn(&str, &str) -> Ran;

/// Runs `source` under `tag` and returns `(exit success, stdout, stderr)`.
///
/// Stdin is closed rather than inherited. A program that calls `input` reads
/// end-of-input immediately and answers the empty text instead of blocking a test
/// suite on a terminal nobody is sitting at.
fn run_named(tag: &str, source: &str) -> Ran {
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

/// Runs `source` under `tag` through `rb compile` and then `rb vm`.
///
/// The second engine, run as the two processes the CLI runs it rather than as
/// the in-process pair `tests/common/vm.rs` compares. A document claim that is
/// true on `rb run` and false on `rb compile` + `rb vm` is false, and the only
/// way this file can see that is to ask both — `map(xs, f)` was `RuntimeError:
/// map was called with arguments it cannot use` here while the tree-walker
/// answered `[2, 4, 6]`, and a gate that only ever ran `rb run` called a
/// specification sentence true on the strength of the engine it happened to use.
///
/// A compiler that refuses the source is a failure of the same program, so it
/// is returned as one: `(false, "", <the compiler's message>)`, which is what
/// makes "the two engines answer the same thing" a claim about compilation as
/// well as about running.
fn run_bytecode_named(tag: &str, source: &str) -> Ran {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/p50names");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    let file = dir.join(format!("{tag}.rb"));
    let chunk = dir.join(format!("{tag}.rbc"));
    std::fs::write(&file, source).expect("the program is written");

    let mut compile_command = Command::new(env!("CARGO_BIN_EXE_rb"));
    compile_command
        .arg("compile")
        .arg(&file)
        .arg("-o")
        .arg(&chunk)
        .stdin(std::process::Stdio::null());
    for name in PROXY_ENV {
        compile_command.env_remove(name);
    }
    let compiled = compile_command.output().expect("rb compiles");
    if !compiled.status.success() {
        return (
            false,
            String::from_utf8_lossy(&compiled.stdout).into_owned(),
            String::from_utf8_lossy(&compiled.stderr).into_owned(),
        );
    }

    let mut command = Command::new(env!("CARGO_BIN_EXE_rb"));
    command
        .arg("vm")
        .arg(&chunk)
        .stdin(std::process::Stdio::null());
    for name in PROXY_ENV {
        command.env_remove(name);
    }
    let out = command.output().expect("rb runs the chunk");

    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The two engines a documented spelling has to answer on, and the runner each
/// one is asked through.
///
/// A pair rather than a list of engines to skip: a gate that runs a document on
/// one engine says nothing about the other, which is how `list.map`'s three
/// spellings could be specified and green while the bytecode VM refused all
/// three.
const ENGINES: &[(&str, Run)] = &[
    ("rb run", run_named),
    ("rb compile + rb vm", run_bytecode_named),
];

/// Runs `source` under `tag` with `typed` on its standard input.
fn run_with_stdin(tag: &str, source: &str, typed: &str) -> Ran {
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
/// `list.reduce` — two builtins that refuse every call — counted as members of
/// it. Deleting `filter` from the registry entirely would have left the old gate
/// green. Every member is now named in `stdlib::MODULE_MEMBERS` and checked on
/// its own, so a member that resolves to nothing is one name rather than a whole
/// namespace nobody looked at — and a name that resolves to a builtin which
/// cannot answer does not belong in the table at all, which is why `filter` and
/// `reduce` are out of the `list` row rather than being pinned as members.
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
///
/// The comparison is between the two *sets*, sorted on both sides. Order is not a
/// property either table promises — `MODULES` happens to be alphabetical and
/// `MODULE_MEMBERS` happens to follow it — so a test that compared them as
/// sequences would fail on a reorder that changed nothing about either surface,
/// and the fix a tired author reaches for is to stop comparing them at all.
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
    let mut registered = stdlib::MODULES.to_vec();
    registered.sort_unstable();
    assert_eq!(
        sorted,
        registered,
        "stdlib::MODULE_MEMBERS and stdlib::MODULES are not the same set, so the \
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
/// a module", which `list.map` passes whether or not `list.length` exists. This
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

// ============================================================ and no other member

/// A member the registry holds is one each document writes down.
///
/// The other direction, and the one whose absence let a defect through twice.
///
/// The gate above reads the documents and asks the table, so it passes whenever a
/// document promises no more than the table allows. Nothing asked the question
/// from the other side, and the question is the one a reader cares about: a
/// spelling that answers and that neither document mentions is surface nobody
/// agreed to — `length` under `text` and under `list` was exactly that. The
/// phase added both to the table and pinned `text.length` in a test, and neither
/// document said the word; the table had grown answering surface that the
/// documents deny in principle ("a module has the functions that are spelled
/// `<module>_<member>` in the registry" — and `list_length` is not a name in the
/// registry).
///
/// **Each document on its own**, for the reason the module-level gate gives. A
/// union would let README.md write `abs(x)` while SPEC.md wrote `math.abs`, and
/// both would pass: which is the drift this phase exists to stop, at one level
/// down.
#[test]
fn edge_a_member_the_registry_holds_is_written_by_a_document() {
    for (document, section) in standard_library_sections() {
        let written = documented_member_pairs(&section);

        for (module, members) in stdlib::MODULE_MEMBERS {
            for member in *members {
                let pair = (module.to_string(), member.to_string());
                assert!(
                    written.contains(&pair),
                    "stdlib::MODULE_MEMBERS holds `{module}.{member}`, so a program may \
                     write it, and {document} § Standard Library does not. It is a name \
                     that answers with nothing written down: add a line for \
                     `{module}.{member}` to the section, or drop the member from the \
                     table. Every pair {document} writes: {:?}",
                    documented_members(&section)
                );
            }
        }
    }
}

/// A flat builtin is a member of the module that lists it and of no other.
///
/// The other half of the table, and the half that was open. `stdlib::module_member_name`
/// used to unwrap `module_member` to `member` for **any** module against **any**
/// registered builtin, which made every module a namespace for every flat builtin:
/// `console.input("n")` read a line, `math.random(1, 6)` drew one and
/// `json.uppercase("hi")` answered `HI` — each of them a working call the
/// documents deny in as many words ("there is no `console.input`", "`math.random`
/// is not implemented", "a module has the functions that are spelled
/// `<module>_<member>` in the registry"). A per-member check that walks
/// `MODULE_MEMBERS` cannot see any of that, because each of those spellings is
/// exactly one the table does not list.
///
/// So this walks the other way: eleven modules against every short name a program
/// could ask one for ([`short_member_names`]), and every pair that is not a
/// spelling a document writes is refused by its dotted spelling. Both engines run
/// each one, because the two resolve a dotted call against the same table and a
/// language that answers in `rb run` and refuses under `rb vm` is a language whose
/// surface depends on how it is executed.
///
/// **The only pairs this skips are the ones the documents promise**, and that is
/// what makes the gate a gate. It used to skip whatever
/// `stdlib::member_resolves(module, builtin)` said, which is a predicate derived
/// from the very table the gate is about: adding `uppercase` to `csv`'s row made
/// `csv.uppercase` answer *and* made this matrix skip `csv.uppercase`, so the one
/// hole the table could open was the one hole the matrix could not see. Its guard
/// (`checked > BUILTIN_NAMES.len()`) did not catch that either, because a table
/// that grants everything still leaves hundreds of pairs to refuse — the guard
/// asked whether there was anything left to check, not whether the right things
/// were being checked. The documents cannot be widened the same way: writing
/// `csv.uppercase` into a code block is itself checked by
/// `edge_a_member_the_documents_name_is_a_function_the_registry_has`.
///
/// The two directions now meet in one place: a pair answers only if the table
/// holds it, a pair the table holds is written down by each document, and a pair
/// no document writes down is refused. Widening any one of the three fails a
/// gate, so there is nothing left to widen.
///
/// The whole matrix runs in-process: it is a property of name resolution, and a
/// refusal that a subprocess has to be spawned to observe is a refusal a hundred
/// programs at a time cannot afford to check.
#[test]
fn edge_a_flat_builtin_is_not_spelled_as_a_member_of_any_other_module() {
    let promised = promised_by_the_documents();
    let members = short_member_names();
    let (mut refused, mut reserved, mut promised_rows) = (0usize, 0usize, 0usize);

    for module in stdlib::MODULES {
        for member in &members {
            // A document writes this one, so the language is expected to answer
            // it: `text.uppercase` called with no argument is refused by
            // `uppercase`, not by the module rule, and asserting the module rule
            // there would be asserting something false. What is asserted instead
            // is that the table holds it — a document cannot promise a member the
            // table does not have, which is the gate above's first claim.
            if promised.contains(&(module.to_string(), member.clone())) {
                promised_rows += 1;
                assert!(
                    stdlib::module_has_member(module, member),
                    "a document writes `{module}.{member}`, so a program may write it, and \
                     stdlib::MODULE_MEMBERS does not list it. \
                     edge_a_member_the_documents_name_is_a_function_the_registry_has is \
                     the gate that says so by name."
                );
                continue;
            }

            let program = format!("say {module}.{member}()\n");
            let refusal = format!("Module '{module}' has no function '{member}'");

            let tokens = redblue::lexer::Lexer::tokenize(&program)
                .unwrap_or_else(|error| panic!("{program} should lex: {error:?}"));
            let ast = match redblue::parser::parse(tokens) {
                Ok(ast) => ast,
                // A name the language reserves cannot be written after a `.` at
                // all — `bytes.expect()` is `ParserError: Expected property name,
                // found reserved word 'expect'`. That is a stronger refusal than a
                // module declining a member it does not have, and it is shared by
                // both engines because both parse the same text, so it is asserted
                // rather than skipped.
                Err(error) => {
                    assert_eq!(
                        error.label(),
                        "ParserError",
                        "`{module}.{member}` did not parse and was not refused as a \
                         reserved word, so the matrix has met a failure it does not \
                         understand:\n{program}\n--- {error}"
                    );
                    reserved += 1;
                    continue;
                }
            };

            // The tree-walker, which is what `rb run` executes.
            let walked = redblue::run_source(&program);
            let walked = walked
                .err()
                .unwrap_or_else(|| panic!("{program} ran, so `{module}` answers `{member}`"));
            assert!(
                walked.to_string().contains(&refusal),
                "`{module}.{member}` was refused, but not by the rule that says a module \
                 has no such member:\n{program}\n--- {walked}\nThe dotted spelling has to \
                 name the module and the member it does not have, and an analyzer refusal \
                 here means the receiver is not reaching the module check at all."
            );

            // The bytecode VM, which is what `rb vm` executes.
            let chunk = redblue::bytecode::compile_program(&ast)
                .unwrap_or_else(|error| panic!("{program} should compile: {error:?}"));
            let compiled = redblue::bytecode::vm::BytecodeVm::new().run(&chunk);
            let compiled = compiled.err().unwrap_or_else(|| {
                panic!(
                    "the bytecode VM answered `{module}.{member}` where the walker refuses \
                     it, so the surface depends on the engine:\n{program}"
                )
            });
            assert!(
                compiled.to_string().contains(&refusal),
                "the two engines refuse `{module}.{member}` differently:\n{program}\n\
                 walker: {walked}\nbytecode: {compiled}"
            );
            refused += 1;
        }
    }

    // Every pair the matrix walked is in exactly one of the three columns, and the
    // counts have to add up to the size of the matrix. A `continue` that grows a
    // fourth column is how a skip hides here, and this is the assertion that
    // makes adding one a deliberate act: the total is computed from the two tables
    // the loop walked, so a skipped pair cannot be subtracted from both sides.
    assert_eq!(
        refused + reserved + promised_rows,
        stdlib::MODULES.len() * members.len(),
        "{refused} refused, {reserved} unparseable and {promised_rows} promised do not add \
         up to the {} pairs this matrix walked, so a pair was skipped without being \
         counted. {members:?}",
        stdlib::MODULES.len() * members.len(),
    );

    // …and the promises are the smaller half of the matrix. A gate whose allowlist
    // has grown to be most of what it walks is a gate that has stopped asking.
    assert!(
        refused > promised_rows,
        "only {refused} of the {} `module.member` spellings were refused, against \
         {promised_rows} the documents promise: this gate has almost nothing left to \
         check. Either stdlib::MODULE_MEMBERS is listing builtins as members of every \
         module, or the documents are promising spellings the language should not have.",
        stdlib::MODULES.len() * members.len(),
    );
}

/// The three promises the table above exists to keep, named one by one.
///
/// The matrix is the general form; these are the specific sentences, so a failure
/// says which claim in the specification broke rather than which of several
/// hundred pairs happened to break with it.
#[test]
fn edge_a_module_does_not_answer_a_function_the_documents_say_it_does_not_have() {
    // SPEC.md § console: "`input` is a registered builtin rather than a module
    // function, so there is no `console.input`". `input` is registered, so only
    // the module spelling is in question — and it is a refusal, not a silent
    // reading of an empty line.
    let (ok, stdout, stderr) = run_named("no_console_input", r#"say console.input("n")"#);
    assert!(
        !ok,
        "`console.input` answered, and SPEC.md says in as many words that there is \
         none:\n--- stdout\n{stdout}"
    );
    assert!(
        stderr.contains("console") && stderr.contains("input"),
        "the refusal does not name the module and the member:\n--- stderr\n{stderr}"
    );

    // SPEC.md § math: "`math.random` is not implemented. The random builtins are
    // spelled flat". The flat one still works, so this is about the spelling.
    let (ok, _, stderr) = run_named(
        "no_math_random",
        r#"random_seed(1)
say math.random(1, 6)"#,
    );
    assert!(
        !ok,
        "`math.random` drew a number, and SPEC.md says it is not implemented:\n{stderr}"
    );

    // SPEC.md § json and csv: a module has the functions spelled `<module>_<member>`
    // in the registry, and `uppercase` is registered flat, so `json.uppercase` is
    // a function of no module at all — not of the json one.
    let (ok, stdout, stderr) = run_named("no_json_uppercase", r#"say json.uppercase("hi")"#);
    assert!(
        !ok,
        "`json.uppercase` answered:\n--- stdout\n{stdout}\nSo every module is a \
         namespace for every flat builtin.\n--- stderr\n{stderr}"
    );

    // ...and the flat spellings are untouched by any of it, because the rule is
    // about the dotted spelling and nothing else.
    let (ok, stdout, stderr) = run_named("flat_input_still_answers", r#"say length(input())"#);
    assert!(
        ok,
        "closing the dotted spelling refused the flat one:\n--- stderr\n{stderr}"
    );
    assert_eq!(
        stdout.trim_end(),
        "0",
        "flat `input` answered something else"
    );

    let (ok, stdout, stderr) = run_named(
        "flat_random_still_answers",
        r#"random_seed(1)
say is_number(random(1, 6))"#,
    );
    assert!(
        ok,
        "closing the dotted spelling refused flat `random`:\n{stderr}"
    );
    assert_eq!(
        stdout.trim_end(),
        "yes",
        "flat `random` answered something else"
    );
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
///
/// Two halves, and the second is what the first cannot do. The table is
/// transcribed, so a line *added* to a section is invisible to it — which is how
/// `text.trim`, `text.replace` and the whole of `math`'s unwritten members were
/// able to sit in `MODULE_MEMBERS` answering calls no document gave. The second
/// half reads each section's block out of SPEC.md and runs every line in it, the
/// way `edge_the_documented_console_lines_answer` does for § console: a line
/// written into a document and not run is a line nobody checked.
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
        // The rest of the two sections' code blocks, with the answers SPEC.md
        // writes beside them.
        (r#"say text.contains("abc", "b")"#, "yes"),
        (r#"say text.starts_with("abc", "ab")"#, "yes"),
        (r#"say text.ends_with("abc", "bc")"#, "yes"),
        (r#"say text.length("héllo")"#, "6"),
        (r#"say list.length([1, 2, 3])"#, "3"),
        (r#"say math.abs(-3)"#, "3"),
        (r#"say math.ceil(1.2)"#, "2"),
        (r#"say math.sin(0)"#, "0"),
        (r#"say math.cos(0)"#, "1"),
        (r#"say math.tan(0)"#, "0"),
        (r#"say math.log(1)"#, "0"),
        (r#"say math.exp(0)"#, "1"),
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

    // Every block of these four sections, read out of SPEC.md and run as it is
    // written. A block rather than a line, because a block is what a reader
    // copies: § json and csv binds `obj` on its first line and reads it on its
    // third, so running the lines one at a time would test a program nobody
    // writes. `### files` and `### bytes` are deliberately absent: their examples
    // write to paths a test cannot promise, which is why they are covered by
    // `examples/files.rb` and `edge_the_bytes_module_answers_a_round_trip_at_both_ends`
    // rather than by running a document in a scratch directory.
    for section in ["### text", "### math", "### list", "### json and csv"] {
        let program = code_block_under(&read_doc("SPEC.md"), section)
            .iter()
            .map(|line| strip_comment(line).trim().to_string())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let (ok, stdout, stderr) = run_named("spec_block", &program);
        assert!(
            ok,
            "the code block SPEC.md {section} writes does not run:\n{program}\n\
             --- stdout\n{stdout}--- stderr\n{stderr}"
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

/// Compiles `program` for the bytecode VM, or reports the refusal that stopped it
/// from being compiled at all.
///
/// The bytecode VM runs no analyzer, so a gate that has to ask it has to do the
/// front end itself. A `ParserError` from any of the three stages is a refusal
/// *before* either engine runs — a reserved word, or a construct the bytecode
/// compiler has not reached — which is a stronger refusal than these gates claim
/// and is asserted as one by the callers rather than passed over.
fn bytecode_chunk(program: &str) -> Result<redblue::Chunk, redblue::Error> {
    let tokens = redblue::lexer::Lexer::tokenize(program)?;
    let ast = redblue::parser::parse(tokens)?;
    redblue::bytecode::compile_program(&ast)
}

/// A registered builtin spelled `<name>_<member>` does not make `<name>` a module.
///
/// The registry encodes a module function internally as `<module>_<member>`, and
/// both engines resolved *any* dotted call through that encoding: if
/// `<receiver>_<member>` was bound, the call was made. So every qualified builtin
/// was also a namespace — `random.seed(1)` answered through `random_seed` and
/// `random.shuffle([1, 2, 3])` through `random_shuffle`, and there is no `random`
/// module in the language at all: `stdlib::MODULES` does not list it and neither
/// document names it. `random` is a *builtin*, which is why the analyzer accepted
/// it as a receiver, and being a registered name was the whole of what the resolver
/// asked.
///
/// [`stdlib::MODULE_MEMBERS`] bounds the surface of a receiver that *is* a module,
/// and it cannot bound the surface of one that is not: the rule it belongs to is
/// only reached for a name in `MODULES`. The matrix above walks that universe, so
/// it never asked about a non-module receiver at all; this walks the registry's
/// own prefixes, which is a source neither the table nor the documents can skew.
///
/// The claim here is the weaker one that still catches the defect — none of them
/// answers — because most of the prefixes are refused earlier still: `ends` is
/// `Unknown variable 'ends'`, and `is` and `to` are keywords. The four `random`
/// pairs are the ones a program can actually write, and they are named below so a
/// failure says which spelling broke rather than which of fourteen.
#[test]
fn edge_a_registered_builtin_is_not_a_module_namespace() {
    let mut pairs: BTreeSet<(String, String)> = BTreeSet::new();
    for builtin in stdlib::BUILTIN_NAMES {
        if let Some((prefix, member)) = builtin.split_once('_') {
            if !stdlib::is_module(prefix) {
                pairs.insert((prefix.to_string(), member.to_string()));
            }
        }
    }
    assert!(
        !pairs.is_empty(),
        "no registered builtin splits into a `<name>_<member>` pair whose name is not a \
         module, so this gate has nothing to ask about and would pass on any tree"
    );

    let (mut refused, mut uncompiled) = (0usize, 0usize);
    for (prefix, member) in &pairs {
        let program = format!("say {prefix}.{member}(1)\n");

        // The tree-walker, which is what `rb run` executes and which analyses
        // before it runs — so a prefix that is not a name at all is refused here
        // for that reason, which is a stronger refusal than this gate claims.
        assert!(
            redblue::run_source(&program).is_err(),
            "`{prefix}.{member}` answered, so a builtin spelled `{prefix}_{member}` made \
             `{prefix}` a module. The registry encodes how a module function is stored; \
             it is not a list of what the language has.\n{program}"
        );

        // The bytecode VM, which analyses nothing, so it is the only engine where
        // a resolver hole can show up as an answer rather than as an unknown name.
        let chunk = match bytecode_chunk(&program) {
            Ok(chunk) => chunk,
            Err(error) => {
                assert_eq!(
                    error.label(),
                    "ParserError",
                    "`{prefix}.{member}` was answered by neither engine and refused by \
                     neither the analyzer nor the module rule, so this gate has met a \
                     failure it does not understand:\n{program}\n--- {error}"
                );
                uncompiled += 1;
                continue;
            }
        };
        assert!(
            redblue::bytecode::vm::BytecodeVm::new()
                .run(&chunk)
                .is_err(),
            "the bytecode VM answered `{prefix}.{member}` where the tree-walker refuses \
             it, so the surface depends on the engine — the walker reached the analyzer \
             first and this VM does not run it:\n{program}"
        );
        refused += 1;
    }

    assert_eq!(
        refused + uncompiled,
        pairs.len(),
        "{refused} refused and {uncompiled} uncompiled do not add up to the {} pairs this \
         matrix walked, so a pair was skipped without being counted. {pairs:?}",
        pairs.len(),
    );

    // The four a program can write, named — `random` is a registered builtin, so
    // the analyzer accepts it as a receiver and each of these reached a function
    // through the internal encoding rather than through a module.
    for member in ["seed", "number", "choice", "shuffle"] {
        let program = format!("say random.{member}(1)\n");
        let refusal = format!("Module 'random' has no function '{member}'");

        let walked = redblue::run_source(&program).err().unwrap_or_else(|| {
            panic!("`random.{member}` answered, and there is no `random` module")
        });
        assert!(
            walked.to_string().contains(&refusal),
            "`random.{member}` was refused, but not by the rule that says a receiver \
             which is not a module cannot be called into:\n{program}\n--- {walked}"
        );

        let chunk = bytecode_chunk(&program)
            .unwrap_or_else(|error| panic!("{program} should compile: {error:?}"));
        let compiled = redblue::bytecode::vm::BytecodeVm::new().run(&chunk);
        let compiled = compiled
            .err()
            .unwrap_or_else(|| panic!("the bytecode VM answered `random.{member}`"));
        assert!(
            compiled.to_string().contains(&refusal),
            "the two engines refuse `random.{member}` differently:\n{program}\nwalker: \
             {walked}\nbytecode: {compiled}"
        );
    }

    // …and the flat spellings are untouched, because the claim above is about the
    // dotted one. Deleting the four builtins would satisfy every row of it, so
    // this is the row that says the gate was not satisfied by removing surface.
    for (source, expected) in [
        ("say random_seed(1)", "nothing"),
        ("say type_of(1)", "number"),
        (r#"say ends_with("ab", "b")"#, "yes"),
        (r#"say starts_with("ab", "a")"#, "yes"),
    ] {
        let (ok, stdout, stderr) = run_named("flat_behind_the_prefix", source);
        assert!(ok, "{source} stopped answering:\n--- stderr\n{stderr}");
        assert_eq!(
            stdout.trim_end(),
            expected,
            "the flat spelling behind the prefix changed while the dotted one was refused, \
             so a fix that removed the builtin would have satisfied the rows above"
        );
    }
}

/// A `module … end` the program declared owns its own name.
///
/// The member table is the surface of a **stdlib** module and says nothing about a
/// module a program writes, which is why the engines keep the table's rule for a
/// receiver in `MODULES` and ask the program's own table otherwise. Asking the
/// stdlib table first — as round 6's check did — refuses a declared module whose
/// name happens to be a stdlib module's: `module text … end` publishing `greet` is
/// `Module 'text' has no function 'greet'`, and all eleven stdlib names refused the
/// same way.
///
/// A program's own declaration winning is the rule an object already follows, and
/// it is pinned in `tests/object_model_test.rs`
/// (`object_named_like_a_module_takes_the_call`): a declared `object json` takes the
/// `json.parse(..)` spelling over the module of the same name because the receiver
/// is looked up first. Every name in `MODULES` is walked here, so this is a claim
/// about all eleven rather than about `text`.
#[test]
fn edge_a_module_the_program_declares_owns_its_own_name() {
    assert!(
        !stdlib::MODULES.is_empty(),
        "stdlib::MODULES is empty, so this gate has nothing to collide with"
    );

    for module in stdlib::MODULES {
        // The last statement *is* the call rather than a `say` or a `set`, because
        // the engine's return value is the last statement's: both of those answer
        // `nothing`, and a value of `nothing` here would be the program finishing
        // rather than the function answering.
        let program = format!(
            "module {module}\n    \
             to greet(who)\n        return \"hi \" + who\n    end\n    \
             export all\nend\n\
             {module}.greet(\"ada\")\n"
        );
        let complaint = "a module the program declared does not answer its own member, \
             because the stdlib member table decided a receiver it says nothing about";

        // The tree-walker, which is what `rb run` executes.
        let walked = redblue::run_source_value(&program);
        assert_eq!(
            walked.as_ref().map(|value| value.to_string()).ok(),
            Some("hi ada".to_string()),
            "{complaint}:\n{program}\n--- {}",
            walked
                .err()
                .map(|error| error.to_string())
                .unwrap_or_default(),
        );

        // The bytecode VM, which is what `rb vm` executes.
        let chunk = bytecode_chunk(&program)
            .unwrap_or_else(|error| panic!("{program} should compile: {error:?}"));
        let compiled = redblue::bytecode::vm::BytecodeVm::new().run(&chunk);
        assert_eq!(
            compiled.as_ref().map(|value| value.to_string()).ok(),
            Some("hi ada".to_string()),
            "the bytecode VM and the tree-walker disagree about a module the program \
             declared under a stdlib module's name:\n{program}\n--- {}",
            compiled
                .err()
                .map(|error| error.to_string())
                .unwrap_or_default(),
        );

        // …and the member it does not have is still refused by the dotted name,
        // rather than reaching a builtin of the stdlib module whose name it took.
        let missing = format!("say {module}.not_a_function_of_this_module()\n");
        let refused = redblue::run_source(&missing)
            .err()
            .unwrap_or_else(|| panic!("`{module}.not_a_function_of_this_module` answered"));
        assert!(
            refused.to_string().contains(&format!(
                "Module '{module}' has no function 'not_a_function_of_this_module'"
            )),
            "the member the declared module does not have was refused by another rule, so \
             the rows above could pass on a module that answers everything:\n--- {refused}"
        );
    }
}

/// `length` answers a text and a list. It does not answer a record.
///
/// SPEC.md § text said all three, and the claim was not one a reader could settle
/// by reading: `runtime::builtin` refuses a record as `length requires a list or
/// text`, `tests/test_text.rb` pins that refusal as a caught error, and
/// `corpus/stdlib-0005` and `corpus/stdlib-0006` record it. Every spelling of the
/// name is walked here so the runtime cannot drift, and the sentence is read out of
/// the document rather than transcribed so re-claiming a record — without writing
/// the refusal down — fails instead of passing on a reader's good faith.
#[test]
fn edge_length_answers_a_text_or_a_list_and_refuses_a_record() {
    for (source, expected) in [
        (r#"say length("abc")"#, "3"),
        (r#"say text.length("abc")"#, "3"),
        ("say length([1, 2, 3])", "3"),
        ("say list.length([1, 2, 3])", "3"),
    ] {
        let (ok, stdout, stderr) = run_named("length_answers", source);
        assert!(ok, "{source} did not run:\n--- stderr\n{stderr}");
        assert_eq!(
            stdout.trim_end(),
            expected,
            "length did not answer {source} as both documents write it"
        );
    }

    // A record is the third thing SPEC.md used to claim `length` answered. Every
    // spelling of the name is asked, because the sentence named the function and
    // not one spelling of it.
    let refusal = "length requires a list or text";
    for source in [
        r#"say length({a: 1})"#,
        r#"say text.length({a: 1})"#,
        r#"say list.length({a: 1})"#,
        "say length({})",
    ] {
        let (ok, stdout, stderr) = run_named("length_refuses_a_record", source);
        assert!(
            !ok,
            "{source} answered, so `length` answers a record as well as a text and a list \
             and SPEC.md's old sentence is right after all:\n--- stdout\n{stdout}"
        );
        assert!(
            stderr.contains(refusal),
            "{source} was refused by another rule than the one that says a record is not a \
             length, so the sentence and the runtime have drifted again:\n--- stderr\n{stderr}"
        );
    }

    // The document, read rather than transcribed: a claim about what `length`
    // answers is only worth anything while it agrees with the rows above.
    let spec = read_doc("SPEC.md");
    let text_section = section_under(&spec, "### text");
    assert!(
        !text_section.trim().is_empty(),
        "SPEC.md has no § text, so the claim about what `length` answers cannot be read"
    );
    assert!(
        text_section.contains(refusal),
        "SPEC.md § text does not say that a record is refused as `{refusal}`, which is what \
         every spelling above does, so a reader is left to believe it answers one:\n\
         {text_section}"
    );
}

/// Every dotted call a document writes on a receiver that is **not** a module is
/// either an answer, or is written down as a refusal.
///
/// The other gates in this file each own one universe, and a receiver that is
/// neither belongs to neither:
///
/// - [`edge_a_module_does_not_answer_a_function_the_documents_say_it_does_not_have`]
///   walks `stdlib::MODULES` × short member names;
/// - [`edge_a_registered_builtin_is_not_a_module_namespace`] walks the prefixes
///   the builtin registry itself spells as `<name>_<member>`;
/// - [`the_text_and_math_examples_answer_what_the_spec_claims`] runs the code
///   blocks, and a call written only in prose is not in a code block.
///
/// `xs` is in none of them. SPEC.md § list promised `xs.map(f)` in a sentence —
/// "it works in all three spellings: `map(xs, f)`, `list.map(xs, f)` and
/// `xs.map(f)`" — and no gate read that sentence, so a call no program can make
/// sat in the specification with the whole suite green. It is
/// `RuntimeError: Module 'xs' has no function 'map'` in both engines: a dotted
/// call resolves its receiver against modules and objects, and a list held in a
/// variable is a value.
///
/// So the property is stated over the *documents* rather than over a table: a
/// `name.member(` on a bare identifier that no module claims either answers or
/// is documented as refused. A third outcome — a call that does not answer and is
/// not written down as refused — is the defect this phase exists for, and it is
/// what a reader copies into a file.
///
/// The probe is the **document's own call**, not a guess at one. It used to be
/// `say {spelling}(1)`, which cannot answer for the very spelling that brought
/// this gate into being: `xs` is bound by nothing, so the program died in the
/// analyzer before it reached the runtime, and `map` given `1` is refused even
/// with a list. The *answered* branch was therefore unreachable for every
/// receiver the documents write, and the gate could only ever pass on the
/// *written down* one — a gate with one working arm is a gate that cannot fail
/// on the half of the claim that matters most.
///
/// So the call is read out of the document with its arguments, and the names it
/// mentions are bound before it: the receiver to a list and each argument to a
/// declared function. That binding is derived from the document rather than
/// listed in the test, which is FINDINGS §10's rule — a gate's list has to come
/// from somewhere the gate is not auditing, and here the list would be exactly
/// the set of receivers the gate does not run.
#[test]
fn edge_a_dotted_receiver_that_is_no_module_answers_or_is_written_down_as_refused() {
    let mut asked: Vec<(&str, String, String)> = Vec::new();
    for document in ["SPEC.md", "README.md"] {
        let section = standard_library_section(&read_doc(document));
        for heading in subheadings(&section) {
            let body = section_under(&section, &heading);
            for (receiver, member) in called_dotted_receivers(&body) {
                if stdlib::is_module(&receiver) {
                    continue;
                }
                asked.push((document, heading.clone(), format!("{receiver}.{member}")));
            }
        }
    }
    assert!(
        !asked.is_empty(),
        "no document writes a dotted call on a receiver that is not a module, so this gate \
         would pass on any tree. It has to keep at least one such name to ask about."
    );

    for (document, heading, spelling) in &asked {
        let body = section_under(&standard_library_section(&read_doc(document)), heading);
        let call = documented_call(&body, spelling);
        let program = format!("{}say {call}\n", preamble_for(&call));
        let refusals = documented_refusals(&body, spelling);

        for (engine, run) in ENGINES {
            let (ok, _, stderr) = run("dotted_receiver", &program);
            if ok {
                continue;
            }
            assert!(
                !refusals.is_empty(),
                "{document} {heading} writes `{call}`, which {engine} does not answer and \
                 does not say it is refused — so a reader who copies it gets an error instead \
                 of the answer the document promised. Either keep it if it answers, or write \
                 the refusal down:\n{body}\n--- the program it would be\n{program}\
                 --- stderr\n{stderr}"
            );
            assert!(
                refusals.iter().any(|message| stderr.contains(message)),
                "{document} {heading} writes `{call}` as refused, and the message it names is \
                 not what {engine} refuses it with — so a reader who copies the sentence is \
                 told to expect a message they will not see:\n{body}\
                 --- the program it would be\n{program}--- stderr\n{stderr}"
            );
        }
    }
}

/// The call a document writes for `spelling`, with its arguments.
///
/// The document's own spelling, backticked or not, so the arity and the argument
/// names are the ones a reader copies rather than ones this file invented.
/// `xs.map(f)` rather than `xs.map(1)`: the first is what § list writes and the
/// second is a call no reader would make, so a probe built from it says nothing
/// about the spelling the gate exists for. `format!("{spelling}(1)")` is the
/// fallback, so a spelling the document writes without an argument list is still
/// probed rather than skipped.
fn documented_call(body: &str, spelling: &str) -> String {
    let mut from = 0;
    while let Some(at) = body[from..].find(spelling) {
        let start = from + at;
        let after = start + spelling.len();
        if let Some(call) = call_from(body, start, after) {
            return call;
        }
        from = after;
    }
    format!("{spelling}(1)")
}

/// The whole of the call that opens with `(` at `arguments` in `body`, up to the
/// parenthesis that closes it.
///
/// Counted rather than cut at the first `)`, so a call written with a nested one
/// — `xs.map(pick(xs, 1))` — is taken whole instead of losing its tail.
fn call_from(body: &str, start: usize, arguments: usize) -> Option<String> {
    if !body[arguments..].starts_with('(') {
        return None;
    }
    let mut depth = 0usize;
    for (offset, c) in body[arguments..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(body[start..arguments + offset + 1].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// The bindings a probe of `call` needs, for the names `call` itself mentions.
///
/// The receiver is bound to a list and each argument to a declared function —
/// the two shapes a `name.member(…)` a document writes in prose has: a list to
/// take a method of, and something to apply. `to f(x) … end` rather than
/// `set f to to (x) … end` because the bytecode compiler refuses a function
/// literal, and a probe that cannot be compiled answers for one engine only.
fn preamble_for(call: &str) -> String {
    let (head, arguments) = call.split_once('(').unwrap_or((call, ""));
    let receiver = head.split('.').next().unwrap_or(head).trim();
    let mut preamble = String::new();
    let mut bound = BTreeSet::new();
    if !receiver.is_empty()
        && receiver
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c == '_')
    {
        bound.insert(receiver.to_string());
        preamble.push_str(&format!("set {receiver} to [1, 2, 3]\n"));
    }
    for argument in arguments
        .trim_end_matches(')')
        .split(',')
        .map(str::trim)
        .filter(|name| {
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
    {
        if !bound.insert(argument.to_string()) {
            continue;
        }
        preamble.push_str(&format!("to {argument}(x)\n    give back x * 2\nend\n"));
    }
    preamble
}

/// The refusals a document writes down for `spelling`, if it writes any.
///
/// Empty unless a line names the spelling and says it is refused. What comes
/// back is the backticked text in the window around that line which reads as a
/// message rather than as a name — the text a reader copies out of the
/// sentence, which is the part that has to be the message the program produces.
/// A word like "refused" next to a call is not a documented refusal, it is
/// prose, and neither is the name of a module member mentioned in the same
/// breath: `… is refused as `Module 'x' has no function 'y'` — the same refusal
/// `list.filter` gives` carries three backticked runs and only the first is the
/// message, so the other two are dropped by asking for text with a space in it.
///
/// The window is the naming line and the two after it, because a document
/// wraps: SPEC.md writes `is refused as` on one line and the message on the
/// next. Every candidate is returned rather than the first, because a caller
/// matches them against what the program actually said.
fn documented_refusals(body: &str, spelling: &str) -> Vec<String> {
    let lines: Vec<&str> = body.lines().collect();
    let Some(index) = lines
        .iter()
        .position(|line| line.contains(spelling) && mentions_a_refusal(line))
    else {
        return Vec::new();
    };
    lines[index..(index + 3).min(lines.len())]
        .iter()
        .flat_map(|line| backticked_runs(line))
        .filter(|run| !run.contains('(') && run.contains(' '))
        .map(|run| run.trim().to_string())
        .collect()
}

/// Whether `line` says a call is refused rather than merely mentioning it.
///
/// The same three words the gate has always read, so a document that spells a
/// refusal as "no function" or "not implemented" still counts as one.
fn mentions_a_refusal(line: &str) -> bool {
    ["refuse", "no function", "not implemented"]
        .iter()
        .any(|word| line.contains(word))
}

/// The backticked runs of `line`, in the order they appear.
fn backticked_runs(line: &str) -> Vec<&str> {
    let mut runs = Vec::new();
    let mut parts = line.split('`');
    let mut inside = false;
    for part in parts.by_ref() {
        if inside {
            runs.push(part);
        }
        inside = !inside;
    }
    runs
}

/// The spellings SPEC.md § list enumerates for `map`, extracted from the sentence
/// that enumerates them and run on **both** engines.
///
/// `map` is the only higher-order builtin and § list is the one place a section
/// makes an enumerated prose promise rather than leaving the reader to find the
/// spellings in a code block — which is exactly why `the_text_and_math_examples_answer_what_the_spec_claims`
/// could not catch it. A section that says "three spellings" and then lists them
/// in a sentence has made a testable claim, so it gets a test: the list is read
/// out of the document, so a fourth spelling written into that sentence is run
/// too, and the sentence is found by what it contains rather than by a line
/// number, so moving it in SPEC.md moves the gate with it.
///
/// The three it names are `map(xs, f)`, `list.map(xs, f)` and a **literal**
/// receiver. A variable is not a spelling — see
/// [`edge_a_dotted_receiver_that_is_no_module_answers_or_is_written_down_as_refused`],
/// which is what keeps `xs.map(f)` out of this sentence.
///
/// Both engines, because one is not enough and saying so in a doc comment is not
/// a substitute. Before this round the gate ran `rb run` only, and wrote down
/// here that the bytecode VM refused every one of these three spellings — so
/// SPEC.md § list promised a sentence that was true on one of the two engines and
/// the suite was green. That is the same trade FINDINGS §4 records being
/// withdrawn for `time`: a claim a document makes about the language, checked
/// against a subset of the language and recorded as correct.
///
/// `xs` and `f` are the two names the section's own spelling list uses, so the
/// program carrying the sentence's spelling and nothing else is the program a
/// reader writes. The function is a *named* declaration rather than a `to (x) …
/// end` literal because the bytecode compiler refuses a literal outright, and a
/// gate for this sentence has to reach both engines.
#[test]
fn edge_every_spelling_the_spec_promises_for_map_answers() {
    let section = section_under(&read_doc("SPEC.md"), "### list");
    assert!(
        !section.trim().is_empty(),
        "SPEC.md has no § list, so the spellings it promises for `map` cannot be read"
    );

    // The enumerating sentence is the one that carries `map(xs, f)`: that is the
    // spelling which cannot be written as a bare receiver, so it is the one a
    // section saying "three spellings" cannot leave out.
    let promise = section
        .lines()
        .find(|line| line.contains("`map(xs, f)`"))
        .unwrap_or_else(|| {
            panic!(
                "SPEC.md § list no longer enumerates the spellings `map` answers, so the \
                 promise cannot be read back. The claim a reader copies lives here:\n{section}"
            )
        });
    let spellings = backticked_calls(promise);
    assert!(
        !spellings.is_empty(),
        "the sentence enumerating the spellings of `map` names none that can be run: {promise}"
    );

    let preamble = "set xs to [1, 2, 3]\nto f(x)\n    give back x * 2\nend\n";
    for (engine, run) in ENGINES {
        for spelling in &spellings {
            let program = format!("{preamble}say {spelling}\n");

            let (ok, stdout, stderr) = run("map_spelling", &program);
            assert!(
                ok,
                "SPEC.md § list promises `{spelling}` and {engine} refused it:\n{program}\
                 --- stderr\n{stderr}"
            );
            assert_eq!(
                stdout.trim_end(),
                "[2, 4, 6]",
                "SPEC.md § list promises `{spelling}` and {engine} answered something \
                 else:\n{program}--- stderr\n{stderr}"
            );
        }
    }
}

/// The `### ` headings inside a `## Standard Library` section.
fn subheadings(section: &str) -> Vec<String> {
    section
        .lines()
        .filter(|line| line.starts_with("### "))
        .map(|line| line.trim_end().to_string())
        .collect()
}

/// The `(receiver, member)` of every `name.member(` `text` writes.
///
/// The `(` is what makes it a call, and it is the whole filter. `config.rb`,
/// `out.bin` and `api.example.com` are a path, a file and a host, and an
/// identifier in this language cannot contain a `.` — so a scan without it reads
/// every dotted thing a document mentions as a receiver, which is how
/// [`dotted_calls`] earns its name and this earns a stricter one.
///
/// A chain of three or more names (`a.b.c(`) is dropped for the same reason: it is
/// a host or a path, not a receiver and a member.
fn called_dotted_receivers(text: &str) -> BTreeSet<(String, String)> {
    let is_chain_part = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '.';

    let mut names = BTreeSet::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_alphabetic() && chars[i] != '_' {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && is_chain_part(chars[i]) {
            i += 1;
        }
        let chain: String = chars[start..i].iter().collect();
        if chain.matches('.').count() != 1 || chars.get(i) != Some(&'(') {
            continue;
        }
        if let Some((receiver, member)) = chain.split_once('.') {
            names.insert((receiver.to_string(), member.to_string()));
        }
    }
    names
}

/// The backticked tokens of `line` that read as a call — one that opens and one
/// that closes.
///
/// Not a parse: a redblue call in prose is written by hand and this only has to
/// tell `map(xs, f)` — which a reader can copy — from `map`, which is a name.
fn backticked_calls(line: &str) -> Vec<String> {
    let mut calls = Vec::new();
    let mut parts = line.split('`');
    let mut inside = false;
    for part in parts.by_ref() {
        if inside && part.contains('(') && part.ends_with(')') {
            calls.push(part.to_string());
        }
        inside = !inside;
    }
    calls
}
