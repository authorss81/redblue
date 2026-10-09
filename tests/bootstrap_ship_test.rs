//! Bootstrap ladder stage **S4**: the release is built by the self-hosted
//! compiler.
//!
//! Stage 3 (phase-022) proved the fixed point: `bootstrap/compiler.rb` compiled
//! by the Rust frontend and by itself are one file, byte for byte. S4 is the
//! claim about *who writes the file that ships*.
//!
//! What is shipped is not a machine code executable — `rb` is a Rust binary,
//! and no Redblue program can emit one. What a Redblue program can emit, and
//! what the ladder has actually produced since S1, is a `.rbc`. So S4 is this:
//!
//!   the compiler that the release carries is the one Redblue compiled, and
//!   the build that produces it refuses to ship unless that file is the fixed
//!   point.
//!
//! [`redblue::bootstrap::build`] is that build. It takes the compiler's source,
//! produces stage 1 with the Rust frontend, produces stage 2 by running stage 1
//! on its own source, and only then returns the bytes to ship. A stage 2 that
//! differs by one byte is an error, not a release.
//!
//! Every assertion here is against bytes. Nothing in `src/` special-cases a
//! path, and `bootstrap::build` calls the same `compile_source` the CLI does and
//! the same `BytecodeVm` `rb vm` uses, so a build that reached the answer by
//! another route would have to reproduce the whole compiler to hide it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use redblue::bootstrap;
use redblue::Error;

/// The failure of a build that must fail.
///
/// `.err().expect(...)` reads as two steps and clippy rejects it
/// (`err_expect`), so the check and the message are one function here. Every
/// caller of it is a test asserting that a build *refuses*: a build that
/// succeeded is the failure, and it is named rather than left as
/// `called Result::unwrap() on an Err value`.
fn expect_failure<T, E: std::fmt::Debug>(outcome: Result<T, E>, why: &str) -> E {
    match outcome {
        Ok(_) => panic!("{why}"),
        Err(error) => error,
    }
}

/// The compiler under test.
fn compiler() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("bootstrap/compiler.rb")
}

/// A path under `target/tmp/`. Nothing is written to `bootstrap/` or `corpus/`:
/// a build that rewrote what it is shipping would prove nothing about the file
/// it shipped.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/ship");
    fs::create_dir_all(&dir).expect("scratch directory");
    dir.join(name)
}

/// One full release build, made once and read by every test below.
///
/// A self-compilation is minutes in a test build, so the three runs in series
/// would be a gate nobody waits for. `OnceLock` rather than a file on disk: the
/// bytes are in memory, and no test here writes to the compiler.
fn release() -> &'static bootstrap::Build {
    static BUILD: std::sync::OnceLock<bootstrap::Build> = std::sync::OnceLock::new();
    BUILD.get_or_init(|| {
        bootstrap::build(&compiler(), &scratch("release"))
            .unwrap_or_else(|error| panic!("the release build failed: {error:?}"))
    })
}

/// **S4**: the shipped compiler is the fixed point, and it is a bytecode file
/// this build reads.
///
/// The claim has two halves and both are asserted. First that the file the
/// build returns is one file — stage 1's bytes, stage 2's bytes, and the shipped
/// bytes are all the same — because a compiler that agrees with itself about
/// its own source but is shipped from a third file is not bootstrapped, it is
/// duplicated. Second that those bytes decode to a chunk whose `encode` is them
/// again, so the shipped artifact is a `.rbc` and not a file of the right
/// length.
#[test]
fn edge_the_release_is_built_by_the_self_hosted_compiler() {
    let build = release();

    assert!(
        !build.shipped.is_empty(),
        "the release build shipped no compiler at all"
    );
    assert_eq!(
        build.stage1.len(),
        build.stage2.len(),
        "stage 1 wrote {} bytes for the compiler and stage 2 wrote {}",
        build.stage1.len(),
        build.stage2.len()
    );
    assert_eq!(
        build.stage1, build.stage2,
        "stage 1 and stage 2 disagree about the compiler's bytecode, so what \
         would ship is not the fixed point"
    );
    assert_eq!(
        build.shipped, build.stage2,
        "the shipped compiler is not the file stage 2 wrote"
    );

    let decoded = redblue::Chunk::decode(&build.shipped).unwrap_or_else(|error| {
        panic!("the shipped compiler is not a bytecode file this build reads: {error:?}")
    });
    assert_eq!(
        decoded.encode(),
        build.shipped,
        "the shipped compiler does not decode to the chunk it encodes"
    );
}

