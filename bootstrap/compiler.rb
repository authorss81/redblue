// bootstrap/compiler.rb — bootstrap stage S2.
//
// `rb compile` is stage 1: `src/lexer.rs` -> `src/parser.rs` ->
// `src/bytecode/codegen.rs` -> `Chunk::encode`. This file is stage 2: the same
// four stages, written in Redblue, so that a Redblue program can produce a
// `.rbc`.
//
//     rb run bootstrap/compiler.rb <input.rb> <output.rbc>
//
// The only claim this file may make is byte-for-byte equality with stage 1 for
// every program both accept. `tests/bootstrap_selfhost_test.rs` is what checks
// it, against `compile_source(source).encode()` and nothing else — so a stage 2
// that compiles *something* without compiling the *same* something fails.
//
// Three rules make the equality hold, and each is a difference between the two
// implementations rather than a convenience:
//
//   1. The source is read as **bytes**, not as characters. The lexer walks a
//      `Vec<char>` in stage 1 and a list of byte values here, and what the
//      format stores for a text constant is its UTF-8 bytes — so a stage 2
//      that counted characters would write different length fields for every
//      non-ASCII string.
//   2. A name is interned and a string literal is not. `Compiler::text` in
//      `src/bytecode/codegen.rs` looks a name up before appending it, while
//      `Expr::Text` appends every occurrence. The pool is in order of first
//      use, so the two rules have to be the same two rules.
//   3. A nested block's body is compiled **before** the instruction naming it,
//      because that is when its constants enter the pool.
//
// Nothing here reads a clock, a hash-map order or a path: the same source gives
// the same bytes on every run.
//
// One property of Redblue shapes every function below, and is why they are
// written the way they are: `give back` inside an `if` or `while` body returns
// from that body, not from the function. So every function computes into a
// variable and returns once, at its own level.

// ---------------------------------------------------------------------------
// The reserved operands of `src/bytecode/format.rs`. `NO_CONST` and `NO_BLOCK`
// are the maximum `u32`, which a constant pool can never reach, so they can
// never be confused with an index; `STATEMENT_MARKER` is one below it.
// ---------------------------------------------------------------------------

constant NO_CONST to 4294967295
constant STATEMENT_MARKER to 4294967294
constant END_TRY_MARKER to 4294967295

// ---------------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------------

to main()
    set main_args to 0
    set main_args to sys.argv()
    if length(main_args) is not 2 then
        refuse(0, "usage: rb run bootstrap/compiler.rb <input.rb> <output.rbc>")
    end

    set main_source to read_source(main_args[0])
    set main_tokens to lex(main_source)
    set main_program to parse_program(main_tokens)
    set main_chunk to compile_program(main_program)
    bytes.write(main_args[1], encode(main_chunk))
    give back 0
end

// The file to compile, as bytes. Read as text and converted, so a file that is
// not UTF-8 is refused here rather than half-lexed — the same refusal the Rust
// frontend makes when it reads a source file.
to read_source(path)
    set read_source_ok to 0
    set read_source_text to 0
    set read_source_ok to yes
    set read_source_text to ""
    try
        set read_source_text to files.read(path)
    catch failure
        set read_source_ok to no
    end
    if read_source_ok is no then
        refuse(0, "cannot read " + path)
    end
    give back bytes.from_text(read_source_text)
end

// ---------------------------------------------------------------------------
// Failure
// ---------------------------------------------------------------------------

// `rb run` reports a runtime error with a non-zero status, and Redblue has no
// `exit`, so this is how the compiler stops: the message goes to stderr first,
// then the out-of-range read raises and ends the program. Without the raise the
// compiler would carry on and write a file nobody asked for, which is the one
// outcome worse than a crash.
to refuse(line, message)
    set refuse_stop_here to 0
    console.error("bootstrap/compiler.rb: " + to_text(line) + ": " + message)
    set refuse_stop_here to [0]
    say refuse_stop_here[1]
end

// ---------------------------------------------------------------------------
// Numbers as bytes
//
// A `Constant::Number` is eight little-endian bytes of the `f64` the frontend
// produced, and a Redblue number *is* an `f64`. So the compiler's one hard
// numeric problem is reading the bits of a number it already holds, with no
// `to_bits` to call.
//
// The split used here is the one that keeps every intermediate value exactly
// representable, because an `f64` has 53 bits of significand and the pattern
// has 64:
//
//     bits  0..32 = the low half of the mantissa fraction
//     bits 32..64 = sign | biased exponent | the high half of the fraction
//
// Both halves are then below 2^32, so both are exact in an `f64`, and each byte
// is a `% 256` away. The exponent comes from scaling the magnitude by powers of
// two, which is exact at every magnitude, subnormals included.
// ---------------------------------------------------------------------------

// `2^k`, by squaring: exact for every `k` a finite number can need.
to pow2(k)
    if k < 0 then
        refuse(0, "pow2 was asked for a negative power")
    end
    set pow2_result to 1
    set pow2_base to 2
    set pow2_rest to k
    while pow2_rest > 0
        if pow2_rest mod 2 is 1 then
            set pow2_result to pow2_result * pow2_base
        end
        set pow2_rest to (pow2_rest - pow2_rest mod 2) / 2
        // Squared only while another bit is still wanted: one more square than
        // that is an infinity, and an infinity in a constant pool is a
        // program the frontend refuses to compile.
        if pow2_rest > 0 then
            set pow2_base to pow2_base * pow2_base
        end
    end
    give back pow2_result
end

// `2^-k`, by halving: exact down to the smallest subnormal.
to inv_pow2(k)
    set inv_pow2_result to 0
    set inv_pow2_i to 0
    set inv_pow2_result to 1
    set inv_pow2_i to 0
    while inv_pow2_i < k
        set inv_pow2_result to inv_pow2_result / 2
        set inv_pow2_i to inv_pow2_i + 1
    end
    give back inv_pow2_result
end

// `floor(log2(mag))` for a finite `mag > 0`, by scaling until it is in [1, 2).
to exponent_of(mag)
    set exponent_of_e to 0
    set exponent_of_m to 0
    set exponent_of_e to 0
    set exponent_of_m to mag
    while exponent_of_m >= 2
        set exponent_of_m to exponent_of_m / 2
        set exponent_of_e to exponent_of_e + 1
    end
    while exponent_of_m < 1
        set exponent_of_m to exponent_of_m * 2
        set exponent_of_e to exponent_of_e - 1
    end
    give back exponent_of_e
end

// The eight bytes of `n`, little-endian.
to number_bytes(n)
    set number_bytes_scaled to 0
    if n is not n then
        refuse(0, "a NaN cannot be a bytecode constant")
    end
    set number_bytes_raw to []
    set number_bytes_sign to 0
    set number_bytes_mag to n
    if n < 0 then
        set number_bytes_sign to 1
        set number_bytes_mag to 0 - n
    end

    // Zero. The lexer never puts a `-` in a numeric literal, so a constant can
    // only be `+0.0`, and `+0.0` is eight zero bytes.
    if number_bytes_mag is 0 then
        set number_bytes_raw to [0, 0, 0, 0, 0, 0, 0, 0]
    else
        set number_bytes_e to exponent_of(number_bytes_mag)
        set number_bytes_biased to 0
        set number_bytes_fraction to 0
        if number_bytes_mag < inv_pow2(1022) then
            // Subnormal: the exponent field is zero and the whole mantissa is
            // the number_bytes_fraction, at a scale of 2^-1074. `number_bytes_mag / 2^-1074` is an exact
            // integer below 2^52, so the division rounds to exactly it.
            set number_bytes_fraction to number_bytes_mag / inv_pow2(1074)
        else
            set number_bytes_biased to number_bytes_e + 1023
            // The number_bytes_fraction is `number_bytes_mag` number_bytes_scaled into [1, 2) with the leading bit
            // taken off. Doubling is exact and needs no power of two the `f64`
            // cannot hold, which matters at the bottom of the range: the
            // smallest normal needs a scale of 2^1074, and 2^1074 is an
            // infinity.
            set number_bytes_scaled to number_bytes_mag
            while number_bytes_scaled >= pow2(53)
                set number_bytes_scaled to number_bytes_scaled / 2
            end
            while number_bytes_scaled < pow2(52)
                set number_bytes_scaled to number_bytes_scaled * 2
            end
            set number_bytes_fraction to number_bytes_scaled - pow2(52)
        end

        // The number_bytes_fraction's number_bytes_high half and its number_bytes_low half. The number_bytes_low half is below 2^32
        // and the number_bytes_high half is `number_bytes_sign | number_bytes_biased exponent | number_bytes_high half`, below 2^32
        // too, so both are exact here and both are `% 256` away from bytes.
        set number_bytes_low to number_bytes_fraction mod pow2(32)
        set number_bytes_mant_high to (number_bytes_fraction - number_bytes_low) / pow2(32)
        set number_bytes_high to number_bytes_biased * pow2(20) + number_bytes_mant_high
        if number_bytes_sign is 1 then
            set number_bytes_high to number_bytes_high + pow2(31)
        end
        set number_bytes_raw to cat(u32_le(number_bytes_low), u32_le(number_bytes_high))
    end
    give back number_bytes_raw
end

// ---------------------------------------------------------------------------
// The encoding
// ---------------------------------------------------------------------------

// Two byte lists joined. Redblue's `+` adds numbers and joins texts — it does
// not concatenate lists — so the one operation the encoder cannot do without is
// written here, once, in terms of `push`.
to cat(first, second)
    set cat_out to 0
    set cat_out to first
    for each each_byte in second
        set cat_out to push(cat_out, each_byte)
    end
    give back cat_out
end

// Four little-endian bytes of `v`, for `0 <= v < 2^32`. Each step divides by
// 256, which is exact, so every byte is the remainder of an exact integer.
to u32_le(v)
    set u32_le_b0 to 0
    set u32_le_rest to 0
    set u32_le_b1 to 0
    set u32_le_b2 to 0
    set u32_le_b3 to 0
    set u32_le_b0 to v mod 256
    set u32_le_rest to (v - u32_le_b0) / 256
    set u32_le_b1 to u32_le_rest mod 256
    set u32_le_rest to (u32_le_rest - u32_le_b1) / 256
    set u32_le_b2 to u32_le_rest mod 256
    set u32_le_rest to (u32_le_rest - u32_le_b2) / 256
    set u32_le_b3 to u32_le_rest
    give back [u32_le_b0, u32_le_b1, u32_le_b2, u32_le_b3]
end

// A `.rbc`: the magic, the format version, the constant pool, then the block
// tree in the order a depth-first walk visits it. `Chunk::encode` in
// `src/bytecode/format.rs` is the specification of that layout.
to encode(chunk)
    set encode_out to 0
    set encode_out to [82, 69, 68, 26]
    set encode_out to cat(encode_out, [5, 0])
    set encode_out to cat(encode_out, u32_le(length(chunk.p)))
    for each each_entry in chunk.p
        set encode_out to cat(encode_out, [each_entry.t])
        // A text constant is the one each_entry with a length in front of it.
        if each_entry.t is 3 then
            set encode_out to cat(encode_out, u32_le(length(each_entry.b)))
        end
        set encode_out to cat(encode_out, each_entry.b)
    end
    give back encode_block(encode_out, chunk.main)
end

// One block record, then its children: preorder, which is what
// `write_block_tree` writes — it pops `main` first and pushes each block's
// children in reverse, so they come out first-to-last.
to encode_block(out, block)
    set out to cat(out, [block.k])
    set out to cat(out, u32_le(block.a))
    set out to cat(out, u32_le(length(block.ps)))
    for each each_param in block.ps
        set out to cat(out, u32_le(length(each_param)))
        set out to cat(out, each_param)
    end
    set out to cat(out, u32_le(length(block.n)))
    set out to cat(out, block.n)
    set out to cat(out, u32_le(length(block.code)))
    for each each_instruction in block.code
        set out to cat(out, [each_instruction.o])
        set out to cat(out, u32_le(each_instruction.a))
        set out to cat(out, u32_le(each_instruction.x))
        set out to cat(out, u32_le(each_instruction.l))
    end
    set out to cat(out, u32_le(length(block.b)))
    for each each_child in block.b
        set out to encode_block(out, each_child)
    end
    give back out
end

// ---------------------------------------------------------------------------
// Lexer
//
// `Lexer::tokenize` in `src/lexer.rs` is the specification: the same character
// classes, the same escapes, the same single `Newline` token for a CRLF pair.
// ---------------------------------------------------------------------------

to is_digit(c)
    give back c >= 48 and c <= 57
end

// A byte that can begin an identifier. Stage 1 asks `char::is_alphabetic`,
// which every non-ASCII letter satisfies too, and a non-ASCII character is its
// own UTF-8 bytes — so consuming every byte above 127 is consuming the
// characters, one identifier either way.
to is_alpha(c)
    give back (c >= 65 and c <= 90) or (c >= 97 and c <= 122) or c is 95 or c >= 128
end

to is_alnum(c)
    give back is_digit(c) or is_alpha(c)
end

// The whitespace stage 1 skips: everything `char::is_whitespace` calls
// whitespace except the two characters that end a line.
to is_space(c)
    give back c is 32 or c is 9 or c is 11 or c is 12
end

// `src[i]`, or -1 past the end. The lexer looks one and two characters ahead on
// every operator, and a source that ends with one is a source, not a crash.
to byte_at(src, i)
    set byte_at_value to 0
    set byte_at_value to -1
    if i >= 0 then
        if i < length(src) then
            set byte_at_value to src[i]
        end
    end
    give back byte_at_value
end

// How many bytes the UTF-8 character beginning with `b` occupies: 1 for ASCII,
// then 2, 3 or 4 from the leading byte. A byte that cannot begin a character is
// 1, so a malformed file is copied rather than guessed at.
to char_width(b)
    set char_width_width to 0
    set char_width_width to 1
    if b >= 192 then
        set char_width_width to 2
        if b >= 224 then
            set char_width_width to 3
            if b >= 240 then
                set char_width_width to 4
            end
        end
    end
    give back char_width_width
end

