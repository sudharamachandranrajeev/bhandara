use std::io;
use std::process::Command;

/// Safely eject the removable volume mounted at `mount`.
pub fn eject(mount: &str) -> io::Result<()> {
    let status = command(mount)?.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("the device is still in use"))
    }
}

#[cfg(target_os = "linux")]
fn command(mount: &str) -> io::Result<Command> {
    let mut command = Command::new("gio");
    command.args(["mount", "--eject", mount]);
    Ok(command)
}

#[cfg(windows)]
fn command(mount: &str) -> io::Result<Command> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    // Explorer's "Eject" verb: the same safe-removal path as the shell menu.
    // Only a drive letter is accepted, so nothing user-supplied reaches the script.
    let drive = mount.trim_end_matches('\\');
    let valid = drive.len() == 2 && drive.as_bytes()[0].is_ascii_alphabetic() && drive.as_bytes()[1] == b':';
    if !valid {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a drive letter"));
    }
    let script = format!(
        "$d = (New-Object -ComObject Shell.Application).Namespace(17).ParseName('{drive}'); if (-not $d) {{ exit 1 }}; $d.InvokeVerb('Eject')"
    );
    let mut command = Command::new("powershell");
    command.args(["-NoProfile", "-NonInteractive", "-Command", &script]).creation_flags(CREATE_NO_WINDOW);
    Ok(command)
}

#[cfg(not(any(target_os = "linux", windows)))]
fn command(_mount: &str) -> io::Result<Command> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "eject is not supported on this platform"))
}
