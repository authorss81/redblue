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
        give back math.PI * this.radius * this.radius
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
console.log("Hello")  // The same as say
console.error("no")   // Print an error with newline
console.clear()       // Clear the screen
```

Reading from the terminal is not documented as a call: `ask` is a reserved word
and no statement parses it, so there is no spelling of it to write down.

### text

```redblue
set upper to text.uppercase("hello")  // "HELLO"
set lower to text.lowercase("HELLO")  // "hello"
set trimmed to text.trim("  spaced  ")  // "spaced"
set parts to text.split("a,b,c", ",")  // ["a", "b", "c"]
set joined to text.join(["a", "b", "c"], ",")  // "a,b,c"
set size to text.length("hello")  // 5
```

The separator is an ordinary second argument. `by` is the step of a range loop
and nothing else, so `text.split("a,b,c", by ",")` does not parse.

### math

```redblue
set sqrt2 to math.sqrt(2)
set magnitude to math.abs(-7)  // 7
set down to math.floor(1.7)  // 1
set up to math.ceil(1.2)  // 2
set rounded to math.round(3.7)  // 4
set rand to math.random(1, 100)
```

`math.PI` is not a member: a constant is a name a program declares with
`constant`, as §Constants describes, so it is written `constant PI to 3.14159`
and read as `PI`.

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
set size to list.length([1, 2, 3])  // 3
```

`map`, `filter` and `reduce` take a function as their second argument, and the
inline function literal they are written with here is not part of the grammar
yet, so they are not documented as calls until it is.

### network

```redblue
set response to network.get("https://api.example.com")
set response to network.post("https://api.example.com", data)
```

### formats

```redblue
set obj to formats.parse_json("{\"name\": \"Alice\"}")
set json to formats.to_json(obj)
set csv to formats.parse_csv("name,age\nAlice,30")
```

Both formats are also modules of their own — `json.parse`, `json.stringify` and
`csv.parse` are the same three functions under their format's name:

```redblue
set obj to json.parse("{\"name\": \"Alice\"}")
set json to json.stringify(obj)
set csv to csv.parse("name,age\nAlice,30")
```

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
