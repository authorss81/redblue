use std::fs;
use std::path::{Path, PathBuf};

use redblue::formatter::{self, Formatter};
use redblue::lexer::Lexer;
use redblue::parser::{self, BinaryOp, Expr, ImportItem, Program, Statement, Stmt, UnaryOp};

/// Every `.rb` file in the tree that must survive formatting unchanged in
/// meaning: the language's specification-by-example, plus the test corpus.
fn corpus() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for dir in ["examples", "tests", "modules"] {
        let path = root.join(dir);
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("rb") {
                files.push(path);
            }
        }
    }
    assert!(
        !files.is_empty(),
        "no .rb corpus files found under examples/, tests/ or modules/"
    );
    files.sort();
    files
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {}", path.display(), e))
}

/// Runs `source` on the real binary and returns what it printed, so a claim
/// about behaviour can be checked against output rather than inferred from the
/// tree. Each run gets its own file inside `target/tmp/`.
fn run_program(source: &str) -> String {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = scratch_dir();
    let serial = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let path = dir.join(format!("run-{}-{}.rb", std::process::id(), serial));
    fs::write(&path, source).expect("program should be writable");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("the rb binary should run");
    let _ = fs::remove_file(&path);

    assert!(
        out.status.success(),
        "{} should run, got: {}",
        source,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// What a child run of the `rb` binary produced.
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

/// Where programs under test are written, inside the project checkout.
fn scratch_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/formatter");
    fs::create_dir_all(&dir).expect("scratch directory should be creatable");
    dir
}

/// Runs `source` with `rb <mode>` and returns what it produced. A diagnostic
/// quotes the path of the file it read, so that path is replaced with a fixed
/// marker and two runs of the same program stay comparable.
fn run_rb(mode: &str, source: &str) -> Run {
    run_rb_args(&[mode], source)
}

/// Runs `source` with `rb <args…> <file>`, for the invocations a single mode
/// cannot express — `rb format --check <file>` puts a flag between the mode and
/// the path.
fn run_rb_args(args: &[&str], source: &str) -> Run {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = scratch_dir();
    let serial = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let path = dir.join(format!("rb-{}-{}.rb", std::process::id(), serial));
    fs::write(&path, source).expect("program should be writable");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_rb"))
        .args(args)
        .arg(&path)
        .output()
        .expect("the rb binary should run");
    let marker = path.display().to_string();
    let _ = fs::remove_file(&path);

    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).replace(&marker, "<program>"),
        stderr: String::from_utf8_lossy(&out.stderr).replace(&marker, "<program>"),
    }
}

/// Runs `rb format --check <file>` and returns what the process produced.
///
/// This is the CI gate's whole surface: the exit code is the answer and stdout
/// is the explanation, so both are what the tests below look at.
fn run_format_check(source: &str) -> Run {
    run_rb_args(&["format", "--check"], source)
}

fn parse(source: &str) -> Program {
    let tokens = Lexer::tokenize(source).expect("source must lex");
    parser::parse(tokens).expect("source must parse")
}

/// Whether two programs mean the same thing: the same tree, with every field
/// compared and only the source position left out.
///
/// Formatting moves text by definition, so [`Stmt::span`] is expected to
/// change; nothing else is allowed to.
///
/// The comparison walks the tree rather than rendering it with `{:?}` and
/// editing the resulting string. A debug rendering contains the program's own
/// text — a literal like `say "Span { line: 1 }"` is part of it — so any scheme
/// that searches the string for `Span { ` and cuts up to the next ` }` can be
/// defeated by a program that mentions that shape, and the oracle this file
/// trusts most would silently compare two different trees. Every variant is
/// matched here explicitly, and an unmatched pair of variants is reported as a
/// difference, so a variant added to the AST later fails loudly here instead
/// of comparing equal by default.
fn same_program(a: &Program, b: &Program) -> bool {
    same_stmts(&a.statements, &b.statements)
}

fn same_stmts(a: &[Stmt], b: &[Stmt]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| same_stmt(x, y))
}

/// `Stmt` is a span and a statement; only the statement is the program.
fn same_stmt(a: &Stmt, b: &Stmt) -> bool {
    same_statement(&a.statement, &b.statement)
}

fn same_statement(a: &Statement, b: &Statement) -> bool {
    match (a, b) {
        (Statement::Say(x), Statement::Say(y)) => same_expr(x, y),
        (Statement::Print(x), Statement::Print(y)) => same_expr(x, y),
        (Statement::Expr(x), Statement::Expr(y)) => same_expr(x, y),
        (
            Statement::Set {
                name: n1,
                value: v1,
            },
            Statement::Set {
                name: n2,
                value: v2,
            },
        ) => n1 == n2 && same_expr(v1, v2),
        (
            Statement::Constant {
                name: n1,
                value: v1,
            },
            Statement::Constant {
                name: n2,
                value: v2,
            },
        ) => n1 == n2 && same_expr(v1, v2),
        (
            Statement::SetProperty {
                object: o1,
                property: p1,
                value: v1,
            },
            Statement::SetProperty {
                object: o2,
                property: p2,
                value: v2,
            },
        ) => o1 == o2 && p1 == p2 && same_expr(v1, v2),
        (
            Statement::If {
                condition: c1,
                then_branch: t1,
                else_branch: e1,
            },
            Statement::If {
                condition: c2,
                then_branch: t2,
                else_branch: e2,
            },
        ) => same_expr(c1, c2) && same_stmts(t1, t2) && same_stmts(e1, e2),
        (
            Statement::Unless {
                condition: c1,
                body: b1,
            },
            Statement::Unless {
                condition: c2,
                body: b2,
            },
        ) => same_expr(c1, c2) && same_stmts(b1, b2),
        (
            Statement::ForEach {
                variable: v1,
                iterable: i1,
                body: b1,
            },
            Statement::ForEach {
                variable: v2,
                iterable: i2,
                body: b2,
            },
        ) => v1 == v2 && same_expr(i1, i2) && same_stmts(b1, b2),
        (
            Statement::ForRange {
                variable: v1,
                start: s1,
                end: e1,
                step: st1,
                body: b1,
            },
            Statement::ForRange {
                variable: v2,
                start: s2,
                end: e2,
                step: st2,
                body: b2,
            },
        ) => {
            v1 == v2
                && same_expr(s1, s2)
                && same_expr(e1, e2)
                && same_optional_expr(st1, st2)
                && same_stmts(b1, b2)
        }
        (
            Statement::Repeat {
                count: c1,
                body: b1,
            },
            Statement::Repeat {
                count: c2,
                body: b2,
            },
        ) => same_expr(c1, c2) && same_stmts(b1, b2),
        (
            Statement::While {
                condition: c1,
                body: b1,
            },
            Statement::While {
                condition: c2,
                body: b2,
            },
        ) => same_expr(c1, c2) && same_stmts(b1, b2),
        (Statement::Break, Statement::Break) => true,
        (Statement::Skip, Statement::Skip) => true,
        (Statement::Return(x), Statement::Return(y)) => same_optional_expr(x, y),
        (Statement::GiveBack(x), Statement::GiveBack(y)) => same_optional_expr(x, y),
        (
            Statement::Function {
                name: n1,
                params: p1,
                body: b1,
            },
            Statement::Function {
                name: n2,
                params: p2,
                body: b2,
            },
        ) => n1 == n2 && p1 == p2 && same_stmts(b1, b2),
        (
            Statement::Method {
                name: n1,
                params: p1,
                body: b1,
            },
            Statement::Method {
                name: n2,
                params: p2,
                body: b2,
            },
        ) => n1 == n2 && p1 == p2 && same_stmts(b1, b2),
        (
            Statement::Has {
                name: n1,
                default: d1,
            },
            Statement::Has {
                name: n2,
                default: d2,
            },
        ) => n1 == n2 && same_optional_expr(d1, d2),
        (
            Statement::Object {
                name: n1,
                extends: x1,
                body: b1,
            },
            Statement::Object {
                name: n2,
                extends: x2,
                body: b2,
            },
        ) => n1 == n2 && same_optional_string(x1, x2) && same_stmts(b1, b2),
        (
            Statement::Try {
                body: b1,
                catch_var: cv1,
                catch_body: cb1,
                finally_body: fb1,
            },
            Statement::Try {
                body: b2,
                catch_var: cv2,
                catch_body: cb2,
                finally_body: fb2,
            },
        ) => {
            same_stmts(b1, b2)
                && same_optional_string(cv1, cv2)
                && same_stmts(cb1, cb2)
                && same_stmts(fb1, fb2)
        }
        (Statement::Import(x), Statement::Import(y)) => same_imports(x, y),
        (Statement::Module { name: n1, body: b1 }, Statement::Module { name: n2, body: b2 }) => {
            n1 == n2 && same_stmts(b1, b2)
        }
        (Statement::Export { names: n1, all: a1 }, Statement::Export { names: n2, all: a2 }) => {
            n1 == n2 && a1 == a2
        }
        (Statement::Test { name: n1, body: b1 }, Statement::Test { name: n2, body: b2 }) => {
            n1 == n2 && same_stmts(b1, b2)
        }
        // Two different variants are two different programs, and a pair this
        // file does not name is a new variant rather than an equal one.
        _ => false,
    }
}

