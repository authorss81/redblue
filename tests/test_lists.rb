// Redblue list tests.

test "lists: an empty list has length zero"
    set empty to []
    expect length(empty) to be 0
    expect type_of(empty) to be "list"
end

test "lists: a singleton list keeps its one element"
    set one to [7]
    expect length(one) to be 1
    expect one[0] to be 7
end

test "lists: first and last element by index"
    set items to [10, 20, 30]
    expect items[0] to be 10
    expect items[2] to be 30
    expect length(items) to be 3
end

test "lists: a negative index counts from the end"
    set items to [10, 20, 30]
    expect items[-1] to be 30
    expect items[-3] to be 10
end

test "lists: index zero of a singleton is its only element"
    set items to [5]
    expect items[0] to be 5
    expect items[-1] to be 5
end

test "lists: equality compares element by element"
    set a to [1, 2, 3]
    set b to [1, 2, 3]
    set c to [1, 2, 4]
    expect a is b to be yes
    expect a is c to be no
end

test "lists: heterogeneous elements keep their own types"
    set mixed to [1, "two", yes, nothing]
    expect length(mixed) to be 4
    expect type_of(mixed[0]) to be "number"
    expect type_of(mixed[1]) to be "text"
    expect type_of(mixed[2]) to be "yes/no"
    expect type_of(mixed[3]) to be "nothing"
end

test "lists: an empty list is not equal to a one element list"
    set a to []
    set b to [nothing]
    expect a is b to be no
    expect length(a) to be 0
    expect length(b) to be 1
end

test "lists: for each accumulates every element"
    set total to 0
    for each value in [1, 2, 3, 4]
        set total to total + value
    end
    expect total to be 10
end

test "lists: for each over an empty list leaves the accumulator alone"
    set total to 0
    for each value in []
        set total to total + 1
    end
    expect total to be 0
end

test "lists: nested for each walks every cell"
    set total to 0
    for each row in [[1, 2], [3, 4]]
        for each cell in row
            set total to total + cell
        end
    end
    expect total to be 10
end

test "lists: three levels of nesting resolve"
    set deep to [[[1, 2], [3]], [[4]]]
    expect deep[0][0][1] to be 2
    expect deep[0][1][0] to be 3
    expect deep[1][0][0] to be 4
end

test "lists: a conditional inside a loop does not disturb the iteration"
    // `break` and `skip` are parsed but no loop honours them yet (see
    // FINDINGS.md), so this pins the iteration count itself.
    set total to 0
    set seen to 0
    for each value in [1, 2, 3, 4]
        if value is 2 then
            skip
        end
        set seen to seen + 1
        set total to total + value
    end
    expect seen to be 4
    expect total to be 10
end

test "lists: break inside a loop does not truncate it"
    // Companion to the test above: `break` currently runs to completion.
    set total to 0
    for each value in [1, 2, 3, 4]
        if value is 3 then
            break
        end
        set total to total + value
    end
    expect total to be 10
end

test "edge_lists_index_past_the_end_yields_nothing_not_an_error"
    set items to [1, 2, 3]
    expect items[999] is nothing to be yes
    expect type_of(items[999]) to be "nothing"
end

test "edge_lists_index_into_an_empty_list_yields_nothing"
    set items to []
    expect items[0] is nothing to be yes
    expect items[999] is nothing to be yes
end

test "edge_lists_index_negative_past_the_start_yields_nothing"
    set items to [1, 2, 3]
    expect items[-99] is nothing to be yes
end

test "edge_lists_indexing_through_a_number_is_a_caught_error"
    set caught to no
    try
        set bad to [1][0][0]
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_lists_a_record_is_not_a_list"
    set r to {a: 1}
    expect type_of(r) to be "record"
    expect r is [1] to be no
end

test "edge_lists_length_of_nothing_is_a_caught_error"
    set caught to no
    try
        set bad to length(nothing)
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "lists: a list of one text element"
    set words to ["solo"]
    expect length(words) to be 1
    expect words[0] to be "solo"
    expect words[0] is words[0] to be yes
end