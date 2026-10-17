//! Choosing the REPL's colours.
//!
//! The palette used to be a `match` inside `Paint::sgr`, so changing a colour
//! meant editing the implementation and there was no way to choose one at all.
//! What replaced it is data: a palette per theme, `:themes` to list them and
//! `:theme <name>` to pick one mid-session.
//!
//! Two guarantees are pinned harder than the rest, because breaking either is
//! worse than not having the feature at all:
//!
//! 1. **The no-colour theme is byte-identical to colour off.** Compared as
//!    bytes, never by eye — a theme that emitted `\u{1b}[0m` around plain text
//!    would corrupt every golden file and every saved transcript.
//! 2. **The dark theme is exactly what shipped before themes existed.** Every
//!    role's SGR is written out here, so the colours a user has been reading do
//!    not move because a new theme was added beside them.

use std::path::Path;

use redblue::repl::commands::ReplCommand;
use redblue::repl::style::{Paint, Palette, Style};

/// Every role, so no test has to remember the list and no role can be added
/// without these tests noticing.
const ROLES: &[Paint] = Paint::ALL;

/// The SGR the shipped dark theme gives each role: the values that were
/// hardcoded in `Paint::sgr` before themes existed.
///
/// Written out in full rather than read back from the palette, because a test
/// that compares a palette with itself cannot fail.
const SHIPPED_DARK: &[(Paint, &str)] = &[
    (Paint::Prompt, "1;34"),
    (Paint::Heading, "1;36"),
    (Paint::Result, "0;32"),
    (Paint::Error, "0;31"),
    (Paint::Dim, "2;37"),
    (Paint::BrandRed, "1;31"),
    (Paint::BrandBlue, "1;36"),
    (Paint::BrandMid, "1;35"),
    (Paint::Keyword, "0;35"),
    (Paint::Str, "0;32"),
    (Paint::Number, "0;36"),
    (Paint::Comment, "0;90"),
    (Paint::Builtin, "0;33"),
    (Paint::Operator, "0;37"),
];

/// The text every byte comparison below is made on: empty, ASCII, and a line
/// with multibyte characters, emoji and a combining mark.
fn sample_texts() -> Vec<&'static str> {
    vec![
        "",
        "x",
        "say \"hello\" — ünïcode ✓ 🎉",
        "e\u{301} combining",
        "42",
    ]
}

// --- the palette is one value per role, not a match arm ----------------------

#[test]
fn the_dark_theme_is_exactly_the_palette_this_build_shipped_before_themes() {
    let dark = Palette::dark();
    assert_eq!(
        SHIPPED_DARK.len(),
        ROLES.len(),
        "the pinned list and Paint::ALL disagree: a role was added and this \
         table was not"
    );
    for (role, sgr) in SHIPPED_DARK {
        assert_eq!(dark.sgr(*role), Some(*sgr), "{:?} changed colour", role);
    }
    // And through the real Style, so the assertion is about the bytes a user
    // sees rather than about a table that only the palette reads.
    let style = Style::with_theme(true, dark);
    assert_eq!(
        style.paint(Paint::Prompt, ">>> "),
        "\u{1b}[1;34m>>> \u{1b}[0m"
    );
    assert_eq!(style.paint(Paint::Result, "ok"), "\u{1b}[0;32mok\u{1b}[0m");
}

/// Every theme that paints anything must say what every role wears. A hole in
/// a palette would leave a role silently plain, which looks like a bug in
/// whatever printed it.
#[test]
fn every_theme_paints_every_role() {
    for palette in Palette::ALL {
        if palette.is_plain() {
            continue;
        }
        for role in ROLES {
            let sgr = palette.sgr(*role);
            assert!(
                sgr.is_some() && !sgr.unwrap().is_empty(),
                "the {} theme says nothing about {:?}",
                palette.name(),
                role
            );
            let painted = Style::with_theme(true, *palette).paint(*role, "x");
            assert_eq!(
                painted,
                format!("\u{1b}[{}mx\u{1b}[0m", sgr.unwrap()),
                "the {} theme paints {:?} differently from the SGR it declares",
                palette.name(),
                role
            );
        }
    }
}

