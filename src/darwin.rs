use std::io;
use std::path::Path;
use std::process::Command;

// Adapted from agenix, may want to revisit/investigate alternatives?

pub fn mount_ramfs(dir: &Path) -> Result<(), io::Error> {
    // 512MB for secrets should be enough for everybody...right?
    let ram_device_name = format!("ram://{}", 2048 * 512);
    // TODO: I don't think this handles non-zero error codes?
    let device_bytes = Command::new("hdiutil")
        .arg("attach")
        .arg("-nomount")
        .arg(&ram_device_name)
        .output()?
        .stdout;

    let device_string = String::from_utf8(device_bytes)
        .expect("invalid utf-8 bytes from hdiutil")
        .split_whitespace()
        .next()
        .expect("no device from hdiutil")
        .to_owned();

    Command::new("newfs_hfs").arg("-v").arg("age-stor").arg(&device_string).output()?;
    Command::new("mount").arg("-t").arg("hfs").arg("-o").arg("nobrowse,nodev,nosuid,-m=0751").arg(&device_string).arg(dir).output()?;

    Ok(())
}