to lex(src)
    set lex_tokens to 0
    set lex_pos to 0
    set lex_line to 0
    set lex_c to 0
    set lex_r to 0
    set lex_tokens to []
    set lex_pos to 0
    set lex_line to 1
    while lex_pos < length(src)
        set lex_c to src[lex_pos]
        if lex_c is 239 and byte_at(src, lex_pos + 1) is 187 and byte_at(src, lex_pos + 2) is 191 then
            // A byte-order mark is a file artefact, not a source character:
            // some editors write it at the start of a file and some on every
            // lex_line. Stage 1 skips it wherever it finds it.
            set lex_pos to lex_pos + 3
        else
            set lex_r to lex_one(src, lex_pos, lex_line)
            set lex_pos to lex_r[1]
            set lex_line to lex_line + lex_r[2]
            if lex_r[0] is not "skip" then
                set lex_tokens to push(lex_tokens, lex_r[0])
            end
        end
    end
    give back push(lex_tokens, {k: "eof", l: lex_line})
end

// One step of the loop: returns `[token, next position, newlines]`, or
// `["skip", next position, 0]` for whitespace and for a comment.
to lex_one(src, pos, line)
    set lex_one_c to 0
    set lex_one_result to 0
    set lex_one_c to src[pos]
    set lex_one_result to nothing
    if is_space(lex_one_c) then
        set lex_one_result to ["skip", pos + 1, 0]
    end
    if type_of(lex_one_result) is "nothing" then
        if lex_one_c is 10 or lex_one_c is 13 then
            set lex_one_p to pos + 1
            if lex_one_c is 13 and byte_at(src, pos + 1) is 10 then
                set lex_one_p to pos + 2
            end
            set lex_one_result to [{k: "nl", l: line}, lex_one_p, 1]
        end
    end
    if type_of(lex_one_result) is "nothing" then
        if lex_one_c is 47 and byte_at(src, pos + 1) is 47 then
            set lex_one_p to pos
            set lex_one_at_end to no
            while lex_one_at_end is no
                if lex_one_p >= length(src) then
                    set lex_one_at_end to yes
                else
                    if src[lex_one_p] is 10 then
                        set lex_one_at_end to yes
                    else
                        if src[lex_one_p] is 13 then
                            set lex_one_at_end to yes
                        else
                            set lex_one_p to lex_one_p + 1
                        end
                    end
                end
            end
            set lex_one_result to ["skip", lex_one_p, 0]
        end
    end
    if type_of(lex_one_result) is "nothing" then
        if is_digit(lex_one_c) or (lex_one_c is 46 and is_digit(byte_at(src, pos + 1))) then
            set lex_one_result to lex_number(src, pos, line)
        end
    end
    if type_of(lex_one_result) is "nothing" then
        if lex_one_c is 34 then
            set lex_one_result to lex_text(src, pos, line)
        end
    end
    if type_of(lex_one_result) is "nothing" then
        if is_alpha(lex_one_c) then
            set lex_one_result to lex_word(src, pos, line)
        end
    end
    if type_of(lex_one_result) is "nothing" then
        set lex_one_result to lex_operator(src, pos, line)
    end
    give back lex_one_result
end

// Digits, at most one `.`, then at most one exponent — exactly the shape
// `Lexer::read_number` accepts, with the same rule that a `+` or `-` is the
// operator that follows the number rather than part of it, so `5-2` is five
// minus two.
to lex_number(src, pos, line)
    set lex_number_text to 0
    set lex_number_has_exponent to 0
    set lex_number_after_exponent to 0
    set lex_number_p to 0
    set lex_number_stop to 0
    set lex_number_c to 0
    set lex_number_take to 0
    set lex_number_text to []
    set lex_number_has_exponent to no
    set lex_number_after_exponent to no
    set lex_number_p to pos
    set lex_number_stop to no
    while lex_number_stop is no
        if lex_number_p >= length(src) then
            set lex_number_stop to yes
        else
            set lex_number_c to src[lex_number_p]
            set lex_number_take to no
            if is_digit(lex_number_c) or lex_number_c is 46 then
                set lex_number_take to yes
                set lex_number_after_exponent to no
            else
                if (lex_number_c is 101 or lex_number_c is 69) and lex_number_has_exponent is no then
                    set lex_number_take to yes
                    set lex_number_has_exponent to yes
                    set lex_number_after_exponent to yes
                else
                    if lex_number_after_exponent is yes and (lex_number_c is 43 or lex_number_c is 45) then
                        set lex_number_take to yes
                        set lex_number_after_exponent to no
                    end
                end
            end
            if lex_number_take is yes then
                set lex_number_text to push(lex_number_text, lex_number_c)
                set lex_number_p to lex_number_p + 1
            else
                set lex_number_stop to yes
            end
        end
    end

    set lex_number_literal to bytes.text(lex_number_text)
    set lex_number_ok to yes
    set lex_number_value to 0
    try
        set lex_number_value to to_number(lex_number_literal)
    catch failure
        set lex_number_ok to no
    end
    if lex_number_ok is no then
        refuse(line, "invalid number '" + lex_number_literal + "'")
    end
    give back [{k: "num", l: line, v: lex_number_value}, lex_number_p, 0]
end

// The body of a text literal, as bytes: the escapes `Lexer::read_text`
// translates, and every other byte as itself.
to lex_text(src, pos, line)
    set lex_text_out to 0
    set lex_text_p to 0
    set lex_text_lines to 0
    set lex_text_stop to 0
    set lex_text_out to []
    set lex_text_p to pos + 1
    set lex_text_lines to 0
    set lex_text_stop to no
    while lex_text_stop is no
        if lex_text_p >= length(src) then
            refuse(line, "unterminated string")
        end
        set lex_text_c to src[lex_text_p]
        if lex_text_c is 34 then
            set lex_text_p to lex_text_p + 1
            set lex_text_stop to yes
        else
            if lex_text_c is 92 then
                set lex_text_p to lex_text_p + 1
                if lex_text_p >= length(src) then
                    refuse(line, "unterminated string")
                end
                set lex_text_e to src[lex_text_p]
                if lex_text_e is 110 then
                    set lex_text_out to push(lex_text_out, 10)
                    set lex_text_p to lex_text_p + 1
                else
                    if lex_text_e is 116 then
                        set lex_text_out to push(lex_text_out, 9)
                        set lex_text_p to lex_text_p + 1
                    else
                        if lex_text_e is 114 then
                            set lex_text_out to push(lex_text_out, 13)
                            set lex_text_p to lex_text_p + 1
                        else
                            if lex_text_e is 92 or lex_text_e is 34 then
                                set lex_text_out to push(lex_text_out, lex_text_e)
                                set lex_text_p to lex_text_p + 1
                            else
                                // Any other escape stands for the character
                                // itself, as stage 1 does — and a non-ASCII one
                                // is every byte of that character, not just
                                // the first.
                                set lex_text_width to char_width(lex_text_e)
                                set lex_text_i to 0
                                while lex_text_i < lex_text_width
                                    set lex_text_out to push(lex_text_out, byte_at(src, lex_text_p))
                                    set lex_text_p to lex_text_p + 1
                                    set lex_text_i to lex_text_i + 1
                                end
                            end
                        end
                    end
                end
            else
                set lex_text_out to push(lex_text_out, lex_text_c)
                if lex_text_c is 10 then
                    set lex_text_lines to lex_text_lines + 1
                end
                set lex_text_p to lex_text_p + 1
            end
        end
    end
    give back [{k: "text", l: line, b: lex_text_out}, lex_text_p, lex_text_lines]
end

// An identifier or a keyword. The bytes are kept beside the kind, because the
// kind alone is not enough: the constant pool holds names as bytes.
to lex_word(src, pos, line)
    set lex_word_p to 0
    set lex_word_p to pos
    set lex_word_at_end to no
    while lex_word_at_end is no
        if lex_word_p >= length(src) then
            set lex_word_at_end to yes
        else
            if is_alnum(src[lex_word_p]) then
                set lex_word_p to lex_word_p + 1
            else
                set lex_word_at_end to yes
            end
        end
    end
    set lex_word_raw to []
    set lex_word_i to pos
    while lex_word_i < lex_word_p
        set lex_word_raw to push(lex_word_raw, src[lex_word_i])
        set lex_word_i to lex_word_i + 1
    end
    give back [{k: keyword_of(bytes.text(lex_word_raw)), l: line, b: lex_word_raw}, lex_word_p, 0]
end

// The keyword table of `src/lexer.rs`, in the same order, as two parallel
// lists: a word's answer is its index, and one linear scan finds it.
to keyword_of(word)
    set keyword_of_words to 0
    set keyword_of_kinds to 0
    set keyword_of_kind to 0
    set keyword_of_i to 0
    set keyword_of_words to ["set", "constant", "to", "is", "are", "if", "then", "else", "end", "when", "unless", "for", "each", "in", "from", "times", "while", "repeat", "until", "break", "skip", "return", "give", "back", "might", "fail", "say", "print", "ask", "try", "catch", "finally", "and", "or", "not", "mod", "yes", "no", "nothing", "module", "import", "export", "as", "test", "expect", "object", "has", "can", "this", "that", "new", "extends", "async", "wait", "parallel", "stop"]
    set keyword_of_kinds to ["set", "constant", "to", "is", "are", "if", "then", "else", "end", "when", "unless", "for", "each", "in", "from", "times", "while", "repeat", "until", "break", "skip", "return", "giveback", "giveback", "mightfail", "mightfail", "say", "print", "ask", "try", "catch", "finally", "and", "or", "not", "mod", "yes", "no", "nothing", "module", "import", "export", "as", "test", "expect", "object", "has", "can", "this", "that", "new", "extends", "async", "wait", "parallel", "stop"]
    set keyword_of_kind to "id"
    set keyword_of_i to 0
    while keyword_of_i < length(keyword_of_words)
        if keyword_of_words[keyword_of_i] is word then
            set keyword_of_kind to keyword_of_kinds[keyword_of_i]
        end
        set keyword_of_i to keyword_of_i + 1
    end
    give back keyword_of_kind
end

// The operators, longest match first: `<=`, `>=`, `==`, `!=` and a bare `!`,
// which stage 1 reads as a prefix for the parser to bind.
to lex_operator(src, pos, line)
    set lex_operator_codes to 0
    set lex_operator_kinds to 0
    set lex_operator_c to 0
    set lex_operator_next to 0
    set lex_operator_kind to 0
    set lex_operator_p to 0
    set lex_operator_i to 0
    // `%` is the same token as the `mod` keyword in stage 1 — `Lexer::tokenize`
    // reads it as `TokenKind::Mod` — so the parser sees one operator, not two.
    set lex_operator_codes to [43, 45, 42, 47, 37, 40, 41, 91, 93, 123, 125, 44, 46, 58, 60, 62, 61, 33]
    set lex_operator_kinds to ["plus", "minus", "star", "slash", "mod", "lparen", "rparen", "lbrack", "rbrack", "lbrace", "rbrace", "comma", "dot", "colon", "lt", "gt", "eq", "not"]
    set lex_operator_c to src[pos]
    set lex_operator_next to byte_at(src, pos + 1)
    set lex_operator_kind to "unknown"
    set lex_operator_p to pos + 1

    set lex_operator_i to 0
    while lex_operator_i < length(lex_operator_codes)
        if lex_operator_codes[lex_operator_i] is lex_operator_c then
            set lex_operator_kind to lex_operator_kinds[lex_operator_i]
        end
        set lex_operator_i to lex_operator_i + 1
    end

    if lex_operator_kind is "lt" and lex_operator_next is 61 then
        set lex_operator_kind to "le"
        set lex_operator_p to pos + 2
    end
    if lex_operator_kind is "gt" and lex_operator_next is 61 then
        set lex_operator_kind to "ge"
        set lex_operator_p to pos + 2
    end
    if lex_operator_kind is "eq" and lex_operator_next is 61 then
        set lex_operator_p to pos + 2
    end
    if lex_operator_kind is "not" and lex_operator_next is 61 then
        set lex_operator_kind to "ne"
        set lex_operator_p to pos + 2
    end
    if lex_operator_kind is "unknown" then
        refuse(line, "unexpected character " + to_text(lex_operator_c))
    end
    give back [{k: lex_operator_kind, l: line}, lex_operator_p, 0]
end

// ---------------------------------------------------------------------------
// Parser
//
// `src/parser.rs` is the specification: the same precedence chain, the same
// statement dispatch, the same blocks. Every function returns
// `[node, next position]`.
// ---------------------------------------------------------------------------

to token_kind(tokens, pos)
    set token_kind_kind to 0
    set token_kind_kind to "eof"
    if pos >= 0 then
        if pos < length(tokens) then
            set token_kind_kind to tokens[pos].k
        end
    end
    give back token_kind_kind
end

to token_line(tokens, pos)
    set token_line_line to 0
    set token_line_line to 0
    if pos >= 0 then
        if pos < length(tokens) then
            set token_line_line to tokens[pos].l
        end
    end
    give back token_line_line
end

// The word at `pos`, or `""` when the token is not an identifier. The comparison
// operators are words to the lexer, so the parser matches them by name exactly
// as stage 1 does.
to word_at(tokens, pos)
    set word_at_word to 0
    set word_at_word to ""
    if token_kind(tokens, pos) is "id" then
        set word_at_word to bytes.text(tokens[pos].b)
    end
    give back word_at_word
end

to expect_kind(tokens, pos, kind)
    if token_kind(tokens, pos) is not kind then
        refuse(token_line(tokens, pos), "expected " + kind + " but got " + token_kind(tokens, pos))
    end
    give back pos + 1
end

to skip_newlines(tokens, pos)
    set skip_newlines_p to 0
    set skip_newlines_p to pos
    while token_kind(tokens, skip_newlines_p) is "nl"
        set skip_newlines_p to skip_newlines_p + 1
    end
    give back skip_newlines_p
end

