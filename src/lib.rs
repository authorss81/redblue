pub mod analyzer;
pub mod bootstrap;
pub mod bytecode;
mod error;
pub mod formatter;
mod interpreter;
pub mod lexer;
pub mod linter;
pub mod lsp;
pub mod parser;
pub mod repl;
mod runtime;
pub mod stdlib;
pub mod testing;
mod value;

use std::env;
use std::fs;
use std::path::Path;
use std::process;

use crate::lexer::Lexer;
use testing::reporter::Reporter;

pub use error::{Error, Span};
pub use lsp::{
    diagnostics, diagnostics_json, diagnostics_to_json, textmate_grammar, Diagnostic, Severity,
    GRAMMAR_PATH, LANGUAGE_CONFIG_PATH,
};
// `FunctionValue` is re-exported because it is the payload of the public
// `Value::Function` variant: a caller that matches that variant has to be able
// to name the type it binds.
pub use bytecode::{compile_source, Chunk, Opcode};
pub use interpreter::{
    resolve_max_iterations, resolve_max_iterations_from, resolve_max_steps, resolve_max_steps_from,
    run_isolated, Vm, MAX_CALL_DEPTH, MAX_CALL_DEPTH_ENV, MAX_ITERATIONS, MAX_ITERATIONS_ENV,
    MAX_STEPS, MAX_STEPS_ENV, NETWORK_CONNECT_TIMEOUT_SECS, NETWORK_TIMEOUT_SECS,
};
pub use value::{expect_repeat_count, FunctionBody, FunctionValue, Value};

pub fn run_file(path: &str) -> Result<(), Error> {
    let source = fs::read_to_string(path).map_err(|e| Error::Io(e.to_string()))?;

    run_source(&source)
}

/// Runs `path` and, when it fails, renders the source line and caret for the
/// failure next to the message.
fn run_file_with_diagnostic(path: &str) -> std::result::Result<(), String> {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(e) => return Err(Error::Io(e.to_string()).to_string()),
    };

    run_source(&source).map_err(|e| e.render(&source, Some(path)))
}

fn report(rendered: &str) -> ! {
    eprintln!("Error: {}", rendered);
    process::exit(1);
}

pub fn run_source(source: &str) -> Result<(), Error> {
    let tokens = Lexer::tokenize(source)?;
    let ast = parser::parse(tokens)?;
    analyzer::analyze(&ast)?;

    let (_vm, result) = interpreter::run_isolated(&ast);
    result.map(|_| ())
}

/// The whole pipeline on `source` — lexer, parser, analyzer, then the VM — and
/// the value of its last statement.
///
/// [`run_source`] is the same pipeline and reports only whether it succeeded.
/// This returns what the program evaluated to, so a caller can assert on a run
/// that went through the analyzer rather than around it: a test that skips the
/// analyzer cannot fail on what the analyzer refuses.
pub fn run_source_value(source: &str) -> Result<Value, Error> {
    let tokens = Lexer::tokenize(source)?;
    let ast = parser::parse(tokens)?;
    analyzer::analyze(&ast)?;

    let (_vm, result) = interpreter::run_isolated(&ast);
    result
}

pub fn run_repl() {
    let mut repl = repl::Repl::new();
    repl.run();
}

pub fn run_test(path: Option<&str>) -> Result<(), Error> {
    let mut harness = testing::TestHarness::new();

    match path {
        Some(p) => {
            harness.run_file(p)?;
            // Report the failures and exit non-zero. Without this a failing
            // `expect` printed only a dot/F and still exited 0.
            testing::PrettyReporter::new().report(&harness.results());
        }
        None => {
            let results = testing::run_all_tests()?;
            println!("Tests run: {}", results.total);
            println!("Passed: {}", results.passed);
            println!("Failed: {}", results.failed);
            if results.failed > 0 {
                process::exit(1);
            }
        }
    }

    Ok(())
}

