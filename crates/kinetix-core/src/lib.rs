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

pub mod config;
pub mod i18n;
pub mod level_code;
pub mod physics;

#[cfg(test)]
pub(crate) mod test_support {
    use super::config::{GameConfig, LevelDef, LevelPack};
    use super::physics::GameWorld;

    pub fn levels() -> Vec<LevelDef> {
        let raw = include_str!("../../../assets/config/levels.json");
        serde_json::from_str::<LevelPack>(raw)
            .expect("valid levels.json")
            .levels
    }

    pub fn config() -> GameConfig {
        let raw = include_str!("../../../assets/config/game_config.json");
        serde_json::from_str(raw).expect("valid game_config.json")
    }

    pub fn world() -> GameWorld {
        GameWorld::new(config(), &levels())
    }
}

#[cfg(test)]
mod tests {
    use super::config::{GameConfig, LevelPack, ThemeRegistry};
    use super::i18n::I18nCatalog;
    use super::physics::{GameWorld, PowerUpKind, SessionStatus};

    #[test]
    fn parses_configs_and_locales_without_missing_keys() {
        let cfg_raw = include_str!("../../../assets/config/game_config.json");
        let themes_raw = include_str!("../../../assets/config/themes.json");
        let levels_raw = include_str!("../../../assets/config/levels.json");
        let en_raw = include_str!("../../../assets/locales/en.json");
        let fr_raw = include_str!("../../../assets/locales/fr.json");

        let cfg: GameConfig = serde_json::from_str(cfg_raw).expect("valid game_config.json");
        let themes: ThemeRegistry = serde_json::from_str(themes_raw).expect("valid themes.json");
        let levels: LevelPack = serde_json::from_str(levels_raw).expect("valid levels.json");
        let en = I18nCatalog::from_json_str(en_raw).expect("valid en.json");
        let fr = I18nCatalog::from_json_str(fr_raw).expect("valid fr.json");

        assert_eq!(themes.themes.len(), 8);
        assert!(!levels.levels.is_empty());

        for kind in PowerUpKind::all() {
            let key = kind.i18n_key();
            assert_ne!(en.t(key), key, "Missing EN key: {}", key);
            assert_ne!(fr.t(key), key, "Missing FR key: {}", key);
            let b_key = kind.badge_key();
            assert_ne!(en.t(b_key), b_key, "Missing EN badge key: {}", b_key);
            assert_ne!(fr.t(b_key), b_key, "Missing FR badge key: {}", b_key);
        }

        let mut world = GameWorld::new(cfg, &levels.levels);
        assert_eq!(world.status, SessionStatus::Playing);
        assert_eq!(world.balls.len(), 1);
        world.apply_powerup(PowerUpKind::MultiBall);
        assert_eq!(world.balls.len(), 3);

        let exported = world.export_level_code();
        assert!(exported.starts_with("KTX1:"));
        assert!(world.import_level_code(&exported).is_ok());
    }