/// The guarantee `repl_color_test` makes about the one theme it knew about,
/// held of every theme now: brand colours must not look like the prompt or
/// like an alert, and a result must not look like a brand step.
#[test]
fn edge_brand_roles_stay_distinct_in_every_theme() {
    for palette in Palette::ALL {
        // `none` paints nothing at all, so every role is the same plain text —
        // which is its guarantee, asserted byte for byte in
        // `the_no_colour_theme_emits_exactly_the_bytes_colour_off_does` and not
        // again here.
        if palette.is_plain() {
            continue;
        }
        let style = Style::with_theme(true, *palette);
        assert_ne!(
            style.paint(Paint::BrandRed, "x"),
            style.paint(Paint::Error, "x"),
            "{}: the wordmark's red must not read as an error",
            palette.name()
        );
        assert_ne!(
            style.paint(Paint::BrandBlue, "x"),
            style.paint(Paint::Prompt, "x"),
            "{}: the wordmark's blue must not read as something to type",
            palette.name()
        );
        assert_ne!(
            style.paint(Paint::BrandMid, "x"),
            style.paint(Paint::Result, "x"),
            "{}: the gradient's middle must not read as a value",
            palette.name()
        );
    }
}

// --- names are resolved the way a human types them ---------------------------

/// `:theme` is typed by hand, so the name it carries is trimmed and matched
/// without regard to case — and a name that is whitespace, or empty, or nothing
/// at all is no theme rather than the first one in the table.
#[test]
fn edge_theme_names_resolve_case_insensitively_and_ignore_surrounding_whitespace() {
    for palette in Palette::ALL {
        let name: &str = palette.name();
        for typed in [
            name.to_string(),
            name.to_ascii_uppercase(),
            name.to_ascii_lowercase(),
            format!("  {}  ", name),
            format!("\t{}\t", name),
        ] {
            assert_eq!(
                Palette::by_name(&typed).map(|palette| palette.name()),
                Some(name),
                "{:?} should resolve to the {} theme",
                typed,
                name
            );
        }
    }

    for empty in ["", "   ", "\t", "dark dark", "darks"] {
        assert!(
            Palette::by_name(empty).is_none(),
            "{:?} is not a theme name and must resolve to none",
            empty
        );
    }
    // Non-ASCII is not a case-fold away from a shipped name: the match is ASCII
    // lowering, so a name with an accent in it is simply not a theme rather than
    // a near miss quietly resolving to one.
    for accented in ["därk", "ligℏt", "ｎｏｎｅ"] {
        assert!(
            Palette::by_name(accented).is_none(),
            "{:?} must not resolve to a theme",
            accented
        );
    }
}

/// The command carries the name as typed and resolution happens where the
/// session can fail loudly: an unknown theme names what there is, and the style
/// in hand is left alone rather than reset to the first entry.
#[test]
fn an_unknown_theme_name_is_refused_and_names_the_themes_that_exist() {
    let mut style = Style::forced(true);
    let before = style.paint(Paint::Prompt, ">>> ");
    let error = style
        .set_theme("neon")
        .expect_err("no theme is called neon");
    for palette in Palette::ALL {
        assert!(
            error.contains(palette.name()),
            "the error should offer {}: {}",
            palette.name(),
            error
        );
    }
    assert!(
        error.contains("neon"),
        "and it should name what was asked for: {}",
        error
    );
    assert_eq!(
        style.paint(Paint::Prompt, ">>> "),
        before,
        "a refused theme must leave the session on the one it had"
    );
}

