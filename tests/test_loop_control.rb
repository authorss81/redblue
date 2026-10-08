test "loop control: break leaves a for each loop at the value that broke"
    set turns to 0
    set last to 0
    for each i in [1, 2, 3]
        if i is 2 then
            break
        end
        set turns to turns + 1
        set last to i
    end
    // The first turn ran to its end; the second began and left before the rest
    // of its body, so the third value is never reached.
    expect turns to be 1
    expect last to be 1
end

test "loop control: skip advances to the next value"
    set seen to ""
    for each i in [1, 2, 3]
        if i is 2 then
            skip
        end
        if i is 1 then
            set seen to seen + "one"
        end
        if i is 3 then
            set seen to seen + "three"
        end
    end
    expect seen to be "onethree"
end

test "loop control: break leaves a repeat loop"
    set n to 0
    repeat 5 times
        set n to n + 1
        if n is 3 then
            break
        end
    end
    expect n to be 3
end

test "loop control: skip advances a repeat loop without ending it"
    set n to 0
    repeat 3 times
        set n to n + 1
        if n is 2 then
            skip
        end
    end
    expect n to be 3
end

test "loop control: break leaves a while loop"
    set n to 0
    while n is not 9
        set n to n + 1
        if n is 4 then
            break
        end
    end
    expect n to be 4
end

test "loop control: skip advances a while loop without ending it"
    set n to 0
    set turns to 0
    while n is not 3
        set n to n + 1
        set turns to turns + 1
        skip
    end
    expect n to be 3
    expect turns to be 3
end

test "loop control: a break in a nested loop leaves only the inner one"
    set outer to 0
    set inner to ""
    for each a in [1, 2, 3]
        set outer to outer + a
        for each b in ["x", "y", "z"]
            if b is "y" then
                break
            end
            set inner to inner + b
        end
    end
    expect inner to be "xxx"
    expect outer to be 6
end

test "loop control: a skip in a nested loop advances only the inner one"
    set outer to 0
    set inner to ""
    for each a in [1, 2]
        set outer to outer + 1
        for each b in ["x", "y"]
            if b is "x" then
                skip
            end
            set inner to inner + b
        end
    end
    expect inner to be "yy"
    expect outer to be 2
end

test "loop control: a break inside a try still leaves the loop and runs the finally"
    set n to 0
    set cleaned to 0
    repeat 5 times
        try
            set n to n + 1
            if n is 2 then
                break
            end
        catch error
            set n to 99
        finally
            set cleaned to cleaned + 1
        end
    end
    expect n to be 2
    expect cleaned to be 2
end

test "loop_control_edge_break_as_the_whole_body_of_a_loop"
    set seen to 0
    for each i in [1, 2, 3]
        break
    end
    expect seen to be 0
end

test "loop_control_edge_break_as_the_last_statement_of_a_body"
    set n to 0
    repeat 3 times
        set n to n + 1
        break
    end
    expect n to be 1
end

test "loop_control_edge_break_two_blocks_deep_inside_a_conditional"
    set n to 0
    for each i in [1, 2, 3]
        if i is 3 then
            if n is 2 then
                break
            end
        end
        set n to n + 1
    end
    expect n to be 2
end

test "loop_control_edge_skip_on_the_final_iteration_is_not_an_error"
    set seen to 0
    for each i in [1, 2, 3]
        if i is 3 then
            skip
        end
        set seen to seen + i
    end
    expect seen to be 3
end

test "loop_control_edge_skip_on_every_turn_of_a_loop_runs_no_statements"
    set n to 0
    repeat 4 times
        skip
        set n to n + 100
    end
    expect n to be 0
end

test "loop_control_edge_break_outside_a_loop_is_a_caught_error"
    set caught to no
    set survived to no
    try
        break
    catch error
        set caught to yes
    end
    set survived to yes
    expect caught to be yes
    expect survived to be yes
end

test "loop_control_edge_skip_outside_a_loop_is_a_caught_error"
    set caught to no
    set survived to no
    try
        skip
    catch error
        set caught to yes
    end
    set survived to yes
    expect caught to be yes
    expect survived to be yes
end

test "loop_control_edge_a_refused_break_inside_a_loop_leaves_the_loop_running"
    // The `break` is in a function, which is not lexically inside the loop, so
    // it is refused. The loop must survive the refusal and go round again.
    to escape()
        break
    end
    set n to 0
    set caught to no
    repeat 2 times
        set n to n + 1
        try
            escape()
        catch error
            set caught to yes
        end
    end
    expect n to be 2
    expect caught to be yes
end

