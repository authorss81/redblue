//! The banner glyph.
//!
//! **Static on purpose.** An animated banner would redraw over time, and every
//! guarantee the colour tests establish — output byte-identical when colour is
//! off, balanced escapes, nothing written between prompts — becomes untestable
//! the moment frames appear. A wordmark that paints once is testable; one that
//! animates is a source of flaky terminals and corrupted transcripts.
//!
//! Three things it borrows from Ipp's banner, because each one is a fix for a
//! way a banner actually fails:
//!
//! - **An ASCII fallback.** Box-drawing glyphs render as blanks or `?` on a
//!   terminal whose font lacks them, which makes the mark *invisible* rather
//!   than ugly. A mark you cannot see is worse than no mark.
//! - **A gradient across the lines**, not one flat colour, so the eye reads it
//!   as one image rather than a stack of coloured rows.
//! - **Centring on the real terminal width**, so it does not sit hard against
//!   column zero on a wide window.
//!
//! It spells REDBLUE, and the gradient follows the name: **RED** is painted red,
//! **BLUE** is painted blue, and B sits on the midpoint between them.
//!
//! The first version of this file drew the letter R three times and called it a
//! wordmark. It looked like "RRR". A glyph that is not the thing it claims to
//! be is not a cosmetic defect — it is a lie in the place the user looks first.

use crate::repl::style::{Paint, Style};

/// One line of the banner: the glyph, and which step of the gradient it takes.
struct Line {
    glyph: &'static str,
    paint: Paint,
}

/// The Unicode wordmark. Five rows, seven letters, `redblue`.
const LOGO_UNICODE: [Line; 6] = [
    Line {
        glyph: "███ ████ ███ ███ █   █ █ ████",
        paint: Paint::BrandRed,
    },
    Line {
        glyph: "█ █ █    █ █ █ █ █   █ █ █   ",
        paint: Paint::BrandRed,
    },
    Line {
        glyph: "███ ███  █ █ ███ █   █ █ ███ ",
        paint: Paint::BrandMid,
    },
    Line {
        glyph: "█ █ █    █ █ █ █ █   █ █ █   ",
        paint: Paint::BrandBlue,
    },
    Line {
        glyph: "█ █ ████ ███ ████ ███ ███ ████",
        paint: Paint::BrandBlue,
    },
    Line {
        glyph: "",
        paint: Paint::BrandBlue,
    },
];

/// The same wordmark in pure ASCII, for a terminal that cannot draw the above.
///
/// Proportioned to match: `#` carries the weight and the gaps are single spaces,
/// so the two render at a similar visual size rather than the ASCII looking
/// cramped.
const LOGO_ASCII: [Line; 6] = [
    Line {
        glyph: "### #### ### ### #   # # ####",
        paint: Paint::BrandRed,
    },
    Line {
        glyph: "# # #    # # # # #   # # #   ",
        paint: Paint::BrandRed,
    },
    Line {
        glyph: "### ###  # # ### #   # # ### ",
        paint: Paint::BrandMid,
    },
    Line {
        glyph: "# # #    # # # # #   # # #   ",
        paint: Paint::BrandBlue,
    },
    Line {
        glyph: "# # #### ### #### ### ### ####",
        paint: Paint::BrandBlue,
    },
    Line {
        glyph: "",
        paint: Paint::BrandBlue,
    },
];

/// Whether box-drawing glyphs can be trusted.
///
/// `REDBLUE_ASCII=1` is the escape hatch, because font coverage cannot be
/// detected reliably from inside the process — the process knows the encoding
/// (always UTF-8 in Rust) but not what the font can draw. `TERM=dumb` is the one
/// case that is detectable and worth honouring unasked.
fn use_ascii() -> bool {
    if let Ok(v) = std::env::var("REDBLUE_ASCII") {
        if matches!(v.trim(), "1" | "true" | "yes") {
            return true;
        }
    }
    matches!(std::env::var("TERM").as_deref(), Ok("dumb"))
}

/// Terminal width, defaulting to 80 when it cannot be read.
fn terminal_width() -> usize {
    // COLUMNS is what the shell exports and is cheap; a failure here must never
    // stop the REPL starting, hence the default rather than an error.
    if let Ok(c) = std::env::var("COLUMNS") {
        if let Ok(n) = c.trim().parse::<usize>() {
            if n > 0 {
                return n;
            }
        }
    }
    80
}

/// Print the banner: centred, gradient-coloured, static.
pub fn print_banner<W: std::io::Write>(out: &mut W, style: &Style) -> std::io::Result<()> {
    let ascii = use_ascii();
    let logo = if ascii {
        &LOGO_ASCII[..]
    } else {
        &LOGO_UNICODE[..]
    };

    // Measure the widest glyph so centring survives colour being off — an
    // escape-wrapped string is longer than the text it wraps.
    let width = logo
        .iter()
        .map(|l| l.glyph.chars().count())
        .max()
        .unwrap_or(0);
    let pad = terminal_width().saturating_sub(width) / 2;

    for line in logo {
        if line.glyph.is_empty() {
            writeln!(out)?;
        } else {
            writeln!(
                out,
                "{}{}",
                " ".repeat(pad),
                style.paint(line.paint, line.glyph)
            )?;
        }
    }
    Ok(())
}

/// The banner as a `String`, for callers that want the text rather than the IO.
pub fn banner_text(style: &Style) -> String {
    let mut out = Vec::new();
    // A Vec never fails to write, so the error is unreachable by construction.
    let _ = print_banner(&mut out, style);
    String::from_utf8(out).unwrap_or_default()
}

/// Whether the ASCII form is in use. Exposed so a test can pin both renderings
/// rather than only whichever the test host happens to pick.
pub fn ascii_in_use() -> bool {
    use_ascii()
}