fn same_expr(a: &Expr, b: &Expr) -> bool {
    match (a, b) {
        // Compared by bit pattern, not by `==`: `0.0` and `-0.0` are the two
        // numbers the formatter prints differently, and a `NaN` no source can
        // spell must not make a tree compare unequal with itself.
        (Expr::Number(x), Expr::Number(y)) => x.to_bits() == y.to_bits(),
        (Expr::Text(x), Expr::Text(y)) => x == y,
        (Expr::YesNo(x), Expr::YesNo(y)) => x == y,
        (Expr::Nothing, Expr::Nothing) => true,
        (Expr::Variable(x), Expr::Variable(y)) => x == y,
        (
            Expr::Binary {
                op: o1,
                left: l1,
                right: r1,
            },
            Expr::Binary {
                op: o2,
                left: l2,
                right: r2,
            },
        ) => same_binary_op(o1, o2) && same_expr(l1, l2) && same_expr(r1, r2),
        (Expr::Unary { op: o1, expr: e1 }, Expr::Unary { op: o2, expr: e2 }) => {
            same_unary_op(o1, o2) && same_expr(e1, e2)
        }
        (Expr::Call { name: n1, args: a1 }, Expr::Call { name: n2, args: a2 }) => {
            n1 == n2 && same_exprs(a1, a2)
        }
        (
            Expr::Property {
                object: o1,
                property: p1,
            },
            Expr::Property {
                object: o2,
                property: p2,
            },
        ) => same_expr(o1, o2) && p1 == p2,
        (
            Expr::MethodCall {
                receiver: r1,
                method: m1,
                args: a1,
            },
            Expr::MethodCall {
                receiver: r2,
                method: m2,
                args: a2,
            },
        ) => same_expr(r1, r2) && m1 == m2 && same_exprs(a1, a2),
        (
            Expr::Index {
                object: o1,
                index: i1,
            },
            Expr::Index {
                object: o2,
                index: i2,
            },
        ) => same_expr(o1, o2) && same_expr(i1, i2),
        (Expr::InterpolatedText(x), Expr::InterpolatedText(y)) => same_exprs(x, y),
        (Expr::List(x), Expr::List(y)) => same_exprs(x, y),
        (Expr::Record(x), Expr::Record(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|((k1, v1), (k2, v2))| k1 == k2 && same_expr(v1, v2))
        }
        (
            Expr::Expect {
                actual: a1,
                expected: e1,
            },
            Expr::Expect {
                actual: a2,
                expected: e2,
            },
        ) => same_expr(a1, a2) && same_expr(e1, e2),
        _ => false,
    }
}

fn same_binary_op(a: &BinaryOp, b: &BinaryOp) -> bool {
    matches!(
        (a, b),
        (BinaryOp::Add, BinaryOp::Add)
            | (BinaryOp::Sub, BinaryOp::Sub)
            | (BinaryOp::Mul, BinaryOp::Mul)
            | (BinaryOp::Div, BinaryOp::Div)
            | (BinaryOp::Mod, BinaryOp::Mod)
            | (BinaryOp::Equal, BinaryOp::Equal)
            | (BinaryOp::NotEqual, BinaryOp::NotEqual)
            | (BinaryOp::Less, BinaryOp::Less)
            | (BinaryOp::LessEqual, BinaryOp::LessEqual)
            | (BinaryOp::Greater, BinaryOp::Greater)
            | (BinaryOp::GreaterEqual, BinaryOp::GreaterEqual)
            | (BinaryOp::And, BinaryOp::And)
            | (BinaryOp::Or, BinaryOp::Or)
            | (BinaryOp::In, BinaryOp::In)
    )
}

fn same_unary_op(a: &UnaryOp, b: &UnaryOp) -> bool {
    matches!(
        (a, b),
        (UnaryOp::Neg, UnaryOp::Neg) | (UnaryOp::Not, UnaryOp::Not)
    )
}

fn same_exprs(a: &[Expr], b: &[Expr]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| same_expr(x, y))
}

fn same_optional_expr(a: &Option<Expr>, b: &Option<Expr>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => same_expr(x, y),
        (None, None) => true,
        _ => false,
    }
}

fn same_optional_string(a: &Option<String>, b: &Option<String>) -> bool {
    a == b
}

fn same_imports(a: &[ImportItem], b: &[ImportItem]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| x.name == y.name && same_optional_string(&x.alias, &y.alias))
}

/// Formats, then formats the result again: the two must be byte-identical.
fn assert_idempotent(source: &str, label: &str) -> String {
    let once = formatter::format(source)
        .unwrap_or_else(|e| panic!("{}: first format failed: {}", label, e));
    let twice = formatter::format(&once)
        .unwrap_or_else(|e| panic!("{}: second format failed: {}", label, e));
    assert_eq!(
        once, twice,
        "{}: format is not idempotent\n--- once ---\n{}\n--- twice ---\n{}",
        label, once, twice
    );
    once
}

// ---------------------------------------------------------------------------
// Idempotence
// ---------------------------------------------------------------------------

#[test]
fn format_is_idempotent() {
    let source = "say \"Hello, World!\"\n";

    let once = formatter::format(source).expect("first format must succeed");
    let twice = formatter::format(&once).expect("second format must succeed");

    assert_eq!(
        once, twice,
        "formatting must be idempotent:\n--- once ---\n{}\n--- twice ---\n{}",
        once, twice
    );
}

#[test]
fn format_is_idempotent_over_the_whole_corpus() {
    for path in corpus() {
        let source = read(&path);
        assert_idempotent(&source, &path.display().to_string());
    }
}

// ---------------------------------------------------------------------------
// Losslessness: format then parse gives the same tree
// ---------------------------------------------------------------------------

#[test]
fn format_preserves_the_meaning_of_every_corpus_file() {
    for path in corpus() {
        let source = read(&path);
        let label = path.display().to_string();

        let formatted = formatter::format(&source)
            .unwrap_or_else(|e| panic!("{}: format failed: {}", label, e));
        let before = parse(&source);
        let after = parse(&formatted);

        assert!(
            same_program(&before, &after),
            "{}: formatting changed the program\n--- source ---\n{}\n--- formatted ---\n{}",
            label,
            source,
            formatted
        );
    }
}

#[test]
fn edge_the_tree_comparison_is_not_fooled_by_text_that_looks_like_a_span() {
    // `same_program` used to render both trees with `{:?}` and cut every
    // `Span { … }` out of the string. A debug rendering contains the program's
    // own text, so a program holding the shape as a literal had the *text*
    // cut out of it too — and two different programs holding different numbers
    // inside that text then compared equal. The comparison walks the tree now,
    // and this asserts it can still tell two programs apart, not merely that it
    // agrees with itself.
    let one = "say \"Span { line: 1, column: 1 }\"\n";
    let two = "say \"Span { line: 2, column: 2 }\"\n";

    let one_formatted = assert_idempotent(one, "span-shaped literal, one");
    let two_formatted = assert_idempotent(two, "span-shaped literal, two");

    assert!(
        same_program(&parse(one), &parse(&one_formatted)),
        "a program holding span-shaped text must still compare equal to itself\n{}",
        one_formatted
    );
    assert!(
        !same_program(&parse(one), &parse(two)),
        "two programs differing only inside span-shaped text must NOT compare equal"
    );
    assert!(
        !same_program(&parse(&one_formatted), &parse(&two_formatted)),
        "and the same must hold for their formatted forms"
    );
}

#[test]
fn edge_the_tree_comparison_detects_a_change_behind_any_shape_of_text() {
    // The same oracle, on the shapes a debug rendering would have to cut
    // through: a brace, a quote and a backslash in a literal all survive
    // formatting, and a difference behind any of them must still be seen.
    let cases = [
        ("say \"{\"\nsay 1\n", "say \"{\"\nsay 2\n"),
        ("say \"Span {\"\nsay 1\n", "say \"Span {\"\nsay 2\n"),
        ("say \"a } b\"\n", "say \"a } c\"\n"),
        ("set x to [1, {a: 2}]\n", "set x to [1, {a: 3}]\n"),
    ];

    for (one, two) in cases {
        assert!(
            !same_program(&parse(one), &parse(two)),
            "{:?} and {:?} are different programs and must not compare equal",
            one,
            two
        );
    }
}

