//! `this` is the operand of a worded index and of a worded length.
//!
//! `this` is a plain variable as far as [`Parser::parse_primary`] is concerned
//! (src/parser.rs:2675), so it is a primary everywhere it can reach. The one
//! place it could not reach was the operand of a worded form: the operand table
//! ([`Parser::kind_starts_operand`], src/parser.rs:1851) did not list
//! `TokenKind::This`, and neither did `is_operand_start`
//! (bootstrap/compiler.rb:1146), so the two things it gates went wrong in two
//! different ways inside a method.
//!
//! - `length of this` was refused by the parser: `ParserError: Expected
//!   something to measure after 'length of'`, positioned at `this`. The same
//!   program with `nothing` in that position parsed and reached the runtime
//!   check `length requires a list or text`.
//! - `this.xs at this` was not an index at all. `at` only reads as an index
//!   when an operand follows it on the same line (`Parser::at_index_ahead`), so
//!   `at this` split into two statements and the analyzer reported
//!   `Unknown variable 'at'; did you mean 'as'?` — the misparse phase-080 was
//!   created to end, in the one spelling `at` still misread.
//!
//! Both engines made the same mistake, so the byte-identity tests in
//! `bootstrap_selfhost_test.rs` compared two wrong answers and stayed green.
//! That is why the table parity test here reads both tables as text: the
//! property "the two tables list the same token kinds" is the one that could
//! have noticed, and no differential test can see it.
//!
//! What a fixed `length of this` answers is a **runtime** failure, not a value:
//! `this` is the prototype *record* (SPEC.md § "Lookup order and shadowing"),
//! and `length` takes a list or a text. `length of this` and `length of
//! nothing` therefore reach the same runtime check, which is exactly the shape
//! the finding asked for — the parser must stop being the thing that decides.
//! The same is true of `this.xs at this`: a record is not an index, so the
//! answer is `Index must be a number`, never `Unknown variable 'at'`.

use redblue::bytecode::vm::BytecodeVm;
use redblue::parser::{Expr, Statement, Stmt};
use redblue::{run_isolated, Error};

/// The two tables, as text, so a table that drifts from the other is a failing
/// test and not a reader's memory.
const PARSER_SOURCE: &str = include_str!("../src/parser.rs");
const COMPILER_SOURCE: &str = include_str!("../bootstrap/compiler.rb");

/// The lexer's spelling of a `TokenKind` variant, which is the name
/// `token_kind` (bootstrap/compiler.rb) prints and the name stage 2's table is
/// written in.
///
/// One Rust variant is two lexemes: `TokenKind::YesNo(true)` and
/// `TokenKind::YesNo(false)` are the same variant with a payload, while
/// `token_kind` says `"yes"` or `"no"` for the two of them. The table is
/// compared in lexer names, so the variant expands to both and the Redblue side
/// — which really does test for the two separately — can answer `yes` where the
/// Rust one answers `YesNo(_)`.
fn lexer_names(variant: &str) -> Vec<String> {
    match variant {
        "Number" => vec!["num".to_string()],
        "Text" => vec!["text".to_string()],
        "YesNo" => vec!["no".to_string(), "yes".to_string()],
        "Nothing" => vec!["nothing".to_string()],
        "Identifier" => vec!["id".to_string()],
        "LeftParen" => vec!["lparen".to_string()],
        "LeftBracket" => vec!["lbrack".to_string()],
        "LeftBrace" => vec!["lbrace".to_string()],
        "Not" => vec!["not".to_string()],
        "Minus" => vec!["minus".to_string()],
        "MightFail" => vec!["mightfail".to_string()],
        "To" => vec!["to".to_string()],
        "This" => vec!["this".to_string()],
        other => panic!(
            "`{other}` is not a variant this file knows the lexer name of, so the \
             comparison would be over a name neither engine writes. Add it here."
        ),
    }
}

