use tokio::process::Command;

pub async fn kill(pid: u32, signal: i32) -> Result<(), std::io::Error> {
    Command::new("kill")
        .arg("-s")
        .arg(signal.to_string())
        .arg(pid.to_string())
        .status()
        .await?;

    Ok(())
}
