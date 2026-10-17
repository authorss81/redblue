# Redblue Grammar Specification

## Version 1.0 - EBNF Notation

### Notation

```
// Comments start with //
KEYWORD   - uppercase keywords
'text'    - literal text
[ x ]     - optional (0 or 1)
{ x }     - repetition (0 or more)
( x | y ) - choice (x or y)
x y       - sequence (x followed by y)
```

---

## 1. Lexical Structure

### 1.1 Whitespace and Comments

```
whitespace    = { ' ' | '\t' | '\n' | '\r' }
comment       = '//' { any_character_except_newline } newline
newline       = '\n' | '\r' | '\r\n'
```

### 1.2 Identifiers

```
identifier    = letter { letter | digit | '_' }
letter        = 'a'..'z' | 'A'..'Z' | unicode_letter
digit         = '0'..'9'
```

**Note:** Identifiers can contain Unicode letters (e.g., `nom`, `名前`, `имя`)

### 1.3 Keywords (Reserved)

```
keywords = 'set' | 'constant' | 'to' | 'is' | 'nothing' | 'yes' | 'no'
         | 'if' | 'then' | 'else' | 'end' | 'unless'
         | 'for' | 'each' | 'in' | 'from' | 'times' | 'while' | 'repeat'
         | 'until'
         | 'break' | 'skip' | 'return'
         | 'give' 'back'             // `give back <expr>` is one form
         | 'might' 'fail'            // `might fail <call>` is one form
         | 'say' | 'print'
         | 'module' | 'import' | 'export' | 'as'
         | 'and' | 'or' | 'not' | 'mod'
         | 'try' | 'catch' | 'finally'
         | 'test' | 'expect'
         | 'object' | 'has' | 'can' | 'this' | 'extends'
```

A word is listed here only while the parser has a rule that reads it. A reserved
word the parser cannot read is not a feature: it cannot be an identifier either,
and the rejection names an internal token kind rather than the word the writer
typed. Nine words — `are`, `ask`, `async`, `done`, `new`, `parallel`, `that`,
`wait`, `when` — were listed and read by nothing; they are identifiers now, so
they are not listed here. The productions that once read them are kept as the
shape a future implementation has to match, each under a **Not implemented**
note: §4.3's `when_statement`, §4.8's `async_function` / `async_call` /
`parallel_block`, and §7's `object_literal`. `object_literal` is therefore not an
alternative of `primary` in §5.1. The table is checked against
`Lexer::keywords()` by `tests/keyword_reservation_test.rs`, so a word added here
without a parser arm fails the suite rather than shipping.

### 1.4 Literals

```
// Numbers
number_literal     = ['-'] digit { digit } [ '.' digit { digit } ]
                   | ['-'] digit { digit } [ '.' ] [ exponent ]
exponent           = ('e' | 'E') ['+' | '-'] digit { digit }

// Text (Strings)
text_literal       = '"' { any_unicode_character_except_quote | escape } '"'
escape             = '\' ( 'n' | 't' | 'r' | '"' | '\' | 'u' hex_digit hex_digit hex_digit hex_digit )
hex_digit          = digit | 'a'..'f' | 'A'..'F'

// The escape set is closed. A `\` followed by anything else is a LexerError at
// the backslash, and so is a `\` followed by `u` and fewer than four hex digits
// — a short run is not padded out to four, and a fifth digit is ordinary text
// after the escape, so `\u1f600` is `\u1f60` then `0`. A `\` followed by a
// surrogate half is a LexerError too: `\uD83D` names no character of its own.

// Boolean
boolean_literal    = 'yes' | 'no'

// Nothing
nothing_literal   = 'nothing'

// List literal
list_literal       = '[' [ expression { ',' expression } ] ']'

// Record literal
record_literal     = 'record' '{' [ property { ',' property } ] '}'
property           = identifier ':' expression
```

### 1.5 Operators

```
// Arithmetic
add_op       = '+'
sub_op       = '-'
mul_op       = '*'
div_op       = '/'
mod_op       = 'mod'

// Comparison
eq_op        = 'is equal to' | '=='
neq_op       = 'is not' | 'isnt' | '!='
lt_op        = 'is less than' | '<'
lte_op       = 'is less than or equal to' | '<='
gt_op        = 'is greater than' | '>'
gte_op       = 'is greater than or equal to' | '>='

// Logical
and_op       = 'and' | '&&'
or_op        = 'or' | '||'
not_op       = 'not' | '!'

// Special operators
in_op        = 'in'
has_op       = 'has'
of_op        = 'of'
to_op        = 'to'
```

