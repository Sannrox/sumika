use std::env;
use std::fs;
use std::io::{self, Write};
use std::process::{Command, Stdio};

pub struct CopyMode {
    lines: Vec<String>,
    cursor: usize,
    search: Option<String>,
    typing: Option<String>,
}

#[derive(Debug)]
pub enum Action {
    Stay,
    Leave,
    Yank(String),
}

impl CopyMode {
    pub fn new(lines: Vec<String>) -> Self {
        let cursor = lines.len().saturating_sub(1);
        Self {
            lines,
            cursor,
            search: None,
            typing: None,
        }
    }

    pub fn current(&self) -> Option<&str> {
        self.lines.get(self.cursor).map(String::as_str)
    }

    pub fn feed(&mut self, byte: u8) -> Action {
        if let Some(buf) = &mut self.typing {
            match byte {
                0x1b | 0x03 => self.typing = None,
                0x0d | 0x0a => {
                    let query = buf.clone();
                    self.typing = None;
                    self.search = Some(query);
                    self.find(true);
                }
                0x7f | 0x08 => {
                    buf.pop();
                }
                b if (32..127).contains(&b) => buf.push(b as char),
                _ => {}
            }
            return Action::Stay;
        }
        match byte {
            b'q' | 0x03 => Action::Leave,
            b'j' => {
                if self.cursor + 1 < self.lines.len() {
                    self.cursor += 1;
                }
                Action::Stay
            }
            b'k' => {
                self.cursor = self.cursor.saturating_sub(1);
                Action::Stay
            }
            b'g' => {
                self.cursor = 0;
                Action::Stay
            }
            b'G' => {
                self.cursor = self.lines.len().saturating_sub(1);
                Action::Stay
            }
            b'y' => self
                .current()
                .map(|line| Action::Yank(line.to_string()))
                .unwrap_or(Action::Stay),
            b'/' => {
                self.typing = Some(String::new());
                Action::Stay
            }
            b'n' => {
                self.find(true);
                Action::Stay
            }
            b'N' => {
                self.find(false);
                Action::Stay
            }
            _ => Action::Stay,
        }
    }

    fn find(&mut self, forward: bool) {
        let Some(query) = self.search.as_ref().filter(|q| !q.is_empty()) else {
            return;
        };
        if self.lines.is_empty() {
            return;
        }
        let n = self.lines.len();
        for step in 1..=n {
            let idx = if forward {
                (self.cursor + step) % n
            } else {
                (self.cursor + n - (step % n)) % n
            };
            if self.lines[idx].contains(query.as_str()) {
                self.cursor = idx;
                return;
            }
        }
    }
}

pub fn yank(text: &str) -> io::Result<()> {
    if let Ok(path) = env::var("SUMIKA_CLIPBOARD_FILE")
        && !path.is_empty()
    {
        return fs::write(path, format!("{text}\n"));
    }
    #[cfg(target_os = "macos")]
    {
        let mut child = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }
        let _ = child.wait();
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        for (bin, args) in [
            ("wl-copy", &[][..]),
            ("xclip", &["-selection", "clipboard"][..]),
        ] {
            if let Ok(mut child) = Command::new(bin)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(text.as_bytes());
                }
                if child.wait().map(|status| status.success()).unwrap_or(false) {
                    return Ok(());
                }
            }
        }
        Ok(())
    }
}

pub fn render(mode: &CopyMode, rows: usize) -> String {
    let rows = rows.max(3);
    let view = rows.saturating_sub(2);
    let start = if mode.cursor >= view {
        mode.cursor + 1 - view
    } else {
        0
    };
    let mut out = String::from("\x1b[H\x1b[J");
    for (i, line) in mode.lines.iter().enumerate().skip(start).take(view) {
        if i == mode.cursor {
            out.push_str("\x1b[7m");
            out.push_str(line);
            out.push_str("\x1b[0m\r\n");
        } else {
            out.push_str(line);
            out.push_str("\r\n");
        }
    }
    if let Some(buf) = &mode.typing {
        out.push_str(&format!("/{buf}"));
    } else {
        out.push_str("y yank  q leave  j/k  / search");
    }
    out
}

pub fn clear_viewport() -> &'static [u8] {
    b"\x1b[H\x1b[J"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g_then_y_yanks_the_first_line() {
        let mut mode = CopyMode::new(vec!["UNIQUE-COPY-LINE".into(), "later".into()]);
        assert!(matches!(mode.feed(b'g'), Action::Stay));
        match mode.feed(b'y') {
            Action::Yank(text) => assert_eq!(text, "UNIQUE-COPY-LINE"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn search_keeps_the_match_as_cursor() {
        let mut mode = CopyMode::new(vec!["aaa".into(), "needle".into(), "ccc".into()]);
        mode.feed(b'/');
        for b in b"needle" {
            mode.feed(*b);
        }
        mode.feed(b'\n');
        assert_eq!(mode.current(), Some("needle"));
        mode.feed(b'n');
        assert_eq!(mode.current(), Some("needle"));
    }
}
