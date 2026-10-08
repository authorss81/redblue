// Redblue control-flow tests.

test "control: if takes the then branch"
    set branch to "none"
    set n to 1
    if n is 1 then
        set branch to "then"
    end
    expect branch to be "then"
end

test "control: if takes the else branch"
    set branch to "none"
    set n to 2
    if n is 1 then
        set branch to "then"
    else
        set branch to "else"
    end
    expect branch to be "else"
end

test "control: an if with no else leaves the accumulator alone"
    set branch to "none"
    set n to 2
    if n is 1 then
        set branch to "then"
    end
    expect branch to be "none"
end

test "control: an else if chain picks the middle branch"
    set branch to "other"
    set n to 0
    if n is 1 then
        set branch to "positive"
    else
        if n is 0 then
            set branch to "zero"
        else
            set branch to "negative"
        end
    end
    expect branch to be "zero"
end

test "control: nested ifs evaluate the inner condition"
    set depth to 0
    set a to 1
    set b to 1
    if a is 1 then
        if b is 1 then
            set depth to 2
        end
    end
    expect depth to be 2
end

test "control: an inner if that does not match leaves the outer value"
    set depth to 0
    set a to 1
    set b to 2
    if a is 1 then
        if b is 1 then
            set depth to 2
        end
    end
    expect depth to be 0
end

test "control: repeat runs its body the requested number of times"
    set n to 0
    repeat 7 times
        set n to n + 1
    end
    expect n to be 7
end

test "control: repeat zero times leaves the accumulator alone"
    set n to 0
    repeat 0 times
        set n to n + 1
    end
    expect n to be 0
end

test "control: repeat once runs exactly one iteration"
    set n to 0
    repeat 1 times
        set n to n + 1
    end
    expect n to be 1
end

test "control: repeat can drive a text accumulator"
    set out to ""
    repeat 4 times
        set out to out + "ab"
    end
    expect out to be "abababab"
    expect length(out) to be 8
end

test "control: while runs until its condition stops holding"
    set n to 0
    while n is not 5
        set n to n + 1
    end
    expect n to be 5
end

test "control: a while whose condition is false at once does nothing"
    set n to 0
    while n is 1
        set n to n + 1
    end
    expect n to be 0
end

test "control: while builds a text accumulator"
    set out to ""
    set n to 0
    while n is not 3
        set out to out + "x"
        set n to n + 1
    end
    expect out to be "xxx"
end

test "control: for each walks a list in order"
    set total to 0
    set count to 0
    for each value in [3, 1, 2]
        set count to count + 1
        set total to total + value
    end
    expect count to be 3
    expect total to be 6
end

test "control: comparison uses is for equality"
    expect 5 is 5 to be yes
    expect 5 is 6 to be no
    expect "a" is "a" to be yes
    expect "a" is "b" to be no
end

test "control: is not is the negated comparison"
    expect 5 is not 6 to be yes
    expect 5 is not 5 to be no
    expect nothing is not nothing to be no
end

test "control: and requires both sides"
    expect yes and yes to be yes
    expect yes and no to be no
    expect no and yes to be no
    expect no and no to be no
end

test "control: an if can branch on a boolean variable"
    set flag to yes
    set out to "no"
    if flag then
        set out to "yes"
    end
    expect out to be "yes"
end

test "edge_control_conditional_branch_does_not_leak_into_the_enclosing_scope"
    // The analyser gives an `if` body its own scope, so a name first written
    // there is not visible afterwards and the program is rejected.
    set n to 0
    if n is 0 then
        set only_inside to 1
    end
    expect n to be 0
end

test "edge_control_an_empty_condition_source_still_parses"
    set n to 0
    if n is 0 then
        set n to 1
    else
        set n to 2
    end
    expect n to be 1
end

test "edge_control_a_type_mismatch_in_a_condition_is_not_silently_true"
    set r to {a: 1}
    expect r is [1] to be no
    expect r is 1 to be no
    expect r is "a" to be no
end

test "edge_control_nothing_is_falsy_in_an_if"
    set taken to "skipped"
    set missing to nothing
    if missing is nothing then
        set taken to "matched"
    end
    expect taken to be "matched"
end
test "edge: a bare catch runs its body"
    // `catch` with no name binding. The parser leaves catch_var empty, and both
    // the VM and the formatter used to treat that as "no catch at all" - so the
    // body never ran and the formatter deleted it.
    set seen to "no"
    try
        say 1 / 0
    catch
        set seen to "yes"
    end
    expect seen to be "yes"
end

test "edge: a bare catch still runs when finally follows"
    set log to ""
    try
        say 1 / 0
    catch
        set log to "c"
    finally
        set log to log + "f"
    end
    expect log to be "cf"
end

test "edge: a named catch still binds the error"
    set caught to ""
    try
        say 1 / 0
    catch err
        set caught to err
    end
    expect caught to be "error"
end

test "edge: a catch leaves the function body it was written in alone"
    // A catch runs in a scope of its own and gives it back - and nothing else.
    // The bytecode VM pushed a scope for the body and popped it again once the
    // body had run, though finishing the body's frame had already taken it, so
    // the second removal took the function's own scope with it. `total` is
    // declared before the try and `x` is a parameter, so the body reads both
    // kinds of name the function owns - and both came back unknown.
    to add(x)
        set total to 0
        try
            set bad to 1 + "one"
        catch error
            set caught to yes
        end
        set total to total + x
        give back total
    end

    expect add(7) to be 7
end

test "edge: a finally is owed when there is no catch to handle the failure"
    // The form SPEC.md documents takes no `catch` beside the `finally`, and a
    // failure is the case the cleanup exists for - so the tree-walking VM used
    // to return above `run_finally_body` and skip it on exactly that path,
    // while the bytecode VM ran it and then swallowed the failure. One promise
    // kept and one broken on the two engines: the two below say the finally
    // runs *and* that the failure is the enclosing `try`'s to catch.
    set log to ""
    try
        try
            set bad to 1 + "one"
        finally
            set log to log + "f"
        end
    catch error
        set log to log + "c"
    end
    expect log to be "fc"
end

test "edge: a try with no catch hands the failure to the one around it"
    // A `try` with no `catch` is not a handler. The bytecode VM's
    // `handle_failure` answered `true` for one anyway - it ran the `finally`
    // and then resumed past the marked `NOP`, so the failure vanished and the
    // program carried on as though the region had succeeded. This catches it.
    set reached to "no"
    try
        try
            set bad to 1 + "one"
        end
        set reached to "yes"
    catch error
        set reached to "caught"
    end
    expect reached to be "caught"
end

test "edge: a failing finally still leaves the outer try to catch the failure"
    // The cleanup ran and it is the cleanup that failed, so the failure that
    // reaches the enclosing `try` is that one - the original is spent. Both VMs
    // have to agree on which, and the `finally`'s `f` says it ran.
    set log to ""
    try
        try
            set bad to 1 + "one"
        finally
            set log to log + "f"
            set bad to 1 + "two"
        end
    catch error
        set log to log + "c"
    end
    expect log to be "fc"
end

test "control: unless takes its body when the condition is false"
    set branch to "none"
    set n to 0
    unless n is 1 then
        set branch to "body"
    end
    expect branch to be "body"
end

test "control: unless skips its body when the condition is true"
    set branch to "none"
    set n to 1
    unless n is 1 then
        set branch to "body"
    end
    expect branch to be "none"
end

test "edge: an unless with an empty body changes nothing"
    set branch to "none"
    unless no then
    end
    unless yes then
    end
    expect branch to be "none"
end
