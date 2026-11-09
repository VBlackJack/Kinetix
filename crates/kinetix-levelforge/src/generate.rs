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

//! Deterministic campaign level generator combining a 48-template hand-authored motif library
//! (`motifs.rs`), 16 hand-authored boss arenas (`bosses.rs`), and world-specific structural
//! passes (steel corridors, explosive cages, paired portal shortcuts, ghost silhouettes,
//! moving conveyor lanes, domino explosive fuses, bumper pinball bays, and regenerating shields).

use crate::bosses::boss_arena;
use crate::motifs::MOTIFS;
use crate::validate::{
    is_asymmetric, is_destructible_cell, BOSS_STEM_MAX_LEN, BOSS_SUFFIX, DENSITY_MAX, DENSITY_MIN,
    FINAL_BOSS_SUFFIX, MAX_LEVEL_NAME_LEN,
};
use kinetix_core::config::LevelDef;
use kinetix_core::level_code::{EDITOR_COLS, EDITOR_ROWS};
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

/// Name candidates tried for one sector before giving up.
const NAME_ATTEMPT_BUDGET: usize = 500;
/// Minimum row count of a generated regular level.
const MIN_REGULAR_ROWS: usize = 4;
/// Maximum row count of a generated regular level.
const MAX_REGULAR_ROWS: usize = EDITOR_ROWS;
/// Minimum destructible bricks kept after structural passes (with safety margin inside 30..=80).
const SAFE_DENSITY_MIN: usize = DENSITY_MIN + 2;
/// Maximum destructible bricks kept after structural passes (with safety margin inside 30..=80).
const SAFE_DENSITY_MAX: usize = DENSITY_MAX - 4;
/// Mid-boss sector index within each 100-sector world.
const MID_BOSS_SECTOR: usize = 50;
/// Final boss sector index within each 100-sector world.
const WORLD_BOSS_SECTOR: usize = 100;
/// Sectors per block in a world.
const BLOCK_SIZE: usize = 10;
/// Seed mixing constant for per-level RNG initialization.
const LEVEL_SEED_SALT: u64 = 0x517C_C1B7_2722_0A95;
/// SplitMix64 golden ratio increment.
const RNG_GOLDEN_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
/// SplitMix64 first multiplier.
const RNG_MIX_MUL_A: u64 = 0xBF58_476D_1CE4_E5B9;
/// SplitMix64 second multiplier.
const RNG_MIX_MUL_B: u64 = 0x94D0_49BB_1331_11EB;
/// Xorshift* output multiplier.
const RNG_STAR_MUL: u64 = 0x2545_F491_4F6C_DD1D;

#[derive(Clone)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        let mut z = seed.wrapping_add(RNG_GOLDEN_GAMMA);
        z = (z ^ (z >> 30)).wrapping_mul(RNG_MIX_MUL_A);
        z = (z ^ (z >> 27)).wrapping_mul(RNG_MIX_MUL_B);
        z ^= z >> 31;
        Self(z.max(1))
    }

    fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(RNG_STAR_MUL) >> 32) as u32
    }

    fn usize_below(&mut self, bound: usize) -> usize {
        if bound <= 1 {
            0
        } else {
            (self.next_u32() as usize) % bound
        }
    }
}

/// Structural pass applied on top of a placed motif.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StructurePass {
    /// Open teaching or breather layout with clean angles.
    OpenShowcase,
    /// Indestructible `#` side tunnels or angled baffles that force bank shots.
    SteelCorridors,
    /// Partial `#` and tough-brick cage protecting an internal `E` chain core.
    ExplosiveCage,
    /// Exactly two `T` portals creating a shortcut around a barrier.
    PortalShortcut,
    /// Ghost `G` silhouette masking a inner pattern.
    GhostSilhouette,
    /// Horizontal conveyor row of `M` bricks with open travel space.
    ConveyorLane,
    /// Diagonal or stepped fuse of `E` bricks that chain-detonate across rows.
    DominoFuse,
    /// Upper/mid `B` bumper pinball bay surrounded by scoring targets.
    BumperPinballBay,
    /// Regenerating `R` shield line guarding key bricks behind it.
    RegenShieldWall,
}

