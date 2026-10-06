// Redblue comparison tests: the symbol forms of `==`, `!=`, `<`, `<=`, `>` and
// `>=`.
//
// These operators were documented (`README.md:18`, `SPEC.md:277-282`,
// `docs/GRAMMAR.md:93-98`) but the lexer had no arm for any of them, so a
// program using one died with `LexerError: Unexpected character '>'`. These
// tests are written in the operators themselves, so the suite cannot pass
// unless they lex, parse and run.

test "comparisons: greater than picks the branch the operator names"
    set x to 20
    set taken to "none"
    if x > 10 then
        set taken to "greater"
    end
    expect taken to be "greater"
end

test "comparisons: greater than or equal is true at the boundary"
    set x to 10
    set loose to no
    set strict to no
    if x >= 10 then
        set loose to yes
    end
    if x > 10 then
        set strict to yes
    end
    expect loose to be yes
    expect strict to be no
end

test "comparisons: less than and less than or equal"
    set x to 10
    set under to no
    set at_most to no
    set under_strict to no
    if x < 11 then
        set under to yes
    end
    if x <= 10 then
        set at_most to yes
    end
    if x < 10 then
        set under_strict to yes
    end
    expect under to be yes
    expect at_most to be yes
    expect under_strict to be no
end

test "comparisons: equality and inequality"
    set x to 10
    set same to no
    set different to no
    set wrongly_different to no
    if x == 10 then
        set same to yes
    end
    if x != 11 then
        set different to yes
    end
    if x != 10 then
        set wrongly_different to yes
    end
    expect same to be yes
    expect different to be yes
    expect wrongly_different to be no
end

test "edge comparisons: x against itself is strictly-less no and non-strict yes"
    set x to 5
    expect x < x to be no
    expect x > x to be no
    expect x <= x to be yes
    expect x >= x to be yes
    expect x == x to be yes
    expect x != x to be no
end

test "edge comparisons: reversed operands swap the strict answer"
    expect 1 < 2 to be yes
    expect 2 < 1 to be no
    expect 1 > 2 to be no
    expect 2 > 1 to be yes
    expect 1 <= 2 to be yes
    expect 2 <= 1 to be no
    expect 1 >= 2 to be no
    expect 2 >= 1 to be yes
end

test "edge comparisons: two lists with equal contents are equal"
    set a to [1, 2, 3]
    set b to [1, 2, 3]
    expect a == b to be yes
    expect a != b to be no
    expect [1, 2] == [2, 1] to be no
    expect [1, 2] != [2, 1] to be yes
    expect [] == [] to be yes
    expect [] == [1] to be no
    expect [7] == [7] to be yes
end

test "edge comparisons: two records with equal contents are equal"
    set a to {name: "Ada", age: 36}
    set b to {name: "Ada", age: 36}
    expect a == b to be yes
    expect a != b to be no
    expect {x: 1} == {x: 2} to be no
    expect {x: 1} != {x: 2} to be yes
    expect {} == {} to be yes
    expect {} == {a: nothing} to be no
end

test "edge comparisons: a chained ordering compares a yes/no against a number"
    // `1 < 2` is `yes`, so `yes < 3` has no defined answer. It must be a
    // catchable error, not a `no` and not a crash.
    set entered to yes
    set caught to no
    try
        set entered to no
        if 1 < 2 < 3 then
            set entered to "branch"
        end
    catch error
        set caught to yes
    end
    expect caught to be yes
    expect entered to be no
end

test "edge comparisons: ordering a list against a number is a catchable error"
    set entered to yes
    set caught to no
    try
        set entered to no
        if [1, 2] < 3 then
            set entered to "branch"
        end
    catch error
        set caught to yes
    end
    expect caught to be yes
    expect entered to be no
end

test "edge comparisons: ordering operands of different types is a catchable error"
    set entered to yes
    set caught to no
    try
        set entered to no
        if 1 < "a" then
            set entered to "branch"
        end
    catch error
        set caught to yes
    end
    expect caught to be yes
    expect entered to be no
end

test "edge comparisons: signed zero orders equal in every direction"
    expect -0.0 == 0.0 to be yes
    expect -0.0 < 0.0 to be no
    expect -0.0 <= 0.0 to be yes
    expect -0.0 >= 0.0 to be yes
    expect -0.0 > 0.0 to be no
end

test "comparisons: a comparison inside a loop counts what clears the bar"
    set count to 0
    for each i in [5, 6, 7]
        if i > 5 then
            set count to count + 1
        end
    end
    expect count to be 2
    set wide to 0
    for each i in [5, 6, 7]
        if i >= 5 then
            set wide to wide + 1
        end
    end
    expect wide to be 3
end
test "comparisons: the word forms take the branch they name"
    set x to 20
    set taken to "none"
    if x is greater than 10 then
        set taken to "greater"
    end
    expect taken to be "greater"
    if x is greater than or equal to 20 then
        set taken to "greater or equal"
    end
    expect taken to be "greater or equal"
    if x is less than 10 then
        set taken to "less"
    end
    expect taken to be "greater or equal"
    if x is less than or equal to 20 then
        set taken to "less or equal"
    end
    expect taken to be "less or equal"
    if x is equal to 20 then
        set taken to "equal"
    end
    expect taken to be "equal"
    if x is not 10 then
        set taken to "not"
    end
    expect taken to be "not"
end

test "edge comparisons: `or equal to` is one comparison, not a logical or"
    set x to 5
    set y to 5
    expect x is greater than or equal to y to be yes
    expect x is greater than y to be no
    expect x is less than or equal to y to be yes
    expect x is less than y to be no
    set z to 6
    expect x is greater than or equal to z to be no
    expect z is greater than or equal to x to be yes
    // A real `or` still binds looser than the word comparison on either side.
    expect x is greater than z or x is less than z to be yes
    expect x is greater than z or x is greater than z to be no
end

// `expect x is equal to be no` cannot be written, because `is equal to`
// would swallow the `to` that `expect` needs. So this one uses `if`.
test "edge comparisons: a variable named greater or equal still compares for equality"
    set greater to 5
    set equal to 7
    set x to 5
    expect x is greater to be yes
    set taken to no
    if x is equal then
        set taken to yes
    end
    expect taken to be no
end

test "edge comparisons: ordering a number against text in the word form is catchable"
    set entered to yes
    set caught to no
    try
        set entered to no
        if 1 is greater than "a" then
            set entered to "branch"
        end
    catch error
        set caught to yes
    end
    expect caught to be yes
    expect entered to be no
end

test "edge comparisons: ordering a record against a number in the word form is catchable"
    set entered to yes
    set caught to no
    try
        set entered to no
        if {a: 1} is less than 1 then
            set entered to "branch"
        end
    catch error
        set caught to yes
    end
    expect caught to be yes
    expect entered to be no
end

test "edge comparisons: a range check in the word forms uses both ends inclusively"
    set x to 100
    set in_range to no
    if x is greater than 0 and x is less than 100 then
        set in_range to yes
    end
    expect in_range to be no
    if x is greater than or equal to 0 and x is less than or equal to 100 then
        set in_range to yes
    end
    expect in_range to be yes
end
