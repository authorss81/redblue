pub mod commands;
pub mod completer;
pub mod history;

use std::io::{self, Write};
use std::collections::HashMap;

pub use commands::ReplCommand;
pub use completer::ReplCompleter;
pub use history::ReplHistory;

use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::analyzer;
use crate::vm::Vm;
use crate::value::Value;

pub struct Repl {
    vm: Vm,
    variables: HashMap<String, Value>,
    history: Vec<String>,
    running: bool,
    multiline_buffer: Vec<String>,
    in_multiline: bool,
}

impl Repl {
    pub fn new() -> Self {
        Self {
            vm: Vm::new(),
            variables: HashMap::new(),
            history: Vec::new(),
            running: true,
            multiline_buffer: Vec::new(),
            in_multiline: false,
        }
    }
    
    pub fn run(&mut self) {
        println!("Welcome to Redblue v0.1.0 - Interactive REPL");
        println!("Type 'help' for commands, 'quit' to exit\n");
        
        loop {
            if !self.running {
                break;
            }
            
            let prompt = if self.in_multiline { "..  " } else { ">>> " };
            
            print!("{}", prompt);
            if let Err(_) = io::stdout().flush() {
                break;
            }
            
            let mut input = String::new();
            if let Err(_) = io::stdin().read_line(&mut input) {
                break;
            }
            
            let input = input.trim();
            if input.is_empty() {
                continue;
            }
            
            self.history.push(input.to_string());
            
            if self.handle_command(input) {
                continue;
            }
            
            self.execute_line(input);
        }
        
        println!("Goodbye!");
    }
    
    fn handle_command(&mut self, input: &str) -> bool {
        if !input.starts_with(':') && !input.starts_with('.') {
            return false;
        }
        
        let parts: Vec<&str> = input.split_whitespace().collect();
        let cmd = parts[0].trim_start_matches(':');
        
        match cmd {
            "quit" | "exit" | "q" => {
                self.running = false;
                true
            },
            "help" | "h" | "?" => {
                self.print_help();
                true
            },
            "clear" | "cls" => {
                print!("\x1B[2J\x1B[H");
                io::stdout().flush().ok();
                true
            },
            "history" | "hist" | "h" => {
                for (i, cmd) in self.history.iter().enumerate() {
                    println!("  {:4}  {}", i + 1, cmd);
                }
                true
            },
            "vars" | "variables" | "v" => {
                self.print_variables();
                true
            },
            "functions" | "funcs" | "f" => {
                self.print_functions();
                true
            },
            "load" | "l" => {
                if parts.len() > 1 {
                    self.load_file(parts[1]);
                } else {
                    println!("Usage: :load <filename>");
                }
                true
            },
            "save" | "s" => {
                if parts.len() > 1 {
                    self.save_session(parts[1]);
                } else {
                    println!("Usage: :save <filename>");
                }
                true
            },
            "reset" => {
                self.vm = Vm::new();
                self.variables.clear();
                println!("REPL state reset.");
                true
            },
            "run" => {
                if parts.len() > 1 {
                    self.run_file(parts[1]);
                } else {
                    println!("Usage: :run <filename>");
                }
                true
            },
            "debug" => {
                self.debug_mode();
                true
            },
            "inspect" | "i" => {
                if parts.len() > 1 {
                    self.inspect_value(parts[1]);
                } else {
                    println!("Usage: :inspect <variable>");
                }
                true
            },
            "ast" => {
                if parts.len() > 1 {
                    self.print_ast(parts[1]);
                } else {
                    println!("Usage: :ast <expression>");
                }
                true
            },
            "tokens" => {
                if parts.len() > 1 {
                    self.print_tokens(parts[1]);
                } else {
                    println!("Usage: :tokens <expression>");
                }
                true
            },
            "example" | "examples" => {
                self.show_examples();
                true
            },
            _ => {
                println!("Unknown command '{}'. Type :help for available commands.", cmd);
                true
            },
        }
    }
    
    fn execute_line(&mut self, input: &str) {
        if self.in_multiline {
            if input == "end" || input == "end." {
                self.multiline_buffer.push(input.to_string());
                let code = self.multiline_buffer.join("\n");
                self.execute_code(&code);
                self.multiline_buffer.clear();
                self.in_multiline = false;
            } else {
                self.multiline_buffer.push(input.to_string());
            }
            return;
        }
        
        let needs_multiline = matches!(
            input,
            s if s.ends_with("then") 
               || s.ends_with("end")
               || s.ends_with("to")
               || s.ends_with("else")
               || s.ends_with("while")
               || s.ends_with("repeat")
        );
        
        if needs_multiline {
            self.multiline_buffer.push(input.to_string());
            self.in_multiline = true;
            return;
        }
        
        self.execute_code(input);
    }
    
