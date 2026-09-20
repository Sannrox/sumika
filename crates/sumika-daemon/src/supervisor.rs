use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use sumika_protocol::{ErrorCode, Request, Response, SessionInfo, Status};
use tokio::sync::oneshot;

const DEFAULT_SIZE: PtySize = PtySize {
    rows: 24,
    cols: 80,
    pixel_width: 0,
    pixel_height: 0,
};

#[derive(Clone)]
pub struct Supervisor {
    sessions: Arc<Mutex<HashMap<String, Arc<Session>>>>,
}

pub struct AttachSlot {
    pub info: SessionInfo,
    pub output: Option<tokio::sync::mpsc::Receiver<Vec<u8>>>,
    pub cancel: oneshot::Receiver<()>,
    input: tokio::sync::mpsc::Sender<Vec<u8>>,
    session: Arc<Session>,
    generation: u64,
}

impl AttachSlot {
    pub fn input(&self) -> tokio::sync::mpsc::Sender<Vec<u8>> {
        self.input.clone()
    }
}

impl Drop for AttachSlot {
    fn drop(&mut self) {
        let mut attach = self.session.attach.lock().expect("attach");
        if attach.generation == self.generation {
            attach.cancel = None;
            *self.session.dest.lock().expect("dest") = None;
        }
    }
}

struct AttachState {
    generation: u64,
    cancel: Option<oneshot::Sender<()>>,
}

struct Live {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    input: tokio::sync::mpsc::Sender<Vec<u8>>,
}

struct Session {
    name: String,
    argv: Vec<String>,
    cwd: PathBuf,
    pid: AtomicU32,
    status: Mutex<Status>,
    attach: Mutex<AttachState>,
    dest: Arc<Mutex<Option<tokio::sync::mpsc::Sender<Vec<u8>>>>>,
    live: Mutex<Option<Live>>,
}

impl Supervisor {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn dispatch(&self, request: Request) -> Response {
        match request {
            Request::Ping => Response::ok(),
            Request::Start { name, argv, cwd } => self.start(name, argv, cwd),
            Request::List => self.list(),
            Request::Resize { name, cols, rows } => self.resize(&name, cols, rows),
            Request::Kill { name, force } => self.kill(&name, force),
            Request::Attach { .. } => Response::err(ErrorCode::InvalidRequest, "attach is not rpc"),
        }
    }

    pub fn attach(&self, name: &str) -> Result<AttachSlot, (ErrorCode, String)> {
        let session = self.get(name).ok_or_else(|| {
            (
                ErrorCode::UnknownSession,
                format!("no session named {name}"),
            )
        })?;
        session.reap();
        let input = {
            let live = session.live.lock().expect("live");
            let Some(live) = live.as_ref() else {
                return Err((ErrorCode::Dead, format!("session {name} is dead")));
            };
            live.input.clone()
        };
        let (output_tx, output_rx) = tokio::sync::mpsc::channel(64);
        let (generation, rx) = {
            let mut attach = session.attach.lock().expect("attach");
            if let Some(prev) = attach.cancel.take() {
                let _ = prev.send(());
            }
            attach.generation += 1;
            let (tx, rx) = oneshot::channel();
            attach.cancel = Some(tx);
            *session.dest.lock().expect("dest") = Some(output_tx);
            (attach.generation, rx)
        };
        Ok(AttachSlot {
            info: session.info(),
            output: Some(output_rx),
            cancel: rx,
            input,
            session,
            generation,
        })
    }

    fn start(&self, name: String, argv: Vec<String>, cwd: Option<String>) -> Response {
        if !valid_name(&name) {
            return Response::err(ErrorCode::InvalidRequest, "invalid session name");
        }
        if argv.is_empty() {
            return Response::err(ErrorCode::InvalidRequest, "argv is empty");
        }
        let cwd = match cwd {
            Some(path) => PathBuf::from(path),
            None => match std::env::current_dir() {
                Ok(path) => path,
                Err(err) => {
                    return Response::err(ErrorCode::SpawnFailed, format!("cwd: {err}"));
                }
            },
        };
        if !cwd.is_dir() {
            return Response::err(
                ErrorCode::SpawnFailed,
                format!("cwd is not a directory: {}", cwd.display()),
            );
        }
        {
            let sessions = self.sessions.lock().expect("sessions");
            if let Some(existing) = sessions.get(&name) {
                existing.reap();
                if *existing.status.lock().expect("status") != Status::Dead {
                    return Response::session(existing.info());
                }
            }
        }
        let session = match spawn_session(name.clone(), argv, cwd) {
            Ok(session) => session,
            Err(err) => {
                return Response::err(ErrorCode::SpawnFailed, err.to_string());
            }
        };
        let mut sessions = self.sessions.lock().expect("sessions");
        if let Some(existing) = sessions.get(&name) {
            existing.reap();
            if *existing.status.lock().expect("status") != Status::Dead {
                existing_kill_new(&session);
                return Response::session(existing.info());
            }
        }
        let info = session.info();
        sessions.insert(name, session);
        Response::session(info)
    }

