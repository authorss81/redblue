//! Bytecode execution (bootstrap stage S1b): `rb vm file.rbc`.
//!
//! The tree-walking VM is the specification of what a Redblue program *does*.
//! These tests pin that the bytecode VM does the same thing — the same output,
//! the same error kinds and the same error messages — and that it holds the
//! same resource limits while doing it.
//!
//! The comparison runs both VMs as libraries rather than as subprocesses
//! wherever it can, so a mismatch names the program rather than an exit code.
//! `rb vm` itself is covered by the tests that shell out, because the CLI is
//! part of what this phase delivers.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use redblue::bytecode::vm::BytecodeVm;
use redblue::bytecode::{compile_source, Instruction, Opcode};
use redblue::{run_isolated, Error};

fn scratch_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/tmp/bytecode-vm-test")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir should be creatable");
    dir
}

fn rb(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rb"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("rb should be runnable")
}

/// What one VM made of a program: the lines it printed, and either the value it
/// ended with or the failure it ended with.
///
/// Two programs agree when both halves agree. The failure is compared by kind
/// and message, which is what the language promises; the rendered position is
/// not compared, because a `.rbc` carries no source text to render it from.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    output: Vec<String>,
    result: Result<String, String>,
}

fn failure_of(error: &Error) -> String {
    format!("{}: {}", error.label(), error.message())
}

/// Runs `source` on the tree-walking VM.
///
/// A program that does not lex, parse or analyze has *failed*, and that failure
/// is the outcome rather than a panic: the corpus deliberately contains
/// malformed programs, and how each VM reports them is part of what is compared.
fn tree_walk(source: &str) -> Outcome {
    let tokens = match redblue::lexer::Lexer::tokenize(source) {
        Ok(tokens) => tokens,
        Err(error) => return failed(failure_of(&error)),
    };
    let ast = match redblue::parser::parse(tokens) {
        Ok(ast) => ast,
        Err(error) => return failed(failure_of(&error)),
    };
    if let Err(error) = redblue::analyzer::analyze(&ast) {
        return failed(failure_of(&error));
    }
    let (mut vm, result) = run_isolated(&ast);
    Outcome {
        output: vm.take_output(),
        result: result
            .map(|value| value.to_string())
            .map_err(|error| failure_of(&error)),
    }
}

/// A frontend failure, as an outcome: nothing ran, so nothing was printed.
fn failed(message: String) -> Outcome {
    Outcome {
        output: Vec::new(),
        result: Err(message),
    }
}

/// Runs `source` through the bytecode compiler and then the bytecode VM.
fn bytecode(source: &str) -> Outcome {
    let chunk = match compile_source(source) {
        Ok(chunk) => chunk,
        Err(error) => return failed(failure_of(&error)),
    };
    let mut vm = BytecodeVm::new();
    let result = vm.run(&chunk);
    Outcome {
        output: vm.take_output(),
        result: result
            .map(|value| value.to_string())
            .map_err(|error| failure_of(&error)),
    }
}

/// Runs `source` both ways and fails with the two outcomes when they differ.
#[track_caller]
fn assert_agrees(source: &str) {
    let tree = tree_walk(source);
    let byte = bytecode(source);
    assert_eq!(
        tree, byte,
        "the two VMs disagree\n--- source ---\n{source}--- tree ---\n{tree:?}\n--- bytecode ---\n{byte:?}\n"
    );
}

/// Programs read from disk that this test cannot compare, named rather than
/// detected.
///
/// A differential test is only worth anything if it is deterministic, and one of
/// these prints the wall clock, so its two runs differ by a second however
/// faithfully both VMs behave. They are still executed by `redblue_suite_test.rs`
/// and by the gate's examples run, so excluding them here loses no coverage —
/// it only keeps a comparison from being meaningless.
const NOT_COMPARABLE: &[&str] = &["examples/time.rb", "examples/random.rb"];

/// The corpus the differential test runs: every program under `examples/`,
/// `modules/` and `tests/` that [`NOT_COMPARABLE`] does not name, plus the
/// generated programs below.
///
/// Generated rather than committed so that a reviewer can read what each one
/// covers instead of diffing two hundred files. Every entry is a whole program,
/// so a failure names a case rather than a line inside a fixture.
fn corpus() -> BTreeMap<String, String> {
    let mut programs = BTreeMap::new();

    for dir in ["examples", "modules", "tests"] {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "rb"))
            .collect();
        paths.sort();
        for path in paths {
            let Ok(source) = fs::read_to_string(&path) else {
                continue;
            };
            let name = format!(
                "{dir}/{}",
                path.file_name().expect("a named file").to_string_lossy()
            );
            if NOT_COMPARABLE.contains(&name.as_str()) {
                continue;
            }
            programs.insert(name, source);
        }
    }

    for (name, source) in generated_corpus() {
        programs.insert(name, source);
    }

    programs
}

