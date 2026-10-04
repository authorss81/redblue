// SuiteKit - a module that exists so the Redblue test suite can exercise the
// `import` statement against a module file that parses.
//
// The VM only reads `set` statements and function *signatures* out of a module,
// so nothing here is expected to be reachable as `SuiteKit.<name>` yet. See
// phases/phase-003/FINDINGS.md.

set SUITE_KIT_NAME to "SuiteKit"

to suite_double(value)
    give back value * 2
end

to suite_triple(value)
    give back value * 3
end