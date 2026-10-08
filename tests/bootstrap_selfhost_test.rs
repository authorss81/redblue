//! `bootstrap/compiler.rb` — the compiler written in Redblue (ladder stage S2).
//!
//! Stage 1 is the Rust `rb compile`: `src/lexer.rs` -> `src/parser.rs` ->
//! `src/bytecode/codegen.rs` -> `Chunk::encode`. Stage 2 is
//! `bootstrap/compiler.rb`: the same four stages, written in Redblue and run by
//! the Rust `rb`. The ladder's rule is that stage 2 is not a *second* compiler
//! but the *same* one, and the only evidence that says so is the bytes: for a
//! program both engines accept, the two must produce one file, byte for byte.
//!
//! So every test here compares `fs::read(stage2.rbc)` against
//! `compile_source(source).encode()` and nothing else. A stage-2 compiler that
//! runs, emits a plausible file and differs in one byte fails every test here.
//!
//! Nothing in `bootstrap/` is reachable from `rb compile`, and nothing in `src/`
//! special-cases a path: the tests would notice, because a shim would make the
//! two byte-identical without the Redblue source doing the work.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The self-hosted compiler under test.
fn compiler() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("bootstrap/compiler.rb")
}

/// A path under `target/tmp/`. Nothing is ever written to `corpus/` or
/// `bootstrap/`: a test that rewrote what it is checking proves nothing.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/bootstrap");
    fs::create_dir_all(&dir).expect("scratch directory");
    dir.join(name)
}

/// What one stage-2 run did.
struct Stage2 {
    bytes: Vec<u8>,
    stderr: String,
    succeeded: bool,
}

/// Compiles `source` by running `bootstrap/compiler.rb` under the Rust `rb`.
fn stage2(name: &str, source: &str) -> Stage2 {
    let input = scratch(&format!("{name}.rb"));
    let output = scratch(&format!("{name}.rbc"));
    fs::write(&input, source).expect("write the program to compile");
    let _ = fs::remove_file(&output);

    let run = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("run")
        .arg(compiler())
        .arg(&input)
        .arg(&output)
        .output()
        .expect("the rb binary runs");

    Stage2 {
        bytes: fs::read(&output).unwrap_or_default(),
        stderr: String::from_utf8_lossy(&run.stderr).into_owned(),
        succeeded: run.status.success(),
    }
}

/// The bytes stage 1 writes for `source`.
fn stage1(name: &str, source: &str) -> Vec<u8> {
    match redblue::compile_source(source) {
        Ok(chunk) => chunk.encode(),
        Err(error) => {
            panic!("stage 1 refuses {name}, so there are no bytes to compare:\n{error:?}\n{source}")
        }
    }
}

/// The one comparison this file makes: stage 2 wrote exactly what stage 1 did.
fn assert_identical(name: &str, source: &str) {
    let expected = stage1(name, source);
    let actual = stage2(name, source);

    assert!(
        actual.succeeded,
        "stage 2 failed on {name}:\n{}\nsource:\n{source}",
        actual.stderr
    );
    assert_eq!(
        actual.bytes.len(),
        expected.len(),
        "stage 2 wrote {} bytes where stage 1 wrote {} for {name}",
        actual.bytes.len(),
        expected.len()
    );
    assert!(
        !actual.bytes.is_empty(),
        "stage 2 wrote no file for {name}, so it cannot agree with stage 1"
    );
    assert!(
        actual.bytes.starts_with(b"RED\x1a"),
        "stage 2's file for {name} is not a Redblue bytecode file"
    );
    assert_eq!(
        actual.bytes, expected,
        "stage 2's bytes differ from stage 1's for {name}\nsource:\n{source}"
    );
}