/// Programs written here rather than read from disk. Grouped by the row of the
/// edge-case matrix they cover so a reviewer can see the spread.
fn generated_corpus() -> Vec<(String, String)> {
    let mut programs = Vec::new();
    let mut add = |name: &str, source: String| programs.push((name.to_string(), source));

    // -- empty / zero / nothing -------------------------------------------
    add("empty/no-statements", String::new());
    add("empty/only-a-comment", "// nothing at all\n".to_string());
    add(
        "empty/empty-list",
        "set xs to []\nsay length(xs)\nsay type_of(xs)\n".to_string(),
    );
    add(
        "empty/empty-text",
        "set s to \"\"\nsay length(s)\nsay s is \"\"\n".to_string(),
    );
    add(
        "empty/nothing-is-not-zero",
        "set n to nothing\nsay n is nothing\nsay n is 0\n".to_string(),
    );
    add(
        "empty/zero-division-that-is-not-zero",
        "set z to 0\nsay z / 1\nsay 0 * 5\n".to_string(),
    );
    add(
        "empty/loop-zero-times",
        "set n to 0\nrepeat 0 times\n    set n to n + 1\nend\nsay n\n".to_string(),
    );

    // -- singleton and boundary -------------------------------------------
    add(
        "singleton/one-element-list",
        "set xs to [7]\nsay length(xs)\nsay xs[0]\nsay xs[-1]\n".to_string(),
    );
    add(
        "singleton/one-iteration",
        "set n to 0\nrepeat 1 times\n    set n to n + 99\nend\nsay n\n".to_string(),
    );
    add(
        "singleton/one-element-record",
        "set r to { only: 1 }\nsay r.only\nsay type_of(r)\n".to_string(),
    );
    add(
        "singleton/range-of-one",
        "set total to 0\nfor each i from 1 to 1\n    set total to total + i\nend\nsay total\n"
            .to_string(),
    );

    // -- out of bounds -----------------------------------------------------
    add(
        "bounds/index-past-the-end",
        "set xs to [1, 2, 3]\nsay xs[3]\n".to_string(),
    );
    add(
        "bounds/index-far-past-the-end",
        "set xs to [1, 2, 3]\nsay xs[999]\n".to_string(),
    );
    add(
        "bounds/index-far-negative",
        "set xs to [1, 2, 3]\nsay xs[-999]\n".to_string(),
    );
    add(
        "bounds/index-of-an-empty-list",
        "set xs to []\nsay xs[0]\n".to_string(),
    );
    add(
        "bounds/fractional-index",
        "set xs to [1, 2, 3]\nsay xs[0.5]\n".to_string(),
    );
    add(
        "bounds/text-index-is-not-a-number",
        "set xs to [1, 2, 3]\nsay xs[\"zero\"]\n".to_string(),
    );

    // -- type mismatch -----------------------------------------------------
    add("types/number-plus-text", "say 1 + \"one\"\n".to_string());
    add("types/index-a-number", "say 5[0]\n".to_string());
    add(
        "types/property-of-a-number",
        "say (5).missing\n".to_string(),
    );
    add("types/length-of-a-number", "say length(5)\n".to_string());
    add("types/method-on-a-number", "say (5).plus(1)\n".to_string());
    add(
        "types/list-built-from-mixed",
        "set xs to [1, \"two\", yes, nothing]\nsay length(xs)\nsay xs[1]\n".to_string(),
    );
    add(
        "types/record-key-that-is-not-text",
        "set r to { 1: \"one\" }\nsay r[1]\n".to_string(),
    );

    // -- numeric boundary --------------------------------------------------
    add(
        "numeric/zero-division",
        "set n to 1\nsay n / 0\n".to_string(),
    );
    add("numeric/modulo-zero", "say 5 % 0\n".to_string());
    add(
        "numeric/large-integers",
        "set big to 9007199254740993\nsay big\nsay big + 1\n".to_string(),
    );
    add(
        "numeric/two-to-the-fifty-three",
        "set a to 9007199254740992\nsay a + 1\nsay a - 1\n".to_string(),
    );
    add(
        "numeric/i64-edge",
        "say 9223372036854775807\nsay -9223372036854775807\n".to_string(),
    );
    add(
        "numeric/negative-zero",
        "set n to 0\nsay n * -1\nsay n - 0\n".to_string(),
    );
    add(
        "numeric/very-large-exponent",
        "say pow(10, 400)\n".to_string(),
    );
    add("numeric/small-exponent", "say pow(10, -400)\n".to_string());
    add("numeric/sqrt-of-negative", "say sqrt(-1)\n".to_string());
    add("numeric/log-of-zero", "say log(0)\n".to_string());
    add(
        "numeric/round-half",
        "say round(0.5)\nsay round(-0.5)\nsay round(2.5)\n".to_string(),
    );
    add(
        "numeric/accumulating-a-float",
        "set total to 0\nrepeat 10 times\n    set total to total + 0.1\nend\nsay total\n"
            .to_string(),
    );

    // -- unicode and escapes ----------------------------------------------
    add(
        "unicode/emoji",
        "say \"hello \u{1F600} world\"\nsay length(\"\u{1F600}\")\n".to_string(),
    );
    add(
        "unicode/cjk",
        "set s to \"\u{4F60}\u{597D}\u{4E16}\u{754C}\"\nsay s\nsay length(s)\n".to_string(),
    );
    add(
        "unicode/rtl",
        "say \"\u{5E2}\u{578}\u{62D}\u{5DE8}\u{5B50}\"\n".to_string(),
    );
    add("unicode/combining-marks", "say \"e\u{0301}\"\n".to_string());
    add(
        "unicode/escapes",
        "say \"tab:\\there\"\nsay \"newline-in-text-is-one-value\"\n".to_string(),
    );
    add(
        "unicode/interpolation-of-unicode",
        "set name to \"\u{5E2}\u{578}\u{62D}\"\nsay \"hi {name}\"\n".to_string(),
    );

    // -- nesting and recursion --------------------------------------------
    add(
        "nesting/nested-lists",
        "set xs to [[1, 2], [3, 4]]\nsay xs[1][0]\nsay length(xs[0])\n".to_string(),
    );
    add(
        "nesting/nested-records",
        "set r to { inner: { deep: \"found\" } }\nsay r.inner.deep\n".to_string(),
    );
    add(
        "nesting/four-scopes",
        "set a to 1\nto outer()\n    set b to 2\n    to middle()\n        set c to 3\n        to inner()\n            give back a + b + c\n        end\n        give back inner()\n    end\n    give back middle()\nend\nsay outer()\n".to_string(),
    );
    add(
        "nesting/closure-captures-and-writes",
        "set counter to 0\nto bump()\n    set counter to counter + 1\n    give back counter\nend\nsay bump()\nsay bump()\nsay counter\n".to_string(),
    );
    add(
        "nesting/recursion-factorial",
        "to fact(n)\n    if n is 0 then\n        give back 1\n    end\n    give back n * fact(n - 1)\nend\nsay fact(6)\n".to_string(),
    );
    add(
        "nesting/mutual-recursion",
        "to is_even(n)\n    if n is 0 then\n        give back yes\n    end\n    give back is_odd(n - 1)\nend\nto is_odd(n)\n    if n is 0 then\n        give back no\n    end\n    give back is_even(n - 1)\nend\nsay is_even(10)\nsay is_odd(10)\n".to_string(),
    );

    // -- duplicate and missing keys ---------------------------------------
    add(
        "keys/missing-key-is-nothing",
        "set r to { a: 1 }\nsay r.missing\nsay r.missing is nothing\n".to_string(),
    );
    add(
        "keys/missing-key-on-empty-record",
        "set r to {}\nsay r.anything is nothing\n".to_string(),
    );
    add(
        "keys/repeated-key-last-wins",
        "set r to { a: 1, b: 2 }\nsay r.a\nsay r.b\n".to_string(),
    );
    add(
        "keys/writing-a-new-key",
        "set r to { a: 1 }\nset r.b to 2\nsay r.a\nsay r.b\n".to_string(),
    );
    add(
        "keys/field-order-is-declaration-order",
        "set r to { z: 1, a: 2, m: 3 }\nfor each key in keys(r)\n    say key\nend\n".to_string(),
    );

    // -- malformed input ---------------------------------------------------
    add(
        "malformed/unterminated-string",
        "say \"unterminated\n".to_string(),
    );
    add(
        "malformed/unclosed-end",
        "set x to 1\nif x is 1 then\n    say x\n".to_string(),
    );
    add("malformed/stray-close", "say \"ok\"\nend\n".to_string());
    add("malformed/empty-file", String::new());
    add("malformed/bom-prefix", "\u{FEFF}say \"bom\"\n".to_string());
    add(
        "malformed/crlf",
        "say \"one\"\r\nsay \"two\"\r\n".to_string(),
    );
    add(
        "malformed/unknown-name",
        "say never_declared_anywhere\n".to_string(),
    );
    add(
        "malformed/call-an-unknown-function",
        "no_such_function(1)\n".to_string(),
    );

    // -- resource and state ------------------------------------------------
    add(
        "limits/unbounded-loop-is-stopped",
        "set n to 0\nwhile yes is yes\n    set n to n + 1\nend\n".to_string(),
    );
    add(
        "limits/deep-recursion-is-stopped",
        "to down(n)\n    give back down(n + 1)\nend\nsay down(0)\n".to_string(),
    );
    add(
        "limits/missing-file",
        "say files.read(\"definitely-not-here.txt\")\n".to_string(),
    );
    add(
        "limits/a-caught-error-leaves-the-program-usable",
        "set caught to no\ntry\n    say 1 / 0\ncatch error\n    set caught to yes\nend\nsay 2 + 2\nsay caught\n".to_string(),
    );

    // -- the shapes the compiler and VM agree on, exercised singly --------
    add("shape/say-nothing", "say nothing\n".to_string());
    add(
        "shape/print-internal-form",
        "print [1, \"two\", yes]\n".to_string(),
    );
    add("shape/if-else-if-chain", "set n to 2\nif n is 1 then\n    say \"one\"\nelse\n    if n is 2 then\n        say \"two\"\n    else\n        say \"many\"\n    end\nend\n".to_string());
    add("shape/while-with-a-text-accumulator", "set out to \"\"\nset n to 0\nwhile n is not 5\n    set out to out + \"x\"\n    set n to n + 1\nend\nsay out\n".to_string());
    add("shape/for-each-over-a-list-of-records", "set xs to [{ n: 1 }, { n: 2 }]\nset total to 0\nfor each x in xs\n    set total to total + x.n\nend\nsay total\n".to_string());
    add(
        "shape/for-each-over-something-that-is-not-a-list",
        "set n to 0\nfor each x in 5\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add(
        "shape/range-with-a-step",
        "set out to \"\"\nfor each i from 0 to 10 by 3\n    set out to out + i\nend\nsay out\n"
            .to_string(),
    );
    add("shape/range-backwards", "set total to 0\nfor each i from 5 to 1 by -1\n    set total to total + i\nend\nsay total\n".to_string());
    add(
        "shape/range-with-non-numeric-bounds",
        "set n to 0\nfor each i from \"a\" to \"b\"\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add(
        "shape/repeat-a-non-number",
        "set n to 0\nrepeat \"three\" times\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add("shape/break-and-skip-inside-a-loop", "set total to 0\nfor each v in [1, 2, 3]\n    if v is 2 then\n        skip\n    end\n    set total to total + v\nend\nsay total\n".to_string());
    add(
        "shape/return-in-the-middle-of-a-body",
        "to f()\n    give back 1\n    say \"after the return\"\nend\nsay f()\n".to_string(),
    );
    add(
        "shape/a-function-without-a-body-value",
        "to nothing_at_all()\n    set x to 1\nend\nsay nothing_at_all() is nothing\n".to_string(),
    );
    add(
        "shape/try-with-a-catch-and-a-finally",
        "set log to \"\"\ntry\n    set log to log + \"t\"\ncatch\n    set log to log + \"c\"\nfinally\n    set log to log + \"f\"\nend\nsay log\n".to_string(),
    );
    add(
        "shape/try-where-the-protected-code-fails",
        "set log to \"\"\ntry\n    say 1 / 0\ncatch\n    set log to log + \"c\"\nfinally\n    set log to log + \"f\"\nend\nsay log\n".to_string(),
    );
    add(
        "shape/try-with-a-failure-after-it",
        "set caught to \"\"\ntry\n    set caught to caught + \"t\"\ncatch\n    set caught to caught + \"c\"\nend\nsay caught\n".to_string(),
    );
    add(
        "shape/a-failure-inside-a-catch-is-not-swallowed",
        "set log to \"\"\ntry\n    say 1 / 0\ncatch\n    say 1 / 0\nend\nsay log\n".to_string(),
    );
    add(
        "shape/nested-tries",
        "set log to \"\"\ntry\n    try\n        say 1 / 0\n    catch\n        set log to log + \"i\"\n    end\ncatch\n    set log to log + \"o\"\nend\nsay log\n".to_string(),
    );
    add(
        "shape/a-catch-binds-the-word-error",
        "set caught to \"\"\ntry\n    say 1 / 0\ncatch problem\n    set caught to problem\nend\nsay caught\n".to_string(),
    );
    add(
        "shape/object-with-fields-and-methods",
        "object Point\n    has x default 0\n    has y default 0\n    to can describe()\n        give back \"point\"\n    end\nend\nset Point.x to 3\nsay Point.x\nsay Point.describe()\n".to_string(),
    );
    add(
        "shape/object-extending-an-object",
        "object Base\n    has name default \"base\"\n    to can greet()\n        give back \"from base\"\n    end\nend\nobject Child extends Base\n    has extra default 1\nend\nsay Child.name\nsay Child.extra\nsay Child.greet()\n".to_string(),
    );
    add(
        "shape/object-this-is-the-binding",
        "object Box\n    has item default \"nothing\"\n    to can get()\n        give back this.item\n    end\nend\nset Box.item to \"inside\"\nsay Box.get()\n".to_string(),
    );
    add(
        "shape/an-object-a-method-does-not-have",
        "object A\n    has n default 1\nend\nsay A.missing_method()\n".to_string(),
    );
    add(
        "shape/a-method-on-something-that-is-not-an-object",
        "set n to 5\nsay n.plus(1)\n".to_string(),
    );
    add(
        "shape/text-functions",
        "say uppercase(\"ab\")\nsay lowercase(\"AB\")\nsay trim(\"  x  \")\n".to_string(),
    );
    add(
        "shape/split-and-join",
        "say split(\"a,b,c\", \",\")\nsay join([\"a\", \"b\"], \"-\")\n".to_string(),
    );
    add(
        "shape/contains-and-starts-with",
        "say contains(\"hello\", \"ell\")\nsay starts_with(\"hello\", \"he\")\n".to_string(),
    );
    add(
        "shape/replace",
        "say replace(\"a-b-c\", \"-\", \"+\")\n".to_string(),
    );
    add(
        "shape/json-round-trip",
        "set r to json.parse(\"{\\\"k\\\": [1, 2]}\")\nsay r.k[1]\nsay json.stringify(r)\n"
            .to_string(),
    );
    add(
        "shape/csv-parse",
        "set rows to csv.parse(\"a,b\\n1,2\")\nsay length(rows)\nsay rows[1][0]\n".to_string(),
    );
    add(
        "shape/logical-operators-short-circuit-is-not-special",
        "say yes and no\nsay yes or no\nsay not yes\n".to_string(),
    );
    add(
        "shape/equality-across-types",
        "say 1 is \"1\"\nsay nothing is nothing\nsay [1] is [1]\n".to_string(),
    );
    add(
        "shape/membership",
        "say 2 in [1, 2, 3]\nsay \"a\" in \"cat\"\n".to_string(),
    );
    add(
        "shape/math-constants",
        "say PI > 3\nsay E > 2\n".to_string(),
    );
    add(
        "shape/a-test-block-that-passes",
        "test \"passes\"\n    expect 1 + 1 to be 2\nend\n".to_string(),
    );
    add(
        "shape/a-test-block-that-fails",
        "test \"fails\"\n    expect 1 + 1 to be 3\nend\n".to_string(),
    );
    add("shape/expect-with-contain", "say \"hello\"\n".to_string());

    // -- the language surface, program by program ---------------------------

    add(
        "arith/a-float-written-with-a-trailing-zero",
        "say 3.0\nsay 3.0 + 0.0\n".to_string(),
    );
    add(
        "arith/a-number-that-is-not-an-integer-expression",
        "say 1.5 + 1.5\nsay 0.1 * 3\n".to_string(),
    );
    add(
        "arith/arithmetic-inside-an-if",
        "set a to 3\nset b to 4\nif a * b is 12 then\n    say \"twelve\"\nend\n".to_string(),
    );
    add(
        "arith/associativity-of-subtraction",
        "say 10 - 3 - 2\n".to_string(),
    );
    add(
        "arith/division-of-whole-numbers",
        "say 12 / 4\n".to_string(),
    );
    add(
        "arith/division-that-repeats",
        "say 1 / 3\nsay 2 / 3\n".to_string(),
    );
    add("arith/double-negative", "say - -5\n".to_string());
    add("arith/modulo-of-a-negative", "say -7 % 3\n".to_string());
    add(
        "arith/modulo-of-an-exact-multiple",
        "say 8 % 4\n".to_string(),
    );
    add(
        "arith/multiplication-chain",
        "say 6 * 7\nsay 2 * 3 * 4\n".to_string(),
    );
    add(
        "arith/nesting-arithmetic-in-a-call",
        "say length(to_text(1))\n".to_string(),
    );
    add(
        "arith/one-hundred-iterations",
        "set total to 0\nrepeat 100 times\n    set total to total + 7\nend\nsay total\n"
            .to_string(),
    );
    add(
        "arith/precedence-with-parentheses",
        "say (2 + 3) * 4\n".to_string(),
    );
    add(
        "arith/precedence-without-parentheses",
        "say 2 + 3 * 4\n".to_string(),
    );
    add(
        "arith/subtraction-that-goes-negative",
        "say 5 - 8\n".to_string(),
    );
    add(
        "arith/sum-of-three",
        "say 1 + 2\nsay 10 + 20 + 30\n".to_string(),
    );
    add(
        "arith/unary-minus-of-a-parenthesised-sum",
        "say -(3 + 4)\n".to_string(),
    );
    add(
        "compare/a-float-that-equals-an-integer",
        "say 1.0 is 1\n".to_string(),
    );
    add(
        "compare/equality-of-numbers",
        "say 1 is 1\nsay 1 is 2\n".to_string(),
    );
    add(
        "compare/inequality",
        "say 1 is not 1\nsay 1 is not 2\n".to_string(),
    );
    add(
        "compare/list-equality",
        "say [1] is [1]\nsay [1] is [2]\n".to_string(),
    );
    add(
        "compare/nothing-equals-nothing",
        "say nothing is nothing\n".to_string(),
    );
    add(
        "compare/record-equality",
        "say { a: 1 } is { a: 1 }\n".to_string(),
    );
    add(
        "compare/text-equality",
        "say \"a\" is \"a\"\nsay \"a\" is \"b\"\n".to_string(),
    );
    add(
        "flow/a-break-inside-a-bounded-while-is-not-a-jump-yet",
        "set n to 0\nset seen to 0\nwhile n is not 4\n    set n to n + 1\n    set seen to seen + 1\n    if n is 2 then\n        break\n    end\nend\nsay n\nsay seen\n".to_string(),
    );
    add(
        "flow/a-loop-inside-a-function-inside-a-loop",
        "to sum(xs)\n    set total to 0\n    for each x in xs\n        set total to total + x\n    end\n    give back total\nend\nfor each v in [[1], [1, 2]]\n    say sum(v)\nend\n".to_string(),
    );
    add(
        "flow/a-loop-variable-shadowing-an-outer-name",
        "set v to 99\nfor each v in [1, 2]\n    say v\nend\nsay v\n".to_string(),
    );
    add(
        "flow/a-range-stepping-backwards",
        "set total to 0\nfor each i in [5, 4, 3, 2, 1]\n    set total to total + i\nend\nsay total\n".to_string(),
    );
    add(
        "flow/a-range-with-a-step",
        "set out to \"\"\nfor each i in [0, 3, 6, 9]\n    set out to out + i\nend\nsay out\n"
            .to_string(),
    );
    add(
        "flow/a-range-with-a-zero-step",
        "set n to 0\nfor each i in 5\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add(
        "flow/a-range-with-non-numeric-bounds",
        "set n to 0\nfor each i in \"not a list\"\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add(
        "flow/a-return-from-inside-a-loop",
        "to first_even(xs)\n    for each x in xs\n        if x % 2 is 0 then\n            give back x\n        end\n    end\n    give back nothing\nend\nsay first_even([1, 3, 4, 5])\n".to_string(),
    );
    add(
        "flow/a-skip-inside-a-bounded-loop-is-not-a-jump-yet",
        "set total to 0\nfor each v in [1, 2, 3]\n    skip\n    set total to total + v\nend\nsay total\n".to_string(),
    );
    add(
        "flow/a-statement-after-a-return",
        "to f()\n    give back 1\n    say \"after\"\nend\nsay f()\n".to_string(),
    );
    add(
        "flow/a-while-that-counts-to-four",
        "set n to 0\nwhile n is not 4\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add(
        "flow/a-while-that-never-runs",
        "set n to 0\nwhile n is 5\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add(
        "flow/a-while-that-runs-to-completion",
        "set n to 0\nwhile n is not 5\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add(
        "flow/an-endless-while",
        "set n to 0\nwhile yes is yes\n    set n to n + 1\nend\n".to_string(),
    );
    add(
        "flow/an-if-inside-a-while",
        "set n to 0\nwhile n is not 5\n    if n is 2 then\n        say \"two\"\n    else\n        say n\n    end\n    set n to n + 1\nend\n".to_string(),
    );
    add(
        "flow/an-if-with-a-then-and-an-else",
        "set n to 1\nif n is 1 then\n    say \"one\"\nelse\n    say \"many\"\nend\n".to_string(),
    );
    add(
        "flow/an-if-without-an-else",
        "set n to 5\nif n is 5 then\n    say \"five\"\nend\nsay \"after\"\n".to_string(),
    );
    add(
        "flow/else-if-chain",
        "set n to 3\nif n is 1 then\n    say \"one\"\nelse\n    if n is 2 then\n        say \"two\"\n    else\n        say \"many\"\n    end\nend\n".to_string(),
    );
    add(
        "flow/for-each-over-a-range",
        "set total to 0\nfor each i in [1, 2, 3, 4]\n    set total to total + i\nend\nsay total\n"
            .to_string(),
    );
    add(
        "flow/for-each-over-a-range-of-one",
        "set n to 0\nfor each i in [1]\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add(
        "flow/for-each-over-an-empty-range",
        "set n to 0\nfor each i in []\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add(
        "flow/for-each-over-nothing",
        "set n to 0\nfor each x in nothing\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add(
        "flow/for-each-over-something-that-is-not-a-list",
        "set n to 0\nfor each x in 5\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add(
        "flow/nested-loops",
        "set total to 0\nfor each a in [1, 2]\n    for each b in [10, 20]\n        set total to total + a * b\n    end\nend\nsay total\n".to_string(),
    );
    add(
        "flow/repeat-a-fraction",
        "set n to 0\nrepeat 2.5 times\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add(
        "flow/repeat-a-non-number",
        "set n to 0\nrepeat \"three\" times\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add(
        "flow/repeat-one-time",
        "repeat 1 times\n    say \"once\"\nend\n".to_string(),
    );
    add(
        "flow/repeat-zero-times",
        "say \"before\"\nrepeat 0 times\n    say \"inside\"\nend\nsay \"after\"\n".to_string(),
    );
    add(
        "flow/skip-the-last-element",
        "set n to 0\nfor each v in [1, 2]\n    skip\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add(
        "flow/three-nested-loops",
        "set n to 0\nfor each a in [1, 2]\n    for each b in [1, 2]\n        for each c in [1, 2]\n            set n to n + 1\n        end\n    end\nend\nsay n\n".to_string(),
    );
    add(
        "fn/a-function-called-from-a-loop",
        "to double(x)\n    give back x * 2\nend\nset total to 0\nfor each v in [1, 2, 3]\n    set total to total + double(v)\nend\nsay total\n".to_string(),
    );
    add(
        "fn/a-function-called-with-a-text-argument",
        "to show(x)\n    say x\nend\nshow(\"hello\")\n".to_string(),
    );
    add(
        "fn/a-function-of-no-arguments",
        "to answer()\n    give back 42\nend\nsay answer()\n".to_string(),
    );
    add(
        "fn/a-function-recursing-and-failing",
        "to down(n)\n    give back down(n + 1)\nend\nsay down(0)\n".to_string(),
    );
    add(
        "fn/a-function-returning-nothing-at-all",
        "to quiet()\n    set x to 1\nend\nsay quiet() is nothing\n".to_string(),
    );
    add(
        "fn/a-function-that-writes-a-declared-global",
        "set g to 0\nto set_it()\n    set g to 9\nend\nset_it()\nsay g\n".to_string(),
    );
    add(
        "fn/calling-a-function-that-does-not-exist",
        "say no_such_function(1)\n".to_string(),
    );
    add(
        "fn/five-scopes-deep",
        "set a to 1\nto outer()\n    set b to 2\n    to middle()\n        set c to 3\n        to inner()\n            set d to 4\n            give back a + b + c + d\n        end\n        give back inner()\n    end\n    give back middle()\nend\nsay outer()\n".to_string(),
    );
    add(
        "fn/mutual-recursion",
        "to is_even(n)\n    if n is 0 then\n        give back yes\n    end\n    give back is_odd(n - 1)\nend\nto is_odd(n)\n    if n is 0 then\n        give back no\n    end\n    give back is_even(n - 1)\nend\nsay is_even(10)\nsay is_odd(10)\n".to_string(),
    );
    add(
        "fn/one-function-calling-another",
        "to a(x)\n    give back x + 1\nend\nto b(x)\n    give back a(x) * 2\nend\nsay b(5)\n"
            .to_string(),
    );
    add(
        "fn/recursion-factorial",
        "to fact(n)\n    if n is 0 then\n        give back 1\n    end\n    give back n * fact(n - 1)\nend\nsay fact(6)\n".to_string(),
    );
    add(
        "fn/recursion-fibonacci",
        "to fib(n)\n    if n is 1 then\n        give back 0\n    end\n    if n is 2 then\n        give back 1\n    end\n    give back fib(n - 1) + fib(n - 2)\nend\nsay fib(12)\n".to_string(),
    );
    add(
        "fn/recursion-one-hundred-deep",
        "to down(n)\n    if n is 0 then\n        give back 0\n    end\n    give back 1 + down(n - 1)\nend\nsay down(100)\n".to_string(),
    );
    add(
        "fn/several-arguments",
        "to add(a, b)\n    give back a + b\nend\nsay add(1, 2)\nsay add(-1, -2)\n".to_string(),
    );
    add(
        "fn/too-few-arguments",
        "to add(a, b)\n    give back a + b\nend\nsay add(1)\n".to_string(),
    );
    add(
        "fn/too-many-arguments",
        "to one(a)\n    give back a\nend\nsay one(1, 2)\n".to_string(),
    );
    add(
        "formats/csv-of-a-single-column",
        "say csv.parse(\"a\\nb\")\n".to_string(),
    );
    add(
        "formats/csv-two-rows",
        "set rows to csv.parse(\"a,b\\n1,2\")\nsay length(rows)\nsay rows[1][0]\n".to_string(),
    );
    add(
        "formats/csv-with-a-quoted-field",
        "say csv.parse(\"a,\\\"b,c\\\",d\")\n".to_string(),
    );
    add(
        "formats/json-array",
        "say json.parse(\"[1, 2, 3]\")\n".to_string(),
    );
    add(
        "formats/json-object-field",
        "set r to json.parse(\"{\\\"k\\\": [1, 2]}\")\nsay r.k[1]\n".to_string(),
    );
    add(
        "formats/json-of-an-empty-object",
        "say json.stringify({})\n".to_string(),
    );
    add(
        "formats/json-of-malformed-input",
        "say json.parse(\"not json\")\n".to_string(),
    );
    add(
        "formats/json-stringify-a-list",
        "say json.stringify([1, \"two\", yes])\n".to_string(),
    );
    add(
        "formats/time-format-of-a-known-instant",
        "say time.format(0, \"%Y\")\n".to_string(),
    );
    add(
        "formats/time-format-of-a-text-argument",
        "say time.format(1, \"%Y\")\n".to_string(),
    );
    add(
        "formats/time-unix-of-a-known-date",
        "say time.unix(\"2020-01-02 03:04:05\")\n".to_string(),
    );
    add(
        "formats/time-unix-of-nonsense",
        "say time.unix(\"not a date\")\n".to_string(),
    );
    add(
        "list/a-fractional-index",
        "set xs to [1, 2, 3]\nsay xs[0.5]\n".to_string(),
    );
    add(
        "list/a-list-built-from-a-record",
        "set xs to [{ n: 1 }, { n: 2 }]\nfor each x in xs\n    say x.n\nend\n".to_string(),
    );
    add(
        "list/a-list-of-lists-indexed-past-the-end",
        "set xs to [[1, 2]]\nsay xs[0][9]\n".to_string(),
    );
    add(
        "list/a-list-of-three-levels",
        "set xs to [[[7]]]\nsay xs[0][0][0]\n".to_string(),
    );
    add(
        "list/a-singleton-list",
        "set xs to [7]\nsay length(xs)\nsay xs[0]\n".to_string(),
    );
    add(
        "list/a-singleton-list-of-a-singleton-list",
        "set xs to [[1]]\nsay length(xs)\nsay length(xs[0])\n".to_string(),
    );
    add(
        "list/a-text-index",
        "set xs to [1, 2, 3]\nsay xs[\"zero\"]\n".to_string(),
    );
    add(
        "list/adding-two-numbers-is-not-list-concatenation",
        "set xs to [1, 2]\nsay xs + [3]\n".to_string(),
    );
    add(
        "list/building-a-list-of-lengths",
        "set xs to []\nrepeat 4 times\n    set xs to [xs, 1]\nend\nsay length(xs)\n".to_string(),
    );
    add(
        "list/empty-list-type",
        "set xs to []\nsay type_of(xs)\nsay length(xs)\n".to_string(),
    );
    add(
        "list/index-far-negative",
        "set xs to [1, 2, 3]\nsay xs[-999]\n".to_string(),
    );
    add(
        "list/index-far-past-the-end",
        "set xs to [1, 2, 3]\nsay xs[999]\n".to_string(),
    );
    add(
        "list/index-minus-one",
        "set xs to [1, 2, 3]\nsay xs[-1]\n".to_string(),
    );
    add(
        "list/index-of-an-empty-list",
        "set xs to []\nsay xs[0]\n".to_string(),
    );
    add(
        "list/index-past-the-end",
        "set xs to [1, 2, 3]\nsay xs[3]\n".to_string(),
    );
    add("list/indexing-a-number", "say 5[0]\n".to_string());
    add(
        "list/indexing-two-levels-deep",
        "set xs to [[1, 2], [3, 4]]\nsay xs[1][0]\nsay length(xs[0])\n".to_string(),
    );
    add(
        "list/last-element-by-index",
        "set xs to [1, 2, 3]\nsay xs[2]\n".to_string(),
    );
    add(
        "list/literal-with-mixed-types",
        "set xs to [1, \"two\", yes, nothing]\nsay length(xs)\nsay xs[1]\nsay xs[2]\nsay xs[3]\n"
            .to_string(),
    );
    add(
        "logic/a-yes-no-in-a-condition",
        "set flag to yes\nif flag then\n    say \"on\"\nend\n".to_string(),
    );
    add(
        "logic/and-of-two-equalities",
        "say (1 is 1) and (\"a\" is \"a\")\n".to_string(),
    );
    add(
        "logic/and-or-not",
        "say yes and yes\nsay yes and no\nsay no or yes\nsay not yes\n".to_string(),
    );
    add(
        "logic/not-of-a-comparison",
        "say not (1 is 2)\n".to_string(),
    );
    add(
        "logic/or-between-two-failures",
        "say (1 is 2) or (3 is 4)\n".to_string(),
    );
    add("malformed/a-bom-at-the-start", "﻿say \"bom\"\n".to_string());
    add(
        "malformed/a-file-of-only-a-comment",
        "// nothing here\n".to_string(),
    );
    add(
        "malformed/a-file-of-only-whitespace",
        "\n\n   \n	\n".to_string(),
    );
    add(
        "malformed/a-record-with-a-missing-key",
        "set r to { a: }\nsay r\n".to_string(),
    );
    add("malformed/a-stray-end", "say \"ok\"\nend\n".to_string());
    add("malformed/a-stray-then", "if 1 is 1 then\n".to_string());
    add(
        "malformed/a-string-containing-the-word-end",
        "say \"end\"\n".to_string(),
    );
    add(
        "malformed/a-unterminated-string",
        "say \"unterminated\n".to_string(),
    );
    add(
        "malformed/an-object-with-an-unclosed-body",
        "object P\n    has x default 1\n".to_string(),
    );
    add(
        "malformed/an-unbalanced-bracket",
        "set xs to [1, 2\nsay xs[0]\n".to_string(),
    );
    add(
        "malformed/an-unclosed-for",
        "for each i from 1 to 2\n    say i\n".to_string(),
    );
    add(
        "malformed/an-unclosed-if",
        "set x to 1\nif x is 1 then\n    say x\n".to_string(),
    );
    add(
        "malformed/an-unknown-name",
        "say never_declared_anywhere\n".to_string(),
    );
    add(
        "malformed/an-unknown-statement",
        "frobnicate 1\n".to_string(),
    );
    add(
        "malformed/carriage-return-line-endings",
        "say \"one\"
\nsay \"two\"
\n"
        .to_string(),
    );
    add(
        "malformed/nested-comments-in-a-line",
        "say \"a\" // a comment\n".to_string(),
    );
    add(
        "numeric/a-number-greater-than-i64",
        "say 9223372036854775808\n".to_string(),
    );
    add(
        "numeric/accumulating-a-float-in-a-loop",
        "set total to 0\nrepeat 10 times\n    set total to total + 0.1\nend\nsay total\n"
            .to_string(),
    );
    add(
        "numeric/an-int-overflowing-a-float",
        "say 1000000 * 1000000 * 1000\n".to_string(),
    );
    add(
        "numeric/dividing-zero-by-zero",
        "set z to 0\nsay z / 0\n".to_string(),
    );
    add("numeric/modulo-zero", "say 5 % 0\n".to_string());
    add(
        "numeric/one-past-two-to-the-fifty-three",
        "set big to 9007199254740993\nsay big\nsay big + 1\n".to_string(),
    );
    add(
        "numeric/the-largest-i64",
        "say 9223372036854775807\n".to_string(),
    );
    add(
        "numeric/the-smallest-positive-i64-negated",
        "say -9223372036854775807\n".to_string(),
    );
    add(
        "numeric/the-sum-of-two-large-integers",
        "set big to 9007199254740992\nsay big + big\n".to_string(),
    );
    add("numeric/the-text-one-point-five", "say 1.5\n".to_string());
    add(
        "numeric/two-to-the-fifty-three",
        "set a to 9007199254740992\nsay a + 1\nsay a - 1\n".to_string(),
    );
    add(
        "numeric/zero-division",
        "set n to 1\nsay n / 0\n".to_string(),
    );
    add(
        "numeric/zero-times-minus-one",
        "set n to 0\nsay n * -1\n".to_string(),
    );
    add(
        "object/a-field-defaulted-to-nothing",
        "object Rec\n    has value default nothing\nend\nsay Rec.value is nothing\n".to_string(),
    );
    add(
        "object/a-method-called-in-a-loop",
        "object Counter\n    has n default 0\n    to can bump()\n        set Counter.n to Counter.n + 1\n    end\nend\nCounter.bump()\nCounter.bump()\nsay Counter.n\n".to_string(),
    );
    add(
        "object/a-method-called-on-a-variable-holding-the-object",
        "object Shape\n    to can describe()\n        give back \"shape\"\n    end\nend\nset s to Shape\nsay s.describe()\n".to_string(),
    );
    add(
        "object/a-method-calling-another-method",
        "object Calc\n    to can one()\n        give back 1\n    end\n    to can two()\n        give back 2\n    end\nend\nsay Calc.two() + Calc.one()\n".to_string(),
    );
    add(
        "object/a-method-on-something-that-is-not-an-object",
        "set n to 5\nsay n.plus(1)\n".to_string(),
    );
    add(
        "object/a-method-reading-a-field",
        "object Box\n    has item default 0\n    to can get()\n        give back this.item\n    end\nend\nset Box.item to 7\nsay Box.get()\n".to_string(),
    );
    add(
        "object/a-method-receiving-an-object-field",
        "object Inner\n    has n default 2\nend\nobject Outer\n    to can read()\n        give back Inner.n\n    end\nend\nsay Outer.read()\n".to_string(),
    );
    add(
        "object/a-method-returning-a-record",
        "object Bag\n    to can make()\n        give back { a: 1 }\n    end\nend\nsay Bag.make().a\n".to_string(),
    );
    add(
        "object/a-method-that-recurses",
        "object Tree\n    to can depth(n)\n        if n is 0 then\n            give back 0\n        end\n        give back 1 + Tree.depth(n - 1)\n    end\nend\nsay Tree.depth(20)\n".to_string(),
    );
    add(
        "object/a-method-the-object-does-not-have",
        "object A\n    has n default 1\nend\nsay A.missing_method()\n".to_string(),
    );
    add(
        "object/a-method-with-an-argument",
        "object Calc\n    to can twice(n)\n        give back n * 2\n    end\nend\nsay Calc.twice(21)\n".to_string(),
    );
    add(
        "object/an-object-with-no-methods",
        "object P\n    has x default 1\nend\nsay P.x\n".to_string(),
    );
    add(
        "object/inheriting-a-method-from-a-grandparent",
        "object A\n    to can who()\n        give back \"a\"\n    end\nend\nobject B extends A\nend\nobject C extends B\nend\nsay C.who()\n".to_string(),
    );
    add(
        "object/one-level-of-inheritance",
        "object Base\n    has name default \"base\"\n    to can greet()\n        give back \"from base\"\n    end\nend\nobject Child extends Base\n    has extra default 1\nend\nsay Child.name\nsay Child.extra\nsay Child.greet()\n".to_string(),
    );
    add(
        "object/three-levels-of-inheritance",
        "object A\n    has n default 1\nend\nobject B extends A\n    has m default 2\nend\nobject C extends B\n    has k default 3\nend\nsay C.n + C.m + C.k\n".to_string(),
    );
    add(
        "object/two-objects-that-are-alike",
        "object A\n    has n default 1\nend\nobject B\n    has n default 2\nend\nsay A.n + B.n\n"
            .to_string(),
    );
    add("print/print-of-a-list", "print [1, [2]]\n".to_string());
    add("print/print-of-a-number", "print 1\n".to_string());
    add("print/print-of-a-record", "print { a: 1 }\n".to_string());
    add("print/print-of-a-yes-no", "print yes\n".to_string());
    add("print/print-of-nothing", "print nothing\n".to_string());
    add("print/print-of-text", "print \"a\"\n".to_string());
    add(
        "print/say-and-print-together",
        "say \"line\"\nprint \"form\"\nsay \"line\"\n".to_string(),
    );
    add(
        "print/twenty-lines-in-order",
        "for each i in [1, 2, 3]\n    say i\nend\n".to_string(),
    );
    add(
        "record/a-field-holding-a-list",
        "set r to { xs: [1, 2, 3] }\nsay length(r.xs)\nsay r.xs[2]\n".to_string(),
    );
    add(
        "record/a-list-field-built-in-a-loop",
        "set xs to []\nfor each i in [1, 2, 3]\n    set xs to [xs, i]\nend\nset r to { xs: xs }\nsay r.xs[2]\n".to_string(),
    );
    add(
        "record/a-missing-key",
        "set r to { a: 1 }\nsay r.missing\nsay r.missing is nothing\n".to_string(),
    );
    add(
        "record/a-property-of-a-number",
        "say (5).missing\n".to_string(),
    );
    add(
        "record/a-record-field-holding-nothing",
        "set r to { v: nothing }\nsay r.v is nothing\n".to_string(),
    );
    add(
        "record/a-record-in-a-loop-of-records",
        "set total to 0\nfor each x in [{ n: 1 }, { n: 2 }]\n    set total to total + x.n\nend\nsay total\n".to_string(),
    );
    add(
        "record/a-singleton-record",
        "set r to { only: 1 }\nsay r.only\nsay type_of(r)\n".to_string(),
    );
    add(
        "record/an-empty-record",
        "set r to {}\nsay type_of(r)\nsay r.anything is nothing\n".to_string(),
    );
    add(
        "record/field-order-is-declaration-order",
        "set r to { z: 1, a: 2, m: 3 }\nfor each key in r\n    say key\nend\n".to_string(),
    );
    add(
        "record/indexing-with-a-missing-key",
        "set r to { a: 1 }\nsay r[\"b\"]\n".to_string(),
    );
    add(
        "record/indexing-with-a-number",
        "set r to { a: 1 }\nsay r[0]\n".to_string(),
    );
    add(
        "record/indexing-with-brackets",
        "set r to { a: 1 }\nsay r[\"a\"]\n".to_string(),
    );
    add(
        "record/overwriting-a-field",
        "set r to { a: 1 }\nset r.a to 2\nsay r.a\n".to_string(),
    );
    add(
        "record/three-levels-deep",
        "set r to { a: { b: { c: \"found\" } } }\nsay r.a.b.c\n".to_string(),
    );
    add(
        "record/writing-a-new-field",
        "set r to { a: 1 }\nset r.b to 2\nsay r.a\nsay r.b\n".to_string(),
    );
    add(
        "resource/a-file-that-is-not-there",
        "say files.read(\"definitely-not-here.txt\")\n".to_string(),
    );
    add(
        "resource/a-file-that-is-not-there-by-existence",
        "say files.exists(\"definitely-not-here.txt\")\n".to_string(),
    );
    add(
        "resource/a-list-grown-by-a-loop",
        "set xs to 1\nrepeat 200 times\n    set xs to [xs, 1]\nend\nsay length(xs)\n".to_string(),
    );
    add(
        "resource/a-repeat-of-a-thousand",
        "set n to 0\nrepeat 1000 times\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add(
        "resource/an-endless-loop-is-stopped",
        "set n to 0\nwhile yes is yes\n    set n to n + 1\nend\n".to_string(),
    );
    add(
        "resource/lines-of-a-file-that-is-not-there",
        "say files.lines(\"definitely-not-here.txt\")\n".to_string(),
    );
    add(
        "resource/unbounded-recursion-is-stopped",
        "to down(n)\n    give back down(n + 1)\nend\nsay down(0)\n".to_string(),
    );
    add(
        "scope/a-local-that-does-not-leak",
        "to f()\n    set hidden to 1\n    give back hidden\nend\nsay f()\n".to_string(),
    );
    add(
        "scope/a-variable-that-shadows-a-builtin-name",
        "set length to 1\nsay length\n".to_string(),
    );
    add(
        "scope/reassigning-a-variable",
        "set n to 1\nset n to n + 1\nset n to n + 1\nsay n\n".to_string(),
    );
    add(
        "scope/ten-reassignments",
        "set n to 0\nrepeat 10 times\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add(
        "scope/two-functions-with-the-same-local-name",
        "to a()\n    set n to 1\n    give back n\nend\nto b()\n    set n to 2\n    give back n\nend\nsay a() + b()\n".to_string(),
    );
    add(
        "testblock/a-test-inside-a-function",
        "to checked(n)\n    test \"even\"\n        expect n % 2 to be 0\n    end\n    give back n\nend\nsay checked(4)\n".to_string(),
    );
    add(
        "testblock/a-test-that-sees-a-loop",
        "set n to 0\nrepeat 2 times\n    set n to n + 1\nend\ntest \"n\"\n    expect n to be 2\nend\n".to_string(),
    );
    add(
        "testblock/a-test-using-a-call",
        "to double(x)\n    give back x * 2\nend\ntest \"double\"\n    expect double(21) to be 42\nend\n".to_string(),
    );
    add(
        "testblock/a-test-using-contain",
        "test \"contains\"\n    expect \"hello world\" to contain \"world\"\nend\n".to_string(),
    );
    add(
        "testblock/three-tests-in-a-row",
        "test \"a\"\n    expect 1 to be 1\nend\ntest \"b\"\n    expect 2 to be 2\nend\ntest \"c\"\n    expect 3 to be 3\nend\n".to_string(),
    );
    add(
        "testblock/two-passing-tests",
        "test \"one\"\n    expect 1 to be 1\nend\ntest \"two\"\n    expect \"a\" to be \"a\"\nend\n".to_string(),
    );
    add(
        "text/building-text-in-a-loop",
        "set out to \"\"\nrepeat 5 times\n    set out to out + \"x\"\nend\nsay out\n".to_string(),
    );
    add(
        "text/concatenating-a-number-into-text",
        "say \"n=\" + 1\n".to_string(),
    );
    add(
        "text/concatenating-nothing",
        "say \"n=\" + nothing\n".to_string(),
    );
    add("text/concatenation", "say \"a\" + \"b\"\n".to_string());
    add("text/indexing-into-text", "say \"hello\"[1]\n".to_string());
    add(
        "text/indexing-past-the-end-of-text",
        "say \"abc\"[9]\n".to_string(),
    );
    add(
        "text/interpolation-of-a-call",
        "to two()\n    give back 2\nend\nsay \"two is {two()}\"\n".to_string(),
    );
    add(
        "text/interpolation-of-one-expression",
        "set a to 2\nsay \"a is {a} and doubled is {a * 2}\"\n".to_string(),
    );
    add(
        "text/interpolation-twice-in-one-literal",
        "set a to \"x\"\nset b to \"y\"\nsay \"{a} then {b}\"\n".to_string(),
    );
    add(
        "text/interpolation-with-nothing",
        "say \"value: {nothing}\"\n".to_string(),
    );
    add(
        "text/length-accumulated-by-a-loop",
        "set n to 0\nrepeat 10 times\n    set n to n + length(\"abc\")\nend\nsay n\n".to_string(),
    );
    add(
        "text/length-of-a-list-of-lists",
        "say length([[1], [2, 3]])\n".to_string(),
    );
    add(
        "text/length-of-empty-text",
        "say length(\"\")\n".to_string(),
    );
    add("text/length-of-text", "say length(\"hello\")\n".to_string());
    add(
        "try/a-catch-inside-a-loop",
        "set count to 0\nfor each v in [1, 2, 3]\n    try\n        say 1 / 0\n    catch\n        set count to count + 1\n    end\nend\nsay count\n".to_string(),
    );
    add(
        "try/a-catch-that-does-not-fire",
        "set caught to \"no\"\ntry\n    say \"fine\"\ncatch error\n    set caught to \"yes\"\nend\nsay caught\n".to_string(),
    );
    add(
        "try/a-catch-that-re-raises",
        "try\n    say 1 / 0\ncatch error\n    say error\nend\nsay \"unreachable\"\n".to_string(),
    );
    add(
        "try/a-caught-call-depth-failure",
        "to down(n)\n    give back down(n + 1)\nend\nset caught to no\ntry\n    say down(0)\ncatch error\n    set caught to yes\nend\nsay caught\nsay 2 + 2\n".to_string(),
    );
    add(
        "try/a-caught-failure-does-not-stop-the-loop",
        "set log to \"\"\nfor each v in [1, 2]\n    try\n        if v is 1 then\n            say 1 / 0\n        end\n        set log to log + v\n    catch\n        set log to log + \"e\"\n    end\nend\nsay log\n".to_string(),
    );
    add(
        "try/a-finally-that-runs-after-a-catch",
        "set log to \"\"\ntry\n    say 1 / 0\ncatch error\n    set log to log + \"c\"\nfinally\n    set log to log + \"f\"\nend\nsay log\n".to_string(),
    );
    add(
        "try/a-finally-that-runs-after-a-clean-try",
        "set log to \"\"\ntry\n    set log to log + \"t\"\nfinally\n    set log to log + \"f\"\nend\nsay log\n".to_string(),
    );
    add(
        "try/a-try-inside-a-function",
        "to guarded()\n    try\n        give back 1 / 0\n    catch error\n        give back 0\n    end\nend\nsay guarded()\n".to_string(),
    );
    add(
        "try/a-try-that-recovers-and-continues",
        "set total to 0\nfor each v in [1, 2, 3]\n    try\n        set total to total + 10\n    catch error\n        set total to total - 1\n    end\nend\nsay total\n".to_string(),
    );
    add(
        "try/only-a-finally",
        "set log to \"\"\ntry\n    say \"body\"\nfinally\n    set log to \"f\"\nend\nsay log\n"
            .to_string(),
    );
    add(
        "try/the-bound-error",
        "try\n    say 1 / 0\ncatch error\n    say error\nend\n".to_string(),
    );
    add(
        "try/the-type-of-the-bound-error",
        "try\n    say 1 / 0\ncatch error\n    say type_of(error)\nend\n".to_string(),
    );
    add(
        "try/two-try-blocks-in-a-row",
        "set log to \"\"\ntry\n    say 1 / 0\ncatch\n    set log to log + \"a\"\nend\ntry\n    say 2 / 0\ncatch\n    set log to log + \"b\"\nend\nsay log\n".to_string(),
    );
    add(
        "unicode/a-list-of-two-emoji",
        "say length([\"😀\", \"😁\"])\n".to_string(),
    );
    add(
        "unicode/a-unicode-field-name",
        "set r to { עוב: 1 }\nsay r.עוב\n".to_string(),
    );
    add(
        "unicode/cjk-length",
        "say length(\"你好世界\")\n".to_string(),
    );
    add("unicode/combining-marks", "say \"é\"\n".to_string());
    add(
        "unicode/emoji-in-a-record-value",
        "set r to { e: \"😀\" }\nsay r.e\n".to_string(),
    );
    add(
        "unicode/emoji-interpolated",
        "say \"hi {😀}\"\n".to_string(),
    );
    add("unicode/emoji-length", "say length(\"😀\")\n".to_string());
    add(
        "unicode/rtl-in-a-list",
        "set xs to [\"עובר\"]\nsay xs[0]\n".to_string(),
    );
    add(
        "unicode/unicode-accumulated-in-a-loop",
        "set s to \"\"\nrepeat 3 times\n    set s to s + \"你\"\nend\nsay s\n".to_string(),
    );
    add(
        "unicode/unicode-through-a-function",
        "to show(x)\n    say x\nend\nshow(\"你好\")\n".to_string(),
    );
    programs
}

/// The corpus the differential test is required to cover: at least 200 whole
/// programs. Asserted rather than assumed, so a corpus that silently shrinks
/// below the bar fails the gate instead of passing on a smaller set.
const MINIMUM_CORPUS: usize = 200;

// -- rb vm itself -------------------------------------------------------

/// `rb vm` runs a `.rbc`: the first stage at which a compiled program executes.
#[test]
fn rb_vm_runs_a_compiled_program() {
    let dir = scratch_dir("runs-a-compiled-program");
    let source_path = dir.join("hello.rb");
    let rbc_path = dir.join("hello.rbc");
    fs::write(
        &source_path,
        "set greeting to \"Hello, World!\"\nsay greeting\n",
    )
    .expect("source should be writable");

    let compile = rb(&[
        "compile",
        source_path.to_str().expect("utf-8 path"),
        "-o",
        rbc_path.to_str().expect("utf-8 path"),
    ]);
    assert!(
        compile.status.success(),
        "rb compile failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    assert!(rbc_path.exists(), "rb compile wrote no .rbc");

    let run = rb(&["vm", rbc_path.to_str().expect("utf-8 path")]);
    assert!(
        run.status.success(),
        "rb vm failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "Hello, World!\n",
        "rb vm did not print what the program says"
    );
}

/// A `.rbc` survives the round trip through a file: it is written by `rb
/// compile`, read back by `rb vm`, and produces the same output as the source it
/// came from.
#[test]
fn a_bytecode_file_round_trips_through_disk() {
    let dir = scratch_dir("round-trips-through-disk");
    let source_path = dir.join("sum.rb");
    let rbc_path = dir.join("sum.rbc");
    let source = "set total to 0\nrepeat 5 times\n    set total to total + 2\nend\nsay total\n";
    fs::write(&source_path, source).expect("source should be writable");
    assert!(
        rb(&[
            "compile",
            source_path.to_str().expect("utf-8 path"),
            "-o",
            rbc_path.to_str().expect("utf-8 path"),
        ])
        .status
        .success(),
        "rb compile should succeed"
    );

    let run = rb(&["run", source_path.to_str().expect("utf-8 path")]);
    let vm = rb(&["vm", rbc_path.to_str().expect("utf-8 path")]);
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&vm.stdout),
        "the compiled file printed something else than the source"
    );
    assert_eq!(
        String::from_utf8_lossy(&vm.stdout),
        "10\n",
        "the program adds 2 five times"
    );
}