---

## 2. Type System

### 2.1 Type Expressions

Not implemented. Redblue has no static types, no type expressions and no type
annotations: every production in this section is kept here as the shape a future
implementation has to match, and nothing below parses today. `number`, `text`,
`list of`, `record of`, `function` and the type names a program would spell are
ordinary identifiers, not reserved words — `set number to 1` is a program, and
`set numbers to list of number` is a refusal naming the three names it could not
resolve.

The five productions that read a `type_expression` back — §4.1's annotated
`declaration`, §4.5's `function_decl` and `parameter`, §4.6's `property_decl` —
carry the same note. Each is pinned by `tests/type_annotation_test.rs`, which
runs every annotated form in the language and requires the parser to refuse it;
the moment one of them is implemented that test goes red and the note has to come
off in the same change.

```
// NOT IMPLEMENTED. Nothing below parses today: `type_expression` is written down
// as the shape a future implementation has to match, and §4.1, §4.5 and §4.6 all
// carry this same note on the productions that read a type expression back.
type_expression  = primitive_type
                 | list_type
                 | record_type
                 | object_type
                 | function_type
                 | generic_type
                 | union_type
                 | 'nothing'

primitive_type   = 'number' | 'text' | 'yes/no'
list_type        = 'list of' type_expression
record_type      = 'record of' '{' type_field { ',' type_field } '}'
type_field       = identifier ':' type_expression
object_type      = 'object' identifier [ 'extends' identifier ]
function_type    = 'function' [ type_expression ] 'to' [ type_expression ]
                 | 'function' '(' [ type_expression { ',' type_expression } ] ')' 'to' [ type_expression ]
generic_type     = identifier 'of' type_expression { 'and' type_expression }
union_type       = type_expression 'or' type_expression
```

---

## 3. Program Structure

### 3.1 Module

```
module           = 'module' identifier
                   { import_statement }
                   { statement }
                   [ export_statement ]
                   'end'

import_statement = 'import' identifier { 'as' identifier }
                 | 'import' identifier '.' identifier 'as' identifier

export_statement = 'export' identifier { ',' identifier }
                 | 'export' 'all'
```

### 3.2 Standalone Script

```
script           = { statement }
```

---

## 4. Statements

### 4.1 Declaration Statements

```
// Variable declaration — NOT IMPLEMENTED in the annotated form: the
// `':' type_expression` bracket is a §2.1 type expression and the parser does
// not read one, so `set x : number to 1` is a parser error and `set x to 1` is
// the whole of what parses today. The multiple-assignment form below, which
// carries no annotation, does parse.
declaration      = 'set' identifier [ ':' type_expression ] 'to' expression

// Multiple assignment
declaration      = 'set' identifier { ',' identifier } 'to' expression { ',' expression }

// Constant declaration — NOT IMPLEMENTED in the annotated form, for the reason
// the `set` declaration above gives: `constant C : number to 1` is a parser
// error, and `constant C to 1` is the whole of what parses today.
declaration      = 'constant' identifier [ ':' type_expression ] 'to' expression
```

A `constant` binds a name of the whole program, so it may be declared once
only — a second declaration of the same name is a runtime error
(`Constant 'NAME' is already declared`) — and no `set` may rebind it
(`Cannot assign to constant 'NAME'`), a module's own `set` bound by an `import`
included. A local of the same name shadows it for the length of its scope.
Reading it before its declaration is an unknown variable, as it is for any name;
a function body declared above the declaration reads it, because a body runs
when it is called.

### 4.2 Assignment Statements

```
assignment       = identifier 'to' expression
                 | identifier '.' identifier 'to' expression
                 | identifier '[' expression ']' 'to' expression
```

### 4.3 Control Flow Statements

```
// If statement
if_statement     = 'if' expression
                    'then'
                    { statement }
                    { 'else if' expression
                      'then'
                      { statement } }
                    [ 'else'
                      { statement } ]
                    'end'

// When statement (pattern matching) — NOT IMPLEMENTED, and `when` is not a
// reserved word: it is an ordinary identifier. Left here so the form a future
// `when` has to parse is written down rather than invented again.
when_statement    = 'when' expression
                    { 'case' pattern 'then' expression }
                    [ 'else' expression ]
                    'end'

// Unless statement
unless_statement = 'unless' expression
                    'then'
                    { statement }
                    'end'

// For loop (iteration)
for_statement    = 'for' 'each' identifier 'in' expression
                    { statement }
                    'end'

// For loop (range)
for_statement    = 'for' 'each' identifier 'from' expression 'to' expression [ 'by' expression ]
                    { statement }
                    'end'

// Repeat loop
repeat_statement = 'repeat' expression 'times'
                    { statement }
                    'end'

// While loop
while_statement  = 'while' expression
                    { statement }
                    'end'

// Repeat until
repeat_until     = 'repeat'
                    { statement }
                    'until' expression
```

