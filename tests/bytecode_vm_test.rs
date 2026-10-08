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
use redblue::{expect_repeat_count, run_isolated, Error, Value};

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
///
/// Both outcomes come back, so a test can say more about what they agreed on —
/// what a program printed, or which of two failures it reported.
#[track_caller]
fn assert_agrees(source: &str) -> (Outcome, Outcome) {
    let tree = tree_walk(source);
    let byte = bytecode(source);
    assert_eq!(
        tree, byte,
        "the two VMs disagree\n--- source ---\n{source}--- tree ---\n{tree:?}\n--- bytecode ---\n{byte:?}\n"
    );
    (tree, byte)
}

/// [`tree_walk`] with the per-loop iteration cap lowered to `max_iterations`.
///
/// The corpus runs at the published cap, which nothing in a corpus program can
/// reach — a program that needs one million turns takes a million turns to run —
/// so a disagreement about *how many turns a cap allows* is invisible to it. This
/// is how such a program is asked about.
fn tree_walk_capped(source: &str, max_iterations: usize) -> Outcome {
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
    let mut vm = redblue::Vm::with_max_iterations(max_iterations);
    let result = vm.run(&ast);
    Outcome {
        output: vm.take_output(),
        result: result
            .map(|value| value.to_string())
            .map_err(|error| failure_of(&error)),
    }
}

/// [`bytecode`] with the per-loop iteration cap lowered to `max_iterations`.
fn bytecode_capped(source: &str, max_iterations: usize) -> Outcome {
    let chunk = match compile_source(source) {
        Ok(chunk) => chunk,
        Err(error) => return failed(failure_of(&error)),
    };
    let mut vm = BytecodeVm::with_max_iterations(max_iterations);
    let result = vm.run(&chunk);
    Outcome {
        output: vm.take_output(),
        result: result
            .map(|value| value.to_string())
            .map_err(|error| failure_of(&error)),
    }
}

/// [`assert_agrees`] with the per-loop iteration cap lowered to `max_iterations`,
/// and the outcome of both VMs returned for a test to say more about.
#[track_caller]
fn assert_agrees_capped(source: &str, max_iterations: usize) -> (Outcome, Outcome) {
    let tree = tree_walk_capped(source, max_iterations);
    let byte = bytecode_capped(source, max_iterations);
    assert_eq!(
        tree, byte,
        "the two VMs disagree under a cap of {max_iterations}\n--- source ---\n{source}\
         --- tree ---\n{tree:?}\n--- bytecode ---\n{byte:?}\n"
    );
    (tree, byte)
}

/// [`tree_walk_capped`] with the step budget lowered as well.
///
/// A corpus program cannot reach either published limit — a program that needs a
/// million turns takes a million turns to run — so a disagreement about which of
/// the two stops a program first is invisible to it. This is how such a program
/// is asked about, with both limits brought down together so that their order is
/// a question about the accounting and not about how long a loop takes.
fn tree_walk_bounded(source: &str, max_iterations: usize, max_steps: usize) -> Outcome {
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
    let mut vm = redblue::Vm::with_limits(max_iterations, max_steps);
    let result = vm.run(&ast);
    Outcome {
        output: vm.take_output(),
        result: result
            .map(|value| value.to_string())
            .map_err(|error| failure_of(&error)),
    }
}

/// [`bytecode_bounded`] through the bytecode compiler.
fn bytecode_bounded(source: &str, max_iterations: usize, max_steps: usize) -> Outcome {
    let chunk = match compile_source(source) {
        Ok(chunk) => chunk,
        Err(error) => return failed(failure_of(&error)),
    };
    let mut vm = BytecodeVm::with_limits(max_iterations, max_steps);
    let result = vm.run(&chunk);
    Outcome {
        output: vm.take_output(),
        result: result
            .map(|value| value.to_string())
            .map_err(|error| failure_of(&error)),
    }
}

