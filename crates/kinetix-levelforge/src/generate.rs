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

//! Deterministic campaign level generator.
//!
//! A regular level is composed from whole features, never from random cells:
//! - a hand-authored motif (`motifs.rs`) picked by the block theme of its world,
//! - an orientation: mirrored from a half motif for the symmetric sectors (`sector % 5` in
//!   `{0, 2}`), flipped, trimmed by whole edge columns and offset for the asymmetric ones (never cropped by
//!   rows, so the motif stays intact),
//! - a structural pass that turns a whole feature into a mechanic (steel flanks, explosive
//!   cage, domino fuse, ghost band, conveyor lane, portal pair, bumpers, regenerating shield),
//! - a colour and toughness style applied per row band or per feature, never per cell.
//!
//! Sector 10 of each block is a set piece from [`SET_PIECES`]. The 16 boss arenas are
//! hand-authored in `bosses.rs`.

use crate::bosses::boss_arena;
use crate::motifs::{MotifFamily, MotifTemplate, MOTIFS};
use crate::simulate::SECTORS_PER_BLOCK;
use crate::validate::{
    colour_regions_per_cell, dust_and_components, inner_gap_cols, inner_gap_rows,
    is_destructible_cell, mirror_mismatch_cells, BOSS_STEM_MAX_LEN, BOSS_SUFFIX, DENSITY_MAX,
    DENSITY_MIN, FINAL_BOSS_SUFFIX, MAX_COLOUR_REGIONS_PER_CELL, MAX_COMPONENTS_8, MAX_DUST,
    MAX_INNER_GAP_COLS, MAX_INNER_GAP_ROWS, MAX_LEVEL_NAME_LEN, MECHANIC_DEBUTS,
    MIN_ASYMMETRIC_CELLS,
};
use kinetix_core::config::LevelDef;
use kinetix_core::level_code::{EDITOR_COLS, EDITOR_ROWS};
use std::collections::BTreeSet;
use std::fmt;

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
        "OMNI",
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

/// Distance between the name-pair indices of two consecutive sectors (coprime with the number
/// of adjective x noun pairs, so a world never repeats a pair).
const NAME_PAIR_STRIDE: usize = 37;
/// Shift of the first name-pair index per world, so worlds do not open on the same noun.
const NAME_WORLD_OFFSET: usize = 5;

/// Minimum row count of a generated regular level.
const MIN_REGULAR_ROWS: usize = 4;
/// Maximum row count of a generated regular level.
const MAX_REGULAR_ROWS: usize = EDITOR_ROWS;
/// Minimum destructible bricks of a generated regular level (margin inside the validate band).
const SAFE_DENSITY_MIN: usize = DENSITY_MIN + 2;
/// Maximum destructible bricks of a generated regular level (margin inside the validate band).
const SAFE_DENSITY_MAX: usize = DENSITY_MAX - 4;
/// Mid-boss sector index within each 100-sector world.
const MID_BOSS_SECTOR: usize = 50;
/// Final boss sector index within each 100-sector world.
const WORLD_BOSS_SECTOR: usize = 100;
/// Blocks per world.
const BLOCKS_PER_WORLD: usize = 10;
/// Position inside a block (0-based) of the set piece.
const SET_PIECE_POS: usize = SECTORS_PER_BLOCK - 1;
/// Positions inside a block (0-based) of the two breathers.
const BREATHER_POSITIONS: [usize; 2] = [3, 7];
/// Sector residues modulo [`SYMMETRY_PERIOD`] that are mirror-symmetric (40% of the sectors).
const SYMMETRIC_RESIDUES: [usize; 2] = [0, 2];
/// Period of the symmetric / asymmetric sector pattern.
const SYMMETRY_PERIOD: usize = 5;
/// An asymmetric sector must differ from its mirror by at least this many cells (the validate
/// gate needs `MIN_ASYMMETRIC_CELLS`; the margin keeps it from sitting on the line).
const ASYMMETRY_MIN_MISMATCH: usize = MIN_ASYMMETRIC_CELLS + 2;
/// Largest horizontal offset of an asymmetric motif (columns), clamped to its bounding box.
const MAX_SHIFT: isize = 3;
/// One in this many non-teaching sectors flips its motif upside down.
const FLIP_VERTICAL_ODDS: usize = 3;
/// Largest number of empty rows above a motif.
const MAX_TOP_PAD: usize = 2;
/// Percent chance that a symmetric motif is cropped (rows, columns) before its feature is added.
/// Asymmetric motifs are never cropped by rows; they are only trimmed by whole edge columns.
const CROP_PERCENT: usize = 50;
/// Most rows cropped off the top or the bottom of a symmetric motif.
const MAX_CROP_ROWS: usize = 2;
/// Most whole edge columns trimmed off one side of an asymmetric motif.
const MAX_TRIM_COLS: usize = 2;
/// An asymmetric motif may lose at most this percent of its cells to the column trim, so the
/// shape stays recognisable.
const TRIM_MAX_CELL_PERCENT: usize = 15;
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

// Density: the destructible brick target of a sector. Legacy levels measured 52..=77.
/// Brick target of the first world before any rise.
const DENSITY_BASE: f32 = 34.0;
/// Brick target added per world.
const DENSITY_PER_WORLD: f32 = 1.4;
/// Brick target added per block of a world.
const DENSITY_PER_BLOCK: f32 = 2.8;
/// Brick target change on the first sector of a block (a gentle opener).
const DENSITY_OPENER_DELTA: f32 = -4.0;
/// Brick target change on the breather sectors of a block.
const DENSITY_BREATHER_DELTA: f32 = -5.0;
/// Brick target change on the set piece of a block.
const DENSITY_SET_PIECE_DELTA: f32 = 3.0;
/// A level may hold this many bricks fewer than its target.
const DENSITY_WINDOW_BELOW: usize = 8;
/// A level may hold this many bricks more than its target.
const DENSITY_WINDOW_ABOVE: usize = 18;
/// Fewest motifs of the block theme that must fit the density window; fewer and the whole
/// library is searched instead.
const MIN_THEMED_POOL: usize = 3;
/// A pass may add or remove a few bricks (steel replaces some, a lane adds some), so motifs this
/// many cells outside the density window are still considered.
const PASS_SLACK: usize = 4;

// Hardness dial (0 = softest, `MAX_HARDNESS` = hardest). It only selects whole features.
/// Highest hardness step.
pub const MAX_HARDNESS: usize = 5;
/// Tough (two-hit) body bands per hardness step.
const TOUGH_BANDS_BY_HARDNESS: [usize; MAX_HARDNESS + 1] = [0, 1, 1, 2, 3, 3];
/// From this hardness the focal bricks (token `Z`) are tough.
const TOUGH_FOCAL_FROM: usize = 2;
/// From this hardness the trim bricks (token `Y`) are tough.
const TOUGH_TRIM_FROM: usize = 4;
/// From this hardness the explosives of the motif are defused into plain bricks.
const DEFUSE_FROM: usize = 4;
/// Hardness ceiling of teaching sectors.
const TEACHING_MAX_HARDNESS: usize = 1;
/// Hardness ceiling of breather sectors.
const BREATHER_MAX_HARDNESS: usize = 2;

// Colour styling.
const NORMAL_COLOURS: [char; 4] = ['1', '2', '3', '4'];
const TOUGH_COLOURS: [char; 2] = ['5', '6'];
/// Candidate heights (rows) of a colour band.
const BAND_ROW_CHOICES: [usize; 3] = [2, 2, 3];
/// Offset of the trim colour from the body base colour.
const TRIM_COLOUR_OFFSET: usize = 2;
/// Offset of the focal colour from the body base colour.
const FOCAL_COLOUR_OFFSET: usize = 3;

