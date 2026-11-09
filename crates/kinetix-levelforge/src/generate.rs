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

//! Deterministic procedural campaign generator from curated motif families,
//! parameterized by world identity, sector position, and difficulty curve.

use crate::validate::{BOSS_SUFFIX, FINAL_BOSS_SUFFIX, MAX_LEVEL_NAME_LEN};
use kinetix_core::config::LevelDef;
use kinetix_core::level_code::EDITOR_COLS;
use std::collections::BTreeSet;

const WORLD_ADJECTIVES: [&[&str]; 8] = [
    &[
        "IGNITION", "SPARK", "EMBER", "PLASMA", "FLAME", "PYRO", "THERMAL", "TORCH", "BLAZE",
        "CINDER", "FUSION", "SOLAR", "RADIANT", "FURNACE", "BEACON", "KINDLE", "FLINT", "SCORCH",
        "HEAT", "GLOW",
    ],
    &[
        "QUANTUM", "WARP", "PHASE", "RIFT", "FLUX", "PORTAL", "CHRONO", "AETHER", "NEXUS",
        "PARADOX", "SHIFT", "SLIP", "TACHYON", "VECTOR", "PULSAR", "QUASAR", "DRIFT", "FOLD",
        "WAVE", "BLINK",
    ],
    &[
        "OMEGA", "ANVIL", "FORGE", "TITAN", "IRON", "COBALT", "PISTON", "ALLOY", "FOUNDRY",
        "HAMMER", "CRUCIBLE", "STEEL", "GIRDLE", "SMELTER", "BASTION", "RIVET", "TEMPER", "INGOT",
        "GEAR", "VALVE",
    ],
    &[
        "CRYO", "FROST", "GLACIER", "ARCTIC", "ZERO", "ICE", "SHARD", "TUNDRA", "HALO", "MIRROR",
        "POLAR", "BLIZZARD", "CRYSTAL", "WINTER", "RIME", "HOAR", "BOREAL", "CHILL", "FLOE",
        "SLEET",
    ],
    &[
        "MAGMA", "CALDERA", "HELIOS", "CORONA", "FLARE", "VOLCAN", "SULFUR", "MANTLE", "TECTONIC",
        "OBSIDIAN", "PYRE", "INFERNO", "CRATER", "NOVA", "ZENITH", "LAVA", "BASALT", "GEYSER",
        "PLUME", "ASH",
    ],
    &[
        "PRISM", "FACET", "QUARTZ", "OPAL", "SPECTRUM", "LATTICE", "REFRACT", "DIAMOND", "CHROMA",
        "MIRAGE", "OPTIC", "LUMEN", "BEVEL", "GEODE", "SAPPHIRE", "RUBY", "TOPAZ", "BERYL", "JADE",
        "ONYX",
    ],
    &[
        "VOID", "SHADOW", "PHANTOM", "ECLIPSE", "ABYSS", "UMBRA", "SPECTRE", "HOLLOW", "DUSK",
        "OBLIVION", "SHROUD", "WRAITH", "SILENT", "NIGHT", "HORIZON", "GLOOM", "NADIR", "PENUMBRA",
        "NULL", "VORTEX",
    ],
    &[
        "APEX",
        "THRONE",
        "CROWN",
        "ETERNAL",
        "SOVEREIGN",
        "ASTRAL",
        "COSMIC",
        "EMPYREAN",
        "HYPERION",
        "MONOLITH",
        "GENESIS",
        "CELESTIAL",
        "ARCHON",
        "VALHALLA",
        "ASCENDANT",
        "PRIME",
        "SUPREME",
        "INFINITE",
        "FINAL",
        "ULTIMA",
    ],
];

const NOUNS: &[&str] = &[
    "GRID", "GATE", "ARRAY", "CORE", "MATRIX", "VAULT", "RING", "CONDUIT", "CHAMBER", "SPIRE",
    "BRIDGE", "CITADEL", "SECTOR", "CANYON", "TUNNEL", "CASCADE", "ENGINE", "TURBINE", "PYLON",
    "SANCTUM", "LATTICE", "PILLAR", "CORRIDOR", "DOMINION", "THRONE", "ALTAR", "BULWARK", "SIGIL",
    "ORBIT", "CRADLE", "BATTERY", "CREST", "FRONTIER", "NEXUS", "TERMINAL", "FORTRESS", "KEEP",
    "ARENA", "WELL", "NODE",
];

