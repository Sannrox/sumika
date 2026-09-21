use std::collections::VecDeque;
use std::sync::Mutex;

use termwiz::cell::{AttributeChange, Blink, CellAttributes, Intensity, Underline};
use termwiz::color::ColorAttribute;
use termwiz::escape::csi::{CSI, Cursor, Edit, EraseInDisplay, EraseInLine, Sgr};
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
    pen: CellAttributes,
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
                pen: CellAttributes::default(),
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
            pen,
        } = &mut *inner;
        let cap = *cap;
        parser.parse(bytes, |action| apply(surface, lines, cap, pen, action));
    }

    pub fn resize(&self, cols: usize, rows: usize) {
        if cols == 0 || rows == 0 {
            return;
        }
        let mut inner = self.inner.lock().expect("frame");
        let Inner {
            surface,
            lines,
            cap,
            ..
        } = &mut *inner;
        let (_old_cols, old_rows) = surface.dimensions();
        if rows < old_rows {
            let drop = old_rows - rows;
            for _ in 0..drop {
                capture_evicted_top(surface, lines, *cap);
                surface.add_change(Change::ScrollRegionUp {
                    first_row: 0,
                    region_size: old_rows,
                    scroll_count: 1,
                });
            }
            let (x, y) = surface.cursor_position();
            surface.add_change(Change::CursorPosition {
                x: Position::Absolute(x),
                y: Position::Absolute(y.saturating_sub(drop)),
            });
        }
        surface.resize(cols, rows);
    }

    #[cfg(test)]
    pub fn dimensions(&self) -> (usize, usize) {
        let inner = self.inner.lock().expect("frame");
        inner.surface.dimensions()
    }

    pub fn snapshot(&self) -> Vec<u8> {
        let inner = self.inner.lock().expect("frame");
        snapshot_bytes(&inner.surface, &inner.pen)
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
        out.extend_from_slice(&snapshot_bytes(&inner.surface, &inner.pen));
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

fn snapshot_bytes(surface: &Surface, pen: &CellAttributes) -> Vec<u8> {
    if snapshot_is_blank(surface) {
        if pen == &CellAttributes::default() {
            return Vec::new();
        }
        let mut out = Vec::from(&b"\x1b[H\x1b[J"[..]);
        push_sgr(&mut out, pen);
        let (x, y) = surface.cursor_position();
        out.extend_from_slice(format!("\x1b[{};{}H", y + 1, x + 1).as_bytes());
        return out;
    }
    let mut out = Vec::from(&b"\x1b[H\x1b[J"[..]);
    let mut last = CellAttributes::default();
    let rows = surface.screen_lines();
    for (i, line) in rows.iter().enumerate() {
        let mut col = 0usize;
        for cell in line.visible_cells() {
            let idx = cell.cell_index();
            if col < idx {
                if last != CellAttributes::default() {
                    out.extend_from_slice(b"\x1b[0m");
                    last = CellAttributes::default();
                }
                out.resize(out.len() + (idx - col), b' ');
                col = idx;
            }
            let attrs = cell.attrs();
            if attrs != &last {
                push_sgr(&mut out, attrs);
                last = attrs.clone();
            }
            out.extend_from_slice(cell.str().as_bytes());
            col += cell.width();
        }
        if i + 1 < rows.len() {
            out.extend_from_slice(b"\r\n");
        }
    }
    if last != *pen {
        if pen == &CellAttributes::default() {
            out.extend_from_slice(b"\x1b[0m");
        } else {
            push_sgr(&mut out, pen);
        }
    }
    let (x, y) = surface.cursor_position();
    out.extend_from_slice(format!("\x1b[{};{}H", y + 1, x + 1).as_bytes());
    out
}

fn snapshot_is_blank(surface: &Surface) -> bool {
    surface.screen_lines().iter().all(|line| {
        line.visible_cells().all(|cell| {
            cell.str().chars().all(char::is_whitespace)
                && cell.attrs() == &CellAttributes::default()
        })
    })
}

fn push_sgr(out: &mut Vec<u8>, attrs: &CellAttributes) {
    out.extend_from_slice(b"\x1b[0");
    match attrs.intensity() {
        Intensity::Bold => out.extend_from_slice(b";1"),
        Intensity::Half => out.extend_from_slice(b";2"),
        Intensity::Normal => {}
    }
    if attrs.italic() {
        out.extend_from_slice(b";3");
    }
    match attrs.underline() {
        Underline::None => {}
        Underline::Single => out.extend_from_slice(b";4"),
        Underline::Double => out.extend_from_slice(b";21"),
        Underline::Curly => out.extend_from_slice(b";4:3"),
        Underline::Dotted => out.extend_from_slice(b";4:4"),
        Underline::Dashed => out.extend_from_slice(b";4:5"),
    }
    match attrs.blink() {
        Blink::None => {}
        Blink::Slow => out.extend_from_slice(b";5"),
        Blink::Rapid => out.extend_from_slice(b";6"),
    }
    if attrs.reverse() {
        out.extend_from_slice(b";7");
    }
    if attrs.invisible() {
        out.extend_from_slice(b";8");
    }
    if attrs.strikethrough() {
        out.extend_from_slice(b";9");
    }
    push_color_params(out, 38, 30, 90, attrs.foreground());
    push_color_params(out, 48, 40, 100, attrs.background());
    out.push(b'm');
}

fn push_color_params(out: &mut Vec<u8>, set: u8, normal: u8, bright: u8, color: ColorAttribute) {
    match color {
        ColorAttribute::Default => {}
        ColorAttribute::PaletteIndex(idx) if idx < 8 => {
            out.extend_from_slice(format!(";{}", normal + idx).as_bytes());
        }
        ColorAttribute::PaletteIndex(idx) if idx < 16 => {
            out.extend_from_slice(format!(";{}", bright + (idx - 8)).as_bytes());
        }
        ColorAttribute::PaletteIndex(idx) => {
            out.extend_from_slice(format!(";{set};5;{idx}").as_bytes());
        }
        ColorAttribute::TrueColorWithPaletteFallback(rgb, _)
        | ColorAttribute::TrueColorWithDefaultFallback(rgb) => {
            let (red, green, blue, _) = rgb.to_srgb_u8();
            out.extend_from_slice(format!(";{set};2;{red};{green};{blue}").as_bytes());
        }
    }
}

fn apply(
    surface: &mut Surface,
    lines: &mut VecDeque<String>,
    cap: usize,
    pen: &mut CellAttributes,
    action: Action,
) {
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
        Action::CSI(CSI::Edit(edit)) => apply_edit(surface, pen, edit),
        Action::CSI(CSI::Sgr(sgr)) => apply_sgr(surface, pen, sgr),
        _ => {}
    }
}

