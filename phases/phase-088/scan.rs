//! Find the tests that run a Redblue program and never compare what it answered.
//!
//! The audit behind phase-088 said that "twenty tests across `tests/` have no
//! reachable `assert!` or `assert_eq!` at all". A count that is asserted rather
//! than derived is the defect this phase set out to remove, so the count has to
//! come out of a scanner anybody can run again.
//!
//! # Running it
//!
//! The scanner is one file with no dependencies, so it runs from `rustc`:
//!
//! ```text
//! rustc --edition 2021 phases/phase-088/scan.rs -o target/tmp/rbscan
//! target/tmp/rbscan tests            # this checkout
//! target/tmp/rbscan /tmp/base/tests  # another tree
//! target/tmp/rbscan --list tests     # the file list only
//! ```
//!
//! `tests/phase_088_scanner_test.rs` includes **this very file**, so the rules
//! below are the rules the test suite checks: one implementation, and no way for
//! the published tool and the checked rules to drift apart. That is also why
//! the scanner is Rust and not a script — a Rust project whose suite needs a
//! Python interpreter to go green is a portability cost the phase did not have
//! to pay, and a suite that cannot run without a second toolchain is a suite
//! whose evidence nobody can check.
//!
//! # The scan, in the order it is applied
//!
//! 1. Every `tests/*.rs` file is read — the top level of `tests/` only. The
//!    `tests/common/` helpers are read as well, and they are followed: the
//!    modules `tests/common/mod.rs` declares are loaded, a qualified call such as
//!    `corpus::compare(...)` is followed into `tests/common/corpus.rs`, and a
//!    bare call is resolved in the file being scanned first and in the modules
//!    after it. A test whose assertion lives in one of those helpers is
//!    therefore seen through it.
//! 2. Comments are masked out, and so are string literals — a test that mentions
//!    `take_output` inside a Redblue fixture is not a Rust assertion. The masking
//!    keeps every byte, so the file's line numbers and character offsets are the
//!    same in the masked copy as in the original, and a second copy with only the
//!    comments masked keeps the fixtures.
//! 3. Every top-level `fn` in the file is brace-matched, and the ones carrying
//!    `#[test]` are the tests. A `fn` nested inside another function is not a
//!    test, however many `#[test]` attributes are in the text before it.
//! 4. A test is a **candidate** when it runs a program — its own body, or the
//!    body of a helper it calls, mentions `.run(`, `run_source`, `run_isolated`,
//!    `run_chunk`, `tree_walk` or `bytecode(`. The last one is spelled with its
//!    bracket on purpose: `redblue::bytecode::compile_program` compiles a program
//!    and does not run one, so a test that reads a compiled chunk's instructions
//!    is not a candidate — it asserts about the compiled form, which is a real
//!    thing to assert.
//! 5. A candidate is **vacuous** when nothing in it compares what the program
//!    answered.
//!
//! # What counts as a comparison, and what does not
//!
//! * A comparison is a place where the program's answer is held against
//!   something else: an `assert_eq!`, `assert_ne!` or `assert_matches!`; an
//!   `assert!` whose condition says more than `is_ok()`; an `==` or `!=`; or a
//!   `match` on the program's own answer against a literal, such as
//!   `Ok(Value::YesNo(true))`.
//! * The comparison only counts if what it holds against something else is the
//!   program's **answer**: the output it printed (`take_output`, `.output`,
//!   `stdout`), the error it refused with (`expect_err`), the value it ended on
//!   (`Ok(Value::…)`), or a name bound to one of those. A test that reads the
//!   program's output and never compares it is vacuous, and so is a test whose
//!   only `assert_eq!` compares something that never came from the program.
//! * A name carries the answer forward in four shapes, and all four are followed:
//!   it is **bound** to the answer (`let lines = vm.take_output();`), bound to a
//!   name that already carries it (`let n = out.len();`), **put into** a
//!   collection the answer is read back out of (`seen.insert(item)`), or it is
//!   the object the program was **run through** (`harness.run_source(..)` — the
//!   harness holds the outcome of the program it ran).
//! * `assert!(run_source(src).is_ok())` does **not** count: it says "the program
//!   ran", which is the shape this scan is looking for. A condition that also
//!   names a value (`is_err()`, `==`, `contains`, `matches`) counts, so a test
//!   that asserts a *failure* was produced is never mistaken for a vacuous one.
//!   A **negated** `is_ok()` counts too: `assert!(!run_source(src).is_ok())`
//!   asserts that the program was *refused*, which is a claim about what the
//!   program answered and not about whether it ran at all.
//! * `panic!` does **not** count on its own: a test that panics only on `Err`
//!   proves the program ran, and proves nothing about what it said. A panic that
//!   guards a comparison counts, because the comparison it guards is the
//!   assertion — the shape `tests/common/corpus.rs::compare` is called in, where
//!   the comparison is in the helper and the panic is in the caller.
//! * A Redblue-level `expect ... to be` **does** count. It lives inside a
//!   fixture, the program checks its own answer, and a failed `expect` is a
//!   `Runtime` error the run hands back (`src/interpreter.rs:2331`), so the Rust
//!   test around it fails when the check fails. The program's own check is an
//!   assertion, not a reachability check.
//!
//! # Why the helper step is the one that matters
//!
//! Two of the candidates an earlier scan flagged are not defects — they assert
//! through `both_print`, which compares both engines' output to a literal — and
//! a scan that cannot see through a helper reads them as vacuous. Each function
//! is judged **on its own**, too: a name bound in one function says nothing
//! about a comparison in another, and a concatenation of two functions' text
//! would let the first function's names satisfy the second function's
//! comparison.
//!
//! # The output
//!
//! One line per vacuous test: the file, the line the test is defined on, and
//! its name. The count of those lines is the number the report has to state.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// The tokens that mean "this test runs a Redblue program".
///
/// `bytecode(` rather than a bare `bytecode`, on purpose:
/// `redblue::bytecode::compile_program` compiles a program and does not run one,
/// so a test that compiles a program and reads its instructions is not a
/// candidate — it asserts about the compiled form, which is a real thing to
/// assert, and there is no program whose answer it could be getting wrong.
const RUNS_A_PROGRAM: [&str; 6] = [
    ".run(",
    "run_source",
    "run_isolated",
    "run_chunk",
    "tree_walk",
    "bytecode(",
];