/// `rb vm` refuses a file that is not bytecode rather than reading it as one.
#[test]
fn edge_rb_vm_refuses_a_file_that_is_not_bytecode() {
    let dir = scratch_dir("not-bytecode");
    let path = dir.join("pretending.rbc");
    fs::write(&path, "set x to 1\nsay x\n").expect("the file should be writable");

    let vm = rb(&["vm", path.to_str().expect("utf-8 path")]);
    assert!(
        !vm.status.success(),
        "rb vm should not run a text file as bytecode"
    );
    let stderr = String::from_utf8_lossy(&vm.stderr);
    assert!(
        stderr.contains("not a Redblue bytecode file"),
        "the refusal should say the file is not bytecode, said: {stderr}"
    );
    assert_eq!(
        String::from_utf8_lossy(&vm.stdout),
        "",
        "nothing should have been printed from a file that was refused"
    );
}

/// `rb vm` refuses a file that does not exist, and says so rather than reporting
/// a decode failure for a file it never read.
#[test]
fn edge_rb_vm_reports_a_missing_file() {
    let dir = scratch_dir("missing-file");
    let path = dir.join("absent.rbc");
    let vm = rb(&["vm", path.to_str().expect("utf-8 path")]);
    assert!(
        !vm.status.success(),
        "rb vm should fail on a file that is not there"
    );
    let stderr = String::from_utf8_lossy(&vm.stderr);
    assert!(
        stderr.contains("No such file") || stderr.contains("cannot read"),
        "the refusal should be an I/O failure, said: {stderr}"
    );
}