pub fn run_cli() {
    let args: Vec<String> = env::args().collect();

    match args.len() {
        1 => run_repl(),
        2 => {
            let cmd = &args[1];
            match cmd.as_str() {
                "help" | "--help" | "-h" => print_help(),
                "version" | "--version" | "-v" => print_version(),
                "test" => {
                    if let Err(e) = run_test(None) {
                        eprintln!("Error: {}", e);
                        process::exit(1);
                    }
                }
                "grammar" => print!("{}", textmate_grammar()),
                "keywords" => {
                    for keyword in Lexer::keywords() {
                        println!("{}", keyword);
                    }
                }
                // `rb bootstrap` with no output directory. It has to be named
                // here, in the arm that treats an unknown name as a file to
                // run: without this arm it fell through to the file-runner,
                // tried to read `bootstrap` — a directory in a checkout — and
                // exited 1 with `Is a directory`, which is the exact failure the
                // command exists to remove. The `3 if` arm below handles the
                // one-positional form; this is the same command with the
                // output directory defaulted to `target/bootstrap`.
                "bootstrap" => {
                    if let Err(message) = bootstrap_args(&[]) {
                        eprintln!("Error: {}", message);
                        process::exit(1);
                    }
                }
                _ => {
                    if let Err(rendered) = run_file_with_diagnostic(cmd) {
                        report(&rendered);
                    }
                }
            }
        }
        // `rb bootstrap <out-dir>` — before the `3` arm, which would read the
        // directory as a file to run. Bare `rb bootstrap` is the `bootstrap`
        // arm in the `2` match above.
        3 if args[1] == "bootstrap" => {
            if let Err(message) = bootstrap_args(&args[2..]) {
                eprintln!("Error: {}", message);
                process::exit(1);
            }
        }
        3 => {
            let cmd = &args[1];
            let path = &args[2];
            match cmd.as_str() {
                "test" => {
                    if let Err(e) = run_test(Some(path)) {
                        eprintln!("Error: {}", e);
                        process::exit(1);
                    }
                }
                "run" => {
                    if let Err(rendered) = run_file_with_diagnostic(path) {
                        report(&rendered);
                    }
                }
                "compile" => {
                    if let Err(message) = compile_command(path, None) {
                        eprintln!("{}", message);
                        process::exit(1);
                    }
                }
                "dis" => {
                    if let Err(message) = dis_command(path) {
                        eprintln!("{}", message);
                        process::exit(1);
                    }
                }
                "vm" => {
                    if let Err(message) = vm_command(path) {
                        eprintln!("Error: {}", message);
                        process::exit(1);
                    }
                }
                "format" => match fs::read_to_string(path) {
                    Ok(source) => match formatter::format(&source) {
                        Ok(formatted) => print!("{}", formatted),
                        Err(e) => {
                            eprintln!("Format error: {}", e);
                            process::exit(1);
                        }
                    },
                    Err(e) => {
                        eprintln!("Error reading file: {}", e);
                        process::exit(1);
                    }
                },
                "lint" => match fs::read_to_string(path) {
                    Ok(source) => {
                        let (errors, warnings) = linter::lint(&source);
                        // The line is printed with every finding: a file with
                        // five unused variables says nothing about which `set`
                        // each one names, and a reader cannot act on that. The
                        // path is not repeated — the reader named the file.
                        for warning in &warnings {
                            eprintln!("Warning: line {}: {}", warning.line, warning.message);
                        }
                        for error in &errors {
                            eprintln!("Error: line {}: {}", error.line, error.message);
                        }
                        if !errors.is_empty() {
                            process::exit(1);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error reading file: {}", e);
                        process::exit(1);
                    }
                },
                "diagnostics" => match fs::read_to_string(path) {
                    Ok(source) => {
                        let found = diagnostics(&source);
                        // Rendered from the findings just computed: re-running
                        // the frontend to format them would lex, parse and
                        // analyze the file a second time.
                        println!("{}", diagnostics_to_json(&found));
                        // Exit non-zero when anything is an error, the way
                        // `rb lint` does, so this is usable as a CI check.
                        if found.iter().any(|d| d.severity == Severity::Error) {
                            process::exit(1);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error reading file: {}", e);
                        process::exit(1);
                    }
                },
                _ => {
                    print_help();
                    process::exit(1);
                }
            }
        }
        5 if args[1] == "compile" && args[3] == "-o" => {
            if let Err(message) = compile_command(&args[2], Some(&args[4])) {
                eprintln!("{}", message);
                process::exit(1);
            }
        }
        // `rb bootstrap [out-dir] [--compiler <file>] [--rollback]` — matched
        // on the command name alone rather than on an argument count, because
        // its flags make the count arbitrary: the `4` arm below claims every
        // four-argument invocation for `format --check` and prints the help
        // text for the rest, so `rb bootstrap out --rollback` exited 1 having
        // said nothing at all, and `rb bootstrap out --compiler f.rb` fell
        // through to the usage arm. Both failures are silent — the help text
        // goes to stdout — which is the worst way for a release command to
        // fail.
        n if n >= 3 && args[1] == "bootstrap" => {
            if let Err(message) = bootstrap_args(&args[2..]) {
                eprintln!("Error: {}", message);
                process::exit(1);
            }
        }
        // `rb run <file> [args...]` — whatever follows the path belongs to the
        // program, and `sys.argv()` is how the program reads it. Without this a
        // Redblue program had no way to be told what to do: `rb run` took a path
        // and nothing else, so a compiler written in Redblue could not be given
        // a file to compile. A program that asks for no arguments is unaffected
        // — `sys.argv()` is then an empty list, as it is everywhere else.
        // `rb vm` takes its arguments the same way, for the same reason: the
        // next rung of the ladder is `rb vm stage1.rbc in.rb out.rbc`, and a
        // `rb vm` that dropped the arguments could not reach it. Before this
        // arm existed `rb vm a.rbc b c d` matched nothing, fell through to
        // `_ => print_help()` and exited 0 having run nothing at all.
        n if n >= 3 && (args[1] == "run" || args[1] == "vm") => {
            let path = args[2].clone();
            runtime::set_program_args(args[3..].to_vec());
            let outcome = if args[1] == "run" {
                run_file_with_diagnostic(&path)
            } else {
                vm_command(&path)
            };
            if let Err(rendered) = outcome {
                report(&rendered);
            }
        }
        4 => {
            let cmd = &args[1];
            if cmd == "format" && args[2] == "--check" {
                let path = &args[3];
                match fs::read_to_string(path) {
                    Ok(source) => match formatter::format(&source) {
                        Ok(formatted) => {
                            if formatter::needs_reformat(&source, &formatted) {
                                println!("File would be reformatted");
                                process::exit(1);
                            }
                        }
                        Err(e) => {
                            eprintln!("Format error: {}", e);
                            process::exit(1);
                        }
                    },
                    Err(e) => {
                        eprintln!("Error reading file: {}", e);
                        process::exit(1);
                    }
                }
            } else {
                print_help();
                process::exit(1);
            }
        }
        // Reaching here is a usage error — a command with arguments no arm
        // claimed — and it exits non-zero. It used to return normally, so the
        // help text was printed and the shell was told the command succeeded.
        _ => {
            print_help();
            process::exit(1);
        }
    }
}

/// `rb bootstrap [out-dir] [--compiler <file>] [--rollback]` — the S4 release
/// build.
///
/// The positional is the output directory and the compiler is a flag, which is
/// the same shape as `rb compile <file> -o <out>`: what is produced is named
/// last. With a positional in the compiler's place a user typing
/// `rb bootstrap target/out` — the natural spelling — had the directory read as
/// the compiler and the build died with "cannot read the compiler
/// target/out".
///
/// Three paths, and they are the whole of the ladder's operational story:
///
/// - the default builds stage 1 with the Rust frontend, stage 2 by running
///   stage 1 on the compiler's own source, checks the fixed point and writes
///   stage 2's bytes as the compiler the release carries. A stage 2 that is not
///   the fixed point is a non-zero exit and nothing shipped;
/// - `--rollback` cuts the release from the Rust frontend instead — `rb compile`
///   and nothing else — for a machine that cannot afford the self-compilation.
///   It writes the same bytes, so it is a rollback and not a second compiler.
/// - `--rollback` alone does *not* skip the self-compilation, because the
///   rollback path has to know the file it is rolling back to. The self-hosted
///   build runs first, and the frontend's bytes overwrite the shipped file only
///   once they are found to be the bytes that build verified; if the fixed point
///   does not hold, the build has already failed and there is nothing to roll
///   back to. That is the conservative order: a rollback that could be reached
///   from a broken fixed point would be a way to ship the broken thing quietly,
///   and one that wrote bytes the ladder never verified would be a way to ship
///   a second compiler under the first one's name.
fn bootstrap_args(rest: &[String]) -> Result<(), String> {
    let mut rollback = false;
    let mut compiler: Option<String> = None;
    let mut out_dir: Option<String> = None;
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--rollback" | "-r" => rollback = true,
            "--compiler" | "-c" => {
                index += 1;
                compiler = Some(
                    rest.get(index)
                        .ok_or_else(|| "--compiler needs a file".to_string())?
                        .clone(),
                );
            }
            other if other.starts_with("--compiler=") => {
                compiler = Some(other["--compiler=".len()..].to_string());
            }
            other if other.starts_with('-') => {
                return Err(format!("rb bootstrap does not take {other}"));
            }
            other => {
                if out_dir.is_some() {
                    return Err(format!(
                        "rb bootstrap takes one output directory, and {other} is a second"
                    ));
                }
                out_dir = Some(other.to_string());
            }
        }
        index += 1;
    }

    let compiler = compiler.unwrap_or_else(|| "bootstrap/compiler.rb".to_string());
    let out_dir = out_dir.unwrap_or_else(|| "target/bootstrap".to_string());
    let built = bootstrap_command(&compiler, &out_dir)?;

    if rollback {
        // The frontend's bytes, written only if `bootstrap::rollback_onto` finds
        // them to be the file the ladder just verified. Without that comparison
        // the write is unconditional: a `--rollback` whose bytes differ from the
        // fixed point — a frontend that has changed since the build, or one
        // that is not deterministic — would overwrite the shipped compiler and
        // print "Rolled back", so a release cut from it would carry a compiler
        // the ladder never approved. `bootstrap/build.sh` catches that with a
        // later `cmp`, but the CLI is what a person runs and it has to refuse
        // rather than ship and let a script find out.
        let bytes = bootstrap::rollback(Path::new(&compiler)).map_err(|e| e.to_string())?;
        bootstrap::rollback_onto(&built, Path::new(&out_dir), &bytes)
            .map_err(|error| error.to_string())?;
        let target = bootstrap::Build::shipped_path(Path::new(&out_dir));
        println!(
            "Rolled back to the Rust frontend: {} bytes written to {}",
            bytes.len(),
            target.display()
        );
    }
    Ok(())
}

