//! The corpus's two halves: curated families, and a seeded property grammar.
//!
//! ## Curated families
//!
//! Every program in `corpus/` is produced here, from a table, so regenerating
//! the corpus is a function of nothing and two machines produce the same bytes.
//! The families are not random: each one exists to pin a row of the edge-case
//! matrix, and the tables name what each row is.
//!
//! ## The property grammar
//!
//! [`generate`] draws a program from a seed. Two properties make a draw
//! meaningful:
//!
//! - **It is typed.** Every expression arm declares the [`Kind`] of value it
//!   produces, and a program is built by binding one name per kind and drawing
//!   arms that read only names of the kind they need. A drawn program therefore
//!   has exactly one way to fail: the fault the draw injected on purpose.
//!   `edge_a_generated_program_only_faults_on_a_fault_that_was_injected` is the
//!   test, and it is a test that has found things.
//! - **It is bounded.** An `add` arm draws two more numbers, each of which may
//!   be an `add`, so without a depth bound a single seed can produce a source
//!   with no size bound at all. [`MAX_DEPTH`] bounds the nesting and
//!   [`Arm::descends`] names the arms that stop.

use super::rng::Rng;

/// The number the corpus's binder names hold. Read only by the arms that need
/// them; an arm that reads `l0` gets a two-element list, so an index into it is
/// a deliberate out-of-bounds rather than an accident.
pub const NUMBER: &str = "n0";
pub const TEXT: &str = "t0";
pub const YESNO: &str = "b0";
pub const LIST: &str = "l0";
pub const RECORD: &str = "r0";

/// The five kinds a drawn expression can be worth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Number,
    Text,
    YesNo,
    List,
    Record,
}

/// How deep a drawn expression may nest before it stops drawing.
pub const MAX_DEPTH: usize = 3;

/// A fault a draw can inject, with the source that produces it and the failure it
/// must produce.
///
/// Every tail is a *typed* program until its last line: a fault that cannot be
/// reached without also tripping a different failure is not a fault test, it is
/// two tests where one of them is a surprise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    None,
    NumberPlusText,
    IndexPastTheEnd,
    IndexIntoANumber,
    CallAnUnknownFunction,
    DivideByZero,
    LengthOfNothing,
    PropertyOfNothing,
}

impl Fault {
    /// The declaration line that introduces a drawn fault, so a reader of the
    /// generated program can see which fault it carries.
    pub fn name(&self) -> &'static str {
        match self {
            Fault::None => "none",
            Fault::NumberPlusText => "number_plus_text",
            Fault::IndexPastTheEnd => "index_past_the_end",
            Fault::IndexIntoANumber => "index_into_a_number",
            Fault::CallAnUnknownFunction => "call_an_unknown_function",
            Fault::DivideByZero => "divide_by_zero",
            Fault::LengthOfNothing => "length_of_nothing",
            Fault::PropertyOfNothing => "property_of_nothing",
        }
    }

    /// The tail a draw with this fault ends with.
    pub fn tail(&self) -> &'static str {
        match self {
            Fault::None => "",
            Fault::NumberPlusText => "say n0 + \"text\"",
            Fault::IndexPastTheEnd => "say l0[9]",
            Fault::IndexIntoANumber => "say n0[0]",
            Fault::CallAnUnknownFunction => "no_such_function_exists()",
            Fault::DivideByZero => "say 1 / 0",
            Fault::LengthOfNothing => "say length(nothing)",
            Fault::PropertyOfNothing => "say nothing.field",
        }
    }

    /// The message the tail must fail with. `None` for no fault: a program with
    /// no fault must run to completion.
    pub fn message(&self) -> Option<&'static str> {
        match self {
            Fault::None => None,
            Fault::NumberPlusText => Some("Cannot add non-numbers"),
            Fault::IndexPastTheEnd => Some("out of bounds"),
            Fault::IndexIntoANumber => Some("Cannot index non-list"),
            Fault::CallAnUnknownFunction => Some("Unknown function"),
            Fault::DivideByZero => Some("Division by zero"),
            Fault::LengthOfNothing => Some("length requires"),
            Fault::PropertyOfNothing => Some("Cannot access property on non-object"),
        }
    }
}

/// Every fault, in declaration order.
pub const ALL_FAULTS: &[Fault] = &[
    Fault::None,
    Fault::NumberPlusText,
    Fault::IndexPastTheEnd,
    Fault::IndexIntoANumber,
    Fault::CallAnUnknownFunction,
    Fault::DivideByZero,
    Fault::LengthOfNothing,
    Fault::PropertyOfNothing,
];

/// Every fault a draw can inject, in declaration order. [`ALL_FAULTS`] is this
/// table with `None` in front of it.
pub const FAULTS: &[Fault] = &[
    Fault::NumberPlusText,
    Fault::IndexPastTheEnd,
    Fault::IndexIntoANumber,
    Fault::CallAnUnknownFunction,
    Fault::DivideByZero,
    Fault::LengthOfNothing,
    Fault::PropertyOfNothing,
];

// --- the typed grammar -------------------------------------------------------

/// One way of drawing an expression.
///
/// Every arm is in [`ARMS`], every arm declares the kind it is worth, and every
/// arm renders for any set of bound names — so the whole grammar is enumerable
/// and a test can ask each arm what it is worth rather than trusting it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arm {
    NumberLiteral,
    NumberNegate,
    NumberAdd,
    NumberSubtract,
    NumberMultiply,
    NumberDivide,
    NumberModulo,
    NumberFromList,
    TextLiteral,
    TextConcat,
    TextConcatLiteralFirst,
    TextDouble,
    TextSpaced,
    TextEmpty,
    YesLiteralYes,
    YesLiteralNo,
    YesIsSelf,
    YesIsNotSelf,
    YesAnd,
    YesOr,
    ListEmpty,
    ListSingleton,
    ListPair,
    ListTriple,
    ListOfText,
    RecordEmpty,
    RecordOneField,
    RecordTwoFields,
    RecordNested,
}

/// Every arm of the grammar.
pub const ARMS: &[Arm] = &[
    Arm::NumberLiteral,
    Arm::NumberNegate,
    Arm::NumberAdd,
    Arm::NumberSubtract,
    Arm::NumberMultiply,
    Arm::NumberDivide,
    Arm::NumberModulo,
    Arm::NumberFromList,
    Arm::TextLiteral,
    Arm::TextConcat,
    Arm::TextConcatLiteralFirst,
    Arm::TextDouble,
    Arm::TextSpaced,
    Arm::TextEmpty,
    Arm::YesLiteralYes,
    Arm::YesLiteralNo,
    Arm::YesIsSelf,
    Arm::YesIsNotSelf,
    Arm::YesAnd,
    Arm::YesOr,
    Arm::ListEmpty,
    Arm::ListSingleton,
    Arm::ListPair,
    Arm::ListTriple,
    Arm::ListOfText,
    Arm::RecordEmpty,
    Arm::RecordOneField,
    Arm::RecordTwoFields,
    Arm::RecordNested,
];

