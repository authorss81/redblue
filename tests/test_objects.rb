// Redblue object tests.
//
// `object Name ... end` binds the name to a fresh record. Fields (`has`) and
// instantiation (`new`) are specified but not parsed yet (see FINDINGS.md), so
// these tests pin what a declaration actually does.

test "objects: a declaration binds the name to a record"
    object Point
    end
    expect type_of(Point) to be "record"
end

test "objects: a declaration with a body still binds a record"
    object Counter
        to bump()
            give back 1
        end
    end
    expect type_of(Counter) to be "record"
end

test "objects: two declarations bind two independent records"
    object First
    end
    object Second
    end
    set First.tag to "first"
    set Second.tag to "second"
    expect type_of(First) to be "record"
    expect type_of(Second) to be "record"
    expect First.tag is Second.tag to be no
end

test "objects: a fresh declaration starts with no fields"
    object Empty
    end
    expect Empty.anything is nothing to be yes
end

test "objects: a declared record accepts a field assignment"
    object Box
    end
    set Box.label to "crate"
    expect Box.label to be "crate"
end

test "objects: an extends declaration binds the child name"
    object Base
    end
    object Derived extends Base
    end
    expect type_of(Derived) to be "record"
    expect type_of(Base) to be "record"
end

test "objects: inheritance does not alias the two records"
    object Base
    end
    object Derived extends Base
    end
    set Base.tag to "base"
    expect Derived.tag is nothing to be yes
end

test "objects: a declared record compares structurally"
    object Same
    end
    expect Same is {} to be yes
end

test "objects: a three level chain declares"
    object One
    end
    object Two extends One
    end
    object Three extends Two
    end
    expect type_of(Three) to be "record"
end

test "objects: a declaration coexists with records"
    object Marker
    end
    set plain to {a: 1}
    expect type_of(plain) to be "record"
    expect type_of(Marker) to be "record"
end

test "objects: a declaration does not disturb a loop accumulator"
    object Loop
    end
    set total to 0
    repeat 3 times
        set total to total + 4
    end
    expect type_of(Loop) to be "record"
    expect total to be 12
end

test "edge_objects_a_declared_record_starts_empty_and_grows"
    object Grower
    end
    set before to Grower
    expect before is {} to be yes
    set Grower.a to 1
    set Grower.b to 2
    expect Grower.a to be 1
    expect Grower.b to be 2
end

test "edge_objects_a_declared_record_is_not_a_list"
    object NotAList
    end
    expect NotAList is [1] to be no
    expect type_of(NotAList) to be "record"
end

test "edge_objects_a_field_named_like_a_builtin_shadows_it"
    object Weird
    end
    set Weird.length to 3
    expect Weird.length to be 3
end

test "edge_objects_reassigning_a_declared_record_replaces_it"
    object Replaceable
    end
    set Replaceable to {replaced: yes}
    expect Replaceable.replaced to be yes
    expect type_of(Replaceable) to be "record"
end