/// The build itself, shared by `rb bootstrap` and the flag handling above.
///
/// Returns the [`bootstrap::Build`] so a caller that has more to do with the
/// result — the rollback path compares the frontend's bytes against it — is
/// working from the build's own bytes rather than from the file it wrote.
fn bootstrap_command(compiler: &str, out_dir: &str) -> Result<bootstrap::Build, String> {
    let built = bootstrap::build(Path::new(compiler), Path::new(out_dir))
        .map_err(|error| error.to_string())?;

    println!(
        "Fixed point holds: stage 1 and stage 2 are one file ({} bytes)",
        built.stage1.len()
    );
    println!(
        "Shipped compiler: {}",
        bootstrap::Build::shipped_path(Path::new(out_dir)).display()
    );
    Ok(built)
}

/// The `.rbc` path `rb compile <file>` writes when no `-o` is given: the
/// source path with its extension replaced.
fn bytecode_path_for(source: &str) -> String {
    match source.rfind('.') {
        Some(dot) if dot + 1 < source.len() => format!("{}.rbc", &source[..dot]),
        _ => format!("{source}.rbc"),
    }
}

/// `rb compile <file> [-o <out.rbc>]` — the frontend, then the encoder.
fn compile_command(source: &str, out: Option<&str>) -> Result<(), String> {
    let text = fs::read_to_string(source).map_err(|e| Error::Io(e.to_string()).to_string())?;
    let chunk = bytecode::compile_source(&text).map_err(|e| e.render(&text, Some(source)))?;
    let target = out
        .map(str::to_string)
        .unwrap_or_else(|| bytecode_path_for(source));

    fs::write(&target, chunk.encode()).map_err(|e| format!("Error: cannot write {target}: {e}"))?;

    println!("Compiled {source} -> {target}");
    Ok(())
}