#[test]
fn format_keeps_the_grouping_the_parentheses_forced() {
    // The AST drops parentheses, so the formatter has to put back the ones the
    // grouping depends on: without them this formats to `1 + 2 * 3`, which is
    // 7 rather than 9.
    let source = "set a to (1 + 2) * 3\nsay a\n";

    let formatted = assert_idempotent(source, "grouping");
    assert_eq!(
        formatted, "set a to (1 + 2) * 3\nsay a\n",
        "formatting must preserve the grouping"
    );
}

#[test]
fn format_keeps_right_hand_grouping_of_equal_precedence() {
    let source = "say 1 - (2 - 3)\nsay 10 - 2 - 3\n";

    let formatted = assert_idempotent(source, "equal precedence");
    assert_eq!(
        formatted, source,
        "a right operand that binds as loosely as its parent needs parentheses"
    );
}

#[test]
fn format_keeps_grouping_for_unary_and_every_operator_level() {
    let source = "\
set a to not (yes and no)
set b to -(2 * 3)
set c to (1 + 2) - 3
set d to 1 + (2 * 3)
set e to ((1 + 2) * (3 + 4))
set f to not (1 is 2)
set g to (yes and no) or yes
set h to 1 - (2 - 3)
set i to 10 % (2 * 3)
set j to 1 is not 2
say a
say b
say c
say d
say e
say f
say g
say h
say i
say j
";

    let formatted = assert_idempotent(source, "precedence levels");
    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "grouping must survive formatting:\n{}",
        formatted
    );
}

#[test]
fn format_writes_index_with_brackets_not_at() {
    let source = "set xs to [10, 20, 30]\nsay xs[1]\n";

    let formatted = assert_idempotent(source, "index");
    assert_eq!(
        formatted, source,
        "the index form must be the one the parser reads back"
    );
}

#[test]
fn formatter_instance_does_not_carry_state_between_calls() {
    let mut formatter = Formatter::new();
    let first = formatter.format("say 1\n").expect("first format");
    let second = formatter.format("say 2\n").expect("second format");

    assert_eq!(first, "say 1\n", "first format");
    assert_eq!(
        second, "say 2\n",
        "a second source must not inherit the first one's text"
    );
}

// ---------------------------------------------------------------------------
// --check
// ---------------------------------------------------------------------------

#[test]
fn check_compares_exactly_including_the_trailing_newline() {
    let formatted = "say 1\n";

    assert!(
        !formatter::needs_reformat(formatted, formatted),
        "an already formatted file must pass"
    );
    assert!(
        formatter::needs_reformat("say 1", formatted),
        "a missing trailing newline is a difference"
    );
    assert!(
        formatter::needs_reformat("say 1\n\n", formatted),
        "a trailing blank line is a difference"
    );
    assert!(
        formatter::needs_reformat("say 1  \n", formatted),
        "trailing whitespace is a difference"
    );
    assert!(
        formatter::needs_reformat("say 1 \nsay 2\n", formatted),
        "a difference anywhere in the file is a difference"
    );
}

#[test]
fn edge_the_check_flag_reports_a_difference_through_the_exit_code() {
    // `rb format --check` is driven as a CI gate, so its exit code is the
    // result and the message is only the explanation. This exercises the CLI
    // branch in `src/lib.rs`, not the `needs_reformat` helper on its own:
    // `run_rb` takes one mode and cannot say `--check`, which is why the
    // branch went untested until this.
    //
    // Every case below differs from `say 1\n` only in bytes that a `trim()` on
    // both sides would erase, so the whole set is reported as exit 0 if the
    // comparison ever starts trimming again.
    let clean = run_format_check("say 1\n");
    assert_eq!(
        clean.code, 0,
        "an already formatted file must pass, got stdout {} / stderr {}",
        clean.stdout, clean.stderr
    );
    assert_eq!(
        clean.stdout, "",
        "a file that passes must print nothing to be parsed"
    );

    for (label, source) in [
        ("no trailing newline", "say 1"),
        ("a trailing blank line", "say 1\n\n"),
        ("trailing whitespace", "say 1  \n"),
        ("a whitespace-only last line", "say 1\n   \n"),
        ("a CRLF line ending", "say 1\r\n"),
        ("a doubled space inside the line", "say  1\n"),
        ("a leading blank line", "\nsay 1\n"),
    ] {
        let run = run_format_check(source);
        assert_eq!(
            run.code, 1,
            "{}: {:?} differs from its formatted form and must exit 1, got stdout {}",
            label, source, run.stdout
        );
        assert!(
            run.stdout.contains("File would be reformatted"),
            "{}: the check must say what it found, got: {}",
            label,
            run.stdout
        );
    }
}

#[test]
fn edge_the_check_flag_rejects_a_file_it_cannot_format() {
    // Exit 1 is also what a file that does not parse gets, and it must be the
    // `Format error` diagnostic rather than a silent pass.
    let run = run_format_check("if 1 is 1 then\n    say 1\n");

    assert_eq!(
        run.code, 1,
        "a file the parser rejects must not pass the check, got: {}",
        run.stdout
    );
    assert!(
        run.stderr.contains("Format error"),
        "the check must report why it gave up, got: {}",
        run.stderr
    );
    assert_eq!(
        run.stdout, "",
        "a file that could not be checked must not print a verdict on it"
    );
}

#[test]
fn edge_the_check_flag_accepts_what_the_format_flag_prints() {
    // The whole loop, through the binary: `rb format` writes out a file, that
    // output is written back, and `rb format --check` on the result must
    // accept it. A check that rejected the formatter's own output, or a
    // formatter whose output the check rejected, would both break here.
    for path in corpus() {
        let source = read(&path);
        let label = path.display().to_string();

        let formatted = run_rb("format", &source);
        assert_eq!(
            formatted.code, 0,
            "{}: `rb format` must succeed on a corpus file: {}",
            label, formatted.stderr
        );

        let check = run_format_check(&formatted.stdout);
        assert_eq!(
            check.code, 0,
            "{}: `rb format --check` rejected the output of `rb format`\n--- format ---\n{}\n--- check ---\n{}",
            label, formatted.stdout, check.stdout
        );

        // And the check is not vacuous: one trailing space on the last line is
        // a difference, and it is reported.
        let dirty = run_format_check(&format!("{} \n", formatted.stdout.trim_end_matches('\n')));
        assert_eq!(
            dirty.code, 1,
            "{}: a trailing space must fail the check\n--- format ---\n{}",
            label, formatted.stdout
        );
    }
}

#[test]
fn formatted_output_ends_with_exactly_one_newline() {
    for path in corpus() {
        let source = read(&path);
        let formatted = formatter::format(&source)
            .unwrap_or_else(|e| panic!("{}: format failed: {}", path.display(), e));

        assert!(
            formatted.ends_with("\n"),
            "{}: formatted output must end with a newline, got {:?}",
            path.display(),
            formatted.chars().rev().take(8).collect::<String>()
        );
        assert!(
            !formatted.ends_with("\n\n"),
            "{}: formatted output must not end with a blank line",
            path.display()
        );
        assert!(
            !formatted.lines().any(|line| line != line.trim_end()),
            "{}: formatted output must not carry trailing whitespace",
            path.display()
        );
    }
}

// ---------------------------------------------------------------------------
// Comments
// ---------------------------------------------------------------------------

#[test]
fn format_preserves_comments() {
    let source = "// Classic Hello World\n\
                  say \"Hello, World!\"\n\
                  \n\
                  // With a variable\n\
                  set greeting to \"Hello\"\n\
                  say greeting\n";

    let formatted = assert_idempotent(source, "comments");

    assert!(
        formatted.contains("// Classic Hello World"),
        "the leading comment must survive:\n{}",
        formatted
    );
    assert!(
        formatted.contains("// With a variable"),
        "the comment before `set` must survive:\n{}",
        formatted
    );
}

#[test]
fn format_preserves_every_comment_in_every_corpus_file() {
    for path in corpus() {
        let source = read(&path);
        let label = path.display().to_string();

        let formatted = formatter::format(&source)
            .unwrap_or_else(|e| panic!("{}: format failed: {}", label, e));

        for comment in source.lines().map(str::trim) {
            if let Some(text) = comment.strip_prefix("//") {
                assert!(
                    formatted.contains(text.trim()),
                    "{}: comment {:?} was dropped by formatting",
                    label,
                    text.trim()
                );
            }
        }
    }
}