/// The body of a function, from its `fn` line to the line that closes it at the
/// same indentation.
///
/// The bound is what makes the reading safe: an operand table's own arms and a
/// neighbouring `match` arm naming the same variants must not be able to be
/// confused for one another, so nothing outside the function counts.
fn function_body(source: &str, signature: &str) -> String {
    let start = source
        .find(signature)
        .unwrap_or_else(|| panic!("`{signature}` is not in the source being checked"));
    let rest = &source[start..];
    let lines: Vec<&str> = rest.lines().collect();
    let indent = lines[0].len() - lines[0].trim_start().len();

    let mut body = String::new();
    for line in lines.iter().skip(1) {
        let line_indent = line.len() - line.trim_start().len();
        if line.trim_start().starts_with('}') && line_indent == indent {
            break;
        }
        body.push_str(line);
        body.push('\n');
    }
    assert!(
        body.contains("matches!") || body.contains("is \""),
        "the body of `{signature}` holds no table to read, so the extraction is \
         wrong rather than the table being empty"
    );
    body
}

/// The `matches!` arm list of the Rust operand table, and nothing else.
///
/// The list is closed by the first line that starts with the `)` of the macro
/// call, which is what stops a later `TokenKind::` in the same file from being
/// read as an operand: `kind_starts_expression`, `is_operand_start` and a dozen
/// other tables name these variants, and a reader that ran past the closing
/// paren would be comparing all of them to one table in stage 2.
fn rust_operand_table() -> Vec<String> {
    let body = function_body(PARSER_SOURCE, "fn kind_starts_operand");
    let Some(open) = body.find("matches!(") else {
        panic!("kind_starts_operand no longer matches a table of token kinds");
    };

    let mut kinds = Vec::new();
    for line in body[open..].lines() {
        // Every arm after the first is written `| TokenKind::…`, so the bar is
        // part of the arm rather than of the variant name.
        let line = line.trim().trim_start_matches('|').trim();
        if line.starts_with(')') {
            break;
        }
        if !line.starts_with("TokenKind::") {
            continue;
        }
        let variant: String = line["TokenKind::".len()..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        kinds.extend(lexer_names(&variant));
    }
    kinds.sort();
    kinds.dedup();
    kinds
}

/// The token kinds the Redblue operand table answers `yes` for.
fn redblue_operand_table() -> Vec<String> {
    let body = function_body(COMPILER_SOURCE, "to is_operand_start(tokens, pos)");
    let mut kinds = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("if is_operand_start_k is \"") else {
            continue;
        };
        kinds.push(rest.split('"').next().unwrap_or_default().to_string());
    }
    kinds.sort();
    kinds.dedup();
    kinds
}

/// What one engine made of a program: the lines it printed, and either the
/// value it ended with or the failure it ended with.
///
/// A program that does not lex, parse or analyze has *failed*, and that failure
/// is the outcome rather than a panic: a parser refusal is one of the two
/// answers this file is about, and a helper that panicked on it could not tell
/// the wrong parser answer from a broken harness.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    output: Vec<String>,
    result: Result<String, Failure>,
}

/// A failure as the engine reported it: its label, its message, and the line it
/// was placed on where the engine carries one.
#[derive(Debug, PartialEq, Eq)]
struct Failure {
    label: String,
    message: String,
    line: Option<usize>,
}

impl Failure {
    fn of(error: &Error) -> Failure {
        Failure {
            label: error.label().to_string(),
            message: error.message().to_string(),
            line: error.span().map(|span| span.line),
        }
    }
}

impl Outcome {
    fn failed(failure: Failure) -> Outcome {
        Outcome {
            output: Vec::new(),
            result: Err(failure),
        }
    }
}

/// Runs `source` on the tree-walking VM, which is the specification.
fn tree_walk(source: &str) -> Outcome {
    let tokens = match redblue::lexer::Lexer::tokenize(source) {
        Ok(tokens) => tokens,
        Err(error) => return Outcome::failed(Failure::of(&error)),
    };
    let ast = match redblue::parser::parse(tokens) {
        Ok(ast) => ast,
        Err(error) => return Outcome::failed(Failure::of(&error)),
    };
    if let Err(error) = redblue::analyzer::analyze(&ast) {
        return Outcome::failed(Failure::of(&error));
    }
    let (mut vm, result) = run_isolated(&ast);
    Outcome {
        output: vm.take_output(),
        result: result
            .map(|value| value.to_string())
            .map_err(|error| Failure::of(&error)),
    }
}