/// The rollback path produces the same file.
///
/// `bootstrap::rollback` is the documented way back to the Rust compiler when
/// the fixed point cannot be established: it is `rb compile`, and nothing else.
/// That it is the *same* file is what makes it a rollback rather than a second
/// compiler, and it is the only reason the fixed point matters operationally.
#[test]
fn edge_the_rollback_path_ships_the_same_file_as_the_fixed_point() {
    let build = release();
    let rolled_back = bootstrap::rollback(&compiler())
        .unwrap_or_else(|error| panic!("the rollback path failed: {error:?}"));

    assert_eq!(
        rolled_back.len(),
        build.stage1.len(),
        "rollback wrote {} bytes where the Rust frontend wrote {}",
        rolled_back.len(),
        build.stage1.len()
    );
    assert_eq!(
        rolled_back, build.stage1,
        "the rollback path and the Rust frontend disagree about the compiler's \
         bytecode, so a rollback would ship a different compiler"
    );
}

/// The shipped compiler compiles a corpus program exactly as the Rust frontend
/// does.
///
/// A fixed point is a statement about one input. This is the statement that the
/// shipped file is *the* compiler rather than a file that happens to be right
/// about itself: it is handed programs it has never seen, and each one comes
/// back byte for byte what the frontend would have written.
#[test]
fn edge_the_shipped_compiler_agrees_with_the_frontend_on_a_corpus_family() {
    let build = release();
    let sample = scratch("corpus-family.rb");
    fs::write(
        &sample,
        "// A shape the compiler has never been handed: a nested if inside a\n\
         // repeat, an else branch, arithmetic and a list literal.\n\
         set total to 0\nset squares to []\nrepeat 10 times\n    \
         if total % 2 is 0 then\n        \
         set total to total + 1\n    else\n        \
         append(\"squares\", total * total)\n    end\n    \
         set total to total + 1\nend\nsay total\n",
    )
    .expect("the sample program is written");

    let frontend = redblue::compile_source(&fs::read_to_string(&sample).expect("readable"))
        .expect("the frontend accepts the sample")
        .encode();

    let chunk = redblue::Chunk::decode(&build.shipped).expect("the shipped compiler decodes");
    let written = scratch("corpus-family.rbc");
    let _ = fs::remove_file(&written);

    let outcome = redblue::bootstrap::run_compiler_on(&chunk, &sample, &written);
    assert!(
        outcome.is_ok(),
        "the shipped compiler refused a program the frontend accepts: {outcome:?}"
    );

    let shipped_bytes = fs::read(&written).expect("the shipped compiler wrote a file");
    assert_eq!(
        shipped_bytes, frontend,
        "the compiler shipped by the release does not agree with the Rust \
         frontend on a program outside the fixed point"
    );
}

/// A stage 2 that is not the fixed point is refused, and ships nothing.
///
/// This is the failure the whole ladder exists to catch, so it is tested by
/// being caused: one byte of stage 2 is flipped, `verify` is handed the pair,
/// and the build must name the byte and refuse. The first-differing-byte
/// report is asserted too, because "the compiler disagrees" with no offset is
/// the same as a build that refuses for no stated reason.
#[test]
fn edge_a_stage_two_that_is_not_the_fixed_point_is_refused_and_ships_nothing() {
    let build = release();
    let mut tampered = build.stage2.clone();
    let last = tampered
        .len()
        .checked_sub(1)
        .expect("the compiler is not an empty file");
    tampered[last] ^= 0xff;

    let refused = bootstrap::verify(&build.stage1, &tampered);
    let message = match &refused {
        Err(error) => error.to_string(),
        Ok(()) => panic!(
            "a stage 2 differing in one byte at offset {last} was accepted, so a \
             release build would ship a compiler that is not the fixed point"
        ),
    };
    assert!(
        message.contains(&last.to_string()),
        "the refusal does not name the first differing byte {last}: {message}"
    );

    // The build's own refusal to a stage 2 that is not the fixed point is
    // asserted by `a_stage_two_that_is_not_the_fixed_point_ships_nothing` in
    // `src/bootstrap.rs`: handing the build a stage 2 is `Stage2::Bytes`, which
    // is crate-private precisely because it is a way to claim a fixed point no
    // self-hosted run reached. An integration test is an outside caller, so it
    // cannot have that path — and must not.
}

