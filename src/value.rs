use std::fmt;
use std::sync::Arc;

use indexmap::IndexMap;

use crate::bytecode::Chunk;
use crate::error::{Error, Result, Span};
use crate::parser::Stmt;

/// Field storage for records and objects.
///
/// `IndexMap` is used rather than `HashMap` so that field order is the order
/// the fields were inserted in. A `HashMap` iterates in an arbitrary, per-map
/// random order, which made `say` and `json.stringify` emit the same record
/// differently on every run.
pub type Fields = IndexMap<String, Value>;

/// One lexical scope's bindings, captured when a function value was declared.
///
/// `IndexMap` for the same reason [`Fields`] uses it: a captured scope's field
/// order must not vary between runs, so two closures that closed over equal
/// bindings always compare equal.
pub type CapturedScope = IndexMap<String, Value>;

/// The local scopes a function value closed over, outermost first.
///
/// A declaration is executed with every enclosing local scope live, and that
/// stack is what the declaration captures. Globals are deliberately absent:
/// they live in the interpreter, not on the stack, and a function that reads
/// one at call time must see the value the program has by then rather than the
/// value it had where the function was written.
pub type Captured = Vec<CapturedScope>;

/// What a function value runs when it is called.
///
/// Two VMs exist and a program may be run by either, so a function value has to
/// be able to hold either representation of a body. This is the whole of the
/// difference between them: the tree-walker interprets the statements, and the
/// bytecode VM runs the compiled block, and everything else about a call — how
/// the parameters are bound, how the captured scopes are restored, how deep a
/// call chain may be — is the same.
#[derive(Debug, Clone)]
pub enum FunctionBody {
    /// Statements, as the tree-walking VM wants them.
    Statements(Arc<Vec<Stmt>>),
    /// A compiled block, named by the chunk it was compiled from and the path of
    /// child-block indexes that leads to it from that chunk's `main`.
    ///
    /// A path and a shared chunk rather than a reference because a `Value` may
    /// outlive the run that built it — it can be stored in a record, put in a
    /// list, or returned from a function — and because a module's blocks live in
    /// a chunk of their own. The bytecode VM reads the path against the chunk.
    Block {
        /// The compiled program the block belongs to.
        chunk: Arc<Chunk>,
        /// Child-block indexes from that program's `main`.
        path: Vec<u32>,
    },
}

impl FunctionBody {
    /// The chunk and path that reach the block, or `None` for a body that is
    /// statements rather than a compiled block.
    pub fn block(&self) -> Option<(&Arc<Chunk>, &[u32])> {
        match self {
            FunctionBody::Statements(_) => None,
            FunctionBody::Block { chunk, path } => Some((chunk, path)),
        }
    }
}

/// A function value: a body, its parameters, and the bindings the declaration
/// closed over.
///
/// Before this existed, `Value::Function` held a name and a parameter list and
/// the body lived in a single flat map keyed by that name, so a nested
/// declaration lost its enclosing bindings and two nested declarations of the
/// same name overwrote each other.
#[derive(Debug, Clone)]
pub struct FunctionValue {
    /// The name in the declaration. Used for display and error messages.
    pub name: String,
    /// The declared parameter names, in order.
    pub params: Vec<String>,
    /// The declared body, in whichever representation the caller that built this
    /// function value works in.
    ///
    /// Behind an `Arc` because reading any variable clones the value bound to
    /// it, and a recursive call reads the name it was reached through. Cloning
    /// a function value must therefore not copy the body.
    pub body: FunctionBody,
    /// The local scopes live where the declaration was executed.
    pub captured: Arc<Captured>,
}

/// Two function values are the same function when they were declared with the
/// same name and parameters and closed over equal bindings.
///
/// The body is not compared: `Stmt` has no equality, and every function value
/// built by one declaration shares that body anyway. Under capture-by-value two
/// closures from the same declaration over equal environments are the same
/// function value, which is what comparing them answers.
impl PartialEq for FunctionValue {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.params == other.params && self.captured == other.captured
    }
}

/// `2^53`, the largest magnitude at which every whole number is still held
/// exactly by an `f64`.
///
/// A Redblue `number` is an `f64`, so a wider integer is rounded on the way
/// in: `9007199254740993` reads as `9007199254740992`. Printing such a value
/// through the `i64` path would state a whole number the program never held, so
/// the display of a whole number outside this range uses the full `f64` form
/// instead.
pub const MAX_EXACT_INT: f64 = 9_007_199_254_740_992.0;

/// The words `say` prints for the three numbers that are not finite.
///
/// No Redblue program can produce one — `Value::number` rejects them — but
/// `Value::Number` is a public variant, so a Rust caller can build
/// `Value::Number(f64::NAN)` and the display still has to be defined.
pub fn non_finite_display(n: f64) -> Option<&'static str> {
    if n.is_finite() {
        None
    } else if n.is_nan() {
        Some("not a number")
    } else if n > 0.0 {
        Some("infinity")
    } else {
        Some("negative infinity")
    }
}

/// The short name of a number that is not finite, for error messages.
///
/// Only meaningful where `non_finite_display` would return `Some`: a finite
/// number has no such name and gives the empty string.
pub fn non_finite_name(n: f64) -> &'static str {
    if n.is_finite() {
        ""
    } else if n.is_nan() {
        "NaN"
    } else if n > 0.0 {
        "infinity"
    } else {
        "-infinity"
    }
}

/// The same policy as [`Value::number`], for a number that stays a number.
///
/// A loop counter is an `f64` and not a `Value`, but it is computed, so it is
/// held to the same rule: a step that overflows the counter fails rather than
/// leaving an infinity in it.
pub fn finite_number(n: f64, span: Span) -> Result<f64> {
    if n.is_finite() {
        Ok(n)
    } else {
        Err(Error::Runtime(
            format!("{} is not a finite number", non_finite_name(n)),
            span,
        ))
    }
}

