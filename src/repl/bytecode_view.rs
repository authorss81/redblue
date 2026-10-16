//! What `.bytecode` and `.asm` show about an expression.
//!
//! Both are a question about code, asked at the prompt because the prompt is
//! where code gets written: *what does this compile to?* The answer is the
//! compiler's own, not a summary of it — the expression goes through the same
//! frontend [`crate::bytecode::compile_source`] that `rb compile` uses and comes
//! out as the same chunk, which is then handed to the same disassembler `rb dis`
//! prints. Read-only: nothing is written to disk, and the session's own VM is
//! never handed the chunk, because a listing is a thing to look at rather than a
//! thing to run.
//!
//! Two renderings of that one chunk, and the difference is what each is for.
//! [`bytecode_lines`] is the listing: what the program is made of, one line per
//! instruction, nothing else. [`asm_lines`] is the annotated form: the same
//! instructions with the constants table above them and a comment on each —
//! which line of source it came from, and what each index names.

use crate::bytecode::Chunk;

/// The lines `.bytecode <code>` prints: a compact listing of the chunk.
///
/// The version line is the compiler's own [`crate::bytecode::FORMAT_VERSION`],
/// so the listing says which format it was written in without anything having to
/// be kept in step with it by hand.
pub fn bytecode_lines(code: &str) -> Result<Vec<String>, String> {
    let chunk = compile(code, "bytecode")?;
    Ok(rendered_lines(&crate::bytecode::disassemble_compact(
        &chunk,
    )))
}

/// The lines `.asm <code>` prints: the annotated disassembly.
///
/// **The same text `rb dis` prints**, byte for byte, for the same source. That
/// is the point of reaching for this rather than for the listing: a reader
/// comparing what the prompt shows against a `.rbc` file on disk is comparing
/// like with like, and there is nothing at the prompt that is a second opinion.
pub fn asm_lines(code: &str) -> Result<Vec<String>, String> {
    let chunk = compile(code, "asm")?;
    Ok(rendered_lines(&crate::bytecode::disassemble(&chunk)))
}

/// Compiles `code` to a chunk, or says why it could not.
///
/// The error is the frontend's own, rendered against the text that was given —
/// with the source line and caret, because `.asm 1 +` is a person mistyping an
/// expression and the line they typed is right there. A listing of nothing would
/// answer "this expression is empty", which is not what went wrong.
fn compile(code: &str, command: &str) -> Result<Chunk, String> {
    if code.trim().is_empty() {
        // Whitespace compiles to an empty program, so this would otherwise print
        // a header and a header-only listing and look like an answer.
        return Err(format!("Error: '{command}' was given nothing to compile."));
    }
    crate::bytecode::compile_source(code).map_err(|error| error.render(code, None))
}