#[test]
fn the_theme_command_carries_the_name_it_was_given() {
    assert_eq!(
        ReplCommand::parse(":theme light"),
        ReplCommand::Theme("light".to_string())
    );
    assert_eq!(
        ReplCommand::parse(".theme Light "),
        ReplCommand::Theme("Light".to_string()),
        "the other sigil reaches it, and the name arrives as typed"
    );
    assert_eq!(
        ReplCommand::argument_text(&ReplCommand::Theme("dark".to_string())),
        Some("dark")
    );
    let bare = ReplCommand::parse(":theme");
    assert!(
        matches!(
            bare,
            ReplCommand::MissingArgument {
                command: "theme",
                ..
            }
        ),
        "a theme name is not optional: no argument is a command typed halfway, \
         not a request for the default theme. Got {bare:?}"
    );
    assert_eq!(
        ReplCommand::parse(":themes"),
        ReplCommand::Themes,
        ":themes lists what there is and takes no name"
    );
    assert_eq!(
        ReplCommand::argument_text(&ReplCommand::Themes),
        None,
        "and carries no argument"
    );
}

// --- switching, and what it does to the bytes -------------------------------

/// Switching is immediate: the next line painted is painted in the new theme,
/// with no restart and no re-detection of the environment.
#[test]
fn switching_a_theme_takes_effect_on_the_next_line() {
    let mut style = Style::forced(true);
    assert_eq!(
        style.paint(Paint::Prompt, ">>> "),
        "\u{1b}[1;34m>>> \u{1b}[0m"
    );

    style.set_theme("light").expect("light should be a theme");
    assert_eq!(style.theme_name(), "light");
    let prompt = style
        .sgr(Paint::Prompt)
        .expect("the light theme paints the prompt");
    assert_eq!(
        style.paint(Paint::Prompt, ">>> "),
        format!("\u{1b}[{prompt}m>>> \u{1b}[0m"),
        "the very next line must be painted in the theme just chosen"
    );
    assert_eq!(
        prompt,
        Palette::light().sgr(Paint::Prompt).unwrap(),
        "and in that theme's colour, not the default's"
    );

    style.set_theme("dark").expect("dark should be a theme");
    assert_eq!(
        style.paint(Paint::Prompt, ">>> "),
        "\u{1b}[1;34m>>> \u{1b}[0m"
    );

    // A palette says which colours, not whether there are any: `NO_COLOR` and
    // `REDBLUE_COLOR=never` outrank the theme in hand exactly as they outrank
    // the default one.
    assert_eq!(
        Style::with_theme(false, Palette::dark()).paint(Paint::Prompt, ">>> "),
        ">>> ",
        "a palette must not be able to turn colour on"
    );
}

/// The no-colour theme is colour off, exactly. Compared byte for byte, on every
/// role and on empty, ASCII and multibyte text, because "looks the same" is not
/// the property that matters — the bytes are.
#[test]
fn the_no_colour_theme_emits_exactly_the_bytes_colour_off_does() {
    let plain = Style::with_theme(true, Palette::plain());
    assert!(plain.enabled(), "the theme is not what turned colour off");
    assert_eq!(plain.theme_name(), "none");

    for text in sample_texts() {
        let off = Style::forced(false);
        for role in ROLES {
            let themed = plain.paint(*role, text);
            let uncoloured = off.paint(*role, text);
            assert_eq!(
                themed.as_bytes(),
                uncoloured.as_bytes(),
                "the {:?} role differs between the plain theme and colour off",
                role
            );
            assert_eq!(
                themed.as_bytes(),
                text.as_bytes(),
                "the {:?} role in the plain theme must be the text, byte for byte",
                role
            );
        }
    }

    // Empty text is its own case: there is nothing to keep, and a theme that
    // still wrapped it would emit a balanced but pointless escape pair.
    assert_eq!(plain.paint(Paint::Result, ""), "");
    assert_eq!(
        plain.paint(Paint::Result, "").matches('\u{1b}').count(),
        0,
        "the plain theme must emit no escape at all"
    );
}

/// A palette with nothing in it is the plain theme, and the way to say "no
/// colour" in the theme table rather than with a flag on the style.
#[test]
fn edge_the_plain_theme_is_the_one_that_paints_nothing() {
    let plain = Palette::plain();
    assert!(plain.is_plain());
    assert_eq!(plain.name(), "none");
    for role in ROLES {
        assert_eq!(
            plain.sgr(*role),
            None,
            "{:?} must have no colour in the theme that has no colour",
            role
        );
    }
    for palette in Palette::ALL {
        assert_eq!(
            palette.is_plain(),
            palette.sgr(Paint::Result).is_none(),
            "{}: a theme is plain exactly when it colours nothing",
            palette.name()
        );
    }
}

