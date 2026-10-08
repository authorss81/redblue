pub mod analyzer;
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
                _ => {
                    if let Err(rendered) = run_file_with_diagnostic(cmd) {
                        report(&rendered);
                    }
                }
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