// Structure pass parameters.
/// Fewest cells a steel feature must replace.
const STEEL_MIN_CELLS: usize = 4;
/// Most cells of a steel girder, so a gap always remains at an edge of the row.
const STEEL_GIRDER_MAX_CELLS: usize = COLS - 4;
/// Longest run of a domino fuse.
const FUSE_MAX_LEN: usize = 5;
/// Shortest run of a domino fuse.
const FUSE_MIN_LEN: usize = 4;
/// Fewest cells of a ghost band.
const GHOST_MIN_CELLS: usize = 4;
/// Most cells of a ghost band in a teaching sector.
const GHOST_TEACHING_MAX_CELLS: usize = 8;
/// Most cells of a ghost band elsewhere.
const GHOST_MAX_CELLS: usize = 20;
/// Rows of a ghost band.
const GHOST_BAND_ROWS: usize = 2;
/// Moving bricks of a teaching conveyor lane.
const LANE_TEACHING_BRICKS: usize = 3;
/// Moving bricks of a conveyor lane elsewhere.
const LANE_BRICKS: usize = 4;
/// Fewest columns between two moving bricks of a lane, so they have room to travel.
const LANE_MIN_SPACING: usize = 2;
/// Widest border column probed for a portal.
const PORTAL_MAX_INSET: usize = 3;
/// Fewest and most focal bricks turned into bumpers.
const BUMPER_FOCAL_RANGE: (usize, usize) = (2, 4);
/// Column pairs probed for a bumper pair when the motif has no focal bricks.
const BUMPER_COL_PAIRS: [(usize, usize); 3] = [(4, 10), (3, 11), (5, 9)];
/// Asymmetric bumper pair spacing in columns.
const BUMPER_ASYMMETRIC_GAP: usize = 5;
/// Fewest contiguous body cells of a regenerating shield row.
const SHIELD_MIN_RUN: usize = 3;
/// Most cells of a regenerating shield.
const SHIELD_MAX_RUN: usize = 7;
/// Percent chance that a later block adds a second pass.
const SECONDARY_PASS_PERCENT: usize = 33;
/// First block whose sectors may carry a second pass.
const SECONDARY_PASS_FROM_BLOCK: usize = 3;

const COLS: usize = EDITOR_COLS;
const MID_COL: usize = COLS / 2;
/// Fraction scale: percent.
const PERCENT: usize = 100;

// Motif tokens.
const T_EMPTY: char = '.';
const T_BODY: char = 'X';
const T_TRIM: char = 'Y';
const T_FOCAL: char = 'Z';
const T_EXPLOSIVE: char = 'E';
/// Cage brick: a tough brick protecting an explosive.
const T_CAGE: char = 'K';
const CELL_EMPTY: char = '0';
const CELL_STEEL: char = '#';
const CELL_GHOST: char = 'G';
const CELL_MOVING: char = 'M';
const CELL_PORTAL: char = 'T';
const CELL_BUMPER: char = 'B';
const CELL_REGEN: char = 'R';

type Grid = Vec<Vec<char>>;

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

    fn chance(&mut self, percent: usize) -> bool {
        self.usize_below(PERCENT) < percent
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.usize_below(items.len())]
    }
}

/// Structural pass applied on top of a placed motif.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StructurePass {
    /// The motif as drawn.
    Open,
    /// Indestructible `#` flanks or girder.
    Steel,
    /// Tough bricks around the explosive cores.
    Cage,
    /// A diagonal run of `E` bricks that chain-detonates.
    Fuse,
    /// A band of `G` ghost bricks.
    Ghost,
    /// A lane of `M` moving bricks.
    Conveyor,
    /// Exactly two `T` portals.
    Portals,
    /// `B` bumpers.
    Bumper,
    /// An `R` regenerating shield row.
    Regen,
}

impl StructurePass {
    /// The mechanic cell this pass introduces, if any.
    fn mechanic(self) -> Option<char> {
        match self {
            StructurePass::Ghost => Some(CELL_GHOST),
            StructurePass::Conveyor => Some(CELL_MOVING),
            StructurePass::Portals => Some(CELL_PORTAL),
            StructurePass::Bumper => Some(CELL_BUMPER),
            StructurePass::Regen => Some(CELL_REGEN),
            StructurePass::Open
            | StructurePass::Steel
            | StructurePass::Cage
            | StructurePass::Fuse => None,
        }
    }

    /// First world in which the pass may be used.
    fn debut_world(self) -> usize {
        self.mechanic()
            .and_then(|m| {
                MECHANIC_DEBUTS
                    .iter()
                    .find(|(cell, _)| *cell == m)
                    .map(|(_, w)| *w)
            })
            .unwrap_or(1)
    }
}

use StructurePass::{Bumper, Cage, Conveyor, Fuse, Ghost, Open, Portals, Regen, Steel};

/// Pass of every teaching sector (block 1) of each world: the debut mechanic alone, or a mix of
/// the known features in the worlds that introduce nothing new.
const TEACHING_PASSES: [&[StructurePass]; 8] = [
    &[Open, Fuse, Cage],
    &[Ghost],
    &[Conveyor],
    &[Portals],
    &[Bumper],
    &[Regen],
    &[Cage, Steel, Fuse],
    &[Fuse, Steel, Cage],
];

/// Pass mix of the later blocks of each world (a pass listed twice is drawn twice as often).
const WORLD_PASS_MIX: [&[StructurePass]; 8] = [
    &[Open, Steel, Cage, Fuse, Steel, Cage],
    &[Ghost, Ghost, Steel, Cage, Fuse, Open],
    &[Conveyor, Conveyor, Steel, Steel, Ghost, Cage, Fuse],
    &[Portals, Portals, Cage, Ghost, Conveyor, Steel],
    &[Bumper, Bumper, Fuse, Fuse, Portals, Cage, Conveyor],
    &[Regen, Regen, Ghost, Bumper, Portals, Cage],
    &[Ghost, Ghost, Steel, Steel, Portals, Regen, Cage, Conveyor],
    &[Bumper, Regen, Fuse, Conveyor, Portals, Ghost, Bumper, Regen],
];

/// Signature pass of each world, used by the set pieces.
const SIGNATURE_PASSES: [StructurePass; 8] =
    [Cage, Ghost, Conveyor, Portals, Bumper, Regen, Ghost, Bumper];

/// Pass added to a set piece on top of the signature pass of its world.
const SET_PIECE_SECONDARY: [Option<StructurePass>; 8] = [
    Some(Steel),
    Some(Fuse),
    Some(Steel),
    Some(Cage),
    Some(Fuse),
    Some(Cage),
    Some(Steel),
    Some(Regen),
];

/// Motifs a set piece cycles through: world `w` uses entry `(w - 1) % SET_PIECE_VARIANTS`, so two
/// worlds only share a motif four worlds apart.
const SET_PIECE_VARIANTS: usize = 4;