### 4.4 Jump Statements

```
break_statement  = 'break'
skip_statement   = 'skip'
return_statement = 'return' [ expression ]
                 | 'give back' [ expression ]
```

Neither `break` nor `skip` takes an operand: both act on the innermost loop the
statement is written inside, and end the block they were written in as well as
the loop — a `catch`, `finally`, `test`, `object` or `module` body, or an `unless`
body, inside a loop is inside that loop. With no enclosing loop they are a
runtime error, `'<keyword>' is only valid inside a loop`, which
`try ... catch error` catches; a function body is not inside the loop that called
it, so a `break` in one is refused too. See SPEC.md, "Break and Skip".

### 4.5 Function Declaration

```
// NOT IMPLEMENTED in the annotated forms. `needs` and `gives` are not reserved
// words: they are ordinary identifiers, so `to f(a) needs number` parses as two
// expressions in a row and is refused by the analyzer with `Unknown variable
// 'needs'`. A bare `to f(a)` is the whole of what parses today. The parameter
// list below carries the same note on the parameters themselves.
function_decl    = 'to' identifier [ '(' [ parameter { ',' parameter } ] ')' ]
                   [ 'needs' type_expression { ',' type_expression } ]
                   [ 'gives' type_expression ]
                   { statement }
                   'end'

// NOT IMPLEMENTED in both forms. Neither `to f(x : number)` nor `to f(x of
// number)` parses: the first is a parser error at the colon, the second at the
// word `of`. `to f(x)` is the whole of what parses today.
parameter        = identifier [ ':' type_expression ]
                 | identifier 'of' type_expression
```

### 4.6 Object Declaration

```
object_decl      = 'object' identifier [ 'extends' identifier ]
                    { property_decl | method_decl }
                    'end'

// NOT IMPLEMENTED in the annotated form: `has x : number` and `has x default 1`
// are both parser errors, and `has x` is the whole of what parses today. A
// property is given its value by writing to the declaration — see SPEC.md,
// "Lookup order and shadowing".
property_decl    = 'has' identifier [ ':' type_expression ] [ 'default' expression ]

method_decl      = 'to' identifier [ '(' [ parameter { ',' parameter } ] ')' ]
                    { statement }
                    'end'

method_decl      = 'to' 'can' identifier [ '(' [ parameter { ',' parameter } ] ')' ]
                    { statement }
                    'end'
```

### 4.7 Try Statement (Error Handling)

```
try_statement    = 'try'
                    { statement }
                    { 'catch' [ identifier [ identifier ]
                                [ 'of' failure_kind
                                  [ identifier ] ] ]
                      { statement } }
                    [ 'finally'
                      { statement } ]
                    'end'

