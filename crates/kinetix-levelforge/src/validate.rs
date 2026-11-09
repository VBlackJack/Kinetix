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

use crate::simulate::SECTORS_PER_BLOCK;
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
/// Minimum percentage of exactly left-right mirror-symmetric regular generated levels per world.
pub const MIN_SYMMETRIC_PERCENT: usize = 30;
/// A connected component of at most this many non-empty cells is "dust" (a stray fragment).
pub const DUST_MAX_CELLS: usize = 3;
/// Dust components allowed in a regular generated level. Legacy measurement (12 regular
/// levels, components of at most `DUST_MAX_CELLS` cells, 8-connectivity): max 0, median 0 (measured 2026-11-09).
pub const MAX_DUST: usize = 0;
/// 8-connected components of non-empty cells allowed in a regular generated level. Legacy
/// measurement: max 3, median 1 (measured 2026-11-09).
pub const MAX_COMPONENTS_8: usize = 4;
/// Colour regions (8-connected same-character regions) per non-empty cell allowed in a regular
/// generated level. Legacy measurement: max 0.390, median 0.32 (measured 2026-11-09; the rejected noisy
/// candidate pack has a median of about 0.55).
pub const MAX_COLOUR_REGIONS_PER_CELL: f32 = 0.42;
/// Widest run of fully empty columns allowed inside the bounding box of an asymmetric regular
/// generated level (the widest gap of a hand-authored motif is 3): a wider one reads as two
/// unrelated pieces.
pub const MAX_INNER_GAP_COLS: usize = 3;
/// Tallest run of fully empty rows allowed inside the bounding box of an asymmetric regular
/// generated level. Measured 2026-11-09: no legacy level, no motif and none of the 471 asymmetric
/// regular levels of the previous pack has an empty row inside its shape (max 0); one empty row
/// still reads as a deliberate band break, two split a 5..=9 row shape into two pieces.
pub const MAX_INNER_GAP_ROWS: usize = 1;
/// Smallest share of the shape cells that the largest 8-connected piece of a regular generated
/// level must hold (symmetric or asymmetric). Measured 2026-11-09 with [`main_piece_share`]: the
/// 15 legacy levels have min 0.828, p10 0.840, p50 1.000; the previous 769 generated regular
/// levels had 126 of 471 asymmetric and 63 of 298 symmetric levels below 0.80 (and 110 and 51
/// below 0.70), which read as two separate pieces. 0.80 is the legacy minimum minus a small
/// margin. Bosses and the legacy levels are exempt.
pub const MIN_MAIN_PIECE_SHARE: f32 = 0.80;
/// A non-empty row string may appear in at most this many generated levels (about 2% of 800);
/// a row whose non-empty cells are one char in one contiguous run is exempt.
pub const MAX_ROW_REUSE: usize = 16;
/// Generated names must not start with this word, except the level carrying
/// `FINAL_BOSS_SUFFIX`.
pub const RESERVED_NAME_PREFIX: &str = "FINAL";
/// Debut world of each mechanic cell: the first generated level using it must be a regular
/// level in the first `SECTORS_PER_BLOCK` sectors of that world, and none may use it earlier.
pub const MECHANIC_DEBUTS: [(char, usize); 5] = [('G', 2), ('M', 3), ('T', 4), ('B', 5), ('R', 6)];
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

/// A level is exactly mirror-symmetric when no cell differs from its left-right counterpart.
pub fn is_mirror_symmetric(level: &LevelDef) -> bool {
    !level.rows.is_empty() && mirror_mismatch_cells(level) == 0
}

/// Cells that take part in the shape metrics: everything except empty cells and portals.
fn is_shape_cell(ch: char) -> bool {
    !matches!(ch, '0' | 'T' | ' ')
}

/// Sizes of the 8-connected regions of shape cells; two neighbouring shape cells share a
/// region when `joins` accepts their chars.
fn region_sizes(rows: &[String], joins: fn(char, char) -> bool) -> Vec<usize> {
    let grid: Vec<Vec<char>> = rows.iter().map(|r| r.chars().collect()).collect();
    let mut seen: Vec<Vec<bool>> = grid.iter().map(|r| vec![false; r.len()]).collect();
    let mut sizes = Vec::new();
    for r0 in 0..grid.len() {
        for c0 in 0..grid[r0].len() {
            if seen[r0][c0] || !is_shape_cell(grid[r0][c0]) {
                continue;
            }
            seen[r0][c0] = true;
            let mut stack = vec![(r0, c0)];
            let mut size = 0usize;
            while let Some((r, c)) = stack.pop() {
                size += 1;
                for dr in -1isize..=1 {
                    for dc in -1isize..=1 {
                        let nr = r as isize + dr;
                        let nc = c as isize + dc;
                        if (dr == 0 && dc == 0) || nr < 0 || nc < 0 {
                            continue;
                        }
                        let (nr, nc) = (nr as usize, nc as usize);
                        if nr >= grid.len()
                            || nc >= grid[nr].len()
                            || seen[nr][nc]
                            || !is_shape_cell(grid[nr][nc])
                            || !joins(grid[r][c], grid[nr][nc])
                        {
                            continue;
                        }
                        seen[nr][nc] = true;
                        stack.push((nr, nc));
                    }
                }
            }
            sizes.push(size);
        }
    }
    sizes
}

/// `(dust, components)` of a grid: 8-connected components of non-empty cells (every char except
/// `0` and `T`), of which dust is the number holding at most `DUST_MAX_CELLS` cells.
pub fn dust_and_components(rows: &[String]) -> (usize, usize) {
    let sizes = region_sizes(rows, |_, _| true);
    let dust = sizes.iter().filter(|&&s| s <= DUST_MAX_CELLS).count();
    (dust, sizes.len())
}

