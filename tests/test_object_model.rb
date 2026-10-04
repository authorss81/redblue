// Redblue object model tests.
//
// `object Name [extends Parent]` with `has` fields and `to can` methods is
// specified in SPEC.md § Objects. The rules these tests pin:
//
// - a declaration binds its name to a record of its resolved fields, so
//   `type_of` answers "record" (see tests/test_objects.rb for the rest of what
//   that means);
// - lookup order is nearest declaration first — the child's own fields and
//   methods, then the parent's, then the grandparent's — and the first
//   declaration of a name wins, so a child field shadows rather than merges;
// - the parent must be declared first, which is what makes the parent chain
//   acyclic: `object A extends A` is reported, and re-declaring a name is
//   refused;
// - `receiver.method(args)` is a method call when the receiver names a
//   declared object, and `this` is bound to the receiver for that call.
//
// The exact error messages are pinned in tests/object_model_test.rs.

test "objects: a method sees the declaration as this"
    object Greeter
        has name default "world"
        to can greet(greeting)
            give back greeting + ", " + this.name
        end
    end
    set Greeter.name to "Ada"
    expect Greeter.greet("hello") to be "hello, Ada"
end

test "objects: a method writes to this and gives the object back"
    object Counter
        has total default 0
        to can bump()
            set this.total to this.total + 1
            give back this
        end
    end
    set bumped to Counter.bump()
    expect bumped.total to be 1
    expect Counter.total to be 0
end

test "objects: the nearest declaration of a method answers"
    object Person
        to can role()
            give back "person"
        end
    end
    object Employee extends Person
        to can role()
            give back "employee"
        end
    end
    expect Employee.role() to be "employee"
    expect Person.role() to be "person"
end

test "objects: a field declared by the child shadows the parent's field"
    object Base
        has tag default "base"
    end
    object Derived extends Base
        has tag default "derived"
        has extra default 1
    end
    expect Derived.tag to be "derived"
    expect Derived.extra to be 1
    expect Base.tag to be "base"
    expect Base.extra is nothing to be yes
end

test "objects: a three level chain inherits from the grandparent"
    object One
        has a default 1
        to can who()
            give back "one"
        end
    end
    object Two extends One
        has b default 2
    end
    object Three extends Two
        has c default 3
    end
    expect Three.a to be 1
    expect Three.b to be 2
    expect Three.c to be 3
    expect Three.who() to be "one"
end

test "edge_objects_a_field_and_a_method_of_one_name_are_separate"
    object Row
        has name default "field"
        to can name()
            give back "method"
        end
    end
    expect Row.name to be "field"
    expect Row.name() to be "method"
end

test "edge_objects_an_object_keeps_the_name_of_a_module"
    object Holder
        has json
    end
    set Holder.json to "not a module"
    set parsed to json.parse("{\"a\": 1}")
    expect parsed.a to be 1
    expect Holder.json to be "not a module"
end

test "edge_objects_a_method_the_type_does_not_have_is_reported"
    set caught to nothing
    try
        object Nothing_Here
        end
        set caught to Nothing_Here.missing()
    catch problem
        set caught to "reported"
    end
    expect caught to be "reported"
end

test "edge_objects_a_self_extending_object_is_reported"
    set caught to nothing
    try
        object Cycle extends Cycle
        end
    catch problem
        set caught to "reported"
    end
    expect caught to be "reported"
end

test "edge_objects_a_module_function_still_works_after_a_declaration"
    set rows to csv.parse("a,b\n1,2")
    expect rows[1][0] to be "1"
    expect rows[0][1] to be "b"
end
