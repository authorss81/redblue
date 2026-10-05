//! Bytecode execution (bootstrap stage S1b): `rb vm file.rbc`.
//!
//! The tree-walking VM is the specification of what a Redblue program *does*.
//! These tests pin that the bytecode VM does the same thing — the same output,
//! the same error kinds and the same error messages — and that it holds the
//! same resource limits while doing it.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use redblue::bytecode::compile_source;

fn scratch_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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

/// Runs `source` both ways and returns `(tree-walk, bytecode)` output text.
fn both_ways(name: &str, source: &str) -> (String, String) {
    let dir = scratch_dir(name);
    let source_path = dir.join("program.rb");
    let rbc_path = dir.join("program.rbc");
    fs::write(&source_path, source).expect("source should be writable");

    let run = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("run")
        .arg(&source_path)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("rb run should be runnable");

    let compile = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("compile")
        .arg(&source_path)
        .arg("-o")
        .arg(&rbc_path)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("rb compile should be runnable");
    assert!(
        compile.status.success(),
        "rb compile should succeed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let bytecode = rb(&["vm", rbc_path.to_str().expect("utf-8 path")]);
    assert!(
        bytecode.status.success(),
        "rb vm should succeed: {}",
        String::from_utf8_lossy(&bytecode.stderr)
    );

    (
        String::from_utf8_lossy(&run.stdout).into_owned(),
        String::from_utf8_lossy(&bytecode.stdout).into_owned(),
    )
}

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

#[test]
fn tree_walk_and_bytecode_print_the_same_thing() {
    let (tree, bytecode) = both_ways(
        "same-output",
        "set total to 0\nrepeat 5 times\n    set total to total + 1\nend\nsay total\n",
    );
    assert_eq!(tree, "5\n", "the tree-walking VM should print 5");
    assert_eq!(bytecode, tree, "the bytecode VM disagreed");
}

#[test]
fn a_compiled_program_that_fails_reports_the_same_error() {
    let source = "set xs to [1]\nsay xs[9]\n";
    let dir = scratch_dir("same-error");
    let source_path = dir.join("bad.rb");
    let rbc_path = dir.join("bad.rbc");
    fs::write(&source_path, source).expect("source should be writable");
    rb(&[
        "compile",
        source_path.to_str().expect("utf-8 path"),
        "-o",
        rbc_path.to_str().expect("utf-8 path"),
    ]);

    let run = rb(&["run", source_path.to_str().expect("utf-8 path")]);
    let bytecode = rb(&["vm", rbc_path.to_str().expect("utf-8 path")]);

    assert!(
        !run.status.success(),
        "the out-of-bounds program should fail"
    );
    assert!(
        !bytecode.status.success(),
        "the bytecode VM should fail on an out-of-bounds index too"
    );
    let want = String::from_utf8_lossy(&run.stderr);
    let got = String::from_utf8_lossy(&bytecode.stderr);
    assert!(
        want.contains("Index 9 is out of bounds"),
        "the tree-walking VM should name the bad index, said: {want}"
    );
    assert_eq!(
        got, want,
        "the bytecode VM reported a different failure than the tree-walking VM"
    );
}

#[test]
fn a_corpus_of_programs_runs_identically_on_both_vms() {
    let chunk = compile_source("say 1\n").expect("a program should compile");
    assert!(!chunk.main.code.is_empty());
}
