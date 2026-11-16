//! Mall store directory state and its pure view model.
//!
//! Port of `app/components/dashboard/MallDirectory.tsx`. Filtering, grouping
//! and sorting live in `argus_core::mall_stores`; this module owns the small
//! UI state (query, category, halal flag) and turns the loaded store list into
//! the sections the focus dialog renders.

use argus_core::mall_stores::{count_categories, filter_stores, group_stores_by_level};
use argus_core::types::MallStore;
use rune::TextEdit;

/// UI state for one mall's directory. Keyed by mall id so switching malls
/// resets the filters, like the TS component's `key={mallId}`.
#[derive(Clone, Debug, Default)]
pub struct MallDirectoryState {
    pub mall_id: Option<String>,
    pub query: TextEdit,
    pub category: Option<String>,
    pub halal_only: bool,
}

impl MallDirectoryState {
    /// Reset filters when the focus dialog switches to a different mall.
    pub fn for_mall(&mut self, mall_id: &str) {
        if self.mall_id.as_deref() == Some(mall_id) {
            return;
        }
        self.mall_id = Some(mall_id.to_string());
        self.query = TextEdit::new("");
        self.category = None;
        self.halal_only = false;
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

pub struct StoreGroupView {
    pub label: String,
    pub stores: Vec<MallStore>,
}

pub struct DirectoryView {
    pub groups: Vec<StoreGroupView>,
    pub categories: Vec<(String, i64)>,
    pub shown: usize,
    pub total: usize,
    pub has_halal: bool,
}

/// Build the directory shown in the dialog: category counts come from the
/// unfiltered list, the groups from the filtered/sorted list. Rows are cloned
/// into the view so it owns its data (a Rune view only borrows `AppState`).
pub fn build_view(stores: &[MallStore], state: &MallDirectoryState) -> DirectoryView {
    let filter = argus_core::mall_stores::StoreFilter {
        query: &state.query.text,
        category: state.category.as_deref(),
        halal_only: state.halal_only,
    };
    let filtered: Vec<MallStore> = filter_stores(stores, &filter)
        .into_iter()
        .cloned()
        .collect();
    let groups = group_stores_by_level(&filtered)
        .into_iter()
        .map(|g| StoreGroupView {
            label: g.label,
            stores: g.stores.into_iter().cloned().collect(),
        })
        .collect();
    DirectoryView {
        groups,
        categories: count_categories(stores),
        shown: filtered.len(),
        total: stores.len(),
        has_halal: stores.iter().any(|s| s.halal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(name: &str, category: Option<&str>, level: Option<&str>, halal: bool) -> MallStore {
        MallStore {
            name: name.into(),
            category: category.map(str::to_string),
            level: level.map(str::to_string),
            unit: None,
            fnb: false,
            halal,
            website: None,
        }
    }

    #[test]
    fn switching_mall_resets_filters() {
        let mut state = MallDirectoryState::default();
        state.for_mall("osm-way-1");
        state.query = TextEdit::new("uniqlo");
        state.category = Some("Fashion".into());
        state.halal_only = true;
        state.for_mall("osm-way-1");
        assert_eq!(state.query.text, "uniqlo", "same mall keeps its filters");
        state.for_mall("osm-way-2");
        assert!(state.query.text.is_empty());
        assert!(state.category.is_none());
        assert!(!state.halal_only);
    }

    #[test]
    fn view_filters_groups_and_counts() {
        let stores = vec![
            store("Zara", Some("Fashion"), Some("L2"), false),
            store("BSB", Some("Fashion"), Some("L1"), true),
            store("Food court", Some("Food"), Some("B1"), true),
        ];
        let mut state = MallDirectoryState::default();
        let view = build_view(&stores, &state);
        assert_eq!(view.total, 3);
        assert_eq!(view.shown, 3);
        assert_eq!(view.groups.len(), 3, "grouped by level");
        assert_eq!(
            view.groups[0].label, "Basement 1",
            "levels sort basement first"
        );
        assert_eq!(view.categories.len(), 2);
        assert!(view.has_halal);

        state.query = TextEdit::new("zara");
        let view = build_view(&stores, &state);
        assert_eq!(view.shown, 1);
        assert_eq!(view.groups[0].stores[0].name, "Zara");
        assert_eq!(view.total, 3, "total stays the unfiltered count");

        state.query = TextEdit::new("");
        state.category = Some("Fashion".into());
        state.halal_only = true;
        let view = build_view(&stores, &state);
        assert_eq!(view.shown, 1);
        assert_eq!(view.groups[0].stores[0].name, "BSB");
    }
}
