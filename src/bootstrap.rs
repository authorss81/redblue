//! The bootstrap ladder's release build: stage **S4**.
//!
//! Stages S1 to S3 are about whether the self-hosted compiler is *correct*.
//! [`build`] is about who writes the file that ships.
//!
//! ```
//! use redblue::bootstrap;
//!
//! // The fixed point, as a pair of byte strings: a build that is handed two
//! // files which are the same file is the only thing that may ship.
//! let a = b"the same bytes".to_vec();
//! assert!(bootstrap::verify(&a, &a).is_ok());
//! ```
//!
//! A build is four steps and it refuses at each of them:
//!
//! 1. **stage 1** — `compile_source`, the Rust frontend, over
//!    `bootstrap/compiler.rb`.
//! 2. **stage 2** — the same source compiled by stage 1, running as bytecode in
//!    a [`Chunk`] the Rust frontend wrote. Its output is a file, and a run that
//!    wrote no file is a failure: an empty program exits successfully having
//!    written nothing, so "the run succeeded" is not "a compiler was produced".
//! 3. **verify** — stage 1 and stage 2 must be one file, byte for byte, or the
//!    first differing byte is named and the build stops.
//! 4. **ship** — the stage 2 bytes, written next to stage 1's as the compiler
//!    the release carries. Not stage 1's: the file that ships is the one
//!    Redblue wrote.
//!
//! Nothing here reaches around the ladder. There is no Rust fast path a
//! self-hosted run falls back to, because [`build`] performs step 2 by running
//! the stage 1 chunk in [`bytecode::vm::BytecodeVm`] — the same VM `rb vm` uses
//! — and never consults [`compile_source`] again until the next build.
//!
//! ## Why the shipped artifact is a `.rbc`
//!
//! `rb` is a Rust binary and no Redblue program emits a machine code
//! executable. What a Redblue program emits is bytecode, so "the release is
//! built by Redblue" means the compiler the release carries was written by the
//! compiler that Redblue ran, and this module exists to make that checkable
//! from a clean checkout and to refuse to ship when it does not hold. The
//! ladder's claim, unchanged: `stage1.rbc == stage2.rbc`, byte for byte.

use std::fs;
use std::path::{Path, PathBuf};

use crate::bytecode::vm::BytecodeVm;
use crate::bytecode::Chunk;
use crate::error::Span;
use crate::runtime;
use crate::Error;
use crate::{compile_source, Error::Io, Error::Runtime};

/// The compiler a release build produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Build {
    /// `bootstrap/compiler.rb` compiled by the Rust frontend.
    pub stage1: Vec<u8>,
    /// The same source compiled by stage 1, running as bytecode.
    pub stage2: Vec<u8>,
    /// The bytes the release carries: stage 2's, never stage 1's.
    pub shipped: Vec<u8>,
}

impl Build {
    /// The fixed point, read without the byte comparison: stage 1 and stage 2
    /// are one file, so what shipped is that file.
    pub fn is_fixed_point(&self) -> bool {
        self.stage1 == self.stage2 && self.shipped == self.stage2
    }

    /// The path the shipped compiler was written to, given the directory a build
    /// was given.
    pub fn shipped_path(out_dir: &Path) -> PathBuf {
        out_dir.join(SHIPPED_NAME)
    }
}

/// The name the shipped compiler has next to stage 1's and stage 2's.
pub const SHIPPED_NAME: &str = "compiler.rbc";

/// The name stage 1's file has in a build directory.
pub const STAGE1_NAME: &str = "stage1.rbc";

/// The name stage 2's file has in a build directory.
pub const STAGE2_NAME: &str = "stage2.rbc";

/// Where stage 2 comes from, so a test can hand the build a stage 2 that is not
/// the fixed point and watch it refuse.
///
/// **Crate-private, and it stays that way.** [`Stage2::Bytes`] is a way to hand
/// the build a stage 2 that no run produced, so publishing it would let any
/// library caller ship bytes as "the verified fixed point" with no self-hosted
/// run behind them — which is the one claim S4 exists to make. A comment saying
/// the only caller is a test is not enforcement, so the type is private and the
/// test is a unit test in this module rather than an integration test. [`build`]
/// has no parameter for it, so there is no public path to it at all.
///
/// The `Bytes` arm exists only under `cfg(test)` — the unit test
/// `a_stage_two_that_is_not_the_fixed_point_ships_nothing` is what constructs it.
/// Without the test build there is no way to reach it, and a dead arm is a
/// compile error rather than something a caller could find and use.
#[cfg_attr(not(test), derive(Debug))]
enum Stage2 {
    /// Compile the compiler's source with stage 1, running as bytecode.
    Computed,
    /// Take stage 2 from here. Only for proving the build refuses a bad pair;
    /// a build handed bytes has not produced them.
    #[cfg(test)]
    Bytes(Vec<u8>),
}

