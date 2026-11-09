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

//! Hand-authored 15-column motif template library of varied row counts (4 to 9 rows).
//!
//! Legend inside motif rows:
//! - `.` : empty space
//! - `X` : primary body brick
//! - `Y` : secondary / trim brick
//! - `Z` : core / focal brick (often tough or special)
//! - `E` : designed explosive chain node

/// A named, hand-authored 15-column motif template.
#[derive(Debug, Clone, Copy)]
pub struct MotifTemplate {
    pub name: &'static str,
    pub family: MotifFamily,
    pub rows: &'static [&'static str],
}

/// Thematic family of a motif template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotifFamily {
    Icon,
    Glyph,
    Mandala,
    Checker,
    Diagonal,
    Arrow,
    Staircase,
    SpiralWave,
}

pub const MOTIFS: [MotifTemplate; 48] = [
    // =========================================================================
    // 1..=12: PIXEL ICONS (varied 5 to 8 rows, symmetric and asymmetric)
    // =========================================================================
    MotifTemplate {
        name: "PIXEL_HEART",
        family: MotifFamily::Icon,
        rows: &[
            "..YYY.....YYY..",
            ".YXXXY...YXXXY.",
            "YXXZXXY.YXXZXXY",
            "YXXXXXYEYXXXXXY",
            ".YXXXXXYXXXXXY.",
            "..YXXXXZXXXXY..",
            "...YYYYEYYYY...",
        ],
    },
    MotifTemplate {
        name: "PIXEL_SKULL",
        family: MotifFamily::Icon,
        rows: &[
            "...YYYYYYYYY...",
            "..YXXXXXXXXXY..",
            ".YXX..XXX..XXY.",
            ".YXX..XEX..XXY.",
            "..YXXXXXXXXXY..",
            "...YX.YZY.XY...",
            "...YY.Y.Y.YY...",
        ],
    },
    MotifTemplate {
        name: "PIXEL_ROCKET",
        family: MotifFamily::Icon,
        rows: &[
            "......YZY......",
            ".....YXXXY.....",
            "....YXXZXXY....",
            "....YXXXXXY....",
            "...YYXXEXXYY...",
            "..YXXYXXXYXXY..",
            ".YXX..YZY..XXY.",
            ".YY...E.E...YY.",
        ],
    },
    MotifTemplate {
        name: "PIXEL_CROWN",
        family: MotifFamily::Icon,
        rows: &[
            ".Z...Y...Y...Z.",
            "YXY.YXY.YXY.YXY",
            "YXXYXXXYXXXYXXY",
            "YXXZXXEXXXZXXXY",
            ".YXXXXXXXXXXXY.",
            ".YYYYYYYYYYYYY.",
        ],
    },
    MotifTemplate {
        name: "SPACE_INVADER",
        family: MotifFamily::Icon,
        rows: &[
            "..Y.........Y..",
            "...Y..YYY..Y...",
            "..YYYYXEXYYYY..",
            ".YY.XXXXXXX.YY.",
            "YYYYYXXXXXYZZZZ",
            ".Y.YXXXXXXX.Y..",
            "...Y..Y.Y..Y...",
        ],
    },
    MotifTemplate {
        name: "PIXEL_KEY",
        family: MotifFamily::Icon,
        rows: &[
            ".YYYYY.........",
            "YXX.XXY........",
            "YXXEXXYYYYYYYYZ",
            "YXX.XXY....Y..Y",
            ".YYYYY.....YY.Y",
        ],
    },
    MotifTemplate {
        name: "PIXEL_SWORD",
        family: MotifFamily::Icon,
        rows: &[
            "..........YYYYZ",
            "........YYXXXY.",
            "......YYXXEXY..",
            "...Z..YXXXXY...",
            "..YXY.YXXXY....",
            ".YXXZYYXXY.....",
            "YXXY..YYY......",
        ],
    },
    MotifTemplate {
        name: "RINGED_PLANET",
        family: MotifFamily::Icon,
        rows: &[
            ".........YYYYYY",
            "....YYYYYXXYY..",
            "..YYXXXXZXXY...",
            ".YXXXYXXXXXY...",
            "YYXXYXXXXXY....",
            "YYYYYXXXXY.....",
            "....YYYYY......",
        ],
    },
    MotifTemplate {
        name: "PIXEL_ANCHOR",
        family: MotifFamily::Icon,
        rows: &[
            ".....YYYYY.....",
            "....YXX.XXY....",
            ".....YYYYY.....",
            "...YYYYEYYYY...",
            "......YXY......",
            ".YY...YXY...YY.",
            ".YXXY.YZY.YXXY.",
            "..YYYYYYYYYYY..",
        ],
    },
    MotifTemplate {
        name: "ALL_SEEING_EYE",
        family: MotifFamily::Icon,
        rows: &[
            "....YYYYYYY....",
            "..YYXXXXXXXYY..",
            ".YXXY..Z..YXXY.",
            "YXXY..YEY..YXXY",
            ".YXXY..Z..YXXY.",
            "..YYXXXXXXXYY..",
            "....YYYYYYY....",
        ],
    },
    MotifTemplate {
        name: "COMET_STREAK",
        family: MotifFamily::Icon,
        rows: &[
            ".YYYY..........",
            "YZZZXY..YYYY...",
            "YZEZXXYYYYXXYY.",
            "YZZZXY...YYYYYY",
            ".YYYY...YYYY...",
            "..........YYYYY",
        ],
    },
    MotifTemplate {
        name: "CRESCENT_MOON",
        family: MotifFamily::Icon,
        rows: &[
            "..YYYYY....Y...",
            ".YXXXXY...YZY..",
            "YXXXXY.....Y...",
            "YXXXEY......Y..",
            "YXXXXY.....YEY.",
            ".YXXXXY.....Y..",
            "..YYYYY........",
        ],
    },
    // =========================================================================
    // 13..=20: LETTERS, DIGITS & GLYPHS (5 to 7 rows)
    // =========================================================================
    MotifTemplate {
        name: "GLYPH_K_X",
        family: MotifFamily::Glyph,
        rows: &[
            "YY...YY.YY...YY",
            "YY..YY...YY.YY.",
            "YYYYE.....YZY..",
            "YYYYY.....YEY..",
            "YY..YY...YY.YY.",
            "YY...YY.YY...YY",
        ],
    },
    MotifTemplate {
        name: "GLYPH_OMEGA",
        family: MotifFamily::Glyph,
        rows: &[
            "...YYYYYYYYY...",
            ".YYXXXXXXXXXYY.",
            "YXXY.......YXXY",
            "YXXY...E...YXXY",
            ".YXXY.....YXXY.",
            "..YYZZ...ZZYY..",
            "YYYYYY...YYYYYY",
        ],
    },
    MotifTemplate {
        name: "GLYPH_SIGMA_DELTA",
        family: MotifFamily::Glyph,
        rows: &[
            "YYYYYYY....Y...",
            ".YY.......YXY..",
            "..YY.....YXXXY.",
            "...YEY..YXXEXXY",
            "..YY...YXX...XX",
            ".YY....YYYYYYYY",
            "YYYYYYY........",
        ],
    },
    MotifTemplate {
        name: "DIGITS_800",
        family: MotifFamily::Glyph,
        rows: &[
            "YYYY.YYYY..YYYY",
            "Y..Y.Y..Y..Y..Y",
            "YEEY.Y.EY..Y.EY",
            "Y..Y.Y..Y..Y..Y",
            "YYYY.YYYY..YYYY",
        ],
    },
    MotifTemplate {
        name: "DIGITS_42",
        family: MotifFamily::Glyph,
        rows: &[
            "YY..YY..YYYYYYY",
            "YY..YY.......YY",
            "YYYYZYY.YYYYYYY",
            "....YY..YY..E..",
            "....YY..YY.....",
            "....YY..YYYYYYY",
        ],
    },
    MotifTemplate {
        name: "GLYPH_INFINITY",
        family: MotifFamily::Glyph,
        rows: &[
            ".YYYYY...YYYYY.",
            "YXX.XXY.YXX.XXY",
            "YX.E.XZYZX.E.XY",
            "YXX.XXY.YXX.XXY",
            ".YYYYY...YYYYY.",
        ],
    },
    MotifTemplate {
        name: "GLYPH_LIGHTNING_Z",
        family: MotifFamily::Glyph,
        rows: &[
            "..YYYYYYYYYYYY.",
            ".........YYYY..",
            ".......YYYY....",
            "....YYYYEYYYY..",
            ".....YYYY......",
            "...YYYY........",
            ".YYYYYYYYYYYY..",
        ],
    },
    MotifTemplate {
        name: "GLYPH_TRIDENT",
        family: MotifFamily::Glyph,
        rows: &[
            "YY....YZY....YY",
            "YXY...YXY...YXY",
            "YXY...YXY...YXY",
            ".YYYYYYEYYYYYY.",
            "...YYYYXYYYY...",
            "......YXY......",
            ".....YYYYY.....",
        ],
    },
    // =========================================================================
    // 21..=26: MANDALAS & RINGS (5 to 8 rows)
    // =========================================================================
    MotifTemplate {
        name: "STAR_MANDALA",
        family: MotifFamily::Mandala,
        rows: &[
            "...Y...Z...Y...",
            "..YXY.YXY.YXY..",
            ".YXXXYXEXYXXXY.",
            "ZXXEXXXZXXXEXXZ",
            ".YXXXYXEXYXXXY.",
            "..YXY.YXY.YXY..",
            "...Y...Z...Y...",
        ],
    },
    MotifTemplate {
        name: "LOTUS_MANDALA",
        family: MotifFamily::Mandala,
        rows: &[
            "..YY..YYY..YY..",
            ".YXXYYXZXYYXXY.",
            "YXX.YXXEXXY.XXY",
            "YXX.YXXXXXY.XXY",
            ".YXXYYXZXYYXXY.",
            "..YY..YYY..YY..",
        ],
    },
    MotifTemplate {
        name: "OCTAGON_VAULT",
        family: MotifFamily::Mandala,
        rows: &[
            "...YYYYYYYYY...",
            "..YXXXXXXXXXY..",
            ".YXX..YYY..XXY.",
            "YXX..YXEXY..XXY",
            "YXX..YXZXY..XXY",
            ".YXX..YYY..XXY.",
            "..YXXXXXXXXXY..",
            "...YYYYYYYYY...",
        ],
    },
    MotifTemplate {
        name: "ROSE_COMPASS",
        family: MotifFamily::Mandala,
        rows: &[
            ".......Z...YYYY",
            "......YXY.YXXY.",
            "..Y..YXXXYXXY..",
            "YYYYYYXEXZXY...",
            "..Y..YXXXXY....",
            "......YXY..Y...",
            ".......Z....Y..",
        ],
    },
    MotifTemplate {
        name: "SUNBURST_HALO",
        family: MotifFamily::Mandala,
        rows: &[
            "Y..Y..YYY..Y..Y",
            ".Y.Y.YXXXY.Y.Y.",
            "..YYYXXEXXYYY..",
            ".Y.Y.YXXXY.Y.Y.",
            "Y..Y..YYY..Y..Y",
        ],
    },
    MotifTemplate {
        name: "PRISM_LATTICE",
        family: MotifFamily::Mandala,
        rows: &[
            "...Y...Y...Y...",
            "..YXY.YXY.YXY..",
            ".YXXXYXEXYXXXY.",
            "YXXZXXYZYXXZXXY",
            "YXXEXXYZYXXEXXY",
            ".YXXXYXEXYXXXY.",
            "..YXY.YXY.YXY..",
            "...Y...Y...Y...",
        ],
    },
    // =========================================================================
    // 27..=30: CHECKERBOARDS & MASONRY GRIDS (4 to 7 rows)
    // =========================================================================
    MotifTemplate {
        name: "CHECKERBOARD_4R",
        family: MotifFamily::Checker,
        rows: &[
            "XX..YY..XX..YY.",
            "..YY..XX..YY..X",
            "YY..EEXX..YY..X",
            "..XX..YY..EEXX.",
        ],
    },
    MotifTemplate {
        name: "CHECKERBOARD_7R",
        family: MotifFamily::Checker,
        rows: &[
            "XX.YY.XX.YY.XX.",
            ".YY.XX.YY.XX.YY",
            "XX.YY..E..YY.XX",
            ".YY..Z.E.Z..YY.",
            "XX.YY..E..YY.XX",
            ".YY.XX.YY.XX.YY",
            "XX.YY.XX.YY.XX.",
        ],
    },
    MotifTemplate {
        name: "WAFFLE_MATRIX",
        family: MotifFamily::Checker,
        rows: &[
            "XXX.YYY.YYY.XXX",
            "XEX.YZY.YZY.XEX",
            "...............",
            "YYY.XXX.XXX.YYY",
            "YZY.XEX.XEX.YZY",
            "XXX.YYY.YYY.XXX",
        ],
    },
    MotifTemplate {
        name: "OFFSET_BRICKWORK",
        family: MotifFamily::Checker,
        rows: &[
            "XXXX.YYYY.XXXX.",
            "..YYYY.XXXX.YYY",
            "XXXX.YEEY.XXXX.",
            "..ZZZZ.XXXX.ZZZ",
            "YYYY.XXXX.YYYY.",
        ],
    },
    // =========================================================================
    // 31..=35: DIAGONALS & SLASHES (5 to 8 rows, strongly asymmetric)
    // =========================================================================
    MotifTemplate {
        name: "RISING_SLASHES",
        family: MotifFamily::Diagonal,
        rows: &[
            "....YY....YY..Z",
            "...YYX...YYX.ZY",
            "..YYX...YYE.ZYY",
            ".YYX...YYX.ZYY.",
            "YYX...YYX.ZYY..",
            "YX...YYX.ZYY...",
        ],
    },
    MotifTemplate {
        name: "FALLING_RIBBONS",
        family: MotifFamily::Diagonal,
        rows: &[
            "Z..YY....YY....",
            "YZ.XYY...XYY...",
            "YYZ.EYY...XYY..",
            ".YYZ.XYY...XYY.",
            "..YYZ.XYY...XYY",
            "...YYZ.EYY...XY",
            "....YYZ.XYY...Y",
        ],
    },
    MotifTemplate {
        name: "CROSS_SASH",
        family: MotifFamily::Diagonal,
        rows: &[
            "YYY.........ZZ.",
            ".YYYY......ZZ..",
            "..YYYY....ZZ...",
            "...YYEY..ZZ....",
            "....YYYYZZ.....",
            "...ZZYYYY......",
            "..ZZ..YYEY.....",
            ".ZZ....YYYY....",
        ],
    },
    MotifTemplate {
        name: "BANDOLIER_BELT",
        family: MotifFamily::Diagonal,
        rows: &[
            "YYYYY..ZZ......",
            "..YYYYY..ZZ....",
            "....YYEYY..ZZ..",
            "......YYYYY..ZZ",
            "ZZ......YYYYY..",
        ],
    },
    MotifTemplate {
        name: "SHEAR_FAULT",
        family: MotifFamily::Diagonal,
        rows: &[
            "XXXXXXX........",
            "YYYYYEX........",
            "ZZZZZZX.XXXXXXX",
            "XXXXXXX.XEYYYYY",
            "........XZZZZZZ",
            "........XXXXXXX",
        ],
    },
    // =========================================================================
    // 36..=40: ARROWS & CHEVRONS (5 to 7 rows)
    // =========================================================================
    MotifTemplate {
        name: "EAST_CHEVRON",
        family: MotifFamily::Arrow,
        rows: &[
            "YY...YY...YY...",
            ".YY...YY...YY..",
            "..YYE..YYE..YYZ",
            "..YYE..YYE..YYZ",
            ".YY...YY...YY..",
            "YY...YY...YY...",
        ],
    },
    MotifTemplate {
        name: "WEST_CHEVRON",
        family: MotifFamily::Arrow,
        rows: &[
            "...YY...YY...YY",
            "..YY...YY...YY.",
            "ZYY..EYY..EYY..",
            "ZYY..EYY..EYY..",
            "..YY...YY...YY.",
            "...YY...YY...YY",
        ],
    },
    MotifTemplate {
        name: "TWIN_SPEARHEAD",
        family: MotifFamily::Arrow,
        rows: &[
            "Y.....Y.Y.....Y",
            "YY...YY.YY...YY",
            ".YY.YY...YY.YY.",
            "..YEY.....YEY..",
            ".YY.YY...YY.YY.",
            "YY...YY.YY...YY",
            ".ZZZZZ...ZZZZZ.",
        ],
    },
    MotifTemplate {
        name: "UPWARD_VECTORS",
        family: MotifFamily::Arrow,
        rows: &[
            ".......Z.......",
            ".....YYXYY.....",
            "...YYXXEXXYY...",
            ".YYXX.....XXYY.",
            "YXX..YYZYY..XXY",
            "...YYXXEXXYY...",
        ],
    },
    MotifTemplate {
        name: "HOOK_ARROW",
        family: MotifFamily::Arrow,
        rows: &[
            "...YYYYYYYYYY..",
            "..YY........YY.",
            ".YY...YYYY...YY",
            "ZYEY.YY..YY..YY",
            ".YY..YY......YY",
            "..YY..YYYYYYYY.",
        ],
    },
    // =========================================================================
    // 41..=44: STAIRCASES & TERRACES (6 to 8 rows)
    // =========================================================================
    MotifTemplate {
        name: "ASCENDING_STAIRS",
        family: MotifFamily::Staircase,
        rows: &[
            "............ZZZ",
            "..........YYYYY",
            "........XXXXXXX",
            "......YYYYEYYYY",
            "....XXXXXXXXXXX",
            "..YYYYYEYYYYY..",
            "XXXXXXXXXXX....",
        ],
    },
    MotifTemplate {
        name: "DESCENDING_STAIRS",
        family: MotifFamily::Staircase,
        rows: &[
            "ZZZ............",
            "YYYYY..........",
            "XXXXXXX........",
            "YYYYEYYYY......",
            "XXXXXXXXXXX....",
            "..YYYYYEYYYYY..",
            "....XXXXXXXXXXX",
        ],
    },
    MotifTemplate {
        name: "ZIGGURAT_PYRAMID",
        family: MotifFamily::Staircase,
        rows: &[
            ".....ZZZZZ.....",
            "....YXXXXXY....",
            "...YXXXEXXXY...",
            "..YXXXXXXXXXY..",
            ".YXXXXEXXXXXXY.",
            "YYYYYYYYYYYYYYY",
        ],
    },
    MotifTemplate {
        name: "SWITCHBACK_RAMP",
        family: MotifFamily::Staircase,
        rows: &[
            "YYYYYYYYYYY....",
            "........YYYYYY.",
            "...YYYYYYYYY...",
            ".YYYYY.........",
            "...YYYYYEYYYYY.",
            "..........YYYYY",
            ".YYYYYYYYYYY...",
            "YYYYY..........",
        ],
    },
    // =========================================================================
    // 45..=48: SPIRALS & WAVES (5 to 9 rows, asymmetric)
    // =========================================================================
    MotifTemplate {
        name: "NAUTILUS_SPIRAL",
        family: MotifFamily::SpiralWave,
        rows: &[
            "YYYYYYYYYYYYYY.",
            "Y............Y.",
            "Y.XXXXXXXXXX.Y.",
            "Y.X........X.Y.",
            "Y.X.ZZEYYY.X.Y.",
            "Y.X.Z....Y.X.Y.",
            "Y.X.ZZZZZY.X.Y.",
            "Y.X........X.Y.",
            "Y.XXXXXXXXXXXY.",
        ],
    },
    MotifTemplate {
        name: "VORTEX_COIL",
        family: MotifFamily::SpiralWave,
        rows: &[
            "..YYYYYYYYYYY..",
            ".YY.........YY.",
            "YY..XXXXXXX..YY",
            "Y..XX..E..XX..Y",
            "Y..X..ZZZ..X..Y",
            "YY..XXXXXXX....",
            ".YYYYYYYYYYYYY.",
        ],
    },
    MotifTemplate {
        name: "SINE_WAVE",
        family: MotifFamily::SpiralWave,
        rows: &[
            "..YYYY.........",
            ".YY..YY.......Y",
            "YY....YY..E..YY",
            ".......YY...YY.",
            "........YYYYY..",
        ],
    },
    MotifTemplate {
        name: "TSUNAMI_CREST",
        family: MotifFamily::SpiralWave,
        rows: &[
            "...YYYYYYYY....",
            ".YYXXXXXXXXYY..",
            "YXXY....YXXXXY.",
            "YXX..ZZ..YXXEXY",
            ".YXX......YXXXY",
            "..YYYYYYYYYYYYY",
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use kinetix_core::level_code::EDITOR_COLS;
    use std::collections::BTreeSet;

    #[test]
    fn motif_library_has_at_least_40_unique_valid_templates_of_varied_size() {
        assert!(MOTIFS.len() >= 40);
        let mut names = BTreeSet::new();
        let mut row_counts = BTreeSet::new();
        let mut asymmetric_count = 0usize;

        for m in &MOTIFS {
            assert!(!m.name.is_empty());
            assert!(
                names.insert(m.name),
                "duplicate motif template name {}",
                m.name
            );
            assert!(
                (4..=9).contains(&m.rows.len()),
                "motif {} has {} rows",
                m.name,
                m.rows.len()
            );
            row_counts.insert(m.rows.len());
            let mut mismatch = 0usize;
            for r in m.rows {
                assert_eq!(
                    r.chars().count(),
                    EDITOR_COLS,
                    "motif {} row {r:?} is not {EDITOR_COLS} chars",
                    m.name
                );
                let chars: Vec<char> = r.chars().collect();
                mismatch += chars
                    .iter()
                    .zip(chars.iter().rev())
                    .filter(|(a, b)| a != b)
                    .count();
            }
            if mismatch >= 6 {
                asymmetric_count += 1;
            }
        }
        assert!(
            row_counts.len() >= 5,
            "expected varied row counts across motifs, got {row_counts:?}"
        );
        assert!(
            asymmetric_count >= 24,
            "expected at least 24 asymmetric motifs, got {asymmetric_count}"
        );
    }
}
