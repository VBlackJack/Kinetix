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
use crate::vfx::{draw_rounded_rect, fit_font_size};
use crate::{rgba, txt};
use kinetix_core::config::{LevelDef, LevelPack, ThemeDef};
use kinetix_core::i18n::I18nCatalog;
use kinetix_core::physics::GameWorld;
use macroquad::prelude::*;

/// Keys that jump straight to the n-th world of the title menu (Key1 is the first world).
pub(crate) const WORLD_SELECT_KEYS: [KeyCode; 9] = [
    KeyCode::Key1,
    KeyCode::Key2,
    KeyCode::Key3,
    KeyCode::Key4,
    KeyCode::Key5,
    KeyCode::Key6,
    KeyCode::Key7,
    KeyCode::Key8,
    KeyCode::Key9,
];

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

/// Position in `worlds` of the world owning level `level_idx` (`fallback` when unknown).
pub(crate) fn world_pos_for_level(
    worlds: &[(usize, usize)],
    levels: &[LevelDef],
    level_idx: usize,
    fallback: usize,
) -> usize {
    levels
        .get(level_idx)
        .and_then(|level| {
            worlds
                .iter()
                .rposition(|&(id, first)| id == level.world && first <= level_idx)
        })
        .unwrap_or(fallback)
}

/// A sector can be started when its index is not above the highest unlocked one.
pub(crate) fn is_sector_unlocked(sector_idx: usize, max_unlocked: usize) -> bool {
    sector_idx <= max_unlocked
}

/// A world is playable once its first sector is unlocked.
pub(crate) fn is_world_unlocked(first_level_idx: usize, max_unlocked: usize) -> bool {
    is_sector_unlocked(first_level_idx, max_unlocked)
}

/// Keeps a menu sector selection on an existing, unlocked sector.
pub(crate) fn clamp_sector(selected: usize, max_unlocked: usize, last_sector: usize) -> usize {
    selected.min(max_unlocked).min(last_sector)
}

/// Direction / jump kind for start-sector navigation in the title menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SectorNav {
    Prev,
    Next,
    PagePrev,
    PageNext,
    WorldHome,
    WorldEnd,
}

/// Pure start-sector navigation: computes the new `(selected_world_pos, selected_sector_idx)`,
/// clamped to unlocked sectors. `WorldHome` and `WorldEnd` jump to the first / last unlocked
/// sector belonging to the currently selected world.
pub(crate) fn navigate_sector(
    nav: SectorNav,
    worlds: &[(usize, usize)],
    levels: &[LevelDef],
    world_pos: usize,
    sector_idx: usize,
    max_unlocked: usize,
) -> (usize, usize) {
    if levels.is_empty() {
        return (world_pos, 0);
    }
    let last_sector = levels.len() - 1;
    let target = match nav {
        SectorNav::Next => sector_idx.saturating_add(1),
        SectorNav::Prev => sector_idx.saturating_sub(1),
        SectorNav::PageNext => sector_idx.saturating_add(MENU_SECTOR_PAGE_STEP),
        SectorNav::PagePrev => sector_idx.saturating_sub(MENU_SECTOR_PAGE_STEP),
        SectorNav::WorldHome => worlds.get(world_pos).map(|&(_, first)| first).unwrap_or(0),
        SectorNav::WorldEnd => {
            if let Some(&(_, first)) = worlds.get(world_pos) {
                let next_first = worlds
                    .get(world_pos + 1)
                    .map(|&(_, nf)| nf)
                    .unwrap_or(levels.len());
                next_first.saturating_sub(1).max(first)
            } else {
                last_sector
            }
        }
    };
    let new_sector = clamp_sector(target, max_unlocked, last_sector);
    let new_world_pos = world_pos_for_level(worlds, levels, new_sector, world_pos);
    (new_world_pos, new_sector)
}

/// Position of the next unlocked world after `from` in the given direction, wrapping around and
/// skipping locked worlds; `from` itself when no other world is unlocked.
pub(crate) fn step_unlocked_world(
    worlds: &[(usize, usize)],
    from: usize,
    forward: bool,
    max_unlocked: usize,
) -> usize {
    let count = worlds.len();
    for step in 1..count {
        let pos = if forward {
            (from + step) % count
        } else {
            (from + count - step) % count
        };
        if is_world_unlocked(worlds[pos].1, max_unlocked) {
            return pos;
        }
    }
    from
}

