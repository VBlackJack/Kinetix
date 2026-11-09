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

//! The satisfaction pack: a fixed set of levels played outside the campaign.

use kinetix_core::config::LevelPack;
use kinetix_core::physics::PackRules;

use crate::play_source::PlaySource;

const SATISFACTION_JSON: &str = include_str!("../../../assets/config/satisfaction.json");

/// Parses the embedded satisfaction pack.
///
/// The asset is compiled in and checked by tests, so a parse failure is a build defect.
pub fn satisfaction_pack() -> LevelPack {
    serde_json::from_str(SATISFACTION_JSON).expect("embedded satisfaction.json must be valid")
}

/// Level names in pack order, used to preload glyphs before the first frame.
pub fn level_names(pack: &LevelPack) -> Vec<&str> {
    pack.levels.iter().map(|l| l.name.as_str()).collect()
}

/// Engine rules for the satisfaction pack: the single source of truth is `PlaySource`.
pub fn satisfaction_rules() -> PackRules {
    PlaySource::Satisfaction.pack_rules()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kinetix_core::config::LevelDef;
    use kinetix_core::level_code::{parse_level_code, EDITOR_COLS, LEVEL_CODE_PREFIX};
    use std::collections::HashSet;

    const EXPECTED_LEVEL_COUNT: usize = 5;
    const ROW_SEPARATOR: &str = "/";

    fn code_of(level: &LevelDef) -> String {
        format!("{LEVEL_CODE_PREFIX}{}", level.rows.join(ROW_SEPARATOR))
    }

    #[test]
    fn asset_parses_with_expected_level_count() {
        assert_eq!(satisfaction_pack().levels.len(), EXPECTED_LEVEL_COUNT);
    }

    #[test]
    fn ids_are_unique_and_sequential_in_world_one() {
        let pack = satisfaction_pack();
        let ids: HashSet<usize> = pack.levels.iter().map(|l| l.id).collect();
        assert_eq!(ids.len(), pack.levels.len());
        for (i, level) in pack.levels.iter().enumerate() {
            assert_eq!(level.id, i + 1);
            assert_eq!(level.world, 1);
            assert!(!level.boss);
        }
    }

    #[test]
    fn rows_have_board_width_and_valid_cells_and_a_breakable_brick() {
        for level in satisfaction_pack().levels {
            assert!(!level.rows.is_empty(), "level {} has no rows", level.id);
            for row in &level.rows {
                assert_eq!(row.chars().count(), EDITOR_COLS, "level {}", level.id);
            }
            // The level code parser rejects invalid cells and boards without a breakable brick.
            let parsed = parse_level_code(&code_of(&level))
                .unwrap_or_else(|e| panic!("level {} invalid: {e}", level.id));
            assert_eq!(parsed, level.rows);
        }
    }

    #[test]
    fn level_code_validation_rejects_bad_boards() {
        assert!(parse_level_code("KTX1:###############").is_err());
        assert!(parse_level_code("KTX1:1Z1000000000000").is_err());
    }

    #[test]
    fn names_are_non_empty_printable_ascii() {
        let pack = satisfaction_pack();
        let names = level_names(&pack);
        assert_eq!(names.len(), EXPECTED_LEVEL_COUNT);
        for name in names {
            assert!(!name.trim().is_empty());
            assert!(name.chars().all(|c| c.is_ascii_graphic() || c == ' '));
        }
    }

    #[test]
    fn rules_are_non_campaign_without_sudden_death() {
        let rules = satisfaction_rules();
        assert!(!rules.campaign);
        assert!(!rules.sudden_death);
    }
}
