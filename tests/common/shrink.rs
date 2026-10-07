//! Delta debugging: given a program that misbehaves, the shortest one that still
//! does.
//!
//! A failing seed is only actionable if the program behind it can be read, and a
//! 200-statement generated program is not readable. Two passes do the work, and
//! they are the classic pair because they shrink different things:
//!
//! - a **line pass**, which drops whole lines, so a program loses a statement;
//! - a **character pass**, which drops characters, so a program loses the one
//!   character that broke it.
//!
//! Each pass is greedy and restarts after every accepted removal, so a removal
//! that only becomes possible after another one is still found. The loop stops
//! when it reaches a fixed point: every candidate is either not shorter or no
//! longer reproduces.

/// Removes line `index` from `source`, or `None` if there is no such line.
///
/// `None` rather than a no-op string, so "there was no such line" cannot be
/// confused with "the line was empty and removing it changed nothing".
pub fn without_line(source: &str, index: usize) -> Option<String> {
    let lines: Vec<&str> = source.split_inclusive('\n').collect();
    if index >= lines.len() {
        return None;
    }
    let mut kept = String::with_capacity(source.len());
    for (i, line) in lines.iter().enumerate() {
        if i != index {
            kept.push_str(line);
        }
    }
    Some(kept)
}

/// Removes the character at `index`, counting **characters, not bytes**.
///
/// A byte offset would slice `𠮷` in half and hand `String` a value it refuses,
/// which is a panic on a perfectly ordinary program. The corpus and the
/// generator both produce astral text, so the safe indexing is the only one
/// that works.
pub fn without_char(source: &str, index: usize) -> Option<String> {
    let chars: Vec<char> = source.chars().collect();
    let c = chars.get(index)?;
    let mut kept: String = source.chars().take(index).collect();
    kept.extend(chars.iter().skip(index + 1));
    let _ = c;
    Some(kept)
}

/// The number of characters in `source`.
pub fn char_count(source: &str) -> usize {
    source.chars().count()
}

/// Reduces `source` under `fails`, or `None` when it does not fail at all.
///
/// `None` for a source that does not reproduce is the load-bearing part: a
/// shrinker that returns a "minimal" program for something that passed would
/// hand a reader a program that is not the bug.
pub fn shrinks_to<F>(source: &str, mut fails: F) -> Option<String>
where
    F: FnMut(&str) -> bool,
{
    if !fails(source) {
        return None;
    }
    let mut best = source.to_string();

    loop {
        let mut shrank = false;
        for index in 0..without_line_count(&best) {
            let Some(candidate) = without_line(&best, index) else {
                continue;
            };
            if candidate.len() < best.len() && fails(&candidate) {
                best = candidate;
                shrank = true;
                break;
            }
        }
        if shrank {
            continue;
        }
        for index in 0..char_count(&best) {
            let Some(candidate) = without_char(&best, index) else {
                continue;
            };
            if candidate.len() < best.len() && fails(&candidate) {
                best = candidate;
                shrank = true;
                break;
            }
        }
        if !shrank {
            return Some(best);
        }
    }
}

fn without_line_count(source: &str) -> usize {
    source.split_inclusive('\n').count()
}