// Whether `kind` is one of the kinds a block body stops at.
to contains(kinds, kind)
    set contains_found to 0
    set contains_i to 0
    set contains_found to no
    set contains_i to 0
    while contains_i < length(kinds)
        if kinds[contains_i] is kind then
            set contains_found to yes
        end
        set contains_i to contains_i + 1
    end
    give back contains_found
end

// The deepest a block body or an expression may nest — the same bound
// `parser::MAX_BLOCK_DEPTH` and `MAX_NESTING_DEPTH` use.
to check_depth(line, depth, what)
    if depth > 64 then
        refuse(line, what + " nest more than 64 levels deep")
    end
    give back depth
end

// ---------------------------------------------------------------------------
// Statements
//
// A note on the shape of everything below, because it decides how it is
// written. A name a Redblue function assigns is one **program-wide** name: the
// VM binds it where no scope holds it and keeps it after the function returns
// (see FINDINGS.md). So a name assigned here is shared by every live invocation
// of this function, and a recursive function that keeps one in a variable
// across a call reads back the innermost call's value.
//
// Two rules follow, and every function below follows both:
//
//   1. State a recursive function carries — the list it is building, the node it
//      is building it from, the position it is at — is a **parameter**, and the
//      next invocation gets it by tail call.
//   2. A value computed before a call is either a parameter of that call, or it
//      is assigned again afterwards, or it is re-derived from the tokens.
//
// `token_kind`, `token_line`, `skip_newlines`, `contains`, `word_at`,
// `expect_kind` and `check_depth` are leaves: they call nothing that calls back,
// so a name read on either side of one of them is safe. Everything else is a
// call that could be inside a nested parse.
// ---------------------------------------------------------------------------

// `[statements, next position]`
to parse_program(tokens)
    set parse_program_body to parse_statements(tokens, 0, 0, [])
    give back parse_program_body[0]
end

// The statement list of a program or of a block body, built by tail recursion so
// that the list it is building is a parameter rather than a name every nested
// statement would overwrite.
to parse_statements(tokens, pos, depth, body)
    set parse_statements_p to skip_newlines(tokens, pos)
    set parse_statements_out to nothing
    if token_kind(tokens, parse_statements_p) is "eof" then
        set parse_statements_out to [body, parse_statements_p]
    end
    if type_of(parse_statements_out) is "nothing" then
        set parse_statements_r to parse_statement(tokens, parse_statements_p, depth, token_line(tokens, parse_statements_p))
        set parse_statements_next to body
        if parse_statements_r[0] is not nothing then
            set parse_statements_next to push(body, parse_statements_r[0])
        end
        set parse_statements_out to parse_statements(tokens, parse_statements_r[1], depth, parse_statements_next)
    end
    give back parse_statements_out
end

// The statement dispatch of `Parser::parse_statement_inner`, one flat branch per
// keyword so that no branch nests inside another. `line` is a parameter because
// the line is needed after the branch's own parse has run.
to parse_statement(tokens, pos, depth, line)
    set parse_statement_stmt to nothing
    set parse_statement_pos to pos

    if token_kind(tokens, pos) is "say" then
        set parse_statement_r to parse_expression(tokens, pos + 1, 0, 0)
        set parse_statement_stmt to {s: "say", l: line, e: parse_statement_r[0]}
        set parse_statement_pos to parse_statement_r[1]
    end
    if token_kind(tokens, pos) is "print" then
        set parse_statement_r to parse_expression(tokens, pos + 1, 0, 0)
        set parse_statement_stmt to {s: "print", l: line, e: parse_statement_r[0]}
        set parse_statement_pos to parse_statement_r[1]
    end
    if token_kind(tokens, pos) is "set" then
        set parse_statement_r to parse_set(tokens, pos + 1, line)
        set parse_statement_stmt to parse_statement_r[0]
        set parse_statement_pos to parse_statement_r[1]
    end
    if token_kind(tokens, pos) is "constant" then
        set parse_statement_r to parse_constant(tokens, pos + 1, line)
        set parse_statement_stmt to parse_statement_r[0]
        set parse_statement_pos to parse_statement_r[1]
    end
    if token_kind(tokens, pos) is "import" then
        set parse_statement_r to parse_import(tokens, pos + 1)
        set parse_statement_stmt to {s: "import", l: line, items: parse_statement_r[0]}
        set parse_statement_pos to parse_statement_r[1]
    end
    if token_kind(tokens, pos) is "if" then
        set parse_statement_r to parse_if(tokens, pos, depth, line)
        set parse_statement_stmt to parse_statement_r[0]
        set parse_statement_pos to parse_statement_r[1]
    end
    if token_kind(tokens, pos) is "unless" then
        set parse_statement_r to parse_unless(tokens, pos, depth, line)
        set parse_statement_stmt to parse_statement_r[0]
        set parse_statement_pos to parse_statement_r[1]
    end
    if token_kind(tokens, pos) is "for" then
        set parse_statement_r to parse_for(tokens, pos, depth, line)
        set parse_statement_stmt to parse_statement_r[0]
        set parse_statement_pos to parse_statement_r[1]
    end
    if token_kind(tokens, pos) is "repeat" then
        set parse_statement_r to parse_repeat(tokens, pos, depth, line)
        set parse_statement_stmt to parse_statement_r[0]
        set parse_statement_pos to parse_statement_r[1]
    end
    if token_kind(tokens, pos) is "while" then
        set parse_statement_r to parse_while(tokens, pos, depth, line)
        set parse_statement_stmt to parse_statement_r[0]
        set parse_statement_pos to parse_statement_r[1]
    end
    if token_kind(tokens, pos) is "break" then
        set parse_statement_stmt to {s: "break", l: line}
        set parse_statement_pos to pos + 1
    end
    if token_kind(tokens, pos) is "skip" then
        set parse_statement_stmt to {s: "skip", l: line}
        set parse_statement_pos to pos + 1
    end
    if token_kind(tokens, pos) is "return" or token_kind(tokens, pos) is "giveback" then
        set parse_statement_r to parse_return(tokens, pos, line)
        set parse_statement_stmt to parse_statement_r[0]
        set parse_statement_pos to parse_statement_r[1]
    end
    if token_kind(tokens, pos) is "to" then
        set parse_statement_r to parse_function(tokens, pos, depth, line)
        set parse_statement_stmt to parse_statement_r[0]
        set parse_statement_pos to parse_statement_r[1]
    end
    if token_kind(tokens, pos) is "object" then
        set parse_statement_r to parse_object(tokens, pos, depth, line)
        set parse_statement_stmt to parse_statement_r[0]
        set parse_statement_pos to parse_statement_r[1]
    end
    if token_kind(tokens, pos) is "try" then
        set parse_statement_r to parse_try(tokens, pos, depth, line)
        set parse_statement_stmt to parse_statement_r[0]
        set parse_statement_pos to parse_statement_r[1]
    end
    if token_kind(tokens, pos) is "module" then
        set parse_statement_r to parse_module(tokens, pos, depth, line)
        set parse_statement_stmt to parse_statement_r[0]
        set parse_statement_pos to parse_statement_r[1]
    end
    if token_kind(tokens, pos) is "export" then
        set parse_statement_r to parse_export(tokens, pos + 1, line)
        set parse_statement_stmt to parse_statement_r[0]
        set parse_statement_pos to parse_statement_r[1]
    end
    if token_kind(tokens, pos) is "test" then
        set parse_statement_r to parse_test(tokens, pos, depth, line)
        set parse_statement_stmt to parse_statement_r[0]
        set parse_statement_pos to parse_statement_r[1]
    end
    if token_kind(tokens, pos) is "expect" then
        set parse_statement_r to parse_expect(tokens, pos + 1)
        set parse_statement_stmt to {s: "expr", l: line, e: {k: "expect", a: parse_statement_r[0][0], b: parse_statement_r[0][1]}}
        set parse_statement_pos to parse_statement_r[1]
    end
    if token_kind(tokens, pos) is "nl" then
        set parse_statement_pos to pos + 1
    end
    // Whether any branch above ran, decided by re-reading the token rather than
    // by a flag: a nested `parse_statement` would have overwritten the flag,
    // because a name a Redblue function assigns is one program-wide name.
    if is_statement_keyword(tokens, pos) is no then
        set parse_statement_r to parse_expression(tokens, pos, 0, 0)
        set parse_statement_stmt to {s: "expr", l: line, e: parse_statement_r[0]}
        set parse_statement_pos to parse_statement_r[1]
    end

    set parse_statement_out to [parse_statement_stmt, parse_statement_pos]
    give back parse_statement_out
end

// Whether the statement at `pos` begins with a keyword this dispatcher reads,
// rather than with an expression. A pure function of the token stream, so that
// asking it after a nested parse gives the same answer as asking it before one.
to is_statement_keyword(tokens, pos)
    set is_statement_keyword_k to token_kind(tokens, pos)
    set is_statement_keyword_yes to no
    if is_statement_keyword_k is "say" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "print" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "set" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "constant" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "import" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "if" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "unless" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "for" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "repeat" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "while" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "break" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "skip" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "return" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "giveback" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "to" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "object" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "try" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "module" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "export" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "test" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "expect" then
        set is_statement_keyword_yes to yes
    end
    if is_statement_keyword_k is "nl" then
        set is_statement_keyword_yes to yes
    end
    give back is_statement_keyword_yes
end

// `give back` with no value, or the expression after it. Split out because a
// branch of `parse_statement` cannot return from it.
to parse_return(tokens, pos, line)
    set parse_return_out to nothing
    if is_expression_start(tokens, pos + 1) then
        set parse_return_r to parse_expression(tokens, pos + 1, 0, 0)
        set parse_return_out to [{s: "return", l: line, e: parse_return_r[0]}, parse_return_r[1]]
    end
    if type_of(parse_return_out) is "nothing" then
        set parse_return_out to [{s: "return", l: line, e: nothing}, pos + 1]
    end
    give back parse_return_out
end

// `Parser::is_expression_start`: what may follow a `give back` with no value.
to is_expression_start(tokens, pos)
    set is_expression_start_k to token_kind(tokens, pos)
    set is_expression_start_starts to no
    if is_expression_start_k is "num" then
        set is_expression_start_starts to yes
    end
    if is_expression_start_k is "text" then
        set is_expression_start_starts to yes
    end
    if is_expression_start_k is "yes" then
        set is_expression_start_starts to yes
    end
    if is_expression_start_k is "no" then
        set is_expression_start_starts to yes
    end
    if is_expression_start_k is "id" then
        set is_expression_start_starts to yes
    end
    if is_expression_start_k is "lparen" then
        set is_expression_start_starts to yes
    end
    if is_expression_start_k is "lbrack" then
        set is_expression_start_starts to yes
    end
    if is_expression_start_k is "lbrace" then
        set is_expression_start_starts to yes
    end
    if is_expression_start_k is "not" then
        set is_expression_start_starts to yes
    end
    if is_expression_start_k is "minus" then
        set is_expression_start_starts to yes
    end
    if is_expression_start_k is "to" then
        set is_expression_start_starts to yes
    end
    give back is_expression_start_starts
end

// `set NAME to <expr>` and `set NAME.field to <expr>`.
to parse_set(tokens, pos, line)
    set parse_set_k to token_kind(tokens, pos)
    set parse_set_out to nothing
    if parse_set_k is not "id" then
        if parse_set_k is not "this" then
            refuse(line, "expected a variable name after 'set'")
        end
    end
    set parse_set_stmt to nothing
    if token_kind(tokens, pos + 1) is "dot" then
        if token_kind(tokens, pos + 2) is not "id" then
            refuse(line, "expected a property name")
        end
        set parse_set_q to expect_kind(tokens, pos + 3, "to")
        set parse_set_r to parse_expression(tokens, parse_set_q, 0, 0)
        set parse_set_stmt to {s: "setproperty", l: line, o: tokens[pos].b, p: tokens[pos + 2].b, e: parse_set_r[0]}
        set parse_set_out to [parse_set_stmt, parse_set_r[1]]
    end
    if token_kind(tokens, pos + 1) is not "dot" then
        set parse_set_q to expect_kind(tokens, pos + 1, "to")
        set parse_set_r to parse_expression(tokens, parse_set_q, 0, 0)
        set parse_set_stmt to {s: "set", l: line, n: tokens[pos].b, e: parse_set_r[0]}
        set parse_set_out to [parse_set_stmt, parse_set_r[1]]
    end
    give back parse_set_out
end

// `constant NAME to <expr>`.
to parse_constant(tokens, pos, line)
    if token_kind(tokens, pos) is not "id" then
        refuse(line, "expected a name after 'constant'")
    end
    set parse_constant_p to expect_kind(tokens, pos + 1, "to")
    set parse_constant_r to parse_expression(tokens, parse_constant_p, 0, 0)
    set parse_constant_out to [{s: "constant", l: line, n: tokens[pos].b, e: parse_constant_r[0]}, parse_constant_r[1]]
    give back parse_constant_out
end

// `import Name [as Alias], ...` — `to` is the older alias keyword and is
// accepted too, because programs in `tests/` use it.
to parse_import(tokens, pos)
    set parse_import_alias to []
    if token_kind(tokens, pos) is not "id" then
        refuse(token_line(tokens, pos), "expected a module name")
    end
    set parse_import_k to token_kind(tokens, pos + 1)
    set parse_import_out to nothing
    if parse_import_k is "as" or parse_import_k is "to" then
        if token_kind(tokens, pos + 2) is not "id" then
            refuse(token_line(tokens, pos + 1), "expected an alias name")
        end
        set parse_import_alias to tokens[pos + 2].b
        set parse_import_out to parse_import_more(tokens, pos + 3, [{n: tokens[pos].b, a: parse_import_alias}])
    end
    if type_of(parse_import_out) is "nothing" then
        set parse_import_out to parse_import_more(tokens, pos + 1, [{n: tokens[pos].b, a: []}])
    end
    give back parse_import_out
end