// 16 distinct 6x8 half-motifs (mirrored across col 7 -> 15 cols wide).
// '.' is empty, 'X' is primary brick, 'Y' is secondary/accent brick.
const HALF_MOTIFS: [&[&str; 6]; 16] = [
    // 0: Pixel Heart
    &[
        "..XX.XX.", ".XXXXXXX", ".XXXXXXX", "..XXXXXX", "...XXXXX", "....XXXX",
    ],
    // 1: Pixel Skull / Visor
    &[
        "..XXXXXX", ".XXXXXXX", ".XX..XXX", ".XXXXXXX", "..X.X.XX", "..XX.XX.",
    ],
    // 2: Pixel Rocket
    &[
        "......XX", ".....XXX", "....XXXX", "...XX.XX", "..XXXXXX", ".XX..XXX",
    ],
    // 3: Pixel Crown
    &[
        ".X..X..X", ".XX.XX.X", ".XXXXXXX", ".X.XX.XX", ".XXXXXXX", "..XXXXXX",
    ],
    // 4: Space Invader
    &[
        "..X...X.", "...X.X..", "..XXXXXX", ".XX.XXXX", "XXXXXXXX", ".X.XXXX.",
    ],
    // 5: Pixel Sword / Blade
    &[
        "......XX", ".....XXX", "...XXXXX", "..XXXX..", ".XXXX...", "XXX.....",
    ],
    // 6: Ringed Planet
    &[
        "....XXXX", "...XXXXX", "XXXXXXXX", ".XXXXXXX", "...XXXXX", "....XXXX",
    ],
    // 7: Pixel Key / Ankh
    &[
        "...XXXX.", "..XX..XX", "...XXXX.", ".....XXX", "....XXXX", ".....XXX",
    ],
    // 8: Symmetric Mandala
    &[
        ".XX..XXX", "X..XX..X", "XXXXXXXX", ".XX..XXX", "X..XX..X", ".XXXXXXX",
    ],
    // 9: Checkerboard Band
    &[
        "X.X.X.X.", ".X.X.X.X", "XXXXXXXX", ".X.X.X.X", "X.X.X.X.", "XXXXXXXX",
    ],
    // 10: Double Chevron Arrow
    &[
        "X...X...", ".X...X..", "..X...X.", "...X...X", "....X..X", ".XXXXXXX",
    ],
    // 11: Staircase / Terraces
    &[
        "XX......", "XXXX....", "XXXXXX..", "..XXXXXX", "....XXXX", ".XXXXXXX",
    ],
    // 12: Twin Pillars / Corridors
    &[
        ".XX..XX.", ".XX..XX.", ".XXXXXXX", ".XX..XX.", ".XX..XX.", ".XXXXXXX",
    ],
    // 13: Hourglass
    &[
        ".XXXXXXX", "..XXXXXX", "...XXXXX", "...XXXXX", "..XXXXXX", ".XXXXXXX",
    ],
    // 14: Honeycomb Cells
    &[
        ".XX.XX.X", "X..X..X.", ".XX.XX.X", "X..X..X.", ".XX.XX.X", ".XXXXXXX",
    ],
    // 15: Concentric Citadel
    &[
        "XXXXXXXX", "X......X", "X.XXXX.X", "X.X..X.X", "X.XXXX.X", "XXXXXXXX",
    ],
];

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        Self(z.max(1))
    }

    fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }
}

