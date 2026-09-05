//! The two things handed to the desktop: a URL for the browser, text for the clipboard. Both go
//! through the platform's own command so no extra crate is needed.

use std::io::Write;
use std::process::{Command, Stdio};

use crate::error::AppError;

pub fn open_url(url: &str) -> Result<(), AppError> {
    let browser = std::env::var("BROWSER").unwrap_or_default();
    let mut command = if !browser.is_empty() {
        let mut command = Command::new(browser);
        command.arg(url);
        command
    } else if cfg!(target_os = "windows") {
        let mut command = Command::new("cmd");
        // The empty string is the window title `start` insists on when the next arg is quoted.
        command.args(["/c", "start", "", url]);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        command.arg(url);
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };
    let status = command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| shell_error("open the browser", &error.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(shell_error("open the browser", &status.to_string()))
    }
}

pub fn copy(text: &str) -> Result<(), AppError> {
    let mut command = if cfg!(target_os = "windows") {
        Command::new("clip")
    } else if cfg!(target_os = "macos") {
        Command::new("pbcopy")
    } else if std::env::var_os("WAYLAND_DISPLAY").is_some_and(|display| !display.is_empty()) {
        Command::new("wl-copy")
    } else {
        let mut command = Command::new("xclip");
        command.args(["-selection", "clipboard"]);
        command
    };
    let failed =
        |error: &dyn std::fmt::Display| shell_error("copy to the clipboard", &error.to_string());
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| failed(&error))?;
    {
        let Some(mut stdin) = child.stdin.take() else {
            return Err(failed(&"no stdin on the clipboard command"));
        };
        stdin
            .write_all(text.as_bytes())
            .map_err(|error| failed(&error))?;
        // Dropping stdin here sends EOF, which is what makes the command finish.
    }
    let status = child.wait().map_err(|error| failed(&error))?;
    if status.success() {
        Ok(())
    } else {
        Err(failed(&status))
    }
}

fn shell_error(what: &str, detail: &str) -> AppError {
    AppError::Shell {
        what: what.to_owned(),
        detail: detail.to_owned(),
    }
}