impl Arm {
    /// The kind of value this arm is worth.
    pub fn kind(&self) -> Kind {
        match self {
            Arm::NumberLiteral
            | Arm::NumberNegate
            | Arm::NumberAdd
            | Arm::NumberSubtract
            | Arm::NumberMultiply
            | Arm::NumberDivide
            | Arm::NumberModulo
            | Arm::NumberFromList => Kind::Number,
            Arm::TextLiteral
            | Arm::TextConcat
            | Arm::TextConcatLiteralFirst
            | Arm::TextDouble
            | Arm::TextSpaced
            | Arm::TextEmpty => Kind::Text,
            Arm::YesLiteralYes
            | Arm::YesLiteralNo
            | Arm::YesIsSelf
            | Arm::YesIsNotSelf
            | Arm::YesAnd
            | Arm::YesOr => Kind::YesNo,
            Arm::ListEmpty
            | Arm::ListSingleton
            | Arm::ListPair
            | Arm::ListTriple
            | Arm::ListOfText => Kind::List,
            Arm::RecordEmpty | Arm::RecordOneField | Arm::RecordTwoFields | Arm::RecordNested => {
                Kind::Record
            }
        }
    }

    /// Whether this arm draws another expression, and so needs depth to draw.
    pub fn descends(&self) -> bool {
        matches!(
            self,
            Arm::NumberAdd
                | Arm::NumberSubtract
                | Arm::NumberMultiply
                | Arm::NumberDivide
                | Arm::NumberModulo
                | Arm::TextConcat
                | Arm::ListSingleton
                | Arm::ListPair
                | Arm::ListTriple
                | Arm::RecordOneField
                | Arm::RecordTwoFields
                | Arm::RecordNested
        )
    }

    /// The arms of `kind` that do not descend, which is what a draw at the depth
    /// bound must choose from.
    pub fn terminal_of(kind: Kind) -> Vec<Arm> {
        ARMS.iter()
            .copied()
            .filter(|arm| arm.kind() == kind && !arm.descends())
            .collect()
    }

    /// The arms of `kind`, in the table's order.
    pub fn all_of(kind: Kind) -> Vec<Arm> {
        ARMS.iter()
            .copied()
            .filter(|arm| arm.kind() == kind)
            .collect()
    }

    /// The arm's source, reading only the bound names.
    ///
    /// Parenthesised per operand rather than relying on an outer pair, because a
    /// drawn operand that already ends in `* …` would otherwise regroup: that is
    /// how a `mod` arm once ended up dividing by a zero that was in the table.
    pub fn render(&self, rng: &mut Rng) -> String {
        match self {
            Arm::NumberLiteral => rng.between(-9, 99).to_string(),
            Arm::NumberNegate => format!("(- {})", self.sibling(Arm::NumberLiteral, rng)),
            Arm::NumberAdd => format!("({NUMBER} + {})", self.sibling(Arm::NumberLiteral, rng)),
            Arm::NumberSubtract => {
                format!("({NUMBER} - {})", self.sibling(Arm::NumberLiteral, rng))
            }
            Arm::NumberMultiply => {
                format!("({NUMBER} * {})", self.sibling(Arm::NumberLiteral, rng))
            }
            // The divisor is `(x * x) + 1`, never a bare literal, so a drawn
            // program cannot divide by zero without a fault being injected for
            // it. `1 % 0` is a real fault and is in `FAULTS`.
            Arm::NumberDivide => format!(
                "({NUMBER} / ((({}) * ({})) + 1))",
                self.sibling(Arm::NumberLiteral, rng),
                self.sibling(Arm::NumberLiteral, rng)
            ),
            Arm::NumberModulo => format!(
                "({NUMBER} % ((({}) * ({})) + 1))",
                self.sibling(Arm::NumberLiteral, rng),
                self.sibling(Arm::NumberLiteral, rng)
            ),
            Arm::NumberFromList => format!("(({LIST}[0]) + {NUMBER})"),
            Arm::TextLiteral => format!("\"{}\"", super::generator::WORDS[rng.below(8)]),
            Arm::TextConcat => format!("({TEXT} + {})", self.sibling(Arm::TextLiteral, rng)),
            Arm::TextConcatLiteralFirst => {
                format!("({} + {TEXT})", self.sibling(Arm::TextLiteral, rng))
            }
            Arm::TextDouble => format!("({TEXT} + {TEXT})"),
            Arm::TextSpaced => format!("(\"  \" + {TEXT} + \"  \")"),
            Arm::TextEmpty => "\"\"".to_string(),
            Arm::YesLiteralYes => "yes".to_string(),
            Arm::YesLiteralNo => "no".to_string(),
            Arm::YesIsSelf => format!("({YESNO} is {YESNO})"),
            Arm::YesIsNotSelf => format!("({YESNO} is not {YESNO})"),
            Arm::YesAnd => format!("({YESNO} and {YESNO})"),
            Arm::YesOr => format!("({YESNO} or {YESNO})"),
            Arm::ListEmpty => "[]".to_string(),
            Arm::ListSingleton => format!("[{}]", self.sibling(Arm::NumberLiteral, rng)),
            Arm::ListPair => format!(
                "[{}, {}]",
                self.sibling(Arm::NumberLiteral, rng),
                self.sibling(Arm::NumberLiteral, rng)
            ),
            Arm::ListTriple => format!(
                "[{}, {}, {}]",
                self.sibling(Arm::NumberLiteral, rng),
                self.sibling(Arm::NumberLiteral, rng),
                self.sibling(Arm::NumberLiteral, rng)
            ),
            Arm::ListOfText => format!("[{}, {}]", self.sibling(Arm::TextLiteral, rng), TEXT),
            Arm::RecordEmpty => "{}".to_string(),
            Arm::RecordOneField => format!("{{a: {}}}", self.sibling(Arm::NumberLiteral, rng)),
            Arm::RecordTwoFields => format!(
                "{{a: {}, b: {}}}",
                self.sibling(Arm::NumberLiteral, rng),
                self.sibling(Arm::TextLiteral, rng)
            ),
            Arm::RecordNested => format!(
                "{{outer: {{inner: {}}}, tag: {TEXT}}}",
                self.sibling(Arm::NumberLiteral, rng)
            ),
        }
    }

    /// A literal of the given arm's kind, or the arm itself when it is one.
    fn sibling(&self, arm: Arm, rng: &mut Rng) -> String {
        arm.render(rng)
    }
}

/// The words a drawn text literal is built from. Fixed, so a draw is a function
/// of its seed and not of a dictionary's iteration order.
pub const WORDS: &[&str] = &[
    "alpha",
    "beta",
    "gamma",
    "delta",
    "日本語",
    "café",
    "🎉",
    "",
];