test "loop_control_edge_break_in_a_loop_inside_a_loop_leaves_the_inner_one_only"
    set pairs to 0
    set singles to 0
    for each a in [1, 2, 3]
        for each b in [1, 2, 3]
            if b is 2 then
                break
            end
            set pairs to pairs + 1
        end
        if a is 2 then
            break
        end
        set singles to singles + 1
    end
    expect singles to be 1
    expect pairs to be 2
end

test "loop control: a loop over an empty list is unaffected by skip and break"
    set n to 0
    for each i in []
        set n to n + 1
        skip
    end
    expect n to be 0
end

test "loop_control_edge_a_refused_skip_inside_a_loop_leaves_the_loop_running"
    // The companion of the test above for `skip`: a `skip` written in a function
    // is in no loop, so it is refused, and the loop that called the function goes
    // on to its next turn rather than being shortened by it.
    to escape()
        skip
    end
    set n to 0
    set caught to no
    repeat 2 times
        set n to n + 1
        try
            escape()
        catch error
            set caught to yes
        end
    end
    expect n to be 2
    expect caught to be yes
end

test "loop control: a break in an if branch does not run the rest of the branch"
    // The branch is a block too: a `break` in it ends the branch and the loop,
    // not only the loop. The statements after it in the same branch are part of
    // the turn the `break` stopped.
    set seen to 0
    for each i in [1, 2, 3]
        if i is 2 then
            break
            set seen to seen + 100
        end
        set seen to seen + 1
    end
    expect seen to be 1
end

test "loop control: a break in a try body does not run the rest of the try"
    // The `finally` is still owed — an abrupt exit from a protected region is
    // not a failure — but nothing else in the protected code runs after the
    // `break`, and neither does anything after the `try`.
    set n to 0
    set cleaned to 0
    repeat 3 times
        set n to n + 1
        try
            break
            set n to n + 100
        finally
            set cleaned to cleaned + 1
        end
        set n to n + 1000
    end
    expect n to be 1
    expect cleaned to be 1
end

test "loop control: a break in a conditional inside a try body ends that turn"
    set n to 0
    set reached to 0
    set cleaned to 0
    repeat 3 times
        set n to n + 1
        try
            if n is 2 then
                break
            end
            set reached to reached + 1
        finally
            set cleaned to cleaned + 1
        end
    end
    expect n to be 2
    expect reached to be 1
    expect cleaned to be 2
end

test "loop control: a break in a loop inside a try keeps that try installed"
    // The other way round from the test above, and the one a `try` is usually
    // written for: the loop is inside the protected region, so a break out of
    // the loop lands back inside it. The failure after the loop must still be
    // caught, and the `finally` is owed at the end of the region rather than at
    // the break.
    set caught to no
    set cleaned to 0
    try
        for each i in [1, 2, 3]
            if i is 2 then
                break
            end
        end
        set bad to 1 + "one"
    catch error
        set caught to yes
    finally
        set cleaned to 1
    end
    expect caught to be yes
    expect cleaned to be 1
end

test "loop control: a skip in a loop inside a try keeps that try installed"
    set caught to no
    try
        for each i in [1, 2, 3]
            if i is 2 then
                skip
            end
        end
        set bad to 1 + "one"
    catch error
        set caught to yes
    finally
        set cleaned to 1
    end
    expect caught to be yes
    expect cleaned to be 1
end

test "loop_control_edge_a_failed_turn_leaves_no_signal_for_the_next_loop"
    // The `break` raises a signal and the `finally` then fails, so the failure
    // is what leaves the loop. Nothing is left pending for the loop after it.
    set caught to no
    try
        repeat 3 times
            try
                break
            finally
                set bad to 1 + "one"
            end
        end
    catch error
        set caught to yes
    end
    set n to 0
    repeat 3 times
        set n to n + 1
    end
    expect caught to be yes
    expect n to be 3
end

test "loop_control_edge_a_failed_turn_does_not_leak_the_scope_of_its_variable"
    set i to "outer"
    try
        repeat 3 times
            for each i in [1, 2]
                try
                    break
                finally
                    set bad to 1 + "one"
                end
            end
        end
    catch error
        set caught to yes
    end
    expect caught to be yes
    expect i to be "outer"
end

test "loop control: every statement of a finally runs on the way out of a break"
    // The cleanup is owed, so it is not half a cleanup: a `finally` a `break`
    // passes through runs every statement it has.
    set cleaned to 0
    repeat 3 times
        try
            break
        finally
            set cleaned to cleaned + 1
            set cleaned to cleaned + 10
        end
    end
    expect cleaned to be 11
end

test "loop control: a break in a finally still ends the loop that is being left"
    // The `finally` is a block with a frame of its own on the bytecode VM, and a
    // `break` written directly in one names the loop that is being left: it is
    // written inside that loop, and a block does not stop a statement being
    // written inside the loop around it.
    set log to ""
    set n to 0
    repeat 3 times
        set n to n + 1
        try
            break
        finally
            set log to log + "a"
            break
            set log to log + "b"
        end
        set log to log + "after the try"
    end
    expect log to be "a"
    expect n to be 1
