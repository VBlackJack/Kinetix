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

use kinetix_core::config::LevelDef;
use kinetix_core::level_code::{EDITOR_COLS, EDITOR_ROWS};
use std::collections::{BTreeSet, VecDeque};

pub const MAX_LEVEL_NAME_LEN: usize = 24;
pub const BOSS_CLEAR_ROWS: usize = 3;

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
/// - Empty cells (`0`), destructible bricks (`1-6`, `E`, `G`, `M`) and portals (`T`) are
///   traversable (destructible bricks are cleared as the ball hits them, opening the path
///   behind them; portals also teleport between pairs).
/// - Indestructible `#` cells block traversal.
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

/// Validates a full campaign pack.
pub fn validate_pack(levels: &[LevelDef]) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    if levels.is_empty() {
        errors.push(ValidationError {
            level_id: 0,
            message: "level pack is empty".to_string(),
        });
        return errors;
    }

    let mut seen_names = BTreeSet::new();
    let mut expected_id = 1usize;
    let mut current_world = 1usize;

    for (idx, level) in levels.iter().enumerate() {
        let lid = level.id;
        if lid != expected_id {
            errors.push(ValidationError {
                level_id: lid,
                message: format!("expected contiguous id {expected_id} at index {idx}, got {lid}"),
            });
        }
        expected_id = lid + 1;

        if idx == 0 {
            if level.world != 1 {
                errors.push(ValidationError {
                    level_id: lid,
                    message: format!("first level must be in world 1, got {}", level.world),
                });
            }
            current_world = level.world;
        } else if level.world < current_world || level.world > current_world + 1 {
            errors.push(ValidationError {
                level_id: lid,
                message: format!(
                    "world numbers must be ascending and contiguous (previous {current_world}, got {})",
                    level.world
                ),
            });
        } else {
            current_world = level.world;
        }

        let name = level.name.as_str();
        if name.trim().is_empty() {
            errors.push(ValidationError {
                level_id: lid,
                message: "name is empty".to_string(),
            });
        }
        // Only enforce the 24-char cap on new levels (index >= 15); level 15 is 26 chars and pinned.
        if idx >= 15 && name.chars().count() > MAX_LEVEL_NAME_LEN {
            errors.push(ValidationError {
                level_id: lid,
                message: format!(
                    "name {:?} is {} chars long (max {MAX_LEVEL_NAME_LEN})",
                    name,
                    name.chars().count()
                ),
            });
        }
        if !name.chars().all(is_valid_name_char) {
            errors.push(ValidationError {
                level_id: lid,
                message: format!(
                    "name {:?} contains disallowed characters (allowed: A-Z, 0-9, space, -, [, ])",
                    name
                ),
            });
        }
        if !seen_names.insert(name.to_string()) {
            errors.push(ValidationError {
                level_id: lid,
                message: format!("duplicate level name {:?}", name),
            });
        }

        if level.rows.is_empty() || level.rows.len() > EDITOR_ROWS {
            errors.push(ValidationError {
                level_id: lid,
                message: format!(
                    "row count {} out of range 1..={EDITOR_ROWS}",
                    level.rows.len()
                ),
            });
        }

        let mut destructible_count = 0usize;
        let mut portal_count = 0usize;

        for (r_idx, row) in level.rows.iter().enumerate() {
            let char_count = row.chars().count();
            if char_count != EDITOR_COLS {
                errors.push(ValidationError {
                    level_id: lid,
                    message: format!("row {r_idx} has {char_count} chars, expected {EDITOR_COLS}"),
                });
            }
            for (c_idx, ch) in row.chars().enumerate() {
                if !is_allowed_cell(ch) {
                    errors.push(ValidationError {
                        level_id: lid,
                        message: format!("invalid cell {ch:?} at row {r_idx}, col {c_idx}"),
                    });
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
            errors.push(ValidationError {
                level_id: lid,
                message: "level has no destructible bricks".to_string(),
            });
        }

        // A single portal cannot teleport anywhere; generated sectors (idx >= 15) must also use even pairs.
        if portal_count == 1 || (idx >= 15 && !portal_count.is_multiple_of(2)) {
            errors.push(ValidationError {
                level_id: lid,
                message: format!(
                    "portals must appear in pairs (even count, not 1), found {portal_count}"
                ),
            });
        }

        if level.boss {
            if level.rows.len() < BOSS_CLEAR_ROWS {
                errors.push(ValidationError {
                    level_id: lid,
                    message: format!("boss level must have at least {BOSS_CLEAR_ROWS} rows"),
                });
            } else {
                for r_idx in 0..BOSS_CLEAR_ROWS {
                    if level.rows[r_idx].chars().any(|c| c != '0') {
                        errors.push(ValidationError {
                            level_id: lid,
                            message: format!(
                                "boss level row {r_idx} must be all '0' for boss clearance"
                            ),
                        });
                    }
                }
            }
        }

        if let Err(msg) = check_reachability(&level.rows) {
            errors.push(ValidationError {
                level_id: lid,
                message: msg,
            });
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
