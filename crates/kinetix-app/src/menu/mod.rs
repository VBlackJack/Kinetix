/*
 * Copyright 2026 Julien Bombled
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! The main menu: pure state machine, pure layout and neon rendering.
//!
//! The application fills a [`MenuModel`] and a [`MenuInput`] each frame, calls
//! [`Menu::update`], performs the returned [`MenuAction`], then calls [`draw_menu`].

mod model;
mod render;
mod state;
mod widgets;

pub use model::{LibraryEntry, LibraryId, MenuAction, MenuInput, MenuModel, SectorRow, WorldRow};
pub use render::draw_menu;
pub use state::Menu;
pub use widgets::{
    fit_to_sizes, page_layout, Activation, Align, Label, PageId, PageLayout, Role, TextItem, Tone,
    UiRect, Widget, CONTROLS_KEYS, MENU_FONT_SIZES,
};

#[cfg(test)]
mod tests {
    use super::*;
    use kinetix_core::i18n::I18nCatalog;
    use serde_json::Value;

    const RAW_EN: &str = include_str!("../../../../assets/locales/en.json");
    const RAW_FR: &str = include_str!("../../../../assets/locales/fr.json");

    /// Keys frozen for the menu v1 (every one must exist in both languages).
    const FROZEN_KEYS: &[&str] = &[
        "menu.home.button.campaign",
        "menu.home.button.satisfaction",
        "menu.home.button.editor",
        "menu.home.button.options",
        "menu.home.button.quit",
        "menu.common.button.back",
        "menu.chapters.title",
        "menu.chapters.label.progress",
        "menu.chapters.label.locked",
        "menu.sectors.title",
        "menu.sectors.label.page",
        "menu.sectors.label.locked",
        "menu.options.title",
        "menu.options.label.language",
        "menu.options.label.renderer",
        "menu.options.label.sfx",
        "menu.options.label.music",
        "menu.options.value.on",
        "menu.options.value.off",
        "menu.options.button.controls",
        "menu.controls.title",
        "menu.pause.title",
        "menu.pause.button.resume",
        "menu.pause.button.restart",
        "menu.pause.button.options",
        "menu.pause.button.quit",
        "menu.satisfaction.title",
        "menu.satisfaction.label.subtitle",
        "menu.library.title",
        "menu.library.button.new",
        "menu.library.button.play",
        "menu.library.button.delete",
        "menu.library.label.empty",
        "menu.library.label.count",
        "menu.library.label.default_name",
        "menu.library.toast.saved",
        "menu.library.toast.full",
        "menu.library.toast.deleted",
        "storage.toast.save_failed",
        "editor.library.save_hint",
    ];

    /// Characters that must never appear in locale values: typographic punctuation and spaces.
    const FORBIDDEN_CHARS: &[char] = &[
        '\u{2014}', '\u{2013}', '\u{2010}', '\u{2011}', '\u{2212}', '\u{2018}', '\u{2019}',
        '\u{201C}', '\u{201D}', '\u{00AB}', '\u{00BB}', '\u{2026}', '\u{00A0}', '\u{202F}',
        '\u{2009}', '\u{200B}', '\u{FEFF}', '\u{0153}', '\u{00E6}',
    ];

    fn catalogs() -> [(&'static str, I18nCatalog, &'static str); 2] {
        [
            ("en", I18nCatalog::from_json_str(RAW_EN).unwrap(), RAW_EN),
            ("fr", I18nCatalog::from_json_str(RAW_FR).unwrap(), RAW_FR),
        ]
    }

    fn lookup<'a>(root: &'a Value, key: &str) -> Option<&'a Value> {
        key.split('.').try_fold(root, |node, part| node.get(part))
    }

    #[test]
    fn frozen_keys_exist_in_both_languages() {
        for (lang, catalog, _) in catalogs() {
            for key in FROZEN_KEYS {
                assert_ne!(catalog.t(key), *key, "missing {key} in {lang}");
                assert!(!catalog.t(key).is_empty(), "empty {key} in {lang}");
            }
        }
    }

    #[test]
    fn every_key_used_by_the_layouts_exists_in_both_languages() {
        let mut model = MenuModel::empty();
        model.worlds.push(WorldRow {
            name: "W".to_owned(),
            unlocked: false,
            cleared: 0,
            total: 12,
            sectors: (0..12)
                .map(|i| SectorRow {
                    index: i,
                    label: "S".to_owned(),
                    unlocked: false,
                })
                .collect(),
            focus_offset: 0,
        });
        model.satisfaction_levels = vec!["A".to_owned(); 12];
        model.library = (0..12)
            .map(|id| LibraryEntry {
                id,
                name: "L".to_owned(),
            })
            .collect();
        model.quit_supported = true;
        let pages = [
            PageId::Home,
            PageId::Chapters,
            PageId::Sectors(0),
            PageId::Options,
            PageId::Controls,
            PageId::Pause,
            PageId::Satisfaction,
            PageId::Library,
        ];
        let mut keys: Vec<&'static str> = Vec::new();
        for page in pages {
            for list_page in 0..2 {
                let layout = page_layout(page, &model, list_page);
                keys.extend(layout.texts.iter().filter_map(|t| t.label.key()));
                for w in &layout.widgets {
                    keys.extend(w.label.key());
                    keys.extend(w.value.as_ref().and_then(Label::key));
                }
            }
        }
        assert!(!keys.is_empty());
        for (lang, catalog, _) in catalogs() {
            for key in &keys {
                assert_ne!(catalog.t(key), *key, "layout key {key} missing in {lang}");
            }
        }
    }

    #[test]
    fn controls_reference_covers_the_existing_controls_keys() {
        let root: Value = serde_json::from_str(RAW_EN).unwrap();
        let controls = lookup(&root, "controls")
            .and_then(Value::as_object)
            .unwrap();
        for name in controls.keys() {
            let key = format!("controls.{name}");
            assert!(CONTROLS_KEYS.contains(&key.as_str()), "{key} not listed");
        }
        assert_eq!(CONTROLS_KEYS.len(), controls.len());
    }

    #[test]
    fn new_values_use_ascii_punctuation_only() {
        for (lang, _, raw) in catalogs() {
            let root: Value = serde_json::from_str(raw).unwrap();
            for key in FROZEN_KEYS {
                let text = lookup(&root, key).and_then(Value::as_str).unwrap();
                for c in text.chars() {
                    assert!(
                        !FORBIDDEN_CHARS.contains(&c),
                        "forbidden char U+{:04X} in {key} ({lang})",
                        c as u32
                    );
                }
            }
        }
    }

    #[test]
    fn french_values_carry_accents_where_expected() {
        let catalog = I18nCatalog::from_json_str(RAW_FR).unwrap();
        assert_eq!(catalog.t("menu.home.button.editor"), "\u{00C9}DITEUR");
        assert_eq!(catalog.t("menu.chapters.label.locked"), "VERROUILL\u{00C9}");
    }

    #[test]
    fn placeholders_survive_in_both_languages() {
        for (lang, catalog, _) in catalogs() {
            assert!(
                catalog.t("menu.chapters.label.progress").contains("{done}"),
                "{lang}"
            );
            assert!(
                catalog
                    .t("menu.chapters.label.progress")
                    .contains("{total}"),
                "{lang}"
            );
            assert!(
                catalog.t("menu.sectors.title").contains("{world}"),
                "{lang}"
            );
            assert!(
                catalog.t("menu.sectors.label.page").contains("{page}"),
                "{lang}"
            );
            assert!(
                catalog.t("menu.sectors.label.page").contains("{pages}"),
                "{lang}"
            );
            assert!(
                catalog.t("menu.library.label.count").contains("{count}"),
                "{lang}"
            );
            assert!(
                catalog.t("menu.library.label.count").contains("{max}"),
                "{lang}"
            );
            assert!(
                catalog.t("menu.library.label.default_name").contains("{n}"),
                "{lang}"
            );
        }
    }

    #[test]
    fn labels_resolve_templates() {
        let catalog = I18nCatalog::from_json_str(RAW_EN).unwrap();
        let label = Label::Template {
            key: "menu.chapters.label.progress",
            args: vec![("done", "3".to_owned()), ("total", "10".to_owned())],
        };
        assert_eq!(label.resolve(&catalog), "3/10");
        assert_eq!(Label::Text("x".to_owned()).resolve(&catalog), "x");
        assert_eq!(
            Label::Key("menu.common.button.back").resolve(&catalog),
            "BACK"
        );
    }
}