/// Generates a unique uppercase name of at most `MAX_LEVEL_NAME_LEN` chars for a sector.
/// Boss names use the hand-authored boss title stem when available and end with `BOSS_SUFFIX`,
/// or `FINAL_BOSS_SUFFIX` for the last level of the pack; their stem is at most
/// `BOSS_STEM_MAX_LEN` chars and never depends on the suffix.
pub fn generate_unique_name(
    world: usize,
    sector_in_world: usize,
    boss: bool,
    final_boss: bool,
    used: &mut BTreeSet<String>,
) -> Result<String, String> {
    let suffix = if final_boss {
        FINAL_BOSS_SUFFIX
    } else {
        BOSS_SUFFIX
    };
    if boss {
        if let Some(arena) = boss_arena(world, sector_in_world) {
            let candidate = format!("{}{suffix}", arena.title_stem);
            if arena.title_stem.len() <= BOSS_STEM_MAX_LEN
                && candidate.len() <= MAX_LEVEL_NAME_LEN
                && used.insert(candidate.clone())
            {
                return Ok(candidate);
            }
        }
    }

    let w_idx = world.saturating_sub(1).min(WORLD_ADJECTIVES.len() - 1);
    let adjs = WORLD_ADJECTIVES[w_idx];
    for attempt in 0..NAME_ATTEMPT_BUDGET {
        let a = adjs[(sector_in_world * 3 + attempt * 7 + w_idx * 5) % adjs.len()];
        let n = NOUNS[(sector_in_world * 5 + attempt * 11 + w_idx * 3) % NOUNS.len()];
        let candidate = if boss {
            let stem = format!("{a} {n}");
            if stem.len() <= BOSS_STEM_MAX_LEN {
                format!("{stem}{suffix}")
            } else {
                format!("{a}{suffix}")
            }
        } else if attempt == 0 {
            format!("{a} {n}")
        } else {
            let num = attempt + 1;
            format!("{a} {n} {num}")
        };
        if candidate.len() <= MAX_LEVEL_NAME_LEN && used.insert(candidate.clone()) {
            return Ok(candidate);
        }
    }
    Err(format!(
        "no unique level name found for world {world}, sector {sector_in_world} after {NAME_ATTEMPT_BUDGET} attempts"
    ))
}

/// Target destructible brick count for `(world, sector_in_world)`.
/// Starts light in Block 1 of each world (teaching block) and rises across blocks 1..=10,
/// with breather dips on sectors 4 and 8 of each block.
pub fn target_brick_count(world: usize, sector_in_world: usize) -> usize {
    let block_idx = sector_in_world.saturating_sub(1) / BLOCK_SIZE;
    let pos_in_block = sector_in_world.saturating_sub(1) % BLOCK_SIZE;
    let world_base = 34.0 + (world.saturating_sub(1) as f32) * 1.4;
    let block_rise = (block_idx as f32) * 2.8;
    let wave = match pos_in_block {
        0 => -4.0,
        3 | 7 => -5.0,
        9 => 3.0,
        _ => 0.0,
    };
    (world_base + block_rise + wave)
        .round()
        .clamp(SAFE_DENSITY_MIN as f32, SAFE_DENSITY_MAX as f32) as usize
}

/// Target good-bot clear rate (`0.0..=1.0`) for `(world, sector_in_world)` used by
/// `cmd_generate` to select candidates that form a smooth non-increasing block curve
/// within each world and a clear breather reset at the start of the next world.
pub fn target_good_clear_rate(world: usize, sector_in_world: usize) -> f32 {
    let block_idx = sector_in_world.saturating_sub(1) / BLOCK_SIZE;
    let pos_in_block = sector_in_world.saturating_sub(1) % BLOCK_SIZE;
    // World 1 prologue (sectors 1..=15) measures 65.6% good clear rate, so World 1 blocks
    // step smoothly from 0.65 down to 0.46; Worlds 2..=8 start high (0.88) in Block 1
    // (teaching block) and step down ~0.04 per block to ~0.50 in Block 10.
    let (start_rate, step_per_block) = if world == 1 {
        (0.68, 0.024)
    } else {
        let w_offset = (world.saturating_sub(2) as f32) * 0.006;
        (0.88 - w_offset, 0.040)
    };
    let block_target = start_rate - (block_idx as f32) * step_per_block;
    let breather_bonus = if pos_in_block == 3 || pos_in_block == 7 {
        0.08
    } else if pos_in_block == 9 {
        -0.04
    } else {
        0.0
    };
    (block_target + breather_bonus).clamp(0.42, 0.95)
}

