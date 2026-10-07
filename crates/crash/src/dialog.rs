//! Telling the player. A release build has no console, so a startup failure
//! or crash that only reaches a log is invisible: these show a plain dialog
//! naming what happened and where the files are, with a button that opens
//! the logs folder.
use std::path::Path;

/// Tell the player something went wrong. With `logs`, offer to open that
/// folder; returns whether they did. Without a desktop (tests, servers,
/// `BRI_NO_DIALOGS`), the text goes to stderr instead.
pub fn alert(title: &str, text: &str, logs: Option<&Path>) -> bool {
    if std::env::var_os("BRI_NO_DIALOGS").is_some() {
        eprintln!("{title}: {text}");
        return false;
    }
    #[cfg(windows)]
    {
        imp::alert(title, text, logs)
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        // The terminal and the logs get it too: the dialog may not show.
        eprintln!("{title}: {text}");
        desktop::alert(title, text, logs)
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = logs;
        eprintln!("{title}: {text}");
        false
    }
}

/// Open a folder in the file browser or a web page in the browser. Returns
/// whether the desktop accepted it.
pub fn open(target: &str) -> bool {
    #[cfg(windows)]
    {
        imp::open(std::ffi::OsStr::new(target))
    }
    #[cfg(not(windows))]
    {
        // macOS has `open`; other desktops have xdg-open.
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        std::process::Command::new(opener)
            .arg(target)
            .spawn()
            .is_ok()
    }
}

/// Keep dialogs to a readable size; the log holds the rest.
pub fn summarize(error: &str) -> String {
    const LIMIT: usize = 700;
    if error.chars().count() <= LIMIT {
        return error.to_string();
    }
    let cut: String = error.chars().take(LIMIT).collect();
    format!("{cut}...")
}

#[cfg(windows)]
mod imp {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt, path::Path};
    use windows_sys::Win32::UI::{
        Shell::ShellExecuteW,
        WindowsAndMessaging::{
            IDYES, MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MB_YESNO, MessageBoxW, SW_SHOWNORMAL,
        },
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }

    pub(super) fn alert(title: &str, text: &str, logs: Option<&Path>) -> bool {
        let (body, buttons) = match logs {
            Some(dir) => (
                format!(
                    "{text}\n\nThe logs are in:\n{}\n\nOpen that folder now?",
                    dir.display()
                ),
                MB_YESNO,
            ),
            None => (text.to_string(), MB_OK),
        };
        let (title, body) = (wide(title), wide(&body));
        // SAFETY: NUL-terminated UTF-16 strings that outlive the call.
        let choice = unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                body.as_ptr(),
                title.as_ptr(),
                buttons | MB_ICONERROR | MB_SETFOREGROUND,
            )
        };
        if choice != IDYES {
            return false;
        }
        let Some(dir) = logs else { return false };
        open(dir.as_os_str())
    }

    pub(super) fn open(target: &OsStr) -> bool {
        let target: Vec<u16> = target.encode_wide().chain([0]).collect();
        let verb = wide("open");
        // SAFETY: NUL-terminated strings; the shell opens a folder in
        // Explorer and a web address in the default browser.
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        result as isize > 32
    }
}

/// Linux and macOS have no message box a Rust program can call without a
/// GUI toolkit, so the desktop's own dialog tool shows it: osascript on
/// macOS, zenity (GNOME and most others) or kdialog (KDE) on Linux. An app
/// or a game started from a file manager has no terminal to print to.
#[cfg(any(target_os = "linux", target_os = "macos", test))]
mod desktop {
    use std::path::Path;

    /// The macOS alert's button that opens the logs folder.
    #[cfg(any(target_os = "macos", test))]
    pub(super) const OPEN_LOGS: &str = "Open Logs";

    /// osascript arguments for a critical alert. The title and message are
    /// passed as arguments, never pasted into the script, so no text can
    /// break out of its quotes.
    #[cfg(any(target_os = "macos", test))]
    pub(super) fn mac_command(title: &str, text: &str, logs: Option<&Path>) -> Vec<String> {
        let (body, buttons) = match logs {
            Some(dir) => (
                format!("{text}\n\nThe logs are in:\n{}", dir.display()),
                format!("buttons {{\"Close\", \"{OPEN_LOGS}\"}} default button \"Close\""),
            ),
            None => (text.to_string(), "buttons {\"Close\"}".to_string()),
        };
        [
            "-e",
            "on run argv",
            "-e",
            &format!(
                "display alert (item 1 of argv) message (item 2 of argv) as critical {buttons}"
            ),
            "-e",
            "end run",
            title,
            &body,
        ]
        .iter()
        .map(|arg| arg.to_string())
        .collect()
    }

