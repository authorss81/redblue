// Redblue text tests.
//
// `length` on text counts bytes, not characters, so the unicode tests pin that
// behaviour instead of assuming character semantics.

test "text: empty text has length zero"
    set empty to ""
    expect empty to be ""
    expect length(empty) to be 0
end

test "text: a singleton text is its own length"
    set one to "a"
    expect one to be "a"
    expect length(one) to be 1
end

test "text: length counts characters for ascii"
    set greeting to "hello"
    expect length(greeting) to be 5
end

test "text: concatenation joins two texts"
    set a to "foo"
    set b to "bar"
    expect a + b to be "foobar"
end

test "text: concatenation is not associative by accident"
    set a to "x"
    set b to "yy"
    set c to "zzz"
    expect a + b + c to be "xyyzzz"
    expect length(a + b + c) to be 6
end

test "text: a well formed brace pair substitutes"
    // `{expr}` interpolation, as SPEC.md § String Interpolation specifies: the
    // braces hold an expression, and the literal is the expression's value
    // written between the characters around it.
    set name to "Alice"
    expect "Hello, {name}!" to be "Hello, Alice!"
    expect length("Hello, {name}!") to be 13
end

test "text: a brace pair around an expression is evaluated"
    // The braces hold an *expression*, not a bare name, so arithmetic inside
    // them is computed rather than spelled out.
    set x to 2
    set y to 3
    expect "Value: {x + y}" to be "Value: 5"
end

test "text: a brace pair around a call is evaluated"
    set name to "world"
    expect "shout: {uppercase(name)}" to be "shout: WORLD"
end

test "text: equality is exact, not normalised"
    set padded to "  spaced  "
    expect padded to be "  spaced  "
    expect "a" is "A" to be no
end

test "text: empty text equals empty text"
    set a to ""
    set b to ""
    expect a is b to be yes
end

test "edge_text_length_counts_bytes_not_characters"
    set accented to "héllo"
    expect length(accented) to be 6
end

test "edge_text_length_counts_bytes_for_cjk"
    set cjk to "世界"
    expect length(cjk) to be 6
end

test "edge_text_length_counts_bytes_for_emoji_outside_the_bmp"
    set party to "🎉"
    expect length(party) to be 4
end

test "edge_text_combining_mark_counts_as_its_own_byte"
    // "e" followed by U+0301 COMBINING ACUTE ACCENT is three bytes and renders
    // as one glyph, so byte length and glyph count deliberately disagree.
    set combining to "é"
    expect length(combining) to be 3
    expect combining to be "é"
end

test "edge_text_right_to_left_text_round_trips"
    set hebrew to "שלום"
    expect hebrew to be "שלום"
    expect length(hebrew) to be 8
end

test "edge_text_emoji_and_cjk_share_one_literal"
    set mixed to "hi 🎉 世界"
    expect mixed to be "hi 🎉 世界"
    expect length(mixed) to be 14
end

test "text: newline escape is one byte and one character"
    set two_lines to "a\nb"
    expect length(two_lines) to be 3
end

test "text: tab escape survives the lexer"
    set tabbed to "a\tb"
    expect length(tabbed) to be 3
end

test "text: escaped quote does not end the literal"
    set quoted to "say \"hi\""
    expect length(quoted) to be 8
end

test "text: escaped backslash is a single backslash"
    set slashed to "a\\b"
    expect length(slashed) to be 3
end

test "edge_text_a_very_long_text_keeps_its_length"
    set long to "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    expect length(long) to be 40
    expect long is long to be yes
end

test "text: length of a non text value is a caught error"
    set caught to no
    try
        set bad to length({a: 1})
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "text: length of nothing is a caught error"
    set caught to no
    try
        set bad to length(nothing)
    catch error
        set caught to yes
    end
    expect caught to be yes
end

test "text: a brace pair at the edges of a text literal substitutes"
    set word to "core"
    expect "{word}" to be "core"
    expect length("{word}") to be 4
    expect "a{word}b" to be "acoreb"
    expect length("a{word}b") to be 6
end

test "edge_text_an_unmatched_open_brace_is_an_ordinary_character"
    // A `{` with no `}` after it never closes, so there is nothing for the
    // braces to be markup around and they are the characters written.
    set x to 1
    expect "a{x" to be "a{x"
    expect length("a{x") to be 3
end

test "edge_text_a_lone_close_brace_is_an_ordinary_character"
    expect "}" to be "}"
    expect length("}") to be 1
end

test "edge_text_a_lone_open_brace_is_an_ordinary_character"
    expect "{" to be "{"
    expect length("{") to be 1
end

test "edge_text_an_empty_brace_pair_is_an_ordinary_pair_of_braces"
    // `{}` holds no expression, so it is not well formed markup.
    expect "{}" to be "{}"
    expect length("{}") to be 2
