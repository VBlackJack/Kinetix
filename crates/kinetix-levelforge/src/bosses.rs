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

//! Hand-authored boss arenas for all 16 generated boss sectors (sectors 50 and 100 of
//! Worlds 1 to 8). Each boss has a distinct structural layout, row footprint, and mechanic
//! focus while keeping wide shot channels to the boss prowler in rows 0..=2.

/// Hand-authored boss arena definition.
#[derive(Debug, Clone, Copy)]
pub struct BossArena {
    pub world: usize,
    pub sector_in_world: usize,
    pub title_stem: &'static str,
    pub rows: &'static [&'static str],
}

pub const BOSS_ARENAS: [BossArena; 16] = [
    // =========================================================================
    // WORLD 1: IGNITION GRID (Standard + Explosive + Steel)
    // =========================================================================
    // Sector 50: Twin Pyre Bastions (6 rows, symmetric bastions with open center lane)
    BossArena {
        world: 1,
        sector_in_world: 50,
        title_stem: "PYRE WARDEN",
        rows: &[
            "000000000000000",
            "000000000000000",
            "000000000000000",
            "0#12E21012E21#0",
            "002353202353200",
            "0001E10001E1000",
        ],
    },
    // Sector 100: Furnace Core Citadel (7 rows, stepped V-funnel with explosive heart)
    BossArena {
        world: 1,
        sector_in_world: 100,
        title_stem: "SOLAR TITAN",
        rows: &[
            "000000000000000",
            "000000000000000",
            "000000000000000",
            "33E000000000E33",
            "045400#E#004540",
            "002E2016102E200",
            "0001120E0211000",
        ],
    },
    // =========================================================================
    // WORLD 2: QUANTUM WARP (Ghost silhouettes + Phase lanes)
    // =========================================================================
    // Sector 150 (W2 S50): Asymmetric Phantom Diagonal (7 rows, diagonal ghost screen)
    BossArena {
        world: 2,
        sector_in_world: 50,
        title_stem: "PHASE REX",
        rows: &[
            "000000000000000",
            "000000000000000",
            "000000000000000",
            "GG2E20000000#00",
            "0GG33200001E100",
            "000GG4E40233200",
            "000001221000000",
        ],
    },
    // Sector 200 (W2 S100): Twin Ghost Arches with Warp Flanks (8 rows, 2 portals)
    BossArena {
        world: 2,
        sector_in_world: 100,
        title_stem: "RIFT ARCHON",
        rows: &[
            "000000000000000",
            "000000000000000",
            "000000000000000",
            "T000GGEEEGG000T",
            "00#2G00000G2#00",
            "0023E00000E3200",
            "000122000221000",
            "000001E1E100000",
        ],
    },
    // =========================================================================
    // WORLD 3: OMEGA FORGE (Moving conveyors + Steel anvil pillars)
    // =========================================================================
    // Sector 250 (W3 S50): Dual Conveyor Foundry (7 rows, staggered M rows)
    BossArena {
        world: 3,
        sector_in_world: 50,
        title_stem: "ANVIL LORD",
        rows: &[
            "000000000000000",
            "000000000000000",
            "000000000000000",
            "00MMM00E00MMM00",
            "0#00025E52000#0",
            "0001230#0321000",
            "00MMM00E0000000",
        ],
    },
    // Sector 300 (W3 S100): Crucible Piston Ring (8 rows, asymmetric forge bridge)
    BossArena {
        world: 3,
        sector_in_world: 100,
        title_stem: "IRON TYRANT",
        rows: &[
            "000000000000000",
            "000000000000000",
            "000000000000000",
            "0#5E5000003E3#0",
            "006G60000024200",
            "000000MMM000000",
            "002E200#004E400",
            "0011000E0001110",
        ],
    },
    // =========================================================================
    // WORLD 4: CRYO MATRIX (Paired Portals + Frost Lattice)
    // =========================================================================
    // Sector 350 (W4 S50): Asymmetric Cryo Warp Bridge (7 rows, 2 portals + upper E crown)
    BossArena {
        world: 4,
        sector_in_world: 50,
        title_stem: "CRYO WARDEN",
        rows: &[
            "000000000000000",
            "000000000000000",
            "000000000000000",
            "T02E202E2035300",
            "00#3200#002E2#0",
            "001E102G201210T",
            "0001101E1000000",
        ],
    },
    // Sector 400 (W4 S100): Boreal Mirror Sanctum (9 rows, deep diamond portal lattice)
    BossArena {
        world: 4,
        sector_in_world: 100,
        title_stem: "FROST KING",
        rows: &[
            "000000000000000",
            "000000000000000",
            "000000000000000",
            "0T00002E20000T0",
            "0000035E5300000",
            "00#002G1G200#00",
            "0000003E3000000",
            "0001200E0021000",
            "000011000110000",
        ],
    },
    // =========================================================================
    // WORLD 5: SOLAR CITADEL (Bumper pinball bays + Explosive chains)
    // =========================================================================
    // Sector 450 (W5 S50): Pinball Wing Pockets (7 rows, upper bumper reflectors)
    BossArena {
        world: 5,
        sector_in_world: 50,
        title_stem: "MAGMA REX",
        rows: &[
            "000000000000000",
            "000000000000000",
            "000000000000000",
            "0B02E20002E20B0",
            "000353000353000",
            "001E100B001E100",
            "000220000022000",
        ],
    },
    // Sector 500 (W5 S100): Helios Caldera (8 rows, volcanic side towers + center B pin)
    BossArena {
        world: 5,
        sector_in_world: 100,
        title_stem: "NOVA ARCHON",
        rows: &[
            "000000000000000",
            "000000000000000",
            "000000000000000",
            "T1E2B00000B2E10",
            "025E000B000E520",
            "0#21001E10012#T",
            "0000012E2100000",
            "0000001E1000000",
        ],
    },
    // =========================================================================
    // WORLD 6: PRISM LABYRINTH (Regenerating shields + Crystal facets)
    // =========================================================================
    // Sector 550 (W6 S50): Regenerating Guard Posts (7 rows, inner prism pockets)
    BossArena {
        world: 6,
        sector_in_world: 50,
        title_stem: "OPAL WARDEN",
        rows: &[
            "000000000000000",
            "000000000000000",
            "000000000000000",
            "T002ER000RE200T",
            "0001E20B02E1000",
            "#0023E1E1E3200#",
            "0000121E1210000",
        ],
    },
    // Sector 600 (W6 S100): Prism Reliquary (8 rows, twin prism columns + center E bridge)
    BossArena {
        world: 6,
        sector_in_world: 100,
        title_stem: "PRISM LORD",
        rows: &[
            "000000000000000",
            "000000000000000",
            "000000000000000",
            "T02ER00000RE20T",
            "001E200B002E100",
            "0#23E01E10E32#0",
            "000120121021000",
            "0000001E1000000",
        ],
    },
    // =========================================================================
    // WORLD 7: VOID HORIZON (Multi-mechanic synthesis)
    // =========================================================================
    // Sector 650 (W7 S50): Eclipse Bastion (9 rows, asymmetric void crescent)
    BossArena {
        world: 7,
        sector_in_world: 50,
        title_stem: "VOID ARCHON",
        rows: &[
            "000000000000000",
            "000000000000000",
            "000000000000000",
            "T12EG000000000T",
            "001E200B00RE200",
            "#002E01E102E10#",
            "0001201210E3200",
            "0000001E1012000",
            "0000001E1000000",
        ],
    },
    // Sector 700 (W7 S100): Abyssal Horizon Gate (10 rows, outer monoliths + central obelisk)
    BossArena {
        world: 7,
        sector_in_world: 100,
        title_stem: "ABYSS KING",
        rows: &[
            "000000000000000",
            "000000000000000",
            "000000000000000",
            "T2E2000B0002E2T",
            "03G300000003R30",
            "#2E2001E1002E2#",
            "002000020000200",
            "00E0001E1000E00",
            "002000121000200",
            "0000001E1000000",
        ],
    },
    // =========================================================================
    // WORLD 8: SINGULARITY THRONE (Endgame gauntlet + Final Boss)
    // =========================================================================
    // Sector 750 (W8 S50): Astral Sovereign Vanguard (9 rows, inner twin spires + center spine)
    BossArena {
        world: 8,
        sector_in_world: 50,
        title_stem: "APEX WARDEN",
        rows: &[
            "000000000000000",
            "000000000000000",
            "000000000000000",
            "T002ER0B0GE200T",
            "0001E20002E1000",
            "#002311E113200#",
            "0001E10201E1000",
            "0000201E1010000",
            "0000001E1000000",
        ],
    },
    // Sector 800 (W8 S100): Singularity Throne [FINAL BOSS] (10 rows, grand crown arena)
    BossArena {
        world: 8,
        sector_in_world: 100,
        title_stem: "APEX ARCHON",
        rows: &[
            "000000000000000",
            "000000000000000",
            "000000000000000",
            "T02E200B002E20T",
            "003R3000003G300",
            "0#2E2000002E2#0",
            "000000MMM000000",
            "001E100E001E100",
            "000200151002000",
            "0000001E1000000",
        ],
    },
];