to parse_import_more(tokens, pos, items)
    set parse_import_more_out to nothing
    if token_kind(tokens, pos) is "comma" then
        if token_kind(tokens, pos + 1) is not "id" then
            refuse(token_line(tokens, pos + 1), "expected a module name")
        end
        set parse_import_more_k to token_kind(tokens, pos + 2)
        set parse_import_more_tail to nothing
        if parse_import_more_k is "as" or parse_import_more_k is "to" then
            if token_kind(tokens, pos + 3) is not "id" then
                refuse(token_line(tokens, pos + 2), "expected an alias name")
            end
            set parse_import_more_tail to tokens[pos + 3].b
        end
        if type_of(parse_import_more_tail) is "nothing" then
            set parse_import_more_next to push(items, {n: tokens[pos + 1].b, a: []})
            set parse_import_more_out to parse_import_more(tokens, pos + 3, parse_import_more_next)
        end
        if type_of(parse_import_more_tail) is not "nothing" then
            set parse_import_more_next to push(items, {n: tokens[pos + 1].b, a: parse_import_more_tail})
            set parse_import_more_out to parse_import_more(tokens, pos + 4, parse_import_more_next)
        end
    end
    if type_of(parse_import_more_out) is "nothing" then
        set parse_import_more_out to [items, pos]
    end
    give back parse_import_more_out
end

// The `{ statement } end` tail every block form shares: every statement up to the
// token that ends the block, with the newlines between them skipped. Built by
// tail recursion, with the body it is building a parameter.
to parse_block_body(tokens, pos, depth, stops)
    give back parse_block_statements(tokens, pos, depth, stops, [])
end

to parse_block_statements(tokens, pos, depth, stops, body)
    set parse_block_statements_p to skip_newlines(tokens, pos)
    set parse_block_statements_out to nothing
    if token_kind(tokens, parse_block_statements_p) is "eof" then
        set parse_block_statements_out to [body, parse_block_statements_p]
    end
    if type_of(parse_block_statements_out) is "nothing" then
        if contains(stops, token_kind(tokens, parse_block_statements_p)) then
            set parse_block_statements_out to [body, parse_block_statements_p]
        end
    end
    if type_of(parse_block_statements_out) is "nothing" then
        set parse_block_statements_r to parse_statement(tokens, parse_block_statements_p, depth, token_line(tokens, parse_block_statements_p))
        set parse_block_statements_next to body
        if parse_block_statements_r[0] is not nothing then
            set parse_block_statements_next to push(body, parse_block_statements_r[0])
        end
        set parse_block_statements_out to parse_block_statements(tokens, parse_block_statements_r[1], depth, stops, parse_block_statements_next)
    end
    give back parse_block_statements_out
end

// `if <expr> then ... else ... end`.
to parse_if(tokens, pos, depth, line)
    set parse_if_cond to parse_expression(tokens, pos + 1, 0, 0)
    give back parse_if_then(tokens, parse_if_cond[1], depth, line, parse_if_cond[0])
end

to parse_if_then(tokens, pos, depth, line, cond)
    set parse_if_then_p to expect_kind(tokens, skip_newlines(tokens, pos), "then")
    set parse_if_then_body to parse_block_body(tokens, parse_if_then_p, check_depth(line, depth + 1, "blocks"), ["end", "else"])
    set parse_if_then_out to nothing
    if token_kind(tokens, parse_if_then_body[1]) is "else" then
        set parse_if_then_tail to parse_block_body(tokens, parse_if_then_body[1] + 1, depth + 1, ["end"])
        set parse_if_then_out to parse_if_finish(tokens, parse_if_then_tail[1], line, cond, parse_if_then_body[0], parse_if_then_tail[0])
    end
    if type_of(parse_if_then_out) is "nothing" then
        set parse_if_then_q to expect_kind(tokens, parse_if_then_body[1], "end")
        set parse_if_then_out to [{s: "if", l: line, c: cond, t: parse_if_then_body[0], e: []}, parse_if_then_q]
    end
    give back parse_if_then_out
end

to parse_if_finish(tokens, pos, line, cond, then_branch, else_branch)
    set parse_if_finish_q to expect_kind(tokens, pos, "end")
    set parse_if_finish_out to [{s: "if", l: line, c: cond, t: then_branch, e: else_branch}, parse_if_finish_q]
    give back parse_if_finish_out
end

// `unless` has no `else` arm: the grammar refuses one, so a block that writes
// one is a parse error here as it is in stage 1.
to parse_unless(tokens, pos, depth, line)
    set parse_unless_cond to parse_expression(tokens, pos + 1, 0, 0)
    give back parse_unless_then(tokens, parse_unless_cond[1], depth, line, parse_unless_cond[0])
end

to parse_unless_then(tokens, pos, depth, line, cond)
    set parse_unless_then_p to expect_kind(tokens, skip_newlines(tokens, pos), "then")
    set parse_unless_then_body to parse_block_body(tokens, parse_unless_then_p, check_depth(line, depth + 1, "blocks"), ["end"])
    set parse_unless_then_q to expect_kind(tokens, parse_unless_then_body[1], "end")
    set parse_unless_then_out to [{s: "unless", l: line, c: cond, t: parse_unless_then_body[0]}, parse_unless_then_q]
    give back parse_unless_then_out
end

// `for each x in <list>` or `for each i from <a> to <b> [by <step>]`. `by` is
// matched as the identifier the lexer produced, because it is not a keyword: a
// parameter may be named `by`.
to parse_for(tokens, pos, depth, line)
    if token_kind(tokens, pos + 1) is not "each" then
        refuse(line, "expected 'each' after 'for'")
    end
    if token_kind(tokens, pos + 2) is not "id" then
        refuse(line, "expected a variable name")
    end
    give back parse_for_body(tokens, pos + 3, depth, line, tokens[pos + 2].b)
end

to parse_for_body(tokens, pos, depth, line, variable)
    set parse_for_body_out to nothing
    if token_kind(tokens, pos) is "from" then
        set parse_for_body_start to parse_expression(tokens, pos + 1, 0, 0)
        set parse_for_body_q to expect_kind(tokens, parse_for_body_start[1], "to")
        set parse_for_body_stop to parse_expression(tokens, parse_for_body_q, 0, 0)
        set parse_for_body_out to parse_for_range(tokens, parse_for_body_stop[1], depth, line, variable, parse_for_body_start[0], parse_for_body_stop[0])
    end
    if type_of(parse_for_body_out) is "nothing" then
        set parse_for_body_q to expect_kind(tokens, pos, "in")
        set parse_for_body_iter to parse_expression(tokens, parse_for_body_q, 0, 0)
        set parse_for_body_out to parse_for_each(tokens, parse_for_body_iter[1], depth, line, variable, parse_for_body_iter[0])
    end
    give back parse_for_body_out
end

to parse_for_each(tokens, pos, depth, line, variable, iterable)
    set parse_for_each_body to parse_block_body(tokens, pos, check_depth(line, depth + 1, "blocks"), ["end"])
    set parse_for_each_q to expect_kind(tokens, parse_for_each_body[1], "end")
    set parse_for_each_out to [{s: "foreach", l: line, v: variable, e: iterable, t: parse_for_each_body[0]}, parse_for_each_q]
    give back parse_for_each_out
end

to parse_for_range(tokens, pos, depth, line, variable, start, stop)
    set parse_for_range_out to nothing
    if word_at(tokens, pos) is "by" then
        set parse_for_range_step to parse_expression(tokens, pos + 1, 0, 0)
        set parse_for_range_out to parse_for_range_body(tokens, parse_for_range_step[1], depth, line, variable, start, stop, parse_for_range_step[0], yes)
    end
    if type_of(parse_for_range_out) is "nothing" then
        set parse_for_range_out to parse_for_range_body(tokens, pos, depth, line, variable, start, stop, nothing, no)
    end
    give back parse_for_range_out
end

to parse_for_range_body(tokens, pos, depth, line, variable, start, stop, step, has_step)
    set parse_for_range_body_inner to parse_block_body(tokens, pos, check_depth(line, depth + 1, "blocks"), ["end"])
    set parse_for_range_body_q to expect_kind(tokens, parse_for_range_body_inner[1], "end")
    set parse_for_range_body_out to [{s: "forrange", l: line, v: variable, a: start, b: stop, step: step, has_step: has_step, t: parse_for_range_body_inner[0]}, parse_for_range_body_q]
    give back parse_for_range_body_out
end

// `repeat <expr> times ... end`
to parse_repeat(tokens, pos, depth, line)
    set parse_repeat_count to parse_expression(tokens, pos + 1, 0, 0)
    give back parse_repeat_body(tokens, parse_repeat_count[1], depth, line, parse_repeat_count[0])
end

to parse_repeat_body(tokens, pos, depth, line, count)
    set parse_repeat_body_p to expect_kind(tokens, pos, "times")
    set parse_repeat_body_inner to parse_block_body(tokens, parse_repeat_body_p, check_depth(line, depth + 1, "blocks"), ["end"])
    set parse_repeat_body_q to expect_kind(tokens, parse_repeat_body_inner[1], "end")
    set parse_repeat_body_out to [{s: "repeat", l: line, e: count, t: parse_repeat_body_inner[0]}, parse_repeat_body_q]
    give back parse_repeat_body_out
end

// `while <expr> ... end` — no `then`: `while` takes the condition and the body.
to parse_while(tokens, pos, depth, line)
    set parse_while_cond to parse_expression(tokens, pos + 1, 0, 0)
    give back parse_while_body(tokens, parse_while_cond[1], depth, line, parse_while_cond[0])
end

to parse_while_body(tokens, pos, depth, line, cond)
    set parse_while_body_inner to parse_block_body(tokens, pos, check_depth(line, depth + 1, "blocks"), ["end"])
    set parse_while_body_q to expect_kind(tokens, parse_while_body_inner[1], "end")
    set parse_while_body_out to [{s: "while", l: line, c: cond, t: parse_while_body_inner[0]}, parse_while_body_q]
    give back parse_while_body_out
end

// `to name(params) ... end`
to parse_function(tokens, pos, depth, line)
    give back parse_callable(tokens, pos + 1, check_depth(line, depth + 1, "blocks"), line, "function")
end

// `to can name(params) ... end` inside an object body. `can` is optional
// because SPEC.md writes the constructor without it.
to parse_method(tokens, pos, depth, line)
    set parse_method_out to nothing
    if token_kind(tokens, pos + 1) is "can" then
        set parse_method_out to parse_callable(tokens, pos + 2, depth, line, "method")
    end
    if type_of(parse_method_out) is "nothing" then
        set parse_method_out to parse_callable(tokens, pos + 1, depth, line, "method")
    end
    give back parse_method_out
end

// The shared tail of `to name(...)` and `to can name(...)`.
to parse_callable(tokens, pos, depth, line, kind)
    if token_kind(tokens, pos) is not "id" then
        refuse(token_line(tokens, pos), "expected a function name")
    end
    give back parse_callable_params(tokens, pos + 1, depth, line, kind, tokens[pos].b)
end

to parse_callable_params(tokens, pos, depth, line, kind, name)
    set parse_callable_params_out to nothing
    if token_kind(tokens, pos) is "lparen" then
        set parse_callable_params_out to parse_callable_args(tokens, pos + 1, depth, line, kind, name, [])
    end
    if type_of(parse_callable_params_out) is "nothing" then
        set parse_callable_params_body to parse_block_body(tokens, pos, depth, ["end"])
        set parse_callable_params_q to expect_kind(tokens, parse_callable_params_body[1], "end")
        set parse_callable_params_out to [{s: kind, l: line, n: name, ps: [], t: parse_callable_params_body[0]}, parse_callable_params_q]
    end
    give back parse_callable_params_out
end

// The `(a, b)` of a callable, when it has one. The list is optional, so
// `to f give back 1 end` and `to f() give back 1 end` are the same function.
to parse_callable_args(tokens, pos, depth, line, kind, name, params)
    set parse_callable_args_out to nothing
    if token_kind(tokens, pos) is "id" then
        set parse_callable_args_next to push(params, tokens[pos].b)
        set parse_callable_args_p to pos + 1
        if token_kind(tokens, parse_callable_args_p) is "comma" then
            set parse_callable_args_out to parse_callable_args(tokens, parse_callable_args_p + 1, depth, line, kind, name, parse_callable_args_next)
        end
        if type_of(parse_callable_args_out) is "nothing" then
            set parse_callable_args_q to expect_kind(tokens, parse_callable_args_p, "rparen")
            set parse_callable_args_out to parse_callable_finish(tokens, parse_callable_args_q, depth, line, kind, name, parse_callable_args_next)
        end
    end
    if type_of(parse_callable_args_out) is "nothing" then
        set parse_callable_args_q to expect_kind(tokens, pos, "rparen")
        set parse_callable_args_out to parse_callable_finish(tokens, parse_callable_args_q, depth, line, kind, name, params)
    end
    give back parse_callable_args_out
end

to parse_callable_finish(tokens, pos, depth, line, kind, name, params)
    set parse_callable_finish_body to parse_block_body(tokens, pos, depth, ["end"])
    set parse_callable_finish_q to expect_kind(tokens, parse_callable_finish_body[1], "end")
    set parse_callable_finish_out to [{s: kind, l: line, n: name, ps: params, t: parse_callable_finish_body[0]}, parse_callable_finish_q]
    give back parse_callable_finish_out
end

// `has name [default <expr>]`. `default` is not a keyword, so it is matched by
// name, as stage 1 does.
to parse_has(tokens, pos, line)
    if token_kind(tokens, pos + 1) is not "id" then
        refuse(line, "expected a field name after 'has'")
    end
    set parse_has_out to nothing
    if word_at(tokens, pos + 2) is "default" then
        set parse_has_r to parse_expression(tokens, pos + 3, 0, 0)
        set parse_has_out to [{s: "has", l: line, n: tokens[pos + 1].b, d: parse_has_r[0]}, parse_has_r[1]]
    end
    if type_of(parse_has_out) is "nothing" then
        set parse_has_out to [{s: "has", l: line, n: tokens[pos + 1].b, d: nothing}, pos + 2]
    end
    give back parse_has_out
end