// --- :themes ----------------------------------------------------------------

/// `:themes` names every theme and shows what it looks like — and what it shows
/// is painted by a real [`Style`] wearing *that* theme, not printed literally.
#[test]
fn the_theme_listing_renders_each_sample_through_the_real_style() {
    let current = Style::with_theme(true, Palette::light());
    let listing = Palette::listing(&current);

    assert_eq!(
        listing.len(),
        Palette::ALL.len(),
        "one line per theme: {:?}",
        listing
    );

    for palette in Palette::ALL {
        let line = listing
            .iter()
            .find(|line| line.contains(palette.name()))
            .unwrap_or_else(|| panic!("{} is not listed: {listing:?}", palette.name()));
        assert!(
            line.contains(palette.description()),
            "the {} line should say what it is for: {line}",
            palette.name()
        );
        if palette.is_plain() {
            assert!(
                !line.contains('\u{1b}'),
                "the plain theme's sample must be plain text: {line:?}"
            );
            continue;
        }
        // The sample carries that theme's own escapes, so it cannot be a string
        // constant in the listing: the codes are read out of the palette.
        for role in [Paint::Prompt, Paint::Result, Paint::Error, Paint::Dim] {
            let sgr = palette.sgr(role).expect("sampled role has a colour");
            assert!(
                line.contains(&format!("\u{1b}[{sgr}m")),
                "the {} sample never used its own {:?} colour {sgr}: {line:?}",
                palette.name(),
                role
            );
        }
    }

    // The theme in hand is marked, and marking it does not paint it: the marker
    // is plain so it survives the plain theme too.
    let marked: Vec<&String> = listing
        .iter()
        .filter(|line| line.trim_start().starts_with('*'))
        .collect();
    assert_eq!(
        marked.len(),
        1,
        "exactly one theme is the one in hand: {listing:?}"
    );
    assert!(
        marked[0].contains("light"),
        "and it is the one the style was built with: {}",
        marked[0]
    );
}

// --- the documentation ------------------------------------------------------

/// Every theme named in the documentation is a theme that resolves. A name in
/// `README.md` or `SPEC.md` that `:theme` refuses is the defect this feature
/// would ship with if the docs were written by hand and never checked.
#[test]
fn every_documented_theme_name_actually_resolves() {
    for (file, heading) in [("README.md", "REPL themes"), ("SPEC.md", "REPL themes")] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(file);
        let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "{} should be readable for the docs test: {error}",
                path.display()
            )
        });
        // The section under the first heading named `heading`, whatever level it
        // is: found by the text rather than by a heading string, so a document
        // that promotes or demotes the section does not fail here.
        let start = text
            .lines()
            .position(|line| {
                line.starts_with('#') && line.trim_start_matches('#').trim() == heading
            })
            .unwrap_or_else(|| panic!("{} should document {}", file, heading));
        let section: String = text
            .lines()
            .skip(start + 1)
            .take_while(|line| !line.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");

        let mut checked = 0;
        for line in section.lines() {
            let Some(name) = line
                .strip_prefix("- `")
                .and_then(|rest| rest.split('`').next())
            else {
                continue;
            };
            checked += 1;
            assert!(
                Palette::by_name(name).is_some(),
                "{} documents the theme {:?}, which :theme does not resolve",
                file,
                name
            );
        }
        assert!(
            checked >= Palette::ALL.len(),
            "{} names {} themes and there are {} to name",
            file,
            checked,
            Palette::ALL.len()
        );
        assert!(
            section.contains(":theme"),
            "{} should document the command that switches",
            file
        );
        assert!(
            section.contains(":themes"),
            "{} should document the command that lists them",
            file
        );
    }
}
