//! Parses the `vt` output of `rex capture --unwrap` into rows of cells.
//!
//! The input has one line per logical line. A line longer than the pane width is wrapped here into
//! several rows, as the terminal does, and those rows are marked as wrapped.
//!
//! Text and SGR (colour) sequences are kept; OSC 8 destinations are stored as metadata only.
//! Every other escape sequence and every control character is dropped. Hyperlink destinations
//! must be absolute URIs without whitespace or control characters before they can reach an action.

use std::collections::HashMap;
use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy)]
pub struct Cell {
    pub ch: char,
    /// Columns the cell uses: 1 or 2. 0 marks the right half of a wide character; it is never drawn.
    pub width: u8,
    /// Index into `Screen::styles`.
    pub style: u32,
    /// Index into `Screen::hyperlinks`. 0 means no hyperlink.
    pub hyperlink: u32,
}

pub struct Screen {
    pub rows: Vec<Vec<Cell>>,
    /// `wrapped[i]` is true when row `i` continues on row `i + 1`.
    pub wrapped: Vec<bool>,
    /// Distinct SGR states. Index 0 is the default style (empty string).
    pub styles: Vec<String>,
    pub hyperlinks: Vec<String>,
}

/// A hyperlink's visible character range and its hidden destination.
pub struct Hyperlink {
    pub start: usize,
    pub end: usize,
    pub uri: String,
}

/// One logical line: the rows that belong together because the terminal wrapped them.
#[derive(Default)]
pub struct Line {
    pub text: String,
    /// Byte offset in `text` of each character.
    pub byte: Vec<usize>,
    /// (row, column) of each character.
    pub pos: Vec<(usize, usize)>,
    pub hyperlinks: Vec<Hyperlink>,
}

impl Line {
    /// Index of the character that starts at byte offset `b`. For `b == text.len()` it is the character count.
    pub fn char_index(&self, b: usize) -> usize {
        self.byte.partition_point(|&x| x < b)
    }

    pub fn char_at(&self, i: usize) -> Option<char> {
        let start = *self.byte.get(i)?;
        self.text[start..].chars().next()
    }
}

struct Builder {
    cols: usize,
    rows: Vec<Vec<Cell>>,
    wrapped: Vec<bool>,
    row: Vec<Cell>,
    styles: Vec<String>,
    index: HashMap<String, u32>,
    current: String,
    style: u32,
    hyperlinks: Vec<String>,
    hyperlink: u32,
}

impl Builder {
    fn new(cols: usize) -> Self {
        let mut index = HashMap::new();
        index.insert(String::new(), 0);
        Builder {
            cols,
            rows: Vec::new(),
            wrapped: Vec::new(),
            row: Vec::new(),
            styles: vec![String::new()],
            index,
            current: String::new(),
            style: 0,
            hyperlinks: vec![String::new()],
            hyperlink: 0,
        }
    }

    fn sgr(&mut self, params: &str) {
        // Only digits, ';' and ':' are replayed later, so nothing else can reach our own terminal.
        if !params.bytes().all(|b| b.is_ascii_digit() || b == b';' || b == b':') {
            return;
        }
        if params.is_empty() || params == "0" {
            self.current.clear();
        } else if let Some(rest) = params.strip_prefix("0;") {
            self.current.clear();
            self.current.push_str(&format!("\x1b[{rest}m"));
        } else {
            self.current.push_str(&format!("\x1b[{params}m"));
        }
        self.style = match self.index.get(&self.current) {
            Some(&i) => i,
            None => {
                let i = self.styles.len() as u32;
                self.styles.push(self.current.clone());
                self.index.insert(self.current.clone(), i);
                i
            }
        };
    }

    fn osc(&mut self, payload: &str) {
        let Some(rest) = payload.strip_prefix("8;") else {
            return;
        };
        self.hyperlink = 0;
        let Some((_, uri)) = rest.split_once(';') else {
            return;
        };
        let Some((scheme, _)) = uri.split_once(':') else {
            return;
        };
        if payload.chars().any(char::is_control)
            || uri.chars().any(char::is_whitespace)
            || !scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            || !scheme.bytes().all(|b| b.is_ascii_alphanumeric() || b"+-.".contains(&b))
        {
            return;
        }
        self.hyperlinks.push(uri.to_string());
        self.hyperlink = (self.hyperlinks.len() - 1) as u32;
    }

