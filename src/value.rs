use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use chrono::{NaiveDateTime, TimeDelta};
use indexmap::IndexMap;

use crate::bytecode::Chunk;
use crate::error::{Error, Result, Span};
use crate::parser::Stmt;
use crate::runtime::LruCache;

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
    /// An identity unique to this function value.
    ///
    /// Name, parameters and captured bindings are not an identity: a factory
    /// executed twice can build two closures with the same name and the same
    /// parameter list that closed over different values, and a memoisation
    /// cache keyed by name alone would let one answer for the other. Cloning a
    /// value keeps this number, so every reference to one declaration's
    /// closure shares its memory, while a second declaration never does.
    pub id: usize,
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
    /// The memory an `lru_cache` declaration gave this function, shared by every
    /// clone of it.
    ///
    /// Kept here rather than in a table on the VM so that it dies with the last
    /// reference to the function. A declaration inside a loop creates a new
    /// function value on every turn; a VM-level table keyed by identity would
    /// keep every turn's cache alive for the rest of the run even though no
    /// value could reach it.
    cache: Arc<OnceLock<Mutex<LruCache>>>,
}

/// The next identity [`FunctionValue::new`] hands out.
///
/// One counter for the process rather than per run: an identity has to be
/// unique across two runs sharing one VM, and the value is only used as a cache
/// key, never printed, so a number that differs between processes changes no
/// observable answer.
static NEXT_FUNCTION_ID: AtomicUsize = AtomicUsize::new(1);

impl FunctionValue {
    /// Builds a function value with a fresh identity.
    pub fn new(
        name: String,
        params: Vec<String>,
        body: FunctionBody,
        captured: Arc<Captured>,
    ) -> Self {
        Self {
            id: NEXT_FUNCTION_ID.fetch_add(1, Ordering::Relaxed),
            name,
            params,
            body,
            captured,
            cache: Arc::new(OnceLock::new()),
        }
    }

    /// Gives this function a memory of `capacity` entries, if it has none yet.
    pub fn attach_cache(&self, capacity: usize) {
        if self.cache.get().is_none() {
            self.cache
                .set(Mutex::new(LruCache::new(capacity)))
                .expect("an empty cache slot accepts a cache");
        }
    }

    /// Gives this function an empty memory of `capacity` entries.
    pub fn with_cache(self, capacity: usize) -> Self {
        self.attach_cache(capacity);
        self
    }

    /// Whether this function has an `lru_cache` memory.
    pub fn has_cache(&self) -> bool {
        self.cache.get().is_some()
    }

    /// Drops every answer this function remembers, keeping its capacity.
    pub fn reset_cache(&self) {
        let Some(cache) = self.cache.get() else {
            return;
        };
        cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    /// The answer this function remembers for `args`, if any.
    pub fn cached(&self, args: &[Value]) -> Option<Value> {
        let cache = self.cache.get()?;
        let mut cache = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.get(args)
    }

    /// Records `value` as this function's answer for `args`.
    pub fn remember(&self, args: &[Value], value: Value) {
        let Some(cache) = self.cache.get() else {
            return;
        };
        let mut cache = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.insert(args, value);
    }
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

/// Drops every `lru_cache` answer reachable from `value`.
///
/// A run starts with empty memories even when the VM still holds a function it
/// declared in an earlier run. The caches live in the function values now, so
/// clearing them means walking the bindings a program can still reach rather
/// than clearing one table on the VM.
pub fn reset_caches(value: &Value) {
    let mut seen = std::collections::HashSet::new();
    reset_caches_in(value, &mut seen);
}

fn reset_caches_in(value: &Value, seen: &mut std::collections::HashSet<usize>) {
    match value {
        Value::Function(function) => {
            if !seen.insert(function.id) {
                return;
            }
            function.reset_cache();
            for scope in function.captured.iter() {
                for captured in scope.values() {
                    reset_caches_in(captured, seen);
                }
            }
        }
        Value::List(items) => {
            for item in items.iter() {
                reset_caches_in(item, seen);
            }
        }
        Value::Record(fields) | Value::Object(_, fields) => {
            for field in fields.values() {
                reset_caches_in(field, seen);
            }
        }
        _ => {}
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

#[derive(Debug, Clone)]
pub enum Value {
    Nothing,
    Number(f64),
    Text(String),
    YesNo(bool),
    List(Shared<Vec<Value>>),
    Record(Shared<Fields>),
    Object(String, Shared<Fields>),
    Function(FunctionValue),
    Builtin(String),
    /// A timezone-naive calendar date and time.
    ///
    /// There is no zone behind it: every component is read and written as the
    /// same wall-clock reading, so adding 24 hours is always one day and no
    /// daylight-saving transition can make an hour disappear. See SPEC.md §
    /// time for the stance.
    Date(NaiveDateTime),
    /// A signed span of time, as the difference between two dates.
    Duration(TimeDelta),
}

/// Value equality, with one exception to structural comparison: two deques
/// made by `collections` compare by the items they hold, in order.
///
/// A deque's `front` and `back` are persistent stack trees, and the same
/// sequence can be split across them differently depending on how it was built.
/// Comparing those fields structurally would make a deque from
/// `collections.deque([1, 2])` unequal to one made by two `push_right` calls,
/// leaking the construction history through `is`, `==`, Counter keys and
/// `collections.get`. The runtime answers the logical comparison; an object that
/// is not a well-formed deque falls back to the structural rule below.
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Nothing, Value::Nothing) => true,
            (Value::Number(left), Value::Number(right)) => left == right,
            (Value::Text(left), Value::Text(right)) => left == right,
            (Value::YesNo(left), Value::YesNo(right)) => left == right,
            (Value::List(left), Value::List(right)) => left == right,
            (Value::Record(left), Value::Record(right)) => left == right,
            (Value::Object(left_name, left_fields), Value::Object(right_name, right_fields)) => {
                if left_name == "deque" && right_name == "deque" {
                    if let Some(equal) = crate::runtime::deques_equal(self, other) {
                        return equal;
                    }
                }
                left_name == right_name && left_fields == right_fields
            }
            (Value::Function(left), Value::Function(right)) => left == right,
            (Value::Builtin(left), Value::Builtin(right)) => left == right,
            (Value::Date(left), Value::Date(right)) => left == right,
            (Value::Duration(left), Value::Duration(right)) => left == right,
            _ => false,
        }
    }
}

