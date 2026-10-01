//! Which build this is, and whether a newer release exists.
//!
//! The game never downloads or installs anything. A release build asks the
//! repository's public GitHub Releases page once per start; when a release
//! published after this build's source exists, the main menu says so and
//! offers the download page. Development builds, the Options toggle
//! (`$pref::Net::CheckForUpdates`) and being offline all mean no message.
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

/// The version players see: the release name (the dist folder's version) or
/// `dev-<commit date>`, plus the short commit hash.
pub const NAME: &str = env!("BRI_BUILD_NAME");
pub const HASH: &str = env!("BRI_BUILD_HASH");
/// Unix seconds when this build's commit was made (0 if unknown).
const COMMITTED: &str = env!("BRI_BUILD_COMMITTED");
/// Built with `BRI_VERSION` set: a release players were given.
pub const RELEASE: bool = !env!("BRI_BUILD_RELEASE").is_empty();

/// Options → Advanced "Check for new versions" (on unless turned off).
pub const CHECK_PREF: &str = "$pref::Net::CheckForUpdates";
const LATEST_API: &str =
    "https://api.github.com/repos/MaxHastings/BlocklandReImagined/releases/latest";

/// `2026-09-28-a13 (1a2b3c4d5)`.
pub fn version() -> String {
    format!("{NAME} ({HASH})")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Newer {
    /// The release's tag, as players know it.
    pub name: String,
    /// Its page on GitHub, where the download is.
    pub url: String,
}

/// Start the one check of this run on a background thread, if this build
/// and the player's settings allow it. Poll the receiver each frame; it
/// yields at most one answer, and nothing when there is no newer release or
/// the check failed.
pub fn start(settings: &bri_ui::api::Settings) -> Option<Receiver<Newer>> {
    let prefs = bri_ui::prefs::Prefs::new(&Default::default(), &settings.prefs);
    if !RELEASE || !prefs.bool_or(CHECK_PREF, true) {
        return None;
    }
    let (send, receive) = mpsc::channel();
    let committed = COMMITTED.parse().unwrap_or(0);
    std::thread::Builder::new()
        .name("bri-update-check".into())
        .spawn(move || match fetch_latest() {
            Ok(body) => match newer(&body, NAME, committed) {
                Some(found) => {
                    eprintln!(
                        "A newer version is available: {} ({})",
                        found.name, found.url
                    );
                    let _ = send.send(found);
                }
                None => eprintln!("Update check: {NAME} is the newest release."),
            },
            // Offline, blocked, or nothing published yet: stay quiet.
            Err(error) => eprintln!("Update check skipped: {error}"),
        })
        .ok()?;
    Some(receive)
}

fn fetch_latest() -> Result<String, String> {
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .tls_config(tls)
        .build()
        .into();
    let mut response = agent
        .get(LATEST_API)
        .header("User-Agent", &format!("BlocklandReImagined/{NAME}"))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?;
    response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_string()
        .map_err(|e| e.to_string())
}

/// The latest release, if it is a different release published after this
/// build's source was committed. Comparing dates as well as names keeps a
/// build made after the latest release (a newer local build) quiet.
fn newer(body: &str, current: &str, committed: u64) -> Option<Newer> {
    let release: serde_json::Value = serde_json::from_str(body).ok()?;
    if release["draft"].as_bool() == Some(true) {
        return None;
    }
    let name = release["tag_name"].as_str()?.trim();
    let url = release["html_url"].as_str()?;
    let published = unix_time(release["published_at"].as_str()?)?;
    let same = name.eq_ignore_ascii_case(current)
        || name
            .trim_start_matches("alpha-")
            .eq_ignore_ascii_case(current.trim_start_matches("alpha-"));
    if same || published <= committed || !url.starts_with("https://github.com/") {
        return None;
    }
    Some(Newer {
        name: name.to_string(),
        url: url.to_string(),
    })
}

/// Seconds since 1970 for GitHub's `YYYY-MM-DDTHH:MM:SSZ`.
fn unix_time(text: &str) -> Option<u64> {
    let b = text.as_bytes();
    if b.len() != 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[19] != b'Z' {
        return None;
    }
    let num = |range: std::ops::Range<usize>| text.get(range)?.parse::<i64>().ok();
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hh, mm, ss) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Howard Hinnant's days-from-civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + hh * 3600 + mm * 60 + ss).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, published: &str) -> String {
        serde_json::json!({
            "tag_name": tag,
            "html_url": format!("https://github.com/MaxHastings/BlocklandReImagined/releases/tag/{tag}"),
            "published_at": published,
            "draft": false,
        })
        .to_string()
    }

    #[test]
    fn github_times_parse_as_utc() {
        assert_eq!(unix_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(unix_time("2026-09-21T14:13:20Z"), Some(1_790_000_000));
        assert_eq!(unix_time("2000-02-29T00:00:00Z"), Some(951_782_400));
        assert_eq!(unix_time("2026-09-21 14:13:20"), None);
        assert_eq!(unix_time("2026-13-21T14:13:20Z"), None);
    }

    #[test]
    fn only_a_later_different_release_is_newer() {
        let built = unix_time("2026-09-28T04:00:00Z").unwrap();
        let later = release("2026-10-02-a14", "2026-10-02T12:00:00Z");
        assert_eq!(
            newer(&later, "2026-09-28-a13", built),
            Some(Newer {
                name: "2026-10-02-a14".into(),
                url:
                    "https://github.com/MaxHastings/BlocklandReImagined/releases/tag/2026-10-02-a14"
                        .into(),
            })
        );
        // The release this build is (published after it was built).
        let same = release("alpha-2026-09-28-a13", "2026-09-28T06:00:00Z");
        assert_eq!(newer(&same, "2026-09-28-a13", built), None);
        // A build made after the latest release.
        let older = release("2026-09-20-a12", "2026-09-20T12:00:00Z");
        assert_eq!(newer(&older, "2026-09-28-a13", built), None);
        // Garbage, drafts and foreign links say nothing.
        assert_eq!(newer("not json", "x", built), None);
        let draft = later.replace("\"draft\":false", "\"draft\":true");
        assert_eq!(newer(&draft, "2026-09-28-a13", built), None);
        let foreign = later.replace("https://github.com/", "https://example.com/");
        assert_eq!(newer(&foreign, "2026-09-28-a13", built), None);
    }

    #[test]
    fn development_builds_never_check() {
        if !RELEASE {
            assert!(start(&Default::default()).is_none());
        }
        let mut settings = bri_ui::api::Settings::default();
        settings.prefs.insert(CHECK_PREF.into(), "0".into());
        assert!(
            start(&settings).is_none(),
            "the Options toggle turns it off"
        );
    }
}