    /// `ESC [ n C` moves the cursor right. Rex uses it for a run of empty cells, so it becomes `n` blank
    /// cells. Dropping it would shift the rest of the row to the left. The cursor stops at the last column.
    fn forward(&mut self, params: &str) {
        if !params.bytes().all(|b| b.is_ascii_digit()) {
            return;
        }
        let n = params.parse::<usize>().unwrap_or(1).max(1);
        let room = if self.cols > 0 { self.cols.saturating_sub(self.row.len()) } else { 4096 };
        for _ in 0..n.min(room) {
            self.row.push(Cell { ch: ' ', width: 1, style: 0, hyperlink: 0 });
        }
    }

    fn put(&mut self, c: char) {
        let w = c.width().unwrap_or(0);
        if w == 0 {
            return;
        }
        if self.cols > 0 && self.row.len() + w > self.cols {
            self.rows.push(std::mem::take(&mut self.row));
            self.wrapped.push(true);
        }
        self.row.push(Cell { ch: c, width: w as u8, style: self.style, hyperlink: self.hyperlink });
        if w == 2 {
            self.row.push(Cell { ch: ' ', width: 0, style: self.style, hyperlink: self.hyperlink });
        }
    }

    /// A newline in the input always ends a logical line: the capture is unwrapped.
    fn newline(&mut self) {
        self.rows.push(std::mem::take(&mut self.row));
        self.wrapped.push(false);
    }

    fn finish(mut self) -> Screen {
        self.rows.push(std::mem::take(&mut self.row));
        self.wrapped.push(false);
        Screen {
            rows: self.rows,
            wrapped: self.wrapped,
            styles: self.styles,
            hyperlinks: self.hyperlinks,
        }
    }
}

impl Screen {
    /// Rex joins rows with "\r\n" and adds a bare "\n" only when the last row is not empty.
    /// So a final "\r\n" means the last row is blank and stays; a final bare "\n" is only a terminator.
    pub fn parse(vt: &str, cols: usize) -> Screen {
        let vt = match vt.strip_suffix('\n') {
            Some(rest) if !rest.ends_with('\r') => rest,
            _ => vt,
        };
        let mut b = Builder::new(cols);
        let mut it = vt.chars();
        while let Some(c) = it.next() {
            match c {
                '\x1b' => match it.next() {
                    Some('[') => {
                        let mut params = String::new();
                        for n in it.by_ref() {
                            if ('\x40'..='\x7e').contains(&n) {
                                match n {
                                    'm' => b.sgr(&params),
                                    'C' => b.forward(&params),
                                    _ => {}
                                }
                                break;
                            }
                            params.push(n);
                        }
                    }
                    Some(']') => {
                        // OSC ends with BEL or ESC \.
                        let mut payload = String::new();
                        for n in it.by_ref() {
                            if n == '\x07' {
                                b.osc(&payload);
                                break;
                            }
                            if n == '\\' && payload.ends_with('\x1b') {
                                payload.pop();
                                b.osc(&payload);
                                break;
                            }
                            payload.push(n);
                        }
                    }
                    _ => {}
                },
                '\n' => b.newline(),
                '\t' => {
                    for _ in 0..(8 - b.row.len() % 8) {
                        b.put(' ');
                    }
                }
                c if c.is_control() => {}
                c => b.put(c),
            }
        }
        b.finish()
    }

    /// Keeps only the last `n` rows.
    pub fn tail(mut self, n: usize) -> Screen {
        let drop = self.rows.len().saturating_sub(n);
        self.rows.drain(..drop);
        self.wrapped.drain(..drop);
        self
    }

