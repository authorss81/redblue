// Redblue integration tests: whole-program behaviour that spans several areas
// of the language. Run with `rb test tests/integration_test.rb`.

test "integration: hello world variables"
    set greeting to "Hello, World!"
    expect greeting to be "Hello, World!"
    expect length(greeting) to be 13
end

test "integration: variables can be reassigned"
    set x to 10
    set y to 20
    set x to x + y
    expect x to be 30
    expect y to be 20
end

test "integration: a conditional guards an assignment"
    set passed to no
    set expected to 42
    set actual to 42
    if actual is expected then
        set passed to yes
    end
    expect passed to be yes
end

test "integration: a failed guard leaves the flag alone"
    set passed to no
    set actual to 41
    set expected to 42
    if actual is expected then
        set passed to yes
    end
    expect passed to be no
end

test "integration: lists survive assignment and index round trip"
    set items to [1, 2, 3]
    set first to items[0]
    set last to items[-1]
    expect first to be 1
    expect last to be 3
    expect items[999] is nothing to be yes
end

test "integration: a record survives assignment and field round trip"
    set config to {host: "localhost", port: 8080}
    expect config.host to be "localhost"
    expect config.port to be 8080
    set config.port to 9090
    expect config.port to be 9090
end

test "integration: a function declaration sits inside a working program"
    to double(value)
        give back value * 2
    end
    set total to 0
    for each value in [1, 2, 3]
        set total to total + value * 2
    end
    expect type_of(double) to be "function"
    expect total to be 12
end

test "integration: an object declaration sits inside a working program"
    object Session
    end
    set Session.user to "alice"
    expect Session.user to be "alice"
    expect type_of(Session) to be "record"
end

test "integration: a module import sits inside a working program"
    set ran to yes
    import SuiteKit
    set marker to 1
    expect marker to be 1
    expect ran to be yes
end

test "integration: unicode text survives every stage"
    set original to "héllo 世界 🎉"
    set copied to original
    expect copied is original to be yes
    expect length(original) to be 18
end

test "integration: text escapes survive every stage"
    set original to "line\nbreak\ttab"
    set copied to original
    expect copied is original to be yes
    expect length(original) to be 14
end

test "integration: json text becomes a record and back"
    set text to json.parse("{\"a\": 1, \"b\": \"two\"}")
    expect text.a to be 1
    expect text.b to be "two"
    set rendered to json.stringify(text)
    expect type_of(rendered) to be "text"
    expect length(rendered) to be 20
end

test "integration: arithmetic inside a loop inside a record"
    set stats to {sum: 0, squares: 0}
    for each n in [1, 2, 3, 4]
        set stats.sum to stats.sum + n
        set stats.squares to stats.squares + n * n
    end
    expect stats.sum to be 10
    expect stats.squares to be 30
end

test "integration: a long loop terminates deterministically"
    set total to 0
    set i to 0
    while i is not 1000
        set total to total + 1
        set i to i + 1
    end
    expect total to be 1000
    expect i to be 1000
end

test "integration: deeply nested access stays in range"
    set data to {rows: [{cells: [1, 2, 3]}, {cells: [4, 5, 6]}]}
    expect data.rows[0].cells[0] to be 1
    expect data.rows[1].cells[2] to be 6
    expect data.rows[9] is nothing to be yes
end

test "edge_integration_a_divisor_of_zero_stops_the_program_at_that_line"
    set finished to no
    try
        set halfway to 1 + 1
        set bad to 1 / 0
        set finished to yes
    catch error
        set finished to no
    end
    expect halfway to be 2
    expect finished to be no
end

test "edge_integration_an_empty_program_body_still_runs"
    set marker to 1
    expect marker to be 1
end

test "edge_integration_a_single_element_collection_flows_through"
    set items to [42]
    set total to 0
    for each value in items
        set total to total + value
    end
    expect length(items) to be 1
    expect total to be 42
end

test "edge_integration_a_program_that_only_fails_reports_the_failure"
    set caught to no
    try
        set bad to [1, 2] * "x"
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "integration: repeating the same computation gives the same answer"
    set first to 0
    set second to 0
    repeat 4 times
        set first to first + 6
    end
    repeat 4 times
        set second to second + 6
    end
    expect first to be second
    expect first to be 24
end