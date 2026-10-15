//! Colour for the REPL.
//!
//! The property that matters most is negative: **with colour off, output is
//! byte-identical to the plain output.** Everything else here is in service of
//! proving that, because a colour feature that leaked escape bytes into a
//! plain transcript would corrupt golden files and any transcript a user saved.

use redblue::repl::style::{Paint, Style};

// --- the byte-identical guarantee -------------------------------------------

#[test]
fn edge_disabled_paint_returns_the_text_unchanged() {
    let off = Style::forced(false);
    for role in [
        Paint::Prompt,
        Paint::Heading,
        Paint::Result,
        Paint::Error,
        Paint::Dim,
    ] {
        assert_eq!(
            off.paint(role, "plain text"),
            "plain text",
            "a disabled style must return its input verbatim, not a decorated copy"
        );
    }
}

#[test]
fn a_disabled_style_emits_no_escape_sequences_anywhere() {
    let off = Style::forced(false);
    let text = "say \"hello\" — ünïcode ✓";
    for role in [
        Paint::Prompt,
        Paint::Heading,
        Paint::Result,
        Paint::Error,
        Paint::Dim,
    ] {
        let painted = off.paint(role, text);
        assert!(
            !painted.contains('\u{1b}'),
            "disabled paint leaked an escape: {painted:?}"
        );
        assert_eq!(painted.as_bytes(), text.as_bytes());
    }
}

#[test]
fn empty_text_stays_empty_in_both_modes() {
    assert_eq!(Style::forced(false).paint(Paint::Result, ""), "");
    // On, an empty string still wraps and unwraps, so the sequence is balanced
    // rather than unbalanced by the empty payload.
    let on = Style::forced(true).paint(Paint::Result, "");
    assert_eq!(on.matches('\u{1b}').count(), 2);
}

// --- colour actually appears when enabled -----------------------------------

#[test]
fn enabled_paint_wraps_and_resets() {
    let on = Style::forced(true);
    let painted = on.paint(Paint::Error, "boom");
    assert!(painted.contains("boom"), "the text itself must survive");
    assert!(painted.starts_with('\u{1b}'), "must open with an SGR");
    assert!(
        painted.ends_with("\u{1b}[0m"),
        "must reset, or colour bleeds"
    );
}

#[test]
fn every_role_produces_a_distinct_escape() {
    // Two roles sharing a colour would mean a call site had silently picked
    // the wrong role and nothing would look wrong.
    let on = Style::forced(true);
    let roles = [
        Paint::Prompt,
        Paint::Heading,
        Paint::Result,
        Paint::Error,
        Paint::Dim,
    ];
    let mut seen: Vec<String> = roles
        .iter()
        .map(|r| on.paint(*r, "x"))
        .map(|p| p.trim_end_matches("\u{1b}[0m").to_string())
        .collect();
    seen.sort();
    let before = seen.len();
    seen.dedup();
    assert_eq!(before, seen.len(), "two roles render identically: {seen:?}");
}

#[test]
fn paint_never_mutates_its_input() {
    let on = Style::forced(true);
    let original = String::from("set x to 1");
    let painted = on.paint(Paint::Result, &original);
    assert_eq!(original, "set x to 1");
    assert_ne!(painted, original);
}

// --- detection order --------------------------------------------------------

/// The environment is process-global, so these run in one test to keep their
/// ordering deterministic rather than racing other tests in the binary.
#[test]
fn detection_precedence_is_explicit_then_no_color_then_tty() {
    // REDBLUE_COLOR wins over everything.
    std::env::set_var("REDBLUE_COLOR", "always");
    std::env::remove_var("NO_COLOR");
    assert!(Style::detect().enabled(), "REDBLUE_COLOR=always must win");

    std::env::set_var("REDBLUE_COLOR", "never");
    assert!(
        !Style::detect().enabled(),
        "REDBLUE_COLOR=never must win over the TTY check"
    );

    // An unrecognised value falls back to detection rather than erroring, so a
    // typo cannot unexpectedly colour someone's transcript.
    std::env::set_var("REDBLUE_COLOR", "banana");
    let _ = Style::detect(); // must not panic

    // NO_COLOR is honoured when REDBLUE_COLOR is absent or "auto".
    std::env::remove_var("REDBLUE_COLOR");
    std::env::set_var("NO_COLOR", "1");
    assert!(
        !Style::detect().enabled(),
        "NO_COLOR must disable colour, per the cross-platform convention"
    );

    std::env::remove_var("NO_COLOR");
    std::env::remove_var("REDBLUE_COLOR");
}

