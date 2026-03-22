// Redblue Test Suite
// Run with: rb test

// ============================================
// LEXER TESTS
// ============================================

// test "Lexer: Numbers"
//     set tokens to lexer.tokenize("42")
//     tokens.length should equal 2  // token + EOF
// end

// test "Lexer: Strings"
//     set tokens to lexer.tokenize('"hello"')
// end

// test "Lexer: Keywords"
//     set tokens to lexer.tokenize("if else end")
// end

// ============================================
// PARSER TESTS
// ============================================

// test "Parser: Say statement"
//     set ast to parser.parse('say "Hello"')
// end

// test "Parser: Variable declaration"
//     set ast to parser.parse("set x to 10")
// end

// test "Parser: If statement"
//     set ast to parser.parse("if x then end")
// end

// ============================================
// VM TESTS
// ============================================

// test "VM: Say prints output"
//     set result to vm.run('say "test"')
// end

// test "VM: Variables work"
//     set result to vm.run("set x to 10")
// end

// test "VM: Math works"
//     set result to vm.run("set x to 10 + 5")
// end

// ============================================
// INTEGRATION TESTS
// ============================================

// test "Hello World"
//     run_file("examples/hello.rb")
//     output should contain "Hello, World!"
// end

// test "FizzBuzz"
//     run_file("examples/fizzbuzz.rb")
// end

// test "Objects"
//     run_file("examples/objects.rb")
// end

say "Test suite ready - run 'rb test' to execute"
