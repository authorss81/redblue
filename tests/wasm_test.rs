//! The WebAssembly playground boundary.
//!
//! `src/wasm.rs` is the one place program output leaves the interpreter, and
//! the one place the exported C ABI lives. Everything here runs natively: the
//! point is not that the tests need a `.wasm` file, but that the *contract* the
//! playground depends on — exact bytes out, rendered errors back, never a panic
//! — holds on the same code the native `rb` binary runs.
//!
//! The goldens below were captured from `rb run examples/<name>.rb` on the
//! native binary. `wasm/check-examples.js` re-captures them under `node` from
//! the built `.wasm` and diffs the two, so a change that makes the two
//! pipelines disagree fails there even if it somehow passed here.

use redblue::wasm;

/// The bytes native `rb` writes for `examples/hello.rb`.
const HELLO: &str = "Hello, World!\nHello, World!\nHello, {name}!\n";

const FIZZBUZZ: &str = "1\n2\nFizz\n4\nBuzz\nFizz\n7\n8\nFizz\nBuzz\n11\nFizz\n13\n14\n\
                        FizzBuzz\n1\n2\n3\n4\n5\n";

const FORMATS: &str = "Parsed JSON:\n{name: Alice, age: 30, active: yes}\nName:\nAlice\n\
                      Array:\n[1, 2, 3]\nStringified:\n\
                      {\"name\": \"Alice\", \"age\": 30, \"active\": true}\nDemo complete!\n";

const TEST_ARITHMETIC: &str = "30\n20\n42\n25\n2\nAll arithmetic tests completed\n";

const RANDOM: &str = "a die:\n5\nthe only member of a range of one:\n5\nbetween 0 and 1:\n\
                      0.23011702022018832\nbetween -10 and -5:\n-5.1540884184635365\n\
                      a colour:\nred\nthe same three, shuffled:\n[green, red, blue]\n";

const FILES: &str =
    "File contents:\nHello from Redblue!\noutput.txt exists!\nNumber of lines:\n1\n\
                     Updated contents:\nHello from Redblue! - appended text\n\
                     Copy created successfully!\nDone!\n";

/// Every example the playground claims byte-identical output for, and the bytes
/// native `rb` produced for it. `examples/time.rb` is deliberately absent: it
/// prints the wall clock, so it has no fixed bytes to be identical to.
const EXAMPLES: [(&str, &str); 6] = [
    ("examples/hello.rb", HELLO),
    ("examples/fizzbuzz.rb", FIZZBUZZ),
    ("examples/formats.rb", FORMATS),
    ("examples/test_arithmetic.rb", TEST_ARITHMETIC),
    ("examples/random.rb", RANDOM),
    ("examples/files.rb", FILES),
];

#[test]
fn wasm_run_reproduces_the_native_output_of_every_example() {
    for (path, expected) in EXAMPLES {
        let source =
            std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
        let out = wasm::run_program(&source)
            .unwrap_or_else(|e| panic!("{path} failed in the wasm path: {e}"));
        assert_eq!(
            out, expected,
            "{path}: the playground output differs from native `rb` output"
        );
    }
}

