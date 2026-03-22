// Redblue Examples - Complete Collection

// ============================================
// HELLO WORLD
// ============================================

say "=== Hello World ==="
say "Hello, World!"

set greeting to "Hello, World!"
say greeting

// ============================================
// VARIABLES
// ============================================

say "\n=== Variables ==="
set name to "Alice"
set age to 30
set height to 5.8
set is_student to yes
set hobbies to ["reading", "coding", "gaming"]

say "Name: {name}"
say "Age: {age}"
say "Height: {height}"
say "Student: {is_student}"
say "Hobbies: {hobbies}"

// ============================================
// MATHEMATICS
// ============================================

say "\n=== Mathematics ==="
set x to 10
set y to 3

say "x = {x}, y = {y}"
say "x + y = {x + y}"
say "x - y = {x - y}"
say "x * y = {x * y}"
say "x / y = {x / y}"
say "x mod y = {x mod y}"

set a to 5
set b to 2
set power to a ^ b
say "{a} raised to {b} = {power}"

// ============================================
// CONDITIONALS
// ============================================

say "\n=== Conditionals ==="
set score to 85

if score is greater than 90
    say "Grade: A"
else if score is greater than 80
    say "Grade: B"
else if score is greater than 70
    say "Grade: C"
else
    say "Grade: F"
end

set temperature to 25

if temperature is greater than 30
    say "It's hot!"
else if temperature is greater than 20
    say "It's nice!"
else if temperature is greater than 10
    say "It's cold!"
else
    say "It's freezing!"
end

// ============================================
// LOOPS
// ============================================

say "\n=== Loops ==="
say "Counting from 1 to 5:"
for each i from 1 to 5
    say i
end

say "\nRepeat 3 times:"
repeat 3 times
    say "Hello!"
end

say "\nIterating over list:"
set colors to ["red", "green", "blue"]
for each color in colors
    say color
end

// ============================================
// FUNCTIONS
// ============================================

say "\n=== Functions ==="

to greet(name)
    say "Hello, {name}!"
end

to add(a, b)
    give back a + b
end

to max(a, b)
    if a is greater than b
        give back a
    else
        give back b
    end
end

to factorial(n)
    if n is less than or equal to 1
        give back 1
    end
    give back n * factorial(n - 1)
end

greet("World")
set sum to add(10, 20)
say "10 + 20 = {sum}"
set biggest to max(15, 8)
say "max(15, 8) = {biggest}"
set fact to factorial(5)
say "5! = {fact}"

// ============================================
// LISTS
// ============================================

say "\n=== Lists ==="
set numbers to [1, 2, 3, 4, 5]
set first to numbers at 0
set last to numbers at -1
set length to numbers.length
say "First: {first}, Last: {last}, Length: {length}"

set doubled to []
for each n in numbers
    // Note: List operations would need stdlib functions
end

// ============================================
// RECORDS (OBJECTS)
// ============================================

say "\n=== Records ==="
set person to record {
    name: "Bob",
    age: 25,
    city: "New York"
}

say "Name: {person.name}"
say "Age: {person.age}"
say "City: {person.city}"

// ============================================
// STRING MANIPULATION
// ============================================

say "\n=== Strings ==="
set text to "  Hello, Redblue!  "
set upper to text.uppercase()
set lower to text.lowercase()
set trimmed to text.trim()

say "Original: '{text}'"
say "Uppercase: '{upper}'"
say "Lowercase: '{lower}'"
say "Trimmed: '{trimmed}'"

set words to "apple,banana,cherry".split(",")
say "Words: {words}"

// ============================================
// ERROR HANDLING
// ============================================

say "\n=== Error Handling ==="
say "Try-catch example:"

try
    set data to "This is a test"
    // Simulate error (would use 'might fail' in real code)
    say "Inside try block"
catch error
    say "Caught error: {error}"
finally
    say "Finally block executed"
end

// ============================================
// PRACTICAL EXAMPLE
// ============================================

say "\n=== Practical Example: FizzBuzz ==="
for each n from 1 to 20
    if n mod 15 is 0
        say "FizzBuzz"
    else if n mod 3 is 0
        say "Fizz"
    else if n mod 5 is 0
        say "Buzz"
    else
        say n
    end
end

say "\n=== All examples completed! ==="