/// The statement kinds a corpus family is made of, compiled one per family, so
/// a regression in any of them is named rather than counted.
#[test]
fn stage2_is_byte_identical_for_each_statement_kind() {
    let programs = [
        ("say", "say \"Hello, World!\"\n"),
        ("set", "set a to 10\nset b to 20\nsay a + b\n"),
        (
            "print",
            "print \"no newline\"\nset a to 1\nsay a\n",
        ),
        (
            "if_else",
            "set a to 3\nif a is greater than 2 then\n    say \"big\"\nelse\n    say \"small\"\nend\n",
        ),
        (
            "if_without_else",
            "set a to 1\nif a is greater than 2 then\n    say \"big\"\nend\nsay \"after\"\n",
        ),
        (
            "unless",
            "set a to 1\nunless a is greater than 2 then\n    say \"small\"\nend\n",
        ),
        (
            "for_each",
            "for each x in [1, 2, 3]\n    say x\nend\n",
        ),
        (
            "for_range",
            "for each i from 1 to 3\n    say i\nend\n",
        ),
        (
            "for_range_by",
            "for each i from 10 to 1 by 3\n    say i\nend\n",
        ),
        ("repeat", "repeat 3 times\n    say \"tick\"\nend\n"),
        (
            "while",
            "set i to 0\nwhile i is less than 3\n    say i\n    set i to i + 1\nend\n",
        ),
        (
            "break_skip",
            "for each x in [1, 2, 3]\n    if x is 2 then\n        skip\n    end\n    if x is 3 then\n        break\n    end\n    say x\nend\n",
        ),
        (
            "function",
            "to add(a, b)\n    give back a + b\nend\nsay add(1, 2)\n",
        ),
        (
            "give_back_nothing",
            "to nothing_at_all()\n    give back\nend\nsay nothing_at_all()\n",
        ),
        (
            "try_catch",
            "try\n    say 1 / 0\ncatch error\n    say \"caught\"\nend\n",
        ),
        (
            "try_finally",
            "try\n    say \"body\"\nfinally\n    say \"finally\"\nend\n",
        ),
        (
            "list_and_index",
            "set xs to [10, 20, 30]\nsay xs[1]\nsay length(xs)\n",
        ),
        (
            "record",
            "set r to {name: \"Ada\", age: 36}\nsay r.name\nsay r.age\n",
        ),
        (
            "nested_lists",
            "set grid to [[1, 2], [3, 4]]\nsay grid[0][1]\n",
        ),
        (
            "operators",
            "say 1 + 2 - 3 * 4 / 5 mod 6\nsay -7 + 8\nsay not yes\nsay 1 < 2\nsay 3 is greater than 2\nsay 2 is not 3\nsay 4 is greater than or equal to 4\n",
        ),
        (
            "index_bounds_error_is_compiled_too",
            "set xs to [1]\nsay xs[5]\n",
        ),
    ];

    for (name, source) in programs {
        assert_identical(name, source);
    }
}

