use std::collections::VecDeque;
use std::sync::Mutex;

use termwiz::color::ColorAttribute;
use termwiz::escape::csi::{CSI, Cursor, Edit, EraseInDisplay, EraseInLine};
use termwiz::escape::parser::Parser;
use termwiz::escape::{Action, ControlCode};
use termwiz::surface::{Change, Position, Surface};

pub const SCROLLBACK_LINES: usize = 10_000;
/// Attach does not yet know the client height. Pad enough that history
/// leaves an ordinary viewport before the snapshot homes and erases.
const REPLAY_PAD_ROWS: usize = 256;

pub struct Frame {
    inner: Mutex<Inner>,
}

struct Inner {
    parser: Parser,
    surface: Surface,
    lines: VecDeque<String>,
    cap: usize,
}

impl Frame {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self::with_scrollback(cols, rows, SCROLLBACK_LINES)
    }

    pub fn with_scrollback(cols: usize, rows: usize, cap: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                parser: Parser::new(),
                surface: Surface::new(cols, rows),
                lines: VecDeque::new(),
                cap,
            }),
        }
    }

    pub fn feed(&self, bytes: &[u8]) {
        let mut inner = self.inner.lock().expect("frame");
        let Inner {
            parser,
            surface,
            lines,
            cap,
        } = &mut *inner;
        let cap = *cap;
        parser.parse(bytes, |action| apply(surface, lines, cap, action));
    }

    pub fn snapshot(&self) -> Vec<u8> {
        let inner = self.inner.lock().expect("frame");
        snapshot_bytes(&inner.surface)
    }

    #[cfg(test)]
    pub fn history(&self) -> Vec<u8> {
        let inner = self.inner.lock().expect("frame");
        history_bytes(&inner.lines)
    }

    pub fn copy_lines(&self) -> Vec<String> {
        let inner = self.inner.lock().expect("frame");
        let mut out: Vec<String> = inner.lines.iter().cloned().collect();
        for row in inner.surface.screen_lines() {
            out.push(row.as_str().trim_end().to_string());
        }
        while out.last().is_some_and(|line| line.is_empty()) {
            out.pop();
        }
        out
    }

    pub fn replay(&self) -> Vec<u8> {
        let inner = self.inner.lock().expect("frame");
        let mut out = history_bytes(&inner.lines);
        if !out.is_empty() {
            let (_cols, rows) = inner.surface.dimensions();
            for _ in 0..rows.max(REPLAY_PAD_ROWS) {
                out.extend_from_slice(b"\r\n");
            }
        }
        out.extend_from_slice(&snapshot_bytes(&inner.surface));
        out
    }
}

