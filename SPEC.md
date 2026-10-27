# Redblue Specification

**Version**: 1.0-draft  
**Status**: Design Phase  
**Last Updated**: March 2026

---

## Table of Contents

1. [Introduction](#introduction)
2. [Lexical Structure](#lexical-structure)
3. [Types](#types)
4. [Variables and Values](#variables-and-values)
5. [Expressions](#expressions)
6. [Statements](#statements)
7. [Functions](#functions)
8. [Objects](#objects)
9. [Modules](#modules)
10. [Error Handling](#error-handling)
11. [Asynchronous Programming](#asynchronous-programming)
12. [Standard Library](#standard-library)
13. [Grammar](#grammar)

---

## Introduction

Redblue is a general-purpose programming language designed for readability. Its syntax is based on natural English sentences, making code self-documenting and accessible to beginners while remaining powerful enough for professional software development.

### Design Goals

1. **Readability**: Code should be understandable by reading it aloud
2. **Simplicity**: Minimal keywords, no symbolic clutter
3. **Power**: Full support for modern programming paradigms
4. **Performance**: Comparable to Python for typical workloads

### Hello World

```redblue
say "Hello, World!"
```

---

## Lexical Structure

### Identifiers

Identifiers name variables, functions, and objects.

```
identifier = letter { letter | digit | '_' }
letter = 'a'..'z' | 'A'..'Z' | unicode_letter
```

**Valid identifiers:**
```redblue
set name to "Alice"
set _private to 10
set camelCase to 20
set 名前 to "Japanese"  // Unicode allowed
```

### Keywords

Redblue reserves 47 words, each with a rule in the parser that reads it:

| Category | Keywords |
|----------|----------|
| Variables | set, constant, to, is, nothing, yes, no |
| Control | if, then, else, end, unless |
| Loops | for, each, in, from, times, while, repeat, until |
| Jumps | break, skip, return, give back |
| Errors | might fail, try, catch, finally |
| Output | say, print |
| Modules | module, import, export, as |
| Logic | and, or, not, mod |
| Tests | test, expect |
| Objects | object, has, can, this, extends |

`give back` and `might fail` are two words each, which is why 47 words make 45
keyword forms. Every other word here is reserved on its own. A word that is not
on this list is an ordinary identifier; see the appendix at the end for the ones
that used to be listed and are not any more.

### Literals

```redblue
// Numbers
set n to 42
set f to 3.14
set neg to -10

// Text (Strings)
set s to "Hello, World!"
set multi to "Line 1
Line 2"

// Boolean
set flag to yes
set flag to no

// Nothing
set empty to nothing

// Lists
set arr to [1, 2, 3]
set words to ["hello", "world"]

// Records
set person to record {
    name: "Alice",
    age: 30
}
```

#### Escapes in a text literal

A text literal carries five one-character escapes and one that names a code
point:

| Written | Reads as |
|---|---|
| `\n` | line feed |
| `\t` | tab |
| `\r` | carriage return |
| `\"` | `"` |
| `\\` | `\` |
| `\uXXXX` | the character the four hex digits name |

```redblue
say "caf\u00e9"      // café — U+00E9, one character
say "A\u0042C"       // ABC
```

`\uXXXX` is *exactly* four hex digits, in either case. It is not padded, not
read past the fourth digit, and not braced: `\u1f600` is `\u1f60` followed by a
literal `0`, because four digits cannot name a code point above U+FFFF. A file
that can hold the character itself should write the character — the escape is
for the code points a file cannot otherwise carry.

Everything the grammar does not spell out is a `LexerError` naming the line and
column of the backslash, never a different string:

```redblue
say "\q"            // LexerError: Unknown escape '\q' in text
say "\uZZZZ"        // LexerError: Unicode escape needs four hex digits
say "\u00"          // LexerError: Unicode escape needs four hex digits
say "\ud83d"        // LexerError: \uD83D is half a surrogate pair; no character
say "abc\           // LexerError: Unterminated string
```

A half of a surrogate pair is refused because it names no character of its own:
a lone surrogate has no UTF-8 encoding, so the only thing a decoder could put
there is a character nobody wrote.

### Comments

```redblue
// Single line comment

// This is a multi-line
// comment spanning
// multiple lines
```

### Whitespace

Whitespace is ignored except as a statement separator. Use newlines or `end` to close blocks.

---

## Types

### Primitive Types

| Type | Description | Example |
|------|-------------|---------|
| `number` | Integers and decimals | `42`, `3.14`, `-10` |
| `text` | Unicode strings | `"Hello"` |
| `yes/no` | Boolean values | `yes`, `no` |
| `nothing` | Null/void | `nothing` |

### Complex Types

```redblue
// List of a type
set numbers to list of number
set names to list of text

// Record type
set Point to record { x: number, y: number }

// Object type
set Person to object
```

### Type Inference

Redblue infers types automatically:

```redblue
set x to 10        // x is number
set name to "Hi"   // name is text
```

### Explicit Types

```redblue
set x to 10 as number
set name to "Hi" as text
```

---

## Variables and Values

### Declaration

```redblue
set name to value
set x to 1
set greeting to "Hello"
```

### Assignment

```redblue
set name to new_value
```

### Constants

```redblue
constant PI to 3.14159
constant MAX_SIZE to 1000
```

---

## Expressions

### Arithmetic

```redblue
set sum to 1 + 2
set diff to 5 - 3
set product to 4 * 3
set quotient to 10 / 2
set remainder to 10 mod 3
```

#### Numeric semantics

A `number` is a 64-bit float. Every operation on numbers is **total**: it either
returns a number or fails with a runtime error. There is no third outcome in
which an operation returns something that is not a number.

| Expression | Outcome |
|---|---|
| `1 / 0`, `0 / 0`, `-1 / 0` | runtime error: `Division by zero` |
| `5 % 0`, `5 % 0.0`, `5 % -0.0` | runtime error: `Modulo by zero` |
| `-0.0` | the number `0`: negative zero is not a distinct value |
| `1e308 * 1e308` | runtime error: `infinity is not a finite number` |
| `1e400` (a literal too large to hold) | runtime error: `infinity is not a finite number` |
| `1e-400` (a literal too small to hold) | the number `0`: see "Underflow", below |
| `1.2.3`, `2..3`, `3e` (not a number) | lex error: `Invalid number '…'` |
| `-5 % 3` | `-2`: a remainder takes the sign of the dividend |
| `5-2` | `3`: a sign belongs to a literal only after its exponent |

Three consequences are worth stating plainly:

1. **A number is always finite.** `NaN`, `infinity` and `-infinity` cannot
   enter a `number`, from arithmetic, from a literal, from `json.parse`, or from
   a library function. There is no way to obtain one, so there is no value whose
   comparisons are all false.
2. **Whole numbers are exact only to 2^53.** `9007199254740993` reads as
   `9007199254740992`. A whole number wider than 2^53 is printed in full rather
   than through a 64-bit integer, so it never reports a number the program does
   not hold.
3. **Non-finite numbers still print.** A number that is not finite can only be
   built by a host program embedding Redblue, and it prints as `not a number`,
   `infinity` or `negative infinity`. `json.stringify` of one is `null`, which is
   what a JSON writer emits for it.

#### Literals

A number literal is digits, at most one `.`, then at most one exponent: an `e`,
an optional sign, and digits. Anything else is a mistake in the source and is
reported as a lex error naming the literal — never read as `0`.

```
5-2        // 5 minus 2, because the sign is not after an exponent
5 + 2      // the same, written with spaces
1e-3       // 0.001: here the sign is after the exponent, so it is part of it
2e-3       // 0.002
1.5e+2     // 150
.5         // 0.5
5.         // 5
1.2.3      // lex error: Invalid number '1.2.3'
```

#### Underflow

A literal too small to hold is **not** an error, because there is nothing
outside the reals below it to report: `1e-400` is the number `0`. Overflow is
different — the answer lies outside the reals and the operation fails rather
than naming a number the program does not hold. A `0` produced by underflow is
an ordinary `0` in every other respect, so it is an ordinary zero divisor:
`1 / 1e-400` is `Division by zero`.

### Comparison

```redblue
// Text comparison
if name is "Alice"
if x is greater than 10
if x is less than or equal to 100

// Alternative operators
if x > 10          // is greater than
if x >= 10         // is greater than or equal to
if x < 10          // is less than
if x <= 10         // is less than or equal to
if x == 10         // is equal to
if x != 10         // is not
```

### Logical

```redblue
if x is greater than 0 and x is less than 100
if status is "active" or status is "pending"
if not is_empty
```

### String Interpolation

```redblue
set greeting to "Hello, {name}!"
set message to "Value: {x + y}"
```

### Property Access

```redblue
set first to person.name
set x_coord to point.x
```

### Function Call

```redblue
set len to text.length("hello")
set result to math.sqrt(2)
```

### List Operations

```redblue
// Indexing
set first to items at 0
set last to items at -1

// Bracketed indexing — the same operation
set first to items[0]
set last to items[-1]

// Length
set count to length of items
set same to length(items)

// Contains
if "hello" is in words
```

`items at N` and `items[N]` are one indexing operation written two ways, and
one set of bounds behind both: an index that names no element is the same
runtime error either way, naming the index as it was written. A negative index
counts from the end, so `items at -1` is the last element — the same answer
`items[-1]` has always given.

The index is the **arithmetic** after `at`: `items at 0` and `items at n + 1`
index by the value of `0` and of `n + 1`. A comparison is not part of the index —
`items at 0 is 10` is `items at 0` compared with `10`, exactly as `items[0] is 10`
is — so `is`, `and` and `or` stay outside it. A second `at` is therefore part of
that index rather than a second step: `items at 1 at 0` asks for `1 at 0` first,
which is not a list, and fails `Cannot index non-list`. To step twice, bind the
step in between (`set row to items at 1` then `row at 0`), or write the bracketed
form `items[1][0]`.

`at` and `length` are ordinary names, not reserved words. `at` is read as an
index only when an operand follows it on the same line, so `set at to 0` and
`say at` are the variable they have always been and `items at at` is an index
whose operand is that variable. `length of` is read only as the two words
together, so `length(items)`, `say length`, and a program that binds its own
`length` are untouched. Neither form takes its operand from the next line:
`items at` at the end of a line is the variable `at`, and `length of` at the end
of a line is a syntax error where it is written.

`length of X` is `length(X)`, which is a builtin and nothing else: a text
answers its length in characters and a list answers its element count, and
anything else is the runtime error `length requires a list or text`.

`is in` tests **list membership**: the right side must be a `list`, and the
answer is `yes` when the left side equals one of its elements by value. There
is no substring form — `x is in "abc"` is a runtime error
`Right side of 'in' must be a list`, not a search — and no `is not in`;
write `if not (x is in haystack)`.

---

## Statements

### If-Then-Else

```redblue
if condition
    // statements
end

if condition
    // then branch
else
    // else branch
end

if x > 0
    say "positive"
else if x < 0
    say "negative"
else
    say "zero"
end
```

### When (Pattern Matching)

```redblue
when value
    case 1 then say "one"
    case 2 then say "two"
    else say "other"
end
```

### Unless

```redblue
unless is_valid
    say "Invalid!"
end
```

### For Loop (Iteration)

```redblue
for each item in items
    say item
end
```

### For Loop (Range)

```redblue
for each i from 1 to 10
    say i
end

// With step
for each i from 0 to 100 by 5
    say i
end

// A negative step counts down
for each i from 5 to 1 by -1
    say i
end
```

Both ends are inclusive: `from 3 to 3` visits 3 once, and a range whose step can
never reach its end — `from 5 to 1` — visits nothing at all.

The step decides the direction. A positive step counts up while the counter is
at or below `to`; a negative one counts down while it is at or above it. An
omitted step is `1`. A zero step never moves the counter, so the loop is endless
and is stopped by the same iteration limit every other loop obeys.

`from`, `to` and `by` must all be numbers. A bound that is not one is a
`RuntimeError` naming the argument and the type it was given, not a loop that
quietly runs zero times.

### Repeat Loop

```redblue
repeat 10 times
    say "Hello"
end
```

### While Loop

```redblue
while x > 0
    set x to x - 1
end
```

### Repeat Until

```redblue
repeat
    set x to x + 1
until x > 10
```

### Break and Skip

```redblue
for each item in items
    if item is nothing
        skip
    end
    process item
end

for each i from 1 to 100
    if i is 50
        break
    end
    say i
end
```

`break` leaves the loop it is written in and `skip` goes on to its next turn,
abandoning the rest of this one. Neither takes an operand: there is nothing for
one to say, because the loop they act on is the one around them and the turn they
go to is the next one. Either ends the block it was written in as well as the
loop, so the statements after one in the same `if` branch, `unless` body, `try`
body, `catch` body, `finally` body or `test` body are part of the turn the signal
stopped and do not run. A `finally` is still owed on the way out — an abrupt exit
from a protected region is not a failure — and the loop's variable is still bound
while it runs, because the turn it stopped has not ended until it has. Each block
is given its turn in that order too: the blocks the signal passes through are
finished one at a time, so a `finally` written inside one of them runs while that
block's scope is live and a `finally` written around all of them runs once they
are gone.

The loop is the one *around* the statement, wherever the statement was written: a
`break` in a `catch`, `finally`, `test`, `object` or `module` body inside a loop
leaves that loop. A module body is in it for the same reason an `object` body is:
the declaration runs where it is written, so a `break` in the module body around a
loop is a `break` in that loop — and the module is then left declaring nothing,
because the statements that would have published it did not run.

```redblue
set saved to nothing
for each name in names
    try
        set record to files.read("records/{name}.rb")
    catch error
        break
    end
    set saved to record
end
```

With no enclosing loop there is nothing to leave, and saying so is the point: a
`break` in no loop is a mistake in the program, and a mistake that runs to
completion reporting success is worse than one that stops with a message naming
the statement.

```redblue
break
```

```
RuntimeError: 'break' is only valid inside a loop
```

Two placements are refused for the same reason, and with the same message:

- **A function body.** A function body is not written inside the loop that calls
  it, so a `break` there is in no loop even when the call was made from one. The
  caller's loop survives the refusal and goes on to its next turn. A `module`
  declared inside a function body inherits this: its body is written inside a
  function body, so it is in no loop either.
- **An `object` body outside any loop.** The statements of an `object` body are run
  once, when the type is declared, so a `break` in one is a `break` in no loop. The
  same body written inside a loop leaves that loop, as above.

The refusal is a runtime error rather than a compile error, so `try ... catch
error` catches it like any other and the program carries on afterwards.

---

## Functions

### Declaration

```redblue
to greet(name)
    say "Hello, {name}!"
end
```

### Return Values

```redblue
to add(a, b)
    give back a + b
end

to max(a, b)
    if a > b
        give back a
    else
        give back b
    end
end
```

### Parameters

```redblue
to create_user(name, email, age)
    // parameters
end

// With default values
to greet(name, greeting default "Hello")
    say "{greeting}, {name}!"
end
```

### Type Annotations

```redblue
to add(a as number, b as number) as number
    give back a + b
end
```

### First-Class Functions

```redblue
set double to to (x) give back x * 2

set numbers to [1, 2, 3]
set doubled to numbers.map(double)
```

A function literal is an expression, so a function is a value: it can be held in
a name, passed as an argument, stored in a list or a record, and read back out
again. The block form — the literal written across several lines and closed with
its own `end` — means exactly the same thing:

```redblue
set double to to (x)
    give back x * 2
end
```

Both forms above capture the bindings live where they were written, by value and
not by reference: a literal defined inside a function sees that function's
locals, never the caller's.

### Closures

```redblue
to make_counter(start)
    set count to start
    give back to
        give back count
        add 1 to count
    end
end
```

---

## Objects

### Declaration

```redblue
object Person
    has name
    has age
    has email default nothing
    
    to introduce()
        say "I'm {this.name}"
    end
    
    to can email(message)
        // email implementation
    end
end
```

### Instantiation

Not implemented. There is no `new`: the word is an ordinary identifier, and
`new Person` reads as two identifiers in a row and is a parser error. A
declaration is its own prototype, so a program writes to the declaration where
it would otherwise construct one — see "Lookup order and shadowing".

```redblue
set Person.name to "Alice"
set Person.age to 30
```

### Constructor

A `to create` method is a constructor written on the declaration itself, and is
called as one. `this` is the prototype, so `give back this` hands back the
declaration rather than a fresh copy — see "Lookup order and shadowing".

```redblue
object Person
    has name
    has age
    
    to create(person_name, person_age)
        set this.name to person_name
        set this.age to person_age
        give back this
    end
end

set person to Person.create("Alice", 30)
```

### Inheritance

```redblue
object Employee extends Person
    has salary
    has department
    
    to get_bonus()
        give back this.salary * 0.1
    end
end
```

### Lookup order and shadowing

A declaration is both the type and its prototype: `object Person ... end` binds
the name `Person` to a record of the fields the declaration resolves to, so
`type_of(Person)` is `"record"` and `Person` is the value a method sees as
`this`.

Lookup order, for both fields and methods, is **nearest declaration first**: the
child's own `has` fields and `to can` methods, then the parent's, then the
grandparent's, and so on. The first declaration of a name in that order wins
and the further ones are not copied in, so a child field or method **shadows**
its parent's rather than merging with it. `Employee.role()` is the child's
`role`, and `Person.role()` is still the parent's. A field and a method of one
name are separate: `Employee.name` is the field, `Employee.name()` is the
method.

Four rules follow from that order:

| Rule | Why |
|---|---|
| The parent must be declared before the child | The chain is walked once, at declaration. An undeclared parent is an error, not an empty base. |
| The chain is acyclic by construction | `object A extends A` is a one-object cycle and is reported as one; a two-object cycle cannot be written, because `A` is already declared when `B` is and re-declaring a name is refused. |
| `has` twice keeps the later default | One field, one default, in the position of the first mention. |
| A write is a write to that prototype | `set Person.name to "Ada"` changes the record `Person` names. A child declared afterwards still starts from the *declared* defaults, not from those writes. |

A method writes to the object it was called on through `this`
(`set this.name to name`) and hands the result back with `give back this`, which
is the constructor SPEC.md § Constructor is written as. The write does not
escape to the declaration by itself: a record is a value, so a method that
mutates `this` and returns nothing leaves the prototype as it was.

A call is a method call when the receiver names a declared object, and a module
function (`receiver_function`, as `files.read` is) when it does not. `this` is
bound by a method call and by nothing else, so reaching for it in free code is
an error rather than an empty record. A method on a receiver that names no
object is not guessed at: a missing method and a non-object receiver are each
their own error.

### Properties

```redblue
object Circle
    has radius
    has color default "white"
    
    to area()
        give back PI * this.radius * this.radius
    end
end
```

---

## Modules

### Declaration

```redblue
module MathUtils
    constant PI to 3.14159
    
    to circle_area(radius)
        give back PI * radius * radius
    end
    
    export all
end
```

### Import

```redblue
import MathUtils

set area to MathUtils.circle_area(5)
```

### Import with Alias

```redblue
import MathUtils as M

set area to M.circle_area(5)
```

### Constants

`constant NAME to <expr>` binds a name to a value that no later assignment may
replace. It exists so a module can give its functions one shared value —
`PI`, a conversion rate, a prefix — that is written once.

```redblue
constant TAU to 6.28318

to radians_to_degrees(radians)
    give back radians * 180 / TAU
end
```

The rules:

- The value is the expression's value **where the declaration runs**, and every
  statement after it reads the name through ordinary lookup.
- A function body declared above the declaration reads the name, because a body
  runs when it is called. The call is what has to come after: a body called
  before the declaration runs before the name is bound, and the read is then the
  unknown variable any other name read too early gives.
- A name may hold one constant only. Declaring it twice is an error
  (`Constant 'TAU' is already declared`) and leaves the first value in place.
- No `set` rebinds a constant: `set TAU to 7` fails with
  `Cannot assign to constant 'TAU'`, including from inside a function body and
  including a module's own `set` bound by an `import`.
- Reading the name before its declaration is an error, the same unknown-variable
  error any other name read too early gives.
- A local of the same name — a parameter, a loop variable — shadows the constant
  inside its own scope and leaves it unchanged afterwards.

An `import` binds a module's `set` and `constant` names into the importing
program, where they read as ordinary names from the `import` onwards, and
importing the same module again is a no-op rather than a second binding of the
same names.

`rb compile` writes the declaration into the `.rbc` as its own instruction
(`DECLARE_CONST`, format version 3), not as the store a `set` compiles to, so a
compiled file says the name is read-only. See `docs/BYTECODE.md`.

---

## Error Handling

### Try-Catch

```redblue
try
    set data to files.read("config.rb")
    process data
catch error
    say "Error: {error}"
end
```

A `catch` may name what it caught, and that name is bound for the body of the
`catch` alone — not before the `try` and not after its `end`:

```redblue
try
    set data to files.read("config.rb")
catch error problem
    say "Error: {problem}"
end
```

The two spellings are one word apart and both are the same handler: `catch error`
binds a name spelled `error`, and `catch error problem` binds a name spelled
`problem`. `error` is a word to the lexer, not a keyword, so which of the two a
line is writing is decided by whether a second name follows it **on the same
line**. A `catch error` whose body starts on the next line is the first spelling,
whatever the body's first word is.

An untyped handler binds the name to the text `error` — the word that says a
failure was handled, and the whole of what such a handler can say about what it
caught. A handler that names a kind binds the failure itself instead, which is
what the next section is about.

### Handling Failures by Kind

A `try` has **one** `catch`. There is no second `catch` and no chain of handlers
to try in turn, but a handler can ask what kind of failure it caught and match on
it: `catch error of <Type>` runs its body only for the kinds `<Type>` names.

```redblue
try
    set record to files.read("records/cleo.rb")
catch error of Io problem
    say "could not read the record: {problem.message}"
    give back
end
```

A failure of any other kind is not handled here. A handler that does not match
does not run at all — the `finally` beside it is still owed — and the failure
belongs to whatever is written around this `try`: a `catch` there takes it, and a
program with nothing around it stops with it as its own.

The list of names is closed, and it is short:

| Name | The failures it matches |
|---|---|
| `Error` | every kind below — what an untyped `catch` matches, spelled as a match |
| `Runtime` | the program's own expression went wrong: a type mismatch, an index past the end of a list, a division by zero, a name that is not bound |
| `Io` | the host's filesystem and environment: a file that is not there, a path that cannot be read, a module that cannot be loaded |
| `Limit` | one of the host's resource limits: the step budget, the call-depth limit, or a loop's iteration cap |

Any other name is refused by the parser, which quotes the name that was written
and the four that exist — `catch error of FileError` does not compile into a
handler that matches no failure at all. So is `of` with nothing after it: the
kind is on the same line as the `of`, and `catch error of` alone is refused
rather than read as a handler binding the name `of`. The host's other kinds,
`Lexer`, `Parser` and `Analyzer`, are not in the list: they are raised before the
program runs, so there is no `try` inside a program that could catch one.

### The Caught Failure

A handler that names a kind binds the failure, and the failure is a record of
four fields:

| Field | What it holds |
|---|---|
| `kind` | the kind that was caught — `Runtime`, `Io` or `Limit` — even by a handler that matched `Error` |
| `message` | the failure's own message, which an untyped handler's `error` cannot give you |
| `line` | the line it happened on, or `0` for a failure with no position |
| `column` | the column it happened at, or `0` for a failure with no position |

```redblue
try
    set sum to 1 + "a"
catch error of Runtime problem
    say "{problem.kind} on line {problem.line}"
    say problem.message
end
```

```
Runtime on line 2
Cannot add non-numbers
```

The failure is a value like any other: it goes into a list, into a record, and
back out of a function. `0` is a line no source file has, which is how a failure
with no position says so without a second spelling. `line` is the position to
read on both engines — a `.rbc` instruction carries a line and no column, so the
two place a failure at a different column by their own convention.

The name a typed handler binds is an ordinary name, and it is optional:
`catch error of Runtime problem` binds `problem`, and `catch error of Runtime`
binds `error` — the same word the untyped handler binds it as, written where the
untyped handler writes it. What a typed handler binds is not the word, though: an
untyped handler's `error` is the text `error`, and a typed one's is the record
below.

The kind a failure was matched on is not the name it is *reported* under when
nothing caught it: an uncaught `Io` failure prints `IoError`, and an uncaught
`Limit` prints `RuntimeError`, because a limit is a runtime failure.

### Handling Failures by Hand

Matching a kind is not the only way to handle a failure. Sometimes the failing
operation is the thing that answers — `might fail` turns a failure into a value
the program can test — and a handler that matched `Error` would swallow the
distinction the program was asking about:

```redblue
try
    set record to files.read("records/cleo.rb")
catch error problem
    say "could not read the record: {problem}"
    give back
end
```

```redblue
set result to might fail network.get("https://example.com")
if result is nothing
    say "the network call failed"
end
```

### Finally

```redblue
try
    set file to might fail files.open("data.rb")
    process file
finally
    if file is not nothing
        might fail files.close(file)
    end
end
```

The `finally` needs no `catch` beside it, and it is owed however its region is
left — including by a failure, which is the case the cleanup above exists for.
The `finally` runs on the way out and the failure is reported after it, not
instead of it, unless the cleanup itself ends the region — see below:

```redblue
try
    set file to might fail files.open("data.rb")
finally
    say "closing what was opened"
end
```

```
closing what was opened
RuntimeError: ...
```

A `try` with no `catch` is not a handler and never was one: the failure belongs
to whatever is written around it, so a `catch` there takes it, and a program with
nothing around it stops with it as its own.

Two things in a `finally` are not that failure being reported. A `finally` that
*fails* replaces it, so the failure that reaches the `try` around this one is the
cleanup's own. And a `finally` that leaves its region abruptly — a `break` or a
`skip` — drops it: the region was left by the jump rather than by the failure, so
there is nothing for a `catch` written around this `try` to handle, and the
statements after the `finally` do not run.

```redblue
set log to ""
for each name in ["ada", "bob", "cleo"]
    try
        try
            set size to 1 + name
        finally
            if name is "bob" then skip end
            set log to log + "read " + name + "\n"
        end
    catch error
        set log to log + "missing " + name + "\n"
    end
end
say log
```

```
read ada
missing ada
read cleo
missing cleo
```

`bob` contributes neither line. The cleanup was told to abandon that turn, and
the turn was abandoned: the `catch` around the `try` did not run for it. Before
this rule, the tree-walking VM ran that `catch` anyway and the bytecode VM did
not — the same program, two answers, both exiting 0.

### `might fail`

`might fail <call>` discards a failure of that call and produces `nothing`, so
the program carries on where it would otherwise have stopped:

```redblue
set config to might fail files.read("config.rb")
if config is nothing
    say "using defaults"
end
```

It is two words. Neither half is the guard alone — `might read("x")` and
`fail read("x")` are errors — and the guarded operand is a call. A number or a
list literal cannot fail, so accepting one would read as though discarding a
failure were something it could do; `might fail 1 + 1` is an error naming the
operand that is not a call.

It is an expression, so it works anywhere a value is taken: as an argument, a
list element, a record value, or a parenthesized operand.

```redblue
set report to {body: might fail files.read("config.rb")}
might fail files.write("output.txt", report body)
```

#### What a guard does not discard

Three things are not a failure of the guarded call, and a `might fail` passes them
on rather than turning them into `nothing`:

- **A loop control.** A `break` or a `skip` inside the guarded call is not a
  failure at all, so a guarded call that leaves the loop still leaves the loop.
- **A failed `expect`.** An `expect` that fails is a test result, and the test
  harness reads it as one. A guard that discarded it would report a red test
  green.
- **A resource limit.** The step budget, the call-depth limit, and a loop's
  iteration cap are the host refusing to keep going, not the call going wrong. A
  program stopped at the call-depth limit has not carried on — it has been
  stopped — so reporting `nothing` for it would turn a runaway recursion into a
  successful value and leave the program running against a bound it has already
  reached:

```redblue
to endless(n)
    give back endless(n + 1)
end

set stopped to might fail endless(1)
```

```
RuntimeError: Maximum call depth of 1000 reached while calling 'endless'
```

The limit stops it either way; `might fail` is not an escape hatch from it. A
failure of the call itself is still discarded:

```redblue
set stopped to might fail files.read("config.rb")   // -> nothing
```

### Raising Errors

There is no `raise`, and no `give back error "..."`. Redblue has no statement
that fails on the writer's account: `give back` hands a value back to the caller,
and a value is not a failure. What `error` would be here is a name like any other
— `give back error "Cannot divide by zero"` reads `error` as a variable, and a
function that has never been given one fails the analyzer naming it.

A function signals a failure by performing an operation that fails, which is the
only thing that can fail:

```redblue
to divide(a, b)
    if b is equal to 0 then
        give back 1 / 0          // the RuntimeError this function owes its caller
    end
    give back a / b
end

set result to might fail divide(1, 0)   // -> nothing
```

---

## Asynchronous Programming

Not implemented. The words `async`, `wait`, `parallel` and `done` are ordinary
identifiers: they were reserved by the lexer with no rule in the parser to read
them, so a program written in the form below died with a parser error rather than
running. They are names again, and the form is not part of the language until an
interpreter arm exists for it.

```redblue
// Not Redblue. Nothing below parses.
async to fetch_data(url)
    set response to wait network.get(url)
    give back json.parse(response)
end
```

---

## Standard Library

### console

```redblue
say "Hello"                      // print with a newline
print "Hello"                    // print without one
console.log("Hello")             // `say` under its module spelling
console.error("Could not read")  // diagnostics go to stderr
console.clear()                  // clear the screen
set name to input("Your name? ")  // read one line from the terminal
```

`say` and `print` are keywords rather than module functions, so there is no
`console.say` and no `console.print`: a module has the functions that are spelled
`<module>_<member>` in the registry, and those two are not. `console.log` is the
module spelling of `say` and answers `nothing`, so `say console.log("x")` prints
`x` and then `nothing`. `console.error` writes to stderr rather than stdout, so a
program that collects its output can keep its diagnostics separate from it.
`console.clear` writes the terminal's clear sequence to stdout and answers
`nothing`; on a stream that is not a terminal the sequence is still written.

`input` is the whole of the language's input: it writes its first argument as a
prompt, reads one line from the terminal, and answers that line as text with the
line terminator removed — `"abc"` from `abc\n` and from `abc` alike, since a pipe
whose last line has no newline is still a line. At the end of the input it
answers the empty text rather than refusing, so a program that runs out of input
sees an empty answer instead of an error. The prompt is optional:
`input()` reads a line and asks nothing. It is a registered builtin rather than a
module function, so there is no `console.input` — the same rule as for `say` and
`print` — and it takes parentheses like any other call.

There is no `ask`. It was a reserved word with no rule in the parser to read it,
which is how this section came to name it, and it is an ordinary identifier
again — `set ask to 1` is a program. Both spellings are refused: `ask("n")` is
`Unknown function 'ask'`, and `ask "n"` — which is not a call at all, because a
function is only called with parentheses — is `Unknown variable 'ask'`.

### text

```redblue
set upper to text.uppercase("hello")  // "HELLO"
set lower to text.lowercase("HELLO")  // "hello"
set trimmed to text.trim("  hi  ")    // "hi"
set parts to text.split("a,b,c", by ",")  // ["a", "b", "c"]
set joined to text.join(["a", "b", "c"], by ",")  // "a,b,c"
set holds to text.contains("abc", "b")  // yes
set starts to text.starts_with("abc", "ab")  // yes
set ends to text.ends_with("abc", "bc")  // yes
set swapped to text.replace("a-b", "-", "+")  // "a+b"
set size to text.length("héllo")  // 5
```

`length` counts **characters**, where a character is one Unicode scalar value:
`length("héllo")` is 5 and `length("世界")` is 2. It is not a glyph count —
`é`, an `e` and a combining acute accent, is one glyph, two characters and a
length of 2. The **byte** count is still available and is spelled as a list by
`length(bytes.from_text("héllo"))`, which is 6, so a program that wants the
byte values rather than the count asks § bytes below for them.
`tests/test_text.rb` pins both numbers in six tests and
`corpus/unicode-0001..0008` record the character counts, including the case the
rule exists for.

`length` answers a text and a list, and it is the same function under each of the
three spellings: `length(xs)`, `text.length(xs)` and `list.length(xs)`. A record is
not a length — `length({a: 1})` is `RuntimeError: length requires a list or text`,
in all three spellings, and `tests/test_text.rb` pins the refusal as a caught
error. Both dotted spellings are module members, so both are written
above and in README.md — a name that answers without a document saying so is
surface nobody agreed to.

### math

```redblue
set pi to PI
set e to E
set absolute to math.abs(-3)   // 3
set floored to math.floor(1.7)  // 1
set ceiling to math.ceil(1.2)  // 2
set sqrt2 to math.sqrt(2)
set rounded to math.round(3.7)  // 4
set powered to math.pow(2, 10)  // 1024
set s to math.sin(0)  // 0
set c to math.cos(0)  // 1
set t to math.tan(0)  // 0
set lg to math.log(1)  // 0
set raised to math.exp(0)  // 1
```

`PI` and `E` are globals, not module members: a module has functions, so
`math.PI` is not a name in the language.

`math.random` is not implemented. The random builtins are spelled flat: `random`,
`random_seed`, `random_number`, `random_choice` and `random_shuffle`.

```redblue
random_seed(42)                // every draw after this is reproducible
set n to random(1, 6)          // a whole number from 1 to 6, both ends included
set f to random_number(0, 1)   // a number from 0 up to (not including) 1
set one to random_choice(xs)    // one member of a list
set mixed to random_shuffle(xs) // the same members, in a seeded order
```

`random_seed` makes the draws after it reproducible: the same seed gives the
same sequence of draws, in one process or in two. A program that never seeds
draws from a fixed default seed rather than from the clock, so Redblue's output
is reproducible by default and a program has to *ask* for variation it cannot
reproduce. A seed outside the range of a 64-bit machine integer has no bits left
to give and takes the nearest bound, so `random_seed(1e300)` is the seed
`random_seed(9223372036854775807)` names; the two ends are different seeds from
each other. `random` answers a whole number and `random_number` a fraction; both
refuse a range whose low end is above its high end, because such a range has no
member. A range may be written with two arguments, with one — which is the high
end, with zero as the low end, so `random(6)` is a die numbered zero through six
and `random_number(10)` is `[0, 10)` — or with none, which is `[0, 100]` for
`random` and `[0, 1)` for `random_number`. `random` answers one of the whole
numbers in its range, so a fractional bound narrows the range rather than making
the draw a fraction: `random(0, 100.5)` answers `0` to `100`, and a range holding
no whole number at all, such as `random(0.2, 0.8)`, is refused. `random_choice`
refuses an empty list, since it has no member to choose.

### files

```redblue
set content to files.read("data.rb")
might fail files.write("output.rb", content)
might fail files.append("log.rb", "new line")
set lines to files.lines("data.rb")  // one entry per line
files.copy("data.rb", "copy.rb")
files.rename("copy.rb", "moved.rb")
files.delete("moved.rb")
if files.exists("config.rb")
    // ...
end
```

`files.copy` and `files.rename` refuse rather than write half a file: a copy
whose source is not there, or a rename onto a path that cannot be written, is an
`IoError` naming the path. `files.rename` moves as well as renames, so a copy
written to a temporary path and then renamed is one file at the end rather than
two.

### list

```redblue
set doubled to list.map([1, 2, 3], to (x) give back x * 2)
set size to list.length([1, 2, 3])  // 3
```

`filter` and `reduce` are registered as flat builtins and neither is
implemented, so neither is a member of `list`: `list.filter(xs, f)` is
`Module 'list' has no function 'filter'`, the same refusal as `math.random`
above. A builtin that cannot answer is not a module member — the alternative is a
spelling the analyzer accepts and the runtime can only refuse.

`list.map` is the only higher-order builtin, and it works in three spellings:
`map(xs, f)`, `list.map(xs, f)` and `[1, 2, 3].map(f)`. `list.length` is `length`
under its module spelling, as § text writes it for a text.

The third spelling takes its receiver **literally**. A dotted call resolves its
receiver against modules and objects, and a list held in a variable is a value
rather than either of those, so `xs.map(f)` is refused as
`Module 'xs' has no function 'map'` — the same refusal `list.filter` gives above.
A list written out in full is the only one a `.` can be put in front of.

### network

```redblue
set response to network.get("https://api.example.com")
set response to network.post("https://api.example.com", data)
```

Both calls block until the response arrives, so there is nothing to write in
front of them. `wait` is an ordinary identifier, and the `wait`-prefixed spelling
this section used to carry did not parse. See "Asynchronous Programming" above.

### json and csv

```redblue
set obj to json.parse("{ \"name\": \"Alice\" }")  // {name: Alice}
set text to json.stringify(obj)                   // {"name": "Alice"}
set rows to csv.parse("name,age\nAlice,30")       // [[name, age], [Alice, 30]]
```

A text is written between double quotes, and a quote *inside* one is escaped
`\"` — there are no single-quoted strings, so `'{"name": "Alice"}'` is a
`LexerError` at the first quote rather than a JSON document.

There is no `formats` module. JSON and CSV are their own modules, spelled
`json.*` and `csv.*`.

### bytes

```redblue
set data to bytes.from_text("Hi")        // [72, 105] — a byte list
set back to bytes.text(data)             // "Hi"
bytes.write("out.bin", data)             // write a byte list to a file
```

A byte is the whole number 0 to 255. `files.write` writes the UTF-8 encoding of
a text, so `bytes` is how a program writes a byte it cannot spell — which a
compiler writing a bytecode file cannot avoid. A value that is not a byte, or a
byte list that is not valid UTF-8, is a refusal naming the byte rather than a
wrapped value or a replacement character three files later.

### sys

```redblue
for each argument in sys.argv()
    say argument
end
```

`sys.argv()` is the list of arguments after the program path, and it is empty
for a program run from the REPL or from a test, because only `rb run` collects
them.

### time

```redblue
set stamp to time.now()                  // a record: seconds, nanoseconds
time.sleep(0.5)                          // pause for half a second
say time.format(1704067200, "%Y-%m-%d")  // "2024-01-01"
say time.format(time.date(2024, 2, 9))   // 2024-02-09 00:00:00
say time.parse("2024-01-01T12:00:00+02:00") // 2024-01-01 10:00:00
say time.unix("2024-01-01 00:00:00")     // 1704067200
say time.from_unix(1704067200)           // 2024-01-01 00:00:00
```

`time.now()` is a record of `seconds` and `nanoseconds` since
1970-01-01T00:00:00Z, and `time.sleep` answers `nothing` once the pause is over.
`time.format` takes a date value or a number of seconds since the same instant
and a format string; with no format string it writes `%Y-%m-%d %H:%M:%S`.
`time.parse` reads an ISO-8601 timestamp as a date, applying any zone offset so
the value is the UTC reading of the instant. `time.unix` is the other direction:
it takes a timestamp text or a date value and answers the seconds since the same
instant, read as UTC wherever it runs; `time.from_unix(seconds)` is the inverse
and turns such seconds into a date value.

`time.sleep` is the one call in this section that blocks — the other three are
computed and returned immediately, so `time.format` and `time.unix` cost no wall
time whatever the clock says. A program that wants a timer rather than a pause
has to read the clock itself — `time.now()` twice and `time.sleep` between them.
The calls that block besides `time.sleep` are `network.get` and `network.post`,
which wait for a response.

#### Dates and durations

A **date** is a first-class value: it can be stored in a record or a list,
passed to and returned from a function, compared, and printed. A **duration** is
a signed span of time, and it is a first-class value too.

```redblue
set start to time.datetime(2024, 2, 29, 23, 30, 0)
set next  to time.add_months(start, 1)      // 2024-03-29 23:30:00
set span  to next - start                   // a duration of 29 days
say to_text(span)                           // 29 days

set later to start + time.duration(86400)   // exactly one day later
say later                                   // 2024-03-01 23:30:00

set jan to time.date(2024, 1, 31)
say to_text(time.add_months(jan, 1))        // 2024-02-29 00:00:00
say to_text(time.add_years(jan, 1))         // 2025-01-31 00:00:00
say to_text(time.add_days(jan, 1))          // 2024-02-01 00:00:00
say to_text(time.add_hours(jan, 24))        // 2024-02-01 00:00:00
say to_text(time.add_minutes(jan, 60))      // 2024-01-31 01:00:00
say to_text(time.add_seconds(jan, 1))       // 2024-01-31 00:00:01
```

`time.date(year, month, day)` makes a date at midnight, and
`time.datetime(year, month, day, hour, minute, second)` makes one at the stated
time. Both read whole numbers and refuse a component that is not a real part of
the calendar (`2024-02-30`, month `13`, hour `24`) with a catchable runtime
error naming the builtin. `time.duration(seconds)` makes a duration from a
number of seconds; the number may be negative or fractional (to millisecond
precision).

The operators are defined on these values:

| Expression | Answer |
|---|---|
| `date < date`, `date <= date`, `date > date`, `date >= date` | `yes` or `no` |
| `date is date` | `yes` when the two readings are identical |
| `date - date` | a duration, negative when the left date is earlier |
| `date + duration`, `duration + date` | the date moved by that span |
| `date - duration` | the date moved back by that span |
| `duration + duration`, `duration - duration` | a duration |
| `duration < duration`, and the other orderings | `yes` or `no` |

A date or a duration compared with a number is a **runtime error**, not a
coercion, whether the comparison is an ordering or `is`/`is not`:
`time.date(2024, 1, 1) < 5` and `time.date(2024, 1, 1) is 5` both name the two
kinds they mixed, and `try … catch error of Runtime` can catch either. Adding a
date to a date, or subtracting a date from a duration, is refused the same way.

The components of a date are read as properties: `year`, `month`, `day`, `hour`,
`minute`, `second`, `nanosecond`, and `weekday` (Monday is `1`, Sunday is `7`). The components
of a duration are `days`, `hours`, `minutes`, `seconds` (the remainder inside
the last day, hour and minute), `total_seconds`, and `nanoseconds`. A negative
span carries its sign in each component, so `time.duration(-1.5).seconds` is
`-1` and its `nanoseconds` is `-500000000`; the components sum to
`total_seconds`. An unknown property answers `nothing`, as a missing record
field does.

Calendar arithmetic is `time.add_years`, `time.add_months`, `time.add_days`,
`time.add_hours`, `time.add_minutes` and `time.add_seconds`; each takes a date
and a whole number, and a negative number moves the date backwards. Years and
months are calendar units, not fixed spans:

- **Month-end overflow clamps to the last day of the target month.**
  `time.add_months(time.date(2024, 1, 31), 1)` is `2024-02-29`; the same call in
  2023 is `2023-02-28`; and `time.add_months(time.date(2024, 3, 31), -1)` is
  `2024-02-29`.
- **29 February plus one year is 28 February.** `time.add_years` clamps the same
  way, so `time.add_years(time.date(2024, 2, 29), 1)` is `2025-02-28`.
- Days, hours, minutes and seconds are fixed spans: one day is always 24 hours
  and one hour is always 60 minutes. A move that leaves the range of dates the
  implementation can represent is a runtime error naming the builtin.

**Timezone stance.** A date is *timezone-naive*: it has no zone, no offset and
no daylight-saving rule. The components are the same reading everywhere, so
`2024-03-10` is not a 23-hour day and `2024-11-03` is not a 25-hour day — adding
24 hours always lands on the same clock time on the next calendar day. Programs
that need a zone, an offset, or a conversion to local time must carry that
information themselves; this phase deliberately defines the zone-free value
first. A zone-free text is read as UTC by `time.parse` and `time.unix`; an
explicit offset in `time.parse` is applied to the reading.

`time.parse` accepts these ISO-8601 shapes, and nothing else:

| Input | Example | Answer |
|---|---|---|
| date only | `2024-02-29` | midnight on that day |
| date and time | `2024-02-29T23:59:58` | that clock reading, read as UTC |
| space instead of `T` | `2024-02-29 23:59:58` | the same instant |
| fraction | `2024-02-29T23:59:58.123456789` | the fraction is kept to the nanosecond |
| comma fraction | `2024-02-29T23:59:58,123456789` | the same instant; ISO-8601 allows either mark |
| `Z` zone | `2024-02-29T23:59:58Z` | the same instant |
| offset zone | `2024-01-01T12:00:00+02:00` | `2024-01-01 10:00:00` in UTC |

The year is one or more digits. Four is ISO-8601's ordinary form; one to three
are accepted because the old `time.unix` layout accepted them, and more than
four are accepted because `%Y` writes the expanded year. An optional leading `-`
or `+` is accepted, so the expanded-year forms `%Y` writes (`-10000` and
`+10000`) read back. A timestamp with no zone is read as UTC, the same stance a
date value has.

Not accepted, each a catchable runtime error naming the builtin and the byte
position where the text stopped matching: a leap second (`:60`), a fraction
with no digits or more than nine after either decimal mark, a time with no
seconds (`HH:MM`), an offset without minutes, trailing characters, and a day
that does not exist (`30` February, `29` February in a common year).
`time.format` refuses a layout `chrono` cannot read the same way. A date can
also be printed with `say` or `to_text`, which writes `YYYY-MM-DD HH:MM:SS` and
drops the fraction.

### regex

```redblue
say regex.match("id=42", "id=([0-9]+)")      // {text: id=42, ...}
say regex.is_match("id=42", "^id")           // yes
say regex.capture("id=42", "id=([0-9]+)", 1) // 42
say regex.replace("id=42", "id=([0-9]+)", "user-$1") // user-42
say regex.split("a1b22c", "[0-9]+")          // [a, b, c]
```

The module binds the `regex` crate named in
[LIBRARIES.md](LIBRARIES.md) § 2; the engine is not written in Redblue.
`regex.match` searches anywhere in the text and answers a match record, or
`nothing` when there is no match. The record is ordinary data: `m.text` is the
whole match, `m.start` and `m.finish` are character positions like `length`
counts, and `m.groups` is a list whose `0` entry is the whole match and whose
`n` entry is capture group `n`. A group that did not participate is `nothing`.
Two match records with the same captures compare equal; there is no identity
behind them.

`regex.replace` substitutes every non-overlapping match and understands `$1`,
`$name` and `${name}` capture references. A reference to a group the pattern
does not have is a catchable `Runtime` failure naming the group, rather than the
empty text a replacement engine would otherwise put there. `regex.split` splits
on a pattern and leaves a text whole when the pattern does not match. An empty
pattern is a valid pattern and matches at position zero.

A pattern that does not compile is a catchable `Runtime` failure carrying the
crate's message; it is never a panic. The `regex` crate is linear-time, so a
pattern that backtracks catastrophically in another engine cannot hang a
Redblue program. `text.split` is unchanged: its separator is still literal, and
`regex.split` is the pattern form.

---

## Grammar

See [GRAMMAR.md](GRAMMAR.md) for the complete EBNF grammar specification.

---

## Appendix: Keywords Reference

| Keyword | Description |
|---------|-------------|
| set | Declare/assign variable |
| to | Assignment or function declaration |
| is | Comparison |
| nothing | Null value |
| yes/no | Boolean literals |
| if/then/else/end | Conditional statements |
| unless | Negative condition |
| for/each/in/from/times | Loop constructs |
| while/repeat/until | Loop constructs |
| break/skip | Loop control |
| return/give back | Function return |
| might fail | Error-prone operation |
| constant | Declare a name that may not be rebound |
| module/import/export/as | Module system |
| and/or/not | Logical operators |
| mod | Remainder |
| try/catch/finally | Error handling |
| object/has/can | Object declarations, properties, methods |
| this/extends | Reference to the receiver; inheritance |
| say/print | Output |
| test/expect | Test blocks |

A word that is not on this list is an ordinary identifier, including `are`,
`ask`, `async`, `done`, `new`, `parallel`, `that`, `wait` and `when`. They were
reserved by the lexer with no rule anywhere in the parser to read them, which
made them unusable as names as well as unimplemented; they are identifiers again
and the four forms that used to be promised for them are not part of the
language. `until` is reserved and belongs to the `while`/`repeat` forms above.
`takes`, `needs`, `called`, `either`, `neither`, `every`, `error`, `number`,
`text`, `list` and `record` have never been reserved and are ordinary
identifiers too.

---

*This specification is a draft. Implementation details may change before v1.0.*