/// A drawn program: its source and the fault it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generated {
    pub seed: u64,
    pub source: String,
    pub fault: Fault,
}

/// Draws a program from `seed`.
///
/// The same seed gives the same bytes on any machine, which is what makes a
/// property failure nameable.
pub fn generate(seed: u64) -> Generated {
    let mut rng = Rng::new(seed);

    let mut source = String::new();
    source.push_str(&format!("// generated: seed {seed}\n"));
    let fault = ALL_FAULTS[(seed % ALL_FAULTS.len() as u64) as usize];
    source.push_str(&format!("// fault: {}\n", fault.name()));
    source.push_str(&format!(
        "set {NUMBER} to {}\n",
        Arm::NumberLiteral.render(&mut rng)
    ));
    source.push_str(&format!(
        "set {TEXT} to {}\n",
        Arm::TextLiteral.render(&mut rng)
    ));
    source.push_str(&format!(
        "set {YESNO} to {}\n",
        Arm::YesLiteralYes.render(&mut rng)
    ));
    source.push_str(&format!("set {LIST} to [1, 2]\n"));
    source.push_str(&format!("set {RECORD} to {{a: 1, b: {TEXT}}}\n"));

    let statements = 1 + rng.below(3);
    for _ in 0..statements {
        source.push_str(&statement(&mut rng, MAX_DEPTH));
    }

    let tail = if fault == Fault::None {
        let kind = [
            Kind::Number,
            Kind::Text,
            Kind::YesNo,
            Kind::List,
            Kind::Record,
        ][rng.below(5)];
        let arm = *rng
            .pick(&Arm::all_of(kind))
            .expect("every kind has at least one arm");
        // A drawn program's last line is its value, so the two engines can be
        // asked what it was worth and not only what it printed.
        format!("{}\n", arm.render(&mut rng))
    } else {
        format!("{}\n", fault.tail())
    };
    source.push_str(&tail);

    Generated {
        seed,
        source,
        fault,
    }
}

/// One drawn statement: a `say`, an `if`, or a `repeat`. Every one of them reads
/// only bound names, so a drawn program cannot fail in a statement.
fn statement(rng: &mut Rng, depth: usize) -> String {
    match rng.below(4) {
        0 => format!("say {}\n", draw(rng, Kind::Text, depth)),
        1 => {
            let left = draw(rng, Kind::Number, depth);
            let right = draw(rng, Kind::Number, depth);
            format!(
                "if ({left} is {right}) then\n    say {}\nend\n",
                draw(rng, Kind::Text, depth)
            )
        }
        2 => format!(
            "if ({YESNO} and no) then\n    say {}\nend\n",
            draw(rng, Kind::Text, depth)
        ),
        _ => {
            let count = rng.between(0, 2);
            format!(
                "repeat {count} times\n    say {}\nend\n",
                draw(rng, Kind::Number, depth)
            )
        }
    }
}

/// One drawn expression of `kind`, at most `depth` deep.
pub fn draw(rng: &mut Rng, kind: Kind, depth: usize) -> String {
    let arms = if depth == 0 {
        Arm::terminal_of(kind)
    } else {
        Arm::all_of(kind)
    };
    let arm = *rng.pick(&arms).expect("every kind has at least one arm");
    arm.render(rng)
}

// --- the curated corpus ------------------------------------------------------

/// Every corpus program, as `(name, source)`, in a stable order.
///
/// The names carry the family so a failure says which row of the edge-case
/// matrix failed, and the numbers so a failure names one program.
pub fn corpus_sources() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    arithmetic(&mut out);
    numeric_boundary(&mut out);
    text_ops(&mut out);
    unicode(&mut out);
    lists(&mut out);
    records(&mut out);
    control_flow(&mut out);
    functions(&mut out);
    objects(&mut out);
    loop_forms(&mut out);
    nesting(&mut out);
    stdlib(&mut out);
    runtime_errors(&mut out);
    faults_family(&mut out);
    malformed(&mut out);
    value_tails(&mut out);
    out
}

fn push(out: &mut Vec<(String, String)>, family: &str, source: &str) {
    let n = out
        .iter()
        .filter(|(name, _)| name.starts_with(&format!("{family}-")))
        .count()
        + 1;
    out.push((format!("{family}-{n:04}"), source.to_string()));
}

const ARITHMETIC_PAIRS: &[(&str, &str)] = &[
    ("10", "20"),
    ("50", "30"),
    ("6", "7"),
    ("100", "4"),
    ("17", "5"),
    ("0", "0"),
    ("-4", "4"),
    ("1", "1"),
    ("2", "3"),
    ("9", "9"),
    ("1000000", "7"),
    ("-9", "3"),
    ("8", "2"),
    ("12", "12"),
    ("7", "13"),
    ("0", "5"),
    ("5", "0"),
    ("100", "7"),
    ("-6", "-7"),
    ("2147483647", "1"),
];

const ARITHMETIC_OPS: &[&str] = &["+", "-", "*", "/", "%"];

fn arithmetic(out: &mut Vec<(String, String)>) {
    for (i, (a, b)) in ARITHMETIC_PAIRS.iter().enumerate() {
        let op = ARITHMETIC_OPS[i % ARITHMETIC_OPS.len()];
        push(
            out,
            "arithmetic",
            &format!("set a to {a}\nset b to {b}\nsay a {op} b\n"),
        );
    }
}

const NUMERIC_BOUNDARY: &[&str] = &[
    "say 1 / 0\n",
    "say 0 / 0\n",
    "say 9 % 0\n",
    "say 1 / -0.0\n",
    "say 0.1 + 0.2\n",
    "say 1 / 3\n",
    "say -0.0\n",
    "say 2147483647 + 1\n",
    "say 2147483647\n",
    "say -2147483648\n",
    "say 9223372036854775807\n",
    "say -9223372036854775808\n",
    "say 9007199254740992\n",
    "say 9007199254740993\n",
    "say 9007199254740992 + 1\n",
    "say -7 / 2\n",
    "say 7 / -2\n",
    "say -7 % 3\n",
    "say 7 % -3\n",
    "say 9 % 9\n",
    "say 0.1 + 0.7\n",
    "say 2 * 0.5\n",
    "say 1.5 + 1.5\n",
    "say -(3)\n",
    "say 0.30000000000000004\n",
];

fn numeric_boundary(out: &mut Vec<(String, String)>) {
    for source in NUMERIC_BOUNDARY {
        push(out, "numeric-boundary", source);
    }
}

