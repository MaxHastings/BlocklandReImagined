//! RTB server preferences: `RTB_registerPref(title, category, global,
//! type, add-on, default, needsRestart, hostOnly)` as server-wide Add-On
//! settings (`bri_package::setting`, scope `server`). The host changes them
//! in the Admin menu's Add-On Settings, which is where RTB's server control
//! put them; a port's rules read each by its global with `pref(name)`.
//!
//! The game plays the copy's RTB branch: with server settings there is no
//! need for the `TT_defaultIfUnset`-style fallback a script ran without
//! RTB, so the defaults are RTB's own.
use bri_convert::tscript::Call;
use bri_package::setting::{
    SettingDef, SettingEditor, SettingItem, SettingScope, SettingType, SettingValue,
};

/// One `RTB_registerPref` call of the copy.
#[derive(Debug, Clone, PartialEq)]
pub struct Pref {
    /// Its global, as the copy spells it (`$Pref::Server::TT::Ammo`).
    pub global: String,
    /// The setting it becomes, or why it cannot be one.
    pub setting: Result<SettingDef, String>,
}

/// `RTB_registerPref` calls among `calls`.
pub fn prefs(calls: &[Call]) -> Vec<(usize, Pref)> {
    calls
        .iter()
        .filter(|c| c.receiver.is_none() && c.callee.eq_ignore_ascii_case("rtb_registerpref"))
        .filter_map(|c| {
            let global = crate::literal(c.args.get(2)?).to_owned();
            Some((
                c.line,
                Pref {
                    setting: setting(&c.args, &global),
                    global,
                },
            ))
        })
        .collect()
}

/// The setting key for `global`: what follows `$Pref::Server::`, lower
/// case, with `::` as `_` (`tt_ammo`).
pub fn key(global: &str) -> Option<String> {
    let rest = global
        .get(.."$Pref::Server::".len())
        .filter(|p| p.eq_ignore_ascii_case("$Pref::Server::"))
        .map(|_| &global["$Pref::Server::".len()..])?;
    let key = rest.replace("::", "_").to_ascii_lowercase();
    (bri_package::setting::is_setting_ref(&key) && !key.contains(':')).then_some(key)
}