/// Builds the release compiler from `compiler_path` and writes the three files
/// into `out_dir`.
///
/// This is the whole of S4: stage 1 with the Rust frontend, stage 2 by running
/// stage 1 on the compiler's own source, the fixed point checked, and stage 2's
/// bytes written as the compiler that ships. It returns the three byte strings
/// so a caller can assert on them rather than trust that a file was written.
pub fn build(compiler_path: &Path, out_dir: &Path) -> Result<Build, Error> {
    build_with(compiler_path, out_dir, Stage2::Computed)
}

/// [`build`], with stage 2 supplied rather than computed.
///
/// Private, because the only caller that passes [`Stage2::Bytes`] is a test in
/// this module that has to make the fixed point fail on purpose, and a public
/// signature here would be a public way to claim a fixed point no run reached.
fn build_with(compiler_path: &Path, out_dir: &Path, stage2: Stage2) -> Result<Build, Error> {
    let text = read_compiler(compiler_path)?;

    // Before stage 2, not after: stage 2 writes `stage2.rbc` into this
    // directory through `files.write`, which does not create directories, so a
    // build into a directory that does not exist yet would be reported as "the
    // compiler refused" — the compiler refused nothing, the path was missing.
    fs::create_dir_all(out_dir)
        .map_err(|error| Io(format!("cannot create {}: {error}", out_dir.display())))?;

    let stage1 = compile_source(&text)
        .map_err(|error| rendered(error, &text, compiler_path))?
        .encode();

    let stage2 = match stage2 {
        Stage2::Computed => {
            let chunk = Chunk::decode(&stage1).map_err(|error| {
                Runtime(
                    format!("stage 1's own bytecode does not decode: {error}"),
                    Span::unknown(),
                )
            })?;
            run_compiler_on(&chunk, compiler_path, &out_dir.join(STAGE2_NAME))?;
            let path = out_dir.join(STAGE2_NAME);
            let bytes = fs::read(&path).map_err(|error| {
                Runtime(
                    format!("stage 2 wrote no file at {}: {error}", path.display()),
                    Span::unknown(),
                )
            })?;
            if bytes.is_empty() {
                return Err(Runtime(
                    format!(
                        "stage 2 wrote no bytecode to {}: the file is empty",
                        path.display()
                    ),
                    Span::unknown(),
                ));
            }
            bytes
        }
        #[cfg(test)]
        Stage2::Bytes(bytes) => bytes,
    };

    verify(&stage1, &stage2)?;

    write_stage1_and_ship(out_dir, &stage1, &stage2)?;

    Ok(Build {
        stage1,
        stage2: stage2.clone(),
        shipped: stage2,
    })
}

/// The rollback path: the Rust frontend alone, with no self-hosted run and no
/// fixed point to establish.
///
/// This is `rb compile bootstrap/compiler.rb`, and it exists so a release can
/// be cut from the Rust compiler when the ladder cannot be run — on a machine
/// that cannot afford a self-compilation, or while a fixed point is being
/// investigated. It produces the same bytes, which is what makes it a rollback
/// rather than a second compiler, and
/// `edge_the_rollback_path_ships_the_same_file_as_the_fixed_point` is what says
/// so.
pub fn rollback(compiler_path: &Path) -> Result<Vec<u8>, Error> {
    let text = read_compiler(compiler_path)?;
    let chunk = compile_source(&text).map_err(|error| rendered(error, &text, compiler_path))?;
    Ok(chunk.encode())
}