/// The gate `check-examples.js` proves from the outside, pinned from the
/// inside: the corpus is the six listed above, and it is derived from the
/// `examples/` directory rather than asserted twice by hand. A new example that
/// reaches the clock is skipped by the rule; anything else has to be added to
/// [`EXAMPLES`] with its golden, or this test fails.
#[test]
fn wasm_example_corpus_is_exactly_the_deterministic_examples() {
    let mut on_disk: Vec<String> = std::fs::read_dir("examples")
        .expect("examples/ must be readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().to_string_lossy().replace('\\', "/"))
        .filter(|path| path.ends_with(".rb"))
        .collect();
    on_disk.sort();

    let deterministic: Vec<String> = on_disk
        .iter()
        .filter(|path| {
            !std::fs::read_to_string(path)
                .unwrap_or_default()
                .contains("time.now()")
        })
        .cloned()
        .collect();

    let mut listed: Vec<String> = EXAMPLES.iter().map(|(path, _)| path.to_string()).collect();
    listed.sort();

    assert_eq!(
        listed, deterministic,
        "the playground corpus and examples/ have drifted apart"
    );
    assert!(
        !listed.contains(&"examples/time.rb".to_string()),
        "time.rb prints the wall clock and has no fixed bytes to be identical to"
    );
}

/// `say` writes its line at the end of the program and `print` writes at once,
/// in the order the interpreter has always used them: a program that mixes the
/// two prints everything `print`ed first, then everything `say`ed. The
/// playground reproduces that ordering rather than "fixing" it, because
/// changing it would make its output differ from native `rb` — which is the
/// one thing this phase promises not to do.
#[test]
fn edge_print_is_immediate_and_say_flushes_at_the_end() {
    let out = wasm::run_program("print \"a\"\nprint \"b\"\nsay \"c\"\nprint \"d\"\n")
        .expect("the program is valid");
    assert_eq!(out, "abdc\n", "say/print interleaving must match native rb");

    // The trailing-newline rule has to survive a program that ends without a
    // `say` at all: the capture is the program's bytes, not a normalised form.
    let bare = wasm::run_program("print \"no newline at all\"").expect("the program is valid");
    assert_eq!(bare, "no newline at all");
}

/// Nothing in, nothing out. An empty program is not an error and must not leave
/// a stray byte behind for the host to print.
#[test]
fn edge_empty_and_whitespace_only_programs_produce_no_output() {
    assert_eq!(wasm::run_program("").expect("empty is valid"), "");
    assert_eq!(
        wasm::run_program("\n\n   \n").expect("blanks are valid"),
        ""
    );
}

/// Interpolation, emoji, CJK, RTL and a combining mark all cross the boundary as
/// UTF-8 with no transcoding, and a lone surrogate-free 4-byte code point is
/// not truncated at the page edge.
#[test]
fn edge_unicode_survives_the_boundary_intact() {
    let out =
        wasm::run_program("say \"\u{1F600} \u{4F60}\u{597D} \u{627}\u{644} \u{E9}\u{301}\"\n")
            .expect("the program is valid");
    assert_eq!(
        out,
        "\u{1F600} \u{4F60}\u{597D} \u{627}\u{644} \u{E9}\u{301}\n"
    );
    // Byte length, not char length: the host copies `len` bytes across.
    assert_eq!(
        out.len(),
        "\u{1F600} \u{4F60}\u{597D} \u{627}\u{644} \u{E9}\u{301}\n".len()
    );
}

/// A program that cannot parse comes back as a rendered error naming the line,
/// and comes back as an `Err` — not as output, and not as a panic. This is the
/// playground's only failure channel, so it has to be the interpreter's own.
#[test]
fn a_malformed_program_is_reported_and_is_not_output() {
    let error =
        wasm::run_program("say \"unterminated\n").expect_err("an unterminated string must not run");
    assert!(
        error.to_lowercase().contains("string"),
        "the error must name the problem, got: {error}"
    );

    let caught = wasm::run_program("set x to 1 / 0\nsay x\n")
        .expect_err("a division by zero is a failure, not a value");
    assert!(
        caught.to_lowercase().contains("division"),
        "the error must name the failure, got: {caught}"
    );
}

#[test]
fn a_runtime_failure_is_reported_rather_than_panicking() {
    let error = wasm::run_program("say 1 / 0\n").expect_err("division by zero must fail");
    assert!(
        !error.is_empty(),
        "a failure must carry a message the playground can display"
    );

    // What the program managed to print before failing is still returned: the
    // host shows it, and hiding it would make the failure unexplainable.
    let partial = wasm::run_program("say \"before\"\nsay 1 / 0\n")
        .expect_err("the second statement must fail");
    assert!(!partial.is_empty(), "the message must not be empty");
}

/// The exported C ABI, exercised natively. The host copies bytes in through
/// `rb_alloc`, calls `rb_run`, and reads bytes back out of the module's linear
/// memory — this test walks the same path so a mistake in the pointer
/// arithmetic fails in `cargo test` rather than in a browser.
#[test]
fn wasm_abi_round_trips_source_and_output() {
    let _channels = host_channels();
    let source = b"say \"through the ABI\"\nprint \"tail\"";
    unsafe {
        let input = wasm::rb_alloc(source.len() as i32);
        assert!(!input.is_null(), "rb_alloc returned null");
        std::ptr::copy_nonoverlapping(source.as_ptr(), input, source.len());

        let status = wasm::rb_run(input, source.len() as i32);
        assert_eq!(status, 0, "the ABI must report success for a valid program");

        // `print` lands before `say` here for the same reason it does natively:
        // the interpreter buffers `say` until the program ends.
        assert_eq!(read_output(), "tailthrough the ABI\n");

        assert_eq!(
            wasm::rb_release(input.cast_const()),
            0,
            "the buffer the host wrote into must be released"
        );
    }
}

/// The same ABI, given input that is not valid UTF-8. A host that hands the
/// module arbitrary bytes must get an error back, not undefined behaviour.
#[test]
fn edge_abi_rejects_bytes_that_are_not_utf8() {
    let _channels = host_channels();
    let bytes: [u8; 4] = [0xff, 0xfe, 0x41, 0x80];
    unsafe {
        let input = wasm::rb_alloc(bytes.len() as i32);
        assert!(!input.is_null(), "rb_alloc returned null");
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), input, bytes.len());

        let status = wasm::rb_run(input, bytes.len() as i32);
        assert_ne!(status, 0, "non-UTF-8 input must be refused");

        let message = read_error();
        assert!(!message.is_empty(), "a refusal must carry a message");
        assert!(
            message.contains("UTF-8"),
            "the refusal must name the problem, got: {message}"
        );

        assert_eq!(wasm::rb_release(input.cast_const()), 0);
    }
}

/// A negative length is what a host that computed `len - 1` on an empty string
/// sends. It must be refused before it becomes a huge `usize`.
#[test]
fn edge_abi_refuses_a_negative_length() {
    // A negative length is refused before the pointer is read at all, so the
    // null here is never dereferenced — that is what the call asserts.
    assert_eq!(unsafe { wasm::rb_run(std::ptr::null(), -1) }, -1);
    assert_eq!(wasm::rb_alloc(-1), std::ptr::null_mut());
    assert_eq!(wasm::rb_release(std::ptr::null()), -1);
}

/// The version and keyword list the playground page shows. A host that asks for
/// them must not get an empty string, which is what a zero `len` looks like.
#[test]
fn wasm_module_reports_its_version_and_keywords() {
    assert!(
        !wasm::version().is_empty(),
        "the module must report a version to the page"
    );
    let keywords = wasm::keywords();
    assert!(
        keywords.contains("say") && keywords.contains("set"),
        "the keyword list must name the language's own keywords, got: {keywords:?}"
    );
}