/// Generates a single candidate level for `(id, world, sector_in_world)` where `sector_in_world`
/// is `1..=100`. `is_last` marks the last level of the pack, named as the final boss.
pub fn generate_sector(
    id: usize,
    world: usize,
    sector_in_world: usize,
    is_last: bool,
    seed: u64,
    used_names: &mut BTreeSet<String>,
) -> Result<LevelDef, String> {
    let boss = sector_in_world == MID_BOSS_SECTOR || sector_in_world == WORLD_BOSS_SECTOR;
    let name = generate_unique_name(world, sector_in_world, boss, boss && is_last, used_names)?;

    if boss {
        let arena = boss_arena(world, sector_in_world).ok_or_else(|| {
            format!("missing hand-authored boss arena for world {world} sector {sector_in_world}")
        })?;
        let rows = arena.rows.iter().map(|r| (*r).to_string()).collect();
        return Ok(LevelDef {
            id,
            world,
            name,
            boss: true,
            rows,
        });
    }

    let mut rng = Rng::new(seed ^ (id as u64).wrapping_mul(LEVEL_SEED_SALT));
    let rows = build_regular_rows(world, sector_in_world, &mut rng);

    Ok(LevelDef {
        id,
        world,
        name,
        boss: false,
        rows,
    })
}

/// Chooses the structural pass for `(world, sector_in_world)` following the "teach before
/// testing" progression:
/// - World 1: SteelCorridors, ExplosiveCage, DominoFuse, OpenShowcase
/// - World 2: introduces GhostSilhouette (teaching block S1-10)
/// - World 3: introduces ConveyorLane (teaching block S1-10)
/// - World 4: introduces PortalShortcut (teaching block S1-10)
/// - World 5: introduces BumperPinballBay (teaching block S1-10)
/// - World 6: introduces RegenShieldWall (teaching block S1-10)
/// - Worlds 7-8: combine all structural passes
fn choose_structure_pass(world: usize, sector_in_world: usize, rng: &mut Rng) -> StructurePass {
    let block_idx = sector_in_world.saturating_sub(1) / BLOCK_SIZE;
    let pos_in_block = sector_in_world.saturating_sub(1) % BLOCK_SIZE;
    let is_breather = pos_in_block == 3 || pos_in_block == 7;

    // Block 1 (sectors 1..=10) is the teaching block of each world.
    if block_idx == 0 {
        return match world {
            1 => StructurePass::OpenShowcase,
            2 => StructurePass::GhostSilhouette,
            3 => StructurePass::ConveyorLane,
            4 => StructurePass::PortalShortcut,
            5 => StructurePass::BumperPinballBay,
            6 => StructurePass::RegenShieldWall,
            _ => StructurePass::DominoFuse,
        };
    }

    if is_breather {
        return if rng.usize_below(2) == 0 {
            StructurePass::DominoFuse
        } else {
            StructurePass::OpenShowcase
        };
    }

    let roll = (sector_in_world + rng.usize_below(7)) % 7;
    match world {
        1 => match roll % 4 {
            0 => StructurePass::SteelCorridors,
            1 => StructurePass::ExplosiveCage,
            2 => StructurePass::DominoFuse,
            _ => StructurePass::OpenShowcase,
        },
        2 => match roll % 5 {
            0 | 1 => StructurePass::GhostSilhouette,
            2 => StructurePass::SteelCorridors,
            3 => StructurePass::ExplosiveCage,
            _ => StructurePass::DominoFuse,
        },
        3 => match roll % 6 {
            0 | 1 => StructurePass::ConveyorLane,
            2 => StructurePass::SteelCorridors,
            3 => StructurePass::GhostSilhouette,
            4 => StructurePass::ExplosiveCage,
            _ => StructurePass::DominoFuse,
        },
        4 => match roll % 6 {
            0 | 1 => StructurePass::PortalShortcut,
            2 => StructurePass::SteelCorridors,
            3 => StructurePass::GhostSilhouette,
            4 => StructurePass::ConveyorLane,
            _ => StructurePass::ExplosiveCage,
        },
        5 => match roll % 7 {
            0 | 1 => StructurePass::BumperPinballBay,
            2 => StructurePass::PortalShortcut,
            3 => StructurePass::ConveyorLane,
            4 => StructurePass::ExplosiveCage,
            5 => StructurePass::GhostSilhouette,
            _ => StructurePass::DominoFuse,
        },
        6 => match roll % 7 {
            0 | 1 => StructurePass::RegenShieldWall,
            2 => StructurePass::PortalShortcut,
            3 => StructurePass::BumperPinballBay,
            4 => StructurePass::GhostSilhouette,
            5 => StructurePass::ConveyorLane,
            _ => StructurePass::ExplosiveCage,
        },
        _ => match roll {
            0 => StructurePass::RegenShieldWall,
            1 => StructurePass::PortalShortcut,
            2 => StructurePass::BumperPinballBay,
            3 => StructurePass::ConveyorLane,
            4 => StructurePass::GhostSilhouette,
            5 => StructurePass::ExplosiveCage,
            _ => StructurePass::SteelCorridors,
        },
    }
}