/// Looks up the hand-authored boss arena for `(world, sector_in_world)`.
pub fn boss_arena(world: usize, sector_in_world: usize) -> Option<&'static BossArena> {
    BOSS_ARENAS
        .iter()
        .find(|b| b.world == world && b.sector_in_world == sector_in_world)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::{
        cell_similarity, check_reachability, is_destructible_cell, BOSS_CLEAR_ROWS,
        BOSS_DENSITY_MAX, BOSS_DENSITY_MIN, BOSS_STEM_MAX_LEN, SIMILARITY_MAX,
    };
    use kinetix_core::config::LevelPack;
    use kinetix_core::level_code::{EDITOR_COLS, EDITOR_ROWS};

    fn shape_grid_from_rows(rows: &[String]) -> [u8; EDITOR_ROWS * EDITOR_COLS] {
        let mut grid = [b'0'; EDITOR_ROWS * EDITOR_COLS];
        for (r, row) in rows.iter().take(EDITOR_ROWS).enumerate() {
            for (c, ch) in row.chars().take(EDITOR_COLS).enumerate() {
                let b = ch as u8;
                grid[r * EDITOR_COLS + c] = if (b'1'..=b'6').contains(&b) { b'1' } else { b };
            }
        }
        grid
    }

    #[test]
    fn all_16_boss_arenas_satisfy_density_portals_clearance_reachability_and_similarity() {
        let raw_legacy = include_str!("../../../assets/config/levels.json");
        let legacy: LevelPack = serde_json::from_str(raw_legacy).expect("levels.json");
        let mut shapes: Vec<(String, [u8; EDITOR_ROWS * EDITOR_COLS])> = legacy
            .levels
            .iter()
            .take(15)
            .filter(|l| l.boss)
            .map(|l| (l.name.clone(), shape_grid_from_rows(&l.rows)))
            .collect();

        for world in 1..=8 {
            for sector in [50, 100] {
                let arena = boss_arena(world, sector)
                    .unwrap_or_else(|| panic!("missing boss arena for W{world} S{sector}"));
                assert!(
                    arena.title_stem.len() <= BOSS_STEM_MAX_LEN,
                    "stem {:?} exceeds {BOSS_STEM_MAX_LEN}",
                    arena.title_stem
                );
                let rows: Vec<String> = arena.rows.iter().map(|s| (*s).to_string()).collect();
                assert!((5..=EDITOR_ROWS).contains(&rows.len()));
                for r in &rows[..BOSS_CLEAR_ROWS] {
                    assert!(r.chars().all(|c| c == '0'));
                }
                let mut destructible = 0usize;
                let mut portals = 0usize;
                for r in &rows {
                    assert_eq!(r.chars().count(), EDITOR_COLS);
                    for ch in r.chars() {
                        if is_destructible_cell(ch) {
                            destructible += 1;
                        }
                        if ch == 'T' {
                            portals += 1;
                        }
                    }
                }
                assert!(
                    (BOSS_DENSITY_MIN..=BOSS_DENSITY_MAX).contains(&destructible),
                    "W{world} S{sector} has {destructible} destructible bricks"
                );
                assert!(
                    portals == 0 || portals == 2,
                    "W{world} S{sector} has {portals} portals"
                );
                check_reachability(&rows)
                    .unwrap_or_else(|e| panic!("W{world} S{sector} unreachable: {e}"));

                let shape = shape_grid_from_rows(&rows);
                let label = format!("W{world} S{sector} ({})", arena.title_stem);
                for (prev_label, prev_shape) in &shapes {
                    let sim = cell_similarity(&shape, prev_shape);
                    assert!(
                        sim <= SIMILARITY_MAX,
                        "{label} has shape similarity {sim:.2} with {prev_label} (max {SIMILARITY_MAX})"
                    );
                }
                shapes.push((label, shape));
            }
        }
    }
}