/// The compiler's source, read and refused if there is nothing in it.
///
/// Shared by [`build`] and [`rollback`] because both need the same two
/// refusals — a file that cannot be read, and a file that is empty — and a
/// rollback that lacked the second was the one path out of the ladder that
/// accepted a no-op: an empty source compiles to a ~35-byte chunk that returns
/// successfully and encodes an empty program, so `rollback(empty)` returned
/// `Ok` and `rollback_onto` would compare those bytes against a real stage 2 and
/// refuse them — but a caller reading `rollback` on its own, or the CLI
/// printing a length, saw a success for a compiler that compiles nothing.
fn read_compiler(compiler_path: &Path) -> Result<String, Error> {
    let text = fs::read_to_string(compiler_path).map_err(|error| {
        Io(format!(
            "cannot read the compiler {}: {error}",
            compiler_path.display()
        ))
    })?;
    if text.is_empty() {
        return Err(Io(format!(
            "the compiler {} is empty, so there is nothing to compile",
            compiler_path.display()
        )));
    }
    Ok(text)
}

/// Writes the frontend's bytes over the shipped file, but only once they are
/// known to be the file the ladder verified.
///
/// A rollback is a rollback because it produces the same file: the bytes are
/// the same and only the run that produced them differs. Comparing against
/// `built.stage2` — the bytes this build verified and wrote — is what makes
/// that a check rather than an intention, and it is the difference between a
/// rollback and a second compiler wearing a rollback's name. A frontend that
/// changed between the build and the rollback, or one that is not
/// deterministic, produces bytes the ladder never approved, and those are not
/// written: the refusal names both lengths and the shipped file is left as the
/// build left it.
pub fn rollback_onto(built: &Build, out_dir: &Path, rolled_back: &[u8]) -> Result<(), Error> {
    let target = Build::shipped_path(out_dir);
    if rolled_back != built.stage2 {
        return Err(Runtime(
            format!(
                "the rollback is not the file the fixed point verified: the frontend \
                 wrote {} bytes where stage 2 wrote {}, so {} is left as the build \
                 wrote it",
                rolled_back.len(),
                built.stage2.len(),
                target.display()
            ),
            Span::unknown(),
        ));
    }
    fs::write(&target, rolled_back).map_err(|error| {
        Io(format!(
            "cannot write the shipped compiler {}: {error}",
            target.display()
        ))
    })
}

/// A frontend failure, carrying its source line and caret, as a build failure.
///
/// The [`Error`]'s **variant and span are the ones the frontend produced**: a
/// compiler with a syntax error is a `Parser` failure at that line and column,
/// and a caller that matches on the variant or reads the position gets what
/// `rb compile` would give it. Only the message grows, to append the source
/// line and the caret [`Error::render`] draws.
///
/// Re-wrapping it as `Runtime(msg, Span::unknown())` — which is what this did
/// — kept the rendered text and threw away everything a programmatic consumer
/// needs: the kind of failure, and where it was. The same source would report
/// differently depending on whether a build or a `rb compile` found it.
fn rendered(error: Error, text: &str, compiler_path: &Path) -> Error {
    let full = error.render(text, compiler_path.to_str());
    let label = format!("{}: ", error.label());
    let body = full
        .strip_prefix(label.as_str())
        .unwrap_or(full.as_str())
        .to_string();

    match error {
        Error::Lexer(_, span) => Error::Lexer(body, span),
        Error::Parser(_, span) => Error::Parser(body, span),
        Error::Analyzer(_, span) => Error::Analyzer(body, span),
        Error::Runtime(_, span) => Error::Runtime(body, span),
        Error::Io(message) => Error::Io(message),
    }
}

