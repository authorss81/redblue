// Redblue numeric edge tests.
//
// A `number` is an f64, so every arithmetic operator can leave the reals:
// `5 % 0` is NaN and `1e308 * 1e308` is infinity. Redblue refuses those results
// instead of storing them, which makes them ordinary catchable runtime errors.
// See SPEC.md, "Numeric semantics".

test "arithmetic: division by zero is a catchable runtime error"
    set caught to no
    try
        set x to 1 / 0
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_modulo by zero is a catchable runtime error"
    set caught to no
    try
        set x to 5 % 0
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_overflow to infinity is a catchable runtime error"
    set caught to no
    try
        set x to 1e308 * 1e308
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_a number literal out of range is refused"
    set caught to no
    try
        set x to 1e400
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "arithmetic: modulo keeps the sign of the dividend"
    set a to 5 % 3
    set b to -5 % 3
    set c to 5 % -3
    set d to 5 % 0.5
    expect a to be 2
    expect b to be -2
    expect c to be 2
    expect d to be 0
end

test "edge_integers are exact up to 2^53 and rounded past it"
    set exact to 9007199254740992
    set rounded to 9007199254740993
    expect exact to be 9007199254740992
    expect rounded to be 9007199254740992
end

test "edge_negative zero equals zero"
    set negative to 0 * -1
    expect negative to be 0
    expect negative is not 1 to be yes
end

test "numeric: a number wider than i64 is still a number"
    set wide to 99999999999999999999
    expect type_of(wide) to be "number"
    // The f64 holds 100000000000000000000 exactly, so that is the number this
    // is - not i64's saturated maximum.
    expect wide is 100000000000000000000 to be yes
end