/// Generates a unique uppercase name of at most `MAX_LEVEL_NAME_LEN` chars for a sector.
/// Boss names end with `BOSS_SUFFIX`, or `FINAL_BOSS_SUFFIX` for the last level of the pack.
pub fn generate_unique_name(
    world: usize,
    sector_in_world: usize,
    boss: bool,
    final_boss: bool,
    used: &mut BTreeSet<String>,
) -> String {
    let suffix = if final_boss {
        FINAL_BOSS_SUFFIX
    } else {
        BOSS_SUFFIX
    };
    let w_idx = world.saturating_sub(1).min(7);
    let adjs = WORLD_ADJECTIVES[w_idx];
    let mut attempt = 0usize;
    loop {
        let a = adjs[(sector_in_world * 3 + attempt * 7) % adjs.len()];
        let n = NOUNS[(sector_in_world * 5 + attempt * 11 + w_idx * 3) % NOUNS.len()];
        let candidate = if boss {
            let base = format!("{a} {n}{suffix}");
            if base.len() <= MAX_LEVEL_NAME_LEN {
                base
            } else {
                format!("{a}{suffix}")
            }
        } else if attempt == 0 {
            format!("{a} {n}")
        } else {
            let roman_or_num = attempt + 1;
            format!("{a} {n} {roman_or_num}")
        };
        if candidate.len() <= MAX_LEVEL_NAME_LEN && used.insert(candidate.clone()) {
            return candidate;
        }
        attempt += 1;
    }
}

/// Target destructible brick count for `(world, sector_in_world)`, rising smoothly within
/// each world, dipping slightly at the start of the next world, and never jumping by more
/// than ~15% between consecutive non-boss sectors.
pub fn target_brick_count(world: usize, sector_in_world: usize) -> usize {
    let base = 36.0 + (world.saturating_sub(1) as f32) * 2.5;
    let rise = ((sector_in_world.saturating_sub(1) as f32) / 99.0) * 16.0;
    let pos_in_block = (sector_in_world - 1) % 10;
    // Breather dip on positions 3 and 7 (sectors 4 and 8 of each 10-sector block),
    // small peak on set piece (position 9, sector 10).
    let wave = match pos_in_block {
        3 | 7 => -3.0,
        9 => 3.0,
        _ => 0.0,
    };
    (base + rise + wave).round().clamp(32.0, 68.0) as usize
}

/// Generates a single candidate level for `(id, world, sector_in_world)` where `sector_in_world`
/// is 1..=100. `is_last` marks the last level of the pack, named as the final boss.
pub fn generate_sector(
    id: usize,
    world: usize,
    sector_in_world: usize,
    is_last: bool,
    seed: u64,
    used_names: &mut BTreeSet<String>,
) -> LevelDef {
    let boss = sector_in_world == 50 || sector_in_world == 100;
    let is_set_piece = !boss && sector_in_world.is_multiple_of(10);
    let is_breather = !boss && (sector_in_world % 10 == 4 || sector_in_world % 10 == 8);
    let name = generate_unique_name(world, sector_in_world, boss, boss && is_last, used_names);
    let mut rng = Rng::new(seed ^ (id as u64).wrapping_mul(0x517C_C1B7_2722_0A95));

    if boss {
        let rows = build_boss_rows(world, sector_in_world, &mut rng);
        return LevelDef {
            id,
            world,
            name,
            boss: true,
            rows,
        };
    }

    let motif_idx =
        (sector_in_world * 3 + world * 5 + (rng.next_u32() as usize % 3)) % HALF_MOTIFS.len();
    let target_bricks = target_brick_count(world, sector_in_world);
    let rows = build_motif_rows(
        world,
        sector_in_world,
        motif_idx,
        target_bricks,
        is_breather,
        is_set_piece,
        &mut rng,
    );

    LevelDef {
        id,
        world,
        name,
        boss: false,
        rows,
    }
}

