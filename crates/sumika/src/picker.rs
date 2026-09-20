use std::collections::HashMap;

use sumika_protocol::{SessionInfo, Status};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Down,
    Up,
    Attach,
    Restart,
    Quit,
    Jump(char),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Attach(String),
    Restart(String),
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker {
    rows: Vec<SessionInfo>,
    selected: usize,
    jumps: HashMap<char, String>,
}

impl Picker {
    pub fn new(rows: Vec<SessionInfo>, jumps: HashMap<char, String>) -> Self {
        Self {
            rows,
            selected: 0,
            jumps,
        }
    }

    pub fn rows(&self) -> &[SessionInfo] {
        &self.rows
    }

    pub fn selected(&self) -> Option<&SessionInfo> {
        self.rows.get(self.selected)
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn has_jump(&self, key: char) -> bool {
        self.jumps.contains_key(&key)
    }

    pub fn replace_rows(&mut self, rows: Vec<SessionInfo>) {
        let name = self.selected().map(|session| session.name.clone());
        self.rows = rows;
        self.selected = name
            .and_then(|name| self.rows.iter().position(|session| session.name == name))
            .unwrap_or(0);
    }

    pub fn handle(&mut self, input: Input) -> Action {
        match input {
            Input::Quit => Action::Quit,
            Input::Down => {
                self.move_sel(1);
                Action::None
            }
            Input::Up => {
                self.move_sel(-1);
                Action::None
            }
            Input::Attach => self
                .selected()
                .map(|session| Action::Attach(session.name.clone()))
                .unwrap_or(Action::None),
            Input::Restart => match self.selected() {
                Some(session) if session.status == Status::Dead => {
                    Action::Restart(session.name.clone())
                }
                _ => Action::None,
            },
            Input::Jump(key) => {
                let Some(name) = self.jumps.get(&key).cloned() else {
                    return Action::None;
                };
                if let Some(index) = self.rows.iter().position(|session| session.name == name) {
                    self.selected = index;
                    Action::Attach(name)
                } else {
                    Action::None
                }
            }
        }
    }

    fn move_sel(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let n = self.rows.len() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(n) as usize;
    }
}

pub fn glyph(status: Status) -> &'static str {
    match status {
        Status::Blocked => "!",
        Status::Idle => "·",
        Status::Running => "…",
        Status::Unknown => "?",
        Status::Dead => "✗",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(name: &str, status: Status) -> SessionInfo {
        SessionInfo {
            name: name.into(),
            argv: vec!["cat".into()],
            cwd: "/tmp".into(),
            status,
            pid: None,
            focused: false,
        }
    }

    #[test]
    fn glyphs_come_from_status_not_pty() {
        assert_eq!(glyph(Status::Blocked), "!");
        assert_eq!(glyph(Status::Idle), "·");
        assert_eq!(glyph(Status::Running), "…");
        assert_eq!(glyph(Status::Unknown), "?");
        assert_eq!(glyph(Status::Dead), "✗");
    }

    #[test]
    fn enter_attaches_the_selected_row() {
        let mut picker = Picker::new(
            vec![
                session("claude", Status::Running),
                session("kiro", Status::Running),
            ],
            HashMap::new(),
        );
        assert_eq!(picker.handle(Input::Down), Action::None);
        assert_eq!(picker.handle(Input::Attach), Action::Attach("kiro".into()));
    }

    #[test]
    fn q_quits_without_an_attach() {
        let mut picker = Picker::new(vec![session("claude", Status::Running)], HashMap::new());
        assert_eq!(picker.handle(Input::Quit), Action::Quit);
    }

    #[test]
    fn r_restarts_only_a_dead_row() {
        let mut picker = Picker::new(
            vec![
                session("claude", Status::Running),
                session("kiro", Status::Dead),
            ],
            HashMap::new(),
        );
        assert_eq!(picker.handle(Input::Restart), Action::None);
        picker.handle(Input::Down);
        assert_eq!(
            picker.handle(Input::Restart),
            Action::Restart("kiro".into())
        );
    }

    #[test]
    fn refresh_can_mark_selected_row_dead() {
        let mut picker = Picker::new(vec![session("kiro", Status::Running)], HashMap::new());
        picker.replace_rows(vec![session("kiro", Status::Dead)]);
        assert_eq!(
            picker.handle(Input::Restart),
            Action::Restart("kiro".into())
        );
    }

    #[test]
    fn configured_key_jumps_and_attaches() {
        let mut jumps = HashMap::new();
        jumps.insert('k', "kiro".into());
        let mut picker = Picker::new(
            vec![
                session("claude", Status::Running),
                session("kiro", Status::Running),
            ],
            jumps,
        );
        assert_eq!(
            picker.handle(Input::Jump('k')),
            Action::Attach("kiro".into())
        );
        assert_eq!(picker.selected().unwrap().name, "kiro");
    }
}