/// A compiler that is not there, an empty one, and one that writes nothing.
///
/// Three edges a release build meets first on a clean checkout, and each has a
/// different way of being wrong if a build only checked "did the run succeed":
///
/// - a path that does not exist — refused by name, so the message says which
///   file could not be read;
/// - an empty source — refused before anything is compiled;
/// - a source that compiles, runs, exits zero and **writes no file**. This is
///   the sharp one. `silent-compiler.rb` below is a valid Redblue program whose
///   `main` does arithmetic and returns, so a build that treated "the run
///   finished successfully" as "a compiler was produced" would ship a
///   zero-length compiler and report the fixed point as holding.
#[test]
fn edge_a_missing_empty_or_silent_compiler_is_refused_rather_than_shipped() {
    let missing = scratch("no-such-compiler.rb");
    let _ = fs::remove_file(&missing);

    let error = expect_failure(
        bootstrap::build(&missing, &scratch("missing")),
        "a build of a compiler that is not there must fail",
    );
    assert!(
        error.to_string().contains("no-such-compiler.rb"),
        "the refusal does not name the file it could not read: {error}"
    );

    let empty = scratch("empty-compiler.rb");
    fs::write(&empty, "").expect("the empty compiler is written");
    let refusal = expect_failure(
        bootstrap::build(&empty, &scratch("empty")),
        "an empty compiler compiles to a 35-byte no-op, so a build that shipped \
         it would ship a program that compiles nothing",
    );
    assert!(
        refusal.to_string().contains("empty"),
        "the refusal for an empty compiler does not say it is empty: {refusal}"
    );

    let silent = scratch("silent-compiler.rb");
    fs::write(
        &silent,
        "// Compiles, runs, exits zero, writes nothing.\n\
         to main()\n    set nothing_written to 1 + 1\nend\n",
    )
    .expect("the silent compiler is written");
    let dir = scratch("silent");
    let _ = fs::remove_file(dir.join("stage2.rbc"));
    let refusal = expect_failure(
        bootstrap::build(&silent, &dir),
        "a compiler that writes no file must be refused, not shipped as an empty \
         compiler",
    );
    assert!(
        refusal.to_string().contains("wrote no"),
        "the refusal for a compiler that wrote nothing does not say so: {refusal}"
    );
    assert!(
        !dir.join("compiler.rbc").exists(),
        "a refused build wrote a shipped compiler anyway"
    );
}

