#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub first: u8,
    pub second: u8,
}

impl Default for Chord {
    fn default() -> Self {
        Self {
            first: ctrl('\\'),
            second: ctrl('b'),
        }
    }
}

const fn ctrl(c: char) -> u8 {
    (c as u8) & 0x1f
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feed {
    Forward,
    Detach,
}

#[derive(Debug, Clone)]
pub struct Matcher {
    chord: Chord,
    pending: bool,
}

impl Matcher {
    pub fn new(chord: Chord) -> Self {
        Self {
            chord,
            pending: false,
        }
    }

    pub fn feed(&mut self, bytes: &[u8], out: &mut Vec<u8>) -> Feed {
        for &b in bytes {
            if self.pending {
                self.pending = false;
                if b == self.chord.second {
                    return Feed::Detach;
                }
                out.push(self.chord.first);
            }
            if b == self.chord.first {
                self.pending = true;
            } else {
                out.push(b);
            }
        }
        Feed::Forward
    }

    pub fn flush_pending(&mut self, out: &mut Vec<u8>) {
        if self.pending {
            self.pending = false;
            out.push(self.chord.first);
        }
    }
}

pub fn parse_key(spec: &str) -> Result<u8, String> {
    let spec = spec.trim();
    let rest = spec
        .strip_prefix("C-")
        .or_else(|| spec.strip_prefix("c-"))
        .or_else(|| spec.strip_prefix("ctrl-"))
        .or_else(|| spec.strip_prefix("CTRL-"))
        .ok_or_else(|| format!("unsupported key {spec}"))?;
    let mut chars = rest.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Ok(ctrl(c)),
        _ => Err(format!("unsupported key {spec}")),
    }
}

pub fn parse_chord(keys: &[String]) -> Result<Chord, String> {
    if keys.len() != 2 {
        return Err("detach_chord must be two keys".into());
    }
    Ok(Chord {
        first: parse_key(&keys[0])?,
        second: parse_key(&keys[1])?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_ctrl_backslash_then_ctrl_b() {
        let chord = Chord::default();
        assert_eq!(chord.first, 0x1c);
        assert_eq!(chord.second, 0x02);
        assert_eq!(parse_key("C-\\").unwrap(), 0x1c);
        assert_eq!(parse_key("C-b").unwrap(), 0x02);
    }

    #[test]
    fn chord_is_not_forwarded() {
        let mut matcher = Matcher::new(Chord::default());
        let mut out = Vec::new();
        assert_eq!(matcher.feed(b"hi", &mut out), Feed::Forward);
        assert_eq!(out, b"hi");
        out.clear();
        assert_eq!(matcher.feed(&[0x1c, 0x02], &mut out), Feed::Detach);
        assert!(out.is_empty());
    }

    #[test]
    fn first_byte_alone_is_held_then_flushed() {
        let mut matcher = Matcher::new(Chord::default());
        let mut out = Vec::new();
        assert_eq!(matcher.feed(&[0x1c], &mut out), Feed::Forward);
        assert!(out.is_empty());
        matcher.flush_pending(&mut out);
        assert_eq!(out, [0x1c]);
    }

    #[test]
    fn false_start_forwards_both_bytes() {
        let mut matcher = Matcher::new(Chord::default());
        let mut out = Vec::new();
        assert_eq!(matcher.feed(&[0x1c, b'x'], &mut out), Feed::Forward);
        assert_eq!(out, [0x1c, b'x']);
    }
}