/// Runs `source` through the bytecode compiler and then the bytecode VM.
///
/// A source the compiler refuses is reported with the label the tree-walking VM
/// would have given, because what a reader of this file asks is "did both
/// engines accept this", not "which half of the pipeline refused it".
fn bytecode(source: &str) -> Outcome {
    let chunk = match redblue::compile_source(source) {
        Ok(chunk) => chunk,
        Err(error) => return Outcome::failed(Failure::of(&error)),
    };
    let mut vm = BytecodeVm::new();
    vm.set_echo(false);
    let result = vm.run(&chunk);
    let output = vm.take_output();
    Outcome {
        output,
        result: result
            .map(|value| value.to_string())
            .map_err(|error| Failure::of(&error)),
    }
}

/// The message of the failure `source` produced, failing the test if the
/// program ran instead.
#[track_caller]
fn failure_message(outcome: &Outcome) -> String {
    match &outcome.result {
        Err(failure) => failure.message.clone(),
        Ok(value) => panic!("the program ran and was worth {value}, so it produced no failure"),
    }
}

/// The label of the failure `source` produced: `ParserError`, `AnalyzerError`
/// or `RuntimeError`.
#[track_caller]
fn failure_label(outcome: &Outcome) -> String {
    match &outcome.result {
        Err(failure) => failure.label.clone(),
        Ok(value) => panic!("the program ran and was worth {value}, so it produced no failure"),
    }
}

/// The line `source`'s failure was reported on, where the engine carried one.
#[track_caller]
fn failure_line(outcome: &Outcome) -> Option<usize> {
    match &outcome.result {
        Err(failure) => failure.line,
        Ok(value) => panic!("the program ran and was worth {value}, so it produced no failure"),
    }
}

/// Asserts both engines ran `source` and ended in the same place.
///
/// The requirement this phase carries is that `rb run` and (`rb compile` then
/// `rb vm`) agree on every case, so the assertion is over both: the printed
/// lines and the whole outcome, failure included. An outcome compared by
/// message alone would not notice one engine reporting the same failure a line
/// away, which is a difference a reader would call one.
#[track_caller]
fn both_engines_agree(source: &str) {
    let walked = tree_walk(source);
    let compiled = bytecode(source);
    assert_eq!(
        walked.output, compiled.output,
        "the two engines printed different lines for:\n{source}"
    );
    assert_eq!(
        walked.result, compiled.result,
        "the two engines ended differently for:\n{source}"
    );
}

