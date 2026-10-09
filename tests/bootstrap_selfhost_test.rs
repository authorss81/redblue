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
use std::io;
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

/// Reads a stage-2 output file, treating *absent* as "wrote nothing" and every
/// other failure as the failure it is.
///
/// A stage 2 that refuses a program writes no file, and "no file" is a result
/// several tests here assert. But that is the *only* absence these runs are
/// allowed to have: `unwrap_or_default()` also turned an unreadable path — a
/// directory where the file should be, a permissions failure, a full disk — into
/// the same empty `Vec`, so a run whose output could not be read at all was
/// reported as a compile that wrote nothing. That is the wrong diagnosis for the
/// wrong cause, and on the byte-equality tests it hid behind a length mismatch
/// rather than naming the IO error.
///
/// So: `NotFound` is the one answer that means "wrote nothing", and everything
/// else stops the test by naming the file and the cause.
fn read_output(path: &Path) -> Vec<u8> {
    match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => panic!(
            "stage 2's output {} could not be read, so there is nothing to compare: {error}",
            path.display()
        ),
    }
}

/// Removes a stale output file, so a run that writes nothing cannot be read as a
/// run that wrote something.
///
/// This was `let _ = fs::remove_file(...)`, which discards every failure and not
/// only the expected one. Several tests below assert "stage 2 wrote no file",
/// and if the stale file could not be removed then the assertion would be
/// reading the previous run's bytes and reporting them as this one's.
///
/// `NotFound` is the expected answer — there is nothing stale. Anything else is
/// stopped here, by name and by cause.
fn clear_output(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => panic!(
            "the stale output {} could not be removed, so a run that writes nothing \
             would be read as a run that did: {error}",
            path.display()
        ),
    }
}

/// Compiles `source` by running `bootstrap/compiler.rb` under the Rust `rb`.
fn stage2(name: &str, source: &str) -> Stage2 {
    let input = scratch(&format!("{name}.rb"));
    let output = scratch(&format!("{name}.rbc"));
    fs::write(&input, source).expect("write the program to compile");
    clear_output(&output);

    let run = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("run")
        .arg(compiler())
        .arg(&input)
        .arg(&output)
        .output()
        .expect("the rb binary runs");

    Stage2 {
        bytes: read_output(&output),
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
    let mut refused = 0usize;
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
                refused += 1;
                continue;
            }
            assert_identical(&name, &source);
            checked += 1;
        }
    }

    // Exact counts, not floors. A floor is a gate that decays: `checked >= 300`
    // still passes with six corpus programs deleted, which is the whole claim
    // this test makes — that stage 2 is byte-identical on *every* program the
    // frontend accepts. And a total that is never asserted is how six files go
    // missing without anything noticing, because `refused == 9` and
    // `checked >= 300` are both still true afterwards.
    assert_eq!(
        checked, 306,
        "only {checked} corpus programs were compared; stage 2 is byte-identical on \
         every program of the corpus the frontend accepts, which is 306 of the 315 in \
         the families above — the other 9 are refused by the frontend, and the 46 in \
         `malformed` are refused too"
    );
    assert_eq!(
        checked + refused,
        315,
        "the 15 families above hold 315 programs between them; this walk saw \
         {checked} compared and {refused} refused, which is not all of them — a file \
         has been added or removed and the counts below still hold"
    );
    assert_eq!(
        refused, 9,
        "{refused} corpus programs in the families above are refused by the frontend; \
         there were 9 when this test was written, and each is listed in \
         phases/phase-021/FINDINGS.md section 2"
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
/// `malformed` is here because the frontend refuses those programs: there are no
/// bytes to agree about. Their refusals are what
/// `edge_malformed_source_is_reported_rather_than_compiled` covers, one shape at a
/// time.
const UNSUPPORTED: &[&str] = &["malformed"];

/// `examples/*.rb` and `modules/*.rb` are the language's specification by
/// example, so stage 2 has to agree with stage 1 about every one of them.
///
/// They are whole programs that use the standard library — `files`, `time`,
/// `formats`, a module import — rather than one construct each, and they are the
/// only Redblue source in the repository that is both large and written by
/// somebody who was not thinking about this compiler. Until this test existed the
/// claim was made in a report and checked by hand; now the gate checks it.
#[test]
fn stage2_is_byte_identical_on_the_examples_and_the_modules() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut checked = 0usize;

    for directory in ["examples", "modules"] {
        let dir = root.join(directory);
        let mut entries: Vec<PathBuf> = fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{directory}/ is readable: {e}"))
            .map(|entry| entry.expect("a directory entry").path())
            .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("rb"))
            .collect();
        entries.sort();

        assert!(
            !entries.is_empty(),
            "{directory}/ has no .rb programs, so the specification by example is not being checked"
        );

        for path in entries {
            let relative = path
                .strip_prefix(env!("CARGO_MANIFEST_DIR"))
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let source = fs::read_to_string(&path).expect("an example is UTF-8");
            assert!(
                redblue::compile_source(&source).is_ok(),
                "stage 1 refuses {relative}, so there are no bytes to compare"
            );
            // Flat, because a `name` carrying the directory would name a
            // subdirectory of the scratch directory that nothing creates.
            let name = format!(
                "{directory}-{}",
                path.file_stem().unwrap().to_string_lossy()
            );
            assert_identical(&name, &source);
            checked += 1;
        }
    }

    assert_eq!(
        checked, 9,
        "{checked} of the examples and modules were compared, not all 9; the language's \
         specification by example is what this test exists to check"
    );
}

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

/// Runs `rb vm <file.rbc> [args...]` and reports what it did.
struct VmRun {
    stdout: String,
    stderr: String,
    succeeded: bool,
}

