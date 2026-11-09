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

//! Level validation rules: grid dimensions, allowed characters, boss room clearance,
//! portal pairing, unique valid names, contiguous IDs/worlds, and ball reachability.
//!
//! The first `LEGACY_LEVEL_COUNT` levels are a frozen reference: they are never rejected by
//! the rules introduced for generated levels (portal count, density, similarity, asymmetry,
//! name suffixes, name length, world layout). Those rules apply to indices at or above it.

use kinetix_core::config::LevelDef;
pub use kinetix_core::config::MAX_LEVEL_NAME_LEN;
use kinetix_core::level_code::{EDITOR_COLS, EDITOR_ROWS};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const BOSS_CLEAR_ROWS: usize = 3;
/// Levels below this index are the legacy reference pack.
pub const LEGACY_LEVEL_COUNT: usize = 15;
/// Sectors per world in the generated layout: `world = index / SECTORS_PER_WORLD + 1`.
pub const SECTORS_PER_WORLD: usize = 100;
/// Maximum number of distinct worlds in a pack.
pub const MAX_WORLDS: usize = 9;
/// Destructible-cell band of a generated regular level (legacy measured 52..=77).
pub const DENSITY_MIN: usize = 30;
pub const DENSITY_MAX: usize = 80;
/// Destructible-cell band of a generated boss level (legacy measured 15..=19).
pub const BOSS_DENSITY_MIN: usize = 12;
pub const BOSS_DENSITY_MAX: usize = 40;
/// A generated level is rejected when more than this fraction of the union of non-empty
/// cells is identical to an earlier level of the same class (boss or regular).
pub const SIMILARITY_MAX: f32 = 0.85;
/// A generated level counts as asymmetric only when at least this many cells differ from their
/// left-right mirror counterpart (a mismatching pair counts as two cells), so a single stray
/// cell does not make a mirrored level pass as asymmetric.
pub const MIN_ASYMMETRIC_CELLS: usize = 6;
/// Minimum percentage of asymmetric (not left-right mirrored) generated levels per world.
pub const MIN_ASYMMETRIC_PERCENT: usize = 30;
/// Name suffix of a boss level.
pub const BOSS_SUFFIX: &str = " [BOSS]";
/// Name suffix of the last level of a pack that contains generated levels.
pub const FINAL_BOSS_SUFFIX: &str = " [FINAL BOSS]";
/// Longest boss stem (the name without its suffix): the room left by the longest suffix, so
/// switching between `BOSS_SUFFIX` and `FINAL_BOSS_SUFFIX` never changes the stem.
pub const BOSS_STEM_MAX_LEN: usize = MAX_LEVEL_NAME_LEN - FINAL_BOSS_SUFFIX.len();
const GRID_CELLS: usize = EDITOR_ROWS * EDITOR_COLS;
const PERCENT: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub level_id: usize,
    pub message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "level {}: {}", self.level_id, self.message)
    }
}

pub fn is_allowed_cell(ch: char) -> bool {
    matches!(ch, '0'..='6' | 'E' | '#' | 'T' | 'G' | 'M' | 'B' | 'R')
}

pub fn is_destructible_cell(ch: char) -> bool {
    matches!(ch, '1'..='6' | 'E' | 'G' | 'M' | 'B' | 'R')
}

pub fn is_valid_name_char(ch: char) -> bool {
    ch.is_ascii_uppercase() || ch.is_ascii_digit() || matches!(ch, ' ' | '-' | '[' | ']')
}

