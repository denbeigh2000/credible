use signal_hook::consts::*;
use tokio::process::Command;

pub const SIGNALS: [i32; 9] = [
    SIGHUP, SIGINT, SIGQUIT, SIGABRT, SIGTERM, SIGTSTP, SIGCONT, SIGUSR1, SIGUSR2,
];

pub async fn kill(pid: u32, signal: i32) -> Result<(), std::io::Error> {
    Command::new("kill")
        .arg(signal.to_string())
        .arg(pid.to_string())
        .status()
        .await?;

    Ok(())
}
