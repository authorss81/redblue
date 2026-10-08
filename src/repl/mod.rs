pub mod commands;
pub mod completer;
pub mod history;

use std::collections::HashMap;
use std::io::{self, Write};

pub use commands::ReplCommand;
pub use completer::ReplCompleter;
pub use history::ReplHistory;

use crate::analyzer;
use crate::interpreter::Vm;
use crate::lexer::Lexer;
use crate::parser::{open_block_depth, Parser, Program};
use crate::value::Value;

/// How many lines a session keeps before the oldest is dropped. A terminal
/// scrollback holds far more, so a session that outgrows this has already lost
/// anything the user would go looking for.
const HISTORY_LIMIT: usize = 1000;

/// The most lines a block may hold before the session stops waiting for its
/// `end`.
///
/// Nobody types five hundred lines into one block by accident, and a piped block
/// that is never closed — `printf 'repeat 3 times\n' | rb`, or a generator that
/// forgot its `end` — buffered without limit until the process ran out of memory.
/// Past this the session says the block is incomplete and starts over, which is
/// what the user would have to do by hand anyway.
const MAX_BLOCK_LINES: usize = 500;

/// The most bytes a block may hold, for the same reason and for the same inputs:
/// a block of a few very long lines gets nowhere near [`MAX_BLOCK_LINES`] and can
/// still be enormous.
const MAX_BLOCK_BYTES: usize = 64 * 1024;

/// How many lines of an unclosed block are echoed back when the session ends or
/// gives up. The rest are counted and reported, so the report itself cannot be
/// the thing that floods a terminal.
const BLOCK_REPORT_LINES: usize = 20;

