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

Redblue has 32 keywords:

| Category | Keywords |
|----------|----------|
| Variables | set, constant, to, is, are, nothing |
| Control | if, then, else, end, when, unless |
| Loops | for, each, in, from, times, while, repeat, until |
| Jumps | break, skip, return, give back |
| Functions | to, takes, needs, might fail |
| Types | number, text, yes/no, list, record, object |
| Modules | module, import, export, as |
| Logic | and, or, not |
| Errors | try, catch, finally, error |
| Objects | has, can, this, that, new, extends |

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

```redblue
set person to new Person
set name of person to "Alice"
set age of person to 30
```

### Constructor

```redblue
object Person
    has name
    has age
    
    to create(name, age)
        set this.name to name
        set this.age to age
        give back this
    end
end

set person to new Person("Alice", 30)
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
    set data to might fail files.read("config.rb")
    process data
catch error
    say "Error: {error message}"
end
```

### Multiple Catch Blocks

```redblue
try
    set result to might fail risky_operation()
catch error of FileError
    handle_file_error(error)
catch error of NetworkError
    handle_network_error(error)
catch error
    handle_generic_error(error)
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

```redblue
to might fail divide(a, b)
    if b is equal to 0
        give back error "Cannot divide by zero"
    end
    give back a / b
end
```

---

## Asynchronous Programming

### Async Functions

```redblue
async to fetch_data(url)
    set response to wait network.get(url)
    give back formats.parse_json(response)
end
```

### Await

```redblue
async to main()
    set users to wait fetch_users()
    for each user in users
        say user.name
    end
end
```

### Parallel Execution

```redblue
async to load_all()
    parallel
        set users to wait fetch_users()
        set posts to wait fetch_posts()
        set comments to wait fetch_comments()
    until done
    
    give back combine(users, posts, comments)
end
```

---

## Standard Library

### console

```redblue
say "Hello"           // Print with newline
print "Hello"         // Print without newline
ask "Your name?"      // Get user input
```

### text

```redblue
set upper to text.uppercase("hello")  // "HELLO"
set lower to text.lowercase("HELLO")  // "hello"
set parts to text.split("a,b,c", by ",")  // ["a", "b", "c"]
set joined to text.join(["a", "b", "c"], by ",")  // "a,b,c"
```

### math

```redblue
set pi to PI
set e to E
set sqrt2 to math.sqrt(2)
set rounded to math.round(3.7)  // 4
set powered to math.pow(2, 10)  // 1024
set floored to math.floor(1.7)  // 1
```

`PI` and `E` are globals, not module members: a module has functions, so
`math.PI` is not a name in the language.

`math.random` is not implemented. The random builtins are `random_number`,
`random_choice` and `random_shuffle`, spelled flat.

### files

```redblue
set content to files.read("data.rb")
might fail files.write("output.rb", content)
might fail files.append("log.rb", "new line")
if files.exists("config.rb")
    // ...
end
```

### list

```redblue
set doubled to list.map([1, 2, 3], to (x) give back x * 2)
```

`list.filter` and `list.reduce` are registered but not implemented. `list.map`
is the only higher-order builtin, and it works in all three spellings:
`map(xs, f)`, `list.map(xs, f)` and `xs.map(f)`.

### network

```redblue
set response to wait network.get("https://api.example.com")
set response to wait network.post("https://api.example.com", data)
```

### json and csv

```redblue
set obj to json.parse('{"name": "Alice"}')
set text to json.stringify(obj)
set rows to csv.parse("name,age\nAlice,30")
```

There is no `formats` module. JSON and CSV are their own modules, spelled
`json.*` and `csv.*`.

---

## Grammar

See [GRAMMAR.md](GRAMMAR.md) for the complete EBNF grammar specification.

---

## Appendix: Keywords Reference

| Keyword | Description |
|---------|-------------|
| set | Declare/assign variable |
| to | Assignment or type annotation |
| is/are | Comparison or type checking |
| nothing | Null value |
| yes/no | Boolean literals |
| if/then/else/end | Conditional statements |
| when | Pattern matching |
| unless | Negative condition |
| for/each/in/from/times | Loop constructs |
| while/repeat/until | Loop constructs |
| break/skip | Loop control |
| return/give back | Function return |
| to | Function declaration |
| takes/needs | Parameter specification |
| might fail | Error-prone operation |
| number/text/yes/no | Primitive types |
| list/record/object | Complex types |
| module/import/export | Module system |
| and/or/not | Logical operators |
| try/catch/finally | Error handling |
| error | Error type |
| has | Object property |
| can | Object method capability |
| this/that | Object reference |
| new | Object instantiation |
| extends | Inheritance |

---

*This specification is a draft. Implementation details may change before v1.0.*
