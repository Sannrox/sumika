use std::io::{self, Write};

use crossterm::cursor::{MoveTo, Show};
use crossterm::event::DisableMouseCapture;
use crossterm::execute;
use crossterm::style::ResetColor;
use crossterm::terminal::{Clear, ClearType, LeaveAlternateScreen};

pub fn write_client_restore(out: &mut impl Write) -> io::Result<()> {
    execute!(
        out,
        LeaveAlternateScreen,
        DisableMouseCapture,
        Show,
        ResetColor,
        Clear(ClearType::All),
        MoveTo(0, 0)
    )?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_clears_the_primary_screen() {
        let mut buf = Vec::new();
        write_client_restore(&mut buf).unwrap();
        let text = String::from_utf8_lossy(&buf);
        assert!(text.contains("\u{1b}[2J"), "missing clear in {text:?}");
    }
}