    fn flatten_keys(
        prefix: &str,
        value: &serde_json::Value,
        out: &mut std::collections::BTreeSet<String>,
    ) {
        match value {
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    let key = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    flatten_keys(&key, v, out);
                }
            }
            _ => {
                out.insert(prefix.to_string());
            }
        }
    }

    /// Content of the legacy levels: (name, boss flag, rows), in campaign order.
    type LegacyLevel = (&'static str, bool, &'static [&'static str]);

    #[test]
    fn legacy_levels_keep_their_content() {
        let expected: [LegacyLevel; 15] = [
            (
                "FIRST SPARK",
                false,
                &[
                    "111222333222111",
                    "1002E23332E2001",
                    "444444555444444",
                    "00333E666E33300",
                    "222200111002222",
                    "5500550E0550055",
                ],
            ),
            (
                "REACTOR CORE",
                false,
                &[
                    "##11122222111##",
                    "#333E44E44E333#",
                    "220055EEE550022",
                    "220055EEE550022",
                    "#444E33E33E444#",
                    "##11166666111##",
                ],
            ),
            (
                "OVERDRIVE MATRIX",
                false,
                &[
                    "6E601E102E203E3",
                    "666011102220333",
                    "0000###0###0000",
                    "444E555E444E555",
                    "1234561E1654321",
                    "111111111111111",
                ],
            ),
            (
                "PRISM FORTRESS",
                false,
                &[
                    "###555EEE555###",
                    "#220044E440022#",
                    "3300660E0660033",
                    "3300660E0660033",
                    "#220044E440022#",
                    "###111222111###",
                ],
            ),
            (
                "IGNITION WARDEN [BOSS]",
                true,
                &[
                    "000000000000000",
                    "000000000000000",
                    "000000000000000",
                    "##222E000E222##",
                    "111100###001111",
                ],
            ),
            (
                "PORTAL NEXUS",
                false,
                &[
                    "T00555EEE55500T",
                    "111222000222111",
                    "00#444GGG444#00",
                    "333000EEE000333",
                    "T0022211122200T",
                    "111000###000111",
                ],
            ),
            (
                "PHANTOM CARRIER",
                false,
                &[
                    "GGGGGGEEEGGGGGG",
                    "00MMM00000MMM00",
                    "222E333T333E222",
                    "00MMM00000MMM00",
                    "GGGG444T444GGGG",
                    "111222333222111",
                ],
            ),
            (
                "CONVEYOR BELT",
                false,
                &[
                    "MMMMM0EEE0MMMMM",
                    "000000###000000",
                    "T0555E666E5550T",
                    "000000###000000",
                    "MMMMM0EEE0MMMMM",
                    "222333444333222",
                ],
            ),
            (
                "EVENT HORIZON",
                false,
                &[
                    "#T#666EEE666#T#",
                    "GGG0055E5500GGG",
                    "00MMM0TTT0MMM00",
                    "444E3330333E444",
                    "GGG0022E2200GGG",
                    "1111111E1111111",
                ],
            ),
            (
                "CHRONO ARCHON [BOSS]",
                true,
                &[
                    "000000000000000",
                    "000000000000000",
                    "000000000000000",
                    "T00GGGEEEGGG00T",
                    "##MMM0###0MMM##",
                ],
            ),
            (
                "ANVIL FOUNDRY",
                false,
                &[
                    "###666EEE666###",
                    "MMM555#T#555MMM",
                    "666E4440444E666",
                    "GGG333#T#333GGG",
                    "MMM222EEE222MMM",
                    "111555666555111",
                ],
            ),
            (
                "TESLA COIL",
                false,
                &[
                    "T0E0T0E0E0T0E0T",
                    "666666###666666",
                    "00MMMMMEMMMMM00",
                    "GGGGGG###GGGGGG",
                    "555E444T444E555",
                    "222333444333222",
                ],
            ),
            (
                "SPECTRAL VAULT",
                false,
                &[
                    "#####T###T#####",
                    "#GGGGGGGGGGGGG#",
                    "#G666E555E666G#",
                    "#G555EEEE5555G#",
                    "#GGGGGGGGGGGGG#",
                    "##MMM22222MMM##",
                ],
            ),
            (
                "SINGULARITY RING",
                false,
                &[
                    "66E660TTT066E66",
                    "600060MMM060006",
                    "E0T0E0GGG0E0T0E",
                    "600060MMM060006",
                    "66E660###066E66",
                    "555555EEE555555",
                ],
            ),
            (
                "OMEGA EMPEROR [FINAL BOSS]",
                true,
                &[
                    "000000000000000",
                    "000000000000000",
                    "000000000000000",
                    "T0MMM6EEE6MMM0T",
                    "##GGG5###5GGG##",
                ],
            ),
        ];
        let levels: LevelPack =
            serde_json::from_str(include_str!("../../../assets/config/levels.json"))
                .expect("valid levels.json");
        assert!(levels.levels.len() >= expected.len());
        for (idx, (name, boss, rows)) in expected.iter().enumerate() {
            let level = &levels.levels[idx];
            assert_eq!(level.id, idx + 1, "id of legacy level {idx}");
            assert_eq!(level.name, *name, "name of legacy level {idx}");
            assert_eq!(level.boss, *boss, "boss flag of legacy level {idx}");
            assert_eq!(level.rows, *rows, "rows of legacy level {idx}");
        }
    }

    #[test]
    fn every_world_in_the_level_pack_has_a_campaign_label() {
        let levels: LevelPack =
            serde_json::from_str(include_str!("../../../assets/config/levels.json"))
                .expect("valid levels.json");
        let locales = [
            ("en", include_str!("../../../assets/locales/en.json")),
            ("fr", include_str!("../../../assets/locales/fr.json")),
        ];
        for (name, raw) in locales {
            let catalog = I18nCatalog::from_json_str(raw).expect("valid locale");
            for level in &levels.levels {
                let key = format!("campaigns.c{}", level.world);
                assert_ne!(catalog.t(&key), key, "{name}.json lacks {key}");
            }
        }
    }

    #[test]
    fn locales_have_identical_key_sets() {
        let parse = |raw: &str| {
            let root: serde_json::Value = serde_json::from_str(raw).expect("valid locale");
            let mut keys = std::collections::BTreeSet::new();
            flatten_keys("", &root, &mut keys);
            keys
        };
        let en = parse(include_str!("../../../assets/locales/en.json"));
        let fr = parse(include_str!("../../../assets/locales/fr.json"));
        let only_en: Vec<_> = en.difference(&fr).collect();
        let only_fr: Vec<_> = fr.difference(&en).collect();
        assert!(only_en.is_empty(), "keys missing from fr.json: {only_en:?}");
        assert!(only_fr.is_empty(), "keys missing from en.json: {only_fr:?}");
    }

    const UI_FONT: &[u8] = include_bytes!("../../../assets/fonts/ui_font.ttf");
    const FRENCH_ACCENTS: &str = "éèêëàâçôûùîïÉÈÊÀÂÇÔÛÙÎÏ";

    fn collect_strings(value: &serde_json::Value, out: &mut String) {
        match value {
            serde_json::Value::String(s) => out.push_str(s),
            serde_json::Value::Object(map) => map.values().for_each(|v| collect_strings(v, out)),
            serde_json::Value::Array(items) => items.iter().for_each(|v| collect_strings(v, out)),
            _ => {}
        }
    }

    #[test]
    fn ui_font_covers_every_locale_character() {
        let face = ttf_parser::Face::parse(UI_FONT, 0).expect("valid ui_font.ttf");
        let locales = [
            ("en", include_str!("../../../assets/locales/en.json")),
            ("fr", include_str!("../../../assets/locales/fr.json")),
        ];
        for (name, raw) in locales {
            let root: serde_json::Value = serde_json::from_str(raw).expect("valid locale");
            let mut text = String::new();
            collect_strings(&root, &mut text);
            let missing: Vec<char> = text
                .chars()
                .filter(|c| !c.is_whitespace() && face.glyph_index(*c).is_none())
                .collect();
            assert!(
                missing.is_empty(),
                "{name}.json uses glyphs absent from the font: {missing:?}"
            );
        }
    }

    #[test]
    fn ui_font_covers_french_accents() {
        let face = ttf_parser::Face::parse(UI_FONT, 0).expect("valid ui_font.ttf");
        for c in FRENCH_ACCENTS.chars() {
            assert!(face.glyph_index(c).is_some(), "font lacks {c}");
        }
    }
}