/// `rb vm` over a compiled program, with the arguments after the path handed to
/// the program itself.
fn vm(args: &[&str]) -> VmRun {
    let run = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("vm")
        .args(args)
        .output()
        .expect("the rb binary runs");
    VmRun {
        stdout: String::from_utf8_lossy(&run.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&run.stderr).into_owned(),
        succeeded: run.status.success(),
    }
}

/// Compiles `source` with stage 1 and returns the `.rbc` path, so a test can
/// hand a real bytecode file to `rb vm`.
fn stage1_file(name: &str, source: &str) -> PathBuf {
    let path = scratch(&format!("{name}.rbc"));
    fs::write(&path, stage1(name, source)).expect("stage 1 writes its .rbc");
    path
}

/// The next ladder rung is `rb vm stage1.rbc in.rb out.rbc`, and the only thing
/// standing in front of it is that `rb vm` took no arguments: `rb run` hands
/// everything after the path to `sys.argv()`, and `rb vm` fell through to the
/// usage arm instead. So the arguments a program run as bytecode could not be
/// read were dropped on the floor, and `rb vm` reported success having run
/// nothing at all.
///
/// This pins the argument channel and nothing else. It does not claim S3: the
/// program below is small enough to compile in test time, and it is the *shape*
/// of the S3 invocation, not the self-compilation that rung would need.
#[test]
fn edge_the_bytecode_vm_hands_arguments_to_the_program_it_runs() {
    // A program that reports exactly what it was given, so the assertion is
    // about the channel and not about the bytecode VM's own output. It reads
    // index 0 only when the list is non-empty, so the empty case is a report
    // rather than an out-of-bounds error.
    let source = "set vm_args to sys.argv()\n\
                  say length(vm_args)\n\
                  if length(vm_args) is 0 then\n\
                  \x20   say \"no arguments\"\n\
                  else\n\
                  \x20   say vm_args[0]\n\
                  end\n";
    let program = stage1_file("edge_vm_args", source);
    let path = program.to_string_lossy().into_owned();

    // Empty: a program that asks for nothing is unaffected by the arm that
    // hands arguments over — the boundary between "no arguments" and "some".
    let none = vm(&[&path]);
    assert!(
        none.succeeded,
        "rb vm on a program that asks for no arguments failed:\n{}",
        none.stderr
    );
    assert_eq!(
        none.stdout, "0\nno arguments\n",
        "a program run with no arguments should see an empty list"
    );

    // Singleton: exactly one argument, which is the shape `rb run` already had
    // and `rb vm` did not.
    let one = vm(&[&path, "first"]);
    assert!(
        one.succeeded,
        "rb vm dropped the program's own arguments and failed:\n{}",
        one.stderr
    );
    assert_eq!(
        one.stdout, "1\nfirst\n",
        "rb vm did not hand 'first' to the program; it ran it with no arguments"
    );

    // More than one, so a partial read — taking only the first — is caught too.
    let three = vm(&[&path, "in.rb", "out.rbc", "extra"]);
    assert!(
        three.succeeded,
        "rb vm failed with three arguments:\n{}",
        three.stderr
    );
    assert_eq!(
        three.stdout, "3\nin.rb\n",
        "rb vm did not hand every argument after the path to the program"
    );
}

/// `rb vm` reaching the usage arm is a refusal, and a refusal has to be visible:
/// it printed the help text and exited **0**, so a shell checking `$?` could
/// not tell "ran the program" from "understood nothing of what you asked".
#[test]
fn edge_the_bytecode_vm_reports_a_bad_invocation_as_a_failure() {
    // A path with no `.rbc` extension: `rb vm` refuses it by extension, so the
    // refusal has to arrive before anything is read.
    let not_bytecode = vm(&["edge_vm_not_a_file.txt"]);
    assert!(
        !not_bytecode.succeeded,
        "rb vm on a file that is not bytecode exited 0"
    );
    assert!(
        not_bytecode.stderr.contains("not a bytecode file"),
        "rb vm did not say why it refused:\n{}",
        not_bytecode.stderr
    );

    // A missing file: an `IoError` has to reach the shell as a non-zero status.
    let missing = vm(&["edge_vm_absent.rbc"]);
    assert!(
        !missing.succeeded,
        "rb vm on a missing file exited 0, having printed:\n{}",
        missing.stdout
    );
    assert!(
        missing.stderr.contains("Error"),
        "rb vm on a missing file did not report the failure:\n{}",
        missing.stderr
    );

    // Four arguments. This is the invocation S3 needs — `rb vm stage1.rbc in.rb
    // out.rbc` — and it matched neither the `compile -o` arm nor the `run` arm
    // nor the bare-`4` arm, so it fell through to `_ => print_help()`, which
    // returns normally. It printed the help text and exited **0**: a shell
    // could not tell it had done nothing.
    let too_many = vm(&["edge_vm_extra.rbc", "in.rb", "out.rbc", "four"]);
    assert!(
        !too_many.succeeded,
        "rb vm with arguments exited 0, having printed:\n{}",
        too_many.stdout
    );
}