// An object body splits: `has` and `to can` are declarations the object reads,
// and everything else is a statement of the enclosing block. The codegen makes
// the same split, so the two agree about which half a statement lands in.
to parse_object(tokens, pos, depth, line)
    if token_kind(tokens, pos + 1) is not "id" then
        refuse(line, "expected an object name")
    end
    give back parse_object_parent(tokens, pos + 2, depth, line, tokens[pos + 1].b)
end

to parse_object_parent(tokens, pos, depth, line, name)
    set parse_object_parent_out to nothing
    if token_kind(tokens, pos) is "extends" then
        if token_kind(tokens, pos + 1) is not "id" then
            refuse(line, "expected a parent object name")
        end
        set parse_object_parent_out to parse_object_body(tokens, pos + 2, depth, line, name, tokens[pos + 1].b)
    end
    if type_of(parse_object_parent_out) is "nothing" then
        set parse_object_parent_out to parse_object_body(tokens, pos, depth, line, name, [])
    end
    give back parse_object_parent_out
end

to parse_object_body(tokens, pos, depth, line, name, parent)
    give back parse_object_statements(tokens, skip_newlines(tokens, pos), depth, line, name, parent, [])
end

to parse_object_statements(tokens, pos, depth, line, name, parent, body)
    set parse_object_statements_p to skip_newlines(tokens, pos)
    set parse_object_statements_out to nothing
    if token_kind(tokens, parse_object_statements_p) is "end" then
        set parse_object_statements_out to parse_object_finish(tokens, parse_object_statements_p + 1, line, name, parent, body)
    end
    if token_kind(tokens, parse_object_statements_p) is "eof" then
        refuse(line, "expected 'end' to close the object")
    end
    if type_of(parse_object_statements_out) is "nothing" then
        set parse_object_statements_r to parse_object_member(tokens, parse_object_statements_p, depth, line)
        set parse_object_statements_next to body
        if parse_object_statements_r[0] is not nothing then
            set parse_object_statements_next to push(body, parse_object_statements_r[0])
        end
        set parse_object_statements_out to parse_object_statements(tokens, parse_object_statements_r[1], depth, line, name, parent, parse_object_statements_next)
    end
    give back parse_object_statements_out
end

// One member of an object body: a `has`, a `to can`, or an ordinary statement.
to parse_object_member(tokens, pos, depth, line)
    set parse_object_member_out to nothing
    // A `has` and a `to can` are lines of their own: each carries the line it
    // was written on, not the line of the `object` above it.
    if token_kind(tokens, pos) is "has" then
        set parse_object_member_out to parse_has(tokens, pos, token_line(tokens, pos))
    end
    if token_kind(tokens, pos) is "to" then
        set parse_object_member_out to parse_method(tokens, pos, depth, token_line(tokens, pos))
    end
    if token_kind(tokens, pos) is not "has" then
        if token_kind(tokens, pos) is not "to" then
            set parse_object_member_out to parse_statement(tokens, pos, depth, token_line(tokens, pos))
        end
    end
    give back parse_object_member_out
end

to parse_object_finish(tokens, pos, line, name, parent, body)
    set parse_object_finish_out to [{s: "object", l: line, n: name, parent: parent, t: body}, pos]
    give back parse_object_finish_out
end

// `try ... catch ... finally ... end`
to parse_try(tokens, pos, depth, line)
    set parse_try_body to parse_block_body(tokens, pos + 1, check_depth(line, depth + 1, "blocks"), ["catch", "finally", "end"])
    give back parse_try_catch(tokens, parse_try_body[1], depth, line, parse_try_body[0])
end

to parse_try_catch(tokens, pos, depth, line, body)
    set parse_try_catch_out to nothing
    if token_kind(tokens, pos) is "catch" then
        set parse_try_catch_out to parse_try_catch_body(tokens, pos + 1, depth, line, body, [])
    end
    if type_of(parse_try_catch_out) is "nothing" then
        set parse_try_catch_out to parse_try_finally(tokens, pos, depth, line, body, [], [])
    end
    give back parse_try_catch_out
end

to parse_try_catch_body(tokens, pos, depth, line, body, catch_name)
    set parse_try_catch_body_out to nothing
    if token_kind(tokens, pos) is "id" then
        set parse_try_catch_body_out to parse_try_catch_body_tail(tokens, pos + 1, depth, line, body, tokens[pos].b)
    end
    if type_of(parse_try_catch_body_out) is "nothing" then
        set parse_try_catch_body_out to parse_try_catch_body_tail(tokens, pos, depth, line, body, catch_name)
    end
    give back parse_try_catch_body_out
end

to parse_try_catch_body_tail(tokens, pos, depth, line, body, catch_name)
    set parse_try_catch_body_tail_inner to parse_block_body(tokens, pos, depth, ["finally", "end"])
    set parse_try_catch_body_tail_out to nothing
    set parse_try_catch_body_tail_out to parse_try_finally(tokens, parse_try_catch_body_tail_inner[1], depth, line, body, catch_name, parse_try_catch_body_tail_inner[0])
    give back parse_try_catch_body_tail_out
end

to parse_try_finally(tokens, pos, depth, line, body, catch_name, catch_body)
    set parse_try_finally_out to nothing
    if token_kind(tokens, pos) is "finally" then
        set parse_try_finally_inner to parse_block_body(tokens, pos + 1, depth, ["end"])
        set parse_try_finally_q to expect_kind(tokens, parse_try_finally_inner[1], "end")
        set parse_try_finally_out to [{s: "try", l: line, t: body, cn: catch_name, c: catch_body, f: parse_try_finally_inner[0]}, parse_try_finally_q]
    end
    if type_of(parse_try_finally_out) is "nothing" then
        set parse_try_finally_q to expect_kind(tokens, pos, "end")
        set parse_try_finally_out to [{s: "try", l: line, t: body, cn: catch_name, c: catch_body, f: []}, parse_try_finally_q]
    end
    give back parse_try_finally_out
end

// `module NAME ... end`
to parse_module(tokens, pos, depth, line)
    if token_kind(tokens, pos + 1) is not "id" then
        refuse(line, "expected a module name")
    end
    set parse_module_body to parse_block_body(tokens, pos + 2, check_depth(line, depth + 1, "blocks"), ["end"])
    set parse_module_q to expect_kind(tokens, parse_module_body[1], "end")
    set parse_module_out to [{s: "module", l: line, n: tokens[pos + 1].b, t: parse_module_body[0]}, parse_module_q]
    give back parse_module_out
end

// `export name, ...` or `export all`.
to parse_export(tokens, pos, line)
    set parse_export_out to nothing
    if word_at(tokens, pos) is "all" then
        set parse_export_out to parse_export_more(tokens, pos + 1, line, [], yes)
    end
    if type_of(parse_export_out) is "nothing" then
        if token_kind(tokens, pos) is "id" then
            set parse_export_out to parse_export_more(tokens, pos + 1, line, [tokens[pos].b], no)
        end
    end
    if type_of(parse_export_out) is "nothing" then
        refuse(token_line(tokens, pos), "expected a name to export after 'export'")
    end
    give back parse_export_out
end

to parse_export_more(tokens, pos, line, names, all)
    set parse_export_more_out to nothing
    if token_kind(tokens, pos) is "comma" then
        set parse_export_more_out to parse_export_more(tokens, pos + 1, line, names, all)
    end
    if type_of(parse_export_more_out) is "nothing" then
        set parse_export_more_out to [{s: "export", l: line, names: names, all: all}, pos]
    end
    give back parse_export_more_out
end

// `test "name" ... end`
to parse_test(tokens, pos, depth, line)
    set parse_test_out to nothing
    if token_kind(tokens, pos + 1) is "text" then
        set parse_test_out to parse_test_body(tokens, pos + 2, depth, line, tokens[pos + 1].b)
    end
    if type_of(parse_test_out) is "nothing" then
        if token_kind(tokens, pos + 1) is "id" then
            set parse_test_out to parse_test_body(tokens, pos + 2, depth, line, tokens[pos + 1].b)
        end
    end
    if type_of(parse_test_out) is "nothing" then
        refuse(line, "expected a test name")
    end
    give back parse_test_out
end

to parse_test_body(tokens, pos, depth, line, name)
    set parse_test_body_inner to parse_block_body(tokens, pos, check_depth(line, depth + 1, "blocks"), ["end"])
    set parse_test_body_q to expect_kind(tokens, parse_test_body_inner[1], "end")
    set parse_test_body_out to [{s: "test", l: line, n: name, t: parse_test_body_inner[0]}, parse_test_body_q]
    give back parse_test_body_out
end

// `expect <actual> to [be] <expected>`
to parse_expect(tokens, pos)
    set parse_expect_actual to parse_expression(tokens, pos, 0, 0)
    give back parse_expect_be(tokens, expect_kind(tokens, parse_expect_actual[1], "to"), parse_expect_actual[0])
end

to parse_expect_be(tokens, pos, actual)
    set parse_expect_be_p to pos
    if word_at(tokens, parse_expect_be_p) is "be" then
        set parse_expect_be_p to parse_expect_be_p + 1
    end
    set parse_expect_be_expected to parse_expression(tokens, parse_expect_be_p, 0, 0)
    set parse_expect_be_out to [[actual, parse_expect_be_expected[0]], parse_expect_be_expected[1]]
    give back parse_expect_be_out
end

// ---------------------------------------------------------------------------
// Expressions
//
// `Parser::parse_expression`: the precedence chain of `parse_or`..
// `parse_multiplication` as one function over five levels. Level 0 is `or`,
// 1 is `and`, 2 is comparison, 3 is `+ -`, 4 is `* / mod`, and 5 falls through to
// the unary and postfix forms. `level` walks the precedence chain; `depth`
// counts nesting, which is a different thing: one operator on level 0 is one
// nesting level in stage 1 and none of the six frames this function costs.
// ---------------------------------------------------------------------------

// `[node, next position]`
to parse_expression(tokens, pos, level, depth)
    if depth > 64 then
        refuse(token_line(tokens, pos), "expressions nest more than 64 levels deep")
    end
    set parse_expression_out to nothing
    if level is 5 then
        set parse_expression_out to parse_unary(tokens, pos, depth)
    end
    if type_of(parse_expression_out) is "nothing" then
        set parse_expression_out to parse_expression_rest(tokens, pos, level, depth)
    end
    give back parse_expression_out
end

// One precedence level's left operand, then the level's operators — as a tail
// recursion, so the node it is building is a parameter of the next step.
to parse_expression_rest(tokens, pos, level, depth)
    set parse_expression_rest_left to parse_expression(tokens, pos, level + 1, depth)
    give back parse_levels(tokens, parse_expression_rest_left[1], level, depth, parse_expression_rest_left[0])
end

// The operator loop of one level. The operator it read is handed to
// `parse_operand`, which hands it back: a name this compiler assigns is one
// program-wide name, and a nested parse would otherwise overwrite it between
// reading it and using it.
to parse_levels(tokens, pos, level, depth, node)
    set parse_levels_read to read_operator(tokens, pos, level)
    set parse_levels_kind to parse_levels_read[0]
    set parse_levels_width to parse_levels_read[1]
    set parse_levels_out to nothing
    if parse_levels_kind is "" then
        set parse_levels_out to [node, pos]
    end
    if type_of(parse_levels_out) is "nothing" then
        set parse_levels_step to parse_operand(tokens, pos + parse_levels_width, level + 1, depth, parse_levels_kind, parse_levels_width)
        set parse_levels_next to bin_node(parse_levels_step[2], node, parse_levels_step[0])
        set parse_levels_out to parse_levels(tokens, parse_levels_step[1], level, depth, parse_levels_next)
    end
    give back parse_levels_out
end

// `[node, position, operator kind, operator width]` — the operator in and the
// operator out, so that the caller can build the binary node with it after this
// call has returned.
to parse_operand(tokens, pos, level, depth, kind, width)
    set parse_operand_node to parse_expression(tokens, pos, level, depth)
    set parse_operand_out to [parse_operand_node[0], parse_operand_node[1], kind, width]
    give back parse_operand_out
end

// A binary node, built by a call so that `kind` — which the caller read before
// its own call and must use after — is read at the call rather than across it.
to bin_node(kind, left, right)
    set bin_node_out to {k: "bin", op: kind, l: left, r: right}
    give back bin_node_out
end

// The operator at `pos` for `level`, as `[op, tokens consumed]` — `["", 0]`
// when the level's operators are not what comes next.
to read_operator(tokens, pos, level)
    set read_operator_k to token_kind(tokens, pos)
    set read_operator_op to ""
    set read_operator_width to 0
    set read_operator_resolved to no

    if level is 2 then
        if read_operator_k is "is" then
            set read_operator_r to read_is_operator(tokens, pos)
            set read_operator_op to read_operator_r[0]
            set read_operator_width to read_operator_r[1]
            set read_operator_resolved to yes
        end
    end

    if read_operator_resolved is no then
        if level is 0 then
            if read_operator_k is "or" then
                set read_operator_op to "or"
                set read_operator_width to 1
            end
        end
        if level is 1 then
            if read_operator_k is "and" then
                set read_operator_op to "and"
                set read_operator_width to 1
            end
        end
        if level is 2 then
            if read_operator_k is "eq" then
                set read_operator_op to "eq"
                set read_operator_width to 1
            end
            if read_operator_k is "ne" then
                set read_operator_op to "ne"
                set read_operator_width to 1
            end
            if read_operator_k is "lt" then
                set read_operator_op to "lt"
                set read_operator_width to 1
            end
            if read_operator_k is "gt" then
                set read_operator_op to "gt"
                set read_operator_width to 1
            end
            if read_operator_k is "le" then
                set read_operator_op to "le"
                set read_operator_width to 1
            end
            if read_operator_k is "ge" then
                set read_operator_op to "ge"
                set read_operator_width to 1
            end
        end
        if level is 3 then
            if read_operator_k is "plus" then
                set read_operator_op to "add"
                set read_operator_width to 1
            end
            if read_operator_k is "minus" then
                set read_operator_op to "sub"
                set read_operator_width to 1
            end
        end
        if level is 4 then
            if read_operator_k is "star" then
                set read_operator_op to "mul"
                set read_operator_width to 1
            end
            if read_operator_k is "slash" then
                set read_operator_op to "div"
                set read_operator_width to 1
            end
            if read_operator_k is "mod" then
                set read_operator_op to "mod"
                set read_operator_width to 1
            end
        end
    end
    set read_operator_out to [read_operator_op, read_operator_width]
    give back read_operator_out