/// Checks coarse grid flood-fill reachability: from the paddle launch zone below the grid
/// and through the side/top arena corridors, a ball must be able to reach every destructible
/// brick without being completely sealed off by `#` walls.
///
/// The coarse grid is `(EDITOR_ROWS + 2) x (EDITOR_COLS + 2)` where row `0` is the top
/// corridor above row 0 of bricks, row `EDITOR_ROWS + 1` is the open bottom playfield,
/// and columns `0` and `EDITOR_COLS + 1` are the left and right side corridors between
/// `wall_side` and `BRICK_GRID_SIDE_MARGIN`.
///
/// - Empty cells (`0`), destructible bricks (`1-6`, `E`, `G`, `M`, `B`, `R`) and portals
///   (`T`) are traversable (destructible bricks are cleared as the ball hits them, opening
///   the path behind them; portals also teleport between pairs).
/// - Indestructible `#` cells block traversal.
/// - Reaching any portal opens every portal. For a generated level (exactly two portals)
///   this is exactly "the two portals connect each other"; for legacy levels with more
///   portals it stays an over-approximation, which is acceptable for a frozen reference.
pub fn check_reachability(rows: &[String]) -> Result<(), String> {
    let h = EDITOR_ROWS + 2;
    let w = EDITOR_COLS + 2;
    let mut blocked = vec![vec![false; w]; h];
    let mut portals = Vec::new();
    let mut destructibles = Vec::new();

    for (r, row) in rows.iter().enumerate() {
        for (c, ch) in row.chars().enumerate() {
            let gr = r + 1;
            let gc = c + 1;
            if gr >= h || gc >= w {
                continue;
            }
            if ch == '#' {
                blocked[gr][gc] = true;
            } else if ch == 'T' {
                portals.push((gr, gc));
            } else if is_destructible_cell(ch) {
                destructibles.push((r, c, gr, gc));
            }
        }
    }

    let mut visited = vec![vec![false; w]; h];
    let mut queue = VecDeque::new();

    // Seed from the bottom playfield row (row h - 1).
    for (gc, cell) in visited[h - 1].iter_mut().enumerate().take(w) {
        *cell = true;
        queue.push_back((h - 1, gc));
    }

    let mut portals_activated = false;
    while let Some((r, c)) = queue.pop_front() {
        for (dr, dc) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
            let nr = r as isize + dr;
            let nc = c as isize + dc;
            if nr < 0 || nr >= h as isize || nc < 0 || nc >= w as isize {
                continue;
            }
            let (nr, nc) = (nr as usize, nc as usize);
            if !visited[nr][nc] && !blocked[nr][nc] {
                visited[nr][nc] = true;
                queue.push_back((nr, nc));
                if !portals_activated && portals.contains(&(nr, nc)) {
                    portals_activated = true;
                    for &(pr, pc) in &portals {
                        if !visited[pr][pc] {
                            visited[pr][pc] = true;
                            queue.push_back((pr, pc));
                        }
                    }
                }
            }
        }
    }

    for (orig_r, orig_c, gr, gc) in destructibles {
        if !visited[gr][gc] {
            return Err(format!(
                "destructible brick at row {orig_r}, col {orig_c} is unreachable (sealed by '#')"
            ));
        }
    }
    Ok(())
}

fn cell_grid(level: &LevelDef) -> [u8; GRID_CELLS] {
    let mut grid = [b'0'; GRID_CELLS];
    for (r, row) in level.rows.iter().take(EDITOR_ROWS).enumerate() {
        for (c, ch) in row.chars().take(EDITOR_COLS).enumerate() {
            grid[r * EDITOR_COLS + c] = if ch.is_ascii() { ch as u8 } else { b'?' };
        }
    }
    grid
}

fn is_empty_cell(cell: u8) -> bool {
    cell == b'0' || cell == b' '
}

/// Equal cells over the union of non-empty cells, in `0.0..=1.0` (0 when both are empty).
pub fn cell_similarity(a: &[u8; GRID_CELLS], b: &[u8; GRID_CELLS]) -> f32 {
    let mut union = 0usize;
    let mut equal = 0usize;
    for (&x, &y) in a.iter().zip(b.iter()) {
        if is_empty_cell(x) && is_empty_cell(y) {
            continue;
        }
        union += 1;
        if x == y {
            equal += 1;
        }
    }
    if union == 0 {
        0.0
    } else {
        equal as f32 / union as f32
    }
}

/// Cell class used for the shape comparison: all brick colours (digits 1 to 6) are one class,
/// every other cell stands for itself.
fn shape_class(cell: u8) -> u8 {
    if (b'1'..=b'6').contains(&cell) {
        b'1'
    } else {
        cell
    }
}

/// The grid with every brick colour collapsed to one class (see `shape_class`).
fn shape_grid(grid: &[u8; GRID_CELLS]) -> [u8; GRID_CELLS] {
    grid.map(shape_class)
}

/// Number of cells that differ from their left-right mirror counterpart (a mismatching pair
/// counts as two cells).
pub fn mirror_mismatch_cells(level: &LevelDef) -> usize {
    level
        .rows
        .iter()
        .map(|row| {
            let cells: Vec<char> = row.chars().collect();
            cells
                .iter()
                .zip(cells.iter().rev())
                .filter(|(a, b)| a != b)
                .count()
        })
        .sum()
}

/// A level is asymmetric when at least `MIN_ASYMMETRIC_CELLS` cells differ from the mirror.
pub fn is_asymmetric(level: &LevelDef) -> bool {
    mirror_mismatch_cells(level) >= MIN_ASYMMETRIC_CELLS
}

fn push_error(errors: &mut Vec<ValidationError>, level_id: usize, message: String) {
    errors.push(ValidationError { level_id, message });
}