/// Ladder stage **S3**: the compiler, compiled by itself, still emits the same
/// bytes.
///
/// `stage1.rbc` is `bootstrap/compiler.rb` compiled by stage 1. Running that
/// `.rbc` under `rb vm` makes it stage 2 the long way round — no Rust fast path,
/// no tree-walker, nothing but the bytecode VM executing Redblue that compiles
/// Redblue. Its output for a program must equal what stage 1 wrote for the same
/// program, byte for byte.
///
/// This could not run at all before `push_loop` recorded a loop's `stack_base`
/// one below the height its body ran at: leaving a `for each` truncated a value
/// the enclosing frame had pushed, so `apply_patches` — a `while` containing a
/// `for each`, with a call after them — lost an operand and died with `bytecode
/// asked for 2 values its frame never pushed`.
///
/// The corpus here is a *sample* of each family, so that the shapes a stage-2
/// run is most likely to get wrong are named one by one.
/// `edge_stage3_is_byte_identical_on_every_corpus_program` is the same comparison
/// over all 306, and `edge_stage3_is_byte_identical_on_the_awkward_shapes` is it
/// over the statement shapes the corpus families do not all reach.
///
/// `name` is the scratch file's stem, so it must be flat — a name carrying the
/// `corpus/` prefix would name a directory under `target/tmp/` that nothing
/// creates, and stage 2 would fail for the wrong reason.
fn stage3(name: &str, path: &Path) -> Stage2 {
    let output = scratch(&format!("stage3-{name}.rbc"));
    clear_output(&output);

    let run = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("vm")
        .arg(stage1_of_the_compiler())
        .arg(path)
        .arg(&output)
        .output()
        .expect("the rb binary runs");

    Stage2 {
        bytes: read_output(&output),
        stderr: String::from_utf8_lossy(&run.stderr).into_owned(),
        succeeded: run.status.success(),
    }
}

/// `bootstrap/compiler.rb` compiled by stage 1 — the self-hosted compiler, built
/// once per test process so the S3 tests do not each pay for it.
fn stage1_of_the_compiler() -> PathBuf {
    use std::sync::OnceLock;
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let path = scratch("stage1.rbc");
        let bytes = stage1("bootstrap/compiler.rb", &compiler_contents());
        fs::write(&path, bytes).expect("stage 1 writes the compiler's .rbc");
        path
    })
    .clone()
}

/// The source of `bootstrap/compiler.rb`.
fn compiler_contents() -> String {
    fs::read_to_string(compiler()).expect("bootstrap/compiler.rb is UTF-8")
}

/// One program per corpus family, named. A fixed sample rather than a slice, so
/// a change to which files exist cannot silently change what this covers.
///
/// `-0001` is the first member of every family — the corpus counts from 1, so a
/// `-0000` name names nothing. The assertion below is what says so: a sample
/// naming a file that is not there fails by naming it, rather than passing on a
/// family it never looked at.
const STAGE3_SAMPLE: &[(&str, &str)] = &[
    ("arithmetic", "corpus/arithmetic-0001.rb"),
    ("control-flow", "corpus/control-flow-0001.rb"),
    ("functions", "corpus/functions-0001.rb"),
    ("lists", "corpus/lists-0001.rb"),
    ("loop-forms", "corpus/loop-forms-0001.rb"),
    ("nesting", "corpus/nesting-0001.rb"),
    ("numeric-boundary", "corpus/numeric-boundary-0001.rb"),
    ("objects", "corpus/objects-0001.rb"),
    ("records", "corpus/records-0001.rb"),
    ("stdlib", "corpus/stdlib-0001.rb"),
    ("text-ops", "corpus/text-ops-0001.rb"),
    ("unicode", "corpus/unicode-0001.rb"),
    ("value-tails", "corpus/value-tails-0001.rb"),
];

/// The S3 fixed point, over one program from every corpus family stage 2
/// compares.
///
/// Byte-identical output is the only claim this makes. A stage 2 that runs,
/// writes a plausible `.rbc` and differs in one byte fails it.
#[test]
fn edge_stage3_is_byte_identical_on_a_sample_of_every_family() {
    let mut checked = 0usize;
    for (family, relative) in STAGE3_SAMPLE {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
        assert!(
            path.is_file(),
            "the {family} sample {} is missing, so the S3 sample no longer \
             covers the family it names",
            relative
        );
        let source = fs::read_to_string(&path).expect("a corpus program is UTF-8");
        if redblue::compile_source(&source).is_err() {
            panic!("{relative} is refused by the frontend, so it has no bytes to compare");
        }

        let expected = stage1(relative, &source);
        let actual = stage3(family, &path);
        assert!(
            actual.succeeded,
            "stage 2 did not compile {relative} when run as bytecode:\n{}",
            actual.stderr
        );
        assert!(
            !actual.bytes.is_empty(),
            "stage 2 wrote no output for {relative}"
        );
        assert_eq!(
            actual.bytes, expected,
            "the self-hosted compiler emitted different bytes than stage 1 for {relative}"
        );
        checked += 1;
    }
    assert_eq!(
        checked,
        STAGE3_SAMPLE.len(),
        "the S3 sample did not run every family it names"
    );
}