failure_kind     = 'Error' | 'Runtime' | 'Io' | 'Limit'
```

A `catch` names what it caught by naming it, and the name is an `identifier` like
any other — `error` is not a keyword. So `catch e` binds `e`, `catch error` binds
a name spelled `error`, and `catch error e` is the same handler with the name
written after the word, which is read when a second identifier follows `error` on
the same line. A handler may instead match the kind of failure it caught:
`catch error of <failure_kind>` binds the failure and runs its body only for that
kind, and a failure of any other kind is not handled there — it belongs to
whatever is written around the `try`. The list of names is closed; anything else
after `of` is a parse error, as is `of` with no name after it at all, and
`SPEC.md` § "Handling Failures by Kind" says what each name matches. `of` is
read as an ordinary identifier that happens to be spelled `of`, so it is not a
keyword and a program may still bind that name.

The word is matched exactly, the way every other word in the language is: `catch
ERROR err` is a `catch` that binds a name spelled `ERROR`, and `err` is left
behind as an orphan statement. Two things the rule depends on follow from the
name being an ordinary identifier — a `catch error <name>` may bind a name
spelled `error` again (`catch error error` binds `error`), and the binder belongs
to the `catch` body alone, so it is not readable before the `try`, after the
`end`, or in a `finally` beside it. `rb format` writes the canonical `catch
<name>` — or `catch error of <failure_kind> <name>` for a typed handler — and
drops the handler word; the formatted program reads back to the same tree and
handles the same failures.

The `catch` is optional, and a `try` without one is not a handler: the `finally`
is still owed however its region is left — a failure runs it before the failure is
reported on — and the failure is then the enclosing program's to catch, or its
own to stop with. See `SPEC.md`, "Finally".

Two things in that `finally` are not the failure being reported:

- a `finally` that **fails** replaces it — the failure that reaches the enclosing
  `try` is the cleanup's own;
- a `finally` that leaves its region **abruptly** — a `break` or a `skip` — drops
  it, because the region was left by the jump rather than by the failure, so a
  `catch` written around the `try` does not run. See `SPEC.md`, "Finally".

### 4.8 Async Statements

Not implemented. `async`, `wait`, `parallel` and `done` are ordinary
identifiers — the lexer reserved them with no rule in the parser to read them.
The productions are kept here as the shape a future implementation has to match;
nothing below parses today.

```
async_function   = 'async' function_decl
async_call       = 'async' set_statement

parallel_block   = 'parallel'
                    { set_statement with 'wait' }
                    'until' 'done'
```

### 4.9 Expression Statement

```
expression_stmt  = expression
```

---

## 5. Expressions

### 5.1 Primary Expressions

```
primary          = identifier
                 | literal
                 | '(' expression ')'
                 | list_literal
                 | record_literal
                 | function_literal
                 | 'length' 'of' additive                  // worded length
```

`length` and `at` are ordinary identifiers. `length 'of'` is the worded form
only as the two words together, and `postfix 'at' additive` is the worded index
only when an operand follows `at` on the same line — otherwise `at` is the name
it has always been.

The operand of both is an **additive** expression (§ 5.5), not a whole
expression. That is what makes `length of xs is 3` and `length(xs) is 3` the same
program rather than one that measures a comparison, and it is the only difference
from the bracketed and called spellings, which take an `expression` because a
closing `]` or `)` already ends them.

### 5.2 Postfix Expressions

```
postfix          = primary
                 | postfix '.' identifier              // property access
                 | postfix '(' [ argument { ',' argument } ] ')'  // function call
                 | postfix '[' expression ']'         // indexing
                 | postfix 'at' additive               // indexing, worded
```

The index of `'at'` is the additive expression that follows it, so `xs at n + 1`
indexes by `n + 1` while `xs at 0 is 10` compares the element with `10` —
`is`, `and` and `or` are looser than the operand and stay outside it. A second
`at` belongs to that index rather than to `xs`, so `xs at 1 at 0` is
`xs at (1 at 0)`: the two spellings of the operator do not agree on associativity,
which is why the precedence table below gives them separate rows. Bind the
intermediate value to step twice (`set row to xs at 1`, then `row at 0`), or write
the bracketed form `xs[1][0]`.

Neither worded form takes its operand from the next line. `xs at` at the end of a
line is the variable `at` (§ 5.1), so the line ends there and the analyzer
refuses it; `length of` at the end of a line is a syntax error where it is
written. The two answers differ because `at` is an ordinary name and `length of`
is not.

### 5.3 Prefix Expressions

```
prefix           = postfix
                 | 'not' prefix
                 | '-' prefix                          // negation
```

### 5.4 Multiplicative Expressions

```
multiplicative   = prefix
                 | multiplicative '*' prefix           // multiply
                 | multiplicative '/' prefix           // divide
                 | multiplicative 'mod' prefix         // modulo (same as '%')
```

### 5.5 Additive Expressions

```
additive         = multiplicative
                 | additive '+' multiplicative         // add
                 | additive '-' multiplicative        // subtract
```

### 5.6 Comparison Expressions

```
comparison       = additive
                 | comparison 'is equal to' additive   // ==
                 | comparison 'is not' additive       // !=
                 | comparison 'is less than' additive // <
                 | comparison 'is greater than' additive // >
                 | comparison 'is less than or equal to' additive // <=
                 | comparison 'is greater than or equal to' additive // >=
                 | comparison 'is in' additive         // in (list membership)
```

### 5.7 Logical AND Expressions

```
logical_and      = comparison
                 | logical_and 'and' comparison       // &&