/// A showcase layout: sector 10 of a block is one of these icons with the signature pass of its
/// world.
#[derive(Debug, Clone, Copy)]
pub struct SetPiece {
    /// Block theme the set piece closes.
    pub theme: &'static str,
    /// Names of the motifs (see `motifs.rs`), one per group of worlds.
    pub motifs: [&'static str; SET_PIECE_VARIANTS],
}

/// One set piece per block that does not end on a boss (blocks 1..=4 and 6..=9 of a world).
pub const SET_PIECES: [SetPiece; 8] = [
    SetPiece {
        theme: "LOTUS BLOOM",
        motifs: [
            "LOTUS_MANDALA",
            "SUNBURST_HALO",
            "ROSE_COMPASS",
            "GLYPH_INFINITY",
        ],
    },
    SetPiece {
        theme: "SKULL GATE",
        motifs: [
            "PIXEL_SKULL",
            "SPACE_INVADER",
            "PIXEL_SWORD",
            "TWIN_SPEARHEAD",
        ],
    },
    SetPiece {
        theme: "LAUNCH PAD",
        motifs: [
            "PIXEL_ROCKET",
            "UPWARD_VECTORS",
            "GLYPH_OMEGA",
            "ZIGGURAT_PYRAMID",
        ],
    },
    SetPiece {
        theme: "TRIDENT WATCH",
        motifs: [
            "GLYPH_TRIDENT",
            "HOOK_ARROW",
            "SHEAR_FAULT",
            "CHECKERBOARD_7R",
        ],
    },
    SetPiece {
        theme: "DEEP ANCHORAGE",
        motifs: [
            "PIXEL_ANCHOR",
            "OFFSET_BRICKWORK",
            "GLYPH_K_X",
            "TWIN_PEAKS",
        ],
    },
    SetPiece {
        theme: "WATCHFUL EYE",
        motifs: [
            "ALL_SEEING_EYE",
            "TSUNAMI_CREST",
            "RINGED_PLANET",
            "GLYPH_SIGMA_DELTA",
        ],
    },
    SetPiece {
        theme: "STAR WHEEL",
        motifs: [
            "STAR_MANDALA",
            "VORTEX_COIL",
            "FALLING_RIBBONS",
            "COMET_STREAK",
        ],
    },
    SetPiece {
        theme: "FUNNEL HALL",
        motifs: ["FUNNEL_VEES", "SWITCHBACK_RAMP", "CROSS_SASH", "PIXEL_KEY"],
    },
];

/// Index into [`SET_PIECES`] of the set piece closing `block`, or `None` for the blocks that end
/// on a boss (the 5th and the 10th).
fn set_piece_index(block: usize) -> Option<usize> {
    match block {
        0..=3 => Some(block),
        5..=8 => Some(block - 1),
        _ => None,
    }
}

use MotifFamily::{Arrow, Checker, Diagonal, Glyph, Icon, Mandala, SpiralWave, Staircase};

/// Block theme of every block of every world: the motif family used for most sectors and the
/// family used for the third sector of each triple.
const BLOCK_FAMILIES: [[(MotifFamily, MotifFamily); BLOCKS_PER_WORLD]; 8] = [
    // World 1: icons, glyphs and terraces.
    [
        (Icon, Glyph),
        (Icon, Staircase),
        (Glyph, Arrow),
        (Staircase, Diagonal),
        (Icon, Glyph),
        (Arrow, Checker),
        (Diagonal, Icon),
        (Glyph, Staircase),
        (Icon, Arrow),
        (Staircase, Icon),
    ],
    // World 2: phase mirrors, spirals and mandalas.
    [
        (Mandala, SpiralWave),
        (SpiralWave, Diagonal),
        (Diagonal, Mandala),
        (Mandala, Glyph),
        (SpiralWave, Arrow),
        (Diagonal, Checker),
        (Mandala, Icon),
        (SpiralWave, Staircase),
        (Diagonal, SpiralWave),
        (Mandala, Diagonal),
    ],
    // World 3: forged blocks and masonry.
    [
        (Checker, Staircase),
        (Staircase, Glyph),
        (Checker, Icon),
        (Glyph, Checker),
        (Staircase, Arrow),
        (Checker, Mandala),
        (Icon, Staircase),
        (Glyph, Diagonal),
        (Checker, Staircase),
        (Staircase, Checker),
    ],
    // World 4: cold lattices and arrows.
    [
        (Arrow, Mandala),
        (Mandala, Checker),
        (Arrow, Glyph),
        (Checker, Arrow),
        (Mandala, SpiralWave),
        (Arrow, Icon),
        (Glyph, Mandala),
        (Arrow, Staircase),
        (Mandala, Arrow),
        (Checker, Diagonal),
    ],
    // World 5: solar icons and bursts.
    [
        (Icon, Mandala),
        (Mandala, Arrow),
        (Icon, SpiralWave),
        (Arrow, Icon),
        (Mandala, Glyph),
        (Icon, Diagonal),
        (SpiralWave, Mandala),
        (Icon, Staircase),
        (Arrow, Mandala),
        (Mandala, Icon),
    ],
    // World 6: crystal facets and diagonals.
    [
        (Diagonal, Checker),
        (Checker, Mandala),
        (Diagonal, Arrow),
        (Mandala, Diagonal),
        (Checker, Glyph),
        (Diagonal, SpiralWave),
        (Arrow, Checker),
        (Diagonal, Mandala),
        (Checker, Icon),
        (Mandala, Diagonal),
    ],
    // World 7: voids, spirals and staircases.
    [
        (SpiralWave, Staircase),
        (Staircase, Diagonal),
        (SpiralWave, Glyph),
        (Diagonal, SpiralWave),
        (Staircase, Mandala),
        (SpiralWave, Arrow),
        (Glyph, Staircase),
        (Diagonal, Staircase),
        (SpiralWave, Icon),
        (Staircase, SpiralWave),
    ],
    // World 8: crowns, thrones and grand glyphs.
    [
        (Icon, Staircase),
        (Glyph, Icon),
        (Arrow, Glyph),
        (Icon, Mandala),
        (Staircase, Arrow),
        (Glyph, Diagonal),
        (Icon, Checker),
        (Mandala, Icon),
        (Arrow, Staircase),
        (Glyph, Mandala),
    ],
];

/// Why a candidate could not be produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectorError {
    /// No unique name is left: retrying with another seed cannot help.
    Name(String),
    /// A boss sector has no hand-authored arena: retrying with another seed cannot help.
    MissingBossArena(String),
    /// This seed produced a layout that misses a design rule: retry with another seed.
    Layout(String),
}

impl fmt::Display for SectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SectorError::Name(m) | SectorError::MissingBossArena(m) | SectorError::Layout(m) => {
                write!(f, "{m}")
            }
        }
    }
}

/// Parameters of one candidate.
#[derive(Debug, Clone, Copy)]
pub struct SectorRequest {
    pub id: usize,
    pub world: usize,
    /// Sector within its world, `1..=100`.
    pub sector_in_world: usize,
    /// The last level of the pack, named as the final boss.
    pub is_last: bool,
    pub seed: u64,
    /// Difficulty dial, `0..=MAX_HARDNESS` (see [`hardness_for_rate`]).
    pub hardness: usize,
}

/// Whether a sector of a world holds a boss arena.
pub fn is_boss_sector(sector_in_world: usize) -> bool {
    sector_in_world == MID_BOSS_SECTOR || sector_in_world == WORLD_BOSS_SECTOR
}

/// Generates a unique uppercase name of at most `MAX_LEVEL_NAME_LEN` chars for a sector.
/// Boss names use the hand-authored boss title stem when available and end with `BOSS_SUFFIX`,
/// or `FINAL_BOSS_SUFFIX` for the last level of the pack; their stem is at most
/// `BOSS_STEM_MAX_LEN` chars and never depends on the suffix. Regular names walk every
/// adjective x noun pair of the world from a per-sector offset, so they never need a number.
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
    let pairs = adjs.len() * NOUNS.len();
    let start = (sector_in_world * NAME_PAIR_STRIDE + w_idx * NAME_WORLD_OFFSET) % pairs;
    for step in 0..pairs {
        let pair = (start + step) % pairs;
        let adjective = adjs[pair % adjs.len()];
        let noun = NOUNS[pair / adjs.len()];
        if adjective == noun {
            continue;
        }
        let candidate = if boss {
            let stem = format!("{adjective} {noun}");
            if stem.len() <= BOSS_STEM_MAX_LEN {
                format!("{stem}{suffix}")
            } else {
                format!("{adjective}{suffix}")
            }
        } else {
            format!("{adjective} {noun}")
        };
        if candidate.len() <= MAX_LEVEL_NAME_LEN && used.insert(candidate.clone()) {
            return Ok(candidate);
        }
    }
    Err(format!(
        "no unique level name left for world {world}, sector {sector_in_world} among {pairs} pairs"
    ))
}

/// Role of a sector inside its block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SectorKind {
    /// Block 1 of a world: the debut mechanic, alone, in an easy layout.
    Teaching,
    /// Sectors 4 and 8 of a block: lighter.
    Breather,
    /// Everything else.
    Standard,
    /// Sector 10 of a block.
    SetPiece,
}

/// Everything about a sector that does not depend on the seed.
#[derive(Debug, Clone, Copy)]
struct SectorPlan {
    world: usize,
    sector: usize,
    block: usize,
    pos: usize,
    kind: SectorKind,
    symmetric: bool,
}

impl SectorPlan {
    fn new(world: usize, sector: usize) -> Self {
        let block = sector.saturating_sub(1) / SECTORS_PER_BLOCK;
        let pos = sector.saturating_sub(1) % SECTORS_PER_BLOCK;
        let kind = if pos == SET_PIECE_POS {
            SectorKind::SetPiece
        } else if block == 0 {
            SectorKind::Teaching
        } else if BREATHER_POSITIONS.contains(&pos) {
            SectorKind::Breather
        } else {
            SectorKind::Standard
        };
        Self {
            world,
            sector,
            block,
            pos,
            kind,
            symmetric: is_symmetric_sector(sector),
        }
    }

    fn world_idx(&self) -> usize {
        self.world.saturating_sub(1).min(WORLD_ADJECTIVES.len() - 1)
    }

    fn max_hardness(&self) -> usize {
        match self.kind {
            SectorKind::Teaching => TEACHING_MAX_HARDNESS,
            SectorKind::Breather => BREATHER_MAX_HARDNESS,
            SectorKind::Standard | SectorKind::SetPiece => MAX_HARDNESS,
        }
    }
}

/// Whether a sector is built mirror-symmetric (40% of the sectors); the others are asymmetric.
pub fn is_symmetric_sector(sector_in_world: usize) -> bool {
    SYMMETRIC_RESIDUES.contains(&(sector_in_world % SYMMETRY_PERIOD))
}

