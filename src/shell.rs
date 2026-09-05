//! Hand-offs to the outside: a URL for the browser, text for the clipboard, and the shell lines
//! of custom commands. All go through the platform's own commands; no extra crate.

use std::io::Write;
use std::process::{Command, Stdio};

use crate::error::AppError;

/// The platform shell running one line: `cmd /C` on Windows, `sh -c` elsewhere.
fn sh(line: &str) -> Command {
    if cfg!(target_os = "windows") {
        let mut command = Command::new("cmd");
        command.args(["/C", line]);
        command
    } else {
        let mut command = Command::new("sh");
        command.args(["-c", line]);
        command
    }
}

/// Runs `line` to completion and returns stdout and stderr together. A non-zero exit ends the
/// text rather than failing: the output is what the person asked to see.
pub fn capture(line: &str) -> Result<String, AppError> {
    let output = sh(line)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| shell_error("run the command", &error.to_string()))?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        text.push_str("\n[");
        text.push_str(&output.status.to_string());
        text.push(']');
    }
    Ok(text)
}

/// Writes `text` to one scratch file and returns the shell line that pages it: `$PAGER`, else
/// `more` on Windows and `less` elsewhere. The file is reused, never cleaned up: one small file
/// in the temp dir beats a race with the pager over when it may go.
pub fn page_line(text: &str) -> Result<String, AppError> {
    let path = std::env::temp_dir().join("lazydatabricks-page.txt");
    std::fs::write(&path, text)
        .map_err(|error| shell_error("write the page file", &error.to_string()))?;
    let pager = std::env::var("PAGER")
        .ok()
        .filter(|pager| !pager.is_empty())
        .unwrap_or_else(|| {
            if cfg!(target_os = "windows") {
                "more".to_owned()
            } else {
                "less".to_owned()
            }
        });
    Ok(format!("{pager} \"{}\"", path.display()))
}

/// The shell line that opens `path` in the person's editor: `$VISUAL`, else `$EDITOR`, else
/// `notepad` on Windows and `vi` elsewhere.
#[must_use]
pub fn editor_line(path: &std::path::Path) -> String {
    let editor = ["VISUAL", "EDITOR"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|editor| !editor.is_empty())
        .unwrap_or_else(|| {
            if cfg!(target_os = "windows") {
                "notepad".to_owned()
            } else {
                "vi".to_owned()
            }
        });
    format!("{editor} \"{}\"", path.display())
}

/// Runs `line` with the terminal: stdin, stdout and stderr inherited. The caller has already
/// stepped out of the alternate screen. Returns the exit status as words.
pub fn interactive(line: &str) -> Result<String, AppError> {
    let status = sh(line)
        .status()
        .map_err(|error| shell_error("run the command", &error.to_string()))?;
    Ok(status.to_string())
}

/// Hands `url` to `open_command` from config when set, else `$BROWSER`, else the platform's
/// opener. The URL is appended as one single-quoted argument; a URL never contains a quote.
pub fn open_url(url: &str, open_command: Option<&str>) -> Result<(), AppError> {
    let mut command = open_command.map_or_else(
        || default_opener(url),
        |line| sh(&format!("{line} '{url}'")),
    );
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

/// Pipes `text` into `copy_command` from config when set, else the platform's clipboard tool.
pub fn copy(text: &str, copy_command: Option<&str>) -> Result<(), AppError> {
    let mut command = copy_command.map_or_else(default_copier, sh);
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

/// `$BROWSER`, else the platform opener, with `url` as its argument.
fn default_opener(url: &str) -> Command {
    let browser = std::env::var("BROWSER").unwrap_or_default();
    if !browser.is_empty() {
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
    }
}

/// The platform's clipboard tool, reading stdin.
fn default_copier() -> Command {
    if cfg!(target_os = "windows") {
        Command::new("clip")
    } else if cfg!(target_os = "macos") {
        Command::new("pbcopy")
    } else if std::env::var_os("WAYLAND_DISPLAY").is_some_and(|display| !display.is_empty()) {
        Command::new("wl-copy")
    } else {
        let mut command = Command::new("xclip");
        command.args(["-selection", "clipboard"]);
        command
    }
}

fn shell_error(what: &str, detail: &str) -> AppError {
    AppError::Shell {
        what: what.to_owned(),
        detail: detail.to_owned(),
    }
}
