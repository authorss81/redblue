//! What a session ran and what it said, in a form that leaves the terminal.
//!
//! A REPL session used to leave the machine only as scrollback: prompts, colour
//! escapes, the banner, whatever the terminal happened to be doing. Sharing one
//! meant copy-pasting that, and every reader of the result got the escapes too.
//! This is the other half — the session as a document, Markdown with the input
//! of each line fenced as `redblue` and what it said written after it.
//!
//! Two properties make it a document rather than a dump, and both are asserted:
//!
//! - **No escape sequence can reach it.** What is recorded is plain text, and
//!   [`Transcript::markdown`] strips escapes from it again on the way out, so a line the
//!   session printed in colour and a line it printed plain export identically.
//! - **It reads back.** [`Transcript::read_markdown`] is the other half of
//!   [`Transcript::markdown`], and
//!   an exported session that cannot be read back is a picture of one.

/// The language the input of each exchange is fenced as.
///
/// Not decoration: a reader — or [`Transcript::read_markdown`] — tells an input
/// block from
/// anything else in the document by this word.
pub const FENCE_LANGUAGE: &str = "redblue";

/// The heading every exported document opens with.
pub const HEADER: &str = "# Redblue session";

/// What a document with nothing in it says instead of holding a fence.
pub const EMPTY: &str = "_Nothing has been run in this session yet._";

/// How many exchanges a session keeps.
///
/// A transcript is a record of a session, not a log to keep forever, and one that
/// grew without limit would be the one thing in the REPL with no bound: the
/// history already refuses to grow past the session's `HISTORY_LIMIT`, and this keeps
/// the same number of exchanges. The oldest are dropped, because the newest are
/// the ones a reader of the export came for.
pub const LIMIT: usize = 500;

/// One line the session ran, and everything it said while running it.
///
/// The input is the block's whole source, not the line it was typed on: a block
/// is what the session ran, and splitting it would export something the session
/// never ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exchange {
    /// What the user typed, verbatim.
    pub input: String,
    /// What the line said, in order: `say` output, the `= value` of an expression
    /// that produced one, and the rendered failure of one that did not.
    pub output: Vec<String>,
}

/// The exchanges of one session, in the order they were run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Transcript {
    exchanges: Vec<Exchange>,
}

impl Transcript {
    /// A session that has run nothing.
    pub fn new() -> Self {
        Self {
            exchanges: Vec::new(),
        }
    }

    /// Records one line the session ran and what it said.
    ///
    /// Drops the oldest exchange past [`LIMIT`] rather than growing, so a long
    /// session cannot turn an export into a memory problem.
    pub fn record(&mut self, input: String, output: Vec<String>) {
        if self.exchanges.len() >= LIMIT {
            self.exchanges.remove(0);
        }
        self.exchanges.push(Exchange { input, output });
    }

    /// Forgets everything, which is what a session reset by `:reset` means by it.
    ///
    /// `:reset` throws away the bindings and the last result; the lines that made
    /// them are in the history and stay there. An export that kept them would
    /// claim a session ran code that `:reset` said it had forgotten.
    pub fn clear(&mut self) {
        self.exchanges.clear();
    }

    /// Every exchange, in the order it was run.
    pub fn exchanges(&self) -> &[Exchange] {
        &self.exchanges
    }

    /// How many exchanges are recorded.
    pub fn len(&self) -> usize {
        self.exchanges.len()
    }

    /// Whether the session has run nothing.
    pub fn is_empty(&self) -> bool {
        self.exchanges.is_empty()
    }

    /// The session as a Markdown document.
    ///
    /// Every exchange is a `redblue` fence holding what was typed and a text
    /// block holding what it said — one block per line, so a reader can tell
    /// which output belongs to which input, and so that reading the fences back
    /// gives the session rather than a document.
    ///
    /// The fence is *widened* for an input that contains one. A line of Redblue
    /// that prints three backticks would otherwise close the block it is written
    /// inside and export the rest of the session as prose, so the fence is as
    /// long as the longest run of backticks in the input plus one.
    pub fn markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(HEADER);
        out.push_str("\n\n");
        if self.exchanges.is_empty() {
            out.push_str(EMPTY);
            out.push('\n');
            return out;
        }