/// The statement shapes that decide whether the compiler, *run as bytecode*,
/// emits the same bytes as the compiler run by the tree-walker.
///
/// The corpus families above are one program each, and each of them is a handful
/// of statements. These are the shapes whose *opcodes* differ from one another —
/// a loop, a call, a closure, a record, an object, a `try` — because the risk on
/// this path is not the compiler's lexer but the bytecode VM mis-executing one
/// construct and the compiler encoding the wrong result. A stage 2 that ran the
/// `for each` of `bootstrap/compiler.rb` wrongly wrote bytes here too, which is
/// what the sample test above caught.
#[test]
fn edge_stage3_is_byte_identical_on_the_awkward_shapes() {
    let shapes: &[(&str, &str)] = &[
        // An empty file and one that is only a comment: no tokens, one `.rbc`.
        ("empty", ""),
        ("comment_only", "// nothing\n"),
        // A loop with a step, and one without, and a `while` with a `skip`: the
        // three shapes that have to agree on where the operand stack is.
        (
            "for_range_with_step",
            "for each i from 1 to 10 by 2\n    say i\nend\n",
        ),
        ("while_with_skip", "set i to 0\nwhile i is less than 5\n    set i to i + 1\n    if i is 3 then\n        skip\n    end\nend\nsay i\n"),
        // A call with several arguments, so the arguments have to survive being
        // pushed and then read back by the callee.
        (
            "call_with_many_arguments",
            "to total(a, b, c)\n    give back a + b + c\nend\nsay total(1, 2, 3)\n",
        ),
        // A closure over a captured variable, and a nested call chain.
        (
            "closure_and_nesting",
            "to adder(n)\n    to add(x)\n        give back x + n\n    end\n    give back add\nend\nset add5 to adder(5)\nsay add5(2)\n",
        ),
        // A record, a list and an object: three different constant-pool shapes.
        (
            "record_list_object",
            "set r to {a: 1, b: \"two\"}\nset xs to [1, 2, 3]\nsay r.a\nsay length(xs)\n",
        ),
        // A `try`/`catch`/`finally`, and an `unless`: both compile to jumps whose
        // targets have to land where the tree-walker's do. The failure is a
        // division at *run* time, so the program itself is a program.
        (
            "try_catch_finally",
            "to risky()\n    give back 1 / 0\nend\ntry\n    say risky()\ncatch failure\n    say \"caught\"\nfinally\n    say \"done\"\nend\nunless yes then\n    say \"no\"\nend\n",
        ),
        // A string with every escape, and non-ASCII text: the constant pool's
        // length fields and UTF-8 bytes.
        (
            "text_and_escapes",
            "say \"tab:\\tnl:\\nquote:\\\"backslash:\\\\\"\nsay \"héllo 日本語 🐉\"\n",
        ),
        // Every number spelling that has to survive as bits, not as text.
        (
            "number_boundaries",
            "say 0\nsay -0.0\nsay 1e308\nsay 5e-324\nsay 9007199254740993\nsay 123456789012345678\n",
        ),
    ];

    for (name, source) in shapes {
        assert!(
            redblue::compile_source(source).is_ok(),
            "{name} is refused by stage 1, so there are no bytes to compare"
        );
        let expected = stage1(name, source);

        let input = scratch(&format!("s3shape-{name}.rb"));
        fs::write(&input, source).expect("write the program for stage 3 to compile");
        let actual = stage3(&format!("s3shape-{name}"), &input);

        assert!(
            actual.succeeded,
            "stage 2 run as bytecode did not compile {name}:\n{}\nsource:\n{source}",
            actual.stderr
        );
        assert!(
            !actual.bytes.is_empty(),
            "stage 2 wrote no file for {name} when run as bytecode"
        );
        assert_eq!(
            actual.bytes.len(),
            expected.len(),
            "stage 2 wrote {} bytes where stage 1 wrote {} for {name} run as bytecode",
            actual.bytes.len(),
            expected.len()
        );
        assert_eq!(
            actual.bytes, expected,
            "stage 2 run as bytecode emitted different bytes than stage 1 for {name}\n\
             source:\n{source}"
        );
    }
}

/// The S3 comparison over the **whole** corpus, not a sample of it.
///
/// `stage2_is_byte_identical_on_its_corpus_families` makes this comparison with
/// the tree-walker executing the compiler. This one makes it with the bytecode VM
/// executing the compiler, which is the only execution path S3 has: a stage 2
/// that is byte-identical under one engine and wrong under the other is not a
/// compiler, it is a coincidence. 306 programs, about 23 seconds.
#[test]
fn edge_stage3_is_byte_identical_on_every_corpus_program() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    let mut checked = 0usize;
    let mut refused = 0usize;

    let mut entries: Vec<PathBuf> = fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("the corpus is readable: {e}"))
        .map(|entry| entry.expect("a corpus entry").path())
        .filter(|path| {
            path.extension().and_then(|e| e.to_str()) == Some("rb")
                && !path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("malformed-"))
        })
        .collect();
    entries.sort();
    // Read before the loop consumes the vector: the total is what says the walk
    // saw the whole corpus rather than a subset of it.
    let total = entries.len();
    assert_eq!(
        total, 315,
        "the corpus holds 315 programs outside `malformed`, and this walk found {total}; \
         a family has been added or removed and the counts below are not the whole \
         corpus any more"
    );

    for path in entries {
        let relative = path
            .strip_prefix(env!("CARGO_MANIFEST_DIR"))
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let source = fs::read_to_string(&path).expect("a corpus program is UTF-8");

        if redblue::compile_source(&source).is_err() {
            // No bytes to agree about. `edge_malformed_source_is_reported` and
            // `edge_stage3_refuses_every_malformed_corpus_program` are where the
            // refusals are asserted.
            refused += 1;
            continue;
        }

        let expected = stage1(&name, &source);
        let actual = stage3(&name, &path);
        assert!(
            actual.succeeded,
            "stage 2 run as bytecode did not compile {relative}:\n{}\nsource:\n{source}",
            actual.stderr
        );
        assert!(
            !actual.bytes.is_empty(),
            "stage 2 run as bytecode wrote no file for {relative}"
        );
        assert_eq!(
            actual.bytes, expected,
            "stage 2 run as bytecode emitted different bytes than stage 1 for {relative}"
        );
        checked += 1;
    }

    // Exact, for the reason `stage2_is_byte_identical_on_its_corpus_families`
    // says it: `checked >= 300` and `refused == 9` are both still true after six
    // corpus programs are deleted, and this test's whole claim is that *every*
    // program the frontend accepts is byte-identical through the bytecode path.
    // A floor cannot carry that claim — only the count itself can.
    assert_eq!(
        checked, 306,
        "{checked} corpus programs were compared through the bytecode path, not the \
         306 of the 315 outside `malformed` that the frontend accepts; stage 2 run \
         as bytecode is byte-identical on every one of them"
    );
    assert_eq!(
        refused, 9,
        "{refused} corpus programs outside `malformed` were refused by the frontend; \
         there were 9 when this test was written"
    );
    assert_eq!(
        checked + refused,
        total,
        "{checked} compared and {refused} refused is not every program this walk read, \
         so a program fell through without being either"
    );
}