/// Storage a list or a record's contents live in, shared rather than owned.
///
/// This is the one place Redblue's value semantics are implemented, and the
/// reason a *copy* of a list is not a *traversal* of it. Every value a program
/// binds is copied when it is read — a variable read, an argument, a returned
/// value — and with an owned `Vec` each of those copies walked the whole list,
/// so a loop that indexes a list of n cost n per index and a loop that appended
/// to one cost n per append. Redblue has no way to observe the difference: an
/// append still produces a new list for every other binding to see, because
/// [`Arc::make_mut`] copies whenever the storage is shared and only writes
/// through when the value holds the last handle.
///
/// The rule that keeps the meaning of a program unchanged is therefore: **read
/// by cloning the handle, write through [`Arc::make_mut`] and never through
/// `&mut`**. A `&mut Vec` reaching an element of a shared list is the one way
/// to make a change a program cannot see through the name it made it with.
pub type Shared<T> = std::sync::Arc<T>;

impl Value {
    /// A list holding `items`.
    pub fn list(items: Vec<Value>) -> Value {
        Value::List(Shared::new(items))
    }

    /// A record holding `fields`.
    pub fn record(fields: Fields) -> Value {
        Value::Record(Shared::new(fields))
    }

    /// An object of type `name` holding `fields`.
    pub fn object(name: String, fields: Fields) -> Value {
        Value::Object(name, Shared::new(fields))
    }

    /// Appends `value` to this list if it is a list, growing the storage in
    /// place when this value holds the only handle to it and copying it when it
    /// does not.
    ///
    /// This is the whole of the growth contract in one place, and both callers
    /// of a growing list go through it: `push`, which returns the grown list, and
    /// `append`, which writes it back through a name. What it never does is
    /// grow a list another binding can see, which is what makes sharing the
    /// storage invisible.
    pub fn append(&mut self, value: Value) -> bool {
        match self {
            Value::List(items) => {
                Shared::make_mut(items).push(value);
                true
            }
            _ => false,
        }
    }
}

/// The text a date prints as: `YYYY-MM-DD HH:MM:SS`, with the fractional
/// second appended when the value holds one.
///
/// `%.f` writes the fraction only when it is non-zero, and uses the shortest
/// of three, six or nine digits that keeps every digit, so a date with no
/// fraction keeps the text it always had. This is the one text form `say`,
/// `to_text` and JSON use, so a parsed fraction survives a round trip through
/// `to_text` instead of being silently dropped.
pub(crate) fn date_text(date: NaiveDateTime) -> String {
    date.format("%Y-%m-%d %H:%M:%S%.f").to_string()
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
            Value::Date(date) => write!(f, "{}", date_text(*date)),
            Value::Duration(duration) => write!(f, "{}", display_duration(*duration)),
        }
    }
}

/// The words `say` prints for a duration, such as `1 day, 2 hours, 3 seconds`.
///
/// Zero is `0 seconds` and a negative span carries one leading `-`. Every
/// component is read from the absolute span, so `-90000` seconds is
/// `-1 day, 1 hour` rather than a mixture of signs.
fn display_duration(duration: TimeDelta) -> String {
    if duration.is_zero() {
        return "0 seconds".to_string();
    }
    let negative = duration < TimeDelta::zero();
    let span = duration.abs();
    let days = span.num_days();
    let hours = span.num_hours() % 24;
    let minutes = span.num_minutes() % 60;
    let seconds = span.num_seconds() % 60;
    let nanos = span.subsec_nanos();

    let mut parts = Vec::new();
    let mut push = |amount: i64, unit: &str| {
        if amount != 0 {
            parts.push(format!(
                "{} {}{}",
                amount,
                unit,
                if amount == 1 { "" } else { "s" }
            ));
        }
    };
    push(days, "day");
    push(hours, "hour");
    push(minutes, "minute");
    push(seconds, "second");
    if nanos != 0 {
        parts.push(format!(
            "{} nanosecond{}",
            nanos,
            if nanos == 1 { "" } else { "s" }
        ));
    }
    format!("{}{}", if negative { "-" } else { "" }, parts.join(", "))
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
            Value::Date(_) => "date",
            Value::Duration(_) => "duration",
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
            Value::Date(_) => true,
            Value::Duration(_) => true,
        }
    }
}
