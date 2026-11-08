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

use super::*;
use crate::physics::Brick;
use crate::test_support::world;

const FULL_ROW: &str = "111111111111111";

fn code_with_rows(rows: &[&str]) -> String {
    format!("{}{}", LEVEL_CODE_PREFIX, rows.join("/"))
}

#[test]
fn max_length_matches_contract() {
    assert_eq!(LEVEL_CODE_MAX_LEN, 164);
}

#[test]
fn rejects_empty_input() {
    assert_eq!(parse_level_code(""), Err(LevelCodeError::Empty));
    assert_eq!(parse_level_code("  KTX1:  "), Err(LevelCodeError::Empty));
}

#[test]
fn rejects_too_long_input() {
    let code = "1".repeat(LEVEL_CODE_MAX_LEN + 1);
    let err = parse_level_code(&code).unwrap_err();
    assert!(matches!(err, LevelCodeError::TooLong { .. }));
    assert_eq!(err.i18n_key(), "editor.import_too_long");
}

#[test]
fn rejects_too_many_rows() {
    let rows = vec!["1"; EDITOR_ROWS + 1];
    let err = parse_level_code(&code_with_rows(&rows)).unwrap_err();
    assert_eq!(
        err,
        LevelCodeError::TooManyRows {
            rows: EDITOR_ROWS + 1,
            max: EDITOR_ROWS
        }
    );
    assert_eq!(err.i18n_key(), "editor.import_invalid");
}

#[test]
fn rejects_too_wide_row() {
    let wide = "1".repeat(EDITOR_COLS + 1);
    let err = parse_level_code(&code_with_rows(&[FULL_ROW, &wide])).unwrap_err();
    assert_eq!(
        err,
        LevelCodeError::RowTooWide {
            row: 1,
            cols: EDITOR_COLS + 1,
            max: EDITOR_COLS
        }
    );
}

#[test]
fn rejects_invalid_character() {
    let err = parse_level_code("KTX1:11x1").unwrap_err();
    assert_eq!(
        err,
        LevelCodeError::InvalidChar {
            row: 0,
            col: 2,
            ch: 'x'
        }
    );
    assert!(matches!(
        parse_level_code("7").unwrap_err(),
        LevelCodeError::InvalidChar { .. }
    ));
}

#[test]
fn rejects_codes_without_destructible_bricks() {
    assert_eq!(
        parse_level_code("KTX1:000/#T#"),
        Err(LevelCodeError::NoDestructibleBricks)
    );
}

#[test]
fn leading_blank_cells_are_kept_without_prefix() {
    let rows = parse_level_code("  1/1").unwrap();
    assert_eq!(rows[0], "001000000000000");
    assert_eq!(
        parse_level_code(
            "
  1/1
"
        )
        .unwrap(),
        rows
    );
}

#[test]
fn pads_short_rows_and_prefix_is_optional() {
    let with = parse_level_code("KTX1:12/ 3").unwrap();
    let without = parse_level_code("12/ 3").unwrap();
    assert_eq!(with, without);
    assert_eq!(with[0], "120000000000000");
    assert_eq!(with[1], "030000000000000");
}

#[test]
fn failed_import_leaves_world_untouched() {
    let mut w = world();
    let count = w.bricks.len();
    assert!(w.import_level_code("nope").is_err());
    assert_eq!(w.bricks.len(), count);
}

#[test]
fn export_always_has_editor_rows() {
    let w = world();
    let code = w.export_level_code();
    let body = code.strip_prefix(LEVEL_CODE_PREFIX).unwrap();
    assert_eq!(body.split('/').count(), EDITOR_ROWS);
    assert!(body.split('/').all(|r| r.len() == EDITOR_COLS));
}

fn cell_signature(b: &Brick) -> (i32, i32, String) {
    let kind = match b.kind {
        BrickKind::Standard {
            max_hp, color_idx, ..
        } => format!("S{max_hp}{color_idx}"),
        BrickKind::Moving { color_idx, .. } => format!("M{color_idx}"),
        other => format!("{other:?}"),
    };
    (b.x.round() as i32, b.y.round() as i32, kind)
}

#[test]
fn export_import_round_trip_preserves_every_cell() {
    let mut w = world();
    w.bricks.clear();
    let grid = BrickGrid::new(&w.config.arena, EDITOR_COLS);
    let half_w = grid.brick_w() * 0.5;
    let paint = |w: &mut GameWorld, col: usize, row: usize, kind: BrickKind| {
        let (x, y) = grid.cell_origin(col, row);
        w.paint_brick_at(x + half_w, y + 2.0, Some(kind));
    };
    for d in 1..=6usize {
        let hp = if d >= 5 { 2 } else { 1 };
        paint(
            &mut w,
            d - 1,
            d - 1,
            BrickKind::Standard {
                hp,
                max_hp: hp,
                color_idx: d - 1,
            },
        );
    }
    paint(&mut w, 7, 0, BrickKind::Explosive);
    paint(&mut w, 8, 0, BrickKind::Indestructible);
    paint(&mut w, 9, 0, BrickKind::Portal);
    paint(&mut w, 10, 0, BrickKind::Ghost { revealed: false });
    paint(
        &mut w,
        11,
        0,
        BrickKind::Moving {
            hp: 2,
            color_idx: 3,
        },
    );
    paint(&mut w, 0, 8, BrickKind::Explosive);
    paint(&mut w, 14, 9, BrickKind::Indestructible);
    // A damaged two-hit brick keeps its colour digit.
    paint(
        &mut w,
        3,
        9,
        BrickKind::Standard {
            hp: 1,
            max_hp: 2,
            color_idx: 5,
        },
    );

    let code = w.export_level_code();
    let mut sig_before: Vec<_> = w.bricks.iter().map(cell_signature).collect();
    sig_before.sort();

    let mut other = world();
    other.import_level_code(&code).unwrap();
    assert_eq!(other.export_level_code(), code);
    let mut sig_after: Vec<_> = other.bricks.iter().map(cell_signature).collect();
    sig_after.sort();
    assert_eq!(sig_before, sig_after);

    let rows: Vec<&str> = code
        .strip_prefix(LEVEL_CODE_PREFIX)
        .unwrap()
        .split('/')
        .collect();
    assert_eq!(rows[8].chars().next(), Some('E'));
    assert_eq!(rows[9].chars().nth(14), Some('#'));
    assert_eq!(rows[9].chars().nth(3), Some('6'));
}