end

test "loop control: a break in a catch body leaves the loop the catch is written in"
    set log to ""
    set n to 0
    repeat 3 times
        set n to n + 1
        try
            set bad to 1 + "one"
        catch error
            if n is 2 then
                break
            end
            set log to log + "c"
        finally
            set log to log + "f"
        end
        set log to log + "."
    end
    // Turn one catches and appends "c", the `finally` appends "f" and the turn
    // finishes with ".". Turn two catches, breaks, and the `finally` still owed
    // appends its "f" before the loop ends — the statement after the `try` is
    // part of the turn the `break` stopped.
    expect log to be "cf.f"
    expect n to be 2
end

test "loop control: a skip in a catch body advances the loop the catch is written in"
    set seen to ""
    for each i in ["one", "two", "three"]
        try
            set bad to 1 + "one"
        catch error
            if i is "two" then
                skip
            end
            set seen to seen + i
        end
        set seen to seen + "!"
    end
    expect seen to be "one!three!"
end

test "loop control: a break in a test body leaves the loop the test is written in"
    set log to ""
    set n to 0
    repeat 3 times
        set n to n + 1
        test "a test written inside a loop"
            if n is 2 then
                break
            end
            set log to log + "t"
        end
        set log to log + "."
    end
    expect log to be "t."
    expect n to be 2
end

test "loop control: a break in an object body leaves the loop the declaration is written in"
    set log to ""
    for each x in ["a", "b"]
        set log to log + x
        object Once
            has a
            break
            set log to log + "unreachable"
        end
        set log to log + "."
    end
    expect log to be "a"
end

test "loop control: a skip in a test body advances the loop the test is written in"
    // The `skip` companion of the block above. A `test` body has a frame of its
    // own, so the loop around it is in another frame's block, and `skip` has to
    // find it the same way `break` does - by the loop the body was written in.
    set log to ""
    set n to 0
    repeat 3 times
        set n to n + 1
        test "a test written inside a loop"
            if n is 2 then
                skip
            end
            set log to log + "t"
        end
        set log to log + "."
    end
    expect log to be "t.t."
    expect n to be 3
end

test "loop control: a skip in an object body advances the loop the declaration is written in"
    // The declaration is made under a guard because a name cannot be declared
    // twice, and a `skip` carries the loop on to its next turn rather than
    // ending it: the body is entered on the first turn only, skips there, and
    // the turns after it find nothing left to declare.
    set log to ""
    set declared to no
    for each x in ["a", "b", "c"]
        if declared is no then
            set declared to yes
            object Once
                has a
                if x is "a" then
                    skip
                end
                set log to log + "o"
            end
        end
        set log to log + x
    end
    expect log to be "bc"
end

test "loop control: a break in a catch body still runs the finally it passed through"
    // The turn the `break` stopped has not ended until the `finally` has run, so
    // the loop's variable is still the turn's own value while the cleanup runs,
    // and the name outside the loop is read again after it.
    set seen to "none"
    set i to "outer"
    for each i in [1, 2]
        try
            set bad to 1 + "one"
        catch error
            break
        finally
            set seen to i
        end
    end
    expect seen to be 1
    expect i to be "outer"
end

test "loop control: a break in a catch body runs the finally of a try inside it"
    set log to ""
    set n to 0
    repeat 3 times
        set n to n + 1
        try
            set bad to 1 + "one"
        catch error
            try
                break
            finally
                set log to log + "i"
            end
            set log to log + "unreachable"
        finally
            set log to log + "o"
        end
        set log to log + "after"
    end
    expect log to be "io"
    expect n to be 1
end

test "loop control: a break in a block inside a block leaves the loop around both"
    set log to ""
    repeat 2 times
        set log to log + "t"
        test "outer"
            test "inner"
                break
                set log to log + "unreachable"
            end
            set log to log + "after the inner test"
        end
        set log to log + "."
    end
    expect log to be "t"
end

test "loop_control_edge_a_break_in_a_block_in_a_function_is_still_refused"
    // The exemption the four blocks above need: a function body is not lexically
    // inside the loop that called it, so a `break` in a block of its own is in no
    // loop at all — and the loop that called the function survives the refusal.
    to escape()
        test "a test inside a function inside a loop"
            break
        end
    end
    set caught to no
    set n to 0
    repeat 2 times
        set n to n + 1
        try
            escape()
        catch error
            set caught to yes
        end
    end
    expect n to be 2
    expect caught to be yes
end

