//! Windows Firewall for hosts. The first time a program listens on the
//! network, Windows asks whether to allow it; "Cancel", or allowing only
//! private networks while on a public one, leaves block rules that silently
//! stop every friend from joining. The host checks the rules for this
//! executable and for its game port and, when friends would be blocked,
//! offers one fix: an elevated copy of the game (one Windows permission
//! prompt) removes this program's inbound rules and sets one allow rule for
//! the game's UDP ports. That rule names ports, not the program, so a new
//! build in another folder is let through without asking again; the rule is
//! kept under one name and replaced, never duplicated.
use anyhow::Result;

/// Flag for the elevated helper run of the game executable.
pub const ALLOW_FLAG: &str = "--allow-firewall";
/// The rule's name in Windows Defender Firewall.
pub const RULE_NAME: &str = "Blockland ReImagined";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Friends' connections are let through on the active networks.
    Allowed,
    /// A rule blocks this program on an active network.
    Blocked,
    /// No rule allows this program yet (Windows asks on first host).
    NotAllowed,
    /// The rules could not be read; nothing is claimed.
    Unknown,
}

impl Status {
    /// A line for the host player, or None when all is well.
    pub fn advice(self) -> Option<&'static str> {
        match self {
            Self::Blocked => Some(
                "Windows Firewall is blocking Blockland ReImagined, so friends cannot join. Choose Yes in the box that opens to fix it.",
            ),
            Self::NotAllowed => Some(
                "Windows Firewall has not allowed Blockland ReImagined yet. If Windows asked, choose Allow; otherwise choose Yes in the box that opens.",
            ),
            Self::Allowed | Self::Unknown => None,
        }
    }
}

/// The UDP ports the rule opens: the game port and LAN discovery.
pub fn ports(game_port: u16) -> String {
    let discovery = bri_net::discovery::DISCOVERY_PORT;
    if game_port == discovery {
        game_port.to_string()
    } else {
        format!("{game_port},{discovery}")
    }
}

/// Firewall state for this executable and `port` on the networks in use.
/// Blocking; takes a second or two on Windows.
pub fn status(port: u16) -> Status {
    #[cfg(windows)]
    {
        windows::status(port)
    }
    #[cfg(not(windows))]
    {
        let _ = port;
        Status::Unknown
    }
}

/// Ask Windows (one permission prompt) to let friends reach `port`.
/// Blocking until the helper finishes or the player declines the prompt.
pub fn allow(port: u16) -> Result<()> {
    #[cfg(windows)]
    {
        windows::allow(port)
    }
    #[cfg(not(windows))]
    {
        let _ = port;
        anyhow::bail!("Firewall rules are only managed on Windows")
    }
}

/// The elevated helper (`bri-client --allow-firewall <port>`): remove this
/// program's inbound rules (a cancelled Windows prompt leaves block rules,
/// and a block rule beats any allow rule) and set the one port rule.
pub fn run_helper(port: &str) -> Result<()> {
    let port: u16 = port
        .parse()
        .ok()
        .filter(|&p| p != 0)
        .ok_or_else(|| anyhow::anyhow!("{ALLOW_FLAG} needs a port"))?;
    #[cfg(windows)]
    {
        windows::run_helper(port)
    }
    #[cfg(not(windows))]
    {
        let _ = port;
        anyhow::bail!("Firewall rules are only managed on Windows")
    }
}

/// Decide from the firewall's own words. `profiles` are the active network
/// categories (`Public`, `Private`, `DomainAuthenticated`), `disabled` the
/// firewall profiles switched off, and each rule `Action|Profile` for an
/// enabled inbound rule on this program or the game port
/// (`Allow|Private, Public`, `Block|Any`).
pub fn decide(profiles: &[&str], disabled: &[&str], rules: &[&str]) -> Status {
    if profiles.is_empty() {
        return Status::Unknown;
    }
    let mut result = Status::Allowed;
    for category in profiles {
        let profile = match category.trim() {
            "DomainAuthenticated" => "Domain",
            other => other,
        };
        if disabled.iter().any(|d| d.trim().eq_ignore_ascii_case(profile)) {
            continue;
        }
        let covers = |rule_profiles: &str| {
            rule_profiles
                .split(',')
                .map(str::trim)
                .any(|p| p.eq_ignore_ascii_case("Any") || p.eq_ignore_ascii_case(profile))
        };
        let (mut allow, mut block) = (false, false);
        for rule in rules {
            let Some((action, rule_profiles)) = rule.split_once('|') else {
                return Status::Unknown;
            };
            if covers(rule_profiles) {
                match action.trim() {
                    "Allow" => allow = true,
                    "Block" => block = true,
                    _ => {}
                }
            }
        }
        // Windows lets a block rule win over an allow rule.
        if block {
            return Status::Blocked;
        }
        if !allow {
            result = Status::NotAllowed;
        }
    }
    result
}

