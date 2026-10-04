// Redblue closure tests.
//
// A function value carries the local scopes that were live where it was
// declared (phase-011). The policy these tests pin down:
//
// * capture by value, at declaration time — a body reads the values its
//   declaration was next to, and an assignment inside a body stays inside
//   that one call
// * globals are not captured, so a closure that counts in a global counts for
//   every call
// * the captured scopes sit above the caller's frames, so an escaped closure
//   reads its own environment and not a same-named binding of its caller
//
// `give back` does not leave a function in this build, so a conditional result
// is computed into a variable and returned after the block.

test "closures: a nested declaration reads the enclosing binding"
    to make_adder(x)
        set bias to 1
        to add(y)
            give back x + y + bias
        end
        give back add
    end
    set add_tens to make_adder(10)
    expect add_tens(5) to be 16
end

test "closures: two declarations of one name keep their own bodies"
    to make_a(start)
        set count to start
        to inner()
            give back count + 1
        end
        give back inner
    end
    to make_b(start)
        set count to start
        to inner()
            give back count + 100
        end
        give back inner
    end
    set a to make_a(1)
    set b to make_b(1)
    expect a() to be 2
    expect b() to be 101
end

test "closures: a parameter shadows what the declaration captured"
    to outer(v)
        to inner(v)
            give back v * 2
        end
        give back inner(21)
    end
    expect outer(5) to be 42
end

test "closures: three levels of nesting each capture the level outside"
    to level_one(a)
        to level_two(b)
            set from_two to b * 2
            to level_three(c)
                set from_three to c + 1
                give back a + b + from_two + from_three
            end
            give back level_three(1)
        end
        give back level_two(10)
    end
    expect level_one(100) to be 132
end

test "closures: a global is read at call time, not captured"
    set ticks to 0
    to make_ticker()
        to tick()
            set ticks to ticks + 1
            give back ticks
        end
        give back tick
    end
    set a_tick to make_ticker()
    set b_tick to make_ticker()
    expect a_tick() to be 1
    expect b_tick() to be 2
    expect a_tick() to be 3
end

test "closures: two nested declarations reach each other"
    to outer(n)
        to even(k)
            set answer to yes
            if k is 0 then
                set answer to yes
            else
                set answer to odd(k - 1)
            end
            give back answer
        end
        to odd(k)
            set answer to no
            if k is 0 then
                set answer to no
            else
                set answer to even(k - 1)
            end
            give back answer
        end
        give back even(n)
    end
    expect outer(10) to be yes
    expect outer(7) to be no
    expect outer(0) to be yes
end

test "closures: a function value called through an alias runs its own body"
    to add(a, b)
        give back a + b
    end
    set alias to add
    expect type_of(alias) to be "function"
    expect alias(2, 3) to be 5
end

test "edge_closures_an_escaped_closure_reads_its_own_environment"
    to make_adder(x)
        to add(y)
            give back x + y
        end
        give back add
    end
    set add_ten to make_adder(10)
    to caller()
        set x to 1000
        give back add_ten(5)
    end
    expect caller() to be 15
end

test "edge_closures_capture_is_by_value_so_an_assignment_does_not_escape"
    to outer(seed)
        to bump(amount)
            set seed to seed + amount
            give back seed
        end
        set first to bump(1)
        set second to bump(1)
        give back first * 1000 + second
    end
    expect outer(10) to be 11011
end

test "edge_closures_an_empty_captured_value_survives"
    to outer()
        set text to ""
        to describe()
            give back text
        end
        give back describe
    end
    set d to outer()
    expect type_of(d()) to be "text"
    expect length(d()) to be 0
end

test "edge_closures_a_closure_declared_in_a_loop_captures_that_iteration"
    set total to 0
    for each n in [10, 20, 30]
        to scaled(x)
            give back x + n
        end
        set total to total + scaled(0)
    end
    expect total to be 60
end

test "edge_closures_an_undefined_name_inside_a_closure_is_a_caught_error"
    to outer()
        to inner()
            no_such_helper(1)
        end
        give back inner
    end
    set broken to outer()
    set caught to no
    try
        set ignored to broken()
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_closures_a_caught_failure_leaves_the_scope_stack_balanced"
    to outer(n)
        to blow_up()
            give back n / 0
        end
        to fine()
            give back n * 2
        end
        give back [blow_up, fine]
    end
    set both to outer(21)
    set caught to no
    try
        set bad to both[0]
        set ignored to bad()
    catch error
        set caught to yes
    end
    set good to both[1]
    expect caught to be yes
    expect good() to be 42
end

test "edge_closures_unbounded_closure_recursion_is_a_caught_error"
    to outer()
        to spin(n)
            spin(n + 1)
        end
        give back spin
    end
    set spin to outer()
    set caught to no
    try
        spin(0)
    catch error
        set caught to yes
    end
    expect caught to be yes
    set caught_again to no
    try
        spin(0)
    catch error
        set caught_again to yes
    end
    expect caught_again to be yes
end
test "edge_closures_assigning_to_an_enclosing_binding_reaches_it"
    to add_to(base)
        for each v in [1, 2]
            set base to base + v
        end
        give back base
    end
    expect add_to(10) to be 13
end

test "closures: two closures over one name do not share an assignment"
    to outer(seed)
        to left(amount)
            set seed to seed + amount
            give back seed
        end
        to right(amount)
            set seed to seed + amount
            give back seed
        end
        set one to left(10)
        set two to right(1000)
        give back [one, two]
    end
    expect outer(5) to be [15, 1005]
end
