# Redblue Design Philosophy

## Core Principles

### 1. Plain English First
Code should read like a sentence in a well-written instruction manual.

**Good:**
```redblue
if user age is greater than 18 then
    allow user to vote
end
```

**Bad (traditional):**
```redblue
if (user.age > 18) {
    vote();
}
```

### 2. Zero Cognitive Overhead
Every keyword should have one clear meaning. No symbolic operators except where universally understood.

**Allowed symbols:**
- `=` for assignment (familiar)
- `+`, `-`, `*`, `/` for math (universal)
- `==`, `!=`, `<`, `>`, `<=`, `>=` for comparison (familiar)
- `" "` for text (familiar)
- `[ ]` for lists (familiar)

**Forbidden symbols:**
- `&&`, `||` (use `and`, `or`)
- `!` (use `not`)
- `?:` ternary (use `if-then-else`)
- `++`, `--` (use `add 1 to`, `subtract 1 from`)
- `->`, `=>` (use `to`, `gives`)
- `::`, `|` (use natural language alternatives)

### 3. Minimal Keyword Count
Target: 28-32 keywords (vs Python's 35, JavaScript's 50+)

**Required keywords:**
```
// Variables & Values
set, to, is, are, becomes, nothing

// Control Flow
if, then, else, end, when, unless
for, each, in, from, times, while
repeat, until, break, skip, return

// Functions
to, give back, takes, needs, called

// Types
number, text, yes/no, list, record, object, nothing

// Structure
module, import, export, as

// Logic
and, or, not, either, neither

// Errors
try, catch, finally, might fail, error

// OOP
has, can, this, that, new, extends, every

// Async
async, wait, parallel, until done
```

**Total: 32 keywords**

### 4. Maximum Capability
Never sacrifice power for simplicity. Every feature must be capable.

**Must support:**
- Variables and all primitive types
- Functions (first-class, higher-order, closures)
- Objects and inheritance
- Lists, records, sets
- Pattern matching
- Async/await
- Error handling
- Type inference
- Generics
- Standard library (files, network, math, text)

### 5. Human-Friendly Errors
Error messages should explain what went wrong and how to fix it.

**Good:**
```
I couldn't find a file called 'config.txt' in the current folder.
Tip: Check the file path or use 'files.exists("config.txt")' to check first.
```

**Bad (traditional):**
```
FileNotFoundError: [Errno 2] No such file: 'config.txt'
```

## Anti-Patterns (What We Won't Support)

### 1. No Semicolons
Use newlines and indentation instead. Semicolons add cognitive load for no benefit.

### 2. No Braces
Use `end` to close blocks. Braces are syntactically noisy.

### 3. No Single-Character Variables Required
`i`, `j`, `k` are valid but `index`, `counter`, `name` are preferred.

### 4. No Underscore Variable Prefix
Variables can be named naturally: `my_data`, `first_name`.

### 5. No Magic Methods
Use explicit method names. `__init__` becomes `to create` or `to initialize`.

### 6. No Silent Failures
Every operation that can fail should be explicit. Use `might fail` keyword.

### 7. No Null Pointer Exceptions
Use `nothing` explicitly. Type system prevents unexpected nulls.

## Design Examples

### Variable Declaration
```redblue
set name to "Alice"
set age to 30
set score to 95.5
set is active to yes
set items to empty list
set data to nothing
```

### Conditional Logic
```redblue
if temperature is greater than 30
    say "It's hot!"
else if temperature is less than 10
    say "It's cold!"
else
    say "It's comfortable"
end
```

### Loops
```redblue
// Count from 1 to 10
repeat 10 times
    say "Hello"
end

// Loop through items
for each person in people
    say "Hello, {person name}!"
end

// Loop with range
for each i from 1 to 100
    say i
end

// While loop
set counter to 0
while counter is less than 10
    add 1 to counter
end
```

### Functions
```redblue
to greet(name)
    say "Hello, {name}!"
    give back "Greeted {name}"
end

to calculate total(items)
    set sum to 0
    for each item in items
        add item price to sum
    end
    give back sum
end

to might fail fetch data(url)
    // Network calls can fail
    give back request.get(url)
end
```

### Objects
```redblue
object Person
    has name
    has age
    has email
    
    to introduce()
        say "I'm {this.name}, {this.age} years old"
    end
    
    to can email others(message)
        // Implementation
    end
end

object Customer extends Person
    has customer_id
    has purchase history
    
    to get discount()
        if this purchase count is greater than 10
            give back 0.1
        else
            give back 0
        end
    end
end
```

### Async Operations
```redblue
async to fetch users()
    set response to wait network.get("https://api.example.com/users")
    give back formats.parse_json(response)
end

async to process all()
    parallel
        set users to wait fetch_users()
        set products to wait fetch_products()
        set orders to wait fetch_orders()
    until done
    
    give back combine_data(users, products, orders)
end
```

### Error Handling
```redblue
try
    set data to might fail files.read("data.txt")
    process data
catch error
    say "Something went wrong: {error message}"
finally
    close connections
end
```

## Comparison with Other Languages

| Feature | Redblue | Python | JavaScript | Ruby |
|---------|-----------|--------|------------|------|
| Keywords | 32 | 35 | 50+ | 40+ |
| Readability | 10/10 | 8/10 | 6/10 | 7/10 |
| Type safety | Optional | Dynamic | Dynamic | Dynamic |
| OOP | Yes | Yes | Yes | Yes |
| Functional | Yes | Yes | Yes | Yes |
| Async | Yes | Yes (3.5+) | Yes | Yes |
| Standard lib | Growing | Excellent | Excellent | Good |
| Performance | Target <2x Python | Baseline | 5-50x Python | 5-30x Python |

## Success Criteria

Redblue succeeds when:

1. **A non-programmer can read a Redblue program and understand what it does**
2. **A beginner can write their first working program in under 5 minutes**
3. **An experienced developer can do anything they can in Python or JavaScript**
4. **Code review can be done by reading aloud the code**

## Implementation Priorities

1. Core language features (variables, functions, control flow)
2. Standard library (files, text, math)
3. Object system
4. Async/await
5. Type system
6. Pattern matching
7. Generics
8. Advanced optimizations

---

*Philosophy v1.0 - March 2026*
