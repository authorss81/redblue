// Redblue function tests.
//
// Redblue functions are values: a declaration binds a name to a `function`
// value, and a call runs the body in its own scope and yields the value of its
// last statement. Call depth is bounded by MAX_CALL_DEPTH (phase-006), so
// unbounded recursion is a caught error rather than a crash.

test "functions: a declaration binds a function value"
    to add(a, b)
        give back a + b
    end
    expect type_of(add) to be "function"
end

test "functions: a zero parameter declaration is still a function"
    to now()
        give back 1
    end
    expect type_of(now) to be "function"
end

test "functions: two declarations bind two distinct names"
    to first(a)
        give back a
    end
    to second(b)
        give back b
    end
    expect type_of(first) to be "function"
    expect type_of(second) to be "function"
    expect first is second to be no
end

test "functions: a name is not a shadow of a variable of the same type"
    set value to 1
    to helper(v)
        give back v
    end
    expect type_of(value) to be "number"
    expect type_of(helper) to be "function"
end

test "functions: a nested declaration is accepted"
    to outer(n)
        to inner(m)
            give back m
        end
        give back n
    end
    expect type_of(outer) to be "function"
end

test "functions: a body may contain control flow"
    to classify(n)
        if n is 0 then
            give back "zero"
        else
            give back "other"
        end
    end
    expect type_of(classify) to be "function"
end

test "functions: a body may contain a loop"
    to total(values)
        set sum to 0
        for each v in values
            set sum to sum + v
        end
        give back sum
    end
    expect type_of(total) to be "function"
end

test "functions: a declaration may sit beside ordinary statements"
    set before to 1
    to middle(v)
        give back v
    end
    set after to 2
    expect type_of(middle) to be "function"
    expect before to be 1
    expect after to be 2
end

test "functions: two parameters are accepted"
    to scale(value, factor)
        give back value * factor
    end
    expect type_of(scale) to be "function"
end

test "functions: many parameters are accepted"
    to combine(a, b, c, d, e)
        give back a + b + c + d + e
    end
    expect type_of(combine) to be "function"
end

test "functions: a built-in is not a user function"
    expect type_of(type_of) to be "builtin"
    expect type_of(type_of) is "function" to be no
end

test "edge_functions_calling_an_undefined_function_is_a_caught_error"
    set caught to no
    try
        set bad to no_such_function(1)
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_functions_a_declaration_does_not_disturb_its_neighbours"
    to helper(v)
        give back v
    end
    set total to 0
    for each v in [1, 2, 3]
        set total to total + v
    end
    expect type_of(helper) to be "function"
    expect total to be 6
end

test "edge_functions_three_nested_declarations_parse"
    to level_one(a)
        to level_two(b)
            to level_three(c)
                give back c
            end
            give back b
        end
        give back a
    end
    expect type_of(level_one) to be "function"
end

test "functions: a call runs the body and yields its last value"
    to add(a, b)
        give back a + b
    end
    expect add(2, 3) to be 5
end

test "functions: a call sees only its own parameters"
    to shadow(v)
        give back v
    end
    set v to 1
    expect shadow(2) to be 2
    expect v to be 1
end

test "edge_functions_unbounded_recursion_is_a_caught_error"
    to boom(n)
        boom(n + 1)
    end
    set caught to no
    try
        boom(0)
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_functions_mutual_recursion_is_bounded"
    set caught to no
    to ping(n)
        pong(n + 1)
    end
    to pong(n)
        ping(n + 1)
    end
    try
        ping(0)
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_functions_a_caught_depth_error_leaves_the_program_usable"
    to boom(n)
        boom(n + 1)
    end
    to double(v)
        give back v * 2
    end
    try
        boom(0)
    catch error
        set marker to yes
    end
    expect double(21) to be 42
end
