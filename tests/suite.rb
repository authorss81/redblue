// Redblue cross-area suite: programs that use lists, records, text and control
// flow together. Run the whole thing with `rb test`.

test "suite: fizzbuzz classification across fifteen numbers"
    set out to ""
    set n to 1
    repeat 15 times
        set by3 to n % 3
        set by5 to n % 5
        set label to "plain"
        if by3 is 0 then
            if by5 is 0 then
                set label to "FizzBuzz"
            else
                set label to "Fizz"
            end
        else
            if by5 is 0 then
                set label to "Buzz"
            end
        end
        set out to out + label + ","
        set n to n + 1
    end
    expect out to be "plain,plain,Fizz,plain,Buzz,Fizz,plain,plain,Fizz,Buzz,plain,Fizz,plain,plain,FizzBuzz,"
end

test "suite: fizzbuzz classifies exactly the multiples"
    set fizz to 0
    set buzz to 0
    set both to 0
    set n to 1
    repeat 100 times
        set by3 to n % 3
        set by5 to n % 5
        if by3 is 0 then
            if by5 is 0 then
                set both to both + 1
            else
                set fizz to fizz + 1
            end
        else
            if by5 is 0 then
                set buzz to buzz + 1
            end
        end
        set n to n + 1
    end
    expect fizz to be 27
    expect buzz to be 14
    expect both to be 6
end

test "suite: sum a nested list of lists"
    set total to 0
    for each row in [[1, 2], [3, 4], [5, 6]]
        for each cell in row
            set total to total + cell
        end
    end
    expect total to be 21
end

test "suite: build a record accumulator from a list"
    set tally to {total: 0, count: 0}
    for each value in [1, 2, 3, 4, 5]
        set tally.total to tally.total + value
        set tally.count to tally.count + 1
    end
    expect tally.total to be 15
    expect tally.count to be 5
end

test "suite: build a text report from records"
    set report to ""
    for each person in [{name: "Al"}, {name: "Bo"}]
        set report to report + person.name + ";"
    end
    expect report to be "Al;Bo;"
    expect length(report) to be 6
end

test "suite: a number cannot be concatenated onto text"
    // There is no wired text conversion yet, so `text + number` must fault
    // rather than silently coercing.
    set caught to no
    try
        set bad to "n=" + 1
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "suite: a list of records sorted by walking and filtering"
    set kept to 0
    set dropped to 0
    for each n in [1, 2, 3, 4, 5, 6]
        set by2 to n % 2
        if by2 is 0 then
            set kept to kept + n
        else
            set dropped to dropped + n
        end
    end
    expect kept to be 12
    expect dropped to be 9
end

test "suite: unicode survives a list round trip"
    set words to ["café", "世界", "🎉", "שלום"]
    expect length(words) to be 4
    expect words[0] to be "café"
    expect words[1] to be "世界"
    expect words[2] to be "🎉"
    expect words[3] to be "שלום"
end

test "suite: a record of every primitive type"
    set kinds to {n: 1, t: "a", b: yes, l: [1], z: nothing}
    expect type_of(kinds.n) to be "number"
    expect type_of(kinds.t) to be "text"
    expect type_of(kinds.b) to be "yes/no"
    expect type_of(kinds.l) to be "list"
    expect type_of(kinds.z) to be "nothing"
end

test "suite: json text parses into a record with the same field values"
    set source to "{\"count\": 3, \"name\": \"suite\"}"
    set parsed to json.parse(source)
    expect parsed.count to be 3
    expect parsed.name to be "suite"
    expect type_of(parsed) to be "record"
end

test "suite: csv text parses into a list of rows"
    set rows to csv.parse("a,b\n1,2")
    expect length(rows) to be 2
    expect length(rows[0]) to be 2
    expect rows[0][0] to be "a"
end

test "suite: a while loop builds a repeated text pattern"
    set out to ""
    set n to 0
    while n is not 5
        set out to out + "ab"
        set n to n + 1
    end
    expect out to be "ababababab"
    expect length(out) to be 10
end

test "edge_suite_a_runtime_error_mid_program_leaves_earlier_work_intact"
    set work to 0
    try
        repeat 5 times
            set work to work + 2
        end
        set bad to 1 / 0
    catch error
        set work to work + 100
    end
    expect work to be 110
end

test "edge_suite_two_independent_failures_are_both_reported"
    set first to no
    set second to no
    try
        set bad to 1 / 0
    catch error
        set first to yes
    end
    try
        set bad to 1 + "x"
    catch error
        set second to yes
    end
    expect first to be yes
    expect second to be yes
end

test "edge_suite_an_empty_list_and_an_empty_text_stay_distinct"
    set listy to []
    set texty to ""
    expect listy is texty to be no
    expect length(listy) to be 0
    expect length(texty) to be 0
end

test "edge_suite_deeply_nested_lists_resolve_at_every_level"
    set deep to [[[[42]]]]
    expect deep[0][0][0][0] to be 42
    expect length(deep) to be 1
    expect length(deep[0][0][0]) to be 1
end

test "edge_suite_deeply_nested_records_resolve_at_every_level"
    set deep to {a: {b: {c: {d: "leaf"}}}}
    expect deep.a.b.c.d to be "leaf"
    expect type_of(deep) to be "record"
end

test "suite: every primitive type reports its own name"
    expect type_of(1) to be "number"
    expect type_of("a") to be "text"
    expect type_of([1]) to be "list"
    expect type_of({a: 1}) to be "record"
    expect type_of(yes) to be "yes/no"
    expect type_of(nothing) to be "nothing"
end

test "suite: an empty for each leaves a record accumulator untouched"
    set tally to {total: 0}
    for each value in []
        set tally.total to tally.total + 1
    end
    expect tally.total to be 0
end