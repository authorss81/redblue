// The word forms of the modulo and membership operators.
//
// `mod` and `is in` are documented in SPEC.md:206 and SPEC.md:325 but were not
// lexed or parsed, so the programs below died with
// `AnalyzerError: Unknown variable 'mod'` and
// `ParserError: Unexpected token In`. They are written here in the language so
// the gate covers the same paths a user writes.

test "mod: the word form computes the documented remainder"
    set remainder to 10 mod 3
    expect remainder to be 1
    expect 10 % 3 to be 1
end

test "mod: the word form and the symbol agree on the sign of the dividend"
    set word to -5 mod 3
    set symbol to -5 % 3
    expect word to be -2
    expect symbol to be -2
end

test "edge_mod: a zero divisor is caught, not fatal"
    set caught to no
    try
        set bad to 10 mod 0
    catch failure
        set caught to failure
    end
    expect caught to be "error"
end

test "edge_mod: a text operand is caught"
    set caught to no
    try
        set bad to 10 mod "2"
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "is in: membership selects the branch it names"
    set words to ["a", "b"]
    set hit to no
    set miss to no
    if "a" is in words then
        set hit to yes
    end
    if "z" is in words then
        set miss to yes
    end
    expect hit to be yes
    expect miss to be no
end

test "edge_is_in: an empty list contains nothing"
    set found to yes
    if "a" is in [] then
        set found to yes
    else
        set found to no
    end
    expect found to be no
    expect nothing is in [] to be no
end

test "edge_is_in: an empty text haystack is an error, not a substring search"
    set caught to no
    try
        set found to "a" is in ""
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_is_in: a record or a number haystack is caught"
    set caught to 0
    try
        set a to "a" is in {name: "a"}
    catch error
        set caught to caught + 1
    end
    try
        set b to "a" is in 5
    catch error
        set caught to caught + 2
    end
    expect caught to be 3
end

test "edge_is_in: a nothing needle and a wrong-type needle"
    expect nothing is in [nothing] to be yes
    expect nothing is in [1, 2] to be no
    expect 1 is in ["1"] to be no
    expect "1" is in [1] to be no
end