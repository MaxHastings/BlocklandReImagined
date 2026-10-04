//! Add-Ons' splashes over the main menu (`splash.json`, Slayer's Happy
//! Holidays): on a day between `from` and `to`, once a year, the first
//! enabled Add-On's splash shows when the game starts. The date is the
//! player's local calendar day, as the original's `getDateTime()` read it
//! (UTC where the system's time zone cannot be read without a library:
//! everywhere but Windows).
use crate::save_picture::Picture;
use bri_package_runtime::{Catalog, content::Kind};
use bri_ui::api::{SplashFallingView, SplashView};

/// UI texture ids splash pictures take (a range of their own).
const FIRST_ID: u64 = 0x425249_53504c00;

/// Today's local (year, month, day).
pub fn today() -> (i64, u8, u8) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    date_at(secs, local_offset(secs))
}

/// The calendar date `secs` after the epoch, `offset` seconds east of UTC.
fn date_at(secs: i64, offset: i64) -> (i64, u8, u8) {
    civil((secs + offset).div_euclid(86_400))
}

/// The system's offset from UTC, in seconds east, at `secs`.
#[cfg(windows)]
fn local_offset(secs: i64) -> i64 {
    use windows_sys::Win32::{Foundation::SYSTEMTIME, System::SystemInformation::GetLocalTime};
    let mut t = SYSTEMTIME::default();
    // SAFETY: GetLocalTime only writes the SYSTEMTIME it is given.
    unsafe { GetLocalTime(&mut t) };
    if t.wYear == 0 {
        return 0;
    }
    let local = days(i64::from(t.wYear), t.wMonth as u8, t.wDay as u8) * 86_400
        + i64::from(t.wHour) * 3_600
        + i64::from(t.wMinute) * 60
        + i64::from(t.wSecond);
    // Zones are whole quarter hours; the two clocks were read apart.
    let offset = ((local - secs) as f64 / 900.0).round() as i64 * 900;
    offset.clamp(-14 * 3_600, 14 * 3_600)
}
#[cfg(not(windows))]
fn local_offset(_secs: i64) -> i64 {
    0
}

/// Days since 1970-01-01 of a calendar date (Howard Hinnant's
/// `days_from_civil`), the inverse of [`civil`].
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn days(year: i64, month: u8, day: u8) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Days since 1970-01-01 as a calendar date (Howard Hinnant's
/// `civil_from_days`).
fn civil(days: i64) -> (i64, u8, u8) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Whether `[month, day]` falls between `from` and `to` (across the new
/// year when `to` is earlier).
fn within(day: [u8; 2], from: [u8; 2], to: [u8; 2]) -> bool {
    if from <= to {
        from <= day && day <= to
    } else {
        day >= from || day <= to
    }
}

/// A splash to show: its pref key, the view and the pictures to upload.
pub type DueSplash = (String, SplashView, Vec<(u64, Picture)>);

/// The splash due today that has not shown this year. `shown(key)` gives the year a splash
/// last showed.
pub fn due(
    catalog: &Catalog,
    today: (i64, u8, u8),
    shown: impl Fn(&str) -> Option<i64>,
) -> Option<DueSplash> {
    let (year, month, day) = today;
    for (id, package) in &catalog.packages {
        for (asset, splash) in &package.splashes {
            let key = format!("$Pref::Splash::{asset}");
            if !within([month, day], splash.from, splash.to) || shown(&key) == Some(year) {
                continue;
            }
            let mut pictures: Vec<(String, u64, Picture)> = Vec::new();
            let mut texture = |file: &str| -> Option<u64> {
                if let Some((_, id, _)) = pictures.iter().find(|(f, _, _)| f == file) {
                    return Some(*id);
                }
                let bytes = &package
                    .assets
                    .iter()
                    .find(|a| a.kind == Kind::Image && a.file == file)?
                    .bytes;
                let image = image::load_from_memory(bytes).ok()?.to_rgba8();
                let id = FIRST_ID + pictures.len() as u64;
                pictures.push((
                    file.to_owned(),
                    id,
                    Picture {
                        width: image.width(),
                        height: image.height(),
                        rgba: image.into_raw(),
                    },
                ));
                Some(id)
            };
            let layers: Option<Vec<_>> = splash
                .layers
                .iter()
                .map(|l| Some((texture(&l.image)?, l.rect, l.fade_in_ms)))
                .collect();
            let Some(layers) = layers else {
                eprintln!("{id}: a splash picture is missing or unreadable");
                continue;
            };
            let falling = splash.falling.as_ref().map(|f| SplashFallingView {
                textures: f.images.iter().filter_map(|i| texture(i)).collect(),
                size: f.size,
                chance: f.chance,
                speed: f.speed,
                step_ms: f.step_ms,
                closing_speed: f.closing_speed,
            });
            let view = SplashView {
                layers,
                falling,
                tip: splash
                    .tip
                    .as_ref()
                    .map(|t| (t.text.clone(), t.rect, t.after_ms)),
                close_after_ms: splash.close_after_ms,
                fade_out_ms: splash.fade_out_ms,
            };
            return Some((
                key,
                view,
                pictures.into_iter().map(|(_, id, p)| (id, p)).collect(),
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_date_is_the_local_calendar_day() {
        // 2025-12-19 23:30 UTC is already the 20th two hours east, and
        // still the 19th five hours west.
        let late = days(2025, 12, 19) * 86_400 + 23 * 3_600 + 1_800;
        assert_eq!(date_at(late, 0), (2025, 12, 19));
        assert_eq!(date_at(late, 2 * 3_600), (2025, 12, 20));
        assert_eq!(date_at(late - 22 * 3_600, -5 * 3_600), (2025, 12, 18));
        for day in [0, 11_016, 20_442, -1, 100_000] {
            let (y, m, d) = civil(day);
            assert_eq!(days(y, m, d), day);
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert!(local_offset(now).abs() <= 14 * 3_600);
    }

    #[test]
    fn calendar_days_and_windows() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_442), (2025, 12, 20));
        assert_eq!(civil(11_016), (2000, 2, 29));
        assert!(within([12, 20], [12, 20], [12, 31]));
        assert!(within([12, 31], [12, 20], [12, 31]));
        assert!(!within([12, 19], [12, 20], [12, 31]));
        assert!(!within([1, 1], [12, 20], [12, 31]));
        // Across the new year.
        assert!(within([1, 2], [12, 20], [1, 5]));
        assert!(!within([6, 1], [12, 20], [1, 5]));
    }
}