/// The tokens that mean "this runs a Redblue program and answers with it".
const ANSWERS_THE_PROGRAM: [&str; 6] = RUNS_A_PROGRAM;

/// The tokens that read what the program said, recorded or answered.
const READS_THE_PROGRAM: [&str; 5] = [
    "take_output",
    ".output",
    "stdout",
    "take_expectation_failure",
    "expect_err",
];

/// A match on the program's own answer — `Ok(Value::…)`.
const MATCHES_THE_ANSWER: &str = "Ok(Value::";

/// The words a `let` pattern may carry that are not names.
const NOT_NAMES: [&str; 16] = [
    "as", "const", "crate", "dyn", "false", "fn", "in", "let", "move", "mut", "ref", "self",
    "static", "super", "true", "use",
];

/// A function's body: the code with the strings masked, and the fixture text
/// with only the comments masked.
type Entry = (Vec<char>, Vec<char>);

/// The scan's verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scan {
    /// The number of files it read.
    pub files: usize,
    /// The vacuous tests it named: the file, the line it is defined on, and its
    /// name.
    pub named: Vec<(String, usize, String)>,
}

impl Scan {
    /// The number of vacuous tests.
    pub fn count(&self) -> usize {
        self.named.len()
    }

    /// The scan's output, in the form the report quotes.
    pub fn render(&self, root: &Path) -> String {
        let mut out = format!("scanned {} files in {}\n", self.files, root.display());
        for (file, line, name) in &self.named {
            out.push_str(&format!("{file}:{line}\t{name}\n"));
        }
        out.push_str(&format!("vacuous tests: {}\n", self.count()));
        out
    }
}

/// Is `c` the start of an identifier?
fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// Is `c` a character an identifier may carry after its first?
fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The index of the first occurrence of `needle` in `haystack`.
fn find_sub(haystack: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&i| haystack[i..i + needle.len()] == needle[..])
}

/// Whether `haystack` contains `needle`.
fn contains(haystack: &[char], needle: &str) -> bool {
    let needle: Vec<char> = needle.chars().collect();
    find_sub(haystack, &needle).is_some()
}

/// `chunk`, with everything but its newlines blanked out.
fn blank(chunk: &[char]) -> Vec<char> {
    chunk
        .iter()
        .map(|c| if *c == '\n' { '\n' } else { ' ' })
        .collect()
}