end

// The comparison that follows an `is`, in either its word form
// (`is greater than or equal to`) or its symbolic one (`is =`, `is not`). A
// word counts as an operator only when the whole phrase is there, so a variable
// named `greater` still compares for equality.
//
// The width is how many tokens the whole comparison consumes, `is` included,
// which is what the caller adds to the position it found the operator at.
to read_is_operator(tokens, pos)
    set read_is_operator_result to ""
    set read_is_operator_width to 0
    set read_is_operator_first to word_at(tokens, pos + 1)
    set read_is_operator_second to word_at(tokens, pos + 2)
    set read_is_operator_k to token_kind(tokens, pos + 1)

    if read_is_operator_first is "equal" then
        if token_kind(tokens, pos + 2) is "to" then
            set read_is_operator_result to "eq"
            set read_is_operator_width to 3
        end
    end
    if read_is_operator_first is "greater" then
        if read_is_operator_second is "than" then
            set read_is_operator_result to "gt"
            set read_is_operator_width to 3
            if or_equal_to_ahead(tokens, pos + 3) then
                set read_is_operator_result to "ge"
                set read_is_operator_width to 6
            end
        end
    end
    if read_is_operator_first is "less" then
        if read_is_operator_second is "than" then
            set read_is_operator_result to "lt"
            set read_is_operator_width to 3
            if or_equal_to_ahead(tokens, pos + 3) then
                set read_is_operator_result to "le"
                set read_is_operator_width to 6
            end
        end
    end
    if read_is_operator_result is "" then
        if read_is_operator_k is "eq" then
            set read_is_operator_result to "eq"
            set read_is_operator_width to 2
        end
        if read_is_operator_k is "not" then
            set read_is_operator_result to "ne"
            set read_is_operator_width to 2
        end
        if read_is_operator_k is "in" then
            set read_is_operator_result to "in"
            set read_is_operator_width to 2
        end
    end
    // A bare `is` is equality, and is the only token it consumes.
    if read_is_operator_result is "" then
        set read_is_operator_result to "eq"
        set read_is_operator_width to 1
    end
    set read_is_operator_out to [read_is_operator_result, read_is_operator_width]
    give back read_is_operator_out
end

// Whether the whole `or equal to` tail of a compound comparison is there. It is
// matched as one phrase rather than as an `or`, so `x is greater than or equal
// to y` is one comparison and not `(x is greater than y) or equal to y`.
to or_equal_to_ahead(tokens, pos)
    set or_equal_to_ahead_found to no
    if token_kind(tokens, pos) is "or" then
        if word_at(tokens, pos + 1) is "equal" then
            if token_kind(tokens, pos + 2) is "to" then
                set or_equal_to_ahead_found to yes
            end
        end
    end
    give back or_equal_to_ahead_found
end

// `not` and `-`, which bind tighter than every binary operator.
to parse_unary(tokens, pos, depth)
    set parse_unary_out to nothing
    if token_kind(tokens, pos) is "not" then
        set parse_unary_inner to parse_unary(tokens, pos + 1, depth + 1)
        set parse_unary_out to [{k: "un", op: "not", e: parse_unary_inner[0]}, parse_unary_inner[1]]
    end
    if type_of(parse_unary_out) is "nothing" then
        if token_kind(tokens, pos) is "minus" then
            set parse_unary_inner to parse_unary(tokens, pos + 1, depth + 1)
            set parse_unary_out to [{k: "un", op: "neg", e: parse_unary_inner[0]}, parse_unary_inner[1]]
        end
    end
    if type_of(parse_unary_out) is "nothing" then
        set parse_unary_out to parse_postfix(tokens, pos, depth)
    end
    give back parse_unary_out
end

// `.property`, `(args)` and `[index]`, each of which turns what has been parsed
// so far into something else. The expression parsed so far is a parameter of the
// next step, because a nested parse would overwrite a name kept in a variable.
to parse_postfix(tokens, pos, depth)
    set parse_postfix_base to parse_primary(tokens, pos, depth)
    give back parse_postfix_more(tokens, parse_postfix_base[1], depth, parse_postfix_base[0])
end

to parse_postfix_more(tokens, pos, depth, node)
    set parse_postfix_more_out to nothing
    if token_kind(tokens, pos) is "dot" then
        if token_kind(tokens, pos + 1) is not "id" then
            refuse(token_line(tokens, pos), "expected a property name")
        end
        set parse_postfix_more_next to {k: "prop", o: node, p: tokens[pos + 1].b}
        set parse_postfix_more_out to parse_postfix_more(tokens, pos + 2, depth, parse_postfix_more_next)
    end
    if type_of(parse_postfix_more_out) is "nothing" then
        if token_kind(tokens, pos) is "lparen" then
            set parse_postfix_more_args to parse_arguments(tokens, pos + 1, depth, [])
            set parse_postfix_more_call to make_call(node, parse_postfix_more_args[0], tokens, pos)
            set parse_postfix_more_out to parse_postfix_more(tokens, parse_postfix_more_args[1], depth, parse_postfix_more_call)
        end
    end
    if type_of(parse_postfix_more_out) is "nothing" then
        if token_kind(tokens, pos) is "lbrack" then
            set parse_postfix_more_index to parse_expression(tokens, pos + 1, 0, depth + 1)
            set parse_postfix_more_next to {k: "index", o: node, i: parse_postfix_more_index[0]}
            set parse_postfix_more_q to expect_kind(tokens, parse_postfix_more_index[1], "rbrack")
            set parse_postfix_more_out to parse_postfix_more(tokens, parse_postfix_more_q, depth, parse_postfix_more_next)
        end
    end
    if type_of(parse_postfix_more_out) is "nothing" then
        set parse_postfix_more_out to [node, pos]
    end
    give back parse_postfix_more_out
end

// `(a, b)` — the arguments of a call, with the position after the `)`.
to parse_arguments(tokens, pos, depth, args)
    set parse_arguments_out to nothing
    if token_kind(tokens, pos) is "rparen" then
        set parse_arguments_out to [args, pos + 1]
    end
    if token_kind(tokens, pos) is "eof" then
        set parse_arguments_out to [args, pos]
    end
    if type_of(parse_arguments_out) is "nothing" then
        set parse_arguments_arg to parse_expression(tokens, pos, 0, depth + 1)
        set parse_arguments_next to push(args, parse_arguments_arg[0])
        set parse_arguments_p to parse_arguments_arg[1]
        set parse_arguments_more to nothing
        if token_kind(tokens, parse_arguments_p) is "comma" then
            set parse_arguments_more to parse_arguments(tokens, parse_arguments_p + 1, depth, parse_arguments_next)
        end
        if type_of(parse_arguments_more) is "nothing" then
            set parse_arguments_more to [parse_arguments_next, expect_kind(tokens, parse_arguments_p, "rparen")]
        end
        set parse_arguments_out to parse_arguments_more
    end
    give back parse_arguments_out
end

// `receiver.method(args)` and `name(args)` are one expression each: which of
// the two it is depends on what the call was written on, and the runtime
// decides whether the receiver names an object or a module.
to make_call(node, args, tokens, pos)
    set make_call_call to nothing
    if node.k is "var" then
        set make_call_call to {k: "call", n: node.n, args: args}
    end
    if node.k is "prop" then
        set make_call_call to {k: "mcall", o: node.o, m: node.p, args: args}
    end
    if type_of(make_call_call) is "nothing" then
        refuse(token_line(tokens, pos), "expected a function name")
    end
    give back make_call_call
end

to parse_primary(tokens, pos, depth)
    set parse_primary_out to nothing
    if token_kind(tokens, pos) is "num" then
        set parse_primary_out to [{k: "num", v: tokens[pos].v}, pos + 1]
    end
    if type_of(parse_primary_out) is "nothing" then
        if token_kind(tokens, pos) is "text" then
            set parse_primary_out to [{k: "text", b: tokens[pos].b}, pos + 1]
        end
    end
    if type_of(parse_primary_out) is "nothing" then
        if token_kind(tokens, pos) is "yes" then
            set parse_primary_out to [{k: "yes"}, pos + 1]
        end
    end
    if type_of(parse_primary_out) is "nothing" then
        if token_kind(tokens, pos) is "no" then
            set parse_primary_out to [{k: "no"}, pos + 1]
        end
    end
    if type_of(parse_primary_out) is "nothing" then
        if token_kind(tokens, pos) is "nothing" then
            set parse_primary_out to [{k: "nothing"}, pos + 1]
        end
    end
    // `this` is a plain variable that only a method call binds.
    if type_of(parse_primary_out) is "nothing" then
        if token_kind(tokens, pos) is "id" then
            set parse_primary_out to [{k: "var", n: tokens[pos].b}, pos + 1]
        end
    end
    if type_of(parse_primary_out) is "nothing" then
        if token_kind(tokens, pos) is "this" then
            set parse_primary_out to [{k: "var", n: tokens[pos].b}, pos + 1]
        end
    end
    if type_of(parse_primary_out) is "nothing" then
        if token_kind(tokens, pos) is "lparen" then
            set parse_primary_out to parse_parenthesised(tokens, pos + 1, depth)
        end
    end
    if type_of(parse_primary_out) is "nothing" then
        if token_kind(tokens, pos) is "lbrack" then
            set parse_primary_items to parse_list_items(tokens, pos + 1, depth, [])
            set parse_primary_out to [{k: "list", items: parse_primary_items[0]}, expect_kind(tokens, parse_primary_items[1], "rbrack")]
        end
    end
    if type_of(parse_primary_out) is "nothing" then
        if token_kind(tokens, pos) is "lbrace" then
            set parse_primary_fields to parse_record_fields(tokens, pos + 1, depth, [])
            set parse_primary_out to [{k: "record", fields: parse_primary_fields[0]}, expect_kind(tokens, parse_primary_fields[1], "rbrace")]
        end
    end

    // A function literal is an expression in the grammar, and stage 1 refuses to
    // compile one rather than compiling it to something the bytecode VM would
    // read differently from the tree-walker. Stage 2 refuses it the same way
    // rather than inventing a third answer.
    if type_of(parse_primary_out) is "nothing" then
        if token_kind(tokens, pos) is "to" then
            refuse(token_line(tokens, pos), "'to (x) ... end' as an expression is not compiled yet; run programs that use one with 'rb run'")
        end
    end
    if type_of(parse_primary_out) is "nothing" then
        refuse(token_line(tokens, pos), "unexpected token " + token_kind(tokens, pos))
    end
    give back parse_primary_out
end

to parse_parenthesised(tokens, pos, depth)
    set parse_parenthesised_inner to parse_expression(tokens, pos, 0, depth + 1)
    set parse_parenthesised_out to [parse_parenthesised_inner[0], expect_kind(tokens, parse_parenthesised_inner[1], "rparen")]
    give back parse_parenthesised_out
end

// `[a, b, ]` — the items, and the position of the `]`. Built by tail recursion
// with the list as a parameter.
to parse_list_items(tokens, pos, depth, items)
    set parse_list_items_out to nothing
    if token_kind(tokens, pos) is "rbrack" then
        set parse_list_items_out to [items, pos]
    end
    if token_kind(tokens, pos) is "eof" then
        set parse_list_items_out to [items, pos]
    end
    if type_of(parse_list_items_out) is "nothing" then
        set parse_list_items_item to parse_expression(tokens, pos, 0, depth + 1)
        set parse_list_items_next to push(items, parse_list_items_item[0])
        set parse_list_items_p to parse_list_items_item[1]
        // A second name for the outcome, because the first was overwritten by
        // the parse above: a name a Redblue function assigns is one
        // program-wide name, and this function recurses.
        set parse_list_items_more to nothing
        if token_kind(tokens, parse_list_items_p) is "comma" then
            set parse_list_items_more to parse_list_items(tokens, parse_list_items_p + 1, depth, parse_list_items_next)
        end
        if type_of(parse_list_items_more) is "nothing" then
            set parse_list_items_more to [parse_list_items_next, parse_list_items_p]
        end
        set parse_list_items_out to parse_list_items_more
    end
    give back parse_list_items_out
end

// `{key: value, ...}` — the fields, and the position of the `}`.
to parse_record_fields(tokens, pos, depth, fields)
    set parse_record_fields_out to nothing
    if token_kind(tokens, pos) is "rbrace" then
        set parse_record_fields_out to [fields, pos]
    end
    if token_kind(tokens, pos) is "eof" then
        set parse_record_fields_out to [fields, pos]
    end
    if type_of(parse_record_fields_out) is "nothing" then
        if token_kind(tokens, pos) is not "id" then
            refuse(token_line(tokens, pos), "expected a field name")
        end
        set parse_record_fields_value to parse_expression(tokens, expect_kind(tokens, pos + 1, "colon"), 0, depth + 1)
        set parse_record_fields_next to push(fields, {k: tokens[pos].b, v: parse_record_fields_value[0]})
        set parse_record_fields_p to parse_record_fields_value[1]
        set parse_record_fields_more to nothing
        if token_kind(tokens, parse_record_fields_p) is "comma" then
            set parse_record_fields_more to parse_record_fields(tokens, parse_record_fields_p + 1, depth, parse_record_fields_next)
        end
        if type_of(parse_record_fields_more) is "nothing" then
            set parse_record_fields_more to [parse_record_fields_next, parse_record_fields_p]
        end
        set parse_record_fields_out to parse_record_fields_more
    end
    give back parse_record_fields_out
end

// ---------------------------------------------------------------------------
// Code generation
//
// `src/bytecode/codegen.rs` is the specification. `ctx` carries the constant
// pool (`p`), the intern table (`i`), the current block's instructions (`c`),
// its pending jump patches (`j`) and its nested blocks (`b`).
// ---------------------------------------------------------------------------

