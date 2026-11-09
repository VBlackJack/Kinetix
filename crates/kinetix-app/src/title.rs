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

//! Title screen: world / sector selection logic, keyboard input and drawing.

use crate::tuning::*;
use kinetix_app::library::{Library, MAX_LIBRARY_LEVELS};
use kinetix_app::menu::{
    LibraryEntry as MenuLibraryEntry, LibraryId, MenuInput, MenuModel, SectorRow, WorldRow,
};
use kinetix_core::config::LevelDef;
use kinetix_core::i18n::{I18nCatalog, Language};
use macroquad::input::{Touch, TouchPhase};
use macroquad::prelude::*;

/// Worlds in campaign order as `(world id, index of the first level)`; a new entry starts
/// whenever the world id changes along the level list.
pub(crate) fn build_worlds(levels: &[LevelDef]) -> Vec<(usize, usize)> {
    let mut worlds: Vec<(usize, usize)> = Vec::new();
    for (idx, level) in levels.iter().enumerate() {
        if worlds.last().map(|&(id, _)| id) != Some(level.world) {
            worlds.push((level.world, idx));
        }
    }
    worlds
}

/// A sector can be started when its index is not above the highest unlocked one.
pub(crate) fn is_sector_unlocked(sector_idx: usize, max_unlocked: usize) -> bool {
    sector_idx <= max_unlocked
}

/// A world is playable once its first sector is unlocked.
pub(crate) fn is_world_unlocked(first_level_idx: usize, max_unlocked: usize) -> bool {
    is_sector_unlocked(first_level_idx, max_unlocked)
}

/// Ids of the first and last sector of the world at `pos` in `worlds`.
pub(crate) fn world_sector_range(
    worlds: &[(usize, usize)],
    levels: &[LevelDef],
    pos: usize,
) -> Option<(usize, usize)> {
    let &(_, first_idx) = worlds.get(pos)?;
    let last_idx = worlds
        .get(pos + 1)
        .map_or(levels.len(), |&(_, next_first)| next_first)
        .checked_sub(1)?;
    Some((levels.get(first_idx)?.id, levels.get(last_idx)?.id))
}

/// Title menu line naming the highlighted start sector.
pub(crate) fn sector_line(label: &str, sector: &LevelDef) -> String {
    format!(
        "{label}{MENU_SECTOR_ID_PREFIX}{}{MENU_SECTOR_NAME_SEPARATOR}{}",
        sector.id, sector.name
    )
}

/// Fills the sector range placeholders of a locale string.
pub(crate) fn substitute_range(template: &str, first: usize, last: usize) -> String {
    template
        .replace(FIRST_PLACEHOLDER, &first.to_string())
        .replace(LAST_PLACEHOLDER, &last.to_string())
}

/// Label of a world row: its localized name followed by the sector range computed from the data.
pub(crate) fn world_row_label(
    i18n: &I18nCatalog,
    world_id: usize,
    worlds: &[(usize, usize)],
    levels: &[LevelDef],
    pos: usize,
) -> String {
    let key = format!("campaigns.c{world_id}");
    let name = i18n.t(&key);
    match world_sector_range(worlds, levels, pos) {
        Some((first, last)) => format!(
            "{name} {}",
            substitute_range(i18n.t("menu.sector_range"), first, last)
        ),
        None => name.to_string(),
    }
}

/// Everything `build_menu_model` needs to describe the campaign and the settings to the menu.
pub(crate) struct MenuModelSource<'a> {
    pub(crate) i18n: &'a I18nCatalog,
    pub(crate) levels: &'a [LevelDef],
    pub(crate) worlds: &'a [(usize, usize)],
    pub(crate) max_unlocked: usize,
    /// Sector started last, focused first when its world is opened again.
    pub(crate) preferred_sector: Option<usize>,
    pub(crate) satisfaction_names: &'a [String],
    pub(crate) library: &'a Library,
    pub(crate) quit_supported: bool,
    pub(crate) language: Language,
    pub(crate) renderer_name: String,
    pub(crate) sfx_on: bool,
    pub(crate) music_on: bool,
}

/// Row text of a sector: its id and name, without the title screen prefix.
pub(crate) fn sector_row_label(sector: &LevelDef) -> String {
    sector_line("", sector).trim_start().to_owned()
}