```

### 5.8 Logical OR Expressions

```
logical_or       = logical_and
                 | logical_or 'or' logical_and       // ||
```

### 5.9 Conditional Expression (Ternary)

```
conditional      = logical_or
                 | logical_or '?' expression ':' conditional
                 | logical_or 'if' expression 'else' conditional
```

### 5.10 Assignment Expression

```
assignment_expr  = conditional
                 | identifier 'to' assignment_expr    // assignment
```

### 5.11 Expression (Final)

```
expression       = assignment_expr
                | 'might fail' call                  // error propagation
```

`might fail` is two words. Neither half is the guard on its own: `might f()` and
`fail f()` are `ParserError`s, because the pair is the form.

The guarded operand is a **call** — a function call or a method call. A prefix
that took any expression would accept `might fail 1 + 1`, whose right-hand side
cannot fail at all and which would read as though discarding a failure were
something a number could do, so anything that is not a call is a `ParserError`
naming the operand that is not one. `docs/GRAMMAR.md` previously wrote
`'might fail' expression` here, which claimed the general prefix; that was wrong
and is corrected above.

A guarded call is an expression, so it may be an argument, a record value, the
operand of an operator when parenthesized, or a statement of its own:

```redblue
set report to {body: might fail files.read("config.txt")}
say (might fail files.read("config.txt")) is nothing
might fail files.write("output.txt", body)
```

---

## 6. Function Literals

```
function_literal = 'to' [ '(' [ parameter { ',' parameter } ] ')' ] { statement } [ 'end' ]
```

The parameter list is optional, so `to give back 42 end` and `to () give back
42 end` are both literals of no parameters.

The closing `end` is optional only for a body written entirely on the `to`'s own
line — the form `SPEC.md` § First-Class Functions uses, where the end of the
line closes the literal:

```redblue
set double to to (x) give back x * 2
set doubled to list.map([1, 2, 3], to (x) give back x * 2)
```

A body that runs onto a second line needs its `end`; without it the parser
reports a spanned error naming the `end` it wanted rather than reading the rest
of the file as the body.

---

## 7. Object Literals

Not implemented, and `new` is not a reserved word: it is an ordinary identifier,
so `new Person(...)` reads as two identifiers in a row and is a parser error. The
production is kept here as the shape a future implementation has to match;
nothing below parses today. `object_literal` is therefore not an alternative of
`primary` in §5.1. A declaration is its own prototype in the meantime, so the
worked spelling is a write to the declaration — see `SPEC.md`, "Lookup order and
shadowing".

```
object_literal   = 'new' identifier '(' [ argument { ',' argument } ] ')'
```

---

## 8. Precedence Table (Lowest to Highest)

| Precedence | Operator(s) | Associativity |
|------------|-------------|---------------|
| 1 | `or` | Left |
| 2 | `and` | Left |
| 3 | `is`, `is not`, `is in` | Left |
| 4 | `<`, `<=`, `>`, `>=` | Left |
| 5 | `+`, `-` | Left |
| 6 | `*`, `/`, `mod`, `%` | Left |
| 7 | `not`, unary `-` | Right |
| 8 | `length of` | — (primary, operand is an additive) |
| 9 | property access, call, index `xs[...]` | Left |
| 10 | worded index `xs at ...` | Right — the operand is an additive, so a second `at` belongs to the first: `xs at 1 at 0` is `xs at (1 at 0)`. See § 5.2 |

---

## 9. Syntax Examples

### Hello World
```redblue
say "Hello, World!"
```

### Variable Declaration
```redblue
set name to "Alice"
set age to 30
set items to [1, 2, 3]
```

### If Statement
```redblue
if age is greater than 18
    say "You can vote"
else
    say "Too young to vote"
end
```

### For Loop
```redblue
for each i from 1 to 10
    say i
end
```

### Function
```redblue
to add(a, b)
    give back a + b
end

set result to add(1, 2)
```

### Object
```redblue
object Person
    has name
    has age
    
    to introduce()
        say "I'm {this.name}"
    end
end

set Person.name to "Bob"
Person.introduce()
```

### Try/Catch
```redblue
try
    set data to might fail files.read("config.txt")
    say data
catch error
    say "Failed to read file: {error message}"
end
```

---

## 10. Grammar Files

This grammar will be implemented using:
- **Lexer**: Custom tokenizer or Logos/Lark
- **Parser**: Recursive descent or Lark parser generator
- **Validator**: Semantic analysis after parsing

---

*Grammar v1.0 - March 2026*
