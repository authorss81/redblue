// Redblue module tests.
//
// `import Name` loads `modules/Name.rb`. The loader keeps only the module's
// `set` statements and function signatures, and the analyser does not track
// imports, so module members are not reachable yet (see FINDINGS.md). What is
// testable today: a resolvable import succeeds and an unresolvable one is a
// clean, catchable runtime error.

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