fn apply_sgr(surface: &mut Surface, pen: &mut CellAttributes, sgr: Sgr) {
    match sgr {
        Sgr::Reset => {
            *pen = CellAttributes::default();
            surface.add_change(Change::AllAttributes(pen.clone()));
        }
        Sgr::Intensity(value) => {
            pen.set_intensity(value);
            surface.add_change(Change::Attribute(AttributeChange::Intensity(value)));
        }
        Sgr::Underline(value) => {
            pen.set_underline(value);
            surface.add_change(Change::Attribute(AttributeChange::Underline(value)));
        }
        Sgr::Blink(value) => {
            pen.set_blink(value);
            surface.add_change(Change::Attribute(AttributeChange::Blink(value)));
        }
        Sgr::Italic(value) => {
            pen.set_italic(value);
            surface.add_change(Change::Attribute(AttributeChange::Italic(value)));
        }
        Sgr::Inverse(value) => {
            pen.set_reverse(value);
            surface.add_change(Change::Attribute(AttributeChange::Reverse(value)));
        }
        Sgr::Invisible(value) => {
            pen.set_invisible(value);
            surface.add_change(Change::Attribute(AttributeChange::Invisible(value)));
        }
        Sgr::StrikeThrough(value) => {
            pen.set_strikethrough(value);
            surface.add_change(Change::Attribute(AttributeChange::StrikeThrough(value)));
        }
        Sgr::Foreground(spec) => {
            let color = ColorAttribute::from(spec);
            pen.set_foreground(color);
            surface.add_change(Change::Attribute(AttributeChange::Foreground(color)));
        }
        Sgr::Background(spec) => {
            let color = ColorAttribute::from(spec);
            pen.set_background(color);
            surface.add_change(Change::Attribute(AttributeChange::Background(color)));
        }
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

fn apply_edit(surface: &mut Surface, pen: &CellAttributes, edit: Edit) {
    let background = pen.background();
    match edit {
        Edit::EraseInDisplay(EraseInDisplay::EraseDisplay)
        | Edit::EraseInDisplay(EraseInDisplay::EraseToEndOfDisplay)
        | Edit::EraseInDisplay(EraseInDisplay::EraseToStartOfDisplay) => {
            surface.add_change(Change::ClearScreen(background));
            surface.add_change(Change::AllAttributes(pen.clone()));
        }
        Edit::EraseInLine(EraseInLine::EraseToEndOfLine)
        | Edit::EraseInLine(EraseInLine::EraseToStartOfLine)
        | Edit::EraseInLine(EraseInLine::EraseLine) => {
            surface.add_change(Change::ClearToEndOfLine(background));
            surface.add_change(Change::AllAttributes(pen.clone()));
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

    #[test]
    fn snapshot_restores_the_live_pen() {
        let frame = Frame::new(80, 24);
        frame.feed(b"\x1b[31mRED-LIVE");
        let shot = String::from_utf8_lossy(&frame.snapshot()).into_owned();
        let cup = shot.rfind("\u{1b}[").expect("cup");
        let before = &shot[..cup];
        assert!(
            !before.ends_with("\u{1b}[0m"),
            "snapshot reset the live pen: {shot:?}"
        );
        assert!(
            before.contains("\u{1b}[31m") || before.contains("\u{1b}[0;31m"),
            "live red pen missing: {shot:?}"
        );
    }

    #[test]
    fn snapshot_keeps_sgr_red() {
        let frame = Frame::new(80, 24);
        frame.feed(b"\x1b[31mRED-MARKER\x1b[0m");
        let shot = frame.snapshot();
        let text = String::from_utf8_lossy(&shot);
        assert!(text.contains("RED-MARKER"), "{text:?}");
        assert!(
            text.contains("\x1b[31m") || text.contains("\x1b[0;31m"),
            "missing red SGR in {text:?}"
        );
    }

    #[test]
    fn resize_keeps_a_marker_past_column_80() {
        let frame = Frame::new(80, 24);
        frame.resize(120, 24);
        assert_eq!(frame.dimensions(), (120, 24));
        frame.feed(b"\x1b[1;100HCOL100-MARKER");
        let shot = String::from_utf8_lossy(&frame.snapshot()).into_owned();
        let start = shot.find("COL100-MARKER").expect(&shot);
        let row = shot[..start]
            .rsplit("\r\n")
            .next()
            .unwrap_or(&shot[..start]);
        let visible = strip_csi(row);
        assert!(
            visible.chars().count() >= 99,
            "marker wrapped into 80 columns: {visible:?} shot={shot:?}"
        );
    }

    #[test]
    fn shrink_height_keeps_the_bottom_and_rings_the_top() {
        let frame = Frame::with_scrollback(80, 4, 8);
        frame.feed(b"TOP-UNIQUE\r\na\r\nb\r\nBOTTOM-UNIQUE");
        frame.resize(80, 2);
        assert_eq!(frame.dimensions(), (80, 2));
        let shot = String::from_utf8_lossy(&frame.snapshot()).into_owned();
        let history = String::from_utf8(frame.history()).unwrap();
        assert!(
            shot.contains("BOTTOM-UNIQUE"),
            "bottom of last frame dropped: {shot:?}"
        );
        assert!(
            !shot.contains("TOP-UNIQUE"),
            "top should have left the viewport: {shot:?}"
        );
        assert!(
            history.contains("TOP-UNIQUE"),
            "top missing from ring: {history:?}"
        );
        assert!(
            shot.contains("\u{1b}[2;"),
            "cursor should stay on the kept bottom row: {shot:?}"
        );
    }

    #[test]
    fn blank_snapshot_still_restores_a_live_pen() {
        let frame = Frame::new(80, 24);
        frame.feed(b"\x1b[31m");
        let shot = String::from_utf8_lossy(&frame.snapshot()).into_owned();
        assert!(!shot.is_empty(), "live pen on a blank surface was dropped");
        assert!(
            shot.contains("\u{1b}[31m") || shot.contains("\u{1b}[0;31m"),
            "missing live red pen: {shot:?}"
        );
    }

    #[test]
    fn snapshot_does_not_lf_after_the_last_row() {
        let frame = Frame::new(80, 2);
        frame.feed(b"A\r\nB");
        let shot = frame.snapshot();
        let cup = b"\x1b[";
        let idx = shot.windows(2).rposition(|w| w == cup).expect("cup");
        assert!(
            !shot[..idx].ends_with(b"\r\n"),
            "last-row LF would scroll a matching-size client: {}",
            String::from_utf8_lossy(&shot)
        );
    }

    #[test]
    fn styled_blank_cells_are_not_an_empty_snapshot() {
        let frame = Frame::new(80, 24);
        frame.feed(b"\x1b[44m\x1b[2J");
        let shot = frame.snapshot();
        assert!(!shot.is_empty(), "background-only last frame was dropped");
        let text = String::from_utf8_lossy(&shot);
        assert!(
            text.contains("\x1b[44m") || text.contains(";44m") || text.contains(";44"),
            "missing background SGR in {text:?}"
        );
    }

    #[test]
    fn snapshot_keeps_underline() {
        let frame = Frame::new(80, 24);
        frame.feed(b"\x1b[4mUNDER-MARKER\x1b[0m");
        let shot = String::from_utf8_lossy(&frame.snapshot()).into_owned();
        assert!(shot.contains("UNDER-MARKER"), "{shot:?}");
        assert!(
            shot.contains("\x1b[4m") || shot.contains(";4m") || shot.contains(";4;"),
            "missing underline SGR in {shot:?}"
        );
    }

    fn strip_csi(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                if chars.peek() == Some(&'[') {
                    chars.next();
                    for next in chars.by_ref() {
                        if next.is_ascii_alphabetic() {
                            break;
                        }
                    }
                    continue;
                }
                continue;
            }
            out.push(c);
        }
        out
    }
}