    pub fn lines(&self) -> Vec<Line> {
        let mut out = Vec::new();
        let mut cur = Line::default();
        for (r, row) in self.rows.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                if cell.width == 0 {
                    continue;
                }
                let index = cur.pos.len();
                cur.byte.push(cur.text.len());
                cur.text.push(cell.ch);
                cur.pos.push((r, c));
                if cell.hyperlink != 0 {
                    let uri = &self.hyperlinks[cell.hyperlink as usize];
                    match cur.hyperlinks.last_mut() {
                        Some(link) if link.end == index && link.uri == *uri => link.end += 1,
                        _ => cur.hyperlinks.push(Hyperlink { start: index, end: index + 1, uri: uri.clone() }),
                    }
                }
            }
            if !self.wrapped[r] {
                out.push(std::mem::take(&mut cur));
            }
        }
        if !cur.pos.is_empty() {
            out.push(cur);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row_text(s: &Screen, r: usize) -> String {
        s.rows[r].iter().filter(|c| c.width > 0).map(|c| c.ch).collect()
    }

    #[test]
    fn keeps_text_and_drops_other_escapes_and_controls() {
        let s = Screen::parse("a\x1b[2Kb\x1b]8;;http://x\x07c\x1b]0;t\x1b\\d\x07e\n", 20);
        assert_eq!(row_text(&s, 0), "abcde");
    }

    #[test]
    fn cursor_forward_is_a_run_of_blank_cells() {
        let s = Screen::parse("a\x1b[5Cb\n\x1b[3Cx\n", 20);
        assert_eq!(row_text(&s, 0), "a     b");
        assert_eq!(row_text(&s, 1), "   x");
    }

    #[test]
    fn cursor_forward_without_a_number_moves_one_cell() {
        let s = Screen::parse("a\x1b[Cb\x1b[0Cc\n", 20);
        assert_eq!(row_text(&s, 0), "a b c");
    }

    #[test]
    fn cursor_forward_stops_at_the_last_column() {
        let s = Screen::parse("ab\x1b[50C\n", 10);
        assert_eq!(s.rows[0].len(), 10);
        assert_eq!(s.rows.len(), 1);
    }

    #[test]
    fn colour_change_gets_its_own_style() {
        let s = Screen::parse("\x1b[31mr\x1b[0mx\n", 20);
        assert_eq!(s.styles[s.rows[0][0].style as usize], "\x1b[31m");
        assert_eq!(s.rows[0][1].style, 0);
    }

    #[test]
    fn zero_prefix_resets_before_applying() {
        let s = Screen::parse("\x1b[31ma\x1b[0;1mb\n", 20);
        assert_eq!(s.styles[s.rows[0][1].style as usize], "\x1b[1m");
    }

    #[test]
    fn sgr_with_unexpected_bytes_is_ignored() {
        let s = Screen::parse("\x1b[?1mx\n", 20);
        assert_eq!(s.rows[0][0].style, 0);
    }

    #[test]
    fn wide_character_takes_two_cells() {
        let s = Screen::parse("日a\n", 20);
        let widths: Vec<u8> = s.rows[0].iter().map(|c| c.width).collect();
        assert_eq!(widths, vec![2, 0, 1]);
    }

    #[test]
    fn long_line_wraps_and_joins_into_one_logical_line() {
        let s = Screen::parse("abcdefghij\nxy\n", 4);
        assert_eq!(s.wrapped, vec![true, true, false, false]);
        assert_eq!(s.rows.len(), 4);
        let lines = s.lines();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "abcdefghij");
        assert_eq!(lines[0].pos[5], (1, 1));
    }

    #[test]
    fn full_width_line_ending_in_newline_is_not_wrapped() {
        let s = Screen::parse("abcd\nef\n", 4);
        let lines = s.lines();
        assert_eq!(lines[0].text, "abcd");
        assert_eq!(lines[1].text, "ef");
    }

    #[test]
    fn final_crlf_means_the_last_row_is_blank() {
        let s = Screen::parse("a\r\nb\r\n", 20);
        assert_eq!(s.rows.len(), 3);
        assert_eq!(row_text(&s, 2), "");
    }

    #[test]
    fn final_bare_newline_is_only_a_terminator() {
        let s = Screen::parse("a\r\nb\n", 20);
        assert_eq!(s.rows.len(), 2);
        assert_eq!(row_text(&s, 1), "b");
    }

    #[test]
    fn tail_keeps_the_last_rows() {
        let s = Screen::parse("a\nb\nc\n", 20).tail(2);
        assert_eq!(s.rows.len(), 2);
        assert_eq!(row_text(&s, 0), "b");
    }

    #[test]
    fn tab_expands_to_next_multiple_of_eight() {
        let s = Screen::parse("a\tb\n", 20);
        assert_eq!(row_text(&s, 0), "a       b");
    }

    #[test]
    fn osc8_keeps_destinations_separate_from_visible_text_with_either_terminator() {
        for end in ["\x07", "\x1b\\"] {
            let vt = format!("see \x1b]8;id=docs;https://example.com/docs;a=1{end}docs\x1b]8;;{end} end");
            let lines = Screen::parse(&vt, 80).lines();
            assert_eq!(lines[0].text, "see docs end");
            let link = &lines[0].hyperlinks[0];
            assert_eq!((link.start, link.end, link.uri.as_str()), (4, 8, "https://example.com/docs;a=1"));
        }
    }

    #[test]
    fn osc8_survives_style_changes_and_unrelated_osc() {
        let lines = Screen::parse(
            "\x1b]8;;https://example.com\x07a\x1b[31mb\x1b[0m\x1b]0;title\x07c\x1b]8;;\x07d",
            80,
        ).lines();
        assert_eq!(lines[0].text, "abcd");
        assert_eq!(lines[0].hyperlinks.len(), 1);
        assert_eq!((lines[0].hyperlinks[0].start, lines[0].hyperlinks[0].end), (0, 3));
    }

    #[test]
    fn osc8_ranges_follow_wide_characters_wrapping_and_trimming() {
        let screen = Screen::parse("\x1b]8;;file:///tmp/docs.md\x07日本語\x1b]8;;\x07!", 4).tail(1);
        let lines = screen.lines();
        assert_eq!(lines[0].text, "語!");
        assert_eq!(lines[0].pos, vec![(0, 0), (0, 2)]);
        let link = &lines[0].hyperlinks[0];
        assert_eq!((link.start, link.end, link.uri.as_str()), (0, 1, "file:///tmp/docs.md"));
    }

    #[test]
    fn osc8_closes_and_replaces_the_active_destination() {
        let lines = Screen::parse(
            "\x1b]8;;https://one\x07a\x1b]8;;\x07b\x1b]8;;https://two\x07c\x1b]8;;https://three\x07d\x1b]8;;\x07e",
            80,
        ).lines();
        let links: Vec<_> = lines[0].hyperlinks.iter().map(|l| (l.start, l.end, l.uri.as_str())).collect();
        assert_eq!(lines[0].text, "abcde");
        assert_eq!(links, vec![(0, 1, "https://one"), (2, 3, "https://two"), (3, 4, "https://three")]);
    }

    #[test]
    fn osc8_rejects_malformed_destinations_and_clears_the_previous_link() {
        for payload in [
            "8;missing-separator", "8;;", "8;;relative/path", "8;;--option", "8;;1bad:target",
            "8;;https://bad url", "8;;https://bad\u{009b}url", "8;id=bad\n;https://example.com",
        ] {
            let vt = format!("\x1b]8;;https://valid\x07a\x1b]{payload}\x1b\\x");
            let lines = Screen::parse(&vt, 80).lines();
            assert_eq!(lines[0].text, "ax");
            assert_eq!(lines[0].hyperlinks.len(), 1, "{payload:?}");
            assert_eq!(lines[0].hyperlinks[0].end, 1, "{payload:?}");
        }
    }

    #[test]
    fn unfinished_osc8_is_not_applied_or_shown() {
        let screen = Screen::parse("visible\x1b]8;;https://hidden", 80);
        assert_eq!(screen.lines()[0].text, "visible");
        assert!(screen.lines()[0].hyperlinks.is_empty());
        assert_eq!(screen.hyperlinks, vec![String::new()]);
    }
}