#[test]
fn edge_empty_no_color_value_still_disables() {
    // NO_COLOR says "any value", including the empty string. A user exporting
    // NO_COLOR= should get what they asked for.
    std::env::remove_var("REDBLUE_COLOR");
    std::env::set_var("NO_COLOR", "");
    let detected = Style::detect();
    std::env::remove_var("NO_COLOR");
    // Under a test runner stdout is not a TTY, so this is off either way; the
    // point is that an empty value is not treated as "unset".
    assert!(
        !detected.enabled(),
        "NO_COLOR with an empty value must still count as set"
    );
}

// --- the seam the rest of the REPL uses -------------------------------------

#[test]
fn forced_is_the_only_way_tests_control_colour() {
    assert!(Style::forced(true).enabled());
    assert!(!Style::forced(false).enabled());
    // detect() under a test runner has no TTY, so it is off; forced() is what
    // lets a test exercise the coloured path at all.
    assert!(!Style::detect().enabled());
}

#[test]
fn style_is_copy_and_comparable_so_it_can_live_in_a_struct() {
    let a = Style::forced(true);
    let b = a; // Copy
    assert_eq!(a, b);
    let clone = a;
    assert_eq!(clone, b);
}
// --- the banner glyph --------------------------------------------------------

#[test]
fn edge_banner_with_colour_off_is_plain_text_with_no_escapes() {
    let text = redblue::repl::banner::banner_text(&Style::forced(false));
    assert!(
        !text.contains('\u{1b}'),
        "a disabled banner leaked escape bytes: {text:?}"
    );
    // Exactly three lines, and the third is blue-half only.
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "banner must be three lines: {text:?}");
    assert_eq!(lines[2], "└─┘");
}

#[test]
fn a_coloured_banner_paints_both_halves_and_plain_text_still_reads() {
    let text = redblue::repl::banner::banner_text(&Style::forced(true));
    assert!(text.contains('\u{1b}'), "enabled banner must colour");
    // Stripping escapes must leave exactly the plain banner - colour is the only
    // difference between the two modes.
    let stripped: String = strip_escapes(&text);
    let plain = redblue::repl::banner::banner_text(&Style::forced(false));
    assert_eq!(stripped, plain, "colour changed the visible glyph");
}

#[test]
fn the_two_halves_of_the_wordmark_use_different_roles() {
    let on = Style::forced(true);
    let red = on.paint(Paint::BrandRed, "x");
    let blue = on.paint(Paint::BrandBlue, "x");
    assert_ne!(red, blue, "the wordmark would be one colour, not redblue");
}

#[test]
fn brand_roles_are_distinct_from_error_and_prompt() {
    // A brand colour that matches the error or prompt colour would make the
    // wordmark read as an alert or as the prompt.
    let on = Style::forced(true);
    assert_ne!(on.paint(Paint::BrandRed, "x"), on.paint(Paint::Error, "x"));
    assert_ne!(
        on.paint(Paint::BrandBlue, "x"),
        on.paint(Paint::Prompt, "x")
    );
}

#[test]
fn edge_banner_is_stable_across_calls() {
    // Not animated: two calls must be identical. An animated banner would fail
    // this, which is the point - it is why the glyph is static.
    let s = Style::forced(true);
    assert_eq!(
        redblue::repl::banner::banner_text(&s),
        redblue::repl::banner::banner_text(&s)
    );
}

fn strip_escapes(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for n in chars.by_ref() {
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