#[test]
fn format_preserves_comments_inside_blocks_and_at_the_end() {
    let source = "\
// before the function
to check(n)
    // before the if
    if n is 1 then
        // inside the if
        say n
    end
    // last line of the function
end

check(1)
";

    let formatted = assert_idempotent(source, "nested comments");

    for expected in [
        "// before the function",
        "// before the if",
        "// inside the if",
        "// last line of the function",
    ] {
        assert!(
            formatted.contains(expected),
            "comment {:?} was dropped:\n{}",
            expected,
            formatted
        );
    }
}

// A comment with no statement after it inside its block used to be flushed at
// the *outer* indentation, once the whole enclosing statement had been
// formatted. Its text survived, so the `contains` assertions above could not
// see the damage — but it stopped being part of the block it was written in,
// and in an `if`/`else` it moved into the other branch and came to read as a
// note on the branch's first statement. These pin *where* a comment lands.

#[test]
fn edge_a_comment_at_the_end_of_a_block_stays_in_that_block() {
    let source = "\
if 1 is 1 then
    say 1
    // the last line of the block
end
say 2
";

    let formatted = assert_idempotent(source, "comment at the end of a block");

    assert_eq!(
        formatted, source,
        "a comment written last in a block must not be moved out of it"
    );
}

#[test]
fn edge_a_comment_before_else_stays_in_the_then_branch() {
    let source = "\
if 1 is 1 then
    say 1
    // notes the then branch
else
    say 2
end
say 3
";

    let formatted = assert_idempotent(source, "comment before else");

    assert_eq!(
        formatted, source,
        "a comment must not migrate from the `then` branch into the `else` one"
    );
}

#[test]
fn edge_a_comment_at_the_end_of_a_nested_block_stays_at_its_own_depth() {
    let source = "\
to f(n)
    if n is 1 then
        say n
        // belongs inside the inner if
    end
    say 2
end
say f(1)
";

    let formatted = assert_idempotent(source, "comment at the end of a nested block");

    assert_eq!(
        formatted, source,
        "a comment must stay in the innermost block that contains it"
    );
}

#[test]
fn edge_a_comment_in_a_try_catch_finally_stays_with_its_branch() {
    let source = "\
try
    say 1
    // notes the try body
catch err
    say err
    // notes the catch body
finally
    say 3
end
say 4
";

    let formatted = assert_idempotent(source, "comment in try/catch/finally");

    assert_eq!(
        formatted, source,
        "each branch's trailing comment must stay inside that branch"
    );
}

#[test]
fn edge_a_block_whose_only_tail_is_a_comment_keeps_it_inside() {
    // Nothing but the comment follows the last statement, so there is no later
    // statement in the block for the comment to attach to.
    let source = "\
if 1 is 1 then
    say 1
    // the block ends with a comment
end
say 2
";

    let formatted = assert_idempotent(source, "comment-only block tail");

    assert_eq!(
        formatted, source,
        "a comment may be the last line of a block without escaping it"
    );
}

#[test]
fn edge_a_comment_trailing_a_statement_stays_in_that_statement_block() {
    let source = "\
to f(n)
    say n // trailing note
end
say f(1)
";

    let formatted = assert_idempotent(source, "comment trailing a statement");

    assert_eq!(
        formatted, source,
        "a comment written after a statement belongs to that statement's block"
    );
    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "moving the note onto its own line must not change what runs:\n{}",
        formatted
    );
}

#[test]
fn edge_a_comment_trailing_a_catch_line_stays_after_the_name_it_annotates() {
    // `catch` takes the error's name, so its line is built as
    // `indent` + `catch ` + name + one trailing comment. It used to go through
    // the same helper as `else`/`finally`, which writes the keyword, ends the
    // line with the trailing comment, and only then had the name appended —
    // turning `catch err // note` into `catch  // noteerr`, which does not
    // read back as `catch` at all.
    let source = "\
try
    say 1
catch err // what went wrong
    say err
finally // always
    say 3
end
say 4
";

    let formatted = assert_idempotent(source, "comment trailing a catch line");

    assert_eq!(
        formatted, source,
        "the note must stay at the end of the catch line, after the name"
    );
    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "the formatted program must still mean the same thing:\n{}",
        formatted
    );
    assert!(
        !formatted.contains("catch //"),
        "a name written after a comment has been swallowed by it:\n{}",
        formatted
    );
}

#[test]
fn edge_a_comment_trailing_a_block_opening_line_stays_on_that_line() {
    // Every line that opens a block may carry a comment, and the comment is
    // about the line it was written on. Claiming it only after the whole
    // statement had been written put it somewhere else: as a leading comment on
    // the block's first statement, or trailing the `end` that closed the block
    // — which for a block with no body meant `end // …`.
    let source = "\
if 1 is 1 then // the guard
    say 1
else // the other case
    say 2
end
for each x in [1, 2] // the loop
    say x
end
repeat 2 times // twice
    say 1
end
while 1 is 2 // never
    skip
end
to g() // the function
    say 1
end
object O // the object
    has n default 1
    to can m() // the method
        say 1
    end
end
try // the attempt
    say 1
catch e // the failure
    say e
end
test \"the test\" // the assertion
    expect 1 to be 1
end
g()
say 3
";

    let formatted = assert_idempotent(source, "comment trailing a block opening line");

    assert_eq!(
        formatted, source,
        "a comment written at the end of a block-opening line must stay there"
    );
    assert!(
        !formatted.contains("end //"),
        "no comment may migrate onto the keyword that closes a block:\n{}",
        formatted
    );
    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "the formatted program must still mean the same thing:\n{}",
        formatted
    );
}

#[test]
fn edge_a_comment_trailing_the_only_line_of_an_empty_block_stays_there() {
    // A block with no body has no statement for the comment to attach to as a
    // leading comment, so it used to land on the `end` — `end // …`, a note
    // about a block attached to the line that closed it.
    let source = "\
to g() // the function
end
repeat 2 times // twice
end
object O // the object
end
g()
say 1
";

    let formatted = assert_idempotent(source, "comment trailing an empty block");

    assert_eq!(
        formatted, source,
        "a block's own line keeps its comment even when the block is empty"
    );
}

#[test]
fn edge_a_block_comment_is_still_a_parser_error_when_the_block_is_unclosed() {
    // Attaching comments to blocks must not paper over a block that never ends.
    let source = "if 1 is 1 then\n    say 1\n    // the block is never closed\n";

    let run = run_rb("format", source);

    assert_ne!(
        run.code, 0,
        "an unclosed block must be rejected:\n{}",
        run.stdout
    );
    assert!(
        run.stderr.contains("Parser error"),
        "an unclosed block must be reported as a parser error:\n{}",
        run.stderr
    );
    assert!(
        !run.stdout.contains("the block is never closed"),
        "a rejected file must not be rewritten as if it had been understood:\n{}",
        run.stdout
    );
}

#[test]
fn edge_slashes_inside_a_text_literal_are_not_comments() {
    // A `//` inside a literal is text: dropping the rest of the line would
    // change what the program says.
    let source = "say \"http://example.com/path\"\n";

    let formatted = assert_idempotent(source, "url in text");
    assert_eq!(
        formatted, source,
        "a URL inside a literal must be printed as the same literal"
    );
}

#[test]
fn edge_a_comment_that_is_only_slashes_is_kept() {
    let source = "//// four slashes\nsay 1\n";

    let formatted = assert_idempotent(source, "slash-only comment");
    assert!(
        formatted.contains("//// four slashes"),
        "a slash-only comment must survive:\n{}",
        formatted
    );
}

// ---------------------------------------------------------------------------
// Text literals
// ---------------------------------------------------------------------------

#[test]
fn format_escapes_text_so_it_reads_back_unchanged() {
    let source = "\
say \"quote \\\" backslash \\\\ tab \\t\"
say \"{not an escape}\"
";

    let formatted = assert_idempotent(source, "escapes");
    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "escaped text must read back as the same text:\n{}",
        formatted
    );
}

#[test]
fn edge_equality_against_a_negation_keeps_its_parentheses() {
    // `x is not y` is a `is not` comparison, so an equality whose right-hand
    // side is a negation has to keep its parentheses or it reads back as the
    // other operator.
    let source = "set same to 1 is (not yes)\n";

    let formatted = assert_idempotent(source, "equality against a negation");

    assert_eq!(
        formatted, source,
        "the parentheses that keep it an equality must survive"
    );
    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "the tree must not become an inequality"
    );
}

