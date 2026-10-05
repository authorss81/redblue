//! Bytecode execution (bootstrap stage S1b): `rb vm file.rbc`.
//!
//! The tree-walking VM is the specification of what a Redblue program *does*.
//! These tests pin that the bytecode VM does the same thing — the same output,
//! the same error kind and the same error message — and that it holds the same
//! resource limits while doing it.
//!
//! The corpus test is the phase's definition of done: a differential run over a
//! corpus of more than two hundred programs, each compiled and executed by
//! `rb vm` and compared against `rb run`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use redblue::bytecode::compile_source;
use redblue::bytecode::vm::{BytecodeVm, DEFAULT_MAX_CALL_DEPTH, DEFAULT_MAX_STEPS};

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir = project_root().join("target/tmp/bytecode-vm-test").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir should be creatable");
    dir
}

/// Runs `rb` with `args` from the project root, so a program that reads a module
/// finds `modules/` whichever VM runs it.
fn rb(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rb"))
        .args(args)
        .current_dir(project_root())
        .output()
        .expect("rb should be runnable")
}

/// What a run produced: its exit status, its output, and the first line of its
/// error.
///
/// The first line of the error is compared rather than the whole of it because
/// the tree-walking VM renders the offending source line and a caret under it,
/// and a `.rbc` has no source line to render. Everything up to the `-->` is the
/// message the two VMs have to agree on.
struct Run {
    succeeded: bool,
    stdout: String,
    error: String,
}

impl Run {
    fn of(output: &Output) -> Run {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let error = stderr
            .lines()
            .next()
            .unwrap_or_default()
            .to_string();
        Run {
            succeeded: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            error,
        }
    }

    /// A message that names neither VM: used where both must fail but the two
    /// are expected to word it differently.
    fn failed(&self) -> bool {
        !self.succeeded
    }
}