/// Every program of the `malformed` family, through the bytecode path, held to
/// what stage 1 does with it.
///
/// `edge_malformed_source_is_reported_rather_than_compiled` covers eight shapes by
/// hand, with the tree-walker running the compiler. This walks all 46 the corpus
/// actually holds, with the bytecode VM running it, which is the path whose
/// operand-stack bookkeeping was wrong: a refusal that arrives through a
/// half-executed block has to be a refusal, not a file.
///
/// The family is not homogeneous, and saying so is the point of walking it:
///
/// - **43** are refused by the frontend's lexer or parser. Stage 2 *is* a lexer
///   and a parser, so it refuses them, exits non-zero and writes nothing.
/// - **1** is refused by the frontend's analyzer alone — an unbound name. Stage 2
///   implements no analyzer and compiles it. That is a gap, counted here so a new
///   one is noticed rather than absorbed.
/// - **2** are programs the frontend accepts. They are misfiled, and they are held
///   to the ordinary rule instead: stage 2 must agree with stage 1, byte for byte.
#[test]
fn edge_stage3_refuses_every_malformed_corpus_program() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    let mut entries: Vec<PathBuf> = fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("the corpus is readable: {e}"))
        .map(|entry| entry.expect("a corpus entry").path())
        .filter(|path| {
            path.extension().and_then(|e| e.to_str()) == Some("rb")
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("malformed-"))
        })
        .collect();
    entries.sort();

    // Exact, not a floor: the three counts below only add up to the family if
    // this one is right, and a floor here is what let `checked >= 40` stand in
    // for "all 46" while the comment above already claimed all 46.
    assert_eq!(
        entries.len(),
        46,
        "the malformed family holds {} programs, and this walk is written to cover \
         all of them; it is the only place the refusals of this family are asserted",
        entries.len()
    );

    let mut refusals = 0usize;
    let mut analyzer_only = 0usize;
    let mut accepted = 0usize;

    for path in entries {
        let relative = path
            .strip_prefix(env!("CARGO_MANIFEST_DIR"))
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        let source = fs::read_to_string(&path).expect("a corpus program is UTF-8");

        let output = scratch("stage3-malformed.rbc");
        clear_output(&output);

        let run = Command::new(env!("CARGO_BIN_EXE_rb"))
            .arg("vm")
            .arg(stage1_of_the_compiler())
            .arg(&path)
            .arg(&output)
            .output()
            .expect("the rb binary runs");

        match redblue::compile_source(&source) {
            // A lexer or parser refusal is the compiler's own business: stage 2
            // is a lexer and a parser, so it has to refuse these too, and to
            // refuse them by failing.
            Err(redblue::Error::Lexer(_, _) | redblue::Error::Parser(_, _)) => {
                assert!(
                    !run.status.success(),
                    "stage 2 run as bytecode accepted {relative}, whose source the \
                     frontend's own lexer or parser refuses"
                );
                assert!(
                    !output.exists(),
                    "stage 2 run as bytecode left a {} byte .rbc for the refused {relative}",
                    output.metadata().map(|m| m.len()).unwrap_or(0)
                );
                refusals += 1;
            }
            // An analyzer refusal is a *name* rule — an unbound variable, a parent
            // that does not exist — and `bootstrap/compiler.rb` implements no
            // analyzer, so it compiles these. That is a gap, not a disagreement.
            //
            // It used to be counted and nothing else, which threw away the run
            // that had just happened: stage 2 could crash on this file, exit
            // non-zero, or write bytes that were not a bytecode file at all and
            // the arm still answered `analyzer_only += 1`. What stage 2 writes
            // for a program stage 1 refuses to compile is exactly the case where
            // nobody is looking, so it is held here to the rule every other arm
            // is held to.
            //
            // Stage 1 has no bytes for this file — it refuses before codegen — so
            // byte-equality is reached by running stage 1's *codegen* on the parse
            // the analyzer refused, which is public (`Lexer` -> `parser::parse` ->
            // `bytecode::compile_program`) and is the whole of stage 1 minus the
            // analyzer stage 2 does not have. The arms then agree about the
            // program, and the only disagreement left is the one being recorded.
            Err(redblue::Error::Analyzer(_, _)) => {
                let tokens = redblue::lexer::Lexer::tokenize(&source)
                    .expect("the lexer accepts a program the analyzer refused");
                let program = redblue::parser::parse(tokens)
                    .expect("the parser accepts a program the analyzer refused");
                let expected = redblue::bytecode::compile_program(&program)
                    .expect("codegen accepts a program the analyzer refused")
                    .encode();

                assert!(
                    run.status.success(),
                    "stage 2 run as bytecode failed on {relative}, whose only frontend \
                     refusal is the analyzer's, and it has no analyzer:\n{}",
                    String::from_utf8_lossy(&run.stderr)
                );
                let actual = read_output(&output);
                assert!(
                    !actual.is_empty(),
                    "stage 2 run as bytecode wrote no file for {relative}, which it \
                     compiles because it has no analyzer"
                );
                assert!(
                    actual.starts_with(b"RED\x1a"),
                    "stage 2 run as bytecode wrote a file for {relative} that is not a \
                     Redblue bytecode file"
                );
                assert_eq!(
                    actual, expected,
                    "stage 2 run as bytecode emitted different bytes than stage 1's \
                     codegen for {relative}, which the analyzer alone refuses"
                );
                // And the file is one this build reads back to itself, so the bytes
                // that were compared are a file rather than something shaped like
                // one.
                let decoded = redblue::Chunk::decode(&actual).unwrap_or_else(|error| {
                    panic!("stage 2's file for {relative} does not decode: {error:?}")
                });
                assert_eq!(
                    decoded.encode(),
                    expected,
                    "stage 2's file for {relative} decodes to a different chunk than \
                     stage 1's codegen emits for it"
                );
                analyzer_only += 1;
            }
            // Two programs of this family are programs the frontend accepts, so
            // they are not refusals at all. They are held to the ordinary rule:
            // stage 2 has to agree with stage 1 about them, byte for byte.
            Ok(chunk) => {
                let expected = chunk.encode();
                assert!(
                    run.status.success(),
                    "stage 2 run as bytecode refused {relative}, which the frontend accepts:\n{}",
                    String::from_utf8_lossy(&run.stderr)
                );
                assert_eq!(
                    read_output(&output),
                    expected,
                    "stage 2 run as bytecode emitted different bytes than stage 1 for {relative}, \
                     which the frontend accepts"
                );
                accepted += 1;
            }
            Err(other) => {
                panic!("{relative} is refused in a way this test does not name: {other:?}")
            }
        }
    }

    assert_eq!(
        refusals, 43,
        "the malformed family holds 43 programs the frontend's lexer or parser refuses; \
         this walk refused {refusals} of them through the bytecode path"
    );
    assert_eq!(
        analyzer_only, 1,
        "{analyzer_only} programs of the malformed family are refused by the frontend's \
         analyzer alone, and stage 2 has no analyzer — there was 1"
    );
    assert_eq!(
        accepted, 2,
        "{accepted} programs of the malformed family are programs the frontend accepts, \
         so they are compared rather than refused; there were 2"
    );
}