/// Parameters passed to the structural overlay pass.
struct PassContext {
    pass: StructurePass,
    world: usize,
    sector_in_world: usize,
    is_teaching_block: bool,
    want_asymmetric: bool,
}

fn build_regular_rows(world: usize, sector_in_world: usize, rng: &mut Rng) -> Vec<String> {
    let block_idx = sector_in_world.saturating_sub(1) / BLOCK_SIZE;
    let pos_in_block = sector_in_world.saturating_sub(1) % BLOCK_SIZE;
    let is_teaching_block = block_idx == 0;
    let is_breather = pos_in_block == 3 || pos_in_block == 7;
    let is_set_piece = pos_in_block == 9;

    // Make ~60% of sectors intentionally asymmetric so every world easily exceeds 30%.
    let want_asymmetric = !sector_in_world.is_multiple_of(3) || rng.usize_below(3) != 0;

    // Pick a motif from the 48-template library; vary choice by world, sector, and RNG.
    let motif_idx =
        (world * 7 + sector_in_world * 5 + rng.usize_below(MOTIFS.len())) % MOTIFS.len();
    let motif = &MOTIFS[motif_idx];

    // Vary row count between 4 and 10 rows based on motif height, block progression, and RNG.
    let base_rows = motif.rows.len();
    let extra_rows = if is_teaching_block || is_breather {
        rng.usize_below(2)
    } else if is_set_piece || block_idx >= 6 {
        1 + rng.usize_below(3)
    } else {
        rng.usize_below(3)
    };
    let row_count = (base_rows + extra_rows).clamp(MIN_REGULAR_ROWS, MAX_REGULAR_ROWS);
    let top_pad = if row_count > base_rows {
        rng.usize_below(row_count - base_rows + 1)
    } else {
        0
    };

    // Horizontal shift and optional flip to diversify footprint and shape.
    let flip_h = rng.usize_below(2) == 1;
    let flip_v = !is_teaching_block && rng.usize_below(3) == 1;
    let shift_x: isize = if want_asymmetric {
        let mag = 1 + (rng.usize_below(3) as isize);
        if rng.usize_below(2) == 0 {
            mag
        } else {
            -mag
        }
    } else {
        0
    };

    // Colour gradient palette for this sector.
    let base_color = ((world + sector_in_world + rng.usize_below(4)) % 4) as u32 + 1;
    let sec_color = ((base_color as usize + 1 + rng.usize_below(3)) % 4) as u32 + 1;
    let tough_chance = if is_teaching_block {
        0.04
    } else if is_breather {
        0.08 + (block_idx as f32) * 0.015
    } else {
        0.10 + (block_idx as f32) * 0.032 + (world.saturating_sub(1) as f32) * 0.018
    };

    let mut grid = vec![vec!['0'; EDITOR_COLS]; row_count];

    for (r_idx, row_cells) in grid.iter_mut().enumerate() {
        if r_idx < top_pad || r_idx >= top_pad + base_rows {
            continue;
        }
        let m_row_idx = r_idx - top_pad;
        let src_r = if flip_v {
            base_rows - 1 - m_row_idx
        } else {
            m_row_idx
        };
        let src_chars: Vec<char> = motif.rows[src_r].chars().collect();
        for (c_idx, dst) in row_cells.iter_mut().enumerate().take(EDITOR_COLS) {
            let shifted = c_idx as isize - shift_x;
            let src_c = shifted.rem_euclid(EDITOR_COLS as isize) as usize;
            let src_c = if flip_h {
                EDITOR_COLS - 1 - src_c
            } else {
                src_c
            };
            let token = src_chars[src_c];
            let cell = match token {
                '.' => '0',
                'E' => 'E',
                'Z' => {
                    if !is_teaching_block
                        && ((rng.next_u32() % 100) as f32 / 100.0) < tough_chance * 1.5
                    {
                        if (r_idx + c_idx).is_multiple_of(2) {
                            '5'
                        } else {
                            '6'
                        }
                    } else {
                        char::from_digit(((base_color + r_idx as u32) % 4) + 1, 10).unwrap_or('3')
                    }
                }
                'Y' => {
                    if ((rng.next_u32() % 100) as f32 / 100.0) < tough_chance {
                        '5'
                    } else {
                        char::from_digit(((sec_color + (c_idx / 4) as u32) % 4) + 1, 10)
                            .unwrap_or('2')
                    }
                }
                'X' => {
                    if ((rng.next_u32() % 100) as f32 / 100.0) < tough_chance * 0.85 {
                        '6'
                    } else {
                        char::from_digit(((base_color + (r_idx / 2) as u32) % 4) + 1, 10)
                            .unwrap_or('1')
                    }
                }
                _ => '0',
            };
            *dst = cell;
        }
    }

    // Apply structural pass on top of the motif.
    let pass = choose_structure_pass(world, sector_in_world, rng);
    apply_structure_pass(
        &mut grid,
        &PassContext {
            pass,
            world,
            sector_in_world,
            is_teaching_block,
            want_asymmetric,
        },
        rng,
    );

    // Tune explosive ('E') count according to block position so early blocks chain-clear
    // briskly and late blocks demand precision.
    let target_explosives = if is_teaching_block || is_breather {
        5 + rng.usize_below(3)
    } else if block_idx <= 3 {
        4 + rng.usize_below(3)
    } else if block_idx <= 6 {
        3 + rng.usize_below(2)
    } else {
        2 + rng.usize_below(2)
    };
    tune_explosive_nodes(&mut grid, target_explosives, rng);

    // Balance destructible density toward `target_brick_count(world, sector_in_world)`.
    let target_bricks = target_brick_count(world, sector_in_world);
    balance_density(&mut grid, target_bricks, base_color, want_asymmetric, rng);

    // Ensure at least 4 destructible bricks on the bottom-most row so ball launches engage bricks immediately.
    let last_r = grid.len() - 1;
    let bottom_destructibles = grid[last_r]
        .iter()
        .filter(|&&ch| is_destructible_cell(ch))
        .count();
    if bottom_destructibles < 4 {
        let start_col = 1 + rng.usize_below(4);
        for step in 0..4 {
            let col = (start_col + step * 3) % EDITOR_COLS;
            if grid[last_r][col] == '0' {
                grid[last_r][col] = char::from_digit((step as u32 % 4) + 1, 10).unwrap_or('1');
            }
        }
    }

    // If this sector is designated asymmetric, guarantee at least 8 mirror-mismatch cells.
    if want_asymmetric {
        enforce_asymmetry(&mut grid, base_color, rng);
    }

    grid.into_iter()
        .map(|r| r.into_iter().collect::<String>())
        .collect()
}

