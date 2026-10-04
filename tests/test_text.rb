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

test "text: braces are ordinary characters in a text literal"
    // `{expr}` string interpolation is specified in SPEC.md but the VM does not
    // substitute yet, so a brace pair survives verbatim. See FINDINGS.md.
    set name to "Alice"
    expect "Hello, {name}!" to be "Hello, {name}!"
    expect length("Hello, {name}!") to be 14
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

test "text: braces survive at the edges of a text literal"
    set word to "core"
    expect "{word}" to be "{word}"
    expect length("{word}") to be 6
end

test "text: three-way concatenation keeps every character"
    set a to "one"
    set b to "two"
    set c to "three"
    expect a + b + c to be "onetwothree"
    expect length(a + b + c) to be 11
end