/// One stage-2 run at a time.
///
/// `sys.argv()` is a process-global (see [`runtime::set_program_args`]), and
/// stage 2 publishes the input and output paths there before running a VM that
/// takes minutes. Two builds in one process would otherwise interleave: the
/// second run's paths would be the ones the first run's compiler read, and each
/// would compile the other's input to the other's output. Cargo runs a test
/// binary's tests in parallel threads, and `bootstrap_ship_test` does build
/// concurrently with the corpus tests, so this is not hypothetical.
///
/// A lock rather than per-VM argv because `sys.argv()` is resolved by
/// [`runtime::builtin`], a free function with no VM to hang arguments off; giving
/// it a VM-scoped argument list is a change to the stdlib call path, which is a
/// bigger change than the bug needs.
///
/// **What this does not do: it does not make the global safe to read at an
/// arbitrary moment.** It serialises *runs* — one publish-and-restore pair at a
/// time, so no two runs can be inside their window together. A bare
/// [`runtime::take_program_args`] from another thread can still land inside that
/// window and take a run's paths as "what was there before", and the restore
/// will then correctly put back the process default. Anything that reads the
/// global across a call another thread can also make has to hold this lock too,
/// which is why `argv_while_no_run_is_publishing` in the tests below does.
static STAGE2: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Runs `chunk` — a compiled Redblue compiler — over `input_path`, writing a
/// `.rbc` to `output_path`.
///
/// This is stage 2 in one call, and it is the same code `rb vm` runs: the chunk
/// is handed to a [`BytecodeVm`] with the two paths as `sys.argv()`. The VM's
/// output is captured rather than printed, so a caller gets the compiler's
/// diagnostics as a string to put in its own failure message.
///
/// A run that fails, or that succeeds having written no file, is an error.
///
/// One stage-2 run happens at a time, because `sys.argv()` is process-global;
/// see [`STAGE2`].
pub fn run_compiler_on(chunk: &Chunk, input_path: &Path, output_path: &Path) -> Result<(), Error> {
    let _guard = STAGE2
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let _ = fs::remove_file(output_path);

    // `sys.argv()` belongs to the process, not to the run: taking what was
    // there before publishing this run's paths is what lets them be put back.
    // Without the restore, a build leaves its stage-2 input and output in place
    // for whatever reads `sys.argv()` next in the same process — a corpus test
    // running beside this one would be handed a `.rbc` path as its arguments.
    let previous = runtime::take_program_args();
    runtime::set_program_args(vec![
        input_path.display().to_string(),
        output_path.display().to_string(),
    ]);

    let (outcome, said) = stage2_run(chunk);
    runtime::set_program_args(previous);

    if let Err(error) = outcome {
        return Err(refused(error, &said, input_path));
    }

    match fs::metadata(output_path) {
        Ok(meta) if meta.len() > 0 => Ok(()),
        Ok(_) => Err(Runtime(
            format!(
                "the compiler wrote no bytecode to {}: the file is empty",
                output_path.display()
            ),
            Span::unknown(),
        )),
        Err(error) => Err(Runtime(
            format!(
                "the compiler wrote no file at {}: {error}",
                output_path.display()
            ),
            Span::unknown(),
        )),
    }
}

/// The stage-2 run itself, and whatever the compiler said while it ran.
///
/// Split from [`run_compiler_on`] so the argv this run published can be restored
/// on both the success and the failure path before anything returns: a `?` on the
/// way out would skip a restore written after the run.
fn stage2_run(chunk: &Chunk) -> (crate::error::Result<crate::value::Value>, String) {
    let mut vm = BytecodeVm::new();
    vm.set_echo(false);
    let outcome = vm.run(chunk);
    let said = vm.take_output().join("\n");
    (outcome, said)
}

/// A stage-2 refusal, keeping the VM's **variant and span**.
///
/// The compiler refuses by writing to stderr and then raising, so what comes
/// back is the VM's own failure — a [`Error::Runtime`] at a line in
/// `bootstrap/compiler.rb`, or whatever kind of failure the run actually was.
/// This keeps that kind and position the way [`rendered`] keeps the frontend's,
/// and only prepends the context a build has and a bare `rb vm` does not: which
/// file it was compiling.
///
/// Returning `Runtime(message, Span::unknown())` on every path — which this did
/// — threw both away, so a caller could not tell what kind of failure a stage-2
/// run was, or where in the compiler it was, and an `Io` failure (a run that
/// could not read its input) arrived as a `Runtime` at no line.
///
/// `said` is the compiler's own diagnostics, which are more specific than the
/// VM's message and are used in preference to it when it said something.
fn refused(error: Error, said: &str, input_path: &Path) -> Error {
    let detail = if said.is_empty() {
        error.message().to_string()
    } else {
        said.to_string()
    };
    let message = format!("the compiler refused {}: {detail}", input_path.display());

    match error {
        Error::Lexer(_, span) => Error::Lexer(message, span),
        Error::Parser(_, span) => Error::Parser(message, span),
        Error::Analyzer(_, span) => Error::Analyzer(message, span),
        Error::Runtime(_, span) => Error::Runtime(message, span),
        Error::Io(detail) => Error::Io(format!("{message}: {detail}")),
    }
}