        for exchange in &self.exchanges {
            let fence = "`".repeat(fence_width(&exchange.input));
            out.push_str(&fence);
            out.push_str(FENCE_LANGUAGE);
            out.push('\n');
            for line in crate::repl::style::strip_escapes(&exchange.input).lines() {
                out.push_str(line);
                out.push('\n');
            }
            out.push_str(&fence);
            out.push('\n');
            for line in &exchange.output {
                for rendered in crate::repl::style::strip_escapes(line).lines() {
                    out.push_str(rendered);
                    out.push('\n');
                }
            }
            out.push('\n');
        }
        out
    }

    /// The inputs of every `redblue` block in an exported document.
    ///
    /// The other half of [`Transcript::markdown`]: what a fresh session is run to
    /// reproduce a session, and what a test pins a round trip on. A document that
    /// is not one this wrote is read anyway — a fence of any width, a language
    /// that is not `redblue` is not a block of input — because a reader that
    /// refuses a document it did not write is a reader that cannot read a
    /// hand-edited one.
    pub fn read_markdown(document: &str) -> Vec<String> {
        let mut blocks: Vec<String> = Vec::new();
        let mut inside: Option<usize> = None;
        let mut fence = String::new();
        for line in document.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("```") {
                if let Some(index) = inside {
                    if !blocks[index].is_empty() {
                        blocks[index].push('\n');
                    }
                    blocks[index].push_str(line);
                }
                continue;
            }
            match inside {
                // Only the fence this block was opened with closes it. A shorter
                // run inside a widened block is content — which is why the writer
                // widens, and why reading it back needs no separate rule.
                Some(_) if trimmed == fence => inside = None,
                Some(index) => {
                    if !blocks[index].is_empty() {
                        blocks[index].push('\n');
                    }
                    blocks[index].push_str(line);
                }
                None => {
                    if fence_language(trimmed) == Some(FENCE_LANGUAGE) {
                        fence = trimmed.chars().take_while(|c| *c == '`').collect();
                        inside = Some(blocks.len());
                        blocks.push(String::new());
                    }
                }
            }
        }
        // A block the document never closed is a truncated export, not a line of
        // the session: reading half a block back and running it would be worse
        // than reading nothing.
        if inside.is_some() {
            blocks.pop();
        }
        blocks
    }
}

/// The language of the fence `line` opens, or `None` for a line that is not one.
///
/// At least three backticks, as Markdown requires, and whatever follows them on
/// the same line is the language — so a widened fence and a plain one are both
/// read, and a line of output that happens to begin with backticks inside a block
/// is not mistaken for one.
fn fence_language(line: &str) -> Option<&str> {
    let ticks = line.bytes().take_while(|byte| *byte == b'`').count();
    if ticks < 3 {
        return None;
    }
    Some(line[ticks..].trim())
}