fn build_boss_rows(world: usize, sector_in_world: usize, rng: &mut Rng) -> Vec<String> {
    let mut rows = vec![
        "000000000000000".to_string(),
        "000000000000000".to_string(),
        "000000000000000".to_string(),
    ];
    let c1 = char::from_digit(((world as u32) % 4) + 1, 10).unwrap_or('2');
    let c2 = if world >= 4 || (world >= 2 && sector_in_world == 100) {
        '5'
    } else {
        '3'
    };
    let use_portal = world >= 2 || sector_in_world == 100;
    let use_ghost = world >= 2 && rng.next_u32().is_multiple_of(2);

    if world >= 8 {
        rows.push("00#EE00T00EE#00".to_string());
        rows.push(format!("{c1}E{c2}1E0###0E1{c2}E{c1}"));
        rows.push("0T01E10T01E10T0".to_string());
        return rows;
    }

    // Wide center corridors (cols 4..=10 open) and explosive clusters for power-up drops.
    let r3 = if use_portal {
        format!("T0{c1}EE00000EE{c1}0T")
    } else {
        format!("##{c1}EE00000EE{c1}##")
    };
    rows.push(r3);

    let mid_feat = if use_ghost { "G1G" } else { "1E1" };
    let r4 = format!("{c1}{mid_feat}{c2}0###0{c2}{mid_feat}{c1}");
    rows.push(r4);
    rows
}