/// Share of the shape cells held by the largest 8-connected component of shape cells (see
/// `is_shape_cell`); 1 for a grid without shape cells.
pub fn main_piece_share(rows: &[String]) -> f32 {
    let sizes = region_sizes(rows, |_, _| true);
    let total: usize = sizes.iter().sum();
    match sizes.iter().max() {
        Some(&largest) if total > 0 => largest as f32 / total as f32,
        _ => 1.0,
    }
}

/// Longest run of values `false` strictly between the first and the last `true`.
fn longest_inner_false_run(occupied: &[bool]) -> usize {
    let (Some(first), Some(last)) = (
        occupied.iter().position(|&o| o),
        occupied.iter().rposition(|&o| o),
    ) else {
        return 0;
    };
    let mut best = 0usize;
    let mut run = 0usize;
    for &o in &occupied[first..=last] {
        run = if o { 0 } else { run + 1 };
        best = best.max(run);
    }
    best
}

/// Longest run of fully empty columns inside the bounding box of the shape cells (every char
/// shape cells (see `is_shape_cell`).
pub fn inner_gap_cols(rows: &[String]) -> usize {
    let width = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0);
    let occupied: Vec<bool> = (0..width)
        .map(|c| {
            rows.iter()
                .any(|row| row.chars().nth(c).is_some_and(is_shape_cell))
        })
        .collect();
    longest_inner_false_run(&occupied)
}

/// Longest run of fully empty rows inside the bounding box of the shape cells (every char
/// shape cells (see `is_shape_cell`).
pub fn inner_gap_rows(rows: &[String]) -> usize {
    let occupied: Vec<bool> = rows
        .iter()
        .map(|row| row.chars().any(is_shape_cell))
        .collect();
    longest_inner_false_run(&occupied)
}

