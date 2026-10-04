// Redblue record tests.

test "records: a record literal keeps its fields"
    set person to {name: "Alice", age: 30}
    expect person.name to be "Alice"
    expect person.age to be 30
    expect type_of(person) to be "record"
end

test "records: an empty record has no fields"
    set blank to {}
    expect type_of(blank) to be "record"
    expect blank.missing is nothing to be yes
end

test "records: a single field record behaves like a singleton"
    set one to {only: 1}
    expect one.only to be 1
    expect type_of(one) to be "record"
end

test "records: fields can be added after construction"
    set counter to {count: 0}
    set counter.count to counter.count + 5
    expect counter.count to be 5
end

test "records: nested record access walks both levels"
    set tree to {a: {b: {c: 7}}}
    expect tree.a.b.c to be 7
    expect tree.a.b is {c: 7} to be yes
end

test "records: equality compares fields, not key order"
    set a to {x: 1, y: 2}
    set b to {y: 2, x: 1}
    expect a is b to be yes
end

test "records: different field values are different records"
    set a to {x: 1}
    set b to {x: 2}
    expect a is b to be no
end

test "records: field values keep their own types"
    set r to {n: 1, t: "two", b: yes, l: [3], nil: nothing}
    expect type_of(r.n) to be "number"
    expect type_of(r.t) to be "text"
    expect type_of(r.b) to be "yes/no"
    expect type_of(r.l) to be "list"
    expect type_of(r.nil) to be "nothing"
end

test "records: a list of records keeps element types"
    set people to [{name: "Al"}, {name: "Bo"}]
    expect length(people) to be 2
    expect people[0].name to be "Al"
    expect people[1].name to be "Bo"
end

test "records: json round trip through a record"
    set parsed to json.parse("{\"name\": \"Alice\", \"age\": 30}")
    expect parsed.name to be "Alice"
    expect parsed.age to be 30
    expect type_of(parsed) to be "record"
end

test "records: stringify renders a record as json"
    set rendered to json.stringify({b: 1})
    expect type_of(rendered) to be "text"
    expect length(rendered) to be 8
end

test "edge_records_missing_key_yields_nothing_not_an_error"
    set person to {name: "Alice"}
    expect person.age is nothing to be yes
    expect type_of(person.age) to be "nothing"
end

test "edge_records_missing_key_on_an_empty_record_yields_nothing"
    set blank to {}
    expect blank.nothing_at_all is nothing to be yes
end

test "edge_records_walking_past_a_missing_key_is_a_caught_error"
    // `blank.a` is `nothing`, and a property access on `nothing` faults.
    set caught to no
    set blank to {}
    try
        set deep to blank.a.b
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_records_a_repeated_key_keeps_the_last_value"
    set r to {a: 1, a: 2}
    expect r.a to be 2
end

test "edge_records_a_repeated_key_with_the_same_value_is_stable"
    set r to {a: 5, a: 5}
    expect r.a to be 5
    expect r is {a: 5} to be yes
end

test "edge_records_a_record_is_not_a_text"
    set r to {a: 1}
    expect r is "a" to be no
    expect type_of(r) to be "record"
end

test "edge_records_a_record_is_not_a_list"
    set r to {a: 1}
    expect r is [1] to be no
end

test "edge_records_indexing_a_record_is_a_caught_error"
    set caught to no
    set r to {a: 1}
    try
        set bad to r[0]
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_records_depth_three_nesting_resolves"
    set deep to {l1: {l2: {l3: {value: 42}}}}
    expect deep.l1.l2.l3.value to be 42
end

test "records: a single field record compared to an empty record"
    set one to {a: 1}
    set none to {}
    expect one is none to be no
    expect none is {} to be yes
end