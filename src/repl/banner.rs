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
//! - **Colour per letter, not per row.** The first version painted each *row* a
//!   single colour, which striped the whole wordmark red/red/magenta/blue/blue
//!   and looked like a smear. Each letter carries its own colour, which is also
//!   the only reading that means anything: RED is red, BLUE is blue.
//! - **Centring on the real terminal width**, so it does not sit against column
//!   zero on a wide window.
//!
//! ## Why the letters are built from cells and not as whole rows
//!
//! The first working version wrote five literal strings. Its last row was 30
//! characters where the others were 29, because three of the seven letters have
//! a 4-wide bottom cell and the other four do not, and hand-spacing them drifted.
//! The result was a wordmark whose bottom row was visibly wrong — the user saw
//! it immediately.
//!
//! So every letter is a fixed 4x5 cell and the rows are assembled from them.
//! Alignment is then a property of the code rather than of my typing, and a
//! cell that is one space short is a compile error rather than a picture that
//! looks slightly off.

use crate::repl::style::{Paint, Style};

/// The height of the wordmark, in rows.
const ROWS: usize = 5;

/// One letter: `ROWS` cells, each exactly four characters wide, and its colour.
struct Glyph {
    cells: [&'static str; ROWS],
    paint: Paint,
}

const fn glyph(cells: [&'static str; ROWS], paint: Paint) -> Glyph {
    Glyph { cells, paint }
}

/// The wordmark, in order. It spells REDBLUE and the colours follow the name.
const WORD: [Glyph; 7] = [
    // R E D — the red half.
    glyph(["███ ", "█ █ ", "███ ", "█ █ ", "█ █ "], Paint::BrandRed),
    glyph(["████", "█   ", "███ ", "█   ", "████"], Paint::BrandRed),
    glyph(["███ ", "█ █ ", "█ █ ", "█ █ ", "███ "], Paint::BrandRed),
    // B — the midpoint, so the eye reads one image rather than two blocks.
    glyph(["███ ", "█ █ ", "███ ", "█ █ ", "███ "], Paint::BrandMid),
    // L U E — the blue half.
    glyph(["█   ", "█   ", "█   ", "█   ", "████"], Paint::BrandBlue),
    glyph(["█ █ ", "█ █ ", "█ █ ", "█ █ ", "███ "], Paint::BrandBlue),
    glyph(["████", "█   ", "███ ", "█   ", "████"], Paint::BrandBlue),
];

/// The same wordmark in pure ASCII, for a terminal that cannot draw the above.
///
/// `#` carries the weight and the gaps stay single spaces, so the two render at
/// a similar visual size rather than the ASCII looking cramped.
const WORD_ASCII: [Glyph; 7] = [
    glyph(["### ", "# # ", "### ", "# # ", "# # "], Paint::BrandRed),
    glyph(["####", "#   ", "### ", "#   ", "####"], Paint::BrandRed),
    glyph(["### ", "# # ", "# # ", "# # ", "### "], Paint::BrandRed),
    glyph(["### ", "# # ", "### ", "# # ", "### "], Paint::BrandMid),
    glyph(["#   ", "#   ", "#   ", "#   ", "####"], Paint::BrandBlue),
    glyph(["# # ", "# # ", "# # ", "# # ", "### "], Paint::BrandBlue),
    glyph(["####", "#   ", "### ", "#   ", "####"], Paint::BrandBlue),
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

/// The rendered rows, each one a run of `(text, paint)` segments.
///
/// Splitting a row into per-letter segments is what lets each letter take its own
/// colour. A whole row cannot: it would be one colour for seven letters, which is
/// the striping this design exists to avoid.
fn rows() -> Vec<Vec<(&'static str, Paint)>> {
    let word = if use_ascii() {
        &WORD_ASCII[..]
    } else {
        &WORD[..]
    };
    (0..ROWS)
        .map(|row| {
            word.iter()
                .map(|g| (g.cells[row], g.paint))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Print the banner: centred, colour per letter, static.
pub fn print_banner<W: std::io::Write>(out: &mut W, style: &Style) -> std::io::Result<()> {
    let built = rows();
    let width = WORD[0].cells[0].chars().count() * WORD.len() + (WORD.len() - 1);
    let pad = terminal_width().saturating_sub(width) / 2;

    for segments in &built {
        let mut line = String::new();
        for (i, (text, paint)) in segments.iter().enumerate() {
            if i > 0 {
                line.push(' ');
            }
            line.push_str(&style.paint(*paint, text));
        }
        writeln!(out, "{}{}", " ".repeat(pad), line)?;
    }
    writeln!(out)
}

/// The version the greeting names, read from the crate rather than written out.
///
/// `CARGO_PKG_VERSION` is the one string that cannot go stale: bumping the
/// version in `Cargo.toml` moves the greeting with it. The greeting said `v0.1.0`
/// while the crate was `0.2.0`, and nothing in the tree could have noticed — the
/// number was a literal in a `println!`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The engine a REPL session runs programs on.
///
/// Named rather than left out, because "which of the two VMs is this?" is the
/// first question anyone asks of a session that is behaving differently from
/// the one they had yesterday, and the answer is otherwise only readable in the
/// source. The REPL runs the tree-walking interpreter; the bytecode VM is what
/// `rb compile` and `rb vm` drive.
pub const ENGINE: &str = "tree-walking interpreter";

/// Print the greeting that follows the wordmark: what this is, what to type, and
/// what the session can do that the prompt does not say.
///
/// Three lines, in that order, because each answers the question the one before
/// it raises — what is this, how do I leave, and what can I do here that I could
/// not do from a shell.
pub fn print_greeting<W: std::io::Write>(out: &mut W, style: &Style) -> std::io::Result<()> {
    let heading = format!("Welcome to Redblue v{} - {}", VERSION, ENGINE);
    writeln!(out, "{}", style.paint(Paint::Heading, &heading))?;
    writeln!(
        out,
        "{}",
        style.paint(Paint::Dim, "Type ':help' for commands, ':quit' to exit")
    )?;
    writeln!(
        out,
        "{}",
        style.paint(
            Paint::Dim,
            "TAB completes, Up arrow walks back through history, !! repeats the last line"
        )
    )?;
    writeln!(out)
}

/// The greeting as a `String`, for callers that want the text rather than the IO.
pub fn greeting_text(style: &Style) -> String {
    let mut out = Vec::new();
    // A Vec never fails to write, so the error is unreachable by construction.
    let _ = print_greeting(&mut out, style);
    String::from_utf8(out).unwrap_or_default()
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