/// Checks the fixed point: `stage1` and `stage2` must be one file, byte for
/// byte.
///
/// The failure names the first byte that differs and both lengths, because
/// "the compiler disagrees" with nothing else is the same as a build that
/// refuses for reasons it will not say.
pub fn verify(stage1: &[u8], stage2: &[u8]) -> Result<(), Error> {
    if stage1 == stage2 {
        return Ok(());
    }
    let first = stage1
        .iter()
        .zip(stage2.iter())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| stage1.len().min(stage2.len()));
    Err(Runtime(
        format!(
            "the fixed point does not hold: stage 1 and stage 2 differ at byte \
             {first} of {} and {} (stage 1 wrote {} bytes, stage 2 wrote {})",
            stage1.len(),
            stage2.len(),
            stage1.len(),
            stage2.len()
        ),
        Span::unknown(),
    ))
}

/// Writes stage 1's file and the shipped compiler. Stage 2's file was written
/// by the compiler itself and is left as the compiler left it.
fn write_stage1_and_ship(out_dir: &Path, stage1: &[u8], stage2: &[u8]) -> Result<(), Error> {
    fs::create_dir_all(out_dir)
        .map_err(|error| Io(format!("cannot create {}: {error}", out_dir.display())))?;
    fs::write(out_dir.join(STAGE1_NAME), stage1).map_err(|error| {
        Io(format!(
            "cannot write {}: {error}",
            out_dir.join(STAGE1_NAME).display()
        ))
    })?;
    fs::write(Build::shipped_path(out_dir), stage2).map_err(|error| {
        Io(format!(
            "cannot write the shipped compiler {}: {error}",
            Build::shipped_path(out_dir).display()
        ))
    })
}