/// A stage-2 run that fails has to say so, and has to write nothing.
///
/// The fixed point above is a comparison of bytes; a stage 2 that exits non-zero
/// after writing a plausible file would satisfy nothing, and one that exits zero
/// after failing would report a compiler that works. So both halves are pinned:
/// a refused program produces no `.rbc` at all, and a bad invocation to the
/// self-hosted compiler is a non-zero exit rather than a silent success.
#[test]
fn edge_stage3_reports_a_failure_rather_than_writing_a_file() {
    // A program the frontend refuses. Stage 2 must not invent bytes for it.
    let broken = scratch("stage3-broken.rb");
    fs::write(&broken, "say \"unterminated\n").expect("the broken program is written");
    let output = scratch("stage3-broken.rbc");
    clear_output(&output);

    let run = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("vm")
        .arg(stage1_of_the_compiler())
        .arg(&broken)
        .arg(&output)
        .output()
        .expect("the rb binary runs");

    assert!(
        !run.status.success(),
        "compiling an unterminated string must be a failure, not an exit 0"
    );
    assert!(
        !output.exists(),
        "a refused program must leave no output file behind, so a stale one \
         cannot be mistaken for a fresh compile"
    );

    // And a path that is not Redblue source at all — a bytecode file — must be
    // refused by the same invocation, rather than being read as text.
    let not_source = scratch("stage3-not-source.rb");
    fs::write(&not_source, stage1("tiny", "say 1\n")).expect("the .rbc is written");
    clear_output(&output);

    let run = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("vm")
        .arg(stage1_of_the_compiler())
        .arg(&not_source)
        .arg(&output)
        .output()
        .expect("the rb binary runs");

    assert!(
        !run.status.success(),
        "handing the compiler a .rbc must be a failure"
    );
    assert!(
        !output.exists(),
        "a refused input must leave no output file behind"
    );
}

// ---------------------------------------------------------------------------
// S3 — the fixed point
// ---------------------------------------------------------------------------

/// Three self-compilations, made once per test process and read by the two tests
/// below.
///
/// The definition of done asks for the fixed point *and* for three consecutive
/// runs, and those are three runs of the same expensive thing, so they are made
/// once here and read twice. A `OnceLock` rather than a file: the bytes are in
/// memory, and two tests in one process must not race for them.
///
/// They run at the same time rather than one after another, because one
/// self-compilation is minutes in a test build and three of them in series is a
/// gate nobody reads. Nothing is shared but the already-written `stage1.rbc`,
/// which no run writes, and each run writes a file of its own — so what runs
/// beside what cannot change a byte, which is exactly the claim
/// `edge_three_consecutive_self_compilations_are_byte_identical` makes about them.
fn self_compilations() -> &'static [Stage2; 3] {
    static RUNS: std::sync::OnceLock<[Stage2; 3]> = std::sync::OnceLock::new();
    RUNS.get_or_init(|| {
        let running: Vec<std::process::Child> = (0..3)
            .map(|run| self_compile(run).expect("the rb binary starts"))
            .collect();

        let runs: Vec<Stage2> = running
            .into_iter()
            .zip(0..3)
            .map(|(child, run)| {
                let executed = child.wait_with_output().unwrap_or_else(|error| {
                    panic!("self-compilation {run} could not be waited on: {error}")
                });
                Stage2 {
                    bytes: read_output(&scratch(&format!("stage3-self-{run}.rbc"))),
                    stderr: String::from_utf8_lossy(&executed.stderr).into_owned(),
                    succeeded: executed.status.success(),
                }
            })
            .collect();

        let mut runs = runs.into_iter();
        [
            runs.next().expect("three runs were made"),
            runs.next().expect("three runs were made"),
            runs.next().expect("three runs were made"),
        ]
    })
}