fn history_bytes(lines: &VecDeque<String>) -> Vec<u8> {
    if lines
        .iter()
        .all(|line| line.chars().all(char::is_whitespace))
    {
        return Vec::new();
    }
    let mut out = Vec::new();
    for line in lines {
        out.extend_from_slice(line.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    out
}

fn push_line(lines: &mut VecDeque<String>, cap: usize, text: String) {
    if cap == 0 {
        return;
    }
    if lines.len() >= cap {
        lines.pop_front();
    }
    lines.push_back(text);
}

fn row_text(surface: &Surface, row: usize) -> Option<String> {
    let lines = surface.screen_lines();
    lines
        .get(row)
        .map(|line| line.as_str().trim_end().to_string())
}

fn capture_evicted_top(surface: &Surface, lines: &mut VecDeque<String>, cap: usize) {
    if let Some(text) = row_text(surface, 0) {
        push_line(lines, cap, text);
    }
}

fn line_feed_will_scroll(surface: &Surface) -> bool {
    let (_cols, rows) = surface.dimensions();
    if rows == 0 {
        return false;
    }
    let (_x, y) = surface.cursor_position();
    y + 1 >= rows
}

fn print_will_wrap_scroll(surface: &Surface) -> bool {
    let (cols, rows) = surface.dimensions();
    if cols == 0 || rows == 0 {
        return false;
    }
    let (x, y) = surface.cursor_position();
    // termwiz defers wrap until x is already past the last column.
    y + 1 >= rows && x >= cols
}

fn snapshot_bytes(surface: &Surface) -> Vec<u8> {
    let text = surface.screen_chars_to_string();
    if text.chars().all(char::is_whitespace) {
        return Vec::new();
    }
    let mut out = Vec::from(&b"\x1b[H\x1b[J"[..]);
    out.extend_from_slice(text.replace('\n', "\r\n").as_bytes());
    let (x, y) = surface.cursor_position();
    out.extend_from_slice(format!("\x1b[{};{}H", y + 1, x + 1).as_bytes());
    out
}

fn apply(surface: &mut Surface, lines: &mut VecDeque<String>, cap: usize, action: Action) {
    match action {
        Action::Print(c) => {
            if print_will_wrap_scroll(surface) {
                capture_evicted_top(surface, lines, cap);
            }
            surface.add_change(Change::Text(c.to_string()));
        }
        Action::Control(ControlCode::LineFeed) => {
            if line_feed_will_scroll(surface) {
                capture_evicted_top(surface, lines, cap);
            }
            surface.add_change(Change::Text("\n".into()));
        }
        Action::Control(ControlCode::CarriageReturn) => {
            surface.add_change(Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Relative(0),
            });
        }
        Action::Control(ControlCode::Backspace) => {
            surface.add_change(Change::CursorPosition {
                x: Position::Relative(-1),
                y: Position::Relative(0),
            });
        }
        Action::CSI(CSI::Cursor(cursor)) => apply_cursor(surface, cursor),
        Action::CSI(CSI::Edit(edit)) => apply_edit(surface, edit),
        _ => {}
    }
}

fn apply_cursor(surface: &mut Surface, cursor: Cursor) {
    match cursor {
        Cursor::Left(n) => {
            surface.add_change(Change::CursorPosition {
                x: Position::Relative(-(n as isize)),
                y: Position::Relative(0),
            });
        }
        Cursor::Right(n) => {
            surface.add_change(Change::CursorPosition {
                x: Position::Relative(n as isize),
                y: Position::Relative(0),
            });
        }
        Cursor::Up(n) => {
            surface.add_change(Change::CursorPosition {
                x: Position::Relative(0),
                y: Position::Relative(-(n as isize)),
            });
        }
        Cursor::Down(n) => {
            surface.add_change(Change::CursorPosition {
                x: Position::Relative(0),
                y: Position::Relative(n as isize),
            });
        }
        Cursor::Position { line, col } | Cursor::CharacterAndLinePosition { line, col } => {
            surface.add_change(Change::CursorPosition {
                x: Position::Absolute(col.as_zero_based() as usize),
                y: Position::Absolute(line.as_zero_based() as usize),
            });
        }
        Cursor::CharacterAbsolute(col) => {
            surface.add_change(Change::CursorPosition {
                x: Position::Absolute(col.as_zero_based() as usize),
                y: Position::Relative(0),
            });
        }
        _ => {}
    }
}

fn apply_edit(surface: &mut Surface, edit: Edit) {
    match edit {
        Edit::EraseInDisplay(EraseInDisplay::EraseDisplay)
        | Edit::EraseInDisplay(EraseInDisplay::EraseToEndOfDisplay)
        | Edit::EraseInDisplay(EraseInDisplay::EraseToStartOfDisplay) => {
            surface.add_change(Change::ClearScreen(ColorAttribute::Default));
        }
        Edit::EraseInLine(EraseInLine::EraseToEndOfLine)
        | Edit::EraseInLine(EraseInLine::EraseToStartOfLine)
        | Edit::EraseInLine(EraseInLine::EraseLine) => {
            surface.add_change(Change::ClearToEndOfLine(ColorAttribute::Default));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_keeps_printed_line() {
        let frame = Frame::new(80, 24);
        frame.feed(b"UNIQUE-SNAPSHOT-XYZ\n");
        let shot = String::from_utf8(frame.snapshot()).unwrap();
        assert!(
            shot.contains("UNIQUE-SNAPSHOT-XYZ"),
            "missing line in {shot:?}"
        );
    }

    #[test]
    fn replay_keeps_lines_that_left_the_viewport() {
        let frame = Frame::new(80, 24);
        for i in 0..40 {
            frame.feed(format!("SCROLL-{i:03}-UNIQUE\n").as_bytes());
        }
        let shot = String::from_utf8(frame.snapshot()).unwrap();
        let replay = String::from_utf8(frame.replay()).unwrap();
        assert!(
            !shot.contains("SCROLL-000-UNIQUE"),
            "first line should have left the last frame: {shot:?}"
        );
        assert!(
            replay.contains("SCROLL-000-UNIQUE"),
            "first line missing from ring: {replay:?}"
        );
        assert!(
            replay.contains("SCROLL-039-UNIQUE"),
            "latest line missing from replay: {replay:?}"
        );
    }

    #[test]
    fn ring_drops_oldest_past_cap() {
        let frame = Frame::with_scrollback(80, 2, 3);
        frame.feed(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\n");
        let history = String::from_utf8(frame.history()).unwrap();
        assert_eq!(history, "three\r\nfour\r\nfive\r\n");
    }

    #[test]
    fn history_is_rendered_text_with_crlf() {
        let frame = Frame::new(80, 2);
        frame.feed(b"\x1b[2J\x1b[Hhello\r\nworld\r\nnext\r\n");
        let history = String::from_utf8(frame.history()).unwrap();
        assert!(
            !history.contains('\u{1b}'),
            "csi leaked into history: {history:?}"
        );
        assert!(
            history.contains("hello\r\n"),
            "evicted line missing from history: {history:?}"
        );
    }

    #[test]
    fn replay_pads_history_into_client_scrollback() {
        let frame = Frame::new(80, 24);
        for i in 0..25 {
            frame.feed(format!("L{i}\r\n").as_bytes());
        }
        let history = frame.history();
        assert!(!history.is_empty());
        let replay = frame.replay();
        let shot = b"\x1b[H\x1b[J";
        let idx = replay.windows(shot.len()).position(|w| w == shot).unwrap();
        let mut expected = history;
        for _ in 0..REPLAY_PAD_ROWS {
            expected.extend_from_slice(b"\r\n");
        }
        assert_eq!(&replay[..idx], expected.as_slice());
    }

    #[test]
    fn copy_lines_include_evicted_and_viewport() {
        let frame = Frame::new(80, 24);
        frame.feed(b"UNIQUE-COPY-LINE\r\n");
        for i in 0..40 {
            frame.feed(format!("pad-{i}\r\n").as_bytes());
        }
        let lines = frame.copy_lines();
        assert!(
            lines.iter().any(|line| line.contains("UNIQUE-COPY-LINE")),
            "{lines:?}"
        );
    }

    #[test]
    fn wrap_evicts_the_top_row_once() {
        let frame = Frame::with_scrollback(4, 2, 8);
        frame.feed(b"abcdefgh");
        frame.feed(b"i");
        let history = String::from_utf8(frame.history()).unwrap();
        assert_eq!(history.matches("abcd").count(), 1, "{history:?}");
        assert!(history.contains("abcd\r\n"), "{history:?}");
    }
}
