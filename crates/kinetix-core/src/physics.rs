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
}

impl Brick {
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

    pub fn symbol(self) -> &'static str {
        match self {
            Self::Fireball => "F",
            Self::MegaBall => "M",
            Self::MultiBall => "x3",
            Self::Expand => "<>",
            Self::Shrink => "><",
            Self::Magnet => "U",
            Self::Laser => "||",
            Self::Lightning => "Z",
            Self::Slow => "-",
            Self::Fast => "+",
            Self::ExtraLife => "+1",
            Self::Shield => "==",
        }
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

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Fireball => "FIRE",
            Self::MegaBall => "MEGA",
            Self::MultiBall => "x3",
            Self::Expand => "WIDE",
            Self::Shrink => "MINI",
            Self::Magnet => "GRIP",
            Self::Laser => "GUN",
            Self::Lightning => "BOLT",
            Self::Slow => "SLOW",
            Self::Fast => "FAST",
            Self::ExtraLife => "+1UP",
            Self::Shield => "WALL",
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
    pub speed: f32,
    pub radius: f32,
    pub stuck_offset: Option<f32>,
    pub fireball: bool,
    pub mega: bool,
    pub portal_cooldown: f32,
    pub boss_hit_cooldown: f32,
    pub trail: Vec<(f32, f32)>,
}