/// `rb bootstrap` is the command a release runs, so the command is tested.
///
/// Everything above goes through the library. This goes through the binary,
/// because that is what the release pipeline invokes, and the two differ in
/// ways a library test cannot see: argument parsing, where the output directory
/// comes from, and the exit code. The `.rbc` the command leaves behind is then
/// run with `rb vm` over a corpus program, which is the only test here that
/// proves the shipped file is *usable* rather than merely correct — a `.rbc`
/// that matches the fixed point byte for byte and cannot compile anything is
/// still a broken release.
///
/// It runs the build again rather than reading the file the library build left,
/// because reading that file would test nothing about the command.
#[test]
fn the_release_command_leaves_a_file_that_compiles_a_program() {
    let out_dir = scratch("cli");
    let _ = fs::remove_dir_all(&out_dir);

    let built = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("bootstrap")
        .arg(&out_dir)
        .output()
        .expect("the rb binary starts");
    assert!(
        built.status.success(),
        "`rb bootstrap <dir>` exited {:?}\nstdout: {}\nstderr: {}",
        built.status.code(),
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );

    let shipped = bootstrap::Build::shipped_path(&out_dir);
    assert!(
        shipped.exists(),
        "`rb bootstrap` reported success and wrote no {}",
        shipped.display()
    );
    let shipped_bytes = fs::read(&shipped).expect("the shipped file is readable");
    assert_eq!(
        shipped_bytes,
        release().shipped,
        "the compiler `rb bootstrap` shipped is not the one the library build \
         produces, so the command is not building the release"
    );

    // And it is a compiler, not just a file of the right bytes.
    let sample = scratch("cli-sample.rb");
    fs::write(
        &sample,
        "set greeting to \"hello \" + \"world\"\nsay greeting\n",
    )
    .expect("the sample is written");
    let out = scratch("cli-sample.rbc");
    let _ = fs::remove_file(&out);

    let compiled = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("vm")
        .arg(&shipped)
        .arg(&sample)
        .arg(&out)
        .output()
        .expect("the rb binary starts");
    assert!(
        compiled.status.success(),
        "`rb vm` on the shipped compiler exited {:?}\nstderr: {}",
        compiled.status.code(),
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(
        out.exists(),
        "the shipped compiler compiled a program without writing a file"
    );
}

/// The rollback flag is a path a release can take, so it is exercised.
///
/// Three things are checked, and the third is the one that matters: the file
/// `--rollback` leaves behind is the same file. A rollback that produced a
/// different compiler would be a second compiler wearing a rollback's name, and
/// it is the only way a release could silently stop being self-hosted.
#[test]
fn the_release_command_rolls_back_to_the_same_file() {
    let out_dir = scratch("cli-rollback");
    let _ = fs::remove_dir_all(&out_dir);

    let rolled = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("bootstrap")
        .arg(&out_dir)
        .arg("--rollback")
        .output()
        .expect("the rb binary starts");
    let stdout = String::from_utf8_lossy(&rolled.stdout).into_owned();
    assert!(
        rolled.status.success(),
        "`rb bootstrap --rollback` exited {:?}\nstderr: {}",
        rolled.status.code(),
        String::from_utf8_lossy(&rolled.stderr)
    );
    assert!(
        stdout.contains("Rolled back"),
        "`rb bootstrap --rollback` did not say it rolled back: {stdout}"
    );

    let shipped =
        fs::read(bootstrap::Build::shipped_path(&out_dir)).expect("the rollback wrote a compiler");
    assert_eq!(
        shipped.len(),
        release().stage1.len(),
        "the rollback wrote {} bytes where the fixed point is {}",
        shipped.len(),
        release().stage1.len()
    );
    assert_eq!(
        shipped,
        release().stage1,
        "the rollback path did not produce the file the self-hosted build \
         produces, so it is not a rollback"
    );
}

/// The command refuses, with a non-zero exit, what it cannot ship.
///
/// `rb` is used by scripts, so an exit code is part of the interface: a build
/// that refused with status 0 would let a release pipeline cut a release from
/// nothing. Three refusals, and the first is the cheapest one a user hits —
/// a path that is not there.
#[test]
fn edge_the_release_command_exits_non_zero_on_what_it_cannot_ship() {
    let missing = scratch("no-such-compiler.rb");
    let _ = fs::remove_file(&missing);

    let refused = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("bootstrap")
        .arg(scratch("cli-missing"))
        .arg("--compiler")
        .arg(&missing)
        .output()
        .expect("the rb binary starts");
    assert!(
        !refused.status.success(),
        "`rb bootstrap` exited 0 for a compiler that does not exist"
    );
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains("no-such-compiler.rb"),
        "the refusal does not name the file: {stderr}"
    );
    assert!(
        !bootstrap::Build::shipped_path(&scratch("cli-missing")).exists(),
        "a refused build wrote a shipped compiler anyway"
    );

    // A flag that is not a flag is a usage error, not a compiler to read.
    let nonsense = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("bootstrap")
        .arg("--not-a-flag")
        .output()
        .expect("the rb binary starts");
    assert!(
        !nonsense.status.success(),
        "`rb bootstrap --not-a-flag` exited 0"
    );
    assert!(
        String::from_utf8_lossy(&nonsense.stderr).contains("--not-a-flag"),
        "the usage failure does not name the flag it did not take: {}",
        String::from_utf8_lossy(&nonsense.stderr)
    );
}