/// Name rules for a generated level: suffix, brackets only in the suffix, length.
fn check_generated_name(errors: &mut Vec<ValidationError>, level: &LevelDef, is_last: bool) {
    let name = level.name.as_str();
    let lid = level.id;
    if is_last && !level.boss {
        push_error(
            errors,
            lid,
            format!("name {name:?}: the last level of the pack must have boss: true"),
        );
    }
    let expected_suffix = if is_last {
        Some(FINAL_BOSS_SUFFIX)
    } else if level.boss {
        Some(BOSS_SUFFIX)
    } else {
        None
    };
    let stem = match expected_suffix {
        Some(suffix) => match name.strip_suffix(suffix) {
            Some(stem) => stem,
            None => {
                push_error(
                    errors,
                    lid,
                    format!("name {name:?} must end with {suffix:?}"),
                );
                name
            }
        },
        None => name,
    };
    if stem.contains('[') || stem.contains(']') {
        push_error(
            errors,
            lid,
            format!("name {name:?} uses '[' or ']' outside the boss suffix"),
        );
    }
    let stem_len = stem.chars().count();
    if expected_suffix.is_some() && stem_len > BOSS_STEM_MAX_LEN {
        push_error(
            errors,
            lid,
            format!("boss name {name:?} has a {stem_len} chars stem (max {BOSS_STEM_MAX_LEN})"),
        );
    }
    let len = name.chars().count();
    if len > MAX_LEVEL_NAME_LEN {
        push_error(
            errors,
            lid,
            format!("name {name:?} is {len} chars long (max {MAX_LEVEL_NAME_LEN})"),
        );
    }
}

/// How much of a pack one validation run covers.
struct Scope {
    /// First index whose per-level rules are checked (earlier levels only feed the
    /// cross-level rules: ids, duplicate names, similarity, asymmetry share).
    first_idx: usize,
    /// Whether the last level of the slice is the final boss of the finished pack.
    last_is_final: bool,
    /// Length of the finished pack, used to judge whether the asymmetry share is still
    /// reachable while a pack is being built.
    planned_total: usize,
}

/// World a level at `idx` must belong to once the pack holds generated levels: the legacy
/// reference levels are all world 1, then `world = idx / SECTORS_PER_WORLD + 1`.
fn campaign_world(idx: usize) -> usize {
    if idx < LEGACY_LEVEL_COUNT {
        1
    } else {
        idx / SECTORS_PER_WORLD + 1
    }
}

/// Validates a full campaign pack.
pub fn validate_pack(levels: &[LevelDef]) -> Vec<ValidationError> {
    validate_levels(
        levels,
        &Scope {
            first_idx: 0,
            last_is_final: true,
            planned_total: levels.len(),
        },
    )
}

/// Runs the per-level rules of [`validate_pack`] on `candidate` as if appended to `prefix`
/// (already validated), so a generator can retry a bad candidate. `is_last` marks the final
/// boss, `planned_total` is the length of the finished pack. Pack-wide rules that only make
/// sense on the finished pack (final boss name, asymmetry share) are judged for feasibility.
pub fn validate_candidate(
    prefix: &[LevelDef],
    candidate: &LevelDef,
    is_last: bool,
    planned_total: usize,
) -> Vec<ValidationError> {
    let mut pack = prefix.to_vec();
    pack.push(candidate.clone());
    validate_levels(
        &pack,
        &Scope {
            first_idx: pack.len() - 1,
            last_is_final: is_last,
            planned_total,
        },
    )
}