#[cfg(windows)]
mod windows {
    use super::*;
    use anyhow::{Context, bail};
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn status(port: u16) -> Status {
        let Ok(exe) = std::env::current_exe() else {
            return Status::Unknown;
        };
        // PowerShell's NetSecurity module reads rules without administrator
        // rights; enum names are not localized, unlike netsh output.
        let program = exe.display().to_string().replace('\'', "''");
        let script = format!(
            "$ErrorActionPreference='SilentlyContinue';\
             'profiles=' + ((Get-NetConnectionProfile | ForEach-Object {{ \"$($_.NetworkCategory)\" }}) -join ';');\
             'disabled=' + ((Get-NetFirewallProfile | Where-Object {{ \"$($_.Enabled)\" -eq 'False' }} | ForEach-Object {{ \"$($_.Name)\" }}) -join ';');\
             @(Get-NetFirewallApplicationFilter -Program '{program}' | Get-NetFirewallRule) + \
             @(Get-NetFirewallPortFilter -Protocol UDP | Where-Object {{ @($_.LocalPort) -contains '{port}' }} | Get-NetFirewallRule) | \
             Where-Object {{ \"$($_.Direction)\" -eq 'Inbound' -and \"$($_.Enabled)\" -eq 'True' }} | \
             Sort-Object -Property Name -Unique | \
             ForEach-Object {{ 'rule=' + \"$($_.Action)|$($_.Profile)\" }}"
        );
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let Ok(output) = output else {
            return Status::Unknown;
        };
        let text = String::from_utf8_lossy(&output.stdout);
        let (mut profiles, mut disabled, mut rules) = (Vec::new(), Vec::new(), Vec::new());
        for line in text.lines() {
            if let Some(value) = line.strip_prefix("profiles=") {
                profiles.extend(value.split(';').filter(|v| !v.is_empty()));
            } else if let Some(value) = line.strip_prefix("disabled=") {
                disabled.extend(value.split(';').filter(|v| !v.is_empty()));
            } else if let Some(value) = line.strip_prefix("rule=") {
                rules.push(value);
            }
        }
        decide(&profiles, &disabled, &rules)
    }

    pub fn allow(port: u16) -> Result<()> {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, GetLastError},
            System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject},
            UI::{
                Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW},
                WindowsAndMessaging::SW_HIDE,
            },
        };
        let exe = std::env::current_exe()?;
        let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let verb = wide("runas");
        let file = wide(&exe.display().to_string());
        let parameters = wide(&format!("{ALLOW_FLAG} {port}"));
        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = parameters.as_ptr();
        info.nShow = SW_HIDE;
        // SAFETY: every pointer in `info` outlives the call.
        if unsafe { ShellExecuteExW(&mut info) } == 0 {
            let error = unsafe { GetLastError() };
            // ERROR_CANCELLED: the player said no to the permission prompt.
            if error == 1223 {
                bail!("Windows permission was not given, so the firewall was left unchanged.");
            }
            bail!("Could not ask Windows for permission (error {error})");
        }
        let process = info.hProcess;
        ensure_process(process)?;
        let mut code = 1u32;
        // SAFETY: `process` is a valid handle owned here and closed below.
        unsafe {
            WaitForSingleObject(process, INFINITE);
            GetExitCodeProcess(process, &mut code);
            CloseHandle(process);
        }
        if code != 0 {
            bail!("Windows did not accept the firewall rule (code {code})");
        }
        Ok(())
    }

    fn ensure_process(process: windows_sys::Win32::Foundation::HANDLE) -> Result<()> {
        if process.is_null() {
            bail!("Could not start the firewall helper");
        }
        Ok(())
    }

    pub fn run_helper(port: u16) -> Result<()> {
        let exe = std::env::current_exe()?;
        let program = format!("program={}", exe.display());
        let netsh = |args: &[&str]| {
            std::process::Command::new("netsh.exe")
                .args(args)
                .creation_flags(CREATE_NO_WINDOW)
                .status()
                .context("Could not run netsh")
        };
        // Old rules for this program, including the block rules a cancelled
        // Windows prompt leaves; failing because none exist is fine.
        let _ = netsh(&["advfirewall", "firewall", "delete", "rule", "name=all", "dir=in", &program]);
        // Our rule from an earlier build or port, replaced rather than
        // duplicated.
        let name = format!("name={RULE_NAME}");
        let _ = netsh(&["advfirewall", "firewall", "delete", "rule", &name, "dir=in"]);
        let ports = format!("localport={}", ports(port));
        let added = netsh(&[
            "advfirewall", "firewall", "add", "rule", &name, "dir=in", "action=allow",
            "protocol=udp", &ports, "profile=any", "enable=yes",
        ])?;
        if !added.success() {
            bail!("netsh could not add the firewall rule");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn firewall_rules_decide_whether_friends_get_through() {
        use Status::*;
        assert_eq!(decide(&["Private"], &[], &["Allow|Private"]), Allowed);
        assert_eq!(decide(&["Private"], &[], &["Allow|Any"]), Allowed);
        // The Windows prompt with only "Private networks" ticked allows
        // private and blocks public.
        let prompt = ["Allow|Private", "Block|Public"];
        assert_eq!(decide(&["Private"], &[], &prompt), Allowed);
        assert_eq!(decide(&["Public"], &[], &prompt), Blocked);
        // Cancel leaves block rules; block wins over allow.
        assert_eq!(decide(&["Private"], &[], &["Block|Any", "Allow|Any"]), Blocked);
        assert_eq!(decide(&["Public"], &[], &["Allow|Private, Public"]), Allowed);
        assert_eq!(decide(&["DomainAuthenticated"], &[], &["Allow|Domain"]), Allowed);
        assert_eq!(decide(&["Public"], &[], &[]), NotAllowed);
        // A switched-off firewall blocks nothing.
        assert_eq!(decide(&["Public"], &["Public"], &["Block|Any"]), Allowed);
        // Two networks at once: each must pass.
        assert_eq!(decide(&["Private", "Public"], &[], &["Allow|Private"]), NotAllowed);
        assert_eq!(decide(&[], &[], &["Allow|Any"]), Unknown);
        assert_eq!(decide(&["Public"], &[], &["garbage"]), Unknown);
    }
    #[test]
    fn the_rule_opens_the_game_port_and_lan_discovery() {
        assert_eq!(ports(28000), "28000,28050");
        assert_eq!(ports(28050), "28050");
    }
    #[test]
    fn only_problems_get_advice() {
        assert!(Status::Allowed.advice().is_none() && Status::Unknown.advice().is_none());
        assert!(Status::Blocked.advice().unwrap().contains("blocking"));
    }
}
