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

//! Validation of fine brick grid levels (levels that carry a `cell` size).

use crate::config::{GameConfig, LevelDef};
use crate::physics::{
    BrickGrid, BRICK_TOP_OFFSET, FINE_MAX_COLS, FINE_MIN_BRICK_SIZE, FINE_PADDLE_CLEARANCE,
};

/// Reason a fine level is refused. Carries no user-facing text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FineLevelError {
    /// The level has no `cell` size.
    MissingCell,
    /// The row height or the gap is not a finite, non-negative number.
    InvalidCellSize,
    /// The level has no row.
    EmptyRows,
    /// A row is empty or its length differs from the first row.
    RaggedRows,
    /// The rows are wider than `FINE_MAX_COLS`.
    TooManyColumns,
    /// A cell character is not `0`, a digit `1`-`9` or `#`.
    InvalidCellChar { row: usize, col: usize },
    /// No brick can be destroyed, so the level could never be won.
    NoDestructibleBrick,
    /// A brick is narrower or lower than `FINE_MIN_BRICK_SIZE`.
    BrickTooSmall,
    /// The gap between bricks lets a ball slip through.
    GapTooLarge,
    /// The grid reaches into the paddle clearance zone.
    GridTooLow,
}

/// Checks that `level` is a playable fine grid for `config`.
///
/// Today this is enforced only by tests on the compiled-in asset. Any future
/// loader of external level packs must call it before accepting a fine level.
pub fn validate_fine_level(level: &LevelDef, config: &GameConfig) -> Result<(), FineLevelError> {
    let cell = level.cell.ok_or(FineLevelError::MissingCell)?;
    if !(cell.h.is_finite() && cell.gap.is_finite() && cell.h > 0.0 && cell.gap >= 0.0) {
        return Err(FineLevelError::InvalidCellSize);
    }
    let first = level.rows.first().ok_or(FineLevelError::EmptyRows)?;
    let cols = first.chars().count();
    if cols == 0 || level.rows.iter().any(|r| r.chars().count() != cols) {
        return Err(FineLevelError::RaggedRows);
    }
    if cols > FINE_MAX_COLS {
        return Err(FineLevelError::TooManyColumns);
    }
    let mut destructible = false;
    for (row, line) in level.rows.iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            match ch {
                '0' | '#' => {}
                '1'..='9' => destructible = true,
                _ => return Err(FineLevelError::InvalidCellChar { row, col }),
            }
        }
    }
    if !destructible {
        return Err(FineLevelError::NoDestructibleBrick);
    }
    let brick_w = BrickGrid::with_cell(&config.arena, cols, cell.h, cell.gap).brick_w();
    if brick_w < FINE_MIN_BRICK_SIZE || cell.h < FINE_MIN_BRICK_SIZE {
        return Err(FineLevelError::BrickTooSmall);
    }
    if cell.gap >= config.ball.default_radius * 2.0 {
        return Err(FineLevelError::GapTooLarge);
    }
    let rows = level.rows.len() as f32;
    let bottom = BRICK_TOP_OFFSET + rows * cell.h + (rows - 1.0) * cell.gap;
    if bottom > config.arena.paddle_y - FINE_PADDLE_CLEARANCE {
        return Err(FineLevelError::GridTooLow);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CellSize;
    use crate::test_support::config;

    const TEST_COLS: usize = 45;
    const TEST_ROWS: usize = 40;
    const TEST_CELL_H: f32 = 13.0;
    const TEST_CELL_GAP: f32 = 2.0;
    const TEST_SHORT_ROWS: usize = 4;
    const TEST_BAD_CHAR_ROW: usize = 5;
    const TEST_BAD_CHAR_COL: usize = 2;
    const TEST_HUGE_GAP: f32 = 20.0;

    fn fine_level() -> LevelDef {
        let mut rows = vec!["1".repeat(TEST_COLS); TEST_ROWS];
        rows[0] = "#".repeat(TEST_COLS);
        LevelDef {
            id: 1,
            world: 1,
            name: "FINE".to_owned(),
            boss: false,
            rows,
            cell: Some(CellSize {
                h: TEST_CELL_H,
                gap: TEST_CELL_GAP,
            }),
        }
    }

    #[test]
    fn accepts_a_synthetic_fine_level() {
        assert_eq!(validate_fine_level(&fine_level(), &config()), Ok(()));
    }

    #[test]
    fn rejects_a_level_without_cell() {
        let mut l = fine_level();
        l.cell = None;
        assert_eq!(
            validate_fine_level(&l, &config()),
            Err(FineLevelError::MissingCell)
        );
    }

    #[test]
    fn rejects_invalid_cell_sizes() {
        for (h, gap) in [
            (f32::NAN, TEST_CELL_GAP),
            (TEST_CELL_H, -1.0),
            (0.0, TEST_CELL_GAP),
            (TEST_CELL_H, f32::INFINITY),
        ] {
            let mut l = fine_level();
            l.cell = Some(CellSize { h, gap });
            assert_eq!(
                validate_fine_level(&l, &config()),
                Err(FineLevelError::InvalidCellSize)
            );
        }
    }

    #[test]
    fn rejects_empty_and_ragged_rows() {
        let mut l = fine_level();
        l.rows.clear();
        assert_eq!(
            validate_fine_level(&l, &config()),
            Err(FineLevelError::EmptyRows)
        );
        let mut l = fine_level();
        l.rows[3].pop();
        assert_eq!(
            validate_fine_level(&l, &config()),
            Err(FineLevelError::RaggedRows)
        );
        let mut l = fine_level();
        l.rows = vec![String::new()];
        assert_eq!(
            validate_fine_level(&l, &config()),
            Err(FineLevelError::RaggedRows)
        );
    }

    #[test]
    fn rejects_too_many_columns() {
        let mut l = fine_level();
        l.rows = vec!["1".repeat(FINE_MAX_COLS + 1); TEST_SHORT_ROWS];
        assert_eq!(
            validate_fine_level(&l, &config()),
            Err(FineLevelError::TooManyColumns)
        );
    }

    #[test]
    fn rejects_characters_outside_the_fine_charset() {
        for ch in ['E', 'T', 'G', 'B', 'R', 'M', ' ', 'x'] {
            let mut l = fine_level();
            l.rows[TEST_BAD_CHAR_ROW]
                .replace_range(TEST_BAD_CHAR_COL..TEST_BAD_CHAR_COL + 1, &ch.to_string());
            assert_eq!(
                validate_fine_level(&l, &config()),
                Err(FineLevelError::InvalidCellChar {
                    row: TEST_BAD_CHAR_ROW,
                    col: TEST_BAD_CHAR_COL
                })
            );
        }
    }

    #[test]
    fn rejects_a_level_without_destructible_brick() {
        let mut l = fine_level();
        l.rows = vec!["#0".repeat(TEST_COLS / 2); TEST_SHORT_ROWS];
        assert_eq!(
            validate_fine_level(&l, &config()),
            Err(FineLevelError::NoDestructibleBrick)
        );
    }

    #[test]
    fn rejects_bricks_below_the_minimum_size() {
        let mut l = fine_level();
        l.cell = Some(CellSize {
            h: FINE_MIN_BRICK_SIZE - 1.0,
            gap: 1.0,
        });
        assert_eq!(
            validate_fine_level(&l, &config()),
            Err(FineLevelError::BrickTooSmall)
        );
        let mut l = fine_level();
        l.rows = vec!["1".repeat(FINE_MAX_COLS); TEST_SHORT_ROWS];
        l.cell = Some(CellSize {
            h: TEST_CELL_H,
            gap: TEST_HUGE_GAP,
        });
        assert_eq!(
            validate_fine_level(&l, &config()),
            Err(FineLevelError::BrickTooSmall)
        );
    }

    #[test]
    fn rejects_a_gap_wider_than_the_ball() {
        let mut l = fine_level();
        l.rows = vec!["1".repeat(10); TEST_SHORT_ROWS];
        l.cell = Some(CellSize {
            h: TEST_CELL_H,
            gap: config().ball.default_radius * 2.0,
        });
        assert_eq!(
            validate_fine_level(&l, &config()),
            Err(FineLevelError::GapTooLarge)
        );
    }

    #[test]
    fn rejects_a_grid_reaching_the_paddle_zone() {
        let mut l = fine_level();
        l.rows = vec!["1".repeat(TEST_COLS); TEST_ROWS * 2];
        assert_eq!(
            validate_fine_level(&l, &config()),
            Err(FineLevelError::GridTooLow)
        );
    }
}