test "loop control: a break in an unless body does not run the statements after it"
    // An `unless` body is a block, so a `break` in it ends the body as well as
    // the loop: the line after it is part of the turn the break stopped.
    set seen to ""
    for each i in ["a", "b", "c"]
        unless i is "b" then
            set seen to seen + i
            break
            set seen to seen + "leaked"
        end
        set seen to seen + "after"
    end
    expect seen to be "a"
end

test "loop control: a skip in an unless body leaves the rest of the turn out"
    set seen to ""
    for each i in ["a", "b", "c"]
        unless i is "z" then
            set seen to seen + i
            skip
            set seen to seen + "leaked"
        end
        set seen to seen + "."
    end
    expect seen to be "abc"
end

test "loop control: a break in a module body leaves the loop around the declaration"
    // A module body runs where the declaration is written, so it is inside that
    // loop exactly as an `object` body is.
    set count to 0
    set leaked to no
    for each i in [1, 2, 3]
        set count to count + 1
        module Inner
            set held to i
            break
            set leaked to yes
        end
        set leaked to yes
    end
    expect count to be 1
    expect leaked to be no
end

test "loop_control_edge_a_skip_in_a_module_body_advances_the_loop_it_is_written_in"
    set log to ""
    for each i in ["a", "b", "c"]
        set log to log + i
        module Inner
            skip
            set log to log + "leaked"
        end
        set log to log + "."
    end
    expect log to be "abc"
end

test "loop_control_edge_a_module_body_left_by_a_break_publishes_nothing"
    // An abandoned body declares nothing, so a later `import` of that name is the
    // miss an import of an unknown module is.
    set count to 0
    repeat 2 times
        set count to count + 1
        module Gone
            export value
            set value to 1
            break
        end
    end
    set imported to "yes"
    try
        import Gone
    catch error
        set imported to "no"
    end
    expect count to be 1
    expect imported to be "no"
end

test "loop_control_edge_a_break_in_a_module_body_outside_a_loop_is_refused"
    // The module-body counterpart of the function-body exemption: a body written
    // where there is no loop has no loop to leave, and the failure is catchable.
    set caught to no
    try
        module Lone
            break
        end
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "loop_control_edge_a_break_in_a_module_body_in_a_function_is_refused"
    // A function body is not lexically inside the loop that called it, and a
    // module declared inside one inherits that: the caller's loop survives.
    to declare()
        module Inner
            break
        end
    end
    set n to 0
    set caught to no
    repeat 2 times
        set n to n + 1
        try
            declare()
        catch error
            set caught to yes
        end
    end
    expect n to be 2
    expect caught to be yes
end

test "loop_control_edge_a_finally_around_a_module_declaration_runs_after_its_scope_is_gone"
    // The signal passes out through one block at a time, so the module has given
    // its scope back before the `finally` written around it runs: the `set` here
    // is a name of the program, not one of the module the exit just left.
    set log to ""
    repeat 2 times
        set log to log + "t"
        try
            module Inner
                set held to 1
                break
            end
            set log to log + "leaked"
        catch error
            set log to log + "c"
        finally
            set log to log + "f"
        end
        set log to log + "."
    end
    expect log to be "tf"
end


test "loop control: a break ends a range loop at the value that broke"
    // `for each i from a to b [by s]` is in the grammar and in the parser, so this
    // is the loop form a program writes rather than one built by hand.
    set total to 0
    set turns to 0
    for each i from 1 to 5
        if i is 3 then
            break
        end
        set total to total + i
        set turns to turns + 1
    end
    // The turn that broke the loop does not reach the statements after the
    // `break`, so 3 is neither added nor counted.
    expect total to be 3
    expect turns to be 2
end

test "loop control: a skip advances a range loop to its next value"
    set total to 0
    set turns to 0
    for each i from 0 to 6 by 2
        if i is 2 then
            skip
        end
        set total to total + i
        set turns to turns + 1
    end
    // 0 is a value the stride visits and is not skipped, 2 is, and 4 and 6 run.
    expect total to be 10
    expect turns to be 3
end

test "loop_control_edge_a_break_in_a_nested_range_loop_leaves_only_the_inner_one"
    set outer to 0
    set inner to 0
    for each i from 1 to 3
        for each j from 1 to 3
            if j is 2 then
                break
            end
            set inner to inner + 1
        end
        set outer to outer + 1
    end
    // The inner loop broke on its second value, so each of the three outer turns
    // counted exactly one inner turn, and the outer loop still reached its last.
    expect outer to be 3
    expect inner to be 3
end

test "loop_control_edge_a_range_loop_that_runs_no_turns_is_unaffected_by_both"
    set turned to 0
    for each i from 5 to 1
        set turned to turned + 1
        break
    end
    for each j from 1 to 3
        if j is 2 then
            skip
        end
        set turned to turned + 1
    end
    expect turned to be 2
end