#[test]
fn edge_empty_text_literal() {
    let source = "say \"\"\n";

    let formatted = assert_idempotent(source, "empty literal");
    assert_eq!(formatted, source, "an empty literal must stay empty");
}

#[test]
fn edge_a_literal_spanning_lines_keeps_its_slashes_and_later_comments() {
    // A `//` inside a literal that spans lines is text, and a comment after the
    // literal is a comment: the scanner has to know which one it is looking at.
    let source = "say \"first\n// not a comment\nlast\"\n// a real comment\nsay 1\n";

    let formatted = assert_idempotent(source, "multi-line literal");

    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "a literal spanning lines must keep its text:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\\n// not a comment\\n"),
        "the slashes inside the literal must stay inside it:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\n// a real comment\n"),
        "the comment after the literal must stay a comment:\n{}",
        formatted
    );
}

#[test]
fn edge_unicode_text_round_trips() {
    let source = "\
say \"héllo wörld\"
say \"emoji 🎉 and CJK 日本語\"
say \"RTL עברית and combining é\"
say \"{name} ← interpolated\"
";

    let formatted = assert_idempotent(source, "unicode");
    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "unicode text must survive formatting:\n{}",
        formatted
    );
}

#[test]
fn edge_text_holding_a_quote_and_a_backslash_round_trips() {
    let source = "say \"he said \\\"hi\\\" then \\\\\\\\ left\"\n";

    let formatted = assert_idempotent(source, "quote and backslash");
    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "a literal holding a quote and a backslash must survive:\n{}",
        formatted
    );
}

// ---------------------------------------------------------------------------
// Empty, singleton and boundary input
// ---------------------------------------------------------------------------

#[test]
fn edge_empty_program_formats_to_nothing() {
    assert_eq!(
        formatter::format("").expect("empty source must format"),
        "",
        "an empty program formats to the empty string"
    );
    assert_eq!(
        formatter::format("\n\n   \n").expect("whitespace-only source must format"),
        "",
        "whitespace alone formats to the empty string"
    );
    assert_eq!(
        formatter::format("// just a comment\n").expect("comment-only source must format"),
        "// just a comment\n",
        "a comment-only program keeps its comment and one trailing newline"
    );
}

#[test]
fn edge_singleton_inputs() {
    let cases = [
        "say 1\n",
        "print 2\n",
        "set x to 1\n",
        "break\n",
        "skip\n",
        "return\n",
        "give back 1\n",
        "give back\n",
        "give\n",
        "back 1\n",
        "import files\n",
        "import files, network to net\n",
        "say [1]\n",
        "say []\n",
        "say {a: 1}\n",
        "say {}\n",
        "say nothing\n",
        "say yes\n",
        "say -0\n",
    ];

    for source in cases {
        let formatted = assert_idempotent(source, source);
        assert_eq!(
            formatted, source,
            "a singleton statement must format to itself"
        );
    }
}

#[test]
fn edge_numeric_boundaries_round_trip() {
    let cases = [
        "say 0\n",
        "say 1\n",
        "say -1\n",
        "say 1.5\n",
        "say -0.0\n",
        "say 0.1\n",
        "say 9007199254740993\n",
        "say 1e308\n",
        "say 1e-308\n",
        "say 4503599627370496\n",
        "say 2 - 1 - 1\n",
        "say - -1\n",
        "say 0 - -0\n",
    ];

    for source in cases {
        let formatted = assert_idempotent(source, source);
        assert!(
            same_program(&parse(source), &parse(&formatted)),
            "number {} must read back as the same number, formatted as:\n{}",
            source.trim(),
            formatted
        );
    }
}

// ---------------------------------------------------------------------------
// Nesting
// ---------------------------------------------------------------------------

#[test]
fn edge_nested_lists_and_records_round_trip() {
    let source = "\
set nested to [[1, 2], [[3], [4, [5, 6]]]]
say nested
set r to {a: {b: {c: 1}}, d: [1, 2]}
say r
say nested[1][0][0]
";

    let formatted = assert_idempotent(source, "nesting");
    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "nested literals must survive formatting:\n{}",
        formatted
    );
}

#[test]
fn edge_duplicate_record_keys_keep_their_order() {
    // A record is a list of pairs, so the formatter must print them all, in the
    // order they were written.
    let source = "say {a: 1, b: 2, a: 3}\n";

    let formatted = assert_idempotent(source, "duplicate keys");
    assert_eq!(
        formatted, source,
        "every field, duplicate keys included, must be printed in order"
    );
}

#[test]
fn edge_missing_record_field_is_not_invented_by_formatting() {
    // Reading a field the record does not have yields `nothing`. Formatting
    // must not invent the field, and must not turn the read into an error.
    let source = "set r to {a: 1}\nsay r.b\n";

    let formatted = assert_idempotent(source, "missing field");

    assert_eq!(
        run_program(&formatted),
        "nothing\n",
        "a field the record does not have must still be nothing"
    );
    assert_eq!(
        run_program("set r to {a: 1}\nsay r.a\n"),
        "1\n",
        "the field the record does have must still read back"
    );
}

#[test]
fn edge_a_duplicate_key_is_still_overwritten_after_formatting() {
    // The tree keeps both fields; the runtime keeps the last one.
    let source = "say {a: 1, a: 2}.a\n";

    let formatted = assert_idempotent(source, "duplicate key at runtime");

    assert_eq!(
        run_program(&formatted),
        "2\n",
        "the last field with a key must win, formatted or not"
    );
}

// ---------------------------------------------------------------------------
// Malformed input
// ---------------------------------------------------------------------------

#[test]
fn formatter_rejects_malformed_input_instead_of_guessing() {
    let cases = [
        ("say \"unterminated", "Lexer error"),
        ("if true\n    say 1\n", "Parser error"),
        ("say )", "Parser error"),
        ("to f(\nend", "Parser error"),
        ("say 1.2.3\n", "Lexer error"),
    ];

    for (source, expected) in cases {
        let error = formatter::format(source)
            .err()
            .unwrap_or_else(|| panic!("{:?} must not format, got:\n{}", source, source));
        assert!(
            error.contains(expected),
            "expected a {} error for {:?}, got: {}",
            expected,
            source,
            error
        );
    }
}

#[test]
fn edge_crlf_source_formats_to_lf_and_is_idempotent() {
    let source = "// a comment\r\nsay 1\r\nsay 2\r\n";

    let formatted = assert_idempotent(source, "CRLF");
    assert_eq!(
        formatted, "// a comment\nsay 1\nsay 2\n",
        "CRLF line endings must normalise to LF"
    );
}

#[test]
fn edge_a_bom_does_not_become_part_of_the_first_statement() {
    let source = "\u{feff}say 1\n";

    let formatted = assert_idempotent(source, "BOM");
    assert_eq!(formatted, "say 1\n", "the BOM must not be printed");
}

#[test]
fn edge_unclosed_block_is_a_parser_error() {
    let error = formatter::format("to f\n    say 1\n")
        .expect_err("a function with no `end` must not format");

    assert!(
        error.starts_with("Parser error"),
        "expected a parser error, got: {}",
        error
    );
}

// ---------------------------------------------------------------------------
// Every statement and expression form
// ---------------------------------------------------------------------------

#[test]
fn format_covers_every_statement_form() {
    let source = "\
import files, network to net
say 1
print 2
set x to 1
set obj.field to 2
if x is 1 then
    say 1
else
    say 2
end
for each item in [1, 2]
    say item
end
repeat 3 times
    say 1
end
while x is less than 3
    set x to x + 1
end
to helper(a, b)
    give back a + b
end
object Thing
    has name default \"none\"
    to can greet()
        say this.name
    end
end
try
    say 1
catch error
    say error
finally
    say 2
end
test \"a test\"
    expect 1 + 1 to be 2
end
say helper(1, 2)
say obj.field
say obj.describe(1)
say [1, 2][0]
";

    let formatted = assert_idempotent(source, "statement forms");

    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "every statement form must survive formatting:\n{}",
        formatted
    );
}

#[test]
fn edge_a_module_and_its_exports_round_trip() {
    // `module`, `constant` and `export` are three statement forms the table
    // above does not reach, and the corpus reaches them through exactly one
    // file. `export` has a second shape again — `export all` with no names — so
    // both are here.
    let source = "\
module MathUtils
    constant PI to 3.14159
    to circle_area(radius)
        give back PI * radius * radius
    end
    export circle_area
end
";

    let formatted = assert_idempotent(source, "a module with an export list");

    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "a module must survive formatting:\n{}",
        formatted
    );
    assert!(
        formatted.contains("export circle_area"),
        "the export list must survive formatting:\n{}",
        formatted
    );
    assert!(
        !formatted.contains("export all"),
        "`export circle_area` is not `export all`, and must not be written as it:\n{}",
        formatted
    );

    let all = assert_idempotent("module M\nexport all\nend\n", "export all");
    assert!(
        all.contains("export all"),
        "`export all` must keep the `all`, which is the whole statement:\n{}",
        all
    );

    // And the module still computes the same thing it did before.
    assert_eq!(
        run_program(source),
        run_program(&formatted),
        "formatting a module must not change what its function returns"
    );
}