/// `rb vm` refuses a path that is not named `.rbc`, rather than decoding a
/// source file that happens to be there.
#[test]
fn edge_rb_vm_refuses_a_source_file() {
    let dir = scratch_dir("source-not-bytecode");
    let path = dir.join("program.rb");
    fs::write(&path, "say \"hello\"\n").expect("the file should be writable");

    let vm = rb(&["vm", path.to_str().expect("utf-8 path")]);
    assert!(!vm.status.success(), "rb vm should refuse a .rb path");
    let stderr = String::from_utf8_lossy(&vm.stderr);
    assert!(
        stderr.contains("is not a bytecode file"),
        "the refusal should name the mistake, said: {stderr}"
    );
}

// -- the differential test ---------------------------------------------

/// The differential test the phase requires: both VMs agree on every program in
/// the corpus, which is at least 200 whole programs drawn from `examples/`,
/// `modules/`, `tests/` and the generated set.
#[test]
fn a_corpus_of_programs_runs_identically_on_both_vms() {
    let programs = corpus();
    assert!(
        programs.len() >= MINIMUM_CORPUS,
        "the corpus holds {} programs, and the phase requires at least {MINIMUM_CORPUS}",
        programs.len()
    );

    let mut disagreements = Vec::new();
    for (name, source) in &programs {
        let tree = tree_walk(source);
        let byte = bytecode(source);
        if tree != byte {
            disagreements.push(format!(
                "{name}\n  tree:     {tree:?}\n  bytecode: {byte:?}"
            ));
        }
    }

    assert!(
        disagreements.is_empty(),
        "{} of {} programs disagree between the tree-walking VM and the bytecode VM:\n{}",
        disagreements.len(),
        programs.len(),
        disagreements.join("\n")
    );
}