/// Replaces the count placeholder of a locale string with the number of worlds.
pub(crate) fn substitute_count(template: &str, count: usize) -> String {
    template.replace(COUNT_PLACEHOLDER, &count.to_string())
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

/// Geometry of the title menu world list, derived from the number of worlds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WorldRowLayout {
    pub(crate) step: f32,
    pub(crate) pill_h: f32,
    pub(crate) text_dy: f32,
    pub(crate) text_size: u16,
}

/// Fits `count` world rows between `MENU_WORLD_LIST_Y` and `MENU_WORLD_LIST_BOTTOM_Y`. While
/// they fit at full size the rows keep their reference look; past that the pills shrink down to
/// `MENU_WORLD_PILL_MIN_H` and the gaps absorb the remainder.
pub(crate) fn world_row_layout(count: usize) -> WorldRowLayout {
    let count = count.min(WORLD_SELECT_KEYS.len());
    let n = count.max(1) as f32;
    let available = MENU_WORLD_LIST_BOTTOM_Y - MENU_WORLD_LIST_Y;
    let full_needed = n * MENU_WORLD_PILL_H + (n - 1.0) * MENU_WORLD_ROW_GAP;
    let pill_h = if full_needed <= available {
        MENU_WORLD_PILL_H
    } else {
        let scale = available / full_needed;
        (MENU_WORLD_PILL_H * scale).clamp(MENU_WORLD_PILL_MIN_H, MENU_WORLD_PILL_H)
    };
    let gap = if count <= 1 {
        MENU_WORLD_ROW_GAP
    } else {
        ((available - n * pill_h) / (n - 1.0)).clamp(0.0, MENU_WORLD_ROW_GAP)
    };
    let ratio = pill_h / MENU_WORLD_PILL_H;
    let text_size = ((f32::from(MENU_WORLD_TEXT_SIZE) * ratio).round() as u16)
        .clamp(MENU_WORLD_TEXT_SIZE_FLOOR, MENU_WORLD_TEXT_SIZE);
    WorldRowLayout {
        step: pill_h + gap,
        pill_h,
        text_dy: MENU_WORLD_TEXT_DY * ratio,
        text_size,
    }
}

/// Highlighted world and start sector of the title screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TitleSelection {
    pub(crate) world_pos: usize,
    pub(crate) sector_idx: usize,
}

/// Applies this frame's navigation keys (world digits, Left / Right, Up / Down, PageUp / PageDown,
/// Home / End) to the title selection, keeping it on an existing, unlocked sector.
pub(crate) fn update_title_selection(
    worlds: &[(usize, usize)],
    levels: &[LevelDef],
    max_unlocked: usize,
    selection: TitleSelection,
) -> TitleSelection {
    let TitleSelection {
        mut world_pos,
        mut sector_idx,
    } = selection;
    if !worlds.is_empty() {
        for (n, key) in WORLD_SELECT_KEYS.iter().enumerate() {
            if is_key_pressed(*key) {
                if let Some(&(_, first_idx)) = worlds.get(n) {
                    if is_world_unlocked(first_idx, max_unlocked) {
                        world_pos = n;
                        sector_idx = first_idx;
                    }
                }
            }
        }
        if is_key_pressed(KeyCode::Left) {
            world_pos = step_unlocked_world(worlds, world_pos, false, max_unlocked);
            sector_idx = worlds[world_pos].1;
        }
        if is_key_pressed(KeyCode::Right) {
            world_pos = step_unlocked_world(worlds, world_pos, true, max_unlocked);
            sector_idx = worlds[world_pos].1;
        }
    }
    let last_sector = levels.len().saturating_sub(1);
    for (key, nav) in [
        (KeyCode::Up, SectorNav::Next),
        (KeyCode::Down, SectorNav::Prev),
        (KeyCode::PageUp, SectorNav::PageNext),
        (KeyCode::PageDown, SectorNav::PagePrev),
        (KeyCode::Home, SectorNav::WorldHome),
        (KeyCode::End, SectorNav::WorldEnd),
    ] {
        if is_key_pressed(key) {
            (world_pos, sector_idx) =
                navigate_sector(nav, worlds, levels, world_pos, sector_idx, max_unlocked);
        }
    }
    sector_idx = clamp_sector(sector_idx, max_unlocked, last_sector);
    TitleSelection {
        world_pos,
        sector_idx,
    }
}