/// Writes `source` into a fresh directory, compiles it, and returns the path of
/// the `.rbc` beside it.
fn compile_to(name: &str, source: &str) -> PathBuf {
    let dir = scratch_dir(name);
    let source_path = dir.join("program.rb");
    let rbc_path = dir.join("program.rbc");
    fs::write(&source_path, source).expect("source should be writable");

    let compile = rb(&[
        "compile",
        source_path.to_str().expect("utf-8 path"),
        "-o",
        rbc_path.to_str().expect("utf-8 path"),
    ]);
    assert!(
        compile.status.success(),
        "rb compile should succeed for:\n{source}\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    assert!(rbc_path.exists(), "rb compile wrote no .rbc");
    rbc_path
}

fn utf8(path: &Path) -> &str {
    path.to_str().expect("path should be UTF-8")
}

/// Runs `source` both ways and returns `(tree-walk, bytecode)`.
fn both_ways(name: &str, source: &str) -> (Run, Run) {
    let dir = scratch_dir(name);
    let source_path = dir.join("program.rb");
    fs::write(&source_path, source).expect("source should be writable");
    let tree = Run::of(&rb(&["run", utf8(&source_path)]));

    let rbc_path = dir.join("program.rbc");
    let compile = rb(&[
        "compile",
        utf8(&source_path),
        "-o",
        utf8(&rbc_path),
    ]);
    assert!(
        compile.status.success(),
        "rb compile should succeed:\n{source}\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let bytecode = Run::of(&rb(&["vm", utf8(&rbc_path)]));
    (tree, bytecode)
}

/// Runs `source` through the library rather than the binary, for the tests that
/// want a VM with lowered limits.
fn run_in_library(source: &str, vm: &mut BytecodeVm) -> Result<(), redblue::Error> {
    let chunk = compile_source(source).expect("program should compile");
    vm.run(&chunk).map(|_| ())
}

// ---------------------------------------------------------------------------
// The phase's definition of done: a differential corpus
// ---------------------------------------------------------------------------

/// Preambles every corpus program may start with. Each one puts something
/// different in scope, so the bodies below are exercised with a variable, a
/// list, a record, a closure and an object all live at once.
const PREAMBLES: &[&str] = &[
    "",
    "set x to 5\n",
    "set x to 5\nset y to 3\n",
    "set x to -2\nset y to 0\n",
    "set x to 2.5\nset y to 1.5\n",
    "set x to 0\nset y to 0\n",
    "set s to \"hello\"\n",
    "set s to \"\"\n",
    "set xs to [3, 1, 2]\n",
    "set xs to []\n",
    "set xs to [1]\n",
    "set r to { name: \"a\", size: 2 }\n",
    "set r to {}\n",
    "to twice(n)\n    return n * 2\nend\n",
    "to twice(n)\n    return n * 2\nend\nset x to 4\n",
    "to make(n)\n    return n + 1\nend\nset y to make(2)\n",
    "set outer to 9\nto addTo(v)\n    return outer + v\nend\n",
    "object Thing\n    has size\n    has label\nend\n",
    "object Base\n    has size\n    to can describe()\n        return size\n    end\nend\nobject Child extends Base\n    has label\nend\n",
    "set x to 1\nset xs to [1, 2]\nset r to { a: 1 }\n",
    "set yes to yes\nset no to no\nset nothing to nothing\n",
    "set flag to no\n",
    "set x to 9007199254740993\n",
    "set x to 1e308\n",
    "set s to \"\\u{1F600} \\u{4E2D}\\u{6587} \\u{5D0}\"\n",
    "set pair to [\"a\", \"b\"]\n",
    "set deep to [[[1, 2], [3]], [[4]]]\n",
    "set x to 5\nset xs to [5, 6]\nset r to { x: 5 }\n",
    "set counter to 0\n",
];

/// Bodies appended to every preamble. Each is a statement or two a program can
/// reasonably contain, and between them they reach every opcode the compiler
/// emits for an ordinary program.
const BODIES: &[&str] = &[
    "say x\n",
    "say s\n",
    "say xs\n",
    "say r\n",
    "say r.name\n",
    "say r.missing\n",
    "say nothing\n",
    "say yes\n",
    "say type_of(x)\n",
    "say type_of(s)\n",
    "say type_of(xs)\n",
    "say type_of(r)\n",
    "say length(xs)\n",
    "say length(s)\n",
    "say 1 + 2\nsay 7 - 3\nsay 4 * 5\nsay 9 / 2\nsay 9 % 4\n",
    "say 1 / 0\n",
    "say 1 % 0\n",
    "say 1 + \"a\"\n",
    "say \"a\" + \"b\"\n",
    "say -x\n",
    "say not flag\n",
    "say flag and yes\nsay flag or yes\n",
    "say x is 5\nsay x is not 5\n",
    "say x > 1\nsay x < 1\nsay x is at least 1\nsay x is at most 1\n",
    "say 3 in xs\nsay 99 in xs\n",
    "if x is 5 then\n    say \"five\"\nelse\n    say \"other\"\nend\n",
    "if s is \"\" then\n    say \"empty\"\nend\n",
    "repeat 3 times\n    say \"tick\"\nend\n",
    "repeat 0 times\n    say \"never\"\nend\n",
    "repeat 2 times\n    set x to x + 1\nend\nsay x\n",
    "set total to 0\nfor each item in xs\n    set total to total + item\nend\nsay total\n",
    "for each item in s\n    say item\nend\n",
    "for each item in x\n    say item\nend\n",
    "set total to 0\nfor each n from 1 to 4\n    set total to total + n\nend\nsay total\n",
    "set total to 0\nfor each n from 4 to 1 by -1\n    set total to total + n\nend\nsay total\n",
    "set total to 0\nfor each n from \"a\" to \"b\"\n    set total to total + 1\nend\nsay total\n",
    "set i to 0\nwhile i is less than 3\n    set i to i + 1\nend\nsay i\n",
    "say xs[0]\n",
    "say xs[1]\n",
    "say xs[-1]\n",
    "say xs[9]\n",
    "say xs[0.5]\n",
    "say xs[\"a\"]\n",
    "say x[0]\n",
    "say twice(4)\n",
    "say twice(\"a\")\n",
    "say twice(4, 5, 6)\n",
    "say make(1)\n",
    "say nothing()\n",
    "say addTo(1)\n",
    "say s.length()\n",
    "say xs.length()\n",
    "say r.name()\n",
    "say xs.append(4)\n",
    "set r.name to \"b\"\nsay r.name\n",
    "set x.name to 5\nsay x\n",
    "set s.name to 5\nsay s\n",
    "say Thing.describe()\n",
    "say Child.describe()\n",
    "say Thing.label\n",
    "say Thing.nothing()\n",
    "say 5.describe()\n",
    "say json.parse(\"[1, 2]\")\n",
    "say json.stringify({ a: 1 })\n",
    "say csv.parse(\"a,b\\n1,2\")\n",
    "say time.unix(\"2024-01-15 12:30:00\")\n",
    "try\n    say xs[9]\ncatch error\n    say \"caught\"\nend\n",
    "try\n    say xs[9]\nend\nsay \"after\"\n",
    "try\n    say \"body\"\nfinally\n    say \"finally\"\nend\n",
    "try\n    say xs[9]\ncatch problem\n    say problem\nend\n",
    "test \"corpus test\"\n    set t to 1 + 1\n    expect t to be 2\nend\n",
    "test \"corpus failing test\"\n    expect 1 to be 2\nend\n",
    "expect 1 + 1 to be 2\n",
    "expect 1 + 1 to be 3\n",
    "print \"printed\"\nsay \"said\"\n",
    "say \"\"\nsay \"two\"\n",
    "say 1000000 * 1000000\n",
    "say 0.1 + 0.2\n",
    "say -0.0\n",
    "say 9007199254740993\n",
    "say 1 / 3\n",
];

/// Every corpus program, in a fixed order.
///
/// `set x to 5` on its own is not a program, so a preamble that binds nothing
/// and a body that reads an unbound name is still a *test*: it is a program
/// that fails, and the two VMs have to fail the same way.
fn corpus() -> Vec<(String, String)> {
    let mut programs = Vec::new();
    for (preamble_index, preamble) in PREAMBLES.iter().enumerate() {
        for (body_index, body) in BODIES.iter().enumerate() {
            programs.push((
                format!("p{preamble_index:02}b{body_index:02}"),
                format!("{preamble}{body}"),
            ));
        }
    }
    programs
}

/// Every corpus program produces the same output, the same exit status and the
/// same first line of error on both VMs.
#[test]
fn a_corpus_of_more_than_two_hundred_programs_runs_identically_on_both_vms() {
    let programs = corpus();
    assert!(
        programs.len() >= 200,
        "the differential corpus must be at least 200 programs, got {}",
        programs.len()
    );

    let dir = scratch_dir("differential-corpus");
    let mut agreeing = 0usize;
    let mut mutating = Vec::new();
    for (name, source) in &programs {
        let source_path = dir.join(format!("{name}.rb"));
        let rbc_path = dir.join(format!("{name}.rbc"));
        fs::write(&source_path, source).expect("source should be writable");

        let tree = Run::of(&rb(&["run", utf8(&source_path)]));
        let compile = rb(&["compile", utf8(&source_path), "-o", utf8(&rbc_path)]);
        assert!(
            compile.status.success(),
            "rb compile should succeed for {name}:\n{source}\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let bytecode = Run::of(&rb(&["vm", utf8(&rbc_path)]));

        if tree.succeeded != bytecode.succeeded
            || tree.stdout != bytecode.stdout
            || tree.error != bytecode.error
        {
            mutating.push(format!(
                "{name}\n--- source ---\n{source}--- tree-walk ---\n{:#?}{}\n\
                 --- bytecode ---\n{:#?}{}",
                tree, tree.error, bytecode, bytecode.error
            ));
        } else {
            agreeing += 1;
        }
    }

    assert!(
        mutating.is_empty(),
        "{} of {} programs behaved differently on the two VMs:\n\n{}",
        mutating.len(),
        programs.len(),
        mutating.join("\n\n")
    );
    assert_eq!(agreeing, programs.len(), "every program should agree");
}