/// Blank out comments (and optionally string literals), keeping every byte.
///
/// Rust block comments nest, so the comment scanner counts them.
fn mask(text: &[char], mask_strings: bool) -> Vec<char> {
    let mut out: Vec<char> = Vec::with_capacity(text.len());
    let mut i = 0;
    let n = text.len();
    while i < n {
        let ch = text[i];
        if ch == '/' && i + 1 < n && text[i + 1] == '/' {
            let mut end = i;
            while end < n && text[end] != '\n' {
                end += 1;
            }
            out.extend(blank(&text[i..end]));
            i = end;
            continue;
        }
        if ch == '/' && i + 1 < n && text[i + 1] == '*' {
            let start = i;
            let mut depth = 0usize;
            while i < n {
                if text[i] == '/' && i + 1 < n && text[i + 1] == '*' {
                    depth += 1;
                    i += 2;
                } else if text[i] == '*' && i + 1 < n && text[i + 1] == '/' {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            out.extend(blank(&text[start..i]));
            continue;
        }
        if mask_strings && ch == 'r' && i + 1 < n && (text[i + 1] == '"' || text[i + 1] == '#') {
            let mut j = i + 1;
            let mut hashes = 0usize;
            while j < n && text[j] == '#' {
                hashes += 1;
                j += 1;
            }
            if j < n && text[j] == '"' {
                let close: Vec<char> = std::iter::once('"')
                    .chain(std::iter::repeat_n('#', hashes))
                    .collect();
                let mut end = j + 1;
                while end + close.len() <= n && text[end..end + close.len()] != close[..] {
                    end += 1;
                }
                if end + close.len() > n {
                    end = n;
                } else {
                    end += close.len();
                }
                out.extend(blank(&text[i..end]));
                i = end;
                continue;
            }
        }
        if mask_strings && ch == '"' {
            let start = i;
            i += 1;
            while i < n {
                if text[i] == '\\' {
                    i += 2;
                    continue;
                }
                if text[i] == '"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.extend(blank(&text[start..i.min(n)]));
            continue;
        }
        if mask_strings && ch == '\'' {
            // A lifetime ('a) is not a char literal; a char literal is one
            // character, optionally escaped, then a closing quote.
            let mut j = i + 1;
            if j < n && text[j] == '\\' {
                j += 2;
                while j < n && text[j] != '\'' {
                    j += 1;
                }
                j += 1;
            } else if j < n {
                j += 1;
                while j < n && text[j] != '\'' {
                    j += 1;
                }
                j += 1;
            }
            let literal = &text[i..j.min(n)];
            if literal.len() <= 4 && literal.last() == Some(&'\'') {
                out.extend(blank(literal));
                i = j.min(n);
                continue;
            }
        }
        out.push(ch);
        i += 1;
    }
    out
}

/// The index just past the `}` that closes the `{` at `start`.
fn match_brace(text: &[char], start: usize) -> usize {
    let mut depth = 0i64;
    for (offset, ch) in text[start..].iter().enumerate() {
        if *ch == '{' {
            depth += 1;
        } else if *ch == '}' {
            depth -= 1;
            if depth == 0 {
                return start + offset + 1;
            }
        }
    }
    text.len()
}

/// The index just past the `)` that closes the `(` at `start`.
fn match_paren(text: &[char], start: usize) -> usize {
    let mut depth = 0i64;
    for (offset, ch) in text[start..].iter().enumerate() {
        if *ch == '(' {
            depth += 1;
        } else if *ch == ')' {
            depth -= 1;
            if depth == 0 {
                return start + offset + 1;
            }
        }
    }
    text.len()
}

/// One top-level function of a file.
#[derive(Debug, Clone)]
struct Function {
    /// Its name.
    name: String,
    /// The line it is defined on.
    line: usize,
    /// The index of the `fn` keyword.
    start: usize,
    /// The index of the `{` that opens its body.
    brace: usize,
    /// The index just past the `}` that closes its body.
    end: usize,
}

/// Every top-level `fn` in `code`.
///
/// A `fn` nested inside another function is not a test, however many `#[test]`
/// attributes are in the text before it — the attribute belongs to the function
/// it is written in.
fn find_functions(code: &[char]) -> Vec<Function> {
    let mut functions = Vec::new();
    let mut i = 0;
    while i < code.len() {
        if !is_ident_start(code[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < code.len() && is_ident_continue(code[i]) {
            i += 1;
        }
        if code[start..i] != ['f', 'n'] {
            continue;
        }
        let mut after = i;
        while after < code.len() && code[after].is_whitespace() {
            after += 1;
        }
        if after >= code.len() || !is_ident_start(code[after]) {
            continue;
        }
        let name_end = identifier_end(code, after);
        let name: String = code[after..name_end].iter().collect();
        let before = &code[..start];
        if before.iter().filter(|c| **c == '{').count()
            != before.iter().filter(|c| **c == '}').count()
        {
            continue;
        }
        let Some(brace) = (name_end..code.len()).find(|&i| code[i] == '{') else {
            continue;
        };
        let end = match_brace(code, brace);
        let line = code[..start].iter().filter(|c| **c == '\n').count() + 1;
        functions.push(Function {
            name,
            line,
            start,
            brace,
            end,
        });
    }
    functions
}

/// The index just past the identifier that starts at `start`.
fn identifier_end(code: &[char], start: usize) -> usize {
    let mut i = start;
    while i < code.len() && is_ident_continue(code[i]) {
        i += 1;
    }
    i
}

/// The names a body calls, so a helper's body can be followed.
fn called_names(body: &[char]) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut i = 0;
    while i < body.len() {
        if !is_ident_start(body[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let end = identifier_end(body, start);
        let mut after = end;
        while after < body.len() && body[after].is_whitespace() {
            after += 1;
        }
        if after < body.len() && body[after] == '(' {
            names.insert(body[start..end].iter().collect());
        }
        i = end;
    }
    names
}

/// The qualified names a body calls, such as `corpus::compare`.
fn called_paths(body: &[char]) -> HashSet<String> {
    let mut paths = HashSet::new();
    let mut i = 0;
    while i < body.len() {
        if !is_ident_start(body[i]) {
            i += 1;
            continue;
        }
        let mut segments: Vec<String> = Vec::new();
        let mut j = i;
        loop {
            let end = identifier_end(body, j);
            if end == j {
                break;
            }
            segments.push(body[j..end].iter().collect());
            if end + 1 < body.len() && body[end] == ':' && body[end + 1] == ':' {
                j = end + 2;
                continue;
            }
            j = end;
            break;
        }
        if segments.len() >= 2 {
            let mut after = j;
            while after < body.len() && body[after].is_whitespace() {
                after += 1;
            }
            if after < body.len() && body[after] == '(' {
                paths.insert(segments.join("::"));
            }
        }
        i = j.max(i + 1);
    }
    paths
}

/// The identifiers a piece of text mentions.
fn names_in(text: &[char]) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut i = 0;
    while i < text.len() {
        if !is_ident_start(text[i]) {
            i += 1;
            continue;
        }
        let end = identifier_end(text, i);
        names.insert(text[i..end].iter().collect());
        i = end;
    }
    names
}

/// Whether an `assert!`'s condition says only "the program ran".
///
/// `is_ok()` with nothing else in the condition is a reachability check.
/// A condition that also names a value — `is_err()`, `==`, `contains`,
/// `matches` — is an assertion about the program's answer, not about whether
/// it ran at all, and so is a **negated** `is_ok()`: `assert!(!result.is_ok())`
/// asserts that the program was refused, which says what the program answered
/// and not merely that it ran.
fn is_reachability_check(argument: &[char]) -> bool {
    if !contains(argument, "is_ok()") {
        return false;
    }
    for token in ["is_err()", "==", "!=", "contains", "matches"] {
        if contains(argument, token) {
            return false;
        }
    }
    if negates_an_is_ok_call(argument) {
        return false;
    }
    true
}

/// Whether the text negates an `is_ok()` call.
///
/// The negation is a `!` in front of the call — `!result.is_ok()`,
/// `!run_source(src).is_ok()`, `!is_ok()` — however many receivers and
/// arguments the call has.
fn negates_an_is_ok_call(text: &[char]) -> bool {
    let needle: Vec<char> = "is_ok()".chars().collect();
    let mut from = 0;
    while let Some(offset) = find_sub(&text[from..], &needle) {
        let at = from + offset;
        from = at + 1;
        if negated_before(text, at) {
            return true;
        }
    }
    false
}

/// Whether the `is_ok()` at `at` is negated by a `!` in front of it.
fn negated_before(text: &[char], at: usize) -> bool {
    let mut i = at;
    while i > 0 && text[i - 1].is_whitespace() {
        i -= 1;
    }
    if i > 0 && text[i - 1] == '.' {
        i -= 1;
        while i > 0 && text[i - 1].is_whitespace() {
            i -= 1;
        }
        if i > 0 && text[i - 1] == ')' {
            // A call: walk back over the argument list to its `(`.
            let mut depth = 0i64;
            let mut k = i;
            while k > 0 {
                k -= 1;
                if text[k] == ')' {
                    depth += 1;
                } else if text[k] == '(' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
            }
            i = k;
        }
        while i > 0 && (is_ident_continue(text[i - 1]) || text[i - 1] == ':') {
            i -= 1;
        }
        while i > 0 && text[i - 1].is_whitespace() {
            i -= 1;
        }
    }
    i > 0 && text[i - 1] == '!'
}

/// The statement the character at `position` is in.
fn statement_around(code: &[char], position: usize) -> &[char] {
    let start = code[..position]
        .iter()
        .rposition(|c| *c == ';' || *c == '{' || *c == '}')
        .map(|i| i + 1)
        .unwrap_or(0);
    let end = code[position..]
        .iter()
        .position(|c| *c == ';' || *c == '}')
        .map(|i| position + i + 1)
        .unwrap_or(code.len());
    &code[start..end]
}

/// The end of the statement that starts at `position`.
fn statement_end(code: &[char], position: usize) -> usize {
    let mut depth = 0i64;
    for (offset, ch) in code[position..].iter().enumerate() {
        let ch = *ch;
        if ch == '(' || ch == '[' || ch == '{' {
            depth += 1;
        } else if ch == ')' || ch == ']' || ch == '}' {
            if depth == 0 {
                return position + offset;
            }
            depth -= 1;
        } else if ch == ';' && depth == 0 {
            return position + offset;
        }
    }
    code.len()
}

/// The helper functions a scan can follow.
///
/// `local` holds the file being scanned; `modules` holds the `tests/common/`
/// modules, by module name. A call is resolved by its bare name — the file's
/// own helper first, then the modules — or by a qualified name such as
/// `corpus::compare`.
struct Helpers {
    local: HashMap<String, Entry>,
    modules: Vec<(String, HashMap<String, Entry>)>,
}

impl Helpers {
    /// The body and the fixture text of the function a call names.
    fn entry_of(&self, path: &str) -> &Entry {
        if let Some((module, bare)) = path.rsplit_once("::") {
            return self
                .modules
                .iter()
                .find(|(name, _)| name == module)
                .and_then(|(_, functions)| functions.get(bare))
                .unwrap_or(&EMPTY_ENTRY);
        }
        if let Some(entry) = self.local.get(path) {
            return entry;
        }
        for (_, functions) in &self.modules {
            if let Some(entry) = functions.get(path) {
                return entry;
            }
        }
        &EMPTY_ENTRY
    }

    /// The body of the function a call names, or the empty string.
    fn body_of(&self, path: &str) -> &[char] {
        &self.entry_of(path).0
    }

    /// The fixture text of the function a call names.
    fn fixture_of(&self, path: &str) -> &[char] {
        &self.entry_of(path).1
    }

    /// The body of a helper and of every helper it calls.
    fn closure(&self, path: &str, seen: &mut HashSet<String>) -> Vec<char> {
        if !seen.insert(path.to_string()) {
            return Vec::new();
        }
        let mut body = self.body_of(path).to_vec();
        let mut callees: Vec<String> = called_names(&body).into_iter().collect();
        callees.extend(called_paths(&body));
        for callee in callees {
            body.extend(self.closure(&callee, seen));
        }
        body
    }

    /// Whether a helper reads or runs the program it is given.
    fn answers(&self, path: &str) -> bool {
        let text = self.closure(path, &mut HashSet::new());
        if READS_THE_PROGRAM.iter().any(|t| contains(&text, t))
            || contains(&text, MATCHES_THE_ANSWER)
        {
            return true;
        }
        ANSWERS_THE_PROGRAM.iter().any(|t| contains(&text, t))
    }
}

/// The entry a call to an unknown helper resolves to.
static EMPTY_ENTRY: Entry = (Vec::new(), Vec::new());

/// The `tests/common/` modules: module name -> {function name: (body, fixture)}.
fn load_modules(root: &Path) -> Vec<(String, HashMap<String, Entry>)> {
    let common = root.join("common");
    let mod_file = common.join("mod.rs");
    if !mod_file.is_file() {
        return Vec::new();
    }
    let Ok(text) = fs::read_to_string(&mod_file) else {
        return Vec::new();
    };
    let text: Vec<char> = text.chars().collect();
    let mut modules = Vec::new();
    for name in declared_modules(&text) {
        let path = common.join(format!("{name}.rs"));
        if !path.is_file() {
            continue;
        }
        modules.push((name, function_bodies(&path)));
    }
    modules
}

/// The module names `tests/common/mod.rs` declares.
fn declared_modules(text: &[char]) -> Vec<String> {
    let mut names = Vec::new();
    let mut i = 0;
    while i < text.len() {
        if !is_ident_start(text[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let end = identifier_end(text, start);
        let word = &text[start..end];
        if word == ['m', 'o', 'd'] {
            let mut after = end;
            while after < text.len() && text[after].is_whitespace() {
                after += 1;
            }
            if after < text.len() && is_ident_start(text[after]) {
                let name_end = identifier_end(text, after);
                let mut semi = name_end;
                while semi < text.len() && text[semi].is_whitespace() {
                    semi += 1;
                }
                if semi < text.len() && text[semi] == ';' {
                    names.push(text[after..name_end].iter().collect());
                    i = semi + 1;
                    continue;
                }
            }
        }
        i = end;
    }
    names
}

/// Every top-level function of one file: name -> (body, fixture).
///
/// The body has the string literals masked out, and the fixture keeps them: a
/// Redblue-level `expect ... to be` lives inside a fixture, and it is the
/// program checking its own answer.
fn function_bodies(path: &Path) -> HashMap<String, Entry> {
    let Ok(raw) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    let raw: Vec<char> = raw.chars().collect();
    let code = mask(&mask(&raw, true), false);
    let words = mask(&raw, false);
    find_functions(&code)
        .into_iter()
        .map(|function| {
            (
                function.name,
                (
                    code[function.brace..function.end].to_vec(),
                    words[function.brace..function.end].to_vec(),
                ),
            )
        })
        .collect()
}

/// The names a body binds, and the expression each is bound to.
///
/// Four shapes are recognised: a `let`, a `for` loop's pattern, a `match`
/// arm whose pattern binds the value the match is looking at, and a plain
/// assignment. The second and third matter because an answer is often read
/// out of a loop or out of a `match` rather than out of a `let`.
fn bindings(code: &[char]) -> Vec<(HashSet<String>, Vec<char>)> {
    let mut found: Vec<(HashSet<String>, Vec<char>)> = Vec::new();
    for (names, right) in let_bindings(code) {
        found.push((names, right));
    }
    for (names, right) in for_bindings(code) {
        found.push((names, right));
    }
    for (names, right) in match_bindings(code) {
        found.push((names, right));
    }
    for (names, right) in assignment_bindings(code) {
        found.push((names, right));
    }
    found
}

/// The names a body binds to the program's own answer.
///
/// A name is derived when it is bound to the output, to the error, to the
/// value the program ended on, or to the result of running one — directly, or
/// through a helper that does any of those, or through another derived name.
/// The last part is a fixpoint: `let n = out.len();` derives `n` because
/// `out` is already derived.
///
/// Three more shapes carry the answer forward:
///
/// * the object a program is run **through** — `harness` holds the outcome of
///   the program it ran, and `vm` holds the lines it printed;
/// * a collection the answer is **put into** — `seen.insert(item)` makes
///   `seen` a name the answer is read out of;
/// * a name the answer is **written to** — `sorted = lines.sort()`.
fn derived_names(code: &[char], helpers: &Helpers) -> HashSet<String> {
    let pairs = bindings(code);
    let mut names: HashSet<String> = HashSet::new();
    for (receiver, method, _) in method_calls(code) {
        if RUNNING_METHODS.contains(&method.as_str()) {
            names.insert(receiver);
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for (bound, source) in &pairs {
            if bound.is_empty() || bound.is_subset(&names) {
                continue;
            }
            if is_the_answer(source, helpers, &names) {
                names.extend(bound.iter().cloned());
                changed = true;
            }
        }
        for (receiver, _method, argument) in method_calls(code) {
            if names.contains(&receiver) {
                continue;
            }
            if is_the_answer(&argument, helpers, &names) {
                names.insert(receiver);
                changed = true;
            }
        }
    }
    names
}

/// The methods whose call means a program was run through the receiver.
const RUNNING_METHODS: [&str; 4] = ["run", "run_source", "run_isolated", "run_chunk"];

/// Every method call in `code`: the receiver, the method, and the argument
/// text inside the call's parentheses.
fn method_calls(code: &[char]) -> Vec<(String, String, Vec<char>)> {
    let mut calls = Vec::new();
    let mut i = 0;
    while i < code.len() {
        if !is_ident_start(code[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let end = identifier_end(code, start);
        let receiver: String = code[start..end].iter().collect();
        let mut dot = end;
        while dot < code.len() && code[dot].is_whitespace() {
            dot += 1;
        }
        if dot < code.len() && code[dot] == '.' {
            let mut after = dot + 1;
            while after < code.len() && code[after].is_whitespace() {
                after += 1;
            }
            if after < code.len() && is_ident_start(code[after]) {
                let method_end = identifier_end(code, after);
                let method: String = code[after..method_end].iter().collect();
                let mut open = method_end;
                while open < code.len() && code[open].is_whitespace() {
                    open += 1;
                }
                if open < code.len() && code[open] == '(' {
                    let close = match_paren(code, open);
                    let argument: Vec<char> = code[open + 1..close].to_vec();
                    calls.push((receiver, method, argument));
                }
            }
        }
        i = end;
    }
    calls
}

/// Whether `text` is the program's answer, or a value read out of it.
fn is_the_answer(text: &[char], helpers: &Helpers, derived: &HashSet<String>) -> bool {
    if READS_THE_PROGRAM.iter().any(|t| contains(text, t)) || contains(text, MATCHES_THE_ANSWER) {
        return true;
    }
    if ANSWERS_THE_PROGRAM.iter().any(|t| contains(text, t)) {
        return true;
    }
    let mentioned = names_in(text);
    if derived.iter().any(|name| mentioned.contains(name)) {
        return true;
    }
    let mut callees: Vec<String> = called_names(text).into_iter().collect();
    callees.extend(called_paths(text));
    callees.iter().any(|callee| helpers.answers(callee))
}

/// The text of every comparison in `code`.
///
/// A comparison is a place where the program's answer is held against
/// something else: an `assert_eq!`, `assert_ne!` or `assert_matches!`; an
/// `assert!` whose condition says more than `is_ok()`; an `==` or `!=`; or a
/// `match` on the program's own answer against a literal.
fn comparison_contexts(code: &[char]) -> Vec<Vec<char>> {
    let mut contexts: Vec<Vec<char>> = Vec::new();
    for (name, argument) in assert_macros(code) {
        if name == "assert" && is_reachability_check(&argument) {
            continue;
        }
        contexts.push(argument);
    }
    for i in 1..code.len().saturating_sub(1) {
        let is_equality = code[i] == '=' && code[i + 1] == '=';
        let is_inequality = code[i] == '!' && code[i + 1] == '=';
        if !(is_equality || is_inequality) {
            continue;
        }
        if "=!<>".contains(code[i - 1]) || code[i + 2] == '=' {
            continue;
        }
        contexts.push(statement_around(code, i - 1).to_vec());
    }
    for (matched, inner) in value_matches(code) {
        if let Some(inner) = inner {
            let literal = strip_booleans(&inner);
            if !names_in(&literal).is_empty() {
                // It binds the answer rather than comparing it.
                continue;
            }
        }
        contexts.push(matched);
    }
    // A call whose name begins with `assert` is an assertion by name: the
    // REPL harness's `assert_stdout_contains(..)` compares what the session
    // printed, and the name is the only place that says so.
    for (start, end) in assert_calls(code) {
        contexts.push(code[start..end].to_vec());
    }
    contexts
}

/// The `assert!` family in `code`: the macro's name and the text inside its
/// parentheses.
fn assert_macros(code: &[char]) -> Vec<(String, Vec<char>)> {
    let mut macros = Vec::new();
    let mut i = 0;
    while i < code.len() {
        if !is_ident_start(code[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let end = identifier_end(code, start);
        let name: String = code[start..end].iter().collect();
        let is_assert = matches!(
            name.as_str(),
            "assert" | "assert_eq" | "assert_ne" | "assert_matches"
        );
        let mut bang = end;
        while bang < code.len() && code[bang].is_whitespace() {
            bang += 1;
        }
        if is_assert && bang < code.len() && code[bang] == '!' {
            if let Some(open) = (bang + 1..code.len()).find(|&i| code[i] == '(') {
                let close = match_paren(code, open);
                macros.push((name, code[open + 1..close].to_vec()));
            }
        }
        i = end;
    }
    macros
}

/// The `Ok(Value::…)` matches in `code`: the matched text and the text inside
/// the variant's parentheses, if it has any.
fn value_matches(code: &[char]) -> Vec<(Vec<char>, Option<Vec<char>>)> {
    let needle: Vec<char> = MATCHES_THE_ANSWER.chars().collect();
    let mut matches = Vec::new();
    let mut from = 0;
    while let Some(offset) = find_sub(&code[from..], &needle) {
        let start = from + offset;
        let mut i = start + needle.len();
        if i >= code.len() || !is_ident_start(code[i]) {
            from = start + 1;
            continue;
        }
        let name_end = identifier_end(code, i);
        i = name_end;
        if i < code.len() && code[i] == '(' {
            let close = (i + 1..code.len()).find(|&k| code[k] == ')');
            let matched = close.filter(|&close| {
                !code[i + 1..close].contains(&'(')
                    && close + 1 < code.len()
                    && code[close + 1] == ')'
            });
            if let Some(close) = matched {
                matches.push((
                    code[start..close + 2].to_vec(),
                    Some(code[i + 1..close].to_vec()),
                ));
                from = close + 2;
                continue;
            }
        } else if i < code.len() && code[i] == ')' {
            matches.push((code[start..i + 1].to_vec(), None));
            from = i + 1;
            continue;
        }
        from = start + 1;
    }
    matches
}

/// The `true` and `false` in a piece of text, removed.
fn strip_booleans(text: &[char]) -> Vec<char> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        if is_ident_start(text[i]) {
            let end = identifier_end(text, i);
            let word: String = text[i..end].iter().collect();
            if word != "true" && word != "false" {
                out.extend_from_slice(&text[i..end]);
            }
            i = end;
            continue;
        }
        out.push(text[i]);
        i += 1;
    }
    out
}

/// The calls whose name begins with `assert`, as a range of `code`.
fn assert_calls(code: &[char]) -> Vec<(usize, usize)> {
    let mut calls = Vec::new();
    let mut i = 0;
    while i < code.len() {
        if !is_ident_start(code[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let end = identifier_end(code, start);
        let name: String = code[start..end].iter().collect();
        let mut open = end;
        while open < code.len() && code[open].is_whitespace() {
            open += 1;
        }
        if name.starts_with("assert_") && open < code.len() && code[open] == '(' {
            calls.push((start, match_paren(code, open)));
        }
        i = end;
    }
    calls
}

/// Whether `text` holds the program's answer against something else.
fn mentions_the_answer(text: &[char], derived: &HashSet<String>, helpers: &Helpers) -> bool {
    if READS_THE_PROGRAM.iter().any(|t| contains(text, t)) || contains(text, MATCHES_THE_ANSWER) {
        return true;
    }
    let mentioned = names_in(text);
    if derived.iter().any(|name| mentioned.contains(name)) {
        return true;
    }
    let mut callees: Vec<String> = called_names(text).into_iter().collect();
    callees.extend(called_paths(text));
    callees.iter().any(|callee| helpers.answers(callee))
}

/// Whether any of these bodies compares what the program answered.
///
/// Each body is judged **on its own**. A name bound in one function says
/// nothing about a comparison in another, and a concatenation of two
/// functions' text would let the first function's names satisfy the second
/// function's comparison — which is a false "not vacuous" of exactly the kind
/// this scan exists to remove.
fn asserts_something(bodies: &[Vec<char>], helpers: &Helpers) -> bool {
    for body in bodies {
        let derived = derived_names(body, helpers);
        for context in comparison_contexts(body) {
            if mentions_the_answer(&context, &derived, helpers) {
                return true;
            }
        }
    }
    false
}

/// The vacuous tests in one file: (name, line).
fn scan_file(path: &Path, modules: &[(String, HashMap<String, Entry>)]) -> Vec<(String, usize)> {
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let raw: Vec<char> = raw.chars().collect();
    let code = mask(&mask(&raw, true), false);
    let words = mask(&raw, false);

    let functions = find_functions(&code);
    let local: HashMap<String, Entry> = functions
        .iter()
        .map(|function| {
            (
                function.name.clone(),
                (
                    code[function.brace..function.end].to_vec(),
                    words[function.brace..function.end].to_vec(),
                ),
            )
        })
        .collect();
    let helpers = Helpers {
        local,
        modules: modules.to_vec(),
    };

    let mut vacuous = Vec::new();
    for function in &functions {
        if !is_a_test(&code, function.start) {
            continue;
        }
        let names = reached(&helpers, &function.name, &mut HashSet::new());
        let bodies: Vec<Vec<char>> = names
            .iter()
            .map(|name| helpers.body_of(name).to_vec())
            .collect();
        if !bodies
            .iter()
            .any(|body| RUNS_A_PROGRAM.iter().any(|token| contains(body, token)))
        {
            continue;
        }
        if asserts_something(&bodies, &helpers) {
            continue;
        }
        let fixtures: Vec<char> = names
            .iter()
            .flat_map(|name| helpers.fixture_of(name).to_vec())
            .collect();
        if redblue_assertion(&fixtures) {
            continue;
        }
        vacuous.push((function.name.clone(), function.line));
    }
    vacuous
}

/// Whether the function at `start` carries the `#[test]` attribute.
fn is_a_test(code: &[char], start: usize) -> bool {
    let before = &code[..start];
    let attributes = match before.iter().rposition(|c| *c == '}') {
        Some(index) => &before[index + 1..],
        None => before,
    };
    attributes
        .windows(7)
        .any(|window| window == ['#', '[', 't', 'e', 's', 't', ']'])
}

/// A Redblue-level `expect ... to be` or `expect ... to fail`.
fn redblue_assertion(text: &[char]) -> bool {
    let mut i = 0;
    while i < text.len() {
        if !is_ident_start(text[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let end = identifier_end(text, i);
        let is_expect = text[start..end] == ['e', 'x', 'p', 'e', 'c', 't'];
        i = end;
        if !is_expect {
            continue;
        }
        let line_end = text[start..]
            .iter()
            .position(|c| *c == '\n')
            .map(|offset| start + offset)
            .unwrap_or(text.len());
        let line = &text[start..line_end];
        if has_word_pair(line, "to", "be") || has_word_pair(line, "to", "fail") {
            return true;
        }
    }
    false
}

/// Whether the text holds the two words, in order, each on a word boundary.
///
/// `expect x to be y` holds the pair `("to", "be")`: two words separated by
/// whitespace, each standing on its own.
fn has_word_pair(text: &[char], first: &str, second: &str) -> bool {
    let first: Vec<char> = first.chars().collect();
    let second: Vec<char> = second.chars().collect();
    let mut from = 0;
    while let Some(offset) = find_sub(&text[from..], &first) {
        let at = from + offset;
        from = at + 1;
        let before_ok = at == 0 || !is_ident_continue(text[at - 1]);
        if !before_ok {
            continue;
        }
        let mut after = at + first.len();
        if after < text.len() && is_ident_continue(text[after]) {
            continue;
        }
        while after < text.len() && text[after].is_whitespace() {
            after += 1;
        }
        if after + second.len() > text.len() || text[after..after + second.len()] != second[..] {
            continue;
        }
        let end = after + second.len();
        if end < text.len() && is_ident_continue(text[end]) {
            continue;
        }
        return true;
    }
    false
}

/// The function a call names, and every helper it calls.
fn reached(helpers: &Helpers, name: &str, seen: &mut HashSet<String>) -> Vec<String> {
    if !seen.insert(name.to_string()) {
        return Vec::new();
    }
    let mut names = vec![name.to_string()];
    let body = helpers.body_of(name);
    let mut callees: Vec<String> = called_names(body).into_iter().collect();
    callees.extend(called_paths(body));
    for callee in callees {
        names.extend(reached(helpers, &callee, seen));
    }
    names
}

/// The `let` bindings in a body: the names, and what each is bound to.
fn let_bindings(code: &[char]) -> Vec<(HashSet<String>, Vec<char>)> {
    let mut found = Vec::new();
    let mut i = 0;
    while i < code.len() {
        if !is_ident_start(code[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let end = identifier_end(code, start);
        let is_let = code[start..end] == ['l', 'e', 't'];
        i = end;
        if !is_let {
            continue;
        }
        let mut after = end;
        while after < code.len() && code[after].is_whitespace() {
            after += 1;
        }
        if after == end {
            // `let` has to be followed by whitespace to bind anything.
            continue;
        }
        let Some(pattern_end) = pattern_end(code, after) else {
            continue;
        };
        let Some(equals) = (pattern_end..code.len()).find(|&k| code[k] == '=') else {
            continue;
        };
        let right_start = equals + 1;
        let right = code[right_start..statement_end(code, right_start)].to_vec();
        let names: HashSet<String> = names_in(&code[after..pattern_end])
            .into_iter()
            .filter(|name| !NOT_NAMES.contains(&name.as_str()))
            .collect();
        found.push((names, right));
    }
    found
}

/// The end of a `let` or `for` pattern: a parenthesised group, or a path.
fn pattern_end(code: &[char], start: usize) -> Option<usize> {
    if start >= code.len() {
        return None;
    }
    if code[start] == '(' {
        let mut depth = 0i64;
        for (offset, ch) in code[start..].iter().enumerate() {
            if *ch == '(' {
                depth += 1;
            } else if *ch == ')' {
                depth -= 1;
                if depth == 0 {
                    return Some(start + offset + 1);
                }
            }
        }
        return None;
    }
    if !is_ident_start(code[start]) {
        return None;
    }
    let mut end = identifier_end(code, start);
    while end + 1 < code.len() && code[end] == ':' && code[end + 1] == ':' {
        let next = end + 2;
        if next >= code.len() || !is_ident_start(code[next]) {
            break;
        }
        end = identifier_end(code, next);
    }
    if end < code.len() && code[end] == '(' {
        let mut depth = 0i64;
        for (offset, ch) in code[end..].iter().enumerate() {
            if *ch == '(' {
                depth += 1;
            } else if *ch == ')' {
                depth -= 1;
                if depth == 0 {
                    return Some(end + offset + 1);
                }
            }
        }
    }
    Some(end)
}

/// The `for` bindings in a body: the names, and what each is bound to.
fn for_bindings(code: &[char]) -> Vec<(HashSet<String>, Vec<char>)> {
    let mut found = Vec::new();
    let mut i = 0;
    while i < code.len() {
        if !is_ident_start(code[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let end = identifier_end(code, start);
        let is_for = code[start..end] == ['f', 'o', 'r'];
        i = end;
        if !is_for {
            continue;
        }
        let mut after = end;
        while after < code.len() && code[after].is_whitespace() {
            after += 1;
        }
        if after == end {
            continue;
        }
        let Some(pattern_end) = pattern_end(code, after) else {
            continue;
        };
        let mut in_at = pattern_end;
        while in_at < code.len() && code[in_at].is_whitespace() {
            in_at += 1;
        }
        if in_at == pattern_end {
            continue;
        }
        if in_at + 1 >= code.len() || code[in_at..in_at + 2] != ['i', 'n'] {
            continue;
        }
        if in_at + 2 < code.len() && is_ident_continue(code[in_at + 2]) {
            continue;
        }
        let right_start = in_at + 2;
        let right = code[right_start..statement_end(code, right_start)].to_vec();
        let names: HashSet<String> = names_in(&code[after..pattern_end])
            .into_iter()
            .filter(|name| !NOT_NAMES.contains(&name.as_str()))
            .collect();
        found.push((names, right));
    }
    found
}

/// The `match` arm bindings in a body: the names, and the value matched on.
fn match_bindings(code: &[char]) -> Vec<(HashSet<String>, Vec<char>)> {
    let mut found = Vec::new();
    let mut i = 0;
    while i < code.len() {
        if !is_ident_start(code[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let end = identifier_end(code, start);
        let is_match = code[start..end] == ['m', 'a', 't', 'c', 'h'];
        i = end;
        if !is_match {
            continue;
        }
        let Some(brace) = (end..code.len()).find(|&k| code[k] == '{') else {
            continue;
        };
        let close = match_brace(code, brace);
        let scrutinee = code[end..brace].to_vec();
        let region = &code[brace..close];
        let mut arm_start = 0usize;
        for k in 0..region.len() {
            if region[k] == '{' || region[k] == '}' {
                arm_start = k + 1;
                continue;
            }
            if region[k] == '=' && k + 1 < region.len() && region[k + 1] == '>' {
                let names: HashSet<String> = names_in(&region[arm_start..k])
                    .into_iter()
                    .filter(|name| !NOT_NAMES.contains(&name.as_str()))
                    .collect();
                found.push((names, scrutinee.clone()));
                arm_start = k + 2;
            }
        }
    }
    found
}

/// The plain assignments in a body: the names, and what each is set to.
fn assignment_bindings(code: &[char]) -> Vec<(HashSet<String>, Vec<char>)> {
    let mut found = Vec::new();
    let mut i = 0;
    while i < code.len() {
        if !is_ident_start(code[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let end = identifier_end(code, start);
        // A name that follows a `.` is a field, not a binding.
        if start > 0 && (is_ident_continue(code[start - 1]) || code[start - 1] == '.') {
            i = end;
            continue;
        }
        let mut equals = end;
        while equals < code.len() && code[equals].is_whitespace() {
            equals += 1;
        }
        if equals >= code.len() || code[equals] != '=' {
            i = end;
            continue;
        }
        if equals + 1 >= code.len() || code[equals + 1] == '=' {
            i = end;
            continue;
        }
        let right_start = equals + 2;
        let right = code[right_start..statement_end(code, right_start)].to_vec();
        found.push((HashSet::from([code[start..end].iter().collect()]), right));
        i = end;
    }
    found
}

/// The files the scan reads: the `*.rs` files at the top level of `root`.
fn scan_files(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect();
    files.sort();
    files
}

/// The vacuous tests under `root`.
pub fn scan_directory(root: &Path) -> Scan {
    let files = scan_files(root);
    let modules = load_modules(root);
    let mut named = Vec::new();
    for path in &files {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        for (test, line) in scan_file(path, &modules) {
            named.push((name.to_string(), line, test));
        }
    }
    Scan {
        files: files.len(),
        named,
    }
}

/// The file list the scan reads, one name per line.
pub fn list_files(root: &Path) -> Vec<String> {
    scan_files(root)
        .iter()
        .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
        .map(|name| name.to_string())
        .collect()
}

/// The command line's work, separated from `main` so a test can drive it.
///
/// It returns the text the command prints, so the test can check the output
/// the report quotes without spawning a process.
pub fn run(args: &[String]) -> Result<String, String> {
    let list_only = args.first().is_some_and(|arg| arg == "--list");
    let roots: &[String] = if list_only { &args[1..] } else { args };
    if roots.len() != 1 {
        return Err("usage: scan [--list] <directory of tests/*.rs>".to_string());
    }
    let root = Path::new(&roots[0]);
    if list_only {
        return Ok(list_files(root).join("\n"));
    }
    Ok(scan_directory(root).render(root))
}

/// The command line.
#[cfg(not(test))]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(output) => print!("{output}"),
        Err(usage) => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    }
}
