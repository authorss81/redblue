//! One program, run on both engines, compared as one value.
//!
//! The corpus compares two things about a run: the lines it printed and the
//! value it was worth. Both are recorded, because a program that prints the
//! right lines and ends with the wrong value is the shape of bug this harness
//! exists to find, and a comparison of printed output alone cannot see it.
//!
//! `Typed` is the harness's own value type rather than `redblue::Value` for two
//! reasons: `Value` carries an `f64` and so cannot be compared for equality in a
//! test without a float comparison that can be fudged, and `Value::Object`
//! renders as `<object>` — two different objects are indistinguishable. Here a
//! record's keys are sorted, so a value that two VMs built in different orders
//! still compares equal, and an object names itself.

use std::fmt;

use redblue::bytecode::vm::BytecodeVm;
use redblue::bytecode::Chunk;
use redblue::{compile_source, run_isolated, Error, Value};

/// A value, with its type, comparable and printable.
#[derive(Debug, Clone, PartialEq)]
pub enum Typed {
    Nothing,
    Number(f64),
    Text(String),
    YesNo(bool),
    List(Vec<Typed>),
    /// Sorted by key, so insertion order cannot make two equal records look
    /// different.
    Record(Vec<(String, Typed)>),
    /// An object, a function or a builtin: named, not rendered, so two of them
    /// can be told apart.
    Opaque(String),
}

impl Typed {
    /// The harness's reading of a [`Value`].
    pub fn of(value: &Value) -> Typed {
        match value {
            Value::Nothing => Typed::Nothing,
            Value::Number(n) => Typed::Number(*n),
            Value::Text(s) => Typed::Text(s.clone()),
            Value::YesNo(b) => Typed::YesNo(*b),
            Value::List(items) => Typed::List(items.iter().map(Typed::of).collect()),
            Value::Record(fields) => {
                let mut entries: Vec<(String, Typed)> = fields
                    .iter()
                    .map(|(k, v)| (k.clone(), Typed::of(v)))
                    .collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                Typed::Record(entries)
            }
            Value::Object(name, _) => Typed::Opaque(format!("Object {name}")),
            Value::Function(f) => Typed::Opaque(format!("Function {}", f.name)),
            Value::Builtin(name) => Typed::Opaque(format!("Builtin {name}")),
        }
    }

    /// The type's own name, as the golden format records it.
    pub fn type_name(&self) -> &'static str {
        match self {
            Typed::Nothing => "nothing",
            Typed::Number(_) => "number",
            Typed::Text(_) => "text",
            Typed::YesNo(_) => "yesno",
            Typed::List(_) => "list",
            Typed::Record(_) => "record",
            Typed::Opaque(_) => "opaque",
        }
    }

    /// How the value prints, with no type: the half of `#value` a reader sees.
    pub fn render(&self) -> String {
        match self {
            Typed::Nothing => "nothing".to_string(),
            Typed::Number(n) => render_number(*n),
            Typed::Text(s) => s.clone(),
            Typed::YesNo(b) => if *b { "yes" } else { "no" }.to_string(),
            Typed::List(items) => {
                // Elements are **tagged**, so a list of `number:1` reads back as
                // numbers and a list of `text:1` as texts. A bare rendering
                // would be ambiguous between them.
                let rendered: Vec<String> = items.iter().map(Typed::tagged).collect();
                format!("[{}]", rendered.join(", "))
            }
            Typed::Record(entries) => {
                let rendered: Vec<String> = entries
                    .iter()
                    .map(|(k, v)| format!("{k}: {}", v.tagged()))
                    .collect();
                format!("{{{}}}", rendered.join(", "))
            }
            Typed::Opaque(name) => name.clone(),
        }
    }

    /// How the golden format records it: the type and the rendering.
    pub fn tagged(&self) -> String {
        match self {
            Typed::Nothing => "nothing".to_string(),
            other => format!("{}:{}", other.type_name(), other.render()),
        }
    }

    /// Reads back what [`Typed::tagged`] wrote, refusing a rendering that does
    /// not name a type.
    pub fn parse(tagged: &str) -> Option<Typed> {
        if tagged == "nothing" {
            return Some(Typed::Nothing);
        }
        let (kind, rest) = tagged.split_once(':')?;
        Some(match kind {
            "number" => Typed::Number(rest.parse().ok()?),
            "text" => Typed::Text(rest.to_string()),
            "yesno" => match rest {
                "yes" => Typed::YesNo(true),
                "no" => Typed::YesNo(false),
                _ => return None,
            },
            "list" => {
                let inner = rest.strip_prefix('[')?.strip_suffix(']')?;
                let mut items = Vec::new();
                for part in split_top(inner) {
                    items.push(Typed::parse(part)?);
                }
                Typed::List(items)
            }
            "record" => {
                let inner = rest.strip_prefix('{')?.strip_suffix('}')?;
                let mut entries = Vec::new();
                for part in split_top(inner) {
                    let (key, value) = part.split_once(": ")?;
                    entries.push((key.to_string(), Typed::parse(value)?));
                }
                Typed::Record(entries)
            }
            "opaque" => Typed::Opaque(rest.to_string()),
            _ => return None,
        })
    }
}