/// Starts one self-compilation and leaves it running.
///
/// `bootstrap/compiler.rb`, compiled by itself, run as bytecode. Stage 1 is the
/// Rust `rb compile`. This is stage 2 obtained the long way round: the *same*
/// Redblue compiler, but its `.rbc` executed by the bytecode VM instead of its
/// source executed by the tree-walker. Nothing here reads
/// `stage1_of_the_compiler()`'s own output — the input and the output are two
/// different files, and the run writes only the output.
///
/// It returns the child rather than a result because the point is to have three
/// of these running at once; `self_compilations` waits for them.
fn self_compile(run: usize) -> io::Result<std::process::Child> {
    let output = scratch(&format!("stage3-self-{run}.rbc"));
    clear_output(&output);

    Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("vm")
        .arg(stage1_of_the_compiler())
        .arg(compiler())
        .arg(&output)
        // Piped, not inherited: `spawn` inherits both by default, and a run that
        // fails then says why to the test log instead of to the assertion that
        // reports which of the three runs it was.
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
}

/// Ladder stage **S3**: `stage1.rbc == stage2.rbc`.
///
/// `stage1.rbc` is `bootstrap/compiler.rb` compiled by the Rust frontend.
/// `stage2.rbc` is the same source compiled by that very file, running as
/// bytecode. The two must be one file, byte for byte — which is the only
/// statement the ladder makes about stage 3, and the only one that cannot be
/// faked by a compiler that happens to be right about small programs.
#[test]
fn edge_the_self_hosted_compiler_compiles_itself_byte_identically() {
    let stage1_bytes = fs::read(stage1_of_the_compiler()).expect("stage 1's file is readable");
    let stage2 = &self_compilations()[0];

    assert!(
        stage2.succeeded,
        "the self-hosted compiler did not compile itself when run as bytecode:\n{}",
        stage2.stderr
    );
    assert!(
        !stage2.bytes.is_empty(),
        "the self-hosted compiler wrote no file for itself"
    );

    // The file that came back has to be a bytecode file this build reads, not
    // merely something the right length: `stage2.rbc == stage1.rbc` is a
    // statement about two `.rbc` files.
    let decoded = redblue::Chunk::decode(&stage2.bytes).unwrap_or_else(|error| {
        panic!(
            "stage 2's file for the compiler does not decode, so it is not a \
             bytecode file: {error:?}"
        )
    });
    assert_eq!(
        decoded.encode(),
        stage1_bytes,
        "stage 2's file for the compiler decodes to a different chunk than stage 1 wrote"
    );

    // And the bytes themselves, which is the claim.
    assert_eq!(
        stage2.bytes.len(),
        stage1_bytes.len(),
        "stage 2 wrote {} bytes for the compiler where stage 1 wrote {}",
        stage2.bytes.len(),
        stage1_bytes.len()
    );
    assert_eq!(
        stage2.bytes, stage1_bytes,
        "the compiler compiled by itself is not the compiler: stage 1 and stage 2 \
         disagree about its bytecode"
    );
}

/// Three consecutive self-compilations, byte for byte.
///
/// Determinism is a separate claim from the fixed point: a compiler that is
/// self-consistent but reads a hash map in whatever order it happens to hold,
/// or a pool that a run fills in a different order from the last, agrees with
/// itself only sometimes. Three runs is the definition of done's number.
///
/// The first is the run `edge_the_self_hosted_compiler_compiles_itself_byte_identically`
/// already made, so this test makes the other two and compares all three.
#[test]
fn edge_three_consecutive_self_compilations_are_byte_identical() {
    let runs = self_compilations();
    let first = &runs[0];
    assert!(
        first.succeeded,
        "the first self-compilation did not finish:\n{}",
        first.stderr
    );
    assert!(
        !first.bytes.is_empty(),
        "the first self-compilation wrote no file, so nothing was compared"
    );

    for (index, run) in runs.iter().enumerate().skip(1) {
        assert!(
            run.succeeded,
            "self-compilation {index} did not finish:\n{}",
            run.stderr
        );
        assert_eq!(
            run.bytes.len(),
            first.bytes.len(),
            "self-compilation {index} wrote {} bytes where run 0 wrote {}",
            run.bytes.len(),
            first.bytes.len()
        );
        assert_eq!(
            run.bytes, first.bytes,
            "self-compilation {index} emitted different bytes than run 0, so the \
             compiler is not deterministic"
        );
    }
}

/// The step budget is a real guard, and the fixed point is a real program run
/// under it.
///
/// Ladder stage S3 needs one self-compilation to finish inside the published
/// per-program step budget, which is why `MAX_STEPS` had to be measured against
/// it. This pins the other half: the budget still refuses a program that
/// outlasts it, and it refuses it by failing rather than by writing a truncated
/// `.rbc` that a later run could mistake for a compile.
#[test]
fn edge_the_self_compilation_still_obeys_the_step_budget() {
    let output = scratch("stage3-starved.rbc");
    clear_output(&output);

    // A budget the compiler cannot possibly finish inside. It is set on the
    // child process only: the point is what `rb vm` does when a run is starved,
    // not what this process's own limits are.
    let run = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("vm")
        .arg(stage1_of_the_compiler())
        .arg(compiler())
        .arg(&output)
        .env("REDBLUE_MAX_STEPS", "1000")
        .output()
        .expect("the rb binary runs");

    assert!(
        !run.status.success(),
        "a run starved of steps exited 0, so the step budget is not a bound"
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("Step budget"),
        "a starved run failed without naming the budget:\n{stderr}"
    );
    assert!(
        !output.exists(),
        "a run stopped by the step budget left a {} byte .rbc behind, which a \
         later run could read as a finished compile",
        output.metadata().map(|m| m.len()).unwrap_or(0)
    );
}

