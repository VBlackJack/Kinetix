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

use crate::config::{ArenaConfig, GameConfig, LevelDef};
use crate::level_code::{EDITOR_COLS, EDITOR_ROWS};
use serde::{Deserialize, Serialize};

const BRICK_TOP_OFFSET: f32 = 140.0;
const BRICK_HEIGHT: f32 = 26.0;
const BRICK_GAP: f32 = 5.0;
const TRAIL_MAX_LEN: usize = 22;
const LASER_WIDTH: f32 = 4.0;
const LASER_HEIGHT: f32 = 22.0;
const MIN_VERTICAL_RATIO: f32 = 0.25;
const SHIELD_Y_OFFSET: f32 = 32.0;
const BOSS_WIDTH: f32 = 230.0;
const BOSS_HEIGHT: f32 = 48.0;
const BOSS_Y: f32 = 165.0;
const HAZARD_RADIUS: f32 = 9.5;
/// Minimum delay between two damaging hits of the same ball on the boss, so that a ball
/// squeezed against the boss cannot drain its HP at substep rate.
const BOSS_HIT_COOLDOWN_SEC: f32 = 0.12;
const MOVING_BRICK_SPEED: f32 = 180.0;
/// Distance kept between the arena side walls and moving bricks, on top of the largest ball
/// diameter, so that a moving brick can never squeeze a ball against a wall.
const MOVING_BRICK_WALL_EXTRA_MARGIN: f32 = 2.0;
const BRICK_GRID_SIDE_MARGIN: f32 = 160.0;
const HAZARD_EXPLOSION_RADIUS: f32 = 55.0;
/// Seed used by `GameWorld::new`, keeping fresh worlds deterministic until `reseed` is called.
const DEFAULT_SEED: u64 = 0x853C_49E6_748F_EA9B;
/// Fallback xorshift state when a scrambled seed would be zero (an all-zero state never advances).
const NONZERO_STATE_FALLBACK: u64 = 0x9E37_79B9_7F4A_7C15;
/// Maximum number of simultaneously falling capsules. The separation logic in `step_capsules`
/// handles exactly two capsules, so this must stay at 2.
const MAX_CAPSULES: usize = 2;
/// Horizontal distance under which two falling capsules push each other apart.
const CAPSULE_SEPARATION_DX: f32 = 210.0;
/// Vertical distance under which two falling capsules push each other apart.
const CAPSULE_SEPARATION_DY: f32 = 160.0;
/// Horizontal speed of the capsule separation push.
const CAPSULE_SEPARATION_SPEED: f32 = 140.0;
/// Maximum horizontal offset used by the vortex pull on capsules.
const CAPSULE_VORTEX_MAX_OFFSET: f32 = 220.0;
/// Gain applied to the vortex pull on capsules.
const CAPSULE_VORTEX_GAIN: f32 = 1.8;
/// Horizontal range over which the vortex pulls balls toward the paddle.
const BALL_VORTEX_RANGE: f32 = 360.0;
/// Fraction of the capsule radius that counts toward a catch, horizontally.
const CAPSULE_CATCH_X_FACTOR: f32 = 0.85;
/// Fraction of the capsule radius that counts toward a catch, vertically.
const CAPSULE_CATCH_Y_FACTOR: f32 = 0.72;
/// Maximum paddle-offset ratio when releasing a stuck ball.
const LAUNCH_RATIO_CLAMP: f32 = 0.85;
/// Maximum paddle-offset ratio for a paddle bounce.
const BOUNCE_RATIO_CLAMP: f32 = 0.95;
/// Initial ball direction (x component) before launch.
const INITIAL_DIR_X: f32 = 0.35;
/// Initial ball direction (y component) before launch.
const INITIAL_DIR_Y: f32 = -0.93;
/// Distance below the arena at which a ball is considered lost.
const OUT_OF_ARENA_MARGIN: f32 = 20.0;
/// Brick hit-flash decay rate per second.
const BRICK_FLASH_DECAY: f32 = 5.0;
/// Boss hit-flash decay rate per second.
const BOSS_FLASH_DECAY: f32 = 4.5;
/// Minimum vortex energy required to activate the vortex.
const VORTEX_MIN_ENERGY: f32 = 5.0;
/// Vortex energy at or below which an active vortex shuts off.
const VORTEX_CUTOFF_ENERGY: f32 = 0.5;
/// Colour index of the boss hit event.
const BOSS_HIT_COLOR_IDX: usize = 1;
/// Colour index of portal and indestructible hit events.
const SOLID_HIT_COLOR_IDX: usize = 0;
/// Colour index of ghost hit events.
const GHOST_HIT_COLOR_IDX: usize = 2;
/// Colour index used for explosive brick score events.
const EXPLOSIVE_COLOR_IDX: usize = 5;
/// Hit points of a freshly loaded moving brick.
pub const MOVING_BRICK_HP: u8 = 2;
/// Colour index of moving bricks.
pub const MOVING_BRICK_COLOR_IDX: usize = 3;
/// First level-file digit that loads as a two-hit standard brick.
const TOUGH_BRICK_DIGIT: usize = 5;
/// Hit points of a two-hit standard brick.
const TOUGH_BRICK_HP: u8 = 2;
/// Number of distinct brick colours cycled by level-file digits.
const BRICK_COLOR_COUNT: usize = 6;
/// Laser bolt origin offset from the paddle centre, as a fraction of the paddle width.
const LASER_OFFSET_RATIO: f32 = 0.36;
/// Boss turret positions as fractions of the boss width.
const BOSS_LEFT_TURRET_RATIO: f32 = 0.25;
const BOSS_RIGHT_TURRET_RATIO: f32 = 0.75;
/// Boss damage dealt by a mega or fire ball.
const BOSS_POWER_DAMAGE: u32 = 2;
/// Hit points of a freshly loaded bumper brick.
pub const BUMPER_BRICK_HP: u8 = 2;
/// Colour index of bumper bricks.
pub const BUMPER_COLOR_IDX: usize = 4;
/// Hit points of a freshly loaded regenerating brick.
pub const REGEN_BRICK_HP: u8 = 2;
/// Flash intensity of a brick that was just painted in the editor.
const EDITOR_PAINT_FLASH: f32 = 0.5;
/// Flash intensity of a regenerating brick that just healed.
const REGEN_HEAL_FLASH: f32 = 0.6;
/// Colour index of regenerating bricks.
pub const REGEN_BRICK_COLOR_IDX: usize = 1;

/// Single definition of the brick grid, shared by level loading, editor painting and export.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BrickGrid {
    start_x: f32,
    play_w: f32,
    cols: usize,
}

impl BrickGrid {
    pub(crate) fn new(arena: &ArenaConfig, cols: usize) -> Self {
        let play_w = arena.width - (arena.wall_side + BRICK_GRID_SIDE_MARGIN) * 2.0;
        Self {
            start_x: arena.wall_side + BRICK_GRID_SIDE_MARGIN,
            play_w,
            cols: cols.max(1),
        }
    }

    pub(crate) fn brick_w(&self) -> f32 {
        (self.play_w - (self.cols as f32 - 1.0) * BRICK_GAP) / self.cols as f32
    }

    /// Top-left corner of the cell at (`col`, `row`).
    pub(crate) fn cell_origin(&self, col: usize, row: usize) -> (f32, f32) {
        (
            self.start_x + col as f32 * (self.brick_w() + BRICK_GAP),
            BRICK_TOP_OFFSET + row as f32 * (BRICK_HEIGHT + BRICK_GAP),
        )
    }

