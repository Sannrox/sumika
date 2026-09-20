use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size as terminal_size};
use sumika_ctl::{Client, ClientError};
use sumika_protocol::{
    EXIT_OK, EXIT_STOLEN, EXIT_USAGE, Request, Response, Status, default_socket_path,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::signal::unix::{SignalKind, signal};

#[derive(Parser)]
#[command(
    name = "sumika",
    about = "Session habitat for CLI agents",
    version,
    arg_required_else_help = true
)]
struct Cli {
    #[arg(long, env = "SUMIKA_SOCK")]
    sock: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the PTY supervisor (launchd / systemd ExecStart).
    Daemon,
    Ping,
    Start {
        name: String,
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        argv: Vec<String>,
    },
    List {
        #[arg(long)]
        json: bool,
    },
    Attach {
        name: String,
    },
    Kill {
        name: String,
        #[arg(long)]
        force: bool,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let sock = cli.sock.unwrap_or_else(default_socket_path);
    match cli.command {
        Command::Daemon => match run_daemon(sock).await {
            Ok(()) => ExitCode::from(EXIT_OK as u8),
            Err(err) => {
                eprintln!("{err:#}");
                ExitCode::from(EXIT_USAGE as u8)
            }
        },
        other => {
            let client = Client::new(sock);
            ExitCode::from(run_client(client, other).await as u8)
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

async fn run_client(client: Client, command: Command) -> i32 {
    match command {
        Command::Daemon => unreachable!(),
        Command::Ping => print_rpc(&client, &Request::Ping, false).await,
        Command::Start { name, cwd, argv } => {
            let cwd = match resolve_cwd(cwd) {
                Ok(path) => Some(path),
                Err(err) => {
                    eprintln!("{err}");
                    return EXIT_USAGE;
                }
            };
            print_rpc(&client, &Request::Start { name, argv, cwd }, false).await
        }
        Command::List { json } => print_rpc(&client, &Request::List, json).await,
        Command::Attach { name } => attach(client, name).await,
        Command::Kill { name, force } => {
            print_rpc(&client, &Request::Kill { name, force }, false).await
        }
    }
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

async fn attach(client: Client, name: String) -> i32 {
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
        let _ = resize(&client, &name, cols, rows).await;
    }
    proxy_tty(&client, &name, stream, raw).await
}

async fn proxy_tty(client: &Client, name: &str, stream: tokio::net::UnixStream, raw: bool) -> i32 {
    let mut winch = if raw {
        signal(SignalKind::window_change()).ok()
    } else {
        None
    };
    let mut term = signal(SignalKind::terminate()).ok();
    let mut stdin_rx = spawn_stdin_thread();
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
        while let Some(bytes) = stdin_rx.recv().await {
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
            } => return EXIT_OK,
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

fn spawn_stdin_thread() -> tokio::sync::mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    let _ = std::thread::Builder::new()
        .name("sumika-stdin".into())
        .spawn(move || {
            let mut stdin = io::stdin();
            let mut buf = [0u8; 4096];
            loop {
                match stdin.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.blocking_send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    rx
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