fn apply_structure_pass(grid: &mut [Vec<char>], ctx: &PassContext, rng: &mut Rng) {
    let pass = ctx.pass;
    let world = ctx.world;
    let sector_in_world = ctx.sector_in_world;
    let is_teaching_block = ctx.is_teaching_block;
    let want_asymmetric = ctx.want_asymmetric;
    let rows = grid.len();
    if rows < MIN_REGULAR_ROWS {
        return;
    }

    match pass {
        StructurePass::OpenShowcase => {
            // Keep lanes open; add a small diagonal accent of standard/explosive cells.
            let r = rows / 2;
            let c = (sector_in_world + rng.usize_below(7)) % (EDITOR_COLS - 4) + 2;
            grid[r][c] = 'E';
        }
        StructurePass::SteelCorridors => {
            // Place non-sealing '#' corridor baffles that create angled bank-shot channels.
            let r1 = (rows / 2).clamp(1, rows - 2);
            let left_col = 1 + rng.usize_below(3);
            let right_col = if want_asymmetric {
                (EDITOR_COLS - 2 - rng.usize_below(4)).max(left_col + 4)
            } else {
                EDITOR_COLS - 1 - left_col
            };
            grid[r1][left_col] = '#';
            grid[r1][right_col] = '#';
            if rows >= 6 && !is_teaching_block {
                let r2 = (r1 + 2).min(rows - 2);
                let mid_col = if want_asymmetric {
                    5 + rng.usize_below(5)
                } else {
                    7
                };
                grid[r2][mid_col] = '#';
            }
        }
        StructurePass::ExplosiveCage => {
            // Partial cage around an 'E' core at an offset position (never sealed).
            let cr = (rows / 2).clamp(1, rows - 2);
            let cc = if want_asymmetric {
                3 + rng.usize_below(EDITOR_COLS - 7)
            } else {
                7
            };
            grid[cr][cc] = 'E';
            if cc >= 2 && cc + 2 < EDITOR_COLS {
                grid[cr][cc - 1] = '5';
                grid[cr][cc + 1] = '5';
                if cr + 1 < rows && !is_teaching_block {
                    grid[cr + 1][cc - 1] = '#';
                    grid[cr + 1][cc + 1] = if want_asymmetric { '6' } else { '#' };
                    grid[cr + 1][cc] = '2';
                }
            }
        }
        StructurePass::PortalShortcut => {
            // Clear any pre-existing 'T' and place exactly 2 paired portals ('T').
            for r in grid.iter_mut() {
                for ch in r.iter_mut() {
                    if *ch == 'T' {
                        *ch = '2';
                    }
                }
            }
            let top_r = rng.usize_below((rows / 2).max(1));
            let bot_r = (rows - 2).max(top_r + 1).min(rows - 1);
            let (c_top, c_bot) = if want_asymmetric {
                let a = rng.usize_below(4);
                let b = EDITOR_COLS - 1 - rng.usize_below(4);
                (a, b)
            } else {
                let a = rng.usize_below(3);
                (a, EDITOR_COLS - 1 - a)
            };
            grid[top_r][c_top] = 'T';
            if want_asymmetric {
                grid[bot_r][c_bot] = 'T';
            } else {
                grid[top_r][c_bot] = 'T';
            }
        }
        StructurePass::GhostSilhouette => {
            // Place a readable silhouette/ribbon of 'G' ghost bricks (4 to 8 cells).
            let gr = (1 + rng.usize_below(rows.saturating_sub(2).max(1))).min(rows - 2);
            let start_c = if want_asymmetric {
                1 + rng.usize_below(5)
            } else {
                3
            };
            let count = if is_teaching_block { 5 } else { 7 };
            for i in 0..count {
                let c = (start_c + i) % EDITOR_COLS;
                let r = if want_asymmetric && i % 2 == 1 {
                    (gr + 1).min(rows - 2)
                } else {
                    gr
                };
                if grid[r][c] != 'T' && grid[r][c] != '#' {
                    grid[r][c] = 'G';
                }
            }
        }
        StructurePass::ConveyorLane => {
            // Dedicate one interior row mostly to '0' with 3 to 5 moving 'M' bricks so they sweep freely.
            let mr = (rows / 2).clamp(1, rows - 2);
            for cell in grid[mr].iter_mut() {
                if *cell != 'T' {
                    *cell = '0';
                }
            }
            let m_count = if is_teaching_block { 3 } else { 4 };
            let offset = rng.usize_below(3);
            for k in 0..m_count {
                let c = (offset + 1 + k * 3) % (EDITOR_COLS - 1);
                grid[mr][c] = 'M';
            }
        }
        StructurePass::DominoFuse => {
            // Diagonal fuse of 'E' cells separated by 1-2 standard bricks.
            let start_c = 2 + rng.usize_below(EDITOR_COLS - 6);
            let dir: isize = if rng.usize_below(2) == 0 { 2 } else { -2 };
            let mut c = start_c as isize;
            for (step, row) in grid.iter_mut().enumerate().take(rows.min(5)) {
                let col = c.rem_euclid(EDITOR_COLS as isize) as usize;
                if row[col] != 'T' && row[col] != '#' {
                    row[col] = if step % 2 == 0 { 'E' } else { '2' };
                }
                c += dir;
            }
        }
        StructurePass::BumperPinballBay => {
            // Place 2 or 3 'B' bumpers in the upper/mid rows (never on the bottom row) so
            // they create pinball rebounds inside the brick field without draining balls.
            let br = (rows / 3).clamp(1, rows.saturating_sub(3).max(1));
            let c1 = 3 + rng.usize_below(3);
            let c2 = if want_asymmetric {
                (c1 + 4 + rng.usize_below(4)).min(EDITOR_COLS - 2)
            } else {
                EDITOR_COLS - 1 - c1
            };
            if grid[br][c1] != 'T' {
                grid[br][c1] = 'B';
            }
            if grid[br][c2] != 'T' {
                grid[br][c2] = 'B';
            }
            if !is_teaching_block && rows >= 6 && world >= 5 {
                let br2 = (br + 1).min(rows - 3);
                let cm = (c1 + c2) / 2;
                if grid[br2][cm] != 'T' {
                    grid[br2][cm] = 'E';
                }
            }
        }
        StructurePass::RegenShieldWall => {
            // Place 2 to 4 'R' regenerating bricks as a shield in front of an 'E' core.
            let rr = (rows / 2).clamp(1, rows - 2);
            let center_c = if want_asymmetric {
                4 + rng.usize_below(7)
            } else {
                7
            };
            if grid[rr.saturating_sub(1)][center_c] != 'T' {
                grid[rr.saturating_sub(1)][center_c] = 'E';
            }
            for dc in [-1isize, 0, 1] {
                let c = (center_c as isize + dc).clamp(1, (EDITOR_COLS - 2) as isize) as usize;
                if grid[rr][c] != 'T' && grid[rr][c] != '#' {
                    grid[rr][c] = 'R';
                }
            }
        }
    }

    // Secondary world seasoning outside teaching blocks so worlds 4..=8 blend mechanics.
    if !is_teaching_block {
        if world >= 4 && pass != StructurePass::PortalShortcut && sector_in_world.is_multiple_of(4)
        {
            let has_portals = grid.iter().flatten().any(|&c| c == 'T');
            if !has_portals {
                let r_top = 0;
                let r_bot = (rows / 2).min(rows - 2);
                let c_left = rng.usize_below(2);
                let c_right = EDITOR_COLS - 1 - rng.usize_below(2);
                grid[r_top][c_left] = 'T';
                grid[r_bot][c_right] = 'T';
            }
        }
        if world >= 6 && pass != StructurePass::RegenShieldWall && sector_in_world.is_multiple_of(3)
        {
            let r = (rows / 2).clamp(1, rows - 2);
            let c = 2 + rng.usize_below(EDITOR_COLS - 4);
            if matches!(grid[r][c], '1'..='6') {
                grid[r][c] = 'R';
            }
        }
    }
}

