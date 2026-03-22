mod lexer;
mod parser;
mod analyzer;
mod vm;
mod value;
mod error;
pub mod stdlib;
pub mod repl;
pub mod testing;
pub mod formatter;
pub mod linter;

pub use error::Error;
pub use value::Value;
pub use vm::Vm;
pub use lexer;
pub use parser;
pub use formatter;
pub use linter;

pub fn run_file(path: &str) -> Result<(), Error> {
    let source = fs::read_to_string(path)
        .map_err(|e| Error::Io(e.to_string()))?;
    
    run_source(&source)
}

pub fn run_source(source: &str) -> Result<(), Error> {
    let tokens = lexer::tokenize(source)?;
    let ast = parser::parse(tokens)?;
    analyzer::analyze(&ast)?;
    
    let mut vm = Vm::new();
    vm.run(&ast)?;
    
    Ok(())
}

pub fn run_repl() {
    let mut repl = repl::Repl::new();
    repl.run();
}

pub fn run_test(path: Option<&str>) -> Result<(), Error> {
    let harness = testing::TestHarness::new();
    
    match path {
        Some(p) => harness.run_file(p)?,
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
                },
                _ => {
                    if cmd.ends_with(".rb") || cmd.ends_with(".redblue") {
                        if let Err(e) = run_file(cmd) {
                            eprintln!("Error: {}", e);
                            process::exit(1);
                        }
                    } else {
                        if let Err(e) = run_file(cmd) {
                            eprintln!("Error: {}", e);
                            process::exit(1);
                        }
                    }
                }
            }
        },
        3 => {
            let cmd = &args[1];
            let path = &args[2];
            match cmd.as_str() {
                "test" => {
                    if let Err(e) = run_test(Some(path)) {
                        eprintln!("Error: {}", e);
                        process::exit(1);
                    }
                },
                "run" => {
                    if let Err(e) = run_file(path) {
                        eprintln!("Error: {}", e);
                        process::exit(1);
                    }
                },
                "format" => {
                    match std::fs::read_to_string(path) {
                        Ok(source) => {
                            match formatter::format(&source) {
                                Ok(formatted) => print!("{}", formatted),
                                Err(e) => {
                                    eprintln!("Format error: {}", e);
                                    process::exit(1);
                                }
                            }
                        },
                        Err(e) => {
                            eprintln!("Error reading file: {}", e);
                            process::exit(1);
                        }
                    }
                },
                "lint" => {
                    match std::fs::read_to_string(path) {
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
                        },
                        Err(e) => {
                            eprintln!("Error reading file: {}", e);
                            process::exit(1);
                        }
                    }
                },
                _ => {
                    print_help();
                    process::exit(1);
                }
            }
        },
        4 => {
            let cmd = &args[1];
            if cmd == "format" && args[2] == "--check" {
                let path = &args[3];
                match std::fs::read_to_string(path) {
                    Ok(source) => {
                        match formatter::format(&source) {
                            Ok(formatted) => {
                                if source.trim() != formatted.trim() {
                                    println!("File would be reformatted");
                                    process::exit(1);
                                }
                            },
                            Err(e) => {
                                eprintln!("Format error: {}", e);
                                process::exit(1);
                            }
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
        },
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
