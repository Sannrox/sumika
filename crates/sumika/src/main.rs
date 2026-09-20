use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    size as terminal_size,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, List, ListItem, ListState};
use sumika::chord::{Chord, Feed, Matcher};
use sumika::config::{SessionSpec, load, resolve_config_path};
use sumika::picker::{Action, Input, Picker, glyph};
use sumika_ctl::{Client, ClientError};
use sumika_protocol::{
    EXIT_OK, EXIT_STOLEN, EXIT_UNREACHABLE, EXIT_USAGE, Request, Response, Status,
    default_socket_path,
};

const EXIT_TERMINATED: i32 = 143;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::signal::unix::{SignalKind, signal};

#[derive(Parser)]
#[command(name = "sumika", about = "Session habitat for CLI agents", version)]
struct Cli {
    #[arg(long, env = "SUMIKA_SOCK")]
    sock: Option<PathBuf>,
    #[arg(long, env = "SUMIKA_CONFIG")]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the PTY supervisor (launchd / systemd ExecStart).
    Daemon,
    Ping,
    Start {
        #[arg(required_unless_present = "all", conflicts_with = "all")]
        name: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        argv: Vec<String>,
    },
    List {
        #[arg(long)]
        json: bool,
    },
    Attach {
        name: Option<String>,
    },
    Kill {
        name: String,
        #[arg(long)]
        force: bool,
    },
    Doctor {
        #[arg(long)]
        json: bool,
    },
    Report {
        name: String,
        status: ReportStatus,
    },
    /// Map a vendor hook payload on stdin to `report`. Always exits 0.
    HookReport,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum ReportStatus {
    Idle,
    Blocked,
    Running,
}

impl From<ReportStatus> for Status {
    fn from(status: ReportStatus) -> Self {
        match status {
            ReportStatus::Idle => Status::Idle,
            ReportStatus::Blocked => Status::Blocked,
            ReportStatus::Running => Status::Running,
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let explicit_sock = cli.sock.is_some();
    let sock = cli.sock.unwrap_or_else(default_socket_path);
    match cli.command {
        Some(Command::Daemon) => match run_daemon(sock).await {
            Ok(()) => ExitCode::from(EXIT_OK as u8),
            Err(err) => {
                eprintln!("{err:#}");
                ExitCode::from(EXIT_USAGE as u8)
            }
        },
        Some(Command::Doctor { json }) => {
            let client = Client::new(sock);
            ExitCode::from(run_doctor(client, cli.config, json).await as u8)
        }
        Some(other) => {
            let client = Client::new(sock);
            if !explicit_sock && std::env::var_os("SUMIKA_SOCK").is_none() {
                ensure_daemon(&client).await;
            }
            ExitCode::from(run_client(client, cli.config, other).await as u8)
        }
        None => {
            let client = Client::new(sock);
            if !explicit_sock && std::env::var_os("SUMIKA_SOCK").is_none() {
                ensure_daemon(&client).await;
            }
            ExitCode::from(run_picker(client, cli.config).await as u8)
        }
    }
}

async fn run_daemon(sock: PathBuf) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(io::stderr)
        .init();
    sumika_daemon::run(sock).await
}

async fn run_client(client: Client, config: Option<PathBuf>, command: Command) -> i32 {
    match command {
        Command::Daemon => unreachable!(),
        Command::Ping => print_rpc(&client, &Request::Ping, false).await,
        Command::Start {
            name,
            all,
            cwd,
            argv,
        } => start_command(&client, config, name, all, cwd, argv).await,
        Command::List { json } => print_rpc(&client, &Request::List, json).await,
        Command::Attach { name } => {
            let name = match name {
                Some(name) => name,
                None => match sumika::last::recall() {
                    Ok(Some(name)) => name,
                    Ok(None) => {
                        eprintln!("no last-attached session; pass a name or open the picker");
                        return EXIT_USAGE;
                    }
                    Err(err) => {
                        eprintln!("{err}");
                        return EXIT_USAGE;
                    }
                },
            };
            attach(&client, name, load_chord(config.as_deref())).await
        }
        Command::Kill { name, force } => {
            print_rpc(&client, &Request::Kill { name, force }, false).await
        }
        Command::Doctor { .. } => unreachable!(),
        Command::Report { name, status } => {
            print_rpc(
                &client,
                &Request::Report {
                    name,
                    status: status.into(),
                },
                false,
            )
            .await
        }
        Command::HookReport => hook_report(&client).await,
    }
}

async fn hook_report(client: &Client) -> i32 {
    let mut body = String::new();
    let _ = io::stdin().read_to_string(&mut body);
    let Ok(payload) = serde_json::from_str::<serde_json::Value>(body.trim()) else {
        return EXIT_OK;
    };
    let Some(status) = sumika::hook_report::status_from_payload(&payload) else {
        return EXIT_OK;
    };
    let Some(name) = std::env::var("SUMIKA_SESSION")
        .ok()
        .filter(|name| !name.is_empty())
    else {
        return EXIT_OK;
    };
    let _ = client.rpc(&Request::Report { name, status }).await;
    EXIT_OK
}

async fn ensure_daemon(client: &Client) {
    if client.rpc(&Request::Ping).await.is_ok() {
        return;
    }
    let Ok(bin) = std::env::current_exe() else {
        return;
    };
    let Ok(plist) = sumika::service::install_launchd(&bin) else {
        return;
    };
    let _ = sumika::service::bootstrap_launchd(&plist);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        if client.rpc(&Request::Ping).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn run_doctor(client: Client, config: Option<PathBuf>, json: bool) -> i32 {
    let reachable = client.rpc(&Request::Ping).await.is_ok();
    let sessions = if reachable {
        client
            .rpc(&Request::List)
            .await
            .ok()
            .and_then(|response| response.sessions)
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let report = sumika::service::DoctorReport::from_parts(
        client.sock_path().to_path_buf(),
        resolve_config_path(config),
        reachable,
        sessions,
    );
    if json {
        match serde_json::to_string_pretty(&report) {
            Ok(body) => println!("{body}"),
            Err(err) => {
                eprintln!("{err}");
                return EXIT_USAGE;
            }
        }
    } else {
        println!("socket\t{}", report.socket);
        println!("reachable\t{}", report.reachable);
        match report.launchd_loaded {
            Some(loaded) => println!("launchd\t{loaded}"),
            None => println!("launchd\t-"),
        }
        println!("config\t{}", report.config);
        for session in &report.sessions {
            println!(
                "session\t{}\t{}\t{}",
                session.name,
                session.status,
                session
                    .pid
                    .map(|pid| pid.to_string())
                    .unwrap_or_else(|| "-".into())
            );
        }
    }
    if report.reachable {
        EXIT_OK
    } else {
        EXIT_UNREACHABLE
    }
}

async fn run_picker(client: Client, config: Option<PathBuf>) -> i32 {
    if !io::stdin().is_terminal() {
        eprintln!("picker requires a terminal");
        return EXIT_USAGE;
    }
    let config_path = resolve_config_path(config);
    let jumps = load(&config_path)
        .map(|config| config.jump_keys())
        .unwrap_or_default();
    let rows = match list_sessions(&client).await {
        Ok(rows) => rows,
        Err(err) => {
            eprintln!("{err}");
            return EXIT_UNREACHABLE;
        }
    };
    let mut picker = Picker::new(rows, jumps);
    loop {
        let action = match run_picker_screen(&client, &mut picker).await {
            Ok(action) => action,
            Err(err) => {
                eprintln!("{err}");
                return EXIT_USAGE;
            }
        };
        match action {
            Action::Quit => return EXIT_OK,
            Action::None => {}
            Action::Attach(name) => {
                let code = attach(&client, name, load_chord(Some(config_path.as_path()))).await;
                if code == EXIT_UNREACHABLE || code == EXIT_TERMINATED {
                    return code;
                }
            }
            Action::Restart(name) => {
                if let Err(err) = restart_session(&client, &config_path, &name).await {
                    eprintln!("{err}");
                }
            }
        }
    }
}

struct PickerScreen;

impl PickerScreen {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(err) = execute!(io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(err);
        }
        Ok(Self)
    }
}

impl Drop for PickerScreen {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
        let _ = io::stdout().flush();
    }
}

async fn run_picker_screen(client: &Client, picker: &mut Picker) -> io::Result<Action> {
    let _screen = PickerScreen::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut term = signal(SignalKind::terminate()).ok();
    loop {
        match tokio::time::timeout(Duration::from_secs(2), list_sessions(client)).await {
            Ok(Ok(rows)) => picker.replace_rows(rows),
            Ok(Err(err)) => return Err(io::Error::other(err)),
            Err(_) => {}
        }
        terminal.draw(|frame| {
            let items: Vec<ListItem> = picker
                .rows()
                .iter()
                .map(|session| ListItem::new(format!("{} {}", glyph(session.status), session.name)))
                .collect();
            let mut state = ListState::default()
                .with_selected((!picker.rows().is_empty()).then_some(picker.selected_index()));
            frame.render_stateful_widget(
                List::new(items)
                    .block(Block::bordered().title("sumika"))
                    .highlight_symbol("> ")
                    .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
                frame.area(),
                &mut state,
            );
        })?;
        tokio::select! {
            _ = async {
                match term.as_mut() {
                    Some(signal) => { signal.recv().await; }
                    None => std::future::pending::<()>().await,
                }
            } => return Ok(Action::Quit),
            polled = tokio::task::spawn_blocking(|| event::poll(Duration::from_millis(400))) => {
                if !polled.map_err(io::Error::other)?? {
                    continue;
                }
            }
        }
        while let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                if !event::poll(Duration::ZERO)? {
                    break;
                }
                continue;
            }
            let input = match key.code {
                KeyCode::Enter => Input::Attach,
                KeyCode::Esc | KeyCode::Char('q') => Input::Quit,
                KeyCode::Char('r') => Input::Restart,
                KeyCode::Down => Input::Down,
                KeyCode::Up => Input::Up,
                KeyCode::Char(c) if picker.has_jump(c) => Input::Jump(c),
                KeyCode::Char('j') => Input::Down,
                KeyCode::Char('k') => Input::Up,
                _ => {
                    if !event::poll(Duration::ZERO)? {
                        break;
                    }
                    continue;
                }
            };
            let action = picker.handle(input);
            if action != Action::None {
                while event::poll(Duration::ZERO)? {
                    let _ = event::read()?;
                }
                return Ok(action);
            }
            if !event::poll(Duration::ZERO)? {
                break;
            }
        }
    }
}

