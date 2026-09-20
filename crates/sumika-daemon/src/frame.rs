use std::sync::Mutex;

use termwiz::color::ColorAttribute;
use termwiz::escape::csi::{CSI, Cursor, Edit, EraseInDisplay, EraseInLine};
use termwiz::escape::parser::Parser;
use termwiz::escape::{Action, ControlCode};
use termwiz::surface::{Change, Position, Surface};

pub struct Frame {
    inner: Mutex<Inner>,
}

struct Inner {
    parser: Parser,
    surface: Surface,
}

impl Frame {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                parser: Parser::new(),
                surface: Surface::new(cols, rows),
            }),
        }
    }

    pub fn feed(&self, bytes: &[u8]) {
        let mut inner = self.inner.lock().expect("frame");
        let Inner { parser, surface } = &mut *inner;
        parser.parse(bytes, |action| apply(surface, action));
    }

    pub fn snapshot(&self) -> Vec<u8> {
        let inner = self.inner.lock().expect("frame");
        let text = inner.surface.screen_chars_to_string();
        if text.chars().all(char::is_whitespace) {
            return Vec::new();
        }
        let mut out = Vec::from(&b"\x1b[H\x1b[J"[..]);
        out.extend_from_slice(text.as_bytes());
        out
    }
}

fn apply(surface: &mut Surface, action: Action) {
    match action {
        Action::Print(c) => {
            surface.add_change(Change::Text(c.to_string()));
        }
        Action::Control(ControlCode::LineFeed) => {
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
}
