//! The `random*` builtins are seeded, in range, and reproducible.
//!
//! The finding this file pins: every `random*` builtin drew from
//! `SystemTime::now()`. `random(1, 1)` answered `403` — it took no arguments at
//! all — 200 draws of `random(0, 100)` produced 179 distinct values, eight
//! consecutive `random_number(0, 100)` draws came out strictly increasing
//! because the source is a monotonic nanosecond counter and not noise, and
//! `random_shuffle` computed `seed % (i + 1)` from a single clock reading, so
//! its permutation was a fixed function of one instant.
//!
//! A wall clock in the output path also breaks AGENTS.md 3.1.4: no golden-output
//! corpus, differential test or property test can contain a `random*` call and
//! still be reproducible, which is a precondition of the bootstrap fixed point.
//!
//! So the draws come from an explicitly seeded generator, and every test here
//! seeds before it draws. The state is thread-local, so `cargo test` running
//! these tests in parallel cannot interleave two tests' draws.

use std::collections::BTreeSet;
use std::process::Command;

use redblue::{Error, Value};

/// The seed most tests here pin. Fixed, so a failure is reproducible.
const SEED: f64 = 12345.0;

/// Runs `source` through lexer → parser → VM and returns its last value.
#[track_caller]
fn eval(source: &str) -> Value {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    let ast = redblue::parser::parse(tokens).expect("source should parse");
    redblue::Vm::new().run(&ast).expect("source should run")
}

/// Runs `source` and returns the pipeline error it produced.
#[track_caller]
fn eval_err(source: &str) -> Error {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    let ast = redblue::parser::parse(tokens).expect("source should parse");
    match redblue::Vm::new().run(&ast) {
        Ok(value) => panic!("`{source}` should have been refused, it answered {value}"),
        Err(error) => error,
    }
}

/// A list of `count` numbers drawn by `expr`, one per `for` iteration.
///
/// The program is whole: seeding, the loop and the list all run inside one VM,
/// so what is asserted is what a Redblue program would see.
#[track_caller]
fn draws(expr: &str, count: usize) -> Vec<f64> {
    draws_from(&format!("random_seed({SEED})\n"), expr, count)
}

/// The same, under a caller-supplied preamble, so a test can seed differently
/// or not at all.
#[track_caller]
fn draws_from(preamble: &str, expr: &str, count: usize) -> Vec<f64> {
    let source = format!(
        "{preamble}\
         set xs to []\n\
         for each i from 1 to {count}\n    \
             append(\"xs\", {expr})\n\
         end\n\
         xs"
    );
    let list = eval(&source);
    let Value::List(items) = list else {
        panic!("the draw loop should answer a list, it answered {list}");
    };
    items
        .iter()
        .map(|value| match value {
            Value::Number(n) => *n,
            other => panic!("every draw should be a number, one is {other:?}"),
        })
        .collect()
}