/// `files.*` reads what it wrote, in both builds. On a native target this is
/// `std::fs`; on `wasm32-unknown-unknown` it is the in-memory filesystem in
/// `src/vfs.rs`. Whichever one answers, the *program* sees one filesystem — so
/// this is the assertion that holds for the playground, not for a backend.
#[test]
fn files_reads_back_what_it_wrote_in_a_temp_dir() {
    let dir = temp_dir("wasm_files_round_trip");
    let path = dir.join("note.txt");
    let source = format!(
        "files.write(\"{path}\", \"first\")\n\
         files.append(\"{path}\", \" second\")\n\
         say files.read(\"{path}\")\n\
         say files.exists(\"{path}\")\n\
         files.copy(\"{path}\", \"{path}.copy\")\n\
         say files.read(\"{path}.copy\")\n\
         files.rename(\"{path}.copy\", \"{path}.moved\")\n\
         say files.exists(\"{path}.copy\")\n\
         say files.exists(\"{path}.moved\")\n\
         files.delete(\"{path}\")\n\
         say files.exists(\"{path}\")\n",
        path = path.display()
    );

    let out = wasm::run_program(&source).unwrap_or_else(|e| panic!("files round trip failed: {e}"));
    assert_eq!(
        out, "first second\nyes\nfirst second\nno\nyes\nno\n",
        "the files module must behave the same whichever filesystem answers"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A path nobody wrote is a failure in both builds — a `NotFound` the program
/// can catch, not an empty file and not a panic. This is the edge the in-memory
/// filesystem could most easily get wrong, since a map lookup "succeeds" with
/// `None`.
#[test]
fn edge_reading_a_file_that_was_never_written_is_a_catchable_error() {
    let dir = temp_dir("wasm_files_missing");
    let missing = dir.join("never-written.txt");
    let source = format!(
        "say files.exists(\"{missing}\")\n\
         try\n\
             say files.read(\"{missing}\")\n\
         catch error\n\
             say \"caught\"\n\
         end\n",
        missing = missing.display()
    );

    let out = wasm::run_program(&source)
        .unwrap_or_else(|e| panic!("a caught failure must not fail the run: {e}"));
    assert_eq!(out, "no\ncaught\n", "a missing file must be catchable");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Deep recursion is the interpreter's own resource limit, and it has to be the
/// same limit in both builds. Natively the room for `MAX_CALL_DEPTH` comes from
/// a spawned thread sized by `STACK_BYTES_PER_CALL`; `wasm32-unknown-unknown`
/// has no threads, so the same depth has to fit in the one stack the module has,
/// and `build.rs` links one sized from the *same product* so the two builds
/// cannot disagree about what they can reach. This asserts the *observable*
/// contract, which is the same either way: a clean, catchable `Limit` error, and
/// a module that still works afterwards.
///
/// The recursion is written in the shape `tests/call_depth_test.rs` uses — the
/// recursive call is *assigned* rather than returned. A `return` inside an `if`
/// does not return from the function in this language, so the obvious
/// `return f(n - 1)` recurses forever at any depth and would make this test pass
/// for any limit at all. This one counts real frames, so the two halves below
/// pin the boundary from both sides.
#[test]
fn edge_recursion_beyond_the_call_depth_is_a_clean_error_not_an_abort() {
    let program = |depth: usize| {
        format!(
            "set reached to 0\n\
             to countdown(n)\n\
                 if n is 0 then\n\
                     set reached to 0\n\
                 else\n\
                     set reached to countdown(n - 1)\n\
                 end\n\
                 give back reached\n\
             end\n\
             try\n\
                 say countdown({depth})\n\
             catch error\n\
                 say \"caught\"\n\
             end\n"
        )
    };

    // Just inside the limit: real frames, and the program still answers.
    let shallow = wasm::run_program(&program(redblue::MAX_CALL_DEPTH - 1))
        .unwrap_or_else(|e| panic!("a depth under the limit must run: {e}"));
    assert_eq!(shallow, "0\n", "a legal depth must produce its value");

    // One frame deeper: the same program, and the limit.
    let deep = wasm::run_program(&program(redblue::MAX_CALL_DEPTH))
        .unwrap_or_else(|e| panic!("a caught depth limit must not fail the run: {e}"));
    assert_eq!(
        deep, "caught\n",
        "recursion past MAX_CALL_DEPTH must be a catchable error"
    );

    // And far past it, which is the shape a runaway program actually has.
    let runaway = wasm::run_program(&program(100_000))
        .unwrap_or_else(|e| panic!("a caught depth limit must not fail the run: {e}"));
    assert_eq!(
        runaway, "caught\n",
        "unbounded recursion must be caught too"
    );

    // The module survives it — this is the assertion that fails if the run
    // traps instead of erroring.
    assert_eq!(
        wasm::run_program("say \"still here\"").expect("the module must survive"),
        "still here\n"
    );
}

/// A failure must not swallow what the program had already written. The message
/// goes in one buffer and the output in the other, and a host that got an empty
/// output buffer beside an error would show the user "this program printed
/// nothing" — which is a different claim from the truth.
///
/// What survives is what reached the output boundary, which is what `print`
/// writes at once; `say` is buffered until the program ends and so is lost with
/// the failure, in the playground and in native `rb` alike. That is the
/// behaviour both builds have always had, and the playground matching it is the
/// point — the alternative, flushing on failure, would make the two disagree.
#[test]
fn edge_a_failure_keeps_the_output_written_before_it() {
    let _channels = host_channels();
    let source = "say \"buffered until the end\"\nprint \"immediate\"\nset x to 1 / 0\n";
    unsafe {
        let input = wasm::rb_alloc(source.len() as i32);
        assert!(!input.is_null(), "rb_alloc returned null");
        std::ptr::copy_nonoverlapping(source.as_ptr(), input, source.len());

        let status = wasm::rb_run(input, source.len() as i32);
        assert_eq!(status, 1, "a division by zero must fail the run");

        assert_eq!(
            read_output(),
            "immediate",
            "the bytes written before the failure must survive it"
        );

        let error = read_error();
        assert!(
            error.to_lowercase().contains("division"),
            "the failure must still be reported, got {error:?}"
        );

        assert_eq!(wasm::rb_release(input.cast_const()), 0);
    }

    // The same two rules on a successful run, so the assertion above cannot be
    // satisfied by a boundary that simply never collects anything.
    assert_eq!(
        wasm::run_program("print \"tail\"\nsay \"line\"\n").expect("a valid program runs"),
        "tailline\n"
    );
}

/// The pointer a host is holding must survive whatever else the module does.
/// `rb_alloc` used to reuse one buffer, so a second allocation for a longer
/// program moved the first one and left the host writing into freed memory —
/// which no test could see, because the tests allocated once and ran once.
#[test]
fn edge_an_allocation_stays_put_however_many_programs_follow_it() {
    let _channels = host_channels();
    let first = b"say \"first\"\n";
    let longer = b"say \"a considerably longer second program\"\n";
    unsafe {
        let first_ptr = wasm::rb_alloc(first.len() as i32);
        assert!(!first_ptr.is_null(), "rb_alloc returned null");
        std::ptr::copy_nonoverlapping(first.as_ptr(), first_ptr, first.len());

        // Several allocations of a different size, each of which is what a
        // reused-and-resized buffer would have reallocated.
        let mut fillers = Vec::new();
        for filler in 0..8u32 {
            let other = wasm::rb_alloc(64 + filler as i32 * 97);
            assert!(!other.is_null(), "rb_alloc returned null");
            fillers.push(other);
        }

        // The first pointer is still writable and still holds its own bytes.
        std::ptr::copy_nonoverlapping(first.as_ptr(), first_ptr, first.len());
        let status = wasm::rb_run(first_ptr, first.len() as i32);
        assert_eq!(status, 0, "the original buffer must still be runnable");
        assert_eq!(
            read_output(),
            "first\n",
            "a later allocation must not have moved the first one"
        );

        // The longer program is a separate allocation and runs separately.
        let longer_ptr = wasm::rb_alloc(longer.len() as i32);
        std::ptr::copy_nonoverlapping(longer.as_ptr(), longer_ptr, longer.len());
        assert_eq!(wasm::rb_run(longer_ptr, longer.len() as i32), 0);
        assert_eq!(read_output(), "a considerably longer second program\n");

        // Every allocation is released by its own address, and every release is
        // checked rather than assumed. This used to call `rb_release` twice with
        // a *length* — which the old signature ignored and the new one takes as
        // an address — and ignore both returns, so it passed whether or not
        // anything had been freed at all.
        for pointer in fillers {
            assert_eq!(
                wasm::rb_release(pointer.cast_const()),
                0,
                "an allocation that was made must be releasable"
            );
        }
        assert_eq!(wasm::rb_release(longer_ptr.cast_const()), 0);
        assert_eq!(
            wasm::rb_release(first_ptr.cast_const()),
            0,
            "the first allocation must be releasable too, not just the newest"
        );
    }
}

/// The playground has to stop somewhere, and it has to say so. A program past
/// the limit is reported as a failure rather than truncated: half an endless
/// loop is not the program's output, and a host that got it would show a
/// success it did not get.
///
/// Driven through `print`, which writes at the boundary, so this is the limit
/// [`emit`] itself enforces. The `say` side of the same limit — a bound charged
/// where the lines are still *buffered* — is
/// [`edge_say_output_is_bounded_while_it_is_still_buffered`].
#[test]
fn edge_output_past_the_playground_limit_is_reported_not_truncated() {
    let _channels = host_channels();
    let program = "repeat 200000 times\n    print \"a line that is long enough to add up\"\nend\n";
    let error = wasm::run_program(program)
        .expect_err("output past the playground's limit must be a failure");
    assert!(
        error.contains("limit") && error.to_lowercase().contains("output"),
        "the refusal must say what it is about, got: {error}"
    );
    assert!(
        error.contains(&wasm::MAX_OUTPUT_BYTES.to_string()),
        "the refusal must name the limit it hit, got: {error}"
    );
}

/// `say` does not write its line as it goes — the interpreter buffers the lines
/// and writes them at the end — so the bound on output has to be charged where
/// the lines are *buffered*, not only where the bytes are finally written. It
/// was not: a `repeat` of [`MAX_ITERATIONS`] long lines grew a `Vec<String>`
/// without limit, and the program was stopped by the output limit only after it
/// had already spent the memory the limit exists to bound.
///
/// Two things are asserted, because either alone would pass for the wrong
/// reason. The limit is the interpreter's own, so a Redblue program can catch
/// it like any other — which is only true if it is raised during execution and
/// not at the end. And a program just under the limit still prints every line,
/// so the bound is a bound and not a smaller cap wearing the same number.
#[test]
fn edge_say_output_is_bounded_while_it_is_still_buffered() {
    let line = "a line long enough that a million of them add up to a great deal";
    let per_line = line.len() + 1;
    let fills = wasm::MAX_OUTPUT_BYTES / per_line;
    assert!(
        fills < 1_000_000 && fills > 10_000,
        "the program below has to be one the limit stops before the loop cap: {fills} lines"
    );

    // The run *succeeds*, which is the whole of the catchability claim: the
    // limit is the interpreter's own `Error::Limit`, so the `catch` around it
    // handles it and nothing propagates. Without the charge there is no limit
    // here at all — the loop would finish, buffer a million lines, and the
    // output below would be 65 MB rather than 4.
    let flood = format!(
        "try\n\
         \x20   repeat 1000000 times\n\
         \x20       say \"{line}\"\n\
         \x20   end\n\
         catch error\n\
         \x20   set caught to 1\n\
         end\n"
    );
    let flooded = wasm::run_program(&flood).expect("a caught limit must not fail the run");

    // Exactly the lines that fitted, and not the ragged remainder: the bound is
    // charged per line as the line is buffered, with its newline, so the buffer
    // is `fills * per_line` bytes and no more. An unbounded buffer would be
    // `1000000 * per_line`.
    assert_eq!(
        flooded.len(),
        fills * per_line,
        "the buffered lines must be exactly the ones the limit allowed"
    );
    assert!(
        flooded.ends_with(&format!("{line}\n")),
        "every buffered line must be the program's own, untruncated"
    );

    // And the charge sticks: a `catch` takes the limit, and then nothing can be
    // said at all. Letting a shorter line through into whatever room the last
    // long line happened to leave would make what a program can still print
    // depend on how its earlier lines divided the limit.
    let after = wasm::run_program(&format!("{flood}say \"after\"\n"))
        .expect_err("nothing may be said once the limit has stopped a say");
    assert!(
        after.contains(&wasm::MAX_OUTPUT_BYTES.to_string()),
        "the refusal after a caught limit must be the same limit, got: {after}"
    );

    // The same limit, named, when nothing catches it.
    let refused = wasm::run_program(&format!("repeat 1000000 times\n    say \"{line}\"\nend\n"))
        .expect_err("an uncaught output limit must fail the run");
    assert!(
        refused.contains(&wasm::MAX_OUTPUT_BYTES.to_string())
            && refused.to_lowercase().contains("limit"),
        "the refusal must name the limit it hit, got: {refused}"
    );

    // And a program that fits still prints all of it, so the charge is a charge
    // and not a smaller ceiling: 1000 lines of 70-odd bytes is far inside the
    // limit and every one of them has to arrive.
    let small = wasm::run_program(&format!("repeat 1000 times\n    say \"{line}\"\nend\n"))
        .expect("a program inside the limit must run");
    assert_eq!(
        small.len(),
        1000 * per_line,
        "every line inside the limit must still be printed"
    );
}

/// The in-memory filesystem, which is what `files.*` reaches on
/// `wasm32-unknown-unknown` and what `cargo test` on this host never touches if
/// it is behind a `#[cfg]`. It is compiled on every target precisely so this
/// test can run, and the behaviours pinned here are the ones a `BTreeMap` gets
/// wrong: a missing file is an error rather than an empty read, `append`
/// creates the way the disk does, and `bytes.write` lands in the same filesystem
/// `files.read` reads.
#[test]
fn the_in_memory_filesystem_behaves_like_the_one_it_replaces() {
    use redblue::vfs::memory;

    let _serialised = memory_filesystem();
    memory::clear();
    let path = "/playground/note.txt";
    let copy = "/playground/note.copy";
    let missing = "/playground/never-written.txt";

    assert!(
        !memory::exists(path),
        "a fresh filesystem has nothing in it"
    );

    memory::write(path, "first").expect("write should succeed");
    assert!(memory::exists(path));
    assert_eq!(
        memory::read_to_string(path).expect("read should succeed"),
        "first"
    );

    memory::append(path, " second").expect("append should succeed");
    assert_eq!(
        memory::read_to_string(path).expect("read should succeed"),
        "first second",
        "append must add to what is there"
    );

    // `append` creates, because `create(true).append(true)` does.
    memory::append("/playground/made-by-append", "x").expect("append should create");
    assert_eq!(
        memory::read_to_string("/playground/made-by-append").expect("read should succeed"),
        "x"
    );

    assert_eq!(memory::copy(path, copy).expect("copy should succeed"), 12);
    assert!(memory::exists(copy));
    memory::rename(copy, "/playground/note.moved").expect("rename should succeed");
    assert!(!memory::exists(copy));
    assert!(memory::exists("/playground/note.moved"));

    memory::remove_file(path).expect("delete should succeed");
    assert!(!memory::exists(path));

    // Bytes and text are one filesystem. `bytes.write` used to call `std::fs`
    // directly, which on the playground does not exist at all.
    memory::write_bytes("/playground/blob", &[0u8, 1, 2, 255]).expect("write should succeed");
    let error = memory::read_to_string("/playground/blob")
        .expect_err("bytes that are not text are not text");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);

    // A path nobody wrote is a failure, not an empty file — the edge a map
    // lookup gets wrong.
    let missing_error = memory::read_to_string(missing).expect_err("a missing file must fail");
    assert_eq!(missing_error.kind(), std::io::ErrorKind::NotFound);
    assert!(
        missing_error
            .to_string()
            .contains("No such file or directory"),
        "the message must be the one `std::fs` gives, got: {missing_error}"
    );
    assert!(
        memory::remove_file(missing).is_err(),
        "deleting it must fail too"
    );

    memory::clear();
}

/// `bytes.write` has to reach the same filesystem `files.write` does. Natively
/// that is `std::fs`, and the playground's is the map above; either way the
/// round trip is the assertion, because the two used to be different paths.
#[test]
fn bytes_write_and_files_read_agree_on_where_the_bytes_went() {
    let dir = temp_dir("wasm_bytes_round_trip");
    let path = dir.join("blob.bin");
    let source = format!(
        "bytes.write(\"{path}\", [104, 105])\n\
         say files.read(\"{path}\")\n\
         say files.exists(\"{path}\")\n\
         files.delete(\"{path}\")\n\
         say files.exists(\"{path}\")\n",
        path = path.display()
    );

    let out =
        wasm::run_program(&source).unwrap_or_else(|e| panic!("the two write paths must meet: {e}"));
    assert_eq!(
        out, "hi\nyes\nno\n",
        "bytes.write must write where files.read reads"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The playground refuses the three things it has no platform for, and refusing
/// them cleanly is the whole point — the alternative was a panic or a hang that
/// killed the module instance rather than failing one run.
///
/// These refusals are *wasm-only*, so they cannot be asserted here: natively
/// `network.get` reaches a real client, `time.sleep` really waits and
/// `input` really reads a terminal, and a test that asserted they fail would be
/// asserting the opposite of what `rb` does. `wasm/check-examples.js` runs them
/// against the built module, where they are true. What is assertable from here
/// is that the native answers are unchanged — a sleep of zero returns, an
/// argument `network` cannot use is still refused the way it always was.
#[test]
fn edge_the_platform_builtins_still_answer_natively() {
    assert_eq!(
        wasm::run_program("time.sleep(0)\nsay \"awake\"\n").expect("a zero sleep is a sleep"),
        "awake\n"
    );

    // Argument refusal happens before the platform is consulted, so it is the
    // same answer in both builds and is assertable from here.
    let refused = wasm::run_program("say network.get(42)")
        .expect_err("a non-text URL is refused before any request");
    assert!(
        refused.to_lowercase().contains("url") || refused.to_lowercase().contains("text"),
        "the refusal must name the argument, got: {refused}"
    );
}

/// One `rb_alloc` used to be able to ask for nearly two gigabytes — the length
/// crosses the ABI as an `i32`, and nothing between the call and `vec![0u8;
/// len]` said no. That is one host call from exhausting the module, and on a
/// native build the host process too.
///
/// The refusal has to be cheap to observe as well as safe, so the reason is
/// left in the error channel where a page already shows failures, and the
/// check is *before* the allocation rather than after it. A `len` at the limit
/// is still served, so this is a bound and not a smaller cap with the same
/// number.
#[test]
fn edge_an_allocation_past_the_source_limit_is_refused_before_anything_is_allocated() {
    let _channels = host_channels();
    assert_eq!(
        wasm::rb_alloc(wasm::MAX_SOURCE_BYTES as i32 + 1),
        std::ptr::null_mut(),
        "an allocation past the source limit must be refused, not attempted"
    );
    assert!(
        read_error().contains(&wasm::MAX_SOURCE_BYTES.to_string()),
        "the refusal must name the limit it hit"
    );

    // The largest legal request is a request, not a refusal. It is freed
    // immediately; nothing is written into it and it is never run.
    let at_limit = wasm::rb_alloc(wasm::MAX_SOURCE_BYTES as i32);
    assert!(
        !at_limit.is_null(),
        "a request at exactly the limit must be served"
    );
    assert_eq!(wasm::rb_release(at_limit.cast_const()), 0);

    // And the module still works after a refusal.
    assert_eq!(
        wasm::run_program("say \"still here\"").expect("the module must survive"),
        "still here\n"
    );
}

/// A release that named "the most recent allocation" could not free a host's
/// first buffer once a second had been made, and could not tell a second
/// release of the same buffer from a release of a second one. Both are ordinary
/// things for a host to do, and both are what this pins: release is by the
/// address the host was handed, in any order, and a second release of one
/// address is told apart from a release of two.
#[test]
fn edge_an_allocation_is_released_by_its_own_address_and_only_once() {
    let _channels = host_channels();
    let first = wasm::rb_alloc(16);
    let second = wasm::rb_alloc(32);
    let third = wasm::rb_alloc(48);
    for pointer in [first, second, third] {
        assert!(!pointer.is_null(), "rb_alloc returned null");
    }
    assert_ne!(first, second, "two live allocations have two addresses");
    assert_ne!(second, third, "two live allocations have two addresses");

    // Out of order: the first, then the last. Neither is "the most recent",
    // and both used to be unreleasable.
    assert_eq!(wasm::rb_release(first.cast_const()), 0);
    assert_eq!(wasm::rb_release(third.cast_const()), 0);
    assert_eq!(wasm::rb_release(second.cast_const()), 0);

    // Every one of them released once, so every one of them is refused a
    // second time — which is the answer a host needs to tell a double
    // release from a second release.
    assert_eq!(wasm::rb_release(first.cast_const()), -1);
    assert_eq!(wasm::rb_release(second.cast_const()), -1);
    assert_eq!(wasm::rb_release(third.cast_const()), -1);

    // A pointer that names nothing at all is refused rather than matched.
    let not_allocated = 0x1000usize as *const u8;
    assert_eq!(wasm::rb_release(not_allocated), -1);

    // A released buffer goes back to the arena and is handed out again,
    // which is what stops a host that runs a thousand programs growing it
    // a thousand times — and it is still its own buffer to release.
    let reused = wasm::rb_alloc(16);
    assert!(!reused.is_null(), "a released buffer must be reusable");
    assert_eq!(wasm::rb_release(reused.cast_const()), 0);

    // A zero-length reservation holds nothing and has nothing to release.
    // It is not refused — it is served — because an empty program is a
    // program; it simply is not an arena entry.
    let empty = wasm::rb_alloc(0);
    assert!(!empty.is_null(), "an empty program still needs an answer");
    assert_eq!(unsafe { wasm::rb_run(empty, 0) }, 0);
    assert_eq!(wasm::rb_release(empty.cast_const()), -1);
}

/// The read half of the ABI is a read *half* of a protocol, and taking the lock
/// in each accessor is not the same as taking it across the protocol: an
/// address, then a length, then the bytes between them are three moments, and a
/// second `rb_run` landing between two of them hands the host another program's
/// output. Two answers, and the second is the one that closes it.
///
/// - `rb_read_output` / `rb_read_error` copy the bytes out from inside the
///   module with the lock held across the whole copy, so a read is never *torn*.
/// - `rb_run_into` is the run and both copies under **one** lock, so there is no
///   second moment for anything to land in and the bytes are provably the ones
///   that run produced.
///
/// The concurrency test below is not decoration: it failed against the first of
/// those alone, which is how the second one came to exist. Eight threads each
/// run and read twenty-five times, and every one of the two hundred reads must
/// come back with the output of the run that thread just made.
#[test]
fn edge_the_output_is_copied_out_under_the_lock_rather_than_read_through_a_pointer() {
    let _channels = host_channels();
    let source = b"say \"one channel\"\n";

    // The two-call form: a run, then copies. Whole bytes, one run's.
    unsafe {
        let input = wasm::rb_alloc(source.len() as i32);
        assert!(!input.is_null(), "rb_alloc returned null");
        std::ptr::copy_nonoverlapping(source.as_ptr(), input, source.len());
        assert_eq!(wasm::rb_run(input, source.len() as i32), 0);
        assert_eq!(wasm::rb_release(input.cast_const()), 0);

        assert_eq!(read_output(), "one channel\n");

        // A copy shorter than the channel takes exactly what fits rather than
        // overrunning the host's buffer.
        let mut one = [0u8; 1];
        assert_eq!(wasm::rb_read_output(one.as_mut_ptr(), 1), 1);
        assert_eq!(
            *one.first().expect("the first byte was written"),
            b'o',
            "the copy must start at the beginning of the channel"
        );

        // A null destination cannot be written to, so it is a refusal rather
        // than a crash — and the length query, which writes nothing, answers
        // with a null all the same.
        assert_eq!(wasm::rb_read_output(std::ptr::null_mut(), 16), -1);
        assert_eq!(wasm::rb_read_error(std::ptr::null_mut(), 16), -1);
        assert_eq!(
            wasm::rb_read_output(std::ptr::null_mut(), -1),
            "one channel\n".len() as i32,
            "a query must answer without a destination"
        );
        assert_eq!(wasm::rb_read_error(std::ptr::null_mut(), -1), 0);
    }

    // The one-call form: the same program, the same bytes, and nothing in
    // between that could go wrong.
    let (status, output, error) = run_into("say \"one channel\"\n");
    assert_eq!(status, 0);
    assert_eq!(output, "one channel\n");
    assert_eq!(error, "");

    let threads: Vec<_> = (0..8)
        .map(|marker| {
            std::thread::spawn(move || {
                for _ in 0..25 {
                    let (status, output, error) = run_into(&format!("say \"thread {marker}\"\n"));
                    assert_eq!(status, 0, "a valid program must run");
                    assert_eq!(error, "", "a valid program must leave no message");
                    assert_eq!(
                        output,
                        format!("thread {marker}\n"),
                        "a host must read back the run it just made"
                    );
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().expect("every thread must finish");
    }
}

/// Runs `source` through [`redblue::wasm::rb_run_into`] — one call, one lock — and
/// returns `(status, output, error)`.
///
/// Every buffer is sized from the module's own published limits, which is what
/// makes a short copy impossible here and is why a shortfall could not be hidden
/// if there were one.
fn run_into(source: &str) -> (i32, String, String) {
    unsafe {
        let bytes = source.as_bytes();
        let input = wasm::rb_alloc(bytes.len() as i32);
        assert!(!input.is_null(), "rb_alloc returned null");
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), input, bytes.len());

        let out_cap = wasm::MAX_OUTPUT_BYTES as i32;
        let err_cap = wasm::MAX_ERROR_BYTES as i32;
        let out_buffer = wasm::rb_alloc(out_cap);
        let err_buffer = wasm::rb_alloc(err_cap);
        let out_cell = wasm::rb_alloc(4);
        let err_cell = wasm::rb_alloc(4);
        for (buffer, cell) in [(out_buffer, out_cell), (err_buffer, err_cell)] {
            assert!(!buffer.is_null(), "rb_alloc returned null");
            assert!(!cell.is_null(), "rb_alloc returned null");
        }

        std::ptr::write(out_cell.cast::<i32>(), out_cap);
        std::ptr::write(err_cell.cast::<i32>(), err_cap);
        let status = wasm::rb_run_into(
            input,
            bytes.len() as i32,
            out_buffer,
            out_cell.cast::<i32>(),
            err_buffer,
            err_cell.cast::<i32>(),
        );

        let written_out = std::ptr::read(out_cell.cast::<i32>());
        let written_err = std::ptr::read(err_cell.cast::<i32>());
        assert_eq!(
            written_out as usize,
            std::slice::from_raw_parts(out_buffer, written_out as usize).len(),
            "the reported length must be the bytes that were copied"
        );
        let output = String::from_utf8(
            std::slice::from_raw_parts(out_buffer, written_out as usize).to_vec(),
        )
        .expect("output is UTF-8");
        let error = String::from_utf8(
            std::slice::from_raw_parts(err_buffer, written_err as usize).to_vec(),
        )
        .expect("a rendered error is UTF-8");

        for pointer in [input, out_buffer, err_buffer, out_cell, err_cell] {
            assert_eq!(
                wasm::rb_release(pointer.cast_const()),
                0,
                "every buffer this test asked for must be releasable"
            );
        }
        (status, output, error)
    }
}

/// A rendered error is the one thing that *can* be longer than its buffer — the
/// source line it quotes is any length a file can hold — and it is truncated
/// visibly rather than failing the run. This is the path that makes
/// `HostBuffer::set`'s truncation live: output past the limit is refused before
/// it reaches a buffer, so nothing else in the module exercises it.
#[test]
fn edge_an_error_too_long_for_its_buffer_is_truncated_and_says_so() {
    let _channels = host_channels();
    let long_name = "n".repeat(200_000);
    let source = format!("say {long_name}\n");
    unsafe {
        let input = wasm::rb_alloc(source.len() as i32);
        assert!(!input.is_null(), "rb_alloc returned null");
        std::ptr::copy_nonoverlapping(source.as_ptr(), input, source.len());
        assert_ne!(
            wasm::rb_run(input, source.len() as i32),
            0,
            "an undeclared name is a failure"
        );
        assert_eq!(wasm::rb_release(input.cast_const()), 0);
    }

    // The channel is 64 KiB and the message is longer than that, so this is the
    // one place in the module where truncation is the answer rather than a
    // failure: output past its limit is refused before it reaches a buffer, but
    // a rendered error quotes a source line of any length and has nowhere else
    // to go.
    let mut bytes = vec![0u8; 64 * 1024];
    unsafe {
        let len = wasm::rb_read_error(bytes.as_mut_ptr(), bytes.len() as i32);
        assert_eq!(len as usize, bytes.len(), "the copy must fill the channel");
    }
    let text = String::from_utf8(bytes).expect("a rendered error is UTF-8");
    assert!(
        text.starts_with("AnalyzerError") && text.contains("Unknown variable"),
        "the truncated message must still name the failure, got the first 200 bytes: {:?}",
        &text[..200.min(text.len())]
    );
    assert!(
        text.ends_with("[truncated: the message does not fit the buffer]"),
        "a truncated message must say that it is one, got the last 80 bytes: {:?}",
        &text[text.len().saturating_sub(80)..]
    );

    // And the module is still usable afterwards.
    assert_eq!(
        wasm::run_program("say \"fine\"").expect("the module must survive"),
        "fine\n"
    );
}

/// The interpreter runs a program on a thread sized for the configured call
/// depth, because the depth counter assumes that much stack is reachable and a
/// stack that is not turns the catchable `Limit` into a native stack overflow.
/// When the thread cannot be had, running the program on the caller's stack is
/// exactly the case that assumption does not cover — so it is refused, on the
/// `Limit` channel a Redblue program can catch, instead of attempted.
///
/// The limit here is a call depth, not a size: `1 << 40` asks for a stack of
/// 2^58 bytes, which no thread can be given.
#[test]
fn edge_a_run_that_cannot_be_given_its_stack_is_refused_rather_than_run_unprotected() {
    let program = parse("say \"this must not run\"");
    let (mut vm, result) = redblue::run_isolated_with_depth(&program, 1 << 40);

    match result {
        Err(error) => {
            assert!(
                error.is_resource_limit(),
                "a refused run must be a resource limit, not an ordinary failure: {error}"
            );
            assert!(
                error.to_string().contains("stack"),
                "the refusal must say what could not be had, got: {error}"
            );
        }
        Ok(_) => panic!("a run with no stack for it must not be attempted"),
    }
    assert!(
        vm.take_output().is_empty(),
        "a refused run must not have produced any output"
    );

    // The ordinary limit still runs, so this is a refusal and not a
    // configuration that refuses everything. The program says nothing: a
    // `say` from a bare `Vm::run` on this thread would go through the same
    // global output boundary every other run in this process uses, and land in
    // whatever capture another test has open.
    let runnable = parse("set total to 2 + 3");
    let (_vm, result) = redblue::run_isolated_with_depth(&runnable, redblue::MAX_CALL_DEPTH);
    assert!(
        result.is_ok(),
        "a runnable depth must still run, got {result:?}"
    );
}

/// `files.exists` answers for a directory as well as a file, and the in-memory
/// filesystem a flat map has to say so itself: the native backend is
/// `Path::exists`, which is `true` for a directory, and a `contains_key` lookup
/// is not. `files.exists("modules")` would have answered `no` in the playground
/// and `yes` in the binary, on the same program.
///
/// A directory here is implied by what is in it, which is the only way a flat
/// map can have one — and it is why the trailing separator is the whole of the
/// rule, so `/playground` is a directory holding `/playground/note.txt` while
/// `/playground/note` is nothing at all.
#[test]
fn edge_the_in_memory_filesystem_answers_for_directories_too() {
    use redblue::vfs::memory;

    let _serialised = memory_filesystem();
    memory::clear();

    let dir = "/wasm-directories";
    let inside = "/wasm-directories/inner/note.txt";
    let sibling = "/wasm-directories-inner.txt";

    assert!(!memory::exists(dir), "nothing has been written yet");
    memory::write(inside, "x").expect("write should succeed");

    assert!(memory::exists(inside), "the file itself exists");
    assert!(
        memory::exists(dir),
        "a directory the native Path::exists calls true must be true here too"
    );
    assert!(
        memory::exists("/wasm-directories/inner"),
        "every directory on the way to a file is a directory"
    );
    assert!(
        memory::exists(&format!("{dir}/")),
        "a trailing separator names the same directory"
    );

    // The separator is what separates them: `/wasm-directories` is a
    // directory holding `/wasm-directories/inner/note.txt` and says nothing
    // about the file `/wasm-directories-inner.txt`, which is a different path
    // that was never written. This is the edge a prefix test without one gets
    // wrong, and it is why the rule is a separator and not a string prefix.
    assert!(!memory::exists(sibling));
    assert!(!memory::exists("/wasm-directories/note"));
    assert!(
        !memory::exists("/wasm-directories/note.tx"),
        "a prefix of a name is not a name"
    );
    assert!(!memory::exists(""), "the empty path names nothing");

    // Reading a directory is still the refusal it is on a disk: there is no
    // such *file*, whatever the path is shaped like.
    let refused = memory::read_to_string(dir).expect_err("a directory is not a file");
    assert_eq!(refused.kind(), std::io::ErrorKind::NotFound);

    memory::clear();
}

/// A file that does not decode is the same failure with the same words in both
/// builds. The in-memory filesystem used to append the path to `std`'s message
/// while `std::fs` does not, and the caller in `runtime.rs` prefixes the path
/// itself — so the playground printed it twice and the binary once, for one
/// program and one mistake.
///
/// The exact sentence is asserted rather than a substring of it: this is the
/// whole of the message, and anything added to it is the divergence again.
#[test]
fn edge_bytes_that_are_not_text_say_the_same_thing_in_both_builds() {
    use redblue::vfs::memory;

    let _serialised = memory_filesystem();
    memory::clear();

    const STD_SENTENCE: &str = "stream did not contain valid UTF-8";
    memory::write_bytes("/not-text", &[0xff, 0xfe]).expect("write should succeed");

    let memory_error = memory::read_to_string("/not-text").expect_err("bytes are not text");
    assert_eq!(
        memory_error.to_string(),
        STD_SENTENCE,
        "the in-memory filesystem must say exactly what std::fs says, path and all"
    );

    // And the message the host actually reads — which is the one with the path
    // prefixed by the caller — has the path in it once, not twice. This is the
    // native half of the same rule, through `std::fs`.
    let dir = temp_dir("wasm_not_text");
    let path = dir.join("blob.bin");
    std::fs::write(&path, [0xffu8, 0xfe]).expect("the native file should be writable");
    let native = wasm::run_program(&format!(
        "say files.read(\"{path}\")\n",
        path = path.display()
    ))
    .expect_err("bytes that are not text are not text");
    assert!(
        native.contains(STD_SENTENCE),
        "the message must say what is wrong with the file, got: {native}"
    );
    assert_eq!(
        native.matches(&path.display().to_string()).count(),
        1,
        "the path must appear once, from the caller's own prefix: {native}"
    );

    memory::clear();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Lexes and parses `source`. Used by the tests that hand a `Program` to
/// [`redblue::run_isolated_with_depth`] rather than to the whole pipeline.
#[track_caller]
fn parse(source: &str) -> redblue::parser::Program {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    redblue::parser::parse(tokens).expect("source should parse")
}

/// A directory for this test's files, named after the test so two of them
/// running in parallel cannot collide. Under `target/tmp/`, never `/tmp`.
fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/tmp")
        .join(name);
    std::fs::create_dir_all(&dir)
        .unwrap_or_else(|e| panic!("cannot create {}: {e}", dir.display()));
    dir
}

/// The in-memory filesystem is one store for the whole process, so every test
/// that writes into it has to hold this for as long as it is looking. Without
/// it two such tests running in parallel — which `cargo test` does — would
/// empty each other's filesystem and fail for reasons that have nothing to do
/// with what they are testing.
fn memory_filesystem() -> std::sync::MutexGuard<'static, ()> {
    static STORE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    STORE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Holds the module's host channels still while a test walks the
/// alloc → run → read protocol.
///
/// There is one output channel and one error channel for the whole process, and
/// `rb_run` writes both — so a test that reads them has to have made the run it
/// is reading, and `cargo test` runs tests in parallel threads that would each
/// be reading the last one's. `rb_read_output` and `rb_read_error` stop a read
/// from *overlapping* a write; this stops the tests here from being each other's
/// writer, which is a different problem and the tests' own.
///
/// It is deliberately not held by the concurrency test's own threads: those are
/// the ones that have to contend on the module's lock, and holding this around
/// the test as a whole keeps every other test out while they do.
fn host_channels() -> std::sync::MutexGuard<'static, ()> {
    static CHANNELS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    CHANNELS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Reads a channel through the copying accessor, which is what a host is meant
/// to use: the bytes come out from *inside* the module, with the lock that
/// serialises it against a run held across the whole copy, so no other run can
/// rewrite them between asking the length and reading them.
///
/// `cap` below zero is the length query; anything else is the copy.
fn read_channel(read: unsafe extern "C" fn(*mut u8, i32) -> i32) -> String {
    unsafe {
        let len = read(std::ptr::null_mut(), -1);
        assert!(len >= 0, "a channel must answer its length, not a refusal");
        if len == 0 {
            return String::new();
        }
        let mut bytes = vec![0u8; len as usize];
        let copied = read(bytes.as_mut_ptr(), len);
        assert_eq!(
            copied, len,
            "the copy must be as long as the length it asked for"
        );
        String::from_utf8(bytes).expect("a host channel carries the program's own UTF-8 bytes")
    }
}

/// The last run's output, copied out.
fn read_output() -> String {
    read_channel(wasm::rb_read_output)
}

/// The last run's error message, copied out.
fn read_error() -> String {
    read_channel(wasm::rb_read_error)
}
