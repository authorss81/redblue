// Redblue module tests.
//
// `import Name` loads `modules/Name.rb`. A module's `to` functions are reached
// as `Name.function` and `import Name as Alias` binds `Alias` to the same
// module, so both spellings of a member call reach one function. The analyser
// does not track a module's members, so a member is reached through the module
// name rather than imported on its own.

test "modules: an existing module imports without fault"
    set ok to no
    try
        import SuiteKit
        set ok to yes
    catch error
        set ok to no
    end
    expect ok to be yes
end

test "modules: an import alias reaches the module's function"
    import MathUtils as M
    set area to M.circle_area(2)
    expect area to be 12.56636
end

test "modules: the module's own name reaches the same function as its alias"
    import MathUtils as M
    set throughAlias to M.circle_area(2)
    set throughOwnName to MathUtils.circle_area(2)
    expect throughOwnName to be throughAlias
end

test "modules: an alias of a builtin namespace reaches its function"
    import json as J
    set text to J.stringify(2)
    expect text to be "2"
end

test "edge_modules_an_alias_shadowing_a_local_is_the_local"
    set M to 5
    import MathUtils as M
    expect M to be 5
end

test "edge_modules_a_member_the_module_does_not_have_is_a_caught_error"
    set caught to no
    try
        import MathUtils as M
        say M.not_a_function(1)
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_modules_an_alias_with_an_unimported_module_is_a_caught_error"
    set caught to no
    try
        import NoSuchModuleAnywhere as N
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "modules: an import does not disturb the surrounding program"
    import SuiteKit
    set total to 0
    repeat 3 times
        set total to total + 2
    end
    expect total to be 6
end

test "modules: the same module can be imported twice"
    import SuiteKit
    import SuiteKit
    set ok to yes
    expect ok to be yes
end

test "modules: an import can appear after ordinary statements"
    set marker to 1
    import SuiteKit
    set marker to marker + 1
    expect marker to be 2
end

test "modules: an alias on an existing module imports"
    set ok to no
    try
        import SuiteKit to K
        set ok to yes
    catch error
        set ok to no
    end
    expect ok to be yes
end

test "edge_modules_an_unknown_module_is_a_caught_runtime_error"
    set caught to no
    try
        import NoSuchModuleAnywhere
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_modules_an_unknown_module_does_not_halt_the_program"
    set caught to no
    set after to 0
    try
        import StillNotAModule
    catch error
        set caught to yes
    end
    set after to 1
    expect caught to be yes
    expect after to be 1
end

test "edge_modules_an_unknown_module_with_an_alias_is_also_caught"
    set caught to no
    try
        import NopeNotHere to N
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_modules_importing_after_a_failed_import_still_works"
    set caught to no
    try
        import AbsentModule
    catch error
        set caught to yes
    end
    import SuiteKit
    expect caught to be yes
end

test "edge_modules_an_import_inside_a_loop_body_imports_once"
    set ok to no
    try
        set n to 0
        repeat 2 times
            import SuiteKit
            set n to n + 1
        end
        set ok to yes
    catch error
        set ok to no
    end
    expect ok to be yes
end
test "modules: a declaration publishes what it exports"
    module Geometry
        to area(r)
            return 3.14159 * r * r
        end
        export all
    end
    set area to Geometry.area(2)
    expect area to be 12.56636
end

test "modules: a declaration's own scope does not leak"
    module Inner
        set hidden to 42
        to read
            return hidden
        end
        export all
    end
    expect Inner.read() to be 42
    set caught to no
    try
        say hidden
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_modules_an_export_of_a_name_it_does_not_define_is_a_caught_error"
    set caught to no
    try
        module Bad
            to f
                return 1
            end
            export not_a_function
        end
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_modules_a_name_declared_twice_is_a_caught_error"
    module Twice
        to f
            return 1
        end
        export f
    end
    set caught to no
    try
        module Twice
            to f
                return 2
            end
            export f
        end
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "edge_modules_an_alias_that_is_a_constant_is_the_constant"
    constant M to 5
    import MathUtils as M
    expect M to be 5
end