const TEXT_OPS: &[&str] = &[
    "say \"hello\" + \" world\"\n",
    "say length(\"hello\")\n",
    "say \"abc\" is \"abc\"\n",
    "say \"\"\n",
    "say length(\"\")\n",
    "say \"\\\"\"\"\n",
    "say \"\\\\\"\n",
    "say \"a\\nb\"\n",
    "say \"a\\tb\"\n",
    "say \"x\" + \"\"\n",
    "say length(\"日本語\")\n",
    "say 5 + \"abc\"\n",
    "say length(\"abc\") + length(\"de\")\n",
    "say \"a\" + \"b\" + \"c\"\n",
    "say \"Hello, World!\"\n",
    "set name to \"World\"\nsay \"Hello, \" + name + \"!\"\n",
    // `{interp}` is documented and unimplemented: the braces are printed. The
    // corpus records what the interpreter does, and FINDINGS.md carries the gap.
    "set name to \"World\"\nsay \"Hello, {name}!\"\n",
    "say \"abc\" is not \"abd\"\n",
    "say length(\"🎉\")\n",
    "say \"  \" + \"x\" + \"  \"\n",
    "say \"\" + \"\"\n",
    // Trailing tokens on a `say` are accepted; the last one becomes the value.
    "say \"a\" \"b\"\n",
    "say 1 2 3\n",
];

fn text_ops(out: &mut Vec<(String, String)>) {
    for source in TEXT_OPS {
        push(out, "text-ops", source);
    }
}

const UNICODE: &[&str] = &[
    "set s to \"日本語\"\nsay s\nsay length(s)\n",
    "set s to \"🎉🎊\"\nsay s\nsay length(s)\n",
    "set s to \"👩‍💻\"\nsay s\nsay length(s)\n",
    "set s to \"مرحبا\"\nsay s\nsay length(s)\n",
    "set s to \"𠮷\"\nsay s\nsay length(s)\n",
    "set s to \"café\"\nsay s\nsay length(s)\n",
    "set s to \"Ω≈ç√∫\"\nsay s\nsay length(s)\n",
    "set s to \"→∞\"\nsay s\nsay length(s)\n",
    "say \"日本語\" is \"日本語\"\n",
    "say \"日本語\" is not \"日本\"\n",
    "say \"🎉\" is \"🎉\"\n",
    "say \"𠮷\" is \"𠮷\"\n",
    "say \"مرحبا\" is \"مرحبا\"\n",
    "set a to \"日本\"\nset b to \"日本\"\nsay a is b\n",
    "set xs to [\"日本\", \"🎉\"]\nsay xs\nsay length(xs)\n",
    "\u{feff}say \"after a byte order mark\"\n",
];

fn unicode(out: &mut Vec<(String, String)>) {
    for source in UNICODE {
        push(out, "unicode", source);
    }
}

const LISTS: &[&str] = &[
    "set xs to []\nsay xs\nsay length(xs)\n",
    "set xs to [1]\nsay xs\nsay length(xs)\n",
    "set xs to [1, 2, 3]\nsay xs[0]\n",
    "set xs to [1, 2, 3]\nsay xs[2]\n",
    "set xs to [1, 2, 3]\nsay xs[-1]\n",
    "set xs to [1, 2, 3]\nsay xs[-3]\n",
    "set xs to [1, 2, 3,]\nsay xs\n",
    "set xs to [[1, 2], [3, 4]]\nsay xs[1][0]\n",
    "set xs to [1, 2]\nset ys to xs\nsay ys is xs\n",
    "set xs to [1, 2]\nfor each x in xs\n    say x\nend\n",
    "for each x in []\n    say x\nend\nsay \"done\"\n",
    "set xs to [\"a\", \"b\"]\nsay xs[1]\n",
    "set xs to [yes, no]\nsay xs[0]\n",
    "set xs to [nothing]\nsay xs[0]\n",
    "set xs to [[]]\nsay length(xs[0])\n",
    "set xs to [{a: 1}]\nsay xs[0].a\n",
    "say length([1, 2, 3])\n",
    "set xs to [1, 2, 3]\nsay xs[9]\n",
    "set xs to [1]\nsay xs[1]\n",
    "set xs to []\nsay xs[0]\n",
];

fn lists(out: &mut Vec<(String, String)>) {
    for source in LISTS {
        push(out, "lists", source);
    }
}

const RECORDS: &[&str] = &[
    "set r to {a: 1}\nsay r\nsay length(r)\n",
    "set r to {}\nsay r\nsay length(r)\n",
    "set r to {a: 1, b: \"x\"}\nsay r\n",
    "set r to {a: {b: {c: 3}}}\nsay r.a.b.c\n",
    "set r to {a: 1}\nsay r.missing\n",
    "set r to {a: {b: 1}}\nsay r.a.missing\n",
    "set r to {a: {b: {c: 1}}}\nsay r.a.b.missing\n",
    "set r to {a: 1, a: 2}\nsay r.a\n",
    "set r to {b: 2, a: 1}\nsay r.a\nsay r.b\n",
    "set r to {a: 1}\nset s to r\nsay s is r\n",
    "set r to {a: 1}\nset s to r\nsay s.a\n",
    "set r to {a: 1}\nsay r[\"a\"]\n",
    "set r to {a: [1, 2]}\nsay r.a[1]\n",
    "set r to {a: nothing}\nsay r.a\n",
    "set r to {a: 1, b: yes}\nsay r.b\n",
    "set r to {}\nset r.a to 5\nsay r.a\n",
];

fn records(out: &mut Vec<(String, String)>) {
    for source in RECORDS {
        push(out, "records", source);
    }
}

const CONTROL_FLOW: &[&str] = &[
    "if yes then\n    say \"yes\"\nend\n",
    "if no then\n    say \"yes\"\nelse\n    say \"no\"\nend\n",
    "if 1 is 1 then\n    say \"a\"\nelse if 1 is 2 then\n    say \"b\"\nelse\n    say \"c\"\nend\n",
    "if 1 is 2 then\n    say \"a\"\nelse if 2 is 2 then\n    say \"b\"\nelse\n    say \"c\"\nend\n",
    "if 1 is 2 then\n    say \"a\"\nelse if 2 is 3 then\n    say \"b\"\nelse\n    say \"c\"\nend\n",
    "if yes then\nend\nsay \"after\"\n",
    "if 1 is 1 and 2 is 2 then\n    say \"both\"\nend\n",
    "if 1 is 2 or 2 is 2 then\n    say \"either\"\nend\n",
    "if not no then\n    say \"not\"\nend\n",
    "unless yes then\n    say \"a\"\nend\nsay \"after\"\n",
    "unless no then\n    say \"a\"\nend\n",
    "if 1 is 1 then\n    if 2 is 2 then\n        if 3 is 3 then\n            say \"deep\"\n        end\n    end\nend\n",
    "set n to 5\nif n is 5 then\n    say n\nelse\n    say \"no\"\nend\n",
    "if \"a\" is \"a\" then\n    say \"text\"\nend\n",
    "if yes and no then\n    say \"never\"\nend\nsay \"after\"\n",
    "if nothing is nothing then\n    say \"nothing\"\nend\n",
    // The two-arm `if`/`else` is what the family really covers: `else if` is
    // written by the three programs above and **refused** by both engines
    // (`SPEC.md:353` and `docs/GRAMMAR.md:206` both document a form neither
    // parses), so the `else` branch is pinned here with something in it, with
    // nothing in it, and with a nested `if` inside it.
    "if no then\n    say \"a\"\nelse\n    say \"b\"\nend\n",
    "if no then\n    say \"a\"\nelse\n    say \"b\"\n    if 1 is 1 then\n        say \"b1\"\n    end\nend\n",
    "if no then\n    say \"a\"\nelse\nend\nsay \"after\"\n",
];