/// [`assert_agrees`] with both limits lowered, and both outcomes returned.
#[track_caller]
fn assert_agrees_bounded(
    source: &str,
    max_iterations: usize,
    max_steps: usize,
) -> (Outcome, Outcome) {
    let tree = tree_walk_bounded(source, max_iterations, max_steps);
    let byte = bytecode_bounded(source, max_iterations, max_steps);
    assert_eq!(
        tree, byte,
        "the two VMs disagree under a cap of {max_iterations} iterations and a budget \
         of {max_steps} steps\n--- source ---\n{source}\
         --- tree ---\n{tree:?}\n--- bytecode ---\n{byte:?}\n"
    );
    (tree, byte)
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

    // -- module declarations ------------------------------------------------
    //
    // A `module ... end` declaration was compiled inline on the bytecode VM,
    // which has no record of one, so every program below is a case the two VMs
    // used to answer differently: the tree-walker gave the module its own scope
    // and published what it exported, and this one ran the body as ordinary
    // top-level statements.
    add(
        "module/declares-and-publishes-all",
        "module Geometry\n    to area\n        return 3.14159 * 2 * 2\n    end\n    export all\nend\nsay Geometry.area()\n".to_string(),
    );
    add(
        "module/publishes-one-named-member",
        "module Math\n    to square\n        return n * n\n    end\n    to cube\n        return n * n * n\n    end\n    export square\nend\nsay Math.square(3)\n".to_string(),
    );
    add(
        "module/publishes-nothing-when-nothing-is-exported",
        "module Quiet\n    to f\n        return 1\n    end\nend\nsay \"ok\"\n".to_string(),
    );
    add(
        "module/the-modules-own-scope-does-not-leak",
        "module Inner\n    set hidden to 42\n    to f\n        return hidden\n    end\n    export all\nend\nsay Inner.f()\ntry\n    say hidden\ncatch error\n    say \"caught\"\nend\n".to_string(),
    );
    add(
        "module/two-modules-and-a-call-across-them",
        "module Outer\n    to twice\n        return n * 2\n    end\n    export twice\nend\nmodule Inner\n    to quad\n        return Outer.twice(Outer.twice(n))\n    end\n    export quad\nend\nsay Inner.quad(3)\n".to_string(),
    );
    add(
        "module/an-import-of-a-declared-module-reaches-its-members",
        "module Counter\n    to bump\n        return n + 1\n    end\n    export bump\nend\nimport Counter as C\nsay C.bump(41)\n".to_string(),
    );
    add(
        "module/an-empty-module-is-accepted",
        "module Empty\nend\nsay \"ok\"\n".to_string(),
    );
    add(
        "module/an-export-of-a-name-it-does-not-define-is-a-caught-error",
        "module Bad\n    to f\n        return 1\n    end\n    export nope\nend\ntry\n    say \"unreached\"\ncatch error\n    say \"caught\"\nend\n".to_string(),
    );
    add(
        "module/a-module-declared-twice-is-a-caught-error",
        "module Twice\n    to f\n        return 1\n    end\n    export f\nend\ntry\n    module Twice\n        to f\n            return 2\n        end\n        export f\n    end\ncatch error\n    say \"caught\"\nend\n".to_string(),
    );
    add(
        "module/a-constant-is-not-refused-by-a-set-inside-the-module",
        "constant OUTER to 1\nmodule Shadow\n    set OUTER to 2\n    to f\n        return OUTER\n    end\n    export f\nend\nsay Shadow.f()\nsay OUTER\n".to_string(),
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
        "flow/a-finally-runs-every-statement-on-the-way-out-of-a-break",
        "set cleaned to 0\nrepeat 3 times\n    try\n        break\n    finally\n        set cleaned to cleaned + 1\n        set cleaned to cleaned + 10\n    end\nend\nsay cleaned\n".to_string(),
    );
    add(
        "flow/a-break-in-a-loop-inside-a-try-keeps-the-try-installed",
        "try\n    for each i in [1, 2, 3]\n        if i is 2 then\n            break\n        end\n        say i\n    end\n    set bad to 1 + \"one\"\ncatch error\n    say \"caught\"\nfinally\n    say \"cleaned\"\nend\nsay \"done\"\n".to_string(),
    );
    add(
        "flow/a-break-in-a-nested-loop-inside-a-try-leaves-the-outer-try-installed",
        "try\n    for each a in [1, 2]\n        for each b in [1, 2]\n            try\n                if b is 2 then\n                    break\n                end\n            finally\n                say \"inner cleaned\"\n            end\n        end\n        say \"outer turn\"\n    end\n    set bad to 1 + \"one\"\ncatch error\n    say \"caught\"\nfinally\n    say \"outer cleaned\"\nend\nsay \"done\"\n".to_string(),
    );
    add(
        "flow/a-break-in-a-while-inside-a-try-keeps-the-try-installed",
        "try\n    set n to 0\n    while n is not 4\n        set n to n + 1\n        if n is 2 then\n            break\n        end\n        say n\n    end\n    set bad to 1 + \"one\"\ncatch error\n    say \"caught\"\nfinally\n    say \"cleaned\"\nend\nsay \"done\"\n".to_string(),
    );
    add(
        "flow/a-failing-finally-on-the-way-out-of-a-break-does-not-end-the-next-loop",
        "set caught to no\ntry\n    repeat 3 times\n        try\n            break\n        finally\n            set bad to 1 + \"one\"\n        end\n    end\ncatch error\n    set caught to yes\nend\nset n to 0\nrepeat 3 times\n    set n to n + 1\nend\nsay caught\nsay n\n".to_string(),
    );
    add(
        "flow/a-skip-in-a-loop-inside-a-try-keeps-the-try-installed",
        "try\n    for each i in [1, 2, 3]\n        if i is 2 then\n            skip\n        end\n        say i\n    end\n    set bad to 1 + \"one\"\ncatch error\n    say \"caught\"\nfinally\n    say \"cleaned\"\nend\nsay \"done\"\n".to_string(),
    );
    add(
        "flow/a-break-in-a-bounded-while-leaves-the-loop",
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
        "flow/a-skip-inside-a-bounded-loop-leaves-its-own-iteration-out",
        "set total to 0\nfor each v in [1, 2, 3]\n    skip\n    set total to total + v\nend\nsay total\n".to_string(),
    );
    // -- a break or a skip in an `unless` body ------------------------------
    //
    // An `unless` body compiles into the *enclosing* block, so the bytecode VM's
    // `BREAK`/`SKIP` jumps over the rest of the body — while the tree-walking VM
    // used to run the body as a plain statement list and so kept going past a
    // signal that was already raised. These are the shapes the two used to
    // answer differently.
    add(
        "flow/a-break-in-an-unless-body-ends-the-block-it-is-written-in",
        "for each i in [1, 2, 3]\n    unless i is 2 then\n        say i\n        break\n        say \"unreachable\"\n    end\n    say \"after\"\nend\nsay \"done\"\n".to_string(),
    );
    add(
        "flow/a-skip-in-an-unless-body-leaves-the-rest-of-it-unreached",
        "set log to \"\"\nfor each i in [1, 2, 3]\n    unless i is 99 then\n        say i\n        skip\n        say \"unreachable\"\n    end\n    set log to log + i\nend\nsay log\nsay \"done\"\n".to_string(),
    );
    // -- a break or a skip in a module body --------------------------------
    //
    // A module body runs where the declaration is written, so it is inside the
    // loop around that declaration exactly as an `object` body is. The bytecode
    // VM gave the module's frame no loop of its own and no owner, so it refused
    // these where the tree-walking VM honoured them.
    add(
        "module/a-break-in-a-module-body-inside-a-loop-leaves-that-loop",
        "set count to 0\nfor each i in [1, 2, 3]\n    set count to count + 1\n    module Inner\n        set held to i\n        break\n        say \"unreachable\"\n    end\n    say \"unreachable\"\nend\nsay count\n".to_string(),
    );
    add(
        "module/a-skip-in-a-module-body-inside-a-loop-advances-that-loop",
        "set log to \"\"\nfor each i in [\"a\", \"b\", \"c\"]\n    set log to log + i\n    module Inner\n        skip\n        say \"unreachable\"\n    end\n    set log to log + \".\"\nend\nsay log\n".to_string(),
    );
    add(
        "module/a-break-in-a-module-body-in-a-while-leaves-the-while",
        "set n to 0\nset log to \"\"\nwhile n is not 9\n    set n to n + 1\n    module Inner\n        set held to n\n        break\n        set log to log + \"unreachable\"\n    end\n    set log to log + \".\"\nend\nsay n\nsay log\n".to_string(),
    );
    add(
        "module/a-module-body-left-by-a-break-publishes-nothing",
        "set count to 0\nrepeat 2 times\n    set count to count + 1\n    module Gone\n        export value\n        set value to 1\n        break\n    end\nend\nsay count\ntry\n    import Gone\n    say \"imported\"\ncatch error\n    say \"no module\"\nend\n".to_string(),
    );
    // The `finally` a jump passes through runs after the module body has given
    // its scope back, so a `set` in it is a name of the program rather than one
    // of the module the exit just left.
    add(
        "module/a-finally-around-a-module-declaration-runs-after-its-scope-is-gone",
        "set log to \"\"\nrepeat 2 times\n    set log to log + \"t\"\n    try\n        module Inner\n            set held to 1\n            break\n        end\n        set log to log + \"unreachable\"\n    catch error\n        set log to log + \"c\"\n    finally\n        set log to log + \"f\"\n    end\n    set log to log + \".\"\nend\nsay log\n".to_string(),
    );
    // The two refusals: a module body written where there is no loop, and one
    // written inside a function body — a function is not lexically inside the
    // loop that called it, so the module inherits the exemption and the caller's
    // loop survives.
    add(
        "module/a-break-in-a-module-body-outside-a-loop-is-refused",
        "set caught to \"no\"\ntry\n    module Lone\n        break\n    end\ncatch error\n    set caught to \"yes\"\nend\nsay caught\n".to_string(),
    );
    add(
        "module/a-break-in-a-module-body-in-a-function-called-from-a-loop-is-refused",
        "to declare()\n    module Inner\n        break\n    end\nend\nset n to 0\nset caught to \"no\"\nrepeat 2 times\n    set n to n + 1\n    try\n        declare()\n    catch error\n        set caught to \"yes\"\n    end\nend\nsay n\nsay caught\n".to_string(),
    );
    // -- a break or a skip in a block that has a frame of its own -------------
    //
    // A `test`, `catch`, `finally` or `object` body is a block with a frame of
    // its own on the bytecode VM, and it is still written inside the loop around
    // it. Each of these is the shape the two VMs used to answer differently: the
    // tree-walking VM counts the loops a statement is lexically inside, and the
    // bytecode VM looked only at the frame the instruction ran in, so it refused
    // these where the other VM honoured them.
    add(
        "flow/a-break-in-a-catch-body-inside-a-loop-leaves-that-loop",
        "set n to 0\nrepeat 3 times\n    set n to n + 1\n    try\n        set bad to 1 + \"one\"\n    catch error\n        if n is 2 then\n            break\n        end\n        say \"caught\"\n    end\nend\nsay n\n".to_string(),
    );
    add(
        "flow/a-skip-in-a-catch-body-inside-a-loop-advances-that-loop",
        "set seen to \"\"\nfor each i in [\"one\", \"two\", \"three\"]\n    try\n        set bad to 1 + \"one\"\n    catch error\n        if i is \"two\" then\n            skip\n        end\n        set seen to seen + i\n    end\n    set seen to seen + \"!\"\nend\nsay seen\n".to_string(),
    );
    add(
        "flow/a-break-in-a-finally-body-inside-a-loop-leaves-that-loop",
        "set log to \"\"\nset n to 0\nrepeat 3 times\n    set n to n + 1\n    try\n        say \"try\"\n    finally\n        if n is 2 then\n            break\n        end\n        set log to log + \"f\"\n    end\n    set log to log + \".\"\nend\nsay log\nsay n\n".to_string(),
    );
    add(
        "flow/a-break-in-a-test-body-inside-a-loop-leaves-that-loop",
        "set log to \"\"\nset n to 0\nrepeat 3 times\n    set n to n + 1\n    test \"a test written inside a loop\"\n        if n is 2 then\n            break\n        end\n        set log to log + \"t\"\n    end\n    set log to log + \".\"\nend\nsay log\nsay n\n".to_string(),
    );
    add(
        "flow/a-break-in-an-object-body-inside-a-loop-leaves-that-loop",
        "set log to \"\"\nfor each x in [\"a\", \"b\"]\n    set log to log + x\n    object Once\n        has a\n        break\n        set log to log + \"unreachable\"\n    end\n    set log to log + \".\"\nend\nsay log\n".to_string(),
    );
    add(
        "flow/a-break-in-a-block-within-a-block-inside-a-loop-leaves-that-loop",
        "set log to \"\"\nrepeat 2 times\n    set log to log + \"t\"\n    test \"outer\"\n        test \"inner\"\n            break\n            set log to log + \"unreachable\"\n        end\n        set log to log + \"after the inner test\"\n    end\n    set log to log + \".\"\nend\nsay log\n".to_string(),
    );
    // The exemption: a function body is not lexically inside the loop that called
    // it, so the block it is written in records no loop and the `break` is
    // refused — while the loop that called it survives and goes round again.
    add(
        "flow/a-break-in-a-test-body-inside-a-function-called-from-a-loop-is-refused",
        "to escape()\n    test \"a test inside a function inside a loop\"\n        break\n    end\nend\nset caught to no\nset n to 0\nrepeat 2 times\n    set n to n + 1\n    try\n        escape()\n    catch error\n        set caught to yes\n    end\nend\nsay n\nsay caught\n".to_string(),
    );
    // The loop's variable is still bound while the `finally` the signal passed
    // through runs: the turn the `break` stopped has not ended yet.
    add(
        "flow/a-catch-body-reads-the-loop-variable-before-the-turn-ends",
        "set seen to \"none\"\nset i to \"outer\"\nfor each i in [1, 2]\n    try\n        set bad to 1 + \"one\"\n    catch error\n        break\n    finally\n        set seen to i\n    end\nend\nsay seen\nsay i\n".to_string(),
    );
    add(
        "flow/a-break-in-a-nested-try-inside-a-catch-runs-its-finally",
        "set log to \"\"\nset n to 0\nrepeat 3 times\n    set n to n + 1\n    try\n        set bad to 1 + \"one\"\n    catch error\n        try\n            break\n        finally\n            set log to log + \"i\"\n        end\n        set log to log + \"unreachable\"\n    finally\n        set log to log + \"o\"\n    end\n    set log to log + \"after\"\nend\nsay log\nsay n\n".to_string(),
    );
    add(
        "flow/a-break-in-a-finally-inside-a-catch-runs-the-finally-it-passed-through",
        "set seen to \"none\"\nset i to \"outer\"\nfor each i in [1, 2]\n    try\n        set bad to 1 + \"one\"\n    catch error\n        try\n            say \"caught\"\n        finally\n            break\n        end\n    finally\n        set seen to i\n    end\nend\nsay seen\nsay i\n".to_string(),
    );
    add(
        "flow/a-break-in-a-catch-inside-a-loop-inside-a-test-leaves-the-inner-loop",
        "set log to \"\"\nrepeat 2 times\n    test \"a test around the loop\"\n        for each x in [1, 2, 3]\n            try\n                set bad to 1 + \"one\"\n            catch error\n                if x is 2 then\n                    break\n                end\n                set log to log + \"c\"\n            finally\n                set log to log + \"f\"\n            end\n            set log to log + \".\"\n        end\n        set log to log + \"|\"\n    end\n    set log to log + \";\"\nend\nsay log\n".to_string(),
    );
    add(
        "flow/a-skip-in-a-finally-inside-a-loop-advances-the-turn",
        "set log to \"\"\nset n to 0\nrepeat 3 times\n    set n to n + 1\n    try\n        set log to log + \"t\"\n    finally\n        if n is 2 then\n            skip\n        end\n        set log to log + \"f\"\n    end\n    set log to log + \".\"\nend\nsay log\nsay n\n".to_string(),
    );
    // The `skip` counterpart of the two entries above it: a `test` body and an
    // `object` body are the other blocks that have a frame of their own, so
    // `skip` through them is a second branch of the same ownership lookup. The
    // object body is declared under a guard because a name cannot be declared
    // twice, and the turn that skipped is the one turn it is declared on.
    add(
        "flow/a-skip-in-a-test-body-inside-a-loop-advances-the-turn",
        "set log to \"\"\nset n to 0\nrepeat 3 times\n    set n to n + 1\n    test \"a test written inside a loop\"\n        if n is 2 then\n            skip\n        end\n        set log to log + \"t\"\n    end\n    set log to log + \".\"\nend\nsay log\nsay n\n".to_string(),
    );
    add(
        "flow/a-skip-in-an-object-body-inside-a-loop-advances-the-turn",
        "set log to \"\"\nset declared to no\nfor each x in [\"a\", \"b\", \"c\"]\n    if declared is no then\n        set declared to yes\n        object Once\n            has a\n            if x is \"a\" then\n                skip\n            end\n            set log to log + \"o\"\n        end\n    end\n    set log to log + x\nend\nsay log\n".to_string(),
    );
    add(
        "flow/a-break-in-a-catch-body-inside-a-while-leaves-the-while",
        "set n to 0\nwhile n is not 9\n    set n to n + 1\n    try\n        set bad to 1 + \"one\"\n    catch error\n        break\n    end\n    say \"after the try\"\nend\nsay n\n".to_string(),
    );
    add(
        "flow/a-break-in-a-catch-body-inside-a-loop-inside-a-function-leaves-the-loop",
        "to walk(items)\n    set log to \"\"\n    for each item in items\n        try\n            set bad to 1 + \"one\"\n        catch error\n            if item is \"two\" then\n                break\n            end\n            set log to log + \"c\"\n        end\n        set log to log + item\n    end\n    give back log\nend\nsay walk([\"one\", \"two\", \"three\"])\n".to_string(),
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
        "flow/repeat-a-fraction-with-a-break-in-it",
        "set n to 0\nrepeat 2.5 times\n    set n to n + 1\n    say n\n    break\nend\nsay n\n"
            .to_string(),
    );
    add(
        "flow/repeat-a-fraction-with-a-skip-in-it",
        "set log to \"\"\nrepeat 3.5 times\n    set log to log + \".\"\n    skip\n    set log to log + \"x\"\nend\nsay log\n"
            .to_string(),
    );
    add(
        "flow/repeat-a-negative-count",
        "set n to 0\nrepeat -5 times\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add(
        "flow/repeat-a-negative-count-with-a-break-that-never-runs",
        "set n to 0\nrepeat -1 times\n    set n to n + 1\n    break\nend\nsay n\n".to_string(),
    );
    add(
        "flow/repeat-a-count-too-large-to-count",
        "set n to 0\nrepeat 1e300 times\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add(
        "flow/repeat-a-count-too-large-to-count-with-a-break-in-it",
        "set n to 0\nrepeat 99999999999999999999 times\n    set n to n + 1\n    break\nend\nsay n\n"
            .to_string(),
    );
    add(
        "flow/repeat-a-non-number",
        "set n to 0\nrepeat \"three\" times\n    set n to n + 1\nend\nsay n\n".to_string(),
    );
    add(
        "flow/repeat-a-non-number-with-a-break-in-it",
        "set n to 0\nrepeat nothing times\n    set n to n + 1\n    break\nend\nsay n\n".to_string(),
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
    // An `object` body written inside another `object`'s body. The bytecode VM
    // held the declaration being assembled in one slot, so the inner
    // declaration took the outer one's place and the outer body's finish found
    // nothing left — a panic, where the tree-walking VM ran the program. See
    // `objects_an_object_declared_inside_an_object_body_declares_both_types`.
    add(
        "object/an-object-declared-inside-an-object-body",
        "object Outer\n    has o default 1\n    object Inner\n        has i default 2\n    end\nend\n\
         say Outer.o + Inner.i\n"
            .to_string(),
    );
    add(
        "object/a-nested-declaration-may-extend-the-body-it-is-written-in",
        "object Outer\n    has o default 1\n    object Inner extends Outer\n        has i default 2\n    end\n\
         say Inner.o + Inner.i\n"
            .to_string(),
    );
    add(
        "object/a-nested-declaration-may-extend-two-levels-up",
        "object One\n    has n default 1\n    object Two\n        object Three extends One\n            has k default 3\n\
         end\n    end\nend\nsay Three.n + Three.k\n"
            .to_string(),
    );
    add(
        "object/three-nested-object-bodies",
        "object L1\n    object L2\n        object L3\n            has deep default \"deep\"\n        end\n    end\nend\n\
         say \"ran\"\n"
            .to_string(),
    );
    add(
        "object/two-declarations-nested-in-the-same-body",
        "object Outer\n    has o default 1\n    object First\n        has a default 1\n    end\n\
         object Second extends Outer\n        has b default 2\n    end\nend\nsay First.a + Second.b + Second.o\n"
            .to_string(),
    );
    add(
        "object/a-nested-declaration-reusing-the-enclosing-name-is-refused",
        "object A\n    has a default 1\n    object A\n        has b default 2\n    end\nend\nsay \"ran\"\n"
            .to_string(),
    );
    add(
        "object/a-nested-declaration-extending-its-own-name-is-a-cycle",
        "object A\n    object B extends B\n        has b default 1\n    end\nend\nsay \"ran\"\n"
            .to_string(),
    );
    add(
        "object/a-failure-inside-a-nested-body-is-caught-outside-both",
        "set caught to \"no\"\ntry\n    object A\n        object B\n            has b default 1\n            set bad to 1 + \"one\"\n\
         end\n    end\ncatch error\n    set caught to \"yes\"\nend\nsay caught\n"
            .to_string(),
    );
    add(
        "object/a-failure-inside-a-nested-body-is-caught-inside-the-outer-one",
        "set after to \"no\"\nobject A\n    object B\n        has b default 1\n        try\n            set bad to 1 + \"one\"\n\
         catch error\n            say \"inner\"\n        end\n    end\n    set after to \"yes\"\nend\nsay after\n"
            .to_string(),
    );
    add(
        "object/an-object-body-nested-in-a-loop-can-break-out-of-it",
        "set n to 0\nrepeat 3 times\n    object A\n        object B\n            has b default 1\n        end\n        set n to n + 1\n\
         break\n    end\nend\nsay n\n"
            .to_string(),
    );
    add(
        "object/a-nested-body-that-recovers-leaves-both-types-declared",
        "object Outer\n    has o default 1\n    object Inner\n        has i default 2\n        try\n            set bad to 1 + \"one\"\n    \
         catch error\n            set inner_ok to \"caught\"\n        end\n    end\n    set outer_ok to \"registered\"\nend\nsay Outer.o\nsay outer_ok\nsay inner_ok\n"
            .to_string(),
    );
    // A `has` default is compiled as an expression, so it can open a
    // declaration of its own — and that declaration's frame records a
    // `pending_objects` height above zero. A `catch` that abandons it must take
    // only its own entry off the stack of declarations being assembled: taking
    // one more took the *enclosing* body's entry with it, and the outer body
    // then found nothing left to register.
    add(
        "object/a-declaration-opened-by-a-has-default-a-failure-abandons",
        "to maker()\n    try\n        object Inner\n            has x default 1 + \"one\"\n        end\n        set reached to \"no failure\"\n    \
         catch error\n        set reached to \"caught\"\n    end\n    give back reached\nend\nobject A\n    has a default maker()\nend\nsay A.a\n"
            .to_string(),
    );
    add(
        "object/a-declaration-opened-by-a-has-default-that-succeeds-registers-both",
        "to maker()\n    object Inner\n        has x default 5\n    end\n    give back 1\nend\nobject A\n    has a default maker()\nend\nsay A.a\n\
         say \"ran\"\n"
            .to_string(),
    );
    // A body the program leaves through a *failure* in the statements after its
    // declarations: the type is registered on both VMs, because each registers
    // it before those statements run.
    add(
        "object/an-object-body-left-through-a-failure-has-registered-its-type",
        "try\n    object Outer\n        has o default 1\n        set bad to 1 + \"one\"\n    end\ncatch error\n    say \"caught\"\n\
         end\nset Outer.o to 5\nsay Outer.o\n"
            .to_string(),
    );
    // The other half of that rule: a failure in a `has` default leaves nothing
    // worth registering, so the name is not bound on either VM.
    add(
        "object/an-object-body-whose-declaration-failed-registers-nothing",
        "try\n    object Half\n        has h default 1 + \"one\"\n    end\ncatch error\n    say \"caught\"\nend\ntry\n    say Half.h\ncatch error\n    say \
         \"no Half\"\nend\n"
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
        "try/a-catch-inside-a-function-body-leaves-the-calls-own-scope-alone",
        "to add(x)\n    set total to 0\n    try\n        set bad to 1 + \"one\"\n    catch error\n        set caught to yes\n    end\n    set total to total + x\n    give back total\nend\nsay add(7)\n".to_string(),
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
        "try/a-failure-with-nothing-around-it-stops-the-program-after-the-finally",
        // The form `SPEC.md`'s \"Finally\" documents needs no `catch` beside it,
        // so a failure in the protected region has nothing there to handle it.
        // The tree-walking VM returned above `run_finally_body` and skipped the
        // cleanup on exactly that path; the bytecode VM ran the cleanup and then
        // answered `true` — swallowing the failure and carrying on past the
        // marked `NOP`. The `say` in the `finally` is the half that was missing
        // on one engine, and the `RuntimeError` is the half the other one lost.
        "try\n    say 1 / 0\nfinally\n    say \"cleaned\"\nend\nsay \"unreachable\"\n".to_string(),
    );
    add(
        "try/a-finally-runs-on-the-way-out-of-a-failure-with-no-catch",
        "set log to \"\"\ntry\n    try\n        set bad to 1 + \"one\"\n    finally\n        set log to log + \"f\"\n    end\ncatch error\n    set log to log + \"c\"\nend\nsay log\n".to_string(),
    );
    add(
        "try/a-failing-finally-inside-a-try-with-no-catch-is-the-one-that-propagates",
        // Two failures, one protected region: the protected code's and the
        // cleanup's, and they say different things so the recorded message says
        // which one left. The cleanup is what the program hears about, because it
        // is the last thing that happened on the way out — and both VMs have to
        // agree on that rather than one reporting a failure the other swapped.
        "try\n    try\n        set a to 1 + \"one\"\n    finally\n        set b to 1 / 0\n    end\nend\nsay \"unreachable\"\n".to_string(),
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
        "try/a-try-with-no-catch-hands-the-failure-to-the-try-around-it",
        "set reached to \"no\"\ntry\n    try\n        set bad to 1 + \"one\"\n    end\n    set reached to \"yes\"\ncatch error\n    set reached to \"caught\"\nend\nsay reached\n".to_string(),
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
    // Assert the error KIND, not OS phrasing: Unix says "No such file ...",
    // Windows says "The system cannot find the file ...", and both print the
    // `IoError` label the binary itself attaches. Matching the label keeps this
    // green on every platform without weakening what it proves.
    assert!(
        stderr.contains("IoError"),
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

/// Held for as long as a test is walking the corpus.
///
/// Three tests here run every program in the corpus, and one of them —
/// `examples/files.rb` — writes `output.txt`, `output_copy.txt` and
/// `renamed.txt` **by relative path**, in the process's own working directory.
/// Two tests walking the corpus at once are therefore two runs of that program
/// interleaved over one set of files, and the residue of one is read by the
/// other: the second run appends to a file the first has not deleted yet and
/// fails to rename a copy the first has already moved. That is a disagreement
/// about the filesystem, not about the two VMs, and it is what
/// `a_corpus_of_programs_runs_identically_on_both_vms` reported before this
/// lock existed.
///
/// Serialised rather than excluded: every program still runs on both VMs in
/// every test that walks the corpus, which is the whole value of the lock. The
/// alternative — dropping `examples/files.rb` from the comparison — would take
/// it out of the gate to make the gate quiet, and that is not a trade worth
/// making for a test file's convenience.
static CORPUS_WALK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Takes [`CORPUS_WALK`], recovering from a poisoned lock.
///
/// A test that panics while holding it leaves the mutex poisoned, and the tests
/// that follow would otherwise refuse to run for a reason that has nothing to do
/// with them: the corpus walk has no shared state to corrupt beyond the files it
/// has already cleaned up, so the guard is taken whatever the poisoning.
#[track_caller]
fn walking_the_corpus() -> std::sync::MutexGuard<'static, ()> {
    CORPUS_WALK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The differential test the phase requires: both VMs agree on every program in
/// the corpus, which is at least 200 whole programs drawn from `examples/`,
/// `modules/`, `tests/` and the generated set.
#[test]
fn a_corpus_of_programs_runs_identically_on_both_vms() {
    let _walking = walking_the_corpus();
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
    let _walking = walking_the_corpus();
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
/// under the other — and names the limit that stopped it in the same words.
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

    // The same loop with a body of several statements is the case the
    // single-statement program above cannot see. While the two engines charged
    // the budget in different units — a statement here, an instruction there —
    // a body long enough for the ratio between them to matter was stopped by the
    // budget on one engine and by the cap on the other, with different words
    // about the same program.
    //
    // The limits are brought down together rather than run at their published
    // values: the loop turns a million times at those, which takes seconds, and
    // what is being asked is *which* limit the accounting reaches first, not how
    // long a million turns is. Ten turns of this body cost ninety steps, so a
    // budget of a hundred is spent after the cap has been reached — on the tree,
    // on an engine that charges instructions, and on one that charges statements.
    let wide = concat!(
        "set n to 0\n",
        "repeat 1e300 times\n",
        "    repeat 2 times\n",
        "        set n to n + 1\n",
        "        skip\n",
        "    end\n",
        "    set n to n + 1\n",
        "end\n",
        "say n\n",
    );
    let (tree, byte) = assert_agrees_bounded(wide, 10, 100);
    assert_eq!(
        tree.result,
        Err("RuntimeError: Maximum of 10 iterations reached in a 'repeat' loop".to_string()),
        "the cap, not the budget, is what stops this program, on both engines"
    );
    assert_eq!(byte.result, tree.result, "and both engines say so");

    // The other order: a budget small enough to be spent inside the ten turns the
    // cap allows. A body that prints turns reports how far each engine got, so
    // an engine that counted differently could not reach the same turn.
    let counted = concat!(
        "set n to 0\n",
        "repeat 100 times\n",
        "    set n to n + 1\n",
        "    say n\n",
        "    skip\n",
        "    say \"unreached\"\n",
        "end\n",
    );
    let (tree, byte) = assert_agrees_bounded(counted, 100, 20);
    assert_eq!(
        tree.result,
        Err("RuntimeError: Step budget of 20 reached before the program finished".to_string()),
        "the budget is what stops this one, on both engines"
    );
    assert_eq!(
        tree.output.len(),
        byte.output.len(),
        "and both engines stop it after the same number of turns"
    );
    assert!(
        !tree.output.is_empty() && !tree.output.contains(&"unreached".to_string()),
        "the body printed what it did and left what the budget cut off: {:?}",
        tree.output
    );
}

/// An `export` in a module body is the one statement the tree-walking VM does not
/// charge a step for: the declaration reads the names and skips running them, so
/// they cost nothing there. The bytecode compiler was marking every statement
/// alike, so a module body cost one step more per `export` than it does on the
/// other engine — and the same program was stopped by the budget at a different
/// statement depending on which engine ran it.
///
/// The budget here is the program's exact cost on both engines, so the two have
/// to agree about which side of it the program falls on.
#[test]
fn edge_a_module_export_is_charged_the_same_step_on_both_engines() {
    let module = concat!(
        "module M\n",
        "    export all\n",
        "    to one\n",
        "        give back 1\n",
        "    end\n",
        "    to two\n",
        "        give back 2\n",
        "    end\n",
        "end\n",
        "import M\n",
        "say M.one()\n",
        "say M.two()\n",
    );
    assert_agrees(module);

    // One step under the cost of the program: both engines stop it, and both say
    // it was the budget rather than anything about the module.
    let (tree, byte) = assert_agrees_bounded(module, 1000, 9);
    assert_eq!(
        tree.result,
        Err("RuntimeError: Step budget of 9 reached before the program finished".to_string()),
        "one step short is what stops this program, on both engines"
    );
    assert_eq!(byte.result, tree.result, "and both engines say so");

    // Exactly the cost of the program: both engines run it to the end. This is
    // the case that failed before — the tree VM finished here while the bytecode
    // VM, one `export` later, reported the budget spent.
    let (tree, byte) = assert_agrees_bounded(module, 1000, 10);
    assert_eq!(
        tree.result,
        Ok("nothing".to_string()),
        "the program fits in its own step count on the tree"
    );
    assert_eq!(byte.result, tree.result, "and on the bytecode engine");
    assert_eq!(
        byte.output,
        vec!["1".to_string(), "2".to_string()],
        "the module's members are callable through both engines alike"
    );

    // The other side of the rule: an `export` *outside* a module declaration is
    // an ordinary statement on the tree — it publishes nothing and is run like
    // any other — so both engines must still charge it. Counting no export at
    // all would pass the module above and break this.
    let bare = "export all\nsay 1\n";
    assert_agrees(bare);
    let (tree, byte) = assert_agrees_bounded(bare, 1000, 1);
    assert_eq!(
        tree.result,
        Err("RuntimeError: Step budget of 1 reached before the program finished".to_string()),
        "a top-level export is charged a step, on both engines"
    );
    assert_eq!(byte.result, tree.result, "and both engines say so");
}

/// A `has` field and a `to can` method are collected by the `object`
/// declaration, not run as statements: the tree-walking VM's `declare_object`
/// takes both with a bare `evaluate` and `make_function`, so only the `object`
/// statement itself costs a step there. The bytecode compiler was marking every
/// statement alike, so an object cost one step per field plus one per method more
/// than it does on the other engine — and the same program, under the same
/// budget, was stopped at a different statement with different words about it.
///
/// The budget here is the program's exact cost on both engines, so the two have
/// to agree about which side of it the program falls on.
#[test]
fn edge_an_object_declaration_is_charged_the_same_step_on_both_engines() {
    // Two fields and two methods, plus one statement after the declarations and
    // one after the declaration: three steps on the tree, and three here.
    let declared = concat!(
        "object Thing\n",
        "    has size default 1\n",
        "    has label default \"x\"\n",
        "    to can grow(amount)\n",
        "        say amount\n",
        "    end\n",
        "    to can shrink(amount)\n",
        "        say amount\n",
        "    end\n",
        "    set Thing.size to 2\n",
        "end\n",
        "say Thing.size\n",
    );
    assert_agrees(declared);

    // One step under the cost of the program: both engines stop it, and both say
    // it was the budget rather than anything about the object.
    let (tree, byte) = assert_agrees_bounded(declared, 1000, 2);
    assert_eq!(
        tree.result,
        Err("RuntimeError: Step budget of 2 reached before the program finished".to_string()),
        "one step short is what stops this program, on both engines"
    );
    assert_eq!(byte.result, tree.result, "and both engines say so");

    // Exactly the cost of the program: both engines run it to the end. This is
    // the case that failed before — the tree VM finished here while the bytecode
    // VM, four declarations later, reported the budget spent.
    let (tree, byte) = assert_agrees_bounded(declared, 1000, 3);
    assert_eq!(
        tree.result,
        Ok("nothing".to_string()),
        "the program fits in its own step count on the tree"
    );
    assert_eq!(byte.result, tree.result, "and on the bytecode engine");
    assert_eq!(
        byte.output,
        vec!["2".to_string()],
        "the statement that follows the declarations ran on both engines alike"
    );

    // The same program with the declarations removed costs the same, which is the
    // claim in one line: what an object declares is not what it runs.
    let bare = "object Thing\nend\nsay 2\n";
    assert_agrees(bare);
    let (tree, byte) = assert_agrees_bounded(bare, 1000, 2);
    assert_eq!(
        tree.result,
        Ok("nothing".to_string()),
        "an object with no declarations and one with four cost the same on the tree"
    );
    assert_eq!(byte.result, tree.result, "and on the bytecode engine");
    let (tree, _) = assert_agrees_bounded(bare, 1000, 1);
    assert_eq!(
        tree.result,
        Err("RuntimeError: Step budget of 1 reached before the program finished".to_string()),
        "and one step short of *that* stops it, on both engines"
    );

    // A method's body is an ordinary statement list, so what it runs is still
    // charged — once the call is reached, and once per statement in the body.
    let called = concat!(
        "object Thing\n",
        "    has size default 1\n",
        "    to can grow(amount)\n",
        "        say amount\n",
        "        say Thing.size\n",
        "    end\n",
        "end\n",
        "say Thing.grow(7)\n",
    );
    assert_agrees(called);
    let (tree, byte) = assert_agrees_bounded(called, 1000, 4);
    assert_eq!(
        tree.result,
        Ok("nothing".to_string()),
        "`object`, the call's `say`, and the body's two statements: four steps on the tree"
    );
    assert_eq!(
        byte.result, tree.result,
        "and the same four on the bytecode engine"
    );
    assert_eq!(
        byte.output,
        vec!["7".to_string(), "1".to_string(), "nothing".to_string()],
        "the body ran once, so its two statements printed once each"
    );
    let (tree, byte) = assert_agrees_bounded(called, 1000, 3);
    assert_eq!(
        tree.result,
        Err("RuntimeError: Step budget of 3 reached before the program finished".to_string()),
        "one statement short of the body is where three steps stops it, on both engines"
    );
    assert_eq!(byte.result, tree.result, "and both engines say so");
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

/// Every shape of a `repeat` count is read through `expect_repeat_count` on both
/// engines, so each engine's turn count is checked against the helper *itself*
/// here rather than only against the other engine.
///
/// That is what makes this a pin and not a change-detector. The two engines
/// used to narrow the count on their own, and the casts they each reached for
/// happened to agree on every value a program can produce — so a test that only
/// compared the two engines would have passed before the helper existed and
/// passes now, and could not tell the two apart. Asking the helper what the
/// turns are and then counting the turns each engine ran is a question with a
/// different answer if either of them stops using it: a tree that read `2.5` as
/// three turns, or a bytecode VM that read `3` as three turns and `-5` as five,
/// fails here even though the other engine still agrees with it.
///
/// A `break` and a `skip` in the body are there because a turn that ends early
/// is the only thing that can make the two disagree about *which* turn they are
/// on, and a count with no turn in it must leave the loop without either signal
/// being raised.
#[test]
fn edge_both_vms_read_a_repeat_count_the_same_way() {
    // What the source writes, what the helper is asked about, and how many turns
    // that comes to.
    let counts = [
        ("3", Value::Number(3.0), 3usize),
        // Whole turns before the fraction, not one more than them.
        ("2.5", Value::Number(2.5), 2),
        ("0.5", Value::Number(0.5), 0),
        // No turn to start from, and no turn to count back to.
        ("-5", Value::Number(-5.0), 0),
        ("-0.5", Value::Number(-0.5), 0),
        // Not a count of anything, so not a loop.
        ("\"five\"", Value::Text("five".to_string()), 0),
        ("nothing", Value::Nothing, 0),
    ];
    let bodies = ["break", "skip", "set n to n + 1"];

    for (count, value, turns) in counts {
        assert_eq!(
            expect_repeat_count(&value, redblue::Span::unknown())
                .unwrap_or_else(|error| panic!("count {count} should be a count: {error:?}")),
            turns as i64,
            "the table's own expectation of count {count} is what the helper says"
        );

        for body in bodies {
            let source = format!(
                "set n to 0\nrepeat {count} times\n    say \"turn\"\n    {body}\n    say \"after\"\nend\nsay n\n"
            );
            let (tree, byte) = assert_agrees(&source);
            // A `break` leaves after the turn it is in; a `skip` and a plain
            // statement leave every turn of the count to run.
            let expected = if body == "break" { turns.min(1) } else { turns };
            let printed = tree.output.iter().filter(|line| *line == "turn").count();
            assert_eq!(
                printed, expected,
                "count {count} with `{body}` should have run {expected} turn(s)"
            );
            assert_eq!(
                byte.output.iter().filter(|line| *line == "turn").count(),
                printed,
                "and the bytecode VM ran the same turns"
            );
        }
    }
}

/// A count past `i64` saturates into the largest countable count, and what stops
/// such a loop is the iteration cap — or a `break` on its first turn. The turn
/// count is checked against `expect_repeat_count` itself, because a cap and a
/// `break` are reached at the same turn on both engines only if both read the
/// count as the same number of turns; asked of the two engines alone, an engine
/// that saturated some other way would be caught only if the other one did not.
/// Asked with the published cap this would be a million turns of printing, so the
/// cap is lowered to five — which is also the only way to ask the question at all.
#[test]
fn edge_both_vms_saturate_a_count_beyond_i64_the_same_way() {
    for count in ["99999999999999999999", "1e300"] {
        assert_eq!(
            expect_repeat_count(&Value::Number(1e300), redblue::Span::unknown())
                .unwrap_or_else(|error| panic!("a count this large is still a count: {error:?}")),
            i64::MAX,
            "a count past the width of a counter saturates rather than refusing, and it \
             saturates to the largest countable count rather than to nothing"
        );

        let source = format!("set n to 0\nrepeat {count} times\n    say \"turn\"\nend\n");
        let (tree, byte) = assert_agrees_capped(&source, 5);
        assert!(
            tree.result.is_err() && byte.result.is_err(),
            "a saturated count must be a failure on both engines, got tree {:?} and bytecode {:?}",
            tree.result,
            byte.result
        );
        assert_eq!(
            tree.result,
            Err("RuntimeError: Maximum of 5 iterations reached in a 'repeat' loop".to_string()),
            "count {count} must be stopped by the cap, with the same words on both engines"
        );
        assert_eq!(tree.output.len(), 5, "both engines print the same turns");
        assert_eq!(byte.output.len(), 5, "and the bytecode VM agrees");

        // A `break` on the first turn is the way out of a count that will not
        // run out, on either engine, and it is the only turn the body runs.
        let broke = format!(
            "set n to 0\nrepeat {count} times\n    set n to n + 1\n    break\n    say \"after\"\nend\nsay n\n"
        );
        let (tree, byte) = assert_agrees_capped(&broke, 5);
        assert!(
            tree.result.is_ok(),
            "a break on the first turn must finish, got {:?}",
            tree.result
        );
        assert_eq!(
            tree.output,
            vec!["1".to_string()],
            "exactly one turn ran, and the break left the rest of it out"
        );
        assert_eq!(byte.output, tree.output, "and both engines agree");
    }
}

/// A count with no turn in it — negative, however large its magnitude — is not a
/// loop on either engine, so the `break` written in its body is never raised:
/// there is no turn to reach it, and nothing has left a loop to complain about.
#[test]
fn edge_both_vms_run_no_turns_of_a_count_with_no_turns_in_it() {
    for count in ["-5", "-0.5", "-1e300"] {
        let source = format!(
            "set n to 0\nrepeat {count} times\n    set n to n + 1\n    say \"turn\"\n    break\nend\nsay n\n"
        );

        let (tree, byte) = assert_agrees_capped(&source, 5);

        assert!(
            tree.result.is_ok(),
            "count {count} must not be a failure, got {:?}",
            tree.result
        );
        assert_eq!(
            tree.output,
            vec!["0".to_string()],
            "count {count} runs no turns, so the break in it is never raised"
        );
        assert_eq!(byte.output, tree.output, "and both engines agree");
    }
}

/// A fractional count is its whole turns, and a `break` in that body leaves the
/// loop after the first of them: the fraction does not buy a turn, and the two
/// engines must not disagree about either half of that.
#[test]
fn edge_both_vms_truncate_a_fractional_count_the_same_way() {
    let source = concat!(
        "set n to 0\n",
        "repeat 2.5 times\n",
        "    set n to n + 1\n",
        "    break\n",
        "end\n",
        "say n\n",
    );

    let (tree, byte) = assert_agrees(source);

    let expected = vec!["1".to_string()];
    assert_eq!(tree.output, expected, "one whole turn, then the break");
    assert_eq!(byte.output, expected, "one whole turn, then the break");
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
    let _walking = walking_the_corpus();
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

/// A loop's `STORE` binds its variable by drawing from the sequence the
/// `GET_ITER` or `GET_RANGE` above it built. No file this compiler writes can
/// reach that `STORE` without one, but a hand-built file can: the loop is found
/// from its backward jump, so a file whose first jump lands on the `STORE`
/// itself looks like a loop with no values to draw.
///
/// That has to be reported. `src/bytecode/format.rs` says a file the compiler
/// did not write is a diagnostic, and a diagnostic is an error message — an
/// `expect` on the sequence turns a hostile file into a process abort, which is
/// the one thing a VM reading untrusted bytes must not do.
#[test]
fn edge_a_loop_variable_stored_with_no_sequence_is_an_error_not_a_panic() {
    let source = "for each v in [1, 2]\n    say v\nend\nsay \"done\"\n";
    let well_formed = compile_source(source).expect("the program should compile");

    // The control: the same file, untouched, loops over both values and says
    // what came after it. So the failure below is about the one instruction this
    // file changes and not about the shape of the loop.
    let mut vm = BytecodeVm::new();
    vm.run(&well_formed).expect("the untouched file should run");
    assert_eq!(
        vm.take_output(),
        vec!["1".to_string(), "2".to_string(), "done".to_string()],
        "the control file runs its loop and then the statement after it"
    );

    // The store of the loop variable: the instruction after the `GET_ITER`, and
    // so the instruction this file's jump will land on.
    let iterable = well_formed
        .main
        .code
        .iter()
        .position(|instruction| instruction.opcode == Opcode::GetIter)
        .expect("the file has a GET_ITER");
    let top = iterable + 1;

    let mut malformed = well_formed.clone();
    // The `PUSH_CONST` that builds the list becomes a jump onto the `STORE`, so
    // the loop opens on a turn with nothing to draw from.
    malformed.main.code[iterable - 1] = Instruction {
        opcode: Opcode::Jump,
        arg: top as u32,
        aux: 0,
        line: 1,
    };

    let mut vm = BytecodeVm::new();
    let error = vm
        .run(&malformed)
        .expect_err("a loop variable bound with no sequence must fail");
    assert_eq!(
        error.label(),
        "RuntimeError",
        "a hostile file is a runtime diagnostic, not a panic and not silence"
    );
    assert!(
        error.message().contains("no sequence"),
        "the failure should say the loop had no sequence, said: {}",
        error.message()
    );
    assert_eq!(
        vm.take_output(),
        Vec::<String>::new(),
        "nothing before the jump printed, and nothing after the failure did"
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

/// A cap of N is N turns on both VMs, for every loop form — and a turn that a
/// `skip` abandons is a turn.
///
/// The tree-walking VM charges at the top of every turn, before the body runs.
/// The bytecode VM charged at the backward `JUMP` that ends one, which is the
/// turn *after* the one it charges, and drew a `while`'s entry only when it
/// turned over — so its cap allowed one turn more than the tree-walking VM's. A
/// `skip` was worse than off by one: it jumps to the loop's `top` over that
/// `JUMP`, so it spent nothing at all, and a `while` that skipped every turn ran
/// until the ten-million-statement step budget stopped it rather than reporting
/// `Maximum of N iterations`. Every program here counts its own turns, so the
/// cap's answer is visible rather than inferred from the message.
#[test]
fn edge_a_cap_counts_turns_on_both_vms_and_a_skipped_turn_is_one() {
    // `set turns to turns + 1` is the first statement of every body, so a
    // skipped turn still counts: that is the property under test, and a `skip`
    // placed after it would not exercise it.
    let looping = |statement: &str, head: &str| {
        format!(
            "try\n    set turns to 0\n    {head}\n        set turns to turns + 1\n\
             {statement}\n    end\ncatch error\n    say \"stopped\"\n    say turns\nend\n"
        )
    };
    let cases: [(&str, String); 6] = [
        ("while", looping("", "while turns is not 100")),
        (
            "while/skip",
            looping("        skip", "while turns is not 100"),
        ),
        ("repeat", looping("", "repeat 100 times")),
        ("repeat/skip", looping("        skip", "repeat 100 times")),
        (
            "for each",
            looping("", "for each i in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]"),
        ),
        (
            "for each/skip",
            looping(
                "        skip",
                "for each i in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]",
            ),
        ),
    ];

    for cap in [1, 3, 4] {
        for (name, source) in &cases {
            let (tree, _) = assert_agrees_capped(source, cap);
            assert_eq!(
                tree.output,
                vec!["stopped".to_string(), cap.to_string()],
                "a cap of {cap} must be {cap} turns of a {name} loop on both VMs, \
                 and no more"
            );
        }
    }
}

/// The cap is the last line of defence rather than the only one: a `while` that
/// skips every turn is still a loop, and charging it is what stops it rather
/// than the program's step budget.
#[test]
fn edge_a_while_that_skips_every_turn_is_stopped_by_the_iteration_cap() {
    let source = "set n to 0\nwhile yes is yes\n    set n to n + 1\n    skip\nend\n";

    let (tree, byte) = assert_agrees_capped(source, 5);
    let message = tree
        .result
        .as_ref()
        .expect_err("a loop that skips every turn must still be bounded");
    assert!(
        message.contains("Maximum of 5 iterations"),
        "the per-loop cap must be what stops it, said: {message}"
    );
    assert!(
        byte.result.is_err(),
        "and the bytecode VM must stop there too, said: {byte:?}"
    );
}

/// The `finally` of a `try` with no `catch` is owed however the region is left,
/// and the failure that leaves afterwards is the *last* one — the cleanup's, if
/// the cleanup failed.
///
/// Both VMs place the cleanup below the propagated failure, so the original is
/// spent: the tree-walking VM's `run_finally_body(...)?` sits under the
/// `return Err(failure)`, and the bytecode VM's search for the handler that takes
/// the failure finds none left once the cleanup's has been taken. The two
/// failures here say different things, so which one the program is told about is
/// visible rather than assumed.
#[test]
fn edge_a_failing_finally_is_the_failure_a_try_with_no_catch_reports() {
    let source = concat!(
        "try\n",
        "    try\n",
        "        set a to 1 + \"one\"\n",
        "    finally\n",
        "        set b to 1 / 0\n",
        "    end\n",
        "end\n",
        "say \"unreachable\"\n",
    );

    let (tree, _) = assert_agrees(source);
    let message = tree
        .result
        .as_ref()
        .expect_err("both failures must stop the program");
    assert!(
        message.contains("Division by zero"),
        "the cleanup's failure is the one that leaves, said: {message}"
    );
    assert!(
        !message.contains("non-numbers"),
        "the protected region's own failure is spent, said: {message}"
    );
    assert!(
        tree.output.is_empty(),
        "and nothing after the `try` runs, said: {:?}",
        tree.output
    );
}

/// A loop whose turn is unwound by a failure is *abandoned*, not resumed: the
/// tree-walking VM propagates the failure out of the loop, so the statements
/// after the failing statement — the rest of the body, and the loop's own
/// back edge — never run.
///
/// This is the shape where the two engines disagreed. The bytecode VM handled
/// the failure, unwound the loops the `try` was written inside, and put the
/// frame after the `try`; `prepare_exit` then made an entry for the loop the
/// `break` was leaving, so `leave_owned_loop` found one and sent the frame to
/// *that* loop's exit instead — which is the statement after the inner loop, and
/// which the abandoned turn then ran. The two landed on the same instruction by
/// accident, through a second abandoned loop's exit, so a program with nothing
/// after the inner loop hid it. This one has something, and both engines must
/// agree about what it prints.
#[test]
fn edge_a_loop_abandoned_by_a_failing_finally_does_not_run_the_rest_of_its_body() {
    let source = concat!(
        "set log to \"\"\n",
        "set caught to no\n",
        "try\n",
        "    repeat 3 times\n",
        "        for each i in [1, 2]\n",
        "            try\n",
        "                break\n",
        "            finally\n",
        "                set bad to 1 + \"one\"\n",
        "            end\n",
        "        end\n",
        "        set log to log + \".\"\n",
        "    end\n",
        "catch error\n",
        "    set caught to yes\n",
        "end\n",
        "say caught\n",
        "say log\n",
    );

    let (tree, byte) = assert_agrees(source);
    assert_eq!(
        tree.result,
        Ok("nothing".to_string()),
        "the failure is caught and the program goes on to the two `say`s"
    );
    assert_eq!(
        tree.output,
        vec!["yes".to_string(), String::new()],
        "the abandoned turn ran nothing: the `set` after the inner loop is inside the \
         turn that failed, and `log` is still empty"
    );
    assert_eq!(byte.output, tree.output, "and the bytecode engine agrees");
}

/// A `break` in a `catch` body written around a `try` with no `catch` still leaves
/// the loop the `catch` is written in, and the `finally` it passed through still
/// runs: the two halves of `prepare_exit` are the same whether the `try` it
/// crosses has a `catch` or only a `finally`.
#[test]
fn edge_a_break_in_a_catch_around_a_catch_less_try_leaves_the_loop_and_cleans_up() {
    let source = concat!(
        "set turns to 0\n",
        "repeat 3 times\n",
        "    set turns to turns + 1\n",
        "    try\n",
        "        try\n",
        "            set bad to 1 + \"one\"\n",
        "        finally\n",
        "            say \"cleaned\"\n",
        "        end\n",
        "    catch error\n",
        "        if turns is 2 then\n",
        "            break\n",
        "        end\n",
        "    end\n",
        "end\n",
        "say turns\n",
    );

    let (tree, _) = assert_agrees(source);
    assert_eq!(
        tree.output,
        vec![
            "cleaned".to_string(),
            "cleaned".to_string(),
            "2".to_string()
        ],
        "two turns were cleaned up and the second one broke out of the loop"
    );
}

/// A cap of N allows a loop N turns, and N turns is a loop that ends: a `while`
/// whose condition goes false on its last turn has finished, not run out of
/// budget.
///
/// The charge is made where the tree-walking VM makes it — after the condition
/// comes out true, before the body — so the cap counts turns that *began*. A
/// charge at the backward `JUMP` instead counts a turn before knowing its
/// condition holds, which is what makes a cap of N refuse a `while` of N turns on
/// this VM and run it on the other.
#[test]
fn edge_a_cap_allows_a_loop_exactly_that_many_turns() {
    // The body's first statement counts, so what the program prints is how many
    // turns the cap allowed. `TURNS` is fixed and only the cap moves, so the
    // boundary is asked about rather than the loop being written to the cap.
    const TURNS: usize = 5;
    let looping = |head: &str, statement: &str| {
        format!("set turns to 0\n{head}\n    set turns to turns + 1\n{statement}end\nsay turns\n")
    };
    // The last case is the nesting one: each outer turn starts a fresh inner
    // loop, so an inner loop that kept its entry past its own last turn would
    // charge the sum of its siblings' turns rather than its own.
    let cases: [(&str, String); 5] = [
        ("while", looping("while turns is not 5", "")),
        ("while/skip", looping("while turns is not 5", "    skip\n")),
        ("repeat", looping("repeat 5 times", "")),
        ("for each", looping("for each i in [1, 2, 3, 4, 5]", "")),
        (
            "while/while",
            concat!(
                "set turns to 0\n",
                "while turns is not 5\n",
                "    set turns to turns + 1\n",
                "    set inner to 0\n",
                "    while inner is not 5\n",
                "        set inner to inner + 1\n",
                "    end\n",
                "end\n",
                "say turns\n",
            )
            .to_string(),
        ),
    ];

    for (name, source) in &cases {
        // A cap of exactly the turns the loop runs: the loop finishes.
        let (tree, _) = assert_agrees_capped(source, TURNS);
        assert_eq!(
            tree.output,
            vec![TURNS.to_string()],
            "a cap of {TURNS} must be {TURNS} turns of a {name} loop on both VMs, and the \
             loop must finish rather than report the cap"
        );

        // One turn less, and the cap is what stops it — on both VMs, naming itself.
        let (tree, byte) = assert_agrees_capped(source, TURNS - 1);
        let message = tree
            .result
            .as_ref()
            .expect_err("a cap one turn short must stop a loop that needs one more");
        assert!(
            message.contains(&format!("Maximum of {} iterations", TURNS - 1)),
            "and it must be the cap that stops it, said: {message}"
        );
        assert!(
            byte.result.is_err(),
            "and the bytecode VM must stop there too, said: {byte:?}"
        );
    }
}

/// A `break` and a `skip` are charged one turn each, on a `while` as much as on a
/// sequence loop.
///
/// The two reach the cap by different instructions — `break` leaves without
/// starting another turn, `skip` starts the next one without running it — so a
/// charge placed at either one of those alone gets the other wrong. This asks the
/// boundary about both: a cap that exactly covers the turns taken runs, and one
/// turn short is the cap that stops it, on both engines.
#[test]
fn edge_a_break_and_a_skip_in_a_while_cost_exactly_one_turn_each() {
    // The jump is on the last turn, so the loop needs every turn the cap allows
    // and not one more. `break` ends it there; `skip` starts a turn that finds
    // the condition false and ends there instead.
    let jumping = |statement: &str| {
        format!(
            "set turns to 0\nwhile turns is not 5\n    set turns to turns + 1\n\
             if turns is 5 then\n        {statement}\n    end\nend\nsay turns\n"
        )
    };
    let cases = [("break", jumping("break")), ("skip", jumping("skip"))];

    for (name, source) in &cases {
        let (tree, _) = assert_agrees_capped(source, 5);
        assert_eq!(
            tree.output,
            vec!["5".to_string()],
            "a cap of 5 must be 5 turns of a `while` whose {name} is on the last one, \
             on both VMs"
        );

        // One turn short the loop cannot finish, and a `skip` cannot pretend to:
        // the turn it starts is a turn, so the cap stops the loop either way.
        let (tree, byte) = assert_agrees_capped(source, 4);
        assert!(
            tree.result
                .as_ref()
                .is_err_and(|message| message.contains("Maximum of 4 iterations")),
            "a cap of 4 must stop a `while` whose {name} is on its fifth turn, said: {:?}",
            tree.result
        );
        assert!(
            byte.result.is_err(),
            "and the bytecode VM must stop there too, said: {byte:?}"
        );
    }
}

/// A `try` with no `catch` is not a handler, and the loop it is written in keeps
/// turning: the failure leaves the region and the program, rather than the region
/// pretending it succeeded.
#[test]
fn edge_a_catch_less_try_does_not_stop_the_loop_it_is_written_in() {
    let source = concat!(
        "set log to \"\"\n",
        "repeat 3 times\n",
        "    try\n",
        "        set log to log + \"x\"\n",
        "        set bad to 1 + \"one\"\n",
        "    end\n",
        "end\n",
        "say log\n",
    );

    let (tree, _) = assert_agrees(source);
    let message = tree
        .result
        .as_ref()
        .expect_err("nothing handles the failure, so the program stops with it");
    assert!(
        message.contains("non-numbers"),
        "the failure must be reported rather than swallowed, said: {message}"
    );
}

/// A `break` leaves the loop rather than spending the rest of the cap, and it
/// does that identically on both VMs — including on the first turn of a `while`,
/// which is the one turn whose entry does not exist until the exit needs it.
#[test]
fn edge_a_break_stops_the_loop_at_the_same_turn_on_both_vms() {
    let cases = [
        (
            "while",
            "set n to 0\nwhile n is not 100\n    set n to n + 1\n    break\nend\nsay n\n",
        ),
        (
            "repeat",
            "set n to 0\nrepeat 100 times\n    set n to n + 1\n    break\nend\nsay n\n",
        ),
        (
            "for each",
            "set n to 0\nfor each i in [1, 2, 3]\n    set n to n + 1\n    break\nend\nsay n\n",
        ),
    ];

    for cap in [1, 2] {
        for (name, source) in cases {
            let (tree, _) = assert_agrees_capped(source, cap);
            assert_eq!(
                tree.output,
                vec!["1".to_string()],
                "a break on the first turn of a {name} loop is one turn, whatever \
                 the cap is"
            );
        }
    }
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

/// An `object` body written inside another `object`'s body declares two types,
/// and the bytecode VM registers both.
///
/// The bytecode VM held the declaration being assembled in one slot, so the
/// inner `DEF_OBJECT` took the outer one's place and the outer body's finish
/// found nothing left to register — `finish_object` panicked on
/// `pending_object.take().expect(...)`, taking the whole process down on a
/// program the tree-walking VM ran to completion. This is the test that says the
/// panic is gone and both VMs declare the same two types.
#[test]
fn objects_an_object_declared_inside_an_object_body_declares_both_types() {
    // The nested type's field is read from inside the outer body, because the
    // analyzer only declares an `object`'s own name to the scope the declaration
    // is written in.
    let source = concat!(
        "object Outer\n    has o default 1\n    object Inner\n        has i default 2\n    end\n",
        "    say Inner.i\nend\nsay Outer.o\n",
    );
    assert_agrees(source);
    assert_eq!(
        bytecode(source).output,
        vec!["2".to_string(), "1".to_string()],
        "both types are registered, each with its own field"
    );
}

/// The same shape with `extends`: a declaration nested in an `object` body may
/// name the body it is written in as its parent.
///
/// The tree-walking VM registers a type before it runs the statements after its
/// declarations, so the parent is already there. This VM runs the nested
/// declaration first, so the parent is a declaration still being assembled rather
/// than a registered type, and the chain walk has to look for it there.
#[test]
fn objects_a_nested_declaration_may_extend_the_body_it_is_written_in() {
    let source = concat!(
        "object Outer\n    has o default 1\n    object Inner extends Outer\n        has i default 2\n    end\n",
        "    say Inner.o\n    say Inner.i\nend\n",
    );
    assert_agrees(source);
    assert_eq!(
        bytecode(source).output,
        vec!["1".to_string(), "2".to_string()],
        "the nested type inherits the enclosing declaration's field and keeps its own"
    );
}

/// A name an open `object` body has already taken is refused, whichever body
/// wrote it.
///
/// The tree-walking VM has registered the outer type before the nested
/// declaration runs, so `objects` alone is enough for it. This VM has not, so
/// the check has to ask about the declarations being assembled too.
#[test]
fn edge_objects_a_nested_declaration_reusing_the_enclosing_name_is_refused() {
    let cases = [
        (
            "object A\n    has a default 1\n    object A\n        has b default 2\n    end\nend\nsay \"ran\"\n",
            "RuntimeError: Object 'A' is already declared",
        ),
        (
            "object A\n    object B\n        has b default 1\n        object B\n            has c default 2\n        end\n    end\nend\nsay \"ran\"\n",
            "RuntimeError: Object 'B' is already declared",
        ),
        (
            "object Outer\n    object Inner\n        has i default 1\n    end\n    object Inner\n        has j default 2\n    end\nend\nsay \"ran\"\n",
            "RuntimeError: Object 'Inner' is already declared",
        ),
    ];
    for (source, expected) in cases {
        let byte = bytecode(source);
        assert_eq!(
            byte.result
                .as_ref()
                .map(String::as_str)
                .map_err(String::as_str),
            Err(expected),
            "a name an open declaration has taken is refused: {byte:?}"
        );
        assert_eq!(
            tree_walk(source).result,
            byte.result,
            "both VMs refuse a name an open declaration has taken"
        );
    }
}

/// A loop written around an `object` body can be left from inside the body, and
/// the body's declaration is registered on the way out.
///
/// This is the path where an abrupt exit finishes an `object` body rather than
/// running it to its end, so it is where the declaration stack and the frame
/// stack have to agree about which declaration is whose.
#[test]
fn edge_objects_an_object_body_nested_in_a_loop_can_break_out_of_it() {
    let source = "set n to 0\nrepeat 3 times\n    object Outer\n        has o default 1\n        object Inner\n            has i default 2\n        end\n        set n to n + 1\n        break\n    end\nend\nsay n\n";
    assert_agrees(source);
    assert_eq!(
        bytecode(source).output,
        vec!["1".to_string()],
        "the loop is left after its first turn"
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
    let mut code = chunk.main.code.clone();
    // The call follows the whole of `set f to 1`, so the frame has pushed
    // nothing and there is nothing of its own on the stack for the call to take.
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

/// A `catch` runs in a scope of its own and gives it back — and gives back
/// **nothing else**.
///
/// `run_catch` pushed a scope for the body and then popped it again once the
/// frame was driven, but the frame's own `unwind_frame` already truncates
/// `locals` to the base that scope sits at. Two removals took the *enclosing*
/// frame's scope with it, so a `catch` inside a function body left that function
/// without its own bindings and every name it declared afterwards read as
/// unknown. A top-level program never showed it: its names live in globals, which
/// no scope pop reaches.
///
/// Both ends are here: the second program reads a parameter *and* a name declared
/// before the `try`, and the third has the catch body's own binding die with the
/// body, which is the scope rule the fix rests on.
#[test]
fn edge_a_catch_body_gives_back_only_the_scope_it_pushed() {
    for (name, source, expected) in [
        (
            "a parameter read after the catch",
            concat!(
                "to f(x)\n",
                "    try\n",
                "        set bad to 1 + \"one\"\n",
                "    catch error\n",
                "        set caught to yes\n",
                "    end\n",
                "    say x\n",
                "    give back x + 1\n",
                "end\n",
                "say f(7)\n",
            ),
            vec!["7", "8"],
        ),
        (
            "a name declared before the try, read after it",
            concat!(
                "to f(x)\n",
                "    set total to 0\n",
                "    try\n",
                "        set bad to 1 + \"one\"\n",
                "    catch error\n",
                "        set caught to yes\n",
                "    end\n",
                "    set total to total + x\n",
                "    say total\n",
                "    give back total\n",
                "end\n",
                "say f(7)\n",
            ),
            vec!["7", "7"],
        ),
        (
            "the catch's own binding dies with its body",
            concat!(
                "to f()\n",
                "    set error to \"mine\"\n",
                "    try\n",
                "        set bad to 1 + \"one\"\n",
                "    catch error\n",
                "        set caught to error\n",
                "    end\n",
                "    say caught\n",
                "    say error\n",
                "end\n",
                "f()\n",
            ),
            vec!["error", "mine"],
        ),
    ] {
        assert_agrees(source);
        let outcome = bytecode(source);
        assert_eq!(
            outcome.output,
            expected
                .iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>(),
            "the catch body took the enclosing function's scope with it on the {name} path"
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

/// An `import X as Y` alias of a builtin namespace reaches the same function the
/// unaliased name does.
///
/// A builtin namespace has no file behind it, so the module loader looked for
/// one, did not find it, and refused the import: `rb run` printed the value and
/// `rb vm` said `Cannot find module 'json'`. The alias was the second half of the
/// same gap — `IMPORT` had nowhere to record which name the import bound.
#[test]
fn edge_import_alias_of_a_builtin_namespace_reaches_the_same_function() {
    let source = "import json as J\nsay J.stringify(2)\n".to_string();
    assert_agrees(&source);
    assert_eq!(
        bytecode(&source).output,
        vec!["2".to_string()],
        "an aliased namespace must call through, not be refused for want of a file"
    );
}

/// An alias and the module's own name are the same name for one module, so the
/// unaliased spelling keeps working once an import aliases it.
///
/// Pinning both halves matters: resolving the alias alone would leave
/// `json.stringify` broken, which is the spelling every existing module example
/// uses.
#[test]
fn edge_an_import_alias_leaves_the_modules_own_name_working() {
    let aliased = "import json as J\nsay json.stringify(2)\n".to_string();
    assert_agrees(&aliased);
    assert_eq!(
        bytecode(&aliased).output,
        vec!["2".to_string()],
        "the module's own name must still name it after an aliased import"
    );
}

/// An `import X as Y` alias of a module *file* reaches that module's functions.
///
/// The module loader compiled a module file down to its `set` and `constant`
/// statements, so a module's `to` functions were never bound under the
/// `module_member` name a call through the alias resolves to: the tree-walking
/// VM printed the area and the bytecode VM said `Unknown function`.
#[test]
fn edge_import_alias_of_a_module_file_reaches_its_functions() {
    let source = "import MathUtils as M\nsay M.circle_area(2)\n".to_string();
    assert_agrees(&source);
    assert_eq!(
        bytecode(&source).output,
        vec!["12.56636".to_string()],
        "a module file's functions must be callable through the import alias"
    );
}

/// A member past the end of a module is refused the same way on both VMs.
///
/// The refusal is the tree-walking VM's `Module 'M' has no function 'nope'`; a
/// bytecode VM that has no such check says `Unknown function 'M_nope'`, which is
/// the same mistake spelled differently — and a program that imports a module
/// whose contents change must not change from one error to another.
#[test]
fn edge_a_missing_module_member_is_refused_the_same_way_on_both_vms() {
    let source = "import MathUtils as M\nsay M.not_a_function(1)\n".to_string();
    assert_agrees(&source);
    assert_eq!(
        bytecode(&source).result,
        Err("RuntimeError: Module 'MathUtils' has no function 'not_a_function'".to_string()),
        "a member that does not exist must be named as one that does not exist"
    );
}

/// A module that reaches itself through an import is a clean error, not a hang.
///
/// The loader recorded a module as loaded *before* running it, so a self-import
/// found it loaded and did nothing: the cycle passed silently on the bytecode VM
/// while the tree-walking one refused it. The refusal has to be catchable, so a
/// caught error is the contract — the failure is reachable from the program
/// rather than fatal.
///
/// Run as subprocesses rather than through [`assert_agrees`] because the loader
/// looks for `<name>.rb` relative to the working directory, and the module that
/// imports itself has to sit where the run can find it. Running both ways from
/// that same directory is the comparison.
#[test]
fn edge_a_circular_import_is_a_caught_error_not_a_hang() {
    let dir = scratch_dir("circular");
    fs::write(
        dir.join("CircleMod.rb"),
        "import CircleMod\nconstant X to 1\n",
    )
    .expect("module file should be writable");
    let program =
        "try\n    import CircleMod\n    say \"body\"\ncatch error\n    say \"caught\"\nend\n";
    fs::write(dir.join("main.rb"), program).expect("program should be writable");

    let walked = rb_in(&dir, &["run", "main.rb"]);
    assert_eq!(
        String::from_utf8_lossy(&walked.stdout)
            .lines()
            .collect::<Vec<_>>(),
        vec!["caught"],
        "a cycle must be caught by the program, not run past: {walked:?}"
    );

    let compiled = rb_in(&dir, &["compile", "main.rb"]);
    assert!(
        compiled.status.success(),
        "the program with a circular import must compile: {compiled:?}"
    );
    let bytecoded = rb_in(&dir, &["vm", "main.rbc"]);
    assert_eq!(
        String::from_utf8_lossy(&bytecoded.stdout)
            .lines()
            .collect::<Vec<_>>(),
        vec!["caught"],
        "the bytecode VM must refuse the cycle too, not pass it silently: {bytecoded:?}"
    );
}

/// Runs `rb` with `dir` as its working directory — which for a module loader is
/// where a module file has to be.
fn rb_in(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rb"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("rb should be runnable")
}

/// A `module ... end` declaration publishes what it exports, and a call through
/// the module reaches it.
///
/// A module declaration compiled its body as ordinary top-level statements and
/// the declaration itself to nothing, so this VM had no record of the module: a
/// call through it said `Unknown function 'Geometry_area'` where the
/// tree-walking one called the function. The same program also had to stop
/// disagreeing for the *scope* — a `set` inside the module is the module's name
/// and not a name of the program that declared it.
#[test]
fn a_module_declaration_publishes_what_it_exports_on_both_vms() {
    let source = "module Geometry\n    to area(r)\n        return 3.14159 * r * r\n    end\n    set scale to 2\n    export all\nend\nsay Geometry.area(2)\nsay Geometry.scale\n".to_string();
    assert_agrees(&source);
    assert_eq!(
        bytecode(&source).output,
        vec!["12.56636".to_string()],
        "the module's function must be published under the module's name"
    );
}

/// An `export` naming a function the module does not define is refused, by name,
/// and before the body runs.
///
/// The refusal is the tree-walking VM's wording, because a program that names a
/// member it does not have must not change from one error to another with the VM
/// that runs it.
#[test]
fn edge_an_export_of_a_name_the_module_does_not_define_is_refused_the_same_way() {
    let source = "module M\n    to f\n        return 1\n    end\n    export not_a_function\nend\nsay \"unreached\"\n".to_string();
    assert_agrees(&source);
    assert_eq!(
        bytecode(&source).result,
        Err(
            "RuntimeError: Module 'M' exports 'not_a_function', which it does not define"
                .to_string()
        ),
        "the refusal names the module and the member, and nothing runs first"
    );
}

/// A module declared twice in one program is refused the second time.
#[test]
fn edge_a_module_declared_twice_is_refused_the_same_way_on_both_vms() {
    let source = "module M\n    to f\n        return 1\n    end\n    export f\nend\nmodule M\n    to f\n        return 2\n    end\n    export f\nend\n".to_string();
    assert_agrees(&source);
    assert_eq!(
        bytecode(&source).result,
        Err("RuntimeError: Module 'M' is already declared".to_string()),
        "a second declaration of the same name is a fault, not a redefinition"
    );
}

/// The module's own name is a name of the program: an unbound one is bound to
/// `nothing` by the import, so a read of it is a read rather than an unknown
/// variable. The bytecode VM used to leave it unbound and said
/// `Unknown variable 'M'`.
#[test]
fn edge_an_import_of_a_declared_module_binds_both_names() {
    let source = "module Counter\n    to bump(n)\n        return n + 1\n    end\n    export bump\nend\nimport Counter as C\nsay C.bump(41)\nsay Counter\n".to_string();
    assert_agrees(&source);
    assert_eq!(
        bytecode(&source).output,
        vec!["42".to_string(), "nothing".to_string()],
        "the alias calls through and the module's own name reads as nothing"
    );
}

/// A `set` inside a module body is the module's own binding and does not become
/// a global of the program that declared it.
#[test]
fn edge_a_modules_own_scope_does_not_leak_into_the_program() {
    let source = "module Inner\n    set hidden to 42\n    to f\n        return hidden\n    end\n    export all\nend\nsay Inner.f()\ntry\n    say hidden\ncatch error\n    say \"caught\"\nend\n".to_string();
    assert_agrees(&source);
    assert_eq!(
        bytecode(&source).output,
        vec!["42".to_string(), "caught".to_string()],
        "the module's `set` is the module's name: the function reads it and the program does not"
    );
}

/// A `set` inside a module body writes the module's own scope even when the
/// program has a *constant* of that name, so the write is not the constant's to
/// refuse. The refusal is raised where the write would land on a global, which
/// is the rule the tree-walking VM's `set_var` writes.
#[test]
fn edge_a_set_inside_a_module_does_not_rebind_a_constant_of_the_program() {
    let source = "constant OUTER to 1\nmodule Shadow\n    set OUTER to 2\n    to f\n        return OUTER\n    end\n    export f\nend\nsay Shadow.f()\nsay OUTER\n".to_string();
    assert_agrees(&source);
    assert_eq!(
        bytecode(&source).output,
        vec!["2".to_string(), "1".to_string()],
        "the module's write is its own, and the program's constant is untouched"
    );
}

/// A module with no `export` at all publishes nothing, and its body still runs.
#[test]
fn edge_a_module_with_nothing_exported_binds_nothing() {
    let source = "module Quiet\n    to f\n        return 1\n    end\nend\ntry\n    say Quiet.f()\ncatch error\n    say \"caught\"\nend\n".to_string();
    assert_agrees(&source);
    assert_eq!(
        bytecode(&source).output,
        vec!["caught".to_string()],
        "a module that publishes nothing answers nothing"
    );
}

/// A module that reaches itself through an `import` inside its own body is a
/// circular import, refused rather than answered with an empty namespace.
#[test]
fn edge_a_module_that_imports_itself_is_a_caught_error() {
    let source = "try\n    module Loop\n        import Loop\n        to f\n            return 1\n        end\n        export f\n    end\ncatch error\n    say \"caught\"\nend\n".to_string();
    assert_agrees(&source);
    assert_eq!(
        bytecode(&source).output,
        vec!["caught".to_string()],
        "the declaration itself is the cycle, so it is refused before its body runs"
    );
}

/// A version-3 `.rbc` is refused, so an `IMPORT` in it is never read as an alias
/// it does not carry.
///
/// Version 3 wrote a filler `0` in the second operand of every `IMPORT`. Read as
/// version 4, that `0` is `constants[0]` — whichever name happens to be first in
/// the pool — so the import would bind the wrong name instead of the one the
/// source wrote.
#[test]
fn edge_a_version_3_file_is_refused_before_its_imports_name_an_alias() {
    let dir = scratch_dir("version-3");
    let source = "import json as J\nsay J.stringify(2)\n";
    fs::write(dir.join("main.rb"), source).expect("program should be writable");

    let compiled = rb_in(&dir, &["compile", "main.rb"]);
    assert!(
        compiled.status.success(),
        "the program should compile: {compiled:?}"
    );
    let mut bytes = fs::read(dir.join("main.rbc")).expect("the compiled file should be readable");
    assert_eq!(
        u16::from_le_bytes([bytes[4], bytes[5]]),
        redblue::bytecode::FORMAT_VERSION,
        "the file states the version this build writes"
    );
    // A version-3 file's `IMPORT` had no alias, so the second operand is the
    // filler. Written here rather than carried in the repo, because the bytes of
    // an older format are not something this build can produce.
    bytes[4..6].copy_from_slice(&3u16.to_le_bytes());
    fs::write(dir.join("old.rbc"), &bytes).expect("the rewritten file should be writable");

    let rejected = rb_in(&dir, &["vm", "old.rbc"]);
    assert!(
        !rejected.status.success(),
        "a version-3 file must be refused, not read with a guessed alias"
    );
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert!(
        stderr.contains("version 3"),
        "the refusal names the file's version, got: {stderr}"
    );

    // And the same program, compiled and run by this build, still works — the
    // refusal is about the file's version, not about imports.
    let run = rb_in(&dir, &["vm", "main.rbc"]);
    assert!(
        run.status.success(),
        "the version this build writes must run: {run:?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout)
            .lines()
            .collect::<Vec<_>>(),
        vec!["2"],
        "an aliased import still reaches the function"
    );
}

/// A sequence loop must put the operand stack back exactly where it found it
/// when the sequence runs out.
///
/// `GET_ITER` pops the sequence into the loop's own entry, so the loop body runs
/// at whatever height the operand stack was left at, and `push_loop` records that
/// height as the one to return to. If it is recorded one too low, leaving the loop
/// truncates a value the *enclosing* frame had already pushed — here the `push`
/// call's own first argument, which the callee then never receives.
///
/// The shape is a sequence loop inside a function called as one argument of a
/// call, so there is a live operand-stack value underneath the loop for the bad
/// truncation to eat.
#[test]
fn edge_leaving_a_sequence_loop_does_not_eat_the_callers_operands() {
    let source = r#"
to total(list)
    set out to 0
    for each x in list
        set out to out + x
    end
    give back out
end
say push(["kept"], total([1, 2, 3]))
"#;

    let (tree, byte) = assert_agrees(source);
    assert_eq!(
        tree.output,
        vec!["[kept, 6]".to_string()],
        "the loop's own answer is the sum of the list"
    );
    assert_eq!(
        byte.output, tree.output,
        "the bytecode VM must keep the caller's first operand across the loop"
    );
}

/// The same truncation, reached through a loop *nested* in another loop, which is
/// the shape `bootstrap/compiler.rb`'s `apply_patches` has: a `while` whose body
/// is a `for each`, both in one function, with a call after them.
///
/// The inner loop leaves by running out of values, and the outer `while` leaves
/// through its own condition, so both recorded heights have to be right for the
/// `push` after them to still receive two operands.
#[test]
fn edge_a_sequence_loop_nested_in_a_while_returns_the_outer_stack() {
    let source = r#"
to first_of_each(code)
    set out to []
    set i to 0
    while i < length(code)
        for each entry in code
            set out to push(out, entry)
        end
        set i to i + 1
    end
    give back out
end
say push(["kept"], first_of_each([[1], [2]]))
"#;

    let (tree, byte) = assert_agrees(source);
    // The `while` has two turns, and each turn walks the whole of `code`, so
    // every element is appended once per turn.
    assert_eq!(
        tree.output,
        vec!["[kept, [[1], [2], [1], [2]]]".to_string()],
        "both elements are visited once on each of the two turns"
    );
    assert_eq!(byte.output, tree.output);
}

/// A loop that is never entered because its sequence is empty still has to leave
/// the caller's operands alone.
///
/// The empty sequence takes the `leave_loop` path on the *first* turn, before
/// the body has run once, which is the boundary between the two ways a sequence
/// loop ends.
#[test]
fn edge_an_empty_sequence_loop_leaves_the_callers_operands_alone() {
    let source = r#"
to nothing_at_all(list)
    set out to 0
    for each x in list
        set out to out + x
    end
    give back out
end
say push(["kept"], nothing_at_all([]))
"#;

    let (tree, byte) = assert_agrees(source);
    assert_eq!(
        tree.output,
        vec!["[kept, 0]".to_string()],
        "an empty sequence sums to zero and the caller's operand survives"
    );
    assert_eq!(byte.output, tree.output);
}