to compile_program(program)
    set compile_program_ctx to 0
    set compile_program_r to 0
    set compile_program_ctx to {p: [], i: [], c: [], j: [], b: []}
    set compile_program_r to compile_block(program, 0, [109, 97, 105, 110], 0, [], 0, compile_program_ctx, yes, "statements")
    give back {p: compile_program_r[1].p, main: compile_program_r[0]}
end

// `Compiler::block`: one declaration compiled into a block of its own.
to compile_block(body, kind, name, arity, params, depth, ctx, reads_value, body_kind)
    if depth > 64 then
        refuse(0, "program nests blocks more than 64 levels deep")
    end
    set compile_block_inner to {p: ctx.p, i: ctx.i, c: [], j: [], b: []}
    set compile_block_inner to compile_statements(body, compile_block_inner, depth, reads_value, body_kind)
    set compile_block_block to {k: kind, n: name, a: arity, ps: params, code: apply_patches(compile_block_inner.c, compile_block_inner.j), b: compile_block_inner.b}
    give back [compile_block_block, compile_block_inner]
end

// A nested block: compiled first, so its constants enter the pool before the
// instruction that names it, then appended to this block's own children.
to nested_block(body, kind, name, arity, params, depth, ctx, reads_value, body_kind)
    set nested_block_r to 0
    set nested_block_next to 0
    set nested_block_r to compile_block(body, kind, name, arity, params, depth, ctx, reads_value, body_kind)
    set nested_block_next to {p: nested_block_r[1].p, i: nested_block_r[1].i, c: ctx.c, j: ctx.j, b: push(ctx.b, nested_block_r[0])}
    give back [nested_block_next, length(ctx.b)]
end

// Carries a block's pool and intern table forward, leaving this block's own
// instruction list and children alone.
to with_state(ctx, other)
    give back {p: other.p, i: other.i, c: ctx.c, j: ctx.j, b: ctx.b}
end

to with_children(ctx, children)
    give back {p: ctx.p, i: ctx.i, c: ctx.c, j: ctx.j, b: children}
end

to with_patches(ctx, patches)
    give back {p: ctx.p, i: ctx.i, c: ctx.c, j: patches, b: ctx.b}
end

to compile_statements(body, ctx, depth, reads_value, body_kind)
    for each each_stmt in body
        set ctx to compile_statement(each_stmt, ctx, depth, reads_value, body_kind)
    end
    give back ctx
end

to emit(ctx, opcode, arg, aux, line)
    give back {p: ctx.p, i: ctx.i, c: push(ctx.c, {o: opcode, a: arg, x: aux, l: line}), j: ctx.j, b: ctx.b}
end

// A jump whose target is not known yet: the slot, and what to do with it once
// the block is complete.
to emit_jump(ctx, opcode, line)
    set emit_jump_slot to length(ctx.c)
    set ctx to emit(ctx, opcode, 0, 0, line)
    give back with_patches(ctx, push(ctx.j, {slot: emit_jump_slot, at: "here", value: 0}))
end

// The same, with the slot it recorded: a caller whose patch target is not the
// end of the block needs to know where the jump is, and cannot keep that in a
// name across the code it compiles next.
to emit_jump_at(ctx, opcode, line)
    set emit_jump_at_slot to length(ctx.c)
    set ctx to emit(ctx, opcode, 0, 0, line)
    set ctx to with_patches(ctx, push(ctx.j, {slot: emit_jump_at_slot, at: "here", value: 0}))
    set emit_jump_at_out to [ctx, emit_jump_at_slot]
    give back emit_jump_at_out
end

// A jump that points at an instruction already emitted.
to patch_at(ctx, slot, offset)
    give back with_patches(ctx, push(ctx.j, {slot: slot, at: "there", value: offset}))
end

// Points every recorded jump at its target. A jump that landed at the end of the
// block is resolved here, which is why the file needs no instruction after the
// last statement for one to land on.
to apply_patches(code, patches)
    set apply_patches_out to 0
    set apply_patches_i to 0
    set apply_patches_arg to 0
    set apply_patches_out to []
    set apply_patches_i to 0
    while apply_patches_i < length(code)
        set apply_patches_arg to code[apply_patches_i].a
        for each each_patch in patches
            if each_patch.slot is apply_patches_i then
                if each_patch.at is "here" then
                    set apply_patches_arg to length(code)
                else
                    set apply_patches_arg to each_patch.value
                end
            end
        end
        set apply_patches_out to push(apply_patches_out, {o: code[apply_patches_i].o, a: apply_patches_arg, x: code[apply_patches_i].x, l: code[apply_patches_i].l})
        set apply_patches_i to apply_patches_i + 1
    end
    give back apply_patches_out
end

// The intern table. A name used a hundred times is one entry, so the pool is
// the same whichever way round the source used them.
to intern(ctx, raw)
    set intern_found to 0
    set intern_i to 0
    set intern_found to -1
    set intern_i to 0
    while intern_i < length(ctx.i)
        if ctx.i[intern_i].k is raw then
            set intern_found to intern_i
        end
        set intern_i to intern_i + 1
    end
    set intern_result to nothing
    if intern_found >= 0 then
        set intern_result to {ctx: ctx, index: ctx.i[intern_found].n}
    else
        set intern_index to length(ctx.p)
        set intern_pool to push(ctx.p, {t: 3, b: raw})
        set intern_table to push(ctx.i, {k: raw, n: intern_index})
        set intern_result to {ctx: {p: intern_pool, i: intern_table, c: ctx.c, j: ctx.j, b: ctx.b}, index: intern_index}
    end
    give back intern_result
end

// `Compiler::text`: intern a name and use it as an operand.
to emit_name(ctx, opcode, raw, aux, line)
    set emit_name_r to 0
    set emit_name_r to intern(ctx, raw)
    give back emit(emit_name_r.ctx, opcode, emit_name_r.index, aux, line)
end

// A constant that is **not** interned: a literal, appended every time it is
// written.
to emit_const(ctx, tag, raw, line)
    set emit_const_index to 0
    set emit_const_pool to 0
    set emit_const_index to length(ctx.p)
    set emit_const_pool to push(ctx.p, {t: tag, b: raw})
    give back emit({p: emit_const_pool, i: ctx.i, c: ctx.c, j: ctx.j, b: ctx.b}, 1, emit_const_index, 0, line)
end

// `Constant::Nothing`, which is the value a `return` with no value returns.
to emit_nothing(ctx, line)
    set emit_nothing_index to 0
    set emit_nothing_pool to 0
    set emit_nothing_index to length(ctx.p)
    set emit_nothing_pool to push(ctx.p, {t: 0, b: []})
    give back emit({p: emit_nothing_pool, i: ctx.i, c: ctx.c, j: ctx.j, b: ctx.b}, 1, emit_nothing_index, 0, line)
end

to compile_statement(stmt, ctx, depth, reads_value, body_kind)
    set compile_statement_charges to 0

    // The marker that begins every statement the tree-walking VM compile_statement_charges a step
    // for. The two exceptions are the statements a declaration *reads* rather
    // than runs: an `export` in a module body, and a `has` or `to can` in an
    // object body.
    set compile_statement_charges to yes
    if body_kind is "module" then
        if stmt.s is "export" then
            set compile_statement_charges to no
        end
    end
    if body_kind is "object" then
        if stmt.s is "has" then
            set compile_statement_charges to no
        end
        if stmt.s is "method" then
            set compile_statement_charges to no
        end
    end
    if compile_statement_charges is yes then
        set ctx to emit(ctx, 0, STATEMENT_MARKER, 0, stmt.l)
    end

    if stmt.s is "say" then
        set ctx to compile_expr(stmt.e, ctx, stmt.l)
        set ctx to emit(ctx, 5, 0, 0, stmt.l)
    end
    if stmt.s is "print" then
        set ctx to compile_expr(stmt.e, ctx, stmt.l)
        set ctx to emit(ctx, 6, 0, 0, stmt.l)
    end
    if stmt.s is "expr" then
        set ctx to compile_expr(stmt.e, ctx, stmt.l)
        // A block nobody reads the value of is a statement, and a statement
        // consumes what it produced. An `expect` is the one expression that
        // pushes nothing — it compares both operands itself — so a `POP` after
        // one would underflow rather than discard.
        if reads_value is no then
            if stmt.e.k is not "expect" then
                set ctx to emit(ctx, 2, 0, 0, stmt.l)
            end
        end
    end
    if stmt.s is "set" then
        set ctx to compile_expr(stmt.e, ctx, stmt.l)
        set ctx to emit_name(ctx, 4, stmt.n, 0, stmt.l)
    end
    if stmt.s is "constant" then
        set ctx to compile_expr(stmt.e, ctx, stmt.l)
        set ctx to emit_name(ctx, 46, stmt.n, 0, stmt.l)
    end
    if stmt.s is "setproperty" then
        set ctx to emit_name(ctx, 3, stmt.o, 0, stmt.l)
        set ctx to compile_expr(stmt.e, ctx, stmt.l)
        set ctx to emit_name(ctx, 8, cat(cat(stmt.o, [46]), stmt.p), 0, stmt.l)
    end
    if stmt.s is "if" then
        set ctx to compile_expr(stmt.c, ctx, stmt.l)
        set compile_statement_r to emit_jump_at(ctx, 35, stmt.l)
        // The slot travels as a parameter into the branch: a name in a variable
        // would not survive the nested parse of that branch, because a name a
        // Redblue function assigns is one program-wide name.
        set ctx to compile_if_branches(stmt, compile_statement_r[0], depth, compile_statement_r[1])
    end
    if stmt.s is "unless" then
        set ctx to compile_expr(stmt.c, ctx, stmt.l)
        // There is no `JumpIfTrue`, so the condition is negated and the existing
        // jump runs the body exactly when it was false.
        set ctx to emit(ctx, 28, 0, 0, stmt.l)
        set compile_statement_r to emit_jump_at(ctx, 35, stmt.l)
        set ctx to compile_unless_body(stmt, compile_statement_r[0], depth, compile_statement_r[1])
    end
    if stmt.s is "foreach" then
        set ctx to compile_expr(stmt.e, ctx, stmt.l)
        set ctx to emit(ctx, 36, 0, 0, stmt.l)
        set ctx to emit_name(ctx, 4, stmt.v, 0, stmt.l)
        set ctx to compile_loop_body(stmt, ctx, depth, length(ctx.c) - 1, stmt.l)
    end
    if stmt.s is "forrange" then
        set ctx to compile_expr(stmt.a, ctx, stmt.l)
        set ctx to compile_expr(stmt.b, ctx, stmt.l)
        set ctx to compile_expr(stmt.step, ctx, stmt.l)
        if stmt.has_step is no then
            // The step operand is compiled either way and popped when the loop
            // has none, so `GET_RANGE`'s arity is the only thing that says how
            // many bounds were pushed.
            set ctx to emit(ctx, 2, 0, 0, stmt.l)
        end
        set ctx to emit(ctx, 37, 0, range_arity(stmt.has_step), stmt.l)
        set ctx to emit_name(ctx, 4, stmt.v, 0, stmt.l)
        set ctx to compile_loop_body(stmt, ctx, depth, length(ctx.c) - 1, stmt.l)
    end
    if stmt.s is "repeat" then
        set ctx to compile_expr(stmt.e, ctx, stmt.l)
        set ctx to emit(ctx, 37, 0, 1, stmt.l)
        set ctx to emit_name(ctx, 4, [36, 99, 111, 117, 110, 116, 101, 114], 0, stmt.l)
        set ctx to compile_loop_body(stmt, ctx, depth, length(ctx.c) - 1, stmt.l)
    end
    if stmt.s is "while" then
        set ctx to compile_while(stmt, ctx, depth, length(ctx.c))
    end
    if stmt.s is "break" then
        set ctx to emit(ctx, 32, 0, 0, stmt.l)
    end
    if stmt.s is "skip" then
        set ctx to emit(ctx, 33, 0, 0, stmt.l)
    end
    if stmt.s is "return" then
        if stmt.e is nothing then
            set ctx to emit_nothing(ctx, stmt.l)
        else
            set ctx to compile_expr(stmt.e, ctx, stmt.l)
        end
        set ctx to emit(ctx, 31, 0, 0, stmt.l)
    end
    if stmt.s is "function" then
        set compile_statement_r to nested_block(stmt.t, 1, stmt.n, length(stmt.ps), stmt.ps, depth, ctx, yes, "statements")
        set ctx to compile_statement_r[0]
        set ctx to emit(ctx, 38, compile_statement_r[1], length(stmt.ps), stmt.l)
        set ctx to emit_name(ctx, 4, stmt.n, 0, stmt.l)
    end
    if stmt.s is "method" then
        set compile_statement_r to nested_block(stmt.t, 2, stmt.n, length(stmt.ps), stmt.ps, depth, ctx, yes, "statements")
        set ctx to compile_statement_r[0]
        set ctx to emit(ctx, 39, compile_statement_r[1], length(stmt.ps), stmt.l)
    end
    if stmt.s is "has" then
        if stmt.d is nothing then
            set ctx to emit_nothing(ctx, stmt.l)
        else
            set ctx to compile_expr(stmt.d, ctx, stmt.l)
        end
        set ctx to emit_name(ctx, 41, stmt.n, 0, stmt.l)
    end
    if stmt.s is "object" then
        set ctx to compile_object(stmt, ctx, depth)
    end
    if stmt.s is "try" then
        set ctx to compile_try(stmt, ctx, depth, stmt.l)
    end
    if stmt.s is "import" then
        for each each_item in stmt.items
            set compile_statement_module_name to intern(ctx, each_item.n)
            set ctx to compile_statement_module_name.ctx
            if length(each_item.a) is 0 then
                // An import with no alias binds its module's own name, so the
                // operand is never the reserved one.
                set ctx to emit(ctx, 43, compile_statement_module_name.index, compile_statement_module_name.index, stmt.l)
                set ctx to emit_name(ctx, 4, each_item.n, 0, stmt.l)
            else
                set compile_statement_bound to intern(ctx, each_item.a)
                set ctx to emit(ctx, 43, compile_statement_module_name.index, compile_statement_bound.index, stmt.l)
                set ctx to emit_name(ctx, 4, each_item.a, 0, stmt.l)
            end
        end
    end
    if stmt.s is "module" then
        set ctx to compile_module(stmt, ctx, depth)
    end
    if stmt.s is "test" then
        set compile_statement_r to nested_block(stmt.t, 4, stmt.n, 0, [], depth, ctx, yes, "statements")
        set ctx to compile_statement_r[0]
        set ctx to emit(ctx, 44, compile_statement_r[1], 0, stmt.l)
    end
    // An `export` outside a module declaration publishes nothing, and one inside
    // one was already compiled into the run of `EXPORT`s the module's own block
    // opens with. Neither emits here.
    give back ctx