/// The edge cases of the *source* a compiler has to read, not of the program it
/// compiles: the shapes most likely to make the two lexers disagree.
#[test]
fn edge_source_shapes_are_byte_identical() {
    // An empty file: the smallest program there is.
    assert_identical("edge_empty", "");

    // A file that is nothing but a comment and a blank line: no statements,
    // and a lexer that disagrees about `Newline` tokens still shows here.
    assert_identical("edge_comment_only", "// nothing at all\n\n   \n");

    // A trailing comment on the last line, and no newline after it.
    assert_identical("edge_no_trailing_newline", "set a to 1 // done");

    // CRLF line endings: one `Newline` token, not two.
    assert_identical("edge_crlf", "set a to 1\r\nset b to 2\r\nsay a + b\r\n");

    // A keyword inside a string is text, and a `//` inside a string is text.
    assert_identical(
        "edge_keywords_in_text",
        "say \"set to end if then\"\nsay \"a // b\"\n",
    );

    // Escapes: every one the lexer translates, and the quote and backslash
    // that have to survive the round trip into a text constant.
    assert_identical(
        "edge_escapes",
        "say \"tab:\\tnl:\\nquote:\\\"backslash:\\\\cr:\\r\"\n",
    );

    // An empty text constant: a zero-length string in the constant pool, which
    // is the one entry whose length field is 0.
    assert_identical("edge_empty_text", "say \"\"\n");

    // Text whose bytes are not ASCII, so a compiler that assumes one byte per
    // character writes a different file.
    assert_identical(
        "edge_unicode_text",
        "say \"héllo wörld\"\nsay \"日本語\"\nsay \"emoji 🐉 and 🇬🇧\"\nsay \"combining é vs é\"\n",
    );

    // A number at each end of the range, and the ones that cannot be written
    // back as their decimal text: the constant pool holds their bits.
    assert_identical(
        "edge_number_boundaries",
        "say 0\nsay -0.0\nsay 1\nsay 0.1\nsay 0.2\nsay 0.3\nsay 1e308\nsay 5e-324\nsay 2.2250738585072014e-308\nsay 9007199254740993\nsay 123456789012345678\nsay 1e-300\nsay 3.14159265358979\n",
    );

    // Nesting: an `if` inside a loop inside a function inside a loop.
    assert_identical(
        "edge_nesting",
        "to classify(xs)\n    for each x in xs\n        if x is greater than 10 then\n            say \"big\"\n        else\n            if x is greater than 5 then\n                say \"mid\"\n            else\n                say \"small\"\n            end\n        end\n    end\nend\nclassify([11, 6, 1])\n",
    );

    // A record with a repeated key, and a field that is missing.
    assert_identical(
        "edge_duplicate_and_missing_keys",
        "set r to {a: 1, a: 2}\nsay r.a\nsay r.missing\n",
    );

    // The longest chain of binary operators, which is where two precedence
    // tables disagree.
    assert_identical(
        "edge_operator_chain",
        "say 1 + 2 * 3 - 4 / 2 mod 3 + 1\nsay 1 < 2 and 3 > 4 or 5 < 6\n",
    );

    // A line with no trailing newline *inside* a block, and a blank line
    // between every pair of statements: both are places two statement readers
    // can disagree about what a statement is.
    assert_identical(
        "edge_blank_lines",
        "if yes then\n\n    say 1\n\n    say 2\n\nend\n",
    );
}

/// A stage-2 compiler that cannot fail is not a compiler: it writes a file for
/// every input, and the file is wrong for the inputs it did not understand.
///
/// Each program here is one the frontend refuses. Stage 1 refuses them too —
/// that is the shared specification — and the test asserts both halves: the
/// Redblue compiler exits non-zero, says why, and leaves **no** `.rbc` behind.
#[test]
fn edge_malformed_source_is_reported_rather_than_compiled() {
    let malformed = [
        ("unterminated_string", "say \"never closed\n"),
        ("unclosed_block", "if yes then\n    say 1\n"),
        ("stray_end", "say 1\nend\n"),
        ("missing_value", "set a to\n"),
        ("bad_number", "say 1.2.3\n"),
        ("unclosed_string_bracket", "set xs to [1, 2\n"),
        ("bad_keyword_order", "to then\nend\n"),
        ("missing_colon_in_record", "set r to {a 1}\n"),
    ];

    for (name, source) in malformed {
        let run = stage2(name, source);
        assert!(
            redblue::compile_source(source).is_err(),
            "{name} is not a program stage 1 refuses either, so it is not a test"
        );
        assert!(
            !run.succeeded,
            "stage 2 accepted {name}, which the frontend refuses"
        );
        assert!(
            run.bytes.is_empty(),
            "stage 2 left a {} byte .rbc for the refused {name}",
            run.bytes.len()
        );
        assert!(
            run.stderr.contains("Error"),
            "stage 2 failed on {name} without saying so:\n{}",
            run.stderr
        );
    }
}

