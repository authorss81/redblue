mod analyzer;
mod error;
pub mod formatter;
pub mod lexer;
pub mod linter;
pub mod parser;
pub mod repl;
pub mod stdlib;
pub mod testing;
mod value;
mod vm;

use std::env;
use std::fs;
use std::process;

use crate::lexer::Lexer;
use testing::reporter::Reporter;

pub use error::{Error, Span};
// `FunctionValue` is re-exported because it is the payload of the public
// `Value::Function` variant: a caller that matches that variant has to be able
// to name the type it binds.
pub use value::{FunctionValue, Value};
pub use vm::{
    resolve_max_iterations, resolve_max_iterations_from, resolve_max_steps, resolve_max_steps_from,
    run_isolated, Vm, MAX_CALL_DEPTH, MAX_CALL_DEPTH_ENV, MAX_ITERATIONS, MAX_ITERATIONS_ENV,
    MAX_STEPS, MAX_STEPS_ENV,
};

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

    let (_vm, result) = vm::run_isolated(&ast);
    result.map(|_| ())
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
                        for warning in &warnings {
                            eprintln!("Warning: {}", warning.message);
                        }
                        for error in &errors {
                            eprintln!("Error: {}", error.message);
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
                _ => {
                    print_help();
                    process::exit(1);
                }
            }
        }
        4 => {
            let cmd = &args[1];
            if cmd == "format" && args[2] == "--check" {
                let path = &args[3];
                match fs::read_to_string(path) {
                    Ok(source) => match formatter::format(&source) {
                        Ok(formatted) => {
                            if source.trim() != formatted.trim() {
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
        _ => print_help(),
    }
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
    println!("  rb help        Show this help");
    println!("  rb version     Show version");
    println!();
    println!("Example:");
    println!("  rb examples/hello.rb");
    println!("  rb format examples/hello.rb");
    println!("  rb lint examples/hello.rb");
}

fn print_version() {
    println!("Redblue v0.1.0");
}