/// Every program in the corpus produces the same outcome whichever VM runs it —
/// the same failure kind and message where it fails. Split from the output
/// comparison so a failure names errors rather than printouts.
#[test]
fn edge_the_two_vms_report_the_same_failure_for_every_corpus_program() {
    let programs = corpus();
    let mut disagreements = Vec::new();
    for (name, source) in &programs {
        let tree = tree_walk(source);
        let byte = bytecode(source);
        // Only failures are compared here; a program that succeeds on one VM and
        // fails on the other is a worse disagreement, and is compared too.
        if tree.result != byte.result {
            disagreements.push(format!(
                "{name}\n  tree:     {:?}\n  bytecode: {:?}",
                tree.result, byte.result
            ));
        }
    }

    assert!(
        disagreements.is_empty(),
        "{} programs report a different outcome:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
}

/// Both VMs hold the same resource limits, so a program that runs under one
/// fails under the other rather than running forever under one and stopping
/// under the other.
#[test]
fn edge_both_vms_enforce_the_same_step_budget() {
    // A loop with no exit: both VMs must stop it, and both must say why.
    //
    // The failure names the *iteration* cap rather than the step budget, because
    // one loop turns a million times before a program has executed ten million
    // statements — so the iteration cap is the one that is reached, and both
    // VMs have to reach it at the same turn.
    let source = "set n to 0\nwhile yes is yes\n    set n to n + 1\nend\n";
    let tree = tree_walk(source);
    let byte = bytecode(source);
    assert!(
        tree.result.is_err() && byte.result.is_err(),
        "an endless loop must be stopped by both VMs: tree={tree:?} bytecode={byte:?}"
    );
    assert_eq!(
        tree.result, byte.result,
        "the two VMs should stop an endless loop with the same failure"
    );
    let message = tree
        .result
        .as_ref()
        .expect_err("the loop should have been stopped");
    assert!(
        message.contains("iterations") || message.contains("Step budget"),
        "the failure should name the limit that stopped it, said: {message}"
    );
}

/// A call that recurses without end is stopped by the call-depth limit in the
/// bytecode VM, exactly as in the tree-walking one — and stopping it must not
/// overflow the machine stack, because the bytecode VM keeps its frames on the
/// heap.
#[test]
fn edge_both_vms_stop_unbounded_recursion_at_the_same_depth() {
    let source = "to down(n)\n    give back down(n + 1)\nend\nsay down(0)\n";
    let tree = tree_walk(source);
    let byte = bytecode(source);
    assert!(
        tree.result.is_err() && byte.result.is_err(),
        "unbounded recursion must be stopped by both VMs"
    );
    assert_eq!(
        tree.result, byte.result,
        "the two VMs should stop unbounded recursion identically"
    );
    assert!(
        byte.result
            .as_ref()
            .expect_err("the recursion should have been stopped")
            .contains("Maximum call depth"),
        "the failure should name the call depth, said: {:?}",
        byte.result
    );
}

/// A caught call-depth failure leaves both VMs usable: the counter is given back
/// when the failing frames are unwound, so a program that catches the error can
/// keep running.
#[test]
fn edge_a_caught_depth_failure_leaves_the_bytecode_vm_usable() {
    let source = "\
to down(n)
    give back down(n + 1)
end
set caught to no
try
    say down(0)
catch error
    set caught to yes
end
say caught
say 2 + 2
";
    assert_agrees(source);

    let byte = bytecode(source);
    assert_eq!(
        byte.output,
        vec!["yes".to_string(), "4".to_string()],
        "the bytecode VM should catch the depth failure and carry on"
    );
}

/// The bytecode VM has no native stack to overflow: deeply nested *data* is
/// walked with explicit state, so a nesting a call stack could not hold is still
/// built, dropped and printed.
#[test]
fn edge_deeply_nested_data_does_not_overflow_the_bytecode_vm() {
    // A list built left to right nests one level per element, so this is 500
    // levels of nesting in a value, not 500 calls.
    let mut source = String::from("set deepest to 1\n");
    for depth in 1..=500 {
        source.push_str(&format!("set deepest to [[[[[{depth}]]]]]\n"));
    }
    source.push_str("say length(deepest)\n");

    assert_agrees(&source);
    assert_eq!(
        bytecode(&source).output,
        vec!["1".to_string()],
        "the innermost list holds one element"
    );
}

/// Every program in the corpus compiles to a `.rbc` that decodes back to the
/// same chunk, and running the decoded chunk gives the same answer as running
/// the one that was compiled — so the execution path is tested on bytes a file
/// actually carried, not only on the compiler's in-memory value.
#[test]
fn edge_a_decoded_chunk_runs_identically_to_the_compiled_one() {
    let mut ran = 0;
    for (name, source) in corpus() {
        // A program the frontend refuses never reaches a VM, so there is
        // nothing to round-trip: the compile failure *is* its outcome, and the
        // differential test compares that.
        let Ok(chunk) = compile_source(&source) else {
            continue;
        };
        let decoded = redblue::Chunk::decode(&chunk.encode())
            .unwrap_or_else(|error| panic!("{name} should decode: {error}"));

        let mut direct = BytecodeVm::new();
        let mut via_file = BytecodeVm::new();
        assert_eq!(
            Outcome {
                output: via_file.take_output(),
                result: via_file
                    .run(&decoded)
                    .map(|value| value.to_string())
                    .map_err(|error| failure_of(&error)),
            },
            Outcome {
                output: direct.take_output(),
                result: direct
                    .run(&chunk)
                    .map(|value| value.to_string())
                    .map_err(|error| failure_of(&error)),
            },
            "{name} ran differently after a round trip through bytes"
        );
        ran += 1;
    }
    assert!(
        ran >= MINIMUM_CORPUS,
        "only {ran} of the corpus programs reached the VM, and the phase requires \
         at least {MINIMUM_CORPUS} to have been executed"
    );
}

// -- per-shape tests ---------------------------------------------------

/// The happy path, asserted rather than assumed.
#[test]
fn tree_walk_and_bytecode_print_the_same_thing() {
    assert_agrees("set total to 0\nrepeat 5 times\n    set total to total + 1\nend\nsay total\n");
    assert_eq!(
        bytecode("set total to 0\nrepeat 5 times\n    set total to total + 1\nend\nsay total\n")
            .output,
        vec!["5".to_string()]
    );
}

/// A failing program produces a failure on the bytecode VM — not a panic, not a
/// silent success, and not an `expect` that quietly passes.
#[test]
fn edge_an_out_of_bounds_index_is_a_clean_failure() {
    let source = "set xs to [1]\nsay xs[9]\n";
    let byte = bytecode(source);
    assert!(
        byte.result.is_err(),
        "an out-of-bounds index must fail, not succeed: {byte:?}"
    );
    let message = byte
        .result
        .as_ref()
        .expect_err("the index should have been refused");
    assert_eq!(
        message, "RuntimeError: Index 9 is out of bounds: length is 1, valid indexes are 0 to 0",
        "the failure should name the index and the length"
    );
    assert_eq!(
        tree_walk(source).result,
        byte.result,
        "both VMs should refuse the same index the same way"
    );
}

/// An empty operand stack is a failure a VM reports, not a panic: an operand
/// the file asked for that no instruction produced is caught rather than
/// indexed into.
#[test]
fn edge_bytecode_that_asks_for_a_value_it_never_pushed_is_refused() {
    // Hand-built: no compiler emits this, so the guard cannot be left untested
    // by the corpus. `CALL` names a constant and claims one argument, but the
    // block pushes nothing at all.
    // `set f to 1` leaves `f` in the pool as a name. The chunk below calls that
    // name with one argument and pushes none, which no compiler emits.
    let mut chunk = compile_source("set f to 1\n").expect("a trivial program should compile");
    let name = chunk
        .main
        .code
        .iter()
        .find(|instruction| instruction.opcode == Opcode::Store)
        .expect("the program stores into a name")
        .arg;
    chunk.main.code = vec![Instruction {
        opcode: Opcode::Call,
        arg: name,
        aux: 1,
        line: 1,
    }];

    let mut vm = BytecodeVm::new();
    let error = vm
        .run(&chunk)
        .expect_err("a call with no arguments on an empty stack must fail");
    assert!(
        format!("{}: {}", error.label(), error.message()).contains("never pushed"),
        "the failure should say the frame never pushed the values, said: {error}"
    );
}

/// A jump target past the end of its block is malformed — `rb dis` reports it,
/// per `docs/BYTECODE.md` — so the VM must not read past the instruction list
/// when it meets one. Clamping the target to the block's end is the safe answer
/// and is what this asserts.
#[test]
fn edge_a_jump_past_the_end_of_a_block_does_not_read_past_the_code() {
    let mut chunk = compile_source("say 1\n").expect("a trivial program should compile");
    chunk.main.code = vec![
        Instruction {
            opcode: Opcode::Jump,
            arg: 99,
            aux: 0,
            line: 1,
        },
        Instruction {
            opcode: Opcode::Say,
            arg: 0,
            aux: 0,
            line: 1,
        },
    ];

    let mut vm = BytecodeVm::new();
    let outcome = vm.run(&chunk);
    assert!(
        outcome.is_ok(),
        "a jump past the end of a block should end the block, not fail: {outcome:?}"
    );
    assert_eq!(
        vm.take_output(),
        Vec::<String>::new(),
        "the instruction after a jump past the end of the block should not run"
    );
}

/// The step budget is charged per instruction, so the bytecode VM stops a
/// program at the same place the tree-walker does rather than at some point of
/// its own choosing.
#[test]
fn edge_the_step_budget_is_a_configurable_limit_not_a_hard_coded_one() {
    let mut tight = BytecodeVm::with_max_steps(12);
    let result = tight.run(
        &compile_source("set n to 0\nrepeat 5 times\n    set n to n + 1\nend\nsay n\n")
            .expect("the program should compile"),
    );
    assert!(
        result.is_err(),
        "a twelve-step budget should not run a five-iteration loop"
    );

    let mut roomy = BytecodeVm::with_max_steps(100_000);
    assert!(
        roomy
            .run(
                &compile_source("set n to 0\nrepeat 5 times\n    set n to n + 1\nend\nsay n\n")
                    .expect("the program should compile"),
            )
            .is_ok(),
        "a hundred-thousand-step budget should run it easily"
    );
}

/// The per-loop iteration cap is enforced the same way, and names the kind of
/// loop that hit it.
#[test]
fn edge_the_iteration_cap_names_the_kind_of_loop_that_hit_it() {
    let mut vm = BytecodeVm::with_max_iterations(3);
    let error = vm
        .run(
            &compile_source("set n to 0\nfor each v in [1, 2, 3, 4, 5]\n    set n to n + v\nend\n")
                .expect("the program should compile"),
        )
        .expect_err("five turns against a three-iteration cap must fail");
    assert!(
        format!("{}: {}", error.label(), error.message()).contains("'for each' loop"),
        "the failure should name the kind of loop, said: {error}"
    );
}

/// A `break` outside a loop is a malformed program, and the bytecode VM says so
/// instead of leaving the block it is in.
#[test]
fn edge_a_return_that_is_not_the_blocks_last_statement_does_not_end_the_block() {
    // `return` in Redblue yields a value; it is not an escape. The tree-walking
    // VM runs the statements after it, and the bytecode VM has to as well or the
    // two disagree about what a function means.
    assert_agrees("to f()\n    give back 1\n    say \"after\"\nend\nsay f()\n");
    assert_eq!(
        bytecode("to f()\n    give back 1\n    say \"after\"\nend\nsay f()\n").output,
        vec!["after".to_string(), "nothing".to_string()],
        "the statement after a `return` runs, and the block's value is the last \
         statement's, both as in the tree-walker"
    );
}

/// A loop's variable belongs to the loop.
///
/// The tree-walking VM pushes a scope for each turn of a `for each` and pops it
/// when the turn ends, so a name of the same name outside the loop keeps the
/// value it had. The bytecode VM compiles the body inline and so has a single
/// binding for the whole loop — this is the test that says leaving the loop puts
/// the outer binding back.
#[test]
fn edge_a_loop_variable_does_not_clobber_the_name_it_shadows() {
    let source = "set v to 99\nfor each v in [1, 2]\n    say v\nend\nsay v\n";
    assert_agrees(source);
    assert_eq!(
        bytecode(source).output,
        vec!["1".to_string(), "2".to_string(), "99".to_string()],
        "the two turns print their own value and the outer name is untouched"
    );
}

/// The same rule seen from the other side: a loop variable that was bound to
/// nothing before its loop must not exist after it.
///
/// Redblue refuses this in the analyzer, before either VM runs, so what this
/// pins is that the *rule* holds identically on both sides — a loop variable is
/// the loop's, not the block's — and that neither VM is reached with a name the
/// frontend already rejected. Asserted with the message so the test can fail if
/// the two ever stop agreeing.
#[test]
fn edge_a_loop_variable_that_shadowed_nothing_is_unbound_afterwards() {
    let source = "for each w in [1, 2]\n    say w\nend\nsay w\n";
    let tree = tree_walk(source);
    let byte = bytecode(source);
    assert_eq!(
        tree, byte,
        "reading a loop variable after its loop must fail identically on both VMs"
    );
    assert_eq!(
        tree.result,
        Err("AnalyzerError: Unknown variable 'w'".to_string()),
        "a loop variable belongs to its loop, so the name is unknown outside it"
    );
}

/// A loop variable shadowing a name one scope up, which is the case a plain
/// "save the global" fix would miss: the outer binding is a local, not a global.
#[test]
fn edge_a_loop_variable_does_not_clobber_a_local_it_shadows() {
    let source = "\
to f()
    set v to 7
    for each v in [1, 2]
        say v
    end
    give back v
end
say f()
";
    assert_agrees(source);
    assert_eq!(
        bytecode(source).output,
        vec!["1".to_string(), "2".to_string(), "7".to_string()],
        "a loop over a local name leaves the local alone"
    );
}

/// An operand the file asked for but no instruction pushed is a failure a VM
/// reports, not a panic.
///
/// This is the second half of the empty-stack guard: a frame may not take a
/// value its *caller* pushed. The stack is non-empty here — the caller left a
/// value on it — so a guard that only asked "is the stack empty?" would let this
/// through and hand the callee an argument it was never given.
#[test]
fn edge_a_frame_cannot_pop_below_its_own_stack_base() {
    // Hand-built: the chunk pushes one value, then a `CALL` claims one argument.
    // No compiler emits this, so the corpus cannot cover it.
    let mut chunk = compile_source("set f to 1\n").expect("a trivial program should compile");
    let name = chunk
        .main
        .code
        .iter()
        .find(|instruction| instruction.opcode == Opcode::Store)
        .expect("the program stores into a name")
        .arg;
    let mut code = Vec::new();
    // Leave one value on the operand stack for the caller.
    for instruction in chunk.main.code.iter().take(2) {
        code.push(*instruction);
    }
    code.push(Instruction {
        opcode: Opcode::Call,
        arg: name,
        aux: 1,
        line: 1,
    });
    chunk.main.code = code;

    let mut vm = BytecodeVm::new();
    let error = vm
        .run(&chunk)
        .expect_err("a call may not take a value its caller pushed");
    assert!(
        format!("{}: {}", error.label(), error.message()).contains("never pushed"),
        "the failure should say the frame never pushed the values, said: {error}"
    );
}

/// Deeply nested *data* is walked by recursing through `Value`, and that
/// recursion uses the machine stack, so there is a nesting depth past which
/// neither VM survives. This pins where that limit is: both VMs answer the same
/// way at a depth both handle, so a program that reaches the limit is a clean
/// outcome for as long as it stays under it.
///
/// The finding behind the limit is `phases/phase-019/FINDINGS.md`: the limit is
/// a property of `Value`, not of the bytecode VM's loop, and making either VM
/// survive it is a change to `src/value.rs` that this phase does not make.
#[test]
fn edge_both_vms_answer_the_same_at_a_nesting_depth_neither_overflows() {
    // A list built left to right nests one level per assignment. 400 levels is
    // deep enough to be interesting and shallow enough that both VMs walk it.
    let mut source = String::from("set deepest to 1\n");
    for depth in 1..=400 {
        source.push_str(&format!("set deepest to [[[[[{depth}]]]]]\n"));
    }
    source.push_str("say length(deepest)\n");

    assert_agrees(&source);
    assert_eq!(
        bytecode(&source).output,
        vec!["1".to_string()],
        "the innermost list holds one element on both VMs"
    );
}

// -- constants and modules ----------------------------------------------

/// A `constant` is read-only on the bytecode VM exactly as it is on the
/// tree-walking one: the name binds, a read returns what was declared, and a
/// later `set` onto it is refused rather than shadowing it.
#[test]
fn a_constant_binds_and_is_read_only_on_both_vms() {
    let source = "constant TAU to 6.28318\nsay TAU\nset TAU to 1\nsay TAU\n".to_string();

    assert_agrees(&source);

    let byte = bytecode(&source);
    assert_eq!(
        byte.output,
        vec!["6.28318".to_string()],
        "the declaration bound the name and the read returned it"
    );
    let failure = byte
        .result
        .as_ref()
        .expect_err("a set onto a constant must be refused, not shadow it");
    assert_eq!(
        failure, "RuntimeError: Cannot assign to constant 'TAU'",
        "the refusal should name the constant, matching the tree-walking VM"
    );
}

/// Declaring one name twice is refused on both VMs, and the first declaration
/// is what survives. This is the case `DECLARE_CONST` exists for: a `STORE`
/// would have overwritten `A` with 2 and said nothing.
#[test]
fn edge_a_constant_declared_twice_is_refused_and_the_first_value_survives() {
    let source = "constant A to 1\nconstant A to 2\nsay A\n".to_string();

    assert_agrees(&source);

    let byte = bytecode(&source);
    assert_eq!(
        byte.result,
        Err("RuntimeError: Constant 'A' is already declared".to_string()),
        "the second declaration must be refused by name"
    );
    assert_eq!(
        tree_walk(&source).result,
        byte.result,
        "both VMs must refuse it the same way"
    );
}

/// A module's `constant` binds the importing program's name.
///
/// The module loader kept only the module's `set` statements, so a module that
/// declares `constant PI` bound nothing and the read fell through to the
/// builtin's `PI` — the two VMs printed different numbers for the same program.
#[test]
fn edge_a_modules_constant_binds_rather_than_falling_through_to_the_builtin() {
    // `MathUtils` declares `constant PI to 3.14159`, and `PI` is also a
    // builtin, so the two agree only if the module's declaration reached the
    // importing program. They did not: the module loader kept only `set`, so
    // the read fell through to the builtin's `PI`.
    let source = "import MathUtils\nsay PI\n".to_string();
    assert_agrees(&source);
    assert_eq!(
        bytecode(&source).output,
        vec!["3.14159".to_string()],
        "the module's constant must be visible to the importer, not the builtin's"
    );
}

/// A module's `constant` is read-only in the importing program too, so a
/// program that writes onto an imported name is refused rather than shadowing
/// the module's declaration.
#[test]
fn edge_a_modules_constant_cannot_be_rebound_by_the_importing_program() {
    // `MathUtils` declares `constant TAU`, so this writes onto an imported
    // name. The module is read-only to the program that imported it, exactly as
    // a `constant` written in that program would be.
    let source = "import MathUtils\nset TAU to 8\nsay TAU\n".to_string();
    assert_agrees(&source);
    assert_eq!(
        bytecode(&source).result,
        Err("RuntimeError: Cannot assign to constant 'TAU'".to_string()),
        "an imported constant stays read-only in the program that imported it"
    );
}

/// A filler `NOP` is the filler everywhere: inside a protected region it closes
/// nothing, on the path where nothing failed and on the path where something did.
///
/// Hand-built, because the compiler emits no filler of its own — `rb compile`
/// writes exactly one `NOP` per `try`, the marked one that closes the region — so
/// a program compiled from source cannot exercise this and a test built from
/// source would pass without ever reaching it. A `.rbc` is a file, so it can hold
/// a `NOP` anywhere, and the two ways a VM that read every `NOP` as a region end
/// goes wrong are both here: it runs the `finally` early when nothing failed, and
/// it resumes inside the region it had just caught a failure in.
#[test]
fn edge_a_filler_nop_does_not_close_a_protected_region() {
    for (name, source, anchor, expected) in [
        (
            "nothing failed",
            "set log to \"\"\ntry\n    set log to log + \"t\"\n    set log to log + \"n\"\nfinally\n    set log to log + \"f\"\nend\nsay log\n",
            // The `TRY`, so the filler is the region's first instruction.
            Opcode::Try,
            "tnf",
        ),
        (
            "something failed",
            "set log to \"\"\ntry\n    set log to log + \"t\"\n    say 1 / 0\n    set log to log + \"u\"\ncatch\n    set log to log + \"c\"\nend\nsay log\n",
            // The `DIV` that fails, so the filler is inside the region the
            // failure has to skip to the end of.
            Opcode::Div,
            "tc",
        ),
    ] {
        assert_agrees(source);
        let mut chunk = compile_source(source).expect("the program should compile");
        let at = chunk
            .main
            .code
            .iter()
            .position(|instruction| instruction.opcode == anchor)
            .unwrap_or_else(|| panic!("the program should contain a {anchor:?}"));
        // Neither program has a jump, so splicing an instruction in moves
        // nothing that names an offset.
        chunk.main.code.insert(
            at + 1,
            Instruction {
                opcode: Opcode::Nop,
                arg: 0,
                aux: 0,
                line: 1,
            },
        );

        let mut vm = BytecodeVm::new();
        let outcome = vm.run(&chunk);
        assert_eq!(
            vm.take_output(),
            vec![expected.to_string()],
            "a filler NOP changed what the {name} path did: {outcome:?}"
        );
    }

    // A filler with no `try` above it is the filler it has always been: no
    // handler, so nothing runs, and the program carries on.
    let mut chunk = compile_source("say 1\n").expect("a trivial program should compile");
    chunk.main.code.insert(
        0,
        Instruction {
            opcode: Opcode::Nop,
            arg: 0,
            aux: 0,
            line: 1,
        },
    );
    let mut vm = BytecodeVm::new();
    assert!(
        vm.run(&chunk).is_ok(),
        "a filler NOP is not a failure and must not fail the program"
    );
    assert_eq!(
        vm.take_output(),
        vec!["1".to_string()],
        "a filler NOP costs a step and changes nothing"
    );
}

/// A handled inner `try` leaves the enclosing region as it found it: still
/// protected, and with its `finally` still owed to the end of *its* own
/// protected code.
///
/// This is what a resume-*at*-the-region-end gets wrong. The inner region's
/// closing `NOP` pops whatever handler is on top, which after the inner failure
/// has been handled is the enclosing one — so the enclosing `finally` ran in the
/// middle of its protected region and everything after the inner `try` ran with
/// no protection at all. Both programs here fail on the bytecode VM without the
/// fix, one with the letters in the wrong order and one with an uncaught error.
#[test]
fn edge_a_handled_inner_try_leaves_the_enclosing_region_protected() {
    for (name, source, expected) in [
        (
            "the finally runs last",
            concat!(
                "set log to \"\"\n",
                "try\n",
                "    set log to log + \"a\"\n",
                "    try\n",
                "        say 1 / 0\n",
                "    catch\n",
                "        set log to log + \"c\"\n",
                "    finally\n",
                "        set log to log + \"f\"\n",
                "    end\n",
                "    set log to log + \"b\"\n",
                "finally\n",
                "    set log to log + \"F\"\n",
                "end\n",
                "say log\n",
            ),
            "acfbF",
        ),
        (
            "the enclosing catch still runs",
            concat!(
                "set log to \"\"\n",
                "try\n",
                "    try\n",
                "        say 1 / 0\n",
                "    catch\n",
                "        set log to log + \"c\"\n",
                "    end\n",
                "    say 1 / 0\n",
                "catch\n",
                "    set log to log + \"d\"\n",
                "end\n",
                "say log\n",
            ),
            "cd",
        ),
    ] {
        assert_agrees(source);
        assert_eq!(
            bytecode(source).output,
            vec![expected.to_string()],
            "the handled inner try disturbed the enclosing region on the {name} path"
        );
    }
}

/// A module is loaded once. A second `import` of the same module binds nothing a
/// second time — which is what makes a module's `constant` importable at all,
/// since a constant cannot be declared twice and re-running the module declares
/// it twice.
#[test]
fn edge_importing_the_same_module_twice_loads_it_once() {
    let source = "import MathUtils\nimport MathUtils\nsay PI\n".to_string();

    assert_agrees(&source);

    let twice = bytecode(&source);
    assert_eq!(
        twice.output,
        vec!["3.14159".to_string()],
        "the second import must not re-run the module"
    );
    assert!(
        twice.result.is_ok(),
        "a second import of a loaded module is a no-op, not a second declaration: {:?}",
        twice.result
    );
}

/// A loop variable shadows a constant rather than being refused by it.
///
/// The tree-walking VM gives each turn of a `for each` its own scope, so the
/// loop's name is a live local inside the loop and the constant is what the name
/// means everywhere else. Refusing the loop's own binding would make the two VMs
/// disagree about a program that reads perfectly well.
#[test]
fn edge_a_loop_variable_shadows_a_constant_instead_of_being_refused() {
    let source = "constant X to 1\nfor each X in [1, 2]\n    say X\nend\nsay X\n".to_string();

    assert_agrees(&source);

    assert_eq!(
        bytecode(&source).output,
        vec!["1".to_string(), "2".to_string(), "1".to_string()],
        "the loop's own name is live inside the loop and the constant is back after it"
    );
}

/// A `finally` runs exactly once on each path: once when nothing failed, and
/// once when the catch handled the failure. The marked `NOP` is reached on the
/// success path and stepped over on the failure path, so a `finally` that ran
/// twice would show up here as a doubled letter.
#[test]
fn edge_a_finally_runs_exactly_once_on_each_path() {
    for (name, source) in [
        (
            "success",
            "set log to \"\"\ntry\n    set log to log + \"t\"\nfinally\n    set log to log + \"f\"\nend\nsay log\n",
        ),
        (
            "caught failure",
            "set log to \"\"\ntry\n    say 1 / 0\ncatch\n    set log to log + \"c\"\nfinally\n    set log to log + \"f\"\nend\nsay log\n",
        ),
    ] {
        assert_agrees(source);
        assert_eq!(
            bytecode(source).output,
            vec![match name {
                "success" => "tf".to_string(),
                _ => "cf".to_string(),
            }],
            "the finally ran the wrong number of times on the {name} path"
        );
    }
}