/// How long the fence around `input` has to be to survive it: three backticks,
/// or one more than the longest run of backticks the input holds, whichever is
/// longer.
fn fence_width(input: &str) -> usize {
    let mut longest = 0;
    let mut run = 0;
    for c in input.chars() {
        if c == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    longest.max(2) + 1
}

#[cfg(test)]
mod tests {
    use super::{fence_width, Exchange, Transcript, EMPTY, FENCE_LANGUAGE, HEADER, LIMIT};

    /// The lines of a fenced block, with `(language, body)` pairs in order.
    fn blocks(document: &str) -> Vec<(String, String)> {
        let mut found = Vec::new();
        let mut fence: Option<String> = None;
        let mut body: Vec<&str> = Vec::new();
        for line in document.lines() {
            let is_fence = line.starts_with("```");
            match (&fence, is_fence) {
                (None, true) => fence = Some(line.trim_start_matches('`').to_string()),
                (Some(_), true) => {
                    found.push((fence.take().unwrap_or_default(), body.join("\n")));
                    body.clear();
                }
                (Some(_), false) => body.push(line),
                (None, false) => {}
            }
        }
        found
    }

    /// A two-line session: what the exported document is pinned against.
    fn two_line_session() -> Transcript {
        let mut transcript = Transcript::new();
        transcript.record("say \"one\"".to_string(), vec!["one".to_string()]);
        transcript.record("say \"two\"".to_string(), vec!["two".to_string()]);
        transcript
    }

    #[test]
    fn an_exported_session_is_markdown_with_one_redblue_block_per_line() {
        let document = two_line_session().markdown();

        assert!(
            document.starts_with(HEADER),
            "it says what it is:\n{document}"
        );
        assert_eq!(
            document.matches("```redblue").count(),
            2,
            "one fence per line the session ran:\n{document}"
        );
        assert_eq!(
            blocks(&document),
            vec![
                (FENCE_LANGUAGE.to_string(), "say \"one\"".to_string()),
                (FENCE_LANGUAGE.to_string(), "say \"two\"".to_string()),
            ],
            "each block is what the user typed, in order:\n{document}"
        );
        assert_eq!(
            document.matches("\none\n").count(),
            1,
            "what a line said is a text block after it, not inside the fence:\n{document}"
        );
        assert!(
            document.ends_with("\n"),
            "the document ends on a newline, so it appends to nothing:\n{document:?}"
        );
    }

    /// The round trip, both halves of it: the blocks a document holds are the
    /// lines the session ran, in the order it ran them.
    ///
    /// Pinned on a fixed session and nothing about time or the terminal, because
    /// the claim is that a document is enough to run the session again.
    #[test]
    fn a_session_read_back_from_its_own_markdown_is_the_session() {
        let transcript = two_line_session();

        assert_eq!(
            Transcript::read_markdown(&transcript.markdown()),
            vec!["say \"one\"".to_string(), "say \"two\"".to_string()],
            "reading the document back gives the session that wrote it"
        );
    }

    /// An escape sequence must not reach the document, because a document is
    /// somewhere else — an issue, a file, another machine — and `\u{1b}[32m` in
    /// the middle of one is garbage there.
    #[test]
    fn edge_no_escape_sequence_can_reach_the_document() {
        let mut transcript = Transcript::new();
        transcript.record(
            "say \"one\"".to_string(),
            vec!["\u{1b}[32mone\u{1b}[0m".to_string()],
        );
        transcript.record(
            "set x to 1 +".to_string(),
            vec!["\u{1b}[31mError: ParserError: Expected expression\u{1b}[0m".to_string()],
        );

        let document = transcript.markdown();

        assert!(
            !document.contains('\u{1b}'),
            "the transcript is recorded in colour, and the document carries none \
             of it:\n{document:?}"
        );
        assert!(
            document.contains("\none\n"),
            "the text survives with only the paint taken off:\n{document}"
        );
        assert!(
            document.contains("Error: ParserError: Expected expression"),
            "and so does a failure, whole:\n{document}"
        );
    }

    /// A line the session typed that prints a fence would close the block it is
    /// written inside, and the rest of the session would export as prose.
    #[test]
    fn edge_a_line_that_prints_a_fence_is_written_inside_a_longer_one() {
        let mut transcript = Transcript::new();
        transcript.record("say \"```\"".to_string(), vec!["```".to_string()]);

        let document = transcript.markdown();

        assert!(
            document.starts_with(&format!("{HEADER}\n\n````{FENCE_LANGUAGE}\n")),
            "the fence is one backtick longer than the longest run inside it:\n{document}"
        );
        assert_eq!(
            Transcript::read_markdown(&document),
            vec!["say \"```\"".to_string()],
            "and the widened block still reads back:\n{document}"
        );
    }

    /// A session that has run nothing exports a document that says so, rather
    /// than a heading followed by a blank file.
    #[test]
    fn edge_a_session_that_has_run_nothing_says_so() {
        let transcript = Transcript::new();

        let document = transcript.markdown();

        assert_eq!(document, format!("{HEADER}\n\n{EMPTY}\n"));
        assert!(
            !document.contains("```"),
            "nothing ran, so nothing is fenced:\n{document}"
        );
        assert!(
            Transcript::read_markdown(&document).is_empty(),
            "and there is nothing to read back either"
        );
    }

    /// A session that ran nothing has to be able to say so, and the empty case is
    /// not the only one that has to be bounded: the *long* one is the direction a
    /// bound is for.
    #[test]
    fn edge_a_long_session_keeps_the_newest_exchanges_and_no_more() {
        let mut transcript = Transcript::new();
        for index in 0..LIMIT + 5 {
            transcript.record(format!("say \"{index}\""), vec![index.to_string()]);
        }

        assert_eq!(
            transcript.len(),
            LIMIT,
            "the transcript is bounded, and says how many it holds"
        );
        assert_eq!(
            transcript.exchanges()[0].input,
            format!("say \"{}\"", 5),
            "the oldest exchanges are the ones dropped: a reader of an export came \
             for the recent ones"
        );
        assert_eq!(
            transcript.exchanges()[LIMIT - 1].input,
            format!("say \"{}\"", LIMIT + 4),
            "and the newest is the last line the session ran"
        );

        let document = transcript.markdown();
        assert_eq!(
            document.matches("```redblue").count(),
            LIMIT,
            "and the document is bounded with it"
        );
    }

    /// A cleared transcript exports a session that has run nothing, because
    /// `:reset` says the session forgot it.
    #[test]
    fn edge_a_cleared_transcript_exports_an_empty_session() {
        let mut transcript = two_line_session();

        transcript.clear();

        assert!(
            transcript.is_empty(),
            "clearing is what a reset does to the record"
        );
        assert_eq!(transcript.markdown(), format!("{HEADER}\n\n{EMPTY}\n"));
    }

    /// A block the user typed over several lines is one exchange, not one per line.
    #[test]
    fn a_block_is_one_exchange_holding_every_line_it_was_typed_on() {
        let mut transcript = Transcript::new();
        transcript.record(
            "repeat 2 times\n    say \"hi\"\nend".to_string(),
            vec!["hi".to_string(), "hi".to_string()],
        );

        let document = transcript.markdown();

        assert_eq!(
            blocks(&document),
            vec![(
                FENCE_LANGUAGE.to_string(),
                "repeat 2 times\n    say \"hi\"\nend".to_string()
            )],
            "the block is written whole, indentation and all:\n{document}"
        );
        assert_eq!(
            Transcript::read_markdown(&document),
            vec!["repeat 2 times\n    say \"hi\"\nend".to_string()],
            "and reads back as the one block it is"
        );
    }

    /// An exchange that said nothing writes the fence and nothing else — no
    /// phantom blank paragraph between two blocks.
    #[test]
    fn an_exchange_that_said_nothing_writes_no_output_block() {
        let mut transcript = Transcript::new();
        transcript.record("set x to 1".to_string(), Vec::new());
        transcript.record("say \"one\"".to_string(), vec!["one".to_string()]);

        let document = transcript.markdown();

        assert_eq!(
            document,
            format!(
                "{HEADER}\n\n```redblue\nset x to 1\n```\n\n```redblue\nsay \"one\"\n```\none\n\n"
            ),
            "a `set` says nothing, so the block after it is the next fence:\n{document:?}"
        );
    }

    /// The fence a block is written inside is as long as it needs to be, which is
    /// the one number the renderer computes.
    #[test]
    fn edge_the_fence_is_one_backtick_longer_than_the_longest_run_inside_it() {
        assert_eq!(
            fence_width("say \"hi\""),
            3,
            "no backticks, the usual fence"
        );
        assert_eq!(fence_width("say \"`\""), 3, "one is still the usual fence");
        assert_eq!(fence_width("say \"``\""), 3);
        assert_eq!(fence_width("say \"```\""), 4);
        assert_eq!(
            fence_width("`` then `"),
            3,
            "the longest run wins, not the last"
        );
    }

    /// A hand-written or hand-edited document is still a document: what is read
    /// is every `redblue` block, whatever width the fences are.
    #[test]
    fn a_document_written_by_hand_is_read_the_same_way() {
        let document = "\
# A session

```redblue
set x to 1
```

```rust
fn main() {}
```

````redblue
say \"```
end\"
````
done

```redblue
say \"two\"
```

trailing
";

        assert_eq!(
            Transcript::read_markdown(document),
            vec![
                "set x to 1".to_string(),
                "say \"```\nend\"".to_string(),
                "say \"two\"".to_string(),
            ],
            "a block of another language is not a line of Redblue, a widened fence \
             is, and text outside a block is not read at all"
        );
    }

    /// A document that ends mid-block — a fence opened and never closed, which is
    /// what a truncated export is — is not a block that reads back as empty text.
    #[test]
    fn edge_an_unclosed_fence_at_the_end_of_a_document_reads_as_nothing() {
        assert!(
            Transcript::read_markdown("# A session\n\n```redblue\nsay \"one\"\n").is_empty(),
            "a block the document never closed is not a line of the session"
        );
    }

    /// `Exchange` is what a session hands the transcript, and the two fields are
    /// read apart by the renderer: the input is fenced and the output is not.
    #[test]
    fn an_exchange_keeps_its_input_apart_from_its_output() {
        let exchange = Exchange {
            input: "1 + 1".to_string(),
            output: vec!["= 2".to_string()],
        };
        let mut transcript = Transcript::new();
        transcript.record(exchange.input.clone(), exchange.output.clone());

        assert_eq!(transcript.exchanges(), &[exchange]);
        let document = transcript.markdown();
        assert!(document.contains("```redblue\n1 + 1\n```\n= 2\n"));
    }
}