fn control_flow(out: &mut Vec<(String, String)>) {
    for source in CONTROL_FLOW {
        push(out, "control-flow", source);
    }
}

const FUNCTIONS: &[&str] = &[
    "to zero()\n    give back 0\nend\nsay zero()\n",
    "to add(a, b)\n    give back a + b\nend\nsay add(1, 2)\n",
    "to one(x)\n    give back x\nend\nsay one(\"v\")\n",
    "to greet(name)\n    say \"hi {name}\"\nend\ngreet(\"world\")\n",
    "to twice(n)\n    give back n * 2\nend\nset f to twice\nsay f(4)\n",
    "to twice(n)\n    give back n * 2\nend\nsay twice(3)\nsay twice(4)\n",
    "to apply(f, v)\n    give back f(v)\nend\nto double(n)\n    give back n + n\nend\nsay apply(double, 5)\n",
    "to pick(flag)\n    if flag is yes then\n        give back \"first\"\n    end\n    give back \"second\"\nend\nsay pick(yes)\nsay pick(no)\n",
    "to count_down(n)\n    if n is 0 then\n        give back 0\n    end\n    give back n + count_down(n - 1)\nend\nsay count_down(3)\n",
    "to fact(n)\n    if n is 0 then\n        give back 1\n    end\n    give back n * fact(n - 1)\nend\nsay fact(5)\n",
    "to a()\n    give back b() + 1\nend\nto b()\n    give back 1\nend\nsay a()\n",
    "to nothing_back()\n    say \"side effect\"\nend\nnothing_back()\n",
    "to outer()\n    give back inner()\nend\nto inner()\n    give back 7\nend\nsay outer()\n",
    "to nokey()\n    give back 1\nend\nsay nokey()\nsay nokey()\n",
    "to wrong(n)\n    give back n\nend\nsay wrong()\n",
    "to missing()\n    give back 1\nend\nsay no_such_function()\n",
    "to pair(a, b)\n    give back [a, b]\nend\nsay pair(1, 2)\n",
];

fn functions(out: &mut Vec<(String, String)>) {
    for source in FUNCTIONS {
        push(out, "functions", source);
    }
}

const OBJECTS: &[&str] = &[
    "object Empty\nend\nsay Empty\n",
    "object Tagged\n    has tag default \"none\"\nend\nsay Tagged.tag\n",
    "object Undefaulted\n    has tag\nend\nsay Tagged.tag\n",
    "object Point\nend\nset Point.x to 3\nsay Point.x\n",
    "object Base\n    has tag default \"base\"\nend\nobject Derived extends Base\nend\nsay Derived.tag\n",
    "object Base\n    has tag default \"base\"\nend\nobject Middle extends Base\n    has tag default \"middle\"\nend\nobject Leaf extends Middle\nend\nsay Leaf.tag\n",
    "object Base\nend\nobject Derived extends Base\nend\nsay Derived is Base\n",
    "object Counter\n    to bump()\n        give back 1\n    end\nend\nsay Counter\n",
    "object Reader\n    has value default 1\n    to get()\n        give back value\n    end\nend\nsay Reader.get()\n",
    "object Plain\nend\nsay Plain.missing\n",
    "object Two\nend\nsay Two.tag\n",
    "object WithTwo\n    has a default 1\n    has b default 2\nend\nsay WithTwo.a + WithTwo.b\n",
    "object Outer\n    has tag default \"o\"\n    object Inner\n        has x default 1\n    end\n    say \"after inner\"\nend\nsay Outer.tag\n",
    "object Outer\n    object Inner\n        has x default 1\n    end\nend\nsay type_of(Outer)\nsay type_of(Inner)\n",
    // An inner declaration `extends`ing the type it is written inside. The
    // enclosing type is registered before the rest of its body runs, so the
    // chain resolves — which is the tree-walking order, and the one the
    // bytecode VM did not have. The inner name is read through `Base` and not
    // through `Child`, because the analyzer does not declare it outside the body
    // it was written in (FINDINGS.md §8).
    "object Base\n    has tag default \"base\"\n    object Child extends Base\n        has extra default 1\n    end\nend\nsay Base.tag\n",
    // Declaring the same name twice is refused, and a nested declaration whose
    // parent was refused never binds, so `Child` is an unknown variable.
    "object Base\n    has tag default \"base\"\nend\nobject Base\nend\nsay Base\n",
    "object Counter\n    has n default 0\nend\nset Counter.n to 1\nset Counter.n to Counter.n + 1\nsay Counter.n\n",
    "object A\nend\nobject B\nend\nsay A is B\n",
    "object Named\n    has name default \"x\"\nend\nsay upper(Named.name)\n",
    "object TwoFields\n    has a default 1\n    has b default 2\nend\nsay TwoFields\n",
    "object Chain\nend\nsay Chain.tag is nothing\n",
    "object After extends Nothing at all\nend\nsay After\n",
    // A field's `default` is compiled as an ordinary expression, so it may call a
    // function that declares an object of its own — and that declaration is still
    // open when the enclosing body declares its *next* field. The bytecode VM
    // assembles the declarations of an object on a stack for this reason; a single
    // slot let the inner declaration take the outer one's place, and the outer
    // body's second field went into the inner type.
    "to declare()\n    object Inner\n        has x default 1\n    end\n    give back 7\nend\nobject Outer\n    has tag default declare()\n    has n default 2\nend\nsay Outer.tag\nsay Outer.n\n",
];

fn objects(out: &mut Vec<(String, String)>) {
    for source in OBJECTS {
        push(out, "objects", source);
    }
}