/// The statements of the method `name` declares in the first object of `source`.
///
/// Read through the AST rather than through the value, because the shape is the
/// claim: `this.xs at this` used to parse as *three* statements where the writer
/// wrote one, and no runtime outcome of either engine could tell a reader that
/// the split had happened — both refused the program, just with different
/// words.
fn method_body(source: &str, name: &str) -> Vec<Stmt> {
    let tokens = redblue::lexer::Lexer::tokenize(source)
        .unwrap_or_else(|error| panic!("the program should lex: {error}"));
    let program = redblue::parser::parse(tokens).unwrap_or_else(|error| {
        panic!(
            "{source}
failed: {error}"
        )
    });

    let object = program
        .statements
        .iter()
        .find_map(|stmt| match &stmt.statement {
            Statement::Object { body, .. } => Some(body),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the program declares no object:\n{source}"));

    object
        .iter()
        .find_map(|stmt| match &stmt.statement {
            Statement::Method {
                name: declared,
                body,
                ..
            } if declared == name => Some(body.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no method named {name} in:\n{source}"))
}

/// The expression a method's one meaningful statement gives back.
///
/// Every `to can` body opens with a **valueless** `give back` statement the
/// writer never wrote — a defect that is on `main` today and has nothing to do
/// with this phase (see `phases/phase-110/FINDINGS.md`). It is filtered here so
/// that this test counts the statements the *writer* wrote: pre-fix
/// `this.xs at this` left behind, after the stray one, a bare `Variable("at")`
/// and a bare `Variable("this")`, which is the split this phase ends.
fn only_expr(source: &str, method: &str) -> Expr {
    let body: Vec<Stmt> = method_body(source, method)
        .into_iter()
        .filter(|stmt| !matches!(stmt.statement, Statement::GiveBack(None)))
        .collect();
    assert_eq!(
        body.len(),
        1,
        "the method must be one statement, as it is written:\n{source}"
    );
    match body.into_iter().next().expect("one statement").statement {
        Statement::Say(expr) => expr,
        other => panic!("the method is one `say`, as it is written: {other:?}"),
    }
}

/// `length of this` inside a method is the call `length(this)` — one expression,
/// not a refusal and not a split.
///
/// The runtime answer for a record is a failure, so the only way to see that
/// `this` reached the *operand* is to look at the tree: a `Call` whose single
/// argument is the variable `this`.
#[test]
fn a_worded_length_of_this_is_the_call_length_this() {
    // `say`, not `give back`: a `to can` body opens with a valueless `give back`
    // the writer never wrote (see `only_expr`), so `give back` inside one is not
    // a statement a reader can count. The operand is what is under test.
    let source =
        "object B\n    has a\n    to can size()\n        say length of this\n    end\nend\n";

    match only_expr(source, "size") {
        Expr::Call { name, args } => {
            assert_eq!(
                name, "length",
                "`length of this` is the builtin, spelled the way it is"
            );
            assert_eq!(args.len(), 1, "`length` takes one operand");
            assert!(
                matches!(&args[0], Expr::Variable(name, _) if name == "this"),
                "the operand is the variable `this`, got {:?}",
                args[0]
            );
        }
        other => panic!("`length of this` is not a call: {other:?}"),
    }
}

/// `this.xs at this` inside a method is one `Index` whose index is `this`.
///
/// This is the DoD in the form that can fail: pre-fix the body held two
/// statements and the analyzer said `Unknown variable 'at'`, post-fix it holds
/// one expression and the runtime says `Index must be a number`.
#[test]
fn a_worded_index_of_this_is_one_index_expression() {
    // `say` for the reason the test above gives.
    let source = "object B\n    has xs default [10, 20, 30]\n    to can pick()\n        say this.xs at this\n    end\nend\n";

    match only_expr(source, "pick") {
        Expr::Index { object, index } => {
            assert!(
                matches!(&*object, Expr::Property { property, .. } if property == "xs"),
                "the indexed value is `this.xs`, got {object:?}"
            );
            assert!(
                matches!(&*index, Expr::Variable(name, _) if name == "this"),
                "the index is the variable `this`, got {index:?}"
            );
        }
        other => panic!("`this.xs at this` is not an index: {other:?}"),
    }
}

/// `this.xs at this` outside a method parses as one index too, and is refused
/// by the analyzer rather than the parser.
///
/// The tree is the same in free code and in a method — what differs is whether
/// `this` is bound. That is what makes the free-code case the honest control:
/// pre-fix it was an index-looking split, post-fix it is an index whose operand
/// is a name nothing binds.
#[test]
fn a_worded_index_of_this_in_free_code_is_parsed_and_refused_by_the_analyzer() {
    let source = "set xs to [1, 2, 3]\nsay xs at this\n";
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("the program should lex");
    let program = redblue::parser::parse(tokens).expect("the program should parse");

    let mut statements = program.statements;
    assert_eq!(
        statements.len(),
        2,
        "`set` and one `say`, as written:\n{source}"
    );
    let say = statements.remove(1);
    match say.statement {
        Statement::Say(Expr::Index { index, .. }) => assert!(
            matches!(&*index, Expr::Variable(name, _) if name == "this"),
            "the index is the variable `this`, got {index:?}"
        ),
        other => panic!("`xs at this` in free code is not an index: {other:?}"),
    }
}

/// An object whose method writes `length of this`, over zero, one and three
/// fields, and the same three through the bytecode engine.
///
/// Zero is the singleton boundary from the other side: an object with no fields
/// is the smallest `this` there is, and it must reach the runtime check rather
/// than the parser's refusal. One field is the singleton row. Three is the
/// ordinary case, and the count of fields is exactly what the message must not
/// depend on — all three have to reach the *same* check.
#[test]
fn edge_length_of_this_reaches_the_runtime_check_for_zero_one_and_many_fields() {
    let cases = [
        ("zero", ""),
        ("one", "    has a\n"),
        ("many", "    has a\n    has b\n    has c\n"),
    ];

    for (name, fields) in cases {
        // `to can` is object-method syntax, so the method is written inside the
        // body: a `to can` after the closing `end` is a function declaration whose
        // name the lexer reserved, which would be a different failure entirely.
        let source = format!(
            "object B\n{fields}    to can size()\n        give back length of this\n    \
             end\nend\nsay B.size()\n"
        );

        let walked = tree_walk(&source);
        assert_eq!(
            failure_label(&walked),
            "RuntimeError",
            "{name} fields: `length of this` must reach the runtime check, not the \
             parser\n{source}"
        );
        assert_eq!(
            failure_message(&walked),
            "length requires a list or text",
            "{name} fields: `length of this` must reach the same check `length of \
             nothing` reaches, whatever the field count is\n{source}"
        );
        both_engines_agree(&source);
    }
}

/// `length of this` and `length of nothing` are refused by the same rule.
///
/// This is the control that names what the fix bought: the two operands are of
/// different token kinds and reach the same runtime check, so the parser is no
/// longer what separates them.
#[test]
fn edge_length_of_this_and_length_of_nothing_are_refused_by_the_same_rule() {
    let source = "object B\n    has a\n    to can size()\n        give back length of nothing\n    end\nend\nsay B.size()\n";
    assert_eq!(
        failure_message(&tree_walk(source)),
        "length requires a list or text",
        "`length of nothing` inside a method still reaches the runtime check"
    );
    both_engines_agree(source);
}

/// `this.xs at this` is an index with a bad index, not a stray variable.
///
/// The pre-fix failure was `AnalyzerError: Unknown variable 'at'; did you mean
/// 'as'?` at the `at`, which is the parse split this phase exists to end. The
/// answer now has to be the runtime check the bracket spelling already gave for
/// a record index — and it has to be an **Analyzer** failure never: `at` is not
/// a variable this program wrote, so `Unknown variable 'at'` in any spelling is
/// the regression being pinned.
#[test]
fn edge_this_as_the_operand_of_at_is_an_index_with_a_bad_index() {
    let source = "object B\n    has xs default [10, 20, 30]\n    to can pick()\n        give back this.xs at this\n    end\nend\nsay B.pick()\n";

    let walked = tree_walk(source);
    let message = failure_message(&walked);
    assert_eq!(
        failure_label(&walked),
        "RuntimeError",
        "`this.xs at this` must be an index that fails at run time, not a parse \
         split the analyzer then reports\n{source}"
    );
    assert_eq!(
        message, "Index must be a number",
        "`this.xs at this` must reach the check `xs[k]` reaches for a record \
         index\n{source}"
    );
    assert!(
        !message.contains("Unknown variable"),
        "the message must never name a variable: {message}"
    );

    both_engines_agree(source);
}

/// The message must place the failure where the writer wrote it, on both
/// engines.
///
/// `this` is at column 27 of the `give back` line, and a failure with no
/// position is not a positioned failure.
#[test]
fn edge_this_as_the_operand_of_at_reports_a_position() {
    let source = "object B\n    has xs default [10, 20, 30]\n    to can pick()\n        give back this.xs at this\n    end\nend\nsay B.pick()\n";

    let walked = tree_walk(source);
    assert_eq!(
        failure_line(&walked),
        Some(4),
        "the failure belongs to the `give back` line the writer wrote:\n{source}"
    );
    both_engines_agree(source);
}

/// `this` as the operand of `at` inside a nested object, and inside a function
/// literal written in a method — the two shapes where a name in scope can go
/// missing.
#[test]
fn edge_this_is_the_operand_of_at_in_a_nested_object_and_a_function_literal() {
    let nested = "object Outer\n    has xs default [1, 2, 3]\n    object Inner\n        has ys default [4, 5]\n        to can pick()\n            give back this.ys at this\n        end\n    end\nend\nsay Outer.Inner.pick()\n";
    let walked = tree_walk(nested);
    assert_eq!(
        failure_label(&walked),
        "RuntimeError",
        "`this` as an `at` operand inside a nested object must reach the runtime \
         check:\n{nested}"
    );
    both_engines_agree(nested);

    let literal = "object B\n    has xs default [7, 8, 9]\n    to can pick()\n        set f to to ()\n            give back this.xs at this\n        end\n        give back f()\n    end\nend\nsay B.pick()\n";
    let walked = tree_walk(literal);
    assert_eq!(
        failure_label(&walked),
        "RuntimeError",
        "`this` as an `at` operand inside a function literal in a method must \
         reach the runtime check:\n{literal}"
    );
    both_engines_agree(literal);
}

/// The three spellings that already worked keep their behaviour, and each is
/// pinned separately so a change to one names itself.
///
/// These three reach `at` through different paths — a method receiver through
/// the postfix chain, a free name through the same chain, and a bracket — and
/// only the first two consult the operand table's *neighbour*, `at_index_ahead`.
#[test]
fn edge_the_index_spellings_that_already_worked_keep_their_behaviour() {
    let cases = [
        (
            "this.xs at 0",
            "object B\n    has xs default [10, 20, 30]\n    to can pick()\n        give back this.xs at 0\n    end\nend\nsay B.pick()\n",
            "10",
        ),
        ("xs at 0", "set xs to [10, 20, 30]\nsay xs at 0\n", "10"),
        ("xs[0]", "set xs to [10, 20, 30]\nsay xs[0]\n", "10"),
        (
            "length of a name",
            "set xs to [10, 20, 30]\nsay length of xs\n",
            "3",
        ),
        (
            "this.xs at 0 as the last element",
            "object B\n    has xs default [10, 20, 30]\n    to can pick()\n        give back this.xs at 2\n    end\nend\nsay B.pick()\n",
            "30",
        ),
    ];

    for (name, source, expected) in cases {
        let walked = tree_walk(source);
        assert_eq!(
            walked.output,
            vec![expected.to_string()],
            "{name} must keep printing {expected}:\n{source}"
        );
        both_engines_agree(source);
    }
}

/// The boundary row for a worded operand that is `this`: `this` at the front of
/// a list is an index of `0` at run time, which is the row that says the new
/// operand is read as an index and not as a name.
#[test]
fn edge_this_xs_at_this_reports_index_must_be_a_number_on_both_engines() {
    let source = "object B\n    has xs default []\n    to can pick()\n        give back this.xs at this\n    end\nend\nsay B.pick()\n";
    for outcome in [tree_walk(source), bytecode(source)] {
        assert_eq!(
            failure_message(&outcome),
            "Index must be a number",
            "an empty list indexed by a record is still an index by a non-number:\n{source}"
        );
    }
}

/// Reaching for `this` where nothing binds it is still an error, and it is
/// *this* error.
///
/// The fix widened a parse table, so the row that must not move is the one
/// where `this` is not bound at all: `xs at this` in free code must still be
/// refused as the unknown variable it is, not read as an index.
#[test]
fn edge_this_as_an_at_operand_outside_a_method_is_still_an_unknown_variable() {
    let source = "set xs to [1, 2, 3]\nsay xs at this\n";

    let walked = tree_walk(source);
    assert_eq!(
        failure_label(&walked),
        "AnalyzerError",
        "`xs at this` in free code is an index whose operand names nothing:\n{source}"
    );
    assert_eq!(
        failure_message(&walked),
        "Unknown variable 'this'",
        "the operand is read as `this` and then found unbound, which is the \
         honest diagnosis:\n{source}"
    );
    both_engines_agree(source);
}

/// **The two operand tables list the same token kinds, and a test says so.**
///
/// This is the assertion that could have caught the finding. Both engines made
/// the same mistake about `this`, so every differential and byte-identity test
/// in the suite stayed green: a table that omits a token does not fail a
/// compilation, it compiles to a *different program*, and the only evidence that
/// says so is a program nobody wrote. The tables are therefore compared as text
/// — one list from the Rust `matches!` arms, one from the Redblue `is` tests —
/// so a token added to one and not the other is a failing test rather than a
/// reader's diligence.
#[test]
fn edge_the_two_operand_tables_name_the_same_token_kinds() {
    let rust = rust_operand_table();
    let redblue = redblue_operand_table();

    assert!(
        !rust.is_empty() && !redblue.is_empty(),
        "a table read as nothing is not agreement: rust={rust:?} redblue={redblue:?}"
    );
    assert_eq!(
        rust, redblue,
        "the operand tables differ. `this` was in neither until this phase added \
         it to both, and a token in one and not the other is a program the two \
         engines read differently"
    );
}

/// The bounds check still answers when the receiver is `this`.
///
/// Adding a token to the operand table cannot change what an index *does* — and
/// the row that says so is a worded index whose index is past the end, read off
/// a receiver reached through `this`. The message names the index, the length
/// and the valid range, which is what makes it an assertion rather than a smoke
/// test.
#[test]
fn edge_a_worded_index_off_this_past_the_end_is_a_clean_runtime_error() {
    let cases = [
        (
            "past the end",
            "object B\n    has xs default [10, 20, 30]\n    to can pick()\n        give back this.xs at 99\n    end\nend\n",
            Some("Index 99 is out of bounds: length is 3, valid indexes are 0 to 2"),
        ),
        (
            "the empty list",
            "object B\n    has xs default []\n    to can pick()\n        give back this.xs at 0\n    end\nend\n",
            Some("Index 0 is out of bounds: length is 0, the list is empty, so it has no valid index"),
        ),
        (
            "the last element",
            "object B\n    has xs default [10, 20, 30]\n    to can pick()\n        give back this.xs at 2\n    end\nend\n",
            None,
        ),
    ];

    for (name, program, expected) in cases {
        let source = format!("{program}say B.pick()\n");
        let walked = tree_walk(&source);

        match expected {
            Some(expected) => {
                assert_eq!(
                    failure_label(&walked),
                    "RuntimeError",
                    "{name}: a worded index off `this` is checked at run time:\n{source}"
                );
                assert_eq!(
                    failure_message(&walked),
                    expected,
                    "{name}: the bounds message names the index, the length and the \
                     range\n{source}"
                );
            }
            None => assert_eq!(
                walked.output,
                vec!["30".to_string()],
                "{name}: the last element of three is in range:\n{source}"
            ),
        }
        both_engines_agree(&source);
    }
}

/// A worded form with nothing to measure is still refused, by the parser.
///
/// The operand table was widened by one token, and the row that must not move
/// with it is the one where the operand is **absent**: `length of` at the end of
/// a line has to be `Expected something to measure` — not a read of whatever
/// token follows, and not a silent `nothing`.
#[test]
fn edge_a_worded_length_with_no_operand_is_still_refused_by_the_parser() {
    let cases = [
        ("free code", "say length of\n"),
        (
            "inside a method",
            "object B\n    to can n()\n        say length of\n    end\nend\nsay B.n()\n",
        ),
    ];

    for (name, source) in cases {
        let walked = tree_walk(source);
        assert_eq!(
            failure_label(&walked),
            "ParserError",
            "{name}: a `length of` with nothing after it is a parse failure, as it \
             always was:\n{source}"
        );
        assert_eq!(
            failure_message(&walked),
            "Expected something to measure after `length of`",
            "{name}: the refusal names what was missing:\n{source}"
        );
        both_engines_agree(source);
    }
}

/// `this` is in both operand tables — stated separately from the parity test, so
/// the answer is a name and not a list of names.
#[test]
fn edge_this_is_in_both_operand_tables() {
    assert!(
        rust_operand_table().contains(&"this".to_string()),
        "TokenKind::This is not in kind_starts_operand (src/parser.rs), so \
         `length of this` and `xs at this` are still refused by the parser"
    );
    assert!(
        redblue_operand_table().contains(&"this".to_string()),
        "`this` is not in is_operand_start (bootstrap/compiler.rb), so stage 2 \
         still refuses what stage 1 reads"
    );
}

/// `might fail` is still not an operand, and `this` did not bring the operand
/// table up to the expression table.
///
/// The two tables differ by exactly one entry, `might fail`, and this is what
/// keeps them two tables rather than one: a worded operand is read at the
/// additive level, where a guard is not readable. Widening the operand table is
/// the phase-080 mistake this test is standing in front of.
#[test]
fn edge_the_operand_table_is_still_the_expression_table_less_might_fail() {
    assert!(
        !rust_operand_table().contains(&"mightfail".to_string()),
        "`might fail` is not a worded operand: `xs at might fail f()` is a parse \
         error on both engines"
    );
    assert!(
        !redblue_operand_table().contains(&"mightfail".to_string()),
        "`might fail` is not a worded operand on stage 2 either"
    );
}