    #[cfg(target_os = "macos")]
    pub(super) fn alert(title: &str, text: &str, logs: Option<&Path>) -> bool {
        let Ok(output) = std::process::Command::new("osascript")
            .args(mac_command(title, text, logs))
            .output()
        else {
            return false;
        };
        // osascript prints "button returned:<name>" for the button pressed.
        let pressed = String::from_utf8_lossy(&output.stdout);
        match logs {
            Some(dir) if pressed.trim_end().ends_with(OPEN_LOGS) => {
                super::open(&dir.to_string_lossy())
            }
            _ => false,
        }
    }

    /// The Linux dialog tools to try in order, each as a program and its
    /// arguments.
    #[cfg(any(target_os = "linux", test))]
    pub(super) fn commands(title: &str, text: &str, logs: Option<&Path>) -> Vec<Vec<String>> {
        let (zenity, kdialog, body) = match logs {
            Some(dir) => (
                "--question",
                "--yesno",
                format!(
                    "{text}\n\nThe logs are in:\n{}\n\nOpen that folder now?",
                    dir.display()
                ),
            ),
            None => ("--error", "--error", text.to_string()),
        };
        let owned = |args: &[&str]| args.iter().map(|arg| arg.to_string()).collect();
        vec![
            owned(&[
                "zenity",
                zenity,
                "--no-markup",
                "--title",
                title,
                "--text",
                &body,
            ]),
            owned(&["kdialog", "--title", title, kdialog, &body]),
        ]
    }

    #[cfg(target_os = "linux")]
    pub(super) fn alert(title: &str, text: &str, logs: Option<&Path>) -> bool {
        let desktop = ["WAYLAND_DISPLAY", "DISPLAY"]
            .iter()
            .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()));
        if !desktop {
            return false;
        }
        for command in commands(title, text, logs) {
            // A tool that is not installed fails to start; try the next.
            let Ok(status) = std::process::Command::new(&command[0])
                .args(&command[1..])
                .status()
            else {
                continue;
            };
            // Both tools exit 0 for Yes (and for OK on an error).
            return match logs {
                Some(dir) if status.success() => super::open(&dir.to_string_lossy()),
                _ => false,
            };
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn linux_dialogs_offer_the_logs_folder_and_show_text_as_plain_text() {
        let logs = Path::new("/home/player/game/logs");
        let commands = desktop::commands("Title", "No <b>GPU</b>", Some(logs));
        let zenity = &commands[0];
        assert_eq!(zenity[..3], ["zenity", "--question", "--no-markup"]);
        let body = zenity.last().unwrap();
        assert!(body.starts_with("No <b>GPU</b>"));
        assert!(body.contains("/home/player/game/logs"));
        assert_eq!(commands[1][..4], ["kdialog", "--title", "Title", "--yesno"]);
        let error_only = desktop::commands("Title", "text", None);
        assert_eq!(error_only[0][1], "--error");
        assert_eq!(error_only[1][3], "--error");
    }

    #[test]
    fn mac_alerts_pass_text_as_arguments_and_offer_the_logs_folder() {
        let logs = Path::new("/Users/player/Library/Application Support/BlocklandReImagined/logs");
        let command = desktop::mac_command("Title", "Quote \" end run", Some(logs));
        let script: Vec<_> = command.iter().skip(1).step_by(2).take(3).collect();
        assert_eq!(script[0], "on run argv");
        assert!(script[1].contains(desktop::OPEN_LOGS));
        assert_eq!(script[2], "end run");
        // The player's text is an argument after the script, not inside it.
        assert_eq!(command[command.len() - 2], "Title");
        assert!(command.last().unwrap().starts_with("Quote \" end run"));
        assert!(command.last().unwrap().contains("BlocklandReImagined/logs"));
        let close_only = desktop::mac_command("Title", "text", None);
        assert!(!close_only[3].contains(desktop::OPEN_LOGS));
    }

    #[test]
    fn long_errors_are_shortened_for_the_dialog() {
        assert_eq!(summarize("short"), "short");
        let long = "x".repeat(2000);
        let shown = summarize(&long);
        assert!(shown.len() < 800 && shown.ends_with("..."));
    }
    #[test]
    fn dialogs_can_be_turned_off_for_headless_runs() {
        // SAFETY: test-local environment; no other test reads this variable.
        unsafe { std::env::set_var("BRI_NO_DIALOGS", "1") };
        assert!(!alert("Title", "Body", None));
    }
}
