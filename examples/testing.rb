// Testing Framework Example
// Demonstrates built-in test blocks and assertions

test "basic arithmetic"
    set result to 2 + 3
    expect result to be 5
    expect 10 - 4 to be 6
    expect 4 * 3 to be 12
    expect 15 / 3 to be 5
end

test "string operations"
    set greeting to "Hello"
    set name to "World"
    set combined to greeting + ", " + name + "!"
    expect combined to be "Hello, World!"
    expect length("test") to be 4
end

test "list operations"
    set numbers to [1, 2, 3, 4, 5]
    expect length(numbers) to be 5
    expect numbers at 0 to be 1
    expect numbers at 4 to be 5
end

test "comparison operations"
    expect 10 is greater than 5 to be yes
    expect 3 is less than 7 to be yes
    expect 5 is equal to 5 to be yes
    expect 5 is not equal to 3 to be yes
end

test "boolean logic"
    expect yes and yes to be yes
    expect yes or no to be yes
    expect not no to be yes
    expect not yes to be no
end

say "All tests passed!"