/// Offset inside a world (of `count` sectors starting at level `first`) focused when it opens:
/// the sector started last when it belongs to the world and is unlocked, else the furthest
/// unlocked sector of the world.
pub(crate) fn focus_offset(
    first: usize,
    count: usize,
    max_unlocked: usize,
    preferred: Option<usize>,
) -> usize {
    let last_offset = count.saturating_sub(1);
    if let Some(sector) = preferred {
        if sector >= first && sector - first < count && is_sector_unlocked(sector, max_unlocked) {
            return sector - first;
        }
    }
    max_unlocked.saturating_sub(first).min(last_offset)
}

/// Builds the data the menu shows from the campaign, the library and the settings.
pub(crate) fn build_menu_model(source: &MenuModelSource) -> MenuModel {
    let worlds = source
        .worlds
        .iter()
        .enumerate()
        .map(|(pos, &(world_id, first))| {
            let end = source
                .worlds
                .get(pos + 1)
                .map_or(source.levels.len(), |&(_, next_first)| next_first);
            let sectors: Vec<SectorRow> = (first..end)
                .filter_map(|index| {
                    source.levels.get(index).map(|level| SectorRow {
                        index,
                        label: sector_row_label(level),
                        unlocked: is_sector_unlocked(index, source.max_unlocked),
                    })
                })
                .collect();
            let total = sectors.len();
            WorldRow {
                name: world_row_label(source.i18n, world_id, source.worlds, source.levels, pos),
                unlocked: is_world_unlocked(first, source.max_unlocked),
                cleared: source.max_unlocked.saturating_sub(first).min(total),
                total,
                focus_offset: focus_offset(
                    first,
                    total,
                    source.max_unlocked,
                    source.preferred_sector,
                ),
                sectors,
            }
        })
        .collect();
    MenuModel {
        worlds,
        satisfaction_levels: source.satisfaction_names.to_vec(),
        library: source
            .library
            .entries
            .iter()
            .map(|entry| MenuLibraryEntry {
                id: LibraryId::from(entry.id),
                name: entry.name.clone(),
            })
            .collect(),
        library_max: MAX_LIBRARY_LEVELS,
        quit_supported: source.quit_supported,
        language: source.language,
        renderer_name: source.renderer_name.clone(),
        sfx_on: source.sfx_on,
        music_on: source.music_on,
    }
}

/// What the cached menu model depends on: the model is rebuilt only when this changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MenuModelKey {
    pub(crate) language: Language,
    pub(crate) max_unlocked: usize,
    pub(crate) preferred_sector: Option<usize>,
    pub(crate) theme_idx: usize,
    pub(crate) sfx_on: bool,
    pub(crate) music_on: bool,
    pub(crate) library_len: usize,
    pub(crate) library_next_id: u32,
}

/// The last built menu model and the key it was built for.
pub(crate) struct MenuModelCache {
    key: Option<MenuModelKey>,
    model: MenuModel,
}

impl MenuModelCache {
    pub(crate) fn new() -> Self {
        Self {
            key: None,
            model: MenuModel::empty(),
        }
    }

    /// The model for `key`, built with `build` only when `key` differs from the cached one.
    pub(crate) fn get(
        &mut self,
        key: MenuModelKey,
        build: impl FnOnce() -> MenuModel,
    ) -> &MenuModel {
        if self.key.as_ref() != Some(&key) {
            self.model = build();
            self.key = Some(key);
        }
        &self.model
    }

    /// The last model returned by `get` (empty before the first call).
    pub(crate) fn current(&self) -> &MenuModel {
        &self.model
    }
}

/// Pointer state of one frame, in screen coordinates, merged from the mouse and the touches.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct PointerFrame {
    pub(crate) position: Option<(f32, f32)>,
    pub(crate) pressed: bool,
    pub(crate) released: bool,
}