fn validate_levels(levels: &[LevelDef], scope: &Scope) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    if levels.is_empty() {
        push_error(&mut errors, 0, "level pack is empty".to_string());
        return errors;
    }

    let first_idx = scope.first_idx.min(levels.len() - 1);
    let mut seen_names: BTreeSet<String> =
        levels[..first_idx].iter().map(|l| l.name.clone()).collect();
    let mut expected_id = first_idx + 1;
    let mut current_world = if first_idx > 0 {
        levels[first_idx - 1].world
    } else {
        1
    };
    let last_idx = levels.len() - 1;
    // Once the pack holds generated levels, the whole pack follows the campaign layout.
    let campaign = levels.len() > LEGACY_LEVEL_COUNT;

    for (idx, level) in levels.iter().enumerate().skip(first_idx) {
        let lid = level.id;
        let generated = idx >= LEGACY_LEVEL_COUNT;
        if lid != expected_id {
            push_error(
                &mut errors,
                lid,
                format!("expected contiguous id {expected_id} at index {idx}, got {lid}"),
            );
        }
        expected_id = lid + 1;

        if campaign {
            let expected_world = campaign_world(idx);
            if level.world != expected_world {
                push_error(
                    &mut errors,
                    lid,
                    format!(
                        "world {} does not match the layout (index {idx} belongs to world {expected_world})",
                        level.world
                    ),
                );
            }
        } else if idx == 0 {
            if level.world != 1 {
                push_error(
                    &mut errors,
                    lid,
                    format!("first level must be in world 1, got {}", level.world),
                );
            }
            current_world = level.world;
        } else if level.world < current_world || level.world > current_world + 1 {
            push_error(
                &mut errors,
                lid,
                format!(
                    "world numbers must be ascending and contiguous (previous {current_world}, got {})",
                    level.world
                ),
            );
        } else {
            current_world = level.world;
        }

        let name = level.name.as_str();
        if name.trim().is_empty() {
            push_error(&mut errors, lid, "name is empty".to_string());
        }
        if generated {
            check_generated_name(&mut errors, level, scope.last_is_final && idx == last_idx);
        }
        if !name.chars().all(is_valid_name_char) {
            push_error(
                &mut errors,
                lid,
                format!(
                    "name {name:?} contains disallowed characters (allowed: A-Z, 0-9, space, -, [, ])"
                ),
            );
        }
        if !seen_names.insert(name.to_string()) {
            push_error(&mut errors, lid, format!("duplicate level name {name:?}"));
        }

        if level.rows.is_empty() || level.rows.len() > EDITOR_ROWS {
            push_error(
                &mut errors,
                lid,
                format!(
                    "row count {} out of range 1..={EDITOR_ROWS}",
                    level.rows.len()
                ),
            );
        }

        let mut destructible_count = 0usize;
        let mut portal_count = 0usize;

        for (r_idx, row) in level.rows.iter().enumerate() {
            let char_count = row.chars().count();
            if char_count != EDITOR_COLS {
                push_error(
                    &mut errors,
                    lid,
                    format!("row {r_idx} has {char_count} chars, expected {EDITOR_COLS}"),
                );
            }
            for (c_idx, ch) in row.chars().enumerate() {
                if !is_allowed_cell(ch) {
                    push_error(
                        &mut errors,
                        lid,
                        format!("invalid cell {ch:?} at row {r_idx}, col {c_idx}"),
                    );
                }
                if is_destructible_cell(ch) {
                    destructible_count += 1;
                }
                if ch == 'T' {
                    portal_count += 1;
                }
            }
        }

        if destructible_count == 0 {
            push_error(
                &mut errors,
                lid,
                "level has no destructible bricks".to_string(),
            );
        }

        if generated {
            if portal_count != 0 && portal_count != 2 {
                push_error(
                    &mut errors,
                    lid,
                    format!("generated levels need exactly 0 or 2 portals, found {portal_count}"),
                );
            }
            let (lo, hi) = if level.boss {
                (BOSS_DENSITY_MIN, BOSS_DENSITY_MAX)
            } else {
                (DENSITY_MIN, DENSITY_MAX)
            };
            if destructible_count < lo || destructible_count > hi {
                push_error(
                    &mut errors,
                    lid,
                    format!(
                        "{destructible_count} destructible cells outside the density band {lo}..={hi}"
                    ),
                );
            }
        } else if portal_count == 1 {
            // A single portal cannot teleport anywhere.
            push_error(
                &mut errors,
                lid,
                "portals must appear in pairs (even count, not 1), found 1".to_string(),
            );
        }

        if level.boss {
            if level.rows.len() < BOSS_CLEAR_ROWS {
                push_error(
                    &mut errors,
                    lid,
                    format!("boss level must have at least {BOSS_CLEAR_ROWS} rows"),
                );
            } else {
                for r_idx in 0..BOSS_CLEAR_ROWS {
                    if level.rows[r_idx].chars().any(|c| c != '0') {
                        push_error(
                            &mut errors,
                            lid,
                            format!("boss level row {r_idx} must be all '0' for boss clearance"),
                        );
                    }
                }
            }
        }

        if let Err(msg) = check_reachability(&level.rows) {
            push_error(&mut errors, lid, msg);
        }
    }

    check_world_count(&mut errors, levels);
    check_similarity(&mut errors, levels, first_idx);
    check_asymmetry(&mut errors, levels, scope.planned_total);
    errors
}

fn check_world_count(errors: &mut Vec<ValidationError>, levels: &[LevelDef]) {
    let worlds: BTreeSet<usize> = levels.iter().map(|l| l.world).collect();
    if worlds.len() > MAX_WORLDS {
        push_error(
            errors,
            0,
            format!("{} worlds in the pack (max {MAX_WORLDS})", worlds.len()),
        );
    }
}