/// `rb dis <file.rbc>` — the disassembler.
fn dis_command(path: &str) -> Result<(), String> {
    if !path.ends_with(".rbc") {
        return Err(format!(
            "Error: {path} is not a bytecode file; compile it first and disassemble the .rbc"
        ));
    }
    let bytes = fs::read(path).map_err(|e| Error::Io(e.to_string()).to_string())?;
    let chunk = bytecode::Chunk::decode(&bytes).map_err(|e| e.to_string())?;

    print!("{}", bytecode::disassemble(&chunk));
    Ok(())
}

/// `rb vm <file.rbc>` — the bytecode VM, which is bootstrap stage S1b.
///
/// A `.rbc` carries no source text, so a failure is reported with the same kind,
/// message and line the tree-walking VM gives, but without the source line and
/// caret `rb run` draws: a bytecode file has no text to draw them from. The
/// gap is in `phases/phase-019/FINDINGS.md`.
fn vm_command(path: &str) -> Result<(), String> {
    // The message carries no `Error: ` prefix of its own: both callers of this
    // function put one there, and one of them is now `report`, which adds it.
    if !path.ends_with(".rbc") {
        return Err(format!("{path} is not a bytecode file; compile it first"));
    }
    let bytes = fs::read(path).map_err(|e| Error::Io(e.to_string()).to_string())?;
    let chunk = bytecode::Chunk::decode(&bytes).map_err(|e| e.to_string())?;

    bytecode::vm::run(&chunk)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn print_help() {
    println!("Redblue v0.1.0 - A programming language as readable as plain English");
    println!();
    println!("Usage:");
    println!("  rb              Start interactive REPL");
    println!("  rb <file>      Run a Redblue file");
    println!("  rb run <file>  Run a Redblue file");
    println!("  rb test        Run all tests");
    println!("  rb test <file> Run specific test file");
    println!("  rb format <file>  Format a Redblue file");
    println!("  rb format --check <file>  Check if file needs formatting");
    println!("  rb lint <file>  Lint a Redblue file");
    println!("  rb compile <file> [-o out.rbc]  Compile to bytecode");
    println!("  rb dis <file.rbc>  Disassemble bytecode");
    println!("  rb vm <file.rbc>  Run bytecode");
    println!("  rb bootstrap [out-dir]  Build the release compiler with the self-hosted compiler");
    println!("  rb bootstrap [out-dir] --compiler <file>  Build a compiler other than bootstrap/compiler.rb");
    println!("  rb bootstrap [out-dir] --rollback  Cut the release from the Rust frontend instead");
    println!("  rb diagnostics <file>  Report errors as JSON for editors");
    println!("  rb grammar  Print the TextMate grammar for .rb files");
    println!("  rb keywords  Print the keyword list");
    println!("  rb help        Show this help");
    println!("  rb version     Show version");
    println!();
    println!("Example:");
    println!("  rb examples/hello.rb");
    println!("  rb format examples/hello.rb");
    println!("  rb lint examples/hello.rb");
    println!("  rb compile examples/hello.rb -o hello.rbc");
    println!("  rb dis hello.rbc");
    println!("  rb vm hello.rbc");
}

fn print_version() {
    println!("Redblue v0.1.0");
}
