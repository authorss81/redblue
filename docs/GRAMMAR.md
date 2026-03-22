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
keywords = 'set' | 'to' | 'is' | 'are' | 'nothing' | 'yes' | 'no'
         | 'if' | 'then' | 'else' | 'end' | 'when' | 'unless'
         | 'for' | 'each' | 'in' | 'from' | 'times' | 'while' | 'repeat'
         | 'break' | 'skip' | 'return' | 'give back'
         | 'to' | 'takes' | 'needs' | 'called' | 'might fail'
         | 'number' | 'text' | 'yes/no' | 'list' | 'record' | 'object'
         | 'module' | 'import' | 'export' | 'as'
         | 'and' | 'or' | 'not' | 'either' | 'neither'
         | 'try' | 'catch' | 'finally' | 'error'
         | 'has' | 'can' | 'this' | 'that' | 'new' | 'extends' | 'every'
         | 'async' | 'wait' | 'parallel' | 'until'
```

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

```
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
// Variable declaration
declaration      = 'set' identifier [ ':' type_expression ] 'to' expression

// Multiple assignment
declaration      = 'set' identifier { ',' identifier } 'to' expression { ',' expression }

// Constant declaration
declaration      = 'constant' identifier [ ':' type_expression ] 'to' expression
```

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

// When statement (pattern matching)
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
skip_statement   = 'skip' [ expression ]
return_statement = 'return' [ expression ]
                 | 'give back' [ expression ]
```

### 4.5 Function Declaration

```
function_decl    = 'to' identifier [ '(' [ parameter { ',' parameter } ] ')' ]
                   [ 'needs' type_expression { ',' type_expression } ]
                   [ 'gives' type_expression ]
                   { statement }
                   'end'

parameter        = identifier [ ':' type_expression ]
                 | identifier 'of' type_expression
```

### 4.6 Object Declaration

```
object_decl      = 'object' identifier [ 'extends' identifier ]
                    { property_decl | method_decl }
                    'end'

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
                    { 'catch' [ identifier ] [ 'of' identifier ]
                      { statement } }
                    [ 'finally'
                      { statement } ]
                    'end'
```

### 4.8 Async Statements

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
                 | object_literal
```

### 5.2 Postfix Expressions

```
postfix          = primary
                 | postfix '.' identifier              // property access
                 | postfix '(' [ argument { ',' argument } ] ')'  // function call
                 | postfix '[' expression ']'         // indexing
                 | postfix 'of' expression            // generic type
```

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
                 | multiplicative 'mod' prefix         // modulo
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
                 | comparison 'in' additive           // in (contains)
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
                 | 'might fail' expression            // error propagation
```

---

## 6. Function Literals

```
function_literal = 'to' [ '(' [ parameter { ',' parameter } ] ')' ]
                    { statement }
                    'end'
```

---

## 7. Object Literals

```
object_literal   = 'new' identifier '(' [ argument { ',' argument } ] ')'
```

---

## 8. Precedence Table (Lowest to Highest)

| Precedence | Operator(s) | Associativity |
|------------|-------------|---------------|
| 1 | `or` | Left |
| 2 | `and` | Left |
| 3 | `is`, `is not`, `in` | Left |
| 4 | `<`, `<=`, `>`, `>=` | Left |
| 5 | `+`, `-` | Left |
| 6 | `*`, `/`, `mod` | Left |
| 7 | `not`, unary `-` | Right |
| 8 | `of` | Left |
| 9 | property access, call, index | Left |

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

set person to new Person
set name of person to "Bob"
person.introduce()
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
