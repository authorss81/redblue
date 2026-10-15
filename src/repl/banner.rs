//! The banner glyph.
//!
//! **Static on purpose.** An animated banner would redraw over time, and every
//! guarantee the colour tests establish — output byte-identical when colour is
//! off, balanced escapes, nothing written between prompts — becomes untestable
//! the moment frames appear. A wordmark that paints once is testable; one that
//! animates is a source of flaky terminals and corrupted transcripts.
//!
//! Half the mark is red and half is blue, which is the whole name. Each half is
//! painted through [`Style::paint`], so with colour off the glyph is plain text
//! and a session transcript is byte-identical to a pre-colour one.

use crate::repl::style::{Paint, Style};

/// The lines of the banner: each is `(red_part, blue_part)`.
///
/// Splitting it this way — rather than as one pre-coloured string — is what lets
/// the two halves take different colours and still degrade to exact plain text.
const BANNER: [(&str, &str); 3] = [("┌─┐", "┬─┐"), ("└─┐", "├─┤"), ("", "└─┘")];

/// Print the banner glyph, coloured when `style` is enabled.
///
/// Writes through `out` rather than `println!` so a caller can capture it.
pub fn print_banner<W: std::io::Write>(out: &mut W, style: &Style) -> std::io::Result<()> {
    for (red, blue) in BANNER.iter() {
        let mut line = String::new();
        if !red.is_empty() {
            line.push_str(&style.paint(Paint::BrandRed, red));
        }
        line.push_str(&style.paint(Paint::BrandBlue, blue));
        writeln!(out, "{}", line)?;
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