    fn list(&self) -> Response {
        let sessions = self.sessions.lock().expect("sessions");
        let mut infos: Vec<SessionInfo> = sessions
            .values()
            .map(|session| {
                session.reap();
                session.info()
            })
            .collect();
        infos.sort_by(|a, b| a.name.cmp(&b.name));
        Response::sessions(infos)
    }

    fn resize(&self, name: &str, cols: u16, rows: u16) -> Response {
        let session = match self.get(name) {
            Some(session) => session,
            None => return unknown(name),
        };
        session.reap();
        let live = session.live.lock().expect("live");
        let Some(live) = live.as_ref() else {
            return Response::err(ErrorCode::Dead, format!("session {name} is dead"));
        };
        match live.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(()) => Response::ok(),
            Err(err) => Response::err(ErrorCode::InvalidRequest, err.to_string()),
        }
    }

    fn kill(&self, name: &str, force: bool) -> Response {
        let session = match self.get(name) {
            Some(session) => session,
            None => return unknown(name),
        };
        session.reap();
        if session.live.lock().expect("live").is_none() {
            return Response::session(session.info());
        }
        session.kill(force);
        session.reap();
        Response::session(session.info())
    }

    fn get(&self, name: &str) -> Option<Arc<Session>> {
        self.sessions.lock().expect("sessions").get(name).cloned()
    }
}

impl Session {
    fn info(&self) -> SessionInfo {
        let status = *self.status.lock().expect("status");
        let pid = self.pid.load(Ordering::SeqCst);
        SessionInfo {
            name: self.name.clone(),
            argv: self.argv.clone(),
            cwd: self.cwd.display().to_string(),
            status,
            pid: if status == Status::Dead || pid == 0 {
                None
            } else {
                Some(pid)
            },
            focused: self.attach.lock().expect("attach").cancel.is_some(),
        }
    }

    fn reap(&self) {
        let mut live = self.live.lock().expect("live");
        let Some(current) = live.as_mut() else {
            return;
        };
        if matches!(current.child.try_wait(), Ok(Some(_))) {
            *self.status.lock().expect("status") = Status::Dead;
            *live = None;
        }
    }

    fn kill(&self, force: bool) {
        let pid = self.pid.load(Ordering::SeqCst);
        if pid != 0 {
            kill_process_group(pid, force);
        }
        if let Some(live) = self.live.lock().expect("live").as_mut() {
            #[cfg(unix)]
            if let Some(pgid) = live.master.process_group_leader() {
                let pgid = pgid as u32;
                if pgid != 0 && pgid != pid {
                    kill_process_group(pgid, force);
                }
            }
        }
    }
}

fn existing_kill_new(session: &Session) {
    session.kill(true);
    session.reap();
}

fn spawn_session(name: String, argv: Vec<String>, cwd: PathBuf) -> anyhow::Result<Arc<Session>> {
    let pty = native_pty_system();
    let pair = pty.openpty(DEFAULT_SIZE)?;
    let mut cmd = CommandBuilder::new(&argv[0]);
    for arg in &argv[1..] {
        cmd.arg(arg);
    }
    cmd.cwd(&cwd);
    let child = pair.slave.spawn_command(cmd)?;
    let pid = child.process_id().unwrap_or(0);
    let reader = pair.master.try_clone_reader()?;
    let mut writer = pair.master.take_writer()?;
    let dest = Arc::new(Mutex::new(None));
    let (input, mut input_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let session = Arc::new(Session {
        name: name.clone(),
        argv,
        cwd,
        pid: AtomicU32::new(pid),
        status: Mutex::new(Status::Running),
        attach: Mutex::new(AttachState {
            generation: 0,
            cancel: None,
        }),
        dest: dest.clone(),
        live: Mutex::new(Some(Live {
            master: pair.master,
            child,
            input,
        })),
    });
    std::thread::Builder::new()
        .name(format!("sumika-pty-{name}"))
        .spawn(move || drain_pty(reader, dest))?;
    std::thread::Builder::new()
        .name(format!("sumika-in-{name}"))
        .spawn(move || {
            while let Some(bytes) = input_rx.blocking_recv() {
                if writer.write_all(&bytes).is_err() || writer.flush().is_err() {
                    break;
                }
            }
        })?;
    let waiter = session.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            waiter.reap();
            if waiter.live.lock().expect("live").is_none() {
                break;
            }
        }
    });
    tracing::info!(session = %name, pid, "started");
    Ok(session)
}

fn drain_pty(
    mut reader: Box<dyn Read + Send>,
    dest: Arc<Mutex<Option<tokio::sync::mpsc::Sender<Vec<u8>>>>>,
) {
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let tx = dest.lock().expect("dest").clone();
                if let Some(tx) = tx {
                    let _ = tx.blocking_send(buf[..n].to_vec());
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    *dest.lock().expect("dest") = None;
}

fn kill_process_group(pid: u32, force: bool) {
    let sig = if force { libc::SIGKILL } else { libc::SIGTERM };
    let pid = pid as libc::pid_t;
    // pid is the child we spawned; killpg fails when it is not a group leader.
    unsafe {
        if libc::killpg(pid, sig) != 0 {
            libc::kill(pid, sig);
        }
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn unknown(name: &str) -> Response {
    Response::err(
        ErrorCode::UnknownSession,
        format!("no session named {name}"),
    )
}