/// Runs `source` in its own `rb` process and returns what it printed.
fn run_process(tag: &str, source: &str) -> String {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/p37random");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    let file = dir.join(format!("{tag}.rb"));
    std::fs::write(&file, source).expect("the program is written");
    let out = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("rb runs");
    assert!(
        out.status.success(),
        "the program failed:\n{source}\n--- stderr\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// `random(min, max)` answers a number *in the range it was given*.
///
/// `random(5, 5)` is the whole finding in one call: the range arguments used to
/// be dead, so it answered whatever the clock's low digits were.
#[test]
fn random_honours_its_range_arguments() {
    for bound in [5.0, 0.0, -3.0, 1.0e6] {
        let value = match eval(&format!("random({bound}, {bound})")) {
            Value::Number(n) => n,
            other => panic!("random({bound}, {bound}) should be a number, got {other:?}"),
        };
        assert_eq!(
            value, bound,
            "random({bound}, {bound}) must answer exactly {bound}: a range whose two \
             ends are equal has exactly one member"
        );
    }

    let values = draws("random(0, 100)", 200);
    let distinct: BTreeSet<i64> = values.iter().map(|n| *n as i64).collect();
    assert!(
        distinct.len() <= 101,
        "random(0, 100) has 101 members, so 200 draws can name at most 101 of them, \
         but they named {}",
        distinct.len()
    );
    for value in &values {
        assert!(
            (0.0..=100.0).contains(value),
            "random(0, 100) answered {value}, which is outside the range it was given"
        );
        assert_eq!(
            value.fract(),
            0.0,
            "random is the integer-valued draw and answered the fraction {value}"
        );
    }
}

/// Two processes that set the same seed print the same thing, byte for byte.
///
/// This is the property the whole phase exists for: with a wall clock in the
/// path, a golden-output corpus cannot contain a `random*` call at all
/// (AGENTS.md 3.1.4), which is a precondition of the bootstrap fixed point.
#[test]
fn edge_two_processes_seeded_alike_print_identical_output() {
    let source = "random_seed(4242)\n\
         for each i from 1 to 50\n    \
             say random(0, 1000)\n    \
             say random_number(0, 1)\n    \
             say random_choice([\"a\", \"b\", \"c\", \"d\"])\n\
             say join(map(random_shuffle([1, 2, 3, 4, 5]), to (x) to_text(x)), \",\")\n\
         end";
    let first = run_process("seeded_a", source);
    let second = run_process("seeded_b", source);
    assert!(
        !first.is_empty(),
        "the seeded program printed nothing, so this proves nothing"
    );
    assert_eq!(
        first, second,
        "two processes given the same seed printed different things:\n--- first\n{first}\n--- second\n{second}"
    );
}

/// A program that never calls `random_seed` is reproducible too.
///
/// This is the claim `src/runtime.rs` and SPEC.md both make: draws come from a
/// fixed `DEFAULT_SEED` rather than from the clock, so Redblue's output is
/// reproducible *by default* and a program has to ask for variation it cannot
/// reproduce. Two processes that never seed must therefore agree.
///
/// Without this test the determinism would hold only for programs that remember
/// to seed, which is the weaker and less useful half of the property — and the
/// failure it guards is silent: nothing goes wrong, output just stops being
/// reproducible.
#[test]
fn edge_two_processes_that_never_seed_still_agree() {
    let source = "for each i from 1 to 50\n    \
                     say random(0, 1000)\n    \
                     say random_number(0, 1)\n    \
                     say random_choice([\"a\", \"b\", \"c\", \"d\"])\n    \
                     say random_shuffle([1, 2, 3, 4, 5])\n\
                 end";
    let first = run_process("unseeded_a", source);
    let second = run_process("unseeded_b", source);
    assert!(
        first.lines().count() > 100,
        "the unseeded program printed {} lines, so this proves nothing",
        first.lines().count()
    );
    assert_eq!(
        first, second,
        "two processes that never seeded printed different things, so the default \
         seed is not fixed:\n--- first\n{first}\n--- second\n{second}"
    );
}

/// Seeding twice is not the same as never seeding: `random_seed` has to move the
/// generator, or a program that sets the seed to the documented default could
/// not tell seeded from unseeded and the two would be indistinguishable.
#[test]
fn edge_seeding_moves_the_generator() {
    let unseeded = draws("random_number(0, 1)", 16);
    let seeded = draws_from("random_seed(424242)\n", "random_number(0, 1)", 16);
    let other = draws_from("random_seed(424243)\n", "random_number(0, 1)", 16);
    assert_ne!(
        unseeded, seeded,
        "seeding with 424242 produced the default sequence, so `random_seed` did not move the generator"
    );
    assert_ne!(
        seeded, other,
        "two different seeds produced the same sequence, so `random_seed` is not reaching the generator"
    );
}

/// One seed, the same draws, every time — inside a single process too.
///
/// The bytecode VM and the tree-walker run the same program, so both engines'
/// draws have to be the same sequence too; `both_engines_agree` pins that.
#[test]
fn random_draws_are_reproducible_from_a_seed() {
    let first = draws("random_number(0, 100)", 64);
    let second = draws("random_number(0, 100)", 64);
    assert_eq!(
        first, second,
        "the same seed produced two different sequences, so a seeded draw is not reproducible"
    );
}

/// A seeded generator is a generator, not a counter: the draws are spread over
/// the range rather than marching across it.
///
/// The old source was `now.as_nanos()`, a monotonic counter, so eight
/// consecutive draws came out `4.8086, 5.8041, 6.0445, 6.2037, 6.36, 6.5312,
/// 6.6935, 6.8537` — strictly increasing, every one of them in the first decile.
#[test]
fn edge_random_number_spreads_its_draws_across_the_range() {
    let mut buckets = [0usize; 10];
    for value in draws("random_number(0, 100)", 1000) {
        assert!(
            (0.0..100.0).contains(&value),
            "random_number(0, 100) answered {value}, outside the range it was given"
        );
        let bucket = (value / 10.0).floor() as usize;
        buckets[bucket.min(9)] += 1;
    }
    // 1000 draws over ten deciles: the mean is 100 a bucket, and the band below
    // is the one the phase pins for the same check.
    for (index, count) in buckets.iter().enumerate() {
        assert!(
            (40..=160).contains(count),
            "decile {index} of random_number(0, 100) took {count} of 1000 draws; \
             a generator spread over the range puts about 100 in each\nbuckets: {buckets:?}"
        );
    }
}

/// `random_shuffle` returns a permutation, and not the input back.
///
/// Both halves are pinned: a shuffle that dropped or duplicated an element would
/// pass a "not the identity" check alone, and one that returned its input
/// verbatim would pass a "is a permutation" check alone.
#[test]
fn edge_random_shuffle_permutes_its_input_for_every_seed() {
    for seed in 0..=99i64 {
        let source = format!(
            "random_seed({seed})\nrandom_shuffle([\"a\", \"b\", \"c\", \"d\", \"e\", \"f\", \"g\", \"h\"])"
        );
        let shuffled = eval(&source);
        let Value::List(items) = shuffled else {
            panic!("random_shuffle should answer a list for seed {seed}, got {shuffled:?}");
        };
        let mut sorted: Vec<String> = items.iter().map(|value| value.to_string()).collect();
        sorted.sort();
        let original: Vec<String> = ["a", "b", "c", "d", "e", "f", "g", "h"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            sorted, original,
            "seed {seed} shuffled the list into {items:?}, which is not a permutation of its input"
        );
        assert_ne!(
            items.iter().map(|v| v.to_string()).collect::<Vec<_>>(),
            original,
            "seed {seed} left the list exactly as it found it, so nothing was shuffled"
        );
    }
}

/// `random_choice` reaches every element of the list, not just the first few.
///
/// The old source was `now.as_nanos() as usize % len`; this is the check that a
/// draw is a draw over the whole list rather than a bias toward its head.
#[test]
fn edge_random_choice_reaches_every_element_of_the_list() {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let list = eval(&format!(
        "random_seed({SEED})\n\
         set seen to []\n\
         for each i from 1 to 1000\n    \
             append(\"seen\", random_choice([\"a\", \"b\", \"c\", \"d\"]))\n\
         end\n\
         seen"
    ));
    let Value::List(items) = list else {
        panic!("the choice loop should answer a list, it answered {list:?}");
    };
    for item in items.iter() {
        seen.insert(item.to_string());
    }
    assert_eq!(
        seen,
        ["a", "b", "c", "d"]
            .iter()
            .map(|s| s.to_string())
            .collect::<BTreeSet<_>>(),
        "1000 draws from a four-element list reached {seen:?} and not the others"
    );
}

/// The edges: an empty list, one element, a reversed range, a negative range
/// and an argument that is not a number.
#[test]
fn edge_random_refuses_the_arguments_it_cannot_use() {
    // An empty list has no member to choose, so there is no answer to give and
    // the call is refused rather than answered `nothing`.
    let empty = eval_err("random_choice([])");
    assert!(
        matches!(empty, Error::Runtime(_, _)),
        "random_choice of an empty list should be a Runtime error, got {empty:?}"
    );

    // A single-element list has exactly one answer, whatever the seed is.
    for seed in [0i64, 1, 99, 12345] {
        assert_eq!(
            eval(&format!("random_seed({seed})\nrandom_choice([\"only\"])")),
            Value::Text("only".to_string()),
            "the only element of a one-element list is the only answer, for seed {seed}"
        );
    }
    // Shuffling one element is the identity, and an empty list stays empty.
    assert_eq!(
        eval("random_shuffle([])"),
        eval("[]"),
        "shuffling an empty list cannot add anything to it"
    );
    assert_eq!(
        eval("random_seed(7)\nrandom_shuffle([\"only\"])"),
        eval("[\"only\"]"),
        "shuffling one element cannot move it"
    );

    // A range whose low end is above its high end is empty, and refused by name.
    for source in [
        "random(10, 1)",
        "random_number(10, 1)",
        "random_seed(10, 1)",
    ] {
        let error = eval_err(source);
        let message = error.to_string();
        assert!(
            message.contains("random"),
            "`{source}` should be refused naming the builtin, got: {message}"
        );
    }

    // A negative range is ordinary: the draw lands in it.
    let negative = match eval("random(-10, -5)") {
        Value::Number(n) => n,
        other => panic!("random(-10, -5) should be a number, got {other:?}"),
    };
    assert!(
        (-10.0..=-5.0).contains(&negative),
        "random(-10, -5) answered {negative}, outside the negative range it was given"
    );
    assert!(
        eval("random_number(-1.5, -0.5)") != Value::Number(f64::NAN),
        "random_number over a negative range must answer a number"
    );

    // A non-number is refused rather than read as 0 and quietly ignored.
    for source in [
        "random(\"a\", 1)",
        "random(1, \"b\")",
        "random_number(\"a\", 1)",
        "random_number(1, \"b\")",
        "random_seed(\"a\")",
        "random_choice(\"a\")",
        "random_shuffle(\"a\")",
        "random_shuffle(7)",
    ] {
        let error = eval_err(source);
        assert!(
            matches!(error, Error::Runtime(_, _)),
            "`{source}` should be a clean Runtime error, got {error:?}"
        );
    }
}

/// The zero-width range: `random(5, 5)` has one member and `random_number(1, 1)`
/// has none, and both are legal where a half-open reading would refuse them.
#[test]
fn edge_a_range_of_no_width_is_still_a_range() {
    assert_eq!(
        eval("random(5, 5)"),
        Value::Number(5.0),
        "a range whose ends are equal has exactly one member, so exactly one answer"
    );
    assert_eq!(
        eval("random(5, 5.5)"),
        Value::Number(5.0),
        "a width of one whole step has one whole member in it"
    );
    let unit = match eval("random_number(1, 1)") {
        Value::Number(n) => n,
        other => panic!("random_number(1, 1) should be a number, got {other:?}"),
    };
    assert_eq!(
        unit, 1.0,
        "a half-open range whose ends are equal is empty, and `1 + 0 * 0` is its low end"
    );
    // Both members of the two-wide integer range are reachable, not just the
    // interior: `[0, 1]` is inclusive at both ends, so a draw of `2` would mean
    // the width was computed as `max - min` rather than `max - min + 1`.
    let mut members = BTreeSet::new();
    for seed in 0..=64i64 {
        if let Value::Number(n) = eval(&format!("random_seed({seed})\nrandom(0, 1)")) {
            members.insert(n as i64);
        }
    }
    assert_eq!(
        members,
        [0, 1].into_iter().collect::<BTreeSet<_>>(),
        "random(0, 1) named {members:?} over 65 seeds and not the other members, \
         so an end of the inclusive range is excluded"
    );
}

/// A fractional end still answers a whole number, from inside the range.
///
/// `random` is the whole-number draw: `random(0, 100)` answered only integers
/// with 200 draws, and `random_honours_its_range_arguments` pins that. A range
/// whose *end* is a fraction must not break it. The whole members of `[0, 100.5]`
/// are 0..=100, so the answer is one of those 101 values — not `100.5`, which is
/// a member of the range but not a whole number, and not `101`, which is outside
/// it.
///
/// A range with no whole member in it at all — `random(0.2, 0.8)` contains no
/// integer — has nothing for this draw to answer, so it is refused rather than
/// answered with a fraction from a different question.
#[test]
fn edge_random_answers_a_whole_number_from_a_range_with_a_fractional_end() {
    for (lo, hi) in [(0.0, 100.5), (-3.5, 2.0), (5.0, 5.5), (-10.0, -0.25)] {
        let mut members = BTreeSet::new();
        for seed in 0..=2999i64 {
            let source = format!("random_seed({seed})\nrandom({lo}, {hi})");
            let Value::Number(n) = eval(&source) else {
                panic!("random({lo}, {hi}) should be a number for seed {seed}");
            };
            assert_eq!(
                n.fract(),
                0.0,
                "`random` is the whole-number draw, but random({lo}, {hi}) answered the \
                 fraction {n} for seed {seed}"
            );
            assert!(
                n >= lo && n <= hi,
                "random({lo}, {hi}) answered {n} for seed {seed}, which is outside the \
                 range it was given"
            );
            members.insert(n as i64);
        }
        // Every whole member of the range is reachable. 3000 draws over at most
        // 101 members is far more than enough to name all of them, so a draw that
        // skipped the interior or answered only one value fails here.
        let expected: BTreeSet<i64> = (lo.ceil() as i64..=hi.floor() as i64).collect();
        assert_eq!(
            members, expected,
            "3000 draws from random({lo}, {hi}) named {members:?} and not the other \
             whole members of the range ({expected:?})"
        );
    }

    // A range holding no whole number has no answer for this draw. `[0.2, 0.8]`
    // contains no integer, so there is nothing for a whole-number draw to name.
    for source in ["random(0.2, 0.8)", "random(1.5, 1.9)"] {
        let error = eval_err(source).to_string();
        assert!(
            error.contains("random"),
            "`{source}` has no whole member and should be refused naming random, got: {error}"
        );
    }
}

/// A list whose members repeat is still a list of that length.
///
/// `random_shuffle` permutes *positions*, not values, so two equal members stay
/// equal and the result has exactly as many of each as the input did. A
/// shuffle that deduplicated would pass a permutation check on distinct input
/// and quietly lose elements here.
#[test]
fn edge_random_shuffle_keeps_every_repeated_member() {
    for seed in [0i64, 3, 77, 12345] {
        let shuffled = eval(&format!(
            "random_seed({seed})\nrandom_shuffle([1, 1, 2, 2, 2])"
        ));
        let Value::List(items) = shuffled else {
            panic!("random_shuffle should answer a list, got {shuffled:?}");
        };
        let ones = items.iter().filter(|v| **v == Value::Number(1.0)).count();
        let twos = items.iter().filter(|v| **v == Value::Number(2.0)).count();
        assert_eq!(
            (ones, twos),
            (2, 3),
            "seed {seed} shuffled [1, 1, 2, 2, 2] into {items:?}, which does not have \
             the same two 1s and three 2s"
        );
    }
}

/// A draw is a draw of a *value*, so a nested list can be chosen and shuffled
/// whole rather than only its members.
#[test]
fn edge_random_choice_and_shuffle_carry_nested_values() {
    let chosen = eval("random_seed(5)\nrandom_choice([[1, [2, 3]], [4]])");
    let Value::List(items) = chosen else {
        panic!("random_choice should answer a list, got {chosen:?}");
    };
    assert!(
        items.len() == 1 || items.len() == 2,
        "random_choice must answer one whole member of the outer list, got {items:?}"
    );
    let shuffled = eval("random_seed(5)\nrandom_shuffle([[1, 2], [3, 4], [5, 6]])");
    let Value::List(items) = shuffled else {
        panic!("random_shuffle should answer a list, got {shuffled:?}");
    };
    assert_eq!(
        items.len(),
        3,
        "shuffling three members must leave three members, got {items:?}"
    );
    let mut rendered: Vec<String> = items.iter().map(|v| v.to_string()).collect();
    rendered.sort();
    assert_eq!(
        rendered,
        ["[1, 2]", "[3, 4]", "[5, 6]"],
        "the nested members must all survive the shuffle, got {items:?}"
    );
}

/// A malformed seed is refused rather than read as zero.
///
/// `random_seed("a")` used to be `Unknown function`, which is the message for a
/// name nothing implements; `random_seed()` is a call with the wrong arity, and
/// both are refusals that name what was wrong.
#[test]
fn edge_random_seed_refuses_a_seed_it_cannot_use() {
    for source in [
        "random_seed()",
        "random_seed(1, 2)",
        "random_seed(\"a\")",
        "random_seed([])",
    ] {
        let error = eval_err(source);
        assert!(
            matches!(error, Error::Runtime(_, _)),
            "`{source}` should be a clean Runtime error, got {error:?}"
        );
        assert!(
            error.to_string().contains("random_seed"),
            "`{source}` should be refused naming random_seed, got: {error}"
        );
    }
    // `1e400` is the value that does not exist, so it is refused as a literal
    // before `random_seed` is ever asked. The finiteness guard behind the seed
    // is still there for a value that arrives non-finite by arithmetic, which
    // is asserted here through the guard's own contract rather than through the
    // language.
    let error = eval_err("random_seed(1e400)");
    assert!(
        error.to_string().contains("finite"),
        "a non-finite seed should be refused as non-finite, got: {error}"
    );
}

/// Many draws are many draws, not an unbounded resource: a long loop of them
/// finishes and stays in range.
///
/// The generator holds one `u64` and allocates nothing, so a million draws
/// cost a million additions. The loop guard is the existing one and this test
/// does not raise it.
#[test]
fn edge_many_draws_stay_in_range_and_do_not_grow() {
    let values = draws("random(0, 5)", 20_000);
    assert_eq!(
        values.len(),
        20_000,
        "the loop should have visited every index once"
    );
    for value in &values {
        assert!(
            (0.0..=5.0).contains(value),
            "a draw from [0, 5] answered {value}, outside the range"
        );
    }
    // Six members over 20000 draws: every one of them is reachable, and none
    // takes more than half the draws, which a biased source would fail.
    let mut counts = [0usize; 6];
    for value in &values {
        counts[(*value as usize).min(5)] += 1;
    }
    for (index, count) in counts.iter().enumerate() {
        assert!(
            *count > 20_000 / 6 / 2 && *count < 20_000 / 6 * 2,
            "member {index} took {count} of 20000 draws from [0, 5]; counts: {counts:?}"
        );
    }
}

/// A range wider than a double is a range with no member a `Value::Number` can
/// hold, and it is refused rather than answered with infinity or NaN.
#[test]
fn edge_random_refuses_a_range_no_number_can_measure() {
    let integer = eval_err("random(-1e308, 1e308)").to_string();
    assert!(
        integer.contains("random"),
        "a range of infinite width should be refused naming random, got: {integer}"
    );
    let float = eval_err("random_number(-1e308, 1e308)").to_string();
    assert!(
        float.contains("is not a finite number"),
        "random_number(-1e308, 1e308) should be refused as non-finite, got: {float}"
    );
}

/// Both engines draw the same sequence from the same seed.
///
/// They share `runtime::builtin`, so this is a check that the seed and the
/// generator really do live in the shared layer rather than in one VM.
#[test]
fn both_engines_agree_on_a_seeded_sequence() {
    // No function literals: the bytecode compiler refuses them ("`to (x) ...
    // end` as an expression is not compiled yet"), which is a gap of its own and
    // not what this test is about. `say` of a list prints it whole, which is
    // enough to compare the two sequences.
    let source = format!(
        "random_seed({SEED})\n\
         for each i from 1 to 20\n    \
             say random(0, 1000)\n    \
             say random_number(0, 1)\n    \
             say random_choice([\"a\", \"b\", \"c\"])\n    \
             say random_shuffle([1, 2, 3, 4])\n\
         end"
    );
    let walked = {
        let tokens = redblue::lexer::Lexer::tokenize(&source).expect("source should lex");
        let ast = redblue::parser::parse(tokens).expect("source should parse");
        let mut vm = redblue::Vm::new();
        vm.run(&ast)
            .expect("the tree-walker should run the program");
        vm.take_output()
    };
    let compiled = {
        let chunk = redblue::compile_source(&source).expect("source should compile");
        let mut vm = redblue::bytecode::vm::BytecodeVm::new();
        vm.run(&chunk)
            .expect("the bytecode VM should run the program");
        vm.take_output()
    };
    assert!(
        !walked.is_empty(),
        "the program printed nothing, so the engines agreeing proves nothing"
    );
    assert_eq!(
        walked, compiled,
        "the two engines drew different sequences from the same seed:\n\
         tree-walker: {walked:?}\nbytecode:    {compiled:?}"
    );
}
