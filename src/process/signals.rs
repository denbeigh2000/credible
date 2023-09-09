// use nix::sys::signal::Signal::SIGEMT;
use nix::libc::SIGEMT;
use signal_hook::consts::*;
use tokio::process::Command;

// All signals we can legally handle
pub const SIGNALS: [i32; 26] = [
    SIGHUP, SIGINT, SIGQUIT, SIGTRAP, SIGABRT, SIGEMT, SIGBUS, SIGSYS, SIGPIPE, SIGALRM, SIGTERM,
    SIGURG, SIGTSTP, SIGCONT, SIGCHLD, SIGTTIN, SIGTTOU, SIGIO, SIGXCPU, SIGXFSZ, SIGVTALRM,
    SIGPROF, SIGWINCH, SIGINFO, SIGUSR1, SIGUSR2,
];

pub async fn kill(pid: u32, signal: i32) -> Result<(), std::io::Error> {
    Command::new("kill")
        .arg(signal.to_string())
        .arg(pid.to_string())
        .status()
        .await?;

    Ok(())
}
