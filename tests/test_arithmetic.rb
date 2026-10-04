// Redblue arithmetic tests.
//
// Every block is self-contained: the harness runs each `test` body on its own,
// and the analyser only sees variables declared at the body's top level.

test "arithmetic: integer addition"
    set a to 10
    set b to 20
    set sum to a + b
    expect sum to be 30
end

test "arithmetic: integer subtraction can go negative"
    set diff to 50 - 30
    set under to 1 - 5
    expect diff to be 20
    expect under to be -4
end

test "arithmetic: multiplication"
    set product to 6 * 7
    set chained to 2 * 3 * 4
    expect product to be 42
    expect chained to be 24
end

test "arithmetic: division keeps the fractional part"
    set whole to 100 / 4
    set half to 7 / 2
    expect whole to be 25
    expect half to be 3.5
end

test "arithmetic: modulo"
    set rest to 17 % 5
    set even to 2 % 2
    expect rest to be 2
    expect even to be 0
end

test "arithmetic: multiplication binds tighter than addition"
    set value to 3 * 2 + 1
    expect value to be 7
end

test "arithmetic: parentheses override precedence"
    set value to (1 + 2) * 3
    expect value to be 9
end

test "arithmetic: subtraction of a negative adds"
    set value to 2 - -3
    expect value to be 5
end

test "arithmetic: text plus text concatenates"
    set greeting to "Hello, "
    set name to "World!"
    set message to greeting + name
    expect message to be "Hello, World!"
end

test "arithmetic: repeated addition accumulates in a loop"
    set total to 0
    repeat 5 times
        set total to total + 2
    end
    expect total to be 10
end

test "edge_arithmetic_division_by_zero_is_a_caught_runtime_error"
    set caught to no
    try
        set bad to 1 / 0
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_arithmetic_division_by_zero_is_not_a_silent_infinity"
    set survived to yes
    try
        set bad to 1 / 0
    catch error
        set survived to no
    end
    expect survived to be no
end

test "edge_arithmetic_zero_over_zero_also_faults"
    set caught to no
    try
        set bad to 0 / 0
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_arithmetic_modulo_by_zero_is_a_catchable_runtime_error"
    // 5 % 0 has no answer: an f64 modulo by zero is NaN, and Redblue refuses to
    // store a NaN in a number. It is an ordinary catchable runtime error.
    set caught to no
    try
        set bad to 5 % 0
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_arithmetic_integers_past_2_53_lose_precision"
    // 9007199254740993 is 2^53+1, which is not representable as an f64, so the
    // literal silently becomes 9007199254740992.
    expect 9007199254740993 to be 9007199254740992
end

test "edge_arithmetic_negative_zero_compares_equal_to_zero"
    expect -0.0 is 0 to be yes
end

test "edge_arithmetic_decimal_addition_is_not_exact"
    // 0.1 + 0.2 is 0.30000000000000004 in binary floating point. Asserting
    // equality with 0.3 would be asserting a rounding that never happens.
    expect 0.1 + 0.2 is 0.3 to be no
end

test "edge_arithmetic_repeating_decimal_is_deterministic"
    expect 1 / 3 to be 0.3333333333333333
    expect 1 / 3 is 1 / 3 to be yes
end

test "edge_arithmetic_number_plus_text_is_a_caught_type_error"
    set caught to no
    try
        set bad to 1 + "x"
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_arithmetic_list_plus_list_is_a_caught_type_error"
    set caught to no
    try
        set bad to [1, 2] + 1
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "arithmetic: large but exact products"
    set product to 1000000 * 1000000
    expect product to be 1000000000000
end

test "arithmetic: one is the multiplicative identity"
    set a to 99
    expect a * 1 to be 99
    expect a * 0 to be 0
end