/// True when this frame's input starts the highlighted sector: Enter, Space or a left click on an
/// unlocked sector, unless developer mode consumed the Enter key.
pub(crate) fn title_start_requested(
    sector_idx: usize,
    max_unlocked: usize,
    dev_enter_consumed: bool,
) -> bool {
    (is_key_pressed(KeyCode::Enter)
        || is_key_pressed(KeyCode::Space)
        || is_mouse_button_pressed(MouseButton::Left))
        && is_sector_unlocked(sector_idx, max_unlocked)
        && !dev_enter_consumed
}

/// Title menu selection state: the world list and the highlighted world / start sector.
pub(crate) struct MenuSelection<'a> {
    pub(crate) worlds: &'a [(usize, usize)],
    pub(crate) world_pos: usize,
    pub(crate) sector_idx: usize,
}

pub(crate) fn draw_title_menu(
    world: &GameWorld,
    levels: &LevelPack,
    theme: &ThemeDef,
    i18n: &I18nCatalog,
    font: &Font,
    selection: &MenuSelection,
) {
    let MenuSelection {
        worlds,
        world_pos: selected_world_pos,
        sector_idx: selected_sector_idx,
    } = *selection;
    let w = world.config.arena.width;
    let h = world.config.arena.height;
    draw_rectangle(0.0, 0.0, w, h, rgba(MENU_DIM_COLOR));
    let pri = rgba(theme.text_primary);
    let acc = rgba(theme.text_accent);
    let border = rgba(theme.border_color);

    // Modern Glassmorphic Modal Panel
    draw_rounded_rect(
        MENU_PANEL_X,
        MENU_PANEL_Y,
        MENU_PANEL_W,
        MENU_PANEL_H,
        MENU_PANEL_RADIUS,
        border,
    );
    draw_rounded_rect(
        MENU_PANEL_X + MENU_PANEL_BORDER,
        MENU_PANEL_Y + MENU_PANEL_BORDER,
        MENU_PANEL_W - 2.0 * MENU_PANEL_BORDER,
        MENU_PANEL_H - 2.0 * MENU_PANEL_BORDER,
        MENU_PANEL_RADIUS - MENU_PANEL_BORDER,
        rgba(MENU_PANEL_COLOR),
    );
    draw_rounded_rect(
        MENU_PANEL_X + MENU_HEADER_INSET_X,
        MENU_PANEL_Y + MENU_HEADER_INSET_Y,
        MENU_PANEL_W - 2.0 * MENU_HEADER_INSET_X,
        MENU_HEADER_H,
        MENU_HEADER_RADIUS,
        Color::new(1.0, 1.0, 1.0, 0.05),
    );

    txt(
        font,
        i18n.t("app.title"),
        MENU_PANEL_X + MENU_TITLE_DX,
        MENU_TITLE_Y,
        MENU_TITLE_SIZE,
        acc,
    );
    txt(
        font,
        i18n.t("app.subtitle"),
        MENU_PANEL_X + MENU_SUBTITLE_DX,
        MENU_SUBTITLE_Y,
        MENU_SUBTITLE_SIZE,
        pri,
    );

    draw_rounded_rect(
        MENU_PANEL_X + MENU_BEST_PILL_DX,
        MENU_BEST_PILL_Y,
        MENU_BEST_PILL_W,
        MENU_BEST_PILL_H,
        MENU_BEST_PILL_RADIUS,
        Color::new(acc.r, acc.g, acc.b, MENU_BEST_PILL_ALPHA),
    );
    let best_line = format!("{}: {:07}", i18n.t("menu.best_score"), world.high_score);
    txt(
        font,
        &best_line,
        MENU_PANEL_X + MENU_BEST_TEXT_DX,
        MENU_BEST_TEXT_Y,
        MENU_BEST_TEXT_SIZE,
        acc,
    );

    let campaigns_line = substitute_count(
        i18n.t("menu.campaigns"),
        worlds.len().clamp(1, WORLD_SELECT_KEYS.len()),
    );
    txt(
        font,
        &campaigns_line,
        MENU_PANEL_X + MENU_LABEL_DX,
        MENU_CAMPAIGNS_Y,
        MENU_LABEL_SIZE,
        pri,
    );
    let row_layout = world_row_layout(worlds.len());
    for (pos, &(world_id, first_idx)) in worlds.iter().take(WORLD_SELECT_KEYS.len()).enumerate() {
        let unlocked = is_world_unlocked(first_idx, world.max_unlocked_level);
        let y = MENU_WORLD_LIST_Y + pos as f32 * row_layout.step;
        let is_sel = selected_world_pos == pos;
        let pill_bg = if is_sel {
            Color::new(acc.r, acc.g, acc.b, MENU_PILL_SELECTED_ALPHA)
        } else {
            rgba(MENU_PILL_IDLE_COLOR)
        };
        draw_rounded_rect(
            MENU_PANEL_X + MENU_WORLD_PILL_DX,
            y,
            MENU_WORLD_PILL_W,
            row_layout.pill_h,
            MENU_WORLD_PILL_RADIUS.min(row_layout.pill_h * 0.5),
            pill_bg,
        );
        let base_col = if is_sel { acc } else { pri };
        let col = if unlocked {
            base_col
        } else {
            Color::new(base_col.r, base_col.g, base_col.b, MENU_LOCKED_ALPHA)
        };
        let prefix = if is_sel {
            MENU_SELECTED_PREFIX
        } else {
            MENU_UNSELECTED_PREFIX
        };
        let row_text = format!(
            "{}{}",
            prefix,
            world_row_label(i18n, world_id, worlds, &levels.levels, pos)
        );
        let text_x = MENU_PANEL_X + MENU_WORLD_TEXT_DX;
        let pill_right = MENU_PANEL_X + MENU_WORLD_PILL_DX + MENU_WORLD_PILL_W;
        // A locked row reserves room for its label on the right and shrinks the name to fit.
        let lock_label = i18n.t("menu.locked");
        let lock_w = if unlocked {
            0.0
        } else {
            measure_text(lock_label, Some(font), row_layout.text_size, 1.0).width + MENU_LOCK_GAP
        };
        let text_limit = pill_right - MENU_LOCK_LABEL_PAD - lock_w - text_x;
        let row_size = fit_font_size(
            |size| measure_text(&row_text, Some(font), size, 1.0).width,
            row_layout.text_size,
            MENU_WORLD_TEXT_SIZE_MIN.min(row_layout.text_size),
            text_limit,
        );
        txt(
            font,
            &row_text,
            text_x,
            y + row_layout.text_dy,
            row_size,
            col,
        );
        if !unlocked {
            let label_w = lock_w - MENU_LOCK_GAP;
            txt(
                font,
                lock_label,
                pill_right - MENU_LOCK_LABEL_PAD - label_w,
                y + row_layout.text_dy,
                row_layout.text_size,
                col,
            );
        }
    }

    let sec_def = &levels.levels[selected_sector_idx % levels.levels.len()];
    let sec_line = sector_line(i18n.t("menu.sector_select"), sec_def);
    let sec_size = fit_font_size(
        |size| measure_text(&sec_line, Some(font), size, 1.0).width,
        MENU_SECTOR_SIZE,
        MENU_SECTOR_SIZE_MIN,
        MENU_SECTOR_MAX_W,
    );
    txt(
        font,
        &sec_line,
        MENU_PANEL_X + MENU_LABEL_DX,
        MENU_SECTOR_Y,
        sec_size,
        acc,
    );

    // Glowing CTA Pill Button
    draw_rounded_rect(
        MENU_CTA_X,
        MENU_CTA_Y,
        MENU_CTA_W,
        MENU_CTA_H,
        MENU_CTA_RADIUS,
        acc,
    );
    draw_rounded_rect(
        MENU_CTA_X + MENU_CTA_BORDER,
        MENU_CTA_Y + MENU_CTA_BORDER,
        MENU_CTA_W - 2.0 * MENU_CTA_BORDER,
        MENU_CTA_H - 2.0 * MENU_CTA_BORDER,
        MENU_CTA_RADIUS - MENU_CTA_BORDER,
        rgba(MENU_CTA_COLOR),
    );
    txt(
        font,
        i18n.t("menu.play"),
        MENU_CTA_X + MENU_CTA_TEXT_DX,
        MENU_CTA_TEXT_Y,
        MENU_CTA_TEXT_SIZE,
        WHITE,
    );

    let footer = [
        i18n.t("menu.editor_btn"),
        i18n.t("menu.theme_btn"),
        i18n.t("menu.lang_btn"),
        i18n.t("menu.music_btn"),
    ]
    .join(MENU_FOOTER_SEPARATOR);
    txt(
        font,
        &footer,
        MENU_PANEL_X + MENU_FOOTER_DX,
        MENU_FOOTER_Y,
        MENU_FOOTER_SIZE,
        pri,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{catalogs, text_width};
    use crate::{RAW_EN, RAW_FONT, RAW_FR, RAW_LEVELS};
    use kinetix_core::config::MAX_LEVEL_NAME_LEN;

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
    fn nine_world_rows_fit_and_extra_worlds_are_capped() {
        let l = world_row_layout(WORLD_SELECT_KEYS.len());
        let last_bottom =
            MENU_WORLD_LIST_Y + (WORLD_SELECT_KEYS.len() - 1) as f32 * l.step + l.pill_h;
        assert!(last_bottom <= MENU_WORLD_LIST_BOTTOM_Y + 0.01);
        assert!(l.text_size <= l.pill_h as u16);
        assert_eq!(world_row_layout(40), l);
    }

    #[test]
    fn count_placeholder_is_substituted() {
        assert_eq!(substitute_count("keys [1-{count}]", 5), "keys [1-5]");
        assert_eq!(substitute_count("no placeholder", 5), "no placeholder");
    }

    #[test]
    fn locale_campaign_strings_use_the_placeholder() {
        for raw in [RAW_EN, RAW_FR] {
            let catalog = I18nCatalog::from_json_str(raw).unwrap();
            assert!(catalog.t("menu.campaigns").contains(COUNT_PLACEHOLDER));
        }
    }

    #[test]
    fn world_rows_keep_reference_look_for_three_worlds() {
        let l = world_row_layout(3);
        assert_eq!(l.pill_h, MENU_WORLD_PILL_H);
        assert_eq!(l.step, MENU_WORLD_PILL_H + MENU_WORLD_ROW_GAP);
        assert_eq!(l.text_size, MENU_WORLD_TEXT_SIZE);
        assert_eq!(l.text_dy, MENU_WORLD_TEXT_DY);
    }

    #[test]
    fn world_rows_fit_above_the_sector_line() {
        for count in 1..=WORLD_SELECT_KEYS.len() {
            let l = world_row_layout(count);
            let last_bottom = MENU_WORLD_LIST_Y + (count - 1) as f32 * l.step + l.pill_h;
            assert!(
                last_bottom <= MENU_WORLD_LIST_BOTTOM_Y + 0.01,
                "count {count}"
            );
            assert!(last_bottom < MENU_SECTOR_Y);
            assert!(l.pill_h >= MENU_WORLD_PILL_MIN_H);
            assert!(l.text_size >= MENU_WORLD_TEXT_SIZE_FLOOR);
        }
        assert!(world_row_layout(6).pill_h < MENU_WORLD_PILL_H);
        assert_eq!(world_row_layout(0), world_row_layout(1));
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
    fn sector_navigation_syncs_the_world_position() {
        let levels = [level(1, 1), level(2, 1), level(3, 2), level(4, 2)];
        let worlds = build_worlds(&levels);
        assert_eq!(world_pos_for_level(&worlds, &levels, 1, 9), 0);
        assert_eq!(world_pos_for_level(&worlds, &levels, 2, 9), 1);
        assert_eq!(world_pos_for_level(&worlds, &levels, 99, 9), 9);
    }

    #[test]
    fn fresh_profile_can_only_start_the_first_sector() {
        let max_unlocked = 0;
        assert!(is_sector_unlocked(0, max_unlocked));
        assert!(!is_sector_unlocked(1, max_unlocked));
        assert_eq!(clamp_sector(9, max_unlocked, 14), 0);
    }

    #[test]
    fn sector_clamp_respects_both_bounds() {
        assert_eq!(clamp_sector(3, 5, 14), 3);
        assert_eq!(clamp_sector(6, 5, 14), 5);
        assert_eq!(clamp_sector(20, 30, 14), 14);
        assert_eq!(clamp_sector(0, 0, 0), 0);
    }

    #[test]
    fn sector_navigation_handles_step_page_and_world_bounds() {
        let levels: Vec<LevelDef> = (0..30).map(|i| level(i + 1, (i / 10) + 1)).collect();
        let worlds = build_worlds(&levels);
        let max_unlocked = 16; // World 1 (0..=9) full, World 2 (10..=16) partial, World 3 locked

        assert_eq!(
            navigate_sector(SectorNav::Next, &worlds, &levels, 0, 0, max_unlocked),
            (0, 1)
        );
        assert_eq!(
            navigate_sector(SectorNav::Prev, &worlds, &levels, 0, 0, max_unlocked),
            (0, 0)
        );
        assert_eq!(
            navigate_sector(SectorNav::PageNext, &worlds, &levels, 0, 2, max_unlocked),
            (1, 12)
        );
        assert_eq!(
            navigate_sector(SectorNav::PageNext, &worlds, &levels, 1, 12, max_unlocked),
            (1, 16)
        );
        assert_eq!(
            navigate_sector(SectorNav::PagePrev, &worlds, &levels, 1, 15, max_unlocked),
            (0, 5)
        );
        assert_eq!(
            navigate_sector(SectorNav::WorldEnd, &worlds, &levels, 0, 3, max_unlocked),
            (0, 9)
        );
        assert_eq!(
            navigate_sector(SectorNav::WorldHome, &worlds, &levels, 0, 9, max_unlocked),
            (0, 0)
        );
        // World 2 end is clamped to max_unlocked (16 instead of 19).
        assert_eq!(
            navigate_sector(SectorNav::WorldEnd, &worlds, &levels, 1, 10, max_unlocked),
            (1, 16)
        );
        assert_eq!(
            navigate_sector(SectorNav::WorldHome, &worlds, &levels, 1, 16, max_unlocked),
            (1, 10)
        );
    }

    #[test]
    fn world_unlock_boundary_is_its_first_sector() {
        assert!(is_world_unlocked(5, 5));
        assert!(!is_world_unlocked(5, 4));
        assert!(is_world_unlocked(0, 0));
    }

    #[test]
    fn world_navigation_skips_locked_worlds() {
        let worlds = [(1, 0), (2, 5), (3, 10)];
        // Only world 1 unlocked: nothing else to move to.
        assert_eq!(step_unlocked_world(&worlds, 0, true, 4), 0);
        assert_eq!(step_unlocked_world(&worlds, 0, false, 4), 0);
        // Worlds 1 and 2 unlocked: wraps between them and never reaches world 3.
        assert_eq!(step_unlocked_world(&worlds, 0, true, 5), 1);
        assert_eq!(step_unlocked_world(&worlds, 1, true, 9), 0);
        assert_eq!(step_unlocked_world(&worlds, 0, false, 9), 1);
        // Everything unlocked: plain wrap-around.
        assert_eq!(step_unlocked_world(&worlds, 2, true, 14), 0);
        assert_eq!(step_unlocked_world(&worlds, 0, false, 14), 2);
        assert_eq!(step_unlocked_world(&[], 0, true, 0), 0);
    }

    #[test]
    fn level_pack_worlds_fit_the_world_select_keys() {
        let pack: LevelPack = serde_json::from_str(RAW_LEVELS).expect("levels");
        assert!(build_worlds(&pack.levels).len() <= WORLD_SELECT_KEYS.len());
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
        for (code, catalog) in catalogs() {
            assert!(catalog.t("menu.sector_range").contains(FIRST_PLACEHOLDER));
            assert!(catalog.t("menu.sector_range").contains(LAST_PLACEHOLDER));
            for world in 1..=WORLD_SELECT_KEYS.len() {
                let key = format!("campaigns.c{world}");
                let name = catalog.t(&key);
                assert!(
                    name == key || !name.contains('('),
                    "{code}/{key} must not embed a sector range"
                );
            }
        }
    }

    #[test]
    fn sector_line_fits_at_the_minimum_font_size() {
        let face = ttf_parser::Face::parse(RAW_FONT, 0).expect("valid font");
        let pack: LevelPack = serde_json::from_str(RAW_LEVELS).expect("levels");
        let worst_id = 999;
        let synthetic = LevelDef {
            id: worst_id,
            world: 1,
            name: "W".repeat(MAX_LEVEL_NAME_LEN),
            boss: false,
            rows: Vec::new(),
        };
        let longest_legacy = pack
            .levels
            .iter()
            .max_by_key(|l| l.name.chars().count())
            .expect("a level")
            .clone();
        // The worst case is the synthetic full-length name; it does not depend on the legacy names.
        assert_eq!(synthetic.name.chars().count(), MAX_LEVEL_NAME_LEN);
        for (code, catalog) in catalogs() {
            for level in [&synthetic, &longest_legacy] {
                let line = sector_line(catalog.t("menu.sector_select"), level);
                let width = text_width(&face, &line, MENU_SECTOR_SIZE_MIN as f32);
                assert!(
                    width <= MENU_SECTOR_MAX_W,
                    "{code}/{}: sector line is {width:.0}px, limit {MENU_SECTOR_MAX_W:.0}px",
                    level.name
                );
            }
        }
    }
}