end

// An object body compiles as two halves, and the split is the tree-walking
// VM's: `has` and `to can` go into the block that assembles the type, and
// everything else compiles into the *enclosing* block, after the `STORE` that
// binds the name.
to compile_object(stmt, ctx, depth)
    set compile_object_line to 0
    set compile_object_declarations to 0
    set compile_object_rest to 0
    set compile_object_line to stmt.l
    set compile_object_declarations to []
    set compile_object_rest to []
    for each each_inner in stmt.t
        if each_inner.s is "has" then
            set compile_object_declarations to push(compile_object_declarations, each_inner)
        else
            if each_inner.s is "method" then
                set compile_object_declarations to push(compile_object_declarations, each_inner)
            else
                set compile_object_rest to push(compile_object_rest, each_inner)
            end
        end
    end
    set compile_object_r to nested_block(compile_object_declarations, 3, stmt.n, 0, [], depth, ctx, no, "object")
    set ctx to compile_object_r[0]
    // The parent goes in as a name, interned like every other name in the
    // program: a flag saying "this one extends something" would leave the file
    // unable to say what.
    if length(stmt.parent) is 0 then
        set ctx to emit(ctx, 40, compile_object_r[1], NO_CONST, compile_object_line)
    else
        set compile_object_parent_name to intern(ctx, stmt.parent)
        set ctx to emit(compile_object_parent_name.ctx, 40, compile_object_r[1], compile_object_parent_name.index, compile_object_line)
    end
    set ctx to emit_name(ctx, 4, stmt.n, 0, compile_object_line)
    give back compile_statements(compile_object_rest, ctx, depth, no, "statements")
end

// `try ... catch ... finally ... end`: the handlers become blocks of their own,
// so the protected code stays a straight run of instructions with no patching.
to compile_try(stmt, ctx, depth, line)
    set compile_try_line to line
    set compile_try_catch_index to 0
    set compile_try_r to 0
    set compile_try_line to stmt.l
    set compile_try_catch_index to NO_CONST
    if length(stmt.cn) > 0 then
        set compile_try_r to nested_block(stmt.c, 5, stmt.cn, 0, [], depth, ctx, no, "statements")
        set ctx to compile_try_r[0]
        set compile_try_catch_index to compile_try_r[1]
    else
        if length(stmt.c) > 0 then
            set compile_try_r to nested_block(stmt.c, 5, [], 0, [], depth, ctx, no, "statements")
            set ctx to compile_try_r[0]
            set compile_try_catch_index to compile_try_r[1]
        end
    end
    set compile_try_finally_index to NO_CONST
    if length(stmt.f) > 0 then
        set compile_try_r to nested_block(stmt.f, 6, [], 0, [], depth, ctx, no, "statements")
        set ctx to compile_try_r[0]
        set compile_try_finally_index to compile_try_r[1]
    end
    // Both the `TRY` and the marked `NOP` below take the line as a *parameter*:
    // the handler bodies are compiled between them, and a nested `try` would
    // overwrite a name held in a variable.
    set ctx to emit(ctx, 42, compile_try_catch_index, compile_try_finally_index, line)
    set ctx to compile_statements(stmt.t, ctx, depth, no, "statements")
    // The marked `NOP` closes the protected region: it is where the handlers are
    // popped and the `finally` runs, whether or not the protected code failed.
    set ctx to emit(ctx, 0, END_TRY_MARKER, 0, line)
    give back ctx
end

// `module NAME ... end`: a block run in a scope of its own, opening with the run
// of `EXPORT`s that says what it publishes.
to compile_module(stmt, ctx, depth)
    set compile_module_line to 0
    set compile_module_line to stmt.l
    if depth >= 64 then
        refuse(compile_module_line, "program nests blocks more than 64 levels deep")
    end
    set compile_module_inner to {p: ctx.p, i: ctx.i, c: [], j: [], b: []}
    set compile_module_inner to compile_exports(stmt.t, compile_module_inner, compile_module_line)
    set compile_module_inner to compile_statements(stmt.t, compile_module_inner, depth + 1, no, "module")
    set compile_module_block to {k: 7, n: stmt.n, a: 0, ps: [], code: apply_patches(compile_module_inner.c, compile_module_inner.j), b: compile_module_inner.b}
    set ctx to with_state(ctx, compile_module_inner)
    set ctx to with_children(ctx, push(ctx.b, compile_module_block))
    give back emit(ctx, 47, length(ctx.b) - 1, 0, compile_module_line)
end

// The run of `EXPORT`s a module declaration opens with. The names come from the
// same two rules stage 1 reads them with: the *last* `export` in the body says
// what is published, and a module publishes the names its body declares.
to compile_exports(body, ctx, line)
    set compile_exports_names to 0
    set compile_exports_all to 0
    set compile_exports_found to 0
    set compile_exports_names to []
    set compile_exports_all to no
    set compile_exports_found to no
    for each each_stmt in body
        if each_stmt.s is "export" then
            set compile_exports_names to each_stmt.names
            set compile_exports_all to each_stmt.all
            set compile_exports_found to yes
        end
    end

    if compile_exports_found is yes then
        set compile_exports_declared to []
        for each each_stmt in body
            if each_stmt.s is "function" then
                set compile_exports_declared to push(compile_exports_declared, each_stmt.n)
            end
            if each_stmt.s is "set" then
                set compile_exports_declared to push(compile_exports_declared, each_stmt.n)
            end
            if each_stmt.s is "constant" then
                set compile_exports_declared to push(compile_exports_declared, each_stmt.n)
            end
        end

        // `export compile_exports_all` compile_exports_names nothing, so the list it publishes is the compile_exports_declared
        // one.
        set compile_exports_published to compile_exports_names
        if compile_exports_all is yes then
            set compile_exports_published to compile_exports_declared
        end
        for each each_name in compile_exports_published
            // An `export` of a each_name the module does not define is written with
            // the reserved operand, which is what a reader refuses rather than
            // publishes. The refusal itself is not made here: the tree-walking
            // VM reports it when the declaration runs.
            set compile_exports_flag to NO_CONST
            if compile_exports_all is yes then
                set compile_exports_flag to 0
            else
                if contains(compile_exports_declared, each_name) then
                    set compile_exports_flag to 0
                end
            end
            set ctx to emit_name(ctx, 48, each_name, compile_exports_flag, line)
        end
    end
    give back ctx
end

// How many bounds a `for each i from <a> to <b>` loop passes: the end, plus the
// step when it has one.
to range_arity(has_step)
    set range_arity_count to 2
    if has_step is yes then
        set range_arity_count to 3
    end
    give back range_arity_count
end
// The body of a loop and the jump back to its head. `top` is the instruction
// the jump goes back to, passed as a parameter because the body is compiled
// between finding it and using it.
to compile_loop_body(stmt, ctx, depth, top, line)
    set ctx to compile_statements(stmt.t, ctx, depth, no, "statements")
    set ctx to emit(ctx, 34, top, 0, line)
    give back ctx
end

// `while` compiles its condition first and jumps back to it, so its head is the
// length of the code before the condition rather than after it.
to compile_while(stmt, ctx, depth, top)
    set ctx to compile_expr(stmt.c, ctx, stmt.l)
    set compile_while_r to emit_jump_at(ctx, 35, stmt.l)
    set ctx to compile_statements(stmt.t, compile_while_r[0], depth, no, "statements")
    set ctx to emit(ctx, 34, top, 0, stmt.l)
    set ctx to patch_at(ctx, compile_while_r[1], length(ctx.c))
    give back ctx
end

// The two branches of an `if`, once its `JumpIfFalse` has been emitted. `slot`
// is where that jump is, and it is a parameter because the body below it is a
// nested parse that would otherwise overwrite a name held in a variable.
to compile_if_branches(stmt, ctx, depth, slot)
    set ctx to compile_statements(stmt.t, ctx, depth, no, "statements")
    if length(stmt.e) is 0 then
        set ctx to patch_at(ctx, slot, length(ctx.c))
    else
        set compile_if_branches_r to emit_jump_at(ctx, 34, stmt.l)
        set ctx to patch_at(compile_if_branches_r[0], slot, length(compile_if_branches_r[0].c))
        set ctx to compile_if_else(stmt, compile_if_branches_r[0], depth, compile_if_branches_r[1])
    end
    give back ctx
end

// The `else` branch of an `if`, and the patch that sends the jump over it to
// the end of the block. `end_slot` is where that jump is: the branch is
// compiled between finding it and using it, so it travels as a parameter.
to compile_if_else(stmt, ctx, depth, end_slot)
    set ctx to compile_statements(stmt.e, ctx, depth, no, "statements")
    set ctx to patch_at(ctx, end_slot, length(ctx.c))
    give back ctx
end

// The body of an `unless`, and the patch that ends it.
to compile_unless_body(stmt, ctx, depth, slot)
    set ctx to compile_statements(stmt.t, ctx, depth, no, "statements")
    set ctx to patch_at(ctx, slot, length(ctx.c))
    give back ctx
end

// The opcode of a binary operator.
to binary_opcode(op)
    set binary_opcode_opcode to 0
    set binary_opcode_opcode to 0
    if op is "add" then
        set binary_opcode_opcode to 13
    end
    if op is "sub" then
        set binary_opcode_opcode to 14
    end
    if op is "mul" then
        set binary_opcode_opcode to 15
    end
    if op is "div" then
        set binary_opcode_opcode to 16
    end
    if op is "mod" then
        set binary_opcode_opcode to 17
    end
    if op is "eq" then
        set binary_opcode_opcode to 18
    end
    if op is "ne" then
        set binary_opcode_opcode to 19
    end
    if op is "lt" then
        set binary_opcode_opcode to 20
    end
    if op is "le" then
        set binary_opcode_opcode to 21
    end
    if op is "gt" then
        set binary_opcode_opcode to 22
    end
    if op is "ge" then
        set binary_opcode_opcode to 23
    end
    if op is "and" then
        set binary_opcode_opcode to 24
    end
    if op is "or" then
        set binary_opcode_opcode to 25
    end
    if op is "in" then
        set binary_opcode_opcode to 26
    end
    give back binary_opcode_opcode
end

to compile_expr(expr, ctx, line)
    if expr.k is "num" then
        set ctx to emit_const(ctx, 1, number_bytes(expr.v), line)
    end
    if expr.k is "text" then
        set ctx to emit_const(ctx, 3, expr.b, line)
    end
    if expr.k is "yes" then
        set ctx to emit_const(ctx, 2, [1], line)
    end
    if expr.k is "no" then
        set ctx to emit_const(ctx, 2, [0], line)
    end
    if expr.k is "nothing" then
        set ctx to emit_nothing(ctx, line)
    end
    if expr.k is "var" then
        set ctx to emit_name(ctx, 3, expr.n, 0, line)
    end
    if expr.k is "bin" then
        set ctx to compile_expr(expr.l, ctx, line)
        set ctx to compile_expr(expr.r, ctx, line)
        set ctx to emit(ctx, binary_opcode(expr.op), 0, 0, line)
    end
    if expr.k is "un" then
        set ctx to compile_expr(expr.e, ctx, line)
        set compile_expr_opcode to 27
        if expr.op is "not" then
            set compile_expr_opcode to 28
        end
        set ctx to emit(ctx, compile_expr_opcode, 0, 0, line)
    end
    if expr.k is "call" then
        for each each_arg in expr.args
            set ctx to compile_expr(each_arg, ctx, line)
        end
        set ctx to emit_name(ctx, 29, expr.n, length(expr.args), line)
    end
    if expr.k is "mcall" then
        // A receiver that is a plain name is not loaded: `CALL_METHOD` already
        // carries `receiver.method` and resolves it against the receiver's
        // *name*. Loading it would ask for a binding that need not exist —
        // `json.parse` names a module and `json` is never a variable.
        if expr.o.k is not "var" then
            set ctx to compile_expr(expr.o, ctx, line)
        end
        for each each_arg in expr.args
            set ctx to compile_expr(each_arg, ctx, line)
        end
        if expr.o.k is "var" then
            set ctx to emit_name(ctx, 30, cat(cat(expr.o.n, [46]), expr.m), length(expr.args), line)
        else
            set ctx to emit_name(ctx, 30, expr.m, length(expr.args), line)
        end
    end
    if expr.k is "prop" then
        set ctx to compile_expr(expr.o, ctx, line)
        set ctx to emit_name(ctx, 7, expr.p, 0, line)
    end
    if expr.k is "index" then
        set ctx to compile_expr(expr.o, ctx, line)
        set ctx to compile_expr(expr.i, ctx, line)
        set ctx to emit(ctx, 9, 0, 0, line)
    end
    if expr.k is "list" then
        for each each_item in expr.items
            set ctx to compile_expr(each_item, ctx, line)
        end
        set ctx to emit(ctx, 10, length(expr.items), 0, line)
    end
    if expr.k is "record" then
        for each each_field in expr.fields
            set ctx to emit_name(ctx, 1, each_field.k, 0, line)
            set ctx to compile_expr(each_field.v, ctx, line)
        end
        set ctx to emit(ctx, 11, length(expr.fields), 0, line)
    end
    if expr.k is "expect" then
        set ctx to compile_expr(expr.a, ctx, line)
        set ctx to compile_expr(expr.b, ctx, line)
        set ctx to emit(ctx, 45, 0, 0, line)
    end
    give back ctx
end

main()
