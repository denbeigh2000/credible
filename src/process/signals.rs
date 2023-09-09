// use nix::sys::signal::Signal::SIGEMT;
#[cfg(target_os = "macos")]
use nix::libc::SIGEMT;
use signal_hook::consts::*;
use tokio::process::Command;

#[cfg(not(target_os = "macos"))]
const NUM_SIGNALS: usize = 24;

#[cfg(target_os = "macos")]
const NUM_SIGNALS: usize = 26;

// All signals we can legally handle
pub const SIGNALS: [i32; NUM_SIGNALS] = [
    SIGHUP,
    SIGINT,
    SIGQUIT,
    SIGTRAP,
    SIGABRT,
    #[cfg(target_os = "macos")]
    SIGEMT,
    SIGBUS,
    SIGSYS,
    SIGPIPE,
    SIGALRM,
    SIGTERM,
    SIGURG,
    SIGTSTP,
    SIGCONT,
    SIGCHLD,
    SIGTTIN,
    SIGTTOU,
    SIGIO,
    SIGXCPU,
    SIGXFSZ,
    SIGVTALRM,
    SIGPROF,
    SIGWINCH,
    #[cfg(target_os = "macos")]
    SIGINFO,
    SIGUSR1,
    SIGUSR2,
];

pub async fn kill(pid: u32, signal: i32) -> Result<(), std::io::Error> {
    Command::new("kill")
        .arg(signal.to_string())
        .arg(pid.to_string())
        .status()
        .await?;

    Ok(())
}