fn tune_explosive_nodes(grid: &mut [Vec<char>], target_e: usize, rng: &mut Rng) {
    let rows = grid.len();
    let mut e_positions: Vec<(usize, usize)> = Vec::new();
    for (r, row) in grid.iter().enumerate() {
        for (c, &ch) in row.iter().enumerate() {
            if ch == 'E' {
                e_positions.push((r, c));
            }
        }
    }

    while e_positions.len() > target_e {
        let remove_idx = rng.usize_below(e_positions.len());
        let (r, c) = e_positions.swap_remove(remove_idx);
        grid[r][c] = char::from_digit(((r + c) % 4) as u32 + 1, 10).unwrap_or('2');
    }

    let mut attempts = 0usize;
    while e_positions.len() < target_e && attempts < 40 {
        attempts += 1;
        let r = rng.usize_below(rows);
        let c = 1 + rng.usize_below(EDITOR_COLS - 2);
        if matches!(grid[r][c], '1'..='6') {
            grid[r][c] = 'E';
            e_positions.push((r, c));
        }
    }
}

fn balance_density(
    grid: &mut [Vec<char>],
    target_bricks: usize,
    base_color: u32,
    want_asymmetric: bool,
    rng: &mut Rng,
) {
    let rows = grid.len();
    let count_destructible = |g: &[Vec<char>]| -> usize {
        g.iter()
            .flat_map(|r| r.iter())
            .filter(|&&ch| is_destructible_cell(ch))
            .count()
    };

    // Add standard bricks if below target.
    let mut guard = 0usize;
    while count_destructible(grid) < target_bricks && guard < 200 {
        guard += 1;
        let r = rng.usize_below(rows);
        let c = rng.usize_below(EDITOR_COLS);
        if grid[r][c] == '0' {
            let d = char::from_digit(((base_color + (r + c) as u32) % 4) + 1, 10).unwrap_or('1');
            grid[r][c] = d;
            if !want_asymmetric && count_destructible(grid) < target_bricks {
                let mc = EDITOR_COLS - 1 - c;
                if grid[r][mc] == '0' {
                    grid[r][mc] = d;
                }
            }
        }
    }

    // Remove standard bricks ('1'..='6') if above target.
    guard = 0;
    while count_destructible(grid) > target_bricks + 2 && guard < 250 {
        guard += 1;
        let r = rng.usize_below(rows);
        let c = rng.usize_below(EDITOR_COLS);
        if matches!(grid[r][c], '1'..='6') {
            grid[r][c] = '0';
            if !want_asymmetric && count_destructible(grid) > target_bricks {
                let mc = EDITOR_COLS - 1 - c;
                if matches!(grid[r][mc], '1'..='6') {
                    grid[r][mc] = '0';
                }
            }
        }
    }
}

