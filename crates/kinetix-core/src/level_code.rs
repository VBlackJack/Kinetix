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

//! Shareable level codes: `KTX1:` followed by up to `EDITOR_ROWS` rows of `EDITOR_COLS` cells,
//! separated by `/`. Parsing is pure; import and export are methods on [`GameWorld`].

use crate::physics::{BrickGrid, BrickKind, GameWorld};
use std::fmt;

pub const LEVEL_CODE_PREFIX: &str = "KTX1:";
pub const EDITOR_COLS: usize = 15;
pub const EDITOR_ROWS: usize = 10;
pub const LEVEL_CODE_MAX_LEN: usize =
    LEVEL_CODE_PREFIX.len() + EDITOR_ROWS * EDITOR_COLS + (EDITOR_ROWS - 1);

const ROW_SEPARATOR: char = '/';
const EMPTY_CELL: char = '0';
const BLANK_CELL: char = ' ';
const EXPLOSIVE_CELL: char = 'E';
const INDESTRUCTIBLE_CELL: char = '#';
const PORTAL_CELL: char = 'T';
const GHOST_CELL: char = 'G';
const MOVING_CELL: char = 'M';
const MIN_COLOR_DIGIT: u32 = 1;
const MAX_COLOR_DIGIT: u32 = 6;
/// First digit that loads as a two-hit brick (see `GameWorld::load_from_rows`).
const FIRST_TOUGH_DIGIT: u32 = 5;
const LEVEL_CODE_BASE_TIER: usize = 1;
const I18N_KEY_TOO_LONG: &str = "editor.import_too_long";
const I18N_KEY_INVALID: &str = "editor.import_invalid";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LevelCodeError {
    Empty,
    TooLong {
        len: usize,
        max: usize,
    },
    TooManyRows {
        rows: usize,
        max: usize,
    },
    /// `row` and `col` are zero-based.
    RowTooWide {
        row: usize,
        cols: usize,
        max: usize,
    },
    InvalidChar {
        row: usize,
        col: usize,
        ch: char,
    },
    NoDestructibleBricks,
}

impl LevelCodeError {
    /// Locale key of the message shown to the player.
    pub fn i18n_key(&self) -> &'static str {
        match self {
            Self::TooLong { .. } => I18N_KEY_TOO_LONG,
            _ => I18N_KEY_INVALID,
        }
    }
}

impl fmt::Display for LevelCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "level code is empty"),
            Self::TooLong { len, max } => {
                write!(f, "level code is {len} bytes long, the maximum is {max}")
            }
            Self::TooManyRows { rows, max } => {
                write!(f, "level code has {rows} rows, the maximum is {max}")
            }
            Self::RowTooWide { row, cols, max } => {
                write!(f, "row {row} has {cols} cells, the maximum is {max}")
            }
            Self::InvalidChar { row, col, ch } => {
                write!(f, "invalid character {ch:?} at row {row}, column {col}")
            }
            Self::NoDestructibleBricks => write!(f, "level code has no destructible brick"),
        }
    }
}

impl std::error::Error for LevelCodeError {}

fn is_destructible_cell(ch: char) -> bool {
    matches!(ch, '1'..='6' | EXPLOSIVE_CELL | GHOST_CELL | MOVING_CELL)
}

fn is_valid_cell(ch: char) -> bool {
    ch == EMPTY_CELL
        || ch == BLANK_CELL
        || is_destructible_cell(ch)
        || ch == INDESTRUCTIBLE_CELL
        || ch == PORTAL_CELL
}