async fn list_sessions(client: &Client) -> Result<Vec<sumika_protocol::SessionInfo>, String> {
    let response = client
        .rpc(&Request::List)
        .await
        .map_err(|err| err.to_string())?;
    if !response.ok {
        return Err(response
            .error
            .map(|err| err.message)
            .unwrap_or_else(|| "list failed".into()));
    }
    Ok(response.sessions.unwrap_or_default())
}

async fn restart_session(
    client: &Client,
    config_path: &std::path::Path,
    name: &str,
) -> Result<(), String> {
    let spec = resolve_start(config_path, name, None, Vec::new())?;
    let response = client
        .rpc(&Request::Start {
            name: name.to_string(),
            argv: spec.argv,
            cwd: spec.cwd,
        })
        .await
        .map_err(|err| err.to_string())?;
    if !response.ok {
        return Err(response
            .error
            .map(|err| err.message)
            .unwrap_or_else(|| "start failed".into()));
    }
    Ok(())
}

async fn start_command(
    client: &Client,
    config: Option<PathBuf>,
    name: Option<String>,
    all: bool,
    cwd: Option<PathBuf>,
    argv: Vec<String>,
) -> i32 {
    let config_path = resolve_config_path(config);
    if all {
        if !argv.is_empty() {
            eprintln!("start --all does not take argv");
            return EXIT_USAGE;
        }
        return start_all(client, &config_path, cwd).await;
    }
    let name = match name {
        Some(name) => name,
        None => {
            eprintln!("session name required");
            return EXIT_USAGE;
        }
    };
    let spec = match resolve_start(&config_path, &name, cwd, argv) {
        Ok(spec) => spec,
        Err(err) => {
            eprintln!("{err}");
            return EXIT_USAGE;
        }
    };
    print_rpc(
        client,
        &Request::Start {
            name,
            argv: spec.argv,
            cwd: spec.cwd,
        },
        false,
    )
    .await
}