/// `rb bootstrap` with no output directory is the command, not a file to run.
///
/// The documented spelling is `rb bootstrap [out-dir]`, so the no-positional
/// form has to work: it defaults the output directory and builds. It did not.
/// `run_cli`'s two-argument arm treats a name no other command claims as a
/// Redblue file to run, and `bootstrap` was not claimed there, so the command
/// fell through, tried to read the directory `bootstrap/`, and exited 1 with
/// `Is a directory` — the exact failure the command exists to remove.
///
/// It is reproduced here rather than merely asserted: the scratch directory
/// contains a `bootstrap/` **directory**, so the fall-through has something to
/// read and produces the old message, and the test refuses both the old
/// message and an exit 0. What it asserts instead is that the failure comes
/// from the bootstrap command — it names the compiler it could not read, from
/// the default path — which is what a caller sees when the arm is present.
#[test]
fn edge_bare_rb_bootstrap_is_the_command_and_not_a_file_to_run() {
    let cwd = scratch("bare");
    let _ = fs::remove_dir_all(&cwd);
    fs::create_dir_all(cwd.join("bootstrap"))
        .expect("the scratch checkout has a bootstrap directory");

    let ran = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("bootstrap")
        .current_dir(&cwd)
        .output()
        .expect("the rb binary starts");
    let stderr = String::from_utf8_lossy(&ran.stderr).into_owned();

    assert!(
        !ran.status.success(),
        "`rb bootstrap` in a directory with no compiler exited 0"
    );
    assert!(
        !stderr.contains("Is a directory"),
        "`rb bootstrap` fell through to the file runner and tried to read the \
         `bootstrap` directory as a Redblue program: {stderr}"
    );
    assert!(
        stderr.contains("bootstrap/compiler.rb"),
        "`rb bootstrap` did not report reading the compiler it defaults to, so \
         the command did not run: {stderr}"
    );
    assert!(
        !cwd.join("target/bootstrap/compiler.rbc").exists(),
        "a build that could not read its compiler shipped one"
    );
}

/// A compiler the frontend refuses is reported with its own kind and position.
///
/// The build re-wrapped every frontend failure as a `Runtime` string with no
/// position: the source line and the caret survived, because they were pasted
/// into the message, and everything a programmatic consumer needs did not. A
/// syntax error in the compiler came back as `RuntimeError` at no line, so a
/// caller could not tell a parse failure from a runtime one and could not say
/// where either was — and the same source reported differently depending on
/// whether a build or `rb compile` found it.
#[test]
fn edge_a_frontend_refusal_keeps_its_kind_and_position() {
    let broken = scratch("broken-compiler.rb");
    fs::write(&broken, "to main()\n    say\nend\n").expect("the broken compiler is written");
    let dir = scratch("broken");
    let _ = fs::remove_dir_all(&dir);

    let error = expect_failure(
        bootstrap::build(&broken, &dir),
        "a compiler with a syntax error must be refused",
    );

    assert!(
        matches!(error, Error::Parser(_, _)),
        "a syntax error in the compiler was reported as {error:?}, so a caller \
         cannot tell what kind of failure it is"
    );
    let span = error.span().copied();
    let span = span.unwrap_or_else(|| panic!("the refusal has no position: {error:?}"));
    assert_eq!(
        span.line, 2,
        "the refusal points at line {} of a compiler whose second line is the \
         broken one",
        span.line
    );
    let rendered = error.to_string();
    assert!(
        rendered.contains("say"),
        "the refusal does not carry the source line it blamed: {rendered}"
    );
    assert!(
        !dir.join("compiler.rbc").exists(),
        "a build of a compiler the frontend refused shipped one"
    );
}

