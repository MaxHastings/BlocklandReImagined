//! Which build this is, and whether a newer release exists.
//!
//! The game never downloads or installs anything. A release build asks the
//! repository's public GitHub Releases page once per start; when a release
//! with a higher version number exists, the main menu says so and
//! offers the download page. Development builds, the Options toggle
//! (`$pref::Net::CheckForUpdates`) and being offline all mean no message.
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

/// The version players see: the release name (the dist folder's version) or
/// `dev-<commit date>`, plus the short commit hash.
pub const NAME: &str = env!("BRI_BUILD_NAME");
pub const HASH: &str = env!("BRI_BUILD_HASH");
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
    std::thread::Builder::new()
        .name("bri-update-check".into())
        .spawn(move || match fetch_latest() {
            Ok(body) => match newer(&body, NAME) {
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

/// The latest release, if its version number is higher than this build's.
/// Only release version numbers count: players download releases, never
/// commits, so the commit hash and dates play no part.
fn newer(body: &str, current: &str) -> Option<Newer> {
    let release: serde_json::Value = serde_json::from_str(body).ok()?;
    if release["draft"].as_bool() == Some(true) {
        return None;
    }
    let name = release["tag_name"].as_str()?.trim();
    let url = release["html_url"].as_str()?;
    let latest = version_number(name);
    if latest.is_empty()
        || latest <= version_number(current)
        || !url.starts_with("https://github.com/")
    {
        return None;
    }
    Some(Newer {
        name: name.to_string(),
        url: url.to_string(),
    })
}

/// A release's version number: the numbers in its name, in order, so that
/// every spelling of one release is equal (`v0.1.11`, `v0.1.11-alpha`,
/// `0.1.11`) and a later release is greater (`v0.1.12-alpha` > `v0.1.11`,
/// `2026-10-02-a14` > `alpha-2026-09-28-a13`).
fn version_number(name: &str) -> Vec<u64> {
    name.split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse().unwrap_or(u64::MAX))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> String {
        serde_json::json!({
            "tag_name": tag,
            "html_url": format!("https://github.com/MaxHastings/BlocklandReImagined/releases/tag/{tag}"),
            "draft": false,
        })
        .to_string()
    }

    #[test]
    fn only_a_higher_release_version_is_newer() {
        let later = release("2026-10-02-a14");
        assert_eq!(
            newer(&later, "2026-09-28-a13"),
            Some(Newer {
                name: "2026-10-02-a14".into(),
                url:
                    "https://github.com/MaxHastings/BlocklandReImagined/releases/tag/2026-10-02-a14"
                        .into(),
            })
        );
        // The release this build is, however the tag spells it.
        assert_eq!(
            newer(&release("alpha-2026-09-28-a13"), "2026-09-28-a13"),
            None
        );
        let tagged = release("v0.1.11-alpha");
        assert_eq!(newer(&tagged, "v0.1.11"), None);
        assert_eq!(newer(&tagged, "0.1.11"), None);
        assert_eq!(newer(&tagged, "V0.1.11-Alpha"), None);
        // A later release, and an older one.
        let next = release("v0.1.12-alpha");
        assert_eq!(
            newer(&next, "v0.1.11").map(|n| n.name),
            Some("v0.1.12-alpha".into())
        );
        assert_eq!(newer(&release("v0.1.10-alpha"), "v0.1.11"), None);
        assert_eq!(newer(&release("v0.1.2"), "v0.1.11"), None);
        // Garbage, drafts, unnumbered tags and foreign links say nothing.
        assert_eq!(newer("not json", "x"), None);
        assert_eq!(newer(&release("latest"), "v0.1.11"), None);
        let draft = later.replace("\"draft\":false", "\"draft\":true");
        assert_eq!(newer(&draft, "2026-09-28-a13"), None);
        let foreign = later.replace("https://github.com/", "https://example.com/");
        assert_eq!(newer(&foreign, "2026-09-28-a13"), None);
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
