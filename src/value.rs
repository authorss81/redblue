use std::fmt;
use std::sync::Arc;

use indexmap::IndexMap;

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
    /// The declared body.
    ///
    /// Behind an `Arc` because reading any variable clones the value bound to
    /// it, and a recursive call reads the name it was reached through. Cloning
    /// a function value must therefore not copy the body.
    pub body: Arc<Vec<Stmt>>,
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