fn build_motif_rows(
    world: usize,
    sector_in_world: usize,
    motif_idx: usize,
    target_bricks: usize,
    is_breather: bool,
    is_set_piece: bool,
    rng: &mut Rng,
) -> Vec<String> {
    let row_count = if target_bricks > 54 { 7 } else { 6 };
    let mut grid = vec![vec!['0'; EDITOR_COLS]; row_count];
    let progress = (sector_in_world as f32) / 100.0;
    let tough_chance =
        ((world as f32 - 1.0) * 0.045 + progress * 0.18) * if is_breather { 0.55 } else { 1.0 };

    let template = HALF_MOTIFS[motif_idx];
    let flip_v = (sector_in_world / 10).is_multiple_of(2);

    for (r, row_cells) in grid.iter_mut().enumerate().take(row_count) {
        let src_r = if r < 6 {
            if flip_v {
                5 - r
            } else {
                r
            }
        } else {
            0
        };
        let line = template[src_r].as_bytes();
        for (c, &b) in line.iter().enumerate().take(8) {
            if b != b'.' {
                let is_tough = ((rng.next_u32() % 100) as f32 / 100.0) < tough_chance;
                let digit = if is_tough {
                    if (r + c).is_multiple_of(2) {
                        '5'
                    } else {
                        '6'
                    }
                } else {
                    let d = ((r + c + world) % 4) as u32 + 1;
                    char::from_digit(d, 10).unwrap_or('1')
                };
                row_cells[c] = digit;
                row_cells[EDITOR_COLS - 1 - c] = digit;
            }
        }
    }

    // Adjust destructible count gently toward `target_bricks` by adding or removing symmetric pairs.
    let count_destructible = |g: &[Vec<char>]| -> usize {
        g.iter()
            .flat_map(|r| r.iter())
            .filter(|&&ch| matches!(ch, '1'..='6' | 'E' | 'G' | 'M' | 'B' | 'R'))
            .count()
    };

    for r in (0..row_count).rev() {
        for c in (1..=6).rev() {
            let cur = count_destructible(&grid);
            if cur >= target_bricks {
                break;
            }
            if grid[r][c] == '0' {
                let d = char::from_digit(((r + c + world) % 4) as u32 + 1, 10).unwrap_or('2');
                grid[r][c] = d;
                grid[r][EDITOR_COLS - 1 - c] = d;
            }
        }
    }
    for r in 0..row_count.saturating_sub(1) {
        for c in 0..=6 {
            let cur = count_destructible(&grid);
            if cur <= target_bricks + 2 {
                break;
            }
            if grid[r][c] != '0' {
                grid[r][c] = '0';
                grid[r][EDITOR_COLS - 1 - c] = '0';
            }
        }
    }

    // Place explosive chain cores ('E') so hits trigger satisfying chain reactions.
    let mid_r = row_count / 2;
    grid[mid_r][7] = 'E';
    grid[1][7] = 'E';
    if is_set_piece || progress > 0.25 {
        grid[mid_r][3] = 'E';
        grid[mid_r][11] = 'E';
    }
    if world >= 5 || is_set_piece {
        grid[row_count - 2][5] = 'E';
        grid[row_count - 2][9] = 'E';
    }

    // World mechanics and teaching blocks:
    // Each mechanic appears in an open layout at the start of its featured block, then combines.
    let teaching_bumper = world == 5 && sector_in_world <= 10;
    let teaching_regen = world == 6 && sector_in_world <= 10;
    let in_teaching = teaching_bumper || teaching_regen;
    let allow_steel = !(world == 1 && sector_in_world < 18) && !in_teaching;
    let allow_portal = (world >= 2 || sector_in_world >= 22) && !in_teaching;
    let allow_ghost = (world >= 2 || sector_in_world >= 35) && !in_teaching;
    let allow_moving =
        (world >= 3 || (world == 2 && sector_in_world >= 15) || sector_in_world >= 55)
            && !in_teaching;

    // Steel '#' bumpers (never sealing)
    if allow_steel && !is_breather && row_count >= 5 {
        let sr = row_count - 2;
        grid[sr][1] = '#';
        grid[sr][13] = '#';
        if world >= 3 && motif_idx.is_multiple_of(2) {
            grid[2][4] = '#';
            grid[2][10] = '#';
        }
    }

    // Portals 'T' (always in symmetric left/right pairs -> even count)
    if allow_portal && (!motif_idx.is_multiple_of(2) || is_set_piece || world == 2 || world == 6) {
        grid[0][0] = 'T';
        grid[0][14] = 'T';
        if (world == 2 || world >= 6) && is_set_piece && row_count >= 6 {
            grid[row_count - 2][0] = 'T';
            grid[row_count - 2][14] = 'T';
        }
    }

    // Ghost 'G' rows/clusters (featured in World 4 & World 7)
    if allow_ghost && (world == 4 || world == 7 || motif_idx.is_multiple_of(3)) && row_count >= 4 {
        let gr = 1;
        for c in [4, 5, 6, 8, 9, 10] {
            if grid[gr][c] != '0' {
                grid[gr][c] = 'G';
            }
        }
    }

    // Moving 'M' conveyor rows (featured in World 3 & World 5+)
    if allow_moving && (world == 3 || world >= 5 || motif_idx.is_multiple_of(4)) && row_count >= 5 {
        let mr = row_count / 2;
        for cell in grid[mr].iter_mut().take(EDITOR_COLS) {
            *cell = '0';
        }
        for c in [2, 3, 4, 10, 11, 12] {
            grid[mr][c] = 'M';
        }
        grid[mr][7] = 'E';
    }

    // Bumper 'B' bricks (introduced in World 5 with a teaching block in sectors 1..=10)
    if world >= 5
        && row_count >= 5
        && (world == 5 || (sector_in_world + motif_idx).is_multiple_of(2))
    {
        let br = row_count - 2;
        grid[br][3] = 'B';
        grid[br][11] = 'B';
        if teaching_bumper || is_set_piece {
            grid[2][5] = 'B';
            grid[2][9] = 'B';
        }
    }

    // Regenerating 'R' bricks (introduced in World 6 with a teaching block in sectors 1..=10)
    if world >= 6
        && row_count >= 5
        && (world == 6 || (sector_in_world + motif_idx).is_multiple_of(3))
    {
        let rr = 2;
        grid[rr][6] = 'R';
        grid[rr][8] = 'R';
        if teaching_regen || is_set_piece {
            grid[row_count - 2][4] = 'R';
            grid[row_count - 2][10] = 'R';
        }
    }

    // Ensure bottom row has destructible bricks so the ball hits immediately on launch.
    let last_r = row_count - 1;
    let mut bottom_destructible = 0;
    for &ch in grid[last_r].iter().take(EDITOR_COLS) {
        if matches!(ch, '1'..='6' | 'E' | 'G' | 'M' | 'B' | 'R') {
            bottom_destructible += 1;
        }
    }
    if bottom_destructible < 6 {
        for c in [2, 3, 6, 7, 8, 11, 12] {
            grid[last_r][c] = '1';
        }
    }

    grid.into_iter()
        .map(|r| r.into_iter().collect::<String>())
        .collect()
}