const LOOP_FORMS: &[&str] = &[
    "repeat 0 times\n    say \"never\"\nend\nsay \"after\"\n",
    "repeat 1 times\n    say \"once\"\nend\n",
    "repeat 3 times\n    say \"x\"\nend\n",
    "set n to 0\nrepeat 5 times\n    set n to n + 1\nend\nsay n\n",
    "for each x in []\n    say x\nend\n",
    "for each x in [1]\n    say x\nend\n",
    "for each x in [1, 2, 3]\n    say x\nend\n",
    "for each x in [[1, 2], [3, 4]]\n    say x\nend\n",
    "for each x in [[1, 2], [3, 4]]\n    for each y in x\n        say y\n    end\nend\n",
    "for each x in [1, 2]\n    if x is 2 then\n        say \"two\"\n    end\nend\n",
    "set total to 0\nfor each x in [1, 2, 3, 4]\n    set total to total + x\nend\nsay total\n",
    // `break` and `skip`: both engines act on them (phase-025). These two goldens
    // used to record the no-op's output, which is why they are named in
    // `corpus::BREAK_AND_SKIP_PROGRAMS` and asserted on both engines by
    // `edge_a_break_and_a_skip_leave_the_loop_on_both_engines` — the table pins the
    // lines the fix produced, so a golden quietly edited back to a defect is a
    // failing test rather than passing coverage.
    "for each x in [1, 2, 3]\n    if x is 2 then\n        say \"found\"\n    end\n    break\nend\nsay \"after\"\n",
    "for each x in [1, 2, 3]\n    if x is 2 then\n        say x\n    end\n    skip\nend\nsay \"after\"\n",
    "set out to \"\"\nfor each x in [\"a\", \"b\"]\n    set out to out + x\nend\nsay out\n",
    "repeat 2 times\n    for each x in [1, 2]\n        say x\n    end\nend\n",
    "repeat 3 times\n    repeat 2 times\n        say \"n\"\n    end\nend\n",
    "set n to 10\nrepeat 10 times\n    set n to n - 1\nend\nsay n\n",
    "set i to 0\nwhile i < 3\n    say i\n    set i to i + 1\nend\n",
    "set i to 0\nwhile i < 2\n    set i to i + 1\nend\nsay i\n",
    "set i to 0\nwhile i < 0\n    say \"never\"\nend\nsay \"after\"\n",
];

fn loop_forms(out: &mut Vec<(String, String)>) {
    for source in LOOP_FORMS {
        push(out, "loop-forms", source);
    }
}

const NESTING: &[&str] = &[
    "say [[[[1]]]]\n",
    "say [[[[[1]]]]]\n",
    "say {a: {b: {c: {d: 1}}}}\n",
    "say {a: {b: {c: {d: {e: 1}}}}}\n",
    "set x to {a: {b: {c: {d: {e: {f: 1}}}}}}\nsay x.a.b.c.d.e.f\n",
    "set xs to [[[1, 2], [3, 4]], [[5, 6], [7, 8]]]\nsay xs[1][0][1]\n",
    "set xs to [[1], [2], [3]]\nsay length(xs)\nsay length(xs[0])\n",
    "set r to {a: [1, {b: 2}]}\nsay r.a[1].b\n",
    "if 1 is 1 then\n    if 2 is 2 then\n        if 3 is 3 then\n            say \"three\"\n        end\n    end\nend\n",
    "if 1 is 2 then\n    if 2 is 2 then\n        if 3 is 3 then\n            say \"a\"\n        end\n    end\nelse\n    say \"b\"\nend\n",
    "for each a in [1, 2]\n    for each b in [3, 4]\n        for each c in [5, 6]\n            say a + b + c\n        end\n    end\nend\n",
    "to outer()\n    give back middle()\nend\nto middle()\n    give back inner() + 1\nend\nto inner()\n    give back 1\nend\nsay outer()\n",
    "to sum(xs)\n    set total to 0\n    for each x in xs\n        set total to total + x\n    end\n    give back total\nend\nsay sum([1, 2, 3])\n",
    "to nest(n)\n    if n is 0 then\n        give back 0\n    end\n    give back n + nest(n - 1)\nend\nsay nest(4)\n",
    "set xs = [1]\n",
    "set r to {a: {b: [1, 2]}}\nsay r.a.b[1]\n",
    "set xs to [[[1]]]\nsay xs[0][0][0]\n",
];

fn nesting(out: &mut Vec<(String, String)>) {
    for source in NESTING {
        push(out, "nesting", source);
    }
}

const STDLIB: &[&str] = &[
    "say length(1)\n",
    "say length(\"abc\")\n",
    "say length([1, 2])\n",
    "say length([])\n",
    "say length({a: 1})\n",
    "say length({})\n",
    "say length(yes)\n",
    "say type_of(1)\n",
    "say type_of(1.5)\n",
    "say type_of(\"a\")\n",
    "say type_of(yes)\n",
    "say type_of(nothing)\n",
    "say type_of([1])\n",
    "say type_of([])\n",
    "say type_of({})\n",
    "say type_of({a: 1})\n",
    "say length(nothing)\n",
    "say length([1])\n",
];

fn stdlib(out: &mut Vec<(String, String)>) {
    for source in STDLIB {
        push(out, "stdlib", source);
    }
}

const RUNTIME_ERRORS: &[&str] = &[
    "set caught to no\ntry\n    say 1 + \"a\"\ncatch error\n    set caught to yes\nend\nsay caught\n",
    "set caught to no\ntry\n    say [1, 2][9]\ncatch error\n    set caught to yes\nend\nsay caught\n",
    "set caught to no\ntry\n    say 1 / 0\ncatch error\n    set caught to yes\nend\nsay caught\n",
    "set caught to no\ntry\n    say 9 % 0\ncatch error\n    set caught to yes\nend\nsay caught\n",
    "set caught to no\ntry\n    say no_such_function()\ncatch error\n    set caught to yes\nend\nsay caught\n",
    "set caught to no\ntry\n    say length(nothing)\ncatch error\n    set caught to yes\nend\nsay caught\n",
    "set caught to no\ntry\n    say nothing.field\ncatch error\n    set caught to yes\nend\nsay caught\n",
    "set caught to no\ntry\n    say [1][4]\ncatch error\n    set caught to yes\nend\nsay caught\n",
    "set caught to no\ntry\n    say [] [0]\ncatch error\n    set caught to yes\nend\nsay caught\n",
    "set caught to no\ntry\n    say 1 - \"a\"\ncatch error\n    set caught to yes\nend\nsay caught\n",
    "set caught to no\ntry\n    say yes + 1\ncatch error\n    set caught to yes\nend\nsay caught\n",
    "set caught to no\ntry\n    say nothing + 1\ncatch error\n    set caught to yes\nend\nsay caught\n",
    "say \"before\"\ntry\n    say 1 + \"a\"\ncatch error\n    say \"caught\"\nend\nsay \"after\"\n",
    "set caught to no\ntry\n    try\n        say 1 / 0\n    catch inner\n        set caught to yes\n    end\ncatch error\n    set caught to no\nend\nsay caught\n",
    "say \"a\"\ntry\n    say 1 / 0\ncatch error\n    say \"b\"\nfinally\n    say \"c\"\nend\n",
    "set caught to no\ntry\n    say 1 + \"a\"\ncatch error\n    say caught\nend\n",
    "set caught to no\ntry\n    say [1][7]\ncatch error\n    set caught to yes\nend\nsay caught\n",
    "set caught to no\ntry\n    say (1 + \"a\") + \"b\"\ncatch error\n    set caught to yes\nend\nsay caught\n",
    "set caught to no\ntry\n    say \"abc\"[0]\ncatch error\n    set caught to yes\nend\nsay caught\n",
];

