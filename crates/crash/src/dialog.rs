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
    #[cfg(not(windows))]
    {
        let _ = logs;
        eprintln!("{title}: {text}");
        false
    }
}

/// Open a folder in Explorer or a web page in the browser. Returns whether
/// the desktop accepted it.
pub fn open(target: &str) -> bool {
    #[cfg(windows)]
    {
        imp::open(std::ffi::OsStr::new(target))
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
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
                format!("{text}\n\nThe logs are in:\n{}\n\nOpen that folder now?", dir.display()),
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

#[cfg(test)]
mod tests {
    use super::*;
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