async fn start_all(client: &Client, config_path: &std::path::Path, cwd: Option<PathBuf>) -> i32 {
    let config = match load(config_path) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("{err}");
            return EXIT_USAGE;
        }
    };
    let mut code = EXIT_OK;
    for session in config.sessions {
        let spec = match apply_start(Some(&session), cwd.clone(), Vec::new()) {
            Ok(spec) => spec,
            Err(err) => {
                eprintln!("{err}");
                return EXIT_USAGE;
            }
        };
        let exit = print_rpc(
            client,
            &Request::Start {
                name: session.name,
                argv: spec.argv,
                cwd: spec.cwd,
            },
            false,
        )
        .await;
        if exit != EXIT_OK && code == EXIT_OK {
            code = exit;
        }
    }
    code
}

struct ResolvedStart {
    argv: Vec<String>,
    cwd: Option<String>,
}

fn resolve_start(
    config_path: &std::path::Path,
    name: &str,
    cwd: Option<PathBuf>,
    argv: Vec<String>,
) -> Result<ResolvedStart, String> {
    let needs_config = argv.is_empty() || cwd.is_none();
    let loaded = if needs_config {
        match load(config_path) {
            Ok(config) => Some(config),
            Err(err) if argv.is_empty() => return Err(err.to_string()),
            Err(_) => None,
        }
    } else {
        None
    };
    let spec = loaded.as_ref().and_then(|config| config.lookup(name));
    if argv.is_empty() && spec.is_none() {
        return Err(format!("no configured session named {name}"));
    }
    apply_start(spec, cwd, argv)
}