fn runtime_errors(out: &mut Vec<(String, String)>) {
    for source in RUNTIME_ERRORS {
        push(out, "runtime-errors", source);
    }
}

/// The same errors the runtime-errors family catches, uncaught, with a `say` in
/// front so the harness has to record what was printed *before* the failure.
const FAULTS_FAMILY: &[&str] = &[
    "say \"before\"\nsay 1 + \"a\"\n",
    "say \"before\"\nsay [1, 2][9]\n",
    "say \"before\"\nsay 1 / 0\n",
    "say \"before\"\nsay 9 % 0\n",
    "say \"before\"\nsay no_such_function()\n",
    "say \"before\"\nsay length(nothing)\n",
    "say \"before\"\nsay nothing.field\n",
    "say \"before\"\nsay [1][4]\n",
    "say \"before\"\nsay 1 - \"a\"\n",
    "say \"before\"\nsay yes + 1\n",
    "say \"before\"\nsay nothing + 1\n",
    "say \"before\"\nsay \"abc\"[0]\n",
    "say \"before\"\nsay 1[0]\n",
    "    say \"indented before\"\n    say 1 + \"a\"\n",
    "        say \"deeper\"\n        say 1 / 0\n",
    "say \"a\"\nsay \"b\"\nsay \"c\"\nsay 1 + \"a\"\n",
    "set xs to [1, 2]\nsay \"before\"\nsay xs[9]\n",
    "say \"before\"\nset r to {}\nsay r.missing.deep\n",
    "say \"before\"\nsay length(1)\n",
];

fn faults_family(out: &mut Vec<(String, String)>) {
    for source in FAULTS_FAMILY {
        push(out, "faults", source);
    }
}

const MALFORMED: &[&str] = &[
    "say \"unterminated\n",
    "set x to\n",
    "set to 5\n",
    "set x 5\n",
    "say 1\nend\n",
    "end\n",
    "if 1 is 1 then\n    say 1\n",
    "for each x in [1]\n    say x\n",
    "repeat 3 times\n    say 1\n",
    "repeat 3\nend\n",
    "say (1 + 2\n",
    "set xs to [1, 2\n",
    "set r to {a: 1\n",
    "set r to {1}\n",
    "say {a: }\n",
    "say\n",
    "for each in [1]\n    say 1\nend\n",
    "for each x in\n    say x\nend\n",
    "repeat times\n    say 1\nend\n",
    "say 1 }\n",
    "if is 1 then\n    say 1\nend\n",
    "say @\n",
    "set x to = 5\n",
    "to f(\n    give back 1\nend\n",
    "say 1 === 2\n",
    "say 1 <> 2\n",
    "for each x in [1] from 2\n    say x\nend\n",
    "if 1 is 1\n    say 1\nend\n",
    "if yes\nend\n",
    "say [1, 2][0\n",
    "say unknown_variable\n",
    "set xs to [1]\nsay xs[\n",
    "object\nend\n",
    "object O\n    has\nend\n",
    "say 1 +\n",
    "set to\n",
    "try\n    say 1\n",
    "catch error\n    say 1\nend\n",
    "module\nend\n",
    "say )\n",
    "say 1 to 2\n",
    "to f(a b)\n    give back 1\nend\n",
    "unless\n    say 1\nend\n",
    "say 1 % \n",
    "set xs to 1[0]\n",
    "give back\n",
];

fn malformed(out: &mut Vec<(String, String)>) {
    for source in MALFORMED {
        push(out, "malformed", source);
    }
}

/// Programs whose **last line is a value**, or a block whose value is read.
///
/// A program ending in a bare expression is worth something, and the bytecode VM
/// used to answer `nothing` for every one of them: the top-level frame is not a
/// call, and a frame that is not a call threw away whatever it finished with.
/// The family exists so that stays fixed.
const VALUE_TAILS: &[&str] = &[
    "set t to 5\nsay t\nt\n",
    "set t to \"x\"\nt\n",
    "1 + 2\n",
    "1 - 2\n",
    "2 * 3\n",
    "6 / 2\n",
    "7 % 3\n",
    "\"a\" + \"b\"\n",
    "yes\n",
    "no\n",
    "nothing\n",
    "[]\n",
    "[1, 2, 3]\n",
    "{}\n",
    "{a: 1}\n",
    "{a: 1, b: \"x\"}\n",
    "length([1, 2])\n",
    "length(\"abc\")\n",
    "type_of(1)\n",
    "uppercase(\"a\")\n",
    "-5\n",
    "-0.5\n",
    "0\n",
    "0.0\n",
    "set xs to [1, 2]\nxs[1]\n",
    "set r to {a: 7}\nr.a\n",
    "set r to {a: 7}\nr.missing\n",
    "set r to {}\nset r.a to 9\nr.a\n",
    "to one()\n    give back 1\nend\none()\n",
    "to add(a, b)\n    give back a + b\nend\nadd(1, 2)\n",
    "say \"printed\"\n1\n",
    "set n to 0\nrepeat 3 times\n    set n to n + 1\nend\nn\n",
    "for each x in [1, 2]\n    say x\nend\n2\n",
    "if yes then\n    7\nend\n",
    "unless no then\n    8\nend\n",
    "if no then\n    1\nelse\n    2\nend\n",
    "if 1 is 2 then\n    if 2 is 2 then\n        3\n    end\nend\n",
    "try\n    4\ncatch error\n    5\nend\n",
    "set xs to [[1], [2]]\nxs[0]\n",
    "set xs to [{a: 1}]\nxs[0].a\n",
    "\u{feff}9\n",
    "say \"a\"\nsay \"b\"\n\"c\"\n",
    // Trailing tokens are accepted and the last one becomes the program's value;
    // `text-ops-0023` is the `say` shape of the same thing.
    "set x to 1 2\nx\n",
];