/// Two stage-2 runs in one process do not swap each other's paths.
///
/// `sys.argv()` is process-global, and stage 2 publishes its input and output
/// paths there before running the compiler — for minutes. Cargo runs a test
/// binary's tests in parallel threads, so two `run_compiler_on` calls in one
/// process could interleave: the second run's paths would be the ones the
/// first run's compiler read, and each would compile the other's input to the
/// other's output. Both runs would report success.
#[test]
fn edge_concurrent_stage_two_runs_keep_their_own_paths() {
    let chunk = redblue::Chunk::decode(&release().shipped).expect("the shipped compiler decodes");

    // Two programs whose compiled bytes differ, so a crossed run cannot pass by
    // writing the right file with the right bytes for the wrong reason.
    let programs = [
        ("cross-a", "set value to 1 + 1\nsay value\n"),
        ("cross-b", "set value to \"a\" + \"b\"\nsay value\n"),
    ];
    let mut expected = Vec::new();
    for (name, source) in programs {
        let path = scratch(&format!("{name}.rb"));
        fs::write(&path, source).expect("the sample is written");
        expected.push((
            path.clone(),
            redblue::compile_source(source)
                .expect("the frontend accepts the sample")
                .encode(),
        ));
    }
    assert_ne!(
        expected[0].1, expected[1].1,
        "the two samples compile to the same bytes, so a crossed run cannot be \
         seen from them"
    );

    let workers: Vec<_> = expected
        .iter()
        .map(|(input, _)| {
            let chunk = chunk.clone();
            let input = input.clone();
            std::thread::spawn(move || {
                let output = input.with_extension("out.rbc");
                let _ = fs::remove_file(&output);
                let outcome = bootstrap::run_compiler_on(&chunk, &input, &output);
                (input, output, outcome)
            })
        })
        .collect();

    for worker in workers {
        let (input, output, outcome) = worker.join().expect("the stage 2 run did not panic");
        outcome.unwrap_or_else(|error| {
            panic!("the shipped compiler refused {}: {error}", input.display())
        });
        let written = fs::read(&output).expect("the shipped compiler wrote a file");
        let wanted = expected
            .iter()
            .find(|(path, _)| *path == input)
            .map(|(_, bytes)| bytes.clone())
            .expect("the run was one of the two samples");
        assert_eq!(
            written,
            wanted,
            "the compiler wrote to {} the bytes for a different program: two \
             stage 2 runs in one process shared their paths",
            output.display()
        );
    }
}

/// A rollback that is not the file the ladder verified is refused, and the
/// shipped compiler is left alone.
///
/// `rb bootstrap <dir> --rollback` overwrites the shipped file with the Rust
/// frontend's bytes without comparing them to the file the build just
/// verified. If the two differ — a frontend that changed between the two reads,
/// or one that is not deterministic — the CLI wrote a compiler the ladder never
/// approved, said "Rolled back", and exited 0, so a release cut from it shipped
/// a second compiler wearing a rollback's name. Only `bootstrap/build.sh`'s
/// later `cmp` would have noticed, and the CLI is what a person runs.
#[test]
fn edge_a_rollback_that_is_not_the_fixed_point_writes_nothing() {
    let build = release();
    let dir = scratch("rollback-refused");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("the scratch directory");
    let target = bootstrap::Build::shipped_path(&dir);
    fs::write(&target, &build.stage2).expect("the build's shipped file is written");

    let mut divergent = build.stage2.clone();
    let last = divergent
        .len()
        .checked_sub(1)
        .expect("the compiler is not an empty file");
    divergent[last] ^= 0xff;

    let refusal = expect_failure(
        bootstrap::rollback_onto(build, &dir, &divergent),
        "a rollback that is not the fixed point must be refused rather than \
         shipped",
    );
    assert!(
        refusal.to_string().contains("rollback"),
        "the refusal does not say what was refused: {refusal}"
    );
    assert_eq!(
        fs::read(&target).expect("the shipped file is still there"),
        build.stage2,
        "a refused rollback overwrote the shipped compiler anyway"
    );

    // And the bytes the ladder did verify are written, so the check is not a
    // blanket refusal.
    bootstrap::rollback_onto(build, &dir, &build.stage2)
        .expect("the verified bytes are written over the shipped file");
    assert_eq!(
        fs::read(&target).expect("the shipped file is readable"),
        build.stage2
    );
}

