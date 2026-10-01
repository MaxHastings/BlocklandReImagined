//! Mini-game rules as the Create Mini-Game screen offers them.
use bri_client::app::App;
use bri_ui::api::MiniGameRules;

/// `rules` with its loadout keeping only the items `app`'s content offers,
/// as the Create Mini-Game screen does (the default loadout names v20's
/// stock items, which a made-up root does not have).
pub fn offered(app: &App, mut rules: MiniGameRules) -> MiniGameRules {
    for item in &mut rules.loadout {
        if item
            .as_ref()
            .is_some_and(|id| !app.content.weapons.pack.items.contains_key(id))
        {
            *item = None;
        }
    }
    rules
}