fn value_tails(out: &mut Vec<(String, String)>) {
    for source in VALUE_TAILS {
        push(out, "value-tails", source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A source that binds every name the arms read, and nothing else.
    fn binders() -> String {
        format!(
            "set {NUMBER} to 3\nset {TEXT} to \"abc\"\nset {YESNO} to yes\n\
             set {LIST} to [1, 2]\nset {RECORD} to {{a: 1, b: \"x\"}}\n"
        )
    }

    /// What one arm is worth: the arm alone, bound, as a program's value.
    fn worth(arm: Arm, seed: u64) -> Option<super::super::vm::Typed> {
        let mut rng = Rng::new(seed);
        let source = format!("{}{}\n", binders(), arm.render(&mut rng));
        crate::common::vm::tree_walk(&source).result.ok()
    }

    #[test]
    fn edge_every_kind_is_reachable_and_its_arms_produce_it() {
        for arm in ARMS {
            let seed = 0x9E37u64 ^ (*arm as u64);
            let value = worth(*arm, seed)
                .unwrap_or_else(|| panic!("{arm:?} produced no value at seed {seed}"));
            let expected = match arm.kind() {
                Kind::Number => "number",
                Kind::Text => "text",
                Kind::YesNo => "yesno",
                Kind::List => "list",
                Kind::Record => "record",
            };
            assert_eq!(
                value.type_name(),
                expected,
                "{arm:?} declares Kind::{:?} but produced {value:?}",
                arm.kind(),
            );
        }
    }

    #[test]
    fn edge_every_arm_is_reachable_from_more_than_one_seed() {
        // An arm that only works for the seed this test happens to pick is an
        // arm that will fail on some other seed and look like a language bug.
        for arm in ARMS {
            let mut drawn = 0;
            for delta in 0..64u64 {
                if worth(*arm, delta).is_some() {
                    drawn += 1;
                }
            }
            assert_eq!(
                drawn, 64,
                "{arm:?} produced a value on only {drawn}/64 seeds"
            );
        }
    }

    #[test]
    fn edge_every_declared_fault_has_a_tail_that_faults() {
        let mut messages = Vec::new();
        for fault in FAULTS {
            let source = format!("{}{}\n", binders(), fault.tail());
            let outcome = crate::common::vm::tree_walk(&source);
            let Some(message) = fault.message() else {
                panic!("{fault:?} must declare the message it produces");
            };
            let failure = outcome
                .result
                .expect_err(&format!("{fault:?} must fail, tail {:?}", fault.tail()));
            let text = crate::common::vm::show(&failure);
            assert!(
                text.contains(message),
                "{fault:?} tail {:?} failed with {text:?}, which does not name {message:?}",
                fault.tail(),
            );
            messages.push(text);
        }
        messages.sort();
        messages.dedup();
        assert_eq!(
            messages.len(),
            FAULTS.len(),
            "two faults produce the same failure, so the corpus cannot tell them \
             apart: {messages:?}",
        );
    }

    #[test]
    fn edge_every_declared_fault_is_reachable_from_some_seed() {
        let mut seen: Vec<Fault> = (0..400u64).map(|seed| generate(seed).fault).collect();
        seen.sort_by_key(|f| f.name());
        seen.dedup_by_key(|f| f.name());
        for fault in FAULTS {
            assert!(
                seen.contains(fault),
                "no seed in 0..400 draws {}: {fault:?}",
                fault.name(),
            );
        }
        assert!(
            seen.contains(&Fault::None),
            "and no seed draws a program with no fault at all",
        );
    }

    #[test]
    fn edge_generated_programs_end_in_a_value_rather_than_in_a_statement() {
        for seed in 0..200u64 {
            let generated = generate(seed);
            let last = generated.source.lines().last().unwrap_or_default();
            let last = last.trim();
            assert!(
                !last.is_empty() && !last.ends_with("end"),
                "seed {seed} ends in {last:?}, which is not a value:\n{}",
                generated.source,
            );
            assert!(
                !generated.source.contains("a.out"),
                "seed {seed} writes a file: a generated program that touches the \
                 filesystem is not reproducible",
            );
        }
    }

    #[test]
    fn edge_almost_every_seed_produces_a_program_of_its_own() {
        let mut sources: Vec<String> = (0..200u64).map(|seed| generate(seed).source).collect();
        let total = sources.len();
        sources.sort();
        sources.dedup();
        assert_eq!(
            sources.len(),
            total,
            "of {total} seeds, {} produced a source another seed also produced",
            total - sources.len(),
        );
    }

    #[test]
    fn edge_the_generator_is_a_function_of_nothing() {
        for seed in [0u64, 1, 7, 999, u64::MAX] {
            assert_eq!(
                generate(seed),
                generate(seed),
                "seed {seed} is not replayable"
            );
        }
    }

    #[test]
    fn edge_the_generator_emits_every_statement_form_it_lists() {
        let mut saw_say = false;
        let mut saw_if = false;
        let mut saw_repeat = false;
        let mut saw_false_if = false;
        for seed in 0..200u64 {
            let source = generate(seed).source;
            saw_say |= source.contains("    say ");
            saw_if |= source.contains(") then\n");
            saw_repeat |= source.contains("repeat ");
            saw_false_if |= source.contains("and no) then");
        }
        assert!(saw_say, "no drawn program says anything");
        assert!(saw_if, "no drawn program branches on a comparison");
        assert!(saw_repeat, "no drawn program loops");
        assert!(
            saw_false_if,
            "no drawn program takes the false branch, so a broken condition is invisible",
        );
    }

    #[test]
    fn edge_a_text_literal_escapes_what_a_source_cannot_hold_raw() {
        // A newline in a text literal would end the statement, so the arm that
        // can produce one has to escape it.
        for word in WORDS {
            assert!(
                !word.contains('\n') && !word.contains('"'),
                "{word:?} cannot be written inside a Redblue string literal",
            );
        }
        let source = format!(
            "{}{}\n",
            binders(),
            Arm::TextLiteral.render(&mut Rng::new(0))
        );
        assert!(source.contains("set t0 to \""), "{source}");
        assert_eq!(
            source.lines().filter(|l| l.starts_with("set t0")).count(),
            1
        );
    }

    #[test]
    fn edge_the_binders_and_readers_agree() {
        let generated = generate(3);
        for name in [NUMBER, TEXT, YESNO, LIST, RECORD] {
            assert!(
                generated.source.contains(&format!("set {name} to ")),
                "the grammar reads {name} but does not bind it:\n{}",
                generated.source,
            );
        }
    }

    #[test]
    fn edge_the_corpus_is_a_function_of_nothing() {
        let first = corpus_sources();
        let second = corpus_sources();
        assert_eq!(first, second, "two calls must produce the same corpus");

        let mut names: Vec<String> = first.iter().map(|(name, _)| name.clone()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), first.len(), "two programs share a name");

        let mut sources: Vec<String> = first.iter().map(|(_, source)| source.clone()).collect();
        let total = sources.len();
        sources.sort();
        sources.dedup();
        assert_eq!(
            sources.len(),
            total,
            "of {total} programs, {} are duplicates of another",
            total - sources.len(),
        );
    }

    #[test]
    fn edge_the_corpus_holds_every_family_it_lists() {
        let sources = corpus_sources();
        let mut families: Vec<&str> = Vec::new();
        for (name, _) in &sources {
            let family = name.rsplit_once('-').map(|(f, _)| f).unwrap_or(name);
            families.push(family);
        }
        families.sort();
        families.dedup();
        assert_eq!(
            families.len(),
            16,
            "the corpus holds {} families: {families:?}",
            families.len(),
        );
        assert!(sources.len() >= 200, "the corpus holds {}", sources.len());
    }
}