// The two paths that must not be reachable from outside the crate are the ones
// with no business being public, so these tests live here rather than in
// `tests/`: `Stage2::Bytes` and `build_with` are crate-private, and a test in
// `tests/` is an outside caller.

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/bootstrap-unit");
        fs::create_dir_all(&dir).expect("scratch directory");
        dir.join(name)
    }

    /// A stage 2 that is not the fixed point ships nothing.
    ///
    /// This is what [`Stage2::Bytes`] exists for: the build has to be *able* to
    /// be handed a stage 2 and refuse it, or the fixed point would be an
    /// intention. It is also why `Stage2` is crate-private — a public
    /// `build_with` would let any caller hand the build bytes no self-hosted run
    /// produced and call the result a verified fixed point, which is the one
    /// claim this module exists to make.
    #[test]
    fn a_stage_two_that_is_not_the_fixed_point_ships_nothing() {
        let dir = scratch("refused");
        let _ = fs::remove_dir_all(&dir);

        // A compiler that is really on disk. `build_with` reads the source
        // before it consults the stage 2 it was handed, so this test used to be
        // handed `Path::new("unused")` — a file that does not exist — and
        // satisfied both of its assertions on the *missing file*: `read_compiler`
        // refused, and nothing was written because nothing ran. `verify` was
        // never called, so the test passed with `verify` answering `Ok(())` to
        // every pair, which is the only claim it exists to pin.
        let compiler = scratch("refused-compiler.rb");
        fs::write(&compiler, "say 1\n").expect("the compiler is written");

        let stage1 = compile_source("say 1\n").expect("the frontend accepts the program");
        let stage1 = stage1.encode();
        let mut tampered = stage1.clone();
        let last = tampered
            .len()
            .checked_sub(1)
            .expect("the program is not empty");
        tampered[last] ^= 0xff;

        let outcome = build_with(&compiler, &dir, Stage2::Bytes(tampered));

        let error = match outcome {
            Ok(build) => panic!(
                "a build handed a stage 2 that is not the fixed point shipped {} \
                 bytes instead of refusing",
                build.shipped.len()
            ),
            Err(error) => error,
        };

        // The refusal is the *fixed point* check and it says where. Without the
        // offset the test would accept a refusal for any reason, which is what
        // the missing-file path demonstrated.
        let message = error.to_string();
        assert!(
            message.contains("the fixed point does not hold"),
            "the refusal does not name the fixed point, so it may have refused \
             for some other reason entirely: {message}"
        );
        assert!(
            message.contains(&format!("byte {last}")),
            "the refusal does not name byte {last}, which is the byte that was \
             flipped: {message}"
        );

        assert!(
            !Build::shipped_path(&dir).exists(),
            "a build that refused to ship wrote a shipped compiler anyway"
        );
        assert!(
            !dir.join(STAGE1_NAME).exists(),
            "a build that refused to ship wrote stage 1 anyway, so the directory \
             holds a half-finished build that looks like a successful one"
        );
    }

    /// The refusal that the bytes *are* the fixed point check, reached through
    /// `build_with` with a matching pair, is not a blanket refusal.
    ///
    /// The test above could pass if `Stage2::Bytes` refused everything. Handing
    /// back the fixed point itself has to be accepted, or the private path tests
    /// a build that refuses rather than a build that compares.
    #[test]
    fn a_stage_two_that_is_the_fixed_point_is_written() {
        let compiler = scratch("tiny-compiler.rb");
        fs::write(&compiler, "say 1\n").expect("the compiler is written");
        let dir = scratch("fixed-point");
        let _ = fs::remove_dir_all(&dir);

        let stage1 = compile_source("say 1\n")
            .expect("the frontend accepts the compiler")
            .encode();
        let built = build_with(&compiler, &dir, Stage2::Bytes(stage1.clone()))
            .unwrap_or_else(|error| panic!("the fixed point was refused: {error:?}"));

        assert!(built.is_fixed_point());
        assert_eq!(
            fs::read(Build::shipped_path(&dir)).expect("the shipped file is readable"),
            stage1,
            "the build wrote something other than the verified bytes"
        );
    }

    /// `rollback` refuses an empty compiler, as `build` does.
    ///
    /// An empty source compiles: the frontend accepts it and hands back a
    /// ~35-byte chunk encoding an empty program. `build` refuses it before
    /// compiling, and `rollback` did not — so `rollback` on an empty file
    /// returned `Ok` for a "compiler" that compiles nothing, and the CLI's
    /// `--rollback` path had a success to print a length for. The two paths are
    /// the same operation on the same file, so they refuse the same file.
    #[test]
    fn an_empty_compiler_is_refused_by_the_rollback_path_too() {
        let empty = scratch("empty-compiler.rb");
        fs::write(&empty, "").expect("the empty compiler is written");

        let built = rollback(&empty);
        let error = match built {
            Ok(bytes) => panic!(
                "rollback compiled an empty compiler into {} bytes and reported \
                 success: a rollback of nothing is not a rollback",
                bytes.len()
            ),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("empty"),
            "the refusal for an empty compiler does not say it is empty: {error}"
        );

        // And `build` refuses the same file with the same reason, so the two
        // paths cannot drift apart.
        let refused = build(&empty, &scratch("empty-build"));
        assert!(
            refused.is_err(),
            "the build path accepted an empty compiler, so the two paths disagree"
        );
    }

    /// `sys.argv()` read at a moment when no stage-2 run is publishing into it.
    ///
    /// `PROGRAM_ARGS` is a process-global and [`STAGE2`] is what makes a
    /// publish-and-restore pair atomic against another *run*. It does nothing
    /// for a bare read: a thread that calls [`runtime::take_program_args`]
    /// while another thread is inside its publish-and-restore window takes that
    /// run's input and output as "what was there before", and the restore then
    /// correctly puts back the process default — so the two disagree and the
    /// only way to tell which is wrong is to lose a race in CI.
    ///
    /// That is exactly what happened: this module ran the sample bare, and two
    /// tests in this binary share one thread pool, so
    /// `a_stage_two_run_restores_the_arguments_it_published` failed with
    /// another run's paths on one side and `[]` on the other.
    fn argv_while_no_run_is_publishing() -> Vec<String> {
        let _guard = STAGE2
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        runtime::take_program_args()
    }

    /// A stage-2 refusal keeps the VM's kind and position.
    ///
    /// `run_compiler_on` re-wrapped every failure as `Runtime(message,
    /// Span::unknown())`, which kept the text and dropped everything a
    /// programmatic consumer needs: what kind of failure it was and where. The
    /// same run would report one way as a build and another as `rb vm`. The
    /// failure here is the VM's own, so its variant and span have to arrive
    /// intact — this asserts a runtime failure comes back as a positioned
    /// `Runtime` rather than an unpositioned one.
    #[test]
    fn a_stage_two_refusal_keeps_its_kind_and_position() {
        // A name no other test in this binary uses. These tests run in parallel
        // threads, and `run_compiler_on` *deletes* its output before it runs —
        // so two tests sharing one scratch path delete each other's input while
        // the other's VM is mid-run. `boom.rb`/`boom.rbc` were each used twice.
        let program = scratch("out-of-bounds.rb");
        fs::write(
            &program,
            "// Reads past the end of a list, which is a positioned runtime\n\
             // failure in the compiler itself.\n\
             set xs to [1, 2]\n\
             say xs[9]\n",
        )
        .expect("the failing compiler is written");

        let chunk = compile_source(&fs::read_to_string(&program).expect("readable"))
            .expect("the frontend accepts the compiler")
            .encode();
        let chunk = Chunk::decode(&chunk).expect("its own bytecode decodes");

        let out = scratch("boom.rbc");
        let _ = fs::remove_file(&out);

        let error = match run_compiler_on(&chunk, &program, &out) {
            Ok(()) => panic!("a compiler that reads out of bounds must be refused"),
            Err(error) => error,
        };
        assert!(
            matches!(error, Error::Runtime(_, _)),
            "the refusal came back as {error:?}, so a caller cannot tell what \
             kind of failure a stage-2 run was"
        );
        let span = error.span().copied();
        assert!(
            span.is_some(),
            "the refusal has no position: {error:?}, so a caller cannot say where \
             in the compiler it was"
        );
    }

    /// `run_compiler_on` puts `sys.argv()` back the way it found it.
    ///
    /// `sys.argv()` is a process-global. Stage 2 publishes its input and output
    /// paths there before running a compiler for minutes, and without a restore
    /// those paths were left for whatever read `sys.argv()` next in the same
    /// process — a program run beside a build would be handed a `.rbc` path as
    /// its own arguments. Both paths are checked: the run that succeeded and the
    /// run that refused, because a `?` between them is exactly how a restore gets
    /// skipped on the failure path.
    #[test]
    fn a_stage_two_run_restores_the_arguments_it_published() {
        let before = argv_while_no_run_is_publishing();

        // A "compiler" that reads the paths out of argv and writes a file, so the
        // success path both reads the arguments it published and writes the file
        // `run_compiler_on` checks for.
        let source = "set args to sys.argv()\n\
             set xs to [1, 2]\n\
             files.write(args[1], \"say \" + to_text(xs[0]))\n";
        let chunk = compile_source(source).expect("the frontend accepts the program");
        let input = scratch("argv-in.rb");
        fs::write(&input, source).expect("the program is written");

        let ok_out = scratch("argv-ok.rbc");
        let _ = fs::remove_file(&ok_out);
        run_compiler_on(&chunk, &input, &ok_out).expect("the compiler succeeds");

        let boom = scratch("argv-out-of-bounds.rb");
        fs::write(&boom, "set xs to [1]\nsay xs[9]\n").expect("the failing program is written");
        let boom_chunk = compile_source(&fs::read_to_string(&boom).expect("readable"))
            .expect("the frontend accepts the failing program");
        let boom_out = scratch("argv-out-of-bounds.rbc");
        let _ = fs::remove_file(&boom_out);
        assert!(
            run_compiler_on(&boom_chunk, &boom, &boom_out).is_err(),
            "a compiler that reads out of bounds must be refused"
        );

        assert_eq!(
            argv_while_no_run_is_publishing(),
            before,
            "a stage-2 run left its input and output paths in sys.argv()"
        );

        // The compiler really did see its own arguments, so the check above is
        // not passing because nothing was ever published.
        assert!(
            ok_out.exists(),
            "the successful run wrote no file, so its argv was never read either"
        );
    }

    /// [`verify`] accepts one file and refuses every other, at every boundary.
    ///
    /// `verify` is the ladder's whole rule and it is what `build` calls before
    /// it writes anything, so the shapes of "not equal" it has to tell apart are
    /// the shapes a compiler disagreement arrives in. Two are missed by a
    /// comparison that only zips: a shorter stage 2, and a longer one, where
    /// every byte they share agrees and the first difference is one past the
    /// end of the shorter file. The reported offset is asserted for each, so a
    /// refusal that says "the fixed point does not hold" and nothing else is a
    /// failure here rather than a pass.
    #[test]
    fn edge_the_fixed_point_comparison_names_the_offset_that_differs() {
        // empty, and singleton: the two sizes at which a comparison that
        // indexes rather than iterates would go out of bounds.
        assert!(
            verify(&[], &[]).is_ok(),
            "two empty files are one file and the fixed point holds"
        );
        assert!(
            verify(b"a", b"a").is_ok(),
            "two one-byte files that agree are the fixed point"
        );

        let cases: &[(&str, &[u8], &[u8], &str)] = &[
            ("differing first byte", b"ab", b"bb", "byte 0"),
            ("differing last byte", b"ab", b"ac", "byte 1"),
            ("singleton against empty", b"", b"x", "byte 0"),
            ("empty against singleton", b"x", b"", "byte 0"),
            // The two a zip cannot see: every shared byte agrees, and the files
            // differ only in length.
            ("stage 2 a prefix of stage 1", b"abc", b"ab", "byte 2"),
            ("stage 1 a prefix of stage 2", b"ab", b"abc", "byte 2"),
        ];

        for (why, stage1, stage2, offset) in cases {
            let error = match verify(stage1, stage2) {
                Ok(()) => panic!("{why}: the fixed point was accepted"),
                Err(error) => error,
            };
            let message = error.to_string();
            assert!(
                message.contains(offset),
                "{why}: the refusal does not name {offset}, so a disagreement \
                 at the end of a file is reported as one with no position: \
                 {message}"
            );
        }

        // And the offset is the *first* difference, not merely one of them.
        let error = verify(b"axyz", b"axzz").expect_err("the pair disagrees");
        assert!(
            error.to_string().contains("byte 2"),
            "the refusal names a difference that is not the first one: {error}"
        );
    }

    /// A stage-2 run that exits cleanly without producing bytecode is refused.
    ///
    /// `run_compiler_on`'s contract is that it wrote a `.rbc`, and a run that
    /// merely succeeded is not that: an empty Redblue program exits zero having
    /// done nothing, and a compiler whose output file is written empty exits
    /// zero having produced no compiler. `build` catches the second by way of
    /// `verify`, but `run_compiler_on` is public and is what a caller reads the
    /// compiler through, so both refusals are checked here against the three
    /// shapes directly — nothing written, an empty file written, and a file with
    /// bytes in it — so the last one shows the checks are not a blanket refusal.
    #[test]
    fn edge_a_stage_two_run_that_writes_no_bytecode_is_refused() {
        let cases: &[(&str, &str)] = &[
            // Wrote nothing at all.
            (
                "said something and wrote nothing",
                "say \"nothing to compile\"\n",
            ),
            // Wrote a file with no bytes in it.
            ("wrote an empty file", "files.write(sys.argv()[1], \"\")\n"),
        ];

        for (why, source) in cases {
            let chunk = compile_source(source).expect("the frontend accepts the program");
            let input = scratch("writes-nothing-in.rb");
            fs::write(&input, source).expect("the program is written");
            let out = scratch("writes-nothing-out.rbc");
            let _ = fs::remove_file(&out);

            let error = match run_compiler_on(&chunk, &input, &out) {
                Ok(()) => panic!("{why}: the run reported a compiler it did not write"),
                Err(error) => error,
            };
            let message = error.to_string();
            assert!(
                message.contains("the compiler wrote no"),
                "{why}: the refusal does not say what is missing: {message}"
            );
            assert!(
                message.contains(&out.display().to_string()),
                "{why}: the refusal does not name the file that is missing: {message}"
            );
        }

        // The same path, writing bytes, is accepted — so the two refusals above
        // are the absence of a compiler rather than a refusal of everything.
        let source = "files.write(sys.argv()[1], \"say 1\")\n";
        let chunk = compile_source(source).expect("the frontend accepts the program");
        let input = scratch("writes-bytes-in.rb");
        fs::write(&input, source).expect("the program is written");
        let out = scratch("writes-bytes-out.rbc");
        let _ = fs::remove_file(&out);
        run_compiler_on(&chunk, &input, &out)
            .unwrap_or_else(|error| panic!("a compiler that wrote bytes was refused: {error}"));
        assert_eq!(
            fs::read(&out).expect("the compiler wrote a file"),
            b"say 1".to_vec(),
            "the run accepted a compiler but did not leave what it wrote"
        );
    }
}