/// Splits on the commas that are *not* inside a nested `[…]` or `{…}`, so a
/// list of records parses back as records rather than as pieces.
fn split_top(inner: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in inner.char_indices() {
        match c {
            '[' | '{' => depth += 1,
            ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(inner[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = inner[start..].trim();
    if !tail.is_empty() || !parts.is_empty() {
        parts.push(tail);
    }
    parts.into_iter().filter(|p| !p.is_empty()).collect()
}

/// The same rendering `Value`'s `Display` gives a number, written out so the
/// golden format does not depend on a `Display` impl changing under it.
fn render_number(n: f64) -> String {
    const MAX_EXACT_INT: f64 = 9_007_199_254_740_992.0; // 2^53
    if n.is_nan() {
        "not a number".to_string()
    } else if n.is_infinite() {
        if n > 0.0 { "infinity" } else { "-infinity" }.to_string()
    } else if n.fract() == 0.0 && n.abs() <= MAX_EXACT_INT {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// What one engine made of a program.
#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    /// The public `Error::label()` and its message, unescaped.
    Label {
        label: String,
        message: String,
        /// `None` for a failure with no position at all.
        position: Option<(usize, usize)>,
        /// Whether the engine that reported this could name the **column** of the
        /// statement that failed, or only its line.
        ///
        /// This is a property of the engine that ran the program, not of the
        /// label it happened to report, and it is recorded **per failure** rather
        /// than inferred. A `.rbc` instruction carries a line and no column, so
        /// the bytecode VM places a runtime failure at the start of the line its
        /// instruction is on while the tree-walking VM places it at the column of
        /// the failing statement. A blanket "a runtime failure's column does not
        /// count" rule hid a column that had *moved* on either engine; see
        /// [`same_place`], which asserts the line-granular convention instead of
        /// excusing it.
        column_exact: bool,
    },
}

impl Failure {
    /// Where the failure happened, if it happened somewhere.
    pub fn position(&self) -> Option<(usize, usize)> {
        match self {
            Failure::Label { position, .. } => *position,
        }
    }

    /// The failure's message on its own.
    pub fn message(&self) -> &str {
        match self {
            Failure::Label { message, .. } => message,
        }
    }

    /// Whether this failure's column is the column of the statement that failed.
    pub fn column_exact(&self) -> bool {
        match self {
            Failure::Label { column_exact, .. } => *column_exact,
        }
    }

    /// The same failure reported at `column`.
    ///
    /// This is how a test asks the question the limit exists for — *"would the
    /// comparison notice if this column moved?"* — over the corpus rather than
    /// over a fixture, so a corpus that cannot fail that question says so.
    pub fn with_column(&self, column: usize) -> Failure {
        match self {
            Failure::Label {
                label,
                message,
                position,
                column_exact,
            } => Failure::Label {
                label: label.clone(),
                message: message.clone(),
                position: position.map(|(line, _)| (line, column)),
                column_exact: *column_exact,
            },
        }
    }
}

/// What one engine made of a program: what it printed, and either what it was
/// worth or how it failed.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub output: Vec<String>,
    pub result: Result<Typed, Failure>,
}

impl Outcome {
    /// A program that never started: nothing was printed and something failed.
    pub fn failed(failure: Failure) -> Outcome {
        Outcome {
            output: Vec::new(),
            result: Err(failure),
        }
    }

    /// The failure's label, if the program failed.
    pub fn label(&self) -> Option<&str> {
        match &self.result {
            Err(Failure::Label { label, .. }) => Some(label.as_str()),
            _ => None,
        }
    }

    /// Whether two engines said exactly the same thing: same lines, and the same
    /// value or the same failure including the position *and* whether that
    /// position could name a column.
    ///
    /// Equality, not agreement — see [`Outcome::agrees_within_format_limits`] for
    /// the comparison a harness makes across two engines.
    pub fn agrees_with(&self, other: &Outcome) -> bool {
        self == other
    }

    /// The same verdict, with the position compared as far as the two formats
    /// can carry it; see [`same_place`].
    ///
    /// This is the comparison a caller makes across engines. It is not
    /// [`Outcome::agrees_with`], which is equality: two engines can say the same
    /// thing while one of them could not name a column, and a harness that asked
    /// for equality there would report a format limit as a divergence.
    pub fn agrees_within_format_limits(&self, other: &Outcome) -> bool {
        self.output == other.output
            && match (&self.result, &other.result) {
                (Ok(a), Ok(b)) => a == b,
                (Err(a), Err(b)) => same_place(a, b) && a.message() == b.message(),
                _ => false,
            }
    }
}

fn failure_of(error: &Error, column_exact: bool) -> Failure {
    Failure::Label {
        label: error.label().to_string(),
        message: error.message().to_string(),
        position: error.span().map(|s| (s.line, s.column)),
        column_exact,
    }
}

/// Runs `source` on the tree-walking VM, which is the specification.
///
/// The pipeline runs **once**. A caller that recovered the value by running it a
/// second time would be measuring two runs and calling the difference a
/// verdict; `edge_one_run_of_a_program_is_all_the_harness_asks_for` runs a
/// program that appends to a file and holds the harness to it.
pub fn tree_walk(source: &str) -> Outcome {
    let tokens = match redblue::lexer::Lexer::tokenize(source) {
        Ok(tokens) => tokens,
        Err(error) => return Outcome::failed(failure_of(&error, true)),
    };
    let ast = match redblue::parser::parse(tokens) {
        Ok(ast) => ast,
        Err(error) => return Outcome::failed(failure_of(&error, true)),
    };
    if let Err(error) = redblue::analyzer::analyze(&ast) {
        return Outcome::failed(failure_of(&error, true));
    }
    let (mut vm, result) = run_isolated(&ast);
    let output = vm.take_output();
    Outcome {
        output,
        result: result
            .map(|v| Typed::of(&v))
            .map_err(|e| failure_of(&e, true)),
    }
}

/// Runs `source` through the bytecode compiler and then the bytecode VM.
///
/// A source the compiler refuses is reported with the same [`Failure::Label`]
/// the tree-walking VM would have given, not with a separate "the compiler
/// said so" channel: a caller comparing the two engines wants to know that both
/// refused the program, not which half of the pipeline refused it.
pub fn bytecode(source: &str) -> Outcome {
    match compile_source(source) {
        Ok(chunk) => run_chunk(&chunk),
        // A refusal of the compiler's own is exact: the compiler has the source it
        // refused in front of it.
        Err(error) => Outcome::failed(failure_of(&error, true)),
    }
}

/// Runs an already-compiled chunk on the bytecode VM.
pub fn run_chunk(chunk: &Chunk) -> Outcome {
    let mut vm = BytecodeVm::new();
    // The lines are taken, not read off stdout: a test run that interleaved a
    // program's own output with the failure report is a test run nobody can read.
    vm.set_echo(false);
    let result = vm.run(chunk);
    let output = vm.take_output();
    Outcome {
        output,
        result: result
            .map(|v| Typed::of(&v))
            // A compiled instruction carries a line and no column, so every
            // failure raised while running one is placed at the start of its line.
            // Recorded per failure so `same_place` can hold this engine to that
            // convention rather than to nothing at all.
            .map_err(|e| failure_of(&e, false)),
    }
}

/// Whether two engines place the same failure in the same place.
///
/// The **line** is always comparable and always compared. The **column** is
/// compared wherever both engines can carry one, and where only one can the other
/// is held to the convention its format imposes — see
/// [`Failure::column_exact`] and [`same_column`]. The limit is per failure and
/// recorded in the outcome, not inferred from the failure's label: "every
/// `RuntimeError` ignores its column" widened the gate over 43 corpus programs,
/// which is exactly how a column that moved on one engine or the other became
/// invisible.
pub fn same_place(a: &Failure, b: &Failure) -> bool {
    match (a.position(), b.position()) {
        (Some((line_a, column_a)), Some((line_b, column_b))) => {
            line_a == line_b && same_column(a, b, column_a, column_b)
        }
        (None, None) => true,
        _ => false,
    }
}

/// Whether two reported columns may be the same column.
///
/// Both engines can name the column: they have to agree on it. Where one cannot
/// — a runtime failure on the bytecode VM, whose instruction carries a line and
/// no column — that engine's column has to be the start of the line, which is
/// what `Span::new(line, 1)` is. Asserting the convention is what makes the limit
/// a limit: a column that is anything else is a change in the interpreter, not a
/// property of the format.
fn same_column(a: &Failure, b: &Failure, column_a: usize, column_b: usize) -> bool {
    match (a.column_exact(), b.column_exact()) {
        // Both engines can name the column: it has to be the same column.
        (true, true) => column_a == column_b,
        // Neither can: both report the start of the line they are on.
        (false, false) => column_a == 1 && column_b == 1,
        // One can and one cannot. The engine that cannot is held to the
        // convention, which is what keeps the limit a limit; the engine that can
        // is left to the golden file, which records its column for the corpus.
        (false, true) => column_a == 1,
        (true, false) => column_b == 1,
    }
}

/// Renders a failure for a report or a golden file.
pub fn show(failure: &Failure) -> String {
    match failure {
        Failure::Label {
            label,
            message,
            position,
            ..
        } => match position {
            Some((line, column)) => format!("{label}: {message} at {line}:{column}"),
            None => format!("{label}: {message}"),
        },
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", show(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_a_number_and_the_text_that_spells_it_are_different_outcomes() {
        // `5` and `"5"` print identically, so a comparison that only looked at
        // output could not tell them apart. This is the whole reason a golden
        // file records a type as well as a rendering — at the top level and
        // inside a container alike.
        assert_eq!(Typed::Number(5.0).render(), "5");
        assert_eq!(Typed::Text("5".to_string()).render(), "5");
        assert_ne!(Typed::Number(5.0), Typed::Text("5".to_string()));

        assert_eq!(Typed::Nothing.render(), "nothing");
        assert_eq!(Typed::Text("nothing".to_string()).render(), "nothing");
        assert_ne!(Typed::Nothing, Typed::Text("nothing".to_string()));

        // The same collision inside a list. `Value`'s own `Display` renders both
        // as `[5]`, which is the collision the golden format's tagging removes.
        let numbers = Typed::List(vec![Typed::Number(5.0)]);
        let texts = Typed::List(vec![Typed::Text("5".to_string())]);
        assert_eq!(
            Value::list(vec![Value::Number(5.0)]).to_string(),
            Value::list(vec![Value::Text("5".to_string())]).to_string(),
        );
        assert_ne!(numbers, texts);
        assert_eq!(numbers.tagged(), "list:[number:5]");
        assert_eq!(texts.tagged(), "list:[text:5]");
    }

    #[test]
    fn edge_the_tagged_form_names_the_type_for_every_kind() {
        for (typed, expected) in [
            (Typed::Nothing, "nothing"),
            (Typed::Number(5.0), "number:5"),
            (Typed::Text("hi".to_string()), "text:hi"),
            (Typed::YesNo(true), "yesno:yes"),
            (Typed::YesNo(false), "yesno:no"),
            (
                Typed::List(vec![Typed::Number(1.0), Typed::Number(2.0)]),
                "list:[number:1, number:2]",
            ),
            (
                Typed::List(vec![Typed::Text("1".to_string()), Typed::Number(1.0)]),
                "list:[text:1, number:1]",
            ),
            (
                Typed::Record(vec![("a".to_string(), Typed::Number(1.0))]),
                "record:{a: number:1}",
            ),
            (
                Typed::Record(vec![("a".to_string(), Typed::Nothing)]),
                "record:{a: nothing}",
            ),
            (Typed::Opaque("Object A".to_string()), "opaque:Object A"),
        ] {
            assert_eq!(typed.tagged(), expected);
            assert_eq!(
                Typed::parse(expected),
                Some(typed),
                "reading {expected} back"
            );
        }
    }

    #[test]
    fn edge_an_untagged_value_is_rejected_for_a_typed_one() {
        // The golden format's `#value 5` names a rendering and not a type. It
        // parses as nothing at all rather than as a number, because guessing
        // would let a corpus silently stop recording types.
        assert_eq!(Typed::parse("5"), None);
        assert_eq!(Typed::parse("yes"), None);
        assert_eq!(Typed::parse("nothing"), Some(Typed::Nothing));
        assert_eq!(Typed::parse("number:not-a-number"), None);
        assert_eq!(Typed::parse("yesno:maybe"), None);
        assert_eq!(Typed::parse("list:[1, 2"), None);
    }

    #[test]
    fn edge_a_nested_value_round_trips_through_its_tagged_form() {
        let typed = Typed::Record(vec![
            (
                "xs".to_string(),
                Typed::List(vec![Typed::Number(1.0), Typed::Text("a".into())]),
            ),
            (
                "inner".to_string(),
                Typed::Record(vec![("k".to_string(), Typed::YesNo(false))]),
            ),
            ("empty".to_string(), Typed::List(vec![])),
            ("none".to_string(), Typed::Nothing),
        ]);
        assert_eq!(Typed::parse(&typed.tagged()), Some(typed));
    }

    #[test]
    fn edge_a_number_renders_the_way_the_interpreter_prints_it() {
        assert_eq!(render_number(5.0), "5");
        assert_eq!(render_number(-0.0), "0");
        assert_eq!(render_number(0.5), "0.5");
        assert_eq!(render_number(1.0 / 3.0), "0.3333333333333333");
        assert_eq!(render_number(9_007_199_254_740_992.0), "9007199254740992");
        assert_eq!(render_number(f64::INFINITY), "infinity");
        assert_eq!(render_number(f64::NEG_INFINITY), "-infinity");
        assert_eq!(render_number(f64::NAN), "not a number");
    }

    #[test]
    fn edge_a_failure_carries_the_position_it_happened_at() {
        let failure = tree_walk("set xs to [1, 2]\nsay xs[9]\n");
        match failure.result {
            Err(Failure::Label {
                ref label,
                ref message,
                position,
                column_exact,
            }) => {
                assert_eq!(label, "RuntimeError");
                assert!(
                    message.starts_with("Index 9 is out of bounds"),
                    "unexpected message {message}",
                );
                assert_eq!(position, Some((2, 1)), "the line the failure happened on");
                assert!(
                    column_exact,
                    "the tree-walking VM walks the source, so it can name the column",
                );
            }
            other => panic!("expected a labelled failure, got {other:?}"),
        }
    }

    #[test]
    fn edge_a_frontend_failure_printed_nothing_on_either_vm() {
        for source in ["say \"unterminated", "set x to", "say 1\nend"] {
            let tree = tree_walk(source);
            let byte = bytecode(source);
            assert!(
                tree.output.is_empty() && byte.output.is_empty(),
                "{source:?}: a program that never ran must have printed nothing, \
                 tree={:?} bytecode={:?}",
                tree.output,
                byte.output,
            );
        }
    }

    #[test]
    fn edge_the_two_engines_agree_on_a_failure_and_on_what_was_printed_first() {
        let source = "say \"before\"\nset xs to [1]\nsay xs[7]\n";
        let tree = tree_walk(source);
        let byte = bytecode(source);
        assert_eq!(
            tree.output,
            vec!["before".to_string()],
            "the harness must keep what ran before the failure, or a program that \
             prints and then fails is recorded as having printed nothing",
        );
        // Two engines, so the format-limited comparison: they report the same
        // failure at the same line, and only one of them can name the column.
        assert!(
            tree.agrees_within_format_limits(&byte),
            "tree={tree:?} bytecode={byte:?}",
        );
        // Running one engine twice is equality, and it is equality on both halves
        // of the outcome — the flag that says which columns are comparable
        // included, so a fixture cannot pass by having the two engines answer
        // differently in a way the harness does not look at.
        assert!(tree.agrees_with(&tree_walk(source)));
    }

    #[test]
    fn edge_one_run_of_a_program_is_all_the_harness_asks_for() {
        // Two runs are visible on disk. A harness that ran the pipeline twice
        // to recover a value would leave `abab` here.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/vm-one-run");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir should be creatable");
        // Forward slashes: the path is embedded in Redblue source, where a
        // backslash starts an escape (`\t` is a tab), so a Windows path would
        // arrive mangled. Windows file APIs accept `/` everywhere, so this
        // keeps the test's intent (the harness runs the program exactly once)
        // identical on every platform.
        let file = dir.join("out.txt");
        let unix_path = file.display().to_string().replace('\\', "/");
        let source = format!(
            "files.append(\"{}\", \"a\")\nfiles.append(\"{}\", \"b\")\n",
            unix_path, unix_path,
        );
        let outcome = tree_walk(&source);
        assert!(
            outcome.result.is_ok(),
            "the fixture must run: {:?}",
            outcome.result.err(),
        );
        assert_eq!(
            std::fs::read_to_string(&file).expect("the fixture should have written"),
            "ab",
            "the program ran more than once",
        );
    }

    /// The program the echo probe runs. Its two lines are the whole point: they
    /// are what an echoing VM prints and what a silent one must not.
    const ECHO_FIXTURE: &str = "say \"one\"\nsay \"two\"\n";

    /// The variable that tells this test it is the **child** of the echo probe.
    ///
    /// The value is the echo path to run: `on` or `off`. The child measures; the
    /// parent asserts, which is the only way either half can be trusted: `stdout`
    /// is a process boundary, so the child is the only place the question "did
    /// this print?" can be asked of a program that prints.
    const ECHO_PROBE: &str = "RB_ECHO_PROBE";

    /// The markers the child prints around its half of the probe.
    ///
    /// The child prints one *before* it runs the program and one *after*, so the
    /// program's own lines are lines of their own between two markers, and so a
    /// child that never reached this code is a failure rather than silence read as a
    /// pass. The leading newline closes the line the test harness has already begun
    /// writing its name to, which it does not terminate.
    fn echo_marker(what: &str, mode: &str) -> String {
        format!("RB-ECHO-PROBE-{what} {mode}")
    }

    #[test]
    fn edge_a_vm_built_as_a_library_does_not_print_what_the_program_said() {
        // Child half: run one echo path, between two markers. Nothing is asserted
        // here — the parent's reading of this child's stdout is the measurement.
        if let Ok(mode) = std::env::var(ECHO_PROBE) {
            assert!(
                matches!(mode.as_str(), "on" | "off"),
                "the probe runs one echo path or the other, not {mode:?}",
            );
            println!("\n{}", echo_marker("BEGIN", &mode));
            let mut vm = BytecodeVm::new();
            if mode == "off" {
                vm.set_echo(false);
            }
            let _ = vm.run(&redblue::compile_source(ECHO_FIXTURE).expect("the fixture compiles"));
            println!("{}", echo_marker("END", &mode));
            return;
        }

        // `BytecodeVm::run` prints on its own by default, because `rb vm` is the
        // only way most callers run a `.rbc`. A library caller turns that off —
        // and must still get the lines back, because taking them is the whole
        // point of turning it off.
        let source = ECHO_FIXTURE;

        let mut loud = BytecodeVm::new();
        assert!(
            loud.run(&redblue::compile_source(source).expect("the fixture compiles"))
                .is_ok(),
            "the fixture must run",
        );
        assert_eq!(
            loud.take_output(),
            vec!["one".to_string(), "two".to_string()],
            "echoing off must not change what the program said",
        );

        let outcome = bytecode(source);
        assert_eq!(
            outcome.output,
            vec!["one".to_string(), "two".to_string()],
            "the harness asks for the lines and must get them",
        );

        // Everything above is what the harness *takes*. None of it can see what
        // was **printed**, and a `set_echo(false)` that did nothing would hand the
        // lines back and print them anyway. So the printing is measured where it
        // happens, in a child process whose stdout is the parent's to read: on for
        // the control, off for the claim. The control is what makes the silence
        // mean something — a child that never ran, or a harness that cannot read
        // its own child's stdout, prints nothing on both halves and the `off` case
        // would pass vacuously.
        for (mode, printed) in [("on", true), ("off", false)] {
            let output = run_echo_probe(mode);
            let said = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success(),
                "the echo-{mode} probe failed: {}",
                String::from_utf8_lossy(&output.stderr),
            );
            // The lines between the child's two markers, and only those: what the
            // echo-{mode} VM printed.
            let between = said
                .lines()
                .skip_while(|line| *line != echo_marker("BEGIN", mode))
                .skip(1)
                .take_while(|line| *line != echo_marker("END", mode))
                .collect::<Vec<_>>();
            assert!(
                !between.is_empty() || mode == "off",
                "the echo-{mode} probe never reached this test, so its silence \
                 proves nothing. Its stdout was:\n{said}",
            );
            assert!(
                between.contains(&"one") || mode == "off",
                "the echo-{mode} probe did not print the first line. Its stdout \
                 was:\n{said}",
            );
            assert_eq!(
                between.contains(&"one") && between.contains(&"two"),
                printed,
                "echo {mode}: the program said two lines and the VM printed {:?}. \
                 Its stdout was:\n{said}",
                between,
            );
        }
    }

    /// Runs this test again in a child process with `RB_ECHO_PROBE` set to `mode`,
    /// and gives back what the child wrote to stdout.
    fn run_echo_probe(mode: &str) -> std::process::Output {
        use std::process::Command;

        Command::new(std::env::current_exe().expect("this test binary should have a path"))
            .args([
                "--exact",
                "--nocapture",
                "--test-threads=1",
                "common::vm::tests::edge_a_vm_built_as_a_library_does_not_print_what_the_program_said",
            ])
            .env(ECHO_PROBE, mode)
            .output()
            .expect("the echo probe should be runnable")
    }

    #[test]
    fn edge_the_two_halves_of_an_outcome_are_both_recorded() {
        let outcome = tree_walk("say \"x\"\n42\n");
        assert_eq!(outcome.output, vec!["x".to_string()]);
        assert_eq!(outcome.result, Ok(Typed::Number(42.0)));
        assert_eq!(outcome.label(), None, "a completion has no label");
    }

    #[test]
    fn edge_a_failure_renders_with_its_label_message_and_position() {
        let failure = Failure::Label {
            label: "ParserError".to_string(),
            message: "Expected 'end'".to_string(),
            position: Some((3, 1)),
            column_exact: true,
        };
        assert_eq!(show(&failure), "ParserError: Expected 'end' at 3:1");
        assert_eq!(
            show(&Failure::Label {
                label: "IoError".to_string(),
                message: "cannot read".to_string(),
                position: None,
                column_exact: false,
            }),
            "IoError: cannot read",
        );
        assert_eq!(failure.position(), Some((3, 1)));
        assert_eq!(failure.message(), "Expected 'end'");
        assert!(failure.column_exact());
    }

    /// A recorded format limit, not a blanket exemption.
    ///
    /// A `.rbc` instruction carries a line and no column, so the bytecode VM can
    /// only place a runtime failure at the start of its line — and this holds it
    /// to *that*, rather than to any column at all. `is exact` says which side of
    /// the comparison is speaking.
    fn at(label: &str, line: usize, column: usize, is_exact: bool) -> Failure {
        Failure::Label {
            label: label.to_string(),
            message: "boom".to_string(),
            position: Some((line, column)),
            column_exact: is_exact,
        }
    }

    #[test]
    fn edge_a_column_only_one_engine_can_name_is_held_to_the_convention() {
        let source = "set xs to [1, 2]\nsay xs[9]\n";
        let tree = tree_walk(source).result.expect_err("the fixture must fail");
        let byte = bytecode(source).result.expect_err("the fixture must fail");
        assert!(
            byte.message().starts_with("Index 9 is out of bounds"),
            "the bytecode VM reports the same failure: {}",
            byte.message(),
        );
        assert!(
            !byte.column_exact(),
            "a compiled instruction carries a line and no column, so the bytecode \
             VM's position is a line-granular one",
        );

        // What the two engines really report: the same line, and a column of 1
        // from each, which the convention allows.
        assert_eq!(tree.position(), Some((2, 1)));
        assert_eq!(byte.position(), Some((2, 1)));
        assert!(same_place(&tree, &byte));

        // The column the old blanket rule hid: the bytecode VM reporting a column
        // other than the start of the line is a change in the interpreter, and it
        // is a divergence rather than a format limit.
        assert!(
            !same_place(&at("RuntimeError", 2, 7, false), &tree),
            "a line-granular column that is not the start of the line must not pass",
        );
        // And the other direction: an exact column that has moved.
        assert!(!same_place(&at("RuntimeError", 2, 5, true), &tree));
        // Two exact columns have to be the same column.
        assert!(same_place(
            &at("RuntimeError", 2, 5, true),
            &at("RuntimeError", 2, 5, true)
        ));
        // The line is compared either way.
        assert!(!same_place(&at("RuntimeError", 3, 1, false), &tree));
    }

    /// The mutation this column comparison exists to catch: a position that is
    /// nowhere on one engine, and a line that is somewhere else on the other.
    #[test]
    fn edge_a_position_is_compared_as_a_whole_not_as_a_line() {
        let with = |position| Failure::Label {
            label: "RuntimeError".to_string(),
            message: "boom".to_string(),
            position,
            column_exact: true,
        };
        assert!(same_place(&with(Some((2, 1))), &with(Some((2, 1)))));
        assert!(same_place(&with(None), &with(None)));
        assert!(!same_place(&with(Some((2, 1))), &with(None)));
        assert!(!same_place(&with(None), &with(Some((2, 1)))));
    }
}