fn apply_start(
    spec: Option<&SessionSpec>,
    cwd: Option<PathBuf>,
    argv: Vec<String>,
) -> Result<ResolvedStart, String> {
    let argv = if argv.is_empty() {
        spec.map(|session| session.argv.clone())
            .filter(|argv| !argv.is_empty())
            .ok_or_else(|| "argv is empty".to_string())?
    } else {
        argv
    };
    let cwd = match cwd {
        Some(path) => Some(resolve_cwd(Some(path)).map_err(|err| err.to_string())?),
        None => match spec.and_then(|session| session.cwd.as_deref()) {
            Some(path) => {
                Some(resolve_cwd(Some(PathBuf::from(path))).map_err(|err| err.to_string())?)
            }
            None => Some(resolve_cwd(None).map_err(|err| err.to_string())?),
        },
    };
    Ok(ResolvedStart { argv, cwd })
}

async fn print_rpc(client: &Client, request: &Request, json: bool) -> i32 {
    match client.rpc(request).await {
        Ok(response) => {
            if json {
                match serde_json::to_string_pretty(&response) {
                    Ok(body) => println!("{body}"),
                    Err(err) => {
                        eprintln!("{err}");
                        return EXIT_USAGE;
                    }
                }
            } else if let Some(error) = &response.error {
                eprintln!("{}", error.message);
            } else if let Some(sessions) = &response.sessions {
                for session in sessions {
                    let pid = session
                        .pid
                        .map(|pid| pid.to_string())
                        .unwrap_or_else(|| "-".into());
                    let status = match session.status {
                        sumika_protocol::Status::Running => "running",
                        sumika_protocol::Status::Idle => "idle",
                        sumika_protocol::Status::Blocked => "blocked",
                        sumika_protocol::Status::Dead => "dead",
                        sumika_protocol::Status::Unknown => "unknown",
                    };
                    println!(
                        "{}\t{}\t{}\t{}",
                        session.name,
                        status,
                        pid,
                        session.argv.join(" ")
                    );
                }
            } else if let Some(session) = &response.session {
                println!(
                    "{}\t{}",
                    session.name,
                    session.pid.map(|pid| pid.to_string()).unwrap_or_default()
                );
            }
            response.exit_code()
        }
        Err(err) => {
            eprintln!("{err}");
            err.exit_code()
        }
    }
}

struct RawGuard;

impl RawGuard {
    fn enable() -> io::Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = io::stdout().flush();
    }
}

fn load_chord(config: Option<&std::path::Path>) -> Chord {
    let path = resolve_config_path(config.map(PathBuf::from));
    match load(&path) {
        Ok(config) => config.detach_chord().unwrap_or_default(),
        Err(_) => Chord::default(),
    }
}

async fn attach(client: &Client, name: String, chord: Chord) -> i32 {
    let (response, stream) = match client.attach(&name).await {
        Ok(value) => value,
        Err(err) => {
            eprintln!("{err}");
            return err.exit_code();
        }
    };
    if !response.ok {
        if let Some(error) = &response.error {
            eprintln!("{}", error.message);
        }
        return response.exit_code();
    }
    let _ = sumika::last::remember(&name);
    let raw = io::stdin().is_terminal();
    let _guard = if raw {
        match RawGuard::enable() {
            Ok(guard) => Some(guard),
            Err(err) => {
                eprintln!("raw mode: {err}");
                return EXIT_USAGE;
            }
        }
    } else {
        None
    };
    if raw && let Ok((cols, rows)) = terminal_size() {
        let _ = resize(client, &name, cols, rows).await;
    }
    proxy_tty(client, &name, stream, raw, chord).await
}