/// Target destructible brick count for `(world, sector_in_world)`: light in the first block of a
/// world, rising block by block, with a gentle opener and breathers in each block.
pub fn target_brick_count(world: usize, sector_in_world: usize) -> usize {
    let plan = SectorPlan::new(world, sector_in_world);
    let wave = if plan.pos == 0 {
        DENSITY_OPENER_DELTA
    } else if BREATHER_POSITIONS.contains(&plan.pos) {
        DENSITY_BREATHER_DELTA
    } else if plan.pos == SET_PIECE_POS {
        DENSITY_SET_PIECE_DELTA
    } else {
        0.0
    };
    let target = DENSITY_BASE
        + world.saturating_sub(1) as f32 * DENSITY_PER_WORLD
        + plan.block as f32 * DENSITY_PER_BLOCK
        + wave;
    target
        .round()
        .clamp(SAFE_DENSITY_MIN as f32, SAFE_DENSITY_MAX as f32) as usize
}

/// Allowed destructible-brick window of a sector. Set pieces are showcases of large icons, so
/// they may use the whole safe band.
fn density_window(world: usize, sector_in_world: usize) -> (usize, usize) {
    if SectorPlan::new(world, sector_in_world).kind == SectorKind::SetPiece {
        return (SAFE_DENSITY_MIN, SAFE_DENSITY_MAX);
    }
    let target = target_brick_count(world, sector_in_world);
    (
        target
            .saturating_sub(DENSITY_WINDOW_BELOW)
            .max(SAFE_DENSITY_MIN),
        (target + DENSITY_WINDOW_ABOVE).min(SAFE_DENSITY_MAX),
    )
}

/// Hardness step that aims at a good-bot clear rate: 0.75 and above is the softest step, each
/// 0.125 lower (one seed of eight) is one step harder.
pub fn hardness_for_rate(desired_rate: f32) -> usize {
    const SOFTEST_RATE: f32 = 0.75;
    const RATE_PER_STEP: f32 = 0.125;
    let steps = ((SOFTEST_RATE - desired_rate) / RATE_PER_STEP).round();
    steps.clamp(0.0, MAX_HARDNESS as f32) as usize
}

/// Offset of the wanted good-bot clear rate of a sector from the target of its world, in rate
/// units: teaching sectors and breathers are softer, set pieces and the end of a world harder.
/// The offsets of one world sum to about zero so the world mean stays on its target.
pub fn difficulty_shape(world: usize, sector_in_world: usize) -> f32 {
    const TEACHING_BONUS: f32 = 0.12;
    const BREATHER_BONUS: f32 = 0.08;
    const SET_PIECE_PENALTY: f32 = -0.06;
    const BLOCK_SLOPE: f32 = 0.016;
    let plan = SectorPlan::new(world, sector_in_world);
    let slope = (BLOCKS_PER_WORLD as f32 / 2.0 - plan.block as f32) * BLOCK_SLOPE;
    let role = match plan.kind {
        SectorKind::Teaching => TEACHING_BONUS,
        SectorKind::Breather => BREATHER_BONUS,
        SectorKind::SetPiece => SET_PIECE_PENALTY,
        SectorKind::Standard => 0.0,
    };
    slope + role
}

/// Theme of the set piece that `sector_in_world` of `world` is, `None` for any other sector.
pub fn set_piece_theme(world: usize, sector_in_world: usize) -> Option<&'static str> {
    let plan = SectorPlan::new(world, sector_in_world);
    if plan.kind != SectorKind::SetPiece {
        return None;
    }
    set_piece_index(plan.block)
        .and_then(|i| SET_PIECES.get(i))
        .map(|piece| piece.theme)
}

/// Builds the boss level of `(world, sector_in_world)` from its hand-authored arena.
fn boss_level(
    id: usize,
    world: usize,
    sector_in_world: usize,
    name: String,
) -> Result<LevelDef, String> {
    let arena = boss_arena(world, sector_in_world).ok_or_else(|| {
        format!("missing hand-authored boss arena for world {world} sector {sector_in_world}")
    })?;
    Ok(LevelDef {
        id,
        world,
        name,
        boss: true,
        rows: arena.rows.iter().map(|r| (*r).to_string()).collect(),
    })
}

/// Generates a single candidate level for `request`.
pub fn generate_sector(
    request: &SectorRequest,
    used_names: &mut BTreeSet<String>,
) -> Result<LevelDef, SectorError> {
    let boss = is_boss_sector(request.sector_in_world);
    let name = generate_unique_name(
        request.world,
        request.sector_in_world,
        boss,
        boss && request.is_last,
        used_names,
    )
    .map_err(SectorError::Name)?;

    if boss {
        return boss_level(request.id, request.world, request.sector_in_world, name)
            .map_err(SectorError::MissingBossArena);
    }

    let mut rng = Rng::new(request.seed ^ (request.id as u64).wrapping_mul(LEVEL_SEED_SALT));
    let plan = SectorPlan::new(request.world, request.sector_in_world);
    let hardness = request.hardness.min(plan.max_hardness());
    let rows = build_regular_rows(&plan, hardness, &mut rng).map_err(SectorError::Layout)?;
    Ok(LevelDef {
        id: request.id,
        world: request.world,
        name,
        boss: false,
        rows,
    })
}

// ---------------------------------------------------------------------------------------------
// Grid helpers
// ---------------------------------------------------------------------------------------------

fn motif_grid(motif: &MotifTemplate) -> Grid {
    motif.rows.iter().map(|r| r.chars().collect()).collect()
}

fn is_body_token(ch: char) -> bool {
    matches!(ch, T_BODY | T_TRIM | T_FOCAL)
}

/// Body or trim brick: the plain bricks a feature may turn into a mechanic.
fn is_plain_token(ch: char) -> bool {
    matches!(ch, T_BODY | T_TRIM)
}

fn count_tokens(grid: &Grid, pred: impl Fn(char) -> bool) -> usize {
    grid.iter().flatten().filter(|&&c| pred(c)).count()
}

/// Leftmost and rightmost non-empty columns.
fn bbox_cols(grid: &Grid) -> Option<(usize, usize)> {
    let mut cols = grid
        .iter()
        .flat_map(|row| row.iter().enumerate())
        .filter(|(_, &ch)| ch != T_EMPTY)
        .map(|(c, _)| c);
    let first = cols.next()?;
    let (min, max) = cols.fold((first, first), |(lo, hi), c| (lo.min(c), hi.max(c)));
    Some((min, max))
}

/// Keeps one half of the grid and mirrors it onto the other, so the result is exactly symmetric.
fn mirror_half(grid: &mut Grid, keep_left: bool) {
    for row in grid.iter_mut() {
        for c in 0..MID_COL {
            if keep_left {
                row[COLS - 1 - c] = row[c];
            } else {
                row[c] = row[COLS - 1 - c];
            }
        }
    }
}

/// Shifts the grid horizontally by up to `dx` columns, clamped to the bounding box so no cell
/// leaves the grid and none wraps around.
fn shift_clamped(grid: &mut Grid, dx: isize) {
    let Some((min, max)) = bbox_cols(grid) else {
        return;
    };
    let room_left = min as isize;
    let room_right = (COLS - 1 - max) as isize;
    let dx = dx.clamp(-room_left, room_right);
    if dx == 0 {
        return;
    }
    for row in grid.iter_mut() {
        let mut shifted = vec![T_EMPTY; COLS];
        for (c, &ch) in row.iter().enumerate() {
            if ch != T_EMPTY {
                shifted[(c as isize + dx) as usize] = ch;
            }
        }
        *row = shifted;
    }
}

/// Empties the first `left` and last `right` columns.
fn crop_columns(grid: &mut Grid, left: usize, right: usize) {
    for row in grid.iter_mut() {
        for cell in row.iter_mut().take(left) {
            *cell = T_EMPTY;
        }
        for cell in row.iter_mut().skip(COLS.saturating_sub(right)) {
            *cell = T_EMPTY;
        }
    }
}

/// Number of non-empty cells in the `count` first (or last) columns of the grid.
fn edge_cells(grid: &Grid, count: usize, from_left: bool) -> usize {
    grid.iter()
        .map(|row| {
            let edge: Vec<&char> = if from_left {
                row.iter().take(count).collect()
            } else {
                row.iter().skip(COLS.saturating_sub(count)).collect()
            };
            edge.into_iter().filter(|&&c| c != T_EMPTY).count()
        })
        .sum()
}