    /// Cell containing the world point, if it lies inside the grid of `max_rows` rows.
    pub(crate) fn cell_at(&self, x: f32, y: f32, max_rows: usize) -> Option<(usize, usize)> {
        if x < self.start_x || x > self.start_x + self.play_w || y < BRICK_TOP_OFFSET {
            return None;
        }
        let col = ((x - self.start_x) / (self.brick_w() + BRICK_GAP)).floor() as usize;
        let row = ((y - BRICK_TOP_OFFSET) / (BRICK_HEIGHT + BRICK_GAP)).floor() as usize;
        if col >= self.cols || row >= max_rows {
            return None;
        }
        Some((col, row))
    }
}

/// Result of a circle versus rectangle overlap test.
#[derive(Debug, Clone, Copy)]
struct RectContact {
    /// Unit axis-aligned normal pointing out of the rectangle.
    nx: f32,
    ny: f32,
    /// Displacement that moves the ball out of the rectangle.
    push_x: f32,
    push_y: f32,
}

/// Tests a circle against a rectangle. The normal follows the minimum penetration axis, also when
/// the circle centre is inside the rectangle.
fn rect_contact(ball: &Ball, rx: f32, ry: f32, rw: f32, rh: f32) -> Option<RectContact> {
    let (bx, by, r) = (ball.x, ball.y, ball.radius);
    let cx = bx.clamp(rx, rx + rw);
    let cy = by.clamp(ry, ry + rh);
    let dx = bx - cx;
    let dy = by - cy;
    let dist_sq = dx * dx + dy * dy;
    if dist_sq > r * r {
        return None;
    }
    if dist_sq > 0.0 {
        let dist = dist_sq.sqrt();
        let pen = r - dist;
        let (nx, ny) = if dx.abs() > dy.abs() {
            (dx.signum(), 0.0)
        } else {
            (0.0, dy.signum())
        };
        return Some(RectContact {
            nx,
            ny,
            push_x: dx / dist * pen,
            push_y: dy / dist * pen,
        });
    }
    // Centre inside the rectangle: leave through the nearest edge.
    let to_left = bx - rx;
    let to_right = rx + rw - bx;
    let to_top = by - ry;
    let to_bottom = ry + rh - by;
    let min = to_left.min(to_right).min(to_top).min(to_bottom);
    let (nx, ny) = if min == to_top {
        (0.0, -1.0)
    } else if min == to_bottom {
        (0.0, 1.0)
    } else if min == to_left {
        (-1.0, 0.0)
    } else {
        (1.0, 0.0)
    };
    let travel = min + r;
    Some(RectContact {
        nx,
        ny,
        push_x: nx * travel,
        push_y: ny * travel,
    })
}