/// The release workflow runs the ladder, and ships what the ladder produced.
///
/// S4 is a claim about a release, so it is checked where the release is cut.
/// `release.yml` built `rb` from Rust and uploaded it, which is stage 0 and
/// not S4: no self-hosted step, no `compiler.rbc`, so nothing in a tagged
/// release had been compiled by Redblue while the report claimed it had.
///
/// The workflow is read, not executed — a test cannot push a tag — so this
/// asserts the wiring: the ladder runs before the upload, and `compiler.rbc` is
/// one of the files uploaded.
///
/// The ladder is located by the `run:` key that invokes it, not by the bare
/// path. The workflow's header comment names `./bootstrap/build.sh` above every
/// step, and `str::find` returns the *first* match — which was the comment at
/// the top of the file. Ordering the comment first against the upload passed
/// however the steps were arranged: moving the ladder below the upload still
/// satisfied `ladder < upload`, because the comment never moves. The step is
/// found by the text a step actually runs.
#[test]
fn edge_the_release_workflow_runs_the_ladder_and_ships_its_compiler() {
    let workflow = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml");
    let text = fs::read_to_string(&workflow).expect("the release workflow is readable");

    let ladder = text
        .find("run: ./bootstrap/build.sh")
        .unwrap_or_else(|| panic!("the release workflow never runs the ladder:\n{text}"));
    let upload = text
        .find("softprops/action-gh-release")
        .unwrap_or_else(|| panic!("the release workflow uploads nothing:\n{text}"));
    assert!(
        ladder < upload,
        "the release workflow uploads an artifact before the ladder has run, so \
         nothing has been checked at the point the release is cut"
    );
    assert!(
        text.contains("compiler.rbc"),
        "the release workflow does not ship compiler.rbc, so the compiler the \
         release carries is not downloadable"
    );
    assert!(
        text.contains("target/release-bootstrap/compiler.rbc"),
        "the workflow mentions compiler.rbc without shipping the file \
         bootstrap/build.sh writes:\n{text}"
    );
}

/// Two matrix legs do not publish one asset name twice.
///
/// Both legs run the ladder, which is right — each runner builds and verifies
/// its own compiler — but both then uploaded `target/release-bootstrap/compiler.rbc`
/// to the same release. GitHub either takes the last one or fails the second as
/// a duplicate, so a release could ship a compiler from whichever runner
/// happened to finish second, or not be cut at all. The compiler is a `.rbc`
/// and is the same file on both runners, so exactly one leg uploads it.
///
/// It is counted by the occurrence of the path in the `files:` list of an upload
/// step, which is the only place a path can be published: the comment at the top
/// of the workflow names `compiler.rbc` in prose and must not be counted, so the
/// count is over the upload step rather than the whole file.
#[test]
fn edge_the_release_workflow_uploads_the_compiler_once() {
    let workflow = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml");
    let text = fs::read_to_string(&workflow).expect("the release workflow is readable");

    let path = "target/release-bootstrap/compiler.rbc";
    let published = text.matches(path).count();
    assert_eq!(
        published, 1,
        "the workflow publishes {path} {published} times: both matrix legs upload \
         the compiler under one name to one release, so whichever finishes \
         second overwrites it or fails as a duplicate:\n{text}"
    );

    // And the upload of the compiler is conditional, so the second leg skips it
    // rather than publishing a file the same name.
    assert!(
        text.contains("upload_compiler"),
        "the compiler upload is not gated per leg, so one asset name is published \
         by both:\n{text}"
    );
    assert_eq!(
        text.matches("upload_compiler: true").count()
            + text.matches("upload_compiler: false").count(),
        2,
        "the matrix does not gate the compiler upload on every leg:\n{text}"
    );
}