/// Merges the mouse and the touches into one pointer. The first touch, when there is one, is the
/// pointer: `Started` is a press, `Ended` a release at its position and `Cancelled` a release
/// without a position. Touch positions are physical pixels and are scaled by `1 / dpi_scale`
/// like the mouse position.
pub(crate) fn pointer_frame(
    mouse: (f32, f32),
    mouse_pressed: bool,
    mouse_released: bool,
    touches: &[Touch],
    dpi_scale: f32,
) -> PointerFrame {
    let Some(touch) = touches.first() else {
        return PointerFrame {
            position: Some(mouse),
            pressed: mouse_pressed,
            released: mouse_released,
        };
    };
    let position = (touch.position.x / dpi_scale, touch.position.y / dpi_scale);
    match touch.phase {
        TouchPhase::Started => PointerFrame {
            position: Some(position),
            pressed: true,
            released: mouse_released,
        },
        TouchPhase::Ended => PointerFrame {
            position: Some(position),
            pressed: mouse_pressed,
            released: true,
        },
        TouchPhase::Cancelled => PointerFrame {
            position: None,
            pressed: false,
            released: true,
        },
        TouchPhase::Moved | TouchPhase::Stationary => PointerFrame {
            position: Some(position),
            pressed: mouse_pressed,
            released: mouse_released,
        },
    }
}

/// Builds the menu input of the frame: keyboard edges plus the pointer, in arena coordinates.
/// Enter and Escape are dropped when developer sector entry consumed them this frame.
pub(crate) fn read_menu_input(
    pointer: PointerFrame,
    to_arena: impl Fn(f32, f32) -> (f32, f32),
    dev_enter_consumed: bool,
    dev_esc_consumed: bool,
) -> MenuInput {
    let input = MenuInput {
        pointer: pointer.position.map(|(x, y)| to_arena(x, y)),
        pointer_pressed: pointer.pressed,
        pointer_released: pointer.released,
        up: is_key_pressed(KeyCode::Up),
        down: is_key_pressed(KeyCode::Down),
        left: is_key_pressed(KeyCode::Left),
        right: is_key_pressed(KeyCode::Right),
        page_up: is_key_pressed(KeyCode::PageUp),
        page_down: is_key_pressed(KeyCode::PageDown),
        home: is_key_pressed(KeyCode::Home),
        end: is_key_pressed(KeyCode::End),
        enter: is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space),
        escape: is_key_pressed(KeyCode::Escape),
    };
    without_dev_keys(input, dev_enter_consumed, dev_esc_consumed)
}