/// Gives a motif its asymmetric orientation with structured means only: an optional horizontal
/// flip, an optional trim of whole edge columns (never more than [`TRIM_MAX_CELL_PERCENT`] of its
/// cells, so no row is ever cut through), then an offset to one side inside its bounding box.
fn orient_asymmetric(grid: &mut Grid, rng: &mut Rng) {
    if rng.usize_below(2) == 1 {
        for row in grid.iter_mut() {
            row.reverse();
        }
    }
    let total = count_tokens(grid, |c| c != T_EMPTY);
    let from_left = rng.usize_below(2) == 0;
    let wanted = rng.usize_below(MAX_TRIM_COLS + 1);
    let amount = (0..=wanted)
        .rev()
        .find(|&k| edge_cells(grid, k, from_left) * 100 <= total * TRIM_MAX_CELL_PERCENT)
        .unwrap_or(0);
    if from_left {
        crop_columns(grid, amount, 0);
    } else {
        crop_columns(grid, 0, amount);
    }
    let magnitude = 1 + rng.usize_below(MAX_SHIFT as usize) as isize;
    let dx = if rng.usize_below(2) == 0 {
        magnitude
    } else {
        -magnitude
    };
    shift_clamped(grid, dx);
}

/// Sets a cell, and its mirror counterpart on symmetric sectors.
fn put(grid: &mut Grid, r: usize, c: usize, ch: char, symmetric: bool) {
    grid[r][c] = ch;
    if symmetric {
        grid[r][COLS - 1 - c] = ch;
    }
}

// ---------------------------------------------------------------------------------------------
// Structure passes
// ---------------------------------------------------------------------------------------------

/// Context shared by every structure pass.
#[derive(Debug, Clone, Copy)]
struct PassContext {
    symmetric: bool,
    teaching: bool,
}

/// Applies `pass` to a token grid; false when the motif offers no room for the feature.
fn apply_pass(pass: StructurePass, grid: &mut Grid, ctx: PassContext, rng: &mut Rng) -> bool {
    match pass {
        Open => true,
        Steel => pass_steel(grid, ctx, rng),
        Cage => pass_cage(grid, ctx),
        Fuse => pass_fuse(grid, ctx, rng),
        Ghost => pass_ghost(grid, ctx, rng),
        Conveyor => pass_conveyor(grid, ctx, rng),
        Portals => pass_portals(grid, ctx, rng),
        Bumper => pass_bumper(grid, ctx, rng),
        Regen => pass_regen(grid, ctx, rng),
    }
}

/// Steel flanks (the outermost column of the shape) or a steel girder (the widest trim row).
fn pass_steel(grid: &mut Grid, ctx: PassContext, rng: &mut Rng) -> bool {
    let rows = grid.len();
    if rng.usize_below(2) == 0 {
        let Some((min, max)) = bbox_cols(grid) else {
            return false;
        };
        let right_only = !ctx.symmetric && rng.usize_below(2) == 1;
        let left_only = !ctx.symmetric && !right_only;
        let mut cells = Vec::new();
        for (r, row) in grid.iter().enumerate() {
            if !right_only && is_body_token(row[min]) {
                cells.push((r, min));
            }
            if !left_only && max != min && is_body_token(row[max]) {
                cells.push((r, max));
            }
        }
        if cells.len() < STEEL_MIN_CELLS {
            return false;
        }
        for (r, c) in cells {
            grid[r][c] = CELL_STEEL;
        }
        true
    } else {
        let best = (0..rows)
            .map(|r| (r, grid[r].iter().filter(|&&c| c == T_TRIM).count()))
            .max_by_key(|&(_, n)| n);
        let Some((r, n)) = best else {
            return false;
        };
        if !(STEEL_MIN_CELLS..=STEEL_GIRDER_MAX_CELLS).contains(&n) {
            return false;
        }
        for ch in grid[r].iter_mut() {
            if *ch == T_TRIM {
                *ch = CELL_STEEL;
            }
        }
        true
    }
}

/// Turns the body bricks next to every explosive into tough cage bricks.
fn pass_cage(grid: &mut Grid, _ctx: PassContext) -> bool {
    let rows = grid.len();
    let mut cage = Vec::new();
    for r in 0..rows {
        for c in 0..COLS {
            if grid[r][c] != T_EXPLOSIVE {
                continue;
            }
            for (dr, dc) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
                let (nr, nc) = (r as isize + dr, c as isize + dc);
                if nr < 0 || nc < 0 || nr >= rows as isize || nc >= COLS as isize {
                    continue;
                }
                let (nr, nc) = (nr as usize, nc as usize);
                if is_body_token(grid[nr][nc]) {
                    cage.push((nr, nc));
                }
            }
        }
    }
    if cage.is_empty() {
        return false;
    }
    for (r, c) in cage {
        grid[r][c] = T_CAGE;
    }
    true
}

/// Turns a diagonal run of body bricks into a chain of explosives.
fn pass_fuse(grid: &mut Grid, ctx: PassContext, rng: &mut Rng) -> bool {
    let rows = grid.len();
    let mut runs: Vec<(usize, usize, isize, usize)> = Vec::new();
    for r in 0..rows {
        for c in 0..COLS {
            for dir in [1isize, -1] {
                let mut len = 0usize;
                loop {
                    let (nr, nc) = (r + len, c as isize + dir * len as isize);
                    if nr >= rows || nc < 0 || nc >= COLS as isize {
                        break;
                    }
                    if !is_body_token(grid[nr][nc as usize]) {
                        break;
                    }
                    len += 1;
                }
                if len >= FUSE_MIN_LEN {
                    runs.push((r, c, dir, len));
                }
            }
        }
    }
    if runs.is_empty() {
        return false;
    }
    let (r, c, dir, len) = runs[rng.usize_below(runs.len())];
    for i in 0..len.min(FUSE_MAX_LEN) {
        let col = (c as isize + dir * i as isize) as usize;
        put(grid, r + i, col, T_EXPLOSIVE, ctx.symmetric);
    }
    true
}

/// Turns the plain bricks of a two-row band into ghost bricks.
fn pass_ghost(grid: &mut Grid, ctx: PassContext, rng: &mut Rng) -> bool {
    let rows = grid.len();
    let max_cells = if ctx.teaching {
        GHOST_TEACHING_MAX_CELLS
    } else {
        GHOST_MAX_CELLS
    };
    // The band spans the whole motif, so the ghost bricks frame it instead of cutting a half.
    let bands: Vec<Vec<(usize, usize)>> = (0..rows.saturating_sub(GHOST_BAND_ROWS - 1))
        .map(|r0| {
            (r0..r0 + GHOST_BAND_ROWS)
                .flat_map(|r| (0..COLS).map(move |c| (r, c)))
                .filter(|&(r, c)| is_plain_token(grid[r][c]))
                .collect::<Vec<_>>()
        })
        .filter(|cells| (GHOST_MIN_CELLS..=max_cells).contains(&cells.len()))
        .collect();
    if bands.is_empty() {
        return false;
    }
    for &(r, c) in &bands[rng.usize_below(bands.len())] {
        grid[r][c] = CELL_GHOST;
    }
    true
}

/// Inserts a lane of moving bricks above, inside or below the motif. A moving brick only sits in
/// a column that touches the shape above or below the lane, so it is never a stray cell.
fn pass_conveyor(grid: &mut Grid, ctx: PassContext, rng: &mut Rng) -> bool {
    if grid.len() + 1 > MAX_REGULAR_ROWS {
        return false;
    }
    let at = match rng.usize_below(3) {
        0 => 0,
        1 => grid.len() / 2,
        _ => grid.len(),
    };
    let touches = |row: Option<&Vec<char>>, c: usize| {
        row.is_some_and(|r| (c.saturating_sub(1)..=(c + 1).min(COLS - 1)).any(|k| r[k] != T_EMPTY))
    };
    let above = at.checked_sub(1).and_then(|i| grid.get(i));
    let below = grid.get(at);
    // The lane is mirrored around the centre of the motif (the grid centre on symmetric sectors),
    // so it frames the shape instead of floating beside it.
    let (lo, hi) = if ctx.symmetric {
        (0, COLS - 1)
    } else {
        bbox_cols(grid).unwrap_or((0, COLS - 1))
    };
    let mirror_sum = lo + hi;
    let eligible: Vec<usize> = (lo.max(1)..=hi.min(COLS - 2))
        .filter(|&c| touches(above, c) || touches(below, c))
        .collect();
    let wanted = if ctx.teaching {
        LANE_TEACHING_BRICKS
    } else {
        LANE_BRICKS
    };
    let mut lane = vec![T_EMPTY; COLS];
    let mut placed = 0usize;
    let mut pool = eligible;
    while placed < wanted && !pool.is_empty() {
        let c = pool.remove(rng.usize_below(pool.len()));
        let mirror = mirror_sum - c;
        let cols: Vec<usize> = if mirror != c {
            vec![c, mirror]
        } else {
            vec![c]
        };
        let clear = cols.iter().all(|&col| {
            (col.saturating_sub(LANE_MIN_SPACING - 1)..=(col + LANE_MIN_SPACING - 1).min(COLS - 1))
                .all(|k| lane[k] == T_EMPTY)
        });
        if !clear || (mirror != c && c.abs_diff(mirror) < LANE_MIN_SPACING) {
            continue;
        }
        for col in cols {
            lane[col] = CELL_MOVING;
            placed += 1;
        }
    }
    if placed < LANE_TEACHING_BRICKS - 1 {
        return false;
    }
    grid.insert(at, lane);
    true
}