fn enforce_asymmetry(grid: &mut [Vec<char>], base_color: u32, rng: &mut Rng) {
    let rows = grid.len();
    for step in 0..8 {
        let probe = LevelDef {
            id: 1,
            world: 1,
            name: String::new(),
            boss: false,
            rows: grid.iter().map(|r| r.iter().collect()).collect(),
        };
        if is_asymmetric(&probe) {
            return;
        }
        let r = (step + rng.usize_below(rows)) % rows;
        let c = (step + rng.usize_below(EDITOR_COLS / 2)) % (EDITOR_COLS / 2);
        let mc = EDITOR_COLS - 1 - c;
        let d = char::from_digit(((base_color + step as u32) % 4) + 1, 10).unwrap_or('1');
        if grid[r][c] == grid[r][mc] {
            if grid[r][c] == '0' {
                grid[r][c] = d;
            } else if matches!(grid[r][c], '1'..='6') {
                grid[r][mc] = '0';
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boss_stem_fits_the_final_suffix_and_never_depends_on_the_suffix() {
        assert_eq!(
            BOSS_STEM_MAX_LEN + FINAL_BOSS_SUFFIX.len(),
            MAX_LEVEL_NAME_LEN
        );
        for world in 1..=8 {
            for sector in [50, 100] {
                let regular =
                    generate_unique_name(world, sector, true, false, &mut BTreeSet::new())
                        .expect("boss name");
                let last = generate_unique_name(world, sector, true, true, &mut BTreeSet::new())
                    .expect("final boss name");
                let regular_stem = regular.strip_suffix(BOSS_SUFFIX).expect("boss suffix");
                let last_stem = last
                    .strip_suffix(FINAL_BOSS_SUFFIX)
                    .expect("final boss suffix");
                assert_eq!(regular_stem, last_stem, "world {world} sector {sector}");
                assert!(last_stem.len() <= BOSS_STEM_MAX_LEN);
                assert!(last.len() <= MAX_LEVEL_NAME_LEN);
            }
        }
    }

    #[test]
    fn generated_sectors_vary_row_counts_and_exceed_asymmetry_threshold() {
        let mut used = BTreeSet::new();
        let mut row_counts = BTreeSet::new();
        let mut asym = 0usize;
        for s in 16..=100 {
            let lvl = generate_sector(s, 1, s, s == 100, 12345, &mut used).expect("sector");
            row_counts.insert(lvl.rows.len());
            if is_asymmetric(&lvl) {
                asym += 1;
            }
        }
        assert!(row_counts.len() >= 4, "row counts: {row_counts:?}");
        assert!(asym >= 30, "expected >= 30 asymmetric in W1, got {asym}");
    }
}