/// Moves the ball out of the contact and reflects it when it moves into the surface.
/// Returns true when a reflection happened.
fn bounce_off(ball: &mut Ball, contact: &RectContact) -> bool {
    ball.x += contact.push_x;
    ball.y += contact.push_y;
    if ball.vx * contact.nx + ball.vy * contact.ny < 0.0 {
        if contact.nx != 0.0 {
            ball.vx = -ball.vx;
        } else {
            ball.vy = -ball.vy;
        }
        true
    } else {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrickKind {
    Standard {
        hp: u8,
        max_hp: u8,
        color_idx: usize,
    },
    Explosive,
    Indestructible,
    Portal,
    Ghost {
        revealed: bool,
    },
    Moving {
        hp: u8,
        color_idx: usize,
    },
    Bumper {
        hp: u8,
    },
    Regenerating {
        hp: u8,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Brick {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub vx: f32,
    pub kind: BrickKind,
    pub hit_flash: f32,
    /// Seconds left before a damaged regenerating brick heals (unused by other kinds).
    #[serde(default)]
    pub regen_timer: f32,
}

impl Ball {
    /// Speed actually travelled: base plus the bumper boost, never above `max_speed`.
    pub fn effective_speed(&self, max_speed: f32) -> f32 {
        (self.speed + self.boost).min(max_speed)
    }
}

impl Brick {
    pub fn new(x: f32, y: f32, w: f32, h: f32, vx: f32, kind: BrickKind, hit_flash: f32) -> Self {
        Self {
            x,
            y,
            w,
            h,
            vx,
            kind,
            hit_flash,
            regen_timer: 0.0,
        }
    }

    pub fn is_destructible(&self) -> bool {
        !matches!(self.kind, BrickKind::Indestructible | BrickKind::Portal)
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w * 0.5, self.y + self.h * 0.5)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerUpKind {
    Fireball,
    MegaBall,
    MultiBall,
    Expand,
    Shrink,
    Magnet,
    Laser,
    Lightning,
    Slow,
    Fast,
    ExtraLife,
    Shield,
}

impl PowerUpKind {
    pub fn all() -> &'static [PowerUpKind] {
        &[
            Self::Fireball,
            Self::MegaBall,
            Self::MultiBall,
            Self::Expand,
            Self::Shrink,
            Self::Magnet,
            Self::Laser,
            Self::Lightning,
            Self::Slow,
            Self::Fast,
            Self::ExtraLife,
            Self::Shield,
        ]
    }

    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Fireball => "powerups.fireball",
            Self::MegaBall => "powerups.megaball",
            Self::MultiBall => "powerups.multiball",
            Self::Expand => "powerups.expand",
            Self::Shrink => "powerups.shrink",
            Self::Magnet => "powerups.magnet",
            Self::Laser => "powerups.laser",
            Self::Lightning => "powerups.lightning",
            Self::Slow => "powerups.slow",
            Self::Fast => "powerups.fast",
            Self::ExtraLife => "powerups.extra_life",
            Self::Shield => "powerups.shield",
        }
    }

    pub fn is_malus(self) -> bool {
        matches!(self, Self::Shrink | Self::Fast)
    }

    pub fn badge_key(self) -> &'static str {
        match self {
            Self::Fireball => "powerups.badge.fireball",
            Self::MegaBall => "powerups.badge.megaball",
            Self::MultiBall => "powerups.badge.multiball",
            Self::Expand => "powerups.badge.expand",
            Self::Shrink => "powerups.badge.shrink",
            Self::Magnet => "powerups.badge.magnet",
            Self::Laser => "powerups.badge.laser",
            Self::Lightning => "powerups.badge.lightning",
            Self::Slow => "powerups.badge.slow",
            Self::Fast => "powerups.badge.fast",
            Self::ExtraLife => "powerups.badge.extra_life",
            Self::Shield => "powerups.badge.shield",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PowerUpCapsule {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub kind: PowerUpKind,
}

#[derive(Debug, Clone)]
pub struct LaserBolt {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone)]
pub struct BossEntity {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub vx: f32,
    pub hp: u32,
    pub max_hp: u32,
    pub fire_timer: f32,
    pub hit_flash: f32,
}

#[derive(Debug, Clone)]
pub struct HazardProjectile {
    pub x: f32,
    pub y: f32,
    pub vy: f32,
    pub radius: f32,
}

#[derive(Debug, Clone)]
pub struct Ball {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    /// Base speed, changed by hits and the slow / fast power-ups.
    pub speed: f32,
    /// Temporary bumper boost added on top of `speed`, decaying to zero.
    pub boost: f32,
    pub radius: f32,
    pub stuck_offset: Option<f32>,
    pub fireball: bool,
    pub mega: bool,
    pub portal_cooldown: f32,
    pub boss_hit_cooldown: f32,
    pub trail: Vec<(f32, f32)>,
}

/// Multiplier reported when no combo bonus applies.
pub const NO_MULTIPLIER: u32 = 1;

/// Single source of the combo formula: one extra multiplier level per `step` combo hits,
/// capped at `max_mult`.
pub fn combo_multiplier(combo: u32, step: u32, max_mult: u32) -> u32 {
    (NO_MULTIPLIER + combo / step.max(1)).min(max_mult)
}

#[derive(Debug, Clone)]
pub enum GameEvent {
    BrickHit {
        x: f32,
        y: f32,
        color_idx: usize,
        destroyed: bool,
        /// Score awarded by this hit: 0 unless the brick was destroyed.
        points: u64,
        /// Combo multiplier applied to `points` (`NO_MULTIPLIER` when none applies).
        multiplier: u32,
    },
    Explosion {
        x: f32,
        y: f32,
        radius: f32,
    },
    LightningBolt {
        from: (f32, f32),
        to: (f32, f32),
    },
    PaddleHit {
        x: f32,
        y: f32,
    },
    WallHit {
        x: f32,
        y: f32,
    },
    PowerUpCollected {
        kind: PowerUpKind,
        x: f32,
        y: f32,
    },
    LaserFired {
        x: f32,
        y: f32,
    },
    PortalWarp {
        from: (f32, f32),
        to: (f32, f32),
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Playing,
    Paused,
    Victory,
    GameOver,
}

pub struct GameWorld {
    pub config: GameConfig,
    pub status: SessionStatus,
    pub paddle_x: f32,
    pub paddle_width: f32,
    pub magnet_active: bool,
    pub laser_active: bool,
    pub lightning_active: bool,
    pub shield_hits: u8,
    pub vortex_active: bool,
    pub vortex_energy: f32,
    pub level_elapsed: f32,
    pub sudden_death_timer: f32,
    pub laser_cooldown: f32,
    pub balls: Vec<Ball>,
    pub bricks: Vec<Brick>,
    pub capsules: Vec<PowerUpCapsule>,
    pub lasers: Vec<LaserBolt>,
    pub boss: Option<BossEntity>,
    pub hazards: Vec<HazardProjectile>,
    pub score: u64,
    pub high_score: u64,
    pub max_unlocked_level: usize,
    /// True only while the board comes straight from a campaign sector; editing or importing
    /// turns it into a custom board, which can never unlock campaign progress.
    pub(crate) campaign_board: bool,
    level_count: usize,
    pub lives: u32,
    pub combo: u32,
    pub combo_timer: f32,
    pub current_level_idx: usize,
    pub last_powerup: Option<PowerUpKind>,
    pub last_powerup_timer: f32,
    pub powerup_spawn_cooldown: f32,
    rng_state: u64,
}

impl GameWorld {
    pub fn new(config: GameConfig, levels: &[LevelDef]) -> Self {
        let max_energy = config.gameplay.vortex_max_energy;
        let mut world = Self {
            paddle_x: config.arena.width * 0.5,
            paddle_width: config.paddle.default_width,
            magnet_active: false,
            laser_active: false,
            lightning_active: false,
            shield_hits: 0,
            vortex_active: false,
            vortex_energy: max_energy,
            level_elapsed: 0.0,
            sudden_death_timer: 0.0,
            laser_cooldown: 0.0,
            balls: Vec::new(),
            bricks: Vec::new(),
            capsules: Vec::new(),
            lasers: Vec::new(),
            boss: None,
            hazards: Vec::new(),
            score: 0,
            high_score: 0,
            max_unlocked_level: 0,
            campaign_board: false,
            level_count: levels.len(),
            lives: config.gameplay.initial_lives,
            combo: 0,
            combo_timer: 0.0,
            current_level_idx: 0,
            last_powerup: None,
            last_powerup_timer: 0.0,
            powerup_spawn_cooldown: 0.0,
            status: SessionStatus::Playing,
            config,
            rng_state: DEFAULT_SEED,
        };
        world.load_level(0, levels);
        world
    }

    /// Reseeds the random generator. The seed is scrambled with splitmix64 so that nearby seeds
    /// yield unrelated sequences, and the resulting state is never zero.
    pub fn reseed(&mut self, seed: u64) {
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        self.rng_state = if z == 0 { NONZERO_STATE_FALLBACK } else { z };
    }

    fn next_rand_f32(&mut self) -> f32 {
        self.rng_state ^= self.rng_state >> 12;
        self.rng_state ^= self.rng_state << 25;
        self.rng_state ^= self.rng_state >> 27;
        let val = self.rng_state.wrapping_mul(0x2545_F491_4F6C_DD1D);
        ((val >> 40) as u32 as f32) / ((1u32 << 24) as f32)
    }

    pub fn shield_y(&self) -> f32 {
        self.config.arena.paddle_y + SHIELD_Y_OFFSET
    }

    pub fn load_level(&mut self, idx: usize, levels: &[LevelDef]) {
        if levels.is_empty() {
            return;
        }
        self.current_level_idx = idx % levels.len();
        let level = &levels[self.current_level_idx];
        self.load_from_rows(&level.rows, level.boss, level.world);
        self.campaign_board = true;
    }

    /// True when the current board is an untouched campaign sector.
    pub fn is_campaign_board(&self) -> bool {
        self.campaign_board
    }

    /// Removes every brick (editor clear); the board stops counting as a campaign sector.
    pub fn clear_bricks(&mut self) {
        self.bricks.clear();
        self.campaign_board = false;
    }

    pub fn load_from_rows(&mut self, rows: &[String], has_boss: bool, world_tier: usize) {
        self.bricks.clear();
        self.capsules.clear();
        self.lasers.clear();
        self.hazards.clear();
        self.level_elapsed = 0.0;
        self.sudden_death_timer = 0.0;
        self.powerup_spawn_cooldown = 0.0;
        self.shield_hits = 0;
        self.status = SessionStatus::Playing;

        if has_boss {
            let tier_mult = world_tier.max(1) as u32;
            let hp = self.config.gameplay.boss_base_hp
                + (tier_mult - 1) * self.config.gameplay.boss_hp_per_tier;
            self.boss = Some(BossEntity {
                x: (self.config.arena.width - BOSS_WIDTH) * 0.5,
                y: BOSS_Y,
                w: BOSS_WIDTH,
                h: BOSS_HEIGHT,
                vx: self.config.gameplay.boss_speed
                    * (1.0 + (tier_mult as f32 - 1.0) * self.config.gameplay.boss_speed_per_tier),
                hp,
                max_hp: hp,
                fire_timer: self.config.gameplay.boss_fire_interval_sec,
                hit_flash: 0.0,
            });
        } else {
            self.boss = None;
        }

        for (r_idx, row) in rows.iter().enumerate() {
            let grid = BrickGrid::new(&self.config.arena, row.chars().count());
            let brick_w = grid.brick_w();
            for (c_idx, ch) in row.chars().enumerate() {
                if ch == '0' || ch == ' ' {
                    continue;
                }
                let (x, y) = grid.cell_origin(c_idx, r_idx);
                let (kind, vx) = match ch {
                    'E' => (BrickKind::Explosive, 0.0),
                    '#' => (BrickKind::Indestructible, 0.0),
                    'T' => (BrickKind::Portal, 0.0),
                    'G' => (BrickKind::Ghost { revealed: false }, 0.0),
                    'B' => (
                        BrickKind::Bumper {
                            hp: BUMPER_BRICK_HP,
                        },
                        0.0,
                    ),
                    'R' => (BrickKind::Regenerating { hp: REGEN_BRICK_HP }, 0.0),
                    'M' => (
                        BrickKind::Moving {
                            hp: MOVING_BRICK_HP,
                            color_idx: MOVING_BRICK_COLOR_IDX,
                        },
                        if r_idx % 2 == 0 {
                            MOVING_BRICK_SPEED
                        } else {
                            -MOVING_BRICK_SPEED
                        },
                    ),
                    d if d.is_ascii_digit() => {
                        let digit = d.to_digit(10).unwrap_or(1) as usize;
                        let hp = if digit >= TOUGH_BRICK_DIGIT {
                            TOUGH_BRICK_HP
                        } else {
                            1
                        };
                        (
                            BrickKind::Standard {
                                hp,
                                max_hp: hp,
                                color_idx: (digit.saturating_sub(1)) % BRICK_COLOR_COUNT,
                            },
                            0.0,
                        )
                    }
                    _ => (
                        BrickKind::Standard {
                            hp: 1,
                            max_hp: 1,
                            color_idx: 0,
                        },
                        0.0,
                    ),
                };
                self.bricks
                    .push(Brick::new(x, y, brick_w, BRICK_HEIGHT, vx, kind, 0.0));
            }
        }
        self.reset_paddle_and_ball();
    }

    pub fn reset_paddle_and_ball(&mut self) {
        self.paddle_width = self.config.paddle.default_width;
        self.magnet_active = false;
        self.laser_active = false;
        self.lightning_active = false;
        self.vortex_active = false;
        self.vortex_energy = self.config.gameplay.vortex_max_energy;
        self.hazards.clear();
        self.balls.clear();
        self.balls.push(Ball {
            x: self.paddle_x,
            y: self.config.arena.paddle_y
                - self.config.paddle.height * 0.5
                - self.config.ball.default_radius
                - 1.0,
            vx: INITIAL_DIR_X,
            vy: INITIAL_DIR_Y,
            speed: self.config.ball.default_speed,
            boost: 0.0,
            radius: self.config.ball.default_radius,
            stuck_offset: Some(0.0),
            fireball: false,
            mega: false,
            portal_cooldown: 0.0,
            boss_hit_cooldown: 0.0,
            trail: Vec::with_capacity(TRAIL_MAX_LEN),
        });
    }

    pub fn paint_brick_at(&mut self, world_x: f32, world_y: f32, kind: Option<BrickKind>) {
        self.campaign_board = false;
        let grid = BrickGrid::new(&self.config.arena, EDITOR_COLS);
        let Some((c_idx, r_idx)) = grid.cell_at(world_x, world_y, EDITOR_ROWS) else {
            return;
        };
        let brick_w = grid.brick_w();
        let (target_x, target_y) = grid.cell_origin(c_idx, r_idx);

        // Moving bricks drift off their cell origin: match on the brick centre instead.
        self.bricks.retain(|b| {
            let (cx, cy) = b.center();
            grid.cell_at(cx, cy, EDITOR_ROWS) != Some((c_idx, r_idx))
        });

        if let Some(k) = kind {
            let vx = if matches!(k, BrickKind::Moving { .. }) {
                MOVING_BRICK_SPEED
            } else {
                0.0
            };
            self.bricks.push(Brick::new(
                target_x,
                target_y,
                brick_w,
                BRICK_HEIGHT,
                vx,
                k,
                EDITOR_PAINT_FLASH,
            ));
        }
    }

    pub fn set_paddle_target(&mut self, target_x: f32) {
        if self.status != SessionStatus::Playing {
            return;
        }
        let half = self.paddle_width * 0.5;
        let min_x = self.config.arena.wall_side + half;
        let max_x = self.config.arena.width - self.config.arena.wall_side - half;
        self.paddle_x = target_x.clamp(min_x, max_x);
    }

    pub fn set_vortex(&mut self, active: bool) {
        self.vortex_active = active && self.vortex_energy > VORTEX_MIN_ENERGY;
    }

    pub fn launch_or_fire(&mut self, events: &mut Vec<GameEvent>) {
        if self.status != SessionStatus::Playing {
            return;
        }
        let mut released_any = false;
        for ball in &mut self.balls {
            if let Some(offset) = ball.stuck_offset.take() {
                let ratio = (offset / (self.paddle_width * 0.5))
                    .clamp(-LAUNCH_RATIO_CLAMP, LAUNCH_RATIO_CLAMP);
                let angle = ratio * self.config.paddle.max_bounce_angle_deg.to_radians();
                ball.vx = angle.sin();
                ball.vy = -angle.cos();
                released_any = true;
            }
        }
        if !released_any && self.laser_active && self.laser_cooldown <= 0.0 {
            self.laser_cooldown = self.config.paddle.laser_cooldown_sec;
            let left_x = self.paddle_x - self.paddle_width * LASER_OFFSET_RATIO;
            let right_x = self.paddle_x + self.paddle_width * LASER_OFFSET_RATIO;
            let y = self.config.arena.paddle_y - self.config.paddle.height * 0.5;
            self.lasers.push(LaserBolt { x: left_x, y });
            self.lasers.push(LaserBolt { x: right_x, y });
            events.push(GameEvent::LaserFired { x: left_x, y });
            events.push(GameEvent::LaserFired { x: right_x, y });
        }
    }

    pub fn update(&mut self, dt: f32, events: &mut Vec<GameEvent>) {
        if self.status != SessionStatus::Playing {
            return;
        }

        if self.balls.iter().any(|b| b.stuck_offset.is_none()) {
            self.level_elapsed += dt;
            if self.level_elapsed >= self.config.gameplay.sudden_death_time_sec {
                self.sudden_death_timer += dt;
                if self.sudden_death_timer >= self.config.gameplay.sudden_death_strike_interval_sec
                {
                    self.sudden_death_timer = 0.0;
                    if let Some((idx, b)) = self
                        .bricks
                        .iter()
                        .enumerate()
                        .find(|(_, b)| b.is_destructible())
                    {
                        let target = b.center();
                        events.push(GameEvent::LightningBolt {
                            from: (self.config.arena.width * 0.5, self.config.arena.wall_top),
                            to: target,
                        });
                        self.damage_brick(idx, true, events);
                    }
                }
            }
        }

        if self.vortex_active {
            self.vortex_energy =
                (self.vortex_energy - self.config.gameplay.vortex_drain_rate * dt).max(0.0);
            if self.vortex_energy <= VORTEX_CUTOFF_ENERGY {
                self.vortex_active = false;
            }
        } else {
            self.vortex_energy = (self.vortex_energy
                + self.config.gameplay.vortex_recharge_rate * dt)
                .min(self.config.gameplay.vortex_max_energy);
        }

        if self.laser_cooldown > 0.0 {
            self.laser_cooldown = (self.laser_cooldown - dt).max(0.0);
        }
        if self.last_powerup_timer > 0.0 {
            self.last_powerup_timer = (self.last_powerup_timer - dt).max(0.0);
        }
        if self.powerup_spawn_cooldown > 0.0 {
            self.powerup_spawn_cooldown = (self.powerup_spawn_cooldown - dt).max(0.0);
        }
        if self.combo_timer > 0.0 {
            self.combo_timer -= dt;
            if self.combo_timer <= 0.0 {
                self.combo = 0;
            }
        }

        // Bumper boosts fade at a constant rate on the frame clock, not per substep.
        let boost_decay = self.config.ball.bumper_speed_boost
            / self.config.ball.bumper_boost_decay_sec.max(f32::EPSILON)
            * dt;
        for ball in &mut self.balls {
            if ball.boost > 0.0 {
                ball.boost = (ball.boost - boost_decay).max(0.0);
            }
        }

        let moving_margin = self.config.ball.mega_radius * 2.0 + MOVING_BRICK_WALL_EXTRA_MARGIN;
        let min_bx = self.config.arena.wall_side + moving_margin;
        let max_bx = self.config.arena.width - self.config.arena.wall_side - moving_margin;
        for brick in &mut self.bricks {
            if brick.hit_flash > 0.0 {
                brick.hit_flash = (brick.hit_flash - dt * BRICK_FLASH_DECAY).max(0.0);
            }
            if let BrickKind::Regenerating { hp } = &mut brick.kind {
                if *hp < REGEN_BRICK_HP && brick.regen_timer > 0.0 {
                    brick.regen_timer -= dt;
                    if brick.regen_timer <= 0.0 {
                        brick.regen_timer = 0.0;
                        *hp = REGEN_BRICK_HP;
                        brick.hit_flash = REGEN_HEAL_FLASH;
                    }
                }
            }
            if brick.vx.abs() > 0.1 {
                brick.x += brick.vx * dt;
                if brick.x < min_bx {
                    brick.x = min_bx;
                    brick.vx = brick.vx.abs();
                } else if brick.x + brick.w > max_bx {
                    brick.x = max_bx - brick.w;
                    brick.vx = -brick.vx.abs();
                }
            }
        }

        self.step_boss_and_hazards(dt, events);

        let substeps = self.config.ball.substeps.max(1);
        let sub_dt = dt / substeps as f32;
        for _ in 0..substeps {
            self.step_balls(sub_dt, events);
        }

        self.step_lasers(dt, events);
        self.step_capsules(dt, events);

        self.balls
            .retain(|b| b.y - b.radius <= self.config.arena.height + OUT_OF_ARENA_MARGIN);
        if self.balls.is_empty() {
            self.lives = self.lives.saturating_sub(1);
            self.combo = 0;
            if self.lives == 0 {
                self.status = SessionStatus::GameOver;
            } else {
                self.reset_paddle_and_ball();
            }
            return;
        }

        let bricks_done = !self.bricks.iter().any(|b| b.is_destructible());
        let boss_done = self.boss.is_none();
        if bricks_done && boss_done {
            self.status = SessionStatus::Victory;
            if self.campaign_board {
                let last_level = self.level_count.saturating_sub(1);
                let unlocked = (self.current_level_idx + 1).min(last_level);
                self.max_unlocked_level = self.max_unlocked_level.max(unlocked);
            }
        }
    }

    fn step_boss_and_hazards(&mut self, dt: f32, events: &mut Vec<GameEvent>) {
        let wall_margin = self.config.ball.mega_radius * 2.0 + MOVING_BRICK_WALL_EXTRA_MARGIN;
        let wall_left = self.config.arena.wall_side + wall_margin;
        let wall_right = self.config.arena.width - self.config.arena.wall_side - wall_margin;

        if let Some(boss) = &mut self.boss {
            if boss.hit_flash > 0.0 {
                boss.hit_flash = (boss.hit_flash - dt * BOSS_FLASH_DECAY).max(0.0);
            }
            boss.x += boss.vx * dt;
            if boss.x < wall_left {
                boss.x = wall_left;
                boss.vx = boss.vx.abs();
            } else if boss.x + boss.w > wall_right {
                boss.x = wall_right - boss.w;
                boss.vx = -boss.vx.abs();
            }

            boss.fire_timer -= dt;
            if boss.fire_timer <= 0.0 {
                boss.fire_timer = self.config.gameplay.boss_fire_interval_sec;
                let hy = boss.y + boss.h;
                let left_turret = boss.x + boss.w * BOSS_LEFT_TURRET_RATIO;
                let right_turret = boss.x + boss.w * BOSS_RIGHT_TURRET_RATIO;
                self.hazards.push(HazardProjectile {
                    x: left_turret,
                    y: hy,
                    vy: self.config.gameplay.boss_hazard_speed,
                    radius: HAZARD_RADIUS,
                });
                self.hazards.push(HazardProjectile {
                    x: right_turret,
                    y: hy,
                    vy: self.config.gameplay.boss_hazard_speed,
                    radius: HAZARD_RADIUS,
                });
                events.push(GameEvent::LaserFired {
                    x: left_turret,
                    y: hy,
                });
            }
        }

        let paddle_y = self.config.arena.paddle_y;
        let paddle_h = self.config.paddle.height;
        let paddle_half_w = self.paddle_width * 0.5;
        let arena_h = self.config.arena.height;
        let mut hit_player = false;

        self.hazards.retain_mut(|h| {
            h.y += h.vy * dt;
            let hit = (h.x - self.paddle_x).abs() <= paddle_half_w + h.radius
                && (h.y - paddle_y).abs() <= paddle_h * 0.5 + h.radius;
            if hit {
                hit_player = true;
                false
            } else {
                h.y - h.radius <= arena_h
            }
        });

        if hit_player {
            events.push(GameEvent::Explosion {
                x: self.paddle_x,
                y: paddle_y,
                radius: HAZARD_EXPLOSION_RADIUS,
            });
            self.shrink_paddle();
        }
    }

    fn step_balls(&mut self, dt: f32, events: &mut Vec<GameEvent>) {
        let wall_left = self.config.arena.wall_side;
        let wall_right = self.config.arena.width - self.config.arena.wall_side;
        let wall_top = self.config.arena.wall_top;
        let paddle_y = self.config.arena.paddle_y;
        let paddle_h = self.config.paddle.height;
        let paddle_half_w = self.paddle_width * 0.5;
        let max_angle = self.config.paddle.max_bounce_angle_deg.to_radians();
        let shield_y = self.shield_y();
        let max_speed = self.config.ball.max_speed;

        let mut pending_explosions = Vec::new();
        let mut pending_lightning = Vec::new();

        for b_idx in 0..self.balls.len() {
            if let Some(offset) = self.balls[b_idx].stuck_offset {
                self.balls[b_idx].x = self.paddle_x + offset;
                self.balls[b_idx].y = paddle_y - paddle_h * 0.5 - self.balls[b_idx].radius - 1.0;
                continue;
            }

            let mut ball = self.balls[b_idx].clone();
            if ball.portal_cooldown > 0.0 {
                ball.portal_cooldown = (ball.portal_cooldown - dt).max(0.0);
            }
            if ball.boss_hit_cooldown > 0.0 {
                ball.boss_hit_cooldown = (ball.boss_hit_cooldown - dt).max(0.0);
            }

            if self.vortex_active {
                let dx_to_paddle = (self.paddle_x - ball.x)
                    .clamp(-BALL_VORTEX_RANGE, BALL_VORTEX_RANGE)
                    / BALL_VORTEX_RANGE;
                ball.vx += dx_to_paddle * self.config.gameplay.vortex_pull_strength * dt;
                let norm = (ball.vx * ball.vx + ball.vy * ball.vy).sqrt().max(0.001);
                ball.vx /= norm;
                ball.vy /= norm;
            }

            let speed = ball.effective_speed(max_speed);
            ball.x += ball.vx * speed * dt;
            ball.y += ball.vy * speed * dt;

            if ball.trail.len() >= TRAIL_MAX_LEN {
                ball.trail.remove(0);
            }
            ball.trail.push((ball.x, ball.y));

            if ball.x - ball.radius < wall_left {
                ball.x = wall_left + ball.radius;
                ball.vx = ball.vx.abs();
                events.push(GameEvent::WallHit {
                    x: ball.x,
                    y: ball.y,
                });
            } else if ball.x + ball.radius > wall_right {
                ball.x = wall_right - ball.radius;
                ball.vx = -ball.vx.abs();
                events.push(GameEvent::WallHit {
                    x: ball.x,
                    y: ball.y,
                });
            }

            if ball.y - ball.radius < wall_top {
                ball.y = wall_top + ball.radius;
                ball.vy = ball.vy.abs();
                events.push(GameEvent::WallHit {
                    x: ball.x,
                    y: ball.y,
                });
            }

            if self.shield_hits > 0 && ball.vy > 0.0 && ball.y + ball.radius >= shield_y {
                ball.y = shield_y - ball.radius - 1.0;
                ball.vy = -ball.vy.abs();
                self.shield_hits = self.shield_hits.saturating_sub(1);
                events.push(GameEvent::PaddleHit {
                    x: ball.x,
                    y: shield_y,
                });
            }

            // Paddle collision
            let p_left = self.paddle_x - paddle_half_w;
            let p_right = self.paddle_x + paddle_half_w;
            let p_top = paddle_y - paddle_h * 0.5;
            let p_bottom = paddle_y + paddle_h * 0.5;

            if ball.vy > 0.0
                && ball.x + ball.radius >= p_left
                && ball.x - ball.radius <= p_right
                && ball.y + ball.radius >= p_top
                && ball.y - ball.radius <= p_bottom
            {
                let rel = ((ball.x - self.paddle_x) / paddle_half_w)
                    .clamp(-BOUNCE_RATIO_CLAMP, BOUNCE_RATIO_CLAMP);
                let angle = rel * max_angle;
                ball.vx = angle.sin();
                ball.vy = -angle.cos();
                ball.y = p_top - ball.radius - 0.5;
                if self.magnet_active {
                    ball.stuck_offset = Some(ball.x - self.paddle_x);
                }
                events.push(GameEvent::PaddleHit {
                    x: ball.x,
                    y: p_top,
                });
            }

            // Boss collision
            if let Some(boss) = &mut self.boss {
                if let Some(contact) = rect_contact(&ball, boss.x, boss.y, boss.w, boss.h) {
                    if bounce_off(&mut ball, &contact) && ball.boss_hit_cooldown <= 0.0 {
                        ball.boss_hit_cooldown = BOSS_HIT_COOLDOWN_SEC;
                        let dmg = if ball.mega || ball.fireball {
                            BOSS_POWER_DAMAGE
                        } else {
                            1
                        };
                        boss.hp = boss.hp.saturating_sub(dmg);
                        boss.hit_flash = 1.0;
                        let bx = boss.x + boss.w * 0.5;
                        let by = boss.y + boss.h * 0.5;
                        let destroyed = boss.hp == 0;
                        let points = if destroyed {
                            self.config.gameplay.base_brick_score
                                * self.config.gameplay.boss_kill_score_mult
                        } else {
                            0
                        };
                        events.push(GameEvent::BrickHit {
                            x: ball.x,
                            y: ball.y,
                            color_idx: BOSS_HIT_COLOR_IDX,
                            destroyed,
                            points,
                            multiplier: NO_MULTIPLIER,
                        });
                        if destroyed {
                            self.boss = None;
                            self.score += points;
                            self.high_score = self.high_score.max(self.score);
                            pending_explosions.push((bx, by));
                        }
                    }
                }
            }

            // Brick collision
            let mut hit = None;
            for (idx, brick) in self.bricks.iter().enumerate() {
                // A portal on cooldown must not mask a neighbouring brick.
                if matches!(brick.kind, BrickKind::Portal) && ball.portal_cooldown > 0.0 {
                    continue;
                }
                if let Some(contact) = rect_contact(&ball, brick.x, brick.y, brick.w, brick.h) {
                    hit = Some((idx, contact));
                    break;
                }
            }

            if let Some((idx, contact)) = hit {
                if matches!(self.bricks[idx].kind, BrickKind::Portal) {
                    if ball.portal_cooldown <= 0.0 {
                        let other_portal = self
                            .bricks
                            .iter()
                            .enumerate()
                            .find(|(i, b)| *i != idx && matches!(b.kind, BrickKind::Portal))
                            .map(|(_, b)| b.center());
                        if let Some((tx, ty)) = other_portal {
                            let from = self.bricks[idx].center();
                            ball.x = tx;
                            ball.y = ty;
                            ball.portal_cooldown = self.config.gameplay.portal_cooldown_sec;
                            self.bricks[idx].hit_flash = 1.0;
                            events.push(GameEvent::PortalWarp { from, to: (tx, ty) });
                        }
                    }
                } else {
                    let is_indestructible =
                        matches!(self.bricks[idx].kind, BrickKind::Indestructible);
                    let should_pierce = (ball.fireball || ball.mega) && !is_indestructible;

                    // A contact counts only when the ball is reflected (or pierces): a ball that
                    // overlaps a moving brick without moving into it is just pushed out.
                    let counted = if should_pierce {
                        true
                    } else if bounce_off(&mut ball, &contact) {
                        if ball.vy.abs() < MIN_VERTICAL_RATIO {
                            ball.vy = MIN_VERTICAL_RATIO * ball.vy.signum();
                            let norm = (ball.vx * ball.vx + ball.vy * ball.vy).sqrt();
                            ball.vx /= norm;
                            ball.vy /= norm;
                        }
                        true
                    } else {
                        false
                    };

                    if counted {
                        if matches!(self.bricks[idx].kind, BrickKind::Bumper { .. }) {
                            // The boost is temporary and never pushes the total past the cap.
                            ball.boost = self
                                .config
                                .ball
                                .bumper_speed_boost
                                .min((max_speed - ball.speed).max(0.0));
                        } else {
                            ball.speed =
                                (ball.speed + self.config.ball.speed_step_on_hit).min(max_speed);
                        }

                        let center = self.bricks[idx].center();
                        let was_explosive = matches!(self.bricks[idx].kind, BrickKind::Explosive);
                        let destroyed = self.damage_brick(idx, ball.mega || ball.fireball, events);
                        // An explosive brick already detonates inside `damage_brick`.
                        if destroyed && ball.fireball && !was_explosive {
                            pending_explosions.push(center);
                        }
                        if self.lightning_active {
                            pending_lightning.push(center);
                        }
                    }
                }
            }

            self.balls[b_idx] = ball;
        }

        for (ex, ey) in pending_explosions {
            self.trigger_explosion(ex, ey, events);
        }
        for (lx, ly) in pending_lightning {
            self.trigger_chain_lightning(lx, ly, events);
        }
    }

    fn damage_brick(
        &mut self,
        idx: usize,
        instant_kill: bool,
        events: &mut Vec<GameEvent>,
    ) -> bool {
        let mut chain = Vec::new();
        let destroyed = self.damage_brick_inner(idx, instant_kill, events, &mut chain);
        self.resolve_explosions(chain, events);
        destroyed
    }

    /// Damages one brick. An explosive brick does not detonate here: its centre is pushed onto
    /// `chain` so that the caller resolves every explosion iteratively, without recursion.
    fn damage_brick_inner(
        &mut self,
        idx: usize,
        instant_kill: bool,
        events: &mut Vec<GameEvent>,
        chain: &mut Vec<(f32, f32)>,
    ) -> bool {
        if idx >= self.bricks.len() {
            return false;
        }
        let center = self.bricks[idx].center();
        match self.bricks[idx].kind {
            BrickKind::Indestructible | BrickKind::Portal => {
                self.bricks[idx].hit_flash = 1.0;
                events.push(GameEvent::BrickHit {
                    x: center.0,
                    y: center.1,
                    color_idx: SOLID_HIT_COLOR_IDX,
                    destroyed: false,
                    points: 0,
                    multiplier: NO_MULTIPLIER,
                });
                false
            }
            BrickKind::Explosive => {
                self.bricks.swap_remove(idx);
                self.register_brick_score(center.0, center.1, EXPLOSIVE_COLOR_IDX, events);
                chain.push(center);
                true
            }
            BrickKind::Ghost { revealed } => {
                if instant_kill || revealed {
                    self.bricks.swap_remove(idx);
                    self.register_brick_score(center.0, center.1, GHOST_HIT_COLOR_IDX, events);
                    true
                } else {
                    self.bricks[idx].kind = BrickKind::Ghost { revealed: true };
                    self.bricks[idx].hit_flash = 1.0;
                    events.push(GameEvent::BrickHit {
                        x: center.0,
                        y: center.1,
                        color_idx: GHOST_HIT_COLOR_IDX,
                        destroyed: false,
                        points: 0,
                        multiplier: NO_MULTIPLIER,
                    });
                    false
                }
            }
            BrickKind::Regenerating { mut hp } => {
                if instant_kill || hp <= 1 {
                    self.bricks.swap_remove(idx);
                    self.register_brick_score(center.0, center.1, REGEN_BRICK_COLOR_IDX, events);
                    true
                } else {
                    hp -= 1;
                    self.bricks[idx].kind = BrickKind::Regenerating { hp };
                    self.bricks[idx].regen_timer = self.config.gameplay.regen_delay_sec;
                    self.bricks[idx].hit_flash = 1.0;
                    events.push(GameEvent::BrickHit {
                        x: center.0,
                        y: center.1,
                        color_idx: REGEN_BRICK_COLOR_IDX,
                        destroyed: false,
                        points: 0,
                        multiplier: NO_MULTIPLIER,
                    });
                    false
                }
            }
            BrickKind::Bumper { mut hp } => {
                if instant_kill || hp <= 1 {
                    self.bricks.swap_remove(idx);
                    self.register_brick_score(center.0, center.1, BUMPER_COLOR_IDX, events);
                    true
                } else {
                    hp -= 1;
                    self.bricks[idx].kind = BrickKind::Bumper { hp };
                    self.bricks[idx].hit_flash = 1.0;
                    events.push(GameEvent::BrickHit {
                        x: center.0,
                        y: center.1,
                        color_idx: BUMPER_COLOR_IDX,
                        destroyed: false,
                        points: 0,
                        multiplier: NO_MULTIPLIER,
                    });
                    false
                }
            }
            BrickKind::Moving { mut hp, color_idx } => {
                if instant_kill || hp <= 1 {
                    self.bricks.swap_remove(idx);
                    self.register_brick_score(center.0, center.1, color_idx, events);
                    true
                } else {
                    hp -= 1;
                    self.bricks[idx].kind = BrickKind::Moving { hp, color_idx };
                    self.bricks[idx].hit_flash = 1.0;
                    events.push(GameEvent::BrickHit {
                        x: center.0,
                        y: center.1,
                        color_idx,
                        destroyed: false,
                        points: 0,
                        multiplier: NO_MULTIPLIER,
                    });
                    false
                }
            }
            BrickKind::Standard {
                mut hp,
                max_hp,
                color_idx,
            } => {
                if instant_kill || hp <= 1 {
                    self.bricks.swap_remove(idx);
                    self.register_brick_score(center.0, center.1, color_idx, events);
                    true
                } else {
                    hp -= 1;
                    self.bricks[idx].kind = BrickKind::Standard {
                        hp,
                        max_hp,
                        color_idx,
                    };
                    self.bricks[idx].hit_flash = 1.0;
                    events.push(GameEvent::BrickHit {
                        x: center.0,
                        y: center.1,
                        color_idx,
                        destroyed: false,
                        points: 0,
                        multiplier: NO_MULTIPLIER,
                    });
                    false
                }
            }
        }
    }

    fn register_brick_score(
        &mut self,
        x: f32,
        y: f32,
        color_idx: usize,
        events: &mut Vec<GameEvent>,
    ) {
        let gp = &self.config.gameplay;
        self.combo += 1;
        self.combo_timer = gp.combo_timeout_sec;
        let multiplier = combo_multiplier(self.combo, gp.combo_step, gp.combo_max_mult);
        let points = gp.base_brick_score * u64::from(multiplier);
        self.score += points;
        self.high_score = self.high_score.max(self.score);

        events.push(GameEvent::BrickHit {
            x,
            y,
            color_idx,
            destroyed: true,
            points,
            multiplier,
        });

        let gp = &self.config.gameplay;
        let min_distance = gp.powerup_min_spawn_distance;
        let can_spawn = self.powerup_spawn_cooldown <= 0.0
            && self.capsules.len() < MAX_CAPSULES
            && self
                .capsules
                .iter()
                .all(|c| (c.x - x).hypot(c.y - y) > min_distance);

        if can_spawn && self.next_rand_f32() < self.config.gameplay.powerup_drop_chance {
            let kinds = PowerUpKind::all();
            let pick = ((self.next_rand_f32() * kinds.len() as f32) as usize) % kinds.len();
            let (min_cx, max_cx) = self.capsule_x_bounds();
            self.powerup_spawn_cooldown = self.config.gameplay.powerup_spawn_cooldown_sec;
            self.capsules.push(PowerUpCapsule {
                x: x.clamp(min_cx, max_cx),
                y,
                radius: self.config.gameplay.powerup_radius,
                kind: kinds[pick],
            });
        }
    }

    /// Horizontal range in which a capsule centre may lie.
    fn capsule_x_bounds(&self) -> (f32, f32) {
        let margin = self.config.gameplay.powerup_wall_margin;
        (
            self.config.arena.wall_side + margin,
            self.config.arena.width - self.config.arena.wall_side - margin,
        )
    }

    fn trigger_explosion(&mut self, x: f32, y: f32, events: &mut Vec<GameEvent>) {
        self.resolve_explosions(vec![(x, y)], events);
    }

    /// Resolves explosions iteratively. Each centre destroys every destructible brick in range;
    /// exploding bricks add their own centre to the pending list. Targets are damaged in
    /// descending index order so that `swap_remove` never invalidates an index still to visit.
    fn resolve_explosions(&mut self, mut chain: Vec<(f32, f32)>, events: &mut Vec<GameEvent>) {
        let radius = self.config.gameplay.explosion_radius;
        while let Some((x, y)) = chain.pop() {
            events.push(GameEvent::Explosion { x, y, radius });
            let targets: Vec<usize> = self
                .bricks
                .iter()
                .enumerate()
                .filter(|(_, b)| {
                    let (bx, by) = b.center();
                    (bx - x).powi(2) + (by - y).powi(2) <= radius * radius && b.is_destructible()
                })
                .map(|(i, _)| i)
                .collect();
            for idx in targets.into_iter().rev() {
                self.damage_brick_inner(idx, true, events, &mut chain);
            }
        }
    }

    fn trigger_chain_lightning(&mut self, x: f32, y: f32, events: &mut Vec<GameEvent>) {
        let targets = self.config.gameplay.lightning_targets;
        for _ in 0..targets {
            let candidate = self
                .bricks
                .iter()
                .enumerate()
                .filter(|(_, b)| b.is_destructible())
                .min_by(|(_, a), (_, b)| {
                    let (ax, ay) = a.center();
                    let (bx, by) = b.center();
                    let da = (ax - x).powi(2) + (ay - y).powi(2);
                    let db = (bx - x).powi(2) + (by - y).powi(2);
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, b)| (i, b.center()));

            if let Some((idx, target_pos)) = candidate {
                events.push(GameEvent::LightningBolt {
                    from: (x, y),
                    to: target_pos,
                });
                self.damage_brick(idx, true, events);
            } else {
                break;
            }
        }
    }

    fn step_lasers(&mut self, dt: f32, events: &mut Vec<GameEvent>) {
        let speed = self.config.paddle.laser_speed;
        let wall_top = self.config.arena.wall_top;
        let bolts = std::mem::take(&mut self.lasers);
        let mut keep = Vec::with_capacity(bolts.len());

        for mut bolt in bolts {
            bolt.y -= speed * dt;
            if bolt.y < wall_top {
                continue;
            }
            let mut hit = None;
            for (idx, brick) in self.bricks.iter().enumerate() {
                if bolt.x + LASER_WIDTH >= brick.x
                    && bolt.x - LASER_WIDTH <= brick.x + brick.w
                    && bolt.y - LASER_HEIGHT <= brick.y + brick.h
                    && bolt.y >= brick.y
                {
                    hit = Some(idx);
                    break;
                }
            }
            if let Some(idx) = hit {
                self.damage_brick(idx, false, events);
            } else {
                keep.push(bolt);
            }
        }
        self.lasers = keep;
    }

    fn step_capsules(&mut self, dt: f32, events: &mut Vec<GameEvent>) {
        let fall_speed = self.config.gameplay.powerup_fall_speed;
        let paddle_y = self.config.arena.paddle_y;
        let paddle_h = self.config.paddle.height;
        let paddle_half_w = self.paddle_width * 0.5;
        let arena_h = self.config.arena.height;
        let vortex = self.vortex_active;
        let paddle_x = self.paddle_x;
        let (min_cx, max_cx) = self.capsule_x_bounds();

        // Horizontal anti-clustering separation; only valid for exactly MAX_CAPSULES == 2.
        if self.capsules.len() == MAX_CAPSULES {
            let dx = self.capsules[1].x - self.capsules[0].x;
            let dy = (self.capsules[1].y - self.capsules[0].y).abs();
            if dx.abs() < CAPSULE_SEPARATION_DX && dy < CAPSULE_SEPARATION_DY {
                let dir = if dx >= 0.0 { 1.0 } else { -1.0 };
                let push = CAPSULE_SEPARATION_SPEED * dt * dir;
                self.capsules[0].x = (self.capsules[0].x - push).clamp(min_cx, max_cx);
                self.capsules[1].x = (self.capsules[1].x + push).clamp(min_cx, max_cx);
            }
        }

        let mut collected = Vec::new();
        self.capsules.retain_mut(|cap| {
            cap.y += fall_speed * dt;
            if vortex {
                let pull = (paddle_x - cap.x)
                    .clamp(-CAPSULE_VORTEX_MAX_OFFSET, CAPSULE_VORTEX_MAX_OFFSET)
                    * CAPSULE_VORTEX_GAIN
                    * dt;
                cap.x = (cap.x + pull).clamp(min_cx, max_cx);
            }
            let caught = (cap.x - paddle_x).abs()
                <= paddle_half_w + cap.radius * CAPSULE_CATCH_X_FACTOR
                && (cap.y - paddle_y).abs() <= paddle_h * 0.5 + cap.radius * CAPSULE_CATCH_Y_FACTOR;
            if caught {
                collected.push((cap.kind, cap.x, cap.y));
                false
            } else {
                cap.y - cap.radius <= arena_h
            }
        });

        for (kind, x, y) in collected {
            self.apply_powerup(kind);
            events.push(GameEvent::PowerUpCollected { kind, x, y });
        }
    }

    /// Narrows the paddle by one step. No score, no toast: shared by the power-up and boss hazards.
    fn shrink_paddle(&mut self) {
        self.paddle_width =
            (self.paddle_width - self.config.paddle.width_step).max(self.config.paddle.min_width);
    }

    pub fn apply_powerup(&mut self, kind: PowerUpKind) {
        self.last_powerup = Some(kind);
        self.last_powerup_timer = self.config.gameplay.powerup_toast_sec;
        self.score +=
            self.config.gameplay.base_brick_score * self.config.gameplay.powerup_score_mult;
        self.high_score = self.high_score.max(self.score);

        match kind {
            PowerUpKind::Fireball => {
                for b in &mut self.balls {
                    b.fireball = true;
                }
            }
            PowerUpKind::MegaBall => {
                for b in &mut self.balls {
                    b.mega = true;
                    b.radius = self.config.ball.mega_radius;
                }
            }
            PowerUpKind::MultiBall => {
                if let Some(first) = self.balls.first().cloned() {
                    let spread = self.config.gameplay.multiball_spread_rad;
                    for angle_offset in [-spread, spread] {
                        let mut clone = first.clone();
                        clone.stuck_offset = None;
                        let base_angle = clone.vx.atan2(-clone.vy) + angle_offset;
                        clone.vx = base_angle.sin();
                        clone.vy = -base_angle.cos();
                        self.balls.push(clone);
                    }
                }
            }
            PowerUpKind::Expand => {
                self.paddle_width = (self.paddle_width + self.config.paddle.width_step)
                    .min(self.config.paddle.max_width);
            }
            PowerUpKind::Shrink => self.shrink_paddle(),
            PowerUpKind::Magnet => {
                self.magnet_active = true;
            }
            PowerUpKind::Laser => {
                self.laser_active = true;
            }
            PowerUpKind::Lightning => {
                self.lightning_active = true;
            }
            PowerUpKind::Slow => {
                for b in &mut self.balls {
                    b.speed = (b.speed * self.config.gameplay.slow_factor)
                        .max(self.config.ball.min_speed);
                }
            }
            PowerUpKind::Fast => {
                for b in &mut self.balls {
                    b.speed = (b.speed * self.config.gameplay.fast_factor)
                        .min(self.config.ball.max_speed);
                }
            }
            PowerUpKind::ExtraLife => {
                self.lives += 1;
            }
            PowerUpKind::Shield => {
                self.shield_hits = self.config.gameplay.shield_hits;
            }
        }
    }
}

#[cfg(test)]
mod tests;