fn setting(args: &[String], global: &str) -> Result<SettingDef, String> {
    if args.len() < 6 {
        return Err("has fewer than its 6 arguments".into());
    }
    if !bri_package::setting::is_pref_global(global) {
        return Err(format!("`{global}` is not a $Pref::Server:: global"));
    }
    let key = key(global).ok_or_else(|| format!("`{global}` makes no setting key"))?;
    let title = crate::literal(&args[0]).trim().to_owned();
    // "Tier+Tactical | Ammo": the menu heads the Add-On's settings with its
    // own name, so the part after the bar is the heading.
    let category = crate::literal(&args[1]);
    let category = category
        .rsplit_once('|')
        .map_or(category, |(_, c)| c)
        .trim()
        .chars()
        .take(32)
        .collect::<String>();
    let kind = crate::literal(&args[3]);
    let default = crate::player_types::number(&args[5]);
    let mut words = kind.split_whitespace();
    let mut def = SettingDef {
        key,
        title: title.chars().take(64).collect(),
        category,
        scope: SettingScope::Server,
        kind: SettingType::Bool,
        default: SettingValue::Bool(false),
        min: None,
        max: None,
        items: Vec::new(),
        max_length: None,
        editor: SettingEditor::Admin,
        shown_when: None,
        global: Some(global.to_owned()),
        // needsRestart: the Add-On read it as it loaded.
        restart: args.get(6).is_some_and(|a| crate::literal(a).trim() == "1"),

        quiet: false,
        resets: false,
        help: String::new(),
        avatar: None,
    };
    let whole = |n: f32| (n.fract() == 0.0 && n.abs() <= 1e9).then_some(n as i64);
    match words.next().map(str::to_ascii_lowercase).as_deref() {
        Some("bool") => {
            def.default = SettingValue::Bool(default.ok_or("its default is not a number")? != 0.0);
        }
        Some("int") => {
            let bound = |w: Option<&str>| w.and_then(|w| whole(w.parse().ok()?));
            let (min, max) = (bound(words.next()), bound(words.next()));
            let (Some(min), Some(max)) = (min, max) else {
                return Err(format!("its type `{kind}` has no whole-number range"));
            };
            def.kind = SettingType::Int;
            def.min = Some(min);
            def.max = Some(max);
            let n = default
                .and_then(whole)
                .ok_or("its default is not a whole number")?;
            def.default = SettingValue::Int(n.clamp(min, max));
        }
        Some("list") => {
            let rest: Vec<&str> = words.collect();
            if rest.is_empty() || !rest.len().is_multiple_of(2) {
                return Err(format!("its type `{kind}` is not name and value pairs"));
            }
            def.kind = SettingType::List;
            for pair in rest.chunks(2) {
                let value = match pair[1].parse::<i64>() {
                    Ok(n) => SettingValue::Int(n),
                    Err(_) => SettingValue::Text(pair[1].to_owned()),
                };
                def.items.push(SettingItem {
                    value,
                    name: pair[0].replace('_', " "),
                });
            }
            let raw = crate::literal(&args[5]).trim();
            def.default = default
                .and_then(whole)
                .map(SettingValue::Int)
                .filter(|v| def.items.iter().any(|i| i.value == *v))
                .or_else(|| {
                    def.items
                        .iter()
                        .find(|i| i.value == SettingValue::Text(raw.to_owned()))
                        .map(|i| i.value.clone())
                })
                .ok_or("its default is none of its choices")?;
        }
        Some("string") => {
            let n = words
                .next()
                .and_then(|w| w.parse::<u32>().ok())
                .filter(|n| (1..=bri_package::setting::MAX_TEXT as u32).contains(n))
                .ok_or_else(|| format!("its type `{kind}` has no length"))?;
            def.kind = SettingType::Text;
            def.max_length = Some(n);
            def.default =
                SettingValue::Text(crate::literal(&args[5]).chars().take(n as usize).collect());
        }
        _ => return Err(format!("its type `{kind}` has no setting kind here")),
    }
    def.validate()?;
    Ok(def)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(args: &[&str]) -> Call {
        Call {
            callee: "RTB_registerPref".into(),
            receiver: None,
            args: args.iter().map(|a| (*a).to_owned()).collect(),
            line: 1,
        }
    }

    #[test]
    fn tier_prefs_become_server_settings_with_rtb_defaults() {
        let read = |args: &[&str]| prefs(&[call(args)]).remove(0).1;
        let ammo = read(&[
            "\"Ammo System\"",
            "\"Tier+Tactical | Ammo\"",
            "\"$Pref::Server::TT::Ammo\"",
            "\"list T+T2 0 T+T1 1 Classic 2 Arena 3\"",
            "\"Weapon_Package_Tier1\"",
            "0",
            "0",
            "1",
        ]);
        let def = ammo.setting.unwrap();
        assert_eq!(
            (def.key.as_str(), def.category.as_str()),
            ("tt_ammo", "Ammo")
        );
        assert_eq!(def.kind, SettingType::List);
        assert_eq!(def.items.len(), 4);
        assert_eq!(def.items[2].name, "Classic");
        assert_eq!(def.default, SettingValue::Int(0));
        assert_eq!(def.global.as_deref(), Some("$Pref::Server::TT::Ammo"));
        let start = read(&[
            "\"Starting Pistol Ammo\"",
            "\"Tier+Tactical | Starting Ammo\"",
            "\"$Pref::Server::TT::Start9MM\"",
            "\"int 0 280\"",
            "%mod",
            "35*4",
            "0",
            "1",
        ])
        .setting
        .unwrap();
        assert_eq!(
            (start.key.as_str(), start.min, start.max),
            ("tt_start9mm", Some(0), Some(280))
        );
        assert_eq!(start.default, SettingValue::Int(140));
        let heal = read(&[
            "\"Can Heal Bots\"",
            "\"Tier+Tactical | Medical\"",
            "\"$Pref::Server::TT::MedicHealBots\"",
            "\"bool\"",
            "%mod",
            "0",
            "0",
            "1",
        ])
        .setting
        .unwrap();
        assert_eq!(
            (heal.kind, heal.default),
            (SettingType::Bool, SettingValue::Bool(false))
        );
        let restart = read(&[
            "\"???\"",
            "\"Tier+Tactical | Miscellaneous\"",
            "\"$Pref::Server::TT::EasterEgg\"",
            "\"bool\"",
            "\"Weapon_Package_Tier1\"",
            "0",
            "1",
            "1",
        ]);
        // needsRestart: a setting the game reads at the next start.
        let restart = restart.setting.unwrap();
        assert!(restart.restart);
        assert_eq!(restart.default, SettingValue::Bool(false));
        let odd = read(&[
            "\"X\"",
            "\"Y\"",
            "\"$Pref::Server::TT::X\"",
            "\"num 0 1\"",
            "%m",
            "0",
            "0",
            "1",
        ]);
        assert!(
            odd.setting.is_err(),
            "a type with no setting kind is said so"
        );
    }
}