/// Rejects a generated level too similar, on exact cells or on shape (digits 1 to 6 as one
/// class), to any earlier level of the same class; legacy
/// levels take part as reference but are never rejected themselves.
fn check_similarity(errors: &mut Vec<ValidationError>, levels: &[LevelDef], first_idx: usize) {
    if levels.len() <= LEGACY_LEVEL_COUNT {
        return;
    }
    let grids: Vec<[u8; GRID_CELLS]> = levels.iter().map(cell_grid).collect();
    let shapes: Vec<[u8; GRID_CELLS]> = grids.iter().map(shape_grid).collect();
    for idx in first_idx.max(LEGACY_LEVEL_COUNT)..levels.len() {
        for earlier in 0..idx {
            if levels[earlier].boss != levels[idx].boss {
                continue;
            }
            let sim = cell_similarity(&grids[idx], &grids[earlier]);
            let shape_sim = cell_similarity(&shapes[idx], &shapes[earlier]);
            if sim > SIMILARITY_MAX || shape_sim > SIMILARITY_MAX {
                let (kind, value) = if sim > SIMILARITY_MAX {
                    ("identical to", sim)
                } else {
                    ("same shape (colours ignored) as", shape_sim)
                };
                push_error(
                    errors,
                    levels[idx].id,
                    format!(
                        "{:.0}% {kind} level {} (max {:.0}%)",
                        value * PERCENT as f32,
                        levels[earlier].id,
                        SIMILARITY_MAX * PERCENT as f32
                    ),
                );
                break;
            }
        }
    }
}