#[derive(Debug, Clone)]
pub enum GameEvent {
    BrickHit {
        x: f32,
        y: f32,
        color_idx: usize,
        destroyed: bool,
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
    pub lives: u32,
    pub combo: u32,
    pub combo_timer: f32,
    pub current_level_idx: usize,
    pub last_powerup_key: Option<&'static str>,
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
            lives: config.gameplay.initial_lives,
            combo: 0,
            combo_timer: 0.0,
            current_level_idx: 0,
            last_powerup_key: None,
            last_powerup_timer: 0.0,
            powerup_spawn_cooldown: 0.0,
            status: SessionStatus::Playing,
            config,
            rng_state: 0x853C_49E6_748F_EA9B,
        };
        world.load_level(0, levels);
        world
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
        self.max_unlocked_level = self.max_unlocked_level.max(self.current_level_idx);
        let level = levels[self.current_level_idx].clone();
        self.load_from_rows(&level.rows, level.boss, level.world);
    }

    pub fn load_from_rows(&mut self, rows: &[String], has_boss: bool, world_tier: usize) {
        self.bricks.clear();
        self.capsules.clear();
        self.lasers.clear();
        self.hazards.clear();
        self.level_elapsed = 0.0;
        self.sudden_death_timer = 0.0;
        self.powerup_spawn_cooldown = 0.0;
        self.status = SessionStatus::Playing;

        if has_boss {
            let tier_mult = world_tier.max(1) as u32;
            let hp = self.config.gameplay.boss_base_hp + (tier_mult - 1) * 8;
            self.boss = Some(BossEntity {
                x: (self.config.arena.width - BOSS_WIDTH) * 0.5,
                y: BOSS_Y,
                w: BOSS_WIDTH,
                h: BOSS_HEIGHT,
                vx: self.config.gameplay.boss_speed * (1.0 + (tier_mult as f32 - 1.0) * 0.2),
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
                    'M' => (
                        BrickKind::Moving {
                            hp: 2,
                            color_idx: 3,
                        },
                        if r_idx % 2 == 0 {
                            MOVING_BRICK_SPEED
                        } else {
                            -MOVING_BRICK_SPEED
                        },
                    ),
                    d if d.is_ascii_digit() => {
                        let digit = d.to_digit(10).unwrap_or(1) as usize;
                        let hp = if digit >= 5 { 2 } else { 1 };
                        (
                            BrickKind::Standard {
                                hp,
                                max_hp: hp,
                                color_idx: (digit.saturating_sub(1)) % 6,
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
                self.bricks.push(Brick {
                    x,
                    y,
                    w: brick_w,
                    h: BRICK_HEIGHT,
                    vx,
                    kind,
                    hit_flash: 0.0,
                });
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
            vx: 0.35,
            vy: -0.93,
            speed: self.config.ball.default_speed,
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
            self.bricks.push(Brick {
                x: target_x,
                y: target_y,
                w: brick_w,
                h: BRICK_HEIGHT,
                vx,
                kind: k,
                hit_flash: 0.5,
            });
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
        self.vortex_active = active && self.vortex_energy > 5.0;
    }

    pub fn launch_or_fire(&mut self, events: &mut Vec<GameEvent>) {
        if self.status != SessionStatus::Playing {
            return;
        }
        let mut released_any = false;
        for ball in &mut self.balls {
            if let Some(offset) = ball.stuck_offset.take() {
                let ratio = (offset / (self.paddle_width * 0.5)).clamp(-0.85, 0.85);
                let angle = ratio * self.config.paddle.max_bounce_angle_deg.to_radians();
                ball.vx = angle.sin();
                ball.vy = -angle.cos();
                released_any = true;
            }
        }
        if !released_any && self.laser_active && self.laser_cooldown <= 0.0 {
            self.laser_cooldown = self.config.paddle.laser_cooldown_sec;
            let left_x = self.paddle_x - self.paddle_width * 0.36;
            let right_x = self.paddle_x + self.paddle_width * 0.36;
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
                if self.sudden_death_timer >= 2.2 {
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
            if self.vortex_energy <= 0.5 {
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

        let moving_margin = self.config.ball.mega_radius * 2.0 + MOVING_BRICK_WALL_EXTRA_MARGIN;
        let min_bx = self.config.arena.wall_side + moving_margin;
        let max_bx = self.config.arena.width - self.config.arena.wall_side - moving_margin;
        for brick in &mut self.bricks {
            if brick.hit_flash > 0.0 {
                brick.hit_flash = (brick.hit_flash - dt * 5.0).max(0.0);
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
            .retain(|b| b.y - b.radius <= self.config.arena.height + 20.0);
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
            self.max_unlocked_level = self.max_unlocked_level.max(self.current_level_idx + 1);
        }
    }

    fn step_boss_and_hazards(&mut self, dt: f32, events: &mut Vec<GameEvent>) {
        let wall_margin = self.config.ball.mega_radius * 2.0 + MOVING_BRICK_WALL_EXTRA_MARGIN;
        let wall_left = self.config.arena.wall_side + wall_margin;
        let wall_right = self.config.arena.width - self.config.arena.wall_side - wall_margin;

        if let Some(boss) = &mut self.boss {
            if boss.hit_flash > 0.0 {
                boss.hit_flash = (boss.hit_flash - dt * 4.5).max(0.0);
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
                let left_turret = boss.x + boss.w * 0.25;
                let right_turret = boss.x + boss.w * 0.75;
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
                let dx_to_paddle = (self.paddle_x - ball.x).clamp(-360.0, 360.0) / 360.0;
                ball.vx += dx_to_paddle * self.config.gameplay.vortex_pull_strength * dt;
                let norm = (ball.vx * ball.vx + ball.vy * ball.vy).sqrt().max(0.001);
                ball.vx /= norm;
                ball.vy /= norm;
            }

            ball.x += ball.vx * ball.speed * dt;
            ball.y += ball.vy * ball.speed * dt;

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
                let rel = ((ball.x - self.paddle_x) / paddle_half_w).clamp(-0.95, 0.95);
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
                        let dmg = if ball.mega || ball.fireball { 2 } else { 1 };
                        boss.hp = boss.hp.saturating_sub(dmg);
                        boss.hit_flash = 1.0;
                        let bx = boss.x + boss.w * 0.5;
                        let by = boss.y + boss.h * 0.5;
                        let destroyed = boss.hp == 0;
                        events.push(GameEvent::BrickHit {
                            x: ball.x,
                            y: ball.y,
                            color_idx: 1,
                            destroyed,
                        });
                        if destroyed {
                            self.boss = None;
                            self.score += self.config.gameplay.base_brick_score * 50;
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
                            ball.portal_cooldown = 0.45;
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
                        ball.speed = (ball.speed + self.config.ball.speed_step_on_hit)
                            .min(self.config.ball.max_speed);

                        let center = self.bricks[idx].center();
                        let destroyed = self.damage_brick(idx, ball.mega || ball.fireball, events);
                        if destroyed && ball.fireball {
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
                    color_idx: 0,
                    destroyed: false,
                });
                false
            }
            BrickKind::Explosive => {
                self.bricks.swap_remove(idx);
                self.register_brick_score(center.0, center.1, 5, events);
                self.trigger_explosion(center.0, center.1, events);
                true
            }
            BrickKind::Ghost { revealed } => {
                if instant_kill || revealed {
                    self.bricks.swap_remove(idx);
                    self.register_brick_score(center.0, center.1, 2, events);
                    true
                } else {
                    self.bricks[idx].kind = BrickKind::Ghost { revealed: true };
                    self.bricks[idx].hit_flash = 1.0;
                    events.push(GameEvent::BrickHit {
                        x: center.0,
                        y: center.1,
                        color_idx: 2,
                        destroyed: false,
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
        self.combo += 1;
        self.combo_timer = self.config.gameplay.combo_timeout_sec;
        let mult = (1 + self.combo / 4).min(8) as u64;
        self.score += self.config.gameplay.base_brick_score * mult;
        self.high_score = self.high_score.max(self.score);

        events.push(GameEvent::BrickHit {
            x,
            y,
            color_idx,
            destroyed: true,
        });

        let can_spawn = self.powerup_spawn_cooldown <= 0.0
            && self.capsules.len() < 2
            && self
                .capsules
                .iter()
                .all(|c| (c.x - x).hypot(c.y - y) > 280.0);

        if can_spawn && self.next_rand_f32() < self.config.gameplay.powerup_drop_chance {
            let kinds = PowerUpKind::all();
            let pick = ((self.next_rand_f32() * kinds.len() as f32) as usize) % kinds.len();
            let min_cx = self.config.arena.wall_side + 76.0;
            let max_cx = self.config.arena.width - self.config.arena.wall_side - 76.0;
            self.powerup_spawn_cooldown = 1.65;
            self.capsules.push(PowerUpCapsule {
                x: x.clamp(min_cx, max_cx),
                y,
                radius: self.config.gameplay.powerup_radius,
                kind: kinds[pick],
            });
        }
    }

    fn trigger_explosion(&mut self, x: f32, y: f32, events: &mut Vec<GameEvent>) {
        let radius = self.config.gameplay.explosion_radius;
        events.push(GameEvent::Explosion { x, y, radius });

        let mut idx = 0;
        while idx < self.bricks.len() {
            let (bx, by) = self.bricks[idx].center();
            let dist_sq = (bx - x).powi(2) + (by - y).powi(2);
            if dist_sq <= radius * radius && self.bricks[idx].is_destructible() {
                self.damage_brick(idx, true, events);
            } else {
                idx += 1;
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
        let min_cx = self.config.arena.wall_side + 76.0;
        let max_cx = self.config.arena.width - self.config.arena.wall_side - 76.0;

        // Horizontal anti-clustering separation if 2 capsules are falling simultaneously
        if self.capsules.len() == 2 {
            let dx = self.capsules[1].x - self.capsules[0].x;
            let dy = (self.capsules[1].y - self.capsules[0].y).abs();
            if dx.abs() < 210.0 && dy < 160.0 {
                let dir = if dx >= 0.0 { 1.0 } else { -1.0 };
                let push = 140.0 * dt * dir;
                self.capsules[0].x = (self.capsules[0].x - push).clamp(min_cx, max_cx);
                self.capsules[1].x = (self.capsules[1].x + push).clamp(min_cx, max_cx);
            }
        }

        let mut collected = Vec::new();
        self.capsules.retain_mut(|cap| {
            cap.y += fall_speed * dt;
            if vortex {
                let pull = (paddle_x - cap.x).clamp(-220.0, 220.0) * 1.8 * dt;
                cap.x = (cap.x + pull).clamp(min_cx, max_cx);
            }
            let caught = (cap.x - paddle_x).abs() <= paddle_half_w + cap.radius * 0.85
                && (cap.y - paddle_y).abs() <= paddle_h * 0.5 + cap.radius * 0.72;
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
        self.last_powerup_key = Some(kind.i18n_key());
        self.last_powerup_timer = 3.0;
        self.score += self.config.gameplay.base_brick_score * 2;
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
                    for angle_offset in [-0.38f32, 0.38f32] {
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
                    b.speed = (b.speed * 0.78).max(self.config.ball.min_speed);
                }
            }
            PowerUpKind::Fast => {
                for b in &mut self.balls {
                    b.speed = (b.speed * 1.22).min(self.config.ball.max_speed);
                }
            }
            PowerUpKind::ExtraLife => {
                self.lives += 1;
            }
            PowerUpKind::Shield => {
                self.shield_hits = 3;
            }
        }
    }
}

#[cfg(test)]
mod tests;