    fn execute_code(&mut self, code: &str) {
        let result = self.run_code(code);
        
        match result {
            Ok(value) => {
                if !matches!(value, Value::Nothing) {
                    println!("= {}", value);
                    self.variables.insert("_".to_string(), value);
                }
            },
            Err(e) => {
                println!("Error: {}", e);
            }
        }
    }
    
    fn run_code(&mut self, code: &str) -> Result<Value, crate::error::Error> {
        let tokens = Lexer::tokenize(code)?;
        let mut parser = Parser::new(tokens);
        let program = parser.parse()?;
        analyzer::analyze(&program)?;
        self.vm.run(&program)
    }
    
    fn print_help(&self) {
        println!("Redblue REPL Commands:");
        println!();
        println!("  :help, :h          Show this help");
        println!("  :quit, :q          Exit the REPL");
        println!("  :clear             Clear the screen");
        println!("  :history           Show command history");
        println!("  :vars              Show all variables");
        println!("  :funcs             Show defined functions");
        println!("  :load <file>       Load and run a file");
        println!("  :save <file>       Save current session");
        println!("  :reset             Reset REPL state");
        println!("  :run <file>        Run a Redblue file");
        println!("  :inspect <var>     Inspect a variable");
        println!("  :ast <expr>        Print AST for expression");
        println!("  :tokens <expr>     Print tokens for expression");
        println!("  :example           Show example code");
        println!();
        println!("Quick Reference:");
        println!("  set x to 10        // Variable");
        println!("  say \"Hello\"        // Print");
        println!("  if x > 5 then       // Condition");
        println!("  for each i from 1 to 10  // Loop");
        println!();
    }
    
    fn print_variables(&self) {
        if self.variables.is_empty() {
            println!("No variables defined.");
            return;
        }
        
        println!("Variables:");
        for (name, value) in &self.variables {
            println!("  {} = {}", name, value);
        }
    }
    
    fn print_functions(&self) {
        println!("User-defined functions are stored in the VM.");
        println!("Use :inspect <name> to see function details.");
    }
    
    fn load_file(&self, path: &str) {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                println!("Loaded {} bytes from '{}'", content.len(), path);
            },
            Err(e) => {
                println!("Could not load file: {}", e);
            }
        }
    }
    
    fn save_session(&self, path: &str) {
        let content = self.history.join("\n");
        match std::fs::write(path, content) {
            Ok(_) => println!("Session saved to '{}'", path),
            Err(e) => println!("Could not save session: {}", e),
        }
    }
    
    fn run_file(&self, path: &str) {
        match crate::run_file(path) {
            Ok(_) => println!("File '{}' executed successfully.", path),
            Err(e) => println!("Error running file: {}", e),
        }
    }
    
    fn debug_mode(&self) {
        println!("Debug mode - Not implemented yet");
        println!("Press Ctrl+C to exit debug mode");
    }
    
    fn inspect_value(&self, name: &str) {
        match self.variables.get(name) {
            Some(value) => {
                println!("{}: {}", name, value);
                println!("Type: {:?}", std::mem::type_name_of_val(value));
            },
            None => {
                println!("Variable '{}' not found.", name);
            }
        }
    }
    
    fn print_ast(&self, code: &str) {
        match Lexer::tokenize(code) {
            Ok(tokens) => {
                let mut parser = Parser::new(tokens);
                match parser.parse() {
                    Ok(program) => {
                        println!("{:#?}", program);
                    },
                    Err(e) => {
                        println!("Parse error: {}", e);
                    }
                }
            },
            Err(e) => {
                println!("Lexer error: {}", e);
            }
        }
    }
    
    fn print_tokens(&self, code: &str) {
        match Lexer::tokenize(code) {
            Ok(tokens) => {
                for token in tokens {
                    println!("{:?}", token);
                }
            },
            Err(e) => {
                println!("Lexer error: {}", e);
            }
        }
    }
    
    fn show_examples(&self) {
        println!("Example Redblue Code:");
        println!();
        println!("  // Variables");
        println!("  set name to \"World\"");
        println!("  set age to 30");
        println!();
        println!("  // Print");
        println!("  say \"Hello, {name}!\"");
        println!();
        println!("  // Math");
        println!("  set x to (10 + 5) * 2");
        println!();
        println!("  // Conditionals");
        println!("  if age is greater than 18");
        println!("      say \"Adult\"");
        println!("  else");
        println!("      say \"Minor\"");
        println!("  end");
        println!();
        println!("  // Loops");
        println!("  repeat 3 times");
        println!("      say \"Hello!\"");
        println!("  end");
        println!();
        println!("  // Functions");
        println!("  to greet(name)");
        println!("      say \"Hello, {name}!\"");
        println!("  end");
    }
}

impl Default for Repl {
    fn default() -> Self {
        Self::new()
    }
}