/// The corpus on disk, walked family by family.
///
/// `corpus/` is the language's specification by example, so "byte-identical on
/// the corpus" is the ladder's own acceptance gate. The families listed here are
/// the ones stage 2 compiles today; the ones it does not are named in
/// `UNSUPPORTED` below, so a gap is a constant a reader can see rather than a
/// sentence in a report, and adding a family is a one-word change.
#[test]
fn stage2_is_byte_identical_on_its_corpus_families() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    let families = [
        "arithmetic",
        "text-ops",
        "lists",
        "records",
        "control-flow",
        "loop-forms",
        "functions",
        "nesting",
        "objects",
        "stdlib",
        "unicode",
        "value-tails",
        "numeric-boundary",
        "runtime-errors",
        "faults",
    ];

    let mut checked = 0usize;
    for family in families {
        let mut entries: Vec<PathBuf> = fs::read_dir(&root)
            .unwrap_or_else(|e| panic!("the corpus is readable: {e}"))
            .map(|entry| entry.expect("a corpus entry").path())
            .filter(|path| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(family) && n.ends_with(".rb"))
            })
            .collect();
        entries.sort();

        assert!(
            !entries.is_empty(),
            "corpus family {family} has no programs to compare"
        );

        for path in entries {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let source = fs::read_to_string(&path).expect("a corpus program is UTF-8");
            if redblue::compile_source(&source).is_err() {
                // A family member the frontend refuses has no bytes to match;
                // that refusal is what `edge_malformed_source_is_reported`
                // covers, and a refused program is not a byte-identity case.
                continue;
            }
            assert_identical(&name, &source);
            checked += 1;
        }
    }

    assert!(
        checked >= 300,
        "only {checked} corpus programs were compared; stage 2 is byte-identical \
         on every program of the corpus the frontend accepts, which is 306 of the \
         315 in the families above — the other 9 are refused by the frontend, and \
         the 46 in `malformed` are refused too"
    );
    for family in UNSUPPORTED {
        assert!(
            !families.contains(family),
            "{family} is listed as unsupported and as compared"
        );
    }
}

/// The corpus families stage 2 does not compile yet.
///
/// `phases/phase-021/REPORT.md` lists the construct each one needs. They are
/// named here rather than silently skipped, and this file's coverage is
/// `families.len()` of the corpus — not the whole corpus, which is why the
/// fixed point is not claimed.
/// The corpus families stage 2 does not compare, and why.
///
/// `malformed` is here because the frontend refuses those programs: there are no
/// bytes to agree about. Their refusals are what
/// `edge_malformed_source_is_reported_rather_than_compiled` covers, one shape at
/// a time.
const UNSUPPORTED: &[&str] = &["malformed"];