/// The shape stage 2 gets wrong: a `while` loop whose body holds another one.
///
/// `compile_while` used to keep the slot of its own `JumpIfFalse` in a name
/// across the call that compiles its body — and a name a Redblue function
/// assigns is one program-wide name, so the inner loop's `compile_while` took
/// the outer loop's slot and patched the wrong jump. The compiler's own lexer
/// has exactly this shape, which is why nothing but compiling the compiler
/// found it: the outer loop then pointed at the end of its block instead of the
/// instruction after itself, and `bootstrap/compiler.rb` came back 15 bytes
/// longer than stage 1 wrote for it.
///
/// Each `while` is one statement, so the shape is reproduced here with nested
/// `while`s of different lengths, and the assertion is the whole `.rbc`.
#[test]
fn edge_a_while_loop_nested_in_another_patches_its_own_jump() {
    let shapes: &[(&str, &str)] = &[
        // The smallest shape that has the defect: an outer loop whose body ends
        // in an inner loop, and a statement after the outer `end` so that "one
        // past the loop" and "end of the block" are different offsets.
        (
            "nested_while",
            "to count(n)\n\
             \x20   set total to 0\n\
             \x20   set i to 0\n\
             \x20   while i < n\n\
             \x20       set j to 0\n\
             \x20       while j < i\n\
             \x20           set total to total + j\n\
             \x20           set j to j + 1\n\
             \x20       end\n\
             \x20       set i to i + 1\n\
             \x20   end\n\
             \x20   give back total\n\
             end\n\
             say count(4)\n",
        ),
        // Three deep, because the third loop's slot is the one the second took
        // and the second's is the one the first took.
        (
            "three_nested_while",
            "to count(n)\n\
             \x20   set total to 0\n\
             \x20   set i to 0\n\
             \x20   while i < n\n\
             \x20       set j to 0\n\
             \x20       while j < i\n\
             \x20           set k to 0\n\
             \x20           while k < j\n\
             \x20               set total to total + k\n\
             \x20               set k to k + 1\n\
             \x20           end\n\
             \x20           set j to j + 1\n\
             \x20       end\n\
             \x20       set i to i + 1\n\
             \x20   end\n\
             \x20   give back total\n\
             end\n\
             say count(4)\n",
        ),
        // An `if` inside the inner loop: the branch's own jump is patched too,
        // so a body that is not only a loop is covered as well.
        (
            "while_with_if_inside",
            "to count(n)\n\
             \x20   set total to 0\n\
             \x20   set i to 0\n\
             \x20   while i < n\n\
             \x20       set j to 0\n\
             \x20       while j < i\n\
             \x20           if j is 1 then\n\
             \x20               set total to total + 10\n\
             \x20           else\n\
             \x20               set total to total + j\n\
             \x20           end\n\
             \x20           set j to j + 1\n\
             \x20       end\n\
             \x20       set i to i + 1\n\
             \x20   end\n\
             \x20   give back total\n\
             end\n\
             say count(4)\n",
        ),
        // A `while` inside a `for each`: the other loop form, whose body is
        // compiled by `compile_loop_body` and reaches the same `compile_while`.
        (
            "while_inside_for_each",
            "set total to 0\n\
             for each n in [1, 2, 3]\n\
             \x20   set i to 0\n\
             \x20   while i < n\n\
             \x20       set total to total + i\n\
             \x20       set i to i + 1\n\
             \x20   end\n\
             end\n\
             say total\n",
        ),
        // The boundary the fix turns on: "the instruction after this loop" and
        // "the end of the block" are the same offset only when nothing follows
        // the loop. An *empty* body puts the condition's `JumpIfFalse` one
        // instruction from the jump back to the top, so a patch that is off by
        // one instruction is visible here and nowhere else.
        (
            "nested_while_with_empty_bodies",
            "to count(n)\n\
             \x20   set i to 0\n\
             \x20   while i < n\n\
             \x20       set j to 0\n\
             \x20       while j < 0\n\
             \x20       end\n\
             \x20       set i to i + 1\n\
             \x20   end\n\
             \x20   give back i\n\
             end\n\
             say count(3)\n",
        ),
        // The same shape reached through a branch rather than straight down: the
        // inner loop is inside the `else` of an `if`, so two jumps and one loop
        // exit are patched in the order they were recorded.
        (
            "while_in_else_containing_while",
            "to count(n)\n\
             \x20   set total to 0\n\
             \x20   set i to 0\n\
             \x20   while i < n\n\
             \x20       if i is 0 then\n\
             \x20           set total to total + 1\n\
             \x20       else\n\
             \x20           set j to 0\n\
             \x20           while j < i\n\
             \x20               set total to total + j\n\
             \x20               set j to j + 1\n\
             \x20           end\n\
             \x20       end\n\
             \x20       set i to i + 1\n\
             \x20   end\n\
             \x20   give back total\n\
             end\n\
             say count(4)\n",
        ),
        // A `while` whose body holds no loop, so the slot survives: the case
        // that already worked, pinned so the fix cannot be "always take the
        // end of the block".
        (
            "while_with_plain_body",
            "to count(n)\n\
             \x20   set total to 0\n\
             \x20   set i to 0\n\
             \x20   while i < n\n\
             \x20       set total to total + i\n\
             \x20       set i to i + 1\n\
             \x20   end\n\
             \x20   give back total\n\
             end\n\
             say count(4)\n",
        ),
        // One loop, in a block that is nothing but that loop: the exit offset
        // and the end of the block are then the same number, so the two answers
        // the patch could give are indistinguishable. It is here so that a fix
        // which *always* used one of them would still be caught by the cases
        // above rather than passing because of this one.
        (
            "lone_while_filling_its_block",
            "to count(n)\n\
             \x20   set i to 0\n\
             \x20   while i < n\n\
             \x20       set i to i + 1\n\
             \x20   end\n\
             \x20   set i to 0\n\
             end\n\
             say count(3)\n",
        ),
    ];

    for (name, source) in shapes {
        assert_identical(name, source);
    }
}
