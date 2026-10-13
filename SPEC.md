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

// Length
set count to length of items

// Contains
if "hello" is in words
```

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

The name is bound to the text `error` — the word that says a failure was handled.
Redblue errors are messages, not tagged values: there is no error object with a
type to dispatch on, so `say error` is the whole of what a handler can say about
what it caught.

### Handling Failures by Hand

A `try` has **one** `catch`. There is no `catch error of <Type>`, no second
`catch`, and no type a failure could be matched against — `of` is not a word in
the language, so `catch error of FileError` reads as a binder named `of` and
leaves `FileError` as a statement of its own. A handler written that way fails
the analyzer, naming the orphan.

Asking what kind of failure it was is work for the program, not for the handler:
the failing operation is inside the `try`, and its own result is what says which
kind it was.

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

A module function is written `<module>.<member>`, and that is the only spelling
a program may use. Each one is stored in the registry under the name
`<module>_<member>` — `files.read` as `files_read` — and that encoding is an
internal name rather than a second spelling: a program that writes it without a
receiver is refused with

```
RuntimeError: files_read is not a name on its own: it is how files.read is stored
```

The refusal names the spelling that works, and it is the same on both engines.
The encoding used to answer, which meant the REPL was withholding names a program
could write and `flat_builtin_names` was not the set of names a program could
write; it does not answer now.

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
set size to text.length("héllo")  // 6
```

`length` counts **bytes**, which is what a text is made of: `length("héllo")` is
6 and `length("世界")` is 6, not 2. The same number is spelled as a list by
`length(bytes.from_text("héllo"))`, so a program that wants the byte values
rather than the count asks § bytes below for them. `tests/test_text.rb` pins this
in six tests and `corpus/unicode-0001..0008` record it, including the case the
rule exists for: `é` — an `e` and a combining acute accent — is one glyph, three
bytes and a length of 3.

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
say time.unix("2024-01-01 00:00:00")     // 1704067200
```

`time.now()` is a record of `seconds` and `nanoseconds` since
1970-01-01T00:00:00Z, and `time.sleep` answers `nothing` once the pause is over.
`time.format` takes a number of seconds since the same instant and a format
string; with no format string it writes `%Y-%m-%d %H:%M:%S`. `time.unix` is the
other direction: it takes `YYYY-MM-DD HH:MM:SS` and answers the seconds since
the same instant, read as UTC whichever time zone the machine is in, so the
answer is the same on the machine that wrote the text and on one that reads it.

`time.sleep` is the one call in this section that blocks — the other three are
computed and returned immediately, so `time.format` and `time.unix` cost no wall
time whatever the clock says. A program that wants a timer rather than a pause
has to read the clock itself — `time.now()` twice and `time.sleep` between them.
The calls that block besides `time.sleep` are `network.get` and `network.post`,
which wait for a response.

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