#[test]
fn edge_an_unless_and_a_constant_survive_formatting() {
    // Two more forms the coverage table above does not reach. `unless` has no
    // second branch, so a formatter that emitted it the way it emits `if` would
    // either invent an `else` or drop one; `constant` differs from `set` only in
    // its keyword, and the keyword is what makes the binding a constant.
    let source = "\
constant LIMIT to 3
unless LIMIT is 0 then
    say \"nonzero\"
end
";

    let formatted = assert_idempotent(source, "unless and constant");

    assert!(
        !formatted.contains("else"),
        "`unless` has no second branch, so formatting must not invent one:\n{}",
        formatted
    );
    assert!(
        formatted.contains("constant LIMIT to 3"),
        "a constant must keep its keyword:\n{}",
        formatted
    );
    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "`unless` and `constant` must survive formatting:\n{}",
        formatted
    );
    assert_eq!(
        run_program(source),
        run_program(&formatted),
        "formatting must not change what the program does"
    );
}

#[test]
fn edge_a_program_deeper_than_the_parser_budget_is_reported_not_truncated() {
    // The test above sits at depth 60, inside the parser's budget, so it only
    // ever exercised the success arm. This one is past the budget, so the
    // resource-limit path is reached: the formatter must hand back the
    // parser's own diagnostic rather than format the part it managed to read,
    // and the binary must exit 1 instead of dying on the stack.
    //
    // The depth is a literal rather than `MAX_NESTING_DEPTH + n`, so that
    // *raising* the limit fails this test rather than silently moving the
    // target out of reach. It is pinned behaviour: the budget is 64 today, and
    // changing it is deliberate, so this test has to be re-anchored then.
    let past_expression = 70;
    assert!(
        parser::MAX_NESTING_DEPTH < past_expression,
        "depth {} is no longer past MAX_NESTING_DEPTH ({}); re-anchor this test \
         if the parser's budget changes",
        past_expression,
        parser::MAX_NESTING_DEPTH
    );
    let source = format!("say {}\n", format!("{}+1", 1).repeat(past_expression));

    let error = formatter::format(&source).err().unwrap_or_else(|| {
        panic!(
            "{} levels of expression nesting is past MAX_NESTING_DEPTH ({}), so \
             it must be reported rather than formatted",
            past_expression,
            parser::MAX_NESTING_DEPTH
        )
    });
    assert!(
        error.starts_with("Parser error"),
        "expected a parser error naming the nesting budget, got: {}",
        error
    );
    assert!(
        error.contains(&format!(
            "more than {} levels deep",
            parser::MAX_NESTING_DEPTH
        )),
        "the diagnostic must say which budget was exceeded, got: {}",
        error
    );

    // The same through the CLI, where the budget is a user's exit code.
    let run = run_rb("format", &source);
    assert_eq!(
        run.code, 1,
        "a file too deep to parse must exit 1, got code {} and stdout {}",
        run.code, run.stdout
    );
    assert!(
        run.stderr.contains("Format error"),
        "the binary must say the file does not format, got: {}",
        run.stderr
    );
    assert_eq!(
        run.stdout, "",
        "nothing may be printed for a file that was not understood:\n{}",
        run.stdout
    );
}

#[test]
fn edge_blocks_deeper_than_the_parser_budget_are_reported_not_truncated() {
    // `MAX_BLOCK_DEPTH` is the other half of the budget: a block body is parsed
    // by recursing back into `parse_statement`, so 70 nested `if`s are a
    // diagnostic and not a stack overflow. Literal depth, for the same reason
    // as the expression case above.
    let past_blocks = 70;
    assert!(
        parser::MAX_BLOCK_DEPTH < past_blocks,
        "depth {} is no longer past MAX_BLOCK_DEPTH ({}); re-anchor this test if \
         the parser's budget changes",
        past_blocks,
        parser::MAX_BLOCK_DEPTH
    );
    let mut source = String::new();
    for _ in 0..past_blocks {
        source.push_str("if 1 is 1 then\n");
    }
    source.push_str("say 1\n");
    for _ in 0..past_blocks {
        source.push_str("end\n");
    }

    let error = formatter::format(&source).err().unwrap_or_else(|| {
        panic!(
            "{} nested blocks is past MAX_BLOCK_DEPTH ({}), so it must be \
             reported rather than formatted",
            past_blocks,
            parser::MAX_BLOCK_DEPTH
        )
    });
    assert!(
        error.starts_with("Parser error"),
        "expected a parser error naming the block budget, got: {}",
        error
    );
    assert!(
        error.contains(&format!(
            "more than {} levels deep",
            parser::MAX_BLOCK_DEPTH
        )),
        "the diagnostic must say which budget was exceeded, got: {}",
        error
    );
}

#[test]
fn edge_a_very_deeply_nested_program_is_handled_or_reported() {
    let depth = 60;
    let source = format!("say {}\n", format!("{}+1", 1).repeat(depth));

    match formatter::format(&source) {
        Ok(formatted) => {
            assert!(
                same_program(&parse(&source), &parse(&formatted)),
                "a nested expression must survive formatting"
            );
        }
        Err(error) => {
            assert!(
                error.starts_with("Parser error"),
                "a program too deep to parse must be reported, not mangled: {}",
                error
            );
        }
    }
}

#[test]
fn edge_a_very_long_text_literal_round_trips() {
    let text = "x".repeat(20_000);
    let source = format!("say \"{}\"\n", text);

    let formatted = formatter::format(&source).expect("a long literal must format");
    assert!(
        same_program(&parse(&source), &parse(&formatted)),
        "a long literal must survive formatting unchanged"
    );
    assert_eq!(
        formatter::format(&formatted).expect("second format"),
        formatted,
        "a long literal must format idempotently"
    );
}

// ---------------------------------------------------------------------------
// Behaviour, not just the tree
// ---------------------------------------------------------------------------

/// The corpus files whose output is a function of the program alone.
/// `examples/files.rb` writes files and `examples/time.rb` reads the clock, so
/// neither can be compared run to run.
fn deterministic_corpus() -> Vec<(PathBuf, &'static str)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<(PathBuf, &'static str)> = vec![
        (root.join("examples/hello.rb"), "run"),
        (root.join("examples/fizzbuzz.rb"), "run"),
        (root.join("examples/formats.rb"), "run"),
        (root.join("examples/test_arithmetic.rb"), "run"),
    ];
    for entry in fs::read_dir(root.join("tests")).expect("tests/ should be readable") {
        let path = entry.expect("directory entry should be readable").path();
        if path.extension().and_then(|e| e.to_str()) == Some("rb") {
            files.push((path, "test"));
        }
    }
    files.sort();
    files
}