/// Every world that contains generated levels needs a share of asymmetric ones. While a pack
/// is being built (`planned_total` beyond `levels.len()`), the share only has to remain
/// reachable if every still-missing level of that world turned out asymmetric.
fn check_asymmetry(errors: &mut Vec<ValidationError>, levels: &[LevelDef], planned_total: usize) {
    // world -> (first generated level id, generated count, asymmetric count)
    let mut per_world: BTreeMap<usize, (usize, usize, usize)> = BTreeMap::new();
    for level in levels.iter().skip(LEGACY_LEVEL_COUNT) {
        let entry = per_world.entry(level.world).or_insert((level.id, 0, 0));
        entry.1 += 1;
        if is_asymmetric(level) {
            entry.2 += 1;
        }
    }
    for (world, (first_id, total, asymmetric)) in per_world {
        let world_start = (world.saturating_sub(1) * SECTORS_PER_WORLD).max(LEGACY_LEVEL_COUNT);
        let world_end = (world * SECTORS_PER_WORLD).min(planned_total);
        let missing = world_end.saturating_sub(world_start).saturating_sub(total);
        let planned = total + missing;
        if (asymmetric + missing) * PERCENT < planned * MIN_ASYMMETRIC_PERCENT {
            push_error(
                errors,
                first_id,
                format!(
                    "world {world}: only {asymmetric}/{total} generated levels are asymmetric (min {MIN_ASYMMETRIC_PERCENT}% of {planned})"
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kinetix_core::config::LevelPack;

    const RAW_LEVELS: &str = include_str!("../../../assets/config/levels.json");
    /// Fill probability (out of 100) of the pseudo-random test patterns.
    const TEST_FILL_PERCENT: u64 = 62;

    fn legacy_pack() -> Vec<LevelDef> {
        let pack: LevelPack = serde_json::from_str(RAW_LEVELS).expect("levels.json");
        pack.levels
    }

    /// Deterministic pseudo-random rows: asymmetric, dense, no steel, no portals.
    fn random_rows(seed: u64, boss: bool) -> Vec<String> {
        let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).max(1);
        let mut next = move || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33
        };
        let (empty_rows, filled_rows) = if boss { (BOSS_CLEAR_ROWS, 3) } else { (0, 6) };
        let mut rows = vec!["0".repeat(EDITOR_COLS); empty_rows];
        for _ in 0..filled_rows {
            let row: String = (0..EDITOR_COLS)
                .map(|_| {
                    if next() % PERCENT as u64 >= TEST_FILL_PERCENT {
                        '0'
                    } else {
                        char::from_digit((next() % 4) as u32 + 1, 10).unwrap_or('1')
                    }
                })
                .collect();
            rows.push(row);
        }
        rows
    }

    fn make_level(idx: usize, name: &str, boss: bool, seed: u64) -> LevelDef {
        LevelDef {
            id: idx + 1,
            world: idx / SECTORS_PER_WORLD + 1,
            name: name.to_string(),
            boss,
            rows: random_rows(seed, boss),
        }
    }

    /// Legacy pack plus `extra` regular generated levels and a final boss.
    fn extended_pack(extra: usize) -> Vec<LevelDef> {
        let mut levels = legacy_pack();
        for level in &mut levels {
            level.world = 1;
        }
        for k in 0..extra {
            let idx = levels.len();
            levels.push(make_level(
                idx,
                &format!("TEST SECTOR {k}"),
                false,
                1000 + k as u64,
            ));
        }
        let idx = levels.len();
        levels.push(make_level(idx, "TEST FINALE [FINAL BOSS]", true, 9000));
        levels
    }

    fn has_error(errors: &[ValidationError], needle: &str) -> bool {
        errors.iter().any(|e| e.message.contains(needle))
    }

    fn sample_level(id: usize, world: usize, name: &str, rows: Vec<&str>) -> LevelDef {
        LevelDef {
            id,
            world,
            name: name.to_string(),
            boss: false,
            rows: rows.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn sealed_destructible_brick_is_rejected_by_reachability() {
        let rows = vec![
            "00000#####00000".to_string(),
            "00000#111#00000".to_string(),
            "00000#####00000".to_string(),
        ];
        assert!(check_reachability(&rows).is_err());
    }

    #[test]
    fn portal_shortcut_into_sealed_chamber_is_accepted() {
        let rows = vec![
            "T0000#####00000".to_string(),
            "00000#1T1#00000".to_string(),
            "00000#####00000".to_string(),
        ];
        assert!(check_reachability(&rows).is_ok());
    }

    #[test]
    fn valid_single_level_passes() {
        let lvl = sample_level(1, 1, "ALPHA", vec!["111222333222111"]);
        assert!(validate_pack(&[lvl]).is_empty());
    }

    #[test]
    fn shipped_legacy_pack_has_zero_errors() {
        let errors = validate_pack(&legacy_pack());
        assert!(errors.is_empty(), "{errors:#?}");
    }

    #[test]
    fn extended_pack_baseline_is_clean() {
        let errors = validate_pack(&extended_pack(3));
        assert!(errors.is_empty(), "{errors:#?}");
    }

    #[test]
    fn legacy_levels_are_never_rejected_by_new_rules() {
        // A legacy name longer than the cap (none of the shipped names is, so one is made up
        // here) and several legacy levels with more than two portals stay accepted, also
        // inside an extended pack.
        let mut legacy = legacy_pack();
        legacy[1].name = "LEGACY NAME BEYOND THE CAP".to_string();
        assert!(legacy[1].name.chars().count() > MAX_LEVEL_NAME_LEN);
        assert!(validate_pack(&legacy).is_empty());
        let mut extended = extended_pack(3);
        extended[1].name = legacy[1].name.clone();
        let errors = validate_pack(&extended);
        assert!(errors.is_empty(), "{errors:#?}");
    }

    #[test]
    fn generated_portal_count_must_be_zero_or_two() {
        let mut pack = extended_pack(3);
        pack[16].rows[0].replace_range(0..1, "T");
        assert!(has_error(&validate_pack(&pack), "exactly 0 or 2 portals"));

        let mut pack = extended_pack(3);
        pack[16].rows[0].replace_range(0..1, "T");
        pack[16].rows[0].replace_range(14..15, "T");
        assert!(!has_error(&validate_pack(&pack), "exactly 0 or 2 portals"));

        let mut pack = extended_pack(3);
        for c in [0, 1, 13, 14] {
            pack[16].rows[0].replace_range(c..c + 1, "T");
        }
        assert!(has_error(&validate_pack(&pack), "exactly 0 or 2 portals"));
    }

    #[test]
    fn generated_density_band_is_enforced() {
        let mut pack = extended_pack(3);
        pack[15].rows = vec!["111000000000000".to_string()];
        assert!(has_error(&validate_pack(&pack), "density band"));

        let mut pack = extended_pack(3);
        pack[15].rows = vec!["111111111111111".to_string(); 6];
        assert!(has_error(&validate_pack(&pack), "density band"));
    }

    #[test]
    fn near_duplicate_generated_level_is_rejected() {
        let mut pack = extended_pack(3);
        pack[16].rows = pack[15].rows.clone();
        let errors = validate_pack(&pack);
        assert!(has_error(&errors, "identical to level 16"), "{errors:#?}");

        // A single changed cell stays above the similarity cap.
        let mut pack = extended_pack(3);
        pack[16].rows = pack[15].rows.clone();
        pack[16].rows[0].replace_range(0..1, "4");
        pack[15].rows[0].replace_range(0..1, "1");
        assert!(has_error(&validate_pack(&pack), "identical to level 16"));
    }

    #[test]
    fn recoloured_near_duplicate_is_rejected_on_shape() {
        let mut pack = extended_pack(3);
        pack[16].rows = pack[15]
            .rows
            .iter()
            .map(|r| {
                r.chars()
                    .map(|ch| match ch.to_digit(10) {
                        Some(d) if (1..=6).contains(&d) => {
                            char::from_digit(d % 6 + 1, 10).unwrap_or(ch)
                        }
                        _ => ch,
                    })
                    .collect()
            })
            .collect();
        let a = cell_grid(&pack[15]);
        let b = cell_grid(&pack[16]);
        assert!(
            cell_similarity(&a, &b) <= SIMILARITY_MAX,
            "exact cells differ"
        );
        let errors = validate_pack(&pack);
        assert!(has_error(&errors, "same shape"), "{errors:#?}");
    }

    #[test]
    fn one_cell_asymmetry_is_not_counted() {
        let mut level = sample_level(20, 1, "X", vec!["111111111111111"; 3]);
        assert_eq!(mirror_mismatch_cells(&level), 0);
        assert!(!is_asymmetric(&level));
        level.rows[0].replace_range(0..1, "2");
        assert_eq!(mirror_mismatch_cells(&level), 2);
        assert!(!is_asymmetric(&level));
        level.rows[1].replace_range(0..1, "2");
        level.rows[2].replace_range(0..1, "2");
        assert_eq!(mirror_mismatch_cells(&level), MIN_ASYMMETRIC_CELLS);
        assert!(is_asymmetric(&level));
    }

    #[test]
    fn similarity_is_per_class_and_uses_the_union_of_non_empty_cells() {
        let a = [b'0'; GRID_CELLS];
        let mut b = [b'0'; GRID_CELLS];
        assert_eq!(cell_similarity(&a, &b), 0.0);
        b[0] = b'1';
        b[1] = b'1';
        let mut c = b;
        c[1] = b'2';
        // Union is 2 cells, one equal.
        assert!((cell_similarity(&b, &c) - 0.5).abs() < f32::EPSILON);

        // A regular level identical to a boss level is not a duplicate (different class).
        let mut pack = extended_pack(3);
        let boss_rows = pack[4].rows.clone();
        assert!(pack[4].boss);
        pack[15].rows = boss_rows;
        assert!(!has_error(&validate_pack(&pack), "identical to level 5"));
    }

    #[test]
    fn legacy_levels_serve_as_similarity_reference() {
        let mut pack = extended_pack(3);
        pack[15].rows = pack[0].rows.clone();
        let errors = validate_pack(&pack);
        assert!(has_error(&errors, "identical to level 1"), "{errors:#?}");
    }

    #[test]
    fn asymmetry_share_per_world_is_enforced() {
        let mut pack = extended_pack(5);
        for lvl in pack.iter_mut().skip(LEGACY_LEVEL_COUNT) {
            let mirrored: Vec<String> = lvl
                .rows
                .iter()
                .map(|r| {
                    let half: String = r.chars().take(8).collect();
                    let tail: String = half.chars().rev().skip(1).collect();
                    format!("{half}{tail}")
                })
                .collect();
            lvl.rows = mirrored;
        }
        assert!(has_error(&validate_pack(&pack), "asymmetric"));
        assert!(!has_error(&validate_pack(&extended_pack(5)), "asymmetric"));
    }

    #[test]
    fn at_most_nine_worlds() {
        let mut pack = legacy_pack();
        for level in &mut pack {
            level.world = 1;
        }
        let mut next = 0usize;
        while pack.len() < 10 * SECTORS_PER_WORLD {
            let idx = pack.len();
            pack.push(make_level(
                idx,
                &format!("W SECTOR {next}"),
                false,
                5000 + next as u64,
            ));
            next += 1;
        }
        assert!(has_error(&validate_pack(&pack), "worlds in the pack"));
        pack.truncate(9 * SECTORS_PER_WORLD);
        assert!(!has_error(&validate_pack(&pack), "worlds in the pack"));
    }

    #[test]
    fn generated_worlds_follow_the_layout() {
        let mut pack = extended_pack(3);
        pack[16].world = 2;
        assert!(has_error(
            &validate_pack(&pack),
            "does not match the layout"
        ));
    }

    #[test]
    fn legacy_worlds_with_generated_levels_are_rejected() {
        let mut pack = legacy_pack();
        assert!(pack.iter().any(|l| l.world > 1));
        let idx = pack.len();
        pack.push(make_level(idx, "TEST FINALE [FINAL BOSS]", true, 9000));
        assert!(has_error(
            &validate_pack(&pack),
            "does not match the layout"
        ));
    }

    #[test]
    fn legacy_reassigned_to_world_one_with_generated_levels_passes() {
        let errors = validate_pack(&extended_pack(3));
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn candidate_rules_run_before_the_pack_is_finished() {
        let prefix = extended_pack(2);
        let prefix = &prefix[..prefix.len() - 1];
        let total = 100;
        let idx = prefix.len();
        let good = make_level(idx, "NEXT SECTOR", false, 4242);
        let errors = validate_candidate(prefix, &good, false, total);
        assert!(errors.is_empty(), "{errors:?}");

        let mut bad = good.clone();
        bad.rows[0] = "T".repeat(EDITOR_COLS);
        assert!(has_error(
            &validate_candidate(prefix, &bad, false, total),
            "portals"
        ));
        let mut dup = good;
        dup.name = prefix[LEGACY_LEVEL_COUNT].name.clone();
        assert!(has_error(
            &validate_candidate(prefix, &dup, false, total),
            "duplicate"
        ));
    }

    #[test]
    fn boss_and_final_boss_suffixes_are_required() {
        let mut pack = extended_pack(3);
        pack[15].boss = true;
        pack[15].rows = random_rows(77, true);
        pack[15].name = "TEST BOSS".to_string();
        assert!(has_error(
            &validate_pack(&pack),
            "must end with \" [BOSS]\""
        ));
        pack[15].name = "TEST BOSS [BOSS]".to_string();
        assert!(!has_error(&validate_pack(&pack), "must end with"));

        let mut pack = extended_pack(3);
        let last = pack.len() - 1;
        pack[last].name = "TEST FINALE [BOSS]".to_string();
        assert!(has_error(&validate_pack(&pack), "[FINAL BOSS]"));
    }

    #[test]
    fn last_level_must_be_flagged_as_boss() {
        let mut pack = extended_pack(3);
        let last = pack.len() - 1;
        pack[last].boss = false;
        pack[last].rows = random_rows(9000, false);
        assert!(has_error(&validate_pack(&pack), "must have boss: true"));
    }

    #[test]
    fn generated_boss_stem_is_capped() {
        let mut pack = extended_pack(3);
        let last = pack.len() - 1;
        let long_stem = "X".repeat(BOSS_STEM_MAX_LEN + 1);
        pack[last].name = format!("{long_stem}{FINAL_BOSS_SUFFIX}");
        assert!(has_error(&validate_pack(&pack), "chars stem"));
        let ok_stem = "X".repeat(BOSS_STEM_MAX_LEN);
        pack[last].name = format!("{ok_stem}{FINAL_BOSS_SUFFIX}");
        assert!(!has_error(&validate_pack(&pack), "chars stem"));
        pack[15].boss = true;
        pack[15].rows = random_rows(77, true);
        pack[15].name = format!("{long_stem}{BOSS_SUFFIX}");
        assert!(has_error(&validate_pack(&pack), "chars stem"));
    }

    #[test]
    fn brackets_only_allowed_in_the_suffix() {
        let mut pack = extended_pack(3);
        pack[15].name = "TEST [SECTOR]".to_string();
        assert!(has_error(&validate_pack(&pack), "outside the boss suffix"));
        let mut pack = extended_pack(3);
        pack[15].name = "TEST SECTOR".to_string();
        assert!(!has_error(&validate_pack(&pack), "outside the boss suffix"));
    }

    #[test]
    fn generated_name_length_is_capped() {
        let mut pack = extended_pack(3);
        pack[15].name = "A".repeat(MAX_LEVEL_NAME_LEN + 1);
        assert!(has_error(&validate_pack(&pack), "chars long"));
        let mut pack = extended_pack(3);
        pack[15].name = "A".repeat(MAX_LEVEL_NAME_LEN);
        assert!(!has_error(&validate_pack(&pack), "chars long"));
    }

    #[test]
    fn generated_portals_must_connect_each_other() {
        // Two portals: one outside, one inside a chamber sealed by steel.
        let mut pack = extended_pack(3);
        pack[15].rows = vec![
            "T11111111111111".to_string(),
            "11111#####11111".to_string(),
            "11111#1T1#11111".to_string(),
            "11111#####11111".to_string(),
            "123412341234123".to_string(),
        ];
        assert!(!has_error(&validate_pack(&pack), "unreachable"));
        // Without a portal inside, the sealed chamber is rejected.
        pack[15].rows[2] = "11111#111#11111".to_string();
        assert!(has_error(&validate_pack(&pack), "unreachable"));
    }
}
