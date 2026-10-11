//! Naming the token a mistyped one was probably meant to be.
//!
//! A failure that reports only a position sends the reader to a place in the
//! file and stops there. `say coun` is told that `coun` is unknown, which is
//! true and is the whole of the help: the name the writer meant is in scope,
//! one character away, and this is what finds it.
//!
//! Two rules keep a suggestion worth reading:
//!
//! - **A transposition is one mistake, not two.** `sya` for `say` is the most
//!   common way to mistype a keyword, and a distance that charges a substitution
//!   and an insertion for it puts the pair out of reach of the budget below.
//!   The distance here is optimal string alignment: insertion, deletion,
//!   substitution, or swapping two adjacent characters.
//! - **A suggestion must be shorter than the name it is about.** A distance
//!   budget alone admits a one-character name, where every other name is one
//!   edit away and the "nearest" of them is an accident of the alphabet rather
//!   than a guess. `z` is never near anything.

use std::cmp::min;

/// The distance between `left` and `right`: the fewest insertions, deletions,
/// substitutions and adjacent transpositions that turn one into the other.
///
/// Characters, not bytes, so a name written in another script is compared by
/// what a reader would call its letters.
pub fn distance(left: &str, right: &str) -> usize {
    let a: Vec<char> = left.chars().collect();
    let b: Vec<char> = right.chars().collect();

    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }

    // Three rows of the table: the one being filled, the one above it, and the
    // one above that — a transposition reads from row `i-2`. All three are the
    // same length and are rotated rather than reallocated, because each row is
    // the buffer the next one is filled into.
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut before_previous: Vec<usize> = vec![0; b.len() + 1];
    let mut current: Vec<usize> = vec![0; b.len() + 1];

    for i in 1..=a.len() {
        current[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = min(
                previous[j] + 1,
                min(current[j - 1] + 1, previous[j - 1] + cost),
            );

            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = min(best, before_previous[j - 2] + 1);
            }

            current[j] = best;
        }

        std::mem::swap(&mut before_previous, &mut previous);
        std::mem::swap(&mut previous, &mut current);
    }

    previous[b.len()]
}

/// The candidate nearest to `target`, or `None` when nothing is close enough to
/// be worth offering.
///
/// `None` is the common answer and it is the important one: a suggestion is
/// advice, and advice that names something the writer never meant costs the
/// reader more than no advice at all.
///
/// The winner is the candidate with the smallest distance, and among equals the
/// one that sorts first — so two runs over the same candidates in the same or a
/// different order name the same thing.
pub fn closest<'a>(target: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let target_length = target.chars().count();
    if target_length == 0 {
        return None;
    }

    let mut best: Option<(&str, usize)> = None;

    for candidate in candidates {
        if candidate == target {
            continue;
        }

        let reach = target_length.max(candidate.chars().count());
        let budget = (reach / 3).max(1);
        let found = distance(target, candidate);
        if found > budget || found >= target_length {
            continue;
        }

        let better = match best {
            Some((name, best_found)) => {
                found < best_found || (found == best_found && candidate < name)
            }
            None => true,
        };
        if better {
            best = Some((candidate, found));
        }
    }

    best.map(|(name, _)| name)
}

/// The `; did you mean 'x'?` half of a message — empty when nothing is close
/// enough to suggest.
pub fn did_you_mean(target: &str, candidates: impl IntoIterator<Item = String>) -> String {
    let owned: Vec<String> = candidates.into_iter().collect();

    match closest(target, owned.iter().map(String::as_str)) {
        Some(name) => format!("; did you mean '{}'?", name),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: &[&str] = &["say", "set", "count", "total", "end", "and", "true"];

    #[test]
    fn test_a_transposition_costs_one() {
        assert_eq!(distance("sya", "say"), 1, "two letters swapped");
        assert_eq!(distance("coun", "count"), 1, "one letter missing");
        assert_eq!(distance("", "abc"), 3, "an empty name is three insertions");
        assert_eq!(distance("abc", "abc"), 0, "a name equals itself");
    }

    #[test]
    fn edge_the_nearest_of_several_is_chosen() {
        assert_eq!(closest("coun", NAMES.iter().copied()), Some("count"));
        assert_eq!(closest("sya", NAMES.iter().copied()), Some("say"));
    }

    #[test]
    fn edge_a_tie_is_broken_the_same_way_every_time() {
        // `abd` is one substitution from both `add` and `and`. Whichever is
        // named, it must be the same one however the candidates are ordered — a
        // tie broken by iteration order would render a different diagnostic run
        // to run.
        let forward = closest("abd", ["add", "and"]);
        let backward = closest("abd", ["and", "add"]);

        assert_eq!(forward, Some("add"), "the first by sort order wins");
        assert_eq!(
            forward, backward,
            "the order candidates arrive in must not change the answer"
        );
    }

    #[test]
    fn edge_no_candidate_is_too_close() {
        assert_eq!(
            closest("zzzzzzzzz", NAMES.iter().copied()),
            None,
            "nothing is near"
        );
        assert_eq!(closest("x", [] as [&str; 0]), None, "no candidates at all");
        assert_eq!(
            closest("x", NAMES.iter().copied()),
            None,
            "a name cannot be guessed at"
        );
        assert_eq!(
            closest("", NAMES.iter().copied()),
            None,
            "an empty name has no spelling"
        );
        assert_eq!(
            closest("say", ["say"]),
            None,
            "a name is never suggested back to the reader"
        );
    }

    #[test]
    fn edge_a_single_candidate_is_accepted_or_rejected_on_its_merits() {
        assert_eq!(closest("adn", ["and"]), Some("and"), "one edit away");
        assert_eq!(closest("elephant", ["and"]), None, "a long way away");
    }

    #[test]
    fn test_the_message_half_reads_as_a_sentence() {
        let candidates: Vec<String> = NAMES.iter().map(|n| (*n).to_string()).collect();

        assert_eq!(
            did_you_mean("sya", candidates.clone()),
            "; did you mean 'say'?"
        );
        assert_eq!(did_you_mean("qqqqqqq", candidates), "");
    }
}