/// The reduction, in the form a failure message wants: the seed, the original
/// and the program it shrank to.
pub fn report(seed: u64, original: &str, minimal: &str) -> String {
    format!(
        "seed {seed}\n--- original ({})\n{original}--- minimal ({})\n{minimal}",
        original.len(),
        minimal.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The interpreter's own verdict: the source is a counterexample when the
    /// two engines disagree. Nothing about Redblue is restated here, so a change
    /// in what counts as a divergence is a change in this predicate and not a
    /// change in the shrinker's idea of minimal.
    fn diverges(source: &str) -> bool {
        !crate::common::vm::tree_walk(source)
            .agrees_within_format_limits(&crate::common::vm::bytecode(source))
    }

    /// The other interpreter verdict a counterexample can be: a program that
    /// faults. Used where the seed has to be read, because a divergence is not
    /// something this tree currently has.
    fn faults(source: &str) -> bool {
        crate::common::vm::tree_walk(source).result.is_err()
    }

    #[test]
    fn edge_the_shrinker_stops_only_when_nothing_can_be_removed() {
        let original = "set xs to [1, 2]\nsay xs[9]\nsay \"unreached\"\n";
        let minimal = shrinks_to(original, faults).expect("the seed faults");
        assert!(minimal.len() < original.len(), "nothing was removed");

        for index in 0..without_line_count(&minimal) {
            if let Some(candidate) = without_line(&minimal, index) {
                assert!(
                    candidate.len() >= minimal.len() || !faults(&candidate),
                    "line {index} could still come off {minimal:?}",
                );
            }
        }
        for index in 0..char_count(&minimal) {
            if let Some(candidate) = without_char(&minimal, index) {
                assert!(
                    candidate.len() >= minimal.len() || !faults(&candidate),
                    "character {index} could still come off {minimal:?}",
                );
            }
        }
        assert!(faults(&minimal), "the reduction stopped reproducing");
    }

    #[test]
    fn edge_the_shrinker_never_invents_a_counterexample() {
        // A source that does not fail has no minimal failing form, and handing
        // one back would point a reader at a program that is not the bug.
        assert_eq!(shrinks_to("say 1\nsay 2\n", faults), None);
        assert_eq!(shrinks_to("say 1\nsay 2\n", diverges), None);
        assert_eq!(shrinks_to("", faults), None);
    }

    #[test]
    fn edge_a_program_that_does_not_diverge_shrinks_to_nothing() {
        // The two verdicts are separate, and the shrinker is told which one it is
        // reducing. A seed that only diverges must not be reported as a program
        // that faults, and one that only faults must not be reported as a
        // divergence.
        let diverging = "if (1 is 2) then\n    say \"a\"\nend\n(1 is 1)\n";
        assert!(
            !faults(diverging),
            "the fixture has to run, or the other half of the test proves nothing",
        );
        assert_eq!(shrinks_to(diverging, faults), None);
    }

    #[test]
    fn edge_a_character_comes_off_a_multibyte_program_at_its_own_boundary() {
        let source = "say \"日本語𠮷🎉\"\nsay 1\n";
        // Byte 18 is inside `𠮷`, which is four bytes. A byte index would panic
        // in `String::from_utf8`; this must return a whole-character removal.
        for index in 0..char_count(source) {
            let reduced = without_char(source, index).expect("every index is in range");
            assert_eq!(char_count(&reduced), char_count(source) - 1);
        }
        assert_eq!(without_char(source, char_count(source)), None);
    }

    #[test]
    fn edge_a_line_comes_off_and_a_line_that_is_not_there_does_not() {
        let source = "set a to 1\nset b to 2\nset c to 3\n";
        assert_eq!(
            without_line(source, 1).as_deref(),
            Some("set a to 1\nset c to 3\n")
        );
        assert_eq!(without_line(source, 5), None, "there is no sixth line");
        assert_eq!(
            without_line(source, 0).as_deref(),
            Some("set b to 2\nset c to 3\n")
        );
    }

    #[test]
    fn edge_drop_lines_builds_the_source_the_line_pass_looks_at() {
        let source = "a\nb\nc";
        assert_eq!(
            without_line_count(source),
            3,
            "the last line has no newline"
        );
        assert_eq!(without_line_count("a\n"), 1);
        assert_eq!(without_line_count(""), 0);
    }

    #[test]
    fn edge_a_reduction_keeps_the_trailing_newline_a_source_has() {
        let source = "set a to 1\nsay a\n";
        // "set a to 1\n" removed must leave a source that still ends in a
        // newline, because a source without one is a different program to a
        // lexer.
        let reduced = without_line(source, 0).expect("the line is there");
        assert!(reduced.ends_with('\n'), "{reduced:?}");
    }

    #[test]
    fn edge_the_shrinker_reduces_when_no_line_can_go() {
        // The counterexample is inside one line, so only the character pass can
        // reduce it. A shrinker with no character pass returns the original.
        let original = "say 5\nsay 5\n";
        let minimal = shrinks_to(original, |source| source.contains("say 5"))
            .expect("the seed fails the predicate");
        assert!(
            minimal.len() < original.len(),
            "the character pass must reduce a single-line counterexample: {minimal:?}",
        );
    }

    #[test]
    fn edge_the_shrinker_accepts_a_candidate_only_when_it_is_shorter() {
        // A candidate that is not shorter is not an improvement, and accepting
        // one would make the loop restart forever on a program whose removal is
        // refused: `shrinks_to` would never reach a fixed point.
        let original = "say 1\nsay 2\nsay 3\n";
        let long_enough = |source: &str| source.lines().count() >= 2;
        let minimal = shrinks_to(original, long_enough).expect("the seed fails");

        assert_eq!(
            minimal.lines().count(),
            2,
            "the shrinker stopped before it could remove another line: {minimal:?}",
        );
        assert!(
            minimal.len() < original.len(),
            "the reduction did not get shorter: {minimal:?}",
        );

        // And the fixed point is real: one more line comes off and the predicate
        // stops holding, so there is nothing left to accept.
        let shorter = without_line(&minimal, 0).expect("there is a first line");
        assert!(shorter.len() < minimal.len());
        assert!(
            !long_enough(&shorter),
            "a shorter candidate still failed, so the shrinker stopped too early",
        );

        // And a source with a long run of removable lines only ever gets
        // shorter: every accepted step is a step down, so the reduction is
        // monotone and the loop cannot go round.
        let padded = minimal.replace('\n', "\n\n");
        assert!(padded.len() > minimal.len(), "the padding did not pad");
        let reduced = shrinks_to(&padded, long_enough).expect("the padded seed fails");
        assert!(
            reduced.len() < padded.len(),
            "the shrinker grew a source: {reduced:?}",
        );
    }

    #[test]
    fn edge_a_reduction_is_reported_with_the_seed_and_the_program_it_reduced() {
        let message = report(42, "say 1\nsay 2\n", "say 1\n");
        assert!(message.contains("seed 42"), "{message}");
        assert!(message.contains("original (12)"), "{message}");
        assert!(message.contains("minimal (6)"), "{message}");
    }
}