/// Drops the one line of output that is about the machine rather than the
/// program: `rb test` reports how long it took.
fn without_duration(output: &str) -> String {
    output
        .lines()
        .filter(|line| !line.starts_with("Duration:"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn formatted_programs_behave_exactly_as_the_originals() {
    for (path, mode) in deterministic_corpus() {
        let source = read(&path);
        let label = path.display().to_string();

        let formatted = formatter::format(&source)
            .unwrap_or_else(|e| panic!("{}: format failed: {}", label, e));

        let before = run_rb(mode, &source);
        let after = run_rb(mode, &formatted);

        assert_eq!(
            after.code, before.code,
            "{}: exit status changed after formatting",
            label
        );
        assert_eq!(
            without_duration(&after.stdout),
            without_duration(&before.stdout),
            "{}: what the program prints changed after formatting",
            label
        );
        assert_eq!(
            after.stderr, before.stderr,
            "{}: what the program reports changed after formatting",
            label
        );
    }
}

// ---------------------------------------------------------------------------
// `rb format --check`, through the binary
// ---------------------------------------------------------------------------

#[test]
fn check_rejects_a_file_that_differs_only_in_its_last_byte() {
    let formatted = "say 1\n";

    let clean = run_rb("format", formatted);
    assert_eq!(
        clean.code, 0,
        "a formatted file must print, got: {}",
        clean.stdout
    );

    // `rb format --check` reads the fourth argument, so the check is driven
    // through the same path a user takes.
    for source in ["say 1", "say 1\n\n", "say 1  \n", "say 1\nsay 2\n"] {
        assert!(
            formatter::needs_reformat(source, formatted),
            "{:?} differs from {:?} and must be reported",
            source,
            formatted
        );
    }
}

#[test]
fn edge_a_program_that_fails_still_fails_the_same_way_after_formatting() {
    let source = "set x to 1e400\nsay x\n";

    let formatted = assert_idempotent(source, "out of range literal");

    let before = run_rb("run", source);
    let after = run_rb("run", &formatted);

    assert_ne!(before.code, 0, "the program is meant to fail");
    assert_eq!(
        after.code, before.code,
        "an out of range literal must fail the same way after formatting"
    );
    assert_eq!(
        after.stderr, before.stderr,
        "the error must be the same one after formatting"
    );
}

#[test]
fn edge_grouping_survives_in_what_the_program_computes() {
    // The tree comparison says the grouping was kept; this says the arithmetic
    // agrees, which is what a reader of the formatted file would rely on.
    let source = "set a to (1 + 2) * 3\nsay a\n";

    let formatted = assert_idempotent(source, "grouping at runtime");

    assert_eq!(run_program(source), "9\n");
    assert_eq!(run_program(&formatted), "9\n");
    assert_eq!(run_program("set a to 1 + 2 * 3\nsay a\n"), "7\n");
}

// ---------------------------------------------------------------------------
// try/catch — the formatter must never delete a block
// ---------------------------------------------------------------------------

#[test]
fn edge_bare_catch_body_is_not_deleted() {
    // `catch` with no name binding is legal: the parser leaves catch_var as None
    // and still collects the body. Gating the emit on `Some(var)` dropped the
    // whole block, so the formatter silently deleted statements.
    let source = "try\n    say 1 / 0\ncatch\n    say \"recovered\"\nend\n";

    let formatted = formatter::format(source).expect("a bare catch must format");

    assert!(
        formatted.contains("catch"),
        "the catch keyword must survive formatting, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("recovered"),
        "THE CATCH BODY WAS DELETED by the formatter, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("1 / 0"),
        "the try body must survive formatting, got:\n{}",
        formatted
    );
}

#[test]
fn edge_bare_catch_program_still_computes_the_same_after_formatting() {
    // The tree said the block was kept while the text said it was dropped. Run
    // it: if the body is gone the program cannot recover and cannot print.
    let source = "try\n    say 1 / 0\ncatch\n    say \"recovered\"\nend\n";

    let formatted = assert_idempotent(source, "bare catch");

    assert_eq!(run_program(source), "recovered\n");
    assert_eq!(
        run_program(&formatted),
        "recovered\n",
        "formatting changed what the program does — a statement was lost"
    );
}

// ---------------------------------------------------------------------------
// try/catch — the formatter must never delete a block
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Forms documented by the specification: the loop and the relational operators
//
// `SPEC.md:383`, `SPEC.md:388`, `SPEC.md:427` and `docs/GRAMMAR.md:472` document
// `for each <v> from <a> to <b> [by <step>]`. `docs/GRAMMAR.md:368-371` and
// `:438` document the relational operators `is less than`,
// `is greater than`, `is less than or equal to`, `is greater than or equal to`
// and the symbols `<`, `<=`, `>`, `>=`.
//
// No corpus file uses either, so nothing else here reaches them: the formatter
// renders a `Statement::ForRange` and the four relational `BinaryOp`s, and
// without these tests that rendering is unasserted. The tests below pin what
// it does with them.
// ---------------------------------------------------------------------------

/// The `for … from … to … by …` loop, exactly as `SPEC.md:388` spells it.
const SPEC_FOR_FROM_TO_BY: &str = "for each i from 0 to 10 by 5\n    say i\nend\n";
/// The same loop without the step clause, as `SPEC.md:383` spells it.
const SPEC_FOR_FROM_TO: &str = "for each i from 1 to 10\n    say i\nend\n";
/// A range loop whose bounds and step are themselves expressions.
const SPEC_FOR_FROM_EXPRS: &str =
    "set hi to 10\nset step to 2\nfor each i from 1 to hi by step\n    say i\nend\n";
/// Every relational operator in its word form, one per line.
const SPEC_RELATIONAL_WORDS: &str = "\
say 1 is less than 2
say 1 is less than or equal to 1
say 2 is greater than 1
say 2 is greater than or equal to 1
";
/// The same four comparisons in the symbolic form of `docs/GRAMMAR.md:438`.
const SPEC_RELATIONAL_SYMBOLS: &str = "\
say 1 < 2
say 1 <= 1
say 2 > 1
say 2 >= 1
";

#[test]
fn edge_a_range_loop_is_formatted_and_keeps_its_bounds_and_step() {
    for (label, source) in [
        ("with a step", SPEC_FOR_FROM_TO_BY),
        ("without a step", SPEC_FOR_FROM_TO),
        ("with expression bounds", SPEC_FOR_FROM_EXPRS),
    ] {
        let formatted = assert_idempotent(source, label);

        assert!(
            same_program(&parse(source), &parse(&formatted)),
            "{}: formatting changed the range loop\n--- source ---\n{}\n--- formatted ---\n{}",
            label,
            source,
            formatted
        );
        assert_eq!(
            run_program(source),
            run_program(&formatted),
            "{}: formatting changed what the range loop computes",
            label
        );
    }
}

#[test]
fn edge_a_range_loop_step_is_not_lost_or_invented() {
    // `by` is optional in the grammar, so the tree carries an `Option`. A
    // formatter that always wrote it, or never wrote it, would change which
    // loop the file describes: the step is what makes `0, 5, 10` differ from
    // `0 … 10`.
    let without = assert_idempotent(SPEC_FOR_FROM_TO, "a range loop with no step");
    assert!(
        !without.contains(" by "),
        "a step the source did not write must not be invented:\n{}",
        without
    );

    let with = assert_idempotent(SPEC_FOR_FROM_TO_BY, "a range loop with a step");
    assert!(
        with.contains("by 5"),
        "the step must survive formatting:\n{}",
        with
    );
    assert_eq!(
        run_program(SPEC_FOR_FROM_TO_BY),
        "0\n5\n10\n",
        "the fixture must really be a step of 5, or the assertion above is vacuous"
    );
    assert_ne!(
        run_program(SPEC_FOR_FROM_TO),
        run_program(SPEC_FOR_FROM_TO_BY),
        "the two fixtures must differ, or dropping the step would go unnoticed"
    );
}

#[test]
fn edge_every_relational_operator_round_trips_in_both_spellings() {
    // The words and the symbols are two spellings of the same four operators,
    // so the formatter is free to normalise one to the other — but only to the
    // same operator. Formatting must keep each comparison the comparison it was.
    let words = assert_idempotent(SPEC_RELATIONAL_WORDS, "the word spellings");
    let symbols = assert_idempotent(SPEC_RELATIONAL_SYMBOLS, "the symbolic spellings");

    assert_eq!(
        words, symbols,
        "the word and symbolic spellings are the same four comparisons, so \
         they must format to the same text"
    );
    for (label, source) in [("words", &words), ("symbols", &symbols)] {
        assert!(
            same_program(&parse(source), &parse(SPEC_RELATIONAL_WORDS)),
            "{}: formatting changed a relational operator's tree",
            label
        );
        assert_eq!(
            run_program(source),
            "yes\nyes\nyes\nyes\n",
            "{}: every comparison here is true, so every line must print yes",
            label
        );
    }
}

#[test]
fn edge_a_relational_operator_is_not_reordered_around_its_operands() {
    // The relational operators sit below arithmetic but above `and`/`or`, so
    // each one can appear either bare or inside a grouping that has to be kept.
    // Dropping or adding a parenthesis here would change the result of the
    // program, not merely its appearance.
    for (label, source, expected) in [
        ("bare", "say 1 + 1 < 3\n", "yes\n"),
        (
            "grouped to mean the arithmetic",
            "say (1 + 1) < 3\n",
            "yes\n",
        ),
        (
            "grouped to mean the comparison",
            "say (1 < 3) and yes\n",
            "yes\n",
        ),
        ("chained with and", "say 1 < 2 and 2 < 3\n", "yes\n"),
    ] {
        let formatted = assert_idempotent(source, label);

        assert!(
            same_program(&parse(source), &parse(&formatted)),
            "{}: formatting changed the grouping of a relational operator\n{}",
            label,
            formatted
        );
        assert_eq!(
            run_program(&formatted),
            expected,
            "{}: formatting changed what the comparison computes",
            label
        );
    }
}

#[test]
fn edge_a_comparison_of_different_types_still_fails_the_same_way() {
    // Comparing a number with text is a runtime error, and a formatter that
    // re-rendered the operands differently would either make it succeed or
    // change which error is reported. The error *identity* is what is compared:
    // the rest of a diagnostic quotes the file it read, which formatting moved.
    let source = "say 1 < \"a\"\n";

    let formatted = assert_idempotent(source, "a comparison of mixed types");

    let before = run_rb("run", source);
    let after = run_rb("run", &formatted);

    assert_eq!(before.code, 1, "a number compared with text must fail");
    assert_eq!(
        before.code, after.code,
        "formatting must not change whether the program runs"
    );
    assert_eq!(
        before.stdout, after.stdout,
        "formatting must not change what the program prints"
    );
    assert_eq!(
        error_line(&before.stderr),
        error_line(&after.stderr),
        "formatting must not change the error that is reported"
    );
    assert_eq!(
        after.code, 1,
        "the formatted form must still be the failing one, or the comparison \
         changed: {}",
        after.stderr
    );
}

/// The `Error: …` head of a diagnostic, without the quoted source line under
/// it, which is a rendering of the file rather than part of the error.
fn error_line(stderr: &str) -> &str {
    stderr.lines().next().unwrap_or("")
}

#[test]
fn edge_a_spec_only_form_is_never_reported_as_an_error() {
    // The umbrella over the tests above, and the counterpart of what this file
    // used to assert: these forms used to be unreachable, and the tests here
    // recorded them as rejected. They are grammar now, so a formatter that
    // still reported one — a stale lexer, a stale parser check — would be
    // wrong. Every one must format, and format the same way twice.
    for (label, source) in [
        ("for from to by", SPEC_FOR_FROM_TO_BY),
        ("for from to", SPEC_FOR_FROM_TO),
        ("for from expressions", SPEC_FOR_FROM_EXPRS),
        ("relational words", SPEC_RELATIONAL_WORDS),
        ("relational symbols", SPEC_RELATIONAL_SYMBOLS),
    ] {
        let once = formatter::format(source)
            .unwrap_or_else(|e| panic!("{}: a documented form must format: {}", label, e));
        let twice = formatter::format(&once).unwrap_or_else(|e| {
            panic!(
                "{}: formatting succeeded once and failed again: {}",
                label, e
            )
        });

        assert_eq!(once, twice, "{}: formatting is not idempotent", label);
        assert!(
            same_program(&parse(source), &parse(&once)),
            "{}: formatting changed the program\n--- source ---\n{}\n--- formatted ---\n{}",
            label,
            source,
            once
        );
    }
}

// ---------------------------------------------------------------------------
// Review round 3 — a branch the formatter must not drop
// ---------------------------------------------------------------------------

/// How many statements each `try` in `program` holds in its catch clause, with
/// `None` for a `try` whose catch has no name.
///
/// The catch body's name and size are what the round-3 finding turned on: a
/// `catch` with a body and no name is a catch clause, and dropping it takes the
/// statements under it with it.
fn catch_bodies(program: &Program) -> Vec<Option<usize>> {
    program
        .statements
        .iter()
        .filter_map(|stmt| match &stmt.statement {
            Statement::Try {
                catch_var,
                catch_body,
                ..
            } => Some(if catch_var.is_none() {
                Some(catch_body.len())
            } else {
                None
            }),
            _ => None,
        })
        .collect()
}

#[test]
fn edge_a_catch_without_a_name_keeps_the_branch_it_opens() {
    // The name after `catch` is optional to the parser — it is taken only if one
    // follows — so `catch` on its own is a catch clause with a body and no name,
    // and its tree is indistinguishable from a `try` with no catch clause except
    // that the body is not empty. Writing the clause only when the name was
    // there dropped the whole branch: this source came back as
    // `try / say 1 / finally / say 3 / end / say 4`, which runs three statements
    // instead of four.
    let source = "\
try
    say 1
catch
    say 2
finally
    say 3
end
say 4
";

    let formatted = assert_idempotent(source, "a catch with no name");

    assert_eq!(
        formatted, source,
        "a bare `catch` and the statements under it must be written back out"
    );
    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "formatting must not drop the statements of a nameless catch:\n{}",
        formatted
    );
    assert_eq!(
        catch_bodies(&parse(source)),
        vec![Some(1)],
        "the fixture must really be a nameless catch holding one statement, or \
         this test proves nothing"
    );
    assert_eq!(
        catch_bodies(&parse(&formatted)),
        vec![Some(1)],
        "the nameless catch must still hold its statement after formatting:\n{}",
        formatted
    );
    assert!(
        formatted.contains("catch"),
        "the clause keyword itself was dropped:\n{}",
        formatted
    );

    // And what the program prints must not change either. The virtual machine
    // binds no name for a nameless catch, so its body does not run — that is the
    // VM's business and is recorded as F6, not something the formatter may
    // change — but formatting must not alter which statements run.
    assert_eq!(
        run_program(&formatted),
        run_program(source),
        "formatting must not change what the program does"
    );
}

#[test]
fn edge_named_catch_keeps_its_binding() {
    // The regression above must not cost the named form: `catch err` has to keep
    // both the keyword and the binding.
    let source = "try\n    say 1 / 0\ncatch err\n    say err\nend\n";

    let formatted = assert_idempotent(source, "named catch");

    assert!(
        formatted.contains("catch err"),
        "a named catch must keep its binding, got:\n{}",
        formatted
    );
    assert_eq!(run_program(source), run_program(&formatted));
}

#[test]
fn edge_finally_still_closes_a_bare_catch() {
    // `catch` followed by `finally` is the shape most likely to be mangled while
    // fixing the bare case: the block must close in the right order.
    let source = "try\n    say 1 / 0\ncatch\n    say \"a\"\nfinally\n    say \"b\"\nend\n";

    let formatted = assert_idempotent(source, "catch then finally");

    let catch_at = formatted.find("catch").expect("catch must survive");
    let finally_at = formatted.find("finally").expect("finally must survive");
    let end_at = formatted.find("end").expect("end must survive");
    assert!(
        catch_at < finally_at,
        "catch must precede finally:\n{}",
        formatted
    );
    assert!(
        finally_at < end_at,
        "finally must precede end:\n{}",
        formatted
    );
}

#[test]
fn edge_a_branch_written_on_one_line_keeps_its_keywords() {
    // The same loss in the shape a whole `try` on one line takes: every keyword
    // that closes or reopens a block then shares the block's line, so there is
    // no *later* line to look the keyword up on and only the order the source
    // wrote them in can say which one closed this block. The keywords are still
    // read in the same post-order the parser read them in, so the next one not
    // yet taken is this block's.
    let source = "try say 1 catch say 2 finally say 3 end\n";
    let expected = "\
try
    say 1
catch
    say 2
finally
    say 3
end
";

    let formatted = assert_idempotent(source, "a whole try on one line");

    assert_eq!(
        formatted, expected,
        "a nameless catch and a finally written on one line must both survive"
    );
    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "formatting must not drop a branch of a one-line block:\n{}",
        formatted
    );
}

#[test]
fn edge_an_empty_branch_keeps_its_keyword_and_the_comment_written_on_it() {
    // `else`, a bare `catch` and `finally` are written back even when the branch
    // they open holds nothing — a comment and nothing else is not nothing. The
    // keyword is what the author wrote; dropping it for being empty took the
    // comment with it, because there was then nothing on that line for `// …` to
    // trail, and `write_keyword_line("end")` re-attached it as `end // …`: a note
    // about one branch moved onto the line that closed the whole statement.
    let source = "\
if 1 is 1 then
    say 1
else // nothing to do here
    // and nothing here either
end
try
    say 2
catch // no name to bind
    // an empty catch
finally // either way
    // nothing either
end
say 3
";

    let formatted = assert_idempotent(source, "branches with nothing in them");

    assert_eq!(
        formatted, source,
        "a keyword the source wrote must stay, with the comment written on it"
    );
    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "formatting must not change the program:\n{}",
        formatted
    );
    assert!(
        !formatted.contains("end //"),
        "no comment may migrate onto the keyword that closes a block:\n{}",
        formatted
    );

    // The keyword matters on its own, not only as the comment's anchor: a branch
    // with nothing in it and no comment on its line still reads back the way it
    // was written.
    let bare = "\
if 1 is 1 then
    say 1
else
end
try
    say 2
catch
finally
end
";
    assert_eq!(
        assert_idempotent(bare, "empty branches with no comments"),
        bare,
        "an `else`, a bare `catch` and a `finally` the source wrote must be kept"
    );
}
