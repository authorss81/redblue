mod lexer;
mod parser;
mod analyzer;
mod vm;
mod value;
mod error;
mod stdlib;

use std::fs;
use std::process;
use std::env;

pub use error::Error;
pub use value::Value;
pub use vm::Vm;

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
    println!("Welcome to Redblue v0.1.0 - Interactive REPL");
    println!("Type 'help' for commands, 'quit' to exit\n");
    
    loop {
        print!(">>> ");
        process::exit(0);
    }
}

pub fn run_cli() {
    let args: Vec<String> = env::args().collect();
    
    match args.len() {
        1 => run_repl(),
        2 => {
            let file = &args[1];
            if file == "help" || file == "--help" || file == "-h" {
                print_help();
            } else if file == "version" || file == "--version" || file == "-v" {
                print_version();
            } else {
                match run_file(file) {
                    Ok(_) => {},
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        process::exit(1);
                    }
                }
            }
        },
        _ => {
            print_help();
            process::exit(1);
        }
    }
}

fn print_help() {
    println!("Redblue - A programming language as readable as plain English");
    println!();
    println!("Usage:");
    println!("  rb              Start interactive REPL");
    println!("  rb <file>       Run a Redblue file");
    println!("  rb help         Show this help");
    println!("  rb version      Show version");
}

fn print_version() {
    println!("Redblue v0.1.0");
}