async fn proxy_tty(
    client: &Client,
    name: &str,
    stream: tokio::net::UnixStream,
    raw: bool,
    chord: Chord,
) -> i32 {
    let mut winch = if raw {
        signal(SignalKind::window_change()).ok()
    } else {
        None
    };
    let mut term = signal(SignalKind::terminate()).ok();
    let mut stdin_pump = spawn_stdin_thread(chord);
    let (mut reader, mut writer) = stream.into_split();
    let mut stdout = tokio::io::stdout();
    let pump_out = async {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => return EXIT_STOLEN,
                Ok(n) => {
                    if stdout.write_all(&buf[..n]).await.is_err() {
                        return EXIT_STOLEN;
                    }
                    let _ = stdout.flush().await;
                }
                Err(_) => return EXIT_STOLEN,
            }
        }
    };
    let pump_in = async {
        while let Some(bytes) = stdin_pump.rx.recv().await {
            if writer.write_all(&bytes).await.is_err() {
                return EXIT_STOLEN;
            }
        }
        EXIT_OK
    };
    tokio::pin!(pump_out);
    tokio::pin!(pump_in);
    loop {
        tokio::select! {
            code = &mut pump_out => return map_attach_eof(client, name, code).await,
            code = &mut pump_in => return map_attach_eof(client, name, code).await,
            _ = async {
                match winch.as_mut() {
                    Some(signal) => { signal.recv().await; }
                    None => std::future::pending::<()>().await,
                }
            } => {
                if let Ok((cols, rows)) = terminal_size() {
                    let _ = resize(client, name, cols, rows).await;
                }
            }
            _ = async {
                match term.as_mut() {
                    Some(signal) => { signal.recv().await; }
                    None => std::future::pending::<()>().await,
                }
            } => return EXIT_TERMINATED,
        }
    }
}

async fn map_attach_eof(client: &Client, name: &str, code: i32) -> i32 {
    if code != EXIT_STOLEN {
        return code;
    }
    match client.rpc(&Request::List).await {
        Ok(response) => {
            let dead = response
                .sessions
                .as_ref()
                .and_then(|sessions| sessions.iter().find(|session| session.name == name))
                .is_none_or(|session| session.status == Status::Dead);
            if dead { EXIT_OK } else { EXIT_STOLEN }
        }
        Err(_) => EXIT_STOLEN,
    }
}

struct StdinPump {
    rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
    _cancel: File,
}

fn spawn_stdin_thread(chord: Chord) -> StdinPump {
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    let (cancel_r, cancel_w) = pipe_pair();
    let stdin_fd = unsafe { libc::dup(libc::STDIN_FILENO) };
    if stdin_fd < 0 {
        panic!("dup stdin: {}", io::Error::last_os_error());
    }
    let mut stdin = unsafe { File::from_raw_fd(stdin_fd) };
    let cancel_fd = cancel_r.as_raw_fd();
    let _ = std::thread::Builder::new()
        .name("sumika-stdin".into())
        .spawn(move || {
            let _cancel_r = cancel_r;
            let mut matcher = Matcher::new(chord);
            let mut buf = [0u8; 4096];
            loop {
                if !wait_stdin_or_cancel(stdin.as_raw_fd(), cancel_fd) {
                    break;
                }
                match stdin.read(&mut buf) {
                    Ok(0) => {
                        let mut out = Vec::new();
                        matcher.flush_pending(&mut out);
                        if !out.is_empty() {
                            let _ = tx.blocking_send(out);
                        }
                        break;
                    }
                    Ok(n) => {
                        let mut out = Vec::new();
                        match matcher.feed(&buf[..n], &mut out) {
                            Feed::Detach => {
                                if !out.is_empty() {
                                    let _ = tx.blocking_send(out);
                                }
                                break;
                            }
                            Feed::Forward if out.is_empty() => {}
                            Feed::Forward => {
                                if tx.blocking_send(out).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    StdinPump {
        rx,
        _cancel: cancel_w,
    }
}

fn pipe_pair() -> (File, File) {
    let mut fds = [0; 2];
    let rc = unsafe { libc::pipe(fds.as_mut_ptr()) };
    if rc != 0 {
        panic!("pipe: {}", io::Error::last_os_error());
    }
    let reader = unsafe { File::from_raw_fd(fds[0]) };
    let writer = unsafe { File::from_raw_fd(fds[1]) };
    (reader, writer)
}

fn wait_stdin_or_cancel(stdin_fd: i32, cancel_fd: i32) -> bool {
    let mut fds = [
        libc::pollfd {
            fd: stdin_fd,
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: cancel_fd,
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    loop {
        let rc = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
        if rc < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return false;
        }
        if rc == 0 {
            return false;
        }
        return fds[1].revents == 0;
    }
}

fn resolve_cwd(cwd: Option<PathBuf>) -> io::Result<String> {
    let path = match cwd {
        Some(path) if path.is_absolute() => path,
        Some(path) => std::env::current_dir()?.join(path),
        None => std::env::current_dir()?,
    };
    Ok(path.display().to_string())
}

async fn resize(
    client: &Client,
    name: &str,
    cols: u16,
    rows: u16,
) -> Result<Response, ClientError> {
    client
        .rpc(&Request::Resize {
            name: name.to_string(),
            cols,
            rows,
        })
        .await
}