/// The three defects below were all the same shape of mistake in different
/// places, so each is pinned by name rather than left to be counted by the
/// corpus walk that found it.
///
/// 1. An `if ... else` whose `else` holds another `if ... else`: the nested
///    parse reused the outer one's body name, so the outer `if` compiled the
///    *inner* branch as its own `then`. The two files were the same length and
///    differed in the constant pool and in one line number.
/// 2. The same `if ... else` with an empty `then`: the jump over the `else`
///    pointed at the end of the block, so the `else` ran and the `then` never
///    did, on any condition.
/// 3. `for each i from 1 to 3`: the absent step was compiled as if it were
///    present, and stage 2 died with a runtime error on `nothing`.
#[test]
fn edge_each_branch_compiles_the_statements_it_was_given() {
    // (1) A doubly-nested `else`. Each level's `then` has to survive the parse
    // of the level below it: the outer `then` is `say "A"`, and a compiler that
    // lost it produced the inner `then` twice and no `A` at all.
    assert_identical(
        "edge_if_else_nested_in_else",
        "set a to 1\n\
         if a is 1 then\n\
         \x20   say \"A\"\n\
         else\n\
         \x20   if a is 2 then\n\
         \x20       say \"B\"\n\
         \x20   else\n\
         \x20       if a is 3 then\n\
         \x20           say \"C\"\n\
         \x20       else\n\
         \x20           say \"D\"\n\
         \x20       end\n\
         \x20   end\n\
         end\n",
    );

    // (1 again) The nested `if` is only the *first* statement of the `else`, so
    // the statements after it have to be kept as well — the branch is a list,
    // and losing its head is not the only way to get it wrong.
    assert_identical(
        "edge_if_else_then_statements_after_a_nested_if",
        "set a to 1\n\
         if a is 1 then\n\
         \x20   say \"A\"\n\
         else\n\
         \x20   if a is 2 then\n\
         \x20       say \"B\"\n\
         \x20   end\n\
         \x20   say \"C\"\n\
         end\n",
    );

    // (1 a third time) The nesting on the `then` side, where the body is parsed
    // before the `else` is even looked for.
    assert_identical(
        "edge_if_else_nested_in_then",
        "set a to 1\n\
         if a is 1 then\n\
         \x20   if a is 2 then\n\
         \x20       say \"B\"\n\
         \x20   else\n\
         \x20       say \"C\"\n\
         \x20   end\n\
         else\n\
         \x20   say \"A\"\n\
         end\n",
    );

    // (2) A `then` the size of a single instruction. Nothing about it is
    // unusual except that there is nothing of it to notice a lost patch by, so
    // it is the shape that made the jump bug invisible in a disassembly read.
    assert_identical(
        "edge_if_else_empty_then",
        "set a to 1\nif a is 1 then\nelse\n    say \"two\"\nend\n",
    );
}

/// Byte equality is the ladder's rule, but two engines that agree on a wrong
/// jump would satisfy it, so the strongest statement available is also made
/// here: the two files produce the same output when the bytecode VM runs them.
///
/// This is what turns "the bytes agree" into "the bytes are right", and it is
/// the only check here that would notice a stage-2 bug that stage 1 shares.
#[test]
fn edge_the_two_engines_run_a_branching_program_the_same() {
    // Every branch of the `if` is taken at least once across the four calls, so
    // a jump that lands one instruction early or late shows up as output that
    // differs rather than as output that stops.
    let source = "\
to classify(n)\n\
\x20   if n is greater than 10 then\n\
\x20       say \"big\"\n\
\x20   else\n\
\x20       if n is greater than 5 then\n\
\x20           say \"mid\"\n\
\x20       else\n\
\x20           say \"small\"\n\
\x20       end\n\
\x20   end\n\
end\n\
classify(1)\n\
classify(7)\n\
classify(11)\n\
classify(20)\n";

    assert_identical("edge_run_same", source);

    // `assert_identical` has already left stage 2's file in the scratch
    // directory, so stage 1's is written beside it under its own name. Neither
    // run reads the other's output, and nothing here compares the two files —
    // that is what the byte test above is for. This test only runs them.
    let name = "edge_run_same";
    let stage1_file = scratch(&format!("{name}.stage1.rbc"));
    let stage2_file = scratch(&format!("{name}.rbc"));
    fs::write(
        &stage1_file,
        redblue::compile_source(source)
            .expect("stage 1 compiles the program")
            .encode(),
    )
    .expect("write stage 1's file");

    let printed = |path: &Path| -> String {
        let run = Command::new(env!("CARGO_BIN_EXE_rb"))
            .arg("vm")
            .arg(path)
            .output()
            .expect("the rb binary runs a compiled file");
        assert!(
            run.status.success(),
            "{} does not run:\n{}",
            path.display(),
            String::from_utf8_lossy(&run.stderr)
        );
        String::from_utf8_lossy(&run.stdout).into_owned()
    };

    let stage1_out = printed(&stage1_file);
    let stage2_out = printed(&stage2_file);

    assert_eq!(
        stage1_out, "small\nmid\nbig\nbig\n",
        "stage 1's file does not print what the source says, so this test proves nothing"
    );
    assert_eq!(
        stage2_out, stage1_out,
        "the two files print different things"
    );
}