/// Places exactly two portals on empty cells next to the shape.
fn pass_portals(grid: &mut Grid, ctx: PassContext, rng: &mut Rng) -> bool {
    let rows = grid.len();
    if ctx.symmetric {
        let candidates: Vec<(usize, usize)> = (0..rows)
            .flat_map(|r| (0..=PORTAL_MAX_INSET).map(move |c| (r, c)))
            .filter(|&(r, c)| grid[r][c] == T_EMPTY && grid[r][COLS - 1 - c] == T_EMPTY)
            .collect();
        if candidates.is_empty() {
            return false;
        }
        let (r, c) = candidates[rng.usize_below(candidates.len())];
        put(grid, r, c, CELL_PORTAL, true);
        true
    } else {
        let left: Vec<(usize, usize)> = (0..rows.div_ceil(2))
            .flat_map(|r| (0..=PORTAL_MAX_INSET).map(move |c| (r, c)))
            .filter(|&(r, c)| grid[r][c] == T_EMPTY)
            .collect();
        let right: Vec<(usize, usize)> = (rows / 2..rows)
            .flat_map(|r| (0..=PORTAL_MAX_INSET).map(move |c| (r, COLS - 1 - c)))
            .filter(|&(r, c)| grid[r][c] == T_EMPTY)
            .collect();
        if left.is_empty() || right.is_empty() {
            return false;
        }
        let (r1, c1) = left[rng.usize_below(left.len())];
        let (r2, c2) = right[rng.usize_below(right.len())];
        grid[r1][c1] = CELL_PORTAL;
        grid[r2][c2] = CELL_PORTAL;
        true
    }
}

/// Turns the focal bricks (or a pair of body bricks on the centre row) into bumpers.
fn pass_bumper(grid: &mut Grid, ctx: PassContext, rng: &mut Rng) -> bool {
    let focal = count_tokens(grid, |c| c == T_FOCAL);
    if (BUMPER_FOCAL_RANGE.0..=BUMPER_FOCAL_RANGE.1).contains(&focal) {
        for ch in grid.iter_mut().flatten() {
            if *ch == T_FOCAL {
                *ch = CELL_BUMPER;
            }
        }
        return true;
    }
    let rows = grid.len();
    let r = rows / 2;
    let pairs: Vec<(usize, usize)> = if ctx.symmetric {
        BUMPER_COL_PAIRS.to_vec()
    } else {
        let a = 3 + rng.usize_below(3);
        vec![(a, a + BUMPER_ASYMMETRIC_GAP)]
    };
    for (a, b) in pairs {
        if is_body_token(grid[r][a]) && is_body_token(grid[r][b]) {
            grid[r][a] = CELL_BUMPER;
            grid[r][b] = CELL_BUMPER;
            return true;
        }
    }
    false
}

/// Turns a contiguous run of plain bricks of one row into a regenerating shield.
fn pass_regen(grid: &mut Grid, ctx: PassContext, rng: &mut Rng) -> bool {
    let rows = grid.len();
    let mut runs: Vec<(usize, usize, usize)> = Vec::new();
    for (r, row) in grid.iter().enumerate() {
        let mut c = 0;
        while c < COLS {
            if is_plain_token(row[c]) {
                let start = c;
                while c < COLS && is_plain_token(row[c]) {
                    c += 1;
                }
                if c - start >= SHIELD_MIN_RUN {
                    runs.push((r, start, c - start));
                }
            } else {
                c += 1;
            }
        }
    }
    if runs.is_empty() || rows == 0 {
        return false;
    }
    let (r, start, len) = runs[rng.usize_below(runs.len())];
    let take = len.min(SHIELD_MAX_RUN);
    let from = start + (len - take) / 2;
    for c in from..from + take {
        put(grid, r, c, CELL_REGEN, ctx.symmetric);
    }
    true
}

// ---------------------------------------------------------------------------------------------
// Style: colours and toughness, per band or feature
// ---------------------------------------------------------------------------------------------

/// Colour and toughness choices of a sector, all made per feature or per row band.
#[derive(Debug, Clone, Copy)]
struct Style {
    band_rows: usize,
    base: usize,
    tough_bands: usize,
    tough_from_bottom: bool,
    tough_focal: bool,
    tough_trim: bool,
    defuse: bool,
}

fn style_for(plan: &SectorPlan, hardness: usize, rng: &mut Rng) -> Style {
    Style {
        band_rows: rng.pick(&BAND_ROW_CHOICES),
        base: (plan.world + plan.sector + rng.usize_below(NORMAL_COLOURS.len()))
            % NORMAL_COLOURS.len(),
        tough_bands: TOUGH_BANDS_BY_HARDNESS[hardness.min(MAX_HARDNESS)],
        tough_from_bottom: rng.usize_below(2) == 0,
        tough_focal: hardness >= TOUGH_FOCAL_FROM,
        tough_trim: hardness >= TOUGH_TRIM_FROM,
        defuse: hardness >= DEFUSE_FROM,
    }
}

fn normal_colour(index: usize) -> char {
    NORMAL_COLOURS[index % NORMAL_COLOURS.len()]
}

fn tough_colour(index: usize) -> char {
    TOUGH_COLOURS[index % TOUGH_COLOURS.len()]
}