/// The bound of a `for each x from <argument> ...` loop, refused when it is
/// not a number.
///
/// `from`, `to` and `by` are all bounds of the same arithmetic, so all three
/// are named in the failure. Silently skipping a loop whose bounds are text
/// would leave the reader with a program that runs and does nothing.
pub fn expect_range_number(value: &Value, argument: &str, span: Span) -> Result<f64> {
    match value {
        Value::Number(n) => Ok(*n),
        other => Err(Error::Runtime(
            format!(
                "The '{}' value of a range loop must be a number, but it is {}",
                argument,
                other.type_name()
            ),
            span,
        )),
    }
}

/// The number of turns a `repeat <count> times` loop takes.
///
/// A loop needs a whole number of turns, so the count — an `f64` like every
/// number — is turned into one here rather than by the `as i64` cast each VM
/// used with its own `_ => 0` beside it. Two engines that each narrowed a count
/// themselves agree only by accident, and the shapes a cast cannot answer are
/// the ones where they had least reason to:
///
/// - A count that is not a number is not a loop at all, and runs no times. That
///   is the behaviour `for each x in 5` already has, and `repeat "five" times`
///   is pinned to it.
/// - A count is the number of *whole* turns before the fraction, so
///   `repeat 2.5 times` runs twice and `repeat -5 times` runs no times: there is
///   no turn before the first one for a negative count to be counted back from.
/// - A count past [`i64::MAX`] saturates rather than refusing, because the bound
///   on turns is [`crate::interpreter::MAX_ITERATIONS`] and not the width of this counter:
///   such a loop is stopped by the iteration cap, which names the limit that
///   stopped it, or left by a `break` on its first turn.
/// - A count that is not finite is refused by [`finite_number`], the refusal
///   every computed number gets. A Redblue program cannot write one — the number
///   door refuses it before a loop ever sees it — so this is the second door, and
///   it is here so that the first one is not the only thing between a `NaN` and
///   a turn count.
///
/// Both VMs call this, so a count cannot be read two ways.
pub fn expect_repeat_count(value: &Value, span: Span) -> Result<i64> {
    let count = match value {
        Value::Number(n) => *n,
        _ => return Ok(0),
    };
    let count = finite_number(count, span)?;
    // `trunc` towards zero is what leaves `-5` at zero rather than at a negative
    // turn count, which a loop cannot have.
    let turns = count.trunc();
    if turns <= 0.0 {
        return Ok(0);
    }
    if turns >= i64::MAX as f64 {
        return Ok(i64::MAX);
    }
    Ok(turns as i64)
}

/// Whether a `for each x from current to end by step` range has another value
/// to visit.
///
/// The step's sign decides the direction: a positive step counts up while
/// `current <= end`, a negative one counts down while `current >= end`. A zero
/// step is not special-cased here — it never reaches `end`, so the caller's
/// iteration guard stops the loop, which is the same error a `while` that never
/// ends gives.
pub fn range_has_next(current: f64, end: f64, step: f64) -> bool {
    if step < 0.0 {
        current >= end
    } else {
        current <= end
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nothing,
    Number(f64),
    Text(String),
    YesNo(bool),
    List(Vec<Value>),
    Record(Fields),
    Object(String, Fields),
    Function(FunctionValue),
    Builtin(String),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Nothing => write!(f, "nothing"),
            Value::Number(n) => {
                if let Some(text) = non_finite_display(*n) {
                    write!(f, "{}", text)
                } else if n.fract() == 0.0 && n.abs() <= MAX_EXACT_INT {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{}", n)
                }
            }
            Value::Text(s) => write!(f, "{}", s),
            Value::YesNo(b) => write!(f, "{}", if *b { "yes" } else { "no" }),
            Value::List(items) => {
                let items: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                write!(f, "[{}]", items.join(", "))
            }
            Value::Record(fields) => {
                let fields: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v))
                    .collect();
                write!(f, "{{{}}}", fields.join(", "))
            }
            Value::Function(function) => write!(f, "<function {}>", function.name),
            Value::Builtin(name) => write!(f, "<builtin {}>", name),
            Value::Object(_, _) => write!(f, "<object>"),
        }
    }
}

impl Value {
    /// Wraps `n` as a `Value::Number`, refusing the values that are not finite.
    ///
    /// This is the one door every computed number goes through, so `NaN`,
    /// `infinity` and `-infinity` cannot enter `Value::Number` silently:
    /// `5 % 0` and `1e308 * 1e308` are `Runtime` errors rather than values
    /// that print as `not a number` and compare false against everything.
    pub fn number(n: f64, span: Span) -> Result<Value> {
        finite_number(n, span).map(Value::Number)
    }

    /// The name `type_of` reports, and the name a `Runtime` error quotes when
    /// it has to say what it was handed instead of the type it wanted.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nothing => "nothing",
            Value::Number(_) => "number",
            Value::Text(_) => "text",
            Value::YesNo(_) => "yes/no",
            Value::List(_) => "list",
            Value::Record(_) => "record",
            Value::Object(_, _) => "object",
            Value::Function(_) => "function",
            Value::Builtin(_) => "builtin",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Nothing => false,
            Value::YesNo(b) => *b,
            Value::Number(n) => *n != 0.0,
            Value::Text(s) => !s.is_empty(),
            Value::List(items) => !items.is_empty(),
            Value::Record(_) => true,
            Value::Object(_, _) => true,
            Value::Function(_) => true,
            Value::Builtin(_) => true,
        }
    }
}
