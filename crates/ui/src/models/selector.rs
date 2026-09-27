//! Brick selector cart and favorites (`BSD_ClickInv`, `BSD_ClickIcon`,
//! `BSD_*Favorites`, c:10181–10500). Bricks are referenced by catalog index;
//! favorites are stored by `uiName` like v20's `config/client/Favorites.cs`.

use crate::api::BrickInfo;
use std::collections::BTreeMap;

pub const CART_SLOTS: usize = 10;

/// Tabs and sections derived from the catalog (datablock order).
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogLayout {
    pub tabs: Vec<Tab>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tab {
    pub name: String,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub name: String,
    /// Catalog indices.
    pub bricks: Vec<usize>,
}

impl CatalogLayout {
    /// Categories and sub-categories in first-appearance order.
    pub fn build(catalog: &[BrickInfo]) -> Self {
        let mut tabs: Vec<Tab> = Vec::new();
        for (i, b) in catalog.iter().enumerate() {
            let t = match tabs
                .iter_mut()
                .position(|t| t.name.eq_ignore_ascii_case(&b.category))
            {
                Some(p) => p,
                None => {
                    tabs.push(Tab {
                        name: b.category.clone(),
                        sections: Vec::new(),
                    });
                    tabs.len() - 1
                }
            };
            let tab = &mut tabs[t];
            match tab
                .sections
                .iter_mut()
                .find(|s| s.name.eq_ignore_ascii_case(&b.subcategory))
            {
                Some(s) => s.bricks.push(i),
                None => tab.sections.push(Section {
                    name: b.subcategory.clone(),
                    bricks: vec![i],
                }),
            }
        }
        CatalogLayout { tabs }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectorModel {
    /// `$BSD_InvData` (persists between openings).
    pub cart: [Option<usize>; CART_SLOTS],
    /// `$BSD_CurrClickData`.
    pub clicked_brick: Option<usize>,
    /// `$BSD_CurrClickInv`.
    pub clicked_slot: Option<usize>,
    pub tab: usize,
    /// "Set Favs>" helper visible.
    pub setting_favs: bool,
    pub queue_brick_buying: bool,
    pub favorites: BTreeMap<u8, Vec<String>>,
}

impl Default for SelectorModel {
    fn default() -> Self {
        SelectorModel {
            cart: [None; CART_SLOTS],
            clicked_brick: None,
            clicked_slot: None,
            tab: 0,
            setting_favs: false,
            queue_brick_buying: true,
            favorites: BTreeMap::new(),
        }
    }
}

impl SelectorModel {
    /// `BrickSelectorDlg::onWake`.
    pub fn open(&mut self) {
        self.clicked_brick = None;
        self.clicked_slot = None;
        self.setting_favs = false;
    }

    /// `BSD_ClickInv`.
    pub fn click_slot(&mut self, i: usize) {
        if self.clicked_slot == Some(i) {
            self.cart[i] = None;
            self.clicked_brick = None;
            self.clicked_slot = None;
        } else if let Some(prev) = self.clicked_slot {
            self.cart.swap(i, prev);
            self.clicked_brick = None;
            self.clicked_slot = None;
        } else if let Some(b) = self.clicked_brick {
            self.cart[i] = Some(b);
            self.clicked_brick = None;
            self.clicked_slot = None;
        } else {
            self.clicked_brick = None;
            self.clicked_slot = Some(i);
        }
    }

    /// `BSD_ClickIcon`: select; click the same tile again to add it.
    pub fn click_brick(&mut self, b: usize) {
        if self.clicked_brick == Some(b) {
            if let Some(open) = self.cart.iter().position(Option::is_none) {
                self.cart[open] = Some(b);
            } else if self.queue_brick_buying {
                self.cart.rotate_left(1);
                self.cart[CART_SLOTS - 1] = Some(b);
            }
            self.clicked_brick = None;
            self.clicked_slot = None;
        } else {
            self.clicked_brick = Some(b);
            self.clicked_slot = None;
        }
    }

    pub fn clear_cart(&mut self) {
        self.cart = [None; CART_SLOTS];
        self.clicked_slot = None;
    }

    pub fn next_tab(&mut self, tabs: usize) {
        if tabs > 0 {
            self.tab = (self.tab + 1) % tabs;
        }
    }

    /// "Set Favs>" / " Cancel ".
    pub fn toggle_set_favs(&mut self) {
        self.setting_favs = !self.setting_favs;
    }

    pub fn set_favs_label(&self) -> &'static str {
        if self.setting_favs {
            " Cancel "
        } else {
            "Set Favs>"
        }
    }

    /// `BSD_ClickFav`: save the cart (when "Set Favs" is active) or load it.
    /// Returns true when favorites changed (persist settings).
    pub fn click_fav(&mut self, idx: u8, catalog: &[BrickInfo]) -> bool {
        if self.setting_favs {
            self.save_favorite(idx, catalog);
            self.setting_favs = false;
            true
        } else {
            self.load_favorite(idx, catalog);
            false
        }
    }

    /// `BSD_SaveFavorites`: by uiName; an empty cart clears the favorite.
    pub fn save_favorite(&mut self, idx: u8, catalog: &[BrickInfo]) {
        let names: Vec<String> = self
            .cart
            .iter()
            .map(|c| {
                c.and_then(|i| catalog.get(i))
                    .map(|b| b.ui_name.clone())
                    .unwrap_or_default()
            })
            .collect();
        if names.iter().all(String::is_empty) {
            self.favorites.remove(&idx);
        } else {
            self.favorites.insert(idx, names);
        }
    }

    /// `BSD_BuyFavorites`: names missing from the catalog leave empty slots.
    pub fn load_favorite(&mut self, idx: u8, catalog: &[BrickInfo]) {
        let names = self.favorites.get(&idx).cloned().unwrap_or_default();
        for (i, slot) in self.cart.iter_mut().enumerate() {
            *slot = names
                .get(i)
                .filter(|n| !n.is_empty())
                .and_then(|n| catalog.iter().position(|b| &b.ui_name == n));
        }
    }

    /// `updateFavButtons`: full alpha only when slot 0 of that favorite is set.
    pub fn favorite_filled(&self, idx: u8) -> bool {
        self.favorites
            .get(&idx)
            .and_then(|f| f.first())
            .is_some_and(|n| !n.is_empty())
    }

    /// Brick ids for `UiAction::BuyBricks`.
    pub fn purchase(&self, catalog: &[BrickInfo]) -> Vec<Option<String>> {
        self.cart
            .iter()
            .map(|c| c.and_then(|i| catalog.get(i)).map(|b| b.id.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::IconRef;

    fn cat() -> Vec<BrickInfo> {
        ["1x1", "1x2", "2x2", "1x1F", "Ramp"]
            .iter()
            .enumerate()
            .map(|(i, n)| BrickInfo {
                id: format!("b{i}"),
                ui_name: n.to_string(),
                category: if n.ends_with('F') {
                    "Plates"
                } else if *n == "Ramp" {
                    "Ramps"
                } else {
                    "Bricks"
                }
                .into(),
                subcategory: "1x".into(),
                icon: IconRef::None,
            })
            .collect()
    }

    #[test]
    fn layout_follows_catalog_order() {
        let mut input = cat();
        input[1].category = "bricks".into();
        input[1].subcategory = "1X".into();
        let l = CatalogLayout::build(&input);
        let names: Vec<&str> = l.tabs.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Bricks", "Plates", "Ramps"]);
        assert_eq!(l.tabs[0].sections[0].bricks, vec![0, 1, 2]);
    }

    #[test]
    fn click_rules() {
        let mut m = SelectorModel::default();
        m.click_brick(2);
        assert_eq!(m.cart[0], None);
        m.click_brick(2); // same tile again → first empty slot
        assert_eq!(m.cart[0], Some(2));
        m.click_brick(4);
        m.click_slot(5); // place selected brick into slot 5
        assert_eq!(m.cart[5], Some(4));
        m.click_slot(0);
        m.click_slot(5); // swap
        assert_eq!((m.cart[0], m.cart[5]), (Some(4), Some(2)));
        m.click_slot(5);
        m.click_slot(5); // same slot twice clears
        assert_eq!(m.cart[5], None);
        // Full cart with queue buying shifts left.
        for i in 0..CART_SLOTS {
            m.cart[i] = Some(i % 3);
        }
        m.click_brick(4);
        m.click_brick(4);
        assert_eq!(m.cart[9], Some(4));
        assert_eq!(m.cart[0], Some(1));
        m.queue_brick_buying = false;
        m.click_brick(3);
        m.click_brick(3);
        assert!(!m.cart.contains(&Some(3)));
    }

    #[test]
    fn favorites_by_name() {
        let c = cat();
        let mut m = SelectorModel::default();
        m.cart[0] = Some(1);
        m.cart[3] = Some(4);
        m.toggle_set_favs();
        assert_eq!(m.set_favs_label(), " Cancel ");
        assert!(m.click_fav(3, &c));
        assert!(!m.setting_favs);
        assert_eq!(m.favorites[&3][0], "1x2");
        assert!(m.favorite_filled(3));
        m.clear_cart();
        // A renamed/missing brick leaves its slot empty.
        m.favorites.get_mut(&3).unwrap()[3] = "Gone".into();
        m.click_fav(3, &c);
        assert_eq!(m.cart[0], Some(1));
        assert_eq!(m.cart[3], None);
        assert_eq!(m.purchase(&c)[0].as_deref(), Some("b1"));
        // Saving an empty cart clears the favorite.
        m.clear_cart();
        m.toggle_set_favs();
        m.click_fav(3, &c);
        assert!(!m.favorite_filled(3));
    }
}