/// The rendered disassembly as lines, without the newline it ends in.
///
/// The renderer ends every block with a newline; the session prints line by
/// line, so keeping it would put a blank line at the end of every answer and
/// make the two renderings differ by a line that is not in either.
fn rendered_lines(rendered: &str) -> Vec<String> {
    rendered.lines().map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::{disassemble, disassemble_compact, FORMAT_VERSION};

    /// The listing of `say 1`: the version, the block, and its three
    /// instructions.
    fn listing(code: &str) -> Vec<String> {
        bytecode_lines(code).expect("the expression should compile")
    }

    #[test]
    fn bytecode_names_the_format_version_and_the_constant_load() {
        let lines = listing("say 1");
        let joined = lines.join("\n");

        assert!(
            lines[0] == format!("; redblue bytecode v{FORMAT_VERSION}"),
            "the listing says which format it was written in, read from the crate \
             rather than written out here: {:?}",
            lines
        );
        assert!(
            joined.contains("PUSH_CONST"),
            "a constant load is in the listing: {}",
            joined
        );
        assert!(joined.contains("SAY"), "and so is the call: {}", joined);
        for line in &lines {
            assert_eq!(
                line.trim_end(),
                line,
                "a listing line ends in its last operand, not in padding: {:?}",
                line
            );
        }
    }

    /// A block nested in another is listed under it, in both renderings: the walk
    /// is the block tree, not the instructions of the top block alone.
    #[test]
    fn bytecode_lists_the_blocks_nested_inside_the_program() {
        let code = "to twice(n)\n    return n * 2\nend\nsay twice(2)\n";

        let compact = listing(code);
        let joined = compact.join("\n");

        assert!(
            joined.contains("main.blocks[0]"),
            "the function's own block is in the listing:\n{}",
            joined
        );
        assert!(
            joined.contains("arity 1"),
            "and says how many arguments it takes:\n{}",
            joined
        );
        assert!(
            compact
                .iter()
                .filter(|line| line.contains("RETURN"))
                .count()
                >= 1,
            "and holds the instructions inside it:\n{}",
            joined
        );
        assert!(
            !joined.contains("; line"),
            "the listing carries no comments at all:\n{}",
            joined
        );
        assert!(
            asm_lines(code)
                .expect("the program compiles")
                .iter()
                .any(|line| line.contains("main.blocks[0]")),
            "the annotated form walks the same tree"
        );
    }

    #[test]
    fn asm_is_longer_than_the_listing_on_the_same_input() {
        let code = "say 1";

        let compact = listing(code);
        let annotated = asm_lines(code).expect("the expression should compile");

        assert!(
            annotated.len() > compact.len(),
            "the annotated form adds the constants table and a comment per \
             instruction: {} compact lines against {} annotated:\n{}",
            compact.len(),
            annotated.len(),
            annotated.join("\n")
        );
        assert!(
            annotated.len() - compact.len() >= 3,
            "and the difference is more than a header line: {} against {}",
            compact.len(),
            annotated.len()
        );
    }

    /// `.asm` is not a rendering of its own: it is what `rb dis` prints for the
    /// same source, and this is the diff that says so.
    #[test]
    fn asm_says_byte_for_byte_what_the_disassembler_says() {
        let code = "set total to 0\nsay total\n";

        let chunk = crate::bytecode::compile_source(code).expect("the program compiles");

        assert_eq!(
            asm_lines(code).expect("the program compiles").join("\n"),
            disassemble(&chunk).trim_end_matches('\n'),
            "`.asm` and `rb dis` render one chunk, so they print one text"
        );
        assert_eq!(
            bytecode_lines(code)
                .expect("the program compiles")
                .join("\n"),
            disassemble_compact(&chunk).trim_end_matches('\n'),
            "and the listing is the compact rendering of that same chunk"
        );
    }

    /// An expression that does not compile is reported, not rendered: the
    /// listing of a program that was never built would look like a program with
    /// nothing in it.
    #[test]
    fn edge_an_expression_that_does_not_compile_is_reported_rather_than_listed() {
        let error = bytecode_lines("might fail 1 +").expect_err("the expression does not compile");

        assert!(
            error.contains("Error") || error.contains("error"),
            "the compile error is what is reported: {}",
            error
        );
        assert!(
            !error.contains("redblue bytecode"),
            "and no listing is printed over the top of it: {}",
            error
        );

        let asm = asm_lines("might fail 1 +").expect_err("the same expression, the same refusal");
        assert_eq!(
            asm, error,
            "both commands report the compile error the frontend gave"
        );
    }

    /// The two commands on nothing at all: an empty expression is a compile
    /// error, not an empty listing and not a panic.
    #[test]
    fn edge_nothing_to_compile_is_a_refusal_and_not_an_empty_listing() {
        let error = bytecode_lines("   ").expect_err("there is no expression here");

        assert!(
            error.contains("bytecode") && error.contains("nothing"),
            "the refusal names the command and what it lacked: {}",
            error
        );
        assert!(!error.is_empty(), "the refusal says something: {}", error);
        assert!(
            !error.contains("redblue bytecode"),
            "and prints no listing: {}",
            error
        );
    }

    /// A program far larger than anything a person types at a prompt still
    /// renders, and renders in full.
    #[test]
    fn edge_a_two_thousand_line_program_renders_without_hanging() {
        let mut source = String::new();
        for index in 0..2_000 {
            source.push_str(&format!("say {index}\n"));
        }

        let compact = bytecode_lines(&source).expect("a large program still compiles");
        let annotated = asm_lines(&source).expect("the same program, annotated");

        assert_eq!(
            compact.len(),
            2 + 3 * 2_000,
            "three instructions per `say`, plus the version and the block: {} lines\n{}",
            compact.len(),
            compact[..4].join("\n")
        );
        assert!(
            annotated.len() > compact.len(),
            "and the annotated form is longer still: {} against {}",
            annotated.len(),
            compact.len()
        );
        assert!(
            compact[compact.len() - 1].contains("SAY"),
            "and the last statement is the last line of the listing: {:?}",
            compact.last()
        );
    }

    /// Constants are rendered as text, and text a terminal would mangle — a NUL,
    /// a newline escape, an emoji, a right-to-left mark, a combining sequence —
    /// is escaped rather than written out raw. Rendering must not panic on any
    /// of them, and must not be able to fail to terminate.
    #[test]
    fn edge_text_a_terminal_would_mangle_is_escaped_rather_than_written_out() {
        let code = "say \"a\\u0000b \\n c \\u202E d \\u00E9\\t🎉\"";

        let compact = bytecode_lines(code).expect("a string constant compiles");
        let annotated = asm_lines(code).expect("the same constant, annotated");

        assert!(
            !compact.is_empty() && !annotated.is_empty(),
            "both renderings produce output: {:?} / {:?}",
            compact,
            annotated
        );
        assert!(
            annotated.join("\n").contains("\\u{202e}"),
            "the right-to-left mark is escaped rather than emitted raw:\n{}",
            annotated.join("\n").escape_debug()
        );
        for rendered in [&annotated, &compact] {
            assert!(
                !rendered.join("\n").contains('\u{202E}') && !rendered.join("\n").contains('\0'),
                "no raw direction or NUL character reaches the terminal in either \
                 rendering:\n{}",
                rendered.join("\n").escape_debug()
            );
        }
        assert!(
            annotated.join("\n").contains('\u{e9}') || annotated.join("\n").contains("\\u{e9}"),
            "a combining acute accent is escaped rather than composed away:\n{}",
            annotated.join("\n").escape_debug()
        );
    }
}