/// Validates a level code and returns its rows, each padded to `EDITOR_COLS` cells.
/// The `KTX1:` prefix is optional; blank cells are read as empty cells.
pub fn parse_level_code(code: &str) -> Result<Vec<String>, LevelCodeError> {
    // Leading spaces are blank cells when there is no prefix: only other whitespace is trimmed.
    let fully_trimmed = code.trim();
    let trimmed = if fully_trimmed.starts_with(LEVEL_CODE_PREFIX) {
        fully_trimmed
    } else {
        code.trim_matches(|c: char| c.is_whitespace() && c != BLANK_CELL)
    };
    if trimmed.trim().is_empty() {
        return Err(LevelCodeError::Empty);
    }
    if trimmed.len() > LEVEL_CODE_MAX_LEN {
        return Err(LevelCodeError::TooLong {
            len: trimmed.len(),
            max: LEVEL_CODE_MAX_LEN,
        });
    }
    let body = trimmed.strip_prefix(LEVEL_CODE_PREFIX).unwrap_or(trimmed);

    let mut raw_rows: Vec<&str> = body.split(ROW_SEPARATOR).collect();
    while raw_rows.last().is_some_and(|r| r.is_empty()) {
        raw_rows.pop();
    }
    if raw_rows.is_empty() {
        return Err(LevelCodeError::Empty);
    }
    if raw_rows.len() > EDITOR_ROWS {
        return Err(LevelCodeError::TooManyRows {
            rows: raw_rows.len(),
            max: EDITOR_ROWS,
        });
    }

    let mut rows = Vec::with_capacity(raw_rows.len());
    let mut has_destructible = false;
    for (row_idx, raw) in raw_rows.iter().enumerate() {
        let cols = raw.chars().count();
        if cols > EDITOR_COLS {
            return Err(LevelCodeError::RowTooWide {
                row: row_idx,
                cols,
                max: EDITOR_COLS,
            });
        }
        let mut row = String::with_capacity(EDITOR_COLS);
        for (col_idx, ch) in raw.chars().enumerate() {
            if !is_valid_cell(ch) {
                return Err(LevelCodeError::InvalidChar {
                    row: row_idx,
                    col: col_idx,
                    ch,
                });
            }
            has_destructible |= is_destructible_cell(ch);
            row.push(if ch == BLANK_CELL { EMPTY_CELL } else { ch });
        }
        while row.len() < EDITOR_COLS {
            row.push(EMPTY_CELL);
        }
        rows.push(row);
    }
    if !has_destructible {
        return Err(LevelCodeError::NoDestructibleBricks);
    }
    Ok(rows)
}

impl GameWorld {
    /// Loads a level from a shareable code. On error the world is left untouched.
    pub fn import_level_code(&mut self, code: &str) -> Result<(), LevelCodeError> {
        let rows = parse_level_code(code)?;
        self.load_from_rows(&rows, false, LEVEL_CODE_BASE_TIER);
        Ok(())
    }

    /// Encodes the current bricks as a level code with exactly `EDITOR_ROWS` rows.
    pub fn export_level_code(&self) -> String {
        let grid = BrickGrid::new(&self.config.arena, EDITOR_COLS);
        let mut cells = vec![vec![EMPTY_CELL; EDITOR_COLS]; EDITOR_ROWS];
        for brick in &self.bricks {
            let (cx, cy) = brick.center();
            let Some((col, row)) = grid.cell_at(cx, cy, EDITOR_ROWS) else {
                continue;
            };
            cells[row][col] = match brick.kind {
                BrickKind::Explosive => EXPLOSIVE_CELL,
                BrickKind::Indestructible => INDESTRUCTIBLE_CELL,
                BrickKind::Portal => PORTAL_CELL,
                BrickKind::Ghost { .. } => GHOST_CELL,
                BrickKind::Moving { .. } => MOVING_CELL,
                BrickKind::Standard {
                    max_hp, color_idx, ..
                } => {
                    // Inverse of the loader: digit d is colour d - 1, two hits from
                    // FIRST_TOUGH_DIGIT. Keyed on max_hp so damaged bricks keep their digit.
                    let digit = (color_idx as u32 + 1).clamp(MIN_COLOR_DIGIT, MAX_COLOR_DIGIT);
                    let digit = if max_hp > 1 {
                        digit.max(FIRST_TOUGH_DIGIT)
                    } else {
                        digit.min(FIRST_TOUGH_DIGIT - 1)
                    };
                    char::from_digit(digit, 10).unwrap_or(EMPTY_CELL)
                }
            };
        }
        let rows: Vec<String> = cells.into_iter().map(|r| r.into_iter().collect()).collect();
        format!(
            "{}{}",
            LEVEL_CODE_PREFIX,
            rows.join(&ROW_SEPARATOR.to_string())
        )
    }
}

#[cfg(test)]
mod tests;