/// Colour regions (8-connected same-char regions of non-empty cells) divided by the number of
/// non-empty cells; 0 for a grid without non-empty cells.
pub fn colour_regions_per_cell(rows: &[String]) -> f32 {
    let cells = rows
        .iter()
        .flat_map(|r| r.chars())
        .filter(|&c| is_shape_cell(c))
        .count();
    if cells == 0 {
        return 0.0;
    }
    region_sizes(rows, |a, b| a == b).len() as f32 / cells as f32
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
    if name.chars().last().is_some_and(|c| c.is_ascii_digit()) {
        push_error(
            errors,
            lid,
            format!("name {name:?} must not end with a digit"),
        );
    }
    if name.starts_with(RESERVED_NAME_PREFIX) && !name.ends_with(FINAL_BOSS_SUFFIX) {
        push_error(
            errors,
            lid,
            format!(
                "name {name:?} must not start with {RESERVED_NAME_PREFIX:?} (reserved for the final boss)"
            ),
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

        if generated && !level.boss {
            let (dust, components) = dust_and_components(&level.rows);
            if dust > MAX_DUST {
                push_error(
                    &mut errors,
                    lid,
                    format!(
                        "{dust} dust fragments (components of at most {DUST_MAX_CELLS} cells), max {MAX_DUST}"
                    ),
                );
            }
            if components > MAX_COMPONENTS_8 {
                push_error(
                    &mut errors,
                    lid,
                    format!("{components} connected components, max {MAX_COMPONENTS_8}"),
                );
            }
            let main_share = main_piece_share(&level.rows);
            if main_share < MIN_MAIN_PIECE_SHARE {
                push_error(
                    &mut errors,
                    lid,
                    format!(
                        "main piece holds {main_share:.2} of the shape, min {MIN_MAIN_PIECE_SHARE}"
                    ),
                );
            }
            if is_asymmetric(level) {
                let gap_cols = inner_gap_cols(&level.rows);
                if gap_cols > MAX_INNER_GAP_COLS {
                    push_error(
                        &mut errors,
                        lid,
                        format!(
                            "asymmetric level reads as separate pieces: {gap_cols} empty columns inside the shape, max {MAX_INNER_GAP_COLS}"
                        ),
                    );
                }
                let gap_rows = inner_gap_rows(&level.rows);
                if gap_rows > MAX_INNER_GAP_ROWS {
                    push_error(
                        &mut errors,
                        lid,
                        format!(
                            "asymmetric level reads as separate pieces: {gap_rows} empty rows inside the shape, max {MAX_INNER_GAP_ROWS}"
                        ),
                    );
                }
            }
            let regions = colour_regions_per_cell(&level.rows);
            if regions > MAX_COLOUR_REGIONS_PER_CELL {
                push_error(
                    &mut errors,
                    lid,
                    format!(
                        "colour fragmentation {regions:.2} regions per cell, max {MAX_COLOUR_REGIONS_PER_CELL}"
                    ),
                );
            }
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
    check_row_reuse(&mut errors, levels, first_idx);
    check_mechanic_order(&mut errors, levels, first_idx);
    errors
}

/// Whether a row is exempt from the reuse cap: blank, or its shape cells (portals ignored, as in
/// the shape metrics) are one char in one contiguous run.
fn is_reuse_exempt(row: &str) -> bool {
    let cells: Vec<char> = row.chars().collect();
    let is_set = |c: &char| is_shape_cell(*c);
    let Some(start) = cells.iter().position(is_set) else {
        return true;
    };
    let end = cells.iter().rposition(is_set).unwrap_or(start);
    cells[start..=end].iter().all(|&c| c == cells[start])
}

/// A non-exempt row string may appear in at most `MAX_ROW_REUSE` generated levels.
fn check_row_reuse(errors: &mut Vec<ValidationError>, levels: &[LevelDef], first_idx: usize) {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for level in levels.iter().skip(LEGACY_LEVEL_COUNT) {
        let distinct: BTreeSet<&str> = level
            .rows
            .iter()
            .map(String::as_str)
            .filter(|r| !is_reuse_exempt(r))
            .collect();
        for row in distinct {
            *counts.entry(row).or_default() += 1;
        }
    }
    for level in levels.iter().skip(first_idx.max(LEGACY_LEVEL_COUNT)) {
        let worst = level
            .rows
            .iter()
            .filter(|r| !is_reuse_exempt(r))
            .map(|r| (r, counts.get(r.as_str()).copied().unwrap_or(0)))
            .find(|&(_, n)| n > MAX_ROW_REUSE);
        if let Some((row, n)) = worst {
            push_error(
                errors,
                level.id,
                format!("row {row:?} is reused by {n} generated levels (max {MAX_ROW_REUSE})"),
            );
        }
    }
}

/// Mechanic teaching order over generated levels (see `MECHANIC_DEBUTS`): none before its
/// debut world, the first use is a regular level in the first block of the debut world, and a
/// complete debut world must contain it.
fn check_mechanic_order(errors: &mut Vec<ValidationError>, levels: &[LevelDef], first_idx: usize) {
    for &(mechanic, debut) in &MECHANIC_DEBUTS {
        let mut first_use: Option<usize> = None;
        for (idx, level) in levels.iter().enumerate().skip(LEGACY_LEVEL_COUNT) {
            if !level.rows.iter().any(|r| r.contains(mechanic)) {
                continue;
            }
            let world = campaign_world(idx);
            if idx >= first_idx && world < debut {
                push_error(
                    errors,
                    level.id,
                    format!("mechanic {mechanic:?} is used before its debut world {debut}"),
                );
            }
            if first_use.is_none() {
                first_use = Some(idx);
                let sector = idx - (world - 1) * SECTORS_PER_WORLD;
                if idx >= first_idx && world == debut {
                    if level.boss {
                        push_error(
                            errors,
                            level.id,
                            format!("mechanic {mechanic:?} is introduced by a boss level"),
                        );
                    } else if sector >= SECTORS_PER_BLOCK {
                        push_error(
                            errors,
                            level.id,
                            format!(
                                "mechanic {mechanic:?} is introduced at sector {}, after the first {SECTORS_PER_BLOCK} of world {debut}",
                                sector + 1
                            ),
                        );
                    }
                } else if idx >= first_idx && world > debut {
                    push_error(
                        errors,
                        level.id,
                        format!(
                            "mechanic {mechanic:?} first appears in world {world}, after its debut world {debut}"
                        ),
                    );
                }
            }
        }
        let debut_end = debut * SECTORS_PER_WORLD;
        let introduced = first_use.is_some_and(|idx| campaign_world(idx) <= debut);
        if levels.len() >= debut_end && !introduced && debut_end > first_idx {
            push_error(
                errors,
                levels[debut_end - 1].id,
                format!("mechanic {mechanic:?} never appears in its debut world {debut}"),
            );
        }
    }
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

/// Smallest count that reaches `percent` of `total`.
fn share_needed(total: usize, percent: usize) -> usize {
    (total * percent).div_ceil(PERCENT)
}

/// Per-world tallies of the generated levels, used by [`check_asymmetry`].
struct WorldCounts {
    first_id: usize,
    total: usize,
    regular: usize,
    asymmetric: usize,
    symmetric: usize,
}

/// Every world that contains generated levels needs a share of asymmetric levels and a share of
/// exactly mirror-symmetric regular levels. While a pack is being built (`planned_total` beyond
/// `levels.len()`), the shares only have to remain reachable together: the levels still missing
/// from the world must be able to cover both shortfalls (an upper bound that assumes every
/// missing level is a regular one).
fn check_asymmetry(errors: &mut Vec<ValidationError>, levels: &[LevelDef], planned_total: usize) {
    let mut per_world: BTreeMap<usize, WorldCounts> = BTreeMap::new();
    for level in levels.iter().skip(LEGACY_LEVEL_COUNT) {
        let entry = per_world.entry(level.world).or_insert(WorldCounts {
            first_id: level.id,
            total: 0,
            regular: 0,
            asymmetric: 0,
            symmetric: 0,
        });
        entry.total += 1;
        if is_asymmetric(level) {
            entry.asymmetric += 1;
        }
        if !level.boss {
            entry.regular += 1;
            if is_mirror_symmetric(level) {
                entry.symmetric += 1;
            }
        }
    }
    for (world, counts) in per_world {
        let world_start = (world.saturating_sub(1) * SECTORS_PER_WORLD).max(LEGACY_LEVEL_COUNT);
        let world_end = (world * SECTORS_PER_WORLD).min(planned_total);
        let missing = world_end
            .saturating_sub(world_start)
            .saturating_sub(counts.total);
        let planned = counts.total + missing;
        let planned_regular = counts.regular + missing;
        let need_asymmetric =
            share_needed(planned, MIN_ASYMMETRIC_PERCENT).saturating_sub(counts.asymmetric);
        let need_symmetric =
            share_needed(planned_regular, MIN_SYMMETRIC_PERCENT).saturating_sub(counts.symmetric);
        if need_asymmetric > missing {
            push_error(
                errors,
                counts.first_id,
                format!(
                    "world {world}: only {}/{} generated levels are asymmetric (min {MIN_ASYMMETRIC_PERCENT}% of {planned})",
                    counts.asymmetric, counts.total
                ),
            );
        }
        if need_symmetric > missing {
            push_error(
                errors,
                counts.first_id,
                format!(
                    "world {world}: only {}/{} regular levels are exactly mirror-symmetric (min {MIN_SYMMETRIC_PERCENT}% of {planned_regular})",
                    counts.symmetric, counts.regular
                ),
            );
        }
        if need_asymmetric <= missing
            && need_symmetric <= missing
            && need_asymmetric + need_symmetric > missing
        {
            push_error(
                errors,
                counts.first_id,
                format!(
                    "world {world}: {need_symmetric} more symmetric and {need_asymmetric} more asymmetric levels are needed but only {missing} slots remain"
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

    /// Levels per world in the shipped legacy layout.
    const LEGACY_LEVELS_PER_WORLD: usize = 5;

    /// The shipped legacy reference levels with their legacy world layout, whatever else the
    /// installed pack holds after them.
    fn legacy_pack() -> Vec<LevelDef> {
        let pack: LevelPack = serde_json::from_str(RAW_LEVELS).expect("levels.json");
        pack.levels
            .into_iter()
            .take(LEGACY_LEVEL_COUNT)
            .enumerate()
            .map(|(idx, mut level)| {
                level.world = idx / LEGACY_LEVELS_PER_WORLD + 1;
                level
            })
            .collect()
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

    /// Rows per designed test level.
    const DESIGNED_ROWS: usize = 6;
    /// Margin profiles (cells left empty on each side of a symmetric row), one per seed modulo
    /// the number of profiles, so consecutive seeds get clearly different shapes.
    const DESIGNED_PROFILES: [[usize; DESIGNED_ROWS]; 5] = [
        [1, 1, 2, 2, 3, 3],
        [3, 3, 2, 2, 1, 1],
        [1, 2, 3, 3, 2, 1],
        [3, 2, 1, 1, 2, 3],
        [1, 3, 1, 3, 1, 3],
    ];
    /// Number of colours cycled by the designed rows.
    const DESIGNED_COLOURS: u64 = 4;
    /// Seeds whose remainder modulo `DESIGNED_SEED_PERIOD` equals `DESIGNED_ASYMMETRIC_REMAINDER`
    /// get an asymmetric designed level; the others are mirror-symmetric.
    const DESIGNED_SEED_PERIOD: u64 = 3;
    const DESIGNED_ASYMMETRIC_REMAINDER: u64 = 1;
    /// An asymmetric row keeps its profile margin minus this on one side and plus
    /// `DESIGNED_SHIFT_RIGHT` on the other, so it differs from its mirror by 3 cells per side.
    const DESIGNED_SHIFT_LEFT: usize = 1;
    const DESIGNED_SHIFT_RIGHT: usize = 2;
    /// Letters of the alphabet, for digit-free test names.
    const ALPHABET: u64 = 26;

    /// Digit-free name: `prefix` and the base-26 letters of `k`.
    fn letter_name(prefix: &str, k: usize) -> String {
        let mut n = k as u64;
        let mut letters = String::new();
        loop {
            letters.insert(0, char::from(b'A' + (n % ALPHABET) as u8));
            n /= ALPHABET;
            if n == 0 {
                break;
            }
        }
        format!("{prefix} {letters}")
    }

    /// One run of a single colour per row, rows overlapping: a few big colour regions, one
    /// component, no dust. Symmetric runs are centred; asymmetric ones are shifted to one side.
    fn designed_rows(seed: u64, symmetric: bool) -> Vec<String> {
        let profile = DESIGNED_PROFILES[(seed % DESIGNED_PROFILES.len() as u64) as usize];
        let flip = (seed / DESIGNED_PROFILES.len() as u64) % 2 == 1;
        profile
            .iter()
            .enumerate()
            .map(|(r, &margin)| {
                let (left, right) = if symmetric {
                    (margin, margin)
                } else if flip {
                    (margin + DESIGNED_SHIFT_RIGHT, margin - DESIGNED_SHIFT_LEFT)
                } else {
                    (margin - DESIGNED_SHIFT_LEFT, margin + DESIGNED_SHIFT_RIGHT)
                };
                let colour =
                    char::from_digit(((seed + r as u64) % DESIGNED_COLOURS) as u32 + 1, 10)
                        .unwrap_or('1');
                (0..EDITOR_COLS)
                    .map(|c| {
                        if c >= left && c < EDITOR_COLS - right {
                            colour
                        } else {
                            '0'
                        }
                    })
                    .collect()
            })
            .collect()
    }

    /// Whether the designed level of `seed` is mirror-symmetric.
    fn designed_is_symmetric(seed: u64) -> bool {
        seed % DESIGNED_SEED_PERIOD != DESIGNED_ASYMMETRIC_REMAINDER
    }

    /// A boss gets noisy random rows; a regular level a designed one that satisfies every
    /// generated-level rule.
    fn make_level(idx: usize, name: &str, boss: bool, seed: u64) -> LevelDef {
        LevelDef {
            id: idx + 1,
            world: idx / SECTORS_PER_WORLD + 1,
            name: name.to_string(),
            boss,
            rows: if boss {
                random_rows(seed, true)
            } else {
                designed_rows(seed, designed_is_symmetric(seed))
            },
        }
    }

    /// Like `make_level` with an explicit symmetry choice.
    fn make_designed(idx: usize, name: &str, seed: u64, symmetric: bool) -> LevelDef {
        let mut level = make_level(idx, name, false, seed);
        level.rows = designed_rows(seed, symmetric);
        level
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
                &letter_name("TEST SECTOR", k),
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
                &letter_name("W SECTOR", next),
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

    fn rows_of(rows: &[&str]) -> Vec<String> {
        rows.iter().map(|r| r.to_string()).collect()
    }

    /// Legacy levels (world 1) plus regular designed levels up to `total` levels, no final boss.
    fn regular_pack(total: usize) -> Vec<LevelDef> {
        let mut levels = legacy_pack();
        for level in &mut levels {
            level.world = 1;
        }
        while levels.len() < total {
            let idx = levels.len();
            let seed = 2000 + idx as u64;
            levels.push(make_level(
                idx,
                &letter_name("FILLER SECTOR", idx),
                false,
                seed,
            ));
        }
        levels
    }

    /// Sets one cell of a level's first row.
    fn put_cell(level: &mut LevelDef, col: usize, ch: char) {
        level.rows[0].replace_range(col..col + 1, &ch.to_string());
    }

    #[test]
    fn dust_and_components_are_counted_on_eight_connectivity() {
        let solid = rows_of(&["111100000000000", "111100000000000"]);
        assert_eq!(dust_and_components(&solid), (0, 1));
        // A diagonal touch joins two cells into one component.
        let diagonal = rows_of(&["100000000000000", "010000000000000"]);
        assert_eq!(dust_and_components(&diagonal), (1, 1));
        // A lone cell and a 3-cell fragment are dust, a 4-cell block is not; portals are ignored.
        let mixed = rows_of(&["100011100001100", "00000000000000T"]);
        assert_eq!(dust_and_components(&mixed), (3, 3));
        let block = rows_of(&["110000000000000", "110000000000000"]);
        assert_eq!(dust_and_components(&block), (0, 1));
    }

    #[test]
    fn generated_regular_level_with_dust_is_rejected_but_a_boss_is_exempt() {
        let pack = extended_pack(3);
        assert!(!has_error(&validate_pack(&pack), "dust"));
        // A lone cell far from the body of a regular level.
        let mut dusty = pack.clone();
        dusty[15].rows[0] = "100000000000000".to_string();
        dusty[15].rows[1] = "0".repeat(EDITOR_COLS);
        assert!(has_error(&validate_pack(&dusty), "dust fragments"));
        // The final boss of the baseline pack is full of fragments and still passes.
        let last = pack.last().expect("final boss");
        assert!(dust_and_components(&last.rows).0 > MAX_DUST);
        assert!(!validate_pack(&pack).iter().any(|e| e.level_id == last.id));
    }

    #[test]
    fn too_many_components_are_rejected() {
        let blocks = |count: usize| -> Vec<String> {
            let row: String = (0..EDITOR_COLS)
                .map(|c| if c % 3 < 2 && c / 3 < count { '1' } else { '0' })
                .collect();
            vec![row; 3]
        };
        assert_eq!(
            dust_and_components(&blocks(MAX_COMPONENTS_8)).1,
            MAX_COMPONENTS_8
        );
        let mut pack = extended_pack(3);
        pack[15].rows = blocks(MAX_COMPONENTS_8);
        assert!(!has_error(&validate_pack(&pack), "connected components"));
        pack[15].rows = blocks(MAX_COMPONENTS_8 + 1);
        assert!(has_error(&validate_pack(&pack), "connected components"));
    }

    /// Rows holding one piece of `main` cells and one detached piece of `stray` cells.
    fn main_and_stray(main: usize, stray: usize) -> Vec<String> {
        let piece = |count: usize, ch: char| -> String {
            (0..EDITOR_COLS)
                .map(|c| if c < count { ch } else { '0' })
                .collect()
        };
        let mut rows = vec![piece(main, '1')];
        rows.push("0".repeat(EDITOR_COLS));
        rows.push("0".repeat(EDITOR_COLS));
        rows.push(piece(stray, '2').chars().rev().collect());
        rows
    }

    #[test]
    fn main_piece_share_is_the_largest_eight_connected_piece_over_all_shape_cells() {
        assert_eq!(main_piece_share(&[]), 1.0);
        assert_eq!(main_piece_share(&rows_of(&["000000000000000"])), 1.0);
        assert_eq!(main_piece_share(&main_and_stray(4, 1)), 0.8);
        assert_eq!(main_piece_share(&main_and_stray(3, 1)), 0.75);
        // A diagonal touch joins pieces; portals are not shape cells.
        let diagonal = rows_of(&["110000000000000", "001000000000000", "T00000000000000"]);
        assert_eq!(main_piece_share(&diagonal), 1.0);
    }

    #[test]
    fn regular_level_below_the_main_piece_share_is_rejected_at_the_exact_boundary() {
        let boundary = main_and_stray(4, 1);
        assert_eq!(main_piece_share(&boundary), MIN_MAIN_PIECE_SHARE);
        let mut pack = extended_pack(3);
        pack[15].rows = boundary.clone();
        assert!(!has_error(&validate_pack(&pack), "main piece"));
        pack[15].rows = main_and_stray(3, 1);
        assert!(has_error(&validate_pack(&pack), "main piece"));
        // The same rule runs when a generator checks one candidate, symmetric or not.
        let prefix = extended_pack(2);
        let prefix = &prefix[..prefix.len() - 1];
        let mut candidate = make_level(prefix.len(), "NEXT SECTOR", false, 4242);
        candidate.rows = boundary;
        let planned = prefix.len() + 1;
        assert!(!has_error(
            &validate_candidate(prefix, &candidate, false, planned),
            "main piece"
        ));
        candidate.rows = main_and_stray(3, 1);
        assert!(has_error(
            &validate_candidate(prefix, &candidate, false, planned),
            "main piece"
        ));
        candidate.rows = rows_of(&["110000000000011", "110000000000011"]);
        assert!(is_mirror_symmetric(&candidate));
        assert!(has_error(
            &validate_candidate(prefix, &candidate, false, planned),
            "main piece"
        ));
    }

    #[test]
    fn main_piece_gate_exempts_legacy_levels_and_bosses() {
        let mut legacy = legacy_pack();
        legacy[1].rows = main_and_stray(3, 3);
        assert!(!has_error(&validate_pack(&legacy), "main piece"));
        let mut pack = extended_pack(3);
        pack[2].rows = main_and_stray(3, 3);
        let boss_idx = pack.len() - 1;
        assert!(pack[boss_idx].boss);
        let mut boss_rows = vec!["0".repeat(EDITOR_COLS); BOSS_CLEAR_ROWS];
        boss_rows.extend(main_and_stray(3, 3));
        pack[boss_idx].rows = boss_rows;
        assert!(main_piece_share(&pack[boss_idx].rows) < MIN_MAIN_PIECE_SHARE);
        let errors = validate_pack(&pack);
        assert!(!has_error(&errors, "main piece"), "{errors:#?}");
    }

    /// Asymmetric rows made of two blocks of `block_rows` rows separated by `gap_rows` empty rows.
    fn two_blocks_stacked(block_rows: usize, gap_rows: usize) -> Vec<String> {
        let mut rows = vec!["111111000000000".to_string(); block_rows];
        rows.extend(vec!["0".repeat(EDITOR_COLS); gap_rows]);
        rows.extend(vec!["000000111111000".to_string(); block_rows]);
        rows
    }

    #[test]
    fn inner_gaps_are_measured_inside_the_shape_bounding_box() {
        let padded = rows_of(&["000000000000000", "011000000000110", "000000000000000"]);
        assert_eq!(inner_gap_rows(&padded), 0);
        assert_eq!(inner_gap_cols(&padded), 9);
        assert_eq!(inner_gap_rows(&two_blocks_stacked(2, 2)), 2);
        assert_eq!(inner_gap_rows(&two_blocks_stacked(2, 0)), 0);
        // Portals are not shape cells, so they neither fill a gap nor widen the box.
        let portals = rows_of(&["T00000000000000", "000011000000000", "00000000000000T"]);
        assert_eq!(inner_gap_cols(&portals), 0);
        assert_eq!(inner_gap_rows(&portals), 0);
        assert_eq!(inner_gap_rows(&[]), 0);
        assert_eq!(inner_gap_cols(&[]), 0);
    }

    #[test]
    fn asymmetric_level_with_a_wide_empty_column_gap_is_rejected() {
        let wide: Vec<String> = vec!["111100000001111".to_string(); 3]
            .into_iter()
            .chain(vec!["111000000001100".to_string(); 3])
            .collect();
        assert!(inner_gap_cols(&wide) > MAX_INNER_GAP_COLS);
        let mut pack = extended_pack(3);
        pack[15].rows = wide.clone();
        assert!(has_error(&validate_pack(&pack), "empty columns inside"));
        // The same rule runs when a generator checks one candidate.
        let prefix = extended_pack(2);
        let prefix = &prefix[..prefix.len() - 1];
        let mut candidate = make_level(prefix.len(), "NEXT SECTOR", false, 4242);
        candidate.rows = wide;
        assert!(has_error(
            &validate_candidate(prefix, &candidate, false, 100),
            "empty columns inside"
        ));
        // A gap of exactly the cap is fine.
        let narrow = rows_of(&["111100011111000", "111000011111100", "111100011110000"]);
        assert_eq!(inner_gap_cols(&narrow), MAX_INNER_GAP_COLS);
        pack[15].rows = narrow;
        assert!(!has_error(&validate_pack(&pack), "empty columns inside"));
    }

    #[test]
    fn asymmetric_level_with_a_tall_empty_row_gap_is_rejected() {
        let split = two_blocks_stacked(2, MAX_INNER_GAP_ROWS + 1);
        assert!(is_asymmetric(&sample_level(
            1,
            1,
            "X",
            vec![&split[0], &split[2]]
        )));
        let mut pack = extended_pack(3);
        pack[15].rows = split.clone();
        assert!(has_error(&validate_pack(&pack), "empty rows inside"));
        let prefix = extended_pack(2);
        let prefix = &prefix[..prefix.len() - 1];
        let mut candidate = make_level(prefix.len(), "NEXT SECTOR", false, 4242);
        candidate.rows = split;
        assert!(has_error(
            &validate_candidate(prefix, &candidate, false, 100),
            "empty rows inside"
        ));
        // A gap of exactly the cap is fine.
        pack[15].rows = two_blocks_stacked(2, MAX_INNER_GAP_ROWS);
        assert!(!has_error(&validate_pack(&pack), "empty rows inside"));
    }

    #[test]
    fn symmetric_boss_and_legacy_levels_are_exempt_from_the_inner_gap_rule() {
        let mirrored: Vec<String> = vec!["111000000000111".to_string(); 3]
            .into_iter()
            .chain(vec!["0".repeat(EDITOR_COLS); MAX_INNER_GAP_ROWS + 2])
            .chain(vec!["111000000000111".to_string(); 3])
            .collect();
        let mut pack = extended_pack(3);
        pack[15].rows = mirrored;
        let errors = validate_pack(&pack);
        assert!(!has_error(&errors, "empty columns inside"));
        assert!(!has_error(&errors, "empty rows inside"));
        // A legacy level is never judged, even asymmetric and split.
        let mut pack = extended_pack(3);
        pack[0].rows = two_blocks_stacked(2, MAX_INNER_GAP_ROWS + 2);
        assert!(!has_error(&validate_pack(&pack), "empty rows inside"));
    }

    #[test]
    fn colour_fragmentation_is_rejected() {
        // Four colours in 2x2 tiles: no two neighbours share a colour, every cell is a region.
        let tiles: Vec<String> = (0..DESIGNED_ROWS)
            .map(|r| {
                (0..EDITOR_COLS)
                    .map(|c| char::from_digit(((r % 2) * 2 + c % 2 + 1) as u32, 10).unwrap_or('1'))
                    .collect()
            })
            .collect();
        assert!(colour_regions_per_cell(&tiles) > MAX_COLOUR_REGIONS_PER_CELL);
        let mut pack = extended_pack(3);
        pack[15].rows = tiles;
        assert!(has_error(&validate_pack(&pack), "colour fragmentation"));
        // Bands of one colour per row are calm.
        let bands = designed_rows(1002, true);
        assert!(colour_regions_per_cell(&bands) <= MAX_COLOUR_REGIONS_PER_CELL);
        assert!(!has_error(
            &validate_pack(&extended_pack(3)),
            "colour fragmentation"
        ));
        assert_eq!(colour_regions_per_cell(&rows_of(&["000", "0T0"])), 0.0);
    }

    #[test]
    fn mirror_symmetry_is_exact() {
        let sym = make_designed(20, "SYM", 1002, true);
        let asym = make_designed(21, "ASYM", 1002, false);
        assert!(is_mirror_symmetric(&sym));
        assert!(!is_mirror_symmetric(&asym));
        let mut nearly = sym.clone();
        put_cell(&mut nearly, 4, '2');
        assert!(!is_mirror_symmetric(&nearly));
    }

    #[test]
    fn symmetric_share_per_world_is_enforced() {
        let mut pack = extended_pack(5);
        for k in 0..5 {
            let idx = LEGACY_LEVEL_COUNT + k;
            pack[idx] = make_designed(idx, &letter_name("TEST SECTOR", k), 1000 + k as u64, false);
        }
        assert!(has_error(&validate_pack(&pack), "mirror-symmetric"));
        assert!(!has_error(
            &validate_pack(&extended_pack(5)),
            "mirror-symmetric"
        ));
    }

    #[test]
    fn candidate_mode_checks_both_symmetry_shares_together() {
        // Two slots of the world are used (one symmetric, one asymmetric) and the candidate is
        // neither; one slot remains but one symmetric and one asymmetric level are still needed.
        let mut prefix = legacy_pack();
        for level in &mut prefix {
            level.world = 1;
        }
        prefix.push(make_designed(15, "ONE SECTOR", 1000, true));
        prefix.push(make_designed(16, "TWO SECTOR", 1001, false));
        let total = LEGACY_LEVEL_COUNT + 4;
        let mut neither = make_designed(17, "THREE SECTOR", 1002, true);
        put_cell(&mut neither, 4, '2');
        assert!(!is_mirror_symmetric(&neither) && !is_asymmetric(&neither));
        let errors = validate_candidate(&prefix, &neither, false, total);
        assert!(has_error(&errors, "slots remain"), "{errors:#?}");
        // A symmetric candidate leaves one slot for the one asymmetric level still needed.
        let sym = make_designed(17, "THREE SECTOR", 1002, true);
        let errors = validate_candidate(&prefix, &sym, false, total);
        assert!(errors.is_empty(), "{errors:#?}");
    }

    #[test]
    fn a_row_is_reused_by_at_most_sixteen_levels() {
        let reused = "120120120120120";
        let mut pack = extended_pack(MAX_ROW_REUSE + 2);
        for level in pack.iter_mut().skip(LEGACY_LEVEL_COUNT).take(MAX_ROW_REUSE) {
            level.rows[1] = reused.to_string();
        }
        assert!(!has_error(&validate_pack(&pack), "reused by"));
        pack[LEGACY_LEVEL_COUNT + MAX_ROW_REUSE].rows[1] = reused.to_string();
        let errors = validate_pack(&pack);
        assert!(
            has_error(&errors, "reused by 17 generated levels"),
            "{errors:#?}"
        );
    }

    #[test]
    fn single_run_rows_and_legacy_rows_are_exempt_from_reuse() {
        assert!(is_reuse_exempt("000000000000000"));
        assert!(is_reuse_exempt("111111111111111"));
        assert!(is_reuse_exempt("000333330000000"));
        assert!(!is_reuse_exempt("110011000000000"));
        assert!(!is_reuse_exempt("111222000000000"));
        // A portal is not a shape cell: it neither breaks a blank row nor counts as a run.
        assert!(is_reuse_exempt("T00000000000T00"));
        assert!(is_reuse_exempt("T00333330000T00"));
        assert!(!is_reuse_exempt("T0033T330000000"));
        let mut pack = extended_pack(MAX_ROW_REUSE + 2);
        for level in pack.iter_mut().skip(LEGACY_LEVEL_COUNT) {
            level.rows[1] = "000333330000000".to_string();
        }
        assert!(!has_error(&validate_pack(&pack), "reused by"));
        // Legacy levels do not count toward the cap.
        let mut pack = extended_pack(MAX_ROW_REUSE);
        let row = "120120120120120".to_string();
        for level in pack.iter_mut().take(LEGACY_LEVEL_COUNT) {
            level.rows[0] = row.clone();
        }
        for level in pack
            .iter_mut()
            .skip(LEGACY_LEVEL_COUNT)
            .take(MAX_ROW_REUSE - 1)
        {
            level.rows[1] = row.clone();
        }
        assert!(!has_error(&validate_pack(&pack), "reused by"));
    }

    #[test]
    fn generated_names_end_without_a_digit_and_do_not_start_with_final() {
        let mut pack = extended_pack(3);
        pack[15].name = "SECTOR 7".to_string();
        assert!(has_error(
            &validate_pack(&pack),
            "must not end with a digit"
        ));
        pack[15].name = "SECTOR SEVEN".to_string();
        assert!(!has_error(
            &validate_pack(&pack),
            "must not end with a digit"
        ));

        pack[15].name = "FINAL APPROACH".to_string();
        assert!(has_error(&validate_pack(&pack), "must not start with"));
        pack[15].name = "THE FINAL APPROACH".to_string();
        assert!(!has_error(&validate_pack(&pack), "must not start with"));
        // The level carrying the final boss suffix may start with the word.
        let last = pack.len() - 1;
        pack[last].name = "FINAL TEST [FINAL BOSS]".to_string();
        assert!(!has_error(&validate_pack(&pack), "must not start with"));
        // Legacy names are exempt.
        let mut pack = extended_pack(3);
        pack[1].name = "LEGACY 9".to_string();
        assert!(!has_error(
            &validate_pack(&pack),
            "must not end with a digit"
        ));
    }

    #[test]
    fn mechanics_are_rejected_before_their_debut_world() {
        let mut pack = regular_pack(LEGACY_LEVEL_COUNT + 3);
        put_cell(&mut pack[16], 7, 'G');
        let errors = validate_pack(&pack);
        assert!(
            has_error(&errors, "before its debut world 2"),
            "{errors:#?}"
        );
        // Legacy levels keep their mechanics (the shipped pack uses them in early worlds).
        assert!(!has_error(
            &validate_pack(&regular_pack(LEGACY_LEVEL_COUNT + 3)),
            "debut world"
        ));
    }

    #[test]
    fn a_mechanic_debuts_in_the_first_block_of_its_world_on_a_regular_level() {
        let first_of_world_2 = SECTORS_PER_WORLD;
        let build = |idx: usize, boss: bool| {
            let mut pack = regular_pack(2 * SECTORS_PER_WORLD);
            put_cell(&mut pack[idx], 7, 'G');
            pack[idx].boss = boss;
            validate_pack(&pack)
        };
        let ok = build(first_of_world_2, false);
        assert!(!has_error(&ok, "mechanic 'G'"), "{ok:#?}");
        let last_of_block = build(first_of_world_2 + SECTORS_PER_BLOCK - 1, false);
        assert!(
            !has_error(&last_of_block, "mechanic 'G'"),
            "{last_of_block:#?}"
        );
        let late = build(first_of_world_2 + SECTORS_PER_BLOCK, false);
        assert!(has_error(&late, "after the first"), "{late:#?}");
        let boss = build(first_of_world_2 + 1, true);
        assert!(has_error(&boss, "introduced by a boss"), "{boss:#?}");
    }

    #[test]
    fn a_complete_debut_world_must_contain_its_mechanic() {
        let pack = regular_pack(2 * SECTORS_PER_WORLD);
        assert!(has_error(
            &validate_pack(&pack),
            "never appears in its debut world 2"
        ));
        // An unfinished debut world is not judged.
        let pack = regular_pack(2 * SECTORS_PER_WORLD - 1);
        assert!(!has_error(&validate_pack(&pack), "never appears"));
        // Later in the pack is too late.
        let mut pack = regular_pack(3 * SECTORS_PER_WORLD);
        put_cell(&mut pack[2 * SECTORS_PER_WORLD], 7, 'G');
        let errors = validate_pack(&pack);
        assert!(has_error(&errors, "after its debut world 2"), "{errors:#?}");
        assert!(has_error(&errors, "never appears in its debut world 2"));
    }

    #[test]
    fn candidate_mode_judges_a_mechanic_introduction() {
        let prefix = regular_pack(SECTORS_PER_WORLD + SECTORS_PER_BLOCK);
        let total = 2 * SECTORS_PER_WORLD;
        let idx = prefix.len();
        let mut late = make_level(idx, "LATE GLASS", false, 3001);
        put_cell(&mut late, 7, 'G');
        assert!(has_error(
            &validate_candidate(&prefix, &late, false, total),
            "after the first"
        ));

        let prefix = regular_pack(SECTORS_PER_WORLD + 1);
        let idx = prefix.len();
        let mut early = make_level(idx, "EARLY GLASS", false, 3002);
        put_cell(&mut early, 7, 'G');
        let errors = validate_candidate(&prefix, &early, false, total);
        assert!(!has_error(&errors, "mechanic"), "{errors:#?}");
        let mut boss = make_level(idx, "EARLY GLASS [BOSS]", true, 3003);
        boss.rows[BOSS_CLEAR_ROWS] = format!("{}G{}", "0".repeat(7), "0".repeat(EDITOR_COLS - 8));
        let errors = validate_candidate(&prefix, &boss, false, total);
        assert!(has_error(&errors, "introduced by a boss"), "{errors:#?}");
    }
}