/// Clears the keys that developer sector entry already used this frame.
pub(crate) fn without_dev_keys(
    input: MenuInput,
    dev_enter_consumed: bool,
    dev_esc_consumed: bool,
) -> MenuInput {
    MenuInput {
        enter: input.enter && !dev_enter_consumed,
        escape: input.escape && !dev_esc_consumed,
        ..input
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::catalogs;
    use crate::RAW_LEVELS;
    use kinetix_core::config::LevelPack;

    fn level(id: usize, world: usize) -> LevelDef {
        LevelDef {
            id,
            world,
            name: String::new(),
            boss: false,
            rows: Vec::new(),
        }
    }

    #[test]
    fn worlds_are_derived_from_the_level_list() {
        let levels = [
            level(1, 1),
            level(2, 1),
            level(3, 2),
            level(4, 2),
            level(5, 5),
        ];
        assert_eq!(build_worlds(&levels), vec![(1, 0), (2, 2), (5, 4)]);
        assert!(build_worlds(&[]).is_empty());
    }

    #[test]
    fn fresh_profile_can_only_start_the_first_sector() {
        let max_unlocked = 0;
        assert!(is_sector_unlocked(0, max_unlocked));
        assert!(!is_sector_unlocked(1, max_unlocked));
    }

    #[test]
    fn world_unlock_boundary_is_its_first_sector() {
        assert!(is_world_unlocked(5, 5));
        assert!(!is_world_unlocked(5, 4));
        assert!(is_world_unlocked(0, 0));
    }

    #[test]
    fn world_rows_show_a_localized_sector_range_computed_from_the_data() {
        let levels = [
            level(1, 1),
            level(2, 1),
            level(3, 2),
            level(4, 2),
            level(5, 2),
        ];
        let worlds = build_worlds(&levels);
        assert_eq!(world_sector_range(&worlds, &levels, 0), Some((1, 2)));
        assert_eq!(world_sector_range(&worlds, &levels, 1), Some((3, 5)));
        assert_eq!(world_sector_range(&worlds, &levels, 2), None);
        let [(_, en), (_, fr)] = catalogs();
        assert_eq!(
            world_row_label(&en, 2, &worlds, &levels, 1),
            "WORLD 2: QUANTUM WARP (Sectors 3-5)"
        );
        assert_eq!(
            world_row_label(&fr, 1, &worlds, &levels, 0),
            "MONDE 1 : GRILLE D'IGNITION (Secteurs 1-2)"
        );
    }

    #[test]
    fn locale_world_names_carry_no_sector_range() {
        let pack: LevelPack = serde_json::from_str(RAW_LEVELS).expect("levels");
        for (code, catalog) in catalogs() {
            assert!(catalog.t("menu.sector_range").contains(FIRST_PLACEHOLDER));
            assert!(catalog.t("menu.sector_range").contains(LAST_PLACEHOLDER));
            for world in 1..=build_worlds(&pack.levels).len() {
                let key = format!("campaigns.c{world}");
                let name = catalog.t(&key);
                assert!(
                    name == key || !name.contains('('),
                    "{code}/{key} must not embed a sector range"
                );
            }
        }
    }

    fn named_level(id: usize, world: usize, name: &str) -> LevelDef {
        LevelDef {
            name: name.to_owned(),
            ..level(id, world)
        }
    }

    fn model_for(max_unlocked: usize, preferred: Option<usize>, library: &Library) -> MenuModel {
        let levels = [
            named_level(1, 1, "ALPHA"),
            named_level(2, 1, "BETA"),
            named_level(3, 2, "GAMMA"),
            named_level(4, 2, "DELTA"),
            named_level(5, 2, "OMEGA"),
        ];
        let worlds = build_worlds(&levels);
        let [(_, en), _] = catalogs();
        let names = vec!["PROTO".to_owned()];
        build_menu_model(&MenuModelSource {
            i18n: &en,
            levels: &levels,
            worlds: &worlds,
            max_unlocked,
            preferred_sector: preferred,
            satisfaction_names: &names,
            library,
            quit_supported: true,
            language: Language::Fr,
            renderer_name: "R".to_owned(),
            sfx_on: false,
            music_on: true,
        })
    }

    #[test]
    fn menu_model_lists_worlds_sectors_and_progress() {
        let model = model_for(3, None, &Library::default());
        assert_eq!(model.worlds.len(), 2);
        let first = &model.worlds[0];
        assert_eq!((first.total, first.cleared), (2, 2));
        assert!(first.unlocked);
        assert_eq!(first.sectors[0].index, 0);
        assert_eq!(first.sectors[0].label, "#1 - ALPHA");
        let second = &model.worlds[1];
        assert_eq!((second.total, second.cleared), (3, 1));
        assert_eq!(second.sectors[0].index, 2);
        let unlocked: Vec<bool> = second.sectors.iter().map(|s| s.unlocked).collect();
        assert_eq!(unlocked, [true, true, false]);
        assert_eq!(model.satisfaction_levels, ["PROTO"]);
        assert!(model.quit_supported && !model.sfx_on && model.music_on);
        assert_eq!(model.language, Language::Fr);
        assert_eq!(model.renderer_name, "R");
        assert_eq!(model.library_max, MAX_LIBRARY_LEVELS);
    }

    #[test]
    fn a_fresh_profile_only_unlocks_the_first_sector_and_world() {
        let model = model_for(0, None, &Library::default());
        assert!(model.worlds[0].unlocked);
        assert!(!model.worlds[1].unlocked);
        assert_eq!(model.worlds[0].cleared, 0);
        assert_eq!(model.worlds[1].cleared, 0);
        assert_eq!(
            model.worlds[0]
                .sectors
                .iter()
                .filter(|s| s.unlocked)
                .count(),
            1
        );
    }

    #[test]
    fn library_ids_are_widened_at_the_boundary() {
        let mut library = Library::default();
        let id = library.add("MINE".to_owned(), "KTX1".to_owned()).unwrap();
        let model = model_for(0, None, &library);
        assert_eq!(model.library.len(), 1);
        assert_eq!(model.library[0].id, LibraryId::from(id));
        assert_eq!(model.library[0].name, "MINE");
    }

    #[test]
    fn focus_offset_prefers_the_last_started_sector_then_the_furthest_unlocked() {
        // World of 5 sectors starting at level 10.
        assert_eq!(focus_offset(10, 5, 12, None), 2);
        assert_eq!(focus_offset(10, 5, 99, None), 4);
        assert_eq!(focus_offset(10, 5, 5, None), 0);
        assert_eq!(focus_offset(10, 5, 14, Some(11)), 1);
        // A preferred sector outside the world, locked, or past the end is ignored.
        assert_eq!(focus_offset(10, 5, 12, Some(3)), 2);
        assert_eq!(focus_offset(10, 5, 12, Some(14)), 2);
        assert_eq!(focus_offset(10, 5, 14, Some(15)), 4);
        assert_eq!(focus_offset(10, 0, 14, None), 0);
    }

    #[test]
    fn the_model_cache_rebuilds_only_when_the_key_changes() {
        let key = |max_unlocked| MenuModelKey {
            language: Language::En,
            max_unlocked,
            preferred_sector: None,
            theme_idx: 0,
            sfx_on: true,
            music_on: true,
            library_len: 0,
            library_next_id: 1,
        };
        let mut cache = MenuModelCache::new();
        assert!(cache.current().worlds.is_empty());
        let mut builds = 0;
        for max_unlocked in [0, 0, 1, 1, 0] {
            cache.get(key(max_unlocked), || {
                builds += 1;
                let mut model = MenuModel::empty();
                model.language = Language::Fr;
                model
            });
        }
        assert_eq!(builds, 3);
        assert_eq!(cache.current().language, Language::Fr);
    }

    fn touch(phase: TouchPhase, x: f32, y: f32) -> Touch {
        Touch {
            id: 1,
            phase,
            position: Vec2::new(x, y),
        }
    }

    #[test]
    fn without_a_touch_the_mouse_is_the_pointer() {
        let frame = pointer_frame((10.0, 20.0), true, false, &[], 2.0);
        assert_eq!(
            frame,
            PointerFrame {
                position: Some((10.0, 20.0)),
                pressed: true,
                released: false,
            }
        );
    }

    #[test]
    fn a_touch_start_is_a_press_and_a_touch_end_a_release_at_its_position() {
        let started = pointer_frame(
            (0.0, 0.0),
            false,
            false,
            &[touch(TouchPhase::Started, 200.0, 100.0)],
            2.0,
        );
        assert_eq!(started.position, Some((100.0, 50.0)));
        assert!(started.pressed && !started.released);
        let ended = pointer_frame(
            (0.0, 0.0),
            false,
            false,
            &[touch(TouchPhase::Ended, 300.0, 80.0)],
            2.0,
        );
        assert_eq!(ended.position, Some((150.0, 40.0)));
        assert!(!ended.pressed && ended.released);
        let moved = pointer_frame(
            (0.0, 0.0),
            false,
            false,
            &[touch(TouchPhase::Moved, 40.0, 40.0)],
            1.0,
        );
        assert_eq!(moved.position, Some((40.0, 40.0)));
        assert!(!moved.pressed && !moved.released);
    }

    #[test]
    fn a_cancelled_touch_releases_without_a_position() {
        let frame = pointer_frame(
            (5.0, 5.0),
            true,
            false,
            &[touch(TouchPhase::Cancelled, 1.0, 1.0)],
            1.0,
        );
        assert_eq!(frame.position, None);
        assert!(!frame.pressed && frame.released);
    }

    #[test]
    fn only_the_first_touch_drives_the_pointer() {
        let touches = [
            touch(TouchPhase::Started, 10.0, 10.0),
            touch(TouchPhase::Ended, 99.0, 99.0),
        ];
        let frame = pointer_frame((0.0, 0.0), false, false, &touches, 1.0);
        assert_eq!(frame.position, Some((10.0, 10.0)));
        assert!(frame.pressed && !frame.released);
    }

    #[test]
    fn developer_entry_consumes_enter_and_escape_only() {
        let input = MenuInput {
            enter: true,
            escape: true,
            up: true,
            ..MenuInput::default()
        };
        let kept = without_dev_keys(input, false, false);
        assert!(kept.enter && kept.escape && kept.up);
        let no_enter = without_dev_keys(input, true, false);
        assert!(!no_enter.enter && no_enter.escape && no_enter.up);
        let no_escape = without_dev_keys(input, false, true);
        assert!(no_escape.enter && !no_escape.escape && no_escape.up);
    }
}