/// The snippets `:help` prints under "Quick Reference", each one a complete
/// program the session has run.
///
/// Kept as source rather than as a hand-written sentence so the help cannot
/// promise something the interpreter does not do: `if x > 5 then` was printed for
/// years and is not Redblue, and `say "Hello, {{name}}!"` promised an
/// interpolation `say` does not perform.
const QUICK_REFERENCE: &[(&str, &str)] = &[
    ("Variable", "set x to 10"),
    ("Print", r#"say "Hello""#),
    ("Condition", "if x is greater than 5 then say \"big\" end"),
    ("Loop", "for each i from 1 to 3\n    say i\nend"),
];

/// The code `:example` shows: a caption, the program, and the text that program
/// prints.
///
/// Every entry has run in this session before it is printed here — the test at
/// the bottom of this module analyzes each source and asserts it is accepted —
/// so an example cannot promise a feature the interpreter does not have. The old
/// examples promised string interpolation: `say "Hello, {{name}}!"` printed
/// `Hello, {{name}}!`.
const EXAMPLES: &[(&str, &str)] = &[
    ("// Variables", "set name to \"World\"\nsay name"),
    (
        "// Print",
        "set name to \"World\"\nsay \"Hello, \" + name + \"!\"",
    ),
    ("// Math", "set x to (10 + 5) * 2\nsay x"),
    (
        "// Conditionals",
        "set age to 30\nif age is greater than 18 then\n    say \"Adult\"\nelse\n    say \"Minor\"\nend",
    ),
    (
        "// Loops",
        "repeat 3 times\n    say \"Hello!\"\nend",
    ),
    (
        "// Functions",
        "to greet(who)\n    say \"Hello, \" + who + \"!\"\nend\ngreet(\"World\")",
    ),
];

pub struct Repl {
    vm: Vm,
    variables: HashMap<String, Value>,
    history: ReplHistory,
    completer: ReplCompleter,
    running: bool,
    multiline_buffer: Vec<String>,
    in_multiline: bool,
}

impl Repl {
    pub fn new() -> Self {
        Self {
            vm: Vm::new(),
            variables: HashMap::new(),
            history: ReplHistory::new(HISTORY_LIMIT),
            completer: ReplCompleter::new(),
            running: true,
            multiline_buffer: Vec::new(),
            in_multiline: false,
        }
    }

    /// Completes `word` against the session: the language's own keywords and
    /// builtins, the REPL's commands, and every name this session has bound.
    pub fn complete(&self, word: &str) -> Vec<String> {
        self.completer.complete_with(word, &self.session_names())
    }

    /// Every name a completion could offer from the live session: the REPL's own
    /// variables, plus the VM's own bindings, which is where a `to` function and
    /// an `import`ed module's own `set` and `constant` declarations live.
    ///
    /// The `module_member` names a module's functions are published under are not
    /// here — see [`Vm::user_names`]. `MathUtils.circle_area` is what a program
    /// writes, and `MathUtils_circle_area` is only how the VM reaches it.
    fn session_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.variables.keys().cloned().collect();
        names.extend(self.vm.user_names());
        names.sort();
        names.dedup();
        names
    }

    /// Whether `code` is a program the user has not finished typing, and so wants
    /// the session to read another line before running anything.
    ///
    /// The question is asked of the tokens rather than of the last word of the
    /// line. Matching the last word got both directions wrong: `repeat 3 times`
    /// ends in `times` and `for each i from 1 to 10` ends in `10`, so real blocks
    /// were run half-written, while a stray `end` matched and captured every line
    /// after it.
    ///
    /// A line the lexer cannot read is treated as finished. It is a line the user
    /// is going to retype, and waiting for an `end` that will never balance it
    /// would hang the session on it.
    fn needs_continuation(code: &str) -> bool {
        match Lexer::tokenize(code) {
            Ok(tokens) => open_block_depth(&tokens) > 0,
            Err(_) => false,
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
            if io::stdout().flush().is_err() {
                break;
            }

            let mut input = String::new();
            match io::stdin().read_line(&mut input) {
                Ok(0) => {
                    self.report_unclosed_block();
                    break;
                }
                Ok(_) => {}
                Err(_) => break,
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

    /// Runs one command line, returning whether `input` was a command rather
    /// than Redblue source.
    ///
    /// The command table lives in [`ReplCommand::parse`]; this only dispatches
    /// what it produced. Two copies of the table meant an alias added to one was
    /// invisible to the other.
    fn handle_command(&mut self, input: &str) -> bool {
        match ReplCommand::parse(input) {
            ReplCommand::Quit => {
                self.running = false;
                true
            }
            ReplCommand::Help => {
                self.print_help();
                true
            }
            ReplCommand::Clear => {
                print!("\x1B[2J\x1B[H");
                let _ = io::stdout().flush();
                true
            }
            ReplCommand::History => {
                for i in 0..self.history.len() {
                    if let Some(entry) = self.history.get(i) {
                        println!("  {:4}  {}", i + 1, entry);
                    }
                }
                true
            }
            ReplCommand::Vars => {
                self.print_variables();
                true
            }
            ReplCommand::Functions => {
                self.print_functions();
                true
            }
            ReplCommand::Load(path) => {
                self.load_file(&path);
                true
            }
            ReplCommand::Save(path) => {
                self.save_session(&path);
                true
            }
            ReplCommand::Restore(path) => {
                self.restore_session(&path);
                true
            }
            ReplCommand::Reset => {
                self.reset_session();
                true
            }
            ReplCommand::Run(path) => {
                self.run_file(&path);
                true
            }
            ReplCommand::Debug => {
                self.debug_mode();
                true
            }
            ReplCommand::Inspect(name) => {
                self.inspect_value(&name);
                true
            }
            ReplCommand::Ast(expr) => {
                self.print_ast(&expr);
                true
            }
            ReplCommand::Tokens(expr) => {
                self.print_tokens(&expr);
                true
            }
            ReplCommand::Example => {
                self.show_examples();
                true
            }
            ReplCommand::NotACommand => false,
            ReplCommand::MissingArgument { command, usage } => {
                println!("Usage: :{} {}", command, usage);
                true
            }
            ReplCommand::Unknown(word) => {
                println!(
                    "Unknown command '{}'. Type :help for available commands.",
                    word
                );
                true
            }
        }
    }

    /// Buffers `input` and runs the buffer once it holds a whole program.
    ///
    /// A block the user opened and has not closed yet leaves the session in
    /// continuation mode; the buffer is run the moment it balances, and a line
    /// that leaves a block open is the only thing that opens continuation mode at
    /// all. That is what makes a block work whichever of its forms the user typed
    /// it in, and what stops a stray `end` from swallowing the rest of the
    /// session.
    fn execute_line(&mut self, input: &str) {
        self.multiline_buffer.push(input.to_string());
        let buffered = self.multiline_buffer.join("\n");
        if Self::exceeds_block_limits(&self.multiline_buffer, buffered.len()) {
            self.report_buffered_block_too_large();
            return;
        }
        if Self::needs_continuation(&buffered) {
            self.in_multiline = true;
            return;
        }

        let code = std::mem::take(&mut self.multiline_buffer).join("\n");
        self.in_multiline = false;
        self.execute_code(&code);
    }

    /// Forgets everything the session has done: its bindings, its last result,
    /// and any block it is halfway through typing.
    ///
    /// The block is the half that is easy to leave out. `:reset` used to replace
    /// the VM and clear `variables` and nothing else, so `repeat 3 times` then
    /// `:reset` left the session in continuation mode with that line still
    /// buffered: the next line was appended to a block nobody was typing any
    /// more, so `say "hi"` waited for an `end` that was never coming and never
    /// ran. A session that has been reset is a session at a prompt, so the buffer
    /// goes with the bindings.
    ///
    /// The history is left alone: it is the record of what was typed, and a reset
    /// is not a new session in the sense that the earlier lines were never typed.
    fn reset_session(&mut self) {
        self.vm = Vm::new();
        self.variables.clear();
        self.multiline_buffer.clear();
        self.in_multiline = false;
        println!("REPL state reset.");
    }

    /// Whether a buffer of `bytes` over `lines` is too much for a block the user
    /// is still typing.
    ///
    /// Both limits, because either alone leaves a hole: a block of a thousand
    /// blank lines is small in bytes, and one line of a megabyte is one line.
    fn exceeds_block_limits(lines: &[String], bytes: usize) -> bool {
        lines.len() > MAX_BLOCK_LINES || bytes > MAX_BLOCK_BYTES
    }

    /// Gives up on a block that has outgrown what a session will hold, saying so
    /// rather than buffering on.
    ///
    /// The buffer is only unbounded while `needs_continuation` stays true, which
    /// is forever for a piped block whose `end` was never written — or never
    /// typed, because the user walked away. Reporting is better than dying: the
    /// user finds out which block was dropped, and the session carries on reading
    /// the next line instead of holding the rest of the input in memory.
    fn report_buffered_block_too_large(&mut self) {
        let lines = self.multiline_buffer.len();
        let bytes: usize = self
            .multiline_buffer
            .iter()
            .map(|line| line.len() + 1)
            .sum();
        self.multiline_buffer.clear();
        self.in_multiline = false;
        println!(
            "Error: incomplete block: dropping a block of {} line(s) and {} byte(s) \
             that never balanced; a session holds at most {} lines or {} bytes.",
            lines, bytes, MAX_BLOCK_LINES, MAX_BLOCK_BYTES
        );
    }

    /// Reports a block the user opened and never closed, instead of dropping the
    /// lines of it on the floor at EOF.
    ///
    /// Discarding the buffer made an unfinished block and a finished one look the
    /// same from the outside: the session printed `Goodbye!` either way, so a
    /// block that never ran was indistinguishable from one that did.
    ///
    /// At most [`BLOCK_REPORT_LINES`] lines are echoed. The buffer is bounded now,
    /// so this cannot flood, but the count of what was left out is printed rather
    /// than the rest of it — a report is not the place to be the second problem.
    fn report_unclosed_block(&mut self) {
        if self.multiline_buffer.is_empty() {
            return;
        }

        let code = std::mem::take(&mut self.multiline_buffer).join("\n");
        self.in_multiline = false;
        let lines: Vec<&str> = code.lines().collect();
        println!(
            "Error: incomplete block: {} line(s) were never closed with 'end'.",
            lines.len()
        );
        for line in lines.iter().take(BLOCK_REPORT_LINES) {
            println!("  {}", line);
        }
        if lines.len() > BLOCK_REPORT_LINES {
            println!(
                "  ... and {} more line(s)",
                lines.len() - BLOCK_REPORT_LINES
            );
        }
    }

    fn execute_code(&mut self, code: &str) {
        let result = self.run_code(code);

        match result {
            Ok(value) => {
                if !matches!(value, Value::Nothing) {
                    println!("= {}", value);
                    self.variables.insert("_".to_string(), value);
                }
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
    }

    /// Runs one line as a program of its own, analyzed against the names the
    /// session has already bound.
    ///
    /// The analyzer is seeded from the session rather than started empty, which
    /// is the whole difference between a REPL and a program runner: the VM is
    /// reused across lines, so a `set`, a `to` or an `import` from an earlier line
    /// is still bound here, and without the seed the next line could not read it.
    /// `set x to 1` then `say x` was `AnalyzerError: Unknown variable 'x'` — the
    /// value was right there in the VM and the analyzer had never heard of the
    /// name.
    fn run_code(&mut self, code: &str) -> Result<Value, crate::error::Error> {
        let tokens = Lexer::tokenize(code)?;
        let mut parser = Parser::new(tokens);
        let program = parser.parse()?;
        let bound = self.session_names();
        analyzer::analyze_with_bound_names(&program, &bound)?;
        self.run_program(&program)
    }

    /// Runs `program` in this session's VM and prints what it printed.
    ///
    /// [`Vm::run`] prints the VM's whole output buffer at the end of a program and
    /// leaves the buffer alone, and that buffer belongs to the VM rather than to
    /// one line. Draining it first is what stops `say "A"` on the second line from
    /// coming back on every line after it, which made a session's output grow with
    /// the square of its length.
    ///
    /// Printing what is left on the error path is the other half: a program that
    /// fails half way through never reaches [`Vm::run`]'s print loop, so the `say`
    /// it had already run would otherwise be reported several lines later, by
    /// whichever program ran next.
    fn run_program(&mut self, program: &Program) -> Result<Value, crate::error::Error> {
        self.vm.take_output();
        let result = self.vm.run(program);
        if result.is_err() {
            for line in self.vm.take_output() {
                println!("{}", line);
            }
        }
        result
    }

    fn print_help(&self) {
        println!("Redblue REPL Commands:");
        println!();
        // Every line of this list comes from the one table the parser accepts
        // commands in, aliases included. It was written out separately, so `:q`,
        // `:h`, `:hist`, `:v`, `:f`, `:cls`, `:l`, `:s` and `:i` all worked and
        // none of them was documented.
        for line in ReplCommand::help_lines() {
            println!("{}", line);
        }
        println!();
        // The second sigil is parsed by [`ReplCommand::parse`] and completed by
        // the completer, so a table that lists only `:` leaves half of what the
        // REPL answers to unexplained. One sentence says it once rather than
        // printing every command twice.
        let alt = commands::ALT_SIGIL;
        println!(
            "  Every command also answers to '{alt}' in place of ':' \
             — {alt}quit is :quit."
        );
        println!();
        println!("Quick Reference:");
        for (caption, source) in QUICK_REFERENCE {
            println!("  // {}", caption);
            for line in source.lines() {
                println!("  {}", line);
            }
        }
        println!();
    }

    /// Every name the session has bound, with the value it holds, in name order.
    ///
    /// The REPL's own `variables` map is nearly empty — it holds `_`, the last
    /// result — because a `set`, a `to` and an `import` all bind in the VM, which
    /// is why `:vars` used to be able to print one line for a session that had
    /// bound a dozen names. The VM's bindings are read as well.
    ///
    /// Sorted because both maps are `HashMap`s and their iteration order is not a
    /// result: printing it directly gave one session a different listing from the
    /// next for the same bindings. Built here rather than printed inline so the
    /// order is something a test can read back.
    fn session_bindings(&self) -> Vec<(String, Value)> {
        let mut bindings: HashMap<String, Value> = self.variables.clone();
        for name in self.vm.user_names() {
            if let Some(value) = self.vm.user_value(&name) {
                // The REPL's own entry for a name wins: `_` is the last result,
                // and the VM holds a `Nothing` for it rather than the value.
                bindings.entry(name).or_insert_with(|| value.clone());
            }
        }

        let mut bindings: Vec<(String, Value)> = bindings.into_iter().collect();
        bindings.sort_by(|left, right| left.0.cmp(&right.0));
        bindings
    }

    /// The `:vars` listing, or `None` when the session holds nothing at all.
    fn variable_lines(&self) -> Option<Vec<String>> {
        let bindings = self.session_bindings();
        if bindings.is_empty() {
            return None;
        }

        Some(
            bindings
                .iter()
                .map(|(name, value)| format!("  {} = {}", name, value))
                .collect(),
        )
    }

    fn print_variables(&self) {
        let Some(lines) = self.variable_lines() else {
            println!("No variables defined.");
            return;
        };

        println!("Variables:");
        for line in lines {
            println!("{}", line);
        }
    }

    /// The `:funcs` listing: every function the session declared, in name order,
    /// or `None` when it has declared none.
    ///
    /// It used to print "User-defined functions are stored in the VM" and nothing
    /// else, which named no function and told the user nothing they could not read
    /// off the same line — and the test that ran `:funcs` asserted that sentence,
    /// so the missing listing could not fail anything.
    fn function_lines(&self) -> Option<Vec<String>> {
        let functions = self.vm.user_functions();
        if functions.is_empty() {
            return None;
        }

        Some(functions.iter().map(|name| format!("  {}", name)).collect())
    }

    fn print_functions(&self) {
        let Some(lines) = self.function_lines() else {
            println!("No functions defined.");
            return;
        };

        println!("Functions:");
        for line in lines {
            println!("{}", line);
        }
    }

    /// Reads the file at `path` and runs its source in this session's VM.
    ///
    /// `:load` runs, which is what `:help` promises: "Load and run a file". It
    /// used to read the file and print its size without running anything, so a
    /// session that loaded a file and carried on held none of the bindings the
    /// file declared.
    ///
    /// The source goes through [`Repl::execute_code`] rather than
    /// [`Repl::run_file`], so the bindings it makes are the session's own and
    /// stay available to the lines that follow — the difference from `:run`,
    /// which runs the file in a VM of its own and throws them away.
    fn load_file(&mut self, path: &str) {
        let source = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) => {
                println!("Could not load file: {}", e);
                return;
            }
        };

        println!("Loaded {} bytes from '{}'.", source.len(), path);
        self.execute_code(&source);
    }

    /// Writes the session's history to `path`, one entry per line, in the
    /// format [`ReplHistory::load_from_file`] reads back — and read back by
    /// [`Repl::restore_session`], which is what made the round trip one.
    fn save_session(&self, path: &str) {
        match self.history.save_to_file(path) {
            Ok(()) => println!(
                "Session saved to '{}' ({} line(s)).",
                path,
                self.history.len()
            ),
            Err(e) => println!("Could not save session: {}", e),
        }
    }

    /// Reads a history file written by [`Repl::save_session`] back into this
    /// session, and re-runs what it holds.
    ///
    /// `:save` had no other half: nothing in the REPL ever called
    /// [`ReplHistory::load_from_file`], so a saved session could not be loaded
    /// and the round trip was asserted only between two `ReplHistory`s in a unit
    /// test, never through the commands that write and read the file.
    ///
    /// The file replaces the history rather than being appended to it: `:save`
    /// writes the whole session, so restoring it into a session that has since
    /// typed more lines would give back two sessions at once.
    ///
    /// The lines are *run*, not only listed. A restore that only filled the
    /// history printed "Restored 2 line(s)" and left the session unable to read
    /// what those lines had bound: `set x to 1`, `:save`, a fresh session,
    /// `:restore`, `say x` was still `AnalyzerError: Unknown variable 'x'`. The
    /// lines were back and the state they had made was not, which is the half
    /// `:restore` was named for.
    fn restore_session(&mut self, path: &str) {
        let mut restored = ReplHistory::new(HISTORY_LIMIT);
        if let Err(e) = restored.load_from_file(path) {
            println!("Could not restore session: {}", e);
            return;
        }

        let lines: Vec<String> = (0..restored.len())
            .filter_map(|index| restored.get(index).cloned())
            .collect();
        let restored_lines = lines.len();
        self.history = restored;

        let (replayed, skipped) = self.replay(&lines);
        println!("Restored {} line(s) from '{}'", restored_lines, path);
        if restored_lines > 0 {
            println!(
                "Re-ran {} of them; {} REPL command(s) were listed and not re-run.",
                replayed, skipped
            );
        }
    }

    /// Runs `lines` in this session in the order they were typed, as the read
    /// loop would have run them, and reports how many were Redblue source and how
    /// many were REPL commands.
    ///
    /// The lines are run one at a time rather than as one program, so a line that
    /// fails is reported and the lines after it are still restored — and so a
    /// block that was typed over several lines comes back whole, through
    /// [`Repl::execute_line`]'s buffering.
    ///
    /// A line that was a command is *not* run. `:save`, `:vars` and `:history`
    /// are about the session rather than about its bindings, and the other four
    /// do what they say at the user: a restore that cleared the screen, wrote a
    /// file or quit the session would be doing something nobody asked for. The
    /// bindings a command made are the ones the replayed source makes again.
    fn replay(&mut self, lines: &[String]) -> (usize, usize) {
        let mut replayed = 0;
        let mut skipped = 0;
        for line in lines {
            if matches!(ReplCommand::parse(line), ReplCommand::NotACommand) {
                self.execute_line(line);
                replayed += 1;
            } else {
                skipped += 1;
            }
        }
        (replayed, skipped)
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

    /// The `:inspect` listing for `name`, or `None` when nothing in the session is
    /// bound to it.
    ///
    /// The lookup asks the VM as well as the REPL's own map. `set`, `to` and
    /// `import` all bind in the VM, and the REPL's map held only `_`, so
    /// `:inspect <session-name>` answered "Variable not found" for every name the
    /// session had actually bound — including one it had bound a line earlier.
    ///
    /// The type is the Redblue one, from [`Value::type_name`]: the name `type_of`
    /// reports and a `Runtime` error quotes. It used to be
    /// `std::any::type_name_of_val`, which is the *Rust* type of every value the
    /// VM holds — so `:inspect counter` answered `Type: "redblue::value::Value"`
    /// for a number, a text, a list and a function alike, and told the user
    /// nothing about any of them.
    ///
    /// Built here rather than printed inline so the wording is something a test
    /// can read back.
    fn inspect_lines(&self, name: &str) -> Option<Vec<String>> {
        let value = self.lookup(name)?;
        Some(vec![
            format!("{}: {}", name, value),
            format!("Type: {}", value.type_name()),
        ])
    }

    /// Prints what [`Repl::inspect_lines`] has to say about `name`.
    fn inspect_value(&self, name: &str) {
        match self.inspect_lines(name) {
            Some(lines) => {
                for line in lines {
                    println!("{}", line);
                }
            }
            None => {
                println!("Variable '{}' not found.", name);
            }
        }
    }

    /// The value bound to `name` anywhere in this session, VM included.
    fn lookup(&self, name: &str) -> Option<&Value> {
        self.variables
            .get(name)
            .or_else(|| self.vm.user_value(name))
    }

    fn print_ast(&self, code: &str) {
        match Lexer::tokenize(code) {
            Ok(tokens) => {
                let mut parser = Parser::new(tokens);
                match parser.parse() {
                    Ok(program) => {
                        println!("{:#?}", program);
                    }
                    Err(e) => {
                        println!("Parse error: {}", e);
                    }
                }
            }
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
            }
            Err(e) => {
                println!("Lexer error: {}", e);
            }
        }
    }

    /// Prints [`EXAMPLES`], which is the same list the tests at the bottom of
    /// this module run. Each one is code the session has run and the analyzer has
    /// accepted, so `:example` cannot advertise a feature that is not there — the
    /// old examples promised `say "Hello, {{name}}!"`, and `say` printed
    /// `Hello, {{name}}!`.
    fn show_examples(&self) {
        println!("Example Redblue Code:");
        println!();
        for (caption, source) in EXAMPLES {
            println!("  {}", caption);
            for line in source.lines() {
                println!("  {}", line);
            }
            println!();
        }
    }
}

impl Default for Repl {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::Repl;
    use crate::value::Value;

    /// Where the `.rb` file under test is written, inside the project checkout
    /// rather than in a user's temp directory.
    fn scratch_file(name: &str, source: &str) -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp/repl-load");
        std::fs::create_dir_all(&dir).expect("scratch directory should be creatable");
        let path = dir.join(format!("{}-{}.rb", name, serial));
        std::fs::write(&path, source).expect("the file should be writable");
        path
    }

    /// Where a non-`.rb` file under test — a history file — is written.
    fn scratch_path(name: &str, extension: &str) -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp/repl-session");
        std::fs::create_dir_all(&dir).expect("scratch directory should be creatable");
        let path = dir.join(format!("{}-{}.{}", name, serial, extension));
        std::fs::write(&path, "").expect("the file should be creatable");
        path
    }

    /// Feeds `lines` to the session one at a time, as the read loop does: each line
    /// is recorded in the history before it is dispatched, because that is the
    /// order the loop keeps them in.
    fn feed(repl: &mut Repl, lines: &[&str]) {
        for line in lines {
            repl.history.push((*line).to_string());
            if repl.handle_command(line) {
                continue;
            }
            repl.execute_line(line);
        }
    }

    /// Feeds `count` copies of `line` to a session, as a piped input would.
    fn feed_repeatedly(repl: &mut Repl, line: &str, count: usize) {
        for _ in 0..count {
            repl.history.push(line.to_string());
            repl.execute_line(line);
        }
    }

    #[test]
    fn handle_command_consumes_a_command_and_leaves_source_alone() {
        let mut repl = Repl::new();

        assert!(
            !repl.handle_command("2 + 3"),
            "Redblue source is not a command and must fall through to the VM"
        );
        assert!(repl.handle_command(":help"));
        assert!(
            repl.handle_command(":bogus"),
            "an unrecognised command is consumed and reported, not run as source"
        );
    }

    #[test]
    fn handle_command_of_quit_stops_the_session() {
        let mut repl = Repl::new();

        assert!(repl.handle_command(":quit"));
        assert!(!repl.running, "quit ends the read-eval-print loop");
        assert!(
            repl.handle_command(":help"),
            "a later command is still handled rather than panicking on a dead session"
        );
    }

    #[test]
    fn handle_command_reports_a_command_missing_its_argument() {
        let mut repl = Repl::new();

        assert!(
            repl.handle_command(":load"),
            "a command with nothing to load is reported, not run with an empty path"
        );
        assert!(repl.running, "and it does not end the session");
    }

    #[test]
    fn a_session_records_every_line_it_reads() {
        let mut repl = Repl::new();

        // The read loop records each line before dispatching it, so the history
        // holds both commands and source, in the order they were typed.
        for line in ["set x to 1", ":vars", "2 + 3"] {
            repl.history.push(line.to_string());
        }

        assert_eq!(repl.history.len(), 3);
        assert_eq!(repl.history.get(2).map(String::as_str), Some("2 + 3"));
        assert_eq!(repl.history.search(":"), Some(":vars".to_string()));
    }

    #[test]
    fn edge_a_line_repeated_in_a_row_is_one_entry_of_history() {
        let mut repl = Repl::new();

        repl.history.push(":vars".to_string());
        repl.history.push(":vars".to_string());

        assert_eq!(repl.history.len(), 1);
    }

    /// An empty line is not a line the user ran, so it is not history.
    #[test]
    fn edge_empty_input_is_not_recorded_and_runs_nothing() {
        let mut repl = Repl::new();

        assert!(!repl.handle_command("   "));
        assert!(!repl.handle_command(""));

        assert_eq!(repl.history.len(), 0, "a blank line leaves no trace");
        assert!(repl.running);
    }

    #[test]
    fn complete_offers_the_names_the_session_has_bound() {
        let mut repl = Repl::new();
        repl.execute_code("set total to 3");

        let matches = repl.complete("to");
        assert!(
            matches.contains(&"total".to_string()),
            "a variable the session bound is completable: {:?}",
            matches
        );
        assert!(
            matches.contains(&"to".to_string()),
            "the keyword of the same name is offered too: {:?}",
            matches
        );
    }

    #[test]
    fn complete_offers_nothing_for_a_prefix_the_session_has_no_name_for() {
        let repl = Repl::new();

        assert!(
            repl.complete("zqx").is_empty(),
            "a prefix nothing starts with completes to nothing"
        );
    }

    #[test]
    fn edge_completion_after_reset_forgets_the_sessions_names() {
        let mut repl = Repl::new();
        repl.execute_code("set counter to 3");
        assert!(repl.complete("count").contains(&"counter".to_string()));

        repl.handle_command(":reset");

        assert!(
            !repl.complete("count").contains(&"counter".to_string()),
            "reset drops the bindings, so they cannot still be completed"
        );
    }

    /// A reset session is a session at a prompt. It used to replace the VM and
    /// clear the last result and leave the block buffer alone, so a `repeat` the
    /// user had opened but not closed was still half-typed after the reset: the
    /// next line was appended to it, and nothing ran until some `end` arrived that
    /// the user had no reason to type.
    #[test]
    fn edge_reset_drops_a_block_the_session_had_not_finished_typing() {
        let mut repl = Repl::new();

        feed(&mut repl, &["repeat 3 times"]);
        assert!(repl.in_multiline, "the block is open before the reset");

        assert!(repl.handle_command(":reset"));

        assert!(
            repl.multiline_buffer.is_empty(),
            "the buffer is gone: {:?}",
            repl.multiline_buffer
        );
        assert!(
            !repl.in_multiline,
            "and the session is back at a prompt rather than waiting for an end"
        );

        repl.execute_line(r#"say "hi""#);

        assert_eq!(
            repl.lookup("_"),
            None,
            "the line ran on its own, so the block it would have joined did not"
        );
    }

    /// `:help` says `:load <file>` "Load and run a file", so `:load` runs it and
    /// what the file declared becomes the session's own. It used to read the file
    /// and print its size, leaving the session holding nothing the file declared.
    #[test]
    fn load_runs_the_file_so_its_declarations_become_the_sessions_own() {
        let path = scratch_file("load-runs", "to shout(word)\n    say word\nend\n");
        let mut repl = Repl::new();

        assert!(repl.handle_command(&format!(":load {}", path.display())));
        assert!(
            repl.complete("shout").contains(&"shout".to_string()),
            "a function the loaded file declared is the session's to offer: {:?}",
            repl.complete("shout")
        );

        let _ = std::fs::remove_file(&path);
    }

    /// `:load` and `:run` differ in where the file's bindings land: `:load` binds
    /// them into this session, `:run` runs the file in a VM of its own. Before the
    /// fix neither did — `:load` read the file and ran nothing.
    #[test]
    fn load_keeps_a_value_the_file_bound_where_run_does_not() {
        let path = scratch_file("load-keeps", "set loaded_value to 7\n");

        let mut run_only = Repl::new();
        run_only.handle_command(&format!(":run {}", path.display()));
        assert!(
            !run_only
                .complete("loaded_v")
                .contains(&"loaded_value".to_string()),
            "a file run with :run binds into its own VM, not this session"
        );

        let mut loaded = Repl::new();
        loaded.handle_command(&format!(":load {}", path.display()));
        assert!(
            loaded
                .complete("loaded_v")
                .contains(&"loaded_value".to_string()),
            "a file loaded with :load binds into the session: {:?}",
            loaded.complete("loaded_v")
        );

        let _ = std::fs::remove_file(&path);
    }

    /// `:vars` is a listing, and a listing whose order changes from one session
    /// to the next for the same bindings cannot be read or compared. `variables`
    /// is a `HashMap`, so the names are sorted rather than printed in whatever
    /// order the map hands them over.
    #[test]
    fn edge_vars_lists_every_name_in_a_fixed_order() {
        let mut repl = Repl::new();

        assert_eq!(repl.variable_lines(), None, "a session with nothing bound");

        for name in ["zebra", "apple", "mango", "_"] {
            repl.variables.insert(name.to_string(), Value::Number(1.0));
        }

        assert_eq!(
            repl.variable_lines(),
            Some(vec![
                "  _ = 1".to_string(),
                "  apple = 1".to_string(),
                "  mango = 1".to_string(),
                "  zebra = 1".to_string(),
            ]),
            "the listing is in name order however the map was filled"
        );
    }

    /// A block opens continuation mode and closes it when it balances. These are
    /// the three openers the old last-word heuristic missed — `repeat 3 times`
    /// ends in `times`, `to greet(name)` in `)` and `for each i from 1 to 10` in
    /// `10` — so each of them used to run half-written and report a parse error.
    #[test]
    fn edge_every_block_form_asks_for_its_continuation_line() {
        for opener in [
            "if 1 is greater than 0 then",
            "repeat 3 times",
            "to greet(name)",
            "for each i from 1 to 10",
            "try",
            "while 1 is less than 10",
            "unless 1 is nothing then",
        ] {
            let mut repl = Repl::new();
            repl.execute_line(opener);

            assert!(
                repl.in_multiline,
                "{:?} opens a block, so the session keeps reading",
                opener
            );

            repl.execute_line("end");

            assert!(!repl.in_multiline, "{:?} is closed by its 'end'", opener);
            assert!(
                repl.multiline_buffer.is_empty(),
                "{:?} leaves nothing buffered once it has run",
                opener
            );
        }
    }

    /// A `to` binds into a local scope rather than into the globals, so scanning
    /// the globals alone never offered a function the session had just defined —
    /// the name a user is most likely to be reaching for next.
    #[test]
    fn complete_offers_a_function_the_session_defined() {
        let mut repl = Repl::new();

        feed(&mut repl, &["to shout(word)", "    say word", "end"]);

        let matches = repl.complete("shout");
        assert!(
            matches.contains(&"shout".to_string()),
            "a `to` the session defined is completable: {:?}",
            matches
        );
    }

    /// An `import` publishes a module's functions in the VM under the
    /// `MathUtils_circle_area` name a call through the module resolves to. That name
    /// is how the VM reaches the function, not one the program wrote, so it must not
    /// reach completion, `:vars` or `:funcs` — the module's own name is what a
    /// session offers instead. The call itself has to keep working, so this asserts
    /// both halves.
    #[test]
    fn edge_an_imported_modules_internal_names_are_not_session_names() {
        let mut repl = Repl::new();

        feed(&mut repl, &["import MathUtils"]);
        assert!(
            repl.run_code("MathUtils.circle_area(2)").is_ok(),
            "the name the module is reached by still calls through"
        );

        let names = repl.session_names();
        for name in &names {
            assert!(
                !name.contains("MathUtils_"),
                "{:?} is the VM's own name for a module member, not the session's: {:?}",
                name,
                names
            );
        }
        assert!(
            names.contains(&"MathUtils".to_string()) && names.contains(&"TAU".to_string()),
            "what the module binds under its own names is still the session's: {:?}",
            names
        );
        assert!(
            !repl
                .complete("MathUtils_")
                .contains(&"MathUtils_circle_area".to_string()),
            "and the internal name is not completable: {:?}",
            repl.complete("MathUtils_")
        );
        assert_eq!(
            repl.function_lines(),
            None,
            "a module's functions are reached as MathUtils.circle_area, so :funcs has \
         nothing of the session's own to list"
        );
    }

    /// A block the whole user typed in one line needs no continuation line, and a
    /// REPL that asked for one anyway would sit waiting for an `end` the user
    /// never typed.
    #[test]
    fn edge_a_block_typed_on_one_line_does_not_wait_for_an_end() {
        let mut repl = Repl::new();

        repl.execute_line("if 1 is greater than 0 then say \"yes\" end");

        assert!(
            !repl.in_multiline,
            "the block was already closed by the time the line ended"
        );
        assert!(repl.multiline_buffer.is_empty());
    }

    /// A stray `end` opens nothing. Matching the last word of a line made it open
    /// a block, and every line after it was buffered into that block and never run
    /// or reported.
    #[test]
    fn edge_a_stray_end_does_not_capture_the_lines_after_it() {
        let mut repl = Repl::new();

        repl.execute_line("end");
        assert!(
            !repl.in_multiline,
            "an 'end' that closes nothing opens nothing"
        );

        repl.execute_line("2 + 3");

        assert_eq!(
            repl.variables.get("_"),
            Some(&Value::Number(5.0)),
            "the line after a stray 'end' ran instead of being buffered"
        );
    }

    /// A block keyword inside a string is text. Matching the line's last word got
    /// this wrong too, in both directions.
    #[test]
    fn edge_a_block_keyword_inside_a_string_is_not_a_block() {
        let mut repl = Repl::new();

        repl.execute_line(r#"say "end""#);

        assert!(
            !repl.in_multiline,
            "a quoted 'end' neither closes nor opens a block"
        );
        assert!(repl.multiline_buffer.is_empty());
    }

    /// A line the lexer refuses is a line the user is going to retype. Waiting for
    /// an `end` that will never balance it would hang the session on it forever.
    #[test]
    fn edge_a_line_that_cannot_be_lexed_does_not_wait_for_an_end() {
        let mut repl = Repl::new();

        repl.execute_line("say \"unclosed");

        assert!(
            !repl.in_multiline,
            "an unlexable line is reported, not buffered forever"
        );
        assert!(repl.multiline_buffer.is_empty());
    }

    /// The two `to`s that are not function declarations are mid-statement. A
    /// detector that counted every `to` would hold the session open for an `end`
    /// the user has no reason to type, and a loop would then need a second one.
    #[test]
    fn edge_a_range_bound_or_an_alias_is_not_a_function_declaration() {
        let mut looped = Repl::new();
        looped.execute_line("for each i from 1 to 10");
        assert!(
            looped.in_multiline,
            "the loop itself is a block, whatever its range bound says"
        );
        looped.execute_line("end");
        assert!(
            !looped.in_multiline,
            "one 'end' closes the loop, so the range bound's 'to' is not a second block"
        );

        for line in ["import files to F", "set x to 10", r#"say "to""#] {
            let mut repl = Repl::new();
            repl.execute_line(line);

            assert!(
                !repl.in_multiline,
                "{:?} is a whole statement, not an unclosed body",
                line
            );
        }
    }

    /// EOF with a block still open ends the session, and says why. Dropping the
    /// buffer made an unfinished block indistinguishable from a finished one: the
    /// session printed `Goodbye!` either way.
    #[test]
    fn edge_eof_with_an_unclosed_block_reports_it_and_lets_the_buffer_go() {
        let mut repl = Repl::new();

        feed(
            &mut repl,
            &["if 1 is greater than 0 then", "    say \"yes\""],
        );
        assert!(repl.in_multiline);

        repl.report_unclosed_block();

        assert!(
            repl.multiline_buffer.is_empty(),
            "the unclosed lines are reported, not silently dropped"
        );
        assert!(
            !repl.in_multiline,
            "and the session is not left waiting for an end that will not come"
        );
    }

    /// Nothing buffered is nothing to report, so EOF after a complete program is
    /// a clean goodbye rather than a complaint about an empty block.
    #[test]
    fn edge_eof_with_nothing_buffered_is_not_an_incomplete_block() {
        let mut repl = Repl::new();

        repl.execute_line("2 + 3");
        repl.report_unclosed_block();

        assert!(!repl.in_multiline);
        assert!(repl.multiline_buffer.is_empty());
    }

    /// A REPL has to carry state from one line to the next, and it could not: the
    /// VM was reused so the *value* survived, but each line was analyzed against
    /// an empty scope, so `set x to 1` then `say x` was
    /// `AnalyzerError: Unknown variable 'x'`. Every other test here avoided a
    /// cross-line read, which is why the suite was green.
    #[test]
    fn a_name_bound_on_one_line_is_readable_by_the_next() {
        let mut repl = Repl::new();

        feed(
            &mut repl,
            &[
                "set x to 41",
                "set y to x + 1",
                "if y is greater than 40 then",
                "    say \"CARRYING\"",
                "end",
            ],
        );

        assert!(
            repl.complete("x").contains(&"x".to_string()),
            "the name is the session's: {:?}",
            repl.complete("x")
        );
        assert!(
            !repl.in_multiline,
            "the block that reads a bound name ran instead of being reported"
        );
    }

    /// The same across a `to`, which is the case a name list is most likely to
    /// miss: a function binds through `declare` + `set_var`, and a line that only
    /// calls it names nothing.
    #[test]
    fn a_function_defined_on_one_line_is_callable_from_the_next() {
        let mut repl = Repl::new();

        feed(
            &mut repl,
            &[
                "to shout(word)",
                "    say word",
                "end",
                r#"shout("CALLED")"#,
            ],
        );

        assert!(
            repl.complete("shout").contains(&"shout".to_string()),
            "the function is the session's: {:?}",
            repl.complete("shout")
        );
        assert_eq!(
            repl.lookup("shout").map(|value| value.to_string()),
            Some("<function shout>".to_string()),
            "and it is callable from the line that follows the declaration"
        );
    }

    /// `:funcs` is a listing. It used to print "User-defined functions are stored
    /// in the VM" and name none, so a test that ran it could not tell a working
    /// listing from a missing one.
    #[test]
    fn funcs_lists_the_functions_the_session_declared() {
        let mut repl = Repl::new();
        assert_eq!(repl.function_lines(), None, "a session with no functions");

        feed(
            &mut repl,
            &[
                "to shout(word)",
                "    say word",
                "end",
                "to whisper(word)",
                "    say word",
                "end",
            ],
        );

        assert_eq!(
            repl.function_lines(),
            Some(vec!["  shout".to_string(), "  whisper".to_string()]),
            "both functions, in name order"
        );
    }

    /// A variable a `set` bound is a name of the session, so it is in `:vars` and
    /// in completion. `:vars` read only the REPL's own map, which held `_` and
    /// nothing else, so a session that had bound a dozen names listed one.
    #[test]
    fn edge_vars_lists_the_names_the_session_has_bound() {
        let mut repl = Repl::new();

        feed(&mut repl, &["set zebra to 1", "set apple to 2"]);

        let lines = repl.variable_lines().expect("the session bound two names");
        assert!(
            lines.contains(&"  zebra = 1".to_string())
                && lines.contains(&"  apple = 2".to_string()),
            "a `set` the session made is one of its variables: {:?}",
            lines
        );
    }

    /// `:inspect` looks in the VM, not only in the REPL's own map. `set`, `to` and
    /// `import` all bind there, so it reported "Variable not found" for every name
    /// a session had actually bound — including one bound a line earlier.
    #[test]
    fn edge_inspect_finds_a_name_the_session_bound() {
        let mut repl = Repl::new();

        assert_eq!(repl.lookup("nothing_here"), None);

        feed(&mut repl, &["set counter to 7"]);

        assert_eq!(
            repl.lookup("counter").map(|value| value.to_string()),
            Some("7".to_string()),
            "the value a `set` bound is the session's to inspect"
        );
        assert!(
            repl.handle_command(":inspect counter"),
            "and the command that reads it runs"
        );
        assert_eq!(
            repl.lookup("nothing_here"),
            None,
            "a name nothing bound is still absent"
        );
    }

    /// `:inspect` reports the Redblue type of what it found, which is the name
    /// `type_of` reports and a `Runtime` error quotes. It used to report
    /// `std::any::type_name_of_val`, the *Rust* type of the value — so a number,
    /// a text, a list and a function were all `redblue::value::Value` and the
    /// line said the same thing about every value in the session.
    #[test]
    fn edge_inspect_reports_the_redblue_type_of_the_value() {
        let mut repl = Repl::new();

        feed(
            &mut repl,
            &[
                "set counter to 7",
                "set words to [\"a\", \"b\"]",
                "set who to \"ann\"",
                "set answer to yes",
                "to greet(who)",
                "    return who",
                "end",
            ],
        );

        for (name, expected) in [
            ("counter", "number"),
            ("words", "list"),
            ("who", "text"),
            ("answer", "yes/no"),
            ("greet", "function"),
        ] {
            let value = repl
                .lookup(name)
                .unwrap_or_else(|| panic!("the session bound {name:?}"));
            let lines = repl
                .inspect_lines(name)
                .unwrap_or_else(|| panic!("{name:?} is the session's to inspect"));
            assert_eq!(
                lines,
                vec![format!("{name}: {value}"), format!("Type: {expected}")],
                "{name:?} holds {value} and :inspect reports it as a {expected}"
            );
        }

        assert_eq!(
            repl.inspect_lines("nosuchthing"),
            None,
            "a name nothing bound has no lines to print"
        );
    }

    /// `:save` and `:restore` are the same file, and a restore has to bring back
    /// the *state*, not only the text of the session. It used to put the lines
    /// back in the history and nothing else, so `set x to 1`, `:save`, a fresh
    /// session and `:restore` left `say x` an `AnalyzerError: Unknown variable
    /// 'x'` — the lines were back and what they had bound was not.
    #[test]
    fn save_and_restore_round_trip_the_session_through_the_repl() {
        let path = scratch_path("session", "txt");
        let mut saved = Repl::new();
        feed(&mut saved, &["set x to 1", "say \"saved\""]);
        assert!(saved.handle_command(&format!(":save {}", path.display())));
        assert_eq!(saved.history.len(), 2, "both lines were recorded");

        let mut restored = Repl::new();
        assert!(restored.handle_command(&format!(":restore {}", path.display())));

        assert_eq!(
            restored.history.len(),
            2,
            "the restored session holds the lines that were saved"
        );
        assert_eq!(
            restored.history.get(0).map(String::as_str),
            Some("set x to 1")
        );
        assert_eq!(
            restored.history.search("say"),
            Some("say \"saved\"".to_string()),
            "and they are the lines the user can reach again"
        );
        assert_eq!(
            restored.lookup("x").map(|value| value.to_string()),
            Some("1".to_string()),
            "and the name they bound is the session's to read on the next line"
        );
        assert!(
            restored.run_code("say x").is_ok(),
            "so a restored session can say what it restored"
        );

        let _ = std::fs::remove_file(&path);
    }

    /// A restore runs the lines it read back, and a block the session had typed
    /// over several lines comes back whole — through the same buffering the read
    /// loop uses, not as one line of source.
    #[test]
    fn edge_a_restored_block_runs_as_the_block_it_was() {
        let path = scratch_path("block-session", "txt");
        let mut saved = Repl::new();
        feed(
            &mut saved,
            &["to shout(word)", "    say word", "end", "shout(\"SAVED\")"],
        );
        assert!(saved.handle_command(&format!(":save {}", path.display())));

        let mut restored = Repl::new();
        assert!(restored.handle_command(&format!(":restore {}", path.display())));

        assert!(
            restored.run_code("shout(\"RESTORED\")").is_ok(),
            "the function the saved lines declared is the restored session's to call"
        );
        assert!(
            !restored.in_multiline,
            "and the replayed block closed itself: {:?}",
            restored.multiline_buffer
        );

        let _ = std::fs::remove_file(&path);
    }

    /// A line of the saved file that was a command is listed again and not run
    /// again. `:restore` used to end the session it was restoring into, because
    /// the saved file held the `:quit` that wrote it — a command that quits is
    /// not something a restore replays.
    #[test]
    fn edge_a_restored_command_is_listed_and_not_run_again() {
        let path = scratch_path("command-session", "txt");
        let mut saved = Repl::new();
        feed(&mut saved, &["set x to 1", ":vars"]);
        assert!(saved.handle_command(&format!(":save {}", path.display())));

        let mut restored = Repl::new();
        assert!(restored.handle_command(&format!(":restore {}", path.display())));

        assert!(
            restored.running,
            "the session is still there after the restore"
        );
        assert_eq!(
            restored.history.len(),
            2,
            "both lines are back, the command among them"
        );
        assert_eq!(
            restored.lookup("x").map(|value| value.to_string()),
            Some("1".to_string()),
            "and the source among them ran, which is the binding it made"
        );

        let _ = std::fs::remove_file(&path);
    }

    /// A restore of a file that is not there is a failure the user is told about,
    /// and the history it was going to replace is left alone.
    #[test]
    fn edge_restore_of_a_missing_file_leaves_the_history_alone() {
        let path = scratch_path("never-saved", "txt");
        let _ = std::fs::remove_file(&path);
        assert!(
            !path.exists(),
            "the path under test must not exist for this to mean anything"
        );

        let mut repl = Repl::new();
        feed(&mut repl, &["set x to 1"]);

        assert!(repl.handle_command(&format!(":restore {}", path.display())));

        assert_eq!(
            repl.history.len(),
            1,
            "a restore that failed did not throw the session's own history away"
        );
    }

    /// A block the user opened and never closed buffered without limit, so a piped
    /// block whose `end` never arrived grew until the process ran out of memory —
    /// and `report_unclosed_block` then printed all of it. Past the limit the
    /// session drops the block, says so, and goes back to reading lines.
    #[test]
    fn edge_a_block_that_outgrows_the_buffer_is_dropped_not_kept() {
        let mut repl = Repl::new();
        repl.execute_line("repeat 3 times");

        feed_repeatedly(&mut repl, r#"    say "x""#, super::MAX_BLOCK_LINES + 10);

        assert!(
            repl.multiline_buffer.is_empty(),
            "the block is dropped rather than buffered without limit"
        );
        assert!(
            !repl.in_multiline,
            "and the session goes back to reading whole lines"
        );

        repl.execute_line("2 + 3");
        assert_eq!(
            repl.lookup("_").map(|value| value.to_string()),
            Some("5".to_string()),
            "the session carries on after giving up on the block"
        );
    }

    /// A block can be huge in a handful of lines, so the byte limit is not implied
    /// by the line limit.
    #[test]
    fn edge_a_block_of_few_very_long_lines_is_dropped_too() {
        let mut repl = Repl::new();
        repl.execute_line("repeat 3 times");

        let long = format!("say \"{}\"", "a".repeat(super::MAX_BLOCK_BYTES / 4));
        feed_repeatedly(&mut repl, &long, 8);

        assert!(
            repl.multiline_buffer.is_empty(),
            "{} bytes of block is more than a session holds",
            super::MAX_BLOCK_BYTES
        );
        assert!(!repl.in_multiline);
    }

    /// The report of an unclosed block echoes the lines back so the user can see
    /// what was lost — but not all of them, or the report is the second problem.
    /// The count of what was left out is printed instead.
    #[test]
    fn edge_the_report_of_an_unclosed_block_echoes_only_some_of_it() {
        let mut repl = Repl::new();
        repl.execute_line("repeat 3 times");

        let body = super::BLOCK_REPORT_LINES + 5;
        let lines: Vec<String> = (0..body)
            .map(|index| format!("    say \"{}\"", index))
            .collect();
        feed(
            &mut repl,
            &lines.iter().map(String::as_str).collect::<Vec<_>>(),
        );

        repl.report_unclosed_block();

        assert!(
            repl.multiline_buffer.is_empty(),
            "and the buffer is released either way"
        );
        assert!(
            repl.history.len() <= body + 1,
            "the session kept the lines it read"
        );
    }

    /// Everything `:help` shows under "Quick Reference" is a program this session
    /// can run. It was a hand-written list, and `if x > 5 then` on it is not
    /// Redblue — a user who typed what the help showed got a parse error.
    #[test]
    fn edge_every_quick_reference_snippet_is_a_program_the_session_runs() {
        let mut repl = Repl::new();

        for (caption, source) in super::QUICK_REFERENCE {
            let outcome = repl.run_code(source);
            assert!(
                outcome.is_ok(),
                "the {:?} example {:?} is Redblue: {:?}",
                caption,
                source,
                outcome.err()
            );
        }
    }

    /// `:example` is the same list, and it used to promise an interpolation `say`
    /// does not perform: `say "Hello, {{name}}!"` printed `Hello, {{name}}!`. Each
    /// entry is run here, so an example cannot be shown unless the session can run
    /// it.
    #[test]
    fn edge_every_example_is_a_program_the_session_runs() {
        let mut repl = Repl::new();

        for (caption, source) in super::EXAMPLES {
            let outcome = repl.run_code(source);
            assert!(
                outcome.is_ok(),
                "the {:?} example is Redblue: {:?}",
                caption,
                outcome.err()
            );
            assert!(
                !source.contains("{{") && !source.contains("}}"),
                "the {:?} example does not promise an interpolation `say` does not do",
                caption
            );
        }
    }
}