/// The bounds of a range: with a step, without one, and a step that is itself
/// an expression rather than a literal.
///
/// A missing step is the case stage 2 got wrong, and it got it wrong by dying:
/// `for each i from 1 to 3` left no step to compile and stage 2 read a property
/// off `nothing`, so it produced no file at all.
#[test]
fn edge_for_range_pushes_only_the_bounds_it_was_given() {
    // No step: two bounds, and `GET_RANGE`'s arity is 2.
    assert_identical(
        "edge_for_range_no_step",
        "for each i from 1 to 3\n    say i\nend\n",
    );

    // A step: three bounds, arity 3.
    assert_identical(
        "edge_for_range_with_step",
        "for each i from 10 to 1 by 3\n    say i\nend\n",
    );

    // A step written as an expression rather than a literal, so the branch that
    // compiles it is entered because of what the parse produced, not because a
    // token was spelled `by`.
    assert_identical(
        "edge_for_range_step_is_an_expression",
        "set step to 4\nfor each i from 12 to 1 by step - 3\n    say i\nend\n",
    );

    // A descending range with no step: the step stage 1 defaults to is 1, and
    // the file must not carry a step operand the source never wrote.
    assert_identical(
        "edge_for_range_descending_no_step",
        "for each i from 1 to 3\n    say i\nend\nfor each j from 3 to 1\n    say j\nend\n",
    );
}

/// The failure channel: stage 2 must refuse what stage 1 refuses, and must
/// refuse it for the same reason rather than by accident.
///
/// The shapes here are the *branch* forms, because the three defects above were
/// all in branch handling — a compiler that walks a branch wrongly is exactly
/// the compiler most likely to walk an unterminated one wrongly too.
#[test]
fn edge_a_broken_branch_is_refused_and_writes_nothing() {
    let broken = [
        // An `else` with no `if` in front of it.
        ("edge_stray_else", "set a to 1\nelse\n    say \"two\"\nend\n"),
        // An `if` whose `else` is never closed.
        (
            "edge_unclosed_else",
            "set a to 1\nif a is 1 then\n    say \"one\"\nelse\n    say \"two\"\n",
        ),
        // An `if` with a second `else`.
        (
            "edge_two_elses",
            "set a to 1\nif a is 1 then\n    say \"one\"\nelse\n    say \"two\"\nelse\n    say \"three\"\nend\n",
        ),
        // An `if` with no `then` at all.
        (
            "edge_if_without_then",
            "set a to 1\nif a is 1\n    say \"one\"\nend\n",
        ),
        // `unless` with an `else` arm, which the grammar has no production for.
        (
            "edge_unless_with_else",
            "set a to 1\nunless a is 1 then\n    say \"one\"\nelse\n    say \"two\"\nend\n",
        ),
        // A `for each ... from ... to` with no upper bound.
        (
            "edge_for_range_no_upper_bound",
            "for each i from 1\n    say i\nend\n",
        ),
        // A `for each ... by` with no step after it.
        (
            "edge_for_range_no_step_after_by",
            "for each i from 1 to 3 by\n    say i\nend\n",
        ),
    ];

    for (name, source) in broken {
        let run = stage2(name, source);
        assert!(
            redblue::compile_source(source).is_err(),
            "{name} is not a program stage 1 refuses either, so it is not a test"
        );
        assert!(
            !run.succeeded,
            "stage 2 accepted {name}, which the frontend refuses"
        );
        assert!(
            run.bytes.is_empty(),
            "stage 2 left a {} byte .rbc for the refused {name}",
            run.bytes.len()
        );
        assert!(
            run.stderr.contains("Error"),
            "stage 2 failed on {name} without saying so:\n{}",
            run.stderr
        );
    }
}