/// Turns the motif tokens into level cells: body bricks take one colour per row band, trim and
/// focal bricks one colour each, tough features one tough digit.
fn colourise(grid: &Grid, style: &Style) -> Vec<String> {
    let first_row = grid
        .iter()
        .position(|row| row.iter().any(|&c| c != T_EMPTY))
        .unwrap_or(0);
    let band_of = |r: usize| r.saturating_sub(first_row) / style.band_rows;
    let mut body_bands: Vec<usize> = grid
        .iter()
        .enumerate()
        .filter(|(_, row)| row.contains(&T_BODY))
        .map(|(r, _)| band_of(r))
        .collect();
    body_bands.dedup();
    let tough: BTreeSet<usize> = if style.tough_from_bottom {
        body_bands
            .iter()
            .rev()
            .take(style.tough_bands)
            .copied()
            .collect()
    } else {
        body_bands.iter().take(style.tough_bands).copied().collect()
    };
    let body_colour = |r: usize| {
        let band = band_of(r);
        if tough.contains(&band) {
            tough_colour(band)
        } else {
            normal_colour(style.base + band)
        }
    };
    grid.iter()
        .enumerate()
        .map(|(r, row)| {
            row.iter()
                .map(|&ch| match ch {
                    T_EMPTY => CELL_EMPTY,
                    T_BODY => body_colour(r),
                    T_TRIM if style.tough_trim => tough_colour(style.base + band_of(r)),
                    T_TRIM => normal_colour(style.base + TRIM_COLOUR_OFFSET + band_of(r)),
                    T_FOCAL if style.tough_focal => tough_colour(style.base + 1),
                    T_FOCAL => normal_colour(style.base + FOCAL_COLOUR_OFFSET),
                    T_CAGE => tough_colour(style.base),
                    T_EXPLOSIVE if style.defuse => body_colour(r),
                    other => other,
                })
                .collect()
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Level assembly
// ---------------------------------------------------------------------------------------------

fn find_motif(name: &str) -> Option<&'static MotifTemplate> {
    MOTIFS.iter().find(|m| m.name == name)
}

fn token_count(motif: &MotifTemplate) -> usize {
    motif
        .rows
        .iter()
        .flat_map(|r| r.chars())
        .filter(|&c| c != T_EMPTY)
        .count()
}

/// Whether a motif can become asymmetric: it already differs from its mirror by enough cells, or
/// it leaves room to be offset inside the grid.
fn can_be_asymmetric(motif: &MotifTemplate) -> bool {
    let mismatch: usize = motif
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
        .sum();
    mismatch >= ASYMMETRY_MIN_MISMATCH
        || bbox_cols(&motif_grid(motif)).is_some_and(|(lo, hi)| lo + (COLS - 1 - hi) >= 2)
}

/// Picks the motif of a sector: the set piece of its block, or a motif of the block theme whose
/// size fits the density window of the sector.
fn pick_motif(plan: &SectorPlan, window: (usize, usize), rng: &mut Rng) -> &'static MotifTemplate {
    if plan.kind == SectorKind::SetPiece {
        if let Some(motif) = set_piece_index(plan.block)
            .and_then(|i| SET_PIECES.get(i))
            .and_then(|piece| find_motif(piece.motifs[(plan.world - 1) % SET_PIECE_VARIANTS]))
        {
            return motif;
        }
    }
    let (primary, secondary) =
        BLOCK_FAMILIES[plan.world_idx()][plan.block.min(BLOCKS_PER_WORLD - 1)];
    let family = if plan.pos % 3 == 2 {
        secondary
    } else {
        primary
    };
    let fits = |m: &&MotifTemplate| {
        let cells = token_count(m);
        cells + PASS_SLACK >= window.0
            && cells <= window.1 + PASS_SLACK
            && (plan.symmetric || can_be_asymmetric(m))
    };
    let themed: Vec<&MotifTemplate> = MOTIFS
        .iter()
        .filter(|m| m.family == family)
        .filter(fits)
        .collect();
    if themed.len() >= MIN_THEMED_POOL {
        return themed[rng.usize_below(themed.len())];
    }
    let any: Vec<&MotifTemplate> = MOTIFS.iter().filter(fits).collect();
    if any.is_empty() {
        &MOTIFS[rng.usize_below(MOTIFS.len())]
    } else {
        any[rng.usize_below(any.len())]
    }
}

/// Picks the primary and the optional secondary pass of a sector.
fn pick_passes(plan: &SectorPlan, rng: &mut Rng) -> (StructurePass, Option<StructurePass>) {
    let w = plan.world_idx();
    match plan.kind {
        SectorKind::Teaching => (rng.pick(TEACHING_PASSES[w]), None),
        SectorKind::SetPiece => (SIGNATURE_PASSES[w], SET_PIECE_SECONDARY[w]),
        SectorKind::Breather | SectorKind::Standard => {
            let primary = rng.pick(WORLD_PASS_MIX[w]);
            let secondary = if plan.block >= SECONDARY_PASS_FROM_BLOCK
                && plan.kind == SectorKind::Standard
                && rng.chance(SECONDARY_PASS_PERCENT)
            {
                Some(rng.pick(WORLD_PASS_MIX[w])).filter(|p| *p != primary && *p != Open)
            } else {
                None
            };
            (primary, secondary)
        }
    }
}

/// Why a rendered level misses a design rule (`None` when it satisfies all of them).
fn layout_issue(rows: &[String], plan: &SectorPlan, window: (usize, usize)) -> Option<String> {
    if !(MIN_REGULAR_ROWS..=MAX_REGULAR_ROWS).contains(&rows.len()) {
        return Some(format!("{} rows", rows.len()));
    }
    let cells = rows
        .iter()
        .flat_map(|r| r.chars())
        .filter(|&c| is_destructible_cell(c))
        .count();
    if cells < window.0 || cells > window.1 {
        return Some(format!(
            "{cells} destructible bricks outside the window {}..={}",
            window.0, window.1
        ));
    }
    let level = LevelDef {
        id: 0,
        world: plan.world,
        name: String::new(),
        boss: false,
        rows: rows.to_vec(),
    };
    let mismatch = mirror_mismatch_cells(&level);
    if plan.symmetric && mismatch != 0 {
        return Some(format!("symmetric sector has {mismatch} mismatching cells"));
    }
    if !plan.symmetric && mismatch < ASYMMETRY_MIN_MISMATCH {
        return Some(format!(
            "asymmetric sector has only {mismatch} mismatching cells"
        ));
    }
    let portals = rows
        .iter()
        .flat_map(|r| r.chars())
        .filter(|&c| c == CELL_PORTAL)
        .count();
    if portals != 0 && portals != 2 {
        return Some(format!("{portals} portals"));
    }
    for &(mechanic, debut) in &MECHANIC_DEBUTS {
        if plan.world < debut && rows.iter().any(|r| r.contains(mechanic)) {
            return Some(format!(
                "mechanic {mechanic} before its debut world {debut}"
            ));
        }
        if plan.world == debut
            && matches!(plan.kind, SectorKind::Teaching)
            && !rows.iter().any(|r| r.contains(mechanic))
        {
            return Some(format!(
                "teaching sector without the debut mechanic {mechanic}"
            ));
        }
    }
    if !plan.symmetric
        && (inner_gap_cols(rows) > MAX_INNER_GAP_COLS || inner_gap_rows(rows) > MAX_INNER_GAP_ROWS)
    {
        return Some("asymmetric level reads as two separate pieces".to_string());
    }
    let (dust, components) = dust_and_components(rows);
    if dust > MAX_DUST {
        return Some(format!("{dust} dust fragments"));
    }
    if components > MAX_COMPONENTS_8 {
        return Some(format!("{components} connected components"));
    }
    if colour_regions_per_cell(rows) > MAX_COLOUR_REGIONS_PER_CELL {
        return Some("colour fragmentation".to_string());
    }
    None
}

fn build_regular_rows(
    plan: &SectorPlan,
    hardness: usize,
    rng: &mut Rng,
) -> Result<Vec<String>, String> {
    let window = density_window(plan.world, plan.sector);
    let teaching = plan.kind == SectorKind::Teaching;
    let motif = pick_motif(plan, window, rng);
    let mut grid = motif_grid(motif);

    // Orientation: symmetric sectors mirror one half of the motif; asymmetric ones are flipped,
    // trimmed by whole edge columns and offset inside the grid, but never cropped by rows.
    if plan.symmetric {
        mirror_half(&mut grid, rng.usize_below(2) == 0);
    } else {
        orient_asymmetric(&mut grid, rng);
    }
    if rng.usize_below(FLIP_VERTICAL_ODDS) == 0 {
        grid.reverse();
    }
    if plan.symmetric && rng.chance(CROP_PERCENT) {
        // Crop rows off one end and one column off each side.
        let spare_rows = grid
            .len()
            .saturating_sub(MIN_REGULAR_ROWS + 1)
            .min(MAX_CROP_ROWS);
        let crop = rng.usize_below(spare_rows + 1);
        if crop > 0 {
            if rng.usize_below(2) == 0 {
                grid.drain(..crop);
            } else {
                grid.truncate(grid.len() - crop);
            }
        }
        let side = rng.usize_below(2);
        crop_columns(&mut grid, side, side);
    }

    let ctx = PassContext {
        symmetric: plan.symmetric,
        teaching,
    };
    let (primary, secondary) = pick_passes(plan, rng);
    if primary.debut_world() > plan.world {
        return Err(format!("pass {primary:?} before its debut world"));
    }
    if !apply_pass(primary, &mut grid, ctx, rng) {
        return Err(format!(
            "pass {primary:?} does not fit motif {}",
            motif.name
        ));
    }
    if let Some(second) = secondary.filter(|p| p.debut_world() <= plan.world) {
        // The second pass is a garnish: a motif without room for it keeps its first pass.
        let mut garnished = grid.clone();
        if apply_pass(second, &mut garnished, ctx, rng) {
            grid = garnished;
        }
    }

    // Empty rows above the shape move the whole feature down.
    let spare = MAX_REGULAR_ROWS.saturating_sub(grid.len()).min(MAX_TOP_PAD);
    let pad = rng.usize_below(spare + 1);
    for _ in 0..pad {
        grid.insert(0, vec![T_EMPTY; COLS]);
    }

    let style = style_for(plan, hardness, rng);
    let rows = colourise(&grid, &style);
    match layout_issue(&rows, plan, window) {
        Some(issue) => Err(issue),
        None => Ok(rows),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::{is_asymmetric, is_mirror_symmetric};

    fn request(world: usize, sector: usize, seed: u64, hardness: usize) -> SectorRequest {
        SectorRequest {
            id: (world - 1) * 100 + sector,
            world,
            sector_in_world: sector,
            is_last: false,
            seed,
            hardness,
        }
    }

    /// Candidate levels of a world over a handful of seeds.
    fn sample_world(world: usize, seeds: u64) -> Vec<(usize, LevelDef)> {
        let mut out = Vec::new();
        for sector in 1..=100 {
            if is_boss_sector(sector) {
                continue;
            }
            for seed in 0..seeds {
                let mut used = BTreeSet::new();
                if let Ok(level) = generate_sector(&request(world, sector, seed, 2), &mut used) {
                    out.push((sector, level));
                    break;
                }
            }
        }
        out
    }

    /// Number of seeds tried per motif by the orientation tests.
    const ORIENTATION_SEEDS: u64 = 40;

    #[test]
    fn asymmetric_orientation_keeps_each_motif_intact() {
        for motif in MOTIFS.iter() {
            let original = motif_grid(motif);
            let total = count_tokens(&original, |c| c != T_EMPTY);
            for seed in 0..ORIENTATION_SEEDS {
                let mut view = original.clone();
                orient_asymmetric(&mut view, &mut Rng::new(seed));
                assert_eq!(view.len(), original.len(), "{} lost a row", motif.name);
                let kept = count_tokens(&view, |c| c != T_EMPTY);
                assert!(
                    kept * 100 >= total * (100 - TRIM_MAX_CELL_PERCENT),
                    "{}: {kept} of {total} cells survive",
                    motif.name
                );
                // The view is the motif (or its mirror image), shifted as a whole, with at most
                // MAX_TRIM_COLS whole edge columns removed: no row is cut through.
                let explained = [false, true].iter().any(|&flip| {
                    (-(COLS as isize)..COLS as isize).any(|dx| {
                        original.iter().zip(&view).all(|(src, got)| {
                            let src: Vec<char> = if flip {
                                src.iter().rev().copied().collect()
                            } else {
                                src.clone()
                            };
                            (0..COLS).all(|c| {
                                let shown = got
                                    .get((c as isize + dx) as usize)
                                    .copied()
                                    .filter(|_| (c as isize + dx) >= 0)
                                    .unwrap_or(T_EMPTY);
                                if shown != T_EMPTY {
                                    shown == src[c]
                                } else {
                                    let in_view = (c as isize + dx) >= 0
                                        && ((c as isize + dx) as usize) < COLS;
                                    src[c] == T_EMPTY
                                        || !in_view
                                        || !(MAX_TRIM_COLS..COLS - MAX_TRIM_COLS).contains(&c)
                                }
                            })
                        })
                    })
                });
                assert!(explained, "{} (seed {seed}) was cut through", motif.name);
            }
        }
    }

    #[test]
    fn ghost_band_takes_every_plain_brick_of_its_rows() {
        let ctx = PassContext {
            symmetric: false,
            teaching: false,
        };
        let mut applied = 0usize;
        for motif in MOTIFS.iter() {
            for seed in 0..ORIENTATION_SEEDS {
                let mut grid = motif_grid(motif);
                if !pass_ghost(&mut grid, ctx, &mut Rng::new(seed)) {
                    continue;
                }
                applied += 1;
                for row in grid.iter().filter(|row| row.contains(&CELL_GHOST)) {
                    assert!(
                        !row.iter().any(|&c| is_plain_token(c)),
                        "{}: a ghost row keeps plain bricks",
                        motif.name
                    );
                }
            }
        }
        assert!(applied > 0);
    }

    #[test]
    fn asymmetric_levels_read_as_one_composition() {
        let mut checked = 0usize;
        for world in 1..=8 {
            for (sector, level) in sample_world(world, 40) {
                if is_symmetric_sector(sector) {
                    continue;
                }
                checked += 1;
                assert!(
                    is_asymmetric(&level),
                    "world {world} sector {sector}: not asymmetric"
                );
                assert!(
                    inner_gap_cols(&level.rows) <= MAX_INNER_GAP_COLS
                        && inner_gap_rows(&level.rows) <= MAX_INNER_GAP_ROWS,
                    "world {world} sector {sector} is a collage:\n{}",
                    level.rows.join("\n")
                );
                let (dust, _) = dust_and_components(&level.rows);
                assert_eq!(dust, 0, "world {world} sector {sector} has loose bricks");
            }
        }
        assert!(checked > 200, "only {checked} asymmetric levels sampled");
    }

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
    fn all_785_names_are_unique_short_and_free_of_digits_and_final() {
        let mut used = BTreeSet::new();
        let mut count = 0usize;
        for world in 1..=8 {
            for sector in 1..=100 {
                if world == 1 && sector <= 15 {
                    continue;
                }
                let boss = is_boss_sector(sector);
                let last = world == 8 && sector == 100;
                let name =
                    generate_unique_name(world, sector, boss, last, &mut used).expect("name");
                assert!(name.len() <= MAX_LEVEL_NAME_LEN, "{name}");
                assert!(!name.chars().any(|c| c.is_ascii_digit()), "{name}");
                assert!(last || !name.starts_with("FINAL"), "{name}");
                count += 1;
            }
        }
        assert_eq!(count, 785);
        assert_eq!(used.len(), 785);
    }

    #[test]
    fn symmetric_sectors_are_forty_percent_and_exactly_mirrored() {
        let symmetric = (1..=100)
            .filter(|&s| !is_boss_sector(s) && is_symmetric_sector(s))
            .count();
        assert!((38..=40).contains(&symmetric), "{symmetric}");
        for world in [1usize, 4, 8] {
            for (sector, level) in sample_world(world, 40) {
                if is_symmetric_sector(sector) {
                    assert!(is_mirror_symmetric(&level), "w{world} s{sector}");
                } else {
                    assert!(is_asymmetric(&level), "w{world} s{sector}");
                    assert!(
                        mirror_mismatch_cells(&level) >= ASYMMETRY_MIN_MISMATCH,
                        "w{world} s{sector}"
                    );
                }
            }
        }
    }

    #[test]
    fn generation_is_deterministic() {
        for world in [1usize, 5] {
            let a = sample_world(world, 40);
            let b = sample_world(world, 40);
            assert_eq!(a.len(), b.len());
            for ((_, x), (_, y)) in a.iter().zip(&b) {
                assert_eq!(x.rows, y.rows);
                assert_eq!(x.name, y.name);
            }
        }
    }

    #[test]
    fn generated_levels_vary_row_counts_and_obey_the_mechanic_order() {
        for world in 1..=8usize {
            let levels = sample_world(world, 60);
            assert!(levels.len() >= 80, "world {world}: {} levels", levels.len());
            let row_counts: BTreeSet<usize> = levels.iter().map(|(_, l)| l.rows.len()).collect();
            assert!(row_counts.len() >= 4, "world {world}: {row_counts:?}");
            for (sector, level) in &levels {
                for &(mechanic, debut) in &MECHANIC_DEBUTS {
                    if world < debut {
                        assert!(
                            !level.rows.iter().any(|r| r.contains(mechanic)),
                            "w{world} s{sector} uses {mechanic}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn debut_mechanics_appear_in_the_teaching_block() {
        for &(mechanic, debut) in &MECHANIC_DEBUTS {
            let levels = sample_world(debut, 60);
            assert!(
                levels
                    .iter()
                    .any(|(s, l)| *s <= SECTORS_PER_BLOCK
                        && l.rows.iter().any(|r| r.contains(mechanic))),
                "mechanic {mechanic} missing from the first block of world {debut}"
            );
        }
    }

    #[test]
    fn set_pieces_name_existing_motifs_and_cover_every_non_boss_block() {
        let mut all = BTreeSet::new();
        for piece in &SET_PIECES {
            assert!(!piece.theme.is_empty());
            for name in piece.motifs {
                let motif = find_motif(name).expect(name);
                assert!(
                    (SAFE_DENSITY_MIN..=SAFE_DENSITY_MAX).contains(&token_count(motif)),
                    "set piece {name} does not fit the density band"
                );
                assert!(all.insert(name), "set piece motif {name} is used twice");
            }
        }
        let covered: Vec<usize> = (0..BLOCKS_PER_WORLD).filter_map(set_piece_index).collect();
        assert_eq!(covered, vec![0, 1, 2, 3, 4, 5, 6, 7]);
        assert!(set_piece_index(4).is_none() && set_piece_index(9).is_none());
        // Sector 10 of a block is symmetric, so a showcase is always mirrored.
        for block in 0..BLOCKS_PER_WORLD {
            assert!(is_symmetric_sector((block + 1) * SECTORS_PER_BLOCK));
        }
    }

    #[test]
    fn shift_never_wraps_and_stays_inside_the_bounding_box() {
        let mut grid: Grid = vec![".XX............".chars().collect()];
        shift_clamped(&mut grid, -3);
        assert_eq!(grid[0].iter().collect::<String>(), "XX.............");
        shift_clamped(&mut grid, 30);
        assert_eq!(grid[0].iter().collect::<String>(), ".............XX");
    }

    #[test]
    fn hardness_follows_the_wanted_rate() {
        assert_eq!(hardness_for_rate(0.9), 0);
        assert_eq!(hardness_for_rate(0.5), 2);
        assert_eq!(hardness_for_rate(0.25), 4);
        assert_eq!(hardness_for_rate(0.0), MAX_HARDNESS);
    }
}