end

test "edge_text_braces_around_something_that_is_not_an_expression_survive"
    // The contents of a pair have to read as one whole expression. A JSON
    // document in a string does not, which is what keeps `json.parse` working
    // on a literal that is full of braces.
    set document to "{\"a\": 1}"
    expect document to be "{\"a\": 1}"
    expect length(document) to be 8
    set parsed to json.parse("{\"a\": 1}")
    expect parsed.a to be 1
end

test "edge_text_nested_braces_are_one_substitution"
    // The record's own braces are counted, so the outer pair closes where the
    // record ends rather than at its first `}`.
    set n to 3
    expect "r: {{n: n}}" to be "r: {n: 3}"
    expect length("r: {{n: n}}") to be 9
end

test "edge_text_a_brace_pair_inside_a_record_value_substitutes"
    set n to 7
    set r to {label: "n is {n}"}
    expect r.label to be "n is 7"
end

test "edge_text_a_brace_pair_inside_a_list_item_substitutes"
    set n to 7
    set xs to ["n is {n}", 1]
    expect xs[0] to be "n is 7"
    expect xs[1] to be 1
end

test "edge_text_a_list_a_record_and_an_object_all_substitute"
    set xs to [1, 2]
    set r to {a: 1}
    object Counter
        has total default 0
    end
    set Counter.total to 5
    expect "list: {xs}" to be "list: [1, 2]"
    expect "record: {r}" to be "record: {a: 1}"
    expect "object: {Counter.total}" to be "object: 5"
end

test "edge_text_an_unknown_name_inside_braces_is_never_printed"
    // The name inside the braces is an expression, so a name that is not bound
    // where the read happens cannot be substituted — and the literal that holds
    // it is never produced at all, because the failure comes before `printed`
    // is assigned.
    //
    // The braces are the point of this test, so the name has to be one the
    // analyzer will accept and the run cannot supply. `constant late` sits
    // below the body that reads it, which is the one place Redblue reads a name
    // the walk has not reached yet (a function body is analysed against the
    // program as it is at the call), so the analyser sees a well formed
    // `{late}` and the interpreter — running the body before that `constant`
    // line — does not. That split is what makes this observable from Redblue at
    // all: a name nothing binds anywhere is an `AnalyzerError`, which is raised
    // before the program runs and which `try` cannot catch. Which error it is,
    // and where it is reported, is pinned in tests/interpolation_test.rs.
    set printed to "untouched"
    to show()
        set printed to "value: {late}"
    end
    set caught to no
    try
        show()
    catch error
        set caught to yes
    end
    expect caught to be yes
    expect printed to be "untouched"
    constant late to "bound after the call that needed it"
end

test "edge_text_the_same_body_without_braces_is_not_a_failure"
    // The control for the test above, and the reason it is not vacuous: drop the
    // braces and the name is not read at all — it is part of the text, so the
    // literal is produced as it was written and nothing fails. That is exactly
    // the outcome a parser that printed its braces as text would give the test
    // above, which is what makes the two assertions in it worth having.
    set printed to "untouched"
    to show_plain()
        set printed to "value: late"
    end
    set caught to no
    try
        show_plain()
    catch error
        set caught to yes
    end
    expect caught to be no
    expect printed to be "value: late"
end

test "edge_text_a_substitution_whose_expression_fails_is_a_caught_error"
    // The failure comes from the expression, and `try` catches it exactly as it
    // would catch the same expression written on its own.
    set caught to no
    try
        say "half: {1 / 0}"
    catch error
        set caught to yes
    end
    expect caught to be yes
    say "still here"
end

test "edge_text_two_substitutions_in_one_literal_keep_their_order"
    set a to "one"
    set b to "two"
    expect "{a} and {b}" to be "one and two"
    expect length("{a} and {b}") to be 11
end

test "edge_text_three_substitutions_in_one_literal_keep_their_order"
    set a to "1"
    set b to "2"
    set c to "3"
    expect "{a}{b}{c}" to be "123"
    expect length("{a}{b}{c}") to be 3
end

test "edge_text_unicode_around_a_substitution_is_kept"
    set name to "世界"
    expect "héllo {name} 🎉" to be "héllo 世界 🎉"
end

test "edge_text_a_substitution_of_nothing_is_the_word_nothing"
    set missing to nothing
    expect "v: {missing}" to be "v: nothing"
end

test "edge_text_a_substitution_of_a_yesno_is_the_word_yes_or_no"
    set flag to yes
    expect "f: {flag}" to be "f: yes"
end

test "text: three-way concatenation keeps every character"
    set a to "one"
    set b to "two"
    set c to "three"
    expect a + b + c to be "onetwothree"
    expect length(a + b + c) to be 11
end